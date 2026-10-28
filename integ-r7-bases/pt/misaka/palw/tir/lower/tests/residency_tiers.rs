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

/// **The residency of real checkpoints, from their configs alone** (`tests/configs/real/`, the
/// programs lowered at real shapes; no weight is read): the table an operator sizes a host by —
/// the weights, the pinned set, one token's routed rows, the floor, the default fifth, and what a
/// replay of `F` forwards reads (the expected union of its routes, and its gathered rows) at 845 MB/s.
/// The ratio is a property of the mixture (ADR-0112 §8): a model that routes few of many experts
/// holds its floor inside a fifth; one that routes two of eight does not, and its default stays on
/// the page cache unless a budget is stated.
#[test]
fn real_checkpoints_size_their_residency_from_the_config() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    let rules = TirTierRulesV1::default();
    let mut rows = Vec::new();
    for name in [
        "qwen3-30b-a3b",
        "qwen3-next-80b-a3b-instruct",
        "mixtral-8x7b-v0.1",
        "olmoe-1b-7b-0924",
        "qwen1.5-moe-a2.7b",
        "granite-3.1-3b-a800m",
        "gpt-oss-20b-bf16",
        "deepseek-v2-lite",
        "gemma-3n-e4b",
        "qwen2.5-7b-instruct",
    ] {
        let Ok(text) = std::fs::read_to_string(root.join(format!("{name}.json"))) else { continue };
        let Ok(prep) = fidelity::prepare(&text, &LowerOpts { max_window: Some(4096), ..Default::default() }) else {
            eprintln!("{name}: not lowered here");
            continue;
        };
        let t = TirTiersV1::of(&prep.lowered.program, rules);
        let a = t.arithmetic();
        assert_eq!(a.weight_bytes, a.pinned_bytes + a.routed_bytes + a.gathered_bytes, "{name}");
        assert_eq!(a.floor_bytes, a.pinned_bytes + a.routed_token_bytes + a.in_flight_bytes, "{name}");
        // One forward reads its token's routed rows; a long job, close to every expert.
        let (one, long) = (t.routed_union_bytes(1), t.routed_union_bytes(4097));
        assert!(
            one.abs_diff(a.routed_token_bytes) <= 1 + a.routed_token_bytes / 1_000_000,
            "{name}: {one} vs {}",
            a.routed_token_bytes
        );
        assert!(long <= a.routed_bytes && long >= one, "{name}");
        let replay = long + a.gathered_token_bytes * 4097;
        rows.push(format!(
            "{name:<30} weights {:>7.2} GiB  pinned {:>6.2}  routed {:>7.2} ({:.3} a token)  floor {:>6.2}  fifth {:>6.2} {}  replay@4097 {:>7.2} GiB = {:>5.0} s at 845 MB/s",
            gib(a.weight_bytes),
            gib(a.pinned_bytes),
            gib(a.routed_bytes),
            gib(a.routed_token_bytes),
            gib(a.floor_bytes),
            gib(a.fifth_bytes),
            if a.fifth_bytes >= a.floor_bytes { "holds" } else { "SHORT" },
            gib(replay),
            replay as f64 / 845e6
        ));
    }
    for r in &rows {
        eprintln!("{r}");
    }
    assert!(rows.len() >= 6, "{rows:?}");
}

/// **What one evaluation position costs, on the reference and on the executor** (RFC-0004 §7.2's subject
/// stage; `docs/design/palw/tir/runtime-residency.md` §8), from real checkpoints' configs (the programs
/// lowered at real shapes, no weight read). The reference interpreter takes every param a node reads
/// whole, at every node that reads it, as `i128` (its `ParamSource` door: a whole tensor per request); the
/// executor reads the pinned set where it is held, one token's routed rows (from the residency, read on a
/// miss) and one token's gathered rows — its own dtypes, nothing widened.
#[test]
fn an_evaluation_position_on_the_executor_reads_rows_where_the_reference_widens_every_weight() {
    use misaka_palw_tir::Ref;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    let mut seen = 0;
    for name in ["qwen3-30b-a3b", "qwen2.5-1.5b-instruct", "qwen2.5-7b-instruct"] {
        let Ok(text) = std::fs::read_to_string(root.join(format!("{name}.json"))) else { continue };
        let Ok(prep) = fidelity::prepare(&text, &LowerOpts { max_window: Some(4096), ..Default::default() }) else { continue };
        let p = &prep.lowered.program;
        // The reference: every param use of a position, whole.
        let (mut read, mut widened) = (0u64, 0u64);
        for (block, _) in p.occurrences() {
            for n in &p.blocks[block as usize].nodes {
                for r in &n.inputs {
                    if let Ref::Param(j) = *r {
                        let d = &p.params[j as usize];
                        let elems = d.shape.iter().fold(1u64, |a, x| a * *x as u64);
                        read += elems * d.dtype.width() as u64;
                        widened += elems * 16;
                    }
                }
            }
        }
        let a = TirTiersV1::of(p, TirTierRulesV1::default()).arithmetic();
        let executor = a.pinned_bytes + a.routed_token_bytes + a.gathered_token_bytes;
        eprintln!(
            "{name:<24} reference: {:>7.2} GiB of weights read whole, {:>8.2} GiB as i128 | executor: {:>6.3} GiB pinned + {:>6.3} GiB \
             routed rows + {:>5.1} KiB gathered = {:>6.3} GiB ({:>4.0}x less than the reference reads, {:>5.0}x less than it widens)",
            gib(read),
            gib(widened),
            gib(a.pinned_bytes),
            gib(a.routed_token_bytes),
            a.gathered_token_bytes as f64 / 1024.0,
            gib(executor),
            read as f64 / executor as f64,
            widened as f64 / executor as f64,
        );
        assert!(read >= a.weight_bytes, "{name}: the reference reads every weight whole a position");
        assert!(executor < read, "{name}");
        if a.routed_bytes > 0 {
            assert!(executor * 8 < read, "{name}: a mixture's position reads its token's experts, not every expert");
        }
        seen += 1;
    }
    assert!(seen >= 2, "{seen}");
}
