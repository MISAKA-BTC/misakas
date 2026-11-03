//! RFC-0046 fixed syntax/consent/price DATA, not native R custody or acceptance.
use libcrux_ml_dsa::ml_dsa_87::portable as mldsa;
use misaka_palw_positive_contract::{
    assignment::{AssignmentId, funding_relay::*, funding_relay_policy::*},
    identity::*,
    rows::*,
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn point(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: 1 }
}
fn terms() -> FundingRelayTermsV2 {
    FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: d(1),
        contract_ruleset: d(2),
        assignment: AssignmentId(d(3)),
        contract_anchor: NativeHash64([4; 64]),
        relay_ruleset: d(5),
        relay_policy: d(6),
        payer_bond: point(7),
        payer: PrincipalId(d(8)),
        refund_payload: d(9),
        funding_purpose: RelayFundingPurposeV2::PosterA,
        target: RelayTargetV2::InitialAssignment { bundle: d(10), poster_nonce: 0 },
        funding_outpoint: point(11),
        expected_amount: 1000,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: 100,
        funding_admission_before_daa: 200,
        custody_until_daa: 500,
        metadata_fee_schedule: d(12),
        physical_custody_profile: d(13),
        max_metadata_fee: 1_000_000_000_000,
    }
}
fn body(terms: FundingRelayTermsV2) -> FundingRelayRegistrationBodyV2 {
    FundingRelayRegistrationBodyV2 {
        version: 2,
        revision: 1,
        operation: 14,
        terms,
        purpose_signature: [17; SIGNATURE_BYTES],
        relay_signature: [18; SIGNATURE_BYTES],
    }
}
#[test]
fn exact_fixed_widths_and_unknown_trailing_truncated_data_are_bounded() {
    let mut all = Vec::new();
    let mut t = terms();
    all.push((t, 895));
    t.funding_purpose = RelayFundingPurposeV2::CaseC;
    t.target =
        RelayTargetV2::InitialCase { result: ResultId(d(20)), funding_scope: FundingScopeId(d(21)), consent: FundingConsentId(d(22)) };
    all.push((t, 1015));
    t.funding_purpose = RelayFundingPurposeV2::ProverJoin;
    t.target = RelayTargetV2::InitialProver { prover: t.payer_bond, join: d(23) };
    all.push((t, 955));
    t.target = RelayTargetV2::Continuation {
        original_funding: point(24),
        predecessor: point(25),
        parent_spend: t.funding_outpoint.transaction,
        original_contract: d(26),
    };
    all.push((t, 1087));
    for (terms, length) in all {
        let data = body(terms);
        let raw = borsh::to_vec(&data).unwrap();
        assert_eq!(borsh::to_vec(&terms).unwrap().len(), length);
        assert_eq!(raw.len(), 5 + length + 2 * SIGNATURE_BYTES);
        assert_eq!(FundingRelayRegistrationBodyV2::decode(&raw, 20_000).unwrap(), data);
        assert!(FundingRelayRegistrationBodyV2::decode(&raw, (raw.len() - 1) as u64).is_err());
        assert!(FundingRelayRegistrationBodyV2::decode(&raw[..raw.len() - 1], 20_000).is_err());
        let mut changed = raw.clone();
        changed.push(0);
        assert!(FundingRelayRegistrationBodyV2::decode(&changed, 20_000).is_err());
        let mut changed = raw.clone();
        changed[4] = 13;
        assert!(FundingRelayRegistrationBodyV2::decode(&changed, 20_000).is_err());
        let mut changed = raw;
        changed[590] = 255;
        assert!(FundingRelayRegistrationBodyV2::decode(&changed, 20_000).is_err());
    }
    assert_eq!(FUNDING_RELAY_MAX_BODY_BYTES_V2, 10_346);
}
#[test]
fn signature_material_and_terms_have_separate_acyclic_identities() {
    let keys = mldsa::generate_key_pair([31; 32]);
    let key = keys.verification_key.as_ref();
    let mut t = terms();
    t.payer = principal(key);
    let signature = mldsa::sign(&keys.signing_key, &t.id().0, FUNDING_RELAY_REGISTER_CONTEXT_V2, [32; 32]).unwrap();
    t.authenticate_purpose(key, signature.as_ref()).unwrap();
    let mut packet = body(t);
    packet.purpose_signature = *signature.as_ref();
    let terms_id = t.id();
    let material_id = packet.material_id();
    packet.relay_signature[0] ^= 1;
    assert_eq!(packet.terms.id(), terms_id);
    assert_ne!(packet.material_id(), material_id);
    let mut changed = t;
    changed.expected_amount += 1;
    assert!(changed.authenticate_purpose(key, signature.as_ref()).is_err());
    let signature = mldsa::sign(&keys.signing_key, &t.id().0, b"misaka-positive/assignment-poster/v2/r1", [32; 32]).unwrap();
    assert!(t.authenticate_purpose(key, signature.as_ref()).is_err());
}
#[test]
fn cross_purpose_continuation_clock_and_nonfinite_declarations_are_rejected() {
    let t = terms();
    t.validate_shape().unwrap();
    let mut changed = t;
    changed.funding_purpose = RelayFundingPurposeV2::CaseC;
    assert!(changed.validate_shape().is_err());
    let mut changed = t;
    changed.target = RelayTargetV2::InitialProver { prover: point(99), join: d(1) };
    changed.funding_purpose = RelayFundingPurposeV2::ProverJoin;
    assert!(changed.validate_shape().is_err());
    let mut changed = t;
    changed.target = RelayTargetV2::Continuation {
        original_funding: point(24),
        predecessor: point(25),
        parent_spend: NativeHash64([99; 64]),
        original_contract: d(26),
    };
    assert!(changed.validate_shape().is_err());
    for amount in [0, u64::MAX] {
        let mut changed = t;
        changed.expected_amount = amount;
        assert!(changed.validate_shape().is_err());
    }
    let mut changed = t;
    changed.custody_until_daa = changed.funding_admission_before_daa;
    assert!(changed.validate_shape().is_err());
}
fn fee() -> FundingRelayFeeScheduleV2 {
    FundingRelayFeeScheduleV2 {
        version: 2,
        revision: 1,
        admission: 1,
        per_signature: 1,
        per_frame_byte: 1,
        per_retained_byte_daa: 1,
        per_database_byte: 1,
        per_replay_read: 1,
        per_replay_work: 1,
        retirement: 1,
    }
}
fn physical() -> FundingRelayPhysicalProfileV2 {
    FundingRelayPhysicalProfileV2 {
        version: 2,
        revision: 1,
        max_frame_bytes: 64_000,
        max_decoded_bytes: 64_000,
        max_heap_bytes: 4_000_000,
        max_retained_bytes: 64_000,
        max_database_bytes: 64_000,
        max_batch_bytes: 128_000,
        max_replay_reads: 1000,
        max_replay_work: 1_000_000,
        max_signatures: 2,
    }
}
#[test]
fn finite_price_is_stable_checked_and_never_a_native_custody_receipt() {
    let fee = fee();
    let physical = physical();
    let policy = FundingRelayPolicyV2 {
        version: 2,
        revision: 1,
        max_pending_global: 100,
        max_pending_per_poster: 10,
        max_pending_per_prover: 10,
        max_account_candidates: 3,
        max_account_continuations: 5,
        max_stored_rows: 1000,
        max_declared_amount: 1_000_000,
        register_window_daa: 100,
        funding_window_daa: 300,
        custody_window_daa: 1000,
        rollback_margin_daa: 10,
        metadata_fee_schedule: fee.id(),
        physical_custody_profile: physical.id(),
    };
    let mut t = terms();
    t.metadata_fee_schedule = fee.id();
    t.physical_custody_profile = physical.id();
    t.relay_policy = policy.id();
    let usage = FundingRelayUsageV2 {
        frame_bytes: borsh::to_vec(&body(t)).unwrap().len() as u64,
        decoded_bytes: size_of::<FundingRelayRegistrationBodyV2>() as u64,
        heap_bytes: 100_000,
        retained_bytes: 32_000,
        database_bytes: 64_000,
        batch_bytes: 100_000,
        replay_reads: 5,
        replay_work: 100,
        signatures: 2,
    };
    let quote = fee.quote(&policy, &physical, &t, usage, 0).unwrap();
    assert_eq!(quote.retention_daa, 500);
    assert_eq!(quote.fee as u128, 1 + 2 + usage.frame_bytes as u128 + 32_000 * 500 + 64_000 + 5 + 100 + 1);
    assert_eq!(quote, fee.quote(&policy, &physical, &t, usage, 99).unwrap());
    assert!(fee.quote(&policy, &physical, &t, usage, 100).is_err());
    let mut short = usage;
    short.retained_bytes = 64_001;
    assert!(fee.quote(&policy, &physical, &t, short, 0).is_err());
    let mut too_low = t;
    too_low.max_metadata_fee = quote.fee - 1;
    assert!(fee.quote(&policy, &physical, &too_low, usage, 0).is_err());
    let mut bad = policy;
    bad.max_pending_global = 0;
    assert!(bad.validate().is_err());
    let mut expensive = fee;
    expensive.per_retained_byte_daa = u64::MAX - 1;
    let mut changed_policy = policy;
    changed_policy.metadata_fee_schedule = expensive.id();
    let mut changed_terms = t;
    changed_terms.metadata_fee_schedule = expensive.id();
    changed_terms.relay_policy = changed_policy.id();
    assert!(expensive.quote(&changed_policy, &physical, &changed_terms, usage, 0).is_err());
}

