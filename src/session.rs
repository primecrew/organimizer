//! Session-level helpers used by the File and Presets menus: "New Project"
//! and read-only preset summaries. Pure functions on [`Settings`]; nothing
//! here touches the disk.

use std::path::Path;

use crate::date_source::DateMode;
use crate::filename_date::DateOrder;
use crate::settings::{Preset, PresetScope, Settings};

/// Settings for File > New Project: clears the source folder and resets the
/// rule chain and the output-stage options (Month YYYY on/off, folder
/// template, date source, ambiguous order, fallback) to their defaults.
///
/// Kept unchanged: saved presets, the extension list and the Auto date
/// priority (Tools > Options), "Include subfolders", theme, font size and
/// window size. Never touches files or undo logs on disk.
pub fn new_project(s: &Settings) -> Settings {
    let d = Settings::default();
    let mut n = s.clone();
    n.last_folder = d.last_folder;
    n.rules = d.rules;
    n.sort_into_folders = d.sort_into_folders;
    n.folder_template = d.folder_template;
    n.date.mode = d.date.mode;
    n.date.order = d.date.order;
    n.date.fallback = d.date.fallback;
    n
}

/// Reasons to ask "Start a new project?" before resetting. Empty = no
/// confirmation needed.
///
/// * `pending_changes`: files the current preview would change (not applied).
/// * `undo_log`: the not-yet-undone undo log of the current folder, if any.
/// * `rules_unsaved`: the rule chain is non-empty and not identical to any
///   saved preset (it would be lost).
pub fn new_project_warnings(pending_changes: usize, undo_log: Option<&Path>, rules_unsaved: bool) -> Vec<String> {
    let mut v = Vec::new();
    if pending_changes > 0 {
        v.push(format!("The preview shows {pending_changes} file(s) that would change; they have not been applied."));
    }
    if let Some(log) = undo_log {
        let name = log.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        v.push(format!(
            "The last Apply in this folder has not been undone. Its undo log ({name}) stays on disk; choose the folder again to use “Undo last run”."
        ));
    }
    if rules_unsaved {
        v.push("The current rename rules are not saved as a preset and will be cleared.".into());
    }
    v
}

/// True when `s.rules` is non-empty and no saved preset has exactly this chain.
pub fn rules_unsaved(s: &Settings) -> bool {
    !s.rules.is_empty() && !s.presets.iter().any(|p| p.rules == s.rules)
}

