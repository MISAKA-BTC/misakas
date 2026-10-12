//! Public-page snapshot attacks and exact program/attempt binding, with generated weights.
use kaspa_consensus_core::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1};
use kaspa_consensus_core::palw_onboarding_v1::{
    ConformanceAttemptRowV1, KernelBindingRowV1, PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1,
    PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::{ConformanceCommitmentV1, OnboardingRecordV1, RootV1, SubjectKindV1};
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::ledger::{AuthV1, KernelRouteObjectV1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_sdk::onboarding_snapshot::{KernelRowsPageV1, KernelRowsSnapshotV1, MAX_SNAPSHOT_BYTES_V1};

fn fixture() -> (Hash64, KernelRowsPageV1) {
    fixture_for(misaka_palw_tir_sketch::fixture::dense_moe_v1(7), false)
}

fn fixture_for(fx: misaka_palw_tir_sketch::fixture::TirSketchFixtureV1, complete: bool) -> (Hash64, KernelRowsPageV1) {
    let z = Hash64::from_bytes([0; 64]);
    let policy = palw_kernel_route_policy_v1(z, z);
    let mut route = PalwKernelRouteStateV1::new(policy, None, Default::default());
    let mut ledger = route.ledger().unwrap();
    let program = fx.program.encode();
    let d = if complete { k2_tir_v2_descriptor() } else { k2_tir_v1_descriptor() };
    let plan = plan_for_tir_program_v1(&d, &fx.program, misaka_palw_kernel::public::program_root_v1(&program), 2).unwrap();
    let pc = ParamCommitmentsV1::of(&fx.params);
    ledger.sync_bond([7; 64], 100_000);
    ledger.attest_artifact(pc.root());
    ledger.begin_block(5).unwrap();
    ledger
        .apply_object(
            &KernelRouteObjectV1::RegisterClass {
                descriptor: d.digest(),
                program_bytes: program.clone(),
                plan: plan.clone(),
                param_commitments: pc.clone(),
            },
            &AuthV1 { signer_bond: [7; 64] },
        )
        .unwrap();
    let kernel = *ledger.classes.keys().next().unwrap();
    route.rows = ledger.to_rows();
    route.header.scalars.daa = 5;
    let class = Hash64::from_bytes([9; 64]);
    let key = borsh::to_vec(&class).unwrap();
    let sealed = if complete {
        kaspa_consensus_core::palw_opv_bootstrap_v1::palw_onboarding_complete_check_policy_v1()
    } else {
        kaspa_consensus_core::palw_conformance_evidence_v1::palw_onboarding_sealed_policy_v1()
    };
    let commitment = ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: [0; 64],
        ruleset_id: [0; 64],
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: class.as_bytes(),
        kernel_descriptor_id: d.digest(),
        challenge_policy_id: sealed.id(),
        artifact_root: [3; 64],
        program_root: plan.program_root,
        source_root: RootV1::Absent,
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [4; 64],
        verification_plan_root: plan.root(),
        constraint_root: RootV1::Absent,
        implementation_set_root: [5; 64],
        test_scope_root: [6; 64],
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [8; 64],
        commitment_object_id: None,
        canonical_commitment_position: Some(5),
    };
    let mut record = OnboardingRecordV1::new(sealed.retry_limit + 1);
    record.state = misaka_palw_challenge::OnboardingStateV1::ChallengePending;
    let attempt = ConformanceAttemptRowV1 {
        record,
        commitment,
        committed_daa: 5,
        challenge_epoch: 0,
        eligible_profiles: vec![],
        excluded_profiles: vec![class, Hash64::from_bytes(kernel)],
        evidence: None,
        last_end: None,
        refuters_judged: vec![],
    };
    let binding = KernelBindingRowV1 {
        kernel_class: Hash64::from_bytes(kernel),
        kernel_param_root: Hash64::from_bytes(pc.root()),
        plan_root: Hash64::from_bytes(plan.root()),
        challenge_policy_id: Hash64::from_bytes(sealed.id()),
        binder: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(7), 0)),
        bound_daa: 5,
    };
    route.aux.insert((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, key.clone()), borsh::to_vec(&attempt).unwrap());
    route.aux.insert((PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, key), borsh::to_vec(&binding).unwrap());
    page_of(class, &route)
}

