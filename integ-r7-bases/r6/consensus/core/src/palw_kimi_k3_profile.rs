//! **Kimi K3 as an adjudicable execution class** — native geometry, not the Qwen36 stand-in.
//!
//! ADR-0097 §1.3 wrote Kimi as `stand_ins::KIMI_K3_AS_HYBRID_V1`: 92 layers, KDA as GatedDeltaNet,
//! MLA as 6-KV GQA. That constant is still the fit-table's stand-in and **must not be registered**.
//! This module is the graph a class would actually declare:
//!
//! * 93 layers, 24 MLA at `i % 4 == 0` (layers 0,4,…,92) and 69 KDA.
//! * MLA cache `kv_lora_rank + qk_rope = 576` a position, 96 heads, `v_head_dim` 128.
//! * 896 routed experts, top-16, two shared, hidden 7168, vocab 163_840.
//! * Registered **job** context is 10 (ADR-0097's RC fit). The card's 1,048,576 is `card_n_ctx`
//!   and is not the class's `n_ctx` — the geometry ceiling refuses that product.
//!
//! `PalwShapeProfileV3` cannot spell `i % 4 == 0` with its V1 interval phase `(i+1) % interval`.
//! Kimi rows therefore carry [`crate::palw_step::PALW_STEP_OBJECT_VERSION_V2`]. Existing V1
//! class ids do not move.

use crate::Hash64;
use crate::palw_kimi_k3_ops::{KIMI_K3_EXPERTS_PER_TOKEN, KIMI_K3_NUM_EXPERTS, KIMI_K3_SHARED_EXPERTS};
use crate::palw_kimi_k3_tokenizer_v1::{KimiK3TokenizerSpecV1, kimi_k3_tokenizer_id_v1};
use crate::palw_qwen36_profile::QWEN36_WEIGHT_DTYPE_I8;
use crate::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V2, PalwShapeProfileV3,
    PalwStepError, PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_EMBED, KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM, KDESC_KIMI_KDA_STEP, KDESC_KIMI_MLA_FUSED,
    KDESC_KIMI_MOE_COMBINE, KDESC_KIMI_ROUTER_TOPK, KDESC_Q36_DECAY, KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_SIGMOID,
};
use borsh::BorshSerialize;

/// Native card geometry. `n_ctx` here is the **registered job width**, not the card's 1M.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwKimiK3GeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub attn_heads: u16,
    pub attn_head_dim: u32,
    pub qk_rope_head_dim: u32,
    pub kv_lora_rank: u32,
    pub v_head_dim: u32,
    pub kda_heads: u16,
    pub kda_head_dim: u32,
    pub kda_conv_kernel: u16,
    pub n_experts: u32,
    pub experts_per_token: u32,
    pub shared_experts: u32,
    pub moe_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    /// The card's advertised context. Not what `validate_geometry` sees.
    pub card_n_ctx: u32,
    pub n_threads: u32,
    pub rms_eps_q: i64,
    pub tile_len: u32,
    pub rope_freq_base_bits: u32,
}

/// Public card (ADR-0097 §1.3), registered at the RC fit of 10 positions.
pub const KIMI_K3_CARD: PalwKimiK3GeometryV1 = PalwKimiK3GeometryV1 {
    layer_count: 93,
    hidden_dim: 7168,
    attn_heads: 96,
    attn_head_dim: 128,
    qk_rope_head_dim: 64,
    kv_lora_rank: 512,
    v_head_dim: 128,
    kda_heads: 56,
    kda_head_dim: 128,
    kda_conv_kernel: 4,
    n_experts: KIMI_K3_NUM_EXPERTS as u32,
    experts_per_token: KIMI_K3_EXPERTS_PER_TOKEN as u32,
    shared_experts: KIMI_K3_SHARED_EXPERTS as u32,
    moe_dim: 3072,
    vocab_size: 163_840,
    n_ctx: 10,
    card_n_ctx: 1_048_576,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 128,
    rope_freq_base_bits: 0x4B18_9680,
};

