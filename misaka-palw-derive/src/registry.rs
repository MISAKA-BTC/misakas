//! The registered grammars and transformers (ADR-0078 Decisions 2 and 3): the ones this build
//! ships, addressable by name and by id. A name that is not here is not a grammar or a transformer
//! under this ADR; an id that is not here is one another build named.

use crate::ids::{grammar_id_v1, transformer_id};
use crate::{Grammar, Transformer};
use kaspa_hashes::Hash64;
use std::sync::OnceLock;

struct Registry {
    grammars: Vec<Box<dyn Grammar>>,
    transformers: Vec<Box<dyn Transformer>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry { grammars: crate::kinds::grammars(), transformers: crate::kinds::transformers() })
}

pub fn grammar_by_name(name: &str) -> Option<&'static dyn Grammar> {
    registry().grammars.iter().find(|g| g.name() == name).map(|g| g.as_ref())
}

pub fn grammar_by_id(id: &Hash64) -> Option<&'static dyn Grammar> {
    registry().grammars.iter().find(|g| grammar_id_v1(g.name()) == *id).map(|g| g.as_ref())
}

pub fn transformer_by_name(name: &str) -> Option<&'static dyn Transformer> {
    registry().transformers.iter().find(|t| t.manifest().name == name).map(|t| t.as_ref())
}

pub fn transformer_by_id(id: &Hash64) -> Option<&'static dyn Transformer> {
    registry().transformers.iter().find(|t| transformer_id(&t.manifest()) == *id).map(|t| t.as_ref())
}

/// Every registered grammar name.
pub fn grammar_names() -> Vec<&'static str> {
    registry().grammars.iter().map(|g| g.name()).collect()
}

// -------------------------------------------------------------------------------------------
// SA-5 — a manifest a consumer can fetch, or no derivation
// -------------------------------------------------------------------------------------------

/// **ADR-0078 SA-5**: whether a transformer manifest is published in THIS tree at that id.
/// Decision 5's promise is that anyone holding the answer can check the whole derivation; they
/// cannot if the manifest behind `transformer_id` is a document nobody has, so a derivation
/// naming one is refused at the source ([`crate::derive::derive_with`]) rather than made and
/// left unverifiable.
pub fn manifest_is_published(id: &Hash64) -> bool {
    transformer_by_id(id).is_some()
}

/// The published manifest of one transformer, by name or by id (hex, any case) — what a consumer
/// fetches to check a `transformer_id` themselves.
pub fn published_manifest(spec: &str) -> Option<crate::TransformerManifest> {
    if let Some(t) = transformer_by_name(spec) {
        return Some(t.manifest());
    }
    let spec = spec.strip_prefix("0x").unwrap_or(spec);
    if spec.len() != 128 {
        return None;
    }
    let mut bytes = [0u8; 64];
    faster_hex::hex_decode(spec.to_ascii_lowercase().as_bytes(), &mut bytes).ok()?;
    transformer_by_id(&Hash64::from_bytes(bytes)).map(|t| t.manifest())
}

/// The published manifest as a document: every field of the preimage, the preimage itself, and
/// the id it hashes to.
///
/// The `manifest_bytes` field is the point of the document rather than a curiosity. A manifest
/// printed as prose is a description of a preimage; a consumer who has to reconstruct the
/// preimage from a description is trusting the description. With the bytes in hand they compute
/// `transformer_id_v1(manifest_bytes)` with their own hasher and compare — and they can check
/// the bytes against the fields, both ways.
pub fn published_manifest_document(m: &crate::TransformerManifest) -> serde_json::Value {
    let bytes = crate::ids::transformer_manifest_bytes(m);
    let limits = m.named_input_limits();
    let bounds = serde_json::json!({
        "max_dsl_bytes": m.max_dsl_bytes,
        "max_artifact_bytes": m.max_artifact_bytes,
        "max_steps": m.max_steps,
        "step_unit": m.step_unit(),
        "max_named_inputs": limits.max_inputs,
        "max_named_input_bytes": limits.max_bytes,
    });
    serde_json::json!({
        "schema": "misaka.palw.transformer-manifest.v1",
        "name": m.name,
        "kind": m.kind,
        "kind_name": kaspa_consensus_core::palw_derived_v1::kind::name(m.kind),
        "grammar": m.grammar,
        "grammar_id": faster_hex::hex_string(grammar_id_v1(m.grammar).as_byte_slice()),
        "discipline": m.discipline.as_str(),
        "writer": m.writer,
        "source_tree_sha256": m.source_tree_sha256,
        "bounds": bounds,
        "manifest_bytes": faster_hex::hex_string(&bytes),
        "transformer_id": faster_hex::hex_string(transformer_id(m).as_byte_slice()),
    })
}

