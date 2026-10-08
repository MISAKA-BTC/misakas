//! **The canonical beacon facts a conformance run consumes, and the boundary they cross.**
//!
//! The chain, not this tool, knows the canonical PALW history: the position at which a commitment was accepted, the profiles that
//! were Active and G14-complete at that moment, and every Final useful-work settlement since. The node RPC that serves them does
//! not exist yet. [`BeaconFactSource`] is the one seam: lane D implements it over RPC; [`FileFactSource`] implements it over a file
//! of the same facts (`misaka.palw.beacon-facts.v1`), so a conformance run is reproducible from persisted public facts and a
//! unit test can say exactly which history it ran on.
//!
//! A facts source supplies only what the CHAIN knows ([`ChainBeaconFactsV1`]). Everything that the commitment fixes — the chain,
//! ruleset, subject kind, statement root and, above all, the challenge policy — is taken from the commitment, never from the
//! facts: a facts file cannot substitute a policy, and a policy id it names must be the committed one. What a source returns is
//! a claim about history to recompute, never an input to trust: [`resolve_facts`] refuses a context that does not exclude the
//! candidate under test (no self-beacon) and the beacon itself is derived by the contract's `collect_work_beacon_v1`, which sorts
//! what it is given, so arrival order, RPC order and a producer-selected list are not inputs.

use super::commit::{Refusal, hex, unhex64};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::PostCommitChallengePolicyV1;
use misaka_palw_challenge::beacon::{BeaconContextV1, FinalPathV1, WorkFinalEventV1, WorkSourceKindV1};
use misaka_palw_challenge::conformance::ConformanceCommitmentV1;
use misaka_palw_challenge::hash::Digest;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub const FACTS_SCHEMA_V1: &str = "misaka.palw.beacon-facts.v1";
pub const DOMAIN_FACTS: &[u8] = b"misaka.palw.runtime-pack.beacon-facts.v1";

/// Where the facts came from. A verdict reached on `Synthetic` facts says so and is never a statement about a chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum FactsProvenanceV1 {
    /// Made by a tool or a test for exercising the pipeline: NOT derived from any canonical history.
    Synthetic(String),
    /// Read from a node: the endpoint (and whatever the node attests about its tip) as a label.
    Node(String),
}

impl FactsProvenanceV1 {
    pub fn label(&self) -> String {
        match self {
            Self::Synthetic(s) => format!("synthetic:{s}"),
            Self::Node(s) => format!("node:{s}"),
        }
    }

    pub fn is_synthetic(&self) -> bool {
        matches!(self, Self::Synthetic(_))
    }
}

/// **What the chain supplies** for one commitment, on the branch the caller follows.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChainBeaconFactsV1 {
    pub provenance: FactsProvenanceV1,
    /// The accepted chain position of the commitment.
    pub commitment_position: u64,
    pub challenge_epoch: u64,
    /// Source profiles that were Active AND G14-complete in the commitment's state (frozen then).
    pub eligible_profiles: BTreeSet<Digest>,
    /// The candidate under test and every profile depending on its proposed semantics.
    pub excluded_profiles: BTreeSet<Digest>,
    /// Every settlement event of the branch (the contract sorts, de-duplicates and filters them).
    pub events: Vec<WorkFinalEventV1>,
    /// The branch's tip position the facts were read at.
    pub tip_position: u64,
}

impl ChainBeaconFactsV1 {
    pub fn digest(&self) -> Digest {
        super::commit::tool_root(DOMAIN_FACTS, self)
    }
}

/// **The loader boundary.** Lane D implements this over node RPC.
///
/// `commitment` is the committed statement (the chain looks the commitment up by `statement_root()`); `policy` is the committed
/// policy (a source that carries one names its id, and the consumer refuses any other).
pub trait BeaconFactSource {
    fn facts(&self, commitment: &ConformanceCommitmentV1, policy: &PostCommitChallengePolicyV1)
    -> Result<ChainBeaconFactsV1, Refusal>;
}

