//! **ADR-0152 v3.1 Phase 2, P2-8e — T54g on the 8k fixtures, through the node and the fold.**
//!
//! The held graph-v7 row at context 8,192 (`held_fixture(8_192)`: the answerable held class, the
//! genesis 8k row's rules at the fixture's small geometry). The producer's free-prompt run lies in
//! ONE committed fused-attention tile (`PalwFreePromptDrillFaultV1::Leaf`: the tile's first lane
//! moved, every other leaf the honest execution's) and retains the FOLD a held class serves, which
//! every seat verifies against the claim's roots — the attempt lane's dense drill is refused by every
//! seat on a held class (its honest attempt folds), so it could never reach a replay filer. The claim
//! rides the fixture fold's attempt lane (the fold reads its roots); the seat is served it as a
//! free-prompt payload (`FPC1`), so the node's free-prompt lane replays it from the carried ids.
//!
//! **What is a stand-in, and what is not (the review's HIGH).** A FRESH seat instance can neither
//! bisect a lying fold nor build its opening: the bisection's prefix state, the one move's evidence at
//! the fused leaf and N1 all read the served fold below its retained level, which base0 does only by
//! an honest re-execution the lie's roots refuse — and the fold keeps no tile for any other reading.
//! So everything that reads the SERVED capture here goes through the liar's own instance
//! ([`ServedView`]: the drill's rule, the instance that ran a lie re-derives it — what a retention
//! readable at the lie would serve): the bisection's served rungs, the leaf's evidence and N1. What
//! is the seat's own — its replay of the job, its N2, the artifact rows — runs on its honest instance;
//! the book, the loop half, the fold and the held route are the real ones. The gaps are pinned as
//! they stand today: `t54g_gap_a_a_canonical_8k_attempt_is_past_the_whole_capture_cap`,
//! `t54g_gap_b_a_fresh_seat_cannot_bisect_a_lying_held_fold` and
//! `t54g_gap_c_a_fresh_seat_cannot_build_the_opening_of_a_lying_held_fold`. **P2-8e does not open
//! a held dissection on testnet-12's 8k row today.**
//!
//! * **T54g** (on the stand-in) — the bisection lands on the fused leaf; the node opens the held
//!   dissection there with the right evidence (the leaf, the claim's roots and binding, both bonds,
//!   the bound verdict's `NeedsDissection`), due now (a one-move accusation, as the A-held node dates the
//!   capture arm's) — before the fold's `Final` less the landing margin; the fold
//!   opens the session, the producer's root claim due at the move turn off the session; the liar's
//!   least lie and the seat's held route play it to a bottom the seat files, and the claim voids
//!   `CourtHeldVerdict` (reason 8, F3 (B)) — the producer charged, the seat not.
//! * **Liveness** — an honest 8k claim yields no dissection (the seat's replay reproduces it), and a
//!   seat whose replay parts from an honest claim AT a fused leaf opens nothing: the court's kernels
//!   reproduce the committed tile from the accused's own filing.
//! * **Missing material** — a served capture whose leaf the seat cannot open falls back to P2-8d's
//!   `StepLeaf` demand of that leaf (the case's one step: never both), which the builder's stateless
//!   half refuses for a fused leaf (DA-3) before anything is signed; an opening the fold's verdict
//!   refuses is not "missing" and settles.
//! * **Dated** — due now, before the fold's `Final` (on testnet-12's windows too), as the A-held node
//!   dates a one-move accusation.
//! * **Dedup and restart** — one queue entry per accusation whoever built it; in the tick's own order
//!   the chain's session is read before a trigger is noted, a waiting case ends before its run, the
//!   run in flight files nothing on return, and a restart seeds the openings still in the pool.
//! * **Fence-off twin** — with no held route (below `palw_offence_attribution`) the same run builds
//!   no opening and the case settles with nothing queued; below `palw_rcore_plus` the filer is dormant.
//! * **The acceptance layer's gates** on testnet-12's court: signature, domain, ladder, bytes, verdict.
use super::*;
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
use crate::palw_panel::held_court::{
    PalwHeldCourtV1, PalwHeldHostV1, PalwHeldMaterialV1, PalwHeldMoveV1, palw_held_move_of_duty_v1, palw_held_moves_v1,
    palw_held_start_builds_v1,
};
use crate::palw_panel::held_court_e2e::held_fixture;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_attn_court_v1::palw_attn_opened_lanes_v1;
use kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_producer_v2::{PalwCourtDutyV2, palw_court_duties_v2, palw_seat_duties_v2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_replay_refute_v1::{PalwReplayFindingV1, PalwReplayNothingV1, palw_replay_bisect_rungs_v1};
use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClassAdmissionCarriageV2, PalwCourtVerdictV2, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateCarriageV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwVoidReasonV2, apply_delta_v2,
    apply_palw_transition_v2_with_extras, palw_operator_id_v2, palw_v2_apply_one_object_v1, revert_delta_v2,
};
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_base0::artifact::Base0ArtifactV1;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use std::sync::Arc;

const NETWORK: &[u8] = b"misaka-palw-rc";
const PRODUCER: u64 = 1;
const SEAT: u64 = 2;
const COLLUDER: u64 = 4;
/// The fixture's network ladder: its job builds densely too, so the drill can lie in a retained tile.
const LADDER: u64 = 1 << 22;
/// The class's canonical job: 64 prompt positions, 8 decode calls.
const CANONICAL: (u32, u32) = (64, 8);
/// The court's turn this fold runs.
const TURN: u64 = 20;
/// The held row this file plays: the answerable context, 8,192 (C5's bound; the 8k row).
const N_CTX_8K: u32 = 8_192;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

fn backend(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), CANONICAL)
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(LADDER)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
}

/// A producer's attempt: the anchor's job, its run (honest, or with the committed tile at the fused
/// site's leaf moved — a lie only a held dissection reaches), and that leaf.
struct Produced {
    /// The instance that ran the job — the drill's rule: it re-derives its own lie.
    backend: Qwen25A16Backend,
    fp_job: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    ids: Vec<u32>,
    ctx: PalwJobContextV2,
    material: Vec<u8>,
    execution_root: Hash64,
    trace_root: Hash64,
    leaf: u64,
    /// The committed step leaves, re-derived densely under the same lie and proven to root to the
    /// fold's committed step root — what a retention the seat can read at the lie serves.
    committed_leaves: Vec<Hash64>,
}

impl Produced {
    /// The roots every seat arm checks the served capture against — the claim's, and the job the
    /// payload names (`fp_job_id_v3`); the fixture's claim commits a placeholder output root, so none
    /// is read here.
    fn roots(&self) -> PalwClaimRootsV1 {
        PalwClaimRootsV1 {
            execution_root: self.execution_root,
            trace_root: self.trace_root,
            anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&self.fp_job),
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        }
    }

    /// What the producer serves a seat: the `FPC1` payload — its job, the ids, the fold.
    fn served(&self) -> Vec<u8> {
        kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(&self.fp_job, &self.ids, &self.material)
    }
}

/// The prefill position the drill lies at, and the prompt's length.
const LIE_POSITION: u32 = 40;
const PROMPT_TOKENS: u32 = 48;

/// The fused site's leaf at layer 0, head `head` (one head a tile), of call 0 at `position`.
fn fused_leaf_at(profile: &PalwShapeProfileV3, job: &PalwJobContextV2, position: u32, head: u32) -> u64 {
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("the slot");
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position, tile_index: head };
    canonical_step_leaf_index(profile, job, &coord).expect("the site's leaf")
}

/// **The producer's free-prompt run** of the class — honest, or lying in ONE committed fused tile of
/// layer 0 at or past `LIE_POSITION` (the shipped drill, `PalwFreePromptDrillFaultV1::Leaf`: the
/// tile's first lane moved by one, the execution honest), the fold retained. The tile is the first
/// whose first lane the move keeps inside the A16 range — a lie no root claim can finalize to would
/// leave the liar only silence (C1's default), and T54g plays the liar that answers.
fn produce(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, lie: bool) -> Produced {
    use kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
    };
    let backend = backend(artifact, profile);
    let ids: Vec<u32> = (0..PROMPT_TOKENS).map(|i| (i * 37 + 11) % 128).collect();
    let fp_job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: h64(999),
        class_id: profile.shape_profile_id(),
        executor_bond: bond_key(PRODUCER).0,
        executor_pubkey: vec![7; 4],
        operator_id: palw_operator_id_v2(&op_key(20 + PRODUCER)),
        anchor_block: h64(0xA0),
        anchor_daa: 100,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            backend.prompt_ids_form(),
            &ids,
        )
        .expect("commits"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: 2,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    };
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    // The job's context, off an honest run (the drill's leaf is named in it).
    let honest = backend.execute_free_prompt(&fp_job, &prompt).expect("the honest run");
    let binding_of =
        |material: &[u8]| misaka_palw_base0::produce::base0_material_decode_any_v1(material).expect("decodes").binding().clone();
    let ctx = binding_of(&honest.outcome.material).job_context;
    let engine = misaka_palw_base0::engine_a16::A16Engine::new(artifact).expect("an A16 class");
    let plan = engine.plan_from_profile(profile).expect("the class's plan");
    let dense_of = |drill: Option<u64>| {
        misaka_palw_base0::qwen25_a16_backend::a16_execute_in_storage_v1(
            artifact,
            profile,
            Some(&plan),
            &ctx,
            &prompt,
            LADDER,
            misaka_palw_base0::legs::Base0CaptureKindV1::DenseTiles,
            &mut |_| {},
            drill,
            misaka_palw_base0::engine_a16::KV_STORAGE_SHIPPED_V1,
        )
        .expect("the dense re-derivation")
    };
    let honest_dense = dense_of(None);
    let first_lane = |leaf: u64| {
        let (_, tile) = honest_dense.tiles.tiles.iter().find(|(i, _)| *i == leaf).expect("the tile");
        (tile.values_le[0], i32::from_le_bytes(tile.values_le[0..4].try_into().expect("a lane")))
    };
    let leaf = (LIE_POSITION..PROMPT_TOKENS)
        .flat_map(|position| (0..profile.attn_heads).map(move |head| (position, head)))
        .map(|(position, head)| fused_leaf_at(profile, &ctx, position, u32::from(head)))
        .find(|leaf| {
            let (low, lane) = first_lane(*leaf);
            low < u8::MAX && (-32_000..32_000).contains(&lane)
        })
        .expect("a fused tile the move keeps in range");
    let run = if lie {
        backend
            .execute_free_prompt_with_drill_fault_v2(&fp_job, &prompt, PalwFreePromptDrillFaultV1::Leaf { leaf })
            .expect("the liar's run commits")
    } else {
        honest
    };
    let dense = if lie { dense_of(Some(leaf)) } else { honest_dense };
    let committed = binding_of(&run.outcome.material);
    assert_eq!(
        kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&dense.tiles.leaves, LADDER).expect("roots"),
        committed.step_merkle_root,
        "the re-derived leaves are the fold's committed tree"
    );
    Produced {
        backend,
        fp_job,
        ids,
        ctx,
        material: run.outcome.material,
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        leaf,
        committed_leaves: dense.tiles.leaves,
    }
}

