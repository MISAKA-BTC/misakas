//! **T18v (ADR-0152 v3.1 addendum §4-bis.9): the court door.** A decode-token close past
//! `palw_offence_attribution` is judged only at the head of its call and never acquits.
//!
//! Adopted from the F1 courts probe (`probe_f1_courts`), which found the two doors on testnet-12:
//! a `ShardCourtAccused` at a fused prefill leaf opens a HELD dissection that is `Terminal` at once,
//! and `adjudicate_court_close_v3` then took a `DecodeTokenTiled` close on that session — so the
//! accused could file a decode close on garbage logits whose token IS their argmax (F1c's R1) and
//! take `ChallengerDefeated`, charging the accuser (measured: 49,448,481,934 sompi). Here, over the
//! real registered 8k class and a claim the fold admitted:
//!
//! * below the fence (`decode_close_convicts_only = false`) the court is what it was — the forged
//!   token convicts, the R1 claim acquits, and folding that acquittal charges the accuser;
//! * past it both closes are refused `DecodeCloseNotAtTheHead` (the held dissection narrowed to a
//!   fused attention leaf, not the head of call 0), so no verdict exists for a block to carry and the
//!   accuser is never charged — the forged tiled token is `ForgedOutputTiled`'s (11) and the R1 claim
//!   `LogitsNotStepOutput`'s (12), through kind 4.
//!
//! **The fixture is well-formed since the A-held merge.** It first carried a fake step root and an
//! invented `state_layout_id` (`h(0)`), which the unbound one-move verdict never read. The bound
//! verdict (`palw_one_move_verdict_bound_v2`, 8be0f661) runs the formal rules first and convicts that
//! binding as `CourtFraud` before the fused leaf can defer, so no dissection opened and the door went
//! untested. The binding now commits a canonical tile at the fused leaf under a consistent step root,
//! with the family's checkpoint profile. The old binding is kept as a third row, which pins that
//! conviction.
#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_context_ladder::{
    palw_checkpoint_count_v1, palw_checkpoint_covered_at_index_v1, palw_checkpoint_positions_at_v1,
};
use kaspa_consensus_core::palw_court_v2::{
    PalwCourtV2Error, PalwCourtVerdictProofV2, adjudicate_court_close_v3, palw_refutation_leaf_cap_v2,
};
use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_shard_court_v1::{
    PALW_SHARD_COURT_VERSION_V1, PalwOneMoveClaimV2, PalwShardCourtAccusationV1, PalwShardCourtVerdictV1,
    palw_shard_court_verdict_at_v2, palw_shard_court_verdict_v1,
};
use kaspa_consensus_core::palw_state_chunk_map::{
    PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, integer_kv_checkpoint_profile_v1, palw_state_chunk_count_at_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2};
use kaspa_consensus_core::palw_state_v2::{PalwCourtVerdictV2, PalwVoidReasonV2};
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepOutLenV1, canonical_step_coordinates,
    step_leaf_count_capped_v1,
};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwCheckpointLeafV2, PalwStepBindingV2, PalwStepEvidenceV1, PalwStepFaultV1, PalwStepLegError,
    PalwStepOpeningV1, PalwStepRefutationV1, PalwStepTileLeafV1, binding_commitment_root_v1, check_step_refutation_capped_v1,
    checkpoint_empty_root_v2, checkpoint_genesis_prev_v2, checkpoint_leaf_hash_v2, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1, step_merkle_path_v1, step_merkle_root_v1, step_opening_root_v1,
    step_tile_leaf_hash_v1, verify_binding_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PALW_LOGITS_TILE_LANES, PalwExecutionStepRefutationV1, PalwTiledDecodePinV1, tiled_logits_row_root_v1, tiled_logits_scheme_id_v1,
    tiled_logits_tile_leaf_v1, tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

const NOW: u64 = 1_000;

struct Fx {
    binding: PalwStepBindingV2,
    rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
    /// The fused prefill leaf the accusation names, its canonical coordinates, the tile committed
    /// there and that tile's opening under `binding.step_merkle_root`.
    leaf: u64,
    coord: PalwStepCoordinateV1,
    tile: PalwStepTileLeafV1,
    opening: PalwStepOpeningV1,
}

/// The job every fixture runs: the 8k class, tiled logits, four prompt tokens and one decode token.
fn job_context(profile: &PalwShapeProfileV3) -> PalwJobContextV2 {
    let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 4, 1);
    ctx.trace_scheme_id = tiled_logits_scheme_id_v1();
    ctx.job_id = h(0xB0B);
    ctx.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &[1, 2, 3, 4]).unwrap();
    ctx
}

