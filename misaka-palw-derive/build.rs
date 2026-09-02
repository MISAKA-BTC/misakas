//! ADR-0078 Decision 3: a transformer's manifest carries "the build's source-tree hash", so a
//! transformer id names the code that produced the artifact. The walk, the framing and the
//! SHA-256 all live in `src/source_tree.rs` and are INCLUDED here rather than restated, so the
//! value this script bakes in and the value `source_tree::source_tree_sha256_hex` recomputes
//! from a checkout cannot drift apart. A build script with its own copy of the rule is a second
//! spelling, and a second spelling is a place for the two to disagree while both look right.

include!("src/source_tree.rs");

fn main() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // Watch the whole directory, not only the files found this time: emitting any
    // `rerun-if-changed` turns OFF cargo's default "rerun when anything in the package changed",
    // so a per-file list would leave an ADDED or DELETED source file unnoticed and the baked-in
    // hash naming a tree that no longer exists.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    for rel in source_files(&root) {
        println!("cargo:rerun-if-changed={rel}");
    }
    println!("cargo:rustc-env=PALW_DERIVE_SOURCE_TREE_SHA256={}", source_tree_sha256_hex(&root));
}