/// The class's canonical job — an attempt's, as the class is registered (the free-prompt claim of
/// another shape is still the class's claim).
fn canonical(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> PalwJobContextV2 {
    let backend = backend(artifact, profile);
    let (job, prompt) = backend.job_for_anchor(h64(0x0A11_E1D0)).expect("the anchor's job");
    let run = backend.execute(&job, &prompt).expect("the attempt runs");
    misaka_palw_base0::produce::base0_material_decode_any_v1(&run.material).expect("decodes").binding().job_context.clone()
}

// ---- the fold ------------------------------------------------------------------------------------

thread_local! {
    /// **testnet-12's challenge windows** (the review's MED-2): `window_challenge` 1,200 with the
    /// short window (120) in force from genesis — the claim rows read the first, the fold's `Final`
    /// the second. Off: the fixture's own 20.
    static T12_WINDOWS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn params() -> PalwStateParamsV2 {
    let t12 = T12_WINDOWS.with(|on| on.get());
    PalwStateParamsV2::new(100, 10, 10, if t12 { 1_200 } else { 20 }, 600, 1000, h64(1), 4, 1000, 10_000, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_turn_deadline_daa(TURN)
        .unwrap()
        .with_worker_carve_permille(300)
        .unwrap()
        .with_short_challenge_window_from_daa(t12.then_some(0))
}

/// testnet-12's launch line as far as this fold reads it — `palw_offence_attribution` in force.
fn launch() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        shard_court_ladder: Some(1 << 26),
        held_context_ladder: Some(1 << 26),
        audit_2026_09_23_active: true,
        audit_2026_09_11_deep_active: true,
        attn_anchored_root_active: true,
        court_responder_coverage_active: true,
        offence_attribution_active: true,
        ..Default::default()
    }
}

fn point(daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Hash64::from_u64_word(0x4E1D_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 10_000 }
}

/// One block through the real transition, checked: consistency, the delta re-applies and reverts, the
/// carriage reloads under its root.
fn step_with(
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = params();
    let (child, delta) =
        apply_palw_transition_v2_with_extras(parent, &p, &point(daa), objects, att, false, false, false, true, &launch())?;
    child.assert_internal_consistency(&p).expect("internal consistency");
    child.assert_deadline_consistency(&p).expect("deadline consistency");
    assert_eq!(apply_delta_v2(parent, &delta, &p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &p).expect("reverts"), *parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&p, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    Ok(child)
}

fn step(parent: &PalwChainStateV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    step_with(parent, daa, objects, None)
}

/// The produced claim on the chain with its panel bound: the registry at 100, the attempt at 101, the
/// panel (SEAT and COLLUDER) at 102 — where the seat's duty stands and its replay runs.
fn bound(
    d: &Produced,
    canonical: &PalwJobContextV2,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
) -> (PalwChainStateV2, Hash64) {
    let class_id = profile.shape_profile_id();
    let bond = |n: u64| PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![n as u8; 4],
        operator_pubkey: op_key(20 + n),
        collateral: 1_000_000_000,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    let register = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        bond(PRODUCER),
        bond(SEAT),
        bond(COLLUDER),
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 100,
            activation_daa: 0,
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                profile: profile.clone(),
                canonical: canonical.clone(),
                registrant_bond: bond_key(PRODUCER),
                signature: Vec::new(),
            })),
        },
    ];
    let s = step(&PalwChainStateV2::genesis(), 100, &register).expect("the registry");
    assert!(s.class_is_held_v1(&class_id), "the held class records its ladder");
    let network_domain = h64(999);
    let executor_bond = bond_key(PRODUCER).0;
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, 1, h64(1), &executor_bond),
            class_id,
            executor_bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&op_key(20 + PRODUCER)),
            artifact_root,
            trace_root: d.trace_root,
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: d.execution_root,
        },
        signature: vec![0; 8],
    };
    let claim = attempt_id_v2(&env.attempt);
    let s = step_with(&s, 101, &[], Some(&env)).expect("the claim");
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let s = step(&s, 102, &[PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats }]).expect("the panel");
    (s, claim)
}

/// The claim licensed at 103 by its panel's receipts (the fixture's licence door; the seat's own
/// replay is what refutes the claim, whatever the licence says).
fn licence(s: &PalwChainStateV2, claim: Hash64) -> PalwChainStateV2 {
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    step(s, 103, &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: vec![valid(SEAT), valid(COLLUDER)] }])
        .expect("the licence")
}

fn phase_of(s: &PalwChainStateV2, claim: &Hash64) -> PalwClaimPhaseV2 {
    s.claim(claim).expect("the claim").phase.clone()
}

fn collateral(s: &PalwChainStateV2, n: u64) -> u64 {
    s.bond(&bond_key(n)).expect("the bond").collateral
}

fn duty_of(s: &PalwChainStateV2, bond: u64, sid: Hash64) -> Option<PalwCourtDutyV2> {
    palw_court_duties_v2(s, &[bond_key(bond)]).into_iter().find(|duty| duty.session_id == sid)
}

fn adjudicate(state: &PalwChainStateV2, sid: Hash64, proof: &PalwCourtVerdictProofV2) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 20, 2).unwrap();
    kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
        state,
        &sid,
        proof,
        &court,
        1 << 26,
        PalwPromptIdsFormV1::MerkleV1,
        true,
        true,
    )
}

// ---- the seat's node: the replay filer's run, its book, P2-8e's loop half -------------------------

/// What a run builds a held opening under on this fixture (the held ladder for both the site's rows
/// and the one move; the bound verdict in force).
fn opening_ctx(artifact_root: Hash64) -> PalwHeldOpeningCtxV1 {
    PalwHeldOpeningCtxV1 { artifact_root, opening_cap: PALW_HELD_STEP_LADDER_V1, ladder: PALW_HELD_STEP_LADDER_V1, bound: true }
}

/// **The seat's replay filer run, off the loop** — the node's own run body on the served `capture`,
/// the attempt lane's job the anchor's, the claim as the state resolves it.
fn seat_run(
    seat: &dyn PalwExecutionBackendV1,
    d: &Produced,
    s: &PalwChainStateV2,
    claim: Hash64,
    payload: Vec<u8>,
    held: Option<PalwHeldOpeningCtxV1>,
) -> PalwReplayRunV1 {
    let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(s, &claim).expect("the claim is in state");
    let input = PalwReplayRunInputV1 {
        retained: None,
        payloads: vec![payload],
        // The free-prompt lane: each payload's own job (`fp_job_id_v3`) is the anchor it is read under.
        lane: PalwReplayLaneV1::FreePrompt {
            class_id: target.class_id,
            executor: bond_key(PRODUCER),
            roots: PalwClaimRootsV1 { anchor: Hash64::default(), ..d.roots() },
        },
        target,
        ladder: PALW_HELD_STEP_LADDER_V1,
        // The class's own form (the held row commits its prompt tiled).
        form: d.backend.prompt_ids_form(),
        held,
    };
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None);
    palw_replay_filer_run_v1(
        seat,
        input,
        |capture| Ok::<u64, String>(capture.len() as u64),
        |_| Ok::<(), String>(()),
        || Ok::<(), String>(()),
        &ledger,
    )
}

/// A reporter filer's door that holds nothing (the replay filer asks it whether a kind 4 is filed).
fn nodoor() -> crate::palw_panel::reporter_filer::PalwBookDoorV1 {
    crate::palw_panel::reporter_filer::PalwBookDoorV1::new(bond_key(SEAT), 0)
}

/// The seat's duty on the bound claim (what its SEAT-R replay refuted), noted in a fresh book.
fn noted(s: &PalwChainStateV2, claim: Hash64) -> (PalwReplayFilerV1, PalwSeatDutyV2) {
    let duty =
        palw_seat_duties_v2(s, &params(), &[bond_key(SEAT)]).into_iter().find(|d| d.claim_id == claim).expect("the seat's duty");
    let mut filer = PalwReplayFilerV1::default();
    assert!(filer.note_v1(&duty, PalwReplayMismatchSiteV1::Replay, 102, &bond_key(SEAT), &nodoor()), "noted");
    (filer, duty)
}