fn page_of(class: Hash64, route: &PalwKernelRouteStateV1) -> (Hash64, KernelRowsPageV1) {
    let rows: Vec<_> = route.rows.iter().chain(route.aux.iter()).map(|((t, k), v)| (*t, k.clone(), v.clone())).collect();
    (
        class,
        KernelRowsPageV1 {
            tip_daa: 5,
            ledger_root: route.ledger_root(),
            aux_root: route.aux_root(),
            header: borsh::to_vec(&route.header).unwrap(),
            total_rows: rows.len() as u64,
            rows,
            next: None,
        },
    )
}

fn split(mut page: KernelRowsPageV1) -> (KernelRowsPageV1, KernelRowsPageV1) {
    let rest = page.rows.split_off(page.rows.len() / 2);
    let mut second = page.clone();
    second.rows = rest;
    second.next = None;
    let last = page.rows.last().unwrap();
    page.next = Some((last.0, last.1.clone()));
    (page, second)
}

#[test]
fn one_snapshot_rebuilds_the_registered_program_and_attempt_without_models_or_separate_final_reads() {
    let (class, page) = fixture();
    let (first, second) = split(page);
    let mut snapshot = KernelRowsSnapshotV1::default();
    assert!(snapshot.push(first).unwrap().is_some());
    assert!(snapshot.push(second).unwrap().is_none());
    let reads = snapshot.into_reads(class).unwrap();
    let attempt: ConformanceAttemptRowV1 = borsh::from_slice(&reads.attempt_row).unwrap();
    assert_eq!(attempt.commitment.program_root, misaka_palw_kernel::public::program_root_v1(&reads.program));
    assert!(reads.events.is_empty() && reads.sealed_sources.is_empty());
    assert_eq!(reads.tip_daa, 5);
    // Missing a candidate's statement cannot turn into the statement of some other class.
    let (_, page) = fixture();
    let mut snapshot = KernelRowsSnapshotV1::default();
    snapshot.push(page).unwrap();
    assert!(snapshot.into_reads(Hash64::from_bytes([10; 64])).is_err());
}

#[test]
fn tip_header_roots_and_count_cannot_change_between_pages_even_when_rows_are_unchanged() {
    let (_, page) = fixture();
    let (first, second) = split(page);
    for mutate in 0..5 {
        let mut snapshot = KernelRowsSnapshotV1::default();
        snapshot.push(first.clone()).unwrap();
        let mut bad = second.clone();
        match mutate {
            0 => bad.tip_daa += 1,
            1 => bad.header[0] ^= 1,
            2 => bad.ledger_root = Hash64::from_bytes([22; 64]),
            3 => bad.aux_root = Hash64::from_bytes([23; 64]),
            _ => bad.total_rows += 1,
        }
        assert_eq!(snapshot.push(bad).unwrap_err().code, "SNAPSHOT_MOVED");
        // The rejected page did not advance the cursor or contaminate the held snapshot.
        assert!(snapshot.push(second.clone()).unwrap().is_none());
    }
}

#[test]
fn repeated_cursors_duplicates_omitted_rows_and_unbounded_snapshots_are_refused() {
    let (_, page) = fixture();
    let (first, second) = split(page.clone());
    let mut s = KernelRowsSnapshotV1::default();
    s.push(first.clone()).unwrap();
    assert_eq!(s.push(first.clone()).unwrap_err().code, "SNAPSHOT_ORDER");
    assert!(s.push(second.clone()).unwrap().is_none());
    assert_eq!(s.push(second).unwrap_err().code, "SNAPSHOT_CLOSED");
    let mut bad = first.clone();
    bad.next.as_mut().unwrap().1.push(0);
    assert_eq!(KernelRowsSnapshotV1::default().push(bad).unwrap_err().code, "SNAPSHOT_CURSOR");
    let mut bad = first;
    bad.next = None;
    assert_eq!(KernelRowsSnapshotV1::default().push(bad).unwrap_err().code, "SNAPSHOT_INCOMPLETE");
    let mut bad = page.clone();
    bad.total_rows = u64::MAX;
    bad.rows.clear();
    assert_eq!(KernelRowsSnapshotV1::default().push(bad).unwrap_err().code, "SNAPSHOT_TOO_LARGE");
    let mut bad = page;
    bad.header = vec![0; (64 << 10) + 1];
    assert_eq!(KernelRowsSnapshotV1::default().push(bad).unwrap_err().code, "SNAPSHOT_TOO_LARGE");
    assert_eq!(MAX_SNAPSHOT_BYTES_V1, 128 << 20);
}

