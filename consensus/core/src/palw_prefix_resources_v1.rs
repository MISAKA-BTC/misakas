//! Checked prospective family prefix work. Geometry comes from the authenticated canonical
//! graph and job, never from a prover's asserted cache size. No historical deadline is changed.
//! The whole-head recurrence implementation declares its actual resident requirement; paging
//! is still necessary to provide a fixed resident bound for the largest admitted Q36 heads.
use crate::palw_class_semantics_v1::PalwPrefixStateRelationV1;
use crate::palw_prefix_family_state_v1::PalwQwen36PrefixCachePlanV1;
use crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1;
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1};
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixResourcePlanV1 {
    pub envelope: PalwRenderingResourcePlanV1,
    pub maximum_head_working_set_bytes: u64,
    pub maximum_carrier_bytes: u64,
}
fn exact(v: u128) -> Result<u64, String> {
    u64::try_from(v).map_err(|_| "prefix exact resource contract overflows u64".into())
}
impl PalwPrefixResourcePlanV1 {
    pub fn derive(
        profile: &PalwShapeProfileV3,
        relation: PalwPrefixStateRelationV1,
        prefix_tokens: u32,
        step_leaves: u64,
        carrier_maximum: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        relation.check_profile(profile)?;
        if prefix_tokens == 0
            || prefix_tokens >= profile.n_ctx
            || prices.inclusion_margin_daa == 0
            || carrier_maximum < 65_536 + 16_384
        {
            return Err("prefix resource contract requires a proper prefix and bounded usable carrier".into());
        }
        let k = u128::from(prefix_tokens);
        let mut head_working = 0u128;
        let (bytes, leaves, moves, work) = match relation {
            PalwPrefixStateRelationV1::DenseInt32KvV1 | PalwPrefixStateRelationV1::Base0Int8KvV1 => {
                let plan = PalwPrefixKvPlanV1::of_profile(profile, prefix_tokens)?;
                let bytes = u128::from(profile.layer_count) * 2 * k * u128::from(plan.width) * 4;
                let leaves = u128::from(plan.tile_count);
                // Per-tile rounding adds at most one body move per leaf; every old leaf
                // requires a begin/opening and a finish before its tentative bytes count.
                (bytes, leaves, bytes.div_ceil(65_536) + leaves * 3 + 1, bytes * 2)
            }
            PalwPrefixStateRelationV1::Qwen36HybridCacheV1 => {
                let plan = PalwQwen36PrefixCachePlanV1::new(profile, prefix_tokens)?;
                let gdn_layers =
                    (0..profile.layer_count).filter(|l| profile.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).count() as u128;
                let heads = gdn_layers * u128::from(profile.gdn_heads);
                let dk = u128::from(profile.gdn_head_k_dim);
                let dv = u128::from(profile.gdn_head_v_dim);
                // Exact current clone/scratch envelope of the owned zero-state replay.
                head_working = if heads == 0 { 0 } else { dk * dv * 8 + (2 * dk + 3 * dv + 2) * 4 + 256 };
                // Head inputs can share old full tiles with other heads, but authentication
                // must still cover those bytes. This conservative graph-wide bound counts
                // each original step leaf once per head plus the public KV/conv pass.
                let leaves = u128::from(step_leaves) * (heads + 1);
                let parameters = heads * u128::from(profile.gdn_heads) * 68;
                let bytes =
                    leaves * crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES as u128 + parameters + u128::from(plan.total_bytes());
                let moves = bytes.div_ceil(65_536) + leaves * 3 + heads * (k + 2) + u128::from(plan.unit_count()) + 1;
                let work = bytes * 2 + heads * k * dk * dv * 32;
                (bytes, leaves, moves, work)
            }
        };
        let moves = moves + u128::from(prices.reorg_replay_moves);
        let blake = bytes.div_ceil(128) + leaves * 4;
        let profile_bytes = borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u128;
        let row = head_working + profile_bytes + 262_144 + 65_536 + 32_768 + 16_384;
        let resident = head_working + profile_bytes * 4 + 1_048_576;
        // Narrow every admitted cumulative quantity BEFORE multiplying by u64 prices.
        // Multiplication then fits u128; sums still fail explicitly instead of wrapping.
        let bytes = exact(bytes)?;
        let leaves = exact(leaves)?;
        let moves = exact(moves)?;
        let work = exact(work)?;
        let blake = exact(blake)?;
        let row = exact(row)?;
        let resident = exact(resident)?;
        let rent = u128::from(bytes) * u128::from(prices.rent_per_stream_byte);
        let rent = rent
            .checked_add(u128::from(work) * u128::from(prices.rent_per_parser_step))
            .and_then(|n| n.checked_add(u128::from(row) * u128::from(prices.rent_per_row_byte)))
            .ok_or("prefix exact rent overflows u128")?;
        let envelope = PalwRenderingResourcePlanV1 {
            prefix_tokens,
            vocab: profile.vocab_size,
            maximum_stream_bytes: bytes,
            maximum_stream_leaves: leaves,
            maximum_eog_bytes: 0,
            maximum_moves: moves,
            maximum_parser_steps: work,
            maximum_blake_blocks: blake,
            inclusion_daa: exact(u128::from(moves) * u128::from(prices.inclusion_margin_daa))?,
            rent_sompi: exact(rent)?,
            deposit_sompi: exact(u128::from(moves) * u128::from(prices.deposit_per_move))?,
            row_bytes: row,
            resident_ram_bytes: resident,
        };
        Ok(Self {
            envelope,
            maximum_head_working_set_bytes: exact(head_working)?,
            maximum_carrier_bytes: exact(u128::from(moves) * u128::from(carrier_maximum))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prices() -> PalwRenderingResourcePricesV1 {
        PalwRenderingResourcePricesV1 {
            inclusion_margin_daa: 4,
            rent_per_stream_byte: 1,
            rent_per_parser_step: 1,
            rent_per_row_byte: 1,
            deposit_per_move: 1,
            reorg_replay_moves: 3,
        }
    }
    #[test]
    fn authenticated_i8_geometry_prices_every_old_i32_leaf_and_refuses_price_overflow() {
        let (binding, _, _) = crate::palw_prefix_state_court_v1::tests::fixture(false);
        let p = &binding.shape_profile;
        let plan = PalwPrefixResourcePlanV1::derive(
            p,
            PalwPrefixStateRelationV1::Base0Int8KvV1,
            2,
            binding.step_leaf_count,
            90_000,
            prices(),
        )
        .unwrap();
        let kv = PalwPrefixKvPlanV1::of_profile(p, 2).unwrap();
        assert_eq!(plan.envelope.maximum_stream_bytes, u64::from(kv.layer_count) * 2 * 2 * u64::from(kv.width) * 4);
        assert_eq!(plan.envelope.maximum_stream_leaves, kv.tile_count);
        assert_eq!(plan.envelope.inclusion_daa, plan.envelope.maximum_moves * 4);
        assert_eq!(plan.maximum_head_working_set_bytes, 0);
        assert!(
            PalwPrefixResourcePlanV1::derive(
                p,
                PalwPrefixStateRelationV1::Base0Int8KvV1,
                2,
                binding.step_leaf_count,
                90_000,
                PalwRenderingResourcePricesV1 { rent_per_stream_byte: u64::MAX, ..prices() }
            )
            .is_err()
        );
        assert!(
            PalwPrefixResourcePlanV1::derive(
                p,
                PalwPrefixStateRelationV1::Base0Int8KvV1,
                p.n_ctx,
                binding.step_leaf_count,
                90_000,
                prices()
            )
            .is_err()
        );
        assert!(
            PalwPrefixResourcePlanV1::derive(
                p,
                PalwPrefixStateRelationV1::Base0Int8KvV1,
                2,
                binding.step_leaf_count,
                70_000,
                prices()
            )
            .is_err()
        );
    }
}
