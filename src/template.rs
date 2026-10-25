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
//!
//! Unknown `{...}` tokens are kept literally. Characters that are illegal in
//! Windows file names are replaced with `_`.

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

/// Format a folder name for `date` using `template`.
pub fn format_folder_name(template: &str, date: NaiveDate) -> String {
    let template = if template.trim().is_empty() { DEFAULT_TEMPLATE } else { template };
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
    sanitize_component(&out)
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
/// *some* plausible date (used to recognise folders that PhotoSorter, or the
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
}
