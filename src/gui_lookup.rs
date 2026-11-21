//! Plex tab: TMDB lookup windows (Mp3tag / Discogs style).
//!
//! * a strip above the table lists "what you have" ("Looks like: Alien
//!   (1979) - 1 movie, 3 extras") with a Look up button per item and one for
//!   the selected file;
//! * the results window shows poster, title, year, original title and a
//!   short overview; the search text and Movie / TV can be changed;
//! * the match window pairs the user's files (left) with the TMDB entries
//!   (right); OK writes the names into the preview rows (from TMDB). The
//!   normal Apply / Undo then moves the files - nothing happens here.
//!
//! Self-contained (logic in `organimizer::lookup`, HTTP in `tmdb.rs`), so
//! the feature can be removed by deleting this module and its three calls.

use super::*;
use organimizer::lookup::{self, Group, GroupKind, MatchSet, Target};
use organimizer::plex::{self, Kind};
use organimizer::tmdb::{self, Episode, Match, MovieDetails, SearchKind, TvDetails};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

const NO_KEY_HINT: &str = "Looking up needs your own free TMDB key and lookups switched on:\n\
     Tools > Options > TMDB. Help (F1) > “Getting a TMDB API key” shows how.";

enum Msg {
    Search { ticket: u64, key: String, result: Result<Vec<Match>, String> },
    Movie { ticket: u64, result: Result<MovieDetails, String> },
    Show { ticket: u64, key: (u64, u32), result: Result<(TvDetails, Vec<Episode>), String> },
    Poster { path: String, result: Result<Vec<u8>, String> },
}

/// Match window click: (pair, new target, up/down step, toggle tick).
type RowAction = (usize, Option<Option<usize>>, Option<bool>, bool);

enum Poster {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct Picker {
    group: Group,
    kind: SearchKind,
    query: String,
    year: String,
    results: Option<Result<Vec<Match>, String>>,
    ticket: u64,
}

struct Matcher {
    group: Group,
    chosen: Match,
    kind: SearchKind,
    set: Option<Result<MatchSet, String>>,
    ticket: u64,
}

pub(super) struct LookupState {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ticket: u64,
    groups: Vec<Group>,
    groups_sig: u64,
    picker: Option<Picker>,
    matcher: Option<Matcher>,
    /// Session caches: nothing is asked twice while the app runs.
    searches: HashMap<String, Vec<Match>>,
    movies: HashMap<u64, MovieDetails>,
    shows: HashMap<(u64, u32), (TvDetails, Vec<Episode>)>,
    posters: HashMap<String, Poster>,
    pending: HashSet<u64>,
    pub status: String,
}

impl Default for LookupState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        LookupState {
            tx,
            rx,
            ticket: 0,
            groups: vec![],
            groups_sig: 0,
            picker: None,
            matcher: None,
            searches: HashMap::new(),
            movies: HashMap::new(),
            shows: HashMap::new(),
            posters: HashMap::new(),
            pending: HashSet::new(),
            status: String::new(),
        }
    }
}

impl LookupState {
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || self.posters.values().any(|p| matches!(p, Poster::Loading))
    }
}

/// Changes whenever the grouping could change (titles, kinds, seasons).
fn rows_signature(rows: &[plex::Row]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rows.len().hash(&mut h);
    for r in rows {
        let a = &r.analysis;
        (a.kind as u8, &a.title, a.year, a.season, a.episode, a.extra_of_show, r.subtitle_of, a.edition.as_deref()).hash(&mut h);
    }
    h.finish()
}

fn search_key(kind: SearchKind, q: &str, year: Option<u16>, lang: &str) -> String {
    format!("{kind:?}|{}|{year:?}|{lang}", q.trim().to_lowercase())
}

fn decode_jpeg(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut d = jpeg_decoder::Decoder::new(bytes);
    let px = d.decode().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 || w > 2000 || h > 3000 {
        return None;
    }
    let rgba: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => px.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&l| [l, l, l, 255]).collect(),
        _ => return None,
    };
    (rgba.len() == w * h * 4).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

impl App {
    fn lookup_ready(&self) -> bool {
        self.tmdb_auth().is_some() && !self.plex.moving()
    }

