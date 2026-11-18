//! Opening the help file / folders with the default Windows app, safely.
//!
//! Only paths PhotoSorter builds itself are opened: `HELP.md` (or, if that
//! is missing, `README.txt`) next to the exe, the exe's folder, or the folder that holds an undo log. On
//! Windows the path goes to `ShellExecuteW(…, "open", path, NULL, …)`, which
//! opens it with the registered default app (Notepad for .txt, whatever is
//! registered for .md, Explorer for folders) - no `cmd /c start`, no command string, no parameters - and
//! only after checking it is an existing absolute file / folder. Fallback:
//! `notepad.exe <file>` / `explorer.exe <folder>` with one argument.
//!
//! [`monitor_work_areas`] lists the monitors (read-only) so a remembered
//! window position can be checked before the window is created.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const README_NAME: &str = "README.txt";
/// The help file shipped beside the exe (plain Markdown, readable in Notepad).
pub const HELP_NAME: &str = "HELP.md";

fn regular_file(p: PathBuf) -> Result<PathBuf, PathBuf> {
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.is_file() => Ok(p),
        _ => Err(p),
    }
}

/// The file Help > Help (F1) opens: `HELP.md` beside the exe, or
/// `README.txt` there if HELP.md is missing. `Err(HELP.md path)` when
/// neither exists as a regular file.
pub fn find_help(exe_dir: &Path) -> Result<PathBuf, PathBuf> {
    regular_file(exe_dir.join(HELP_NAME)).or_else(|help| find_readme(exe_dir).map_err(|_| help))
}

/// `README.txt` beside the exe if it exists as a regular file, otherwise
/// `Err(expected_path)`.
pub fn find_readme(exe_dir: &Path) -> Result<PathBuf, PathBuf> {
    regular_file(exe_dir.join(README_NAME))
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

/// Open an existing text file with its default app (Notepad fallback, e.g.
/// when no app is registered for `.md`).
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

/// Work areas of all connected monitors in logical pixels (physical / the
/// monitor's DPI scale), primary first. Empty if unknown (non-Windows).
pub fn monitor_work_areas() -> Vec<crate::window_state::MonitorRect> {
    #[cfg(windows)]
    {
        use crate::window_state::MonitorRect;
        use windows_sys::Win32::Foundation::{LPARAM, RECT};
        use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
        use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        const MONITORINFOF_PRIMARY: u32 = 1;

        unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> windows_sys::core::BOOL {
            // SAFETY: `data` is the &mut Vec passed to EnumDisplayMonitors
            // below, alive for the whole (synchronous) enumeration.
            let out = unsafe { &mut *(data as *mut Vec<(bool, MonitorRect)>) };
            let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            // SAFETY: valid monitor handle from the enumeration, sized struct.
            if unsafe { GetMonitorInfoW(m, &mut info) } != 0 {
                let (mut dx, mut dy) = (96u32, 96u32);
                // SAFETY: plain out-parameters.
                let ok = unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } == 0;
                let scale = if ok && dx >= 48 { dx as f32 / 96.0 } else { 1.0 };
                let r = info.rcWork;
                out.push((
                    info.dwFlags & MONITORINFOF_PRIMARY != 0,
                    MonitorRect {
                        x: r.left as f32 / scale,
                        y: r.top as f32 / scale,
                        w: (r.right - r.left) as f32 / scale,
                        h: (r.bottom - r.top) as f32 / scale,
                    },
                ));
            }
            1
        }

        let mut list: Vec<(bool, MonitorRect)> = Vec::new();
        // SAFETY: null HDC / clip rect enumerate every monitor; the callback
        // only writes into `list`, which outlives the call.
        unsafe {
            EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(cb), &mut list as *mut _ as LPARAM);
        }
        list.sort_by_key(|(primary, _)| !*primary);
        list.into_iter().map(|(_, r)| r).collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
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

    #[test]
    fn help_prefers_help_md_and_falls_back_to_readme() {
        let d = tempfile::tempdir().unwrap();
        // neither file: the error names HELP.md (the file we ship)
        assert_eq!(find_help(d.path()), Err(d.path().join("HELP.md")));
        // only README.txt (e.g. an older zip): fall back to it
        std::fs::write(d.path().join("README.txt"), b"readme").unwrap();
        assert_eq!(find_help(d.path()), Ok(d.path().join("README.txt")));
        // a folder named HELP.md is ignored
        std::fs::create_dir(d.path().join("HELP.md")).unwrap();
        assert_eq!(find_help(d.path()), Ok(d.path().join("README.txt")));
        std::fs::remove_dir(d.path().join("HELP.md")).unwrap();
        // HELP.md wins when present
        std::fs::write(d.path().join("HELP.md"), b"# Help").unwrap();
        assert_eq!(find_help(d.path()), Ok(d.path().join("HELP.md")));
        std::fs::remove_file(d.path().join("README.txt")).unwrap();
        assert_eq!(find_help(d.path()), Ok(d.path().join("HELP.md")));
    }
}
