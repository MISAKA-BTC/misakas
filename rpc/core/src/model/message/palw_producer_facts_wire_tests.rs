//! **`message`'s `palw_producer_facts_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn v4_response() -> GetPalwProducerFactsResponse {
    GetPalwProducerFactsResponse {
        available: true,
        chain_point: "cc".repeat(64),
        daa_score: 1_234,
        class_id: "aa".repeat(64),
        artifact_root: "bb".repeat(64),
        class_target: "340282366920938463463374607431768211455".to_string(),
        pwu: 7_708,
        is_base_class: false,
        min_trace_retention_daa: 9_000,
        epoch_index: 12,
        epoch_budget_blocks: 40,
        epoch_produced_blocks: 3,
        bond_known: true,
        bond_registered_pubkey: "dd".repeat(32),
        bond_operator_id: "ee".repeat(64),
        bond_collateral: 20_000_000_000,
        bond_reserved_exposure: "1000".to_string(),
        bond_exposure_ceiling: "10000000".to_string(),
        bond_claim_exposure: "250".to_string(),
        not_ready_reason: String::new(),
        fp_certified: true,
        fp_quanta_per_canonical_job: 8,
        fp_max_quanta_per_receipt: 64,
        // ADR-0082 Decisions 10/11: `true` so the round trip distinguishes "carried" from
        // "defaulted" — the field's own default is false and a lost field would pass silently.
        fp_decode_rules_armed: true,
        locked_bond_outpoints: vec![format!("{}:0", "aa".repeat(64)), format!("{}:7", "bb".repeat(64))],
        // Version 5 and 6 fields, non-default so a lost field cannot pass as a carried one.
        palw_retention_dir: "/var/lib/misaka/palw".to_string(),
        panel_da_armed: true,
        prompt_ids_merkle: true,
        // Version 7 (ADR-0096 Decision 8), non-default for the same reason.
        fp_decode_constraint_armed: true,
        // Version 8 (ADR-0118 Decision 3): DIFFERENT from `prompt_ids_merkle`, so a field lost
        // on the wire, or read from the wrong slot, cannot pass as carried.
        class_prompt_ids_merkle: false,
        // Version 9 (ADR-0152 P6): committed DIFFERENT from reserved, so the old ledger cannot
        // pass as the new one.
        bond_committed: "7000".to_string(),
        bond_producer_floor_shortfall: 4_200,
        bond_accuser_exposure: "320".to_string(),
    }
}

/// **The borsh wire, both directions.** wRPC carries this response through `Serializer` /
/// `Deserializer`, and a gateway that reads its class's certification over wRPC gets a
/// *different* answer from a gRPC one if the two encodings disagree — which is exactly the
/// failure the version prefix exists to make impossible.
#[test]
fn the_producer_facts_survive_the_borsh_round_trip() {
    let response = v4_response();
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(back.fp_certified, "ADR-0077 Decision 3: a gateway cannot name its refusal without this");
    assert_eq!(back.fp_quanta_per_canonical_job, 8, "the lane's price is the chain's, not the gateway's");
    assert_eq!(back.fp_max_quanta_per_receipt, 64);
    assert!(back.fp_decode_rules_armed, "ADR-0082 D10/D11: a builder on the wrong side of this fence is unreproducible");
    assert!(back.fp_decode_constraint_armed, "ADR-0096 D8: an entrance that loses this serves a committed format nobody replays");
    assert!(back.prompt_ids_merkle && !back.class_prompt_ids_merkle, "ADR-0118 D3: the class's form is its own field");
    assert_eq!(back.locked_bond_outpoints, response.locked_bond_outpoints, "the must-not-spend set must not shorten");
    assert_eq!(back.class_target, response.class_target);
    assert_eq!(back.bond_exposure_ceiling, response.bond_exposure_ceiling);
    assert_eq!(back.not_ready_reason, response.not_ready_reason);
    assert_eq!(
        (back.bond_committed.as_str(), back.bond_producer_floor_shortfall, back.bond_accuser_exposure.as_str()),
        ("7000", 4_200, "320"),
        "ADR-0152 P6: the one ledger, the floor and the accuser ledger"
    );
}

