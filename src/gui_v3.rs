//! 3.0 additions to the GUI: the Plex tab, the folder blacklist editor,
//! favorites (star button, menu, Manage Favorites) and the TMDB lookup.
//! A child module of `gui`, so it works directly on `App`'s fields.

use super::*;
use organimizer::blacklist::{self, Blacklist};
use organimizer::favorites::{self, Favorites};
use organimizer::plex::{self, Confidence, ExtraKind, Kind};
use organimizer::tmdb;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Organize,
    Plex,
}

pub(super) enum PlexMsg {
    Progress {
        text: String,
        frac: f32,
    },
    Scanned {
        root: PathBuf,
        rows: Vec<plex::Row>,
        skipped: Vec<PathBuf>,
        errors: Vec<(PathBuf, String)>,
        cancelled: bool,
    },
    Applied(RunReport),
    Undone(UndoReport),
    Matches {
        row: usize,
        kind: tmdb::SearchKind,
        result: Result<Vec<tmdb::Match>, String>,
    },
    EpisodeName {
        row: usize,
        result: Result<Option<String>, String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlexJob {
    Scan,
    Apply,
    Undo,
    Lookup,
}

pub(super) struct PlexWorker {
    rx: Receiver<PlexMsg>,
    cancel: Arc<AtomicBool>,
    kind: PlexJob,
}

fn spawn_plex<F>(ctx: &egui::Context, kind: PlexJob, f: F) -> PlexWorker
where
    F: FnOnce(Sender<PlexMsg>, Arc<AtomicBool>, egui::Context) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let ctx2 = ctx.clone();
    std::thread::spawn(move || {
        f(tx, c2, ctx2.clone());
        ctx2.request_repaint();
    });
    PlexWorker { rx, cancel, kind }
}

/// TMDB results waiting for the user to pick one.
pub(super) struct TmdbPick {
    row: usize,
    kind: tmdb::SearchKind,
    list: Vec<tmdb::Match>,
}

pub(super) enum PlexConfirm {
    Apply,
    Undo { log: PathBuf, moves: usize, started: String },
}

#[derive(Default)]
pub(super) struct PlexState {
    pub rows: Vec<plex::Row>,
    pub root: Option<PathBuf>,
    pub plan: Option<Plan>,
    pub plan_dirty: bool,
    pub selected: Option<usize>,
    pub worker: Option<PlexWorker>,
    pub progress: (String, f32),
    pub status: String,
    pub filter: String,
    pub only_attention: bool,
    pub skipped: Vec<PathBuf>,
    pub errors: Vec<(PathBuf, String)>,
    pub report: Option<RunReport>,
    pub undo_report: Option<UndoReport>,
    pub pick: Option<TmdbPick>,
    pub confirm: Option<PlexConfirm>,
    pub undo_cache: (String, Option<PathBuf>),
}

impl PlexState {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    /// Apply / Undo running: files are being moved.
    pub fn moving(&self) -> bool {
        matches!(self.worker.as_ref().map(|w| w.kind), Some(PlexJob::Apply | PlexJob::Undo))
    }
}

/// Unsaved edits in Tools > Folder Blacklist.
pub(super) struct BlacklistDraft {
    entries: Vec<String>,
    custom: bool,
    new_entry: String,
    editing: Option<(usize, String)>,
    error: Option<String>,
    test_path: String,
}

/// Unsaved edits in Favorites > Manage Favorites.
pub(super) struct FavoritesDraft {
    list: Favorites,
    sel: Option<usize>,
    name: String,
    path: String,
    error: Option<String>,
}

/// Colored dot + text (the font has no reliable "●" glyph).
fn dot_label(ui: &mut egui::Ui, color: Color32, text: impl Into<String>) -> egui::Response {
    let d = ui.text_style_height(&TextStyle::Body) * 0.55;
    let (r, _) = ui.allocate_exact_size(Vec2::splat(d), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), d * 0.45, color);
    ui.label(RichText::new(text.into()).color(color))
}

fn conf_color(c: Confidence) -> Color32 {
    match c {
        Confidence::High => green(),
        Confidence::Medium => amber(),
        Confidence::Low => red(),
    }
}

/// "Movies\Heat (1995)\Heat (1995).mkv" style text for a plan item.
fn dest_text(it: &plan::PlanItem, base: &Path) -> String {
    match it.dest_path.strip_prefix(base) {
        Ok(r) => r.display().to_string(),
        Err(_) => it.dest_path.display().to_string(),
    }
}

impl App {
    // ------------------------------------------------------------------
    // Shared: blacklist, favorites, folder checks
    // ------------------------------------------------------------------

    /// Why a folder may not be used (None = fine).
    pub(super) fn blocked_reason(&self, p: &Path) -> Option<String> {
        let s = p.to_string_lossy();
        if s.trim().is_empty() {
            return None;
        }
        self.s.blacklist.blocked_by(&s).map(|e| {
            if e == blacklist::DRIVE_ROOTS {
                format!("{} is a drive root. Choose a folder on the drive instead (Tools > Folder Blacklist).", p.display())
            } else {
                format!(
                    "{} is a protected system folder (blacklisted by “{e}”). Choose another folder, or change the list in Tools > Folder Blacklist.",
                    p.display()
                )
            }
        })
    }

    /// Show a "folder blocked" message. Returns true if blocked.
    pub(super) fn reject_if_blocked(&mut self, p: &Path) -> bool {
        match self.blocked_reason(p) {
            Some(text) => {
                self.status = "That folder is blacklisted; nothing was selected.".into();
                self.dialog = Some(Dialog::Message { title: "Folder not allowed".into(), text });
                true
            }
            None => false,
        }
    }

    /// The ☆ / ★ toggle next to a folder field.
    pub(super) fn star_button(&mut self, ui: &mut egui::Ui, path: &str) {
        let p = path.trim().to_string();
        let fav = self.s.favorites.contains(&p);
        let (glyph, tip) = if fav {
            ("★", "In Favorites - click to remove this folder from Favorites")
        } else {
            ("☆", "Add this folder to Favorites (one click; Favorites menu to jump back)")
        };
        let txt = RichText::new(glyph).size(self.s.font_size * 1.25).color(if fav {
            amber()
        } else {
            ui.visuals().text_color()
        });
        let r = ui.add_enabled(!p.is_empty(), egui::Button::new(txt).frame(false)).on_hover_text(tip);
        if r.clicked() {
            let now = self.s.favorites.toggle(&p);
            self.save_settings();
            self.status = if now { format!("Added {p} to Favorites.") } else { format!("Removed {p} from Favorites.") };
        }
    }

    /// Jump to a favorite: the folder of the current tab.
    pub(super) fn go_to_favorite(&mut self, path: &str) {
        let p = PathBuf::from(path.trim());
        if self.reject_if_blocked(&p) {
            return;
        }
        if !p.is_dir() {
            self.dialog = Some(Dialog::Message {
                title: "Folder not found".into(),
                text: format!(
                    "{} does not exist (any more). Edit or remove it in Favorites > Manage Favorites.",
                    p.display()
                ),
            });
            return;
        }
        match self.mode {
            Mode::Organize => self.set_folder(&p),
            Mode::Plex => {
                self.s.plex.source = p.to_string_lossy().into_owned();
                self.plex.status = format!("Selected {} - click Scan.", p.display());
            }
        }
    }

    pub(super) fn favorites_menu(&mut self, ui: &mut egui::Ui) {
        let locked = self.locked();
        ui.menu_button("Favorites", |ui| {
            let cur = match self.mode {
                Mode::Organize => self.s.folder_trimmed().to_string(),
                Mode::Plex => self.s.plex.source.trim().to_string(),
            };
            let is_fav = self.s.favorites.contains(&cur);
            let label =
                if is_fav { "★ Remove current folder from Favorites" } else { "☆ Add current folder to Favorites" };
            if ui.add_enabled(!cur.is_empty(), egui::Button::new(label)).clicked() {
                ui.close();
                self.s.favorites.toggle(&cur);
                self.save_settings();
            }
            ui.separator();
            if self.s.favorites.items.is_empty() {
                ui.label(RichText::new("No favorites yet - use the ☆ next to a folder").weak());
            }
            let items = self.s.favorites.items.clone();
            for f in items {
                let missing = !Path::new(&f.path).is_dir();
                let text = if missing { RichText::new(&f.name).weak() } else { RichText::new(&f.name) };
                let r = ui.add_enabled(!locked, egui::Button::new(text)).on_hover_text(if missing {
                    format!("{} (not found)", f.path)
                } else {
                    f.path.clone()
                });
                if r.clicked() {
                    ui.close();
                    self.go_to_favorite(&f.path);
                }
            }
            ui.separator();
            if ui.button("Manage Favorites…").on_hover_text("Rename, edit, reorder or remove favorites").clicked() {
                ui.close();
                self.open_favorites_editor();
            }
        });
    }

