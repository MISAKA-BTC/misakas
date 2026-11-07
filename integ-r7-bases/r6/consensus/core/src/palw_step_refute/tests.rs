//! **`palw_step_refute`'s `tests` module**, moved out of `palw_step_refute.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

/// No gather: the challenged coordinate is the graph's origin and no prompt ids are carried.
/// Every kernel but `Embed` ignores it, and `Embed` has its own tests.
const NO_GATHER: (&PalwStepCoordinateV1, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1<'static>, &[u32]) = (
    &PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
    crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::EMPTY,
    &[],
);

use super::*;
use crate::palw_legs::PalwCheckpointProfileV1;
use crate::palw_step::{
    PALW_STEP_OBJECT_VERSION_V1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, canonical_step_coordinates,
    kv_aux_leaf_count, step_leaf_count,
};
use crate::palw_step_leg::step_tile_leaf_hash_v1;
use crate::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepLegBuilderV1, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1, step_opening_v1,
};
use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2};

fn h64(fill: u8) -> Hash64 {
    Hash64::from_bytes([fill; 64])
}

struct NoWeights;
impl PalwWeightOracleV1 for NoWeights {
    fn operand_bytes(&self, _t: &str, _l: Option<u16>, _o: u32, _n: u32) -> Option<Vec<u8>> {
        None
    }
}

/// Serves one fixed byte row, so a test can hand `base0_row` exactly the bytes an on-chain
/// weight oracle would have committed.
struct FixedRow(Vec<u8>);
impl PalwWeightOracleV1 for FixedRow {
    /// **Honours `byte_len`, because a double that does not cannot witness a size defect.**
    /// This returned the whole row regardless of what was asked for, which is why op 9 asking
    /// for one byte and requiring five had a passing test (ADR-0049 Decision A).
    fn operand_bytes(&self, _t: &str, _l: Option<u16>, o: u32, n: u32) -> Option<Vec<u8>> {
        let (o, n) = (o as usize, n as usize);
        (self.0.len() >= o + n).then(|| self.0[o..o + n].to_vec())
    }
}

/// A node for the BASE-0 requantize path. `base0_row` dispatches on its `op` ARGUMENT and reads
/// only `weight_name` off the node for this op, so the remaining fields are shape formality.
fn requantize_node() -> PalwStepNodeV1 {
    PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::Scale,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: "blk.{layer}.requant".to_string(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements: 4 },
        tile_len: 4,
        kernel_semantics_id: h64(0),
        input_refs: vec![0],
    }
}

/// **§3.3: `RmsNorm` is adjudicated with the CLASS's epsilon, not a constant.**
///
/// The court recomputed with a hardcoded `eps = 1`, so every honest producer of a class
/// registered with any other epsilon was convicted on every norm step. The epsilon now comes
/// from the registered shape profile, which `shape_profile_id` binds.
#[test]
fn rms_norm_is_adjudicated_with_the_registered_epsilon() {
    let mut node = requantize_node();
    node.weight_name = String::new();
    let input: Vec<Vec<u32>> = vec![vec![10i32 as u32, (-20i32) as u32, 30, (-40i32) as u32]];
    let x: Vec<i8> = vec![10, -20, 30, -40];

    // A class registered with a large epsilon must be adjudicated with THAT epsilon.
    let mut big = profile();
    big.base0_rms_eps_q = 1 << 30;
    let got = base0_row(Base0Op::RmsNorm, &node, Some(0), &big, &input, &NoWeights, 1, NO_GATHER).expect("adjudicable");
    let want: Vec<u32> = crate::palw_base0_ops::rms_norm(&x, 1 << 30).unwrap().into_iter().map(|v| v as u32).collect();
    assert_eq!(got, want, "the registered epsilon must be the one used");

    // ...and it must NOT equal the hardcoded-1 recompute, or the class's epsilon is decorative
    // and an honest producer under `big` would be convicted.
    let with_one: Vec<u32> = crate::palw_base0_ops::rms_norm(&x, 1).unwrap().into_iter().map(|v| v as u32).collect();
    assert_ne!(got, with_one, "eps must actually change the adjudicated result");

    // A different registered epsilon is a different class identity, so the two cannot collide.
    let mut small = profile();
    small.base0_rms_eps_q = 1 << 8;
    assert_ne!(big.shape_profile_id(), small.shape_profile_id(), "the epsilon is inside the class id");
}
/// **Op 9 adjudicating through the PRODUCTION oracle, which it had never done.**
///
/// `rescale_is_adjudicable_and_bounded` below proves the arithmetic, and proves it through
/// `FixedRow` — a double that used to return whole rows regardless of the size requested. The
/// arm asked for ONE element and required FIVE bytes, and `PalwProvenOperandsV1` returned
/// `elements` bytes, so through a real Merkle opening `Rescale` received one byte and refused
/// every step it was handed. Coverage still reported the kernel catalogued, because coverage
/// compares ids.
///
/// This is the same step, served by the oracle a full node actually holds: an opening verified
/// against a registered `artifact_root`, no local model, no double. It is the test ADR-0049
/// Decision A exists to make possible.
#[test]
fn op_nine_adjudicates_through_a_real_artifact_opening() {
    use crate::palw_artifact::{
        PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1, artifact_leaf_v1, artifact_root_v1,
    };

    let mut node = requantize_node();
    node.weight_name = "blk.{layer}.scale".to_string();
    let acc: Vec<i32> = vec![1_000, -2_000, 3_000, -4_000];
    let input: Vec<Vec<u32>> = vec![acc.iter().map(|v| *v as u32).collect()];

    // The five bytes a `Rescale` node's parameters are: one i32 multiplier LE, one u8 shift.
    let mut bytes = i32::MAX.to_le_bytes().to_vec();
    bytes.push(23);
    let operand = PalwArtifactOperandV1 { tensor_name: "blk.{layer}.scale".to_string(), layer: Some(0), row_start: 0, bytes };
    let leaf = artifact_leaf_v1(&operand);
    let root = artifact_root_v1(&[leaf]).expect("a one-leaf inventory has a root");
    let opening = PalwArtifactOpeningV1 { operand, leaf_index: 0, leaf_count: 1, path: vec![] };
    let oracle = PalwProvenOperandsV1::from_openings_v1(&[opening], root).expect("the opening verifies against its root");

    let got = base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &oracle, 1, NO_GATHER)
        .expect("op 9 adjudicates through a proven operand");
    let want: Vec<u32> =
        crate::palw_base0_ops::rescale_row(&acc, crate::palw_base0_ops::ScaleParams { multiplier: i32::MAX, shift: 23 })
            .into_iter()
            .map(|v| v as u32)
            .collect();
    assert_eq!(got, want, "the court recomputes the step from carried evidence alone");

    // And a node whose parameters nobody opened stays Unadjudicable — no proof, no conviction.
    let mut unopened = node.clone();
    unopened.weight_name = "blk.{layer}.never_registered".to_string();
    assert_eq!(
        base0_row(Base0Op::Rescale, &unopened, Some(0), &profile(), &input, &oracle, 1, NO_GATHER),
        Err(PalwStepRefuteError::Unadjudicable),
        "an operand nobody proved leaves the step unchecked rather than decided"
    );
}

/// ADR-0040 H op 9 is adjudicable, and a committed shift outside its domain is refused.
///
/// `Rescale` was missing from the catalog entirely while Decision H recorded that the other
/// nine cannot compute without it — so the one op that makes the class work was the one the
/// court could not adjudicate, an `Unadjudicable` hole at the centre of the graph.
#[test]
fn rescale_is_adjudicable_and_bounded() {
    let mut node = requantize_node();
    node.weight_name = "blk.{layer}.scale".to_string();
    let acc: Vec<i32> = vec![1_000, -2_000, 3_000, -4_000];
    let input: Vec<Vec<u32>> = vec![acc.iter().map(|v| *v as u32).collect()];

    // One per-tensor (multiplier, shift): a gain of 2^8 at shift 23.
    let mut row = i32::MAX.to_le_bytes().to_vec();
    row.push(23);
    let got = base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER).expect("adjudicable");
    let want: Vec<u32> =
        crate::palw_base0_ops::rescale_row(&acc, crate::palw_base0_ops::ScaleParams { multiplier: i32::MAX, shift: 23 })
            .into_iter()
            .map(|v| v as u32)
            .collect();
    assert_eq!(got, want, "op 9 must recompute through the catalog op");

    // A shift past the op's domain is malformed by construction: refuse the step rather than
    // clamp-and-compare against arithmetic the specification does not define.
    for bad in [crate::palw_base0::RESCALE_MAX_SHIFT + 1, 100, 255] {
        let mut row = i32::MAX.to_le_bytes().to_vec();
        row.push(bad);
        assert!(
            matches!(
                base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER),
                Err(PalwStepRefuteError::InputSetNotCanonical(_))
            ),
            "shift {bad} is outside the 0..=62 domain and must be refused"
        );
    }

    // The descriptor is in the catalog, so the coverage gate sees it.
    assert!(catalogued_kernel_ids_v1().contains(&kernel_semantics_id_v1(KDESC_BASE0_RESCALE)));
}

/// **§3.3: `MatMulQuant` asks the oracle for the WHOLE weight block.**
///
/// It used to request one input row's worth, so `out_dim` was always 1 and any node producing a
/// wider row failed as `InputSetNotCanonical` — the CHALLENGER blamed for the adjudicator's own
/// under-request. A width this side cannot determine, or an oracle that cannot serve it, is
/// `Unadjudicable`.
#[test]
fn matmul_recomputes_the_whole_output_row() {
    let mut node = requantize_node();
    node.weight_name = "blk.{layer}.w".to_string();
    node.out_len = PalwStepOutLenV1::Fixed { elements: 3 };
    let x: Vec<i8> = vec![1, 2, 3, 4];
    let input: Vec<Vec<u32>> = vec![x.iter().map(|v| *v as i32 as u32).collect()];

    // 3 output rows × 4 inputs = 12 weight bytes.
    let w: Vec<i8> = (1..=12i8).collect();
    let row: Vec<u8> = w.iter().map(|v| *v as u8).collect();
    let got = base0_row(Base0Op::MatMul, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER).expect("adjudicable");
    let want: Vec<u32> = crate::palw_base0_ops::matmul_quant(&w, &x, 3).unwrap().into_iter().map(|v| v as u32).collect();
    assert_eq!(got.len(), 3, "the full declared output row is recomputed, not one element");
    assert_eq!(got, want);

    // An oracle that serves the OLD amount (one row's worth) can no longer be mistaken for a
    // valid block: it is a class/registration disagreement this court cannot resolve.
    let short: Vec<u8> = w[..4].iter().map(|v| *v as u8).collect();
    assert_eq!(
        base0_row(Base0Op::MatMul, &node, Some(0), &profile(), &input, &FixedRow(short), 1, NO_GATHER),
        Err(PalwStepRefuteError::Unadjudicable),
        "a short weight block is unadjudicable, never the challenger's fault"
    );

    // **A kv-scaled width IS determinable here, and this assertion used to say otherwise.**
    //
    // It read "a kv-scaled width is not determinable here, so it is refused rather than
    // guessed" and passed — but not for that reason. `out_dim` is `multiplier x kv_len` and the
    // caller passes `kv_len`, which the arm's own comment records ("the old refusal … no longer
    // describes anything"). What produced the refusal was `FixedRow` serving twelve bytes when
    // the step asked for eight, so the length check rejected an oracle that had over-answered.
    // With the double honouring `byte_len` (ADR-0049 Decision A) the over-answer is gone and
    // the step adjudicates, which is correct.
    let mut kv = node.clone();
    kv.out_len = PalwStepOutLenV1::KvScaled { multiplier: 2 };
    let full: Vec<u8> = w.iter().map(|v| *v as u8).collect();
    let kv_got = base0_row(Base0Op::MatMul, &kv, Some(0), &profile(), &input, &FixedRow(full.clone()), 1, NO_GATHER)
        .expect("multiplier x kv_len is a width this arm holds");
    let kv_want: Vec<u32> = crate::palw_base0_ops::matmul_quant(&w[..8], &x, 2).unwrap().into_iter().map(|v| v as u32).collect();
    assert_eq!(kv_got, kv_want, "a kv-scaled node recomputes multiplier x kv_len output rows");

    // What IS still refused: a width the oracle cannot serve. Not being able to check is never
    // the challenger's fault, so it is `Unadjudicable` rather than a conviction or an acquittal.
    let mut wide = node.clone();
    wide.out_len = PalwStepOutLenV1::KvScaled { multiplier: 9 };
    assert_eq!(
        base0_row(Base0Op::MatMul, &wide, Some(0), &profile(), &input, &FixedRow(full), 1, NO_GATHER),
        Err(PalwStepRefuteError::Unadjudicable),
        "an oracle that cannot serve the declared width leaves the step unchecked, not decided"
    );
}

