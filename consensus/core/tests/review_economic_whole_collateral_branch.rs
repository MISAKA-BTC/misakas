//! Adversarial review (economic / DoS re-exploit lens): the whole-collateral branch of
//! `consume_objective_offence` (`palw_state_v2.rs`, the `else` after the lock and the liability row)
//! is still live on t12 (report #12 says it must change together with pruning; G1 did not land).
//! A `Valid` signer whose receipt was NOT carried by the licensing set holds no lock and is not in
//! the liability row, so a `PanelFalseValid` against it slashes its WHOLE collateral — 938,888 MSK
//! for a t12 genesis seat — where a carried signer loses its lock amount. Combined with
//! `review_economic_forged_false_valid` (a stranger can build that evidence from any leaked Valid
//! receipt), and since 8e28aa17 the same object also voids the Final.
//!
//! Fixture: `dos_g2_conviction_takes_back.rs`'s real t12 fold, except the licence carries three of
//! the five seats' Valid receipts (a quorum).
//!
//! **Fixed past the audit fence (2026-09-24 review):** a seat that holds no lock on the claim and
//! is on no liability row is not convicted at all — `ObjectiveOffenceRefused`, the object dropped,
//! the collateral untouched. The measurement of the defect is kept, below the fence, as an ignored
//! PRE-FENCE DEFECT RECORD. The contradiction is now the EXECUTOR's equivocation (its bond, its
//! registered key), which the review's other fix requires.
#![allow(dead_code)]

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_execution_lane_v1::{
    PalwExecFinalV1, PalwExecScheduleV1, palw_execution_permits_v1, palw_execution_schedule_snapshot_v1,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_bounded_v1};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateDeltaV2,
    PalwStateParamsV2, PalwTransitionExtrasV1, PalwVoidReasonV2, apply_palw_transition_v7, revert_delta_v2,
};

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// One block through the real fold, with the t12 extras — or with the audit fence switched OFF,
/// which is the fold every other network runs.
fn step(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
    audit: bool,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let f = flags(p, daa);
    let mut e: PalwTransitionExtrasV1 = extras(p, daa);
    e.audit_2026_09_23_active = audit;
    if !audit {
        e.settled_anchor_depth = None;
    }
    let (s, d, _) = apply_palw_transition_v7(
        parent,
        sp,
        None,
        &ctx(0x6200_0000 + daa, daa, daa, subsidy),
        objects,
        work,
        &[],
        exec_key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"));
    (s, d)
}

/// [`step`] that hands the fold's refusal back instead of panicking on it.
fn try_step(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    audit: bool,
) -> Result<PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
    let f = flags(p, daa);
    let mut e: PalwTransitionExtrasV1 = extras(p, daa);
    e.audit_2026_09_23_active = audit;
    if !audit {
        e.settled_anchor_depth = None;
    }
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        &ctx(0x6200_0000 + daa, daa, daa, 0),
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .map(|(s, _, _)| s)
}

/// The floor's registry row (what `step_model_registry` writes at a span boundary), so the fold's
/// probe counters have a row to move.
fn with_floor_lifecycle(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
        palw_model_work_from_carriage_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    let work = palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut c = PalwStateCarriageV2::from_state(s);
    c.model_lifecycles.insert(floor, row);
    rebuild(p, c)
}

fn rebuild(p: &Params, c: PalwStateCarriageV2) -> PalwChainStateV2 {
    c.into_state_v3(&bundle(p).state, None, flags(p, 0).uncertified_weightless, p.palw_canonical_work_daa())
        .expect("carriage rebuilds")
}

/// The loader's own check — `load_tip` runs it on every virtual resolution, so a state this fails
/// is a node that refuses its own tip.
fn reloads(p: &Params, s: &PalwChainStateV2) {
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("serializes");
    let c: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    let back = c
        .into_state_v3(&bundle(p).state, Some(s.state_root()), flags(p, 0).uncertified_weightless, p.palw_canonical_work_daa())
        .expect("the state the fold wrote passes the loader's consistency check");
    assert_eq!(back.state_root(), s.state_root());
}

fn attempt_attempts(s: &PalwChainStateV2) -> u64 {
    PalwStateCarriageV2::from_state(s).model_versions.values().map(|v| v.usage.attempt_claims).sum()
}

struct Finalized {
    p: Params,
    sp: PalwStateParamsV2,
    floor: Hash64,
    claim_id: Hash64,
    execution_root: Hash64,
    executor_bond: PalwBondKeyV2,
    executor_pubkey: Vec<u8>,
    valid_seats: Vec<PalwBondKeyV2>,
    before_final: PalwChainStateV2,
    at_final: PalwChainStateV2,
    final_daa: u64,
    daa: u64,
}

