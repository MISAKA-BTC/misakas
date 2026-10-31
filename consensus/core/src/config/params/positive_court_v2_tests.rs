use super::*;
use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
use crate::palw_lifecycle_objects_v2::{
    PalwLifecycleKindFencesV1, PalwLifecycleTxError, palw_lifecycle_objects_from_accepted_txs_in_context_v2,
    palw_positive_lifecycle_payload_v2, palw_positive_lifecycle_unjudged_v2, validate_palw_lifecycle_tx,
    validate_palw_lifecycle_tx_in_context_v1,
};

fn policy() -> PositiveCourtPolicyV2 {
    PositiveCourtPolicyV2 {
        version: 2,
        revision: 1,
        profile: CheckProfile::ExactStreamV1,
        derivation_id: Hash64::from_bytes(tir::exact_derivation_id().0),
        selector_id: Hash64::from_bytes(tir::greedy_selector_id().0),
        service_profile_id: Hash64::from_u64_word(1),
        funding_schedule_id: Hash64::from_u64_word(2),
        max_frame_bytes: 32 << 10,
        max_part_heap: 1 << 20,
        max_relations: 100,
        max_attempts: 2,
        max_all_attempt_bytes: 1 << 20,
        max_all_attempt_elements: 1_000_000,
        max_all_attempt_terms: 1_000_000,
        max_case_heap_bytes: 2 << 20,
        max_case_journal_bytes: 2 << 20,
        max_case_work: 4_000_000,
        max_retention_daa: 1000,
    }
}
fn fence(at: u64) -> PositiveCourtFenceV2 {
    PositiveCourtFenceV2 { activation: ForkActivation::new(at), policy: policy() }
}

#[test]
fn positive2_policy_root_matches_independent_python_fixed_schema_vector() {
    // Independently generated with Python struct.pack('<HHB') / '<QQQIQQQQQQQ',
    // the pinned A0 TIR IDs, and hashlib.blake2b(key=..., digest_size=64).
    let bytes = policy().root();
    let actual: String = bytes.as_byte_slice().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        actual,
        "bcd3c926459e75afcbb6d832e65cb3e8329a246c7044c310b843cdc21911b5640332b49ab32315d6f1c91a18212fb085975a17cfb69973d2a10951964014d56a"
    );
    assert_eq!(borsh::object_length(&policy()).unwrap(), 345);
    assert_eq!(policy(), PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(1)).policy);
}

#[test]
fn positive2_released_presets_are_absent_and_none_writes_no_fingerprint_field() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS] {
        assert!(p.palw_positive_court_v2.is_none());
        p.validate_palw_positive_court_v2().unwrap();
        assert!(p.palw_positive_court_v2_at(u64::MAX).is_none());
        let ids = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
        let mut copied = p.clone();
        copied.palw_positive_court_v2 = None;
        assert_eq!(ids, (copied.consensus_params_id(), copied.consensus_schedule_id(), copied.consensus_identity_id()));
        assert_eq!(p.fence_schedule_v1(), copied.fence_schedule_v1());
    }
}

#[test]
fn positive2_never_collapses_the_whole_profile_but_raw_diagnostics_commit_it() {
    let p = TESTNET_PARAMS;
    let mut never = p.clone();
    never.palw_positive_court_v2 = Some(fence(u64::MAX));
    never.validate_palw_positive_court_v2().unwrap();
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
    assert_eq!(p.fence_schedule_v1(), never.fence_schedule_v1());
    assert_ne!(p.consensus_params_id(), never.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), never.consensus_schedule_id());
    never.palw_positive_court_v2.as_mut().unwrap().policy.max_case_heap_bytes += 1;
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
}

#[test]
fn positive2_scheduled_profile_preserves_pre_fork_peer_identity_and_is_not_a_public_permit() {
    let p = TESTNET_PARAMS;
    let mut future = p.clone();
    future.palw_positive_court_v2 = Some(fence(9_000_000));
    assert_ne!(p.consensus_params_id(), future.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), future.consensus_schedule_id());
    assert_eq!(p.consensus_identity_id(), future.consensus_identity_id());
    assert!(future.fence_schedule_v1().contains(&9_000_000));
    assert!(future.palw_positive_court_v2_at(8_999_999).is_none());
    assert!(future.palw_positive_court_v2_at(9_000_000).is_some());
    assert!(future.validate_palw_positive_court_v2().is_err(), "finite shape is not native activation authority");
    let mut genesis = future.clone();
    genesis.palw_positive_court_v2.as_mut().unwrap().activation = ForkActivation::always();
    assert_ne!(p.consensus_identity_id(), genesis.consensus_identity_id());
    assert!(genesis.validate_palw_positive_court_v2().is_err());
}