/// Audit §2.3: the court REFUSES a step whose committed requantize shift is outside ADR-0040
/// C1's `0..=31` domain, instead of recomputing with it.
///
/// `rounding_shift_right` clamps such a shift so it can never panic, but clamping is a release
/// safety net, not an adjudication rule: a step that committed an out-of-domain shift is
/// malformed by construction (no honest producer emits one), and comparing a clamped recompute
/// against it would let a malformed step be *adjudicated* — convicting or acquitting on
/// arithmetic the specification does not define. An adversarial verifier replaced this reject
/// with `if false` and found every suite still green, so this is the coverage.
#[test]
fn a_requantize_shift_outside_the_domain_is_refused_not_recomputed() {
    let node = requantize_node();
    let input: Vec<Vec<u32>> = vec![vec![1_000i32 as u32, 2_000, 3_000, 4_000]];

    // 9 bytes per channel: multiplier LE, shift, zero LE (the ADR-0040 amendment's additive
    // term). In-domain shift (=10) must adjudicate.
    let mut ok_row = Vec::new();
    for _ in 0..4 {
        ok_row.extend_from_slice(&i32::MAX.to_le_bytes());
        ok_row.push(10);
        ok_row.extend_from_slice(&0i32.to_le_bytes());
    }
    let got = base0_row(Base0Op::Requantize, &node, Some(0), &profile(), &input, &FixedRow(ok_row), 1, NO_GATHER);
    assert!(got.is_ok(), "an in-domain shift must still be recomputed: {got:?}");

    // Out-of-domain shifts are refused as non-canonical — including 32, the first one past the
    // domain edge, and 255, the largest a byte can carry.
    for bad in [32u8, 63, 64, 200, 255] {
        let mut row = Vec::new();
        for channel in 0..4 {
            row.extend_from_slice(&i32::MAX.to_le_bytes());
            // Only ONE channel is out of domain: the check must scan every chunk, not just the
            // first, or a malformed shift hides behind three well-formed neighbours.
            row.push(if channel == 3 { bad } else { 10 });
            row.extend_from_slice(&0i32.to_le_bytes());
        }
        let refused = base0_row(Base0Op::Requantize, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER);
        assert!(
            matches!(refused, Err(PalwStepRefuteError::InputSetNotCanonical(_))),
            "shift {bad} in the last channel must be refused as non-canonical, got {refused:?}"
        );
    }
}

/// A pure-GDN profile: pre = embed(one row feeding everything), one GDN layer whose
/// nodes are the five wiring inputs then the core. Geometry: 2 heads × k16 × v16.
///
/// `pub(crate)` because it is the only real GatedDeltaNet wiring in the tree, and
/// `palw_class_admission_v2`'s cost derivation has to be checked against the SAME profile the
/// court's own input rule is written for. A second hand-written GDN profile over there would be
/// a second description of this one, which is the defect that arm already had.
pub(crate) fn profile() -> PalwShapeProfileV3 {
    let mk = |kind, elements, desc: &str, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements },
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(desc),
        input_refs: refs,
    };
    PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: crate::palw_step::PalwStepLaneV1::Float32,
        layer_count: 1,
        full_attention_interval: 0, // pure recurrent
        hidden_dim: 32,
        ffn_dim: 32,
        attn_heads: 1,
        attn_kv_heads: 1,
        attn_head_dim: 16,
        rope_dims: 2,
        rope_sections: [1, 1, 0, 0],
        rope_freq_base_bits: 0x4CBE_BC20,
        rms_eps_bits: 0x3583_37BD,
        base0_rms_eps_q: 1 << 8,
        logits_scheme_id: crate::palw_step_refute::flat_logits_scheme_id_v1(),
        l2_eps_bits: 0x3583_37BD,
        gdn_heads: 2,
        gdn_head_k_dim: 16,
        gdn_head_v_dim: 16,
        gdn_conv_kernel: 4,
        vocab_size: 40,
        repack_on: 1,
        llamafile_on: 1,
        flash_attn_disabled: 1,
        fused_gdn_on: 1,
        use_ref_off: 1,
        kv_cache_f16: 1,
        n_ctx: 64,
        n_batch: 64,
        n_ubatch: 64,
        n_seq: 1,
        n_threads: 4,
        pre_nodes: vec![mk(PalwStepOpKindV1::EmbedLookup, 32, "unimplemented/embed", vec![])],
        gdn_nodes: vec![
            // Five wiring rows (schema exercise: sourced from the layer input through
            // implemented elementwise kernels so the whole chain is adjudicable).
            mk(PalwStepOpKindV1::L2Norm, 32, KDESC_L2_NORM, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // q
            mk(PalwStepOpKindV1::L2Norm, 32, KDESC_L2_NORM, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // k
            mk(PalwStepOpKindV1::Sigmoid, 32, KDESC_SIGMOID_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // v
            mk(PalwStepOpKindV1::Softplus, 16, KDESC_SOFTPLUS_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // g (2 heads → 2, padded to tile-able 16)
            mk(PalwStepOpKindV1::Sigmoid, 16, KDESC_SIGMOID_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]),   // beta
            mk(PalwStepOpKindV1::GatedDeltaNet, 32, KDESC_GDN_CORE_NEON, vec![0, 1, 2, 3, 4]),
        ],
        attn_nodes: vec![],
        post_nodes: vec![mk(PalwStepOpKindV1::Silu, 32, "unimplemented/logits", vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN])],
        reference_ruleset_id: h64(0x22),
        transcendental_bindings: vec![],
        contraction_facts: vec![],
        kv_chunk_calls: 0,
        state_chunk_map_id: h64(0x44),
    }
}

pub(crate) fn context() -> PalwJobContextV2 {
    let mut ctx = PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"step-refute-test".to_vec(),
        job_id: h64(1),
        job_nullifier: h64(2),
        assignment_id: h64(3),
        execution_seed: [7; 32],
        model_profile_id: h64(4),
        runtime_manifest_hash: h64(5),
        runtime_class_id: h64(6),
        // Placeholder; the fixtures below overwrite it with the profile they actually carry,
        // because honest material declares the profile it was produced under.
        shape_profile_id: h64(7),
        trace_scheme_id: h64(8),
        cu_ruleset_id: h64(9),
        tokenizer_id: h64(10),
        prompt_token_ids_hash: h64(11),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 2,
        max_context_tokens: 64,
    };
    ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
    ctx
}

/// Deterministic pseudo-values in a tame numeric range (bit patterns of ~[-2, 2]).
fn val(seed: u32) -> u32 {
    let m = seed.wrapping_mul(0x9E37_79B9) >> 9;
    0x3F00_0000 | (m & 0x007F_FFFF) | ((seed & 1) << 31)
}

/// Executes the profile's graph HONESTLY with the catalog programs, committing every
/// node-output tile; returns the binding plus retained material.
/// A structurally real refutation with NO openings — enough for callers that exercise the
/// authorship half and the gating around adjudication, and deliberately not adjudicable, so a
/// test using it cannot accidentally assert a conviction it did not prove.
pub(crate) fn skeleton_refutation() -> PalwExecutionStepRefutationV1 {
    let (binding, _material, _rows) = honest_execution();
    PalwExecutionStepRefutationV1 {
        binding,
        output_opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: h64(0x01), siblings: Vec::new() },
        output_preimage: PalwStepTileLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord: PalwStepCoordinateV1 { node_slot: 0, call_index: 0, position: 0, tile_index: 0 },
            value_count: 0,
            values_le: Vec::new(),
        },
        inputs: Vec::new(),
        prompt_token_ids: Vec::new(),
        decode_tokens: None,
        kv_checkpoint: None,
    }
}

pub(super) fn honest_execution() -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>) {
    honest_execution_with_context(|_| {})
}

/// [`honest_execution`] with the job context edited BEFORE any material is produced.
///
/// The whole commitment chain hangs off `context_hash()` — leaf hashes, the step-leg root, the
/// execution root — so a test that reaches into a finished binding and rewrites a context field
/// gets `Leg(CommittedRootMismatch)` and never reaches the rule it was trying to test. Measured
/// while writing the prompt-ids opening tests: all five arms returned that one error. The edit
/// has to happen where the producer would have made it.
fn honest_execution_with_context(
    edit: impl FnOnce(&mut PalwJobContextV2),
) -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>) {
    let p = profile();
    // Honest material declares the profile it was produced under (the equality the step-leg
    // verifier enforces).
    let mut ctx = context();
    ctx.shape_profile_id = p.shape_profile_id();
    edit(&mut ctx);
    // rows[position_ordinal][node_slot] = full output row. Position ordinals: prefill
    // p0, p1, then decode call 1.
    let positions: Vec<(u32, u32)> = vec![(0, 0), (0, 1), (1, 0)];
    let slots = p.global_node_count();
    let mut rows: Vec<Vec<Vec<u32>>> = vec![vec![Vec::new(); slots as usize]; positions.len()];
    for (ord, &(_call, _pos)) in positions.iter().enumerate() {
        // pre: embed row — arbitrary but deterministic and distinct per position.
        rows[ord][0] = (0..32).map(|i| val(ord as u32 * 100 + i)).collect();
        // gdn nodes 1..=5 (slots 1..=5): elementwise from the layer input (slot 0).
        let layer_in = rows[ord][0].clone();
        rows[ord][1] = l2_norm_row(&layer_in, p.l2_eps_bits);
        rows[ord][2] = l2_norm_row(&layer_in, p.l2_eps_bits);
        rows[ord][3] = layer_in.iter().map(|&v| crate::palw_transcendental::ggml_sigmoid_v1(v, true)).collect();
        rows[ord][4] = layer_in.iter().take(16).map(|&v| crate::palw_transcendental::ggml_softplus_v1(v, true)).collect();
        rows[ord][5] = layer_in.iter().take(16).map(|&v| crate::palw_transcendental::ggml_sigmoid_v1(v, true)).collect();
        // gdn core (slot 6): genesis replay over ordinals 0..=ord.
        let mut gdn_inputs: Vec<Vec<u32>> = Vec::new();
        for row in rows.iter().take(ord + 1) {
            gdn_inputs.push(row[1].clone());
            gdn_inputs.push(row[2].clone());
            gdn_inputs.push(row[3].clone());
            gdn_inputs.push(row[4][..2].to_vec()); // 2 heads — hmm, see note below
            gdn_inputs.push(row[5][..2].to_vec());
        }
        // The checker resolves g/beta rows as the FULL node rows (16 wide); mirror that.
        let mut gdn_inputs_full: Vec<Vec<u32>> = Vec::new();
        for row in rows.iter().take(ord + 1) {
            gdn_inputs_full.push(row[1].clone());
            gdn_inputs_full.push(row[2].clone());
            gdn_inputs_full.push(row[3].clone());
            gdn_inputs_full.push(row[4].clone());
            gdn_inputs_full.push(row[5].clone());
        }
        let _ = gdn_inputs;
        rows[ord][6] = gdn_core_from_wide_rows(&p, &gdn_inputs_full).unwrap();
        // post (slot 7): logits-only positions — computed where committed (p1, decode).
        rows[ord][7] = rows[ord][6].iter().map(|&v| crate::palw_transcendental::ggml_v_silu_v1(v)).collect();
    }
    // Commit every tile in canonical order.
    let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
    let main = b.expected_main_leaves();
    for i in 0..main {
        let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
        let ord = match (coord.call_index, coord.position) {
            (0, 0) => 0usize,
            (0, 1) => 1,
            (1, 0) => 2,
            _ => unreachable!(),
        };
        let row = &rows[ord][coord.node_slot as usize];
        let start = coord.tile_index as usize * 16;
        let end = (start + 16).min(row.len());
        b.push_step_tile(coord, &row[start..end]).unwrap();
    }
    let material = b.finish().unwrap();
    // Assemble the composite (no checkpoints for this shape: decode_calls 1, interval 8).
    let ctx_hash = ctx.context_hash();
    let profile_hash = p.shape_profile_id();
    let ckpt_profile = PalwCheckpointProfileV1 {
        version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
        // The interval stays this fixture's own — only the LAYOUT is the family's, which is
        // what `verify_binding_v1` pins. See its comment for why the interval belongs in the
        // class catalog rather than here.
        checkpoint_interval: 8,
        state_layout_id: crate::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
    };
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
    let ckpt_root =
        checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile.profile_hash(), &h64(0x44), 1, 0, &checkpoint_empty_root_v2(&ctx_hash));
    let committed = execution_commitment_root_v2(&ctx_hash, &h64(0xAA), &h64(0xBB), &ckpt_root, &step_root);
    let binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx,
        shape_profile: p,
        checkpoint_profile: ckpt_profile,
        state_chunk_map_id: h64(0x44),
        full_logits_trace_root: h64(0xAA),
        activation_leg_root: h64(0xBB),
        step_leaf_count: material.leaf_count,
        step_merkle_root: material.merkle_root,
        checkpoint_count: 0,
        checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
        committed_execution_root: committed,
    };
    (binding, material, rows)
}

