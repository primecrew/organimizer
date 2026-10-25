//! "Smart" date detection from file names.
//!
//! Handles, among others:
//! * `20240315`, `2024-03-15`, `2024_03_15`, `2024.03.15`, `2024 03 15`
//! * `IMG_20240315_142530`, `PXL_20240315_142530123`, `IMG-20240315-WA0001`
//! * `Screenshot 2024-03-15 at 14.25.30`, `Screenshot_20240315-142530`
//! * `Photo 2024-03-15`, `2024-03-15_14-25-30`
//! * `15-03-2024`, `03-15-2024` (day/month order resolved by range checks,
//!   and when truly ambiguous by the [`DateOrder`] preference)
//! * Unix timestamps: 10 digits (seconds) or 13 digits (milliseconds),
//!   accepted only between 2000-01-01 and now + 1 year.
//!
//! Every candidate is validated as a real calendar date with a year in
//! `MIN_YEAR..=max_year()` so random digit runs are not mistaken for dates.

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeZone};

/// How to read ambiguous `NN-NN-YYYY` dates where both numbers are <= 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateOrder {
    /// Month first (US): `03-04-2024` = March 4, 2024.
    #[default]
    Us,
    /// Day first: `03-04-2024` = 3 April 2024.
    Intl,
}

/// Result of a successful parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoundDate {
    pub date: NaiveDate,
    /// Present for unix timestamps / names that carry a time of day.
    pub datetime: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Digits(String),
    Sep(char),
    Other,
}

fn tokenize(s: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut digits = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            toks.push(Tok::Digits(std::mem::take(&mut digits)));
        }
        match c {
            '-' | '_' | '.' | ' ' => toks.push(Tok::Sep(c)),
            _ => toks.push(Tok::Other),
        }
    }
    if !digits.is_empty() {
        toks.push(Tok::Digits(digits));
    }
    toks
}

fn valid_ymd(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    if y < crate::MIN_YEAR || y > crate::max_year() {
        return None;
    }
    NaiveDate::from_ymd_opt(y, m, d)
}

fn num(s: &str) -> Option<u32> {
    s.parse().ok()
}

/// Resolve `a-b-YYYY` where a/b are day and month in unknown order.
fn resolve_ambiguous(a: u32, b: u32, y: i32, order: DateOrder) -> Option<NaiveDate> {
    match (a > 12, b > 12) {
        (true, true) => None,
        (true, false) => valid_ymd(y, b, a), // a must be the day
        (false, true) => valid_ymd(y, a, b), // b must be the day
        (false, false) => match order {
            DateOrder::Us => valid_ymd(y, a, b),
            DateOrder::Intl => valid_ymd(y, b, a),
        },
    }
}

/// Try to read a time `HH MM SS` from up to three following digit groups
/// (or one 6-digit group), for display only.
fn time_from_digits(s: &str) -> Option<chrono::NaiveTime> {
    if s.len() >= 6 {
        let h = num(&s[0..2])?;
        let m = num(&s[2..4])?;
        let sec = num(&s[4..6])?;
        return chrono::NaiveTime::from_hms_opt(h, m, sec);
    }
    None
}

fn unix_to_local(secs: i64) -> Option<NaiveDateTime> {
    let min = NaiveDate::from_ymd_opt(2000, 1, 1)?.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
    let max = (chrono::Utc::now() + chrono::Duration::days(366)).timestamp();
    if secs < min || secs > max {
        return None;
    }
    let dt = chrono::Local.timestamp_opt(secs, 0).single()?;
    Some(dt.naive_local())
}

/// Interpret one contiguous run of digits.
fn from_digit_run(run: &str, next_run: Option<&str>, order: DateOrder) -> Option<FoundDate> {
    let len = run.len();
    // YYYYMMDD[HHMMSS[mmm]] – preferred interpretation.
    if len >= 8
        && let (Some(y), Some(m), Some(d)) = (run[0..4].parse::<i32>().ok(), num(&run[4..6]), num(&run[6..8]))
        && let Some(date) = valid_ymd(y, m, d)
    {
        let time = if len >= 14 { time_from_digits(&run[8..]) } else { next_run.and_then(time_from_digits) };
        return Some(FoundDate { date, datetime: time.map(|t| date.and_time(t)) });
    }
    // Unix timestamps (seconds / milliseconds).
    if (len == 10 || len == 13)
        && let Ok(v) = run.parse::<i64>()
    {
        let secs = if len == 13 { v / 1000 } else { v };
        if let Some(dt) = unix_to_local(secs) {
            return Some(FoundDate { date: dt.date(), datetime: Some(dt) });
        }
    }
    // DDMMYYYY / MMDDYYYY (only exactly 8 digits, after YYYYMMDD failed).
    if len == 8
        && let (Some(a), Some(b), Ok(y)) = (num(&run[0..2]), num(&run[2..4]), run[4..8].parse::<i32>())
        && let Some(date) = resolve_ambiguous(a, b, y, order)
    {
        return Some(FoundDate { date, datetime: None });
    }
    None
}

