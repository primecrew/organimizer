//! egui front-end. All real work happens in the `photo_sorter` library on a
//! background thread; this module only draws state and forwards actions.

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
use photo_sorter::scan;
use photo_sorter::settings::{self, Loaded, Settings, Theme};
use photo_sorter::template::format_folder_name;

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

enum Msg {
    Progress { text: String, frac: f32 },
    Planned { plan: Plan, scan_errors: Vec<(PathBuf, String)>, dirs: usize, cancelled: bool },
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

// ---------------------------------------------------------------------------
// Table state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortCol {
    File,
    Current,
    Dest,
    Date,
    Source,
    Status,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusFilter {
    All,
    Move,
    Sorted,
    NoDate,
    Error,
    Collision,
}

impl StatusFilter {
    const ALL: [StatusFilter; 6] = [
        StatusFilter::All,
        StatusFilter::Move,
        StatusFilter::Collision,
        StatusFilter::Sorted,
        StatusFilter::NoDate,
        StatusFilter::Error,
    ];
    fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All statuses",
            StatusFilter::Move => "Will move",
            StatusFilter::Collision => "Renamed (collision)",
            StatusFilter::Sorted => "Already sorted",
            StatusFilter::NoDate => "No date",
            StatusFilter::Error => "Errors",
        }
    }
    fn matches(self, it: &plan::PlanItem) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Move => it.status == Status::Move,
            StatusFilter::Collision => it.status == Status::Move && it.renamed_for_collision,
            StatusFilter::Sorted => it.status == Status::AlreadySorted,
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
    plan_sig: String,
    scan_errors: Vec<(PathBuf, String)>,
    dirs_scanned: usize,

    worker: Option<Worker>,
    progress_text: String,
    progress: f32,
    status: String,

    run_report: Option<RunReport>,
    undo_report: Option<UndoReport>,

    tab: Tab,
    filter_text: String,
    filter_status: StatusFilter,
    filter_source: Option<SourceKind>,
    sort: (SortCol, bool),
    view: Vec<usize>,
    view_dirty: bool,

    confirm: Option<Confirm>,
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
        App {
            order_ui,
            ini_path: loaded.path,
            notice: loaded.notice,
            plan: None,
            counts: Default::default(),
            folder_rows: vec![],
            source_summary: String::new(),
            plan_sig: String::new(),
            scan_errors: vec![],
            dirs_scanned: 0,
            worker: None,
            progress_text: String::new(),
            progress: 0.0,
            status: if s.last_folder.trim().is_empty() {
                "Choose a folder, then click Preview.".into()
            } else {
                "Ready — click “Preview (dry run)” to see what would happen.".into()
            },
            run_report: None,
            undo_report: None,
            tab: Tab::Files,
            filter_text: String::new(),
            filter_status: StatusFilter::All,
            filter_source: None,
            sort: (SortCol::Current, true),
            view: vec![],
            view_dirty: true,
            confirm: None,
            undo_cache: (String::from("\0"), None),
            applied_theme: None,
            applied_font: 0.0,
            s,
        }
    }

    fn root(&self) -> PathBuf {
        PathBuf::from(self.s.folder_trimmed())
    }

    fn plan_options(&self) -> PlanOptions {
        PlanOptions { template: self.s.folder_template.clone(), date: self.s.date.clone() }
    }

    /// Everything that influences the plan. If it changes, the preview is stale.
    fn signature(&self) -> String {
        format!(
            "{:?}|{}|{}|{:?}|{}",
            self.s.folder_trimmed(),
            self.s.include_subfolders,
            self.s.extensions,
            self.s.date,
            self.s.folder_template
        )
    }

    fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn plan_is_stale(&self) -> bool {
        self.plan.is_some() && self.plan_sig != self.signature()
    }

    fn save_settings(&mut self) {
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

    // ----- actions -----

    fn start_preview(&mut self, ctx: &egui::Context) {
        let root = self.root();
        if !root.is_dir() {
            self.status = "Please choose an existing folder first.".into();
            return;
        }
        self.save_settings();
        let recursive = self.s.include_subfolders;
        let exts = scan::parse_extensions(&self.s.extensions);
        let opts = self.plan_options();
        self.plan_sig = self.signature();
        self.run_report = None;
        self.undo_report = None;
        self.progress = 0.0;
        self.status = "Scanning…".into();
        self.worker = Some(spawn(ctx, JobKind::Preview, move |tx, cancel, ctx| {
            let mut t = Ticker::new();
            let res = scan::scan(&root, recursive, &exts, &cancel, &mut |n, dir| {
                if t.due() {
                    let _ = tx.send(Msg::Progress {
                        text: format!("Scanning… {n} images found — {}", dir.display()),
                        frac: 0.0,
                    });
                    ctx.request_repaint();
                }
            });
            let mut t = Ticker::new();
            let plan = plan::build_plan(&root, &res.files, &opts, &plan::default_stages(), &cancel, &mut |i, n| {
                if t.due() {
                    let _ = tx.send(Msg::Progress {
                        text: format!("Reading dates… {i} / {n}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Planned {
                plan,
                scan_errors: res.errors,
                dirs: res.dirs_scanned,
                cancelled: cancel.load(Ordering::Relaxed),
            });
        }));
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.plan.clone() else { return };
        self.status = "Moving files…".into();
        self.progress = 0.0;
        self.worker = Some(spawn(ctx, JobKind::Apply, move |tx, cancel, ctx| {
            let mut t = Ticker::new();
            let rep = ops::execute(&plan, &cancel, &mut |i, n, p| {
                if t.due() {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let _ = tx.send(Msg::Progress {
                        text: format!("Moving {i} / {n}  {name}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Applied(rep));
        }));
    }

    fn start_undo(&mut self, ctx: &egui::Context, log: PathBuf) {
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
                Msg::Planned { plan, scan_errors, dirs, cancelled } => {
                    let c = plan.counts();
                    self.status = if cancelled {
                        "Preview cancelled (partial results shown; Apply is disabled).".into()
                    } else {
                        format!(
                            "Preview ready: {} images in {} folders — {} to move, {} already sorted, {} without date. Nothing has been changed yet.",
                            c.total, dirs, c.to_move, c.already_sorted, c.no_date
                        )
                    };
                    if cancelled {
                        self.plan_sig = "cancelled".into();
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
                    self.scan_errors = scan_errors;
                    self.dirs_scanned = dirs;
                    self.view_dirty = true;
                    self.tab = Tab::Files;
                    finished = true;
                }
                Msg::Applied(rep) => {
                    self.status = format!(
                        "Done: {} moved, {} errors{}. {}",
                        rep.moved,
                        rep.errors.len(),
                        if rep.cancelled { " (cancelled)" } else { "" },
                        rep.undo_log.as_ref().map(|p| format!("Undo log: {}", p.display())).unwrap_or_default()
                    );
                    self.run_report = Some(rep);
                    self.plan = None;
                    self.view.clear();
                    self.tab = Tab::Results;
                    self.invalidate_undo_cache();
                    finished = true;
                }
                Msg::Undone(rep) => {
                    self.status = format!(
                        "Undo: {} files restored, {} empty folders removed, {} problems.",
                        rep.restored,
                        rep.dirs_removed,
                        rep.errors.len()
                    );
                    self.undo_report = Some(rep);
                    self.plan = None;
                    self.view.clear();
                    self.tab = Tab::Results;
                    self.invalidate_undo_cache();
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
            .filter(|(_, it)| match self.filter_source {
                None => true,
                Some(k) => it.date.map(|d| d.source) == Some(k),
            })
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
                SortCol::Current => x.current_dir.cmp(&y.current_dir).then_with(|| name(a).cmp(&name(b))),
                SortCol::Dest => x.dest_path.cmp(&y.dest_path),
                SortCol::Date => x.date.map(|d| (d.date, d.datetime)).cmp(&y.date.map(|d| (d.date, d.datetime))),
                SortCol::Source => x.date.map(|d| d.source).cmp(&y.date.map(|d| d.source)),
                SortCol::Status => x.status.label().cmp(y.status.label()),
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
            Theme::Dark => egui::Theme::Dark,
            Theme::Light => egui::Theme::Light,
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

fn section<R>(ui: &mut egui::Ui, num: &str, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let fill = ui.visuals().faint_bg_color;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(num).strong().color(accent(ui)));
                ui.label(RichText::new(title).strong().size(ui.style().text_styles[&TextStyle::Body].size * 1.12));
            });
            ui.add_space(4.0);
            add(ui)
        })
        .inner
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

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_style(&ctx);
        self.poll_worker();
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // remember window size
        if let Some(r) = ctx.input(|i| i.viewport().inner_rect)
            && r.width() >= 640.0
            && r.height() >= 420.0
        {
            self.s.window_width = r.width();
            self.s.window_height = r.height();
        }

        // drag & drop a folder (or a file inside it)
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.first()
            && !self.busy()
        {
            self.set_folder(p);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        self.top_bar(ui);
        self.status_bar(ui);
        self.side_panel(ui, &ctx);
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
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("📷 PhotoSorter").heading().strong());
                ui.label(RichText::new("sort images into “Month YYYY” folders").weak());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (icon, tip, next) = match self.s.theme {
                        Theme::Dark => ("☀ Light", "Switch to light theme", Theme::Light),
                        Theme::Light => ("🌙 Dark", "Switch to dark theme", Theme::Dark),
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

    fn side_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::left("side").resizable(true).default_size(self.s.font_size * 24.0).show(ui, |ui| {
            // Actions stay pinned at the bottom so they are always visible.
            egui::Panel::bottom("actions")
                .frame(egui::Frame::NONE.inner_margin(Margin { left: 0, right: 0, top: 8, bottom: 8 }))
                .show_separator_line(false)
                .show(ui, |ui| self.actions_section(ui, ctx));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                let busy = self.busy();
                ui.add_enabled_ui(!busy, |ui| {
                    self.source_section(ui);
                    ui.add_space(8.0);
                    self.options_section(ui);
                });
                ui.add_space(4.0);
            });
        });
    }

    fn source_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "1", "Source folder", |ui| {
            ui.horizontal(|ui| {
                if ui.button("📂 Browse…").clicked() {
                    let mut dlg = rfd::FileDialog::new().set_title("Choose a folder with images");
                    let cur = self.root();
                    if cur.is_dir() {
                        dlg = dlg.set_directory(&cur);
                    }
                    if let Some(p) = dlg.pick_folder() {
                        self.set_folder(&p);
                    }
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
            ui.checkbox(&mut self.s.include_subfolders, "Include subfolders")
                .on_hover_text("Every folder containing images gets its own Month YYYY subfolders.");
            ui.horizontal(|ui| {
                ui.label("Extensions");
                ui.add(egui::TextEdit::singleline(&mut self.s.extensions).desired_width(f32::INFINITY))
                    .on_hover_text("Comma-separated, case-insensitive");
            });
        });
    }

    fn options_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "2", "Options", |ui| {
            egui::Grid::new("opts").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                ui.label("Date source");
                egui::ComboBox::from_id_salt("mode").width(220.0).selected_text(self.s.date.mode.label()).show_ui(
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
                ui.end_row();

                ui.label("If no date found");
                egui::ComboBox::from_id_salt("fallback")
                    .width(220.0)
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

                ui.label("Folder name");
                ui.add(egui::TextEdit::singleline(&mut self.s.folder_template).desired_width(220.0))
                    .on_hover_text("Tokens: {MMMM} {MMM} {MM} {M} {YYYY} {YY} {DD}");
                ui.end_row();

                ui.label("");
                let example = format_folder_name(&self.s.folder_template, chrono::Local::now().date_naive());
                ui.label(RichText::new(format!("e.g. “{example}”")).weak().small());
                ui.end_row();
            });

            if self.s.date.mode == DateMode::Auto {
                ui.add_space(4.0);
                ui.label(RichText::new("Auto priority (first match wins)").strong());
                let mut changed = false;
                let mut swap: Option<(usize, usize)> = None;
                let n = self.order_ui.len();
                let enabled_count = self.order_ui.iter().filter(|(_, on)| *on).count();
                for i in 0..n {
                    let (kind, on) = self.order_ui[i];
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", i + 1)).weak());
                        let mut on2 = on;
                        let can_toggle = !(on && enabled_count == 1);
                        let cb = ui.add_enabled(can_toggle, egui::Checkbox::new(&mut on2, kind_desc(kind)));
                        let cb = if kind == SourceKind::Filename {
                            cb.on_hover_text("e.g. IMG_20240315_142530, 2024-03-15, Screenshot_20240315-…, IMG-20240315-WA0001, 1710512345")
                        } else {
                            cb
                        };
                        if cb.changed() {
                            self.order_ui[i].1 = on2;
                            changed = true;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).clicked() {
                                swap = Some((i, i + 1));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() {
                                swap = Some((i, i - 1));
                            }
                        });
                    });
                }
                if let Some((a, b)) = swap {
                    self.order_ui.swap(a, b);
                    changed = true;
                }
                if changed {
                    self.sync_order();
                }
            }
        });
    }

    fn actions_section(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        section(ui, "3", "Actions", |ui| {
            let busy = self.busy();
            let full = Vec2::new(ui.available_width(), self.s.font_size + 16.0);
            if busy {
                let label = match self.worker.as_ref().map(|w| w.kind) {
                    Some(JobKind::Undo) => "Undo running…",
                    _ => "⏹ Cancel",
                };
                let can_cancel = self.worker.as_ref().map(|w| w.kind != JobKind::Undo).unwrap_or(false);
                if ui.add_enabled(can_cancel, egui::Button::new(label).min_size(full)).clicked()
                    && let Some(w) = &self.worker
                {
                    w.cancel.store(true, Ordering::Relaxed);
                }
                return;
            }

            if ui
                .add(egui::Button::new(RichText::new("🔍 Preview (dry run)").strong()).min_size(full))
                .on_hover_text("Scan and show what would happen. Nothing is changed.")
                .clicked()
            {
                self.start_preview(ctx);
            }

            let to_move = if self.plan.is_some() { self.counts.to_move } else { 0 };
            let stale = self.plan_is_stale();
            let can_apply = self.plan.is_some() && !stale && to_move > 0;
            let apply_text =
                if to_move > 0 { format!("✔ Apply — move {to_move} files") } else { "✔ Apply".to_string() };
            let btn = if can_apply {
                egui::Button::new(RichText::new(apply_text).strong().color(Color32::WHITE))
                    .fill(green().gamma_multiply(0.9))
                    .min_size(full)
            } else {
                egui::Button::new(RichText::new(apply_text)).min_size(full)
            };
            if ui.add_enabled(can_apply, btn).clicked() {
                self.confirm = Some(Confirm::Apply);
            }
            if stale {
                ui.label(
                    RichText::new("Options changed since the preview — run Preview again.").color(amber()).small(),
                );
            } else if self.plan.is_none() {
                ui.label(RichText::new("Run a preview first; Apply is enabled afterwards.").weak().small());
            }

            ui.add_space(4.0);
            let log = self.undo_log_for_root();
            let undo_btn = egui::Button::new("↩ Undo last run").min_size(full);
            let r = ui.add_enabled(log.is_some(), undo_btn);
            let r = match &log {
                Some(p) => r.on_hover_text(format!("Uses {}", p.display())),
                None => r.on_disabled_hover_text("No undo log (PhotoSorter-undo-*.json) in this folder"),
            };
            if r.clicked()
                && let Some(p) = log
            {
                match ops::read_log(&p) {
                    Ok(l) => self.confirm = Some(Confirm::Undo { moves: l.moves.len(), started: l.started, log: p }),
                    Err(e) => self.status = format!("Cannot read undo log: {e}"),
                }
            }
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Preview");
            ui.add_space(16.0);
            ui.selectable_value(&mut self.tab, Tab::Files, "Files");
            ui.selectable_value(&mut self.tab, Tab::Folders, "By destination");
            let n_err = self.run_report.as_ref().map(|r| r.errors.len()).unwrap_or(0)
                + self.undo_report.as_ref().map(|r| r.errors.len()).unwrap_or(0)
                + self.scan_errors.len();
            let res_label = if n_err > 0 { format!("Results / errors ({n_err})") } else { "Results / errors".into() };
            ui.selectable_value(&mut self.tab, Tab::Results, res_label);
        });
        ui.separator();

        if self.plan.is_some() {
            let c = self.counts;
            ui.horizontal_wrapped(|ui| {
                chip(ui, "Images", c.total, accent(ui));
                chip(ui, "To move", c.to_move, green());
                chip(ui, "Renamed", c.collisions, amber());
                chip(ui, "Already sorted", c.already_sorted, grey());
                chip(ui, "No date", c.no_date, amber());
                chip(ui, "Errors", c.errors, red());
                chip(ui, "New folders", c.new_folders, accent(ui));
            });
            ui.label(RichText::new(format!("Date sources — {}", self.source_summary)).weak());
            ui.add_space(4.0);
        }

        match self.tab {
            Tab::Files => self.files_tab(ui),
            Tab::Folders => self.folders_tab(ui),
            Tab::Results => self.results_tab(ui),
        }
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        if self.plan.is_none() {
            empty_state(ui, self.run_report.is_some() || self.undo_report.is_some());
            return;
        }
        // filters
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter_text)
                    .hint_text("Filter by name or path…")
                    .desired_width(240.0),
            );
            if r.changed() {
                self.view_dirty = true;
            }
            let before = (self.filter_status, self.filter_source);
            egui::ComboBox::from_id_salt("fstatus").selected_text(self.filter_status.label()).show_ui(ui, |ui| {
                for f in StatusFilter::ALL {
                    ui.selectable_value(&mut self.filter_status, f, f.label());
                }
            });
            let src_label = self.filter_source.map(|k| k.label()).unwrap_or("All sources");
            egui::ComboBox::from_id_salt("fsource").selected_text(src_label).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filter_source, None, "All sources");
                for k in SourceKind::ALL {
                    ui.selectable_value(&mut self.filter_source, Some(k), k.label());
                }
            });
            if before != (self.filter_status, self.filter_source) {
                self.view_dirty = true;
            }
            if ui.button("Clear").clicked() {
                self.filter_text.clear();
                self.filter_status = StatusFilter::All;
                self.filter_source = None;
                self.view_dirty = true;
            }
        });
        if self.view_dirty {
            self.rebuild_view();
        }
        let total = self.plan.as_ref().map(|p| p.items.len()).unwrap_or(0);
        ui.label(RichText::new(format!("Showing {} of {} files", self.view.len(), total)).weak().small());

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
        TableBuilder::new(ui)
            .id_salt("files")
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(40.0))
            .column(Column::remainder().at_least(90.0).clip(true))
            .column(Column::remainder().at_least(70.0).clip(true))
            .column(Column::remainder().at_least(90.0).clip(true))
            .column(Column::initial(self.s.font_size * 8.5).at_least(70.0).clip(true))
            .column(Column::initial(self.s.font_size * 5.2).at_least(50.0).clip(true))
            .column(Column::initial(self.s.font_size * 7.5).at_least(60.0).clip(true))
            .header(row_h + 4.0, |mut h| {
                h.col(|ui| {
                    ui.label(RichText::new("#").strong());
                });
                h.col(|ui| header(ui, SortCol::File, "File", &mut clicked));
                h.col(|ui| header(ui, SortCol::Current, "Current folder", &mut clicked));
                h.col(|ui| header(ui, SortCol::Dest, "Destination", &mut clicked));
                h.col(|ui| header(ui, SortCol::Date, "Date used", &mut clicked));
                h.col(|ui| header(ui, SortCol::Source, "Source", &mut clicked));
                h.col(|ui| header(ui, SortCol::Status, "Status", &mut clicked));
            })
            .body(|body| {
                body.rows(row_h, view.len(), |mut row| {
                    let idx = row.index();
                    let it = &plan.items[view[idx]];
                    let name = it.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    row.col(|ui| {
                        ui.label(RichText::new((idx + 1).to_string()).weak().small());
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(&name).truncate()).on_hover_text(it.source.display().to_string());
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(rel(&root, &it.current_dir)).truncate())
                            .on_hover_text(it.current_dir.display().to_string());
                    });
                    row.col(|ui| {
                        let txt = match it.status {
                            Status::Move => rel(&root, &it.dest_path),
                            Status::AlreadySorted => "— (stays)".into(),
                            _ => "—".into(),
                        };
                        let mut rt = RichText::new(txt);
                        if it.renamed_for_collision {
                            rt = rt.color(amber());
                        }
                        let r = ui.add(egui::Label::new(rt).truncate());
                        if it.status == Status::Move {
                            r.on_hover_text(it.dest_path.display().to_string());
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
                        ui.label(txt);
                    });
                    row.col(|ui| {
                        match it.date {
                            Some(d) => {
                                let r = ui.label(d.source.label());
                                if d.via_fallback {
                                    r.on_hover_text("Used the fallback rule");
                                }
                            }
                            None => {
                                ui.label("—");
                            }
                        };
                    });
                    row.col(|ui| {
                        let label = match &it.status {
                            Status::Move if it.renamed_for_collision => "Move (renamed)".to_string(),
                            s => s.label().to_string(),
                        };
                        let r =
                            ui.label(RichText::new(label).color(status_color(&it.status, it.renamed_for_collision)));
                        if let Status::Error(e) = &it.status {
                            r.on_hover_text(e);
                        }
                    });
                });
            });
        if let Some(c) = clicked {
            self.sort = if self.sort.0 == c { (c, !self.sort.1) } else { (c, true) };
            self.view_dirty = true;
        }
    }

    fn folders_tab(&mut self, ui: &mut egui::Ui) {
        let Some(plan) = &self.plan else {
            empty_state(ui, false);
            return;
        };
        let root = plan.root.clone();
        let rows = &self.folder_rows;
        ui.label(RichText::new(format!("{} destination folders", rows.len())).weak().small());
        let row_h = self.s.font_size + 10.0;
        TableBuilder::new(ui)
            .id_salt("folders")
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(460.0).at_least(120.0).clip(true))
            .column(Column::initial(90.0))
            .column(Column::remainder())
            .header(row_h + 4.0, |mut h| {
                h.col(|ui| {
                    ui.strong("Destination folder");
                });
                h.col(|ui| {
                    ui.strong("Files");
                });
                h.col(|ui| {
                    ui.strong("Folder");
                });
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
    }

    fn results_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut any = false;
            if let Some(r) = &self.run_report {
                any = true;
                ui.strong("Last apply");
                ui.horizontal_wrapped(|ui| {
                    chip(ui, "Moved", r.moved, green());
                    chip(ui, "Skipped", r.skipped, grey());
                    chip(ui, "Errors", r.errors.len(), red());
                    if r.copied_across_devices > 0 {
                        chip(ui, "Copied across drives", r.copied_across_devices, amber());
                    }
                });
                if r.cancelled {
                    ui.label(
                        RichText::new("Run was cancelled part-way; the moves done so far are in the undo log.")
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
            ui.set_width(440.0);
            match c {
                Confirm::Apply => {
                    let n = self.counts;
                    ui.heading("Move files?");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "{} files will be moved into {} destination folders ({} of them new). Nothing is overwritten; name clashes get “ (1)”, “ (2)”…",
                        n.to_move, n.dest_folders, n.new_folders
                    ));
                    ui.label("An undo log is written to the chosen folder first, so you can undo this run.");
                }
                Confirm::Undo { moves, started, log } => {
                    ui.heading("Undo last run?");
                    ui.add_space(6.0);
                    ui.label(format!("Move {moves} files back to where they were (run started {started})."));
                    ui.label("Only empty folders created by that run are removed. Nothing else is deleted.");
                    ui.label(RichText::new(log.display().to_string()).weak().small());
                }
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match c {
                    Confirm::Apply => "Move files",
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
                Some(Confirm::Apply) => self.start_apply(ctx),
                Some(Confirm::Undo { log, .. }) => self.start_undo(ctx, log),
                None => {}
            }
        } else if close {
            self.confirm = None;
        }
    }
}

fn kind_desc(k: SourceKind) -> &'static str {
    match k {
        SourceKind::Filename => "Filename",
        SourceKind::Exif => "EXIF date taken",
        SourceKind::Modified => "Modified date",
        SourceKind::Created => "Created date",
    }
}

fn empty_state(ui: &mut egui::Ui, after_run: bool) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        if after_run {
            ui.label(RichText::new("Run finished — see “Results / errors”.").size(18.0));
            ui.label(RichText::new("Click Preview again to re-scan the folder.").weak());
        } else {
            ui.label(RichText::new("No preview yet").size(20.0).strong());
            ui.add_space(4.0);
            ui.label(
                RichText::new("1. Choose or drop a folder   2. Check the options   3. Click “Preview (dry run)”")
                    .weak(),
            );
            ui.label(RichText::new("Nothing is moved until you click Apply and confirm.").weak());
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