/// A lifecycle carrier of `object`, as the mempool holds it.
fn lifecycle_carrier_of(object: PalwConsensusObjectV2) -> kaspa_consensus_core::tx::Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
        .expect("a lifecycle payload serializes");
    kaspa_consensus_core::tx::Transaction::new(
        0,
        vec![],
        vec![kaspa_consensus_core::tx::TransactionOutput::new(1, kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    )
}

/// **The node's P2-8e host over the fixture fold**: the tip `state` at `daa` (the next block's).
struct FilerHost {
    state: PalwChainStateV2,
    daa: u64,
}

impl PalwHeldFilerHostV1 for FilerHost {
    fn network_domain(&self) -> Hash64 {
        h64(999)
    }
    fn accusation_ladder(&self, _class_id: Hash64, _daa: u64) -> u64 {
        PALW_HELD_STEP_LADDER_V1
    }
    fn sign(&self, _message: &[u8], _context: &[u8]) -> Option<Vec<u8>> {
        Some(vec![0x5E; 8])
    }
    /// The fold's own arm on the tip — `palw_v2_apply_one_object_v1`, with this fold's flags.
    fn rehearse(&self, object: &PalwConsensusObjectV2) -> Option<PalwObjectRehearsalV1> {
        Some(
            match palw_v2_apply_one_object_v1(&self.state, &params(), &point(self.daa), object, false, false, false, true, &launch()) {
                Ok(_) => PalwObjectRehearsalV1::Accepted,
                Err(why) => PalwObjectRehearsalV1::Refused(why),
            },
        )
    }
}

/// **What the panel's claim rows say the claim's phase ends at** (`palw_claim_rows_v1`, the RPC's
/// read) — the date P2-8e used to be due by, which read `window_challenge` where the fold reads
/// `window_challenge_at` (the review's MED-2) until F3 of the pre-t12 drill made it the fold's floor.
fn row_deadline(state: &PalwChainStateV2, claim: &Hash64) -> Option<u64> {
    let (rows, _) = kaspa_consensus_core::palw_producer_v2::palw_claim_rows_v1(
        state,
        &params(),
        &bond_key(SEAT),
        kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat,
        false,
        4_096,
    );
    rows.into_iter().find(|row| row.claim_id == *claim).and_then(|row| row.deadline_daa)
}

/// The court queue and its schedule, as the panel keeps them.
#[derive(Default)]
struct Queue {
    pending: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    due: HashMap<(Hash64, u32, bool), u64>,
    moved: HashMap<(Hash64, u32, bool), u64>,
}

impl Queue {
    /// **The replay filer's tick at `daa` on the tip `state`, in `replay_filer_tick_v1`'s own order**
    /// (the review's MED-4): the chain's held dissections read first (`observe_held_v1`), then this
    /// tick's triggers noted (`notes`), then the run in flight polled (`run`), then P2-8e's openings
    /// (`palw_held_dissections_step_v1`). Returns the openings queued.
    fn tick(
        &mut self,
        filer: &mut PalwReplayFilerV1,
        state: &PalwChainStateV2,
        daa: u64,
        notes: &[PalwSeatDutyV2],
        run: Option<(Hash64, PalwReplayRunV1)>,
    ) -> usize {
        let duties = palw_court_duties_v2(state, &[bond_key(SEAT)]);
        let _ = filer.observe_held_v1(&duties, &bond_key(SEAT), daa, &mut self.pending);
        for duty in notes {
            let _ = filer.note_v1(duty, PalwReplayMismatchSiteV1::Replay, daa, &bond_key(SEAT), &nodoor());
        }
        if let Some((claim, run)) = run {
            // The task returned: the tick takes its handle, then files what it found.
            filer.running = None;
            let _ = filer.on_run_v1(claim, run, daa, &bond_key(SEAT));
        }
        let host = FilerHost { state: state.clone(), daa };
        palw_held_dissections_step_v1(&host, filer, daa, bond_key(SEAT), &mut self.pending, &mut self.due, &self.moved)
    }

    /// A tick with no trigger and no run returning.
    fn held_step(&mut self, filer: &mut PalwReplayFilerV1, state: &PalwChainStateV2, daa: u64) -> usize {
        self.tick(filer, state, daa, &[], None)
    }

    /// The priority lane's one carrier: the soonest due first (`palw_court_queue_edf_v1`).
    fn carry(&mut self, daa: u64) -> Option<PalwConsensusObjectV2> {
        if self.pending.is_empty() {
            return None;
        }
        palw_court_queue_edf_v1(&mut self.pending, &self.due);
        let (a, b, c, object) = self.pending.remove(0);
        self.due.remove(&(a, b, c));
        self.moved.insert((a, b, c), daa);
        Some(object)
    }
}

// ---- the seat's held route (the A-held line's), over the fixture fold -----------------------------

/// What the seat node's held route asks of its host.
struct HeldHost {
    state: PalwChainStateV2,
    make: Box<dyn Fn() -> Qwen25A16Backend + Send + Sync>,
    ledger: Arc<PalwMemoryLedgerV1>,
    /// The claim's prompt, as the node holds it from the served payload.
    carried: Vec<u32>,
}

impl PalwHeldHostV1 for HeldHost {
    fn offence_attribution_active(&self, _daa: u64) -> bool {
        true
    }
    fn class_is_held(&self, class_id: &Hash64) -> Option<bool> {
        self.state.class(class_id).map(|_| self.state.class_is_held_v1(class_id))
    }
    fn opening_cap(&self, _class_id: &Hash64, _daa: u64) -> u64 {
        PALW_HELD_STEP_LADDER_V1
    }
    fn arity(&self, _daa: u64) -> Option<u8> {
        Some(2)
    }
    fn disclose_window_daa(&self) -> u64 {
        kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&params())
    }
    fn window_court(&self) -> u64 {
        params().window_court()
    }
    fn network_domain(&self) -> Hash64 {
        h64(999)
    }
    fn bond(&self) -> PalwBondKeyV2 {
        bond_key(SEAT)
    }
    fn verdict_of(&self, session_id: &Hash64, proof: &PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2> {
        adjudicate(&self.state, *session_id, proof).ok()
    }
    fn sign(&self, _message: &[u8], _context: &[u8]) -> Option<Vec<u8>> {
        Some(vec![0xAA; 8])
    }
    fn pin(&self, _claim: Hash64) {}
    fn ledger(&self) -> Arc<PalwMemoryLedgerV1> {
        self.ledger.clone()
    }
    fn backend(&self, _duty: &PalwCourtDutyV2) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        Ok(Box::new((self.make)()))
    }
    fn build_need_bytes(&self, _backend: &dyn PalwExecutionBackendV1, _duty: &PalwCourtDutyV2) -> u64 {
        1_000
    }
    fn responder_material(
        &self,
        _duty: &PalwCourtDutyV2,
        _backend: &dyn PalwExecutionBackendV1,
    ) -> Result<PalwHeldMaterialV1, String> {
        Err("the seat is no responder here".into())
    }
    fn carried_prompt(&self, _duty: &PalwCourtDutyV2, _backend: &dyn PalwExecutionBackendV1) -> Result<Option<Vec<u32>>, String> {
        Ok(Some(self.carried.clone()))
    }
    fn claim_open_until_v1(&self, claim_id: &Hash64) -> Option<u64> {
        use kaspa_consensus_core::palw_producer_v2::{PalwClaimRoleV1, palw_claim_rows_v1, palw_disputable_claims_v2};
        let (rows, _) = palw_claim_rows_v1(&self.state, &params(), &bond_key(SEAT), PalwClaimRoleV1::Seat, false, 4_096);
        let disputable = palw_disputable_claims_v2(&self.state, &[bond_key(SEAT)]);
        crate::palw_panel::held_court::palw_held_open_claims_v1(&rows, &disputable, &HashSet::from([*claim_id])).get(claim_id).copied()
    }
}

/// **The seat node's held route for one tick** — its court duties off the fold, routed, the chain
/// reads answered from the objects mined so far, its moves, its builds (settled: a fixture build is
/// milliseconds) — and the priority lane's one carrier, soonest due first.
async fn held_tick(
    host: &mut HeldHost,
    held: &mut PalwHeldCourtV1,
    queue: &mut Queue,
    state: &PalwChainStateV2,
    chain: &[PalwConsensusObjectV2],
    daa: u64,
) -> Option<PalwConsensusObjectV2> {
    host.state = state.clone();
    let host: &HeldHost = host;
    let duties = palw_court_duties_v2(state, &[bond_key(SEAT)]);
    held.begin_tick_v1(&duties, daa).await;
    let held_duties: Vec<PalwCourtDutyV2> = duties.into_iter().filter(|d| held.routes_v1(host, d, daa)).collect();
    for read in held.chain_reads_v1(host, &held_duties, daa) {
        let objects = chain
            .iter()
            .filter(|o| crate::palw_panel::palw_held_chain_object_is_the_sessions_v1(o, &read.session_id, &read.claim_id))
            .cloned()
            .collect();
        held.note_chain_v1(read.session_id, objects, daa);
    }
    let (pending, moved) = (&queue.pending, &queue.moved);
    let busy = |key: &(Hash64, u32, bool)| {
        pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == *key)
            || moved.get(key).is_some_and(|at| daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
    };
    let tick = palw_held_moves_v1(host, held, &held_duties, daa, busy);
    for queued in tick.queued {
        queue.due.insert(queued.key, queued.due);
        queue.pending.push((queued.key.0, queued.key.1, queued.key.2, queued.object));
    }
    palw_held_start_builds_v1(host, held, daa);
    held.settle_v1().await;
    queue.carry(daa)
}

