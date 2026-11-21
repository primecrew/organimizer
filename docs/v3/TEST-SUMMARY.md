# PrimeCrew Organimizer 3.0.0 - test summary

Branch `v3-organimizer`, not released. All tests below ran on Linux with
**Wine** (no real Windows machine was available), so please test on Windows
before merging, tagging or releasing.

## Automated

- `cargo test --no-default-features`: 130 unit tests pass (Plex name parser,
  extras/subtitle linking, blacklist matching and defaults, protected file
  types, custom type lists, favorites, PhotoSorter settings migration, TMDB
  request building and response parsing, plus all 2.x tests), and 3
  regression tests in `tests/text_input.rs` (eframe built with the system
  clipboard; typing and Ctrl+V paste reach a password field inside a modal).
- `cargo clippy --target x86_64-pc-windows-gnu`: no warnings.

## Manual (Wine, Windows 10 mode)

| Test | Result |
|---|---|
| Fresh install (`Organimizer-3.0.0-setup.exe /S`) into a new, empty prefix | OK: files in `%LOCALAPPDATA%\Programs\Organimizer`, Start Menu `PrimeCrew Software\PrimeCrew Organimizer`, Add/Remove entry "PrimeCrew Organimizer 3.0.0" |
| Re-running setup over the same version (`/S`) | OK, exe replaced |
| Upgrade over an installed PhotoSorter 2.0.5 (GUI) | OK: banner "Updating existing installation: PhotoSorter v2.0.5 -> PrimeCrew Organimizer v3.0.0"; old exe, folder, shortcut and Add/Remove entry removed; `PhotoSorter.ini` left byte-for-byte unchanged |
| First start after the upgrade | OK: settings and the preset "Holiday (from PhotoSorter)" imported, notice shown on its own status line |
| Launch, Organize preview on a sample photo folder | OK |
| Favorites: star toggle, Favorites menu, Manage Favorites window | OK |
| Folder blacklist: editor, "Test a folder" (`C:\Windows\System32` -> Blocked), typing `C:\Windows` as source is rejected and Apply stays disabled | OK |
| Plex tab on a sample folder (movies, TV episodes, subtitles, trailer, featurette, sample clip): scan, proposals, correct a low-confidence row | OK |
| Plex Apply + Undo on the sample (build from earlier the same day; the Plex code did not change after it) | OK, undo restored every file |
| TMDB with an invalid key | OK: clear error dialog, nothing changed (no real key was used in testing) |
| Tools > Options: type into the TMDB key field, Ctrl+V paste of a key copied outside the app (xclip), paste into Custom file types, Save -> values in Organimizer.ini | Before the fix: typing OK, paste did nothing. After: OK |
| F1 opens HELP.md (incl. "Getting a TMDB API key") | OK |

## Known limitations of this test run

- Wine only. The installer's "->" arrow in the upgrade banner is drawn with
  "→", which the Wine font shows as a box; Windows renders it (same as in the
  2.0.5 installer).
- Wine renders the window slowly in the virtual display; this is a Wine
  artefact (the same happens with PhotoSorter 2.0.5, and the app itself only
  repaints on input).
- A real TMDB lookup was not tested (no API key); the request format is
  covered by unit tests and an invalid-key request reached TMDB (HTTP 401).
- The exe and setup are unsigned: Windows SmartScreen will warn on first
  start. Check the SHA-256 against `Organimizer-3.0.0-SHA256SUMS.txt`.

Screenshots: `docs/v3/screenshots/`.
