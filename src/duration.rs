//! Media duration from the container header (pure Rust, read-only, small
//! reads). Used by the TMDB lookup (part of the "lookup" feature) to tell a
//! main movie from extras and to show runtimes in the match window.
//!
//! Supported: MP4 / MOV / M4V / 3GP (`moov/mvhd`), Matroska / WebM
//! (`Segment/Info/Duration`) and AVI (`avih`). Anything else, or a damaged
//! file, gives `None`; callers treat that as "unknown".

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Duration in whole seconds, if the header says so.
pub fn media_duration(path: &Path) -> Option<u64> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let f = File::open(path).ok()?;
    let mut r = BufReader::with_capacity(16 * 1024, f);
    match ext.as_str() {
        "mp4" | "m4v" | "mov" | "3gp" | "3g2" => mp4_duration(&mut r),
        "mkv" | "webm" | "mk3d" => mkv_duration(&mut r),
        "avi" => avi_duration(&mut r),
        _ => mp4_duration(&mut r).or_else(|| {
            r.seek(SeekFrom::Start(0)).ok()?;
            mkv_duration(&mut r)
        }),
    }
    .filter(|s| *s > 0 && *s < 100 * 3600)
}

/// "1:57:03" / "46 min" / "2:31" style label.
pub fn label(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn read_u32<R: Read>(r: &mut R) -> Option<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b).ok()?;
    Some(u32::from_be_bytes(b))
}

fn read_u64<R: Read>(r: &mut R) -> Option<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b).ok()?;
    Some(u64::from_be_bytes(b))
}

/// Walk MP4 boxes from the current position up to `end`; return (type,
/// payload start, payload end) of the first box named `want`.
fn find_box<R: Read + Seek>(r: &mut R, end: u64, want: &[u8; 4]) -> Option<(u64, u64)> {
    let mut pos = r.stream_position().ok()?;
    for _ in 0..10_000 {
        if pos + 8 > end {
            return None;
        }
        r.seek(SeekFrom::Start(pos)).ok()?;
        let size32 = read_u32(r)? as u64;
        let mut kind = [0u8; 4];
        r.read_exact(&mut kind).ok()?;
        let (header, size) = match size32 {
            1 => (16, read_u64(r)?),
            0 => (8, end - pos),
            n => (8, n),
        };
        if size < header {
            return None;
        }
        if &kind == want {
            return Some((pos + header, (pos + size).min(end)));
        }
        pos = pos.checked_add(size)?;
    }
    None
}

fn mp4_duration<R: Read + Seek>(r: &mut R) -> Option<u64> {
    let end = r.seek(SeekFrom::End(0)).ok()?;
    r.seek(SeekFrom::Start(0)).ok()?;
    let (moov, moov_end) = find_box(r, end, b"moov")?;
    r.seek(SeekFrom::Start(moov)).ok()?;
    let (mvhd, _) = find_box(r, moov_end, b"mvhd")?;
    r.seek(SeekFrom::Start(mvhd)).ok()?;
    let vf = read_u32(r)?;
    let (scale, dur) = if vf >> 24 == 1 {
        read_u64(r)?;
        read_u64(r)?;
        (read_u32(r)?, read_u64(r)?)
    } else {
        read_u32(r)?;
        read_u32(r)?;
        (read_u32(r)?, read_u32(r)? as u64)
    };
    if scale == 0 || dur == u64::MAX || dur == u32::MAX as u64 {
        return None;
    }
    Some((dur as f64 / scale as f64).round() as u64)
}

/// EBML variable-length integer. `keep_marker` for element IDs.
fn vint<R: Read>(r: &mut R, keep_marker: bool) -> Option<(u64, usize, bool)> {
    let mut first = [0u8; 1];
    r.read_exact(&mut first).ok()?;
    let len = first[0].leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let mut v = if keep_marker { first[0] as u64 } else { (first[0] as u64) & ((1u64 << (8 - len)) - 1) };
    let mut all_ones = v == (1u64 << (8 - len)) - 1;
    for _ in 1..len {
        let mut b = [0u8; 1];
        r.read_exact(&mut b).ok()?;
        all_ones &= b[0] == 0xFF;
        v = (v << 8) | b[0] as u64;
    }
    Some((v, len, !keep_marker && all_ones))
}

