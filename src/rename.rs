//! Renamer-style ordered rename rules.
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

/// Where inserted text goes, relative to the current stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Prefix,
    Suffix,
    /// After this many characters from the start (0 = prefix).
    At(usize),
    /// This many characters before the end (0 = suffix).
    FromEnd(usize),
    /// Replace the whole stem.
    Replace,
}

impl Position {
    pub fn key(self) -> String {
        match self {
            Position::Prefix => "prefix".into(),
            Position::Suffix => "suffix".into(),
            Position::At(n) => format!("at:{n}"),
            Position::FromEnd(n) => format!("end:{n}"),
            Position::Replace => "replace".into(),
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        let num = |p: &str| s.strip_prefix(p).and_then(|n| n.trim().parse::<usize>().ok());
        match s.as_str() {
            "prefix" | "start" => Some(Position::Prefix),
            "suffix" | "end" => Some(Position::Suffix),
            "replace" | "name" => Some(Position::Replace),
            _ => num("at:").map(Position::At).or_else(|| num("end:").map(Position::FromEnd)),
        }
    }
    pub fn describe(self) -> String {
        match self {
            Position::Prefix => "at the start".into(),
            Position::Suffix => "at the end".into(),
            Position::At(n) => format!("after {n} chars"),
            Position::FromEnd(n) => format!("{n} chars before the end"),
            Position::Replace => "as the whole name".into(),
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
    /// Restart the sequence in every source folder.
    pub per_folder: bool,
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
    pub const ALL: [CaseMode; 5] = [CaseMode::Lower, CaseMode::Upper, CaseMode::Title, CaseMode::Sentence, CaseMode::Invert];
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
    AddText(AddTextRule),
    Numbering(NumberingRule),
    Case(CaseMode),
    Clean(CleanRule),
    Date(DateRule),
    ParentFolder(ParentRule),
    ExtensionCase(ExtCase),
}

/// The rule types, in the order the "Add rule" menu lists them.
pub const RULE_TYPES: [&str; 9] =
    ["replace", "remove", "add", "number", "case", "clean", "date", "parent", "extcase"];

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
            "remove" => RuleKind::Remove(RemoveMode::First(0)),
            "add" => RuleKind::AddText(AddTextRule { text: String::new(), position: Position::Prefix }),
            "number" => RuleKind::Numbering(NumberingRule {
                start: 1,
                step: 1,
                padding: 3,
                position: Position::Suffix,
                separator: "_".into(),
                per_folder: true,
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
            RuleKind::AddText(_) => "add",
            RuleKind::Numbering(_) => "number",
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
            "remove" => "Remove",
            "add" => "Add text",
            "number" => "Add index / sequence",
            "case" => "Change case",
            "clean" => "Trim / clean",
            "date" => "Insert date",
            "parent" => "Insert parent folder name",
            "extcase" => "Extension case",
            _ => "?",
        }
    }

    pub fn type_label(&self) -> &'static str {
        Self::type_label_for(self.type_key())
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
                RemoveMode::Chars { chars, .. } => format!("Remove characters “{chars}”"),
                RemoveMode::First(n) => format!("Remove first {n} characters"),
                RemoveMode::Last(n) => format!("Remove last {n} characters"),
                RemoveMode::Range { from, to } => format!("Remove characters {from} to {to}"),
                RemoveMode::Whitespace => "Remove all whitespace".into(),
                RemoveMode::Digits => "Remove all digits".into(),
            },
            RuleKind::AddText(a) => format!("Add “{}” {}", a.text, a.position.describe()),
            RuleKind::Numbering(n) => format!(
                "Add number {} (step {}, {} digits) {}{}",
                n.start,
                n.step,
                n.padding.max(1),
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
                Err("Range must satisfy 1 ≤ from ≤ to".into())
            }
            RuleKind::Numbering(n) if n.padding > 32 => Err("Padding must be at most 32 digits".into()),
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
            RuleKind::Remove(m) => Box::new(m.clone()),
            RuleKind::AddText(a) => Box::new(a.clone()),
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
pub fn insert_at(stem: &str, value: &str, pos: Position, sep: &str) -> String {
    match pos {
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
    }
}

fn build_replace_regex(r: &ReplaceRule) -> Result<Regex, String> {
    let pat = if r.regex { r.find.clone() } else { regex::escape(&r.find) };
    RegexBuilder::new(&pat)
        .case_insensitive(!r.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| match e {
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
        name.stem = insert_at(&name.stem, &self.text, self.position, "");
    }
}

/// Format a sequence number with zero padding (sign kept in front).
pub fn format_number(n: i64, padding: usize) -> String {
    let digits = format!("{:0>width$}", n.unsigned_abs(), width = padding.clamp(1, 32));
    if n < 0 { format!("-{digits}") } else { digits }
}

impl RenameRule for NumberingRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let i = if self.per_folder { ctx.folder_index } else { ctx.index } as i64;
        let n = self.start.saturating_add(self.step.saturating_mul(i));
        name.stem = insert_at(&name.stem, &format_number(n, self.padding), self.position, &self.separator);
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
            name.stem = insert_at(&name.stem, &text, self.position, &self.separator);
        }
    }
}

impl RenameRule for ParentRule {
    fn apply(&self, name: &mut NameParts, ctx: &RuleContext) {
        let Some(parent) = ctx.base_dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            name.warnings.push("Insert parent folder: the file is in a drive root, nothing inserted".into());
            return;
        };
        name.stem = insert_at(&name.stem, &parent, self.position, &self.separator);
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
            let (pre, num) = if base.len() > 3 { base.split_at(3) } else { return false };
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
            }
            RuleKind::Case(c) => kv("mode", c.key()),
            RuleKind::Clean(c) => {
                kv("trim", b(c.trim));
                kv("spaces", b(c.collapse_spaces));
                kv("us2sp", b(c.underscores_to_spaces));
                kv("sp2us", b(c.spaces_to_underscores));
                kv("dot2sp", b(c.dots_to_spaces));
                kv("illegal", b(c.remove_illegal));
                kv("repeats", b(c.collapse_repeats));
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
                r.regex = flag("regex", false);
                r.case_sensitive = flag("case", false);
                r.occurrence = get("occ").and_then(Occurrence::from_key).unwrap_or(Occurrence::All);
            }
            RuleKind::Remove(m) => {
                *m = match get("mode").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
                    Some("chars") => RemoveMode::Chars {
                        chars: get("chars").unwrap_or_default().to_string(),
                        case_sensitive: flag("case", false),
                    },
                    Some("last") => RemoveMode::Last(num("n", 0)),
                    Some("range") => RemoveMode::Range { from: num("from", 1), to: num("to", 1) },
                    Some("whitespace") => RemoveMode::Whitespace,
                    Some("digits") => RemoveMode::Digits,
                    _ => RemoveMode::First(num("n", 0)),
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
