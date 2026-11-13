//! The helper programs, downloaded once into %LOCALAPPDATA%\Corvus\bin
//! with Windows' own curl.exe and tar.exe, each checked against the
//! SHA-256 checksum its project publishes:
//!   yt-dlp.exe              fetches audio/video from YouTube, TikTok, Instagram
//!   ffmpeg.exe, ffprobe.exe convert to MP3, embed title and cover
//!   deno.exe                JavaScript runtime yt-dlp needs for YouTube
//!   cacert.pem              Mozilla root certificates (see certs.rs)

use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const NO_WINDOW: u32 = 0x0800_0000;

struct Tool {
    label: &'static str,
    url: &'static str,
    /// where the official checksum list lives
    sums: &'static str,
    /// files this download provides
    files: &'static [&'static str],
}

const TOOLS: [Tool; 4] = [
    Tool {
        label: "yt-dlp",
        url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        sums: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS",
        files: &["yt-dlp.exe"],
    },
    Tool {
        label: "FFmpeg",
        url: "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
        sums: "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/checksums.sha256",
        files: &["ffmpeg.exe", "ffprobe.exe"],
    },
    Tool {
        label: "Deno",
        url: "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip",
        sums: "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip.sha256sum",
        files: &["deno.exe"],
    },
    Tool {
        // Mozilla's trusted root certificates, see certs.rs
        label: "Zertifikate",
        url: "https://curl.se/ca/cacert.pem",
        sums: "https://curl.se/ca/cacert.pem.sha256",
        files: &["cacert.pem"],
    },
];

pub fn bin() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Corvus").join("bin")
}

pub fn ytdlp() -> PathBuf {
    bin().join("yt-dlp.exe")
}

pub fn deno() -> PathBuf {
    bin().join("deno.exe")
}

pub fn ready() -> bool {
    let b = bin();
    TOOLS.iter().flat_map(|t| t.files).all(|f| b.join(f).exists())
}

/// Windows programs by full path, so nothing named like them elsewhere is started.
pub fn windows_exe(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
    if name.eq_ignore_ascii_case("explorer.exe") {
        root.join(name)
    } else {
        root.join("System32").join(name)
    }
}

pub fn hidden(exe: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(NO_WINDOW).stdin(Stdio::null());
    c
}

fn curl() -> Command {
    let mut c = hidden(windows_exe("curl.exe"));
    // --ssl-revoke-best-effort: antivirus HTTPS scanning often blocks revocation checks
    c.args(["-L", "--fail", "-sS", "--proto", "=https", "--ssl-revoke-best-effort", "--retry", "2"]);
    c
}

/// Size announced by the server (after GitHub's redirects), if any.
fn remote_size(url: &str) -> u64 {
    let Ok(out) = curl().arg("-I").arg(url).output() else { return 0 };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<u64>().ok())?
        })
        .last()
        .unwrap_or(0)
}

