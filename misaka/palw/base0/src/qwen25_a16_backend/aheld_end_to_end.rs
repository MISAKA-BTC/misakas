//! **`qwen25_a16_backend`'s `aheld_end_to_end` module**, moved out of `qwen25_a16_backend.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::aheld_windowed_builder::{PAST_THE_CAP, backend, binding_of, held_fixture, job_for};
use super::*;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_attn_court_v1::{
    PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCourtError, PalwAttnCourtVerdictV1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
    check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
};
use kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1;
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnAccusedFilingV1, PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
use kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_state_layout_v4, palw_state_slice_sub_roots_v4};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2,
    PalwCourtVerdictV2, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_operator_id_v2,
    revert_delta_v2,
};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const PRODUCER: u64 = 1;
const SEAT: u64 = 2;
/// The producer's own Sybil: bonded, on no panel of the claim — the decoy's challenger.
const BYSTANDER: u64 = 3;
const COLLUDER: u64 = 4;
const PREFILL: u32 = 40;
const DECODE: u32 = 3;
/// The followed lie's sizes, smallest first: at this fixture's widths a fused output moved by
/// `2^10` or less is absorbed by the output projection's requant and reaches no later row
/// (measured at layer 0: the checkpoint root moves from `2^13`), and a lie nothing downstream
/// reads is `follow = false` by another name — so the consistent forger takes the first size
/// whose execution commits a different anchor ([`following`]). The unfollowed lie moves the
/// committed code by 64, inside the A16 range.
const FOLLOW_DELTAS: [i32; 4] = [8_192, 16_384, 30_000, -30_000];
const TILE_DELTA: i32 = 64;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

// ---- the court's kernels ---------------------------------------------------------------

/// **The dispute as the court plays it**: the accused's root (its evidence's own, or — when that
/// does not finalize to the tile it committed — the least lie that does, as a forger must file),
/// its rounds (the lie pushed into the child that holds tile 0, the only place the fold admits
/// it), and the challenger's choice (the child its own recompute names, else the one covering
/// `target`). Returns the narrowed phase and whether the accused had to lie.
struct Played {
    phase: PalwAttnDissectPhaseV1,
    /// `(lane, delta)` of the accused's lie in `v_acc`, when it lied.
    lie: Option<(usize, i64)>,
}

fn session() -> Hash64 {
    h64(0xE2E0)
}

/// The least lie in one lane of `V*` that finalizes to `committed`, or `None` when the honest root
/// already does.
fn least_lie(
    honest: &PalwAttnRootClaimV1,
    committed: &[i32],
    values: kaspa_consensus_core::palw_base0_a16::A16QuantParams,
) -> Option<(PalwAttnRootClaimV1, usize, i64)> {
    let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
    let lane = (0..committed.len()).find(|l| finalize(&honest.claim.v_acc)[*l] != committed[*l])?;
    let at = |delta: i64| {
        let mut v = honest.claim.v_acc.clone();
        v[lane] += delta;
        finalize(&v)[lane]
    };
    let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if at(mid) < committed[lane] { lo = mid + 1 } else { hi = mid }
    }
    let mut lying = honest.clone();
    lying.claim.v_acc[lane] += lo;
    assert_eq!(finalize(&lying.claim.v_acc), committed, "the least lie finalizes to the committed tile");
    Some((lying, lane, lo))
}

fn play(accused: &PalwAttnHeldEvidenceV1, challenger: &PalwAttnHeldEvidenceV1, artifact_root: Hash64, target: u64) -> Played {
    let site = accused.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("the site derives");
    let values = site.site.params.values;
    let committed =
        palw_attn_opened_lanes_v1(&accused.evidence.out_tile, &site.binding, site.head_lanes.2 as usize).expect("the tile opens");
    let honest = accused.evidence.root_claim_v1(&site).expect("the root computes");
    let (root, lie) = match least_lie(&honest, &committed, values) {
        None => (honest, None),
        Some((lying, lane, delta)) => (lying, Some((lane, delta))),
    };
    let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
        session(),
        &root,
        site.head_lanes,
        site.history_positions,
        &committed,
        values,
        2,
        site.tile_positions,
        0,
        10,
        true,
    )
    .expect("the root finalizes to the committed tile");
    let mut daa = 1;
    while phase.turn() != PalwBisectTurnV1::Terminal {
        let mut round = accused.round_v1(&site, &phase).expect("a round computes");
        if let Some((lane, delta)) = lie {
            let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
            round.children[first].v_acc[lane] += delta;
        }
        phase.apply_round(&round, daa, 10).expect("the accused's disclosure folds to its own root");
        let named = challenger.divergent_child_v1(&site, &phase).expect("the challenger recomputes");
        assert_eq!(named.is_some(), lie.is_some(), "the challenger names a child exactly when the accused lied");
        let child = named
            .unwrap_or_else(|| phase.child_ranges().iter().position(|&(f, c)| target >= f && target < f + c).expect("covered") as u8);
        let choice =
            PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: session(), round: phase.round(), child };
        phase.apply_choice(&choice, daa + 1, 10).expect("a legal choice");
        daa += 2;
    }
    Played { phase, lie }
}

