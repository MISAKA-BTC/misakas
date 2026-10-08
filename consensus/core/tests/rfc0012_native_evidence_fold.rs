//! **RFC-0012 native settlement evidence, through the REAL fold on testnet-12's own `Params`.**
//!
//! `apply_palw_transition_v7` drives a floor claim from its attempt to `Final` and then to its
//! retirement (3,000 DAA later), and the evidence functions of `palw_native_settlement_v1` are asked, at
//! every block, what they would see. Nothing here is a model of the fold: the deltas are the fold's.
//!
//! * The dormant reader looked for work in the claims a state still holds whose trace retention has lapsed.
//!   Across this claim's whole life that is never true (`the_old_reader_never_sees_the_claim`): it is retired
//!   at `Final + 3,000` and its retention lapses at `accepted + 5,400`.
//! * The delta of the block that finalized the claim carries the whole record, so the work is read there
//!   and survives the retirement (`the_finalizing_delta_carries_the_work_past_retirement`).
//! * The floor is never evidence (`a_floor_claim_is_no_evidence`); the shared fixture only has a floor claim
//!   it can drive without the class registry, so the conversion is exercised on the same record relabelled
//!   to another class, which is stated where it is done.
//!
//! Run: cargo test -p kaspa-consensus-core --test rfc0012_native_evidence_fold -- --nocapture

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_attempt_v2::{attempt_id_v2, execution_anchor_v3, execution_commitment_v3};
use kaspa_consensus_core::palw_native_settlement_v1::{
    NativeDeltaEvidenceV1, NativeFactRulesV1, PalwSettlementPolicyV1, SettlementStopV1, certify_native_effect_v1, native_delta_evidence_v1,
    native_facts_of_block_v1,
};
use kaspa_consensus_core::palw_producer_v2::palw_min_trace_retention_daa_v1;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateDeltaV2,
    PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use std::collections::BTreeSet;

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


/// One claim's life through the fold.
struct Life {
    p: Params,
    sp: PalwStateParamsV2,
    id: Hash64,
    accepted_daa: u64,
    final_daa: u64,
    retired_daa: u64,
    /// The delta of the block that finalized the claim.
    final_delta: PalwStateDeltaV2,
    /// The delta of the block that retired it.
    retire_delta: PalwStateDeltaV2,
    /// The state at the block that finalized it, and the first state without the claim.
    at_final: PalwChainStateV2,
    after_retirement: PalwChainStateV2,
    /// For every DAA the claim was in state: whether the old reader (Final and retention lapsed) saw it.
    old_reader_saw: Vec<(u64, bool)>,
    class: Hash64,
    retention_daa: u64,
}

fn live_life() -> Life {
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
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => Some((*bond, pubkey.clone(), operator_pubkey.clone())),
            _ => None,
        })
        .expect("a genesis bond");
    let seats: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let valid_seats: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    let s = with_floor_lifecycle(&p, &genesis_state(&p));
    let (mut env, _, _) = junk_attempt(floor, exec_bond, exec_pk, &exec_op, pwu, 77, 0x6077);
    let accepted_daa = 1_001u64;
    // The retention a producer commits (`palw_min_trace_retention_daa_v1`), not the fixture's placeholder.
    env.attempt.trace_retention_daa = accepted_daa + palw_min_trace_retention_daa_v1(&sp);
    let anchor = execution_anchor_v3(h(NET), h(0x6077), floor, &exec_bond.0, 7);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    let retention_daa = env.attempt.trace_retention_daa;
    let (s, _) = step(&p, &sp, &s, accepted_daa, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, true);
    let (s, _) = step(
        &p,
        &sp,
        &s,
        1_002,
        &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0x6A), seats: seats_of(&seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        true,
    );
    let (mut s, _) = step(
        &p,
        &sp,
        &s,
        1_003,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &valid_seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        true,
    );
    assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    let mut daa = 1_003u64;
    let mut old_reader_saw = Vec::new();
    let (mut final_daa, mut final_delta, mut at_final) = (0, None, None);
    let (retired_daa, retire_delta, after_retirement) = loop {
        daa += 1;
        assert!(daa < 20_000, "the claim retires");
        let (next, delta) = step(&p, &sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, true);
        match next.claim(&id) {
            Some(c) => {
                if let PalwClaimPhaseV2::Final { final_daa: f } = c.phase {
                    if final_delta.is_none() {
                        (final_daa, final_delta, at_final) = (f, Some(delta), Some(next.clone()));
                    }
                    // The dormant reader: a Final claim the state still holds whose trace retention has lapsed.
                    old_reader_saw.push((daa, c.trace_retention_daa <= daa));
                }
            }
            None => break (daa, delta, next),
        }
        s = next;
    };
    Life {
        p,
        sp,
        id,
        accepted_daa,
        final_daa,
        retired_daa,
        final_delta: final_delta.expect("the claim reached Final"),
        retire_delta,
        at_final: at_final.unwrap(),
        after_retirement,
        old_reader_saw,
        class: floor,
        retention_daa,
    }
}

