//! **Which registered width the DEMONSTRATION needs — which is not the width the model gates
//! measured, and not the width the grammar floors need.**
//!
//! Three numbers get confused because they are all "tokens for a kind":
//!
//! * the GRAMMAR FLOOR — the shortest legal non-degenerate answer, 38 / 60 / 104 (`grammar_floor`).
//!   It answers "can this kind be expressed at all", and it fits a 119-token budget.
//! * what a MODEL GATE needed — the width at which a checkpoint's own answers parsed. Measured at
//!   256 for both tiers.
//! * what the SHIPPED CORPUS costs — the answers a demonstration actually shows someone. Those are
//!   whole scenes and melodies, not one-note floors, and they are the numbers here.
//!
//! A launch sized on the first two registers a class that can express every kind and cannot emit
//! the artifacts the announcement is about. `music/03-overlapping-melody` is 261 tokens and
//! `scene/02-hierarchy` is 286 — both over a 247-token budget and both under 503. **That is the
//! whole distance between n_ctx 256 and n_ctx 512**, and under ADR-0082's flat close 512 costs 64
//! bytes more than 256 and is equally prosecutable, so there is no reason to take the smaller one.
//!
//! # Four numbers, not three — and two of them were reconciled twice
//!
//! `grammar_floor`'s header pins a "smallest corpus file" column at music 73 / cad 38 / scene 135.
//! This file measures music 261 / cad 66 / scene 286. Both are right and they are DIFFERENT FILES,
//! which cost two people an exchange to establish, so it is written here where a reader lands:
//!
//! ```text
//!   kind    grammar floor   smallest corpus file        the DEMONSTRATION publishes
//!   cad          38          38  (07-box)                 66  (01-extrude-l-bracket)
//!   music        60          73  (01-single-note)        261  (03-overlapping-melody)
//!   scene       104         135  (01-cube)               286  (02-hierarchy)
//! ```
//!
//! The floor is the shortest legal answer; the smallest corpus file is the smallest thing anyone
//! bothered to write down; the demonstration column is the only one that decides a width, because
//! it is the artifacts an announcement is about. `the_three_columns_are_different_questions` pins
//! the middle one so that a corpus edit fails HERE as well as in `grammar_floor` — one measurement
//! with two spellings is how they drifted apart enough to need reconciling in the first place.
//!
//! # Absence is a decision, not a default
//!
//! A token count is a claim about a particular tokenizer and this file refuses to invent one — but
//! refusing to invent one is not the same as passing without one. The checkpoint is not in the
//! repository, so the dense tokenizer is taken from `MISAKA_FLOOR_TOKENIZER_DENSE`, else from the
//! paths it lives at on the hosts that have it (`DENSE_TOKENIZER_FIXTURES`, the shape
//! `answer_binding`'s fixtures use). If neither produces a file, the test **fails by name**,
//! listing every path it looked at — unless `MISAKA_PALW_WIDTH_NO_TOKENIZER` is set, which is how
//! a host with no checkpoint says so out loud (the SKIPPED line it prints names the test and says
//! the widths were NOT checked).
//!
//! The distinction is load-bearing: `n_ctx` is inside the borsh that `shape_profile_id` hashes, so
//! the width these numbers decide cannot be adjusted after the mint, and the announcement cites a
//! run of this file as the measurement. Until 2026-09-03 three of these tests printed "No width
//! was checked" and passed with exit 0 — a run that checked nothing was indistinguishable from
//! one that checked three columns, except by its wall clock (0.00 s against 0.42 s).

use misaka_palw_base0::tokenizer::QwenTokenizer;

/// The corpus answers the demonstration actually derives and publishes. Their token cost is what
/// decides the registered width; the rest of the corpus is coverage for the grammar.
const DEMONSTRATED: &[&str] = &["music/03-overlapping-melody.json", "scene/02-hierarchy.json", "cad/01-extrude-l-bracket.json"];