/// The challenger's evidence with its OWN sub-roots in place of the filed ones — the bottom it
/// could build without the held root claim (the anchored form's).
fn with_own_top(challenger: &PalwAttnHeldEvidenceV1, artifact_root: Hash64) -> PalwAttnHeldEvidenceV1 {
    let site = challenger.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("site");
    let layout = palw_state_layout_v4(&challenger.evidence.binding.shape_profile, site.site.anchor_positions).expect("layout");
    let anchor = challenger.evidence.anchor.as_ref().expect("anchored");
    let own = palw_state_slice_sub_roots_v4(&layout, &anchor.chunk_hashes).expect("sub-roots");
    PalwAttnHeldEvidenceV1 { evidence: challenger.evidence.clone(), slice_sub_roots: own }
}

fn bottom_verdict(
    challenger: &PalwAttnHeldEvidenceV1,
    played: &Played,
    artifact_root: Hash64,
) -> Result<PalwAttnCourtVerdictV1, PalwAttnCourtError> {
    let anchored = challenger.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("the anchored site");
    let bottom = challenger.bottom_v1(&anchored, &played.phase).expect("the bottom builds");
    check_attn_dissect_bottom_v1(&played.phase, &bottom, &anchored.binding, &anchored.site, true)
}

/// A drilled run of the fixture's job, its material, and the fused leaf of `layer` at `call`,
/// head 0 — the site of the drill's lie, `lane` 1 of the head moved by `delta`.
struct Drilled {
    backend: Qwen25A16Backend,
    run: kaspa_consensus_core::palw_backend::PalwFpRunV1,
    leaf: u64,
}

fn drilled(
    artifact: &std::sync::Arc<Base0ArtifactV1>,
    profile: &PalwShapeProfileV3,
    layer: u16,
    call: u32,
    lie: Option<(i32, bool)>,
) -> Drilled {
    drilled_at(artifact, profile, layer, call, (0, 1), lie)
}

/// [`drilled`] at head `site.0`, lane `site.1` of it.
fn drilled_at(
    artifact: &std::sync::Arc<Base0ArtifactV1>,
    profile: &PalwShapeProfileV3,
    layer: u16,
    call: u32,
    site: (u16, u16),
    lie: Option<(i32, bool)>,
) -> Drilled {
    let producer = backend(artifact, profile, PAST_THE_CAP);
    let (job, prompt, _) = job_for(&producer, profile, PREFILL, DECODE);
    let run = match lie {
        None => producer.execute_free_prompt(&job, &prompt).expect("the honest run"),
        Some((delta, follow)) => producer
            .execute_free_prompt_with_drill_fault_v2(
                &job,
                &prompt,
                PalwFreePromptDrillFaultV1::AttnOutput { call, layer, position: 0, head: site.0, lane: site.1, delta, follow },
            )
            .expect("the drilled run commits"),
    };
    let binding = binding_of(&run.outcome.material);
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, layer, fused).expect("the slot");
    // One head per tile at this geometry: head `h`'s output is tile `h`.
    let coord = PalwStepCoordinateV1 { call_index: call, node_slot: slot, position: 0, tile_index: u32::from(site.0) };
    let leaf = canonical_step_leaf_index(profile, &binding.job_context, &coord).expect("the site's leaf");
    Drilled { backend: producer, run, leaf }
}

/// **The consistent forger at `layer`**: the first of [`FOLLOW_DELTAS`] whose followed lie reaches
/// the checkpoint leg — its execution's checkpoint root is not the honest one.
fn following(
    artifact: &std::sync::Arc<Base0ArtifactV1>,
    profile: &PalwShapeProfileV3,
    layer: u16,
    call: u32,
    honest: &Drilled,
) -> Drilled {
    let honest_root = binding_of(&honest.run.outcome.material).checkpoint_merkle_root;
    let mut tried = 0;
    for head in 0..profile.attn_heads {
        for lane in 0..profile.attn_head_dim as u16 {
            for delta in FOLLOW_DELTAS {
                tried += 1;
                let liar = drilled_at(artifact, profile, layer, call, (head, lane), Some((delta, true)));
                if binding_of(&liar.run.outcome.material).checkpoint_merkle_root != honest_root {
                    eprintln!("layer {layer}: head {head} lane {lane} delta {delta} reaches the anchor (try {tried})");
                    return liar;
                }
            }
        }
    }
    panic!("layer {layer}: no followed lie of one lane reaches the anchor ({tried} tried)")
}

