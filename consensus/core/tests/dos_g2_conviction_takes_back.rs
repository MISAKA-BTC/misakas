//! **2026-09-24 DoS audit, fixes #7 and #8 — what a conviction takes back, through the REAL fold.**
//!
//! Before these fixes no test folded a `PanelFalseValid` conviction against a claim that had
//! already reached `Final`: `consume_objective_offence` slashed the seat's lock and recorded the
//! execution root, and the claim kept everything its `Final` had counted — its `safe_weight` in
//! fork choice, the class's probe pass, the version's usage — and any execution tickets its work
//! had already been minted kept their permits.
//!
//! Every step below is testnet-12's own `Params`, its own genesis fold and `apply_palw_transition_v7`
//! (`dos_l5_common::fold`). A junk floor attempt is bound, licensed by an all-Valid quorum and swept
//! to `Final`; then one Valid signer is convicted. Two things the fixture cannot reach through blocks
//! are written through the carriage, as `dos_repro_1` does: the floor's registry lifecycle row (a
//! span boundary with an open execution lane writes it) and, for #7, a minted execution schedule (the
//! first block of span `n + 2` writes it). Everything MEASURED is the fold.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_g2_conviction_takes_back -- --nocapture --test-threads=2

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
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &valid_seats) }],
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
        bond_outpoint: accused.0,
        signature: Vec::new(),
    };
    let equivocation = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: accused.0,
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

/// **#8: a conviction after `Final` reverses the weight, the probe pass and the usage the `Final`
/// counted — and the reversal reloads and reverts.**
///
/// Fails without the fix: `consume_objective_offence` never touched the claim, so the phase stays
/// `Final` and `safe_weight` keeps the claim's contribution (checked by disabling
/// `reverse_convicted_final`; the first assertion below is the one that trips).
#[test]
fn dos_g2_a_conviction_after_final_takes_back_what_the_final_counted() {
    let f = drive_to_final(true);
    let (p, sp) = (&f.p, &f.sp);
    let claim = f.at_final.claim(&f.claim_id).unwrap().clone();
    let contribution = f.at_final.safe_weight() - f.before_final.safe_weight();
    assert!(contribution > 0, "the Final added its weight ({} -> {})", f.before_final.safe_weight(), f.at_final.safe_weight());
    let row_final = f.at_final.model_lifecycle(&f.floor).expect("the seeded floor row").clone();
    let row_before = f.before_final.model_lifecycle(&f.floor).expect("the seeded floor row").clone();
    let usage_final = attempt_attempts(&f.at_final);
    reloads(p, &f.at_final);

    let conviction_daa = f.daa + 5;
    let accused = f.valid_seats[0];
    let collateral_before = f.at_final.bond(&accused).unwrap().collateral;
    let (s, delta) =
        step(p, sp, &f.at_final, conviction_daa, &[false_valid(&f, accused)], PalwBlockWorkV3::None, Hash64::default(), 0, true);
    let after = s.claim(&f.claim_id).expect("the claim is still held — voided, not retired").clone();
    println!("\n=== #8: a conviction after Final (t12, real fold) ===");
    println!("claim {} Final at DAA {}; convicted at {conviction_daa}", f.claim_id, f.final_daa);
    println!(
        "safe_weight: before Final {} -> Final {} -> convicted {}",
        f.before_final.safe_weight(),
        f.at_final.safe_weight(),
        s.safe_weight()
    );
    println!(
        "floor probes passed/failed: before Final {}/{} -> Final {}/{} -> convicted {}/{}",
        row_before.probes_passed,
        row_before.probes_failed,
        row_final.probes_passed,
        row_final.probes_failed,
        s.model_lifecycle(&f.floor).unwrap().probes_passed,
        s.model_lifecycle(&f.floor).unwrap().probes_failed
    );
    println!("version usage (attempt claims): Final {usage_final} -> convicted {}", attempt_attempts(&s));
    assert!(
        matches!(after.phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == conviction_daa),
        "the conviction voids the Final claim: {:?}",
        after.phase
    );
    assert_eq!(s.safe_weight(), f.before_final.safe_weight(), "the claim's whole contribution leaves safe_weight");
    assert!(s.bond(&accused).unwrap().collateral < collateral_before, "the seat is still slashed");
    let row = s.model_lifecycle(&f.floor).unwrap();
    assert_eq!(row_final.probes_passed, row_before.probes_passed + 1, "the Final noted a pass");
    assert_eq!(row.probes_passed, row_before.probes_passed, "the pass is withdrawn");
    assert_eq!(row.probes_passed_this_span, row_before.probes_passed_this_span, "from the span's count too (same span)");
    assert_eq!(row.probes_failed, row_final.probes_failed + 1, "and a failure noted, as void_claim notes one for CourtFraud");
    assert_eq!(usage_final, attempt_attempts(&f.before_final), "usage is counted at acceptance, not at Final");
    assert!(usage_final >= 1, "the attempt counted on the floor's founding version");
    assert_eq!(attempt_attempts(&s), usage_final - 1, "the version's usage is uncounted");
    // The loader accepts what the fold wrote: a voided claim weighs nothing, and safe_weight is
    // the Final claims' sum again (equality or bound, whichever this network runs).
    reloads(p, &s);
    // A reorg that unwinds the conviction restores the Final exactly.
    let reverted = revert_delta_v2(&s, &delta, sp).expect("the conviction's delta reverts");
    assert_eq!(reverted.state_root(), f.at_final.state_root(), "revert restores the Final state root");
    assert_eq!(reverted.claim(&f.claim_id).unwrap().phase, claim.phase);
    assert_eq!(reverted.safe_weight(), f.at_final.safe_weight());

    // A second Valid signer convicted on the same claim takes nothing back twice.
    let (s2, _) =
        step(p, sp, &s, conviction_daa + 1, &[false_valid(&f, f.valid_seats[1])], PalwBlockWorkV3::None, Hash64::default(), 0, true);
    assert_eq!(s2.safe_weight(), s.safe_weight(), "the second conviction finds a voided claim and reverses nothing");
    assert_eq!(s2.model_lifecycle(&f.floor).unwrap().probes_failed, row.probes_failed, "no second failure");
    reloads(p, &s2);
}

