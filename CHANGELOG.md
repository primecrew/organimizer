# Changelog

All versions are Windows 10/11 builds (portable exe; from 2.0.5 also an
installer). None of them is code-signed, and so far they have been tested on
Linux and under Wine only, not on a real Windows PC.

## 3.0.0 – PrimeCrew Organimizer (branch `v3-organimizer`, not released yet)
- **Renamed** PhotoSorter → **PrimeCrew Organimizer**: window title,
  header, About, `Organimizer.exe` (version info ProductName "PrimeCrew
  Organimizer"), `Organimizer.ini`, `Organimizer-undo-*.json`, installer,
  Start Menu `PrimeCrew Software\PrimeCrew Organimizer.lnk`, Add/Remove
  Programs "PrimeCrew Organimizer", docs. Repository renamed to
  `primecrew/organimizer`.
- **Settings migration**: on first start `PhotoSorter.ini` (beside the exe
  or in `%APPDATA%\PrimeCrew Software\PhotoSorter`) is imported once with
  all presets; the old file is never modified or deleted. Undo also finds
  `PhotoSorter-undo-*.json` logs.
- **Installer** `Organimizer-3.0.0-setup.exe`: detects an installed
  PhotoSorter (HKCU/HKLM uninstall entry) and treats it as an upgrade with
  the shared banner "Updating existing installation: PhotoSorter v2.0.5 →
  PrimeCrew Organimizer v3.0.0"; installs next to it
  (`…\Programs\Organimizer`, same mode) and then removes only the old
  PhotoSorter program files, its shortcuts and its uninstall entry
  (settings, undo logs and user files are kept). Portable zip
  `Organimizer-3.0.0-win64.zip`.
- **Plex Media tab**: scan a folder, analyze movies / TV episodes / extras /
  subtitles, propose the Plex layout (in place or into Movies / TV library
  folders), per-file confidence (green / amber / red), editable overrides
  (type, title, year, edition, season, episode, last episode, episode
  title, extra type / name / movie-or-show), include checkboxes, filter,
  preview / Apply / Undo with the same never-overwrite and undo-log
  guarantees as the Organize tab.
- **Optional TMDB lookup** (off by default): the user's own key (API Read
  Access Token or v3 API key) stored only in `Organimizer.ini`; matches are
  listed for the user to confirm; TV matches fill the episode title;
  offline otherwise, friendly messages for network errors, 401 and 429.
  HTTPS via `ureq` + `rustls` (feature `net`). Help > About shows the TMDB
  logo and "This product uses the TMDB API but is not endorsed or certified
  by TMDB."; THIRD-PARTY-NOTICES has a TMDB section.
- **HELP.md** rewritten for 3.0 with a step-by-step "Getting a TMDB API
  key" section; Tools > Options links to it next to the key field.
- **File types**: Images & videos (default, unchanged 2.x list), Images,
  Videos, Audio, Documents, Archives, All files, Custom list (Tools >
  Options). Hard protections in every mode: executables, libraries,
  installers, scripts, shortcuts, system files, Organimizer/PhotoSorter
  settings and undo logs, hidden/system files and VCS / recycle-bin /
  System Volume Information folders. Rules can change an extension only
  between image/video types.
- **Folder blacklist** (Tools > Folder Blacklist…): defaults from the
  environment (SystemRoot/windir, ProgramFiles*, ProgramData, profile
  AppData, Default / All Users, Windows.old, Recovery, `$Recycle.Bin`,
  System Volume Information, EFI, drive roots, …); add / edit / remove,
  test a path, Reset to defaults. Case-insensitive, component-wise
  (prefix-safe) matching with `*` and `*:` wildcards. Blocked folders
  cannot be selected ("Folder not allowed") and are skipped in recursion.
- **Favorites**: ☆/★ toggle next to the folder field (Organize and Plex),
  Favorites menu, Manage Favorites window (add, rename, edit path, remove,
  reorder).
- Tests: 130 unit tests (Plex parser cases, blacklist, protected types,
  favorites, settings migration, plus all 2.x tests).

## 2.0.5 (git tag `v2.0.5`, 2026-11-21)
- Help > About: app icon (64x64), version, publisher (PrimeCrew Software),
  short description, portable / unsigned note, license line.
- Main-window header now shows the app icon, name and version.
- New `HELP.md` beside the exe (quick start + section stubs). Help (F1)
  opens it, or `README.txt` if HELP.md is missing.
- License is now GPL-3.0-or-later; `LICENSE` ships in the zip.
- Exe version info: CompanyName PrimeCrew Software, ProductName and
  FileDescription PhotoSorter, version 2.0.5. Release builds remap
  build-machine paths (`scripts/build-release.sh`).
- Wording: "Undo Last Run…" in the File menu (matches the Undo button),
  "date folders" instead of "Month folders" in the preview summary, US
  spelling (color, canceled, recognized), "Help file not found" dialog.
- More hover help: Open Folder, Undo Last Run, folder path field, preset
  Delete, Clear all, Sort into date folders, Date source, If no date found.
