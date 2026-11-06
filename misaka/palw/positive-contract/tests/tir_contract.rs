use misaka_palw_positive_contract::{ContractError, identity::*, messages::*, tir::*, wire::*};
fn d(x: u8) -> Digest {
    Digest([x; 64])
}
fn result() -> MinerResultCommit {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let s = v["result_borsh_hex"].as_str().unwrap();
    let bytes: Vec<_> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
    borsh::from_slice(&bytes).unwrap()
}
#[test]
fn cone_and_trace_are_borrowed_exact_existing_g14_payloads() {
    for trace in [None, Some(&b"trace"[..])] {
        let part = ExactPart::new(b"cone", trace, 1024).unwrap();
        let bytes = part.encode();
        let decoded = ExactPart::decode(&bytes, 1024).unwrap();
        assert_eq!(decoded, part);
        assert!(std::ptr::eq(decoded.cone().as_ptr(), bytes[16..].as_ptr()));
        for n in 0..bytes.len() {
            assert!(ExactPart::decode(&bytes[..n], 1024).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(ExactPart::decode(&extra, 1024).is_err());
        assert!(ExactPart::decode(&bytes, bytes.len() as u64 - 1).is_err());
        let mut bad = bytes.clone();
        bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ExactPart::decode(&bad, 1024).is_err());
    }
    assert_eq!(ExactPart::new(b"", None, 1024), Err(ContractError::NonCanonical));
    assert_eq!(ExactPart::new(b"cone", Some(b""), 1024), Err(ContractError::NonCanonical));
}
#[test]
fn current_frame_limit_covers_both_wrappers_and_exactly_one_relation() {
    let part = ExactPart::new(b"cone", None, 1024).unwrap().encode();
    let header = EvidenceHeader {
        case: CaseId(d(1)),
        result: ResultId(d(2)),
        witness: WitnessId(d(3)),
        plan: PlanId(d(4)),
        first_relation: 1,
        relation_count: 1,
        payload_bytes: part.len() as u32,
    };
    let frame = EvidenceFrame::new(header, &part, 1024).unwrap();
    assert_eq!(decode_frame(&frame, 1024).unwrap().cone(), b"cone");
    assert_eq!(raw_part_budget(1024), Ok(724));
    assert_eq!(raw_part_budget(300), Err(ContractError::Budget));
    assert_eq!(raw_part_budget(u64::MAX), Ok(u32::MAX as u64 - EXACT_PART_HEADER_BYTES as u64));
    assert_eq!(decode_frame(&frame, 303), Err(ContractError::Budget));
    let batch = EvidenceFrame::new(EvidenceHeader { relation_count: 2, ..header }, &part, 1024).unwrap();
    assert_eq!(decode_frame(&batch, 1024), Err(ContractError::NonCanonical));
}
#[test]
fn independent_python_vectors_pin_tir_annex_domains() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/tir-vectors-r1.json")).unwrap();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(hex(&exact_derivation_id().0), v["derivation"].as_str().unwrap());
    assert_eq!(hex(&greedy_selector_id().0), v["selector"].as_str().unwrap());
    let r = result();
    assert_eq!(hex(&provenance_id(&r).0), v["provenance"].as_str().unwrap());
    for i in 0..7 {
        let mut changed = r.clone();
        match i {
            0 => changed.registered_class = d(99),
            1 => changed.artifact = d(99),
            2 => changed.program = d(99),
            3 => changed.input = d(99),
            4 => changed.tokenizer = d(99),
            5 => changed.execution_rules = d(99),
            _ => changed.dependencies = d(99),
        };
        assert_ne!(provenance_id(&r), provenance_id(&changed));
    }
    let mut other = r.clone();
    other.output = d(99);
    assert_eq!(provenance_id(&r), provenance_id(&other));
}
