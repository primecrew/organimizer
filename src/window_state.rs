//! Remembered window position / size / layout (`[Window]` in
//! `PhotoSorter.ini`, since 2.0.3) and the safety clamp used when restoring.
//!
//! Everything here is plain data so it can be unit tested without a window:
//! the GUI reads the monitor work areas (see [`crate::desktop::monitor_work_areas`]),
//! calls [`restore_placement`] and only then creates the window.

/// Smallest window (inner size, logical pixels) PhotoSorter allows.
pub const MIN_SIZE: [f32; 2] = [760.0, 480.0];
/// Size used the first time, after "Reset window layout", or when the saved
/// size is unusable.
pub const DEFAULT_SIZE: [f32; 2] = [1200.0, 800.0];
/// Rough size of the title bar + borders around the inner area.
const DECOR: [f32; 2] = [16.0, 60.0];
/// How much of the title bar must be on a monitor for a saved position to
/// count as visible.
const MIN_VISIBLE: [f32; 2] = [120.0, 24.0];

/// Layout state kept between runs. Window width/height live in
/// [`crate::settings::Settings`] (`width` / `height`, as since 2.0).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowLayout {
    /// Outer top-left corner in logical pixels; None = centered.
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub maximized: bool,
    /// Height of the resizable steps 1–3 panel; None = automatic.
    pub setup_height: Option<f32>,
    /// Widths of the resizable preview-table columns; empty = defaults.
    pub files_columns: Vec<f32>,
    pub folders_columns: Vec<f32>,
    pub source_open: bool,
    pub rules_open: bool,
    pub output_open: bool,
    /// "More options" inside step 3.
    pub output_more_open: bool,
    /// Raw template field shown under the folder name builder.
    pub folder_advanced: bool,
}

impl Default for WindowLayout {
    fn default() -> Self {
        WindowLayout {
            x: None,
            y: None,
            maximized: false,
            setup_height: None,
            files_columns: Vec::new(),
            folders_columns: Vec::new(),
            source_open: true,
            rules_open: true,
            output_open: true,
            output_more_open: false,
            folder_advanced: false,
        }
    }
}

/// Lenient number: finite and inside `min..=max`, else None.
pub fn parse_num(s: &str, min: f32, max: f32) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v >= min && *v <= max)
}

/// `"120, 260, 90"` -> widths. Any bad entry (or more than 16) discards the
/// whole list, so the table falls back to its default widths.
pub fn parse_widths(s: &str) -> Vec<f32> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let v: Option<Vec<f32>> = s.split(',').map(|p| parse_num(p, 20.0, 5000.0)).collect();
    match v {
        Some(v) if v.len() <= 16 => v,
        _ => Vec::new(),
    }
}

/// Widths for the INI, rounded to whole pixels.
pub fn format_widths(v: &[f32]) -> String {
    v.iter().map(|w| format!("{}", w.round())).collect::<Vec<_>>().join(", ")
}

/// Area of a monitor that windows may use (work area, logical pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl MonitorRect {
    fn overlap(&self, x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
        let ow = (self.x + self.w).min(x + w) - self.x.max(x);
        let oh = (self.y + self.h).min(y + h) - self.y.max(y);
        (ow.max(0.0), oh.max(0.0))
    }
}

/// Where and how big to open the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Outer top-left; None = let the system center it.
    pub pos: Option<[f32; 2]>,
    /// Inner size.
    pub size: [f32; 2],
    pub maximized: bool,
}

