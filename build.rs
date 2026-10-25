// Embeds the icon and version information into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "PhotoSorter")
            .set("FileDescription", "PhotoSorter - sort images into Month YYYY folders")
            .set("OriginalFilename", "PhotoSorter.exe")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
