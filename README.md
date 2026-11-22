# Organimizer 3.0.3

By PrimeCrew Software. Formerly **PhotoSorter** (2.x; the last PhotoSorter
release is [v2.0.5](https://github.com/primecrew/organimizer/releases/tag/v2.0.5)).

Organimizer is a Windows 10/11 desktop app (Rust + eframe/egui). It ships as a
portable zip (one exe you can run from any folder) and an optional installer,
with two tabs:

- **Organize** – renames files with an ordered, ReNamer Pro-style rule list
  and then, optionally, moves them into date folders such as `March 2024`
  inside each file's own folder (the PhotoSorter pipeline).
- **Plex Media** – analyzes movies, TV episodes, extras and subtitles and
  moves them into the folder layout Plex expects, in place or into your
  library folders.

Everything is shown in a preview first, nothing is ever overwritten or
deleted, and the last run can be undone.

## Features

- **Rename rules** modelled on den4b ReNamer Pro: stack rules in order
  (insert, delete, remove, replace, regex, serialize/number, case, clean up,
  insert date, insert folder name and more), each with on/off, reorder,
  duplicate and delete.
- **Date folders**: `Month YYYY` by default, other ready-made layouts, a
  folder-name builder, an Advanced template field, and nested folders
  (`2024\03 March`). Dates come from filename, EXIF / video metadata,
  modified or created time, in a configurable priority order.
- **Plex Media tab** (3.0): movie / episode / extra / subtitle detection
  (`Title.2010.1080p…`, `S01E02`, `1x02`, Season folders, Specials,
  date-based shows, multi-episode files, editions, `-trailer` and extras
  folders, `Subs` folders and language / forced / SDH tags, samples
  skipped), Plex names such as `Heat (1995)\Heat (1995).mkv` and
  `Show\Season 01\Show - s01e02 - Title.mkv`, a confidence per file
  (green / amber / red), and an editor for type, title, year, edition,
  season, episode, episode title and extra type.
- **Optional TMDB lookup** (off by default): with your own free TMDB key
  (stored only in your INI) Organimizer lists TMDB matches for you to
  confirm; offline otherwise. HTTPS via `ureq` + `rustls`. HELP.md explains
  step by step how to get a key.
- **File types**: Images & videos (default, the 2.x list), Images, Videos,
  Audio, Documents, Archives, All files or a custom list. Programs, scripts,
  shortcuts, system and hidden files are always protected.
- **Folder blacklist** (Tools > Folder Blacklist…): Windows system folders
  by default (from the environment: SystemRoot, Program Files, ProgramData,
  AppData, …, plus drive roots); add / edit / remove / reset. Blocked
  folders cannot be chosen and are skipped while scanning; matching is
  case-insensitive and component-wise (`C:\Windows` ≠ `C:\WindowsStuff`).
- **Favorites**: a ☆/★ toggle next to the folder field, a Favorites menu
  and a Manage Favorites window (rename, edit, reorder, remove).
- **Live preview**, **undo** (JSON undo log written before anything moves),
  **collision safety** (never overwrites; ` (1)` suffixes; Windows name
  rules checked), **presets**, Dark / Light / Follow system theme and
  window memory.
- **Settings** in a commented `Organimizer.ini`: beside the exe for the
  portable zip, `%APPDATA%\PrimeCrew Software\Organimizer` when installed
  (no registry). PhotoSorter's `PhotoSorter.ini` is imported on first start
  and never modified.

**Download:** `Organimizer-v3.0.3-win64.zip` (portable) or
`Organimizer-v3.0.3-setup.exe` (installer: Start Menu › PrimeCrew Software ›
Organimizer, Add/Remove Programs "Organimizer"; it upgrades an installed
PhotoSorter 2.x or Organimizer 3.0.0/3.0.1 and removes leftover program
files, the old "PrimeCrew Organimizer" shortcut and a duplicate Add/Remove
entry). Both contain README.txt, HELP.md, LICENSE, COPYRIGHT and
THIRD-PARTY-NOTICES.txt.

**Status:** the exe is not code-signed, so Windows SmartScreen will probably
warn on first run (More info > Run anyway; compare the SHA-256 published with
each release). It has been tested on Linux and under Wine only, not yet on a
real Windows PC.

End-user docs: `dist/v3.0.3/README.txt` and `dist/v3.0.3/HELP.md` (shipped
as `README.txt` and `HELP.md`, together with `LICENSE`, `COPYRIGHT` and
`dist/v3.0.3/THIRD-PARTY-NOTICES.txt`); older versions under `dist/v2.*`,
v1 docs: `dist/README.txt`. Version history: [CHANGELOG.md](CHANGELOG.md).
Test notes: `docs/v2/TEST-SUMMARY.md`, `docs/v3/TEST-SUMMARY.md`;
screenshots: `docs/v3/screenshots/`.

TMDB: This product uses the TMDB API but is not endorsed or certified by
TMDB.

## Window layout (top to bottom) and why

0. **Header** – the app icon, "Organimizer", the version and a
   one-line description, with the theme toggle and A−/A+ text size on the
   right. **Menu bar** – File (New Project Ctrl+N, Open Folder… Ctrl+O, Undo
   Last Run…, Exit), Presets (Load ▸, Save current as preset…, Manage
   Presets…), Favorites (add/remove current folder, the favorites, Manage
   Favorites…), Tools (Options…: file types, custom list, TMDB key, Auto
   date priority; Folder Blacklist…; Manage Favorites…; Open Settings / Undo
   Log Folder), Help (F1 → HELP.md, or README.txt if HELP.md is missing,
   via `ShellExecuteW`; About… with the TMDB attribution).
   **Tabs** – Organize (steps 1–5 below) | Plex Media (source, destination,
   TMDB status; table with confidence dots; "Correct this file" panel;
   Apply / Undo bar).
1. **Source & filter** – folder with the ☆/★ favorite toggle, include
   subfolders (off by default since 2.0.4), file-type picker with a
   summary + "Options…" button.
2. **Rename rules** – ReNamer-style list: checkbox per rule, ⏶/⏷ reorder,
   duplicate, delete, ✏ inline editor, "➕ Add rule" menu, presets row.
3. **Output folder** – "Sort into date folders" toggle + date source; a
   folder-name builder (ready-made layouts with a live example, "Build your
   own…" Year / Month / Day / order / separator controls, and an "Advanced"
   raw template field with a token legend, kept in sync; inline warnings);
   fallback, ambiguous dates and the Auto priority summary under "More
   options".
4. **Live preview** – Before / After name, current and destination folder,
   date, action and notes (validation warnings); "By destination" and
   "Results / errors" tabs.
5. **Apply + Undo** – always visible bar at the bottom.

The order is the order of the pipeline (pick files → rename → choose folder →
check → commit), so the window reads like the data flows and nothing below a
step can be affected by something further down. Steps 1–3 sit in one
resizable, collapsible, scrollable panel so the preview keeps most of the
height; Apply/Undo is pinned so it is never scrolled away, and Apply is only
enabled when the preview is current and no rule has an error.
Rarely-changed settings (the 26-entry extension list and the Auto priority
editor) live in Tools > Options since 2.0.2 to keep the main window short.

Since 2.0.3 the window remembers its position, size, maximized state, the
height of the top panel, the preview column widths and which sections are
collapsed (`[Window]` in `Organimizer.ini`; saved on exit and ~1.5 s after a
change, clamped on restore so it is never off-screen or tiny). Tools >
Options > Appearance has Dark / Light / Follow system, text size and "Reset
window layout"; the ☀/🌙 icon in the top bar stays as a quick toggle.
Folder templates may contain `{/}` to make nested folders such as
`{YYYY}{/}{MM} {MMMM}` → `2024\03 March`; each level is sanitised on its own
and every created level is recorded in the undo log.

## Include subfolders: off by default (since 2.0.4)

A fresh start (no `Organimizer.ini`) now has "Include subfolders" unticked,
so the preview and Apply only touch the files directly inside the chosen
folder; tick the box to also scan every subfolder (each folder with photos
then gets its own date folders). A missing or invalid `include_subfolders`
value in `[General]` or in a `[Preset:Name]` section also means off.
**Upgrading:** an INI saved by an older version normally contains
`include_subfolders = true` (the old default); that saved choice is kept,
not overridden. To switch to the new default, untick the box (it is saved on
exit), set the line to `false`, delete the line, or delete the INI. Presets
that store an explicit value keep it. File > New Project still keeps the
current "Include subfolders" setting.

## Code layout

| Path | Purpose |
|---|---|
| `src/lib.rs` | core library (no GUI), fully unit tested |
| `src/scan.rs` | folder walk (no symlink following, protected / hidden / blacklisted folders skipped), extension filter |
| `src/filetypes.rs` | file type groups, custom list, protected extensions / names / folders |
| `src/blacklist.rs` | folder blacklist: defaults from the environment, component-wise case-insensitive matching with `*` / `*:` |
| `src/favorites.rs` | favorites list (add / toggle / edit / reorder, INI lines) |
| `src/plex.rs` | Plex analysis (movies, episodes, extras, subtitles, confidence), Plex paths, plan building |
| `src/tmdb.rs` | optional TMDB lookup (key detection, URLs, JSON parsing; HTTPS with `ureq` + `rustls` behind the `net` feature) |
| `src/filename_date.rs` | smart filename date parser |
| `src/date_source.rs` | Filename / EXIF (+ MP4/MOV creation time) / Modified / Created, Auto priority, fallback |
| `src/template.rs` | folder template `{MMMM} {YYYY}` (+ `{/}` nested levels), ready-made layouts, builder ⇄ template mapping, warnings, "already sorted" detection |
| `src/rename.rs` | rename rules (`RuleSpec` data + compiled `Box<dyn RenameRule>`), INI encoding, Windows name validation/sanitising |
| `src/plan.rs` | pipeline `RenameStage` (rules) → `FolderStage` (Month YYYY); collision resolution incl. same-batch renames |
| `src/ops.rs` | apply (rename, case-only rename, copy+verify+delete across volumes), undo log written first, undo |
| `src/settings.rs` | commented `Organimizer.ini` (beside the exe, or `%APPDATA%` when installed: `choose_ini_path`) incl. `[Rules]`, `[Preset:Name]` (+ `scope`), `[Blacklist]`, `[Favorites]`, `[Plex]`; one-time import of `PhotoSorter.ini`; atomic save with `.bak` |
| `src/window_state.rs` | `[Window]` layout values: parse, validate and clamp the restored placement to the monitors' work areas |
| `src/session.rs` | File > New Project reset + confirmation reasons, preset summaries |
| `src/about.rs` | product, publisher (PrimeCrew Software), version and license strings for About and the header |
| `src/desktop.rs` | open HELP.md (or README.txt) / folders with the default app (`ShellExecuteW`, no shell), monitor work areas |
| `src/gui.rs`, `src/gui_v3.rs`, `src/main.rs` | egui front-end (Organize tab; Plex tab, blacklist editor, favorites in `gui_v3.rs`), background workers, debounced live preview |
| `examples/e2e.rs` | headless driver used for end-to-end tests (not shipped) |

With no rule enabled the rename stage is a no-op and behaviour is identical
to v1 (covered by `no_enabled_rules_is_identical_to_v1`).

## Rules (den4b ReNamer names)

Insert, Delete, Remove, Replace, Rearrange, Extension (set to another
image/video extension or detect from file content), Strip, Case, Serialize,
Randomize, Padding, Clean up, Translit, Reformat date, Regular expressions,
plus Insert date (plan / filename / EXIF / modified / created date with a
strftime format), Insert folder name and Extension case. Not implemented:
Pascal Script (runs code), User Input / Mapping (manual name lists).

## Build (cross-compile from Linux)

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64-posix        # linker + windres
cargo test --no-default-features                    # core tests on Linux
cargo build --release --target x86_64-pc-windows-gnu
scripts/build-release.sh   # release: exe (--remap-path-prefix), notices, zip, NSIS setup.exe, SHA256SUMS
# run the core tests as a Windows binary under Wine
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine64 cargo test --no-default-features --target x86_64-pc-windows-gnu
# headless end-to-end run with a preset from an INI
cargo run --no-default-features --example e2e -- <folder> <Organimizer.ini> --preset "Name" [--apply] [--undo]
```

The release build links statically (`+crt-static`, `-static`; see
`scripts/build-release.sh`), so the exe only imports DLLs that ship with
Windows, and remaps build-machine paths so none end up in the exe. The
version resource names PrimeCrew Software as the company.

## Installer

`installer/installer.nsi` (NSIS 3, built by `scripts/build-release.sh`
with `makensis`) follows the shared PrimeCrew installer look (Encode FFS,
Tweakinator): per-user by default (`%LOCALAPPDATA%\Programs\Organimizer`,
no admin rights) or for everyone (`Program Files\Organimizer`), Start Menu
shortcut `PrimeCrew Software\Organimizer.lnk` (shared brand
folder; leftover `PrimeCrew Organimizer.lnk` from 3.0.0/3.0.1 is removed),
optional desktop shortcut, Add/Remove Programs entry
"Organimizer" under `...\Uninstall\Organimizer`, in-place
upgrade with the "Updating existing installation: … v<old> → v<new>"
banner. An installed PhotoSorter 2.x (`...\Uninstall\PhotoSorter`) is
detected as an upgrade ("PhotoSorter v2.0.5 → Organimizer
v3.0.3"); after installing, setup removes the old PhotoSorter program
files, shortcuts and uninstall entry (never settings, undo logs or user
files). It writes `Organimizer.installed` next to the exe;
`settings::choose_ini_path` then uses
`%APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini`. The
uninstaller removes only what setup installed and the brand folder only
when it is empty. `scripts/gen-third-party-notices.py` writes
THIRD-PARTY-NOTICES.txt from `cargo metadata`.

## Changelog

See [CHANGELOG.md](CHANGELOG.md): 0.1.0 (v1.0), 2.0, 2.0.1, 2.0.2, 2.0.3, 2.0.4, 2.0.5, 3.0.0, 3.0.1, 3.0.2, 3.0.3.

## License

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General Public License v3.0 or later. See LICENSE and COPYRIGHT.

The TMDB logo in `assets/tmdb/` is TMDB's approved attribution logo, a
trademark of TMDB, and not covered by the GPL.

