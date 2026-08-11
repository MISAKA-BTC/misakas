//! Links the worker against the **pinned** llama.cpp static build (`qwen35_pins::LLAMA_COMMIT`,
//! built per `qwen35_pins::METAL_BUILD_PROFILE`). The checkout/build location comes from
//! `MISAKA_LLAMA_SRC` so the pinned tree itself never enters this repository; the shim is
//! compiled here against that tree's real `llama.h`, which is what keeps the FFI surface
//! ABI-safe — no hand-declared structs, only the flat functions `src/shim.c` exports.

fn main() {
    let src = std::env::var("MISAKA_LLAMA_SRC").unwrap_or_else(|_| "/Users/wata/Downloads/misaka-palw-runtime/llama.cpp".to_string());
    println!("cargo:rerun-if-env-changed=MISAKA_LLAMA_SRC");
    println!("cargo:rerun-if-changed=src/shim.c");

    cc::Build::new()
        .file("src/shim.c")
        .include(format!("{src}/include"))
        .include(format!("{src}/ggml/include"))
        .flag_if_supported("-O2")
        .compile("misaka_palw_shim");

    let build = format!("{src}/build");
    println!("cargo:rustc-link-search=native={build}/src");
    println!("cargo:rustc-link-search=native={build}/ggml/src");
    println!("cargo:rustc-link-search=native={build}/ggml/src/ggml-metal");
    println!("cargo:rustc-link-search=native={build}/ggml/src/ggml-blas");
    for lib in ["llama", "ggml", "ggml-base", "ggml-cpu", "ggml-metal", "ggml-blas"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    for framework in ["Metal", "MetalKit", "Foundation", "Accelerate", "CoreGraphics"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=c++");
}
