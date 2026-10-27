PhotoSorter 2.0.2  -  portable photo & video renamer / sorter for Windows 10/11
===============================================================================

WHAT IT DOES
  1. Renames image and video files with an ordered list of rules (modelled
     on den4b ReNamer: Insert, Delete, Remove, Replace, Serialize, ...).
  2. Optionally moves them into "Month YYYY" subfolders (e.g. "March 2024")
     created inside each file's OWN folder.
  Everything is shown in a live preview first; nothing changes on disk until
  you click Apply and confirm, and the last run can always be undone.

INSTALL
  None. Put PhotoSorter.exe in any folder (or on a USB stick) and run it.
  No installer, no runtime, no DLLs, no registry entries. Settings live in
  PhotoSorter.ini next to the exe. Settings from PhotoSorter 0.1 load as-is.

THE WINDOW (top to bottom = the order things happen)
  Menu bar           File / Presets / Tools / Help (see MENU BAR below).
  1 Source & filter  Browse... or drag a folder onto the window. "Include
                     subfolders". A one-line summary of the file types,
                     e.g. "File types: 26 image & video types", with an
                     "Options..." button to change them.
  2 Rename rules     The rule list. Each row: checkbox (on/off), name,
                     summary, Edit, move up/down, duplicate, delete.
                     "+ Add rule" adds a rule at the end. Rules run top to
                     bottom on the name WITHOUT its extension. Presets row:
                     choose / Load / Delete, or type a name and "Save as
                     preset" (stores the rules + options of steps 1 and 3),
                     "Manage..." (see PRESETS below).
  3 Output folder    "Sort into Month YYYY folders" (untick = rename in
                     place only), date source (Auto priority / Filename /
                     EXIF or video date / Modified / Created), what to do if
                     no date is found, ambiguous dates (US / Intl), folder
                     name template ({MMMM} {MMM} {MM} {M} {YYYY} {YY} {DD}).
                     With "Auto" a one-line summary of the priority order
                     is shown (change it in Tools > Options).
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

MENU BAR
  File     New Project (Ctrl+N)  Clears the folder, the preview and the
                                 results, and resets the rename rules and
                                 the output options (Month YYYY on/off,
                                 folder template, date source, ambiguous
                                 dates, if no date found) to defaults.
                                 Kept: saved presets, file types, Auto
                                 priority, "Include subfolders", theme and
                                 text size. Never moves, renames or deletes
                                 a file, and leaves undo logs on disk. Asks
                                 first if the preview has unapplied changes,
                                 the last Apply in the folder was not
                                 undone, or the rules are not saved as a
                                 preset.
           Open Folder... (Ctrl+O)  Same as Browse...
           Undo Last Apply...    Same as "Undo last run" (enabled only when
                                 the chosen folder has an undo log).
           Exit                  Closes the app (settings are saved).
  Presets  Load >                Lists the saved presets.
           Save current as preset...  Asks for a name.
           Save changes to "X"   Shown while a preset is being edited.
           Manage Presets...     See PRESETS below.
  Tools    Options...            See OPTIONS below.
           Open Settings Folder  The folder with PhotoSorter.exe/.ini.
           Open Undo Log Folder  The chosen folder, when it has an undo log.
  Help     Help (F1)             Opens this README.txt (the one next to
                                 PhotoSorter.exe) with its default app,
                                 normally Notepad. If it is missing, a
                                 message offers to open the program folder.
           About PhotoSorter...  Version, unsigned-build note.

