PhotoSorter 2.0.4  -  portable photo & video renamer / sorter for Windows 10/11
===============================================================================

WHAT IT DOES
  1. Renames image and video files with an ordered list of rules (modelled
     on den4b ReNamer: Insert, Delete, Remove, Replace, Serialize, ...).
  2. Optionally moves them into date folders (default "Month YYYY", e.g.
     "March 2024"; other layouts such as "2024-03" or nested "2024\03
     March") created inside each file's OWN folder.
  Everything is shown in a live preview first; nothing changes on disk until
  you click Apply and confirm, and the last run can always be undone.

INSTALL
  None. Put PhotoSorter.exe in any folder (or on a USB stick) and run it.
  No installer, no runtime, no DLLs, no registry entries. Settings live in
  PhotoSorter.ini next to the exe. Settings from PhotoSorter 0.1 load as-is.

THE WINDOW (top to bottom = the order things happen)
  Menu bar           File / Presets / Tools / Help (see MENU BAR below).
  1 Source & filter  Browse... or drag a folder onto the window. "Include
                     subfolders" (OFF by default: only the files directly
                     in the chosen folder; tick it to also process every
                     subfolder). A one-line summary of the file types,
                     e.g. "File types: 26 image & video types", with an
                     "Options..." button to change them.
  2 Rename rules     The rule list. Each row: checkbox (on/off), name,
                     summary, Edit, move up/down, duplicate, delete.
                     "+ Add rule" adds a rule at the end. Rules run top to
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
  Every change is written to PhotoSorter.ini at once.

OPTIONS  (Tools > Options..., or the "Options..." buttons)
  Appearance       Theme: Dark / Light / Follow system (uses the Windows
                   app colour setting), and the text size. These apply at
                   once and are saved automatically (no Save needed). The
                   sun/moon button in the top-right corner is a quick
                   Dark/Light switch. "Reset window layout" restores the
                   default window size and position, top-area height,
                   column widths and open/collapsed sections.
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
  * A rename never moves a file to another folder by itself; only the date
    folder step does. Names are checked for Windows rules: forbidden characters
    become "_", reserved names (CON, PRN, AUX, NUL, COM1, LPT1, ...) get a
    "_", trailing dots/spaces are removed, names are cut to 255 characters,
    paths longer than 259 characters are flagged.
  * Nothing is ever overwritten - not by an existing file, and not by
    another file renamed in the same run. Clashes get " (1)", " (2)", ...
    Case-only renames (photo.JPG -> Photo.jpg) work on Windows.
  * Before the first change an undo log is written to the chosen folder:
        PhotoSorter-undo-YYYYMMDD-HHMMSS.json
    If it cannot be written, nothing is changed. "Undo last run" restores
    every original name AND location and removes only the empty date
    folders (all levels) that run created; afterwards the log becomes ...undone.json.
    Undo never overwrites a file that has reappeared at the old location.
  * Nothing is deleted. Across drives a file is copied, size-checked and
    only then the original is removed. Symbolic links are not followed.

SETTINGS: PhotoSorter.ini
  Next to PhotoSorter.exe, created on first run and fully commented (open
  it in Notepad: every option, allowed values and defaults are explained).
  Rewritten when the app closes (and when presets / options are saved, and
  a moment after the window layout changes),
  so edit it while the app is closed. Each write goes to
  PhotoSorter.ini.tmp first and then replaces the file; the previous
  version is kept as PhotoSorter.ini.bak.
  [General]   last_folder, include_subfolders, extensions
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
  [Preset:Name]  one section per saved preset: the options above plus
              rule1..ruleN, and scope = both / rename / folder (what
              loading it changes; missing = both, so older INIs load as
              before). Copy a section to share a preset.
  Invalid values fall back to defaults; an invalid rule is shown in red and
  blocks Apply until fixed. Delete the file to reset everything.

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

SHA-256 (PhotoSorter.exe)
  2315d088044c271a5f5049d022c8784987fd6a4332700f583a5c7b6fd58b6256
