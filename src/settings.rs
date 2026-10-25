//! Portable settings in `PhotoSorter.ini` beside the executable.
//!
//! * The file is always (re)generated from a commented template in
//!   [`Settings::to_ini`], so comments survive every save.
//! * Parsing is lenient: unknown sections/keys are ignored, bad values fall
//!   back to defaults, nothing panics.
//! * Nothing is written to the registry or AppData.
//! * Future presets are planned as `[Preset:Name]` sections in the same file;
//!   [`parse_ini`] already returns every section so they can be added later.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::date_source::{DateConfig, DateMode, Fallback, SourceKind};
use crate::filename_date::DateOrder;
use crate::scan::DEFAULT_EXTENSIONS;
use crate::template::DEFAULT_TEMPLATE;

pub const INI_NAME: &str = "PhotoSorter.ini";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub last_folder: String,
    pub include_subfolders: bool,
    pub extensions: String,
    pub date: DateConfig,
    pub folder_template: String,
    pub theme: Theme,
    pub font_size: f32,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_folder: String::new(),
            include_subfolders: true,
            extensions: DEFAULT_EXTENSIONS.to_string(),
            date: DateConfig::default(),
            folder_template: DEFAULT_TEMPLATE.to_string(),
            theme: Theme::Dark,
            font_size: 15.0,
            window_width: 1200.0,
            window_height: 800.0,
        }
    }
}

/// section (lowercase) -> key (lowercase) -> raw value
pub type IniData = BTreeMap<String, BTreeMap<String, String>>;

/// Minimal lenient INI parser. `;` and `#` start comment lines.
pub fn parse_ini(text: &str) -> IniData {
    let mut data = IniData::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                section = line[1..end].trim().to_lowercase();
            }
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_lowercase();
            let mut val = line[eq + 1..].trim();
            if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
                val = &val[1..val.len() - 1];
            }
            if !key.is_empty() {
                data.entry(section.clone()).or_default().insert(key, val.to_string());
            }
        }
    }
    data
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_f32(s: &str, min: f32, max: f32) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v >= min && *v <= max)
}

impl Settings {
    pub fn from_ini(text: &str) -> Settings {
        let d = parse_ini(text);
        let mut s = Settings::default();
        let get = |sec: &str, key: &str| d.get(sec).and_then(|m| m.get(key)).map(String::as_str);

        if let Some(v) = get("general", "last_folder") {
            s.last_folder = v.to_string();
        }
        if let Some(v) = get("general", "include_subfolders").and_then(parse_bool) {
            s.include_subfolders = v;
        }
        if let Some(v) = get("general", "extensions")
            && !crate::scan::parse_extensions(v).is_empty()
            && !v.trim().is_empty()
        {
            s.extensions = v.to_string();
        }
        if let Some(v) = get("date", "source").and_then(DateMode::from_key) {
            s.date.mode = v;
        }
        if let Some(v) = get("date", "auto_order") {
            let mut order: Vec<SourceKind> = Vec::new();
            for k in v.split([',', '>', ';']).filter_map(SourceKind::from_key) {
                if !order.contains(&k) {
                    order.push(k);
                }
            }
            if !order.is_empty() {
                s.date.auto_order = order;
            }
        }
        if let Some(v) = get("date", "ambiguous_order") {
            match v.trim().to_ascii_lowercase().as_str() {
                "us" | "mdy" => s.date.order = DateOrder::Us,
                "intl" | "dmy" => s.date.order = DateOrder::Intl,
                _ => {}
            }
        }
        if let Some(v) = get("date", "fallback").and_then(Fallback::from_key) {
            s.date.fallback = v;
        }
        if let Some(v) = get("output", "folder_template")
            && !v.trim().is_empty()
        {
            s.folder_template = v.to_string();
        }
        if let Some(v) = get("window", "theme") {
            match v.trim().to_ascii_lowercase().as_str() {
                "dark" => s.theme = Theme::Dark,
                "light" => s.theme = Theme::Light,
                _ => {}
            }
        }
        if let Some(v) = get("window", "font_size").and_then(|v| parse_f32(v, 10.0, 28.0)) {
            s.font_size = v;
        }
        if let Some(v) = get("window", "width").and_then(|v| parse_f32(v, 640.0, 10000.0)) {
            s.window_width = v;
        }
        if let Some(v) = get("window", "height").and_then(|v| parse_f32(v, 420.0, 10000.0)) {
            s.window_height = v;
        }
        s
    }

