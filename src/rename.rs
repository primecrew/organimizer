//! Ordered rename rules modelled on den4b ReNamer's rule stack
//! (Insert, Delete, Remove, Replace, Rearrange, Extension, Strip, Case,
//! Serialize, Randomize, Padding, Clean Up, Translit, Reformat Date,
//! Regular Expressions, plus date / folder-name "meta tag" inserts).
//! Pascal Script, User Input and Mapping are deliberately not supported.
//!
//! A rule chain is a list of [`RuleSpec`]s (plain data: what the GUI edits
//! and what the INI file stores). [`compile_rules`] turns the *enabled* specs
//! into `Vec<Box<dyn RenameRule>>`, which [`crate::plan::RenameStage`] runs in
//! order on every file before the `Month YYYY` folder stage.
//!
//! Rules work on the file name **without** its extension ("stem"); only the
//! Extension-case rule touches the extension, and only its letter case.
//! After the chain, [`sanitize_file_name`] makes the result a valid Windows
//! file name (no `\ / : * ? " < > |`, no reserved device names such as `CON`,
//! no trailing dots/spaces, at most 255 UTF-16 units). Because path
//! separators are always replaced, a rule can never move a file into a
//! different folder.

use std::fmt::Write as _;
use std::path::Path;

use chrono::NaiveDateTime;
use chrono::format::{Item, StrftimeItems};
use regex::{NoExpand, Regex, RegexBuilder};

use crate::date_source::{DateConfig, ResolvedDate, SourceKind, try_source};

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Where inserted text goes, relative to the current stem (ReNamer's
/// "Where" options of the Insert / Serialize rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    Prefix,
    Suffix,
    /// After this many characters from the start (0 = prefix).
    At(usize),
    /// This many characters before the end (0 = suffix).
    FromEnd(usize),
    /// Replace the whole stem.
    Replace,
    /// After the first occurrence of this text (nothing happens if absent).
    AfterText(String),
    /// Before the first occurrence of this text (nothing happens if absent).
    BeforeText(String),
}

