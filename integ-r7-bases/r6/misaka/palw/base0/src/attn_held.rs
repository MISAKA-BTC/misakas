//! **ADR-0152 §4-ter (A-held), the node's side: a held fused site's evidence, WINDOWED.**
//!
//! The dense builder (`attn_responder`) re-executes the job densely and holds every tile — refused
//! past the materialization cap (`2^26` leaves), which a held class's canonical job is past at 8k
//! (≈ 10^8 leaves). Nothing the court asks at a fused site needs the tiles: the site's inputs are the
//! query slice at the site's position and layer ℓ's K and V over the history, and the anchor the
//! bottom reads is the checkpoint AFTER the site's position (`covered = p + 1`), whose state holds
//! exactly those rows. So both parties build the evidence from ONE forward to `p + 1` — the state, in
//! `O(state)` (≈ 1.5–2 GB at the peak at 8,192 positions of the 1.5B row, with the chunks and the
//! engine's working set; the trait verb's note has the budget and why no consensus cap bounds it) —
//! plus two committed leaves opened, the site's output tile and its query row:
//!
//! * **the responder** opens both from its own fold (a block replayed from the interval's anchor,
//!   the path from the digests it retained), anchors at its own retained checkpoint leaf, and files
//!   the anchor's slice sub-roots from its own state;
//! * **a challenger** holds no capture of the accused's: it takes the out tile, the anchor and the
//!   sub-roots off the accused's held root claim, and opens the query row against the accused's step
//!   root from a STREAM of its own honest leaves before the site
//!   (`kaspa_consensus_core::palw_step_leg::PalwStepPrefixStreamV1`: a frontier per level, never the
//!   prefix's ~10^8 leaves) and the accused's filed path of the site.
//!
//! This module is the family-independent half — the site, its inputs out of a state, the evidence
//! assembled; the family's backend produces the state and the two openings. Every number a move
//! carries is then computed by the court's own kernels (`kaspa_consensus_core::palw_attn_responder_v1`),
//! exactly as for the dense builder — whose evidence this equals wherever both run.

use kaspa_consensus_core::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1, open_artifact_leaf_v1};
use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnRowOpeningV1};
use kaspa_consensus_core::palw_attn_responder_v1::{
    PalwAttnAnchorEvidenceV1, PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1, PalwAttnSiteEvidenceV1, PalwAttnSiteInputsV1,
};
use kaspa_consensus_core::palw_court_v2::PalwAttnDisputeSiteV2;
use kaspa_consensus_core::palw_state_chunk_map::{PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, PalwStepTileLeafV1};
use kaspa_hashes::Hash64;

fn lanes_of(leaf: &PalwStepTileLeafV1) -> Vec<i32> {
    leaf.values_le.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

/// **The site's registered narrowings, opened out of the family's inventory**, and the site derived
/// from them — with `anchor` when one is in hand, its rows opened under `opening_cap` (the claim's
/// own ladder under the held regime, as the court opens them).
pub fn base0_attn_held_site_v1(
    binding: &PalwStepBindingV2,
    narrowed: u64,
    operands: &[PalwArtifactOperandV1],
    artifact_root: Hash64,
    anchor: Option<&PalwAttnCheckpointAnchorV1>,
    opening_cap: u64,
) -> Result<(PalwAttnDisputeSiteV2, Vec<PalwArtifactOpeningV1>), String> {
    let names = kaspa_consensus_core::palw_court_v2::palw_attn_site_operand_names_v2(binding, narrowed).map_err(|e| e.to_string())?;
    let mut openings = Vec::with_capacity(names.len());
    for (name, layer) in &names {
        let index = operands
            .iter()
            .position(|o| o.tensor_name == *name && o.layer == *layer && o.row_start == 0)
            .ok_or_else(|| format!("the inventory holds no row {name} at layer {layer:?}"))?;
        openings.push(open_artifact_leaf_v1(operands, index as u32).ok_or_else(|| format!("the inventory row {name} does not open"))?);
    }
    let proven = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).map_err(|e| e.to_string())?;
    let site =
        kaspa_consensus_core::palw_court_v2::palw_attn_dispute_site_unpinned_v3(binding, &proven, narrowed, anchor, opening_cap)
            .map_err(|e| e.to_string())?;
    Ok((site, openings))
}