    /// Render the fully commented INI file.
    pub fn to_ini(&self) -> String {
        let order: Vec<&str> = self.date.auto_order.iter().map(|k| k.key()).collect();
        let theme = match self.theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        };
        let amb = match self.date.order {
            DateOrder::Us => "us",
            DateOrder::Intl => "intl",
        };
        // Single-line values only: strip any newline characters.
        let one = |s: &str| s.replace(['\r', '\n'], " ");
        format!(
            r#"; ===========================================================================
; PhotoSorter settings
; ---------------------------------------------------------------------------
; This file lives next to PhotoSorter.exe and is the ONLY place PhotoSorter
; stores anything (no registry, no AppData). Delete it to reset to defaults;
; a fresh, fully commented copy is written on the next start.
;
; Lines starting with ';' are comments. Unknown keys are ignored and invalid
; values silently fall back to their defaults. PhotoSorter rewrites this file
; when it closes, so edit it while the app is not running.
; ===========================================================================

; ---------------------------------------------------------------------------
; [General] - what to scan
; ---------------------------------------------------------------------------
[General]

; Folder that was selected last time (restored on start-up).
; Any absolute path, or empty for none. Default: empty
last_folder = {last_folder}

; Also scan every subfolder of the chosen folder.
; Each folder that contains images gets its own "Month YYYY" subfolders.
; Allowed: true / false. Default: true
include_subfolders = {include_subfolders}

; File extensions treated as images (case-insensitive), separated by commas.
; Default: {default_exts}
extensions = {extensions}

; ---------------------------------------------------------------------------
; [Date] - which date decides the destination folder
; ---------------------------------------------------------------------------
[Date]

; Date source.
;   auto     = try the sources in 'auto_order' below, first one found wins
;   filename = detect the date from the file name (IMG_20240315_..., 2024-03-15, ...)
;   exif     = EXIF "date taken" (JPEG, TIFF, HEIC, PNG, WebP when present)
;   modified = file modified date (local time)
;   created  = file creation date (local time)
; Default: auto
source = {source}

; Priority order used when source = auto. Comma-separated list of
; filename, exif, modified, created (each at most once; missing ones are skipped).
; Default: filename, exif, modified, created
auto_order = {auto_order}

; How to read ambiguous dates such as 03-04-2024 where both numbers are <= 12.
;   us   = month first (March 4, 2024)
;   intl = day first   (3 April 2024)
; Unambiguous dates (e.g. 15-03-2024) and year-first dates are always read correctly.
; Default: us
ambiguous_order = {amb}

; What to do when the chosen source finds no date (e.g. filename without a date).
;   modified = use the file modified date
;   created  = use the file creation date
;   skip     = leave the file where it is
; Default: modified
fallback = {fallback}

; ---------------------------------------------------------------------------
; [Output] - how destination folders are named
; ---------------------------------------------------------------------------
[Output]

; Folder name template. Tokens:
;   {{MMMM}} = full month name (October)   {{MMM}} = short month (Oct)
;   {{MM}}   = 2-digit month (10)          {{M}}   = month (10, 9)
;   {{YYYY}} = 4-digit year (2026)         {{YY}}  = 2-digit year (26)
;   {{DD}}   = 2-digit day (05)
; Characters not allowed in Windows names (\ / : * ? " < > |) become '_'.
; Default: {{MMMM}} {{YYYY}}
folder_template = {template}

; ---------------------------------------------------------------------------
; [Window] - appearance
; ---------------------------------------------------------------------------
[Window]

; Colour theme. Allowed: dark / light. Default: dark
theme = {theme}

; Base font size in points. Allowed: 10 - 28. Default: 15
font_size = {font_size}

; Window size in logical pixels, remembered on exit.
; Allowed: width 640 - 10000, height 420 - 10000. Default: 1200 x 800
width = {w}
height = {h}

; ---------------------------------------------------------------------------
; Reserved for future versions: named presets will be stored here as
; [Preset:Name] sections using the same keys as above.
; ---------------------------------------------------------------------------
"#,
            last_folder = one(&self.last_folder),
            include_subfolders = self.include_subfolders,
            default_exts = DEFAULT_EXTENSIONS,
            extensions = one(&self.extensions),
            source = self.date.mode.key(),
            auto_order = order.join(", "),
            amb = amb,
            fallback = self.date.fallback.key(),
            template = one(&self.folder_template),
            theme = theme,
            font_size = self.font_size.round(),
            w = self.window_width.round(),
            h = self.window_height.round(),
        )
        .replace('\n', "\r\n") // Notepad-friendly line endings
    }
}