/// A junk floor attempt, bound to five genesis seats, licensed all-Valid and swept to `Final`.
fn drive_to_final(audit: bool) -> Finalized {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let bonds = genesis_bonds(&p);
    let (exec_bond, exec_pk, exec_op) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                Some((*bond, pubkey.clone(), operator_pubkey.clone()))
            }
            _ => None,
        })
        .expect("a genesis bond");
    let seats: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let valid_seats: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    let s = with_floor_lifecycle(&p, &genesis_state(&p));
    // The attempt names the floor's founding version root, so `note_claim_usage` counts it on the
    // version and the reversal has a usage row to take it back from. A founding line has no ROW
    // until something touches it (the fold synthesises it), and usage is counted only on a row, so
    // the row the first publication would write is written here through the carriage.
    let (s, founding_root) = {
        let line = s.model_line_or_founding(&floor).expect("the floor's founding line");
        let version = s.model_version(&floor, 1).expect("its version 1");
        let root = version.root;
        let mut c = PalwStateCarriageV2::from_state(&s);
        c.model_lines.insert(floor, line);
        c.model_versions.insert((floor, 1), version);
        (rebuild(&p, c), root)
    };
    let (env, key, claim_id) = attempt_on_root(floor, exec_bond, exec_pk.clone(), &exec_op, pwu, founding_root);
    let execution_root = env.attempt.execution_root;
    let (s, _) = step(&p, &sp, &s, 1_001, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, audit);
    let (s, _) = step(
        &p,
        &sp,
        &s,
        1_002,
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x6A), seats: seats_of(&seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        audit,
    );
    let (s, _) = step(
        &p,
        &sp,
        &s,
        1_003,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &valid_seats[..3]) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        audit,
    );
    assert!(matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    // Sweep, one block at a time, until the challenge window closes.
    let mut s = s;
    let mut daa = 1_003u64;
    let (before_final, final_daa) = loop {
        daa += 1;
        assert!(daa < 20_000, "the claim reaches Final inside the challenge window");
        let (next, _) = step(&p, &sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, audit);
        if let PalwClaimPhaseV2::Final { final_daa } = next.claim(&claim_id).unwrap().phase {
            let before = std::mem::replace(&mut s, next);
            break (before, final_daa);
        }
        s = next;
    };
    Finalized {
        p,
        sp,
        floor,
        claim_id,
        execution_root,
        executor_bond: exec_bond,
        executor_pubkey: exec_pk,
        valid_seats,
        before_final,
        at_final: s,
        final_daa,
        daa,
    }
}

/// `dos_l5_common::junk_attempt` with a chosen artifact root — every other field as junk as there.
fn attempt_on_root(
    class_id: Hash64,
    bond: PalwBondKeyV2,
    bond_pubkey: Vec<u8>,
    operator_pubkey: &[u8],
    pwu: u64,
    artifact_root: Hash64,
) -> (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64) {
    use kaspa_consensus_core::palw_attempt_v2::{attempt_id_v2, execution_anchor_v3, execution_commitment_v3};
    let (mut env, _, _) = junk_attempt(class_id, bond, bond_pubkey, operator_pubkey, pwu, 77, 0x6077);
    env.attempt.artifact_root = artifact_root;
    let anchor = execution_anchor_v3(h(NET), h(0x6077), class_id, &bond.0, 7);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    (env, key, id)
}

/// `dos_repro_1`'s contradiction: an `ExecutorEquivocation` carriage, which the acceptance layer
/// verifies and the fold takes as convicting the execution.
fn false_valid(f: &Finalized, accused: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: f.executor_bond.0,
        signature: Vec::new(),
    };
    let equivocation = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: f.executor_bond.0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context: kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor_profile(), 512, 256),
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    };
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: f.claim_id,
        network_domain: h(NET),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 {
            claim: f.claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: accused,
            signed_daa: 0,
            signature: Vec::new(),
        },
        executor_pubkey: f.executor_pubkey.clone(),
        contradiction: PalwPanelContradictionV1::ExecutorEquivocation(equivocation),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::PanelFalseValid,
        accused,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}


