Organimizer 3.0.5  -  file renamer, date sorter and Plex organizer
===============================================================================
PrimeCrew Software, for Windows 10/11. Formerly PhotoSorter (2.x).
Quick start and help: see HELP.md (Help > Help, F1).

WHAT IT DOES
  Organize tab (the PhotoSorter pipeline):
  1. Renames files with an ordered list of rules (modelled on den4b
     ReNamer: Insert, Delete, Remove, Replace, Serialize, ...). Images &
     videos by default; other file types can be picked (see FILE TYPES).
  2. Optionally moves them into date folders (default "Month YYYY", e.g.
     "March 2024"; other layouts such as "2024-03" or nested "2024\03
     March") created inside each file's OWN folder.
  Plex Media tab (new in 3.0):
  3. Analyzes movies, TV episodes, extras and subtitles and proposes the
     folder layout Plex expects, in place or in your library folders, with
     a confidence per file and editable corrections (see PLEX MEDIA TAB).
  Everything is shown in a preview first; nothing changes on disk until
  you click Apply and confirm, and the last run can always be undone.

INSTALL
  Two ways, same program:
  * Portable (Organimizer-v3.0.5-win64.zip): put Organimizer.exe in any
    folder (or on a USB stick) and run it. No runtime, no DLLs, no
    registry entries. Settings live in Organimizer.ini next to the
    exe. A PhotoSorter.ini in the same folder is imported on first start
    (and left as it is).
  * Optional installer (Organimizer-v3.0.5-setup.exe): installs just for you
    (%LOCALAPPDATA%\Programs\Organimizer, no administrator rights) or for
    everyone (Program Files). Start Menu shortcut "Organimizer"
    in the "PrimeCrew Software" folder (next to Encode FFS), optional
    desktop shortcut, listed in Add/Remove Programs as "Organimizer"
    (publisher PrimeCrew Software). Settings live in
    %APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini.
    Running a newer (or the same) setup again upgrades in place, without
    duplicate shortcuts. Uninstall removes the program files and
    shortcuts only; your files, undo logs, settings and presets stay.
    Silent: setup.exe /S [/ALLUSERS] [/DESKTOP] [/D=folder].
  * Upgrading from Organimizer 3.0.0/3.0.1: the visible name is now
    Organimizer (publisher stays PrimeCrew Software). Setup keeps the same
    folder and settings, shows "Updating existing installation: Organimizer
    v3.0.1 -> v3.0.2", and removes leftover "PrimeCrew Organimizer"
    Start Menu / desktop shortcuts.
  * Upgrading from PhotoSorter 2.x: setup finds the installed PhotoSorter
    and shows "Updating existing installation: PhotoSorter v2.0.5 ->
    Organimizer v3.0.5". Organimizer is installed next to it
    (...\Programs\Organimizer, same mode); then setup removes the old
    Organimizer.exe and the other files its setup installed, the
    "PhotoSorter" Start Menu / desktop shortcuts and the PhotoSorter
    Add/Remove Programs entry. Organimizer imports
    %APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini (settings and
    presets) on first start and never changes or deletes the old file.

THE WINDOW (top to bottom = the order things happen)
  Menu bar           File / Presets / Favorites / Tools / Help (see MENU
                     BAR below).
  Tabs               Organize (steps 1-5 below) and Plex Media (see PLEX
                     MEDIA TAB). The last tab is remembered.
  1 Source & filter  Browse... or drag a folder onto the window; the star
                     next to the folder adds it to Favorites. "Include
                     subfolders" (OFF by default: only the files directly
                     in the chosen folder; tick it to also process every
                     subfolder). "File types": Images & videos (default),
                     Images, Videos, Audio, Documents, Archives, All files
                     or Custom list, with a summary and an "Options..."
                     button (see FILE TYPES).
  2 Rename rules     The rule list. Each row: checkbox (on/off), name,
                     summary, Edit, move up/down, duplicate, delete.
                     "+ Add rule" adds a rule at the end; "Clear all..."
                     removes every rule (asks first; no files touched). Rules run top to
                     bottom on the name WITHOUT its extension. Presets row:
                     choose / Load / Delete, or type a name and "Save as
                     preset" (stores the rules + options of steps 1 and 3),
                     "Manage..." (see PRESETS below).
  3 Output folder    "Sort into date folders" (untick = rename in place
                     only) and the date source (Auto priority / Filename /
                     EXIF or video date / Modified / Created).
                     Folder name: pick a ready-made layout; "Example:"
                     always shows the result for 5 March 2024. See FOLDER
                     NAMES below. "More options" (click to open) holds what
                     to do if no date is found, ambiguous dates (US / Intl)
                     and, with "Auto", a one-line summary of the priority
                     order (change it in Tools > Options).
  4 Live preview     Updates automatically while you edit. Columns: Before,
                     After, Current folder, Destination folder, Date used,
                     Action, Notes. Amber names got a " (1)" suffix to avoid
                     a clash; Notes (!) show validation fixes (invalid
                     characters, reserved names, long paths, extension
                     changes). Click headers to sort; filter by text/status.
                     "By destination" lists target folders, "Results /
                     errors" shows what the last Apply / Undo did.
  5 Apply / Undo     Apply (enabled only when the preview is current and no
                     rule has an error) and "Undo last run".
  The window remembers its position, size, maximized state, the height of
  the top area (drag its lower edge), the preview column widths and which
  sections are open or collapsed. It is never restored off-screen or
  smaller than 760 x 480; if the saved place no longer fits a screen (e.g. a
  monitor was unplugged) it opens centred. Tools > Options > "Reset window
  layout" puts everything back to the defaults.