/// Read-only description of a preset for Presets > Manage Presets.
pub fn preset_summary(p: &Preset) -> Vec<String> {
    let mut v = vec![format!("Loads: {}", p.scope.label())];
    let rules = matches!(p.scope, PresetScope::Both | PresetScope::RenameOnly);
    let folder = matches!(p.scope, PresetScope::Both | PresetScope::FolderOnly);
    if rules {
        if p.rules.is_empty() {
            v.push("Rename rules: none (names are kept)".into());
        } else {
            v.push(format!("Rename rules ({}):", p.rules.len()));
            for (i, r) in p.rules.iter().enumerate() {
                let off = if r.enabled { "" } else { "  (off)" };
                v.push(format!("  {}. {} — {}{off}", i + 1, r.kind.type_label(), r.kind.summary()));
            }
        }
    }
    if folder {
        if p.sort_into_folders {
            v.push(format!("Month YYYY folders: on, template “{}”", p.folder_template));
        } else {
            v.push("Month YYYY folders: off (rename in place)".into());
        }
        let order: Vec<&str> = p.date.auto_order.iter().map(|k| k.label()).collect();
        let mode = match p.date.mode {
            DateMode::Auto => format!("{} ({})", p.date.mode.label(), order.join(" > ")),
            m => m.label().to_string(),
        };
        v.push(format!("Date source: {mode}"));
        let amb = match p.date.order {
            DateOrder::Us => "US (MM-DD)",
            DateOrder::Intl => "Intl (DD-MM)",
        };
        v.push(format!("Ambiguous dates: {amb}; if no date: {}", p.date.fallback.label()));
    }
    if p.scope == PresetScope::Both {
        v.push(format!(
            "Source: {}; file types: {}",
            if p.include_subfolders { "include subfolders" } else { "this folder only" },
            crate::scan::extension_summary(&p.extensions)
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date_source::{Fallback, SourceKind};
    use crate::rename::{CaseMode, RuleKind, RuleSpec};
    use crate::settings::Theme;

    fn busy_settings() -> Settings {
        let mut s = Settings::default();
        s.last_folder = r"D:\Photos\2024".into();
        s.include_subfolders = false;
        s.extensions = "jpg, mp4".into();
        s.rules = vec![RuleSpec::new(RuleKind::Case(CaseMode::Upper))];
        s.sort_into_folders = false;
        s.folder_template = "{YYYY}-{MM}".into();
        s.date.mode = DateMode::Only(SourceKind::Exif);
        s.date.order = DateOrder::Intl;
        s.date.fallback = Fallback::Skip;
        s.date.auto_order = vec![SourceKind::Exif, SourceKind::Modified];
        s.theme = Theme::Light;
        s.font_size = 19.0;
        s.window_width = 1500.0;
        s.save_preset("Keep me");
        s
    }

    #[test]
    fn new_project_resets_session_and_keeps_preferences() {
        let s = busy_settings();
        let n = new_project(&s);
        let d = Settings::default();
        // reset
        assert_eq!(n.last_folder, "");
        assert!(n.rules.is_empty());
        assert_eq!(n.sort_into_folders, d.sort_into_folders);
        assert_eq!(n.folder_template, d.folder_template);
        assert_eq!(n.date.mode, d.date.mode);
        assert_eq!(n.date.order, d.date.order);
        assert_eq!(n.date.fallback, d.date.fallback);
        // kept
        assert_eq!(n.presets, s.presets);
        assert_eq!(n.extensions, "jpg, mp4");
        assert_eq!(n.date.auto_order, s.date.auto_order);
        assert!(!n.include_subfolders);
        assert_eq!((n.theme, n.font_size, n.window_width), (Theme::Light, 19.0, 1500.0));
        // idempotent
        assert_eq!(new_project(&n), n);
    }

    #[test]
    fn new_project_never_touches_disk_or_undo_log() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("PhotoSorter-undo-20240101-120000.json");
        std::fs::write(&log, b"{}").unwrap();
        std::fs::write(d.path().join("a.jpg"), b"x").unwrap();
        let mut s = busy_settings();
        s.last_folder = d.path().to_string_lossy().into_owned();
        let before = crate::ops::find_latest_log(d.path());
        let _ = new_project(&s);
        assert_eq!(crate::ops::find_latest_log(d.path()), before);
        assert_eq!(std::fs::read(&log).unwrap(), b"{}");
        assert!(d.path().join("a.jpg").is_file());
    }

    #[test]
    fn new_project_warnings_only_when_needed() {
        assert!(new_project_warnings(0, None, false).is_empty());
        assert_eq!(new_project_warnings(3, None, false).len(), 1);
        let w = new_project_warnings(0, Some(Path::new("C:/x/PhotoSorter-undo-1.json")), true);
        assert_eq!(w.len(), 2);
        assert!(w[0].contains("PhotoSorter-undo-1.json"), "{w:?}");
        let mut s = busy_settings();
        assert!(!rules_unsaved(&s)); // identical chain saved as "Keep me"
        s.rules.push(RuleSpec::new(RuleKind::Case(CaseMode::Lower)));
        assert!(rules_unsaved(&s));
        s.rules.clear();
        assert!(!rules_unsaved(&s));
    }

    #[test]
    fn preset_summary_follows_scope() {
        let mut s = busy_settings();
        let p = s.find_preset("keep me").unwrap().clone();
        let all = preset_summary(&p).join("\n");
        assert!(all.contains("Rename rules (1)") && all.contains("Month YYYY folders: off"), "{all}");
        s.set_preset_scope("Keep me", PresetScope::RenameOnly);
        let r = preset_summary(s.find_preset("Keep me").unwrap()).join("\n");
        assert!(r.contains("Rename rules") && !r.contains("Month YYYY"), "{r}");
        s.set_preset_scope("Keep me", PresetScope::FolderOnly);
        let f = preset_summary(s.find_preset("Keep me").unwrap()).join("\n");
        assert!(!f.contains("Rename rules") && f.contains("Date source"), "{f}");
    }
}