- Windows installer `PhotoSorter-2.0.5-setup.exe` (NSIS, modelled on the
  Encode FFS one): per-user (`%LOCALAPPDATA%\Programs\PhotoSorter`) or
  for everyone (Program Files), Start Menu shortcut in the shared
  "PrimeCrew Software" folder, optional desktop shortcut, Add/Remove
  Programs entry (publisher PrimeCrew Software), in-place upgrade without
  duplicate shortcuts. Uninstall removes only the installed files and
  shortcuts (the brand folder only when empty); photos, undo logs,
  settings and presets are never deleted.
- Setup shows clearly whether it updates or installs, in the same look as
  the other PrimeCrew Software installers. Upgrade: the first page and the
  Ready to install page start with "Updating existing installation:
  PhotoSorter v<old> → v<new>", followed by "Installed in: <path>" as plain
  text (no Browse; an update never moves the folder, and a per-user or
  all-users install stays that way). Fresh install: "New installation",
  folder page labelled "Install to:" with Browse. New Ready to install page
  (who, folder, shortcuts). Finish page: "Updated to v<new> in <path>" or
  "Installed v<new> to <path>".
- Installed mode: setup writes `PhotoSorter.installed` next to the exe and
  the app then keeps `PhotoSorter.ini` in
  `%APPDATA%\PrimeCrew Software\PhotoSorter`. A writable INI or a
  `PhotoSorter.portable` file next to the exe keeps portable mode, so the
  zip behaves as before. Undo logs stay in the sorted photo folder.
- Larger default window (1280 x 900); Browse / drop folds step 1 to its
  summary so steps 2 and 3 fit without scrolling.
- Preview counters with 0 are hidden (Files is always shown).
- "Clear all…" rules asks for confirmation.
- Theme button shows Light / Dark text instead of sun / moon glyphs.
- `THIRD-PARTY-NOTICES.txt` (crate licenses and notices, embedded font
  licenses: Ubuntu Font Licence 1.0, SIL OFL 1.1) in the zip and the
  installer.
- Fixed the two clippy warnings in `rename.rs` (regex compiled per Padding
  rule, identity map).
- No change to how files are renamed, moved or undone. "Include subfolders"
  stays off by default.

## 2.0.4 (git tag `v2.0.4`, 2026-10-27)
- "Include subfolders" is now **off** by default on a fresh start (no INI);
  a missing or invalid `include_subfolders` value also means off.
- An existing `PhotoSorter.ini` that saved `include_subfolders = true` keeps
  that choice; untick the box once to switch.

## 2.0.3 (`v2.0.3`, 2026-10-27)
- Remembers window position, size, maximized state, top-panel height,
  preview column widths and open/collapsed sections (`[Window]` in the INI),
  clamped so it never opens off-screen; "Reset window layout" in Options.
- Tools > Options > Appearance: Dark / Light / Follow system theme.
- Folder-name builder: ready-made layouts with a live example, "Build your
  own" controls, and an "Advanced" raw template field kept in sync.
- New `{/}` token for nested folders (e.g. `2024\03 March`); undo removes
  every created level.

## 2.0.2 (`v2.0.2`, 2026-10-27)
- Menu bar: File (New Project Ctrl+N, Open Folder Ctrl+O, Undo Last Apply,
  Exit), Presets (Load, Save, Manage), Tools (Options, Open Settings Folder,
  Open Undo Log Folder), Help (F1 opens README.txt, About placeholder).
- Extension list and Auto date priority moved to Tools > Options; the main
  window shows a one-line summary.
- Manage Presets: view, edit, duplicate, rename, delete; presets can load
  rules, folder options or both (`scope`).
- INI saved via a temp file, previous copy kept as `PhotoSorter.ini.bak`.

## 2.0.1 (`v2.0.1`, 2026-10-27)
- New app icon (orange folder with papers). No functional changes.

## 2.0 (`v2.0`, 2026-10-25)
- Ordered rename-rule list modelled on den4b ReNamer Pro (Insert, Delete,
  Remove, Replace, Rearrange, Extension, Strip, Case, Serialize, Randomize,
  Padding, Clean up, Translit, Reformat date, Regular expressions, plus
  Insert date, Insert folder name, Extension case), run before the folder step.
- Live before/after preview of names and destination folders.
- Video support (MP4/MOV creation date) and a media-only extension allowlist
  (images, camera RAW, video); everything else is ignored.
- Windows name validation (illegal characters, reserved names, trailing
  dots/spaces, path length); same-batch collision handling; undo restores
  names and locations.
- Named presets stored as `[Preset:Name]` sections in `PhotoSorter.ini`.

## 0.1.0 (git tag `v1.0`, 2026-10-25)
- First release: moves images into `Month YYYY` folders inside each image's
  own folder, with date from filename / EXIF / modified / created (Auto
  priority order), preview first, confirmation, JSON undo log + Undo,
  collision-safe ` (1)` suffixes, commented portable INI, dark/light theme.