/// **Past the fence: a `Valid` signer the licence did not carry is not convicted.** The carried
/// signer loses exactly its lock; the uncarried one — no lock, not on the liability row — is
/// refused by name and keeps every sompi it posted.
///
/// Fails without the fix: the uncarried signer's conviction folds and takes its whole collateral
/// (939,063 MSK of a t12 genesis seat against the carried signer's 1,174 MSK lock).
#[test]
fn review_economic_a_valid_signer_the_licence_did_not_carry_is_not_convicted() {
    let f = drive_to_final(true);
    let (p, sp) = (&f.p, &f.sp);
    let carried = f.valid_seats[0];
    let uncarried = f.valid_seats[3];
    let lock = f.at_final.slashable_lock(carried, f.claim_id).copied().expect("a carried signer is locked");
    assert!(f.at_final.slashable_lock(uncarried, f.claim_id).is_none(), "the uncarried seat holds no lock");
    assert!(
        !f.at_final.panel_liability(&f.claim_id).expect("the Final's liability row").valid_signers.iter().any(|(s, _)| *s == uncarried.0),
        "and is not on the liability row"
    );
    let before_c = f.at_final.bond(&carried).unwrap().collateral;
    let before_u = f.at_final.bond(&uncarried).unwrap().collateral;
    let (s, _) = step(p, sp, &f.at_final, f.daa + 5, &[false_valid(&f, carried)], PalwBlockWorkV3::None, Hash64::default(), 0, true);
    let lost_c = before_c - s.bond(&carried).unwrap().collateral;
    assert_eq!(lost_c as u128, lock.amount, "the carried signer loses its lock");
    let refused = try_step(p, sp, &s, f.daa + 6, &[false_valid(&f, uncarried)], true);
    println!("carried Valid signer: lock {} sompi, lost {} ({:.2} MSK)", lock.amount, lost_c, msk(lost_c as u128));
    println!("uncarried Valid signer: {refused:?}");
    assert!(
        matches!(&refused, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ObjectiveOffenceRefused(_, why)) if why.contains("no Valid lock")),
        "the uncarried signer is not convicted: {refused:?}"
    );
    assert_eq!(s.bond(&uncarried).unwrap().collateral, before_u, "its collateral is untouched");
    // The same holds before the carried seat's conviction and before the Final: nothing about the
    // order lets the whole-collateral arm back in.
    let early = try_step(p, sp, &f.at_final, f.daa + 5, &[false_valid(&f, uncarried)], true);
    assert!(matches!(early, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ObjectiveOffenceRefused(..))), "{early:?}");
}

/// The audit review's measurement, below the fence where the rule is unchanged: the uncarried
/// signer loses its whole collateral.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: below palw_audit_2026_09_23 a Valid signer the licence did not carry loses its whole collateral to one PanelFalseValid (review_economic_whole_collateral_branch); closed past the fence"]
fn review_economic_a_valid_signer_the_licence_did_not_carry_loses_its_whole_collateral_defect_record() {
    let f = drive_to_final(false);
    let (p, sp) = (&f.p, &f.sp);
    let carried = f.valid_seats[0];
    let uncarried = f.valid_seats[3];
    let lock = f.at_final.slashable_lock(carried, f.claim_id).copied().expect("a carried signer is locked");
    assert!(f.at_final.slashable_lock(uncarried, f.claim_id).is_none(), "the uncarried seat holds no lock");
    let before_c = f.at_final.bond(&carried).unwrap().collateral;
    let before_u = f.at_final.bond(&uncarried).unwrap().collateral;
    let (s, _) = step(p, sp, &f.at_final, f.daa + 5, &[false_valid(&f, carried)], PalwBlockWorkV3::None, Hash64::default(), 0, false);
    let (s, _) = step(p, sp, &s, f.daa + 6, &[false_valid(&f, uncarried)], PalwBlockWorkV3::None, Hash64::default(), 0, false);
    let lost_c = before_c - s.bond(&carried).unwrap().collateral;
    let lost_u = before_u - s.bond(&uncarried).unwrap().collateral;
    println!("carried Valid signer: lock {} sompi, lost {} ({:.2} MSK)", lock.amount, lost_c, msk(lost_c as u128));
    println!("uncarried Valid signer: no lock, lost {} ({:.2} MSK) of {} posted", lost_u, msk(lost_u as u128), before_u);
    assert_eq!(lost_c as u128, lock.amount, "the carried signer loses its lock");
    assert_eq!(lost_u, before_u, "the uncarried signer loses its whole collateral");
    assert!(lost_u as u128 > 100 * lock.amount);
}