/// The checker's view of GDN inputs: full node rows (g/beta 16-wide with only the first
/// `heads` entries meaningful). Mirror of `run_program`'s slicing so the honest world
/// and the adjudicator agree byte-for-byte.
fn gdn_core_from_wide_rows(p: &PalwShapeProfileV3, inputs: &[Vec<u32>]) -> Result<Vec<u32>, PalwStepRefuteError> {
    let narrowed: Vec<Vec<u32>> = inputs
        .chunks_exact(5)
        .flat_map(|c| {
            vec![
                c[0].clone(),
                c[1].clone(),
                c[2].clone(),
                c[3][..p.gdn_heads as usize].to_vec(),
                c[4][..p.gdn_heads as usize].to_vec(),
            ]
        })
        .collect();
    gdn_core_genesis_replay(p, &narrowed, DotStructure::Step16Epr4)
}

/// **An unbounded GDN dimension is refused, not panicked on** (ADR-0068 launch audit sweep).
///
/// `validate_geometry` bounds the ATTENTION geometry and explains itself while doing it; it
/// never looks at `gdn_heads`, `gdn_head_k_dim` or `gdn_head_v_dim`. So a class could register
/// `gdn_head_k_dim = 1 << 24` — a multiple of both dot steps, so the arm's multiple-of-step
/// rule waved it through — and `i32_len_to_f32_bits` then computed `leading_zeros() - 8` on a
/// value with seven leading zeros. Under the release profile's `overflow-checks` that is a
/// panic, in virtual processing, on a block already stored and relayed.
///
/// The premise is asserted alongside the fix, because the premise is what makes the guard
/// load-bearing: if `validate_geometry` ever starts refusing this profile, this test should
/// keep passing for a second reason rather than quietly stop testing anything.
/// **The attention geometry has a ceiling too — the GDN sweep's third network kill, one table
/// over.**
///
/// `validate_geometry` bounds `attn_heads`/`attn_head_dim` against ZERO and against nothing
/// else, and it explains why: refusing an unusual-but-adjudicable shape would refuse classes
/// this court can judge, and the reachable defect belongs in the arm. The arm did not hold it.
/// `attn_head_dim` is a `u32`, so a registered profile could carry `heads * attn_head_dim`
/// past 2^32 into `vec![one; count]` — and a failed Rust allocation ABORTS rather than
/// unwinding, in virtual processing, on a block already stored and relayed, and again on every
/// restart. Unlike a shift, there is no panic to catch.
///
/// The premise is asserted with the fix, so that if the class-level check ever starts refusing
/// this profile the test keeps passing for a second reason rather than quietly stop testing.
#[test]
fn an_unbounded_attention_dimension_is_refused_rather_than_allocated_from() {
    // The premise: nothing upstream refuses this profile.
    let mut poisoned = profile();
    poisoned.attn_head_dim = PALW_ATTN_MAX_DIM as u32 + 1;
    assert!(
        poisoned.validate_geometry().is_ok(),
        "the class-level check bounds the attention geometry against zero only — which is why the arm must bound it"
    );

    // Values, not the arm's plumbing: the count is a pure function, so the ceiling is testable
    // without an oracle, a node and a gather.
    assert_eq!(attn_params_count_v1(&poisoned, false, 4_096), None, "a dimension past the ceiling is unservable");
    assert_eq!(attn_params_count_v1(&poisoned, true, 4_096), None);

    // The PRODUCT, which is what the values arm allocates from: two individually legal
    // dimensions still multiply. `2^16 heads x 2^16 head-dim` is 2^32 elements — inside a
    // `usize`, so no overflow refuses it, and 64 GiB of `A16QuantParams` if anything allocates
    // it.
    let mut wide = profile();
    wide.attn_heads = u16::MAX;
    wide.attn_head_dim = PALW_ATTN_MAX_DIM as u32;
    assert!(wide.validate_geometry().is_ok(), "and each factor alone clears every rule there is");
    assert!(
        (u16::MAX as usize).checked_mul(PALW_ATTN_MAX_DIM).is_some_and(|n| n > PALW_ATTN_MAX_STATE_WORDS),
        "the product is representable and past the ceiling — the case a per-factor bound misses"
    );
    assert_eq!(attn_params_count_v1(&wide, false, 4_096), None, "so the product is bounded on its own");

    // And the shapes this chain actually registers stay servable, or the ceiling would be a
    // refusal of the classes it exists to protect.
    let shipped = profile();
    assert!(attn_params_count_v1(&shipped, false, 4_096).is_some(), "the registered geometry is inside the ceiling");
    assert_eq!(
        attn_params_count_v1(&shipped, false, 4_096),
        Some(shipped.attn_heads as usize * shipped.attn_head_dim as usize),
        "and the count is unchanged for it — this is a ceiling, not a new rule"
    );
    // Zero keeps answering the way it did, through the same door.
    let mut zero = profile();
    zero.attn_kv_heads = 0;
    assert_eq!(attn_params_count_v1(&zero, false, 4_096), None);
}

#[test]
fn an_unbounded_gdn_dimension_is_refused_rather_than_panicked_on() {
    // The premise, in arithmetic: the value the old assertion permitted is the value that
    // underflows. `debug_assert!(n <= 1 << 24)` admitted exactly this.
    assert_eq!((1u32 << 24).leading_zeros(), 7, "fewer than the eight `leading_zeros() - 8` subtracts");

    // The premise, in the class check: nothing upstream refuses this profile.
    let mut poisoned = profile();
    poisoned.gdn_head_k_dim = 1 << 24;
    assert!(poisoned.validate_geometry().is_ok(), "the class-level check does not bound the GDN geometry — which is why the arm must");
    assert!((1usize << 24).is_multiple_of(16) && (1usize << 24).is_multiple_of(32), "and it clears the multiple-of-step rule");

    // Five rows, so the input-shape clause cannot be what refuses it: the ceiling is.
    let rows = vec![Vec::<u32>::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    assert!(
        matches!(gdn_core_genesis_replay(&poisoned, &rows, DotStructure::Step16Epr4), Err(PalwStepRefuteError::Unadjudicable)),
        "an unservable geometry is a refusal of the DISPUTE, never a dead node"
    );
    // `gdn_head_v_dim` rides the same clause — it is the other factor in the state buffer.
    let mut wide = profile();
    wide.gdn_head_v_dim = PALW_GDN_MAX_DIM as u32 + 1;
    assert!(matches!(gdn_core_genesis_replay(&wide, &rows, DotStructure::Step16Epr4), Err(PalwStepRefuteError::Unadjudicable)));
    // `gdn_heads` is a `u16` and so cannot reach the ceiling at all. Its clause is kept for
    // symmetry rather than reachability: the three dimensions are one geometry, and a later
    // widening of the field would otherwise arrive with no guard and no test.
    assert!(u16::MAX as usize <= PALW_GDN_MAX_DIM, "the heads clause is unreachable today, and this is why");

    // **The product, which is what the GdnStep arm allocates from and the replay arm does not.**
    //
    // Two individually-legal `u32` dimensions still multiply into a `usize`, and that arm asks
    // for `hd_v * hd_k` elements in one go. A failed Rust allocation ABORTS rather than
    // unwinding, so unlike the shift there is no panic to catch and no error to return — which
    // is why that site bounds the product on its own and not just each factor.
    // In `u64`, because the ceiling squared is `2^32` — representable in a `usize` on the
    // 64-bit targets this fleet runs and NOT on the 32-bit ones this repository also builds
    // for (the wasm32 jobs). The bound is a quantity; the width it is checked in is a property
    // of the machine, and mixing the two is a compile-time overflow on one of them.
    let widest = PALW_GDN_MAX_DIM as u64;
    assert_eq!(widest.checked_mul(widest), Some(PALW_GDN_MAX_DIM_SQUARED), "the ceiling squared is the constant the guard uses");
    assert!(
        (u32::MAX as u64).checked_mul(u32::MAX as u64).unwrap() > widest * widest,
        "…and the unbounded declaration reaches far past it, which is the whole reason for the bound"
    );

    // And the honest shape is untouched — a ceiling that refused real classes would be a
    // different defect wearing this fix's clothes.
    let honest = profile();
    assert!(honest.gdn_head_k_dim as usize <= PALW_GDN_MAX_DIM);
    assert!(
        !matches!(gdn_core_genesis_replay(&honest, &rows, DotStructure::Step16Epr4), Err(PalwStepRefuteError::Unadjudicable)),
        "the ceiling must not be what refuses an adjudicable class"
    );
}

// ---- P0-8's end-to-end fixture: a BASE-0 MatMul execution, real weights, no model ----

/// A BASE-0 profile whose layer is one `MatMulQuant` node over a registered weight tensor.
///
/// BASE-0 rather than the float `profile()` above, and that is the point of the fixture: the
/// float classes' matmuls are `Q4_K`/`Q5_K`/`Q6_K` and `KERNEL_CATALOG` has no adjudicator
/// for any of them, so a float matmul conviction cannot be built at all today. BASE-0's
/// `i8 x i8 -> i32` matmul is exact and IS in the catalog, which is the whole reason that
/// class exists.
pub(crate) fn base0_matmul_profile() -> PalwShapeProfileV3 {
    let mut p = profile();
    // The lane that made this fixture possible at all: BASE-0 commits int32 codes, and the
    // float finiteness rule rejects every integer in `[-8_388_608, -1]`.
    p.lane = crate::palw_step::PalwStepLaneV1::Int32;
    p.gdn_nodes = vec![PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::MatMulQuant,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: "blk.{layer}.w".to_string(),
        // One byte, because this profile's GDN table covers its one layer.
        weight_dtypes: vec![24],
        out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_MATMUL),
        input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
    }];
    p.post_nodes = vec![PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::Silu,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_SILU),
        input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
    }];
    p.pre_nodes = vec![PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::EmbedLookup,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_EMBED),
        input_refs: vec![],
    }];
    p
}

/// The weight block the class registered: `32 out x 32 in` int8 codes, deterministic.
pub(crate) fn base0_matmul_weights() -> Vec<i8> {
    (0..32 * 32).map(|i| (((i * 7) % 13) - 6) as i8).collect()
}

/// An honest BASE-0 execution over [`base0_matmul_profile`], plus its leg material.
///
/// Same three position ordinals the GDN fixture uses. Every value stays inside the int8 lane,
/// because BASE-0 activations are int8 codes riding i32 lanes and an out-of-range lane is
/// `InputSetNotCanonical` rather than arithmetic.
pub(crate) fn base0_honest_execution() -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>) {
    let p = base0_matmul_profile();
    let mut ctx = context();
    ctx.shape_profile_id = p.shape_profile_id();
    let w = base0_matmul_weights();
    let slots = p.global_node_count();
    let mut rows: Vec<Vec<Vec<u32>>> = vec![vec![Vec::new(); slots as usize]; 3];
    for (ord, row) in rows.iter_mut().enumerate().take(3) {
        // pre (slot 0): the embedding row, int8 codes.
        row[0] = (0..32).map(|i| (((ord * 5 + i) % 11) as i32 - 5) as u32).collect();
        let x: Vec<i8> = row[0].iter().map(|v| *v as i32 as i8).collect();
        // gdn (slot 1): the matmul, by the SAME function the court will recompute with.
        row[1] = crate::palw_base0_ops::matmul_quant(&w, &x, 32).unwrap().into_iter().map(|v| v as u32).collect();
        // post (slot 2): silu over the layer output.
        row[2] =
            crate::palw_base0_ops::silu(&row[1].iter().map(|v| *v as i32).collect::<Vec<_>>()).into_iter().map(|v| v as u32).collect();
    }
    let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
    for i in 0..b.expected_main_leaves() {
        let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
        let ord = match (coord.call_index, coord.position) {
            (0, 0) => 0usize,
            (0, 1) => 1,
            (1, 0) => 2,
            _ => unreachable!(),
        };
        let row = &rows[ord][coord.node_slot as usize];
        let start = coord.tile_index as usize * 16;
        let end = (start + 16).min(row.len());
        b.push_step_tile(coord, &row[start..end]).unwrap();
    }
    let material = b.finish().unwrap();
    let ctx_hash = ctx.context_hash();
    let profile_hash = p.shape_profile_id();
    let ckpt_profile = PalwCheckpointProfileV1 {
        version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
        // The interval stays this fixture's own — only the LAYOUT is the family's, which is
        // what `verify_binding_v1` pins. See its comment for why the interval belongs in the
        // class catalog rather than here.
        checkpoint_interval: 8,
        state_layout_id: crate::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
    };
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
    let ckpt_root =
        checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile.profile_hash(), &h64(0x44), 1, 0, &checkpoint_empty_root_v2(&ctx_hash));
    let committed = execution_commitment_root_v2(&ctx_hash, &h64(0xAA), &h64(0xBB), &ckpt_root, &step_root);
    let binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx,
        shape_profile: p,
        checkpoint_profile: ckpt_profile,
        state_chunk_map_id: h64(0x44),
        full_logits_trace_root: h64(0xAA),
        activation_leg_root: h64(0xBB),
        step_leaf_count: material.leaf_count,
        step_merkle_root: material.merkle_root,
        checkpoint_count: 0,
        checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
        committed_execution_root: committed,
    };
    (binding, material, rows)
}

