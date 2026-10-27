//! Lane F02b (2026-10-26): the kernel-route slice's atomic court admission on the native fold (TransitionBuilder, journal).
//! Every schedule here is `KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1()` — lane VAL's PROVISIONAL ESTIMATES used as
//! TEST values, plus a test-only follow-up fan-out of 2. Production is typed-missing (`None`).
use super::super::palw_kernel_route_fold_v1::{kernel_court_admissions_v1, load_ledger, write_kernel_court_admissions_v1};
use super::*;
use crate::palw_kernel_court_admission_v1::{
    KernelCourtAdmissionScheduleV1, KernelCourtCapitalRowV1, PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1, kernel_court_packets_v1,
};
use misaka_palw_kernel::ledger::{ClaimBodyV1, ClaimRowV1, LedgerEventV1};

const COMMIT_DAA: u64 = 1_000;
const CLAIM: [u8; 64] = [0xC1; 64];

fn producer() -> PalwBondKeyV2 {
    bond_key(5)
}
fn test_only_estimate_extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        kernel_court_admission_v1: Some(KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1()),
        kernel_route: Some(route_extras()),
        ..Default::default()
    }
}
/// The slice's fence `palw_kernel_route_panel_one_v1` in force from DAA 0 (fixture only).
fn slice_params() -> PalwStateParamsV2 {
    params().with_kernel_route_panel_one_from_daa(Some(0))
}
fn route_extras() -> crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
    crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 { network_domain: h64(1), ruleset_digest: h64(2), ..Default::default() }
}
/// Producer 5 with `collateral`, a second bond 6 (a seat candidate), the kernel route.
fn parent(collateral: u64) -> PalwChainStateV2 {
    let (mut state, _) = apply_palw_transition_v2(
        &PalwChainStateV2::genesis(),
        &params(),
        &ctx(1, 10, 1),
        &[bond_registration_with_collateral(5, 7, 21, collateral), bond_registration_with_collateral(6, 8, 22, 1_000_000_000_000)],
        None,
    )
    .expect("the bonds register");
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    // The committed claim's row, as the object's flush would have written it before the admission rows.
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .rows
        .insert((misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&CLAIM).unwrap()), borsh::to_vec(&program_row()).unwrap());
    state
}
fn program_row() -> ClaimRowV1 {
    use misaka_palw_kernel::{evidence::*, job::KernelClaimV1, lifecycle::*};
    let header = EvidenceHeaderV1 {
        network_domain: [0; 64],
        ruleset_digest: [0; 64],
        class_binding_id: [0xCB; 64],
        program_root: [0; 64],
        artifact_root: [0; 64],
        plan_root: [0; 64],
    };
    let suite = SuiteParamsV1 {
        descriptor_digest: [0; 64],
        checker_suite_id: 0,
        challenge_policy_id: 0,
        repetitions: 0,
        target_bits: 0,
        field_bits: 0,
    };
    ClaimRowV1 {
        producer: [5; 64],
        class_binding_id: [0xCB; 64],
        job_id: [0x10; 64],
        body: ClaimBodyV1::Program {
            claim: KernelClaimV1 { job_id: [0x10; 64], producer_bond: [5; 64], generated: vec![1], evidence_root: [0; 64] },
            evidence: VerificationEvidenceV1 {
                version: 1,
                header,
                job_input_root: [0; 64],
                positions: 1,
                initial_state_root: [0; 64],
                final_state_root: [0; 64],
                trace_root: [0; 64],
                output_root: [0; 64],
                segments: Vec::new(),
                suite,
            },
            commitments: Vec::new(),
        },
        committed_daa: COMMIT_DAA,
        life: ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 30, challenge_window_daa: 30 }),
        reserved: 0,
        liability_until: None,
        convicted: false,
        rewarded: false,
        sealed_daa: COMMIT_DAA,
    }
}
fn capital_needed() -> u128 {
    let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
    // Upper bound on liable signers: the producer and at most one seat in this two-bond fixture.
    let signers: std::collections::BTreeSet<_> = [bond_key(5), bond_key(6)].into_iter().collect();
    kernel_court_packets_v1(Hash64::from_bytes(CLAIM), h64(1), h64(1), COMMIT_DAA, &signers, &s).unwrap().len() as u128
        * s.capital_per_packet_sompi as u128
}

