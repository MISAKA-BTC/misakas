//! **ADR-0093 as built, the capture's side: a fused site's evidence, out of a dense capture.**
//!
//! Family-independent on purpose. Both A16 families commit the same step leaves (`Base0StepTilesV1`)
//! and the same checkpoint leg (`Base0CheckpointsV1`); what a family owns is producing the dense
//! run (its engine) and recomputing a checkpoint's state (its kernels) — both handed in. Everything
//! read here is a COMMITTED value: the query row the fused site read, every K and V row its history
//! wrote, the output tile it committed, the anchor its step must carry. No arithmetic is done on
//! them; `kaspa_consensus_core::palw_attn_responder_v1` computes every claim with the court's own
//! kernels.

use std::collections::HashMap;

use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, PalwProvenOperandsV1, open_artifact_leaf_v1};
use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnRowOpeningV1};
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnAnchorEvidenceV1, PalwAttnSiteEvidenceV1, PalwAttnSiteInputsV1};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepMerkleTreeV1, PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_path_capped_v1, step_merkle_root_capped_v1,
};
use kaspa_hashes::Hash64;

fn lanes_of(leaf: &PalwStepTileLeafV1) -> Vec<i32> {
    leaf.values_le.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

/// **The evidence one capture yields about the fused site at `narrowed`.**
///
/// * `binding` / `tiles` — the CAPTURE's own commitment and its committed rows: the accused's
///   retained tiles when the capture is the accused's, so the openings prove against the claim's
///   step tree even when the execution is a forgery (the bottom must open what the claim
///   committed, not what an honest re-execution computes). Refused unless they root to the binding.
/// * `checkpoints` — the checkpoint leg, from a re-execution: a capture retains none under the
///   per-position cadence. Refused unless its leaves root to the capture's own
///   `checkpoint_merkle_root`, so a leg that is not the capture's is never passed off as its anchor.
/// * `operands` / `artifact_root` — the family's operand inventory under the class's profile; the
///   site's four registered narrowings are opened out of it by the one description of their names
///   (`palw_attn_site_operand_names_v2`), so the openings filed are the ones the court asks for.
/// * `anchor_chunks(covered)` — the family's recompute of the checkpoint whose counter is
///   `covered`, every chunk in map order. Asked only when the capture did not keep that
///   checkpoint's chunks (the per-position cadence keeps none), and refused unless the bytes root
///   to the committed leaf: a recompute that disagrees with the executor's own commitment is not
///   evidence about it.
///
/// Refuses empty tiles (a fold) by name: the caller supplies the dense rows.
#[allow(clippy::too_many_arguments)]
pub fn base0_attn_site_evidence_v1(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &crate::legs::Base0StepTilesV1,
    checkpoints: &crate::legs::Base0CheckpointsV1,
    narrowed: u64,
    operands: &[PalwArtifactOperandV1],
    artifact_root: Hash64,
    step_ladder_cap: u64,
    anchor_chunks: &mut dyn FnMut(u32) -> Result<Vec<Vec<u8>>, String>,
) -> Result<PalwAttnSiteEvidenceV1, String> {
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    if tiles.tiles.is_empty() || tiles.leaves.is_empty() {
        return Err("the capture holds no dense rows; a fused site's evidence opens committed rows".to_string());
    }

    // The site's registered narrowings, opened out of the family's own inventory.
    let names = kaspa_consensus_core::palw_court_v2::palw_attn_site_operand_names_v2(binding, narrowed).map_err(|e| e.to_string())?;
    let mut operand_openings = Vec::with_capacity(names.len());
    for (name, layer) in &names {
        let index = operands
            .iter()
            .position(|o| o.tensor_name == *name && o.layer == *layer && o.row_start == 0)
            .ok_or_else(|| format!("the inventory holds no row {name} at layer {layer:?}"))?;
        let opening =
            open_artifact_leaf_v1(operands, index as u32).ok_or_else(|| format!("the inventory row {name} does not open"))?;
        operand_openings.push(opening);
    }
    let proven = PalwProvenOperandsV1::from_openings_v1(&operand_openings, artifact_root).map_err(|e| e.to_string())?;
    let site = kaspa_consensus_core::palw_court_v2::palw_attn_dispute_site_unpinned_v2(binding, &proven, narrowed, None)
        .map_err(|e| e.to_string())?;
    let s = &site.site;

    // The committed rows by index, and the tree they root — built once for every opening below.
    let by_index: HashMap<u64, &PalwStepTileLeafV1> = tiles.tiles.iter().map(|(i, leaf)| (*i, leaf)).collect();
    let tree = PalwStepMerkleTreeV1::build_capped_v1(&tiles.leaves, step_ladder_cap).map_err(|e| format!("{e:?}"))?;
    if tree.root() != binding.step_merkle_root {
        return Err("the capture's leaves do not root to its own binding".to_string());
    }
    let opening = |index: u64| -> Result<PalwAttnRowOpeningV1, String> {
        let leaf = by_index.get(&index).ok_or_else(|| format!("the capture holds no committed row at leaf {index}"))?;
        let leaf_hash = *tiles.leaves.get(index as usize).ok_or_else(|| format!("leaf {index} is outside the capture"))?;
        let siblings = tree.path_v1(index as usize).map_err(|e| format!("{e:?}"))?;
        Ok(PalwAttnRowOpeningV1 { leaf: (*leaf).clone(), opening: PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings } })
    };
    let index_of = |call_index: u32, node_slot: u32, position: u32, tile_index: u32| -> Result<u64, String> {
        canonical_step_leaf_index(profile, ctx, &PalwStepCoordinateV1 { call_index, node_slot, position, tile_index })
            .ok_or_else(|| format!("({call_index}, {node_slot}, {position}, {tile_index}) is not a canonical coordinate of this job"))
    };

    // The output tile the ladder narrowed to, and the query row the disputed head reads.
    let out_tile = opening(narrowed)?;
    let query = opening(index_of(s.disputed.call_index, s.query_slot, s.disputed.position, s.query_tile_index)?)?;
    let q_row = lanes_of(&query.leaf);
    let qh = q_row
        .get(s.query_lane_offset..s.query_lane_offset + s.d_head)
        .ok_or_else(|| {
            format!("the query row holds {} lanes and the head's slice ends at {}", q_row.len(), s.query_lane_offset + s.d_head)
        })?
        .to_vec();

    // The history: every K and V row the site reads, as the cache writers committed them. A
    // history position's own coordinate is the canonical enumeration's — `j < prefill` is
    // `(0, j)`, `j ≥ prefill` is `(j − prefill + 1, 0)`.
    let (k_slot, v_slot) = match (s.k_slot, s.v_slot) {
        (Some(k), Some(v)) => (k, v),
        _ => return Err("the class declares no cache-writer pair to read the history from".to_string()),
    };
    if s.kv_tile_lanes == 0 {
        return Err("the class's two cache writers tile their rows differently".to_string());
    }
    let row_tiles = s.kv_dim.div_ceil(s.kv_tile_lanes) as u32;
    let history = site.history_positions;
    let prefill = s.prefill_positions;
    let mut k_series = Vec::with_capacity(history as usize * s.kv_dim);
    let mut v_series = Vec::with_capacity(history as usize * s.kv_dim);
    let mut k_rows = Vec::new();
    let mut v_rows = Vec::new();
    for j in 0..history {
        let (call, position) = if j < prefill { (0, j) } else { (j - prefill + 1, 0) };
        for (slot, series, rows) in [(k_slot, &mut k_series, &mut k_rows), (v_slot, &mut v_series, &mut v_rows)] {
            for t in 0..row_tiles {
                let index = index_of(call, slot, position, t)?;
                let leaf = by_index.get(&index).ok_or_else(|| format!("the capture holds no cache row at leaf {index}"))?;
                series.extend(lanes_of(leaf));
                if row_tiles == 1 {
                    rows.push(opening(index)?);
                }
            }
        }
    }
    let inputs = PalwAttnSiteInputsV1 { qh, k_series, v_series };
    inputs.check_v1(&site).map_err(|e| e.to_string())?;
    // One opening per row is the only cache-write route a bottom can carry; a class whose rows
    // are several leaves serves its bottom from the anchor or not at all.
    let cache_rows = (row_tiles == 1).then_some((k_rows, v_rows));

    // The checkpoint the disputed step's evidence must anchor at, with every chunk of its state.
    let anchor = match s.anchor_covered_decode_call {
        None => None,
        Some(covered) => {
            let leaves = &checkpoints.leaves;
            let i = leaves
                .iter()
                .position(|leaf| leaf.covered_decode_call == covered)
                .ok_or_else(|| format!("the capture commits no checkpoint at counter {covered}"))?;
            // The checkpoint leg is its own small tree, walked at the ruleset's ladder like every
            // step walk (ADR-0084 U-08) — its count is bounded far below it.
            if step_merkle_root_capped_v1(&checkpoints.leaf_hashes, step_ladder_cap).ok() != Some(binding.checkpoint_merkle_root) {
                return Err("the capture's checkpoint leaves do not root to its own binding".to_string());
            }
            let siblings = step_merkle_path_capped_v1(&checkpoints.leaf_hashes, i, step_ladder_cap).map_err(|e| format!("{e:?}"))?;
            let chunks = match checkpoints.chunks.get(i) {
                Some(kept) if !kept.is_empty() => kept.clone(),
                _ => anchor_chunks(covered)?,
            };
            // The leaves and the root under the CLASS's map (ADR-0103 Decision 3): the held map's
            // leaf binds `(slice, block)` and its root is a tree over slice sub-roots.
            let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                &binding.shape_profile,
                &binding.job_context,
                covered,
            );
            let chunk_hashes: Vec<Hash64> = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_leaves_for_map_v1(
                &binding.shape_profile,
                positions,
                &chunks,
            )
            .map_err(|e| format!("the anchor's chunks at counter {covered} are not the map's: {e:?}"))?;
            let root = kaspa_consensus_core::palw_state_chunk_map::palw_state_root_from_leaves_for_map_v1(
                &binding.shape_profile,
                positions,
                &chunk_hashes,
            );
            if root.ok() != Some(leaves[i].state_chunks_root) {
                return Err(format!("the anchor's state at counter {covered} does not root to the committed checkpoint"));
            }
            Some(PalwAttnAnchorEvidenceV1 {
                anchor: PalwAttnCheckpointAnchorV1 {
                    leaf: leaves[i].clone(),
                    opening: PalwStepOpeningV1 { leaf_index: i as u64, leaf_hash: checkpoints.leaf_hashes[i], siblings },
                },
                chunks,
                chunk_hashes,
            })
        }
    };

    Ok(PalwAttnSiteEvidenceV1 { narrowed, binding: binding.clone(), out_tile, query, operand_openings, inputs, anchor, cache_rows })
}
