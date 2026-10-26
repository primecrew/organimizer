//! Executing a plan, the undo log, and undo.
//!
//! Safety rules:
//! * never overwrite: a destination that exists is given a " (n)" suffix;
//! * `std::fs::rename` first, copy + verify + delete only across devices;
//! * the undo log is written *before* the first move and kept up to date,
//!   so an interrupted run can still be undone;
//! * undo only ever deletes folders that this app created and that are empty;
//! * renames (from the rule chain) are recorded exactly like moves, so undo
//!   restores both the original name and the original folder.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::plan::{Plan, Status, same_file, unique_dest_for};

pub const UNDO_PREFIX: &str = "PhotoSorter-undo-";
pub const UNDO_DONE_SUFFIX: &str = ".undone.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoveRecord {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UndoLog {
    pub app: String,
    pub version: u32,
    /// Local time the run started, `YYYY-MM-DD HH:MM:SS`.
    pub started: String,
    pub root: PathBuf,
    pub moves: Vec<MoveRecord>,
    /// Folders created by this run (only these may be removed by undo).
    pub created_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub finished: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub moved: usize,
    pub skipped: usize,
    pub copied_across_devices: usize,
    pub errors: Vec<(PathBuf, String)>,
    pub undo_log: Option<PathBuf>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UndoReport {
    pub restored: usize,
    pub dirs_removed: usize,
    pub errors: Vec<(PathBuf, String)>,
    pub log_path: PathBuf,
}

fn is_cross_device(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    // EXDEV on Unix = 18, ERROR_NOT_SAME_DEVICE on Windows = 17
    matches!(e.raw_os_error(), Some(18) if cfg!(unix)) || matches!(e.raw_os_error(), Some(17) if cfg!(windows))
}