/// Admit (or refuse) the committed claim on a builder over `state`; returns whether it was admitted.
fn admit(b: &mut TransitionBuilder<'_>) -> bool {
    let at = ctx(2, COMMIT_DAA, 2);
    let mut ledger = load_ledger(b, &at).unwrap();
    ledger.preview_mut_v1().claims.insert(CLAIM, program_row());
    let events = [LedgerEventV1::ClaimCommitted { claim: CLAIM }];
    match kernel_court_admissions_v1(b, &at, &ledger, &events, &producer()).unwrap() {
        Some(admissions) => {
            write_kernel_court_admissions_v1(b, admissions).unwrap();
            true
        }
        None => false,
    }
}

#[test]
fn f02b_kernel_admission_reserves_every_branch_and_its_capital_atomically() {
    let state = parent(1_000_000_000_000);
    let (p, e) = (slice_params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    let committed_before = b.committed_at(&producer(), COMMIT_DAA);
    let mark = b.entries.len();
    assert!(admit(&mut b), "admitted with enough free collateral");
    let book = b.state.court_service_state_v1().expect("book written").clone();
    let binding = &book.claims[&Hash64::from_bytes(CLAIM)];
    let rows = b.state.kernel_court_capital_rows_v1();
    assert_eq!(rows.len(), 1);
    let lock: KernelCourtCapitalRowV1 = rows[0];
    assert_eq!((lock.payer, lock.claim, lock.version), (producer(), Hash64::from_bytes(CLAIM), 1));
    assert!(b.state.slashable_locks.is_empty(), "the capital is not a slashable lock");
    let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
    assert_eq!(lock.amount, binding.packet_ids.len() as u128 * s.capital_per_packet_sompi as u128, "capital of every packet");
    assert_eq!(lock.hard_deadline_daa, COMMIT_DAA + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA);
    assert!(binding.packet_ids.len() as u64 >= 1 + (s.profile.packets_per_signer - 1) * s.follow_up_branches_per_turn);
    assert_eq!(b.committed_at(&producer(), COMMIT_DAA), committed_before + lock.amount, "capital is committed collateral");
    book.check_context(&b.state).unwrap_or_else(|e| panic!("kernel binding passes the context check: {e}"));
    // Reorg: undo restores the parent exactly; restart: replaying the journal reproduces the admitted state.
    let done = b.state.state_root();
    let mut back = b.state.clone();
    for entry in b.entries[mark..].iter().rev() {
        apply_delta_entry(&mut back, entry, true).unwrap();
    }
    assert_eq!(back.state_root(), state.state_root(), "reorg undoes the book and the lock");
    assert!(back.court_service_state_v1().is_none() && back.kernel_court_capital_rows_v1().is_empty());
    for entry in &b.entries[mark..] {
        apply_delta_entry(&mut back, entry, false).unwrap();
    }
    assert_eq!(back.state_root(), done, "replay after restart reproduces the admission");
    // A second admission of the same claim is refused (dropped), nothing more written.
    let root = b.state.state_root();
    assert!(!admit(&mut b));
    assert_eq!(b.state.state_root(), root);
}

#[test]
fn f02b_kernel_admission_refusal_changes_nothing() {
    // Free collateral one sompi short of the minimum possible requirement (the producer alone): refused, no write.
    let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
    let alone = (1 + (s.profile.packets_per_signer - 1) * s.follow_up_branches_per_turn) as u128 * s.capital_per_packet_sompi as u128;
    assert!(alone <= capital_needed());
    let state = parent((alone - 1) as u64);
    let (p, e) = (slice_params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    let root = b.state.state_root();
    assert!(!admit(&mut b), "insufficient free collateral refuses");
    assert_eq!(b.state.state_root(), root, "nothing changed");
    assert!(b.state.court_service_state_v1().is_none() && b.state.kernel_court_capital_rows_v1().is_empty(), "no book, no row");
    // Positive control on the same fixture with ample collateral is the test above. No schedule in the extras (the processor's
    // value below `palw_kernel_route_panel_one_v1`), no admission.
    let state = parent(1_000_000_000_000);
    let e = PalwTransitionExtrasV1 { kernel_route: Some(route_extras()), ..Default::default() };
    assert!(PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1.is_some(), "G14-FIX SPEC-1: the production schedule is pinned");
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(admit(&mut b));
    assert!(
        b.state.court_service_state_v1().is_none() && b.state.kernel_court_capital_rows_v1().is_empty(),
        "typed-missing schedule writes nothing"
    );
}

/// G14-FIX SPEC-1: the processor's schedule is the pinned production one only at/after `palw_kernel_route_panel_one_v1`; below it
/// (and with the fence unarmed) the extras get `None`, exactly as before the schedule was pinned.
#[test]
fn spec1_production_schedule_is_gated_on_the_slice_fence() {
    let k = PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1.expect("pinned");
    let p = params().with_kernel_route_panel_one_from_daa(Some(17_400));
    assert_eq!(p.kernel_court_admission_schedule_v1(0), None);
    assert_eq!(p.kernel_court_admission_schedule_v1(17_399), None);
    assert_eq!(p.kernel_court_admission_schedule_v1(17_400), Some(k));
    assert_eq!(params().kernel_court_admission_schedule_v1(u64::MAX), None, "unarmed: never");
}

/// Below the slice's fence nothing is admitted or written, even with a schedule.
#[test]
fn f02b_kernel_admission_is_dormant_below_the_slice_fence() {
    let state = parent(1_000_000_000_000);
    let (p, e) = (params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(admit(&mut b));
    assert!(b.state.court_service_state_v1().is_none() && b.state.kernel_court_capital_rows_v1().is_empty());
}

/// Held set: premise None -> held; premise adopted and every packet supplied -> not held; one missing supply -> held (S3c').
/// Release: the resolved book drops the binding and the capital row goes with it; at hard the row is released regardless.
#[test]
fn f02b_held_set_supply_check_and_capital_release() {
    use crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1 as Premise;
    use crate::palw_court_service_v1::ServiceCost;
    let state = parent(1_000_000_000_000);
    let (p, e) = (slice_params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(admit(&mut b));
    let claim = Hash64::from_bytes(CLAIM);
    const TEST_ONLY_PREMISE: Premise = Premise { version: 1, inclusion_delay_daa: 4 };
    let hard = COMMIT_DAA + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA;
    assert_eq!(b.state.kernel_court_default_held_v1(None).get(&CLAIM), Some(&hard), "no premise: held until hard");
    // Supply every DAA through hard, skipping the first assigned slot once.
    let mut book = b.state.court_service_state_v1().unwrap().clone();
    let skipped = *book.book.assignments.values().min().unwrap();
    for daa in COMMIT_DAA + 1..=hard {
        if daa != skipped {
            book.book.observe_block(daa, [1; 64], ServiceCost::default()).unwrap();
        }
    }
    let mut with_gap = b.state.clone();
    with_gap.staged_courts.insert(
        crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService,
        crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtService(Box::new(book).into()),
    );
    assert!(with_gap.kernel_court_default_held_v1(Some(&TEST_ONLY_PREMISE)).contains_key(&CLAIM), "missing supply: held (S3c')");
    let mut full = b.state.court_service_state_v1().unwrap().clone();
    for daa in COMMIT_DAA + 1..=hard {
        full.book.observe_block(daa, [1; 64], ServiceCost::default()).unwrap();
    }
    let mut supplied = b.state.clone();
    supplied.staged_courts.insert(
        crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService,
        crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtService(Box::new(full).into()),
    );
    assert!(supplied.kernel_court_default_held_v1(Some(&TEST_ONLY_PREMISE)).is_empty(), "all supplied: ordinary default");
    assert!(supplied.court_service_release_eligible_v1(&claim), "a fully decided kernel binding is release-eligible");
    // Capital release: the row stays before hard while bound, and goes at hard.
    let committed = b.committed_at(&producer(), COMMIT_DAA);
    super::super::palw_kernel_route_fold_v1::release_kernel_court_capital_v1(&mut b, &ctx(3, hard - 1, 3)).unwrap();
    assert_eq!(b.state.kernel_court_capital_rows_v1().len(), 1, "hard - 1: still reserved");
    super::super::palw_kernel_route_fold_v1::release_kernel_court_capital_v1(&mut b, &ctx(4, hard, 4)).unwrap();
    assert!(b.state.kernel_court_capital_rows_v1().is_empty(), "= hard: released");
    assert!(b.committed_at(&producer(), COMMIT_DAA) < committed, "the collateral is returned");
}

/// **G14 S3c''' (lane LEGACY, 2026-10-27): no producer default without an opportunity under the premise.** On the fully supplied
/// book of the test above, an open demand whose response deadline leaves no reserved, supplied unit `u` with
/// `u + inclusion_delay_daa <= deadline` inside its packet's interval is HELD (no slash); a demand due late enough that such a unit
/// exists is NOT held (the silent producer defaults); with the premise typed-missing the demanded claim is held.
#[test]
fn s3c_triple_prime_default_needs_an_opportunity_under_the_premise() {
    use crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1 as Premise;
    use crate::palw_court_service_v1::ServiceCost;
    use misaka_palw_kernel::ledger::{DemandRowV1, KernelLedgerV1};
    const TEST_ONLY_PREMISE: Premise = Premise { version: 1, inclusion_delay_daa: 4 };
    let state = parent(1_000_000_000_000);
    let (p, e) = (slice_params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(admit(&mut b));
    let hard = COMMIT_DAA + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA;
    let mut full = b.state.court_service_state_v1().unwrap().clone();
    for daa in COMMIT_DAA + 1..=hard {
        full.book.observe_block(daa, [1; 64], ServiceCost::default()).unwrap();
    }
    let first_unit = *full.book.assignments.values().min().unwrap();
    let mut supplied = b.state.clone();
    supplied.staged_courts.insert(
        crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService,
        crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtService(Box::new(full).into()),
    );
    let base: KernelLedgerV1 =
        (*supplied.kernel_route.as_ref().unwrap().ledger_native_projection_checked_v9().expect("projection")).clone();
    let with_demand = |deadline: u64| {
        let mut l = base.clone();
        l.demands.insert(
            (CLAIM, 0, 0),
            DemandRowV1 { demanders: vec![([7; 64], 1)], filed_daa: COMMIT_DAA, deadline_daa: deadline, last: None },
        );
        l
    };
    // Typed-missing premise: held.
    assert!(supplied.kernel_court_default_held_v2(None, &with_demand(hard)).contains_key(&CLAIM), "typed-missing: held");
    // No opportunity: the earliest reserved unit plus the inclusion delay is past the deadline.
    let early = first_unit + TEST_ONLY_PREMISE.inclusion_delay_daa - 1;
    assert_eq!(
        supplied.kernel_court_default_held_v2(Some(&TEST_ONLY_PREMISE), &with_demand(early)).get(&CLAIM),
        Some(&hard),
        "no opportunity under the premise: held to the hard deadline, no slash"
    );
    // Opportunity: the earliest unit fits the delay before the deadline -> not held (a silent producer defaults).
    let fits = first_unit + TEST_ONLY_PREMISE.inclusion_delay_daa;
    assert!(
        !supplied.kernel_court_default_held_v2(Some(&TEST_ONLY_PREMISE), &with_demand(fits)).contains_key(&CLAIM),
        "an opportunity existed: ordinary default"
    );
    // No open demand: nothing beyond the v1 set (all supplied: empty).
    assert!(supplied.kernel_court_default_held_v2(Some(&TEST_ONLY_PREMISE), &base).is_empty());
}
