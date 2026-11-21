//! Folder blacklist (3.0): system folders that can never be chosen and are
//! skipped while scanning subfolders.
//!
//! Each entry is a Windows path pattern. Matching is case-insensitive and
//! works on whole path components, so `C:\Windows` blocks `C:\Windows` and
//! everything below it but not `C:\WindowsStuff`. A component that is just
//! `*` matches any one folder name (`C:\Users\*\AppData`), and `*:` matches
//! any drive (`*:\$Recycle.Bin`). The special entry [`DRIVE_ROOTS`] blocks
//! choosing a drive root itself (`D:\`) without blocking the folders on it.
//!
//! Defaults are resolved from the Windows known-folder environment variables
//! (`SystemRoot`, `ProgramFiles`, `ProgramData`, `USERPROFILE`, ...), never
//! from hard-coded user names.

use std::path::Path;

/// Special entry: every drive root (`C:\`, `D:\`, `\\server\share\`).
pub const DRIVE_ROOTS: &str = "<drive roots>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blacklist {
    pub entries: Vec<String>,
}

impl Default for Blacklist {
    fn default() -> Self {
        Blacklist { entries: default_entries(&|k| std::env::var(k).ok()) }
    }
}

fn clean_env(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().trim_end_matches(['\\', '/']).to_string())
        .filter(|s| s.len() >= 2 && s.as_bytes()[1] == b':' || s.starts_with("\\\\"))
}

/// Default entries for the given environment (`env("ProgramFiles")` ...).
pub fn default_entries(env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let get = |k: &str| clean_env(env(k));
    let sys_drive = get("SystemDrive").filter(|d| d.len() == 2).unwrap_or_else(|| "C:".into());
    let windir = get("SystemRoot").or_else(|| get("windir")).unwrap_or_else(|| format!("{sys_drive}\\Windows"));
    let pf = get("ProgramFiles").unwrap_or_else(|| format!("{sys_drive}\\Program Files"));
    let pf86 = get("ProgramFiles(x86)").unwrap_or_else(|| format!("{sys_drive}\\Program Files (x86)"));
    let pf64 = get("ProgramW6432");
    let pdata =
        get("ProgramData").or_else(|| get("ALLUSERSPROFILE")).unwrap_or_else(|| format!("{sys_drive}\\ProgramData"));
    // folder that holds the user profiles (normally C:\Users)
    let profiles = get("USERPROFILE")
        .or_else(|| get("PUBLIC"))
        .and_then(|p| Path::new(&p.replace('/', "\\")).parent().map(|x| x.to_string_lossy().into_owned()))
        .map(|s| s.replace('/', "\\"))
        .filter(|s| s.len() > 3)
        .unwrap_or_else(|| format!("{sys_drive}\\Users"));
    let mut v: Vec<String> = vec![DRIVE_ROOTS.into(), windir, format!("{sys_drive}\\Windows.old"), pf, pf86];
    if let Some(p) = pf64 {
        v.push(p);
    }
    v.extend([
        pdata,
        format!("{profiles}\\*\\AppData"),
        format!("{profiles}\\Default"),
        format!("{profiles}\\Default User"),
        format!("{profiles}\\All Users"),
        format!("{sys_drive}\\Documents and Settings"),
        format!("{sys_drive}\\Recovery"),
        format!("{sys_drive}\\Boot"),
        format!("{sys_drive}\\PerfLogs"),
        format!("{sys_drive}\\Config.Msi"),
        format!("{sys_drive}\\$WinREAgent"),
        format!("{sys_drive}\\$SysReset"),
        format!("{sys_drive}\\$Windows.~BT"),
        format!("{sys_drive}\\$Windows.~WS"),
        format!("{sys_drive}\\$GetCurrent"),
        format!("{sys_drive}\\OneDriveTemp"),
        "*:\\$Recycle.Bin".into(),
        "*:\\System Volume Information".into(),
        "*:\\Recovery".into(),
        "*:\\EFI".into(),
    ]);
    // the current user's AppData even if it was moved off the profile folder
    for k in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(p) = get(k) {
            v.push(p);
        }
    }
    let mut seen = std::collections::HashSet::new();
    v.retain(|e| seen.insert(e.to_ascii_lowercase()));
    v
}

