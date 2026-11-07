//! One owned, genesis-anchored Q36 prefix head, reconstructed from public committed inputs.
//! This is a semantic transition for a typed staged certificate, not a standalone conviction.
//! The surrounding court owns the initial zero state and every transition; a serialized state
//! supplied by a challenger is never an admissible initial state. Full family certificates must
//! additionally cover all heads, attention KV and the oldest-first convolution window.

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_base0_a16::A16QuantParams;
use crate::palw_class_semantics_v1::PalwPrefixStateRelationV1;
use crate::palw_freeprompt_v3::PalwFreePromptJobV3;
use crate::palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1};
use crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1;
use crate::palw_qwen36_ops::{Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_gdn_step};
use crate::palw_step::{
    PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepLaneV1, PalwStepOutLenV1, PalwStepTableV1,
    canonical_step_leaf_index, kernel_semantics_id_v1,
};
use crate::palw_step_leg::{PalwStepBindingV2, step_opening_root_capped_v1, step_tile_leaf_hash_ctx_v1};
use crate::palw_step_refute::{KDESC_Q36_GDN_STEP, PalwWeightOracleV1, qwen36_gdn_slice_v1};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixGdnHeadReplayV1 {
    claim: Hash64,
    execution_root: Hash64,
    job_id: Hash64,
    layer: u16,
    head: u32,
    prefix_tokens: u32,
    next_position: u32,
    node_slot: u32,
    d_k: u32,
    d_v: u32,
    maximum_working_set_bytes: u64,
    parameters: [u8; 68],
    state: Vec<i32>,
}

