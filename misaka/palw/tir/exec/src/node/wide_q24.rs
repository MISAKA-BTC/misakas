//! Build a disclosure for one tile in an authenticated wide capture.
use super::tree::TirStepTreeV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::{Hash64, palw_logits_q24_v2 as wide};

pub(super) fn event(ctx: &PalwJobContextV2, rows: &[Vec<i64>], ids: &[u32], row: u32, tile: u32) -> Option<wide::WideLogitsEventV2> {
    let roots: Vec<Hash64> =
        rows.iter().enumerate().map(|(i, r)| wide::row_root(&ctx.context_hash(), i as u32, r)).collect::<Option<_>>()?;
    let row_root = *roots.get(row as usize)?;
    let row_opening = TirStepTreeV1::full(&roots).opening(row as u64)?;
    let lanes = rows.get(row as usize)?;
    let tile_hashes: Vec<_> = lanes
        .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
        .enumerate()
        .map(|(i, l)| wide::tile_leaf(&ctx.context_hash(), row, i as u32, l))
        .collect();
    let tile_opening = TirStepTreeV1::full(&tile_hashes).opening(tile as u64)?;
    let lanes = lanes.chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES).nth(tile as usize)?.to_vec();
    Some(wide::WideLogitsEventV2 { row, generated: ids.to_vec(), row_root, row_opening, lanes, tile_opening })
}
