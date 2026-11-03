//! Syntax/heap fixtures only: dummy signatures and origins grant no admission.
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeHeapV1, NativeWireReaderV1, NativeWireScanV1},
    palw_positive_court_v2::coordinator::{assignment_body::*, native_body::*},
    palw_tir_class_v1::PalwTirLayoutV1,
};
use misaka_palw_positive_contract::{
    assignment::{consents::*, role_actions::*},
    funding::*,
    identity::*,
    messages::*,
    rows::*,
};
#[path = "../../../misaka/palw/positive-contract/tests/support/assignment.rs"]
mod support;
use support::*;

fn limits() -> AssignmentBodyLimitsV2 {
    AssignmentBodyLimitsV2 {
        max_body_bytes: 64_000,
        max_layout_bytes: 32_000,
        max_layout_entries: 64,
        max_settlement_receipts: 10,
        max_outer_signatures: 3,
        max_decode_heap: 512_000,
    }
}
fn old_limits() -> NativeBodyLimitsV2 {
    NativeBodyLimitsV2 { max_body_bytes: 64_000, max_opaque_bytes: 32_000, max_exact_relations: 64, max_signatures: 2 }
}
fn budget() -> AggregateBudget {
    AggregateBudget {
        rounds: 10,
        unique_source_bytes: 1000,
        transmitted_bytes: 2000,
        additional_work: 1000,
        retained_bytes: 1000,
        retention_until: 500,
        fee: 10,
        witness_attempts: 2,
    }
}
fn admission() -> PositiveAssignmentNativeBodyV2 {
    let mut b = bundle();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: b.terms.id(),
        worker: b.terms.worker,
        source_catalog: b.source_catalog,
        source_policy: b.terms.source_policy,
        budget: budget(),
        service_before_daa: 400,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: b.terms.id(),
        prover: b.terms.prover,
        positive_policy: d(40),
        max_attempts: 2,
        max_build_work: 1000,
        max_submitted_bytes: 1000,
        max_retained_bytes: 1000,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 10,
        cancellation_policy: b.cancellation_policy,
    };
    b.source_consent = source.id();
    b.prover_consent = prover.id();
    source.check(&b).unwrap();
    prover.check(&b, d(40)).unwrap();
    let layout = PalwTirLayoutV1 {
        version: 1,
        max_context: 16,
        checkpoint_interval: 4,
        h_tile: 4,
        commit_tiles: vec![4, 8],
        state_tiles: vec![2],
    };
    PositiveAssignmentNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment {
            bundle: b,
            source,
            prover,
            signatures: [[0; SIGNATURE_BYTES]; 3],
            layout: borsh::to_vec(&layout).unwrap(),
        },
    }
}
fn payload() -> AssignedResultPayloadV2 {
    let b = bundle();
    let result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        job: d(4),
        execution: d(5),
        miner: PrincipalId(d(6)),
        registered_class: d(7),
        artifact: d(8),
        program: d(9),
        input: d(10),
        tokenizer: d(0),
        execution_rules: d(0),
        selector: d(11),
        generation_seed: [12; 32],
        temperature_q24: 0,
        prefill: 1,
        decode: 2,
        max_context: 16,
        output: d(13),
        dependencies: d(14),
        service_contract: d(15),
    };
    let service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: b.source_catalog,
        provider_delegations: d(17),
        budget: budget(),
        funding_consent: d(18),
        deadline: 300,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: d(19),
        mandatory_relations: 3,
        source_catalog: b.source_catalog,
        resource_quote: d(20),
        policy: d(21),
        schedule: d(22),
        required_fee: 10,
        required_capital: 20,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: result.case_id(),
        scope: FundingScopeId(d(23)),
        payer: b.terms.payer,
        max_fees: 10,
        max_locked: 30,
        funding_output_index: 0,
        refund_payload: b.terms.refund_payload,
        admission_expiry: 200,
    };
    AssignedResultPayloadV2 {
        result,
        miner_signature: [0; SIGNATURE_BYTES],
        service,
        basis,
        consent,
        payer_signature: [0; SIGNATURE_BYTES],
    }
}
fn origin() -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 1,
        accepted_blue_score: 1,
    }
}
fn bodies() -> Vec<PositiveAssignmentNativeBodyV2> {
    let b = bundle();
    let id = b.terms.id();
    let operations = [
        PositiveAssignmentNativeOperationV2::AdmitAssignedResult { assignment: id, payload: payload() },
        PositiveAssignmentNativeOperationV2::CancelAssignment {
            cancellation: AssignmentCancellationV2 {
                assignment: id,
                original_bundle: b.id(),
                cancellation_policy: b.cancellation_policy,
                cancel_before_daa: 199,
            },
            poster_signature: [0; SIGNATURE_BYTES],
            worker_signature: [0; SIGNATURE_BYTES],
        },
        PositiveAssignmentNativeOperationV2::ExpireAssignment { assignment: id },
        PositiveAssignmentNativeOperationV2::SettleAssignment {
            assignment: id,
            case: Some(CaseId(d(30))),
            receipts: vec![AcceptedMaterial { origin: origin(), digest: d(31) }],
        },
        PositiveAssignmentNativeOperationV2::JoinAssignmentProver {
            consent: AssignmentProverJoinV2 {
                version: 2,
                revision: 1,
                assignment: id,
                original_bundle: b.id(),
                case: Some((CaseId(d(30)), ResultId(d(32)))),
                prover: bond(40),
                payer: PrincipalId(d(41)),
                funding_output_index: 0,
                max_fee: 10,
                max_capital: 20,
                max_locked: 30,
                max_attempts: 1,
                max_build_work: 1000,
                max_submitted_bytes: 2000,
                max_retained_bytes: 1000,
                admission_before_daa: 150,
                service_before_daa: 400,
                retention_until_daa: 500,
                cancellation_policy: b.cancellation_policy,
            },
            signature: [0; SIGNATURE_BYTES],
        },
    ];
    std::iter::once(admission())
        .chain(operations.into_iter().map(|operation| PositiveAssignmentNativeBodyV2 { version: 2, revision: 1, operation }))
        .collect()
}

