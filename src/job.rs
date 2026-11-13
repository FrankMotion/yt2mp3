//! One download: yt-dlp fetches the audio (or video), ffmpeg converts or
//! merges it and embeds title, artist and cover where the format allows.

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

/// What the format list offers: (label, id, is video).
pub const FORMATS: [(&str, &str, bool); 8] = [
    ("MP3", "mp3", false),
    ("M4A", "m4a", false),
    ("Opus", "opus", false),
    ("FLAC", "flac", false),
    ("WAV", "wav", false),
    ("MP4 (Video)", "mp4", true),
    ("MKV (Video)", "mkv", true),
    ("WebM (Video)", "webm", true),
];

/// Video qualities: (label, id). "best" = highest the video has.
pub const QUALITIES: [(&str, &str); 7] = [
    ("Beste", "best"),
    ("4K", "2160"),
    ("1440p", "1440"),
    ("1080p", "1080"),
    ("720p", "720"),
    ("480p", "480"),
    ("360p", "360"),
];

pub fn is_video(format: &str) -> bool {
    FORMATS.iter().any(|f| f.1 == format && f.2)
}

/// yt-dlp options for one format (and, for video, one quality).
fn format_args(format: &str, quality: &str) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    // square album cover from the 16:9 thumbnail, for the audio formats that keep a cover
    let square = "ThumbnailsConvertor+FFmpeg_o:-c:v mjpeg -vf \"crop=w='min(iw,ih)':h='min(iw,ih)'\"";
    let cover = s(&["--embed-thumbnail", "--convert-thumbnails", "jpg", "--ppa", square]);
    let mut a = match format {
        // re-encoded at the best MP3 rate
        "mp3" => s(&["-f", "bestaudio/best", "-x", "--audio-format", "mp3", "--audio-quality", "320K"]),
        // YouTube's own AAC / Opus stream, kept as it is (no re-encoding)
        "m4a" => s(&["-f", "bestaudio[ext=m4a]/bestaudio/best", "-x", "--audio-format", "m4a"]),
        "opus" => s(&["-f", "bestaudio[acodec=opus]/bestaudio/best", "-x", "--audio-format", "opus"]),
        // lossless containers (as good as the source, much bigger)
        "flac" => s(&["-f", "bestaudio/best", "-x", "--audio-format", "flac"]),
        "wav" => s(&["-f", "bestaudio/best", "-x", "--audio-format", "wav"]),
        _ => {
            let res = if quality == "best" { "res".to_string() } else { format!("res:{quality}") };
            match format {
                // H.264 + AAC first, so it plays everywhere
                "mp4" => s(&["-f", "bv*+ba/b", "-S", &format!("{res},vcodec:h264,acodec:m4a"), "--merge-output-format", "mp4"]),
                "webm" => s(&["-f", "bv*[ext=webm]+ba[ext=webm]/bv*+ba/b", "-S", &format!("{res},vcodec:vp9,acodec:opus"), "--merge-output-format", "webm"]),
                // MKV takes any codec: simply the best picture and sound
                _ => s(&["-f", "bv*+ba/b", "-S", &res, "--merge-output-format", "mkv"]),
            }
        }
    };
    a.push("--embed-metadata".into());
    match format {
        "mp3" | "m4a" | "opus" | "flac" => a.extend(cover),
        "mp4" | "mkv" => a.push("--embed-thumbnail".into()),
        _ => {} // WAV and WebM can't carry a cover
    }
    a
}

/// Runs the download. `pid` receives the process id (for cancelling),
/// `send` gets progress; returns when yt-dlp has exited.
pub fn run(
    url: &str,
    folder: &str,
    format: &str,
    quality: &str,
    pid: &dyn Fn(Option<u32>),
    send: &dyn Fn(Update),
) {
    let video = is_video(format);
    let label = FORMATS.iter().find(|f| f.1 == format).map(|f| f.0).unwrap_or("MP3");
    let folder = Path::new(folder);
    if let Err(e) = std::fs::create_dir_all(folder) {
        return send(Update::Failed(format!("Ordner nicht nutzbar: {e}")));
    }
    let work = std::env::temp_dir().join("corvus");
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
        .args(format_args(format, quality))
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
    // Video comes as two downloads (picture, then sound); the bar spans both:
    // audio: download 0-90 %, video: picture 0-70 %, sound 70-88 %.
    let mut part = 0u32;
    for line in rx {
        if let Some(rest) = line.strip_prefix("[[P]]") {
            let f: Vec<f64> = rest.split('|').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
            let at = |i: usize| f.get(i).copied().unwrap_or(0.0);
            let size = if at(1) > 0.0 { at(1) } else { at(2) };
            let frac = if size > 0.0 { (at(0) / size).clamp(0.0, 1.0) } else { 0.0 };
            let (bar, shown) = if !video {
                ((frac * 90.0) as u32, (frac * 100.0) as u32)
            } else if part <= 1 {
                ((frac * 70.0) as u32, (frac * 100.0) as u32)
            } else {
                (70 + (frac * 18.0) as u32, (frac * 100.0) as u32)
            };
            if bar != last {
                last = bar;
                let what = match (video, part) {
                    (true, p) if p >= 2 => "Lädt Ton",
                    (true, _) => "Lädt Bild",
                    _ => "Lädt",
                };
                let speed = at(3);
                let text = if speed > 0.0 {
                    format!("{what} … {shown} %  ·  {:.1} MB/s", speed / 1_048_576.0)
                } else {
                    format!("{what} … {shown} %")
                };
                send(Update::Status(text, bar));
            }
        } else if line.starts_with("[download] Destination:") {
            part += 1;
        } else if line.starts_with("[ExtractAudio]") {
            send(Update::Status(format!("Wandelt in {label} um …"), 92));
        } else if line.starts_with("[Merger]") {
            send(Update::Status("Fügt Bild und Ton zusammen …".into(), 90));
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