FOLDER NAMES  (step 3, "Folder name")
  Ready-made layouts (the first one is the default):
    March 2024                 {MMMM} {YYYY}      (default)
    Mar 2024                   {MMM} {YYYY}
    2024-03                    {YYYY}-{MM}        (sorts by date)
    2024_03                    {YYYY}_{MM}
    2024-03 March              {YYYY}-{MM} {MMMM}
    2024 \ March               {YYYY}{/}{MMMM}     (year folder)
    2024 \ 03 March            {YYYY}{/}{MM} {MMMM}
    2024-03-05                 {YYYY}-{MM}-{DD}   (one folder per day)
    March 05 2024              {MMMM} {DD} {YYYY}
  Build your own...  choose Year (2024 / 24), Month (March / Mar / 03 /
                     3), Day (on/off), the order (year first /
                     month first) and the separator (space, -, _, ., none,
                     subfolder, or Custom... up to 5 characters, no
                     \ / : * ? " < > | { }).
  Advanced           shows the raw template, which you can type directly.
                     Tokens: {YYYY} 2024  {YY} 24  {MMMM} March  {MMM} Mar
                     {MM} 03  {M} 3  {DD} 05  {/} = new subfolder level.
                     The template and the controls stay in sync; a
                     template the controls cannot express shows "Custom
                     template".
  Warnings are shown under the field at once: empty template, unknown
  {token}, unclosed {, characters Windows forbids (/ and \ become "_"; use
  {/} for subfolders), no date or no year token, trailing dots/spaces,
  reserved names (CON, NUL, ...), more than 4 levels. Every folder level is
  cleaned like a file name, so a template can never point outside the
  chosen folder. Files that are already in a matching folder (also just in
  the year folder of a nested layout) count as "already sorted". Undo
  removes every folder level the run created, if it is empty.

PLEX MEDIA TAB
  1 Source       The folder to analyze (Browse..., drop a folder, or a
                 favorite) and "Include subfolders" (on by default). Scan
                 reads it and proposes a Plex layout; nothing is changed.
  2 Destination  In place: the Plex folders are built inside the scanned
                 folder. Library folders: movies go to your Movies folder,
                 episodes to your TV shows folder (empty field = the
                 scanned folder). Blacklisted or missing folders block
                 Apply with a message.
  TMDB           Shows whether the optional TMDB lookup is on (see TMDB).
  What you have  One line per movie (with its extras and subtitles) or TV
                 season, e.g. "Looks like: Alien (1979) - 1 movie, 3
                 extras", with a "Look up" button (TMDB; greyed out without
                 a key) and "Look up the selected file".
  Table          One row per video and subtitle: include checkbox, match
                 (colored dot: green = high, amber = medium, red = low
                 confidence, "Edited" once you changed it, "TMDB" once
                 confirmed on TMDB), type, current file, new location and
                 Why (the reasons in plain words; hover for all of them,
                 notes to check come first). Filter by name; "Only files
                 that need a look" hides the sure ones.
  Correct this file  (click a file name) Type (Movie / TV episode / Extra /
                 Skip), title, year, edition (movies), season, episode,
                 last episode (multi-episode files), episode title, extra
                 type and name, and whether an extra belongs to a movie or
                 a show. "Use for all files of this title" copies title and
                 year to every file detected with the same title; "Reset"
                 goes back to the detected values. With TMDB on: "Look up on
                 TMDB" lists matches to choose from (a TV match also fills
                 the episode title).
  3 Apply / Undo Apply moves the included files (asks first); Undo last run
                 moves them back (undo log in the scanned folder).
  Official Plex folder and file names (movies, the eight extras
  folders, inline suffixes, TV seasons, specials, multi-episode and
  date-based shows) are in HELP.md, chapter "Plex tab". Open it with
  Help > Plex naming conventions..., the "Plex naming conventions
  (Help)" button on an empty Plex tab, or Help (F1).

  What is recognized:
    Movies     Title.Year.1080p.BluRay.x264-GROUP.mkv, Title (Year) [tags],
               a "Title (Year)" folder with a generic file name, editions
               (Director's Cut, Extended, Final Cut, ...).
               -> Title (Year)\Title (Year) {edition-...}.ext
    Episodes   S01E02, s01e02e03 / S01E02-E03, 1x02, "Season 1 Episode 2",
               "Episode 4" inside a "Season N" folder, Specials = season 0,
               date-based shows (2024-03-05).
               -> Show (Year)\Season 01\Show (Year) - s01e02 - Title.ext
    Extras     Trailers, Featurettes, Behind The Scenes, Deleted Scenes,
               Interviews, Scenes, Shorts, Other - from extras folders or
               names such as "-trailer" -> Title (Year)\Trailers\...
    Subtitles  srt ass ssa sub idx vtt sup smi next to the video or in a
               Subs folder, with language / forced / sdh / cc tags kept:
               Title (Year).en.forced.srt
    Skipped    sample clips, files that could not be identified (until you
               fill in the missing fields), everything that is not a video
               or subtitle.
  Same safety as the Organize tab: preview first, never overwrite (a taken
  name gets " (1)"), never delete (old empty folders are left), undo log
  first, blacklisted folders are skipped.

FILE TYPES  (step 1 "File types"; lists in Tools > Options)
  Images & videos  the editable list in Tools > Options (default; the 2.x
                   behaviour). Only image, camera RAW and video types.
  Images, Videos, Audio, Documents, Archives  built-in lists.
  All files        every file with an extension (files without one are
                   skipped).
  Custom list      the extensions in Tools > Options > Custom file types.
  Always protected, whatever you choose: programs and libraries (exe dll
  sys drv ocx cpl scr com msi msp msix appx ...), scripts (bat cmd ps1 vbs
  js wsf hta sh ...), shortcuts and system files (lnk url inf reg ini cat
  manifest mui efi ...), desktop.ini, thumbs.db, pagefile.sys, NTUSER.DAT,
  Organimizer/PhotoSorter settings and undo logs, hidden or system files,
  and folders such as .git, $Recycle.Bin and System Volume Information.
  They are never renamed or moved. A rename rule may change an extension
  only between image / video types; otherwise the original is kept (note
  in the preview).

FOLDER BLACKLIST  (Tools > Folder Blacklist...)
  Folders that can never be chosen (Browse, drop, typed path, favorite or
  Plex library folder: "Folder not allowed") and that are skipped when
  subfolders are scanned (listed in the results). The default list is
  worked out from this PC's environment (SystemRoot / windir, ProgramFiles,
  ProgramFiles(x86), ProgramW6432, ProgramData, the user profiles' AppData,
  Default, All Users, APPDATA, LOCALAPPDATA) plus Windows.old, Recovery,
  Boot, PerfLogs, $Recycle.Bin, System Volume Information, EFI and other
  system folders, and the drive roots themselves (<drive roots>).
  Matching ignores case, works on whole folder names (C:\Windows does not
  block C:\WindowsStuff) and also catches \\?\ and .. forms. Wildcards:
  * = one folder name (C:\Users\*\AppData), *: = any drive.
  Add / Edit / Remove entries, "Test a folder", "Reset to defaults"; Save
  writes the list to the INI ([Blacklist]). Until you edit it, the default
  list follows the PC (recomputed on each start).

