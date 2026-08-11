//! Links the worker against the **pinned** llama.cpp static build (`qwen35_pins::LLAMA_COMMIT`,
//! built per `qwen35_pins::METAL_BUILD_PROFILE`). The checkout/build location comes from
//! `MISAKA_LLAMA_SRC` so the pinned tree itself never enters this repository; the shim is
//! compiled here against that tree's real `llama.h`, which is what keeps the FFI surface
//! ABI-safe — no hand-declared structs, only the flat functions `src/shim.c` exports.

fn main() {
    let src = std::env::var("MISAKA_LLAMA_SRC")
        .unwrap_or_else(|_| "/Users/wata/Downloads/misaka-palw-runtime/llama.cpp".to_string());
    println!("cargo:rerun-if-env-changed=MISAKA_LLAMA_SRC");
    println!("cargo:rerun-if-changed=src/shim.c");

    // The CPU profile is a different consensus identity, not a runtime toggle: it must be chosen
    // at BUILD time so `runtime_manifest_hash` cannot disagree with what the process actually
    // does. `MISAKA_PALW_CPU=1` selects it, and the Rust side keys its reported identity on the
    // same cfg.
    println!("cargo:rustc-check-cfg=cfg(misaka_palw_cpu)");
    let cpu_only = std::env::var("MISAKA_PALW_CPU").is_ok_and(|v| v == "1");
    println!("cargo:rerun-if-env-changed=MISAKA_PALW_CPU");
    if cpu_only {
        println!("cargo:rustc-cfg=misaka_palw_cpu");
    }
    let mut build = cc::Build::new();
    if cpu_only {
        build.define("MISAKA_PALW_CPU_ONLY", None);
    }
    build
        .file("src/shim.c")
        .include(format!("{src}/include"))
        .include(format!("{src}/ggml/include"))
        .flag_if_supported("-O2")
        .compile("misaka_palw_shim");

    let build = format!("{src}/build");
    println!("cargo:rustc-link-search=native={build}/src");
    println!("cargo:rustc-link-search=native={build}/ggml/src");
    // The CPU profile links NO GPU or BLAS backend — its identity says `gpu-off`/`no-blas`, and
    // linking them anyway would make the manifest hash a claim about a binary that is not this
    // one. It therefore needs its own llama.cpp build (`-DGGML_METAL=OFF -DGGML_BLAS=OFF`);
    // point `MISAKA_LLAMA_SRC` at that tree when building with `MISAKA_PALW_CPU=1`.
    if !cpu_only {
        println!("cargo:rustc-link-search=native={build}/ggml/src/ggml-metal");
        println!("cargo:rustc-link-search=native={build}/ggml/src/ggml-blas");
    }
    let libs: &[&str] =
        if cpu_only { &["llama", "ggml", "ggml-base", "ggml-cpu"] } else { &["llama", "ggml", "ggml-base", "ggml-cpu", "ggml-metal", "ggml-blas"] };
    for lib in libs {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    if !cpu_only {
        for framework in ["Metal", "MetalKit", "Foundation", "Accelerate", "CoreGraphics"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
    println!("cargo:rustc-link-lib=c++");
}
