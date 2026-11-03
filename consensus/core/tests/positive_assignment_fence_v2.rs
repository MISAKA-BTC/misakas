//! Fixed pre-fence native baseline plus private selection checks; not public activation.
use kaspa_consensus_core::config::params::*;
use misaka_palw_positive_contract::{
    assignment::{AssignmentModeV2, policy::*},
    identity::Digest,
};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn policy(positive: &PositiveCourtFenceV2) -> AssignmentPolicyV2 {
    AssignmentPolicyV2 {
        version: 2,
        revision: 1,
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        positive_policy: Digest(positive.policy.root().as_bytes()),
        derivation_policy: canonical_assignment_derivation_id(),
        source_policy: d(11),
        assignment_fee_schedule: d(41),
        case_fee_schedule: d(42),
        reserve_schedule: d(12),
        physical_service_profile: d(43),
        cancellation_policy: d(18),
        live: AssignmentLiveCapsV2 { global: 10, per_poster: 2, per_worker: 2, per_prover: 2, per_class: 4 },
        max_frame_bytes: 32_000,
        max_program_bytes: 65_536,
        max_layout_entries: 1000,
        max_source_catalog_entries: 1000,
        max_role_candidates: 5,
        max_prover_joins: 2,
        max_history_frames: 100,
        max_history_bytes: 100_000,
        max_retained_bytes: 200_000,
        max_replay_bytes: 300_000,
        max_replay_work: 1_000_000,
        max_heap_bytes: 1_000_000,
        accept_window_daa: 100,
        result_window_daa: 100,
        build_window_daa: 100,
        check_window_daa: 100,
        fraud_tail_daa: 100,
        refund_window_daa: 100,
        retention_window_daa: 1000,
    }
}
fn selected(at: u64) -> Params {
    let mut p = TESTNET_PARAMS;
    let positive = PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(at));
    let assignment =
        PositiveAssignmentFenceV2 { activation: ForkActivation::new(at), policy: policy(&positive), native_preimages: None };
    p.palw_positive_court_v2 = Some(positive);
    p.palw_positive_assignment_v2 = Some(assignment);
    p
}
fn unchanged(name: &str, p: Params) {
    assert!(p.palw_positive_assignment_v2.is_none());
    p.validate_palw_positive_assignment_v2().unwrap();
    assert!(p.palw_positive_assignment_v2_at(u64::MAX).is_none());
    let (a, b, c) = match name {
        "mainnet" => (
            "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
            "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
            "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        ),
        "testnet" => (
            "3de9ba33581b12b8478a1d476a4e58889ed21ddb91dc24cc00706f49dd5b5b97",
            "7ea3296f36fc827898aa6560a1f71159652a12a7dd69105178564b9f7723d5d0",
            "22307d0c46b75110aea7967d16708fdf50a69f31450f7ed44d7309574395972a",
        ),
        "testnet11" => (
            "03b8564ece85e237b9252f390d0a73001bd67d02aee26418032a716e94057beb",
            "cb379fa021867dab89971e6f99a3469a6d044084d688113fa0b8e20f93af04d1",
            "929a3367d45faebd8c23927923abf10f91e6025cfb4b52a1ecb8768ade2d0e78",
        ),
        "simnet" => (
            "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
            "f981edc9bff1b71ae46abf030c0c56c40beafabeeae78d8435dd502ad6191f69",
            "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        ),
        "devnet" => (
            "80cbefe885c82921843f431a0d0ca85d93bec6d6cd74f0bf3413c2fd3827be52",
            "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
            "80cbefe885c82921843f431a0d0ca85d93bec6d6cd74f0bf3413c2fd3827be52",
        ),
        "testnet12" => (
            "edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189",
            "4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04",
            "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        ),
        _ => unreachable!(),
    };
    assert_eq!(p.consensus_params_id().to_string(), a, "{name} params");
    assert_eq!(p.consensus_schedule_id().to_string(), b, "{name} schedule");
    assert_eq!(p.consensus_identity_id().to_string(), c, "{name} identity");
}
#[test]
fn all_six_shipped_presets_match_actual_pre_assignment_fence_binary_fingerprints() {
    unchanged("mainnet", MAINNET_PARAMS);
    unchanged("testnet", TESTNET_PARAMS);
    unchanged("testnet11", TESTNET11_PARAMS);
    unchanged("simnet", SIMNET_PARAMS);
    unchanged("devnet", DEVNET_PARAMS);
    unchanged("testnet12", palw_t12_shipped_params());
}
#[test]
fn assigned_policy_is_some_only_future_aware_and_never_is_wholly_dormant() {
    let p = TESTNET_PARAMS;
    let mut future = selected(9_000_000);
    assert_ne!(future.consensus_params_id(), p.consensus_params_id());
    assert_ne!(future.consensus_schedule_id(), p.consensus_schedule_id());
    assert_eq!(future.consensus_identity_id(), p.consensus_identity_id());
    assert!(future.palw_positive_assignment_v2_at(8_999_999).is_none());
    assert!(future.palw_positive_assignment_v2_at(9_000_000).is_some());
    assert!(future.validate_palw_positive_assignment_v2().is_err());
    assert!(future.validate_palw_v2().is_err());
    let before = future.consensus_params_id();
    future.palw_positive_assignment_v2.as_mut().unwrap().policy.max_history_frames += 1;
    assert_ne!(future.consensus_params_id(), before);
    assert_eq!(future.consensus_identity_id(), p.consensus_identity_id());
    let mut never = p.clone();
    let mut dormant = selected(u64::MAX).palw_positive_assignment_v2.unwrap();
    dormant.policy.max_heap_bytes = 0;
    never.palw_positive_assignment_v2 = Some(dormant);
    assert_eq!(never.consensus_identity_id(), p.consensus_identity_id());
    assert_eq!(never.fence_schedule_v1(), p.fence_schedule_v1());
    assert_ne!(never.consensus_params_id(), p.consensus_params_id());
    never.validate_palw_positive_assignment_v2().unwrap();
}
#[test]
fn assignment_selection_requires_the_exact_active_positive_policy_and_r1_ruleset() {
    let mut p = selected(0);
    let fence = p.palw_positive_assignment_v2_at(0).unwrap();
    let scope = p.palw_positive_assignment_rule_scope_v2_at(0).unwrap();
    assert_eq!(fence.ruleset_id(scope.fired_rules()), p.palw_positive_court_v2.as_ref().unwrap().ruleset_id(scope.fired_rules()));
    let before = p.consensus_identity_id();
    p.palw_positive_assignment_v2.as_mut().unwrap().policy.physical_service_profile = d(99);
    assert_ne!(p.consensus_identity_id(), before);
    p.palw_positive_assignment_v2.as_mut().unwrap().policy.positive_policy = d(98);
    assert!(p.palw_positive_assignment_v2_at(0).is_none());
    assert!(p.validate_palw_positive_assignment_v2().is_err());
    p.palw_positive_court_v2 = None;
    assert!(p.palw_positive_assignment_v2_at(0).is_none());
    assert!(p.validate_palw_positive_assignment_v2().is_err());
}

#[test]
fn fired_assignment_rule_scope_commits_policy_contents_and_excludes_future_policies() {
    let mut p = selected(1000);
    p.palw_positive_court_v2.as_mut().unwrap().activation = ForkActivation::always();
    assert!(p.palw_positive_assignment_rule_scope_v2_at(999).is_none());
    let original = p.palw_positive_assignment_rule_scope_v2_at(1000).unwrap();
    // Advertised ForkIdV1 commits a height list, not companion policy contents.
    let advertised = kaspa_consensus_core::fork_id_v1::fork_id_v1(&p, 1000).fired;
    let mut changed = p.clone();
    changed.palw_positive_assignment_v2.as_mut().unwrap().policy.max_history_frames += 1;
    assert_eq!(kaspa_consensus_core::fork_id_v1::fork_id_v1(&changed, 1000).fired, advertised);
    let different = changed.palw_positive_assignment_rule_scope_v2_at(1000).unwrap();
    assert_ne!(different.fired_rules(), original.fired_rules());
    assert_ne!(different.ruleset(), original.ruleset());
    assert_ne!(different.assignment_policy(), original.assignment_policy());

    changed = p.clone();
    changed.palw_native_mining_v39 =
        Some(kaspa_consensus_core::palw_native_mining_v39::PalwNativeMiningFenceV39::unapproved_probe_at(
            ForkActivation::new(5000),
            [7; 64],
        ));
    assert_ne!(changed.consensus_params_id(), p.consensus_params_id());
    assert_eq!(changed.palw_positive_assignment_rule_scope_v2_at(1000).unwrap(), original);
    assert_ne!(changed.palw_positive_assignment_rule_scope_v2_at(5000).unwrap().fired_rules(), original.fired_rules());
}

#[test]
fn assigned_scope_passes_the_unchanged_r1_result_row_recipe_and_rejects_the_retired_candidate() {
    use misaka_palw_positive_contract::{identity::*, messages::*, rows::*};
    let p = selected(0);
    let scope = p.palw_positive_assignment_rule_scope_v2_at(0).unwrap();
    let key = [7; PUBKEY_BYTES];
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 0,
        accepted_blue_score: 0,
    };
    let result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: Digest(scope.ruleset().as_bytes()), branch: d(2) },
        job: d(3),
        execution: d(4),
        miner: principal(&key),
        registered_class: d(5),
        artifact: d(6),
        program: d(7),
        input: d(8),
        tokenizer: d(0),
        execution_rules: d(0),
        selector: d(9),
        generation_seed: [10; 32],
        temperature_q24: 0,
        prefill: 1,
        decode: 2,
        max_context: 3,
        output: d(11),
        dependencies: d(12),
        service_contract: d(13),
    };
    // Shape fixture only: these dummy signature/origin bytes grant no acceptance.
    let row = ResultRow {
        result,
        miner_key: key,
        miner_signature: [0; SIGNATURE_BYTES],
        origin,
        policy: scope.positive_policy(),
        fired_fork: scope.fired_rules_digest(),
        funding_scope: FundingScopeId(d(14)),
        dependencies: AcceptedMaterial { origin, digest: d(12) },
        role_assignment: AcceptedMaterial { origin, digest: d(15) },
    };
    NativeRowBody::Result(row.clone()).validate_shape().unwrap();
    let bytes = borsh::to_vec(&(scope.fired_rules_digest(), scope.positive_policy(), scope.assignment_policy())).unwrap();
    let retired = blake2b_simd::Params::new().hash_length(64).key(b"misaka/positive/assignment-ruleset/v2/r1").hash(&bytes);
    let mut wrong = row;
    wrong.result.scope.ruleset = Digest(retired.as_bytes().try_into().unwrap());
    assert!(NativeRowBody::Result(wrong).validate_shape().is_err());
}

