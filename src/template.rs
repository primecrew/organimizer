//! Folder-name template formatting, e.g. `"{MMMM} {YYYY}"` -> `"October 2026"`.
//!
//! Supported tokens (case-sensitive):
//! * `{MMMM}` full English month name (`October`)
//! * `{MMM}`  short English month name (`Oct`)
//! * `{MM}`   two-digit month (`10`)
//! * `{M}`    month without padding (`9`)
//! * `{YYYY}` four-digit year (`2026`)
//! * `{YY}`   two-digit year (`26`)
//! * `{DD}`   two-digit day (`05`)
//! * `{/}`    subfolder separator (since 2.0.3): `{YYYY}{/}{MMMM}` sorts
//!   into `2024\March`. Every part stays a plain, sanitised folder name
//!   inside the file's own folder (no `..`, no drive or absolute paths).
//!
//! Unknown `{...}` tokens are kept literally. Characters that are illegal in
//! Windows file names (including a typed `/` or `\`) are replaced with `_`.
//!
//! [`FolderBuilder`] maps the simple "Build your own" controls of the GUI to
//! a template string and back; [`check_template`] lists inline warnings.

use chrono::{Datelike, NaiveDate};

pub const DEFAULT_TEMPLATE: &str = "{MMMM} {YYYY}";

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Subfolder separator token.
pub const SUBFOLDER: &str = "{/}";

