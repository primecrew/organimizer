//! Plex mode (3.0): work out what a video file is from its name and the
//! folders around it, and propose the layout Plex expects.
//!
//! * Movies:   `Title (Year)/Title (Year) {edition-X}.ext`
//! * Episodes: `Show (Year)/Season 01/Show (Year) - s01e02[-e03][ - Title].ext`
//!   (date-based shows: `Show/Season 08/Show - 2011-11-15.ext` when the
//!   season is known, otherwise `Show/Season 2024/Show - 2024-03-15.ext`;
//!   specials go to `Season 00`; stacked parts keep `-pt1` / `-cd2` etc.)
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
use crate::plan::{Plan, PlanItem, Status, same_file, split_name, unique_dest_for};

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
    pub fn singular(self) -> &'static str {
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
    /// Stacked / split part, if the name ended in a Plex split token
    /// (`pt1`, `cd2`, `disc1`, `disk1`, `dvd1`, `part2`; only 1-8).
    pub stack: Option<String>,
    pub episode_title: Option<String>,
    pub extra: Option<ExtraKind>,
    /// Extras: the name the file gets inside the extras folder.
    pub extra_name: Option<String>,
    /// Extras: does it belong to a show (true) or a movie (false)?
    pub extra_of_show: bool,
    pub confidence: Confidence,
    pub notes: Vec<String>,
    /// Plain-language reasons for the decision ("Why" column), in order.
    pub why: Vec<String>,
    /// TMDB id of the confirmed movie / show (adds `{tmdb-ID}` to the
    /// folder name when set).
    pub tmdb_id: Option<u64>,
    /// Identity confirmed on TMDB by the user (lookup feature).
    pub from_tmdb: bool,
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
            stack: None,
            episode_title: None,
            extra: None,
            extra_name: None,
            extra_of_show: false,
            confidence: Confidence::Low,
            notes: vec![],
            why: vec![],
            tmdb_id: None,
            from_tmdb: false,
        }
    }

    pub fn because(&mut self, reason: impl Into<String>) {
        let r = reason.into();
        if !r.is_empty() && !self.why.contains(&r) {
            self.why.push(r);
        }
    }

    /// The reasons as one line for the table.
    pub fn why_text(&self) -> String {
        self.why.join("; ")
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
re!(INLINE_EXTRA, r"(?i)-(behindthescenes|deleted|featurette|interview|scene|short|trailer|other)(\d*)$");
// Plex stacked/split files: cdX, discX, diskX, dvdX, partX, ptX (X = 1-8).
re!(STACK, r"(?i)(?:^|[ ._\-])((?:cd|disc|disk|dvd|part|pt)[ ._-]?([1-8]))$");
re!(SAMPLE, r"(?i)(?:^|[ ._\-\[\(])sample(?:$|[ ._\-\]\)])");
re!(SPACES, r"\s+");
re!(
    GENERIC_TRACK,
    r"(?i)^(?:(?:[a-z]{1,2}\d{1,2}|disc ?\d{1,2}|d\d{1,2})[ _-]?)?(?:title|t|track|vts|chapter|clip|segment)[ _-]?\d+(?:[ _-]\d+)*$|^0\d{3,4}$|^\d{5}$"
);
// MakeMKV "<DISC_LABEL>_t00" (label = often the movie name)
re!(DISC_TRACK_SUFFIX, r"(?i)[ _-](?:b\d{1,2}_)?t(\d{2,3})$");
// folders of a disc structure / multi-disc sets: never the title
re!(
    DISC_DIR,
    r"(?i)^(?:video_ts|audio_ts|bdmv|stream|playlist|clipinf|backup|certificate|meta|jar|bdjo|auxdata|makemkv|(?:disc|disk|dvd|cd|bd|d)[ ._-]?\d{1,2})$"
);
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
    let all_upper =
        letters.len() > 4 && letters.iter().all(|c| c.is_uppercase()) && !s.contains('.') && !s.contains('-');
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
        let t = tidy(s[..a].trim_end_matches([' ', '.', '_', '-', '(', '[']));
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

/// The extras word found in a name, quoted ("\"making of\""), for the Why text.
fn keyword_quote(s: &str) -> Option<String> {
    let l = format!(" {} ", s.to_lowercase().replace(['.', '_', '-', '(', ')', '[', ']'], " "));
    let l = SPACES.replace_all(&l, " ");
    [
        "behind the scenes",
        "making of",
        "deleted scenes",
        "deleted scene",
        "gag reel",
        "short film",
        "trailer",
        "teaser",
        "bts",
        "deleted",
        "featurette",
        "featurettes",
        "interview",
        "interviews",
        "bloopers",
        "blooper",
        "outtakes",
        "bonus",
        "trailers",
    ]
    .iter()
    .find(|w| l.contains(&format!(" {w} ")))
    .map(|w| format!("\"{w}\""))
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
            let mut h = hit(c.get(1).unwrap().start(), c.get(3).unwrap().end(), None, None, None, true);
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
    why: String,
) -> Analysis {
    let mut a = Analysis::new(Kind::Extra);
    a.because(why);
    a.extra = Some(kind);
    a.title = folder_title.to_string();
    a.year = folder_year;
    a.extra_name = Some(extra_display_name(name_part, kind, folder_title));
    a.confidence = if folder_title.is_empty() { Confidence::Low } else { conf };
    if folder_title.is_empty() {
        a.notes.push("Extra: which movie or show? Enter the title".into());
    } else {
        a.because(format!("belongs to {} (folder name)", name_with_year(folder_title, folder_year)));
    }
    a
}

/// The name an extra gets inside its extras folder: the movie title, year and
/// release junk are dropped, "Featurette - The Beast Within" keeps only the
/// description, and a bare keyword ("Deleted Scenes") stays as it is.
/// `alien making of` -> `Making of`.
fn extra_display_name(name_part: &str, kind: ExtraKind, movie_title: &str) -> String {
    let cut = earliest_junk(name_part, 1, false).filter(|&j| j > 0).unwrap_or(name_part.len());
    let t = tidy(&name_part[..cut]);
    let mut words: Vec<&str> = t.split_whitespace().collect();
    // drop the movie title at the start (word by word, ignoring case/punctuation)
    let title_words: Vec<String> = movie_title.split_whitespace().map(norm).filter(|w| !w.is_empty()).collect();
    if !title_words.is_empty()
        && words.len() >= title_words.len()
        && words.iter().take(title_words.len()).map(|w| norm(w)).eq(title_words.iter().cloned())
    {
        words.drain(..title_words.len());
    }
    let is_sep = |w: &str| w.chars().all(|c| "-:–|,".contains(c));
    let is_year = |w: &str| {
        let d = w.trim_matches(['(', ')', '[', ']']);
        d.len() == 4 && d.parse::<u16>().is_ok_and(|y| (1900..=2100).contains(&y))
    };
    while words.first().is_some_and(|w| is_sep(w) || is_year(w)) {
        words.remove(0);
    }
    while words.last().is_some_and(|w| is_sep(w) || is_year(w)) {
        words.pop();
    }
    let mut rest = words.join(" ");
    // "Featurette - The Beast Within" -> "The Beast Within"
    if let Some(kw) = keyword_quote(&rest).map(|q| q.trim_matches('"').to_string()) {
        let low = rest.to_lowercase();
        if low.starts_with(&kw) {
            let after = rest[kw.len()..].trim_start();
            if let Some(desc) = after.strip_prefix(['-', ':', '–']) {
                let desc = desc.trim();
                if !desc.is_empty() {
                    rest = desc.to_string();
                }
            }
        }
    }
    let rest = rest.trim().to_string();
    if rest.is_empty() || norm(&rest) == norm(movie_title) {
        return kind.singular().to_string();
    }
    let rest = smart_case(&rest);
    let mut c = rest.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => kind.singular().to_string(),
    }
}