/// **The liar's play**: its held root claim — the least lie in one lane of `V*` that finalizes to the
/// tile it committed — and the `(lane, delta)` it pushes into tile 0's child every round.
fn least_lie_root(accused: &PalwAttnHeldEvidenceV1, root: Hash64) -> (PalwAttnRootClaimV1, usize, i64) {
    let site = accused.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let committed = palw_attn_opened_lanes_v1(&accused.evidence.out_tile, &site.binding, site.head_lanes.2 as usize).expect("tile");
    let honest_root = accused.evidence.root_claim_v1(&site).expect("root");
    let values = site.site.params.values;
    let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
    let lane = (0..committed.len()).find(|l| finalize(&honest_root.claim.v_acc)[*l] != committed[*l]).expect("the tile is a lie");
    let at = |delta: i64| {
        let mut v = honest_root.claim.v_acc.clone();
        v[lane] += delta;
        finalize(&v)[lane]
    };
    let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if at(mid) < committed[lane] { lo = mid + 1 } else { hi = mid }
    }
    let mut lying_root = honest_root.clone();
    lying_root.claim.v_acc[lane] += lo;
    assert_eq!(finalize(&lying_root.claim.v_acc), committed, "the least lie finalizes to the committed tile");
    (lying_root, lane, lo)
}

/// The fixture as T54g's cases start it: the 8k row, its inventory root, the class's canonical job
/// and the producer's run.
struct Fixture {
    d: Produced,
    profile: PalwShapeProfileV3,
    artifact: Arc<Base0ArtifactV1>,
    root: Hash64,
    canonical: PalwJobContextV2,
}

impl Fixture {
    fn new(lie: bool) -> Self {
        let (artifact, profile) = held_fixture(N_CTX_8K);
        let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
        let canonical = canonical(&artifact, &profile);
        Self { d: produce(&artifact, &profile, lie), profile, artifact, root, canonical }
    }

    fn seat(&self) -> Qwen25A16Backend {
        backend(&self.artifact, &self.profile)
    }

    /// The seat reading this producer's served capture whole ([`ServedView`]).
    fn view<'a>(&'a self, seat: &'a Qwen25A16Backend, withhold: bool) -> ServedView<'a> {
        ServedView {
            liar: &self.d.backend,
            own: seat,
            served: &self.d.material,
            ctx: &self.d.ctx,
            committed_leaves: &self.d.committed_leaves,
            withhold,
            probe: None,
        }
    }

    fn bound(&self) -> (PalwChainStateV2, Hash64) {
        bound(&self.d, &self.canonical, &self.profile, self.root)
    }

    /// The seat's run on the served payload, with (`held`) or without the held route.
    fn run(&self, seat: &dyn PalwExecutionBackendV1, s: &PalwChainStateV2, claim: Hash64, held: bool) -> PalwReplayRunV1 {
        seat_run(seat, &self.d, s, claim, self.d.served(), held.then(|| opening_ctx(self.root)))
    }

    /// The accusation this seat's node files at the lie's leaf (the one move's evidence off the fold).
    fn opening(&self, s: &PalwChainStateV2, claim: Hash64, duty: &PalwSeatDutyV2) -> ((Hash64, u32, bool), PalwConsensusObjectV2) {
        let PalwReplayRunV1::FusedLeaf { opening: PalwHeldOpeningV1::Open { evidence, .. }, .. } =
            self.run(&self.view(&self.seat(), false), s, claim, true)
        else {
            panic!("an opening")
        };
        palw_held_opening_object_v1(&evidence, duty, bond_key(SEAT), &h64(999), PALW_HELD_STEP_LADDER_V1, |_, _| Some(vec![0x5E; 8]))
            .expect("built")
    }
}

/// **The node opens the held dissection** — the run, the book, the loop half at 104 on the licensed
/// state, the lane's carrier folded at 104. Returns the state with the session open, its id, the claim
/// and the filer.
fn opened_by_the_node(f: &Fixture) -> (PalwChainStateV2, Hash64, Hash64, PalwReplayFilerV1) {
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let seat = f.seat();
    let run = f.run(&f.view(&seat, false), &s102, claim, true);
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    let s103 = licence(&s102, claim);
    assert_eq!(queue.held_step(&mut filer, &s103, 104), 1, "the opening is queued");
    let carried = queue.carry(104).expect("the lane carries it");
    let s104 = step(&s103, 104, &[carried]).expect("the fold opens the held dissection");
    let sid = s104
        .court_sessions_iter()
        .find(|(_, x)| x.claim == claim && x.challenger_bond == bond_key(SEAT))
        .map(|(k, _)| *k)
        .expect("the session this seat opened");
    (s104, sid, claim, filer)
}

