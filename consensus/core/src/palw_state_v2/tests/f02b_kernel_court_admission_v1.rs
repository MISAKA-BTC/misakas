//! Lane F02b (2026-10-26): the kernel-route slice's atomic court admission on the native fold (TransitionBuilder, journal).
//! Every schedule here is `KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1()` — lane VAL's PROVISIONAL ESTIMATES used as
//! TEST values, plus a test-only follow-up fan-out of 2. Production is typed-missing (`None`).
use super::super::palw_kernel_route_fold_v1::{kernel_court_admissions_v1, load_ledger, write_kernel_court_admissions_v1};
use super::*;
use crate::palw_kernel_court_admission_v1::{
    KernelCourtAdmissionScheduleV1, PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1, kernel_court_funding_lock_id_v1, kernel_court_packets_v1,
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
    state.kernel_route.as_mut().unwrap().rows.insert(
        (misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&CLAIM).unwrap()),
        borsh::to_vec(&program_row()).unwrap(),
    );
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
    let suite = SuiteParamsV1 { descriptor_digest: [0; 64], checker_suite_id: 0, challenge_policy_id: 0, repetitions: 0, target_bits: 0, field_bits: 0 };
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
    let (p, e) = (params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    let committed_before = b.committed_at(&producer(), COMMIT_DAA);
    let mark = b.entries.len();
    assert!(admit(&mut b), "admitted with enough free collateral");
    let book = b.state.court_service_state_v1().expect("book written").clone();
    let binding = &book.claims[&Hash64::from_bytes(CLAIM)];
    let lock = b.state.slashable_locks[&(producer(), kernel_court_funding_lock_id_v1(Hash64::from_bytes(CLAIM)))];
    let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
    assert_eq!(lock.amount, binding.packet_ids.len() as u128 * s.capital_per_packet_sompi as u128, "capital of every packet");
    assert_eq!(lock.expiry_daa, COMMIT_DAA + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA);
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
    assert!(back.court_service_state_v1().is_none());
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
    let (p, e) = (params(), test_only_estimate_extras());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    let root = b.state.state_root();
    assert!(!admit(&mut b), "insufficient free collateral refuses");
    assert_eq!(b.state.state_root(), root, "nothing changed");
    assert!(b.state.court_service_state_v1().is_none() && b.state.slashable_locks.is_empty(), "no book, no lock");
    // Positive control on the same fixture with ample collateral is the test above. Production: no schedule, no admission.
    let state = parent(1_000_000_000_000);
    let e = PalwTransitionExtrasV1 { kernel_route: Some(route_extras()), ..Default::default() };
    assert!(PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1.is_none());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
    assert!(admit(&mut b));
    assert!(b.state.court_service_state_v1().is_none() && b.state.slashable_locks.is_empty(), "typed-missing schedule writes nothing");
}
