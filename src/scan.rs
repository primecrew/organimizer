//! Folder scanning. Runs completely before any planning/moving, so folders
//! created later can never be re-visited in the same run.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::blacklist::Blacklist;
use crate::filetypes::{self, Filter};

/// Default extension list (images, camera RAW, videos).
pub const DEFAULT_EXTENSIONS: &str = "png, jpg, jpeg, gif, bmp, webp, tif, tiff, heic, heif, \
cr2, nef, arw, dng, \
mp4, mov, m4v, avi, mkv, wmv, webm, mpg, mpeg, mts, m2ts, 3gp";

/// v1 default list (images only); kept for reference / tests.
pub const V1_DEFAULT_EXTENSIONS: &str = "png, jpg, jpeg, gif, bmp, webp, tif, tiff, heic";

/// Hard safety limit: the ONLY extensions PhotoSorter will ever touch, no
/// matter what the user types. Anything else (txt, pdf, xmp, json sidecars,
/// documents...) is never scanned, moved or renamed.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    // images
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "avif", "jxl",
    // camera RAW
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef", "srw", "x3f", "3fr",
    "erf", "kdc", "mrw", "rwl", "iiq", "raw", // video
    "mp4", "mov", "m4v", "avi", "mkv", "wmv", "webm", "mpg", "mpeg", "mts", "m2ts", "3gp", "3g2", "flv", "vob",
];

pub fn is_media_extension(ext: &str) -> bool {
    MEDIA_EXTENSIONS.contains(&ext.trim_start_matches('.').to_ascii_lowercase().as_str())
}

fn split_list(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace() || c == '\\')
        .map(|e| e.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

/// Entries of a user list that are not image/video types (ignored).
pub fn rejected_extensions(s: &str) -> Vec<String> {
    let mut v: Vec<String> = split_list(s).filter(|e| !is_media_extension(e)).collect();
    v.sort();
    v.dedup();
    v
}

/// Parse a user-entered extension list (`"png, .JPG;tiff"`) into lowercase
/// extensions without dots. Only image/video types ([`MEDIA_EXTENSIONS`]) are
/// kept. Empty input (or nothing usable) yields the defaults.
pub fn parse_extensions(s: &str) -> Vec<String> {
    let mut v: Vec<String> = split_list(s).filter(|e| is_media_extension(e)).collect();
    v.sort();
    v.dedup();
    if v.is_empty() && s.trim() != DEFAULT_EXTENSIONS {
        return parse_extensions(DEFAULT_EXTENSIONS);
    }
    v
}

/// Short one-line description of a user-entered extension list for the main
/// window, e.g. `"26 image & video types"` or
/// `"3 image & video types, 2 ignored (not image/video)"`. Reports what
/// [`parse_extensions`] will actually use (including the fallback to the
/// defaults when nothing usable was entered).
pub fn extension_summary(s: &str) -> String {
    let used = parse_extensions(s).len();
    let typed_ok = split_list(s).any(|e| is_media_extension(&e));
    let rejected = rejected_extensions(s).len();
    let types = if used == 1 { "type" } else { "types" };
    if !typed_ok {
        if rejected > 0 {
            return format!("{used} default image & video {types} ({rejected} ignored: not image/video)");
        }
        return format!("{used} default image & video {types}");
    }
    if rejected > 0 {
        format!("{used} image & video {types}, {rejected} ignored (not image/video)")
    } else {
        format!("{used} image & video {types}")
    }
}

pub fn has_extension(path: &Path, exts: &[String]) -> bool {
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_ascii_lowercase();
            exts.contains(&e)
        })
        .unwrap_or(false)
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<PathBuf>,
    /// Blacklisted subfolders that were not entered (3.0).
    pub skipped_dirs: Vec<PathBuf>,
    pub errors: Vec<(PathBuf, String)>,
    pub dirs_scanned: usize,
}

/// Collect image / video files under `root` (the 2.x behavior): only the
/// listed extensions, and only image / video types.
pub fn scan(
    root: &Path,
    recursive: bool,
    exts: &[String],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, &Path),
) -> ScanResult {
    let media: Vec<String> = exts.iter().filter(|e| is_media_extension(e)).cloned().collect();
    scan_filtered(root, recursive, &Filter::Exts(media), &Blacklist { entries: vec![] }, cancel, progress)
}