impl PalwPrefixGdnHeadReplayV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        layer: u16,
        head: u32,
        parameter_openings: &[PalwArtifactOpeningV1],
        maximum_working_set_bytes: u64,
    ) -> Result<Self, String> {
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        let row = target
            .registered_semantics
            .as_ref()
            .filter(|r| r.matches_context(&binding.job_context))
            .ok_or("the prefix has no immutable family-state statement")?;
        if row.prefix_state_relation != Some(PalwPrefixStateRelationV1::Qwen36HybridCacheV1) {
            return Err("the claim did not bind the Q36 hybrid cache relation".into());
        }
        let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
        let profile = &binding.shape_profile;
        let (slot, node, widths) = head_geometry(profile, layer, head)?;
        let words = u64::from(profile.gdn_head_k_dim)
            .checked_mul(u64::from(profile.gdn_head_v_dim))
            .ok_or("the GDN state length overflows")?;
        let bytes = words.checked_mul(4).ok_or("the GDN state bytes overflow")?;
        let scratch_words = u64::from(profile.gdn_head_k_dim)
            .checked_mul(2)
            .and_then(|n| u64::from(profile.gdn_head_v_dim).checked_mul(3).and_then(|v| n.checked_add(v)))
            .and_then(|n| n.checked_add(2))
            .ok_or("the input scratch length overflows")?;
        let working = bytes
            .checked_mul(2)
            .and_then(|n| scratch_words.checked_mul(4).and_then(|scratch| n.checked_add(scratch)))
            .and_then(|n| n.checked_add(256))
            .ok_or("the transition working set overflows")?;
        if working > maximum_working_set_bytes {
            return Err("this head needs a paged owned-state court beyond the granted resident budget".into());
        }
        let words = usize::try_from(words).map_err(|_| "the head state does not fit this host's indexing")?;
        if named.prefix_tokens >= binding.job_context.declared_prefill_tokens {
            return Err("the named GDN state is not a proper prefix".into());
        }
        qwen36_gdn_slice_v1(profile, node, &widths, head).ok_or("the GDN head slices are not canonical")?;
        let proven = PalwProvenOperandsV1::from_openings_v1(parameter_openings, target.artifact_root).map_err(|e| e.to_string())?;
        let first = head
            .checked_mul(4)
            .and_then(|n| n.checked_mul(A16QuantParams::WIRE_BYTES as u32))
            .ok_or("the head parameter offset overflows")?;
        let bytes = proven
            .operand_bytes(&node.weight_name, Some(layer), first, 68)
            .ok_or("the exact four registered head parameter triples are not proven")?;
        let parameters: [u8; 68] = bytes.try_into().map_err(|_| "the registered head parameter length is not68")?;
        for p in parameters.chunks_exact(A16QuantParams::WIRE_BYTES) {
            A16QuantParams::from_wire(p).map_err(|_| "a registered head parameter is outside the integer kernel domain")?;
        }
        Ok(Self {
            claim: target.claim_id,
            execution_root: target.execution_root,
            job_id: binding.job_context.job_id,
            layer,
            head,
            prefix_tokens: named.prefix_tokens,
            next_position: 0,
            node_slot: slot,
            d_k: profile.gdn_head_k_dim,
            d_v: profile.gdn_head_v_dim,
            maximum_working_set_bytes,
            parameters,
            state: vec![0; words],
        })
    }
    pub(crate) fn matches_contract_v1(
        &self,
        claim: Hash64,
        binding: &PalwStepBindingV2,
        layer: u16,
        head: u32,
        prefix_tokens: u32,
    ) -> bool {
        self.claim == claim
            && self.execution_root == binding.committed_execution_root
            && self.job_id == binding.job_context.job_id
            && self.layer == layer
            && self.head == head
            && self.prefix_tokens == prefix_tokens
            && self.d_k == binding.shape_profile.gdn_head_k_dim
            && self.d_v == binding.shape_profile.gdn_head_v_dim
            && u64::from(self.d_k) * u64::from(self.d_v) == self.state.len() as u64
    }
    pub fn next_position(&self) -> u32 {
        self.next_position
    }
    pub fn resident_state_bytes(&self) -> u64 {
        self.state.len() as u64 * 4
    }
    /// Exactly five ordered, canonical sliced input rows at this next position. Each row's
    /// complete covering tiles authenticate against this claim, never an independent replay.
    pub fn advance(
        &mut self,
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        inputs: &[Vec<PalwPrefixKvTileV1>],
        maximum_leaves: u64,
    ) -> Result<(), String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| "the resumed prefix binding does not authenticate")?;
        if target.claim_id != self.claim
            || target.execution_root != self.execution_root
            || binding.committed_execution_root != self.execution_root
            || binding.job_context.job_id != self.job_id
            || self.next_position >= self.prefix_tokens
            || inputs.len() != 5
            || self.resident_state_bytes() > self.maximum_working_set_bytes
            || u64::from(self.d_k) * u64::from(self.d_v) != self.state.len() as u64
        {
            return Err("the GDN continuation changes claim/context, is complete, or exceeds its owned budget".into());
        }
        let profile = &binding.shape_profile;
        let (slot, node, widths) = head_geometry(profile, self.layer, self.head)?;
        if slot != self.node_slot || profile.gdn_head_k_dim != self.d_k || profile.gdn_head_v_dim != self.d_v {
            return Err("the GDN continuation's fixed head geometry changed".into());
        }
        let ranges = qwen36_gdn_slice_v1(profile, node, &widths, self.head).ok_or("the head slices are not canonical")?;
        let mut rows = Vec::with_capacity(5);
        for ordinal in 0..5 {
            let reference = usize::from(node.input_refs[ordinal]);
            let source = &profile.gdn_nodes[reference];
            let source_slot =
                profile.global_node_slot(PalwStepTableV1::Gdn, self.layer, reference).ok_or("the source slot is absent")?;
            rows.push(authenticated_slice(
                binding,
                source_slot,
                self.next_position,
                source.tile_len,
                widths[ordinal],
                ranges[ordinal],
                &inputs[ordinal],
                maximum_leaves,
            )?);
        }
        self.advance_authenticated_rows_v1(target, binding, &rows)
    }
    /// Only the owning family court calls this after every canonical source leaf has
    /// authenticated through the original-leaf stream. Not an externally admissible state.
    pub(crate) fn advance_authenticated_rows_v1(
        &mut self,
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        rows: &[Vec<i32>],
    ) -> Result<(), String> {
        if !self.matches_contract_v1(target.claim_id, binding, self.layer, self.head, self.prefix_tokens)
            || target.execution_root != self.execution_root
            || self.next_position >= self.prefix_tokens
            || rows.len() != 5
            || rows[0].len() != self.d_k as usize
            || rows[1].len() != self.d_v as usize
            || rows[2].len() != self.d_k as usize
            || rows[3].len() != 1
            || rows[4].len() != 1
            || self.resident_state_bytes() > self.maximum_working_set_bytes
        {
            return Err("owned authenticated head rows change context, geometry or phase".into());
        }
        let parse = |n: usize| {
            A16QuantParams::from_wire(&self.parameters[n * 17..(n + 1) * 17])
                .map_err(|_| "the owned registered parameters changed".to_string())
        };
        let parameters = Qwen36GdnParamsV1 { read: parse(0)?, delta: parse(1)?, write_shift: parse(2)?.zero as i32, out: parse(3)? };
        let mut state = Qwen36GdnStateV1 { d_k: self.d_k as usize, d_v: self.d_v as usize, s: self.state.clone() };
        q36_gdn_step(&mut state, &rows[0], &rows[1], &rows[2], i64::from(rows[3][0]), i64::from(rows[4][0]), parameters)
            .map_err(|e| format!("the authenticated GDN position is outside its kernel domain: {e:?}"))?;
        self.state = state.s;
        self.next_position += 1;
        Ok(())
    }
    /// A completed head's exact row-major state. The family digest consumes every head in
    /// layer/head order and still owes convolution history and any attention KV units.
    pub fn completed_matrix(&self) -> Result<&[i32], String> {
        if self.next_position != self.prefix_tokens {
            return Err("the prefix head is incomplete".into());
        }
        Ok(&self.state)
    }
}