/// **T54g: a garbage fused-attention leaf on the 8k row, found by the replaying seat, opened by its
/// node as a held dissection and played to the producer's conviction (`CourtHeldVerdict`, reason 8).**
#[tokio::test(flavor = "multi_thread")]
async fn t54g_a_garbage_fused_leaf_is_dissected_by_the_seats_node_and_its_producer_convicted() {
    let f = Fixture::new(true);
    let liar = &f.d;
    let (s102, claim) = f.bound();
    let (mut filer, duty) = noted(&s102, claim);
    let seat = f.seat();
    // 1. The seat's run: its replay parts from the served fold, and the bisection lands on the fused leaf.
    assert_eq!(seat.verify_material(&liar.material, liar.roots()), PalwMaterialVerdictV1::Matches, "every seat verifies the fold");
    let n = seat.capture_shape(&liar.material).expect("a capture").step_leaf_count;
    let run = f.run(&f.view(&seat, false), &s102, claim, true);
    let PalwReplayRunV1::FusedLeaf { leaf, binding, rungs, opening } = &run else { panic!("a fused leaf: {run:?}") };
    assert_eq!(*leaf, liar.leaf, "the first divergent step is the lie's own fused leaf");
    assert!(kaspa_consensus_core::palw_da_rcore_v1::palw_da_step_leaf_is_fused_v1(binding, *leaf), "DA-3's predicate");
    assert!(*rungs >= 2 && *rungs <= palw_replay_bisect_rungs_v1(n), "{rungs} rungs over {n} leaves");
    let PalwHeldOpeningV1::Open { evidence, route } = opening else { panic!("opened: {opening:?}") };
    assert_eq!(*route, "a bottom on the filing", "the tile lies against its own history: the plain case");
    // The right evidence: the leaf, stripped for the bound verdict, deferred to a dissection.
    assert_eq!(evidence.leaf_index(), liar.leaf);
    assert_eq!(evidence.refutation.binding.committed_execution_root, liar.execution_root);
    let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
        execution_root: liar.execution_root,
        class_id: f.profile.shape_profile_id(),
        artifact_root: f.root,
    };
    assert_eq!(
        evidence.verdict_at_v2(&bound_to, PALW_HELD_STEP_LADDER_V1, true),
        Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
    );
    // 2. The book: the case's one step is the held dissection's opening.
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(matches!(&filer.case(&claim).expect("the case").step, PalwReplayCaseStepV1::Dissect(d) if d.leaf == liar.leaf));
    assert!(queue.pending.is_empty(), "nothing is queued off the loop");
    // 3. The loop half on the licensed claim: built, asked of the fold, queued with its due date.
    let s103 = licence(&s102, claim);
    assert!(matches!(phase_of(&s103, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    let final_daa = s103.deadline_of(&claim).expect("the licence's Final, in the fold's sweep queue");
    assert_eq!(queue.held_step(&mut filer, &s103, 104), 1);
    let (a, b, c, object) = queue.pending[0].clone();
    assert_eq!(queue.due.get(&(a, b, c)), Some(&104), "a one-move accusation: due now, as the A-held node dates it");
    assert!(104 < final_daa, "before the fold's Final");
    let PalwConsensusObjectV2::ShardCourtAccused { accusation } = &object else { panic!("a ShardCourtAccused: {object:?}") };
    assert_eq!(
        (accusation.claim, accusation.leaf_index, accusation.executor_bond, accusation.accuser_bond),
        (claim, liar.leaf, bond_key(PRODUCER), bond_key(SEAT))
    );
    assert_eq!((accusation.execution_root, accusation.trace_root), (duty.execution_root, duty.trace_root));
    let id = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(h64(999).as_byte_slice(), accusation);
    assert_eq!((a, b, c), (id, PALW_HELD_OPENING_QUEUE_ROUND_V1, false), "keyed by the accusation, as the capture arm keys it");
    assert_eq!(queue.held_step(&mut filer, &s103, 104), 0, "one queue entry: asked again only a re-plan later");
    assert_eq!(queue.pending.len(), 1);
    // 4. The fold opens it — the producer's root claim due at the class's compute turn (F5).
    let carried = queue.carry(104).expect("carried");
    let s104 = step(&s103, 104, &[carried]).expect("the fold opens the held dissection");
    let sid = s104
        .court_sessions_iter()
        .find(|(_, x)| x.claim == claim && x.challenger_bond == bond_key(SEAT))
        .map(|(k, _)| *k)
        .expect("the session");
    let producer = duty_of(&s104, PRODUCER, sid).expect("the producer's duty");
    assert_eq!(palw_held_move_of_duty_v1(&producer), Some(PalwHeldMoveV1::Root), "the producer owes the held root claim");
    assert_eq!(producer.terminal_index, Some(liar.leaf), "narrowed at the named leaf");
    let turn = kaspa_consensus_core::palw_class_admission_v2::palw_held_move_turn_daa_v1(&f.profile, TURN);
    assert_eq!(producer.rung_deadline_daa, (104 + turn).min(producer.session_deadline_daa), "the move turn, off the session");
    // Not discriminating on this fixture (the review's LOW): its compute turn is at or below the
    // court's TURN, so `palw_held_move_turn_daa_v1` is TURN whether or not the fold stamps F5's compute
    // turn. The discriminating cases — a row whose compute turn exceeds the court's — are the A-held
    // line's own: held_court_e2e's
    // `f5_the_producers_node_files_inside_the_compute_turn_where_the_courts_turn_is_shorter` (the
    // node) and the fold's `f5_the_held_compute_moves_get_the_classs_compute_turn`. P2-8e reads no turn.
    let compute = kaspa_consensus_core::palw_class_admission_v2::palw_held_compute_turn_daa_v1(&f.profile).expect("a held row");
    assert!(compute <= TURN && turn == TURN, "pinned: F5 is not what this fixture tells apart ({compute} ≤ {TURN})");
    assert!(duty_of(&s104, SEAT, sid).is_some_and(|d| !d.i_am_responder), "the seat is its challenger");
    // 5. The next tick reads the session off the chain and stands aside: the held route's from here.
    assert_eq!(queue.held_step(&mut filer, &s104, 105), 0);
    assert!(filer.case(&claim).is_none() && filer.settled_v1(&claim), "the case settled");
    assert!(filer.held.dissected_v1(&claim, liar.leaf));
    // 6. Played: the liar's least lie against the seat's held route, every move folded.
    let accused =
        liar.backend.attn_site_evidence_held_v1(&liar.material, liar.leaf, Some(&liar.ids), None).expect("the liar's own evidence");
    let site = accused.site_v1(f.root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let (lying_root, lane, delta) = least_lie_root(&accused, f.root);
    let (a2, p2) = (f.artifact.clone(), f.profile.clone());
    let mut host = HeldHost {
        state: s104.clone(),
        make: Box::new(move || backend(&a2, &p2)),
        ledger: PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None),
        carried: liar.ids.clone(),
    };
    let mut held = PalwHeldCourtV1::default();
    let mut seat_queue = Queue::default();
    let before = (collateral(&s104, PRODUCER), collateral(&s104, SEAT));
    let mut s = s104;
    let mut chain: Vec<PalwConsensusObjectV2> = vec![object];
    let mut seat_moves = Vec::new();
    let mut liar_rounds = 0usize;
    for daa in 105..105 + 2_000 {
        if s.court_session(&sid).is_none() {
            break;
        }
        let mut objects = Vec::new();
        if let Some(duty) = duty_of(&s, PRODUCER, sid) {
            match palw_held_move_of_duty_v1(&duty) {
                Some(PalwHeldMoveV1::Root) => {
                    let mut root_object = accused.root_claim_held_v1(&site, sid, 2).expect("the liar's held root claim");
                    let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { root, signature, .. } = &mut root_object else {
                        unreachable!()
                    };
                    *root = lying_root.clone();
                    *signature = vec![0xAA; 8];
                    objects.push(root_object);
                }
                Some(PalwHeldMoveV1::Round) => {
                    let phase = duty.dissection.as_ref().expect("the phase");
                    let mut round = accused.round_v1(&site, phase).expect("the liar's round");
                    let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
                    round.children[first].v_acc[lane] += delta;
                    liar_rounds += 1;
                    objects.push(PalwConsensusObjectV2::CourtAttnDissected { session_id: sid, round, signature: vec![0xAA; 8] });
                }
                _ => {}
            }
        }
        if objects.is_empty()
            && let Some(object) = held_tick(&mut host, &mut held, &mut seat_queue, &s, &chain, daa).await
        {
            seat_moves.push(object.clone());
            objects.push(object);
        }
        s = step(&s, daa, &objects).unwrap_or_else(|e| panic!("DAA {daa}: the fold refuses a move: {e}"));
        chain.extend(objects);
    }
    assert!(s.court_session(&sid).is_none(), "the held dissection closed");
    let choices = seat_moves.iter().filter(|o| matches!(o, PalwConsensusObjectV2::CourtAttnChildChosen { .. })).count();
    assert!(liar_rounds >= 1, "the lie sits deep enough in its history for the dissection to play rounds");
    assert_eq!(choices, liar_rounds, "the seat named the lied child at every round the liar disclosed");
    assert!(
        seat_moves.iter().any(|o| matches!(o, PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ExecutorGuilty, .. })),
        "the seat's node filed the bottom"
    );
    let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&s, &claim) else { panic!("voided: {:?}", phase_of(&s, &claim)) };
    assert_eq!(reason, PalwVoidReasonV2::CourtHeldVerdict, "F3 (B): a held dissection's verdict");
    assert_eq!(borsh::to_vec(&reason).unwrap(), vec![8], "reason 8");
    assert!(collateral(&s, PRODUCER) < before.0, "the producer is charged");
    assert_eq!(collateral(&s, SEAT), before.1, "the winning seat pays nothing");
}

/// **Liveness: an honest 8k claim is never dissected.** The seat's replay reproduces the honest fold —
/// the run finds nothing and the case settles with nothing queued. And a seat whose replay parts from
/// the honest claim AT a fused leaf (its own engine's fault) opens nothing either, on either lane: N2
/// against the accused's own filing reproduces the committed tile, so the court's kernels say there is
/// no lie ([`PalwHeldOpeningV1::Reproduces`]).
#[tokio::test(flavor = "multi_thread")]
async fn t54g_an_honest_8k_claim_produces_no_dissection() {
    let f = Fixture::new(false);
    let honest = &f.d;
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let seat = f.seat();
    let run = f.run(&seat, &s102, claim, true);
    assert!(
        matches!(run, PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, .. })),
        "{run:?}"
    );
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(filer.settled_v1(&claim));
    assert_eq!(queue.held_step(&mut filer, &licence(&s102, claim), 104), 0);
    assert!(queue.pending.is_empty(), "nothing filed against an honest producer");
    // A seat whose own replay parted at the fused leaf: the kernels on the accused's filing decide.
    let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&s102, &claim).expect("the claim");
    let n = seat.capture_shape(&honest.material).expect("a capture").step_leaf_count;
    let served = PalwHeldServedV1 { capture: honest.material.clone(), roots: honest.roots(), carried: Some(honest.ids.clone()) };
    let opening = palw_held_opening_v1(&seat, &served, &target, honest.leaf, n, seat.prompt_ids_form(), &opening_ctx(f.root));
    assert!(matches!(opening, PalwHeldOpeningV1::Reproduces), "the free-prompt lane: an honest tile is the kernels' own: {opening:?}");
    // The attempt lane (its prompt re-derived from the anchor): the class's honest attempt fold.
    let (job, prompt) = seat.job_for_anchor(h64(0x0A11_E1D0)).expect("the anchor's job");
    let attempt = seat.execute(&job, &prompt).expect("the honest attempt");
    let attempt_ctx =
        misaka_palw_base0::produce::base0_material_decode_any_v1(&attempt.material).expect("decodes").binding().job_context.clone();
    let attempt_target = kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1 {
        execution_root: attempt.execution_root,
        trace_root: attempt.trace_root,
        ..target
    };
    let served = PalwHeldServedV1 {
        capture: attempt.material.clone(),
        roots: PalwClaimRootsV1 {
            execution_root: attempt.execution_root,
            trace_root: attempt.trace_root,
            anchor: h64(0x0A11_E1D0),
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        },
        carried: None,
    };
    let n = seat.capture_shape(&attempt.material).expect("a capture").step_leaf_count;
    let leaf = fused_leaf_at(&f.profile, &attempt_ctx, 4, 0);
    let opening = palw_held_opening_v1(&seat, &served, &attempt_target, leaf, n, seat.prompt_ids_form(), &opening_ctx(f.root));
    assert!(matches!(opening, PalwHeldOpeningV1::Reproduces), "the attempt lane: {opening:?}");
}

/// **The seat, reading the served fold whole** — the stand-in T54g needs for a retention the seat
/// can read at the lie. Today a FRESH instance cannot: a held class retains a fold, and every verb
/// that reads below its retained level (`bisect_prefix_state`, the leaf provers, the windowed
/// responder) re-derives it by an honest re-execution, which does not reproduce a lying fold's roots
/// — the gap [`t54g_gap_b_a_fresh_seat_cannot_bisect_a_lying_held_fold`] pins. The stand-in reads the
/// SERVED capture at its committed leaves: its prefix states from the leaf vector the producer
/// committed (re-derived and proven to root to the fold's step root in [`produce`]), its leaf
/// evidence and its held filing (N1) through the liar's own instance (the drill's rule: the
/// instance that ran a lie re-derives it). Everything that is the seat's own — its replay of the
/// job, its N2 (the windowed challenger against the filing), the artifact rows, any capture that is
/// not the served one — runs on the seat's honest instance. `withhold` refuses the leaf provers
/// (the trait's defaults): served material that verifies and bisects and does not carry the leaf's
/// opening.
struct ServedView<'a> {
    liar: &'a Qwen25A16Backend,
    own: &'a Qwen25A16Backend,
    served: &'a [u8],
    /// The served capture's job and committed leaves: its prefix states, as a readable retention
    /// answers them (`base0_bisect_prefix_state_v1`, the family's own rung).
    ctx: &'a PalwJobContextV2,
    committed_leaves: &'a [Hash64],
    withhold: bool,
    /// A ledger whose reserved bytes are recorded at every windowed build (N1 and N2).
    probe: Option<(Arc<PalwMemoryLedgerV1>, std::sync::Mutex<Vec<u64>>)>,
}