/// Analyze one video file. `folders` are the names of the folders that
/// Pull a Plex stacked-part suffix off the file stem (` - pt1`, `_cd2`).
/// Official tokens: cd, disc, disk, dvd, part, pt, with a number 1-8.
fn take_stack(stem: &str) -> (String, Option<String>) {
    let Some(c) = STACK.captures(stem) else {
        return (stem.to_string(), None);
    };
    let raw = c.get(1).unwrap().as_str();
    let n = c.get(2).unwrap().as_str();
    let kind = {
        let l = raw.to_ascii_lowercase();
        if l.starts_with("part") {
            "part"
        } else if l.starts_with("disc") {
            "disc"
        } else if l.starts_with("disk") {
            "disk"
        } else if l.starts_with("dvd") {
            "dvd"
        } else if l.starts_with("cd") {
            "cd"
        } else {
            "pt"
        }
    };
    let tag = format!("{kind}{n}");
    let left = stem[..c.get(0).unwrap().start()].trim_end_matches([' ', '.', '_', '-']);
    (left.to_string(), Some(tag))
}

fn stamp_stack(mut a: Analysis, stack: Option<String>) -> Analysis {
    if let Some(s) = stack
        && matches!(a.kind, Kind::Movie | Kind::Episode)
    {
        a.stack = Some(s.clone());
        a.because(format!("split part \"{s}\" (Plex stacked file)"));
    }
    a
}