PRESETS  (Presets > Manage Presets..., or "Manage..." in step 2)
  A preset stores the whole rename rule chain AND the folder/output options
  (Month YYYY on/off, folder template, date source, Auto priority,
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
  Every change is written to PhotoSorter.ini at once.

OPTIONS  (Tools > Options..., or the "Options..." buttons)
  File types       The editable extension list, comma-separated, case-
                   insensitive. "Reset to defaults" restores the default
                   list. Only image, camera RAW and video types are
                   accepted; anything else you type is ignored, listed in
                   amber and never touched (see SAFETY).
  Date detection   The Auto priority order: move sources up/down, untick
                   to skip one (at least one stays on). Used when Date
                   source is "Auto" and by the "Insert date" rule.
  Save applies the changes and writes them to PhotoSorter.ini at once;
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
  Insert date         Insert the file's date (the date PhotoSorter uses for
                      the folder, or a specific source) with a format such as
                      %Y-%m-%d_%H%M%S.
  Insert folder name  Insert the name of the file's folder.
  Extension case      .JPG -> .jpg or the other way round.
  Not available: Pascal Script (PhotoSorter never runs scripts; the only
  programs it starts are the default viewer for README.txt and Explorer,
  from the Help and Tools menus), User Input and Mapping (manual lists of
  new names).

SAFETY RULES
  * Only image, camera RAW and video files are ever touched. Default list:
    png jpg jpeg gif bmp webp tif tiff heic heif cr2 nef arw dng mp4 mov m4v
    avi mkv wmv webm mpg mpeg mts m2ts 3gp (edit in Tools > Options). Other
    types you type into the list (txt, pdf, xmp, json sidecars, ...) are
    ignored and shown in amber.
  * A rename never moves a file to another folder by itself; only the Month
    YYYY step does. Names are checked for Windows rules: forbidden characters
    become "_", reserved names (CON, PRN, AUX, NUL, COM1, LPT1, ...) get a
    "_", trailing dots/spaces are removed, names are cut to 255 characters,
    paths longer than 259 characters are flagged.
  * Nothing is ever overwritten - not by an existing file, and not by
    another file renamed in the same run. Clashes get " (1)", " (2)", ...
    Case-only renames (photo.JPG -> Photo.jpg) work on Windows.
  * Before the first change an undo log is written to the chosen folder:
        PhotoSorter-undo-YYYYMMDD-HHMMSS.json
    If it cannot be written, nothing is changed. "Undo last run" restores
    every original name AND location and removes only the empty Month YYYY
    folders that run created; afterwards the log becomes ...undone.json.
    Undo never overwrites a file that has reappeared at the old location.
  * Nothing is deleted. Across drives a file is copied, size-checked and
    only then the original is removed. Symbolic links are not followed.

SETTINGS: PhotoSorter.ini
  Next to PhotoSorter.exe, created on first run and fully commented (open
  it in Notepad: every option, allowed values and defaults are explained).
  Rewritten when the app closes (and when presets / options are saved),
  so edit it while the app is closed. Each write goes to
  PhotoSorter.ini.tmp first and then replaces the file; the previous
  version is kept as PhotoSorter.ini.bak.
  [General]   last_folder, include_subfolders, extensions
  [Date]      source, auto_order, ambiguous_order, fallback
  [Output]    folder_template, sort_into_month_folders
  [Rules]     the current rule list, one line per rule:
                rule1 = type | on/off | key=value | key=value ...
              e.g.
                rule1 = regex | on | find=^(IMG%7CVID)_ | with=
                rule2 = serialize | on | start=1 | pad=3 | pos=suffix | sep=_
              Characters | % " ; # [ ] are written as %XX (| = %7C).
              Every rule type and key is listed in the INI comments.
  [Window]    theme, font_size, width, height
  [Preset:Name]  one section per saved preset: the options above plus
              rule1..ruleN, and scope = both / rename / folder (what
              loading it changes; missing = both, so older INIs load as
              before). Copy a section to share a preset.
  Invalid values fall back to defaults; an invalid rule is shown in red and
  blocks Apply until fixed. Delete the file to reset everything.

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

SHA-256 (PhotoSorter.exe)
  e0123bb246de99e70cdb44b9fdf4d3eba8171c214c3e24cf8e0b3e5b9f18947f