/// **Below the fence a conviction after `Final` changes nothing about the claim** — the fold every
/// other network runs, byte for byte: the phase stays `Final` and the weight stays in.
#[test]
fn dos_g2_below_the_fence_a_conviction_after_final_leaves_the_final() {
    let f = drive_to_final(false);
    let (p, sp) = (&f.p, &f.sp);
    let accused = f.valid_seats[0];
    let (s, _) = step(p, sp, &f.at_final, f.daa + 5, &[false_valid(&f, accused)], PalwBlockWorkV3::None, Hash64::default(), 0, false);
    assert!(matches!(s.claim(&f.claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "dormant: the phase does not move");
    assert_eq!(s.safe_weight(), f.at_final.safe_weight(), "dormant: the weight stays");
    assert!(s.bond(&accused).unwrap().collateral < f.at_final.bond(&accused).unwrap().collateral, "the slash still lands");
}

/// A schedule minted from `finals`, as the first block of its span writes it.
fn minted(span: u64, finals: &[PalwExecFinalV1]) -> PalwExecScheduleV1 {
    let seed = h(0x5EED_0007);
    let quanta = palw_execution_mint_quanta_bounded_v1(
        finals,
        seed,
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        10_000,
        0,
        &Default::default(),
        1 << 16,
    );
    let domains = palw_execution_schedule_snapshot_v1(span, finals).domains;
    PalwExecScheduleV1 { span_index: span, seed, domains, finals: finals.to_vec(), quanta }
}

fn permitted_rounds(schedule: &PalwExecScheduleV1, bond: PalwBondKeyV2) -> usize {
    schedule
        .quanta
        .iter()
        .filter(|q| palw_execution_permits_v1(schedule, q.scheduled_round, 1).iter().any(|permit| permit.bond == bond))
        .count()
}

/// **#7: the conviction reaches the schedule already minted.** A schedule whose tickets were minted
/// from the convicted execution (and from an honest one) sits in state when the conviction lands;
/// past the fence the convicted work's tickets leave it, the honest ones keep their rounds, the
/// rewrite reloads, and a revert of the conviction puts every ticket back.
///
/// Fails without the fix: the schedule row is untouched by the conviction and every convicted
/// ticket is still a permit (`dos_l3_forfeiture_does_not_reach_a_minted_schedule`'s defect record).
#[test]
fn dos_g2_a_conviction_prunes_the_minted_schedule_and_a_revert_restores_it() {
    let f = drive_to_final(true);
    let (p, sp) = (&f.p, &f.sp);
    let convicted_bond = bond_key(0xC0);
    let honest_bond = bond_key(0x0C);
    let convicted = PalwExecFinalV1 {
        domain: h(0xD1),
        bond: convicted_bond,
        operator_id: h(0x0B1),
        claim_id: f.claim_id,
        execution_root: f.execution_root,
        credit: 4 * PALW_EXECUTION_QUANTUM_V1,
    };
    let honest = PalwExecFinalV1 {
        domain: h(0xD2),
        bond: honest_bond,
        operator_id: h(0x0B2),
        claim_id: h(0x4011_E57),
        execution_root: h(0x4011_E57_0000),
        credit: 3 * PALW_EXECUTION_QUANTUM_V1,
    };
    let span = 3u64;
    let schedule = minted(span, &[convicted, honest]);
    assert_eq!(permitted_rounds(&schedule, convicted_bond), 4, "four tickets of the convicted work are permits before");
    assert_eq!(permitted_rounds(&schedule, honest_bond), 3);
    // And the snapshot the next span will be seeded from, taken before the conviction too.
    let pending = palw_execution_schedule_snapshot_v1(span + 1, &[convicted, honest]);
    let mut c = PalwStateCarriageV2::from_state(&f.at_final);
    c.round_schedules.insert(span, schedule.clone());
    c.round_pending.insert(span + 1, pending.clone());
    let s0 = rebuild(p, c);

    let (s, delta) =
        step(p, sp, &s0, f.daa + 5, &[false_valid(&f, f.valid_seats[0])], PalwBlockWorkV3::None, Hash64::default(), 0, true);
    assert!(s.palw_execution_root_is_forfeited_v1(&f.execution_root), "t12 records the convicted root (ADR-0151 armed)");
    let pruned = s.round_schedule(span).expect("the span keeps its schedule");
    println!("\n=== #7: a conviction vs a minted schedule (t12, real fold) ===");
    println!(
        "quanta {} -> {}; convicted-bond permits {} -> {}; honest-bond permits {} -> {}",
        schedule.quanta.len(),
        pruned.quanta.len(),
        permitted_rounds(&schedule, convicted_bond),
        permitted_rounds(pruned, convicted_bond),
        permitted_rounds(&schedule, honest_bond),
        permitted_rounds(pruned, honest_bond)
    );
    assert_eq!(permitted_rounds(pruned, convicted_bond), 0, "no ticket of the convicted work is a permit after the conviction");
    assert!(pruned.quanta.iter().all(|q| q.final_id != f.claim_id), "its tickets left the schedule");
    assert!(pruned.finals.iter().all(|x| x.execution_root != f.execution_root), "and its Final left the list");
    assert!(pruned.domains.iter().all(|d| d.bonds.iter().all(|b| b.bond != convicted_bond)), "and the lottery's domains");
    let honest_before: Vec<_> = schedule.quanta.iter().filter(|q| q.final_id == honest.claim_id).copied().collect();
    let honest_after: Vec<_> = pruned.quanta.iter().filter(|q| q.final_id == honest.claim_id).copied().collect();
    assert_eq!(honest_after, honest_before, "the honest tickets keep their rounds");
    let pending_after = s.round_pending_snapshot().expect("the pending snapshot keeps the honest work");
    assert_eq!(pending_after, &palw_execution_schedule_snapshot_v1(span + 1, &[honest]), "re-taken without the convicted Final");
    reloads(p, &s);
    let reverted = revert_delta_v2(&s, &delta, sp).expect("reverts");
    assert_eq!(reverted.round_schedule(span), Some(&schedule), "a revert restores the minted schedule");
    assert_eq!(reverted.round_pending_snapshot(), Some(&pending), "and the pending snapshot");
    assert_eq!(reverted.state_root(), s0.state_root());

    // Below the fence the schedule is untouched.
    let (dormant, _) =
        step(p, sp, &s0, f.daa + 5, &[false_valid(&f, f.valid_seats[0])], PalwBlockWorkV3::None, Hash64::default(), 0, false);
    assert_eq!(dormant.round_schedule(span), Some(&schedule), "dormant: the conviction does not reach the schedule");
}

/// **#7, the lottery half: pruning the LAST ticket must not hand the permits back through the
/// ADR-0125 domain lottery.** `palw_execution_permits_v1` draws from `domains` whenever `quanta`
/// is empty; a schedule whose only work is convicted must grant nothing after the prune.
#[test]
fn dos_g2_pruning_every_ticket_does_not_reopen_the_lottery() {
    let f = drive_to_final(true);
    let (p, sp) = (&f.p, &f.sp);
    let convicted_bond = bond_key(0xC0);
    let convicted = PalwExecFinalV1 {
        domain: h(0xD1),
        bond: convicted_bond,
        operator_id: h(0x0B1),
        claim_id: f.claim_id,
        execution_root: f.execution_root,
        credit: 2 * PALW_EXECUTION_QUANTUM_V1,
    };
    let span = 3u64;
    let schedule = minted(span, &[convicted]);
    let mut c = PalwStateCarriageV2::from_state(&f.at_final);
    c.round_schedules.insert(span, schedule.clone());
    c.round_pending.insert(span + 1, palw_execution_schedule_snapshot_v1(span + 1, &[convicted]));
    let s0 = rebuild(p, c);
    let (s, _) = step(p, sp, &s0, f.daa + 5, &[false_valid(&f, f.valid_seats[0])], PalwBlockWorkV3::None, Hash64::default(), 0, true);
    let pruned = s.round_schedule(span).unwrap();
    assert!(pruned.quanta.is_empty() && pruned.domains.is_empty(), "no tickets and no domains remain");
    let granted: usize = (10_000u64..10_000 + 70_000).step_by(97).map(|round| palw_execution_permits_v1(pruned, round, 3).len()).sum();
    assert_eq!(granted, 0, "an empty schedule grants no lottery permit");
    assert!(
        s.round_pending_snapshot().is_none(),
        "a pending snapshot the conviction emptied is dropped, as rotation drops one with no domain"
    );
    reloads(p, &s);
}