fn rules<'a>(l: &'a Life, state: &'a PalwChainStateV2, open: &'a BTreeSet<Hash64>) -> NativeFactRulesV1<'a> {
    NativeFactRulesV1 { state, params: &l.sp, canonical_work_daa: l.p.palw_canonical_work_daa(), quantum_maturity_daa: 120, claims_with_open_da: open }
}

#[test]
fn rfc0012_the_old_reader_never_sees_the_claim_and_the_finalizing_delta_carries_the_work_past_retirement() {
    let l = live_life();
    let retirement = l.sp.claim_retirement_daa();
    eprintln!(
        "[rfc0012-fold] claim {} accepted {} Final {} (+{}) retired {} (+{} after Final) retention lapses {} (+{} after acceptance); old reader looked at {} blocks",
        l.id,
        l.accepted_daa,
        l.final_daa,
        l.final_daa - l.accepted_daa,
        l.retired_daa,
        l.retired_daa - l.final_daa,
        l.retention_daa,
        l.retention_daa - l.accepted_daa,
        l.old_reader_saw.len()
    );
    // EXPECTED: the claim is retired exactly `claim_retirement` after Final, before its retention lapses.
    assert!(l.retired_daa >= l.final_daa + retirement && l.retired_daa <= l.final_daa + retirement + 2, "retired at Final + {retirement}");
    assert!(l.retired_daa < l.retention_daa, "retired at {} before its retention lapses at {}", l.retired_daa, l.retention_daa);
    // The dormant reader never saw the Final claim, at any block of its life.
    assert!(!l.old_reader_saw.is_empty() && l.old_reader_saw.iter().all(|(_, saw)| !saw), "the old reader saw an ordinary claim");

    // The new reader: the Final transition is in the finalizing block's delta, whole.
    let evidence = native_delta_evidence_v1(&l.final_delta);
    assert_eq!(evidence.finalized_attempts.len(), 1, "one REAL-attempt Final in that block");
    assert!(evidence.fp_spends.is_empty() && evidence.voided.is_empty());
    let (key, claim) = &evidence.finalized_attempts[0];
    assert_eq!(*key, l.id);
    assert_eq!(claim.phase, PalwClaimPhaseV2::Final { final_daa: l.final_daa });
    // The retiring block's delta removes the claim: it is not a Final, a spend or a void.
    assert!(native_delta_evidence_v1(&l.retire_delta).is_empty(), "a retirement is not evidence");

    // The floor is never evidence, retired or not.
    let open = BTreeSet::new();
    let none = native_facts_of_block_v1(&rules(&l, &l.after_retirement, &open), &evidence, &BTreeSet::new(), (l.accepted_daa, l.accepted_daa));
    assert!(none.is_empty(), "a floor claim is no evidence");

    // The same record relabelled to a REAL class (the fixture can only drive the floor without the class registry).
    let real = genesis_classes(&l.p)[1].0;
    assert_ne!(real, l.class);
    let mut relabelled = NativeDeltaEvidenceV1::default();
    relabelled.finalized_attempts.push((*key, { let mut c = claim.clone(); c.class_id = real; c }));
    assert!(l.after_retirement.claim(&l.id).is_none(), "the claim is gone from the sink state");
    let facts = native_facts_of_block_v1(&rules(&l, &l.after_retirement, &open), &relabelled, &BTreeSet::new(), (l.accepted_daa, l.accepted_daa));
    assert_eq!(facts.len(), 1, "the work survives the claim's retirement");
    let f = facts[0];
    assert_eq!(f.work, claim.pwu as u128, "weight is the canonical pwu");
    assert_eq!(f.accepted_daa, l.accepted_daa);
    assert_eq!(f.matured_daa, l.retention_daa.max(l.final_daa + retirement), "matured when it can no longer be convicted and its trace may be dropped");
    assert_eq!(f.identity, claim.work_id.expect("a canonical work id"));
    assert_eq!(f.class, real);

    // Counting: not before maturity, once after, however many chain blocks restate it.
    let policy = PalwSettlementPolicyV1 { settled_anchor_depth: 1, unique_mature_work: 1, max_operator_permille: 1000, max_class_permille: 1000 };
    let effect = (l.accepted_daa, l.accepted_daa);
    let at = |snapshot| certify_native_effect_v1(policy, effect, snapshot, true, true, true, true, &facts);
    assert_eq!(at(f.matured_daa - 1), Err(SettlementStopV1::InsufficientDepth), "an immature fact counts nothing");
    let ok = at(f.matured_daa).expect("matured work certifies");
    assert_eq!((ok.depth, ok.work), (1, f.work));
    // A conviction after Final retracts it, wherever on the chain the void landed.
    let voided: BTreeSet<Hash64> = [l.id].into_iter().collect();
    assert!(native_facts_of_block_v1(&rules(&l, &l.after_retirement, &open), &relabelled, &voided, effect).is_empty());
    // An open DA session on the claim keeps it out.
    let open_da: BTreeSet<Hash64> = [l.id].into_iter().collect();
    assert!(native_facts_of_block_v1(&rules(&l, &l.after_retirement, &open_da), &relabelled, &BTreeSet::new(), effect).is_empty());
    // A bond the sink no longer holds attributes nothing (never guessed).
    let mut ghost = relabelled.clone();
    ghost.finalized_attempts[0].1.bond = bond_key(0xDEAD);
    assert!(native_facts_of_block_v1(&rules(&l, &l.after_retirement, &open), &ghost, &BTreeSet::new(), effect).is_empty());
    // The state at Final still holds the claim (the lifecycle question reads it from there).
    assert!(l.at_final.claim(&l.id).is_some());
}