/// Detect a date in a file name (the extension is ignored).
pub fn parse_filename_date(file_name: &str, order: DateOrder) -> Option<FoundDate> {
    let stem = match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    };
    let toks = tokenize(stem);
    let digit_at = |i: usize| -> Option<&str> {
        match toks.get(i) {
            Some(Tok::Digits(s)) => Some(s.as_str()),
            _ => None,
        }
    };
    let sep_at = |i: usize| -> Option<char> {
        match toks.get(i) {
            Some(Tok::Sep(c)) => Some(*c),
            _ => None,
        }
    };

    for i in 0..toks.len() {
        let Some(a) = digit_at(i) else { continue };

        // Separated triple: A sep B sep C (same separator both times).
        if let (Some(s1), Some(b), Some(s2), Some(c)) = (sep_at(i + 1), digit_at(i + 2), sep_at(i + 3), digit_at(i + 4))
            && s1 == s2
        {
            let (la, lb, lc) = (a.len(), b.len(), c.len());
            // YYYY-MM-DD
            if la == 4
                && (1..=2).contains(&lb)
                && (1..=2).contains(&lc)
                && let (Ok(y), Some(m), Some(d)) = (a.parse::<i32>(), num(b), num(c))
                && let Some(date) = valid_ymd(y, m, d)
            {
                let time = time_after(&toks, i + 5);
                return Some(FoundDate { date, datetime: time.map(|t| date.and_time(t)) });
            }
            // DD-MM-YYYY / MM-DD-YYYY
            if (1..=2).contains(&la)
                && (1..=2).contains(&lb)
                && lc == 4
                && let (Some(x), Some(y2), Ok(y)) = (num(a), num(b), c.parse::<i32>())
                && let Some(date) = resolve_ambiguous(x, y2, y, order)
            {
                return Some(FoundDate { date, datetime: None });
            }
        }

        // Contiguous digit run.
        let next_run = match (sep_at(i + 1), digit_at(i + 2)) {
            (Some(_), Some(n)) => Some(n),
            _ => None,
        };
        if let Some(found) = from_digit_run(a, next_run, order) {
            return Some(found);
        }
    }
    None
}

/// Look for `HH sep MM sep SS` or `HHMMSS` starting after a separator at `i`.
fn time_after(toks: &[Tok], i: usize) -> Option<chrono::NaiveTime> {
    let mut j = i;
    // skip separators and short words like "at"
    let mut skipped = 0;
    while j < toks.len() && skipped < 4 {
        match &toks[j] {
            Tok::Digits(_) => break,
            _ => {
                j += 1;
                skipped += 1;
            }
        }
    }
    let get = |k: usize| match toks.get(k) {
        Some(Tok::Digits(s)) => Some(s.as_str()),
        _ => None,
    };
    let h = get(j)?;
    if h.len() >= 6 {
        return time_from_digits(h);
    }
    let m = get(j + 2)?;
    let s = get(j + 4)?;
    if h.len() == 2 && m.len() == 2 && s.len() == 2 {
        return chrono::NaiveTime::from_hms_opt(num(h)?, num(m)?, num(s)?);
    }
    None
}

