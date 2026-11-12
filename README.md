# YT2MP3

YouTube-Link rein, MP3 raus. Ein kleines schwarzes Windows-Fenster, sonst nichts.

- MP3 in 320 kbit/s, mit Titel, Künstler und quadratischem Cover
- Natives Windows-Fenster in Rust: ca. 400 KB, ca. 3 MB RAM
- Keine Installation: `YT2MP3.exe` starten, fertig
- Holt sich beim ersten Start seine Hilfsprogramme selbst und hält yt-dlp aktuell

## Benutzen

1. `YT2MP3.exe` unter [Releases](../../releases) herunterladen und starten
2. Beim ersten Start lädt die App einmalig ca. 200 MB Hilfsprogramme (yt-dlp, FFmpeg, Deno)
3. Link einfügen → **Download**
4. **Ordner** → *Öffnen* zeigt die Datei, *Ändern …* wählt den Speicherort

## Sicherheit

- Nimmt nur echte YouTube-Links an (der Server muss YouTube sein, Tricks wie
  `youtube.com@andere-seite` werden abgelehnt) und lässt yt-dlp nur den YouTube-Teil benutzen.
- yt-dlp läuft ohne Konfigurationsdateien, ohne Plugins und ohne `--exec`.
- Die Hilfsprogramme kommen nur von den offiziellen GitHub-Seiten der Projekte (nur HTTPS) und
  werden vor der Benutzung mit deren veröffentlichter SHA-256-Prüfsumme verglichen.
- Windows-Programme (curl, tar, explorer, taskkill) werden mit vollem Pfad gestartet.
- Keine Admin-Rechte, kein offener Port, keine Daten an irgendwen außer YouTube und GitHub.

## Wo liegt was

| | |
|---|---|
| Einstellungen | `%APPDATA%\YT2MP3\settings.txt` |
| Hilfsprogramme | `%LOCALAPPDATA%\YT2MP3\bin` |

## Selbst bauen

Voraussetzungen: [Rust](https://rustup.rs) und die Visual Studio Build Tools (C++).

```powershell
./scripts/build.ps1
```

Ergebnis: `YT2MP3.exe` im Projektordner. Das Skript sorgt dafür, dass keine lokalen
Ordnernamen (z. B. dein Windows-Benutzername) in der `.exe` landen.
Ein neues Release entsteht automatisch, wenn ein Tag wie `v1.0.1` gepusht wird.

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/main.rs` | Fenster und Bedienung (native-windows-gui) |
| `src/dark.rs` | schwarzer Look |
| `src/job.rs` | ein Download: Link prüfen, yt-dlp + FFmpeg, Fortschritt, Abbrechen |
| `src/tools.rs` | Hilfsprogramme laden und prüfen (über Windows' `curl` und `tar`) |
| `src/settings.rs` | Speicherort merken |
| `res/` | Icon und Windows-Manifest |
