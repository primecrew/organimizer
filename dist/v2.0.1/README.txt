PhotoSorter 2.0.1  -  portable photo & video renamer / sorter for Windows 10/11
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
  1 Source & filter  Browse... or drag a folder onto the window. "Include
                     subfolders". Extension list (image, camera RAW and
                     video types only - see SAFETY).
  2 Rename rules     The rule list. Each row: checkbox (on/off), name,
                     summary, Edit, move up/down, duplicate, delete.
                     "+ Add rule" adds a rule at the end. Rules run top to
                     bottom on the name WITHOUT its extension. Presets row:
                     choose / Load / Delete, or type a name and "Save as
                     preset" (stores the rules + options of steps 1 and 3).
  3 Output folder    "Sort into Month YYYY folders" (untick = rename in
                     place only), date source (Auto priority / Filename /
                     EXIF or video date / Modified / Created), what to do if
                     no date is found, ambiguous dates (US / Intl), folder
                     name template ({MMMM} {MMM} {MM} {M} {YYYY} {YY} {DD}).
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
  Not available: Pascal Script (PhotoSorter never runs scripts or external
  programs), User Input and Mapping (manual lists of new names).

SAFETY RULES
  * Only image, camera RAW and video files are ever touched. Default list:
    png jpg jpeg gif bmp webp tif tiff heic heif cr2 nef arw dng mp4 mov m4v
    avi mkv wmv webm mpg mpeg mts m2ts 3gp. Other types you type into the
    list (txt, pdf, xmp, json sidecars, ...) are ignored and shown in amber.
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
  Rewritten when the app closes, so edit it while the app is closed.
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
              rule1..ruleN. Copy a section to share a preset.
  Invalid values fall back to defaults; an invalid rule is shown in red and
  blocks Apply until fixed. Delete the file to reset everything.

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
  90c14b08eed3952b6d627a5798ac22041d7390fada9389ebee6b129a1dff898b