/// MLA layers: `i % 4 == 0` over 93 layers → 24 MLA, 69 KDA. Matches PalwShapeProfile V2 phase.
pub fn kimi_k3_layer_is_mla_v1(layer: u16) -> bool {
    layer.is_multiple_of(4)
}

pub fn kimi_k3_layer_spans_v1(g: PalwKimiK3GeometryV1) -> (usize, usize) {
    let mla = (0..g.layer_count).filter(|&i| kimi_k3_layer_is_mla_v1(i)).count();
    (g.layer_count as usize - mla, mla)
}

/// Canonical job the class tells producer and seat to run. Fits `n_ctx` 10.
pub const KIMI_K3_RC_CANONICAL: (u32, u32) = (8, 2);

fn dtypes(span: usize) -> Vec<u8> {
    vec![QWEN36_WEIGHT_DTYPE_I8; span]
}

fn fixed(elements: u32) -> PalwStepOutLenV1 {
    PalwStepOutLenV1::Fixed { elements }
}

fn node(
    op: PalwStepOpKindV1,
    kernel: &'static str,
    weight: &'static str,
    out: PalwStepOutLenV1,
    inputs: Vec<u16>,
    span: usize,
    tile: u32,
    role: PalwStepNodeRoleV1,
) -> PalwStepNodeV1 {
    let named = !weight.is_empty();
    PalwStepNodeV1 {
        op_kind: op,
        role,
        weight_name: weight.to_string(),
        weight_dtypes: if named { dtypes(span) } else { Vec::new() },
        out_len: out,
        tile_len: tile.max(crate::palw_step::PALW_STEP_MIN_TILE_LEN),
        kernel_semantics_id: kernel_semantics_id_v1(kernel),
        input_refs: inputs,
    }
}

fn kda_table(g: PalwKimiK3GeometryV1, span: usize) -> Vec<PalwStepNodeV1> {
    let hidden = fixed(g.hidden_dim);
    let kdim = fixed(g.kda_heads as u32 * g.kda_head_dim);
    let vdim = kdim;
    let heads = fixed(g.kda_heads as u32);
    let experts = fixed(g.n_experts);
    let topk = fixed(2 * g.experts_per_token);
    let routed = fixed(g.experts_per_token * g.hidden_dim);
    let shared = fixed(g.shared_experts * g.hidden_dim);
    let t = g.tile_len;
    let min = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
    let n = |op, k, w, out, ins: &[u16]| node(op, k, w, out, ins.to_vec(), span, t, PalwStepNodeRoleV1::Plain);
    vec![
        n(PalwStepOpKindV1::RmsNorm, KDESC_A16_RMS_NORM, "", hidden, &[PALW_STEP_INPUT_LAYER_IN]),
        n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.kda_norm.a16", hidden, &[0]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_q.weight", kdim, &[1]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_k.weight", kdim, &[1]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_v.weight", vdim, &[1]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_dt.weight", heads, &[1]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_beta.weight", heads, &[1]),
        n(PalwStepOpKindV1::Softplus, KDESC_Q36_DECAY, "", heads, &[5]),
        n(PalwStepOpKindV1::Sigmoid, KDESC_Q36_SIGMOID, "", heads, &[6]),
        node(
            PalwStepOpKindV1::GatedDeltaNet,
            KDESC_KIMI_KDA_STEP,
            "blk.{layer}.kda_step.a16",
            vdim,
            vec![3, 4, 2, 7, 8],
            span,
            g.kda_head_dim,
            PalwStepNodeRoleV1::Plain,
        ),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.kda_o.weight", hidden, &[9]),
        n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", hidden, &[PALW_STEP_INPUT_LAYER_IN, 10]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_router.weight", experts, &[11]),
        n(PalwStepOpKindV1::SoftMax, KDESC_KIMI_ROUTER_TOPK, "blk.{layer}.ffn_router_topk.a16", topk, &[12]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_up_exps.routed", routed, &[11]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_shared.weight", shared, &[11]),
        node(
            PalwStepOpKindV1::MulElem,
            KDESC_KIMI_MOE_COMBINE,
            "blk.{layer}.ffn_combine.a16",
            hidden,
            vec![14, 15, 13],
            span,
            min,
            PalwStepNodeRoleV1::Plain,
        ),
        n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", hidden, &[11, 16]),
    ]
}