FAVORITES  (Favorites menu; Favorites > Manage Favorites...)
  The star next to the folder field (Organize step 1 and Plex Source) adds
  or removes the folder; filled = favorite. The Favorites menu jumps to a
  favorite in the current tab (missing folders are greyed out). Manage
  Favorites: rename, change the folder, add, remove, Up / Down, Go to
  folder; changes are saved at once ([Favorites] in the INI).

TMDB  (optional, off by default; Tools > Options > TMDB lookup)
  Looks up movie / TV titles on The Movie Database to confirm a match
  ("Look up" in What you have: results with posters, then a match window
  with your files on the left and TMDB's movie / episodes on the right; OK
  writes the names into the preview, Apply moves the files). You
  need your own free TMDB API key: HELP.md section "Getting a TMDB API key"
  explains every step (free account, Settings > API, request a Developer
  key, copy the API Read Access Token or the v3 API key, paste it in
  Tools > Options). The key is stored only in Organimizer.ini on your PC
  and sent only to api.themoviedb.org (HTTPS) when you click "Look up"
  (poster thumbnails come from image.tmdb.org). "Add {tmdb-ID} to the
  folder name" (Options, on by default) names folders like
  "Alien (1979) {tmdb-348}". Without it everything works offline. Network errors, a wrong key
  (401) or rate limits (429) show a message and change nothing.
  This product uses the TMDB API but is not endorsed or certified by TMDB.

