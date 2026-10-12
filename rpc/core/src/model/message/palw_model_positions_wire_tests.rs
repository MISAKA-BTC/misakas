//! **`message`'s `palw_model_positions_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn a_tier() -> RpcPalwModelBenefitTier {
    RpcPalwModelBenefitTier {
        min_units: 2,
        grants: 0b1000_0100,
        grant_names: vec!["PRIORITY_INFERENCE".to_string(), "SUPPORT".to_string()],
        lead_daa: 0,
        min_hold_daa: 50,
        note: "two jobs ahead".to_string(),
    }
}

fn a_response() -> GetPalwModelPositionsResponse {
    GetPalwModelPositionsResponse {
        holder: "b0".repeat(64),
        positions: vec![
            RpcPalwModelPosition {
                line_id: "c1".repeat(64),
                units: 4_656,
                holding_since_daa: Some(260),
                tenure_daa: 60,
                tier_index: Some(1),
                tier: Some(a_tier()),
            },
            // A holding with no clock and no tier, so a field read from the wrong slot, or a
            // `None` written as a value, cannot pass as carried.
            RpcPalwModelPosition { line_id: "c2".repeat(64), units: 7, ..Default::default() },
        ],
        tip_daa: 320,
        tip_hash: "a7".repeat(64),
    }
}

#[test]
fn the_tenure_and_the_tier_survive_the_borsh_round_trip() {
    let response = a_response();
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwModelPositionsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.tip_daa, 320, "the height a challenge names");
    assert_eq!(back.tip_hash, "a7".repeat(64), "and the block that height was read at");
    assert_eq!(back.positions, response.positions, "every row, the clock and the tier included");
    assert_eq!(back.positions[1].tier_index, None, "no tier stays no tier");
}

/// **A version-1 writer reads FAIL-CLOSED.** An older node computed no tier; reading its silence
/// as tier 0 would serve a stranger on a peer's ignorance.
#[test]
fn a_version_one_writer_reads_as_no_clock_and_no_tier() {
    let mut row = Vec::new();
    store!(u16, &1, &mut row).unwrap();
    store!(String, &"c1".repeat(64), &mut row).unwrap();
    store!(u64, &4_656, &mut row).unwrap();
    // `serialize!` frames the Vec as one payload, and each row inside it as its own.
    let mut rows = Vec::new();
    store!(u32, &1, &mut rows).unwrap();
    store!(Vec<u8>, &row, &mut rows).unwrap();
    let mut v1 = Vec::new();
    store!(u16, &1, &mut v1).unwrap();
    store!(String, &"b0".repeat(64), &mut v1).unwrap();
    store!(Vec<u8>, &rows, &mut v1).unwrap();

    let back = <GetPalwModelPositionsResponse as Deserializer>::deserialize(&mut v1.as_slice()).unwrap();
    assert_eq!(back.positions.len(), 1);
    let p = &back.positions[0];
    assert_eq!(p.line_id, "c1".repeat(64), "everything version 1 DID say is read");
    assert_eq!(p.units, 4_656);
    assert_eq!((p.holding_since_daa, p.tenure_daa, p.tier_index), (None, 0, None), "and nothing it did not");
    assert!(p.tier.is_none(), "no tier, never tier 0");
    assert_eq!(back.tip_daa, 0, "a peer that read no tier names no height");
    assert!(back.tip_hash.is_empty(), "…and no tip");
}

/// **A version-1 READER is not broken by a version-2 frame** — the property that lets a node
/// ship this without its peers' clients. `serialize!` frames the Vec, and each row inside it,
/// as a length-prefixed payload, so the appended fields are skipped with the row; the
/// response's own tail is past the last field an old reader loads. Spelled as the old
/// deserializer, field by field.
#[test]
fn a_version_one_reader_still_reads_a_version_two_frame() {
    let response = a_response();
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();

    let mut r = bytes.as_slice();
    let _version = load!(u16, &mut r).unwrap();
    assert_eq!(load!(String, &mut r).unwrap(), response.holder);
    let framed = load!(Vec<u8>, &mut r).unwrap();
    let mut rows = framed.as_slice();
    assert_eq!(load!(u32, &mut rows).unwrap(), 2);
    for want in &response.positions {
        let payload = load!(Vec<u8>, &mut rows).unwrap();
        let mut row = payload.as_slice();
        let _row_version = load!(u16, &mut row).unwrap();
        assert_eq!(load!(String, &mut row).unwrap(), want.line_id);
        assert_eq!(load!(u64, &mut row).unwrap(), want.units, "the old reader's units are the new writer's units");
        assert!(!row.is_empty(), "version 2's fields are in the row's frame, and an old reader leaves them there");
    }
    let mut tail = r;
    assert_eq!(load!(u64, &mut tail).unwrap(), 320, "past the rows an old reader stops, and `tipDaa` is left behind…");
    assert_eq!(load!(String, &mut tail).unwrap(), response.tip_hash, "…with `tipHash`");
    assert!(tail.is_empty());
}
