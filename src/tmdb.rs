//! Optional TMDB lookup for the Plex tab (off by default).
//!
//! * The user's own key (v4 "API Read Access Token" or v3 "API Key") lives
//!   only in their settings file; it is never logged or shown in full.
//! * Nothing is sent unless lookups are switched on and the user asks for
//!   a lookup. Without a network everything else keeps working.
//! * Attribution (required by TMDB): "This product uses the TMDB API but is
//!   not endorsed or certified by TMDB." - shown in Help > About.
//!
//! The JSON parsing is separate from the HTTP call so it is unit tested
//! offline; the HTTP part (ureq + rustls) is behind the `net` feature.

use serde::Deserialize;

pub const ATTRIBUTION: &str = "This product uses the TMDB API but is not endorsed or certified by TMDB.";
pub const WEBSITE: &str = "https://www.themoviedb.org";
pub const API_SETTINGS_URL: &str = "https://www.themoviedb.org/settings/api";
pub const SIGNUP_URL: &str = "https://www.themoviedb.org/signup";
const API: &str = "https://api.themoviedb.org/3";
const IMAGES: &str = "https://image.tmdb.org/t/p";

/// API base URL. A debug override (environment variable, only for the
/// automated UI test with a local mock server) is accepted for
/// `http://127.0.0.1` / `http://localhost` addresses only.
pub fn api_base() -> String {
    debug_override("ORGANIMIZER_TMDB_API").unwrap_or_else(|| API.to_string())
}

/// Poster image base URL (same local-only debug override).
pub fn image_base() -> String {
    debug_override("ORGANIMIZER_TMDB_IMAGES").unwrap_or_else(|| IMAGES.to_string())
}

fn debug_override(var: &str) -> Option<String> {
    let v = std::env::var(var).ok()?;
    let v = v.trim().trim_end_matches('/').to_string();
    (v.starts_with("http://127.0.0.1:") || v.starts_with("http://localhost:")).then_some(v)
}

/// Poster thumbnail URL (`w92` / `w154`).
pub fn poster_url(path: &str, width: u32) -> String {
    format!("{}/w{width}{}", image_base(), if path.starts_with('/') { path.to_string() } else { format!("/{path}") })
}

/// How the key is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    /// v4 API Read Access Token (a long JWT): `Authorization: Bearer ...`.
    Bearer(String),
    /// v3 API Key (32 hex characters): `?api_key=...`.
    ApiKey(String),
}

impl Auth {
    /// Classify what the user pasted. None for an empty / implausible key.
    pub fn from_key(key: &str) -> Option<Auth> {
        let k: String = key.trim().trim_start_matches("Bearer ").trim().to_string();
        if k.is_empty() || k.chars().any(char::is_whitespace) {
            return None;
        }
        if k.len() > 40 && k.starts_with("eyJ") && k.matches('.').count() == 2 {
            Some(Auth::Bearer(k))
        } else if k.len() == 32 && k.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(Auth::ApiKey(k))
        } else {
            None
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Auth::Bearer(_) => "API Read Access Token (v4)",
            Auth::ApiKey(_) => "API Key (v3)",
        }
    }
}

/// "eyJh…x9Q" - for status text; never the whole key.
pub fn masked(key: &str) -> String {
    let k = key.trim();
    if k.chars().count() <= 8 {
        return "•".repeat(k.chars().count());
    }
    let start: String = k.chars().take(4).collect();
    let end: String = k.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{start}…{end}")
}

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub id: u64,
    pub title: String,
    pub year: Option<u16>,
    pub original_title: String,
    pub overview: String,
    /// `/abc.jpg` (relative to the image base), if TMDB has a poster.
    pub poster_path: Option<String>,
}

#[derive(Deserialize)]
struct SearchPage {
    #[serde(default)]
    results: Vec<RawResult>,
}

#[derive(Deserialize)]
struct RawResult {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    original_name: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
}

fn year_of(date: &Option<String>) -> Option<u16> {
    date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()).filter(|y| *y >= 1870)
}

/// Parse `/search/movie` or `/search/tv` JSON (at most `limit` results).
pub fn parse_search(json: &str, limit: usize) -> Result<Vec<Match>, String> {
    let page: SearchPage = serde_json::from_str(json).map_err(|e| format!("Unexpected answer from TMDB: {e}"))?;
    Ok(page
        .results
        .into_iter()
        .take(limit)
        .map(|r| {
            let title = r.title.or(r.name).unwrap_or_default();
            Match {
                id: r.id,
                year: year_of(&r.release_date).or(year_of(&r.first_air_date)),
                original_title: r.original_title.or(r.original_name).unwrap_or_else(|| title.clone()),
                overview: r.overview.unwrap_or_default().chars().take(300).collect(),
                poster_path: r.poster_path.filter(|p| !p.trim().is_empty()),
                title,
            }
        })
        .filter(|m| !m.title.trim().is_empty())
        .collect())
}