/// **T-A3: the consistent forger at layers {0, mid, L−2}.** On a five-layer held row, at the
/// last decode call:
/// * `follow = true` — the anchor's slices after layer ℓ hold rows the lie fed, (K|V, ℓ) does not;
///   the dense filing route (the anchored form's) refuses to build a bottom; the windowed
///   challenger's bottom with its OWN top path does not reach the anchor (`ChunkNotInCheckpoint`:
///   the acquittal-by-construction F9 named); with the FILED sub-roots it convicts;
/// * `follow = false` — the anchor is honest, and the challenger's bottom convicts on either top;
/// * the honest responder is acquitted at every tile.
#[test]
fn t_a3_the_consistent_forger_is_convicted_only_through_the_filed_sub_roots_at_every_depth() {
    let (artifact, profile) = held_fixture(5, 128);
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let challenger = backend(&artifact, &profile, PAST_THE_CAP);
    let wide = backend(&artifact, &profile, super::aheld_windowed_builder::DENSE_NETWORK);
    let (_, _, ids) = job_for(&challenger, &profile, PREFILL, DECODE);
    let call = DECODE - 1;
    for layer in [0u16, 2, 3] {
        let honest = drilled(&artifact, &profile, layer, call, None);
        let responder = honest
            .backend
            .attn_site_evidence_held_v1(&honest.run.outcome.material, honest.leaf, Some(&ids), None)
            .expect("the held site's evidence");
        let site = responder.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
        let tiles = u64::from(site.history_positions).div_ceil(u64::from(site.tile_positions));
        assert!(tiles >= 3, "layer {layer}: the history plays rounds");
        let filing = PalwAttnHeldFilingV1 {
            binding: responder.evidence.binding.clone(),
            out_tile: responder.evidence.out_tile.clone(),
            anchor: responder.evidence.anchor.as_ref().expect("anchored").anchor.clone(),
            slice_sub_roots: responder.slice_sub_roots.clone(),
        };
        let own = challenger.attn_site_evidence_held_v1(&[], honest.leaf, Some(&ids), Some(&filing)).expect("challenger");
        for target in 0..tiles {
            let played = play(&responder, &own, root, target);
            assert!(played.lie.is_none());
            assert_eq!(
                bottom_verdict(&own, &played, root),
                Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
                "layer {layer} tile {target}"
            );
        }

        for follow in [true, false] {
            let liar = if follow {
                following(&artifact, &profile, layer, call, &honest)
            } else {
                drilled(&artifact, &profile, layer, call, Some((TILE_DELTA, false)))
            };
            assert_eq!(liar.leaf, honest.leaf);
            let accused = liar
                .backend
                .attn_site_evidence_held_v1(&liar.run.outcome.material, liar.leaf, Some(&ids), None)
                .expect("the liar's own");
            let filing = PalwAttnHeldFilingV1 {
                binding: accused.evidence.binding.clone(),
                out_tile: accused.evidence.out_tile.clone(),
                anchor: accused.evidence.anchor.as_ref().expect("anchored").anchor.clone(),
                slice_sub_roots: accused.slice_sub_roots.clone(),
            };
            let ch = challenger
                .attn_site_evidence_held_v1(&[], liar.leaf, Some(&ids), Some(&filing))
                .expect("the honest challenger builds from the liar's filing");
            let anchored = ch.site_v1(root, true, PALW_HELD_STEP_LADDER_V1).expect("site");
            assert_eq!(ch.fallback_v1(&anchored), Ok(None), "layer {layer}: slice (K|V, ℓ) precedes the lie");
            let own_top = with_own_top(&ch, root);
            let differs: Vec<bool> = own_top.slice_sub_roots.iter().zip(&filing.slice_sub_roots).map(|(a, b)| a != b).collect();
            assert_eq!(differs.iter().any(|d| *d), follow, "layer {layer} follow {follow}: the anchor follows the lie or not");
            let played = play(&accused, &ch, root, 0);
            assert!(played.lie.is_some(), "layer {layer}: the forger must lie in its root");
            assert_eq!(
                bottom_verdict(&ch, &played, root),
                Ok(PalwAttnCourtVerdictV1::ExecutorGuilty),
                "layer {layer} follow {follow}: through the filed sub-roots the forger is convicted"
            );
            let without = bottom_verdict(&own_top, &played, root);
            if follow {
                assert!(
                    matches!(without, Err(PalwAttnCourtError::ChunkNotInCheckpoint { .. })),
                    "layer {layer}: without the filed sub-roots the challenger's path does not reach the anchor: {without:?}"
                );
                let dense = wide
                    .attn_site_evidence_from_filing(
                        &PalwAttnAccusedFilingV1 {
                            binding: filing.binding.clone(),
                            out_tile: filing.out_tile.clone(),
                            anchor: Some(filing.anchor.clone()),
                        },
                        liar.leaf,
                        Some(&ids),
                    )
                    .expect_err("the anchored form's filing route cannot build this bottom");
                assert!(dense.contains("does not root to the filed checkpoint"), "layer {layer}: {dense}");
            } else {
                assert_eq!(without, Ok(PalwAttnCourtVerdictV1::ExecutorGuilty), "layer {layer}: an unfollowed lie either way");
            }
        }
    }
}

