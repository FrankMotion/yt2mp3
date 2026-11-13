<img src="res/icon-source.png" width="128" align="right" alt="Corvus">

# Corvus

Link rein, Musik oder Video raus – von **YouTube, TikTok und Instagram**. Ein kleines schwarzes Windows-Fenster, sonst nichts.

- **Musik:** MP3 (320 kbit/s), M4A, Opus, FLAC, WAV – mit Titel, Künstler und quadratischem Cover
- **Video:** MP4, MKV, WebM – Qualität wählbar von 360p bis 4K
- Natives Windows-Fenster in Rust: ca. 400 KB, ca. 3 MB RAM, Größe frei veränderbar
- Keine Installation: `Corvus.exe` starten, fertig
- Holt sich beim ersten Start seine Hilfsprogramme selbst und hält yt-dlp aktuell

## Benutzen

1. `Corvus.exe` unter [Releases](../../releases) herunterladen und starten
2. Beim ersten Start lädt die App einmalig ca. 200 MB Hilfsprogramme (yt-dlp, FFmpeg, Deno, Zertifikatsliste)
3. Link einfügen (YouTube-Video, TikTok, Instagram-Reel oder -Post), Format wählen (bei Video zusätzlich die Qualität) → **Download**
4. **Ordner** → *Öffnen* zeigt die Datei, *Ändern …* wählt den Speicherort
   (Musik und Videos haben je einen eigenen Ordner)

| Format | Gut für |
|---|---|
| MP3 | läuft überall: Handy, Auto, jeder Player |
| M4A / Opus | YouTubes eigener Ton ohne Umwandlung – etwas kleiner und besser |
| FLAC / WAV | verlustfrei bzw. unkomprimiert, für Schnitt und Produktion (groß) |
| MP4 | Video, das überall läuft |
| MKV / WebM | Video in bester Qualität bzw. im Web-Format |

## Sicherheit

- Nimmt nur echte Links von YouTube, TikTok und Instagram an (der Server muss stimmen, Tricks wie
  `youtube.com@andere-seite` werden abgelehnt) und lässt yt-dlp nur diese drei Seiten benutzen.
- yt-dlp läuft ohne Konfigurationsdateien, ohne Plugins und ohne `--exec`.
- Die Hilfsprogramme kommen nur von den offiziellen GitHub-Seiten der Projekte (nur HTTPS) und
  werden vor der Benutzung mit deren veröffentlichter SHA-256-Prüfsumme verglichen.
- Windows-Programme (curl, tar, explorer, taskkill) werden mit vollem Pfad gestartet.
- Keine Admin-Rechte, kein offener Port, keine Daten an irgendwen außer YouTube und GitHub.

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/main.rs` | Fenster und Bedienung (native-windows-gui) |
| `src/dark.rs` | schwarzer Look |
| `src/job.rs` | ein Download: Link prüfen, yt-dlp + FFmpeg, Fortschritt, Abbrechen |
| `src/tools.rs` | Hilfsprogramme laden und prüfen (über Windows' `curl` und `tar`) |
| `src/certs.rs` | Zertifikate für Instagram (funktioniert auch hinter Virenscannern) |
| `src/settings.rs` | Ordner, Format und Qualität merken |
| `res/` | Icon und Windows-Manifest |

## Hinweise

- **Instagram:** Nur öffentliche Reels und Posts. Stories und private Konten brauchen eine Anmeldung und gehen nicht.
- **TikTok:** Direkt-Links zu einem Video (auch die kurzen `vm.tiktok.com`-Links). Profilseiten gehen nicht.