/// Parse `/tv/{id}/season/{s}/episode/{e}` JSON: the episode name.
pub fn parse_episode_name(json: &str) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct Ep {
        #[serde(default)]
        name: Option<String>,
    }
    let ep: Ep = serde_json::from_str(json).map_err(|e| format!("Unexpected answer from TMDB: {e}"))?;
    Ok(ep.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()))
}

/// A TMDB video entry (trailer, featurette, ...): used as a name hint for
/// the user's extras only.
#[derive(Debug, Clone, PartialEq)]
pub struct Video {
    pub name: String,
    /// TMDB type: Trailer, Teaser, Featurette, Behind the Scenes, Clip, Bloopers ...
    pub kind: String,
}

/// `/movie/{id}?append_to_response=videos`.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieDetails {
    pub id: u64,
    pub title: String,
    pub year: Option<u16>,
    /// minutes
    pub runtime: Option<u32>,
    pub videos: Vec<Video>,
}

/// `/tv/{id}`.
#[derive(Debug, Clone, PartialEq)]
pub struct TvDetails {
    pub id: u64,
    pub name: String,
    pub year: Option<u16>,
    /// (season number, episode count)
    pub seasons: Vec<(u32, u32)>,
}

/// One episode of `/tv/{id}/season/{n}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub season: u32,
    pub episode: u32,
    pub name: String,
    /// minutes
    pub runtime: Option<u32>,
}

fn bad(e: serde_json::Error) -> String {
    format!("Unexpected answer from TMDB: {e}")
}

pub fn parse_movie_details(json: &str) -> Result<MovieDetails, String> {
    #[derive(Deserialize)]
    struct V {
        #[serde(default)]
        name: String,
        #[serde(default, rename = "type")]
        kind: String,
        #[serde(default)]
        site: String,
    }
    #[derive(Deserialize, Default)]
    struct Vs {
        #[serde(default)]
        results: Vec<V>,
    }
    #[derive(Deserialize)]
    struct M {
        id: u64,
        #[serde(default)]
        title: String,
        #[serde(default)]
        release_date: Option<String>,
        #[serde(default)]
        runtime: Option<u32>,
        #[serde(default)]
        videos: Option<Vs>,
    }
    let m: M = serde_json::from_str(json).map_err(bad)?;
    let mut videos: Vec<Video> = m
        .videos
        .unwrap_or_default()
        .results
        .into_iter()
        .filter(|v| !v.name.trim().is_empty() && (v.site.is_empty() || v.site == "YouTube" || v.site == "Vimeo"))
        .map(|v| Video { name: v.name.trim().to_string(), kind: v.kind })
        .collect();
    videos.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    Ok(MovieDetails { id: m.id, title: m.title, year: year_of(&m.release_date), runtime: m.runtime.filter(|r| *r > 0), videos })
}

pub fn parse_tv_details(json: &str) -> Result<TvDetails, String> {
    #[derive(Deserialize)]
    struct S {
        season_number: u32,
        #[serde(default)]
        episode_count: u32,
    }
    #[derive(Deserialize)]
    struct T {
        id: u64,
        #[serde(default)]
        name: String,
        #[serde(default)]
        first_air_date: Option<String>,
        #[serde(default)]
        seasons: Vec<S>,
    }
    let t: T = serde_json::from_str(json).map_err(bad)?;
    Ok(TvDetails {
        id: t.id,
        name: t.name,
        year: year_of(&t.first_air_date),
        seasons: t.seasons.into_iter().map(|s| (s.season_number, s.episode_count)).collect(),
    })
}

pub fn parse_season(json: &str) -> Result<Vec<Episode>, String> {
    #[derive(Deserialize)]
    struct E {
        #[serde(default)]
        season_number: u32,
        episode_number: u32,
        #[serde(default)]
        name: String,
        #[serde(default)]
        runtime: Option<u32>,
    }
    #[derive(Deserialize)]
    struct S {
        #[serde(default)]
        season_number: Option<u32>,
        #[serde(default)]
        episodes: Vec<E>,
    }
    let s: S = serde_json::from_str(json).map_err(bad)?;
    let sn = s.season_number;
    let mut eps: Vec<Episode> = s
        .episodes
        .into_iter()
        .map(|e| Episode {
            season: sn.unwrap_or(e.season_number),
            episode: e.episode_number,
            name: e.name.trim().to_string(),
            runtime: e.runtime.filter(|r| *r > 0),
        })
        .collect();
    eps.sort_by_key(|e| e.episode);
    Ok(eps)
}

