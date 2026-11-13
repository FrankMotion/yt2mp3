//! Corvus – a small dark Windows window: YouTube / TikTok / Instagram link in,
//! music or video out, with a list of what was downloaded.
#![windows_subsystem = "windows"]

mod certs;
mod dark;
mod history;
mod job;
mod settings;
mod tools;

use native_windows_derive::NwgUi;
use native_windows_gui as nwg;
use nwg::NativeUi;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// One waiting link, with the format and folder chosen when it was added.
struct QueueItem {
    url: String,
    folder: String,
    format: &'static str,
    quality: &'static str,
}

/// State shared between the UI thread and the download worker.
#[derive(Default)]
struct Shared {
    text: String,
    percent: u32,
    /// the worker is running
    busy: bool,
    ready: bool,
    pid: Option<u32>,
    cancelled: bool,
    /// links still waiting
    queue: VecDeque<QueueItem>,
    /// finished files the UI has not put into the history yet
    done_files: Vec<String>,
    /// a newer Corvus on GitHub ("1.8.0"), found by the start-up check
    update: Option<String>,
}

/// "…  ·  noch 3" while links are waiting.
fn with_queue(text: &str, waiting: usize) -> String {
    if waiting > 0 { format!("{text}  ·  noch {waiting}") } else { text.to_string() }
}

#[derive(Default, NwgUi)]
pub struct App {
    #[nwg_resource]
    embed: nwg::EmbedResource,

    #[nwg_resource(source_embed: Some(&data.embed), source_embed_id: 1)]
    icon: nwg::Icon,

    // starts hidden; init() paints it, then shows it (no white flash)
    #[nwg_control(size: (640, 380), center: true, title: "Corvus", icon: Some(&data.icon), flags: "WINDOW|MINIMIZE_BOX|MAXIMIZE_BOX|RESIZABLE")]
    #[nwg_events(OnWindowClose: [App::exit], OnInit: [App::init], OnResize: [App::layout], OnWindowMaximize: [App::layout])]
    window: nwg::Window,

    #[nwg_control(parent: window, size: (300, 28), placeholder_text: Some("Link von YouTube, TikTok oder Instagram"))]
    #[nwg_events(OnKeyEnter: [App::go], OnTextInput: [App::update_go_label])]
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

    #[nwg_control(parent: window, text: "Ordner", size: (78, 28))]
    #[nwg_events(OnButtonClick: [App::folder_menu_show])]
    folder_btn: nwg::Button,

    // only shown when GitHub has a newer version
    #[nwg_control(parent: window, text: "Update", size: (120, 28), flags: "NONE")]
    #[nwg_events(OnButtonClick: [App::open_update])]
    update_btn: nwg::Button,

    #[nwg_control(parent: window, text: "Verlauf", size: (200, 20))]
    hist_label: nwg::Label,

    #[nwg_control(parent: window, size: (400, 200), list_style: nwg::ListViewStyle::Detailed, flags: "VISIBLE|SINGLE_SELECTION|NO_HEADER|TAB_STOP", ex_flags: nwg::ListViewExFlags::FULL_ROW_SELECT)]
    #[nwg_events(OnListViewDoubleClick: [App::hist_play], OnListViewRightClick: [App::hist_menu_show])]
    history: nwg::ListView,

    #[nwg_control(parent: window, popup: true)]
    folder_menu: nwg::Menu,

    #[nwg_control(parent: folder_menu, text: "Öffnen")]
    #[nwg_events(OnMenuItemSelected: [App::open_folder])]
    menu_open: nwg::MenuItem,

    #[nwg_control(parent: folder_menu, text: "Ändern …")]
    #[nwg_events(OnMenuItemSelected: [App::change_folder])]
    menu_change: nwg::MenuItem,

    #[nwg_control(parent: window, popup: true)]
    hist_menu: nwg::Menu,

    #[nwg_control(parent: hist_menu, text: "Abspielen")]
    #[nwg_events(OnMenuItemSelected: [App::hist_play])]
    hist_open: nwg::MenuItem,

