//! TMDB lookup for the Plex tab, pure logic (no network, no GUI):
//!
//! 1. **Analyze**: the scanned rows are grouped into "what you have" items:
//!    a movie with its extras and subtitles, or a TV show per season with
//!    its episodes ([`groups`], [`Group::summary`]).
//! 2. **Look up**: [`Group::query`] is what is searched on TMDB.
//! 3. **Match**: the user's files (left) are paired with TMDB entries
//!    (right): [`MatchSet`] auto-matches, the user reassigns / unticks.
//! 4. **Apply**: [`apply`] writes the confirmed names into the rows, marked
//!    "from TMDB"; the normal preview / apply / undo then runs.
//!
//! Self-contained so the whole feature can be backed out: the GUI part is
//! `gui_lookup.rs`, the HTTP part lives in `tmdb.rs`.

use crate::plex::{self, Confidence, ExtraKind, Kind, Row};
use crate::tmdb::{Episode, MovieDetails, SearchKind, TvDetails, Video};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Movie,
    Show,
}

/// One "what you have" item.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub kind: GroupKind,
    pub title: String,
    pub year: Option<u16>,
    /// Shows: the season of these episodes (None = mixed / unknown).
    pub season: Option<u32>,
    /// Row indices: main movie file(s), episodes, extras, subtitles.
    pub main: Vec<usize>,
    pub episodes: Vec<usize>,
    pub extras: Vec<usize>,
    pub subtitles: Vec<usize>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Group {
    /// "Looks like: Alien (1979) - 1 movie, 6 extras, 2 subtitles" /
    /// "Looks like: The Expanse - Season 2, 13 episodes".
    pub fn summary(&self) -> String {
        let mut parts = vec![];
        let name = plex::name_with_year(&self.title, self.year);
        let head = match (self.kind, self.season) {
            (GroupKind::Show, Some(0)) => format!("{name} - Specials"),
            (GroupKind::Show, Some(s)) => format!("{name} - Season {s}"),
            _ => name,
        };
        match self.kind {
            GroupKind::Movie => parts.push(plural(self.main.len(), "movie", "movies")),
            GroupKind::Show => parts.push(plural(self.episodes.len(), "episode", "episodes")),
        }
        if !self.extras.is_empty() {
            parts.push(plural(self.extras.len(), "extra", "extras"));
        }
        if !self.subtitles.is_empty() {
            parts.push(plural(self.subtitles.len(), "subtitle", "subtitles"));
        }
        let sep = if self.kind == GroupKind::Show { ", " } else { " - " };
        format!("Looks like: {head}{sep}{}", parts.join(", "))
    }

    /// What is searched on TMDB: kind, title and year (when known).
    pub fn query(&self) -> (SearchKind, String, Option<u16>) {
        let kind = match self.kind {
            GroupKind::Movie => SearchKind::Movie,
            GroupKind::Show => SearchKind::Tv,
        };
        (kind, query_text(&self.title), self.year)
    }

    /// All rows of the group (for "already confirmed" checks etc.).
    pub fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.main.iter().chain(&self.episodes).chain(&self.extras).chain(&self.subtitles).copied()
    }
}

/// Search text: the title without edition words or punctuation that
/// confuses the search ("S.H.I.E.L.D" stays, "Alien: Director's Cut" -> "Alien").
pub fn query_text(title: &str) -> String {
    let t = title.trim();
    let lower = t.to_lowercase();
    let mut cut = t.len();
    for w in [" director's cut", " directors cut", " extended", " remastered", " unrated", " theatrical"] {
        if let Some(i) = lower.find(w) {
            cut = cut.min(i);
        }
    }
    t[..cut].trim_end_matches([' ', '-', ':', ',']).trim().to_string()
}