/// Decide the start-up placement from saved values and the monitors that
/// exist now (primary first; may be empty if unknown):
/// * a size that is not a number or below [`MIN_SIZE`] -> [`DEFAULT_SIZE`];
/// * a position whose title bar is not on any monitor (unplugged screen,
///   changed resolution, minimized coordinates like -32000) -> centered;
/// * otherwise the size is shrunk to fit that monitor and the window is
///   moved fully onto it;
/// * with unknown monitors the position is dropped (centered).
pub fn restore_placement(
    pos: Option<[f32; 2]>,
    size: [f32; 2],
    maximized: bool,
    monitors: &[MonitorRect],
) -> Placement {
    let ok_size = size.iter().all(|v| v.is_finite()) && size[0] >= MIN_SIZE[0] && size[1] >= MIN_SIZE[1];
    let mut size = if ok_size { size } else { DEFAULT_SIZE };
    let monitors: Vec<MonitorRect> = monitors
        .iter()
        .copied()
        .filter(|m| m.w.is_finite() && m.h.is_finite() && m.w >= 200.0 && m.h >= 150.0)
        .collect();
    let fit = |size: [f32; 2], m: &MonitorRect| -> [f32; 2] {
        let max_w = (m.w - DECOR[0]).max(MIN_SIZE[0].min(m.w));
        let max_h = (m.h - DECOR[1]).max(MIN_SIZE[1].min(m.h));
        [size[0].min(max_w), size[1].min(max_h)]
    };
    let Some(primary) = monitors.first() else {
        return Placement { pos: None, size, maximized };
    };
    let pos = pos.filter(|p| p.iter().all(|v| v.is_finite()));
    let target = pos.and_then(|[x, y]| {
        // title bar visible on some monitor?
        let visible = monitors.iter().any(|m| {
            let (ow, oh) = m.overlap(x, y, size[0] + DECOR[0], DECOR[1]);
            ow >= MIN_VISIBLE[0].min(size[0]) && oh >= MIN_VISIBLE[1]
        });
        if !visible {
            return None;
        }
        // the monitor holding most of the window
        monitors
            .iter()
            .max_by(|a, b| {
                let area = |m: &MonitorRect| {
                    let (w, h) = m.overlap(x, y, size[0] + DECOR[0], size[1] + DECOR[1]);
                    w * h
                };
                area(a).total_cmp(&area(b))
            })
            .copied()
    });
    match (pos, target) {
        (Some([x, y]), Some(m)) => {
            size = fit(size, &m);
            let ow = size[0] + DECOR[0];
            let oh = size[1] + DECOR[1];
            let nx = x.min(m.x + m.w - ow).max(m.x);
            let ny = y.min(m.y + m.h - oh).max(m.y);
            Placement { pos: Some([nx, ny]), size, maximized }
        }
        _ => Placement { pos: None, size: fit(size, primary), maximized },
    }
}

/// Remembered (position, size); None = keep the previous value.
pub type Remembered = (Option<[f32; 2]>, Option<[f32; 2]>);

