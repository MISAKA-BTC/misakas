//! **`PALW-BASE-0`'s step space, as a `PalwShapeProfileV3` (P0-8b, for the class that needs it).**
//!
//! The register's line was that no `PalwShapeProfileV3` exists for any real class — every
//! instance in the tree is a test fixture — and that the profile IS the step space's definition,
//! so without one there are no coordinates to capture tiles into and nothing recomputes
//! `execution_root` from an execution.
//!
//! Its work order said "write the Qwen3.5-2B profile". Measuring the model to write it showed
//! that cannot be the first step: a profile names a `kernel_semantics_id` per node, and
//! `verify_palw_genesis_v2` accepts a class only if every reachable kernel is in
//! `catalogued_kernel_ids_v1()`. **There is no float quantized matmul in that catalog at all** —
//! the only matmul is `base0/matmul-quant/i8xi8-i32-exact` — and no float RoPE and no float
//! softmax, while every layer of the pinned model is a `Q4_K`/`Q5_K`/`Q6_K` matmul and its six
//! attention layers are IMRoPE plus softmax. A faithful profile for it would name kernels this
//! build cannot adjudicate, and the coverage gate would refuse the class. Correctly.
//!
//! That is by design. `palw_base0_ops` opens by saying BASE-0's ops were "chosen for closability
//! rather than for parity with the float classes' graph", because integerising GatedDeltaNet,
//! IMRoPE and fused SwiGLU "would reproduce the catalog problem this class exists to escape."
//! And ADR-0039 makes BASE-0 the permanent liveness floor — so **the profile an RC genesis is
//! waiting on is this one**, and the pinned float model's is an FP-lane item blocked on
//! adjudicators existing first.
//!
//! # What this module is, and what it is not
//!
//! It is the GRAPH: ADR-0040 Decision D's plain decoder-only transformer, expressed as the four
//! node tables, with each node's kernel, tile length, output width and data inputs. Everything
//! here is a function of the class's geometry, so a registration cannot describe a graph the
//! adjudicator does not implement — [`base0_profile_v1`] names only catalogued kernels, and
//! `base0_profile_names_only_adjudicable_kernels` holds it to that against
//! `catalogued_kernel_ids_v1()` itself rather than against a restated list.
//!
//! It is NOT the weights. BASE-0's artifact inventory — the int8 weight rows, the per-tensor
//! requantization parameters, and the pinned integer sin/cos table Decision D substitutes for
//! `sinf`/`cosf` — is a registration artifact somebody produces and hashes into
//! `artifact_root`. Code cannot mint it, and a profile that pretended to would be describing an
//! execution nobody can perform.
//!
//! # Why the geometry is an argument
//!
//! For the pinned GGUF the geometry is MEASURED — a profile that disagrees with the file
//! describes an execution that never ran. BASE-0 has no file: it is a specification, and its
//! dimensions are what the network registering it chose. So they arrive as
//! [`PalwBase0GeometryV1`] and the profile is their consequence, which keeps "the RC picked these
//! numbers" separate from "this is what the graph must then look like".

use crate::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3,
    PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_BASE0_ADD_ELEM, KDESC_BASE0_EMBED, KDESC_BASE0_MATMUL, KDESC_BASE0_MUL_ELEM, KDESC_BASE0_REQUANTIZE,
    KDESC_BASE0_RMS_NORM, KDESC_BASE0_ROPE, KDESC_BASE0_SILU, KDESC_BASE0_SOFTMAX,
};
use crate::{Hash64, palw_step::PalwStepError};

/// The int8 GGML dtype byte. BASE-0 has exactly one weight dtype — that is the class — so the
/// per-layer dtype list is this value repeated, and any variance would mean it is not BASE-0.
pub const BASE0_WEIGHT_DTYPE_I8: u8 = 24;

/// The dimensions a network chooses when it registers BASE-0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBase0GeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    pub attn_head_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    pub n_threads: u32,
    /// `Qk` integer epsilon added inside `RmsNorm` before `IntRsqrt` (ADR-0040 Decision D op 3).
    /// A registration fact, not a constant: the court recomputes norms with the CLASS's epsilon,
    /// and a hardcoded one convicted every honest producer of a class that chose another.
    pub rms_eps_q: i64,
    /// Elements per committed tile. One value for the whole graph: a tile length is how finely a
    /// dispute can be localized, and per-node tuning would be a knob with consensus meaning.
    pub tile_len: u32,
}

