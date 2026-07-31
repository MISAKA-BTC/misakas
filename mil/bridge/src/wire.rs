//! Wire types of the palw-gateway coordinator protocol v1 — field-for-field the JSON the
//! gateway's `HttpCoordinator` speaks (palw-gateway README "PALW coordinator protocol"). The
//! gateway repo is a separate checkout, so the shapes are mirrored here and PINNED by the
//! `gateway_wire_shapes_are_frozen` test below; changing either side is a protocol rev, not a
//! refactor.

use serde::{Deserialize, Serialize};

/// The qi35 engine's per-generation execution roots (`ROOTS route= kv= state=`): the MoE
/// routing-trace root, the KV-cache root, and the recurrent-state root, hex-encoded.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRootsV1 {
    pub route: String,
    pub kv: String,
    pub state: String,
}

/// A-commit: what a submitting gateway registers for replication. `prompt_ids` doubles as the
/// context payload handed to the replica (Conversation-DA in miniature).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobSubmissionV1 {
    pub job_id: String,
    pub provider_id: String,
    pub prompt_ids: Vec<u32>,
    pub max_new: u32,
    /// blake2b-256 over the little-endian output ids — A's claim.
    pub output_root: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub receipt_json: Option<String>,
    /// REQUIRED by this bridge (qi35-serve class): the match key covers execution structure.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub runtime_roots: Option<RuntimeRootsV1>,
    /// Seam 1 — the challenge this bridge leased BEFORE generation. Required when the bridge
    /// runs with consensus seams enabled.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub job_challenge: Option<String>,
    /// The answer's token ids — needed for the salted receipt-v3 commitment and for the DA
    /// context object an auditor replays from.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output_token_ids: Option<Vec<u32>>,
    /// `output_commitment_v3(output_token_ids, job_challenge)`, hex.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output_commitment: Option<String>,
}

/// B's answer for an assignment.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicaResultV1 {
    pub job_id: String,
    pub provider_id: String,
    pub output_root: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub runtime_roots: Option<RuntimeRootsV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplicaAssignmentV1 {
    pub job_id: String,
    pub prompt_ids: Vec<u32>,
    pub max_new: u32,
    /// Absolute unix-ms deadline; the replica computes remaining time for admission.
    pub deadline_unix_ms: i64,
}

/// Verdict names on the wire — the gateway maps them onto its `TurnVerificationStatus` walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobVerdictV1 {
    ReplicaMatched,
    Certified,
    Mismatch,
}

impl JobVerdictV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplicaMatched => "replica_matched",
            Self::Certified => "certified",
            Self::Mismatch => "mismatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The EXACT JSON a current palw-gateway worker emits (captured from its `JobSubmission` /
    /// `ReplicaResultV1` serde shapes). If this test fails, the protocol drifted.
    #[test]
    fn gateway_wire_shapes_are_frozen() {
        let submission = r#"{
            "job_id": "turn_ab",
            "provider_id": "prov-a",
            "prompt_ids": [1, 2, 3],
            "max_new": 256,
            "output_root": "aabb",
            "receipt_json": "{\"v\":3}",
            "runtime_roots": { "route": "r1", "kv": "k1", "state": "s1" }
        }"#;
        let parsed: JobSubmissionV1 = serde_json::from_str(submission).unwrap();
        assert_eq!(parsed.prompt_ids, vec![1, 2, 3]);
        assert_eq!(parsed.runtime_roots.as_ref().unwrap().route, "r1");

        // Optional fields really are optional (an older gateway omits them).
        let minimal = r#"{"job_id":"j","provider_id":"p","prompt_ids":[],"max_new":1,"output_root":"00"}"#;
        let parsed: JobSubmissionV1 = serde_json::from_str(minimal).unwrap();
        assert!(parsed.runtime_roots.is_none() && parsed.receipt_json.is_none());

        let result = r#"{"job_id":"j","provider_id":"p","output_root":"00","runtime_roots":{"route":"r","kv":"k","state":"s"}}"#;
        let parsed: ReplicaResultV1 = serde_json::from_str(result).unwrap();
        assert_eq!(parsed.runtime_roots.unwrap().kv, "k");

        // Assignment + verdict names as the gateway parses them.
        let assignment = ReplicaAssignmentV1 { job_id: "j".into(), prompt_ids: vec![9], max_new: 4, deadline_unix_ms: 5 };
        let json = serde_json::to_value(&assignment).unwrap();
        assert_eq!(json["deadline_unix_ms"], 5);
        assert_eq!(serde_json::to_value(JobVerdictV1::ReplicaMatched).unwrap(), "replica_matched");
        assert_eq!(JobVerdictV1::Certified.as_str(), "certified");
    }
}
