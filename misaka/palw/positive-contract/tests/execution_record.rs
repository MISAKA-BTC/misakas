use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::*,
    record::*,
    wire::{decode_result, decode_witness},
};

fn fixture() -> (MinerResultCommit, PanelWitnessCommit, RecordHeader) {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let bytes = |key: &str| {
        let s = vectors[key].as_str().unwrap();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<_>>()
    };
    // The independent vectors are body-only; use the actual public codecs.
    let wrapped = |kind: u8, body: Vec<u8>| {
        let mut out = b"PC2R1DTO".to_vec();
        out.push(kind);
        out.extend(body);
        out
    };
    let result = decode_result(&wrapped(0, bytes("result_borsh_hex"))).unwrap();
    let witness = decode_witness(&wrapped(1, bytes("witness_borsh_hex"))).unwrap();
    let header = RecordHeader {
        version: 2,
        revision: 1,
        result: result.id(),
        artifact: result.artifact,
        full_logits_trace_root: Digest([40; 64]),
        step_leaf_count: 0x0102_0304_0506_0708,
        step_merkle_root: Digest([41; 64]),
        committed_execution_root: witness.execution_record,
    };
    (result, witness, header)
}
const IDS: [u32; 3] = [7, u32::MAX, 13];

#[test]
fn fixed_record_has_no_class_program_job_or_second_dtype_and_preserves_u32_ids() {
    let (r, w, header) = fixture();
    assert_eq!(RECORD_HEADER_BYTES, 332);
    assert_eq!(RECORD_OVERHEAD_BYTES, 344);
    assert_eq!(borsh::object_length(&header).unwrap(), RECORD_HEADER_BYTES);
    let bytes = encode_record(header, &IDS, r.decode, 4096).unwrap();
    assert_eq!(bytes.len(), RECORD_OVERHEAD_BYTES + IDS.len() * 4);
    let record = ExecutionRecord::decode(&bytes, 4096, r.decode).unwrap();
    assert_eq!(record.header(), header);
    assert_eq!(record.wire_bytes(), bytes.len() as u64);
    assert_eq!(record.generated_count(), 3);
    assert!(std::ptr::eq(record.generated_bytes().as_ptr(), bytes[344..].as_ptr()));
    assert_eq!(record.tokens().len(), 3);
    assert_eq!(record.tokens().collect::<Vec<_>>(), IDS);
    assert_eq!(record.tokens().rev().collect::<Vec<_>>(), [13, u32::MAX, 7]);
    for (i, id) in IDS.iter().enumerate() {
        assert_eq!(record.token(i as u32), Some(*id));
    }
    assert_eq!(record.token(3), None);
    assert_eq!(record.token(u32::MAX), None);
    assert_eq!(record.validate_binding(&r, &w), Ok(()));
}

#[test]
fn independent_python_struct_pack_vector_matches_every_record_byte() {
    // Python: b'PC2R1REC' + pack('<HH',2,1) + pinned ResultID +
    // d(8)+d(40)+pack('<Q',0x0102030405060708)+d(41)+d(20)+
    // pack('<IIII',3,7,0xffffffff,13). No Rust/Borsh encoder produced this vector.
    let expected = concat!(
        "5043325231524543020001009577dc7d000ed73f9faea6c8569b572111ca596bf62d7906f72a0dc8803ca7b1d066978d810b7718bde30291d10f538143de00b121f25f38b023d6167d466047",
        "08080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808",
        "28282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828282828",
        "0807060504030201",
        "29292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929",
        "14141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414",
        "0300000007000000ffffffff0d000000",
    );
    let (r, _, header) = fixture();
    let encoded = encode_record(header, &IDS, r.decode, 4096).unwrap();
    let actual: String = encoded.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(actual, expected);
}

