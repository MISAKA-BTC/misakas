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
    let t = terms();
    all.push((t, 895));
    let mut t = t;
    t.funding_purpose = RelayFundingPurposeV2::CaseC;
    t.target =
        RelayTargetV2::InitialCase { result: ResultId(d(20)), funding_scope: FundingScopeId(d(21)), consent: FundingConsentId(d(22)) };
    all.push((t, 1015));
    let mut t = t;
    t.funding_purpose = RelayFundingPurposeV2::ProverJoin;
    t.target = RelayTargetV2::InitialProver { prover: t.payer_bond, join: d(23) };
    all.push((t, 955));
    let mut t = t;
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
