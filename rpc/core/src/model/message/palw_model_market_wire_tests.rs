//! **`message`'s `palw_model_market_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn a_market() -> GetPalwModelMarketResponse {
    GetPalwModelMarketResponse {
        found: true,
        line_id: "74".repeat(64),
        opened: true,
        seed_pledged_sompi: 200_000_000,
        seed_min_sompi: 100_000_000_000_000,
        burn_permille: 50,
        leg_permille: 50,
        leg_v2_activation_daa: 3_500,
        class_status: "Active".to_string(),
        // Version 7, non-default so a lost field cannot pass as a carried one.
        class_lifecycle: "Prefetching".to_string(),
        market_refusal: "class 74… is Prefetching under the model registry".to_string(),
        ..Default::default()
    }
}

/// **The 2026-09-23 Position route matrix, P-B3: the gate reaches the client.** A version-7 node
/// carries the lifecycle and the refusal; they are what `misaka palw model-seed` and `model-buy`
/// refuse on before a carrier's sink output is spent.
#[test]
fn the_lifecycle_and_the_market_refusal_survive_the_round_trip() {
    let response = a_market();
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwModelMarketResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.class_lifecycle, "Prefetching");
    assert_eq!(back.market_refusal, response.market_refusal);
    assert_eq!((back.seed_pledged_sompi, back.leg_v2_activation_daa), (200_000_000, 3_500), "the version-6 fields still land");
}

/// A version-6 peer's answer — the same bytes without the suffix — reads as no refusal: its fold
/// never asked the registry, so there is nothing to refuse on, and a client does what it did.
#[test]
fn a_version_6_peer_reads_as_no_refusal() {
    let response = GetPalwModelMarketResponse { class_lifecycle: String::new(), market_refusal: String::new(), ..a_market() };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    // Version 7 appended two strings; two empty borsh strings are two zero u32 lengths.
    assert_eq!(&bytes[bytes.len() - 8..], &[0u8; 8]);
    bytes.truncate(bytes.len() - 8);
    bytes[..2].copy_from_slice(&6u16.to_le_bytes());
    let back = <GetPalwModelMarketResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(back.class_lifecycle.is_empty() && back.market_refusal.is_empty());
    assert_eq!((back.burn_permille, back.leg_permille, back.leg_v2_activation_daa), (50, 50, 3_500));
}

/// **The Activation Pool's P4 on the registry's wire**: a class row carries the non-binding `recommended_pool_sompi` after every
/// version-1 field (version 2); a version-1 peer's row — the same bytes without it — reads as no recommendation, and the deprecated
/// `registration_bond_sompi` keeps its bytes and its meaning. Since RFC-0017 the row is version 3 (the eligibility fields follow);
/// a version-2 peer's row is the version-3 bytes without them and reads as ungoverned.
#[test]
fn a_version_1_registry_row_reads_as_no_recommended_pool() {
    let row = RpcPalwModelLifecycle {
        class_id: "ab".repeat(64),
        registration_bond_sompi: 279_900_000_000_000,
        recommended_pool_sompi: 240_000_000_000,
        reason: "candidate".to_string(),
        // What a version-1 or version-2 row decodes the RFC-0017 fields as.
        eligibility_stage: "not-governed".to_string(),
        eligibility_subsidy: true,
        eligibility_block_right: true,
        eligibility_final_weight: true,
        ..Default::default()
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&row, &mut bytes).unwrap();
    assert_eq!(&bytes[..2], &3u16.to_le_bytes());
    // Version 3's suffix: the stage (borsh: u32 length + 12 bytes), the renewal DAA, three bools, two u32s — 35 bytes.
    let suffix = 4 + "not-governed".len() + 8 + 3 + 4 + 4;
    bytes.truncate(bytes.len() - suffix);
    bytes[..2].copy_from_slice(&2u16.to_le_bytes());
    let back = <RpcPalwModelLifecycle as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(format!("{back:?}"), format!("{row:?}"), "a version-2 row reads as ungoverned");
    assert_eq!((back.registration_bond_sompi, back.recommended_pool_sompi), (279_900_000_000_000, 240_000_000_000));
    assert_eq!(&bytes[bytes.len() - 8..], &240_000_000_000u64.to_le_bytes());
    bytes.truncate(bytes.len() - 8);
    bytes[..2].copy_from_slice(&1u16.to_le_bytes());
    let v1 = <RpcPalwModelLifecycle as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(format!("{v1:?}"), format!("{:?}", RpcPalwModelLifecycle { recommended_pool_sompi: 0, ..row }));
}

/// **RFC-0017 on the registry's wire (version 3)**: the stage, the next renewal, the three rights separately, the exams passed and
/// the lineage's attempts survive the round trip — non-default values, so a lost field cannot pass as a carried one.
#[test]
fn a_version_3_registry_row_carries_the_eligibility_stage() {
    let row = RpcPalwModelLifecycle {
        class_id: "cd".repeat(64),
        eligibility_stage: "probation".to_string(),
        eligibility_next_renewal_daa: 1_234_567,
        eligibility_subsidy: true,
        eligibility_block_right: true,
        eligibility_final_weight: false,
        eligibility_exams_passed: 2,
        eligibility_lineage_attempts: 5,
        ..Default::default()
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&row, &mut bytes).unwrap();
    let back = <RpcPalwModelLifecycle as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(format!("{back:?}"), format!("{row:?}"));
}
