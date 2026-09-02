//! The names (ADR-0078 Decisions 2–4): a grammar's id from its name, a transformer's id from its
//! manifest, the DSL hash, the artifact hash — every one a keyed BLAKE2b-512 under a domain that
//! `kaspa-consensus-core::palw_derived_v1` owns, so this crate and the chain spell each id the
//! same way by construction (the functions are re-exported from there, not restated).

use crate::TransformerManifest;
use kaspa_hashes::Hash64;

pub use kaspa_consensus_core::palw_derived_v1::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id_v1};

/// The manifest's canonical bytes — the preimage of `transformer_id`. A fixed field order with
/// length prefixes, so a manifest is one byte string and a changed field is a changed id.
///
/// The three SA-2 ceilings are in the preimage, after the kind, because a bound that could be
/// loosened without moving the id would not be a bound: an executor could publish a derivation
/// under a strict manifest and run under a lax one, and no consumer could tell. Loosening one is
/// therefore a NEW transformer, and the derivations made under the old one stay checkable against
/// the old id — the same rule Decision 8 states for a kind's version.
pub fn transformer_manifest_bytes(m: &TransformerManifest) -> Vec<u8> {
    let mut out = Vec::new();
    for field in [m.name, m.grammar, m.discipline.as_str(), m.writer, m.source_tree_sha256] {
        out.extend_from_slice(&(field.len() as u64).to_le_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    out.extend_from_slice(&m.kind.to_le_bytes());
    out.extend_from_slice(&m.max_dsl_bytes.to_le_bytes());
    out.extend_from_slice(&m.max_artifact_bytes.to_le_bytes());
    out.extend_from_slice(&m.max_steps.to_le_bytes());
    out
}

/// `transformer_id = H(manifest)` (Decision 3).
pub fn transformer_id(m: &TransformerManifest) -> Hash64 {
    transformer_id_v1(&transformer_manifest_bytes(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Discipline;

    fn manifest() -> TransformerManifest {
        TransformerManifest {
            name: "scene/glb/v1",
            kind: 1,
            grammar: "scene/v1",
            discipline: Discipline::Integer,
            writer: "gltf-binary/2.0/canonical-v1",
            source_tree_sha256: "00",
            max_dsl_bytes: 1 << 20,
            max_artifact_bytes: 1 << 21,
            max_steps: 1_000,
        }
    }

    /// **SA-2 is in the id.** The tail of the preimage is the three ceilings, so a build that
    /// loosened one could not keep the old id and hand the old manifest to a verifier.
    #[test]
    fn the_three_ceilings_are_the_tail_of_the_preimage() {
        let m = manifest();
        let mut want = Vec::new();
        want.extend_from_slice(&m.max_dsl_bytes.to_le_bytes());
        want.extend_from_slice(&m.max_artifact_bytes.to_le_bytes());
        want.extend_from_slice(&m.max_steps.to_le_bytes());
        assert!(transformer_manifest_bytes(&m).ends_with(&want), "the ceilings are not the tail of the preimage");
    }

    #[test]
    fn every_manifest_field_moves_the_transformer_id() {
        let base = transformer_id(&manifest());
        let mut ids = std::collections::BTreeSet::new();
        ids.insert(base);
        let variants = [
            TransformerManifest { name: "scene/glb/v2", ..manifest() },
            TransformerManifest { kind: 2, ..manifest() },
            TransformerManifest { grammar: "scene/v2", ..manifest() },
            TransformerManifest { discipline: Discipline::ExactRational, ..manifest() },
            TransformerManifest { writer: "other", ..manifest() },
            TransformerManifest { source_tree_sha256: "01", ..manifest() },
            TransformerManifest { max_dsl_bytes: (1 << 20) + 1, ..manifest() },
            TransformerManifest { max_artifact_bytes: (1 << 21) + 1, ..manifest() },
            TransformerManifest { max_steps: 1_001, ..manifest() },
        ];
        for v in variants {
            assert!(ids.insert(transformer_id(&v)), "a field did not move the id, or two collided: {v:?}");
        }
    }
}
