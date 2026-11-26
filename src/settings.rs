//! Settings in `Organimizer.ini`: beside the executable (portable, the
//! default) or, for a copy installed by setup, in
//! `%APPDATA%\PrimeCrew Software\Organimizer` (see [`choose_ini_path`]).
//! On the first start of 3.0 an existing PhotoSorter 2.x `PhotoSorter.ini`
//! is imported (copied, never changed or deleted); see [`legacy_ini_paths`].
//!
//! * The file is always (re)generated from a commented template in
//!   [`Settings::to_ini`], so comments survive every save.
//! * Parsing is lenient: unknown sections/keys are ignored, bad values fall
//!   back to defaults, nothing panics.
//! * Nothing is written to the registry; AppData only in installed mode.
//! * The current rename-rule chain lives in `[Rules]` (`rule1 = ...`).
//! * Named presets (whole chain + options) are `[Preset:Name]` sections in
//!   the same file, using the same keys as the main sections.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::blacklist::Blacklist;
use crate::date_source::{DateConfig, DateMode, Fallback, SourceKind};
use crate::favorites::Favorites;
use crate::filename_date::DateOrder;
use crate::filetypes::TypeGroup;
use crate::rename::RuleSpec;
use crate::scan::DEFAULT_EXTENSIONS;
use crate::template::DEFAULT_TEMPLATE;
use crate::window_state::{WindowLayout, format_widths, parse_num, parse_widths};

pub const INI_NAME: &str = "Organimizer.ini";
/// Written beside the exe by setup: settings go to %APPDATA% instead.
pub const INSTALLED_MARKER: &str = "Organimizer.installed";
/// Optional file beside the exe that forces portable mode.
pub const PORTABLE_MARKER: &str = "Organimizer.portable";
/// Settings folder below %APPDATA% in installed mode.
pub const APPDATA_SUBDIR: [&str; 2] = ["PrimeCrew Software", "Organimizer"];
/// PhotoSorter 2.x settings, imported once (never modified or deleted).
pub const LEGACY_INI_NAME: &str = "PhotoSorter.ini";
pub const LEGACY_APPDATA_SUBDIR: [&str; 2] = ["PrimeCrew Software", "PhotoSorter"];
/// 3.0.0/3.0.1 display name. Settings always lived in [`APPDATA_SUBDIR`]; if a
/// folder with this name exists it is imported once and never deleted.
pub const PREV_DISPLAY_APPDATA_SUBDIR: [&str; 2] = ["PrimeCrew Software", "PrimeCrew Organimizer"];
pub const DEFAULT_TMDB_LANGUAGE: &str = "en-US";

/// Plex tab settings (`[Plex]`, 3.0).
#[derive(Debug, Clone, PartialEq)]
pub struct PlexSettings {
    /// Folder to analyze.
    pub source: String,
    pub include_subfolders: bool,
    /// true = organize inside the source folder; false = move into the
    /// Movies / TV library roots below.
    pub in_place: bool,
    pub movies_root: String,
    pub tv_root: String,
    /// TMDB lookups (off by default; needs the user's own key).
    pub tmdb_enabled: bool,
    /// The user's TMDB key or read access token. Stored only in this file.
    pub tmdb_key: String,
    pub tmdb_language: String,
    /// Add "{tmdb-ID}" to the folder name of titles confirmed on TMDB.
    pub tmdb_id_tag: bool,
    /// 3.0.6: true = "Rename only" (rename files where they are, nothing
    /// moves); false = "Full sort" (build the Plex folders).
    pub rename_only: bool,
    /// 3.0.6: last folders used on the Plex tab, most recent first (max 4).
    pub recent: Vec<String>,
}

impl Default for PlexSettings {
    fn default() -> Self {
        PlexSettings {
            source: String::new(),
            include_subfolders: true,
            in_place: true,
            movies_root: String::new(),
            tv_root: String::new(),
            tmdb_enabled: false,
            tmdb_key: String::new(),
            tmdb_language: DEFAULT_TMDB_LANGUAGE.into(),
            tmdb_id_tag: true,
            rename_only: true,
            recent: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    /// Follow the Windows light/dark app setting (since 2.0.3).
    System,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::System];

    pub fn key(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
        }
    }

