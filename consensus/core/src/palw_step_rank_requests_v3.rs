//! Native coordinate-rank scratch admitted from actual active private terms.
//! Historical index calls and cached index calls share exactly one rank body.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::mem::size_of;

pub(super) enum LeafShapeTermV3 {
    Fixed { contribution: u128, is_logits: bool },
    Kv { term: PalwKvTermV1, is_logits: bool },
}
/// The constructor and request count share table order, multiplicity and zero
/// term exclusions; no independent guess about the private KvTerm layout.
pub(super) fn visit_leaf_shape_terms_v3(profile: &PalwShapeProfileV3, visits: &mut u64, mut visit: impl FnMut(LeafShapeTermV3)) {
    let attn_layers = attention_layer_count_v1(profile);
    let gdn_layers = (profile.layer_count as u128).saturating_sub(attn_layers);
    for (table, weight, is_logits) in [
        (&profile.pre_nodes, 1u128, false),
        (&profile.gdn_nodes, gdn_layers, false),
        (&profile.attn_nodes, attn_layers, false),
        (&profile.post_nodes, 1u128, true),
    ] {
        for node in table {
            *visits += 1;
            let tile = node.tile_len as u128;
            match node.out_len {
                PalwStepOutLenV1::Fixed { elements } => visit(LeafShapeTermV3::Fixed {
                    contribution: weight.saturating_mul(tiles_u128(elements as u128, tile)),
                    is_logits,
                }),
                PalwStepOutLenV1::KvScaled { multiplier } => {
                    if weight == 0 || tile == 0 {
                        continue;
                    }
                    visit(LeafShapeTermV3::Kv {
                        term: PalwKvTermV1 { multiplier: multiplier as u128, tile_len: tile, weight },
                        is_logits,
                    });
                }
            }
        }
    }
}
pub(super) fn fill_leaf_shape_v3(shape: &mut PalwLeafShapeV1, profile: &PalwShapeProfileV3, visits: &mut u64) {
    visit_leaf_shape_terms_v3(profile, visits, |term| match term {
        LeafShapeTermV3::Fixed { contribution, is_logits } => {
            if is_logits {
                shape.logits_const = shape.logits_const.saturating_add(contribution);
            } else {
                shape.body_const = shape.body_const.saturating_add(contribution);
            }
        }
        LeafShapeTermV3::Kv { term, is_logits } => {
            if is_logits {
                shape.logits_kv.push(term);
            } else {
                shape.body_kv.push(term);
            }
        }
    });
}
fn term_slots(count: u64) -> Result<u64, NativeCoverageV1> {
    count
        .checked_mul(size_of::<PalwKvTermV1>() as u64)
        .ok_or_else(|| NativeCoverageV1::overflow::<PalwKvTermV1>("native coordinate rank terms"))
}
/// Cached shape arrays drop before their owner. Borrowed profile/context are
/// the exact immutable native inputs, not a second registered model/profile.
pub struct PalwCanonicalStepRankV3<'a> {
    shape: PalwLeafShapeV1,
    _lease: Option<NativeDemandLeaseV1>,
    profile: &'a PalwShapeProfileV3,
    context: &'a PalwJobContextV2,
}
impl<'a> PalwCanonicalStepRankV3<'a> {
    pub fn new_v3(profile: &'a PalwShapeProfileV3, context: &'a PalwJobContextV2) -> Result<Self, NativeCoverageV1> {
        let (mut body, mut logits) = (Some(0u64), Some(0u64));
        visit_leaf_shape_terms_v3(profile, &mut 0, |term| {
            if let LeafShapeTermV3::Kv { is_logits, .. } = term {
                let count = if is_logits { &mut logits } else { &mut body };
                *count = count.and_then(|n| n.checked_add(1));
            }
        });
        let overflow = || NativeCoverageV1::overflow::<PalwKvTermV1>("native coordinate rank count");
        let bytes =
            term_slots(body.ok_or_else(overflow)?)?.checked_add(term_slots(logits.ok_or_else(overflow)?)?).ok_or_else(overflow)?;
        let lease = reserve_native_requests_v1(bytes)?;
        let mut shape = PalwLeafShapeV1::default();
        let capacity = |why: &str| NativeCoverageV1::unsupported::<PalwKvTermV1>("native coordinate rank terms", why);
        shape
            .body_kv
            .try_reserve_exact(usize::try_from(body.ok_or_else(overflow)?).map_err(|_| overflow())?)
            .map_err(|_| capacity("body rank term allocation refused"))?;
        shape
            .logits_kv
            .try_reserve_exact(usize::try_from(logits.ok_or_else(overflow)?).map_err(|_| overflow())?)
            .map_err(|_| capacity("logits rank term allocation refused"))?;
        fill_leaf_shape_v3(&mut shape, profile, &mut 0);
        let actual = term_slots(shape.body_kv.capacity() as u64)?
            .checked_add(term_slots(shape.logits_kv.capacity() as u64)?)
            .ok_or_else(overflow)?;
        if actual > bytes {
            return Err(capacity("rank arrays exceed admitted exact capacity"));
        }
        Ok(Self { shape, _lease: lease, profile, context })
    }
    /// No native allocation: uses the admitted arrays for this context's rank.
    pub fn index_v3(&self, coordinate: &PalwStepCoordinateV1) -> Option<u64> {
        canonical_rank_with_shape_v3(self.profile, self.context, coordinate, &mut 0, Some(&self.shape))
    }
}