/// Directory containing the running executable.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

pub fn ini_path() -> Option<PathBuf> {
    exe_dir().map(|d| d.join(INI_NAME))
}

/// Result of loading the settings.
pub struct Loaded {
    pub settings: Settings,
    /// Where to save, or None if the folder is not writable.
    pub path: Option<PathBuf>,
    /// Notice to show the user, if any.
    pub notice: Option<String>,
}

/// Load `path`, creating a commented default file if missing. Never fails.
pub fn load_or_create(path: Option<PathBuf>) -> Loaded {
    let Some(path) = path else {
        return Loaded {
            settings: Settings::default(),
            path: None,
            notice: Some("Could not locate the program folder; using default settings (not saved).".into()),
        };
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let settings = Settings::from_ini(&text);
            // Check writability without changing content.
            let writable = std::fs::OpenOptions::new().append(true).open(&path).is_ok();
            let notice = (!writable).then(|| format!("{} is read-only; settings changes will not be saved.", INI_NAME));
            Loaded { settings, path: writable.then_some(path), notice }
        }
        Err(_) => {
            let settings = Settings::default();
            match save(&path, &settings) {
                Ok(()) => Loaded { settings, path: Some(path), notice: None },
                Err(_) => Loaded {
                    settings,
                    path: None,
                    notice: Some(
                        "The program folder is not writable; running with default settings (not saved).".into(),
                    ),
                },
            }
        }
    }
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, s.to_ini())?;
    std::fs::rename(&tmp, path).or_else(|_| {
        // rename over an existing file can fail on some setups; fall back
        let r = std::fs::write(path, s.to_ini());
        let _ = std::fs::remove_file(&tmp);
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Settings::default();
        s.last_folder = r"C:\Users\me\Pictures\My Photos".into();
        s.include_subfolders = false;
        s.extensions = "jpg, png".into();
        s.date.mode = DateMode::Only(SourceKind::Created);
        s.date.auto_order = vec![SourceKind::Exif, SourceKind::Filename];
        s.date.order = DateOrder::Intl;
        s.date.fallback = Fallback::Skip;
        s.folder_template = "{YYYY}-{MM} {MMMM}".into();
        s.theme = Theme::Light;
        s.font_size = 17.0;
        s.window_width = 1400.0;
        s.window_height = 900.0;
        let text = s.to_ini();
        assert!(text.contains("; Date source."));
        assert_eq!(Settings::from_ini(&text), s);
    }

    #[test]
    fn defaults_roundtrip_and_every_key_commented() {
        let text = Settings::default().to_ini();
        assert_eq!(Settings::from_ini(&text), Settings::default());
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            if t.is_empty() || t.starts_with(';') {
                continue;
            }
            // previous non-empty line must be a comment (or another key in a pair like width/height)
            let prev = lines[..i].iter().rev().find(|p| !p.trim().is_empty()).unwrap();
            assert!(prev.trim().starts_with(';') || prev.contains("width"), "uncommented: {t}");
        }
    }

    #[test]
    fn lenient_parsing() {
        let text = "\u{feff}[general]\nINCLUDE_SUBFOLDERS = maybe\nbogus=1\n[Date]\nsource = sundial\nauto_order = created, nonsense, created, filename\nfallback=CREATED\n[Window]\nfont_size = 999\nwidth = abc\ntheme = Light\n[Preset:Holiday]\nsource=exif\ngarbage line without equals\n";
        let s = Settings::from_ini(text);
        assert!(s.include_subfolders); // invalid -> default
        assert_eq!(s.date.mode, DateMode::Auto);
        assert_eq!(s.date.auto_order, vec![SourceKind::Created, SourceKind::Filename]);
        assert_eq!(s.date.fallback, Fallback::Created);
        assert_eq!(s.font_size, 15.0);
        assert_eq!(s.window_width, 1200.0);
        assert_eq!(s.theme, Theme::Light);
        // presets are visible to the parser for later use
        assert_eq!(parse_ini(text)["preset:holiday"]["source"], "exif");
        // total garbage
        assert_eq!(Settings::from_ini("\0\0[[[==="), Settings::default());
    }

    #[test]
    fn creates_default_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(INI_NAME);
        let l = load_or_create(Some(p.clone()));
        assert!(l.notice.is_none());
        assert!(p.exists());
        assert_eq!(l.settings, Settings::default());
        let mut s = l.settings.clone();
        s.font_size = 20.0;
        save(&p, &s).unwrap();
        assert_eq!(load_or_create(Some(p)).settings.font_size, 20.0);
    }
}
