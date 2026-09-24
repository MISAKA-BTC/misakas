//! **ADR-0152 §4-ter T-A9 and T-A10 (the node half), on a held graph-v7 fixture row: the node's own
//! held route against the fold, and N4 live on a node past the fence.**
//!
//! T-A9 plays one held dissection per case through the real transition (every block re-applied,
//! reverted and reloaded), with each party's moves made the way the panel's tick makes them: the
//! party's court duty off the fold's duty view (`palw_court_duties_v2`, what the tick reads), routed
//! ([`palw_held_route_v1`]), the move and its due DAA read off that duty
//! ([`palw_held_move_of_duty_v1`], [`palw_held_move_deadline_v1`]), the evidence built by the node's
//! windowed builders — the responder's out of its claim's material through P2-7's loader
//! ([`palw_held_responder_evidence_v1`]), a challenger's from the accused's filing read back off the
//! object ([`palw_held_filing_of_duty_v1`], [`palw_held_challenger_evidence_v1`]) — and the object built
//! by [`palw_held_move_object_v1`]. Every move lands at a DAA no later than the deadline its session
//! carries. The fixture job is an ATTEMPT (its prompt the anchor's), so the node's own lane logic
//! runs unchanged.
//!
//! * an honest producer's node answers automatically — the held root claim (tag 57, the anchor's
//!   sub-roots) from a capture it had pruned and re-makes, then every round — and is never
//!   defaulted: against this node's honest seat the accusation dies on the challenger's own clock,
//!   and against a challenger that plays on to the bottom the producer's node files the acquittal;
//! * a producer's node that stays silent is defaulted (`CourtDefault`), only after the deadline the
//!   session carries;
//! * a lying producer (a lie in the committed fused tile, and the least lie in its root claim that
//!   finalizes to it) is still convicted: this node's seat names the lied child at every round and
//!   files the bottom — `CourtHeldVerdict` (F3, decision (B)).
use super::held_court::{
    PalwHeldMoveCtxV1, PalwHeldMoveOutcomeV1, PalwHeldMoveV1, palw_held_challenger_evidence_v1, palw_held_filing_of_duty_v1,
    palw_held_move_deadline_v1, palw_held_move_object_v1, palw_held_move_of_duty_v1, palw_held_responder_evidence_v1,
    palw_held_route_v1,
};
use super::{PalwDaClaimFactsV1, PalwDaLaneV1};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_attn_court_v1::{
    PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectChoiceV1, palw_attn_opened_lanes_v1,
};
use kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1;
use kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1};
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_producer_v2::{PalwCourtDutyV2, palw_court_duties_v2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2,
    PalwCourtVerdictV2, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_operator_id_v2,
    revert_delta_v2,
};
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index,
};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::Base0ArtifactV1;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const NETWORK: &[u8] = b"misaka-palw-rc";
const PRODUCER: u64 = 1;
const SEAT: u64 = 2;
const COLLUDER: u64 = 4;
/// The fixture's network ladder: its job builds densely too, so the drill can lie in a retained tile.
const LADDER: u64 = 1 << 22;
/// The class's canonical job: 64 prompt positions, 8 decode calls.
const CANONICAL: (u32, u32) = (64, 8);

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

/// **The held graph-v7 row at one head per tile** (base0's `held_fixture`): two layers at context
/// `n_ctx` — a fused site's output tile and query slice are one head's, which is what makes it
/// dissectable (`palw_fused_sites_are_dissectable_v1`).
pub(super) fn held_fixture(n_ctx: u32) -> (Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    held_fixture_of(kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 8,
    })
}

/// [`held_fixture`] at any held geometry (one head a tile).
fn held_fixture_of(
    geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
) -> (Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    use misaka_palw_base0::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a held graph-v7 profile");
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: geometry.rms_eps_q,
    };
    let artifact = Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    (artifact, profile)
}

fn backend(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> Qwen25A16Backend {
    backend_with(artifact, profile, CANONICAL)
}

fn backend_with(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), canonical)
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(LADDER)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
}

fn binding_of(material: &[u8]) -> PalwStepBindingV2 {
    misaka_palw_base0::produce::base0_material_decode_any_v1(material).expect("decodes").binding().clone()
}

/// A producer's attempt: the anchor's job, its run by `backend` (honest, or with the committed tile at
/// the fused site's leaf moved — the attempt lane's drill, a lie only the dissection reaches), the leaf.
struct Produced {
    backend: Qwen25A16Backend,
    job: PalwJobContextV2,
    prompt: Vec<usize>,
    anchor: Hash64,
    material: Vec<u8>,
    execution_root: Hash64,
    trace_root: Hash64,
    leaf: u64,
}

impl Produced {
    fn ids(&self) -> Vec<u32> {
        self.prompt.iter().map(|t| *t as u32).collect()
    }

    fn roots(&self) -> PalwClaimRootsV1 {
        PalwClaimRootsV1 {
            execution_root: self.execution_root,
            trace_root: self.trace_root,
            anchor: self.anchor,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        }
    }
}

/// The fused site's leaf at layer 0, head 0, of the job's last decode call.
fn fused_leaf(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u64 {
    fused_leaf_at(profile, job, job.exact_decode_tokens - 1, 0, 0)
}

/// The fused site's leaf at `layer`, head `head` (one head a tile), of call `call` at `position`.
fn fused_leaf_at(profile: &PalwShapeProfileV3, job: &PalwJobContextV2, call: u32, position: u32, head: u32) -> u64 {
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("the slot");
    let coord = PalwStepCoordinateV1 { call_index: call, node_slot: slot, position, tile_index: head };
    canonical_step_leaf_index(profile, job, &coord).expect("the site's leaf")
}

fn produce(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, lie: bool) -> Produced {
    produce_at(backend(artifact, profile), profile, lie, h64(0x0A11_E1D0))
}

/// An attempt of `backend`'s class at `anchor` — honest, or lying in the committed fused tile.
fn produce_at(backend: Qwen25A16Backend, profile: &PalwShapeProfileV3, lie: bool, anchor: Hash64) -> Produced {
    let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor's job");
    let leaf = fused_leaf(profile, &job);
    let outcome = if lie {
        backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drilled run commits")
    } else {
        backend.execute(&job, &prompt).expect("the honest run")
    };
    Produced {
        backend,
        job,
        prompt,
        anchor,
        material: outcome.material,
        execution_root: outcome.execution_root,
        trace_root: outcome.trace_root,
        leaf,
    }
}

// ---- the fold ------------------------------------------------------------------------------------

thread_local! {
    /// The court's turn this test's fold runs (20 unless a test sets it — each test is its own thread).
    static TURN: std::cell::Cell<u64> = const { std::cell::Cell::new(20) };
    /// Whether this test's fold runs past `palw_offence_attribution` (unless a test sets it).
    static FENCED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 600, 1000, h64(1), 4, 1000, 10_000, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_turn_deadline_daa(TURN.with(|turn| turn.get()))
        .unwrap()
        .with_worker_carve_permille(300)
        .unwrap()
}

/// testnet-12's launch line as far as the fold reads it — `palw_offence_attribution` in force.
fn launch() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        shard_court_ladder: Some(1 << 26),
        held_context_ladder: Some(1 << 26),
        audit_2026_09_23_active: true,
        audit_2026_09_11_deep_active: true,
        attn_anchored_root_active: true,
        court_responder_coverage_active: true,
        offence_attribution_active: FENCED.with(|fenced| fenced.get()),
        ..Default::default()
    }
}

/// One block through the real transition, checked: consistency, the delta re-applies and reverts, and
/// the carriage reloads under its root.
fn step(parent: &PalwChainStateV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    step_with(parent, daa, objects, None)
}

fn step_with(
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = params();
    let c = PalwBlockContextV2 { block: Hash64::from_u64_word(0x4E1D_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 10_000 };
    let (child, delta) = apply_palw_transition_v2_with_extras(parent, &p, &c, objects, att, false, false, false, true, &launch())?;
    child.assert_internal_consistency(&p).expect("internal consistency");
    child.assert_deadline_consistency(&p).expect("deadline consistency");
    assert_eq!(apply_delta_v2(parent, &delta, &p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &p).expect("reverts"), *parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&p, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    Ok(child)
}

/// The produced claim on the chain, licensed at DAA 103 by both seats of its panel.
fn licensed(d: &Produced, profile: &PalwShapeProfileV3, artifact_root: Hash64) -> (PalwChainStateV2, Hash64) {
    let (s, claims) = licensed_many(&[d], profile, artifact_root);
    (s, claims[0])
}

/// Every produced claim on the chain, licensed by both seats of its panel: the registry at 100, one
/// attempt a block from 101, every panel in the block after the last, every licence in the next —
/// for one claim the attempt at 101, the panel at 102, the licence at 103.
fn licensed_many(ds: &[&Produced], profile: &PalwShapeProfileV3, artifact_root: Hash64) -> (PalwChainStateV2, Vec<Hash64>) {
    licensed_many_under(ds, &binding_of(&ds[0].material).job_context, profile, artifact_root)
}

/// [`licensed_many`] with the class registered under `canonical` (an attempt's job — a claim of
/// another shape, the step-6 forger's free-prompt run, is still the class's claim).
fn licensed_many_under(
    ds: &[&Produced],
    canonical: &PalwJobContextV2,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
) -> (PalwChainStateV2, Vec<Hash64>) {
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
    let mut s = step(&PalwChainStateV2::genesis(), 100, &register).expect("the registry");
    assert!(s.class_is_held_v1(&class_id), "the held class records its ladder");
    let network_domain = h64(999);
    let executor_bond = bond_key(PRODUCER).0;
    let mut claims = Vec::new();
    for (i, d) in ds.iter().enumerate() {
        let env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: challenge_v2(network_domain, h64(5 + i as u64), 1_700, 1, h64(1), &executor_bond),
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
        claims.push(attempt_id_v2(&env.attempt));
        s = step_with(&s, 101 + i as u64, &[], Some(&env)).expect("the claim");
    }
    let at = 101 + ds.len() as u64;
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let panels: Vec<_> = claims
        .iter()
        .map(|claim| PalwConsensusObjectV2::PanelBound { claim: *claim, anchor: h64(77), seats: seats.clone() })
        .collect();
    let s = step(&s, at, &panels).expect("the panels");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    let licences: Vec<_> = claims
        .iter()
        .map(|claim| PalwConsensusObjectV2::ReceiptLicensed { claim: *claim, receipts: vec![valid(SEAT), valid(COLLUDER)] })
        .collect();
    let s = step(&s, at + 1, &licences).expect("the licences");
    (s, claims)
}

/// `SEAT`'s one-move accusation of `d`'s claim at `d`'s fused leaf: the executor's own leaf evidence,
/// stripped of its history — the object the bound verdict defers to the held dissection.
fn accusation_of(d: &Produced, claim_id: Hash64) -> PalwConsensusObjectV2 {
    let binding = binding_of(&d.material);
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        &d.backend,
        &d.material,
        &d.ids(),
        d.roots(),
        binding.step_leaf_count,
        d.leaf,
        d.backend.prompt_ids_form(),
    )
    .expect("the executor's own evidence at the leaf")
    .for_the_one_move_v2();
    let mut accusation = evidence.into_accusation_v1(claim_id, d.execution_root, d.trace_root, bond_key(PRODUCER), bond_key(SEAT));
    accusation.signature = vec![9; 8];
    PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }
}

