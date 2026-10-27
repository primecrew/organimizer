//! egui front-end. All real work happens in the `photo_sorter` library on a
//! background thread; this module only draws state and forwards actions.
//!
//! Layout: one window, top to bottom in the order the pipeline runs:
//! 1. Source & filter  2. Rename rules  3. Output folder (Month YYYY)
//! 4. Live preview table  5. Apply / Undo bar.
//!
//! Rarely-changed settings (file types, Auto date priority) live in the
//! Tools > Options window; the main window only shows a one-line summary.
//!
//! Menu bar: File (New Project, Open Folder, Undo Last Apply, Exit),
//! Presets (Load, Save current, Manage Presets), Tools (Options, open the
//! settings / undo-log folder), Help (README, About).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, RichText,
    Stroke, TextStyle, Vec2,
};
use egui_extras::{Column, TableBuilder};

use photo_sorter::date_source::{DateMode, Fallback, SourceKind};
use photo_sorter::filename_date::DateOrder;
use photo_sorter::ops::{self, RunReport, UndoReport};
use photo_sorter::plan::{self, Plan, PlanOptions, Status};
use photo_sorter::rename::{
    self, AddTextRule, CaseMode, CleanRule, DateRule, DeleteFrom, DeleteRule, DeleteUntil, ExtCase, ExtensionRule,
    NumberSystem, NumberingRule, Occurrence, PaddingRule, ParentRule, Position, RULE_TYPES, RandomizeRule,
    RearrangeRule, ReformatDateRule, RemoveMode, ReplaceRule, RuleKind, RuleSpec, StripRule,
};
use photo_sorter::scan;
use photo_sorter::settings::{self, Loaded, PresetScope, Settings, Theme};
use photo_sorter::template::{self as tpl, FolderBuilder, MonthStyle, PartOrder, Separator, YearStyle};
use photo_sorter::window_state::{self, WindowLayout};
use photo_sorter::{desktop, session};

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ScanCache {
    sig: String,
    files: Arc<Vec<PathBuf>>,
    errors: Vec<(PathBuf, String)>,
    dirs: usize,
}

enum Msg {
    Progress { text: String, frac: f32 },
    Planned { plan: Plan, scan: ScanCache, cancelled: bool },
    Applied(RunReport),
    Undone(UndoReport),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Preview,
    Apply,
    Undo,
}

struct Worker {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    kind: JobKind,
}

fn spawn<F>(ctx: &egui::Context, kind: JobKind, f: F) -> Worker
where
    F: FnOnce(Sender<Msg>, Arc<AtomicBool>, egui::Context) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let ctx2 = ctx.clone();
    std::thread::spawn(move || {
        f(tx, c2, ctx2.clone());
        ctx2.request_repaint();
    });
    Worker { rx, cancel, kind }
}

/// Throttled progress sender.
struct Ticker {
    last: Instant,
}
impl Ticker {
    fn new() -> Self {
        Ticker { last: Instant::now() - Duration::from_secs(1) }
    }
    fn due(&mut self) -> bool {
        if self.last.elapsed() >= Duration::from_millis(50) {
            self.last = Instant::now();
            true
        } else {
            false
        }
    }
}

/// Wait this long after the last edit before re-running the live preview.
const DEBOUNCE: Duration = Duration::from_millis(350);

// ---------------------------------------------------------------------------
// Table state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortCol {
    File,
    NewName,
    Current,
    Dest,
    Date,
    Status,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusFilter {
    All,
    Changes,
    Renamed,
    Move,
    Collision,
    Warnings,
    Unchanged,
    NoDate,
    Error,
}

impl StatusFilter {
    const ALL: [StatusFilter; 9] = [
        StatusFilter::All,
        StatusFilter::Changes,
        StatusFilter::Renamed,
        StatusFilter::Move,
        StatusFilter::Collision,
        StatusFilter::Warnings,
        StatusFilter::Unchanged,
        StatusFilter::NoDate,
        StatusFilter::Error,
    ];
    fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All files",
            StatusFilter::Changes => "Will change",
            StatusFilter::Renamed => "Renamed",
            StatusFilter::Move => "Moved to another folder",
            StatusFilter::Collision => "Name clash (suffix added)",
            StatusFilter::Warnings => "With notes / warnings",
            StatusFilter::Unchanged => "Unchanged / already sorted",
            StatusFilter::NoDate => "No date",
            StatusFilter::Error => "Errors",
        }
    }
    fn matches(self, it: &plan::PlanItem) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Changes => it.status == Status::Move,
            StatusFilter::Renamed => it.is_renamed(),
            StatusFilter::Move => it.status == Status::Move && !it.is_rename_only(),
            StatusFilter::Collision => it.status == Status::Move && it.renamed_for_collision,
            StatusFilter::Warnings => !it.warnings.is_empty(),
            StatusFilter::Unchanged => matches!(it.status, Status::AlreadySorted | Status::Unchanged),
            StatusFilter::NoDate => it.status == Status::NoDate,
            StatusFilter::Error => matches!(it.status, Status::Error(_)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Files,
    Folders,
    Results,
}

enum Confirm {
    Apply,
    Undo { log: PathBuf, moves: usize, started: String },
}

/// Unsaved edits in the Tools > Options window. Applied to the settings (and
/// written to PhotoSorter.ini) only on Save; Close discards them.
#[derive(Clone, PartialEq)]
struct OptionsDraft {
    extensions: String,
    order_ui: Vec<(SourceKind, bool)>,
}

/// Small modal dialogs of the menu bar.
enum Dialog {
    NewProject(Vec<String>),
    SavePreset { name: String },
    DeletePreset(String),
    ReadmeMissing(PathBuf),
    About,
    Message { title: String, text: String },
}

enum RuleAction {
    Up,
    Down,
    Duplicate,
    Delete,
    ToggleOpen,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    s: Settings,
    ini_path: Option<PathBuf>,
    notice: Option<String>,
    /// Auto-order editor: every source with an enabled flag, in order.
    order_ui: Vec<(SourceKind, bool)>,

    plan: Option<Plan>,
    /// Cached per plan (counting touches the disk, so not done every frame).
    counts: plan::PlanCounts,
    folder_rows: Vec<(PathBuf, usize, bool)>,
    source_summary: String,
    /// Signature of the options the current plan was built with.
    plan_sig: String,
    plan_cancelled: bool,
    scan_cache: Option<ScanCache>,
    scan_errors: Vec<(PathBuf, String)>,
    dirs_scanned: usize,

    /// Live preview bookkeeping.
    seen_sig: String,
    changed_at: Option<Instant>,
    auto_paused: bool,
    rule_errors: Vec<(usize, String)>,

    worker: Option<Worker>,
    progress_text: String,
    progress: f32,
    status: String,

    run_report: Option<RunReport>,
    undo_report: Option<UndoReport>,

    tab: Tab,
    filter_text: String,
    filter_status: StatusFilter,
    sort: (SortCol, bool),
    view: Vec<usize>,
    view_dirty: bool,

    /// Step 3 "Build your own" chosen (otherwise a ready-made layout).
    folder_build: bool,
    /// Last valid builder state (kept while the template is "Custom").
    builder: FolderBuilder,
    /// Text of the custom separator field.
    custom_sep: String,
    /// "Custom…" separator chosen (kept even if the text equals e.g. "-").
    sep_custom: bool,
    /// Template the builder controls were last synced with.
    builder_synced: String,
    /// Bumped by "Reset window layout" so panels/tables forget their sizes.
    layout_gen: u32,
    /// Window/layout persistence: snapshot last written, pending snapshot
    /// and since when it is pending (debounced save, no per-frame writes).
    saved_snapshot: String,
    pending_snapshot: (String, Option<Instant>),
    started: Instant,
    open_rule: Option<usize>,
    preset_sel: String,
    preset_name: String,

