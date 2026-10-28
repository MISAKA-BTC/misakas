//! **RFC-0002 Phase F, step F8's report: Qwen2.5-A16 as an IR program against the legacy class**
//! (`docs/design/palw/tir/phase-f-integration.md` §2.9, §4).
//!
//! The same model — Qwen2.5-1.5B's geometry, the A16 engine — priced two ways: the legacy canonical
//! work vector over the graph-v7 shape profile (ADR-0145, `palw_canonical_work_v1`), and the IR work
//! vector over `a16_mirror_program` of the same shape (`palw_tir_work_v1`, structure only). The
//! numbers are printed (`--nocapture`) as the report the plan asks for; the test holds what must be
//! equal between the two readings of one model — the weight matmuls' MACs and the weight bytes they
//! stream, which both derive from the same dense shapes, and which agree exactly.
//!
//! **The report (2026-09-28, P = 64/512 prefill, G = 1/64).** `dense_matmul` and
//! `weight_traffic_bytes` are equal; total arithmetic is 1.3-2.1 % higher on the IR reading. The rest
//! differs by construction and the ratios are stable across jobs: `other_verified_ops` ×7.5 and
//! `normalization` ×6.4-6.8 (the IR counts every integer operation of the A16 requantisation, rotary
//! and norm segments as §8 elementwise ops, where the legacy table charges a fixed per-element weight
//! for a whole fused kernel), attention ×1.56 (the softmax's shifted exponent and requantisation are
//! H-sized elementwise ops in the IR), and the KV bytes ×0.5 (the IR reads the history's declared
//! i16 rows; the legacy term assumes 4-byte cache elements for a cache that is not f16).

use kaspa_consensus_core::palw_canonical_work_v1::{
    PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, palw_canonical_work_v1,
};
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, qwen25_a16_profile_v7};
use kaspa_consensus_core::palw_tir_work_v1::palw_tir_canonical_work_v1;
use misaka_palw_base0::artifact::Base0ShapeV1;
use misaka_palw_base0::tir_a16::a16_mirror_program;

#[test]
fn qwen25_a16_as_an_ir_program_prices_its_weights_as_the_legacy_class_does() {
    let g = QWEN25_1_5B;
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: 0,
        eps_q: g.rms_eps_q,
    };
    let program = a16_mirror_program(&shape, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).expect("the A16 program");
    let profile = qwen25_a16_profile_v7(g).expect("the graph-v7 profile");
    let descriptor =
        PalwCanonicalClassDescriptorV1::of(&profile, kaspa_consensus_core::Hash64::from_bytes([0; 64])).expect("one weight format");
    for (prefill, generated) in [(64u32, 1u32), (512, 1), (64, 64)] {
        let facts = PalwCanonicalExecutionFactsV1::uncached(prefill, generated);
        let legacy = palw_canonical_work_v1(&descriptor, &facts).expect("the legacy vector");
        let ir = palw_tir_canonical_work_v1(&program, &facts).expect("the IR vector");
        eprintln!("P {prefill:>4} G {generated:>3}");
        eprintln!("  legacy: {legacy:?}");
        eprintln!("  ir:     {ir:?}");
        let ratio = |a: u128, b: u128| if b == 0 { f64::NAN } else { a as f64 / b as f64 };
        eprintln!(
            "  ir/legacy: dense {:.4} attention {:.4} normalization {:.4} other {:.4} arithmetic {:.4} weight bytes {:.4}",
            ratio(ir.dense_matmul, legacy.dense_matmul),
            ratio(ir.attention_prefill + ir.attention_decode, legacy.attention_prefill + legacy.attention_decode),
            ratio(ir.normalization, legacy.normalization),
            ratio(ir.other_verified_ops, legacy.other_verified_ops),
            ratio(ir.arithmetic_mac_eq(), legacy.arithmetic_mac_eq()),
            ratio(ir.weight_traffic_bytes, legacy.weight_traffic_bytes),
        );
        assert_eq!(ir.routed_expert_matmul, 0);
        assert_eq!(ir.recurrence, 0);
        assert!(ir.dense_matmul > 0 && legacy.dense_matmul > 0);
        // The weights are the same shapes read the same number of times: the two derivations agree
        // on every weight MAC and every streamed weight byte, exactly.
        assert_eq!(ir.dense_matmul, legacy.dense_matmul, "dense MACs");
        assert_eq!(ir.weight_traffic_bytes, legacy.weight_traffic_bytes, "weight bytes");
    }
}
