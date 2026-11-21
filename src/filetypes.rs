//! File-type groups (3.0) and the hard protections that apply in every
//! mode, including "All files".
//!
//! * A [`TypeGroup`] decides which extensions a scan picks up. The default,
//!   [`TypeGroup::Media`], is exactly the 2.x behavior (the image & video
//!   list from Tools > Options).
//! * [`is_protected_name`] / [`is_protected_ext`] list what is NEVER
//!   touched, whatever the user picks: programs, scripts, shortcuts, system
//!   and settings files, undo logs. Hidden or system-flagged files and
//!   version-control folders (`.git`, `.svn`, ...) are skipped by the scan.

use std::path::Path;

use crate::scan;

/// Which files a scan picks up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypeGroup {
    /// Images & videos from the editable list in Tools > Options (2.x default).
    #[default]
    Media,
    Images,
    Videos,
    Audio,
    Documents,
    Archives,
    /// Every file that is not protected.
    All,
    /// The custom extension list from Tools > Options.
    Custom,
}

impl TypeGroup {
    pub const ALL: [TypeGroup; 8] = [
        TypeGroup::Media,
        TypeGroup::Images,
        TypeGroup::Videos,
        TypeGroup::Audio,
        TypeGroup::Documents,
        TypeGroup::Archives,
        TypeGroup::All,
        TypeGroup::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TypeGroup::Media => "media",
            TypeGroup::Images => "images",
            TypeGroup::Videos => "videos",
            TypeGroup::Audio => "audio",
            TypeGroup::Documents => "documents",
            TypeGroup::Archives => "archives",
            TypeGroup::All => "all",
            TypeGroup::Custom => "custom",
        }
    }

    pub fn from_key(s: &str) -> Option<TypeGroup> {
        let k = s.trim().to_ascii_lowercase();
        TypeGroup::ALL.into_iter().find(|g| g.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            TypeGroup::Media => "Images & videos",
            TypeGroup::Images => "Images",
            TypeGroup::Videos => "Videos",
            TypeGroup::Audio => "Audio",
            TypeGroup::Documents => "Documents",
            TypeGroup::Archives => "Archives",
            TypeGroup::All => "All files",
            TypeGroup::Custom => "Custom list",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            TypeGroup::Media => "Images, camera RAW and videos from the list in Tools > Options (the 2.x default)",
            TypeGroup::Images => "Photos, pictures and camera RAW files",
            TypeGroup::Videos => "Video files",
            TypeGroup::Audio => "Music and sound files",
            TypeGroup::Documents => "Text, office, PDF and e-book files",
            TypeGroup::Archives => "Zip, 7z, RAR, disc images and other archives",
            TypeGroup::All => {
                "Every file except programs, scripts, shortcuts, system and hidden files (those are never touched)"
            }
            TypeGroup::Custom => "The custom extension list in Tools > Options",
        }
    }
}

pub const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "avif", "jxl", "cr2",
    "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef", "srw", "x3f", "3fr", "erf",
    "kdc", "mrw", "rwl", "iiq", "raw", "psd", "svg", "ico",
];
pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "wmv", "webm", "mpg", "mpeg", "mts", "m2ts", "ts", "3gp", "3g2", "flv", "vob",
    "ogv", "divx", "xvid", "rmvb", "asf",
];
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "oga", "opus", "wma", "aiff", "aif", "alac", "ape", "mka", "mid",
    "midi", "m4b", "wv", "dsf",
];
pub const DOCUMENT_EXTS: &[&str] = &[
    "pdf", "txt", "md", "rtf", "doc", "docx", "odt", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp", "epub", "mobi",
    "azw3", "djvu", "xps", "pages", "numbers", "key", "srt", "ass", "ssa", "sub", "vtt", "nfo",
];
pub const ARCHIVE_EXTS: &[&str] =
    &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "img", "cab", "lz", "lzma", "cbz", "cbr"];