impl Position {
    pub fn key(&self) -> String {
        match self {
            Position::Prefix => "prefix".into(),
            Position::Suffix => "suffix".into(),
            Position::At(n) => format!("at:{n}"),
            Position::FromEnd(n) => format!("end:{n}"),
            Position::Replace => "replace".into(),
            Position::AfterText(t) => format!("after:{t}"),
            Position::BeforeText(t) => format!("before:{t}"),
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        let raw = s.trim_start();
        if raw.len() >= 6 && raw[..6].eq_ignore_ascii_case("after:") {
            return Some(Position::AfterText(raw[6..].to_string()));
        }
        if raw.len() >= 7 && raw[..7].eq_ignore_ascii_case("before:") {
            return Some(Position::BeforeText(raw[7..].to_string()));
        }
        let s = s.trim().to_ascii_lowercase();
        let num = |p: &str| s.strip_prefix(p).and_then(|n| n.trim().parse::<usize>().ok());
        match s.as_str() {
            "prefix" | "start" => Some(Position::Prefix),
            "suffix" | "end" => Some(Position::Suffix),
            "replace" | "name" => Some(Position::Replace),
            _ => num("at:").map(Position::At).or_else(|| num("end:").map(Position::FromEnd)),
        }
    }
    pub fn describe(&self) -> String {
        match self {
            Position::Prefix => "at the start".into(),
            Position::Suffix => "at the end".into(),
            Position::At(n) => format!("after {n} chars"),
            Position::FromEnd(n) => format!("{n} chars before the end"),
            Position::Replace => "as the whole name".into(),
            Position::AfterText(t) => format!("after “{t}”"),
            Position::BeforeText(t) => format!("before “{t}”"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurrence {
    All,
    First,
    Last,
}

impl Occurrence {
    pub fn key(self) -> &'static str {
        match self {
            Occurrence::All => "all",
            Occurrence::First => "first",
            Occurrence::Last => "last",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Occurrence::All),
            "first" => Some(Occurrence::First),
            "last" => Some(Occurrence::Last),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceRule {
    pub find: String,
    pub with: String,
    /// Treat `find` as a regular expression (`with` may use `$1`, `${name}`).
    pub regex: bool,
    pub case_sensitive: bool,
    pub occurrence: Occurrence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveMode {
    /// ReNamer Remove: this text (all / first / last occurrence).
    Text { text: String, occurrence: Occurrence, case_sensitive: bool },
    /// Every occurrence of any of these characters.
    Chars { chars: String, case_sensitive: bool },
    /// The first N characters.
    First(usize),
    /// The last N characters.
    Last(usize),
    /// Characters `from..=to` (1-based, inclusive).
    Range { from: usize, to: usize },
    /// All whitespace.
    Whitespace,
    /// All digits 0-9.
    Digits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTextRule {
    pub text: String,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberingRule {
    pub start: i64,
    pub step: i64,
    /// Minimum number of digits (zero padded), 0/1 = no padding.
    pub padding: usize,
    pub position: Position,
    /// Placed between the number and the name (prefix/suffix only).
    pub separator: String,
    /// Restart the sequence in every source folder ("Reset if folder changes").
    pub per_folder: bool,
    /// Use each index this many times before incrementing (ReNamer "Repeat").
    pub repeat: usize,
    /// Restart after this many files (0 = never; ReNamer "Reset every").
    pub reset_every: usize,
    pub system: NumberSystem,
}

/// ReNamer "Numbering system".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberSystem {
    Decimal,
    /// a, b, ..., z, aa, ab, ...
    LettersLower,
    /// A, B, ..., Z, AA, AB, ...
    LettersUpper,
    /// I, II, III, IV, ... (1..3999)
    Roman,
}

impl NumberSystem {
    pub const ALL: [NumberSystem; 4] =
        [NumberSystem::Decimal, NumberSystem::LettersLower, NumberSystem::LettersUpper, NumberSystem::Roman];
    pub fn key(self) -> &'static str {
        match self {
            NumberSystem::Decimal => "decimal",
            NumberSystem::LettersLower => "abc",
            NumberSystem::LettersUpper => "ABC",
            NumberSystem::Roman => "roman",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim() {
            "abc" => Some(NumberSystem::LettersLower),
            "ABC" => Some(NumberSystem::LettersUpper),
            x if x.eq_ignore_ascii_case("decimal") => Some(NumberSystem::Decimal),
            x if x.eq_ignore_ascii_case("roman") => Some(NumberSystem::Roman),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            NumberSystem::Decimal => "Decimal digits (1, 2, 3)",
            NumberSystem::LettersLower => "Letters (a, b, … z, aa)",
            NumberSystem::LettersUpper => "Letters (A, B, … Z, AA)",
            NumberSystem::Roman => "Roman numerals (I, II, III)",
        }
    }
}

/// ReNamer Delete rule: "From" ...
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteFrom {
    /// 1-based character position.
    Position(usize),
    /// First occurrence of this delimiter.
    Delimiter(String),
}

/// ... "Until".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteUntil {
    Count(usize),
    Delimiter(String),
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRule {
    pub from: DeleteFrom,
    pub until: DeleteUntil,
    /// Count positions / search delimiters from the right.
    pub right_to_left: bool,
    /// Keep the delimiters themselves.
    pub keep_delimiters: bool,
}

/// ReNamer Strip rule: remove whole character classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripRule {
    pub english: bool,
    pub digits: bool,
    /// ASCII punctuation / symbols (except brackets).
    pub symbols: bool,
    /// ( ) [ ] { } < >
    pub brackets: bool,
    pub user: String,
    /// Strip everything EXCEPT the selected characters.
    pub except: bool,
    pub case_sensitive: bool,
}

/// ReNamer Rearrange rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RearrangeRule {
    /// Split at any of these delimiters (`|`-separated list) or, with
    /// `by_positions`, at these 1-based character positions.
    pub split: String,
    pub by_positions: bool,
    /// New name, using $1 $2 ... $-1 (from the end) and $0 (whole name).
    pub pattern: String,
}

/// ReNamer Padding rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaddingRule {
    /// Zero-pad every number sequence to this many digits (0 = off).
    pub zero_pad: usize,
    /// Strip leading zeros from number sequences.
    pub remove_zero_padding: bool,
    /// Pad the whole name to this length (0 = off) ...
    pub text_pad_len: usize,
    /// ... with these characters (repeated) ...
    pub pad_chars: String,
    /// ... on the left (true) or right side.
    pub pad_left: bool,
}

/// ReNamer Randomize rule. Deterministic for a given seed, so the preview
/// and the applied names are identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomizeRule {
    pub length: usize,
    pub digits: bool,
    pub lower: bool,
    pub upper: bool,
    pub user: String,
    pub unique: bool,
    pub position: Position,
    pub seed: u64,
}

/// ReNamer Reformat Date rule: find dates written in one format and
/// rewrite them in another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformatDateRule {
    /// strftime-style formats to look for, `|`-separated (e.g. `%d.%m.%Y|%Y%m%d`).
    pub find: String,
    /// strftime-style output format.
    pub to: String,
    pub whole_words: bool,
}

/// ReNamer Extension rule (safe subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRule {
    /// Set a new extension; only image/video extensions are accepted.
    Set(String),
    /// Fix the extension from the file's binary signature (only when the
    /// current extension does not match the detected media type).
    Detect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Lower,
    Upper,
    /// Capitalize Every Word (rest lower case).
    Title,
    /// First letter capital, rest lower case.
    Sentence,
    /// iNVERT cASE.
    Invert,
}

impl CaseMode {
    pub const ALL: [CaseMode; 5] =
        [CaseMode::Lower, CaseMode::Upper, CaseMode::Title, CaseMode::Sentence, CaseMode::Invert];
    pub fn key(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower",
            CaseMode::Upper => "upper",
            CaseMode::Title => "title",
            CaseMode::Sentence => "sentence",
            CaseMode::Invert => "invert",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        CaseMode::ALL.into_iter().find(|m| m.key().eq_ignore_ascii_case(s.trim()))
    }
    pub fn label(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower case",
            CaseMode::Upper => "UPPER CASE",
            CaseMode::Title => "Title Case",
            CaseMode::Sentence => "Sentence case",
            CaseMode::Invert => "iNVERT cASE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanRule {
    /// Remove leading/trailing spaces, underscores, hyphens and dots.
    pub trim: bool,
    /// Turn runs of whitespace into a single space.
    pub collapse_spaces: bool,
    pub underscores_to_spaces: bool,
    pub spaces_to_underscores: bool,
    pub dots_to_spaces: bool,
    /// Remove characters that are not allowed in Windows names.
    pub remove_illegal: bool,
    /// Collapse repeated separators (`__`, `--`, `..`, `  `) into one.
    pub collapse_repeats: bool,
    /// Remove (…), […] and {…} including their content.
    pub strip_brackets: bool,
    /// Turn every Unicode space (no-break, thin, ...) into a normal space.
    pub normalize_spaces: bool,
    pub strip_emoji: bool,
    /// Remove accents / combining marks (á -> a).
    pub strip_marks: bool,
    /// "HolidayInRome" -> "Holiday In Rome".
    pub space_before_capitals: bool,
}

impl Default for CleanRule {
    fn default() -> Self {
        CleanRule {
            trim: true,
            collapse_spaces: true,
            underscores_to_spaces: false,
            spaces_to_underscores: false,
            dots_to_spaces: false,
            remove_illegal: true,
            collapse_repeats: true,
            strip_brackets: false,
            normalize_spaces: true,
            strip_emoji: false,
            strip_marks: false,
            space_before_capitals: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateRule {
    /// `None` = the date the plan resolved (the same date that picks the
    /// Month YYYY folder). `Some(k)` = read that source for this rule.
    pub source: Option<SourceKind>,
    /// strftime-style format, e.g. `%Y-%m-%d` or `%Y%m%d_%H%M%S`.
    pub format: String,
    pub position: Position,
    pub separator: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentRule {
    pub position: Position,
    pub separator: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtCase {
    Lower,
    Upper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleKind {
    Replace(ReplaceRule),
    Remove(RemoveMode),
    /// ReNamer "Insert".
    AddText(AddTextRule),
    Delete(DeleteRule),
    Strip(StripRule),
    Rearrange(RearrangeRule),
    Padding(PaddingRule),
    Randomize(RandomizeRule),
    Translit,
    ReformatDate(ReformatDateRule),
    Extension(ExtensionRule),
    Numbering(NumberingRule),
    Case(CaseMode),
    Clean(CleanRule),
    Date(DateRule),
    ParentFolder(ParentRule),
    ExtensionCase(ExtCase),
}

/// The rule types, in the order the "Add rule" menu lists them (ReNamer's
/// order, then the meta-tag style inserts). "regex" creates a Replace rule
/// with regular expressions switched on (ReNamer's Regular Expressions rule).
pub const RULE_TYPES: [&str; 18] = [
    "insert",
    "delete",
    "remove",
    "replace",
    "rearrange",
    "extension",
    "strip",
    "case",
    "serialize",
    "randomize",
    "padding",
    "clean",
    "translit",
    "reformatdate",
    "regex",
    "date",
    "parent",
    "extcase",
];

impl RuleKind {
    /// A sensible new rule of the given type key.
    pub fn new_default(key: &str) -> Option<RuleKind> {
        Some(match key {
            "replace" => RuleKind::Replace(ReplaceRule {
                find: String::new(),
                with: String::new(),
                regex: false,
                case_sensitive: false,
                occurrence: Occurrence::All,
            }),
            "regex" => RuleKind::Replace(ReplaceRule {
                find: String::new(),
                with: String::new(),
                regex: true,
                case_sensitive: false,
                occurrence: Occurrence::All,
            }),
            "remove" => RuleKind::Remove(RemoveMode::Text {
                text: String::new(),
                occurrence: Occurrence::All,
                case_sensitive: false,
            }),
            "add" | "insert" => RuleKind::AddText(AddTextRule { text: String::new(), position: Position::Prefix }),
            "delete" => RuleKind::Delete(DeleteRule {
                from: DeleteFrom::Position(1),
                until: DeleteUntil::Count(1),
                right_to_left: false,
                keep_delimiters: false,
            }),
            "strip" => RuleKind::Strip(StripRule {
                english: false,
                digits: true,
                symbols: false,
                brackets: false,
                user: String::new(),
                except: false,
                case_sensitive: false,
            }),
            "rearrange" => RuleKind::Rearrange(RearrangeRule {
                split: " - ".into(),
                by_positions: false,
                pattern: "$2 - $1".into(),
            }),
            "padding" => RuleKind::Padding(PaddingRule {
                zero_pad: 3,
                remove_zero_padding: false,
                text_pad_len: 0,
                pad_chars: "_".into(),
                pad_left: false,
            }),
            "randomize" => RuleKind::Randomize(RandomizeRule {
                length: 6,
                digits: true,
                lower: true,
                upper: false,
                user: String::new(),
                unique: true,
                position: Position::Suffix,
                seed: new_seed(),
            }),
            "translit" => RuleKind::Translit,
            "reformatdate" => RuleKind::ReformatDate(ReformatDateRule {
                find: "%d.%m.%Y|%d-%m-%Y".into(),
                to: "%Y-%m-%d".into(),
                whole_words: true,
            }),
            "extension" => RuleKind::Extension(ExtensionRule::Detect),
            "number" | "serialize" => RuleKind::Numbering(NumberingRule {
                start: 1,
                step: 1,
                padding: 3,
                position: Position::Suffix,
                separator: "_".into(),
                per_folder: true,
                repeat: 1,
                reset_every: 0,
                system: NumberSystem::Decimal,
            }),
            "case" => RuleKind::Case(CaseMode::Lower),
            "clean" => RuleKind::Clean(CleanRule::default()),
            "date" => RuleKind::Date(DateRule {
                source: None,
                format: "%Y-%m-%d".into(),
                position: Position::Prefix,
                separator: " ".into(),
            }),
            "parent" => RuleKind::ParentFolder(ParentRule { position: Position::Prefix, separator: " - ".into() }),
            "extcase" => RuleKind::ExtensionCase(ExtCase::Lower),
            _ => return None,
        })
    }

    pub fn type_key(&self) -> &'static str {
        match self {
            RuleKind::Replace(_) => "replace",
            RuleKind::Remove(_) => "remove",
            RuleKind::AddText(_) => "insert",
            RuleKind::Delete(_) => "delete",
            RuleKind::Strip(_) => "strip",
            RuleKind::Rearrange(_) => "rearrange",
            RuleKind::Padding(_) => "padding",
            RuleKind::Randomize(_) => "randomize",
            RuleKind::Translit => "translit",
            RuleKind::ReformatDate(_) => "reformatdate",
            RuleKind::Extension(_) => "extension",
            RuleKind::Numbering(_) => "serialize",
            RuleKind::Case(_) => "case",
            RuleKind::Clean(_) => "clean",
            RuleKind::Date(_) => "date",
            RuleKind::ParentFolder(_) => "parent",
            RuleKind::ExtensionCase(_) => "extcase",
        }
    }

    pub fn type_label_for(key: &str) -> &'static str {
        match key {
            "replace" => "Replace",
            "regex" => "Regular expressions",
            "remove" => "Remove",
            "add" | "insert" => "Insert",
            "delete" => "Delete",
            "strip" => "Strip",
            "rearrange" => "Rearrange",
            "padding" => "Padding",
            "randomize" => "Randomize",
            "translit" => "Translit",
            "reformatdate" => "Reformat date",
            "extension" => "Extension",
            "number" | "serialize" => "Serialize",
            "case" => "Case",
            "clean" => "Clean up",
            "date" => "Insert date",
            "parent" => "Insert folder name",
            "extcase" => "Extension case",
            _ => "?",
        }
    }

    pub fn type_label(&self) -> &'static str {
        match self {
            RuleKind::Replace(r) if r.regex => Self::type_label_for("regex"),
            _ => Self::type_label_for(self.type_key()),
        }
    }

    /// One-line human description, used in the GUI and as INI comment.
    pub fn summary(&self) -> String {
        match self {
            RuleKind::Replace(r) => {
                let what = if r.regex { "regex" } else { "text" };
                let occ = match r.occurrence {
                    Occurrence::All => "",
                    Occurrence::First => " (first only)",
                    Occurrence::Last => " (last only)",
                };
                let cs = if r.case_sensitive { ", case-sensitive" } else { "" };
                format!("Replace {what} “{}” with “{}”{occ}{cs}", r.find, r.with)
            }
            RuleKind::Remove(m) => match m {
                RemoveMode::Text { text, occurrence, .. } => {
                    format!(
                        "Remove “{text}”{}",
                        match occurrence {
                            Occurrence::All => "",
                            Occurrence::First => " (first)",
                            Occurrence::Last => " (last)",
                        }
                    )
                }
                RemoveMode::Chars { chars, .. } => format!("Remove characters “{chars}”"),
                RemoveMode::First(n) => format!("Remove first {n} characters"),
                RemoveMode::Last(n) => format!("Remove last {n} characters"),
                RemoveMode::Range { from, to } => format!("Remove characters {from} to {to}"),
                RemoveMode::Whitespace => "Remove all whitespace".into(),
                RemoveMode::Digits => "Remove all digits".into(),
            },
            RuleKind::AddText(a) => format!("Insert “{}” {}", a.text, a.position.describe()),
            RuleKind::Delete(d) => {
                let from = match &d.from {
                    DeleteFrom::Position(n) => format!("position {n}"),
                    DeleteFrom::Delimiter(t) => format!("“{t}”"),
                };
                let until = match &d.until {
                    DeleteUntil::Count(n) => format!("{n} characters"),
                    DeleteUntil::Delimiter(t) => format!("until “{t}”"),
                    DeleteUntil::End => "until the end".into(),
                };
                format!("Delete from {from}, {until}{}", if d.right_to_left { " (right-to-left)" } else { "" })
            }
            RuleKind::Strip(st) => {
                let mut sets = vec![];
                if st.english {
                    sets.push("letters".to_string());
                }
                if st.digits {
                    sets.push("digits".into());
                }
                if st.symbols {
                    sets.push("symbols".into());
                }
                if st.brackets {
                    sets.push("brackets".into());
                }
                if !st.user.is_empty() {
                    sets.push(format!("“{}”", st.user));
                }
                format!("Strip {}{}", if st.except { "all except " } else { "" }, sets.join(", "))
            }
            RuleKind::Rearrange(r) => format!(
                "Rearrange: split at {} “{}”, new name “{}”",
                if r.by_positions { "positions" } else { "delimiters" },
                r.split,
                r.pattern
            ),
            RuleKind::Padding(pd) => {
                let mut v = vec![];
                if pd.zero_pad > 0 {
                    v.push(format!("zero-pad numbers to {} digits", pd.zero_pad));
                }
                if pd.remove_zero_padding {
                    v.push("remove zero padding".into());
                }
                if pd.text_pad_len > 0 {
                    v.push(format!("pad name to {} chars with “{}”", pd.text_pad_len, pd.pad_chars));
                }
                if v.is_empty() { "Padding (nothing selected)".into() } else { format!("Padding: {}", v.join(", ")) }
            }
            RuleKind::Randomize(r) => format!("Insert {} random characters {}", r.length, r.position.describe()),
            RuleKind::Translit => "Transliterate to Latin letters (é->e, ß->ss, Ж->Zh …)".into(),
            RuleKind::ReformatDate(r) => format!("Reformat dates “{}” -> “{}”", r.find, r.to),
            RuleKind::Extension(ExtensionRule::Set(e)) => format!("Set extension to .{}", e.trim_start_matches('.')),
            RuleKind::Extension(ExtensionRule::Detect) => "Fix extension from the file's real type".into(),
            RuleKind::Numbering(n) => format!(
                "Serialize from {} (step {}{}) {}{}",
                n.start,
                n.step,
                if n.system == NumberSystem::Decimal {
                    format!(", {} digits", n.padding.max(1))
                } else {
                    format!(", {}", n.system.label())
                },
                n.position.describe(),
                if n.per_folder { ", restart per folder" } else { "" }
            ),
            RuleKind::Case(c) => format!("Change case to {}", c.label()),
            RuleKind::Clean(_) => "Trim / clean up the name".into(),
            RuleKind::Date(d) => format!(
                "Insert {} date as “{}” {}",
                d.source.map(|k| k.label()).unwrap_or("plan"),
                d.format,
                d.position.describe()
            ),
            RuleKind::ParentFolder(p) => format!("Insert parent folder name {}", p.position.describe()),
            RuleKind::ExtensionCase(ExtCase::Lower) => "Extension to lower case".into(),
            RuleKind::ExtensionCase(ExtCase::Upper) => "Extension to UPPER case".into(),
        }
    }

    /// Check the rule's settings (regex syntax, date format...).
    pub fn validate(&self) -> Result<(), String> {
        match self {
            RuleKind::Replace(r) => build_replace_regex(r).map(|_| ()),
            RuleKind::Date(d) => check_date_format(&d.format),
            RuleKind::Remove(RemoveMode::Range { from, to }) if *from == 0 || to < from => {
                Err("Range must satisfy 1 <= from <= to".into())
            }
            RuleKind::Numbering(n) if n.padding > 32 => Err("Padding must be at most 32 digits".into()),
            RuleKind::Numbering(n) if n.repeat == 0 => Err("Repeat must be at least 1".into()),
            RuleKind::Delete(DeleteRule { from: DeleteFrom::Position(0), .. }) => Err("Positions start at 1".into()),
            RuleKind::Delete(DeleteRule { from: DeleteFrom::Delimiter(d), .. }) if d.is_empty() => {
                Err("Enter the delimiter to delete from".into())
            }
            RuleKind::Delete(DeleteRule { until: DeleteUntil::Delimiter(d), .. }) if d.is_empty() => {
                Err("Enter the delimiter to delete until".into())
            }
            RuleKind::Randomize(r) if r.length == 0 || r.length > 64 => Err("Length must be 1 – 64".into()),
            RuleKind::Randomize(r) if random_alphabet(r).is_empty() => Err("Select at least one character set".into()),
            RuleKind::Padding(p) if p.zero_pad > 32 || p.text_pad_len > 255 => Err("Padding length too large".into()),
            RuleKind::Padding(p) if p.text_pad_len > 0 && p.pad_chars.is_empty() => {
                Err("Enter the padding characters".into())
            }
            RuleKind::ReformatDate(r) => {
                check_date_format(&r.to)?;
                for f in r.find.split('|') {
                    find_format_regex(f)?;
                }
                Ok(())
            }
            RuleKind::Extension(ExtensionRule::Set(e)) if !crate::scan::is_media_extension(e) => {
                Err(format!("“{}” is not an image/video extension; only those are allowed", e.trim_start_matches('.')))
            }
            _ => Ok(()),
        }
    }
}

/// One entry of the rule chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpec {
    pub enabled: bool,
    pub kind: RuleKind,
}

impl RuleSpec {
    pub fn new(kind: RuleKind) -> Self {
        RuleSpec { enabled: true, kind }
    }
}

// ---------------------------------------------------------------------------
// Rule trait + runtime context
// ---------------------------------------------------------------------------

/// The name being built: stem (no extension) and extension (with dot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameParts {
    pub stem: String,
    pub ext: String,
    pub warnings: Vec<String>,
}

/// Per-file information available to the rules.
pub struct RuleContext<'a> {
    pub source: &'a Path,
    /// Folder the file is in now.
    pub current_dir: &'a Path,
    /// Folder whose name the "parent folder" rule uses: the file's own folder,
    /// or its grandparent when the file already sits in a Month YYYY folder.
    pub base_dir: &'a Path,
    pub date: Option<ResolvedDate>,
    pub date_config: &'a DateConfig,
    /// 0-based position of this file among all renamed files.
    pub index: u64,
    /// 0-based position of this file within its source folder.
    pub folder_index: u64,
}

pub trait RenameRule: Send + Sync {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext);
}

/// Compile the enabled rules of a chain, in order. Fails on the first
/// invalid rule with "Rule N (Type): message".
pub fn compile_rules(specs: &[RuleSpec]) -> Result<Vec<Box<dyn RenameRule>>, String> {
    let mut out: Vec<Box<dyn RenameRule>> = Vec::new();
    for (i, s) in specs.iter().enumerate() {
        if !s.enabled {
            continue;
        }
        let err = |e: String| format!("Rule {} ({}): {e}", i + 1, s.kind.type_label());
        s.kind.validate().map_err(err)?;
        out.push(match &s.kind {
            RuleKind::Replace(r) => Box::new(CompiledReplace {
                re: build_replace_regex(r).map_err(err)?,
                with: r.with.clone(),
                expand: r.regex,
                occurrence: r.occurrence,
            }),
            RuleKind::Remove(RemoveMode::Text { text, occurrence, case_sensitive }) => Box::new(CompiledReplace {
                re: build_replace_regex(&ReplaceRule {
                    find: text.clone(),
                    with: String::new(),
                    regex: false,
                    case_sensitive: *case_sensitive,
                    occurrence: *occurrence,
                })
                .map_err(err)?,
                with: String::new(),
                expand: false,
                occurrence: *occurrence,
            }),
            RuleKind::Remove(m) => Box::new(m.clone()),
            RuleKind::AddText(a) => Box::new(a.clone()),
            RuleKind::Delete(d) => Box::new(d.clone()),
            RuleKind::Strip(st) => Box::new(st.clone()),
            RuleKind::Rearrange(r) => Box::new(r.clone()),
            RuleKind::Padding(p) => Box::new(CompiledPadding { rule: p.clone(), digits: Regex::new(r"\d+").unwrap() }),
            RuleKind::Randomize(r) => Box::new(CompiledRandomize {
                rule: r.clone(),
                alphabet: random_alphabet(r),
                used: std::sync::Mutex::new(std::collections::HashSet::new()),
            }),
            RuleKind::Translit => Box::new(Translit),
            RuleKind::ReformatDate(r) => Box::new(CompiledReformatDate {
                finders: r
                    .find
                    .split('|')
                    .filter(|f| !f.trim().is_empty())
                    .map(|f| find_format_regex(f).map(|(re, fields)| (re, fields)))
                    .collect::<Result<Vec<_>, String>>()
                    .map_err(err)?,
                to: r.to.clone(),
                whole_words: r.whole_words,
            }),
            RuleKind::Extension(e) => Box::new(e.clone()),
            RuleKind::Numbering(n) => Box::new(n.clone()),
            RuleKind::Case(c) => Box::new(*c),
            RuleKind::Clean(c) => Box::new(c.clone()),
            RuleKind::Date(d) => Box::new(d.clone()),
            RuleKind::ParentFolder(p) => Box::new(p.clone()),
            RuleKind::ExtensionCase(e) => Box::new(*e),
        });
    }
    Ok(out)
}

/// Validate a whole chain; returns (rule index, message) for every problem.
pub fn validate_chain(specs: &[RuleSpec]) -> Vec<(usize, String)> {
    specs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.enabled)
        .filter_map(|(i, s)| s.kind.validate().err().map(|e| (i, e)))
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Byte offset of the `n`-th character (clamped to the end).
fn byte_at(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

/// Insert `value` into `stem` at `pos`; `sep` goes between value and name
/// for prefix/suffix when both are non-empty.
pub fn insert_at(stem: &str, value: &str, pos: &Position, sep: &str) -> String {
    match *pos {
        Position::Replace => value.to_string(),
        Position::Prefix if stem.is_empty() || value.is_empty() => format!("{value}{stem}"),
        Position::Prefix => format!("{value}{sep}{stem}"),
        Position::Suffix if stem.is_empty() || value.is_empty() => format!("{stem}{value}"),
        Position::Suffix => format!("{stem}{sep}{value}"),
        Position::At(n) => {
            let b = byte_at(stem, n);
            format!("{}{value}{}", &stem[..b], &stem[b..])
        }
        Position::FromEnd(n) => {
            let len = char_len(stem);
            let b = byte_at(stem, len.saturating_sub(n));
            format!("{}{value}{}", &stem[..b], &stem[b..])
        }
        Position::AfterText(ref t) => match stem.find(t.as_str()) {
            Some(i) if !t.is_empty() => format!("{}{value}{}", &stem[..i + t.len()], &stem[i + t.len()..]),
            _ => stem.to_string(),
        },
        Position::BeforeText(ref t) => match stem.find(t.as_str()) {
            Some(i) if !t.is_empty() => format!("{}{value}{}", &stem[..i], &stem[i..]),
            _ => stem.to_string(),
        },
    }
}

fn build_replace_regex(r: &ReplaceRule) -> Result<Regex, String> {
    let pat = if r.regex { r.find.clone() } else { regex::escape(&r.find) };
    RegexBuilder::new(&pat).case_insensitive(!r.case_sensitive).size_limit(1 << 20).build().map_err(|e| match e {
        regex::Error::Syntax(s) => format!("invalid regular expression: {}", s.lines().last().unwrap_or("")),
        other => format!("invalid regular expression: {other}"),
    })
}

/// Is `fmt` a usable strftime format?
pub fn check_date_format(fmt: &str) -> Result<(), String> {
    if fmt.trim().is_empty() {
        return Err("date format is empty".into());
    }
    if StrftimeItems::new(fmt).any(|i| matches!(i, Item::Error)) {
        return Err(format!("invalid date format “{fmt}” (use %Y %m %d %H %M %S %b %B %y %j …)"));
    }
    // Formats needing a time zone (%z, %Z, %s...) cannot be used with a local date.
    let probe = NaiveDateTime::default();
    let mut s = String::new();
    if write!(s, "{}", probe.format(fmt)).is_err() {
        return Err(format!("date format “{fmt}” cannot be used (time-zone fields are not supported)"));
    }
    Ok(())
}

/// Format `dt` with `fmt`, replacing characters Windows does not allow
/// (e.g. `:` in `%H:%M`) with `-`. Returns None if the format is unusable.
pub fn format_date(dt: NaiveDateTime, fmt: &str) -> Option<String> {
    check_date_format(fmt).ok()?;
    let mut s = String::new();
    write!(s, "{}", dt.format(fmt)).ok()?;
    Some(s.chars().map(|c| if is_illegal_char(c) { '-' } else { c }).collect())
}

pub fn is_illegal_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20
}

// ---------------------------------------------------------------------------
// Rule implementations
// ---------------------------------------------------------------------------

struct CompiledReplace {
    re: Regex,
    with: String,
    expand: bool,
    occurrence: Occurrence,
}

impl CompiledReplace {
    fn replace_one(&self, s: &str, m: regex::Captures) -> String {
        let whole = m.get(0).unwrap();
        let mut rep = String::new();
        if self.expand {
            m.expand(&self.with, &mut rep);
        } else {
            rep.push_str(&self.with);
        }
        format!("{}{}{}", &s[..whole.start()], rep, &s[whole.end()..])
    }
}

impl RenameRule for CompiledReplace {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        if self.re.as_str().is_empty() {
            return;
        }
        let s = &name.stem;
        let out = match self.occurrence {
            Occurrence::All if self.expand => self.re.replace_all(s, self.with.as_str()).into_owned(),
            Occurrence::All => self.re.replace_all(s, NoExpand(&self.with)).into_owned(),
            Occurrence::First => match self.re.captures(s) {
                Some(c) => self.replace_one(s, c),
                None => return,
            },
            Occurrence::Last => match self.re.captures_iter(s).last() {
                Some(c) => self.replace_one(s, c),
                None => return,
            },
        };
        name.stem = out;
    }
}

impl RenameRule for RemoveMode {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        let s = &name.stem;
        name.stem = match self {
            RemoveMode::Text { text, case_sensitive, .. } if !text.is_empty() => {
                if *case_sensitive {
                    s.replace(text.as_str(), "")
                } else {
                    let re = RegexBuilder::new(&regex::escape(text)).case_insensitive(true).build().unwrap();
                    re.replace_all(s, "").into_owned()
                }
            }
            RemoveMode::Text { .. } => s.clone(),
            RemoveMode::Chars { chars, case_sensitive } => {
                if *case_sensitive {
                    s.chars().filter(|c| !chars.contains(*c)).collect()
                } else {
                    let set: Vec<char> = chars.chars().flat_map(char::to_lowercase).collect();
                    s.chars().filter(|c| !c.to_lowercase().any(|l| set.contains(&l))).collect()
                }
            }
            RemoveMode::First(n) => s.chars().skip(*n).collect(),
            RemoveMode::Last(n) => {
                let len = char_len(s);
                s.chars().take(len.saturating_sub(*n)).collect()
            }
            RemoveMode::Range { from, to } => s
                .chars()
                .enumerate()
                .filter(|(i, _)| {
                    let p = i + 1;
                    p < *from || p > *to
                })
                .map(|(_, c)| c)
                .collect(),
            RemoveMode::Whitespace => s.chars().filter(|c| !c.is_whitespace()).collect(),
            RemoveMode::Digits => s.chars().filter(|c| !c.is_ascii_digit()).collect(),
        };
    }
}