/// Lower-case path components of a Windows-style path. The first element is
/// the root (`c:` or `\\server\share`), or "" for a relative path.
fn components(path: &str) -> Vec<String> {
    let s = path.trim().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\UNC\").map(|r| format!(r"\\{r}")).unwrap_or(s);
    let s = s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s);
    let lower = s.to_lowercase();
    if let Some(rest) = lower.strip_prefix(r"\\") {
        let mut parts = rest.split('\\').filter(|p| !p.is_empty());
        let server = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        let mut v = vec![format!(r"\\{server}\{share}")];
        v.extend(parts.filter(|p| *p != ".").map(|p| p.trim_end_matches([' ', '.']).to_string()));
        return v;
    }
    let mut parts = lower.split('\\');
    let mut first = parts.next().unwrap_or_default().to_string();
    if first.is_empty() && lower.starts_with('\\') {
        // rooted path without a drive (\folder, or /tmp on Linux)
        first = "\\".into();
    }
    let mut v = vec![first];
    for p in parts {
        if p.is_empty() || p == "." {
            continue;
        }
        if p == ".." {
            if v.len() > 1 {
                v.pop();
            }
            continue;
        }
        // Windows ignores trailing dots / spaces in names
        v.push(p.trim_end_matches([' ', '.']).to_string());
    }
    v
}

/// A drive (`c:`, `*:`), UNC share or `\` root, not a relative name.
fn is_rooted(first: &str) -> bool {
    first.starts_with('\\') || (first.len() == 2 && first.ends_with(':'))
}

fn comp_matches(pat: &str, c: &str) -> bool {
    pat == "*" || (pat == "*:" && c.len() == 2 && c.ends_with(':')) || pat == c
}

/// Is this path a drive root (`C:\`) or a bare UNC share (`\\srv\share`)?
pub fn is_drive_root(path: &str) -> bool {
    let c = components(path);
    c.len() == 1 && is_rooted(&c[0])
}

impl Blacklist {
    /// The entry that blocks `path`, if any.
    pub fn blocked_by(&self, path: &str) -> Option<&str> {
        let pc = components(path);
        if pc.len() == 1 && pc[0].is_empty() {
            return None;
        }
        for e in &self.entries {
            let e_trim = e.trim();
            if e_trim.is_empty() {
                continue;
            }
            if e_trim.eq_ignore_ascii_case(DRIVE_ROOTS) {
                if is_drive_root(path) {
                    return Some(e);
                }
                continue;
            }
            let ec = components(e_trim);
            if ec.len() < 2 || !is_rooted(&ec[0]) {
                // drive-only or relative entries would block too much: ignore
                continue;
            }
            if pc.len() >= ec.len() && ec.iter().zip(&pc).all(|(p, c)| comp_matches(p, c)) {
                return Some(e);
            }
        }
        None
    }

    pub fn is_blocked(&self, path: &Path) -> bool {
        self.blocked_by(&path.to_string_lossy()).is_some()
    }