/// The session `SEAT` opened on `claim_id`.
fn session_of(s: &PalwChainStateV2, claim_id: Hash64) -> Hash64 {
    s.court_sessions_iter()
        .find(|(_, x)| x.claim == claim_id && x.challenger_bond == bond_key(SEAT))
        .map(|(k, _)| *k)
        .expect("the held dissection opened")
}

/// `SEAT`'s one-move accusation at the fused leaf, folded at DAA 104.
fn opened(d: &Produced, profile: &PalwShapeProfileV3, artifact_root: Hash64) -> (PalwChainStateV2, Hash64, Hash64) {
    let (s, claim_id) = licensed(d, profile, artifact_root);
    let s = step(&s, 104, &[accusation_of(d, claim_id)]).expect("the accusation");
    let sid = session_of(&s, claim_id);
    (s, claim_id, sid)
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
        // ADR-0152 v3.1 §4-bis.9's decode-close door: read by a decode-token close only.
        true,
    )
}

fn collateral(s: &PalwChainStateV2, n: u64) -> u64 {
    s.bond(&bond_key(n)).expect("the bond").collateral
}

fn phase_of(s: &PalwChainStateV2, claim: &Hash64) -> PalwClaimPhaseV2 {
    s.claim(claim).expect("the claim").phase.clone()
}

// ---- the node ------------------------------------------------------------------------------------

/// `bond`'s court duty in the session, as the panel's tick reads it.
fn duty_of(s: &PalwChainStateV2, bond: u64, sid: Hash64) -> Option<PalwCourtDutyV2> {
    palw_court_duties_v2(s, &[bond_key(bond)]).into_iter().find(|duty| duty.session_id == sid)
}

/// **The node's held route for `bond` at `daa`**: its duty routed, the move it owes, the DAA the
/// session says it is due by, and the object built from `evidence` — `None` on the other party's
/// turn, past the move's deadline (the tick builds nothing then: the sweep decides it), or when there
/// is nothing to file.
fn node_move(
    s: &PalwChainStateV2,
    bond: u64,
    sid: Hash64,
    evidence: &PalwAttnHeldEvidenceV1,
    artifact_root: Hash64,
    daa: u64,
) -> Option<(PalwHeldMoveV1, u64, PalwConsensusObjectV2)> {
    let duty = duty_of(s, bond, sid)?;
    assert!(palw_held_route_v1(true, s.class_is_held_v1(&duty.class_id), &duty), "past the fence, the held route");
    let mv = palw_held_move_of_duty_v1(&duty)?;
    let due = palw_held_move_deadline_v1(&duty, mv);
    if daa > due {
        return None;
    }
    // The held regime's opening cap (the claim's ladder) and the fixture court's arity.
    let ctx = PalwHeldMoveCtxV1 { artifact_root, opening_cap: PALW_HELD_STEP_LADDER_V1, arity: 2 };
    match palw_held_move_object_v1(evidence, &duty, mv, &ctx, |proof| adjudicate(s, sid, proof).ok(), |_, _| Some(vec![0xAA; 8]))
        .unwrap_or_else(|e| panic!("{mv:?} builds: {e}"))
    {
        PalwHeldMoveOutcomeV1::File(object) => Some((mv, due, object)),
        PalwHeldMoveOutcomeV1::Nothing(_) => None,
    }
}

/// **The producer's node builds its evidence** — the responder's (N1) through P2-7's loader: the
/// capture it retained (`kept`), or, pruned, re-made by replaying the claim's job (the block's, chain
/// data) and handed to `keep`.
fn responder_evidence(d: &Produced, claim_id: Hash64, kept: Vec<Vec<u8>>) -> (PalwAttnHeldEvidenceV1, bool, Option<Vec<u8>>) {
    let facts = PalwDaClaimFactsV1 {
        claim_id,
        class_id: d.backend.profile().shape_profile_id(),
        executor_bond: bond_key(PRODUCER),
        execution_root: d.execution_root,
        trace_root: d.trace_root,
        work_leaves: binding_of(&d.material).step_leaf_count,
        form: d.backend.prompt_ids_form(),
        lane: PalwDaLaneV1::Attempt { anchor: d.anchor, attempt_draw: None, job: Some((d.job.clone(), d.prompt.clone())) },
        job_pin: None,
    };
    let mut kept_back = None;
    let (evidence, remade) =
        palw_held_responder_evidence_v1(&d.backend, &facts, kept, |bytes| kept_back = Some(bytes.to_vec()), d.leaf).expect("N1");
    (evidence, remade, kept_back)
}

/// **A seat's node builds its evidence** once the accused's filing is on chain — the held root claim
/// objects read back off the chain (what `attn_held_objects_from_chain_v1` returns), the first that
/// stands the fold's own checks kept, and N2 from it and one honest replay.
fn challenger_evidence(
    s: &PalwChainStateV2,
    sid: Hash64,
    seat: &Qwen25A16Backend,
    filed: &PalwConsensusObjectV2,
) -> PalwAttnHeldEvidenceV1 {
    let duty = duty_of(s, SEAT, sid).expect("the seat's duty");
    let (_, filing) = palw_held_filing_of_duty_v1(std::slice::from_ref(filed), &duty, PALW_HELD_STEP_LADDER_V1, &HashSet::new())
        .expect("the filing stands for the claim at its leaf");
    palw_held_challenger_evidence_v1(seat, &filing, duty.terminal_index.expect("the leaf"), None).expect("N2")
}