pub(crate) fn head_geometry(
    profile: &PalwShapeProfileV3,
    layer: u16,
    head: u32,
) -> Result<(u32, &crate::palw_step::PalwStepNodeV1, Vec<u64>), String> {
    profile.validate_shape().map_err(|e| e.to_string())?;
    if profile.lane != PalwStepLaneV1::Int32
        || layer >= profile.layer_count
        || profile.layer_kind(layer) != PalwLayerKindV1::GatedDeltaNet
        || head >= u32::from(profile.gdn_heads)
        || profile.gdn_head_k_dim == 0
        || profile.gdn_head_v_dim == 0
    {
        return Err("the selected prefix head is not a declared integer GDN head".into());
    }
    let mut matching =
        profile.gdn_nodes.iter().enumerate().filter(|(_, n)| n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_GDN_STEP));
    let Some((index, node)) = matching.next() else { return Err("the registered GDN kernel is absent".into()) };
    if matching.next().is_some() || node.input_refs.len() != 5 {
        return Err("the GDN kernel is not unique with five inputs".into());
    }
    let mut widths = Vec::new();
    for reference in &node.input_refs {
        let source = profile.gdn_nodes.get(usize::from(*reference)).ok_or("a GDN input is not a node in its own table")?;
        let PalwStepOutLenV1::Fixed { elements } = source.out_len else { return Err("a GDN prefix input width is not fixed".into()) };
        widths.push(u64::from(elements));
    }
    let slot = profile.global_node_slot(PalwStepTableV1::Gdn, layer, index).ok_or("the GDN head slot is absent")?;
    Ok((slot, node, widths))
}

