//! **Qwen2.5 as a second deterministic execution class (Phase 0 → conditions 3 and 4).**
//!
//! `PALW-BASE-0` is the permanent liveness floor and this does not touch it. Qwen2.5 is a
//! *second* class that inherits BASE-0's integer arithmetic, its court, its artifact root and its
//! step-dispute machinery — the same closed kernel catalog, a different graph.
//!
//! # What is measured and what is chosen
//!
//! The geometry is MEASURED, from Hugging Face's own `config.json` and from the `safetensors`
//! header of the real weight file (`docs/palw-qwen25-class-phase0.md` records the readings and
//! the date). A profile that disagrees with the file describes an execution that never ran, and
//! the court would then adjudicate steps against it.
//!
//! One thing is chosen and is the user's to overrule: **`Qwen2.5-2B` does not exist.** Hugging
//! Face answers `{"error":"Invalid username or password."}` for that repository — its response
//! for one that is not there — while `Qwen2.5-1.5B` returns real metadata. The dense base family
//! is 0.5B, 1.5B, 3B, 7B, 14B, 32B, 72B. All three small members are the same architecture
//! (`Qwen2ForCausalLM`) and differ only in geometry, so the graph below is one graph; the size is
//! a constant, and [`QWEN25_1_5B`] and [`QWEN25_3B`] are both here.
//!
//! # The three transformations the artifact must record
//!
//! Qwen2.5 is not BASE-0's graph, and three of its steps have no BASE-0 op. Each is resolved by
//! an EXACT transformation applied when the artifact is built — none is an approximation, and
//! every one of them must be recorded in the artifact's quantization semantics so a verifier
//! reproduces it rather than trusting it:
//!
//! * **G1, the RMSNorm learned gain.** BASE-0's `RmsNorm` takes no weight, and neither `MulElem`
//!   nor `AddElem` can multiply by a *registered* vector (both need two opened rows). A gain
//!   followed by a linear layer is `W·diag(g)·x`, so `diag(g)` folds into `W`. Every norm here is
//!   consumed only by linear layers — `input_layernorm` by q/k/v, which all see the same gain;
//!   `post_attention_layernorm` by gate/up; `model.norm` by the tied lm_head. So there is no gain
//!   node in this graph, and that is not a simplification: the arithmetic is identical.
//! * **G2, the q/k/v bias.** BASE-0 had no additive registered term at all until `QuantParams`
//!   gained a zero point (ADR-0040 amendment). The bias rides that: `Requantize` after each of
//!   the three projections carries it per channel.
//! * **G3, RoPE's convention.** `KDESC_BASE0_ROPE` is `pinned-table-pairwise`; Qwen2 is NEOX-style
//!   `rotate_half`, pairing `(i, i + d/2)` where pairwise pairs `(2i, 2i+1)`. A fixed permutation
//!   of the head-dim axis converts one into the other, and it folds into the q and k projection
//!   rows — exact, and it leaves the adjudicated kernel untouched, which is the point.
//!
//! # What this module is not
//!
//! It is the graph, not the weights. The artifact — int8 rows, per-channel requantization
//! parameters carrying the folded biases, the pinned integer rotary table, the tokenizer
//! commitment — is Phase 2's, and no function can invent it.

use crate::Hash64;
use crate::palw_step::{
    PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3,
    PalwStepError, PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1,
    kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_BASE0_EMBED, KDESC_BASE0_MATMUL, KDESC_BASE0_REQUANTIZE, KDESC_BASE0_RMS_NORM, KDESC_BASE0_ROPE,
};

/// The int8 dtype byte. One weight type throughout: the class is integer arithmetic, and any
/// variance would mean it is not this class.
pub const QWEN25_WEIGHT_DTYPE_I8: u8 = 24;

/// A Qwen2.5 dense member's measured geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwQwen25GeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    /// **Grouped-query attention.** Every member of the family has 2, against 12–16 query heads,
    /// so this is never equal to `attn_heads` and the profile must carry both.
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    pub n_threads: u32,
    /// The integer RMS-norm epsilon in Qk. The float config says `1e-06`; the value here is the
    /// integer the class is registered with, because BASE-0's norm has no float epsilon and the
    /// court recomputes with the CLASS's constant.
    pub rms_eps_q: i64,
    pub tile_len: u32,
}

/// `Qwen2.5-1.5B`, measured 2026-08-21. The nearest existing member to the "2B" the goal names.
pub const QWEN25_1_5B: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 {
    layer_count: 28,
    hidden_dim: 1536,
    ffn_dim: 8960,
    attn_heads: 12,
    attn_kv_heads: 2,
    attn_head_dim: 128,
    vocab_size: 151_936,
    n_ctx: 4_096,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 128,
};

/// **The integer-engine shape an artifact for this geometry must have.**
///
/// The mirror of the floor's `palw_rc_base0_shape_v1`, and it exists for the same stated reason:
/// "every field is the geometry's, so the two cannot describe different classes". Before it, the
/// converter carried its own copy of the arithmetic — `eps_q: 1 << 8`, inherited from the floor —
/// while `QWEN25_1_5B` declared `rms_eps_q: 1`. That is two arithmetic specifications under one
/// model id, and it is not a cosmetic split: the engine norms with the ARTIFACT's epsilon and the
/// court re-norms with the CLASS's, so an artifact built at 256 under a class registered at 1 has
/// every honest execution convicted. This repo already has that bug in its history.
///
/// Returned as scalars because `Base0ShapeV1` lives in the engine crate, which consensus cannot
/// name. The engine crate's `classes` registry is what assembles them.
pub struct PalwQwen25ArtifactShapeV1 {
    pub n_layers: usize,
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub d_head: usize,
    pub d_ff: usize,
    pub vocab: usize,
    pub max_position: usize,
    pub eps_q: i64,
}

