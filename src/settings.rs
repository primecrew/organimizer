//! Portable settings in `PhotoSorter.ini` beside the executable.
//!
//! * The file is always (re)generated from a commented template in
//!   [`Settings::to_ini`], so comments survive every save.
//! * Parsing is lenient: unknown sections/keys are ignored, bad values fall
//!   back to defaults, nothing panics.
//! * Nothing is written to the registry or AppData.
//! * The current rename-rule chain lives in `[Rules]` (`rule1 = ...`).
//! * Named presets (whole chain + options) are `[Preset:Name]` sections in
//!   the same file, using the same keys as the main sections.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::date_source::{DateConfig, DateMode, Fallback, SourceKind};
use crate::filename_date::DateOrder;
use crate::rename::RuleSpec;
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
    /// Move files into Month YYYY folders (false = rename in place only).
    pub sort_into_folders: bool,
    /// Ordered rename rules (may be empty).
    pub rules: Vec<RuleSpec>,
    pub presets: Vec<Preset>,
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
            sort_into_folders: true,
            rules: Vec::new(),
            presets: Vec::new(),
            theme: Theme::Dark,
            font_size: 15.0,
            window_width: 1200.0,
            window_height: 800.0,
        }
    }
}

/// What loading a preset changes. Stored as `scope = both|rename|folder` in
/// the `[Preset:Name]` section; a missing or unknown value means `both`
/// (what every preset written before 2.0.2 did).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetScope {
    /// Rule chain + all options (include_subfolders, extensions, date, folder).
    #[default]
    Both,
    /// Only the rename rule chain.
    RenameOnly,
    /// Only the output/folder stage: Month YYYY on/off, template, date
    /// source, ambiguous order, fallback, auto order.
    FolderOnly,
}

impl PresetScope {
    pub const ALL: [PresetScope; 3] = [PresetScope::Both, PresetScope::RenameOnly, PresetScope::FolderOnly];

    pub fn key(self) -> &'static str {
        match self {
            PresetScope::Both => "both",
            PresetScope::RenameOnly => "rename",
            PresetScope::FolderOnly => "folder",
        }
    }

    pub fn from_key(s: &str) -> Option<PresetScope> {
        match s.trim().to_ascii_lowercase().as_str() {
            "both" | "all" => Some(PresetScope::Both),
            "rename" | "rename_only" | "rules" => Some(PresetScope::RenameOnly),
            "folder" | "folder_only" | "output" => Some(PresetScope::FolderOnly),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PresetScope::Both => "Rules + folder options",
            PresetScope::RenameOnly => "Rename rules only",
            PresetScope::FolderOnly => "Folder / date options only",
        }
    }
}

/// Characters a preset name may not contain (they would break the
/// `[Preset:Name]` section header or start an INI comment).
pub const PRESET_NAME_FORBIDDEN: &[char] = &['[', ']', ';', '#'];
pub const PRESET_NAME_MAX: usize = 64;

/// A named, saved rule chain plus the options that shape a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    /// What loading this preset changes.
    pub scope: PresetScope,
    pub include_subfolders: bool,
    pub extensions: String,
    pub date: DateConfig,
    pub folder_template: String,
    pub sort_into_folders: bool,
    pub rules: Vec<RuleSpec>,
}

/// Make a preset name safe for an INI section header.
pub fn clean_preset_name(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '[' | ']' | '\r' | '\n' | ';' | '#')).collect::<String>().trim().to_string()
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

/// Like [`parse_ini`] but keeps section order and the original section
/// name (needed for preset names). Keys are lowercase.
pub fn parse_ini_sections(text: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut out: Vec<(String, BTreeMap<String, String>)> = vec![(String::new(), BTreeMap::new())];
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                out.push((line[1..end].trim().to_string(), BTreeMap::new()));
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
                out.last_mut().unwrap().1.insert(key, val.to_string());
            }
        }
    }
    out
}

/// `rule1`, `rule2`, ... (sorted by number; gaps allowed; bad ones skipped).
fn rules_from(map: Option<&BTreeMap<String, String>>) -> Vec<RuleSpec> {
    let Some(map) = map else { return Vec::new() };
    let mut v: Vec<(u32, RuleSpec)> = map
        .iter()
        .filter_map(|(k, val)| {
            let n = k.strip_prefix("rule")?.trim().parse::<u32>().ok()?;
            Some((n, RuleSpec::from_ini_value(val)?))
        })
        .collect();
    v.sort_by_key(|(n, _)| *n);
    v.into_iter().map(|(_, r)| r).collect()
}

