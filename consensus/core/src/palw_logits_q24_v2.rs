//! Explicit wide Q24 output profile. All values retain natural-log units times 2^24.
//! Old i32 trace types, domains and bytes are unchanged. Public presets do not arm court v2.
use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_root_v1, step_opening_root_capped_v1};
use crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
use crate::{Hash64, palw_v2::PalwJobContextV2};

pub fn scheme_id() -> Hash64 {
    Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2)
}
fn hash(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}
pub fn tile_leaf(ctx: &Hash64, row: u32, tile: u32, lanes: &[i64]) -> Hash64 {
    let bytes: Vec<u8> = lanes.iter().flat_map(|v| v.to_le_bytes()).collect();
    hash(
        b"misaka-palw/tiled-logits/tile/v2",
        &[ctx.as_byte_slice(), &(row as u64).to_le_bytes(), &(tile as u64).to_le_bytes(), &bytes],
    )
}
pub fn row_root(ctx: &Hash64, row: u32, lanes: &[i64]) -> Option<Hash64> {
    let leaves: Vec<_> = lanes.chunks(PALW_LOGITS_TILE_LANES).enumerate().map(|(t, l)| tile_leaf(ctx, row, t as u32, l)).collect();
    step_merkle_root_v1(&leaves).ok()
}
pub fn rows_root(ctx: &PalwJobContextV2, rows: &[Vec<i64>]) -> Option<Hash64> {
    let roots: Vec<_> = rows.iter().enumerate().map(|(r, l)| row_root(&ctx.context_hash(), r as u32, l)).collect::<Option<_>>()?;
    step_merkle_root_v1(&roots).ok()
}
pub fn outer_root(ctx: &PalwJobContextV2, count: u64, root: &Hash64, ids: &[u32]) -> Hash64 {
    let ids: Vec<_> = ids.iter().flat_map(|i| i.to_le_bytes()).collect();
    hash(
        b"misaka-palw/tiled-logits/root/v2",
        &[
            ctx.context_hash().as_byte_slice(),
            &(ctx.declared_prefill_tokens as u64).to_le_bytes(),
            &(ctx.exact_decode_tokens as u64).to_le_bytes(),
            &count.to_le_bytes(),
            root.as_byte_slice(),
            &ids,
        ],
    )
}
pub fn trace_root(ctx: &PalwJobContextV2, rows: &[Vec<i64>], ids: &[u32]) -> Option<Hash64> {
    if rows.len() != ids.len() || rows.len() != ctx.exact_decode_tokens as usize {
        return None;
    }
    Some(outer_root(ctx, rows.len() as u64, &rows_root(ctx, rows)?, ids))
}
/// Q48 keys need i128: i64 Q24 times 2^24 fits within 88 signed bits.
pub fn lane_key(value: i64, sampling: &crate::palw_decode_select_v2::PalwDecodeSamplingV2, position: u32, lane: usize) -> i128 {
    let base = (value as i128) * (1i128 << 24);
    if sampling.is_greedy() {
        return base;
    }
    base + (sampling.temperature_q as i128) * (crate::palw_decode_select_v2::gumbel_q24_v1(&sampling.seed, position, lane) as i128)
}
pub fn select(values: &[i64], sampling: &crate::palw_decode_select_v2::PalwDecodeSamplingV2, position: u32) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (i, lane_key(*v, sampling, position, i)))
        .reduce(|a, b| if b.1 > a.1 { b } else { a })
        .map(|x| x.0)
}

/// Independently versioned event. It authenticates one row and one 8-byte-lane tile.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct WideLogitsEventV2 {
    pub row: u32,
    pub generated: Vec<u32>,
    pub row_root: Hash64,
    pub row_opening: PalwStepOpeningV1,
    pub lanes: Vec<i64>,
    pub tile_opening: PalwStepOpeningV1,
}
impl WideLogitsEventV2 {
    pub fn authenticate(&self, ctx: &PalwJobContextV2, trace: &Hash64, vocab: usize, cap: u64) -> Result<(), &'static str> {
        if ctx.trace_scheme_id != scheme_id()
            || self.generated.len() != ctx.exact_decode_tokens as usize
            || self.row >= ctx.exact_decode_tokens
        {
            return Err("wide trace shape/scheme");
        }
        let tile = usize::try_from(self.tile_opening.leaf_index).map_err(|_| "tile index")?;
        let start = tile.checked_mul(PALW_LOGITS_TILE_LANES).ok_or("tile index overflow")?;
        if start >= vocab || self.lanes.len() != (vocab - start).min(PALW_LOGITS_TILE_LANES) {
            return Err("wide tile width");
        }
        if self.row_opening.leaf_index != self.row as u64 || self.row_opening.leaf_hash != self.row_root {
            return Err("wide row identity");
        }
        let rows =
            step_opening_root_capped_v1(ctx.exact_decode_tokens as u64, &self.row_opening, cap).map_err(|_| "wide row opening")?;
        if outer_root(ctx, ctx.exact_decode_tokens as u64, &rows, &self.generated) != *trace {
            return Err("wide trace root");
        }
        if self.tile_opening.leaf_hash != tile_leaf(&ctx.context_hash(), self.row, tile as u32, &self.lanes) {
            return Err("wide tile preimage");
        }
        if step_opening_root_capped_v1(vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64, &self.tile_opening, cap)
            .map_err(|_| "wide tile opening")?
            != self.row_root
        {
            return Err("wide tile root");
        }
        Ok(())
    }
}
