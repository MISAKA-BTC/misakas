//! **The dense-container lineage: the derived floor, converted dense classes, and the A16 tier.**
//!
//! One lineage, because one container: everything here rides a `.palwart` file (or derives its
//! bytes from the RC seed), decodes whole through `decode_artifact_file_v1`, and is tabled by
//! `misaka_palw_base0::classes::canonical_classes_v1` — the registry this module deliberately does
//! NOT restate. The SDK wraps that table rather than copying it, so a new dense-family member is
//! still exactly what it was: a geometry constant and a table row in `classes.rs`, visible here
//! with zero further code.
//!
//! What IS this module's own is the lineage contract: how a dense file is sniffed (it is the
//! container fallback — its decoder authenticates the format internally), which root form each
//! entry pins (an A16 entry registers the artifact's DIGEST, everything else an operand-inventory
//! root — `CanonicalClassV1::artifact_root` decides, per entry source), and how the chain's
//! `(class_id, artifact_root)` resolves to an engine (`Base0Backend` for the integer tier,
//! `Qwen25A16Backend` for the A16 tier — ported arm-for-arm from the kaspad registry this
//! replaced, error strings included).

use std::path::Path;
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, decode_artifact_file_v1};
use misaka_palw_base0::classes::{ArtifactSourceV1, CanonicalClassV1, canonical_classes_v1, resolve_class_v1};

use crate::lineage::{PalwClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1};

/// The lineage id every dense entry and artifact carries.
pub const DENSE_LINEAGE_ID: &str = "base0-dense-v1";

/// The dense lineage. Stateless: the table it serves lives in `misaka_palw_base0::classes` and is
/// re-read per call, for the same reason the panel rebuilds its registry per use — a cache would
/// be a second place the build's ledger lives.
pub struct DenseLineageV1;

/// Wrap an already-decoded dense artifact as a holding of this lineage — the constructor node
/// code and tests use when the artifact did not come through [`PalwModelLineageV1::load`] (the
/// derived floor, fixtures).
pub fn holding_from_artifact(artifact: Arc<Base0ArtifactV1>, path: Option<std::path::PathBuf>) -> PalwLoadedArtifactV1 {
    let summary = match &path {
        Some(p) => format!(
            "loaded class artifact {} ({} layers, vocab {}, eps_q {})",
            p.display(),
            artifact.shape.n_layers,
            artifact.shape.vocab,
            artifact.shape.eps_q
        ),
        None => format!(
            "holding a dense class artifact ({} layers, vocab {}, eps_q {})",
            artifact.shape.n_layers, artifact.shape.vocab, artifact.shape.eps_q
        ),
    };
    PalwLoadedArtifactV1::from_parts(DENSE_LINEAGE_ID, path, summary, artifact)
}

/// The dense artifact inside a holding of this lineage, if it is one.
pub fn artifact_of(holding: &PalwLoadedArtifactV1) -> Option<Arc<Base0ArtifactV1>> {
    if holding.lineage_id != DENSE_LINEAGE_ID {
        return None;
    }
    holding.payload().downcast::<Base0ArtifactV1>().ok()
}

/// Every dense artifact among `holdings`, cloned out of their `Arc`s — the shape
/// `resolve_class_v1` wants. The clone is per-resolve and a dense artifact is ~1.65 GiB; it is
/// the cost the kaspad registry already paid on this path (its holdings were `Arc`ed for the
/// panel's tick, the resolve still cloned), carried rather than silently "improved" because the
/// resolve path's allocation behavior is load-bearing for a producer under cadence.
/// The dense holding whose digest is `root`, as the `Arc` the backend wants — no clone of the
/// payload, because the chain arm resolves per claim and a 1.7 GiB copy per resolve is the cost
/// `dense_artifacts` documents, not one to pay twice.
pub(crate) fn dense_artifact_by_digest(
    holdings: &[PalwLoadedArtifactV1],
    root: kaspa_hashes::Hash64,
) -> Option<std::sync::Arc<Base0ArtifactV1>> {
    holdings.iter().filter_map(artifact_of).find(|a| a.artifact_digest() == root)
}

