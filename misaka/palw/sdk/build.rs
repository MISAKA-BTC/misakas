//! Implementation revisions for beacon-conformance commitments (RFC-0013 §9): the reference evaluator, the independent second
//! implementation, the typed backend, the container reader and this tool's checker are committed BEFORE the randomness, so each is
//! identified by a digest of the source it was built from — a crate version string names no revision.
//!
//! The digest is BLAKE2b-512 over `(relative path, length, bytes)` of every `*.rs` file under the crate's `src`, in path order.
//! It is a build-time fact of this binary, not a claim about any other build.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|err| format!("cannot read {}: {err}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|err| format!("cannot read an entry of {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn digest_of(root: &Path) -> Result<String, String> {
    if !root.is_dir() {
        return Err(format!("implementation identity directory is missing: {}", root.display()));
    }
    let mut files = Vec::new();
    collect(root, &mut files)?;
    if files.is_empty() {
        return Err(format!("implementation identity directory has no Rust sources: {}", root.display()));
    }
    files.sort();
    let mut st = blake2b_simd::Params::new().hash_length(64).key(b"misaka.palw.runtime-pack.impl-revision.v1").to_state();
    for f in &files {
        let rel = f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(f).map_err(|err| format!("cannot read {}: {err}", f.display()))?;
        st.update(&(rel.len() as u64).to_le_bytes());
        st.update(rel.as_bytes());
        st.update(&(bytes.len() as u64).to_le_bytes());
        st.update(&bytes);
    }
    Ok(st.finalize().as_bytes().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

fn main() {
    println!("cargo:rerun-if-env-changed=MISAKA_IMPL_ID_PROBE");
    if let Ok(probe) = std::env::var("MISAKA_IMPL_ID_PROBE") {
        let failure = match probe.as_str() {
            "missing" => digest_of(Path::new("/no/such/misaka-impl-id")).expect_err("a missing identity directory fails"),
            "empty" => {
                let empty = std::env::temp_dir().join("misaka-impl-id-empty-probe");
                let _ = std::fs::remove_dir_all(&empty);
                std::fs::create_dir_all(&empty).expect("probe dir");
                digest_of(&empty).expect_err("an identity directory with no Rust sources fails")
            }
            other => panic!("unknown implementation identity probe {other}"),
        };
        panic!("implementation identity probe failed closed: {failure}");
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let roots: [(&str, &str, &str); 5] = [
        ("reference", "misaka-palw-tir", "../tir/core/src"),
        ("independent", "misaka-palw-tir-ref2", "../../../testing/reference/palw-tir-ref2/src"),
        ("backend", "misaka-palw-tir-exec", "../tir/exec/src"),
        ("container", "misaka-palw-tir-artifact", "../tir/artifact/src"),
        ("checker", "misaka-palw-sdk/runtime_pack", "src/runtime_pack"),
    ];
    let mut code = String::from(
        "/// `(role, crate, source digest)` of the implementations this binary was built from.\npub const IMPL_REVISIONS: &[(&str, &str, &str)] = &[\n",
    );
    for (role, name, rel) in roots {
        let dir = manifest.join(rel);
        println!("cargo:rerun-if-changed={}", dir.display());
        let digest = digest_of(&dir).unwrap_or_else(|why| panic!("implementation identity for {name} ({rel}): {why}"));
        let _ = writeln!(code, "    ({role:?}, {name:?}, {digest:?}),");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("impl_revisions.rs");
    std::fs::write(out, code).expect("write impl_revisions.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