/// A conviction after `Final` is a `Final -> Voided` entry in the convicting block's delta: it is extracted as a void
/// and retracts what the Final had counted.
#[test]
fn rfc0012_a_void_entry_is_extracted_and_a_resumed_final_is_not_evidence_twice() {
    use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwVoidReasonV2, palw_claim_template_v1};
    let id = h(0xC1A1);
    let mut final_claim = palw_claim_template_v1(h(0x77), bond_key(1), 10, 0, 0);
    final_claim.phase = PalwClaimPhaseV2::Final { final_daa: 50 };
    let mut voided = final_claim.clone();
    voided.phase = PalwClaimPhaseV2::Voided { voided_daa: 60, reason: PalwVoidReasonV2::CourtFraud };
    let mut provisional = final_claim.clone();
    provisional.phase = PalwClaimPhaseV2::Provisional;
    let point = ctx(1, 60, 60, 0);
    let delta = |entries| PalwStateDeltaV2 { point, entries };
    let entry = |old: Option<&_>, new: Option<&_>| PalwDeltaEntryV2::Claim { key: id, old: old.cloned(), new: new.cloned() };
    // Provisional -> Final is the evidence; a re-write of the same Final (Final -> Final) is not a second one.
    let e = native_delta_evidence_v1(&delta(vec![entry(Some(&provisional), Some(&final_claim)), entry(Some(&final_claim), Some(&final_claim))]));
    assert_eq!(e.finalized_attempts.len(), 1);
    // Final -> Voided retracts; a later Voided -> Voided (the retirement re-arm) retracts nothing new.
    let e = native_delta_evidence_v1(&delta(vec![entry(Some(&final_claim), Some(&voided)), entry(Some(&voided), Some(&voided))]));
    assert_eq!(e.voided, vec![id]);
    assert!(e.finalized_attempts.is_empty());
    // A removal (retirement) is neither.
    assert!(native_delta_evidence_v1(&delta(vec![entry(Some(&final_claim), None)])).is_empty());
}


