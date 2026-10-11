//! Bounded continuation of the existing integer prefix K/V digest. Each output tile opens
//! against the original claim step tree; no producer cache snapshot or substitute Merkle tree.
//! Outer admission must bind the job's named prefix tuple to this accepted claim.
use crate::Hash64;
use crate::palw_prefix_state_court_v1::{
    PALW_A16_PREFIX_KV_DIGEST_DOMAIN_V1, PalwPrefixKvPlanV1, PalwPrefixKvTileV1, palw_prefix_kv_authenticated_bytes_v1,
};
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step_leg::{PalwStepBindingV2, verify_binding_v1};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_PREFIX_STREAM_TILE_BYTES_V1: usize = 65_536;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixStateStreamV1 {
    context: Hash64,
    binding: PalwStepBindingV2,
    prefix_tokens: u32,
    total_tiles: u64,
    next_tile: u64,
    value_bytes: u64,
    maximum_leaves: u64,
    digest: LeafHashContinuationV1,
    pending: Option<PalwPrefixTileStreamV1>,
}
/// Small journal checkpoint; immutable authenticated binding stays in the seed row.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PalwPrefixTileStreamV1 {
    length: u32,
    offset: u32,
    expected: Hash64,
    hash: LeafHashContinuationV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixStreamCheckpointV1 {
    context: Hash64,
    next_tile: u64,
    value_bytes: u64,
    digest: LeafHashContinuationV1,
    pending: Option<PalwPrefixTileStreamV1>,
}
impl PalwPrefixStateStreamV1 {
    pub fn new(
        context: Hash64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        prefix_tokens: u32,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        if expected_root != binding.committed_execution_root {
            return Err("prefix stream binding is not the accepted claim root".into());
        }
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if prefix_tokens >= binding.job_context.declared_prefill_tokens {
            return Err("prefix stream is not a proper prompt prefix".into());
        }
        if crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, maximum_leaves)
            .map_err(|e| e.to_string())?
            != binding.step_leaf_count
        {
            return Err("prefix stream has noncanonical step count".into());
        }
        let plan = PalwPrefixKvPlanV1::of_profile(&binding.shape_profile, prefix_tokens).map_err(str::to_string)?;
        let mut digest = LeafHashContinuationV1::new(PALW_A16_PREFIX_KV_DIGEST_DOMAIN_V1);
        digest.update(&u64::from(plan.layer_count).to_le_bytes());
        Ok(Self {
            context,
            binding: binding.clone(),
            prefix_tokens,
            total_tiles: plan.tile_count,
            next_tile: 0,
            value_bytes: 0,
            maximum_leaves,
            digest,
            pending: None,
        })
    }
    pub fn check(&self) -> Result<(), String> {
        if self.next_tile > self.total_tiles
            || self.value_bytes as u128 > self.total_tiles as u128 * crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES as u128
            || self.digest.byte_count() != Some(136 + u128::from(self.value_bytes))
        {
            return Err("prefix digest/cumulative counters inconsistent".into());
        }
        if let Some(p) = &self.pending {
            if p.offset > p.length
                || p.length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES
                || p.hash.byte_count() != Some(282 + u128::from(p.offset))
            {
                return Err("prefix pending oldleaf hash counters inconsistent".into());
            }
        }
        Ok(())
    }
    pub fn next_tile(&self) -> u64 {
        self.next_tile
    }
    pub fn total_tiles(&self) -> u64 {
        self.total_tiles
    }
    /// Convenience for one small already opened tile. Larger legal tiles use begin_tile,
    /// bounded byte moves and finish_tile instead of enlarging a carrier.
    pub fn push(&mut self, context: Hash64, ordinal: u64, tile: &PalwPrefixKvTileV1) -> Result<(), String> {
        if context != self.context || ordinal != self.next_tile || self.next_tile >= self.total_tiles || self.pending.is_some() {
            return Err("prefix tile changes owned identity/order or bounded move".into());
        }
        let plan = PalwPrefixKvPlanV1::of_profile(&self.binding.shape_profile, self.prefix_tokens).map_err(str::to_string)?;
        let bytes =
            palw_prefix_kv_authenticated_bytes_v1(&self.binding, &plan, ordinal, tile, self.maximum_leaves).map_err(str::to_string)?;
        if bytes.len() > PALW_PREFIX_STREAM_TILE_BYTES_V1 {
            return Err("large legal prefix tile requires bounded byte moves".into());
        }
        let total = self.value_bytes.checked_add(bytes.len() as u64).ok_or("prefix cumulative bytes overflow")?;
        self.digest.update(bytes);
        self.value_bytes = total;
        self.next_tile += 1;
        Ok(())
    }
    /// Authenticate the OLD whole tile's header/opening, then consume its bytes in bounded
    /// moves. Up to65536 values (256KiB) remains legal; no active tile profile is dropped.
    pub fn begin_tile(
        &mut self,
        context: Hash64,
        ordinal: u64,
        opening: &crate::palw_step_leg::PalwStepOpeningV1,
    ) -> Result<(), String> {
        if context != self.context || ordinal != self.next_tile || ordinal >= self.total_tiles || self.pending.is_some() {
            return Err("prefix tile opening changes ordered seed or pending tile".into());
        }
        let plan = PalwPrefixKvPlanV1::of_profile(&self.binding.shape_profile, self.prefix_tokens).map_err(str::to_string)?;
        let (coord, count) = plan.coordinate(&self.binding.shape_profile, ordinal).ok_or("prefix next coordinate missing")?;
        let index = crate::palw_step::canonical_step_leaf_index(&self.binding.shape_profile, &self.binding.job_context, &coord)
            .ok_or("prefix coordinate noncanonical")?;
        if opening.leaf_index != index
            || crate::palw_step_leg::step_opening_root_capped_v1(self.binding.step_leaf_count, opening, self.maximum_leaves)
                .map_err(|e| e.to_string())?
                != self.binding.step_merkle_root
        {
            return Err("prefix next tile opening is not in original claim step tree".into());
        }
        let length = count.checked_mul(4).ok_or("prefix tile byte length overflow")?;
        if length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES {
            return Err("prefix tile exceeds active shape format".into());
        }
        let mut hash = LeafHashContinuationV1::new(crate::palw_step_leg::PALW_STEP_LEG_DOMAIN_TILE_LEAF);
        hash.update(&crate::palw_step::PALW_STEP_OBJECT_VERSION_V1.to_le_bytes());
        hash.update(self.binding.job_context.leaf_context_hash_v1(coord.call_index, coord.position).as_byte_slice());
        hash.update(self.binding.shape_profile.shape_profile_id().as_byte_slice());
        for n in [coord.call_index, coord.node_slot, coord.position, coord.tile_index, count, length] {
            hash.update(&n.to_le_bytes());
        }
        self.pending = Some(PalwPrefixTileStreamV1 { length, offset: 0, expected: opening.leaf_hash, hash });
        Ok(())
    }
    pub fn push_tile_bytes(&mut self, context: Hash64, ordinal: u64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.check()?;
        let pending = self.pending.as_ref().ok_or("prefix tile header missing")?;
        if context != self.context
            || ordinal != self.next_tile
            || offset != pending.offset
            || bytes.is_empty()
            || bytes.len() > PALW_PREFIX_STREAM_TILE_BYTES_V1
            || bytes.len() as u64 > u64::from(pending.length - pending.offset)
        {
            return Err("prefix tile byte move changes seed/order/offset or bounded remaining length".into());
        }
        let total = self.value_bytes.checked_add(bytes.len() as u64).ok_or("prefix cumulative bytes overflow")?;
        let pending = self.pending.as_mut().unwrap();
        pending.hash.update(bytes);
        pending.offset += bytes.len() as u32;
        self.digest.update(bytes);
        self.value_bytes = total;
        Ok(())
    }
    pub fn finish_tile(&mut self) -> Result<(), String> {
        self.check()?;
        let pending = self.pending.as_ref().ok_or("prefix tile header missing")?;
        if pending.offset != pending.length || pending.hash.finish() != pending.expected {
            return Err("prefix tile is incomplete or fails original wholeleaf hash".into());
        }
        self.next_tile += 1;
        self.pending = None;
        Ok(())
    }
    pub fn finish(&self) -> Result<Hash64, String> {
        self.check()?;
        if self.pending.is_some()
            || self.next_tile != self.total_tiles
            || self.digest.byte_count() != Some(136 + u128::from(self.value_bytes))
        {
            return Err("prefix stream is incomplete or has inconsistent digest counters".into());
        }
        Ok(crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(
            &self.binding.shape_profile.shape_profile_id(),
            self.prefix_tokens,
            &self.digest.finish(),
        ))
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwPrefixStreamCheckpointV1 {
        PalwPrefixStreamCheckpointV1 {
            context: self.context,
            next_tile: self.next_tile,
            value_bytes: self.value_bytes,
            digest: self.digest.clone(),
            pending: self.pending.clone(),
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwPrefixStreamCheckpointV1) -> Result<(), String> {
        if cp.pending.as_ref().is_some_and(|p| {
            p.offset > p.length
                || p.length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES
                || p.hash.byte_count() != Some(282 + u128::from(p.offset))
        }) {
            return Err("prefix checkpoint pending leaf counters invalid".into());
        }
        if cp.context != self.context
            || cp.next_tile > self.total_tiles
            || cp.value_bytes as u128 > self.total_tiles as u128 * crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES as u128
            || (cp.next_tile == self.total_tiles && cp.pending.is_some())
            || cp.digest.byte_count() != Some(136 + u128::from(cp.value_bytes))
        {
            return Err("prefix journal checkpoint changes seed/counter bounds".into());
        }
        self.next_tile = cp.next_tile;
        self.value_bytes = cp.value_bytes;
        self.digest = cp.digest.clone();
        self.pending = cp.pending.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_prefix_state_court_v1::{palw_prefix_kv_state_root_v1, tests::fixture};
    const CAP: u64 = 1 << 32;
    #[test]
    fn chunked_prefix_matches_full_authenticated_court_and_restores_each_move() {
        for inherited in [false, true] {
            let (binding, tiles, expected) = fixture(inherited);
            let context = Hash64::from_u64_word(91);
            let mut stream = PalwPrefixStateStreamV1::new(context, binding.committed_execution_root, &binding, 2, CAP).unwrap();
            assert!(stream.finish().is_err());
            for (ordinal, tile) in tiles.iter().enumerate() {
                stream.begin_tile(context, ordinal as u64, &tile.opening).unwrap();
                assert!(stream.finish_tile().is_err());
                let mut offset = 0;
                // Ragged moves exercise continuation compression boundaries independently of tile boundaries.
                for bytes in tile.preimage.values_le.chunks(37) {
                    stream.push_tile_bytes(context, ordinal as u64, offset, bytes).unwrap();
                    offset += bytes.len() as u32;
                    let cp = stream.checkpoint_v1();
                    let serialized = borsh::to_vec(&cp).unwrap();
                    assert!(serialized.len() < 1_024);
                    let cp = borsh::from_slice(&serialized).unwrap();
                    let mut restored =
                        PalwPrefixStateStreamV1::new(context, binding.committed_execution_root, &binding, 2, CAP).unwrap();
                    restored.restore_checkpoint_v1(&cp).unwrap();
                    assert_eq!(restored, stream);
                    stream = restored;
                }
                stream.finish_tile().unwrap();
            }
            assert_eq!(stream.finish(), Ok(expected));
            assert_eq!(stream.finish(), palw_prefix_kv_state_root_v1(&binding, 2, &tiles, CAP).map_err(str::to_string));
        }
    }
    #[test]
    fn tentative_corrupt_leaf_or_wrong_seed_cannot_finalize_a_prefix() {
        let (binding, tiles, _) = fixture(false);
        let context = Hash64::from_u64_word(92);
        let make = || PalwPrefixStateStreamV1::new(context, binding.committed_execution_root, &binding, 2, CAP).unwrap();
        let mut stream = make();
        assert!(stream.begin_tile(Hash64::default(), 0, &tiles[0].opening).is_err());
        assert!(stream.begin_tile(context, 1, &tiles[1].opening).is_err());
        assert_eq!(stream, make());
        stream.begin_tile(context, 0, &tiles[0].opening).unwrap();
        let before = stream.clone();
        assert!(stream.push_tile_bytes(context, 0, 1, &[0]).is_err());
        assert_eq!(stream, before);
        let mut bytes = tiles[0].preimage.values_le.clone();
        bytes[0] ^= 1;
        for (i, chunk) in bytes.chunks(PALW_PREFIX_STREAM_TILE_BYTES_V1).enumerate() {
            stream.push_tile_bytes(context, 0, (i * PALW_PREFIX_STREAM_TILE_BYTES_V1) as u32, chunk).unwrap();
        }
        assert!(stream.finish_tile().is_err());
        assert!(stream.finish().is_err());
        let mut forged = stream.checkpoint_v1();
        forged.value_bytes = u64::MAX;
        let before = stream.clone();
        assert!(stream.restore_checkpoint_v1(&forged).is_err());
        assert_eq!(stream, before);
    }
}
