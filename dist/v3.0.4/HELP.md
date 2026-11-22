# Organimizer Help

Organimizer 3.0.4 by PrimeCrew Software (formerly **PhotoSorter**).
It renames files with ordered rules and sorts them into date folders such as
`March 2024` (the **Organize** tab), and it organizes movies and TV episodes
into the folder layout Plex expects (the **Plex Media** tab).

The full reference is `README.txt` next to `Organimizer.exe`.

Contents: Quick start (Organize) | Quick start (Plex Media) | File types and
protection | Folder blacklist | Favorites | Getting a TMDB API key |
Looking up titles on TMDB | Portable or installed | Coming from PhotoSorter | Safety | Troubleshooting |
Keyboard shortcuts | About and license

## Quick start: Organize

1. **Choose a folder.** Click **Browse...** (or File > Open Folder..., Ctrl+O),
   drag a folder onto the window, or pick one from the **Favorites** menu.
   "Include subfolders" is off by default.
2. **Choose the file types.** Next to "File types" pick *Images & videos*
   (the default, same as PhotoSorter), *Images*, *Videos*, *Audio*,
   *Documents*, *Archives*, *All files* or *Custom list* (Tools > Options).
3. **Add rename rules (optional).** In step 2 click **Add rule**. Rules
   run top to bottom on the name without its extension.
4. **Choose the output.** In step 3 leave **Sort into date folders** on to
   move each file into a folder like `March 2024` next to it, or turn it off
   to only rename.
5. **Check the preview**, then click **Apply** and confirm. An undo log is
   written first; **Undo last run** puts everything back.

## Quick start: Plex Media

1. Click the **Plex Media** tab at the top.
2. **Source:** choose the folder with your movies or episodes (downloads,
   rips, an old library) and click **Scan**. Nothing is changed.
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
6. Subtitles (`.srt`, `.ass`, `.sub`/`.idx`, `.vtt`, ...) next to a video or in
   a `Subs` folder are renamed with their video, keeping language tags such
   as `.en`, `.forced` or `.sdh`. Sample clips are skipped. Extras
   (trailers, featurettes, deleted scenes, ...) go into the Plex extras
   folders (`Trailers`, `Featurettes`, ...).
7. Click **Apply** and confirm. Existing files are never overwritten (a
   taken name gets ` (1)`), and **Undo last run** moves everything back.
   Old, now empty folders are left in place (nothing is ever deleted).

**Why this suggestion?** The **Why** column (and "Why this suggestion" in
the right-hand panel) says in plain words how each proposal was worked out,
for example `"making of" in the name, next to Alien (1979).mkv -> Behind The
Scenes` or `in a "Season2" folder -> Season 2`. Organimizer looks at the file
name, the files next to it, the parent and grandparent folder names, disc
rip names (`title_t00`, `LABEL_t03`, `VIDEO_TS`, `BDMV`) and the runtime:
when several files have the same title, the longest one is the movie and the
shorter ones become extras (under 2.5 minutes: trailer, under 45 minutes:
featurette). Names that are "almost right" are fixed to the exact Plex
spelling: `Alien.1979.Directors.Cut.mkv`, `The Expanse S2 E5.mkv`,
`Expanse - 2x05 - Doors and Corners.mkv`, `alien making of.mkv`, a `Season2`
folder, a year without parentheses and so on.

Optional: turn on the **TMDB lookup** (see the next sections) and click
**Look up** in "What you have" to confirm titles and episode names against
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
  `thumbs.db`, `pagefile.sys`, ...), Organimizer's own settings and undo logs,
  hidden or system files, and folders such as `.git`, `$Recycle.Bin` and
  `System Volume Information`. Such files are never renamed or moved, and a
  rule can never change a file's extension to another type.

## Folder blacklist

