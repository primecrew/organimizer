# PhotoSorter 2.0.5

By PrimeCrew Software.

PhotoSorter is a portable Windows 10/11 desktop app (Rust + eframe/egui, one
exe, no installer) for tidying photo and video folders. It renames files with
an ordered, ReNamer Pro-style rule list and then, optionally, moves them into
date folders such as `March 2024` inside each file's own folder. Everything is
shown in a live preview first, and the last run can be undone.

## Features

- **Rename rules** modelled on den4b ReNamer Pro: stack rules in order
  (insert, delete, remove, replace, regex, serialize/number, case, clean up,
  insert date, insert folder name and more), each with on/off, reorder,
  duplicate and delete.
- **Date folders**: `Month YYYY` by default, other ready-made layouts, a
  folder-name builder, an Advanced template field, and nested folders
  (`2024\03 March`). Dates come from filename, EXIF / video metadata,
  modified or created time, in a configurable priority order.
- **Live preview** of before/after names and destination folders; nothing
  changes until you click Apply and confirm.
- **Undo**: a JSON undo log is written before anything moves; Undo restores
  the original names and locations and removes created folders.
- **Collision safety**: never overwrites, never deletes; clashes (including
  ones created by renames in the same batch) get ` (1)` suffixes; Windows
  name rules (illegal characters, reserved names, trailing dots/spaces, path
  length) are checked and flagged.
- **Images and videos only**: png, jpg, jpeg, gif, bmp, webp, tif, tiff,
  heic, heif, camera RAW (cr2, nef, arw, dng) and mp4, mov, m4v, avi, mkv,
  wmv, webm, mpg, mpeg, mts, m2ts, 3gp. Other files (txt, pdf, xmp/json
  sidecars...) are never touched. The list is editable in Tools > Options.
- **Presets** for rule chains and folder options (load, save, manage,
  duplicate, rename, delete).
- **Menus and Options**: File / Presets / Tools / Help; theme (Dark, Light,
  Follow system), file types and date priority live in Tools > Options.
  Help (F1) opens `HELP.md` beside the exe (falls back to `README.txt`);
  Help > About shows the icon, version, publisher and build notes.
- **Settings** in a commented `PhotoSorter.ini`: beside the exe for the
  portable zip, `%APPDATA%\PrimeCrew Software\PhotoSorter` when installed
  (no registry), including **window memory** (position, size, column
  widths, panel height, open sections).

**Download:** `PhotoSorter-2.0.5-win64.zip` (portable: settings in
`PhotoSorter.ini` next to the exe) or `PhotoSorter-2.0.5-setup.exe`
(installer: Start Menu › PrimeCrew Software › PhotoSorter, Add/Remove
Programs, settings in `%APPDATA%\PrimeCrew Software\PhotoSorter`). Both
contain README.txt, HELP.md, LICENSE and THIRD-PARTY-NOTICES.txt.

**Status:** the exe is not code-signed, so Windows SmartScreen will probably
warn on first run (More info > Run anyway; compare the SHA-256 published with
each release). It has been tested on Linux and under Wine only, not yet on a
real Windows PC.

End-user docs: `dist/v2.0.5/README.txt` and `dist/v2.0.5/HELP.md` (shipped
as `README.txt` and `HELP.md`, together with `LICENSE` and
`dist/v2.0.5/THIRD-PARTY-NOTICES.txt`);
v1 docs: `dist/README.txt`. Version history: [CHANGELOG.md](CHANGELOG.md).
Test notes: `docs/v2/TEST-SUMMARY.md`.

## Window layout (top to bottom) and why

0. **Header** – the app icon, "PhotoSorter", the version and a one-line
   description, with the theme toggle and A−/A+ text size on the right.
   **Menu bar** – File (New Project Ctrl+N, Open Folder… Ctrl+O, Undo Last
   Run…, Exit), Presets (Load ▸, Save current as preset…, Manage
   Presets…), Tools (Options…: file types with "Reset to defaults" + Auto
   date priority; Open Settings / Undo Log Folder), Help (F1 → HELP.md, or
   README.txt if HELP.md is missing, via `ShellExecuteW`; About…).
