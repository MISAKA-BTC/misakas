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
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
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
    use misaka_palw_base0::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
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
    };
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
    Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), CANONICAL)
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
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("the slot");
    let coord = PalwStepCoordinateV1 { call_index: CANONICAL.1 - 1, node_slot: slot, position: 0, tile_index: 0 };
    canonical_step_leaf_index(profile, job, &coord).expect("the site's leaf")
}

fn produce(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, lie: bool) -> Produced {
    let backend = backend(artifact, profile);
    let anchor = h64(0x0A11_E1D0);
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

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 600, 1000, h64(1), 4, 1000, 10_000, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_turn_deadline_daa(20)
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
        offence_attribution_active: true,
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
    let class_id = profile.shape_profile_id();
    let binding = binding_of(&d.material);
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
                canonical: binding.job_context.clone(),
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
    let claim_id = attempt_id_v2(&env.attempt);
    let s = step_with(&s, 101, &[], Some(&env)).expect("the claim");
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let s = step(&s, 102, &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }]).expect("the panel");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    let s = step(&s, 103, &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: vec![valid(SEAT), valid(COLLUDER)] }])
        .expect("the licence");
    (s, claim_id)
}

/// `SEAT`'s one-move accusation at the fused leaf, folded at DAA 104: the executor's own leaf
/// evidence, stripped of its history — the object the bound verdict defers to the held dissection.
fn opened(d: &Produced, profile: &PalwShapeProfileV3, artifact_root: Hash64) -> (PalwChainStateV2, Hash64, Hash64) {
    let (s, claim_id) = licensed(d, profile, artifact_root);
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
    let s = step(&s, 104, &[PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }]).expect("the accusation");
    let sid = s
        .court_sessions_iter()
        .find(|(_, x)| x.claim == claim_id && x.challenger_bond == bond_key(SEAT))
        .map(|(k, _)| *k)
        .expect("the held dissection opened");
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

/// **A seat's node builds its evidence** once the accused's filing is on chain — read back off the
/// held root claim object (what `attn_held_filings_from_chain_v1` returns), kept only if it stands for
/// the duty's claim and leaf, and N2 from it and one honest replay.
fn challenger_evidence(
    s: &PalwChainStateV2,
    sid: Hash64,
    seat: &Qwen25A16Backend,
    filed: &PalwConsensusObjectV2,
) -> PalwAttnHeldEvidenceV1 {
    let duty = duty_of(s, SEAT, sid).expect("the seat's duty");
    let (at, filing) = PalwAttnHeldFilingV1::from_object_v1(filed).expect("the held root claim carries its filing");
    assert_eq!(at, sid);
    let filing = palw_held_filing_of_duty_v1(vec![filing], &duty, LADDER).expect("the filing stands for the claim at its leaf");
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
/// panel's one constructor) on testnet-12's params hands every backend the court's turn
/// (`Params::palw_held_answer_turn_v1`). A held row this build answers inside that turn (context
/// 8,192) dissects and is produced; one past the answerable context (16,384) does not dissect, and
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
    assert!(past.palw_held_answer_turn_v1().is_some(), "testnet-12 arms palw_offence_attribution");
    let mut below = past.clone();
    below.palw_offence_attribution = None;
    assert_eq!(below.palw_held_answer_turn_v1(), None);
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
                params.palw_held_answer_turn_v1(),
                "the node's registry carries the turn"
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
    assert!(constructor.contains(".with_held_answerability_v1(params.palw_held_answer_turn_v1())"), "the turn off the node's params");
    assert!(constructor.contains("palw_attempt_rules_of_params_v1(params)"), "beside the attempt rule");
    let sdk = include_str!("../../../misaka-palw-sdk/src/sdk.rs");
    assert!(
        body(sdk, "    fn ruled_v1(").contains("backend.set_held_answerability_v1(self.held_answer_turn);"),
        "every lineage's backend"
    );
    let chain_arm = body(sdk, "    pub fn resolve_chain_registered(");
    assert_eq!(chain_arm.matches(".with_held_answerability_v1(self.held_answer_turn)").count(), 2, "both families of the chain arm");
    assert!(
        body(sdk, "    pub fn with_lineage(").contains(".with_held_answerability_v1(held_answer_turn)"),
        "kept across a composed lineage"
    );
}

/// **The tabled path too: a class a lineage serves gets the turn through the trait's setter.**
/// testnet-12's genesis rows are the build's own table rows, so a node resolves them through a
/// lineage — the SDK's `ruled_v1`, which hands the turn to the resolved backend by
/// `PalwExecutionBackendV1::set_held_answerability_v1` — not through the chain arm's builder. A
/// lineage serving the held fixture row past the answerable context: an SDK carrying testnet-12's
/// turn resolves a backend that declines the dissection, one carrying none (every other network)
/// resolves it as it was.
#[test]
fn t_a10_a_tabled_held_class_takes_the_turn_through_the_lineage_door() {
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
    let turn = kaspa_consensus_core::config::params::palw_t12_shipped_params().palw_held_answer_turn_v1();
    assert!(turn.is_some());
    for (carried, dissects) in [(turn, false), (None, true)] {
        let sdk = PalwClassSdk::with_lineages(
            vec![Arc::new(HeldRowLineage(artifact.clone(), profile.clone()))],
            PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court"),
            PalwPromptIdsFormV1::Flat,
            NETWORK.to_vec(),
        )
        .with_held_answerability_v1(carried);
        let backend = sdk.resolve(class_id, h64(0x2007), &[]).expect("the lineage serves the row");
        assert_eq!(backend.supports_dissection(), dissects, "turn {carried:?}");
    }
}