/// The first fused-attention leaf of the job, in call 0 (the prefill).
fn fused_prefill_leaf(profile: &PalwShapeProfileV3, ctx: &PalwJobContextV2, leaves: u64) -> (u64, PalwStepCoordinateV1) {
    (0..leaves)
        .find_map(|leaf| {
            let c = canonical_step_coordinates(profile, ctx, leaf).unwrap();
            profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused).then_some((leaf, c))
        })
        .expect("graph-v7 has a fused site in the prefill")
}

/// The canonical tile at `coord`: its value count is the node's output length at that tile, and
/// its values are the executor's (a fused site's recomputation is the dissection's, not the one
/// move's, so zeros are as committed as anything).
fn canonical_tile(profile: &PalwShapeProfileV3, ctx: &PalwJobContextV2, coord: PalwStepCoordinateV1) -> PalwStepTileLeafV1 {
    let (node, _) = profile.resolve_node_slot(coord.node_slot).expect("the node");
    let kv_len =
        if coord.call_index == 0 { coord.position as u64 + 1 } else { ctx.declared_prefill_tokens as u64 + coord.call_index as u64 };
    let len = match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    };
    let start = coord.tile_index as u64 * node.tile_len as u64;
    let value_count = (len - start).min(node.tile_len as u64) as u32;
    PalwStepTileLeafV1 { version: PALW_STEP_LEG_OBJECT_VERSION_V1, coord, value_count, values_le: vec![0u8; 4 * value_count as usize] }
}

/// The opening of `leaf_hash` at `leaf_index` in a tree of `leaf_count` leaves whose every other
/// subtree is a synthetic sibling — as many siblings as the promote-odd walk consumes, so the root
/// the opening implies IS the committed step root (the job's step tree is 431,048 leaves; only the
/// one the court reads is spelled out).
fn committed_opening(leaf_count: u64, leaf_index: u64, leaf_hash: Hash64) -> PalwStepOpeningV1 {
    let (mut position, mut width, mut siblings) = (leaf_index, leaf_count, Vec::new());
    while width > 1 {
        if width.is_multiple_of(2) || position != width - 1 {
            siblings.push(h(0x5EB1_0000 + siblings.len() as u64));
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    PalwStepOpeningV1 { leaf_index, leaf_hash, siblings }
}

/// A binding over the REAL registered 8k class profile, tiled logits, one decode token — WELL-FORMED
/// under the bound one-move verdict (`palw_one_move_verdict_bound_v2`, `palw_audit_2026_09_23`).
///
/// **Why the fixture changed (A-held 8be0f661):** it was written against the unbound verdict, which
/// deferred a fused leaf after checking only the class, so a fake step root (`h(0x57E9)`) and an
/// invented `state_layout_id` (`h(0)`) never mattered. The bound verdict runs the step refutation's
/// formal rules first, and those convict that binding (`CheckpointProfileNotCanonical`: `CourtFraud`,
/// no dissection — pinned below as the malformed row). So the binding now carries the family's
/// checkpoint profile (`integer_kv_state_layout_id_v1`, the pinned interval), the canonical
/// checkpoint count with a chained checkpoint leg, and a step root that commits a canonical tile at
/// the fused prefill leaf; the accusation opens that tile. What the test proves is unchanged: the
/// fused leaf goes to the held dissection, and a decode close there is refused past the fence.
fn fixture(profile: &PalwShapeProfileV3, rows: Vec<Vec<i32>>, generated: Vec<u32>, ladder: u64) -> Fx {
    let ctx = job_context(profile);
    let trace_root = tiled_logits_trace_root_v1(&ctx, &rows, &generated).expect("rows tile");
    let step_leaf_count = step_leaf_count_capped_v1(profile, &ctx, ladder).expect("leaf count");
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    // The step leg: the canonical tile at the fused leaf, committed under the step root.
    let (leaf, coord) = fused_prefill_leaf(profile, &ctx, step_leaf_count);
    let tile = canonical_tile(profile, &ctx, coord);
    let opening = committed_opening(step_leaf_count, leaf, step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &tile));
    let step_merkle_root = step_opening_root_v1(step_leaf_count, &opening).expect("the opening walks");
    // The checkpoint leg: the family's profile and the class's canonical count, chained from the
    // job's genesis (the state roots are synthetic — no court here reads a checkpoint).
    let checkpoint_profile = integer_kv_checkpoint_profile_v1(PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
    let checkpoint_count = palw_checkpoint_count_v1(profile, &ctx, checkpoint_profile.checkpoint_interval);
    let mut prev = checkpoint_genesis_prev_v2(&ctx_hash);
    let mut checkpoint_hashes = Vec::new();
    for c in 0..checkpoint_count {
        let covered = palw_checkpoint_covered_at_index_v1(profile, c, checkpoint_profile.checkpoint_interval).expect("covered");
        let positions = palw_checkpoint_positions_at_v1(profile, &ctx, covered);
        let leaf = PalwCheckpointLeafV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            checkpoint_index: c,
            covered_decode_call: covered,
            prev_checkpoint_leaf_hash: prev,
            state_chunk_count: palw_state_chunk_count_at_v1(profile, positions).expect("a layout") as u32,
            state_chunks_root: h(0x5C_0000 + c as u64),
        };
        prev = checkpoint_leaf_hash_v2(&ctx_hash, &checkpoint_profile.profile_hash(), &profile.state_chunk_map_id, &leaf);
        checkpoint_hashes.push(prev);
    }
    let checkpoint_merkle_root = if checkpoint_hashes.is_empty() {
        checkpoint_empty_root_v2(&ctx_hash)
    } else {
        step_merkle_root_v1(&checkpoint_hashes).expect("a checkpoint root")
    };
    let mut binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx,
        shape_profile: profile.clone(),
        checkpoint_profile,
        state_chunk_map_id: profile.state_chunk_map_id,
        full_logits_trace_root: trace_root,
        activation_leg_root: h(0xAC7),
        step_leaf_count,
        step_merkle_root,
        checkpoint_count,
        checkpoint_merkle_root,
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = binding_commitment_root_v1(&binding);
    verify_binding_v1(&binding).expect("binding verifies");
    // Well-formed: the structural pass at the fused leaf finds no fault (the shape rules hold and
    // the tile opens, canonical, under the committed step root).
    let structural = PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: PalwStepEvidenceV1::StepTile { opening: opening.clone(), preimage: tile.clone() },
    };
    assert_eq!(check_step_refutation_capped_v1(&structural, ladder), Err(PalwStepLegError::NoFaultFound), "a well-formed binding");
    Fx { binding, rows, generated, leaf, coord, tile, opening }
}

