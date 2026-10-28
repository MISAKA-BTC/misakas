//! **ADR-0078 Decision 5 — the consumer's read path** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_consumer_read::*`).

use super::*;

// ------------------------------------------------------------------------------------------
// ADR-0078 Decision 5 — the consumer's read path
// ------------------------------------------------------------------------------------------
//
// "Verification belongs to the consumer, and the chain makes it possible." The chain stores one
// `DerivedArtifactV1` per (claim, transformer) and never checks its content: the whole guarantee
// is that anyone holding the answer can recompute `output_root`, `dsl_hash` and `artifact_hash`
// and compare them with what the chain says (invariant X6). That is a promise about ids the
// consumer can FETCH, and until these two calls existed the `derived_artifacts` table had no
// reader outside the transition that wrote it.
//
// **What is NOT here, deliberately: `output_token_ids`.** The claim commits
// `output_root = output_commitment_v2(job_context_hash, ids, family_rendered_hash)` — the ids are
// not on the chain in any form, and ADR-0044 Decision 8's sentence about not publishing prompts
// applies to answers word for word. So a verifier holds the ids from the gateway's own response
// (`misaka.output_token_ids`, beside `job_context_hash` and `family`) and these calls return the
// chain-side facts to check them against. A response that carried the ids would be the chain
// publishing the answer.

/// ADR-0078 Decision 5: which claim's derivations to read. 128-hex `fp_claim_id`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwDerivedArtifactsRequest {
    /// 128-hex free-prompt claim id (`fp_claim_id`, what the gateway returns and what the
    /// derivation's `claim_id` names).
    pub claim_id: String,
}

impl Serializer for GetPalwDerivedArtifactsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwDerivedArtifactsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let claim_id = load!(String, reader)?;
        Ok(Self { claim_id })
    }
}

/// **One row of the chain's derived-artifact table** (ADR-0078 Decision 4), as a reader sees it.
///
/// The key's half and the row's half in one record: `transformer_id` is the key's, and the state
/// row does not repeat it, so a response that returned rows alone would hand a verifier a
/// `dsl_hash` with no way to name the function that produced it.
///
/// `kind_name` is resolved from the kind table shipped in `kaspa-consensus-core`, and it is a
/// convenience for a human reader and nothing more: the chain checks `kind != 0` and interprets
/// no kind (Decision 9 / X8). A `kind` this build has no name for reads as an empty string rather
/// than an error — a newer network may have assigned it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwDerivedArtifact {
    /// 128-hex. The transformer this row is keyed by — a content name, which the chain never
    /// resolves (SA-5 binds the submitter and the consumer, not consensus).
    pub transformer_id: String,
    /// 128-hex `derived_id_v1` — total over every field of the object, including the ones the
    /// state row does not keep, so it is the id a signature was made over.
    pub derived_id: String,
    /// 128-hex.
    pub grammar_id: String,
    pub kind: u32,
    /// The kind table's name for `kind`, or empty when this build has none.
    pub kind_name: String,
    /// 128-hex `H(grammar_id ‖ canonical DSL bytes)`.
    pub dsl_hash: String,
    /// 128-hex `H(artifact bytes)`.
    pub artifact_hash: String,
    pub artifact_bytes: u64,
    /// The DAA score of the block whose transition accepted this derivation.
    pub accepted_daa: u64,
}

impl Serializer for RpcPalwDerivedArtifact {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transformer_id, writer)?;
        store!(String, &self.derived_id, writer)?;
        store!(String, &self.grammar_id, writer)?;
        store!(u32, &self.kind, writer)?;
        store!(String, &self.kind_name, writer)?;
        store!(String, &self.dsl_hash, writer)?;
        store!(String, &self.artifact_hash, writer)?;
        store!(u64, &self.artifact_bytes, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwDerivedArtifact {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transformer_id = load!(String, reader)?;
        let derived_id = load!(String, reader)?;
        let grammar_id = load!(String, reader)?;
        let kind = load!(u32, reader)?;
        let kind_name = load!(String, reader)?;
        let dsl_hash = load!(String, reader)?;
        let artifact_hash = load!(String, reader)?;
        let artifact_bytes = load!(u64, reader)?;
        let accepted_daa = load!(u64, reader)?;
        Ok(Self { transformer_id, derived_id, grammar_id, kind, kind_name, dsl_hash, artifact_hash, artifact_bytes, accepted_daa })
    }
}