fn mla_table(g: PalwKimiK3GeometryV1, span: usize) -> Vec<PalwStepNodeV1> {
    let hidden = fixed(g.hidden_dim);
    let qdim = fixed(g.attn_heads as u32 * g.attn_head_dim);
    let cache = fixed(g.kv_lora_rank + g.qk_rope_head_dim);
    let experts = fixed(g.n_experts);
    let topk = fixed(2 * g.experts_per_token);
    let routed = fixed(g.experts_per_token * g.hidden_dim);
    let shared = fixed(g.shared_experts * g.hidden_dim);
    let t = g.tile_len;
    let min = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
    let n = |op, k, w, out, ins: &[u16]| node(op, k, w, out, ins.to_vec(), span, t, PalwStepNodeRoleV1::Plain);
    vec![
        n(PalwStepOpKindV1::RmsNorm, KDESC_A16_RMS_NORM, "", hidden, &[PALW_STEP_INPUT_LAYER_IN]),
        n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_norm.a16", hidden, &[0]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_q.weight", qdim, &[1]),
        node(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_Q36_MATMUL_GROUPED,
            "blk.{layer}.attn_kv_lora.weight",
            cache,
            vec![1],
            span,
            t,
            PalwStepNodeRoleV1::KCacheWrite,
        ),
        node(
            PalwStepOpKindV1::MulElem,
            KDESC_A16_REQUANTIZE,
            "blk.{layer}.attn_v_cache.a16",
            cache,
            vec![3],
            span,
            t,
            PalwStepNodeRoleV1::VCacheWrite,
        ),
        node(
            PalwStepOpKindV1::AttnFused,
            KDESC_KIMI_MLA_FUSED,
            "blk.{layer}.attn_softmax_up.a16",
            qdim,
            vec![2, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V],
            span,
            g.v_head_dim.max(crate::palw_step::PALW_STEP_MIN_TILE_LEN),
            PalwStepNodeRoleV1::Plain,
        ),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_o.weight", hidden, &[5]),
        n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", hidden, &[PALW_STEP_INPUT_LAYER_IN, 6]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_router.weight", experts, &[7]),
        n(PalwStepOpKindV1::SoftMax, KDESC_KIMI_ROUTER_TOPK, "blk.{layer}.ffn_router_topk.a16", topk, &[8]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_up_exps.routed", routed, &[7]),
        n(PalwStepOpKindV1::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_shared.weight", shared, &[7]),
        node(
            PalwStepOpKindV1::MulElem,
            KDESC_KIMI_MOE_COMBINE,
            "blk.{layer}.ffn_combine.a16",
            hidden,
            vec![10, 11, 9],
            span,
            min,
            PalwStepNodeRoleV1::Plain,
        ),
        n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", hidden, &[7, 12]),
    ]
}