fn read_uint<R: Read>(r: &mut R, n: u64) -> Option<u64> {
    if n > 8 {
        return None;
    }
    let mut v = 0u64;
    for _ in 0..n {
        let mut b = [0u8; 1];
        r.read_exact(&mut b).ok()?;
        v = (v << 8) | b[0] as u64;
    }
    Some(v)
}

fn read_float<R: Read>(r: &mut R, n: u64) -> Option<f64> {
    match n {
        4 => {
            let mut b = [0u8; 4];
            r.read_exact(&mut b).ok()?;
            Some(f32::from_be_bytes(b) as f64)
        }
        8 => {
            let mut b = [0u8; 8];
            r.read_exact(&mut b).ok()?;
            Some(f64::from_be_bytes(b))
        }
        _ => None,
    }
}

const EBML_HEADER: u64 = 0x1A45_DFA3;
const SEGMENT: u64 = 0x1853_8067;
const INFO: u64 = 0x1549_A966;
const TIMECODE_SCALE: u64 = 0x2A_D7B1;
const DURATION: u64 = 0x4489;

fn mkv_duration<R: Read + Seek>(r: &mut R) -> Option<u64> {
    let (id, _, _) = vint(r, true)?;
    if id != EBML_HEADER {
        return None;
    }
    let (size, _, _) = vint(r, false)?;
    r.seek(SeekFrom::Current(size as i64)).ok()?;
    let (id, _, _) = vint(r, true)?;
    if id != SEGMENT {
        return None;
    }
    let (_seg_size, _, _) = vint(r, false)?;
    // top-level children of the Segment until Info
    for _ in 0..64 {
        let (id, _, _) = vint(r, true)?;
        let (size, _, unknown) = vint(r, false)?;
        if id == INFO && !unknown {
            let start = r.stream_position().ok()?;
            let end = start + size;
            let mut scale = 1_000_000u64;
            let mut dur: Option<f64> = None;
            while r.stream_position().ok()? < end {
                let (cid, _, _) = vint(r, true)?;
                let (csize, _, _) = vint(r, false)?;
                match cid {
                    TIMECODE_SCALE => scale = read_uint(r, csize)?.max(1),
                    DURATION => dur = read_float(r, csize),
                    _ => {
                        r.seek(SeekFrom::Current(csize as i64)).ok()?;
                    }
                }
            }
            let d = dur?;
            if !d.is_finite() || d <= 0.0 {
                return None;
            }
            return Some((d * scale as f64 / 1e9).round() as u64);
        }
        if unknown {
            return None; // e.g. a live-streamed Cluster: cannot skip it
        }
        r.seek(SeekFrom::Current(size as i64)).ok()?;
    }
    None
}

