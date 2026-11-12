//! Tiny key=value settings file in %APPDATA%\YT2MP3\settings.txt.

use std::path::PathBuf;

#[derive(Clone)]
pub struct Settings {
    /// Where the MP3s go.
    pub folder: String,
    /// Unix seconds of the last yt-dlp self-update.
    pub last_update: u64,
}

fn file() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("YT2MP3");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.txt"))
}

fn default_folder() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\Music"))
        .unwrap_or_else(|_| "C:\\".into())
}

pub fn load() -> Settings {
    let mut s = Settings { folder: default_folder(), last_update: 0 };
    if let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) {
        // tolerate a UTF-8 BOM (Notepad and PowerShell like to add one)
        for line in text.trim_start_matches('\u{feff}').lines() {
            match line.split_once('=') {
                Some(("folder", v)) if !v.trim().is_empty() => s.folder = v.trim().to_string(),
                Some(("last_update", v)) => s.last_update = v.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    s
}

pub fn save(s: &Settings) {
    if let Some(f) = file() {
        let _ = std::fs::write(f, format!("folder={}\nlast_update={}\n", s.folder, s.last_update));
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