impl RenameRule for AddTextRule {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        name.stem = insert_at(&name.stem, &self.text, &self.position, "");
    }
}

/// Format a sequence number with zero padding (sign kept in front).
pub fn format_number(n: i64, padding: usize) -> String {
    let digits = format!("{:0>width$}", n.unsigned_abs(), width = padding.clamp(1, 32));
    if n < 0 { format!("-{digits}") } else { digits }
}

impl RenameRule for NumberingRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let mut i = if self.per_folder { ctx.folder_index } else { ctx.index };
        if self.reset_every > 0 {
            i %= self.reset_every as u64;
        }
        i /= self.repeat.max(1) as u64;
        let n = self.start.saturating_add(self.step.saturating_mul(i as i64));
        let text = match self.system {
            NumberSystem::Decimal => format_number(n, self.padding),
            NumberSystem::LettersLower => to_letters(n).map(|s| s.to_lowercase()).unwrap_or_else(|| n.to_string()),
            NumberSystem::LettersUpper => to_letters(n).unwrap_or_else(|| n.to_string()),
            NumberSystem::Roman => to_roman(n).unwrap_or_else(|| n.to_string()),
        };
        name.stem = insert_at(&name.stem, &text, &self.position, &self.separator);
    }
}

pub fn change_case(s: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Invert => s
            .chars()
            .flat_map(|c| -> Vec<char> {
                if c.is_uppercase() {
                    c.to_lowercase().collect()
                } else if c.is_lowercase() {
                    c.to_uppercase().collect()
                } else {
                    vec![c]
                }
            })
            .collect(),
        CaseMode::Title => {
            let mut out = String::with_capacity(s.len());
            let mut start = true;
            for c in s.chars() {
                if c.is_alphanumeric() || c == '\'' {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = false;
                } else {
                    out.push(c);
                    start = true;
                }
            }
            out
        }
        CaseMode::Sentence => {
            let mut out = String::with_capacity(s.len());
            let mut done = false;
            for c in s.chars() {
                if !done && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    done = true;
                } else {
                    out.extend(c.to_lowercase());
                }
            }
            out
        }
    }
}

impl RenameRule for CaseMode {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        name.stem = change_case(&name.stem, *self);
    }
}

impl RenameRule for CleanRule {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        let mut s = name.stem.clone();
        if self.strip_brackets {
            s = strip_bracket_content(&s);
        }
        if self.normalize_spaces {
            s = s.chars().map(|c| if c.is_whitespace() && c != ' ' { ' ' } else { c }).collect();
        }
        if self.strip_emoji {
            s.retain(|c| !is_emoji(c));
        }
        if self.strip_marks {
            s = strip_marks(&s);
        }
        if self.space_before_capitals {
            let chars: Vec<char> = s.chars().collect();
            let mut out = String::with_capacity(s.len() + 8);
            for (i, c) in chars.iter().enumerate() {
                if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
                    out.push(' ');
                }
                out.push(*c);
            }
            s = out;
        }
        if self.remove_illegal {
            s.retain(|c| !is_illegal_char(c));
        }
        if self.underscores_to_spaces {
            s = s.replace('_', " ");
        }
        if self.dots_to_spaces {
            s = s.replace('.', " ");
        }
        if self.spaces_to_underscores {
            s = s.replace(' ', "_");
        }
        if self.collapse_spaces {
            let mut out = String::with_capacity(s.len());
            let mut prev_ws = false;
            for c in s.chars() {
                if c.is_whitespace() {
                    if !prev_ws {
                        out.push(' ');
                    }
                    prev_ws = true;
                } else {
                    out.push(c);
                    prev_ws = false;
                }
            }
            s = out;
        }
        if self.collapse_repeats {
            let mut out = String::with_capacity(s.len());
            let mut prev: Option<char> = None;
            for c in s.chars() {
                if matches!(c, ' ' | '_' | '-' | '.') && prev == Some(c) {
                    continue;
                }
                out.push(c);
                prev = Some(c);
            }
            s = out;
        }
        if self.trim {
            s = s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '_' | '-' | '.')).to_string();
        }
        name.stem = s;
    }
}

impl RenameRule for DateRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let dt: Option<NaiveDateTime> = match self.source {
            None => ctx.date.map(|d| d.datetime.unwrap_or_else(|| d.date.and_hms_opt(0, 0, 0).unwrap())),
            Some(kind) => {
                let meta = std::fs::metadata(ctx.source).ok();
                try_source(kind, ctx.source, meta.as_ref(), ctx.date_config.order)
                    .map(|(d, dt)| dt.unwrap_or_else(|| d.and_hms_opt(0, 0, 0).unwrap()))
            }
        };
        let Some(dt) = dt else {
            name.warnings.push(format!(
                "Insert date: no {} date found, nothing inserted",
                self.source.map(|k| k.label()).unwrap_or("usable")
            ));
            return;
        };
        if let Some(text) = format_date(dt, &self.format) {
            name.stem = insert_at(&name.stem, &text, &self.position, &self.separator);
        }
    }
}

impl RenameRule for ParentRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let Some(parent) = ctx.base_dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            name.warnings.push("Insert parent folder: the file is in a drive root, nothing inserted".into());
            return;
        };
        name.stem = insert_at(&name.stem, &parent, &self.position, &self.separator);
    }
}

impl RenameRule for ExtCase {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        name.ext = match self {
            ExtCase::Lower => name.ext.to_lowercase(),
            ExtCase::Upper => name.ext.to_uppercase(),
        };
    }
}

/// Bijective base-26: 1 -> A, 26 -> Z, 27 -> AA. None for n < 1.
pub fn to_letters(n: i64) -> Option<String> {
    if n < 1 {
        return None;
    }
    let mut n = n as u64;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Some(out.into_iter().rev().collect())
}

/// Roman numerals for 1..=3999.
pub fn to_roman(n: i64) -> Option<String> {
    if !(1..=3999).contains(&n) {
        return None;
    }
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut s = String::new();
    for (v, r) in table {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    Some(s)
}

fn strip_bracket_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut stack: Vec<char> = Vec::new();
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' if !stack.is_empty() => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.last() == Some(&open) {
                    stack.pop();
                }
            }
            _ if stack.is_empty() => out.push(c),
            _ => {}
        }
    }
    out
}

fn is_emoji(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D | 0xE0020..=0xE007F)
}

/// Remove accents / combining marks: "Ñandú" -> "Nandu".
pub fn strip_marks(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    use unicode_normalization::char::is_combining_mark;
    s.nfd().filter(|c| !is_combining_mark(*c)).nfc().collect()
}

/// Transliterate to plain Latin letters (Latin specials, Greek, Cyrillic).
pub fn transliterate(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Some(r) = translit_char(c) {
            out.push_str(r);
            continue;
        }
        // accented letter whose base letter is in the table (ή -> η -> i)
        let base = strip_marks(&c.to_string());
        let mut bc = base.chars();
        match (bc.next(), bc.next()) {
            (Some(b), None) if b != c => match translit_char(b) {
                Some(r) => out.push_str(r),
                None => out.push(c),
            },
            _ => out.push(c),
        }
    }
    strip_marks(&out)
}

fn translit_char(c: char) -> Option<&'static str> {
    {
        let r: &str = match c {
            'ß' => "ss",
            'ẞ' => "SS",
            'æ' => "ae",
            'Æ' => "AE",
            'œ' => "oe",
            'Œ' => "OE",
            'ø' => "o",
            'Ø' => "O",
            'đ' | 'ð' => "d",
            'Đ' | 'Ð' => "D",
            'ł' => "l",
            'Ł' => "L",
            'þ' => "th",
            'Þ' => "Th",
            'ı' => "i",
            'ħ' => "h",
            'Ħ' => "H",
            // Cyrillic (Russian / Ukrainian / Belarusian, simple scheme)
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'ґ' => "g",
            'д' => "d",
            'е' => "e",
            'ё' => "yo",
            'є' => "ye",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'і' => "i",
            'ї' => "yi",
            'й' => "y",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ў' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' => "",
            'ы' => "y",
            'ь' => "",
            'э' => "e",
            'ю' => "yu",
            'я' => "ya",
            'А' => "A",
            'Б' => "B",
            'В' => "V",
            'Г' => "G",
            'Ґ' => "G",
            'Д' => "D",
            'Е' => "E",
            'Ё' => "Yo",
            'Є' => "Ye",
            'Ж' => "Zh",
            'З' => "Z",
            'И' => "I",
            'І' => "I",
            'Ї' => "Yi",
            'Й' => "Y",
            'К' => "K",
            'Л' => "L",
            'М' => "M",
            'Н' => "N",
            'О' => "O",
            'П' => "P",
            'Р' => "R",
            'С' => "S",
            'Т' => "T",
            'У' => "U",
            'Ў' => "U",
            'Ф' => "F",
            'Х' => "Kh",
            'Ц' => "Ts",
            'Ч' => "Ch",
            'Ш' => "Sh",
            'Щ' => "Shch",
            'Ъ' => "",
            'Ы' => "Y",
            'Ь' => "",
            'Э' => "E",
            'Ю' => "Yu",
            'Я' => "Ya",
            // Greek
            'α' => "a",
            'β' => "v",
            'γ' => "g",
            'δ' => "d",
            'ε' => "e",
            'ζ' => "z",
            'η' => "i",
            'θ' => "th",
            'ι' => "i",
            'κ' => "k",
            'λ' => "l",
            'μ' => "m",
            'ν' => "n",
            'ξ' => "x",
            'ο' => "o",
            'π' => "p",
            'ρ' => "r",
            'σ' | 'ς' => "s",
            'τ' => "t",
            'υ' => "y",
            'φ' => "f",
            'χ' => "ch",
            'ψ' => "ps",
            'ω' => "o",
            'Α' => "A",
            'Β' => "V",
            'Γ' => "G",
            'Δ' => "D",
            'Ε' => "E",
            'Ζ' => "Z",
            'Η' => "I",
            'Θ' => "Th",
            'Ι' => "I",
            'Κ' => "K",
            'Λ' => "L",
            'Μ' => "M",
            'Ν' => "N",
            'Ξ' => "X",
            'Ο' => "O",
            'Π' => "P",
            'Ρ' => "R",
            'Σ' => "S",
            'Τ' => "T",
            'Υ' => "Y",
            'Φ' => "F",
            'Χ' => "Ch",
            'Ψ' => "Ps",
            'Ω' => "O",
            _ => return None,
        };
        Some(r)
    }
}