/// Group the scanned rows. Movies by title + year, shows by name + season;
/// extras join the movie / show they belong to; subtitles follow their
/// video's group. Skipped files and rows without a title are left out.
pub fn groups(rows: &[Row]) -> Vec<Group> {
    let mut out: Vec<Group> = vec![];
    let key = |kind: GroupKind, title: &str, year: Option<u16>, season: Option<u32>| {
        (kind, plex::norm(title), if kind == GroupKind::Movie { year } else { None }, season)
    };
    let mut keys: Vec<(GroupKind, String, Option<u16>, Option<u32>)> = vec![];
    let mut find_or_add = |out: &mut Vec<Group>, kind, title: &str, year, season| -> usize {
        let k = key(kind, title, year, season);
        if let Some(i) = keys.iter().position(|x| *x == k) {
            return i;
        }
        keys.push(k);
        out.push(Group {
            kind,
            title: title.to_string(),
            year,
            season,
            main: vec![],
            episodes: vec![],
            extras: vec![],
            subtitles: vec![],
        });
        out.len() - 1
    };
    let mut group_of_row: Vec<Option<usize>> = vec![None; rows.len()];
    // main videos first so extras find them
    for (i, r) in rows.iter().enumerate() {
        let a = &r.analysis;
        if r.subtitle_of.is_some() || a.title.trim().is_empty() {
            continue;
        }
        match a.kind {
            Kind::Movie => {
                let g = find_or_add(&mut out, GroupKind::Movie, &a.title, a.year, None);
                out[g].main.push(i);
                group_of_row[i] = Some(g);
            }
            Kind::Episode if a.air_date.is_none() || a.season.is_some() => {
                let g = find_or_add(&mut out, GroupKind::Show, &a.title, a.year, a.season);
                out[g].episodes.push(i);
                if out[g].year.is_none() {
                    out[g].year = a.year;
                }
                group_of_row[i] = Some(g);
            }
            _ => {}
        }
    }
    for (i, r) in rows.iter().enumerate() {
        let a = &r.analysis;
        if r.subtitle_of.is_some() || a.kind != Kind::Extra || a.title.trim().is_empty() {
            continue;
        }
        let n = plex::norm(&a.title);
        let g = if a.extra_of_show {
            // the show's first season group
            out.iter()
                .enumerate()
                .filter(|(_, g)| g.kind == GroupKind::Show && plex::norm(&g.title) == n)
                .min_by_key(|(_, g)| g.season.unwrap_or(u32::MAX))
                .map(|(j, _)| j)
                .unwrap_or_else(|| find_or_add(&mut out, GroupKind::Show, &a.title, a.year, None))
        } else {
            find_or_add(&mut out, GroupKind::Movie, &a.title, a.year, None)
        };
        out[g].extras.push(i);
        group_of_row[i] = Some(g);
    }
    for (i, r) in rows.iter().enumerate() {
        if let Some(g) = r.subtitle_of.and_then(|v| group_of_row.get(v).copied().flatten()) {
            out[g].subtitles.push(i);
        }
    }
    out.retain(|g| !(g.main.is_empty() && g.episodes.is_empty() && g.extras.is_empty()));
    out.sort_by(|a, b| {
        (a.kind == GroupKind::Show, plex::norm(&a.title), a.season).cmp(&(b.kind == GroupKind::Show, plex::norm(&b.title), b.season))
    });
    out
}

/// The group a row is in (for the per-row "Look up" button).
pub fn group_of(groups: &[Group], row: usize) -> Option<usize> {
    groups.iter().position(|g| g.rows().any(|r| r == row))
}

/// A TMDB entry on the right side of the match window.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The movie itself.
    Movie { title: String, year: Option<u16>, runtime: Option<u32> },
    /// A TMDB video (name hint for an extra).
    Video { name: String, kind: ExtraKind },
    /// An episode of the show.
    Episode(Episode),
}

impl Target {
    pub fn label(&self) -> String {
        match self {
            Target::Movie { title, year, runtime } => format!(
                "Movie: {}{}",
                plex::name_with_year(title, *year),
                runtime.map(|m| format!(" - {}:{:02}", m / 60, m % 60)).unwrap_or_default()
            ),
            Target::Video { name, kind } => format!("{}: {name}", kind.singular()),
            Target::Episode(e) => format!(
                "s{:02}e{:02} - {}{}",
                e.season,
                e.episode,
                if e.name.is_empty() { "(no title)" } else { &e.name },
                e.runtime.map(|m| format!(" ({m} min)")).unwrap_or_default()
            ),
        }
    }
}