/// An HONEST BASE-0 execution and the artifact openings a challenger would carry against it.
///
/// The mirror of [`base0_matmul_fraud`]: same profile, same weights, same coordinate — and
/// nothing corrupted. What a court does with this is the whole false-slash question.
pub(crate) fn base0_honest_case() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64) {
    use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
    let (binding, material, rows) = base0_honest_execution();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 1 };
    let refutation = build_refutation(&binding, &material, &rows, coord);
    // **ADR-0049 Decision B: the CHALLENGED TILE's weight rows, not the matrix.** The
    // coordinate is tile 1 of a 16-wide tiling over a 32x32 matmul, so the step reduces over
    // output rows 16..32 and nothing else — bytes 512..1024. Half the matrix at this fixture's
    // size; one part in 1,187 of Qwen2.5-1.5B's unembed, which is the difference between a
    // court close that fits a transaction and one that does not.
    const IN_LEN: usize = 32;
    const TILE_START_ROW: usize = 16;
    const TILE_WIDTH: usize = 16;
    let all_weights = base0_matmul_weights();
    let byte_offset = TILE_START_ROW * IN_LEN;
    let operands = [
        PalwArtifactOperandV1 {
            tensor_name: "blk.{layer}.w".to_string(),
            layer: Some(0),
            row_start: byte_offset as u32,
            bytes: all_weights[byte_offset..byte_offset + TILE_WIDTH * IN_LEN].iter().map(|v| *v as u8).collect(),
        },
        PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![9, 9, 9] },
    ];
    let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let artifact_root = artifact_root_v1(&leaves).unwrap();
    let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
        operand: operands[0].clone(),
        leaf_index: 0,
        leaf_count: leaves.len() as u32,
        path: vec![leaves[1]],
    }];
    (refutation, openings, artifact_root)
}

/// **Condition 11, at the arithmetic layer: an honest step is NoFault, not a conviction.**
///
/// The conviction test says the court can catch fraud. This says it does not catch anything
/// else — which is the harder half, because a court that convicts on every challenge would
/// pass every fraud test ever written.
#[test]
fn palw_v2_an_honest_matmul_is_not_a_fault() {
    use crate::palw_artifact::PalwProvenOperandsV1;
    let (refutation, openings, artifact_root) = base0_honest_case();
    let operands = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).expect("the openings prove");
    assert_eq!(
        check_execution_step_refutation_v1(&refutation, &operands),
        Err(PalwStepRefuteError::NoFaultFound),
        "an honest step must be NoFaultFound — recomputed and found correct, not merely unchecked"
    );

    // And it is NoFault because the court RECOMPUTED it: with no weights the same refutation
    // is `Unadjudicable`, a different answer entirely. Without this, "not convicted" could
    // mean "never checked".
    assert_eq!(
        check_execution_step_refutation_v1(&refutation, &NoWeights),
        Err(PalwStepRefuteError::Unadjudicable),
        "not-convicted and not-checked must be distinguishable"
    );
}

/// The same execution with ONE committed MatMul value corrupted, plus the artifact openings a
/// challenger carries. Returns everything a `CourtClosed` proof needs.
pub(crate) fn base0_matmul_fraud() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64) {
    use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
    let (mut binding, mut material, rows) = base0_honest_execution();
    let p = binding.shape_profile.clone();
    let ctx = binding.job_context.clone();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 1 };
    let idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
    let ctx_hash = ctx.context_hash();
    let profile_hash = p.shape_profile_id();

    // The miner's world: the committed matmul output is off by one at value 3 of tile 1.
    let mut row = rows[2][1].clone();
    row[16 + 3] = (row[16 + 3] as i32).wrapping_add(1) as u32;
    let leaf = PalwStepTileLeafV1 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        coord,
        value_count: 16,
        values_le: row[16..32].iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    material.leaf_hashes[idx as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
    let merkle = crate::palw_step_leg::step_merkle_root_v1(&material.leaf_hashes).unwrap();
    material.merkle_root = merkle;
    binding.step_merkle_root = merkle;
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &merkle);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        1,
        0,
        &binding.checkpoint_merkle_root,
    );
    binding.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &binding.full_logits_trace_root, &binding.activation_leg_root, &ckpt_root, &step_root);

    let mut refutation = build_refutation(&binding, &material, &rows, coord);
    refutation.output_preimage = leaf;
    refutation.output_opening = step_opening_v1(&material.leaf_hashes, idx).unwrap();

    // The class's registered artifact inventory, and the opening that proves this weight
    // block belongs to it. This is the whole "no model" mechanism: the court never reads a
    // GGUF, it reads bytes a Merkle path binds to the root the class registered.
    // **ADR-0049 Decision B: the CHALLENGED TILE's weight rows, not the matrix.** The
    // coordinate is tile 1 of a 16-wide tiling over a 32x32 matmul, so the step reduces over
    // output rows 16..32 and nothing else — bytes 512..1024. Half the matrix at this fixture's
    // size; one part in 1,187 of Qwen2.5-1.5B's unembed, which is the difference between a
    // court close that fits a transaction and one that does not.
    const IN_LEN: usize = 32;
    const TILE_START_ROW: usize = 16;
    const TILE_WIDTH: usize = 16;
    let all_weights = base0_matmul_weights();
    let byte_offset = TILE_START_ROW * IN_LEN;
    let operands = [
        PalwArtifactOperandV1 {
            tensor_name: "blk.{layer}.w".to_string(),
            layer: Some(0),
            row_start: byte_offset as u32,
            bytes: all_weights[byte_offset..byte_offset + TILE_WIDTH * IN_LEN].iter().map(|v| *v as u8).collect(),
        },
        PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![9, 9, 9] },
    ];
    let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let artifact_root = artifact_root_v1(&leaves).unwrap();
    let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
        operand: operands[0].clone(),
        leaf_index: 0,
        leaf_count: leaves.len() as u32,
        path: vec![leaves[1]],
    }];
    (refutation, openings, artifact_root)
}

/// **ADR-0084 U-08: the ladder is an ARGUMENT, and the refutation's own leaf count is its
/// boundary.** One leaf short and the court refuses by name — `LeafCountOutOfRange`, never an
/// acquittal — exactly at it the honest step is `NoFaultFound` again, and at the shipped
/// ladder the capped name answers what the uncapped one always did. Pinned at the boundary
/// so an off-by-one in the walk cannot pass as "refused for being too big".
#[test]
fn palw_v2_the_step_ladder_is_an_argument_and_the_refutations_own_space_is_its_boundary() {
    use crate::palw_artifact::PalwProvenOperandsV1;
    use crate::palw_step_leg::{PALW_STEP_LEG_MAX_LEAVES, PalwStepLegError};
    let (refutation, openings, artifact_root) = base0_honest_case();
    let operands = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).expect("the openings prove");
    let leaves = refutation.binding.step_leaf_count;
    assert!(leaves > 1, "the fixture must have a space to be one short of");
    assert_eq!(
        check_execution_step_refutation_capped_v1(&refutation, &operands, leaves - 1),
        Err(PalwStepRefuteError::Leg(PalwStepLegError::LeafCountOutOfRange { got: leaves, max: leaves - 1 })),
        "one leaf short of the refutation's own space is a refusal BY NAME, not a verdict"
    );
    assert_eq!(
        check_execution_step_refutation_capped_v1(&refutation, &operands, leaves),
        Err(PalwStepRefuteError::NoFaultFound),
        "at exactly its own space the honest step is recomputed and found correct"
    );
    assert_eq!(
        check_execution_step_refutation_capped_v1(&refutation, &operands, PALW_STEP_LEG_MAX_LEAVES),
        check_execution_step_refutation_v1(&refutation, &operands),
        "the uncapped name is the capped one at the shipped ladder"
    );
}

/// **P0-8's owed end-to-end conviction, at this layer.** A node holding NO model recomputes a
/// BASE-0 matmul from proof-carried weights and convicts.
#[test]
fn palw_v2_matmul_fraud_convicts_without_model() {
    use crate::palw_artifact::PalwProvenOperandsV1;
    let (refutation, openings, artifact_root) = base0_matmul_fraud();

    // The oracle IS the openings, verified against the class's registered root. Nothing here
    // opens a file, and `NoWeights` — the "a full node has no model" stand-in — is what the
    // court would otherwise be stuck with.
    let operands = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).expect("the openings prove");
    let verdict = check_execution_step_refutation_v1(&refutation, &operands).expect("a recomputable step");
    assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: 3 });

    // And the same refutation WITHOUT the weights is `Unadjudicable`, never a conviction:
    // not being able to check is nobody's fault. This is the half that makes the assertion
    // above mean something — otherwise it would pass on a court that convicts blindly.
    assert_eq!(
        check_execution_step_refutation_v1(&refutation, &NoWeights),
        Err(PalwStepRefuteError::Unadjudicable),
        "with no weights the court must refuse, not convict"
    );

    // An opening against a DIFFERENT root proves nothing, so it never becomes an oracle.
    assert!(PalwProvenOperandsV1::from_openings_v1(&openings, h64(0xBD)).is_err());
}

/// **G5d: the gather is adjudicated from the registered table and a hash-checked token id.**
///
/// It used to return the identity of `inputs[0]`, which is an admission that a real gather
/// could not be checked — and it also forced the node to declare an input row that a pre
/// table has no upstream to supply. Both are gone.
///
/// The safety half is the point: the ids decide what "correct" means for this step, so an
/// unchecked list is a challenger naming whatever convicts an honest producer. They are
/// matched against the job context's own commitment before one of them is read.
#[test]
fn palw_v2_the_gather_reads_the_table_at_a_hash_checked_token_id() {
    let node = PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::EmbedLookup,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: "token_embd.weight".to_string(),
        weight_dtypes: vec![24],
        out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 4 },
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_EMBED),
        input_refs: Vec::new(),
    };
    // A four-row table: row t is [t, t+1, t+2, t+3] as int8 codes.
    struct Table;
    impl PalwWeightOracleV1 for Table {
        fn operand_bytes(&self, name: &str, _l: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>> {
            // `int8` table: one byte per value, so the byte range is the value range.
            if name != "token_embd.weight" || byte_offset + byte_len > 16 {
                return None;
            }
            Some((byte_offset..byte_offset + byte_len).map(|i| (i % 4 + i / 4) as u8).collect())
        }
    }
    let want = |t: u32| -> Vec<u32> { (0..4u32).map(|i| ((t * 4 + i) % 4 + t) as i32 as u32).collect() };

    let ids = [3u32, 1, 2];
    for (pos, id) in ids.iter().enumerate() {
        let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: pos as u32, tile_index: 0 };
        let got = base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&coord, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &[]),
        )
        .expect("a prefill gather adjudicates");
        assert_eq!(got, want(*id), "position {pos} must gather the row its token id names");
    }

    // **A DECODE gather adjudicates now** (ADR-0049 Decision E). It refused every decode
    // position, and the reason was right: a token pinned by nothing lets a challenger name
    // whatever convicts an honest producer. The ids are pinned — the caller has already
    // recomputed `full_logits_trace_root_v2` from the carried summary and them — so the arm
    // reads the token the claim itself committed. Decode call `c` consumes what call `c - 1`
    // generated.
    let generated: Vec<u32> = vec![2, 0];
    let decode = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 0, tile_index: 0 };
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&decode, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &generated)
        )
        .expect("a decode gather adjudicates against the claim's own generated ids"),
        want(generated[0]),
        "decode call 1 gathers the row the token generated by call 0 names"
    );
    let decode2 = PalwStepCoordinateV1 { call_index: 2, node_slot: 0, position: 1, tile_index: 0 };
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&decode2, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &generated)
        )
        .expect("and so does the next one"),
        want(generated[1])
    );
    // A decode position past what the claim generated stays a refusal, never a default —
    // index 0 would adjudicate every out-of-range gather against the first generated token.
    let past_decode = PalwStepCoordinateV1 { call_index: 9, node_slot: 0, position: 8, tile_index: 0 };
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&past_decode, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &generated)
        ),
        Err(PalwStepRefuteError::Unadjudicable)
    );
    // And with no ids carried at all it is unadjudicable rather than falling back to the prompt,
    // which would gather a prefill row for a decode step and convict an honest producer.
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&decode, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &[])
        ),
        Err(PalwStepRefuteError::Unadjudicable),
        "a decode step with no generated ids carried is unchecked, not decided"
    );

    // A position past the carried ids is a refusal, never a default: index 0 would adjudicate
    // every out-of-range gather against the FIRST token and convict honest producers.
    let past = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 9, tile_index: 0 };
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Table,
            1,
            (&past, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &[])
        ),
        Err(PalwStepRefuteError::Unadjudicable)
    );
    // A table that cannot serve the row is unadjudicable, not a conviction.
    struct Empty;
    impl PalwWeightOracleV1 for Empty {
        fn operand_bytes(&self, _n: &str, _l: Option<u16>, _r: u32, _e: u32) -> Option<Vec<u8>> {
            None
        }
    }
    let first = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 };
    assert_eq!(
        base0_row(
            Base0Op::Embed,
            &node,
            None,
            &profile(),
            &[],
            &Empty,
            1,
            (&first, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids), &[])
        ),
        Err(PalwStepRefuteError::Unadjudicable)
    );
}