    confirm: Option<Confirm>,
    options: Option<OptionsDraft>,
    dialog: Option<Dialog>,
    /// Presets > Manage Presets window.
    manage_open: bool,
    manage_sel: String,
    manage_rename: String,
    manage_error: Option<String>,
    /// Preset loaded via "Edit" in Manage Presets; "Save changes" writes back to it.
    editing_preset: Option<String>,
    undo_cache: (String, Option<PathBuf>),
    applied_theme: Option<Theme>,
    applied_font: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, loaded: Loaded) -> Self {
        install_fonts(&cc.egui_ctx);
        let s = loaded.settings;
        let mut order_ui: Vec<(SourceKind, bool)> = s.date.auto_order.iter().map(|k| (*k, true)).collect();
        for k in SourceKind::ALL {
            if !s.date.auto_order.contains(&k) {
                order_ui.push((k, false));
            }
        }
        let mut app = App {
            order_ui,
            ini_path: loaded.path,
            notice: loaded.notice,
            plan: None,
            counts: Default::default(),
            folder_rows: vec![],
            source_summary: String::new(),
            plan_sig: String::new(),
            plan_cancelled: false,
            scan_cache: None,
            scan_errors: vec![],
            dirs_scanned: 0,
            seen_sig: String::new(),
            changed_at: None,
            auto_paused: false,
            rule_errors: vec![],
            worker: None,
            progress_text: String::new(),
            progress: 0.0,
            status: if s.last_folder.trim().is_empty() {
                "Choose a folder — the preview updates live; nothing changes until you click Apply.".into()
            } else {
                "Ready.".into()
            },
            run_report: None,
            undo_report: None,
            tab: Tab::Files,
            filter_text: String::new(),
            filter_status: StatusFilter::All,
            sort: (SortCol::Current, true),
            view: vec![],
            view_dirty: true,
            folder_build: false,
            builder: FolderBuilder::from_template(&s.folder_template).unwrap_or_default(),
            custom_sep: "+".into(),
            sep_custom: false,
            builder_synced: String::from("\0"),
            layout_gen: 0,
            saved_snapshot: String::new(),
            pending_snapshot: (String::new(), None),
            started: Instant::now(),
            open_rule: None,
            preset_sel: String::new(),
            preset_name: String::new(),
            confirm: None,
            options: None,
            dialog: None,
            manage_open: false,
            manage_sel: String::new(),
            manage_rename: String::new(),
            manage_error: None,
            editing_preset: None,
            undo_cache: (String::from("\0"), None),
            applied_theme: None,
            applied_font: 0.0,
            s,
        };
        app.saved_snapshot = app.layout_snapshot();
        app
    }

    fn root(&self) -> PathBuf {
        PathBuf::from(self.s.folder_trimmed())
    }

    fn plan_options(&self) -> PlanOptions {
        PlanOptions {
            template: self.s.folder_template.clone(),
            date: self.s.date.clone(),
            sort_into_folders: self.s.sort_into_folders,
        }
    }

    fn scan_signature(&self) -> String {
        format!("{:?}|{}|{}", self.s.folder_trimmed(), self.s.include_subfolders, self.s.extensions)
    }

    /// Everything that influences the plan. If it changes, the preview is stale.
    fn signature(&self) -> String {
        format!(
            "{}|{:?}|{}|{}|{:?}",
            self.scan_signature(),
            self.s.date,
            self.s.folder_template,
            self.s.sort_into_folders,
            self.s.rules
        )
    }

    fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn busy_kind(&self) -> Option<JobKind> {
        self.worker.as_ref().map(|w| w.kind)
    }

    fn plan_is_current(&self) -> bool {
        self.plan.is_some() && !self.plan_cancelled && self.plan_sig == self.signature()
    }

    /// Everything window-related that is saved automatically.
    fn layout_snapshot(&self) -> String {
        format!(
            "{:?}|{}|{}|{}|{:?}",
            self.s.theme,
            self.s.font_size,
            self.s.window_width.round(),
            self.s.window_height.round(),
            self.s.layout
        )
    }

    /// Save window / layout / theme changes at most once they have been
    /// stable for 1.5 s (and no mouse button is held), never per frame.
    fn autosave_layout(&mut self, ctx: &egui::Context) {
        let snap = self.layout_snapshot();
        if snap == self.saved_snapshot {
            self.pending_snapshot = (String::new(), None);
            return;
        }
        if self.pending_snapshot.0 != snap {
            self.pending_snapshot = (snap, Some(Instant::now()));
        }
        let since = self.pending_snapshot.1.map(|t| t.elapsed()).unwrap_or_default();
        let held = ctx.input(|i| i.pointer.any_down());
        if since >= Duration::from_millis(1500) && !held {
            self.save_settings();
        } else {
            ctx.request_repaint_after(Duration::from_millis(1600).saturating_sub(since));
        }
    }

    /// Track the native window (position, size, maximized). Ignored for the
    /// first second, while the OS is still placing / maximizing the window.
    fn track_window(&mut self, ctx: &egui::Context) {
        if self.started.elapsed() < Duration::from_secs(1) {
            ctx.request_repaint_after(Duration::from_millis(1100));
            return;
        }
        let (outer, inner, max, min) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.maximized.unwrap_or(false), v.minimized.unwrap_or(false))
        });
        if let Some((pos, size)) =
            window_state::remember(outer.map(|r| [r.min.x, r.min.y]), inner.map(|r| [r.width(), r.height()]), max, min)
        {
            self.s.layout.maximized = max;
            if let Some([x, y]) = pos {
                (self.s.layout.x, self.s.layout.y) = (Some(x), Some(y));
            }
            if let Some([w, h]) = size {
                (self.s.window_width, self.s.window_height) = (w, h);
            }
        }
    }

    /// Tools > Options > Reset window layout: default size, centered, not
    /// maximized, default panel height / column widths / open sections.
    fn reset_layout(&mut self, ctx: &egui::Context) {
        self.s.layout = WindowLayout::default();
        let [w, h] = window_state::DEFAULT_SIZE;
        let p = window_state::restore_placement(None, [w, h], false, &photo_sorter::desktop::monitor_work_areas());
        (self.s.window_width, self.s.window_height) = (p.size[0], p.size[1]);
        self.layout_gen += 1;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(p.size[0], p.size[1])));
        if let Some(m) = photo_sorter::desktop::monitor_work_areas().first() {
            let x = m.x + ((m.w - p.size[0] - 16.0) / 2.0).max(0.0);
            let y = m.y + ((m.h - p.size[1] - 60.0) / 2.0).max(0.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }
        // ignore the old rect while the window moves
        self.started = Instant::now();
        self.status = "Window layout reset to the defaults.".into();
    }

    fn save_settings(&mut self) {
        self.saved_snapshot = self.layout_snapshot();
        if let Some(p) = &self.ini_path
            && let Err(e) = settings::save(p, &self.s)
        {
            self.notice = Some(format!("Could not save {}: {e}", settings::INI_NAME));
            self.ini_path = None;
        }
    }

    fn sync_order(&mut self) {
        let order: Vec<SourceKind> = self.order_ui.iter().filter(|(_, on)| *on).map(|(k, _)| *k).collect();
        self.s.date.auto_order = order;
    }

    fn rebuild_order_ui(&mut self) {
        let mut order_ui: Vec<(SourceKind, bool)> = self.s.date.auto_order.iter().map(|k| (*k, true)).collect();
        for k in SourceKind::ALL {
            if !self.s.date.auto_order.contains(&k) {
                order_ui.push((k, false));
            }
        }
        self.order_ui = order_ui;
    }

    fn set_folder(&mut self, p: &Path) {
        let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_default() };
        self.s.last_folder = dir.to_string_lossy().into_owned();
        self.status = format!("Selected {}", dir.display());
    }

    fn undo_log_for_root(&mut self) -> Option<PathBuf> {
        let key = self.s.folder_trimmed().to_string();
        if self.undo_cache.0 != key {
            let root = self.root();
            let found = if key.is_empty() || !root.is_dir() { None } else { ops::find_latest_log(&root) };
            self.undo_cache = (key, found);
        }
        self.undo_cache.1.clone()
    }

    fn invalidate_undo_cache(&mut self) {
        self.undo_cache.0 = "\0".into();
    }

    /// Force a fresh scan + preview (disk may have changed).
    fn refresh(&mut self) {
        self.scan_cache = None;
        self.plan_sig.clear();
        self.auto_paused = false;
        self.changed_at = Some(Instant::now() - DEBOUNCE);
    }

    // ----- live preview driver -----

    fn drive_live_preview(&mut self, ctx: &egui::Context) {
        let sig = self.signature();
        if sig != self.seen_sig {
            self.seen_sig = sig.clone();
            self.changed_at = Some(Instant::now());
            self.auto_paused = false;
            self.rule_errors = rename::validate_chain(&self.s.rules);
        }
        if matches!(self.busy_kind(), Some(JobKind::Apply | JobKind::Undo)) || self.auto_paused {
            return;
        }
        if self.plan_sig == sig {
            return;
        }
        let root = self.root();
        if self.s.folder_trimmed().is_empty() || !root.is_dir() || !self.rule_errors.is_empty() {
            return;
        }
        let Some(t) = self.changed_at else { return };
        let waited = t.elapsed();
        if waited >= DEBOUNCE {
            self.changed_at = None;
            self.start_preview(ctx);
        } else {
            ctx.request_repaint_after(DEBOUNCE - waited);
        }
    }

    // ----- actions -----

    fn start_preview(&mut self, ctx: &egui::Context) {
        if let Some(w) = self.worker.take() {
            if w.kind != JobKind::Preview {
                self.worker = Some(w);
                return;
            }
            w.cancel.store(true, Ordering::Relaxed); // superseded
        }
        let root = self.root();
        if !root.is_dir() {
            self.status = "Please choose an existing folder first.".into();
            return;
        }
        let exts = scan::parse_extensions(&self.s.extensions);
        let stages = match plan::stages_with_rules(&self.s.rules, &exts) {
            Ok(s) => s,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let recursive = self.s.include_subfolders;
        let opts = self.plan_options();
        let scan_sig = self.scan_signature();
        let cached = self.scan_cache.clone().filter(|c| c.sig == scan_sig);
        self.plan_sig = self.signature();
        self.progress = 0.0;
        self.status = "Building preview…".into();
        self.worker = Some(spawn(ctx, JobKind::Preview, move |tx, cancel, ctx| {
            let sc = match cached {
                Some(c) => c,
                None => {
                    let mut t = Ticker::new();
                    let res = scan::scan(&root, recursive, &exts, &cancel, &mut |n, dir| {
                        if t.due() {
                            let _ = tx.send(Msg::Progress {
                                text: format!("Scanning… {n} files found — {}", dir.display()),
                                frac: 0.0,
                            });
                            ctx.request_repaint();
                        }
                    });
                    ScanCache {
                        sig: if cancel.load(Ordering::Relaxed) { String::new() } else { scan_sig },
                        files: Arc::new(res.files),
                        errors: res.errors,
                        dirs: res.dirs_scanned,
                    }
                }
            };
            let mut t = Ticker::new();
            let plan = plan::build_plan(&root, &sc.files, &opts, &stages, &cancel, &mut |i, n| {
                if t.due() {
                    let _ = tx.send(Msg::Progress {
                        text: format!("Reading dates and applying rules… {i} / {n}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Planned { plan, scan: sc, cancelled: cancel.load(Ordering::Relaxed) });
        }));
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.plan.clone() else { return };
        self.save_settings();
        self.status = "Renaming / moving files…".into();
        self.progress = 0.0;
        self.worker = Some(spawn(ctx, JobKind::Apply, move |tx, cancel, ctx| {
            let mut t = Ticker::new();
            let rep = ops::execute(&plan, &cancel, &mut |i, n, p| {
                if t.due() {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let _ = tx.send(Msg::Progress {
                        text: format!("Processing {i} / {n}  {name}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Applied(rep));
        }));
    }

    fn start_undo(&mut self, ctx: &egui::Context, log: PathBuf) {
        if let Some(w) = self.worker.take() {
            w.cancel.store(true, Ordering::Relaxed);
        }
        self.status = "Undoing last run…".into();
        self.progress = 0.0;
        self.worker = Some(spawn(ctx, JobKind::Undo, move |tx, _cancel, ctx| {
            let mut t = Ticker::new();
            let rep = ops::undo(&log, &mut |i, n| {
                if t.due() {
                    let _ = tx.send(Msg::Progress {
                        text: format!("Restoring {i} / {n}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Undone(rep));
        }));
    }

    /// After apply / undo: the disk changed, so forget the scan; do not
    /// re-preview automatically (keep the results visible).
    fn after_disk_change(&mut self) {
        self.scan_cache = None;
        self.plan = None;
        self.view.clear();
        self.tab = Tab::Results;
        self.plan_sig = self.signature();
        self.invalidate_undo_cache();
    }

    fn poll_worker(&mut self) {
        let Some(w) = &self.worker else { return };
        let msgs: Vec<Msg> = w.rx.try_iter().collect();
        let mut finished = false;
        for msg in msgs {
            match msg {
                Msg::Progress { text, frac } => {
                    self.progress_text = text;
                    self.progress = frac;
                }
                Msg::Planned { plan, scan, cancelled } => {
                    let c = plan.counts();
                    self.status = if cancelled {
                        "Preview cancelled (partial results shown; Apply is disabled). Click “Refresh preview” to run it again.".into()
                    } else {
                        let moved = c.to_move - c.rename_only;
                        format!(
                            "Preview: {} files in {} folders — {} renamed, {} moved to Month folders, {} unchanged/already sorted, {} without date. Nothing has been changed yet.",
                            c.total,
                            scan.dirs,
                            c.renamed,
                            moved,
                            c.already_sorted + c.unchanged,
                            c.no_date
                        )
                    };
                    self.plan_cancelled = cancelled;
                    if cancelled {
                        self.auto_paused = true;
                    }
                    self.counts = c;
                    let mut map: BTreeMap<PathBuf, (usize, bool)> = BTreeMap::new();
                    let mut by_src: BTreeMap<SourceKind, usize> = BTreeMap::new();
                    for it in &plan.items {
                        if let Some(d) = it.date {
                            *by_src.entry(d.source).or_default() += 1;
                        }
                        if it.status == Status::Move {
                            let e = map.entry(it.dest_dir.clone()).or_insert_with(|| (0, !it.dest_dir.exists()));
                            e.0 += 1;
                        }
                    }
                    self.folder_rows = map.into_iter().map(|(p, (n, new))| (p, n, new)).collect();
                    let txt: Vec<String> = by_src.iter().map(|(k, n)| format!("{}: {n}", k.label())).collect();
                    self.source_summary = if txt.is_empty() { "none".into() } else { txt.join("  ·  ") };
                    self.plan = Some(plan);
                    self.scan_errors = scan.errors.clone();
                    self.dirs_scanned = scan.dirs;
                    if !scan.sig.is_empty() {
                        self.scan_cache = Some(scan);
                    }
                    self.view_dirty = true;
                    if self.tab == Tab::Results {
                        self.tab = Tab::Files;
                    }
                    finished = true;
                }
                Msg::Applied(rep) => {
                    self.status = format!(
                        "Done: {} files renamed/moved, {} errors{}. {}",
                        rep.moved,
                        rep.errors.len(),
                        if rep.cancelled { " (cancelled)" } else { "" },
                        rep.undo_log.as_ref().map(|p| format!("Undo log: {}", p.display())).unwrap_or_default()
                    );
                    self.run_report = Some(rep);
                    self.undo_report = None;
                    self.after_disk_change();
                    finished = true;
                }
                Msg::Undone(rep) => {
                    self.status = format!(
                        "Undo: {} files restored to their original names and folders, {} empty folders removed, {} problems.",
                        rep.restored,
                        rep.dirs_removed,
                        rep.errors.len()
                    );
                    self.undo_report = Some(rep);
                    self.after_disk_change();
                    finished = true;
                }
            }
        }
        if finished {
            self.worker = None;
            self.progress = 0.0;
            self.progress_text.clear();
        }
    }

    fn rebuild_view(&mut self) {
        self.view_dirty = false;
        let Some(plan) = &self.plan else {
            self.view.clear();
            return;
        };
        let needle = self.filter_text.to_lowercase();
        let mut v: Vec<usize> = plan
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| self.filter_status.matches(it))
            .filter(|(_, it)| {
                needle.is_empty()
                    || it.source.to_string_lossy().to_lowercase().contains(&needle)
                    || it.dest_path.to_string_lossy().to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        let items = &plan.items;
        let (col, asc) = self.sort;
        let name =
            |i: usize| items[i].source.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        v.sort_by(|&a, &b| {
            let (x, y) = (&items[a], &items[b]);
            let o = match col {
                SortCol::File => name(a).cmp(&name(b)),
                SortCol::NewName => x.final_name().to_lowercase().cmp(&y.final_name().to_lowercase()),
                SortCol::Current => x.current_dir.cmp(&y.current_dir).then_with(|| name(a).cmp(&name(b))),
                SortCol::Dest => x.dest_path.cmp(&y.dest_path),
                SortCol::Date => x.date.map(|d| (d.date, d.datetime)).cmp(&y.date.map(|d| (d.date, d.datetime))),
                SortCol::Status => status_text(x).cmp(&status_text(y)),
            };
            if asc { o } else { o.reverse() }
        });
        self.view = v;
    }
    fn apply_style(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.s.theme) && self.applied_font == self.s.font_size {
            return;
        }
        self.applied_theme = Some(self.s.theme);
        self.applied_font = self.s.font_size;
        ctx.set_theme(match self.s.theme {
            Theme::Dark => egui::ThemePreference::Dark,
            Theme::Light => egui::ThemePreference::Light,
            Theme::System => egui::ThemePreference::System,
        });
        let fs = self.s.font_size;
        ctx.all_styles_mut(|style| {
            style.text_styles = [
                (TextStyle::Heading, FontId::new(fs * 1.45, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(fs, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(fs, FontFamily::Proportional)),
                (TextStyle::Small, FontId::new(fs * 0.82, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(fs * 0.95, FontFamily::Monospace)),
            ]
            .into();
            style.spacing.item_spacing = Vec2::new(8.0, 7.0);
            style.spacing.button_padding = Vec2::new(12.0, 5.0);
            style.spacing.interact_size.y = fs + 12.0;
            let r = CornerRadius::same(6);
            let v = &mut style.visuals;
            v.widgets.noninteractive.corner_radius = r;
            v.widgets.inactive.corner_radius = r;
            v.widgets.hovered.corner_radius = r;
            v.widgets.active.corner_radius = r;
            v.widgets.open.corner_radius = r;
            v.window_corner_radius = CornerRadius::same(10);
        });
    }
}

trait FolderTrim {
    fn folder_trimmed(&self) -> &str;
}
impl FolderTrim for Settings {
    fn folder_trimmed(&self) -> &str {
        self.last_folder.trim().trim_matches('"')
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

fn accent(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(90, 160, 255) } else { Color32::from_rgb(30, 100, 200) }
}

fn chip(ui: &mut egui::Ui, label: &str, value: usize, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(format!("{label}  {value}")).color(ui.visuals().strong_text_color()))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

fn rel(root: &Path, p: &Path) -> String {
    match p.strip_prefix(root) {
        Ok(r) if r.as_os_str().is_empty() => ".".into(),
        Ok(r) => format!(".{}{}", std::path::MAIN_SEPARATOR, r.display()),
        Err(_) => p.display().to_string(),
    }
}

fn green() -> Color32 {
    Color32::from_rgb(60, 170, 100)
}
fn amber() -> Color32 {
    Color32::from_rgb(220, 150, 40)
}
fn red() -> Color32 {
    Color32::from_rgb(220, 80, 80)
}
fn grey() -> Color32 {
    Color32::from_rgb(140, 140, 150)
}

fn status_color(s: &Status, renamed: bool) -> Color32 {
    match s {
        Status::Move if renamed => amber(),
        Status::Move => green(),
        Status::AlreadySorted => grey(),
        Status::NoDate => amber(),
        Status::Error(_) => red(),
        Status::Unchanged => grey(),
    }
}

/// Use Segoe UI on Windows when available (keeps egui fonts as fallback for
/// symbols). Reading the system font file is read-only and optional.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fonts_dir =
        std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts");
    let read =
        |name: &str| std::fs::read(fonts_dir.join(name)).ok().filter(|b| b.len() > 10_000 && b.len() < 40_000_000);
    if let Some(bytes) = read("segoeui.ttf") {
        fonts.font_data.insert("ui".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    // symbol fallback, after egui's own fonts
    if let Some(bytes) = read("seguisym.ttf") {
        fonts.font_data.insert("sym".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().push("sym".into());
    }
    if let Some(bytes) = read("consola.ttf") {
        fonts.font_data.insert("mono".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}

/// Window icon (same artwork as the embedded .ico), 64x64 raw RGBA.
pub fn app_icon() -> egui::IconData {
    egui::IconData { rgba: include_bytes!("../assets/icon64.rgba").to_vec(), width: 64, height: 64 }
}

fn status_text(it: &plan::PlanItem) -> String {
    match &it.status {
        Status::Move => {
            let renamed = it.is_renamed();
            let mut s = match (it.is_rename_only(), renamed) {
                (true, _) => "Rename".to_string(),
                (false, true) => "Rename + move".to_string(),
                (false, false) => "Move".to_string(),
            };
            if it.renamed_for_collision {
                s.push_str(" (clash -> suffix)");
            }
            s
        }
        s => s.label().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Rule editor widgets (free functions: they only touch the rule)
// ---------------------------------------------------------------------------

const POS_LABELS: [&str; 7] = [
    "Prefix (at the start)",
    "Suffix (at the end)",
    "At position N",
    "N chars before the end",
    "After text",
    "Before text",
    "Replace current name",
];

fn position_editor(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, pos: &mut Position) {
    let cur = match pos {
        Position::Prefix => 0,
        Position::Suffix => 1,
        Position::At(_) => 2,
        Position::FromEnd(_) => 3,
        Position::AfterText(_) => 4,
        Position::BeforeText(_) => 5,
        Position::Replace => 6,
    };
    let mut k = cur;
    egui::ComboBox::from_id_salt(id).width(190.0).selected_text(POS_LABELS[k]).show_ui(ui, |ui| {
        for (i, l) in POS_LABELS.iter().enumerate() {
            ui.selectable_value(&mut k, i, *l);
        }
    });
    let n = match pos {
        Position::At(n) | Position::FromEnd(n) => *n,
        _ => 1,
    };
    let t = match pos {
        Position::AfterText(t) | Position::BeforeText(t) => t.clone(),
        _ => String::new(),
    };
    if k != cur {
        *pos = match k {
            0 => Position::Prefix,
            1 => Position::Suffix,
            2 => Position::At(n),
            3 => Position::FromEnd(n),
            4 => Position::AfterText(t),
            5 => Position::BeforeText(t),
            _ => Position::Replace,
        };
    }
    match pos {
        Position::At(n) | Position::FromEnd(n) => {
            ui.add(egui::DragValue::new(n).range(0..=255).prefix("N = "));
        }
        Position::AfterText(t) | Position::BeforeText(t) => {
            text_field(ui, t, "text", 110.0);
        }
        _ => {}
    }
}

fn text_field(ui: &mut egui::Ui, s: &mut String, hint: &str, width: f32) {
    ui.add(egui::TextEdit::singleline(s).hint_text(hint).desired_width(width));
}

/// Editor for one rule's settings.
fn rule_editor(ui: &mut egui::Ui, idx: usize, kind: &mut RuleKind) {
    let w = 220.0;
    egui::Grid::new(("rule-grid", idx)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match kind {
        RuleKind::Replace(r) => {
            let ReplaceRule { find, with, regex, case_sensitive, occurrence } = r;
            ui.label("Find");
            text_field(ui, find, if *regex { r"regex, e.g. ^IMG_(\d+)" } else { "text to find" }, w);
            ui.end_row();
            ui.label("Replace with");
            text_field(ui, with, if *regex { "text, $1 = group 1" } else { "(empty = remove)" }, w);
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.checkbox(regex, "Regular expression");
                ui.checkbox(case_sensitive, "Case-sensitive");
            });
            ui.end_row();
            ui.label("Occurrences");
            ui.horizontal(|ui| {
                ui.radio_value(occurrence, Occurrence::All, "All");
                ui.radio_value(occurrence, Occurrence::First, "First");
                ui.radio_value(occurrence, Occurrence::Last, "Last");
            });
            ui.end_row();
        }
        RuleKind::Remove(m) => {
            let labels = [
                "Text (occurrences)",
                "Any of these characters",
                "First N characters",
                "Last N characters",
                "Range (from–to)",
                "All whitespace",
                "All digits",
            ];
            let cur = match m {
                RemoveMode::Text { .. } => 0,
                RemoveMode::Chars { .. } => 1,
                RemoveMode::First(_) => 2,
                RemoveMode::Last(_) => 3,
                RemoveMode::Range { .. } => 4,
                RemoveMode::Whitespace => 5,
                RemoveMode::Digits => 6,
            };
            let mut k = cur;
            ui.label("Remove");
            egui::ComboBox::from_id_salt(("rm-mode", idx)).width(w).selected_text(labels[k]).show_ui(ui, |ui| {
                for (i, l) in labels.iter().enumerate() {
                    ui.selectable_value(&mut k, i, *l);
                }
            });
            ui.end_row();
            if k != cur {
                *m = match k {
                    0 => RemoveMode::Text { text: String::new(), occurrence: Occurrence::All, case_sensitive: false },
                    1 => RemoveMode::Chars { chars: String::new(), case_sensitive: false },
                    2 => RemoveMode::First(1),
                    3 => RemoveMode::Last(1),
                    4 => RemoveMode::Range { from: 1, to: 1 },
                    5 => RemoveMode::Whitespace,
                    _ => RemoveMode::Digits,
                };
            }
            match m {
                RemoveMode::Text { text, occurrence, case_sensitive } => {
                    ui.label("Text");
                    text_field(ui, text, "text to remove", w);
                    ui.end_row();
                    ui.label("Occurrences");
                    ui.horizontal(|ui| {
                        ui.radio_value(occurrence, Occurrence::All, "All");
                        ui.radio_value(occurrence, Occurrence::First, "First");
                        ui.radio_value(occurrence, Occurrence::Last, "Last");
                        ui.checkbox(case_sensitive, "Case-sensitive");
                    });
                    ui.end_row();
                }
                RemoveMode::Chars { chars, case_sensitive } => {
                    ui.label("Characters");
                    ui.horizontal(|ui| {
                        text_field(ui, chars, "e.g. ()[]#", 140.0);
                        ui.checkbox(case_sensitive, "Case-sensitive");
                    });
                    ui.end_row();
                }
                RemoveMode::First(n) | RemoveMode::Last(n) => {
                    ui.label("N");
                    ui.add(egui::DragValue::new(n).range(0..=255));
                    ui.end_row();
                }
                RemoveMode::Range { from, to } => {
                    ui.label("Positions");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(from).range(1..=255).prefix("from "));
                        ui.add(egui::DragValue::new(to).range(1..=255).prefix("to "));
                        ui.label(RichText::new("(1 = first character)").weak().small());
                    });
                    ui.end_row();
                }
                _ => {}
            }
        }
        RuleKind::AddText(AddTextRule { text, position }) => {
            ui.label("Text");
            text_field(ui, text, "text to add", w);
            ui.end_row();
            ui.label("Where");
            ui.horizontal(|ui| position_editor(ui, ("add-pos", idx), position));
            ui.end_row();
        }
        RuleKind::Numbering(NumberingRule {
            start,
            step,
            padding,
            position,
            separator,
            per_folder,
            repeat,
            reset_every,
            system,
        }) => {
            ui.label("Index");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(start).prefix("starts at "));
                ui.add(egui::DragValue::new(step).prefix("step "));
                ui.add(egui::DragValue::new(repeat).range(1..=1000).prefix("repeat "));
                ui.add(egui::DragValue::new(reset_every).range(0..=100000).prefix("reset every "))
                    .on_hover_text("Start again after this many files (0 = never)");
            });
            ui.end_row();
            ui.label("Numbering");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("num-sys", idx)).width(200.0).selected_text(system.label()).show_ui(
                    ui,
                    |ui| {
                        for n in NumberSystem::ALL {
                            ui.selectable_value(system, n, n.label());
                        }
                    },
                );
                if *system == NumberSystem::Decimal {
                    ui.add(egui::DragValue::new(padding).range(0..=32).prefix("pad with zeros to "));
                }
            });
            ui.end_row();
            ui.label("Where");
            ui.horizontal(|ui| position_editor(ui, ("num-pos", idx), position));
            ui.end_row();
            ui.label("Separator");
            text_field(ui, separator, "between name and number", 120.0);
            ui.end_row();
            ui.label("");
            ui.checkbox(per_folder, "Reset if folder changes");
            ui.end_row();
        }
        RuleKind::Case(c) => {
            ui.label("Case");
            ui.horizontal_wrapped(|ui| {
                for m in CaseMode::ALL {
                    ui.radio_value(c, m, m.label());
                }
            });
            ui.end_row();
        }
        RuleKind::Clean(CleanRule {
            trim,
            collapse_spaces,
            underscores_to_spaces,
            spaces_to_underscores,
            dots_to_spaces,
            remove_illegal,
            collapse_repeats,
            strip_brackets,
            normalize_spaces,
            strip_emoji,
            strip_marks,
            space_before_capitals,
        }) => {
            ui.label("Clean up");
            ui.vertical(|ui| {
                ui.checkbox(strip_brackets, "Strip out content of brackets (…) […] {…}");
                ui.checkbox(normalize_spaces, "Normalize unicode spaces");
                ui.checkbox(strip_emoji, "Strip emoji");
                ui.checkbox(strip_marks, "Strip accents / marks (é -> e)");
                ui.checkbox(space_before_capitals, "Insert a space before capital letters");
                ui.checkbox(trim, "Trim spaces, _ - . at start and end");
                ui.checkbox(collapse_spaces, "Collapse multiple spaces");
                ui.checkbox(collapse_repeats, "Collapse repeated _ - . separators");
                ui.checkbox(underscores_to_spaces, "Underscores -> spaces");
                ui.checkbox(spaces_to_underscores, "Spaces -> underscores");
                ui.checkbox(dots_to_spaces, "Dots -> spaces");
                ui.checkbox(remove_illegal, "Remove characters not allowed on Windows");
            });
            ui.end_row();
        }
        RuleKind::Date(DateRule { source, format, position, separator }) => {
            ui.label("Date from");
            let label = |s: &Option<SourceKind>| match s {
                None => "Same date as the folder (date source below)".to_string(),
                Some(k) => format!("{} date", k.label()),
            };
            egui::ComboBox::from_id_salt(("date-src", idx)).width(w + 60.0).selected_text(label(source)).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(source, None, label(&None));
                    for k in SourceKind::ALL {
                        ui.selectable_value(source, Some(k), label(&Some(k)));
                    }
                },
            );
            ui.end_row();
            ui.label("Format");
            ui.horizontal(|ui| {
                text_field(ui, format, "%Y-%m-%d", 140.0);
                for (f, tip) in
                    [("%Y-%m-%d", "2024-03-15"), ("%Y%m%d_%H%M%S", "20240315_142530"), ("%B %Y", "March 2024")]
                {
                    if ui.small_button(tip).on_hover_text(f).clicked() {
                        *format = f.to_string();
                    }
                }
            });
            ui.end_row();
            ui.label("");
            let sample = chrono::NaiveDate::from_ymd_opt(2024, 3, 15).unwrap().and_hms_opt(14, 25, 30).unwrap();
            match rename::format_date(sample, format) {
                Some(t) => ui.label(
                    RichText::new(format!("e.g. “{t}”   (%Y year, %m month, %d day, %H %M %S time, %B month name)"))
                        .weak()
                        .small(),
                ),
                None => ui.label(RichText::new("Invalid format").color(red()).small()),
            };
            ui.end_row();
            ui.label("Where");
            ui.horizontal(|ui| position_editor(ui, ("date-pos", idx), position));
            ui.end_row();
            ui.label("Separator");
            text_field(ui, separator, "e.g. space or _", 120.0);
            ui.end_row();
        }
        RuleKind::ParentFolder(ParentRule { position, separator }) => {
            ui.label("Where");
            ui.horizontal(|ui| position_editor(ui, ("par-pos", idx), position));
            ui.end_row();
            ui.label("Separator");
            text_field(ui, separator, "e.g.  - ", 120.0);
            ui.end_row();
            ui.label("");
            ui.label(
                RichText::new("Uses the file's folder; for files already in a Month YYYY folder, the folder above it.")
                    .weak()
                    .small(),
            );
            ui.end_row();
        }
        RuleKind::Delete(DeleteRule { from, until, right_to_left, keep_delimiters }) => {
            ui.label("From");
            ui.horizontal(|ui| {
                let mut by_delim = matches!(from, DeleteFrom::Delimiter(_));
                ui.radio_value(&mut by_delim, false, "position");
                ui.radio_value(&mut by_delim, true, "delimiter");
                match (by_delim, &*from) {
                    (true, DeleteFrom::Position(_)) => *from = DeleteFrom::Delimiter(String::new()),
                    (false, DeleteFrom::Delimiter(_)) => *from = DeleteFrom::Position(1),
                    _ => {}
                }
                match from {
                    DeleteFrom::Position(n) => {
                        ui.add(egui::DragValue::new(n).range(1..=255));
                    }
                    DeleteFrom::Delimiter(t) => text_field(ui, t, "e.g. (", 90.0),
                }
            });
            ui.end_row();
            ui.label("Until");
            ui.horizontal(|ui| {
                let mut k = match until {
                    DeleteUntil::Count(_) => 0,
                    DeleteUntil::Delimiter(_) => 1,
                    DeleteUntil::End => 2,
                };
                let old = k;
                ui.radio_value(&mut k, 0, "count");
                ui.radio_value(&mut k, 1, "delimiter");
                ui.radio_value(&mut k, 2, "till the end");
                if k != old {
                    *until = match k {
                        0 => DeleteUntil::Count(1),
                        1 => DeleteUntil::Delimiter(String::new()),
                        _ => DeleteUntil::End,
                    };
                }
                match until {
                    DeleteUntil::Count(n) => {
                        ui.add(egui::DragValue::new(n).range(0..=255));
                    }
                    DeleteUntil::Delimiter(t) => text_field(ui, t, "e.g. )", 90.0),
                    DeleteUntil::End => {}
                }
            });
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.checkbox(right_to_left, "Right-to-left");
                ui.checkbox(keep_delimiters, "Do not remove delimiters");
            });
            ui.end_row();
        }
        RuleKind::Strip(StripRule { english, digits, symbols, brackets, user, except, case_sensitive }) => {
            ui.label("Characters");
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(english, "English letters");
                ui.checkbox(digits, "Digits");
                ui.checkbox(symbols, "Symbols");
                ui.checkbox(brackets, "Brackets");
            });
            ui.end_row();
            ui.label("User defined");
            text_field(ui, user, "e.g. #~", 140.0);
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.checkbox(except, "Strip all characters EXCEPT selected");
                ui.checkbox(case_sensitive, "Case-sensitive");
            });
            ui.end_row();
        }
        RuleKind::Rearrange(RearrangeRule { split, by_positions, pattern }) => {
            ui.label("Split using");
            ui.horizontal(|ui| {
                ui.radio_value(by_positions, false, "delimiters");
                ui.radio_value(by_positions, true, "positions");
                text_field(ui, split, if *by_positions { "e.g. 5|7" } else { "e.g.  - |_" }, 120.0);
            });
            ui.end_row();
            ui.label("New pattern");
            text_field(ui, pattern, "$2 - $1", w);
            ui.end_row();
            ui.label("");
            ui.label(
                RichText::new(
                    "Separate several delimiters/positions with |.  $1 $2 … parts, $-1 last part, $0 whole name",
                )
                .weak()
                .small(),
            );
            ui.end_row();
        }
        RuleKind::Padding(PaddingRule { zero_pad, remove_zero_padding, text_pad_len, pad_chars, pad_left }) => {
            ui.label("Number sequences");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(zero_pad).range(0..=32).prefix("zero-pad to ")).on_hover_text("0 = off");
                ui.checkbox(remove_zero_padding, "Remove zero padding");
            });
            ui.end_row();
            ui.label("Text padding");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(text_pad_len).range(0..=255).prefix("to length ")).on_hover_text("0 = off");
                text_field(ui, pad_chars, "chars", 50.0);
                ui.radio_value(pad_left, true, "left");
                ui.radio_value(pad_left, false, "right");
            });
            ui.end_row();
        }
        RuleKind::Randomize(RandomizeRule { length, digits, lower, upper, user, unique, position, .. }) => {
            ui.label("Length");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(length).range(1..=64));
                ui.checkbox(unique, "Unique if possible");
            });
            ui.end_row();
            ui.label("Characters");
            ui.horizontal(|ui| {
                ui.checkbox(digits, "0-9");
                ui.checkbox(lower, "a-z");
                ui.checkbox(upper, "A-Z");
                text_field(ui, user, "more", 60.0);
            });
            ui.end_row();
            ui.label("Where");
            ui.horizontal(|ui| position_editor(ui, ("rnd-pos", idx), position));
            ui.end_row();
        }
        RuleKind::Translit => {
            ui.label("");
            ui.label(
                RichText::new("Latin accents and special letters (é->e, ß->ss, Ø->O), Cyrillic and Greek -> Latin")
                    .weak(),
            );
            ui.end_row();
        }
        RuleKind::ReformatDate(ReformatDateRule { find, to, whole_words }) => {
            ui.label("Find dates like");
            text_field(ui, find, "%d.%m.%Y|%Y%m%d", w);
            ui.end_row();
            ui.label("Convert to");
            text_field(ui, to, "%Y-%m-%d", w);
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.checkbox(whole_words, "Match as whole words only");
                ui.label(RichText::new("%Y %y %m %d %H %M %S %b %B; several formats separated by |").weak().small());
            });
            ui.end_row();
        }
        RuleKind::Extension(e) => {
            ui.label("Extension");
            ui.horizontal(|ui| {
                let mut detect = matches!(e, ExtensionRule::Detect);
                ui.radio_value(&mut detect, true, "Fix from file contents");
                ui.radio_value(&mut detect, false, "Set to");
                match (detect, &*e) {
                    (true, ExtensionRule::Set(_)) => *e = ExtensionRule::Detect,
                    (false, ExtensionRule::Detect) => *e = ExtensionRule::Set("jpg".into()),
                    _ => {}
                }
                if let ExtensionRule::Set(x) = e {
                    text_field(ui, x, "jpg", 60.0);
                }
            });
            ui.end_row();
            ui.label("");
            ui.label(RichText::new("Only image/video extensions are allowed.").weak().small());
            ui.end_row();
        }
        RuleKind::ExtensionCase(e) => {
            ui.label("Extension");
            ui.horizontal(|ui| {
                ui.radio_value(e, ExtCase::Lower, "lower case (.jpg)");
                ui.radio_value(e, ExtCase::Upper, "UPPER CASE (.JPG)");
            });
            ui.end_row();
        }
    });
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_style(&ctx);
        self.poll_worker();
        self.drive_live_preview(&ctx);
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // remember window position / size / maximized (saved debounced)
        self.track_window(&ctx);

        // drag & drop a folder (or a file inside it)
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.first()
            && !matches!(self.busy_kind(), Some(JobKind::Apply | JobKind::Undo))
        {
            self.set_folder(p);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        self.shortcuts(&ctx);
        self.menu_bar(ui);
        self.top_bar(ui);
        self.status_bar(ui);
        self.action_bar(ui, &ctx);
        self.setup_panel(ui);
        egui::CentralPanel::default_margins().show(ui, |ui| self.central(ui));

        if hovering {
            let rect = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("photosorter-drop-overlay"),
            ));
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop a folder to select it",
                FontId::proportional(self.s.font_size * 1.8),
                Color32::WHITE,
            );
        }

        self.confirm_dialogs(&ctx);
        self.options_window(&ctx);
        self.manage_presets_window(&ctx);
        self.dialogs(&ctx);
        self.autosave_layout(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

/// Collapsible numbered section frame.
fn step_section(
    ui: &mut egui::Ui,
    num: &str,
    title: &str,
    summary: &str,
    open: &mut bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    let fill = ui.visuals().faint_bg_color;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let r = ui
                .horizontal(|ui| {
                    let arrow = if *open { "⏷" } else { "⏵" };
                    let t = ui.add(
                        egui::Button::new(RichText::new(format!("{arrow} {num}")).strong().color(accent(ui)))
                            .frame(false),
                    );
                    let h = ui.add(
                        egui::Button::new(
                            RichText::new(title).strong().size(ui.style().text_styles[&TextStyle::Body].size * 1.12),
                        )
                        .frame(false),
                    );
                    ui.add(egui::Label::new(RichText::new(summary).weak()).truncate());
                    t.clicked() || h.clicked()
                })
                .inner;
            if r {
                *open = !*open;
            }
            if *open {
                ui.add_space(4.0);
                add(ui);
            }
        });
}

