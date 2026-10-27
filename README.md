# PhotoSorter 2.0.4

Portable Windows 10/11 GUI (Rust + eframe/egui) that renames images and
videos with an ordered, ReNamer-style rule list and then (optionally) moves
them into `Month YYYY` subfolders inside each file's own folder — with a live
preview, collision-safe moves and a JSON undo log. End-user docs:
`dist/v2.0.4/README.txt` (shipped as `README.txt` in the zip). v1 docs:
`dist/README.txt`.

## Window layout (top to bottom) and why

0. **Menu bar** – File (New Project Ctrl+N, Open Folder… Ctrl+O, Undo Last
   Apply…, Exit), Presets (Load ▸, Save current as preset…, Manage
   Presets…), Tools (Options…: file types with "Reset to defaults" + Auto
   date priority; Open Settings / Undo Log Folder), Help (F1 → README.txt
   via `ShellExecuteW`, About).
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
| `src/settings.rs` | commented `PhotoSorter.ini` beside the exe incl. `[Rules]` and `[Preset:Name]` (+ `scope`), preset duplicate/rename/validate, atomic save with `.bak` |
| `src/window_state.rs` | `[Window]` layout values: parse, validate and clamp the restored placement to the monitors' work areas |
| `src/session.rs` | File > New Project reset + confirmation reasons, preset summaries |
| `src/desktop.rs` | open README.txt / folders with the default app (`ShellExecuteW`, no shell), monitor work areas |
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

## Build (from Linux)

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64-posix        # linker + windres
cargo test --no-default-features                    # core tests on Linux
cargo build --release --target x86_64-pc-windows-gnu
# run the core tests as a Windows binary under Wine
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine64 cargo test --no-default-features --target x86_64-pc-windows-gnu
# headless end-to-end run with a preset from an INI
cargo run --no-default-features --example e2e -- <folder> <PhotoSorter.ini> --preset "Name" [--apply] [--undo]
```

`.cargo/config.toml` links statically (`+crt-static`, `-static`), so the exe
only imports DLLs that ship with Windows.
