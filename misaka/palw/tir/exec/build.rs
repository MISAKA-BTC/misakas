//! The Metal bridge (feature `metal`, macOS only): compiles `src/fused/metal/shim.m` with the kernel source embedded as a string,
//! so the kernel is compiled by the system's Metal runtime at first use and no offline shader toolchain is needed.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/fused/metal/shim.m");
    println!("cargo:rerun-if-changed=src/fused/metal/unit_rows.metal");
    #[cfg(target_os = "macos")]
    if std::env::var_os("CARGO_FEATURE_METAL").is_some() {
        let src = std::fs::read_to_string("src/fused/metal/unit_rows.metal").expect("the Metal kernel source");
        let literal: String = src.lines().map(|l| format!("\"{}\\n\"\n", l.replace('\\', "\\\\").replace('"', "\\\""))).collect();
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        std::fs::write(out.join("unit_rows.inc"), literal).expect("write the kernel literal");
        cc::Build::new().file("src/fused/metal/shim.m").include(&out).flag("-fobjc-arc").compile("tir_metal_shim");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
