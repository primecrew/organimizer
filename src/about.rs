//! Product / publisher strings shared by Help > About, the main-window
//! header and (as literals kept in sync by a test) `build.rs`, which writes
//! the Windows version resource.

/// Product name (window title, About, version resource ProductName).
pub const PRODUCT: &str = "PhotoSorter";
/// Publisher (About, version resource CompanyName).
pub const PUBLISHER: &str = "PrimeCrew Software";
/// Version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// One-line description (About, version resource FileDescription).
pub const DESCRIPTION: &str = "Rename and sort photos and videos into date folders";
/// Short distribution note for the About window.
pub const DISTRIBUTION_NOTE: &str = "Portable: no installer, no registry entries; settings live in \
PhotoSorter.ini next to the exe. The exe is not code-signed, so Windows SmartScreen may warn on \
first run (More info > Run anyway); check the SHA-256 from the release notes.";
/// License line (About); the full text ships as LICENSE beside the exe.
pub const LICENSE_LINE: &str = "Licensed under GPL v3";

/// "PhotoSorter 2.0.5".
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
        assert_eq!(PRODUCT, "PhotoSorter");
        assert_eq!(PUBLISHER, "PrimeCrew Software");
        assert_eq!(VERSION, "2.0.5");
        assert_eq!(title_with_version(), "PhotoSorter 2.0.5");
        assert!(copyright().contains("PrimeCrew Software"));
        // build.rs writes the version resource from literals; keep them in sync
        let build = include_str!("../build.rs");
        for want in [PUBLISHER, PRODUCT, DESCRIPTION] {
            assert!(build.contains(want), "build.rs is missing {want:?}");
        }
        assert!(build.contains("Copyright (C) 2026 PrimeCrew Software. Licensed under GPL v3 or later."));
        assert_eq!(LICENSE_LINE, "Licensed under GPL v3");
        assert_eq!(env!("CARGO_PKG_LICENSE"), "GPL-3.0-or-later");
        // the full, unmodified GPL text is in the repo (and in the zip)
        let gpl = include_str!("../LICENSE");
        assert!(gpl.starts_with("                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007"));
        assert!(gpl.contains("END OF TERMS AND CONDITIONS") && gpl.lines().count() == 674);
        assert_eq!(build.matches("CompanyName").count(), 1);
        assert!(DISTRIBUTION_NOTE.contains("not code-signed") && DISTRIBUTION_NOTE.contains("Portable"));
    }
}
