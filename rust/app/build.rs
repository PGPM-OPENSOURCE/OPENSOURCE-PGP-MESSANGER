//! Windows: puts PGPM's icon and version info into PGPM.exe (Explorer, the taskbar and the
//! setup's shortcuts show it). Other systems: nothing to do.

fn main() {
    // winresource is a build-dependency on Windows hosts only; build for a Windows target only
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico")
            .set("ProductName", "PGPM")
            .set("FileDescription", "PGPM")
            .set("CompanyName", "PGPM")
            .set("LegalCopyright", "GPL-3.0");
        if let Err(e) = res.compile() {
            panic!("couldn't embed the Windows icon / version info: {e}");
        }
    }
}
