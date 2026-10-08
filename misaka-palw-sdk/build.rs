//! Implementation revisions for beacon-conformance commitments (RFC-0013 §9): the reference evaluator, the independent second
//! implementation, the typed backend, the container reader and this tool's checker are committed BEFORE the randomness, so each is
//! identified by a digest of the source it was built from — a crate version string names no revision.
//!
//! The digest is BLAKE2b-512 over `(relative path, length, bytes)` of every `*.rs` file under the crate's `src`, in path order.
//! It is a build-time fact of this binary, not a claim about any other build.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn digest_of(root: &Path) -> String {
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    let mut st = blake2b_simd::Params::new().hash_length(64).key(b"misaka.palw.runtime-pack.impl-revision.v1").to_state();
    for f in &files {
        let rel = f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(f).unwrap_or_default();
        st.update(&(rel.len() as u64).to_le_bytes());
        st.update(rel.as_bytes());
        st.update(&(bytes.len() as u64).to_le_bytes());
        st.update(&bytes);
    }
    st.finalize().as_bytes().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let roots: [(&str, &str, &str); 5] = [
        ("reference", "misaka-palw-tir", "../misaka-palw-tir/src"),
        ("independent", "misaka-palw-tir-ref2", "../misaka-palw-tir-ref2/src"),
        ("backend", "misaka-palw-tir-exec", "../misaka-palw-tir-exec/src"),
        ("container", "misaka-palw-tir-artifact", "../misaka-palw-tir-artifact/src"),
        ("checker", "misaka-palw-sdk/runtime_pack", "src/runtime_pack"),
    ];
    let mut code = String::from(
        "/// `(role, crate, source digest)` of the implementations this binary was built from.\npub const IMPL_REVISIONS: &[(&str, &str, &str)] = &[\n",
    );
    for (role, name, rel) in roots {
        let dir = manifest.join(rel);
        println!("cargo:rerun-if-changed={}", dir.display());
        let _ = writeln!(code, "    ({role:?}, {name:?}, {:?}),", digest_of(&dir));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("impl_revisions.rs");
    std::fs::write(out, code).expect("write impl_revisions.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