    pub fn from_key(s: &str) -> Option<Theme> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "system" | "auto" => Some(Theme::System),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::System => "Follow system",
        }
    }
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
    /// Window position, maximized state, panel height, column widths and
    /// open/closed sections (`[Window]`, since 2.0.3).
    pub layout: WindowLayout,
    /// Which files the Organize tab picks up (3.0; default = 2.x behavior).
    pub file_types: TypeGroup,
    /// Extensions for the "Custom list" type group.
    pub custom_extensions: String,
    /// Folders that cannot be chosen or scanned.
    pub blacklist: Blacklist,
    /// false = the blacklist is the built-in default list (recomputed from
    /// the Windows folders on every start); true = the user's edited list.
    pub blacklist_custom: bool,
    pub favorites: Favorites,
    pub plex: PlexSettings,
    /// Last top-level tab: "organize" or "plex".
    pub mode: String,
    /// 3.0.6: last folders used on the Organize tab, most recent first
    /// (max 4). The source box itself starts blank on every launch.
    pub recent_folders: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_folder: String::new(),
            // Off since 2.0.4 (was on): only the chosen folder is scanned
            // unless the user ticks "Include subfolders".
            include_subfolders: false,
            extensions: DEFAULT_EXTENSIONS.to_string(),
            date: DateConfig::default(),
            folder_template: DEFAULT_TEMPLATE.to_string(),
            sort_into_folders: true,
            rules: Vec::new(),
            presets: Vec::new(),
            theme: Theme::Dark,
            font_size: 15.0,
            window_width: crate::window_state::DEFAULT_SIZE[0],
            window_height: crate::window_state::DEFAULT_SIZE[1],
            layout: WindowLayout::default(),
            file_types: TypeGroup::Media,
            custom_extensions: String::new(),
            blacklist: Blacklist::default(),
            blacklist_custom: false,
            favorites: Favorites::default(),
            plex: PlexSettings::default(),
            mode: "organize".into(),
            recent_folders: Vec::new(),
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

        if let Some(t) = get("window", "theme").and_then(Theme::from_key) {
            s.theme = t;
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
        // [Window] layout (2.0.3); missing / bad values keep the defaults
        let l = &mut s.layout;
        let x = get("window", "x").and_then(|v| parse_num(v, -100000.0, 100000.0));
        let y = get("window", "y").and_then(|v| parse_num(v, -100000.0, 100000.0));
        if x.is_some() && y.is_some() {
            (l.x, l.y) = (x, y);
        }
        let flag = |key: &str, default: bool| get("window", key).and_then(parse_bool).unwrap_or(default);
        l.maximized = flag("maximized", l.maximized);
        l.setup_height = get("window", "setup_panel_height").and_then(|v| parse_num(v, 40.0, 5000.0));
        l.files_columns = get("window", "files_columns").map(parse_widths).unwrap_or_default();
        l.folders_columns = get("window", "folders_columns").map(parse_widths).unwrap_or_default();
        l.source_open = flag("open_source", l.source_open);
        l.rules_open = flag("open_rules", l.rules_open);
        l.output_open = flag("open_output", l.output_open);
        l.output_more_open = flag("open_output_more", l.output_more_open);
        l.folder_advanced = flag("folder_advanced", l.folder_advanced);

        // 3.0 sections (missing in 2.x files: defaults)
        if let Some(g) = get("general", "file_types").and_then(TypeGroup::from_key) {
            s.file_types = g;
        }
        if let Some(v) = get("general", "custom_extensions") {
            s.custom_extensions = v.to_string();
        }
        if get("blacklist", "custom").and_then(parse_bool) == Some(true) {
            let mut entries: Vec<(u32, String)> = d
                .get("blacklist")
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| k.strip_prefix("block").and_then(|n| n.parse().ok()).map(|n| (n, v.clone())))
                .filter(|(_, v)| Blacklist::validate_entry(v).is_ok())
                .collect();
            entries.sort_by_key(|(n, _)| *n);
            s.blacklist = Blacklist { entries: entries.into_iter().map(|(_, v)| v).collect() };
            s.blacklist_custom = true;
        }
        s.favorites = Favorites::from_ini_map(d.get("favorites"));
        let p = &mut s.plex;
        let pget = |k: &str| get("plex", k);
        if let Some(v) = pget("source") {
            p.source = v.to_string();
        }
        if let Some(v) = pget("include_subfolders").and_then(parse_bool) {
            p.include_subfolders = v;
        }
        if let Some(v) = pget("destination") {
            p.in_place = !v.trim().eq_ignore_ascii_case("roots");
        }
        if let Some(v) = pget("movies_root") {
            p.movies_root = v.to_string();
        }
        if let Some(v) = pget("tv_root") {
            p.tv_root = v.to_string();
        }
        if let Some(v) = pget("tmdb_enabled").and_then(parse_bool) {
            p.tmdb_enabled = v;
        }
        if let Some(v) = pget("tmdb_key") {
            p.tmdb_key = v.trim().to_string();
        }
        if let Some(v) = pget("tmdb_id_folder_tag").and_then(parse_bool) {
            p.tmdb_id_tag = v;
        }
        if let Some(v) = pget("tmdb_language").filter(|v| !v.trim().is_empty() && v.len() <= 10) {
            p.tmdb_language = v.trim().to_string();
        }
        if let Some(v) = pget("mode") {
            p.rename_only = matches!(v.trim().to_ascii_lowercase().as_str(), "rename_only" | "rename" | "renameonly");
        }
        p.recent = recent_from(d.get("plex"));
        s.recent_folders = recent_from(d.get("general"));
        if let Some(v) = get("window", "mode").filter(|v| matches!(*v, "organize" | "plex")) {
            s.mode = v.to_string();
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
        let theme = self.theme.key();
        let l = &self.layout;
        let opt = |v: Option<f32>| v.map(|v| format!("{}", v.round())).unwrap_or_default();
        let amb = match self.date.order {
            DateOrder::Us => "us",
            DateOrder::Intl => "intl",
        };
        // Single-line values only: strip any newline characters.
        let one = |s: &str| s.replace(['\r', '\n'], " ");
        format!(
            r#"; ===========================================================================
; Organimizer settings
; ---------------------------------------------------------------------------
; This file is the ONLY place Organimizer stores settings (no registry). It
; lives next to Organimizer.exe (portable copy) or, when Organimizer was
; installed with setup, in %APPDATA%\PrimeCrew Software\Organimizer.
; (On the first start of 3.0 an existing PhotoSorter.ini was copied here;
; the old file is left untouched.)
; Delete it to reset to defaults; a fresh, fully commented copy is written
; on the next start.
;
; Lines starting with ';' are comments. Unknown keys are ignored and invalid
; values silently fall back to their defaults. Organimizer rewrites this file
; when it closes, so edit it while the app is not running.
; ===========================================================================

; ---------------------------------------------------------------------------
; [General] - what to scan
; ---------------------------------------------------------------------------
[General]

; Folder selected in the last session. Since 3.0.6 it is NOT restored on
; start-up (the Source box starts blank); see recent1..recent4 below.
; Any absolute path, or empty for none. Default: empty
last_folder = {last_folder}

; Last folders used (Recent list next to the Source box), most recent
; first, at most 4. Default: empty
{recent_general}

; Also scan every subfolder of the chosen folder.
; Each folder that contains images gets its own "Month YYYY" subfolders.
; false = only the files directly inside the chosen folder.
; Allowed: true / false. Default: false (off since 2.0.4; an explicit
; true saved by an older version is kept - set false or delete the line).
include_subfolders = {include_subfolders}

; File extensions to process (case-insensitive), separated by commas.
; Only image, camera RAW and video types are ever accepted; anything else
; (txt, pdf, xmp, json, ...) is ignored and never touched.
; Default: {default_exts}
extensions = {extensions}

; Which files the Organize tab picks up (3.0):
;   media     = images & videos from 'extensions' above (as in PhotoSorter 2.x)
;   images / videos / audio / documents / archives = built-in lists
;   all       = every file except the protected ones below
;   custom    = the extensions in 'custom_extensions'
; Never touched, whatever is chosen: programs and scripts (exe, dll, sys, msi,
; bat, cmd, ps1, vbs, com, scr, ...), shortcuts (lnk, url), ini files,
; desktop.ini, Thumbs.db, hidden or system files, .git / .svn folders.
; Default: media
file_types = {file_types}

; Extensions for file_types = custom, separated by commas. Protected types
; are ignored. Default: empty
custom_extensions = {custom_extensions}

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
;   {{/}}    = subfolder: {{YYYY}}{{/}}{{MMMM}} sorts into 2024\March (since 2.0.3)
; Characters not allowed in Windows names (\ / : * ? " < > |) become '_'.
; The "Folder name" builder in the app writes this value for you.
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
; [Window] - appearance and window layout (remembered on exit)
; ---------------------------------------------------------------------------
[Window]

; Color theme. Allowed: dark / light / system (follow the Windows app
; color setting). Default: dark
theme = {theme}

; Base font size in points. Allowed: 10 - 28. Default: 15
font_size = {font_size}

; Window size in logical pixels, remembered on exit.
; Allowed: width 640 - 10000, height 420 - 10000. Default: 1280 x 900
width = {w}
height = {h}

; Window position (top-left corner, logical pixels; may be negative on a
; second monitor). Empty = centered. If the position is not on a connected
; monitor at start-up the window is centered instead, and a size larger
; than the monitor is shrunk to fit. Default: empty
x = {x}
; (y: see x above)
y = {y}

; Start maximized. Allowed: true / false. Default: false
maximized = {maximized}

; Height of the steps 1-3 panel above the preview, in pixels.
; Empty = automatic. Allowed: 40 - 5000. Default: empty
setup_panel_height = {setup_h}

; Preview column widths in pixels, comma-separated (Files tab: Before,
; After, Current folder, Destination, Date, Action; By destination tab:
; Folder, Files). Empty or invalid = default widths. Default: empty
files_columns = {files_cols}
; (By destination tab)
folders_columns = {folders_cols}

; Which sections are expanded. Allowed: true / false.
; 1. Source & filter. Default: true
open_source = {source_open}
; 2. Rename rules. Default: true
open_rules = {rules_open}
; 3. Output folder. Default: true
open_output = {output_open}
; "More options" in 3. Output folder. Default: false
open_output_more = {output_more_open}
; Show the raw folder name template ("Advanced"). Default: false
folder_advanced = {folder_advanced}

; Tools > Options > "Reset window layout" (or deleting the keys above)
; restores the default window size, position and layout.

; Last tab: organize / plex. Default: organize
mode = {mode}

; ---------------------------------------------------------------------------
; [Blacklist] - folders that can never be chosen and are skipped while
; scanning subfolders (Tools > Folder Blacklist). Case-insensitive, whole
; folder names: C:\Windows does not block C:\WindowsStuff. * = any one
; folder name (C:\Users\*\AppData), *: = any drive, <drive roots> = choosing
; a drive root itself. custom = false: the built-in list, worked out from the
; Windows folders on every start (the blockN lines are then ignored).
; ---------------------------------------------------------------------------
[Blacklist]
; true = use the blockN lines below; false = built-in list. Default: false
custom = {blacklist_custom}
{blacklist}
; ---------------------------------------------------------------------------
; [Favorites] - favN = Name | Folder (the star button / Favorites menu)
; ---------------------------------------------------------------------------
[Favorites]
{favorites}
; ---------------------------------------------------------------------------
; [Plex] - the Plex tab
; ---------------------------------------------------------------------------
[Plex]
; Folder to analyze. Default: empty
source = {plex_source}
; Also analyze subfolders. Default: true
include_subfolders = {plex_sub}
; inplace = organize inside the source folder; roots = move movies to
; movies_root and TV episodes to tv_root. Default: inplace
destination = {plex_dest}
; Library root for movies (destination = roots). Default: empty
movies_root = {plex_movies}
; Library root for TV shows (destination = roots). Default: empty
tv_root = {plex_tv}
; Optional TMDB lookups (title / year / episode names). Off by default.
; tmdb_key is YOUR OWN TMDB API Read Access Token or API Key. It is stored
; only here, on this PC, in plain text - do not share this file with it.
; Help (F1) > "Getting a TMDB API key" explains how to get one (free).
tmdb_enabled = {tmdb_enabled}
; Your TMDB key (see above). Default: empty
tmdb_key = {tmdb_key}
; Language for TMDB titles, e.g. en-US, de-DE. Default: en-US
tmdb_language = {tmdb_language}
; Add {{tmdb-ID}} to the folder of titles confirmed on TMDB (Plex then
; skips its own guess). Default: true
tmdb_id_folder_tag = {tmdb_id_tag}
; full_sort = build Show (Year)\Season NN and Movie (Year) folders;
; rename_only = rename files where they are (nothing moves; folder name
; fixes are offered unticked). Default: rename_only
mode = {plex_mode}
; Last folders used on the Plex tab (Recent list), most recent first,
; at most 4. Default: empty
{plex_recent}

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
            x = if l.y.is_some() { opt(l.x) } else { String::new() },
            y = if l.x.is_some() { opt(l.y) } else { String::new() },
            maximized = l.maximized,
            setup_h = opt(l.setup_height),
            files_cols = format_widths(&l.files_columns),
            folders_cols = format_widths(&l.folders_columns),
            source_open = l.source_open,
            rules_open = l.rules_open,
            output_open = l.output_open,
            output_more_open = l.output_more_open,
            folder_advanced = l.folder_advanced,
            file_types = self.file_types.key(),
            custom_extensions = one(&self.custom_extensions),
            mode = if self.mode == "plex" { "plex" } else { "organize" },
            blacklist_custom = self.blacklist_custom,
            blacklist = if self.blacklist_custom {
                self.blacklist
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| format!("block{} = {}\n", i + 1, one(e)))
                    .collect::<String>()
            } else {
                String::new()
            },
            favorites = self.favorites.to_ini_lines(),
            plex_source = one(&self.plex.source),
            plex_sub = self.plex.include_subfolders,
            plex_dest = if self.plex.in_place { "inplace" } else { "roots" },
            plex_movies = one(&self.plex.movies_root),
            plex_tv = one(&self.plex.tv_root),
            tmdb_enabled = self.plex.tmdb_enabled,
            tmdb_key = one(self.plex.tmdb_key.trim()),
            tmdb_language = one(&self.plex.tmdb_language),
            tmdb_id_tag = self.plex.tmdb_id_tag,
            plex_mode = if self.plex.rename_only { "rename_only" } else { "full_sort" },
            plex_recent = recent_ini(&self.plex.recent),
            recent_general = recent_ini(&self.recent_folders),
        )
        .replace('\n', "\r\n") // Notepad-friendly line endings
    }
}