// =====================================================================================================================
// Measurements for the policy proposal (docs/design/palw/rfc-0012-policy-proposal.md): every number comes from the fold or
// from the certification code, on testnet-12's own Params. They are printed; the assertions pin the facts the proposal quotes.
// =====================================================================================================================

/// Drive a floor claim from acceptance until it is `Voided` or `Final`; `bind` / `receipts` choose how far honest verification gets.
/// Returns `(accepted, terminal_phase, terminal_daa)`.
fn run_claim(bind: bool, receipts: bool) -> (u64, PalwClaimPhaseV2, u64) {
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
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => Some((*bond, pubkey.clone(), operator_pubkey.clone())),
            _ => None,
        })
        .expect("a genesis bond");
    let seats: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let valid_seats: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    let s = with_floor_lifecycle(&p, &genesis_state(&p));
    let (mut env, _, _) = junk_attempt(floor, exec_bond, exec_pk, &exec_op, pwu, 77, 0x6077);
    let accepted = 1_001u64;
    env.attempt.trace_retention_daa = accepted + palw_min_trace_retention_daa_v1(&sp);
    let anchor = execution_anchor_v3(h(NET), h(0x6077), floor, &exec_bond.0, 7);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    let (mut s, _) = step(&p, &sp, &s, accepted, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, true);
    let mut daa = accepted;
    if bind {
        daa += 1;
        s = step(&p, &sp, &s, daa, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0x6A), seats: seats_of(&seats) }], PalwBlockWorkV3::None, Hash64::default(), 0, true).0;
        if receipts {
            daa += 1;
            s = step(&p, &sp, &s, daa, &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &valid_seats) }], PalwBlockWorkV3::None, Hash64::default(), 0, true).0;
        }
    }
    loop {
        daa += 1;
        assert!(daa < 30_000);
        s = step(&p, &sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, true).0;
        match s.claim(&id).map(|c| c.phase.clone()) {
            Some(phase @ PalwClaimPhaseV2::Final { final_daa: t }) => return (accepted, phase, t),
            Some(phase @ PalwClaimPhaseV2::Voided { voided_daa: t, .. }) => return (accepted, phase, t),
            _ => {}
        }
    }
}

/// **How long can one claim hold the lifecycle open?** A claim is open from its acceptance until it is `Final` (and its retention
/// lapses or it retires) or `Voided`. The lifecycle rule holds every effect at or after an open claim, so this is the longest a
/// claim the producer simply does not get verified can hold `safe` back.
#[test]
fn rfc0012_measure_how_long_an_unverified_claim_holds_the_lifecycle_open() {
    let (a0, phase0, t0) = run_claim(false, false);
    let (a1, phase1, t1) = run_claim(true, false);
    let (a2, phase2, t2) = run_claim(true, true);
    eprintln!("[rfc0012-measure] never bound:           {phase0:?} at +{} DAA after acceptance", t0 - a0);
    eprintln!("[rfc0012-measure] bound, no receipts:    {phase1:?} at +{} DAA after acceptance", t1 - a1);
    eprintln!("[rfc0012-measure] bound, licensed:       {phase2:?} at +{} DAA after acceptance", t2 - a2);
    let sp = bundle(&t12()).state;
    assert!(matches!(phase0, PalwClaimPhaseV2::Voided { .. }) && matches!(phase1, PalwClaimPhaseV2::Voided { .. }));
    assert!(matches!(phase2, PalwClaimPhaseV2::Final { .. }));
    assert!(t0 - a0 <= sp.window_bind() + 2, "an unbound claim voids at the bind window");
    assert!(t1 - a1 <= sp.window_bind() + sp.window_receipt() + 4, "a bound claim with no receipts voids at bind + receipt");
}