pub fn qwen25_artifact_shape_v1(g: PalwQwen25GeometryV1) -> PalwQwen25ArtifactShapeV1 {
    PalwQwen25ArtifactShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        // The rotary table is generated for the class's own context length, and `max_position` is
        // inside the artifact digest — so a class registered at one context and an artifact built
        // for another are two classes.
        max_position: g.n_ctx as usize,
        eps_q: g.rms_eps_q,
    }
}

/// The canonical geometry for a Hugging Face model id, or `None` for one this build does not know.
///
/// The single place a model id becomes numbers. A converter that read a checkpoint's `config.json`
/// and believed it would let the file decide the class; this makes the checkpoint something to
/// CHECK against instead.
pub fn qwen25_canonical_geometry_v1(model_id: &str) -> Option<PalwQwen25GeometryV1> {
    match model_id {
        "Qwen/Qwen2.5-1.5B" => Some(QWEN25_1_5B),
        "Qwen/Qwen2.5-3B" => Some(QWEN25_3B),
        _ => None,
    }
}

/// `Qwen2.5-3B`, measured the same day — the other reading of "2B".
pub const QWEN25_3B: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 {
    layer_count: 36,
    hidden_dim: 2048,
    ffn_dim: 11_008,
    attn_heads: 16,
    attn_kv_heads: 2,
    attn_head_dim: 128,
    vocab_size: 151_936,
    n_ctx: 4_096,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 128,
};

/// **The (tile, context) pair a given ruleset can actually adjudicate for this model** — audit
/// H-04's other half.
///
/// The two shipped constants above are the MODEL: 4,096 tokens is what Qwen2.5 has, and saying
/// otherwise would be a constant that lies about the thing it names.
/// `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context` pins that neither is
/// admissible as declared, and the measurement behind it is real — 132.4 M and 219.7 M worst-case
/// step leaves against a 4,194,304 ladder at `tile_len` 128.
///
/// What was missing is the other number: given a court, WHICH pair does this model fit into? It is
/// not a choice, it is a search, and leaving it as prose is how "either the tile grows or the
/// context shrinks" stayed a sentence instead of a value. Two ceilings pull against each other —
/// a bigger tile buys ladder depth and costs opening bytes — so the feasible set is an interval
/// per tile, and the answer is the pair with the widest context inside it.
///
/// Deterministic: tiles are tried in ascending order and the FIRST tile achieving the maximum
/// context wins, so two nodes computing this reach one geometry. `None` means the court admits
/// this model at no context at all, which is a real answer about a real ruleset.
pub fn qwen25_admissible_geometry_v1(
    model: PalwQwen25GeometryV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Option<PalwQwen25GeometryV1> {
    let fits = |tile: u32, n_ctx: u32| -> bool {
        let candidate = PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..model };
        let Ok(profile) = qwen25_profile_v1(candidate) else { return false };
        // The ladder: the whole context as prefill is the longest job the class admits.
        if crate::palw_step::worst_case_step_leaf_count_v1(&profile).map(|w| w > court.max_step_leaf_count()).unwrap_or(true) {
            return false;
        }
        // ADR-0049 Decision C: and what prosecuting one of its steps costs.
        let Ok(cost) = crate::palw_class_admission_v2::derive_court_cost_v1(&profile) else { return false };
        cost.max_opening_bytes <= court.max_opening_bytes()
            && cost.max_terminal_macs <= court.max_terminal_macs()
            && u64::from(cost.max_operand_count) <= u64::from(court.max_operand_count())
    };
    let mut best: Option<PalwQwen25GeometryV1> = None;
    let mut tile = 64u32;
    while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
        if fits(tile, 2) {
            // Widest context this tile admits. Monotone in `n_ctx` (more context is strictly more
            // leaves and never fewer opened bytes), so a binary search is exact.
            let (mut lo, mut hi) = (2u32, model.n_ctx.max(2) + 1);
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if fits(tile, mid) { lo = mid } else { hi = mid }
            }
            if best.map(|b| lo > b.n_ctx).unwrap_or(true) {
                best = Some(PalwQwen25GeometryV1 { n_ctx: lo, tile_len: tile, ..model });
            }
        }
        tile *= 2;
    }
    best
}

/// The tensor names this graph consumes. `{layer}` is substituted with the layer index.
///
/// Compare against the measured safetensors table: the norm gains are ABSENT (G1 folds them), the
/// q/k/v biases are absent as tensors and present as requantization zero points (G2), and there
/// is no `output.weight` because `tie_word_embeddings` is true — the lm_head reads the embedding
/// table. The `.requant` entries are the per-channel `(multiplier, shift, zero)` triples.
/// **Projected from the IR** (ADR-0049 Decision F), with this family's head tensor.
///
/// It was a hand-written list of 17 names beside a hand-written graph, and the graph it was
/// supposed to describe reads 27 — it omitted every narrowing (`qk_to_code.requant`,
/// `code_product.requant`, `rope_clamp.requant`, the two norm requants, the residual requants and
/// scales) exactly as the hand-written node table omitted every narrowing node. An inventory that
/// does not list a tensor the graph reads is an operand the court cannot open, and a step that
/// cannot be adjudicated.
pub fn qwen25_tensor_names_v1() -> Vec<&'static str> {
    // Tied embeddings: the head reads the embedding table, which is already first in the list, so
    // naming it here dedups to nothing and no `output.weight` is declared.
    crate::palw_base0_profile::base0_tensor_names_for_head_v1("token_embd.weight")
}