/// Read the run options from `get(key)` into `o` (lenient).
fn read_options(get: &dyn Fn(&str) -> Option<String>, o: &mut Preset) {
    if let Some(v) = get("include_subfolders").as_deref().and_then(parse_bool) {
        o.include_subfolders = v;
    }
    if let Some(v) = get("extensions")
        && !v.trim().is_empty()
        && crate::scan::rejected_extensions(&v).len()
            < v.split([',', ';', ' ']).filter(|x| !x.trim().is_empty()).count()
    {
        o.extensions = v;
    }
    if let Some(v) = get("source").as_deref().and_then(DateMode::from_key) {
        o.date.mode = v;
    }
    if let Some(v) = get("auto_order") {
        let mut order: Vec<SourceKind> = Vec::new();
        for k in v.split([',', '>', ';']).filter_map(SourceKind::from_key) {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        if !order.is_empty() {
            o.date.auto_order = order;
        }
    }
    if let Some(v) = get("ambiguous_order") {
        match v.trim().to_ascii_lowercase().as_str() {
            "us" | "mdy" => o.date.order = DateOrder::Us,
            "intl" | "dmy" => o.date.order = DateOrder::Intl,
            _ => {}
        }
    }
    if let Some(v) = get("fallback").as_deref().and_then(Fallback::from_key) {
        o.date.fallback = v;
    }
    if let Some(v) = get("folder_template")
        && !v.trim().is_empty()
    {
        o.folder_template = v;
    }
    if let Some(v) = get("sort_into_month_folders").as_deref().and_then(parse_bool) {
        o.sort_into_folders = v;
    }
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
        let main_get = |k: &str| -> Option<String> {
            let sec = match k {
                "include_subfolders" | "extensions" => "general",
                "folder_template" | "sort_into_month_folders" => "output",
                _ => "date",
            };
            get(sec, k).map(str::to_string)
        };
        let mut o = s.to_preset("");
        read_options(&main_get, &mut o);
        o.rules = rules_from(d.get("rules"));
        s.apply_preset(&o);

        for (sec, map) in parse_ini_sections(text) {
            let Some(name) =
                sec.get(..7).filter(|p| p.eq_ignore_ascii_case("preset:")).map(|_| clean_preset_name(&sec[7..]))
            else {
                continue;
            };
            if name.is_empty() || s.presets.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                continue;
            }
            let mut p = Settings::default().to_preset(&name);
            read_options(&|k: &str| map.get(k).cloned(), &mut p);
            p.scope = map.get("scope").and_then(|v| PresetScope::from_key(v)).unwrap_or_default();
            p.rules = rules_from(Some(&map));
            s.presets.push(p);
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

    /// Snapshot of the current chain + options as a preset.
    pub fn to_preset(&self, name: &str) -> Preset {
        Preset {
            name: clean_preset_name(name),
            scope: PresetScope::Both,
            include_subfolders: self.include_subfolders,
            extensions: self.extensions.clone(),
            date: self.date.clone(),
            folder_template: self.folder_template.clone(),
            sort_into_folders: self.sort_into_folders,
            rules: self.rules.clone(),
        }
    }

    /// Load a preset (folder, theme etc. are kept). What is loaded depends
    /// on the preset's [`PresetScope`]; `Both` loads the chain + all options.
    pub fn apply_preset(&mut self, p: &Preset) {
        let rules = matches!(p.scope, PresetScope::Both | PresetScope::RenameOnly);
        let folder = matches!(p.scope, PresetScope::Both | PresetScope::FolderOnly);
        if p.scope == PresetScope::Both {
            self.include_subfolders = p.include_subfolders;
            self.extensions = p.extensions.clone();
        }
        if folder {
            self.date = p.date.clone();
            self.folder_template = p.folder_template.clone();
            self.sort_into_folders = p.sort_into_folders;
        }
        if rules {
            self.rules = p.rules.clone();
        }
    }

    /// Save the current chain + options under `name` (replacing a preset
    /// with the same name, case-insensitive, and keeping its scope). Returns
    /// false for an empty name.
    pub fn save_preset(&mut self, name: &str) -> bool {
        let mut p = self.to_preset(name);
        if p.name.is_empty() {
            return false;
        }
        match self.presets.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&p.name)) {
            Some(x) => {
                p.scope = x.scope;
                p.name = x.name.clone();
                *x = p
            }
            None => self.presets.push(p),
        }
        true
    }

    pub fn find_preset(&self, name: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Check a new preset name. `except` is the preset being renamed (its
    /// own name, in any case, is allowed). Returns the trimmed name.
    pub fn validate_preset_name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let n = name.trim();
        if n.is_empty() {
            return Err("Enter a name.".into());
        }
        if n.chars().any(|c| PRESET_NAME_FORBIDDEN.contains(&c) || c.is_control()) {
            return Err("Names cannot contain [ ] ; # or line breaks.".into());
        }
        if n.chars().count() > PRESET_NAME_MAX {
            return Err(format!("Use at most {PRESET_NAME_MAX} characters."));
        }
        let taken = self
            .presets
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(n) && !except.is_some_and(|e| p.name.eq_ignore_ascii_case(e)));
        if taken {
            return Err(format!("A preset named “{n}” already exists."));
        }
        Ok(n.to_string())
    }

    /// Rename a preset after validating the new name.
    pub fn rename_preset(&mut self, old: &str, new: &str) -> Result<String, String> {
        let n = self.validate_preset_name(new, Some(old))?;
        let p = self.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(old)).ok_or("No such preset.")?;
        p.name = n.clone();
        Ok(n)
    }

    /// Copy a preset as "Name copy" (or "Name copy 2", ...), inserted right
    /// after the original. Returns the new name.
    pub fn duplicate_preset(&mut self, name: &str) -> Option<String> {
        let i = self.presets.iter().position(|p| p.name.eq_ignore_ascii_case(name))?;
        let base: String = self.presets[i].name.chars().take(PRESET_NAME_MAX - 8).collect();
        let free = |n: &str| !self.presets.iter().any(|p| p.name.eq_ignore_ascii_case(n));
        let mut new = format!("{base} copy");
        let mut k = 2;
        while !free(&new) {
            new = format!("{base} copy {k}");
            k += 1;
        }
        let mut copy = self.presets[i].clone();
        copy.name = new.clone();
        self.presets.insert(i + 1, copy);
        Some(new)
    }

    /// "Save changes": overwrite an existing preset with the current working
    /// state, keeping its name, position and scope.
    pub fn update_preset(&mut self, name: &str) -> bool {
        let snap = self.to_preset(name);
        match self.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => {
                let (n, scope) = (p.name.clone(), p.scope);
                *p = snap;
                p.name = n;
                p.scope = scope;
                true
            }
            None => false,
        }
    }

    pub fn set_preset_scope(&mut self, name: &str, scope: PresetScope) -> bool {
        match self.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => {
                p.scope = scope;
                true
            }
            None => false,
        }
    }

    pub fn delete_preset(&mut self, name: &str) -> bool {
        let n = self.presets.len();
        self.presets.retain(|p| !p.name.eq_ignore_ascii_case(name));
        n != self.presets.len()
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

; File extensions to process (case-insensitive), separated by commas.
; Only image, camera RAW and video types are ever accepted; anything else
; (txt, pdf, xmp, json, ...) is ignored and never touched.
; Default: {default_exts}
extensions = {extensions}

; ---------------------------------------------------------------------------
; [Date] - which date decides the destination folder
; ---------------------------------------------------------------------------
[Date]

; Date source.
;   auto     = try the sources in 'auto_order' below, first one found wins
;   filename = detect the date from the file name (IMG_20240315_..., 2024-03-15, ...)
;   exif     = EXIF "date taken" (JPEG, TIFF, HEIC, PNG, WebP when present),
;              or the creation time stored in MP4 / MOV / M4V / 3GP videos
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

; Move files into the Month YYYY folders above. Set to false to only
; rename files in place with the rules below (no folders are created).
; Allowed: true / false. Default: true
sort_into_month_folders = {sort}

; ---------------------------------------------------------------------------
; [Rules] - rename rules, applied in order to the name WITHOUT extension,
; before the Month YYYY folder is chosen. No rules = names are kept.
; One line per rule:  ruleN = type | on/off | key=value | key=value ...
; (values use %XX for | % " ; # [ ] and leading/trailing spaces). Rule names
; follow den4b ReNamer. Booleans are 0/1. Missing keys use the defaults.
;   insert    text= pos=                       (alias: add)
;   delete    from=N | fromdelim=TEXT ; count=N | untildelim=TEXT | until=end
;             rtl= keepdelim=
;   remove    mode=text text= occ=all/first/last case= |
;             mode=chars chars= case= | mode=first/last n= |
;             mode=range from= to= (1-based) | mode=whitespace | mode=digits
;   replace   find= with= regex= case= occ=all/first/last
;   regex     same keys as replace (regex=1); with= may use $1, $2 ...
;   rearrange split=DELIMS (several separated by %7C) bypos= pattern=$1 $-1 $0
;   extension set=jpg (image/video extensions only) | mode=detect (by content)
;   strip     english= digits= symbols= brackets= user=CHARS except= case=
;   case      mode=lower/upper/title/sentence/invert
;   serialize start= step= pad= pos= sep= perfolder= repeat= reset=
;             system=decimal/abc/ABC/roman              (alias: number)
;   randomize len= digits= lower= upper= user= unique= pos= seed=
;   padding   zeropad=N unpad= len=N chars= left=
;   clean     trim= spaces= us2sp= sp2us= dot2sp= illegal= repeats=
;             brackets= normspaces= emoji= marks= capspace=
;   translit  (no options: e->e, ss, Zh ... to plain Latin)
;   reformatdate find=%d.%m.%Y%7C%Y%m%d to=%Y-%m-%d whole=
;   date      source=plan/filename/exif/modified/created fmt=%Y-%m-%d pos= sep=
;   parent    pos= sep=
;   extcase   mode=lower/upper
; pos = prefix / suffix / at:N (after N chars) / end:N (N chars before the
; end) / replace (whole name) / after:TEXT / before:TEXT.
; The extension only changes with the extension / extcase rules, and only
; ever to another image or video extension. Pascal Script is not supported.
; ---------------------------------------------------------------------------
[Rules]
{rules}
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
; Presets: [Preset:Name] sections, saved from the app. Each holds a whole
; rule chain (ruleN keys as in [Rules]) plus the options include_subfolders,
; extensions, source, auto_order, ambiguous_order, fallback, folder_template
; and sort_into_month_folders (same meaning as above).
; scope = what loading the preset changes. Allowed: both (rules + all
; options) / rename (rule chain only) / folder (Month YYYY on/off, template,
; date source, auto_order, ambiguous_order, fallback). Default: both
; ---------------------------------------------------------------------------
{presets}"#,
            last_folder = one(&self.last_folder),
            include_subfolders = self.include_subfolders,
            default_exts = DEFAULT_EXTENSIONS,
            extensions = one(&self.extensions),
            source = self.date.mode.key(),
            auto_order = order.join(", "),
            amb = amb,
            fallback = self.date.fallback.key(),
            template = one(&self.folder_template),
            sort = self.sort_into_folders,
            rules = rules_ini(&self.rules),
            presets = self.presets.iter().map(preset_ini).collect::<String>(),
            theme = theme,
            font_size = self.font_size.round(),
            w = self.window_width.round(),
            h = self.window_height.round(),
        )
        .replace('\n', "\r\n") // Notepad-friendly line endings
    }
}

