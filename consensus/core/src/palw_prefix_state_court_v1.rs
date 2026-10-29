//! Public consistency court for a named dense integer prefix state.
//!
//! The job's root must name the K/V cache its own committed prefix writes describe.
//! This checks consistency, not that those writes are correct model computation: a lie in
//! their arithmetic remains a step-court question. No producer cache snapshot is trusted.

use crate::Hash64;
use crate::palw_step::{
    PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepOutLenV1, PalwStepTableV1,
};
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};

pub const PALW_A16_PREFIX_KV_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/qwen25-a16/prefix-kv-digest/v1";

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPrefixKvTileV1 {
    pub opening: PalwStepOpeningV1,
    pub preimage: PalwStepTileLeafV1,
}

/// A constant-size cursor into the canonical layer, K then V, position, tile walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPrefixKvPlanV1 {
    pub prefix_tokens: u32,
    pub layer_count: u16,
    pub key_node: u32,
    pub value_node: u32,
    pub key_tile_len: u32,
    pub value_tile_len: u32,
    pub width: u32,
    pub tile_count: u64,
}

impl PalwPrefixKvPlanV1 {
    /// Only the integer attention cache's published digest is defined here. A recurrence or
    /// auxiliary-state family needs its own state court, not reinterpretation as an A16 cache.
    pub fn of_profile(profile: &PalwShapeProfileV3, prefix_tokens: u32) -> Result<Self, &'static str> {
        profile.validate_shape().map_err(|_| "the prefix court needs a valid shape")?;
        if profile.lane != PalwStepLaneV1::Int32
            || profile.full_attention_interval != 1
            || profile.layer_count == 0
            || profile.kv_chunk_calls != 0
            || prefix_tokens == 0
        {
            return Err("the prefix court needs a dense integer attention cache without auxiliary recurrence");
        }
        let width = u32::from(profile.attn_kv_heads)
            .checked_mul(profile.attn_head_dim)
            .filter(|width| *width != 0)
            .ok_or("the prefix cache width overflows")?;
        let find = |role| -> Result<(u32, u32), &'static str> {
            let mut matching = profile.attn_nodes.iter().enumerate().filter(|(_, node)| node.role == role);
            let (index, node) = matching.next().ok_or("the prefix cache role is absent")?;
            if matching.next().is_some() || node.out_len != (PalwStepOutLenV1::Fixed { elements: width }) {
                return Err("the prefix cache role is not one exact cache row");
            }
            Ok((u32::try_from(index).map_err(|_| "the cache node index overflows")?, node.tile_len))
        };
        let (key_node, key_tile_len) = find(PalwStepNodeRoleV1::KCacheWrite)?;
        let (value_node, value_tile_len) = find(PalwStepNodeRoleV1::VCacheWrite)?;
        let per_position = u64::from(width.div_ceil(key_tile_len)) + u64::from(width.div_ceil(value_tile_len));
        let tile_count = u64::from(profile.layer_count)
            .checked_mul(u64::from(prefix_tokens))
            .and_then(|n| n.checked_mul(per_position))
            .ok_or("the prefix tile count overflows")?;
        Ok(Self {
            prefix_tokens,
            layer_count: profile.layer_count,
            key_node,
            value_node,
            key_tile_len,
            value_tile_len,
            width,
            tile_count,
        })
    }

    /// No full-context vector is allocated merely to discover the next public unit.
    pub fn coordinate(&self, profile: &PalwShapeProfileV3, ordinal: u64) -> Option<(PalwStepCoordinateV1, u32)> {
        if ordinal >= self.tile_count {
            return None;
        }
        let key_tiles = u64::from(self.width.div_ceil(self.key_tile_len));
        let value_tiles = u64::from(self.width.div_ceil(self.value_tile_len));
        let keys = u64::from(self.prefix_tokens).checked_mul(key_tiles)?;
        let values = u64::from(self.prefix_tokens).checked_mul(value_tiles)?;
        let layer_span = keys.checked_add(values)?;
        let layer = u16::try_from(ordinal / layer_span).ok()?;
        let local = ordinal % layer_span;
        let (node, tile_len, tiles, local) = if local < keys {
            (self.key_node, self.key_tile_len, key_tiles, local)
        } else {
            (self.value_node, self.value_tile_len, value_tiles, local - keys)
        };
        let tile = u32::try_from(local % tiles).ok()?;
        let start = tile.checked_mul(tile_len)?;
        Some((
            PalwStepCoordinateV1 {
                call_index: 0,
                node_slot: profile.global_node_slot(PalwStepTableV1::Attn, layer, node as usize)?,
                position: u32::try_from(local / tiles).ok()?,
                tile_index: tile,
            },
            (self.width - start).min(tile_len),
        ))
    }
}

