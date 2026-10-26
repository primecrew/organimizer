//! Choosing which date to use for a file.
//!
//! Sources: file name (smart parsing), EXIF `DateTimeOriginal` (JPEG, TIFF,
//! HEIC/HEIF, PNG, WebP via the pure-Rust `kamadak-exif` crate), file
//! modified time and file creation time (both local time).

use std::fs::Metadata;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};

use crate::filename_date::{DateOrder, parse_filename_date};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKind {
    Filename,
    Exif,
    Modified,
    Created,
}

impl SourceKind {
    pub const ALL: [SourceKind; 4] =
        [SourceKind::Filename, SourceKind::Exif, SourceKind::Modified, SourceKind::Created];

    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Filename => "Filename",
            SourceKind::Exif => "EXIF",
            SourceKind::Modified => "Modified",
            SourceKind::Created => "Created",
        }
    }
    /// Key used in the INI file.
    pub fn key(self) -> &'static str {
        match self {
            SourceKind::Filename => "filename",
            SourceKind::Exif => "exif",
            SourceKind::Modified => "modified",
            SourceKind::Created => "created",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "filename" | "name" => Some(SourceKind::Filename),
            "exif" => Some(SourceKind::Exif),
            "modified" | "mtime" => Some(SourceKind::Modified),
            "created" | "ctime" | "creation" => Some(SourceKind::Created),
            _ => None,
        }
    }
}

/// Which date source the user selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateMode {
    /// Try `DateConfig::auto_order` in order, first hit wins.
    Auto,
    /// Use exactly one source (then the fallback if it yields nothing).
    Only(SourceKind),
}

impl DateMode {
    pub fn key(self) -> &'static str {
        match self {
            DateMode::Auto => "auto",
            DateMode::Only(k) => k.key(),
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        if s.trim().eq_ignore_ascii_case("auto") {
            return Some(DateMode::Auto);
        }
        SourceKind::from_key(s).map(DateMode::Only)
    }
    pub fn label(self) -> &'static str {
        match self {
            DateMode::Auto => "Auto (priority order)",
            DateMode::Only(SourceKind::Filename) => "Detect from filename",
            DateMode::Only(SourceKind::Exif) => "EXIF / video date taken",
            DateMode::Only(SourceKind::Modified) => "Modified date",
            DateMode::Only(SourceKind::Created) => "Created date",
        }
    }
}

/// What to do when the selected source finds no date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    Modified,
    Created,
    Skip,
}