#[test]
fn append_only_operations_roundtrip_with_exact_fixed_widths_and_heap_quotes() {
    let all = bodies();
    for (i, body) in all.iter().enumerate() {
        let bytes = encode_assignment_body_v2(body, limits()).unwrap();
        let summary = scan_assignment_body_v2(&bytes, limits()).unwrap();
        assert_eq!(summary.kind, 8 + i as u8);
        assert_eq!(summary.signatures, [3, 2, 2, 0, 0, 1][i]);
        assert_eq!(summary.canonical_bytes, bytes.len() as u64);
        let decoded = decode_assignment_body_v2(&bytes, limits()).unwrap();
        assert_eq!(&decoded, body);
        let continuation = match &body.operation {
            PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment { .. } => None,
            PositiveAssignmentNativeOperationV2::AdmitAssignedResult { assignment, .. }
            | PositiveAssignmentNativeOperationV2::ExpireAssignment { assignment }
            | PositiveAssignmentNativeOperationV2::SettleAssignment { assignment, .. } => Some(*assignment),
            PositiveAssignmentNativeOperationV2::CancelAssignment { cancellation, .. } => Some(cancellation.assignment),
            PositiveAssignmentNativeOperationV2::JoinAssignmentProver { consent, .. } => Some(consent.assignment),
        };
        assert_eq!(scan_assignment_continuation_id_v2(&bytes).ok(), continuation);
        if let Some(id) = continuation {
            let end = if summary.kind == 13 { 73 } else { 69 };
            assert_eq!(scan_assignment_continuation_id_v2(&bytes[..end]).unwrap(), id);
            assert!(scan_assignment_continuation_id_v2(&bytes[..end - 1]).is_err());
        }
        assert!(decoded.native_heap_v1("assignment body").bytes().unwrap() <= summary.heap);
        let mut r = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = PositiveAssignmentNativeBodyV2::scan_native_wire_v1(&mut r).unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(shape.heap, summary.heap);
        assert!(scan_native_body_v2(&bytes, old_limits()).is_err(), "new kinds never widen old 0..7");
    }
    if let PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment { bundle, source, prover, .. } = &all[0].operation {
        assert_eq!(borsh::object_length(&bundle.terms).unwrap() as u64, ASSIGNMENT_TERMS_BYTES_V2);
        assert_eq!(borsh::object_length(bundle).unwrap() as u64, ASSIGNMENT_BUNDLE_BYTES_V2);
        assert_eq!(borsh::object_length(source).unwrap() as u64, ASSIGNMENT_SOURCE_CONSENT_BYTES_V2);
        assert_eq!(borsh::object_length(prover).unwrap() as u64, ASSIGNMENT_PROVER_CONSENT_BYTES_V2);
    } else {
        unreachable!();
    }
}