/// Expand the date tokens of one template part (no sanitising).
fn expand(template: &str, date: NaiveDate) -> String {
    let month = MONTHS[date.month0() as usize];
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('}') {
            Some(end) => {
                let token = &after[1..end];
                let rep = match token {
                    "MMMM" => Some(month.to_string()),
                    "MMM" => Some(month[..3].to_string()),
                    "MM" => Some(format!("{:02}", date.month())),
                    "M" => Some(date.month().to_string()),
                    "YYYY" => Some(format!("{:04}", date.year())),
                    "YY" => Some(format!("{:02}", date.year() % 100)),
                    "DD" => Some(format!("{:02}", date.day())),
                    _ => None,
                };
                match rep {
                    Some(r) => out.push_str(&r),
                    None => out.push_str(&after[..=end]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Windows device names that cannot be used as folder names.
fn is_reserved_name(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or("").trim().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

/// The destination folder(s) for `date`: one sanitised name per level, e.g.
/// `["March 2024"]` or, with `{/}`, `["2024", "March"]`. Never empty, never
/// contains `.`/`..`, path separators, drive letters or device names.
pub fn format_folder_parts(template: &str, date: NaiveDate) -> Vec<String> {
    let template = if template.trim().is_empty() { DEFAULT_TEMPLATE } else { template };
    let mut parts: Vec<String> = template
        .split(SUBFOLDER)
        .map(|p| expand(p, date))
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let c = sanitize_component(&p);
            // "." / ".." become "_" in sanitize_component (dots are trimmed)
            if is_reserved_name(&c) { format!("{c}_") } else { c }
        })
        .collect();
    if parts.is_empty() {
        parts.push(sanitize_component(&expand(DEFAULT_TEMPLATE, date)));
    }
    parts
}

/// Number of folder levels the template creates (1, or more with `{/}`).
pub fn template_depth(template: &str) -> usize {
    format_folder_parts(template, NaiveDate::from_ymd_opt(2024, 3, 5).unwrap()).len()
}

/// Format a folder name for `date` using `template`. With `{/}` the levels
/// are joined with the platform separator (for display; see
/// [`format_folder_parts`] for the real path components).
pub fn format_folder_name(template: &str, date: NaiveDate) -> String {
    format_folder_parts(template, date).join(std::path::MAIN_SEPARATOR_STR)
}

/// Make a string safe to use as a single Windows path component.
pub fn sanitize_component(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows does not allow trailing dots/spaces in names.
    let trimmed = cleaned.trim().trim_end_matches('.').trim_end().to_string();
    if trimmed.is_empty() { "_".to_string() } else { trimmed }
}

/// Does the template depend on the day? (Then "already sorted" detection
/// needs the exact date, not just month/year.)
fn template_uses_day(template: &str) -> bool {
    template.contains("{DD}")
}

/// Returns true if `folder_name` is exactly what `template` would produce for
/// *some* plausible date (used to recognise folders that Organimizer (or PhotoSorter), or the
/// user, already created). Case-insensitive, as Windows is.
pub fn is_sorted_folder_name(template: &str, folder_name: &str) -> bool {
    let target = folder_name.to_lowercase();
    let uses_day = template_uses_day(template);
    for year in crate::MIN_YEAR..=crate::max_year() {
        for month in 1..=12 {
            let days: &[u32] = if uses_day {
                &[
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
                    28, 29, 30, 31,
                ]
            } else {
                &[1]
            };
            for &d in days {
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, d)
                    && format_folder_name(template, date).to_lowercase() == target
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Like [`is_sorted_folder_name`] for the last `parts.len()` folder levels
/// (nested `{/}` templates): true if they are exactly what `template`
/// produces for some plausible date. Case-insensitive.
pub fn is_sorted_folder_path(template: &str, parts: &[String]) -> bool {
    !parts.is_empty() && parts.len() == template_depth(template) && matches_levels(template, parts)
}

/// True if `parts` equal the first `parts.len()` levels the template
/// produces for some plausible date (case-insensitive).
fn matches_levels(template: &str, parts: &[String]) -> bool {
    let target: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
    let uses_day = template_uses_day(template);
    for year in crate::MIN_YEAR..=crate::max_year() {
        for month in 1..=12 {
            let last = if uses_day { 31 } else { 1 };
            for d in 1..=last {
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, d) {
                    let got = format_folder_parts(template, date);
                    if got.len() >= target.len() && got.iter().zip(&target).all(|(a, b)| a.to_lowercase() == *b) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// How many trailing levels of `dir_levels` (folders below the chosen root,
/// outermost first) are already sorted folders of `template`: the full
/// depth (e.g. `2024\March`), or with a nested template also a leading part
/// of it (e.g. just `2024`, whose month level is still missing). 0 = none.
/// Files in such folders are sorted relative to the folder above them.
pub fn sorted_levels(template: &str, dir_levels: &[String]) -> usize {
    let depth = template_depth(template);
    (1..=depth.min(dir_levels.len()))
        .rev()
        .find(|&k| matches_levels(template, &dir_levels[dir_levels.len() - k..]))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Inline validation
// ---------------------------------------------------------------------------

/// Tokens the engine understands, for the legend.
pub const TOKEN_LEGEND: &str =
    "{YYYY} 2024 · {YY} 24 · {MMMM} March · {MMM} Mar · {MM} 03 · {M} 3 · {DD} 05 · {/} subfolder";

const KNOWN_TOKENS: [&str; 8] = ["YYYY", "YY", "MMMM", "MMM", "MM", "M", "DD", "/"];

/// Warnings about a template, for showing next to the text field. Nothing
/// here blocks a run: illegal characters are replaced with `_` anyway.
pub fn check_template(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    if template.trim().is_empty() {
        out.push(format!("Empty — the default {DEFAULT_TEMPLATE} is used."));
        return out;
    }
    // walk tokens / literals
    let mut tokens: Vec<&str> = Vec::new();
    let mut literal = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        literal.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('}') {
            Some(end) => {
                let tok = &after[1..end];
                if KNOWN_TOKENS.contains(&tok) {
                    tokens.push(tok);
                } else if tok.contains('{') {
                    out.push(format!("“{{{tok}}}”: a “{{” without a closing “}}” — kept as text."));
                    literal.push_str(&after[..=end]);
                } else {
                    out.push(format!("{{{tok}}} is not a token and is kept as text."));
                    literal.push_str(&after[..=end]);
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push("“{” without a closing “}” is kept as text.".to_string());
                literal.push_str(after);
                rest = "";
            }
        }
    }
    literal.push_str(rest);
    let mut bad: Vec<char> = Vec::new();
    for c in literal.chars() {
        if (matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()) && !bad.contains(&c) {
            bad.push(c);
        }
    }
    if !bad.is_empty() {
        let list: Vec<String> = bad
            .iter()
            .map(|c| if c.is_control() { "control character".to_string() } else { format!("“{c}”") })
            .collect();
        let mut msg = format!("{} not allowed in Windows folder names — replaced with “_”.", list.join(" "));
        if bad.iter().any(|c| matches!(c, '/' | '\\')) {
            msg.push_str(" Use {/} for a subfolder.");
        }
        out.push(msg);
    }
    let date_tokens = tokens.iter().filter(|t| **t != "/").count();
    if date_tokens == 0 {
        out.push("No date token — every file would go into the same folder.".to_string());
    } else if !tokens.iter().any(|t| *t == "YYYY" || *t == "YY") {
        out.push("No year — e.g. March 2023 and March 2024 would share one folder.".to_string());
    }
    let sample = NaiveDate::from_ymd_opt(2024, 3, 5).unwrap();
    for part in template.split(SUBFOLDER) {
        let e = expand(part, sample);
        if !e.trim().is_empty() && (e.ends_with('.') || e.ends_with(' ') || e.starts_with(' ')) {
            out.push("Leading/trailing spaces and trailing dots are removed (Windows rule).".to_string());
            break;
        }
    }
    if format_folder_parts(template, sample)
        .iter()
        .any(|p| p.ends_with('_') && is_reserved_name(p.trim_end_matches('_')))
    {
        out.push("A Windows device name (CON, NUL, COM1 …) gets a “_” appended.".to_string());
    }
    if template.split(SUBFOLDER).filter(|p| !p.trim().is_empty()).count() > 4 {
        out.push("More than 4 folder levels — is that intended?".to_string());
    }
    out
}

// ---------------------------------------------------------------------------
// Folder name builder (GUI "Build your own")
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YearStyle {
    /// 2024
    Full,
    /// 24
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonthStyle {
    /// March
    Name,
    /// Mar
    Short,
    /// 03
    Num2,
    /// 3
    Num,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartOrder {
    /// 2024-03(-05)
    YearFirst,
    /// March (05) 2024
    MonthFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Separator {
    Space,
    Dash,
    Underscore,
    Dot,
    None,
    /// `{/}`: one folder level per part.
    Subfolder,
    /// Short user text (1–5 characters, no illegal characters or braces).
    Custom(String),
}

impl YearStyle {
    pub const ALL: [YearStyle; 2] = [YearStyle::Full, YearStyle::Short];
    pub fn token(self) -> &'static str {
        match self {
            YearStyle::Full => "{YYYY}",
            YearStyle::Short => "{YY}",
        }
    }
}

impl MonthStyle {
    pub const ALL: [MonthStyle; 4] = [MonthStyle::Name, MonthStyle::Short, MonthStyle::Num2, MonthStyle::Num];
    pub fn token(self) -> &'static str {
        match self {
            MonthStyle::Name => "{MMMM}",
            MonthStyle::Short => "{MMM}",
            MonthStyle::Num2 => "{MM}",
            MonthStyle::Num => "{M}",
        }
    }
}

impl Separator {
    /// Choices for the GUI (Custom is edited separately).
    pub const FIXED: [Separator; 6] = [
        Separator::Space,
        Separator::Dash,
        Separator::Underscore,
        Separator::Dot,
        Separator::None,
        Separator::Subfolder,
    ];

    pub fn text(&self) -> &str {
        match self {
            Separator::Space => " ",
            Separator::Dash => "-",
            Separator::Underscore => "_",
            Separator::Dot => ".",
            Separator::None => "",
            Separator::Subfolder => SUBFOLDER,
            Separator::Custom(s) => s,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Separator::Space => "Space".into(),
            Separator::Dash => "Dash  -".into(),
            Separator::Underscore => "Underscore  _".into(),
            Separator::Dot => "Dot  .".into(),
            Separator::None => "None".into(),
            Separator::Subfolder => "Subfolder  \\".into(),
            Separator::Custom(_) => "Custom…".into(),
        }
    }

    pub fn from_text(s: &str) -> Separator {
        match s {
            " " => Separator::Space,
            "-" => Separator::Dash,
            "_" => Separator::Underscore,
            "." => Separator::Dot,
            "" => Separator::None,
            SUBFOLDER => Separator::Subfolder,
            other => Separator::Custom(other.to_string()),
        }
    }
}

/// Is `s` usable as a custom separator? (1–5 characters, nothing Windows
/// rejects, no braces so it cannot form a token.)
pub fn valid_custom_separator(s: &str) -> bool {
    let n = s.chars().count();
    (1..=5).contains(&n)
        && !s
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '{' | '}') || c.is_control())
}

/// The "Build your own" controls. Day always follows the month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderBuilder {
    pub year: YearStyle,
    pub month: MonthStyle,
    pub day: bool,
    pub order: PartOrder,
    pub sep: Separator,
}

impl Default for FolderBuilder {
    /// Same as [`DEFAULT_TEMPLATE`]: `{MMMM} {YYYY}`.
    fn default() -> Self {
        FolderBuilder {
            year: YearStyle::Full,
            month: MonthStyle::Name,
            day: false,
            order: PartOrder::MonthFirst,
            sep: Separator::Space,
        }
    }
}

impl FolderBuilder {
    pub fn to_template(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(3);
        match self.order {
            PartOrder::YearFirst => {
                parts.push(self.year.token());
                parts.push(self.month.token());
                if self.day {
                    parts.push("{DD}");
                }
            }
            PartOrder::MonthFirst => {
                parts.push(self.month.token());
                if self.day {
                    parts.push("{DD}");
                }
                parts.push(self.year.token());
            }
        }
        parts.join(self.sep.text())
    }

    /// The builder state that produces exactly `template`, if any
    /// (otherwise the GUI shows "Custom").
    pub fn from_template(template: &str) -> Option<FolderBuilder> {
        // split into tokens and the literal text between them
        let mut toks: Vec<String> = Vec::new();
        let mut seps: Vec<String> = Vec::new();
        let mut lit = String::new();
        let mut rest = template;
        let mut expect_token = true;
        while !rest.is_empty() {
            let tok =
                ["{YYYY}", "{YY}", "{MMMM}", "{MMM}", "{MM}", "{M}", "{DD}"].into_iter().find(|t| rest.starts_with(*t));
            match tok {
                Some(t) => {
                    if !expect_token {
                        seps.push(std::mem::take(&mut lit));
                    }
                    toks.push(t.to_string());
                    rest = &rest[t.len()..];
                    expect_token = false;
                }
                None => {
                    if toks.is_empty() {
                        return None; // leading literal
                    }
                    if rest.starts_with(SUBFOLDER) {
                        lit.push_str(SUBFOLDER);
                        rest = &rest[SUBFOLDER.len()..];
                    } else {
                        let c = rest.chars().next()?;
                        lit.push(c);
                        rest = &rest[c.len_utf8()..];
                    }
                }
            }
        }
        if !lit.is_empty() {
            return None; // trailing literal
        }
        if !(2..=3).contains(&toks.len()) || seps.len() + 1 != toks.len() || seps.iter().any(|s| *s != seps[0]) {
            return None;
        }
        let sep = Separator::from_text(&seps[0]);
        if let Separator::Custom(s) = &sep
            && !valid_custom_separator(s)
        {
            return None;
        }
        let year = |t: &str| match t {
            "{YYYY}" => Some(YearStyle::Full),
            "{YY}" => Some(YearStyle::Short),
            _ => None,
        };
        let month = |t: &str| match t {
            "{MMMM}" => Some(MonthStyle::Name),
            "{MMM}" => Some(MonthStyle::Short),
            "{MM}" => Some(MonthStyle::Num2),
            "{M}" => Some(MonthStyle::Num),
            _ => None,
        };
        let t: Vec<&str> = toks.iter().map(String::as_str).collect();
        let b = if let Some(y) = year(t[0]) {
            let m = month(t[1])?;
            let day = match t.get(2) {
                Some(&"{DD}") => true,
                Some(_) => return None,
                None => false,
            };
            FolderBuilder { year: y, month: m, day, order: PartOrder::YearFirst, sep }
        } else {
            let m = month(t[0])?;
            let (day, y) = if t.len() == 3 {
                if t[1] != "{DD}" {
                    return None;
                }
                (true, year(t[2])?)
            } else {
                (false, year(t[1])?)
            };
            FolderBuilder { year: y, month: m, day, order: PartOrder::MonthFirst, sep }
        };
        (b.to_template() == template).then_some(b)
    }
}

/// Ready-made layouts for the "Folder name" dropdown: (template, note).
/// The first one is the default.
pub const LAYOUTS: [(&str, &str); 9] = [
    ("{MMMM} {YYYY}", "default"),
    ("{MMM} {YYYY}", "short month"),
    ("{YYYY}-{MM}", "sorts by date"),
    ("{YYYY}_{MM}", "sorts by date"),
    ("{YYYY}-{MM} {MMMM}", "sorts by date, with name"),
    ("{YYYY}{/}{MMMM}", "year folder, month inside"),
    ("{YYYY}{/}{MM} {MMMM}", "year folder, sortable months"),
    ("{YYYY}-{MM}-{DD}", "one folder per day"),
    ("{MMMM} {DD} {YYYY}", "one folder per day"),
];

/// Fixed sample date used for the examples in the GUI.
pub fn sample_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 3, 5).unwrap()
}

/// "March 2024" / "2024 \ March" — the sample folder name for display.
pub fn example(template: &str) -> String {
    format_folder_parts(template, sample_date()).join(" \\ ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn default_template() {
        assert_eq!(format_folder_name(DEFAULT_TEMPLATE, d(2026, 10, 25)), "October 2026");
        assert_eq!(format_folder_name(DEFAULT_TEMPLATE, d(2024, 1, 1)), "January 2024");
        assert_eq!(format_folder_name(DEFAULT_TEMPLATE, d(1999, 12, 31)), "December 1999");
        assert_eq!(format_folder_name("", d(2024, 2, 29)), "February 2024");
    }

    #[test]
    fn all_months_english() {
        for (i, name) in MONTHS.iter().enumerate() {
            assert_eq!(format_folder_name("{MMMM}", d(2020, i as u32 + 1, 1)), *name);
        }
    }

    #[test]
    fn other_tokens() {
        assert_eq!(format_folder_name("{YYYY}-{MM} {MMM}", d(2026, 3, 5)), "2026-03 Mar");
        assert_eq!(format_folder_name("{M}.{YY} {DD}", d(2026, 3, 5)), "3.26 05");
        assert_eq!(format_folder_name("{FOO} {YYYY}", d(2026, 3, 5)), "{FOO} 2026");
        assert_eq!(format_folder_name("{YYYY}/{MM}", d(2026, 3, 5)), "2026_03");
        assert_eq!(format_folder_name("{YYYY} {", d(2026, 3, 5)), "2026 {");
    }

    #[test]
    fn sorted_folder_detection() {
        assert!(is_sorted_folder_name(DEFAULT_TEMPLATE, "October 2026"));
        assert!(is_sorted_folder_name(DEFAULT_TEMPLATE, "march 2024"));
        assert!(!is_sorted_folder_name(DEFAULT_TEMPLATE, "Holiday 2024"));
        assert!(!is_sorted_folder_name(DEFAULT_TEMPLATE, "October"));
        assert!(is_sorted_folder_name("{YYYY}-{MM}", "2024-03"));
    }

    #[test]
    fn nested_subfolder_parts_are_safe() {
        let day = d(2024, 3, 5);
        assert_eq!(format_folder_parts("{YYYY}{/}{MMMM}", day), vec!["2024", "March"]);
        assert_eq!(format_folder_parts("{YYYY}{/}{MM} {MMMM}", day), vec!["2024", "03 March"]);
        // empty levels are dropped, "." / ".." / separators / drive letters cannot escape
        assert_eq!(format_folder_parts("{/}{YYYY}{/}{/}{MM}{/}", day), vec!["2024", "03"]);
        assert_eq!(format_folder_parts("..{/}{YYYY}", day), vec!["_", "2024"]);
        assert_eq!(format_folder_parts(".{/}..{/}{YYYY}", day), vec!["_", "_", "2024"]);
        assert_eq!(format_folder_parts("C:\\{/}/etc/{YYYY}", day), vec!["C__", "_etc_2024"]);
        assert_eq!(format_folder_parts("{/}", day), vec!["March 2024"]);
        assert_eq!(format_folder_parts("  {/}  ", day), vec!["March 2024"]);
        assert_eq!(format_folder_parts("CON{/}{YYYY}", day), vec!["CON_", "2024"]);
        assert_eq!(format_folder_parts("com1.{YYYY}", day), vec!["com1.2024_"]);
        for p in format_folder_parts("{YYYY}{/}..{/} {MMMM}. ", day) {
            assert!(p != "." && p != ".." && !p.contains(['/', '\\', ':']) && !p.is_empty());
        }
        assert_eq!(template_depth("{YYYY}{/}{MMMM}"), 2);
        assert_eq!(template_depth(DEFAULT_TEMPLATE), 1);
        // single-level templates format exactly as before
        assert_eq!(format_folder_name("{YYYY}/{MM}", day), "2024_03");
        assert!(is_sorted_folder_path("{YYYY}{/}{MMMM}", &["2023".into(), "july".into()]));
        assert!(!is_sorted_folder_path("{YYYY}{/}{MMMM}", &["2023".into()]));
        assert!(!is_sorted_folder_path("{YYYY}{/}{MMMM}", &["Trip".into(), "July".into()]));
        assert!(is_sorted_folder_path(DEFAULT_TEMPLATE, &["July 2023".into()]));
        let lv = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        assert_eq!(sorted_levels("{YYYY}{/}{MMMM}", &lv(&["Trip", "2024", "March"])), 2);
        assert_eq!(sorted_levels("{YYYY}{/}{MMMM}", &lv(&["Trip", "2024"])), 1);
        assert_eq!(sorted_levels("{YYYY}{/}{MMMM}", &lv(&["2024", "Trip"])), 0);
        assert_eq!(sorted_levels("{YYYY}{/}{MMMM}", &lv(&["March"])), 0);
        assert_eq!(sorted_levels(DEFAULT_TEMPLATE, &lv(&["Trip", "march 2024"])), 1);
        assert_eq!(sorted_levels(DEFAULT_TEMPLATE, &lv(&["2024"])), 0);
    }

    #[test]
    fn builder_template_mapping_and_round_trips() {
        let b = FolderBuilder::default();
        assert_eq!(b.to_template(), DEFAULT_TEMPLATE);
        assert_eq!(FolderBuilder::from_template(DEFAULT_TEMPLATE), Some(b));
        let cases = [
            ("{YYYY}-{MM}", YearStyle::Full, MonthStyle::Num2, false, PartOrder::YearFirst, Separator::Dash),
            ("{YYYY}_{MM}", YearStyle::Full, MonthStyle::Num2, false, PartOrder::YearFirst, Separator::Underscore),
            ("{MMM} {YY}", YearStyle::Short, MonthStyle::Short, false, PartOrder::MonthFirst, Separator::Space),
            ("{YYYY}{MM}{DD}", YearStyle::Full, MonthStyle::Num2, true, PartOrder::YearFirst, Separator::None),
            ("{M}.{DD}.{YY}", YearStyle::Short, MonthStyle::Num, true, PartOrder::MonthFirst, Separator::Dot),
            ("{YYYY}{/}{MMMM}", YearStyle::Full, MonthStyle::Name, false, PartOrder::YearFirst, Separator::Subfolder),
            (
                "{YYYY} ~ {MM}",
                YearStyle::Full,
                MonthStyle::Num2,
                false,
                PartOrder::YearFirst,
                Separator::Custom(" ~ ".into()),
            ),
        ];
        for (t, year, month, day, order, sep) in cases {
            let b = FolderBuilder { year, month, day, order, sep };
            assert_eq!(b.to_template(), t);
            assert_eq!(FolderBuilder::from_template(t), Some(b), "{t}");
        }
        // every builder state round-trips
        let seps =
            Separator::FIXED.iter().cloned().chain([Separator::Custom("+".into()), Separator::Custom(" - ".into())]);
        for sep in seps {
            for year in YearStyle::ALL {
                for month in MonthStyle::ALL {
                    for day in [false, true] {
                        for order in [PartOrder::YearFirst, PartOrder::MonthFirst] {
                            let b = FolderBuilder { year, month, day, order, sep: sep.clone() };
                            assert_eq!(FolderBuilder::from_template(&b.to_template()), Some(b));
                        }
                    }
                }
            }
        }
        // presets: all but the mixed one are builder states; all format sanely
        for (t, _) in LAYOUTS {
            assert!(check_template(t).is_empty(), "{t}: {:?}", check_template(t));
            if t != "{YYYY}-{MM} {MMMM}" && t != "{YYYY}{/}{MM} {MMMM}" {
                assert!(FolderBuilder::from_template(t).is_some(), "{t}");
            }
        }
        assert_eq!(LAYOUTS[0].0, DEFAULT_TEMPLATE);
        assert_eq!(example("{YYYY}{/}{MMMM}"), "2024 \\ March");
        assert_eq!(example(DEFAULT_TEMPLATE), "March 2024");
    }

    #[test]
    fn builder_rejects_custom_templates_and_invalid_input() {
        for t in [
            "",
            "{YYYY}",
            "{MMMM}",
            "Photos {YYYY}-{MM}",
            "{YYYY}-{MM} {MMMM}",
            "{YYYY}-{MM}_{DD}",
            "{YYYY}-{YY}",
            "{DD}-{MM}-{YYYY}",
            "{YYYY}-{DD}",
            "{YYYY}-{MM}!",
            "{YYYY}:{MM}",
            "{YYYY}{FOO}{MM}",
            "{YYYY}toolong{MM}",
            "{YYYY} {/} {MM}",
            "{yyyy}-{mm}",
        ] {
            assert_eq!(FolderBuilder::from_template(t), None, "{t}");
        }
        assert!(valid_custom_separator("+"));
        assert!(valid_custom_separator(" - "));
        assert!(!valid_custom_separator(""));
        assert!(!valid_custom_separator("abcdef"));
        assert!(!valid_custom_separator(":"));
        assert!(!valid_custom_separator("{"));
        assert!(!valid_custom_separator("a/b"));
        // inline warnings
        assert!(check_template(DEFAULT_TEMPLATE).is_empty());
        assert!(check_template("{YYYY}{/}{MMMM}").is_empty());
        assert_eq!(check_template("").len(), 1);
        assert!(check_template("{YYYY}:{MM}")[0].contains("“:”"));
        assert!(check_template("{YYYY}/{MM}")[0].contains("{/}"));
        assert!(check_template("{FOO} {YYYY}").iter().any(|w| w.contains("{FOO}")));
        assert!(check_template("{YYYY {MM}").iter().any(|w| w.contains("closing")));
        assert!(check_template("Photos").iter().any(|w| w.contains("No date token")));
        assert!(check_template("{MMMM}").iter().any(|w| w.contains("No year")));
        assert!(check_template("{MMMM} {YYYY}.").iter().any(|w| w.contains("trailing")));
        assert!(check_template("NUL{/}{YYYY}").iter().any(|w| w.contains("device")));
    }
}