/// Facts read from a file of `misaka.palw.beacon-facts.v1` JSON.
#[derive(Clone, Debug)]
pub struct FileFactSource(pub PathBuf);

impl BeaconFactSource for FileFactSource {
    fn facts(
        &self,
        commitment: &ConformanceCommitmentV1,
        policy: &PostCommitChallengePolicyV1,
    ) -> Result<ChainBeaconFactsV1, Refusal> {
        let text =
            std::fs::read_to_string(&self.0).map_err(|e| Refusal::new("FACTS_UNREADABLE", format!("{}: {e}", self.0.display())))?;
        facts_from_json(&text, commitment, policy)
    }
}

/// Facts already in memory (tests, and a caller that read them from elsewhere).
#[derive(Clone, Debug)]
pub struct MemoryFactSource(pub ChainBeaconFactsV1);

impl BeaconFactSource for MemoryFactSource {
    fn facts(&self, _: &ConformanceCommitmentV1, _: &PostCommitChallengePolicyV1) -> Result<ChainBeaconFactsV1, Refusal> {
        Ok(self.0.clone())
    }
}

/// The contract's context for these facts, after the checks a consumer owes the commitment.
pub fn resolve_facts(
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
    facts: &ChainBeaconFactsV1,
) -> Result<BeaconContextV1, Refusal> {
    commitment.well_formed().map_err(|e| Refusal::new("COMMITMENT_INVALID", e))?;
    policy.validate().map_err(|e| Refusal::new("POLICY_INVALID", e.to_string()))?;
    if policy.id() != commitment.challenge_policy_id {
        return Err(Refusal::new("POLICY_SUBSTITUTED", "the policy is not the one the commitment names"));
    }
    if !facts.excluded_profiles.contains(&commitment.candidate_id) {
        return Err(Refusal::new(
            "FACTS_MISSING_SELF_EXCLUSION",
            "the facts do not exclude the candidate under test: a candidate must never seed its own conformance",
        ));
    }
    if facts.eligible_profiles.contains(&commitment.candidate_id) {
        return Err(Refusal::new(
            "FACTS_SELF_ELIGIBLE",
            "the facts list the candidate under test as an eligible beacon source profile",
        ));
    }
    if let Some(p) = commitment.canonical_commitment_position
        && p != facts.commitment_position
    {
        return Err(Refusal::new(
            "FACTS_POSITION_MISMATCH",
            format!("the commitment was observed at position {p}, the facts say {}", facts.commitment_position),
        ));
    }
    Ok(BeaconContextV1 {
        chain_genesis: commitment.chain_genesis,
        ruleset_id: commitment.ruleset_id,
        policy: policy.clone(),
        subject_kind: commitment.subject_kind,
        commitment_root: commitment.statement_root(),
        commitment_position: facts.commitment_position,
        challenge_epoch: facts.challenge_epoch,
        eligible_profiles: facts.eligible_profiles.clone(),
        excluded_profiles: facts.excluded_profiles.clone(),
    })
}

// ---------------------------------------------------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------------------------------------------------

fn kind_name(k: WorkSourceKindV1) -> &'static str {
    match k {
        WorkSourceKindV1::RealUsefulWork => "REAL_USEFUL_WORK",
        WorkSourceKindV1::Heartbeat => "HEARTBEAT",
        WorkSourceKindV1::Base0Fallback => "BASE0_FALLBACK",
        WorkSourceKindV1::ExecTx => "EXEC_TX",
        WorkSourceKindV1::ExecWorkSlice => "EXEC_WORK_SLICE",
        WorkSourceKindV1::ReceiptOnly => "RECEIPT_ONLY",
        WorkSourceKindV1::ProvisionalAttempt => "PROVISIONAL_ATTEMPT",
        WorkSourceKindV1::PanelReceipt => "PANEL_RECEIPT",
    }
}

