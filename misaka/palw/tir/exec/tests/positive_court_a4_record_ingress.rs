//! The borrowed record boundary checks framing/DTO identity only. The fixture
//! contains no arithmetic acceptance or native role/admission capability.
#![cfg(feature = "node")]
use kaspa_consensus_core::palw_positive_court_v2::coordinator::{
    native_body::{MAX_OPAQUE_BYTES_V2, PositiveNativeBodyV2, PositiveNativeOperationV2},
    record_ingress::{RecordIngressErrorV2, preflight_witness_record_v2},
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::*,
    record::{ExecutionRecord, RECORD_OVERHEAD_BYTES, RecordHeader, encode_record},
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn fixture() -> (MinerResultCommit, PositiveNativeBodyV2) {
    let vector: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let hex = vector["result_borsh_hex"].as_str().unwrap();
    let raw: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let result: MinerResultCommit = borsh::from_slice(&raw).unwrap();
    assert!(result.decode > 0 && result.decode <= 16);
    let header = RecordHeader {
        version: 2,
        revision: 1,
        result: result.id(),
        artifact: result.artifact,
        full_logits_trace_root: d(40),
        step_leaf_count: 8,
        step_merkle_root: d(41),
        committed_execution_root: d(42),
    };
    let witness = PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: result.scope,
        case: result.case_id(),
        result: result.id(),
        author: PrincipalId(d(43)),
        attempt: 0,
        profile: CheckProfile::ExactStreamV1,
        plan: PlanId(d(44)),
        execution_record: header.committed_execution_root,
        provenance: d(45),
        candidate_output: result.output,
    };
    let ids: Vec<u32> = (0..result.decode).collect();
    let record = encode_record(header, &ids, result.decode, 4096).unwrap();
    (
        result,
        PositiveNativeBodyV2 {
            version: 2,
            revision: 1,
            operation: PositiveNativeOperationV2::WitnessCommit { witness, panel_signature: [0; SIGNATURE_BYTES], record },
        },
    )
}
fn bytes(body: &PositiveNativeBodyV2) -> &[u8] {
    match &body.operation {
        PositiveNativeOperationV2::WitnessCommit { record, .. } => record,
        _ => panic!("witness fixture"),
    }
}
fn replace_bytes(body: &mut PositiveNativeBodyV2, value: Vec<u8>) {
    match &mut body.operation {
        PositiveNativeOperationV2::WitnessCommit { record, .. } => *record = value,
        _ => panic!("witness fixture"),
    }
}

#[test]
fn checked_record_keeps_the_original_body_and_tokens_borrowed_without_role_authority() {
    let (result, body) = fixture();
    let checked = preflight_witness_record_v2(&result, &body, 4096).unwrap();
    assert!(std::ptr::eq(checked.rooted_result(), &result));
    assert_eq!(checked.record_bytes().as_ptr(), bytes(&body).as_ptr());
    assert_eq!(checked.execution_record().generated_bytes().as_ptr(), bytes(&body)[RECORD_OVERHEAD_BYTES..].as_ptr());
    assert_eq!(checked.execution_record().tokens().collect::<Vec<_>>(), (0..result.decode).collect::<Vec<_>>());
    assert_eq!(checked.witness().result, result.id());
    assert_eq!(checked.panel_signature(), &[0; SIGNATURE_BYTES]); // carried bytes are not authenticated here
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}

#[test]
fn direct_dto_construction_cannot_skip_outer_versions_operation_expected_result_or_bounds() {
    let (result, body) = fixture();
    for mode in 0..2 {
        let mut changed = body.clone();
        if mode == 0 {
            changed.version = 1;
        } else {
            changed.revision = 2;
        }
        assert_eq!(preflight_witness_record_v2(&result, &changed, 4096).err(), Some(RecordIngressErrorV2::BodyVersion));
    }
    let finish = PositiveNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveNativeOperationV2::Finish { case: result.case_id(), result: result.id(), witness: WitnessId(d(50)) },
    };
    assert_eq!(preflight_witness_record_v2(&result, &finish, 4096).err(), Some(RecordIngressErrorV2::WrongOperation));
    let mut other_result = result.clone();
    other_result.input = d(51);
    assert!(preflight_witness_record_v2(&other_result, &body, 4096).is_err());
    for cap in [0, bytes(&body).len() as u64 - 1, MAX_OPAQUE_BYTES_V2 + 1] {
        assert_eq!(preflight_witness_record_v2(&result, &body, cap).err(), Some(RecordIngressErrorV2::Budget));
    }
}

#[test]
fn record_identity_decode_and_hostile_length_tampering_refuse_the_delivery() {
    let (result, body) = fixture();
    let decoded = ExecutionRecord::decode(bytes(&body), 4096, result.decode).unwrap();
    let ids = decoded.tokens().collect::<Vec<_>>();
    for field in 0..3 {
        let mut changed = body.clone();
        let mut header = decoded.header();
        match field {
            0 => header.result = ResultId(d(52)),
            1 => header.artifact = d(53),
            _ => header.committed_execution_root = d(54),
        }
        replace_bytes(&mut changed, encode_record(header, &ids, result.decode, 4096).unwrap());
        assert!(preflight_witness_record_v2(&result, &changed, 4096).is_err());
    }
    for mode in 0..6 {
        let mut changed = body.clone();
        let mut raw = bytes(&body).to_vec();
        match mode {
            0 => raw[0] ^= 1,
            1 => raw[8] = 255,
            2 => raw[10] = 255,
            3 => {
                raw.pop();
            }
            4 => raw.push(0),
            _ => raw[RECORD_OVERHEAD_BYTES - 4..RECORD_OVERHEAD_BYTES].copy_from_slice(&u32::MAX.to_le_bytes()),
        }
        replace_bytes(&mut changed, raw);
        assert!(preflight_witness_record_v2(&result, &changed, 4096).is_err(), "record mode {mode}");
    }
}

#[test]
fn decoded_roots_and_token_values_still_require_the_independent_a1_semantic_checker() {
    let (result, mut body) = fixture();
    let decoded = ExecutionRecord::decode(bytes(&body), 4096, result.decode).unwrap();
    let mut header = decoded.header();
    header.step_leaf_count = u64::MAX;
    header.step_merkle_root = d(55);
    header.full_logits_trace_root = d(56);
    let unverified_ids = vec![u32::MAX; result.decode as usize];
    replace_bytes(&mut body, encode_record(header, &unverified_ids, result.decode, 4096).unwrap());
    let checked = preflight_witness_record_v2(&result, &body, 4096).unwrap();
    assert_eq!(checked.execution_record().header().step_leaf_count, u64::MAX);
    assert_eq!(checked.execution_record().token(0), Some(u32::MAX));
    // This success is intentionally only a borrowed DTO binding. No A4 state,
    // attempt counter, Panel fault, Miner conviction or computation pass exists.
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
