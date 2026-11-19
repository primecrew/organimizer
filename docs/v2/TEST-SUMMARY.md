# PhotoSorter 2.0 – test summary (2026-10-25, America/Los_Angeles)

## Unit / integration tests
* `cargo test --no-default-features` (Linux, rustc 1.99.1): **79 passed, 0 failed**
  (v1 baseline: 33). By module: rename 29, ops 15, filename_date 8, settings 8,
  date_source 6, plan 6, template 4, scan 3.
* Same suite cross-compiled to `x86_64-pc-windows-gnu` and run under **Wine**
  (`wine64`, case-insensitive file-name semantics): **79 passed, 0 failed**.
* One v1 assertion changed on purpose: `parse_extensions("")` now returns the
  26 v2 defaults (images + RAW + video) instead of 9; a separate check keeps
  the 9 v1 defaults (`V1_DEFAULT_EXTENSIONS`).

## Sample data (downloaded on the Linux build machine)

Note: absolute test paths in the recorded logs were replaced with the generic
`C:\PhotoSorterTest\...`; screenshots show the Wine test folders (`Z:\...`).

* 6 × picsum.photos JPEGs, 5 × Wikimedia Commons JPEGs with EXIF
  DateTimeOriginal (2005–2010, Canon PowerShot A520 category), 2 small MP4s
  (Big Buck Bunny 10 s from test-videos.co.uk with a QuickTime CreateDate set
  to 2023-07-04 via exiftool; samplelib 5 s with no date).
* `make_tree.sh` builds a messy nested tree (24 files): `IMG_20240315_142530.jpg`,
  `Screenshot 2024-03-15 at 10.22.01.png`, `photo (1).JPG`, `photo (2).JPG`,
  `photo.JPG`, `été à Paris – copy.jpg`, `wallpaper   final__v2..jpg`, a PNG
  saved as `.jpg`, `.MP4`/`.mp4` videos, an already-sorted `March 2024` folder,
  plus 7 non-media files (txt, pdf, `.xmp` and `.json` sidecars) that must stay
  untouched.

## End-to-end, core (examples/e2e.rs), Linux and Wine
Presets in `e2e-core-linux/test.ini`:
* **Camera roll** (Month YYYY on): Extension detect → Insert date (whole name)
  → Insert folder name → Serialize per folder → [Case UPPER, disabled] →
  Extension case lower. 17 media files renamed, 16 moved into 10 new folders,
  1 renamed in place (already in the right month folder), disguised PNG
  fixed to `.png` with a note.
* **Clean names** (Month YYYY off, rename only): Regular expressions →
  Reformat date → Replace → Clean up → Translit → Title case. Produces 2
  same-batch clashes (` (1)` suffix), no folder created.
For both presets on Linux AND under Wine: apply had 0 errors; the multiset of
SHA-256 file hashes was identical after apply (nothing lost/overwritten);
all 7 non-media files stayed at their paths; undo restored 17/17 and the
tree + every file hash matched the original exactly (only
`PhotoSorter-undo-*.undone.json` remains). Under Wine the clash handling is
case-insensitive (`photo.JPG` → `Photo.JPG` is a case-only rename;
`photo (1).JPG` → `Photo (1).JPG`), on Linux case-sensitive – both correct.

## End-to-end, GUI (PhotoSorter.exe release build under Wine + Xvfb + xfwm4)
Driven with xdotool: start with the Camera roll chain from the INI → collapse
section 1 → open the Serialize editor → enable the disabled Case rule (live
preview switched to UPPER within the debounce) → Apply → confirm → 17 renamed
/ moved, 0 errors → Undo → confirm → 17 restored, 10 empty folders removed →
tree and hashes identical to the original → save a preset "GUI test" → close
(INI rewritten from the template with `[Rules]` and 3 `[Preset:...]`
sections) → restart → load preset "Clean names" (rename-only preview with
amber clash suffixes). Screenshots in `screenshots/`, trees in
`e2e-gui-wine/`.

## v2.0.2 (menu bar, Tools > Options, Manage Presets)
* `cargo test --no-default-features`: **89 passed, 0 failed** on Linux and
  as a Windows binary under Wine (79 earlier tests unchanged + 10 new:
  `extension_summary_counts_effective_types`,
  `options_extension_edit_persists_and_applies_to_scan`,
  `preset_duplicate_rename_delete_and_uniqueness`,
  `preset_scope_roundtrip_apply_and_old_inis`,
  `save_is_atomic_and_keeps_a_backup`, 4 × `session::` New Project /
  preset-summary tests, `readme_is_found_only_next_to_exe`).
