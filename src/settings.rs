//! Tiny key=value settings file in %APPDATA%\YT2MP3\settings.txt.

use std::path::PathBuf;

#[derive(Clone)]
pub struct Settings {
    /// Where music (MP3, M4A, …) goes.
    pub folder: String,
    /// Where videos (MP4, MKV, WebM) go.
    pub video_folder: String,
    /// Last chosen format, e.g. "mp3" or "mp4".
    pub format: String,
    /// Last chosen video quality, e.g. "1080" or "best".
    pub quality: String,
    /// Unix seconds of the last yt-dlp self-update.
    pub last_update: u64,
}

fn file() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("YT2MP3");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.txt"))
}

fn user_folder(name: &str) -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\{name}"))
        .unwrap_or_else(|_| "C:\\".into())
}

pub fn load() -> Settings {
    let mut s = Settings {
        folder: user_folder("Music"),
        video_folder: user_folder("Videos"),
        format: "mp3".into(),
        quality: "1080".into(),
        last_update: 0,
    };
    if let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) {
        // tolerate a UTF-8 BOM (Notepad and PowerShell like to add one)
        for line in text.trim_start_matches('\u{feff}').lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "folder" if !v.is_empty() => s.folder = v.to_string(),
                "video_folder" if !v.is_empty() => s.video_folder = v.to_string(),
                "format" if !v.is_empty() => s.format = v.to_string(),
                "quality" if !v.is_empty() => s.quality = v.to_string(),
                "last_update" => s.last_update = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    s
}

pub fn save(s: &Settings) {
    if let Some(f) = file() {
        let text = format!(
            "folder={}\nvideo_folder={}\nformat={}\nquality={}\nlast_update={}\n",
            s.folder, s.video_folder, s.format, s.quality, s.last_update
        );
        let _ = std::fs::write(f, text);
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
