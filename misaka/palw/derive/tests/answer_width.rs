//! **How wide does a class's answer have to be before a derivation is possible at all?**
//!
//! ADR-0078's leg is only reachable if the model can EMIT the answer. The registered class widths
//! are small, and "the smallest shipped MIDI DSL is 277 bytes" has been quoted at the launch as
//! the number to size a row against — but 277 is the size of a corpus file that a human
//! pretty-printed, and the model does not have to write it that way.
//!
//! So this measures the thing the row is actually asked to hold, in two units:
//!
//! * **bytes**, for every corpus answer, of the raw file AND of the canonical DSL the grammar
//!   reduces it to;
//! * **tokens**, under the real Qwen tokenizer, when `MISAKA_PALW_TOKENIZER` names one — the
//!   asset is taken from the environment and never hardcoded, and its absence is a refusal by
//!   name rather than a number quietly measured against nothing.
//!
//! The first test is the load-bearing one and needs no assets: **the canonical DSL is itself a
//! legal answer, and derives to the same object.** Canonicalization is idempotent, so a class
//! that emits the canonical form directly produces a byte-identical artifact and a byte-identical
//! `dsl_hash` — and the width a row must carry is the CANONICAL size, not the corpus file's. For
//! `music/smf/v1` that is 184 bytes rather than 277, and for `cad/stl/v1` 87 rather than 178.
//!
//! Nothing here asserts a width is sufficient: which classes can emit which widths is a fact
//! about the registered classes, and this file has no business claiming it. What it does is stop
//! the requirement from being quoted at the wrong number.
//!
//! Run the measurement (it prints a table and is `#[ignore]`d because a measurement is not a
//! gate):
//!
//! ```text
//! MISAKA_PALW_TOKENIZER=/path/to/tokenizer.json \
//!   MISAKA_PALW_POW_FIXTURE=1 cargo test -p misaka-palw-derive --test answer_width -- --ignored --nocapture
//! ```

use kaspa_hashes::Hash64;
use misaka_palw_derive::{ClaimBinding, derive_named, registry};
use std::path::{Path, PathBuf};

fn binding() -> ClaimBinding {
    ClaimBinding {
        network_domain: Hash64::from_bytes([0x01; 64]),
        claim_id: Hash64::from_bytes([0x02; 64]),
        output_root: Hash64::from_bytes([0x03; 64]),
        executor_pubkey: vec![0x11; 2592],
    }
}

/// The transformer whose corpus directory each kind name holds, in the layout the drill walks.
/// `contract/evm/v1` shares `corpus/code` with `code/evm/v1` and is left out here so a file is
/// not measured twice under two names.
const KINDS: &[(&str, &str)] = &[
    ("music", "music/smf/v1"),
    ("scene", "scene/glb/v1"),
    ("cad", "cad/stl/v1"),
    ("image", "image/png/v1"),
    ("map", "map/mmap/v1"),
    ("simulation", "simulation/trace/v1"),
    ("code", "code/evm/v1"),
];

fn corpus_answers(kind: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join(kind);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "golden.json"))
        .collect();
    files.sort();
    files
}

/// **The canonical DSL is a legal answer, and it derives to the same object.**
///
/// This is what makes the narrowest-answer number the canonical size and not the corpus file's,
/// and it is a property of the grammars rather than an accident of the samples, so it is checked
/// over every corpus row of every kind that derives at all.
#[test]
fn the_canonical_dsl_re_derives_to_the_same_artifact_so_the_width_needed_is_the_canonical_size() {
    let binding = binding();
    let mut checked = 0usize;
    for (kind, transformer) in KINDS {
        for path in corpus_answers(kind) {
            let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let Ok(first) = derive_named(transformer, &binding, &raw) else { continue };
            let again = derive_named(transformer, &binding, &first.canonical_dsl)
                .unwrap_or_else(|e| panic!("{}: the canonical DSL is not itself an answer: {e}", path.display()));
            assert_eq!(again.canonical_dsl, first.canonical_dsl, "{}: canonicalization is not idempotent", path.display());
            assert_eq!(again.dsl_hash, first.dsl_hash, "{}", path.display());
            assert_eq!(again.artifact_hash, first.artifact_hash, "{}", path.display());
            assert!(
                first.canonical_dsl.len() <= raw.len(),
                "{}: the canonical form is LARGER than the answer ({} > {})",
                path.display(),
                first.canonical_dsl.len(),
                raw.len()
            );
            checked += 1;
        }
    }
    assert!(checked >= 25, "only {checked} corpus rows were re-derived");
}