/// The adjudicable profile. Version 2 so `layer_kind` uses `i % 4 == 0`.
pub fn kimi_k3_profile_v1(g: PalwKimiK3GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let (kda_span, mla_span) = kimi_k3_layer_spans_v1(g);
    if kda_span != 69 || mla_span != 24 {
        return Err(PalwStepError::ProfileNotCanonical("Kimi K3 is 69 KDA + 24 MLA over 93 layers"));
    }
    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V2,
        lane: PalwStepLaneV1::Int32,
        layer_count: g.layer_count,
        full_attention_interval: 4,
        hidden_dim: g.hidden_dim,
        ffn_dim: g.moe_dim,
        attn_heads: g.attn_heads,
        // Post-decompress the scores run as 96 independent heads. The compressed cache width
        // lives in the K/V write nodes (`kv_lora_rank + qk_rope`), not in this GQA field.
        attn_kv_heads: g.attn_heads,
        attn_head_dim: g.v_head_dim,
        rope_dims: g.qk_rope_head_dim as u16,
        rope_sections: [g.qk_rope_head_dim as u16 / 2, 0, 0, 0],
        rope_freq_base_bits: g.rope_freq_base_bits,
        rms_eps_bits: 0x3589_705F,
        l2_eps_bits: 0x3589_705F,
        base0_rms_eps_q: g.rms_eps_q,
        logits_scheme_id: crate::palw_step_refute::tiled_logits_scheme_id_v1(),
        gdn_heads: g.kda_heads,
        gdn_head_k_dim: g.kda_head_dim,
        gdn_head_v_dim: g.kda_head_dim,
        gdn_conv_kernel: g.kda_conv_kernel,
        vocab_size: g.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 1,
        kv_cache_f16: 0,
        n_ctx: g.n_ctx,
        n_batch: 1,
        n_ubatch: 1,
        n_seq: 1,
        n_threads: g.n_threads,
        pre_nodes: vec![
            node(
                PalwStepOpKindV1::EmbedLookup,
                KDESC_A16_EMBED,
                "token_embd.weight",
                fixed(g.hidden_dim),
                vec![],
                1,
                g.tile_len,
                PalwStepNodeRoleV1::Plain,
            ),
            node(
                PalwStepOpKindV1::MulElem,
                KDESC_A16_REQUANTIZE,
                "embed_lift.a16",
                fixed(g.hidden_dim),
                vec![0],
                1,
                g.tile_len,
                PalwStepNodeRoleV1::Plain,
            ),
        ],
        gdn_nodes: kda_table(g, kda_span),
        attn_nodes: mla_table(g, mla_span),
        post_nodes: vec![
            node(
                PalwStepOpKindV1::RmsNorm,
                KDESC_A16_RMS_NORM,
                "",
                fixed(g.hidden_dim),
                vec![PALW_STEP_INPUT_LAYER_IN],
                1,
                g.tile_len,
                PalwStepNodeRoleV1::Plain,
            ),
            node(
                PalwStepOpKindV1::MulElem,
                KDESC_A16_REQUANTIZE,
                "final_norm.a16",
                fixed(g.hidden_dim),
                vec![0],
                1,
                g.tile_len,
                PalwStepNodeRoleV1::Plain,
            ),
            node(
                PalwStepOpKindV1::MatMulQuant,
                KDESC_Q36_MATMUL_GROUPED,
                "output.weight",
                fixed(g.vocab_size),
                vec![1],
                1,
                g.tile_len,
                PalwStepNodeRoleV1::Plain,
            ),
        ],
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        // Hybrid: 69 KDA + 24 MLA. The map names both halves (ADR-0082 Decision 4) so a dispute at
        // either site opens a checkpoint the court can read. v3, not v4: v4 is the held regime.
        state_chunk_map_id: crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v3(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

pub fn kimi_k3_reachable_kernels_v1(g: PalwKimiK3GeometryV1) -> Result<std::collections::BTreeSet<Hash64>, PalwStepError> {
    let p = kimi_k3_profile_v1(g)?;
    Ok(crate::palw_class_admission_v2::reachable_kernels_v1(&p))
}

pub fn kimi_k3_class_id_v1() -> Hash64 {
    kimi_k3_profile_v1(KIMI_K3_CARD).expect("the card projects").shape_profile_id()
}

pub fn kimi_k3_tokenizer_id_card_v1() -> Hash64 {
    kimi_k3_tokenizer_id_v1(&KimiK3TokenizerSpecV1::CARD).expect("card tokenizer")
}

/// Runtime working set a Ready seat must hold — **not** the 2.8 T parameter file.
///
/// KDA state is O(layers × heads × k × v). MLA cache is O(attn_layers × (kv_lora + rope) × n_ctx).
/// Activated weights are the 16 routed experts + 2 shared + the dense projections, not the idle
/// 880 experts. A host that only proves it has the artifact bytes is not Ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwKimiK3WorkingSetV1 {
    pub kda_state_bytes: u64,
    pub mla_cache_bytes: u64,
    pub activated_weight_bytes: u64,
    pub artifact_bytes_lower_bound: u64,
}

impl PalwKimiK3WorkingSetV1 {
    pub fn runtime_bytes(self) -> u64 {
        self.kda_state_bytes.saturating_add(self.mla_cache_bytes).saturating_add(self.activated_weight_bytes)
    }

    /// Ready is a working-set fact. Advertising the whole artifact is neither necessary nor
    /// sufficient.
    pub fn seat_is_ready(self, resident_runtime_bytes: u64) -> bool {
        resident_runtime_bytes >= self.runtime_bytes()
    }
}

pub fn kimi_k3_runtime_working_set_v1(g: PalwKimiK3GeometryV1, n_ctx: u32) -> PalwKimiK3WorkingSetV1 {
    let (kda_layers, mla_layers) = kimi_k3_layer_spans_v1(g);
    let elem = 4u64; // i32 activations
    let kda_state_bytes = (kda_layers as u64)
        .saturating_mul(g.kda_heads as u64)
        .saturating_mul(g.kda_head_dim as u64)
        .saturating_mul(g.kda_head_dim as u64)
        .saturating_mul(elem);
    let cache_w = crate::palw_kimi_k3_ops::kimi_k3_mla_cache_width_v1(g.kv_lora_rank, g.qk_rope_head_dim) as u64;
    let mla_cache_bytes = (mla_layers as u64).saturating_mul(cache_w).saturating_mul(n_ctx as u64).saturating_mul(elem);
    let dense = (g.hidden_dim as u64).saturating_mul(g.hidden_dim as u64); // q/k/v/o-scale
    let routed = (g.experts_per_token as u64).saturating_mul(g.hidden_dim as u64).saturating_mul(g.moe_dim as u64);
    let shared = (g.shared_experts as u64).saturating_mul(g.hidden_dim as u64).saturating_mul(g.moe_dim as u64);
    let activated_weight_bytes = dense.saturating_add(routed).saturating_add(shared);
    PalwKimiK3WorkingSetV1 {
        kda_state_bytes,
        mla_cache_bytes,
        activated_weight_bytes,
        artifact_bytes_lower_bound: crate::palw_model_fit_v1::stand_ins::KIMI_K3_TOTAL_PARAMETERS,
    }
}

/// Whether the registered job (`g.n_ctx`) fits the **global** geometry ceiling. Does not lengthen
/// any window. The card's 1M context is a separate question and is refused here on purpose.
pub fn kimi_k3_fits_global_geometry_v1(g: PalwKimiK3GeometryV1) -> bool {
    kimi_k3_profile_v1(g).is_ok()
}

/// Genesis-form object a post-genesis registrant would carry. Not in any shipped genesis set.
pub fn kimi_k3_registration_v1(
    artifact_root: Hash64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
) -> Result<
    (PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_state_v2::PalwConsensusObjectV2),
    PalwStepError,
> {
    let profile = kimi_k3_profile_v1(KIMI_K3_CARD)?;
    let class_id = profile.shape_profile_id();
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, KIMI_K3_RC_CANONICAL.0, KIMI_K3_RC_CANONICAL.1);
    let ladder = crate::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES;
    let counted = crate::palw_step::step_leaf_count_capped_v1(&profile, &canonical, ladder)?;
    let entry = crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count: crate::palw_step::worst_case_step_leaf_count_capped_v1(&profile, ladder)?,
        canonical_step_leaf_count: counted,
        reachable_kernels: crate::palw_class_admission_v2::reachable_kernels_v1(&profile),
        court_cost: crate::palw_class_admission_v2::derive_court_cost_shaped_v1(
            &profile,
            crate::palw_class_admission_v2::PalwCourtCostShapeV1::genesis_anchored_v1(&profile, ladder),
        )
        .map_err(|_| PalwStepError::ProfileNotCanonical("the Kimi K3 court cost does not derive"))?,
    };
    let object = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    Ok((profile, entry, object))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_model_fit_v1::stand_ins::KIMI_K3_AS_HYBRID_V1;
    use crate::palw_qwen36_profile::qwen36_profile_v2;
    use crate::palw_step::PalwLayerKindV1;

    #[test]
    fn the_card_is_twenty_four_mla_of_ninety_three() {
        let (kda, mla) = kimi_k3_layer_spans_v1(KIMI_K3_CARD);
        assert_eq!((kda, mla), (69, 24));
        let attn: Vec<u16> = (0..93).filter(|&i| kimi_k3_layer_is_mla_v1(i)).collect();
        assert_eq!(attn.first(), Some(&0));
        assert_eq!(attn.last(), Some(&92));
        assert_eq!(attn.len(), 24);
    }

    #[test]
    fn the_profile_is_not_the_qwen36_stand_in() {
        let kimi = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let stand_in = qwen36_profile_v2(KIMI_K3_AS_HYBRID_V1);
        assert!(stand_in.is_err() || stand_in.unwrap().shape_profile_id() != kimi.shape_profile_id());
        assert_eq!(kimi.version, PALW_STEP_OBJECT_VERSION_V2);
        assert_eq!(kimi.layer_count, 93);
        assert_eq!(kimi.n_ctx, 10, "the registered job is the RC fit, not the card's 1M");
        let attn: Vec<u16> = (0..kimi.layer_count).filter(|&l| kimi.layer_kind(l) == PalwLayerKindV1::Attention).collect();
        assert_eq!(attn.len(), 24);
        assert_eq!(attn[0], 0);
    }

    #[test]
    fn the_class_id_is_stable_and_the_stand_in_must_not_register_as_it() {
        let a = kimi_k3_class_id_v1();
        let b = kimi_k3_class_id_v1();
        assert_eq!(a, b);
        assert_ne!(a, Hash64::default());
    }

    #[test]
    fn reachable_kernels_include_kimi_ops() {
        let ids = kimi_k3_reachable_kernels_v1(KIMI_K3_CARD).expect("projects");
        let fenced = crate::palw_step_refute::kimi_fenced_kernel_ids_v1();
        for d in [KDESC_KIMI_KDA_STEP, KDESC_KIMI_MLA_FUSED, KDESC_KIMI_ROUTER_TOPK, KDESC_KIMI_MOE_COMBINE] {
            assert!(ids.contains(&kernel_semantics_id_v1(d)), "missing {d}");
        }
        assert!(!ids.is_disjoint(&fenced), "the class reaches the fenced Kimi kernels");
        assert!(fenced.is_disjoint(&crate::palw_step_refute::catalogued_kernel_ids_v1()));
    }

    #[test]
    fn canonical_work_identity_does_not_follow_tiling() {
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let tok = kimi_k3_tokenizer_id_card_v1();
        let a = crate::palw_canonical_work_v1::PalwCanonicalClassDescriptorV1::of(&profile, tok).expect("dtype");
        let mut tiled = profile.clone();
        for table in [&mut tiled.pre_nodes, &mut tiled.gdn_nodes, &mut tiled.attn_nodes, &mut tiled.post_nodes] {
            for node in table.iter_mut() {
                node.tile_len = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
            }
        }
        tiled.validate_shape().expect("re-tile still validates");
        let b = crate::palw_canonical_work_v1::PalwCanonicalClassDescriptorV1::of(&tiled, tok).expect("dtype");
        assert_eq!(a.canonical_class_id_v1(), b.canonical_class_id_v1(), "tile_len is not the model");
        assert_ne!(profile.shape_profile_id(), tiled.shape_profile_id(), "the court profile still sees the tile");
    }

    #[test]
    fn canonical_work_prices_the_run_not_the_declaration() {
        use crate::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, palw_canonical_work_v1};
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let desc = PalwCanonicalClassDescriptorV1::of(&profile, kimi_k3_tokenizer_id_card_v1()).expect("dtype");
        let one = palw_canonical_work_v1(&desc, &PalwCanonicalExecutionFactsV1::uncached(8, 1)).expect("one token");
        let two = palw_canonical_work_v1(&desc, &PalwCanonicalExecutionFactsV1::uncached(8, 2)).expect("two tokens");
        assert!(two.arithmetic_mac_eq() > one.arithmetic_mac_eq(), "a second generated token is real work");
        let padded = palw_canonical_work_v1(&desc, &PalwCanonicalExecutionFactsV1::uncached(8, 1)).expect("same run");
        assert_eq!(one.arithmetic_mac_eq(), padded.arithmetic_mac_eq(), "repeating the same facts does not mint work");
        assert!(one.routed_expert_matmul > 0, "the mixture's 16 routed experts are in the derivation");
        assert!(one.recurrence > 0, "KDA is priced as recurrence, not as attention padding");
    }

    #[test]
    fn admission_refuses_kimi_until_its_fence() {
        let ids = kimi_k3_reachable_kernels_v1(KIMI_K3_CARD).expect("projects");
        assert!(!ids.is_disjoint(&crate::palw_step_refute::kimi_fenced_kernel_ids_v1()));
        assert_eq!(
            format!("{}", crate::palw_class_admission_v2::PalwClassAdmissionError::KimiFamilyNeedsItsFence),
            "the class reaches a Kimi K3 kernel and this network has not armed the Kimi family fence"
        );
    }

    #[test]
    fn registration_derives_from_the_profile() {
        let (profile, entry, obj) = kimi_k3_registration_v1(Hash64::from_u64_word(0x4B33), 0, 1, 1).expect("derives");
        assert_eq!(entry.class_id, profile.shape_profile_id());
        assert_eq!(entry.reachable_kernels, crate::palw_class_admission_v2::reachable_kernels_v1(&profile));
        let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, .. } = obj else {
            panic!("registration object");
        };
        assert_eq!(class_id, entry.class_id);
    }

    #[test]
    fn every_node_is_servable() {
        let p = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        crate::palw_catalog_coverage::verify_profile_coverage_v1(&p).expect("every reachable coordinate class adjudicates");
    }

    #[test]
    fn residency_is_the_working_set_not_the_artifact() {
        let ws = kimi_k3_runtime_working_set_v1(KIMI_K3_CARD, KIMI_K3_CARD.n_ctx);
        assert!(ws.runtime_bytes() < ws.artifact_bytes_lower_bound, "2.8T is not what a seat holds at n_ctx=10");
        assert!(ws.seat_is_ready(ws.runtime_bytes()));
        assert!(!ws.seat_is_ready(ws.runtime_bytes().saturating_sub(1)));
        assert!(!ws.seat_is_ready(0));
        let card = kimi_k3_runtime_working_set_v1(KIMI_K3_CARD, KIMI_K3_CARD.card_n_ctx);
        assert!(card.mla_cache_bytes > ws.mla_cache_bytes, "cache grows with context, not with file size");
        assert!(kimi_k3_fits_global_geometry_v1(KIMI_K3_CARD));
        let mut too_wide = KIMI_K3_CARD;
        too_wide.n_ctx = KIMI_K3_CARD.card_n_ctx;
        assert!(
            !kimi_k3_fits_global_geometry_v1(too_wide),
            "the card's 1M is refused by the global ceiling; no per-class deadline is invented"
        );
    }
}
