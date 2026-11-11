//! Source-only Selection material geometry for the prospective plain profile.
//! It preserves the legacy FullIds + all row tiles × supplier roles envelope.
//! No branch reservation, active notice, supplier identity or verdict is created.

pub(crate) mod budget;
pub(crate) mod delivery;

use super::plain_output::PlainBatchOutputV2;
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_shared_material_intake_v1::{MATERIAL_PIPELINED_MAX_BYTES, MaterialTargetV1},
    palw_court_tir_requests_v1::source_trace_tile_target_v1,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicSourceV1},
};

/// Constant-size geometry. `roles` must eventually be derived from the native
/// liability/profile; a caller-selected count never grants funding authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct PlainBatchSelectionV2 {
    claim: Hash64,
    source: Hash64,
    positions: u32,
    vocab: u32,
    roles: u64,
    tiles: u32,
    dependencies: u64,
    full_ids_bytes: u64,
    tile_bytes: u64,
    last_tile_bytes: u64,
    payload_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct SelectionMaterialV2 {
    pub role: u64,
    pub target: MaterialTargetV1,
    pub maximum_bytes: u64,
}

#[allow(dead_code)]
impl PlainBatchSelectionV2 {
    pub(crate) fn derive_v2(
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        roles: u64,
        maximum_dependencies: u64,
    ) -> Result<Self, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, DeliveryCost};
        // Checks original job/source roots, exact D, class and supported plain
        // decoder without depending on an allocated legacy Frame.
        let full_ids_bytes = PlainBatchOutputV2::full_ids_bound_v2(original, source)?;
        let tiles = source.vocab_v1().div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32);
        let dependencies = (u64::from(tiles) + 1).checked_mul(roles).ok_or(CaseGraph)?;
        if roles == 0 || tiles == 0 || dependencies > maximum_dependencies {
            return Err(CaseGraph);
        }
        let bound = |tile| {
            source
                .unit_bound_v1(&PalwTirPublicRequestV1::TraceTile { row: 0, tile })
                .map(|bound| bound.maximum_response_bytes)
                .map_err(|_| CaseGraph)
        };
        let tile_bytes = bound(0)?;
        let last_tile_bytes = bound(tiles - 1)?;
        if [tile_bytes, last_tile_bytes].iter().any(|bytes| !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(bytes)) {
            return Err(DeliveryCost);
        }
        let payload_bytes = u64::from(tiles - 1)
            .checked_mul(tile_bytes)
            .and_then(|n| n.checked_add(last_tile_bytes))
            .and_then(|n| n.checked_add(full_ids_bytes))
            .and_then(|n| n.checked_mul(roles))
            .ok_or(DeliveryCost)?;
        Ok(Self {
            claim: fp_claim_id_v3(original),
            source: source.source_id_v1(),
            positions: source.decode_count_v1(),
            vocab: source.vocab_v1(),
            roles,
            tiles,
            dependencies,
            full_ids_bytes,
            tile_bytes,
            last_tile_bytes,
            payload_bytes,
        })
    }

    pub(crate) fn target_count_v2(&self) -> u64 {
        u64::from(self.positions) * u64::from(self.vocab)
    }

    pub(crate) fn dependency_count_v2(&self) -> u64 {
        self.dependencies
    }

    pub(crate) fn payload_bytes_per_target_v2(&self) -> u64 {
        self.payload_bytes
    }

    /// This identity binds input geometry only. It is not a complete resource
    /// plan ID: control/replay, DA fallback, retention and causal windows remain.
    pub(crate) fn geometry_id_v2(&self) -> Result<Hash64, String> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-selection-geometry/v2",
            &(
                self.claim,
                self.source,
                self.positions,
                self.vocab,
                self.roles,
                self.tiles,
                self.dependencies,
                self.full_ids_bytes,
                self.tile_bytes,
                self.last_tile_bytes,
                self.payload_bytes,
            ),
        )
    }

    /// Canonical legacy dependency order: FullIds × roles, then each tile ×
    /// roles. Beat only selects the eventual court comparison; before FullIds
    /// authentication the committed lane cannot reduce this potential set.
    pub(crate) fn material_at_v2(
        &self,
        source: &PalwTirPublicSourceV1,
        position: u32,
        beat: u32,
        index: u64,
    ) -> Result<SelectionMaterialV2, String> {
        if source.source_id_v1() != self.source || position >= self.positions || beat >= self.vocab || index >= self.dependencies {
            return Err("plain Selection material is outside its original source/target".into());
        }
        let group = index / self.roles;
        let (target, maximum_bytes) = if group == 0 {
            (MaterialTargetV1::FullIds, self.full_ids_bytes)
        } else {
            let tile = u32::try_from(group - 1).map_err(|_| "plain Selection tile overflows")?;
            let (target, bound) = source_trace_tile_target_v1(source, position, tile)?;
            let maximum = if tile == self.tiles - 1 { self.last_tile_bytes } else { self.tile_bytes };
            if bound.maximum_response_bytes != maximum {
                return Err("plain Selection native tile bound changed".into());
            }
            (target, maximum)
        };
        Ok(SelectionMaterialV2 { role: index % self.roles, target, maximum_bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_court_shared_tir_builder_store_v1::selection_terminal::{NativeSelectionUniverseV1, SelectionTargetV1},
        palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3, SourceGraphBudgetV3},
        palw_court_tir_requests_v1::TirSharedRequestSetV1,
    };

    #[test]
    fn bct2_plain_selection_source_geometry_matches_complete_legacy_dependencies() {
        for fixture in [
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 2),
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1(),
        ] {
            let frame = &fixture.frame;
            let original = frame.original_v1();
            let source = PalwTirPublicSourceV1::new_native_v1(
                &fixture.record,
                &fixture.layout,
                frame.header_v1(),
                original.execution_root,
                original.trace_root,
                fixture.rules.maximum_leaves,
            )
            .unwrap();
            let profile = Hash64::from_u64_word(71);
            let budget = SourceGraphBudgetV3 { max_queries: 100_000, max_roles: 8, max_vertices: 2_000_000, max_edges: 8_000_000 };
            let graph = NativeSourceMaterialGraphV3::derive_native_v3(frame, &source, profile, budget).unwrap();
            let legacy = NativeSelectionUniverseV1::derive_native_v1(frame, &source, &graph, profile).unwrap();
            let shape = PlainBatchSelectionV2::derive_v2(original, &source, graph.role_slots_v3(), u64::MAX).unwrap();
            assert_eq!(shape.target_count_v2(), legacy.len_v1());
            let set = TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
            for ordinal in [0, legacy.len_v1() / 2, legacy.len_v1() - 1] {
                let target = legacy.target_at_v1(ordinal).unwrap();
                let duty = legacy.project_native_v1(target, budget.max_vertices).unwrap();
                assert_eq!(shape.dependency_count_v2(), duty.dependencies_v1().len() as u64);
                let mut bytes = 0;
                for (i, dependency) in duty.dependencies_v1().iter().enumerate() {
                    let material = shape.material_at_v2(&source, target.position, target.beat, i as u64).unwrap();
                    assert_eq!(material.role, dependency.role);
                    match (material.target, dependency.tile) {
                        (MaterialTargetV1::FullIds, None) => {}
                        (MaterialTargetV1::Unit { ordinal }, Some(tile)) => {
                            assert_eq!(
                                set.request_at(ordinal).unwrap(),
                                PalwTirPublicRequestV1::TraceTile { row: target.position, tile: tile as u32 }
                            );
                        }
                        _ => panic!("selection omitted or substituted a dependency"),
                    }
                    bytes += material.maximum_bytes;
                }
                assert_eq!(shape.payload_bytes_per_target_v2(), bytes);
            }
            assert!(PlainBatchSelectionV2::derive_v2(original, &source, graph.role_slots_v3(), shape.dependencies - 1).is_err());
            assert!(PlainBatchSelectionV2::derive_v2(original, &source, 0, u64::MAX).is_err());
            assert!(PlainBatchSelectionV2::derive_v2(original, &source, u64::MAX, u64::MAX).is_err());
            for SelectionTargetV1 { position, beat } in [
                SelectionTargetV1 { position: source.decode_count_v1(), beat: 0 },
                SelectionTargetV1 { position: 0, beat: source.vocab_v1() },
            ] {
                assert!(shape.material_at_v2(&source, position, beat, 0).is_err());
            }
            assert!(shape.material_at_v2(&source, 0, 0, shape.dependencies).is_err());
            let mut changed = original.clone();
            changed.execution_root = Hash64::from_u64_word(91);
            assert!(PlainBatchSelectionV2::derive_v2(&changed, &source, graph.role_slots_v3(), u64::MAX).is_err());
            let another_role_count = PlainBatchSelectionV2::derive_v2(original, &source, graph.role_slots_v3() + 1, u64::MAX).unwrap();
            assert_ne!(shape.geometry_id_v2().unwrap(), another_role_count.geometry_id_v2().unwrap());
        }
    }
}