use misaka_palw_positive_contract::{assignment::funding_relay_rows::*, native::NativeRowKey};
fn row_origin(tx: NativeHash64, daa: u64) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([41; 64]),
        carrier_block: NativeHash64([42; 64]),
        carrier_transaction: tx,
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: daa,
        accepted_blue_score: daa,
    }
}
fn relay_pair(mut terms: FundingRelayTermsV2) -> (FundingRelayAuthorizationRowV2, FundingRelayCustodyRowV2) {
    let keys = mldsa::generate_key_pair([43; 32]);
    let key = *keys.verification_key.as_ref();
    terms.payer = principal(&key);
    let fees = fee();
    let physical = physical();
    let policy = FundingRelayPolicyV2 {
        version: 2,
        revision: 1,
        max_pending_global: 100,
        max_pending_per_poster: 10,
        max_pending_per_prover: 10,
        max_account_candidates: 3,
        max_account_continuations: 5,
        max_stored_rows: 1000,
        max_declared_amount: 1_000_000,
        register_window_daa: 100,
        funding_window_daa: 300,
        custody_window_daa: 1000,
        rollback_margin_daa: 10,
        metadata_fee_schedule: fees.id(),
        physical_custody_profile: physical.id(),
    };
    terms.relay_policy = policy.id();
    terms.metadata_fee_schedule = fees.id();
    terms.physical_custody_profile = physical.id();
    let usage = FundingRelayUsageV2 {
        frame_bytes: 10_346,
        decoded_bytes: 16_000,
        heap_bytes: 32_000,
        retained_bytes: 32_000,
        database_bytes: 32_000,
        batch_bytes: 32_000,
        replay_reads: 10,
        replay_work: 100,
        signatures: 2,
    };
    let quote = fees.quote(&policy, &physical, &terms, usage, 50).unwrap();

    let sig = mldsa::sign(&keys.signing_key, &terms.id().0, FUNDING_RELAY_REGISTER_CONTEXT_V2, [44; 32]).unwrap();
    let mut body = body(terms);
    body.purpose_signature = *sig.as_ref();
    let origin = row_origin(NativeHash64([45; 64]), 50);
    let material = AcceptedMaterial { origin, digest: body.material_id() };
    (
        FundingRelayAuthorizationRowV2 {
            terms,
            payer_key: key,
            purpose_signature: body.purpose_signature,
            origin,
            material,
            metadata_quote: AcceptedMaterial { origin, digest: quote.id() },
            metadata_charge: AcceptedMaterial { origin, digest: d(47) },
            policy,
            fees,
            physical,
            quote,
        },
        FundingRelayCustodyRowV2 {
            funding_outpoint: terms.funding_outpoint,
            purpose: terms.funding_purpose,
            terms: terms.id(),
            relay_signature: body.relay_signature,
            material,
            reservation: AcceptedMaterial { origin, digest: d(48) },
            retention_until_daa: terms.custody_until_daa,
            binding: None,
        },
    )
}
#[test]
fn two_fixed_rows_retain_literal_signature_package_without_future_binding() {
    let (auth, custody) = relay_pair(terms());
    custody.check_pair(&auth).unwrap();
    auth.terms.authenticate_purpose(&auth.payer_key, &auth.purpose_signature).unwrap();
    assert!(custody.binding.is_none());
    for field in 0..4 {
        let mut changed = auth.clone();
        match field {
            0 => changed.policy.max_pending_global += 1,
            1 => changed.fees.retirement += 1,
            2 => changed.physical.max_retained_bytes += 1,
            _ => changed.quote.fee += 1,
        };
        assert!(custody.check_pair(&changed).is_err());
    }

    for (value, kind) in
        [(NativeRowBody::FundingRelayAuthorization(auth.clone()), 19), (NativeRowBody::FundingRelayCustody(custody.clone()), 20)]
    {
        let key = value.key().encode();
        let row = NativeRowValue::new(value).unwrap();
        let raw = row.encode().unwrap();
        assert_eq!(key.len(), 74);
        assert_eq!(&key[..5], &[2, 0, 1, 0, kind]);
        assert_eq!(raw[4], kind);
        assert_eq!(NativeRowValue::decode(&key, &raw).unwrap(), row);
        let mut wrong = key;
        wrong[4] = if kind == 19 { 20 } else { 19 };
        assert!(NativeRowValue::decode(&wrong, &raw).is_err());
    }
    let mut changed = custody.clone();
    changed.relay_signature[0] ^= 1;
    assert!(changed.check_pair(&auth).is_err());
    let mut changed = auth.clone();
    changed.purpose_signature[0] ^= 1;
    assert!(custody.check_pair(&changed).is_err());
    let mut changed = custody;
    changed.material.origin.accepted_daa += 1;
    assert!(changed.check_pair(&auth).is_err());
}
#[test]
fn maximum_relay_rows_fit_exact_ceiling_with_continuation_and_binding() {
    let mut t = terms();
    t.target = RelayTargetV2::Continuation {
        original_funding: point(24),
        predecessor: point(25),
        parent_spend: t.funding_outpoint.transaction,
        original_contract: d(26),
    };
    let (auth, mut custody) = relay_pair(t);
    let origin = row_origin(t.funding_outpoint.transaction, 150);
    custody.binding = Some(FundingRelayBindingV2 {
        origin,
        original_funding: point(24),
        predecessor: Some(point(25)),
        original_contract: d(26),
        reservation: AcceptedMaterial { origin, digest: d(49) },
    });
    custody.check_pair(&auth).unwrap();
    assert_eq!(borsh::to_vec(&auth).unwrap().len(), 10038);
    assert_eq!(borsh::to_vec(&custody).unwrap().len(), 6026);
    for (row, len) in
        [(NativeRowBody::FundingRelayAuthorization(auth.clone()), 10043), (NativeRowBody::FundingRelayCustody(custody.clone()), 6031)]
    {
        let row = NativeRowValue::new(row).unwrap();
        let raw = row.encode().unwrap();
        assert_eq!(raw.len(), len);
        assert!(len <= MAX_NATIVE_ROW_BYTES);
        assert_eq!(NativeRowValue::decode(&row.body.key().encode(), &raw).unwrap(), row);
    }
    let binding = custody.binding.unwrap();
    for n in 0..4 {
        let mut changed = custody.clone();
        let b = changed.binding.as_mut().unwrap();
        match n {
            0 => b.predecessor = None,
            1 => b.original_funding = point(99),
            2 => b.original_contract = d(99),
            _ => b.origin.carrier_transaction = NativeHash64([99; 64]),
        };
        assert!(changed.check_pair(&auth).is_err());
    }
    let mut changed = custody;
    changed.binding =
        Some(FundingRelayBindingV2 { origin: row_origin(t.funding_outpoint.transaction, t.funding_admission_before_daa), ..binding });
    assert!(changed.check_pair(&auth).is_err());
}
#[test]
fn pending_and_current_row_shapes_preserve_original_exclusive_clocks() {
    let (auth, mut custody) = relay_pair(terms());
    let origin = row_origin(auth.terms.funding_outpoint.transaction, 150);
    custody.binding = Some(FundingRelayBindingV2 {
        origin,
        original_funding: auth.terms.funding_outpoint,
        predecessor: None,
        original_contract: d(50),
        reservation: AcceptedMaterial { origin, digest: d(51) },
    });
    custody.check_pair(&auth).unwrap();
    // Stored original acceptance remains valid shape after new-intake expiry;
    // no current-clock API deletes a live signature or unlocks its account.
    let mut invalid = auth.clone();
    invalid.origin.accepted_daa = auth.terms.register_before_daa;
    assert!(custody.check_pair(&invalid).is_err());
    let mut changed = custody.clone();
    changed.retention_until_daa -= 1;
    assert!(changed.check_pair(&auth).is_err());
    let mut changed = custody.clone();
    changed.binding.as_mut().unwrap().predecessor = Some(point(99));
    assert!(changed.check_pair(&auth).is_err());
    let key =
        NativeRowKey::FundingRelayCustody { funding: auth.terms.funding_outpoint, purpose: RelayFundingPurposeV2::CaseC }.encode();
    let raw = NativeRowValue::new(NativeRowBody::FundingRelayCustody(custody)).unwrap().encode().unwrap();
    assert!(NativeRowValue::decode(&key, &raw).is_err());
}