/// TMDB video type -> Plex extras folder. None for types that are not extras.
pub fn video_kind(tmdb_type: &str) -> Option<ExtraKind> {
    Some(match tmdb_type.to_lowercase().as_str() {
        "trailer" | "teaser" => ExtraKind::Trailers,
        "featurette" => ExtraKind::Featurettes,
        "behind the scenes" => ExtraKind::BehindTheScenes,
        "clip" => ExtraKind::Scenes,
        "bloopers" | "opening credits" => ExtraKind::Other,
        _ => return None,
    })
}

/// One left-hand row: a file of the group and the TMDB entry it is paired with.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub row: usize,
    /// Index into [`MatchSet::targets`].
    pub target: Option<usize>,
    pub include: bool,
}

/// The side-by-side match: the user's files and the TMDB entries.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchSet {
    pub kind: GroupKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u16>,
    pub targets: Vec<Target>,
    pub pairs: Vec<Pair>,
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !["the", "and", "with", "from", "for"].contains(w))
        .map(String::from)
        .collect()
}

/// Share of the shorter name's words found in the other (0.0 ..= 1.0).
fn similarity(a: &str, b: &str) -> f32 {
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return 0.0;
    }
    let common = wa.iter().filter(|w| wb.contains(w)).count();
    common as f32 / wa.len().min(wb.len()) as f32
}

impl MatchSet {
    /// Movie: the main file(s) pair with the movie (longest first), extras
    /// with a TMDB video of a similar name (hints only; unpaired extras keep
    /// their own name and folder).
    pub fn for_movie(group: &Group, rows: &[Row], d: &MovieDetails) -> MatchSet {
        let mut targets = vec![Target::Movie { title: d.title.clone(), year: d.year, runtime: d.runtime }];
        for Video { name, kind } in &d.videos {
            if let Some(k) = video_kind(kind) {
                targets.push(Target::Video { name: name.clone(), kind: k });
            }
        }
        let mut main = group.main.clone();
        main.sort_by_key(|&i| std::cmp::Reverse(rows[i].duration.unwrap_or(0)));
        let mut pairs: Vec<Pair> = vec![];
        let mut plain_seen = false;
        for &i in &main {
            // editions all pair with the movie; of the plain files only the
            // longest (a second one is probably an extra: the user decides)
            let edition = rows[i].analysis.edition.is_some();
            let take = edition || !plain_seen;
            plain_seen |= !edition;
            pairs.push(Pair { row: i, target: take.then_some(0), include: true });
        }
        for &i in &group.extras {
            pairs.push(Pair { row: i, target: None, include: true });
        }
        let mut m = MatchSet { kind: GroupKind::Movie, tmdb_id: d.id, title: d.title.clone(), year: d.year, targets, pairs };
        m.auto_match_videos(rows);
        m
    }

    /// Show: every episode file pairs with the TMDB episode of the same
    /// season / number.
    pub fn for_show(group: &Group, rows: &[Row], d: &TvDetails, episodes: &[Episode]) -> MatchSet {
        let targets: Vec<Target> = episodes.iter().cloned().map(Target::Episode).collect();
        let mut eps = group.episodes.clone();
        eps.sort_by_key(|&i| (rows[i].analysis.season, rows[i].analysis.episode));
        let pairs = eps
            .iter()
            .map(|&i| {
                let a = &rows[i].analysis;
                let t = targets.iter().position(|t| {
                    matches!(t, Target::Episode(e) if Some(e.season) == a.season && Some(e.episode) == a.episode)
                });
                Pair { row: i, target: t, include: true }
            })
            .chain(group.extras.iter().map(|&i| Pair { row: i, target: None, include: true }))
            .collect();
        MatchSet { kind: GroupKind::Show, tmdb_id: d.id, title: d.name.clone(), year: d.year, targets, pairs }
    }

