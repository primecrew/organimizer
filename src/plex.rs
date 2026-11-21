//! Plex mode (3.0): work out what a video file is from its name and the
//! folders around it, and propose the layout Plex expects.
//!
//! * Movies:   `Title (Year)/Title (Year) {edition-X}.ext`
//! * Episodes: `Show (Year)/Season 01/Show (Year) - s01e02[-e03][ - Title].ext`
//!   (date-based shows: `Show/Season 2024/Show - 2024-03-15.ext`; specials
//!   go to `Season 00`)
//! * Extras:   `Title (Year)/Featurettes/Name.ext` (and the other Plex
//!   extras folders), next to the movie or show they belong to.
//! * Subtitles follow their video and keep language / forced / SDH tags
//!   (`Title (Year).en.forced.srt`).
//!
//! Everything here is pure string work: nothing touches the disk, so the
//! parser is unit tested on a large sample set. The GUI turns the result
//! into a normal [`crate::plan::Plan`], so Apply / Undo / never-overwrite
//! work exactly like the rest of the app.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::filetypes::VIDEO_EXTS;
use crate::plan::{Plan, PlanItem, Status, split_name, unique_dest_for};

pub const SUBTITLE_EXTS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "vtt", "sup", "smi"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Movie,
    Episode,
    Extra,
    /// Sample clips and files that could not be identified: not moved.
    Skip,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Movie, Kind::Episode, Kind::Extra, Kind::Skip];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Movie => "Movie",
            Kind::Episode => "TV episode",
            Kind::Extra => "Extra",
            Kind::Skip => "Skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraKind {
    BehindTheScenes,
    DeletedScenes,
    Featurettes,
    Interviews,
    Scenes,
    Shorts,
    Trailers,
    Other,
}

impl ExtraKind {
    pub const ALL: [ExtraKind; 8] = [
        ExtraKind::BehindTheScenes,
        ExtraKind::DeletedScenes,
        ExtraKind::Featurettes,
        ExtraKind::Interviews,
        ExtraKind::Scenes,
        ExtraKind::Shorts,
        ExtraKind::Trailers,
        ExtraKind::Other,
    ];
    /// Plex extras folder name.
    pub fn folder(self) -> &'static str {
        match self {
            ExtraKind::BehindTheScenes => "Behind The Scenes",
            ExtraKind::DeletedScenes => "Deleted Scenes",
            ExtraKind::Featurettes => "Featurettes",
            ExtraKind::Interviews => "Interviews",
            ExtraKind::Scenes => "Scenes",
            ExtraKind::Shorts => "Shorts",
            ExtraKind::Trailers => "Trailers",
            ExtraKind::Other => "Other",
        }
    }
    fn singular(self) -> &'static str {
        match self {
            ExtraKind::BehindTheScenes => "Behind the Scenes",
            ExtraKind::DeletedScenes => "Deleted Scene",
            ExtraKind::Featurettes => "Featurette",
            ExtraKind::Interviews => "Interview",
            ExtraKind::Scenes => "Scene",
            ExtraKind::Shorts => "Short",
            ExtraKind::Trailers => "Trailer",
            ExtraKind::Other => "Extra",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => "High",
            Confidence::Medium => "Medium",
            Confidence::Low => "Low",
        }
    }
}

/// What a video file was recognized as. Every field can be overridden in
/// the preview; [`relative_path`] turns it into the Plex path.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub kind: Kind,
    /// Movie title, or the show name for episodes (and the movie/show an
    /// extra belongs to).
    pub title: String,
    pub year: Option<u16>,
    pub edition: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// Last episode of a multi-episode file (`s01e01-e02`).
    pub episode_end: Option<u32>,
    /// Date-based shows (`Show - 2024-03-15`).
    pub air_date: Option<(i32, u32, u32)>,
    pub episode_title: Option<String>,
    pub extra: Option<ExtraKind>,
    /// Extras: the name the file gets inside the extras folder.
    pub extra_name: Option<String>,
    /// Extras: does it belong to a show (true) or a movie (false)?
    pub extra_of_show: bool,
    pub confidence: Confidence,
    pub notes: Vec<String>,
}

impl Analysis {
    fn new(kind: Kind) -> Self {
        Analysis {
            kind,
            title: String::new(),
            year: None,
            edition: None,
            season: None,
            episode: None,
            episode_end: None,
            air_date: None,
            episode_title: None,
            extra: None,
            extra_name: None,
            extra_of_show: false,
            confidence: Confidence::Low,
            notes: vec![],
        }
    }
}

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).expect("valid regex"));
    };
}

// "strong" junk: never part of a real title
re!(
    JUNK_STRONG,
    r"(?i)(?:^|[ ._\-\[\(])(2160p|1440p|1080p|1080i|720p|576p|480p|360p|4k|uhd|x\.?26[45]|h\.?26[45]|hevc|avc|xvid|divx|av1|vp9|10bit|10-bit|8bit|hdr10\+?|hdr|dovi|blu-?ray|bdrip|brrip|bdremux|remux|web-?dl|web-?rip|hdtv|pdtv|dvdrip|dvdscr|dvd9|dvd5|hdrip|hdcam|telesync|aac2?\.?0?|e?ac3|dts(?:-?hd)?(?:-?ma)?|ddp?\.?[257]\.?[01]|ddp|atmos|truehd|5\.1|7\.1|2\.0|proper|repack|rerip|yts(?:\.[a-z]{2})?|yify|rarbg|eztv|ettv|amzn|dsnp|hmax|atvp|nf|hulu|pcok|sdr)(?:$|[ ._\-\]\)])"
);
// "weak" junk: only junk after the title (year / episode number)
re!(
    JUNK_WEAK,
    r"(?i)(?:^|[ ._\-\[\(])(web|dvd|bluray|cam|ts|dv|multi|dubbed|subbed|subs|internal|limited|complete|extended|unrated|uncut|remastered|imax|criterion|directors?|theatrical|final|ultimate|special|collectors?|1080|720|2160)(?:$|[ ._\-\]\)])"
);
re!(BRACKETS, r"\[[^\]]*\]|\{[^}]*\}");
re!(EDITION_TAG, r"(?i)\{edition-([^}]+)\}");
re!(YEAR, r"(?:^|[ ._\-\(\[])((?:19|20)\d{2})(?:$|[ ._\-\)\]])");
re!(SXE, r"(?i)(?:^|[^a-z0-9])s(\d{1,2})[ ._-]?e(\d{1,4})");
re!(SXE_CONT, r"(?i)^(?:[ ._-]*-?[ ._-]*e(\d{1,4})|-(\d{1,4}))");
re!(NXN, r"(?i)(?:^|[^a-z0-9])(\d{1,2})x(\d{2,3})(?:$|[^a-z0-9])");
re!(NXN_CONT, r"(?i)^(?:[ ._-]*x(\d{2,3})|-(\d{2,3}))");
re!(SEASON_EP_WORDS, r"(?i)\bseason[ ._-]*(\d{1,2})[ ._-]*(?:episode|ep)[ ._-]*(\d{1,4})");
re!(DATE, r"(?:^|[^0-9])((?:19|20)\d{2})[ ._-](\d{2})[ ._-](\d{2})(?:$|[^0-9])");
re!(EP_ONLY, r"(?i)(?:^|[^a-z0-9])(?:episode|ep|e)[ ._-]?(\d{1,4})(?:$|[^a-z0-9])");
re!(BARE_NUM, r"(?:^|[ ._-])(\d{1,3})(?:$|[ ._-])");
re!(SEASON_DIR, r"(?i)^(?:season|series|staffel|saison|temporada|seizoen)[ ._-]*(\d{1,4})(?:\b.*)?$|^s(\d{1,2})$");
re!(SEASON_IN_NAME, r"(?i)(?:^|[ ._-])(?:s(\d{1,2})|season[ ._-]?(\d{1,2}))(?:$|[ ._-])");
re!(INLINE_EXTRA, r"(?i)-(behindthescenes|deleted|featurette|interview|scene|short|trailer|other)$");
re!(SAMPLE, r"(?i)(?:^|[ ._\-\[\(])sample(?:$|[ ._\-\]\)])");
re!(SPACES, r"\s+");
re!(GENERIC_TRACK, r"(?i)^(?:title|t|track|vts)[ _-]?\d+(?:[ _-]\d+)*$");
re!(
    EDITION_WORDS,
    r"(?i)\b(director'?s[ ._-]cut|extended[ ._-](?:cut|edition)|extended|final[ ._-]cut|theatrical(?:[ ._-]cut)?|unrated|uncut|ultimate[ ._-](?:cut|edition)|special[ ._-]edition|collector'?s[ ._-]edition|anniversary[ ._-]edition|criterion(?:[ ._-]collection)?|remastered|imax(?:[ ._-]edition)?)\b"
);

