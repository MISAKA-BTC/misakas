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
//! Skips without `MISAKA_FLOOR_TOKENIZER_DENSE`, because a token count is a claim about a
//! particular tokenizer and this test refuses to invent one.

use misaka_palw_base0::tokenizer::QwenTokenizer;

/// The corpus answers the demonstration actually derives and publishes. Their token cost is what
/// decides the registered width; the rest of the corpus is coverage for the grammar.
const DEMONSTRATED: &[&str] = &["music/03-overlapping-melody.json", "scene/02-hierarchy.json", "cad/01-extrude-l-bracket.json"];

/// The decode budget of a graph-v5 dense row at `n_ctx` 512: 512 less the 8-token chat template
/// and one token for the shortest possible request. Measured in `palw_context_ladder`: that row's
/// close is 80,504 bytes, one carrier, prosecutable.
const BUDGET_AT_512: usize = 503;

#[test]
fn every_demonstrated_corpus_answer_fits_the_width_the_launch_registers() {
    let Ok(path) = std::env::var("MISAKA_FLOOR_TOKENIZER_DENSE") else {
        println!("SKIPPED: set MISAKA_FLOOR_TOKENIZER_DENSE to a checkpoint's tokenizer.json. No width was checked.");
        return;
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