MENU BAR
  File     New Project (Ctrl+N)  Clears the folder, the preview and the
                                 results, and resets the rename rules and
                                 the output options (date folders on/off,
                                 folder template, date source, ambiguous
                                 dates, if no date found) to defaults.
                                 Kept: saved presets, file types, Auto
                                 priority, "Include subfolders", theme,
                                 text size and the window layout. Never moves, renames or deletes
                                 a file, and leaves undo logs on disk. Asks
                                 first if the preview has unapplied changes,
                                 the last Apply in the folder was not
                                 undone, or the rules are not saved as a
                                 preset.
           Open Folder... (Ctrl+O)  Same as Browse...
           Undo Last Run...      Same as "Undo last run" (enabled only when
                                 the chosen folder has an undo log).
           Exit                  Closes the app (settings are saved).
  Presets  Load >                Lists the saved presets.
           Save current as preset...  Asks for a name.
           Save changes to "X"   Shown while a preset is being edited.
           Manage Presets...     See PRESETS below.
  Tools    Options...            See OPTIONS below.
           Open Settings Folder  The folder with Organimizer.ini.
           Open Undo Log Folder  The chosen folder, when it has an undo log.
  Help     Help (F1)             Opens HELP.md (the one next to
                                 Organimizer.exe) with its default app, or
                                 Notepad if no app is set up for .md. If
                                 HELP.md is missing it opens this
                                 README.txt; if both are missing, a message
                                 offers to open the program folder.
           Plex naming           Opens the same HELP.md. On the Plex tab
           conventions...        the status line points at the "Plex tab"
                                 chapter (movies, extras, TV naming).
           About Organimizer...  Icon, version, publisher, license and the
                                 portable / unsigned-build note.

PRESETS  (Presets > Manage Presets..., or "Manage..." in step 2)
  A preset stores the whole rename rule chain AND the folder/output options
  (date folders on/off, folder template, date source, Auto priority,
  ambiguous dates, if no date found) plus "Include subfolders" and the file
  types. "Loads" chooses what loading it changes:
    Rules + folder options   everything above (default; what 2.0/2.0.1
                             presets always did)
    Rename rules only        only the rule chain
    Folder / date options only  only the output/folder options
  Manage Presets lists every preset with a read-only summary, and:
    Edit          loads the preset into the main window; change the rules /
                  options there, then "Save changes" (here, in step 2, or in
                  the Presets menu) writes them back to the same preset.
    Duplicate     copies it as "Name copy" ("Name copy 2", ...).
    Rename        names must be unique (case-insensitive), not empty, at
                  most 64 characters, and may not contain [ ] ; # or line
                  breaks.
    Delete...     asks first.
  Every change is written to Organimizer.ini at once.

