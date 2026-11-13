//! Corvus – a plain black Windows window: YouTube link in, music or video out.
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
    #[nwg_control(size: (600, 96), center: true, title: "Corvus", icon: Some(&data.icon), flags: "WINDOW|MINIMIZE_BOX|MAXIMIZE_BOX|RESIZABLE")]
    #[nwg_events(OnWindowClose: [App::exit], OnInit: [App::init], OnResize: [App::layout], OnWindowMaximize: [App::layout])]
    window: nwg::Window,

    #[nwg_control(parent: window, position: (12, 12), size: (300, 26), placeholder_text: Some("YouTube-Link"))]
    #[nwg_events(OnKeyEnter: [App::go])]
    url: nwg::TextInput,

    #[nwg_control(parent: window, size: (116, 26), collection: job::FORMATS.iter().map(|f| f.0).collect(), selected_index: Some(0))]
    #[nwg_events(OnComboxBoxSelection: [App::format_changed])]
    format: nwg::ComboBox<&'static str>,

    #[nwg_control(parent: window, size: (78, 26), collection: job::QUALITIES.iter().map(|q| q.0).collect(), selected_index: Some(3))]
    #[nwg_events(OnComboxBoxSelection: [App::quality_changed])]
    quality: nwg::ComboBox<&'static str>,

    #[nwg_control(parent: window, text: "Download", size: (100, 28))]
    #[nwg_events(OnButtonClick: [App::go])]
    go_btn: nwg::Button,

    #[nwg_control(parent: window, size: (416, 4), range: 0..100)]
    progress: nwg::ProgressBar,

    #[nwg_control(parent: window, text: "", size: (330, 22))]
    status: nwg::Label,

    #[nwg_control(parent: window, text: "Ordner", size: (78, 26))]
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

    #[nwg_resource(title: "Ordner wählen", action: nwg::FileDialogAction::OpenDirectory)]
    dialog: nwg::FileDialog,

    #[nwg_control(parent: window)]
    #[nwg_events(OnNotice: [App::on_notice])]
    notice: nwg::Notice,

    shared: Arc<Mutex<Shared>>,
    settings: RefCell<Option<settings::Settings>>,
    last_file: RefCell<Option<String>>,
    dark_hook: RefCell<Option<nwg::RawEventHandler>>,
    frame_hooks: RefCell<Vec<nwg::RawEventHandler>>,
}

fn hwnd(h: &nwg::ControlHandle) -> *mut std::ffi::c_void {
    h.hwnd().map(|p| p as *mut std::ffi::c_void).unwrap_or(std::ptr::null_mut())
}

impl App {
    fn setting<T>(&self, f: impl FnOnce(&settings::Settings) -> T) -> T {
        f(self.settings.borrow().as_ref().expect("settings loaded in init"))
    }

    /// Changes the remembered settings and writes them, keeping the
    /// update time a background thread may have written meanwhile.
    fn change(&self, f: impl FnOnce(&mut settings::Settings)) {
        if let Some(s) = self.settings.borrow_mut().as_mut() {
            f(s);
            s.last_update = s.last_update.max(settings::load().last_update);
            settings::save(s);
        }
    }

