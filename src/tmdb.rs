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
    format!("{API}{path}?{}", q.join("&"))
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

    fn get(url: &str, auth: &Auth) -> Result<String, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("PrimeCrewOrganimizer/", env!("CARGO_PKG_VERSION")))
            .build();
        let mut req = agent.get(url).set("Accept", "application/json");
        if let Auth::Bearer(t) = auth {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        match req.call() {
            Ok(r) => r.into_string().map_err(|e| format!("Could not read the TMDB answer: {e}")),
            Err(ureq::Error::Status(code, r)) => Err(error_text(code, &r.into_string().unwrap_or_default())),
            Err(ureq::Error::Transport(t)) => Err(format!("Could not reach TMDB (offline?): {t}")),
        }
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
pub use net::{episode_name, search};

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