#[test]
fn incomplete_or_mutated_material_never_authenticates_to_the_served_roots() {
    let (class, page) = fixture();
    let (first, _) = split(page.clone());
    let mut s = KernelRowsSnapshotV1::default();
    s.push(first).unwrap();
    assert!(s.into_reads(class).is_err());
    let mut bad = page.clone();
    bad.rows.last_mut().unwrap().2[0] ^= 1;
    let mut s = KernelRowsSnapshotV1::default();
    s.push(bad).unwrap();
    assert_eq!(s.into_reads(class).err().unwrap().code, "ROWS_NOT_THE_CHAIN");
    let mut bad = page;
    bad.tip_daa = 4;
    let mut s = KernelRowsSnapshotV1::default();
    s.push(bad).unwrap();
    assert_eq!(s.into_reads(class).err().unwrap().code, "SNAPSHOT_CLOCK");
}

#[test]
fn an_authenticated_snapshot_cannot_substitute_the_candidate_program_plan_or_kernel_link() {
    let (class, page) = fixture();
    for fault in 0..6 {
        let mut route =
            PalwKernelRouteStateV1::from_served_rows_v1(&page.header, page.rows.clone(), &page.ledger_root, &page.aux_root).unwrap();
        let key = borsh::to_vec(&class).unwrap();
        if fault < 4 {
            let mut a = route.conformance_attempt_v1(&class).unwrap();
            match fault {
                0 => a.commitment.candidate_id[0] ^= 1,
                1 => a.commitment.program_root[0] ^= 1,
                2 => a.commitment.verification_plan_root[0] ^= 1,
                _ => a.commitment.kernel_descriptor_id[0] ^= 1,
            }
            route.aux.insert((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, key), borsh::to_vec(&a).unwrap());
        } else {
            let mut b = route.kernel_binding_v1(&class).unwrap();
            if fault == 4 {
                b.kernel_param_root = Hash64::from_bytes([88; 64]);
            } else {
                b.challenge_policy_id = Hash64::from_bytes([88; 64]);
            }
            route.aux.insert((PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, key), borsh::to_vec(&b).unwrap());
        }
        let (_, changed) = page_of(class, &route);
        let mut s = KernelRowsSnapshotV1::default();
        s.push(changed).unwrap();
        assert_eq!(s.into_reads(class).err().unwrap().code, "SNAPSHOT_BINDING");
    }
}