/// TMDB error JSON (`{"status_message": ...}`) or the HTTP status as text.
pub fn error_text(status: u16, body: &str) -> String {
    #[derive(Deserialize)]
    struct E {
        status_message: Option<String>,
    }
    let msg = serde_json::from_str::<E>(body).ok().and_then(|e| e.status_message);
    match (status, msg) {
        (401, _) => "TMDB rejected the key (401). Check it in Tools > Options > TMDB.".into(),
        (404, _) => "Not found on TMDB (404).".into(),
        (429, _) => "TMDB rate limit reached (429); wait a moment and try again.".into(),
        (_, Some(m)) => format!("TMDB error {status}: {m}"),
        _ => format!("TMDB error {status}"),
    }
}

/// Percent-encode a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build a request URL (the v3 key is added as `api_key`).
pub fn url(path: &str, params: &[(&str, String)], auth: &Auth) -> String {
    let mut q: Vec<String> = params.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    if let Auth::ApiKey(k) = auth {
        q.push(format!("api_key={}", encode(k)));
    }
    format!("{}{path}?{}", api_base(), q.join("&"))
}

/// What to search for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Movie,
    Tv,
}

pub fn search_url(kind: SearchKind, title: &str, year: Option<u16>, language: &str, auth: &Auth) -> String {
    let mut p = vec![("query", title.to_string()), ("include_adult", "false".to_string())];
    if !language.trim().is_empty() {
        p.push(("language", language.trim().to_string()));
    }
    match (kind, year) {
        (SearchKind::Movie, Some(y)) => p.push(("year", y.to_string())),
        (SearchKind::Tv, Some(y)) => p.push(("first_air_date_year", y.to_string())),
        _ => {}
    }
    let path = match kind {
        SearchKind::Movie => "/search/movie",
        SearchKind::Tv => "/search/tv",
    };
    url(path, &p, auth)
}

#[cfg(feature = "net")]
mod net {
    use super::*;
    use std::time::Duration;

    use std::sync::Mutex;
    use std::time::Instant;

    /// At most ~8 requests a second from this app (TMDB allows more), and
    /// one polite retry after a 429.
    const MIN_GAP: Duration = Duration::from_millis(125);
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);

    fn pace() {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = *last {
            let since = t.elapsed();
            if since < MIN_GAP {
                std::thread::sleep(MIN_GAP - since);
            }
        }
        *last = Some(Instant::now());
    }

    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("PrimeCrewOrganimizer/", env!("CARGO_PKG_VERSION")))
            .build()
    }

    fn call(url: &str, auth: Option<&Auth>, accept: &str) -> Result<ureq::Response, String> {
        for attempt in 0..2 {
            pace();
            let mut req = agent().get(url).set("Accept", accept);
            if let Some(Auth::Bearer(t)) = auth {
                req = req.set("Authorization", &format!("Bearer {t}"));
            }
            match req.call() {
                Ok(r) => return Ok(r),
                Err(ureq::Error::Status(429, r)) if attempt == 0 => {
                    let wait = r.header("Retry-After").and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(1).min(5);
                    std::thread::sleep(Duration::from_secs(wait));
                }
                Err(ureq::Error::Status(code, r)) => {
                    return Err(error_text(code, &r.into_string().unwrap_or_default()));
                }
                Err(ureq::Error::Transport(t)) => {
                    return Err(format!("Could not reach TMDB (offline?): {}", t.kind()));
                }
            }
        }
        Err(error_text(429, ""))
    }

    fn get(url: &str, auth: &Auth) -> Result<String, String> {
        call(url, Some(auth), "application/json")?
            .into_string()
            .map_err(|e| format!("Could not read the TMDB answer: {e}"))
    }

    fn lang(language: &str) -> Vec<(&'static str, String)> {
        if language.trim().is_empty() { vec![] } else { vec![("language", language.trim().to_string())] }
    }

    pub fn movie_details(id: u64, language: &str, auth: &Auth) -> Result<MovieDetails, String> {
        let mut p = lang(language);
        p.push(("append_to_response", "videos".into()));
        parse_movie_details(&get(&url(&format!("/movie/{id}"), &p, auth), auth)?)
    }

    pub fn tv_details(id: u64, language: &str, auth: &Auth) -> Result<TvDetails, String> {
        parse_tv_details(&get(&url(&format!("/tv/{id}"), &lang(language), auth), auth)?)
    }

    pub fn season(id: u64, season: u32, language: &str, auth: &Auth) -> Result<Vec<Episode>, String> {
        parse_season(&get(&url(&format!("/tv/{id}/season/{season}"), &lang(language), auth), auth)?)
    }

    /// Poster bytes (JPEG), at most 2 MB. Images need no key.
    pub fn image(path: &str, width: u32) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let r = call(&poster_url(path, width), None, "image/*")?;
        let mut buf = vec![];
        r.into_reader().take(2 << 20).read_to_end(&mut buf).map_err(|e| format!("Could not read the poster: {e}"))?;
        Ok(buf)
    }

    pub fn search(
        kind: SearchKind,
        title: &str,
        year: Option<u16>,
        language: &str,
        auth: &Auth,
    ) -> Result<Vec<Match>, String> {
        let mut found = parse_search(&get(&search_url(kind, title, year, language, auth), auth)?, 8)?;
        if found.is_empty() && year.is_some() {
            // the year in the file name may be off by one: try without it
            found = parse_search(&get(&search_url(kind, title, None, language, auth), auth)?, 8)?;
        }
        Ok(found)
    }

    pub fn episode_name(
        tv_id: u64,
        season: u32,
        episode: u32,
        language: &str,
        auth: &Auth,
    ) -> Result<Option<String>, String> {
        let mut p = vec![];
        if !language.trim().is_empty() {
            p.push(("language", language.trim().to_string()));
        }
        parse_episode_name(&get(&url(&format!("/tv/{tv_id}/season/{season}/episode/{episode}"), &p, auth), auth)?)
    }
}