OPTIONS  (Tools > Options..., or the "Options..." buttons)
  Appearance       Theme: Dark / Light / Follow system (uses the Windows
                   app color setting), and the text size. These apply at
                   once and are saved automatically (no Save needed). The
                   Light / Dark button in the top-right corner is a
                   quick switch. "Reset window layout" restores the
                   default window size and position, top-area height,
                   column widths and open/collapsed sections.
  File types       The editable extension list, comma-separated, case-
                   insensitive. "Reset to defaults" restores the default
                   list. Only image, camera RAW and video types are
                   accepted; anything else you type is ignored, listed in
                   amber and never touched (see SAFETY).
  Custom file types  Extensions for the "Custom list" file type; protected
                   types are refused and listed in amber.
  TMDB lookup      On/off (off by default), your TMDB key (hidden; "Show"
                   to check it), title language, a link to the HELP.md
                   section "Getting a TMDB API key" and to TMDB's API page.
  Date detection   The Auto priority order: move sources up/down, untick
                   to skip one (at least one stays on). Used when Date
                   source is "Auto" and by the "Insert date" rule.
  Save applies the changes and writes them to Organimizer.ini at once;
  Close (or Esc) discards unsaved changes. Save is disabled while Apply /
  Undo is running.

RULES  (pos = where text goes: start, end, after N chars, N chars before the
        end, replace the whole name, after "text", before "text")
  Insert              Insert text at pos.
  Delete              From position N or from a delimiter, until N chars,
                      until a delimiter or till the end; optional right-to-
                      left and "keep delimiters".
  Remove              Remove text (all / first / last occurrence), a set of
                      characters, the first or last N chars, chars N..M,
                      all whitespace or all digits.
  Replace             Find and replace text (case option, all/first/last).
  Regular expressions Replace with a regular expression ($1, $2 ... in the
                      replacement).
  Rearrange           Split the name by delimiters (or positions) and
                      rebuild it with a pattern: $1 = first part, $-1 = last
                      part, $0 = whole name. Example: split " - ",
                      pattern "$2 - $1".
  Extension           Set the extension to another IMAGE/VIDEO extension, or
                      "detect" it from the file content (fixes a PNG saved as
                      .jpg). A wrong-looking change is never made silently:
                      it is shown in Notes.
  Strip               Strip English letters, digits, symbols, brackets and/or
                      your own characters (or everything EXCEPT them).
  Case                lower / UPPER / Title / Sentence / iNVERT.
  Serialize           Number files: start, step, repeat, reset every N,
                      zero padding, decimal / abc / ABC / Roman, restart in
                      every folder, at pos with a separator.
  Randomize           Random characters (digits / letters / your set),
                      unique within the run. The seed is stored, so the
                      preview and the real run give the same names.
  Padding             Zero-pad numbers to N digits, or remove zero padding,
                      or pad the whole name to N chars.
  Clean up            Trim, collapse spaces, _ -> space, space -> _,
                      dots -> spaces, remove characters Windows forbids,
                      collapse repeated separators, strip (...) [...] {...}
                      contents, normalise unicode spaces, strip emoji,
                      strip accents, space before capitals.
  Translit            Accented Latin, Cyrillic and Greek letters to plain
                      Latin (e->e, ss, Zh ...).
  Reformat date       Find dates written in one or more formats and rewrite
                      them, e.g. %Y%m%d -> %Y-%m-%d. Invalid dates are kept.
  Insert date         Insert the file's date (the date Organimizer uses for
                      the folder, or a specific source) with a format such as
                      %Y-%m-%d_%H%M%S.
  Insert folder name  Insert the name of the file's folder.
  Extension case      .JPG -> .jpg or the other way round.
  Not available: Pascal Script (Organimizer never runs scripts; the only
  programs it starts are the default viewer for HELP.md / README.txt and Explorer,
  from the Help and Tools menus), User Input and Mapping (manual lists of
  new names).

SAFETY RULES
  * Only the file types you pick are touched (default: image, camera RAW
    and video files - png jpg jpeg gif bmp webp tif tiff heic heif cr2 nef
    arw dng mp4 mov m4v avi mkv wmv webm mpg mpeg mts m2ts 3gp, edit in
    Tools > Options). Programs, scripts, shortcuts, system and hidden files
    are never touched (see FILE TYPES). Blacklisted folders are never
    chosen or scanned (see FOLDER BLACKLIST).
  * A rename never moves a file to another folder by itself; only the date
    folder step does. Names are checked for Windows rules: forbidden characters
    become "_", reserved names (CON, PRN, AUX, NUL, COM1, LPT1, ...) get a
    "_", trailing dots/spaces are removed, names are cut to 255 characters,
    paths longer than 259 characters are flagged.
  * Nothing is ever overwritten - not by an existing file, and not by
    another file renamed in the same run. Clashes get " (1)", " (2)", ...
    Case-only renames (photo.JPG -> Photo.jpg) work on Windows.
  * Before the first change an undo log is written to the chosen folder:
        Organimizer-undo-YYYYMMDD-HHMMSS.json
    If it cannot be written, nothing is changed. "Undo last run" restores
    every original name AND location and removes only the empty date
    folders (all levels) that run created; afterwards the log becomes ...undone.json.
    Undo also finds PhotoSorter-undo-*.json logs written by PhotoSorter.
    Undo never overwrites a file that has reappeared at the old location.
  * Nothing is deleted. Across drives a file is copied, size-checked and
    only then the original is removed. Symbolic links are not followed.