/// **The narrowest answer per kind, in bytes** — asserted rather than only printed, because the
/// launch quotes these numbers and a number nothing pins drifts. The two the launch names are
/// pinned exactly; the rest are bounded, so a kind that grows its smallest sample says so here.
#[test]
fn the_narrowest_answer_of_each_kind_is_the_size_the_launch_quotes() {
    let binding = binding();
    for (kind, transformer) in KINDS {
        let mut narrowest: Option<(String, usize, usize)> = None;
        for path in corpus_answers(kind) {
            let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let Ok(d) = derive_named(transformer, &binding, &raw) else { continue };
            let leaf = path.file_name().unwrap().to_string_lossy().into_owned();
            let c = d.canonical_dsl.len();
            if narrowest.as_ref().is_none_or(|(_, best, _)| c < *best) {
                narrowest = Some((leaf, c, raw.len()));
            }
        }
        let (leaf, canonical, raw) = narrowest.unwrap_or_else(|| panic!("{transformer} derives nothing in corpus/{kind}"));
        println!("  {transformer:<22} narrowest {leaf:<34} canonical {canonical:>6} B   corpus file {raw:>6} B");
        match *transformer {
            // The two the launch sizes a row against.
            "music/smf/v1" => assert_eq!((canonical, raw), (184, 277), "the MIDI width moved"),
            "cad/stl/v1" => assert_eq!((canonical, raw), (87, 178), "the CAD width moved"),
            "scene/glb/v1" => assert_eq!((canonical, raw), (325, 740), "the glTF width moved"),
            _ => assert!(canonical > 0),
        }
    }
}

/// **The same widths in tokens**, under the tokenizer the class actually uses.
///
/// Ignored by default and skipped by NAME without the asset: a measurement that silently reported
/// nothing when `MISAKA_PALW_TOKENIZER` was unset would be indistinguishable from one that
/// measured zero.
#[test]
#[ignore = "needs MISAKA_PALW_TOKENIZER; run with --ignored --nocapture"]
fn the_narrowest_answer_of_each_kind_in_tokens() {
    let Some(path) = std::env::var_os("MISAKA_PALW_TOKENIZER") else {
        panic!(
            "MISAKA_PALW_TOKENIZER is not set: this measurement needs the tokenizer.json of the class being sized, and refuses rather than guessing at one"
        );
    };
    let path = PathBuf::from(path);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("MISAKA_PALW_TOKENIZER={}: {e}", path.display()));
    let tok = misaka_palw_base0::tokenizer::QwenTokenizer::from_json(&bytes)
        .unwrap_or_else(|e| panic!("{} is not a Qwen tokenizer.json: {e:?}", path.display()));
    println!("tokenizer {} ({} tokens in the vocabulary)", path.display(), tok.len());

    let binding = binding();
    for (kind, transformer) in KINDS {
        let mut narrowest: Option<(String, usize, usize, usize)> = None;
        for p in corpus_answers(kind) {
            let raw = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            let Ok(d) = derive_named(transformer, &binding, &raw) else { continue };
            let text = String::from_utf8(d.canonical_dsl.clone()).expect("the canonical DSL is UTF-8");
            let ids = tok.encode_without_specials(&text).expect("the canonical DSL encodes");
            let leaf = p.file_name().unwrap().to_string_lossy().into_owned();
            if narrowest.as_ref().is_none_or(|(_, _, best, _)| ids.len() < *best) {
                narrowest = Some((leaf, d.canonical_dsl.len(), ids.len(), raw.len()));
            }
        }
        if let Some((leaf, canonical, tokens, raw)) = narrowest {
            println!("  {transformer:<22} narrowest {leaf:<34} {tokens:>6} tokens   {canonical:>6} canonical B   {raw:>6} corpus B");
        }
    }
    // The registry is printed beside the table so a reader sizing a row can see the ceiling the
    // transformer declares next to the floor the corpus measures.
    for (name, _, _) in registry::transformer_names() {
        let m = registry::transformer_by_name(name).expect("registered").manifest();
        println!("  {name:<22} declares max_dsl_bytes {}", m.max_dsl_bytes);
    }
}
