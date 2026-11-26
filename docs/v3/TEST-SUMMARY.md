# Organimizer 3.0.6 - test summary

Branch `v3-organimizer`, not released. All tests below ran on Linux with
**Wine** (no real Windows machine was available), so please test on Windows
before merging, tagging or releasing.

## Automated

- `cargo test --no-default-features`: 171 tests pass:
  - 164 unit tests (Plex Rename only: inside a Season folder, show folder,
    library root, doubtful parents flagged, missing year is only a note
    plus an unticked add-year fix, folder fixes unticked by
    default, folder-fix collision never applied, apply + undo of folder
    renames, unticked fixes leave files correct; Recent folders MRU and
    blank start; Plex name parser, per-file recognition and Why text,
    disc rips and runtime rules, media duration from MP4/MKV/AVI headers,
    TMDB lookup grouping / query building / episode mapping / reassign /
    apply, the type-the-name prompt (low confidence and zero results,
    override query with year and type, invalid year rejected, cancel
    changes nothing) and JSON parsing against hand-written fixtures in
    `tests/fixtures/tmdb`, extras/subtitle linking, blacklist, protected
    file types, favorites, settings migration, plus all 2.x tests);
  - `tests/near_miss.rs`: 111 "almost named right" inputs with the exact
    expected Plex path (`Alien.1979.Directors.Cut.1080p.mkv`,
    `The Expanse S2 E5.mkv`, `Expanse - 2x05 - Doors and Corners.mkv`,
    `alien making of.mkv`, `Season2` folders, ...) and a check that every
    row has a Why text;
  - 3 regression tests in `tests/text_input.rs` (system clipboard; typing
    and Ctrl+V paste reach a password field inside a modal).
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
| Plex tab, Why column and "Why this suggestion" on near-miss demo files (`Alien.1979.1080p.BluRay.x264` with `alien making of.mkv`, trailer, featurette, subtitle; `The Expanse\Season2` with `2x01`, `S2 E2`, `s2e5` names) | OK: every row has a reason; runtimes shown |
| TMDB lookup flow against a local mock TMDB (`tests/wine_lookup_flow.sh` + `tests/mock_tmdb.py`, made-up key, hidden debug override of the API base): What you have -> Look up -> results with posters -> This one -> match window -> Up swaps two episodes -> OK -> preview shows `Alien (1979) {tmdb-348}` and `The Expanse (2015) {tmdb-63639}\Season 02\... - s02e01 - Safe.mkv`, Match "TMDB" | OK (screenshots `lookup-results.png`, `lookup-match.png`, `lookup-preview.png`) |

## 3.0.1 Wine smoke (own Xvfb display and Wine prefix)

| Test | Result |
|---|---|
| Portable `Organimizer.exe` launch; header shows v3.0.1; Help > About shows "Version 3.0.1" | OK |
| Installer upgrade over a silent 3.0.0 install (GUI, new empty prefix) | OK: first page banner "Updating existing installation: PrimeCrew Organimizer v3.0.0 -> v3.0.1" (Wine draws "->" as a box, same as 3.0.0); read-only "Installed in: %LOCALAPPDATA%\Programs\Organimizer". Cancel left DisplayVersion at 3.0.0 |

## 3.0.2 Wine smoke (own Xvfb display and Wine prefix)

| Test | Result |
|---|---|
| Portable launch; window title and header "Organimizer" v3.0.2; Help > About is a centred window with 72 px icon, "Organimizer", "Version 3.0.2", "by PrimeCrew Software", description, unsigned/GPL note, copyright, TMDB attribution and a Close button | OK (screenshot `about-3.0.2.png`) |
| Installer upgrade over a silent 3.0.1 install (GUI, completed) | OK: first page and Ready banner "Updating existing installation: Organimizer v3.0.1 -> v3.0.2" (Wine draws "->" as a box); Installed in stays `%LOCALAPPDATA%\Programs\Organimizer`. After install, Start Menu has only `PrimeCrew Software\Organimizer.lnk` (leftover `PrimeCrew Organimizer.lnk` removed); one Add/Remove entry, DisplayName "Organimizer", DisplayVersion 3.0.2 (same `Uninstall\Organimizer` key). BrandingText footer "PrimeCrew Software". Screenshot `installer-upgrade-3.0.2.png` |