struct Translit;
impl RenameRule for Translit {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        name.stem = transliterate(&name.stem);
    }
}

fn find_sub(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

impl RenameRule for DeleteRule {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        let rev =
            |v: &str| -> Vec<char> { if self.right_to_left { v.chars().rev().collect() } else { v.chars().collect() } };
        let s = rev(&name.stem);
        let len = s.len();
        let (start, search_from) = match &self.from {
            DeleteFrom::Position(n) => {
                let i = n.saturating_sub(1);
                if i >= len {
                    return;
                }
                (i, i)
            }
            DeleteFrom::Delimiter(d) => {
                let d = rev(d);
                let Some(i) = find_sub(&s, &d, 0) else { return };
                if self.keep_delimiters { (i + d.len(), i + d.len()) } else { (i, i + d.len()) }
            }
        };
        let end = match &self.until {
            DeleteUntil::Count(n) => (start + n).min(len),
            DeleteUntil::End => len,
            DeleteUntil::Delimiter(d) => {
                let d = rev(d);
                let Some(i) = find_sub(&s, &d, search_from) else { return };
                if self.keep_delimiters { i } else { i + d.len() }
            }
        };
        if end <= start {
            return;
        }
        let mut out: Vec<char> = s[..start].iter().chain(s[end..].iter()).copied().collect();
        if self.right_to_left {
            out.reverse();
        }
        name.stem = out.into_iter().collect();
    }
}

const SYMBOLS: &str = "!\"#$%&'*+,-./:;=?@\\^_`|~";
const BRACKETS: &str = "()[]{}<>";

impl StripRule {
    fn selected(&self, c: char) -> bool {
        if self.english && c.is_ascii_alphabetic() {
            return true;
        }
        if self.digits && c.is_ascii_digit() {
            return true;
        }
        if self.symbols && SYMBOLS.contains(c) {
            return true;
        }
        if self.brackets && BRACKETS.contains(c) {
            return true;
        }
        if self.case_sensitive {
            self.user.contains(c)
        } else {
            let lc: Vec<char> = c.to_lowercase().collect();
            self.user.chars().flat_map(char::to_lowercase).any(|u| lc.contains(&u))
        }
    }
}

impl RenameRule for StripRule {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        name.stem = name.stem.chars().filter(|c| self.selected(*c) == self.except).collect();
    }
}

/// Split a name for the Rearrange rule.
pub fn rearrange_parts(stem: &str, split: &str, by_positions: bool) -> Vec<String> {
    let chars: Vec<char> = stem.chars().collect();
    if by_positions {
        let mut cuts: Vec<usize> = split
            .split('|')
            .filter_map(|p| p.trim().parse::<usize>().ok())
            .filter(|p| *p >= 1 && *p <= chars.len())
            .map(|p| p - 1)
            .collect();
        cuts.sort();
        cuts.dedup();
        let mut parts = Vec::new();
        let mut last = 0;
        for c in cuts {
            parts.push(chars[last..c].iter().collect());
            last = c;
        }
        parts.push(chars[last..].iter().collect());
        return parts;
    }
    let delims: Vec<Vec<char>> = split.split('|').filter(|d| !d.is_empty()).map(|d| d.chars().collect()).collect();
    if delims.is_empty() {
        return vec![stem.to_string()];
    }
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    'outer: while i < chars.len() {
        for d in &delims {
            if chars[i..].starts_with(d) {
                parts.push(std::mem::take(&mut cur));
                i += d.len();
                continue 'outer;
            }
        }
        cur.push(chars[i]);
        i += 1;
    }
    parts.push(cur);
    parts
}

/// Fill `$0`, `$N`, `$-N` in `pattern`.
pub fn fill_pattern(pattern: &str, whole: &str, parts: &[String]) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let mut j = i + 1;
            let neg = j < chars.len() && chars[j] == '-';
            if neg {
                j += 1;
            }
            let ds = j;
            while j < chars.len() && chars[j].is_ascii_digit() && j - ds < 3 {
                j += 1;
            }
            if j > ds {
                let n: usize = chars[ds..j].iter().collect::<String>().parse().unwrap_or(0);
                let part = if n == 0 && !neg {
                    Some(whole.to_string())
                } else if neg {
                    parts.len().checked_sub(n).and_then(|k| parts.get(k)).cloned()
                } else {
                    parts.get(n - 1).cloned()
                };
                out.push_str(&part.unwrap_or_default());
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

impl RenameRule for RearrangeRule {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        let parts = rearrange_parts(&name.stem, &self.split, self.by_positions);
        name.stem = fill_pattern(&self.pattern, &name.stem, &parts);
    }
}

struct CompiledPadding {
    rule: PaddingRule,
    digits: Regex,
}

impl RenameRule for CompiledPadding {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        let r = &self.rule;
        let mut s = name.stem.clone();
        if r.remove_zero_padding {
            s = self
                .digits
                .replace_all(&s, |c: &regex::Captures| {
                    let t = c[0].trim_start_matches('0');
                    if t.is_empty() { "0".to_string() } else { t.to_string() }
                })
                .into_owned();
        }
        if r.zero_pad > 0 {
            s = self
                .digits
                .replace_all(&s, |c: &regex::Captures| format!("{:0>w$}", &c[0], w = r.zero_pad))
                .into_owned();
        }
        let len = char_len(&s);
        if r.text_pad_len > len && !r.pad_chars.is_empty() {
            let pad: String = r.pad_chars.chars().cycle().take(r.text_pad_len - len).collect();
            s = if r.pad_left { format!("{pad}{s}") } else { format!("{s}{pad}") };
        }
        name.stem = s;
    }
}

/// A seed for a new Randomize rule.
pub fn new_seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    splitmix(t as u64 ^ 0x5EED_1234_ABCD_EF01)
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn random_alphabet(r: &RandomizeRule) -> Vec<char> {
    let mut v: Vec<char> = Vec::new();
    if r.digits {
        v.extend('0'..='9');
    }
    if r.lower {
        v.extend('a'..='z');
    }
    if r.upper {
        v.extend('A'..='Z');
    }
    v.extend(r.user.chars().filter(|c| !is_illegal_char(*c) && !c.is_whitespace()));
    v.sort();
    v.dedup();
    v
}

struct CompiledRandomize {
    rule: RandomizeRule,
    alphabet: Vec<char>,
    used: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl RenameRule for CompiledRandomize {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let make = |nonce: u64| -> String {
            let mut x = splitmix(self.rule.seed ^ splitmix(ctx.index.wrapping_mul(0x1000_0001) ^ nonce));
            (0..self.rule.length)
                .map(|_| {
                    x = splitmix(x);
                    self.alphabet[(x % self.alphabet.len() as u64) as usize]
                })
                .collect()
        };
        let mut text = make(0);
        if self.rule.unique {
            let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
            let mut nonce = 1;
            while used.contains(&text) && nonce < 1000 {
                text = make(nonce);
                nonce += 1;
            }
            used.insert(text.clone());
        }
        name.stem = insert_at(&name.stem, &text, &self.rule.position, "");
    }
}

/// Turn a strftime-style "find" format into a regex + the field order.
/// Supported: %Y %y %m %d %H %M %S %b %B (and literal text).
pub fn find_format_regex(fmt: &str) -> Result<(Regex, Vec<char>), String> {
    let fmt = fmt.trim();
    if fmt.is_empty() {
        return Err("empty date format".into());
    }
    let mut re = String::new();
    let mut fields = Vec::new();
    let mut it = fmt.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            re.push_str(&regex::escape(&c.to_string()));
            continue;
        }
        let f = it.next().ok_or_else(|| format!("format “{fmt}” ends with %"))?;
        let part = match f {
            'Y' => r"(\d{4})",
            'y' => r"(\d{2})",
            'm' | 'd' | 'H' | 'M' | 'S' => r"(\d{2})",
            'b' => r"(?i:(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec))",
            'B' => r"(?i:(january|february|march|april|may|june|july|august|september|october|november|december))",
            '%' => {
                re.push('%');
                continue;
            }
            other => {
                return Err(format!("%{other} is not supported in “find” formats (use %Y %y %m %d %H %M %S %b %B)"));
            }
        };
        re.push_str(part);
        fields.push(f);
    }
    if !fields.iter().any(|f| matches!(f, 'Y' | 'y')) || !fields.iter().any(|f| matches!(f, 'm' | 'b' | 'B')) {
        return Err(format!("“{fmt}” must contain a year (%Y or %y) and a month (%m, %b or %B)"));
    }
    Ok((Regex::new(&re).map_err(|e| e.to_string())?, fields))
}

struct CompiledReformatDate {
    finders: Vec<(Regex, Vec<char>)>,
    to: String,
    whole_words: bool,
}

fn month_from_name(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    crate::template::MONTHS
        .iter()
        .position(|m| m.to_ascii_lowercase().starts_with(&l[..3.min(l.len())]))
        .map(|i| i as u32 + 1)
}

impl RenameRule for CompiledReformatDate {
    fn apply(&self, name: &mut NameParts, _ctx: &RuleContext) {
        for (re, fields) in &self.finders {
            let stem = name.stem.clone();
            let mut out = String::new();
            let mut last = 0;
            for caps in re.captures_iter(&stem) {
                let m = caps.get(0).unwrap();
                if self.whole_words {
                    let before = stem[..m.start()].chars().last();
                    let after = stem[m.end()..].chars().next();
                    if before.is_some_and(|c| c.is_alphanumeric()) || after.is_some_and(|c| c.is_alphanumeric()) {
                        continue;
                    }
                }
                let (mut y, mut mo, mut d, mut h, mut mi, mut se) = (None, None, 1u32, 0u32, 0u32, 0u32);
                for (k, f) in fields.iter().enumerate() {
                    let v = &caps[k + 1];
                    let n = v.parse::<u32>().ok();
                    match f {
                        'Y' => y = n.map(|n| n as i32),
                        'y' => y = n.map(|n| 2000 + n as i32 - if n > 69 { 100 } else { 0 }),
                        'm' => mo = n,
                        'b' | 'B' => mo = month_from_name(v),
                        'd' => d = n.unwrap_or(0),
                        'H' => h = n.unwrap_or(99),
                        'M' => mi = n.unwrap_or(99),
                        'S' => se = n.unwrap_or(99),
                        _ => {}
                    }
                }
                let dt = y
                    .zip(mo)
                    .and_then(|(y, mo)| chrono::NaiveDate::from_ymd_opt(y, mo, d))
                    .and_then(|date| date.and_hms_opt(h, mi, se));
                if let Some(dt) = dt
                    && let Some(text) = format_date(dt, &self.to)
                {
                    out.push_str(&stem[last..m.start()]);
                    out.push_str(&text);
                    last = m.end();
                }
            }
            out.push_str(&stem[last..]);
            name.stem = out;
        }
    }
}

/// Media type families detected from the first bytes of a file. The first
/// extension is used when the current one does not belong to the family.
pub fn detect_media_extensions(path: &Path) -> Option<&'static [&'static str]> {
    use std::io::Read;
    let mut b = [0u8; 64];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut b).ok()?;
    let b = &b[..n];
    let at = |i: usize, sig: &[u8]| b.len() >= i + sig.len() && &b[i..i + sig.len()] == sig;
    const TIFF_FAMILY: &[&str] = &[
        "tif", "tiff", "dng", "cr2", "nef", "nrw", "arw", "srf", "sr2", "orf", "pef", "srw", "3fr", "erf", "kdc",
        "mrw", "raw", "rwl", "iiq", "rw2",
    ];
    if at(0, &[0xFF, 0xD8, 0xFF]) {
        return Some(&["jpg", "jpeg", "jpe", "jfif"]);
    }
    if at(0, &[0x89, b'P', b'N', b'G']) {
        return Some(&["png"]);
    }
    if at(0, b"GIF8") {
        return Some(&["gif"]);
    }
    if at(0, b"BM") && b.len() > 14 {
        return Some(&["bmp"]);
    }
    if at(0, b"RIFF") && at(8, b"WEBP") {
        return Some(&["webp"]);
    }
    if at(0, b"RIFF") && at(8, b"AVI ") {
        return Some(&["avi"]);
    }
    if at(0, b"II*\0") || at(0, b"MM\0*") || at(0, b"IIRO") || at(0, b"IIU\0") {
        return Some(TIFF_FAMILY);
    }
    if at(4, b"ftyp") {
        let brand = &b[8..12.min(b.len())];
        return Some(match brand {
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => &["heic", "heif"],
            b"avif" | b"avis" => &["avif"],
            b"crx " => &["cr3"],
            b"qt  " => &["mov", "mp4", "m4v"],
            b"3gp4" | b"3gp5" | b"3gp6" | b"3gg6" | b"3ge6" => &["3gp", "mp4"],
            b"3g2a" | b"3g2b" | b"3g2c" => &["3g2", "mp4"],
            _ => &["mp4", "m4v", "mov", "3gp", "3g2"],
        });
    }
    if at(4, b"moov") || at(4, b"mdat") || at(4, b"wide") || at(4, b"free") {
        return Some(&["mov", "mp4", "m4v"]);
    }
    if at(0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        let is_webm = b.windows(4).any(|w| w == b"webm");
        return Some(if is_webm { &["webm", "mkv"] } else { &["mkv", "webm"] });
    }
    if at(0, &[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return Some(&["wmv"]);
    }
    if at(0, &[0x00, 0x00, 0x01, 0xBA]) {
        return Some(&["mpg", "mpeg", "vob"]);
    }
    if at(0, b"FLV") {
        return Some(&["flv"]);
    }
    if b.first() == Some(&0x47) || at(4, &[0x47]) {
        return Some(&["mts", "m2ts"]);
    }
    if at(0, &[0xFF, 0x0A]) || at(4, b"JXL ") {
        return Some(&["jxl"]);
    }
    None
}

