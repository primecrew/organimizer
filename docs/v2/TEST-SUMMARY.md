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

## Sample data (downloaded on the build box)
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

## Not verified
Everything above ran on Linux or under Wine 10.0 only, never on real Windows
10/11 (NTFS, SmartScreen, Defender, high-DPI, OneDrive folders, network
drives and >260-char paths were not exercised).
