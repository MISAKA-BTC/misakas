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
#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_court_v2::{
    PalwCourtV2Error, PalwCourtVerdictProofV2, adjudicate_court_close_v3, palw_refutation_leaf_cap_v2,
};
use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_shard_court_v1::{
    PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1, PalwShardCourtVerdictV1, palw_shard_court_verdict_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, canonical_step_coordinates, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1, step_merkle_path_v1, step_merkle_root_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PALW_LOGITS_TILE_LANES, PalwExecutionStepRefutationV1, PalwTiledDecodePinV1, tiled_logits_row_root_v1, tiled_logits_scheme_id_v1,
    tiled_logits_tile_leaf_v1, tiled_logits_trace_root_v1,
};

const NOW: u64 = 1_000;

struct Fx {
    binding: PalwStepBindingV2,
    rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
}

/// A binding over the REAL registered 8k class profile, tiled logits, one decode token.
fn fixture(
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
    ladder: u64,
) -> Fx {
    let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 4, 1);
    ctx.trace_scheme_id = tiled_logits_scheme_id_v1();
    ctx.job_id = h(0xB0B);
    ctx.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &[1, 2, 3, 4]).unwrap();
    let trace_root = tiled_logits_trace_root_v1(&ctx, &rows, &generated).expect("rows tile");
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
    kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).expect("binding verifies");
    Fx { binding, rows, generated }
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

    // A fused-attention leaf in call 0 (the prefill).
    let ctx = &forged.binding.job_context;
    let mut fused_leaf = None;
    for leaf in 0..forged.binding.step_leaf_count {
        let c = canonical_step_coordinates(&profile, ctx, leaf).unwrap();
        if profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused) {
            fused_leaf = Some((leaf, c));
            break;
        }
    }
    let (leaf, coord) = fused_leaf.expect("graph-v7 has a fused site in the prefill");
    assert_eq!(coord.call_index, 0);

    for (label, fx, expect) in [
        ("forged tiled token", &forged, PalwCourtVerdictV2::ExecutorGuilty),
        ("F1c garbage logits (token = their argmax)", &f1c, PalwCourtVerdictV2::ChallengerDefeated),
    ] {
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
        env.attempt.trace_root = fx.binding.full_logits_trace_root;
        env.attempt.execution_root = fx.binding.committed_execution_root;
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x71_0000_00), short, &bond_key(exe).0, 7);
        let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
        let claim_id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
        let (s2, skips) =
            go(&s1, NOW + 1, 2, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI).expect("attempt folds");
        assert!(skips.is_empty(), "{skips:?}");
        let claim = s2.claim(&claim_id).expect("claim recorded").clone();

        // The one-move accusation at the fused prefill leaf: no arithmetic is read before NeedsDissection.
        let refutation = PalwExecutionStepRefutationV1 {
            binding: fx.binding.clone(),
            output_opening: PalwStepOpeningV1 { leaf_index: leaf, leaf_hash: h(1), siblings: Vec::new() },
            output_preimage: PalwStepTileLeafV1 {
                version: 1,
                coord: PalwStepCoordinateV1 { ..coord },
                value_count: 0,
                values_le: Vec::new(),
            },
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
        let v = palw_shard_court_verdict_v1(&accusation, short, artifact_root, ladder);
        assert_eq!(v, Ok(PalwShardCourtVerdictV1::NeedsDissection));
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