#[cfg(feature = "net")]
pub use net::{episode_name, image, movie_details, search, season, tv_details};

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up token shape (not a real key), built at run time.
    fn token() -> String {
        ["eyJhbGciOiJub25lIn0", "eyJ0ZXN0Ijp0cnVlfQ", "ZmFrZS1zaWduYXR1cmU"].join(".")
    }

    #[test]
    fn key_kinds_and_masking() {
        let t = token();
        assert_eq!(Auth::from_key(&format!("  {t} ")), Some(Auth::Bearer(t.clone())));
        assert_eq!(Auth::from_key(&format!("Bearer {t}")), Some(Auth::Bearer(t.clone())));
        let v3 = "0123456789abcdef0123456789ABCDEF";
        assert_eq!(Auth::from_key(v3), Some(Auth::ApiKey(v3.into())));
        assert_eq!(Auth::from_key(""), None);
        assert_eq!(Auth::from_key("not a key"), None);
        assert_eq!(Auth::from_key("12345"), None);
        assert_eq!(masked(v3), "0123…DEF");
        assert!(!masked(&t).contains("eyJ0ZXN0"));
    }

    #[test]
    fn poster_urls_and_local_only_override() {
        assert_eq!(poster_url("/a.jpg", 92), format!("{}/w92/a.jpg", image_base()));
        // the debug override only ever points at this PC
        assert!(debug_override("ORGANIMIZER_TMDB_TEST_UNSET_VAR").is_none());
    }

    #[test]
    fn urls() {
        let v3 = Auth::ApiKey("0123456789abcdef0123456789abcdef".into());
        let u = search_url(SearchKind::Movie, "Amélie & Co", Some(2001), "en-US", &v3);
        assert_eq!(
            u,
            "https://api.themoviedb.org/3/search/movie?query=Am%C3%A9lie%20%26%20Co&include_adult=false&language=en-US&year=2001&api_key=0123456789abcdef0123456789abcdef"
        );
        let t = token();
        let b = Auth::Bearer(t.clone());
        let u = search_url(SearchKind::Tv, "Severance", Some(2022), "", &b);
        assert_eq!(
            u,
            "https://api.themoviedb.org/3/search/tv?query=Severance&include_adult=false&first_air_date_year=2022"
        );
        assert!(!u.contains(&t), "a v4 token goes in the header, never the URL");
    }

    #[test]
    fn parse_results() {
        let movie = r#"{"page":1,"results":[{"id":603,"title":"The Matrix","original_title":"The Matrix","release_date":"1999-03-31","overview":"Neo..."},{"id":604,"title":"The Matrix Reloaded","release_date":""}],"total_results":2}"#;
        let m = parse_search(movie, 8).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id, m[0].title.as_str(), m[0].year), (603, "The Matrix", Some(1999)));
        assert_eq!(m[1].year, None);
        let tv = r#"{"results":[{"id":95396,"name":"Severance","original_name":"Severance","first_air_date":"2022-02-17"}]}"#;
        let t = parse_search(tv, 8).unwrap();
        assert_eq!((t[0].title.as_str(), t[0].year), ("Severance", Some(2022)));
        assert!(parse_search("not json", 8).is_err());
        assert_eq!(parse_search(r#"{"results":[]}"#, 8).unwrap(), vec![]);
        assert_eq!(
            parse_episode_name(r#"{"name":"In Perpetuity","episode_number":3}"#).unwrap(),
            Some("In Perpetuity".into())
        );
        assert_eq!(parse_episode_name(r#"{"name":""}"#).unwrap(), None);
        assert!(error_text(401, "").contains("rejected the key"));
        assert_eq!(error_text(500, r#"{"status_message":"Oops"}"#), "TMDB error 500: Oops");
    }
}