/// Move one file without ever overwriting. Returns true if a copy+delete was
/// needed (different volume).
pub fn move_file(from: &Path, to: &Path) -> io::Result<bool> {
    if to.exists() {
        if from != to && same_file(from, to) {
            // case-only rename on a case-insensitive drive (a.JPG -> a.jpg)
            case_only_rename(from, to)?;
            return Ok(false);
        }
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"));
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(false),
        Err(e) if is_cross_device(&e) => {
            copy_then_delete(from, to)?;
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

/// Rename via a unique temporary name in the same folder, so that only the
/// letter case changes. Rolls back if the second step fails.
fn case_only_rename(from: &Path, to: &Path) -> io::Result<()> {
    let dir = from.parent().unwrap_or(Path::new("."));
    let mut n = 0u32;
    let tmp = loop {
        let t = dir.join(format!(".photosorter-case-{}-{n}.tmp", std::process::id()));
        if !t.exists() {
            break t;
        }
        n += 1;
    };
    fs::rename(from, &tmp)?;
    if to.exists() {
        // something else appeared meanwhile: put it back, never overwrite
        let _ = fs::rename(&tmp, from);
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"));
    }
    if let Err(e) = fs::rename(&tmp, to) {
        let _ = fs::rename(&tmp, from);
        return Err(e);
    }
    Ok(())
}

fn copy_then_delete(from: &Path, to: &Path) -> io::Result<()> {
    let src_meta = fs::metadata(from)?;
    {
        let mut src = fs::File::open(from)?;
        // create_new: fails if the destination appeared meanwhile (no overwrite)
        let mut dst = fs::File::create_new(to)?;
        let res = io::copy(&mut src, &mut dst).and_then(|n| {
            dst.sync_all()?;
            if n != src_meta.len() {
                return Err(io::Error::other("copy size mismatch"));
            }
            if let Ok(m) = src_meta.modified() {
                let _ = dst.set_modified(m);
            }
            Ok(())
        });
        if let Err(e) = res {
            drop(dst);
            let _ = fs::remove_file(to); // remove our own partial copy only
            return Err(e);
        }
    }
    fs::remove_file(from)
}

fn now_local() -> chrono::DateTime<chrono::Local> {
    chrono::Local::now()
}

fn write_log(path: &Path, log: &UndoLog) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(log).map_err(io::Error::other)?;
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// Where the undo log for a run in `root` goes.
pub fn new_log_path(root: &Path) -> PathBuf {
    let ts = now_local().format("%Y%m%d-%H%M%S");
    let mut p = root.join(format!("{UNDO_PREFIX}{ts}.json"));
    let mut n = 1;
    while p.exists() {
        p = root.join(format!("{UNDO_PREFIX}{ts}-{n}.json"));
        n += 1;
    }
    p
}

/// Execute the `Move` items of a plan.
pub fn execute(plan: &Plan, cancel: &AtomicBool, progress: &mut dyn FnMut(usize, usize, &Path)) -> RunReport {
    let mut report = RunReport::default();
    let log_path = new_log_path(&plan.root);
    let mut log = UndoLog {
        app: "PhotoSorter".into(),
        version: 1,
        started: now_local().format("%Y-%m-%d %H:%M:%S").to_string(),
        root: plan.root.clone(),
        moves: vec![],
        created_dirs: vec![],
        finished: false,
    };
    if let Err(e) = write_log(&log_path, &log) {
        report.errors.push((log_path, format!("Cannot write undo log, nothing was moved: {e}")));
        return report;
    }
    report.undo_log = Some(log_path.clone());

    let todo: Vec<_> = plan.items.iter().filter(|i| i.status == Status::Move).collect();
    report.skipped = plan.items.len() - todo.len();
    let total = todo.len();
    let mut created: HashSet<PathBuf> = HashSet::new();
    let mut dirty = 0usize;

    for (i, item) in todo.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(i, total, &item.source);

        if !item.source.is_file() {
            report.errors.push((item.source.clone(), "Source file no longer exists".into()));
            continue;
        }
        if !item.dest_dir.exists() {
            match fs::create_dir(&item.dest_dir) {
                Ok(()) => {
                    created.insert(item.dest_dir.clone());
                    log.created_dirs.push(item.dest_dir.clone());
                    dirty += 1;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    report
                        .errors
                        .push((item.source.clone(), format!("Cannot create folder {}: {e}", item.dest_dir.display())));
                    continue;
                }
            }
        }
        // Re-check collisions at apply time (disk may have changed).
        let dest = if item.dest_path.exists() && !same_file(&item.source, &item.dest_path) {
            unique_dest_for(&item.dest_dir, &item.new_name, &HashSet::new(), Some(&item.source)).0
        } else {
            item.dest_path.clone()
        };
        match move_file(&item.source, &dest) {
            Ok(copied) => {
                report.moved += 1;
                if copied {
                    report.copied_across_devices += 1;
                }
                log.moves.push(MoveRecord { from: item.source.clone(), to: dest });
                dirty += 1;
            }
            Err(e) => report.errors.push((item.source.clone(), format!("Move failed: {e}"))),
        }
        if dirty >= 25 {
            let _ = write_log(&log_path, &log);
            dirty = 0;
        }
    }
    progress(total, total, Path::new(""));
    log.finished = true;
    if let Err(e) = write_log(&log_path, &log) {
        report.errors.push((log_path.clone(), format!("Could not finalise undo log: {e}")));
    }
    if log.moves.is_empty() && log.created_dirs.is_empty() {
        // Nothing to undo, don't leave clutter behind.
        let _ = fs::remove_file(&log_path);
        report.undo_log = None;
    }
    report
}

/// Most recent not-yet-undone log in `root`.
pub fn find_latest_log(root: &Path) -> Option<PathBuf> {
    let mut logs: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            n.starts_with(UNDO_PREFIX) && n.ends_with(".json") && !n.ends_with(UNDO_DONE_SUFFIX)
        })
        .collect();
    // timestamped names sort chronologically
    logs.sort();
    logs.pop()
}

