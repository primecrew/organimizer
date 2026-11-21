# PrimeCrew Organimizer Help

PrimeCrew Organimizer 3.0.0 by PrimeCrew Software (formerly **PhotoSorter**).
It renames files with ordered rules and sorts them into date folders such as
`March 2024` (the **Organize** tab), and it organizes movies and TV episodes
into the folder layout Plex expects (the **Plex Media** tab).

The full reference is `README.txt` next to `Organimizer.exe`.

Contents: Quick start (Organize) · Quick start (Plex Media) · File types and
protection · Folder blacklist · Favorites · Getting a TMDB API key ·
Portable or installed · Coming from PhotoSorter · Safety · Troubleshooting ·
Keyboard shortcuts · About and license

## Quick start: Organize

1. **Choose a folder.** Click **Browse…** (or File > Open Folder…, Ctrl+O),
   drag a folder onto the window, or pick one from the **Favorites** menu.
   "Include subfolders" is off by default.
2. **Choose the file types.** Next to "File types" pick *Images & videos*
   (the default, same as PhotoSorter), *Images*, *Videos*, *Audio*,
   *Documents*, *Archives*, *All files* or *Custom list* (Tools > Options).
3. **Add rename rules (optional).** In step 2 click **➕ Add rule**. Rules
   run top to bottom on the name without its extension.
4. **Choose the output.** In step 3 leave **Sort into date folders** on to
   move each file into a folder like `March 2024` next to it, or turn it off
   to only rename.
5. **Check the preview**, then click **✔ Apply** and confirm. An undo log is
   written first; **↩ Undo last run** puts everything back.

## Quick start: Plex Media

1. Click the **Plex Media** tab at the top.
2. **Source:** choose the folder with your movies or episodes (downloads,
   rips, an old library) and click **🔍 Scan**. Nothing is changed.
3. **Destination:** *In place* builds the Plex folders inside the scanned
   folder. *Library folders* moves movies into your Movies library folder and
   episodes into your TV shows folder.
4. **Check the proposals.** Every video gets a type (Movie, TV episode,
   Extra, Skip) and a new location, for example
   `Heat (1995)\Heat (1995).mkv` or
   `Breaking Bad\Season 01\Breaking Bad - s01e02.mkv`. The colored dot shows
   how sure Organimizer is: **green** = high, **amber** = medium, **red** =
   low, please check. "Only files that need a look" hides the sure ones.
5. **Correct anything** by clicking the file name: the panel on the right
   lets you change the type, title, year, edition, season, episode, episode
   title, extra type and whether an extra belongs to a movie or a show.
   "Use for all files of this title" copies title and year to a whole
   season. Untick a row to leave that file alone.
6. Subtitles (`.srt`, `.ass`, `.sub`/`.idx`, `.vtt`, …) next to a video or in
   a `Subs` folder are renamed with their video, keeping language tags such
   as `.en`, `.forced` or `.sdh`. Sample clips are skipped. Extras
   (trailers, featurettes, deleted scenes, …) go into the Plex extras
   folders (`Trailers`, `Featurettes`, …).
7. Click **✔ Apply** and confirm. Existing files are never overwritten (a
   taken name gets ` (1)`), and **↩ Undo last run** moves everything back.
   Old, now empty folders are left in place (nothing is ever deleted).

Optional: turn on the **TMDB lookup** (see the next sections) and click
**🔎 Look up on TMDB** in the right-hand panel to confirm a title against
The Movie Database. You always choose the match yourself.

## File types and protection

- *Images & videos* uses the list in Tools > Options (image, camera RAW and
  video types only).
- *Custom list*: type extensions in Tools > Options > Custom file types,
  for example `pdf, docx, mp3`.
- *All files*: every file that has an extension.
- **Always protected, in every mode:** programs, libraries, drivers,
  installers, scripts and shortcuts (`exe dll sys msi bat cmd ps1 vbs js
  lnk url reg inf ini` and similar), system files (`desktop.ini`,
  `thumbs.db`, `pagefile.sys`, …), Organimizer's own settings and undo logs,
  hidden or system files, and folders such as `.git`, `$Recycle.Bin` and
  `System Volume Information`. Such files are never renamed or moved, and a
  rule can never change a file's extension to another type.

## Folder blacklist