fn native_preimages() -> AssignmentNativePreimagesV2 {
    use kaspa_consensus_core::palw_positive_assignment_funds_v2::*;
    use kaspa_consensus_core::palw_positive_assignment_resources_v2::*;
    AssignmentNativePreimagesV2 {
        source_policy: AssignmentSourcePolicyV2 {
            version: 2,
            revision: 1,
            responsibility: kaspa_consensus_core::palw_positive_assignment_resources_v2::AssignmentSourceResponsibilityV2::FeeOnlyV1,
            release: kaspa_consensus_core::palw_positive_assignment_resources_v2::AssignmentSourceReleaseV2::AfterSignedDutiesRetentionAndLiabilityV1,
            work: AssignmentWorkScheduleV2 {
                version: 2,
                revision: 1,
                per_codec_step: 1,
                per_hash_byte: 1,
                per_signature: 1,
                per_element: 1,
                per_term: 1,
                per_io_byte: 1,
                per_control: 1,
            },
            max_parameter_instances: 1024,
            max_requests: 100,
            max_responses: 100,
            max_history_events: 200,
            max_history_bytes: 1_000_000,
            max_intervals: 100,
            max_units_per_request: 100,
            max_native_body_bytes: 64_000,
        },
        case_fees: AssignmentCaseFeeScheduleV2 {
            version: 2,
            revision: 1,
            admission: 1,
            per_source_unit: 1,
            per_source_opening_byte: 1,
            per_checked_relation: 1,
            per_record_byte: 1,
            capital_per_retained_byte: 1,
        },
        cancellation: AssignmentCancellationPolicyV2 {
            version: 2,
            revision: 1,
            rule: AssignmentCancellationRuleV2::PreserveEarnedAndLiabilityV1,
        },
        assignment_fees: AssignmentFeeScheduleV2 {
            version: 2,
            revision: 1,
            admission: 1,
            per_signature: 1,
            per_admission_byte: 1,
            per_retained_byte_daa: 1,
            per_replay_byte: 1,
            per_utxo_write: 1,
            capital_per_retained_byte: 1,
        },
        physical: AssignmentPhysicalProfileV2 {
            version: 2,
            revision: 1,
            max_admission_bytes: 1_000_000,
            max_retained_bytes: 1_000_000,
            max_replay_bytes: 1_000_000,
            max_heap_bytes: 1_000_000,
            max_utxo_writes: 100,
            max_signatures: 100,
            max_retention_daa: 1000,
        },
    }
}
#[test]
fn assignment_native_preimages_are_complete_policy_bound_and_fingerprinted() {
    let mut p = selected(100);
    let preimages = native_preimages();
    let fence = p.palw_positive_assignment_v2.as_mut().unwrap();
    fence.policy.source_policy = preimages.source_policy.id();
    fence.policy.case_fee_schedule = preimages.case_fees.id();
    fence.policy.cancellation_policy = preimages.cancellation.id();
    fence.policy.assignment_fee_schedule = preimages.assignment_fees.id();
    fence.policy.reserve_schedule = preimages.assignment_fees.reserve_id();
    fence.policy.physical_service_profile = preimages.physical.id();
    preimages.validate(&fence.policy).unwrap();
    fence.native_preimages = Some(preimages.clone());
    let first = p.palw_positive_assignment_rule_scope_v2_at(100).unwrap();
    let mut changed = p.clone();
    changed.palw_positive_assignment_v2.as_mut().unwrap().native_preimages.as_mut().unwrap().assignment_fees.admission += 1;
    let after = changed.palw_positive_assignment_rule_scope_v2_at(100).unwrap();
    assert_ne!(first.fired_rules(), after.fired_rules());
    assert_ne!(first.ruleset(), after.ruleset());
    assert!(
        changed
            .palw_positive_assignment_v2
            .as_ref()
            .unwrap()
            .native_preimages
            .as_ref()
            .unwrap()
            .validate(&changed.palw_positive_assignment_v2.as_ref().unwrap().policy)
            .is_err()
    );
    assert!(p.validate_palw_positive_assignment_v2().is_err(), "complete prices still do not authorize public activation");
}
#[test]
fn scheduled_native_preimages_do_not_change_the_genesis_handshake_identity() {
    let mut p = selected(100);
    p.palw_positive_assignment_v2.as_mut().unwrap().native_preimages = Some(native_preimages());
    let mut other = p.clone();
    other.palw_positive_assignment_v2.as_mut().unwrap().native_preimages.as_mut().unwrap().physical.max_utxo_writes += 1;
    // Future companions do not alter the genesis handshake identity. The
    // raw scheduled-parameter fingerprint intentionally commits their bytes.
    assert_eq!(p.palw_positive_assignment_rule_scope_v2_at(99), None);
    assert_eq!(p.consensus_identity_id(), other.consensus_identity_id());
    assert_ne!(p.consensus_params_id(), other.consensus_params_id());
}