/// Authenticate every prefix cache byte before hashing it. Omission, duplication, reordering,
/// a wrong coordinate, a ragged length or an unrelated opening cannot manufacture a verdict.
/// The caller authenticates the job and its named prefix against the accepted claim first.
pub fn palw_prefix_kv_state_root_v1(
    binding: &PalwStepBindingV2,
    prefix_tokens: u32,
    tiles: &[PalwPrefixKvTileV1],
    maximum_leaves: u64,
) -> Result<Hash64, &'static str> {
    if prefix_tokens >= binding.job_context.declared_prefill_tokens {
        return Err("the named state is not a proper prompt prefix");
    }
    palw_prefix_kv_export_state_root_v1(binding, prefix_tokens, tiles, maximum_leaves)
}

/// RFC0040 source-export court: a source may export its complete prefill state.
/// Unlike the consumer's proper-prefix court above, equality is meaningful here.
/// Every cache position must still be inside the source's actual committed work.
pub fn palw_prefix_kv_export_state_root_v1(
    binding: &PalwStepBindingV2,
    prefix_tokens: u32,
    tiles: &[PalwPrefixKvTileV1],
    maximum_leaves: u64,
) -> Result<Hash64, &'static str> {
    crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| "the prefix binding is unverified")?;
    if prefix_tokens > binding.job_context.declared_prefill_tokens {
        return Err("the exported state exceeds the source's committed prefill work");
    }
    if crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, maximum_leaves)
        .map_err(|_| "the prefix step count is not derivable")?
        != binding.step_leaf_count
    {
        return Err("the prefix binding carries a noncanonical step count");
    }
    let plan = PalwPrefixKvPlanV1::of_profile(&binding.shape_profile, prefix_tokens)?;
    if plan.tile_count != tiles.len() as u64 {
        return Err("the prefix cache tile set is incomplete");
    }
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut digest = blake2b_simd::Params::new().hash_length(64).key(PALW_A16_PREFIX_KV_DIGEST_DOMAIN_V1).to_state();
    digest.update(&u64::from(plan.layer_count).to_le_bytes());
    for (ordinal, tile) in tiles.iter().enumerate() {
        let values_le = palw_prefix_kv_authenticated_bytes_v1(binding, &plan, ordinal as u64, tile, maximum_leaves)?;
        digest.update(values_le);
    }
    let digest = Hash64::from_bytes(digest.finalize().as_bytes().try_into().expect("a 64-byte digest"));
    Ok(crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&profile_hash, prefix_tokens, &digest))
}

