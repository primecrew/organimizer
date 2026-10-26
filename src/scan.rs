//! Folder scanning. Runs completely before any planning/moving, so folders
//! created later can never be re-visited in the same run.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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
    "erf", "kdc", "mrw", "rwl", "iiq", "raw",
    // video
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
    pub errors: Vec<(PathBuf, String)>,
    pub dirs_scanned: usize,
}

/// Collect image files under `root`. Symlinks / junctions are never followed
/// (avoids loops and escaping the chosen folder).
pub fn scan(
    root: &Path,
    recursive: bool,
    exts: &[String],
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
            if ft.is_dir() {
                if recursive {
                    subdirs.push(path);
                }
            } else if ft.is_file() && has_extension(&path, exts) && path.extension().is_some_and(|e| is_media_extension(&e.to_string_lossy())) {
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
}