/// **The dissection played by the two nodes** from DAA 105 until the session closes: the producer's
/// node answers from `responder` (`None`: it stays silent), this node's seat from its N2 once the
/// filing is on chain; `lie` is a lying producer's play instead of its node's (the harness's
/// `(root, lane, delta)`: its root claim and the lie pushed into tile 0's child every round);
/// `stubborn` names the child covering the last tile when the seat's node names nothing — a
/// challenger that plays on to the bottom. Returns the final state and every object a node filed,
/// `(DAA filed, party, move, object)` — each checked on time as it is filed.
struct Played {
    state: PalwChainStateV2,
    filed: Vec<(u64, u64, PalwHeldMoveV1, PalwConsensusObjectV2)>,
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut s: PalwChainStateV2,
    sid: Hash64,
    artifact_root: Hash64,
    responder: Option<&PalwAttnHeldEvidenceV1>,
    lie: Option<(&PalwAttnHeldEvidenceV1, &PalwAttnRootClaimV1, usize, i64)>,
    seat: &Qwen25A16Backend,
    stubborn: bool,
) -> Played {
    let mut filed = Vec::new();
    let mut seat_evidence: Option<PalwAttnHeldEvidenceV1> = None;
    let mut root_object: Option<PalwConsensusObjectV2> = None;
    for daa in 105..105 + 2_000 {
        if s.court_session(&sid).is_none() {
            return Played { state: s, filed };
        }
        let mut objects = Vec::new();
        // The producer: its node, or the liar's play.
        match (responder, lie) {
            (Some(evidence), None) => {
                if let Some((mv, due, object)) = node_move(&s, PRODUCER, sid, evidence, artifact_root, daa) {
                    assert!(daa <= due, "the producer's {mv:?} filed at DAA {daa}, due by DAA {due}");
                    if mv == PalwHeldMoveV1::Root {
                        root_object = Some(object.clone());
                    }
                    filed.push((daa, PRODUCER, mv, object.clone()));
                    objects.push(object);
                }
            }
            (None, Some((accused, lying_root, lane, delta))) => {
                if let Some(duty) = duty_of(&s, PRODUCER, sid) {
                    let site = accused.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("the liar's site");
                    match palw_held_move_of_duty_v1(&duty) {
                        Some(PalwHeldMoveV1::Root) => {
                            let mut object = accused.root_claim_held_v1(&site, sid, 2).expect("the liar's held root claim");
                            let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { root, signature, .. } = &mut object else {
                                unreachable!("the builder files the held form")
                            };
                            *root = lying_root.clone();
                            *signature = vec![0xAA; 8];
                            root_object = Some(object.clone());
                            objects.push(object);
                        }
                        Some(PalwHeldMoveV1::Round) => {
                            let phase = duty.dissection.as_ref().expect("the phase");
                            let mut round = accused.round_v1(&site, phase).expect("the liar's round");
                            let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
                            round.children[first].v_acc[lane] += delta;
                            objects.push(PalwConsensusObjectV2::CourtAttnDissected {
                                session_id: sid,
                                round,
                                signature: vec![0xAA; 8],
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        // This node's seat, once the filing is on chain.
        if objects.is_empty() {
            if seat_evidence.is_none()
                && let Some(root) = root_object.as_ref()
                && duty_of(&s, SEAT, sid).is_some_and(|duty| duty.dissection.is_some())
            {
                seat_evidence = Some(challenger_evidence(&s, sid, seat, root));
            }
            if let Some(evidence) = seat_evidence.as_ref() {
                match node_move(&s, SEAT, sid, evidence, artifact_root, daa) {
                    Some((mv, due, object)) => {
                        assert!(daa <= due, "the seat's {mv:?} filed at DAA {daa}, due by DAA {due}");
                        filed.push((daa, SEAT, mv, object.clone()));
                        objects.push(object);
                    }
                    None if stubborn => {
                        let duty = duty_of(&s, SEAT, sid).expect("the seat's duty");
                        if duty.turn == PalwBisectTurnV1::AwaitVerdict
                            && let Some(phase) = duty.dissection.as_ref()
                        {
                            let last = phase.child_ranges().iter().map(|&(f, c)| f + c).max().expect("children") - 1;
                            let child =
                                phase.child_ranges().iter().position(|&(f, c)| last >= f && last < f + c).expect("covered") as u8;
                            let choice = PalwAttnDissectChoiceV1 {
                                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                                session_id: sid,
                                round: phase.round(),
                                child,
                            };
                            objects.push(PalwConsensusObjectV2::CourtAttnChildChosen {
                                session_id: sid,
                                choice,
                                signature: vec![0xBB; 8],
                            });
                        }
                    }
                    None => {}
                }
            }
        }
        s = step(&s, daa, &objects).unwrap_or_else(|e| panic!("DAA {daa}: {e}"));
    }
    panic!("the held dissection did not close")
}

/// **T-A9 (1): an honest producer's node answers a held dissection by itself and is never defaulted.**
/// Its capture pruned, the node re-makes it from the claim's block job through P2-7's loader (and
/// keeps it), builds N1, and files the held root claim — tag 57, carrying its anchor's slice
/// sub-roots — inside the opening rung, then every round inside the phase's clock. Against this
/// node's own honest seat, whose N2 reproduces every child, the accusation ends on the challenger's
/// clock: the seat is charged, the producer is not, the claim stands and goes on to `Final`. Against
/// a challenger that plays on to the bottom, the producer's node files the acquittal
/// (`ChallengerDefeated`) itself.
#[test]
fn t_a9_an_honest_producers_node_answers_the_held_dissection_and_is_never_defaulted() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let honest = produce(&artifact, &profile, false);
    let seat = backend(&artifact, &profile);
    for stubborn in [false, true] {
        let (s, claim, sid) = opened(&honest, &profile, root);
        let opening = duty_of(&s, PRODUCER, sid).expect("the producer's duty");
        assert_eq!(palw_held_move_of_duty_v1(&opening), Some(PalwHeldMoveV1::Root), "the responder owes the root claim");
        let (responder, remade, kept) = responder_evidence(&honest, claim, Vec::new());
        assert!(remade, "nothing kept: the capture is re-made from the claim's job");
        assert_eq!(kept.as_deref(), Some(honest.material.as_slice()), "and kept, byte for byte the producer's");
        let (again, remade, _) = responder_evidence(&honest, claim, vec![honest.material.clone()]);
        assert!(!remade && again == responder, "a kept copy that reproduces the roots answers as it is");
        let (before_s, before_p) = (collateral(&s, SEAT), collateral(&s, PRODUCER));
        let played = play(s, sid, root, Some(&responder), None, &seat, stubborn);
        let (daa, by, mv, object) = played.filed.first().expect("the producer's node filed").clone();
        assert_eq!((by, mv), (PRODUCER, PalwHeldMoveV1::Root));
        assert!(daa <= opening.rung_deadline_daa, "inside the opening rung ({daa} ≤ {})", opening.rung_deadline_daa);
        let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { slice_sub_roots, .. } = &object else {
            panic!("move 1 is the held form (tag 57): {object:?}")
        };
        assert!(!slice_sub_roots.is_empty() && *slice_sub_roots == responder.slice_sub_roots, "C2: the anchor's slice sub-roots");
        let rounds = played.filed.iter().filter(|(_, by, mv, _)| *by == PRODUCER && *mv == PalwHeldMoveV1::Round).count();
        assert!(rounds >= 1, "every round answered by the node");
        assert!(
            played.filed.iter().all(|(_, by, mv, _)| *by == PRODUCER || *mv != PalwHeldMoveV1::Choice),
            "an honest disclosure names nothing"
        );
        let closes: Vec<_> = played.filed.iter().filter(|(_, _, mv, _)| *mv == PalwHeldMoveV1::Close).collect();
        if stubborn {
            assert_eq!(closes.len(), 1, "the bottom is closed once");
            assert_eq!(closes[0].1, PRODUCER, "by the producer's node: its acquittal");
            assert!(matches!(
                &closes[0].3,
                PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ChallengerDefeated, .. }
            ));
        } else {
            assert!(closes.is_empty(), "no bottom: the accusation ends on the challenger's own clock");
        }
        let end = played.state;
        assert!(collateral(&end, SEAT) < before_s, "stubborn {stubborn}: the losing challenger pays");
        assert_eq!(collateral(&end, PRODUCER), before_p, "stubborn {stubborn}: the honest producer pays nothing");
        assert!(
            matches!(phase_of(&end, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "never defaulted: {:?}",
            phase_of(&end, &claim)
        );
        let mut state = end;
        for at in 3_000..5_000 {
            if matches!(phase_of(&state, &claim), PalwClaimPhaseV2::Final { .. }) {
                break;
            }
            state = step(&state, at, &[]).unwrap_or_else(|e| panic!("DAA {at}: {e}"));
        }
        assert!(matches!(phase_of(&state, &claim), PalwClaimPhaseV2::Final { .. }), "stubborn {stubborn}: the claim goes on to Final");
    }
}

/// **T-A9 (2): a producer's node that stays silent is defaulted** — `CourtDefault` (C1: an answerable
/// held class's silence is a default, not the mercy), and not one DAA before the deadline its session
/// carried: the claim stands through the rung and is voided the block after it.
#[test]
fn t_a9_a_silent_producers_node_is_defaulted_after_the_deadline_the_session_carries() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let honest = produce(&artifact, &profile, false);
    let (mut s, claim, sid) = opened(&honest, &profile, root);
    let due = duty_of(&s, PRODUCER, sid).expect("the producer's duty").rung_deadline_daa;
    let before = collateral(&s, PRODUCER);
    let mut voided_at = None;
    for daa in 105..due + 50 {
        s = step(&s, daa, &[]).unwrap_or_else(|e| panic!("DAA {daa}: {e}"));
        if let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&s, &claim) {
            assert_eq!(reason, PalwVoidReasonV2::CourtDefault, "silence is a default, not a verdict");
            voided_at = Some(daa);
            break;
        }
    }
    let voided_at = voided_at.expect("the silent producer is defaulted");
    assert!(voided_at > due, "not before the deadline the session carries ({voided_at} > {due})");
    assert!(collateral(&s, PRODUCER) < before, "and charged");
    assert!(s.court_session(&sid).is_none(), "the session is closed");
}

/// **T-A9 (3): a lying producer is still convicted — by this node's seat, automatically.** The
/// producer committed a lie in the fused tile (the attempt lane's drill at the site's leaf) and files
/// the least lie in its root claim that finalizes to it, then pushes the lie into tile 0's child at
/// every round. The seat's node reads the filing off the held root claim, builds N2, names the lied
/// child at every round, and files the bottom: `ExecutorGuilty`, and the claim is voided
/// `CourtHeldVerdict` (F3, decision (B)) — the producer charged, the winning seat not.
#[test]
fn t_a9_a_lying_producer_is_convicted_by_the_seats_node_as_a_held_verdict() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let liar = produce(&artifact, &profile, true);
    let seat = backend(&artifact, &profile);
    let (s, claim, sid) = opened(&liar, &profile, root);
    let accused = liar.backend.attn_site_evidence_held_v1(&liar.material, liar.leaf, None, None).expect("the liar's own evidence");
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
    let before = (collateral(&s, PRODUCER), collateral(&s, SEAT));
    let played = play(s, sid, root, None, Some((&accused, &lying_root, lane, lo)), &seat, false);
    let choices = played.filed.iter().filter(|(_, by, mv, _)| *by == SEAT && *mv == PalwHeldMoveV1::Choice).count();
    assert!(choices >= 1, "the seat's node names the lied child");
    let (_, by, _, close) = played.filed.iter().find(|(_, _, mv, _)| *mv == PalwHeldMoveV1::Close).expect("a bottom");
    assert_eq!(*by, SEAT, "filed by the seat's node");
    assert!(matches!(close, PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ExecutorGuilty, .. }));
    let end = played.state;
    assert!(
        matches!(phase_of(&end, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtHeldVerdict, .. }),
        "a held dissection's verdict (decision (B)): {:?}",
        phase_of(&end, &claim)
    );
    assert!(collateral(&end, PRODUCER) < before.0, "the liar is charged");
    assert_eq!(collateral(&end, SEAT), before.1, "the winning seat pays nothing");
}

/// **T-A10, the node half: N4 is live on a node past the fence, and dormant below it.** The registry
/// a node's services resolve through (`PalwBackendRegistry::for_node_v1`, the producer's and the
/// panel's one constructor) on testnet-12's params makes every backend held-aware
/// (`Params::palw_held_answerability_v1`). A held row this build answers (context 8,192, its compute
/// turn inside the cap) dissects and is produced; one past the answerable context (16,384) does not
/// dissect, and
/// the producer's guard (`palw_dissection_refusal_v1`, which the panel's canonical claim asks too)
/// refuses it by name. The twin — the same ruleset with `palw_offence_attribution` unset — resolves
/// both as before N4: both dissect and neither is refused. Both rows resolve through the
/// chain-registered arm, as a held class reaches a live network (ADR-0118). (The first registry a
/// process builds runs the SDK's certification drill once — about a minute in a debug build.)
#[test]
fn t_a10_n4_is_live_on_a_node_past_the_fence_and_dormant_below_it() {
    use crate::palw_backends::PalwBackendRegistry;
    use crate::palw_producer::palw_dissection_refusal_v1;
    use kaspa_consensus_core::config::params::palw_t12_shipped_params;
    let past = palw_t12_shipped_params();
    assert!(past.palw_held_answerability_v1(), "testnet-12 arms palw_offence_attribution");
    let mut below = past.clone();
    below.palw_offence_attribution = None;
    assert!(!below.palw_held_answerability_v1());
    let court = match &past.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.court,
        _ => panic!("testnet-12 is a V2 network"),
    };
    for (n_ctx, answerable) in [(8_192u32, true), (16_384, false)] {
        let (artifact, profile) = held_fixture(n_ctx);
        // A converted artifact declares its tokenizer; the chain lane refuses one that does not.
        let artifact = Arc::new((*artifact).clone().with_tokenizer_commitment(h64(0x70C)));
        let class_id = profile.shape_profile_id();
        let root = artifact.artifact_digest();
        let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, CANONICAL.0, CANONICAL.1);
        for (params, fenced) in [(&past, true), (&below, false)] {
            let registry = PalwBackendRegistry::for_node_v1(
                params,
                court,
                PalwPromptIdsFormV1::Flat,
                vec![misaka_palw_sdk::lineages::dense::holding_from_artifact(artifact.clone(), None)],
                NETWORK.to_vec(),
                true,
            );
            assert_eq!(
                registry.sdk().held_answerability_v1(),
                params.palw_held_answerability_v1(),
                "the node's registry carries the selector"
            );
            let backend = registry
                .resolve_or_chain(class_id, root, |_| Some((profile.clone(), canonical.clone())))
                .unwrap_or_else(|e| panic!("n_ctx {n_ctx}: the chain arm serves the held row: {e}"));
            assert!(backend.has_fused_site(), "a held graph-v7 row commits a fused site");
            let dissects = !fenced || answerable;
            assert_eq!(backend.supports_dissection(), dissects, "n_ctx {n_ctx}, fenced {fenced}");
            let refusal = palw_dissection_refusal_v1(backend.as_ref(), params, 0, class_id);
            assert_eq!(refusal.is_some(), !dissects, "n_ctx {n_ctx}, fenced {fenced}: {refusal:?}");
            if let Some(why) = refusal {
                assert!(why.contains("ADR-0152 §4-ter N4") && why.contains(&class_id.to_string()), "named: {why}");
            }
        }
    }
}

/// **N4 is wired where a node builds its registries, and nowhere is it built another way.** Both
/// services' `backends()` return the one node constructor; the constructor applies the turn off the
/// node's `Params`; the SDK hands it to every backend it resolves — through a lineage (`ruled_v1`)
/// and through both families of the chain-registered arm — and keeps it across `with_lineage`.
#[test]
fn t_a10_every_node_registry_answers_held_dissections_through_one_constructor() {
    let body = |source: &'static str, head: &str| -> &'static str {
        let at = source.find(head).unwrap_or_else(|| panic!("{head}"));
        &source[at..at + source[at..].find("\n    }\n").expect("its end")]
    };
    for (name, source) in [("producer", include_str!("../palw_producer.rs")), ("panel", include_str!("../palw_panel.rs"))] {
        let backends = body(source, "    fn backends(&self) -> crate::palw_backends::PalwBackendRegistry {");
        assert!(backends.contains("crate::palw_backends::PalwBackendRegistry::for_node_v1("), "{name}: the one node constructor");
        assert!(!backends.contains("PalwBackendRegistry::new"), "{name}: never a registry built without the node's rules");
    }
    let registry = include_str!("../palw_backends.rs");
    let constructor = body(registry, "    pub fn for_node_v1(");
    assert!(constructor.contains(".with_held_answerability_v1(params.palw_held_answerability_v1())"), "off the node's params");
    assert!(constructor.contains("palw_attempt_rules_of_params_v1(params)"), "beside the attempt rule");
    let sdk = include_str!("../../../misaka-palw-sdk/src/sdk.rs");
    assert!(
        body(sdk, "    fn ruled_v1(").contains("backend.set_held_answerability_v1(self.held_answerability);"),
        "every lineage's backend"
    );
    let chain_arm = body(sdk, "    pub fn resolve_chain_registered(");
    assert_eq!(chain_arm.matches(".with_held_answerability_v1(self.held_answerability)").count(), 2, "both families of the chain arm");
    assert!(
        body(sdk, "    pub fn with_lineage(").contains(".with_held_answerability_v1(held_answerability)"),
        "kept across a composed lineage"
    );
}

/// **The tabled path too: a class a lineage serves is made held-aware through the trait's setter.**
/// testnet-12's genesis rows are the build's own table rows, so a node resolves them through a
/// lineage — the SDK's `ruled_v1`, which hands the selector to the resolved backend by
/// `PalwExecutionBackendV1::set_held_answerability_v1` — not through the chain arm's builder. A
/// lineage serving the held fixture row past the answerable context: an SDK armed as testnet-12's
/// params arm it resolves a backend that declines the dissection, one unarmed (every other network)
/// resolves it as it was.
#[test]
fn t_a10_a_tabled_held_class_is_made_held_aware_through_the_lineage_door() {
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    use misaka_palw_sdk::{PalwClassEntryV1, PalwClassSdk, PalwLoadedArtifactV1, PalwModelLineageV1, PalwWeightResidencyV1};
    struct HeldRowLineage(Arc<Base0ArtifactV1>, PalwShapeProfileV3);
    impl PalwModelLineageV1 for HeldRowLineage {
        fn lineage_id(&self) -> &'static str {
            "held-row-fixture"
        }
        fn classes(&self, _court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
            Vec::new()
        }
        fn sniffs(&self, _head: &[u8; 8]) -> bool {
            false
        }
        fn load(&self, _path: &std::path::Path, _residency: PalwWeightResidencyV1) -> Result<PalwLoadedArtifactV1, String> {
            Err("the fixture loads nothing".into())
        }
        fn registered_weight_keys(&self, _artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
            Vec::new()
        }
        fn pair(
            &self,
            _court: &PalwCourtParamsV2,
            _entry: &PalwClassEntryV1,
            _artifact: &PalwLoadedArtifactV1,
        ) -> Result<Hash64, String> {
            Err("the fixture pairs nothing".into())
        }
        fn resolve(
            &self,
            _court: &PalwCourtParamsV2,
            _prompt_ids_form: PalwPromptIdsFormV1,
            class_id: Hash64,
            _artifact_root: Hash64,
            _holdings: &[PalwLoadedArtifactV1],
            _network_id: &[u8],
        ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
            (class_id == self.1.shape_profile_id()).then(|| Ok(Box::new(backend(&self.0, &self.1)) as Box<dyn PalwExecutionBackendV1>))
        }
    }
    let (artifact, profile) = held_fixture(16_384);
    let class_id = profile.shape_profile_id();
    let armed = kaspa_consensus_core::config::params::palw_t12_shipped_params().palw_held_answerability_v1();
    assert!(armed);
    for (carried, dissects) in [(armed, false), (false, true)] {
        let sdk = PalwClassSdk::with_lineages(
            vec![Arc::new(HeldRowLineage(artifact.clone(), profile.clone()))],
            PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court"),
            PalwPromptIdsFormV1::Flat,
            NETWORK.to_vec(),
        )
        .with_held_answerability_v1(carried);
        let backend = sdk.resolve(class_id, h64(0x2007), &[]).expect("the lineage serves the row");
        assert_eq!(backend.supports_dissection(), dissects, "armed {carried}");
    }
}

