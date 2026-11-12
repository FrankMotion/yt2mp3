//! YT2MP3 – a plain black Windows window: YouTube link in, MP3 out.
#![windows_subsystem = "windows"]

mod dark;
mod job;
mod settings;
mod tools;

use native_windows_derive::NwgUi;
use native_windows_gui as nwg;
use nwg::NativeUi;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

/// State the worker thread hands to the UI thread (read on every notice).
#[derive(Default)]
struct Shared {
    text: String,
    percent: u32,
    busy: bool,
    ready: bool,
    pid: Option<u32>,
    cancelled: bool,
    done_file: Option<String>,
}

#[derive(Default, NwgUi)]
pub struct App {
    #[nwg_resource]
    embed: nwg::EmbedResource,

    #[nwg_resource(source_embed: Some(&data.embed), source_embed_id: 1)]
    icon: nwg::Icon,

    // starts hidden; init() paints it black, then shows it (no white flash)
    #[nwg_control(size: (440, 96), center: true, title: "YT2MP3", icon: Some(&data.icon), flags: "WINDOW|MINIMIZE_BOX")]
    #[nwg_events(OnWindowClose: [App::exit], OnInit: [App::init])]
    window: nwg::Window,

    #[nwg_control(parent: window, position: (12, 12), size: (300, 26), placeholder_text: Some("YouTube-Link"))]
    #[nwg_events(OnKeyEnter: [App::go])]
    url: nwg::TextInput,

    #[nwg_control(parent: window, text: "Download", position: (320, 11), size: (108, 28))]
    #[nwg_events(OnButtonClick: [App::go])]
    go_btn: nwg::Button,

    #[nwg_control(parent: window, position: (12, 48), size: (416, 4), range: 0..100)]
    progress: nwg::ProgressBar,

    #[nwg_control(parent: window, text: "", position: (12, 62), size: (330, 22))]
    status: nwg::Label,

    #[nwg_control(parent: window, text: "Ordner", position: (350, 59), size: (78, 26))]
    #[nwg_events(OnButtonClick: [App::folder_menu_show])]
    folder_btn: nwg::Button,

    #[nwg_control(parent: window, popup: true)]
    folder_menu: nwg::Menu,

    #[nwg_control(parent: folder_menu, text: "Öffnen")]
    #[nwg_events(OnMenuItemSelected: [App::open_folder])]
    menu_open: nwg::MenuItem,

    #[nwg_control(parent: folder_menu, text: "Ändern …")]
    #[nwg_events(OnMenuItemSelected: [App::change_folder])]
    menu_change: nwg::MenuItem,

    #[nwg_resource(title: "Ordner für MP3s", action: nwg::FileDialogAction::OpenDirectory)]
    dialog: nwg::FileDialog,

    #[nwg_control(parent: window)]
    #[nwg_events(OnNotice: [App::on_notice])]
    notice: nwg::Notice,

    shared: Arc<Mutex<Shared>>,
    settings: RefCell<Option<settings::Settings>>,
    last_file: RefCell<Option<String>>,
    dark_hook: RefCell<Option<nwg::RawEventHandler>>,
}

fn hwnd(h: &nwg::ControlHandle) -> *mut std::ffi::c_void {
    h.hwnd().map(|p| p as *mut std::ffi::c_void).unwrap_or(std::ptr::null_mut())
}

impl App {
    fn folder(&self) -> String {
        self.settings.borrow().as_ref().map(|s| s.folder.clone()).unwrap_or_default()
    }

    fn idle_text(&self) -> String {
        format!("→ {}", self.folder())
    }

    /// Black title bar, dark field and buttons, white-on-dark progress bar.
    fn paint_black(&self) {
        let edit = hwnd(&self.url.handle);
        dark::title_bar(hwnd(&self.window.handle));
        dark::theme(hwnd(&self.go_btn.handle), "DarkMode_Explorer");
        dark::theme(hwnd(&self.folder_btn.handle), "DarkMode_Explorer");
        dark::theme(edit, "DarkMode_CFD");
        dark::progress(hwnd(&self.progress.handle));
        let paint = dark::Paint::new();
        let edit_addr = edit as usize;
        let hook = nwg::bind_raw_event_handler(&self.window.handle, 0x1_0001, move |h, msg, w, l| {
            paint.handle(h as *mut std::ffi::c_void, msg, w, l, edit_addr as *mut std::ffi::c_void)
        });
        *self.dark_hook.borrow_mut() = hook.ok();
    }