impl App {
    fn open_options(&mut self) {
        self.options = Some(OptionsDraft { extensions: self.s.extensions.clone(), order_ui: self.order_ui.clone() });
    }

    fn locked(&self) -> bool {
        matches!(self.busy_kind(), Some(JobKind::Apply | JobKind::Undo))
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.confirm.is_some() || self.dialog.is_some() || self.options.is_some() {
            return;
        }
        let ctrl_n = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
        let ctrl_o = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
        let (n, o, f1) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&ctrl_n),
                i.consume_shortcut(&ctrl_o),
                i.consume_key(egui::Modifiers::NONE, egui::Key::F1),
            )
        });
        if f1 {
            self.open_help();
        }
        if self.locked() {
            return;
        }
        if n {
            self.request_new_project();
        } else if o {
            self.browse_folder();
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let locked = self.locked();
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add_enabled(!locked, egui::Button::new("New Project").shortcut_text("Ctrl+N"))
                        .on_hover_text("Clear the folder, preview and rules; no files are touched")
                        .clicked()
                    {
                        ui.close();
                        self.request_new_project();
                    }
                    if ui.add_enabled(!locked, egui::Button::new("Open Folder…").shortcut_text("Ctrl+O")).clicked() {
                        ui.close();
                        self.browse_folder();
                    }
                    ui.separator();
                    let log = self.undo_log_for_root();
                    if ui
                        .add_enabled(!locked && log.is_some(), egui::Button::new("Undo Last Apply…"))
                        .on_disabled_hover_text("No undo log (PhotoSorter-undo-*.json) in the chosen folder")
                        .clicked()
                        && let Some(p) = log
                    {
                        ui.close();
                        self.request_undo(p);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(!locked, egui::Button::new("Exit"))
                        .on_disabled_hover_text("Wait until Apply / Undo has finished")
                        .clicked()
                    {
                        ui.close();
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Presets", |ui| {
                    ui.add_enabled_ui(!locked, |ui| {
                        ui.menu_button("Load", |ui| {
                            if self.s.presets.is_empty() {
                                ui.label(RichText::new("No presets saved yet").weak());
                            }
                            let names: Vec<(String, PresetScope)> =
                                self.s.presets.iter().map(|p| (p.name.clone(), p.scope)).collect();
                            for (name, scope) in names {
                                if ui.button(&name).on_hover_text(format!("Loads: {}", scope.label())).clicked() {
                                    ui.close();
                                    self.load_preset(&name);
                                }
                            }
                        });
                        if ui.button("Save current as preset…").clicked() {
                            ui.close();
                            self.dialog =
                                Some(Dialog::SavePreset { name: self.editing_preset.clone().unwrap_or_default() });
                        }
                        if let Some(n) = self.editing_preset.clone()
                            && ui.button(format!("Save changes to “{n}”")).clicked()
                        {
                            ui.close();
                            self.save_editing_preset();
                        }
                        ui.separator();
                        if ui.button("Manage Presets…").clicked() {
                            ui.close();
                            self.open_manage(None);
                        }
                    });
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Options…").on_hover_text("File types and date-detection priority").clicked() {
                        self.open_options();
                        ui.close();
                    }
                    ui.separator();
                    let settings_dir = self.ini_path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
                    if ui
                        .add_enabled(settings_dir.is_some(), egui::Button::new("Open Settings Folder"))
                        .on_hover_text("The folder with PhotoSorter.exe and PhotoSorter.ini")
                        .clicked()
                        && let Some(d) = settings_dir
                    {
                        ui.close();
                        self.open_folder(&d);
                    }
                    let log_dir = self.undo_log_for_root().and_then(|p| p.parent().map(Path::to_path_buf));
                    if ui
                        .add_enabled(log_dir.is_some(), egui::Button::new("Open Undo Log Folder"))
                        .on_disabled_hover_text("No undo log in the chosen folder")
                        .clicked()
                        && let Some(d) = log_dir
                    {
                        ui.close();
                        self.open_folder(&d);
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.add(egui::Button::new("Help").shortcut_text("F1")).on_hover_text("Open README.txt").clicked()
                    {
                        ui.close();
                        self.open_help();
                    }
                    if ui.button("About PhotoSorter…").clicked() {
                        ui.close();
                        self.dialog = Some(Dialog::About);
                    }
                });
            });
        });
    }

    fn browse_folder(&mut self) {
        let mut dlg = rfd::FileDialog::new().set_title("Choose a folder with photos / videos");
        let cur = self.root();
        if cur.is_dir() {
            dlg = dlg.set_directory(&cur);
        }
        if let Some(p) = dlg.pick_folder() {
            self.set_folder(&p);
        }
    }

    fn request_undo(&mut self, p: PathBuf) {
        match ops::read_log(&p) {
            Ok(l) => self.confirm = Some(Confirm::Undo { moves: l.moves.len(), started: l.started, log: p }),
            Err(e) => self.status = format!("Cannot read undo log: {e}"),
        }
    }

    fn open_folder(&mut self, d: &Path) {
        if let Err(e) = desktop::open_folder(d) {
            self.dialog = Some(Dialog::Message { title: "Cannot open folder".into(), text: e.to_string() });
        }
    }

    fn open_help(&mut self) {
        let Some(dir) = settings::exe_dir() else {
            self.dialog = Some(Dialog::ReadmeMissing(PathBuf::from(desktop::README_NAME)));
            return;
        };
        match desktop::find_readme(&dir) {
            Ok(p) => match desktop::open_text_file(&p) {
                Ok(()) => self.status = format!("Opened {}", p.display()),
                Err(e) => {
                    self.dialog = Some(Dialog::Message {
                        title: "Cannot open README.txt".into(),
                        text: format!("{}: {e}", p.display()),
                    })
                }
            },
            Err(p) => self.dialog = Some(Dialog::ReadmeMissing(p)),
        }
    }

    fn request_new_project(&mut self) {
        if self.locked() {
            return;
        }
        let pending = if self.plan.is_some() { self.counts.to_move } else { 0 };
        let log = self.undo_log_for_root();
        let w = session::new_project_warnings(pending, log.as_deref(), session::rules_unsaved(&self.s));
        if w.is_empty() {
            self.do_new_project();
        } else {
            self.dialog = Some(Dialog::NewProject(w));
        }
    }

    /// Reset the in-memory session. Never touches files, folders or undo logs.
    fn do_new_project(&mut self) {
        if self.locked() {
            return;
        }
        if let Some(w) = self.worker.take() {
            w.cancel.store(true, Ordering::Relaxed); // a running preview only
        }
        self.s = session::new_project(&self.s);
        self.rebuild_order_ui();
        self.plan = None;
        self.counts = Default::default();
        self.folder_rows.clear();
        self.source_summary.clear();
        self.plan_sig.clear();
        self.plan_cancelled = false;
        self.scan_cache = None;
        self.scan_errors.clear();
        self.dirs_scanned = 0;
        self.changed_at = None;
        self.auto_paused = false;
        self.rule_errors.clear();
        self.progress_text.clear();
        self.progress = 0.0;
        self.run_report = None;
        self.undo_report = None;
        self.tab = Tab::Files;
        self.filter_text.clear();
        self.filter_status = StatusFilter::All;
        self.view.clear();
        self.view_dirty = true;
        self.open_rule = None;
        self.preset_sel.clear();
        self.preset_name.clear();
        self.editing_preset = None;
        self.invalidate_undo_cache();
        self.status = "New project — choose a folder. No files were changed.".into();
    }

    fn load_preset(&mut self, name: &str) {
        if let Some(p) = self.s.find_preset(name).cloned() {
            self.s.apply_preset(&p);
            self.rebuild_order_ui();
            self.open_rule = None;
            self.preset_sel = p.name.clone();
            self.status = format!("Loaded preset “{}” ({}).", p.name, p.scope.label().to_lowercase());
        }
    }

    fn save_editing_preset(&mut self) {
        if let Some(n) = self.editing_preset.clone() {
            if self.s.update_preset(&n) {
                self.save_settings();
                self.status = format!("Saved changes to preset “{n}”.");
            } else {
                self.editing_preset = None;
            }
        }
    }

    fn open_manage(&mut self, sel: Option<String>) {
        self.manage_open = true;
        let sel = sel
            .or_else(|| self.editing_preset.clone())
            .or_else(|| self.s.presets.first().map(|p| p.name.clone()))
            .unwrap_or_default();
        self.manage_rename = sel.clone();
        self.manage_sel = sel;
        self.manage_error = None;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("📷 PhotoSorter").heading().strong());
                ui.label(
                    RichText::new("rename & sort photos and videos — ReNamer-style rules, then “Month YYYY” folders")
                        .weak(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // quick shortcut; Dark / Light / Follow system in Tools > Options
                    let (icon, tip, next) = if ui.visuals().dark_mode {
                        ("☀", "Switch to the light theme (more in Tools > Options > Appearance)", Theme::Light)
                    } else {
                        ("🌙", "Switch to the dark theme (more in Tools > Options > Appearance)", Theme::Dark)
                    };
                    if ui.button(icon).on_hover_text(tip).clicked() {
                        self.s.theme = next;
                    }
                    if ui.button("A+").on_hover_text("Larger text").clicked() {
                        self.s.font_size = (self.s.font_size + 1.0).min(28.0);
                    }
                    if ui.button("A−").on_hover_text("Smaller text").clicked() {
                        self.s.font_size = (self.s.font_size - 1.0).max(10.0);
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                if self.busy() {
                    ui.add(egui::Spinner::new());
                    ui.add(egui::ProgressBar::new(self.progress).desired_width(220.0).show_percentage().animate(true));
                    ui.add(egui::Label::new(&self.progress_text).truncate());
                } else {
                    ui.add(egui::Label::new(&self.status).truncate());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| match (&self.notice, &self.ini_path) {
                    (Some(n), _) => {
                        ui.label(RichText::new(format!("⚠ {n}")).color(amber()));
                    }
                    (None, Some(p)) => {
                        ui.label(RichText::new(format!("Settings: {}", settings::INI_NAME)).weak())
                            .on_hover_text(p.display().to_string());
                    }
                    _ => {}
                });
            });
            ui.add_space(3.0);
        });
    }

    /// Step 5: Apply + Undo, always visible at the bottom.
    fn action_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("5").strong().color(accent(ui)));
                let h = self.s.font_size + 16.0;
                let kind = self.busy_kind();
                if matches!(kind, Some(JobKind::Apply | JobKind::Undo)) {
                    let can_cancel = kind == Some(JobKind::Apply);
                    let label = if can_cancel { "⏹ Cancel" } else { "Undo running…" };
                    if ui.add_enabled(can_cancel, egui::Button::new(label).min_size(Vec2::new(160.0, h))).clicked()
                        && let Some(w) = &self.worker
                    {
                        w.cancel.store(true, Ordering::Relaxed);
                    }
                    return;
                }
                let current = self.plan_is_current() && kind.is_none();
                let n = if current { self.counts.to_move } else { 0 };
                let text = if n > 0 {
                    format!(
                        "✔ Apply — {} files ({} renamed, {} moved)",
                        n,
                        self.counts.renamed,
                        n - self.counts.rename_only
                    )
                } else {
                    "✔ Apply".to_string()
                };
                let btn = if current && n > 0 {
                    egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
                        .fill(green().gamma_multiply(0.9))
                        .min_size(Vec2::new(280.0, h))
                } else {
                    egui::Button::new(RichText::new(text)).min_size(Vec2::new(280.0, h))
                };
                let r = ui.add_enabled(current && n > 0, btn);
                let r = if !self.rule_errors.is_empty() {
                    r.on_disabled_hover_text("Fix the rule errors first")
                } else if kind == Some(JobKind::Preview) {
                    r.on_disabled_hover_text("Preview is still updating…")
                } else {
                    r.on_disabled_hover_text("Nothing to do (or the preview is not up to date)")
                };
                if r.clicked() {
                    self.confirm = Some(Confirm::Apply);
                }

                let log = self.undo_log_for_root();
                let undo_btn = egui::Button::new("↩ Undo last run").min_size(Vec2::new(170.0, h));
                let r = ui.add_enabled(log.is_some(), undo_btn);
                let r = match &log {
                    Some(p) => r.on_hover_text(format!("Restores original names and folders using {}", p.display())),
                    None => r.on_disabled_hover_text("No undo log (PhotoSorter-undo-*.json) in this folder"),
                };
                if r.clicked()
                    && let Some(p) = log
                {
                    self.request_undo(p);
                }
                if kind == Some(JobKind::Preview) {
                    if ui.button("⏹ Stop preview").clicked()
                        && let Some(w) = &self.worker
                    {
                        w.cancel.store(true, Ordering::Relaxed);
                    }
                } else if ui
                    .button("↻ Refresh preview")
                    .on_hover_text("Re-scan the folder (e.g. after files changed on disk)")
                    .clicked()
                {
                    self.refresh();
                    let _ = ctx;
                }
                ui.add_space(8.0);
                let hint = if !self.rule_errors.is_empty() {
                    RichText::new("⚠ A rule has an error — see the rules list").color(red())
                } else if self.plan.is_some() && !self.plan_is_current() && kind.is_none() {
                    RichText::new("Preview is out of date").color(amber())
                } else {
                    RichText::new(
                        "Nothing changes on disk until you click Apply and confirm. Undo log is written first.",
                    )
                    .weak()
                    .small()
                };
                ui.add(egui::Label::new(hint).truncate());
            });
            ui.add_space(6.0);
        });
    }

    /// Steps 1–3 in a resizable, scrollable top area.
    fn setup_panel(&mut self, ui: &mut egui::Ui) {
        let fs = self.s.font_size;
        let default_h = self.s.layout.setup_height.unwrap_or(fs * 29.0);
        let panel =
            egui::Panel::top(("setup", self.layout_gen)).resizable(true).default_size(default_h).min_size(fs * 4.0);
        let r = panel.show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                let locked = matches!(self.busy_kind(), Some(JobKind::Apply | JobKind::Undo));
                ui.add_enabled_ui(!locked, |ui| {
                    self.source_section(ui);
                    ui.add_space(6.0);
                    self.rules_section(ui);
                    ui.add_space(6.0);
                    self.output_section(ui);
                });
                ui.add_space(6.0);
            });
        });
        // remember a height the user dragged to (not the automatic default)
        let h = r.response.rect.height().round();
        if (h - default_h).abs() > 1.5 && h >= 40.0 {
            self.s.layout.setup_height = Some(h);
        }
    }

    fn source_section(&mut self, ui: &mut egui::Ui) {
        let summary = if self.s.folder_trimmed().is_empty() {
            "no folder chosen".to_string()
        } else {
            format!("{}{}", self.s.folder_trimmed(), if self.s.include_subfolders { "  (+ subfolders)" } else { "" })
        };
        let mut open = self.s.layout.source_open;
        step_section(ui, "1", "Source & filter", &summary, &mut open, |ui| {
            ui.horizontal(|ui| {
                if ui.button("📂 Browse…").on_hover_text("File > Open Folder… (Ctrl+O)").clicked() {
                    self.browse_folder();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.s.last_folder)
                        .hint_text("…or drop a folder here")
                        .desired_width(f32::INFINITY),
                );
            });
            if !self.s.folder_trimmed().is_empty() && !self.root().is_dir() {
                ui.label(RichText::new("Folder does not exist").color(red()).small());
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.s.include_subfolders, "Include subfolders")
                    .on_hover_text("Every folder containing photos/videos gets its own Month YYYY subfolders.");
                ui.separator();
                let summary = scan::extension_summary(&self.s.extensions);
                let warn = !scan::rejected_extensions(&self.s.extensions).is_empty();
                let text = RichText::new(format!("File types: {summary} (change in Tools > Options)")).weak();
                let text = if warn { text.color(amber()) } else { text };
                ui.add(egui::Label::new(text).truncate())
                    .on_hover_text(scan::parse_extensions(&self.s.extensions).join(", "));
                if ui.small_button("Options…").on_hover_text("Edit the file types (Tools > Options)").clicked() {
                    self.open_options();
                }
            });
        });
        self.s.layout.source_open = open;
    }

    fn rules_section(&mut self, ui: &mut egui::Ui) {
        let enabled = self.s.rules.iter().filter(|r| r.enabled).count();
        let summary = if self.s.rules.is_empty() {
            "no rules — file names are kept".to_string()
        } else {
            format!("{} rules ({} enabled), applied top to bottom", self.s.rules.len(), enabled)
        };
        let mut open = self.s.layout.rules_open;
        step_section(ui, "2", "Rename rules", &summary, &mut open, |ui| {
            self.presets_row(ui);
            ui.add_space(4.0);
            let n = self.s.rules.len();
            let mut action: Option<(usize, RuleAction)> = None;
            for i in 0..n {
                let err = self.rule_errors.iter().find(|(k, _)| *k == i).map(|(_, e)| e.clone());
                let is_open = self.open_rule == Some(i);
                let stroke = if err.is_some() {
                    Stroke::new(1.5, red())
                } else if is_open {
                    Stroke::new(1.0, accent(ui))
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                };
                egui::Frame::new()
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let rule = &mut self.s.rules[i];
                            ui.checkbox(&mut rule.enabled, "").on_hover_text("Enable / disable this rule");
                            ui.label(RichText::new(format!("{}.", i + 1)).weak());
                            let title = RichText::new(rule.kind.type_label()).strong();
                            let title = if rule.enabled { title } else { title.weak() };
                            if ui.add(egui::Button::new(title).frame(false)).on_hover_text("Edit").clicked() {
                                action = Some((i, RuleAction::ToggleOpen));
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("🗑").on_hover_text("Delete rule").clicked() {
                                    action = Some((i, RuleAction::Delete));
                                }
                                if ui.small_button("📋").on_hover_text("Duplicate rule").clicked() {
                                    action = Some((i, RuleAction::Duplicate));
                                }
                                if ui
                                    .add_enabled(i + 1 < n, egui::Button::new("⏷").small())
                                    .on_hover_text("Move down")
                                    .clicked()
                                {
                                    action = Some((i, RuleAction::Down));
                                }
                                if ui
                                    .add_enabled(i > 0, egui::Button::new("⏶").small())
                                    .on_hover_text("Move up")
                                    .clicked()
                                {
                                    action = Some((i, RuleAction::Up));
                                }
                                if ui.small_button(if is_open { "Done" } else { "✏ Edit" }).clicked() {
                                    action = Some((i, RuleAction::ToggleOpen));
                                }
                                let rule = &self.s.rules[i];
                                let summ = RichText::new(rule.kind.summary());
                                let summ = if rule.enabled { summ } else { summ.weak().strikethrough() };
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.add(egui::Label::new(summ).truncate());
                                });
                            });
                        });
                        if let Some(e) = &err {
                            ui.label(RichText::new(format!("⚠ {e}")).color(red()).small());
                        }
                        if is_open {
                            ui.separator();
                            rule_editor(ui, i, &mut self.s.rules[i].kind);
                        }
                    });
                ui.add_space(2.0);
            }
            if let Some((i, a)) = action {
                match a {
                    RuleAction::Up if i > 0 => {
                        self.s.rules.swap(i, i - 1);
                        self.open_rule = self.open_rule.map(|o| {
                            if o == i {
                                i - 1
                            } else if o == i - 1 {
                                i
                            } else {
                                o
                            }
                        });
                    }
                    RuleAction::Down if i + 1 < self.s.rules.len() => {
                        self.s.rules.swap(i, i + 1);
                        self.open_rule = self.open_rule.map(|o| {
                            if o == i {
                                i + 1
                            } else if o == i + 1 {
                                i
                            } else {
                                o
                            }
                        });
                    }
                    RuleAction::Duplicate => {
                        let mut copy = self.s.rules[i].clone();
                        if let RuleKind::Randomize(r) = &mut copy.kind {
                            r.seed = rename::new_seed();
                        }
                        self.s.rules.insert(i + 1, copy);
                        self.open_rule = Some(i + 1);
                    }
                    RuleAction::Delete => {
                        self.s.rules.remove(i);
                        self.open_rule = match self.open_rule {
                            Some(o) if o == i => None,
                            Some(o) if o > i => Some(o - 1),
                            o => o,
                        };
                    }
                    RuleAction::ToggleOpen => {
                        self.open_rule = if self.open_rule == Some(i) { None } else { Some(i) };
                    }
                    _ => {}
                }
            }
            ui.horizontal(|ui| {
                ui.menu_button(RichText::new("➕ Add rule").strong(), |ui| {
                    for key in RULE_TYPES {
                        if ui.button(RuleKind::type_label_for(key)).clicked() {
                            if let Some(k) = RuleKind::new_default(key) {
                                self.s.rules.push(RuleSpec::new(k));
                                self.open_rule = Some(self.s.rules.len() - 1);
                            }
                            ui.close();
                        }
                    }
                });
                if !self.s.rules.is_empty() && ui.button("Clear all").on_hover_text("Remove every rule").clicked() {
                    self.s.rules.clear();
                    self.open_rule = None;
                }
                ui.label(
                    RichText::new(
                        "Rules change the name without its extension, top to bottom. The preview below updates live.",
                    )
                    .weak()
                    .small(),
                );
            });
        });
        self.s.layout.rules_open = open;
    }

    fn presets_row(&mut self, ui: &mut egui::Ui) {
        if let Some(n) = self.editing_preset.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("✏ Editing preset “{n}”")).strong().color(accent(ui)));
                ui.label(RichText::new("change the rules / options, then").weak());
                if ui
                    .button("💾 Save changes")
                    .on_hover_text("Write the current rules + options back to this preset")
                    .clicked()
                {
                    self.save_editing_preset();
                }
                if ui.button("Stop editing").clicked() {
                    self.editing_preset = None;
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Preset");
            let sel_text = if self.preset_sel.is_empty() { "(choose)".to_string() } else { self.preset_sel.clone() };
            egui::ComboBox::from_id_salt("preset").width(180.0).selected_text(sel_text).show_ui(ui, |ui| {
                for p in &self.s.presets {
                    ui.selectable_value(&mut self.preset_sel, p.name.clone(), &p.name);
                }
                if self.s.presets.is_empty() {
                    ui.label(RichText::new("No presets saved yet").weak());
                }
            });
            let exists = self.s.presets.iter().any(|p| p.name == self.preset_sel);
            if ui.add_enabled(exists, egui::Button::new("Load")).on_hover_text("Load rules + options").clicked() {
                let name = self.preset_sel.clone();
                self.load_preset(&name);
            }
            if ui.add_enabled(exists, egui::Button::new("Delete")).clicked() {
                self.dialog = Some(Dialog::DeletePreset(self.preset_sel.clone()));
            }
            if ui.button("Manage…").on_hover_text("Presets > Manage Presets…").clicked() {
                let sel = exists.then(|| self.preset_sel.clone());
                self.open_manage(sel);
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.preset_name).hint_text("new preset name").desired_width(150.0));
            let name = settings::clean_preset_name(&self.preset_name);
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("💾 Save as preset"))
                .on_hover_text("Saves the rules and the options of steps 1 and 3 into PhotoSorter.ini")
                .clicked()
                && self.s.save_preset(&name)
            {
                self.preset_sel = name.clone();
                self.preset_name.clear();
                self.save_settings();
                self.status = format!("Saved preset “{name}”.");
            }
        });
    }

    fn output_section(&mut self, ui: &mut egui::Ui) {
        let summary = if self.s.sort_into_folders {
            format!("into folders like “{}”, date: {}", tpl::example(&self.s.folder_template), self.s.date.mode.label())
        } else {
            "off — files stay in their folders (rename only)".to_string()
        };
        let mut open = self.s.layout.output_open;
        step_section(ui, "3", "Output folder", &summary, &mut open, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.s.sort_into_folders, RichText::new("Sort into date folders").strong())
                    .on_hover_text("Off: only rename files where they are; no folders are created.");
                ui.add_enabled_ui(self.s.sort_into_folders, |ui| {
                    ui.separator();
                    ui.label("Date source");
                    egui::ComboBox::from_id_salt("mode").width(200.0).selected_text(self.s.date.mode.label()).show_ui(
                        ui,
                        |ui| {
                            for m in [
                                DateMode::Auto,
                                DateMode::Only(SourceKind::Filename),
                                DateMode::Only(SourceKind::Exif),
                                DateMode::Only(SourceKind::Modified),
                                DateMode::Only(SourceKind::Created),
                            ] {
                                ui.selectable_value(&mut self.s.date.mode, m, m.label());
                            }
                        },
                    );
                });
            });
            ui.add_enabled_ui(self.s.sort_into_folders, |ui| {
                self.folder_name_builder(ui);
                let id = ui.make_persistent_id(("output-more", self.layout_gen));
                let more_open = self.s.layout.output_more_open;
                let r = egui::CollapsingHeader::new(RichText::new("More options").small())
                    .id_salt(id)
                    .open(Some(more_open))
                    .show(ui, |ui| {
                        egui::Grid::new("more-opts").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            ui.label("If no date found");
                            egui::ComboBox::from_id_salt("fallback")
                                .width(180.0)
                                .selected_text(self.s.date.fallback.label())
                                .show_ui(ui, |ui| {
                                    for f in [Fallback::Modified, Fallback::Created, Fallback::Skip] {
                                        ui.selectable_value(&mut self.s.date.fallback, f, f.label());
                                    }
                                });
                            ui.end_row();
                            ui.label("Ambiguous dates")
                                .on_hover_text("For names like 03-04-2024 where both numbers could be the month.");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.s.date.order, DateOrder::Us, "US (MM-DD)");
                                ui.radio_value(&mut self.s.date.order, DateOrder::Intl, "Intl (DD-MM)");
                            });
                            ui.end_row();
                        });
                        if self.s.date.mode == DateMode::Auto {
                            ui.horizontal(|ui| {
                                let order = order_summary(&self.order_ui);
                                ui.add(
                                    egui::Label::new(RichText::new(format!("Auto priority: {order}")).weak())
                                        .truncate(),
                                )
                                .on_hover_text("First match wins; also used by “Insert date”.");
                                if ui
                                    .small_button("Options…")
                                    .on_hover_text("Edit the Auto priority (Tools > Options)")
                                    .clicked()
                                {
                                    self.open_options();
                                }
                            });
                        }
                    });
                if r.header_response.clicked() {
                    self.s.layout.output_more_open = !more_open;
                }
            });
        });
        self.s.layout.output_open = open;
    }

    /// "Folder name": ready-made layouts, "Build your own" controls and the
    /// raw template ("Advanced"), all editing `folder_template`.
    fn folder_name_builder(&mut self, ui: &mut egui::Ui) {
        let template = self.s.folder_template.clone();
        let matched = FolderBuilder::from_template(&template);
        // Re-sync the controls only when the template changed elsewhere
        // (layout list, Advanced field, preset, New Project), so a custom
        // separator being typed is not overwritten mid-edit.
        if template != self.builder_synced {
            self.builder_synced = template.clone();
            if let Some(b) = &matched {
                self.builder = b.clone();
                self.sep_custom = matches!(b.sep, Separator::Custom(_));
                if let Separator::Custom(t) = &b.sep {
                    self.custom_sep = t.clone();
                }
            }
        }
        let layout_idx = tpl::LAYOUTS.iter().position(|(t, _)| *t == template);
        // a hand-typed template that fits the controls shows them
        if layout_idx.is_none() && matched.is_some() {
            self.folder_build = true;
        }
        let custom = layout_idx.is_none() && matched.is_none();
        let selected = if custom {
            "Custom template".to_string()
        } else if self.folder_build {
            "Build your own…".to_string()
        } else {
            let (t, note) = tpl::LAYOUTS[layout_idx.unwrap_or(0)];
            format!("{}   ({note})", tpl::example(t))
        };

        ui.horizontal_wrapped(|ui| {
            ui.label("Folder name");
            egui::ComboBox::from_id_salt("folder-layout").width(300.0).height(900.0).selected_text(selected).show_ui(
                ui,
                |ui| {
                    for (t, note) in tpl::LAYOUTS {
                        let on = !self.folder_build && layout_idx.is_some_and(|i| tpl::LAYOUTS[i].0 == t);
                        let text = format!("{}   ({note})", tpl::example(t));
                        if ui.selectable_label(on, text).on_hover_text(t).clicked() {
                            self.s.folder_template = t.to_string();
                            self.folder_build = false;
                        }
                    }
                    ui.separator();
                    if ui
                        .selectable_label(self.folder_build && !custom, "Build your own…")
                        .on_hover_text("Pick year / month / day style, order and separator")
                        .clicked()
                    {
                        self.folder_build = true;
                        if matched.is_none() {
                            self.s.folder_template = self.builder.to_template();
                        }
                    }
                    if custom {
                        let _ = ui.selectable_label(true, "Custom template (typed)");
                    }
                },
            );
            ui.label(RichText::new("Example:").weak());
            ui.label(RichText::new(tpl::example(&self.s.folder_template)).strong().color(accent(ui)))
                .on_hover_text("Folder for a photo taken on 5 March 2024");
            ui.toggle_value(&mut self.s.layout.folder_advanced, "Advanced")
                .on_hover_text("Show the template text, e.g. {MMMM} {YYYY}");
        });

        if self.folder_build || custom {
            let mut b = self.builder.clone();
            if self.sep_custom {
                b.sep = Separator::Custom(self.custom_sep.clone());
            }
            let before = b.clone();
            let mut custom_sep_edit = false;
            ui.horizontal_wrapped(|ui| {
                if custom {
                    ui.label(RichText::new("Custom — changing a control replaces it:").weak().small());
                }
                ui.label("Year");
                egui::ComboBox::from_id_salt("b-year").selected_text(year_label(b.year)).show_ui(ui, |ui| {
                    for y in YearStyle::ALL {
                        ui.selectable_value(&mut b.year, y, year_label(y));
                    }
                });
                ui.label("Month");
                egui::ComboBox::from_id_salt("b-month").selected_text(month_label(b.month)).show_ui(ui, |ui| {
                    for m in MonthStyle::ALL {
                        ui.selectable_value(&mut b.month, m, month_label(m));
                    }
                });
                ui.checkbox(&mut b.day, "Day");
                egui::ComboBox::from_id_salt("b-order")
                    .selected_text(order_label(b.order))
                    .show_ui(ui, |ui| {
                        for o in [PartOrder::MonthFirst, PartOrder::YearFirst] {
                            ui.selectable_value(&mut b.order, o, order_label(o));
                        }
                    })
                    .response
                    .on_hover_text("Order of the parts (the day always follows the month)");
                ui.label("Separator");
                let is_custom = matches!(b.sep, Separator::Custom(_));
                egui::ComboBox::from_id_salt("b-sep").height(400.0).selected_text(b.sep.label()).show_ui(ui, |ui| {
                    for sep in Separator::FIXED {
                        let label = sep.label();
                        if ui.selectable_label(!is_custom && b.sep == sep, label).clicked() {
                            b.sep = sep;
                            self.sep_custom = false;
                        }
                    }
                    if ui.selectable_label(is_custom, "Custom…").clicked() {
                        b.sep = Separator::Custom(self.custom_sep.clone());
                        self.sep_custom = true;
                    }
                });
                if matches!(b.sep, Separator::Custom(_)) {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.custom_sep)
                            .id_salt("custom-sep-text")
                            .desired_width(48.0)
                            .char_limit(5),
                    );
                    if r.changed() {
                        custom_sep_edit = true;
                        b.sep = Separator::Custom(self.custom_sep.clone());
                    }
                    if !tpl::valid_custom_separator(&self.custom_sep) {
                        ui.label(RichText::new("1–5 characters, not \\ / : * ? \" < > | { }").color(red()).small());
                    }
                }
            });
            if b != before || custom_sep_edit {
                let ok = match &b.sep {
                    Separator::Custom(t) => tpl::valid_custom_separator(t),
                    _ => true,
                };
                if ok {
                    self.s.folder_template = b.to_template();
                    self.builder_synced = self.s.folder_template.clone();
                    self.folder_build = true;
                }
                self.builder = b;
            }
        }

        if self.s.layout.folder_advanced {
            ui.horizontal_wrapped(|ui| {
                ui.label("Template");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.s.folder_template)
                        .id_salt("folder-template-text")
                        .desired_width(220.0)
                        .font(TextStyle::Monospace)
                        .hint_text(tpl::DEFAULT_TEMPLATE),
                );
                if r.changed() {
                    // matches a ready-made layout -> show that; else the
                    // controls follow if they can express it ("Custom" if not)
                    let t = self.s.folder_template.clone();
                    if tpl::LAYOUTS.iter().any(|(l, _)| *l == t) {
                        self.folder_build = false;
                    } else if FolderBuilder::from_template(&t).is_some() {
                        self.folder_build = true;
                    }
                }
                ui.label(RichText::new(tpl::TOKEN_LEGEND).weak().small());
            });
        }

        for w in tpl::check_template(&self.s.folder_template) {
            ui.label(RichText::new(format!("⚠ {w}")).color(amber()).small());
        }
    }

    // ----- step 4: live preview -----

    fn central(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("4").strong().color(accent(ui)));
            ui.heading("Live preview");
            ui.add_space(16.0);
            ui.selectable_value(&mut self.tab, Tab::Files, "Files");
            ui.selectable_value(&mut self.tab, Tab::Folders, "By destination");
            let n_err = self.run_report.as_ref().map(|r| r.errors.len()).unwrap_or(0)
                + self.undo_report.as_ref().map(|r| r.errors.len()).unwrap_or(0)
                + self.scan_errors.len();
            let res_label = if n_err > 0 { format!("Results / errors ({n_err})") } else { "Results / errors".into() };
            ui.selectable_value(&mut self.tab, Tab::Results, res_label);
            if self.busy_kind() == Some(JobKind::Preview) {
                ui.add(egui::Spinner::new());
            }
        });

        if self.plan.is_some() {
            let c = self.counts;
            ui.horizontal_wrapped(|ui| {
                chip(ui, "Files", c.total, accent(ui));
                chip(ui, "Renamed", c.renamed, green());
                chip(ui, "Moved", c.to_move - c.rename_only, green());
                chip(ui, "Name clash (suffix added)", c.collisions, amber());
                chip(ui, "Notes", c.warnings, amber());
                chip(ui, "Unchanged / sorted", c.already_sorted + c.unchanged, grey());
                chip(ui, "No date", c.no_date, amber());
                chip(ui, "Errors", c.errors, red());
                chip(ui, "New folders", c.new_folders, accent(ui));
            });
            if self.s.sort_into_folders {
                ui.label(RichText::new(format!("Date sources — {}", self.source_summary)).weak().small());
            }
        }

        match self.tab {
            Tab::Files => self.files_tab(ui),
            Tab::Folders => self.folders_tab(ui),
            Tab::Results => self.results_tab(ui),
        }
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        if self.plan.is_none() {
            empty_state(ui, self.run_report.is_some() || self.undo_report.is_some(), !self.rule_errors.is_empty());
            return;
        }
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter_text)
                    .hint_text("Filter by name or path…")
                    .desired_width(240.0),
            );
            if r.changed() {
                self.view_dirty = true;
            }
            let before = self.filter_status;
            egui::ComboBox::from_id_salt("fstatus").selected_text(self.filter_status.label()).show_ui(ui, |ui| {
                for f in StatusFilter::ALL {
                    ui.selectable_value(&mut self.filter_status, f, f.label());
                }
            });
            if before != self.filter_status {
                self.view_dirty = true;
            }
            if ui.button("Clear").clicked() {
                self.filter_text.clear();
                self.filter_status = StatusFilter::All;
                self.view_dirty = true;
            }
        });
        if self.view_dirty {
            self.rebuild_view();
        }
        let total = self.plan.as_ref().map(|p| p.items.len()).unwrap_or(0);
        let stale = !self.plan_is_current();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Showing {} of {} files", self.view.len(), total)).weak().small());
            if stale {
                ui.label(RichText::new("(updating…)").color(amber()).small());
            }
        });

        let plan = self.plan.as_ref().unwrap();
        let root = plan.root.clone();
        let row_h = self.s.font_size + 10.0;
        let mut clicked: Option<SortCol> = None;
        let sort = self.sort;
        let header = |ui: &mut egui::Ui, col: SortCol, title: &str, clicked: &mut Option<SortCol>| {
            let arrow = if sort.0 == col { if sort.1 { " ⏶" } else { " ⏷" } } else { "" };
            if ui.add(egui::Button::new(RichText::new(format!("{title}{arrow}")).strong()).frame(false)).clicked() {
                *clicked = Some(col);
            }
        };
        let view = &self.view;
        let fs = self.s.font_size;
        let saved = self.s.layout.files_columns.clone();
        let w = |i: usize, d: f32| saved.get(i).copied().filter(|_| saved.len() == 6).unwrap_or(d);
        // left edges of the 8 header cells -> widths of the 6 resizable columns
        let mut lefts = [0.0f32; 8];
        TableBuilder::new(ui)
            .id_salt(("files", self.layout_gen))
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(44.0))
            .column(Column::initial(w(0, fs * 13.0)).at_least(80.0).clip(true))
            .column(Column::initial(w(1, fs * 17.0)).at_least(80.0).clip(true))
            .column(Column::initial(w(2, fs * 8.0)).at_least(60.0).clip(true))
            .column(Column::initial(w(3, fs * 11.0)).at_least(70.0).clip(true))
            .column(Column::initial(w(4, fs * 7.5)).at_least(60.0).clip(true))
            .column(Column::initial(w(5, fs * 7.0)).at_least(60.0).clip(true))
            .column(Column::remainder().at_least(60.0).clip(true))
            .header(row_h + 4.0, |mut h| {
                lefts[0] = h
                    .col(|ui| {
                        ui.label(RichText::new("#").strong());
                    })
                    .1
                    .rect
                    .left();
                lefts[1] = h.col(|ui| header(ui, SortCol::File, "Before (current name)", &mut clicked)).1.rect.left();
                lefts[2] = h.col(|ui| header(ui, SortCol::NewName, "After (new name)", &mut clicked)).1.rect.left();
                lefts[3] = h.col(|ui| header(ui, SortCol::Current, "Current folder", &mut clicked)).1.rect.left();
                lefts[4] = h.col(|ui| header(ui, SortCol::Dest, "Destination folder", &mut clicked)).1.rect.left();
                lefts[5] = h.col(|ui| header(ui, SortCol::Date, "Date used", &mut clicked)).1.rect.left();
                lefts[6] = h.col(|ui| header(ui, SortCol::Status, "Action", &mut clicked)).1.rect.left();
                lefts[7] = h
                    .col(|ui| {
                        ui.label(RichText::new("Notes").strong());
                    })
                    .1
                    .rect
                    .left();
            })
            .body(|body| {
                body.rows(row_h, view.len(), |mut row| {
                    let idx = row.index();
                    let it = &plan.items[view[idx]];
                    let name = it.original_name();
                    row.col(|ui| {
                        ui.label(RichText::new((idx + 1).to_string()).weak().small());
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(&name).truncate()).on_hover_text(it.source.display().to_string());
                    });
                    row.col(|ui| {
                        if it.status == Status::Move {
                            let new = it.final_name();
                            let rt = if it.renamed_for_collision {
                                RichText::new(&new).color(amber())
                            } else if new != name {
                                RichText::new(&new).color(green()).strong()
                            } else {
                                RichText::new(&new).weak()
                            };
                            let r = ui.add(egui::Label::new(rt).truncate());
                            if it.renamed_for_collision {
                                r.on_hover_text("A file with this name already exists or is planned; a suffix was added so nothing is overwritten.");
                            } else {
                                r.on_hover_text(it.dest_path.display().to_string());
                            }
                        } else {
                            ui.label(RichText::new("—").weak());
                        }
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(rel(&root, &it.current_dir)).truncate())
                            .on_hover_text(it.current_dir.display().to_string());
                    });
                    row.col(|ui| {
                        let txt = match it.status {
                            Status::Move if it.is_rename_only() => "(same folder)".to_string(),
                            Status::Move => rel(&root, &it.dest_dir),
                            Status::AlreadySorted | Status::Unchanged => "— (stays)".into(),
                            _ => "—".into(),
                        };
                        let r = ui.add(egui::Label::new(txt).truncate());
                        if it.status == Status::Move {
                            r.on_hover_text(it.dest_dir.display().to_string());
                        }
                    });
                    row.col(|ui| {
                        let txt = match it.date {
                            Some(d) => match d.datetime {
                                Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
                                None => d.date.format("%Y-%m-%d").to_string(),
                            },
                            None => "—".into(),
                        };
                        let r = ui.label(txt);
                        if let Some(d) = it.date {
                            r.on_hover_text(format!(
                                "Source: {}{}",
                                d.source.label(),
                                if d.via_fallback { " (fallback)" } else { "" }
                            ));
                        }
                    });
                    row.col(|ui| {
                        let r = ui.label(
                            RichText::new(status_text(it)).color(status_color(&it.status, it.renamed_for_collision)),
                        );
                        if let Status::Error(e) = &it.status {
                            r.on_hover_text(e);
                        }
                    });
                    row.col(|ui| {
                        if let Some(first) = it.warnings.first() {
                            let more = if it.warnings.len() > 1 { format!(" (+{})", it.warnings.len() - 1) } else { String::new() };
                            ui.add(egui::Label::new(RichText::new(format!("⚠ {first}{more}")).color(amber())).truncate())
                                .on_hover_text(it.warnings.join("\n"));
                        }
                    });
                });
            });
        if let Some(c) = clicked {
            self.sort = if self.sort.0 == c { (c, !self.sort.1) } else { (c, true) };
            self.view_dirty = true;
        }
        let sp = ui.spacing().item_spacing.x;
        let widths: Vec<f32> = (1..7).map(|i| lefts[i + 1] - lefts[i] - sp).collect();
        if widths.iter().all(|w| *w >= 20.0) {
            let v: Vec<f32> = widths.iter().map(|w| w.round()).collect();
            if v != self.s.layout.files_columns {
                self.s.layout.files_columns = v;
            }
        }
    }

    fn folders_tab(&mut self, ui: &mut egui::Ui) {
        let Some(plan) = &self.plan else {
            empty_state(ui, false, false);
            return;
        };
        let root = plan.root.clone();
        let rows = &self.folder_rows;
        ui.label(RichText::new(format!("{} destination folders", rows.len())).weak().small());
        let row_h = self.s.font_size + 10.0;
        let saved = &self.s.layout.folders_columns;
        let w = |i: usize, d: f32| saved.get(i).copied().filter(|_| saved.len() == 2).unwrap_or(d);
        let mut lefts = [0.0f32; 3];
        TableBuilder::new(ui)
            .id_salt(("folders", self.layout_gen))
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(w(0, 460.0)).at_least(120.0).clip(true))
            .column(Column::initial(w(1, 90.0)))
            .column(Column::remainder())
            .header(row_h + 4.0, |mut h| {
                lefts[0] = h
                    .col(|ui| {
                        ui.strong("Destination folder");
                    })
                    .1
                    .rect
                    .left();
                lefts[1] = h
                    .col(|ui| {
                        ui.strong("Files");
                    })
                    .1
                    .rect
                    .left();
                lefts[2] = h
                    .col(|ui| {
                        ui.strong("Folder");
                    })
                    .1
                    .rect
                    .left();
            })
            .body(|body| {
                body.rows(row_h, rows.len(), |mut row| {
                    let (dir, n, new) = &rows[row.index()];
                    row.col(|ui| {
                        ui.add(egui::Label::new(rel(&root, dir)).truncate()).on_hover_text(dir.display().to_string());
                    });
                    row.col(|ui| {
                        ui.label(n.to_string());
                    });
                    row.col(|ui| {
                        if *new {
                            ui.label(RichText::new("will be created").color(green()));
                        } else {
                            ui.label(RichText::new("exists").weak());
                        }
                    });
                });
            });
        let sp = ui.spacing().item_spacing.x;
        let widths = [lefts[1] - lefts[0] - sp, lefts[2] - lefts[1] - sp];
        if widths.iter().all(|w| *w >= 20.0) {
            let v: Vec<f32> = widths.iter().map(|w| w.round()).collect();
            if v != self.s.layout.folders_columns {
                self.s.layout.folders_columns = v;
            }
        }
    }

    fn results_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut any = false;
            if let Some(r) = &self.run_report {
                any = true;
                ui.strong("Last apply");
                ui.horizontal_wrapped(|ui| {
                    chip(ui, "Renamed / moved", r.moved, green());
                    chip(ui, "Skipped", r.skipped, grey());
                    chip(ui, "Errors", r.errors.len(), red());
                    if r.copied_across_devices > 0 {
                        chip(ui, "Copied across drives", r.copied_across_devices, amber());
                    }
                });
                if r.cancelled {
                    ui.label(
                        RichText::new("Run was cancelled part-way; the changes done so far are in the undo log.")
                            .color(amber()),
                    );
                }
                if let Some(p) = &r.undo_log {
                    ui.label(format!("Undo log: {}", p.display()));
                }
                error_list(ui, &r.errors);
                ui.add_space(10.0);
            }
            if let Some(r) = &self.undo_report {
                any = true;
                ui.strong("Last undo");
                ui.horizontal_wrapped(|ui| {
                    chip(ui, "Restored", r.restored, green());
                    chip(ui, "Empty folders removed", r.dirs_removed, grey());
                    chip(ui, "Problems", r.errors.len(), red());
                });
                if !r.errors.is_empty() {
                    ui.label("Entries that could not be undone were kept in the log; fix them and click Undo again.");
                }
                error_list(ui, &r.errors);
                ui.add_space(10.0);
            }
            if any {
                ui.label(RichText::new("Click “Refresh preview” to re-scan the folder.").weak());
            }
            if !self.scan_errors.is_empty() {
                any = true;
                ui.strong("Scan problems");
                error_list(ui, &self.scan_errors);
            }
            if let Some(plan) = &self.plan {
                let errs: Vec<(PathBuf, String)> = plan
                    .items
                    .iter()
                    .filter_map(|i| match &i.status {
                        Status::Error(e) => Some((i.source.clone(), e.clone())),
                        _ => None,
                    })
                    .collect();
                if !errs.is_empty() {
                    any = true;
                    ui.strong("Preview errors");
                    error_list(ui, &errs);
                }
            }
            if !any {
                ui.label(RichText::new("No results yet.").weak());
            }
        });
    }

    fn confirm_dialogs(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let mut close = false;
        let mut go = false;
        let modal = egui::Modal::new(egui::Id::unique("photosorter-confirm")).show(ctx, |ui| {
            ui.set_width(480.0);
            match c {
                Confirm::Apply => {
                    let n = self.counts;
                    ui.heading("Apply changes?");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "{} files will change: {} get a new name, {} move into {} destination folders ({} new).",
                        n.to_move,
                        n.renamed,
                        n.to_move - n.rename_only,
                        n.dest_folders,
                        n.new_folders
                    ));
                    if n.warnings > 0 {
                        ui.label(RichText::new(format!("{} file(s) have notes in the preview (⚠).", n.warnings)).color(amber()));
                    }
                    ui.label("Nothing is overwritten or deleted; name clashes get “ (1)”, “ (2)”…");
                    ui.label("An undo log is written to the chosen folder first, so Undo can restore every original name and location.");
                }
                Confirm::Undo { moves, started, log } => {
                    ui.heading("Undo last run?");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "Restore {moves} files to their original names and folders (run started {started})."
                    ));
                    ui.label("Only empty folders created by that run are removed. Nothing else is deleted.");
                    ui.label(RichText::new(log.display().to_string()).weak().small());
                }
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match c {
                    Confirm::Apply => "Apply",
                    Confirm::Undo { .. } => "Undo",
                };
                if ui.add(egui::Button::new(RichText::new(label).strong().color(Color32::WHITE)).fill(green())).clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        if go {
            match self.confirm.take() {
                Some(Confirm::Apply) => {
                    if self.plan_is_current() {
                        self.start_apply(ctx)
                    } else {
                        self.status = "The preview changed; please review it and click Apply again.".into();
                    }
                }
                Some(Confirm::Undo { log, .. }) => self.start_undo(ctx, log),
                None => {}
            }
        } else if close {
            self.confirm = None;
        }
    }
}