/// The tensor names BASE-0's registration inventory must contain, with `{layer}` standing for the
/// layer index. Public because the inventory is built from them and `verify_palw_genesis_v2`'s
/// artifact root is over that inventory — one list, not two.
pub const BASE0_TENSOR_NAMES: &[&str] = &[
    "token_embd.weight",
    "blk.{layer}.attn_norm.weight",
    "blk.{layer}.attn_q.weight",
    "blk.{layer}.attn_k.weight",
    "blk.{layer}.attn_v.weight",
    "blk.{layer}.attn_output.weight",
    "blk.{layer}.attn_output.requant",
    "blk.{layer}.rope_table",
    "blk.{layer}.ffn_norm.weight",
    "blk.{layer}.ffn_gate.weight",
    "blk.{layer}.ffn_up.weight",
    "blk.{layer}.ffn_down.weight",
    "output_norm.weight",
    "output.weight",
];

/// ADR-0040 Decision D's graph, for `geometry`.
///
/// Every layer is an attention layer (`full_attention_interval = 1`), because BASE-0 is a plain
/// decoder-only transformer — there is no GatedDeltaNet arm to interleave, which is exactly the
/// simplification the class exists for. So the GDN table is empty and the attention table is the
/// per-layer template.
///
/// The node order IS the execution order, and `input_refs` names which committed material each
/// step is recomputed from — without it a challenger could open unrelated tiles as "the inputs"
/// and manufacture a conviction.
pub fn base0_profile_v1(geometry: PalwBase0GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let layers = geometry.layer_count as usize;
    let i8_per_layer = vec![BASE0_WEIGHT_DTYPE_I8; layers.max(1)];
    let i8_once = vec![BASE0_WEIGHT_DTYPE_I8];
    let tile = geometry.tile_len;
    let hidden = geometry.hidden_dim;
    let kv_dim = geometry.attn_heads as u32 * geometry.attn_head_dim;

    // A node with no weight operand.
    let plain = |kind: PalwStepOpKindV1, desc: &str, elements: u32, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements },
        tile_len: tile,
        kernel_semantics_id: kernel_semantics_id_v1(desc),
        input_refs: refs,
    };
    // A node that consumes a registered tensor. `dtypes` is one byte per layer the table covers.
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

    // --- pre: the embedding gather ---
    let pre_nodes = vec![weighted(
        PalwStepOpKindV1::EmbedLookup,
        KDESC_BASE0_EMBED,
        "token_embd.weight",
        &i8_once,
        PalwStepNodeRoleV1::Plain,
        PalwStepOutLenV1::Fixed { elements: hidden },
        Vec::new(),
    )];

    // --- the per-layer template. Slot numbers are intra-table indices; `input_refs` uses them. ---
    let attn_nodes = vec![
        // 0: pre-attention norm over the layer input.
        weighted(
            PalwStepOpKindV1::RmsNorm,
            KDESC_BASE0_RMS_NORM,
            "blk.{layer}.attn_norm.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![PALW_STEP_INPUT_LAYER_IN],
        ),
        // 1..=3: Q, K, V projections. K and V are the cache writes — the role is what makes a
        // later position's attention able to name them as inputs.
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.attn_q.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: kv_dim },
            vec![0],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.attn_k.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::KCacheWrite,
            PalwStepOutLenV1::Fixed { elements: kv_dim },
            vec![0],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.attn_v.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::VCacheWrite,
            PalwStepOutLenV1::Fixed { elements: kv_dim },
            vec![0],
        ),
        // 4: rotary, by the PINNED TABLE — ADR-0040 Decision D's central absence. The angles
        // depend only on (position, dimension), both bounded by this shape, so they are a
        // registration artifact and `sinf`/`cosf` are not in the class at all.
        weighted(
            PalwStepOpKindV1::RopeImrope,
            KDESC_BASE0_ROPE,
            "blk.{layer}.rope_table",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: kv_dim },
            vec![1],
        ),
        // 5: attention scores — one per cached key, so the width scales with the TRUE kv length
        // of the position (never the padded cache length). Its second operand is the K SERIES,
        // not a registered weight: the rotated query row multiplies the cached keys. That shape
        // was unadjudicable until G5 was closed, and `kernel_can_serve_node_v1` is what now says
        // so at registration rather than at the first dispute.
        plain(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, 0, vec![4, PALW_STEP_INPUT_KV_K]),
        // 6: the softmax over those scores, same width.
        plain(PalwStepOpKindV1::SoftMax, KDESC_BASE0_SOFTMAX, 0, vec![5]),
        // 7: the weighted sum of values — probabilities against the V series, same two-operand
        // shape.
        plain(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, kv_dim, vec![6, PALW_STEP_INPUT_KV_V]),
        // 8: output projection, 9: back to int8, 10: the residual add.
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.attn_output.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![7],
        ),
        weighted(
            PalwStepOpKindV1::MulElem,
            KDESC_BASE0_REQUANTIZE,
            "blk.{layer}.attn_output.requant",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![8],
        ),
        plain(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, hidden, vec![9, PALW_STEP_INPUT_LAYER_IN]),
        // 11: the FFN norm, 12/13: gate and up, 14: SiLU, 15: the gating multiply,
        // 16: down projection, 17: the second residual.
        weighted(
            PalwStepOpKindV1::RmsNorm,
            KDESC_BASE0_RMS_NORM,
            "blk.{layer}.ffn_norm.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![10],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.ffn_gate.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: geometry.ffn_dim },
            vec![11],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.ffn_up.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: geometry.ffn_dim },
            vec![11],
        ),
        plain(PalwStepOpKindV1::Silu, KDESC_BASE0_SILU, geometry.ffn_dim, vec![12]),
        plain(PalwStepOpKindV1::MulElem, KDESC_BASE0_MUL_ELEM, geometry.ffn_dim, vec![14, 13]),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "blk.{layer}.ffn_down.weight",
            &i8_per_layer,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![15],
        ),
        plain(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, hidden, vec![16, 10]),
    ];

    // The two `KvScaled` widths, patched in after construction so the slot comments above read as
    // the graph rather than as a list of exceptions.
    let mut attn_nodes = attn_nodes;
    attn_nodes[5].out_len = PalwStepOutLenV1::KvScaled { multiplier: 1 };
    attn_nodes[6].out_len = PalwStepOutLenV1::KvScaled { multiplier: 1 };

    // --- post: the final norm and the logits ---
    let post_nodes = vec![
        weighted(
            PalwStepOpKindV1::RmsNorm,
            KDESC_BASE0_RMS_NORM,
            "output_norm.weight",
            &i8_once,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: hidden },
            vec![PALW_STEP_INPUT_LAYER_IN],
        ),
        weighted(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            "output.weight",
            &i8_once,
            PalwStepNodeRoleV1::Plain,
            PalwStepOutLenV1::Fixed { elements: geometry.vocab_size },
            vec![0],
        ),
    ];

    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        // Int32: BASE-0 commits integer codes, and the float lanes' non-finite rule would convict
        // every negative activation of being a NaN.
        lane: PalwStepLaneV1::Int32,
        layer_count: geometry.layer_count,
        // Every layer is attention: there is no GDN arm in this class.
        full_attention_interval: 1,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        // No grouped-query attention: one kv head per query head keeps the projection widths
        // equal and the graph one template.
        attn_kv_heads: geometry.attn_heads,
        attn_head_dim: geometry.attn_head_dim,
        rope_dims: geometry.attn_head_dim as u16,
        rope_sections: [0, 0, 0, 0],
        // The three float constants a float class pins are all ZERO here, and that is a property
        // rather than a gap: BASE-0 evaluates no float transcendental and holds no float epsilon.
        // Its norm epsilon is `base0_rms_eps_q`, an integer in Qk.
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: geometry.rms_eps_q,
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: geometry.vocab_size,
        // Execution-shape flags. The repack/llamafile/fused-GDN paths are llama.cpp's and this
        // class does not run llama.cpp; flash attention is pinned disabled because the profile
        // requires it (Fact 7) and no BASE-0 attention kernel exists to enable.
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        // No cache holds floats — ADR-0040 Decision D's second deliberate absence.
        kv_cache_f16: 0,
        n_ctx: geometry.n_ctx,
        n_batch: geometry.n_ctx,
        n_ubatch: geometry.n_ctx,
        n_seq: 1,
        n_threads: geometry.n_threads,
        pre_nodes,
        // Empty, and `validate_shape` requires it to be: `full_attention_interval = 1` means no
        // layer is a GDN layer, and a table for a kind of layer that does not exist would be
        // unreachable arithmetic inside a consensus identity.
        gdn_nodes: Vec::new(),
        attn_nodes,
        post_nodes,
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        // Empty, and this is the class's whole thesis: a transcendental evaluated at registration
        // is data, and BASE-0 has no site that evaluates one at inference. `IntExp`, `IntRecip`
        // and `IntRsqrt` are integer programs in the ruleset above, not libm bindings.
        transcendental_bindings: Vec::new(),
        // Empty for the same kind of reason: integer addition is exactly associative, so there is
        // no FMA contraction to pin (ADR-0040 Decision E).
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        state_chunk_map_id: Hash64::default(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

/// **A BASE-0 catalog entry whose numbers are COUNTED from the profile, never chosen.**
///
/// `canonical_step_leaf_count` is what ADR-0045 Decision 1 makes `pwu_per_inference` normatively
/// equal to, and it is a direct, permanent multiplier on the class's fork-choice weight — so a
/// registration that could pick it could pick its own weight. `verify_palw_genesis_v2` already
/// refuses a registration whose declaration differs from the catalog's number; this is the other
/// half, which makes the catalog's number a measurement rather than a second declaration.
///
/// Both counts come from `step_leaf_count`, the same function the leg builder sizes itself with,
/// applied to the canonical job shape and to the class's worst case. Two counters would be two
/// answers, and the leg builder's is the one an execution actually has to satisfy.
///
/// `canonical` is the (prefill, decode) shape of one canonical inference — the unit the class is
/// paid per — and `worst_case` is the deepest run the ladder must still be able to walk.
pub fn base0_catalog_entry_v1(
    class_id: Hash64,
    artifact_root: Hash64,
    profile: &PalwShapeProfileV3,
    canonical: &crate::palw_v2::PalwJobContextV2,
    worst_case: &crate::palw_v2::PalwJobContextV2,
) -> Result<crate::palw_mode_v2::PalwClassCatalogEntryV2, PalwStepError> {
    let canonical_step_leaf_count = crate::palw_step::step_leaf_count(profile, canonical)?;
    let max_step_leaf_count = crate::palw_step::step_leaf_count(profile, worst_case)?;
    // Every kernel the graph can reach, read off the graph itself. A hand-maintained list here
    // would be the coverage gate certifying a set nobody derived from the thing it covers.
    let reachable_kernels = [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|node| node.kernel_semantics_id)
        .collect();
    Ok(crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count,
        canonical_step_leaf_count,
        reachable_kernels,
    })
}