fn kind_from(s: &str) -> Option<WorkSourceKindV1> {
    Some(match s {
        "REAL_USEFUL_WORK" => WorkSourceKindV1::RealUsefulWork,
        "HEARTBEAT" => WorkSourceKindV1::Heartbeat,
        "BASE0_FALLBACK" => WorkSourceKindV1::Base0Fallback,
        "EXEC_TX" => WorkSourceKindV1::ExecTx,
        "EXEC_WORK_SLICE" => WorkSourceKindV1::ExecWorkSlice,
        "RECEIPT_ONLY" => WorkSourceKindV1::ReceiptOnly,
        "PROVISIONAL_ATTEMPT" => WorkSourceKindV1::ProvisionalAttempt,
        "PANEL_RECEIPT" => WorkSourceKindV1::PanelReceipt,
        _ => return None,
    })
}

fn final_path_json(p: &FinalPathV1) -> Value {
    match p {
        FinalPathV1::PanelLicensed { panel_seed_id, panel_epoch } => {
            json!({ "kind": "PANEL_LICENSED", "panel_seed_id": hex(panel_seed_id), "panel_epoch": panel_epoch })
        }
        FinalPathV1::PanelIndependent => json!({ "kind": "PANEL_INDEPENDENT" }),
    }
}

/// How a work reached Final: required in every event (a missing path is a malformed file, never a default).
fn final_path_from(v: &Value) -> Result<FinalPathV1, Refusal> {
    let p = field(v, "final_path")?;
    match field(p, "kind")?.as_str() {
        Some("PANEL_LICENSED") => {
            Ok(FinalPathV1::PanelLicensed { panel_seed_id: digestf(p, "panel_seed_id")?, panel_epoch: u64f(p, "panel_epoch")? })
        }
        Some("PANEL_INDEPENDENT") => Ok(FinalPathV1::PanelIndependent),
        _ => Err(Refusal::new("FACTS_MALFORMED", "final_path.kind is PANEL_LICENSED or PANEL_INDEPENDENT")),
    }
}

/// The file form of facts (`misaka.palw.beacon-facts.v1`): digests are 128 lowercase hex characters.
pub fn facts_to_json(f: &ChainBeaconFactsV1, policy_id: &Digest) -> Value {
    let (pk, pl) = match &f.provenance {
        FactsProvenanceV1::Synthetic(s) => ("synthetic", s.clone()),
        FactsProvenanceV1::Node(s) => ("node", s.clone()),
    };
    json!({
        "schema": FACTS_SCHEMA_V1,
        "provenance": { "kind": pk, "label": pl },
        "policy_id": hex(policy_id),
        "commitment_position": f.commitment_position,
        "challenge_epoch": f.challenge_epoch,
        "eligible_profiles": f.eligible_profiles.iter().map(|d| hex(d)).collect::<Vec<_>>(),
        "excluded_profiles": f.excluded_profiles.iter().map(|d| hex(d)).collect::<Vec<_>>(),
        "tip_position": f.tip_position,
        "events": f.events.iter().map(|e| json!({
            "kind": kind_name(e.kind),
            "source_profile_id": hex(&e.source_profile_id),
            "canonical_work_id": hex(&e.canonical_work_id),
            "execution_commitment": hex(&e.execution_commitment),
            "accepted_position": e.accepted_position,
            "settlement_position": e.settlement_position,
            "occurrence_index": e.occurrence_index,
            "claim_final": e.claim_final,
            "da_satisfied": e.da_satisfied,
            "validity_independent": e.validity_independent,
            "depends_on_profiles": e.depends_on_profiles.iter().map(|d| hex(d)).collect::<Vec<_>>(),
            "final_path": final_path_json(&e.final_path),
        })).collect::<Vec<_>>(),
    })
}

fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value, Refusal> {
    v.get(k).ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("missing `{k}`")))
}

fn u64f(v: &Value, k: &str) -> Result<u64, Refusal> {
    field(v, k)?.as_u64().ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("`{k}` is not an unsigned integer")))
}