impl ServedView<'_> {
    fn of(&self, material: &[u8]) -> &Qwen25A16Backend {
        if material == self.served { self.liar } else { self.own }
    }
}

impl PalwExecutionBackendV1 for ServedView<'_> {
    fn model_id(&self) -> &str {
        self.own.model_id()
    }
    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        self.own.job_for_anchor(anchor)
    }
    fn execute(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1, String> {
        self.own.execute(job, prompt)
    }
    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.own.execute_free_prompt(job, prompt_tokens)
    }
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        self.of(material).verify_material(material, claim)
    }
    fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
        self.of(material).capture_shape(material)
    }
    fn disclose_trace_event(
        &self,
        material: &[u8],
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        self.of(material).disclose_trace_event(material, row, tile)
    }
    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<Hash64> {
        if material == self.served {
            return Some(misaka_palw_base0::legs::base0_bisect_prefix_state_v1(self.ctx, self.committed_leaves, index));
        }
        self.own.bisect_prefix_state(material, index)
    }
    fn fp_leaf_refutation_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        claim: PalwClaimRootsV1,
        work_leaves: u64,
        leaf: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        if self.withhold {
            return Err("withheld".into());
        }
        self.of(capture).fp_leaf_refutation_v1(capture, prompt_token_ids, claim, work_leaves, leaf)
    }
    fn refutation_for_free_prompt_index(
        &self,
        material: &[u8],
        index: u64,
        prompt_token_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        if self.withhold {
            return Err("withheld".into());
        }
        self.of(material).refutation_for_free_prompt_index(material, index, prompt_token_ids)
    }
    fn operand_openings_for(
        &self,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
        self.own.operand_openings_for(refutation)
    }
    fn attn_site_evidence_held_v1(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        filing: Option<&kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1>,
    ) -> Result<PalwAttnHeldEvidenceV1, String> {
        if let Some((ledger, seen)) = &self.probe {
            seen.lock().unwrap().push(ledger.reserved_bytes());
        }
        // N1 reads the served capture; N2 (no material, the filing) is the seat's own replay.
        self.of(material).attn_site_evidence_held_v1(material, narrowed, carried_prompt, filing)
    }
}

/// **Missing material: P2-8d's `StepLeaf` demand is the one fallback — and DA-3 refuses it for a fused
/// leaf before anything is signed.** The seat holds the claim's fold (it verifies, it bisects to the
/// fused leaf) but cannot open the leaf's evidence from it: the run returns `Missing`, the case's one
/// step becomes the demand of that leaf (never a dissection beside it), the loop half queues no
/// opening, and the ONE held builder refuses the fused unit (`NeedsDissection`) — so P2-8d's demand
/// settles with the fold's reason and nothing is filed.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_missing_material_falls_back_to_the_step_leaf_demand_never_both() {
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, duty) = noted(&s102, claim);
    let seat = f.seat();
    let run = f.run(&f.view(&seat, true), &s102, claim, true);
    let PalwReplayRunV1::FusedLeaf { leaf, opening: PalwHeldOpeningV1::Missing(why), .. } = &run else { panic!("missing: {run:?}") };
    assert_eq!(*leaf, f.d.leaf);
    assert!(why.contains("does not open"), "{why}");
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    let PalwReplayCaseStepV1::Demand { leaf: demanded, binding, sends, .. } = &filer.case(&claim).expect("the case").step else {
        panic!("P2-8d's demand: {:?}", filer.case(&claim).map(|c| &c.step))
    };
    assert_eq!((*demanded, *sends), (f.d.leaf, 0));
    let binding = (**binding).clone();
    assert_eq!(queue.held_step(&mut filer, &licence(&s102, claim), 104), 0, "never both: no opening beside the demand");
    assert!(queue.pending.is_empty());
    assert_eq!(filer.demands_due_v1(104, &queue.pending, &queue.moved), vec![(claim, f.d.leaf)], "the demand is P2-8d's to ask");
    let refused = kaspa_consensus_core::palw_da_rcore_v1::palw_da_held_accusation_object_v1(
        &h64(999),
        claim,
        &duty.execution_root,
        kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: f.d.leaf },
        binding,
        bond_key(SEAT),
        PalwPromptIdsFormV1::Flat,
        |_, _| panic!("nothing is signed for a unit the fold refuses"),
    );
    assert_eq!(
        refused,
        Err(kaspa_consensus_core::palw_da_rcore_v1::PalwDaHeldAccusationBuildErrorV1::NeedsDissection(f.d.leaf)),
        "DA-3: a fused leaf is a dissection's, never a unit"
    );
}

/// **Dedup: one dissection per `(claim, leaf)` — across filers, ticks, the chain and a restart.** The
/// same accusation already queued by another filer (the capture arm keys it identically) is one
/// entry; once the session is open, the case settles, no new case is noted for the claim, and a second
/// `Open` for the leaf settles — in the book that opened it and in a RESTARTED node's fresh book, whose
/// first tick reads the session off the chain; and the fold refuses a second opening by this seat.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_one_dissection_per_claim_and_leaf_and_a_restart_rebuilds_it_from_the_chain() {
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    // Another filer queued the same accusation first: this case's opening is the same queue entry.
    let (mut filer, duty) = noted(&s102, claim);
    let (key, object) = f.opening(&s102, claim, &duty);
    let mut queue = Queue::default();
    queue.pending.push((key.0, key.1, key.2, object));
    let run = f.run(&f.view(&f.seat(), false), &s102, claim, true);
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    let s103 = licence(&s102, claim);
    assert_eq!(queue.held_step(&mut filer, &s103, 104), 0, "already queued by the other filer");
    assert_eq!(queue.pending.len(), 1, "one entry");
    assert_eq!(queue.due.get(&key), Some(&104), "dated all the same: due now");
    assert_eq!(queue.held_step(&mut filer, &s103, 105), 0);
    assert_eq!(queue.pending.len(), 1);
    // The session opens; the opener's next tick settles its case.
    let (s104, _sid, opened_claim, mut opener) = opened_by_the_node(&f);
    assert_eq!(opened_claim, claim);
    let mut q2 = Queue::default();
    assert_eq!(q2.held_step(&mut opener, &s104, 105), 0);
    assert!(opener.settled_v1(&claim) && opener.held.dissected_v1(&claim, f.d.leaf));
    // A restarted node, in the tick's own order (the review's MED-4): its first tick reads the chain's
    // session BEFORE the standing trigger is noted — so the trigger notes nothing, nothing runs.
    let mut restarted = PalwReplayFilerV1::default();
    let mut q3 = Queue::default();
    assert_eq!(q3.tick(&mut restarted, &s104, 106, std::slice::from_ref(&duty), None), 0);
    assert!(restarted.held.dissected_v1(&claim, f.d.leaf), "rebuilt from the chain's court duties");
    assert!(restarted.case(&claim).is_none() && restarted.due_v1(106).is_none(), "the trigger noted nothing; nothing runs");
    // A node whose case was noted before the chain showed the session (it restarted while the
    // opening's carrier was in flight): the next tick's read ends the waiting case before its run.
    let mut early = PalwReplayFilerV1::default();
    let mut q4 = Queue::default();
    assert_eq!(q4.tick(&mut early, &s103, 104, std::slice::from_ref(&duty), None), 0);
    assert_eq!(early.due_v1(104), Some(claim), "noted, due to run");
    assert_eq!(q4.tick(&mut early, &s104, 105, std::slice::from_ref(&duty), None), 0);
    assert!(early.case(&claim).is_none() && early.due_v1(105).is_none(), "ended at the tick's top, its run never paid for");
    // The run in flight when the session appeared (the one case the read leaves, its task holding the
    // ledger): it settles on return, filing nothing.
    early.running = Some((claim, tokio::task::spawn_blocking(|| PalwReplayRunV1::HostFailed("in flight".into()))));
    early.cases.insert(
        claim,
        PalwReplayCaseV1 {
            duty: duty.clone(),
            site: PalwReplayMismatchSiteV1::Replay,
            noted_daa: 106,
            runs: 1,
            retry_at: 106,
            step: PalwReplayCaseStepV1::Running,
        },
    );
    let again = f.run(&f.view(&f.seat(), false), &s102, claim, true);
    assert_eq!(q4.tick(&mut early, &s104, 106, &[], Some((claim, again))), 0);
    assert!(early.case(&claim).is_none() && q4.pending.is_empty(), "settled: the chain holds this dissection; nothing filed");
    // A restart while the opening is still in the mempool (not yet in a block): seeded once a start,
    // so the standing trigger does not run the case again while the carrier may land.
    let carrier = lifecycle_carrier_of(f.opening(&s102, claim, &duty).1);
    assert_eq!(palw_held_pooled_openings_v1([&carrier], bond_key(SEAT)), vec![(claim, f.d.leaf)]);
    assert!(palw_held_pooled_openings_v1([&carrier], bond_key(COLLUDER)).is_empty(), "another bond's opening is not this one's");
    let mut pooled = PalwReplayFilerV1::default();
    pooled.seed_held_v1(palw_held_pooled_openings_v1([&carrier], bond_key(SEAT)), 103);
    let mut q5 = Queue::default();
    assert_eq!(q5.tick(&mut pooled, &s103, 103, std::slice::from_ref(&duty), None), 0);
    assert!(pooled.case(&claim).is_none() && pooled.seeded_v1(), "seeded: not noted, not run");
    // The fold agrees: a second opening by this seat on the claim is refused while its session is open.
    let (_, second) = f.opening(&s102, claim, &duty);
    let answer = FilerHost { state: s104.clone(), daa: 107 }.rehearse(&second).expect("a tip");
    assert!(matches!(answer, PalwObjectRehearsalV1::Refused(_)), "{answer:?}");
    assert_eq!(palw_held_rehearsal_step_v1(&answer), PalwSeatAccuseStepV1::Retry, "a wait the chain read settles first");
}

