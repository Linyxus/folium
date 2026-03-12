use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // OUT_DIR is target/{profile}/build/folium-<hash>/out — walk up three levels
    // to reach target/{profile}/, which is where the binary lands.
    let target_dir = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let src = manifest_dir.join("libs/libpdfium.dylib");
    let dst = target_dir.join("libpdfium.dylib");

    std::fs::copy(&src, &dst).unwrap_or_else(|e| {
        panic!("build.rs: failed to copy libs/libpdfium.dylib → {dst:?}: {e}");
    });

    println!("cargo:rerun-if-changed=libs/libpdfium.dylib");
}
