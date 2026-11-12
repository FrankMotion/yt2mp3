//! One download: yt-dlp fetches the best audio, ffmpeg turns it into a
//! 320 kbit/s MP3 with title, artist and a square cover.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc;

use crate::tools;

pub enum Update {
    Status(String, u32),
    Done(String),
    Failed(String),
}

/// Only real YouTube links: the host itself must be YouTube (not just appear
/// somewhere in the text). Returns the trimmed link.
pub fn check_url(url: &str) -> Result<String, String> {
    let u = url.trim();
    let bad = || "Kein YouTube-Link".to_string();
    if u.len() > 2000 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let lower = u.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(bad()); // "https://youtube.com@evil.example/" tricks
    }
    let host = authority.split(':').next().unwrap_or_default();
    let youtube = host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com");
    if !youtube {
        return Err(bad());
    }
    if rest.contains("/playlist?") {
        return Err("Playlists gehen nicht – bitte ein einzelnes Video".into());
    }
    Ok(u.to_string())
}

fn friendly(err: &str) -> String {
    let e = err.to_lowercase();
    let rules = [
        ("private video", "Video ist privat"),
        ("video unavailable", "Video nicht verfügbar"),
        ("confirm your age", "Altersbeschränkt – geht nicht ohne Anmeldung"),
        ("not a bot", "YouTube blockt gerade – später nochmal"),
        ("http error 403", "Von YouTube abgelehnt – später nochmal"),
        ("http error 429", "Zu viele Anfragen – kurz warten"),
        ("is live", "Livestreams gehen nicht"),
        ("getaddrinfo failed", "Keine Internetverbindung"),
    ];
    for (needle, msg) in rules {
        if e.contains(needle) {
            return msg.into();
        }
    }
    err.lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .map(|l| l.replace("ERROR:", "").trim().chars().take(90).collect())
        .unwrap_or_else(|| "Fehler bei yt-dlp".into())
}

fn pump(r: impl Read + Send + 'static, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut r = BufReader::new(r);
        let mut buf = Vec::new();
        while matches!(r.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
            if tx.send(String::from_utf8_lossy(&buf).trim_end().to_string()).is_err() {
                break;
            }
            buf.clear();
        }
    });
}

/// Runs the download. `pid` receives the process id (for cancelling),
/// `send` gets progress; returns when yt-dlp has exited.
pub fn run(url: &str, folder: &str, pid: &dyn Fn(Option<u32>), send: &dyn Fn(Update)) {
    let folder = Path::new(folder);
    if let Err(e) = std::fs::create_dir_all(folder) {
        return send(Update::Failed(format!("Ordner nicht nutzbar: {e}")));
    }
    let work = std::env::temp_dir().join("yt2mp3");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::create_dir_all(&work);
    let result = work.join("result.txt");

    let mut c = tools::hidden(tools::ytdlp());
    // Locked down: no config files, no plugins, no --exec, YouTube extractor only.
    c.args(["--ignore-config", "--no-plugin-dirs", "--no-exec", "--use-extractors", "youtube"])
        .args(["--no-playlist", "--newline", "--no-mtime", "--windows-filenames"])
        // use the Windows certificate store: works behind antivirus HTTPS scanning
        .args(["--compat-options", "no-certifi"])
        .arg("--ffmpeg-location")
        .arg(tools::bin())
        .arg("--js-runtimes")
        .arg(format!("deno:{}", tools::deno().display()))
        .arg("-P")
        .arg(folder)
        .arg("-P")
        .arg(format!("temp:{}", work.display()))
        .args(["-o", "%(title)s.%(ext)s"])
        .args(["-f", "bestaudio/best", "-x", "--audio-format", "mp3", "--audio-quality", "320K"])
        .args(["--embed-metadata", "--embed-thumbnail", "--convert-thumbnails", "jpg"])
        .arg("--ppa")
        .arg("ThumbnailsConvertor+FFmpeg_o:-c:v mjpeg -vf \"crop=w='min(iw,ih)':h='min(iw,ih)'\"")
        .args(["--progress-template", "download:[[P]]%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s"])
        .arg("--print-to-file")
        .arg("after_move:%(filepath)s")
        .arg(&result)
        .arg("--")
        .arg(url)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return send(Update::Failed(format!("yt-dlp startet nicht: {e}"))),
    };
    pid(Some(child.id()));
    let (tx, rx) = mpsc::channel();
    if let Some(o) = child.stdout.take() {
        pump(o, tx.clone());
    }
    if let Some(e) = child.stderr.take() {
        pump(e, tx.clone());
    }
    drop(tx);

    send(Update::Status("Lädt …".into(), 0));
    let mut errors = String::new();
    let mut last = u32::MAX;
    for line in rx {
        if let Some(rest) = line.strip_prefix("[[P]]") {
            let f: Vec<f64> = rest.split('|').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
            let at = |i: usize| f.get(i).copied().unwrap_or(0.0);
            let size = if at(1) > 0.0 { at(1) } else { at(2) };
            let pct = if size > 0.0 { (at(0) / size * 90.0) as u32 } else { 0 };
            if pct != last {
                last = pct;
                let speed = at(3);
                let text = if speed > 0.0 {
                    format!("Lädt … {} %  ·  {:.1} MB/s", pct * 100 / 90, speed / 1_048_576.0)
                } else {
                    format!("Lädt … {} %", pct * 100 / 90)
                };
                send(Update::Status(text, pct));
            }
        } else if line.starts_with("[ExtractAudio]") {
            send(Update::Status("Wandelt in MP3 um …".into(), 92));
        } else if line.starts_with("[EmbedThumbnail]") || line.starts_with("[Metadata]") {
            send(Update::Status("Cover und Titel …".into(), 97));
        } else if line.contains("ERROR") {
            errors.push_str(&line);
            errors.push('\n');
        }
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    pid(None);
    let file = std::fs::read_to_string(&result)
        .ok()
        .and_then(|t| t.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()));
    let _ = std::fs::remove_dir_all(&work);
    match (ok, file) {
        (true, Some(f)) => send(Update::Done(f)),
        (true, None) => send(Update::Failed("Fertig, aber Datei nicht gefunden".into())),
        (false, _) => send(Update::Failed(friendly(&errors))),
    }
}

/// Kills yt-dlp and the ffmpeg it started.
pub fn cancel(pid: u32) {
    let _ = tools::hidden(tools::windows_exe("taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}