pub(crate) fn authenticated_slice(
    binding: &PalwStepBindingV2,
    node_slot: u32,
    position: u32,
    tile_len: u32,
    width: u64,
    range: (u64, u64),
    tiles: &[PalwPrefixKvTileV1],
    maximum_leaves: u64,
) -> Result<Vec<i32>, String> {
    let (start, end) = range;
    if tile_len == 0 || start >= end || end > width {
        return Err("the GDN input slice is outside its exact row".into());
    }
    let first = start / u64::from(tile_len);
    let last = (end - 1) / u64::from(tile_len);
    if tiles.len() as u64 != last - first + 1 {
        return Err("the public input does not carry exactly its covering tiles".into());
    }
    let mut row = Vec::with_capacity(usize::try_from(end - start).map_err(|_| "slice length overflows")?);
    for (ordinal, tile) in tiles.iter().enumerate() {
        let index = first + ordinal as u64;
        let coordinate = PalwStepCoordinateV1 {
            call_index: 0,
            node_slot,
            position,
            tile_index: u32::try_from(index).map_err(|_| "tile index overflows")?,
        };
        let leaf = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coordinate)
            .ok_or("the public input tile is not a canonical step")?;
        let values = u64::from(tile_len).min(width - index * u64::from(tile_len));
        if tile.preimage.coord != coordinate
            || tile.preimage.version != crate::palw_step::PALW_STEP_OBJECT_VERSION_V1
            || u64::from(tile.preimage.value_count) != values
            || tile.preimage.values_le.len() as u64 != values * 4
            || tile.opening.leaf_index != leaf
            || step_tile_leaf_hash_ctx_v1(&binding.job_context, &binding.shape_profile.shape_profile_id(), &tile.preimage)
                != tile.opening.leaf_hash
            || step_opening_root_capped_v1(binding.step_leaf_count, &tile.opening, maximum_leaves).map_err(|e| e.to_string())?
                != binding.step_merkle_root
        {
            return Err("the prefix input tile does not open the claim's exact canonical committed row".into());
        }
        let base = index * u64::from(tile_len);
        for lane in start.max(base)..end.min(base + values) {
            let at = usize::try_from((lane - base) * 4).map_err(|_| "the input byte offset overflows")?;
            row.push(i32::from_le_bytes(tile.preimage.values_le[at..at + 4].try_into().expect("a validated four-byte lane")));
        }
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_slice_uses_only_exact_claim_tiles_and_refuses_reordered_or_forged_rows() {
        let (binding, tiles, _) = crate::palw_prefix_state_court_v1::tests::fixture(false);
        let tile = &tiles[0];
        let coordinate = tile.preimage.coord;
        let width = u64::from(tile.preimage.value_count);
        assert!(width >= 2);
        let expected: Vec<i32> =
            tile.preimage.values_le[4..8].chunks_exact(4).map(|bytes| i32::from_le_bytes(bytes.try_into().unwrap())).collect();
        assert_eq!(
            authenticated_slice(
                &binding,
                coordinate.node_slot,
                coordinate.position,
                tile.preimage.value_count,
                width,
                (1, 2),
                std::slice::from_ref(tile),
                1 << 32
            )
            .unwrap(),
            expected
        );
        let mut forged = tile.clone();
        forged.preimage.values_le[0] ^= 1;
        assert!(
            authenticated_slice(
                &binding,
                coordinate.node_slot,
                coordinate.position,
                tile.preimage.value_count,
                width,
                (1, 2),
                &[forged],
                1 << 32
            )
            .is_err()
        );
        assert!(
            authenticated_slice(
                &binding,
                coordinate.node_slot,
                coordinate.position,
                tile.preimage.value_count,
                width,
                (1, 2),
                &[tile.clone(), tile.clone()],
                1 << 32
            )
            .is_err()
        );
        assert!(
            authenticated_slice(
                &binding,
                coordinate.node_slot,
                coordinate.position + 1,
                tile.preimage.value_count,
                width,
                (1, 2),
                std::slice::from_ref(tile),
                1 << 32
            )
            .is_err()
        );
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_prefix_gdn_replay_v1.rs"]
mod native_resource_inventory_v1;