/// **The carried ids are checked against the job context before any is read.**
///
/// Without this the gather is a false-slash machine: a challenger picks the ids that make an
/// honest producer's committed row look wrong, and the court convicts on the challenger's own
/// choice of what the input was.
#[test]
fn palw_v2_a_refutation_carrying_foreign_prompt_ids_is_refused() {
    let (binding, material, rows) = honest_execution();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let mut refutation = build_refutation(&binding, &material, &rows, coord);

    // Honest and empty: the refutation addresses no gather, and the check is inert.
    assert!(refutation.prompt_token_ids.is_empty());
    assert!(check_execution_step_refutation_v1(&refutation, &NoWeights).is_err(), "this fixture is a NoFault case");

    // Ids the job context does not commit to are refused as non-canonical, in the
    // `InputSetNotCanonical` family — the challenger's evidence is wrong, not the producer's.
    refutation.prompt_token_ids = vec![1, 2, 3];
    assert!(
        matches!(check_execution_step_refutation_v1(&refutation, &NoWeights), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
        "ids the context does not commit to must never reach a kernel"
    );

    // …and the ids the context DOES commit to pass the check (the step then fails on its own
    // merits, which is a different error).
    let mut honest_ids = binding.job_context.clone();
    let ids = vec![7u32, 8];
    honest_ids.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&ids);
    let mut with_ids = refutation.clone();
    with_ids.binding.job_context = honest_ids;
    with_ids.prompt_token_ids = ids;
    assert!(
        !matches!(
            check_execution_step_refutation_v1(&with_ids, &NoWeights),
            Err(PalwStepRefuteError::InputSetNotCanonical(m)) if m.contains("carried prompt ids")
        ),
        "ids matching the commitment must pass the id check"
    );
}

/// **An honest execution whose job context commits its prompt as a Merkle root**, with the
/// honest opening of that prompt's first tile — the court's fixture for its `ArithmeticOpened`
/// arm (`palw_court_v2::tests`). The step is the same one the flat fixtures refute; only what
/// the prompt is CALLED changed, which is exactly what the arm is about.
pub(crate) fn base0_merkle_prompt_honest() -> (PalwExecutionStepRefutationV1, crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1) {
    let ids = vec![7u32, 8];
    let merkle_root = crate::palw_prompt_ids_v1::prompt_token_ids_root_v1(&ids).expect("two ids commit");
    let (binding, material, rows) = honest_execution_with_context(|c| c.prompt_token_ids_hash = merkle_root);
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let refutation = build_refutation(&binding, &material, &rows, coord);
    let opening = crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&ids, 0).expect("position 0 opens");
    (refutation, opening)
}

/// **ADR-0081 Decision 3: an opening is bound to the job's own root before anything reads an
/// id out of it** — the G5d discipline, carried over to the openable commitment.
///
/// The failure this closes is the one G5d already names: a challenger who can NAME the prompt
/// ids convicts an honest producer, and under a Merkle commitment "naming them" means offering
/// a tile. Every way of offering one that does not bind the job's root is refused BY NAME, and
/// none of them reaches a kernel — the window a gather reads from is only constructible on the
/// far side of `verify_prompt_ids_opening_v1`.
#[test]
fn palw_v2_a_prompt_ids_opening_is_bound_to_the_jobs_root_before_it_is_read() {
    use crate::palw_prompt_ids_v1::{PalwPromptIdsOpeningV1, prompt_ids_opening_v1, prompt_token_ids_root_v1};
    // The fixture's job context declares two prefill tokens, so this is its whole prompt.
    let ids = vec![7u32, 8];
    let merkle_root = prompt_token_ids_root_v1(&ids).expect("two ids commit");
    let (binding, material, rows) = honest_execution_with_context(|c| c.prompt_token_ids_hash = merkle_root);
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let merkle = build_refutation(&binding, &material, &rows, coord);

    let refusal = |r: &PalwExecutionStepRefutationV1, o: Option<&PalwPromptIdsOpeningV1>| -> Option<&'static str> {
        match check_execution_step_refutation_opened_v1(r, &NoWeights, o) {
            Err(PalwStepRefuteError::InputSetNotCanonical(m)) => Some(m),
            _ => None,
        }
    };
    let honest = prompt_ids_opening_v1(&ids, 0).expect("position 0 opens");

    // The honest opening of the job's own prompt passes the id gate and the step is judged on
    // its own merits — which for this fixture is `NoFaultFound`, an HONEST producer.
    assert_eq!(
        check_execution_step_refutation_opened_v1(&merkle, &NoWeights, Some(&honest)),
        Err(PalwStepRefuteError::NoFaultFound),
        "an opening that binds the job's root must let the step be judged, not refuse it"
    );

    // One id altered: the tile no longer hashes to the leaf the opening names, and the refusal
    // is the scheme's own, not a generic one.
    let mut lying = honest.clone();
    lying.tile_ids[0] ^= 1;
    assert_eq!(refusal(&merkle, Some(&lying)), Some("the opened prompt ids do not hash to their leaf"));

    // A tile of a DIFFERENT prompt of a different length: refused at the count, before any
    // hashing — the structural-bounds-first order.
    let longer = vec![7u32, 8, 9];
    let foreign = prompt_ids_opening_v1(&longer, 0).expect("three ids open");
    assert_eq!(refusal(&merkle, Some(&foreign)), Some("the prompt-ids opening's length is not the one the job context declares"));

    // Both carriers at once is refused rather than resolved: two answers to "which id sits at
    // this position" with nothing choosing between them is ignored evidence.
    let mut both = merkle.clone();
    both.prompt_token_ids = ids.clone();
    assert_eq!(refusal(&both, Some(&honest)), Some("a refutation carries the prompt ids or an opening of them, never both"));

    // **And the COMMITMENT is the discriminator, not the fence.** On a job that committed the
    // FLAT digest, a Merkle opening cannot bind it and is refused by name — so offering one to
    // a network that has not armed `Params::palw_prompt_ids_merkle` is safe by arithmetic
    // rather than by the checker being told which form to expect.
    let flat_root = crate::palw_v2::prompt_token_ids_hash_v2(&ids);
    let (fb, fm, fr) = honest_execution_with_context(|c| c.prompt_token_ids_hash = flat_root);
    let flat = build_refutation(&fb, &fm, &fr, coord);
    assert_eq!(
        refusal(&flat, Some(&honest)),
        Some("the prompt-ids opening does not bind the job's committed prompt root"),
        "a Merkle opening must never authenticate against a flat commitment"
    );
    // …while the SAME job judged through the whole carried list is honest, so the two carriers
    // agree about the verdict wherever both are legal.
    let mut whole = flat.clone();
    whole.prompt_token_ids = ids.clone();
    assert_eq!(check_execution_step_refutation_v1(&whole, &NoWeights), Err(PalwStepRefuteError::NoFaultFound));
}

/// **ADR-0103 Decision 1's carriage: the list rides on a flat network, one tile on a Merkle
/// one** (`palw_refutation_prompt_carriage_v1`). A prefill leaf opens the tile holding ITS
/// position — and that tile authenticates against the job's Merkle root, which is the only way
/// the court reads it; a decode leaf reads the generated ids and carries neither; a refutation
/// that addresses no gather is untouched. The one-move court's bytes grow by the tile it rides.
#[test]
fn a_refutation_rides_the_list_on_a_flat_network_and_one_tile_on_a_merkle_one() {
    use crate::palw_prompt_ids_v1::{
        PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, prompt_token_ids_root_v1, verify_prompt_ids_opening_v1,
    };
    let ids: Vec<u32> = (0..70u32).map(|i| i * 3 + 1).collect();
    let root = prompt_token_ids_root_v1(&ids).expect("70 ids commit");
    // The carriage reads only the list and the leaf's coordinate; the fixture's own job is any.
    let (binding, material, rows) = honest_execution();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let mut refutation = build_refutation(&binding, &material, &rows, coord);
    refutation.prompt_token_ids = ids.clone();
    let at = |call_index: u32, position: u32| {
        let mut r = refutation.clone();
        r.output_preimage.coord.call_index = call_index;
        r.output_preimage.coord.position = position;
        r
    };

    // Flat: the prover's object, byte for byte.
    let (same, none) = palw_refutation_prompt_carriage_v1(PalwPromptIdsFormV1::Flat, at(0, 40)).expect("flat carries");
    assert_eq!(same, at(0, 40));
    assert!(none.is_none());

    // Merkle at prefill position 40: the list is out, and tile 1 (positions 32..64) rides.
    let (carried, tile) = palw_refutation_prompt_carriage_v1(PalwPromptIdsFormV1::MerkleV1, at(0, 40)).expect("the tile opens");
    assert!(carried.prompt_token_ids.is_empty(), "the list does not ride");
    let tile = tile.expect("a prefill leaf rides its tile");
    assert_eq!(tile.tile_index, 40 / PALW_PROMPT_IDS_TILE_LEN);
    let window = verify_prompt_ids_opening_v1(&root, 70, &tile).expect("the tile binds the job's root");
    assert_eq!(window.at(40), Some(ids[40]), "the window reads the leaf's own position");
    assert_eq!(window.at(0), None, "and nothing it does not carry");

    // Merkle at a decode leaf: the gather reads the generated ids, so neither rides.
    let (decode, none) = palw_refutation_prompt_carriage_v1(PalwPromptIdsFormV1::MerkleV1, at(3, 72)).expect("decode carries");
    assert!(decode.prompt_token_ids.is_empty() && none.is_none());

    // A refutation that addresses no gather is untouched on either form.
    let mut bare = at(0, 40);
    bare.prompt_token_ids.clear();
    let (untouched, none) = palw_refutation_prompt_carriage_v1(PalwPromptIdsFormV1::MerkleV1, bare.clone()).expect("carries");
    assert_eq!(untouched, bare);
    assert!(none.is_none());

    // The one-move court prices the tile it carries.
    let accusation = |prompt_ids_opening| crate::palw_shard_court_v1::PalwShardCourtAccusationV1 {
        version: crate::palw_shard_court_v1::PALW_SHARD_COURT_VERSION_V1,
        claim: Hash64::from_u64_word(1),
        execution_root: binding.committed_execution_root,
        trace_root: Hash64::from_u64_word(2),
        executor_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
            crate::tx::TransactionId::from_u64_word(3),
            0,
        )),
        accuser_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
            crate::tx::TransactionId::from_u64_word(4),
            0,
        )),
        leaf_index: carried.output_opening.leaf_index,
        refutation: carried.clone(),
        artifact_openings: vec![],
        prompt_ids_opening,
        signature: vec![],
    };
    let bare_bytes = crate::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(&accusation(None));
    let tile_bytes = borsh::to_vec(&tile).expect("borsh").len() as u64;
    assert_eq!(
        crate::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(&accusation(Some(tile.clone()))),
        bare_bytes + tile_bytes
    );
    // The wire order, pinned at the tail a round trip cannot see reordered: the tile rides after
    // the artifact openings and before the signature.
    let mut signed = accusation(Some(tile.clone()));
    signed.signature = vec![0xA5; 3];
    let wire = borsh::to_vec(&signed).expect("borsh");
    let tail = [borsh::to_vec(&Some(tile)).expect("borsh"), borsh::to_vec(&signed.signature).expect("borsh")].concat();
    assert!(wire.ends_with(&tail), "… artifact_openings, prompt_ids_opening, signature");
}