    /// Validate an entry typed in the editor.
    pub fn validate_entry(entry: &str) -> Result<String, String> {
        let e = entry.trim().trim_matches('"').trim().to_string();
        if e.is_empty() {
            return Err("Enter a folder path".into());
        }
        if e.eq_ignore_ascii_case(DRIVE_ROOTS) {
            return Ok(DRIVE_ROOTS.into());
        }
        let c = components(&e);
        if !is_rooted(&c[0]) {
            return Err("Use a full path such as C:\\Folder (or *:\\Folder for every drive)".into());
        }
        if c.len() < 2 {
            return Err(format!("Use “{DRIVE_ROOTS}” to block drive roots; a whole drive cannot be blocked"));
        }
        if c.iter().skip(1).any(|p| p.contains(['*', '?']) && p != "*") {
            return Err("Wildcards: only a whole folder name can be * (e.g. C:\\Users\\*\\AppData)".into());
        }
        Ok(e.trim_end_matches(['\\', '/']).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(k: &str) -> Option<String> {
        Some(
            match k {
                "SystemDrive" => "C:",
                "SystemRoot" => "C:\\WINDOWS",
                "ProgramFiles" => "C:\\Program Files",
                "ProgramFiles(x86)" => "C:\\Program Files (x86)",
                "ProgramW6432" => "C:\\Program Files",
                "ProgramData" => "C:\\ProgramData",
                "USERPROFILE" => "C:\\Users\\someone",
                "APPDATA" => "C:\\Users\\someone\\AppData\\Roaming",
                "LOCALAPPDATA" => "D:\\Moved\\Local",
                _ => return None,
            }
            .to_string(),
        )
    }

    fn bl() -> Blacklist {
        Blacklist { entries: default_entries(&env) }
    }

    #[test]
    fn defaults_come_from_environment() {
        let e = default_entries(&env);
        assert!(e.contains(&"C:\\WINDOWS".to_string()));
        assert!(e.contains(&"C:\\Users\\*\\AppData".to_string()));
        assert!(e.contains(&"D:\\Moved\\Local".to_string()));
        assert_eq!(e.iter().filter(|x| x.eq_ignore_ascii_case("C:\\Program Files")).count(), 1);
        assert!(!e.iter().any(|x| x.contains("someone\\") && !x.contains("AppData")));
        // no environment at all -> sensible C: defaults
        let e2 = default_entries(&|_| None);
        assert!(e2.contains(&"C:\\Windows".to_string()) && e2.contains(&"C:\\Users\\*\\AppData".to_string()));
    }

    #[test]
    fn blocks_system_folders_case_insensitively() {
        let b = bl();
        for p in [
            "C:\\Windows",
            "c:\\windows\\System32",
            "C:/WINDOWS/temp/",
            "C:\\Program Files\\App",
            "C:\\PROGRAM FILES (X86)",
            "C:\\ProgramData\\x",
            "C:\\Users\\someone\\AppData\\Local\\Temp",
            "C:\\Users\\Other Person\\AppData",
            "D:\\$Recycle.Bin",
            "E:\\System Volume Information",
            "C:\\",
            "D:\\",
            "C:",
            "\\\\nas\\media",
            "C:\\Recovery",
            "C:\\Windows.\\",
            "C:\\Photos\\..\\Windows",
            "\\\\?\\C:\\Windows\\x",
            "D:\\Moved\\Local\\cache",
        ] {
            assert!(b.blocked_by(p).is_some(), "{p} should be blocked");
        }
    }

    #[test]
    fn prefix_safe_and_normal_folders_allowed() {
        let b = bl();
        for p in [
            "C:\\WindowsStuff",
            "C:\\Windows Photos",
            "C:\\Program Files Backup",
            "C:\\Users\\someone\\Pictures",
            "C:\\Users\\someone\\AppDataBackup",
            "D:\\Photos",
            "D:\\Media\\Movies",
            "\\\\nas\\media\\Movies",
            "C:\\Users",
            "",
        ] {
            assert!(b.blocked_by(p).is_none(), "{p} should be allowed");
        }
    }

    #[test]
    fn editor_validation() {
        assert_eq!(Blacklist::validate_entry(" \"D:\\Keep\\\" "), Ok("D:\\Keep".into()));
        assert!(Blacklist::validate_entry("").is_err());
        assert!(Blacklist::validate_entry("D:\\").is_err());
        assert!(Blacklist::validate_entry("relative\\path").is_err());
        assert!(Blacklist::validate_entry("D:\\a*b").is_err());
        assert_eq!(Blacklist::validate_entry("*:\\Temp"), Ok("*:\\Temp".into()));
        assert_eq!(Blacklist::validate_entry("<DRIVE ROOTS>"), Ok(DRIVE_ROOTS.into()));
        // a user entry works like the defaults
        let b = Blacklist { entries: vec!["D:\\Keep".into(), "*:\\Temp".into()] };
        assert!(b.blocked_by("d:\\keep\\sub").is_some());
        assert!(b.blocked_by("X:\\TEMP").is_some());
        assert!(b.blocked_by("D:\\Keeper").is_none());
        assert!(b.blocked_by("D:\\").is_none(), "drive roots only blocked by the special entry");
    }
}
