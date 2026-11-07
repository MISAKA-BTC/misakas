//! Reuse immutable context digests while constructing a batch of canonical STEP leaves.
//!
//! The inherited prefix and ordinary job digest are distinct. Computing the ordinary
//! digest once and using it for every coordinate would move version-3 prefix leaves.

use kaspa_consensus_core::{
    palw_step_leg::{PalwStepTileLeafV1, step_tile_leaf_hash_v1},
    palw_v2::PalwJobContextV2,
};
use kaspa_hashes::Hash64;
use std::marker::PhantomData;

pub(crate) struct StepTileHasherV1<'a> {
    job: Hash64,
    prefix: Option<(u32, Hash64)>,
    shape: Hash64,
    // Keep the context immutably borrowed for the batch: its hashes cannot go stale.
    context: PhantomData<&'a PalwJobContextV2>,
}

impl<'a> StepTileHasherV1<'a> {
    pub(crate) fn new(ctx: &'a PalwJobContextV2, shape: Hash64) -> Self {
        let prefix = ctx.inherited_prefix_v1().and_then(|(k, _)| ctx.prefix_context_hash_v1().map(|hash| (k, hash)));
        Self { job: ctx.context_hash(), prefix, shape, context: PhantomData }
    }

    pub(crate) fn hash(&self, leaf: &PalwStepTileLeafV1) -> Hash64 {
        let hash = match self.prefix {
            Some((k, ref prefix)) if leaf.coord.call_index == 0 && leaf.coord.position < k => prefix,
            _ => &self.job,
        };
        step_tile_leaf_hash_v1(hash, &self.shape, leaf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{palw_step::PalwStepCoordinateV1, palw_step_leg::step_tile_leaf_hash_ctx_v1};

    fn context(version: u16, k: u64) -> PalwJobContextV2 {
        let mut nullifier = [0; 64];
        nullifier[56..].copy_from_slice(&k.to_le_bytes());
        PalwJobContextV2 {
            version,
            network_id: b"testnet-12".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_bytes(nullifier),
            assignment_id: Hash64::from_u64_word(2),
            execution_seed: [3; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: Hash64::from_u64_word(7),
            trace_scheme_id: Hash64::from_u64_word(8),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: Hash64::from_u64_word(11),
            declared_prefill_tokens: 7,
            exact_decode_tokens: 3,
            max_context_tokens: 10,
        }
    }

    fn leaf(call_index: u32, position: u32) -> PalwStepTileLeafV1 {
        PalwStepTileLeafV1 {
            version: 1,
            coord: PalwStepCoordinateV1 { call_index, position, node_slot: 13, tile_index: 17 },
            value_count: 2,
            values_le: [1.25f32.to_le_bytes(), (-2.5f32).to_le_bytes()].concat(),
        }
    }

    #[test]
    fn cached_contexts_match_canonical_hash_at_every_prefix_boundary() {
        let shape = Hash64::from_u64_word(19);
        for version in [0, 2, 3, u16::MAX] {
            for k in [0, 1, 4, 6, 7, 8, u32::MAX as u64, u32::MAX as u64 + 1] {
                let ctx = context(version, k);
                let cached = StepTileHasherV1::new(&ctx, shape);
                for call in [0, 1, 2, u32::MAX] {
                    for position in [0, 1, 3, 4, 5, 6, 7, 8, u32::MAX] {
                        let leaf = leaf(call, position);
                        assert_eq!(cached.hash(&leaf), step_tile_leaf_hash_ctx_v1(&ctx, &shape, &leaf));
                    }
                }
            }
        }
        for invalid in [0, 1] {
            let mut ctx = context(3, 4);
            if invalid == 0 {
                ctx.assignment_id = Hash64::default();
            } else {
                let mut bytes = ctx.job_nullifier.as_bytes();
                bytes[0] = 1;
                ctx.job_nullifier = Hash64::from_bytes(bytes);
            }
            let cached = StepTileHasherV1::new(&ctx, shape);
            assert_eq!(cached.hash(&leaf(0, 0)), step_tile_leaf_hash_ctx_v1(&ctx, &shape, &leaf(0, 0)));
        }
    }

    #[test]
    fn cached_prefix_is_job_independent_but_suffix_and_decode_rebind() {
        let shape = Hash64::from_u64_word(19);
        let left = context(3, 4);
        let mut right = left.clone();
        right.job_id = Hash64::from_u64_word(31);
        right.execution_seed = [32; 32];
        right.prompt_token_ids_hash = Hash64::from_u64_word(33);
        let left_hash = StepTileHasherV1::new(&left, shape);
        let right_hash = StepTileHasherV1::new(&right, shape);
        assert_eq!(left_hash.hash(&leaf(0, 3)), right_hash.hash(&leaf(0, 3)));
        assert_ne!(left_hash.hash(&leaf(0, 4)), right_hash.hash(&leaf(0, 4)));
        assert_ne!(left_hash.hash(&leaf(1, 0)), right_hash.hash(&leaf(1, 0)));
        let other_shape = StepTileHasherV1::new(&left, Hash64::from_u64_word(20));
        assert_ne!(left_hash.hash(&leaf(0, 3)), other_shape.hash(&leaf(0, 3)));
    }

    #[test]
    #[ignore = "comparative leaf-hashing measurement, no whole-model cost approval"]
    fn cached_hashing_measurement_preserves_every_leaf_and_merkle_root() {
        use kaspa_consensus_core::palw_step_leg::step_merkle_root_v1;
        use std::{hint::black_box, time::Instant};
        eprintln!("[rfc06-step-hash] cache_bytes={}", std::mem::size_of::<StepTileHasherV1>());
        for version in [2, 3] {
            let ctx = context(version, 4);
            let shape = Hash64::from_u64_word(19);
            let mut tile = leaf(0, 0);
            let begun = Instant::now();
            let mut historical = Vec::with_capacity(100_000);
            for i in 0..100_000 {
                tile.coord.position = i % 10;
                tile.coord.call_index = (i / 10) % 2;
                tile.coord.tile_index = i;
                historical.push(black_box(step_tile_leaf_hash_ctx_v1(black_box(&ctx), &shape, black_box(&tile))));
            }
            let historical_us = begun.elapsed().as_micros();
            let begun = Instant::now();
            let hasher = StepTileHasherV1::new(&ctx, shape);
            let mut cached = Vec::with_capacity(100_000);
            for i in 0..100_000 {
                tile.coord.position = i % 10;
                tile.coord.call_index = (i / 10) % 2;
                tile.coord.tile_index = i;
                cached.push(black_box(hasher.hash(black_box(&tile))));
            }
            let cached_us = begun.elapsed().as_micros();
            assert_eq!(historical, cached);
            assert_eq!(step_merkle_root_v1(&historical), step_merkle_root_v1(&cached));
            eprintln!("[rfc06-step-hash] version={version} leaves=100000 canonical_us={historical_us} cached_us={cached_us}");
        }
    }
}