#[test]
fn registration_fee_floor_matches_literal_payload_and_pending_row_images() {
    let base = terms();
    let targets = [
        (RelayFundingPurposeV2::PosterA, base.target),
        (
            RelayFundingPurposeV2::CaseC,
            RelayTargetV2::InitialCase {
                result: ResultId(d(20)),
                funding_scope: FundingScopeId(d(21)),
                consent: FundingConsentId(d(22)),
            },
        ),
        (RelayFundingPurposeV2::ProverJoin, RelayTargetV2::InitialProver { prover: base.payer_bond, join: d(23) }),
        (
            RelayFundingPurposeV2::PosterA,
            RelayTargetV2::Continuation {
                original_funding: point(24),
                predecessor: point(25),
                parent_spend: base.funding_outpoint.transaction,
                original_contract: d(26),
            },
        ),
    ];
    for (funding_purpose, target) in targets {
        let (auth, custody) = relay_pair(FundingRelayTermsV2 { funding_purpose, target, ..base });
        let FundingRelayAuthorizationRowV2 { terms, policy, fees, physical, .. } = auth;
        let mut literal = body(terms);
        literal.purpose_signature = auth.purpose_signature;
        literal.relay_signature = custody.relay_signature;
        let frame_bytes = borsh::object_length(&literal).unwrap() as u64;
        let mut retained_bytes = frame_bytes + 3;
        for row in [NativeRowBody::FundingRelayAuthorization(auth), NativeRowBody::FundingRelayCustody(custody)] {
            retained_bytes += row.key().encode().len() as u64;
            retained_bytes += NativeRowValue::new(row).unwrap().encode().unwrap().len() as u64;
        }
        let database_bytes = retained_bytes + 66;
        let usage = FundingRelayUsageV2 {
            frame_bytes,
            decoded_bytes: size_of::<FundingRelayRegistrationBodyV2>() as u64,
            heap_bytes: 32_000,
            retained_bytes,
            database_bytes,
            batch_bytes: database_bytes,
            replay_reads: 1,
            replay_work: 1,
            signatures: 2,
        };
        let complete = fees.quote(&policy, &physical, &terms, usage, 50).unwrap();
        let floor = fees.minimum_registration_fee(&policy, &physical, &terms).unwrap();
        assert_eq!(floor + fees.per_replay_read + fees.per_replay_work, complete.fee);

        for undersized in [
            FundingRelayUsageV2 { frame_bytes: frame_bytes - 1, ..usage },
            FundingRelayUsageV2 { retained_bytes: retained_bytes - 1, ..usage },
            FundingRelayUsageV2 { database_bytes: database_bytes - 1, ..usage },
            FundingRelayUsageV2 { batch_bytes: database_bytes - 1, ..usage },
        ] {
            assert!(fees.quote(&policy, &physical, &terms, undersized, 50).is_err());
        }

        // A signed physical profile with room for everything except one
        // retained byte still cannot admit the package.
        let physical = FundingRelayPhysicalProfileV2 { max_retained_bytes: retained_bytes - 1, ..physical };
        let policy = FundingRelayPolicyV2 { physical_custody_profile: physical.id(), ..policy };
        let terms = FundingRelayTermsV2 { relay_policy: policy.id(), physical_custody_profile: physical.id(), ..terms };
        assert!(fees.minimum_registration_fee(&policy, &physical, &terms).is_err());
    }
}