impl RenameRule for ExtensionRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let cur = name.ext.trim_start_matches('.').to_ascii_lowercase();
        let new = match self {
            ExtensionRule::Set(e) => e.trim().trim_start_matches('.').to_string(),
            ExtensionRule::Detect => match detect_media_extensions(ctx.source) {
                Some(fam) if fam.contains(&cur.as_str()) => return,
                Some(fam) => {
                    name.warnings.push(format!("Extension fixed: the file is really .{}", fam[0]));
                    fam[0].to_string()
                }
                None => {
                    name.warnings.push("Extension: file type not recognised, extension kept".into());
                    return;
                }
            },
        };
        if new.is_empty() || new.eq_ignore_ascii_case(&cur) {
            return;
        }
        // keep the user's letter case style (.JPG -> .PNG)
        let upper = name.ext.chars().skip(1).all(|c| !c.is_lowercase()) && name.ext.len() > 1;
        name.ext = format!(".{}", if upper { new.to_uppercase() } else { new.to_lowercase() });
    }
}

// ---------------------------------------------------------------------------
// Windows file-name validation / sanitising
// ---------------------------------------------------------------------------

pub const MAX_NAME_UTF16: usize = 255;
/// Classic MAX_PATH (260 including the terminating NUL).
pub const MAX_PATH_UTF16: usize = 259;

/// Device names Windows reserves regardless of extension.
pub fn is_reserved_device_name(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    match base.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        _ => {
            let pre: String = base.chars().take(3).collect();
            let num: String = base.chars().skip(3).collect();
            let num = num.as_str();
            (pre == "COM" || pre == "LPT")
                && matches!(num, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        }
    }
}

pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Problems with a file name on Windows (empty list = fine).
pub fn windows_name_problems(name: &str) -> Vec<String> {
    let mut v = Vec::new();
    if name.is_empty() {
        v.push("empty name".into());
        return v;
    }
    if name.chars().any(is_illegal_char) {
        v.push("contains characters not allowed on Windows (\\ / : * ? \" < > | or control characters)".into());
    }
    if is_reserved_device_name(name) {
        v.push("is a reserved Windows device name (CON, PRN, AUX, NUL, COM1-9, LPT1-9)".into());
    }
    if name.ends_with('.') || name.ends_with(' ') {
        v.push("ends with a dot or space".into());
    }
    if utf16_len(name) > MAX_NAME_UTF16 {
        v.push(format!("longer than {MAX_NAME_UTF16} characters"));
    }
    if name == "." || name == ".." {
        v.push("is . or ..".into());
    }
    v
}

/// Turn the rule output into a valid Windows file name. `fallback_stem` is
/// used when the rules produced an empty name. Returns (name, warnings).
pub fn sanitize_file_name(stem: &str, ext: &str, fallback_stem: &str) -> (String, Vec<String>) {
    let mut warn = Vec::new();
    let clean = |s: &str| -> String { s.chars().map(|c| if is_illegal_char(c) { '_' } else { c }).collect() };
    let mut stem_s = clean(stem);
    let ext_s = clean(ext);
    if stem_s != stem || ext_s != ext {
        warn.push("Characters not allowed on Windows were replaced with _".to_string());
    }
    // trailing dots/spaces are stripped by Windows; with an extension they are
    // harmless, without one they must go
    if ext_s.is_empty() {
        let t = stem_s.trim_end_matches(['.', ' ']).to_string();
        if t != stem_s {
            warn.push("Trailing dots/spaces removed".to_string());
            stem_s = t;
        }
    }
    if stem_s.trim_matches(['.', ' ']).is_empty() {
        warn.push("The rules produced an empty name; the original name is kept".to_string());
        stem_s = fallback_stem.to_string();
    }
    let mut name = format!("{stem_s}{ext_s}");
    if is_reserved_device_name(&name) {
        warn.push("Reserved Windows name (CON, NUL, COM1...); added _".to_string());
        let base_len = name.split('.').next().unwrap_or("").trim_end_matches(' ').len();
        name.insert(base_len, '_');
        stem_s = name[..name.len() - ext_s.len()].to_string();
    }
    if utf16_len(&name) > MAX_NAME_UTF16 {
        warn.push(format!("Name longer than {MAX_NAME_UTF16} characters was shortened"));
        let mut st: Vec<char> = stem_s.chars().collect();
        while !st.is_empty() && utf16_len(&st.iter().collect::<String>()) + utf16_len(&ext_s) > MAX_NAME_UTF16 {
            st.pop();
        }
        let mut s: String = st.into_iter().collect();
        if ext_s.is_empty() {
            s = s.trim_end_matches(['.', ' ']).to_string();
        }
        name = format!("{s}{ext_s}");
    }
    (name, warn)
}

// ---------------------------------------------------------------------------
// INI (de)serialisation of a single rule:
//   type | on/off | key=value | key=value ...
// Values are percent-encoded for %, |, ", control characters and leading /
// trailing spaces, so any text round-trips through the INI file.
// ---------------------------------------------------------------------------