/// **ADR-0080 design A: a declared close's arrival bitmap survives the wRPC wire.**
///
/// `present` is the field a resume is decided on and it is a BITMAP because chunks arrive in any
/// order — so an encoding that lost a bit, or that carried a count of parts instead, would send
/// a mover under a court deadline to re-pay for a carrier the chain already holds and leave the
/// hole that convicts it. A sparse pattern with a gap in the middle and the top index set is the
/// one that catches that; a full or empty bitmap round-trips through either shape.
#[test]
fn the_declared_close_survives_the_wrpc_round_trip() {
    let response = GetPalwPendingChunkGroupResponse {
        found: true,
        session_id: "5e".repeat(64),
        side: "executor".to_string(),
        count: 27,
        present: 0b1011 | (1u64 << 26),
        parts_present: 4,
        complete: false,
        declared_daa: 91_300,
        assembly_deadline_daa: 91_408,
        close_digest: "c1".repeat(64),
        verdict: "executor_guilty".to_string(),
        declarer_bond: format!("{}:1", "b0".repeat(32)),
        deposit: 33_750_000,
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwPendingChunkGroupResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.present, response.present, "the arrival bitmap did not survive the wire");
    assert_eq!(back.count, 27);
    assert_eq!(back.parts_present, 4);
    assert!(!back.complete);
    assert_eq!(back.side, "executor");
    assert_eq!(back.close_digest, response.close_digest);
    assert_eq!(back.assembly_deadline_daa, 91_408);
    assert_eq!(back.declarer_bond, response.declarer_bond);
    assert_eq!(back.deposit, 33_750_000);
    // The default is the honest negative every non-ConsensusV2 node answers with.
    let mut bytes = Vec::new();
    Serializer::serialize(&GetPalwPendingChunkGroupResponse::default(), &mut bytes).unwrap();
    let back = <GetPalwPendingChunkGroupResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(!back.found && back.count == 0 && back.present == 0);
}

/// **An older writer is read fail-CLOSED.** A version-2 peer knows nothing about the free-prompt
/// lane; reading its silence as "certified" would make a gateway submit on a peer's ignorance.
/// Uncertified and unpriced is the safe reading, and it costs a retry, not a fee.
#[test]
fn a_version_two_writer_reads_as_uncertified_and_unpriced() {
    let r = v4_response();
    let mut v2 = Vec::new();
    store!(u16, &2, &mut v2).unwrap();
    store!(bool, &r.available, &mut v2).unwrap();
    store!(String, &r.chain_point, &mut v2).unwrap();
    store!(u64, &r.daa_score, &mut v2).unwrap();
    store!(String, &r.class_id, &mut v2).unwrap();
    store!(String, &r.artifact_root, &mut v2).unwrap();
    store!(String, &r.class_target, &mut v2).unwrap();
    store!(u64, &r.pwu, &mut v2).unwrap();
    store!(bool, &r.is_base_class, &mut v2).unwrap();
    store!(u64, &r.min_trace_retention_daa, &mut v2).unwrap();
    store!(u64, &r.epoch_index, &mut v2).unwrap();
    store!(u64, &r.epoch_budget_blocks, &mut v2).unwrap();
    store!(u64, &r.epoch_produced_blocks, &mut v2).unwrap();
    store!(bool, &r.bond_known, &mut v2).unwrap();
    store!(String, &r.bond_registered_pubkey, &mut v2).unwrap();
    store!(String, &r.bond_operator_id, &mut v2).unwrap();
    store!(u64, &r.bond_collateral, &mut v2).unwrap();
    store!(String, &r.bond_reserved_exposure, &mut v2).unwrap();
    store!(String, &r.bond_exposure_ceiling, &mut v2).unwrap();
    store!(String, &r.bond_claim_exposure, &mut v2).unwrap();
    store!(String, &r.not_ready_reason, &mut v2).unwrap();
    store!(Vec<String>, &r.locked_bond_outpoints, &mut v2).unwrap();

    let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut v2.as_slice()).unwrap();
    assert_eq!(back.locked_bond_outpoints, r.locked_bond_outpoints, "everything version 2 DID say is read");
    assert!(!back.fp_certified, "silence is not certification");
    assert_eq!((back.fp_quanta_per_canonical_job, back.fp_max_quanta_per_receipt), (0, 0), "and it prices nothing");
    assert!(!back.fp_decode_rules_armed, "and a peer older than the fence is not asserting it is dormant either");
    assert!(!back.fp_decode_constraint_armed, "nor that ADR-0096's constraint fence is (version 7 reads false)");
}

