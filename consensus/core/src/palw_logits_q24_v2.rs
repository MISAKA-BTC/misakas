//! Explicit wide Q24 output profile. All values retain natural-log units times 2^24.
//! Old i32 trace types, domains and bytes are unchanged. No native admission,
//! wire dispatch, resource exemption or public activation is supplied here.
//!
//! Callers must authenticate the registered class/job/sampling separately and
//! reserve finite input bytes, hashing/work and peak temporary/retained memory
//! BEFORE calling these helpers. Root builders retain leaf/root vectors and the
//! existing Merkle implementation has its own temporary storage; their resource
//! preflight is the caller's responsibility. The opening `cap` limits Merkle
//! leaf counts, not total allocation/work or model admissibility.
//!
//! Commitments/openings establish data identity only. An authenticated token ID
//! may be outside the vocabulary or lose the sampling comparison: preserving
//! such committed bad data is necessary for a separate deterministic decode
//! refutation. Authentication does not certify token selection or computation.
use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_root_v1, step_opening_root_capped_v1};
use crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
use crate::{Hash64, palw_v2::PalwJobContextV2};

pub fn scheme_id() -> Hash64 {
    Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2)
}
// The registered vocabulary/count coordinate is u32. This check prevents
// truncating indices before hashing and excludes empty/ragged trace rows.
fn vocabulary(width: usize) -> Option<u32> {
    u32::try_from(width).ok().filter(|n| *n != 0)
}
/// Raw data commitment; the caller supplies the row/tile/width interpretation.
/// Empty or malformed preimages can be committed, but do not authenticate as a
/// structurally valid event. No token/computation truth is implied.
pub fn tile_leaf(ctx: &Hash64, row: u32, tile: u32, lanes: &[i64]) -> Hash64 {
    // Hash the identical length-prefixed lane bytes without another full tile
    // allocation. An i64 slice's actual byte length is representable in u64 on
    // the supported 32/64-bit targets.
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tiled-logits/tile/v2").to_state();
    let parts: [&[u8]; 3] = [ctx.as_byte_slice(), &u64::from(row).to_le_bytes(), &u64::from(tile).to_le_bytes()];
    for part in parts {
        h.update(&u64::try_from(part.len()).expect("fixed part fits u64").to_le_bytes());
        h.update(part);
    }
    let bytes = u64::try_from(lanes.len()).expect("slice length fits u64").checked_mul(8).expect("i64 slice bytes fit u64");
    h.update(&bytes.to_le_bytes());
    for lane in lanes {
        h.update(&lane.to_le_bytes());
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte hash"))
}
pub fn row_root(ctx: &Hash64, row: u32, lanes: &[i64]) -> Option<Hash64> {
    vocabulary(lanes.len())?;
    let leaves: Vec<_> = lanes
        .chunks(PALW_LOGITS_TILE_LANES)
        .enumerate()
        .map(|(t, l)| Some(tile_leaf(ctx, row, u32::try_from(t).ok()?, l)))
        .collect::<Option<_>>()?;
    step_merkle_root_v1(&leaves).ok()
}
pub fn rows_root(ctx: &PalwJobContextV2, rows: &[Vec<i64>]) -> Option<Hash64> {
    if ctx.trace_scheme_id != scheme_id()
        || ctx.exact_decode_tokens == 0
        || rows.len() != usize::try_from(ctx.exact_decode_tokens).ok()?
    {
        return None;
    }
    let width = rows.first()?.len();
    vocabulary(width)?;
    if rows.iter().any(|row| row.len() != width) {
        return None;
    }
    let context = ctx.context_hash();
    let roots: Vec<_> = rows.iter().enumerate().map(|(r, l)| row_root(&context, u32::try_from(r).ok()?, l)).collect::<Option<_>>()?;
    step_merkle_root_v1(&roots).ok()
}
/// Raw outer commitment, including the literal count and IDs. Structural
/// authentication checks count/shape; vocabulary membership and selection are
/// separate computation checks, including for coherently committed invalid IDs.
pub fn outer_root(ctx: &PalwJobContextV2, count: u64, root: &Hash64, ids: &[u32]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tiled-logits/root/v2").to_state();
    let context = ctx.context_hash();
    let parts: [&[u8]; 5] = [
        context.as_byte_slice(),
        &u64::from(ctx.declared_prefill_tokens).to_le_bytes(),
        &u64::from(ctx.exact_decode_tokens).to_le_bytes(),
        &count.to_le_bytes(),
        root.as_byte_slice(),
    ];
    for part in parts {
        h.update(&u64::try_from(part.len()).expect("fixed part fits u64").to_le_bytes());
        h.update(part);
    }
    let bytes = u64::try_from(ids.len()).expect("slice length fits u64").checked_mul(4).expect("u32 slice bytes fit u64");
    h.update(&bytes.to_le_bytes());
    for id in ids {
        h.update(&id.to_le_bytes());
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte hash"))
}
pub fn trace_root(ctx: &PalwJobContextV2, rows: &[Vec<i64>], ids: &[u32]) -> Option<Hash64> {
    if rows.len() != ids.len() || rows.len() != usize::try_from(ctx.exact_decode_tokens).ok()? {
        return None;
    }
    Some(outer_root(ctx, u64::try_from(rows.len()).ok()?, &rows_root(ctx, rows)?, ids))
}
/// Q48 keys need i128: i64 Q24 times 2^24 and the existing u32-temperature /
/// i32-Gumbel product fit comfortably. Caller authenticates sampling against the
/// actual job, never the peer's disclosure. Units and lowest-index tie rule are
/// identical to the legacy selector for every i32 input.
pub fn lane_key(value: i64, sampling: &crate::palw_decode_select_v2::PalwDecodeSamplingV2, position: u32, lane: usize) -> i128 {
    let base = i128::from(value) * (1i128 << 24);
    if sampling.is_greedy() {
        return base;
    }
    base + i128::from(sampling.temperature_q) * i128::from(crate::palw_decode_select_v2::gumbel_q24_v1(&sampling.seed, position, lane))
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
    /// Authenticate exact row/tile/count identity under the supplied context.
    /// `vocab` and `cap` must come from independently resolved class/rules. A
    /// generated ID outside `vocab` remains authentic bad-computation evidence;
    /// this method does not check its selector key or authorize any credit.
    pub fn authenticate(&self, ctx: &PalwJobContextV2, trace: &Hash64, vocab: usize, cap: u64) -> Result<(), &'static str> {
        let decode = usize::try_from(ctx.exact_decode_tokens).map_err(|_| "decode index width")?;
        let vocabulary = vocabulary(vocab).ok_or("wide vocabulary width")?;
        let tiles = u64::from(vocabulary).div_ceil(u64::try_from(PALW_LOGITS_TILE_LANES).expect("tile width fits u64"));
        if ctx.trace_scheme_id != scheme_id()
            || decode == 0
            || self.generated.len() != decode
            || self.row >= ctx.exact_decode_tokens
            || u64::from(ctx.exact_decode_tokens) > cap
            || tiles > cap
        {
            return Err("wide trace shape/scheme");
        }
        let tile_coordinate = u32::try_from(self.tile_opening.leaf_index).map_err(|_| "tile index width")?;
        let tile = usize::try_from(tile_coordinate).map_err(|_| "tile index")?;
        let start = tile.checked_mul(PALW_LOGITS_TILE_LANES).ok_or("tile index overflow")?;
        if start >= vocab || self.lanes.len() != (vocab - start).min(PALW_LOGITS_TILE_LANES) {
            return Err("wide tile width");
        }
        if self.row_opening.leaf_index != u64::from(self.row) || self.row_opening.leaf_hash != self.row_root {
            return Err("wide row identity");
        }
        let rows =
            step_opening_root_capped_v1(u64::from(ctx.exact_decode_tokens), &self.row_opening, cap).map_err(|_| "wide row opening")?;
        if outer_root(ctx, u64::from(ctx.exact_decode_tokens), &rows, &self.generated) != *trace {
            return Err("wide trace root");
        }
        if self.tile_opening.leaf_hash != tile_leaf(&ctx.context_hash(), self.row, tile_coordinate, &self.lanes) {
            return Err("wide tile preimage");
        }
        if step_opening_root_capped_v1(tiles, &self.tile_opening, cap).map_err(|_| "wide tile opening")? != self.row_root {
            return Err("wide tile root");
        }
        Ok(())
    }
}