// ---- through the tick: the node's held route, driven block by block --------------------------------
//
// A node here is the panel's held route with its host answered by the fixture chain
// ([`FixtureHost`]): each block it begins its tick on the state the chain holds, reads the chain the
// route asks for (the objects mined so far, filtered by the panel's own predicate), makes its moves,
// starts the builds the ledger admits, lets them finish (a fixture build is milliseconds — a real one
// spans ticks, which the next `begin_tick_v1` collects either way), and offers its carrier slot
// through P2-6's scheduler (`PalwCarrierSlotsV1`, the licences' turn and one carrier in flight) with
// the priority lane in EDF order. What it carries is folded in the block of that DAA.

use super::held_court::{PalwHeldCourtV1, PalwHeldHostV1, PalwHeldMaterialV1, palw_held_moves_v1, palw_held_start_builds_v1};
use super::{
    COURT_MOVE_REPLAN_DAA, PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1, palw_court_queue_edf_v1,
    palw_held_chain_object_is_the_sessions_v1,
};
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};

/// What a fixture node's host answers.
struct FixtureHost {
    bond: PalwBondKeyV2,
    state: PalwChainStateV2,
    make: Box<dyn Fn() -> Qwen25A16Backend + Send + Sync>,
    ledger: Arc<PalwMemoryLedgerV1>,
    need: u64,
    material: HashMap<Hash64, PalwDaClaimFactsV1>,
    carried: Option<Vec<u32>>,
    fenced: bool,
    asked: std::sync::Mutex<Vec<(Hash64, bool)>>,
}