/// Collect the files under `root` that `filter` accepts (3.0).
///
/// Never returned, whatever the filter: protected names (programs, scripts,
/// shortcuts, system files, undo logs), hidden or system-flagged files.
/// Never entered: symlinks / junctions, hidden or system folders,
/// version-control folders (`.git` ...) and blacklisted folders (each one
/// skipped is listed in `skipped_dirs`).
pub fn scan_filtered(
    root: &Path,
    recursive: bool,
    filter: &Filter,
    blacklist: &Blacklist,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, &Path),
) -> ScanResult {
    let mut res = ScanResult::default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        res.dirs_scanned += 1;
        progress(res.files.len(), &dir);
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                res.errors.push((dir.clone(), format!("Cannot read folder: {e}")));
                continue;
            }
        };
        let mut subdirs = Vec::new();
        for entry in rd {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    res.errors.push((dir.clone(), format!("Cannot read entry: {e}")));
                    continue;
                }
            };
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    res.errors.push((entry.path(), format!("Cannot read type: {e}")));
                    continue;
                }
            };
            let path = entry.path();
            if ft.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let hidden = || entry.metadata().map(|m| filetypes::is_hidden_or_system(&path, &m)).unwrap_or(true);
            if ft.is_dir() {
                if !recursive {
                    continue;
                }
                if filetypes::is_protected_dir_name(&name) || hidden() {
                    continue;
                }
                if blacklist.is_blocked(&path) {
                    res.skipped_dirs.push(path);
                    continue;
                }
                subdirs.push(path);
            } else if ft.is_file() && filter.accepts(&name) && !hidden() {
                res.files.push(path);
            }
        }
        subdirs.sort();
        // reverse so that pop() visits in alphabetical order
        stack.extend(subdirs.into_iter().rev());
    }
    res.files.sort();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_parsing() {
        assert_eq!(parse_extensions("PNG, .jpg;*.Tiff  heic"), vec!["heic", "jpg", "png", "tiff"]);
        // v2: the default list also covers RAW and video formats
        assert_eq!(parse_extensions("").len(), 26);
        assert_eq!(parse_extensions(V1_DEFAULT_EXTENSIONS).len(), 9);
        assert!(has_extension(Path::new("a/B.JPG"), &parse_extensions("jpg")));
        assert!(!has_extension(Path::new("a/B.jpg.txt"), &parse_extensions("jpg")));
        assert!(!has_extension(Path::new("a/jpg"), &parse_extensions("jpg")));
    }

    #[test]
    fn non_media_extensions_are_never_accepted() {
        assert_eq!(parse_extensions("jpg, txt, pdf, xmp, json, exe"), vec!["jpg"]);
        assert_eq!(rejected_extensions("jpg, TXT, .pdf, xmp"), vec!["pdf", "txt", "xmp"]);
        // only non-media -> defaults (never an empty or unsafe list)
        assert_eq!(parse_extensions("txt"), parse_extensions(""));
        for e in parse_extensions("") {
            assert!(is_media_extension(&e), "{e}");
        }
        for e in ["mp4", "MOV", ".mkv", "cr2", "dng", "heif"] {
            assert!(is_media_extension(e), "{e}");
        }
        // a forged list cannot make scan pick up sidecars
        let d = tempfile::tempdir().unwrap();
        for f in ["a.jpg", "a.xmp", "a.json", "b.mp4", "c.txt", "d.pdf"] {
            std::fs::write(d.path().join(f), b"x").unwrap();
        }
        let forged = vec!["jpg".to_string(), "xmp".into(), "json".into(), "mp4".into(), "txt".into()];
        let r = scan(d.path(), true, &forged, &AtomicBool::new(false), &mut |_, _| {});
        let names: Vec<_> = r.files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["a.jpg", "b.mp4"]);
    }

    #[test]
    fn extension_summary_counts_effective_types() {
        assert_eq!(parse_extensions(DEFAULT_EXTENSIONS).len(), 26);
        assert_eq!(extension_summary(DEFAULT_EXTENSIONS), "26 image & video types");
        assert_eq!(extension_summary("jpg"), "1 image & video type");
        assert_eq!(extension_summary("jpg, .PNG; mp4, jpg"), "3 image & video types");
        assert_eq!(extension_summary("jpg, txt, pdf"), "1 image & video type, 2 ignored (not image/video)");
        // nothing usable -> the defaults are used, and the summary says so
        assert_eq!(extension_summary(""), "26 default image & video types");
        assert_eq!(extension_summary("txt, exe"), "26 default image & video types (2 ignored: not image/video)");
    }

    #[test]
    fn scan_recursive_and_flat() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("sub/deeper")).unwrap();
        for f in ["a.jpg", "b.TXT", "sub/c.PNG", "sub/deeper/d.heic"] {
            std::fs::write(d.path().join(f), b"x").unwrap();
        }
        let exts = parse_extensions(DEFAULT_EXTENSIONS);
        let c = AtomicBool::new(false);
        let r = scan(d.path(), true, &exts, &c, &mut |_, _| {});
        assert_eq!(r.files.len(), 3);
        let r = scan(d.path(), false, &exts, &c, &mut |_, _| {});
        assert_eq!(r.files.len(), 1);
    }

    #[test]
    fn default_settings_scan_only_top_folder() {
        // 2.0.4: Include subfolders is off by default -> non-recursive scan
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("2023/Trip/deep")).unwrap();
        for f in ["top1.jpg", "top2.png", "2023/a.jpg", "2023/Trip/b.heic", "2023/Trip/deep/c.mp4"] {
            std::fs::write(d.path().join(f), b"x").unwrap();
        }
        let s = crate::settings::Settings::default();
        let exts = parse_extensions(&s.extensions);
        let c = AtomicBool::new(false);
        let r = scan(d.path(), s.include_subfolders, &exts, &c, &mut |_, _| {});
        let mut names: Vec<String> =
            r.files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["top1.jpg", "top2.png"]);
        assert!(r.files.iter().all(|p| p.parent() == Some(d.path())));
        assert_eq!(r.dirs_scanned, 1);
        // ticking the box finds the nested files too
        let r = scan(d.path(), true, &exts, &c, &mut |_, _| {});
        assert_eq!(r.files.len(), 5);
    }

    #[test]
    fn scan_filtered_protections_and_blacklist() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for f in [
            "a.jpg", "song.mp3", "doc.pdf", "setup.exe", "run.bat", "link.lnk", "desktop.ini", "Thumbs.db",
            ".hidden.jpg", "README", "PhotoSorter-undo-20250101-000000.json", "sub/b.mp4", "sub/x.dll",
            ".git/config.txt", ".git/obj.jpg", "keep/c.txt", "blocked/d.jpg", "blocked/deep/e.jpg",
        ] {
            let p = r.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        let c = AtomicBool::new(false);
        let names = |res: &ScanResult| -> Vec<String> {
            res.files.iter().map(|p| p.strip_prefix(r).unwrap().to_string_lossy().replace('\\', "/")).collect()
        };
        // All files: everything safe, nothing protected
        let bl = Blacklist { entries: vec![r.join("blocked").to_string_lossy().replace('/', "\\")] };
        let res = scan_filtered(r, true, &Filter::All, &bl, &c, &mut |_, _| {});
        assert_eq!(names(&res), ["a.jpg", "doc.pdf", "keep/c.txt", "song.mp3", "sub/b.mp4"]);
        assert_eq!(res.skipped_dirs, vec![r.join("blocked")]);
        // a forged list cannot pick up protected types either
        let forged = Filter::Exts(vec!["exe".into(), "lnk".into(), "ini".into(), "db".into(), "jpg".into()]);
        let res = scan_filtered(r, false, &forged, &Blacklist { entries: vec![] }, &c, &mut |_, _| {});
        assert_eq!(names(&res), ["a.jpg"]);
        // the 2.x entry point still only returns images / videos
        let res = scan(r, true, &["jpg".into(), "mp3".into(), "mp4".into()], &c, &mut |_, _| {});
        assert_eq!(names(&res), ["a.jpg", "blocked/d.jpg", "blocked/deep/e.jpg", "sub/b.mp4"]);
    }
}
