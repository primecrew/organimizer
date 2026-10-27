//! Opening the README / folders with the default Windows app, safely.
//!
//! Only paths PhotoSorter builds itself are opened: `README.txt` next to
//! the exe, the exe's folder, or the folder that holds an undo log. On
//! Windows the path goes to `ShellExecuteW(…, "open", path, NULL, …)`, which
//! opens it with the registered default app (Notepad for .txt, Explorer for
//! folders) - no `cmd /c start`, no command string, no parameters - and
//! only after checking it is an existing absolute file / folder. Fallback:
//! `notepad.exe <file>` / `explorer.exe <folder>` with one argument.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const README_NAME: &str = "README.txt";

/// `README.txt` beside the exe if it exists as a regular file, otherwise
/// `Err(expected_path)`.
pub fn find_readme(exe_dir: &Path) -> Result<PathBuf, PathBuf> {
    let p = exe_dir.join(README_NAME);
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.is_file() => Ok(p),
        _ => Err(p),
    }
}

#[cfg(windows)]
fn shell_open(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide(std::ffi::OsStr::new("open"));
    let file = wide(path.as_os_str());
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the
    // call; null HWND / parameters / directory are allowed by the API.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    r as usize > 32
}

#[cfg(windows)]
fn explorer() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("explorer.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("explorer.exe"))
}

/// Open an existing text file with its default app (Notepad fallback).
pub fn open_text_file(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{} not found", path.display())));
    }
    #[cfg(windows)]
    {
        if shell_open(path) {
            return Ok(());
        }
        Command::new("notepad.exe").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}

/// Open an existing folder in Explorer.
pub fn open_folder(dir: &Path) -> io::Result<()> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{} not found", dir.display())));
    }
    #[cfg(windows)]
    {
        if shell_open(dir) {
            return Ok(());
        }
        Command::new(explorer()).arg(dir).spawn().map(|_| ())
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open").arg(dir).spawn().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_is_found_only_next_to_exe() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(find_readme(d.path()), Err(d.path().join("README.txt")));
        std::fs::create_dir(d.path().join("README.txt")).unwrap(); // a folder is not a README
        assert!(find_readme(d.path()).is_err());
        std::fs::remove_dir(d.path().join("README.txt")).unwrap();
        std::fs::write(d.path().join("README.txt"), b"hi").unwrap();
        assert_eq!(find_readme(d.path()), Ok(d.path().join("README.txt")));
        // never opens relative or missing paths
        assert!(open_text_file(Path::new("README.txt")).is_err());
        assert!(open_text_file(&d.path().join("missing.txt")).is_err());
        assert!(open_folder(&d.path().join("nope")).is_err());
    }
}