/// Convenience: year/month/day tuple, for tests and display.
pub fn ymd(f: &FoundDate) -> (i32, u32, u32) {
    (f.date.year(), f.date.month(), f.date.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> Option<(i32, u32, u32)> {
        parse_filename_date(name, DateOrder::Us).map(|f| ymd(&f))
    }
    fn pi(name: &str) -> Option<(i32, u32, u32)> {
        parse_filename_date(name, DateOrder::Intl).map(|f| ymd(&f))
    }

    #[test]
    fn yyyymmdd_variants() {
        assert_eq!(p("20240315.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("2024-03-15.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("2024_03_15.png"), Some((2024, 3, 15)));
        assert_eq!(p("2024.03.15.png"), Some((2024, 3, 15)));
        assert_eq!(p("2024 03 15.png"), Some((2024, 3, 15)));
        assert_eq!(p("2024-3-5 party.jpg"), Some((2024, 3, 5)));
        assert_eq!(p("2024-03-15_14-25-30.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("20240315142530.jpg"), Some((2024, 3, 15)));
    }

    #[test]
    fn camera_and_phone_names() {
        assert_eq!(p("IMG_20240315_142530.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("IMG_20240315_142530_HDR.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("PXL_20240315_142530123.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("PXL_20240315_142530123.MP.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("IMG-20240315-WA0001.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("VID-20231231-WA0042.jpg"), Some((2023, 12, 31)));
        assert_eq!(p("Screenshot 2024-03-15 at 14.25.30.png"), Some((2024, 3, 15)));
        assert_eq!(p("Screenshot_20240315-142530.png"), Some((2024, 3, 15)));
        assert_eq!(p("Screenshot_20240315-142530_Chrome.png"), Some((2024, 3, 15)));
        assert_eq!(p("Screenshot 2024-03-15 142530.png"), Some((2024, 3, 15)));
        assert_eq!(p("Photo 2024-03-15.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("signal-2024-03-15-142530.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("MVIMG_20240315_142530.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("Snapchat-20240315.jpg"), Some((2024, 3, 15)));
    }

    #[test]
    fn time_is_captured_when_present() {
        let f = parse_filename_date("IMG_20240315_142530.jpg", DateOrder::Us).unwrap();
        assert_eq!(f.datetime.unwrap().format("%H:%M:%S").to_string(), "14:25:30");
        let f = parse_filename_date("Screenshot 2024-03-15 at 14.25.30.png", DateOrder::Us).unwrap();
        assert_eq!(f.datetime.unwrap().format("%H:%M:%S").to_string(), "14:25:30");
    }

    #[test]
    fn day_month_order() {
        // unambiguous: 15 can only be a day
        assert_eq!(p("15-03-2024.jpg"), Some((2024, 3, 15)));
        assert_eq!(pi("15-03-2024.jpg"), Some((2024, 3, 15)));
        assert_eq!(p("03-15-2024.jpg"), Some((2024, 3, 15)));
        assert_eq!(pi("03-15-2024.jpg"), Some((2024, 3, 15)));
        // ambiguous: depends on preference
        assert_eq!(p("03-04-2024.jpg"), Some((2024, 3, 4)));
        assert_eq!(pi("03-04-2024.jpg"), Some((2024, 4, 3)));
        assert_eq!(p("3.4.2024.jpg"), Some((2024, 3, 4)));
        assert_eq!(pi("3.4.2024.jpg"), Some((2024, 4, 3)));
        // contiguous DDMMYYYY
        assert_eq!(pi("15032024.jpg"), Some((2024, 3, 15)));
        // impossible
        assert_eq!(p("31-31-2024.jpg"), None);
        assert_eq!(p("02-30-2024.jpg"), None);
    }

    #[test]
    fn unix_timestamps() {
        let s = parse_filename_date("1710512345.jpg", DateOrder::Us).unwrap();
        assert_eq!(s.date.year(), 2024);
        assert_eq!(s.date.month(), 3);
        let ms = parse_filename_date("image_1710512345678.png", DateOrder::Us).unwrap();
        assert_eq!(ms.date.year(), 2024);
        // out of range (1990s / far future)
        assert_eq!(p("0912345678.jpg"), None);
        assert_eq!(p("9999999999.jpg"), None);
    }

    #[test]
    fn rejects_false_positives() {
        assert_eq!(p("IMG_1234.JPG"), None);
        assert_eq!(p("DSC01234.jpg"), None);
        assert_eq!(p("DSCN0042.jpg"), None);
        assert_eq!(p("holiday.png"), None);
        assert_eq!(p("12345678.jpg"), None);
        assert_eq!(p("123456789012.jpg"), None);
        assert_eq!(p("20241345.jpg"), None); // month 13
        assert_eq!(p("20240230.jpg"), None); // Feb 30
        assert_eq!(p("18990101.jpg"), None); // too old
        assert_eq!(p("30000101.jpg"), None); // too new
        assert_eq!(p("1985-06-01.jpg"), None); // before MIN_YEAR
        assert_eq!(p("v1.2.3.png"), None);
        assert_eq!(p("800x600.png"), None);
        assert_eq!(p("2024.png"), None);
        assert_eq!(p(""), None);
    }

    #[test]
    fn leap_day_ok() {
        assert_eq!(p("2024-02-29.jpg"), Some((2024, 2, 29)));
        assert_eq!(p("2023-02-29.jpg"), None);
    }

    #[test]
    fn extension_is_ignored() {
        // the extension itself must not contribute digits
        assert_eq!(p("photo.20240315"), None);
        assert_eq!(p("20240315"), Some((2024, 3, 15)));
    }
}