    /// Results from the lookup threads (call every frame).
    pub(super) fn lookup_poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.lookup.rx.try_recv() {
            match msg {
                Msg::Search { ticket, key, result } => {
                    self.lookup.pending.remove(&ticket);
                    if let Ok(list) = &result {
                        self.lookup.searches.insert(key, list.clone());
                    }
                    if let Some(p) = self.lookup.picker.as_mut().filter(|p| p.ticket == ticket) {
                        p.results = Some(result);
                    }
                }
                Msg::Movie { ticket, result } => {
                    self.lookup.pending.remove(&ticket);
                    if let Ok(d) = &result {
                        self.lookup.movies.insert(d.id, d.clone());
                    }
                    if let Some(m) = self.lookup.matcher.as_mut().filter(|m| m.ticket == ticket) {
                        m.set = Some(result.map(|d| MatchSet::for_movie(&m.group, &self.plex.rows, &d)));
                    }
                }
                Msg::Show { ticket, key, result } => {
                    self.lookup.pending.remove(&ticket);
                    if let Ok(d) = &result {
                        self.lookup.shows.insert(key, d.clone());
                    }
                    if let Some(m) = self.lookup.matcher.as_mut().filter(|m| m.ticket == ticket) {
                        m.set = Some(result.map(|(t, eps)| MatchSet::for_show(&m.group, &self.plex.rows, &t, &eps)));
                    }
                }
                Msg::Poster { path, result } => {
                    let p = match result.ok().and_then(|b| decode_jpeg(&b)) {
                        Some(img) => Poster::Ready(ctx.load_texture(format!("tmdb-poster{path}"), img, egui::TextureOptions::LINEAR)),
                        None => Poster::Failed,
                    };
                    self.lookup.posters.insert(path, p);
                }
            }
        }
        if self.lookup.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn next_gen(&mut self) -> u64 {
        self.lookup.ticket += 1;
        self.lookup.pending.insert(self.lookup.ticket);
        self.lookup.ticket
    }