/// **The binding as int-3's F1c fixture (f3bd0fa8) first wrote it**: a fake step root and checkpoint
/// root, `state_layout_id = h(0)`, one checkpoint on a job that files none. It authenticates
/// (`verify_binding_v1` recomputes the root it carries) and it is malformed: the bound verdict's
/// shape pass convicts it before any leaf is read.
fn malformed_binding(profile: &PalwShapeProfileV3, rows: &[Vec<i32>], generated: &[u32], ladder: u64) -> PalwStepBindingV2 {
    let ctx = job_context(profile);
    let trace_root = tiled_logits_trace_root_v1(&ctx, rows, generated).expect("rows tile");
    let checkpoint_profile =
        PalwCheckpointProfileV1 { version: PALW_LEGS_OBJECT_VERSION_V1, checkpoint_interval: 1, state_layout_id: h(0) };
    let step_leaf_count = step_leaf_count_capped_v1(profile, &ctx, ladder).expect("leaf count");
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let step_merkle_root = h(0x57E9);
    let checkpoint_merkle_root = h(0xC4EC);
    let activation_leg_root = h(0xAC7);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, step_leaf_count, &step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &checkpoint_profile.profile_hash(),
        &profile.state_chunk_map_id,
        0,
        1,
        &checkpoint_merkle_root,
    );
    let committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &trace_root, &activation_leg_root, &checkpoint_root, &step_root);
    let binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx,
        shape_profile: profile.clone(),
        checkpoint_profile,
        state_chunk_map_id: profile.state_chunk_map_id,
        full_logits_trace_root: trace_root,
        activation_leg_root,
        step_leaf_count,
        step_merkle_root,
        checkpoint_count: 1,
        checkpoint_merkle_root,
        committed_execution_root,
    };
    verify_binding_v1(&binding).expect("the malformed binding still authenticates");
    binding
}