impl FixtureHost {
    fn new(bond: u64, make: impl Fn() -> Qwen25A16Backend + Send + Sync + 'static) -> Self {
        Self {
            bond: bond_key(bond),
            state: PalwChainStateV2::genesis(),
            make: Box::new(make),
            ledger: PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None),
            need: 1_000,
            material: HashMap::new(),
            carried: None,
            fenced: true,
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl PalwHeldHostV1 for FixtureHost {
    fn offence_attribution_active(&self, _daa: u64) -> bool {
        self.fenced
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
        self.bond
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
    fn backend(&self, duty: &PalwCourtDutyV2) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        self.asked.lock().unwrap().push((duty.claim_id, duty.i_am_responder));
        Ok(Box::new((self.make)()))
    }
    fn build_need_bytes(&self, _backend: &dyn PalwExecutionBackendV1, _duty: &PalwCourtDutyV2) -> u64 {
        self.need
    }
    fn responder_material(&self, duty: &PalwCourtDutyV2, _backend: &dyn PalwExecutionBackendV1) -> Result<PalwHeldMaterialV1, String> {
        let facts = self.material.get(&duty.claim_id).cloned().ok_or("no material of the claim")?;
        // Nothing kept: the capture was pruned, and P2-7's loader re-makes it from the claim's job.
        Ok(PalwHeldMaterialV1 { facts, pooled: Vec::new(), paths: Vec::new(), keep_dir: None })
    }
    fn carried_prompt(&self, _duty: &PalwCourtDutyV2, _backend: &dyn PalwExecutionBackendV1) -> Result<Option<Vec<u32>>, String> {
        Ok(self.carried.clone())
    }
    /// The panel's own read, on the fixture chain: this bond's seat rows, through the panel's filter.
    fn claim_open_until_v1(&self, claim_id: &Hash64) -> Option<u64> {
        use kaspa_consensus_core::palw_producer_v2::{PalwClaimRoleV1, palw_claim_rows_v1};
        let (rows, _) = palw_claim_rows_v1(&self.state, &params(), &self.bond, PalwClaimRoleV1::Seat, false, 4_096);
        super::held_court::palw_held_open_claims_v1(&rows, &HashSet::from([*claim_id])).get(claim_id).copied()
    }
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

/// The facts P2-7's loader re-makes an attempt claim's capture under.
fn attempt_facts(d: &Produced, claim_id: Hash64) -> PalwDaClaimFactsV1 {
    PalwDaClaimFactsV1 {
        claim_id,
        class_id: d.backend.profile().shape_profile_id(),
        executor_bond: bond_key(PRODUCER),
        execution_root: d.execution_root,
        trace_root: d.trace_root,
        work_leaves: binding_of(&d.material).step_leaf_count,
        form: d.backend.prompt_ids_form(),
        lane: PalwDaLaneV1::Attempt { anchor: d.anchor, attempt_draw: None, job: Some((d.job.clone(), d.prompt.clone())) },
        job_pin: None,
    }
}

/// A node: its host, its held route, its court queue and the scheduler's memory.
struct Node {
    host: FixtureHost,
    held: PalwHeldCourtV1,
    pending: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    due: HashMap<(Hash64, u32, bool), u64>,
    moved: HashMap<(Hash64, u32, bool), u64>,
    last_lane: Option<PalwCarrierLaneV1>,
    /// Every carrier this node sent: `(DAA, key, due)`.
    carried: Vec<(u64, (Hash64, u32, bool), Option<u64>)>,
    /// Builds running right after the last tick's start (before they settled).
    running_after_start: usize,
}

impl Node {
    fn new(host: FixtureHost) -> Self {
        Self {
            host,
            held: PalwHeldCourtV1::default(),
            pending: Vec::new(),
            due: HashMap::new(),
            moved: HashMap::new(),
            last_lane: None,
            carried: Vec::new(),
            running_after_start: 0,
        }
    }

    /// **One tick of the panel's held route, then its carrier slot**; the object it carries, if any.
    async fn tick(&mut self, state: &PalwChainStateV2, chain: &[PalwConsensusObjectV2], daa: u64) -> Option<PalwConsensusObjectV2> {
        self.host.state = state.clone();
        let duties = palw_court_duties_v2(state, &[self.host.bond]);
        self.held.begin_tick_v1(&duties, daa).await;
        let held_duties: Vec<PalwCourtDutyV2> = duties.into_iter().filter(|d| self.held.routes_v1(&self.host, d, daa)).collect();
        for read in self.held.chain_reads_v1(&self.host, &held_duties, daa) {
            let objects = chain
                .iter()
                .filter(|o| palw_held_chain_object_is_the_sessions_v1(o, &read.session_id, &read.claim_id))
                .cloned()
                .collect();
            self.held.note_chain_v1(read.session_id, objects, daa);
        }
        let (pending, moved) = (&self.pending, &self.moved);
        let busy = |key: &(Hash64, u32, bool)| {
            pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == *key)
                || moved.get(key).is_some_and(|at| daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
        };
        let tick = palw_held_moves_v1(&self.host, &mut self.held, &held_duties, daa, busy);
        for queued in tick.queued {
            self.due.insert(queued.key, queued.due);
            self.pending.push((queued.key.0, queued.key.1, queued.key.2, queued.object));
        }
        palw_held_start_builds_v1(&self.host, &mut self.held, daa);
        self.running_after_start = usize::from(self.held.any_running_v1());
        self.held.settle_v1().await;
        // P2-6's slots: the priority lane first except on the licences' turn (no licences here, so
        // the slot passes to it after the collector), one carrier in flight — EDF inside the lane.
        let mut slots = PalwCarrierSlotsV1::new(self.last_lane);
        let mut inflight = 0usize;
        let mut sent = None;
        for site in PalwCarrierSiteV1::TICK_ORDER {
            slots.at(site, inflight);
            let priority = matches!(site, PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences);
            if priority && slots.offers(site, inflight) && !self.pending.is_empty() {
                palw_court_queue_edf_v1(&mut self.pending, &self.due);
                let (a, b, c, object) = self.pending.remove(0);
                let key = (a, b, c);
                self.carried.push((daa, key, self.due.remove(&key)));
                self.moved.insert(key, daa);
                inflight += 1;
                sent = Some(object);
            }
        }
        self.last_lane = slots.finish(inflight);
        sent
    }
}

/// **T-A9 and the review's MEDIUM, through the tick: four held sessions against one producer's node
/// are all answered in time.** Four claims of the producer, each accused by the seat in one block; a
/// six-DAA court turn (so a node that built one evidence at a time — each build a tick, one carrier
/// a tick — would file its fourth root claim past its rung). The producer's node: its first tick
/// wants four builds and starts them together under the ledger (each a re-make of a pruned capture
/// and N1), its next queues four root claims (tag 57) due at their rungs, and the priority lane
/// carries them one a tick, soonest due first; every one lands by the rung its session carries and
/// opens its phase — no session is defaulted. The ledger's other face: with room for one build, one
/// starts and the rest wait for it (never failed).
#[tokio::test(flavor = "multi_thread")]
async fn t_a9_through_the_tick_four_held_sessions_against_one_producer_are_all_answered_in_time() {
    TURN.with(|turn| turn.set(6));
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let claims: Vec<Produced> =
        (0..4u64).map(|i| produce_at(backend(&artifact, &profile), &profile, false, h64(0x0A11_E1D0 + i))).collect();
    let refs: Vec<&Produced> = claims.iter().collect();
    let (s, ids) = licensed_many(&refs, &profile, root);
    let opened_at = 101 + claims.len() as u64 + 2;
    let accusations: Vec<_> = claims.iter().zip(&ids).map(|(d, id)| accusation_of(d, *id)).collect();
    let mut s = step(&s, opened_at, &accusations).expect("four held dissections open");
    let sessions: Vec<Hash64> = ids.iter().map(|id| session_of(&s, *id)).collect();
    let rung: HashMap<Hash64, u64> =
        sessions.iter().map(|sid| (*sid, duty_of(&s, PRODUCER, *sid).expect("the duty").rung_deadline_daa)).collect();
    assert!(rung.values().all(|r| *r == opened_at + 6), "every opening rung is the court's six-DAA turn: {rung:?}");

    let (a, p) = (artifact.clone(), profile.clone());
    let mut host = FixtureHost::new(PRODUCER, move || backend(&a, &p));
    for (d, id) in claims.iter().zip(&ids) {
        host.material.insert(*id, attempt_facts(d, *id));
    }
    let mut producer = Node::new(host);
    let mut chain: Vec<PalwConsensusObjectV2> = accusations.clone();
    let mut landed: HashMap<Hash64, u64> = HashMap::new();
    for daa in opened_at + 1..opened_at + 12 {
        let sent = producer.tick(&s, &chain, daa).await;
        if daa == opened_at + 1 {
            assert!(producer.running_after_start > 0, "the first tick starts builds");
            assert_eq!(producer.host.asked.lock().unwrap().len(), 4, "all four builds start in the first tick, in parallel");
        }
        let objects: Vec<_> = sent.into_iter().collect();
        for object in &objects {
            if let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { session_id, .. } = object {
                landed.insert(*session_id, daa);
            }
        }
        s = step(&s, daa, &objects).unwrap_or_else(|e| panic!("DAA {daa}: {e}"));
        chain.extend(objects);
    }
    assert_eq!(landed.len(), 4, "every session's root claim landed: {landed:?}");
    for sid in &sessions {
        assert!(landed[sid] <= rung[sid], "session {sid}: filed at {} with its rung at {}", landed[sid], rung[sid]);
        assert!(s.court_session(sid).and_then(|x| x.dissection.as_ref()).is_some(), "session {sid}: its phase is open");
    }
    for id in &ids {
        assert!(matches!(phase_of(&s, id), PalwClaimPhaseV2::ReceiptLicensed { .. }), "claim {id}: never defaulted");
    }
    // The carriers went soonest-due first (EDF) — every root claim due at the same rung, so in turn.
    let dues: Vec<u64> = producer.carried.iter().filter_map(|(_, _, due)| *due).collect();
    assert!(dues.windows(2).all(|w| w[0] <= w[1]), "the priority lane carried in due order: {dues:?}");

    // The ledger's other face: room for one build — one starts, the rest wait.
    let (a, p) = (artifact.clone(), profile.clone());
    let mut tight = FixtureHost::new(PRODUCER, move || backend(&a, &p));
    tight.ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1_500), || None);
    for (d, id) in claims.iter().zip(&ids) {
        tight.material.insert(*id, attempt_facts(d, *id));
    }
    let mut serial = Node::new(tight);
    let reopened = step(&licensed_many(&refs, &profile, root).0, opened_at, &accusations).expect("the same four sessions");
    let _ = serial.tick(&reopened, &accusations, opened_at + 1).await;
    assert_eq!(serial.host.asked.lock().unwrap().len(), 4, "every candidate asked in order");
    assert_eq!(serial.host.ledger.reserved_bytes(), 0, "every build finished and released its reservation (1 built, 3 waiting)");
    TURN.with(|turn| turn.set(20));
}

/// **The responder builds first** (the review's MEDIUM: its silence is a default, a challenger's is
/// only its own loss), then the soonest deadline. One node is the producer of one claim and the seat
/// that accused another producer's claim — whose rung comes first; the node starts the responder's
/// build before the challenger's all the same.
#[tokio::test(flavor = "multi_thread")]
async fn the_responder_builds_first_then_the_soonest_deadline() {
    use super::held_court::palw_held_evidence_key_v1;
    let (artifact, profile) = held_fixture(128);
    let (a, p) = (artifact.clone(), profile.clone());
    let host = FixtureHost::new(PRODUCER, move || backend(&a, &p));
    let mut held = PalwHeldCourtV1::default();
    // Two duties as the tick would see them: the node's challenger duty (a phase open, due at 110) and
    // its responder duty (due at 130).
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let (s, claim, sid) = opened(&produce(&artifact, &profile, false), &profile, root);
    let mut responder = duty_of(&s, PRODUCER, sid).expect("the responder's duty");
    responder.rung_deadline_daa = 130;
    let mut challenger = responder.clone();
    challenger.i_am_responder = false;
    challenger.claim_id = h64(0xC1A1);
    challenger.session_id = h64(0x5E55);
    challenger.rung_deadline_daa = 110;
    challenger.dissection = Some(
        kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectPhaseV1::open_with_arity(
            challenger.session_id,
            &kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1 {
                version: kaspa_consensus_core::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
                head: 0,
                lane_first: 0,
                lane_count: 1,
                history_positions: 8,
                claim: kaspa_consensus_core::palw_attn_dissect::PalwAttnRangeClaimV1 { max: 0, exp_sum: 20_000_000, v_acc: vec![0] },
            },
            (0, 0, 1),
            8,
            &kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(
                &[0],
                kaspa_consensus_core::palw_base0_a16::A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            ),
            kaspa_consensus_core::palw_base0_a16::A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            2,
            4,
            100,
            20,
            true,
        )
        .expect("a phase"),
    );
    let _ = claim;
    // The challenger has no filing on chain: it is asked for its backend only after the responder.
    let tick = palw_held_moves_v1(&host, &mut held, &[challenger.clone(), responder.clone()], 105, |_| false);
    assert!(tick.queued.is_empty());
    palw_held_start_builds_v1(&host, &mut held, 105);
    let asked = host.asked.lock().unwrap().clone();
    assert_eq!(asked.first(), Some(&(responder.claim_id, true)), "the responder's build is started first: {asked:?}");
    assert!(palw_held_evidence_key_v1(&responder).is_some());
    held.settle_v1().await;
}

/// **The review's HIGH, on the node: a decoy the fold refused never poisons the seat's N2, and a
/// filing N2 fails on is evicted.** The chain holds the forger's decoy — its genuine held root claim
/// with one slice sub-root swapped, refused by the fold at H3 but on an accepted carrier — BEFORE the
/// genuine one. The seat's node reads both, skips the decoy (the fold's own checks), builds N2 from
/// the genuine filing and names the lie's child at every round. A second seat whose backend's weights
/// are not the class's fails N2 on the genuine filing: the filing is evicted, and no build is started
/// from it again.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_decoy_filing_never_poisons_the_seats_node_and_a_failed_filing_is_evicted() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let liar = produce(&artifact, &profile, true);
    let (mut s, claim, sid) = opened(&liar, &profile, root);
    let accused = liar.backend.attn_site_evidence_held_v1(&liar.material, liar.leaf, None, None).expect("the liar's own evidence");
    let site = accused.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let (lying_root, lane, delta) = least_lie_root(&accused, root);
    let mut genuine = accused.root_claim_held_v1(&site, sid, 2).expect("the held root claim");
    if let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { root, .. } = &mut genuine {
        *root = lying_root.clone();
    }
    let mut decoy = genuine.clone();
    if let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { slice_sub_roots, .. } = &mut decoy {
        slice_sub_roots[0] = h64(0xDEC0);
    }
    assert!(step(&s, 105, std::slice::from_ref(&decoy)).is_err(), "the fold refuses the decoy (H3)");
    s = step(&s, 105, std::slice::from_ref(&genuine)).expect("the genuine root claim opens the phase");
    let chain = vec![decoy.clone(), genuine.clone()];
    let (a, p) = (artifact.clone(), profile.clone());
    let mut seat = Node::new(FixtureHost::new(SEAT, move || backend(&a, &p)));
    // Tick: the chain read, the N2 build from the genuine filing; the next tick has the evidence.
    let _ = seat.tick(&s, &chain, 106).await;
    let asked = seat.host.asked.lock().unwrap().clone();
    assert_eq!(asked, vec![(claim, false)], "one N2 build, from the filing that stands");
    // The liar's first round (the lie pushed into tile 0's child), then the seat's node names it.
    let phase = duty_of(&s, SEAT, sid).expect("the seat's duty").dissection.expect("the phase");
    let mut round = accused.round_v1(&site, &phase).expect("the round");
    let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
    round.children[first].v_acc[lane] += delta;
    let s2 = step(&s, 106, &[PalwConsensusObjectV2::CourtAttnDissected { session_id: sid, round, signature: vec![0xAA; 8] }])
        .expect("the liar's round folds to its own root");
    let named = seat.tick(&s2, &chain, 107).await;
    assert!(
        matches!(named, Some(PalwConsensusObjectV2::CourtAttnChildChosen { .. })),
        "the seat's node, built from the genuine filing, names the lied child: {named:?}"
    );
    assert_eq!(seat.host.asked.lock().unwrap().len(), 1, "and never rebuilt");

