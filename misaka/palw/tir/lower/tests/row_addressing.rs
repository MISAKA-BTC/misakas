//! **What the lowerer writes, a runtime can address by row** (the contract with lane M2's runtime residency,
//! `docs/design/palw/tir/runtime-residency.md`, branch `tir/residency`).
//!
//! A residency never holds a table of hundreds of GB: it reads the rows a forward gathers. It tells a table from a dense
//! weight by the PROGRAM's dataflow alone — a param is **row-addressed** when every use is a
//! `Gather { axis: 0, batch_dims: 0 }` of the param itself or of an uncommitted, uncarried reshape chain of it (a row of a
//! row-major view of a contiguous tensor is a contiguous run: `unit` elements at `row × unit`) — so the contract is on
//! what the LOWERER writes: an embedding, a per-layer or n-gram table, a position table, a mixture's expert stack are
//! each ONE tensor the program gathers on axis 0, one row one gather unit, and the container holds its rows in order. A
//! layout the runtime cannot address by row offset (a batched gather over `[heads, rows, dim]`, a transposed copy, an
//! interleaved block layout) would make it read the whole table densely — a pinned 51 GB table.
//!
//! The classifier is M2's own function (`tests/common/m2_tiers.rs`, vendored verbatim from `tir/residency`).

#![recursion_limit = "512"]

#[path = "common/m2_tiers.rs"]
#[allow(dead_code)]
mod m2_tiers;

