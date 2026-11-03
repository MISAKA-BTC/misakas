//! Retained DATA fixtures only. These do not claim a native accepted origin,
//! physical custody, live funds, an executed journal or successful P approval.
use kaspa_consensus_core::{
    Hash64,
    config::params::{AssignmentNativePreimagesV2, ForkActivation, PositiveCourtFenceV2},
    palw_positive_assignment_funds_v2::*,
    palw_positive_assignment_material_v2::*,
    palw_positive_assignment_resources_v2::*,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    assignment::open_material::*,
    assignment::{consents::*, policy::*, rows::*, *},
    identity::{Digest, PUBKEY_BYTES, PrincipalId},
    messages::AggregateBudget,
    rows::{AcceptedMaterial, AcceptedOrigin, NativeHash64, NativeOutpoint},
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn origin() -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 1,
        event_ordinal: 0,
        accepted_daa: 10,
        accepted_blue_score: 11,
    }
}
fn reference(n: u8) -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(), digest: d(n) }
}
fn bond(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: 0 }
}

#[test]
fn actual_control_boundary_matches_borsh_and_envelope_extent_without_padding() {
    use kaspa_consensus_core::{
        palw_state_v2::AssignmentControlBoundaryV2,
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use misaka_palw_positive_contract::assignment::control_material::*;
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(h(21), 0), vec![], 0, 1)],
        vec![TransactionOutput::new(17, ScriptPublicKey::default())],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        vec![],
    );
    let mut point = origin();
    point.carrier_transaction = NativeHash64(tx.id().as_bytes());
    let boundary = AssignmentControlBoundaryV2::from_actual_data_v2(
        AssignmentId(d(9)),
        point,
        Hash64::from_u64_word(10),
        Hash64::from_u64_word(11),
        12,
        &[UtxoEntry::new(20, ScriptPublicKey::default(), 1, false)],
        3,
    )
    .unwrap();
    let entries = [UtxoEntry::new(20, ScriptPublicKey::default(), 1, false)];
    boundary.check_populated_data(&tx, &entries, 3).unwrap();
    assert!(boundary.check_populated_data(&tx, &entries, 2).is_err());
    assert!(boundary.check_populated_data(&tx, &[], 3).is_err());
    for changed in [
        UtxoEntry::new(21, ScriptPublicKey::default(), 1, false),
        UtxoEntry::new(20, ScriptPublicKey::default(), 2, false),
        UtxoEntry::new(20, ScriptPublicKey::default(), 1, true),
        UtxoEntry::new(20, ScriptPublicKey::from_vec(0, vec![1]), 1, false),
    ] {
        assert!(boundary.check_populated_data(&tx, &[changed], 3).is_err());
    }
    let mut stale = tx.clone();
    stale.outputs[0].value += 1;
    assert!(boundary.check_populated_data(&stale, &entries, 3).is_err());
    let inconsistent = AssignmentControlBoundaryV2::from_actual_data_v2(
        AssignmentId(d(9)),
        point,
        Hash64::from_u64_word(10),
        Hash64::from_u64_word(11),
        12,
        &entries,
        4,
    )
    .unwrap();
    assert!(inconsistent.check_populated_data(&tx, &entries, 4).is_err());
    let canonical = boundary.canonical_bytes().unwrap();
    assert_eq!(canonical.len(), 496);
    assert_eq!(canonical.as_slice(), borsh::to_vec(&boundary).unwrap());
    assert_eq!(AssignmentControlBoundaryV2::decode(&canonical).unwrap(), boundary);
    let inner = [2, 0, 1, 0, 12];
    let envelope = encode_assignment_control_v2(&canonical, &inner, 1024).unwrap();
    assert_eq!(envelope.len(), 516 + inner.len());
    let view = AssignmentControlViewV2::decode(&envelope, 1024).unwrap();
    assert_eq!(view.boundary(), &canonical);
    assert_eq!(view.inner(), inner);
    assert!(AssignmentControlViewV2::decode(&envelope, (envelope.len() - 1) as u64).is_err());
    let mut changed = boundary;
    changed.input_count = 0;
    assert!(changed.canonical_bytes().is_err());
}
#[path = "support/funding_relay.rs"]
mod funding_relay_fixture;
fn fixture() -> (AssignmentRow, AssignmentAdmissionMaterialV2, PalwJobContextV2) {
    fixture_with_relay(false)
}
fn fixture_with_relay(requires_relay: bool) -> (AssignmentRow, AssignmentAdmissionMaterialV2, PalwJobContextV2) {
    let mut positive = PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(1)).policy;
    let native = AssignmentNativePreimagesV2 {
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
            max_admission_bytes: 1 << 20,
            max_retained_bytes: 1 << 20,
            max_replay_bytes: 1 << 20,
            max_heap_bytes: 1 << 20,
            max_utxo_writes: 100,
            max_signatures: 100,
            max_retention_daa: 1000,
        },
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
            max_requests: 16,
            max_responses: 32,
            max_history_events: 64,
            max_history_bytes: 4096,
            max_intervals: 8,
            max_units_per_request: 8,
            max_native_body_bytes: 1024,
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
    };
    positive.service_profile_id = Hash64::from_bytes(native.source_policy.id().0);
    positive.funding_schedule_id = Hash64::from_bytes(native.case_fees.id().0);
    let policy = AssignmentPolicyV2 {
        version: 2,
        revision: 1,
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        positive_policy: Digest(positive.root().as_bytes()),
        derivation_policy: canonical_assignment_derivation_id(),
        source_policy: native.source_policy.id(),
        assignment_fee_schedule: native.assignment_fees.id(),
        case_fee_schedule: native.case_fees.id(),
        reserve_schedule: native.assignment_fees.reserve_id(),
        physical_service_profile: native.physical.id(),
        cancellation_policy: native.cancellation.id(),
        live: AssignmentLiveCapsV2 { global: 8, per_poster: 4, per_worker: 4, per_prover: 4, per_class: 4 },
        max_frame_bytes: 1 << 16,
        max_program_bytes: 1 << 16,
        max_layout_entries: 1024,
        max_source_catalog_entries: 1024,
        max_role_candidates: 4,
        max_prover_joins: 2,
        max_history_frames: 32,
        max_history_bytes: 1 << 20,
        max_retained_bytes: 1 << 20,
        max_replay_bytes: 1 << 20,
        max_replay_work: 1 << 20,
        max_heap_bytes: 1 << 20,
        accept_window_daa: 100,
        result_window_daa: 100,
        build_window_daa: 50,
        check_window_daa: 50,
        fraud_tail_daa: 50,
        refund_window_daa: 50,
        retention_window_daa: 500,
    };
    use kaspa_consensus_core::config::params::{AssignmentFundingRelayProfileV2, PositiveAssignmentFenceV2, TESTNET_PARAMS};
    let mut params = TESTNET_PARAMS;
    params.palw_positive_court_v2 = Some(PositiveCourtFenceV2 { activation: ForkActivation::new(0), policy: positive.clone() });
    params.palw_positive_assignment_v2 =
        Some(PositiveAssignmentFenceV2 { activation: ForkActivation::new(0), policy, native_preimages: Some(native.clone()) });
    if requires_relay {
        params.palw_positive_funding_relay_v2 = Some(funding_relay_fixture::relay_fence(0));
    }
    let scope = params.palw_positive_assignment_rule_scope_v2_at(10).unwrap();
    let funding_relay = AssignmentFundingRelayProfileV2::capture_at_intake(&params, 10).unwrap();
    let terms = AssignmentTermsV2 {
        version: 2,
        revision: 1,
        network: d(20),
        ruleset: Digest(scope.ruleset().as_bytes()),
        branch_anchor: NativeHash64([22; 64]),
        mode: policy.mode,
        registered_class: d(23),
        poster: bond(24),
        worker: bond(25),
        prover: bond(26),
        payer: PrincipalId(d(27)),
        refund_payload: d(28),
        poster_nonce: 0,
        derivation_policy: policy.derivation_policy,
        source_policy: policy.source_policy,
        reserve_schedule: policy.reserve_schedule,
        worker_reward: 100,
        max_assignment_fee: 10_000_000,
        max_assignment_capital: 100_000,
        max_assignment_locked: 20_000_000,
        accept_before_daa: 100,
        result_before_daa: 200,
        reserve_until_daa: 500,
    };
    let id = terms.id();
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: h(30),
        job_nullifier: h(31),
        assignment_id: h(0),
        execution_seed: [32; 32],
        model_profile_id: h(33),
        runtime_manifest_hash: h(34),
        runtime_class_id: h(35),
        shape_profile_id: Hash64::from_bytes(terms.registered_class.0),
        trace_scheme_id: h(36),
        cu_ruleset_id: h(0),
        tokenizer_id: h(0),
        prompt_token_ids_hash: h(37),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 1,
        max_context_tokens: 257,
    };
    let mut job = job;
    job.job_id = Hash64::from_bytes(id.0.0);
    let money = quote_assignment_money_v2(
        &policy,
        &terms,
        &native.assignment_fees,
        &native.physical,
        AssignmentPhysicalUsageV2 {
            admission_bytes: 100,
            retained_bytes: 8192,
            replay_bytes: 8192,
            heap_bytes: 8192,
            utxo_writes: 2,
            signatures: 3,
        },
        10,
    )
    .unwrap();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: id,
        worker: terms.worker,
        source_catalog: d(40),
        source_policy: terms.source_policy,
        budget: AggregateBudget {
            rounds: 10,
            unique_source_bytes: 10,
            transmitted_bytes: 20,
            additional_work: 10,
            retained_bytes: 10,
            retention_until: 500,
            fee: 20,
            witness_attempts: 2,
        },
        service_before_daa: 450,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: id,
        prover: terms.prover,
        positive_policy: policy.positive_policy,
        max_attempts: 2,
        max_build_work: 10,
        max_submitted_bytes: 10,
        max_retained_bytes: 10,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 20,
        cancellation_policy: policy.cancellation_policy,
    };
    let bundle = AssignmentAdmissionBundleV2 {
        terms,
        job_context: Digest(job.context_hash().as_bytes()),
        quote: money.id(),
        source_catalog: source.source_catalog,
        funding: AssignmentFundingAuthorizationV2 {
            assignment: id,
            quote: money.id(),
            payer: terms.payer,
            funding_output_index: 0,
            refund_payload: terms.refund_payload,
            max_fee: terms.max_assignment_fee,
            max_capital: terms.max_assignment_capital,
            max_reward: 100,
            max_locked: terms.max_assignment_locked,
            admission_expiry: 100,
        },
        future_case: FutureCaseFundingBoundsV2 {
            assignment: id,
            payer: terms.payer,
            refund_payload: terms.refund_payload,
            worker: terms.worker,
            max_case_fee: 40,
            max_case_capital: 60,
            max_case_locked: 100,
            case_admission_before_daa: 200,
            service_before_daa: 450,
        },
        source_consent: source.id(),
        prover_consent: prover.id(),
        cancellation_policy: policy.cancellation_policy,
    };
    let registered = AssignmentRegisteredDescriptorV2 {
        class: terms.registered_class,
        artifact: d(41),
        program: d(42),
        layout: d(43),
        registered_tokenizer: d(44),
        registration: reference(45),
    };
    let material = AssignmentAdmissionMaterialV2 {
        version: 2,
        revision: 1,
        assignment: id,
        unsigned_bundle: bundle.id(),
        origin: origin(),
        network: terms.network,
        ruleset: terms.ruleset,
        fired_rules: scope.fired_rules_digest(),
        branch_anchor: terms.branch_anchor,
        assignment_policy: policy,
        positive_policy: positive,
        native_preimages: native,
        funding_relay,
        certification_policy: PalwSecurityCertificationPolicyV1 {
            m: 1,
            k: 1,
            veto: 0,
            anchor_delay_spans: 2,
            attest_window_daa: 5,
            prosecution_window_daa: 19,
            certify_by_daa: 7,
            verifier_maturity_daa: 3,
            verifier_min_collateral_sompi: 1,
            draws_per_epoch: 4,
            epoch_daa: 11,
        },
        network_prompt_form: PalwPromptIdsFormV1::Flat,
        class_prompt_form: PalwPromptIdsFormV1::Flat,
        job: AssignmentJobDescriptorV2::from_native(&job).unwrap(),
        registered,
        source: AssignmentSourceQuoteDescriptorV2 {
            digest: bundle.source_catalog,
            units: 2,
            data_bytes: 10,
            opening_bytes: 20,
            max_unit_bytes: 10,
        },
        money,
        custody: AssignmentCustodyDescriptorV2 {
            reservation: reference(46),
            liability: reference(47),
            preimage_bytes: [assignment_custody_bytes_id_v2(&[1]), assignment_custody_bytes_id_v2(&[2])],
            retention_until_daa: 500,
            retained_bytes: 8192,
            replay_bytes: 8192,
            replay_work: 100,
            heap_bytes: 8192,
        },
    };
    let row = AssignmentRow {
        bundle,
        source_consent: source,
        prover_consent: prover,
        origin: origin(),
        worker_key: [0; PUBKEY_BYTES],
        positive_policy: policy.positive_policy,
        assignment_policy: policy.id(),
        fired_rules: scope.fired_rules_digest(),
        job: Digest(job.job_id.as_bytes()),
        input: Digest(job.prompt_token_ids_hash.as_bytes()),
        context: Digest(job.context_hash().as_bytes()),
        registered_metadata: registered.id(),
        status: AssignmentStatusV2::Open,
        derivation: reference(48),
        dependencies: AcceptedMaterial { origin: origin(), digest: terms.empty_dependencies_id() },
        reservation: material.custody.reservation,
        liability: material.custody.liability,
        last_event: origin(),
        admission_material: AcceptedMaterial { origin: origin(), digest: material.id() },
    };
    (row, material, job)
}
fn limits() -> AssignmentOpenLimitsV2 {
    AssignmentOpenLimitsV2::within_payload(16 << 10)
}
fn payload(m: &AssignmentAdmissionMaterialV2) -> Vec<u8> {
    misaka_palw_positive_contract::assignment::open_material::encode_assignment_open_v2(
        &borsh::to_vec(m).unwrap(),
        &[2, 0, 1, 0, 8],
        [&[1], &[2]],
        limits(),
    )
    .unwrap()
}
#[test]
fn frozen_original_policy_survives_future_policy_change_and_rejects_substitution() {
    let (row, material, _job) = fixture();
    material.check_row(&row).unwrap();
    let raw = payload(&material);
    let view = AssignmentAdmissionMaterialViewV2::decode(&row, &raw, &[2, 0, 1, 0, 8], limits()).unwrap();
    assert_eq!(view.material, material);
    let mut future = material.assignment_policy;
    future.check_window_daa += 1;
    assert_ne!(future.id(), material.assignment_policy.id());
    // Recovery uses the accepted original, not any later Params/policy value.
    view.material.check_row(&row).unwrap();
    let mut substituted = material.clone();
    substituted.assignment_policy = future;
    assert!(substituted.check_row(&row).is_err());
    let mut substituted = material.clone();
    substituted.native_preimages.assignment_fees.admission += 1;
    assert!(substituted.check_row(&row).is_err());
    let mut substituted = material.clone();
    substituted.positive_policy.max_case_work += 1;
    assert!(substituted.check_row(&row).is_err());
}
#[test]
fn custody_and_literal_transaction_cannot_be_replaced_or_missing() {
    let (row, material, _job) = fixture();
    let raw = payload(&material);
    assert!(AssignmentAdmissionMaterialViewV2::decode(&row, &raw, &[2, 0, 1, 0, 9], limits()).is_err());
    assert!(AssignmentOpenViewV2::decode(&[2, 0, 1, 0, 8], limits()).is_err());
    let mut hostile = raw.clone();
    hostile[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(AssignmentOpenViewV2::decode(&hostile, limits()).is_err());
    let mut wrong_magic = raw.clone();
    wrong_magic[0] ^= 1;
    assert!(AssignmentOpenViewV2::decode(&wrong_magic, limits()).is_err());
    let mut corrupt = raw.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(AssignmentAdmissionMaterialViewV2::decode(&row, &corrupt, &[2, 0, 1, 0, 8], limits()).is_err());
    assert!(AssignmentAdmissionMaterialViewV2::decode(&row, &raw[..raw.len() - 1], &[2, 0, 1, 0, 8], limits()).is_err());
    assert!(
        misaka_palw_positive_contract::assignment::open_material::encode_assignment_open_v2(
            &[1],
            &[2, 0, 1, 0, 8],
            [&[], &[2]],
            limits()
        )
        .is_err()
    );
    let mut premature = material;
    premature.custody.retention_until_daa = 499;
    let mut row_with_matching_pin = row;
    row_with_matching_pin.admission_material.digest = premature.id();
    assert!(premature.check_row(&row_with_matching_pin).is_err());
}
#[test]
fn bounded_fixed_codec_and_original_job_profile_are_checked() {
    let (row, material, job) = fixture();
    let raw = borsh::to_vec(&material).unwrap();
    assert!(raw.len() < 16_384);
    assert_eq!(AssignmentAdmissionMaterialV2::decode(&raw, raw.len() as u64).unwrap(), material);
    assert!(AssignmentAdmissionMaterialV2::decode(&raw, raw.len() as u64 - 1).is_err());
    let mut trailing = raw;
    trailing.push(0);
    assert!(AssignmentAdmissionMaterialV2::decode(&trailing, 16_384).is_err());
    material.job.check_native(&job).unwrap();
    assert_eq!(job.max_context_tokens, 257);
    assert_eq!(job.cu_ruleset_id, Hash64::default());
    let mut changed_job = job;
    changed_job.max_context_tokens -= 1;
    assert!(material.job.check_native(&changed_job).is_err());
    let mut changed = material;
    changed.job.network[63] = 1;
    assert!(changed.job.check_native(&changed_job).is_err());
    changed.check_row(&row).unwrap_err();
}

#[test]
fn required_relay_profile_is_retained_in_original_material_without_current_policy_fallback() {
    let (row, material, _) = fixture_with_relay(true);
    assert!(material.funding_relay.requires_relay());
    material.check_row(&row).unwrap();
    let raw = borsh::to_vec(&material).unwrap();
    let restored = AssignmentAdmissionMaterialV2::decode(&raw, 16 << 10).unwrap();
    assert_eq!(restored, material);
    restored.check_row(&row).unwrap();
    let mut downgraded = restored.clone();
    downgraded.funding_relay = kaspa_consensus_core::config::params::AssignmentFundingRelayProfileV2::CooperativeV1;
    assert_ne!(downgraded.id(), material.id());
    assert!(downgraded.check_row(&row).is_err());
    let mut changed = restored;
    let kaspa_consensus_core::config::params::AssignmentFundingRelayProfileV2::RequiresRelayV2 { fees, .. } =
        &mut changed.funding_relay
    else {
        panic!("required original profile")
    };
    fees.retirement += 1;
    assert!(changed.check_row(&row).is_err());
}

/// Framing DATA fixtures only: these five-byte bodies are not full signed
/// operations and are never represented as accepted native transactions.
#[test]
fn original_control_carriage_borrows_exact_body_and_rejects_detached_subject_or_origin() {
    use kaspa_consensus_core::palw_state_v2::AssignmentControlBoundaryV2;
    use misaka_palw_positive_contract::assignment::{control_material::*, journal::*};
    let boundary = AssignmentControlBoundaryV2 {
        version: 2,
        revision: 1,
        assignment: AssignmentId(d(9)),
        origin: origin(),
        selected_parent: NativeHash64([10; 64]),
        pre_event_root: d(11),
        before_delta_end: 12,
        populated_inputs: d(13),
        input_count: 1,
        fee: 3,
    };
    let bytes = boundary.canonical_bytes().unwrap();
    for (kind, op) in [
        (AssignmentJournalEventKindV2::JoinProver, 13),
        (AssignmentJournalEventKindV2::Cancel, 10),
        (AssignmentJournalEventKindV2::Expire, 11),
        (AssignmentJournalEventKindV2::BindResult, 9),
        (AssignmentJournalEventKindV2::Settlement, 12),
    ] {
        let body = [2, 0, 1, 0, op];
        let payload = encode_assignment_control_v2(&bytes, &body, 1024).unwrap();
        for (subject, point, valid) in [
            (boundary.assignment, boundary.origin, true),
            (AssignmentId(d(99)), boundary.origin, false),
            (boundary.assignment, AcceptedOrigin { accepted_tx_ordinal: 19, ..boundary.origin }, false),
        ] {
            let frame = assignment_frame_v2(subject, 1, d(8), kind, point, &payload, 2048).unwrap();
            let raw = encode_assignment_journal_record_v2(frame.header, &payload, 2048).unwrap();
            let record = AssignmentJournalRecordViewV2::decode(&raw, 2048).unwrap();
            let actual = AssignmentControlBoundaryV2::native_body_from_record_data_v2(record, 2048);
            if valid {
                let (resolved, native) = actual.unwrap();
                assert_eq!(resolved, boundary);
                assert_eq!(native, body);
                assert!(native.as_ptr() >= raw.as_ptr() && native.as_ptr() < raw.as_ptr().wrapping_add(raw.len()));
            } else {
                assert!(actual.is_err());
            }
        }
        let frame = assignment_frame_v2(boundary.assignment, 1, d(8), kind, boundary.origin, &body, 2048).unwrap();
        let legacy = encode_assignment_journal_record_v2(frame.header, &body, 2048).unwrap();
        let record = AssignmentJournalRecordViewV2::decode(&legacy, 2048).unwrap();
        assert!(AssignmentControlBoundaryV2::native_body_from_record_data_v2(record, 2048).is_err());
    }
    let (row, material, _) = fixture_with_relay(true);
    let open = payload(&material);
    let envelope = encode_assignment_control_v2(
        &AssignmentControlBoundaryV2 { assignment: row.id(), origin: row.origin, ..boundary }.canonical_bytes().unwrap(),
        &open,
        16 << 10,
    )
    .unwrap();
    let frame = assignment_frame_v2(row.id(), 0, d(0), AssignmentJournalEventKindV2::Open, row.origin, &envelope, 16 << 10).unwrap();
    let raw = encode_assignment_journal_record_v2(frame.header, &envelope, 16 << 10).unwrap();
    let record = AssignmentJournalRecordViewV2::decode(&raw, 16 << 10).unwrap();
    let (_, native) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(record, 16 << 10).unwrap();
    assert_eq!(native, &[2, 0, 1, 0, 8]);
}