/// The published SHA-256 for `file_name`: a 64-hex word on the line naming the
/// file, or the only one in a single-file checksum (Deno's format).
fn expected_hash(sums_url: &str, file_name: &str) -> Result<String, String> {
    let out = curl().arg(sums_url).output().map_err(|e| format!("curl fehlt: {e}"))?;
    if !out.status.success() {
        return Err("Prüfsumme nicht abrufbar".into());
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let hex64 = |l: &str| {
        l.split(|c: char| !c.is_ascii_hexdigit())
            .find(|w| w.len() == 64)
            .map(|w| w.to_ascii_lowercase())
    };
    text.lines()
        .filter(|l| l.contains(file_name))
        .find_map(hex64)
        .or_else(|| text.lines().find_map(hex64))
        .ok_or_else(|| "Prüfsumme nicht gefunden".into())
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Downloads `url` to `dest` (via a .part file), calling `progress(done, total)`.
fn download(url: &str, dest: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let total = remote_size(url);
    let part = dest.with_extension("part");
    let _ = std::fs::remove_file(&part);
    let mut child = curl()
        .arg("-o")
        .arg(&part)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl fehlt: {e}"))?;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                let _ = std::fs::remove_file(&part);
                return Err(format!("Download fehlgeschlagen: {}", err.trim()));
            }
            break;
        }
        let done = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        progress(done, total);
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Extracts the zip into a temp folder and moves only the wanted files into bin.
fn unzip(zip: &Path, wanted: &[&str]) -> Result<(), String> {
    let tmp = bin().join("unzip");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let ok = hidden(windows_exe("tar.exe"))
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(&tmp)
        .status()
        .map_err(|e| format!("tar fehlt: {e}"))?
        .success();
    if !ok {
        return Err("Entpacken fehlgeschlagen".into());
    }
    let mut found = 0;
    let mut stack = vec![tmp.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if wanted.contains(&name) {
                    std::fs::rename(&p, bin().join(name)).map_err(|e| e.to_string())?;
                    found += 1;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if found < wanted.len() {
        return Err("Archiv unvollständig".into());
    }
    Ok(())
}

/// First start: fetch whatever is missing. `report(text, percent)`.
pub fn setup(report: &dyn Fn(String, u32)) -> Result<(), String> {
    let b = bin();
    std::fs::create_dir_all(&b).map_err(|e| e.to_string())?;
    for t in &TOOLS {
        if t.files.iter().all(|f| b.join(f).exists()) {
            continue;
        }
        let label = t.label;
        let show = |done: u64, total: u64| {
            let mb = |x: u64| x / 1_048_576;
            let pct = if total > 0 { (done * 100 / total) as u32 } else { 0 };
            let text = if total > 0 {
                format!("Einrichtung: {label} {} / {} MB", mb(done), mb(total))
            } else {
                format!("Einrichtung: {label} {} MB", mb(done))
            };
            report(text, pct);
        };
        let file_name = t.url.rsplit('/').next().unwrap_or_default();
        let target = b.join(format!("{label}.download"));
        download(t.url, &target, &show)?;

        report(format!("Einrichtung: {label} prüfen …"), 100);
        let expected = expected_hash(t.sums, file_name)?;
        if sha256_of(&target)? != expected {
            let _ = std::fs::remove_file(&target);
            return Err(format!("{label}: Prüfsumme falsch – Datei verworfen"));
        }

        if file_name.ends_with(".zip") {
            report(format!("Einrichtung: {label} entpacken …"), 100);
            let r = unzip(&target, t.files);
            let _ = std::fs::remove_file(&target);
            r?;
        } else {
            std::fs::rename(&target, b.join(t.files[0])).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// yt-dlp updates itself (and checks its own download). Returns the new version.
pub fn update_ytdlp() -> Result<String, String> {
    let out = hidden(ytdlp())
        .args(["--compat-options", "no-certifi", "-U"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v = hidden(ytdlp()).arg("--version").output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&v.stdout).trim().to_string())
}

/// Where Corvus' own releases live.
pub const REPO: &str = "FrankMotion/corvus";

/// "v1.8.0" / "1.8.0" -> [1, 8, 0]
fn version_numbers(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.trim().parse().unwrap_or(0)).collect()
}

/// The newest release on GitHub, if it is newer than this build ("1.8.0").
/// Any problem (offline, GitHub busy) simply means "no update".
pub fn newer_release() -> Option<String> {
    let out = curl()
        .args(["--max-time", "10", "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: Corvus"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .output()
        .ok()?;
    let body = String::from_utf8_lossy(&out.stdout);
    // "tag_name": "v1.8.0"
    let rest = &body[body.find("\"tag_name\"")? + 10..];
    let start = rest.find('"')? + 1;
    let tag = &rest[start..start + rest[start..].find('"')?];
    let latest = version_numbers(tag);
    let mine = version_numbers(env!("CARGO_PKG_VERSION"));
    (latest > mine).then(|| tag.trim_start_matches(['v', 'V']).to_string())
}