impl App {
    /// Tools > Options: grouped sections, Save (apply + write PhotoSorter.ini)
    /// or Close (discard). Modal, so the main window cannot change underneath.
    fn options_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.options.take() else { return };
        let locked = matches!(self.busy_kind(), Some(JobKind::Apply | JobKind::Undo));
        let current = OptionsDraft { extensions: self.s.extensions.clone(), order_ui: self.order_ui.clone() };
        let mut save = false;
        let mut close = false;
        let mut reset_layout = false;
        let mut theme = self.s.theme;
        let mut font_size = self.s.font_size;
        let fs = self.s.font_size;
        let modal = egui::Modal::new(egui::Id::unique("photosorter-options")).show(ctx, |ui| {
            ui.set_width((fs * 40.0).min(ctx.content_rect().width() - 60.0));
            ui.heading("Options");
            ui.add_space(6.0);

            options_group(ui, "Appearance", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    for t in Theme::ALL {
                        ui.radio_value(&mut theme, t, t.label()).on_hover_text(match t {
                            Theme::System => "Use the Windows light/dark app colour setting",
                            _ => "",
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Text size");
                    ui.add(egui::Slider::new(&mut font_size, 10.0..=28.0).step_by(1.0).suffix(" pt"));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("Reset window layout")
                        .on_hover_text(
                            "Default window size, centered, not maximized; default panel height, column widths \
                             and expanded sections",
                        )
                        .clicked()
                    {
                        reset_layout = true;
                    }
                    ui.label(
                        RichText::new("Appearance applies at once and is saved automatically.").weak().small(),
                    );
                });
            });
            ui.add_space(6.0);

            options_group(ui, "File types", |ui| {
                ui.label(
                    "Extensions to process, separated by commas (case-insensitive). Only image, camera RAW and \
                     video types are accepted; anything else is ignored and never touched.",
                );
                ui.add(
                    egui::TextEdit::multiline(&mut draft.extensions)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("e.g. jpg, png, heic, mp4"),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(draft.extensions != scan::DEFAULT_EXTENSIONS, egui::Button::new("Reset to defaults"))
                        .on_hover_text(scan::DEFAULT_EXTENSIONS)
                        .clicked()
                    {
                        draft.extensions = scan::DEFAULT_EXTENSIONS.to_string();
                    }
                    ui.label(RichText::new(format!("Uses: {}", scan::extension_summary(&draft.extensions))).weak());
                });
                let rejected = scan::rejected_extensions(&draft.extensions);
                if !rejected.is_empty() {
                    ui.label(
                        RichText::new(format!("Ignored (not image/video, never touched): {}", rejected.join(", ")))
                            .color(amber())
                            .small(),
                    );
                }
            });
            ui.add_space(6.0);

            options_group(ui, "Date detection", |ui| {
                ui.label("Auto priority — used when Date source is “Auto”, and by the “Insert date” rule. First match wins; untick to skip a source.");
                let n = draft.order_ui.len();
                let enabled_count = draft.order_ui.iter().filter(|(_, on)| *on).count();
                let mut swap: Option<(usize, usize)> = None;
                for i in 0..n {
                    let (kind, on) = draft.order_ui[i];
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", i + 1)).weak());
                        if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).on_hover_text("Earlier").clicked() {
                            swap = Some((i, i - 1));
                        }
                        if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).on_hover_text("Later").clicked() {
                            swap = Some((i, i + 1));
                        }
                        let mut on2 = on;
                        let can_toggle = !(on && enabled_count == 1);
                        if ui.add_enabled(can_toggle, egui::Checkbox::new(&mut on2, kind_desc(kind))).changed() {
                            draft.order_ui[i].1 = on2;
                        }
                    });
                }
                if let Some((a, b)) = swap {
                    draft.order_ui.swap(a, b);
                }
            });

            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dirty = draft != current;
                let btn = egui::Button::new(RichText::new("Save").strong().color(Color32::WHITE)).fill(green());
                let r = ui.add_enabled(dirty && !locked, btn).on_hover_text(format!("Apply and save to {}", settings::INI_NAME));
                let r = if locked { r.on_disabled_hover_text("Wait until Apply / Undo has finished") } else { r };
                if r.clicked() {
                    save = true;
                }
                let close_label = if dirty { "Close without saving" } else { "Close" };
                if ui.button(close_label).clicked() {
                    close = true;
                }
                if dirty {
                    ui.label(RichText::new("Unsaved changes").color(amber()).small());
                }
            });
        });
        // appearance is live (not part of the Save / Close draft)
        self.s.theme = theme;
        self.s.font_size = font_size.round().clamp(10.0, 28.0);
        if reset_layout {
            self.reset_layout(ctx);
        }
        if save {
            self.s.extensions = draft.extensions.trim().to_string();
            self.order_ui = draft.order_ui;
            self.sync_order();
            self.save_settings();
            self.status = format!("Options saved to {}.", settings::INI_NAME);
            return;
        }
        if close || modal.should_close() {
            return;
        }
        self.options = Some(draft);
    }
}

