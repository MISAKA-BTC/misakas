//! The derivation itself (ADR-0078 §2): answer → canonical DSL → artifact → the object the chain
//! accepts, and the consumer's verification of the same chain of pure functions (Decision 5).

use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
use crate::registry::{grammar_by_id, transformer_by_id};
use crate::{Artifact, DeriveError, Grammar, Transformer};
use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, derived_id_v1};
use kaspa_hashes::Hash64;

/// What binds a derivation to a claim — the chain-side facts the executor holds when the
/// inference finishes (ADR-0077 Decision 4's handoff).
#[derive(Clone, Debug)]
pub struct ClaimBinding {
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub output_root: Hash64,
    pub executor_pubkey: Vec<u8>,
}

/// One derivation: the canonical DSL, the artifact, and the unsigned object that names both.
#[derive(Clone, Debug)]
pub struct Derivation {
    pub grammar_id: Hash64,
    pub transformer_id: Hash64,
    pub kind: u16,
    pub canonical_dsl: Vec<u8>,
    pub dsl_hash: Hash64,
    pub artifact: Artifact,
    pub artifact_hash: Hash64,
    pub object: PalwDerivedArtifactV1,
}

impl Derivation {
    pub fn derived_id(&self) -> Hash64 {
        derived_id_v1(&self.object)
    }
}

/// Derive under an explicit grammar and transformer. `Err(Grammar)` is X4: the answer did not
/// parse, no object exists, and the claim is untouched.
pub fn derive_with(
    grammar: &dyn Grammar,
    transformer: &dyn Transformer,
    binding: &ClaimBinding,
    answer: &[u8],
) -> Result<Derivation, DeriveError> {
    let manifest = transformer.manifest();
    if manifest.grammar != grammar.name() {
        return Err(DeriveError::Transformer(format!(
            "transformer {} consumes grammar {}, not {}",
            manifest.name,
            manifest.grammar,
            grammar.name()
        )));
    }
    let canonical_dsl = grammar.canonicalize(answer)?;
    let grammar_id = grammar_id_v1(grammar.name());
    let dsl_hash = dsl_hash_v1(&grammar_id, &canonical_dsl);
    let artifact = transformer.run(&canonical_dsl)?;
    if artifact.bytes.is_empty() {
        return Err(DeriveError::Transformer("the transformer produced no bytes".into()));
    }
    let artifact_hash = artifact_hash_v1(&artifact.bytes);
    let transformer_id = transformer_id(&manifest);
    let object = PalwDerivedArtifactV1 {
        version: PALW_DERIVED_V1_VERSION,
        network_domain: binding.network_domain,
        claim_id: binding.claim_id,
        output_root: binding.output_root,
        grammar_id,
        transformer_id,
        kind: manifest.kind,
        dsl_hash,
        artifact_hash,
        artifact_bytes: artifact.bytes.len() as u64,
        executor_pubkey: binding.executor_pubkey.clone(),
    };
    Ok(Derivation { grammar_id, transformer_id, kind: manifest.kind, canonical_dsl, dsl_hash, artifact, artifact_hash, object })
}

/// Derive under the registry's transformer of the given name.
pub fn derive_named(transformer_name: &str, binding: &ClaimBinding, answer: &[u8]) -> Result<Derivation, DeriveError> {
    let transformer = crate::registry::transformer_by_name(transformer_name)
        .ok_or_else(|| DeriveError::UnknownTransformer(transformer_name.to_string()))?;
    let grammar_name = transformer.manifest().grammar;
    let grammar = crate::registry::grammar_by_name(grammar_name).ok_or_else(|| DeriveError::UnknownGrammar(grammar_name.to_string()))?;
    derive_with(grammar, transformer, binding, answer)
}

/// What a consumer checks (Decision 5, X6): from the answer bytes and the object alone, recompute
/// `dsl_hash` and `artifact_hash` and `artifact_bytes`, and demand equality. `output_root` is
/// checked by the caller against the claim (`verify_output_root`), because it needs the job's
/// context hash and the family's rendered-hash rule, which are not this crate's to know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    pub dsl_hash_matches: bool,
    pub artifact_hash_matches: bool,
    pub artifact_bytes_matches: bool,
    pub recomputed_dsl_hash: Hash64,
    pub recomputed_artifact_hash: Hash64,
    pub recomputed_artifact_bytes: u64,
}

impl Verification {
    pub fn all_match(&self) -> bool {
        self.dsl_hash_matches && self.artifact_hash_matches && self.artifact_bytes_matches
    }
}

/// Re-run the derivation named by the object over `answer` and compare. `Err` means the
/// derivation could not even be re-run (unknown ids, a grammar refusal) — which is itself a
/// demonstrable mismatch: the object names a computation the answer does not admit.
pub fn verify(object: &PalwDerivedArtifactV1, answer: &[u8]) -> Result<Verification, DeriveError> {
    let grammar = grammar_by_id(&object.grammar_id).ok_or_else(|| DeriveError::UnknownGrammar(object.grammar_id.to_string()))?;
    let transformer =
        transformer_by_id(&object.transformer_id).ok_or_else(|| DeriveError::UnknownTransformer(object.transformer_id.to_string()))?;
    let canonical_dsl = grammar.canonicalize(answer)?;
    let recomputed_dsl_hash = dsl_hash_v1(&object.grammar_id, &canonical_dsl);
    let artifact = transformer.run(&canonical_dsl)?;
    let recomputed_artifact_hash = artifact_hash_v1(&artifact.bytes);
    Ok(Verification {
        dsl_hash_matches: recomputed_dsl_hash == object.dsl_hash,
        artifact_hash_matches: recomputed_artifact_hash == object.artifact_hash,
        artifact_bytes_matches: artifact.bytes.len() as u64 == object.artifact_bytes,
        recomputed_dsl_hash,
        recomputed_artifact_hash,
        recomputed_artifact_bytes: artifact.bytes.len() as u64,
    })
}

/// Verify a derivation against artifact BYTES the consumer holds (they were handed the GLB, not
/// only the JSON): the artifact's hash and size against the object.
pub fn verify_artifact_bytes(object: &PalwDerivedArtifactV1, artifact: &[u8]) -> bool {
    artifact_hash_v1(artifact) == object.artifact_hash && artifact.len() as u64 == object.artifact_bytes
}