/// The geometry the PALW-RC network registers BASE-0 with.
///
/// Small on purpose. ADR-0039 §2a makes BASE-0 the slow floor by design — it exists so the chain
/// can always produce and always adjudicate, not so it can be fast — and every dimension here is
/// inside the class identity, so this is the RC declaring what its floor is rather than a tuning
/// knob anyone may turn.
pub const PALW_RC_BASE0_GEOMETRY: PalwBase0GeometryV1 = PalwBase0GeometryV1 {
    layer_count: 4,
    hidden_dim: 256,
    ffn_dim: 512,
    attn_heads: 4,
    attn_head_dim: 64,
    vocab_size: 4_096,
    n_ctx: 512,
    n_threads: 1,
    rms_eps_q: 1 << 8,
    tile_len: 64,
};

/// The canonical inference BASE-0 is paid per, and the worst case its ladder must walk.
pub const PALW_RC_BASE0_CANONICAL: (u32, u32) = (8, 4);
pub const PALW_RC_BASE0_WORST_CASE: (u32, u32) = (64, 64);

/// **Everything the RC's BASE-0 registration needs, from the ONE thing code cannot mint.**
///
/// The class id, the shape profile, the catalog and its root, `pwu_per_inference`, the reachable
/// kernel set and the court catalog root are all DERIVED here — from
/// [`PALW_RC_BASE0_GEOMETRY`], from `step_leaf_count`, and from this build's own adjudication
/// table. The only input is `artifact_root`, the commitment to the int8 weights, the
/// requantization parameters and the pinned sin/cos table, because those are bytes somebody
/// produces and no function can invent.
///
/// The class id IS the shape profile id. A class is its graph: two registrations of the same
/// graph are the same class, two different graphs cannot share an id, and there is no separate
/// label anyone could pick.
pub fn palw_rc_base0_registration_v1(
    artifact_root: Hash64,
) -> Result<(PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogV2), PalwStepError> {
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY)?;
    let class_id = profile.shape_profile_id();
    let canonical = rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let worst = rc_job_context(&profile, PALW_RC_BASE0_WORST_CASE.0, PALW_RC_BASE0_WORST_CASE.1);
    let entry = base0_catalog_entry_v1(class_id, artifact_root, &profile, &canonical, &worst)?;
    let catalog = crate::palw_mode_v2::PalwClassCatalogV2::new(vec![entry])
        .map_err(|_| PalwStepError::ProfileNotCanonical("the RC catalog is not well-formed"))?;
    Ok((profile, catalog))
}