    fn lookup_search(&mut self, ctx: &egui::Context) {
        let Some(auth) = self.tmdb_auth() else { return };
        let lang = self.s.plex.tmdb_language.clone();
        let Some(p) = self.lookup.picker.as_ref() else { return };
        let (kind, q, year) = (p.kind, p.query.trim().to_string(), p.year.trim().parse::<u16>().ok());
        if q.is_empty() {
            return;
        }
        let key = search_key(kind, &q, year, &lang);
        if let Some(hit) = self.lookup.searches.get(&key).cloned() {
            let p = self.lookup.picker.as_mut().unwrap();
            p.results = Some(Ok(hit));
            return;
        }
        let ticket = self.next_gen();
        let p = self.lookup.picker.as_mut().unwrap();
        p.ticket = ticket;
        p.results = None;
        let tx = self.lookup.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = tmdb::search(kind, &q, year, &lang, &auth);
            let _ = tx.send(Msg::Search { ticket, key, result });
            ctx.request_repaint();
        });
    }

    fn lookup_poster(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
        match self.lookup.posters.get(path) {
            Some(Poster::Ready(t)) => return Some(t.clone()),
            Some(_) => return None,
            None => {}
        }
        self.lookup.posters.insert(path.to_string(), Poster::Loading);
        let (tx, ctx, path) = (self.lookup.tx.clone(), ctx.clone(), path.to_string());
        std::thread::spawn(move || {
            let result = tmdb::image(&path, 92);
            let _ = tx.send(Msg::Poster { path, result });
            ctx.request_repaint();
        });
        None
    }

    fn lookup_open(&mut self, ctx: &egui::Context, group: Group) {
        let (kind, query, year) = group.query();
        self.lookup.picker = Some(Picker {
            group,
            kind,
            query,
            year: year.map(|y| y.to_string()).unwrap_or_default(),
            results: None,
            ticket: 0,
        });
        self.lookup_search(ctx);
    }

    fn lookup_details(&mut self, ctx: &egui::Context, group: Group, kind: SearchKind, chosen: Match) {
        let Some(auth) = self.tmdb_auth() else { return };
        let lang = self.s.plex.tmdb_language.clone();
        let id = chosen.id;
        let mut m = Matcher { group, chosen, kind, set: None, ticket: 0 };
        let as_show = kind == SearchKind::Tv;
        if !as_show {
            if let Some(d) = self.lookup.movies.get(&id) {
                m.set = Some(Ok(MatchSet::for_movie(&m.group, &self.plex.rows, d)));
            }
        } else {
            let season = m.group.season.or_else(|| {
                m.group.episodes.first().and_then(|&i| self.plex.rows.get(i)).and_then(|r| r.analysis.season)
            });
            let season = season.unwrap_or(1);
            if let Some((t, eps)) = self.lookup.shows.get(&(id, season)) {
                m.set = Some(Ok(MatchSet::for_show(&m.group, &self.plex.rows, t, eps)));
            }
        }
        if m.set.is_none() {
            let ticket = self.next_gen();
            m.ticket = ticket;
            let (tx, ctx2) = (self.lookup.tx.clone(), ctx.clone());
            let season = m.group.season.unwrap_or(1);
            std::thread::spawn(move || {
                let msg = if as_show {
                    let result = tmdb::tv_details(id, &lang, &auth).and_then(|t| {
                        let eps = if t.seasons.iter().any(|(s, _)| *s == season) || t.seasons.is_empty() {
                            tmdb::season(id, season, &lang, &auth)?
                        } else {
                            vec![]
                        };
                        Ok((t, eps))
                    });
                    Msg::Show { ticket, key: (id, season), result }
                } else {
                    Msg::Movie { ticket, result: tmdb::movie_details(id, &lang, &auth) }
                };
                let _ = tx.send(msg);
                ctx2.request_repaint();
            });
        }
        self.lookup.matcher = Some(m);
    }

    /// "What you have" strip above the Plex table.
    pub(super) fn lookup_strip(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.plex.rows.is_empty() || self.plex.busy() {
            return;
        }
        let sig = rows_signature(&self.plex.rows);
        if sig != self.lookup.groups_sig {
            self.lookup.groups = lookup::groups(&self.plex.rows);
            self.lookup.groups_sig = sig;
        }
        if self.lookup.groups.is_empty() {
            return;
        }
        let ready = self.lookup_ready();
        let hint = if ready { "Search TMDB for this title and confirm the names" } else { NO_KEY_HINT };
        let mut open: Option<Group> = None;
        egui::CollapsingHeader::new(format!("What you have ({})", self.lookup.groups.len()))
            .id_salt("lookup-groups")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("lookup-groups-scroll").max_height(self.s.font_size * 7.5).show(ui, |ui| {
                    for g in &self.lookup.groups {
                        ui.horizontal(|ui| {
                            let resp = ui.add_enabled(ready, egui::Button::new("Look up").small()).on_hover_text(hint);
                            let resp = if ready { resp } else { resp.on_disabled_hover_text(NO_KEY_HINT) };
                            if resp.clicked() {
                                open = Some(g.clone());
                            }
                            let confirmed = g.rows().all(|i| self.plex.rows.get(i).is_some_and(|r| r.analysis.from_tmdb || r.subtitle_of.is_some()));
                            ui.label(g.summary());
                            if confirmed {
                                ui.label(RichText::new("✔ from TMDB").color(green()).small());
                            }
                        });
                    }
                });
                ui.horizontal(|ui| {
                    let sel = self.plex.selected.and_then(|i| lookup::group_of(&self.lookup.groups, i));
                    let resp = ui
                        .add_enabled(ready && sel.is_some(), egui::Button::new("Look up the selected file").small())
                        .on_hover_text("Looks up the title of the file selected in the table");
                    let resp = if ready { resp.on_disabled_hover_text("Select a file in the table first") } else { resp.on_disabled_hover_text(NO_KEY_HINT) };
                    if resp.clicked()
                        && let Some(g) = sel
                    {
                        open = Some(self.lookup.groups[g].clone());
                    }
                    if !self.lookup.status.is_empty() {
                        ui.label(RichText::new(&self.lookup.status).weak().small());
                    }
                });
            });
        if let Some(g) = open {
            self.lookup_open(ctx, g);
        }
    }

    /// The results and match windows (call every frame).
    pub(super) fn lookup_windows(&mut self, ctx: &egui::Context) {
        self.lookup_picker_window(ctx);
        self.lookup_match_window(ctx);
    }

    fn lookup_picker_window(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.lookup.picker.take() else { return };
        let fs = self.s.font_size;
        let mut research = false;
        let mut chosen: Option<Match> = None;
        let mut close = false;
        let mut want_posters: Vec<String> = vec![];
        let posters: HashMap<String, egui::TextureHandle> = self
            .lookup
            .posters
            .iter()
            .filter_map(|(k, v)| if let Poster::Ready(t) = v { Some((k.clone(), t.clone())) } else { None })
            .collect();
        let m = egui::Modal::new(egui::Id::unique("lookup-picker")).show(ctx, |ui| {
            ui.set_width((fs * 40.0).min(ctx.content_rect().width() - 40.0));
            ui.heading("Look up on TMDB");
            ui.label(RichText::new(p.group.summary()).weak());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.kind, SearchKind::Movie, "Movie");
                ui.selectable_value(&mut p.kind, SearchKind::Tv, "TV show");
                let r1 = ui.add(egui::TextEdit::singleline(&mut p.query).desired_width(fs * 16.0).hint_text("Title"));
                ui.label("Year");
                let r2 = ui.add(egui::TextEdit::singleline(&mut p.year).desired_width(fs * 4.5).hint_text("any"));
                let enter = (r1.lost_focus() || r2.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Search again").clicked() || enter {
                    research = true;
                }
            });
            ui.separator();
            match &p.results {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Asking TMDB…");
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(red()));
                    ui.label(RichText::new("Check the internet connection and the key in Tools > Options > TMDB, then search again.").weak().small());
                }
                Some(Ok(list)) if list.is_empty() => {
                    ui.label("Nothing found. Change the title (or clear the year, or switch Movie / TV show) and search again.");
                }
                Some(Ok(list)) => {
                    egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.6).show(ui, |ui| {
                        for mt in list {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal_top(|ui| {
                                    let size = egui::vec2(fs * 3.4, fs * 5.1);
                                    match mt.poster_path.as_ref() {
                                        Some(pp) => match posters.get(pp) {
                                            Some(t) => {
                                                ui.add(egui::Image::new(t).fit_to_exact_size(size));
                                            }
                                            None => {
                                                want_posters.push(pp.clone());
                                                ui.allocate_space(size);
                                            }
                                        },
                                        None => {
                                            ui.allocate_space(size);
                                        }
                                    }
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(plex::name_with_year(&mt.title, mt.year)).strong());
                                            ui.label(RichText::new(format!("TMDB {}", mt.id)).weak().small());
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if ui.button("This one").clicked() {
                                                    chosen = Some(mt.clone());
                                                }
                                            });
                                        });
                                        if !mt.original_title.is_empty() && mt.original_title != mt.title {
                                            ui.label(RichText::new(format!("Original title: {}", mt.original_title)).weak());
                                        }
                                        if !mt.overview.is_empty() {
                                            let mut o: String = mt.overview.chars().take(200).collect();
                                            if mt.overview.chars().count() > 200 {
                                                o.push('…');
                                            }
                                            ui.label(RichText::new(o).small().weak());
                                        }
                                    });
                                });
                            });
                            ui.add_space(2.0);
                        }
                    });
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                ui.label(RichText::new(tmdb::ATTRIBUTION).weak().small());
            });
        });
        let kind = p.kind;
        let group = p.group.clone();
        if chosen.is_none() && !close && !m.should_close() {
            self.lookup.picker = Some(p);
        }
        for pp in want_posters {
            self.lookup_poster(ctx, &pp);
        }
        if research {
            self.lookup_search(ctx);
        }
        if let Some(c) = chosen {
            self.lookup_details(ctx, group, kind, c);
        }
    }

    fn lookup_match_window(&mut self, ctx: &egui::Context) {
        let Some(mut m) = self.lookup.matcher.take() else { return };
        let fs = self.s.font_size;
        let mut ok = false;
        let mut back = false;
        let mut close = false;
        let rows = &self.plex.rows;
        let modal = egui::Modal::new(egui::Id::unique("lookup-match")).show(ctx, |ui| {
            ui.set_width((fs * 52.0).min(ctx.content_rect().width() - 40.0));
            ui.heading(format!("Match your files: {}", plex::name_with_year(&m.chosen.title, m.chosen.year)));
            ui.label(
                RichText::new("Left: your files. Right: what TMDB has. Pick another entry, use Up / Down, or untick a file to leave it as it is.")
                    .weak(),
            );
            ui.separator();
            match m.set.as_mut() {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Getting the details from TMDB…");
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e.as_str()).color(red()));
                }
                Some(Ok(set)) => {
                    let labels: Vec<String> = set.targets.iter().map(Target::label).collect();
                    let mut actions: Vec<RowAction> = vec![];
                    egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.6).show(ui, |ui| {
                        egui::Grid::new("lookup-match-grid").num_columns(4).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                            ui.label("");
                            ui.label(RichText::new("Your file").strong());
                            ui.label("");
                            ui.label(RichText::new("TMDB").strong());
                            ui.end_row();
                            for (pi, pair) in set.pairs.iter().enumerate() {
                                let r = &rows[pair.row];
                                let mut inc = pair.include;
                                if ui.checkbox(&mut inc, "").changed() {
                                    actions.push((pi, None, None, true));
                                }
                                let name = r.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                let what = match r.analysis.kind {
                                    Kind::Episode => format!(
                                        "s{:02}e{:02}",
                                        r.analysis.season.unwrap_or(0),
                                        r.analysis.episode.unwrap_or(0)
                                    ),
                                    Kind::Extra => r.analysis.extra.map(|e| e.singular().to_string()).unwrap_or_default(),
                                    _ => "movie".into(),
                                };
                                let dur = r.duration.map(|d| format!(", {}", organimizer::duration::label(d))).unwrap_or_default();
                                let label = format!("{name}  ({what}{dur})");
                                ui.allocate_ui_with_layout(
                                    egui::vec2(fs * 19.0, fs + 6.0),
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.set_min_width(fs * 19.0);
                                        ui.add(egui::Label::new(label).truncate()).on_hover_text(r.source.display().to_string());
                                    },
                                );
                                ui.horizontal(|ui| {
                                    if ui.small_button("Up").on_hover_text("Previous TMDB entry").clicked() {
                                        actions.push((pi, None, Some(false), false));
                                    }
                                    if ui.small_button("Down").on_hover_text("Next TMDB entry").clicked() {
                                        actions.push((pi, None, Some(true), false));
                                    }
                                });
                                let current = pair.target.map(|t| labels[t].clone()).unwrap_or_else(|| {
                                    if r.analysis.kind == Kind::Extra { "(keep its own name)".into() } else { "(no match)".into() }
                                });
                                egui::ComboBox::from_id_salt(("lookup-target", pi))
                                    .width(fs * 22.0)
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(pair.target.is_none(), "(none)").clicked() {
                                            actions.push((pi, Some(None), None, false));
                                        }
                                        for (t, l) in labels.iter().enumerate() {
                                            if ui.selectable_label(pair.target == Some(t), l).clicked() {
                                                actions.push((pi, Some(Some(t)), None, false));
                                            }
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                    });
                    for (pi, target, step, toggle) in actions {
                        if toggle {
                            set.toggle(pi);
                        }
                        if let Some(t) = target {
                            set.reassign(pi, t);
                        }
                        if let Some(down) = step {
                            set.step(pi, down);
                        }
                    }
                    let missing = set.unmatched_targets().len();
                    if set.kind == GroupKind::Show && missing > 0 {
                        ui.label(RichText::new(format!("{missing} TMDB episodes have no file here (not a problem).")).weak().small());
                    }
                    if set.targets.iter().any(|t| matches!(t, Target::Video { .. })) {
                        ui.label(RichText::new("TMDB videos are only name suggestions for your extras; the files stay the same.").weak().small());
                    }
                    ui.label(format!("{} files get their names from TMDB.", set.applied_count(rows)));
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can = matches!(m.set, Some(Ok(_)));
                if ui.add_enabled(can, egui::Button::new("OK - use these names")).on_hover_text("Writes the names into the preview. Nothing is moved until you click Apply.").clicked() {
                    ok = true;
                }
                if ui.button("Back to the results").clicked() {
                    back = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if ok {
            if let Some(Ok(set)) = &m.set {
                let changed = lookup::apply(&mut self.plex.rows, set, self.s.plex.tmdb_id_tag);
                self.plex.plan_dirty = true;
                self.lookup.status = format!(
                    "{} files named from TMDB ({}). Check the preview, then Apply.",
                    changed.len(),
                    plex::name_with_year(&set.title, set.year)
                );
                self.plex.status = self.lookup.status.clone();
            }
        } else if back {
            let (kind, group) = (m.kind, m.group.clone());
            let query = m.chosen.title.clone();
            self.lookup.picker = Some(Picker { group, kind, query, year: String::new(), results: None, ticket: 0 });
            self.lookup_search(ctx);
        } else if !close && !modal.should_close() {
            self.lookup.matcher = Some(m);
        }
    }
}