impl Fallback {
    pub fn key(self) -> &'static str {
        match self {
            Fallback::Modified => "modified",
            Fallback::Created => "created",
            Fallback::Skip => "skip",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "modified" => Some(Fallback::Modified),
            "created" => Some(Fallback::Created),
            "skip" => Some(Fallback::Skip),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Fallback::Modified => "Use modified date",
            Fallback::Created => "Use created date",
            Fallback::Skip => "Skip the file",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DateConfig {
    pub mode: DateMode,
    pub auto_order: Vec<SourceKind>,
    pub fallback: Fallback,
    pub order: DateOrder,
}

impl Default for DateConfig {
    fn default() -> Self {
        DateConfig {
            mode: DateMode::Auto,
            auto_order: SourceKind::ALL.to_vec(),
            fallback: Fallback::Modified,
            order: DateOrder::Us,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedDate {
    pub date: NaiveDate,
    pub datetime: Option<NaiveDateTime>,
    pub source: SourceKind,
    /// True when the date came from the fallback rule.
    pub via_fallback: bool,
}

fn system_time_local(t: SystemTime) -> NaiveDateTime {
    let dt: DateTime<Local> = t.into();
    dt.naive_local()
}

/// Read EXIF DateTimeOriginal (or DateTimeDigitized / DateTime).
pub fn exif_date(path: &Path) -> Option<NaiveDateTime> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized, exif::Tag::DateTime] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY)
            && let exif::Value::Ascii(ref v) = field.value
            && let Some(bytes) = v.first()
            && let Ok(dt) = exif::DateTime::from_ascii(bytes)
        {
            let date = NaiveDate::from_ymd_opt(dt.year as i32, dt.month as u32, dt.day as u32)?;
            use chrono::Datelike;
            if date.year() < crate::MIN_YEAR || date.year() > crate::max_year() {
                continue;
            }
            let t =
                chrono::NaiveTime::from_hms_opt(dt.hour as u32, dt.minute as u32, dt.second as u32).unwrap_or_default();
            return Some(date.and_time(t));
        }
    }
    None
}

/// Creation time from the `moov/mvhd` box of MP4 / MOV / M4V / 3GP files
/// (seconds since 1904-01-01 UTC, converted to local time). Read-only; only
/// box headers are read, never the media data.
pub fn video_date(path: &Path) -> Option<NaiveDateTime> {
    use std::io::{Read, Seek, SeekFrom};
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if !matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "3gp" | "3g2") {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len();

    // Find box `want` among the boxes in [start, end). Returns (payload start, payload end).
    fn find_box(f: &mut std::fs::File, start: u64, end: u64, want: &[u8; 4]) -> Option<(u64, u64)> {
        let mut pos = start;
        let mut guard = 0;
        while pos + 8 <= end && guard < 10_000 {
            guard += 1;
            f.seek(SeekFrom::Start(pos)).ok()?;
            let mut h = [0u8; 8];
            f.read_exact(&mut h).ok()?;
            let mut size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
            let mut header = 8;
            if size == 1 {
                let mut l = [0u8; 8];
                f.read_exact(&mut l).ok()?;
                size = u64::from_be_bytes(l);
                header = 16;
            } else if size == 0 {
                size = end - pos;
            }
            if size < header {
                return None;
            }
            if &h[4..8] == want {
                return Some((pos + header, (pos + size).min(end)));
            }
            pos = pos.checked_add(size)?;
        }
        None
    }

    let (ms, me) = find_box(&mut f, 0, file_len, b"moov")?;
    let (hs, _) = find_box(&mut f, ms, me, b"mvhd")?;
    f.seek(SeekFrom::Start(hs)).ok()?;
    let mut vf = [0u8; 4];
    f.read_exact(&mut vf).ok()?;
    let secs = if vf[0] == 1 {
        let mut b = [0u8; 8];
        f.read_exact(&mut b).ok()?;
        u64::from_be_bytes(b)
    } else {
        let mut b = [0u8; 4];
        f.read_exact(&mut b).ok()?;
        u32::from_be_bytes(b) as u64
    };
    const MAC_TO_UNIX: u64 = 2_082_844_800;
    if secs <= MAC_TO_UNIX {
        return None; // 0 = not set
    }
    let utc = DateTime::from_timestamp((secs - MAC_TO_UNIX) as i64, 0)?;
    let local = utc.with_timezone(&Local).naive_local();
    use chrono::Datelike;
    if local.year() < crate::MIN_YEAR || local.year() > crate::max_year() {
        return None;
    }
    Some(local)
}

/// Try a single source.
pub fn try_source(
    kind: SourceKind,
    path: &Path,
    meta: Option<&Metadata>,
    order: DateOrder,
) -> Option<(NaiveDate, Option<NaiveDateTime>)> {
    match kind {
        SourceKind::Filename => {
            let name = path.file_name()?.to_string_lossy();
            parse_filename_date(&name, order).map(|f| (f.date, f.datetime))
        }
        SourceKind::Exif => exif_date(path).or_else(|| video_date(path)).map(|dt| (dt.date(), Some(dt))),
        SourceKind::Modified => meta?.modified().ok().map(system_time_local).map(|dt| (dt.date(), Some(dt))),
        SourceKind::Created => meta?.created().ok().map(system_time_local).map(|dt| (dt.date(), Some(dt))),
    }
}

/// Resolve the date for one file according to `cfg`.
pub fn resolve_date(path: &Path, meta: Option<&Metadata>, cfg: &DateConfig) -> Option<ResolvedDate> {
    let primary: Vec<SourceKind> = match cfg.mode {
        DateMode::Auto => {
            if cfg.auto_order.is_empty() {
                SourceKind::ALL.to_vec()
            } else {
                cfg.auto_order.clone()
            }
        }
        DateMode::Only(k) => vec![k],
    };
    for kind in &primary {
        if let Some((date, datetime)) = try_source(*kind, path, meta, cfg.order) {
            return Some(ResolvedDate { date, datetime, source: *kind, via_fallback: false });
        }
    }
    let fb = match cfg.fallback {
        Fallback::Modified => SourceKind::Modified,
        Fallback::Created => SourceKind::Created,
        Fallback::Skip => return None,
    };
    if primary.contains(&fb) {
        // Already tried; maybe try the other metadata time as last resort.
        return None;
    }
    try_source(fb, path, meta, cfg.order).map(|(date, datetime)| ResolvedDate {
        date,
        datetime,
        source: fb,
        via_fallback: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn filename_beats_modified_in_auto() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("IMG_20200115_101010.jpg");
        fs::write(&p, b"not really a jpeg").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let r = resolve_date(&p, Some(&meta), &DateConfig::default()).unwrap();
        assert_eq!(r.source, SourceKind::Filename);
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2020, 1, 15).unwrap());
    }

