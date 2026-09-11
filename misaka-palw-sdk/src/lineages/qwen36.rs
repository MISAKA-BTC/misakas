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
    /// **ADR-0102: the operand-inventory root under each graph this holding was asked about**,
    /// keyed by the graph's id. A graph-v6 row registers that root, and deriving it copies every
    /// row of the artifact — so it is derived once per graph and then read, like the computed root
    /// is once per holding. Refusals are kept too: an artifact the graph cannot serve stays one.
    inventory_roots: std::sync::Mutex<std::collections::BTreeMap<Hash64, Result<Hash64, String>>>,
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
    PalwLoadedArtifactV1::from_parts(
        QWEN36_LINEAGE_ID,
        path,
        summary,
        Arc::new(Qwen36HoldingV1 { computed_root, artifact, inventory_roots: Default::default() }),
    )
}

/// The `(computed_root, mapping)` inside a holding of this lineage, if it is one.
pub fn parts_of(holding: &PalwLoadedArtifactV1) -> Option<(Hash64, Arc<Qwen36ArtifactV1>)> {
    if holding.lineage_id != QWEN36_LINEAGE_ID {
        return None;
    }
    holding.payload().downcast::<Qwen36HoldingV1>().ok().map(|h| (h.computed_root, h.artifact.clone()))
}

/// **The root a registration of this holding under `profile` pins** (ADR-0102): the
/// operand-inventory root where the graph registers one
/// (`misaka_palw_base0::inventory::qwen36_registers_inventory_root_v1`), derived once per graph and
/// memoized on the holding; the computed root otherwise. `None` for a holding of another lineage.
pub fn registered_root_of(
    holding: &PalwLoadedArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Option<Result<Hash64, String>> {
    if holding.lineage_id != QWEN36_LINEAGE_ID {
        return None;
    }
    let held = holding.payload().downcast::<Qwen36HoldingV1>().ok()?;
    if !misaka_palw_base0::inventory::qwen36_registers_inventory_root_v1(profile) {
        return Some(Ok(held.computed_root));
    }
    let key = profile.shape_profile_id();
    if let Some(known) = held.inventory_roots.lock().expect("the memo is never poisoned").get(&key) {
        return Some(known.clone());
    }
    // Derived outside the lock: a pass over the whole artifact must not hold up a reader of
    // another graph's root. Two racing derivations of one graph agree, so the second write is moot.
    // Streamed (ADR-0106): the pass hashes each row where it reads it, so a node deriving the root
    // of a 33 GiB holding holds one read block of it, not a copy.
    let derived = misaka_palw_base0::inventory::qwen36_inventory_summary_v1(&held.artifact, profile)
        .map(|summary| summary.root)
        .map_err(|e| format!("the artifact has no inventory under this graph: {e:?}"));
    held.inventory_roots.lock().expect("the memo is never poisoned").insert(key, derived.clone());
    Some(derived)
}

/// The held mapping whose COMPUTED root is `root`, if this node loaded one — the chain-registered
/// arm's lookup, the exact analogue of the dense lineage's by-digest one. The root was derived
/// from the mapping's own bytes at load, so a match here IS possession of the registered weights.
pub(crate) fn qwen36_artifact_by_root(holdings: &[PalwLoadedArtifactV1], root: Hash64) -> Option<Arc<Qwen36ArtifactV1>> {
    holdings.iter().filter_map(parts_of).find(|(computed, _)| *computed == root).map(|(_, artifact)| artifact)
}

/// **The held mapping a registration of `profile` at `root` names** (ADR-0102): by the computed
/// root first — every row before graph-v6, and what the chain registered for them — then, for a
/// graph that registers its inventory root, by that root under the SAME graph. The dense lineage's
/// `dense_artifact_by_registered_root`, for this container.
pub(crate) fn qwen36_artifact_by_registered_root(
    holdings: &[PalwLoadedArtifactV1],
    root: Hash64,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Option<Arc<Qwen36ArtifactV1>> {
    if let Some(artifact) = qwen36_artifact_by_root(holdings, root) {
        return Some(artifact);
    }
    if !misaka_palw_base0::inventory::qwen36_registers_inventory_root_v1(profile) {
        return None;
    }
    holdings
        .iter()
        .find(|h| registered_root_of(h, profile).is_some_and(|r| r.is_ok_and(|r| r == root)))
        .and_then(parts_of)
        .map(|(_, artifact)| artifact)
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
        let (_, mapping) =
            parts_of(artifact).ok_or_else(|| format!("the artifact offered for {} is not a Qwen3.6 mapping", entry.model_id))?;
        table.shape_matches(&mapping.shape)?;
        // ADR-0102: the computed root for the rows the chain already registered that way, the
        // operand-inventory root for a graph-v6 row — one rule, `registered_root_of`.
        registered_root_of(artifact, &entry.profile)
            .expect("a holding of this lineage")
            .map_err(|e| format!("{}: {e}", entry.model_id))
    }

    fn resolve(
        &self,
        court: &PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        network_id: &[u8],
    ) -> Option<Result<Box<dyn PalwExecutionBackendV1>, String>> {
        let entry = qwen36_canonical_classes_v1().into_iter().find(|c| c.class_id() == Some(class_id))?;
        let profile = entry.profile().ok()?;
        if let Some(artifact) = qwen36_artifact_by_registered_root(holdings, artifact_root, &profile) {
            // The ladder the RULESET froze — see the dense lineage's resolve for the whole note.
            return Some(Ok(Box::new(
                misaka_palw_base0::qwen36_backend::Qwen36Backend::new(
                    artifact,
                    entry.model_id,
                    entry.canonical_job,
                    class_id,
                    network_id.to_vec(),
                )
                .with_step_ladder_cap(court.max_step_leaf_count())
                .with_prompt_ids_form(prompt_ids_form),
            )));
        }
        Some(Err(format!(
            "the chain names the {} class and this node holds no artifact whose computed root is {artifact_root} \
             (pass the converted .palwq36 with --palw-class-artifact)",
            entry.model_id
        )))
    }
}