/// **A version-THREE writer reads as decode-rules-dormant**, and everything version 3 did say
/// is still read. The suffix property one version on: each version's fields are a strict
/// prefix of the next, so a new field can never shift an old one.
#[test]
fn a_version_three_writer_reads_as_decode_rules_dormant() {
    let r = v4_response();
    let mut v3 = Vec::new();
    store!(u16, &3, &mut v3).unwrap();
    store!(bool, &r.available, &mut v3).unwrap();
    store!(String, &r.chain_point, &mut v3).unwrap();
    store!(u64, &r.daa_score, &mut v3).unwrap();
    store!(String, &r.class_id, &mut v3).unwrap();
    store!(String, &r.artifact_root, &mut v3).unwrap();
    store!(String, &r.class_target, &mut v3).unwrap();
    store!(u64, &r.pwu, &mut v3).unwrap();
    store!(bool, &r.is_base_class, &mut v3).unwrap();
    store!(u64, &r.min_trace_retention_daa, &mut v3).unwrap();
    store!(u64, &r.epoch_index, &mut v3).unwrap();
    store!(u64, &r.epoch_budget_blocks, &mut v3).unwrap();
    store!(u64, &r.epoch_produced_blocks, &mut v3).unwrap();
    store!(bool, &r.bond_known, &mut v3).unwrap();
    store!(String, &r.bond_registered_pubkey, &mut v3).unwrap();
    store!(String, &r.bond_operator_id, &mut v3).unwrap();
    store!(u64, &r.bond_collateral, &mut v3).unwrap();
    store!(String, &r.bond_reserved_exposure, &mut v3).unwrap();
    store!(String, &r.bond_exposure_ceiling, &mut v3).unwrap();
    store!(String, &r.bond_claim_exposure, &mut v3).unwrap();
    store!(String, &r.not_ready_reason, &mut v3).unwrap();
    store!(Vec<String>, &r.locked_bond_outpoints, &mut v3).unwrap();
    store!(bool, &r.fp_certified, &mut v3).unwrap();
    store!(u32, &r.fp_quanta_per_canonical_job, &mut v3).unwrap();
    store!(u32, &r.fp_max_quanta_per_receipt, &mut v3).unwrap();

    let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut v3.as_slice()).unwrap();
    assert!(back.fp_certified, "everything version 3 DID say is read");
    assert_eq!(back.fp_quanta_per_canonical_job, r.fp_quanta_per_canonical_job);
    assert_eq!(back.fp_max_quanta_per_receipt, r.fp_max_quanta_per_receipt);
    assert!(!back.fp_decode_rules_armed, "a version-3 peer knows nothing about the fence — read fail-closed");
    assert_eq!(
        (back.bond_committed.as_str(), back.bond_producer_floor_shortfall, back.bond_accuser_exposure.as_str()),
        (r.bond_reserved_exposure.as_str(), 0, "0"),
        "a pre-P6 peer: its one ledger, no floor, no accuser ledger"
    );
}