/// **The query row's leaf** — the site's rotated-query node at the site's own call and position, the
/// tile holding the disputed head's slice.
pub fn base0_attn_held_query_leaf_v1(binding: &PalwStepBindingV2, site: &PalwAttnDisputeSiteV2) -> Result<u64, String> {
    let s = &site.site;
    canonical_step_leaf_index(
        &binding.shape_profile,
        &binding.job_context,
        &PalwStepCoordinateV1 {
            call_index: s.disputed.call_index,
            node_slot: s.query_slot,
            position: s.disputed.position,
            tile_index: s.query_tile_index,
        },
    )
    .ok_or_else(|| "the query row is not a canonical coordinate of this job".to_string())
}

/// **The step the site's position is** — `p + 1` for prefill position `p`, `prefill + c` for decode
/// call `c`: the window a replay to the site walks, and the positions its state then covers.
pub fn base0_attn_held_site_step_v1(binding: &PalwStepBindingV2, site: &PalwAttnDisputeSiteV2) -> u64 {
    let d = &site.site.disputed;
    if d.call_index == 0 {
        u64::from(d.position) + 1
    } else {
        u64::from(binding.job_context.declared_prefill_tokens) + u64::from(d.call_index)
    }
}

/// **The site's inputs out of the anchor's state**: the head's query slice from the opened query
/// row, and layer ℓ's K and V rows for the whole history `0..history` from the state's chunks (the
/// anchor covers exactly the history: `covered = p + 1`). Every row is the map's `i32` codes, the
/// same bytes the bottom reads out of a chunk.
pub fn base0_attn_held_inputs_v1(
    site: &PalwAttnDisputeSiteV2,
    query: &PalwStepTileLeafV1,
    chunks: &[Vec<u8>],
) -> Result<PalwAttnSiteInputsV1, String> {
    let s = &site.site;
    let q_row = lanes_of(query);
    let qh = q_row
        .get(s.query_lane_offset..s.query_lane_offset + s.d_head)
        .ok_or_else(|| {
            format!("the query row holds {} lanes and the head's slice ends at {}", q_row.len(), s.query_lane_offset + s.d_head)
        })?
        .to_vec();
    let geometry = s.anchor_geometry.as_ref().ok_or("the site derived no anchor layout")?;
    let history = site.history_positions as usize;
    if s.anchor_positions as usize != history {
        return Err(format!("the anchor covers {} positions and the site reads {history}", s.anchor_positions));
    }
    let mut k_series = vec![0i32; history * s.kv_dim];
    let mut v_series = vec![0i32; history * s.kv_dim];
    let (mut k_rows, mut v_rows) = (0usize, 0usize);
    for index in 0..geometry.chunk_count() {
        let entry = integer_kv_state_chunk_entry_v1(geometry, index).ok_or("a chunk outside the anchor's layout")?;
        if entry.attn_layer != s.attn_layer {
            continue;
        }
        if entry.row_bytes as usize != 4 * s.kv_dim {
            return Err(format!("a state row is {} bytes and the site's cache row {} codes", entry.row_bytes, s.kv_dim));
        }
        let bytes = chunks.get(index as usize).ok_or("the state holds fewer chunks than its layout")?;
        if bytes.len() as u64 != entry.byte_len() {
            return Err(format!("chunk {index} holds {} bytes and its entry {}", bytes.len(), entry.byte_len()));
        }
        let (series, counted) = match entry.kind {
            PalwStateChunkKindV1::Key => (&mut k_series, &mut k_rows),
            PalwStateChunkKindV1::Value => (&mut v_series, &mut v_rows),
        };
        let first = entry.position_start as usize * s.kv_dim;
        let into = series.get_mut(first..first + bytes.len() / 4).ok_or("a chunk past the history")?;
        for (lane, b) in into.iter_mut().zip(bytes.chunks_exact(4)) {
            *lane = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        *counted += entry.position_count as usize;
    }
    if (k_rows, v_rows) != (history, history) {
        return Err(format!("the state holds {k_rows} K and {v_rows} V rows of layer {} for a history of {history}", s.attn_layer));
    }
    let inputs = PalwAttnSiteInputsV1 { qh, k_series, v_series };
    inputs.check_v1(site).map_err(|e| e.to_string())?;
    Ok(inputs)
}

/// **The evidence, assembled** — the anchor with `chunks` (THIS party's own state at it) and their
/// leaf hashes under the class's map, the inputs out of the same state, the two openings, and the
/// slice sub-roots: the party's own (`filed = None`, the responder — whose state must then root to
/// its own anchor) or the accused's (`filed = Some`, a challenger — whose state need not: past the
/// lie it is not the accused's, and [`PalwAttnHeldEvidenceV1::fallback_v1`] names the one case the
/// bottom cannot be built).
#[allow(clippy::too_many_arguments)]
pub fn base0_attn_held_evidence_v1(
    binding: &PalwStepBindingV2,
    narrowed: u64,
    site: &PalwAttnDisputeSiteV2,
    operand_openings: Vec<PalwArtifactOpeningV1>,
    out_tile: PalwAttnRowOpeningV1,
    query: PalwAttnRowOpeningV1,
    anchor: PalwAttnCheckpointAnchorV1,
    chunks: Vec<Vec<u8>>,
    filed: Option<Vec<Hash64>>,
) -> Result<PalwAttnHeldEvidenceV1, String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    let profile = &binding.shape_profile;
    let positions = site.site.anchor_positions;
    let chunk_hashes = map::palw_state_chunk_leaves_for_map_v1(profile, positions, &chunks)
        .map_err(|e| format!("the anchor's state is not the map's: {e:?}"))?;
    let layout = map::palw_state_layout_v4(profile, positions).map_err(|e| format!("the anchor's held layout: {e:?}"))?;
    let own = map::palw_state_slice_sub_roots_v4(&layout, &chunk_hashes).map_err(|e| format!("the state's sub-roots: {e:?}"))?;
    let slice_sub_roots = match filed {
        Some(filed) => filed,
        None => {
            let root = map::palw_state_top_root_from_sub_roots_v4(&layout, &own).map_err(|e| format!("{e:?}"))?;
            if root != anchor.leaf.state_chunks_root {
                return Err(format!(
                    "this node's state at the anchor ({positions} positions) does not root to its own committed checkpoint"
                ));
            }
            own
        }
    };
    let inputs = base0_attn_held_inputs_v1(site, &query.leaf, &chunks)?;
    let evidence = PalwAttnSiteEvidenceV1 {
        narrowed,
        binding: binding.clone(),
        out_tile,
        query,
        operand_openings,
        inputs,
        anchor: Some(PalwAttnAnchorEvidenceV1 { anchor, chunks, chunk_hashes }),
        cache_rows: None,
    };
    Ok(PalwAttnHeldEvidenceV1 { evidence, slice_sub_roots })
}

