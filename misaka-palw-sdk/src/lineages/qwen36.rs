//! **The Qwen3.6 mmap lineage: memory-mapped `.palwq36` artifacts and the hybrid/qwen3moe table.**
//!
//! The artifact is a different animal from the dense tier's — a 33 GiB class is memory-mapped,
//! never decoded whole, and the root a registration pins is COMPUTED over the mapping
//! (`Qwen36ArtifactV1::artifact_root`), one pass over the file at load — so the shape check, the
//! root derivation and the backend dispatch are all different code. The class table itself stays
//! in `misaka_palw_base0::classes::qwen36_canonical_classes_v1`, single-sourced: a new member of
//! this lineage is a geometry constant in `palw_qwen36_profile` and a row there, and this module
//! moves not at all.

use std::path::Path;
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::classes::{Qwen36CanonicalClassV1, qwen36_canonical_classes_v1};
use misaka_palw_base0::qwen36::{QWEN36_FILE_MAGIC, Qwen36ArtifactV1, open_artifact};

use crate::lineage::{PalwClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1};

/// The lineage id every Qwen3.6 entry and artifact carries.
pub const QWEN36_LINEAGE_ID: &str = "qwen36-mmap-v1";

/// The Qwen3.6 lineage. Stateless, like the dense one: the table is re-read per call.
pub struct Qwen36LineageV1;

/// What a loaded `.palwq36` holding carries: the mapping, and the root THIS NODE computed over it.
/// The root is computed at construction — once, because it costs a pass over the file — and never
/// read from a sidecar: it is this node's proof that it holds what the chain registered, and a
/// declared root would prove nothing (derive, never declare).
struct Qwen36HoldingV1 {
    computed_root: Hash64,
    artifact: Arc<Qwen36ArtifactV1>,
}

/// Wrap an already-open mapping as a holding of this lineage, computing its root. The constructor
/// node code and tests use when the artifact did not come through [`PalwModelLineageV1::load`].
pub fn holding_from_artifact(artifact: Arc<Qwen36ArtifactV1>, path: Option<std::path::PathBuf>) -> PalwLoadedArtifactV1 {
    let computed_root = artifact.artifact_root();
    let summary = match &path {
        Some(p) => format!(
            "mapped Qwen3.6 artifact {} ({} layers, {:.2} GiB, computed root {computed_root})",
            p.display(),
            artifact.shape.n_layers(),
            artifact.weight_bytes() as f64 / (1u64 << 30) as f64,
        ),
        None => format!(
            "holding a Qwen3.6 mapping ({} layers, {:.2} GiB, computed root {computed_root})",
            artifact.shape.n_layers(),
            artifact.weight_bytes() as f64 / (1u64 << 30) as f64,
        ),
    };
    PalwLoadedArtifactV1::from_parts(QWEN36_LINEAGE_ID, path, summary, Arc::new(Qwen36HoldingV1 { computed_root, artifact }))
}

/// The `(computed_root, mapping)` inside a holding of this lineage, if it is one.
pub fn parts_of(holding: &PalwLoadedArtifactV1) -> Option<(Hash64, Arc<Qwen36ArtifactV1>)> {
    if holding.lineage_id != QWEN36_LINEAGE_ID {
        return None;
    }
    holding.payload().downcast::<Qwen36HoldingV1>().ok().map(|h| (h.computed_root, h.artifact.clone()))
}

/// The held mapping whose COMPUTED root is `root`, if this node loaded one — the chain-registered
/// arm's lookup, the exact analogue of the dense lineage's by-digest one. The root was derived
/// from the mapping's own bytes at load, so a match here IS possession of the registered weights.
pub(crate) fn qwen36_artifact_by_root(holdings: &[PalwLoadedArtifactV1], root: Hash64) -> Option<Arc<Qwen36ArtifactV1>> {
    holdings.iter().filter_map(parts_of).find(|(computed, _)| *computed == root).map(|(_, artifact)| artifact)
}

fn table_entry(model_id: &str) -> Option<Qwen36CanonicalClassV1> {
    qwen36_canonical_classes_v1().into_iter().find(|c| c.model_id == model_id)
}

impl PalwModelLineageV1 for Qwen36LineageV1 {
    fn lineage_id(&self) -> &'static str {
        QWEN36_LINEAGE_ID
    }

    fn classes(&self, _court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
        qwen36_canonical_classes_v1()
            .into_iter()
            // A geometry that does not project is not a class this build can supply; the
            // conformance harness is what would make that loud, exactly as the panel's own
            // enumeration skipped rows whose profile did not build.
            .filter_map(|c| {
                let profile = c.profile().ok()?;
                Some(PalwClassEntryV1 {
                    model_id: c.model_id,
                    lineage_id: QWEN36_LINEAGE_ID,
                    profile,
                    canonical_job: c.canonical_job,
                    needs_artifact_file: true,
                })
            })
            .collect()
    }

    fn sniffs(&self, head: &[u8; 8]) -> bool {
        head == QWEN36_FILE_MAGIC
    }

    fn load(&self, path: &Path) -> Result<PalwLoadedArtifactV1, String> {
        let artifact = open_artifact(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(holding_from_artifact(Arc::new(artifact), Some(path.to_path_buf())))
    }

    /// The computed root — the exact value a registration of these weights pinned, whichever
    /// class it was registered under.
    fn registered_weight_keys(&self, artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
        parts_of(artifact).map(|(root, _)| vec![root]).unwrap_or_default()
    }

    fn pair(&self, _court: &PalwCourtParamsV2, entry: &PalwClassEntryV1, artifact: &PalwLoadedArtifactV1) -> Result<Hash64, String> {
        let table = table_entry(entry.model_id)
            .ok_or_else(|| format!("{} is not a class of the {QWEN36_LINEAGE_ID} lineage", entry.model_id))?;
        let (computed_root, artifact) =
            parts_of(artifact).ok_or_else(|| format!("the artifact offered for {} is not a Qwen3.6 mapping", entry.model_id))?;
        table.shape_matches(&artifact.shape)?;
        Ok(computed_root)
    }

    fn resolve(
        &self,
        _court: &PalwCourtParamsV2,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
        let entry = qwen36_canonical_classes_v1().into_iter().find(|c| c.class_id() == Some(class_id))?;
        if let Some((_, artifact)) = holdings.iter().filter_map(parts_of).find(|(root, _)| *root == artifact_root) {
            return Some(Ok(Box::new(misaka_palw_base0::qwen36_backend::Qwen36Backend::new(
                artifact,
                entry.model_id,
                entry.canonical_job,
                class_id,
                network_id.to_vec(),
            ))));
        }
        Some(Err(format!(
            "the chain names the {} class and this node holds no artifact whose computed root is {artifact_root} \
             (pass the converted .palwq36 with --palw-class-artifact)",
            entry.model_id
        )))
    }
}
