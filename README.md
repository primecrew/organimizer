# PhotoSorter

Portable Windows 10/11 GUI (Rust + eframe/egui) that moves images into
`Month YYYY` subfolders inside each image's own folder, with preview,
collision-safe moves and undo. End-user docs: `dist/README.txt`.

## Layout

| Path | Purpose |
|---|---|
| `src/lib.rs` | core library (no GUI), fully unit tested |
| `src/scan.rs` | folder walk (no symlink/junction following), extension list |
| `src/filename_date.rs` | smart filename date parser (+ many sample-name tests) |
| `src/date_source.rs` | Filename / EXIF / Modified / Created, Auto priority, fallback |
| `src/template.rs` | folder template `{MMMM} {YYYY}` etc., "already sorted" detection |
| `src/plan.rs` | Plan/Stage pipeline: `RenameStage` (placeholder for Renamer-style rules) → `FolderStage`; collision resolution |
| `src/ops.rs` | apply (rename, copy+verify+delete across volumes), JSON undo log, undo |
| `src/settings.rs` | commented `PhotoSorter.ini` beside the exe (lenient parser, template writer) |
| `src/gui.rs`, `src/main.rs` | egui front-end, background worker thread |

Extension points: add rename rules as a `Vec<Box<dyn RenameRule>>` inside
`RenameStage` (it already runs before `FolderStage` and sets
`PlanItem::new_name`); presets go into `[Preset:Name]` INI sections
(`settings::parse_ini` already returns all sections).

## Build (from Linux)

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64-posix        # linker + windres
cargo test --no-default-features                    # core tests on Linux
cargo build --release --target x86_64-pc-windows-gnu
# optional: run the core tests as a Windows binary under Wine
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine64 cargo test --no-default-features --target x86_64-pc-windows-gnu
```

`.cargo/config.toml` links statically (`+crt-static`, `-static`), so the exe
only imports DLLs that ship with Windows.
