//! Checked prospective family prefix work. Geometry comes from the authenticated canonical
//! graph and job, never from a prover's asserted cache size. No historical deadline is changed.
//! Q36 paging declares linear authenticated-input scratch and fixed page buffers, never a
//! resident whole-matrix requirement. Complete signed page carriers are priced explicitly.
use crate::palw_class_semantics_v1::PalwPrefixStateRelationV1;
use crate::palw_prefix_family_state_v1::PalwQwen36PrefixCachePlanV1;
use crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1;
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1};
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepOutLenV1, kernel_semantics_id_v1};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPrefixGdnModeV1 {
    Whole,
    Paged,
}
/// Geometry-owned resident ceiling: exact native whole-head clone/scratch if it fits this
/// linear page envelope, otherwise exact paged recurrence. No accuser chooses a cheaper quote
/// and later spends a more expensive representation's work.
pub fn palw_prefix_gdn_mode_v1(profile: &PalwShapeProfileV3) -> (PalwPrefixGdnModeV1, u128) {
    let dk = u128::from(profile.gdn_head_k_dim);
    let dv = u128::from(profile.gdn_head_v_dim);
    let paged = (5 * dk + 5 * dv + 4) * 4 + 196_608;
    let whole = dk * dv * 8 + (2 * dk + 3 * dv + 2) * 4 + 256;
    if whole <= paged { (PalwPrefixGdnModeV1::Whole, whole) } else { (PalwPrefixGdnModeV1::Paged, paged) }
}
fn range_cost(width: u64, tile: u32, start: u64, end: u64) -> Result<(u128, u128, u128), String> {
    if tile == 0 || start >= end || end > width {
        return Err("prefix quoted source range is not canonical".into());
    }
    let t = u64::from(tile);
    let first = start / t;
    let last = (end - 1) / t;
    let leaves = last - first + 1;
    let bytes = (width.min((last + 1) * t) - first * t) * 4;
    let ragged = if last == width / t && width % t != 0 { width % t } else { 0 };
    let full = leaves - u64::from(ragged != 0);
    let moves =
        u128::from(full) * (u128::from(tile) * 4).div_ceil(65_536) + u128::from(ragged * 4).div_ceil(65_536) + u128::from(leaves) * 2;
    Ok((u128::from(bytes), u128::from(leaves), moves))
}
fn fixed_width(node: &crate::palw_step::PalwStepNodeV1) -> Result<u64, String> {
    let PalwStepOutLenV1::Fixed { elements } = node.out_len else {
        return Err("prefix quote source width is not fixed".into());
    };
    Ok(u64::from(elements))
}
fn full_cache_row_cost(width: u64, tile: u32, base: u64, total: u64) -> Result<(u128, u128, u128), String> {
    let (b, l, m) = range_cost(width, tile, 0, width)?;
    // Exact for a one-chunk unit or when source tiles align with every cache-chunk boundary.
    // Otherwise this bounds all chunk/source boundary repeats using that source's own tiles.
    let repeats = if total <= 16_384 || (16_384 % u64::from(tile) == 0 && base % u64::from(tile) == 0) {
        1
    } else {
        u64::from(tile).div_ceil(16_384) + 1
    };
    Ok((b * u128::from(repeats), l * u128::from(repeats), m * u128::from(repeats)))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixResourcePlanV1 {
    /// Physical signed packets, separate from envelope.maximum_moves logical work.
    pub maximum_carriers: u64,
    pub maximum_packet_body_bytes: u64,
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
        let profile_bytes = borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64;
        Self::derive_with_profile_bytes(profile, relation, prefix_tokens, step_leaves, carrier_maximum, prices, profile_bytes)
    }
    /// Fee-equivalent stateless skeleton quoting. The rooted hydration fold must separately
    /// require this exact original Borsh size, not a caller's smaller estimate.
    pub fn derive_with_profile_bytes(
        profile: &PalwShapeProfileV3,
        relation: PalwPrefixStateRelationV1,
        prefix_tokens: u32,
        step_leaves: u64,
        carrier_maximum: u64,
        prices: PalwRenderingResourcePricesV1,
        original_profile_bytes: u64,
    ) -> Result<Self, String> {
        relation.check_profile(profile)?;
        let stub_bytes = borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64;
        if original_profile_bytes < stub_bytes {
            return Err("hydrated profile byte count is below quoted skeleton".into());
        }
        if prefix_tokens == 0
            || prefix_tokens >= profile.n_ctx
            || step_leaves == 0
            || prices.inclusion_margin_daa == 0
            || carrier_maximum < 65_536 + 16_384
        {
            return Err("prefix resource contract requires a proper prefix and bounded usable carrier".into());
        }
        let k = u128::from(prefix_tokens);
        let mut head_working = 0u128;
        let mut parameter_heads = 0u128;
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
                parameter_heads = heads;
                let dk = u128::from(profile.gdn_head_k_dim);
                let dv = u128::from(profile.gdn_head_v_dim);
                let (mode, working) = palw_prefix_gdn_mode_v1(profile);
                head_working = if heads == 0 { 0 } else { working };
                let mut original_bytes = 0u128;
                let mut leaves = 0u128;
                let mut source_moves = 0u128;
                if let Some(layer) = (0..profile.layer_count).find(|l| profile.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet) {
                    let (_, node, widths) = crate::palw_prefix_gdn_replay_v1::head_geometry(profile, layer, 0)?;
                    for head in 0..u32::from(profile.gdn_heads) {
                        let ranges = crate::palw_step_refute::qwen36_gdn_slice_v1(profile, node, &widths, head)
                            .ok_or("quoted head slices are not canonical")?;
                        for (ordinal, reference) in node.input_refs.iter().enumerate() {
                            let source = &profile.gdn_nodes[usize::from(*reference)];
                            let (b, l, m) = range_cost(widths[ordinal], source.tile_len, ranges[ordinal].0, ranges[ordinal].1)?;
                            original_bytes += b * k * gdn_layers;
                            leaves += l * k * gdn_layers;
                            source_moves += m * k * gdn_layers;
                        }
                    }
                    let mut convs = profile
                        .gdn_nodes
                        .iter()
                        .filter(|n| n.kernel_semantics_id == kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_SSM_CONV));
                    let conv = convs.next().ok_or("quoted convolution absent")?;
                    if convs.next().is_some() || conv.input_refs.len() != 3 {
                        return Err("quoted convolution sources not unique".into());
                    }
                    let total = conv.input_refs.iter().try_fold(0u64, |n, r| {
                        fixed_width(&profile.gdn_nodes[usize::from(*r)])
                            .and_then(|w| n.checked_add(w).ok_or("convolution width overflow".into()))
                    })?;
                    let expected = 2
                        * u64::from(crate::palw_step::palw_gdn_key_heads_v1(profile).map_err(|e| e.to_string())?)
                        * u64::from(profile.gdn_head_k_dim)
                        + u64::from(profile.gdn_heads) * u64::from(profile.gdn_head_v_dim);
                    if total != expected {
                        return Err("quoted convolution sources change exact cache width".into());
                    }
                    let rows = k.min(u128::from(profile.gdn_conv_kernel)) * gdn_layers;
                    let mut base = 0u64;
                    for r in &conv.input_refs {
                        let source = &profile.gdn_nodes[usize::from(*r)];
                        let width = fixed_width(source)?;
                        let (b, l, m) = full_cache_row_cost(width, source.tile_len, base, total)?;
                        original_bytes += b * rows;
                        leaves += l * rows;
                        source_moves += m * rows;
                        base += width;
                    }
                }
                let attention_layers = u128::from(profile.layer_count) - gdn_layers;
                if attention_layers != 0 {
                    let expected = u64::from(profile.attn_kv_heads) * u64::from(profile.attn_head_dim);
                    for role in [PalwStepNodeRoleV1::KCacheWrite, PalwStepNodeRoleV1::VCacheWrite] {
                        let mut nodes = profile.attn_nodes.iter().filter(|n| n.role == role);
                        let node = nodes.next().ok_or("quoted KV source absent")?;
                        if nodes.next().is_some() || fixed_width(node)? != expected {
                            return Err("quoted KV source not unique/exact".into());
                        }
                        let (b, l, m) = full_cache_row_cost(expected, node.tile_len, 0, expected)?;
                        original_bytes += b * k * attention_layers;
                        leaves += l * k * attention_layers;
                        source_moves += m * k * attention_layers;
                    }
                }
                // Canonical inventory has one68-byte original artifact leaf per head.
                original_bytes += heads * 68;
                leaves += heads;
                let pages = dk.div_ceil(crate::palw_prefix_gdn_paged_v1::PALW_GDN_PAGE_WORDS_V1 as u128) * dv;
                let page_moves = if mode == PalwPrefixGdnModeV1::Paged { heads * pages * (2 * k + 1) } else { 0 };
                let page_bytes = page_moves * (16_384 + 22 * 64 + 64);
                let bytes = original_bytes + page_bytes;
                let public_chunks =
                    (u128::from(plan.total_bytes()) - heads * dk * dv * 4).div_ceil(65_536) + u128::from(plan.unit_count());
                let whole_digest = if mode == PalwPrefixGdnModeV1::Whole { heads * (dk * dv * 4).div_ceil(65_536) } else { 0 };
                let moves = source_moves + heads * (k + 1) + public_chunks + whole_digest + page_moves + 1;
                let work = bytes * 2 + heads * k * dk * dv * 32 + (page_moves + heads * k) * working;
                (bytes, leaves, moves, work)
            }
        };
        // Reorg allowance is measured in physical signed packets, each carrying at most64
        // independently charged logical actions. Do not reuse the old single-action quote.
        let replay_logical = u128::from(prices.reorg_replay_moves) * 64;
        let moves = exact(moves + replay_logical)?;
        // Current pure calls revalidate/hash the immutable graph/context. Account every
        // move's full hydrated profile and the public-filer accepted wire job bound,
        // including conservative repeated validation passes; a small stub cannot discount it.
        let repeated_validation = exact(
            u128::from(moves)
                .checked_mul(u128::from(original_profile_bytes) + 125_000u128)
                .and_then(|n| n.checked_mul(32))
                .ok_or("prefix repeated validation budget overflows u128")?,
        )?;
        // Complete signed carrier budget includes all old-tree openings, headers,
        // signatures and the seed, not just cache/raw bytes.
        // Deterministic next-fit packing of at most64 flat actions, reserving16KiB for
        // the complete outer signed carrier. Every logical action remains individually
        // checked/charged. A byte-closed packet plus the next packet exceeds body_max;
        // disjoint adjacent pairs bound those closures by ceil(2*wire_bytes/body_max).
        // Count-closed packets independently cost at most ceil(logical_moves/64).
        let body_max = carrier_maximum.checked_sub(16_384).ok_or("prefix packet header budget absent")?;
        let step_depth = if step_leaves <= 1 { 0 } else { 64 - (step_leaves - 1).leading_zeros() };
        // Original inventory has at mostu32 leaves. Names may be erased in a fee skeleton;
        // the exact retained profile byte count safely bounds any single declared name.
        let artifact_opening = 128u128 + u128::from(original_profile_bytes) + 32 * 64;
        let largest_action = (65_536u128 + 128)
            .max(128 + u128::from(step_depth) * 64)
            .max(if parameter_heads == 0 { 0 } else { artifact_opening })
            .max(16_384 + 22 * 64 + 128);
        if largest_action > u128::from(body_max) {
            return Err(
                "original prefix action does not fit its signed bounded packet; a streamed identity contract is required".into()
            );
        }
        let logical = u128::from(moves);
        let action_wire = bytes
            + logical * 128
            + leaves * u128::from(step_depth) * 64
            + parameter_heads * (u128::from(original_profile_bytes) + 32 * 64)
            + u128::from(prices.reorg_replay_moves) * u128::from(body_max);
        let maximum_carriers = exact((action_wire * 2).div_ceil(u128::from(body_max)) + logical.div_ceil(64) + 2)?;
        let maximum_carriers = maximum_carriers.min(moves);
        let carrier_bytes = exact(u128::from(maximum_carriers) * u128::from(carrier_maximum))?;
        let replay_raw = u128::from(prices.reorg_replay_moves) * u128::from(body_max);
        let blake = bytes.div_ceil(128)
            + leaves * (4 + 64 * 3)
            + replay_raw.div_ceil(128)
            + replay_logical * (4 + 64 * 3)
            + (u128::from(carrier_bytes) + u128::from(repeated_validation)).div_ceil(128);
        let work = work
            + leaves * 64 * 384
            + replay_raw * 2
            + replay_logical * 64 * 384
            + u128::from(carrier_bytes)
            + u128::from(repeated_validation);
        let profile_bytes = u128::from(original_profile_bytes);
        let row = head_working + profile_bytes + 262_144 + 65_536 + 32_768 + 16_384;
        let resident = head_working + profile_bytes * 4 + 1_048_576;
        // Narrow every admitted cumulative quantity BEFORE multiplying by u64 prices.
        // Multiplication then fits u128; sums still fail explicitly instead of wrapping.
        let bytes = exact(bytes)?;
        let leaves = exact(leaves)?;
        let work = exact(work)?;
        let blake = exact(blake)?;
        let row = exact(row)?;
        let resident = exact(resident)?;
        let charged_bytes = u128::from(bytes) + u128::from(carrier_bytes);
        let rent =
            charged_bytes.checked_mul(u128::from(prices.rent_per_stream_byte)).ok_or("prefix exact byte rent overflows u128")?;
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
            inclusion_daa: exact(u128::from(maximum_carriers) * u128::from(prices.inclusion_margin_daa))?,
            rent_sompi: exact(rent)?,
            deposit_sompi: exact(u128::from(moves) * u128::from(prices.deposit_per_move))?,
            row_bytes: row,
            resident_ram_bytes: resident,
        };
        Ok(Self {
            envelope,
            maximum_head_working_set_bytes: exact(head_working)?,
            maximum_carrier_bytes: carrier_bytes,
            maximum_carriers,
            maximum_packet_body_bytes: body_max,
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
    fn canonical_hybrid_quotes_only_required_public_sources_and_selects_efficient_whole_head() {
        let p = crate::palw_qwen36_profile::qwen36_profile_v6(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            crate::palw_qwen36_profile::QWEN36_35B_A3B,
        ))
        .unwrap();
        assert_eq!(palw_prefix_gdn_mode_v1(&p), (PalwPrefixGdnModeV1::Whole, 133_896));
        let quote = |leaves| {
            PalwPrefixResourcePlanV1::derive(&p, PalwPrefixStateRelationV1::Qwen36HybridCacheV1, 2, leaves, 90_000, prices()).unwrap()
        };
        let a = quote(1);
        let b = quote(1 << 40);
        assert_eq!(a.envelope.maximum_stream_bytes, b.envelope.maximum_stream_bytes);
        assert_eq!(a.envelope.maximum_stream_leaves, b.envelope.maximum_stream_leaves);
        assert_eq!(
            a.envelope.maximum_moves, b.envelope.maximum_moves,
            "unrelated committed decode/calculation leaves are not demanded once per head"
        );
        assert!(a.maximum_carriers <= b.maximum_carriers, "original path depth is still authenticated/priced");
        assert!(b.maximum_carriers < b.envelope.maximum_moves);
        assert_eq!(a.envelope.maximum_stream_bytes, 8_503_040);
        assert_eq!(a.envelope.maximum_stream_leaves, 73_300);
        assert!(a.envelope.resident_ram_bytes < 2 * 1024 * 1024);
        let mut huge = p.clone();
        huge.gdn_head_k_dim = 1 << 17;
        huge.gdn_head_v_dim = 1 << 17;
        assert_eq!(palw_prefix_gdn_mode_v1(&huge).0, PalwPrefixGdnModeV1::Paged);
        assert!(palw_prefix_gdn_mode_v1(&huge).1 < 6 * 1024 * 1024);
    }
    #[test]
    fn hydrated_profile_quote_charges_original_bytes_on_every_move_and_refuses_understatement() {
        let (binding, _, _) = crate::palw_prefix_state_court_v1::tests::fixture(false);
        let p = &binding.shape_profile;
        let size = borsh::to_vec(p).unwrap().len() as u64;
        let quote = |n| {
            PalwPrefixResourcePlanV1::derive_with_profile_bytes(
                p,
                PalwPrefixStateRelationV1::Base0Int8KvV1,
                2,
                binding.step_leaf_count,
                90_000,
                prices(),
                n,
            )
        };
        let ordinary = PalwPrefixResourcePlanV1::derive(
            p,
            PalwPrefixStateRelationV1::Base0Int8KvV1,
            2,
            binding.step_leaf_count,
            90_000,
            prices(),
        )
        .unwrap();
        assert_eq!(quote(size).unwrap(), ordinary);
        let bigger = quote(size + 1024).unwrap();
        assert_eq!(bigger.envelope.maximum_moves, ordinary.envelope.maximum_moves);
        assert_eq!(bigger.envelope.row_bytes, ordinary.envelope.row_bytes + 1024);
        assert_eq!(bigger.envelope.resident_ram_bytes, ordinary.envelope.resident_ram_bytes + 4096);
        assert_eq!(
            bigger.envelope.maximum_parser_steps,
            ordinary.envelope.maximum_parser_steps + ordinary.envelope.maximum_moves * 1024 * 32
        );
        assert!(bigger.envelope.maximum_blake_blocks > ordinary.envelope.maximum_blake_blocks);
        assert!(bigger.envelope.rent_sompi > ordinary.envelope.rent_sompi);
        assert!(quote(size - 1).is_err());
        assert!(quote(u64::MAX).is_err());
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
        assert_eq!(plan.envelope.inclusion_daa, plan.maximum_carriers * 4);
        assert!(plan.maximum_carriers <= plan.envelope.maximum_moves);
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