fn max_year() -> u16 {
    crate::max_year().clamp(1990, 9999) as u16
}

fn is_video(ext: &str) -> bool {
    VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

pub fn is_subtitle(ext: &str) -> bool {
    SUBTITLE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// `.`/`_` to spaces, brackets removed, collapsed spaces, trimmed of
/// separators. Dots inside abbreviations like `S.H.I.E.L.D.` are kept.
pub fn tidy(s: &str) -> String {
    let s = BRACKETS.replace_all(s, " ");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            out.push(' ');
        } else if c == '.' {
            let single_before = i >= 1 && chars[i - 1].is_alphabetic() && (i < 2 || !chars[i - 2].is_alphanumeric());
            let single_after = i + 1 < chars.len()
                && chars[i + 1].is_alphabetic()
                && (i + 2 >= chars.len() || !chars[i + 2].is_alphanumeric());
            if single_before && (single_after || i + 1 == chars.len()) {
                out.push('.');
            } else {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    let out = out.replace("()", " ");
    let out = SPACES.replace_all(&out, " ");
    let t = out.trim_matches(|c: char| c.is_whitespace() || "-–—,:;+~(".contains(c));
    // a lone trailing ")" left over from "(2010)" handling
    let t = if t.ends_with(')') && !t.contains('(') { t.trim_end_matches(')') } else { t };
    t.trim().to_string()
}

/// Title-case text that is all lower case (or all upper case); mixed-case
/// input is kept as typed.
pub fn smart_case(s: &str) -> String {
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    let all_lower = letters.iter().all(|c| c.is_lowercase());
    let all_upper = letters.len() > 4 && letters.iter().all(|c| c.is_uppercase()) && !s.contains('.');
    if !(all_lower || all_upper) {
        return s.to_string();
    }
    const SMALL: &[&str] =
        &["a", "an", "and", "as", "at", "but", "by", "for", "in", "of", "on", "or", "the", "to", "vs", "with"];
    s.split(' ')
        .enumerate()
        .map(|(i, w)| {
            let lw = w.to_lowercase();
            if i > 0 && SMALL.contains(&lw.as_str()) {
                return lw;
            }
            let mut c = lw.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn earliest_junk(s: &str, from: usize, weak_too: bool) -> Option<usize> {
    let tail = s.get(from..)?;
    let strong = JUNK_STRONG.find(tail).map(|m| m.start());
    let weak = if weak_too { JUNK_WEAK.find(tail).map(|m| m.start()) } else { None };
    match (strong, weak) {
        (Some(a), Some(b)) => Some(from + a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(from + a),
        _ => None,
    }
}

/// Last plausible year before `limit` (not at the very start, so titles
/// like "1917" or "2012" stay titles).
fn find_year(s: &str, limit: usize) -> Option<(usize, usize, u16)> {
    let mut found = None;
    let mut pos = 0;
    while pos < s.len() {
        let Some(c) = YEAR.captures_at(s, pos) else { break };
        let m = c.get(1).unwrap();
        if m.start() >= limit {
            break;
        }
        let y: u16 = m.as_str().parse().unwrap_or(0);
        if m.start() > 0 && (1900..=max_year()).contains(&y) {
            found = Some((m.start(), m.end(), y));
        }
        pos = m.start() + 1;
    }
    found
}

fn find_edition(s: &str) -> Option<String> {
    if let Some(c) = EDITION_TAG.captures(s) {
        return Some(c[1].trim().to_string());
    }
    let m = EDITION_WORDS.find(s)?;
    let t = m.as_str().replace(['.', '_', '-'], " ").to_lowercase();
    let t = match t.as_str() {
        "extended" => "Extended Edition".to_string(),
        "theatrical" => "Theatrical Cut".to_string(),
        "criterion" => "Criterion Collection".to_string(),
        "imax" => "IMAX".to_string(),
        "directors cut" | "director's cut" => "Director's Cut".to_string(),
        "collectors edition" | "collector's edition" => "Collector's Edition".to_string(),
        _ => smart_case(&t),
    };
    Some(t.replace("Imax", "IMAX"))
}

/// Split "Show Name (2019) ..." / "Show.Name.2019...." into name + year,
/// dropping release junk.
fn title_and_year(s: &str) -> (String, Option<u16>) {
    let junk = earliest_junk(s, 1, false).unwrap_or(s.len());
    if let Some((a, _, y)) = find_year(s, junk) {
        let t = tidy(&s[..a]);
        if !t.is_empty() {
            return (smart_case(&t), Some(y));
        }
    }
    let cut = earliest_junk(s, 1, false).filter(|&j| j > 0).unwrap_or(s.len());
    (smart_case(&tidy(&s[..cut])), None)
}

fn season_of_dir(name: &str) -> Option<u32> {
    let n = name.trim();
    if n.eq_ignore_ascii_case("specials") || n.eq_ignore_ascii_case("special") {
        return Some(0);
    }
    let c = SEASON_DIR.captures(n)?;
    c.get(1).or(c.get(2)).and_then(|m| m.as_str().parse().ok())
}

fn extra_of_dir(name: &str) -> Option<ExtraKind> {
    let n = name.trim().to_lowercase().replace(['_', '.'], " ");
    Some(match n.as_str() {
        "behind the scenes" | "behindthescenes" | "making of" | "making-of" | "bts" => ExtraKind::BehindTheScenes,
        "deleted scenes" | "deleted scene" | "deleted" => ExtraKind::DeletedScenes,
        "featurettes" | "featurette" => ExtraKind::Featurettes,
        "interviews" | "interview" => ExtraKind::Interviews,
        "scenes" => ExtraKind::Scenes,
        "shorts" | "short films" => ExtraKind::Shorts,
        "trailers" | "trailer" | "teasers" => ExtraKind::Trailers,
        "other" | "others" | "extras" | "extra" | "bonus" | "bonus features" | "special features" => ExtraKind::Other,
        _ => return None,
    })
}

fn is_subs_dir(name: &str) -> bool {
    matches!(name.trim().to_lowercase().as_str(), "subs" | "sub" | "subtitles" | "subtitle" | "srt")
}

fn extra_keyword(s: &str) -> Option<ExtraKind> {
    let l = format!(" {} ", s.to_lowercase().replace(['.', '_', '-', '(', ')', '[', ']'], " "));
    let l = SPACES.replace_all(&l, " ");
    let has = |w: &str| l.contains(&format!(" {w} "));
    if has("trailer") || has("teaser") || has("trailers") {
        Some(ExtraKind::Trailers)
    } else if has("behind the scenes") || has("making of") || has("bts") || has("behindthescenes") {
        Some(ExtraKind::BehindTheScenes)
    } else if has("deleted scene") || has("deleted scenes") || has("deleted") {
        Some(ExtraKind::DeletedScenes)
    } else if has("featurette") || has("featurettes") {
        Some(ExtraKind::Featurettes)
    } else if has("interview") || has("interviews") {
        Some(ExtraKind::Interviews)
    } else if has("gag reel") || has("bloopers") || has("blooper") || has("outtakes") || has("bonus") {
        Some(ExtraKind::Other)
    } else if has("short film") {
        Some(ExtraKind::Shorts)
    } else {
        None
    }
}

const GENERIC_NAMES: &[&str] = &[
    "movie",
    "movies",
    "film",
    "films",
    "video",
    "videos",
    "main",
    "feature",
    "main feature",
    "title",
    "untitled",
    // container folders that are never a title
    "downloads",
    "download",
    "tv",
    "tv shows",
    "shows",
    "series",
    "media",
    "plex",
    "library",
    "new",
    "incoming",
    "complete",
    "completed",
    "torrents",
    "usenet",
    "temp",
    "tmp",
    "desktop",
    "documents",
    "unsorted",
    "to sort",
];

fn looks_generic(t: &str) -> bool {
    let l = t.to_lowercase();
    l.chars().filter(|c| c.is_alphanumeric()).count() < 2
        || GENERIC_NAMES.contains(&l.as_str())
        || GENERIC_TRACK.is_match(&l)
        || (l.len() <= 2 && l.chars().all(|c| c.is_ascii_digit()))
}

/// Normalized key for comparing titles.
pub fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

struct TvHit {
    start: usize,
    end: usize,
    season: Option<u32>,
    episode: Option<u32>,
    episode_end: Option<u32>,
    date: Option<(i32, u32, u32)>,
    strong: bool,
}

fn continuation(re: &Regex, s: &str, mut end: usize) -> (Option<u32>, usize) {
    let mut last = None;
    while let Some(c) = re.captures(&s[end..]) {
        let m = c.get(0).unwrap();
        let num = c.get(1).or(c.get(2)).unwrap();
        // the "-02" form must not run into a letter / digit (e.g. -720p)
        let after = s[end + m.end()..].chars().next();
        if c.get(2).is_some() && after.is_some_and(|ch| ch.is_alphanumeric()) {
            break;
        }
        last = num.as_str().parse().ok();
        end += m.end();
    }
    (last, end)
}

fn hit(
    start: usize,
    end: usize,
    season: Option<u32>,
    episode: Option<u32>,
    episode_end: Option<u32>,
    strong: bool,
) -> TvHit {
    TvHit { start, end, season, episode, episode_end, date: None, strong }
}

fn find_tv(s: &str) -> Option<TvHit> {
    if let Some(c) = SXE.captures(s) {
        let m0 = c.get(0).unwrap();
        let start = m0.start() + usize::from(!m0.as_str().starts_with(|ch: char| ch.is_ascii_alphanumeric()));
        let (cont, end) = continuation(&SXE_CONT, s, m0.end());
        return Some(hit(start, end, c[1].parse().ok(), c[2].parse().ok(), cont, true));
    }
    if let Some(c) = SEASON_EP_WORDS.captures(s) {
        let m0 = c.get(0).unwrap();
        return Some(hit(m0.start(), m0.end(), c[1].parse().ok(), c[2].parse().ok(), None, true));
    }
    if let Some(c) = NXN.captures(s) {
        let (m1, m2) = (c.get(1).unwrap(), c.get(2).unwrap());
        let (cont, end) = continuation(&NXN_CONT, s, m2.end());
        return Some(hit(m1.start(), end, m1.as_str().parse().ok(), m2.as_str().parse().ok(), cont, true));
    }
    if let Some(c) = DATE.captures(s) {
        let (y, mo, d): (i32, u32, u32) = (c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
        if chrono::NaiveDate::from_ymd_opt(y, mo, d).is_some() {
            let mut h = hit(c.get(1).unwrap().start(), c.get(3).unwrap().end(), Some(y as u32), None, None, true);
            h.date = Some((y, mo, d));
            return Some(h);
        }
    }
    None
}

/// Weaker episode patterns ("Episode 4", "E04", "03 - Title"), only used
/// inside a Season folder.
fn find_episode_only(s: &str) -> Option<TvHit> {
    if let Some(c) = EP_ONLY.captures(s) {
        let (m0, m) = (c.get(0).unwrap(), c.get(1).unwrap());
        let start = m0.start() + usize::from(!m0.as_str().starts_with(|ch: char| ch.is_ascii_alphanumeric()));
        return Some(hit(start, m.end(), None, m.as_str().parse().ok(), None, false));
    }
    let junk = earliest_junk(s, 0, false).unwrap_or(s.len());
    let mut first = None;
    let mut pos = 0;
    while pos < s.len() {
        let Some(c) = BARE_NUM.captures_at(s, pos) else { break };
        let m = c.get(1).unwrap();
        if m.start() >= junk {
            break;
        }
        if first.is_none() {
            first = Some(hit(m.start(), m.end(), None, m.as_str().parse().ok(), None, false));
        }
        pos = m.end();
    }
    first
}

fn extra_analysis(
    kind: ExtraKind,
    name_part: &str,
    folder_title: &str,
    folder_year: Option<u16>,
    conf: Confidence,
) -> Analysis {
    let mut a = Analysis::new(Kind::Extra);
    a.extra = Some(kind);
    a.title = folder_title.to_string();
    a.year = folder_year;
    let (n, _) = title_and_year(name_part);
    let n = if n.is_empty() || norm(&n) == norm(folder_title) { kind.singular().to_string() } else { n };
    a.extra_name = Some(n);
    a.confidence = if folder_title.is_empty() { Confidence::Low } else { conf };
    if folder_title.is_empty() {
        a.notes.push("Extra: which movie or show? Enter the title".into());
    }
    a
}

/// Analyze one video file. `folders` are the names of the folders that
/// contain it, outermost first (normally starting with the scanned folder's
/// own name); `file_name` includes the extension.
pub fn analyze(folders: &[String], file_name: &str) -> Analysis {
    let (stem, _ext) = split_name(file_name);
    let parent = folders.last().map(String::as_str).unwrap_or("");

    if SAMPLE.is_match(stem) || parent.eq_ignore_ascii_case("sample") || parent.eq_ignore_ascii_case("samples") {
        let mut a = Analysis::new(Kind::Skip);
        a.title = tidy(stem);
        a.confidence = Confidence::High;
        a.notes.push("Sample clip: left where it is".into());
        return a;
    }

    // folder context: Season folder, extras folder, then the show / movie folder
    let mut season_from_dir = None;
    let mut extra_from_dir = None;
    let mut context: Vec<&str> = vec![];
    for f in folders.iter().rev() {
        if let Some(sn) = season_of_dir(f) {
            if season_from_dir.is_none() && context.is_empty() {
                season_from_dir = Some(sn);
            }
            continue;
        }
        if let Some(e) = extra_of_dir(f) {
            if extra_from_dir.is_none() && context.is_empty() && season_from_dir.is_none() {
                extra_from_dir = Some(e);
            }
            continue;
        }
        context.push(f.as_str());
    }
    let folder_name = context.first().copied().unwrap_or("");
    let (folder_title_raw, folder_year) = {
        // "Show.Name.S02.1080p.WEB-DL" -> cut at the season marker first
        let f = match SEASON_IN_NAME.find(folder_name) {
            Some(m) if m.start() > 0 => &folder_name[..m.start()],
            _ => folder_name,
        };
        title_and_year(f)
    };
    let folder_title = if looks_generic(&folder_title_raw) { String::new() } else { folder_title_raw };

    // inline Plex suffix: "Name-trailer"
    if let Some(c) = INLINE_EXTRA.captures(stem) {
        let kind = match &c[1].to_lowercase()[..] {
            "behindthescenes" => ExtraKind::BehindTheScenes,
            "deleted" => ExtraKind::DeletedScenes,
            "featurette" => ExtraKind::Featurettes,
            "interview" => ExtraKind::Interviews,
            "scene" => ExtraKind::Scenes,
            "short" => ExtraKind::Shorts,
            "trailer" => ExtraKind::Trailers,
            _ => ExtraKind::Other,
        };
        let name_part = &stem[..c.get(0).unwrap().start()];
        return extra_analysis(kind, name_part, &folder_title, folder_year, Confidence::High);
    }
    if let Some(kind) = extra_from_dir {
        return extra_analysis(kind, stem, &folder_title, folder_year, Confidence::High);
    }

    // TV episode?
    let tv = find_tv(stem).or_else(|| season_from_dir.and_then(|_| find_episode_only(stem)));
    if let Some(h) = tv {
        let mut a = Analysis::new(Kind::Episode);
        let (show, show_year) = title_and_year(&stem[..h.start]);
        let show = if looks_generic(&show) { String::new() } else { show };
        a.season = h.season.or(season_from_dir).or(if h.date.is_some() { None } else { Some(1) });
        a.episode = h.episode;
        a.episode_end = h.episode_end.filter(|e| Some(*e) > h.episode);
        a.air_date = h.date;
        let mut conf = if h.strong { Confidence::High } else { Confidence::Medium };
        if h.season.is_none() && season_from_dir.is_none() && h.date.is_none() {
            a.notes.push("No season found: assumed Season 1".into());
            conf = Confidence::Low;
        }
        if !show.is_empty() {
            a.year = show_year.or(if norm(&show) == norm(&folder_title) { folder_year } else { None });
            if !folder_title.is_empty() && norm(&show) == norm(&folder_title) {
                a.title = folder_title.clone(); // keep the folder's spelling / case
            } else {
                a.title = show;
            }
        } else if !folder_title.is_empty() {
            a.title = folder_title.clone();
            a.year = folder_year;
            if h.strong {
                a.notes.push("Show name taken from the folder".into());
            }
            conf = conf.min(if h.strong { Confidence::High } else { Confidence::Medium });
        } else {
            a.notes.push("Show name not found: please enter it".into());
            conf = Confidence::Low;
        }
        // episode title: text after the episode marker, up to the junk
        let rest = &stem[h.end..];
        let cut = earliest_junk(rest, 0, true).unwrap_or(rest.len());
        let et = tidy(&rest[..cut]);
        if !et.is_empty() && et.chars().any(char::is_alphabetic) && !looks_generic(&et) {
            a.episode_title = Some(smart_case(&et));
        }
        if a.season == Some(0) {
            a.notes.push("Special: Season 00".into());
        }
        a.confidence = conf;
        return a;
    }

    // keyword extras ("Heat Teaser.mkv"); a dated title such as
    // "The Interview (2014)" stays a movie (unless it sits next to its movie)
    let kw = extra_keyword(stem);
    let (title, year) = title_and_year(stem);
    if let Some(kind) = kw
        && year.is_none()
    {
        return extra_analysis(kind, stem, &folder_title, folder_year, Confidence::Medium);
    }

    let mut a = Analysis::new(Kind::Movie);
    a.edition = find_edition(stem).or_else(|| find_edition(folder_name));
    let generic = looks_generic(&title);
    match (generic, year, folder_year) {
        (false, Some(y), fy) => {
            a.title = title;
            a.year = Some(y);
            if fy.is_some() && fy != Some(y) && !folder_title.is_empty() {
                a.notes.push(format!("Folder year {} differs", fy.unwrap_or_default()));
                a.confidence = Confidence::Medium;
            } else {
                a.confidence = Confidence::High;
            }
        }
        (false, None, Some(fy)) if !folder_title.is_empty() && norm(&folder_title).contains(&norm(&title)) => {
            a.title = folder_title.clone();
            a.year = Some(fy);
            a.confidence = Confidence::High;
        }
        (_, _, Some(fy)) if !folder_title.is_empty() => {
            a.title = folder_title.clone();
            a.year = Some(fy);
            a.confidence = Confidence::Medium;
            a.notes.push("Title and year taken from the folder".into());
        }
        (false, None, None) => {
            a.title = title;
            a.confidence = Confidence::Low;
            a.notes.push("No year found: add it, or look it up on TMDB".into());
        }
        _ => {
            a.title = if folder_title.is_empty() { tidy(stem) } else { folder_title.clone() };
            a.confidence = Confidence::Low;
            a.notes.push("Could not identify this file".into());
        }
    }
    if kw.is_some() {
        a.notes.push("Name mentions an extra (trailer, interview...): check the type".into());
        a.confidence = a.confidence.min(Confidence::Medium);
    }
    a
}

/// Make one path component safe for Windows: `:` becomes ` -`, other
/// forbidden characters are removed, trailing dots/spaces trimmed.
pub fn safe_component(s: &str) -> String {
    let s = s.replace(": ", " - ").replace(':', "-");
    let s: String =
        s.chars().filter(|c| !matches!(c, '<' | '>' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect();
    let s = SPACES.replace_all(&s, " ");
    let s = s.trim().trim_end_matches(['.', ' ']).to_string();
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
        "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if RESERVED.contains(&s.to_lowercase().as_str()) { format!("{s}_") } else { s }
}

fn name_with_year(title: &str, year: Option<u16>) -> String {
    let t = title.trim();
    match year {
        Some(y) => format!("{t} ({y})"),
        None => t.to_string(),
    }
}

/// Why the file cannot be moved yet (None = complete).
pub fn missing(a: &Analysis) -> Option<&'static str> {
    match a.kind {
        Kind::Skip => Some("Skipped"),
        _ if safe_component(&a.title).is_empty() => Some("Title missing"),
        Kind::Episode if a.air_date.is_none() && (a.season.is_none() || a.episode.is_none()) => {
            Some("Season / episode missing")
        }
        Kind::Extra if a.extra.is_none() => Some("Extra type missing"),
        _ => None,
    }
}

/// Which library root a file goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    Movies,
    Tv,
}

pub fn library_of(a: &Analysis) -> Library {
    match a.kind {
        Kind::Episode => Library::Tv,
        Kind::Extra if a.extra_of_show => Library::Tv,
        _ => Library::Movies,
    }
}

/// Plex path relative to the library root, e.g.
/// `The Matrix (1999)/The Matrix (1999).mkv`. `ext` includes the dot.
pub fn relative_path(a: &Analysis, ext: &str) -> Option<PathBuf> {
    if missing(a).is_some() {
        return None;
    }
    let ext = ext.to_lowercase();
    let top = safe_component(&name_with_year(&a.title, a.year));
    let mut p = PathBuf::from(&top);
    match a.kind {
        Kind::Movie => {
            let mut name = top.clone();
            if let Some(e) =
                a.edition.as_deref().map(|e| safe_component(e).replace(['{', '}'], "")).filter(|e| !e.is_empty())
            {
                name.push_str(&format!(" {{edition-{e}}}"));
            }
            p.push(format!("{name}{ext}"));
        }
        Kind::Episode => {
            let mut name;
            if let Some((y, m, d)) = a.air_date {
                p.push(format!("Season {y}"));
                name = format!("{top} - {y:04}-{m:02}-{d:02}");
            } else {
                let s = a.season.unwrap_or(1);
                let e = a.episode.unwrap_or(1);
                p.push(format!("Season {s:02}"));
                name = format!("{top} - s{s:02}e{e:02}");
                if let Some(end) = a.episode_end.filter(|x| *x > e) {
                    name.push_str(&format!("-e{end:02}"));
                }
            }
            if let Some(t) = a.episode_title.as_deref().map(safe_component).filter(|t| !t.is_empty()) {
                name.push_str(&format!(" - {t}"));
            }
            p.push(format!("{name}{ext}"));
        }
        Kind::Extra => {
            let k = a.extra.unwrap_or(ExtraKind::Other);
            p.push(k.folder());
            let n = a.extra_name.as_deref().map(safe_component).filter(|n| !n.is_empty());
            p.push(format!("{}{ext}", n.unwrap_or_else(|| k.singular().to_string())));
        }
        Kind::Skip => return None,
    }
    Some(p)
}

// ---------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------

const LANGS: &[(&str, &str)] = &[
    ("english", "en"),
    ("eng", "en"),
    ("en", "en"),
    ("spanish", "es"),
    ("spa", "es"),
    ("es", "es"),
    ("espanol", "es"),
    ("french", "fr"),
    ("fre", "fr"),
    ("fra", "fr"),
    ("fr", "fr"),
    ("german", "de"),
    ("ger", "de"),
    ("deu", "de"),
    ("de", "de"),
    ("italian", "it"),
    ("ita", "it"),
    ("it", "it"),
    ("portuguese", "pt"),
    ("por", "pt"),
    ("pt", "pt"),
    ("brazilian", "pt-BR"),
    ("pt-br", "pt-BR"),
    ("dutch", "nl"),
    ("dut", "nl"),
    ("nld", "nl"),
    ("nl", "nl"),
    ("swedish", "sv"),
    ("swe", "sv"),
    ("sv", "sv"),
    ("norwegian", "no"),
    ("nor", "no"),
    ("no", "no"),
    ("danish", "da"),
    ("dan", "da"),
    ("da", "da"),
    ("finnish", "fi"),
    ("fin", "fi"),
    ("fi", "fi"),
    ("polish", "pl"),
    ("pol", "pl"),
    ("pl", "pl"),
    ("russian", "ru"),
    ("rus", "ru"),
    ("ru", "ru"),
    ("japanese", "ja"),
    ("jpn", "ja"),
    ("ja", "ja"),
    ("chinese", "zh"),
    ("chi", "zh"),
    ("zho", "zh"),
    ("zh", "zh"),
    ("korean", "ko"),
    ("kor", "ko"),
    ("ko", "ko"),
    ("arabic", "ar"),
    ("ara", "ar"),
    ("ar", "ar"),
    ("hebrew", "he"),
    ("heb", "he"),
    ("he", "he"),
    ("turkish", "tr"),
    ("tur", "tr"),
    ("tr", "tr"),
    ("greek", "el"),
    ("gre", "el"),
    ("ell", "el"),
    ("el", "el"),
    ("czech", "cs"),
    ("cze", "cs"),
    ("ces", "cs"),
    ("cs", "cs"),
    ("hungarian", "hu"),
    ("hun", "hu"),
    ("hu", "hu"),
    ("romanian", "ro"),
    ("rum", "ro"),
    ("ron", "ro"),
    ("ro", "ro"),
    ("hindi", "hi"),
    ("hin", "hi"),
    ("hi", "hi"),
    ("thai", "th"),
    ("tha", "th"),
    ("th", "th"),
    ("vietnamese", "vi"),
    ("vie", "vi"),
    ("vi", "vi"),
    ("indonesian", "id"),
    ("ind", "id"),
    ("ukrainian", "uk"),
    ("ukr", "uk"),
    ("uk", "uk"),
];

fn lang_code(w: &str) -> Option<String> {
    let base = w.trim_start_matches(|c: char| c.is_ascii_digit()).trim_matches(['-', '_']);
    if let Some((_, code)) = LANGS.iter().find(|(k, _)| *k == base) {
        return Some(code.to_string());
    }
    let (a, _) = base.split_once('-')?;
    LANGS.iter().find(|(k, _)| *k == a).map(|(_, c)| c.to_string())
}

/// Language / flag tags in a subtitle name suffix such as `.en.forced`,
/// `2_English` or `.eng.sdh`, in Plex form (`["en", "forced"]`).
pub fn subtitle_tags(suffix: &str) -> Vec<String> {
    let mut lang: Option<String> = None;
    let mut flags: Vec<&str> = vec![];
    let lower = suffix.to_lowercase();
    for w in lower.split(['.', ' ', '(', ')', '[', ']']).filter(|w| !w.is_empty()) {
        let w = w.trim_matches('-');
        let flag = match w {
            "forced" | "foreign" => Some("forced"),
            "sdh" | "cc" => Some("sdh"),
            "hi" if lang.is_some() => Some("sdh"),
            "default" => Some("default"),
            _ => None,
        };
        match flag {
            Some(f) if !flags.contains(&f) => flags.push(f),
            Some(_) => {}
            None => {
                if lang.is_none() {
                    // "2_english" -> english
                    let part = w.rsplit('_').next().unwrap_or(w);
                    lang = lang_code(part);
                }
            }
        }
    }
    let mut v: Vec<String> = lang.into_iter().collect();
    v.extend(flags.into_iter().map(String::from));
    v
}

// ---------------------------------------------------------------------------
// Scan results -> preview rows -> plan
// ---------------------------------------------------------------------------

/// One row of the Plex preview: a video, or a subtitle that follows one.
#[derive(Debug, Clone)]
pub struct Row {
    pub source: PathBuf,
    /// Detected (and possibly user-edited) identity. Subtitles carry their
    /// video's analysis.
    pub analysis: Analysis,
    /// The analysis as detected (for "Reset" in the editor).
    pub detected: Analysis,
    /// Subtitles: index of the video row.
    pub subtitle_of: Option<usize>,
    /// Subtitles: Plex tags (`en`, `forced`, `sdh`).
    pub sub_tags: Vec<String>,
    /// Included in Apply.
    pub include: bool,
}

/// Folder names from `root` (its own name first) down to the file's parent.
pub fn folder_chain(root: &Path, file: &Path) -> Vec<String> {
    let mut v = vec![];
    if let Some(n) = root.file_name() {
        v.push(n.to_string_lossy().into_owned());
    }
    if let Some(parent) = file.parent()
        && let Ok(rel) = parent.strip_prefix(root)
    {
        v.extend(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    }
    v
}

fn file_stem_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn dir_of(p: &Path) -> PathBuf {
    p.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Build the preview rows for scanned files (videos and subtitles; other
/// files are ignored).
pub fn analyze_files(root: &Path, files: &[PathBuf]) -> Vec<Row> {
    let mut rows: Vec<Row> = vec![];
    let mut subs: Vec<&PathBuf> = vec![];
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (_, ext) = split_name(&name);
        let ext = ext.trim_start_matches('.');
        if is_video(ext) {
            let a = analyze(&folder_chain(root, f), &name);
            rows.push(Row {
                source: f.clone(),
                include: a.kind != Kind::Skip,
                detected: a.clone(),
                analysis: a,
                subtitle_of: None,
                sub_tags: vec![],
            });
        } else if is_subtitle(ext) {
            subs.push(f);
        }
    }
    link_extras(&mut rows);
    let videos = rows.len();
    for s in subs {
        let (video, tags) = match_subtitle(s, &rows[..videos]);
        let a = match video {
            Some(i) => rows[i].analysis.clone(),
            None => {
                let mut a = Analysis::new(Kind::Skip);
                a.title = file_stem_of(s);
                a.notes = vec!["Subtitle without a matching video: left where it is".into()];
                a
            }
        };
        rows.push(Row {
            source: s.clone(),
            include: video.is_some_and(|i| rows[i].include),
            detected: a.clone(),
            analysis: a,
            subtitle_of: video,
            sub_tags: tags,
        });
    }
    rows
}

/// Give extras the title / year of the movie or show next to them (same
/// folder, or the parent folder when the extra sits in an extras folder).
/// "Heat (1995) Trailer.mkv" next to "Heat (1995).mkv" becomes an extra.
fn link_extras(rows: &mut [Row]) {
    let mains: Vec<(PathBuf, PathBuf, Analysis)> = rows
        .iter()
        .filter(|r| {
            matches!(r.analysis.kind, Kind::Movie | Kind::Episode) && extra_keyword(&file_stem_of(&r.source)).is_none()
        })
        .map(|r| (dir_of(&r.source), r.source.clone(), r.analysis.clone()))
        .collect();
    for r in rows.iter_mut() {
        let dir = dir_of(&r.source);
        let stem = file_stem_of(&r.source);
        let kw = extra_keyword(&stem);
        let keyword_movie = r.analysis.kind == Kind::Movie && kw.is_some();
        if r.analysis.kind != Kind::Extra && !keyword_movie {
            continue;
        }
        let dir_name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parent_dir = if extra_of_dir(&dir_name).is_some() { dir.parent().map(Path::to_path_buf) } else { None };
        // in the same folder: the main video with the same title, or the
        // only title there (a mixed downloads folder does not guess)
        let same_dir: Vec<&(PathBuf, PathBuf, Analysis)> =
            mains.iter().filter(|(d, src, _)| *d == dir && *src != r.source).collect();
        let titled = same_dir
            .iter()
            .copied()
            .find(|(_, _, a)| !r.analysis.title.is_empty() && norm(&a.title) == norm(&r.analysis.title));
        let only = {
            let mut titles: Vec<String> = same_dir.iter().map(|(_, _, a)| norm(&a.title)).collect();
            titles.sort();
            titles.dedup();
            if titles.len() == 1 { same_dir.first().copied() } else { None }
        };
        let main = titled
            .or(if keyword_movie { None } else { only })
            .or_else(|| parent_dir.as_ref().and_then(|pd| mains.iter().find(|(d, _, _)| d == pd)))
            .or_else(|| {
                // a show: an episode in a Season folder below the show folder
                parent_dir.as_ref().and_then(|pd| {
                    mains.iter().find(|(d, _, a)| a.kind == Kind::Episode && d.parent() == Some(pd.as_path()))
                })
            });
        let Some((_, _, m)) = main else { continue };
        if keyword_movie {
            if norm(&r.analysis.title) != norm(&m.title) || (r.analysis.year.is_some() && r.analysis.year != m.year) {
                continue;
            }
            let Some(kind) = kw else { continue };
            let mut a = extra_analysis(kind, &stem, &m.title, m.year, Confidence::Medium);
            let rest = norm(&tidy(&stem)).replacen(&norm(&m.title), "", 1);
            let rest = rest.replacen(&m.year.map(|y| y.to_string()).unwrap_or_default(), "", 1);
            if rest == norm(kind.singular()) || rest.is_empty() {
                a.extra_name = Some(kind.singular().to_string());
            }
            a.extra_of_show = m.kind == Kind::Episode;
            r.analysis = a;
            r.detected = r.analysis.clone();
            continue;
        }
        let a = &mut r.analysis;
        let before = a.title.clone();
        a.title = m.title.clone();
        a.year = m.year;
        a.extra_of_show = m.kind == Kind::Episode;
        if a.extra_name.as_deref().map(norm) == Some(norm(&m.title)) {
            a.extra_name = a.extra.map(|k| k.singular().to_string());
        }
        if before.is_empty() || norm(&before) == norm(&m.title) {
            a.confidence = a.confidence.max(Confidence::Medium);
            if !before.is_empty() {
                a.confidence = Confidence::High;
            }
        }
        a.notes.retain(|n| !n.starts_with("Extra: which"));
        r.detected = r.analysis.clone();
    }
}

/// The video a subtitle belongs to: same folder with a name that starts
/// with the video's name, or a `Subs` folder next to the video (also
/// `Subs/<video name>/2_English.srt`), or the only movie / episode in the
/// same folder.
fn match_subtitle(sub: &Path, videos: &[Row]) -> (Option<usize>, Vec<String>) {
    let stem = file_stem_of(sub);
    let dir = dir_of(sub);
    let lower = stem.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for (i, r) in videos.iter().enumerate() {
        let vs = file_stem_of(&r.source);
        if dir_of(&r.source) == dir && lower.starts_with(&vs.to_lowercase()) && best.is_none_or(|(_, l)| vs.len() > l) {
            best = Some((i, vs.len()));
        }
    }
    if let Some((i, l)) = best {
        return (Some(i), subtitle_tags(&stem[l..]));
    }
    let is_subs = |p: &Path| p.file_name().is_some_and(|n| is_subs_dir(&n.to_string_lossy()));
    let (subs_dir, named) = if is_subs(&dir) {
        (Some(dir.clone()), None)
    } else if let Some(p) = dir.parent().filter(|p| is_subs(p)) {
        (Some(p.to_path_buf()), dir.file_name().map(|n| n.to_string_lossy().into_owned()))
    } else {
        (None, None)
    };
    if let Some(video_dir) = subs_dir.as_deref().and_then(Path::parent) {
        let cands: Vec<usize> = (0..videos.len()).filter(|&i| dir_of(&videos[i].source) == video_dir).collect();
        let pick = named
            .as_ref()
            .and_then(|n| cands.iter().copied().find(|&i| file_stem_of(&videos[i].source).eq_ignore_ascii_case(n)))
            .or_else(|| {
                let mains: Vec<usize> = cands
                    .iter()
                    .copied()
                    .filter(|&i| matches!(videos[i].analysis.kind, Kind::Movie | Kind::Episode))
                    .collect();
                (mains.len() == 1).then(|| mains[0])
            });
        if let Some(i) = pick {
            return (Some(i), subtitle_tags(&stem));
        }
    }
    // a name that is only tags ("English.srt", "en.forced.srt") next to a
    // single movie / episode
    let only_tags = !subtitle_tags(&stem).is_empty()
        && stem.split(['.', '_', ' ', '-']).all(|w| {
            w.is_empty() || w.chars().all(|c| c.is_ascii_digit()) || !subtitle_tags(&format!(".{w}")).is_empty()
        });
    let same: Vec<usize> = (0..videos.len())
        .filter(|&i| dir_of(&videos[i].source) == dir && matches!(videos[i].analysis.kind, Kind::Movie | Kind::Episode))
        .collect();
    if only_tags && same.len() == 1 {
        return (Some(same[0]), subtitle_tags(&stem));
    }
    (None, vec![])
}

/// Where the files go. `None` = in place (inside the scanned folder).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Destination {
    pub movies_root: Option<PathBuf>,
    pub tv_root: Option<PathBuf>,
}

impl Destination {
    pub fn root_for(&self, lib: Library, scanned: &Path) -> PathBuf {
        match lib {
            Library::Movies => self.movies_root.clone(),
            Library::Tv => self.tv_root.clone(),
        }
        .unwrap_or_else(|| scanned.to_path_buf())
    }
}

/// Turn the rows into a plan (the same item type as the main tab, so Apply,
/// Undo and the never-overwrite checks are shared). Subtitles take the
/// final name of their video, including a " (1)" collision suffix. Items
/// are returned in row order.
pub fn build_plan(root: &Path, rows: &[Row], dest: &Destination) -> Plan {
    let mut reserved: HashSet<String> = HashSet::new();
    let key = |p: &Path| p.to_string_lossy().to_lowercase();
    let mut video_final: HashMap<usize, (PathBuf, PathBuf)> = HashMap::new();
    let order = (0..rows.len())
        .filter(|&i| rows[i].subtitle_of.is_none())
        .chain((0..rows.len()).filter(|&i| rows[i].subtitle_of.is_some()));
    let mut by_index: Vec<Option<PlanItem>> = vec![None; rows.len()];
    for i in order {
        let r = &rows[i];
        let name = r.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (_, ext) = split_name(&name);
        let current_dir = dir_of(&r.source);
        let mut item = PlanItem {
            source: r.source.clone(),
            current_dir: current_dir.clone(),
            base_dir: current_dir.clone(),
            date: None,
            new_name: name.clone(),
            dest_folder_name: String::new(),
            dest_dir: current_dir.clone(),
            dest_path: PathBuf::new(),
            renamed_for_collision: false,
            status: Status::Unchanged,
            warnings: vec![],
        };
        let (lib_root, rel) = match r.subtitle_of {
            None => (dest.root_for(library_of(&r.analysis), root), relative_path(&r.analysis, ext)),
            Some(v) => match video_final.get(&v) {
                Some((vroot, vp)) => {
                    let mut n = file_stem_of(vp);
                    for t in &r.sub_tags {
                        n.push('.');
                        n.push_str(t);
                    }
                    n.push_str(&ext.to_lowercase());
                    let rel_dir = vp.parent().and_then(|p| p.strip_prefix(vroot).ok()).map(Path::to_path_buf);
                    (vroot.clone(), rel_dir.map(|d| d.join(n)))
                }
                None => (root.to_path_buf(), None),
            },
        };
        match (r.include, rel) {
            (false, _) => item.warnings.push("Not included".into()),
            (true, None) => {
                let why = match r.subtitle_of {
                    Some(_) => "Its video is not moved",
                    None => missing(&r.analysis).unwrap_or("Not identified"),
                };
                item.warnings.push(why.into());
            }
            (true, Some(rel)) => {
                let full = lib_root.join(&rel);
                let dest_dir = full.parent().map(Path::to_path_buf).unwrap_or_else(|| lib_root.clone());
                let file_name = full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                item.dest_dir = dest_dir.clone();
                item.base_dir = lib_root.clone();
                item.new_name = file_name.clone();
                item.dest_folder_name =
                    dest_dir.strip_prefix(&lib_root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                if r.source == full {
                    item.status = Status::AlreadySorted;
                    item.dest_path = r.source.clone();
                } else {
                    let (p, renamed) = unique_dest_for(&dest_dir, &file_name, &reserved, Some(&r.source));
                    reserved.insert(key(&p));
                    if r.source == p {
                        item.status = Status::AlreadySorted;
                    } else {
                        item.status = Status::Move;
                    }
                    item.dest_path = p;
                    item.renamed_for_collision = renamed;
                }
                if r.subtitle_of.is_none() {
                    video_final.insert(i, (lib_root.clone(), item.dest_path.clone()));
                }
            }
        }
        by_index[i] = Some(item);
    }
    Plan { root: root.to_path_buf(), items: by_index.into_iter().flatten().collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(path: &str) -> Analysis {
        let parts: Vec<String> = path.split('/').map(String::from).collect();
        let (file, folders) = parts.split_last().unwrap();
        analyze(folders, file)
    }

    fn rel(path: &str) -> String {
        let an = a(path);
        let ext = format!(".{}", path.rsplit('.').next().unwrap());
        relative_path(&an, &ext)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| format!("<none: {:?}>", missing(&an)))
    }

    #[test]
    fn movies() {
        let cases = [
            ("Downloads/The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv", "The Matrix (1999)/The Matrix (1999).mkv"),
            ("Downloads/Inception (2010) [1080p] [YTS.MX].mp4", "Inception (2010)/Inception (2010).mp4"),
            (
                "Downloads/blade_runner_2049_2017_2160p_uhd_bluray_hevc.mkv",
                "Blade Runner 2049 (2017)/Blade Runner 2049 (2017).mkv",
            ),
            ("Downloads/2012.2009.720p.BRRip.mkv", "2012 (2009)/2012 (2009).mkv"),
            ("Downloads/1917.2019.HDR.2160p.WEB.H265-DEFLATE.mkv", "1917 (2019)/1917 (2019).mkv"),
            (
                "Downloads/Blade.Runner.1982.Final.Cut.1080p.BluRay.mkv",
                "Blade Runner (1982)/Blade Runner (1982) {edition-Final Cut}.mkv",
            ),
            (
                "Downloads/Aliens.1986.Directors.Cut.REMASTERED.1080p.mkv",
                "Aliens (1986)/Aliens (1986) {edition-Director's Cut}.mkv",
            ),
            (
                "Downloads/The Lord of the Rings The Fellowship of the Ring (2001) Extended.mkv",
                "The Lord of the Rings The Fellowship of the Ring (2001)/The Lord of the Rings The Fellowship of the Ring (2001) {edition-Extended Edition}.mkv",
            ),
            ("Movies/Heat (1995)/Heat (1995).mkv", "Heat (1995)/Heat (1995).mkv"),
            (
                "Movies/Heat (1995) {edition-Director's Cut}/Heat (1995) {edition-Director's Cut}.mkv",
                "Heat (1995)/Heat (1995) {edition-Director's Cut}.mkv",
            ),
            ("Movies/Arrival (2016)/movie.mkv", "Arrival (2016)/Arrival (2016).mkv"),
            (
                "Movies/Dune.Part.Two.2024.1080p.WEB-DL.DDP5.1.Atmos.H.264-FLUX/dune.part.two.2024.1080p.web-dl.mkv",
                "Dune Part Two (2024)/Dune Part Two (2024).mkv",
            ),
            ("Movies/Gladiator 2000/title_t00.mkv", "Gladiator (2000)/Gladiator (2000).mkv"),
            (
                "Movies/Mission Impossible (1996)/Mission Impossible 1996.mkv",
                "Mission Impossible (1996)/Mission Impossible (1996).mkv",
            ),
            ("Movies/[ABC] Spirited Away (2001) [BD 1080p].mkv", "Spirited Away (2001)/Spirited Away (2001).mkv"),
            ("Movies/the.dark.knight.2008.mkv", "The Dark Knight (2008)/The Dark Knight (2008).mkv"),
            ("Movies/The Interview (2014).mkv", "The Interview (2014)/The Interview (2014).mkv"),
            ("Movies/Amelie.2001.FRENCH.1080p.mkv", "Amelie (2001)/Amelie (2001).mkv"),
        ];
        for (input, want) in cases {
            assert_eq!(rel(input), want, "{input}");
            assert_eq!(a(input).kind, Kind::Movie, "{input}");
        }
        assert_eq!(a(cases[0].0).confidence, Confidence::High);
        assert_eq!(a("Movies/Arrival (2016)/movie.mkv").confidence, Confidence::Medium);
        let no_year = a("Downloads/Some.Random.Film.1080p.WEB.mkv");
        assert_eq!(
            (no_year.title.as_str(), no_year.year, no_year.confidence),
            ("Some Random Film", None, Confidence::Low)
        );
        assert_eq!(rel("Downloads/Some.Random.Film.1080p.WEB.mkv"), "Some Random Film/Some Random Film.mkv");
        // a year after next year is part of the title
        let br = a("Downloads/Blade.Runner.2049.mkv");
        assert_eq!((br.title.as_str(), br.year), ("Blade Runner 2049", None));
    }

    #[test]
    fn tv_episodes() {
        let cases = [
            ("TV/Breaking.Bad.S01E02.720p.HDTV.x264-CTU.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e02.mkv"),
            (
                "TV/The.Office.US.S03E10.A.Benihana.Christmas.1080p.WEB-DL.mkv",
                "The Office US/Season 03/The Office US - s03e10 - A Benihana Christmas.mkv",
            ),
            (
                "TV/Friends - 1x05 - The One with the East German Laundry Detergent.avi",
                "Friends/Season 01/Friends - s01e05 - The One with the East German Laundry Detergent.avi",
            ),
            ("TV/Lost.S01E01E02.Pilot.720p.mkv", "Lost/Season 01/Lost - s01e01-e02 - Pilot.mkv"),
            ("TV/Lost.S01E01-E02.mkv", "Lost/Season 01/Lost - s01e01-e02.mkv"),
            ("TV/Lost S01E01-02 Pilot.mkv", "Lost/Season 01/Lost - s01e01-e02 - Pilot.mkv"),
            (
                "TV/Doctor.Who.2005.S00E01.The.Christmas.Invasion.mkv",
                "Doctor Who (2005)/Season 00/Doctor Who (2005) - s00e01 - The Christmas Invasion.mkv",
            ),
            (
                "TV/The Daily Show 2024.03.15 Guest Name 720p.mp4",
                "The Daily Show/Season 2024/The Daily Show - 2024-03-15 - Guest Name.mp4",
            ),
            (
                "TV/Severance (2022)/Season 1/Severance (2022) - s01e03 - In Perpetuity.mkv",
                "Severance (2022)/Season 01/Severance (2022) - s01e03 - In Perpetuity.mkv",
            ),
            ("TV/Severance (2022)/Season 2/Episode 4.mkv", "Severance (2022)/Season 02/Severance (2022) - s02e04.mkv"),
            ("TV/Severance (2022)/Season 2/S02E05.mkv", "Severance (2022)/Season 02/Severance (2022) - s02e05.mkv"),
            ("TV/Fargo/Specials/Fargo - Episode 3.mkv", "Fargo/Season 00/Fargo - s00e03.mkv"),
            (
                "TV/Show.Name.S02.1080p.WEB-DL/Show.Name.S02E07.1080p.WEB-DL.mkv",
                "Show Name/Season 02/Show Name - s02e07.mkv",
            ),
            (
                "TV/Shogun (2024)/Season 01/03 - Tomorrow is Tomorrow.mkv",
                "Shogun (2024)/Season 01/Shogun (2024) - s01e03 - Tomorrow is Tomorrow.mkv",
            ),
            (
                "TV/Planet Earth Season 1 Episode 4 Caves.mkv",
                "Planet Earth/Season 01/Planet Earth - s01e04 - Caves.mkv",
            ),
            ("TV/S.H.I.E.L.D.S01E01.mkv", "S.H.I.E.L.D/Season 01/S.H.I.E.L.D - s01e01.mkv"),
            ("TV/Trailer Park Boys S01E01.mkv", "Trailer Park Boys/Season 01/Trailer Park Boys - s01e01.mkv"),
            (
                "TV/Some Show/Season 1/Some.Show.S01E06.The.Interview.mkv",
                "Some Show/Season 01/Some Show - s01e06 - The Interview.mkv",
            ),
            (
                "TV/the.expanse.s02e13.caliban's.war.mkv",
                "The Expanse/Season 02/The Expanse - s02e13 - Caliban's War.mkv",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(rel(input), want, "{input}");
            assert_eq!(a(input).kind, Kind::Episode, "{input}");
        }
        assert_eq!(a(cases[0].0).confidence, Confidence::High);
        assert_eq!(a("TV/Severance (2022)/Season 2/Episode 4.mkv").confidence, Confidence::Medium);
        let r = a("TV/Show.S01E01-720p.mkv");
        assert_eq!((r.episode, r.episode_end), (Some(1), None), "resolution is not an episode range");
        assert_ne!(a("TV/1920x1080 test clip.mkv").kind, Kind::Episode);
        let unknown = a("Downloads/S03E04.mkv");
        assert_eq!((unknown.kind, unknown.confidence), (Kind::Episode, Confidence::Low));
        assert_eq!(missing(&unknown), Some("Title missing"));
    }

    #[test]
    fn extras_and_samples() {
        let cases = [
            ("Movies/Heat (1995)/Featurettes/Making the Movie.mkv", "Heat (1995)/Featurettes/Making the Movie.mkv"),
            ("Movies/Heat (1995)/Behind The Scenes/On Set.mkv", "Heat (1995)/Behind The Scenes/On Set.mkv"),
            (
                "Movies/Heat (1995)/Deleted Scenes/Alternate Ending.mkv",
                "Heat (1995)/Deleted Scenes/Alternate Ending.mkv",
            ),
            ("Movies/Heat (1995)/Trailers/Heat (1995).mp4", "Heat (1995)/Trailers/Trailer.mp4"),
            ("Movies/Heat (1995)/Interviews/Director.mkv", "Heat (1995)/Interviews/Director.mkv"),
            ("Movies/Heat (1995)/Scenes/Bank Shootout.mkv", "Heat (1995)/Scenes/Bank Shootout.mkv"),
            ("Movies/Heat (1995)/Shorts/Short One.mkv", "Heat (1995)/Shorts/Short One.mkv"),
            ("Movies/Heat (1995)/Extras/Gag Reel.mkv", "Heat (1995)/Other/Gag Reel.mkv"),
            ("Movies/Heat (1995)/Heat-trailer.mp4", "Heat (1995)/Trailers/Trailer.mp4"),
            ("Movies/Heat (1995)/Final Showdown-scene.mp4", "Heat (1995)/Scenes/Final Showdown.mp4"),
            ("Movies/Heat (1995)/Heat Teaser.mp4", "Heat (1995)/Trailers/Heat Teaser.mp4"),
            ("Movies/Heat (1995)/heat deleted scene 2.mkv", "Heat (1995)/Deleted Scenes/Heat Deleted Scene 2.mkv"),
        ];
        for (input, want) in cases {
            assert_eq!(rel(input), want, "{input}");
            assert_eq!(a(input).kind, Kind::Extra, "{input}");
        }
        assert_eq!(a("Movies/Heat (1995)/Featurettes/x.mkv").extra, Some(ExtraKind::Featurettes));
        for s in
            ["Movies/Heat (1995)/heat-sample.mkv", "Movies/Heat (1995)/Sample/heat.mkv", "Movies/Heat.1995.SAMPLE.mkv"]
        {
            assert_eq!(a(s).kind, Kind::Skip, "{s}");
            assert_eq!(rel(s), "<none: Some(\"Skipped\")>");
        }
        let orphan = a("Downloads/Some Trailer.mp4");
        assert_eq!((orphan.kind, orphan.confidence), (Kind::Extra, Confidence::Low));
    }

    #[test]
    fn junk_and_tidy() {
        assert_eq!(tidy("The.Matrix_Reloaded..[x]"), "The Matrix Reloaded");
        assert_eq!(smart_case("the lord of the rings"), "The Lord of the Rings");
        assert_eq!(smart_case("THE OFFICE"), "The Office");
        assert_eq!(smart_case("iCarly"), "iCarly");
        assert_eq!(safe_component("Mission: Impossible? <x>"), "Mission - Impossible x");
        assert_eq!(safe_component("CON"), "CON_");
        assert_eq!(safe_component("Trailing..."), "Trailing");
        for junk in ["x265", "HEVC", "WEB-DL", "BluRay", "1080p", "DDP5.1", "Atmos", "10bit", "REMUX", "YTS.MX", "AMZN"]
        {
            let (t, y) = title_and_year(&format!("Some Film 2001 {junk} extra words"));
            assert_eq!((t.as_str(), y), ("Some Film", Some(2001)), "{junk}");
            let (t2, _) = title_and_year(&format!("Some.Film.{junk}.GRP"));
            assert_eq!(t2, "Some Film", "{junk}");
        }
    }

    #[test]
    fn subtitle_tags_parse() {
        assert_eq!(subtitle_tags(".en"), vec!["en"]);
        assert_eq!(subtitle_tags(".eng.forced"), vec!["en", "forced"]);
        assert_eq!(subtitle_tags(".English.SDH"), vec!["en", "sdh"]);
        assert_eq!(subtitle_tags("2_English"), vec!["en"]);
        assert_eq!(subtitle_tags(".pt-BR"), vec!["pt-BR"]);
        assert_eq!(subtitle_tags(".fr.forced.default"), vec!["fr", "forced", "default"]);
        assert_eq!(subtitle_tags(".en.cc"), vec!["en", "sdh"]);
        assert_eq!(subtitle_tags(".hi"), vec!["hi"], "alone, hi is Hindi");
        assert!(subtitle_tags("").is_empty() && subtitle_tags(".xyz").is_empty());
    }

    #[test]
    fn extras_in_a_mixed_folder_follow_their_own_movie() {
        let root = PathBuf::from("/lib/Downloads");
        let files: Vec<PathBuf> = [
            "Blade.Runner.1982.Final.Cut.1080p.BluRay.mkv",
            "Inception (2010) [1080p] [YTS.MX].mp4",
            "Inception.2010.trailer.mp4",
            "Behind the scenes.mkv",
        ]
        .iter()
        .map(|f| root.join(f))
        .collect();
        let rows = analyze_files(&root, &files);
        let trailer = rows.iter().find(|r| r.source.ends_with("Inception.2010.trailer.mp4")).unwrap();
        assert_eq!(trailer.analysis.kind, Kind::Extra);
        assert_eq!(trailer.analysis.extra, Some(ExtraKind::Trailers));
        assert_eq!((trailer.analysis.title.as_str(), trailer.analysis.year), ("Inception", Some(2010)));
        // two different movies in the folder: an untitled extra is not guessed
        let bts = rows.iter().find(|r| r.source.ends_with("Behind the scenes.mkv")).unwrap();
        assert_ne!(bts.analysis.title, "Blade Runner");
        assert_ne!(bts.analysis.confidence, Confidence::High);
    }

    #[test]
    fn rows_subtitles_and_library_roots() {
        let root = PathBuf::from("/lib/Downloads");
        let files: Vec<PathBuf> = [
            "The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv",
            "The.Matrix.1999.1080p.BluRay.x264-GROUP.en.srt",
            "The.Matrix.1999.1080p.BluRay.x264-GROUP.en.forced.srt",
            "Heat (1995)/Heat (1995).mkv",
            "Heat (1995)/Heat (1995) Trailer.mkv",
            "Heat (1995)/Subs/2_English.srt",
            "Heat (1995)/Subs/3_Spanish.srt",
            "Heat (1995)/Featurettes/Making Of.mkv",
            "Show (2020)/Season 1/Show.S01E01.mkv",
            "Show (2020)/Season 1/Show.S01E01.srt",
            "Show (2020)/Featurettes/Cast Chat.mkv",
            "orphan.srt",
            "notes.txt",
        ]
        .iter()
        .map(|f| root.join(f))
        .collect();
        let rows = analyze_files(&root, &files);
        assert_eq!(rows.len(), 12, "notes.txt is ignored");
        let dest = Destination { movies_root: Some("/lib/Movies".into()), tv_root: Some("/lib/TV".into()) };
        let plan = build_plan(&root, &rows, &dest);
        let mut got: Vec<(String, String)> = plan
            .items
            .iter()
            .map(|i| {
                let to = if i.status == Status::Move {
                    i.dest_path.to_string_lossy().into_owned()
                } else {
                    format!("{:?}", i.status)
                };
                (i.source.strip_prefix(&root).unwrap().to_string_lossy().into_owned(), to)
            })
            .collect();
        got.sort();
        let mut want: Vec<(String, String)> = [
            ("The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv", "/lib/Movies/The Matrix (1999)/The Matrix (1999).mkv"),
            (
                "The.Matrix.1999.1080p.BluRay.x264-GROUP.en.srt",
                "/lib/Movies/The Matrix (1999)/The Matrix (1999).en.srt",
            ),
            (
                "The.Matrix.1999.1080p.BluRay.x264-GROUP.en.forced.srt",
                "/lib/Movies/The Matrix (1999)/The Matrix (1999).en.forced.srt",
            ),
            ("Heat (1995)/Heat (1995).mkv", "/lib/Movies/Heat (1995)/Heat (1995).mkv"),
            ("Heat (1995)/Heat (1995) Trailer.mkv", "/lib/Movies/Heat (1995)/Trailers/Trailer.mkv"),
            ("Heat (1995)/Subs/2_English.srt", "/lib/Movies/Heat (1995)/Heat (1995).en.srt"),
            ("Heat (1995)/Subs/3_Spanish.srt", "/lib/Movies/Heat (1995)/Heat (1995).es.srt"),
            ("Heat (1995)/Featurettes/Making Of.mkv", "/lib/Movies/Heat (1995)/Featurettes/Making Of.mkv"),
            ("Show (2020)/Season 1/Show.S01E01.mkv", "/lib/TV/Show (2020)/Season 01/Show (2020) - s01e01.mkv"),
            ("Show (2020)/Season 1/Show.S01E01.srt", "/lib/TV/Show (2020)/Season 01/Show (2020) - s01e01.srt"),
            ("Show (2020)/Featurettes/Cast Chat.mkv", "/lib/TV/Show (2020)/Featurettes/Cast Chat.mkv"),
            ("orphan.srt", "Unchanged"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn plan_in_place_collisions_overrides_apply_undo() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Media");
        std::fs::create_dir_all(root.join("Heat (1995)")).unwrap();
        std::fs::write(root.join("Heat (1995)/Heat (1995).mkv"), b"old").unwrap();
        std::fs::write(root.join("Heat.1995.1080p.mkv"), b"new").unwrap();
        std::fs::write(root.join("Heat.1995.1080p.en.srt"), b"s").unwrap();
        std::fs::write(root.join("mystery file.mkv"), b"m").unwrap();
        let files = vec![
            root.join("Heat (1995)/Heat (1995).mkv"),
            root.join("Heat.1995.1080p.en.srt"),
            root.join("Heat.1995.1080p.mkv"),
            root.join("mystery file.mkv"),
        ];
        let mut rows = analyze_files(&root, &files);
        let dest = Destination::default();
        let find = |p: &Plan, n: &str| p.items.iter().find(|i| i.source.ends_with(n)).unwrap().clone();
        let plan = build_plan(&root, &rows, &dest);
        assert_eq!(find(&plan, "Heat (1995).mkv").status, Status::AlreadySorted);
        // the target exists -> never overwritten, " (1)" suffix; the subtitle follows
        assert_eq!(find(&plan, "Heat.1995.1080p.mkv").dest_path, root.join("Heat (1995)/Heat (1995) (1).mkv"));
        assert_eq!(find(&plan, "Heat.1995.1080p.en.srt").dest_path, root.join("Heat (1995)/Heat (1995) (1).en.srt"));
        // override: the unidentified file is an episode
        let i = rows.iter().position(|r| r.source.ends_with("mystery file.mkv")).unwrap();
        assert_eq!(rows[i].analysis.confidence, Confidence::Low);
        let an = &mut rows[i].analysis;
        an.kind = Kind::Episode;
        an.title = "Mystery".into();
        an.season = Some(2);
        an.episode = Some(3);
        let plan = build_plan(&root, &rows, &dest);
        assert_eq!(find(&plan, "mystery file.mkv").dest_path, root.join("Mystery/Season 02/Mystery - s02e03.mkv"));
        rows[i].include = false;
        assert_eq!(find(&build_plan(&root, &rows, &dest), "mystery file.mkv").status, Status::Unchanged);
        rows[i].include = true;
        // apply + undo through the shared ops module
        let plan = build_plan(&root, &rows, &dest);
        let rep = crate::ops::execute(&plan, &std::sync::atomic::AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(std::fs::read(root.join("Heat (1995)/Heat (1995).mkv")).unwrap(), b"old");
        assert_eq!(std::fs::read(root.join("Heat (1995)/Heat (1995) (1).mkv")).unwrap(), b"new");
        assert!(root.join("Heat (1995)/Heat (1995) (1).en.srt").is_file());
        assert!(root.join("Mystery/Season 02/Mystery - s02e03.mkv").is_file());
        let u = crate::ops::undo(&rep.undo_log.unwrap(), &mut |_, _| {});
        assert!(u.errors.is_empty(), "{:?}", u.errors);
        assert!(root.join("Heat.1995.1080p.mkv").is_file() && root.join("mystery file.mkv").is_file());
        assert!(!root.join("Mystery").exists(), "folders created by Apply are removed by Undo");
    }
}
