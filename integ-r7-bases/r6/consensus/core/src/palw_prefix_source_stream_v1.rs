//! One canonical public source slice, transported through bounded chunks of its ORIGINAL
//! step-leaf preimage. Only overlapping lanes are retained; neither bytes nor a result are
//! usable until each whole old leaf hash matches the claim's authenticated opening.
use crate::Hash64;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index};
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, step_opening_root_capped_v1};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwPrefixSourceRangeV1 {
    pub node_slot: u32,
    pub position: u32,
    pub tile_len: u32,
    pub width: u64,
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PendingV1 {
    length: u32,
    offset: u32,
    expected: Hash64,
    hash: LeafHashContinuationV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwPrefixSourceStreamV1 {
    context: Hash64,
    execution_root: Hash64,
    source: PalwPrefixSourceRangeV1,
    next_tile: u64,
    values: Vec<i32>,
    pending: Option<PendingV1>,
}
impl PalwPrefixSourceStreamV1 {
    pub fn new(context: Hash64, execution_root: Hash64, source: PalwPrefixSourceRangeV1) -> Result<Self, String> {
        let s = Self {
            context,
            execution_root,
            source,
            next_tile: if source.tile_len == 0 { 0 } else { source.start / u64::from(source.tile_len) },
            values: vec![],
            pending: None,
        };
        s.check()?;
        Ok(s)
    }
    pub fn matches_v1(&self, context: Hash64, execution_root: Hash64, source: PalwPrefixSourceRangeV1) -> bool {
        self.context == context && self.execution_root == execution_root && self.source == source
    }
    pub fn source_v1(&self) -> PalwPrefixSourceRangeV1 {
        self.source
    }
    fn last(&self) -> u64 {
        (self.source.end - 1) / u64::from(self.source.tile_len)
    }
    pub fn complete(&self) -> bool {
        self.next_tile > self.last() && self.pending.is_none()
    }
    pub fn cursor_v1(&self) -> Option<(PalwStepCoordinateV1, Option<(u32, u32)>)> {
        if self.complete() {
            return None;
        }
        Some((
            PalwStepCoordinateV1 {
                call_index: 0,
                node_slot: self.source.node_slot,
                position: self.source.position,
                tile_index: self.next_tile as u32,
            },
            self.pending.as_ref().map(|p| (p.length, p.offset)),
        ))
    }
    pub fn check(&self) -> Result<(), String> {
        let s = self.source;
        if s.tile_len == 0
            || s.tile_len > crate::palw_step::PALW_STEP_MAX_TILE_LEN
            || s.start >= s.end
            || s.end > s.width
            || s.end - s.start > 1 << 17
            || s.width > u64::from(u32::MAX)
            || self.next_tile < s.start / u64::from(s.tile_len)
            || self.next_tile > self.last() + 1
        {
            return Err("streamed source changes bounded canonical range/cursor".into());
        }
        let base = self.next_tile * u64::from(s.tile_len);
        let consumed = if let Some(p) = &self.pending {
            if self.next_tile > self.last()
                || p.length
                    != u32::try_from(u64::from(s.tile_len).min(s.width - base) * 4).map_err(|_| "source leaf byte length overflow")?
                || p.offset > p.length
                || p.offset % 4 != 0
                || p.hash.byte_count() != Some(282 + u128::from(p.offset))
            {
                return Err("streamed old leaf counters/hash length inconsistent".into());
            }
            base + u64::from(p.offset / 4)
        } else {
            base
        };
        let count = consumed.min(s.end).saturating_sub(s.start);
        if self.values.len() as u64 != count {
            return Err("streamed source retained lane count inconsistent".into());
        }
        Ok(())
    }
    pub fn begin_tile(
        &mut self,
        context: Hash64,
        binding: &PalwStepBindingV2,
        opening: &PalwStepOpeningV1,
        maximum_leaves: u64,
    ) -> Result<(), String> {
        self.check()?;
        if context != self.context
            || binding.committed_execution_root != self.execution_root
            || self.pending.is_some()
            || self.complete()
        {
            return Err("source opening changes owned context or phase".into());
        }
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        let (coordinate, _) = self.cursor_v1().ok_or("source complete")?;
        let index =
            canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coordinate).ok_or("source coordinate absent")?;
        if opening.leaf_index != index
            || step_opening_root_capped_v1(binding.step_leaf_count, opening, maximum_leaves).map_err(|e| e.to_string())?
                != binding.step_merkle_root
        {
            return Err("source does not open original claim step leaf".into());
        }
        let count =
            u32::try_from(u64::from(self.source.tile_len).min(self.source.width - self.next_tile * u64::from(self.source.tile_len)))
                .map_err(|_| "source count overflow")?;
        let length = count.checked_mul(4).ok_or("source byte length overflow")?;
        let mut hash = LeafHashContinuationV1::new(crate::palw_step_leg::PALW_STEP_LEG_DOMAIN_TILE_LEAF);
        hash.update(&crate::palw_step::PALW_STEP_OBJECT_VERSION_V1.to_le_bytes());
        hash.update(binding.job_context.leaf_context_hash_v1(coordinate.call_index, coordinate.position).as_byte_slice());
        hash.update(binding.shape_profile.shape_profile_id().as_byte_slice());
        for n in [coordinate.call_index, coordinate.node_slot, coordinate.position, coordinate.tile_index, count, length] {
            hash.update(&n.to_le_bytes());
        }
        self.pending = Some(PendingV1 { length, offset: 0, expected: opening.leaf_hash, hash });
        self.check()
    }
    pub fn push_part(&mut self, context: Hash64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.check()?;
        let p = self.pending.as_ref().ok_or("source old leaf opening missing")?;
        if context != self.context
            || offset != p.offset
            || bytes.is_empty()
            || bytes.len() > 65_536
            || !bytes.len().is_multiple_of(4)
            || bytes.len() as u64 > u64::from(p.length - p.offset)
        {
            return Err("source chunk changes bounded contiguous old preimage interval".into());
        }
        let base = self.next_tile * u64::from(self.source.tile_len) + u64::from(offset / 4);
        for (i, b) in bytes.chunks_exact(4).enumerate() {
            let lane = base + i as u64;
            if self.source.start <= lane && lane < self.source.end {
                self.values.push(i32::from_le_bytes(b.try_into().expect("aligned lane")));
            }
        }
        let p = self.pending.as_mut().expect("checked pending source");
        p.hash.update(bytes);
        p.offset += bytes.len() as u32;
        self.check()
    }
    pub fn finish_tile(&mut self, context: Hash64) -> Result<(), String> {
        self.check()?;
        let p = self.pending.as_ref().ok_or("source old leaf opening missing")?;
        if context != self.context || p.offset != p.length || p.hash.finish() != p.expected {
            return Err("source original whole leaf has not authenticated".into());
        }
        self.pending = None;
        self.next_tile += 1;
        self.check()
    }
    pub fn authenticated_values_v1(&self) -> Result<&[i32], String> {
        self.check()?;
        if !self.complete() {
            return Err("source original leaves are incomplete".into());
        }
        Ok(&self.values)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slice_is_usable_only_after_whole_original_leaf_eof_and_survives_restart() {
        let (b, tiles, _) = crate::palw_prefix_state_court_v1::tests::fixture(false);
        let tile = &tiles[0];
        let count = tile.preimage.value_count;
        assert!(count > 1);
        let range = PalwPrefixSourceRangeV1 {
            node_slot: tile.preimage.coord.node_slot,
            position: tile.preimage.coord.position,
            tile_len: count,
            width: u64::from(count),
            start: 1,
            end: u64::from(count),
        };
        let context = Hash64::from_u64_word(0x171967);
        let mut s = PalwPrefixSourceStreamV1::new(context, b.committed_execution_root, range).unwrap();
        s.begin_tile(context, &b, &tile.opening, 1 << 32).unwrap();
        assert!(s.authenticated_values_v1().is_err());
        let before = s.clone();
        assert!(s.push_part(context, 4, &tile.preimage.values_le).is_err());
        assert_eq!(s, before);
        for (i, bytes) in tile.preimage.values_le.chunks(20).enumerate() {
            s.push_part(context, i as u32 * 20, bytes).unwrap();
            s = borsh::from_slice(&borsh::to_vec(&s).unwrap()).unwrap();
            assert!(s.authenticated_values_v1().is_err());
        }
        s.finish_tile(context).unwrap();
        let expected: Vec<_> =
            tile.preimage.values_le[4..].chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).collect();
        assert_eq!(s.authenticated_values_v1().unwrap(), expected);
        let mut bad = PalwPrefixSourceStreamV1::new(context, b.committed_execution_root, range).unwrap();
        bad.begin_tile(context, &b, &tile.opening, 1 << 32).unwrap();
        let mut forged = tile.preimage.values_le.clone();
        forged[0] ^= 1;
        bad.push_part(context, 0, &forged).unwrap();
        assert!(bad.finish_tile(context).is_err());
        assert!(bad.authenticated_values_v1().is_err());
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_prefix_source_stream_v1.rs"]
mod native_resource_inventory_v1;