#[test]
fn truncation_trailing_bytes_budget_and_hostile_counts_are_rejected_without_token_allocation() {
    let (r, _, header) = fixture();
    let bytes = encode_record(header, &IDS, r.decode, 4096).unwrap();
    for n in 0..bytes.len() {
        assert!(ExecutionRecord::decode(&bytes[..n], 4096, r.decode).is_err(), "cut {n}");
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert_eq!(ExecutionRecord::decode(&extra, 4096, r.decode), Err(ContractError::NonCanonical));
    assert!(ExecutionRecord::decode(&bytes, bytes.len() as u64, r.decode).is_ok());
    assert_eq!(ExecutionRecord::decode(&bytes, bytes.len() as u64 - 1, r.decode), Err(ContractError::Budget));
    assert_eq!(encode_record(header, &IDS, r.decode, bytes.len() as u64 - 1), Err(ContractError::Budget));
    assert_eq!(ExecutionRecord::decode(&bytes, 4096, r.decode - 1), Err(ContractError::Range));
    assert_eq!(ExecutionRecord::decode(&bytes, 4096, 0), Err(ContractError::Range));
    assert_eq!(encode_record(header, &IDS, r.decode + 1, 4096), Err(ContractError::Range));
    assert_eq!(encode_record(header, &[], 0, 4096), Err(ContractError::Range));
    let mut huge = bytes.clone();
    huge[340..344].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(ExecutionRecord::decode(&huge, 4096, r.decode), Err(ContractError::Range));
    assert_eq!(ExecutionRecord::decode(&huge, 4096, u32::MAX), Err(ContractError::Budget));
    assert_eq!(ExecutionRecord::decode(&huge, u64::MAX, u32::MAX), Err(ContractError::NonCanonical));
}

#[test]
fn unknown_magic_and_contract_versions_are_not_old_capture_fallbacks() {
    let (r, _, header) = fixture();
    let bytes = encode_record(header, &IDS, r.decode, 4096).unwrap();
    for offset in [0, 8, 10] {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        assert_eq!(ExecutionRecord::decode(&changed, 4096, r.decode), Err(ContractError::Version));
    }
    for (v, rev) in [(1, 1), (2, 0), (2, 2)] {
        let mut bad = header;
        bad.version = v;
        bad.revision = rev;
        assert_eq!(encode_record(bad, &IDS, r.decode, 4096), Err(ContractError::Version));
    }
}

#[test]
fn record_result_artifact_execution_and_decode_bindings_cannot_be_substituted() {
    let (r, w, header) = fixture();
    for field in 0..3 {
        let mut changed = header;
        match field {
            0 => changed.result = ResultId(Digest([99; 64])),
            1 => changed.artifact = Digest([99; 64]),
            _ => changed.committed_execution_root = Digest([99; 64]),
        }
        let bytes = encode_record(changed, &IDS, r.decode, 4096).unwrap();
        let record = ExecutionRecord::decode(&bytes, 4096, r.decode).unwrap();
        assert_eq!(record.validate_binding(&r, &w), Err(ContractError::Identity));
    }
    let bytes = encode_record(header, &IDS, r.decode, 4096).unwrap();
    let record = ExecutionRecord::decode(&bytes, 4096, r.decode).unwrap();
    let mut different = r.clone();
    different.input = Digest([99; 64]);
    assert!(record.validate_binding(&different, &w).is_err());
    let mut different = w.clone();
    different.result = ResultId(Digest([99; 64]));
    assert!(record.validate_binding(&r, &different).is_err());
    let mut shorter = r.clone();
    shorter.decode -= 1;
    let mut short_witness = w.clone();
    short_witness.result = shorter.id();
    let mut short_header = header;
    short_header.result = shorter.id();
    let bytes = encode_record(short_header, &IDS, 3, 4096).unwrap();
    let decoded_with_wrong_expectation = ExecutionRecord::decode(&bytes, 4096, 3).unwrap();
    assert_eq!(decoded_with_wrong_expectation.validate_binding(&shorter, &short_witness), Err(ContractError::Range));
}

#[test]
fn framing_and_binding_do_not_assert_arithmetic_or_terminal_correctness() {
    let (r, mut w, mut header) = fixture();
    // These unverified DTO values still need A1's actual TIR reconstruction and
    // existing terminal-output calculation. This layer must not invent either.
    header.step_leaf_count = u64::MAX;
    header.step_merkle_root = Digest([99; 64]);
    w.candidate_output = Digest([99; 64]);
    let bytes = encode_record(header, &IDS, r.decode, 4096).unwrap();
    let record = ExecutionRecord::decode(&bytes, 4096, r.decode).unwrap();
    assert_eq!(record.validate_binding(&r, &w), Ok(()));
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