/// The staged court uses the same exact coordinate, byte count and claim-bound opening as
/// the finite proof. Only these authenticated bytes may enter an incremental prefix digest.
pub fn palw_prefix_kv_authenticated_bytes_v1<'a>(
    binding: &PalwStepBindingV2,
    plan: &PalwPrefixKvPlanV1,
    ordinal: u64,
    tile: &'a PalwPrefixKvTileV1,
    maximum_leaves: u64,
) -> Result<&'a [u8], &'static str> {
    if *plan != PalwPrefixKvPlanV1::of_profile(&binding.shape_profile, plan.prefix_tokens)? {
        return Err("the prefix plan is not canonical");
    }
    let profile_hash = binding.shape_profile.shape_profile_id();
    let (coord, values) = plan.coordinate(&binding.shape_profile, ordinal).ok_or("the prefix cache coordinate is absent")?;
    let expected_index = crate::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord)
        .ok_or("the prefix cache tile is not a canonical step")?;
    let preimage = &tile.preimage;
    if tile.opening.leaf_index != expected_index
        || preimage.coord != coord
        || preimage.version != crate::palw_step::PALW_STEP_OBJECT_VERSION_V1
        || preimage.value_count != values
        || preimage.values_le.len() as u64 != u64::from(values) * 4
    {
        return Err("the prefix cache tile is not the exact canonical preimage");
    }
    if crate::palw_step_leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, preimage) != tile.opening.leaf_hash
        || crate::palw_step_leg::step_opening_root_capped_v1(binding.step_leaf_count, &tile.opening, maximum_leaves)
            .map_err(|_| "the prefix cache path does not verify")?
            != binding.step_merkle_root
    {
        return Err("the prefix cache preimage is not committed by this claim");
    }
    Ok(&preimage.values_le)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use crate::palw_step_leg::{PalwStepMerkleTreeV1, binding_commitment_root_v1, step_tile_leaf_hash_ctx_v1};

    pub(crate) fn fixture(inherited: bool) -> (PalwStepBindingV2, Vec<PalwPrefixKvTileV1>, Hash64) {
        fixture_tokens(inherited, 2)
    }

    fn fixture_tokens(inherited: bool, tokens: u32) -> (PalwStepBindingV2, Vec<PalwPrefixKvTileV1>, Hash64) {
        let mut b = crate::palw_attempt_rules_v1::floor_binding_for_tests_v1(&Hash64::from_u64_word(0xF17), PalwPromptIdsFormV1::Flat);
        b.job_context.declared_prefill_tokens = 4;
        b.job_context.exact_decode_tokens = 2;
        if inherited {
            b.job_context.version = crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V3_INHERITED;
            b.job_context.job_nullifier = Hash64::from_u64_word(2);
            b.job_context.assignment_id = Hash64::from_u64_word(0xBAD17);
        }
        b.step_leaf_count = crate::palw_step::step_leaf_count(&b.shape_profile, &b.job_context).unwrap();
        b.activation_leg_root = crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&b.job_context);
        let plan = PalwPrefixKvPlanV1::of_profile(&b.shape_profile, tokens).unwrap();
        let mut hashes = vec![Hash64::default(); b.step_leaf_count as usize];
        let mut tiles = Vec::new();
        let mut digest = blake2b_simd::Params::new().hash_length(64).key(PALW_A16_PREFIX_KV_DIGEST_DOMAIN_V1).to_state();
        digest.update(&u64::from(plan.layer_count).to_le_bytes());
        for ordinal in 0..plan.tile_count {
            let (coord, value_count) = plan.coordinate(&b.shape_profile, ordinal).unwrap();
            let values_le: Vec<u8> =
                (0..value_count).flat_map(|value| ((ordinal as i32) * 256 - value as i32).to_le_bytes()).collect();
            digest.update(&values_le);
            let preimage =
                PalwStepTileLeafV1 { version: crate::palw_step::PALW_STEP_OBJECT_VERSION_V1, coord, value_count, values_le };
            let leaf_index = crate::palw_step::canonical_step_leaf_index(&b.shape_profile, &b.job_context, &coord).unwrap();
            let leaf_hash = step_tile_leaf_hash_ctx_v1(&b.job_context, &b.shape_profile.shape_profile_id(), &preimage);
            hashes[leaf_index as usize] = leaf_hash;
            tiles.push(PalwPrefixKvTileV1 { opening: PalwStepOpeningV1 { leaf_index, leaf_hash, siblings: Vec::new() }, preimage });
        }
        let tree = PalwStepMerkleTreeV1::build_capped_v1(&hashes, 1 << 32).unwrap();
        b.step_merkle_root = tree.root();
        for tile in &mut tiles {
            tile.opening.siblings = tree.path_v1(tile.opening.leaf_index as usize).unwrap();
        }
        b.committed_execution_root = binding_commitment_root_v1(&b);
        let digest = Hash64::from_bytes(digest.finalize().as_bytes().try_into().unwrap());
        let expected = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&b.shape_profile.shape_profile_id(), tokens, &digest);
        (b, tiles, expected)
    }

    #[test]
    fn source_can_export_complete_prefill_without_reinterpreting_consumer_court() {
        let (binding, tiles, root) = fixture_tokens(false, 4);
        assert_eq!(palw_prefix_kv_export_state_root_v1(&binding, 4, &tiles, 1 << 32), Ok(root));
        assert!(palw_prefix_kv_state_root_v1(&binding, 4, &tiles, 1 << 32).is_err());
        assert!(palw_prefix_kv_export_state_root_v1(&binding, 5, &tiles, 1 << 32).is_err());
        assert!(palw_prefix_kv_export_state_root_v1(&binding, 4, &tiles[..tiles.len()-1], 1 << 32).is_err());
    }

    #[test]
    fn every_committed_prefix_byte_is_required_in_both_receipt_contexts() {
        for inherited in [false, true] {
            let (b, tiles, root) = fixture(inherited);
            assert_eq!(palw_prefix_kv_state_root_v1(&b, 2, &tiles, 1 << 32), Ok(root));
            assert_ne!(root, Hash64::from_u64_word(0xBAD17));
            assert!(palw_prefix_kv_state_root_v1(&b, 4, &tiles, 1 << 32).is_err());
            assert!(palw_prefix_kv_state_root_v1(&b, 2, &tiles[..tiles.len() - 1], 1 << 32).is_err());
            for mutation in 0..6 {
                let mut forged = tiles.clone();
                match mutation {
                    0 => forged.swap(0, 1),
                    1 => forged[0] = forged[1].clone(),
                    2 => forged[0].preimage.values_le[0] ^= 1,
                    3 => forged[0].preimage.coord.position += 1,
                    4 => forged[0].preimage.value_count += 1,
                    _ => forged[0].opening.leaf_index += 1,
                }
                assert!(palw_prefix_kv_state_root_v1(&b, 2, &forged, 1 << 32).is_err(), "mutation {mutation}");
            }
        }
    }
}
