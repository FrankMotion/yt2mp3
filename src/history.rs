//! Download history in %APPDATA%\Corvus\history.txt, newest first.
//! One line per download: "YYYY-MM-DD HH:MM<TAB>full path".

use std::path::{Path, PathBuf};

const MAX: usize = 200;

#[derive(Clone)]
pub struct Entry {
    /// local time, "YYYY-MM-DD HH:MM"
    pub when: String,
    pub path: String,
}

#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    millis: u16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(t: *mut SystemTime);
}

/// Local time now, "YYYY-MM-DD HH:MM".
pub fn now() -> String {
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute)
}

fn file() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("Corvus");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history.txt"))
}

pub fn load() -> Vec<Entry> {
    let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else {
        return Vec::new();
    };
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|l| {
            let (when, path) = l.split_once('\t')?;
            Some(Entry { when: when.trim().into(), path: path.trim().into() })
        })
        .filter(|e| !e.path.is_empty())
        .take(MAX)
        .collect()
}

pub fn save(entries: &[Entry]) {
    if let Some(f) = file() {
        let text: String = entries
            .iter()
            .take(MAX)
            .map(|e| format!("{}\t{}\n", e.when, e.path))
            .collect();
        let _ = std::fs::write(f, text);
    }
}

/// The two columns of a list row: when (time today, else date) and what.
pub fn row(e: &Entry, today: &str) -> [String; 2] {
    let (date, time) = e.when.split_once(' ').unwrap_or((&e.when, ""));
    let when = if date == today {
        time.to_string()
    } else {
        // "2026-11-13" -> "13.11."
        let p: Vec<&str> = date.split('-').collect();
        if p.len() == 3 { format!("{}.{}.", p[2], p[1]) } else { date.to_string() }
    };
    let p = Path::new(&e.path);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| e.path.clone());
    let video = matches!(
        p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(),
        Some("mp4" | "mkv" | "webm")
    );
    let mark = if !p.exists() { "✕" } else if video { "▶" } else { "♪" };
    [when, format!("{mark}   {name}")]
}