/// **G5c: the KV sentinels resolve to the cache-role nodes over the position history.**
///
/// `canonical_input_leaves` used to answer `None` for them — "KV / checkpoint arms:
/// registration-opaque today" — and a `None` is `Unadjudicable` before any kernel runs, so
/// attention was unreachable however capable the kernel became.
///
/// The fix needs no new leaf format and reads no float aux series: the cache contents are
/// already ordinary step tiles, because the K and V projection nodes carry `KCacheWrite` /
/// `VCacheWrite` and commit their output at every position. This asserts the resolution
/// itself — which node it names and which positions it spans — rather than that some function
/// returned `Some`.
#[test]
fn palw_v2_the_kv_sentinels_resolve_to_the_cache_nodes_over_the_history() {
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepNodeRoleV1};
    let mut p = base0_matmul_profile();
    // Every layer is an attention layer: `KvScaled` widths are only meaningful in a graph
    // that has one, and the cache roles live in the attention table.
    p.full_attention_interval = 1;
    let node = |kind, desc: &str, role, out, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: kind,
        role,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: out,
        tile_len: 16,
        kernel_semantics_id: kernel_semantics_id_v1(desc),
        input_refs: refs,
    };
    // slot 0: the layer input. 1: K cache write. 2: V cache write. 3: scores (q x K^T).
    p.gdn_nodes = Vec::new();
    p.attn_nodes = vec![
        node(
            PalwStepOpKindV1::Silu,
            KDESC_BASE0_SILU,
            PalwStepNodeRoleV1::Plain,
            crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
            vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
        ),
        node(
            PalwStepOpKindV1::Silu,
            KDESC_BASE0_SILU,
            PalwStepNodeRoleV1::KCacheWrite,
            crate::palw_step::PalwStepOutLenV1::Fixed { elements: 16 },
            vec![0],
        ),
        node(
            PalwStepOpKindV1::Silu,
            KDESC_BASE0_SILU,
            PalwStepNodeRoleV1::VCacheWrite,
            crate::palw_step::PalwStepOutLenV1::Fixed { elements: 16 },
            vec![0],
        ),
        node(
            PalwStepOpKindV1::MatMulQuant,
            KDESC_BASE0_MATMUL,
            PalwStepNodeRoleV1::Plain,
            crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier: 1 },
            vec![0, PALW_STEP_INPUT_KV_K],
        ),
    ];
    p.validate_shape().expect("the probe profile is well-formed");
    let mut ctx = context();
    ctx.shape_profile_id = p.shape_profile_id();

    // Challenge the scores node at the last prefill position (the fixture context declares
    // two prefill tokens, so positions are 0 and 1): it reads its query at position 1 and the
    // cached keys at positions 0 and 1.
    let scores_slot = 1 + 3; // pre(1) + attn slots 0..2
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 1, tile_index: 0 };
    let required = canonical_input_leaves_anchored(&p, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul), false)
        .expect("the KV sentinel resolves — this returned None before G5c");

    assert_eq!(required.len(), 2, "one group per input ref: the query row, then the whole key history");
    let query: Vec<_> = required[0].iter().map(|(_, c)| (c.node_slot, c.position)).collect();
    assert_eq!(query, vec![(1, 1), (1, 1)], "the query is this position's layer-input row, both its tiles");
    let keys: Vec<_> = required[1].iter().map(|(_, c)| (c.node_slot, c.position)).collect();
    assert_eq!(keys, vec![(2, 0), (2, 1)], "the keys are the KCacheWrite node (slot 2) at every position up to the challenged one");

    // The V sentinel names the OTHER role, so the two are not interchangeable.
    let mut v_node = p.clone();
    v_node.attn_nodes[3].input_refs = vec![0, PALW_STEP_INPUT_KV_V];
    let required_v = canonical_input_leaves_anchored(&v_node, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul), false)
        .expect("the V sentinel resolves too");
    assert_eq!(
        required_v[1].iter().map(|(_, c)| c.node_slot).collect::<Vec<_>>(),
        vec![3, 3],
        "the V sentinel names the VCacheWrite node (slot 3), not the K one"
    );

    // A layer with no such role names nothing, and that is a refusal rather than a guess.
    let mut roleless = p.clone();
    roleless.attn_nodes[1].role = PalwStepNodeRoleV1::Plain;
    assert!(
        canonical_input_leaves_anchored(&roleless, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul), false).is_none(),
        "no KCacheWrite node means the K sentinel names nothing"
    );
    // Two of them would make "the K cache" ambiguous, and a court that had to choose would be
    // choosing its own evidence.
    let mut doubled = p.clone();
    doubled.attn_nodes[2].role = PalwStepNodeRoleV1::KCacheWrite;
    assert!(
        canonical_input_leaves_anchored(&doubled, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul), false).is_none(),
        "two KCacheWrite nodes is an ambiguous cache"
    );
}

#[test]
fn honest_steps_recompute_to_no_fault() {
    let (binding, material, rows) = honest_execution();
    let p = &binding.shape_profile;
    let ctx = &binding.job_context;
    // Adjudicate an L2Norm step, a Sigmoid step, and the GDN core at the decode call.
    for (slot, tile) in [(1u32, 0u32), (3, 1), (6, 0), (6, 1)] {
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: slot, position: 0, tile_index: tile };
        let out_idx = canonical_step_leaf_index(p, ctx, &coord).unwrap();
        let refutation = build_refutation(&binding, &material, &rows, coord);
        let got = check_execution_step_refutation_v1(&refutation, &NoWeights);
        assert_eq!(got, Err(PalwStepRefuteError::NoFaultFound), "slot {slot} tile {tile} leaf {out_idx}");
    }
}

#[test]
fn tampered_output_convicts_with_computation_mismatch() {
    let (mut binding, mut material, rows) = honest_execution();
    let p = binding.shape_profile.clone();
    let ctx = binding.job_context.clone();
    // Corrupt ONE committed value of the decode-call GDN output and rebuild the tree —
    // the miner's world where the computation lies.
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 6, position: 0, tile_index: 1 };
    let idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
    let ctx_hash = ctx.context_hash();
    let profile_hash = p.shape_profile_id();
    let mut row = rows[2][6].clone();
    row[16 + 3] = ref_add_v1(row[16 + 3], 0x3F80_0000); // +1.0 to value 3 of tile 1
    let leaf = PalwStepTileLeafV1 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        coord,
        value_count: 16,
        values_le: row[16..32].iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    material.leaf_hashes[idx as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
    let merkle = crate::palw_step_leg::step_merkle_root_v1(&material.leaf_hashes).unwrap();
    material.merkle_root = merkle;
    binding.step_merkle_root = merkle;
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &merkle);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        1,
        0,
        &binding.checkpoint_merkle_root,
    );
    binding.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &binding.full_logits_trace_root, &binding.activation_leg_root, &ckpt_root, &step_root);
    let mut refutation = build_refutation(&binding, &material, &rows, coord);
    refutation.output_preimage = leaf;
    refutation.output_opening = step_opening_v1(&material.leaf_hashes, idx).unwrap();
    let verdict = check_execution_step_refutation_v1(&refutation, &NoWeights).unwrap();
    assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: 3 });
}

#[test]
fn wrong_input_set_is_rejected_not_convicted() {
    let (binding, material, rows) = honest_execution();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
    let mut refutation = build_refutation(&binding, &material, &rows, coord);
    // Swap in a DIFFERENT honest tile as "the input".
    let alien_coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 1, tile_index: 0 };
    let alien_idx = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &alien_coord).unwrap();
    refutation.inputs[0].preimages[0] = tile_preimage(&rows, &binding, alien_coord);
    refutation.inputs[0].run_siblings[0] =
        crate::palw_step_leg::step_merkle_range_siblings_v1(&material.leaf_hashes, alien_idx as usize, 1).unwrap();
    let got = check_execution_step_refutation_v1(&refutation, &NoWeights);
    assert!(
        matches!(got, Err(PalwStepRefuteError::InputSetNotCanonical(_))),
        "an alien input must reject the refutation, got {got:?}"
    );
}

/// A refutation that passes every STRUCTURAL check and then meets an uncatalogued kernel — the
/// only route to `Unadjudicable`, and therefore the only way anything downstream can test what
/// a coverage gap does.
///
/// `skeleton_refutation` beside it deliberately cannot get this far (it fails on its opening
/// path), which is why the class-freeze rule was recorded twice as an untested path. Exposed so
/// `palw_facts` can build the conviction carriage that reaches its freeze end to end, and used
/// by the test below so the two cannot drift into disagreeing about what "unadjudicable" is.
pub(crate) fn unadjudicable_refutation() -> PalwExecutionStepRefutationV1 {
    let (binding, material, rows) = honest_execution();
    // The post node carries an unimplemented descriptor.
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 7, position: 0, tile_index: 0 };
    build_refutation(&binding, &material, &rows, coord)
}

#[test]
fn unknown_kernel_is_unadjudicable() {
    assert_eq!(check_execution_step_refutation_v1(&unadjudicable_refutation(), &NoWeights), Err(PalwStepRefuteError::Unadjudicable));
}

// --- helpers ---

fn tile_preimage(rows: &[Vec<Vec<u32>>], binding: &PalwStepBindingV2, coord: PalwStepCoordinateV1) -> PalwStepTileLeafV1 {
    let ord = match (coord.call_index, coord.position) {
        (0, 0) => 0usize,
        (0, 1) => 1,
        (1, 0) => 2,
        _ => unreachable!(),
    };
    let row = &rows[ord][coord.node_slot as usize];
    let start = coord.tile_index as usize * 16;
    let end = (start + 16).min(row.len());
    let _ = binding;
    PalwStepTileLeafV1 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        coord,
        value_count: (end - start) as u32,
        values_le: row[start..end].iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}

pub(super) fn build_refutation(
    binding: &PalwStepBindingV2,
    material: &crate::palw_step_leg::PalwStepLegMaterialV1,
    rows: &[Vec<Vec<u32>>],
    coord: PalwStepCoordinateV1,
) -> PalwExecutionStepRefutationV1 {
    let p = &binding.shape_profile;
    let ctx = &binding.job_context;
    let out_idx = canonical_step_leaf_index(p, ctx, &coord).unwrap();
    let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
    let program = resolve_kernel(&node.kernel_semantics_id);
    // The ROW form the carrier requires: preimages plus one sibling set per derived run —
    // built with the exported range builder, exactly as a real challenger builds it.
    let inputs = match program {
        Some(prog) => canonical_input_leaves_anchored(p, ctx, coord.node_slot, &coord, prog, false)
            .unwrap_or_default()
            .into_iter()
            .map(|row| {
                let preimages = row.iter().map(|(_, c)| tile_preimage(rows, binding, *c)).collect();
                let mut runs: Vec<(usize, usize)> = Vec::new();
                for (i, (idx, _)) in row.iter().enumerate() {
                    match runs.last_mut() {
                        Some((start, len)) if row[*start].0 + *len as u64 == *idx => *len += 1,
                        _ => runs.push((i, 1)),
                    }
                }
                let run_siblings = runs
                    .iter()
                    .map(|(start, len)| {
                        crate::palw_step_leg::step_merkle_range_siblings_v1(&material.leaf_hashes, row[*start].0 as usize, *len)
                            .unwrap()
                    })
                    .collect();
                PalwStepInputRowV1 { preimages, run_siblings }
            })
            .collect(),
        None => Vec::new(),
    };
    PalwExecutionStepRefutationV1 {
        binding: binding.clone(),
        output_opening: step_opening_v1(&material.leaf_hashes, out_idx).unwrap(),
        output_preimage: tile_preimage(rows, binding, coord),
        inputs,
        prompt_token_ids: Vec::new(),
        decode_tokens: None,
        kv_checkpoint: None,
    }
}

/// Recompute a binding's `committed_execution_root` from its own parts — what every fixture
/// that edits a bound field must do, extracted so they cannot each do it differently.
pub(crate) fn rebind_committed_root(binding: &mut PalwStepBindingV2) {
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let decode_calls = binding.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &binding.step_merkle_root);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        decode_calls,
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
    );
    binding.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &binding.full_logits_trace_root, &binding.activation_leg_root, &ckpt_root, &step_root);
}

/// The matmul fixture's execution, re-committed with a REAL integer decode commitment:
/// `full_logits_trace_root` becomes [`base0_logits_trace_root_v1`] over the given rows and
/// ids. The rows must be `exact_decode_tokens` of them, each `vocab_size` wide — the shape
/// the pin check enforces.
pub(crate) fn base0_binding_with_decode_root(
    logits_rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
) -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>, PalwBase0DecodeTokensV1) {
    let (mut binding, material, rows) = base0_honest_execution();
    binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, &logits_rows, &generated);
    rebind_committed_root(&mut binding);
    (binding, material, rows, PalwBase0DecodeTokensV1 { logits_rows, generated_token_ids: generated })
}

/// Deterministic logits rows at the fixture's shape (2 decode calls, vocabulary 40), and the
/// ids the pinned selection rule derives from them.
pub(crate) fn base0_honest_decode_commitment()
-> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>, PalwBase0DecodeTokensV1) {
    let (b, _, _) = base0_honest_execution();
    let vocab = b.shape_profile.vocab_size as usize;
    let decode = b.job_context.exact_decode_tokens as usize;
    let logits_rows: Vec<Vec<i32>> = (0..decode).map(|c| (0..vocab).map(|i| ((c * 31 + i * 7) % 23) as i32 - 11).collect()).collect();
    let generated: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    base0_binding_with_decode_root(logits_rows, generated)
}

