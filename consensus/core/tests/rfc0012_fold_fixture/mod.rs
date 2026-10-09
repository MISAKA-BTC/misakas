//! Shared fixture of the RFC-0012 real-fold tests (`rfc0012_native_evidence_fold.rs`, `rfc0012_safe_maturity_attacks.rs`): one claim's
//! life through `apply_palw_transition_v7` on testnet-12's own `Params`. Not a test target (it lives in a directory with a `mod.rs`).

#![allow(dead_code)]

use super::common::*;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_attempt_v2::{attempt_id_v2, execution_anchor_v3, execution_commitment_v3};
use kaspa_consensus_core::palw_native_settlement_v1::{NativeFactRulesV1, native_delta_evidence_v1};
use kaspa_consensus_core::palw_producer_v2::palw_min_trace_retention_daa_v1;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateDeltaV2,
    PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use std::collections::BTreeSet;

pub fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// An edit to the fold's extras, applied after the fixture built them: how a test arms a lane the fixture does not (RFC-0012 C4/C6).
pub type ExtrasEdit = std::sync::Arc<dyn Fn(&mut PalwTransitionExtrasV1) + Send + Sync>;

pub fn no_edit() -> ExtrasEdit {
    std::sync::Arc::new(|_| {})
}

/// One block through the real fold, with the t12 extras - or with the audit fence switched OFF,
/// which is the fold every other network runs. `Err` is the fold's own refusal of the block.
pub fn try_step(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
    audit: bool,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
    try_step_with(p, sp, parent, daa, objects, work, exec_key, subsidy, audit, &|_| {})
}

/// [`try_step`] with `edit` applied to the extras the fixture built.
pub fn try_step_with(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
    audit: bool,
    edit: &dyn Fn(&mut PalwTransitionExtrasV1),
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
    let f = flags(p, daa);
    let mut e: PalwTransitionExtrasV1 = extras(p, daa);
    e.audit_2026_09_23_active = audit;
    if !audit {
        e.settled_anchor_depth = None;
    }
    edit(&mut e);
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
    )?;
    Ok((s, d))
}

/// [`try_step`] for a [`Life`]: its params, its state params and its extras edit, the audit fence on.
pub fn try_step_for(
    l: &Life,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
    try_step_with(&l.p, &l.sp, parent, daa, objects, work, exec_key, subsidy, true, &*l.extras_edit)
}

/// [`try_step`], and a refusal is a panic naming the DAA.
pub fn step(
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
    try_step(p, sp, parent, daa, objects, work, exec_key, subsidy, audit).unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"))
}

/// [`step`] with an extras edit.
pub fn step_with(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
    edit: &dyn Fn(&mut PalwTransitionExtrasV1),
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    try_step_with(p, sp, parent, daa, objects, work, exec_key, subsidy, true, edit).unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"))
}