/// The decode budget of a graph-v5 dense row at `n_ctx` 512: 512 less the 8-token chat template
/// and one token for the shortest possible request. Measured in `palw_context_ladder`: that row's
/// close is 80,504 bytes, one carrier, prosecutable.
const BUDGET_AT_512: usize = 503;

/// Where the dense lane's `tokenizer.json` is looked for when `MISAKA_FLOOR_TOKENIZER_DENSE` is
/// unset — the asset is not in the repository, and an env var nobody is told to set is how a host
/// that HAS the checkpoint still measures nothing. Only the dense lane's own tokenizer belongs
/// here: another model's `tokenizer.json` reads perfectly well and yields different counts, which
/// `the_three_columns_are_different_questions` would then report as the corpus having moved.
const DENSE_TOKENIZER_FIXTURES: &[&str] = &["/Users/wata/Downloads/qwen25-tokenizer.json"];

/// Set to any value on a host that has no checkpoint. It turns the refusals below into named
/// SKIPPED lines; without it, absence FAILS.
const NO_CHECKPOINT: &str = "MISAKA_PALW_WIDTH_NO_TOKENIZER";

/// `Ok(path)` when the dense tokenizer was found. `Err(line)` only when it was not found AND
/// `MISAKA_PALW_WIDTH_NO_TOKENIZER` is set — the caller prints `line` and returns. Otherwise this
/// panics, by test name, naming every path it looked at.
fn dense_tokenizer_path(test: &str) -> Result<std::path::PathBuf, String> {
    if let Ok(p) = std::env::var("MISAKA_FLOOR_TOKENIZER_DENSE") {
        let p = std::path::PathBuf::from(p);
        assert!(p.exists(), "MISAKA_FLOOR_TOKENIZER_DENSE={} does not exist", p.display());
        return Ok(p);
    }
    if let Some(p) = DENSE_TOKENIZER_FIXTURES.iter().map(std::path::PathBuf::from).find(|p| p.exists()) {
        return Ok(p);
    }
    let looked = DENSE_TOKENIZER_FIXTURES.join(", ");
    if std::env::var_os(NO_CHECKPOINT).is_some() {
        return Err(format!(
            "SKIPPED {test}: no dense checkpoint on this host and {NO_CHECKPOINT} is set. \
             MISAKA_FLOOR_TOKENIZER_DENSE is unset and none of [{looked}] exists. \
             NO WIDTH WAS CHECKED BY THIS RUN."
        ));
    }
    panic!(
        "{test} has no dense tokenizer: MISAKA_FLOOR_TOKENIZER_DENSE is unset and none of [{looked}] exists. \
         This file measures the widths the registered n_ctx was chosen from, and n_ctx is inside the borsh that \
         shape_profile_id hashes — a run that checks nothing must not read as a run that checked. Set \
         MISAKA_FLOOR_TOKENIZER_DENSE to the dense checkpoint's tokenizer.json, or set {NO_CHECKPOINT} to declare \
         that this host cannot check them."
    );
}

/// The QWEN36 lane's `.gguf`, under the same rule. No fixture path is listed because no host in
/// this tree keeps one at a fixed location; `MISAKA_QWEN36_GGUF` is the only way in.
fn qwen36_gguf_path(test: &str) -> Result<std::path::PathBuf, String> {
    if let Ok(p) = std::env::var("MISAKA_QWEN36_GGUF") {
        let p = std::path::PathBuf::from(p);
        assert!(p.exists(), "MISAKA_QWEN36_GGUF={} does not exist", p.display());
        return Ok(p);
    }
    if std::env::var_os(NO_CHECKPOINT).is_some() {
        return Err(format!(
            "SKIPPED {test}: MISAKA_QWEN36_GGUF is unset and {NO_CHECKPOINT} is set. \
             THE SECOND LANE'S WIDTHS WERE NOT CHECKED BY THIS RUN."
        ));
    }
    panic!(
        "{test} has no QWEN36 checkpoint: MISAKA_QWEN36_GGUF is unset. The second lane tokenizes the same JSON \
         with a different vocabulary, and a width chosen from the first lane is only known to fit the second by \
         measuring it. Set MISAKA_QWEN36_GGUF to the checkpoint's .gguf, or set {NO_CHECKPOINT} to declare that \
         this host cannot check it."
    );
}