    // A seat whose weights are not the class's: N2 fails on the genuine filing — evicted.
    let other = {
        use misaka_palw_base0::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 64,
            vocab: 128,
            max_position: 128,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Arc::new(
            Base0ArtifactV1::derive_deterministic(shape, 0x0BAD)
                .expect("a valid shape")
                .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
                .expect("sorted"),
        )
    };
    let p = profile.clone();
    let mut wrong = Node::new(FixtureHost::new(SEAT, move || backend(&other, &p)));
    let _ = wrong.tick(&s, &chain, 106).await; // N2 starts from the genuine filing, and fails
    let _ = wrong.tick(&s, &chain, 107).await; // collected: the filing is evicted
    let _ = wrong.tick(&s, &chain, 107 + COURT_MOVE_REPLAN_DAA + 1).await; // no standing filing is left to build from
    let asked = wrong.host.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "one failed build, and the evicted filing is never built from again: {asked:?}");
}

// ---- ADR-0152 §4-ter.3 step 6, through the tick and the fold ---------------------------------------

/// **The step-6 forger**: a free-prompt run of the class with the V row layer 0 writes at prefill
/// position `q` moved in the cache (`PalwFreePromptDrillFaultV1::CacheRow`, lane 0 by `delta`) — the
/// cache-write step row committed honest, every checkpoint holding the position and every later read
/// holding the moved row. Its committed attention at `(layer 0, q)` follows the lie, so its fused leaf
/// there (head 0, the query head reading the moved kv head) is the first leaf an honest replay does
/// not reproduce, and the site's anchor — the checkpoint after `q` — holds the forged row: the
/// consistent forger whose held dissection bottoms on its own filing. Its claim rides the attempt
/// lane (the fold reads its roots; the job is carried as a free prompt's, so the seat's node replays
/// it from the carried ids).
fn forge_cache_row(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, q: u32, kind: u8, delta: i32) -> Option<Produced> {
    use kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
    };
    let backend = backend(artifact, profile);
    let ids: Vec<u32> = (0..q + 4).map(|i| (i * 37 + 11) % 128).collect();
    let job = PalwFreePromptJobV3 {
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
    let fault = PalwFreePromptDrillFaultV1::CacheRow { call: 0, layer: 0, position: q, kind, lane: 0, delta };
    let run = backend.execute_free_prompt_with_drill_fault_v2(&job, &prompt, fault).expect("the forger's run commits");
    let ctx = binding_of(&run.outcome.material).job_context;
    // The first fused leaf the lie reaches: the moved row's weight may round away at `q` itself, so
    // the committed tiles are compared with an honest run's, position by position from `q`.
    let honest_backend = self::backend(artifact, profile);
    let honest = honest_backend.execute_free_prompt(&job, &prompt).expect("the honest run");
    let tile = |b: &Qwen25A16Backend, material: &[u8], leaf: u64| {
        b.attn_site_evidence_held_v1(material, leaf, Some(&ids), None).expect("a responder's evidence").evidence.out_tile.leaf
    };
    let coordinates =
        (q..ctx.declared_prefill_tokens).map(|position| (0, position)).chain((1..ctx.exact_decode_tokens).map(|call| (call, 0)));
    let leaf = coordinates.into_iter().find_map(|(call, position)| {
        (0..profile.attn_heads as u32)
            .map(|head| fused_leaf_at(profile, &ctx, call, position, head))
            .find(|leaf| tile(&backend, &run.outcome.material, *leaf) != tile(&honest_backend, &honest.outcome.material, *leaf))
    })?;
    Some(Produced {
        backend,
        anchor: ctx.job_id,
        job: ctx,
        prompt,
        material: run.outcome.material,
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        leaf,
    })
}

