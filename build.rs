fn main() {
    println!("cargo:rerun-if-changed=src/collect/disk_io.c");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    cc::Build::new()
        .file("src/collect/disk_io.c")
        .warnings(true)
        .compile("disk_io");
    println!("cargo:rustc-link-lib=framework=IOKit");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}