    pub(super) fn open_favorites_editor(&mut self) {
        let list = self.s.favorites.clone();
        let sel = if list.items.is_empty() { None } else { Some(0) };
        let (name, path) = sel.map(|i| (list.items[i].name.clone(), list.items[i].path.clone())).unwrap_or_default();
        self.favorites_editor = Some(FavoritesDraft { list, sel, name, path, error: None });
    }

    pub(super) fn open_blacklist_editor(&mut self) {
        self.blacklist_editor = Some(BlacklistDraft {
            entries: self.s.blacklist.entries.clone(),
            custom: self.s.blacklist_custom,
            new_entry: String::new(),
            editing: None,
            error: None,
            test_path: String::new(),
        });
    }
}

impl App {
    /// Tools > Folder Blacklist: list with add / edit / remove, Reset to
    /// defaults, a "test a path" field. Save writes the INI; Close discards.
    pub(super) fn blacklist_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.blacklist_editor.take() else { return };
        let fs = self.s.font_size;
        let mut save = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("organimizer-blacklist")).show(ctx, |ui| {
            ui.set_width((fs * 42.0).min(ctx.content_rect().width() - 60.0));
            ui.heading("Folder Blacklist");
            ui.label(
                RichText::new(
                    "These folders can never be chosen, and are skipped when subfolders are scanned (Organize and Plex). \
                     Matching ignores letter case and works on whole folder names: C:\\Windows does not block C:\\WindowsStuff. \
                     * stands for any one folder name (C:\\Users\\*\\AppData), *: for any drive.",
                )
                .weak(),
            );
            ui.label(
                RichText::new(if d.custom { "Your edited list." } else { "Built-in list (worked out from this PC's Windows folders)." })
                    .small()
                    .color(accent(ui)),
            );
            ui.add_space(4.0);
            let mut remove: Option<usize> = None;
            let mut commit_edit: Option<(usize, String)> = None;
            egui::ScrollArea::vertical().max_height(fs * 18.0).auto_shrink([false, true]).show(ui, |ui| {
                for i in 0..d.entries.len() {
                    ui.horizontal(|ui| {
                        let editing = d.editing.as_ref().is_some_and(|(j, _)| *j == i);
                        if editing {
                            let (_, text) = d.editing.as_mut().unwrap();
                            let r = ui.add(egui::TextEdit::singleline(text).desired_width(fs * 28.0));
                            let enter = r.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                            if ui.button("OK").clicked() || enter {
                                commit_edit = Some((i, text.clone()));
                            }
                            if ui.button("Cancel").clicked() {
                                d.editing = None;
                            }
                        } else {
                            let e = &d.entries[i];
                            let label = if e == blacklist::DRIVE_ROOTS {
                                RichText::new("<drive roots>  (C:\\, D:\\ … themselves)").italics()
                            } else {
                                RichText::new(e.as_str()).monospace()
                            };
                            ui.add(egui::Label::new(label).truncate());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(i);
                                }
                                if ui.small_button("Edit").clicked() {
                                    d.editing = Some((i, d.entries[i].clone()));
                                }
                            });
                        }
                    });
                }
                if d.entries.is_empty() {
                    ui.label(RichText::new("The list is empty: every folder can be chosen.").color(amber()));
                }
            });
            if let Some((i, text)) = commit_edit {
                match Blacklist::validate_entry(&text) {
                    Ok(v) if d.entries.iter().enumerate().any(|(j, e)| j != i && e.eq_ignore_ascii_case(&v)) => {
                        d.error = Some(format!("{v} is already in the list"));
                    }
                    Ok(v) => {
                        d.entries[i] = v;
                        d.custom = true;
                        d.editing = None;
                        d.error = None;
                    }
                    Err(e) => d.error = Some(e),
                }
            }
            if let Some(i) = remove {
                d.entries.remove(i);
                d.custom = true;
                d.editing = None;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut d.new_entry)
                        .hint_text("Add a folder, e.g. D:\\Backups or *:\\Temp")
                        .desired_width(fs * 24.0),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Browse…").clicked()
                    && let Some(p) = rfd::FileDialog::new().set_title("Folder to blacklist").pick_folder()
                {
                    d.new_entry = p.to_string_lossy().into_owned();
                }
                if ui.button("Add").clicked() || enter {
                    match Blacklist::validate_entry(&d.new_entry) {
                        Ok(v) if d.entries.iter().any(|e| e.eq_ignore_ascii_case(&v)) => {
                            d.error = Some(format!("{v} is already in the list"));
                        }
                        Ok(v) => {
                            d.entries.push(v);
                            d.custom = true;
                            d.new_entry.clear();
                            d.error = None;
                        }
                        Err(e) => d.error = Some(e),
                    }
                }
            });
            if let Some(e) = &d.error {
                ui.label(RichText::new(e).color(red()).small());
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Test a folder:");
                ui.add(egui::TextEdit::singleline(&mut d.test_path).hint_text("C:\\Windows\\Temp").desired_width(fs * 18.0));
                if !d.test_path.trim().is_empty() {
                    let bl = Blacklist { entries: d.entries.clone() };
                    match bl.blocked_by(&d.test_path) {
                        Some(e) => ui.label(RichText::new(format!("Blocked (by {e})")).color(red())),
                        None => ui.label(RichText::new("Allowed").color(green())),
                    };
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Reset to defaults")
                    .on_hover_text("Back to the built-in list of Windows system folders")
                    .clicked()
                {
                    d.entries = Blacklist::default().entries;
                    d.custom = false;
                    d.editing = None;
                    d.error = None;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let dirty = d.entries != self.s.blacklist.entries || d.custom != self.s.blacklist_custom;
                    let btn = egui::Button::new(RichText::new("Save").strong().color(Color32::WHITE)).fill(green());
                    if ui.add_enabled(dirty && !self.locked(), btn).clicked() {
                        save = true;
                    }
                    if ui.button(if dirty { "Close without saving" } else { "Close" }).clicked() {
                        close = true;
                    }
                });
            });
        });
        if save {
            self.s.blacklist = Blacklist { entries: d.entries };
            self.s.blacklist_custom = d.custom;
            self.save_settings();
            self.refresh();
            self.status = "Folder blacklist saved.".into();
            return;
        }
        if close || modal.should_close() {
            return;
        }
        self.blacklist_editor = Some(d);
    }

    /// Favorites > Manage Favorites: list on the left, name + folder on the
    /// right, Add / Save changes / Remove / Up / Down. Changes are saved at
    /// once (like Manage Presets).
    pub(super) fn favorites_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.favorites_editor.take() else { return };
        let fs = self.s.font_size;
        let mut close = false;
        let mut changed = false;
        let mut jump: Option<String> = None;
        let modal = egui::Modal::new(egui::Id::unique("organimizer-favorites")).show(ctx, |ui| {
            ui.set_width((fs * 44.0).min(ctx.content_rect().width() - 60.0));
            ui.heading("Manage Favorites");
            ui.label(RichText::new("Folders you use often. The ☆ next to a folder adds it in one click.").weak());
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(fs * 14.0);
                    egui::ScrollArea::vertical().id_salt("fav-list").max_height(fs * 14.0).show(ui, |ui| {
                        for i in 0..d.list.items.len() {
                            let f = &d.list.items[i];
                            let missing = !Path::new(&f.path).is_dir();
                            let t = if missing {
                                RichText::new(format!("{} (missing)", f.name)).weak()
                            } else {
                                RichText::new(&f.name)
                            };
                            if ui.selectable_label(d.sel == Some(i), t).on_hover_text(&f.path).clicked() {
                                d.sel = Some(i);
                                d.name = f.name.clone();
                                d.path = f.path.clone();
                                d.error = None;
                            }
                        }
                        if d.list.items.is_empty() {
                            ui.label(RichText::new("No favorites yet").weak());
                        }
                    });
                    ui.horizontal(|ui| {
                        let sel = d.sel;
                        if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new("⏶ Up")).clicked()
                            && let Some(j) = d.list.move_by(sel.unwrap(), -1)
                        {
                            d.sel = Some(j);
                            changed = true;
                        }
                        if ui
                            .add_enabled(sel.is_some_and(|i| i + 1 < d.list.items.len()), egui::Button::new("⏷ Down"))
                            .clicked()
                            && let Some(j) = d.list.move_by(sel.unwrap(), 1)
                        {
                            d.sel = Some(j);
                            changed = true;
                        }
                    });
                });
                ui.add_space(fs);
                ui.vertical(|ui| {
                    egui::Grid::new("fav-edit").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut d.name).desired_width(fs * 16.0).hint_text("e.g. Movies"),
                        );
                        ui.end_row();
                        ui.label("Folder");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut d.path)
                                    .desired_width(fs * 16.0)
                                    .hint_text("D:\\Media\\Movies"),
                            );
                            if ui.small_button("…").on_hover_text("Browse").clicked()
                                && let Some(p) = rfd::FileDialog::new().set_title("Favorite folder").pick_folder()
                            {
                                d.path = p.to_string_lossy().into_owned();
                                if d.name.trim().is_empty() {
                                    d.name = favorites::default_name(&d.path);
                                }
                            }
                        });
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Add as new").clicked() {
                            let p = d.path.trim().to_string();
                            if p.is_empty() {
                                d.error = Some("Enter a folder".into());
                            } else if let Some(r) = self.blocked_reason(Path::new(&p)) {
                                d.error = Some(r);
                            } else if d.list.add(&d.name, &p) {
                                d.sel = Some(d.list.items.len() - 1);
                                d.name = d.list.items.last().map(|f| f.name.clone()).unwrap_or_default();
                                d.error = None;
                                changed = true;
                            } else {
                                d.error = Some("That folder is already a favorite".into());
                            }
                        }
                        if let Some(i) = d.sel {
                            if ui.button("Save changes").clicked() {
                                if let Some(r) = self.blocked_reason(Path::new(d.path.trim())) {
                                    d.error = Some(r);
                                } else {
                                    match d.list.edit(i, &d.name, &d.path) {
                                        Ok(()) => {
                                            d.name = d.list.items[i].name.clone();
                                            d.error = None;
                                            changed = true;
                                        }
                                        Err(e) => d.error = Some(e),
                                    }
                                }
                            }
                            if ui.button("Remove").clicked() {
                                d.list.items.remove(i);
                                d.sel =
                                    if d.list.items.is_empty() { None } else { Some(i.min(d.list.items.len() - 1)) };
                                if let Some(j) = d.sel {
                                    d.name = d.list.items[j].name.clone();
                                    d.path = d.list.items[j].path.clone();
                                }
                                changed = true;
                            }
                            if ui.button("Go to folder").clicked() {
                                jump = d.list.items.get(i).map(|f| f.path.clone());
                            }
                        }
                    });
                    if let Some(e) = &d.error {
                        ui.label(RichText::new(e).color(red()).small());
                    }
                });
            });
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
                ui.label(RichText::new("Changes are saved at once.").weak().small());
            });
        });
        if changed {
            self.s.favorites = d.list.clone();
            self.save_settings();
        }
        if let Some(p) = jump {
            self.go_to_favorite(&p);
            return;
        }
        if close || modal.should_close() {
            return;
        }
        self.favorites_editor = Some(d);
    }
}