impl App {
    /// Presets > Manage Presets: list on the left, details + actions on the
    /// right. Every change is written to PhotoSorter.ini at once.
    fn manage_presets_window(&mut self, ctx: &egui::Context) {
        if !self.manage_open {
            return;
        }
        let mut open = true;
        let locked = self.locked();
        let fs = self.s.font_size;
        egui::Window::new("Manage Presets")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(fs * 48.0)
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(ctx.content_rect().center_top() + Vec2::new(0.0, fs * 9.0))
            .show(ctx, |ui| {
                if self.s.presets.is_empty() {
                    ui.label("No presets yet. Set up rules / options, then use Presets > Save current as preset…");
                    return;
                }
                if self.s.find_preset(&self.manage_sel).is_none() {
                    self.manage_sel = self.s.presets[0].name.clone();
                    self.manage_rename = self.manage_sel.clone();
                    self.manage_error = None;
                }
                ui.add_enabled_ui(!locked, |ui| {
                    ui.horizontal_top(|ui| {
                        // list
                        ui.vertical(|ui| {
                            ui.set_width(fs * 14.0);
                            ui.label(RichText::new(format!("{} presets", self.s.presets.len())).weak());
                            egui::ScrollArea::vertical().id_salt("preset-list").max_height(fs * 24.0).show(ui, |ui| {
                                let names: Vec<String> = self.s.presets.iter().map(|p| p.name.clone()).collect();
                                for n in names {
                                    let label = if self.editing_preset.as_deref() == Some(n.as_str()) {
                                        format!("✏ {n}")
                                    } else {
                                        n.clone()
                                    };
                                    if ui.selectable_label(self.manage_sel == n, label).clicked() {
                                        self.manage_rename = n.clone();
                                        self.manage_sel = n;
                                        self.manage_error = None;
                                    }
                                }
                            });
                        });
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.set_width(fs * 33.0);
                            self.manage_details(ui)
                        });
                    });
                });
                ui.separator();
                ui.label(
                    RichText::new(
                        "Stored as [Preset:Name] sections in PhotoSorter.ini. Every change here is saved at once; the previous file is kept as PhotoSorter.ini.bak.",
                    )
                    .weak()
                    .small(),
                );
            });
        if !open {
            self.manage_open = false;
        }
    }

    fn manage_details(&mut self, ui: &mut egui::Ui) {
        let Some(p) = self.s.find_preset(&self.manage_sel).cloned() else { return };
        ui.heading(&p.name);
        ui.horizontal(|ui| {
            ui.label("Loads");
            let mut scope = p.scope;
            egui::ComboBox::from_id_salt("preset-scope").width(220.0).selected_text(scope.label()).show_ui(ui, |ui| {
                for sc in PresetScope::ALL {
                    ui.selectable_value(&mut scope, sc, sc.label());
                }
            });
            if scope != p.scope {
                self.s.set_preset_scope(&p.name, scope);
                self.save_settings();
                self.status = format!("Preset “{}” now loads: {}.", p.name, scope.label().to_lowercase());
            }
        });
        egui::Frame::new()
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().id_salt("preset-view").max_height(self.s.font_size * 14.0).show(
                    ui,
                    |ui| {
                        for line in session::preset_summary(&p) {
                            ui.label(line);
                        }
                    },
                );
            });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let editing = self.editing_preset.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&p.name));
            if editing {
                if ui
                    .add(
                        egui::Button::new(RichText::new("💾 Save changes").strong().color(Color32::WHITE))
                            .fill(green()),
                    )
                    .on_hover_text("Write the current rules + options from the main window back to this preset")
                    .clicked()
                {
                    self.save_editing_preset();
                }
                if ui.button("Stop editing").clicked() {
                    self.editing_preset = None;
                }
            } else if ui
                .button("✏ Edit")
                .on_hover_text("Load this preset into the main window; change it there, then “Save changes”")
                .clicked()
            {
                self.load_preset(&p.name);
                self.editing_preset = Some(p.name.clone());
            }
            if ui.button("📋 Duplicate").clicked()
                && let Some(n) = self.s.duplicate_preset(&p.name)
            {
                self.save_settings();
                self.status = format!("Duplicated preset as “{n}”.");
                self.manage_rename = n.clone();
                self.manage_sel = n;
                self.manage_error = None;
            }
            if ui.button("🗑 Delete…").clicked() {
                self.dialog = Some(Dialog::DeletePreset(p.name.clone()));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Rename to");
            let r = ui.add(egui::TextEdit::singleline(&mut self.manage_rename).desired_width(200.0));
            let check = self.s.validate_preset_name(&self.manage_rename, Some(&p.name));
            let changed = self.manage_rename.trim() != p.name;
            let go = ui.add_enabled(changed && check.is_ok(), egui::Button::new("Rename")).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && changed && check.is_ok());
            if go {
                match self.s.rename_preset(&p.name, &self.manage_rename) {
                    Ok(n) => {
                        if self.editing_preset.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&p.name)) {
                            self.editing_preset = Some(n.clone());
                        }
                        if self.preset_sel.eq_ignore_ascii_case(&p.name) {
                            self.preset_sel = n.clone();
                        }
                        self.save_settings();
                        self.status = format!("Renamed preset “{}” to “{n}”.", p.name);
                        self.manage_rename = n.clone();
                        self.manage_sel = n;
                        self.manage_error = None;
                    }
                    Err(e) => self.manage_error = Some(e),
                }
            } else {
                self.manage_error = if changed { check.err() } else { None };
            }
        });
        if let Some(e) = &self.manage_error {
            ui.label(RichText::new(format!("⚠ {e}")).color(red()).small());
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let mut keep = true;
        let fs = self.s.font_size;
        let modal = egui::Modal::new(egui::Id::unique("photosorter-dialog")).show(ctx, |ui| {
            ui.set_width(fs * 32.0);
            match &mut d {
                Dialog::NewProject(w) => {
                    ui.heading("Start a new project?");
                    ui.add_space(4.0);
                    ui.label(
                        "Clears the folder, the preview and the rename rules, and resets the output options to their defaults. \
                         No files are moved, renamed or deleted. Saved presets, file types and other settings are kept.",
                    );
                    for line in w.iter() {
                        ui.label(RichText::new(format!("⚠ {line}")).color(amber()));
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("New Project").strong().color(Color32::WHITE)).fill(green())).clicked() {
                            self.do_new_project();
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::SavePreset { name } => {
                    ui.heading("Save current as preset");
                    ui.add_space(4.0);
                    ui.label("Saves the rename rules and the options of steps 1 and 3 into PhotoSorter.ini.");
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text("preset name").desired_width(f32::INFINITY));
                    if !r.has_focus() && name.is_empty() {
                        r.request_focus();
                    }
                    let check = self.s.validate_preset_name(name, Some(name.trim()));
                    match &check {
                        Err(e) if !name.trim().is_empty() => {
                            ui.label(RichText::new(format!("⚠ {e}")).color(red()).small());
                        }
                        Ok(n) => {
                            if let Some(p) = self.s.find_preset(n) {
                                let text = format!("Replaces the existing preset “{}” (what it loads is kept).", p.name);
                                ui.label(RichText::new(text).color(amber()).small());
                            }
                        }
                        _ => {}
                    }
                    ui.add_space(8.0);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save = ui.add_enabled(check.is_ok(), egui::Button::new(RichText::new("Save").strong().color(Color32::WHITE)).fill(green())).clicked();
                        if (save || enter) && let Ok(n) = &check && self.s.save_preset(n) {
                            let n = self.s.find_preset(n).map(|p| p.name.clone()).unwrap_or_else(|| n.clone());
                            self.preset_sel = n.clone();
                            self.save_settings();
                            self.status = format!("Saved preset “{n}”.");
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::DeletePreset(name) => {
                    ui.heading("Delete preset?");
                    ui.add_space(4.0);
                    ui.label(format!(
                        "Delete the preset “{name}”? Its [Preset:{name}] section is removed from PhotoSorter.ini (the previous file is kept as PhotoSorter.ini.bak). No photos or videos are touched."
                    ));
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Delete").strong().color(Color32::WHITE)).fill(red())).clicked() {
                            let n = name.clone();
                            if self.s.delete_preset(&n) {
                                if self.preset_sel.eq_ignore_ascii_case(&n) {
                                    self.preset_sel.clear();
                                }
                                if self.editing_preset.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&n)) {
                                    self.editing_preset = None;
                                }
                                self.save_settings();
                                self.status = format!("Deleted preset “{n}”.");
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::ReadmeMissing(p) => {
                    ui.heading("README.txt not found");
                    ui.add_space(4.0);
                    ui.label("The help file README.txt ships in the PhotoSorter zip and should sit next to PhotoSorter.exe:");
                    ui.label(RichText::new(p.display().to_string()).monospace().small());
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                        if let Some(dir) = p.parent().filter(|d| d.is_dir()).map(Path::to_path_buf)
                            && ui.button("Open program folder").clicked()
                        {
                            keep = false;
                            if let Err(e) = desktop::open_folder(&dir) {
                                self.status = format!("Cannot open folder: {e}");
                            }
                        }
                    });
                }
                Dialog::About => {
                    ui.heading(format!("PhotoSorter {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(4.0);
                    ui.label("Portable photo & video renamer / sorter for Windows 10/11.");
                    ui.label("Unsigned build — Windows SmartScreen may warn on first run. MIT License.");
                    ui.label(RichText::new("A fuller About page will come in a later version.").weak().small());
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::Message { title, text } => {
                    ui.heading(title.as_str());
                    ui.add_space(4.0);
                    ui.label(text.as_str());
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            keep = false;
        }
        // an action may have opened another dialog; only restore if not
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        }
    }
}

fn options_group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().color(accent(ui)));
            ui.add_space(2.0);
            add(ui);
        });
}