    fn auto_match_videos(&mut self, rows: &[Row]) {
        for p in 0..self.pairs.len() {
            let r = &rows[self.pairs[p].row];
            if r.analysis.kind != Kind::Extra {
                continue;
            }
            let name = r.analysis.extra_name.clone().unwrap_or_default();
            let own = r.analysis.extra;
            let best = self
                .targets
                .iter()
                .enumerate()
                .filter(|(t, _)| !self.pairs.iter().any(|x| x.target == Some(*t)))
                .filter_map(|(t, tg)| match tg {
                    // same extras type breaks a tie
                    Target::Video { name: n, kind } => {
                        Some((t, similarity(&name, n) + if Some(*kind) == own { 0.05 } else { 0.0 }))
                    }
                    _ => None,
                })
                .filter(|(_, s)| *s >= 0.6)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((t, _)) = best {
                self.pairs[p].target = Some(t);
            }
        }
    }

    /// Pair a file with a TMDB entry (None = no TMDB entry). If another file
    /// had that entry, the two swap (an entry is used once; the movie itself
    /// can take several files only for different editions).
    pub fn reassign(&mut self, pair: usize, target: Option<usize>) {
        if pair >= self.pairs.len() || target.is_some_and(|t| t >= self.targets.len()) {
            return;
        }
        let shareable = matches!(target.and_then(|t| self.targets.get(t)), Some(Target::Movie { .. }));
        if let Some(t) = target
            && !shareable
            && let Some(other) = self.pairs.iter().position(|p| p.target == Some(t))
            && other != pair
        {
            self.pairs[other].target = self.pairs[pair].target;
        }
        self.pairs[pair].target = target;
    }

    /// Move a file's pairing to the previous / next TMDB entry (the
    /// up / down buttons): episode 5 -> 4 or 6.
    pub fn step(&mut self, pair: usize, down: bool) {
        let Some(p) = self.pairs.get(pair) else { return };
        let n = self.targets.len();
        if n == 0 {
            return;
        }
        let next = match (p.target, down) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(t), true) => (t + 1).min(n - 1),
            (Some(t), false) => t.saturating_sub(1),
        };
        self.reassign(pair, Some(next));
    }

    pub fn toggle(&mut self, pair: usize) {
        if let Some(p) = self.pairs.get_mut(pair) {
            p.include = !p.include;
        }
    }

    /// Ticked rows that have a TMDB entry, plus ticked extras (they get the
    /// confirmed movie / show name even without an entry).
    pub fn applied_count(&self, rows: &[Row]) -> usize {
        self.pairs.iter().filter(|p| p.include && (p.target.is_some() || rows[p.row].analysis.kind == Kind::Extra)).count()
    }

    /// TMDB episodes no file is paired with ("missing" episodes).
    pub fn unmatched_targets(&self) -> Vec<usize> {
        (0..self.targets.len()).filter(|t| !self.pairs.iter().any(|p| p.target == Some(*t))).collect()
    }
}