#[test]
fn positive2_all_profile_fields_are_committed() {
    let before = policy();
    for field in 0..18 {
        let mut changed = before.clone();
        match field {
            0 => changed.version += 1,
            1 => changed.revision += 1,
            2 => changed.profile = CheckProfile::FullRelationDirectV1,
            3 => changed.derivation_id = Hash64::from_u64_word(99),
            4 => changed.selector_id = Hash64::from_u64_word(99),
            5 => changed.service_profile_id = Hash64::from_u64_word(99),
            6 => changed.funding_schedule_id = Hash64::from_u64_word(99),
            7 => changed.max_frame_bytes += 1,
            8 => changed.max_part_heap += 1,
            9 => changed.max_relations += 1,
            10 => changed.max_attempts += 1,
            11 => changed.max_all_attempt_bytes += 1,
            12 => changed.max_all_attempt_elements += 1,
            13 => changed.max_all_attempt_terms += 1,
            14 => changed.max_case_heap_bytes += 1,
            15 => changed.max_case_journal_bytes += 1,
            16 => changed.max_case_work += 1,
            _ => changed.max_retention_daa += 1,
        }
        assert_ne!(before.root(), changed.root(), "field {field}");
        let mut baseline = TESTNET_PARAMS;
        baseline.palw_positive_court_v2 = Some(fence(1000));
        let mut different = baseline.clone();
        different.palw_positive_court_v2.as_mut().unwrap().policy = changed;
        assert_ne!(baseline.consensus_params_id(), different.consensus_params_id(), "field {field}");
        assert_ne!(baseline.consensus_schedule_id(), different.consensus_schedule_id(), "field {field}");
        assert_eq!(baseline.consensus_identity_id(), different.consensus_identity_id(), "future field {field}");
        baseline.palw_positive_court_v2.as_mut().unwrap().activation = ForkActivation::always();
        different.palw_positive_court_v2.as_mut().unwrap().activation = ForkActivation::always();
        assert_ne!(baseline.consensus_identity_id(), different.consensus_identity_id(), "active field {field}");
    }
}

#[test]
fn positive2_rejects_unsupported_ids_zero_unbounded_overflow_and_inconsistent_caps() {
    for field in 0..12 {
        let mut p = policy();
        match field {
            0 => p.profile = CheckProfile::FullRelationDirectV1,
            1 => p.derivation_id = Hash64::default(),
            2 => p.selector_id = Hash64::default(),
            3 => p.service_profile_id = Hash64::default(),
            4 => p.funding_schedule_id = Hash64::default(),
            5 => p.max_frame_bytes = 300,
            6 => p.max_part_heap = p.max_case_heap_bytes + 1,
            7 => p.max_attempts = 0,
            8 => p.max_relations = u64::MAX - 1,
            9 => p.max_all_attempt_elements = u64::MAX - 1,
            10 => p.max_all_attempt_bytes = p.max_case_journal_bytes + 1,
            _ => p.max_retention_daa = u64::MAX,
        }
        assert!(p.validate_shape().is_err(), "field {field}");
        assert!(!PositiveCourtFenceV2 { activation: ForkActivation::always(), policy: p }.active_at(0));
    }
}

#[test]
fn positive2_ruleset_binds_only_the_fired_fork_and_current_policy() {
    let f = fence(1000);
    let fork = crate::Hash::from_u64_word(1);
    let id = f.ruleset_id(fork);
    assert_ne!(id, f.ruleset_id(crate::Hash::from_u64_word(2)));
    let mut other = f.clone();
    other.policy.max_case_work += 1;
    assert_ne!(id, other.ruleset_id(fork));
    other = f.clone();
    other.activation = ForkActivation::new(2000);
    assert_eq!(id, other.ruleset_id(fork), "future schedule is not signed as an already fired rule");
}

#[test]
fn positive2_raw_tag_is_opaque_before_decode_signature_resources_or_funding() {
    // Deliberately invalid/truncated body and hostile length bytes. The raw
    // pre-fence classification must not attempt to interpret any of them.
    let payload = [1, 0, 176, 255, 255, 255, 255];
    let fences = PalwLifecycleKindFencesV1::default().with_positive_court_reserved_v2(Some(ForkActivation::new(100)));
    assert!(palw_positive_lifecycle_payload_v2(&payload));
    assert!(palw_positive_lifecycle_unjudged_v2(&payload, &fences, 99));
    assert!(!palw_positive_lifecycle_unjudged_v2(&payload, &fences, 100));
    assert!(validate_palw_lifecycle_tx(&payload, true).is_ok());
    assert!(matches!(validate_palw_lifecycle_tx(&payload, false), Err(PalwLifecycleTxError::Undecodable)));
    assert!(validate_palw_lifecycle_tx_in_context_v1(&payload, &fences, 99).is_ok());
    let tx = crate::tx::Transaction::new(0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload.to_vec());
    let result = palw_lifecycle_objects_from_accepted_txs_in_context_v2(&[tx], &fences, 99);
    assert!(result.objects.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].1, "positive2 is undecodable below its owning fence");
}
