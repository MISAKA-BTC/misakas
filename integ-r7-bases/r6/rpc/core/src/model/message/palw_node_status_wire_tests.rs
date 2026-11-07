//! **`message`'s `palw_node_status_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn version_five_bytes(response: &GetPalwNodeStatusResponse) -> Vec<u8> {
    let mut bytes = Vec::new();
    Serializer::serialize(response, &mut bytes).unwrap();
    let mut tail = Vec::new();
    store!(bool, &response.import_session_available, &mut tail).unwrap();
    store!(u64, &response.import_session_epoch, &mut tail).unwrap();
    store!(String, &response.import_session_point, &mut tail).unwrap();
    store!(String, &response.import_session_phase, &mut tail).unwrap();
    store!(bool, &response.pruning_import_stable, &mut tail).unwrap();
    bytes.truncate(bytes.len() - tail.len());
    bytes[..2].copy_from_slice(&5u16.to_le_bytes());
    bytes
}

/// **Version 4 carries the genesis and the drill id; a version-3 sender decodes with both
/// empty** — which `palw_node_genesis_verdict_v1` reads as "predates the salt, so no drill".
#[test]
fn the_node_status_carries_its_genesis_and_a_v3_sender_carries_none() {
    let response = GetPalwNodeStatusResponse {
        consensus_params_id: "ab".repeat(32),
        lane_alarm: "quiet".to_string(),
        genesis_hash: "cd".repeat(64),
        drill_salt_id: "0123456789abcdef".to_string(),
        ..Default::default()
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!((back.genesis_hash.as_str(), back.drill_salt_id.as_str()), (response.genesis_hash.as_str(), "0123456789abcdef"));
    assert_eq!(back.lane_alarm, "quiet", "the version-3 fields before them are where they were");

    // A version-3 sender: the same bytes without the two version-4 strings and the version-5 one, version 3.
    let public = GetPalwNodeStatusResponse { genesis_hash: String::new(), drill_salt_id: String::new(), ..response };
    let v5 = version_five_bytes(&public);
    let mut empty = Vec::new();
    store!(String, &String::new(), &mut empty).unwrap();
    let mut v3 = v5[..v5.len() - 3 * empty.len()].to_vec();
    v3[..2].copy_from_slice(&3u16.to_le_bytes());
    let old = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut v3.as_slice()).unwrap();
    assert_eq!((old.genesis_hash.as_str(), old.drill_salt_id.as_str(), old.lane_alarm.as_str()), ("", "", "quiet"));
}

/// **Version 5 carries the verification debt; a version-4 sender decodes with it empty** — and the
/// version-4 fields before it are where they were.
#[test]
fn the_node_status_carries_its_verification_debt_and_a_v4_sender_carries_none() {
    let response = GetPalwNodeStatusResponse {
        consensus_params_id: "ab".repeat(32),
        genesis_hash: "cd".repeat(64),
        drill_salt_id: "0123456789abcdef".to_string(),
        verification: "seat_receipts_1h=61 slots=2 cgroup_naive_mib=268 cgroup_credited_mib=9306".to_string(),
        ..Default::default()
    };
    let mut v5 = Vec::new();
    Serializer::serialize(&response, &mut v5).unwrap();
    let back = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut v5.as_slice()).unwrap();
    assert_eq!(back.verification, response.verification);
    assert_eq!(back.drill_salt_id, "0123456789abcdef");
    let mut empty = Vec::new();
    store!(String, &String::new(), &mut empty).unwrap();
    let without = GetPalwNodeStatusResponse { verification: String::new(), ..response };
    let v4 = version_five_bytes(&without);
    let mut v4 = v4[..v4.len() - empty.len()].to_vec();
    v4[..2].copy_from_slice(&4u16.to_le_bytes());
    let old = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut v4.as_slice()).unwrap();
    assert_eq!((old.verification.as_str(), old.drill_salt_id.as_str()), ("", "0123456789abcdef"));
}

#[test]
fn rfc12_node_status_frames_session_diagnostics_and_older_senders_are_unknown() {
    let response = GetPalwNodeStatusResponse {
        genesis_hash: "ab".repeat(64),
        verification: "unchanged-prefix".into(),
        import_session_available: true,
        import_session_epoch: 41,
        import_session_point: "cd".repeat(64),
        import_session_phase: "utxo-stream".into(),
        pruning_import_stable: false,
        ..Default::default()
    };
    let mut wire = Vec::new();
    Serializer::serialize(&response, &mut wire).unwrap();
    let mut framed = wire.as_slice();
    let back = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut framed).unwrap();
    assert!(framed.is_empty(), "the v6 decoder consumes exactly its framed diagnostics");
    assert!(back.import_session_available && !back.pruning_import_stable);
    assert_eq!(
        (back.import_session_epoch, back.import_session_point.as_str(), back.import_session_phase.as_str()),
        (41, response.import_session_point.as_str(), "utxo-stream")
    );
    assert_eq!(back.verification, "unchanged-prefix");
    let v5 = version_five_bytes(&response);
    let old = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut v5.as_slice()).unwrap();
    assert!(!old.import_session_available, "absent diagnostics cannot mean a verified stable import");
    assert_eq!((old.import_session_epoch, old.import_session_point.as_str(), old.import_session_phase.as_str()), (0, "", ""));
    assert_eq!(old.verification, response.verification);
    // Prefix compatibility control: the pre-v6 fields consume the same
    // bytes, leaving the appended tail. This does not execute an old client.
    let mut old_reader_frame = wire.clone();
    old_reader_frame[..2].copy_from_slice(&5u16.to_le_bytes());
    let mut old_reader_frame = old_reader_frame.as_slice();
    let prefix = <GetPalwNodeStatusResponse as Deserializer>::deserialize(&mut old_reader_frame).unwrap();
    assert_eq!(prefix.verification, response.verification);
    assert_eq!(old_reader_frame.len(), wire.len() - v5.len());
    let mut json = serde_json::to_value(&response).unwrap();
    for key in ["importSessionAvailable", "importSessionEpoch", "importSessionPoint", "importSessionPhase", "pruningImportStable"] {
        json.as_object_mut().unwrap().remove(key);
    }
    let old_json: GetPalwNodeStatusResponse = serde_json::from_value(json).unwrap();
    assert!(!old_json.import_session_available && old_json.import_session_point.is_empty());
}