// ---- the fold ----------------------------------------------------------------------------

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

/// testnet-12's launch line as far as the fold reads it.
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

fn below() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { offence_attribution_active: false, ..launch() }
}

/// One block through the real transition, checked: consistency, the delta re-applies and reverts,
/// and the carriage reloads under its root.
fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
    extras: &PalwTransitionExtrasV1,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let c = PalwBlockContextV2 { block: Hash64::from_u64_word(0xE2E_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 10_000 };
    let (child, delta) = apply_palw_transition_v2_with_extras(parent, p, &c, objects, att, false, false, false, true, extras)?;
    child.assert_internal_consistency(p).expect("internal consistency");
    child.assert_deadline_consistency(p).expect("deadline consistency");
    assert_eq!(apply_delta_v2(parent, &delta, p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, p).expect("reverts"), *parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(p, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    Ok(child)
}

/// The drilled run's claim on the chain, licensed by both seats, and a held dissection opened by
/// `SEAT` at the drill's leaf with the one-move accusation the executor's fold evidence makes.
fn opened(
    d: &Drilled,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    ids: &[u32],
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, Hash64, Hash64) {
    let (s, claim_id) = licensed(d, profile, artifact_root, extras);
    accuse(&s, d, claim_id, SEAT, d.leaf, ids, 104, extras).map(|(s, sid)| (s, claim_id, sid)).expect("the held dissection opens")
}

/// The drilled run's claim, licensed at DAA 103: the floor and the held class (through the
/// carriage), the bonds, the attempt committing the run's roots, the panel of two seats and both
/// seats' `Valid`.
fn licensed(
    d: &Drilled,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, Hash64) {
    let p = params();
    let class_id = profile.shape_profile_id();
    let binding = binding_of(&d.run.outcome.material);
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
        bond(BYSTANDER),
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
    let s = step(&PalwChainStateV2::genesis(), &p, 100, &register, None, extras).expect("the registry");
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
            operator_id: palw_operator_id_v2(&op_key(21)),
            artifact_root,
            trace_root: d.run.outcome.trace_root,
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: d.run.outcome.execution_root,
        },
        signature: vec![0; 8],
    };
    let claim_id = attempt_id_v2(&env.attempt);
    let s = step(&s, &p, 101, &[], Some(&env), extras).expect("the claim");
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let s = step(&s, &p, 102, &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None, extras)
        .expect("the panel");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    let s = step(
        &s,
        &p,
        103,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: vec![valid(SEAT), valid(COLLUDER)] }],
        None,
        extras,
    )
    .expect("the licence");
    (s, claim_id)
}

/// `accuser`'s one-move accusation at the fused `leaf` folded at `daa`: the executor's fold
/// evidence, stripped of its history — the object the bound verdict defers to the dissection.
/// The session it opened, or the fold's refusal.
#[allow(clippy::too_many_arguments)]
fn accuse(
    s: &PalwChainStateV2,
    d: &Drilled,
    claim_id: Hash64,
    accuser: u64,
    leaf: u64,
    ids: &[u32],
    daa: u64,
    extras: &PalwTransitionExtrasV1,
) -> Result<(PalwChainStateV2, Hash64), PalwStateV2Error> {
    let p = params();
    let binding = binding_of(&d.run.outcome.material);
    let roots = PalwClaimRootsV1 {
        execution_root: d.run.outcome.execution_root,
        trace_root: d.run.outcome.trace_root,
        anchor: binding.job_context.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        &d.backend,
        &d.run.outcome.material,
        ids,
        roots,
        binding.step_leaf_count,
        leaf,
        d.backend.prompt_ids_form(),
    )
    .expect("the executor's own evidence at the leaf")
    .for_the_one_move_v2();
    let mut accusation =
        evidence.into_accusation_v1(claim_id, roots.execution_root, roots.trace_root, bond_key(PRODUCER), bond_key(accuser));
    accusation.signature = vec![9; 8];
    let s = step(s, &p, daa, &[PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }], None, extras)?;
    let sid = s
        .court_sessions_iter()
        .find(|(_, x)| x.claim == claim_id && x.challenger_bond == bond_key(accuser))
        .map(|(k, _)| *k)
        .expect("a session on the claim");
    Ok((s, sid))
}

