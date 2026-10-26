//! Headless end-to-end driver used for testing (not shipped).
//!
//! ```text
//! e2e <folder> <PhotoSorter.ini> [--preset NAME] [--apply] [--undo]
//! ```
//! Loads the settings (and optionally a `[Preset:NAME]`), prints the preview
//! exactly as the GUI would build it, then optionally applies it or undoes the
//! latest run in `<folder>`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use photo_sorter::plan::{PlanOptions, Status, build_plan, stages_with_rules};
use photo_sorter::{ops, scan, settings};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: e2e <folder> <ini> [--preset NAME] [--apply] [--undo]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[1]);
    let text = std::fs::read_to_string(&args[2]).expect("read ini");
    let mut s = settings::Settings::from_ini(&text);
    let flag = |f: &str| args.iter().any(|a| a == f);
    if let Some(i) = args.iter().position(|a| a == "--preset") {
        let name = &args[i + 1];
        let p = s.presets.iter().find(|p| &p.name == name).cloned().expect("preset not found");
        s.apply_preset(&p);
        println!("Preset: {name}");
    }
    if flag("--undo") {
        let log = ops::find_latest_log(&root).expect("no undo log");
        println!("Undo log: {}", log.display());
        let rep = ops::undo(&log, &mut |_, _| {});
        println!("Undo: restored={} dirs_removed={} errors={:?}", rep.restored, rep.dirs_removed, rep.errors);
        std::process::exit(if rep.errors.is_empty() { 0 } else { 1 });
    }
    println!("Rules ({}):", s.rules.len());
    for (i, r) in s.rules.iter().enumerate() {
        println!("  {}. [{}] {} - {}", i + 1, if r.enabled { "x" } else { " " }, r.kind.type_label(), r.kind.summary());
    }
    let exts = scan::parse_extensions(&s.extensions);
    let no = AtomicBool::new(false);
    let sc = scan::scan(&root, s.include_subfolders, &exts, &no, &mut |_, _| {});
    let stages = match stages_with_rules(&s.rules, &exts) {
        Ok(st) => st,
        Err(e) => {
            println!("RULE ERROR: {e}");
            std::process::exit(1);
        }
    };
    let opts = PlanOptions {
        template: s.folder_template.clone(),
        date: s.date.clone(),
        sort_into_folders: s.sort_into_folders,
    };
    let plan = build_plan(&root, &sc.files, &opts, &stages, &no, &mut |_, _| {});
    let rel =
        |p: &std::path::Path| p.strip_prefix(&root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    println!("\n{:<45} | {:<48} | {:<22} | {:<10} | notes", "BEFORE", "AFTER", "DEST FOLDER", "STATUS");
    for it in &plan.items {
        let dest = match it.status {
            Status::Move => {
                let d = rel(&it.dest_dir);
                if d.is_empty() { ".".into() } else { d }
            }
            _ => "-".into(),
        };
        let after = if it.status == Status::Move { it.final_name() } else { "-".into() };
        println!(
            "{:<45} | {:<48} | {:<22} | {:<10} | {}{}",
            rel(&it.source),
            after,
            dest,
            it.status.label(),
            if it.renamed_for_collision { "[collision suffix] " } else { "" },
            it.warnings.join("; ")
        );
    }
    let c = plan.counts();
    println!(
        "\nCounts: total={} to_move={} renamed={} rename_only={} collisions={} already_sorted={} unchanged={} no_date={} errors={} warnings={} new_folders={}",
        c.total,
        c.to_move,
        c.renamed,
        c.rename_only,
        c.collisions,
        c.already_sorted,
        c.unchanged,
        c.no_date,
        c.errors,
        c.warnings,
        c.new_folders
    );
    if flag("--apply") {
        let rep = ops::execute(&plan, &no, &mut |_, _, _| {});
        println!(
            "Apply: moved={} skipped={} errors={:?} undo_log={:?}",
            rep.moved, rep.skipped, rep.errors, rep.undo_log
        );
        std::process::exit(if rep.errors.is_empty() { 0 } else { 1 });
    }
}