/// Never touched in any mode: programs, libraries, drivers, installers,
/// scripts, shortcuts and other files Windows (or another program) runs or
/// depends on.
pub const PROTECTED_EXTS: &[&str] = &[
    "exe",
    "dll",
    "sys",
    "drv",
    "ocx",
    "cpl",
    "scr",
    "com",
    "pif",
    "msi",
    "msp",
    "msu",
    "mst",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "psd1",
    "ps1xml",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "wsc",
    "hta",
    "sct",
    "lnk",
    "url",
    "inf",
    "reg",
    "ini",
    "msc",
    "jar",
    "gadget",
    "application",
    "efi",
    "mui",
    "cat",
    "manifest",
    "ax",
    "tlb",
    "job",
    "sh",
    "library-ms",
    "searchconnector-ms",
    "settingcontent-ms",
    "installed",
    "portable",
];

/// Exact file names (case-insensitive) that are never touched.
pub const PROTECTED_NAMES: &[&str] = &[
    "desktop.ini",
    "thumbs.db",
    "ehthumbs.db",
    "ehthumbs_vista.db",
    "iconcache.db",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "bootmgr",
    "bootnxt",
    "autorun.inf",
    ".ds_store",
    "organimizer.ini",
    "organimizer.ini.bak",
    "organimizer.ini.tmp",
    "photosorter.ini",
    "photosorter.ini.bak",
    "photosorter.ini.tmp",
];

/// File-name prefixes (case-insensitive) that are never touched: Windows
/// user-registry hives and this app's own undo logs.
pub const PROTECTED_PREFIXES: &[&str] =
    &["ntuser.", "usrclass.", "organimizer-undo-", "photosorter-undo-", ".organimizer-", ".photosorter-"];

/// Folder names that are never entered (version control, recycle bin...).
pub const PROTECTED_DIR_NAMES: &[&str] =
    &[".git", ".svn", ".hg", ".bzr", "_darcs", "cvs", "$recycle.bin", "system volume information", "$windows.~bt"];

fn ext_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

pub fn is_protected_ext(ext: &str) -> bool {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    PROTECTED_EXTS.contains(&e.as_str())
}

/// True if a file with this name must never be scanned, renamed or moved.
pub fn is_protected_name(name: &str) -> bool {
    let lower = name.trim_end_matches([' ', '.']).to_ascii_lowercase();
    if PROTECTED_NAMES.contains(&lower.as_str()) || PROTECTED_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    // every dot-separated part counts: "setup.exe.jpg" stays allowed, but a
    // double extension like "photo.jpg.exe" or "x.lnk" is protected
    ext_of(&lower).is_some_and(|e| is_protected_ext(&e))
}

pub fn is_protected_dir_name(name: &str) -> bool {
    PROTECTED_DIR_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// Hidden or system-flagged (Windows attributes); elsewhere dot-files.
pub fn is_hidden_or_system(path: &Path, meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        let _ = path;
        meta.file_attributes() & (HIDDEN | SYSTEM) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
    }
}

fn split_list(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(|e| e.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty() && !e.contains(['\\', '/', ':', '*', '?']))
}

/// Parse the custom extension list: (accepted, refused-because-protected).
pub fn parse_custom(s: &str) -> (Vec<String>, Vec<String>) {
    let (mut ok, mut bad): (Vec<String>, Vec<String>) = split_list(s).partition(|e| !is_protected_ext(e));
    ok.sort();
    ok.dedup();
    bad.sort();
    bad.dedup();
    (ok, bad)
}

/// What a scan accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Only these lower-case extensions (no dots).
    Exts(Vec<String>),
    /// Any file with an extension that is not protected.
    All,
}

impl Filter {
    /// Build the filter for a group. `media_list` is the 2.x image & video
    /// list, `custom` the custom extension list.
    pub fn for_group(group: TypeGroup, media_list: &str, custom: &str) -> Filter {
        let list = |l: &[&str]| Filter::Exts(l.iter().map(|s| s.to_string()).collect());
        match group {
            TypeGroup::Media => Filter::Exts(scan::parse_extensions(media_list)),
            TypeGroup::Images => list(IMAGE_EXTS),
            TypeGroup::Videos => list(VIDEO_EXTS),
            TypeGroup::Audio => list(AUDIO_EXTS),
            TypeGroup::Documents => list(DOCUMENT_EXTS),
            TypeGroup::Archives => list(ARCHIVE_EXTS),
            TypeGroup::All => Filter::All,
            TypeGroup::Custom => Filter::Exts(parse_custom(custom).0),
        }
    }