/// The responder's move 1: the held form past the fence (the builder's object), the anchored form
/// below it (the same fields, no sub-roots) — `root` in place of the evidence's own when it lied.
fn move_one(
    responder: &PalwAttnHeldEvidenceV1,
    artifact_root: Hash64,
    sid: Hash64,
    root: &PalwAttnRootClaimV1,
    held: bool,
) -> PalwConsensusObjectV2 {
    let site = responder.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { binding, out_tile, anchor, slice_sub_roots, operand_openings, .. } =
        responder.root_claim_held_v1(&site, sid, 2).expect("the held root claim")
    else {
        unreachable!("the builder files the held form")
    };
    let (session_id, root, arity, signature) = (sid, root.clone(), 2, vec![0xAA; 8]);
    if held {
        PalwConsensusObjectV2::CourtAttnRootClaimedHeld {
            session_id,
            root,
            arity,
            binding,
            out_tile,
            anchor,
            slice_sub_roots,
            operand_openings,
            signature,
        }
    } else {
        PalwConsensusObjectV2::CourtAttnRootClaimedAnchored {
            session_id,
            root,
            arity,
            binding,
            out_tile,
            anchor,
            operand_openings,
            signature,
        }
    }
}

fn adjudicate(state: &PalwChainStateV2, sid: Hash64, proof: &PalwCourtVerdictProofV2) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 20, 2).unwrap();
    kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
        state,
        &sid,
        proof,
        &court,
        1 << 26,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        true,
        // ADR-0152 v3.1 §4-bis.9's decode-close door (`palw_offence_attribution`): read by a
        // decode-token close only, never by an attention bottom.
        true,
        None,
    )
}

fn collateral(s: &PalwChainStateV2, n: u64) -> u64 {
    s.bond(&bond_key(n)).expect("the bond").collateral
}

fn phase_of(s: &PalwChainStateV2, sid: &Hash64) -> PalwAttnDissectPhaseV1 {
    s.court_session(sid).and_then(|x| x.dissection.clone()).expect("the phase is open")
}

/// The rounds and choices through the fold, as [`play`] plays them against the kernels: the
/// accused discloses (its lie pushed into tile 0's child), the challenger names the divergent
/// child or takes the one covering `target`. Returns the state at `Terminal` and the next DAA.
#[allow(clippy::too_many_arguments)]
fn rounds_through_the_fold(
    mut state: PalwChainStateV2,
    p: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    sid: Hash64,
    accused: &PalwAttnHeldEvidenceV1,
    challenger: &PalwAttnHeldEvidenceV1,
    artifact_root: Hash64,
    lie: Option<(usize, i64)>,
    target: u64,
    mut daa: u64,
) -> (PalwChainStateV2, u64) {
    let site = accused.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let mut guard = 0;
    while phase_of(&state, &sid).turn() != PalwBisectTurnV1::Terminal {
        let phase = phase_of(&state, &sid);
        let mut round = accused.round_v1(&site, &phase).expect("a round computes");
        if let Some((lane, delta)) = lie {
            let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
            round.children[first].v_acc[lane] += delta;
        }
        state = step(
            &state,
            p,
            daa,
            &[PalwConsensusObjectV2::CourtAttnDissected { session_id: sid, round, signature: vec![0xAA; 8] }],
            None,
            extras,
        )
        .expect("the round folds");
        let phase = phase_of(&state, &sid);
        let named = challenger.divergent_child_v1(&site, &phase).expect("the challenger recomputes");
        assert_eq!(named.is_some(), lie.is_some(), "a child is named exactly when the accused lied");
        let child = named
            .unwrap_or_else(|| phase.child_ranges().iter().position(|&(f, c)| target >= f && target < f + c).expect("covered") as u8);
        let choice =
            PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: sid, round: phase.round(), child };
        state = step(
            &state,
            p,
            daa + 1,
            &[PalwConsensusObjectV2::CourtAttnChildChosen { session_id: sid, choice, signature: vec![0xBB; 8] }],
            None,
            extras,
        )
        .expect("the choice folds");
        daa += 2;
        guard += 1;
        assert!(guard < 32, "the dissection did not narrow");
    }
    (state, daa)
}

fn proof_of(
    challenger: &PalwAttnHeldEvidenceV1,
    state: &PalwChainStateV2,
    sid: Hash64,
    artifact_root: Hash64,
) -> PalwCourtVerdictProofV2 {
    let anchored = challenger.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("the anchored site");
    let bottom = challenger.bottom_v1(&anchored, &phase_of(state, &sid)).expect("the bottom builds");
    PalwCourtVerdictProofV2::AttnDissection {
        binding: Box::new(challenger.evidence.binding.clone()),
        bottom: Box::new(bottom),
        operand_openings: challenger.evidence.operand_openings.clone(),
    }
}