    #[nwg_control(parent: hist_menu, text: "Im Ordner zeigen")]
    #[nwg_events(OnMenuItemSelected: [App::hist_reveal])]
    hist_reveal_item: nwg::MenuItem,

    #[nwg_control(parent: hist_menu, text: "Aus Verlauf entfernen")]
    #[nwg_events(OnMenuItemSelected: [App::hist_remove])]
    hist_remove_item: nwg::MenuItem,

    #[nwg_control(parent: hist_menu)]
    hist_sep: nwg::MenuSeparator,

    #[nwg_control(parent: hist_menu, text: "Verlauf leeren")]
    #[nwg_events(OnMenuItemSelected: [App::hist_clear])]
    hist_clear_item: nwg::MenuItem,

    #[nwg_resource(title: "Ordner wählen", action: nwg::FileDialogAction::OpenDirectory)]
    dialog: nwg::FileDialog,

    #[nwg_control(parent: window)]
    #[nwg_events(OnNotice: [App::on_notice])]
    notice: nwg::Notice,

    shared: Arc<Mutex<Shared>>,
    settings: RefCell<Option<settings::Settings>>,
    last_file: RefCell<Option<String>>,
    entries: RefCell<Vec<history::Entry>>,
    dark_hook: RefCell<Option<nwg::RawEventHandler>>,
    frame_hooks: RefCell<Vec<nwg::RawEventHandler>>,
}

fn hwnd(h: &nwg::ControlHandle) -> *mut std::ffi::c_void {
    h.hwnd().map(|p| p as *mut std::ffi::c_void).unwrap_or(std::ptr::null_mut())
}