/// The dense holding a CHAIN-REGISTERED root names, under either root form a dense registration
/// can pin: the artifact's digest (the v1 A16 spelling), or — when the registered profile is the
/// court-capable one (the four-byte map, the same predicate `supports_court` answers) — the A16
/// operand-inventory root derived from THAT profile, which is what an arithmetic close's openings
/// prove against. The profile is the registration's own carriage, so the derivation cannot be a
/// second mapping: it is `a16_inventory_v1` at the class's declared graph, per candidate holding.
pub(crate) fn dense_artifact_by_registered_root(
    holdings: &[PalwLoadedArtifactV1],
    root: kaspa_hashes::Hash64,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Option<std::sync::Arc<Base0ArtifactV1>> {
    if let Some(artifact) = dense_artifact_by_digest(holdings, root) {
        return Some(artifact);
    }
    if profile.state_chunk_map_id != kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2() {
        return None;
    }
    holdings
        .iter()
        .filter_map(artifact_of)
        .find(|a| misaka_palw_base0::inventory::a16_inventory_v1(a, profile).is_ok_and(|inv| inv.root() == root))
}

fn dense_artifacts(holdings: &[PalwLoadedArtifactV1]) -> Vec<Base0ArtifactV1> {
    holdings.iter().filter_map(artifact_of).map(|a| (*a).clone()).collect()
}

fn table_entry(court: &PalwCourtParamsV2, model_id: &str) -> Option<CanonicalClassV1> {
    canonical_classes_v1(court).into_iter().find(|c| c.model_id == model_id)
}

impl PalwModelLineageV1 for DenseLineageV1 {
    fn lineage_id(&self) -> &'static str {
        DENSE_LINEAGE_ID
    }

    fn classes(&self, court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
        canonical_classes_v1(court)
            .into_iter()
            .map(|c| PalwClassEntryV1 {
                model_id: c.model_id,
                lineage_id: DENSE_LINEAGE_ID,
                needs_artifact_file: !matches!(c.source, ArtifactSourceV1::Derived(_)),
                canonical_job: c.canonical_job,
                profile: c.profile,
            })
            .collect()
    }

    /// The dense container has no sniffable claim of its own here: it is the fallback, and its
    /// decoder's own magic check is the authentication.
    fn sniffs(&self, _head: &[u8; 8]) -> bool {
        false
    }

    fn is_container_fallback(&self) -> bool {
        true
    }

    fn load(&self, path: &Path) -> Result<PalwLoadedArtifactV1, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let artifact = decode_artifact_file_v1(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(holding_from_artifact(Arc::new(artifact), Some(path.to_path_buf())))
    }

    /// The artifact's DIGEST — the root form the A16 tier registers, and therefore the key under
    /// which a dense file's weights can already sit on chain. Inventory-root registrations of the
    /// same file are caught by the registered-class-id filter instead: their root depends on the
    /// class's geometry, so there is no artifact-only key to compare.
    fn registered_weight_keys(&self, artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
        artifact_of(artifact).map(|a| vec![a.artifact_digest()]).unwrap_or_default()
    }

    fn pair(&self, court: &PalwCourtParamsV2, entry: &PalwClassEntryV1, artifact: &PalwLoadedArtifactV1) -> Result<Hash64, String> {
        let table = table_entry(court, entry.model_id)
            .ok_or_else(|| format!("{} is not a class of the {DENSE_LINEAGE_ID} lineage", entry.model_id))?;
        let artifact = artifact_of(artifact)
            .ok_or_else(|| format!("the artifact offered for {} is not a dense-container holding", entry.model_id))?;
        table.shape_matches(&artifact).map_err(|e| e.to_string())?;
        table.artifact_root(&artifact).map_err(|e| format!("the artifact has no canonical inventory: {e:?}"))
    }

    fn resolve(
        &self,
        court: &PalwCourtParamsV2,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
        // The integer tier first: the floor (derived, so a node holding nothing can always serve
        // it) and converted dense classes, both through `resolve_class_v1`, which checks id AND
        // root and refuses anything it cannot prove. A refusal here is not final — the id may be
        // the A16 tier's, whose entries the floor resolver deliberately does not know.
        if let Ok(resolved) = resolve_class_v1(court, class_id, artifact_root, &dense_artifacts(holdings)) {
            return Some(Ok(Box::new(misaka_palw_base0::backend::Base0Backend::new(resolved))));
        }
        // **The A16 dense class.** Its artifact rides the same container as the floor's, so it is
        // found in the same holdings — under the ROOT FORM the row registers: the v1 row's is the
        // artifact's digest, the court-capable row's is the A16 operand-inventory root
        // (`CanonicalClassV1::artifact_root` decides, and deciding it here a second time is the
        // two-mappings defect). The inventory derivation costs a pass over the store per candidate
        // holding; a resolve is per producer tick, not per block validation, and correctness of
        // WHICH bytes the court opens is the thing this lineage exists to keep single-sourced.
        if let Some(entry) = canonical_classes_v1(court)
            .into_iter()
            .filter(|c| matches!(c.source, ArtifactSourceV1::ConvertedA16))
            .find(|c| c.class_id() == class_id)
        {
            if let Some(artifact) =
                holdings.iter().filter_map(artifact_of).find(|a| entry.artifact_root(a).is_ok_and(|root| root == artifact_root))
            {
                return Some(Ok(Box::new(misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::new(
                    artifact,
                    network_id.to_vec(),
                    entry.profile.clone(),
                    entry.canonical_job,
                ))));
            }
            return Some(Err(format!(
                "the chain names the {} class and this node holds no artifact whose registered root form is {artifact_root} \
                 (pass the converted .palwart with --palw-class-artifact)",
                entry.model_id
            )));
        }
        None
    }
}
