//! PrimeCrew Organimizer core library (PhotoSorter before 3.0).
//!
//! The GUI (`main.rs` / `gui.rs`) is a thin shell over this library so that
//! all of the file-system logic can be unit tested on any platform.
//!
//! Pipeline overview (each stage is a separate module):
//!
//! 1. [`scan`]      – walk the chosen folder and collect candidate image files.
//! 2. [`date_source`] – decide which date to use for each file
//!    (filename / EXIF / modified / created, with an "Auto" priority order).
//! 3. [`plan`]      – build a list of [`plan::PlanItem`]s by running each file
//!    through an ordered list of [`plan::Stage`]s. First the
//!    "rename" stage runs the ordered ReNamer-style rules from [`rename`]
//!    (a no-op when no rule is enabled, exactly like v1), then the optional
//!    "Month YYYY" destination-folder stage driven by a [`template`].
//!    Collisions are resolved here (" (1)", " (2)", ...).
//! 4. [`ops`]       – execute a plan (rename, or copy+delete across devices),
//!    write an undo log, and undo the last run.
//! 5. [`settings`]  – portable, commented `PhotoSorter.ini` beside the exe.
//! 6. [`window_state`] – parse/clamp the remembered window placement and
//!    layout (`[Window]` section of the INI) so it never restores off-screen.
//! 7. [`session`] (New Project, preset summaries) and [`desktop`] (open the
//!    help file / folders with the default app) back the menu bar;
//!    [`about`] holds the product / publisher strings.
//! 8. 3.0: [`filetypes`] (type groups + files that are never touched),
//!    [`blacklist`] (system folders that cannot be chosen or scanned),
//!    [`favorites`], [`plex`] (movie / TV recognition and the Plex layout)
//!    and [`tmdb`] (optional online lookup).

pub mod about;
pub mod blacklist;
pub mod date_source;
pub mod desktop;
pub mod duration;
pub mod favorites;
pub mod filetypes;
pub mod filename_date;
pub mod ops;
pub mod plan;
pub mod plex;
pub mod rename;
pub mod scan;
pub mod session;
pub mod settings;
pub mod template;
pub mod tmdb;
pub mod window_state;

/// Lowest year considered a plausible photo date.
pub const MIN_YEAR: i32 = 1990;

/// Highest plausible year: current local year + 1.
pub fn max_year() -> i32 {
    use chrono::Datelike;
    chrono::Local::now().year() + 1
}