    fn init(&self) {
        self.paint_black();
        self.window.set_visible(true);
        let s = settings::load();
        let due = settings::now().saturating_sub(s.last_update) > 3 * 24 * 3600;
        *self.settings.borrow_mut() = Some(s);
        self.url.set_focus();

        let shared = self.shared.clone();
        let sender = self.notice.sender();
        if tools::ready() {
            {
                let mut sh = shared.lock().unwrap();
                sh.ready = true;
                sh.text = self.idle_text();
            }
            self.on_notice();
            if due {
                // keep yt-dlp current, quietly, in the background
                std::thread::spawn(move || {
                    if tools::update_ytdlp().is_ok() {
                        let mut s = settings::load();
                        s.last_update = settings::now();
                        settings::save(&s);
                    }
                });
            }
            return;
        }
        let idle = self.idle_text();
        std::thread::spawn(move || {
            let report = |text: String, pct: u32| {
                let mut sh = shared.lock().unwrap();
                sh.text = text;
                sh.percent = pct;
                drop(sh);
                sender.notice();
            };
            let result = tools::setup(&report);
            let mut sh = shared.lock().unwrap();
            sh.percent = 0;
            match result {
                Ok(()) => {
                    sh.ready = true;
                    sh.text = idle;
                    let mut s = settings::load();
                    s.last_update = settings::now();
                    settings::save(&s);
                }
                Err(e) => sh.text = format!("{e} – App neu starten"),
            }
            drop(sh);
            sender.notice();
        });
        self.on_notice();
    }

    fn go(&self) {
        let (busy, ready, pid) = {
            let sh = self.shared.lock().unwrap();
            (sh.busy, sh.ready, sh.pid)
        };
        if busy {
            if let Some(pid) = pid {
                self.shared.lock().unwrap().cancelled = true;
                job::cancel(pid);
            }
            return;
        }
        if !ready {
            return;
        }
        let url = match job::check_url(&self.url.text()) {
            Ok(u) => u,
            Err(e) => {
                self.status.set_text(&e);
                self.url.set_focus();
                return;
            }
        };
        {
            let mut sh = self.shared.lock().unwrap();
            sh.busy = true;
            sh.cancelled = false;
            sh.done_file = None;
            sh.percent = 0;
            sh.text = "Startet …".into();
        }
        self.on_notice();

        let folder = self.folder();
        let shared = self.shared.clone();
        let sender = self.notice.sender();
        std::thread::spawn(move || {
            let set_pid = |p: Option<u32>| shared.lock().unwrap().pid = p;
            let send = |u: job::Update| {
                let mut sh = shared.lock().unwrap();
                match u {
                    job::Update::Status(t, p) => {
                        sh.text = t;
                        sh.percent = p;
                    }
                    job::Update::Done(file) => {
                        let name = std::path::Path::new(&file)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        sh.text = format!("✓ {name}");
                        sh.percent = 100;
                        sh.done_file = Some(file);
                        sh.busy = false;
                    }
                    job::Update::Failed(e) => {
                        sh.text = if sh.cancelled { "Abgebrochen".into() } else { e };
                        sh.percent = 0;
                        sh.busy = false;
                    }
                }
                drop(sh);
                sender.notice();
            };
            job::run(&url, &folder, &set_pid, &send);
        });
    }

    /// Worker thread poked us: copy its state into the controls.
    fn on_notice(&self) {
        let sh = self.shared.lock().unwrap();
        self.status.set_text(&sh.text);
        self.progress.set_pos(sh.percent);
        self.go_btn.set_text(if sh.busy { "Abbrechen" } else { "Download" });
        self.go_btn.set_enabled(sh.ready);
        self.url.set_readonly(sh.busy);
        self.folder_btn.set_enabled(!sh.busy);
        if let Some(f) = &sh.done_file {
            if self.last_file.borrow().as_deref() != Some(f.as_str()) {
                *self.last_file.borrow_mut() = Some(f.clone());
                self.url.set_text("");
                self.url.set_focus();
            }
        }
    }

    fn folder_menu_show(&self) {
        let (x, y) = nwg::GlobalCursor::position();
        self.folder_menu.popup(x, y);
    }

    fn open_folder(&self) {
        let mut c = tools::hidden(tools::windows_exe("explorer.exe"));
        match self.last_file.borrow().as_deref() {
            Some(f) if std::path::Path::new(f).exists() => {
                use std::os::windows::process::CommandExt;
                c.raw_arg(format!("/select,\"{f}\""));
            }
            _ => {
                c.arg(self.folder());
            }
        }
        let _ = c.spawn();
    }

    fn change_folder(&self) {
        let _ = self.dialog.set_default_folder(&self.folder());
        if self.dialog.run(Some(&self.window)) {
            if let Ok(dir) = self.dialog.get_selected_item() {
                let dir = dir.to_string_lossy().into_owned();
                if let Some(s) = self.settings.borrow_mut().as_mut() {
                    s.folder = dir;
                    settings::save(s);
                }
                let busy = self.shared.lock().unwrap().busy;
                if !busy {
                    self.shared.lock().unwrap().text = self.idle_text();
                    self.on_notice();
                }
            }
        }
    }

    fn exit(&self) {
        if let Some(pid) = self.shared.lock().unwrap().pid {
            job::cancel(pid);
        }
        nwg::stop_thread_dispatch();
    }
}

fn main() {
    dark::dark_menus();
    nwg::init().expect("Windows-Oberfläche konnte nicht starten");
    let mut font = nwg::Font::default();
    if nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).is_ok() {
        let _ = nwg::Font::set_global_default(Some(font));
    }
    let _app = App::build_ui(Default::default()).expect("Fenster konnte nicht erstellt werden");
    nwg::dispatch_thread_events();
}