**Tools > Folder Blacklist…** lists folders that can never be chosen and
are skipped when subfolders are scanned (both tabs). The built-in list is
worked out from this PC's Windows folders (Windows, Program Files,
ProgramData, the AppData folders, Recovery, `$Recycle.Bin`, drive roots
such as `C:\` …). Add, edit or remove entries, test a folder, or click
**Reset to defaults**. Matching ignores letter case and works on whole
folder names: `C:\Windows` blocks `C:\Windows\Temp` but not
`C:\WindowsStuff`. `*` stands for one folder name
(`C:\Users\*\AppData`), `*:` for any drive (`*:\$Recycle.Bin`).
Choosing a blocked folder shows "Folder not allowed" and selects nothing.

## Favorites

- Click the **☆** next to a folder field to add it to Favorites; it turns
  into a filled **★**. Click again to remove it.
- The **Favorites** menu jumps to a favorite (in the tab you are on).
- **Favorites > Manage Favorites…** (also in Tools): rename, change the
  folder, add, remove and reorder (Up / Down). Changes are saved at once.

## Getting a TMDB API key

The TMDB lookup on the Plex Media tab is **optional** and **off by
default**: without it everything is worked out offline from file and folder
names. To use it you need your own TMDB API key. It is **free** for
personal, non-commercial use and takes a few minutes:

1. **Create a free account** at <https://www.themoviedb.org/signup> and
   confirm your e-mail address. (Use a desktop browser: TMDB says the API
   pages are not made for phones.)
2. **Log in** and open your account **Settings** (click your profile
   picture at the top right, then *Settings*).
3. In the menu on the left click **API**
   (<https://www.themoviedb.org/settings/api>).
4. **Request an API key:** choose the **Developer** (personal use) option
   and **accept TMDB's terms of use**.
5. **Fill in the form** that follows: what you use it for (for example
   "Personal use: organizing my own movie files for Plex with PrimeCrew
   Organimizer"), an application name and URL (you can enter
   `https://www.themoviedb.org` if you have none), and your contact
   details. Submit it; the key is normally shown right away.
6. **Copy the key.** The API page now shows two values. Copy the long
   **API Read Access Token** (starts with `eyJ…`). The shorter **API Key**
   (32 letters and digits, "v3 auth") works too.
7. In Organimizer open **Tools > Options**, find **TMDB lookup**, tick
   **Look up movie and TV titles on TMDB**, **paste the key** into
   **Your TMDB key**, optionally set the language (for example `en-US` or
   `de-DE`), and click **Save**. The line under the field says whether the
   key looks right.

Good to know:

- The key is stored **only on your PC**, in `Organimizer.ini` (see
  "Portable or installed"), and it is sent only to TMDB
  (`api.themoviedb.org`) when you click **Look up on TMDB**. Organimizer has
  no server of its own and sends nothing else. Do not share the INI file if
  you want to keep the key private.
- Lookups only suggest titles: you choose the right match, and nothing is
  moved until you click Apply.
- No internet, a wrong key or too many requests just show a message; the
  offline analysis keeps working. To stop using TMDB, untick the option or
  clear the key.
- This product uses the TMDB API but is not endorsed or certified by TMDB.

## Portable or installed

- **Portable zip:** settings are kept in `Organimizer.ini` next to
  `Organimizer.exe`.
- **Installed with setup:** settings are kept in
  `%APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini`; the Start
  Menu shortcut is *PrimeCrew Software > PrimeCrew Organimizer*.
- Either way, undo logs (`Organimizer-undo-*.json`) are written into the
  folder you sorted or scanned.

## Coming from PhotoSorter

- On the first start Organimizer imports your PhotoSorter settings and
  presets (`PhotoSorter.ini` next to the exe, or
  `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`) into the new
  `Organimizer.ini`. The old file is left exactly as it is.
- Setup recognizes an installed PhotoSorter: it shows "Updating existing
  installation: PhotoSorter v2.0.5 → PrimeCrew Organimizer v3.0.0",
  installs Organimizer next to it, then removes the old PhotoSorter program,
  its Start Menu shortcut and its Add/Remove Programs entry. Your files,
  settings and undo logs are kept.
- **Undo last run** also finds undo logs written by PhotoSorter.

## Safety

- Only the file types you choose are touched, and protected files never.
- Nothing is overwritten or deleted. Name clashes get ` (1)`, ` (2)`, …
- The preview always comes first, and every Apply can be undone.

## Troubleshooting

- **Windows SmartScreen:** the exe is not code-signed, so Windows may show
  "Windows protected your PC" on first run. Click **More info > Run
  anyway** after checking the SHA-256 published with the release.
- **"Folder not allowed":** the folder is on the blacklist (Tools > Folder
  Blacklist).
- **TMDB says the key is not valid (401):** copy the key again from
  <https://www.themoviedb.org/settings/api> and paste it into Tools >
  Options.
- **A movie is detected as something else:** click its name in the Plex
  table and correct the type and title, or untick it.

## Keyboard shortcuts

| Key    | Action        |
|--------|---------------|
| Ctrl+N | New Project   |
| Ctrl+O | Open Folder…  |
| F1     | This help     |

## About and license

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General
Public License v3.0 or later. See `LICENSE` next to `Organimizer.exe`.
Third-party licenses: `THIRD-PARTY-NOTICES.txt`.

This product uses the TMDB API but is not endorsed or certified by TMDB.