/// **2026-09-24 DoS audit #12 (a): an obligation past its evidence horizon is pruned, and a
/// conviction of it is REFUSED — never priced at the seat's whole collateral.**
///
/// The real t12 fold from the Final on: one claimless block per epoch boundary until the carried
/// signer's lock and the claim's liability row leave the state together. The boundary before that
/// still holds both (and a conviction there takes exactly the lock); the boundary that prunes them
/// is at or past `expiry + window_court` and past the second clock's hold; the conviction on the
/// pruned state is `ObjectiveOffenceRefused` with the collateral untouched; and the pruning block's
/// delta reverts to its parent's root.
///
/// Fails without the fix: nothing is ever pruned, and the loop runs out of boundaries.
#[test]
fn fix12_a_pruned_obligation_is_refused_never_the_whole_collateral() {
    let f = drive_to_final(true);
    let (p, sp) = (&f.p, &f.sp);
    let carried = f.valid_seats[0];
    let lock = f.at_final.slashable_lock(carried, f.claim_id).copied().expect("a carried signer is locked");
    let row = f.at_final.panel_liability(&f.claim_id).cloned().expect("the Final's liability row");
    let horizon = row.expiry_daa + sp.window_court();
    let epoch = sp.epoch_length();
    let mut state = f.at_final.clone();
    let mut daa = f.daa;
    let (held, pruned, delta) = loop {
        daa = (daa / epoch + 1) * epoch;
        assert!(daa < horizon + 10 * epoch, "the obligation must be pruned within an epoch or two of its horizon");
        let (next, delta) = step(p, sp, &state, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, true);
        if next.panel_liability(&f.claim_id).is_none() {
            break (state, next, delta);
        }
        assert!(next.slashable_lock(carried, f.claim_id).is_some(), "the lock never leaves before its liability row");
        state = next;
    };
    println!(
        "Final at {}, liability expiry {}, horizon {horizon}; last boundary holding it {}, pruned at {daa}",
        f.final_daa,
        row.expiry_daa,
        held.last_point().map(|c| c.daa_score).unwrap_or(0)
    );
    assert!(daa >= horizon, "pruned no earlier than expiry + window_court");
    assert!(pruned.slashable_lock(carried, f.claim_id).is_none(), "the lock went with its liability row");
    for seat in &f.valid_seats {
        assert!(pruned.slashable_lock(*seat, f.claim_id).is_none());
    }
    // The boundary before: a conviction still takes exactly the lock.
    let before_c = held.bond(&carried).unwrap().collateral;
    let convicted = step(p, sp, &held, daa - 1, &[false_valid(&f, carried)], PalwBlockWorkV3::None, Hash64::default(), 0, true).0;
    assert_eq!((before_c - convicted.bond(&carried).unwrap().collateral) as u128, lock.amount, "inside the horizon: the lock");
    // The pruned state: refused, the collateral whole.
    let refused = try_step(p, sp, &pruned, daa + 1, &[false_valid(&f, carried)], true);
    println!("conviction after pruning: {refused:?}");
    assert!(
        matches!(refused, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ObjectiveOffenceRefused(..))),
        "a pruned obligation convicts nobody: {refused:?}"
    );
    assert_eq!(pruned.bond(&carried).unwrap().collateral, before_c);
    reloads(p, &pruned);
    // A reorg across the pruning boundary un-prunes it exactly.
    let back = revert_delta_v2(&pruned, &delta, sp).expect("the pruning block reverts");
    assert_eq!(back.state_root(), held.state_root(), "every pruning delta reverts");
    assert_eq!(back.panel_liability(&f.claim_id), Some(&row));
}

/// **2026-09-24 DoS audit #12 (a): a `BindTimeout` that no `Valid` signed leaves no liability row**
/// past the fence; below it the row is written as it always was (the panel-less fallback).
///
/// Fails without the fix: the audited fold writes the row too.
#[test]
fn fix12_a_a_signerless_bind_timeout_writes_no_liability() {
    let p = t12();
    let sp = bundle(&p).state.clone();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let b = bundle(&p);
    let (exec_bond, exec_pk, exec_op) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                Some((*bond, pubkey.clone(), operator_pubkey.clone()))
            }
            _ => None,
        })
        .expect("a genesis bond");
    for audit in [true, false] {
        let s = with_floor_lifecycle(&p, &genesis_state(&p));
        let (env, key, claim_id) = junk_attempt(floor, exec_bond, exec_pk.clone(), &exec_op, pwu, 5, 0x6B7);
        let (s, _) = step(&p, &sp, &s, 1_001, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, audit);
        let (s, _) = step(&p, &sp, &s, 1_001 + sp.window_bind() + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0, audit);
        assert!(
            matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
            "the claim voided at BindTimeout"
        );
        assert_eq!(s.panel_liability(&claim_id).is_some(), !audit, "audit {audit}: a liability row only below the fence");
    }
}