fn rules_ini(rules: &[RuleSpec]) -> String {
    let mut out = String::new();
    for (i, r) in rules.iter().enumerate() {
        let state = if r.enabled { "" } else { " (disabled)" };
        let summary = r.kind.summary().replace(['\r', '\n'], " ");
        out.push_str(&format!("; {}. {summary}{state}\nrule{} = {}\n", i + 1, i + 1, r.to_ini_value()));
    }
    out
}

fn preset_ini(p: &Preset) -> String {
    let one = |s: &str| s.replace(['\r', '\n'], " ");
    let order: Vec<&str> = p.date.auto_order.iter().map(|k| k.key()).collect();
    let amb = match p.date.order {
        DateOrder::Us => "us",
        DateOrder::Intl => "intl",
    };
    format!(
        "\n[Preset:{name}]\n; what loading this preset changes: both / rename / folder\nscope = {scope}\n; options\ninclude_subfolders = {inc}\nextensions = {ext}\nsource = {src}\nauto_order = {ord}\nambiguous_order = {amb}\nfallback = {fb}\nfolder_template = {tpl}\nsort_into_month_folders = {sort}\n; rules ({n})\n{rules}",
        name = p.name,
        scope = p.scope.key(),
        inc = p.include_subfolders,
        ext = one(&p.extensions),
        src = p.date.mode.key(),
        ord = order.join(", "),
        fb = p.date.fallback.key(),
        tpl = one(&p.folder_template),
        sort = p.sort_into_folders,
        n = p.rules.len(),
        rules = rules_ini(&p.rules),
    )
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

/// Write the INI safely: the new text goes to `PhotoSorter.ini.tmp` first,
/// the previous file is kept as `PhotoSorter.ini.bak`, then the temp file
/// replaces the INI (an atomic rename on the same volume).
pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, s.to_ini())?;
    if path.is_file() {
        let _ = std::fs::copy(path, path.with_extension("ini.bak"));
    }
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

    // ---------------------------------------------------------------- v2 --

    use crate::rename::*;

    fn sample_rules() -> Vec<RuleSpec> {
        let mut v = vec![
            RuleSpec::new(RuleKind::Replace(ReplaceRule {
                find: r"^IMG_(\d+)".into(),
                with: "Rome $1 | 100%".into(),
                regex: true,
                case_sensitive: true,
                occurrence: Occurrence::First,
            })),
            RuleSpec::new(RuleKind::new_default("number").unwrap()),
            RuleSpec::new(RuleKind::new_default("date").unwrap()),
            RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Lower)),
        ];
        v[1].enabled = false;
        v
    }

    #[test]
    fn rules_and_presets_roundtrip() {
        let mut s = Settings::default();
        s.last_folder = r"D:\Photos".into();
        s.rules = sample_rules();
        s.sort_into_folders = false;
        assert!(s.save_preset("Holiday [Rome]; v2"));
        // a second preset with different options
        s.rules = vec![RuleSpec::new(RuleKind::Case(CaseMode::Title))];
        s.sort_into_folders = true;
        s.extensions = "jpg, mp4".into();
        s.date.mode = DateMode::Only(SourceKind::Exif);
        s.folder_template = "{YYYY}-{MM}".into();
        assert!(s.save_preset("Title only"));
        assert!(!s.save_preset("  [] "));
        assert_eq!(s.presets.len(), 2);
        assert_eq!(s.presets[0].name, "Holiday Rome v2");

        let text = s.to_ini();
        assert!(text.contains("[Preset:Holiday Rome v2]"), "{text}");
        assert!(text.contains("[Rules]"));
        assert!(text.contains("; 1. Change case to Title Case"));
        let back = Settings::from_ini(&text);
        assert_eq!(back, s);

        // loading a preset brings back the whole chain + options
        let mut t = back.clone();
        let p = t.presets[0].clone();
        t.apply_preset(&p);
        assert_eq!(t.rules, sample_rules());
        assert!(!t.sort_into_folders);
        assert_eq!(t.extensions, Settings::default().extensions);
        assert_eq!(t.last_folder, r"D:\Photos"); // not part of a preset

        // overwrite by name (case-insensitive) and delete
        assert!(t.save_preset("title ONLY"));
        assert_eq!(t.presets.len(), 2);
        assert!(t.delete_preset("TITLE only"));
        assert!(!t.delete_preset("nope"));
        assert_eq!(t.presets.len(), 1);
    }

    #[test]
    fn v1_ini_loads_unchanged_with_no_rules() {
        // what v0.1.0 wrote: no [Rules], no sort_into_month_folders
        let v1 = "[General]\r\nlast_folder = C:\\Pics\r\ninclude_subfolders = true\r\nextensions = png, jpg, jpeg, gif, bmp, webp, tif, tiff, heic\r\n[Date]\r\nsource = auto\r\n[Output]\r\nfolder_template = {MMMM} {YYYY}\r\n";
        let s = Settings::from_ini(v1);
        assert!(s.rules.is_empty());
        assert!(s.sort_into_folders);
        assert!(s.presets.is_empty());
        assert_eq!(s.extensions, crate::scan::V1_DEFAULT_EXTENSIONS);
    }

    #[test]
    fn lenient_rules_and_presets() {
        let text = "[Rules]\nrule10 = case | on | mode=upper\nrule2 = bogus | on\nrule1 = add | off | text=%20x%7C | pos=end:2\nrulex = case\n[preset:]\nsource=exif\n[PRESET:Mine]\nrule1 = extcase | on | mode=upper\nsort_into_month_folders = no\nextensions = txt, pdf\n[Preset:mine]\nrule1 = case\n";
        let s = Settings::from_ini(text);
        assert_eq!(s.rules.len(), 2);
        assert_eq!(
            s.rules[0],
            RuleSpec {
                enabled: false,
                kind: RuleKind::AddText(AddTextRule { text: " x|".into(), position: Position::FromEnd(2) })
            }
        );
        assert_eq!(s.rules[1].kind, RuleKind::Case(CaseMode::Upper));
        assert_eq!(s.presets.len(), 1); // empty name skipped, duplicate name skipped
        let p = &s.presets[0];
        assert_eq!(p.name, "Mine");
        assert!(!p.sort_into_folders);
        assert_eq!(p.extensions, DEFAULT_EXTENSIONS); // non-media list rejected
        assert_eq!(p.rules, vec![RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Upper))]);
    }

    #[test]
    fn preset_sections_are_commented() {
        let mut s = Settings::default();
        s.rules = sample_rules();
        s.save_preset("P");
        let text = s.to_ini();
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            if !t.starts_with("rule") {
                continue;
            }
            assert!(lines[i - 1].trim().starts_with(';'), "uncommented rule line: {t}");
        }
    }

    #[test]
    fn options_extension_edit_persists_and_applies_to_scan() {
        // What Tools > Options > Save does: change the list, write the INI,
        // and on the next start the edited list is what the scan uses.
        let d = tempfile::tempdir().unwrap();
        let ini = d.path().join(INI_NAME);
        let first = load_or_create(Some(ini.clone()));
        assert_eq!(first.settings.extensions, DEFAULT_EXTENSIONS);
        let mut s = first.settings;
        s.extensions = "jpg, mp4, txt".into(); // txt typed by the user: ignored
        save(&ini, &s).unwrap();
        let text = std::fs::read_to_string(&ini).unwrap();
        assert!(text.contains("extensions = jpg, mp4, txt"), "{text}");

        let again = load_or_create(Some(ini.clone())).settings;
        assert_eq!(again.extensions, "jpg, mp4, txt");
        let exts = crate::scan::parse_extensions(&again.extensions);
        assert_eq!(exts, vec!["jpg", "mp4"]);
        let pics = d.path().join("pics");
        std::fs::create_dir(&pics).unwrap();
        for f in ["a.jpg", "b.png", "c.mp4", "d.txt"] {
            std::fs::write(pics.join(f), b"x").unwrap();
        }
        let r = crate::scan::scan(&pics, false, &exts, &std::sync::atomic::AtomicBool::new(false), &mut |_, _| {});
        let mut names: Vec<_> = r.files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, vec!["a.jpg", "c.mp4"]);

        // "Reset to defaults" = the default list, which round-trips unchanged
        s.extensions = DEFAULT_EXTENSIONS.to_string();
        save(&ini, &s).unwrap();
        assert_eq!(load_or_create(Some(ini)).settings.extensions, DEFAULT_EXTENSIONS);
    }

    #[test]
    fn preset_duplicate_rename_delete_and_uniqueness() {
        let mut s = Settings::default();
        s.rules = sample_rules();
        assert!(s.save_preset("Trip"));
        assert_eq!(s.duplicate_preset("trip").as_deref(), Some("Trip copy"));
        assert_eq!(s.duplicate_preset("Trip").as_deref(), Some("Trip copy 2"));
        assert_eq!(s.duplicate_preset("Trip copy").as_deref(), Some("Trip copy copy"));
        assert_eq!(s.duplicate_preset("missing"), None);
        let names: Vec<&str> = s.presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Trip", "Trip copy 2", "Trip copy", "Trip copy copy"]);
        assert_eq!(s.presets[1].rules, s.presets[0].rules);

        // rename validation
        assert!(s.rename_preset("Trip copy", "  ").is_err());
        for bad in ["a]b", "[x", "a;b", "a#b", "two\nlines", "tab\there"] {
            assert!(s.rename_preset("Trip copy", bad).is_err(), "{bad:?}");
        }
        assert!(s.rename_preset("Trip copy", &"x".repeat(PRESET_NAME_MAX + 1)).is_err());
        assert!(s.rename_preset("Trip copy", "TRIP").is_err()); // taken (case-insensitive)
        assert_eq!(s.rename_preset("Trip copy", "TRIP COPY").as_deref(), Ok("TRIP COPY")); // own name, new case
        assert_eq!(s.rename_preset("trip copy", "  Rome 2024 ").as_deref(), Ok("Rome 2024"));
        assert!(s.rename_preset("nope", "x").is_err());
        assert!(s.validate_preset_name("Rome 2024", None).is_err());
        assert!(s.validate_preset_name("Rome 2024", Some("rome 2024")).is_ok());

        // update keeps name, position and scope
        s.set_preset_scope("Rome 2024", PresetScope::RenameOnly);
        s.rules.clear();
        s.sort_into_folders = false;
        assert!(s.update_preset("rome 2024"));
        let p = s.find_preset("Rome 2024").unwrap();
        assert_eq!(
            (p.name.as_str(), p.scope, p.rules.len(), p.sort_into_folders),
            ("Rome 2024", PresetScope::RenameOnly, 0, false)
        );
        assert_eq!(s.presets[2].name, "Rome 2024");
        // overwrite via "Save as" keeps the existing scope too
        assert!(s.save_preset("ROME 2024"));
        assert_eq!(s.find_preset("Rome 2024").unwrap().scope, PresetScope::RenameOnly);
        assert_eq!(s.presets.len(), 4);

        assert!(s.delete_preset("rome 2024"));
        assert!(!s.delete_preset("rome 2024"));
        assert_eq!(s.presets.len(), 3);
    }

    #[test]
    fn preset_scope_roundtrip_apply_and_old_inis() {
        let mut s = Settings::default();
        s.rules = sample_rules();
        s.sort_into_folders = false;
        s.folder_template = "{YYYY}".into();
        s.save_preset("Both");
        s.save_preset("R");
        s.save_preset("F");
        s.set_preset_scope("R", PresetScope::RenameOnly);
        s.set_preset_scope("F", PresetScope::FolderOnly);
        let text = s.to_ini();
        assert!(text.contains("scope = rename") && text.contains("scope = folder"), "{text}");
        let back = Settings::from_ini(&text);
        assert_eq!(back, s);

        // rename-only loads rules only, folder-only loads folder options only
        let mut t = Settings::default();
        t.apply_preset(back.find_preset("R").unwrap());
        assert_eq!(t.rules, sample_rules());
        assert!(t.sort_into_folders);
        assert_eq!(t.folder_template, Settings::default().folder_template);
        let mut t = Settings::default();
        t.apply_preset(back.find_preset("F").unwrap());
        assert!(t.rules.is_empty());
        assert!(!t.sort_into_folders);
        assert_eq!(t.folder_template, "{YYYY}");

        // a 2.0 / 2.0.1 preset (no scope key) or an unknown value = both
        let old = "[Preset:Old]\nsort_into_month_folders = no\nrule1 = case | on | mode=upper\n[Preset:Odd]\nscope = sideways\n";
        let o = Settings::from_ini(old);
        assert_eq!(o.presets[0].scope, PresetScope::Both);
        assert_eq!(o.presets[1].scope, PresetScope::Both);
        let mut t = Settings::default();
        t.apply_preset(&o.presets[0]);
        assert!(!t.sort_into_folders);
        assert_eq!(t.rules.len(), 1);
    }

    #[test]
    fn save_is_atomic_and_keeps_a_backup() {
        let d = tempfile::tempdir().unwrap();
        let ini = d.path().join(INI_NAME);
        let mut s = Settings::default();
        save(&ini, &s).unwrap();
        assert!(!d.path().join("PhotoSorter.ini.bak").exists()); // nothing to back up yet
        let first = std::fs::read_to_string(&ini).unwrap();
        s.save_preset("New one");
        save(&ini, &s).unwrap();
        assert_eq!(std::fs::read_to_string(d.path().join("PhotoSorter.ini.bak")).unwrap(), first);
        assert!(!d.path().join("PhotoSorter.ini.tmp").exists());
        let now = std::fs::read_to_string(&ini).unwrap();
        assert!(now.starts_with("; ====") && now.contains("[Preset:New one]"));
        assert_eq!(Settings::from_ini(&now), s);
    }
}
