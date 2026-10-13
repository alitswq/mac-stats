fn main() {
    println!("cargo:rerun-if-changed=BUILD_NUMBER");
    let build_number: u64 = std::fs::read_to_string("BUILD_NUMBER")
        .expect("BUILD_NUMBER must exist")
        .trim()
        .parse()
        .expect("BUILD_NUMBER must contain a positive integer");
    assert!(build_number > 0, "BUILD_NUMBER must be greater than zero");
    println!("cargo:rustc-env=MAC_STATS_BUILD_NUMBER={build_number}");

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