SETTINGS: Organimizer.ini
  Where: next to Organimizer.exe (portable copy), or in
  %APPDATA%\PrimeCrew Software\Organimizer when installed with setup
  (setup puts an Organimizer.installed file next to the exe). Rules, in
  order: a file named Organimizer.portable next to the exe forces
  portable mode; a writable Organimizer.ini next to the exe is always
  used; otherwise an installed copy, or a copy in a folder that is not
  writable, uses %APPDATA%. Tools > Open Settings Folder opens the folder
  in use. Undo logs are not settings: they are always written into the
  photo folder you sorted.
  Created on first run and fully commented (open
  it in Notepad: every option, allowed values and defaults are explained).
  Rewritten when the app closes (and when presets / options are saved, and
  a moment after the window layout changes),
  so edit it while the app is closed. Each write goes to
  Organimizer.ini.tmp first and then replaces the file; the previous
  version is kept as Organimizer.ini.bak.
  First start of 3.0: if there is no Organimizer.ini yet, a PhotoSorter.ini
  in the same place (next to the exe, or %APPDATA%\PrimeCrew Software\
  PhotoSorter) is imported - settings, rules and presets - and the old file
  is never changed or deleted. The status bar says so once.
  [General]   last_folder, include_subfolders, extensions, file_types
              (media / images / videos / audio / documents / archives /
              all / custom), custom_extensions
              include_subfolders: default false (off) since 2.0.4.
              A missing or invalid value means false.
  [Date]      source, auto_order, ambiguous_order, fallback
  [Output]    folder_template ({/} = subfolder level),
              sort_into_month_folders
  [Rules]     the current rule list, one line per rule:
                rule1 = type | on/off | key=value | key=value ...
              e.g.
                rule1 = regex | on | find=^(IMG%7CVID)_ | with=
                rule2 = serialize | on | start=1 | pad=3 | pos=suffix | sep=_
              Characters | % " ; # [ ] are written as %XX (| = %7C).
              Every rule type and key is listed in the INI comments.
  [Window]    theme (dark / light / system), font_size, width, height,
              x, y, maximized, setup_panel_height, files_columns,
              folders_columns (comma-separated widths), open_source,
              open_rules, open_output, open_output_more, folder_advanced.
              Missing or invalid values use the defaults, so older INIs
              load unchanged; 2.0.2 ignores the new keys.
  [Blacklist] custom (false = the built-in list), block1..blockN
  [Favorites] fav1 = Name | Path, ... (in menu order)
  [Plex]      source, include_subfolders, destination (inplace / roots),
              movies_root, tv_root, tmdb_enabled, tmdb_key (your own key;
              keep the INI private), tmdb_language (e.g. en-US)
  [Window]    also mode (organize / plex): the last tab.
  [Preset:Name]  one section per saved preset: the options above plus
              rule1..ruleN, and scope = both / rename / folder (what
              loading it changes; missing = both, so older INIs load as
              before). Copy a section to share a preset.
  Invalid values fall back to defaults; an invalid rule is shown in red and
  blocks Apply until fixed. Delete the file to reset everything.