/// The forger's answer to a step-6 demand of `(checkpoint, chunk)`: its own anchor chunk and path
/// (`MaterialDisclosedHeld`, the held DA court's v1 answer — this fixture's ruleset; R-core+'s
/// `MaterialDisclosedV2` carries the same carriage, and the node reads both).
fn disclosure_of(
    accused: &PalwAttnHeldEvidenceV1,
    claim_id: Hash64,
    positions: u32,
    checkpoint: u32,
    chunk: u32,
) -> PalwConsensusObjectV2 {
    use kaspa_consensus_core::palw_attn_court_v1::PalwAttnChunkOpeningV1;
    use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureCarriageV1, PalwHeldDisclosureV1, PalwHeldMissingV1};
    let anchor = accused.evidence.anchor.as_ref().expect("a held site's anchor");
    assert_eq!(anchor.anchor.leaf.checkpoint_index, checkpoint, "the demand names the filed anchor");
    let profile = &accused.evidence.binding.shape_profile;
    let siblings =
        kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(profile, positions, &anchor.chunks, chunk)
            .expect("the chunk's held path");
    PalwConsensusObjectV2::MaterialDisclosedHeld {
        disclosure: Box::new(PalwHeldDisclosureCarriageV1 {
            version: 1,
            claim: claim_id,
            missing: PalwHeldMissingV1::StateChunk { checkpoint, chunk },
            binding: accused.evidence.binding.clone(),
            disclosure: PalwHeldDisclosureV1::StateChunk {
                anchor: anchor.anchor.clone(),
                chunk: PalwAttnChunkOpeningV1 { chunk_index: chunk, chunk_bytes: anchor.chunks[chunk as usize].clone(), siblings },
            },
            signature: vec![0xAA; 8],
        }),
    }
}

/// What a step-6 play recorded: the end state and the DAA each landmark landed at.
#[derive(Default)]
struct Step6Played {
    demand: Option<(u64, u64)>,
    disclosed: Option<u64>,
    accused: Option<(u64, u64)>,
    forger_closed: Option<u64>,
    choices: usize,
}

/// **The forger against the seat's node, block by block.** The forger plays its held dissection from
/// its own evidence (its root claim, every round — a consistent forger's are the honest kernels' on
/// its forged state), closes the bottom with its acquittal when `race` (what a forger's stock node
/// does the block the bottom is reached), and answers the demand of its chunk the block after it
/// lands. The seat's node is the tick: its N2 from the filing, its choices, and step 6. Stops when
/// the claim is voided or at `until`.
#[allow(clippy::too_many_arguments)]
async fn play_step6(
    mut s: PalwChainStateV2,
    claim: Hash64,
    sid: Hash64,
    artifact_root: Hash64,
    accused: &PalwAttnHeldEvidenceV1,
    seat: &mut Node,
    chain: &mut Vec<PalwConsensusObjectV2>,
    race: bool,
    until: u64,
) -> (PalwChainStateV2, Step6Played) {
    let mut played = Step6Played::default();
    let positions = accused.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("the anchored site").site.anchor_positions;
    let mut owed: Option<(u32, u32)> = None;
    for daa in 105..until {
        if matches!(phase_of(&s, &claim), PalwClaimPhaseV2::Voided { .. }) {
            break;
        }
        let mut objects = Vec::new();
        // The forger's moves.
        if let Some(duty) = duty_of(&s, PRODUCER, sid)
            && (race || palw_held_move_of_duty_v1(&duty) != Some(PalwHeldMoveV1::Close))
            && let Some((mv, _, object)) = node_move(&s, PRODUCER, sid, accused, artifact_root, daa)
        {
            if mv == PalwHeldMoveV1::Close {
                played.forger_closed = Some(daa);
            }
            objects.push(object);
        }
        if let Some((checkpoint, chunk)) = owed.take() {
            objects.push(disclosure_of(accused, claim, positions, checkpoint, chunk));
            played.disclosed = Some(daa);
        }
        // The seat's node.
        if let Some(object) = seat.tick(&s, chain, daa).await {
            let due = seat.carried.last().and_then(|(_, _, due)| *due).expect("every step-6 item is dated");
            match &object {
                PalwConsensusObjectV2::CourtAttnChildChosen { .. } => played.choices += 1,
                PalwConsensusObjectV2::DefaultAccusedHeld { accusation } => {
                    let kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StateChunk { checkpoint, chunk } =
                        accusation.missing
                    else {
                        panic!("step 6 demands a StateChunk: {:?}", accusation.missing)
                    };
                    owed = Some((checkpoint, chunk));
                    played.demand = Some((daa, due));
                }
                PalwConsensusObjectV2::CheckpointAccused { .. } => played.accused = Some((daa, due)),
                _ => {}
            }
            objects.push(object);
        }
        s = step(&s, daa, &objects).unwrap_or_else(|e| panic!("DAA {daa}: {e}"));
        chain.extend(objects);
    }
    (s, played)
}