/// **ADR-0078 Decision 5: everything the chain has about one claim's derivations.**
///
/// A verifier needs the rows AND the claim they hang off, because a row alone proves nothing: the
/// object's `output_root` had to equal the claim's to be accepted, and the executor's key had to
/// be the claim's bond key. Both are here so the check is one call.
///
/// The three recomputations X6 names are then the consumer's, over bytes the chain does not have:
///
/// ```text
/// output_root   = output_commitment_v2(job_context_hash, output_token_ids, family_rendered_hash)
/// dsl_hash      = H(grammar_id ‖ grammar.canonicalize(answer))
/// artifact_hash = H(transformer.run(canonical DSL))
/// ```
///
/// `output_token_ids` are NOT returned by this call and are not on the chain in any form: the
/// consumer holds them from the gateway's response beside `job_context_hash` and `family`. What
/// this call returns is the chain's side of the comparison.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwDerivedArtifactsResponse {
    /// False when this network is not `ConsensusV2`, or the chain does not hold that claim. An
    /// honest answer rather than an error: a claim on another chain is a claim this node cannot
    /// speak for, and a verifier should be able to find that out by asking.
    pub found: bool,
    /// 128-hex, echoed so a response can be filed without its request.
    pub claim_id: String,
    /// 128-hex — the claim's committed `output_root`, which every accepted row's own
    /// `output_root` equals (Decision 4's cross-check) and which X6 recomputes.
    pub output_root: String,
    /// Hex of the executor bond's registered ML-DSA-87 public key — whose name is on this
    /// provenance. Empty when the bond has since retired: the claim outlives the bond record.
    pub executor_pubkey: String,
    /// The executor's bond outpoint, `txid_hex:index`.
    pub executor_bond: String,
    /// 128-hex class id the claim was executed under.
    pub class_id: String,
    /// The claim's phase, named: `provisional`, `panel_bound`, `receipt_licensed`, `final`, or
    /// `voided`. Decision 4: "a derivation of a claim that later voids is a derivation of a voided
    /// claim, and says so when read" — this field is that sentence.
    pub claim_phase: String,
    /// For `voided`, the reason; empty otherwise.
    pub claim_void_reason: String,
    /// The block that carried the claim's commitment, and its DAA score.
    pub claim_accepted_block: String,
    pub claim_accepted_daa: u64,
    /// The derivations, in transformer-id order (the table's own order). Empty for a claim with
    /// none — which is the common case and not a failure.
    pub artifacts: Vec<RpcPalwDerivedArtifact>,
}

impl Serializer for GetPalwDerivedArtifactsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.output_root, writer)?;
        store!(String, &self.executor_pubkey, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.claim_phase, writer)?;
        store!(String, &self.claim_void_reason, writer)?;
        store!(String, &self.claim_accepted_block, writer)?;
        store!(u64, &self.claim_accepted_daa, writer)?;
        serialize!(Vec<RpcPalwDerivedArtifact>, &self.artifacts, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwDerivedArtifactsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let claim_id = load!(String, reader)?;
        let output_root = load!(String, reader)?;
        let executor_pubkey = load!(String, reader)?;
        let executor_bond = load!(String, reader)?;
        let class_id = load!(String, reader)?;
        let claim_phase = load!(String, reader)?;
        let claim_void_reason = load!(String, reader)?;
        let claim_accepted_block = load!(String, reader)?;
        let claim_accepted_daa = load!(u64, reader)?;
        let artifacts = deserialize!(Vec<RpcPalwDerivedArtifact>, reader)?;
        Ok(Self {
            found,
            claim_id,
            output_root,
            executor_pubkey,
            executor_bond,
            class_id,
            claim_phase,
            claim_void_reason,
            claim_accepted_block,
            claim_accepted_daa,
            artifacts,
        })
    }
}

/// ADR-0077 R0: which free-prompt claim to read. 128-hex `fp_claim_id`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptClaimRequest {
    pub claim_id: String,
}

impl Serializer for GetPalwFreePromptClaimRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptClaimRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let claim_id = load!(String, reader)?;
        Ok(Self { claim_id })
    }
}

