PhotoSorter 0.1.0  -  portable image sorter for Windows 10/11
==============================================================

WHAT IT DOES
  Moves image files into "Month YYYY" subfolders (e.g. "October 2026"),
  created inside each image's OWN folder. With "Include subfolders" on,
  every folder that contains images gets its own Month YYYY subfolders.

INSTALL
  None. Put PhotoSorter.exe in any folder (or on a USB stick) and run it.
  No installer, no VC++ runtime, no DLLs, no registry entries.

HOW TO USE
  1. Source folder: click "Browse..." (or drag a folder onto the window).
     Tick/untick "Include subfolders". Edit the extension list if needed
     (default: png, jpg, jpeg, gif, bmp, webp, tif, tiff, heic).
  2. Options:
       Date source   Auto (priority order) / Detect from filename /
                     EXIF date taken / Modified date / Created date
       Auto priority Order in which sources are tried (default:
                     Filename > EXIF > Modified > Created). Untick to skip one.
       If no date    Use modified date / Use created date / Skip the file
       Ambiguous     US (MM-DD) or Intl (DD-MM) for names like 03-04-2024
       Folder name   Template, default {MMMM} {YYYY}. Tokens: {MMMM} {MMM}
                     {MM} {M} {YYYY} {YY} {DD}
  3. Click "Preview (dry run)". Nothing is changed. The table shows file,
     current folder, destination, date used, source and status; click a
     column header to sort, use the filter box / dropdowns to filter.
     "By destination" lists every target folder with its file count.
  4. Click "Apply" and confirm. Progress is shown in the status bar;
     results and per-file errors are under "Results / errors".
  5. "Undo last run" moves the files back and removes the (now empty)
     Month YYYY folders that run created.

FILENAME DATE DETECTION (examples)
  20240315, 2024-03-15, 2024_03_15, 2024.03.15, IMG_20240315_142530,
  PXL_20240315_142530123, IMG-20240315-WA0001 (WhatsApp),
  Screenshot 2024-03-15 at 14.25.30, Screenshot_20240315-142530,
  Photo 2024-03-15, 15-03-2024 / 03-15-2024, Unix timestamps
  (10 or 13 digits, 2000 .. now+1 year). Dates are validated (real
  calendar date, year 1990 .. next year) to avoid false positives such as
  IMG_1234 or DSC01234.

SAFETY RULES
  * Nothing is ever overwritten. If the destination name exists, the file
    gets " (1)", " (2)", ... before the extension.
  * Files already in the correct Month YYYY folder are skipped. A file in
    a wrong Month YYYY folder moves to the correct sibling folder (not a
    nested one). The whole folder is scanned BEFORE anything is moved, so
    newly created folders are never re-processed.
  * Files are moved with a rename; across drives they are copied, the copy
    is size-checked and only then is the original removed.
  * Before the first move an undo log is written to the chosen folder:
        PhotoSorter-undo-YYYYMMDD-HHMMSS.json
    (JSON list of every move and every folder created). After a successful
    undo it is renamed to ...undone.json. If the log cannot be written,
    nothing is moved.
  * Undo only deletes folders that PhotoSorter created and that are empty.
    It never overwrites a file that has since reappeared at the original
    location; such entries stay in the log so you can retry.
  * Symbolic links / junctions are not followed.

SETTINGS: PhotoSorter.ini
  Stored ONLY in PhotoSorter.ini next to PhotoSorter.exe (created on first
  run, fully commented - open it in Notepad to see every option, its
  allowed values and defaults). It holds the last folder, include
  subfolders, extensions, date source + auto order, ambiguous date order,
  fallback, folder template, theme, font size and window size.
  It is rewritten when the app closes; edit it while the app is closed.
  Invalid values fall back to defaults; delete the file to reset.
  If the exe's folder is not writable, the app runs with defaults and
  shows a notice in the status bar.

NOTES / CAVEATS
  * "Modified date" and "Created date" are the file system times in local
    time. Copying files to a new PC often resets "Created" to the copy date;
    "Modified" usually survives. EXIF is read for JPEG/TIFF/HEIC/PNG/WebP
    when present (only used if selected or in the Auto order).
  * The exe is not code-signed. Windows SmartScreen may show "Windows
    protected your PC" on first run (More info > Run anyway), and some
    antivirus products occasionally flag unsigned Rust/MinGW programs.
    Check the SHA-256 below against the file you received.
  * Requires a graphics driver with OpenGL 2.0+ (any normal Windows 10/11
    PC). It may not start in some bare VMs / remote sessions without GPU
    drivers.
  * Windows' own folder picker may remember the last folder it showed;
    that is Windows behaviour, not PhotoSorter.

SHA-256 (PhotoSorter.exe)
  32493bed9de39503d2ea2e393489a4751328cf70cd2c2c770fd43a9e0c7beac5