#[path = "../../../misaka/palw/positive-contract/tests/support/assignment.rs"]
mod assignment_data;

#[test]
fn signed_money_quote_is_stable_across_eligible_acceptance_clocks() {
    use kaspa_consensus_core::palw_positive_assignment_funds_v2::{AssignmentPhysicalUsageV2, quote_assignment_money_v2};
    let params = selected(100);
    let mut policy = params.palw_positive_assignment_v2.unwrap().policy;
    let preimages = native_preimages();
    policy.assignment_fee_schedule = preimages.assignment_fees.id();
    policy.reserve_schedule = preimages.assignment_fees.reserve_id();
    policy.physical_service_profile = preimages.physical.id();
    let mut terms = assignment_data::bundle().terms;
    terms.derivation_policy = policy.derivation_policy;
    terms.reserve_schedule = policy.reserve_schedule;
    terms.accept_before_daa = 200;
    terms.result_before_daa = 300;
    terms.reserve_until_daa = 1050;
    terms.max_assignment_fee = 1_000_000;
    terms.max_assignment_capital = 1_000_000;
    terms.max_assignment_locked = 3_000_000;
    let usage = AssignmentPhysicalUsageV2 {
        admission_bytes: 100,
        retained_bytes: 100,
        replay_bytes: 100,
        heap_bytes: 100,
        utxo_writes: 1,
        signatures: 3,
    };
    let quote = |now| quote_assignment_money_v2(&policy, &terms, &preimages.assignment_fees, &preimages.physical, usage, now);
    let first = quote(100).unwrap();
    assert_eq!(first.retention_daa, 950);
    for now in [101, 150, 199] {
        assert_eq!(quote(now).unwrap(), first);
    }
    assert!(quote(99).is_err());
    assert!(quote(200).is_err());
    // The original retention window can independently raise the earliest
    // eligible DAA; it is not an additive TTL or an earned-service claim.
    terms.reserve_until_daa = 1150;
    let quote = |now| quote_assignment_money_v2(&policy, &terms, &preimages.assignment_fees, &preimages.physical, usage, now);
    assert!(quote(149).is_err());
    assert_eq!(quote(150).unwrap().retention_daa, 1000);
    assert_eq!(quote(150).unwrap(), quote(199).unwrap());
}