/// Most-recently-used folders kept in the INI and shown in the Recent list.
pub const RECENT_MAX: usize = 4;

/// Put `path` at the front of `list`: trimmed, surrounding quotes and a
/// trailing slash removed, duplicates dropped case-insensitively (Windows
/// paths), at most [`RECENT_MAX`] entries. Empty paths are ignored.
pub fn push_recent(list: &mut Vec<String>, path: &str) {
    let p = clean_recent(path);
    if p.is_empty() {
        return;
    }
    let k = recent_key(&p);
    list.retain(|x| recent_key(x) != k);
    list.insert(0, p);
    list.truncate(RECENT_MAX);
}

fn clean_recent(path: &str) -> String {
    let p = path.replace(['\r', '\n'], " ");
    let p = p.trim().trim_matches('"').trim();
    // keep a drive root such as "D:\" or "/" intact
    let trimmed = p.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || trimmed.ends_with(':') { p.to_string() } else { trimmed.to_string() }
}

fn recent_key(p: &str) -> String {
    clean_recent(p).replace('/', "\\").to_lowercase()
}

fn recent_from(map: Option<&BTreeMap<String, String>>) -> Vec<String> {
    let mut v: Vec<(u32, String)> = map
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| k.strip_prefix("recent").and_then(|n| n.parse().ok()).map(|n| (n, v.clone())))
        .collect();
    v.sort_by_key(|(n, _)| *n);
    let mut out = Vec::new();
    for (_, p) in v.into_iter().rev() {
        push_recent(&mut out, &p);
    }
    out
}

