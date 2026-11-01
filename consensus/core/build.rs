//! Detect only the audited local Source allocation profile. Legacy/native
//! behavior does not depend on this private result; unknown builds still build
//! the crate and the new local entry point refuses before scanning.
use std::{env, fs, path::PathBuf, process::Command};

const SUPPORTED: &str = "rust-1.93.0-borsh-1.5.1-v1";

fn nonempty(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn audited_flags(flags: &[&str]) -> bool {
    let mut at = 0;
    while at < flags.len() {
        let flag = flags[at];
        if flag == "-Dwarnings" {
            at += 1;
            continue;
        }
        if flag == "-D" && flags.get(at + 1) == Some(&"warnings") {
            at += 2;
            continue;
        }
        let option = if flag == "-C" {
            at += 1;
            let Some(option) = flags.get(at) else { return false };
            *option
        } else if let Some(option) = flag.strip_prefix("-C") {
            option
        } else {
            // In particular: custom --sysroot, -Z/build-std, arbitrary cfg,
            // target JSON and unknown code-generation flags are not admitted.
            return false;
        };
        let Some((key, _)) = option.split_once('=') else { return false };
        if !matches!(
            key,
            "opt-level"
                | "debuginfo"
                | "codegen-units"
                | "force-frame-pointers"
                | "force-unwind-tables"
                | "link-arg"
                | "link-args"
                | "target-cpu"
                | "target-feature"
                | "panic"
                | "strip"
                | "embed-bitcode"
                | "split-debuginfo"
                | "lto"
        ) {
            return false;
        }
        at += 1;
    }
    true
}

fn lock_has_only_audited_borsh(lock: &str) -> bool {
    const REGISTRY: &str = "\"registry+https://github.com/rust-lang/crates.io-index\"";
    let audited = [
        ("\"borsh\"", "\"a6362ed55def622cddc70a4746a68554d7b687713770de539e59a739b249f8ed\""),
        ("\"borsh-derive\"", "\"c3ef8005764f53cd4dca619f5bf64cafd4664dada50ece25e4d81de54c80cc0b\""),
    ];
    let mut found = [false; 2];
    for package in lock.split("[[package]]").skip(1) {
        let mut name = None;
        let mut version = None;
        let mut source = None;
        let mut checksum = None;
        for line in package.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value);
            } else if let Some(value) = line.strip_prefix("version = ") {
                version = Some(value);
            } else if let Some(value) = line.strip_prefix("source = ") {
                source = Some(value);
            } else if let Some(value) = line.strip_prefix("checksum = ") {
                checksum = Some(value);
            }
        }
        for (index, (audited_name, audited_checksum)) in audited.iter().enumerate() {
            if name == Some(*audited_name) {
                if found[index] || version != Some("\"1.5.1\"") || source != Some(REGISTRY) || checksum != Some(*audited_checksum) {
                    return false;
                }
                found[index] = true;
            }
        }
    }
    found.into_iter().all(|present| present)
}

fn supported_build(manifest: &std::path::Path) -> bool {
    if ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP", "RUST_TARGET_PATH"].into_iter().any(nonempty) {
        return false;
    }
    if env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() != Ok("64")
        || env::var("CARGO_CFG_TARGET_ENDIAN").as_deref() != Ok("little")
        || !matches!(
            env::var("TARGET").as_deref(),
            Ok("aarch64-apple-darwin" | "x86_64-apple-darwin" | "aarch64-unknown-linux-gnu" | "x86_64-unknown-linux-gnu")
        )
    {
        return false;
    }
    let flags = env::var("RUSTFLAGS").unwrap_or_default();
    let encoded = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    if !audited_flags(&flags.split_whitespace().collect::<Vec<_>>())
        || !audited_flags(&encoded.split('\u{1f}').filter(|value| !value.is_empty()).collect::<Vec<_>>())
    {
        return false;
    }
    let Some(rustc) = env::var_os("RUSTC") else { return false };
    let Ok(output) = Command::new(rustc).arg("-vV").output() else { return false };
    if !output.status.success()
        || !std::str::from_utf8(&output.stdout).is_ok_and(|text| text.lines().any(|line| line == "release: 1.93.0"))
    {
        return false;
    }
    let Ok(lock) = fs::read_to_string(manifest.join("../../Cargo.lock")) else { return false };
    lock_has_only_audited_borsh(&lock)
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(misaka_source_local_audited_rust193_v1)");
    for name in [
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_BOOTSTRAP",
        "RUST_TARGET_PATH",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "TARGET",
        "CARGO_CFG_TARGET_POINTER_WIDTH",
        "CARGO_CFG_TARGET_ENDIAN",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=../../rust-toolchain.toml");
    let supported = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from).is_some_and(|path| supported_build(&path));
    if supported {
        println!("cargo:rustc-cfg=misaka_source_local_audited_rust193_v1");
    }
    println!("cargo:rustc-env=MISAKA_SOURCE_LOCAL_BUILD_PROFILE_V1={}", if supported { SUPPORTED } else { "unsupported" });
}