/// **A challenger's check of the accused's filing** against the site derived with its anchor: the
/// anchor is the site's committed checkpoint (the bottom's own check), the filed sub-roots root to it
/// (the fold's H3), and the out tile is the narrowed leaf, opening against the filed binding.
pub fn base0_attn_held_filing_is_the_sites_v1(
    filing: &PalwAttnHeldFilingV1,
    site: &PalwAttnDisputeSiteV2,
    narrowed: u64,
) -> Result<(), String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    kaspa_consensus_core::palw_attn_court_v1::palw_attn_anchor_is_the_sites_v1(&filing.anchor, &site.binding, &site.site)
        .map_err(|e| format!("the filed anchor is not the site's committed checkpoint: {e}"))?;
    let layout = map::palw_state_layout_v4(&filing.binding.shape_profile, site.site.anchor_positions)
        .map_err(|e| format!("the anchor's held layout: {e:?}"))?;
    let rooted = map::palw_state_top_root_from_sub_roots_v4(&layout, &filing.slice_sub_roots)
        .map_err(|e| format!("the filed sub-roots are not the layout's: {e:?}"))?;
    if rooted != filing.anchor.leaf.state_chunks_root {
        return Err("the filed sub-roots do not root to the filed anchor".to_string());
    }
    if filing.out_tile.opening.leaf_index != narrowed {
        return Err(format!(
            "the filed output tile is leaf {}, and the court narrowed to {narrowed}",
            filing.out_tile.opening.leaf_index
        ));
    }
    kaspa_consensus_core::palw_attn_court_v1::palw_attn_opened_lanes_v1(&filing.out_tile, &site.binding, site.head_lanes.2 as usize)
        .map_err(|e| format!("the filed output tile does not open against the filed binding: {e}"))?;
    Ok(())
}