/// Opens a file with its default program, or shows it selected in Explorer.
fn open_in_explorer(path: &str, select: bool) {
    let mut c = tools::hidden(tools::windows_exe("explorer.exe"));
    if select {
        use std::os::windows::process::CommandExt;
        c.raw_arg(format!("/select,\"{path}\""));
    } else {
        c.arg(path);
    }
    let _ = c.spawn();
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

    /// Dark title bar, framed field, self-drawn buttons, dark list.
    fn paint(&self) {
        let edit = hwnd(&self.url.handle);
        dark::title_bar(hwnd(&self.window.handle));
        dark::owner_draw(hwnd(&self.go_btn.handle));
        dark::owner_draw(hwnd(&self.folder_btn.handle));
        dark::owner_draw(hwnd(&self.update_btn.handle));
        dark::theme(hwnd(&self.format.handle), "DarkMode_CFD");
        dark::theme(hwnd(&self.quality.handle), "DarkMode_CFD");
        dark::theme(edit, "DarkMode_CFD");
        dark::progress(hwnd(&self.progress.handle));
        dark::list(hwnd(&self.history.handle));
        // smallest size: a bit narrower and lower than the start size
        let (w, h) = dark::window_pixels(hwnd(&self.window.handle));
        let paint = dark::Paint::new((w * 85 / 100, h * 60 / 100), hwnd(&self.go_btn.handle));
        let edit_addr = edit as usize;
        let hook = nwg::bind_raw_event_handler(&self.window.handle, 0x1_0001, move |h, msg, w, l| {
            paint.handle(h as *mut std::ffi::c_void, msg, w, l, edit_addr as *mut std::ffi::c_void)
        });
        *self.dark_hook.borrow_mut() = hook.ok();

        // frames: the link field gets a thin outline, the rest just stays dark
        let border = dark::brush(dark::BORDER);
        let parts = [
            (&self.url.handle, dark::FIELD, border),
            (&self.status.handle, dark::BG, 0),
            (&self.hist_label.handle, dark::BG, 0),
            (&self.progress.handle, dark::BG, 0),
        ];
        // the history list paints itself on our dark colour, not white
        let list_fill = dark::brush(dark::FIELD);
        if let Ok(hook) = nwg::bind_raw_event_handler(&self.history.handle, 0x1_0020, move |h, msg, _w, _l| {
            dark::list_paint(h as *mut std::ffi::c_void, msg, list_fill)
        }) {
            self.frame_hooks.borrow_mut().push(hook);
        }
        for (i, (handle, color, edge)) in parts.into_iter().enumerate() {
            let fill = dark::brush(color);
            let hook = nwg::bind_raw_event_handler(handle, 0x1_0010 + i, move |h, msg, _w, _l| {
                dark::frame(h as *mut std::ffi::c_void, msg, fill, edge)
            });
            if let Ok(hook) = hook {
                self.frame_hooks.borrow_mut().push(hook);
            }
        }
    }

    /// Fits the controls to the window: link field, bar, status and history
    /// grow with the width, lists and buttons stay on the right, the history
    /// fills the height. The quality list only shows for video.
    fn layout(&self) {
        let win = hwnd(&self.window.handle);
        if win.is_null() {
            return;
        }
        let (w, h) = dark::client_logical(win);
        if w <= 0 || h <= 0 {
            return; // minimised
        }
        const M: i32 = 16; // margin
        const GAP: i32 = 8;
        const FMT_W: i32 = 116;
        const Q_W: i32 = 78;
        const GO_W: i32 = 104;
        const DIR_W: i32 = 84;
        const TIME_W: i32 = 64;
        const MAX_W: i32 = 960; // content never wider than this; centred beyond
        let cw = w.min(MAX_W);
        let left = (w - cw) / 2;
        let right = left + cw;
        let wide = |x: i32, min: i32| x.max(min) as u32;

        let video = self.video();
        self.quality.set_visible(video);
        // row 1, right to left: Download, [quality], format; the link takes the rest
        let go_x = right - M - GO_W;
        let q_x = go_x - GAP - Q_W;
        let fmt_x = if video { q_x - GAP - FMT_W } else { go_x - GAP - FMT_W };
        self.go_btn.set_position(go_x, M);
        self.go_btn.set_size(GO_W as u32, 28);
        self.quality.set_position(q_x, M + 1);
        self.quality.set_size(Q_W as u32, 26);
        self.format.set_position(fmt_x, M + 1);
        self.format.set_size(FMT_W as u32, 26);
        self.url.set_position(left + M, M);
        self.url.set_size(wide(fmt_x - GAP - (left + M), 60), 28);
        // row 2: thin progress bar
        self.progress.set_position(left + M, M + 40);
        self.progress.set_size(wide(cw - 2 * M, 10), 3);
        // row 3: status line, [update], folder button
        const UPD_W: i32 = 120;
        let update = self.update_btn.visible();
        let upd_x = right - M - DIR_W - GAP - UPD_W;
        let status_right = if update { upd_x - GAP } else { right - M - DIR_W - GAP };
        self.status.set_position(left + M, M + 54);
        self.status.set_size(wide(status_right - (left + M), 40), 22);
        self.update_btn.set_position(upd_x, M + 50);
        self.update_btn.set_size(UPD_W as u32, 28);
        self.folder_btn.set_position(right - M - DIR_W, M + 50);
        self.folder_btn.set_size(DIR_W as u32, 28);
        // history fills the rest
        let list_top = M + 108;
        self.hist_label.set_position(left + M, M + 88);
        self.hist_label.set_size(wide(cw - 2 * M, 40), 18);
        self.history.set_position(left + M, list_top);
        let list_w = wide(cw - 2 * M, 100);
        self.history.set_size(list_w, wide(h - list_top - M, 40));
        self.history.set_column_width(0, TIME_W as isize);
        self.history.set_column_width(1, (list_w as i32 - TIME_W - 22).max(60) as isize);
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

        self.history.insert_column(nwg::InsertListViewColumn {
            index: Some(0),
            fmt: None,
            width: Some(64),
            text: Some("Wann".into()),
        });
        self.history.insert_column(nwg::InsertListViewColumn {
            index: Some(1),
            fmt: None,
            width: Some(400),
            text: Some("Datei".into()),
        });
        *self.entries.borrow_mut() = history::load();
        self.refresh_history();

        self.paint();
        self.layout();
        self.window.set_visible(true);
        self.url.set_focus();

        // is there a newer Corvus on GitHub? (quietly, in the background)
        {
            let shared = self.shared.clone();
            let sender = self.notice.sender();
            std::thread::spawn(move || {
                if let Some(v) = tools::newer_release() {
                    shared.lock().unwrap().update = Some(v);
                    sender.notice();
                }
            });
        }

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

    /// Enter / Download: a link in the field joins the queue (and starts the
    /// worker if it is idle); an empty field while busy means "Abbrechen".
    fn go(&self) {
        let text = self.url.text();
        let (busy, ready) = {
            let sh = self.shared.lock().unwrap();
            (sh.busy, sh.ready)
        };
        if text.trim().is_empty() {
            if busy {
                self.cancel_all();
            }
            return;
        }
        if !ready {
            return;
        }
        let url = match job::check_url(&text) {
            Ok(u) => u,
            Err(e) => {
                self.status.set_text(&e);
                self.url.set_focus();
                return;
            }
        };
        let item = QueueItem {
            url,
            folder: self.folder(),
            format: self.format_id(),
            quality: self.quality_id(),
        };
        {
            let mut sh = self.shared.lock().unwrap();
            sh.queue.push_back(item);
            if busy {
                // show the new count right away
                let base = sh.text.split("  ·  noch ").next().unwrap_or_default().to_string();
                let waiting = sh.queue.len();
                sh.text = with_queue(&base, waiting);
            }
        }
        self.url.set_text("");
        self.url.set_focus();
        if !busy {
            self.start_worker();
        }
        self.on_notice();
    }

    /// Works through the queue, one link after the other, until it is empty
    /// or everything is cancelled.
    fn start_worker(&self) {
        {
            let mut sh = self.shared.lock().unwrap();
            sh.busy = true;
            sh.cancelled = false;
            sh.percent = 0;
            sh.text = "Startet …".into();
        }
        let shared = self.shared.clone();
        let sender = self.notice.sender();
        std::thread::spawn(move || loop {
            let next = {
                let mut sh = shared.lock().unwrap();
                let next = if sh.cancelled { None } else { sh.queue.pop_front() };
                if next.is_none() {
                    sh.busy = false;
                    sh.queue.clear();
                }
                next
            };
            let Some(item) = next else {
                sender.notice();
                break;
            };
            let set_pid = |p: Option<u32>| shared.lock().unwrap().pid = p;
            let send = |u: job::Update| {
                let mut sh = shared.lock().unwrap();
                let waiting = sh.queue.len();
                match u {
                    job::Update::Status(t, p) => {
                        sh.text = with_queue(&t, waiting);
                        sh.percent = p;
                    }
                    job::Update::Done(file) => {
                        let name = std::path::Path::new(&file)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        sh.text = with_queue(&format!("✓ {name}"), waiting);
                        sh.percent = 100;
                        sh.done_files.push(file);
                    }
                    job::Update::Failed(e) => {
                        sh.text = if sh.cancelled { "Abgebrochen".into() } else { with_queue(&e, waiting) };
                        sh.percent = 0;
                    }
                }
                drop(sh);
                sender.notice();
            };
            job::run(&item.url, &item.folder, item.format, item.quality, &set_pid, &send);
        });
    }

    /// Stops the running download and drops everything still waiting.
    fn cancel_all(&self) {
        let pid = {
            let mut sh = self.shared.lock().unwrap();
            sh.cancelled = true;
            sh.queue.clear();
            sh.pid
        };
        if let Some(pid) = pid {
            job::cancel(pid);
        }
    }

    /// The main button says "Abbrechen" only while busy with an empty field;
    /// otherwise it adds what is in the field.
    fn update_go_label(&self) {
        let busy = self.shared.lock().unwrap().busy;
        let empty = self.url.text().trim().is_empty();
        self.go_btn.set_text(if busy && empty { "Abbrechen" } else { "Download" });
    }

    /// Worker thread poked us: copy its state into the controls.
    fn on_notice(&self) {
        let (new_files, update) = {
            let mut sh = self.shared.lock().unwrap();
            self.status.set_text(&sh.text);
            self.progress.set_pos(sh.percent);
            self.go_btn.set_enabled(sh.ready);
            (std::mem::take(&mut sh.done_files), sh.update.clone())
        };
        self.update_go_label();
        if let Some(v) = update {
            if !self.update_btn.visible() {
                self.update_btn.set_text(&format!("Update {v}"));
                self.update_btn.set_visible(true);
                self.layout();
            }
        }
        if !new_files.is_empty() {
            let when = history::now();
            for f in &new_files {
                self.entries.borrow_mut().insert(0, history::Entry { when: when.clone(), path: f.clone() });
            }
            *self.last_file.borrow_mut() = new_files.last().cloned();
            history::save(&self.entries.borrow());
            self.refresh_history();
        }
    }

    // ---------- history ----------

    fn refresh_history(&self) {
        let today = history::now().split(' ').next().unwrap_or_default().to_string();
        self.history.clear();
        for e in self.entries.borrow().iter() {
            let row = history::row(e, &today);
            self.history.insert_items_row(None, &row);
        }
    }

    fn selected_entry(&self) -> Option<(usize, history::Entry)> {
        let i = self.history.selected_item()?;
        self.entries.borrow().get(i).cloned().map(|e| (i, e))
    }

    fn hist_play(&self) {
        if let Some((_, e)) = self.selected_entry() {
            if std::path::Path::new(&e.path).exists() {
                open_in_explorer(&e.path, false);
            } else {
                self.status.set_text("Datei gibt es nicht mehr");
            }
        }
    }

    fn hist_reveal(&self) {
        if let Some((_, e)) = self.selected_entry() {
            let p = std::path::Path::new(&e.path);
            if p.exists() {
                open_in_explorer(&e.path, true);
            } else if let Some(dir) = p.parent().filter(|d| d.exists()) {
                open_in_explorer(&dir.to_string_lossy(), false);
            }
        }
    }

    fn hist_remove(&self) {
        if let Some((i, _)) = self.selected_entry() {
            self.entries.borrow_mut().remove(i);
            history::save(&self.entries.borrow());
            self.refresh_history();
        }
    }

    fn hist_clear(&self) {
        self.entries.borrow_mut().clear();
        history::save(&[]);
        self.refresh_history();
    }

    fn hist_menu_show(&self) {
        let has = self.selected_entry().is_some();
        self.hist_open.set_enabled(has);
        self.hist_reveal_item.set_enabled(has);
        self.hist_remove_item.set_enabled(has);
        self.hist_clear_item.set_enabled(!self.entries.borrow().is_empty());
        let (x, y) = nwg::GlobalCursor::position();
        self.hist_menu.popup(x, y);
    }

    /// Opens the newest release on GitHub in the browser.
    fn open_update(&self) {
        open_in_explorer(&format!("https://github.com/{}/releases/latest", tools::REPO), false);
    }

    // ---------- folder ----------

    fn folder_menu_show(&self) {
        let (x, y) = nwg::GlobalCursor::position();
        self.folder_menu.popup(x, y);
    }

    fn open_folder(&self) {
        let folder = self.folder();
        match self.last_file.borrow().as_deref() {
            // the last download, if it went into this folder
            Some(f) if std::path::Path::new(f).exists() && f.starts_with(&folder) => open_in_explorer(f, true),
            _ => open_in_explorer(&folder, false),
        }
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
        self.cancel_all();
        nwg::stop_thread_dispatch();
    }
}

fn main() {
    settings::migrate_old_name();
    dark::dark_menus();
    nwg::init().expect("Windows-Oberfläche konnte nicht starten");
    let mut font = nwg::Font::default();
    if nwg::Font::builder().family("Segoe UI").size(17).build(&mut font).is_ok() {
        let _ = nwg::Font::set_global_default(Some(font));
    }
    let _app = App::build_ui(Default::default()).expect("Fenster konnte nicht erstellt werden");
    nwg::dispatch_thread_events();
}
