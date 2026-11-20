//! Read-only check of a publisher's actual bytes against the native policy helper.
//! No repair/rebinding is written. A mismatch fails instead of masking the producer.
use kaspa_consensus_core::{
    palw_positive_contract_v2::ProtectedPlanningDeclarationV3, palw_protected_original_quote_v1::ProtectedOriginalQuoteMaterialV1,
};
use std::{fs::File, io::Read};

fn read(path: &str, limit: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path).unwrap().take(limit + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() as u64 <= limit, "bounded policy-check input");
    bytes
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "protected-material.hex complete-declaration-v3.json");
    let encoded = read(&args[1], 32 << 10);
    assert_eq!(encoded.len() % 2, 0);
    let bytes: Vec<_> =
        encoded.chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect();
    let actual = ProtectedOriginalQuoteMaterialV1::decode_v1(&bytes).unwrap();
    let declaration =
        ProtectedPlanningDeclarationV3::decode_json_v3(&read(&args[2], ProtectedPlanningDeclarationV3::MAX_JSON_BYTES as u64))
            .unwrap();
    let expected = actual.clone().with_planning_declaration_v3(&declaration).unwrap();
    assert_eq!(actual, expected, "publisher did not bind the exact native planning scope");
    println!(
        "{{\"nativePolicyMatched\":true,\"declarationId\":\"{}\",\"nativeAdmission\":false}}",
        declaration.identity_v3().unwrap()
    );
}
