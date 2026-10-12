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
    PalwStepMerkleTreeV1, PalwStepOpeningV1, PalwStepPrefixTreeV1, PalwStepTileLeafV1, step_merkle_path_capped_v1,
    step_merkle_root_capped_v1,
};
use kaspa_hashes::Hash64;

fn lanes_of(leaf: &PalwStepTileLeafV1) -> Vec<i32> {
    leaf.values_le.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

/// **Where a fused site's committed rows are read from** (ADR-0093 Decisions 1 and 7).
pub enum Base0AttnRowsV1<'a> {
    /// The capture's OWN committed rows: a dense capture's tiles, or a fold re-executed into the
    /// same execution (its root reproduced). Opened against the tree they build, which must be the
    /// binding's.
    Committed(&'a crate::legs::Base0StepTilesV1),
    /// **A fold whose re-execution is NOT the committed execution** — the forged claim a fold
    /// cannot re-derive, because it keeps no rows. The rows before the disputed leaf are the
    /// challenger's own (`honest`): the court narrowed to the FIRST leaf the challenger could not
    /// reproduce, so every earlier committed leaf is one the challenger's execution computes. The
    /// disputed leaf is the accused's own opened output tile — its root claim's, on chain. Opened
    /// against the accused's root through [`PalwStepPrefixTreeV1`], and refused by name unless that
    /// root IS the binding's: a prefix that is not the accused's opens nothing.
    HonestPrefix { honest: &'a crate::legs::Base0StepTilesV1, accused_out_tile: &'a PalwAttnRowOpeningV1 },
}

/// A filing without an anchor, refused because this node's own checkpoint leg is not the accused's:
/// the accused's execution followed its lie past the disputed call, and only the anchored root
/// claim (ADR-0093 Decision 8) carries the path a challenger cannot rebuild. Named so, rather than
/// as the generic leg mismatch it surfaces as.
pub fn base0_attn_name_the_missing_anchor_v1(
    why: String,
    filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1,
) -> String {
    if filing.anchor.is_none() && why.contains("checkpoint leaves do not root") {
        format!(
            "{why} — the accused's execution followed its lie past the disputed call, and its root claim carries no anchor \
             (the anchored root claim of ADR-0093 Decision 8 carries it)"
        )
    } else {
        why
    }
}

/// **Where the anchor a checkpoint-route bottom reads comes from** (ADR-0093 Decisions 1 and 8).
pub enum Base0AttnAnchorSourceV1<'a> {
    /// A checkpoint leg: a re-execution's for a capture that reproduces its own execution, a fold's
    /// own retained leaves otherwise. Refused unless its leaves root to the binding's leg.
    Leg(&'a crate::legs::Base0CheckpointsV1),
    /// The accused's own anchor, off its anchored root claim — checked against the site the class
    /// derives and against the binding's checkpoint leg (the bottom's own check), its state
    /// recomputed and required to root to it. The path a challenger cannot rebuild once the
    /// accused's execution followed its lie.
    Filed(&'a PalwAttnCheckpointAnchorV1),
}

/// **The evidence one capture yields about the fused site at `narrowed`.**
///
/// * `binding` / `rows` — the CAPTURE's own commitment and where its committed rows come from
///   ([`Base0AttnRowsV1`]): the accused's retained tiles when the capture is the accused's, so the
///   openings prove against the claim's step tree even when the execution is a forgery (the bottom
///   must open what the claim committed, not what an honest re-execution computes); or, for a
///   fold that re-executes into another execution, the honest prefix and the accused's opened
///   output tile. Refused unless they root to the binding.
/// * `anchor_source` — where the bottom's anchor comes from ([`Base0AttnAnchorSourceV1`]): a
///   checkpoint leg (a re-execution's for a capture that reproduces its own execution, the fold's
///   own retained leaves otherwise), refused unless its leaves root to the capture's own
///   `checkpoint_merkle_root`; or the accused's filed anchor, checked as the bottom checks it.
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
    rows: Base0AttnRowsV1<'_>,
    anchor_source: Base0AttnAnchorSourceV1<'_>,
    narrowed: u64,
    operands: &[PalwArtifactOperandV1],
    artifact_root: Hash64,
    step_ladder_cap: u64,
    anchor_chunks: &mut dyn FnMut(u32) -> Result<Vec<Vec<u8>>, String>,
) -> Result<PalwAttnSiteEvidenceV1, String> {
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;
    let tiles = match rows {
        Base0AttnRowsV1::Committed(tiles) | Base0AttnRowsV1::HonestPrefix { honest: tiles, .. } => tiles,
    };
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
    // **The rows open under this backend's ladder** (ADR-0119 Decision 4): the court opens a fused
    // site's rows under the claim's ladder where the held regime is in force — the network's step
    // ladder for every class this node produces, which is the ladder it was built at — and under the
    // structural `2^22` before it. Deriving at `2^22` here refused, at the first opening below,
    // every claim past about 36 prompt tokens on testnet-11's dense row, so the regime's raise of
    // the court's cap would have been one no honest party could use. Before the regime the panel
    // files nothing the court's `2^22` would refuse (`attn_root_claim_is_openable_v1`).
    let site =
        kaspa_consensus_core::palw_court_v2::palw_attn_dispute_site_unpinned_v3(binding, &proven, narrowed, None, step_ladder_cap)
            .map_err(|e| e.to_string())?;
    let s = &site.site;

    // The committed rows by index, and the tree they root — built once for every opening below.
    let by_index: HashMap<u64, &PalwStepTileLeafV1> = tiles.tiles.iter().map(|(i, leaf)| (*i, leaf)).collect();
    enum Tree<'a> {
        Whole(PalwStepMerkleTreeV1),
        Prefix(PalwStepPrefixTreeV1, &'a PalwAttnRowOpeningV1),
    }
    let tree = match rows {
        Base0AttnRowsV1::Committed(_) => {
            let tree = PalwStepMerkleTreeV1::build_capped_v1(&tiles.leaves, step_ladder_cap).map_err(|e| format!("{e:?}"))?;
            if tree.root() != binding.step_merkle_root {
                return Err("the capture's leaves do not root to its own binding".to_string());
            }
            Tree::Whole(tree)
        }
        Base0AttnRowsV1::HonestPrefix { accused_out_tile, .. } => {
            if accused_out_tile.opening.leaf_index != narrowed {
                return Err(format!(
                    "the accused's opened output tile is leaf {}, and the court narrowed to {narrowed}",
                    accused_out_tile.opening.leaf_index
                ));
            }
            let before = tiles.leaves.get(..narrowed as usize).ok_or_else(|| format!("the honest rows end before leaf {narrowed}"))?;
            let mut prefix = before.to_vec();
            prefix.push(accused_out_tile.opening.leaf_hash);
            let tree =
                PalwStepPrefixTreeV1::build_capped_v1(binding.step_leaf_count, &prefix, &accused_out_tile.opening, step_ladder_cap)
                    .map_err(|e| format!("the honest rows before the disputed leaf are not the accused's: {e}"))?;
            if tree.root() != binding.step_merkle_root {
                return Err(
                    "the honest rows before the disputed leaf and the accused's opened tile do not root to the accused's binding \
                     — the first divergence is not this leaf"
                        .to_string(),
                );
            }
            Tree::Prefix(tree, accused_out_tile)
        }
    };
    let opening = |index: u64| -> Result<PalwAttnRowOpeningV1, String> {
        match &tree {
            Tree::Whole(tree) => {
                let leaf = by_index.get(&index).ok_or_else(|| format!("the capture holds no committed row at leaf {index}"))?;
                let leaf_hash = *tiles.leaves.get(index as usize).ok_or_else(|| format!("leaf {index} is outside the capture"))?;
                let siblings = tree.path_v1(index as usize).map_err(|e| format!("{e:?}"))?;
                Ok(PalwAttnRowOpeningV1 {
                    leaf: (*leaf).clone(),
                    opening: PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings },
                })
            }
            Tree::Prefix(tree, accused_out_tile) => {
                if index == narrowed {
                    return Ok((*accused_out_tile).clone());
                }
                if index > narrowed {
                    return Err(format!("leaf {index} is after the disputed leaf, where the honest rows are not the accused's"));
                }
                let leaf = by_index.get(&index).ok_or_else(|| format!("the honest rows hold no row at leaf {index}"))?;
                Ok(PalwAttnRowOpeningV1 { leaf: (*leaf).clone(), opening: tree.opening_v1(index).map_err(|e| format!("{e:?}"))? })
            }
        }
    };
    let index_of = |call_index: u32, node_slot: u32, position: u32, tile_index: u32| -> Result<u64, String> {
        canonical_step_leaf_index(profile, ctx, &PalwStepCoordinateV1 { call_index, node_slot, position, tile_index })
            .ok_or_else(|| format!("({call_index}, {node_slot}, {position}, {tile_index}) is not a canonical coordinate of this job"))
    };

    // The output tile the ladder narrowed to, and the query row the disputed head reads. On the
    // prefix route the tile is the accused's, carried in: its preimage must open against the
    // accused's own commitment, which the site's bottom binding checks lane by lane.
    let out_tile = opening(narrowed)?;
    if matches!(tree, Tree::Prefix(..)) {
        kaspa_consensus_core::palw_attn_court_v1::palw_attn_opened_lanes_v1(&out_tile, &site.binding, site.head_lanes.2 as usize)
            .map_err(|e| format!("the accused's opened output tile does not open against its own binding: {e}"))?;
    }
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
    let anchor = match (s.anchor_covered_decode_call, anchor_source) {
        (None, _) => None,
        (Some(covered), Base0AttnAnchorSourceV1::Filed(filed)) => {
            // The filed anchor, checked exactly as the bottom will check it — against the site
            // derived WITH it (the anchor's layout is the site's), and the binding's leg.
            let with_anchor = kaspa_consensus_core::palw_court_v2::palw_attn_dispute_site_unpinned_v3(
                binding,
                &proven,
                narrowed,
                Some(filed),
                step_ladder_cap,
            )
            .map_err(|e| e.to_string())?;
            kaspa_consensus_core::palw_attn_court_v1::palw_attn_anchor_is_the_sites_v1(filed, &with_anchor.binding, &with_anchor.site)
                .map_err(|e| format!("the filed anchor is not the site's committed checkpoint: {e}"))?;
            let chunks = anchor_chunks(covered)?;
            // Under the CLASS's map, as the leg's branch below (ADR-0103 Decision 3) — the flat
            // leaf and root this branch was written with are the v3 map's, and a held class's
            // filed anchor roots its chunks under the held map's tree over slice sub-roots.
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
            if root.ok() != Some(filed.leaf.state_chunks_root) {
                return Err(format!("the anchor's state at counter {covered} does not root to the filed checkpoint"));
            }
            Some(PalwAttnAnchorEvidenceV1 { anchor: filed.clone(), chunks, chunk_hashes })
        }
        (Some(covered), Base0AttnAnchorSourceV1::Leg(checkpoints)) => {
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

#[cfg(test)]
mod tests {
    /// **ADR-0119 Decision 4, pinned where it lives: a party derives the fused site at its
    /// backend's ladder, never at the structural `2^22`.** The court opens the site's rows at the
    /// claim's ladder under the held regime; a party that derived at `2^22` refused its own first
    /// opening for every claim past it — on testnet-11's dense row, every claim past about 36 prompt
    /// tokens — so the regime's raise would have been one no honest party could answer with. A
    /// capture that large is too heavy for a unit test, so the derivation is pinned by its source.
    #[test]
    fn a_party_derives_the_fused_site_at_its_backends_ladder() {
        let whole = include_str!("attn_responder.rs");
        let source = &whole[..whole.find("#[cfg(test)]\nmod tests {").expect("the unit tests follow the code")];
        assert!(!source.contains("palw_attn_dispute_site_unpinned_v2("), "no derivation at the structural cap");
        let derivations: Vec<&str> =
            source.match_indices("palw_attn_dispute_site_unpinned_v3(").map(|(at, _)| &source[at..]).collect();
        assert_eq!(derivations.len(), 2, "the site, and the site with the accused's filed anchor");
        for call in derivations {
            let args = &call[..call.find(".map_err").expect("the call ends")];
            assert!(args.contains("step_ladder_cap"), "derived at the backend's ladder: {args}");
        }
    }
}
