# PhotoSorter Help

PhotoSorter 2.0.5 by PrimeCrew Software. Renames photos and videos with
ordered rules and sorts them into date folders such as `March 2024`.

> This help file is a first draft. Sections marked *(coming soon)* will be
> filled in by a later version. The full reference for now is `README.txt`
> next to `PhotoSorter.exe`.

## Quick start

1. **Choose a folder.** Click **Browse…** (or File > Open Folder…, Ctrl+O),
   or drag a folder onto the window. "Include subfolders" is off by
   default, so only the files directly in that folder are used.
2. **Add rename rules (optional).** In step 2 click **➕ Add rule**, pick a
   rule (for example *Insert date* or *Replace*) and edit it. Rules run top
   to bottom on the name without its extension. With no rules, names are
   kept.
3. **Choose the output.** In step 3 leave **Sort into date folders** on to
   move each file into a folder like `March 2024` next to it, or turn it
   off to only rename. Pick the date source (Auto is a good start).
4. **Check the preview.** Step 4 shows every file's old and new name and
   its destination folder. Nothing on disk has changed yet.
5. **Apply.** Click **✔ Apply** and confirm. An undo log is written first.
6. **Undo if needed.** **↩ Undo last run** (or File > Undo Last Run…)
   puts every file back under its original name and folder.

## Portable or installed

- **Portable zip:** settings are kept in `PhotoSorter.ini` next to
  `PhotoSorter.exe`.
- **Installed with setup:** settings are kept in
  `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`; the Start
  Menu shortcut is in the *PrimeCrew Software* folder.
- Either way, undo logs are written into the photo folder you sorted.

## Safety

- Only image and video files are touched (the list is in Tools > Options);
  other files such as `.txt`, `.pdf` or sidecar files are never changed.
- Nothing is overwritten or deleted. Name clashes get ` (1)`, ` (2)`, …
- The preview always comes first, and every Apply can be undone.

## The window *(coming soon)*

## Rename rules *(coming soon)*

## Date folders and the folder name builder *(coming soon)*

## Date sources (filename, EXIF / video, modified, created) *(coming soon)*

## Presets *(coming soon)*

## Options and settings (PhotoSorter.ini) *(coming soon)*

## Undo and the undo log *(coming soon)*

## Troubleshooting *(coming soon)*

- Windows SmartScreen: the exe is not code-signed, so Windows may show
  "Windows protected your PC" on first run. Click **More info > Run
  anyway** after checking the SHA-256 published with the release.

## Keyboard shortcuts

| Key    | Action        |
|--------|---------------|
| Ctrl+N | New Project   |
| Ctrl+O | Open Folder…  |
| F1     | This help     |

## About and license

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General
Public License v3.0 or later. See `LICENSE` next to `PhotoSorter.exe`.
