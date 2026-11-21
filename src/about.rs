//! Product / publisher strings shared by Help > About, the main-window
//! header and (as literals kept in sync by a test) `build.rs`, which writes
//! the Windows version resource.

/// Product name (window title, About, version resource ProductName).
pub const PRODUCT: &str = "Organimizer";
/// Short name (exe, settings file, installer folder).
pub const SHORT_NAME: &str = "Organimizer";
/// Visible name in 3.0.0 and 3.0.1 (leftover Start Menu shortcuts, settings fallback).
pub const PREV_PRODUCT: &str = "PrimeCrew Organimizer";
/// Name before 3.0 (settings import, installer upgrade).
pub const FORMER_NAME: &str = "PhotoSorter";
/// Publisher (About, version resource CompanyName).
pub const PUBLISHER: &str = "PrimeCrew Software";
/// Version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// One-line description (About, version resource Comments).
pub const DESCRIPTION: &str = "Rename and sort files into date folders, and organize movies and TV shows for Plex";
/// Short distribution note (docs / tests); Help > About uses a shorter line.
pub const DISTRIBUTION_NOTE: &str = "Portable zip or installer. The portable copy keeps Organimizer.ini \
next to the exe; an installed copy keeps it in %APPDATA%\\PrimeCrew Software\\Organimizer. The exe \
is not code-signed, so Windows SmartScreen may warn on first run (More info > Run anyway); check \
the SHA-256 from the release notes.";
/// Compact About line (Tweakinator-style: short legal / distribution note + license).
pub const ABOUT_NOTE: &str = "The exe is not code-signed, so Windows SmartScreen may warn on first run \
(More info > Run anyway). Licensed under GPL v3.";
/// License line (About); the full text ships as LICENSE beside the exe.
pub const LICENSE_LINE: &str = "Licensed under GPL v3";

/// "Organimizer 3.0.2".
pub fn title_with_version() -> String {
    format!("{PRODUCT} {VERSION}")
}

/// Copyright line (About; LegalCopyright in the version resource is the
/// same text plus "Licensed under GPL v3 or later").
pub fn copyright() -> String {
    format!("Copyright (C) 2026 {PUBLISHER}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branding_is_consistent() {
        assert_eq!(PRODUCT, "Organimizer");
        assert_eq!(SHORT_NAME, "Organimizer");
        assert_eq!(PREV_PRODUCT, "PrimeCrew Organimizer");
        assert_eq!(PUBLISHER, "PrimeCrew Software");
        assert_eq!(VERSION, "3.0.2");
        assert_eq!(title_with_version(), "Organimizer 3.0.2");
        assert!(copyright().contains("PrimeCrew Software"));
        // build.rs writes the version resource from literals; keep them in sync
        let build = include_str!("../build.rs");
        for want in [PUBLISHER, PRODUCT, DESCRIPTION] {
            assert!(build.contains(want), "build.rs is missing {want:?}");
        }
        assert!(build.contains("Copyright (C) 2026 PrimeCrew Software. Licensed under GPL v3 or later."));
        assert_eq!(LICENSE_LINE, "Licensed under GPL v3");
        assert!(ABOUT_NOTE.contains("not code-signed") && ABOUT_NOTE.contains("Licensed under GPL v3"));
        assert_eq!(env!("CARGO_PKG_LICENSE"), "GPL-3.0-or-later");
        // the full, unmodified GPL text is in the repo (and in the zip)
        let gpl = include_str!("../LICENSE");
        // a one-line program copyright sits above the unmodified GPL v3 text
        assert!(gpl.starts_with("Copyright (C) 2026 PrimeCrew Software\n\n"));
        assert!(gpl.contains("                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007"));
        assert!(gpl.contains("END OF TERMS AND CONDITIONS"));
        assert_eq!(gpl.lines().count(), 676);
        assert_eq!(build.matches("CompanyName").count(), 1);
        assert!(DISTRIBUTION_NOTE.contains("not code-signed") && DISTRIBUTION_NOTE.contains("Portable"));
        assert!(DISTRIBUTION_NOTE.contains(r"%APPDATA%\PrimeCrew Software\Organimizer"));
        assert!(crate::tmdb::ATTRIBUTION.contains("not endorsed or certified by TMDB"));
    }
}