/// The floor's registry row (what `step_model_registry` writes at a span boundary), so the fold's
/// probe counters have a row to move.
pub fn with_floor_lifecycle(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
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

pub fn rebuild(p: &Params, c: PalwStateCarriageV2) -> PalwChainStateV2 {
    c.into_state_v3(&bundle(p).state, None, flags(p, 0).uncertified_weightless, p.palw_canonical_work_daa())
        .expect("carriage rebuilds")
}

/// One claim's life through the fold.
pub struct Life {
    pub p: Params,
    pub sp: PalwStateParamsV2,
    pub id: Hash64,
    pub accepted_daa: u64,
    pub final_daa: u64,
    pub retired_daa: u64,
    /// The delta of the block that finalized the claim.
    pub final_delta: PalwStateDeltaV2,
    /// The delta of the block that retired it.
    pub retire_delta: PalwStateDeltaV2,
    /// The state at the block that finalized it, and the first state without the claim.
    pub at_final: PalwChainStateV2,
    pub after_retirement: PalwChainStateV2,
    /// For every DAA the claim was in state: whether the old reader (Final and retention lapsed) saw it.
    pub old_reader_saw: Vec<(u64, bool)>,
    pub class: Hash64,
    pub retention_daa: u64,
    pub executor_bond: PalwBondKeyV2,
    pub executor_pubkey: Vec<u8>,
    pub valid_seats: Vec<PalwBondKeyV2>,
    /// How long `native_delta_evidence_v1` took over every delta of the claim's life, and how many there were.
    pub extraction: (std::time::Duration, usize),
    /// The extras edit this life was folded under (a lane the fixture does not arm, armed for a measurement).
    pub extras_edit: ExtrasEdit,
}

pub fn live_life() -> Life {
    live_life_with(no_edit())
}

/// [`live_life`] with the fold's extras edited at every block (RFC-0012 C4/C6: arm a lane, re-measure).
pub fn live_life_with(extras_edit: ExtrasEdit) -> Life {
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
    let exec_pk_kept = exec_pk.clone();
    let (mut env, _, _) = junk_attempt(floor, exec_bond, exec_pk, &exec_op, pwu, 77, 0x6077);
    let accepted_daa = 1_001u64;
    // The retention a producer commits (`palw_min_trace_retention_daa_v1`), not the fixture's placeholder.
    env.attempt.trace_retention_daa = accepted_daa + palw_min_trace_retention_daa_v1(&sp);
    let anchor = execution_anchor_v3(h(NET), h(0x6077), floor, &exec_bond.0, 7);
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    let retention_daa = env.attempt.trace_retention_daa;
    let (s, _) =
        step_with(&p, &sp, &s, accepted_daa, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, &*extras_edit);
    let (s, _) = step_with(
        &p,
        &sp,
        &s,
        1_002,
        &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0x6A), seats: seats_of(&seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        &*extras_edit,
    );
    let (mut s, _) = step_with(
        &p,
        &sp,
        &s,
        1_003,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &valid_seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        &*extras_edit,
    );
    assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    let mut daa = 1_003u64;
    let mut old_reader_saw = Vec::new();
    let mut extraction = (std::time::Duration::ZERO, 0usize);
    let (mut final_daa, mut final_delta, mut at_final) = (0, None, None);
    let (retired_daa, retire_delta, after_retirement) = loop {
        daa += 1;
        assert!(daa < 20_000, "the claim retires");
        let (next, delta) = step_with(&p, &sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, &*extras_edit);
        // What a cold row costs: the delta's stored bytes decoded, then read.
        let bytes = borsh::to_vec(&delta).unwrap();
        let started = std::time::Instant::now();
        let decoded = borsh::from_slice::<PalwStateDeltaV2>(&bytes).unwrap();
        let _ = native_delta_evidence_v1(&decoded);
        extraction = (extraction.0 + started.elapsed(), extraction.1 + 1);
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
        executor_bond: exec_bond,
        executor_pubkey: exec_pk_kept,
        valid_seats,
        extraction,
        extras_edit,
    }
}

pub fn rules<'a>(l: &'a Life, state: &'a PalwChainStateV2, open: &'a BTreeSet<Hash64>) -> NativeFactRulesV1<'a> {
    NativeFactRulesV1 {
        state,
        params: &l.sp,
        canonical_work_daa: l.p.palw_canonical_work_daa(),
        quantum_maturity_daa: 120,
        claims_with_open_da: open,
    }
}

/// The same equivocation `dos_g2_conviction_takes_back` files against a `Final` floor claim: an `ExecutorEquivocation` carriage
/// that convicts the execution, accusing seat `accused`.
pub fn false_valid(l: &Life, accused: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    use kaspa_consensus_core::palw_offence_v1::{
        PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
        palw_offence_evidence_digest_v1,
    };
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: l.executor_bond.0,
        signature: Vec::new(),
    };
    let equivocation = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: l.executor_bond.0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context: kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor_profile(), 512, 256),
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    };
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: l.id,
        network_domain: h(NET),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 {
            claim: l.id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: accused,
            signed_daa: 0,
            signature: Vec::new(),
        },
        executor_pubkey: l.executor_pubkey.clone(),
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