CHANGES IN 3.0.5
  * HELP.md has a Plex tab chapter covering official Plex Media Server
    naming for movies, extras folders and suffixes, TV shows
    (including specials, multi-episode and date-based files), common
    mistakes (from Organimizer's own tests) and a cheat sheet. Help >
    Plex naming conventions... and a button on an empty Plex tab open
    it. Split / stacked files (pt1, cd1, disc1, disk1, dvd1, part1)
    keep that suffix; numbered extras suffixes (-deleted1) are
    recognized; a date-based episode already in a Season NN folder
    stays in that season.

CHANGES IN 3.0.4
  * Look up on the Plex tab asks you to type the movie or TV show
    name (and an optional 4-digit year, 1870 through next year) when
    the filename confidence is not High (Medium or Low) or when TMDB
    finds nothing. Movie / TV show is pre-filled from what was
    detected. The results window has Search for something else... at
    any time. Enter searches; Esc or Cancel leaves the preview
    unchanged. A typed name is remembered for that folder until you
    close the app. Matching still goes through preview, Apply and
    Undo; nothing is renamed from the prompt.

CHANGES IN 3.0.3
  * LICENSE is the official, unmodified GNU GPL v3 text. A new COPYRIGHT
    file has the program copyright (Copyright (C) 2026 PrimeCrew Software)
    and a short GPL-3.0-or-later notice pointing to LICENSE. Both files
    ship in the zip and the installer.

CHANGES IN 3.0.2
  * Visible name is now Organimizer (publisher stays PrimeCrew Software).
    Start Menu shortcut PrimeCrew Software\Organimizer; leftover
    "PrimeCrew Organimizer" shortcuts from 3.0.0/3.0.1 are removed on
    upgrade. Add/Remove Programs shows "Organimizer" (same key, no
    duplicate). Install folder and settings path are unchanged.
  * Help > About matches the other PrimeCrew apps (icon, name, Version,
    "by PrimeCrew Software", copyright, license, Close). TMDB attribution
    is kept.
  * Setup over an installed 3.0.1 shows "Updating existing installation:
    Organimizer v3.0.1 -> v3.0.2" and keeps the folder.

CHANGES IN 3.0.1
  * Version bump of the 3.0.0 lookup and recognition build (smarter
    per-file Plex naming with a Why column, and TMDB lookup windows).
    Setup over an installed 3.0.0 shows "Updating existing installation:
    PrimeCrew Organimizer v3.0.0 -> v3.0.1" and keeps the folder.

CHANGES IN 3.0.0
  * Renamed: PhotoSorter is now PrimeCrew Organimizer (Organimizer.exe,
    Organimizer.ini, Organimizer-undo-*.json, Start Menu "PrimeCrew
    Organimizer"). PhotoSorter settings and presets are imported on first
    start; the old file is never changed. Undo finds old PhotoSorter logs.
  * Setup upgrades an installed PhotoSorter 2.x: banner "Updating existing
    installation: PhotoSorter v2.0.5 -> PrimeCrew Organimizer v3.0.1", then
    the old program, shortcut and Add/Remove entry are removed.
  * New Plex Media tab: scan, analyze movies / TV episodes / extras /
    subtitles, Plex layout in place or into library folders, confidence per
    file, editable corrections, preview / apply / undo / never overwrite.
  * Optional TMDB lookup (off by default, your own free key, kept only on
    your PC; step-by-step key guide in HELP.md). TMDB attribution and logo
    in Help > About.
  * File types: Images & videos (default), Images, Videos, Audio,
    Documents, Archives, All files or a Custom list; programs, scripts,
    shortcuts, system and hidden files are always protected.
  * Folder blacklist (Tools > Folder Blacklist...) with system-folder
    defaults from the environment; blocked folders cannot be chosen and are
    skipped in subfolder scans.
  * Favorites: star next to the folder, Favorites menu, Manage Favorites.

CHANGES IN 2.0.5
  * Help > About window with the app icon, version, publisher (PrimeCrew
    Software), license and the portable / unsigned note. The main window
    header shows the icon, name and version.
  * New HELP.md next to the exe; Help (F1) opens it (README.txt if it is
    missing).
  * Now licensed under the GNU GPL v3 or later (LICENSE in the zip).
  * Wording and hover-help tidy-ups ("Undo Last Run..." in the File menu,
    "date folders", more tooltips). Renaming, sorting and undo are
    unchanged; "Include subfolders" is still OFF by default.
  * New Windows installer (PhotoSorter-2.0.5-setup.exe) next to the
    portable zip; see INSTALL. Installed copies keep their settings in
    %APPDATA%\PrimeCrew Software\PhotoSorter; the portable zip works
    exactly as before.
  * Larger default window (1280 x 900; a saved size is kept). Choosing a
    folder with Browse... or by dropping it folds step 1 to its one-line
    summary so steps 2 and 3 fit; click it to open it again.
  * The preview shows only the counters that are not 0 (Files always).
  * "Clear all..." rules now asks first.
  * The theme button says Light / Dark (the sun/moon symbols did not
    always display).
  * THIRD-PARTY-NOTICES.txt lists the licenses of the libraries and fonts
    built into PhotoSorter.exe.

CHANGES IN 2.0.4
  * "Include subfolders" is now OFF by default: on a fresh start (no
    PhotoSorter.ini) only the files directly inside the chosen folder are
    previewed and sorted. Tick the box to include subfolders as before.
  * A missing or invalid include_subfolders value (also in a preset) now
    means off.
  * UPGRADING: if you used an older version, your PhotoSorter.ini most
    likely contains "include_subfolders = true" (the old default). That
    saved choice is kept. To use the new default, untick "Include
    subfolders" (saved when the app closes), or close the app and change
    the line to false / delete the line / delete PhotoSorter.ini (deleting
    the INI resets ALL settings and presets). Presets that store a value
    keep it. File > New Project still keeps the current setting.

CHANGES IN 2.0.3
  * The window remembers position, size, maximized state, top-area height,
    preview column widths and open/collapsed sections ([Window] in the
    INI), clamped so it never opens off-screen. "Reset window layout" in
    Tools > Options.
  * Tools > Options > Appearance: Dark / Light / Follow system theme (live,
    saved). The top-bar sun/moon button stays as a quick switch.
  * Step 3 folder name builder: ready-made layouts with a live example,
    "Build your own..." controls, "Advanced" raw template with a token
    legend, inline warnings, and a "More options" section. The default
    "{MMMM} {YYYY}" and existing templates/presets are unchanged.
  * New {/} token for nested folders (e.g. 2024\03 March); undo removes
    every created level.

CHANGES IN 2.0.2
  * Extension list moved to Tools > Options; no functional changes to
    rules/sorting. The main window shows a one-line summary instead of
    the full list. The Auto date priority editor moved there too (one-line
    summary in step 3). New "Reset to defaults" button for the list.
  * New menu bar: File (New Project Ctrl+N, Open Folder Ctrl+O, Undo Last
    Apply, Exit), Presets (Load, Save current, Manage Presets), Tools
    (Options, Open Settings Folder, Open Undo Log Folder), Help (F1 opens
    README.txt, About).
  * Manage Presets window: view, edit + save changes, duplicate, rename,
    delete (with confirmation); presets can load rules + folder options,
    rules only or folder options only. "Delete" in step 2 now asks first.
  * PhotoSorter.ini is written via a temp file and the previous version is
    kept as PhotoSorter.ini.bak. New optional preset key "scope"; older
    INIs load unchanged and 2.0.1 ignores the new key.

CHANGES IN 2.0.1
  * New app icon (orange folder with papers). No functional changes.

NOTES / CAVEATS
  * Tested on Linux and under Wine; not yet on a real Windows PC.
  * Plex names are proposals worked out from file and folder names (and
    TMDB, if you turn it on): check the red and amber rows before Apply.
  * The TMDB lookup needs internet access and your own key; without it the
    Plex tab works fully offline.
  * The exe is not code-signed. Windows SmartScreen will probably show
    "Windows protected your PC" on first run (More info > Run anyway); some
    antivirus products occasionally flag unsigned programs. Compare the
    SHA-256 below with the file you received.
  * Requires OpenGL 2.0+ (any normal Windows 10/11 PC); may not start in
    some bare VMs / remote sessions without GPU drivers.
  * "Created" dates are often reset when files are copied; "Modified"
    usually survives. EXIF is read from JPEG/TIFF/HEIC/PNG/WebP, the
    creation time from MP4/MOV/M4V/3GP videos.
  * Serialize counts every file in the preview, including files that are
    skipped later (e.g. "no date" with fallback = skip).
  * With a nested layout the "New folders" count in the preview and the
    Apply confirmation counts only the innermost folders (e.g. "03 March"),
    not the year folders that are also created.
  * "Follow system" uses the Windows light/dark app setting as reported to
    the program; under Wine it resolved to Light.

LICENSE
  Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General
  Public License v3.0 or later. See LICENSE and COPYRIGHT (next to
  Organimizer.exe). Third-party licenses: THIRD-PARTY-NOTICES.txt.
  This product uses the TMDB API but is not endorsed or certified by TMDB.

SHA-256 (Organimizer.exe)
  f238457de022a6fe5c871be98fb86925e13d143351a02dac9e16b17d8eff497a