/// Assemble a challenger's tiled pin for one position, from the full rows — the same work a
/// real challenger does from served material, so a test that uses it is exercising the real
/// opening path and not a shortcut past it.
fn tiled_pin(
    ctx: &crate::palw_v2::PalwJobContextV2,
    logits_rows: &[Vec<i32>],
    generated: &[u32],
    position: u32,
    beat_lane: u32,
) -> PalwTiledDecodePinV1 {
    // Promoted to the prover every challenger now calls (ADR-0152 v3.1 §4-bis.5).
    tiled_decode_pin_v1(ctx, logits_rows, generated, position, beat_lane).expect("the fixture's rows build the pin")
}

/// **A disclosure authenticates exactly what the decode close does** (ADR-0062 SA-2; mainnet
/// audit 2026-09-05). The tiled row and tile authenticators are the decode close's own checks
/// restated for one tile, and this holds the two spellings equal over the same rows: what the
/// close opens, the disclosure opens; every bend the close refuses, the disclosure refuses.
#[test]
fn the_disclosure_authenticates_exactly_what_the_decode_close_does() {
    let (mut binding, _m, _r, _) = base0_honest_decode_commitment();
    let vocab = 10_000usize;
    let decode = binding.job_context.exact_decode_tokens as usize;
    binding.shape_profile.vocab_size = vocab as u32;
    binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
    // The fold pins the binding (`verify_binding_v1`) before it reads a row, so the context
    // must name the profile it was edited into — the decode close's own test skips that pin.
    binding.job_context.shape_profile_id = binding.shape_profile.shape_profile_id();
    let logits_rows: Vec<Vec<i32>> =
        (0..decode).map(|c| (0..vocab).map(|i| ((c * 131 + i * 7919) % 65_536) as i32 - 32_768).collect()).collect();
    let generated: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, &logits_rows, &generated).expect("a tree");
    rebind_committed_root(&mut binding);
    let cap = crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
    let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64;
    let roots = (binding.full_logits_trace_root, binding.committed_execution_root);

    // Every (row, tile) the run has opens, and is accepted by the checker.
    for row in 0..decode as u32 {
        for tile in 0..tiles as u8 {
            let (row_root, row_opening, lanes, tile_opening) =
                tiled_trace_event_disclosure_v1(&binding.job_context, &logits_rows, row, tile).expect("in the run");
            let d = PalwTraceEventDisclosureV1::Tiled {
                binding: Box::new(binding.clone()),
                generated_token_ids: generated.clone(),
                row_root,
                row_opening,
                tile_lanes: lanes,
                tile_opening,
            };
            check_trace_event_disclosure_v1(roots.0, roots.1, row, tile, &d, cap).expect("an honest tile opens");
            // The decode close's own pin over the same row agrees about the row.
            let pin = tiled_pin(&binding.job_context, &logits_rows, &generated, row, 0);
            assert_eq!(
                tiled_row_authenticate_v1(&binding, &pin.generated_token_ids, row, &pin.row_root, &pin.row_opening, cap)
                    .map(|(_, v, t)| (v, t)),
                Ok((vocab, tiles)),
                "the disclosure's row authentication is the close's"
            );
        }
    }

    // Past the run: `OutOfRange` is the answer, and an opening is not.
    let out = PalwTraceEventDisclosureV1::OutOfRange { binding: Box::new(binding.clone()) };
    check_trace_event_disclosure_v1(roots.0, roots.1, decode as u32, 0, &out, cap).expect("a row past the run is absent");
    check_trace_event_disclosure_v1(roots.0, roots.1, 0, tiles as u8, &out, cap).expect("a tile past the row is absent");
    assert!(
        check_trace_event_disclosure_v1(roots.0, roots.1, 0, 0, &out, cap).is_err(),
        "an event the run has may not be declared absent"
    );
    assert!(tiled_trace_event_disclosure_v1(&binding.job_context, &logits_rows, decode as u32, 0).is_none());

    // The bends the close refuses, the disclosure refuses: a lane, an id, a path, the wrong
    // tile under the accused index, and a binding of another execution.
    let (row_root, row_opening, lanes, tile_opening) =
        tiled_trace_event_disclosure_v1(&binding.job_context, &logits_rows, 1, 1).unwrap();
    let honest = PalwTraceEventDisclosureV1::Tiled {
        binding: Box::new(binding.clone()),
        generated_token_ids: generated.clone(),
        row_root,
        row_opening,
        tile_lanes: lanes,
        tile_opening,
    };
    let refused = |edit: &dyn Fn(&mut PalwTraceEventDisclosureV1), what: &str| {
        let mut bent = honest.clone();
        edit(&mut bent);
        assert!(check_trace_event_disclosure_v1(roots.0, roots.1, 1, 1, &bent, cap).is_err(), "{what} must be refused");
    };
    refused(
        &|d| {
            if let PalwTraceEventDisclosureV1::Tiled { tile_lanes, .. } = d {
                tile_lanes[3] ^= 1
            }
        },
        "a bent lane",
    );
    refused(
        &|d| {
            if let PalwTraceEventDisclosureV1::Tiled { generated_token_ids, .. } = d {
                generated_token_ids[0] ^= 1
            }
        },
        "a bent id",
    );
    refused(
        &|d| {
            if let PalwTraceEventDisclosureV1::Tiled { tile_opening, .. } = d {
                tile_opening.siblings.push(Hash64::from_u64_word(0x77))
            }
        },
        "a longer path",
    );
    refused(
        &|d| {
            if let PalwTraceEventDisclosureV1::Tiled { row_opening, .. } = d {
                row_opening.leaf_index ^= 1
            }
        },
        "another row's opening",
    );
    assert!(check_trace_event_disclosure_v1(roots.0, roots.1, 1, 2, &honest, cap).is_err(), "tile 1 does not answer for tile 2");
    assert!(
        check_trace_event_disclosure_v1(Hash64::from_u64_word(0xBAD), roots.1, 1, 1, &honest, cap).is_err(),
        "another claim's trace root"
    );
    assert!(
        check_trace_event_disclosure_v1(roots.0, Hash64::from_u64_word(0xBAD), 1, 1, &honest, cap).is_err(),
        "another claim's execution root"
    );
    // A disclosure carries one tile where the close carries two: it fits wherever that fits.
    let bytes = borsh::to_vec(&honest).unwrap().len();
    assert!(bytes < crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES as usize, "{bytes} bytes against the close ceiling");
}

/// **The tiled scheme convicts a lying token and clears an honest one, at a vocabulary the
/// flat pin could not carry** — multi-tile rows with a ragged last tile, the exact geometry
/// Qwen-scale classes have. Also measures the pin: the whole close stays under the 80 KiB a
/// lifecycle carrier can relay, which is the number this scheme exists for.
#[test]
fn the_tiled_decode_pin_convicts_and_stays_carriable() {
    let (mut binding, _m, _r, _) = base0_honest_decode_commitment();
    // A vocabulary of 10,000: three tiles, the last ragged — past anything the flat pin
    // admits at this budget, small enough for a test to build rows for.
    let vocab = 10_000usize;
    let decode = binding.job_context.exact_decode_tokens as usize;
    binding.shape_profile.vocab_size = vocab as u32;
    // A class that COMMITS tiled: the fixture registers flat, and the arm now refuses a pin
    // whose scheme is not the class's.
    binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
    let logits_rows: Vec<Vec<i32>> =
        (0..decode).map(|c| (0..vocab).map(|i| ((c * 131 + i * 7919) % 65_536) as i32 - 32_768).collect()).collect();
    let generated: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    binding.full_logits_trace_root =
        tiled_logits_trace_root_v1(&binding.job_context, &logits_rows, &generated).expect("the fixture's rows build a tree");
    rebind_committed_root(&mut binding);

    // Honest: no beating lane exists, whichever tile a challenger opens.
    for p in 0..decode as u32 {
        let argmax = generated[p as usize];
        let elsewhere = if argmax == 0 { 1 } else { argmax - 1 };
        let pin = tiled_pin(&binding.job_context, &logits_rows, &generated, p, elsewhere);
        assert!(
            matches!(check_tiled_decode_token_refutation_v1(&binding, &pin), Err(PalwStepRefuteError::NoFaultFound)),
            "an honest committed token at position {p} clears"
        );
    }

    // The lie is inside the commitment, exactly as a fraudulent producer would make it.
    let mut lying = generated.clone();
    lying[1] = lying[1].wrapping_add(3) % vocab as u32;
    binding.full_logits_trace_root =
        tiled_logits_trace_root_v1(&binding.job_context, &logits_rows, &lying).expect("the fixture's rows build a tree");
    rebind_committed_root(&mut binding);
    let true_argmax = generated[1];
    let pin = tiled_pin(&binding.job_context, &logits_rows, &lying, 1, true_argmax);
    let verdict = check_tiled_decode_token_refutation_v1(&binding, &pin).expect("the true argmax beats the lie");
    assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 1 });

    // The carriable-close measurement, at QWEN3.6's real vocabulary arithmetic: two tiles of
    // 4,096 lanes, the paths a 248,320-lane row needs, the ids of the canonical job.
    let tile_bytes = PALW_LOGITS_TILE_LANES * 4;
    let tiles_per_row = 248_320usize.div_ceil(PALW_LOGITS_TILE_LANES);
    let path = 64 * tiles_per_row.next_power_of_two().trailing_zeros() as usize;
    let close = 2 * (tile_bytes + path) + 64 + 64 + 4 * decode + 64;
    assert!(close < 80 * 1024, "the tiled close is {close} bytes against the 80 KiB carrier budget");

    // And a tampered pin is malformed evidence, not a verdict: bend one lane of the beat tile.
    let mut bent = tiled_pin(&binding.job_context, &logits_rows, &lying, 1, true_argmax);
    bent.beat_tile_lanes[0] = bent.beat_tile_lanes[0].wrapping_add(1);
    assert!(
        matches!(check_tiled_decode_token_refutation_v1(&binding, &bent), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
        "a bent tile must be refused as evidence, never adjudicated"
    );
}

/// **Z8's refutation half: under a seed and `T_q > 0` the two-disclosure arm convicts a token
/// that is not the keyed argmax and acquits the one that is** — and at `GREEDY` it is the
/// shipped arm, verdict for verdict, on the same fixture the test above uses.
///
/// The whole point of Decision 11's key being a PER-LANE function is exercised here: the court
/// opens exactly two tiles, computes two keys, and compares. Nothing about the row's other
/// 9,998 lanes is fetched, at any temperature.
#[test]
fn the_seeded_arm_convicts_the_token_that_is_not_the_keyed_argmax() {
    use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
    // A temperature of 16 in the class's own logit units (Q24), which is large beside this
    // fixture's ~6-unit spacing between adjacent logits — the row has to be genuinely
    // re-orderable by the noise or the test proves nothing about the sampler.
    let sampling = PalwDecodeSamplingV2 { seed: [0x2Bu8; 32], temperature_q: 1 << 28 };
    let (mut binding, _m, _r, _) = base0_honest_decode_commitment();
    let vocab = 10_000usize;
    let decode = binding.job_context.exact_decode_tokens as usize;
    binding.shape_profile.vocab_size = vocab as u32;
    binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
    let logits_rows: Vec<Vec<i32>> =
        (0..decode).map(|c| (0..vocab).map(|i| ((c * 131 + i * 7919) % 65_536) as i32 - 32_768).collect()).collect();

    // The HONEST producer under this seed commits the keyed argmax, which is a different token
    // from the greedy one — otherwise the test would prove nothing about the sampler.
    let seeded: Vec<u32> = logits_rows.iter().enumerate().map(|(p, r)| sampling.select(r, p as u32) as u32).collect();
    let greedy: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    assert_ne!(seeded, greedy, "the fixture's temperature must actually move the selection");

    binding.full_logits_trace_root =
        tiled_logits_trace_root_v1(&binding.job_context, &logits_rows, &seeded).expect("the fixture's rows build a tree");
    rebind_committed_root(&mut binding);

    // Acquits the honest seeded token against EVERY other lane a challenger might open.
    for p in 0..decode as u32 {
        for beat in [0u32, 1, seeded[p as usize].wrapping_add(1) % vocab as u32, greedy[p as usize], vocab as u32 - 1] {
            if beat == seeded[p as usize] {
                continue;
            }
            let pin = tiled_pin(&binding.job_context, &logits_rows, &seeded, p, beat);
            assert!(
                matches!(check_tiled_decode_token_refutation_v2(&binding, &pin, sampling), Err(PalwStepRefuteError::NoFaultFound)),
                "the keyed argmax at position {p} must clear against lane {beat}"
            );
        }
    }

    // And convicts a producer that committed the GREEDY token while its job declared a
    // temperature — the exact fraud a sampler makes possible, caught by the same two tiles.
    binding.full_logits_trace_root =
        tiled_logits_trace_root_v1(&binding.job_context, &logits_rows, &greedy).expect("the fixture's rows build a tree");
    rebind_committed_root(&mut binding);
    let differs = (0..decode as u32).find(|p| seeded[*p as usize] != greedy[*p as usize]).expect("checked above");
    let pin = tiled_pin(&binding.job_context, &logits_rows, &greedy, differs, seeded[differs as usize]);
    let verdict = check_tiled_decode_token_refutation_v2(&binding, &pin, sampling).expect("the keyed argmax beats the greedy one");
    assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: differs });

    // The same close, under GREEDY, is the shipped verdict — an acquittal, because the greedy
    // token IS the greedy argmax. The v1 entry point and the v2 arm at GREEDY agree.
    assert!(matches!(check_tiled_decode_token_refutation_v1(&binding, &pin), Err(PalwStepRefuteError::NoFaultFound)));
    assert!(matches!(
        check_tiled_decode_token_refutation_v2(&binding, &pin, PalwDecodeSamplingV2::GREEDY),
        Err(PalwStepRefuteError::NoFaultFound)
    ));
}