/// Position + size + maximized state to remember, from what egui reports.
/// Returns None while minimized or when the rect is unusable, so a
/// minimized window (Windows parks it at -32000) is never saved.
pub fn remember(
    outer_min: Option<[f32; 2]>,
    inner_size: Option<[f32; 2]>,
    maximized: bool,
    minimized: bool,
) -> Option<Remembered> {
    if minimized {
        return None;
    }
    let pos = outer_min.filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 30000.0));
    // While maximized keep the normal (restored) rect from before.
    if maximized {
        return Some((None, None));
    }
    let size = inner_size.filter(|s| s.iter().all(|v| v.is_finite()) && s[0] >= MIN_SIZE[0] && s[1] >= MIN_SIZE[1]);
    Some((pos.map(|p| [p[0].round(), p[1].round()]), size.map(|s| [s[0].round(), s[1].round()])))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FHD: MonitorRect = MonitorRect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 };
    const LEFT: MonitorRect = MonitorRect { x: -1280.0, y: 0.0, w: 1280.0, h: 984.0 };

    #[test]
    fn widths_and_numbers_parse_leniently() {
        assert_eq!(parse_widths("120, 260.5 ,90"), vec![120.0, 260.5, 90.0]);
        assert_eq!(parse_widths(""), Vec::<f32>::new());
        assert_eq!(parse_widths("120, abc, 90"), Vec::<f32>::new());
        assert_eq!(parse_widths("120, 5, 90"), Vec::<f32>::new()); // below 20
        assert_eq!(parse_widths("NaN"), Vec::<f32>::new());
        assert_eq!(parse_widths(&vec!["50"; 17].join(",")), Vec::<f32>::new());
        assert_eq!(format_widths(&[120.4, 260.6]), "120, 261");
        assert_eq!(parse_widths(&format_widths(&[120.0, 261.0])), vec![120.0, 261.0]);
        assert_eq!(parse_num(" -300 ", -1e5, 1e5), Some(-300.0));
        assert_eq!(parse_num("inf", -1e5, 1e5), None);
        assert_eq!(parse_num("", 0.0, 1.0), None);
    }

    #[test]
    fn restore_keeps_a_valid_saved_window() {
        let p = restore_placement(Some([100.0, 80.0]), [1300.0, 850.0], false, &[FHD]);
        assert_eq!(p, Placement { pos: Some([100.0, 80.0]), size: [1300.0, 850.0], maximized: false });
        // on a second monitor left of the primary
        let p = restore_placement(Some([-1200.0, 50.0]), [1000.0, 700.0], true, &[FHD, LEFT]);
        assert_eq!(p.pos, Some([-1200.0, 50.0]));
        assert!(p.maximized);
    }

    #[test]
    fn restore_clamps_offscreen_tiny_and_huge_windows() {
        // monitor unplugged: position on the right of the only screen -> centered
        let p = restore_placement(Some([2500.0, 100.0]), [1200.0, 800.0], false, &[FHD]);
        assert_eq!(p.pos, None);
        // minimized-window coordinates
        assert_eq!(restore_placement(Some([-32000.0, -32000.0]), [1200.0, 800.0], false, &[FHD]).pos, None);
        // title bar above the screen
        assert_eq!(restore_placement(Some([100.0, -500.0]), [1200.0, 800.0], false, &[FHD]).pos, None);
        // tiny or garbage size -> default
        assert_eq!(restore_placement(None, [200.0, 100.0], false, &[FHD]).size, DEFAULT_SIZE);
        assert_eq!(restore_placement(None, [f32::NAN, 800.0], false, &[FHD]).size, DEFAULT_SIZE);
        // partly off the right edge: moved fully onto the monitor
        let p = restore_placement(Some([1500.0, 600.0]), [1200.0, 800.0], false, &[FHD]);
        assert_eq!(p.pos, Some([1920.0 - 1216.0, 1040.0 - 860.0]));
        // bigger than the monitor (resolution lowered): shrunk to fit
        let small = MonitorRect { x: 0.0, y: 0.0, w: 1280.0, h: 680.0 };
        let p = restore_placement(Some([0.0, 0.0]), [2400.0, 1400.0], false, &[small]);
        assert_eq!(p.size, [1264.0, 620.0]);
        assert_eq!(p.pos, Some([0.0, 0.0]));
        let p = restore_placement(None, [2400.0, 1400.0], false, &[small]);
        assert_eq!((p.pos, p.size), (None, [1264.0, 620.0]));
        // unknown monitors: size kept, centered
        let p = restore_placement(Some([100.0, 100.0]), [1300.0, 900.0], false, &[]);
        assert_eq!((p.pos, p.size), (None, [1300.0, 900.0]));
        // NaN position
        assert_eq!(restore_placement(Some([f32::NAN, 0.0]), [1200.0, 800.0], false, &[FHD]).pos, None);
    }

    #[test]
    fn remember_skips_minimized_and_maximized_rects() {
        assert_eq!(remember(Some([-32000.0, -32000.0]), Some([160.0, 28.0]), false, true), None);
        assert_eq!(remember(Some([0.0, 0.0]), Some([1920.0, 1000.0]), true, false), Some((None, None)));
        assert_eq!(
            remember(Some([10.4, 20.6]), Some([1300.2, 800.0]), false, false),
            Some((Some([10.0, 21.0]), Some([1300.0, 800.0])))
        );
        assert_eq!(remember(Some([10.0, 20.0]), Some([300.0, 200.0]), false, false), Some((Some([10.0, 20.0]), None)));
        assert_eq!(remember(Some([-40000.0, 0.0]), None, false, false), Some((None, None)));
    }
}