#[path = "support/funding_relay.rs"]
mod funding_relay_fixture;
use funding_relay_fixture::relay_fence;

#[test]
fn relay_metadata_is_some_only_and_every_shipped_preset_remains_dormant() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS, palw_t12_shipped_params()] {
        assert!(p.palw_positive_funding_relay_v2.is_none());
        assert!(p.palw_positive_funding_relay_v2_at(u64::MAX).is_none());
        p.validate_palw_positive_funding_relay_v2().unwrap();
    }
    let original = selected(1000);
    let mut future = original.clone();
    future.palw_positive_funding_relay_v2 = Some(relay_fence(5000));
    assert_ne!(future.consensus_params_id(), original.consensus_params_id());
    assert_ne!(future.consensus_schedule_id(), original.consensus_schedule_id());
    assert_eq!(future.consensus_identity_id(), original.consensus_identity_id());
    assert_eq!(future.palw_positive_assignment_rule_scope_v2_at(1000), original.palw_positive_assignment_rule_scope_v2_at(1000));
    assert!(future.palw_positive_funding_relay_v2_at(4999).is_none());
    assert!(future.palw_positive_funding_relay_v2_at(5000).is_some());
    assert!(future.validate_palw_positive_funding_relay_v2().is_err());
    let malformed = [1, 0, 176, 2, 0, 1, 0, 14];
    let fences = future.palw_lifecycle_kind_fences_v1();
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{
        palw_positive_lifecycle_unjudged_v2, validate_palw_lifecycle_tx_in_context_v1,
    };
    assert!(palw_positive_lifecycle_unjudged_v2(&malformed, &fences, 4999));
    assert!(validate_palw_lifecycle_tx_in_context_v1(&malformed, &fences, 4999).is_ok());
    assert!(!palw_positive_lifecycle_unjudged_v2(&malformed, &fences, 5000));
    assert!(validate_palw_lifecycle_tx_in_context_v1(&malformed, &fences, 5000).is_err());
    let mut never = original.clone();
    let mut fence = relay_fence(u64::MAX);
    fence.policy.max_pending_global = 0;
    never.palw_positive_funding_relay_v2 = Some(fence);
    never.validate_palw_positive_funding_relay_v2().unwrap();
    assert_eq!(never.fence_schedule_v1(), original.fence_schedule_v1());
    assert_eq!(never.consensus_identity_id(), original.consensus_identity_id());
}
#[test]
fn relay_scope_binds_all_actual_preimages_and_discards_future_policies() {
    let mut p = selected(1000);
    p.palw_positive_funding_relay_v2 = Some(relay_fence(1000));
    let original = p.palw_positive_funding_relay_rule_scope_v2_at(1000).unwrap();
    for n in 0..3 {
        let mut changed = p.clone();
        let fence = changed.palw_positive_funding_relay_v2.as_mut().unwrap();
        match n {
            0 => fence.policy.max_account_continuations += 1,
            1 => {
                fence.fees.retirement += 1;
                fence.policy.metadata_fee_schedule = fence.fees.id();
            }
            _ => {
                fence.physical.max_retained_bytes += 1;
                fence.policy.physical_custody_profile = fence.physical.id();
            }
        }
        assert_ne!(changed.consensus_params_id(), p.consensus_params_id());
        assert_ne!(changed.palw_positive_funding_relay_rule_scope_v2_at(1000).unwrap().ruleset(), original.ruleset());
    }
    let mut changed = p.clone();
    changed.palw_native_mining_v39 =
        Some(kaspa_consensus_core::palw_native_mining_v39::PalwNativeMiningFenceV39::unapproved_probe_at(
            ForkActivation::new(5000),
            [7; 64],
        ));
    assert_eq!(changed.palw_positive_funding_relay_rule_scope_v2_at(1000), Some(original));
    assert_ne!(changed.palw_positive_funding_relay_rule_scope_v2_at(5000), Some(original));
}
#[test]
fn relay_selection_rejects_missing_assignment_and_substituted_physical_or_fee_preimage() {
    let mut p = selected(1000);
    p.palw_positive_funding_relay_v2 = Some(relay_fence(1000));
    p.palw_positive_funding_relay_v2.as_ref().unwrap().validate_preimages().unwrap();
    for n in 0..3 {
        let mut changed = p.clone();
        match n {
            0 => changed.palw_positive_assignment_v2 = None,
            1 => changed.palw_positive_funding_relay_v2.as_mut().unwrap().fees.retirement += 1,
            _ => changed.palw_positive_funding_relay_v2.as_mut().unwrap().physical.max_retained_bytes += 1,
        };
        assert!(changed.palw_positive_funding_relay_v2_at(1000).is_none());
        assert!(changed.validate_palw_positive_funding_relay_v2().is_err());
    }
}