/// **The claim a derivation names, as the chain holds it** — ADR-0077 R0's "one inference, one
/// claim", read back.
///
/// The committed roots are here because they are what a verifier compares against and what a
/// disputer opens against: `output_root` for ADR-0078 X6, `trace_root` and `execution_root` for
/// the court's own bindings. The ids the answer consists of are NOT here and are not on the chain
/// (see [`GetPalwDerivedArtifactsResponse`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptClaimResponse {
    /// False off `ConsensusV2` and for a claim this chain does not hold.
    pub found: bool,
    pub claim_id: String,
    /// True when the claim is a free-prompt claim (ADR-0044); false for a block's own attempt
    /// claim, which has no derivable answer.
    pub is_free_prompt: bool,
    pub class_id: String,
    /// Hex of the executor bond's registered ML-DSA-87 public key; empty once the bond retires.
    pub executor_pubkey: String,
    /// `txid_hex:index`.
    pub executor_bond: String,
    /// 128-hex. What ADR-0078 X6 recomputes from (ids, job context hash, family).
    pub output_root: String,
    /// 128-hex. The committed step-trace root the court adjudicates against.
    pub trace_root: String,
    /// 128-hex. The committed execution root a refutation's binding must equal.
    pub execution_root: String,
    /// The capture's leaf count this claim was priced from (ADR-0074 Decision 5).
    pub work_leaves: u64,
    /// The work identity a free-prompt claim holds while it lives; empty on an attempt claim.
    pub work_id: String,
    /// Certified quanta, and how many of them this chain has already spent into receipt blocks.
    pub quanta: u32,
    pub quanta_spent: u32,
    /// `provisional` / `panel_bound` / `receipt_licensed` / `final` / `voided`.
    pub phase: String,
    /// For `voided`, the reason; empty otherwise.
    pub void_reason: String,
    /// The DAA the phase was entered at (bound / licensed / final / voided); 0 while provisional.
    pub phase_daa: u64,
    pub accepted_block: String,
    pub accepted_daa: u64,
    /// The DAA through which the producer owes openings and chunks — and, for a claim whose DSL
    /// was elected into the ADR-0078 Decision 6 obligation, the window in which it is served.
    pub trace_retention_daa: u64,
    /// How many derivations the chain holds for this claim (at most
    /// `PALW_DERIVED_MAX_PER_CLAIM`); the rows themselves are `GetPalwDerivedArtifacts`.
    pub derived_count: u32,
}

impl Serializer for GetPalwFreePromptClaimResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(bool, &self.is_free_prompt, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.executor_pubkey, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.output_root, writer)?;
        store!(String, &self.trace_root, writer)?;
        store!(String, &self.execution_root, writer)?;
        store!(u64, &self.work_leaves, writer)?;
        store!(String, &self.work_id, writer)?;
        store!(u32, &self.quanta, writer)?;
        store!(u32, &self.quanta_spent, writer)?;
        store!(String, &self.phase, writer)?;
        store!(String, &self.void_reason, writer)?;
        store!(u64, &self.phase_daa, writer)?;
        store!(String, &self.accepted_block, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        store!(u64, &self.trace_retention_daa, writer)?;
        store!(u32, &self.derived_count, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptClaimResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let claim_id = load!(String, reader)?;
        let is_free_prompt = load!(bool, reader)?;
        let class_id = load!(String, reader)?;
        let executor_pubkey = load!(String, reader)?;
        let executor_bond = load!(String, reader)?;
        let output_root = load!(String, reader)?;
        let trace_root = load!(String, reader)?;
        let execution_root = load!(String, reader)?;
        let work_leaves = load!(u64, reader)?;
        let work_id = load!(String, reader)?;
        let quanta = load!(u32, reader)?;
        let quanta_spent = load!(u32, reader)?;
        let phase = load!(String, reader)?;
        let void_reason = load!(String, reader)?;
        let phase_daa = load!(u64, reader)?;
        let accepted_block = load!(String, reader)?;
        let accepted_daa = load!(u64, reader)?;
        let trace_retention_daa = load!(u64, reader)?;
        let derived_count = load!(u32, reader)?;
        Ok(Self {
            found,
            claim_id,
            is_free_prompt,
            class_id,
            executor_pubkey,
            executor_bond,
            output_root,
            trace_root,
            execution_root,
            work_leaves,
            work_id,
            quanta,
            quanta_spent,
            phase,
            void_reason,
            phase_daa,
            accepted_block,
            accepted_daa,
            trace_retention_daa,
            derived_count,
        })
    }
}
