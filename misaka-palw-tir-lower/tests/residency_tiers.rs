//! **The residency's tiers of every Hugging Face fixture's program** (`misaka_palw_tir_exec::tiers`,
//! `docs/design/palw/tir/runtime-residency.md`): each fixture's `config.json` lowered — the program
//! alone, no weight read — and its params classified by the program's dataflow, with every
//! row-addressed param served by rows (`pin_below_bytes = 0`).
//!
//! What the corpus must show, with no model name consulted by the classifier (the test knows which
//! fixtures are mixtures only from their configs, to check the classifier against): a mixture's
//! expert stacks are routed, and nothing else's are; every model gathers its token embedding by the
//! token; a model with Gemma-3n/4's per-layer inputs gathers its per-layer table by the token too;
//! and the arithmetic adds up — the weights are the three tiers, the floor the pinned set, one token's
//! routed rows and one admission in flight.

use misaka_palw_tir_exec::{TirTierRulesV1, TirTierV1, TirTiersV1};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::lower::LowerOpts;
use std::path::Path;

/// Does the config declare routed experts? (The test's knowledge, from the config's own keys; the
/// classifier reads the program.)
fn routes_experts(cfg: &serde_json::Value) -> bool {
    let text = cfg.get("text_config").unwrap_or(cfg);
    ["num_experts", "num_local_experts", "n_routed_experts", "moe_num_experts"]
        .iter()
        .any(|k| text.get(*k).and_then(|v| v.as_u64()).is_some_and(|n| n > 1))
}

#[test]
fn every_fixture_program_is_tiered_by_its_dataflow() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&root).expect("the fixtures").flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let rules = TirTierRulesV1 { pin_below_bytes: 0 };
    let (mut lowered, mut mixtures, mut per_layer_tables) = (0, Vec::new(), Vec::new());
    for name in names {
        let Ok(text) = std::fs::read_to_string(root.join(&name).join("config.json")) else { continue };
        let cfg: serde_json::Value = serde_json::from_str(&text).expect("json");
        // A fixture this lowerer does not lower (a vision tower alone, an encoder) has no program here.
        let Ok(prep) = fidelity::prepare(&text, &LowerOpts { max_window: Some(64), ..Default::default() }) else { continue };
        let program = &prep.lowered.program;
        let tiers = TirTiersV1::of(program, rules);
        let routed: Vec<&str> = tiers
            .params
            .iter()
            .enumerate()
            .filter(|(_, t)| t.tier == TirTierV1::Routed)
            .map(|(j, _)| program.params[j].name.as_str())
            .collect();
        let gathered: Vec<&str> = tiers
            .params
            .iter()
            .enumerate()
            .filter(|(_, t)| t.tier == TirTierV1::Gathered)
            .map(|(j, _)| program.params[j].name.as_str())
            .collect();
        if routes_experts(&cfg) {
            assert!(routed.len() >= 2, "{name}: a mixture's expert stacks are routed: {routed:?}");
            mixtures.push(name.clone());
        } else {
            // A dense model may still route a TABLE by an activation (an activation lookup); never a
            // stack of a layer's experts.
            assert!(
                tiers.params.iter().filter(|t| t.tier == TirTierV1::Routed).all(|t| t.unit <= 1),
                "{name}: a dense model routes no stack: {routed:?}"
            );
        }
        assert!(!gathered.is_empty(), "{name}: the token embedding is gathered");
        let text_cfg = cfg.get("text_config").unwrap_or(&cfg);
        if text_cfg.get("hidden_size_per_layer_input").and_then(|v| v.as_u64()).is_some_and(|d| d > 0) {
            let per_layer = tiers
                .params
                .iter()
                .enumerate()
                .any(|(j, t)| t.tier == TirTierV1::Gathered && program.params[j].per_layer && t.instances.len() > 1);
            assert!(per_layer, "{name}: the per-layer input table is gathered by the token at every layer: {gathered:?}");
            per_layer_tables.push(name.clone());
        }
        let a = tiers.arithmetic();
        assert_eq!(a.weight_bytes, a.pinned_bytes + a.routed_bytes + a.gathered_bytes, "{name}");
        assert_eq!(a.floor_bytes, a.pinned_bytes + a.routed_token_bytes + a.in_flight_bytes, "{name}");
        assert!(a.routed_token_bytes <= a.routed_bytes && a.in_flight_bytes <= a.routed_token_bytes, "{name}: {a:?}");
        // At the node's default rule a fixture this small routes nothing — its stacks are under a MiB;
        // what it may still gather is a table sized by the context (a LongRoPE row per position).
        assert!(TirTiersV1::of(program, TirTierRulesV1::default()).params.iter().all(|t| t.tier != TirTierV1::Routed), "{name}");
        lowered += 1;
    }
    eprintln!("{lowered} fixture programs tiered; mixtures {mixtures:?}; per-layer input tables {per_layer_tables:?}");
    assert!(lowered >= 40, "{lowered}");
    assert!(mixtures.len() >= 8, "{mixtures:?}");
}