## 3.0.3 Wine smoke (own Xvfb display and Wine prefix)

| Test | Result |
|---|---|
| Silent install (`Organimizer-v3.0.3-setup.exe /S`) into a new prefix | OK: `LICENSE` and `COPYRIGHT` in `%LOCALAPPDATA%\Programs\Organimizer`; installed `LICENSE` is byte-identical to `origin/master:LICENSE`; `COPYRIGHT` matches the repo file |

## 3.0.4 Wine smoke (own Xvfb display and Wine prefix)

| Test | Result |
|---|---|
| `cargo test --no-default-features` (160 tests, including official stacked-file / numbered-extras / date-based Season NN tests, and the type-the-name prompt tests) | OK |
| Wine lookup flow against the local mock TMDB: results picker, **Search for something else...**, type-the-name prompt, Esc leaves the preview unchanged, then the usual This one / OK / Apply path | OK (screenshot `lookup-manual-search.png`) |

## Known limitations of this test run




- Wine only. The installer's "->" arrow in the upgrade banner is drawn with
  "→", which the Wine font shows as a box; Windows renders it (same as in the
  2.0.5 installer).
- Wine renders the window slowly in the virtual display; this is a Wine
  artefact (the same happens with PhotoSorter 2.0.5, and the app itself only
  repaints on input).
- A real TMDB lookup was not tested (no API key); the request format is
  covered by unit tests, an invalid-key request reached TMDB (HTTP 401),
  and the whole lookup flow ran against a local mock server serving
  hand-written fixtures in TMDB's format.
- The exe and setup are unsigned: Windows SmartScreen will warn on first
  start. Check the SHA-256 against `Organimizer-v3.0.5-SHA256SUMS.txt` (earlier sums remain in the 3.0.0, 3.0.1, 3.0.2, 3.0.3 and 3.0.4 SHA256SUMS files).

Screenshots: `docs/v3/screenshots/`.

## 3.0.5 Wine smoke (own Xvfb display and Wine prefix)

Own Xvfb `:96` and prefix `wpfx-305-help`. Shared displays `:1`, `:5`, `:77` and shared prefixes were not touched.

| Check | Result |
|---|---|
| Portable `Organimizer.exe` launch; header shows v3.0.5; Plex Media tab empty state has a **Plex naming conventions (Help)** button | OK |
| F1 (from the Plex tab) opens `HELP.md` in Notepad; status line is `Opened ...HELP.md - see the Plex tab chapter in Contents` | OK |
| HELP.md is ASCII, starts at Organimizer 3.0.5, and contains the Plex tab chapter (movies, eight extras folders and suffixes, TV naming, common mistakes, cheat sheet, official support URLs) | OK (screenshot `help-plex.png`) |
| Help > Plex naming conventions... is in the Help menu | OK (present in the menu; F1 used for the screenshot) |

## 3.0.6 Wine smoke (own Xvfb display and Wine prefix)

Own Xvfb `:96` and prefix `ps306-test/prefix`. Shared displays and prefixes were not touched.

| Check | Result |
|---|---|
| Plex tab starts with an empty Source box; **Recent ⏷** lists the folder from the INI; picking it fills the box | OK |
| Mode shows **Rename only** selected by default; Scan of `TV` (`Lost (2004)\Season 1`, `Specials`) proposes `Lost (2004) - s01e02.mkv` / `- s00e01.mkv` in their own folders; two folder fixes (`Season 01`, `Season 00`) shown unticked | OK (screenshot `plex-rename-only.png`) |
| Ticking one fix changes Apply to "rename 2 files and 1 folders"; switching to Full sort shows Destination *Inside source folder* / *Library folders* and "Mode changed: click Rescan" (Apply disabled) | OK (screenshot `plex-full-sort-destination.png`) |
| Apply / Undo with folder renames on disk | unit tests only (`apply_with_folder_fixes_then_undo_restores_everything`); not run in the GUI |