* Backwards compatibility: the v2.0.1 core (`git worktree` at tag v2.0.1,
  `examples/e2e.rs`) loads an INI written by 2.0.2 (with `scope = …`
  preset keys) and builds the same preview; 2.0/2.0.1 presets without
  `scope` load as "both" (unit test).
* GUI under Wine 10 + Xvfb + xfwm4, driven with xdotool (screenshots
  `screenshots/v2.0.2-*.png`): main window shows "File types: 26 image &
  video types (change in Tools > Options)" and a one-line Auto priority;
  Tools > Options: typed `jpg, mp4, txt` → "txt" listed as ignored → Save →
  `extensions = jpg, mp4, txt` in PhotoSorter.ini and the preview dropped
  from 4 to 2 files (png/mov no longer scanned); "Reset to defaults"
  restores the 26-type list. Manage Presets: Duplicate ("Camera roll
  copy"), Rename rejected `Trip]bad`, accepted `Trip 2026`, Loads → "Rename
  rules only" (`scope = rename` written), Edit → delete a rule → Save
  changes (preset now 1 rule), Delete with confirmation; Save current as
  preset (empty name refused, existing name shows "Replaces…").
  PhotoSorter.ini.bak created on save. Apply (2 files) → Ctrl+N showed the
  "last Apply not undone" warning → New Project: file tree + SHA-256 of
  every file and the INI unchanged, undo log kept → re-entered the folder →
  File > Undo Last Apply restored everything → File > Exit saved the INI
  (rules cleared, presets/extensions kept). F1 opened README.txt in Wine
  Notepad via ShellExecuteW; with README.txt missing the "not found" dialog
  appeared and "Open program folder" opened Wine's Explorer. About dialog
  shows 2.0.2 / unsigned build.

## v2.0.3 (window layout memory, theme in Options, folder name builder)
* `cargo test --no-default-features`: **98 passed, 0 failed** on Linux and
  as a Windows binary under Wine (`wine64`); the 89 earlier tests are
  unchanged + 9 new: `template::nested_subfolder_parts_are_safe`,
  `template::builder_template_mapping_and_round_trips`,
  `template::builder_rejects_custom_templates_and_invalid_input`,
  `ops::nested_year_month_template_apply_rerun_and_undo`,
  `settings::window_layout_and_theme_roundtrip_and_old_inis`, and 4 ×
  `window_state::` (parse/validate column widths and numbers, clamp to the
  monitor work areas incl. off-screen / partly off / tiny / huge / no
  monitor / multi-monitor, remembered rect while maximized/minimized).
* GUI under Wine 10 + Xvfb (1920×1080, one monitor) + xfwm4, release
  exe, driven with xdotool (`screenshots/v2.0.3-*.png`):
  * Folder name: default `{MMMM} {YYYY}` shown as "March 2024 (default)"
    with a live example; layout dropdown (9 layouts); Build your own with a
    Custom separator ` - ` (valid) and `a/b` (red hint, not applied);
    Advanced raw template with legend; invalid template `Pics {YYYY}:{MM}/{FOO}`
    shows "Custom template" + the inline warnings (unknown token, `:` `/`
    replaced with `_`, use `{/}`); nested `{YYYY}{/}{MM} {MMMM}` example
    `2024 \ 03 March`; "More options" collapses fallback / ambiguous /
    Auto priority.
  * Sort run with `{YYYY}{/}{MM} {MMMM}` on the 24-file sample tree:
    preview 17 moved → Apply → 17 moved, 0 errors, multiset of file
    SHA-256 identical → Refresh: 17 "already sorted" → Undo: 17 restored,
    21 empty folders removed (year + month levels), tree and every hash
    identical to the original, log renamed `….undone.json`
    (`e2e-gui-wine-v2.0.3/`).
  * Tools > Options > Appearance: Dark / Light / Follow system apply live
    (Follow system resolved to Light under Wine) and are written to the
    INI without pressing Save; "Reset window layout" → 1200×800 centred,
    default panel height / column widths / sections, layout keys cleared.
  * Restart: moved/resized to 1480×1020, widened two columns, collapsed
    "Rename rules", opened "More options" + Advanced → quit → start: same
    position and size (`geo-before/after.txt`), same panel height and
    column widths (`window-ini-*.txt` identical, no growth on restart),
    same open/collapsed sections.
  * Clamp: x = 5000 with 300×200 → default size centred; (1500, 900) at
    1300×850 → moved fully on-screen; `maximized = true` → starts maximized
    and keeps the normal size in the INI; first start centred.
  * Older INIs (2.0.2, no new keys) load unchanged (unit test).
* Known minor issue: with a nested template the "New folders" count in
  the preview / confirm dialog counts only the innermost folders (11 shown
  vs 21 created and removed on undo).

## v2.0.4 ("Include subfolders" off by default)
* Change: `Settings::default().include_subfolders` is now `false` (was
  `true`); INI template comment says "Default: false"; GUI hover text
  explains off/on. A missing / empty / invalid value in `[General]` or in a
  `[Preset:Name]` section → false; an explicit `true` (e.g. saved by an
  older version) is kept. File > New Project still keeps the user's value.
* `cargo test --no-default-features`: **103 passed, 0 failed** on Linux and
  as a Windows binary under Wine (`wine64`). 98 earlier tests, of which 3
  updated: `settings::lenient_parsing` (invalid → default is now false),
  `settings::v1_ini_loads_unchanged_with_no_rules` (now also asserts the
  saved `include_subfolders = true` is kept),
  `session::new_project_resets_session_and_keeps_preferences` (uses a
  non-default `true` so "kept" is really tested). 5 new:
  `settings::fresh_default_include_subfolders_is_off` (default, template
  text, fresh INI created with `false`),
  `settings::include_subfolders_explicit_true_preserved` (true/yes/1/on +
  roundtrip), `settings::include_subfolders_missing_or_invalid_is_off`,
  `settings::preset_include_subfolders_missing_off_explicit_kept`,
  `scan::default_settings_scan_only_top_folder` (default settings list only
  the 2 top-level files of a 3-level tree, 1 dir scanned; ticked → all 5).
* GUI under Wine 10 + Xvfb (1920×1080) + xfwm4, release exe, **no INI
  present**, driven with xdotool, on the 24-file sample tree
  (`make_tree.sh`, `e2e-gui-wine-v2.0.4/tree-sample.txt`):
  * Start: INI created with `include_subfolders = false`; checkbox
    unticked. Typed the folder → preview "5 files in 1 folders", only the
    top-level files (current folder `.`)
    (`screenshots/v2.0.4-fresh-start-subfolders-off.png`).
  * Ticked "Include subfolders" → "17 files in 10 folders" incl.
    `Already sorted`, `Downloads`, `Holiday 2023\Videos`, `Phone\DCIM\Camera`
    (`screenshots/v2.0.4-subfolders-ticked.png`).
  * Exit → INI has `include_subfolders = true`; restart → checkbox still
    ticked and 17 files (saved choice honoured). Nothing was applied; the
    tree was unchanged.

## Not verified
Everything above ran on Linux or under Wine 10.0 only, never on real Windows
10/11 (NTFS, SmartScreen, Defender, high-DPI, OneDrive folders, network
drives and >260-char paths were not exercised). For 2.0.3 in particular:
the multi-monitor / per-monitor-DPI clamp (Win32 EnumDisplayMonitors +
GetDpiForMonitor) is unit-tested with synthetic monitor lists and was only
run on Wine's single virtual monitor; "Follow system" was only seen with
Wine's default (light) setting, not with a real Windows dark-mode switch;
window decorations sizes are Wine/xfwm4's, not Windows 11's.

## v2.0.5 (Help > About, header icon, HELP.md, GPL-3.0-or-later)
* `cargo test --no-default-features`: **105 passed, 0 failed** on Linux and
  as a Windows binary under Wine (103 earlier + 2 new: `find_help` prefers
  HELP.md and falls back to README.txt; product / publisher / license
  strings, `build.rs` version-resource literals and the bundled GPL text
  are consistent).
* Release exe built with `scripts/build-release.sh` (`--remap-path-prefix`);
  `strings` finds no build-machine paths. Version resource: CompanyName
  PrimeCrew Software, ProductName / FileDescription PhotoSorter, version
  2.0.5; icon resources present (RT_ICON ×7, RT_GROUP_ICON).
* GUI under Wine + Xvfb + xfwm4: header icon + name + version, Help >
  About (64x64 icon, version, publisher, notes, "Licensed under GPL v3"),
  Help (F1) from About opened HELP.md in Notepad, title-bar icon shown.
  Screenshots: `screenshots/v2.0.5-main-window.png`,
  `screenshots/v2.0.5-about.png`, `screenshots/v2.0.5-titlebar-icon.png`.
* Not checked under Wine: which app real Windows uses for `.md` (Notepad
  is the fallback when none is registered), Explorer's file-properties
  Details tab.

## v2.0.5 amendment (installer, installed-mode settings, UI polish)

- Unit tests: 108 pass on Linux (`cargo test --no-default-features`); the
  Windows test binary passes 107 under Wine (one test is Unix-only).
  `cargo clippy --release --target x86_64-pc-windows-gnu`: 0 warnings.
- `scripts/build-release.sh`: no build-machine paths in `PhotoSorter.exe` or
  the setup exe (`strings` check).
- Installer (Wine, separate test prefix, per-user install):
  - Fresh install: files in `%LOCALAPPDATA%\Programs\PhotoSorter`;
    `Start Menu\Programs\PrimeCrew Software\PhotoSorter.lnk` created beside a
    pre-existing fake `Encode FFS.lnk`; HKCU uninstall key with
    Publisher `PrimeCrew Software`, DisplayName, DisplayVersion, DisplayIcon.
  - Installed mode: settings created in
    `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`; nothing
    written beside the exe. Preview, apply (5 files into 4 folders) and undo
    (5 restored, 4 empty folders removed); file hashes identical to before.
  - Setup while PhotoSorter is running: refused (silent exit code 1618),
    installed exe unchanged.
  - Reinstall over the top (skips the who/where pages); two silent
    reinstalls with the desktop shortcut:
    exactly one brand `.lnk` and one desktop `.lnk`.
  - Uninstall: install folder, `PhotoSorter.lnk`, desktop shortcut and
    registry key removed; `Encode FFS.lnk` and the brand folder kept;
    APPDATA settings and photos untouched. With no other shortcut in the
    brand folder, the folder is removed.
- Installer, shared PrimeCrew setup look (Wine, per-user and all-users):
  - Fresh install: welcome title "New installation"; folder page labelled
    "Install to:" (editable, Browse); Ready page with a "New installation"
    band and "Install to: <path>"; finish page "Installed v2.0.5 to <path>".
  - Second run over it, with DisplayVersion set to 2.0.4 in the Wine
    registry to fake an older install: first page title "Updating existing
    installation: PhotoSorter v2.0.4 → v2.0.5", directly followed by
    "Installed in: <path>" (text only); who/where pages skipped; Ready page
    repeats the banner and read-only path; finish page "Updated to v2.0.5
    in <path>"; DisplayVersion back to 2.0.5, still one Start Menu `.lnk`.
  - All-users: silent `/S /ALLUSERS` install to `C:\Program Files\PhotoSorter`
    (HKLM key), then a normal run of setup: detected from HKLM, Ready page
    shows "Installed in: C:\Program Files\PhotoSorter" and "Everyone on this
    PC" (canceled there, then uninstalled with `/S /ALLUSERS`).
  - Under Wine the → glyph comes from a fallback font and looks small; on
    Windows the dialog font is expected to draw it normally (not checked).
  - Screenshots: `screenshots/v2.0.5-setup-welcome.png`,
    `v2.0.5-setup-new-install-to.png`, `v2.0.5-setup-new-ready.png`,
    `v2.0.5-setup-new-finish.png`, `v2.0.5-setup-upgrade.png`,
    `v2.0.5-setup-upgrade-ready.png`, `v2.0.5-setup-upgrade-finish.png`,
    `v2.0.5-setup-upgrade-ready-allusers.png`.
- Screenshots: `screenshots/v2.0.5-main-window.png` (1280x900 default),
  `screenshots/v2.0.5-about.png`, `screenshots/v2.0.5-clear-all-confirm.png`,
  `screenshots/v2.0.5-setup-welcome.png`,
  `screenshots/v2.0.5-setup-components.png`.
- Not covered (Wine only): SmartScreen, all-users install to Program Files
  with UAC, the Windows "Installed apps" UI.
