//! Fixed body and structural pair DATA. These tests do not claim canonical R
//! acceptance, metadata burn, custody, a future output or payment entitlement.
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    palw_positive_funding_relay_v2::*,
    palw_positive_relay_signature_v2::FundingRelaySignatureTemplateV2,
    palw_positive_rows_v2::restore::validate_positive_aux_restore_v2,
    tx::TransactionOutpoint,
};
use libcrux_ml_dsa::ml_dsa_87::portable as mldsa;
use misaka_palw_positive_contract::{
    assignment::{AssignmentId, funding_relay::*, funding_relay_policy::*, funding_relay_rows::*},
    identity::*,
    native::KERNEL_AUX_TABLE,
    rows::*,
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn fixture() -> (FundingRelayRegistrationBodyV2, [u8; PUBKEY_BYTES]) {
    let keys = mldsa::generate_key_pair([51; 32]);
    let key = *keys.verification_key.as_ref();
    let point = NativeOutpoint { transaction: NativeHash64([52; 64]), index: 1 };
    let mut terms = FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: d(1),
        contract_ruleset: d(2),
        assignment: AssignmentId(d(3)),
        contract_anchor: NativeHash64([4; 64]),
        relay_ruleset: d(5),
        relay_policy: d(6),
        payer_bond: NativeOutpoint { transaction: NativeHash64([7; 64]), index: 0 },
        payer: principal(&key),
        refund_payload: Digest(kaspa_hashes::blake2b_512_address_payload(&key).as_bytes()),
        funding_purpose: RelayFundingPurposeV2::PosterA,
        target: RelayTargetV2::InitialAssignment { bundle: d(9), poster_nonce: 0 },
        funding_outpoint: point,
        expected_amount: 1000,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: 100,
        funding_admission_before_daa: 200,
        custody_until_daa: 500,
        metadata_fee_schedule: d(10),
        physical_custody_profile: d(11),
        max_metadata_fee: 1_000_000_000,
    };
    let (fees, physical, policy) = metadata_preimages();
    terms.relay_policy = policy.id();
    terms.metadata_fee_schedule = fees.id();
    terms.physical_custody_profile = physical.id();
    let template = FundingRelaySignatureTemplateV2::new(
        terms.assignment,
        TransactionOutpoint::new(Hash64::from_bytes(point.transaction.0), point.index),
        terms.expected_amount,
        key,
    )
    .unwrap();
    let purpose = mldsa::sign(&keys.signing_key, &terms.id().0, FUNDING_RELAY_REGISTER_CONTEXT_V2, [53; 32]).unwrap();
    let relay = template.sign(&keys.signing_key, [54; 32]).unwrap();
    (
        FundingRelayRegistrationBodyV2 {
            version: 2,
            revision: 1,
            operation: 14,
            terms,
            purpose_signature: *purpose.as_ref(),
            relay_signature: relay,
        },
        key,
    )
}
#[test]
fn exact_body_and_real_typed_owner_enforce_both_frame_and_construction_limits() {
    let (body, key) = fixture();
    let raw = borsh::to_vec(&body).unwrap();
    let q = quote_funding_relay_body_v2(&raw, raw.len() as u64).unwrap();
    assert_eq!(q.encoded_bytes, raw.len() as u64);
    assert_eq!(q.signatures, 2);
    assert!(q.scan_steps > 0);
    assert_eq!(q.decoded_inline_bytes, std::mem::size_of::<FundingRelayRegistrationBodyV2>() as u64);
    assert!(quote_funding_relay_body_v2(&raw, (raw.len() - 1) as u64).is_err());
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 20, database_bytes: 1 << 20 }));
    assert!(ReservedFundingRelayBodyV2::decode(&raw, raw.len() as u64, q.logical_decode_requests - 1).is_err());
    let owned = ReservedFundingRelayBodyV2::decode(&raw, raw.len() as u64, q.logical_decode_requests).unwrap();
    assert_eq!(owned.body(), &body);
    assert_eq!(owned.quote(), q);
    drop(owned);

    verify_funding_relay_package_signatures_v2(&body, &key, 32_000).unwrap();
    assert!(verify_funding_relay_package_signatures_v2(&body, &key, 1).is_err());
    let mut wrong = body.clone();
    wrong.purpose_signature[0] ^= 1;
    assert!(verify_funding_relay_package_signatures_v2(&wrong, &key, 32_000).is_err());
    let mut wrong = body;
    wrong.relay_signature[0] ^= 1;
    assert!(verify_funding_relay_package_signatures_v2(&wrong, &key, 32_000).is_err());
}
#[test]
fn fixed_body_scan_rejects_every_cut_trailing_unknown_variant_and_wrong_operation() {
    let (body, _) = fixture();
    let raw = borsh::to_vec(&body).unwrap();
    for end in 0..raw.len() {
        assert!(quote_funding_relay_body_v2(&raw[..end], 20_000).is_err());
    }
    let mut changed = raw.clone();
    changed.push(0);
    assert!(quote_funding_relay_body_v2(&changed, 20_000).is_err());
    for offset in [0, 2, 4, 590] {
        let mut changed = raw.clone();
        changed[offset] = 255;
        assert!(quote_funding_relay_body_v2(&changed, 20_000).is_err());
    }
}
#[test]
fn structural_restore_requires_complete_same_material_and_one_purpose_per_outpoint() {
    use misaka_palw_positive_contract::native::NativeRowKey;
    let (body, key) = fixture();
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([55; 64]),
        carrier_block: NativeHash64([56; 64]),
        carrier_transaction: NativeHash64([57; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 50,
        accepted_blue_score: 50,
    };
    let material = AcceptedMaterial { origin, digest: body.material_id() };
    let (fees, physical, policy) = metadata_preimages();
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
    let quote = fees.quote(&policy, &physical, &body.terms, usage, origin.accepted_daa).unwrap();
    let auth = FundingRelayAuthorizationRowV2 {
        terms: body.terms,
        payer_key: key,
        purpose_signature: body.purpose_signature,
        origin,
        material,
        metadata_quote: AcceptedMaterial { origin, digest: quote.id() },
        metadata_charge: AcceptedMaterial { origin, digest: d(59) },
        policy,
        fees,
        physical,
        quote,
    };
    let custody = FundingRelayCustodyRowV2 {
        funding_outpoint: body.terms.funding_outpoint,
        purpose: body.terms.funding_purpose,
        terms: body.terms.id(),
        relay_signature: body.relay_signature,
        material,
        reservation: AcceptedMaterial { origin, digest: d(60) },
        retention_until_daa: 500,
        binding: None,
    };
    let encode = |body: NativeRowBody| {
        let row = NativeRowValue::new(body).unwrap();
        ((KERNEL_AUX_TABLE, row.body.key().encode()), row.encode().unwrap())
    };
    let mut rows = std::collections::BTreeMap::new();
    let (k, v) = encode(NativeRowBody::FundingRelayAuthorization(auth.clone()));
    rows.insert(k, v);
    assert!(validate_positive_aux_restore_v2(&rows).is_err());
    let (k, v) = encode(NativeRowBody::FundingRelayCustody(custody.clone()));
    rows.insert(k.clone(), v);
    let report = validate_positive_aux_restore_v2(&rows).unwrap();
    assert_eq!(report.rows, 2);
    assert_eq!(report.signature_checks, 1);
    assert_eq!(report.cases, 0);
    let mut changed = custody.clone();
    changed.relay_signature[0] ^= 1;
    rows.insert(k.clone(), encode(NativeRowBody::FundingRelayCustody(changed)).1);
    assert!(validate_positive_aux_restore_v2(&rows).is_err());
    rows.insert(k, encode(NativeRowBody::FundingRelayCustody(custody)).1);
    let mut other = auth;
    other.terms.funding_purpose = RelayFundingPurposeV2::CaseC;
    other.terms.target =
        RelayTargetV2::InitialCase { result: ResultId(d(61)), funding_scope: FundingScopeId(d(62)), consent: FundingConsentId(d(63)) };
    other.quote =
        other.fees.quote(&other.policy, &other.physical, &other.terms, other.quote.usage, other.origin.accepted_daa).unwrap();
    other.metadata_quote.digest = other.quote.id();
    let (k, v) = encode(NativeRowBody::FundingRelayAuthorization(other));
    rows.insert(k, v);
    assert!(validate_positive_aux_restore_v2(&rows).is_err());
    assert!(rows.keys().all(|(_, k)| matches!(k.get(4), Some(19 | 20))));
    assert_eq!(
        NativeRowKey::FundingRelayAuthorization { funding: body.terms.funding_outpoint, purpose: body.terms.funding_purpose }
            .encode()
            .len(),
        74
    );
}

fn metadata_preimages() -> (FundingRelayFeeScheduleV2, FundingRelayPhysicalProfileV2, FundingRelayPolicyV2) {
    let fees = FundingRelayFeeScheduleV2 {
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
    };
    let physical = FundingRelayPhysicalProfileV2 {
        version: 2,
        revision: 1,
        max_frame_bytes: 64_000,
        max_decoded_bytes: 64_000,
        max_heap_bytes: 128_000,
        max_retained_bytes: 64_000,
        max_database_bytes: 128_000,
        max_batch_bytes: 256_000,
        max_replay_reads: 100,
        max_replay_work: 1_000_000,
        max_signatures: 2,
    };
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
    (fees, physical, policy)
}

#[test]
fn native_fixed_row_scanner_matches_all_targets_and_optional_bound_package() {
    use kaspa_consensus_core::palw_native_resource_v1::{NativeHeapV1, retain_native_value_decode_v1};
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 20, database_bytes: 1 << 20 }));
    let keys = mldsa::generate_key_pair([51; 32]);
    let (base, key) = fixture();
    for variant in 0..4 {
        let mut body = base.clone();
        body.terms.target = match variant {
            0 => body.terms.target,
            1 => {
                body.terms.funding_purpose = RelayFundingPurposeV2::CaseC;
                RelayTargetV2::InitialCase {
                    result: ResultId(d(61)),
                    funding_scope: FundingScopeId(d(62)),
                    consent: FundingConsentId(d(63)),
                }
            }
            2 => {
                body.terms.funding_purpose = RelayFundingPurposeV2::ProverJoin;
                RelayTargetV2::InitialProver { prover: body.terms.payer_bond, join: d(64) }
            }
            _ => RelayTargetV2::Continuation {
                original_funding: NativeOutpoint { transaction: NativeHash64([65; 64]), index: 0 },
                predecessor: NativeOutpoint { transaction: NativeHash64([66; 64]), index: 0 },
                parent_spend: body.terms.funding_outpoint.transaction,
                original_contract: d(67),
            },
        };
        body.purpose_signature =
            *mldsa::sign(&keys.signing_key, &body.terms.id().0, FUNDING_RELAY_REGISTER_CONTEXT_V2, [68; 32]).unwrap().as_ref();
        let raw = borsh::to_vec(&body).unwrap();
        assert_eq!(quote_funding_relay_body_v2(&raw, 20_000).unwrap().encoded_bytes, raw.len() as u64);
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([69; 64]),
            carrier_block: NativeHash64([70; 64]),
            carrier_transaction: NativeHash64([71; 64]),
            accepted_tx_ordinal: 0,
            event_ordinal: 0,
            accepted_daa: 50,
            accepted_blue_score: 50,
        };
        let material = AcceptedMaterial { origin, digest: body.material_id() };
        let (fees, physical, policy) = metadata_preimages();
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
        let quote = fees.quote(&policy, &physical, &body.terms, usage, origin.accepted_daa).unwrap();
        let auth = FundingRelayAuthorizationRowV2 {
            terms: body.terms,
            payer_key: key,
            purpose_signature: body.purpose_signature,
            origin,
            material,
            metadata_quote: AcceptedMaterial { origin, digest: quote.id() },
            metadata_charge: AcceptedMaterial { origin, digest: d(72) },
            policy,
            fees,
            physical,
            quote,
        };
        let bound_origin = AcceptedOrigin {
            carrier_transaction: body.terms.funding_outpoint.transaction,
            accepting_block: NativeHash64([73; 64]),
            accepted_daa: 150,
            accepted_blue_score: 150,
            ..origin
        };
        let (original_funding, predecessor, original_contract) = match body.terms.target {
            RelayTargetV2::Continuation { original_funding, predecessor, original_contract, .. } => {
                (original_funding, Some(predecessor), original_contract)
            }
            _ => (body.terms.funding_outpoint, None, d(74)),
        };
        let custody = FundingRelayCustodyRowV2 {
            funding_outpoint: body.terms.funding_outpoint,
            purpose: body.terms.funding_purpose,
            terms: body.terms.id(),
            relay_signature: body.relay_signature,
            material,
            reservation: AcceptedMaterial { origin, digest: d(75) },
            retention_until_daa: 500,
            binding: Some(FundingRelayBindingV2 {
                origin: bound_origin,
                original_funding,
                predecessor,
                original_contract,
                reservation: AcceptedMaterial { origin: bound_origin, digest: d(76) },
            }),
        };
        custody.check_pair(&auth).unwrap();
        for body in [NativeRowBody::FundingRelayAuthorization(auth), NativeRowBody::FundingRelayCustody(custody)] {
            let row = NativeRowValue::new(body).unwrap();
            let raw = row.encode().unwrap();
            let key = row.body.key().encode();
            assert_eq!(row.native_heap_v1("fixed relay row").bytes(), Some(0));
            let retained = retain_native_value_decode_v1(&raw, || {
                NativeRowValue::decode(&key, &raw)
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "fixed R row codec"))
            })
            .unwrap();
            assert_eq!(&*retained, &row);
        }
    }
}
