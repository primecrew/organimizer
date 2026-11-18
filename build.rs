// Embeds the icon and version information into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            // keep in sync with src/about.rs (checked by a unit test);
            // FileVersion / ProductVersion come from Cargo.toml
            .set("CompanyName", "PrimeCrew Software")
            .set("ProductName", "PhotoSorter")
            // FileDescription is what Task Manager shows, so keep it short
            .set("FileDescription", "PhotoSorter")
            .set("Comments", "Rename and sort photos and videos into date folders")
            .set("InternalName", "PhotoSorter")
            .set("OriginalFilename", "PhotoSorter.exe")
            .set("LegalCopyright", "Copyright (C) 2026 PrimeCrew Software. Licensed under GPL v3 or later.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
