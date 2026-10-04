fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for file in ["icon.ico", "app.manifest"] {
        println!("cargo:rerun-if-changed=assets/{file}");
    }
    let assets = root.join("assets").to_string_lossy().replace('\\', "/");
    for bin in ["ramcleanup", "ramcleanup-helper"] {
        let resource = out.join(format!("{bin}.res"));
        let script = out.join(format!("{bin}.rc"));
        let mut text = format!("1 24 \"{assets}/app.manifest\"\n");
        if bin == "ramcleanup" {
            text.push_str(&format!("1 ICON \"{assets}/icon.ico\"\n"));
        }
        std::fs::write(&script, text).unwrap();
        assert!(
            std::process::Command::new("rc.exe")
                .args(["/nologo", "/fo"])
                .arg(&resource)
                .arg(&script)
                .status()
                .expect("Windows SDK rc.exe is required")
                .success()
        );
        println!("cargo:rustc-link-arg-bin={bin}={}", resource.display());
    }
    for bin in ["ramcleanup", "ramcleanup-helper"] {
        for arg in [
            "/ENTRY:mainCRTStartup",
            "/NODEFAULTLIB",
            "/STACK:131072,4096",
            // Read-only unwind metadata shares a section; its directory is preserved.
            "/MERGE:.pdata=.rdata",
        ] {
            println!("cargo:rustc-link-arg-bin={bin}={arg}");
        }
    }
}
