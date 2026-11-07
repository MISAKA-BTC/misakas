//! Public sources for every non-recurrent Q36 resume-cache byte. Recurrence heads require
//! the owned zero-state replay; projection rows here authenticate exact claim commitments.
use crate::palw_prefix_family_state_v1::{PalwPrefixCacheUnitV1, PalwQwen36PrefixCachePlanV1};
use crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1;
use crate::palw_step::{PalwLayerKindV1, PalwStepNodeRoleV1, PalwStepOutLenV1, PalwStepTableV1, kernel_semantics_id_v1};
use crate::palw_step_leg::PalwStepBindingV2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwQ36PrefixSourceSliceV1 {
    pub node_slot: u32,
    pub position: u32,
    pub tile_len: u32,
    pub row_values: u64,
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwQ36PrefixPublicRowV1 {
    pub unit: PalwPrefixCacheUnitV1,
    pub offset: u64,
    pub length: u64,
    /// Empty only for a canonical pre-genesis convolution row, whose bytes are zero.
    pub sources: Vec<PalwQ36PrefixSourceSliceV1>,
}
impl PalwQ36PrefixPublicRowV1 {
    pub fn new(
        binding: &PalwStepBindingV2,
        prefix_tokens: u32,
        unit: PalwPrefixCacheUnitV1,
        offset: u64,
        length: u64,
    ) -> Result<Self, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| "the public prefix binding is not authentic")?;
        crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::Qwen36HybridCacheV1.check_profile(&binding.shape_profile)?;
        if prefix_tokens >= binding.job_context.declared_prefill_tokens
            || length == 0
            || length > 65_536
            || !offset.is_multiple_of(4)
            || !length.is_multiple_of(4)
        {
            return Err("the public prefix chunk is not a proper bounded aligned cache interval".into());
        }
        let p = &binding.shape_profile;
        let plan = PalwQwen36PrefixCachePlanV1::new(p, prefix_tokens)?;
        let (layer, position, table, indices, zero) = match unit {
            PalwPrefixCacheUnitV1::Key { layer, position } | PalwPrefixCacheUnitV1::Value { layer, position } => {
                if layer >= p.layer_count || p.layer_kind(layer) != PalwLayerKindV1::Attention || position >= prefix_tokens {
                    return Err("the prefix unit is not an attention cache row".into());
                }
                let role = if matches!(unit, PalwPrefixCacheUnitV1::Key { .. }) {
                    PalwStepNodeRoleV1::KCacheWrite
                } else {
                    PalwStepNodeRoleV1::VCacheWrite
                };
                let mut rows = p.attn_nodes.iter().enumerate().filter(|(_, n)| n.role == role);
                let (index, _) = rows.next().ok_or("the exact cache-write role is absent")?;
                if rows.next().is_some() {
                    return Err("the cache-write role is not unique".into());
                }
                (layer, position, PalwStepTableV1::Attn, vec![index], false)
            }
            PalwPrefixCacheUnitV1::Convolution { layer, age } => {
                if layer >= p.layer_count || p.layer_kind(layer) != PalwLayerKindV1::GatedDeltaNet || age >= p.gdn_conv_kernel {
                    return Err("the prefix unit is not a declared convolution history row".into());
                }
                let mut nodes = p
                    .gdn_nodes
                    .iter()
                    .filter(|n| n.kernel_semantics_id == kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_SSM_CONV));
                let n = nodes.next().ok_or("the canonical convolution is absent")?;
                if nodes.next().is_some() || n.input_refs.len() != 3 {
                    return Err("the convolution's q/k/v sources are not unique".into());
                }
                let relative = i64::from(prefix_tokens) - i64::from(p.gdn_conv_kernel) + i64::from(age);
                (
                    layer,
                    relative.max(0) as u32,
                    PalwStepTableV1::Gdn,
                    n.input_refs.iter().map(|r| usize::from(*r)).collect(),
                    relative < 0,
                )
            }
            PalwPrefixCacheUnitV1::GdnHead { .. } => {
                return Err("a recurrence matrix must be completed by owned genesis replay".into());
            }
        };
        // Find the canonical unit without allocating a full-context list. The unit identity
        // includes layer and age/position, so no alternative row can supply its bytes.
        let expected = match unit {
            PalwPrefixCacheUnitV1::Key { .. } | PalwPrefixCacheUnitV1::Value { .. } => {
                u64::from(p.attn_kv_heads) * u64::from(p.attn_head_dim) * 4
            }
            _ => {
                let keys = u64::from(crate::palw_step::palw_gdn_key_heads_v1(p).map_err(|e| e.to_string())?);
                (2 * keys * u64::from(p.gdn_head_k_dim) + u64::from(p.gdn_heads) * u64::from(p.gdn_head_v_dim)) * 4
            }
        };
        let end =
            offset.checked_add(length).filter(|end| *end <= expected).ok_or("the chunk exceeds its exact canonical cache row")?;
        let _ = plan; // Constructor checks overflow, exact geometry and every nonempty component.
        let nodes = if table == PalwStepTableV1::Attn { &p.attn_nodes } else { &p.gdn_nodes };
        let mut base = 0u64;
        let mut sources = Vec::with_capacity(3);
        for index in indices {
            let n = nodes.get(index).ok_or("a public cache source is outside its own table")?;
            let PalwStepOutLenV1::Fixed { elements } = n.out_len else {
                return Err("the cache source width is not fixed".into());
            };
            let width = u64::from(elements);
            let limit = base.checked_add(width).ok_or("the source row width overflows")?;
            let lo = (offset / 4).max(base);
            let hi = (end / 4).min(limit);
            if !zero && lo < hi {
                sources.push(PalwQ36PrefixSourceSliceV1 {
                    node_slot: p.global_node_slot(table, layer, index).ok_or("the source slot is absent")?,
                    position,
                    tile_len: n.tile_len,
                    row_values: width,
                    start: lo - base,
                    end: hi - base,
                });
            }
            base = limit;
        }
        if base.checked_mul(4) != Some(expected) {
            return Err("the source graph does not exactly cover its declared cache row".into());
        }
        Ok(Self { unit, offset, length, sources })
    }
    pub fn authenticate(
        &self,
        binding: &PalwStepBindingV2,
        prefix_tokens: u32,
        inputs: &[Vec<PalwPrefixKvTileV1>],
        maximum_leaves: u64,
    ) -> Result<Vec<u8>, String> {
        if *self != Self::new(binding, prefix_tokens, self.unit, self.offset, self.length)? || inputs.len() != self.sources.len() {
            return Err("the resumed public cache chunk changes its canonical plan or opening count".into());
        }
        if self.sources.is_empty() {
            return Ok(vec![0; self.length as usize]);
        }
        let mut bytes = Vec::with_capacity(self.length as usize);
        for (source, tiles) in self.sources.iter().zip(inputs) {
            let values = crate::palw_prefix_gdn_replay_v1::authenticated_slice(
                binding,
                source.node_slot,
                source.position,
                source.tile_len,
                source.row_values,
                (source.start, source.end),
                tiles,
                maximum_leaves,
            )?;
            bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        }
        if bytes.len() as u64 != self.length {
            return Err("authenticated sources do not cover the exact cache chunk".into());
        }
        Ok(bytes)
    }
}