fn avi_duration<R: Read + Seek>(r: &mut R) -> Option<u64> {
    let mut head = [0u8; 12];
    r.read_exact(&mut head).ok()?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"AVI " {
        return None;
    }
    // LIST hdrl, then avih
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).ok()?;
    if &buf != b"LIST" {
        return None;
    }
    r.seek(SeekFrom::Current(4)).ok()?; // list size
    r.read_exact(&mut buf).ok()?;
    if &buf != b"hdrl" {
        return None;
    }
    r.read_exact(&mut buf).ok()?;
    if &buf != b"avih" {
        return None;
    }
    r.seek(SeekFrom::Current(4)).ok()?; // chunk size
    let mut avih = [0u8; 24];
    r.read_exact(&mut avih).ok()?;
    let usec_per_frame = u32::from_le_bytes(avih[0..4].try_into().ok()?) as u64;
    let frames = u32::from_le_bytes(avih[16..20].try_into().ok()?) as u64;
    if usec_per_frame == 0 || frames == 0 {
        return None;
    }
    Some(((usec_per_frame * frames) as f64 / 1e6).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v
    }

    fn mvhd_v0(scale: u32, dur: u32) -> Vec<u8> {
        let mut p = vec![0, 0, 0, 0];
        p.extend_from_slice(&[0; 8]);
        p.extend_from_slice(&scale.to_be_bytes());
        p.extend_from_slice(&dur.to_be_bytes());
        p.extend_from_slice(&[0; 80]);
        mp4_box(b"mvhd", &p)
    }

    #[test]
    fn mp4_moov_at_start_or_end() {
        let moov = mp4_box(b"moov", &mvhd_v0(1000, 7_021_500));
        let ftyp = mp4_box(b"ftyp", b"isom\0\0\0\0isomiso2");
        let mdat = mp4_box(b"mdat", &[0u8; 300]);
        let start: Vec<u8> = [ftyp.clone(), moov.clone(), mdat.clone()].concat();
        let end: Vec<u8> = [ftyp, mdat, moov].concat();
        assert_eq!(mp4_duration(&mut Cursor::new(start)), Some(7022));
        assert_eq!(mp4_duration(&mut Cursor::new(end)), Some(7022));
    }

    #[test]
    fn mp4_version1_header() {
        let mut p = vec![1, 0, 0, 0];
        p.extend_from_slice(&[0; 16]);
        p.extend_from_slice(&90_000u32.to_be_bytes());
        p.extend_from_slice(&(90_000u64 * 2_760).to_be_bytes());
        p.extend_from_slice(&[0; 80]);
        let data = mp4_box(b"moov", &mp4_box(b"mvhd", &p));
        assert_eq!(mp4_duration(&mut Cursor::new(data)), Some(2760));
    }

    fn ebml(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        // 8-byte size vint
        let n = payload.len() as u64;
        v.push(0x01);
        v.extend_from_slice(&n.to_be_bytes()[1..]);
        v.extend_from_slice(payload);
        v
    }

    fn mkv(duration_ms: f64, float8: bool) -> Vec<u8> {
        let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"matroska"));
        let scale = ebml(&[0x2A, 0xD7, 0xB1], &1_000_000u32.to_be_bytes());
        let dur = if float8 {
            ebml(&[0x44, 0x89], &duration_ms.to_be_bytes())
        } else {
            ebml(&[0x44, 0x89], &(duration_ms as f32).to_be_bytes())
        };
        let seekhead = ebml(&[0x11, 0x4D, 0x9B, 0x74], &[0u8; 20]);
        let info = ebml(&[0x15, 0x49, 0xA9, 0x66], &[scale, dur].concat());
        let segment = ebml(&[0x18, 0x53, 0x80, 0x67], &[seekhead, info].concat());
        [header, segment].concat()
    }

    #[test]
    fn matroska_info_duration() {
        assert_eq!(mkv_duration(&mut Cursor::new(mkv(2_765_400.0, true))), Some(2765));
        assert_eq!(mkv_duration(&mut Cursor::new(mkv(95_000.0, false))), Some(95));
    }

    #[test]
    fn avi_main_header() {
        let mut v = b"RIFF\0\0\0\0AVI LIST\0\0\0\0hdrlavih\x38\0\0\0".to_vec();
        let mut avih = [0u8; 56];
        avih[0..4].copy_from_slice(&40_000u32.to_le_bytes()); // 25 fps
        avih[16..20].copy_from_slice(&3_000u32.to_le_bytes()); // frames
        v.extend_from_slice(&avih);
        assert_eq!(avi_duration(&mut Cursor::new(v)), Some(120));
    }

    #[test]
    fn junk_and_labels() {
        assert_eq!(mp4_duration(&mut Cursor::new(vec![0u8; 3])), None);
        assert_eq!(mkv_duration(&mut Cursor::new(b"not a video".to_vec())), None);
        assert_eq!(avi_duration(&mut Cursor::new(b"RIFF".to_vec())), None);
        assert_eq!(label(7022), "1:57:02");
        assert_eq!(label(151), "2:31");
    }

    #[test]
    fn reads_a_file_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.mkv");
        std::fs::write(&p, mkv(61_000.0, true)).unwrap();
        assert_eq!(media_duration(&p), Some(61));
        let q = dir.path().join("y.mp4");
        std::fs::write(&q, b"garbage").unwrap();
        assert_eq!(media_duration(&q), None);
    }
}