/// Write the confirmed names into the rows. `tag` adds the TMDB id to the
/// folder name (`Alien (1979) {tmdb-348}`). Returns the changed rows.
/// Rows stay ordinary suggestions: nothing is moved here.
pub fn apply(rows: &mut [Row], m: &MatchSet, tag: bool) -> Vec<usize> {
    let mut changed = vec![];
    let name = plex::name_with_year(&m.title, m.year);
    let id_note = format!("Confirmed on TMDB: {name} (TMDB id {})", m.tmdb_id);
    for p in m.pairs.iter().filter(|p| p.include) {
        let Some(r) = rows.get_mut(p.row) else { continue };
        let a = &mut r.analysis;
        let target = p.target.and_then(|t| m.targets.get(t));
        match (a.kind, target) {
            (Kind::Movie, Some(Target::Movie { .. })) | (Kind::Episode, Some(Target::Episode(_))) | (Kind::Extra, _) => {}
            (Kind::Movie, Some(Target::Video { name: n, kind })) => {
                // a second main file paired with a TMDB video: it is an extra
                a.kind = Kind::Extra;
                a.extra = Some(*kind);
                a.extra_name = Some(n.clone());
                a.edition = None;
            }
            _ => continue,
        }
        a.title = m.title.clone();
        a.year = m.year;
        a.tmdb_id = tag.then_some(m.tmdb_id);
        a.from_tmdb = true;
        a.notes.retain(|n| !n.starts_with("No year") && !n.starts_with("Title and year taken") && !n.starts_with("Show name"));
        a.why.retain(|w| !w.starts_with("Confirmed on TMDB"));
        match target {
            Some(Target::Episode(e)) => {
                a.season = Some(e.season);
                a.episode = Some(e.episode);
                if !e.name.is_empty() {
                    a.episode_title = Some(e.name.clone());
                }
                a.because(format!("{id_note}, s{:02}e{:02} \"{}\"", e.season, e.episode, e.name));
                a.confidence = Confidence::High;
            }
            Some(Target::Video { name: n, kind }) if a.kind == Kind::Extra => {
                a.extra = Some(*kind);
                a.extra_name = Some(plex::safe_component(n));
                a.extra_of_show = m.kind == GroupKind::Show;
                a.because(format!("{id_note}; name from the TMDB video \"{n}\" -> {}", kind.folder()));
                a.confidence = Confidence::High;
            }
            _ if a.kind == Kind::Extra => {
                a.extra_of_show = m.kind == GroupKind::Show;
                a.because(format!("{id_note}; extras type kept from the file name / folder"));
                a.confidence = a.confidence.max(Confidence::Medium);
            }
            _ => {
                a.because(id_note.clone());
                a.confidence = Confidence::High;
            }
        }
        changed.push(p.row);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmdb;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tmdb").join(name)).unwrap()
    }

    fn scan(files: &[&str]) -> Vec<Row> {
        let root = PathBuf::from("/lib");
        plex::analyze_files(&root, &files.iter().map(|f| root.join(f)).collect::<Vec<_>>())
    }

    fn alien_rows() -> Vec<Row> {
        scan(&[
            "Alien.1979.Directors.Cut.1080p.mkv",
            "Alien 1979.mkv",
            "alien making of.mkv",
            "Alien.1979.Trailer.mp4",
            "Alien (1979) Featurette - The Beast Within.mkv",
            "Alien.1979.en.srt",
            "Alien 1979.de.srt",
        ])
    }

    fn expanse_rows() -> Vec<Row> {
        scan(&[
            "TV/The Expanse/Season2/Expanse - 2x01 - Safe.mkv",
            "TV/The Expanse/Season2/The Expanse S2 E2.mkv",
            "TV/The Expanse/Season2/The.Expanse.S02E05.mkv",
            "TV/The Expanse/Season2/The.Expanse.S02E05.en.srt",
            "TV/The Expanse/Season 3/The Expanse S03E01.mkv",
            "TV/The Expanse/Featurettes/Inside the Expanse.mkv",
        ])
    }

    #[test]
    fn grouping_and_summaries() {
        let rows = alien_rows();
        let g = groups(&rows);
        assert_eq!(g.len(), 1, "{g:#?}");
        assert_eq!(g[0].kind, GroupKind::Movie);
        assert_eq!((g[0].main.len(), g[0].extras.len(), g[0].subtitles.len()), (2, 3, 2));
        assert_eq!(g[0].summary(), "Looks like: Alien (1979) - 2 movies, 3 extras, 2 subtitles");

        let rows = expanse_rows();
        let g = groups(&rows);
        assert_eq!(g.len(), 2, "{g:#?}");
        assert_eq!(g[0].summary(), "Looks like: The Expanse - Season 2, 3 episodes, 1 extra, 1 subtitle");
        assert_eq!(g[1].summary(), "Looks like: The Expanse - Season 3, 1 episode");
        let s3 = rows.iter().position(|r| r.source.ends_with("The Expanse S03E01.mkv")).unwrap();
        assert_eq!(group_of(&g, s3), Some(1));
    }

    #[test]
    fn queries() {
        let g = groups(&alien_rows());
        assert_eq!(g[0].query(), (SearchKind::Movie, "Alien".to_string(), Some(1979)));
        let g = groups(&expanse_rows());
        assert_eq!(g[0].query(), (SearchKind::Tv, "The Expanse".to_string(), None));
        assert_eq!(query_text("Alien Director's Cut"), "Alien");
        assert_eq!(query_text("S.H.I.E.L.D"), "S.H.I.E.L.D");
        let auth = tmdb::Auth::from_key(&"0123456789abcdef".repeat(2)).unwrap();
        let url = tmdb::search_url(SearchKind::Movie, "Alien", Some(1979), "en-US", &auth);
        assert!(url.ends_with("/search/movie?query=Alien&include_adult=false&language=en-US&year=1979&api_key=0123456789abcdef0123456789abcdef"), "{url}");
    }

    #[test]
    fn parses_fixtures() {
        let found = tmdb::parse_search(&fixture("search_movie_alien.json"), 8).unwrap();
        assert_eq!(found[0].id, 348);
        assert_eq!(found[0].title, "Alien");
        assert_eq!(found[0].year, Some(1979));
        assert_eq!(found[0].poster_path.as_deref(), Some("/alien-poster.jpg"));
        assert_eq!(found[1].poster_path, None);
        let tv = tmdb::parse_search(&fixture("search_tv_expanse.json"), 8).unwrap();
        assert_eq!((tv[0].id, tv[0].title.as_str(), tv[0].year), (63639, "The Expanse", Some(2015)));
        let d = tmdb::parse_movie_details(&fixture("movie_348.json")).unwrap();
        assert_eq!((d.title.as_str(), d.year, d.runtime), ("Alien", Some(1979), Some(117)));
        assert_eq!(d.videos.len(), 4);
        assert_eq!(d.videos[1], Video { name: "The Beast Within: The Making of Alien".into(), kind: "Behind the Scenes".into() });
        let t = tmdb::parse_tv_details(&fixture("tv_63639.json")).unwrap();
        assert_eq!(t.seasons, vec![(0, 2), (1, 10), (2, 13), (3, 13)]);
        let s = tmdb::parse_season(&fixture("tv_63639_season_2.json")).unwrap();
        assert_eq!(s.len(), 13);
        assert_eq!(s[4], Episode { season: 2, episode: 5, name: "Home".into(), runtime: Some(44) });
        assert!(tmdb::parse_season("{\"episodes\": 5}").is_err());
        assert!(tmdb::parse_search("<html>", 8).unwrap_err().contains("Unexpected answer"));
    }

    #[test]
    fn episode_mapping_reassign_and_apply() {
        let mut rows = expanse_rows();
        let g = groups(&rows);
        let t = tmdb::parse_tv_details(&fixture("tv_63639.json")).unwrap();
        let s = tmdb::parse_season(&fixture("tv_63639_season_2.json")).unwrap();
        let mut m = MatchSet::for_show(&g[0], &rows, &t, &s);
        let label = |m: &MatchSet, p: usize| m.pairs[p].target.map(|t| m.targets[t].label()).unwrap_or_default();
        assert_eq!(label(&m, 0), "s02e01 - Safe (45 min)");
        assert_eq!(label(&m, 1), "s02e02 - Doors & Corners (43 min)");
        assert_eq!(label(&m, 2), "s02e05 - Home (44 min)");
        assert_eq!(m.pairs[3].target, None, "the extra has no episode");
        assert_eq!(m.unmatched_targets().len(), 10);
        // the user says file 2 is really episode 4: down / up buttons
        m.step(2, false);
        assert_eq!(label(&m, 2), "s02e04 - Godspeed (42 min)");
        // dropdown onto an entry another file has: they swap
        m.reassign(2, Some(0));
        assert_eq!(label(&m, 2), "s02e01 - Safe (45 min)");
        assert_eq!(label(&m, 0), "s02e04 - Godspeed (42 min)");
        m.reassign(0, Some(0));
        m.reassign(2, Some(4));
        m.toggle(1);
        let changed = apply(&mut rows, &m, true);
        assert_eq!(changed.len(), 3, "2 episodes + the extra (episode 2 unticked)");
        let rel = |r: &Row| plex::relative_path(&r.analysis, ".mkv").unwrap().to_string_lossy().replace('\\', "/");
        let e1 = &rows[m.pairs[0].row];
        assert_eq!(rel(e1), "The Expanse (2015) {tmdb-63639}/Season 02/The Expanse (2015) - s02e01 - Safe.mkv");
        assert!(e1.analysis.from_tmdb && e1.analysis.confidence == Confidence::High);
        assert!(e1.analysis.why_text().contains("Confirmed on TMDB: The Expanse (2015) (TMDB id 63639), s02e01 \"Safe\""));
        assert_eq!(rel(&rows[m.pairs[2].row]), "The Expanse (2015) {tmdb-63639}/Season 02/The Expanse (2015) - s02e05 - Home.mkv");
        assert!(!rows[m.pairs[1].row].analysis.from_tmdb, "unticked rows are not touched");
        assert_eq!(
            rel(&rows[m.pairs[3].row]),
            "The Expanse (2015) {tmdb-63639}/Featurettes/Inside the Expanse.mkv"
        );
    }

    #[test]
    fn movie_match_hints_and_apply() {
        let mut rows = alien_rows();
        let g = groups(&rows);
        let d = tmdb::parse_movie_details(&fixture("movie_348.json")).unwrap();
        let m = MatchSet::for_movie(&g[0], &rows, &d);
        assert_eq!(m.targets[0].label(), "Movie: Alien (1979) - 1:57");
        // both main files are paired with the movie: one is an edition
        assert_eq!(m.pairs[0].target, Some(0));
        assert_eq!(m.pairs[1].target, Some(0));
        let featurette = m.pairs.iter().find(|p| rows[p.row].source.to_string_lossy().contains("Beast Within")).unwrap();
        assert!(matches!(&m.targets[featurette.target.unwrap()], Target::Video { kind: ExtraKind::Featurettes, .. }));
        let making = m.pairs.iter().find(|p| rows[p.row].source.to_string_lossy().contains("making of")).unwrap();
        // "Making of" and the TMDB "The Beast Within: The Making of Alien" (same type)
        assert!(matches!(&m.targets[making.target.unwrap()], Target::Video { kind: ExtraKind::BehindTheScenes, .. }));
        let trailer = m.pairs.iter().find(|p| rows[p.row].source.to_string_lossy().contains("Trailer")).unwrap();
        assert_eq!(m.targets[trailer.target.unwrap()].label(), "Trailer: Alien Official Trailer");
        let changed = apply(&mut rows, &m, false);
        assert_eq!(changed.len(), 5);
        let rel = |r: &Row| {
            let n = r.source.file_name().unwrap().to_string_lossy().into_owned();
            plex::relative_path(&r.analysis, &n[n.rfind('.').unwrap()..]).unwrap().to_string_lossy().replace('\\', "/")
        };
        let out: Vec<String> = rows.iter().filter(|r| r.subtitle_of.is_none()).map(rel).collect();
        assert!(out.contains(&"Alien (1979)/Alien (1979) {edition-Director's Cut}.mkv".to_string()), "{out:#?}");
        assert!(out.contains(&"Alien (1979)/Alien (1979).mkv".to_string()), "{out:#?}");
        assert!(out.contains(&"Alien (1979)/Featurettes/The Beast Within.mkv".to_string()), "{out:#?}");
        assert!(out.contains(&"Alien (1979)/Trailers/Alien Official Trailer.mp4".to_string()), "{out:#?}");
        assert!(out.iter().any(|o| o.starts_with("Alien (1979)/Behind The Scenes/The Beast Within")), "{out:#?}");
        assert!(rows.iter().filter(|r| r.subtitle_of.is_none()).all(|r| r.analysis.from_tmdb));
    }

    #[test]
    fn tmdb_video_types() {
        assert_eq!(video_kind("Behind the Scenes"), Some(ExtraKind::BehindTheScenes));
        assert_eq!(video_kind("Teaser"), Some(ExtraKind::Trailers));
        assert_eq!(video_kind("Recap"), None);
    }
}