    /// Does the filter accept this file name? Protected names never pass.
    /// Files without an extension are skipped in every mode.
    pub fn accepts(&self, name: &str) -> bool {
        if is_protected_name(name) {
            return false;
        }
        let Some(ext) = ext_of(name) else { return false };
        match self {
            Filter::All => true,
            Filter::Exts(v) => v.contains(&ext),
        }
    }

    /// Extensions the rename stage may rename (`["*"]` = any unprotected).
    pub fn rename_exts(&self) -> Vec<String> {
        match self {
            Filter::All => vec!["*".into()],
            Filter::Exts(v) => v.clone(),
        }
    }

    /// Short description for the main window.
    pub fn summary(&self, group: TypeGroup) -> String {
        match self {
            Filter::All => "All files (programs, scripts, system and hidden files excluded)".into(),
            Filter::Exts(v) if v.is_empty() => {
                format!("{}: no extensions - add some in Tools > Options", group.label())
            }
            Filter::Exts(v) => {
                let t = if v.len() == 1 { "type" } else { "types" };
                format!("{}: {} {t}", group.label(), v.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_types_never_pass_any_filter() {
        let all = Filter::All;
        let forged = Filter::Exts(vec!["exe".into(), "lnk".into(), "jpg".into(), "ini".into(), "bat".into()]);
        for n in [
            "setup.exe",
            "LIB.DLL",
            "driver.sys",
            "x.msi",
            "run.bat",
            "run.CMD",
            "s.ps1",
            "s.vbs",
            "a.com",
            "s.scr",
            "Shortcut.lnk",
            "site.url",
            "desktop.ini",
            "Thumbs.db",
            "DESKTOP.INI",
            "NTUSER.DAT",
            "ntuser.dat.LOG1",
            "photo.jpg.exe",
            "Organimizer.ini",
            "PhotoSorter.ini",
            "Organimizer-undo-20260101-000000.json",
            "PhotoSorter-undo-20250101-000000.json",
            "pagefile.sys",
            "autorun.inf",
            "evil.exe. . ",
            "x.reg",
        ] {
            assert!(is_protected_name(n), "{n}");
            assert!(!all.accepts(n), "{n}");
            assert!(!forged.accepts(n), "{n}");
        }
        for n in ["holiday.jpg", "song.MP3", "report.pdf", "setup.exe.jpg", "notes.txt", "a.zip", "movie.mkv"] {
            assert!(!is_protected_name(n), "{n}");
            assert!(all.accepts(n), "{n}");
        }
        // no extension -> skipped even in All mode
        assert!(!all.accepts("README") && !all.accepts(".hidden"));
        for d in [".git", ".SVN", ".hg", "$RECYCLE.BIN", "System Volume Information"] {
            assert!(is_protected_dir_name(d), "{d}");
        }
        assert!(!is_protected_dir_name("git") && !is_protected_dir_name("Photos"));
    }

    #[test]
    fn groups_and_custom_list() {
        assert_eq!(Filter::for_group(TypeGroup::Media, "", ""), Filter::Exts(scan::parse_extensions("")));
        assert!(Filter::for_group(TypeGroup::Audio, "", "").accepts("a.FLAC"));
        assert!(!Filter::for_group(TypeGroup::Audio, "", "").accepts("a.jpg"));
        assert!(Filter::for_group(TypeGroup::Documents, "", "").accepts("a.pdf"));
        assert!(Filter::for_group(TypeGroup::Archives, "", "").accepts("a.7z"));
        let (ok, bad) = parse_custom("PSD, .ai; *.exe  blend, lnk, psd");
        assert_eq!(ok, vec!["ai", "blend", "psd"]);
        assert_eq!(bad, vec!["exe", "lnk"]);
        let f = Filter::for_group(TypeGroup::Custom, "", "blend, exe");
        assert!(f.accepts("x.blend") && !f.accepts("x.exe"));
        for g in TypeGroup::ALL {
            assert_eq!(TypeGroup::from_key(g.key()), Some(g));
        }
        assert_eq!(TypeGroup::from_key("bogus"), None);
        for l in [IMAGE_EXTS, VIDEO_EXTS, AUDIO_EXTS, DOCUMENT_EXTS, ARCHIVE_EXTS] {
            assert!(l.iter().all(|e| !is_protected_ext(e)));
        }
    }
}