/// Qwen2.5's graph, for `geometry`.
///
/// Twenty nodes per layer, and the order IS the execution order. `input_refs` names which
/// committed material each step is recomputed from — without it a challenger could open unrelated
/// tiles as "the inputs" and manufacture a conviction.
///
/// The two cache-role nodes are the ROTATED k, not the raw projection: RoPE is applied before the
/// key enters the cache, so a later position's attention must read the rotated value. That is
/// what the roles select, and getting it backwards would have the court recompute attention
/// against unrotated keys and convict every honest producer.
pub fn qwen25_profile_v1(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let once = vec![QWEN25_WEIGHT_DTYPE_I8];
    let tile = geometry.tile_len;
    let hidden = geometry.hidden_dim;
    let q_dim = geometry.attn_heads as u32 * geometry.attn_head_dim;
    let kv_dim = geometry.attn_kv_heads as u32 * geometry.attn_head_dim;

    let plain = |kind: PalwStepOpKindV1, desc: &str, out: PalwStepOutLenV1, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: out,
        tile_len: tile,
        kernel_semantics_id: kernel_semantics_id_v1(desc),
        input_refs: refs,
    };
    let weighted = |kind: PalwStepOpKindV1,
                    desc: &str,
                    name: &str,
                    dtypes: &[u8],
                    role: PalwStepNodeRoleV1,
                    out: PalwStepOutLenV1,
                    refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role,
        weight_name: name.to_string(),
        weight_dtypes: dtypes.to_vec(),
        out_len: out,
        tile_len: tile,
        kernel_semantics_id: kernel_semantics_id_v1(desc),
        input_refs: refs,
    };
    let fixed = |n: u32| PalwStepOutLenV1::Fixed { elements: n };

    let pre_nodes = vec![weighted(
        PalwStepOpKindV1::EmbedLookup,
        KDESC_BASE0_EMBED,
        "token_embd.weight",
        &once,
        PalwStepNodeRoleV1::Plain,
        fixed(hidden),
        Vec::new(),
    )];

    // **Projected from `BASE0_LAYER_IR`, the engine's own step order** (ADR-0049 Decision F).
    //
    // This table was written here by hand, beside an engine written by hand, and the two were not
    // the same graph: it declared **27** nodes against the engine's **38**, and of the rows that
    // did land the widths diverged from the third node onward — the 1536/256 pairs traded places
    // (the grouped-query boundary sitting in different positions in the two descriptions) and the
    // FFN's 8960 arrived where the residual width was expected. Measured with
    // `qwen25-convert --check-capture`: **842 disagreements over 1068 captured rows**, so no
    // execution of this class could become a step leg, and the class could not produce a block.
    //
    // It was invisible because the only thing ever run against the real checkpoint was a FORWARD
    // PASS (`measure_depth_health`). The step capture is a different path. A model can run
    // perfectly and still be unable to commit to what it ran.
    //
    // The floor was already projected from the IR; the projection just could not express
    // grouped-query attention, because it read `PalwBase0GeometryV1`, which has no kv-head count.
    // `Base0IrGeometryV1` supplies one, and both classes now come from the same call.
    let attn_nodes = crate::palw_base0_profile::base0_ir_attn_nodes_v1(crate::palw_base0_profile::Base0IrGeometryV1 {
        layer_count: geometry.layer_count,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        tile_len: tile,
        weight_dtype: QWEN25_WEIGHT_DTYPE_I8,
    });
    debug_assert_eq!(kv_dim, geometry.attn_kv_heads as u32 * geometry.attn_head_dim);
    debug_assert_eq!(q_dim, hidden, "the query width is the hidden width for every member of this family");

    // The head, and it is THREE steps, not two. `Base0Engine::norm_to_code` is `rms_norm` followed
    // by the narrowing back to activation codes; this table declared the first and not the second,
    // which is the same omission the floor's post table carried before it was corrected — a court
    // recomputing the head would compare a Qk value against a code. The engine emits three rows
    // here and the profile now has three nodes for them.
    let post_nodes = vec![
        plain(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, fixed(hidden), vec![PALW_STEP_INPUT_LAYER_IN]),
        weighted(
            PalwStepOpKindV1::MulElem,
            KDESC_BASE0_REQUANTIZE,
            "output_norm.requant",
            &once,
            PalwStepNodeRoleV1::Plain,
            fixed(hidden),
            vec![0],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "token_embd.weight",
            &once,
            PalwStepNodeRoleV1::Plain,
            fixed(geometry.vocab_size),
            vec![1],
        ),
    ];


    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Int32,
        layer_count: geometry.layer_count,
        full_attention_interval: 1,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        rope_dims: geometry.attn_head_dim as u16,
        rope_sections: [0, 0, 0, 0],
        // Every float constant is zero and every float table is empty, and each is a property of
        // the class rather than an unfilled field: the rotary is a pinned integer table, the norm
        // epsilon is the integer `rms_eps_q`, no cache holds floats, and integer addition is
        // exactly associative so there is no FMA contraction to pin (ADR-0040 Decision E).
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: geometry.rms_eps_q,
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: geometry.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        kv_cache_f16: 0,
        n_ctx: geometry.n_ctx,
        n_batch: geometry.n_ctx,
        n_ubatch: geometry.n_ctx,
        n_seq: 1,
        n_threads: geometry.n_threads,
        pre_nodes,
        gdn_nodes: Vec::new(),
        attn_nodes,
        post_nodes,
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        state_chunk_map_id: Hash64::default(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_catalog_coverage::verify_profile_coverage_v1;
    use crate::palw_step::PalwStepTableV1;
    use crate::palw_step_refute::{catalogued_kernel_ids_v1, kernel_can_serve_node_v1};

    /// Diagnostic: the smallest tile_len that admits the declared 4096 context, projected.
    /// `cargo test -p kaspa-consensus-core --lib dump_full_ctx_tile -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_full_ctx_tile() {
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            let mut found = None;
            for tile in [16_384u32, 32_768, 65_536, 131_072, 262_144] {
                let cand = PalwQwen25GeometryV1 { tile_len: tile, ..g };
                if let Ok(p) = qwen25_profile_v1(cand)
                    && crate::palw_step::worst_case_step_leaf_count_v1(&p).is_ok()
                {
                    found = Some(tile);
                    break;
                }
            }
            println!("{name}: smallest tile admitting n_ctx {} is {found:?}", g.n_ctx);
        }
    }

    /// Diagnostic: the node table the engine's capture must land in, slot by slot.
    /// `cargo test -p kaspa-consensus-core --lib dump_qwen_slots -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_qwen_slots() {
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).unwrap();
        let g = qwen25_admissible_geometry_v1(QWEN25_1_5B, &court).unwrap();
        let p = qwen25_profile_v1(g).unwrap();
        println!("tile_len(geometry)={} pre={} attn={} post={} per-layer-attn={}",
            g.tile_len, p.pre_nodes.len(), p.attn_nodes.len(), p.post_nodes.len(), p.attn_nodes.len());
        for slot in 0..14u32 {
            if let Some((n, layer)) = p.resolve_node_slot(slot) {
                let w = match n.out_len {
                    crate::palw_step::PalwStepOutLenV1::Fixed { elements } => format!("Fixed {elements}"),
                    crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => format!("KvScaled x{multiplier}"),
                };
                println!("  slot {slot:3} layer {layer:?} {:?} out_len={w} tile_len={}", n.op_kind, n.tile_len);
            }
        }
    }

    /// **Would the chain admit this class?** — the four checks `verify_class_admission_v2` runs,
    /// against the SHIPPED bundle, on the geometry a Qwen2.5-1.5B registration would actually use.
    ///
    /// Run before deploying anything: a registration that fails here fails on-chain, and finding
    /// that out with a 1.7 GiB artifact already on four hosts is finding it in the worst place.
    /// The artifact root is a placeholder because the gate does not check it — that binding is the
    /// producer's (its attempt must name the class's registered root) and the genesis gate's.
    #[test]
    fn the_admissible_qwen25_class_passes_the_admission_gate() {
        let bundle = match &crate::config::params::palw_rc_shipped_params().palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
            _ => return, // a build whose card is unset ships no bundle; nothing to check against
        };
        let admissible = qwen25_admissible_geometry_v1(QWEN25_1_5B, &bundle.court)
            .expect("1.5B has an admissible (tile, n_ctx) under the shipped court");
        let profile = qwen25_profile_v1(admissible).expect("expressible");
        let class_id = profile.shape_profile_id();
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, 8, 4);
        let registration = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: crate::Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            // COUNTED from the canonical job, never declared — the gate recounts and refuses a
            // mismatch, which is what makes `pwu_per_inference` a fact rather than a multiplier
            // the registrant picks. (First run of this test declared 1 and was told 366,184.)
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 {
                pwu_per_inference: crate::palw_step::step_leaf_count(&profile, &canonical).expect("the canonical job has a step space"),
            },
            initial_target: u128::MAX / 2,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        };
        match crate::palw_class_admission_v2::verify_class_admission_v2(&bundle, &profile, &canonical, &registration) {
            Ok(entry) => {
                assert_eq!(entry.class_id, class_id);
                println!(
                    "ADMITTED  class_id={class_id}  tile={} n_ctx={} canonical_leaves={}",
                    admissible.tile_len, admissible.n_ctx, entry.canonical_step_leaf_count
                );
            }
            Err(e) => panic!(
                "the admissible Qwen2.5-1.5B geometry is REFUSED by the gate it was derived for: {e:?}\n\
                 (tile={} n_ctx={})",
                admissible.tile_len, admissible.n_ctx
            ),
        }
    }

    /// **Print the admissible geometry a class must actually register at.** The constants above
    /// are the MODEL's shape; `n_ctx 4096` at `tile_len 128` is far past `PALW_STEP_MAX_LEAVES`,
    /// so a class registered at the model's own numbers is refused. Run when picking the pair:
    /// `cargo test -p kaspa-consensus-core --lib print_admissible -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_admissible_qwen25_geometry() {
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("the shipped court");
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            match qwen25_admissible_geometry_v1(g, &court) {
                Some(a) => {
                    let p = qwen25_profile_v1(a).expect("expressible");
                    println!(
                        "{name}: tile_len={} n_ctx={} class_id={} (model declared tile={} n_ctx={})",
                        a.tile_len, a.n_ctx, p.shape_profile_id(), g.tile_len, g.n_ctx
                    );
                }
                None => println!("{name}: NO admissible (tile, n_ctx) under this court"),
            }
        }
    }

    /// **Condition 3: the profile derives from the MEASURED geometry**, for both readings of "2B".
    #[test]
    fn the_profile_derives_from_the_measured_geometry() {
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            let p = qwen25_profile_v1(g).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(p.layer_count, g.layer_count);
            assert_eq!(p.hidden_dim, g.hidden_dim);
            assert_eq!(p.ffn_dim, g.ffn_dim);
            assert_eq!(p.vocab_size, 151_936, "{name}: the family shares one vocabulary");
            // GQA is real and the profile carries both counts: 2 kv heads against 12 or 16.
            assert_eq!(p.attn_kv_heads, 2);
            assert!(p.attn_heads > p.attn_kv_heads, "{name}: grouped-query attention, not multi-head");
            assert_eq!(p.hidden_dim, p.attn_heads as u32 * p.attn_head_dim, "{name}: q width is the hidden width");
            // Every layer is attention: no GatedDeltaNet arm in this architecture.
            assert_eq!(p.table_layer_span(PalwStepTableV1::Attn), g.layer_count as usize);
            assert!(p.gdn_nodes.is_empty());
            assert_eq!(p.lane, PalwStepLaneV1::Int32, "{name}: an integer class commits integer codes");
        }
        // Two geometries are two classes, and the id says so.
        assert_ne!(
            qwen25_profile_v1(QWEN25_1_5B).unwrap().shape_profile_id(),
            qwen25_profile_v1(QWEN25_3B).unwrap().shape_profile_id()
        );
        // …and neither is BASE-0.
        let base0 = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
        assert_ne!(qwen25_profile_v1(QWEN25_1_5B).unwrap().shape_profile_id(), base0.shape_profile_id());
    }

    /// **Condition 4: the coverage gate passes, 100%.**
    ///
    /// Against `catalogued_kernel_ids_v1()` and `kernel_can_serve_node_v1` — the adjudication
    /// table and the adjudicator's own statement of what it can serve — never a restated list.
    /// This is the check a *float* Qwen profile fails: no float quantized matmul is catalogued at
    /// all, which is why this class is integer arithmetic and not llama.cpp's kernels.
    #[test]
    fn the_coverage_gate_passes_on_the_whole_graph() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        // **Coverage over COORDINATES, not kernel ids** (ADR-0049 Decision D) — and it passes only
        // because Decision E landed with it. The gate swept prefill and decode, the embedding
        // gather refused every decode position, and this class's canonical job decodes; the tripwire
        // here asserted that refusal for exactly as long as it was true. What made it false is that
        // a decode token is now pinned by the claim's own `full_logits_trace_root` — which already
        // bound `output_token_ids_hash_v2` — rather than by nothing.
        verify_profile_coverage_v1(&p).expect("every reachable coordinate class adjudicates, decode included");

        let catalogued = catalogued_kernel_ids_v1();
        let mut checked = 0;
        for (name, nodes) in [("pre", &p.pre_nodes), ("gdn", &p.gdn_nodes), ("attn", &p.attn_nodes), ("post", &p.post_nodes)] {
            for node in nodes {
                assert!(catalogued.contains(&node.kernel_semantics_id), "{name}: {:?} names an uncatalogued kernel", node.op_kind);
                kernel_can_serve_node_v1(node, name == "pre").unwrap_or_else(|e| panic!("{name}: {:?}: {e}", node.op_kind));
                checked += 1;
            }
        }
        // Tracks the IR, because the layer table IS the IR now (ADR-0049 Decision F) — a literal
        // here is how the hand-written table's 27 nodes went unnoticed against the engine's 38.
        assert_eq!(
            checked,
            1 + crate::palw_base0_profile::BASE0_LAYER_IR.len() + 3,
            "the whole graph was checked, not a prefix"
        );
    }

    /// **The three transformations, asserted as absences.**
    ///
    /// G1, G2 and G3 are exact and applied when the artifact is built, so the way they show up
    /// here is that certain nodes are NOT in the graph. Asserting the absence is what stops one of
    /// them being quietly re-added as an unadjudicable op later.
    #[test]
    fn the_folded_transformations_leave_no_node_behind() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let names: Vec<&str> =
            [&p.pre_nodes, &p.attn_nodes, &p.post_nodes].into_iter().flatten().map(|n| n.weight_name.as_str()).collect();

        // G1: no norm gain tensor is consumed anywhere — it folded into the following linears.
        assert!(!names.iter().any(|n| n.contains("norm.weight")), "a norm gain node would mean G1 was not folded");
        // G2: no bias tensor either — the biases ride the requantize zero points.
        assert!(!names.iter().any(|n| n.ends_with(".bias")), "a bias tensor would mean G2 was not folded");
        assert!(names.iter().filter(|n| n.ends_with("attn_q.requant")).count() == 1, "q's bias has a home");
        // G3: the rotary is the pinned table, and the ONLY rope kernel is BASE-0's pairwise one.
        for node in &p.attn_nodes {
            if node.op_kind == PalwStepOpKindV1::RopeImrope {
                assert_eq!(node.kernel_semantics_id, kernel_semantics_id_v1(KDESC_BASE0_ROPE));
                assert_eq!(node.weight_name, "blk.{layer}.rope_table");
            }
        }
        // Tied embeddings: the lm_head reads the embedding table, and no `output.weight` exists.
        // Found by op kind rather than by index: the post table gained its missing narrowing, and
        // an index would have moved silently.
        let head = p
            .post_nodes
            .iter()
            .find(|n| n.op_kind == PalwStepOpKindV1::MatMulQuant)
            .expect("the head is a matmul");
        assert_eq!(head.weight_name, "token_embd.weight", "tie_word_embeddings is true");
        assert!(!names.contains(&"output.weight"));
    }

    /// The cache roles sit on the ROTATED key, not the raw projection — a later position's
    /// attention reads rotated keys, and a court recomputing against unrotated ones would convict
    /// every honest producer.
    #[test]
    fn the_cache_roles_name_the_rotated_key_and_the_requantized_value() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let k = p.attn_nodes.iter().position(|n| n.role == PalwStepNodeRoleV1::KCacheWrite).expect("a K cache node");
        // The cached key is the rotated one, NARROWED — what a later position reads is an int8
        // code, not the Qk value rope produces, so the role sits on the narrowing that follows the
        // rotation. Asserted through the input chain rather than by node kind: the hand-written
        // table put the role on the raw rope output, and "is it a RopeImrope" was the check that
        // let that pass.
        assert_eq!(p.attn_nodes[k].op_kind, PalwStepOpKindV1::MulElem, "the cached key is a narrowing");
        assert_eq!(p.attn_nodes[k].weight_name, "blk.{layer}.rope_clamp.requant");
        let feeder = p.attn_nodes[k].input_refs[0] as usize;
        assert_eq!(
            p.attn_nodes[feeder].op_kind,
            PalwStepOpKindV1::RopeImrope,
            "and what it narrows is the rotation — an unrotated cached key convicts every honest producer"
        );
        let v = p.attn_nodes.iter().position(|n| n.role == PalwStepNodeRoleV1::VCacheWrite);
        assert!(v.is_none() || p.attn_nodes[v.unwrap()].op_kind != PalwStepOpKindV1::RopeImrope, "no rotation applies to V");
        // Exactly one node per role, or "the K cache" names two things.
        assert_eq!(p.attn_nodes.iter().filter(|n| n.role == PalwStepNodeRoleV1::KCacheWrite).count(), 1);
    }

    /// A Qwen-shaped geometry small enough to commit a whole step leg for.
    ///
    /// The STRUCTURE is the real thing — grouped-query attention with a real group, the cache
    /// role on the rotated key, kv-scaled scores — because that is what conditions 9 and 10 are
    /// about. The dimensions are not: a leg over 28 layers and a 151,936-wide vocabulary is not a
    /// thing a test commits.
    fn probe_geometry() -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: 1,
            hidden_dim: 32,
            ffn_dim: 64,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 8,
            vocab_size: 48,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1 << 8,
            tile_len: 16,
        }
    }

    fn probe_context(profile: &PalwShapeProfileV3) -> crate::palw_v2::PalwJobContextV2 {
        let mut ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"qwen25-probe".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [5; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 2,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        ctx
    }

    /// **Condition 9: a Qwen-shaped execution commits a step trace with a Merkle root.**
    ///
    /// Every leaf is a canonical coordinate of THIS profile, in canonical order, and the builder
    /// refuses anything else — so the root commits to the graph, not merely to some bytes.
    #[test]
    fn a_qwen_execution_commits_a_step_leg() {
        use crate::palw_step::{canonical_step_coordinates, step_leaf_count};
        use crate::palw_step_leg::PalwStepLegBuilderV1;
        let p = qwen25_profile_v1(probe_geometry()).unwrap();
        let ctx = probe_context(&p);

        let expected = step_leaf_count(&p, &ctx).expect("the leaf space is countable");
        assert!(expected > 0);
        let mut builder = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).expect("a Qwen profile builds a leg");
        assert_eq!(builder.expected_main_leaves(), expected, "the builder and the counter agree");

        // Deterministic filler: a court reads ONE step, so the other tiles are opaque bytes to it.
        // What matters here is that every coordinate is canonical and the order is enforced.
        for i in 0..expected {
            let coord = canonical_step_coordinates(&p, &ctx, i).expect("every index is a coordinate");
            let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
            let width = builder_tile_width(&p, &ctx, &coord);
            let values: Vec<u32> = (0..width).map(|j| ((i * 7 + j as u64 + 1) % 97) as i32 as u32).collect();
            builder.push_step_tile(coord, &values).unwrap_or_else(|e| panic!("leaf {i} ({:?}): {e:?}", node.op_kind));
        }
        let material = builder.finish().expect("the leg closes");
        assert_eq!(material.leaf_count, expected);
        assert_ne!(material.merkle_root, Hash64::default(), "and it has a root to commit");

        // Out of order is refused, which is what makes the root a commitment to the ORDER too.
        let mut wrong = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        let second = canonical_step_coordinates(&p, &ctx, 1).unwrap();
        let width = builder_tile_width(&p, &ctx, &second);
        assert!(wrong.push_step_tile(second, &vec![0u32; width as usize]).is_err(), "leaf 1 cannot be pushed first");
    }

    /// **Condition 10: the court adjudicates a Qwen step from ONE tile opening and no model.**
    ///
    /// The node challenged is `attn_output` — a weighted matmul, so the court needs weights, and
    /// the ONLY weights it gets are the bytes a Merkle path binds to the class's registered
    /// artifact root. No checkpoint is opened, no model is loaded, and the 1.5B artifact is
    /// nowhere near this test.
    ///
    /// Both directions are asserted from the same fixture: the honest commitment recomputes to
    /// `NoFaultFound`, and one corrupted value convicts with `ComputationMismatch` at exactly
    /// that value's index. A court that only ever produced one of the two would be useless in a
    /// different way each time.
    #[test]
    fn a_qwen_step_is_adjudicated_from_one_tile_and_no_model() {
        use crate::palw_artifact::{PalwArtifactOperandV1, PalwProvenOperandsV1, artifact_leaf_v1, artifact_root_v1};
        use crate::palw_step::{PalwStepCoordinateV1, canonical_step_coordinates, canonical_step_leaf_index};
        use crate::palw_step_leg::{
            PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepLegBuilderV1, PalwStepTileLeafV1, checkpoint_empty_root_v2,
            checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1, step_opening_v1,
        };
        use crate::palw_legs::PalwCheckpointProfileV1;
        let g = probe_geometry();
        let p = qwen25_profile_v1(g).unwrap();
        let ctx = probe_context(&p);
        let total = crate::palw_step::step_leaf_count(&p, &ctx).unwrap();

        // The challenged node: `attn_output`, found by the tensor it reads rather than by a slot
        // index — the table is projected from the IR now and an index would be a second, silently
        // rotting description of where the node is.
        let out_index = p
            .attn_nodes
            .iter()
            .position(|n| n.weight_name == "blk.{layer}.attn_output.weight")
            .expect("the output projection is in the graph");
        let out_slot = 1 + out_index as u32;
        let in_slot = 1 + p.attn_nodes[out_index].input_refs[0] as u32; // its one input, the P.V result

        // The weight block the class registered, and the input row a producer committed.
        let hidden = g.hidden_dim as usize;
        let q_dim = (g.attn_heads as u32 * g.attn_head_dim) as usize;
        let weights: Vec<i8> = (0..hidden * q_dim).map(|i| (((i * 5) % 11) as i32 - 5) as i8).collect();
        let input: Vec<u32> = (0..q_dim).map(|i| ((i % 7) as i32 - 3) as u32).collect();
        let x: Vec<i8> = input.iter().map(|v| *v as i32 as i8).collect();
        let honest_out: Vec<u32> =
            crate::palw_base0_ops::matmul_quant(&weights, &x, hidden).unwrap().into_iter().map(|v| v as u32).collect();

        // Build the leg. Every leaf is canonical filler EXCEPT the challenged node's input row and
        // its output row — a court reads one step, so the rest are opaque bytes to it. That IS the
        // condition: adjudication from one opening, not from a model.
        let build = |corrupt: bool| {
            let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
            for i in 0..total {
                let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
                let width = builder_tile_width(&p, &ctx, &coord) as usize;
                let start = coord.tile_index as usize * p.attn_nodes[0].tile_len as usize;
                let values: Vec<u32> = if coord.node_slot == in_slot && coord.call_index == 1 {
                    input[start..start + width].to_vec()
                } else if coord.node_slot == out_slot && coord.call_index == 1 {
                    let mut row = honest_out[start..start + width].to_vec();
                    if corrupt && coord.tile_index == 0 {
                        row[3] = (row[3] as i32).wrapping_add(1) as u32;
                    }
                    row
                } else {
                    (0..width).map(|j| ((i * 3 + j as u64 + 1) % 61) as i32 as u32).collect()
                };
                b.push_step_tile(coord, &values).unwrap();
            }
            b.finish().unwrap()
        };

        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        let ckpt = PalwCheckpointProfileV1 {
            version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 8,
            state_layout_id: Hash64::from_u64_word(0x55),
        };
        let adjudicate = |corrupt: bool| {
            let material = build(corrupt);
            let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
            let ckpt_root = checkpoint_leg_root_v2(
                &ctx_hash,
                &ckpt.profile_hash(),
                &Hash64::default(),
                1,
                0,
                &checkpoint_empty_root_v2(&ctx_hash),
            );
            let binding = crate::palw_step_leg::PalwStepBindingV2 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                job_context: ctx.clone(),
                shape_profile: p.clone(),
                checkpoint_profile: ckpt.clone(),
                state_chunk_map_id: Hash64::default(),
                full_logits_trace_root: Hash64::from_u64_word(0xAA),
                activation_leg_root: Hash64::from_u64_word(0xBB),
                step_leaf_count: material.leaf_count,
                step_merkle_root: material.merkle_root,
                checkpoint_count: 0,
                checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
                committed_execution_root: execution_commitment_root_v2(
                    &ctx_hash,
                    &Hash64::from_u64_word(0xAA),
                    &Hash64::from_u64_word(0xBB),
                    &ckpt_root,
                    &step_root,
                ),
            };
            let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: out_slot, position: 0, tile_index: 0 };
            let out_idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
            let width = builder_tile_width(&p, &ctx, &coord) as usize;
            let mut committed = honest_out[..width].to_vec();
            if corrupt {
                committed[3] = (committed[3] as i32).wrapping_add(1) as u32;
            }
            let inputs = (0..(q_dim as u32).div_ceil(p.attn_nodes[13].tile_len))
                .map(|t| {
                    let c = PalwStepCoordinateV1 { call_index: 1, node_slot: in_slot, position: 0, tile_index: t };
                    let idx = canonical_step_leaf_index(&p, &ctx, &c).unwrap();
                    let w = builder_tile_width(&p, &ctx, &c) as usize;
                    let start = t as usize * p.attn_nodes[13].tile_len as usize;
                    crate::palw_step_refute::PalwStepInputOpeningV1 {
                        opening: step_opening_v1(&material.leaf_hashes, idx).unwrap(),
                        preimage: PalwStepTileLeafV1 {
                            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                            coord: c,
                            value_count: w as u32,
                            values_le: input[start..start + w].iter().flat_map(|v| v.to_le_bytes()).collect(),
                        },
                    }
                })
                .collect();
            let refutation = crate::palw_step_refute::PalwExecutionStepRefutationV1 {
                binding,
                output_opening: step_opening_v1(&material.leaf_hashes, out_idx).unwrap(),
                output_preimage: PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord,
                    value_count: width as u32,
                    values_le: committed.iter().flat_map(|v| v.to_le_bytes()).collect(),
                },
                inputs,
                prompt_token_ids: Vec::new(),
                decode_tokens: None,
            };
            // The class's registered inventory, and the opening that proves this block belongs.
            let operands = vec![
                // **ADR-0049 Decision B, and the reason this test's name is now true.** It opened
                // the whole `hidden x q_dim` block and called itself "from one tile". The step
                // reduces over the challenged tile's output rows alone, so that is what the
                // refutation carries: `width * q_dim` bytes at the tile's own offset. At
                // Qwen2.5-1.5B's real unembed the same change is ~223 MiB down to 192 KiB.
                PalwArtifactOperandV1 {
                    tensor_name: "blk.{layer}.attn_output.weight".to_string(),
                    layer: Some(0),
                    row_start: (coord.tile_index as usize * p.attn_nodes[out_slot as usize].tile_len as usize * q_dim) as u32,
                    bytes: weights[coord.tile_index as usize * p.attn_nodes[out_slot as usize].tile_len as usize * q_dim..]
                        [..width * q_dim]
                        .iter()
                        .map(|v| *v as u8)
                        .collect(),
                },
                PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![1, 2, 3] },
            ];
            let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
            let root = artifact_root_v1(&leaves).unwrap();
            let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
                operand: operands[0].clone(),
                leaf_index: 0,
                leaf_count: 2,
                path: vec![leaves[1]],
            }];
            let proven = PalwProvenOperandsV1::from_openings_v1(&openings, root).expect("the opening proves");
            crate::palw_step_refute::check_execution_step_refutation_v1(&refutation, &proven)
        };

        // Honest: recomputed and found correct.
        assert_eq!(
            adjudicate(false),
            Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound),
            "an honest Qwen step is NoFault"
        );
        // Fraudulent: convicted at the value that was moved, from the same one opening.
        let verdict = adjudicate(true).expect("a wrong Qwen matmul convicts");
        assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::ComputationMismatch { value_index: 3 });
    }

    /// The canonical tile width at a coordinate — the ragged last tile included."""
    fn builder_tile_width(p: &PalwShapeProfileV3, ctx: &crate::palw_v2::PalwJobContextV2, coord: &crate::palw_step::PalwStepCoordinateV1) -> u32 {
        let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
        let kv_len = if coord.call_index == 0 {
            coord.position as u64 + 1
        } else {
            ctx.declared_prefill_tokens as u64 + coord.call_index as u64
        };
        let len = match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => elements as u64,
            PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
        };
        let start = coord.tile_index as u64 * node.tile_len as u64;
        (len - start).min(node.tile_len as u64) as u32
    }

    /// The graph consumes exactly the declared inventory, so an artifact cannot be built over a
    /// different set than the one the court will open against.
    #[test]
    fn the_graph_consumes_exactly_the_declared_inventory() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let mut used: Vec<&str> = Vec::new();
        for node in [&p.pre_nodes, &p.attn_nodes, &p.post_nodes].into_iter().flatten() {
            if !node.weight_name.is_empty() && !used.contains(&node.weight_name.as_str()) {
                used.push(node.weight_name.as_str());
            }
        }
        used.sort_unstable();
        let mut declared: Vec<&str> = qwen25_tensor_names_v1();
        declared.sort_unstable();
        assert_eq!(used, declared, "the graph's operands and the declared inventory are one list");
    }

    /// **Audit H-04: "either the tile grows or the context shrinks" is a value now, not a
    /// sentence.**
    ///
    /// The shipped constants stay the MODEL — 4,096 tokens is what Qwen2.5 has, and a constant
    /// that said otherwise would lie about the thing it names. What was missing is the other
    /// number: given a court, which `(tile_len, n_ctx)` pair does the model actually fit into.
    /// Two ceilings pull against each other, so it is a search, and leaving it as prose is how the
    /// pair stayed unstated after the measurement was done.
    #[test]
    fn the_admissible_qwen_geometry_is_derived_from_the_court_that_must_adjudicate_it() {
        let floor_sized = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).unwrap();

        // Under the shipped (floor-sized) ceilings, both models fit — at a context that IS the
        // finding rather than a target.
        let small = qwen25_admissible_geometry_v1(QWEN25_1_5B, &floor_sized).expect("some pair is admissible");
        let big = qwen25_admissible_geometry_v1(QWEN25_3B, &floor_sized).expect("some pair is admissible");
        // Pinned, because these two pairs are the answer a genesis reads.
        //
        // **They moved when the graph became honest** (ADR-0049 Decision F): 1.5B was 125 and 3B
        // was 79 against a hand-written table of 27 nodes per layer, and the engine performs 38.
        // The projected graph is bigger, so the same ladder buys less context — 125 -> 90 and
        // 79 -> 56. The old numbers were not a measurement of this class; they were a measurement
        // of a graph nothing executed. `DEFAULT_MAX_OPENING_BYTES`' doc still records 125, and
        // that reference is now stale for the same reason.
        assert_eq!((small.tile_len, small.n_ctx), (64, 90), "1.5B under the shipped ceilings");
        assert_eq!((big.tile_len, big.n_ctx), (64, 56), "3B is deeper, so it buys less context for the same court");
        for (name, model, found) in [("1.5B", QWEN25_1_5B, small), ("3B", QWEN25_3B, big)] {
            let profile = qwen25_profile_v1(found).expect("expressible");
            assert!(
                crate::palw_step::worst_case_step_leaf_count_v1(&profile).unwrap() <= floor_sized.max_step_leaf_count(),
                "{name}: the derived pair must fit the ladder"
            );
            let cost = crate::palw_class_admission_v2::derive_court_cost_v1(&profile).unwrap();
            assert!(cost.max_opening_bytes <= floor_sized.max_opening_bytes(), "{name}: and the opening ceiling");
            assert!(cost.max_terminal_macs <= floor_sized.max_terminal_macs(), "{name}: and the MAC ceiling");
            assert!(found.n_ctx < model.n_ctx, "{name}: the context really did shrink — that is the finding");
            // Everything except the two knobs is the model's own.
            assert_eq!(found.layer_count, model.layer_count);
            assert_eq!(found.hidden_dim, model.hidden_dim);
            assert_eq!(found.vocab_size, model.vocab_size);
        }

        // A court with a bigger opening ceiling buys context — which is the trade a genesis makes,
        // irrevocably, and now it can be priced before it is made instead of after.
        let generous = crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            4,
            2,
            32 * 1024 * 1024,
            1 << 30,
            8,
        )
        .unwrap();
        let wider = qwen25_admissible_geometry_v1(QWEN25_1_5B, &generous).expect("admissible");
        assert!(wider.n_ctx > small.n_ctx, "a larger opening ceiling buys context: {} -> {}", small.n_ctx, wider.n_ctx);

        // And a court that admits nothing says so, rather than returning a pair nobody can use.
        let impossible =
            crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2, 1, 1, 8).unwrap();
        assert!(qwen25_admissible_geometry_v1(QWEN25_1_5B, &impossible).is_none());
    }
}