/// **The held happy path, end to end, on a held fixture row**: an honest executor's claim,
/// licensed; a seat's one-move accusation at a fused leaf (the executor's own fold evidence,
/// stripped) opens the held dissection; the executor files the held root claim its windowed
/// builder makes; the challenger reads the filing off that object, builds its own evidence from
/// one replay, and plays every round against the executor's disclosures, naming nothing; its
/// bottom acquits; the `ChallengerDefeated` close charges the challenger and the claim stands and
/// goes on to `Final`. Every block re-applied, reverted and reloaded. The fence-off twin: below
/// `palw_offence_attribution` the held form is refused and the anchored form plays the same
/// dispute to the same acquittal; past it the anchored form is refused.
#[test]
fn the_held_happy_path_end_to_end_through_the_fold() {
    let (artifact, profile) = held_fixture(2, 128);
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let challenger = backend(&artifact, &profile, PAST_THE_CAP);
    let (_, _, ids) = job_for(&challenger, &profile, PREFILL, DECODE);
    let honest = drilled(&artifact, &profile, 0, DECODE - 1, None);
    let p = params();
    for (extras, held) in [(launch(), true), (below(), false)] {
        let (s, claim, sid) = opened(&honest, &profile, root, &ids, &extras);
        let responder = honest
            .backend
            .attn_site_evidence_held_v1(&honest.run.outcome.material, honest.leaf, Some(&ids), None)
            .expect("the executor's windowed evidence");
        let site = responder.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
        let own_root = responder.evidence.root_claim_v1(&site).expect("the root");
        let refused =
            step(&s, &p, 105, &[move_one(&responder, root, sid, &own_root, !held)], None, &extras).expect_err("the other form");
        assert!(matches!(refused, PalwStateV2Error::HeldRootClaimRefused { .. }), "held {held}: {refused:?}");
        let object = move_one(&responder, root, sid, &own_root, held);
        let s = step(&s, &p, 105, std::slice::from_ref(&object), None, &extras).expect("move 1 folds");
        let filing = match PalwAttnHeldFilingV1::from_object_v1(&object) {
            Some((at, filing)) => {
                assert_eq!(at, sid);
                filing
            }
            // Below the fence the chain carries no sub-roots: the challenger's own stand in.
            None => PalwAttnHeldFilingV1 {
                binding: responder.evidence.binding.clone(),
                out_tile: responder.evidence.out_tile.clone(),
                anchor: responder.evidence.anchor.as_ref().expect("anchored").anchor.clone(),
                slice_sub_roots: responder.slice_sub_roots.clone(),
            },
        };
        let ch = challenger.attn_site_evidence_held_v1(&[], honest.leaf, Some(&ids), Some(&filing)).expect("the challenger");
        assert_eq!(with_own_top(&ch, root), ch, "an honest execution's sub-roots are the challenger's own");
        let tiles = u64::from(site.history_positions).div_ceil(u64::from(site.tile_positions));
        let (s, daa) = rounds_through_the_fold(s, &p, &extras, sid, &responder, &ch, root, None, tiles - 1, 106);
        let proof = proof_of(&ch, &s, sid, root);
        assert_eq!(adjudicate(&s, sid, &proof), Ok(PalwCourtVerdictV2::ChallengerDefeated), "held {held}: acquitted");
        let end = step(
            &s,
            &p,
            daa,
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof }],
            None,
            &extras,
        )
        .expect("the close folds");
        assert!(collateral(&end, SEAT) < collateral(&s, SEAT), "held {held}: the losing challenger pays");
        assert_eq!(collateral(&end, PRODUCER), collateral(&s, PRODUCER), "held {held}: the honest executor pays nothing");
        assert!(matches!(end.claim(&claim).expect("claim").phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        let mut state = end;
        for at in daa + 1..daa + 1_000 {
            state = step(&state, &p, at, &[], None, &extras).unwrap_or_else(|e| panic!("DAA {at}: {e}"));
            if matches!(state.claim(&claim).expect("claim").phase, PalwClaimPhaseV2::Final { .. }) {
                break;
            }
        }
        assert!(
            matches!(state.claim(&claim).expect("claim").phase, PalwClaimPhaseV2::Final { .. }),
            "held {held}: the acquitted claim goes on to Final"
        );
    }
}