#[test]
fn complete_check_file_replay_matches_the_committed_policy_and_refuses_other_models() {
    use kaspa_consensus_core::palw_onboarding_v1::PostedEvidenceRowV1;
    use misaka_palw_challenge::OnboardingStateV1 as S;
    use misaka_palw_sdk::onboarding_chain::{PublicOnboardingReadsV1, fresh_verify_complete_check_file_v1};

    let fx = misaka_palw_tir_sketch::fixture::wide128_v1(7);
    let (class, page) = fixture_for(fx.clone(), true);
    let path = std::env::temp_dir().join(format!("onboarding-complete-{}.palwtir", std::process::id()));
    misaka_palw_tir_artifact::write_container_v1(&path, &fx.program, vec![], [0; 64], String::new(), &mut |j, l| {
        Ok(fx.params.tensors[&(j, l)].to_le_bytes())
    })
    .unwrap();
    let root = misaka_palw_sdk::tir_stream::palw_tir_inventory_root_of_file_v1(&path).unwrap().0;
    let mut route = PalwKernelRouteStateV1::from_served_rows_v1(&page.header, page.rows, &page.ledger_root, &page.aux_root).unwrap();
    let key = borsh::to_vec(&class).unwrap();
    let mut attempt = route.conformance_attempt_v1(&class).unwrap();
    attempt.commitment.artifact_root = root.as_bytes();
    route.aux.insert((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, key.clone()), borsh::to_vec(&attempt).unwrap());
    let (_, page) = page_of(class, &route);
    let mut snapshot = KernelRowsSnapshotV1::default();
    snapshot.push(page.clone()).unwrap();
    let PublicOnboardingReadsV1::CompleteCheck(mut reads) = snapshot.into_onboarding_reads_v1(class).unwrap() else {
        panic!("the committed policy selects a complete check, with no invented beacon");
    };
    let fresh = fresh_verify_complete_check_file_v1(&reads, &path).unwrap();
    assert!(fresh.honest_passes && fresh.agrees && !fresh.chain_says_passed);
    assert_eq!(fresh.chain_post_id, None);
    attempt.evidence = Some(PostedEvidenceRowV1 {
        evidence_id: Hash64::from_bytes(fresh.honest_post_id),
        seed: Hash64::from_bytes([0; 64]),
        beacon_output: Hash64::from_bytes([0; 64]),
        lock_position: 5,
        posted_daa: 5,
        window_end_daa: 5,
    });
    attempt.record.state = S::G14Eligible;
    reads.attempt_row = borsh::to_vec(&attempt).unwrap();
    assert!(fresh_verify_complete_check_file_v1(&reads, &path).unwrap().agrees);
    attempt.evidence.as_mut().unwrap().evidence_id = Hash64::from_bytes([66; 64]);
    reads.attempt_row = borsh::to_vec(&attempt).unwrap();
    assert!(!fresh_verify_complete_check_file_v1(&reads, &path).unwrap().agrees, "a forged passing post is contradicted");

    // A file with the same program and different registered weights is refused, not accepted
    // because its metadata calls it the same model.
    let other = misaka_palw_tir_sketch::fixture::wide128_v1(8);
    misaka_palw_tir_artifact::write_container_v1(&path, &other.program, vec![], [0; 64], "same model".into(), &mut |j, l| {
        Ok(other.params.tensors[&(j, l)].to_le_bytes())
    })
    .unwrap();
    assert_eq!(fresh_verify_complete_check_file_v1(&reads, &path).unwrap_err().code, "ARTIFACT_NOT_REGISTERED");
    // A substituted program is refused before any execution of that file's tensor domain.
    let other = misaka_palw_tir_sketch::fixture::wide_v1(8);
    misaka_palw_tir_artifact::write_container_v1(&path, &other.program, vec![], [0; 64], String::new(), &mut |j, l| {
        Ok(other.params.tensors[&(j, l)].to_le_bytes())
    })
    .unwrap();
    assert_eq!(fresh_verify_complete_check_file_v1(&reads, &path).unwrap_err().code, "ARTIFACT_PROGRAM_MISMATCH");
    std::fs::remove_file(path).unwrap();
    let mut snapshot = KernelRowsSnapshotV1::default();
    snapshot.push(page).unwrap();
    assert_eq!(snapshot.into_reads(class).err().unwrap().code, "COMPLETE_CHECK");
}

#[test]
fn a_history_program_cannot_disguise_itself_as_a_complete_bootstrap_check() {
    let (class, page) = fixture();
    let mut route = PalwKernelRouteStateV1::from_served_rows_v1(&page.header, page.rows, &page.ledger_root, &page.aux_root).unwrap();
    let mut attempt = route.conformance_attempt_v1(&class).unwrap();
    attempt.commitment.challenge_policy_id =
        kaspa_consensus_core::palw_opv_bootstrap_v1::palw_onboarding_complete_check_policy_v1().id();
    let mut binding = route.kernel_binding_v1(&class).unwrap();
    binding.challenge_policy_id = Hash64::from_bytes(attempt.commitment.challenge_policy_id);
    route.aux.insert((PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, borsh::to_vec(&class).unwrap()), borsh::to_vec(&binding).unwrap());
    route
        .aux
        .insert((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, borsh::to_vec(&class).unwrap()), borsh::to_vec(&attempt).unwrap());
    let (_, page) = page_of(class, &route);
    let mut snapshot = KernelRowsSnapshotV1::default();
    snapshot.push(page).unwrap();
    assert_eq!(snapshot.into_onboarding_reads_v1(class).err().unwrap().code, "NOT_COMPLETELY_CHECKABLE");
}