fn recent_ini(list: &[String]) -> String {
    list.iter()
        .take(RECENT_MAX)
        .enumerate()
        .map(|(i, p)| format!("recent{} = {}", i + 1, p.replace(['\r', '\n'], " ")))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Settings {
    /// 3.0.6: the Source boxes start blank on every launch. The folders of
    /// the last session (from an older INI too) go into the Recent lists.
    pub fn start_blank(&mut self) {
        let (f, p) = (self.last_folder.clone(), self.plex.source.clone());
        push_recent(&mut self.recent_folders, &f);
        push_recent(&mut self.plex.recent, &p);
        self.last_folder.clear();
        self.plex.source.clear();
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

/// `%APPDATA%` (Roaming) of the current user, if set.
pub fn appdata_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Where the settings live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMode {
    /// `Organimizer.ini` beside the exe (the zip; always the case before 2.0.5).
    Portable,
    /// `%APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini`.
    AppData,
}

fn is_regular_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

fn file_writable(p: &Path) -> bool {
    std::fs::OpenOptions::new().append(true).open(p).is_ok()
}

/// Can a new file be created in `dir`? Probes with a uniquely named temp
/// file that is removed again (nothing else in the folder is touched).
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".Organimizer-write-test-{}.tmp", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Decide where `Organimizer.ini` lives, in this order:
/// 1. `Organimizer.portable` beside the exe -> portable;
/// 2. a writable `Organimizer.ini` beside the exe -> portable (an existing
///    portable copy keeps working exactly as before);
/// 3. `Organimizer.installed` (written by setup) -> AppData;
/// 4. a read-only `Organimizer.ini` beside the exe -> portable, read-only
///    (as before: loaded, not saved, with a notice);
/// 5. the exe folder is writable -> portable (first start of the zip);
/// 6. otherwise (e.g. copied into Program Files by hand) -> AppData.
///
/// AppData falls back to portable when `%APPDATA%` is unknown.
pub fn choose_ini_path(exe_dir: &Path, appdata: Option<&Path>) -> (PathBuf, SettingsMode) {
    let beside = exe_dir.join(INI_NAME);
    let portable = (beside.clone(), SettingsMode::Portable);
    let in_appdata = || {
        appdata.map(|a| {
            let mut p = a.to_path_buf();
            for part in APPDATA_SUBDIR {
                p.push(part);
            }
            (p.join(INI_NAME), SettingsMode::AppData)
        })
    };
    if is_regular_file(&exe_dir.join(PORTABLE_MARKER)) {
        return portable;
    }
    let ini_exists = is_regular_file(&beside);
    if ini_exists && file_writable(&beside) {
        return portable;
    }
    if is_regular_file(&exe_dir.join(INSTALLED_MARKER)) {
        return in_appdata().unwrap_or(portable);
    }
    if ini_exists || dir_writable(exe_dir) {
        return portable;
    }
    in_appdata().unwrap_or(portable)
}

/// Path of the settings file for the running exe (see [`choose_ini_path`]).
pub fn ini_path() -> Option<PathBuf> {
    exe_dir().map(|d| choose_ini_path(&d, appdata_dir().as_deref()).0)
}

/// `Organimizer.ini` in a `%APPDATA%\PrimeCrew Software\PrimeCrew Organimizer`
/// folder, if someone ever used the 3.0.0/3.0.1 display name as the folder.
/// The official path was always `...\Organimizer`; this is only a fallback.
pub fn previous_display_ini_paths(appdata: Option<&Path>) -> Vec<PathBuf> {
    appdata
        .map(|a| {
            let mut p = a.to_path_buf();
            for part in PREV_DISPLAY_APPDATA_SUBDIR {
                p.push(part);
            }
            p.join(INI_NAME)
        })
        .into_iter()
        .collect()
}

/// Where PhotoSorter 2.x kept its settings, most likely first: for an
/// installed copy the 2.x AppData file, for a portable copy the INI next to
/// the exe (an Organimizer.exe unzipped into the old PhotoSorter folder).
pub fn legacy_ini_paths(exe_dir: &Path, appdata: Option<&Path>, mode: SettingsMode) -> Vec<PathBuf> {
    let beside = exe_dir.join(LEGACY_INI_NAME);
    let in_appdata = appdata.map(|a| {
        let mut p = a.to_path_buf();
        for part in LEGACY_APPDATA_SUBDIR {
            p.push(part);
        }
        p.join(LEGACY_INI_NAME)
    });
    match mode {
        SettingsMode::AppData => in_appdata.into_iter().chain(std::iter::once(beside)).collect(),
        SettingsMode::Portable => std::iter::once(beside).chain(in_appdata).collect(),
    }
}

/// Settings path and the PhotoSorter 2.x files to import from.
pub fn ini_path_and_legacy() -> (Option<PathBuf>, Vec<PathBuf>) {
    match exe_dir() {
        Some(d) => {
            let app = appdata_dir();
            let (p, mode) = choose_ini_path(&d, app.as_deref());
            let mut imports = previous_display_ini_paths(app.as_deref());
            imports.extend(legacy_ini_paths(&d, app.as_deref(), mode));
            (Some(p), imports)
        }
        None => (None, vec![]),
    }
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
    load_or_import(path, &[])
}

/// Like [`load_or_create`], but when `path` does not exist yet and one of
/// the `legacy` PhotoSorter.ini files does, its settings and presets are
/// imported into the new file. The old file is only read, never changed.
pub fn load_or_import(path: Option<PathBuf>, legacy: &[PathBuf]) -> Loaded {
    if let Some(p) = &path
        && !p.exists()
        && let Some((old, text)) =
            legacy.iter().filter(|l| is_regular_file(l)).find_map(|l| std::fs::read(l).ok().map(|b| (l, b)))
    {
        let settings = Settings::from_ini(&String::from_utf8_lossy(&text));
        if let Some(dir) = p.parent()
            && !dir.as_os_str().is_empty()
        {
            let _ = std::fs::create_dir_all(dir);
        }
        let saved = save(p, &settings).is_ok();
        let from_photosorter = old.file_name().is_some_and(|n| n == std::ffi::OsStr::new(LEGACY_INI_NAME));
        let what = if from_photosorter { "PhotoSorter settings and presets" } else { "settings and presets" };
        let notice = Some(if saved {
            format!("Imported {what} from {} (the old file was left as it is).", old.display())
        } else if from_photosorter {
            format!("Using PhotoSorter settings from {} (could not save {}).", old.display(), INI_NAME)
        } else {
            format!("Using settings from {} (could not save {}).", old.display(), INI_NAME)
        });
        return Loaded { settings, path: saved.then(|| p.clone()), notice };
    }
    load_plain(path)
}

fn load_plain(path: Option<PathBuf>) -> Loaded {
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
            // installed mode: %APPDATA%\PrimeCrew Software\Organimizer may not exist yet
            if let Some(dir) = path.parent()
                && !dir.as_os_str().is_empty()
            {
                let _ = std::fs::create_dir_all(dir);
            }
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

/// Write the INI safely: the new text goes to `Organimizer.ini.tmp` first,
/// the previous file is kept as `Organimizer.ini.bak`, then the temp file
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
        assert!(!s.include_subfolders); // invalid -> default (off since 2.0.4)
        assert_eq!(s.date.mode, DateMode::Auto);
        assert_eq!(s.date.auto_order, vec![SourceKind::Created, SourceKind::Filename]);
        assert_eq!(s.date.fallback, Fallback::Created);
        assert_eq!(s.font_size, 15.0);
        assert_eq!(s.window_width, 1280.0);
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
        assert!(s.include_subfolders); // explicit saved value kept
        assert!(s.rules.is_empty());
        assert!(s.sort_into_folders);
        assert!(s.presets.is_empty());
        assert_eq!(s.extensions, crate::scan::V1_DEFAULT_EXTENSIONS);
    }

    // ------------------------------------------------------------ 2.0.4 --

    #[test]
    fn fresh_default_include_subfolders_is_off() {
        assert!(!Settings::default().include_subfolders);
        assert!(!Settings::default().to_preset("x").include_subfolders);
        let text = Settings::default().to_ini();
        assert!(text.contains("include_subfolders = false"), "{text}");
        assert!(text.contains("Default: false"));
        // fresh start: no INI file -> created with the default (off)
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(INI_NAME);
        let l = load_or_create(Some(p.clone()));
        assert!(!l.settings.include_subfolders);
        assert!(std::fs::read_to_string(&p).unwrap().contains("include_subfolders = false"));
    }

    #[test]
    fn include_subfolders_explicit_true_preserved() {
        for v in ["true", "TRUE", "yes", "1", "on", " true "] {
            let s = Settings::from_ini(&format!("[General]\ninclude_subfolders = {v}\n"));
            assert!(s.include_subfolders, "{v}");
            // and it survives a save/load roundtrip
            assert!(Settings::from_ini(&s.to_ini()).include_subfolders);
        }
        let s = Settings::from_ini("[General]\ninclude_subfolders = false\n");
        assert!(!s.include_subfolders);
    }

    #[test]
    fn include_subfolders_missing_or_invalid_is_off() {
        for text in [
            "",
            "[General]\nlast_folder = C:\\Pics\n",
            "[General]\ninclude_subfolders =\n",
            "[General]\ninclude_subfolders = maybe\n",
            "[General]\ninclude_subfolders = 2\n",
            "[Date]\ninclude_subfolders = true\n", // wrong section
        ] {
            assert!(!Settings::from_ini(text).include_subfolders, "{text:?}");
        }
    }

    #[test]
    fn preset_include_subfolders_missing_off_explicit_kept() {
        let text = "[General]\ninclude_subfolders = true\n\
                    [Preset:Old]\nsource = exif\n\
                    [Preset:On]\ninclude_subfolders = true\n\
                    [Preset:Off]\ninclude_subfolders = false\n\
                    [Preset:Bad]\ninclude_subfolders = sometimes\n";
        let s = Settings::from_ini(text);
        assert!(s.include_subfolders);
        let get = |n: &str| s.presets.iter().find(|p| p.name == n).unwrap().include_subfolders;
        assert!(!get("Old")); // missing -> app default (off), not the main [General] value
        assert!(get("On"));
        assert!(!get("Off"));
        assert!(!get("Bad"));
        // explicit values are written back unchanged
        let back = Settings::from_ini(&s.to_ini());
        assert_eq!(back.presets, s.presets);
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
        assert!(!d.path().join("Organimizer.ini.bak").exists()); // nothing to back up yet
        let first = std::fs::read_to_string(&ini).unwrap();
        s.save_preset("New one");
        save(&ini, &s).unwrap();
        assert_eq!(std::fs::read_to_string(d.path().join("Organimizer.ini.bak")).unwrap(), first);
        assert!(!d.path().join("Organimizer.ini.tmp").exists());
        let now = std::fs::read_to_string(&ini).unwrap();
        assert!(now.starts_with("; ====") && now.contains("[Preset:New one]"));
        assert_eq!(Settings::from_ini(&now), s);
    }

    #[test]
    fn window_layout_and_theme_roundtrip_and_old_inis() {
        // 2.0.2 INI (no layout keys) -> defaults, theme kept
        let old = Settings { theme: Theme::Light, ..Settings::default() }.to_ini();
        let old: String = old
            .lines()
            .filter(|l| {
                let k = l.split('=').next().unwrap_or("").trim();
                !matches!(
                    k,
                    "x" | "y"
                        | "maximized"
                        | "setup_panel_height"
                        | "files_columns"
                        | "folders_columns"
                        | "open_source"
                        | "open_rules"
                        | "open_output"
                        | "open_output_more"
                        | "folder_advanced"
                )
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let s = Settings::from_ini(&old);
        assert_eq!(s.layout, WindowLayout::default());
        assert_eq!(s.theme, Theme::Light);

        let mut s = Settings::default();
        s.theme = Theme::System;
        s.window_width = 1333.0;
        s.window_height = 777.0;
        s.layout = WindowLayout {
            x: Some(-1200.0),
            y: Some(40.0),
            maximized: true,
            setup_height: Some(321.0),
            files_columns: vec![150.0, 222.0, 90.0, 140.0, 100.0, 95.0],
            folders_columns: vec![500.0, 80.0],
            source_open: false,
            rules_open: true,
            output_open: false,
            output_more_open: true,
            folder_advanced: true,
        };
        let text = s.to_ini();
        assert!(text.contains("theme = system\r\n"));
        assert!(text.contains("files_columns = 150, 222, 90, 140, 100, 95\r\n"));
        let back = Settings::from_ini(&text);
        assert_eq!(back.layout, s.layout);
        assert_eq!(back.theme, Theme::System);
        assert_eq!((back.window_width, back.window_height), (1333.0, 777.0));

        // garbage and half-set values fall back to defaults
        let bad = "[Window]\ntheme = purple\nx = 100\ny =\nmaximized = maybe\nsetup_panel_height = 2\nfiles_columns = 1, 2, x\nopen_rules = no\n";
        let b = Settings::from_ini(bad);
        assert_eq!(b.theme, Theme::Dark);
        assert_eq!((b.layout.x, b.layout.y), (None, None));
        assert!(!b.layout.maximized);
        assert_eq!(b.layout.setup_height, None);
        assert!(b.layout.files_columns.is_empty());
        assert!(!b.layout.rules_open);
        assert!(b.layout.source_open);
        assert!(Settings::from_ini("[Window]\ntheme = Auto\n").theme == Theme::System);

        // an INI written by 2.0.3 still parses in the 2.0.2 reader's style
        // (theme = system is unknown there and simply falls back to dark)
        assert!(Settings::default().to_ini().contains("\r\n[Window]\r\n"));
    }

    #[test]
    fn ini_path_portable_vs_installed() {
        use std::fs;
        let exe = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let in_app = app.path().join("PrimeCrew Software").join("Organimizer").join(INI_NAME);
        let beside = exe.path().join(INI_NAME);
        let choose = || choose_ini_path(exe.path(), Some(app.path()));

        // zip, first start: writable folder, nothing there -> beside the exe
        assert_eq!(choose(), (beside.clone(), SettingsMode::Portable));
        // the write probe leaves nothing behind
        assert_eq!(fs::read_dir(exe.path()).unwrap().count(), 0);

        // installed by setup (marker, no INI) -> AppData
        fs::write(exe.path().join(INSTALLED_MARKER), b"x").unwrap();
        assert_eq!(choose(), (in_app.clone(), SettingsMode::AppData));
        // ...unless %APPDATA% is unknown
        assert_eq!(choose_ini_path(exe.path(), None), (beside.clone(), SettingsMode::Portable));
        // a writable INI beside the exe wins over the marker
        fs::write(&beside, b"[General]\n").unwrap();
        assert_eq!(choose(), (beside.clone(), SettingsMode::Portable));
        fs::remove_file(&beside).unwrap();
        // a portable marker wins over everything
        fs::write(exe.path().join(PORTABLE_MARKER), b"").unwrap();
        assert_eq!(choose(), (beside.clone(), SettingsMode::Portable));
        fs::remove_file(exe.path().join(PORTABLE_MARKER)).unwrap();
        // a folder named like the marker / INI is not a marker / INI
        fs::remove_file(exe.path().join(INSTALLED_MARKER)).unwrap();
        fs::create_dir(exe.path().join(INSTALLED_MARKER)).unwrap();
        assert_eq!(choose(), (beside.clone(), SettingsMode::Portable));
    }

    #[test]
    fn installed_mode_creates_the_appdata_folder() {
        let app = tempfile::tempdir().unwrap();
        let ini = app.path().join("PrimeCrew Software").join("Organimizer").join(INI_NAME);
        let l = load_or_create(Some(ini.clone()));
        assert_eq!(l.path.as_deref(), Some(ini.as_path()));
        assert!(l.notice.is_none());
        assert!(ini.is_file());
        assert!(!l.settings.include_subfolders);
        // the next start reads it back
        let again = load_or_create(Some(ini.clone()));
        assert_eq!(again.settings, l.settings);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_exe_folder_without_ini_uses_appdata() {
        use std::os::unix::fs::PermissionsExt;
        let exe = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        std::fs::set_permissions(exe.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe_ok = std::fs::write(exe.path().join("x"), b"").is_ok(); // root ignores permissions
        let (p, mode) = choose_ini_path(exe.path(), Some(app.path()));
        std::fs::set_permissions(exe.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        if !probe_ok {
            assert_eq!(mode, SettingsMode::AppData);
            assert!(p.starts_with(app.path()));
        }
    }

    #[test]
    fn v3_sections_roundtrip_and_defaults() {
        let mut s = Settings::default();
        assert_eq!(s.file_types, TypeGroup::Media);
        assert!(!s.blacklist_custom && !s.blacklist.entries.is_empty());
        assert!(!s.plex.tmdb_enabled && s.plex.tmdb_key.is_empty() && s.plex.in_place);
        s.file_types = TypeGroup::Custom;
        s.custom_extensions = "psd, blend".into();
        s.blacklist = Blacklist { entries: vec!["D:\\Keep".into(), "*:\\Temp".into()] };
        s.blacklist_custom = true;
        s.favorites.add("Movies", "D:\\Media\\Movies");
        s.favorites.add("Shows; TV #1", "E:\\TV [new]");
        s.plex = PlexSettings {
            source: "D:\\Downloads".into(),
            include_subfolders: false,
            in_place: false,
            movies_root: "D:\\Media\\Movies".into(),
            tv_root: "E:\\TV".into(),
            tmdb_enabled: true,
            tmdb_key: "0123456789abcdef0123456789abcdef".into(),
            tmdb_language: "de-DE".into(),
            tmdb_id_tag: false,
            rename_only: true,
            recent: vec!["D:\\Downloads".into(), "E:\\TV\\Lost".into()],
        };
        s.mode = "plex".into();
        let text = s.to_ini();
        assert_eq!(Settings::from_ini(&text), s);
        assert!(text.contains("[Blacklist]") && text.contains("[Favorites]") && text.contains("[Plex]"));
        // default blacklist is not written out (worked out again on start)
        let d = Settings::default().to_ini();
        assert!(!d.contains("block1 ="));
        assert_eq!(Settings::from_ini(&d), Settings::default());
        // a custom but empty blacklist stays empty
        let mut e = Settings::default();
        e.blacklist = Blacklist { entries: vec![] };
        e.blacklist_custom = true;
        assert_eq!(Settings::from_ini(&e.to_ini()).blacklist.entries.len(), 0);
    }

    #[test]
    fn photosorter_settings_are_imported_once_and_never_touched() {
        let exe = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        // a PhotoSorter 2.0.5 installed copy kept its INI in AppData
        let old_dir = app.path().join("PrimeCrew Software").join("PhotoSorter");
        std::fs::create_dir_all(&old_dir).unwrap();
        let mut old = Settings::default();
        old.last_folder = "D:\\Pictures".into();
        old.rules = sample_rules();
        old.save_preset("Kindroid");
        old.include_subfolders = true;
        let old_text = old.to_ini().replace("Organimizer", "PhotoSorter");
        let old_path = old_dir.join(LEGACY_INI_NAME);
        std::fs::write(&old_path, &old_text).unwrap();
        std::fs::write(exe.path().join(INSTALLED_MARKER), b"").unwrap();

        let (new_path, mode) = choose_ini_path(exe.path(), Some(app.path()));
        assert_eq!(mode, SettingsMode::AppData);
        assert_eq!(new_path, app.path().join("PrimeCrew Software").join("Organimizer").join(INI_NAME));
        let legacy = legacy_ini_paths(exe.path(), Some(app.path()), mode);
        assert_eq!(legacy[0], old_path);
        let l = load_or_import(Some(new_path.clone()), &legacy);
        assert!(l.notice.as_deref().unwrap_or_default().contains("Imported PhotoSorter settings"));
        assert_eq!(l.settings.last_folder, "D:\\Pictures");
        assert_eq!(l.settings.rules, old.rules);
        assert!(l.settings.find_preset("Kindroid").is_some());
        assert!(l.settings.include_subfolders);
        assert!(new_path.is_file());
        // the old file is byte-for-byte unchanged and still there
        assert_eq!(std::fs::read_to_string(&old_path).unwrap(), old_text);
        // second start: the new file wins, no second import
        let mut changed = l.settings.clone();
        changed.last_folder = "E:\\New".into();
        save(&new_path, &changed).unwrap();
        let l2 = load_or_import(Some(new_path.clone()), &legacy);
        assert!(l2.notice.is_none());
        assert_eq!(l2.settings.last_folder, "E:\\New");
        // portable: PhotoSorter.ini next to the exe is imported first
        let port = tempfile::tempdir().unwrap();
        std::fs::write(port.path().join(LEGACY_INI_NAME), &old_text).unwrap();
        let (p2, m2) = choose_ini_path(port.path(), None);
        assert_eq!(m2, SettingsMode::Portable);
        let l3 = load_or_import(Some(p2.clone()), &legacy_ini_paths(port.path(), None, m2));
        assert_eq!(l3.settings.last_folder, "D:\\Pictures");
        assert!(port.path().join(LEGACY_INI_NAME).is_file() && p2.is_file());
        // nothing to import -> plain defaults
        let none = tempfile::tempdir().unwrap();
        let l4 = load_or_import(Some(none.path().join(INI_NAME)), &[none.path().join(LEGACY_INI_NAME)]);
        assert!(l4.notice.is_none());
        assert_eq!(l4.settings.last_folder, "");
    }

    #[test]
    fn previous_display_name_settings_folder_is_imported_and_never_deleted() {
        let exe = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let old_dir = app.path().join("PrimeCrew Software").join("PrimeCrew Organimizer");
        std::fs::create_dir_all(&old_dir).unwrap();
        let mut old = Settings::default();
        old.last_folder = "D:\\Movies".into();
        let old_path = old_dir.join(INI_NAME);
        std::fs::write(&old_path, old.to_ini()).unwrap();
        std::fs::write(exe.path().join(INSTALLED_MARKER), b"").unwrap();
        let (new_path, mode) = choose_ini_path(exe.path(), Some(app.path()));
        assert_eq!(mode, SettingsMode::AppData);
        assert_eq!(new_path, app.path().join("PrimeCrew Software").join("Organimizer").join(INI_NAME));
        let imports = {
            let mut v = previous_display_ini_paths(Some(app.path()));
            v.extend(legacy_ini_paths(exe.path(), Some(app.path()), mode));
            v
        };
        assert_eq!(imports[0], old_path);
        let l = load_or_import(Some(new_path.clone()), &imports);
        assert!(l.notice.as_deref().unwrap_or_default().contains("Imported settings and presets"));
        assert_eq!(l.settings.last_folder, "D:\\Movies");
        assert!(new_path.is_file());
        assert_eq!(std::fs::read_to_string(&old_path).unwrap(), old.to_ini());
        // the official Organimizer folder is used after that; the old folder stays
        let l2 = load_or_import(Some(new_path.clone()), &imports);
        assert!(l2.notice.is_none());
        assert_eq!(l2.settings.last_folder, "D:\\Movies");
        assert!(old_path.is_file());
    }

    #[test]
    fn recent_folders_mru_dedup_and_round_trip() {
        let mut v = vec![];
        for p in ["D:\\A", "D:\\B", "d:\\a\\", "\"D:\\C\"", "", "D:\\D", "D:\\E"] {
            push_recent(&mut v, p);
        }
        // most recent first, "d:\a\" deduped against "D:\A" (case-insensitive), max 4
        assert_eq!(v, vec!["D:\\E", "D:\\D", "D:\\C", "d:\\a"]);
        push_recent(&mut v, "D:\\");
        assert_eq!(v[0], "D:\\"); // a drive root keeps its backslash
        let mut s = Settings::default();
        s.recent_folders = vec!["D:\\Photos".into(), "E:\\Camera".into()];
        s.plex.recent = vec!["M:\\TV\\Lost".into()];
        s.plex.rename_only = false;
        let text = s.to_ini();
        assert!(text.contains("recent1 = D:\\Photos") && text.contains("mode = full_sort"), "{text}");
        assert_eq!(Settings::from_ini(&text), s);
    }

    #[test]
    fn source_boxes_start_blank_and_last_folders_become_recent() {
        let mut s = Settings::default();
        s.last_folder = "D:\\Photos\\2024".into();
        s.plex.source = "M:\\Downloads".into();
        s.recent_folders = vec!["D:\\Old".into(), "d:\\photos\\2024".into()];
        s.start_blank();
        assert_eq!(s.last_folder, "");
        assert_eq!(s.plex.source, "");
        assert_eq!(s.recent_folders, vec!["D:\\Photos\\2024", "D:\\Old"]);
        assert_eq!(s.plex.recent, vec!["M:\\Downloads"]);
        // an older INI with only last_folder / [Plex] source
        let mut old = Settings::from_ini("[General]\nlast_folder = C:\\Pics\n[Plex]\nsource = M:\\Rips\n");
        old.start_blank();
        assert_eq!(
            (old.recent_folders.clone(), old.plex.recent.clone()),
            (vec!["C:\\Pics".into()], vec!["M:\\Rips".into()])
        );
        assert!(old.plex.rename_only, "Rename only is the default mode");
    }
}