**Tools > Folder Blacklist...** lists folders that can never be chosen and
are skipped when subfolders are scanned (both tabs). The built-in list is
worked out from this PC's Windows folders (Windows, Program Files,
ProgramData, the AppData folders, Recovery, `$Recycle.Bin`, drive roots
such as `C:\` ...). Add, edit or remove entries, test a folder, or click
**Reset to defaults**. Matching ignores letter case and works on whole
folder names: `C:\Windows` blocks `C:\Windows\Temp` but not
`C:\WindowsStuff`. `*` stands for one folder name
(`C:\Users\*\AppData`), `*:` for any drive (`*:\$Recycle.Bin`).
Choosing a blocked folder shows "Folder not allowed" and selects nothing.

## Favorites

- Click the star (outline) next to a folder field to add it to Favorites; it turns
  into a filled amber star. Click again to remove it.
- The **Favorites** menu jumps to a favorite (in the tab you are on).
- **Favorites > Manage Favorites...** (also in Tools): rename, change the
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
   **API Read Access Token** (starts with `eyJ...`). The shorter **API Key**
   (32 letters and digits, "v3 auth") works too.
7. In Organimizer open **Tools > Options**, find **TMDB lookup**, tick
   **Look up movie and TV titles on TMDB**, **paste the key** into
   **Your TMDB key**, optionally set the language (for example `en-US` or
   `de-DE`), and click **Save**. The line under the field says whether the
   key looks right.

Good to know:

- The key is stored **only on your PC**, in `Organimizer.ini` (see
  "Portable or installed"), and it is sent only to TMDB
  (`api.themoviedb.org`) when you click **Look up**; poster thumbnails come
from `image.tmdb.org` without the key. Organimizer has
  no server of its own and sends nothing else. Do not share the INI file if
  you want to keep the key private.
- Lookups only suggest titles: you choose the right match, and nothing is
  moved until you click Apply.
- No internet, a wrong key or too many requests just show a message; the
  offline analysis keeps working. To stop using TMDB, untick the option or
  clear the key.
- This product uses the TMDB API but is not endorsed or certified by TMDB.

## Looking up titles on TMDB

Needs your own key (previous section) and the TMDB option switched on;
otherwise the **Look up** buttons are greyed out and their tooltip says what
to do.

1. **Scan** as usual. Above the list, **What you have** shows what
   Organimizer found, one line per movie or per TV season, for example
   `Looks like: Alien (1979) - 1 movie, 3 extras, 1 subtitle` or
   `Looks like: The Expanse - Season 2, 4 episodes`.
2. Click **Look up** on a line (or select a file in the list and click
   **Look up the selected file**). The results window shows the matches from
   TMDB with poster, title, year, original title, a short description and
   the TMDB number. If the filename is messy (confidence is not High) or
   TMDB finds nothing, a prompt asks you to type the name first (see
   below). If the right one is missing, change the title or year, switch
   between **Movie** and **TV show**, and click **Search again**, or click
   **Search for something else...**.
3. Click **This one** on the right match. The **match window** shows your
   files on the left and what TMDB has on the right:
   - a TV season: every file is matched to the TMDB episode with the same
     season and episode number (with its title and runtime);
   - a movie: the main file is matched to the movie; extras keep their own
     name and folder unless TMDB lists a video with a very similar name
     (TMDB videos are only name suggestions).
   Change a match with the drop-down list or the **Up** / **Down** buttons
   (two files never get the same episode: they swap), or untick a file to
   leave it as it is.
4. Click **OK - use these names**. The names go into the preview, marked
   **TMDB** in the Match column, with "Confirmed on TMDB" in the Why column.
   Nothing is moved yet: check the list and click **Apply** as usual
   (never overwrites, **Undo last run** puts everything back).

### If it can't find your show

Organimizer asks you to type the movie or TV show name when the filename
confidence is not High (Medium or Low: the name may be too messy for a
good TMDB search), or when TMDB returns no matches. You can also click
**Search for something else...** at any time in the results window.

The box is pre-filled with the best guess from the filename or folder, and
Movie / TV show is pre-filled from what was detected. Type the real name,
optionally a 4-digit year (1870 through next year; leave the year empty if
you are not sure), and press Enter or click **Search**. Esc or **Cancel**
closes the prompt and leaves the current preview unchanged.

The search then shows the usual results window. Choosing a match still goes
through the preview, then **Apply**, and **Undo last run** as usual: nothing
is renamed from this prompt.

A name you type is remembered for that folder until you close the app, so
Look up again uses it without asking first (you can still search for
something else).

Good to know:

- **Tools > Options > TMDB > Add {tmdb-ID} to the folder name** (on by
  default) names the folder like `Alien (1979) {tmdb-348}`, so Plex uses
  exactly that title instead of guessing.
- Titles and episode names come in the language set in Options (for example
  `en-US`, `de-DE`).
- Each search is asked only once while the app is open; Organimizer also
  waits a little between requests and tries once more if TMDB says "too
  many requests". Posters are small thumbnails from `image.tmdb.org` and are
  simply left out if they cannot be loaded.
- No internet or a wrong key shows a plain message in the window; nothing
  else changes.

## Portable or installed

- **Portable zip:** settings are kept in `Organimizer.ini` next to
  `Organimizer.exe`.
- **Installed with setup:** settings are kept in
  `%APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini`; the Start
  Menu shortcut is *PrimeCrew Software > Organimizer*.
- Either way, undo logs (`Organimizer-undo-*.json`) are written into the
  folder you sorted or scanned.

## Coming from PhotoSorter

- On the first start Organimizer imports your PhotoSorter settings and
  presets (`PhotoSorter.ini` next to the exe, or
  `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`) into the new
  `Organimizer.ini`. The old file is left exactly as it is.
- Setup recognizes an installed PhotoSorter: it shows "Updating existing
  installation: PhotoSorter v2.0.5 -> Organimizer v3.0.4",
  installs Organimizer next to it, then removes the old PhotoSorter program,
  its Start Menu shortcut and its Add/Remove Programs entry. Your files,
  settings and undo logs are kept.
- **Undo last run** also finds undo logs written by PhotoSorter.

## Safety

- Only the file types you choose are touched, and protected files never.
- Nothing is overwritten or deleted. Name clashes get ` (1)`, ` (2)`, ...
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
| Ctrl+O | Open Folder...  |
| F1     | This help     |

## About and license

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General
Public License v3.0 or later. See `LICENSE` and `COPYRIGHT` next to `Organimizer.exe`.
Third-party licenses: `THIRD-PARTY-NOTICES.txt`.

This product uses the TMDB API but is not endorsed or certified by TMDB.
