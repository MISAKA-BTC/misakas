//! Links the worker against the **pinned** llama.cpp static build (`qwen35_pins::LLAMA_COMMIT`,
//! built per `qwen35_pins::METAL_BUILD_PROFILE`). The checkout/build location comes from
//! `MISAKA_LLAMA_SRC` so the pinned tree itself never enters this repository; the shim is
//! compiled here against that tree's real `llama.h`, which is what keeps the FFI surface
//! ABI-safe — no hand-declared structs, only the flat functions `src/shim.c` exports.
//!
//! For the v2 `RuntimeManifestV2` this script also captures the artifact identity of what it
//! links: the SHA-256 of the tree's `CMakeCache.txt`, the combined SHA-256 of every static
//! library actually linked, and the GGML_* flags read out of the cache itself — measured from
//! the real build, never declared. A tree missing any of them fails the build (fail closed).

use sha2::Digest;

fn sha256_file_hex(path: &str) -> String {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| panic!("cannot open {path} for the artifact manifest: {e}"));
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).unwrap_or_else(|e| panic!("cannot read {path} for the artifact manifest: {e}"));
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads a `KEY:BOOL=` line out of a CMake cache. A missing key is a hard build error: the v2
/// manifest must describe the flags the linked kernels were actually compiled with, and a cache
/// without them is not the pinned tree.
fn cmake_flag(cache: &str, key: &str, cache_path: &str) -> bool {
    for line in cache.lines() {
        if let Some(rest) = line.strip_prefix(key)
            && let Some(value) = rest.strip_prefix(":BOOL=")
        {
            return matches!(value.trim(), "ON" | "TRUE" | "YES" | "1");
        }
    }
    panic!("{cache_path} does not define {key}:BOOL — this is not a configured pinned llama.cpp build tree");
}

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
    // The C++ runtime ggml is compiled against differs by platform, and getting it wrong is a
    // link failure, not a silent one: Apple's toolchain ships libc++, a stock Linux toolchain
    // links libstdc++ and has no libc++ at all. Picking by target keeps ONE build.rs able to
    // produce both the Metal profile (macOS) and the portable CPU profile the Linux fleet audits
    // within — which is the whole point of the CPU profile existing.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }

    // ---- v2 artifact identity (RuntimeManifestV2) ------------------------------------------
    // Everything below is measured from the pinned tree, not declared by hand. The GGML flags
    // come from the tree's own CMakeCache.txt (the file that configured the kernels this binary
    // links); the combined library hash covers exactly the archives linked above, in a fixed
    // order with the library name bound to each digest.
    let cache_path = format!("{build}/CMakeCache.txt");
    println!("cargo:rerun-if-changed={cache_path}");
    let cache = std::fs::read_to_string(&cache_path)
        .unwrap_or_else(|e| panic!("cannot read {cache_path}: {e} — build the pinned llama.cpp tree before this crate"));
    for key in [
        "GGML_NATIVE",
        "GGML_OPENMP",
        "GGML_BLAS",
        "GGML_ACCELERATE",
        "GGML_SSE42",
        "GGML_AVX",
        "GGML_AVX2",
        "GGML_FMA",
        "GGML_F16C",
        "GGML_CPU_ALL_VARIANTS",
    ] {
        println!("cargo:rustc-env=MISAKA_PALW_{key}={}", cmake_flag(&cache, key, &cache_path) as u8);
    }
    println!("cargo:rustc-env=MISAKA_PALW_CMAKE_CACHE_SHA256={}", sha256_file_hex(&cache_path));

    let lib_paths: Vec<String> = libs
        .iter()
        .map(|lib| match *lib {
            "llama" => format!("{build}/src/libllama.a"),
            "ggml-metal" => format!("{build}/ggml/src/ggml-metal/libggml-metal.a"),
            "ggml-blas" => format!("{build}/ggml/src/ggml-blas/libggml-blas.a"),
            other => format!("{build}/ggml/src/lib{other}.a"),
        })
        .collect();
    let mut combined = sha2::Sha256::new();
    for (lib, path) in libs.iter().zip(&lib_paths) {
        println!("cargo:rerun-if-changed={path}");
        combined.update(lib.as_bytes());
        combined.update(b":");
        combined.update(sha256_file_hex(path).as_bytes());
        combined.update(b"\n");
    }
    let combined_hex: String = combined.finalize().iter().map(|b| format!("{b:02x}")).collect();
    println!("cargo:rustc-env=MISAKA_PALW_LLAMA_LIBS_SHA256={combined_hex}");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let rustc_version = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unpinned".into());
    println!("cargo:rustc-env=MISAKA_PALW_RUSTC_VERSION={rustc_version}");
    println!("cargo:rustc-env=MISAKA_PALW_TARGET_TRIPLE={}", std::env::var("TARGET").unwrap_or_else(|_| "unknown".into()));
}
