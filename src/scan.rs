//! Folder scanning. Runs completely before any planning/moving, so folders
//! created later can never be re-visited in the same run.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_EXTENSIONS: &str = "png, jpg, jpeg, gif, bmp, webp, tif, tiff, heic";

/// Parse a user-entered extension list (`"png, .JPG;tiff"`) into lowercase
/// extensions without dots. Empty input yields the defaults.
pub fn parse_extensions(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(|e| e.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
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
            } else if ft.is_file() && has_extension(&path, exts) {
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
        assert_eq!(parse_extensions("").len(), 9);
        assert!(has_extension(Path::new("a/B.JPG"), &parse_extensions("jpg")));
        assert!(!has_extension(Path::new("a/B.jpg.txt"), &parse_extensions("jpg")));
        assert!(!has_extension(Path::new("a/jpg"), &parse_extensions("jpg")));
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