fn year_label(y: YearStyle) -> &'static str {
    match y {
        YearStyle::Full => "2024",
        YearStyle::Short => "24",
    }
}

fn month_label(m: MonthStyle) -> &'static str {
    match m {
        MonthStyle::Name => "March",
        MonthStyle::Short => "Mar",
        MonthStyle::Num2 => "03",
        MonthStyle::Num => "3",
    }
}

fn order_label(o: PartOrder) -> &'static str {
    match o {
        PartOrder::MonthFirst => "Month first",
        PartOrder::YearFirst => "Year first",
    }
}

/// "Filename > EXIF / video > Modified" (enabled sources in order).
fn order_summary(order_ui: &[(SourceKind, bool)]) -> String {
    let v: Vec<&str> = order_ui.iter().filter(|(_, on)| *on).map(|(k, _)| kind_desc(*k)).collect();
    v.join(" > ")
}

fn kind_desc(k: SourceKind) -> &'static str {
    match k {
        SourceKind::Filename => "Filename",
        SourceKind::Exif => "EXIF / video",
        SourceKind::Modified => "Modified",
        SourceKind::Created => "Created",
    }
}

fn empty_state(ui: &mut egui::Ui, after_run: bool, rule_error: bool) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        if after_run {
            ui.label(RichText::new("Run finished — see “Results / errors”.").size(18.0));
            ui.label(RichText::new("Click “Refresh preview” to re-scan the folder.").weak());
        } else if rule_error {
            ui.label(RichText::new("A rule has an error").size(20.0).strong().color(red()));
            ui.label(RichText::new("Fix it in step 2 and the preview appears here.").weak());
        } else {
            ui.label(RichText::new("No preview yet").size(20.0).strong());
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "1. Choose or drop a folder   2. Add rename rules (optional)   3. Choose the output folder option",
                )
                .weak(),
            );
            ui.label(
                RichText::new("The preview updates live. Nothing changes until you click Apply and confirm.").weak(),
            );
        }
    });
}

fn error_list(ui: &mut egui::Ui, errors: &[(PathBuf, String)]) {
    if errors.is_empty() {
        return;
    }
    egui::Grid::new(ui.next_auto_id()).striped(true).num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        for (p, e) in errors.iter().take(2000) {
            ui.label(RichText::new(p.display().to_string()).monospace());
            ui.label(RichText::new(e).color(red()));
            ui.end_row();
        }
    });
    if errors.len() > 2000 {
        ui.label(format!("… and {} more", errors.len() - 2000));
    }
}