pub(super) fn canonical_rank_with_shape_v3(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
    visits: &mut u64,
    cached: Option<&PalwLeafShapeV1>,
) -> Option<u64> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let call = coord.call_index as u64;
    if call > decode_calls {
        return None;
    }
    let positions = if call == 0 { prefill } else { 1 };
    if (coord.position as u64) >= positions {
        return None;
    }
    let s = if call == 0 { coord.position as u64 + 1 } else { prefill + call };
    let with_logits = if call == 0 { coord.position as u64 + 1 == prefill } else { true };
    let slot_count = profile.global_node_count();
    if coord.node_slot >= slot_count {
        return None;
    }
    let post_first = slot_count - profile.post_nodes.len() as u32;
    let is_post = coord.node_slot >= post_first;
    if is_post && !with_logits {
        return None; // post nodes do not exist at non-logit positions
    }
    let constructed;
    let shape = match cached {
        Some(shape) => shape,
        None => {
            constructed = palw_leaf_shape_v1(profile, visits);
            &constructed
        }
    };
    let mut index = palw_job_running_total_v1(shape, u128::from(s - 1), u128::from(prefill), visits);
    // Slots before this one within the position.
    let pre = profile.pre_nodes.len() as u64;
    let slot = u64::from(coord.node_slot);
    let node = if slot < pre {
        for n in &profile.pre_nodes[..slot as usize] {
            *visits += 1;
            index = index.saturating_add(palw_node_tiles_at_v1(n, s));
        }
        &profile.pre_nodes[slot as usize]
    } else {
        for n in &profile.pre_nodes {
            *visits += 1;
            index = index.saturating_add(palw_node_tiles_at_v1(n, s));
        }
        let layers = PalwLayerMultiplicityV1::at(profile, s, visits);
        let layer_slots = layers.slots_before(layers.layers);
        let within_layers = slot - pre;
        if within_layers < layer_slots {
            let l = layers.layer_holding(u128::from(within_layers), |m, c| u128::from(m.slots_before(c)));
            index = index.saturating_add(layers.leaves_before(l));
            let at = (within_layers - layers.slots_before(l)) as usize;
            let table = profile.layer_table(u16::try_from(l).ok()?);
            for n in table.get(..at)? {
                *visits += 1;
                index = index.saturating_add(palw_node_tiles_at_v1(n, s));
            }
            table.get(at)?
        } else {
            index = index.saturating_add(layers.leaves_before(layers.layers));
            let at = (within_layers - layer_slots) as usize;
            for n in profile.post_nodes.get(..at)? {
                *visits += 1;
                index = index.saturating_add(palw_node_tiles_at_v1(n, s));
            }
            profile.post_nodes.get(at)?
        }
    };
    if u128::from(coord.tile_index) >= palw_node_tiles_at_v1(node, s) {
        return None;
    }
    u64::try_from(index.saturating_add(u128::from(coord.tile_index))).ok().filter(|v| *v < u64::MAX)
}