fn boolf(v: &Value, k: &str) -> Result<bool, Refusal> {
    field(v, k)?.as_bool().ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("`{k}` is not a boolean")))
}

fn digestf(v: &Value, k: &str) -> Result<Digest, Refusal> {
    let s = field(v, k)?.as_str().ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("`{k}` is not a string")))?;
    unhex64(s).map_err(|e| Refusal::new("FACTS_MALFORMED", format!("`{k}`: {e}")))
}

fn digests(v: &Value, k: &str) -> Result<Vec<Digest>, Refusal> {
    field(v, k)?
        .as_array()
        .ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("`{k}` is not a list")))?
        .iter()
        .map(|x| {
            x.as_str()
                .ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("`{k}` holds a non-string")))
                .and_then(|s| unhex64(s).map_err(|e| Refusal::new("FACTS_MALFORMED", format!("`{k}`: {e}"))))
        })
        .collect()
}

/// Parse a facts file for `commitment`/`policy`. A `policy_id` that is not the committed policy's is refused here.
pub fn facts_from_json(
    text: &str,
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
) -> Result<ChainBeaconFactsV1, Refusal> {
    let v: Value = serde_json::from_str(text).map_err(|e| Refusal::new("FACTS_MALFORMED", format!("not JSON: {e}")))?;
    if v.get("schema").and_then(Value::as_str) != Some(FACTS_SCHEMA_V1) {
        return Err(Refusal::new("FACTS_MALFORMED", format!("the schema is not {FACTS_SCHEMA_V1}")));
    }
    let named = digestf(&v, "policy_id")?;
    if named != policy.id() || named != commitment.challenge_policy_id {
        return Err(Refusal::new("POLICY_SUBSTITUTED", "the facts name another challenge policy than the committed one"));
    }
    let prov = field(&v, "provenance")?;
    let label = field(prov, "label")?.as_str().unwrap_or_default().to_string();
    let provenance = match field(prov, "kind")?.as_str() {
        Some("synthetic") => FactsProvenanceV1::Synthetic(label),
        Some("node") => FactsProvenanceV1::Node(label),
        _ => return Err(Refusal::new("FACTS_MALFORMED", "provenance.kind is `synthetic` or `node`")),
    };
    let mut events = Vec::new();
    for e in field(&v, "events")?.as_array().ok_or_else(|| Refusal::new("FACTS_MALFORMED", "`events` is not a list"))? {
        let k = field(e, "kind")?.as_str().unwrap_or_default();
        events.push(WorkFinalEventV1 {
            kind: kind_from(k).ok_or_else(|| Refusal::new("FACTS_MALFORMED", format!("unknown event kind `{k}`")))?,
            source_profile_id: digestf(e, "source_profile_id")?,
            canonical_work_id: digestf(e, "canonical_work_id")?,
            execution_commitment: digestf(e, "execution_commitment")?,
            accepted_position: u64f(e, "accepted_position")?,
            settlement_position: u64f(e, "settlement_position")?,
            occurrence_index: u32::try_from(u64f(e, "occurrence_index")?)
                .map_err(|_| Refusal::new("FACTS_MALFORMED", "occurrence_index"))?,
            claim_final: boolf(e, "claim_final")?,
            da_satisfied: boolf(e, "da_satisfied")?,
            validity_independent: boolf(e, "validity_independent")?,
            depends_on_profiles: digests(e, "depends_on_profiles")?,
            final_path: final_path_from(e)?,
        });
    }
    Ok(ChainBeaconFactsV1 {
        provenance,
        commitment_position: u64f(&v, "commitment_position")?,
        challenge_epoch: u64f(&v, "challenge_epoch")?,
        eligible_profiles: digests(&v, "eligible_profiles")?.into_iter().collect(),
        excluded_profiles: digests(&v, "excluded_profiles")?.into_iter().collect(),
        events,
        tip_position: u64f(&v, "tip_position")?,
    })
}