    #[test]
    fn fallback_rules() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("holiday.jpg");
        fs::write(&p, b"x").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let mut cfg = DateConfig { mode: DateMode::Only(SourceKind::Filename), ..Default::default() };
        let r = resolve_date(&p, Some(&meta), &cfg).unwrap();
        assert_eq!(r.source, SourceKind::Modified);
        assert!(r.via_fallback);
        cfg.fallback = Fallback::Skip;
        assert!(resolve_date(&p, Some(&meta), &cfg).is_none());
    }

    #[test]
    fn modified_mode_uses_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("IMG_20200115.jpg");
        fs::write(&p, b"x").unwrap();
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000); // Sep 2020
        fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        let meta = fs::metadata(&p).unwrap();
        let cfg = DateConfig { mode: DateMode::Only(SourceKind::Modified), ..Default::default() };
        let r = resolve_date(&p, Some(&meta), &cfg).unwrap();
        assert_eq!(r.source, SourceKind::Modified);
        assert_eq!(r.date, system_time_local(t).date());
    }

    #[test]
    fn exif_date_taken() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exif_2019-07-04.jpg");
        let dt = exif_date(&p).expect("fixture has DateTimeOriginal");
        assert_eq!(dt.format("%Y-%m-%d %H:%M:%S").to_string(), "2019-07-04 18:30:00");
        let meta = fs::metadata(&p).unwrap();
        let r = resolve_date(&p, Some(&meta), &DateConfig::default()).unwrap();
        // "2019-07-04" in the file name is found first by the filename parser
        assert_eq!(r.source, SourceKind::Filename);
        let cfg = DateConfig { auto_order: vec![SourceKind::Exif, SourceKind::Modified], ..Default::default() };
        let r = resolve_date(&p, Some(&meta), &cfg).unwrap();
        assert_eq!(r.source, SourceKind::Exif);
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2019, 7, 4).unwrap());
        // non-image -> no EXIF
        assert!(exif_date(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).is_none());
    }

    #[test]
    fn mp4_creation_time() {
        // minimal ftyp + moov/mvhd (version 0), creation 2021-06-01 12:00:00 UTC
        let secs: u32 = (1_622_548_800u64 + 2_082_844_800) as u32;
        let mut mvhd = vec![0u8, 0, 0, 0];
        mvhd.extend(secs.to_be_bytes());
        mvhd.extend(secs.to_be_bytes());
        mvhd.extend([0u8; 88]);
        let boxed = |t: &[u8], p: &[u8]| {
            let mut v = ((p.len() + 8) as u32).to_be_bytes().to_vec();
            v.extend(t);
            v.extend(p);
            v
        };
        let mut file = boxed(b"ftyp", b"isom\0\0\0\0isom");
        file.extend(boxed(b"moov", &boxed(b"mvhd", &mvhd)));
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("clip.mp4");
        fs::write(&p, &file).unwrap();
        let dt = video_date(&p).unwrap();
        let expect = DateTime::from_timestamp(1_622_548_800, 0).unwrap().with_timezone(&Local).naive_local();
        assert_eq!(dt, expect);
        let r = try_source(SourceKind::Exif, &p, None, DateOrder::Us).unwrap();
        assert_eq!(r.0, expect.date());
        // garbage / truncated files never panic
        fs::write(&p, b"\0\0\0\x01moov").unwrap();
        assert!(video_date(&p).is_none());
        fs::write(&p, &file[..30]).unwrap();
        assert!(video_date(&p).is_none());
        // not a video extension -> not parsed
        let q = dir.path().join("clip.jpg");
        fs::write(&q, &file).unwrap();
        assert!(video_date(&q).is_none());
    }

    #[test]
    fn keys_roundtrip() {
        for k in SourceKind::ALL {
            assert_eq!(SourceKind::from_key(k.key()), Some(k));
            assert_eq!(DateMode::from_key(DateMode::Only(k).key()), Some(DateMode::Only(k)));
        }
        assert_eq!(DateMode::from_key("AUTO"), Some(DateMode::Auto));
        assert_eq!(Fallback::from_key("Skip"), Some(Fallback::Skip));
        assert_eq!(Fallback::from_key("bogus"), None);
    }
}