// ----------------------------------------------------------------------
// Plex tab
// ----------------------------------------------------------------------

impl App {
    pub(super) fn plex_root(&self) -> PathBuf {
        PathBuf::from(self.s.plex.source.trim())
    }

    fn plex_dest(&self) -> plex::Destination {
        if self.s.plex.in_place {
            return plex::Destination::default();
        }
        let p = |s: &str| {
            let t = s.trim();
            if t.is_empty() { None } else { Some(PathBuf::from(t)) }
        };
        plex::Destination { movies_root: p(&self.s.plex.movies_root), tv_root: p(&self.s.plex.tv_root) }
    }

    /// Problems with the destination roots (None = OK).
    fn plex_dest_problem(&self) -> Option<String> {
        if self.s.plex.in_place {
            return None;
        }
        for (label, s) in [("Movies", &self.s.plex.movies_root), ("TV shows", &self.s.plex.tv_root)] {
            let t = s.trim();
            if t.is_empty() {
                continue;
            }
            let p = Path::new(t);
            if let Some(r) = self.blocked_reason(p) {
                return Some(format!("{label} library: {r}"));
            }
            if !p.is_dir() {
                return Some(format!("{label} library folder does not exist: {t}"));
            }
        }
        None
    }

    pub(super) fn plex_undo_log(&mut self) -> Option<PathBuf> {
        let root = self.plex_root();
        let key = root.to_string_lossy().into_owned();
        if self.plex.undo_cache.0 != key {
            let found = if key.is_empty() || !root.is_dir() { None } else { ops::find_latest_log(&root) };
            self.plex.undo_cache = (key, found);
        }
        self.plex.undo_cache.1.clone()
    }

    pub(super) fn plex_browse(&mut self) {
        let mut dlg = rfd::FileDialog::new().set_title("Choose the folder with your movies / TV shows");
        let cur = self.plex_root();
        if cur.is_dir() {
            dlg = dlg.set_directory(&cur);
        }
        if let Some(p) = dlg.pick_folder() {
            self.plex_set_source(&p);
        }
    }

    pub(super) fn plex_set_source(&mut self, p: &Path) {
        let p = if p.is_file() { p.parent().map(Path::to_path_buf).unwrap_or_default() } else { p.to_path_buf() };
        if self.reject_if_blocked(&p) {
            return;
        }
        self.s.plex.source = p.to_string_lossy().into_owned();
        self.plex.status = format!("Selected {} - click Scan to analyze it.", p.display());
        self.status = self.plex.status.clone();
        self.save_settings();
    }

    pub(super) fn plex_scan(&mut self, ctx: &egui::Context) {
        let root = self.plex_root();
        if self.s.plex.source.trim().is_empty() || !root.is_dir() {
            self.dialog = Some(Dialog::Message {
                title: "Choose a folder".into(),
                text: "Choose an existing folder with movies or TV episodes first (Browse… or drop a folder).".into(),
            });
            return;
        }
        if self.reject_if_blocked(&root) {
            return;
        }
        if let Some(w) = self.plex.worker.take() {
            w.cancel.store(true, Ordering::Relaxed);
        }
        self.save_settings();
        let recursive = self.s.plex.include_subfolders;
        let bl = self.s.blacklist.clone();
        let mut exts: Vec<String> = organimizer::filetypes::VIDEO_EXTS.iter().map(|s| s.to_string()).collect();
        exts.extend(plex::SUBTITLE_EXTS.iter().map(|s| s.to_string()));
        let filter = Filter::Exts(exts);
        self.plex.report = None;
        self.plex.undo_report = None;
        self.plex.pick = None;
        self.plex.status = format!("Scanning {}…", root.display());
        self.status = self.plex.status.clone();
        self.plex.worker = Some(spawn_plex(ctx, PlexJob::Scan, move |tx, cancel, ctx| {
            let mut t = Ticker::new();
            let res = scan::scan_filtered(&root, recursive, &filter, &bl, &cancel, &mut |n, p| {
                if t.due() {
                    let _ = tx
                        .send(PlexMsg::Progress { text: format!("Scanning… {n} files  {}", p.display()), frac: 0.0 });
                    ctx.request_repaint();
                }
            });
            let cancelled = cancel.load(Ordering::Relaxed);
            let _ = tx.send(PlexMsg::Progress { text: format!("Analyzing {} files…", res.files.len()), frac: 0.5 });
            ctx.request_repaint();
            let rows = plex::analyze_files(&root, &res.files);
            let _ = tx.send(PlexMsg::Scanned { root, rows, skipped: res.skipped_dirs, errors: res.errors, cancelled });
        }));
    }