/// The two-tile tiled pin for position 0 (both lanes in tile 0 here).
fn pin(fx: &Fx, beat_lane: u32) -> PalwTiledDecodePinV1 {
    let ctx_hash = fx.binding.job_context.context_hash();
    let row = &fx.rows[0];
    let row_root = tiled_logits_row_root_v1(&ctx_hash, 0, row).unwrap();
    let tile_leaves: Vec<Hash64> =
        row.chunks(PALW_LOGITS_TILE_LANES).enumerate().map(|(t, c)| tiled_logits_tile_leaf_v1(&ctx_hash, 0, t as u32, c)).collect();
    assert_eq!(step_merkle_root_v1(&tile_leaves).unwrap(), row_root);
    let tile = 0usize;
    let opening = PalwStepOpeningV1 {
        leaf_index: tile as u64,
        leaf_hash: tile_leaves[tile],
        siblings: step_merkle_path_v1(&tile_leaves, tile).unwrap(),
    };
    let row_opening = PalwStepOpeningV1 { leaf_index: 0, leaf_hash: row_root, siblings: step_merkle_path_v1(&[row_root], 0).unwrap() };
    let lanes = row[..PALW_LOGITS_TILE_LANES].to_vec();
    PalwTiledDecodePinV1 {
        position: 0,
        generated_token_ids: fx.generated.clone(),
        row_root,
        row_opening,
        committed_tile_lanes: lanes.clone(),
        committed_opening: opening.clone(),
        beat_tile_lanes: lanes,
        beat_opening: opening,
        beat_lane,
    }
}