/// **F9 end to end through the fold**: the consistent forger (the N5 drill, `follow = true`, at
/// layer 0 of the last decode call) files the held root claim its own fold builds, with the least
/// lie in its root; the honest challenger reads the filing off the object, names the lie's child
/// at every round, and bottoms — with the FILED sub-roots the close convicts and the claim is
/// voided `CourtHeldVerdict` (the review's F3, decision (B): the producer charged as a fraud, no
/// kind-3 basis); with its own top path the close does not adjudicate. The fence-off twin:
/// below `palw_offence_attribution` the forger files the anchored form and the challenger's only
/// bottom does not reach its anchor — the escape C2 closes.
#[test]
fn the_consistent_forger_end_to_end_is_voided_court_fraud_only_through_the_filed_sub_roots() {
    let (artifact, profile) = held_fixture(2, 128);
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let challenger = backend(&artifact, &profile, PAST_THE_CAP);
    let (_, _, ids) = job_for(&challenger, &profile, PREFILL, DECODE);
    let honest = drilled(&artifact, &profile, 0, DECODE - 1, None);
    let liar = following(&artifact, &profile, 0, DECODE - 1, &honest);
    let p = params();
    let accused = liar
        .backend
        .attn_site_evidence_held_v1(&liar.run.outcome.material, liar.leaf, Some(&ids), None)
        .expect("the forger's own windowed evidence");
    let site = accused.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let committed = palw_attn_opened_lanes_v1(&accused.evidence.out_tile, &site.binding, site.head_lanes.2 as usize).expect("tile");
    let honest_root = accused.evidence.root_claim_v1(&site).expect("root");
    let (lying_root, lane, delta) = least_lie(&honest_root, &committed, site.site.params.values).expect("the forger must lie");

    for (extras, held) in [(launch(), true), (below(), false)] {
        let (s, claim, sid) = opened(&liar, &profile, root, &ids, &extras);
        let object = move_one(&accused, root, sid, &lying_root, held);
        let s = step(&s, &p, 105, std::slice::from_ref(&object), None, &extras).expect("the lying root claim opens the phase");
        let filing = PalwAttnHeldFilingV1::from_object_v1(&object).map(|(_, f)| f).unwrap_or_else(|| PalwAttnHeldFilingV1 {
            binding: accused.evidence.binding.clone(),
            out_tile: accused.evidence.out_tile.clone(),
            anchor: accused.evidence.anchor.as_ref().expect("anchored").anchor.clone(),
            slice_sub_roots: accused.slice_sub_roots.clone(),
        });
        let ch = challenger.attn_site_evidence_held_v1(&[], liar.leaf, Some(&ids), Some(&filing)).expect("the challenger");
        let own_top = with_own_top(&ch, root);
        assert_ne!(own_top.slice_sub_roots, filing.slice_sub_roots, "the anchor follows the lie");
        let (s, daa) = rounds_through_the_fold(s, &p, &extras, sid, &accused, &ch, root, Some((lane, delta)), 0, 106);
        let without = adjudicate(&s, sid, &proof_of(&own_top, &s, sid, root));
        assert!(
            matches!(without, Err(PalwCourtV2Error::AttnCourt(PalwAttnCourtError::ChunkNotInCheckpoint { .. }))),
            "held {held}: the challenger's own top path does not reach the forger's anchor: {without:?}"
        );
        if !held {
            // Below the fence the chain holds no sub-roots: the forger's anchor is out of reach.
            continue;
        }
        let proof = proof_of(&ch, &s, sid, root);
        assert_eq!(adjudicate(&s, sid, &proof), Ok(PalwCourtVerdictV2::ExecutorGuilty), "through the filed sub-roots");
        let end = step(
            &s,
            &p,
            daa,
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof }],
            None,
            &extras,
        )
        .expect("the close folds");
        assert!(
            matches!(
                end.claim(&claim).expect("claim").phase,
                PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtHeldVerdict, .. }
            ),
            "a held dissection's verdict, recorded as one (decision (B)): {:?}",
            end.claim(&claim).expect("claim").phase
        );
        assert!(collateral(&end, PRODUCER) < collateral(&s, PRODUCER), "the forger is charged");
        assert_eq!(collateral(&end, SEAT), collateral(&s, SEAT), "the winning challenger pays nothing");
    }
}