/// **The fence-off twin.** Below `palw_offence_attribution` (or on a class the chain does not record
/// as held) the node builds no held opening: the same garbage claim's run returns the fused finding
/// alone, the case settles through the hook and nothing is queued. Below `palw_rcore_plus` the filer
/// is dormant altogether.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_twin_without_the_held_route_nothing_is_opened() {
    assert!(!palw_replay_filer_armed_v1(false, true), "below palw_rcore_plus: dormant");
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let run = f.run(&f.view(&f.seat(), false), &s102, claim, false);
    let PalwReplayRunV1::Found(PalwReplayFindingV1::NeedsDissection { leaf, .. }) = &run else {
        panic!("the fused finding alone: {run:?}")
    };
    assert_eq!(*leaf, f.d.leaf, "the same leaf is found");
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(filer.settled_v1(&claim), "settled through the hook");
    assert_eq!(queue.held_step(&mut filer, &licence(&s102, claim), 104), 0);
    assert!(queue.pending.is_empty(), "nothing opened");
}

/// **The gap, pinned: a FRESH seat instance cannot bisect a lying held fold** (for the operator; the
/// replay filer's P2-8b half, which P2-8e inherits). Every seat verifies the liar's fold (its roots
/// are read off the retained tree), but the bisection's first rung asks the served capture's prefix
/// state, which base0 answers for a fold only by an honest re-execution — and a re-execution of a
/// lying fold does not reproduce its roots, so the rung is `Unreadable` on the served side and the
/// run settles with nothing filed. On a held class (whose every retention is a fold) no replay filer
/// reaches a fused leaf until the fold can be read at the lie — by its retained nodes and a
/// `StepRange` disclosure of the divergent block, and a disclosure of the fused tile the opening
/// carries (ADR-0152 DA-9's "UNVERIFIED on 8k and 2M").
#[tokio::test(flavor = "multi_thread")]
async fn t54g_gap_b_a_fresh_seat_cannot_bisect_a_lying_held_fold() {
    use kaspa_consensus_core::palw_replay_refute_v1::{PalwReplayBisectStopV1, PalwReplaySideV1};
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let seat = f.seat();
    assert_eq!(seat.verify_material(&f.d.material, f.d.roots()), PalwMaterialVerdictV1::Matches, "every seat verifies the fold");
    let run = f.run(&seat, &s102, claim, true);
    assert!(
        matches!(
            run,
            PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing {
                why: PalwReplayNothingV1::Bisect(PalwReplayBisectStopV1::Unreadable { side: PalwReplaySideV1::Served, .. }),
                ..
            })
        ),
        "{run:?}"
    );
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(filer.settled_v1(&claim), "settled: nothing located");
    assert_eq!(queue.held_step(&mut filer, &licence(&s102, claim), 104), 0);
    assert!(queue.pending.is_empty());
}

/// **The gap, (c) (the review's HIGH): a FRESH seat cannot build the opening of a lying held fold.**
/// Even handed the leaf (as if (b) were closed), a fresh instance reads the served fold only by an
/// honest re-execution, which the lie's roots refuse: the one move's evidence at the leaf does not
/// open (`fp_leaf_refutation_v1`), N1 — the windowed responder on the served capture — does not
/// build, and the opening is `Missing`, whose P2-8d fallback DA-3 refuses before it is signed
/// (`t54g_missing_material_falls_back_to_the_step_leaf_demand_never_both`). The fold keeps no tile
/// and no DA unit discloses one, so this is not a matter of the seat trying harder. Through the liar's
/// own instance (what a retention readable at the lie would serve) the same call opens — which is all
/// the other T54g cases stand on.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_gap_c_a_fresh_seat_cannot_build_the_opening_of_a_lying_held_fold() {
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&s102, &claim).expect("the claim");
    let seat = f.seat();
    let n = seat.capture_shape(&f.d.material).expect("the fold's shape reads off its binding").step_leaf_count;
    let served = PalwHeldServedV1 { capture: f.d.material.clone(), roots: f.d.roots(), carried: Some(f.d.ids.clone()) };
    let fresh = palw_held_opening_v1(&seat, &served, &target, f.d.leaf, n, seat.prompt_ids_form(), &opening_ctx(f.root));
    let PalwHeldOpeningV1::Missing(why) = &fresh else { panic!("a fresh seat's opening: {fresh:?}") };
    assert!(why.contains("does not open"), "{why}");
    let roots = PalwClaimRootsV1 { output_root: None, ..f.d.roots() };
    assert!(seat.fp_leaf_refutation_v1(&f.d.material, &f.d.ids, roots, n, f.d.leaf).is_err(), "the one move's evidence");
    assert!(seat.attn_site_evidence_held_v1(&f.d.material, f.d.leaf, Some(&f.d.ids), None).is_err(), "N1");
    let view = f.view(&seat, false);
    let opened = palw_held_opening_v1(&view, &served, &target, f.d.leaf, n, seat.prompt_ids_form(), &opening_ctx(f.root));
    assert!(matches!(opened, PalwHeldOpeningV1::Open { .. }), "through the liar's own instance: {opened:?}");
}

/// **The gap, (a) (the review's HIGH): a canonical 8k attempt is past the whole-capture cap.** On
/// testnet-12's 8k genesis row the canonical job is ≈ 105.5M step leaves, past the host's 2^26: the
/// node's need (`whole_capture_memory_need_v1`, whose first step is `palw_whole_capture_admits_v1`)
/// refuses it, and the run is `Never` — nothing replayed, bisected or opened — whatever is served, and
/// the case settles. (Every other run here prices the fixture's small capture instead: `seat_run`.)
#[tokio::test(flavor = "multi_thread")]
async fn t54g_gap_a_a_canonical_8k_attempt_is_past_the_whole_capture_cap() {
    use kaspa_consensus_core::palw_resource_profile_v1::PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1;
    use kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1;
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let (profile, job) = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } if c.profile.n_ctx == N_CTX_8K => {
                Some((c.profile.clone(), c.canonical.clone()))
            }
            _ => None,
        })
        .expect("testnet-12's 8k row");
    let ladder = palw_class_step_ladder_v1(PALW_HELD_STEP_LADDER_V1, &profile);
    let leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &job, ladder).expect("the canonical job");
    assert!(leaves > PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, "{leaves} leaves");
    let refused = crate::palw_backends::palw_whole_capture_admits_v1(PALW_HELD_STEP_LADDER_V1, ladder, &profile, leaves)
        .expect_err("past the cap");
    // The run priced as the node prices that capture: `Never`, nothing reserved or replayed.
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&s102, &claim).expect("the claim");
    let input = PalwReplayRunInputV1 {
        retained: None,
        payloads: vec![f.d.served()],
        lane: PalwReplayLaneV1::FreePrompt {
            class_id: target.class_id,
            executor: bond_key(PRODUCER),
            roots: PalwClaimRootsV1 { anchor: Hash64::default(), ..f.d.roots() },
        },
        target,
        ladder: PALW_HELD_STEP_LADDER_V1,
        form: f.d.backend.prompt_ids_form(),
        held: Some(opening_ctx(f.root)),
    };
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None);
    let run = palw_replay_filer_run_v1(
        &f.view(&f.seat(), false),
        input,
        |_| Err::<u64, String>(refused.clone()),
        |_| -> Result<(), String> { panic!("never reserved") },
        || -> Result<(), String> { panic!("never reserved") },
        &ledger,
    );
    let PalwReplayRunV1::Never(why) = &run else { panic!("Never: {run:?}") };
    assert!(why.contains("materialization cap"), "{why}");
    let queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(filer.settled_v1(&claim) && queue.pending.is_empty(), "the case settles; nothing is filed");
}