/// **What each REAL class's claim is worth in `pwu`** — the unit W is measured in. Printed from the shipped genesis registry.
#[test]
fn rfc0012_measure_the_work_a_genesis_claim_carries() {
    let p = t12();
    for (class, leaves, target, slash) in genesis_classes(&p) {
        let pwu = palw_pwu_v1(target, leaves);
        eprintln!("[rfc0012-measure] class {} leaves {leaves} -> pwu per claim {pwu} (slash value per pwu {slash})", &class.to_string()[..12]);
        assert!(pwu > 0);
    }
}

/// **How the concentration caps bite**, through `certify_native_effect_v1` itself: `k` operators supplying shares of one unit of
/// work (equal; 60/40-style skew; Zipf), against caps. Printed as a table; asserts only the arithmetic the proposal relies on
/// (a cap of c permille needs at least ceil(1000 / c) operators).
#[test]
fn rfc0012_measure_which_operator_caps_certify_which_distributions() {
    use kaspa_consensus_core::palw_native_settlement_v1::MatureUsefulWorkV1;
    let fact = |i: u64, operator: u64, work: u128| MatureUsefulWorkV1 {
        identity: h(0x100 + i),
        anchor: h(0x200 + i),
        operator: h(0x300 + operator),
        class: h(0x400 + (i % 3)),
        anchor_blue: 10,
        accepted_blue: 10,
        anchor_daa: 10,
        accepted_daa: 10,
        matured_daa: 0,
        work,
    };
    let distributions: Vec<(&str, Vec<u128>)> = vec![
        ("1 operator", vec![1000]),
        ("2 equal", vec![500, 500]),
        ("60/40", vec![600, 400]),
        ("3 equal", vec![334, 333, 333]),
        ("4 equal", vec![250; 4]),
        ("8 equal", vec![125; 8]),
        ("zipf over 8", vec![368, 184, 123, 92, 74, 61, 53, 45]),
    ];
    let caps = [300u16, 400, 500, 600, 800, 1000];
    eprintln!("[rfc0012-measure] operator cap (permille): {caps:?}  (ok = certifies, X = ConcentratedWork)");
    for (name, shares) in &distributions {
        let facts: Vec<_> = shares.iter().enumerate().map(|(i, w)| fact(i as u64, i as u64, *w)).collect();
        let row: Vec<&str> = caps
            .iter()
            .map(|cap| {
                let policy = PalwSettlementPolicyV1 { settled_anchor_depth: 1, unique_mature_work: 1, max_operator_permille: *cap, max_class_permille: 1000 };
                match certify_native_effect_v1(policy, (10, 10), 20, true, true, true, true, &facts) {
                    Ok(_) => "ok",
                    Err(SettlementStopV1::ConcentratedWork) => "X",
                    Err(other) => panic!("{other:?}"),
                }
            })
            .collect();
        eprintln!("[rfc0012-measure]   {name:<12} {row:?}");
        let top = *shares.iter().max().unwrap() as u32;
        for (cap, outcome) in caps.iter().zip(&row) {
            assert_eq!(*outcome == "ok", top <= *cap as u32, "{name} at {cap}: the largest share is {top}");
        }
    }
}