    fn plex_rebuild_plan(&mut self) {
        self.plex.plan_dirty = false;
        let Some(root) = self.plex.root.clone() else {
            self.plex.plan = None;
            return;
        };
        self.plex.plan = Some(plex::build_plan(&root, &self.plex.rows, &self.plex_dest()));
    }

    pub(super) fn plex_poll(&mut self) {
        let Some(w) = &self.plex.worker else { return };
        let msgs: Vec<PlexMsg> = w.rx.try_iter().collect();
        let mut finished = false;
        for m in msgs {
            match m {
                PlexMsg::Progress { text, frac } => self.plex.progress = (text, frac),
                PlexMsg::Scanned { root, rows, skipped, errors, cancelled } => {
                    let subs = rows.iter().filter(|r| r.subtitle_of.is_some()).count();
                    let low = rows
                        .iter()
                        .filter(|r| r.subtitle_of.is_none() && r.analysis.confidence == Confidence::Low)
                        .count();
                    self.plex.status = format!(
                        "Analyzed {} videos and {} subtitles{}{} - check the proposals (red = please check), then Apply. Nothing has been changed yet.",
                        rows.len() - subs,
                        subs,
                        if low > 0 { format!(", {low} need a look") } else { String::new() },
                        if cancelled { " (scan canceled: partial)" } else { "" }
                    );
                    self.plex.root = Some(root);
                    self.plex.rows = rows;
                    self.plex.skipped = skipped;
                    self.plex.errors = errors;
                    self.plex.selected = None;
                    self.plex_rebuild_plan();
                    finished = true;
                }
                PlexMsg::Applied(rep) => {
                    self.plex.status = format!(
                        "Done: {} files moved into the Plex layout, {} errors{}. Undo is available.",
                        rep.moved,
                        rep.errors.len(),
                        if rep.cancelled { " (canceled)" } else { "" }
                    );
                    self.plex.report = Some(rep);
                    self.plex_after_disk_change();
                    finished = true;
                }
                PlexMsg::Undone(rep) => {
                    self.plex.status = format!(
                        "Undo: {} files restored, {} empty folders removed, {} problems.",
                        rep.restored,
                        rep.dirs_removed,
                        rep.errors.len()
                    );
                    self.plex.undo_report = Some(rep);
                    self.plex_after_disk_change();
                    finished = true;
                }
                PlexMsg::Matches { row, kind, result } => {
                    match result {
                        Ok(list) if list.is_empty() => {
                            self.plex.status =
                                "TMDB found nothing for this title. Edit the title and try again.".into();
                        }
                        Ok(list) => self.plex.pick = Some(TmdbPick { row, kind, list }),
                        Err(e) => {
                            self.plex.status = format!("TMDB lookup failed: {e}");
                            self.dialog = Some(Dialog::Message {
                                title: "TMDB lookup failed".into(),
                                text: format!("{e}\n\nNothing was changed; the offline analysis is still used."),
                            });
                        }
                    }
                    finished = true;
                }
                PlexMsg::EpisodeName { row, result } => {
                    match result {
                        Ok(Some(name)) => {
                            if let Some(r) = self.plex.rows.get_mut(row) {
                                r.analysis.episode_title = Some(plex::safe_component(&name));
                            }
                            self.plex.status = format!("Episode title from TMDB: {name}");
                            self.plex.plan_dirty = true;
                        }
                        Ok(None) => self.plex.status = "TMDB has no title for this episode.".into(),
                        Err(e) => self.plex.status = format!("TMDB episode lookup failed: {e}"),
                    }
                    finished = true;
                }
            }
        }
        if finished {
            self.plex.worker = None;
            self.plex.progress = (String::new(), 0.0);
            self.status = self.plex.status.clone();
        }
    }

    fn plex_after_disk_change(&mut self) {
        self.plex.rows.clear();
        self.plex.plan = None;
        self.plex.selected = None;
        self.plex.undo_cache.0 = "\0".into();
        // the Organize tab may show the same folder: forget its scan too
        self.scan_cache = None;
        self.invalidate_undo_cache();
    }