#[test]
fn original_intake_profile_is_not_reselected_after_relay_activation_or_removal() {
    let mut p = selected(1000);
    p.palw_positive_funding_relay_v2 = Some(relay_fence(5000));
    let old = AssignmentFundingRelayProfileV2::capture_at_intake(&p, 1000).unwrap();
    assert_eq!(old, AssignmentFundingRelayProfileV2::CooperativeV1);
    assert_eq!(borsh::to_vec(&old).unwrap(), vec![0]);
    let captured = AssignmentFundingRelayProfileV2::capture_at_intake(&p, 5000).unwrap();
    assert!(captured.requires_relay());
    let original_scope = p.palw_positive_assignment_rule_scope_v2_at(5000).unwrap();
    let raw = borsh::to_vec(&captured).unwrap();
    assert_eq!(raw.len(), 433);
    assert_eq!(raw[0], 1);
    p.palw_positive_funding_relay_v2 = None;
    assert_eq!(AssignmentFundingRelayProfileV2::capture_at_intake(&p, 6000).unwrap(), old);
    let retained = borsh::from_slice::<AssignmentFundingRelayProfileV2>(&raw).unwrap();
    retained.validate_original_scope(original_scope.fired_rules_digest()).unwrap();
    assert!(retained.requires_relay());
    assert!(!old.requires_relay());
    assert_eq!(retained, captured);
    assert!(retained.validate_original_scope(d(99)).is_err());
}
#[test]
fn invalid_armed_relay_never_downgrades_new_intake_to_cooperative() {
    let mut p = selected(1000);
    p.palw_positive_funding_relay_v2 = Some(relay_fence(1000));
    p.palw_positive_funding_relay_v2.as_mut().unwrap().fees.retirement += 1;
    assert!(AssignmentFundingRelayProfileV2::capture_at_intake(&p, 1000).is_err());
    p.palw_positive_funding_relay_v2.as_mut().unwrap().activation = ForkActivation::new(5000);
    assert_eq!(AssignmentFundingRelayProfileV2::capture_at_intake(&p, 1000).unwrap(), AssignmentFundingRelayProfileV2::CooperativeV1);
    p.palw_positive_assignment_v2 = None;
    assert!(AssignmentFundingRelayProfileV2::capture_at_intake(&p, 6000).is_err());
    assert!(borsh::from_slice::<AssignmentFundingRelayProfileV2>(&[255]).is_err());
}