/// **The flat arm's twin of the same property**, so arming the fence cannot convict honest
/// tokens on one family while acquitting them on the other.
#[test]
fn the_seeded_flat_arm_agrees_with_the_seeded_tiled_one() {
    use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
    // 16 in logit units, against a 40-lane row whose values span ~429 — see the tiled twin.
    let sampling = PalwDecodeSamplingV2 { seed: [0x77u8; 32], temperature_q: 1 << 28 };
    let (b, _, _, _) = base0_honest_decode_commitment();
    let vocab = b.shape_profile.vocab_size as usize;
    let decode = b.job_context.exact_decode_tokens as usize;
    let logits_rows: Vec<Vec<i32>> =
        (0..decode).map(|c| (0..vocab).map(|i| ((c * 37 + i * 11) % 4_096) as i32 - 2_048).collect()).collect();
    let seeded: Vec<u32> = logits_rows.iter().enumerate().map(|(p, r)| sampling.select(r, p as u32) as u32).collect();
    let greedy: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    assert_ne!(seeded, greedy, "the fixture's temperature must actually move the selection");

    let (honest, _, _, pin) = base0_binding_with_decode_root(logits_rows.clone(), seeded.clone());
    for p in 0..decode as u32 {
        assert!(matches!(check_base0_decode_token_refutation_v2(&honest, &pin, p, sampling), Err(PalwStepRefuteError::NoFaultFound)));
    }
    // The first position the two rules disagree about — the seed moves some positions and not
    // others, and a test that assumed position 0 was one of them was asserting the fixture.
    let differs = (0..decode as u32).find(|p| seeded[*p as usize] != greedy[*p as usize]).expect("checked above");
    let (lying, _, _, lying_pin) = base0_binding_with_decode_root(logits_rows, greedy);
    let verdict =
        check_base0_decode_token_refutation_v2(&lying, &lying_pin, differs, sampling).expect("the greedy token is not the draw");
    assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: differs });
    // GREEDY is the shipped arm: the same commitment clears at every position.
    for p in 0..decode as u32 {
        assert!(matches!(check_base0_decode_token_refutation_v1(&lying, &lying_pin, p), Err(PalwStepRefuteError::NoFaultFound)));
    }
}

/// **Neither close arm speaks the other class's scheme.** A flat pin against a tiled class
/// and a tiled pin against a flat class are both malformed EVIDENCE — refused before any
/// arithmetic, never adjudicated under rules the class did not register. This is the
/// per-dispute half of the scheme binding; the admission half refuses unknown schemes.
#[test]
fn a_pin_that_does_not_speak_the_class_scheme_is_refused() {
    // Flat pin, tiled class — the class mutation made COHERENTLY: the job declares the new
    // profile id and the root is recommitted, so the refusal below is the scheme gate's and
    // not the binding verifier's (which runs first on this entry and would otherwise answer
    // `ShapeProfileNotTheDeclaredOne` — about a different defect).
    let (mut binding, _m, _r, pin) = base0_honest_decode_commitment();
    binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
    binding.job_context.shape_profile_id = binding.shape_profile.shape_profile_id();
    binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, &pin.logits_rows, &pin.generated_token_ids);
    rebind_committed_root(&mut binding);
    let err = check_base0_decode_token_refutation_v1(&binding, &pin, 0).expect_err("a flat pin cannot adjudicate a tiled class");
    assert!(
        matches!(err, PalwStepRefuteError::InputSetNotCanonical(why) if why.contains("flat pin is not its scheme")),
        "refused as evidence, by name"
    );

    // Tiled pin, flat class: the fixture's profile registers flat, and the tiled arm refuses
    // before reading a single lane.
    let (binding, _m2, _r2, _) = base0_honest_decode_commitment();
    let hollow = PalwTiledDecodePinV1 {
        position: 0,
        generated_token_ids: vec![0; binding.job_context.exact_decode_tokens as usize],
        row_root: Hash64::default(),
        row_opening: crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: vec![] },
        committed_tile_lanes: vec![],
        committed_opening: crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: vec![] },
        beat_tile_lanes: vec![],
        beat_opening: crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: vec![] },
        beat_lane: 0,
    };
    let err = check_tiled_decode_token_refutation_v1(&binding, &hollow).expect_err("a tiled pin cannot adjudicate a flat class");
    assert!(
        matches!(err, PalwStepRefuteError::InputSetNotCanonical(why) if why.contains("tiled pin is not its scheme")),
        "refused as evidence, by name — before its emptiness is even looked at"
    );
}

/// **ADR-0049 Decision E fires, and fires positionally.** An honest commitment clears at
/// every decode position; a commitment whose id at position 1 is not the selection rule's
/// answer convicts at position 1 with `DecodeTokenMismatch { position: 1 }` — and STILL
/// clears at position 0, because the fault is the lie, not the producer's neighborhood.
#[test]
fn the_decode_token_selection_rule_convicts_exactly_the_lying_position() {
    let (binding, _m, _r, pin) = base0_honest_decode_commitment();
    for p in 0..pin.generated_token_ids.len() as u32 {
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&binding, &pin, p), Err(PalwStepRefuteError::NoFaultFound)),
            "an honest committed token at position {p} clears"
        );
    }
    // The lie is INSIDE the commitment: the root is recomputed over the lying ids, exactly
    // as a fraudulent producer would commit it.
    let mut lying_ids = pin.generated_token_ids.clone();
    lying_ids[1] = lying_ids[1].wrapping_add(1);
    let (lb, _m2, _r2, lpin) = base0_binding_with_decode_root(pin.logits_rows.clone(), lying_ids);
    let verdict = check_base0_decode_token_refutation_v1(&lb, &lpin, 1).expect("a lying committed token convicts");
    assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 1 });
    assert!(
        matches!(check_base0_decode_token_refutation_v1(&lb, &lpin, 0), Err(PalwStepRefuteError::NoFaultFound)),
        "position 0's token was honest, and stays cleared"
    );
    assert!(
        matches!(check_base0_decode_token_refutation_v1(&lb, &lpin, 7), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
        "a position outside the job's decode calls is malformed evidence, never a verdict"
    );
}

/// **The tie rule is the LOWEST index, and the court holds a producer to it.** Two lanes tie
/// at the max; a producer that committed the lower twin clears, one that committed the
/// higher twin is convicted — same rows, same values, one rule.
#[test]
fn the_decode_token_tie_breaks_to_the_lowest_index() {
    let (b, _, _) = base0_honest_execution();
    let vocab = b.shape_profile.vocab_size as usize;
    let decode = b.job_context.exact_decode_tokens as usize;
    let mut row = vec![-5i32; vocab];
    row[3] = 7;
    row[9] = 7; // the twin
    let rows: Vec<Vec<i32>> = (0..decode).map(|_| row.clone()).collect();

    let (hb, _m, _r, hpin) = base0_binding_with_decode_root(rows.clone(), vec![3; decode]);
    for p in 0..decode as u32 {
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&hb, &hpin, p), Err(PalwStepRefuteError::NoFaultFound)),
            "the lower twin is the selection"
        );
    }
    let (gb, _m2, _r2, gpin) = base0_binding_with_decode_root(rows, vec![9; decode]);
    let verdict = check_base0_decode_token_refutation_v1(&gb, &gpin, 0).expect("the higher twin convicts");
    assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 0 });
}

/// **A pin the commitment does not authenticate is refused, never adjudicated.** One lane
/// flipped, one id flipped, one row the wrong width, the wrong row count — each is malformed
/// evidence by name, and none of them can convict anyone.
#[test]
fn a_decode_pin_the_committed_root_does_not_authenticate_is_refused() {
    let (binding, _m, _r, pin) = base0_honest_decode_commitment();
    let cases: Vec<(&str, PalwBase0DecodeTokensV1)> = vec![
        ("a flipped lane", {
            let mut p = pin.clone();
            p.logits_rows[0][0] += 1;
            p
        }),
        ("a flipped id", {
            let mut p = pin.clone();
            p.generated_token_ids[0] = p.generated_token_ids[0].wrapping_add(1);
            p
        }),
        ("a row the wrong width", {
            let mut p = pin.clone();
            p.logits_rows[1].pop();
            p
        }),
        ("the wrong row count", {
            let mut p = pin.clone();
            p.logits_rows.pop();
            p
        }),
    ];
    for (what, bad) in cases {
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&binding, &bad, 0), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
            "{what} must be refused as malformed evidence"
        );
    }
}

/// **The lane guard is refusal by name, in both directions.** A `Float32` class has no
/// decode-token adjudicator yet, so a close against one is refused rather than adjudicated
/// wrongly; and at the step-refutation dispatch, an `Int32` class handed a `FloatV2` pin — or
/// a pin whose root recompute fails — is refused before a single id is read.
#[test]
fn a_pin_that_does_not_speak_the_class_lane_is_refused() {
    // The close guard: same execution, lane rewritten to Float32 (with the context and the
    // committed root made consistent, so the refusal is the LANE's and not the binding's).
    let (binding, _m, _r, pin) = base0_honest_decode_commitment();
    let mut fb = binding.clone();
    fb.shape_profile.lane = crate::palw_step::PalwStepLaneV1::Float32;
    fb.job_context.shape_profile_id = fb.shape_profile.shape_profile_id();
    rebind_committed_root(&mut fb);
    match check_base0_decode_token_refutation_v1(&fb, &pin, 0) {
        Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
            assert!(msg.contains("Float32"), "the refusal names the missing adjudicator: {msg}")
        }
        other => panic!("a Float32 class must be refused by name, got {other:?}"),
    }

    // The step-dispatch guard: a decode-embed refutation of the Int32 class carrying the
    // FLOAT scheme's pin. The refusal happens before any gather or oracle is consulted.
    let (ib, material, rows, _) = base0_honest_decode_commitment();
    let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 0, tile_index: 0 };
    let mut refutation = build_refutation(&ib, &material, &rows, coord);
    refutation.decode_tokens = Some(PalwDecodeTokenPinV1::FloatV2(PalwDecodeTokensV1 {
        summary: crate::palw_v2::PalwTraceSummaryV2 {
            vocab_size: 40,
            logits_dtype: crate::palw_v2::PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: 2,
            exact_decode_tokens: 2,
            event_count: 4,
            first_event_kind: crate::palw_v2::PalwTracePhaseV2::Prefill,
            last_event_kind: crate::palw_v2::PalwTracePhaseV2::Decode,
            output_token_ids_hash: crate::palw_v2::output_token_ids_hash_v2(&[1, 2]),
            stop_reason: crate::palw_v2::PalwStopReasonV2::ExactBudgetReached,
        },
        ordered_event_commitment: h64(0xF0),
        generated_token_ids: vec![1, 2],
    }));
    let empty_oracle = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&[], h64(0x1)).unwrap();
    match check_execution_step_refutation_v1(&refutation, &empty_oracle) {
        Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
            assert!(msg.contains("lane"), "the dispatch refuses the wrong-scheme pin by name: {msg}")
        }
        other => panic!("an Int32 class handed a FloatV2 pin must be refused, got {other:?}"),
    }

    // And a Base0 pin whose recompute does not reproduce the committed root — the same
    // refusal, at the same point, before anything downstream runs.
    let (ub, umaterial, urows) = base0_honest_execution(); // root left at the h64(0xAA) placeholder
    let mut unrooted = build_refutation(&ub, &umaterial, &urows, coord);
    unrooted.decode_tokens = Some(PalwDecodeTokenPinV1::Base0V1(pin));
    match check_execution_step_refutation_v1(&unrooted, &empty_oracle) {
        Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
            assert!(msg.contains("integer trace root"), "the dispatch refuses an unauthenticated pin by name: {msg}")
        }
        other => panic!("a pin the committed root does not authenticate must be refused, got {other:?}"),
    }
}

#[test]
fn total_leaf_shape_sanity() {
    let p = profile();
    // Honest material declares the profile it was produced under (the equality the step-leg
    // verifier enforces).
    let mut ctx = context();
    ctx.shape_profile_id = p.shape_profile_id();
    assert_eq!(kv_aux_leaf_count(&p, &ctx), 0);
    assert!(step_leaf_count(&p, &ctx).unwrap() > 0);
}