pub fn encode_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let n = s.chars().count();
    for (i, c) in s.chars().enumerate() {
        let edge = i == 0 || i + 1 == n;
        match c {
            '%' | '|' | '"' | ';' | '#' | '[' | ']' => {
                let _ = write!(out, "%{:02X}", c as u32);
            }
            ' ' if edge => out.push_str("%20"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "%{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

pub fn decode_value(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            let hex = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(b[i + 1]) * 16 + hex(b[i + 2]));
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn b(v: bool) -> &'static str {
    if v { "1" } else { "0" }
}

impl RuleSpec {
    pub fn to_ini_value(&self) -> String {
        let mut parts: Vec<String> =
            vec![self.kind.type_key().to_string(), if self.enabled { "on" } else { "off" }.to_string()];
        let mut kv = |k: &str, v: &str| parts.push(format!("{k}={}", encode_value(v)));
        match &self.kind {
            RuleKind::Replace(r) => {
                kv("find", &r.find);
                kv("with", &r.with);
                kv("regex", b(r.regex));
                kv("case", b(r.case_sensitive));
                kv("occ", r.occurrence.key());
            }
            RuleKind::Remove(m) => match m {
                RemoveMode::Text { text, occurrence, case_sensitive } => {
                    kv("mode", "text");
                    kv("text", text);
                    kv("occ", occurrence.key());
                    kv("case", b(*case_sensitive));
                }
                RemoveMode::Chars { chars, case_sensitive } => {
                    kv("mode", "chars");
                    kv("chars", chars);
                    kv("case", b(*case_sensitive));
                }
                RemoveMode::First(n) => {
                    kv("mode", "first");
                    kv("n", &n.to_string());
                }
                RemoveMode::Last(n) => {
                    kv("mode", "last");
                    kv("n", &n.to_string());
                }
                RemoveMode::Range { from, to } => {
                    kv("mode", "range");
                    kv("from", &from.to_string());
                    kv("to", &to.to_string());
                }
                RemoveMode::Whitespace => kv("mode", "whitespace"),
                RemoveMode::Digits => kv("mode", "digits"),
            },
            RuleKind::AddText(a) => {
                kv("text", &a.text);
                kv("pos", &a.position.key());
            }
            RuleKind::Numbering(n) => {
                kv("start", &n.start.to_string());
                kv("step", &n.step.to_string());
                kv("pad", &n.padding.to_string());
                kv("pos", &n.position.key());
                kv("sep", &n.separator);
                kv("perfolder", b(n.per_folder));
                kv("repeat", &n.repeat.to_string());
                kv("reset", &n.reset_every.to_string());
                kv("system", n.system.key());
            }
            RuleKind::Delete(d) => {
                match &d.from {
                    DeleteFrom::Position(n) => kv("from", &n.to_string()),
                    DeleteFrom::Delimiter(t) => kv("fromdelim", t),
                }
                match &d.until {
                    DeleteUntil::Count(n) => kv("count", &n.to_string()),
                    DeleteUntil::Delimiter(t) => kv("untildelim", t),
                    DeleteUntil::End => kv("until", "end"),
                }
                kv("rtl", b(d.right_to_left));
                kv("keepdelim", b(d.keep_delimiters));
            }
            RuleKind::Strip(st) => {
                kv("english", b(st.english));
                kv("digits", b(st.digits));
                kv("symbols", b(st.symbols));
                kv("brackets", b(st.brackets));
                kv("user", &st.user);
                kv("except", b(st.except));
                kv("case", b(st.case_sensitive));
            }
            RuleKind::Rearrange(r) => {
                kv("split", &r.split);
                kv("bypos", b(r.by_positions));
                kv("pattern", &r.pattern);
            }
            RuleKind::Padding(pd) => {
                kv("zeropad", &pd.zero_pad.to_string());
                kv("unpad", b(pd.remove_zero_padding));
                kv("len", &pd.text_pad_len.to_string());
                kv("chars", &pd.pad_chars);
                kv("left", b(pd.pad_left));
            }
            RuleKind::Randomize(r) => {
                kv("len", &r.length.to_string());
                kv("digits", b(r.digits));
                kv("lower", b(r.lower));
                kv("upper", b(r.upper));
                kv("user", &r.user);
                kv("unique", b(r.unique));
                kv("pos", &r.position.key());
                kv("seed", &r.seed.to_string());
            }
            RuleKind::Translit => {}
            RuleKind::ReformatDate(r) => {
                kv("find", &r.find);
                kv("to", &r.to);
                kv("whole", b(r.whole_words));
            }
            RuleKind::Extension(ExtensionRule::Set(e)) => kv("set", e),
            RuleKind::Extension(ExtensionRule::Detect) => kv("mode", "detect"),
            RuleKind::Case(c) => kv("mode", c.key()),
            RuleKind::Clean(c) => {
                kv("trim", b(c.trim));
                kv("spaces", b(c.collapse_spaces));
                kv("us2sp", b(c.underscores_to_spaces));
                kv("sp2us", b(c.spaces_to_underscores));
                kv("dot2sp", b(c.dots_to_spaces));
                kv("illegal", b(c.remove_illegal));
                kv("repeats", b(c.collapse_repeats));
                kv("brackets", b(c.strip_brackets));
                kv("normspaces", b(c.normalize_spaces));
                kv("emoji", b(c.strip_emoji));
                kv("marks", b(c.strip_marks));
                kv("capspace", b(c.space_before_capitals));
            }
            RuleKind::Date(d) => {
                kv("source", d.source.map(|k| k.key()).unwrap_or("plan"));
                kv("fmt", &d.format);
                kv("pos", &d.position.key());
                kv("sep", &d.separator);
            }
            RuleKind::ParentFolder(p) => {
                kv("pos", &p.position.key());
                kv("sep", &p.separator);
            }
            RuleKind::ExtensionCase(e) => kv(
                "mode",
                match e {
                    ExtCase::Lower => "lower",
                    ExtCase::Upper => "upper",
                },
            ),
        }
        parts.join(" | ")
    }

    /// Parse one `ruleN = ...` value. Unknown keys are ignored, missing or
    /// invalid ones use the rule's defaults; an unknown rule type gives None.
    pub fn from_ini_value(s: &str) -> Option<RuleSpec> {
        let mut it = s.split('|').map(str::trim);
        let ty = it.next()?.to_ascii_lowercase();
        let mut kind = RuleKind::new_default(&ty)?;
        let mut enabled = true;
        let mut map = std::collections::HashMap::new();
        for p in it {
            if let Some(eq) = p.find('=') {
                map.insert(p[..eq].trim().to_ascii_lowercase(), decode_value(p[eq + 1..].trim()));
            } else {
                match p.to_ascii_lowercase().as_str() {
                    "on" | "1" | "true" | "enabled" => enabled = true,
                    "off" | "0" | "false" | "disabled" => enabled = false,
                    _ => {}
                }
            }
        }
        let get = |k: &str| map.get(k).map(String::as_str);
        let flag = |k: &str, d: bool| match get(k).map(|v| v.trim().to_ascii_lowercase()) {
            Some(v) if v == "1" || v == "true" || v == "yes" || v == "on" => true,
            Some(v) if v == "0" || v == "false" || v == "no" || v == "off" => false,
            _ => d,
        };
        let num = |k: &str, d: usize| get(k).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(d);
        let inum = |k: &str, d: i64| get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(d);
        let pos = |d: Position| get("pos").and_then(Position::from_key).unwrap_or(d);
        match &mut kind {
            RuleKind::Replace(r) => {
                r.find = get("find").unwrap_or_default().to_string();
                r.with = get("with").unwrap_or_default().to_string();
                r.regex = flag("regex", r.regex);
                r.case_sensitive = flag("case", false);
                r.occurrence = get("occ").and_then(Occurrence::from_key).unwrap_or(Occurrence::All);
            }
            RuleKind::Remove(m) => {
                *m = match get("mode").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
                    Some("text") => RemoveMode::Text {
                        text: get("text").unwrap_or_default().to_string(),
                        occurrence: get("occ").and_then(Occurrence::from_key).unwrap_or(Occurrence::All),
                        case_sensitive: flag("case", false),
                    },
                    Some("chars") => RemoveMode::Chars {
                        chars: get("chars").unwrap_or_default().to_string(),
                        case_sensitive: flag("case", false),
                    },
                    Some("last") => RemoveMode::Last(num("n", 0)),
                    Some("range") => RemoveMode::Range { from: num("from", 1), to: num("to", 1) },
                    Some("whitespace") => RemoveMode::Whitespace,
                    Some("digits") => RemoveMode::Digits,
                    Some("first") => RemoveMode::First(num("n", 0)),
                    _ => RemoveMode::Text {
                        text: get("text").unwrap_or_default().to_string(),
                        occurrence: get("occ").and_then(Occurrence::from_key).unwrap_or(Occurrence::All),
                        case_sensitive: flag("case", false),
                    },
                };
            }
            RuleKind::AddText(a) => {
                a.text = get("text").unwrap_or_default().to_string();
                a.position = pos(Position::Prefix);
            }
            RuleKind::Numbering(n) => {
                n.start = inum("start", 1);
                n.step = inum("step", 1);
                n.padding = num("pad", 3).min(32);
                n.position = pos(Position::Suffix);
                if let Some(v) = get("sep") {
                    n.separator = v.to_string();
                }
                n.per_folder = flag("perfolder", true);
                n.repeat = num("repeat", 1).max(1);
                n.reset_every = num("reset", 0);
                n.system = get("system").and_then(NumberSystem::from_key).unwrap_or(NumberSystem::Decimal);
            }
            RuleKind::Delete(d) => {
                d.from = match get("fromdelim") {
                    Some(t) => DeleteFrom::Delimiter(t.to_string()),
                    None => DeleteFrom::Position(num("from", 1)),
                };
                d.until = match (get("untildelim"), get("until")) {
                    (Some(t), _) => DeleteUntil::Delimiter(t.to_string()),
                    (None, Some(e)) if e.trim().eq_ignore_ascii_case("end") => DeleteUntil::End,
                    _ => DeleteUntil::Count(num("count", 1)),
                };
                d.right_to_left = flag("rtl", false);
                d.keep_delimiters = flag("keepdelim", false);
            }
            RuleKind::Strip(st) => {
                st.english = flag("english", false);
                st.digits = flag("digits", false);
                st.symbols = flag("symbols", false);
                st.brackets = flag("brackets", false);
                st.user = get("user").unwrap_or_default().to_string();
                st.except = flag("except", false);
                st.case_sensitive = flag("case", false);
            }
            RuleKind::Rearrange(r) => {
                r.split = get("split").unwrap_or_default().to_string();
                r.by_positions = flag("bypos", false);
                r.pattern = get("pattern").unwrap_or("$0").to_string();
            }
            RuleKind::Padding(pd) => {
                pd.zero_pad = num("zeropad", 0).min(32);
                pd.remove_zero_padding = flag("unpad", false);
                pd.text_pad_len = num("len", 0).min(255);
                pd.pad_chars = get("chars").unwrap_or("_").to_string();
                pd.pad_left = flag("left", false);
            }
            RuleKind::Randomize(r) => {
                r.length = num("len", 6).clamp(1, 64);
                r.digits = flag("digits", true);
                r.lower = flag("lower", true);
                r.upper = flag("upper", false);
                r.user = get("user").unwrap_or_default().to_string();
                r.unique = flag("unique", true);
                r.position = pos(Position::Suffix);
                if let Some(sd) = get("seed").and_then(|v| v.trim().parse::<u64>().ok()) {
                    r.seed = sd;
                }
            }
            RuleKind::Translit => {}
            RuleKind::ReformatDate(r) => {
                if let Some(f) = get("find") {
                    r.find = f.to_string();
                }
                if let Some(t) = get("to") {
                    r.to = t.to_string();
                }
                r.whole_words = flag("whole", true);
            }
            RuleKind::Extension(e) => {
                *e = match get("set") {
                    Some(x) if !x.trim().is_empty() => ExtensionRule::Set(x.trim().to_string()),
                    _ => ExtensionRule::Detect,
                };
            }
            RuleKind::Case(c) => *c = get("mode").and_then(CaseMode::from_key).unwrap_or(CaseMode::Lower),
            RuleKind::Clean(c) => {
                let d = CleanRule::default();
                c.trim = flag("trim", d.trim);
                c.collapse_spaces = flag("spaces", d.collapse_spaces);
                c.underscores_to_spaces = flag("us2sp", d.underscores_to_spaces);
                c.spaces_to_underscores = flag("sp2us", d.spaces_to_underscores);
                c.dots_to_spaces = flag("dot2sp", d.dots_to_spaces);
                c.remove_illegal = flag("illegal", d.remove_illegal);
                c.collapse_repeats = flag("repeats", d.collapse_repeats);
                c.strip_brackets = flag("brackets", d.strip_brackets);
                c.normalize_spaces = flag("normspaces", d.normalize_spaces);
                c.strip_emoji = flag("emoji", d.strip_emoji);
                c.strip_marks = flag("marks", d.strip_marks);
                c.space_before_capitals = flag("capspace", d.space_before_capitals);
            }
            RuleKind::Date(d) => {
                d.source = get("source").and_then(SourceKind::from_key);
                if let Some(f) = get("fmt").filter(|f| !f.trim().is_empty()) {
                    d.format = f.to_string();
                }
                d.position = pos(Position::Prefix);
                if let Some(v) = get("sep") {
                    d.separator = v.to_string();
                }
            }
            RuleKind::ParentFolder(p) => {
                p.position = pos(Position::Prefix);
                if let Some(v) = get("sep") {
                    p.separator = v.to_string();
                }
            }
            RuleKind::ExtensionCase(e) => {
                *e = match get("mode").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
                    Some("upper") => ExtCase::Upper,
                    _ => ExtCase::Lower,
                }
            }
        }
        Some(RuleSpec { enabled, kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date_source::DateConfig;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    struct Env {
        source: PathBuf,
        cur: PathBuf,
        base: PathBuf,
        date: Option<ResolvedDate>,
        cfg: DateConfig,
    }

    fn env(name: &str) -> Env {
        let cur = PathBuf::from("/photos/Trip Rome");
        Env {
            source: cur.join(name),
            base: cur.clone(),
            cur,
            date: Some(ResolvedDate {
                date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
                datetime: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap().and_hms_opt(14, 25, 30),
                source: SourceKind::Filename,
                via_fallback: false,
            }),
            cfg: DateConfig::default(),
        }
    }

    /// Run a chain on one name (index/folder_index given).
    fn run_at(name: &str, rules: &[RuleKind], index: u64, folder_index: u64) -> (String, Vec<String>) {
        let e = env(name);
        let specs: Vec<RuleSpec> = rules.iter().cloned().map(RuleSpec::new).collect();
        let compiled = compile_rules(&specs).unwrap();
        let (stem, ext) = crate::plan::split_name(name);
        let ctx = RuleContext {
            source: &e.source,
            current_dir: &e.cur,
            base_dir: &e.base,
            date: e.date,
            date_config: &e.cfg,
            index,
            folder_index,
        };
        let mut p = NameParts { stem: stem.into(), ext: ext.into(), warnings: vec![] };
        for r in &compiled {
            r.apply(&mut p, &ctx);
        }
        let (n, mut w) = sanitize_file_name(&p.stem, &p.ext, stem);
        p.warnings.append(&mut w);
        (n, p.warnings)
    }

    fn run(name: &str, rules: &[RuleKind]) -> String {
        run_at(name, rules, 0, 0).0
    }

    fn replace(find: &str, with: &str, regex: bool, case: bool, occ: Occurrence) -> RuleKind {
        RuleKind::Replace(ReplaceRule {
            find: find.into(),
            with: with.into(),
            regex,
            case_sensitive: case,
            occurrence: occ,
        })
    }

    #[test]
    fn replace_plain_and_case() {
        assert_eq!(run("IMG_1234.jpg", &[replace("IMG_", "Rome_", false, false, Occurrence::All)]), "Rome_1234.jpg");
        assert_eq!(run("img_1234.jpg", &[replace("IMG_", "X", false, false, Occurrence::All)]), "X1234.jpg");
        assert_eq!(run("img_1234.jpg", &[replace("IMG_", "X", false, true, Occurrence::All)]), "img_1234.jpg");
        assert_eq!(run("a-b-c.jpg", &[replace("-", "_", false, false, Occurrence::First)]), "a_b-c.jpg");
        assert_eq!(run("a-b-c.jpg", &[replace("-", "_", false, false, Occurrence::Last)]), "a-b_c.jpg");
        // plain mode does not interpret regex syntax or $ in the replacement
        assert_eq!(run("a.b(1).jpg", &[replace(".b(1)", "$1", false, false, Occurrence::All)]), "a$1.jpg");
        // the extension is never touched by replace
        assert_eq!(run("jpg.jpg", &[replace("jpg", "png", false, false, Occurrence::All)]), "png.jpg");
        // empty find = no-op
        assert_eq!(run("x.jpg", &[replace("", "y", false, false, Occurrence::All)]), "x.jpg");
    }

    #[test]
    fn replace_regex() {
        let r = replace(r"^IMG_(\d{4})(\d{2})(\d{2})_.*$", "$1-$2-$3", true, false, Occurrence::All);
        assert_eq!(run("IMG_20240315_142530.jpg", &[r]), "2024-03-15.jpg");
        let r = replace(r"\s+", " ", true, false, Occurrence::All);
        assert_eq!(run("a   b \t c.png", &[r]), "a b c.png");
        let r = replace(r"(?P<n>\d+)", "#${n}", true, false, Occurrence::Last);
        assert_eq!(run("a1b22.jpg", &[r]), "a1b#22.jpg");
        // regex trying to inject a path separator is neutralised
        let r = replace("a", r"..\..\evil", true, false, Occurrence::All);
        let (n, w) = run_at("a.jpg", &[r], 0, 0);
        assert_eq!(n, ".._.._evil.jpg");
        assert!(!w.is_empty());
        // invalid regex is reported, not applied
        let bad = RuleSpec::new(replace("(unclosed", "", true, false, Occurrence::All));
        let e = compile_rules(&[bad.clone()]).err().unwrap();
        assert!(e.starts_with("Rule 1 (Regular expressions): invalid regular expression"), "{e}");
        assert_eq!(validate_chain(&[bad]).len(), 1);
        // disabled invalid rules do not block
        let mut off = RuleSpec::new(replace("(", "", true, false, Occurrence::All));
        off.enabled = false;
        assert!(compile_rules(&[off]).unwrap().is_empty());
    }

    #[test]
    fn remove_modes() {
        let rm = |m: RemoveMode| RuleKind::Remove(m);
        assert_eq!(run("IMG_1234.jpg", &[rm(RemoveMode::First(4))]), "1234.jpg");
        assert_eq!(run("IMG_1234.jpg", &[rm(RemoveMode::Last(2))]), "IMG_12.jpg");
        assert_eq!(run("IMG_1234.jpg", &[rm(RemoveMode::Range { from: 4, to: 5 })]), "IMG234.jpg");
        assert_eq!(run("a b\tc.jpg", &[rm(RemoveMode::Whitespace)]), "abc.jpg");
        assert_eq!(run("a1b2c3.jpg", &[rm(RemoveMode::Digits)]), "abc.jpg");
        assert_eq!(
            run("Hello (World).jpg", &[rm(RemoveMode::Chars { chars: "()o".into(), case_sensitive: false })]),
            "Hell Wrld.jpg"
        );
        assert_eq!(run("OoO.jpg", &[rm(RemoveMode::Chars { chars: "o".into(), case_sensitive: true })]), "OO.jpg");
        // unicode-safe positions
        assert_eq!(run("ÄÖÜabc.jpg", &[rm(RemoveMode::First(2))]), "Üabc.jpg");
        // removing everything keeps the original name
        let (n, w) = run_at("abc.jpg", &[rm(RemoveMode::First(99))], 0, 0);
        assert_eq!(n, "abc.jpg");
        assert!(w[0].contains("empty"));
        assert!(RuleKind::Remove(RemoveMode::Range { from: 0, to: 2 }).validate().is_err());
        assert!(RuleKind::Remove(RemoveMode::Range { from: 3, to: 2 }).validate().is_err());
    }

    #[test]
    fn add_text_positions() {
        let add = |t: &str, p: Position| RuleKind::AddText(AddTextRule { text: t.into(), position: p });
        assert_eq!(run("pic.jpg", &[add("Rome_", Position::Prefix)]), "Rome_pic.jpg");
        assert_eq!(run("pic.jpg", &[add("_v2", Position::Suffix)]), "pic_v2.jpg");
        assert_eq!(run("pic.jpg", &[add("-", Position::At(1))]), "p-ic.jpg");
        assert_eq!(run("pic.jpg", &[add("-", Position::At(99))]), "pic-.jpg");
        assert_eq!(run("pic.jpg", &[add("-", Position::FromEnd(1))]), "pi-c.jpg");
        assert_eq!(run("pic.jpg", &[add("new", Position::Replace)]), "new.jpg");
    }

    #[test]
    fn numbering() {
        let num = |start, step, pad, pos, per_folder| {
            RuleKind::Numbering(NumberingRule {
                start,
                step,
                padding: pad,
                position: pos,
                separator: "_".into(),
                per_folder,
                repeat: 1,
                reset_every: 0,
                system: NumberSystem::Decimal,
            })
        };
        assert_eq!(run_at("a.jpg", &[num(1, 1, 3, Position::Suffix, false)], 0, 0).0, "a_001.jpg");
        assert_eq!(run_at("a.jpg", &[num(1, 1, 3, Position::Suffix, false)], 41, 0).0, "a_042.jpg");
        assert_eq!(run_at("a.jpg", &[num(10, 5, 0, Position::Prefix, false)], 2, 0).0, "20_a.jpg");
        // per-folder uses the folder index
        assert_eq!(run_at("a.jpg", &[num(1, 1, 2, Position::Suffix, true)], 41, 3).0, "a_04.jpg");
        assert_eq!(run_at("a.jpg", &[num(1, 1, 4, Position::Replace, true)], 0, 0).0, "0001.jpg");
        assert_eq!(run_at("a.jpg", &[num(0, -1, 2, Position::Suffix, false)], 3, 0).0, "a_-03.jpg");
        assert_eq!(format_number(7, 0), "7");
        assert_eq!(format_number(1234, 3), "1234");
        assert!(num(1, 1, 33, Position::Suffix, false).validate().is_err());
    }

    #[test]
    fn case_modes() {
        let c = |m| RuleKind::Case(m);
        assert_eq!(run("Hello World.JPG", &[c(CaseMode::Lower)]), "hello world.JPG");
        assert_eq!(run("Hello World.jpg", &[c(CaseMode::Upper)]), "HELLO WORLD.jpg");
        assert_eq!(run("hello wORLD_trip-2024.jpg", &[c(CaseMode::Title)]), "Hello World_Trip-2024.jpg");
        assert_eq!(run("HELLO wORLD. again.jpg", &[c(CaseMode::Sentence)]), "Hello world. again.jpg");
        assert_eq!(run("2024 rome.jpg", &[c(CaseMode::Sentence)]), "2024 Rome.jpg");
        assert_eq!(run("AbC.jpg", &[c(CaseMode::Invert)]), "aBc.jpg");
        assert_eq!(run("straße.jpg", &[c(CaseMode::Upper)]), "STRASSE.jpg");
    }

    #[test]
    fn clean_rule() {
        let all = CleanRule::default();
        assert_eq!(run("  my__photo  --  final..jpg", &[RuleKind::Clean(all.clone())]), "my_photo - final.jpg");
        let u2s = CleanRule { underscores_to_spaces: true, ..all.clone() };
        assert_eq!(run("_my_photo_.jpg", &[RuleKind::Clean(u2s)]), "my photo.jpg");
        let s2u = CleanRule { spaces_to_underscores: true, ..all.clone() };
        assert_eq!(run("my  photo.jpg", &[RuleKind::Clean(s2u)]), "my_photo.jpg");
        let d2s = CleanRule { dots_to_spaces: true, ..all.clone() };
        assert_eq!(
            run("Screenshot 2024.03.15 at 10.22.01.png", &[RuleKind::Clean(d2s)]),
            "Screenshot 2024 03 15 at 10 22 01.png"
        );
        let off = CleanRule {
            trim: false,
            collapse_spaces: false,
            underscores_to_spaces: false,
            spaces_to_underscores: false,
            dots_to_spaces: false,
            remove_illegal: false,
            collapse_repeats: false,
            strip_brackets: false,
            normalize_spaces: false,
            strip_emoji: false,
            strip_marks: false,
            space_before_capitals: false,
        };
        assert_eq!(run(" a  b .jpg", &[RuleKind::Clean(off)]), " a  b .jpg");
    }

    #[test]
    fn insert_date() {
        let d = |fmt: &str, pos| {
            RuleKind::Date(DateRule { source: None, format: fmt.into(), position: pos, separator: " ".into() })
        };
        assert_eq!(run("beach.jpg", &[d("%Y-%m-%d", Position::Prefix)]), "2024-03-15 beach.jpg");
        assert_eq!(run("beach.jpg", &[d("%Y%m%d_%H%M%S", Position::Replace)]), "20240315_142530.jpg");
        // ':' is not allowed on Windows -> '-'
        assert_eq!(run("beach.jpg", &[d("%H:%M", Position::Suffix)]), "beach 14-25.jpg");
        assert_eq!(run("beach.jpg", &[d("%B %Y", Position::Suffix)]), "beach March 2024.jpg");
        assert!(check_date_format("%Y-%m-%d").is_ok());
        assert!(check_date_format("%Q").is_err());
        assert!(check_date_format("").is_err());
        assert!(check_date_format("%z").is_err());
        assert!(d("%Q", Position::Prefix).validate().is_err());
    }

    #[test]
    fn insert_date_without_date_warns() {
        let e = Env { date: None, ..env("x.jpg") };
        let rule = DateRule { source: None, format: "%Y".into(), position: Position::Prefix, separator: " ".into() };
        let ctx = RuleContext {
            source: &e.source,
            current_dir: &e.cur,
            base_dir: &e.base,
            date: None,
            date_config: &e.cfg,
            index: 0,
            folder_index: 0,
        };
        let mut p = NameParts { stem: "x".into(), ext: ".jpg".into(), warnings: vec![] };
        rule.apply(&mut p, &ctx);
        assert_eq!(p.stem, "x");
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn insert_date_from_specific_source() {
        // Filename source on a dated name, independent of the plan date
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("IMG_20190704_101010.jpg");
        std::fs::write(&src, b"x").unwrap();
        let cfg = DateConfig::default();
        let ctx = RuleContext {
            source: &src,
            current_dir: dir.path(),
            base_dir: dir.path(),
            date: None,
            date_config: &cfg,
            index: 0,
            folder_index: 0,
        };
        let rule = DateRule {
            source: Some(SourceKind::Filename),
            format: "%Y-%m".into(),
            position: Position::Replace,
            separator: String::new(),
        };
        let mut p = NameParts { stem: "IMG_20190704_101010".into(), ext: ".jpg".into(), warnings: vec![] };
        rule.apply(&mut p, &ctx);
        assert_eq!(p.stem, "2019-07");
    }

    #[test]
    fn parent_folder() {
        let r = RuleKind::ParentFolder(ParentRule { position: Position::Prefix, separator: " - ".into() });
        assert_eq!(run("IMG_1.jpg", &[r]), "Trip Rome - IMG_1.jpg");
        let r = RuleKind::ParentFolder(ParentRule { position: Position::Suffix, separator: "_".into() });
        assert_eq!(run("IMG_1.jpg", &[r]), "IMG_1_Trip Rome.jpg");
    }

    #[test]
    fn extension_case() {
        assert_eq!(run("Photo.JPG", &[RuleKind::ExtensionCase(ExtCase::Lower)]), "Photo.jpg");
        assert_eq!(run("Photo.jpeg", &[RuleKind::ExtensionCase(ExtCase::Upper)]), "Photo.JPEG");
    }

    #[test]
    fn chain_order_matters() {
        let add = RuleKind::AddText(AddTextRule { text: "img ".into(), position: Position::Prefix });
        let up = RuleKind::Case(CaseMode::Upper);
        assert_eq!(run("a.jpg", &[add.clone(), up.clone()]), "IMG A.jpg");
        assert_eq!(run("a.jpg", &[up, add]), "img A.jpg");
        // a realistic chain
        let chain = [
            replace(r"^(IMG|PXL|DSC)[_-]?", "", true, false, Occurrence::All),
            RuleKind::Remove(RemoveMode::Last(7)),
            RuleKind::Date(DateRule {
                source: None,
                format: "%Y-%m-%d".into(),
                position: Position::Replace,
                separator: String::new(),
            }),
            RuleKind::ParentFolder(ParentRule { position: Position::Suffix, separator: " ".into() }),
            RuleKind::Numbering(NumberingRule {
                start: 1,
                step: 1,
                padding: 3,
                position: Position::Suffix,
                separator: " #".into(),
                per_folder: true,
                repeat: 1,
                reset_every: 0,
                system: NumberSystem::Decimal,
            }),
            RuleKind::Clean(CleanRule { spaces_to_underscores: true, ..CleanRule::default() }),
            RuleKind::ExtensionCase(ExtCase::Lower),
        ];
        assert_eq!(run_at("IMG_20240315_142530.JPG", &chain, 0, 4).0, "2024-03-15_Trip_Rome_#005.jpg");
    }

    #[test]
    fn windows_name_rules() {
        for n in ["CON", "con.jpg", "Nul.tar.jpg", "COM1.png", "lpt9.jpg", "AUX .jpg", "COM¹.jpg", "CONIN$.jpg"] {
            assert!(is_reserved_device_name(n), "{n}");
        }
        for n in ["CONSOLE.jpg", "COM10.jpg", "LPT.jpg", "icon.jpg", "com.jpg", "nul_.jpg"] {
            assert!(!is_reserved_device_name(n), "{n}");
        }
        let (n, w) = sanitize_file_name("con", ".jpg", "x");
        assert_eq!(n, "con_.jpg");
        assert_eq!(w.len(), 1);
        let (n, _) = sanitize_file_name("NUL.tar", ".jpg", "x");
        assert_eq!(n, "NUL_.tar.jpg");
        let (n, _) = sanitize_file_name("a<b>c:d\"e/f\\g|h?i*j", ".png", "x");
        assert_eq!(n, "a_b_c_d_e_f_g_h_i_j.png");
        let (n, _) = sanitize_file_name("tab\there", ".png", "x");
        assert_eq!(n, "tab_here.png");
        let (n, w) = sanitize_file_name("name. . ", "", "x");
        assert_eq!(n, "name");
        assert!(!w.is_empty());
        let (n, _) = sanitize_file_name(" . ", ".jpg", "orig");
        assert_eq!(n, "orig.jpg");
        let long = "é".repeat(300);
        let (n, w) = sanitize_file_name(&long, ".jpeg", "x");
        assert_eq!(utf16_len(&n), 255);
        assert!(n.ends_with(".jpeg"));
        assert!(w.iter().any(|w| w.contains("shortened")));
        // surrogate pairs count as 2 UTF-16 units
        let emoji = "📷".repeat(200);
        let (n, _) = sanitize_file_name(&emoji, ".jpg", "x");
        assert!(utf16_len(&n) <= 255);
        assert!(windows_name_problems("ok.jpg").is_empty());
        assert_eq!(windows_name_problems("bad?.jpg").len(), 1);
        assert_eq!(windows_name_problems("PRN.jpg").len(), 1);
        assert_eq!(windows_name_problems("x.").len(), 1);
        // whatever the rules do, the result is a single path component
        let evil = [r"..", r"..\..", "a/b", "C:", "\\\\server\\share", ".", "CON", ""];
        for e in evil {
            let (n, _) = sanitize_file_name(e, ".jpg", "orig");
            let comps: Vec<_> = Path::new(&n).components().collect();
            assert_eq!(comps.len(), 1, "{e} -> {n}");
            assert!(matches!(comps[0], std::path::Component::Normal(_)), "{e} -> {n}");
            assert!(windows_name_problems(&n).is_empty(), "{e} -> {n}");
        }
    }

    fn all_rule_samples() -> Vec<RuleSpec> {
        let mut v: Vec<RuleSpec> =
            RULE_TYPES.iter().map(|k| RuleSpec::new(RuleKind::new_default(k).unwrap())).collect();
        v.push(RuleSpec { enabled: false, kind: replace(" |odd%\"; #[x]=y ", "  a|b ", true, true, Occurrence::Last) });
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::Chars { chars: "|%()".into(), case_sensitive: true })));
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::Range { from: 2, to: 5 })));
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::Last(3))));
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::Whitespace)));
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::Digits)));
        v.push(RuleSpec::new(RuleKind::AddText(AddTextRule {
            text: "ünï cødé ".into(),
            position: Position::FromEnd(2),
        })));
        v.push(RuleSpec::new(RuleKind::Numbering(NumberingRule {
            start: -5,
            step: 10,
            padding: 0,
            position: Position::At(3),
            separator: "".into(),
            per_folder: false,
            repeat: 3,
            reset_every: 10,
            system: NumberSystem::Roman,
        })));
        v.push(RuleSpec::new(RuleKind::Case(CaseMode::Invert)));
        v.push(RuleSpec::new(RuleKind::Clean(CleanRule {
            trim: false,
            collapse_spaces: false,
            underscores_to_spaces: true,
            spaces_to_underscores: true,
            dots_to_spaces: true,
            remove_illegal: false,
            collapse_repeats: false,
            strip_brackets: true,
            normalize_spaces: false,
            strip_emoji: true,
            strip_marks: true,
            space_before_capitals: true,
        })));
        v.push(RuleSpec::new(RuleKind::Date(DateRule {
            source: Some(SourceKind::Exif),
            format: "%Y|%m %%".into(),
            position: Position::Replace,
            separator: " | ".into(),
        })));
        v.push(RuleSpec::new(RuleKind::ParentFolder(ParentRule { position: Position::Suffix, separator: "".into() })));
        v.push(RuleSpec::new(RuleKind::ExtensionCase(ExtCase::Upper)));
        v.push(RuleSpec::new(RuleKind::Remove(RemoveMode::First(2))));
        v.push(RuleSpec::new(RuleKind::AddText(AddTextRule {
            text: "x".into(),
            position: Position::AfterText(" | a=b ".into()),
        })));
        v.push(RuleSpec::new(RuleKind::AddText(AddTextRule {
            text: "x".into(),
            position: Position::BeforeText("%".into()),
        })));
        v.push(RuleSpec::new(RuleKind::Delete(DeleteRule {
            from: DeleteFrom::Delimiter("(".into()),
            until: DeleteUntil::Delimiter(")".into()),
            right_to_left: true,
            keep_delimiters: true,
        })));
        v.push(RuleSpec::new(RuleKind::Delete(DeleteRule {
            from: DeleteFrom::Position(3),
            until: DeleteUntil::End,
            right_to_left: false,
            keep_delimiters: false,
        })));
        v.push(RuleSpec::new(RuleKind::Strip(StripRule {
            english: true,
            digits: false,
            symbols: true,
            brackets: true,
            user: "äÖ|".into(),
            except: true,
            case_sensitive: true,
        })));
        v.push(RuleSpec::new(RuleKind::Rearrange(RearrangeRule {
            split: "3|7".into(),
            by_positions: true,
            pattern: "$-1 $1".into(),
        })));
        v.push(RuleSpec::new(RuleKind::Padding(PaddingRule {
            zero_pad: 0,
            remove_zero_padding: true,
            text_pad_len: 20,
            pad_chars: "-=".into(),
            pad_left: true,
        })));
        v.push(RuleSpec::new(RuleKind::Randomize(RandomizeRule {
            length: 8,
            digits: false,
            lower: false,
            upper: true,
            user: "#".into(),
            unique: false,
            position: Position::At(2),
            seed: 42,
        })));
        v.push(RuleSpec::new(RuleKind::Translit));
        v.push(RuleSpec::new(RuleKind::ReformatDate(ReformatDateRule {
            find: "%d.%m.%Y|%b %Y".into(),
            to: "%Y-%m".into(),
            whole_words: false,
        })));
        v.push(RuleSpec::new(RuleKind::Extension(ExtensionRule::Set("png".into()))));
        v.push(RuleSpec::new(RuleKind::Extension(ExtensionRule::Detect)));
        v
    }

    #[test]
    fn ini_value_roundtrip() {
        for r in all_rule_samples() {
            let s = r.to_ini_value();
            assert!(!s.contains('\n') && !s.contains('"'), "{s}");
            assert_eq!(RuleSpec::from_ini_value(&s), Some(r.clone()), "{s}");
        }
        assert_eq!(decode_value(&encode_value(" %|\"\n\t x ")), " %|\"\n\t x ");
        assert_eq!(decode_value("%zz%4"), "%zz%4");
        assert_eq!(decode_value("%C3%A9"), "é");
        assert!(RuleSpec::from_ini_value("teleport | on | to=mars").is_none());
        assert!(RuleSpec::from_ini_value("").is_none());
        // lenient: missing keys -> defaults; garbage keys ignored
        let r = RuleSpec::from_ini_value("NUMBER|off|pad=abc|bogus").unwrap();
        assert!(!r.enabled);
        assert_eq!(r.kind, RuleKind::new_default("number").unwrap());
        let r = RuleSpec::from_ini_value("number | on | pad=999").unwrap();
        assert!(matches!(r.kind, RuleKind::Numbering(NumberingRule { padding: 32, .. })));
    }

    #[test]
    fn summaries_and_positions() {
        for r in all_rule_samples() {
            assert!(!r.kind.summary().is_empty());
            assert_ne!(r.kind.type_label(), "?");
        }
        for p in [
            Position::Prefix,
            Position::Suffix,
            Position::At(3),
            Position::FromEnd(2),
            Position::Replace,
            Position::AfterText("Ab c".into()),
            Position::BeforeText(" x".into()),
        ] {
            assert_eq!(Position::from_key(&p.key()), Some(p));
        }
        assert_eq!(Position::from_key("at:x"), None);
        assert!(RuleKind::new_default("nope").is_none());
    }

    #[test]
    fn insert_after_before_text() {
        let add = |t: &str, p: Position| RuleKind::AddText(AddTextRule { text: t.into(), position: p });
        assert_eq!(run("IMG_1234.jpg", &[add("Rome_", Position::AfterText("IMG_".into()))]), "IMG_Rome_1234.jpg");
        assert_eq!(run("IMG_1234.jpg", &[add("-", Position::BeforeText("34".into()))]), "IMG_12-34.jpg");
        assert_eq!(run("IMG_1234.jpg", &[add("x", Position::AfterText("zzz".into()))]), "IMG_1234.jpg");
    }

    #[test]
    fn remove_text_occurrences() {
        let rm =
            |t: &str, o, cs| RuleKind::Remove(RemoveMode::Text { text: t.into(), occurrence: o, case_sensitive: cs });
        assert_eq!(run("a-copy-Copy.jpg", &[rm("-copy", Occurrence::All, false)]), "a.jpg");
        assert_eq!(run("a-copy-Copy.jpg", &[rm("-copy", Occurrence::All, true)]), "a-Copy.jpg");
        assert_eq!(run("x1x2x.jpg", &[rm("x", Occurrence::First, false)]), "1x2x.jpg");
        assert_eq!(run("x1x2x.jpg", &[rm("x", Occurrence::Last, false)]), "x1x2.jpg");
        assert_eq!(run("a.b(1).jpg", &[rm(".b(1)", Occurrence::All, false)]), "a.jpg");
    }

    #[test]
    fn delete_rule() {
        let del = |from, until, rtl, keep| {
            RuleKind::Delete(DeleteRule { from, until, right_to_left: rtl, keep_delimiters: keep })
        };
        use DeleteFrom as F;
        use DeleteUntil as U;
        assert_eq!(run("IMG_1234.jpg", &[del(F::Position(1), U::Count(4), false, false)]), "1234.jpg");
        assert_eq!(run("IMG_1234.jpg", &[del(F::Position(4), U::End, false, false)]), "IMG.jpg");
        // keep only the last four characters (ReNamer doc example)
        assert_eq!(run("Holiday_1234.jpg", &[del(F::Position(5), U::End, true, false)]), "1234.jpg");
        assert_eq!(
            run("pic (copy) 2.jpg", &[del(F::Delimiter(" (".into()), U::Delimiter(")".into()), false, false)]),
            "pic 2.jpg"
        );
        assert_eq!(
            run("a[x]b.jpg", &[del(F::Delimiter("[".into()), U::Delimiter("]".into()), false, true)]),
            "a[]b.jpg"
        );
        // right-to-left: from the last "_" "till the end" = everything to its left
        assert_eq!(run("a_b_c.jpg", &[del(F::Delimiter("_".into()), U::End, true, false)]), "c.jpg");
        assert_eq!(run("abc.jpg", &[del(F::Delimiter("z".into()), U::End, false, false)]), "abc.jpg");
        assert_eq!(run("abc.jpg", &[del(F::Position(9), U::End, false, false)]), "abc.jpg");
        assert!(del(F::Position(0), U::End, false, false).validate().is_err());
        assert!(del(F::Delimiter("".into()), U::End, false, false).validate().is_err());
    }

    #[test]
    fn strip_rule() {
        let st = |english, digits, symbols, brackets, user: &str, except| {
            RuleKind::Strip(StripRule {
                english,
                digits,
                symbols,
                brackets,
                user: user.into(),
                except,
                case_sensitive: false,
            })
        };
        assert_eq!(run("IMG_2024 (1).jpg", &[st(false, true, false, false, "", false)]), "IMG_ ().jpg");
        assert_eq!(run("a!b#c(d).jpg", &[st(false, false, true, true, "", false)]), "abcd.jpg");
        assert_eq!(run("Ab1-2.jpg", &[st(false, true, false, false, "", true)]), "12.jpg");
        assert_eq!(run("XaYa.jpg", &[st(false, false, false, false, "A", false)]), "XY.jpg");
        assert_eq!(run("Ünïcode.jpg", &[st(true, false, false, false, "", false)]), "Üï.jpg");
    }

    #[test]
    fn rearrange_rule() {
        let ra = |split: &str, bypos, pat: &str| {
            RuleKind::Rearrange(RearrangeRule { split: split.into(), by_positions: bypos, pattern: pat.into() })
        };
        assert_eq!(run("Artist - Title.jpg", &[ra(" - ", false, "$2 - $1")]), "Title - Artist.jpg");
        assert_eq!(run("a_b-c.jpg", &[ra("_|-", false, "$-1$2$1 [$0]")]), "cba [a_b-c].jpg");
        assert_eq!(run("20240315.jpg", &[ra("5|7", true, "$3.$2.$1")]), "15.03.2024.jpg");
        assert_eq!(run("a.jpg", &[ra("_", false, "$1$2$9")]), "a.jpg");
        assert_eq!(fill_pattern("$$1 $x $", "w", &["p".into()]), "$p $x $");
    }

    #[test]
    fn serialize_options() {
        let ser = |repeat, reset_every, system, per_folder| {
            RuleKind::Numbering(NumberingRule {
                start: 1,
                step: 1,
                padding: 2,
                position: Position::Replace,
                separator: String::new(),
                per_folder,
                repeat,
                reset_every,
                system,
            })
        };
        let seq =
            |r: RuleKind| (0..7).map(|i| run_at("a.jpg", &[r.clone()], i, 0).0.replace(".jpg", "")).collect::<Vec<_>>();
        assert_eq!(seq(ser(2, 0, NumberSystem::Decimal, false)), ["01", "01", "02", "02", "03", "03", "04"]);
        assert_eq!(seq(ser(1, 3, NumberSystem::Decimal, false)), ["01", "02", "03", "01", "02", "03", "01"]);
        assert_eq!(seq(ser(1, 0, NumberSystem::Roman, false)), ["I", "II", "III", "IV", "V", "VI", "VII"]);
        assert_eq!(seq(ser(1, 0, NumberSystem::LettersLower, false)), ["a", "b", "c", "d", "e", "f", "g"]);
        assert_eq!(to_letters(26).unwrap(), "Z");
        assert_eq!(to_letters(27).unwrap(), "AA");
        assert_eq!(to_letters(702).unwrap(), "ZZ");
        assert_eq!(to_letters(0), None);
        assert_eq!(to_roman(1994).unwrap(), "MCMXCIV");
        assert_eq!(to_roman(4000), None);
        assert!(ser(0, 0, NumberSystem::Decimal, false).validate().is_err());
    }

    #[test]
    fn padding_rule() {
        let pad = |zero_pad, unpad, len, chars: &str, left| {
            RuleKind::Padding(PaddingRule {
                zero_pad,
                remove_zero_padding: unpad,
                text_pad_len: len,
                pad_chars: chars.into(),
                pad_left: left,
            })
        };
        assert_eq!(run("track 7 of 12.jpg", &[pad(3, false, 0, "", false)]), "track 007 of 012.jpg");
        assert_eq!(run("img 0007 v0.jpg", &[pad(0, true, 0, "", false)]), "img 7 v0.jpg");
        assert_eq!(run("ab.jpg", &[pad(0, false, 5, "-=", true)]), "-=-ab.jpg");
        assert_eq!(run("ab.jpg", &[pad(0, false, 4, "_", false)]), "ab__.jpg");
        assert_eq!(run("abcdef.jpg", &[pad(0, false, 4, "_", false)]), "abcdef.jpg");
    }

    #[test]
    fn clean_up_extras() {
        let c = CleanRule {
            strip_brackets: true,
            strip_emoji: true,
            strip_marks: true,
            space_before_capitals: true,
            ..CleanRule::default()
        };
        assert_eq!(run("HolidayInRome (copy) [1] {x} 😀 Café.jpg", &[RuleKind::Clean(c)]), "Holiday In Rome Cafe.jpg");
        assert_eq!(run("a\u{a0}b\u{2009}c.jpg", &[RuleKind::Clean(CleanRule::default())]), "a b c.jpg");
    }

    #[test]
    fn translit_rule() {
        assert_eq!(run("Straße Ærø Łódź.jpg", &[RuleKind::Translit]), "Strasse AEro Lodz.jpg");
        assert_eq!(
            run("Москва Санкт-Петербург Йошкар-Ола.jpg", &[RuleKind::Translit]),
            "Moskva Sankt-Peterburg Yoshkar-Ola.jpg"
        );
        assert_eq!(run("Αθήνα.jpg", &[RuleKind::Translit]), "Athina.jpg");
        assert_eq!(run("plain 123.jpg", &[RuleKind::Translit]), "plain 123.jpg");
    }

    #[test]
    fn reformat_date_rule() {
        let rd = |find: &str, to: &str, whole| {
            RuleKind::ReformatDate(ReformatDateRule { find: find.into(), to: to.into(), whole_words: whole })
        };
        assert_eq!(run("Party 15.03.2024.jpg", &[rd("%d.%m.%Y", "%Y-%m-%d", true)]), "Party 2024-03-15.jpg");
        assert_eq!(run("x 31.02.2024.jpg", &[rd("%d.%m.%Y", "%Y-%m-%d", true)]), "x 31.02.2024.jpg"); // invalid date kept
        assert_eq!(run("IMG_20240315_1.jpg", &[rd("%Y%m%d", "%d %B %Y", false)]), "IMG_15 March 2024_1.jpg");
        assert_eq!(run("IMG20240315x.jpg", &[rd("%Y%m%d", "%Y", true)]), "IMG20240315x.jpg"); // not a whole word
        assert_eq!(
            run("Trip Mar 2024 and Apr 2024.jpg", &[rd("%b %Y", "%Y-%m", true)]),
            "Trip 2024-03 and 2024-04.jpg"
        );
        assert_eq!(
            run("a 15-03-24 b 2024.01.02.jpg", &[rd("%d-%m-%y|%Y.%m.%d", "%Y%m%d", true)]),
            "a 20240315 b 20240102.jpg"
        );
        assert_eq!(
            run("at 10.22.01 on 2024-03-15.jpg", &[rd("%Y-%m-%d", "%d/%m/%Y", true)]),
            "at 10.22.01 on 15-03-2024.jpg"
        );
        assert!(rd("%d.%m", "%Y", true).validate().is_err()); // no year
        assert!(rd("%Y %Q %m", "%Y", true).validate().is_err());
        assert!(rd("%Y%m%d", "%Q", true).validate().is_err());
    }

    #[test]
    fn randomize_rule_is_deterministic_and_unique() {
        let rr = |unique| RandomizeRule {
            length: 2,
            digits: true,
            lower: false,
            upper: false,
            user: String::new(),
            unique,
            position: Position::Replace,
            seed: 7,
        };
        let a = run_at("a.jpg", &[RuleKind::Randomize(rr(false))], 3, 0).0;
        let b = run_at("b.jpg", &[RuleKind::Randomize(rr(false))], 3, 0).0;
        assert_eq!(a, b);
        assert_eq!(a.len(), 6);
        // unique within one compiled chain
        let compiled = compile_rules(&[RuleSpec::new(RuleKind::Randomize(rr(true)))]).unwrap();
        let e = env("x.jpg");
        let mut seen = std::collections::HashSet::new();
        for i in 0..90 {
            let ctx = RuleContext {
                source: &e.source,
                current_dir: &e.cur,
                base_dir: &e.base,
                date: None,
                date_config: &e.cfg,
                index: i,
                folder_index: i,
            };
            let mut p = NameParts { stem: "x".into(), ext: ".jpg".into(), warnings: vec![] };
            compiled[0].apply(&mut p, &ctx);
            assert!(seen.insert(p.stem.clone()), "duplicate {}", p.stem);
        }
        let none = RandomizeRule { digits: false, lower: false, upper: false, ..rr(true) };
        assert!(RuleKind::Randomize(none).validate().is_err());
    }

    #[test]
    fn extension_rule_set_and_detect() {
        assert_eq!(run("a.jpeg", &[RuleKind::Extension(ExtensionRule::Set("jpg".into()))]), "a.jpg");
        assert_eq!(run("a.JPEG", &[RuleKind::Extension(ExtensionRule::Set(".jpg".into()))]), "a.JPG");
        assert!(RuleKind::Extension(ExtensionRule::Set("txt".into())).validate().is_err());
        assert!(RuleKind::Extension(ExtensionRule::Set("exe".into())).validate().is_err());
        let dir = tempfile::tempdir().unwrap();
        let cases: [(&str, &[u8], &str); 6] = [
            ("png_really.jpg", b"\x89PNG\r\n\x1a\n0000000000000000", "png_really.png"),
            ("ok.jpeg", b"\xFF\xD8\xFF\xE0000000000000000", "ok.jpeg"),
            ("heic.jpg", b"\0\0\0\x18ftypheic00000000000000", "heic.heic"),
            ("clip.mov", b"\0\0\0\x18ftypqt  0000000000000000", "clip.mov"),
            ("clip.avi", b"\0\0\0\x18ftypisom0000000000000000", "clip.mp4"),
            ("raw.cr2", b"II*\0\x10\0\0\0CR000000000000000", "raw.cr2"),
        ];
        let cfg = DateConfig::default();
        let compiled = compile_rules(&[RuleSpec::new(RuleKind::Extension(ExtensionRule::Detect))]).unwrap();
        for (name, bytes, want) in cases {
            let p = dir.path().join(name);
            std::fs::write(&p, bytes).unwrap();
            let ctx = RuleContext {
                source: &p,
                current_dir: dir.path(),
                base_dir: dir.path(),
                date: None,
                date_config: &cfg,
                index: 0,
                folder_index: 0,
            };
            let (stem, ext) = crate::plan::split_name(name);
            let mut np = NameParts { stem: stem.into(), ext: ext.into(), warnings: vec![] };
            compiled[0].apply(&mut np, &ctx);
            assert_eq!(format!("{}{}", np.stem, np.ext), want, "{name}");
        }
        // unknown content: kept, with a note
        let p = dir.path().join("mystery.jpg");
        std::fs::write(&p, b"hello world, not an image").unwrap();
        let ctx = RuleContext {
            source: &p,
            current_dir: dir.path(),
            base_dir: dir.path(),
            date: None,
            date_config: &cfg,
            index: 0,
            folder_index: 0,
        };
        let mut np = NameParts { stem: "mystery".into(), ext: ".jpg".into(), warnings: vec![] };
        compiled[0].apply(&mut np, &ctx);
        assert_eq!(np.ext, ".jpg");
        assert_eq!(np.warnings.len(), 1);
    }

    #[test]
    fn regex_type_key_defaults_to_regex_mode() {
        let r = RuleSpec::from_ini_value("regex | on | find=^(IMG%7CVID)_ | with=").unwrap();
        match &r.kind {
            RuleKind::Replace(x) => assert!(x.regex),
            k => panic!("{k:?}"),
        }
        assert_eq!(r.kind.type_label(), "Regular expressions");
        assert_eq!(run("IMG_0001.jpg", &[r.kind.clone()]), "0001.jpg");
        let plain = RuleSpec::from_ini_value("replace | on | find=a.c").unwrap();
        assert_eq!(plain.kind.type_label(), "Replace");
    }
}