/// The job context the RC's leaf counts are taken over.
///
/// Only the shape fields matter to `step_leaf_count`; the identity fields are fixed so two nodes
/// deriving the RC's catalog reach the same numbers. It is not an execution — nothing runs this
/// context — it is the yardstick the class is measured with.
fn rc_job_context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> crate::palw_v2::PalwJobContextV2 {
    let mut ctx = crate::palw_v2::PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: Hash64::default(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    };
    ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step::PalwStepTableV1;
    use crate::palw_step_refute::catalogued_kernel_ids_v1;

    fn geometry() -> PalwBase0GeometryV1 {
        PalwBase0GeometryV1 {
            layer_count: 4,
            hidden_dim: 256,
            ffn_dim: 512,
            attn_heads: 4,
            attn_head_dim: 64,
            vocab_size: 4_096,
            n_ctx: 512,
            n_threads: 4,
            rms_eps_q: 1 << 8,
            tile_len: 64,
        }
    }

    /// **The profile exists and is well-formed** — the sentence the register said was false for
    /// every real class.
    #[test]
    fn base0_has_a_real_shape_profile() {
        let p = base0_profile_v1(geometry()).expect("BASE-0's graph is a valid shape");
        assert_eq!(p.layer_count, 4);
        assert_eq!(p.lane, PalwStepLaneV1::Int32, "an integer class commits integer codes");
        assert!(p.gdn_nodes.is_empty(), "a plain decoder-only transformer has no GatedDeltaNet arm");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Attn), 4, "every layer is an attention layer");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Gdn), 0);
        // 1 pre + 18 per layer x 4 + 2 post.
        assert_eq!(p.global_node_count(), 1 + 18 * 4 + 2);
        // A profile id exists, which is what a class registration commits to.
        assert_ne!(p.shape_profile_id(), Hash64::default());
    }

    /// **Every kernel it names is one this build can adjudicate.**
    ///
    /// Checked against `catalogued_kernel_ids_v1()` — the ADJUDICATION table itself — rather than
    /// against a restated list, because the gate's promise is that a registered class can
    /// actually be re-executed here, and only that table knows. This is the check a faithful
    /// Qwen3.5-2B profile fails: its matmuls are `Q4_K`/`Q5_K`/`Q6_K` and no float quantized
    /// matmul is in the catalog at all.
    #[test]
    fn base0_profile_names_only_adjudicable_kernels() {
        let p = base0_profile_v1(geometry()).unwrap();
        let catalogued = catalogued_kernel_ids_v1();
        let mut seen = 0;
        for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
            for node in table {
                assert!(
                    catalogued.contains(&node.kernel_semantics_id),
                    "{:?} names a kernel this build cannot recompute — the class would be unadjudicable",
                    node.op_kind
                );
                seen += 1;
            }
        }
        assert_eq!(seen, 21, "the whole graph was checked, not a prefix of it");
    }

    /// The float constants are zero and the float tables are empty, and every one of those is a
    /// property of the class rather than an unfilled field: BASE-0 evaluates no transcendental at
    /// inference (its rotary is a pinned table), holds no float epsilon, and has no FMA
    /// contraction to pin because integer addition is exactly associative.
    #[test]
    fn base0_pins_no_float_anything() {
        let p = base0_profile_v1(geometry()).unwrap();
        assert_eq!((p.rope_freq_base_bits, p.rms_eps_bits, p.l2_eps_bits), (0, 0, 0));
        assert!(p.transcendental_bindings.is_empty(), "no libm site to bind");
        assert!(p.contraction_facts.is_empty(), "no contraction to pin");
        assert_eq!(p.kv_cache_f16, 0, "no cache holds floats");
        assert_eq!(p.base0_rms_eps_q, 1 << 8, "the epsilon it DOES hold is the integer one, from the registration");
    }

    /// Geometry is an argument, so the identity moves with it: two networks that chose different
    /// dimensions are running different classes, and `shape_profile_id` says so.
    #[test]
    fn the_geometry_is_inside_the_identity() {
        let base = base0_profile_v1(geometry()).unwrap().shape_profile_id();
        for mutate in [
            (|g: &mut PalwBase0GeometryV1| g.layer_count = 5) as fn(&mut PalwBase0GeometryV1),
            |g| g.hidden_dim = 512,
            |g| g.ffn_dim = 1_024,
            |g| g.attn_heads = 8,
            |g| g.attn_head_dim = 32,
            |g| g.vocab_size = 8_192,
            |g| g.n_ctx = 1_024,
            |g| g.n_threads = 8,
            |g| g.rms_eps_q = 1 << 9,
            |g| g.tile_len = 128,
        ] {
            let mut g = geometry();
            mutate(&mut g);
            assert_ne!(base0_profile_v1(g).unwrap().shape_profile_id(), base, "a geometry change must move the class id");
        }
    }

    /// The tensor names the graph consumes are exactly the inventory list, so a registration
    /// cannot build an artifact root over a different set than the one the court will open
    /// against.
    #[test]
    fn the_graph_consumes_exactly_the_declared_inventory() {
        let p = base0_profile_v1(geometry()).unwrap();
        let mut used: Vec<&str> = Vec::new();
        for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
            for node in table {
                if !node.weight_name.is_empty() && !used.contains(&node.weight_name.as_str()) {
                    used.push(node.weight_name.as_str());
                }
            }
        }
        used.sort_unstable();
        let mut declared: Vec<&str> = BASE0_TENSOR_NAMES.to_vec();
        declared.sort_unstable();
        assert_eq!(used, declared, "the graph's operands and the declared inventory are one list");
    }

    /// A canonical job shape for the geometry above, and a worst case ten times deeper.
    fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> crate::palw_v2::PalwJobContextV2 {
        let mut ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"base0-profile-test".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [7; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: Hash64::from_u64_word(11),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        ctx
    }

    /// **The catalog's numbers are counted, not chosen.**
    ///
    /// `canonical_step_leaf_count` is what `pwu_per_inference` must equal (ADR-0045 Decision 1)
    /// and therefore a direct multiplier on the class's fork-choice weight — so a registration
    /// that could pick it could pick its own weight. `verify_palw_genesis_v2` refuses a
    /// declaration that differs from the catalog; this is the half that makes the catalog's
    /// number a measurement.
    #[test]
    fn the_catalog_entry_counts_the_profile_rather_than_declaring_a_number() {
        let p = base0_profile_v1(geometry()).unwrap();
        let canonical = job(&p, 8, 4);
        let worst = job(&p, 64, 64);
        let entry = base0_catalog_entry_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(0xA7), &p, &canonical, &worst)
            .expect("the entry counts");

        // The SAME counter the leg builder sizes itself with — one answer, not two.
        assert_eq!(entry.canonical_step_leaf_count, crate::palw_step::step_leaf_count(&p, &canonical).unwrap());
        assert!(entry.canonical_step_leaf_count > 0, "a canonical inference has steps");
        assert!(
            entry.canonical_step_leaf_count < entry.max_step_leaf_count,
            "a canonical run is strictly inside the class's worst case, which is what keeps \
             'work worth paying for' and 'work the ladder can walk' the same quantity"
        );

        // A deeper job counts more leaves: the number tracks the execution, so it cannot be
        // restated as a constant.
        let deeper = job(&p, 16, 8);
        let deeper_entry =
            base0_catalog_entry_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(0xA7), &p, &deeper, &worst).unwrap();
        assert!(deeper_entry.canonical_step_leaf_count > entry.canonical_step_leaf_count);

        // And the reachable set is read off the graph, so the coverage gate cannot pass on a set
        // nobody derived from the thing it covers.
        assert_eq!(
            entry.reachable_kernels,
            [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes]
                .into_iter()
                .flatten()
                .map(|n| n.kernel_semantics_id)
                .collect()
        );
        assert!(entry.reachable_kernels.is_subset(&catalogued_kernel_ids_v1()), "and every one of them is adjudicable");
    }

    /// **G5, measured: one node of this graph is still unadjudicable, and it says so.**
    ///
    /// The id gate certified this profile at "100% coverage" while several nodes could never be
    /// recomputed, because it compares kernel IDs and never asks what a kernel can SERVE. Asking
    /// properly — `kernel_can_serve_node_v1`, which lives next to the code that does the serving
    /// — turns each of those into a registration-time refusal with a reason.
    ///
    /// Three of the four it found are closed. The attention matmuls multiply an activation by an
    /// opened row instead of demanding a registered weight (G5a), `KvScaled` widths are derived
    /// from the kv length the caller already holds (G5b), and the KV sentinels resolve to this
    /// layer's cache-role nodes over the position history (G5c) — no new leaf format and no float
    /// aux series, because the cache contents are already ordinary step tiles.
    ///
    /// One remains, and it is a blocker recorded in `docs/palw-qwen25-class-phase0.md` rather
    /// than something to route around:
    ///
    /// * **pre/0, the embedding gather (G5d)** — `Base0Op::Embed` needs one input row and the pre
    ///   table has no upstream to name. Adjudicating a real gather needs the TOKEN ID, which is
    ///   not a step input and is not in the job context (only `prompt_token_ids_hash` is).
    ///
    /// This test asserts the CURRENT truth so the state is measured rather than believed. When
    /// G5d closes it must be rewritten to expect zero refusals — the number is the point.
    #[test]
    fn the_servability_gate_names_exactly_what_is_still_unadjudicable() {
        use crate::palw_catalog_coverage::verify_profile_coverage_v1;
        use crate::palw_step_refute::kernel_can_serve_node_v1;
        let p = base0_profile_v1(geometry()).unwrap();

        let mut refused: Vec<(&str, usize)> = Vec::new();
        for (name, nodes) in [("pre", &p.pre_nodes), ("gdn", &p.gdn_nodes), ("attn", &p.attn_nodes), ("post", &p.post_nodes)] {
            for (slot, node) in nodes.iter().enumerate() {
                if kernel_can_serve_node_v1(node, name == "pre").is_err() {
                    refused.push((name, slot));
                }
            }
        }
        assert_eq!(
            refused,
            vec![("pre", 0)],
            "the embedding gather alone — the two attention matmuls closed with G5a/b/c, so 20 of 21 nodes are servable"
        );
        // Which means the profile as a whole is refused, and the refusal names a node.
        assert!(matches!(
            verify_profile_coverage_v1(&p),
            Err(crate::palw_catalog_coverage::PalwCoverageError::NodeNotServable { .. })
        ));

        // The two closed halves, asserted directly so a regression in either is visible here:
        // a weightless matmul with a second row is servable, and a kv-scaled one is too.
        let mut activation_matmul = p.attn_nodes[5].clone();
        activation_matmul.input_refs = vec![4, 3];
        kernel_can_serve_node_v1(&activation_matmul, false).expect("an activation x activation matmul is servable now");
        assert!(matches!(activation_matmul.out_len, PalwStepOutLenV1::KvScaled { .. }), "and at a kv-scaled width");

        // And the shapes it refuses, each for its own stated reason.
        let mut orphan = activation_matmul.clone();
        orphan.input_refs = vec![4];
        assert!(kernel_can_serve_node_v1(&orphan, false).is_err(), "a weightless matmul with one input has nothing to multiply");
        let mut oracle_kv = p.attn_nodes[8].clone();
        oracle_kv.out_len = PalwStepOutLenV1::KvScaled { multiplier: 1 };
        assert!(kernel_can_serve_node_v1(&oracle_kv, false).is_err(), "a kv-scaled weight matmul names no matrix the oracle holds");
    }

    /// Every weighted node carries one dtype byte per layer its table covers, all int8 — BASE-0
    /// has exactly one weight type, and variance would mean it is not BASE-0.
    #[test]
    fn every_weight_is_int8_once_per_covered_layer() {
        let p = base0_profile_v1(geometry()).unwrap();
        for (table, span) in [
            (&p.pre_nodes, p.table_layer_span(PalwStepTableV1::Pre)),
            (&p.attn_nodes, p.table_layer_span(PalwStepTableV1::Attn)),
            (&p.post_nodes, p.table_layer_span(PalwStepTableV1::Post)),
        ] {
            for node in table {
                if node.weight_name.is_empty() {
                    assert!(node.weight_dtypes.is_empty());
                    continue;
                }
                assert_eq!(node.weight_dtypes.len(), span, "{} needs one byte per covered layer", node.weight_name);
                assert!(node.weight_dtypes.iter().all(|d| *d == BASE0_WEIGHT_DTYPE_I8), "BASE-0 is int8 throughout");
            }
        }
    }
}