pub fn read_log(path: &Path) -> io::Result<UndoLog> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Undo the run recorded in `log_path`.
pub fn undo(log_path: &Path, progress: &mut dyn FnMut(usize, usize)) -> UndoReport {
    let mut report = UndoReport { log_path: log_path.to_path_buf(), ..Default::default() };
    let log = match read_log(log_path) {
        Ok(l) => l,
        Err(e) => {
            report.errors.push((log_path.to_path_buf(), format!("Cannot read undo log: {e}")));
            return report;
        }
    };
    let mut remaining = Vec::new();
    let total = log.moves.len();
    for (i, m) in log.moves.iter().enumerate().rev() {
        progress(total - i, total);
        if !m.to.is_file() {
            report.errors.push((m.to.clone(), "File is no longer where PhotoSorter put it".into()));
            remaining.push(m.clone());
            continue;
        }
        if m.from.exists() && !same_file(&m.from, &m.to) {
            report.errors.push((m.from.clone(), "Original location is occupied, not overwriting".into()));
            remaining.push(m.clone());
            continue;
        }
        if let Some(parent) = m.from.parent()
            && !parent.exists()
            && let Err(e) = fs::create_dir_all(parent)
        {
            report.errors.push((m.from.clone(), format!("Cannot recreate folder: {e}")));
            remaining.push(m.clone());
            continue;
        }
        match move_file(&m.to, &m.from) {
            Ok(_) => report.restored += 1,
            Err(e) => {
                report.errors.push((m.to.clone(), format!("Move back failed: {e}")));
                remaining.push(m.clone());
            }
        }
    }
    // Remove created folders, deepest first, only if empty.
    let mut dirs = log.created_dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    let mut remaining_dirs = Vec::new();
    for d in dirs {
        if !d.exists() {
            continue;
        }
        let empty = fs::read_dir(&d).map(|mut r| r.next().is_none()).unwrap_or(false);
        if empty && fs::remove_dir(&d).is_ok() {
            report.dirs_removed += 1;
        } else {
            remaining_dirs.push(d);
        }
    }

    if remaining.is_empty() {
        let done = log_path.with_file_name(
            log_path.file_name().unwrap().to_string_lossy().trim_end_matches(".json").to_string() + UNDO_DONE_SUFFIX,
        );
        if let Err(e) = fs::rename(log_path, &done) {
            report.errors.push((log_path.to_path_buf(), format!("Could not mark log as undone: {e}")));
        }
    } else {
        remaining.reverse();
        let left = UndoLog { moves: remaining, created_dirs: remaining_dirs, ..log };
        if let Err(e) = write_log(log_path, &left) {
            report.errors.push((log_path.to_path_buf(), format!("Could not update undo log: {e}")));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date_source::{DateConfig, DateMode, SourceKind};
    use crate::plan::{PlanOptions, build_plan, default_stages};
    use std::time::{Duration, SystemTime};

    fn touch(p: &Path, unix: u64, content: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(unix))
            .unwrap();
    }

    fn snapshot(root: &Path) -> Vec<(String, String)> {
        let mut out = vec![];
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                if rel.starts_with(UNDO_PREFIX) {
                    continue;
                }
                if p.is_dir() {
                    out.push((rel + "/", String::new()));
                    stack.push(p);
                } else {
                    out.push((rel, fs::read_to_string(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    const MAR_2024: u64 = 1_710_504_000;
    const JUL_2023: u64 = 1_689_422_400;
    const OCT_2026: u64 = 1_792_843_200; // 2026-10-25 12:00 UTC

    #[test]
    fn end_to_end_plan_apply_undo() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024, "a");
        touch(&r.join("B.JPG"), JUL_2023, "b");
        touch(&r.join("IMG_20200105_101010.png"), MAR_2024, "img"); // filename date wins in Auto
        touch(&r.join("notes.txt"), MAR_2024, "txt"); // not an image
        touch(&r.join("Trip/c.heic"), OCT_2026, "c");
        touch(&r.join("Trip/sub/d.webp"), MAR_2024, "d");
        touch(&r.join("Trip/October 2026/e.jpg"), OCT_2026, "e"); // already sorted
        touch(&r.join("March 2024/a.jpg"), MAR_2024, "existing"); // collision target + already sorted
        let before = snapshot(r);

        let opts =
            PlanOptions { date: DateConfig { mode: DateMode::Auto, ..Default::default() }, ..Default::default() };
        let exts = crate::scan::parse_extensions("");
        let no = AtomicBool::new(false);
        let files = crate::scan::scan(r, true, &exts, &no, &mut |_, _| {}).files;
        assert_eq!(files.len(), 7);
        let plan = build_plan(r, &files, &opts, &default_stages(), &no, &mut |_, _| {});
        let c = plan.counts();
        assert_eq!(c.to_move, 5);
        assert_eq!(c.already_sorted, 2);
        assert_eq!(c.collisions, 1);

        let rep = execute(&plan, &no, &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(rep.moved, 5);
        let log_path = rep.undo_log.clone().unwrap();
        assert!(log_path.exists());

        // results
        assert_eq!(fs::read_to_string(r.join("March 2024/a (1).jpg")).unwrap(), "a");
        assert_eq!(fs::read_to_string(r.join("March 2024/a.jpg")).unwrap(), "existing");
        assert_eq!(fs::read_to_string(r.join("July 2023/B.JPG")).unwrap(), "b");
        assert_eq!(fs::read_to_string(r.join("January 2020/IMG_20200105_101010.png")).unwrap(), "img");
        assert!(r.join("notes.txt").exists());
        assert!(r.join("Trip/October 2026/c.heic").exists());
        assert!(r.join("Trip/October 2026/e.jpg").exists());
        assert!(r.join("Trip/sub/March 2024/d.webp").exists());

        // re-running is a no-op: everything is now already sorted
        let files2 = crate::scan::scan(r, true, &exts, &no, &mut |_, _| {}).files;
        let plan2 = build_plan(r, &files2, &opts, &default_stages(), &no, &mut |_, _| {});
        assert_eq!(plan2.counts().to_move, 0);
        assert_eq!(plan2.counts().already_sorted, 7);

        // undo restores the exact original tree
        assert_eq!(find_latest_log(r).as_deref(), Some(log_path.as_path()));
        let u = undo(&log_path, &mut |_, _| {});
        assert!(u.errors.is_empty(), "{:?}", u.errors);
        assert_eq!(u.restored, 5);
        assert_eq!(u.dirs_removed, 3); // July 2023, January 2020, Trip/sub/March 2024 (March 2024 + Trip/October 2026 pre-existed)
        assert_eq!(snapshot(r), before);
        assert!(find_latest_log(r).is_none());
        // modification times are preserved by rename
        let m = fs::metadata(r.join("a.jpg")).unwrap().modified().unwrap();
        assert_eq!(m, SystemTime::UNIX_EPOCH + Duration::from_secs(MAR_2024));
    }

    #[test]
    fn undo_keeps_nonempty_folders_and_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024, "a");
        touch(&r.join("b.jpg"), MAR_2024, "b");
        let opts = PlanOptions {
            date: DateConfig { mode: DateMode::Only(SourceKind::Modified), ..Default::default() },
            ..Default::default()
        };
        let no = AtomicBool::new(false);
        let files = crate::scan::scan(r, true, &crate::scan::parse_extensions(""), &no, &mut |_, _| {}).files;
        let plan = build_plan(r, &files, &opts, &default_stages(), &no, &mut |_, _| {});
        let rep = execute(&plan, &no, &mut |_, _, _| {});
        let log = rep.undo_log.unwrap();
        // user drops a new file into the created folder, and re-creates a.jpg
        fs::write(r.join("March 2024/user.jpg"), "user").unwrap();
        fs::write(r.join("a.jpg"), "new a").unwrap();
        let u = undo(&log, &mut |_, _| {});
        assert_eq!(u.restored, 1);
        assert_eq!(u.errors.len(), 1);
        assert_eq!(fs::read_to_string(r.join("a.jpg")).unwrap(), "new a");
        assert_eq!(fs::read_to_string(r.join("March 2024/a.jpg")).unwrap(), "a");
        assert!(r.join("March 2024/user.jpg").exists());
        // log kept with the remaining entry
        let left = read_log(&log).unwrap();
        assert_eq!(left.moves.len(), 1);
    }

    #[test]
    fn move_file_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        assert!(move_file(&a, &b).is_err());
        assert_eq!(fs::read_to_string(&b).unwrap(), "b");
        assert!(a.exists());
    }

    #[test]
    fn copy_fallback_works() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.jpg");
        let b = d.path().join("b.jpg");
        touch(&a, MAR_2024, "data");
        copy_then_delete(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read_to_string(&b).unwrap(), "data");
        assert_eq!(
            fs::metadata(&b).unwrap().modified().unwrap(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(MAR_2024)
        );
    }

    // ---------------------------------------------------------------- v2 --

    use crate::plan::{Plan, stages_with_rules};
    use crate::rename::*;

    fn plan_with(r: &Path, opts: &PlanOptions, rules: &[RuleSpec]) -> Plan {
        let exts = crate::scan::parse_extensions("");
        let no = AtomicBool::new(false);
        let files = crate::scan::scan(r, true, &exts, &no, &mut |_, _| {}).files;
        let stages = stages_with_rules(rules, &exts).unwrap();
        build_plan(r, &files, opts, &stages, &no, &mut |_, _| {})
    }

    fn mod_opts(sort: bool) -> PlanOptions {
        PlanOptions {
            date: DateConfig { mode: DateMode::Only(SourceKind::Modified), ..Default::default() },
            sort_into_folders: sort,
            ..Default::default()
        }
    }

    fn names(plan: &Plan, root: &Path) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = plan
            .items
            .iter()
            .map(|i| {
                let src = i.source.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                let dst = if i.status == Status::Move {
                    i.dest_path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")
                } else {
                    format!("<{}>", i.status.label())
                };
                (src, dst)
            })
            .collect();
        v.sort();
        v
    }

    fn constant_name(text: &str) -> RuleSpec {
        RuleSpec::new(RuleKind::AddText(AddTextRule { text: text.into(), position: Position::Replace }))
    }

    #[test]
    fn no_enabled_rules_is_identical_to_v1() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024, "a");
        touch(&r.join("B.JPG"), JUL_2023, "b");
        touch(&r.join("x/IMG_20200105_101010.png"), MAR_2024, "img");
        touch(&r.join("March 2024/a.jpg"), MAR_2024, "existing");
        touch(&r.join("July 2023/c.jpg"), MAR_2024, "misfiled");
        let opts = PlanOptions::default();
        let exts = crate::scan::parse_extensions("");
        let no = AtomicBool::new(false);
        let files = crate::scan::scan(r, true, &exts, &no, &mut |_, _| {}).files;
        let v1 = build_plan(r, &files, &opts, &default_stages(), &no, &mut |_, _| {});
        let mut disabled = constant_name("zzz");
        disabled.enabled = false;
        for rules in [vec![], vec![disabled]] {
            let v2 = plan_with(r, &opts, &rules);
            assert_eq!(names(&v1, r), names(&v2, r));
            for (a, b) in v1.items.iter().zip(&v2.items) {
                assert_eq!(a.new_name, b.new_name);
                assert_eq!(a.dest_path, b.dest_path);
                assert_eq!(a.status, b.status);
                assert_eq!(a.renamed_for_collision, b.renamed_for_collision);
                assert!(b.warnings.is_empty());
            }
            assert_eq!(v1.counts(), v2.counts());
        }
    }

    #[test]
    fn rules_then_month_folder_apply_and_undo() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("IMG_0001.JPG"), MAR_2024, "1");
        touch(&r.join("IMG_0002.JPG"), MAR_2024, "2");
        touch(&r.join("trip/IMG_0003.jpg"), JUL_2023, "3");
        touch(&r.join("trip/notes.txt"), JUL_2023, "notes");
        touch(&r.join("trip/IMG_0003.xmp"), JUL_2023, "sidecar");
        touch(&r.join("March 2024/holiday_001.jpg"), MAR_2024, "occupant");
        let before = snapshot(r);
        let rules = vec![
            RuleSpec::new(RuleKind::Replace(ReplaceRule {
                find: r"^IMG_\d+$".into(),
                with: "holiday".into(),
                regex: true,
                case_sensitive: false,
                occurrence: Occurrence::All,
            })),
            RuleSpec::new(RuleKind::new_default("number").unwrap()), // _001, per folder
            RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Lower)),
        ];
        let plan = plan_with(r, &mod_opts(true), &rules);
        assert_eq!(
            names(&plan, r),
            vec![
                ("IMG_0001.JPG".into(), "March 2024/holiday_001 (1).jpg".into()), // occupied on disk
                ("IMG_0002.JPG".into(), "March 2024/holiday_002.jpg".into()),
                ("March 2024/holiday_001.jpg".into(), "March 2024/holiday_001_001.jpg".into()), // renamed in place
                ("trip/IMG_0003.jpg".into(), "trip/July 2023/holiday_001.jpg".into()),
            ]
        );
        let c = plan.counts();
        assert_eq!((c.to_move, c.renamed, c.rename_only, c.collisions), (4, 4, 1, 1));
        let rep = execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(fs::read_to_string(r.join("March 2024/holiday_001 (1).jpg")).unwrap(), "1");
        assert_eq!(fs::read_to_string(r.join("March 2024/holiday_001_001.jpg")).unwrap(), "occupant");
        assert!(!r.join("March 2024/holiday_001.jpg").exists());
        assert_eq!(fs::read_to_string(r.join("trip/notes.txt")).unwrap(), "notes");
        assert_eq!(fs::read_to_string(r.join("trip/IMG_0003.xmp")).unwrap(), "sidecar");
        // the undo log records original names + locations
        let log = read_log(rep.undo_log.as_ref().unwrap()).unwrap();
        assert!(log.moves.iter().any(|m| m.from.ends_with("IMG_0001.JPG") && m.to.ends_with("holiday_001 (1).jpg")));
        let u = undo(rep.undo_log.as_ref().unwrap(), &mut |_, _| {});
        assert!(u.errors.is_empty(), "{:?}", u.errors);
        assert_eq!(snapshot(r), before);
    }

    #[test]
    fn same_batch_renames_never_collide() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for (i, n) in ["a.jpg", "b.JPG", "c.jpeg", "d.png", "sub/e.jpg"].iter().enumerate() {
            touch(&r.join(n), MAR_2024, &i.to_string());
        }
        let before = snapshot(r);
        // every file -> "photo" in place: a clash for every .jpg-ish name
        let rules = vec![constant_name("photo"), RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Lower))];
        let plan = plan_with(r, &mod_opts(false), &rules);
        assert_eq!(
            names(&plan, r),
            vec![
                ("a.jpg".into(), "photo.jpg".into()),
                ("b.JPG".into(), "photo (1).jpg".into()),
                ("c.jpeg".into(), "photo.jpeg".into()),
                ("d.png".into(), "photo.png".into()),
                ("sub/e.jpg".into(), "sub/photo.jpg".into()),
            ]
        );
        let rep = execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(rep.moved, 5);
        assert!(!r.join("July 2023").exists() && !r.join("March 2024").exists()); // no folders when sort is off
        assert_eq!(fs::read_to_string(r.join("photo (1).jpg")).unwrap(), "1");
        undo(rep.undo_log.as_ref().unwrap(), &mut |_, _| {});
        assert_eq!(snapshot(r), before);
    }

    #[test]
    fn swap_like_chains_are_safe() {
        // a -> b while b exists and b -> c: no overwrite, a gets a suffix
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024, "A");
        touch(&r.join("b.jpg"), MAR_2024, "B");
        let rules = vec![
            RuleSpec::new(RuleKind::Replace(ReplaceRule {
                find: "b".into(),
                with: "c".into(),
                regex: false,
                case_sensitive: true,
                occurrence: Occurrence::All,
            })),
            RuleSpec::new(RuleKind::Replace(ReplaceRule {
                find: "a".into(),
                with: "b".into(),
                regex: false,
                case_sensitive: true,
                occurrence: Occurrence::All,
            })),
        ];
        let plan = plan_with(r, &mod_opts(false), &rules);
        assert_eq!(names(&plan, r), vec![("a.jpg".into(), "b (1).jpg".into()), ("b.jpg".into(), "c.jpg".into())]);
        let rep = execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty());
        assert_eq!(fs::read_to_string(r.join("b (1).jpg")).unwrap(), "A");
        assert_eq!(fs::read_to_string(r.join("c.jpg")).unwrap(), "B");
    }

    #[test]
    fn case_only_rename_and_undo() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("Photo.JPG"), MAR_2024, "p");
        touch(&r.join("other.jpg"), MAR_2024, "o");
        let rules = vec![RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Lower))];
        let plan = plan_with(r, &mod_opts(false), &rules);
        assert_eq!(
            names(&plan, r),
            vec![("Photo.JPG".into(), "Photo.jpg".into()), ("other.jpg".into(), "<Unchanged>".into())]
        );
        assert!(!plan.items[0].renamed_for_collision, "case-only rename must not get a (1) suffix");
        let rep = execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        let listing = |r: &Path| {
            let mut v: Vec<String> = fs::read_dir(r)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with(UNDO_PREFIX))
                .collect();
            v.sort();
            v
        };
        assert_eq!(listing(r), vec!["Photo.jpg", "other.jpg"]);
        let u = undo(rep.undo_log.as_ref().unwrap(), &mut |_, _| {});
        assert!(u.errors.is_empty(), "{:?}", u.errors);
        assert_eq!(listing(r), vec!["Photo.JPG", "other.jpg"]);
        assert_eq!(fs::read_to_string(r.join("Photo.JPG")).unwrap(), "p");
    }

    #[test]
    fn rename_in_place_when_already_sorted_and_unchanged_otherwise() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("March 2024/x.jpg"), MAR_2024, "x");
        touch(&r.join("March 2024/keep.JPG"), MAR_2024, "k");
        let rules = vec![RuleSpec::new(RuleKind::Case(CaseMode::Upper))];
        let plan = plan_with(r, &mod_opts(true), &rules);
        assert_eq!(
            names(&plan, r),
            vec![
                ("March 2024/keep.JPG".into(), "March 2024/KEEP.JPG".into()),
                ("March 2024/x.jpg".into(), "March 2024/X.jpg".into()),
            ]
        );
        assert!(plan.items.iter().all(|i| i.is_rename_only()));
        // same chain once more after applying: nothing left to do
        execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        let plan2 = plan_with(r, &mod_opts(true), &rules);
        assert_eq!(plan2.counts().already_sorted, 2);
        assert_eq!(plan2.counts().to_move, 0);
    }

    #[test]
    fn numbering_restarts_per_folder_and_reserved_names_fixed() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for n in ["a/1.jpg", "a/2.jpg", "b/1.jpg", "b/2.jpg", "b/3.jpg"] {
            touch(&r.join(n), MAR_2024, n);
        }
        let mut num = RuleKind::new_default("number").unwrap();
        if let RuleKind::Numbering(n) = &mut num {
            n.position = Position::Replace;
            n.padding = 2;
        }
        let plan = plan_with(r, &mod_opts(false), &[RuleSpec::new(num)]);
        let got: Vec<String> = names(&plan, r).into_iter().map(|(_, d)| d).collect();
        assert_eq!(got, vec!["a/01.jpg", "a/02.jpg", "b/01.jpg", "b/02.jpg", "b/03.jpg"]);

        let plan = plan_with(r, &mod_opts(false), &[constant_name("CON")]);
        let it = &plan.items[0];
        assert_eq!(it.new_name, "CON_.jpg");
        assert!(!it.warnings.is_empty());
        assert_eq!(plan.counts().warnings, 5);
    }

    #[test]
    fn non_media_files_are_never_renamed_by_the_stage() {
        // even if a caller passes a non-media file to the planner directly
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("notes.txt"), MAR_2024, "t");
        let exts = crate::scan::parse_extensions("");
        let stages = stages_with_rules(&[constant_name("x")], &exts).unwrap();
        let plan =
            build_plan(r, &[r.join("notes.txt")], &mod_opts(false), &stages, &AtomicBool::new(false), &mut |_, _| {});
        assert_eq!(plan.items[0].status, Status::Unchanged);
        assert_eq!(plan.items[0].new_name, "notes.txt");
        assert!(!plan.items[0].warnings.is_empty());
    }

    #[test]
    fn invalid_rules_block_planning() {
        let bad = RuleSpec::new(RuleKind::Replace(ReplaceRule {
            find: "[".into(),
            with: String::new(),
            regex: true,
            case_sensitive: false,
            occurrence: Occurrence::All,
        }));
        assert!(stages_with_rules(&[bad], &crate::scan::parse_extensions("")).is_err());
    }

    #[test]
    fn long_paths_are_flagged() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("a.jpg"), MAR_2024, "a");
        let long = "x".repeat(250);
        let plan = plan_with(r, &mod_opts(true), &[constant_name(&long)]);
        let it = &plan.items[0];
        assert_eq!(it.status, Status::Move);
        assert!(it.warnings.iter().any(|w| w.contains("longer than 260")), "{:?}", it.warnings);
    }
}