    fn format_id(&self) -> &'static str {
        let i = self.format.selection().unwrap_or(0);
        job::FORMATS.get(i).map(|f| f.1).unwrap_or("mp3")
    }

    fn quality_id(&self) -> &'static str {
        let i = self.quality.selection().unwrap_or(3);
        job::QUALITIES.get(i).map(|q| q.1).unwrap_or("1080")
    }

    fn video(&self) -> bool {
        job::is_video(self.format_id())
    }

    /// The folder for what is selected: music or video.
    fn folder(&self) -> String {
        let video = self.video();
        self.setting(|s| if video { s.video_folder.clone() } else { s.folder.clone() })
    }

    fn idle_text(&self) -> String {
        format!("→ {}", self.folder())
    }

    fn show_idle(&self) {
        let busy = self.shared.lock().unwrap().busy;
        if !busy {
            self.shared.lock().unwrap().text = self.idle_text();
            self.on_notice();
        }
    }

    /// Black title bar, dark field, lists and buttons, white-on-dark bar.
    fn paint_black(&self) {
        let edit = hwnd(&self.url.handle);
        dark::title_bar(hwnd(&self.window.handle));
        dark::theme(hwnd(&self.go_btn.handle), "DarkMode_Explorer");
        dark::theme(hwnd(&self.folder_btn.handle), "DarkMode_Explorer");
        dark::theme(hwnd(&self.format.handle), "DarkMode_CFD");
        dark::theme(hwnd(&self.quality.handle), "DarkMode_CFD");
        dark::theme(edit, "DarkMode_CFD");
        dark::progress(hwnd(&self.progress.handle));
        // smallest size: full height, a bit narrower than the start width
        let (w, h) = dark::window_pixels(hwnd(&self.window.handle));
        let paint = dark::Paint::new((w * 85 / 100, h));
        let edit_addr = edit as usize;
        let hook = nwg::bind_raw_event_handler(&self.window.handle, 0x1_0001, move |h, msg, w, l| {
            paint.handle(h as *mut std::ffi::c_void, msg, w, l, edit_addr as *mut std::ffi::c_void)
        });
        *self.dark_hook.borrow_mut() = hook.ok();

        // frames of the field, the status line and the bar: dark, never white
        let parts = [
            (&self.url.handle, dark::FIELD),
            (&self.status.handle, dark::BG),
            (&self.progress.handle, dark::BG),
        ];
        for (i, (handle, color)) in parts.into_iter().enumerate() {
            let brush = dark::brush(color);
            let hook = nwg::bind_raw_event_handler(handle, 0x1_0010 + i, move |h, msg, _w, _l| {
                dark::frame(h as *mut std::ffi::c_void, msg, brush)
            });
            if let Ok(hook) = hook {
                self.frame_hooks.borrow_mut().push(hook);
            }
        }
    }

    /// Fits the controls to the window: the link field, bar and status grow
    /// with the width, the lists and buttons stay on the right, the block
    /// stays vertically centred. The quality list only shows for video.
    fn layout(&self) {
        let win = hwnd(&self.window.handle);
        if win.is_null() {
            return;
        }
        let (w, h) = dark::client_logical(win);
        if w <= 0 || h <= 0 {
            return; // minimised
        }
        const M: i32 = 12; // margin
        const GAP: i32 = 8;
        const FMT_W: i32 = 116;
        const Q_W: i32 = 78;
        const GO_W: i32 = 100;
        const DIR_W: i32 = 78;
        const MAX_W: i32 = 960; // content never wider than this; centred beyond
        let cw = w.min(MAX_W);
        let left = (w - cw) / 2;
        let right = left + cw;
        let top = ((h - 96) / 2).max(0);
        let wide = |x: i32, min: i32| x.max(min) as u32;

        let video = self.video();
        self.quality.set_visible(video);
        // right to left: Download, [quality], format
        let go_x = right - M - GO_W;
        let q_x = go_x - GAP - Q_W;
        let fmt_x = if video { q_x - GAP - FMT_W } else { go_x - GAP - FMT_W };
        self.go_btn.set_position(go_x, top + 11);
        self.go_btn.set_size(GO_W as u32, 28);
        self.quality.set_position(q_x, top + 12);
        self.quality.set_size(Q_W as u32, 26);
        self.format.set_position(fmt_x, top + 12);
        self.format.set_size(FMT_W as u32, 26);
        self.url.set_position(left + M, top + 12);
        self.url.set_size(wide(fmt_x - GAP - (left + M), 60), 26);

        self.progress.set_position(left + M, top + 48);
        self.progress.set_size(wide(cw - 2 * M, 10), 4);
        self.status.set_position(left + M, top + 62);
        self.status.set_size(wide(cw - 2 * M - DIR_W - GAP, 40), 22);
        self.folder_btn.set_position(right - M - DIR_W, top + 59);
        self.folder_btn.set_size(DIR_W as u32, 26);
        dark::repaint(win);
    }

    fn init(&self) {
        let s = settings::load();
        let due = settings::now().saturating_sub(s.last_update) > 3 * 24 * 3600;
        if let Some(i) = job::FORMATS.iter().position(|f| f.1 == s.format) {
            self.format.set_selection(Some(i));
        }
        if let Some(i) = job::QUALITIES.iter().position(|q| q.1 == s.quality) {
            self.quality.set_selection(Some(i));
        }
        *self.settings.borrow_mut() = Some(s);

        self.paint_black();
        self.layout();
        self.window.set_visible(true);
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

    fn format_changed(&self) {
        let id = self.format_id();
        self.change(|s| s.format = id.into());
        self.layout();
        self.show_idle();
    }

    fn quality_changed(&self) {
        let id = self.quality_id();
        self.change(|s| s.quality = id.into());
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
        let format = self.format_id();
        let quality = self.quality_id();
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
            job::run(&url, &folder, format, quality, &set_pid, &send);
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
        self.format.set_enabled(!sh.busy);
        self.quality.set_enabled(!sh.busy);
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
        let folder = self.folder();
        let mut c = tools::hidden(tools::windows_exe("explorer.exe"));
        match self.last_file.borrow().as_deref() {
            // the last download, if it went into this folder
            Some(f) if std::path::Path::new(f).exists() && f.starts_with(&folder) => {
                use std::os::windows::process::CommandExt;
                c.raw_arg(format!("/select,\"{f}\""));
            }
            _ => {
                c.arg(&folder);
            }
        }
        let _ = c.spawn();
    }

    fn change_folder(&self) {
        let _ = self.dialog.set_default_folder(&self.folder());
        if self.dialog.run(Some(&self.window)) {
            if let Ok(dir) = self.dialog.get_selected_item() {
                let dir = dir.to_string_lossy().into_owned();
                let video = self.video();
                self.change(|s| {
                    if video {
                        s.video_folder = dir;
                    } else {
                        s.folder = dir;
                    }
                });
                self.show_idle();
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
    settings::migrate_old_name();
    dark::dark_menus();
    nwg::init().expect("Windows-Oberfläche konnte nicht starten");
    let mut font = nwg::Font::default();
    if nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).is_ok() {
        let _ = nwg::Font::set_global_default(Some(font));
    }
    let _app = App::build_ui(Default::default()).expect("Fenster konnte nicht erstellt werden");
    nwg::dispatch_thread_events();
}