#[test]
fn metadata_charge_requires_exact_debit_from_the_observed_collectible_fee() {
    let (auth, _) = relay_pair(terms());
    let charge = auth.quote.fee;
    let before_relay = charge + 13;
    let mandatory = 29;
    let earlier_rent = 7;
    let gross = before_relay + mandatory + earlier_rent;
    let partition = |credited_tip| FundingRelayCarrierFeesV2::new(gross, mandatory, before_relay, credited_tip).unwrap();

    let charged = auth.carrier_charge_material(partition(13)).unwrap();
    assert_eq!(charged.origin, auth.origin);
    assert!(auth.carrier_charge_material(partition(before_relay)).is_err());
    assert!(auth.carrier_charge_material(partition(12)).is_err());
    assert!(auth.carrier_charge_material(partition(14)).is_err());
    assert!(FundingRelayCarrierFeesV2::new(gross, mandatory, gross - mandatory + 1, 13).is_err());
    assert!(FundingRelayCarrierFeesV2::new(gross, gross + 1, 0, 0).is_err());
    assert!(FundingRelayCarrierFeesV2::new(gross, mandatory, before_relay, before_relay + 1).is_err());

    let mut other_branch = auth.clone();
    other_branch.origin.accepting_block = NativeHash64([91; 64]);
    other_branch.material.origin = other_branch.origin;
    other_branch.metadata_quote.origin = other_branch.origin;
    assert_ne!(charged.digest, other_branch.carrier_charge_material(partition(13)).unwrap().digest);

    let different_prior_fee = FundingRelayCarrierFeesV2::new(gross, mandatory + 1, before_relay, 13).unwrap();
    assert_ne!(charged.digest, auth.carrier_charge_material(different_prior_fee).unwrap().digest);
}

#[test]
fn relay_aux_scan_caps_cover_absent_probes_and_value_bytes_atomically() {
    let mut physical = physical();
    physical.max_replay_reads = 3;
    physical.max_replay_work = 300;
    let mut budget = FundingRelayAuxScanBudgetV2::new(&physical).unwrap();

    budget.inspect(74, 0).unwrap();
    assert!(budget.inspect(74, 153).is_err());
    budget.inspect(74, 78).unwrap();
    budget.inspect(74, 0).unwrap();
    assert!(budget.inspect(1, 0).is_err());

    physical.max_replay_work = 1000;
    physical.max_replay_reads = 1;
    let mut budget = FundingRelayAuxScanBudgetV2::new(&physical).unwrap();
    budget.inspect(74, 0).unwrap();
    assert!(budget.inspect(74, 0).is_err());
}