1. **Source & filter** – folder, include subfolders (off by default since
   2.0.4), one-line file-type
   summary ("File types: 26 image & video types") + "Options…" button.
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
collapsed (`[Window]` in `PhotoSorter.ini`; saved on exit and ~1.5 s after a
change, clamped on restore so it is never off-screen or tiny). Tools >
Options > Appearance has Dark / Light / Follow system, text size and "Reset
window layout"; the ☀/🌙 icon in the top bar stays as a quick toggle.
Folder templates may contain `{/}` to make nested folders such as
`{YYYY}{/}{MM} {MMMM}` → `2024\03 March`; each level is sanitised on its own
and every created level is recorded in the undo log.

## Include subfolders: off by default (since 2.0.4)

A fresh start (no `PhotoSorter.ini`) now has "Include subfolders" unticked,
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
| `src/scan.rs` | folder walk (no symlink following), media-only extension allowlist |
| `src/filename_date.rs` | smart filename date parser |
| `src/date_source.rs` | Filename / EXIF (+ MP4/MOV creation time) / Modified / Created, Auto priority, fallback |
| `src/template.rs` | folder template `{MMMM} {YYYY}` (+ `{/}` nested levels), ready-made layouts, builder ⇄ template mapping, warnings, "already sorted" detection |
| `src/rename.rs` | rename rules (`RuleSpec` data + compiled `Box<dyn RenameRule>`), INI encoding, Windows name validation/sanitising |
| `src/plan.rs` | pipeline `RenameStage` (rules) → `FolderStage` (Month YYYY); collision resolution incl. same-batch renames |
| `src/ops.rs` | apply (rename, case-only rename, copy+verify+delete across volumes), undo log written first, undo |
| `src/settings.rs` | commented `PhotoSorter.ini` (beside the exe, or `%APPDATA%` when installed: `choose_ini_path`) incl. `[Rules]` and `[Preset:Name]` (+ `scope`), preset duplicate/rename/validate, atomic save with `.bak` |
| `src/window_state.rs` | `[Window]` layout values: parse, validate and clamp the restored placement to the monitors' work areas |
| `src/session.rs` | File > New Project reset + confirmation reasons, preset summaries |
| `src/about.rs` | product, publisher (PrimeCrew Software), version and license strings for About and the header |
| `src/desktop.rs` | open HELP.md (or README.txt) / folders with the default app (`ShellExecuteW`, no shell), monitor work areas |
| `src/gui.rs`, `src/main.rs` | egui front-end, background worker, debounced live preview |
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
cargo run --no-default-features --example e2e -- <folder> <PhotoSorter.ini> --preset "Name" [--apply] [--undo]
```

The release build links statically (`+crt-static`, `-static`; see
`scripts/build-release.sh`), so the exe only imports DLLs that ship with
Windows, and remaps build-machine paths so none end up in the exe. The
version resource names PrimeCrew Software as the company.

## Installer

`installer/installer.nsi` (NSIS 3, built by `scripts/build-release.sh`
with `makensis`) follows the Encode FFS installer: per-user by default
(`%LOCALAPPDATA%\Programs\PhotoSorter`, no admin rights) or for everyone
(`Program Files\PhotoSorter`), Start Menu shortcut
`PrimeCrew Software\PhotoSorter.lnk` (shared brand folder), optional
desktop shortcut, Add/Remove Programs entry under
`...\Uninstall\PhotoSorter`, in-place upgrade. It writes
`PhotoSorter.installed` next to the exe; `settings::choose_ini_path` then
uses `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`. The
uninstaller removes only what setup installed and the brand folder only
when it is empty. `scripts/gen-third-party-notices.py` writes
THIRD-PARTY-NOTICES.txt from `cargo metadata`.

## Changelog

See [CHANGELOG.md](CHANGELOG.md): 0.1.0 (v1.0), 2.0, 2.0.1, 2.0.2, 2.0.3, 2.0.4, 2.0.5.

## License

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General Public License v3.0 or later. See LICENSE.