    fn plex_start_apply(&mut self, ctx: &egui::Context) {
        if self.plex.plan_dirty {
            self.plex_rebuild_plan();
        }
        let Some(plan) = self.plex.plan.clone() else { return };
        self.save_settings();
        self.plex.status = "Moving files into the Plex layout…".into();
        self.plex.worker = Some(spawn_plex(ctx, PlexJob::Apply, move |tx, cancel, ctx| {
            let mut t = Ticker::new();
            let rep = ops::execute(&plan, &cancel, &mut |i, n, p| {
                if t.due() {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let _ = tx.send(PlexMsg::Progress {
                        text: format!("Moving {i} / {n}  {name}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(PlexMsg::Applied(rep));
        }));
    }

    fn plex_start_undo(&mut self, ctx: &egui::Context, log: PathBuf) {
        self.plex.status = "Undoing the last run…".into();
        self.plex.worker = Some(spawn_plex(ctx, PlexJob::Undo, move |tx, _cancel, ctx| {
            let mut t = Ticker::new();
            let rep = ops::undo(&log, &mut |i, n| {
                if t.due() {
                    let _ = tx.send(PlexMsg::Progress {
                        text: format!("Restoring {i} / {n}"),
                        frac: if n == 0 { 1.0 } else { i as f32 / n as f32 },
                    });
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(PlexMsg::Undone(rep));
        }));
    }

    /// TMDB auth if lookups are switched on and the key looks valid.
    pub(super) fn tmdb_auth(&self) -> Option<tmdb::Auth> {
        if !self.s.plex.tmdb_enabled {
            return None;
        }
        tmdb::Auth::from_key(&self.s.plex.tmdb_key)
    }

    #[allow(unused_variables)]
    fn plex_lookup(&mut self, ctx: &egui::Context, row: usize) {
        let Some(auth) = self.tmdb_auth() else { return };
        let Some(r) = self.plex.rows.get(row) else { return };
        let a = &r.analysis;
        let kind = if a.kind == Kind::Episode || (a.kind == Kind::Extra && a.extra_of_show) {
            tmdb::SearchKind::Tv
        } else {
            tmdb::SearchKind::Movie
        };
        let (title, year, lang) = (a.title.clone(), a.year, self.s.plex.tmdb_language.clone());
        if title.trim().is_empty() {
            self.plex.status = "Enter a title first.".into();
            return;
        }
        self.plex.status = format!("Asking TMDB about “{title}”…");
        #[cfg(feature = "net")]
        {
            self.plex.worker = Some(spawn_plex(ctx, PlexJob::Lookup, move |tx, _c, _ctx| {
                let result = tmdb::search(kind, &title, year, &lang, &auth);
                let _ = tx.send(PlexMsg::Matches { row, kind, result });
            }));
        }
        #[cfg(not(feature = "net"))]
        {
            self.plex.status = "This build has no network support (TMDB lookups unavailable).".into();
        }
    }

    #[allow(unused_variables)]
    fn plex_episode_lookup(&mut self, ctx: &egui::Context, row: usize, tv_id: u64) {
        let Some(auth) = self.tmdb_auth() else { return };
        let Some(r) = self.plex.rows.get(row) else { return };
        let (Some(s), Some(e)) = (r.analysis.season, r.analysis.episode) else { return };
        let lang = self.s.plex.tmdb_language.clone();
        #[cfg(feature = "net")]
        {
            self.plex.worker = Some(spawn_plex(ctx, PlexJob::Lookup, move |tx, _c, _ctx| {
                let result = tmdb::episode_name(tv_id, s, e, &lang, &auth);
                let _ = tx.send(PlexMsg::EpisodeName { row, result });
            }));
        }
    }

    /// Use a confirmed TMDB match for a row (and for every row of the same
    /// detected title, so a whole season is fixed at once).
    fn plex_use_match(&mut self, ctx: &egui::Context, row: usize, kind: tmdb::SearchKind, m: &tmdb::Match) {
        let Some(r) = self.plex.rows.get(row) else { return };
        let old = plex::norm(&r.analysis.title);
        let title = plex::safe_component(&m.title);
        let mut n = 0;
        for x in self.plex.rows.iter_mut() {
            if x.subtitle_of.is_some() || plex::norm(&x.analysis.title) != old {
                continue;
            }
            x.analysis.title = title.clone();
            if m.year.is_some() {
                x.analysis.year = m.year;
            }
            if x.analysis.kind != Kind::Skip {
                x.analysis.confidence = Confidence::High;
            }
            x.analysis.notes.retain(|s| !s.starts_with("TMDB"));
            x.analysis.notes.push(format!("TMDB match confirmed (id {})", m.id));
            n += 1;
        }
        self.plex.plan_dirty = true;
        self.plex.status =
            format!("Using TMDB: {}{} for {n} files.", m.title, m.year.map(|y| format!(" ({y})")).unwrap_or_default());
        let r = &self.plex.rows[row];
        if kind == tmdb::SearchKind::Tv && r.analysis.kind == Kind::Episode && r.analysis.episode.is_some() {
            self.plex_episode_lookup(ctx, row, m.id);
        }
    }
}

/// Optional number: checkbox "known" + drag value.
fn opt_num(ui: &mut egui::Ui, v: &mut Option<u32>, max: u32, default: u32) -> bool {
    let mut on = v.is_some();
    let mut changed = ui.checkbox(&mut on, "").changed();
    if changed {
        *v = if on { Some(default) } else { None };
    }
    if let Some(n) = v.as_mut() {
        changed |= ui.add(egui::DragValue::new(n).range(0..=max)).changed();
    } else {
        ui.label(RichText::new("unknown").weak());
    }
    changed
}

impl App {
    /// Everything below the menu / mode bar in Plex mode.
    pub(super) fn plex_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.plex.plan_dirty {
            self.plex_rebuild_plan();
        }
        self.plex_action_bar(ui, ctx);
        self.plex_setup(ui, ctx);
        if self.plex.selected.is_some() && !self.plex.rows.is_empty() {
            egui::Panel::right("plex-editor")
                .resizable(true)
                .default_size(self.s.font_size * 22.0)
                .min_size(self.s.font_size * 16.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.plex_editor(ui, ctx));
                });
        }
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.lookup_strip(ui, ctx);
            self.plex_table(ui);
        });
    }

    fn plex_setup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let fs = self.s.font_size;
        let moving = self.plex.moving() || self.locked();
        egui::Panel::top("plex-setup").show(ui, |ui| {
            ui.add_space(6.0);
            ui.add_enabled_ui(!moving, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("1").strong().color(accent(ui)));
                    ui.label(RichText::new("Source").strong());
                    if ui
                        .button("📂 Browse…")
                        .on_hover_text("Folder with movies / TV episodes (downloads, rips…)")
                        .clicked()
                    {
                        self.plex_browse();
                    }
                    let src = self.s.plex.source.clone();
                    let w = (ui.available_width() - fs * 20.0).max(fs * 10.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.s.plex.source)
                            .hint_text("…or drop a folder here")
                            .desired_width(w),
                    );
                    self.star_button(ui, &src);
                    ui.checkbox(&mut self.s.plex.include_subfolders, "Include subfolders");
                    let scanning = matches!(self.plex.worker.as_ref().map(|w| w.kind), Some(PlexJob::Scan));
                    if scanning {
                        if ui.button("⏹ Stop").clicked()
                            && let Some(w) = &self.plex.worker
                        {
                            w.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        let label = if self.plex.root.is_some() { "↻ Rescan" } else { "🔍 Scan" };
                        let btn = egui::Button::new(RichText::new(label).strong());
                        if ui
                            .add_enabled(!self.plex.busy(), btn)
                            .on_hover_text("Read the folder and propose a Plex layout (nothing is changed)")
                            .clicked()
                        {
                            self.plex_scan(ctx);
                        }
                    }
                });
                let src = self.plex_root();
                if !self.s.plex.source.trim().is_empty() {
                    if let Some(r) = self.blocked_reason(&src) {
                        ui.label(RichText::new(r).color(red()).small());
                    } else if !src.is_dir() {
                        ui.label(RichText::new("Folder does not exist").color(red()).small());
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("2").strong().color(accent(ui)));
                    ui.label(RichText::new("Destination").strong());
                    let before = (self.s.plex.in_place, self.s.plex.movies_root.clone(), self.s.plex.tv_root.clone());
                    ui.radio_value(&mut self.s.plex.in_place, true, "In place")
                        .on_hover_text("Build the Plex folders inside the scanned folder");
                    ui.radio_value(&mut self.s.plex.in_place, false, "Library folders")
                        .on_hover_text("Move movies and TV shows into your Plex library folders");
                    if !self.s.plex.in_place {
                        for (label, which) in [("Movies:", 0), ("TV shows:", 1)] {
                            ui.label(label);
                            let field =
                                if which == 0 { &mut self.s.plex.movies_root } else { &mut self.s.plex.tv_root };
                            ui.add(
                                egui::TextEdit::singleline(field)
                                    .desired_width(fs * 13.0)
                                    .hint_text("(scanned folder)"),
                            );
                            if ui.small_button("…").on_hover_text("Browse").clicked()
                                && let Some(p) = rfd::FileDialog::new().set_title("Plex library folder").pick_folder()
                            {
                                if let Some(r) = self.blocked_reason(&p) {
                                    self.dialog = Some(Dialog::Message { title: "Folder not allowed".into(), text: r });
                                } else if which == 0 {
                                    self.s.plex.movies_root = p.to_string_lossy().into_owned();
                                } else {
                                    self.s.plex.tv_root = p.to_string_lossy().into_owned();
                                }
                            }
                        }
                    }
                    if before != (self.s.plex.in_place, self.s.plex.movies_root.clone(), self.s.plex.tv_root.clone()) {
                        self.plex.plan_dirty = true;
                    }
                });
                if let Some(p) = self.plex_dest_problem() {
                    ui.label(RichText::new(p).color(red()).small());
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TMDB").strong());
                    match (self.s.plex.tmdb_enabled, self.tmdb_auth()) {
                        (true, Some(a)) => {
                            ui.label(
                                RichText::new(format!(
                                    "lookups on ({}, {})",
                                    a.kind_label(),
                                    tmdb::masked(&self.s.plex.tmdb_key)
                                ))
                                .color(green()),
                            );
                            ui.label(
                                RichText::new("Select a file and click “Look up on TMDB” to confirm its title.")
                                    .weak()
                                    .small(),
                            );
                        }
                        (true, None) => {
                            ui.label(RichText::new("switched on, but the key is missing or not valid").color(amber()));
                        }
                        _ => {
                            ui.label(
                                RichText::new("off - everything is worked out offline from the file and folder names")
                                    .weak(),
                            );
                        }
                    }
                    if ui.small_button("Options…").on_hover_text("TMDB key and language: Tools > Options").clicked() {
                        self.open_options();
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    fn plex_action_bar(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let h = self.s.font_size + 16.0;
        egui::Panel::bottom("plex-actions").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("3").strong().color(accent(ui)));
                if self.plex.moving() {
                    let apply = matches!(self.plex.worker.as_ref().map(|w| w.kind), Some(PlexJob::Apply));
                    let label = if apply { "⏹ Cancel" } else { "Undo running…" };
                    if ui.add_enabled(apply, egui::Button::new(label).min_size(Vec2::new(160.0, h))).clicked()
                        && let Some(w) = &self.plex.worker
                    {
                        w.cancel.store(true, Ordering::Relaxed);
                    }
                    return;
                }
                let n = self.plex.plan.as_ref().map(|p| p.items.iter().filter(|i| i.status == Status::Move).count()).unwrap_or(0);
                let stale = self.plex.root.is_some() && self.plex.root.as_deref() != Some(self.plex_root().as_path());
                let problem = if stale {
                    Some("The source folder changed: click Scan / Rescan first".to_string())
                } else {
                    self.plex_dest_problem()
                };
                let ok = n > 0 && problem.is_none() && !self.plex.busy() && !self.locked();
                let text = if n > 0 { format!("✔ Apply - move {n} files into the Plex layout") } else { "✔ Apply".into() };
                let btn = if ok {
                    egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(green().gamma_multiply(0.9))
                } else {
                    egui::Button::new(text)
                }
                .min_size(Vec2::new(300.0, h));
                let r = ui.add_enabled(ok, btn).on_disabled_hover_text(match &problem {
                    Some(p) => p.clone(),
                    None => "Scan a folder first (nothing to move yet)".into(),
                });
                if r.clicked() {
                    self.plex.confirm = Some(PlexConfirm::Apply);
                }
                let log = self.plex_undo_log();
                let r = ui.add_enabled(log.is_some() && !self.plex.busy() && !self.locked(), egui::Button::new("↩ Undo last run").min_size(Vec2::new(170.0, h)));
                let r = match &log {
                    Some(p) => r.on_hover_text(format!("Moves the files back using {}", p.display())),
                    None => r.on_disabled_hover_text("No undo log (Organimizer-undo-*.json) in the source folder"),
                };
                if r.clicked()
                    && let Some(p) = log
                {
                    match ops::read_log(&p) {
                        Ok(l) => self.plex.confirm = Some(PlexConfirm::Undo { moves: l.moves.len(), started: l.started, log: p }),
                        Err(e) => self.plex.status = format!("Cannot read undo log: {e}"),
                    }
                }
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new("Nothing changes on disk until you click Apply and confirm. Existing files are never overwritten.")
                            .weak()
                            .small(),
                    )
                    .truncate(),
                );
            });
            ui.add_space(6.0);
        });
    }

    fn plex_table(&mut self, ui: &mut egui::Ui) {
        if self.plex.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Plex Media").heading());
                ui.add_space(6.0);
                let text = if let Some(rep) = &self.plex.report {
                    format!(
                        "Last run: {} files moved, {} errors. Scan again to check the result.",
                        rep.moved,
                        rep.errors.len()
                    )
                } else if let Some(rep) = &self.plex.undo_report {
                    format!("Undo: {} files restored. Scan again to check the result.", rep.restored)
                } else {
                    "Choose the folder with your movies or TV episodes and click Scan.\n\
                     Organimizer proposes names like “Movies\\Heat (1995)\\Heat (1995).mkv” and \
                     “TV Shows\\Show (2008)\\Season 01\\Show (2008) - s01e02.mkv”, with subtitles and extras.\n\
                     You check and correct every proposal before anything is moved."
                        .to_string()
                };
                ui.label(RichText::new(text).weak());
                ui.add_space(10.0);
                if ui
                    .button("Plex naming conventions (Help)")
                    .on_hover_text("Opens HELP.md: official Plex folder and file names for movies, TV and extras")
                    .clicked()
                {
                    self.open_help();
                }
                if let Some(rep) = &self.plex.report {
                    for (p, e) in rep.errors.iter().take(20) {
                        ui.label(RichText::new(format!("{}: {e}", p.display())).color(red()).small());
                    }
                }
            });
            return;
        }
        let rows = &self.plex.rows;
        let count = |k: Kind| rows.iter().filter(|r| r.subtitle_of.is_none() && r.analysis.kind == k).count();
        let subs = rows.iter().filter(|r| r.subtitle_of.is_some()).count();
        let low = rows
            .iter()
            .filter(|r| {
                r.subtitle_of.is_none() && r.analysis.confidence == Confidence::Low && r.analysis.kind != Kind::Skip
            })
            .count();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Movies {}", count(Kind::Movie)));
            ui.label(format!("· Episodes {}", count(Kind::Episode)));
            ui.label(format!("· Extras {}", count(Kind::Extra)));
            ui.label(format!("· Subtitles {subs}"));
            ui.label(format!("· Skipped {}", count(Kind::Skip)));
            if low > 0 {
                ui.label(RichText::new(format!("· {low} with low confidence (red)")).color(red()));
            }
            if !self.plex.skipped.is_empty() {
                ui.label(
                    RichText::new(format!("· {} blacklisted folders skipped", self.plex.skipped.len())).color(amber()),
                )
                .on_hover_text(
                    self.plex.skipped.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"),
                );
            }
            if !self.plex.errors.is_empty() {
                ui.label(RichText::new(format!("· {} read errors", self.plex.errors.len())).color(red()));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.plex.filter).hint_text("Filter by name…").desired_width(220.0));
            ui.checkbox(&mut self.plex.only_attention, "Only files that need a look");
            ui.label(RichText::new("Click a row to correct it.").weak().small());
        });
        let Some(plan) = self.plex.plan.as_ref() else { return };
        let root = self.plex.root.clone().unwrap_or_default();
        let needle = self.plex.filter.to_lowercase();
        let only = self.plex.only_attention;
        let view: Vec<usize> = (0..rows.len())
            .filter(|&i| {
                let r = &rows[i];
                let name = r.source.to_string_lossy().to_lowercase();
                (needle.is_empty() || name.contains(&needle))
                    && (!only
                        || r.analysis.confidence < Confidence::High
                        || plan
                            .items
                            .get(i)
                            .is_some_and(|it| it.status != Status::Move && it.status != Status::AlreadySorted))
            })
            .collect();
        let fs = self.s.font_size;
        let row_h = fs + 10.0;
        let selected = self.plex.selected;
        let mut click: Option<usize> = None;
        let mut toggle: Option<usize> = None;
        let dest = self.plex_dest();
        TableBuilder::new(ui)
            .id_salt("plex-table")
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(28.0))
            .column(Column::initial(fs * 5.0).at_least(50.0).clip(true))
            .column(Column::initial(fs * 6.5).at_least(60.0).clip(true))
            .column(Column::initial(fs * 20.0).at_least(80.0).clip(true))
            .column(Column::initial(fs * 26.0).at_least(80.0).clip(true))
            .column(Column::remainder().at_least(60.0).clip(true))
            .header(row_h + 4.0, |mut h| {
                for t in ["", "Match", "Type", "Current file", "New location (Plex)", "Why"] {
                    h.col(|ui| {
                        ui.label(RichText::new(t).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, view.len(), |mut row| {
                    let i = view[row.index()];
                    let r = &rows[i];
                    let it = plan.items.get(i);
                    let sel = selected == Some(i);
                    row.col(|ui| {
                        let mut inc = r.include;
                        if ui.checkbox(&mut inc, "").on_hover_text("Include this file").changed() {
                            toggle = Some(i);
                        }
                    });
                    row.col(|ui| {
                        if r.subtitle_of.is_some() {
                            ui.label(RichText::new("sub").weak());
                        } else if r.analysis.kind == Kind::Skip {
                            ui.label(RichText::new("-").weak()).on_hover_text("Skipped: this file stays where it is");
                        } else {
                            let c = r.analysis.confidence;
                            let edited = r.analysis != r.detected;
                            let text = if r.analysis.from_tmdb { "TMDB" } else if edited { "Edited" } else { c.label() };
                            dot_label(ui, conf_color(c), text).on_hover_text("How sure Organimizer is about this file: green = high, amber = medium, red = low (please check)");
                        }
                    });
                    row.col(|ui| {
                        let k = if r.subtitle_of.is_some() {
                            format!("Subtitle{}", if r.sub_tags.is_empty() { String::new() } else { format!(" ({})", r.sub_tags.join(".")) })
                        } else if r.analysis.kind == Kind::Extra {
                            r.analysis.extra.map(|e| e.folder().to_string()).unwrap_or_else(|| "Extra".into())
                        } else {
                            r.analysis.kind.label().to_string()
                        };
                        ui.label(k);
                    });
                    row.col(|ui| {
                        let rel = r.source.strip_prefix(&root).map(|p| p.display().to_string()).unwrap_or_else(|_| r.source.display().to_string());
                        if ui.add(egui::Button::selectable(sel, rel).truncate()).on_hover_text(r.source.display().to_string()).clicked() {
                            click = Some(i);
                        }
                    });
                    row.col(|ui| match it {
                        Some(it) if it.status == Status::Move || it.status == Status::AlreadySorted => {
                            let lib = match plex::library_of(&r.analysis) {
                                plex::Library::Movies if dest.movies_root.is_some() => "Movies › ",
                                plex::Library::Tv if dest.tv_root.is_some() => "TV › ",
                                _ => "",
                            };
                            let t = format!("{lib}{}", dest_text(it, &it.base_dir));
                            let rt = if it.status == Status::AlreadySorted {
                                RichText::new(format!("{t}  (already there)")).weak()
                            } else if it.renamed_for_collision {
                                RichText::new(t).color(amber())
                            } else {
                                RichText::new(t).color(green())
                            };
                            let resp = ui.add(egui::Label::new(rt).truncate()).on_hover_text(it.dest_path.display().to_string());
                            if it.renamed_for_collision {
                                resp.on_hover_text("That name is taken: a number was added, nothing is overwritten");
                            }
                        }
                        Some(it) => {
                            ui.label(RichText::new(format!("not moved - {}", it.warnings.first().cloned().unwrap_or_default())).color(amber()));
                        }
                        None => {}
                    });
                    row.col(|ui| {
                        let mut notes = r.analysis.notes.clone();
                        if let Some(it) = it {
                            notes.extend(it.warnings.iter().filter(|w| it.status == Status::Move || it.status == Status::AlreadySorted || !w.is_empty()).cloned());
                        }
                        notes.dedup();
                        let (short, full) = why_text(r, rows);
                        let line = if notes.is_empty() { short } else { format!("{}  |  {short}", notes.join("; ")) };
                        let tip = if notes.is_empty() { full } else { format!("{full}\n\nTo check:\n- {}", notes.join("\n- ")) };
                        let rt = if notes.is_empty() { RichText::new(line).weak().small() } else { RichText::new(line).color(amber()).small() };
                        ui.add(egui::Label::new(rt).truncate()).on_hover_text(tip);
                    });
                });
            });
        if let Some(i) = toggle {
            let r = &mut self.plex.rows[i];
            r.include = !r.include;
            let inc = r.include;
            // a subtitle follows its video
            for x in self.plex.rows.iter_mut().filter(|x| x.subtitle_of == Some(i)) {
                x.include = inc;
            }
            self.plex.plan_dirty = true;
        }
        if let Some(i) = click {
            self.plex.selected = if selected == Some(i) { None } else { Some(i) };
        }
    }
}

/// The Why column: a short line for the table and the full reasoning for the
/// tooltip / editor, in plain words.
fn why_text(r: &plex::Row, rows: &[plex::Row]) -> (String, String) {
    if let Some(v) = r.subtitle_of.and_then(|v| rows.get(v)) {
        let name = v.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tags = if r.sub_tags.is_empty() { String::new() } else { format!(" (language/flags: {})", r.sub_tags.join(".")) };
        let t = format!("subtitle with the same name as {name} -> follows it{tags}");
        return (t.clone(), t);
    }
    let mut reasons = r.analysis.why.clone();
    // TMDB confirmation first: it is the strongest reason
    reasons.sort_by_key(|w| !w.starts_with("Confirmed on TMDB"));
    if r.analysis != r.detected && !r.analysis.from_tmdb {
        reasons.insert(0, "Changed by you".into());
    }
    if let Some(d) = r.duration
        && !reasons.iter().any(|w| w.contains(&organimizer::duration::label(d)))
    {
        reasons.push(format!("runtime {}", organimizer::duration::label(d)));
    }
    if reasons.is_empty() {
        return (String::new(), "No reasons recorded".into());
    }
    let short = if r.analysis.from_tmdb {
        reasons.iter().take(2).cloned().collect::<Vec<_>>().join("; ")
    } else {
        reasons.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join("; ")
    };
    let full = format!("Why:\n- {}", reasons.join("\n- "));
    (short, full)
}

impl App {
    /// Right-hand panel: correct the selected file.
    fn plex_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(i) = self.plex.selected.filter(|&i| i < self.plex.rows.len()) else { return };
        let fs = self.s.font_size;
        let lookup_ok = self.tmdb_auth().is_some() && !self.plex.busy();
        let tmdb_on = self.s.plex.tmdb_enabled;
        let moving = self.plex.moving();
        let mut changed = false;
        let mut lookup = false;
        let mut same_title = false;
        let mut reset = false;
        let mut close = false;
        let video_of = self.plex.rows[i].subtitle_of;
        ui.horizontal(|ui| {
            ui.heading("Correct this file");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Close").clicked() {
                    close = true;
                }
            });
        });
        let name = self.plex.rows[i].source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ui.label(RichText::new(&name).monospace());
        {
            let (_, full) = why_text(&self.plex.rows[i], &self.plex.rows);
            egui::CollapsingHeader::new("Why this suggestion")
                .id_salt("plex-why")
                .default_open(true)
                .show(ui, |ui| {
                    for line in full.lines().skip(1) {
                        ui.label(RichText::new(line).small());
                    }
                });
        }
        ui.separator();
        if let Some(v) = video_of {
            let vname = self
                .plex
                .rows
                .get(v)
                .and_then(|r| r.source.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.label(format!("Subtitle for: {vname}"));
            ui.label(RichText::new("It is renamed together with its video. Edit the video to change the name.").weak());
            if ui.button("Go to the video").clicked() {
                self.plex.selected = Some(v);
            }
            if close {
                self.plex.selected = None;
            }
            return;
        }
        ui.add_enabled_ui(!moving, |ui| {
            let r = &mut self.plex.rows[i];
            let a = &mut r.analysis;
            egui::Grid::new("plex-edit").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Type");
                egui::ComboBox::from_id_salt("plex-kind").selected_text(a.kind.label()).show_ui(ui, |ui| {
                    for k in Kind::ALL {
                        if ui.selectable_value(&mut a.kind, k, k.label()).changed() {
                            changed = true;
                        }
                    }
                });
                ui.end_row();
                ui.label("Title");
                changed |= ui.add(egui::TextEdit::singleline(&mut a.title).desired_width(fs * 14.0)).changed();
                ui.end_row();
                ui.label("Year");
                let mut y = a.year.map(u32::from);
                if ui.horizontal(|ui| opt_num(ui, &mut y, 2100, 2000)).inner {
                    a.year = y.map(|v| v.clamp(1870, 2100) as u16);
                    changed = true;
                }
                ui.end_row();
                match a.kind {
                    Kind::Movie => {
                        ui.label("Edition");
                        let mut e = a.edition.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut e)
                                    .hint_text("e.g. Director's Cut")
                                    .desired_width(fs * 14.0),
                            )
                            .changed()
                        {
                            a.edition = if e.trim().is_empty() { None } else { Some(e) };
                            changed = true;
                        }
                        ui.end_row();
                    }
                    Kind::Episode => {
                        ui.label("Season");
                        ui.horizontal(|ui| changed |= opt_num(ui, &mut a.season, 999, 1));
                        ui.end_row();
                        ui.label("Episode");
                        ui.horizontal(|ui| changed |= opt_num(ui, &mut a.episode, 9999, 1));
                        ui.end_row();
                        ui.label("Last episode");
                        ui.horizontal(|ui| {
                            changed |= opt_num(ui, &mut a.episode_end, 9999, a.episode.unwrap_or(1) + 1)
                        })
                        .response
                        .on_hover_text("For a file with two or more episodes (s01e01-e02)");
                        ui.end_row();
                        ui.label("Episode title");
                        let mut t = a.episode_title.clone().unwrap_or_default();
                        if ui
                            .add(egui::TextEdit::singleline(&mut t).hint_text("optional").desired_width(fs * 14.0))
                            .changed()
                        {
                            a.episode_title = if t.trim().is_empty() { None } else { Some(t) };
                            changed = true;
                        }
                        ui.end_row();
                        if a.air_date.is_some() {
                            ui.label("Air date");
                            ui.horizontal(|ui| {
                                let (y, m, d) = a.air_date.unwrap();
                                ui.label(format!("{y:04}-{m:02}-{d:02}"));
                                if ui.small_button("Use season / episode").clicked() {
                                    a.air_date = None;
                                    changed = true;
                                }
                            });
                            ui.end_row();
                        }
                    }
                    Kind::Extra => {
                        ui.label("Extra type");
                        let cur = a.extra.map(|e| e.folder()).unwrap_or("(choose)");
                        egui::ComboBox::from_id_salt("plex-extra").selected_text(cur).show_ui(ui, |ui| {
                            for e in ExtraKind::ALL {
                                if ui.selectable_label(a.extra == Some(e), e.folder()).clicked() {
                                    a.extra = Some(e);
                                    changed = true;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Extra name");
                        let mut t = a.extra_name.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut t).hint_text("e.g. Making Of").desired_width(fs * 14.0),
                            )
                            .changed()
                        {
                            a.extra_name = if t.trim().is_empty() { None } else { Some(t) };
                            changed = true;
                        }
                        ui.end_row();
                        ui.label("Belongs to");
                        ui.horizontal(|ui| {
                            changed |= ui.radio_value(&mut a.extra_of_show, false, "Movie").changed();
                            changed |= ui.radio_value(&mut a.extra_of_show, true, "TV show").changed();
                        });
                        ui.end_row();
                    }
                    Kind::Skip => {}
                }
            });
            ui.add_space(6.0);
            let c = a.confidence;
            ui.horizontal(|ui| dot_label(ui, conf_color(c), format!("Confidence: {}", c.label())));
            for n in &a.notes {
                ui.label(RichText::new(format!("• {n}")).weak().small());
            }
            if let Some(m) = plex::missing(a) {
                ui.label(RichText::new(format!("Not moved yet: {m}")).color(amber()));
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Use for all files of this title")
                    .on_hover_text(
                        "Copy title and year to every file that was detected with the same title (e.g. a whole season)",
                    )
                    .clicked()
                {
                    same_title = true;
                }
                if ui
                    .add_enabled(r.analysis != r.detected, egui::Button::new("Reset"))
                    .on_hover_text("Back to what Organimizer detected")
                    .clicked()
                {
                    reset = true;
                }
            });
        });
        ui.separator();
        ui.label(RichText::new("TMDB").strong());
        if !tmdb_on {
            ui.label(RichText::new("Online lookup is off. Turn it on with your own free TMDB key in Tools > Options (see Help > “Getting a TMDB API key”).").weak().small());
        } else {
            let r = ui.add_enabled(lookup_ok && !moving, egui::Button::new("🔎 Look up on TMDB"));
            let r = if self.tmdb_auth().is_none() {
                r.on_disabled_hover_text("Enter a valid TMDB key in Tools > Options")
            } else {
                r
            };
            if r.on_hover_text("Search TMDB for this title and show the matches to choose from").clicked() {
                lookup = true;
            }
        }
        if close {
            self.plex.selected = None;
        }
        if changed {
            let r = &mut self.plex.rows[i];
            if r.analysis.kind == Kind::Extra && r.analysis.extra.is_none() {
                r.analysis.extra = Some(ExtraKind::Other);
            }
            if r.analysis.kind != Kind::Skip {
                r.analysis.confidence = Confidence::High;
            }
            self.plex.plan_dirty = true;
        }
        if reset {
            let r = &mut self.plex.rows[i];
            r.analysis = r.detected.clone();
            self.plex.plan_dirty = true;
        }
        if same_title {
            let src = self.plex.rows[i].analysis.clone();
            let key = plex::norm(&self.plex.rows[i].detected.title);
            let mut n = 0;
            for (j, x) in self.plex.rows.iter_mut().enumerate() {
                if j != i && x.subtitle_of.is_none() && plex::norm(&x.detected.title) == key {
                    x.analysis.title = src.title.clone();
                    x.analysis.year = src.year;
                    if x.analysis.kind != Kind::Skip {
                        x.analysis.confidence = Confidence::High;
                    }
                    n += 1;
                }
            }
            self.plex.status = format!("Title and year copied to {n} other files.");
            self.plex.plan_dirty = true;
        }
        if lookup {
            self.plex_lookup(ctx, i);
        }
    }

    /// Confirm Apply / Undo and the TMDB match picker.
    pub(super) fn plex_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(c) = self.plex.confirm.take() {
            let fs = self.s.font_size;
            let mut go = false;
            let mut cancel = false;
            let (title, body, ok) = match &c {
                PlexConfirm::Apply => {
                    let plan = self.plex.plan.as_ref();
                    let n = plan.map(|p| p.items.iter().filter(|i| i.status == Status::Move).count()).unwrap_or(0);
                    let coll = plan
                        .map(|p| p.items.iter().filter(|i| i.status == Status::Move && i.renamed_for_collision).count())
                        .unwrap_or(0);
                    let dest = if self.s.plex.in_place {
                        format!("inside {}", self.s.plex.source.trim())
                    } else {
                        "into your library folders".to_string()
                    };
                    (
                        "Move files into the Plex layout?".to_string(),
                        format!(
                            "{n} files will be renamed and moved {dest}.{}\n\nExisting files are never overwritten. \
                             An undo log is written first, so “Undo last run” can move everything back.",
                            if coll > 0 {
                                format!(" {coll} get a number added because the name is taken.")
                            } else {
                                String::new()
                            }
                        ),
                        "Apply",
                    )
                }
                PlexConfirm::Undo { moves, started, log } => (
                    "Undo the last run?".to_string(),
                    format!(
                        "Moves {moves} files from the run of {started} back to their original names and folders.\n\n{}",
                        log.display()
                    ),
                    "Undo",
                ),
            };
            let m = egui::Modal::new(egui::Id::unique("plex-confirm")).show(ctx, |ui| {
                ui.set_width((fs * 30.0).min(ctx.content_rect().width() - 60.0));
                ui.heading(title);
                ui.add_space(6.0);
                ui.label(body);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new(ok).strong().color(Color32::WHITE)).fill(green()))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if go {
                match c {
                    PlexConfirm::Apply => self.plex_start_apply(ctx),
                    PlexConfirm::Undo { log, .. } => self.plex_start_undo(ctx, log),
                }
            } else if !cancel && !m.should_close() {
                self.plex.confirm = Some(c);
            }
        }
        if let Some(p) = self.plex.pick.take() {
            let fs = self.s.font_size;
            let mut chosen: Option<tmdb::Match> = None;
            let mut cancel = false;
            let m = egui::Modal::new(egui::Id::unique("plex-tmdb")).show(ctx, |ui| {
                ui.set_width((fs * 34.0).min(ctx.content_rect().width() - 60.0));
                ui.heading(match p.kind {
                    tmdb::SearchKind::Movie => "TMDB: which movie is it?",
                    tmdb::SearchKind::Tv => "TMDB: which TV show is it?",
                });
                ui.label(RichText::new("Choose the right match. Nothing is moved until you click Apply.").weak());
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(fs * 22.0).show(ui, |ui| {
                    for mt in &p.list {
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(6, 4))
                            .corner_radius(CornerRadius::same(6))
                            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}{}",
                                            mt.title,
                                            mt.year.map(|y| format!(" ({y})")).unwrap_or_default()
                                        ))
                                        .strong(),
                                    );
                                    if !mt.original_title.is_empty() && mt.original_title != mt.title {
                                        ui.label(RichText::new(&mt.original_title).weak());
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.button("Use this").clicked() {
                                            chosen = Some(mt.clone());
                                        }
                                    });
                                });
                                if !mt.overview.is_empty() {
                                    let mut o: String = mt.overview.chars().take(220).collect();
                                    if mt.overview.chars().count() > 220 {
                                        o.push('…');
                                    }
                                    ui.label(RichText::new(o).small().weak());
                                }
                            });
                        ui.add_space(3.0);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("None of these").clicked() {
                        cancel = true;
                    }
                    ui.label(RichText::new(format!("Data from {}", tmdb::WEBSITE)).weak().small());
                });
            });
            if let Some(mt) = chosen {
                self.plex_use_match(ctx, p.row, p.kind, &mt);
            } else if !cancel && !m.should_close() {
                self.plex.pick = Some(p);
            }
        }
    }
}
