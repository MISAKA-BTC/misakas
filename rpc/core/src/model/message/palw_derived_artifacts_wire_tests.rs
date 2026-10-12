//! **`message`'s `palw_derived_artifacts_wire_tests` module**, moved out of `message.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;

fn a_row() -> RpcPalwDerivedArtifact {
    RpcPalwDerivedArtifact {
        transformer_id: "7a".repeat(64),
        derived_id: "d1".repeat(64),
        grammar_id: "6a".repeat(64),
        kind: 6,
        kind_name: "music".to_string(),
        dsl_hash: "d5".repeat(64),
        artifact_hash: "a7".repeat(64),
        artifact_bytes: 4_096,
        accepted_daa: 91_337,
    }
}

fn a_response() -> GetPalwDerivedArtifactsResponse {
    GetPalwDerivedArtifactsResponse {
        found: true,
        claim_id: "cc".repeat(64),
        output_root: "07".repeat(64),
        executor_pubkey: "ab".repeat(2592),
        executor_bond: format!("{}:3", "b0".repeat(32)),
        class_id: "c1".repeat(64),
        claim_phase: "voided".to_string(),
        claim_void_reason: "receipt_timeout".to_string(),
        claim_accepted_block: "bb".repeat(32),
        claim_accepted_daa: 91_300,
        artifacts: vec![a_row(), RpcPalwDerivedArtifact { transformer_id: "7b".repeat(64), ..a_row() }],
    }
}

/// Every field, both directions. The two that would be silently lost are the ones the row does
/// not repeat: `transformer_id` lives in the state table's KEY, and `output_root` on the CLAIM
/// — a verifier that received neither would be checking a `dsl_hash` against nothing.
#[test]
fn the_derived_rows_survive_the_borsh_round_trip() {
    let response = a_response();
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwDerivedArtifactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(back.found);
    assert_eq!(back.output_root, response.output_root, "X6 recomputes against this and nothing else");
    assert_eq!(back.executor_pubkey, response.executor_pubkey, "whose name is on the provenance");
    assert_eq!(back.executor_bond, response.executor_bond);
    assert_eq!(back.claim_phase, "voided", "Decision 4: a derivation of a voided claim says so when read");
    assert_eq!(back.claim_void_reason, "receipt_timeout");
    assert_eq!(back.artifacts.len(), 2);
    assert_eq!(back.artifacts[0].transformer_id, response.artifacts[0].transformer_id, "the key's half of the row");
    assert_eq!(back.artifacts[1].transformer_id, response.artifacts[1].transformer_id);
    assert_eq!(back.artifacts[0].dsl_hash, response.artifacts[0].dsl_hash);
    assert_eq!(back.artifacts[0].artifact_hash, response.artifacts[0].artifact_hash);
    assert_eq!(back.artifacts[0].artifact_bytes, 4_096);
    assert_eq!(back.artifacts[0].kind, 6);
    assert_eq!(back.artifacts[0].derived_id, response.artifacts[0].derived_id);
    assert_eq!(back.artifacts[0].accepted_daa, 91_337);
}

/// A claim this chain does not hold reads as `found: false` with nothing filled in — the
/// answer a node gives about another chain's claim, and not an error.
#[test]
fn an_unknown_claim_round_trips_as_not_found() {
    let mut bytes = Vec::new();
    Serializer::serialize(&GetPalwDerivedArtifactsResponse::default(), &mut bytes).unwrap();
    let back = <GetPalwDerivedArtifactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert!(!back.found);
    assert!(back.artifacts.is_empty());
    assert!(back.output_root.is_empty(), "no output_root is not the empty hash");
}

#[test]
fn the_claim_facts_survive_the_borsh_round_trip() {
    let response = GetPalwFreePromptClaimResponse {
        found: true,
        claim_id: "cc".repeat(64),
        is_free_prompt: true,
        class_id: "c1".repeat(64),
        executor_pubkey: "ab".repeat(2592),
        executor_bond: format!("{}:3", "b0".repeat(32)),
        output_root: "07".repeat(64),
        trace_root: "77".repeat(64),
        execution_root: "e7".repeat(64),
        work_leaves: 4_194_304,
        work_id: "17".repeat(64),
        quanta: 8,
        quanta_spent: 3,
        phase: "final".to_string(),
        void_reason: String::new(),
        phase_daa: 91_500,
        accepted_block: "bb".repeat(32),
        accepted_daa: 91_300,
        trace_retention_daa: 100_000,
        derived_count: 2,
    };
    let mut bytes = Vec::new();
    Serializer::serialize(&response, &mut bytes).unwrap();
    let back = <GetPalwFreePromptClaimResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.output_root, response.output_root);
    assert_eq!(back.trace_root, response.trace_root);
    assert_eq!(back.execution_root, response.execution_root);
    assert_eq!((back.quanta, back.quanta_spent), (8, 3));
    assert_eq!(back.work_leaves, 4_194_304);
    assert_eq!(back.work_id, response.work_id);
    assert_eq!(back.phase, "final");
    assert_eq!(back.derived_count, 2);
    assert!(back.is_free_prompt);
}
