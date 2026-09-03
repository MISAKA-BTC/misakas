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
    // **`a16_court_capable_v1`, the ONE spelling of "is this row court-capable"** (audit D H-4).
    //
    // This carried the v2-map equality by hand — a fourth copy of a predicate stream M had already
    // collapsed into one function, and the one copy that disagreed. `CanonicalClassV1::artifact_root`
    // registers the INVENTORY root for any court-capable row, and a graph-v5 dense row is
    // court-capable through the TILED v3 map, not through the v2 one. Under the equality the
    // inventory arm was skipped for exactly those rows: the digest arm missed (the registered root
    // is an inventory root), the inventory arm never ran, and the node reported "holds no artifact
    // whose digest is the registered root" forever, for an artifact it was holding — no producer,
    // seat or court replay of a v5 class could be built through the SDK at all.
    if !misaka_palw_base0::qwen25_a16_backend::a16_court_capable_v1(profile) {
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
            // **The ladder the RULESET froze, not the module constant** (ADR-0077 Decision 12).
            // The court is right here and the backend has asked for it since W1; until this line
            // the answer was always the default, which made the decode budget a property of the
            // build rather than of the network. Every shipped preset freezes
            // `max_step_leaf_count` at `PALW_STEP_MAX_LEAVES`, so this changes nothing until a
            // preset says otherwise — which is the point.
            return Some(Ok(Box::new(
                misaka_palw_base0::backend::Base0Backend::new(resolved).with_step_ladder_cap(court.max_step_leaf_count()),
            )));
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
                // **The compile IS the resolve's answer** (ADR-0082 audit E, H-1). `::new` now
                // compiles the class's declaration into the program it executes — one authority,
                // the same one `from_registered_profile` uses — so a graph this build cannot serve
                // is a named refusal here instead of a backend that quietly runs a different
                // program from the one the class declares. It is also what lets a dense row move
                // to ADR-0082's fused attention site at all: the plan-less route is the compiled
                // twenty-seven-row v2 program and refuses a v5 declaration by name.
                return Some(
                    misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::new(
                        artifact,
                        network_id.to_vec(),
                        entry.profile.clone(),
                        entry.canonical_job,
                    )
                    .map(|backend| -> Box<dyn PalwExecutionBackendV1> {
                        Box::new(backend.with_step_ladder_cap(court.max_step_leaf_count()))
                    }),
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_artifact_row_profile_v5, qwen25_a16_profile_v2};
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;

    fn geometry() -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 16,
            ffn_dim: 12,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        }
    }

    fn holding() -> (PalwLoadedArtifactV1, std::sync::Arc<Base0ArtifactV1>) {
        let g = geometry();
        let shape = Base0ShapeV1 {
            n_layers: g.layer_count as usize,
            n_heads: g.attn_heads as usize,
            n_kv_heads: g.attn_kv_heads as usize,
            d_head: g.attn_head_dim as usize,
            d_ff: g.ffn_dim as usize,
            vocab: g.vocab_size as usize,
            max_position: g.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = std::sync::Arc::new(
            Base0ArtifactV1::derive_deterministic(shape, 0xF022)
                .expect("a valid shape")
                .with_a16_params(derived_a16_store(&shape))
                .expect("sorted and unique"),
        );
        let loaded = PalwLoadedArtifactV1::from_parts(DENSE_LINEAGE_ID, None, "test fixture".to_string(), artifact.clone());
        (loaded, artifact)
    }

    /// **Audit D H-4: a chain-registered graph-v5 dense row resolves to the artifact this node
    /// holds.**
    ///
    /// `CanonicalClassV1::artifact_root` registers the INVENTORY root for any COURT-CAPABLE row,
    /// and `a16_court_capable_v1` is the one spelling of that predicate — true for the tiled v3
    /// map as well as for the integer-kv v2 one. This function carried a fourth copy of the
    /// predicate by hand as `state_chunk_map_id == integer_kv_state_chunk_map_id_v2()`, so for a
    /// v5 row the digest arm missed (the registered root is an inventory root), the inventory arm
    /// was skipped, and the node reported "holds no artifact whose digest is the registered root"
    /// forever, for an artifact it was holding — no producer, seat or court replay of a v5 class
    /// could be built through the SDK.
    #[test]
    fn a_chain_registered_graph_v5_row_resolves_to_its_inventory_root() {
        let (loaded, artifact) = holding();
        let holdings = [loaded];

        let v5 = qwen25_a16_artifact_row_profile_v5(geometry()).expect("the v5 projection is a valid profile");
        assert!(
            misaka_palw_base0::qwen25_a16_backend::a16_court_capable_v1(&v5),
            "the premise: a v5 row is court-capable through the tiled map"
        );
        assert_ne!(
            v5.state_chunk_map_id,
            kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
            "and it is NOT the v2 map — the equality this function used to carry"
        );
        let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &v5).expect("the inventory roots").root();
        assert_ne!(root, artifact.artifact_digest(), "an inventory root is not the digest, so the digest arm cannot answer");
        assert!(
            dense_artifact_by_registered_root(&holdings, root, &v5).is_some(),
            "a node holding the artifact must resolve the row the chain registered"
        );

        // The v2 row is untouched: it is court-capable too, and its inventory root still resolves.
        let v2 = qwen25_a16_profile_v2(geometry()).expect("the v2 projection is a valid profile");
        let v2_root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &v2).expect("the inventory roots").root();
        assert!(dense_artifact_by_registered_root(&holdings, v2_root, &v2).is_some(), "the v2 row still resolves");
        // And the digest form still resolves, under either profile.
        assert!(dense_artifact_by_registered_root(&holdings, artifact.artifact_digest(), &v5).is_some());
    }
}