/// **The review's HIGH (4-ter.3 step 6), E2E through the tick and the fold: the consistent forger's
/// bottom cannot be built from its filing, so the seat's node demands the chunk and convicts on the
/// bytes the producer discloses.** The forger moved a V row in its cache and committed the row
/// honest; its held dissection at the first fused leaf the lie reaches is played from its own
/// evidence, so every round it discloses is the kernels' on its forged state. The seat's node builds
/// N2 from the filing, names the lie's child down to the bottom, finds its own sub-root of slice
/// (V, layer 0) is not the filed one, and — instead of a bottom that would acquit — files the held DA
/// court's `StateChunk` demand (dated a disclose window before the session's backstop). The forger
/// discloses its forged chunk; the seat's node finds the forged row at `q`, opens the forger's own
/// committed cache-write row there off the tick, dry-runs the checkpoint court and files
/// `CheckpointAccused` (dated by the backstop) through P2-6's carrier: the claim is voided
/// `CourtFraud`, and the producer is charged.
#[tokio::test(flavor = "multi_thread")]
async fn step6_the_seats_node_demands_the_forged_chunk_and_convicts_on_the_disclosed_row() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let canonical = binding_of(&produce(&artifact, &profile, false).material).job_context;
    let q = 20;
    let forger = [(0u8, 20_000), (0, -20_000), (1, 20_000), (1, -20_000)]
        .into_iter()
        .find_map(|(kind, delta)| forge_cache_row(&artifact, &profile, q, kind, delta))
        .expect("a cache-row lie that reaches a committed fused tile");
    let (s, claims) = licensed_many_under(&[&forger], &canonical, &profile, root);
    let claim = claims[0];
    let accusation = accusation_of(&forger, claim);
    let s = step(&s, 104, std::slice::from_ref(&accusation)).expect("the held dissection opens at the forger's fused leaf");
    let sid = session_of(&s, claim);
    let accused = forger
        .backend
        .attn_site_evidence_held_v1(&forger.material, forger.leaf, Some(&forger.ids()), None)
        .expect("the forger's own evidence (its backend replays its lie)");
    let before = collateral(&s, PRODUCER);
    let (a, p) = (artifact.clone(), profile.clone());
    let mut host = FixtureHost::new(SEAT, move || backend(&a, &p));
    host.carried = Some(forger.ids());
    let mut seat = Node::new(host);
    let mut chain = vec![accusation];
    let (end, played) = play_step6(s, claim, sid, root, &accused, &mut seat, &mut chain, false, 1_200).await;
    assert!(played.choices >= 1, "the seat's node names the lie's child");
    let (demanded, demand_due) = played.demand.expect("the seat's node demands the forged chunk (step 6)");
    assert!(demanded <= demand_due, "the demand lands by its due ({demanded} ≤ {demand_due})");
    assert!(played.disclosed.is_some(), "the forger disclosed");
    let (accused_at, accuse_due) = played.accused.expect("the seat's node files CheckpointAccused on the disclosed row");
    assert!(accused_at <= accuse_due, "the accusation lands by the session's backstop ({accused_at} ≤ {accuse_due})");
    assert!(
        matches!(phase_of(&end, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "the checkpoint court convicts: {:?}",
        phase_of(&end, &claim)
    );
    assert!(collateral(&end, PRODUCER) < before, "the forger is charged");
    assert!(end.court_session(&sid).is_none(), "the void closed the dissection");
}

/// **Step 6 outlives the session: the forger's race.** A consistent forger's own node closes the
/// bottom with its acquittal the block the bottom is reached — its bottom, built from its own forged
/// anchor, acquits it (`ChallengerDefeated`), and the session closes with the seat's demand still
/// unanswered. The claim is still licensed, and the DA court and the checkpoint court try it whether
/// a court is open or not: the seat's node, having recorded the pursuit when its last choice named
/// the bottom, reads the forger's disclosure, opens the committed row and files `CheckpointAccused`
/// on the claim — dated by the claim's own phase end — before it is final: voided `CourtFraud`, the
/// forger charged. (The seat still lost its dissection: the acquittal charged it.)
#[tokio::test(flavor = "multi_thread")]
async fn step6_a_forger_that_closes_first_is_still_convicted_after_its_session() {
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let canonical = binding_of(&produce(&artifact, &profile, false).material).job_context;
    let forger = [(0u8, 20_000), (0, -20_000), (1, 20_000), (1, -20_000)]
        .into_iter()
        .find_map(|(kind, delta)| forge_cache_row(&artifact, &profile, 20, kind, delta))
        .expect("a cache-row lie that reaches a committed fused tile");
    let (s, claims) = licensed_many_under(&[&forger], &canonical, &profile, root);
    let claim = claims[0];
    let accusation = accusation_of(&forger, claim);
    let s = step(&s, 104, std::slice::from_ref(&accusation)).expect("the held dissection opens");
    let sid = session_of(&s, claim);
    let accused = forger
        .backend
        .attn_site_evidence_held_v1(&forger.material, forger.leaf, Some(&forger.ids()), None)
        .expect("the forger's own evidence");
    let (before_p, before_s) = (collateral(&s, PRODUCER), collateral(&s, SEAT));
    let (a, p) = (artifact.clone(), profile.clone());
    let mut host = FixtureHost::new(SEAT, move || backend(&a, &p));
    host.carried = Some(forger.ids());
    let mut seat = Node::new(host);
    let mut chain = vec![accusation];
    let (end, played) = play_step6(s, claim, sid, root, &accused, &mut seat, &mut chain, true, 600).await;
    let closed = played.forger_closed.expect("the forger's node closes the bottom with its acquittal");
    assert!(
        chain.iter().any(|o| matches!(o, PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ChallengerDefeated, .. })),
        "the acquittal landed"
    );
    let (demanded, _) = played.demand.expect("the seat's node demanded the chunk");
    assert!(demanded <= closed, "the demand went with the forger's close, not after it ({demanded} ≤ {closed})");
    let (accused_at, due) = played.accused.expect("the pursuit files CheckpointAccused after the session closed");
    assert!(accused_at > closed && accused_at <= due, "after the close, by the claim's phase end ({closed} < {accused_at} ≤ {due})");
    assert!(
        matches!(phase_of(&end, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "convicted before it was final: {:?}",
        phase_of(&end, &claim)
    );
    assert!(collateral(&end, PRODUCER) < before_p, "the forger is charged");
    assert!(collateral(&end, SEAT) < before_s, "the seat lost its dissection to the acquittal");
}

/// **The review's item 11, F5 on the node: a held row whose compute turn exceeds the court's turn.**
/// A 12-layer, 8,192-context held row (one head a tile) is priced at a compute turn of 2 DAA against
/// a 1-DAA court turn. The producer's node, its capture pruned, builds N1 in the tick after the
/// session opens and files its root claim the tick after that — past the court's turn, inside the
/// compute turn (`(opened + base, opened + compute]`). Past the fence the fold stamps the opening rung
/// with the compute turn: the claim lands and the phase opens, never defaulted. The fence-off twin:
/// the rung is the court's turn and the node's held route is dormant; the session ends the block
/// after that rung (below the fence a held opening's silence is the old mercy, not C1's default —
/// the claim stands), and the very root claim the fenced node filed, at the DAA it filed it, is late.
#[tokio::test(flavor = "multi_thread")]
async fn f5_the_producers_node_files_inside_the_compute_turn_where_the_courts_turn_is_shorter() {
    TURN.with(|turn| turn.set(1));
    let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 12,
        hidden_dim: 256,
        ffn_dim: 512,
        attn_heads: 8,
        attn_kv_heads: 1,
        attn_head_dim: 32,
        vocab_size: 128,
        n_ctx: 8_192,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 32,
    };
    let (artifact, profile) = held_fixture_of(geometry);
    let compute =
        kaspa_consensus_core::palw_class_admission_v2::palw_held_compute_turn_daa_v1(&profile).expect("a held row is priced");
    let base = params().turn_deadline_daa();
    assert!(compute > base, "the row's compute turn ({compute}) exceeds the court's ({base})");
    let small = (8, 2);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let honest = produce_at(backend_with(&artifact, &profile, small), &profile, false, h64(0x0A11_E1D0));
    let mut filed: Option<(u64, PalwConsensusObjectV2)> = None;
    for fenced in [true, false] {
        FENCED.with(|f| f.set(fenced));
        let (mut s, claim, sid) = opened(&honest, &profile, root);
        let rung = duty_of(&s, PRODUCER, sid).expect("the producer's duty").rung_deadline_daa;
        assert_eq!(rung, 104 + if fenced { compute } else { base }, "fenced {fenced}: the opening rung");
        let (a, p) = (artifact.clone(), profile.clone());
        let mut host = FixtureHost::new(PRODUCER, move || backend_with(&a, &p, small));
        host.fenced = fenced;
        host.material.insert(claim, attempt_facts(&honest, claim));
        let mut node = Node::new(host);
        let (mut landed, mut ended) = (None, None);
        for daa in 105..104 + compute + 20 {
            if s.court_session(&sid).is_none_or(|session| session.dissection.is_some()) {
                ended = s.court_session(&sid).is_none().then_some(daa - 1);
                break;
            }
            if !fenced && let Some((at, object)) = filed.as_ref().filter(|(at, _)| *at == daa) {
                assert!(
                    step(&s, *at, std::slice::from_ref(object)).is_err(),
                    "below the fence the same root claim at DAA {at} is late"
                );
            }
            let objects: Vec<_> = node.tick(&s, &[], daa).await.into_iter().collect();
            for object in &objects {
                if matches!(object, PalwConsensusObjectV2::CourtAttnRootClaimedHeld { .. }) {
                    landed = Some(daa);
                    filed = Some((daa, object.clone()));
                }
            }
            s = step(&s, daa, &objects).unwrap_or_else(|e| panic!("fenced {fenced}, DAA {daa}: {e}"));
        }
        if fenced {
            let at = landed.expect("the producer's node files its root claim");
            assert!(at > 104 + base && at <= 104 + compute, "filed in (opened + {base}, opened + {compute}]: at {at}");
            assert!(s.court_session(&sid).and_then(|x| x.dissection.as_ref()).is_some(), "the phase opens");
            assert!(matches!(phase_of(&s, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }), "never defaulted");
        } else {
            assert_eq!(landed, None, "below the fence the node's held route is dormant");
            assert_eq!(ended, Some(104 + base + 1), "the session ends the block after the court's turn");
            let (at, _) = filed.as_ref().expect("the fenced run filed");
            assert!(*at > 104 + base, "the fenced node's filing DAA is past the twin's rung (and was refused there, above)");
        }
    }
    FENCED.with(|f| f.set(true));
    TURN.with(|turn| turn.set(20));
}

/// **The review's LOWs (items 7 and 8), on the producer's node.** The grace window: evidence built
/// for a session is kept, by `(claim, leaf, role)`, through ticks whose duty set does not name it —
/// the duty comes back and the root claim is filed from the same build, never rebuilt — and is
/// dropped only once no duty has named it for `PALW_HELD_EVIDENCE_GRACE_DAA_V1`. The ledger: a build
/// the ledger admits but whose resident bytes it will not hold is dropped, never held unreserved —
/// nothing is left reserved, and no root claim is filed from it.
#[tokio::test(flavor = "multi_thread")]
async fn held_evidence_outlives_a_quiet_duty_set_for_the_grace_and_is_never_held_unreserved() {
    use super::held_court::{PALW_HELD_EVIDENCE_GRACE_DAA_V1, palw_held_evidence_key_v1};
    let (artifact, profile) = held_fixture(128);
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let honest = produce(&artifact, &profile, false);
    let (quiet, _) = licensed(&honest, &profile, root);
    let (s, claim, sid) = opened(&honest, &profile, root);
    let key = palw_held_evidence_key_v1(&duty_of(&s, PRODUCER, sid).expect("the duty")).expect("a key");
    let (a, p) = (artifact.clone(), profile.clone());
    let mut host = FixtureHost::new(PRODUCER, move || backend(&a, &p));
    host.material.insert(claim, attempt_facts(&honest, claim));
    let mut node = Node::new(host);
    assert!(node.tick(&s, &[], 105).await.is_none(), "the first tick builds");
    for daa in 106..111 {
        assert!(node.tick(&quiet, &[], daa).await.is_none(), "a duty set that does not name the session");
    }
    assert!(node.held.holds_built_evidence_v1(&key), "kept through the quiet ticks");
    let filed = node.tick(&s, &[], 111).await;
    assert!(matches!(filed, Some(PalwConsensusObjectV2::CourtAttnRootClaimedHeld { .. })), "filed from the same build: {filed:?}");
    assert_eq!(node.host.asked.lock().unwrap().len(), 1, "never rebuilt");
    let _ = node.tick(&quiet, &[], 111 + PALW_HELD_EVIDENCE_GRACE_DAA_V1).await;
    assert!(node.held.holds_built_evidence_v1(&key), "still inside the grace");
    let _ = node.tick(&quiet, &[], 112 + PALW_HELD_EVIDENCE_GRACE_DAA_V1).await;
    assert!(!node.held.holds_built_evidence_v1(&key), "dropped once no duty named it for the grace");

    // The ledger admits the build (1,000 bytes) but not the evidence's resident bytes.
    let (a, p) = (artifact.clone(), profile.clone());
    let mut tight = FixtureHost::new(PRODUCER, move || backend(&a, &p));
    tight.ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1_500), || None);
    tight.material.insert(claim, attempt_facts(&honest, claim));
    let mut node = Node::new(tight);
    assert!(node.tick(&s, &[], 105).await.is_none());
    assert!(node.tick(&s, &[], 106).await.is_none(), "no root claim from an evidence the ledger would not hold");
    assert!(!node.held.holds_built_evidence_v1(&key), "dropped, not held unreserved");
    assert_eq!(node.host.ledger.reserved_bytes(), 0, "nothing left reserved");
}