/// contain it, outermost first (normally starting with the scanned folder's
/// own name); `file_name` includes the extension.
pub fn analyze(folders: &[String], file_name: &str) -> Analysis {
    let (raw_stem, _ext) = split_name(file_name);
    let (stem_owned, stack) = take_stack(raw_stem);
    let stem = stem_owned.as_str();
    let parent = folders.last().map(String::as_str).unwrap_or("");

    if SAMPLE.is_match(stem) || parent.eq_ignore_ascii_case("sample") || parent.eq_ignore_ascii_case("samples") {
        let mut a = Analysis::new(Kind::Skip);
        a.title = tidy(stem);
        a.confidence = Confidence::High;
        a.notes.push("Sample clip: left where it is".into());
        a.because(if SAMPLE.is_match(stem) {
            "\"sample\" in the name: a preview clip, not moved"
        } else {
            "in a Sample folder: a preview clip, not moved"
        });
        return a;
    }

    // folder context: Season folder, extras folder, then the show / movie folder
    let mut season_from_dir = None;
    let mut extra_from_dir = None;
    let mut context: Vec<&str> = vec![];
    let mut disc_dirs: Vec<&str> = vec![];
    for f in folders.iter().rev() {
        if DISC_DIR.is_match(f.trim()) {
            if context.is_empty() {
                disc_dirs.push(f.as_str());
            }
            continue;
        }
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
    let disc_note = if disc_dirs.is_empty() {
        String::new()
    } else {
        let mut d = disc_dirs.clone();
        d.reverse();
        format!("inside a disc rip ({})", d.join("\\"))
    };
    // MakeMKV "LABEL_t03": the label is the title, t03 the disc track
    let (stem, disc_track) = match DISC_TRACK_SUFFIX.captures(stem) {
        Some(c) if c.get(0).unwrap().start() > 0 => (&stem[..c.get(0).unwrap().start()], c[1].parse::<u32>().ok()),
        _ => (stem, None),
    };

    // inline Plex suffix: "Name-trailer" or "Name-deleted1" (a number index is allowed)
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
        let why = format!("name ends in \"{}\" (Plex extras suffix) -> {}", c.get(0).unwrap().as_str(), kind.folder());
        let mut a = extra_analysis(kind, name_part, &folder_title, folder_year, Confidence::High, why);
        if let Some(n) = c.get(2).map(|m| m.as_str()).filter(|n| !n.is_empty()) {
            let base = a.extra_name.clone().unwrap_or_default();
            a.extra_name = Some(if base.is_empty() { n.to_string() } else { format!("{base} {n}") });
        }
        return a;
    }
    if let Some(dir_kind) = extra_from_dir {
        // a generic "Extras" folder: the name can still say which kind
        let kind = match (dir_kind, extra_keyword(stem)) {
            (ExtraKind::Other, Some(k)) => k,
            _ => dir_kind,
        };
        let why = if kind == dir_kind {
            format!("in a \"{parent}\" folder -> {}", kind.folder())
        } else {
            format!(
                "in a \"{parent}\" folder and {} in the name -> {}",
                keyword_quote(stem).unwrap_or_default(),
                kind.folder()
            )
        };
        return extra_analysis(kind, stem, &folder_title, folder_year, Confidence::High, why);
    }

    // TV episode?
    let tv = find_tv(stem).or_else(|| season_from_dir.and_then(|_| find_episode_only(stem)));
    if let Some(h) = tv {
        let mut a = Analysis::new(Kind::Episode);
        let (show, show_year) = title_and_year(stem[..h.start].trim_end_matches([' ', '.', '_', '-', '[', '(']));
        let show = if looks_generic(&show) { String::new() } else { show };
        a.season = h.season.or(season_from_dir).or(if h.date.is_some() { None } else { Some(1) });
        a.episode = h.episode;
        a.episode_end = h.episode_end.filter(|e| Some(*e) > h.episode);
        a.air_date = h.date;
        let mut conf = if h.strong { Confidence::High } else { Confidence::Medium };
        let marker = stem[h.start..h.end].trim().to_string();
        a.because(match (h.date, h.strong) {
            (Some(_), _) => format!("air date \"{marker}\" in the name -> TV episode (date-based show)"),
            (None, true) => format!("\"{marker}\" in the name -> TV episode"),
            (None, false) => format!("number \"{marker}\" inside a Season folder -> episode"),
        });
        if let Some(sn) = season_from_dir {
            a.because(format!("in a \"{parent}\" folder -> Season {sn}"));
        }
        if h.season.is_none() && season_from_dir.is_none() && h.date.is_none() {
            a.notes.push("No season found: assumed Season 1".into());
            conf = Confidence::Low;
        }
        if !show.is_empty() {
            a.year = show_year.or(if norm(&show) == norm(&folder_title) { folder_year } else { None });
            let (ns, nf) = (norm(&show), norm(&folder_title));
            if !folder_title.is_empty() && (ns == nf || (ns.len() >= 4 && nf.ends_with(&ns))) {
                a.title = folder_title.clone(); // keep the folder's spelling / case
                a.because(format!("show name \"{}\" in the name matches the folder", a.title));
            } else {
                a.because(format!("show name \"{show}\" from the file name"));
                a.title = show;
            }
        } else if !folder_title.is_empty() {
            a.title = folder_title.clone();
            a.year = folder_year;
            if h.strong {
                a.notes.push("Show name taken from the folder".into());
            }
            a.because(format!("show name from the folder \"{folder_name}\""));
            conf = conf.min(if h.strong { Confidence::High } else { Confidence::Medium });
        } else {
            a.notes.push("Show name not found: please enter it".into());
            conf = Confidence::Low;
        }
        // episode title: text after the episode marker, up to the junk
        let rest = stem[h.end..].trim_start_matches([']', ')']);
        let cut = earliest_junk(rest, 0, true).unwrap_or(rest.len());
        let et = tidy(&rest[..cut]);
        if !et.is_empty() && et.chars().any(char::is_alphabetic) && !looks_generic(&et) {
            a.episode_title = Some(smart_case(&et));
        }
        if a.season == Some(0) {
            a.notes.push("Special: Season 00".into());
        }
        if let Some(t) = &a.episode_title {
            a.because(format!("episode title \"{t}\" after the episode number"));
        }
        a.confidence = conf;
        return stamp_stack(a, stack);
    }

    // keyword extras ("Heat Teaser.mkv"); a dated title such as
    // "The Interview (2014)" stays a movie (unless it sits next to its movie)
    let kw = extra_keyword(stem);
    let (title, year) = title_and_year(stem);
    if let Some(kind) = kw
        && year.is_none()
    {
        let why = format!("{} in the name -> {}", keyword_quote(stem).unwrap_or_default(), kind.folder());
        return extra_analysis(kind, stem, &folder_title, folder_year, Confidence::Medium, why);
    }

    let mut a = Analysis::new(Kind::Movie);
    a.edition = find_edition(stem).or_else(|| find_edition(folder_name));
    let generic = looks_generic(&title) || (disc_track.is_some() && year.is_none() && !folder_title.is_empty());
    a.because("no episode number or extras keyword -> movie");
    if !disc_note.is_empty() {
        a.because(disc_note.clone());
    }
    if let Some(t) = disc_track {
        a.because(format!("disc-rip track name (track {t}): the name is the disc label, not a title"));
    } else if looks_generic(&title) {
        a.because(format!("\"{stem}\" is a generic / disc track name, so the folder decides"));
    }
    match (generic, year, folder_year) {
        (false, Some(y), fy) => {
            a.because(format!("title \"{title}\" and year {y} from the file name"));
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
            a.because(format!("the folder \"{folder_name}\" has the same title and the year {fy}"));
        }
        (_, _, Some(fy)) if !folder_title.is_empty() => {
            a.title = folder_title.clone();
            a.year = Some(fy);
            a.confidence = Confidence::Medium;
            a.notes.push("Title and year taken from the folder".into());
            a.because(format!("title and year from the folder \"{folder_name}\""));
        }
        (false, None, None) => {
            a.because(format!("title \"{title}\" from the file name, no year anywhere"));
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
    if let Some(e) = &a.edition {
        a.because(format!("edition \"{e}\" in the name"));
    }
    if kw.is_some() {
        a.because(format!(
            "{} in the name, but it has its own year: kept as a movie",
            keyword_quote(stem).unwrap_or_default()
        ));
        a.notes.push("Name mentions an extra (trailer, interview...): check the type".into());
        a.confidence = a.confidence.min(Confidence::Medium);
    }
    stamp_stack(a, stack)
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

pub fn name_with_year(title: &str, year: Option<u16>) -> String {
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
    // Plex reads "{tmdb-123}" in the folder name and skips its own guess
    let mut p = PathBuf::from(match a.tmdb_id {
        Some(id) => format!("{top} {{tmdb-{id}}}"),
        None => top.clone(),
    });
    match a.kind {
        Kind::Movie => {
            let mut name = top.clone();
            if let Some(e) =
                a.edition.as_deref().map(|e| safe_component(e).replace(['{', '}'], "")).filter(|e| !e.is_empty())
            {
                name.push_str(&format!(" {{edition-{e}}}"));
            }
            if let Some(st) = a.stack.as_deref() {
                name.push_str(&format!(" - {st}"));
            }
            p.push(format!("{name}{ext}"));
        }
        Kind::Episode => {
            let mut name;
            if let Some((y, m, d)) = a.air_date {
                // Plex's official layout still uses the show's Season NN
                // folder for date-based episodes. Use that season when we
                // know it; only if we cannot tell the season do we use the
                // year (Season 2024) as a last-resort folder name.
                if let Some(sn) = a.season {
                    p.push(format!("Season {sn:02}"));
                } else {
                    p.push(format!("Season {y}"));
                }
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
            if let Some(st) = a.stack.as_deref() {
                name.push_str(&format!(" - {st}"));
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
    /// Runtime in seconds from the container header (None = unknown).
    pub duration: Option<u64>,
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
    analyze_files_with(root, files, folder_chain)
}

fn analyze_files_with(root: &Path, files: &[PathBuf], chain: fn(&Path, &Path) -> Vec<String>) -> Vec<Row> {
    let mut rows: Vec<Row> = vec![];
    let mut subs: Vec<&PathBuf> = vec![];
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (_, ext) = split_name(&name);
        let ext = ext.trim_start_matches('.');
        if is_video(ext) {
            let a = analyze(&chain(root, f), &name);
            rows.push(Row {
                source: f.clone(),
                include: a.kind != Kind::Skip,
                detected: a.clone(),
                analysis: a,
                subtitle_of: None,
                sub_tags: vec![],
                duration: crate::duration::media_duration(f),
            });
        } else if is_subtitle(ext) {
            subs.push(f);
        }
    }
    link_extras(&mut rows);
    refine_by_runtime(&mut rows);
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
            duration: None,
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
        let Some((_, msrc, m)) = main else { continue };
        let main_name = msrc.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if keyword_movie {
            if norm(&r.analysis.title) != norm(&m.title) || (r.analysis.year.is_some() && r.analysis.year != m.year) {
                continue;
            }
            let Some(kind) = kw else { continue };
            let why = format!(
                "{} in the name, next to {} -> {}",
                keyword_quote(&stem).unwrap_or_default(),
                main_name,
                kind.folder()
            );
            let mut a = extra_analysis(kind, &stem, &m.title, m.year, Confidence::Medium, why);
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
        if let (Some(k), Some(n)) = (a.extra, a.extra_name.clone())
            && norm(&n).starts_with(&norm(&m.title))
        {
            a.extra_name = Some(extra_display_name(&n, k, &m.title));
        }
        if before.is_empty() || norm(&before) == norm(&m.title) {
            a.confidence = a.confidence.max(Confidence::Medium);
            if !before.is_empty() {
                a.confidence = Confidence::High;
            }
        }
        a.notes.retain(|n| !n.starts_with("Extra: which"));
        a.why.retain(|w| !w.starts_with("belongs to "));
        a.because(format!("belongs to {} ({main_name} is next to it)", name_with_year(&m.title, m.year)));
        r.detected = r.analysis.clone();
    }
}

/// Disc root of a file: its folder, or the folder above a disc structure
/// (`Movie/BDMV/STREAM/00001.m2ts` -> `Movie`).
fn unit_dir(p: &Path) -> PathBuf {
    let mut d = dir_of(p);
    while d.file_name().is_some_and(|n| DISC_DIR.is_match(n.to_string_lossy().trim())) {
        match d.parent() {
            Some(x) => d = x.to_path_buf(),
            None => break,
        }
    }
    d
}

fn size_label(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{} MB", n >> 20)
    } else {
        format!("{} KB", (n >> 10).max(1))
    }
}

/// Extras type from the runtime alone (no keyword, no extras folder).
fn extra_kind_by_runtime(secs: u64) -> (ExtraKind, &'static str) {
    if secs <= 150 {
        (ExtraKind::Trailers, "very short, like a trailer")
    } else if secs <= 45 * 60 {
        (ExtraKind::Featurettes, "short bonus feature")
    } else {
        (ExtraKind::Other, "long, but shorter than the main feature (alternate cut?)")
    }
}

/// Several files that look like the same movie in one folder / disc rip:
/// the longest is the main feature, the others become its extras. Extras
/// get their runtime in the Why text. Different editions stay movies.
fn refine_by_runtime(rows: &mut [Row]) {
    let mut groups: HashMap<(PathBuf, String, Option<u16>), Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if r.analysis.kind == Kind::Movie && !r.analysis.title.is_empty() && r.analysis.edition.is_none() {
            groups.entry((unit_dir(&r.source), norm(&r.analysis.title), r.analysis.year)).or_default().push(i);
        }
    }
    for idx in groups.into_values().filter(|v| v.len() > 1) {
        let durations: Option<Vec<(usize, u64)>> = idx.iter().map(|&i| rows[i].duration.map(|d| (i, d))).collect();
        let sizes: Option<Vec<(usize, u64)>> = idx
            .iter()
            .map(|&i| std::fs::metadata(&rows[i].source).ok().map(|m| (i, m.len())).filter(|(_, n)| *n > 0))
            .collect();
        let (measure, by_runtime) = match (durations, sizes) {
            (Some(d), _) => (d, true),
            (None, Some(s)) => (s, false),
            _ => {
                for &i in &idx {
                    let a = &mut rows[i].analysis;
                    a.confidence = a.confidence.min(Confidence::Low);
                    a.notes.push("Several files look like this movie: mark which ones are extras".into());
                    rows[i].detected = rows[i].analysis.clone();
                }
                continue;
            }
        };
        let fmt = |v: u64| if by_runtime { crate::duration::label(v) } else { size_label(v) };
        let &(main, main_v) = measure.iter().max_by_key(|(_, d)| *d).unwrap();
        let main_name = rows[main].source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (title, year) = (rows[main].analysis.title.clone(), rows[main].analysis.year);
        rows[main].analysis.because(format!(
            "{} of {} files for this title ({}) -> main feature",
            if by_runtime { "longest" } else { "largest" },
            idx.len(),
            fmt(main_v)
        ));
        rows[main].detected = rows[main].analysis.clone();
        for &(i, v) in measure.iter().filter(|(i, _)| *i != main) {
            let stem = file_stem_of(&rows[i].source);
            let (kind, why_kind, conf) = match extra_keyword(&stem) {
                Some(k) => (k, format!("{} in the name", keyword_quote(&stem).unwrap_or_default()), Confidence::Medium),
                None if by_runtime => {
                    let (k, w) = extra_kind_by_runtime(v);
                    (k, w.to_string(), Confidence::Medium)
                }
                None => (ExtraKind::Other, "runtime unknown, so the type is a guess".to_string(), Confidence::Low),
            };
            let old_why = rows[i].analysis.why.clone();
            let why = format!(
                "{} ({}) than {} ({}) with the same title -> extra; {} -> {}",
                if by_runtime { "shorter" } else { "smaller" },
                fmt(v),
                main_name,
                fmt(main_v),
                why_kind,
                kind.folder()
            );
            let mut a = extra_analysis(kind, &stem, &title, year, conf, why);
            // keep the disc-rip reasons, drop "-> movie"
            for w in
                old_why.into_iter().filter(|w| w.starts_with("inside a disc rip") || w.starts_with("disc-rip track"))
            {
                a.because(w);
            }
            if looks_generic(&stem)
                || DISC_TRACK_SUFFIX.is_match(&stem)
                || a.extra_name.as_deref().map(norm) == Some(norm(&title))
            {
                let n = if by_runtime { crate::duration::label(v).replace(':', ".") } else { stem.clone() };
                a.extra_name = Some(format!("{} {n}", kind.singular()));
            }
            rows[i].analysis = a;
            rows[i].detected = rows[i].analysis.clone();
        }
    }
    // runtime in the Why text of every other extra
    for r in rows.iter_mut() {
        if r.analysis.kind == Kind::Extra
            && let Some(d) = r.duration
            && !r.analysis.why.iter().any(|w| w.contains(&crate::duration::label(d)))
        {
            r.analysis.why.insert(0, format!("runtime {}", crate::duration::label(d)));
            r.detected = r.analysis.clone();
        }
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
    // same name with other separators: "Alien.1979.en.srt" next to
    // "Alien (1979).mkv" (one video only, so nothing is guessed)
    let ns = norm(&stem);
    let loose: Vec<(usize, usize)> = videos
        .iter()
        .enumerate()
        .filter(|(_, r)| dir_of(&r.source) == dir)
        .filter_map(|(i, r)| {
            let nv = norm(&file_stem_of(&r.source));
            (nv.len() >= 3 && ns.starts_with(&nv)).then_some((i, nv.chars().count()))
        })
        .collect();
    if let [(i, n)] = loose[..] {
        // byte offset in the subtitle stem after `n` letters/digits
        let mut seen = 0;
        let mut cut = stem.len();
        for (b, c) in stem.char_indices() {
            if seen == n {
                cut = b;
                break;
            }
            if c.is_alphanumeric() {
                seen += c.to_lowercase().count();
            }
        }
        return (Some(i), subtitle_tags(&stem[cut..]));
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

// ---------------------------------------------------------------------------
// Rename only (3.0.6): the folders are already right, only the file names
// change. Nothing moves to another folder and no folder is created.
// ---------------------------------------------------------------------------

/// What the Plex tab does with the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlexMode {
    /// Rename each file where it is, using the show / season / movie
    /// folders it is already in (the default: most libraries already have
    /// the right folders).
    #[default]
    RenameOnly,
    /// Build `Show (Year)\Season NN\` and `Movie (Year)\` (3.0.0-3.0.5).
    FullSort,
}

impl PlexMode {
    pub fn label(self) -> &'static str {
        match self {
            PlexMode::RenameOnly => "Rename only",
            PlexMode::FullSort => "Full sort",
        }
    }
}

re!(ID_TAG, r"(?i)\s*\{(?:tmdb|tvdb|imdb)-([a-z0-9]+)\}");
re!(TMDB_TAG, r"(?i)\{tmdb-(\d+)\}");

/// A show / movie folder name split into title, year and `{tmdb-ID}`.
/// `{edition-...}`, `{imdb-...}` and `{tvdb-...}` tags are ignored.
pub fn parse_title_folder(name: &str) -> (String, Option<u16>, Option<u64>) {
    let tmdb = TMDB_TAG.captures(name).and_then(|c| c[1].parse().ok());
    let plain = ID_TAG.replace_all(name, "");
    let plain = EDITION_TAG.replace_all(&plain, "");
    let (t, y) = title_and_year(plain.trim());
    let t = if looks_generic(&t) { String::new() } else { t };
    (t, y, tmdb)
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Folder names to analyze a file with in Rename only: like
/// [`folder_chain`], but when the chosen folder is itself a Season, extras
/// or disc folder the folders above it are included (so picking
/// `Show (2008)\Season 02` still knows the show).
pub fn rename_only_chain(root: &Path, file: &Path) -> Vec<String> {
    let mut start = root.to_path_buf();
    for _ in 0..3 {
        let n = name_of(&start);
        let inner = season_of_dir(&n).is_some() || extra_of_dir(&n).is_some() || DISC_DIR.is_match(n.trim());
        match start.parent() {
            Some(p) if inner && !name_of(p).is_empty() => start = p.to_path_buf(),
            _ => break,
        }
    }
    folder_chain(&start, file)
}

/// Like [`analyze_files`] for the chosen mode.
pub fn analyze_files_mode(root: &Path, files: &[PathBuf], mode: PlexMode) -> Vec<Row> {
    match mode {
        PlexMode::FullSort => analyze_files(root, files),
        PlexMode::RenameOnly => {
            let mut rows = analyze_files_with(root, files, rename_only_chain);
            rename_only_review(&mut rows);
            rows
        }
    }
}

fn flag(a: &mut Analysis, note: impl Into<String>) {
    let n = note.into();
    if !a.notes.contains(&n) {
        a.notes.push(n);
    }
    a.confidence = a.confidence.min(Confidence::Medium);
}

/// Rename only: take the show / season / movie from the folders the file
/// is in (it stays there), and flag rows whose folders do not look right
/// (Medium confidence + a note, so Look up asks for the name instead of
/// guessing).
pub fn rename_only_review(rows: &mut [Row]) {
    for r in rows.iter_mut().filter(|r| r.subtitle_of.is_none()) {
        let a = &mut r.analysis;
        if !matches!(a.kind, Kind::Movie | Kind::Episode) || a.from_tmdb {
            continue;
        }
        let parent = unit_dir(&r.source);
        let pname = name_of(&parent);
        let (title_dir, season_dir) = match season_of_dir(&pname) {
            Some(sn) if a.kind == Kind::Episode => (parent.parent().map(Path::to_path_buf), Some(sn)),
            _ => (Some(parent.clone()), None),
        };
        let (ft, fy, ftmdb) = title_dir.as_deref().map(|d| parse_title_folder(&name_of(d))).unwrap_or_default();
        let what = if a.kind == Kind::Episode { "show" } else { "movie" };
        if ft.is_empty() {
            if a.kind == Kind::Episode && season_dir.is_some() {
                flag(a, "Rename only: this Season folder is not inside a show folder: check the show name (Look up)");
            } else {
                flag(a, format!("Rename only: not inside its own {what} folder: check the name (Look up)"));
            }
        } else if a.title.is_empty() || norm(&a.title) == norm(&ft) || (a.kind == Kind::Episode && season_dir.is_some())
        {
            let differs = !a.title.is_empty() && norm(&a.title) != norm(&ft);
            let old = std::mem::replace(&mut a.title, ft.clone());
            if fy.is_some() {
                a.year = fy;
            }
            a.tmdb_id = ftmdb.or(a.tmdb_id);
            a.because(format!(
                "Rename only: {what} name from its folder \"{}\"",
                name_of(title_dir.as_deref().unwrap_or(&parent))
            ));
            if differs {
                flag(a, format!("Rename only: the file name says \"{old}\", the folder says \"{ft}\""));
            }
        } else {
            flag(
                a,
                format!(
                    "Rename only: the folder \"{}\" does not match \"{}\": check the name (Look up)",
                    name_of(title_dir.as_deref().unwrap_or(&parent)),
                    a.title
                ),
            );
        }
        if let Some(sn) = season_dir {
            if a.season.is_some() && a.season != Some(sn) && a.air_date.is_none() {
                flag(
                    a,
                    format!(
                        "Rename only: the file name says season {}, the folder says Season {sn:02}",
                        a.season.unwrap_or(0)
                    ),
                );
            }
            a.season = Some(sn);
        } else if a.kind == Kind::Episode && a.air_date.is_none() {
            flag(a, "Rename only: not inside a Season folder (Plex expects Show\\Season NN)");
        }
        if a.year.is_none() && a.tmdb_id.is_none() && !a.title.is_empty() {
            flag(a, format!("Rename only: no year for this {what}: add it, or look it up"));
        }
        r.detected = r.analysis.clone();
    }
}

fn inline_suffix(k: ExtraKind) -> &'static str {
    match k {
        ExtraKind::BehindTheScenes => "behindthescenes",
        ExtraKind::DeletedScenes => "deleted",
        ExtraKind::Featurettes => "featurette",
        ExtraKind::Interviews => "interview",
        ExtraKind::Scenes => "scene",
        ExtraKind::Shorts => "short",
        ExtraKind::Trailers => "trailer",
        ExtraKind::Other => "other",
    }
}

/// Rename only: the new file name for a video in `dir` (its own folder).
/// Extras inside a matching extras folder get the plain name; extras
/// anywhere else keep Plex's inline suffix (`Name-trailer.mkv`).
pub fn rename_only_name(a: &Analysis, ext: &str, dir: &Path) -> Option<String> {
    let rel = relative_path(a, ext)?;
    if a.kind == Kind::Extra {
        let k = a.extra.unwrap_or(ExtraKind::Other);
        if extra_of_dir(&name_of(dir)) != Some(k) {
            let n = a.extra_name.as_deref().map(safe_component).filter(|n| !n.is_empty());
            let n = n.unwrap_or_else(|| k.singular().to_string());
            return Some(format!("{n}-{}{}", inline_suffix(k), ext.to_lowercase()));
        }
    }
    rel.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// A folder rename offered in Rename only (`Season 2` -> `Season 02`,
/// adding the year / `{tmdb-ID}`, official extras folder names). Shown as
/// its own preview row, unticked by default.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderFix {
    pub from: PathBuf,
    /// Same parent, new name.
    pub to: PathBuf,
    pub why: String,
    pub include: bool,
    /// Why it cannot be done (name taken, ...): never applied.
    pub problem: Option<String>,
}

/// Folder-name fixes for Rename only, deepest folder first (that is the
/// order they are applied in, after all file renames). Only folders
/// strictly inside `root` are offered: the chosen folder itself holds the
/// undo log, so it is never renamed.
pub fn folder_fixes(root: &Path, rows: &[Row]) -> Vec<FolderFix> {
    let mut out: Vec<FolderFix> = vec![];
    let key = |p: &Path| p.to_string_lossy().to_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add = |from: &Path, to_name: String, why: String, out: &mut Vec<FolderFix>| {
        let to_name = safe_component(&to_name);
        if to_name.is_empty() || name_of(from) == to_name || from == root || !from.starts_with(root) {
            return;
        }
        if !seen.insert(key(from)) {
            return;
        }
        let to = from.with_file_name(&to_name);
        out.push(FolderFix { from: from.to_path_buf(), to, why, include: false, problem: None });
    };
    for r in rows.iter().filter(|r| r.subtitle_of.is_none() && r.include) {
        let a = &r.analysis;
        if missing(a).is_some() {
            continue;
        }
        let dir = unit_dir(&r.source);
        let dname = name_of(&dir);
        let title_dir = match a.kind {
            Kind::Episode => match season_of_dir(&dname) {
                Some(sn) => {
                    if Some(sn) == a.season {
                        let want = format!("Season {sn:02}");
                        let why = if sn == 0 && !dname.to_lowercase().starts_with("season") {
                            "Plex keeps specials in \"Season 00\"".to_string()
                        } else {
                            "Plex season folders are \"Season NN\"".to_string()
                        };
                        add(&dir, want, why, &mut out);
                    }
                    dir.parent().map(Path::to_path_buf)
                }
                None => Some(dir.clone()),
            },
            Kind::Movie => Some(dir.clone()),
            Kind::Extra => {
                if let (Some(k), Some(dk)) = (a.extra, extra_of_dir(&dname))
                    && k == dk
                {
                    add(&dir, k.folder().to_string(), "official Plex extras folder name".into(), &mut out);
                }
                None
            }
            Kind::Skip => None,
        };
        let Some(td) = title_dir else { continue };
        let tname = name_of(&td);
        let (ft, fy, ftmdb) = parse_title_folder(&tname);
        if ft.is_empty() || norm(&ft) != norm(&a.title) {
            continue;
        }
        let add_year = fy.is_none() && a.year.is_some();
        let add_tmdb = ftmdb.is_none() && a.tmdb_id.is_some();
        if !add_year && !add_tmdb {
            continue;
        }
        let mut want = safe_component(&name_with_year(&ft, a.year.or(fy)));
        if let Some(m) = EDITION_TAG.find(&tname) {
            want.push(' ');
            want.push_str(m.as_str());
        }
        if let Some(id) = ftmdb.or(a.tmdb_id) {
            want.push_str(&format!(" {{tmdb-{id}}}"));
        }
        let why = match (add_year, add_tmdb) {
            (true, true) => "adds the year and the TMDB id",
            (true, false) => "adds the year",
            _ => "adds the TMDB id (Plex then skips its own guess)",
        };
        add(&td, want, why.into(), &mut out);
    }
    // never onto something that exists, never two folders to one name
    let mut targets: HashSet<String> = HashSet::new();
    for f in out.iter_mut() {
        if f.to.exists() && !same_file(&f.from, &f.to) {
            f.problem = Some("A folder or file with that name already exists: left as it is".into());
        } else if !targets.insert(key(&f.to)) {
            f.problem = Some("Another folder gets the same name: left as it is".into());
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.from.components().count()));
    out
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
    build_plan_mode(root, rows, dest, PlexMode::FullSort, &[])
}

/// [`build_plan`] for either mode. Rename only: every file keeps its
/// folder (`dest` is ignored) and the ticked `fixes` are added after all
/// file items, deepest folder first, so file names are computed with the
/// folder names as they are now and the folders are renamed last.
pub fn build_plan_mode(root: &Path, rows: &[Row], dest: &Destination, mode: PlexMode, fixes: &[FolderFix]) -> Plan {
    let rename_only = mode == PlexMode::RenameOnly;
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
            is_dir: false,
        };
        let (lib_root, rel) = match r.subtitle_of {
            None if rename_only => {
                (current_dir.clone(), rename_only_name(&r.analysis, ext, &current_dir).map(PathBuf::from))
            }
            None => (dest.root_for(library_of(&r.analysis), root), relative_path(&r.analysis, ext)),
            Some(v) => match video_final.get(&v) {
                Some((_, vp)) if rename_only => {
                    // a subtitle stays in its own folder, named after its video
                    let mut n = file_stem_of(vp);
                    for t in &r.sub_tags {
                        n.push('.');
                        n.push_str(t);
                    }
                    n.push_str(&ext.to_lowercase());
                    (current_dir.clone(), Some(PathBuf::from(n)))
                }
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
    let mut items: Vec<PlanItem> = by_index.into_iter().flatten().collect();
    if rename_only {
        items.extend(fixes.iter().map(folder_fix_item));
    }
    Plan { root: root.to_path_buf(), items }
}

fn folder_fix_item(f: &FolderFix) -> PlanItem {
    let parent = dir_of(&f.from);
    let mut warnings = vec![];
    let status = match (&f.problem, f.include) {
        (Some(p), _) => {
            warnings.push(p.clone());
            Status::Unchanged
        }
        (None, false) => {
            warnings.push("Not ticked".into());
            Status::Unchanged
        }
        (None, true) => Status::Move,
    };
    PlanItem {
        source: f.from.clone(),
        current_dir: parent.clone(),
        base_dir: parent.clone(),
        date: None,
        new_name: name_of(&f.to),
        dest_folder_name: String::new(),
        dest_dir: parent,
        dest_path: f.to.clone(),
        renamed_for_collision: false,
        status,
        warnings,
        is_dir: true,
    }
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
            (
                "Movies/The Dark Knight (2008)/The Dark Knight (2008) - pt1.mp4",
                "The Dark Knight (2008)/The Dark Knight (2008) - pt1.mp4",
            ),
            (
                "Movies/The Dark Knight (2008)/The Dark Knight (2008) - pt2.mp4",
                "The Dark Knight (2008)/The Dark Knight (2008) - pt2.mp4",
            ),
            (
                "Movies/The Dark Knight (2008)/The Dark Knight (2008) - cd1.mkv",
                "The Dark Knight (2008)/The Dark Knight (2008) - cd1.mkv",
            ),
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
                "TV/The Daily Show/Season 08/The Daily Show - 2011-11-15 - Elijah Wood.avi",
                "The Daily Show/Season 08/The Daily Show - 2011-11-15 - Elijah Wood.avi",
            ),
            (
                "TV/Grey's Anatomy/Season 01/Grey's Anatomy - s01e01 - pt1.avi",
                "Grey's Anatomy/Season 01/Grey's Anatomy - s01e01 - pt1.avi",
            ),
            (
                "TV/Grey's Anatomy/Season 01/Grey's Anatomy - s01e01 - pt2.avi",
                "Grey's Anatomy/Season 01/Grey's Anatomy - s01e01 - pt2.avi",
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
            ("Movies/Heat (1995)/Heat Teaser.mp4", "Heat (1995)/Trailers/Teaser.mp4"),
            ("Movies/Heat (1995)/heat deleted scene 2.mkv", "Heat (1995)/Deleted Scenes/Deleted Scene 2.mkv"),
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

#[cfg(test)]
mod recognition_tests {
    use super::*;

    /// Minimal Matroska header with the given duration (seconds).
    fn mkv(secs: f64) -> Vec<u8> {
        fn el(id: &[u8], payload: &[u8]) -> Vec<u8> {
            let mut v = id.to_vec();
            v.push(0x01);
            v.extend_from_slice(&(payload.len() as u64).to_be_bytes()[1..]);
            v.extend_from_slice(payload);
            v
        }
        let header = el(&[0x1A, 0x45, 0xDF, 0xA3], &el(&[0x42, 0x82], b"matroska"));
        let info = el(
            &[0x15, 0x49, 0xA9, 0x66],
            &[el(&[0x2A, 0xD7, 0xB1], &1_000_000u32.to_be_bytes()), el(&[0x44, 0x89], &(secs * 1000.0).to_be_bytes())]
                .concat(),
        );
        [header, el(&[0x18, 0x53, 0x80, 0x67], &info)].concat()
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn rel_of(r: &Row) -> String {
        let name = r.source.file_name().unwrap().to_string_lossy().into_owned();
        let (_, ext) = split_name(&name);
        relative_path(&r.analysis, ext).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
    }

    fn row<'a>(rows: &'a [Row], end: &str) -> &'a Row {
        rows.iter().find(|r| r.source.to_string_lossy().replace('\\', "/").ends_with(end)).unwrap()
    }

    #[test]
    fn makemkv_rip_longest_track_is_the_movie() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Rips");
        let files = vec![
            write(&root, "Alien (1979)/title_t00.mkv", &mkv(7020.0)),
            write(&root, "Alien (1979)/title_t01.mkv", &mkv(480.0)),
            write(&root, "Alien (1979)/title_t02.mkv", &mkv(130.0)),
            write(&root, "Alien (1979)/title_t03.mkv", &mkv(3900.0)),
        ];
        let rows = analyze_files(&root, &files);
        let main = row(&rows, "title_t00.mkv");
        assert_eq!(rel_of(main), "Alien (1979)/Alien (1979).mkv");
        assert!(main.analysis.why_text().contains("longest of 4 files"), "{}", main.analysis.why_text());
        let t1 = row(&rows, "title_t01.mkv");
        assert_eq!(t1.analysis.extra, Some(ExtraKind::Featurettes));
        assert_eq!(rel_of(t1), "Alien (1979)/Featurettes/Featurette 8.00.mkv");
        assert!(
            t1.analysis.why_text().contains("shorter (8:00) than title_t00.mkv (1:57:00)"),
            "{}",
            t1.analysis.why_text()
        );
        assert_eq!(row(&rows, "title_t02.mkv").analysis.extra, Some(ExtraKind::Trailers));
        assert_eq!(row(&rows, "title_t03.mkv").analysis.extra, Some(ExtraKind::Other));
    }

    #[test]
    fn disc_label_track_names_and_bdmv_structure() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Rips");
        let files = vec![
            write(&root, "Gladiator (2000)/GLADIATOR_t00.mkv", &mkv(9300.0)),
            write(&root, "Gladiator (2000)/GLADIATOR_t05.mkv", &mkv(1260.0)),
            // BDMV: no runtime parser for m2ts, the largest file wins
            write(&root, "Heat (1995)/BDMV/STREAM/00001.m2ts", &vec![0u8; 50_000]),
            write(&root, "Heat (1995)/BDMV/STREAM/00002.m2ts", &vec![0u8; 3_000]),
            write(&root, "Heat (1995)/BDMV/STREAM/00003 Trailer.m2ts", &vec![0u8; 2_000]),
        ];
        let rows = analyze_files(&root, &files);
        let g = row(&rows, "GLADIATOR_t00.mkv");
        assert_eq!(rel_of(g), "Gladiator (2000)/Gladiator (2000).mkv");
        assert!(g.analysis.why_text().contains("disc-rip track name"), "{}", g.analysis.why_text());
        assert_eq!(row(&rows, "GLADIATOR_t05.mkv").analysis.extra, Some(ExtraKind::Featurettes));
        let heat = row(&rows, "00001.m2ts");
        assert_eq!(rel_of(heat), "Heat (1995)/Heat (1995).m2ts");
        assert!(heat.analysis.why_text().contains("inside a disc rip (BDMV\\STREAM)"), "{}", heat.analysis.why_text());
        assert!(heat.analysis.why_text().contains("largest of 2 files"), "{}", heat.analysis.why_text());
        let small = row(&rows, "00002.m2ts");
        assert_eq!(small.analysis.kind, Kind::Extra);
        assert_eq!(small.analysis.confidence, Confidence::Low);
        assert_eq!(row(&rows, "00003 Trailer.m2ts").analysis.extra, Some(ExtraKind::Trailers));
    }

    #[test]
    fn keyword_extra_next_to_its_movie_explains_itself() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Movies");
        let files = vec![
            write(&root, "Alien (1979)/Alien (1979).mkv", &mkv(7020.0)),
            write(&root, "Alien (1979)/Alien - Making of.mkv", &mkv(480.0)),
            write(&root, "Alien (1979)/Deleted Scenes/Chestburster alt.mkv", &mkv(200.0)),
        ];
        let rows = analyze_files(&root, &files);
        let m = row(&rows, "Alien - Making of.mkv");
        assert_eq!(m.analysis.extra, Some(ExtraKind::BehindTheScenes));
        assert_eq!(rel_of(m), "Alien (1979)/Behind The Scenes/Making of.mkv");
        let why = m.analysis.why_text();
        assert!(
            why.contains("runtime 8:00") && why.contains("\"making of\"") && why.contains("Alien (1979).mkv"),
            "{why}"
        );
        let del = row(&rows, "Chestburster alt.mkv");
        assert_eq!(rel_of(del), "Alien (1979)/Deleted Scenes/Chestburster alt.mkv");
        assert!(del.analysis.why_text().contains("in a \"Deleted Scenes\" folder"), "{}", del.analysis.why_text());
        // the movie is not touched by the runtime rule (only one file has its title)
        assert!(!row(&rows, "Alien (1979).mkv").analysis.why_text().contains("longest"));
    }

    #[test]
    fn episodes_explain_season_and_show() {
        let a = analyze(&["TV".into(), "The Expanse".into(), "Season 2".into()], "The.Expanse.S02E05.Home.1080p.mkv");
        let why = a.why_text();
        assert!(why.contains("\"S02E05\" in the name -> TV episode"), "{why}");
        assert!(why.contains("Season 2"), "{why}");
        assert!(why.contains("episode title \"Home\""), "{why}");
        let w = analyze(&["Shows".into(), "Lost".into(), "Season 1".into()], "Episode 4.mkv").why_text();
        assert!(w.contains("inside a Season folder") && w.contains("show name from the folder"), "{w}");
    }

    #[test]
    fn official_plex_split_parts_and_numbered_extras() {
        assert_eq!(take_stack("The Dark Knight (2008) - pt1"), ("The Dark Knight (2008)".into(), Some("pt1".into())));
        assert_eq!(
            take_stack("The Dark Knight (2008) - part2"),
            ("The Dark Knight (2008)".into(), Some("part2".into()))
        );
        assert_eq!(take_stack("Apartment (1960)"), ("Apartment (1960)".into(), None));
        assert_eq!(take_stack("The Party (1968)"), ("The Party (1968)".into(), None));
        let a = analyze(&["Movies".into(), "The Dark Knight (2008)".into()], "The Dark Knight (2008) - disc1.mkv");
        assert_eq!(a.stack.as_deref(), Some("disc1"));
        assert!(a.why_text().contains("split part \"disc1\""), "{}", a.why_text());
        // Plex allows a number index on an inline extras suffix.
        let rel = |path: &str| {
            let parts: Vec<String> = path.split('/').map(String::from).collect();
            let (file, folders) = parts.split_last().unwrap();
            let an = analyze(folders, file);
            let ext = format!(".{}", path.rsplit('.').next().unwrap());
            relative_path(&an, &ext).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap()
        };
        assert_eq!(rel("Movies/Avatar (2009)/Bar Fight-deleted1.mp4"), "Avatar (2009)/Deleted Scenes/Bar Fight 1.mp4");
        assert_eq!(rel("Movies/Avatar (2009)/Bar Fight-deleted2.mp4"), "Avatar (2009)/Deleted Scenes/Bar Fight 2.mp4");
        assert_eq!(rel("Movies/Avatar (2009)/Teaser Trailer-trailer.mp4"), "Avatar (2009)/Trailers/Teaser Trailer.mp4");
    }
}

#[cfg(test)]
mod rename_only_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    fn scan(root: &Path) -> Vec<Row> {
        let mut files = vec![];
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() { stack.push(p) } else { files.push(p) }
            }
        }
        files.sort();
        analyze_files_mode(root, &files, PlexMode::RenameOnly)
    }

    /// (source relative to `base`, destination relative to `base`) per file item.
    fn moves(plan: &Plan, base: &Path) -> Vec<(String, String)> {
        let rel = |p: &Path| p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        plan.items
            .iter()
            .filter(|i| i.status == Status::Move && !i.is_dir)
            .map(|i| (rel(&i.source), rel(&i.dest_path)))
            .collect()
    }

    fn row<'a>(rows: &'a [Row], name: &str) -> &'a Row {
        rows.iter().find(|r| r.source.ends_with(name)).unwrap()
    }

    #[test]
    fn rename_only_is_the_default_mode() {
        assert_eq!(PlexMode::default(), PlexMode::RenameOnly);
    }

    #[test]
    fn inside_a_season_folder() {
        let d = tempfile::tempdir().unwrap();
        let show = d.path().join("Breaking Bad (2008) {tmdb-1396}");
        let season = show.join("Season 2");
        touch(&season.join("breaking.bad.s02e05.720p.WEB-DL.mkv"));
        touch(&season.join("breaking.bad.s02e05.720p.WEB-DL.en.srt"));
        touch(&season.join("Episode 6.mkv"));
        // the user picked the Season folder itself
        let rows = scan(&season);
        let a = &row(&rows, "breaking.bad.s02e05.720p.WEB-DL.mkv").analysis;
        assert_eq!((a.title.as_str(), a.year, a.season, a.tmdb_id), ("Breaking Bad", Some(2008), Some(2), Some(1396)));
        assert_eq!(a.confidence, Confidence::High, "{:?}", a.notes);
        let fixes = folder_fixes(&season, &rows);
        assert!(fixes.is_empty(), "the chosen folder itself is never renamed: {fixes:?}");
        let plan = build_plan_mode(&season, &rows, &Destination::default(), PlexMode::RenameOnly, &fixes);
        let mut m = moves(&plan, &season);
        m.sort();
        assert_eq!(
            m,
            vec![
                ("Episode 6.mkv".into(), "Breaking Bad (2008) - s02e06.mkv".into()),
                ("breaking.bad.s02e05.720p.WEB-DL.en.srt".into(), "Breaking Bad (2008) - s02e05.en.srt".into()),
                ("breaking.bad.s02e05.720p.WEB-DL.mkv".into(), "Breaking Bad (2008) - s02e05.mkv".into()),
            ]
        );
        assert!(plan.items.iter().all(|i| i.dest_dir == season), "nothing moves to another folder");
    }

    #[test]
    fn show_folder_with_season_and_specials_folders() {
        let d = tempfile::tempdir().unwrap();
        let show = d.path().join("Lost (2004)");
        touch(&show.join("Season 1/Lost.S01E02.Tabula.Rasa.mkv"));
        touch(&show.join("Specials/Lost.S00E01.mkv"));
        touch(&show.join("Season 03/Lost (2004) - s03e01 - A Tale of Two Cities.mkv"));
        touch(&show.join("Featurettes/Making Lost.mkv"));
        let rows = scan(&show);
        let fixes = folder_fixes(&show, &rows);
        let names: Vec<(String, String)> = fixes.iter().map(|f| (name_of(&f.from), name_of(&f.to))).collect();
        assert!(names.contains(&("Season 1".into(), "Season 01".into())), "{names:?}");
        assert!(names.contains(&("Specials".into(), "Season 00".into())), "{names:?}");
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(fixes.iter().all(|f| !f.include && f.problem.is_none()), "folder fixes are unticked by default");
        let plan = build_plan_mode(&show, &rows, &Destination::default(), PlexMode::RenameOnly, &fixes);
        let m = moves(&plan, &show);
        assert!(
            m.contains(&(
                "Season 1/Lost.S01E02.Tabula.Rasa.mkv".into(),
                "Season 1/Lost (2004) - s01e02 - Tabula Rasa.mkv".into()
            )),
            "{m:?}"
        );
        assert!(m.contains(&("Specials/Lost.S00E01.mkv".into(), "Specials/Lost (2004) - s00e01.mkv".into())), "{m:?}");
        // already named: nothing to do; the extra keeps its folder and name
        assert!(!m.iter().any(|(s, _)| s.starts_with("Season 03") || s.starts_with("Featurettes")), "{m:?}");
        let dirs: Vec<&PlanItem> = plan.items.iter().filter(|i| i.is_dir).collect();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().all(|i| i.status == Status::Unchanged), "unticked folder rows do nothing");
    }

    #[test]
    fn library_root_with_shows_and_movies() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("TV/The Wire (2002)/Season 01/the.wire.s01e01.mkv"));
        touch(&r.join("Movies/Heat/Heat.1995.1080p.BluRay.x264.mkv"));
        touch(&r.join("Movies/Heat/Heat.1995.1080p.BluRay.x264-trailer.mkv"));
        touch(&r.join("Movies/Alien (1979)/Alien.1979.Directors.Cut.pt1.mkv"));
        let rows = scan(r);
        let plan = build_plan_mode(r, &rows, &Destination::default(), PlexMode::RenameOnly, &[]);
        let mut m = moves(&plan, r);
        m.sort();
        assert!(
            m.contains(&(
                "TV/The Wire (2002)/Season 01/the.wire.s01e01.mkv".into(),
                "TV/The Wire (2002)/Season 01/The Wire (2002) - s01e01.mkv".into()
            )),
            "{m:?}"
        );
        assert!(
            m.contains(&("Movies/Heat/Heat.1995.1080p.BluRay.x264.mkv".into(), "Movies/Heat/Heat (1995).mkv".into())),
            "{m:?}"
        );
        // an extra outside an extras folder keeps Plex's inline suffix
        assert!(
            m.iter().any(|(s, t)| s.ends_with("x264-trailer.mkv")
                && t.starts_with("Movies/Heat/")
                && t.ends_with("-trailer.mkv")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|(_, t)| t.starts_with("Movies/Alien (1979)/Alien (1979) {edition-") && t.ends_with(" - pt1.mkv")),
            "{m:?}"
        );
        // "Heat" -> "Heat (1995)" is offered (unticked); "Movies" / "TV" never
        let fixes = folder_fixes(r, &rows);
        let names: Vec<(String, String)> = fixes.iter().map(|f| (name_of(&f.from), name_of(&f.to))).collect();
        assert_eq!(names, vec![("Heat".to_string(), "Heat (1995)".to_string())]);
    }

    #[test]
    fn doubtful_folders_are_flagged_not_guessed() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        touch(&r.join("Downloads/Season 1/Lost.S01E04.mkv"));
        touch(&r.join("Movies/Arrival.2016.mkv"));
        touch(&r.join("Firefly/Season 1/Serenity.S01E02.mkv"));
        let rows = scan(r);
        let a = &row(&rows, "Lost.S01E04.mkv").analysis;
        assert!(
            a.confidence < Confidence::High && a.notes.iter().any(|n| n.contains("not inside a show folder")),
            "{a:?}"
        );
        let a = &row(&rows, "Arrival.2016.mkv").analysis;
        assert!(a.confidence < Confidence::High && a.notes.iter().any(|n| n.contains("own movie folder")), "{a:?}");
        let a = &row(&rows, "Serenity.S01E02.mkv").analysis;
        assert_eq!(a.title, "Firefly", "the file stays in Firefly\\Season 1");
        assert!(a.confidence < Confidence::High, "file and folder disagree, and no year: {a:?}");
        assert!(crate::lookup::needs_name_prompt(a.confidence), "Look up asks for the name");
    }

    #[test]
    fn folder_fix_collision_is_flagged_and_never_applied() {
        let d = tempfile::tempdir().unwrap();
        let show = d.path().join("Lost (2004)");
        touch(&show.join("Season 1/Lost.S01E02.mkv"));
        std::fs::create_dir_all(show.join("Season 01")).unwrap();
        let rows = scan(&show);
        let mut fixes = folder_fixes(&show, &rows);
        assert_eq!(fixes.len(), 1);
        assert!(fixes[0].problem.is_some());
        fixes[0].include = true;
        let plan = build_plan_mode(&show, &rows, &Destination::default(), PlexMode::RenameOnly, &fixes);
        assert!(plan.items.iter().filter(|i| i.is_dir).all(|i| i.status == Status::Unchanged));
    }

    #[test]
    fn apply_with_folder_fixes_then_undo_restores_everything() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("TV");
        touch(&root.join("Lost/Season 1/Lost.S01E02.mkv"));
        touch(&root.join("Lost/Season 1/Lost.S01E02.eng.srt"));
        touch(&root.join("Lost/Specials/Lost.S00E01.mkv"));
        let mut rows = scan(&root);
        // the user types the year (as the type-the-title prompt would)
        for r in rows.iter_mut() {
            r.analysis.year = Some(2004);
        }
        let mut fixes = folder_fixes(&root, &rows);
        let names: Vec<String> = fixes.iter().map(|f| name_of(&f.to)).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(
            names.contains(&"Lost (2004)".to_string())
                && names.contains(&"Season 01".into())
                && names.contains(&"Season 00".into())
        );
        // deepest first: the Season folders are renamed before the show folder
        assert_eq!(name_of(&fixes[2].to), "Lost (2004)");
        for f in fixes.iter_mut() {
            f.include = true;
        }
        let plan = build_plan_mode(&root, &rows, &Destination::default(), PlexMode::RenameOnly, &fixes);
        let rep = crate::ops::execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert!(root.join("Lost (2004)/Season 01/Lost (2004) - s01e02.mkv").is_file());
        assert!(root.join("Lost (2004)/Season 01/Lost (2004) - s01e02.en.srt").is_file());
        assert!(root.join("Lost (2004)/Season 00/Lost (2004) - s00e01.mkv").is_file());
        assert!(!root.join("Lost").exists());
        let log = rep.undo_log.unwrap();
        let u = crate::ops::undo(&log, &mut |_, _| {});
        assert!(u.errors.is_empty(), "{:?}", u.errors);
        assert!(root.join("Lost/Season 1/Lost.S01E02.mkv").is_file());
        assert!(root.join("Lost/Season 1/Lost.S01E02.eng.srt").is_file());
        assert!(root.join("Lost/Specials/Lost.S00E01.mkv").is_file());
        assert!(!root.join("Lost (2004)").exists());
    }

    #[test]
    fn unticked_folder_fixes_leave_files_correct() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("TV");
        touch(&root.join("Lost (2004)/Season 1/Lost.S01E02.mkv"));
        let rows = scan(&root);
        let fixes = folder_fixes(&root, &rows);
        assert_eq!(fixes.len(), 1);
        let plan = build_plan_mode(&root, &rows, &Destination::default(), PlexMode::RenameOnly, &fixes);
        let rep = crate::ops::execute(&plan, &AtomicBool::new(false), &mut |_, _, _| {});
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert!(root.join("Lost (2004)/Season 1/Lost (2004) - s01e02.mkv").is_file());
    }
}