/// **The review's MED-2: the opening is due by the FOLD's `Final`, never the claim rows' date.** On
/// testnet-12's windows (`window_challenge` 1,200, the short window of 120 in force), a claim licensed
/// at 103 goes `Final` at 223 in the fold, while the claim rows (the RPC's read, which P2-8e used to
/// be dated by) say 1,303 — ≈ 1,020 DAA past `Final`, so the EDF lane carried every item due before
/// `Final` first and the opening could meet `WrongPhase`. The node dates it as the A-held node dates a
/// one-move accusation: due now (104), before `Final` − 60 = 163, and the lane carries it before an
/// item due in between.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_the_opening_is_due_by_the_folds_final_on_testnet_12s_windows() {
    T12_WINDOWS.with(|on| on.set(true));
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let run = f.run(&f.view(&f.seat(), false), &s102, claim, true);
    let mut queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    let s103 = licence(&s102, claim);
    let final_daa = s103.deadline_of(&claim).expect("the licence's Final");
    let short = kaspa_consensus_core::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1;
    assert_eq!(final_daa, 103 + short, "the fold: window_challenge_at");
    // F3 of the pre-t12 drill (2026-09-25): the claim rows now read the fold's own `Final` floor —
    // they said `103 + 1,200`, the base window, where the fold finalizes at `103 + 120`.
    assert_eq!(row_deadline(&s103, &claim), Some(final_daa), "the claim rows: the fold's Final floor");
    assert_eq!(queue.held_step(&mut filer, &s103, 104), 1);
    let key = (queue.pending[0].0, queue.pending[0].1, queue.pending[0].2);
    let margin = PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1;
    assert_eq!(queue.due[&key], 104, "due now");
    assert!(queue.due[&key] <= final_daa - margin, "before Final less the landing margin");
    assert!(queue.due[&key] < row_deadline(&s103, &claim).unwrap() - margin, "never the claim rows' date");
    // The lane: an item due between the two dates goes after the opening.
    let between = (h64(0xBE7), 0u32, false);
    queue.pending.insert(0, (between.0, between.1, between.2, queue.pending[0].3.clone()));
    queue.due.insert(between, final_daa + 500);
    palw_court_queue_edf_v1(&mut queue.pending, &queue.due);
    assert_eq!((queue.pending[0].0, queue.pending[0].1, queue.pending[0].2), key, "the opening first");
    T12_WINDOWS.with(|on| on.set(false));
}

/// **The review's LOW: an opening the fold's own verdict refuses is not "missing material".** With the
/// node's accusation ladder at the leaf (a ladder that is not the chain's), the one move built at the
/// lie's leaf does not adjudicate: the run says `NotAdjudicable` (said at `error`), and the case
/// settles with nothing queued — never P2-8d's fallback, which would have hidden the misconfiguration.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_an_opening_the_folds_verdict_refuses_is_not_missing_material() {
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (mut filer, _) = noted(&s102, claim);
    let ctx = PalwHeldOpeningCtxV1 { ladder: f.d.leaf, ..opening_ctx(f.root) };
    let run = seat_run(&f.view(&f.seat(), false), &f.d, &s102, claim, f.d.served(), Some(ctx));
    let PalwReplayRunV1::FusedLeaf { opening: PalwHeldOpeningV1::NotAdjudicable(why), .. } = &run else {
        panic!("not adjudicable: {run:?}")
    };
    assert!(why.contains(&format!("ladder {}", f.d.leaf)), "{why}");
    let queue = Queue::default();
    filer.on_run_v1(claim, run, 102, &bond_key(SEAT));
    assert!(filer.settled_v1(&claim) && filer.case(&claim).is_none(), "settled, not a demand");
    assert!(queue.pending.is_empty() && filer.demands_due_v1(104, &queue.pending, &queue.moved).is_empty());
}

/// **The review's LOW: the opening P2-8e builds meets the acceptance layer's gates, on testnet-12's
/// own court.** The `ShardCourtAccused` arm of `palw_v2_validate_objects`, step by step over the
/// fixture's state: the accuser's ML-DSA-87 over the session id — at the network domain the processor
/// derives (`palw_network_domain_v2_for(net, genesis)`) — under the accusation context verifies with
/// the bond's key; the shape and the bytes at the ladder the processor derives (the claim's class
/// ladder over the network cap, `palw_refutation_leaf_cap_v2`) and its `max_close_bytes`; the bound
/// verdict `NeedsDissection`. And the node's accusation ladder for a held class (`seat_refutation_ladder_v1`
/// over `palw_class_step_ladder_v1`) is that same ladder. (A processor-level rehearsal of a held
/// opening needs the 8k held fixture inside the processor's harness; the fold's own arm is T54g's.)
#[tokio::test(flavor = "multi_thread")]
async fn t54g_the_opening_meets_the_acceptance_layers_gates_on_testnet_12s_court() {
    use kaspa_consensus_core::config::params::Params;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw_shard_court_v1::{
        PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, PalwOneMoveClaimV2, PalwShardCourtVerdictV1, palw_shard_court_accusation_bytes_v1,
        palw_shard_court_session_id_v1, palw_shard_court_verdict_at_v2,
    };
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let (_, duty) = noted(&s102, claim);
    let PalwReplayRunV1::FusedLeaf { opening: PalwHeldOpeningV1::Open { evidence, .. }, .. } =
        f.run(&f.view(&f.seat(), false), &s102, claim, true)
    else {
        panic!("an opening")
    };
    // The processor's ladder for the claim: its class's recorded ladder over the network cap.
    let network_cap = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
        &bundle.court,
        t12.palw_court_ladder.is_some_and(|fence| fence.is_active(104)),
    );
    let chain_ladder = s102.class_step_ladder_v1(&duty.class_id, network_cap);
    assert_eq!(chain_ladder, PALW_HELD_STEP_LADDER_V1, "a held class's recorded ladder");
    let node_class =
        kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(bundle.court.max_step_leaf_count(), &f.profile);
    let node_ladder = if node_class > bundle.court.max_step_leaf_count() { node_class.max(network_cap) } else { network_cap };
    assert_eq!(node_ladder, chain_ladder, "the node's accusation ladder is the chain's");
    // Built and signed with a real ML-DSA-87 key, at the processor's domain.
    let domain =
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(t12.net.to_string().as_bytes(), Some(t12.genesis.hash));
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x5E; 32]);
    let (key, object) = palw_held_opening_object_v1(&evidence, &duty, bond_key(SEAT), &domain, node_ladder, |message, context| {
        PalwPanelService::sign_hedged(&kp.signing_key, message, context)
    })
    .expect("built");
    let PalwConsensusObjectV2::ShardCourtAccused { accusation } = &object else { panic!("a ShardCourtAccused") };
    let session_id = palw_shard_court_session_id_v1(domain.as_byte_slice(), accusation);
    assert_eq!(key, (session_id, PALW_HELD_OPENING_QUEUE_ROUND_V1, false));
    assert!(
        kaspa_txscript::verify_mldsa87_with_context(
            kp.verification_key.as_ref(),
            session_id.as_byte_slice(),
            &accusation.signature,
            PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT
        )
        .unwrap_or(false),
        "signed by the bond it names, over the id the processor derives"
    );
    accusation.validate_shape(chain_ladder).expect("the shape at the chain's ladder");
    assert!(palw_shard_court_accusation_bytes_v1(accusation) <= bundle.court.max_close_bytes(), "inside the close ceiling");
    let claim_record = s102.claim(&claim).expect("the claim");
    assert_eq!(
        (claim_record.bond, claim_record.execution_root, claim_record.trace_root),
        (accusation.executor_bond, accusation.execution_root, accusation.trace_root),
        "the executor and the roots are the claim's"
    );
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: claim_record.execution_root, class_id: claim_record.class_id, artifact_root: f.root };
    assert_eq!(
        palw_shard_court_verdict_at_v2(accusation, &bound_to, chain_ladder, t12.palw_audit_2026_09_23_active_at(104)),
        Ok(PalwShardCourtVerdictV1::NeedsDissection),
        "the bound verdict defers to the held dissection"
    );
}

/// **The review's LOW: the opening's builds run under the held builds' own figure, not the dense
/// replay's.** The run reserves the dense capture's need for the replay (here 1,000,000 bytes); once a
/// fused finding is to be opened it takes the held route's figure for a build (here 10,000) FIRST and,
/// granted, releases the dense one — N1 and N2 are built with 10,000 reserved. Where the ledger cannot
/// also hold the held figure, the dense reservation (which covers it) is kept and the builds run under
/// it, as they always did. Everything is released with the run.
#[tokio::test(flavor = "multi_thread")]
async fn t54g_the_openings_builds_run_under_the_held_figure_not_the_dense_one() {
    use crate::palw_memory_ledger::PalwMemoryReservationKeyV1;
    let f = Fixture::new(true);
    let (s102, claim) = f.bound();
    let seat = f.seat();
    for (share, during) in [(1_010_000u64, 10_000u64), (1_005_000, 1_000_000)] {
        let host = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(share), || None);
        let mut view = f.view(&seat, false);
        view.probe = Some((host.clone(), Default::default()));
        let key = |role| PalwMemoryReservationKeyV1 { role, class_id: h64(1), job: claim };
        let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&s102, &claim).expect("the claim");
        let input = PalwReplayRunInputV1 {
            retained: None,
            payloads: vec![f.d.served()],
            lane: PalwReplayLaneV1::FreePrompt {
                class_id: target.class_id,
                executor: bond_key(PRODUCER),
                roots: PalwClaimRootsV1 { anchor: Hash64::default(), ..f.d.roots() },
            },
            target,
            ladder: PALW_HELD_STEP_LADDER_V1,
            form: f.d.backend.prompt_ids_form(),
            held: Some(opening_ctx(f.root)),
        };
        let rungs = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None);
        let run = palw_replay_filer_run_v1(
            &view,
            input,
            |_| Ok::<u64, String>(1_000_000),
            |need| host.reserve(key("replay-filer"), *need).map_err(|e| e.to_string()),
            || host.reserve(key("held-dissection"), 10_000).map_err(|e| e.to_string()),
            &rungs,
        );
        assert!(matches!(run, PalwReplayRunV1::FusedLeaf { opening: PalwHeldOpeningV1::Open { .. }, .. }), "{run:?}");
        let seen = view.probe.as_ref().expect("probed").1.lock().unwrap().clone();
        assert!(seen.len() >= 2 && seen.iter().all(|bytes| *bytes == during), "share {share}: reserved during N1/N2 {seen:?}");
        assert_eq!(host.reserved_bytes(), 0, "released with the run");
    }
}