#[test]
fn assigned_result_uses_exact_old_payload_and_stream_scan_stops_at_its_own_boundary() {
    let p = payload();
    let old = PositiveNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveNativeOperationV2::ResultAdmission {
            result: p.result.clone(),
            miner_signature: p.miner_signature,
            service: p.service,
            basis: p.basis,
            consent: p.consent.clone(),
            payer_signature: p.payer_signature,
        },
    };
    let old_bytes = encode_native_body_v2(&old, old_limits()).unwrap();
    let new = PositiveAssignmentNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveAssignmentNativeOperationV2::AdmitAssignedResult { assignment: bundle().terms.id(), payload: p },
    };
    let new_bytes = encode_assignment_body_v2(&new, limits()).unwrap();
    assert_eq!(&new_bytes[69..], &old_bytes[5..]);
    assert!(scan_assignment_body_v2(&old_bytes, limits()).is_err());
    let pair = vec![new, bodies().remove(3)];
    let bytes = borsh::to_vec(&pair).unwrap();
    let mut r = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    Vec::<PositiveAssignmentNativeBodyV2>::scan_native_wire_v1(&mut r).unwrap();
    assert_eq!(r.remaining(), 0, "op9 may not consume the next framed value");
}

#[test]
fn truncated_trailing_unknown_and_hostile_lengths_reject_before_decode() {
    for body in bodies() {
        let bytes = encode_assignment_body_v2(&body, limits()).unwrap();
        for n in [0, 1, 4, 5, 6, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode_assignment_body_v2(&bytes[..n], limits()).is_err());
        }
        let mut changed = bytes.clone();
        changed.push(0);
        assert!(decode_assignment_body_v2(&changed, limits()).is_err());
        changed = bytes;
        changed[4] = 14;
        assert_eq!(scan_assignment_body_v2(&changed, limits()), Err(AssignmentBodyErrorV2::Kind));
    }
    let mut bytes = encode_assignment_body_v2(&admission(), limits()).unwrap();
    let layout_length = (5
        + ASSIGNMENT_BUNDLE_BYTES_V2
        + ASSIGNMENT_SOURCE_CONSENT_BYTES_V2
        + ASSIGNMENT_PROVER_CONSENT_BYTES_V2
        + 3 * SIGNATURE_BYTES as u64) as usize;
    bytes[layout_length..layout_length + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(scan_assignment_body_v2(&bytes, limits()).is_err());
}

#[test]
fn role_signature_layout_receipt_and_typed_heap_caps_are_all_preflighted() {
    let all = bodies();
    let bytes = encode_assignment_body_v2(&all[0], limits()).unwrap();
    assert!(scan_assignment_body_v2(&bytes, AssignmentBodyLimitsV2 { max_outer_signatures: 2, ..limits() }).is_err());
    assert!(scan_assignment_body_v2(&bytes, AssignmentBodyLimitsV2 { max_layout_entries: 2, ..limits() }).is_err());
    assert!(scan_assignment_body_v2(&bytes, AssignmentBodyLimitsV2 { max_decode_heap: 1, ..limits() }).is_err());
    assert!(scan_assignment_body_v2(&bytes, AssignmentBodyLimitsV2 { max_body_bytes: 5, ..limits() }).is_err());
    let mut too_many = all[4].clone();
    if let PositiveAssignmentNativeOperationV2::SettleAssignment { receipts, .. } = &mut too_many.operation {
        receipts.resize(11, receipts[0]);
    } else {
        unreachable!();
    }
    assert!(encode_assignment_body_v2(&too_many, limits()).is_err());
    let mut changed = bytes;
    let start = (5
        + ASSIGNMENT_BUNDLE_BYTES_V2
        + ASSIGNMENT_SOURCE_CONSENT_BYTES_V2
        + ASSIGNMENT_PROVER_CONSENT_BYTES_V2
        + 3 * SIGNATURE_BYTES as u64
        + 4) as usize;
    changed[start + 14..start + 18].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(scan_assignment_body_v2(&changed, limits()).is_err());
}
