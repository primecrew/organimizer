//! Planning: turn scanned files into a list of operations, without touching
//! the disk (other than reading metadata).
//!
//! Each file becomes a [`PlanItem`] which is passed through an ordered list of
//! [`Stage`]s. Today:
//!
//! 1. [`RenameStage`] – placeholder for future Renamer-style rules; keeps the
//!    original name.
//! 2. [`FolderStage`] – computes the `Month YYYY` destination folder from the
//!    folder-name template.
//!
//! Then collisions are resolved (never overwrite: " (1)", " (2)", ...).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::date_source::{DateConfig, ResolvedDate, resolve_date};
use crate::template::{DEFAULT_TEMPLATE, format_folder_name, is_sorted_folder_name};

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Will be moved to `dest_path`.
    Move,
    /// Already in the correct folder.
    AlreadySorted,
    /// No date could be determined (and fallback = skip).
    NoDate,
    /// Could not read the file, etc.
    Error(String),
}

impl Status {
    pub fn label(&self) -> &str {
        match self {
            Status::Move => "Move",
            Status::AlreadySorted => "Already sorted",
            Status::NoDate => "No date (skipped)",
            Status::Error(_) => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    pub source: PathBuf,
    /// Folder the file currently lives in.
    pub current_dir: PathBuf,
    /// Folder in which the `Month YYYY` folder is created (normally
    /// `current_dir`; its parent if the file already sits in a sorted folder).
    pub base_dir: PathBuf,
    pub date: Option<ResolvedDate>,
    /// File name after the rename stage (no collision suffix yet).
    pub new_name: String,
    /// Destination folder name, e.g. "October 2026".
    pub dest_folder_name: String,
    pub dest_dir: PathBuf,
    /// Final destination including any collision suffix.
    pub dest_path: PathBuf,
    /// True if a " (n)" suffix was added.
    pub renamed_for_collision: bool,
    pub status: Status,
}

impl PlanItem {
    fn new(source: PathBuf) -> Self {
        let current_dir = source.parent().map(Path::to_path_buf).unwrap_or_default();
        let new_name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        PlanItem {
            base_dir: current_dir.clone(),
            current_dir,
            date: None,
            new_name,
            dest_folder_name: String::new(),
            dest_dir: PathBuf::new(),
            dest_path: PathBuf::new(),
            renamed_for_collision: false,
            status: Status::Move,
            source,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub template: String,
    pub date: DateConfig,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions { template: DEFAULT_TEMPLATE.to_string(), date: DateConfig::default() }
    }
}

/// Shared data available to every stage.
pub struct StageContext<'a> {
    pub root: &'a Path,
    pub options: &'a PlanOptions,
}

/// One step of the pipeline. Stages only run on items whose status is
/// still [`Status::Move`].
pub trait Stage: Send + Sync {
    fn name(&self) -> &str;
    fn process(&self, item: &mut PlanItem, ctx: &StageContext);
}

/// Future home of Renamer-style rules (find/replace, case, numbering, date
/// insertion...). Currently keeps the original file name.
#[derive(Default)]
pub struct RenameStage {
    // pub rules: Vec<Box<dyn RenameRule>>,
}

impl Stage for RenameStage {
    fn name(&self) -> &str {
        "rename"
    }
    fn process(&self, _item: &mut PlanItem, _ctx: &StageContext) {}
}

/// Computes the destination folder from the template.
pub struct FolderStage;

impl Stage for FolderStage {
    fn name(&self) -> &str {
        "folder"
    }
    fn process(&self, item: &mut PlanItem, ctx: &StageContext) {
        let Some(date) = item.date else {
            item.status = Status::NoDate;
            return;
        };
        let folder = format_folder_name(&ctx.options.template, date.date);
        item.dest_dir = item.base_dir.join(&folder);
        // Already in the right folder? (case-insensitive like Windows)
        let cur_name = item.current_dir.file_name().map(|n| n.to_string_lossy().to_lowercase());
        if item.current_dir.parent() == Some(item.base_dir.as_path())
            && cur_name.as_deref() == Some(&folder.to_lowercase())
        {
            item.status = Status::AlreadySorted;
        }
        item.dest_folder_name = folder;
    }
}

pub fn default_stages() -> Vec<Box<dyn Stage>> {
    vec![Box::new(RenameStage::default()), Box::new(FolderStage)]
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub root: PathBuf,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanCounts {
    pub total: usize,
    pub to_move: usize,
    pub already_sorted: usize,
    pub no_date: usize,
    pub errors: usize,
    pub collisions: usize,
    pub new_folders: usize,
    /// Distinct destination folders (new or existing).
    pub dest_folders: usize,
}

impl Plan {
    pub fn counts(&self) -> PlanCounts {
        let mut c = PlanCounts { total: self.items.len(), ..Default::default() };
        let mut folders = HashSet::new();
        let mut all_folders = HashSet::new();
        for it in &self.items {
            match it.status {
                Status::Move => {
                    c.to_move += 1;
                    if it.renamed_for_collision {
                        c.collisions += 1;
                    }
                    all_folders.insert(it.dest_dir.clone());
                    if !it.dest_dir.exists() {
                        folders.insert(it.dest_dir.clone());
                    }
                }
                Status::AlreadySorted => c.already_sorted += 1,
                Status::NoDate => c.no_date += 1,
                Status::Error(_) => c.errors += 1,
            }
        }
        c.new_folders = folders.len();
        c.dest_folders = all_folders.len();
        c
    }
}

/// Split "name.ext" into ("name", ".ext"). Dotfiles keep their name.
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn key(p: &Path) -> String {
    // Windows file systems are case-insensitive.
    p.to_string_lossy().to_lowercase()
}

/// Find a free path in `dir` for `name`, appending " (1)", " (2)" ... before
/// the extension. A path is taken if it exists on disk or is in `reserved`.
pub fn unique_dest(dir: &Path, name: &str, reserved: &HashSet<String>) -> (PathBuf, bool) {
    let first = dir.join(name);
    if !first.exists() && !reserved.contains(&key(&first)) {
        return (first, false);
    }
    let (stem, ext) = split_name(name);
    for n in 1.. {
        let cand = dir.join(format!("{stem} ({n}){ext}"));
        if !cand.exists() && !reserved.contains(&key(&cand)) {
            return (cand, true);
        }
    }
    unreachable!()
}

/// Build a plan for `files` (as returned by [`crate::scan::scan`]).
pub fn build_plan(
    root: &Path,
    files: &[PathBuf],
    options: &PlanOptions,
    stages: &[Box<dyn Stage>],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Plan {
    let ctx = StageContext { root, options };
    let mut sorted_cache: HashMap<PathBuf, bool> = HashMap::new();
    let mut reserved: HashSet<String> = HashSet::new();
    let mut items = Vec::with_capacity(files.len());

    for (i, f) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if i % 64 == 0 {
            progress(i, files.len());
        }
        let mut item = PlanItem::new(f.clone());

        // If the file already sits in a "Month YYYY"-style folder below the
        // root, its siblings-folders live in the grandparent.
        if item.current_dir != root && item.current_dir.starts_with(root) {
            let is_sorted = *sorted_cache.entry(item.current_dir.clone()).or_insert_with(|| {
                item.current_dir
                    .file_name()
                    .map(|n| is_sorted_folder_name(&options.template, &n.to_string_lossy()))
                    .unwrap_or(false)
            });
            if is_sorted && let Some(gp) = item.current_dir.parent() {
                item.base_dir = gp.to_path_buf();
            }
        }

        match std::fs::metadata(f) {
            Ok(meta) => item.date = resolve_date(f, Some(&meta), &options.date),
            Err(e) => item.status = Status::Error(format!("Cannot read metadata: {e}")),
        }

        for st in stages {
            if item.status != Status::Move {
                break;
            }
            st.process(&mut item, &ctx);
        }

        if item.status == Status::Move {
            let (dest, renamed) = unique_dest(&item.dest_dir, &item.new_name, &reserved);
            reserved.insert(key(&dest));
            item.dest_path = dest;
            item.renamed_for_collision = renamed;
        }
        items.push(item);
    }
    progress(files.len(), files.len());
    Plan { root: root.to_path_buf(), items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    use crate::date_source::{DateMode, SourceKind};

    pub(crate) fn touch(p: &Path, unix: u64) {
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, p.to_string_lossy().as_bytes()).unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(unix);
        fs::File::options().write(true).open(p).unwrap().set_modified(t).unwrap();
    }

    fn mod_opts() -> PlanOptions {
        PlanOptions {
            template: DEFAULT_TEMPLATE.into(),
            date: DateConfig { mode: DateMode::Only(SourceKind::Modified), ..Default::default() },
        }
    }

    // 2024-03-15 12:00 UTC: March 2024 in every time zone.
    const MAR_2024: u64 = 1_710_504_000;
    // 2023-07-15 12:00 UTC
    const JUL_2023: u64 = 1_689_422_400;

    fn plan_for(root: &Path, opts: &PlanOptions) -> Plan {
        let files =
            crate::scan::scan(root, true, &crate::scan::parse_extensions(""), &AtomicBool::new(false), &mut |_, _| {})
                .files;
        build_plan(root, &files, opts, &default_stages(), &AtomicBool::new(false), &mut |_, _| {})
    }

    #[test]
    fn split_names() {
        assert_eq!(split_name("a.jpg"), ("a", ".jpg"));
        assert_eq!(split_name("a.b.jpg"), ("a.b", ".jpg"));
        assert_eq!(split_name(".hidden"), (".hidden", ""));
        assert_eq!(split_name("noext"), ("noext", ""));
    }

    #[test]
    fn collision_suffixes() {
        let d = tempfile::tempdir().unwrap();
        let mut reserved = HashSet::new();
        let (p0, r0) = unique_dest(d.path(), "a.jpg", &reserved);
        assert!(!r0);
        fs::write(&p0, b"x").unwrap();
        let (p1, r1) = unique_dest(d.path(), "a.jpg", &reserved);
        assert!(r1);
        assert_eq!(p1.file_name().unwrap(), "a (1).jpg");
        reserved.insert(key(&p1));
        let (p2, _) = unique_dest(d.path(), "a.jpg", &reserved);
        assert_eq!(p2.file_name().unwrap(), "a (2).jpg");
        // case-insensitive reservation
        reserved.insert(key(&d.path().join("B.JPG")));
        let (p3, r3) = unique_dest(d.path(), "b.jpg", &reserved);
        assert!(r3);
        assert_eq!(p3.file_name().unwrap(), "b (1).jpg");
    }

    #[test]
    fn plan_basic_and_per_parent_folders() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024);
        touch(&r.join("sub/b.png"), JUL_2023);
        let plan = plan_for(r, &mod_opts());
        assert_eq!(plan.items.len(), 2);
        let a = plan.items.iter().find(|i| i.source.ends_with("a.jpg")).unwrap();
        assert_eq!(a.dest_path, r.join("March 2024").join("a.jpg"));
        let b = plan.items.iter().find(|i| i.source.ends_with("b.png")).unwrap();
        assert_eq!(b.dest_path, r.join("sub").join("July 2023").join("b.png"));
        assert_eq!(plan.counts().to_move, 2);
        assert_eq!(plan.counts().new_folders, 2);
    }

    #[test]
    fn skip_already_sorted_and_fix_misfiled() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("March 2024/ok.jpg"), MAR_2024);
        touch(&r.join("march 2024/ok2.jpg"), MAR_2024); // case-insensitive (may be same dir on case-insensitive fs)
        touch(&r.join("March 2024/wrong.jpg"), JUL_2023);
        let plan = plan_for(r, &mod_opts());
        let ok = plan.items.iter().find(|i| i.source.ends_with("ok.jpg")).unwrap();
        assert_eq!(ok.status, Status::AlreadySorted);
        let ok2 = plan.items.iter().find(|i| i.source.ends_with("ok2.jpg")).unwrap();
        assert_eq!(ok2.status, Status::AlreadySorted);
        let wrong = plan.items.iter().find(|i| i.source.ends_with("wrong.jpg")).unwrap();
        assert_eq!(wrong.status, Status::Move);
        // goes to a sibling folder, not nested inside "March 2024"
        assert_eq!(wrong.dest_path, r.join("July 2023").join("wrong.jpg"));
    }

    #[test]
    fn plan_collisions_with_existing_and_within_run() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("March 2024/x.jpg"), JUL_2023); // existing occupant, misfiled -> moves away
        touch(&r.join("x.jpg"), MAR_2024);
        touch(&r.join("July 2023/x.jpg"), JUL_2023); // already sorted, occupies the slot
        let plan = plan_for(r, &mod_opts());
        let top = plan.items.iter().find(|i| i.source == r.join("x.jpg")).unwrap();
        // March 2024/x.jpg exists on disk -> suffix
        assert_eq!(top.dest_path, r.join("March 2024").join("x (1).jpg"));
        let mis = plan.items.iter().find(|i| i.source == r.join("March 2024/x.jpg")).unwrap();
        assert_eq!(mis.dest_path, r.join("July 2023").join("x (1).jpg"));
        assert_eq!(plan.counts().collisions, 2);
    }

    #[test]
    fn no_date_is_skipped_with_skip_fallback() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("holiday.jpg"), MAR_2024);
        touch(&r.join("IMG_20200101_000000.jpg"), MAR_2024);
        let opts = PlanOptions {
            template: DEFAULT_TEMPLATE.into(),
            date: DateConfig {
                mode: DateMode::Only(SourceKind::Filename),
                fallback: crate::date_source::Fallback::Skip,
                ..Default::default()
            },
        };
        let plan = plan_for(r, &opts);
        let h = plan.items.iter().find(|i| i.source.ends_with("holiday.jpg")).unwrap();
        assert_eq!(h.status, Status::NoDate);
        let i = plan.items.iter().find(|i| i.source.ends_with("IMG_20200101_000000.jpg")).unwrap();
        assert_eq!(i.dest_path, r.join("January 2020").join("IMG_20200101_000000.jpg"));
    }
}