/// Every registered transformer name, with its kind and grammar.
pub fn transformer_names() -> Vec<(&'static str, u16, &'static str)> {
    registry()
        .transformers
        .iter()
        .map(|t| {
            let m = t.manifest();
            (m.name, m.kind, m.grammar)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_are_unique_and_every_transformer_names_a_registered_grammar() {
        let mut gnames = std::collections::BTreeSet::new();
        for g in grammar_names() {
            assert!(gnames.insert(g), "grammar {g} registered twice");
            assert!(grammar_by_id(&grammar_id_v1(g)).is_some());
        }
        let mut tnames = std::collections::BTreeSet::new();
        let mut tids = std::collections::BTreeSet::new();
        for (name, kind, grammar) in transformer_names() {
            assert!(tnames.insert(name), "transformer {name} registered twice");
            assert!(gnames.contains(grammar), "transformer {name} names unregistered grammar {grammar}");
            assert_ne!(kind, 0);
            let t = transformer_by_name(name).unwrap();
            let id = transformer_id(&t.manifest());
            assert!(tids.insert(id), "transformer {name}'s id collides");
            assert!(transformer_by_id(&id).is_some());
            assert_eq!(t.manifest().source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX, "{name} names this build");
        }
    }

    /// **SA-2: every registered transformer declares three usable ceilings and a unit.**
    /// `derive_with` refuses a manifest with a zero in one of them, so this test is the statement
    /// that no shipped transformer is in that state.
    #[test]
    fn every_registered_transformer_declares_bounds_and_they_are_sane() {
        for (name, _, _) in transformer_names() {
            let m = transformer_by_name(name).unwrap().manifest();
            assert!(crate::check_declared_bounds(&m).is_ok(), "{name} ships a zero ceiling");
            assert!(!m.step_unit().is_empty(), "{name}: a number without a unit is not a bound");
            assert!(
                m.max_dsl_bytes <= kaspa_consensus_core::palw_derived_v1::PALW_FP_DSL_V1_MAX_BYTES as u64,
                "{name} accepts a DSL larger than the DA election could ever serve (Decision 6)"
            );
            // SA-3's fail-closed default: a transformer that takes no second input accepts no
            // upload, and the two numbers agree about that rather than one of them being set.
            let limits = m.named_input_limits();
            assert_eq!(limits.max_inputs == 0, limits.max_bytes == 0, "{name}: the two upload limits disagree");
        }
    }

    /// **The declared ceilings are the kinds' OWN ceilings, not numbers written twice.** Each kind
    /// states its limits as constants of its module and enforces them there; the manifest quotes
    /// those constants, so a kind that changes one moves its `transformer_id` and cannot leave a
    /// manifest promising the old number. A kind agent who changes a ceiling and forgets the
    /// manifest is caught here.
    #[test]
    fn the_declared_ceilings_are_the_constants_those_kinds_enforce() {
        use crate::kinds::{cad, code, image, map, music, scene, simulation};
        let m = |name: &str| transformer_by_name(name).unwrap().manifest();

        let i = m("image/png/v1");
        assert_eq!(
            (i.max_dsl_bytes, i.max_artifact_bytes, i.max_steps),
            (image::MAX_DSL_BYTES, image::ARTIFACT_MAX_BYTES, image::MAX_PIXELS)
        );
        assert_eq!(i.step_unit(), "raster-pixel");

        let u = m("music/smf/v1");
        assert_eq!(
            (u.max_dsl_bytes, u.max_artifact_bytes, u.max_steps),
            (music::MAX_DSL_BYTES, music::ARTIFACT_MAX_BYTES as u64, music::NOTES_MAX_TOTAL as u64)
        );
        assert_eq!(u.step_unit(), "midi-note");

        let s = m("simulation/trace/v1");
        assert_eq!(
            (s.max_dsl_bytes, s.max_artifact_bytes, s.max_steps),
            (simulation::MAX_DSL_BYTES, simulation::MAX_ARTIFACT_BYTES as u64, simulation::MAX_STEPS as u64)
        );
        assert_eq!(s.step_unit(), "simulation-step");

        for name in ["code/evm/v1", "contract/evm/v1"] {
            let c = m(name);
            assert_eq!((c.max_dsl_bytes, c.max_artifact_bytes), (code::MAX_DSL_BYTES, code::MAX_ARTIFACT_BYTES as u64), "{name}");
            // SA-1's gas ceiling: the deploy the toolchain allows plus every test it allows.
            assert_eq!(c.max_steps, code::EVM_V1_DEPLOY_GAS_LIMIT + (code::MAX_TESTS as u64) * code::MAX_TEST_GAS_LIMIT, "{name}");
            assert_eq!(c.step_unit(), "evm-gas");
        }

        // The three kinds that landed with their own declared-bounds values: the manifest is that
        // value, field for field, so there is one declaration and not two.
        let n = m("scene/glb/v1");
        assert_eq!(
            (n.max_dsl_bytes, n.max_artifact_bytes, n.max_steps),
            (scene::BOUNDS.max_dsl_bytes, scene::BOUNDS.max_artifact_bytes, scene::BOUNDS.max_steps)
        );
        assert_eq!(n.step_unit(), scene::STEPS_UNIT);

        let d = m("cad/stl/v1");
        assert_eq!(
            (d.max_dsl_bytes, d.max_artifact_bytes, d.max_steps),
            (cad::BOUNDS.max_dsl_bytes as u64, cad::BOUNDS.max_artifact_bytes as u64, cad::BOUNDS.max_steps)
        );

        let p = m("map/mmap/v1");
        assert_eq!(
            (p.max_dsl_bytes, p.max_artifact_bytes, p.max_steps),
            (map::BOUNDS.max_dsl_bytes, map::BOUNDS.max_artifact_bytes, map::BOUNDS.max_steps)
        );
    }

    /// **SA-5, from both ends.** Every registered transformer's manifest is fetchable by name and
    /// by id, the document carries the exact preimage, and the id in it is the hash of those
    /// bytes — so a consumer can recompute the id with their own hasher. An id this build does
    /// not have is not published, and says so.
    #[test]
    fn every_registered_manifest_is_publishable_and_recomputable_and_a_stranger_id_is_not() {
        for (name, _, _) in transformer_names() {
            let m = transformer_by_name(name).unwrap().manifest();
            let id = transformer_id(&m);
            assert!(manifest_is_published(&id), "{name}");
            assert_eq!(published_manifest(name).as_ref(), Some(&m));
            let hex = faster_hex::hex_string(id.as_byte_slice());
            assert_eq!(published_manifest(&hex).as_ref(), Some(&m), "{name} is not fetchable by its id");
            assert_eq!(published_manifest(&format!("0x{}", hex.to_ascii_uppercase())).as_ref(), Some(&m));

            let doc = published_manifest_document(&m);
            let mut bytes = vec![0u8; doc["manifest_bytes"].as_str().unwrap().len() / 2];
            faster_hex::hex_decode(doc["manifest_bytes"].as_str().unwrap().as_bytes(), &mut bytes).unwrap();
            assert_eq!(bytes, crate::ids::transformer_manifest_bytes(&m));
            assert_eq!(
                doc["transformer_id"].as_str().unwrap(),
                faster_hex::hex_string(kaspa_consensus_core::palw_derived_v1::transformer_id_v1(&bytes).as_byte_slice()),
                "{name}: the document's id is not the hash of the document's own preimage"
            );
            assert_eq!(doc["bounds"]["step_unit"].as_str().unwrap(), m.step_unit());
            assert_eq!(doc["bounds"]["max_dsl_bytes"].as_u64().unwrap(), m.max_dsl_bytes);
        }
        let stranger = Hash64::from_bytes([0xAB; 64]);
        assert!(!manifest_is_published(&stranger));
        assert_eq!(published_manifest(&faster_hex::hex_string(stranger.as_byte_slice())), None);
        assert_eq!(published_manifest("scene/nothing/v9"), None);
    }
}