#[test]
fn t18v_a_decode_close_is_judged_at_the_head_and_never_acquits() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let court = b.court;
    let g = genesis_state(&p);
    let classes = genesis_classes(&p);
    let short = classes[1].0;
    let profile = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(8_192).unwrap();
    assert_eq!(profile.shape_profile_id(), short, "the registered 8k class IS the graph-v7@8192 profile");
    let net_ladder = palw_refutation_leaf_cap_v2(&court, p.palw_court_ladder.is_some_and(|f| f.is_active(NOW)));
    let ladder = g.class_step_ladder_v1(&short, net_ladder);
    let vocab = profile.vocab_size as usize;

    // Forged: argmax of row 0 is lane 5, the committed token is 7.
    let mut row = vec![0i32; vocab];
    row[5] = 100;
    let forged = fixture(&profile, vec![row.clone()], vec![7], ladder);
    // F1c: garbage logits, token = their argmax (lane 9) — nothing about the step tree is read.
    let mut garbage = vec![-3i32; vocab];
    garbage[9] = 77;
    let f1c = fixture(&profile, vec![garbage], vec![9], ladder);

    // A fused-attention leaf in call 0 (the prefill): the one both fixtures commit a tile at.
    let (leaf, coord) = (forged.leaf, forged.coord);
    assert_eq!((f1c.leaf, f1c.coord), (leaf, coord), "one job shape, one fused leaf");
    assert_eq!(coord.call_index, 0);
    // The binding the fixture first carried (int-3 f3bd0fa8): malformed, and convicted by name.
    let malformed = malformed_binding(&profile, &forged.rows, &forged.generated, ladder);

    enum Case<'a> {
        /// A well-formed claim accused at the fused leaf, then a decode close filed on its session.
        Close(&'a Fx, PalwCourtVerdictV2),
        /// The malformed binding accused at the fused leaf, as the fixture first filed it.
        Malformed(&'a PalwStepBindingV2),
    }
    for (label, case) in [
        ("forged tiled token", Case::Close(&forged, PalwCourtVerdictV2::ExecutorGuilty)),
        ("F1c garbage logits (token = their argmax)", Case::Close(&f1c, PalwCourtVerdictV2::ChallengerDefeated)),
        ("the malformed binding (fake step root, state_layout_id h(0))", Case::Malformed(&malformed)),
    ] {
        let binding = match &case {
            Case::Close(fx, _) => &fx.binding,
            Case::Malformed(binding) => *binding,
        };
        // Chain state: 8k class Active with ready seats, a rich executor and a rich accuser.
        let span = registry_fold(&p, NOW).unwrap().span_daa;
        let mut e = extras(&p, NOW);
        e.work_target_active = p.palw_work_target_at(NOW);
        e.shard_court_ladder = Some(net_ladder);
        e.held_context_ladder = Some(net_ladder);
        e.prompt_ids_merkle = true;
        let f = flags(&p, NOW);
        let go = |s: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
                  daa: u64,
                  blue: u64,
                  objs: &[PalwConsensusObjectV2],
                  work: PalwBlockWorkV3<'_>,
                  key: Hash64,
                  subsidy: u64| {
            kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
                s,
                &sp,
                None,
                &ctx_of(0x7C00_0000 + daa, daa, blue, subsidy),
                objs,
                work,
                &[],
                key,
                f.unavailable_abstains,
                f.capability_bound,
                f.uncertified_weightless,
                f.da_court,
                &e,
            )
            .map(|(s, _, skips)| (s, skips))
            .map_err(|e| format!("{e:?}"))
        };
        let mut c = PalwStateCarriageV2::from_state(&g);
        c.model_lifecycles.get_mut(&short).unwrap().state = PalwModelLifecycleV1::Active;
        for (bond, _, _) in genesis_bonds(&p) {
            c.seat_readiness.insert(
                (bond, short),
                PalwSeatReadinessRowV1 { proved_daa: NOW, proved_span: NOW / span.max(1), leaf_index: 0, proof_version: 2, chunks: 8 },
            );
        }
        let s0 = c.into_state(&sp, None).unwrap();
        let rich = 1_000_000_000_000_000u64;
        let (exe, acc) = (9_700u64, 9_701u64);
        let (s1, _) =
            go(&s0, NOW, 1, &[bond_obj(exe, rich), bond_obj(acc, rich)], PalwBlockWorkV3::None, Hash64::default(), 0).expect("bonds");
        let target = classes[1].2;
        let per_draw = g.palw_canonical_per_draw_v1(&short, NOW, Some(0)).unwrap();
        let pwu = palw_attempt_derived_pwu_v1(target, per_draw);
        let (mut env, _, _) = junk_attempt(short, bond_key(exe), pubkey_of(exe), &operator_pubkey_of(exe), pwu, 0x71_00, 0x71_0000_00);
        env.attempt.trace_root = binding.full_logits_trace_root;
        env.attempt.execution_root = binding.committed_execution_root;
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x71_0000_00), short, &bond_key(exe).0, 7);
        let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
        let claim_id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
        let (s2, skips) =
            go(&s1, NOW + 1, 2, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI).expect("attempt folds");
        assert!(skips.is_empty(), "{skips:?}");
        let claim = s2.claim(&claim_id).expect("claim recorded").clone();

        // The one-move accusation at the fused prefill leaf: the claim's binding, the committed tile
        // and its opening, no history — no arithmetic is read before NeedsDissection. The malformed
        // row files what the fixture first filed: a tile and a path that are nothing.
        let (output_opening, output_preimage) = match &case {
            Case::Close(fx, _) => (fx.opening.clone(), fx.tile.clone()),
            Case::Malformed(_) => (
                PalwStepOpeningV1 { leaf_index: leaf, leaf_hash: h(1), siblings: Vec::new() },
                PalwStepTileLeafV1 { version: 1, coord: PalwStepCoordinateV1 { ..coord }, value_count: 0, values_le: Vec::new() },
            ),
        };
        let refutation = PalwExecutionStepRefutationV1 {
            binding: binding.clone(),
            output_opening,
            output_preimage,
            inputs: Vec::new(),
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
            kv_checkpoint: None,
        };
        let accusation = PalwShardCourtAccusationV1 {
            version: PALW_SHARD_COURT_VERSION_V1,
            claim: claim_id,
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            executor_bond: bond_key(exe),
            accuser_bond: bond_key(acc),
            leaf_index: leaf,
            refutation,
            artifact_openings: Vec::new(),
            prompt_ids_opening: None,
            signature: Vec::new(),
        };
        let artifact_root = s2.class(&short).unwrap().artifact_root;
        let bound_to = PalwOneMoveClaimV2 { execution_root: claim.execution_root, class_id: short, artifact_root };
        assert!(e.audit_2026_09_23_active, "testnet-12 folds the bound one-move verdict");
        // The unbound verdict defers the fused leaf on every row: it reads nothing but the class.
        let v = palw_shard_court_verdict_v1(&accusation, short, artifact_root, ladder);
        assert_eq!(v, Ok(PalwShardCourtVerdictV1::NeedsDissection), "[{label}] unbound");
        let bound = palw_shard_court_verdict_at_v2(&accusation, &bound_to, ladder, true);
        if let Case::Malformed(_) = case {
            // **Formal rules first (8be0f661).** The shape pass convicts the binding the claim
            // committed (`state_layout_id` is not the family's) before the fused leaf is deferred: the
            // claim is `CourtFraud`, no dissection opens, and there is no session for a close.
            assert_eq!(bound, Ok(PalwShardCourtVerdictV1::ExecutorGuilty), "[{label}] bound");
            let shape = PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::Shape };
            assert_eq!(
                check_step_refutation_capped_v1(&shape, ladder).map(|v| v.fault),
                Ok(PalwStepFaultV1::CheckpointProfileNotCanonical),
                "[{label}] the binding alone convicts, by name"
            );
            let (s3, _) = go(
                &s2,
                NOW + 2,
                3,
                &[PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }],
                PalwBlockWorkV3::None,
                Hash64::default(),
                0,
            )
            .expect("the fold applies the conviction");
            assert!(
                matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
                "[{label}] {:?}",
                s3.claim(&claim_id).unwrap().phase
            );
            assert!(
                PalwStateCarriageV2::from_state(&s3).court_sessions.values().all(|s| s.claim != claim_id),
                "[{label}] no dissection opens"
            );
            continue;
        }
        let Case::Close(fx, expect) = case else { unreachable!() };
        assert_eq!(bound, Ok(PalwShardCourtVerdictV1::NeedsDissection), "[{label}] bound: well-formed, the fused leaf defers");
        let (s3, _) = go(
            &s2,
            NOW + 2,
            3,
            &[PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        )
        .expect("the fold opens the held dissection");
        let sessions = PalwStateCarriageV2::from_state(&s3).court_sessions;
        let (session_id, session) =
            sessions.iter().find(|(_, s)| s.claim == claim_id).map(|(k, v)| (*k, v.clone())).expect("a session");
        assert_eq!(session.ladder.terminal_index(), Some(leaf), "[{label}] the held dissection is Terminal at the fused leaf");

        // The processor's CourtClosed adjudication, on the tiled decode-token proof at position 0.
        let argmax = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&fx.rows[0]) as u32;
        let beat = if fx.generated[0] == argmax { 0 } else { argmax };
        let proof = PalwCourtVerdictProofV2::DecodeTokenTiled { binding: fx.binding.clone(), pin: pin(fx, beat) };
        // Past the fence: refused at the door — the session narrowed to a fused leaf, not the head.
        let head = profile.global_node_count() - 1;
        assert_ne!(coord.node_slot, head, "the held dissection narrowed to a fused attention leaf");
        let past = adjudicate_court_close_v3(&s3, &session_id, &proof, &court, net_ladder, PalwPromptIdsFormV1::MerkleV1, true, true);
        assert_eq!(
            past,
            Err(PalwCourtV2Error::DecodeCloseNotAtTheHead { narrowed: leaf, slot: coord.node_slot }),
            "[{label}] past the fence"
        );
        // Below it: the court as it was.
        let derived =
            adjudicate_court_close_v3(&s3, &session_id, &proof, &court, net_ladder, PalwPromptIdsFormV1::MerkleV1, true, false);
        assert_eq!(derived, Ok(expect), "[{label}] below the fence");
        if expect == PalwCourtVerdictV2::ExecutorGuilty {
            let (s4, _) = go(
                &s3,
                NOW + 3,
                4,
                &[PalwConsensusObjectV2::CourtClosed { session_id, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof }],
                PalwBlockWorkV3::None,
                Hash64::default(),
                0,
            )
            .expect("the fold applies the close");
            let after = s4.claim(&claim_id).unwrap();
            assert!(matches!(after.phase, PalwClaimPhaseV2::Voided { .. }));
        } else {
            // Below the fence: the SAME door, filed by anyone (CourtClosed carries no signature) — the
            // self-acquittal the executor could file against ANY held dissection, charging the accuser.
            // Past it no verdict exists for the processor to accept, so no block carries this.
            let before = s3.bond(&bond_key(acc)).unwrap().collateral;
            let (s4, _) = go(
                &s3,
                NOW + 3,
                4,
                &[PalwConsensusObjectV2::CourtClosed { session_id, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof }],
                PalwBlockWorkV3::None,
                Hash64::default(),
                0,
            )
            .expect("the fold applies the close");
            let after = s4.bond(&bond_key(acc)).unwrap().collateral;
            assert!(after < before, "below the fence the acquittal charges the accuser of the held dissection ({before} -> {after})");
        }
    }
}

fn ctx_of(block: u64, daa: u64, blue: u64, subsidy: u64) -> kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
    common::ctx(block, daa, blue, subsidy)
}