/// **C3 and C4 end to end: a decoy shields no consistent forger.** The forger's own Sybil (a
/// bonded bystander on no panel) opens a held dissection at a fused leaf the forger can defend —
/// head 1's tile, beside the lie in head 0's — and the forger answers it honestly with the held
/// root claim its builder makes. While that session is open:
/// * below `palw_offence_attribution` the seat's accusation at the lie meets one court at a time
///   (the shield the decoy was for);
/// * past it the seat's dissection opens beside the decoy's;
/// * the decoy plays its dispute to the bottom it asked for and loses — `ChallengerDefeated`
///   charges the Sybil (C4) and the claim stands;
/// * the forger must then answer the seat: its lying root, the rounds, and the seat's bottom through
///   the filed sub-roots convict — voided `CourtHeldVerdict` (decision (B)), no session left, the
///   seat not charged.
#[test]
fn a_decoy_shields_no_consistent_forger_end_to_end() {
    let (artifact, profile) = held_fixture(2, 128);
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let challenger = backend(&artifact, &profile, PAST_THE_CAP);
    let (_, _, ids) = job_for(&challenger, &profile, PREFILL, DECODE);
    let honest = drilled(&artifact, &profile, 0, DECODE - 1, None);
    let liar = following(&artifact, &profile, 0, DECODE - 1, &honest);
    let decoy_leaf = drilled_at(&artifact, &profile, 0, DECODE - 1, (1, 0), None).leaf;
    assert_ne!(decoy_leaf, liar.leaf);
    let p = params();
    let x = launch();
    let (s, claim) = licensed(&liar, &profile, root, &x);
    let (s, decoy) = accuse(&s, &liar, claim, BYSTANDER, decoy_leaf, &ids, 104, &x).expect("the decoy opens at the defensible leaf");

    // The forger answers the decoy, honestly: head 1's tile is not its lie.
    let defended = liar
        .backend
        .attn_site_evidence_held_v1(&liar.run.outcome.material, decoy_leaf, Some(&ids), None)
        .expect("the forger's evidence at the decoy's leaf");
    let defended_site = defended.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let defended_root = defended.evidence.root_claim_v1(&defended_site).expect("root");
    let committed =
        palw_attn_opened_lanes_v1(&defended.evidence.out_tile, &defended_site.binding, defended_site.head_lanes.2 as usize)
            .expect("tile");
    assert!(least_lie(&defended_root, &committed, defended_site.site.params.values).is_none(), "an honest tile");
    let object = move_one(&defended, root, decoy, &defended_root, true);
    let s = step(&s, &p, 105, std::slice::from_ref(&object), None, &x).expect("the decoy's move 1");
    // The Sybil holds the forger's capture: its evidence is the forger's own (head 1's tile comes
    // after the lie in leaf order, so an honest replay's prefix is not the committed one there).
    let sybil = defended.clone();
    let tiles = u64::from(defended_site.history_positions).div_ceil(u64::from(defended_site.tile_positions));
    let (s, daa) = rounds_through_the_fold(s, &p, &x, decoy, &defended, &sybil, root, None, tiles - 1, 106);

    // Below the fence the decoy is a shield; past it the seat opens beside it.
    assert!(
        matches!(
            accuse(&s, &liar, claim, SEAT, liar.leaf, &ids, daa, &below()).expect_err("one court at a time"),
            PalwStateV2Error::ShardCourtClaimUnderSession { .. }
        ),
        "below the fence the seat waits behind the decoy"
    );
    let (s, sid) = accuse(&s, &liar, claim, SEAT, liar.leaf, &ids, daa, &x).expect("the seat's dissection opens beside the decoy's");
    assert_eq!(s.court_sessions_for_claim(&claim), 2);

    // The decoy bottoms the dispute it asked for, and loses.
    let proof = proof_of(&sybil, &s, decoy, root);
    assert_eq!(adjudicate(&s, decoy, &proof), Ok(PalwCourtVerdictV2::ChallengerDefeated));
    let s2 = step(
        &s,
        &p,
        daa + 1,
        &[PalwConsensusObjectV2::CourtClosed { session_id: decoy, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof }],
        None,
        &x,
    )
    .expect("the decoy's close folds");
    assert!(collateral(&s2, BYSTANDER) < collateral(&s, BYSTANDER), "the Sybil pays for the court it lost");
    assert!(matches!(s2.claim(&claim).expect("claim").phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(s2.court_sessions_for_claim(&claim), 1);

    // And the forger answers the seat.
    let accused = liar
        .backend
        .attn_site_evidence_held_v1(&liar.run.outcome.material, liar.leaf, Some(&ids), None)
        .expect("the forger's own evidence at the lie");
    let site = accused.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let committed = palw_attn_opened_lanes_v1(&accused.evidence.out_tile, &site.binding, site.head_lanes.2 as usize).expect("tile");
    let honest_root = accused.evidence.root_claim_v1(&site).expect("root");
    let (lying_root, lane, delta) = least_lie(&honest_root, &committed, site.site.params.values).expect("the forger must lie");
    let object = move_one(&accused, root, sid, &lying_root, true);
    let s3 = step(&s2, &p, daa + 2, std::slice::from_ref(&object), None, &x).expect("the seat's move 1");
    let filing = PalwAttnHeldFilingV1::from_object_v1(&object).expect("a held filing").1;
    let seat = challenger.attn_site_evidence_held_v1(&[], liar.leaf, Some(&ids), Some(&filing)).expect("the seat's evidence");
    let (s4, at) = rounds_through_the_fold(s3, &p, &x, sid, &accused, &seat, root, Some((lane, delta)), 0, daa + 3);
    let proof = proof_of(&seat, &s4, sid, root);
    assert_eq!(adjudicate(&s4, sid, &proof), Ok(PalwCourtVerdictV2::ExecutorGuilty));
    let end = step(
        &s4,
        &p,
        at,
        &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof }],
        None,
        &x,
    )
    .expect("the conviction folds");
    assert!(matches!(
        end.claim(&claim).expect("claim").phase,
        PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtHeldVerdict, .. }
    ));
    assert_eq!(end.court_sessions_for_claim(&claim), 0);
    assert_eq!(collateral(&end, SEAT), collateral(&s4, SEAT), "the winning seat pays nothing");
    assert!(collateral(&end, PRODUCER) < collateral(&s4, PRODUCER), "the forger is charged");
}
