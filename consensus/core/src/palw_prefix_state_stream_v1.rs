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
}
/// Small journal checkpoint; immutable authenticated binding stays in the seed row.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixStreamCheckpointV1 {
    context: Hash64,
    next_tile: u64,
    value_bytes: u64,
    digest: LeafHashContinuationV1,
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
        // Every move must fit without trusting an owner-stated shorter length.
        if plan.key_tile_len as u64 * 4 > PALW_PREFIX_STREAM_TILE_BYTES_V1 as u64
            || plan.value_tile_len as u64 * 4 > PALW_PREFIX_STREAM_TILE_BYTES_V1 as u64
        {
            return Err("prefix stream tile needs a separately chunked authenticated-leaf court".into());
        }
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
        })
    }
    pub fn next_tile(&self) -> u64 {
        self.next_tile
    }
    pub fn total_tiles(&self) -> u64 {
        self.total_tiles
    }
    pub fn push(&mut self, context: Hash64, ordinal: u64, tile: &PalwPrefixKvTileV1) -> Result<(), String> {
        if context != self.context
            || ordinal != self.next_tile
            || self.next_tile >= self.total_tiles
            || tile.preimage.values_le.len() > PALW_PREFIX_STREAM_TILE_BYTES_V1
        {
            return Err("prefix tile changes owned identity/order or bounded move".into());
        }
        let plan = PalwPrefixKvPlanV1::of_profile(&self.binding.shape_profile, self.prefix_tokens).map_err(str::to_string)?;
        let bytes =
            palw_prefix_kv_authenticated_bytes_v1(&self.binding, &plan, ordinal, tile, self.maximum_leaves).map_err(str::to_string)?;
        let total = self.value_bytes.checked_add(bytes.len() as u64).ok_or("prefix cumulative bytes overflow")?;
        self.digest.update(bytes);
        self.value_bytes = total;
        self.next_tile += 1;
        Ok(())
    }
    pub fn finish(&self) -> Result<Hash64, String> {
        if self.next_tile != self.total_tiles || self.digest.byte_count() != Some(136 + u128::from(self.value_bytes)) {
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
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwPrefixStreamCheckpointV1) -> Result<(), String> {
        if cp.context != self.context
            || cp.next_tile > self.total_tiles
            || cp.digest.byte_count() != Some(136 + u128::from(cp.value_bytes))
        {
            return Err("prefix journal checkpoint changes seed/counter bounds".into());
        }
        self.next_tile = cp.next_tile;
        self.value_bytes = cp.value_bytes;
        self.digest = cp.digest.clone();
        Ok(())
    }
}