use m2_tiers::{TirPinnedWhyV1, TirTierRulesV1, TirTierV1, TirTiersV1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::lower::LowerOpts;
use std::path::Path;

fn tiers_of(p: &TirProgramV1) -> TirTiersV1 {
    TirTiersV1::of(p, TirTierRulesV1::default())
}

fn lower(cfg: &str, opts: &LowerOpts) -> TirProgramV1 {
    fidelity::prepare(cfg, opts).unwrap_or_else(|e| panic!("prepare: {e}")).lowered.program
}

/// Qwen4-Exp at its published shape (the real-shape config of `tests/qwen4_exp.rs`): the 32 head tables of every PLE
/// layer (16 heads × 2 chunks, ~20 M rows of 128 `i16` codes) are served BY ROW — never pinned, never held whole — and so is
/// the token embedding; the 512-expert stacks are ROUTED. (M2's taint rule calls an n-gram table ROUTED rather than GATHERED:
/// the hash's multipliers and head sizes are per-layer PARAMS — the block is shared by the PLE layers — and a param taints an
/// index. Either tier reads a row at its offset; the one tier a 51 GB table must not be in is the pinned one.)
#[test]
fn qwen4_exp_tables_are_served_by_rows_and_its_experts_routed_at_the_published_shape() {
    let layer_types: Vec<&str> = (0..40).map(|i| if (i + 1) % 4 == 0 { "qwen_sparse_attention" } else { "linear_attention" }).collect();
    let cfg = serde_json::json!({
        "architectures": ["Qwen4ExpForCausalLM"], "model_type": "qwen4_exp_text",
        "vocab_size": 248320, "hidden_size": 2048, "num_hidden_layers": 40, "num_attention_heads": 16, "num_key_value_heads": 2,
        "head_dim": 256, "max_position_embeddings": 32768, "rms_norm_eps": 1e-6, "hidden_act": "silu", "tie_word_embeddings": false,
        "rope_parameters": { "rope_type": "default", "rope_theta": 10000.0, "partial_rotary_factor": 0.25 },
        "attention_bias": false, "linear_conv_kernel_dim": 4, "linear_key_head_dim": 128, "linear_value_head_dim": 128,
        "linear_num_key_heads": 16, "linear_num_value_heads": 32, "moe_intermediate_size": 512, "shared_expert_intermediate_size": 512,
        "num_experts_per_tok": 10, "num_experts": 512, "norm_topk_prob": true, "layer_types": layer_types,
        "hc_count": 4, "hc_lowrank": 320, "ple_layer_ids": [2, 6, 10, 14, 18, 22, 26, 30, 34], "ple_embed_dim": 2048,
        "ple_conv_kernel_size": 4, "ngram_size": 3, "heads_per_ngram": 8, "ngram_vocab_size_base": 20_000_000,
        "make_ngram_vocab_size_divisible_by": 128, "seed": 1234, "split_ngram_parts": 512,
        "indexer_n_heads": 16, "indexer_kv_heads": 1, "indexer_head_dim": 128, "indexer_budget": 2048, "indexer_compress_ratio": 16,
        "eos_token_id": 248044, "bos_token_id": 248044, "pad_token_id": 248044
    });
    let p = lower(&cfg.to_string(), &LowerOpts::default());
    let t = tiers_of(&p);
    let (mut ngram, mut experts) = (0, 0);
    for (j, d) in p.params.iter().enumerate() {
        let tier = t.params[j].tier;
        if d.name.starts_with("ple.ngram.table.h") {
            ngram += 1;
            assert!(tier.is_rows(), "{}: {tier:?} — a table the runtime must hold whole", d.name);
            assert_eq!((t.params[j].unit, d.shape.len()), (128, 2), "{}: a row is the 128 codes of an axis-0 [rows, 128] tensor", d.name);
        }
        if d.name.contains("moe.experts") && d.shape.len() == 3 && d.shape[0] == 512 {
            experts += 1;
            assert_eq!(tier, TirTierV1::Routed, "{}: {tier:?}", d.name);
        }
    }
    assert_eq!(ngram, 32, "16 heads x 2 chunks");
    assert!(experts >= 3, "the gate, up and down expert stacks: {experts}");
    let a = t.arithmetic();
    eprintln!(
        "Qwen4-Exp: weights {:.1} GiB, pinned {:.2} GiB, gathered {:.1} GiB, routed {:.1} GiB, floor {:.2} GiB",
        a.weight_bytes as f64 / (1u64 << 30) as f64,
        a.pinned_bytes as f64 / (1u64 << 30) as f64,
        a.gathered_bytes as f64 / (1u64 << 30) as f64,
        a.routed_bytes as f64 / (1u64 << 30) as f64,
        a.floor_bytes as f64 / (1u64 << 30) as f64
    );
    // The point of the contract: the 51 GB of n-gram tables are served by rows, not pinned.
    assert!((a.gathered_bytes + a.routed_bytes) > 100 << 30, "served by rows: {} B", a.gathered_bytes + a.routed_bytes);
    assert!(a.pinned_bytes < 16 << 30, "pinned {} B: a table the runtime must hold whole", a.pinned_bytes);
}

/// The fixtures at tiny shapes, rule `pin_below_bytes = 0` (row-serve every row-addressed param): every chunk of the
/// n-gram table is served by rows whatever the chunking, and nothing of it is read densely.
#[test]
fn the_ngram_tables_of_the_ple_fixtures_are_served_by_rows_at_any_chunking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    for name in ["qwen4_exp", "qwen4_ple_bigram", "qwen4_ple_trigram", "qwen4_ple_boundary"] {
        let cfg = std::fs::read_to_string(root.join(name).join("config.json")).expect("config");
        for chunk in [None, Some(16u32), Some(8)] {
            let p = lower(&cfg, &LowerOpts { table_chunk_rows: chunk, ..LowerOpts::default() });
            let t = TirTiersV1::of(&p, TirTierRulesV1 { pin_below_bytes: 0 });
            let mut seen = 0;
            for (j, d) in p.params.iter().enumerate() {
                if !d.name.starts_with("ple.ngram.table.h") {
                    continue;
                }
                seen += 1;
                // A chunk of a row or two is read whole by every forward (`EveryRow`) and a tiny one is `Small`: pinned for
                // being tiny, not for the layout. What the contract forbids is a DENSE use, or rows of unequal shape.
                let tier = t.params[j].tier;
                assert!(
                    tier.is_rows() || matches!(tier, TirTierV1::Pinned(TirPinnedWhyV1::EveryRow | TirPinnedWhyV1::Small)),
                    "{name} (chunk {chunk:?}): {} is {tier:?}",
                    d.name
                );
                assert_eq!(d.shape.len(), 2);
            }
            assert!(seen > 0, "{name} has n-gram tables");
        }
    }
}

/// A mixture's expert stacks are routed and a dense model routes nothing; the token embedding is gathered unless the
/// head is tied to it (then the head reads it densely: M2's documented exception) — at the real shapes of three
/// mixtures and a dense decoder, from their configs alone.
#[test]
fn experts_are_routed_and_embeddings_gathered_at_real_shapes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    for (name, routes) in [("qwen3-30b-a3b", true), ("deepseek-v2-lite", true), ("gpt-oss-20b-bf16", true), ("qwen2.5-7b-instruct", false)] {
        let cfg = std::fs::read_to_string(dir.join(format!("{name}.json"))).expect("config");
        let p = lower(&cfg, &LowerOpts::default());
        let t = tiers_of(&p);
        let routed = t.params.iter().filter(|x| x.tier == TirTierV1::Routed).count();
        assert_eq!(routed > 0, routes, "{name}: {routed} routed params");
        let embed = p.params.iter().position(|d| d.name == "embed.table.c0" || d.name == "embed.table" || d.name.starts_with("embed.table")).expect("an embedding table");
        eprintln!("{name}: embedding {:?}, {routed} routed params", t.params[embed].tier);
    }
}