#[test]
fn every_demonstrated_corpus_answer_fits_the_width_the_launch_registers() {
    let path = match dense_tokenizer_path("every_demonstrated_corpus_answer_fits_the_width_the_launch_registers") {
        Ok(p) => p,
        Err(skipped) => return println!("{skipped}"),
    };
    let t = QwenTokenizer::from_json(&std::fs::read(&path).expect("the tokenizer reads")).expect("the tokenizer parses");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    for rel in DEMONSTRATED {
        let bytes = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        // Canonical (compact) form: what a model must emit, not the pretty-printed file.
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let compact = serde_json::to_string(&value).expect("re-serializes");
        let ids = t.encode_without_specials(&compact).expect("a canonical ASCII DSL encodes");
        println!("{rel}: {} tokens", ids.len());
        assert!(
            ids.len() <= BUDGET_AT_512,
            "{rel} is {} tokens, over the {BUDGET_AT_512}-token decode budget of the n_ctx 512 row. Either the \
             corpus answer grew or the registered width has to. A demonstration whose own artifact cannot be \
             emitted by the class it demonstrates is the failure this test exists for.",
            ids.len()
        );
    }
}

/// **The QWEN36 lane's numbers are not the dense lane's, and nobody had measured them.**
///
/// Every token count in this file and in `grammar_floor` is the shipped Qwen2.5 tokenizer's — a
/// 151,665-entry vocabulary (151,643 + 22 added tokens; the model config's padded `vocab_size` is 151,936) loaded from a `tokenizer.json`. The QWEN36 lane does not use it: its
/// checkpoint ships no `tokenizer.json` at all and carries a 248,320-entry vocabulary inside the
/// GGUF's `tokenizer.ggml.*` metadata (`misaka_palw_base0::gguf::KEEP_ARRAYS` exists for exactly
/// that reason).
///
/// A different vocabulary tokenizes the same JSON differently, so "261 tokens" is a fact about one
/// lane. If the second lane's numbers are materially larger, a width chosen from the first is too
/// small for the second — and that is a genesis input, not a detail: the registered `n_ctx` is
/// inside the borsh that `shape_profile_id` hashes, so it cannot be adjusted after the mint.
///
/// Skips loudly without `MISAKA_QWEN36_GGUF`, because the alternative to measuring is guessing.
#[test]
fn the_qwen36_lanes_tokenizer_is_measured_rather_than_assumed() {
    let gguf_path = match qwen36_gguf_path("the_qwen36_lanes_tokenizer_is_measured_rather_than_assumed") {
        Ok(p) => p,
        Err(skipped) => return println!("{skipped}"),
    };
    let bytes = std::fs::read(&gguf_path).unwrap_or_else(|e| panic!("{}: {e}", gguf_path.display()));
    let dir = misaka_palw_base0::gguf::parse_directory(&bytes).expect("the GGUF directory parses");
    let get = |k: &str| dir.metadata.get(k);
    let tokens = get("tokenizer.ggml.tokens").and_then(|v| v.as_strings()).expect("the GGUF carries its vocabulary");
    let merges = get("tokenizer.ggml.merges").and_then(|v| v.as_strings()).map(|s| s.to_vec()).unwrap_or_default();
    let types = get("tokenizer.ggml.token_type").and_then(|v| v.as_ints()).map(|s| s.to_vec()).unwrap_or_default();
    let moe = QwenTokenizer::from_gguf(tokens, &merges, &types).expect("the GGUF vocabulary builds a tokenizer");
    println!("QWEN36 vocabulary: {} entries", moe.len());

    // The companion follows the same discovery, so the instrument check at the end of this test
    // runs on every host that has the dense checkpoint, not only on one that exported the variable.
    let dense = dense_tokenizer_path("the_qwen36_lanes_tokenizer_is_measured_rather_than_assumed")
        .ok()
        .map(|p| QwenTokenizer::from_json(&std::fs::read(&p).expect("reads")).expect("parses"));

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut worst = 0usize;
    for rel in DEMONSTRATED {
        let raw = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let value: serde_json::Value = serde_json::from_slice(&raw).expect("the corpus answer parses");
        let compact = serde_json::to_string(&value).expect("re-serializes");
        let m = moe.encode_without_specials(&compact).expect("encodes").len();
        worst = worst.max(m);
        match dense.as_ref().and_then(|d| d.encode_without_specials(&compact).ok()) {
            Some(d) => println!("{rel}: qwen36 {m} tokens, dense {} tokens ({:+})", d.len(), m as i64 - d.len() as i64),
            None => println!("{rel}: qwen36 {m} tokens"),
        }
    }
    assert!(
        worst <= BUDGET_AT_512,
        "the widest demonstrated answer is {worst} tokens under the QWEN36 vocabulary, over the {BUDGET_AT_512}-token \
         budget of an n_ctx 512 row. The second lane needs a wider class than the first, and n_ctx is inside the borsh \
         that shape_profile_id hashes — so this is a genesis input, not something to adjust later."
    );

    // **The measured answer is that the two lanes agree exactly, and an agreement is worth nothing
    // until the instrument is shown capable of disagreeing.** A tokenizer built from empty or
    // unread merges degrades to something byte-ish, and two degraded tokenizers agree on
    // everything — which would read as this result. So: the vocabularies must be different sizes,
    // and they must actually split some text differently. The canonical DSL is ASCII JSON, which
    // both tokenize identically; the divergence lives outside that range.
    if let Some(d) = dense.as_ref() {
        assert_ne!(moe.len(), d.len(), "the two lanes report the same vocabulary size — one of them was not loaded");
        let divergent = ["\u{3a9}\u{2248}\u{e7}\u{221a}\u{222b}", "\u{1f3b9}\u{1f3bc}"]
            .iter()
            .filter(|s| moe.encode_without_specials(s).map(|v| v.len()).ok() != d.encode_without_specials(s).map(|v| v.len()).ok())
            .count();
        assert!(
            divergent > 0,
            "the two tokenizers agree on every probe including non-ASCII, so one of them is degenerate and the \
             corpus agreement above proves nothing"
        );
    }
}

/// **The smallest corpus file per kind, pinned here as well as in `grammar_floor`'s header.**
///
/// Not duplication for its own sake: the two files measure adjacent questions and their numbers
/// were read as contradicting each other. Pinning the middle column where the demonstration column
/// lives means an edit that moves one and not the other fails, instead of producing two true
/// tables that look like a disagreement.
#[test]
fn the_three_columns_are_different_questions() {
    let path = match dense_tokenizer_path("the_three_columns_are_different_questions") {
        Ok(p) => p,
        Err(skipped) => return println!("{skipped}"),
    };
    let t = QwenTokenizer::from_json(&std::fs::read(&path).expect("the tokenizer reads")).expect("the tokenizer parses");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    for (rel, want) in [("cad/07-box.json", 38usize), ("music/01-single-note.json", 73), ("scene/01-cube.json", 135)] {
        let raw = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let value: serde_json::Value = serde_json::from_slice(&raw).expect("parses");
        let compact = serde_json::to_string(&value).expect("re-serializes");
        let got = t.encode_without_specials(&compact).expect("encodes").len();
        assert_eq!(got, want, "{rel} is {got} tokens, not the {want} pinned in grammar_floor's header — move both");
    }
}
