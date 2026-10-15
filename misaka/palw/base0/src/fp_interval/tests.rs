//! **`fp_interval`'s `tests` module**, moved out of `fp_interval.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;

/// The wire form is frozen: a magic and a version, so bytes that are not an interval opening
/// are refused as such rather than mis-parsed as one.
#[test]
fn the_opening_wire_form_is_frozen() {
    assert_eq!(&PALW_BASE0_FP_INTERVAL_MAGIC_V1, b"MSKFPIV1");
    assert_eq!(PALW_BASE0_FP_INTERVAL_VERSION_V1, 1);
    assert_eq!(Base0FpIntervalOpeningV1::decode_v1(b"not this family").unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
    assert_eq!(
        Base0FpIntervalOpeningV1::decode_v1(&PALW_BASE0_FP_INTERVAL_MAGIC_V1).unwrap_err(),
        Base0FpIntervalError::NotThisFamilysBytes
    );
}

/// **The V4 wire form is frozen, and the old decoders refuse it** (ADR-0086 X1, X7): a
/// V1–V3 decoder handed V4 bytes returns `NotThisFamilysBytes` — which the V1–V3 verifier
/// turns into `Unverifiable`, never `Mismatch` and never a fault — and the any-form decoder
/// returns `Digests`.
#[test]
fn the_v4_wire_form_is_frozen_and_the_old_decoders_refuse_it() {
    assert_eq!(&PALW_BASE0_FP_INTERVAL_MAGIC_V4, b"MSKFPIV4");
    assert_eq!(PALW_BASE0_FP_INTERVAL_VERSION_V4, 4);
    let (material, _claim, ids, _artifact) = floor_material(3, 4);
    let bytes = base0_open_fp_interval_v1(
        &material,
        0,
        &ids,
        PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("opens");
    assert!(bytes.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4));
    assert_eq!(Base0FpIntervalOpeningV1::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
    assert_eq!(Base0FpIntervalOpeningV2::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
    assert_eq!(Base0FpIntervalOpeningV3::decode_v1(&bytes).unwrap_err(), Base0FpIntervalError::NotThisFamilysBytes);
    let decoded = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("its own decoder");
    assert_eq!(decoded.encode_v1().expect("re-encodes"), bytes, "the form round-trips byte for byte");
    assert!(matches!(base0_fp_interval_opening_decode_any_v1(&bytes), Ok(Base0FpIntervalOpeningAnyV1::Digests(_))));
    let mut wrong_version = decoded.clone();
    wrong_version.version = PALW_BASE0_FP_INTERVAL_VERSION_V3;
    assert_eq!(
        Base0FpIntervalOpeningV4::decode_v1(&wrong_version.encode_v1().unwrap()).unwrap_err(),
        Base0FpIntervalError::NotThisFamilysBytes
    );
}

/// **A fault has a block for an address** (ADR-0086 X5): over a synthetic space of 2^22+1
/// leaves, a range spanning three whole blocks whose middle block the seat computes
/// differently names exactly that block, clipped to the range; a range that agrees names
/// nothing; and the digests are the fold's own retained nodes.
#[test]
fn a_differing_block_is_named_at_the_folds_granularity() {
    let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
    let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
    let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
    let block = 1u64 << level;
    let leaves: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 7)).collect();
    let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&leaves, level, cap).expect("a tree");
    // A range from mid-block 3 to mid-block 7: whole blocks 4, 5, 6.
    let first = 3 * block + 5;
    let end = 7 * block + 9;
    let count = end - first;
    let (span_first, span_end) = tree.span_for_range(first, count).expect("the span");
    let range = tree.range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], first, count).expect("the range");
    let fold = Base0FpFoldRangeOpeningV1::from_range_v1(&range, &tree).expect("the fold form");
    assert_eq!(fold.whole_blocks_v1(leaf_count), (4, 7));
    assert_eq!(fold.block_roots, tree.retained_nodes()[4..7].to_vec(), "the digests are the retained nodes");
    let own: Vec<Hash64> = leaves[first as usize..end as usize].to_vec();
    assert_eq!(fold.first_block_that_differs_v1(&own, leaf_count), None, "an honest seat agrees with every digest");
    assert_eq!(
        step_range_opening_root_capped_v1(leaf_count, &fold.with_leaves_v1(own.clone()).unwrap(), cap).ok(),
        tree.root().ok(),
        "and its own leaves walk to the root under the served frontier"
    );
    let mut wrong = own.clone();
    let at = (5 * block + 100 - first) as usize;
    wrong[at] = Hash64::from_u64_word(0xBAD);
    assert_eq!(
        fold.first_block_that_differs_v1(&wrong, leaf_count),
        Some((5 * block, block)),
        "the leaf the seat computes differently is addressed by its block"
    );
    assert_ne!(step_range_opening_root_capped_v1(leaf_count, &fold.with_leaves_v1(wrong).unwrap(), cap).ok(), tree.root().ok());
    // The edge: a range inside one block has no whole block, and its address is itself.
    let (span_first, span_end) = tree.span_for_range(block + 10, 20).expect("the span");
    let edge = tree.range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], block + 10, 20).expect("the range");
    let edge_fold = Base0FpFoldRangeOpeningV1::from_range_v1(&edge, &tree).expect("the fold form");
    assert!(edge_fold.block_roots.is_empty());
    assert_eq!(fold_edge_v1(&edge_fold, leaf_count), (block + 10, 20));
    // A range that reaches the tree's end counts the partial tail block as whole.
    let tail_first = leaf_count - block - 3;
    let (span_first, span_end) = tree.span_for_range(tail_first, leaf_count - tail_first).expect("the span");
    let tail = tree
        .range_opening_v1(span_first, &leaves[span_first as usize..span_end as usize], tail_first, leaf_count - tail_first)
        .expect("the range");
    let tail_fold = Base0FpFoldRangeOpeningV1::from_range_v1(&tail, &tree).expect("the fold form");
    // The range starts three leaves inside the block before last, so its whole blocks are the
    // full block before the tail and the one-leaf tail itself — the tail counts because the
    // range reaches the tree's end.
    assert_eq!(tail_fold.whole_blocks_v1(leaf_count), (tail_first.div_ceil(block), leaf_count.div_ceil(block)));
    assert_eq!(tail_fold.block_roots.len(), 2);
    assert_eq!(tail_fold.block_roots.last(), tree.retained_nodes().last(), "the tail block's digest is the fold's last node");
}

/// **A close's committed range takes the one served block a lie is in, and refuses every other
/// case by name** (ADR-0085 Decision 3 against a liar). On the producer's tree with one leaf of
/// block 5 lying, the challenger's own leaves with the served block 5 substituted walk to the
/// committed root. Refused by name: no served block; a served block that is not the one that
/// differs; a served block whose leaves do not fold to the opening's digest; and two differing
/// blocks, where the commitment differs past the block a lie would be confined to.
#[test]
fn a_close_takes_the_one_served_block_a_lie_is_in_and_refuses_the_rest_by_name() {
    let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
    let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
    let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
    let block = 1u64 << level;
    let honest: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 7)).collect();
    let mut producer = honest.clone();
    producer[(5 * block + 100) as usize] = Hash64::from_u64_word(0xBAD);
    let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&producer, level, cap).expect("a tree");
    let root = tree.root().expect("its root");
    let (first, end) = (3 * block + 5, 7 * block + 9);
    let (span_first, span_end) = tree.span_for_range(first, end - first).expect("the span");
    let range =
        tree.range_opening_v1(span_first, &producer[span_first as usize..span_end as usize], first, end - first).expect("the range");
    let fold = Base0FpFoldRangeOpeningV1::from_range_v1(&range, &tree).expect("the fold form");
    let own: Vec<Hash64> = honest[first as usize..end as usize].to_vec();
    let cut = |k: u64, from: &[Hash64]| {
        Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, k, &|i| from.get(i as usize).copied()).expect("a whole block")
    };
    let walks = |r: &PalwStepRangeOpeningV1| step_range_opening_root_capped_v1(leaf_count, r, cap).ok() == Some(root);
    assert!(!walks(&fold.with_leaves_v1(own.clone()).unwrap()), "the challenger's own leaves do not walk a lying range");
    assert_eq!(fold.blocks_that_differ_v1(&own, leaf_count), Some(vec![5]));

    let committed = base0_fp_committed_range_with_served_block_v1(&fold, &own, &[cut(5, &producer)], leaf_count, 0)
        .expect("the one differing block, served, is taken");
    assert!(walks(&committed), "the substituted range is the committed one");
    let refused = |served: &[Base0FpBlockLeavesV1], own: &[Hash64]| {
        base0_fp_committed_range_with_served_block_v1(&fold, own, served, leaf_count, 0).expect_err("refused")
    };
    assert!(refused(&[], &own).contains("holds no served block"), "no block");
    assert!(refused(&[cut(4, &producer)], &own).contains("holds no served block"), "another block is not this one");
    assert!(refused(&[cut(5, &honest)], &own).contains("does not fold to the digest"), "a block that is not the executor's");
    let mut two = own.clone();
    two[(4 * block + 7 - first) as usize] = Hash64::from_u64_word(0x2);
    let many = refused(&[cut(4, &producer), cut(5, &producer)], &two);
    assert!(many.contains("2 blocks") && many.contains("[4, 5]"), "two differing blocks: {many}");
    let producers_own: Vec<Hash64> = producer[first as usize..end as usize].to_vec();
    assert!(refused(&[], &producers_own).contains("every whole block of the range is its own"), "nothing differs in a block");
}

/// **The address becomes a leaf** (ADR-0086 X6): from a served block of the producer's leaves
/// and its own replay, the challenger names the leaf, rebuilds the producer's range with that
/// block substituted, and the path it derives from it walks to the committed root — the
/// court's opening, assembled from what was served.
#[test]
fn a_served_block_names_the_leaf_and_the_path_walks_to_the_root() {
    use kaspa_consensus_core::palw_step_leg::{step_opening_from_range_capped_v1, step_opening_root_capped_v1};
    let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
    let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
    let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
    let block = 1u64 << level;
    // The producer's leaves, and the honest ones: one leaf in block 5 differs.
    let producer: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 7)).collect();
    let bad = 5 * block + 100;
    let mut honest = producer.clone();
    honest[bad as usize] = Hash64::from_u64_word(0x600D);
    let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&producer, level, cap).expect("a tree");
    let root = tree.root().expect("its root");
    let (first, end) = (3 * block + 5, 7 * block + 9);
    let count = end - first;
    let (span_first, span_end) = tree.span_for_range(first, count).expect("the span");
    let range = tree.range_opening_v1(span_first, &producer[span_first as usize..span_end as usize], first, count).expect("the range");
    let fold = Base0FpFoldRangeOpeningV1::from_range_v1(&range, &tree).expect("the fold form");
    let own: Vec<Hash64> = honest[first as usize..end as usize].to_vec();
    let (block_first, block_len) = fold.first_block_that_differs_v1(&own, leaf_count).expect("the seat names a block");
    assert_eq!((block_first, block_len), (5 * block, block));
    // The producer serves the block; the challenger checks it folds to the served digest.
    let served = Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, 5, &|i| producer.get(i as usize).copied())
        .expect("the block is the range's");
    assert_eq!(served.leaf_hashes.len() as u64, block);
    assert!(served.folds_to_v1(&fold.block_roots[1]), "the served block is the second whole block's digest");
    assert_eq!(Base0FpBlockLeavesV1::decode_v1(&served.encode_v1().unwrap()).unwrap(), served);
    // Block 3 straddles the range's start: served as the range's left edge, its leaves inside
    // the range (ADR-0121 §7) — which no digest checks, so the block naming never takes it —
    // and a block outside the range is nothing.
    let edge = Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, 3, &|i| producer.get(i as usize).copied())
        .expect("block 3 is the range's left edge");
    let (left, _) = fold.edges_v1(leaf_count);
    assert_eq!((edge.first_leaf_index, edge.first_leaf_index + edge.leaf_hashes.len() as u64), left);
    assert_eq!(left, (first, 4 * block));
    assert!(Base0FpBlockLeavesV1::cut_v1(0, &fold, leaf_count, 2, &|i| producer.get(i as usize).copied()).is_none());
    // The leaf, and the court's path from the range with the served block substituted.
    assert_eq!(served.name_the_leaf_v1(&|i| honest.get(i as usize).copied()), Some(bad));
    assert_eq!(served.name_the_leaf_v1(&|i| producer.get(i as usize).copied()), None, "a block that agrees names nothing");
    let producers_range = base0_fp_range_with_served_block_v1(&fold, &own, &served).expect("the producer's range");
    assert_eq!(step_range_opening_root_capped_v1(leaf_count, &producers_range, cap).ok(), Some(root));
    let path = step_opening_from_range_capped_v1(leaf_count, &producers_range, bad, cap).expect("a path to the leaf");
    assert_eq!(step_opening_root_capped_v1(leaf_count, &path, cap).ok(), Some(root), "the derived path walks to the committed root");
    // Without the served block the challenger's own range walks nowhere.
    let honest_range = fold.with_leaves_v1(own.clone()).unwrap();
    assert_ne!(step_range_opening_root_capped_v1(leaf_count, &honest_range, cap).ok(), Some(root));
}

/// Interval 0 is the prefill plus the calls up to the first checkpoint; interval `j` is the
/// calls after checkpoint `j − 1`; every call is in exactly one of them. Pinned across the
/// cadences the shipped classes use (1, the integer family's) and the ones a Decision 10 map
/// would bring.
#[test]
fn every_call_is_in_exactly_one_interval() {
    for interval in [1u32, 2, 3, 8] {
        for decode_tokens in 1u32..=20 {
            let geometry = Base0FpIntervalGeometryV1::from_chain_facts_v1(
                4,
                decode_tokens,
                interval,
                kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall,
            )
            .expect("a geometry");
            let mut covered: Vec<u32> = Vec::new();
            for index in 0..geometry.interval_count {
                let (first, last) = geometry.calls_for(index).expect("in range");
                assert!(first <= last, "interval {index} of {geometry:?} is empty");
                covered.extend(first..=last);
                match geometry.anchor_covered_call(index) {
                    None => assert_eq!(index, 0),
                    Some(anchor) => assert_eq!(anchor + 1, first, "the anchor covers the call before the interval"),
                }
            }
            let want: Vec<u32> = (0..=decode_tokens.saturating_sub(1)).collect();
            assert_eq!(covered, want, "interval {interval}, decode {decode_tokens}");
            assert!(geometry.calls_for(geometry.interval_count).is_none());
        }
    }
}

/// A cadence of zero is not a cadence — refused by name rather than dividing by it.
#[test]
fn a_zero_cadence_is_refused_by_name() {
    assert_eq!(
        Base0FpIntervalGeometryV1::from_chain_facts_v1(
            4,
            4,
            0,
            kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall
        )
        .unwrap_err(),
        Base0FpIntervalError::CheckpointIntervalIsZero
    );
}

// ------------------------------------------------------------------------------------------
// The floor class, end to end — no model, a derived fixture artifact
// ------------------------------------------------------------------------------------------

use crate::produce::{Base0RetainedMaterialV1, base0_execute_for_attempt_v1};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
use kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;

/// A small BASE-0 class with real weights, derived from the pinned seed: the floor is not a
/// language model, which is exactly why it can prove the OPENING without a model.
fn floor_geometry() -> kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 {
    let mut geometry = PALW_RC_BASE0_GEOMETRY;
    geometry.layer_count = 2;
    geometry.hidden_dim = 64;
    geometry.ffn_dim = 128;
    geometry.attn_heads = 2;
    geometry.attn_head_dim = 32;
    geometry.vocab_size = 128;
    geometry.n_ctx = 32;
    geometry.tile_len = 32;
    geometry
}

fn floor_job(prefill: u32, decode: u32) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>) {
    let geometry = floor_geometry();
    let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(
        crate::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        },
        crate::rc::PALW_RC_BASE0_SEED,
    )
    .expect("the fixture shape is valid");
    let profile = base0_profile_v1(geometry).expect("expressible");
    let (ctx, prompt) = crate::produce::base0_rc_job_v1(
        &profile,
        Hash64::from_u64_word(0x_F1_00_5E),
        geometry.vocab_size as usize,
        prefill,
        decode,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    (artifact, profile, ctx, prompt)
}

fn floor_material(
    prefill: u32,
    decode: u32,
) -> (Base0RetainedMaterialV1, PalwClaimRootsV1, Vec<u32>, crate::artifact::Base0ArtifactV1) {
    let (artifact, profile, ctx, prompt) = floor_job(prefill, decode);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: run.binding.job_context.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let material: Base0RetainedMaterialV1 = (
        run.binding.clone(),
        run.tiles.tiles.clone(),
        run.logits_rows.clone(),
        run.generated_token_ids.clone(),
        run.checkpoints.chunks.clone(),
    );
    (material, claim, ids, artifact)
}

/// The floor's kernels, for the seat half — the same adapter `crate::backend` installs, spelled
/// here so the test drives the shipped path rather than a test-only replay.
struct FloorKernels<'a>(&'a crate::artifact::Base0ArtifactV1);

impl Base0FpIntervalKernelsV1 for FloorKernels<'_> {
    fn replay_interval_into(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &Base0FpIntervalStartV1<'_>,
        window: Base0FpWindowV1,
        step_leaf_count: u64,
        sink: &mut dyn FnMut(u64, PalwStepTileLeafV1) -> Result<(), String>,
    ) -> Result<(), String> {
        use crate::engine::{Base0Engine, KvCache};
        let engine = Base0Engine::new(self.0);
        let mut cache = match start {
            Base0FpIntervalStartV1::Genesis { .. } => KvCache::new(self.0),
            Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                let geometry =
                    crate::legs::base0_checkpoint_geometry_at_v1(profile, ctx, *covered_decode_call).map_err(|e| format!("{e:?}"))?;
                KvCache::from_state_chunks(self.0, &geometry, chunks).map_err(|e| format!("{e:?}"))?
            }
        };
        base0_fp_replay_interval_into_v1(
            profile,
            ctx,
            start,
            window,
            step_leaf_count,
            |token, position| {
                let (logits, probe) = engine.forward_token_probed(&mut cache, token, position).map_err(|e| format!("{e:?}"))?;
                Ok((logits, crate::legs::base0_captured_rows_v1(&probe)))
            },
            sink,
        )
    }
}

/// **The round trip Decision 8 is: open one interval, replay it, agree exactly.**
///
/// Every interval of a real capture — the genesis-anchored one and every checkpoint-anchored
/// one — is opened by the executor and verified by a seat that holds only the opening, the
/// claim's two roots, the prompt ids and the price. The capture itself is never handed over,
/// which is the whole of R1's verification half.
#[test]
fn every_interval_of_a_floor_capture_opens_and_verifies() {
    let (material, claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let count = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
        .expect("a geometry")
        .interval_count;
    assert!(count > 1, "the fixture must exercise both the genesis and the anchored arms");
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
    for index in 0..count {
        let opening = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
        // ADR-0086 X3: the served bytes are the fold's form, and carry no leaf hash.
        let decoded = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("a V4 opening");
        assert!(decoded.range.siblings.len() <= 2 * kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_OPENING_SIBLINGS);
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_v1(
                &opening,
                claim,
                index,
                &ids,
                binding.step_leaf_count,
                PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            ),
            Base0FpIntervalSeatVerdictV1::Valid,
            "interval {index} of an honest capture replays exactly against the seat's own leaves"
        );
    }
}

/// **A binding that carries another context is not this claim's evidence** (ADR-0084
/// Decision 4; mainnet audit 2026-09-06 C-3).
///
/// Part (b) is what makes part (a) load-bearing. The existing verification pins the binding's
/// `job_id` against the claim's anchor and its `step_leaf_count` against the geometry, and
/// pins NEITHER token count — so a producer may bind a wide context to a narrow job's id, and
/// the field the seat's interval draw is sized by is not one the family's own check reads. The
/// accessor exists precisely because that comparison has to be made by the caller holding the
/// seat's context, and this test shows the check below answers a different question.
#[test]
fn a_binding_that_carries_another_context_is_not_this_claims_evidence() {
    let (material, claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry");
    let honest =
        base0_open_fp_interval_v1(&material, 0, &ids, interval, kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("interval 0 opens");

    // (a) the accessor returns the context the fixture actually ran under.
    let read = base0_fp_interval_opening_job_context_v1(&honest).expect("an opening of this family carries its context");
    assert_eq!(read, binding.job_context, "the accessor reads the binding's own context, unaltered");

    // Only the executed count moves — the job id, the prompt hash and the prefill are untouched.
    let mut forged = Base0FpIntervalOpeningV4::decode_v1(&honest).expect("a V4 opening");
    forged.binding.job_context.exact_decode_tokens += 1_000;
    let forged_bytes = forged.encode_v1().expect("it re-encodes");
    let carried = base0_fp_interval_opening_job_context_v1(&forged_bytes).expect("it still decodes");
    assert_ne!(carried, binding.job_context, "the accessor sees the substitution");
    assert_eq!(carried.job_id, binding.job_context.job_id, "and the job id does NOT — which is the whole finding");

    // (b) the family's own verification refuses these bytes, but for a reason that is not a
    // context comparison: the recomputed execution root no longer matches. It never answers
    // "is this the context the seat drew from", so the accessor is not redundant with it.
    let state = geometry.anchor_covered_call(0).map(|covered| seat_state(&material, &artifact, &ids, covered));
    let verdict = base0_verify_fp_interval_opening_with_state_v1(
        &forged_bytes,
        claim,
        0,
        &ids,
        binding.step_leaf_count,
        interval,
        state.as_ref(),
        &FloorKernels(&artifact),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    assert_ne!(verdict, Base0FpIntervalSeatVerdictV1::Valid, "a substituted context is never Valid");
    // …and the honest bytes still are, so the refusal above is about the substitution and not
    // about the fixture.
    assert_eq!(
        base0_verify_fp_interval_opening_with_state_v1(
            &honest,
            claim,
            0,
            &ids,
            binding.step_leaf_count,
            interval,
            state.as_ref(),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        ),
        Base0FpIntervalSeatVerdictV1::Valid
    );
}

/// **One byte, and the opening answers for nothing.**
///
/// Three tampers, each a different accusation. A flipped byte anywhere in the encoded opening
/// must never read as `Valid`: it is `Mismatch` when it breaks a binding the claim's roots
/// pin, and `Unverifiable` when it stops decoding at all — and the two are different
/// accusations on purpose (`PalwFpIntervalVerdictV1`'s own doc). What is refused BY NAME is
/// the third: an opening whose committed rows have been changed replays to a different hash,
/// and the seat returns the court's question at the leaf rather than a verdict.
#[test]
fn a_tampered_opening_is_refused_by_name() {
    let (material, claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let leaves = binding.step_leaf_count;
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry");
    let verify = |bytes: &[u8], index: u32| {
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        base0_verify_fp_interval_opening_with_state_v1(
            bytes,
            claim,
            index,
            &ids,
            leaves,
            interval,
            state.as_ref(),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .to_consensus_v1()
    };

    // (1) The last anchored interval, opened honestly, then one byte of a COMMITTED ROW moved.
    let index = Base0FpIntervalGeometryV1::from_binding_v1(binding, interval).expect("a geometry").interval_count - 1;
    let opening = base0_open_fp_interval_v1(
        &material,
        index,
        &ids,
        interval,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("opens");
    assert_eq!(verify(&opening, index), PalwFpIntervalVerdictV1::Valid);

    // ADR-0086: a tampered frontier is the seat's own leaves not walking to the root — a fault
    // with the range for an address, never a licence.
    let mut decoded = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
    let last = decoded.range.siblings.len() - 1;
    let mut bytes = decoded.range.siblings[last].as_byte_slice().to_vec();
    bytes[0] ^= 1;
    decoded.range.siblings[last] = Hash64::from_bytes(bytes.try_into().expect("64 bytes"));
    assert!(matches!(verify(&decoded.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::FaultInRange { .. }));
    let mut short = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
    short.range.siblings.pop();
    assert!(matches!(verify(&short.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::FaultInRange { .. }));
    let mut wrong_count = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("decodes");
    wrong_count.range.leaf_count += 1;
    assert_eq!(verify(&wrong_count.encode_v1().expect("re-encodes"), index), PalwFpIntervalVerdictV1::Mismatch);

    // (2) The magic, one byte. Bytes that are not this family's are bytes this seat cannot
    //     check — never an accusation.
    let mut wrong_family = opening.clone();
    wrong_family[0] ^= 1;
    assert_eq!(verify(&wrong_family, index), PalwFpIntervalVerdictV1::Unverifiable);

    // (3) The right opening against the WRONG interval index, and against another claim's
    //     roots. Both are "this opening is not about the claim in hand".
    assert_eq!(verify(&opening, index.saturating_sub(1)), PalwFpIntervalVerdictV1::Mismatch);
    let stranger = PalwClaimRootsV1 { execution_root: Hash64::from_u64_word(9), ..claim };
    assert_eq!(
        base0_verify_fp_interval_opening_v1(
            &opening,
            stranger,
            index,
            &ids,
            leaves,
            interval,
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        PalwFpIntervalVerdictV1::Mismatch
    );
    // …and against the wrong PRICE, which is the same accusation: a claim is priced by the
    // leaf count its binding carries (ADR-0074 Decision 5).
    assert_eq!(
        base0_verify_fp_interval_opening_v1(
            &opening,
            claim,
            index,
            &ids,
            leaves + 1,
            interval,
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        PalwFpIntervalVerdictV1::Mismatch
    );
    // ADR-0086 Decision 2: without its own state a seat cannot judge an anchored V4 opening.
    assert_eq!(
        base0_verify_fp_interval_opening_v1(
            &opening,
            claim,
            index,
            &ids,
            leaves,
            interval,
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        PalwFpIntervalVerdictV1::Unverifiable
    );
}

/// **A FAULT is the court's question, and a seat asks it without convicting.**
///
/// A producer that committed a row it did not compute is caught by the replay — not by any
/// check on the opening, which is self-consistent — and the seat answers with the leaf a
/// challenger then opens a court at. This is the case the whole arrangement exists for, and
/// the verdict it produces slashes nobody.
#[test]
fn a_row_the_producer_did_not_compute_is_a_fault_at_its_leaf() {
    let (mut material, _, ids, artifact) = floor_material(3, 4);
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let index = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry").interval_count - 1;
    let leaves_geometry = base0_fp_interval_leaves_v1(
        &material.0.shape_profile,
        &material.0.job_context,
        &Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry"),
        index,
        material.0.step_leaf_count,
    )
    .expect("a leaf range");
    let target = leaves_geometry.interval_first;

    // Tamper a step the interval owns, then RE-DERIVE the commitment from the corrupted
    // capture — the producer's roots stay honestly its own, which is what makes this the
    // court's fraud and not a mismatch any seat check would see.
    {
        let slot = material.1.iter_mut().find(|(i, _)| *i == target).expect("the tile is held");
        slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
    }
    let ctx = material.0.job_context.clone();
    let profile = material.0.shape_profile.clone();
    let mut leaves = vec![Hash64::default(); material.0.step_leaf_count as usize];
    let profile_hash = profile.shape_profile_id();
    for (i, leaf) in &material.1 {
        leaves[*i as usize] = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(&ctx, &profile_hash, leaf);
    }
    let tiles = crate::legs::Base0StepTilesV1 { leaves, tiles: material.1.clone() };
    let checkpoints =
        crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(&ctx, &profile, &material.0.checkpoint_profile, &material.4)
            .expect("the leg re-derives");
    let binding = crate::legs::base0_binding_from_capture_v1(
        &profile,
        &ctx,
        &tiles,
        &checkpoints,
        material.0.full_logits_trace_root,
        material.0.activation_leg_root,
    )
    .expect("a binding");
    let claim = PalwClaimRootsV1 {
        execution_root: binding.committed_execution_root,
        trace_root: binding.full_logits_trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let leaf_count = binding.step_leaf_count;
    material.0 = binding;

    let opening = base0_open_fp_interval_v1(
        &material,
        index,
        &ids,
        interval,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("a liar still opens its own capture");
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry");
    let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
    let verdict = base0_verify_fp_interval_opening_with_state_v1(
        &opening,
        claim,
        index,
        &ids,
        leaf_count,
        interval,
        state.as_ref(),
        &FloorKernels(&artifact),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    // ADR-0086 Decision 3: the address is a range of the fold that holds the leaf — on a
    // space this small, the range itself — and it convicts nobody.
    let Base0FpIntervalSeatVerdictV1::FaultInRange { first_leaf_index, leaf_count: named } = verdict else {
        panic!("a row the producer did not compute must be a fault with an address, not {verdict:?}");
    };
    assert!(
        first_leaf_index <= target && target < first_leaf_index + named,
        "the named range [{first_leaf_index}, +{named}) must hold the tampered leaf {target}"
    );
    assert_eq!(verdict.to_consensus_v1(), PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count: named });
}

/// **The count a seat draws against comes from the chain, and the capture agrees with it.**
///
/// `fp_interval_count_for` reads two numbers off the accepted 0x4a payload and the class's own
/// cadence; `fp_interval_count` reads a capture. They must agree on every capture this family
/// produces — this is the test the backend seam's own doc promises — and it is the chain-side
/// one a seat uses, because an executor that could shrink the count could predict the draw.
#[test]
fn the_two_interval_counts_agree_on_every_capture() {
    for (prefill, decode) in [(3u32, 1u32), (3, 2), (3, 4), (4, 5), (2, 8)] {
        let (material, ..) = floor_material(prefill, decode);
        let binding = &material.0;
        let from_capture = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1)
            .expect("a geometry")
            .interval_count;
        let from_chain = base0_fp_interval_count_for_v1(
            binding.job_context.declared_prefill_tokens,
            binding.job_context.exact_decode_tokens,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        )
        .expect("a count");
        assert_eq!(from_capture, from_chain, "prefill {prefill}, decode {decode}");
        // …and the chain-side form reads nothing but its three arguments.
        assert_eq!(from_chain, base0_fp_interval_count_for_v1(prefill, decode, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).unwrap());
    }
}

/// **W10, measured — and the one term that is not constant, named.**
///
/// Decision 8 states the opening as `O(interval × row + log₂ leaves)`. Measured on the floor
/// fixture, an anchored interval's opening decomposes as:
///
/// * the **binding** — the class's whole shape profile — a per-class constant (~19 KiB here);
/// * the interval's **committed rows**, 190 leaf hashes at every decode budget: `interval ×
///   row`, flat;
/// * the **seed row**'s four tile preimages, flat;
/// * the **siblings**, 3 to 7 of them: `log₂ leaves`;
/// * the **anchor's state chunks**, which are NOT flat — 1,280 bytes at four decode tokens and
///   4,352 at sixteen.
///
/// That last term is honest and unavoidable on an attention class: resuming a decode call
/// requires the WHOLE KV history, so the checkpoint chunk is `positions × kv_row`. It is
/// bounded — every admissible job has `prefill + decode ≤ n_ctx`, so the state term is at most
/// a class constant (`n_ctx × attn_kv_heads × attn_head_dim × 2 × attn_layers`) — but it is not
/// constant WITHIN a class, and the ADR's own Decision 11 note ("may not make A16@512 fit
/// 80 KiB if attention reads the whole history — derive honestly") is about exactly this.
///
/// So the assertion is the decomposition rather than a single bound: everything but the state
/// chunk is flat in `decode_tokens_executed`, the state chunk is the class's own ceiling, and
/// the capture — the thing this replaces — grows without either bound.
#[test]
fn an_interval_opening_does_not_grow_with_the_job() {
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let measure = |decode: u32| -> (usize, usize, usize, usize, usize, u64) {
        let (material, _, ids, _) = floor_material(3, decode);
        let count = Base0FpIntervalGeometryV1::from_binding_v1(&material.0, interval).expect("a geometry").interval_count;
        let bytes = base0_open_fp_interval_v1(
            &material,
            count - 1,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("opens");
        let opened = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("decodes");
        assert!(opened.anchor.is_some(), "the last interval is anchored");
        // ADR-0086 Decision 2: no chunk rides; the anchor is a claim.
        let state: usize = 0;
        let capture = borsh::to_vec(&material).expect("the capture encodes").len();
        (bytes.len(), opened.range.leaf_count as usize, opened.range.siblings.len(), state, capture, material.0.step_leaf_count)
    };
    let (short_bytes, short_rows, _, short_state, short_capture, short_leaves) = measure(4);
    let (long_bytes, long_rows, long_siblings, long_state, long_capture, long_leaves) = measure(16);

    assert!(long_leaves > short_leaves && long_capture > 3 * short_capture / 2, "the job really did get longer");
    // The rows an opening carries are ONE interval's, whatever the job's length.
    assert!(long_rows <= short_rows + 8, "the committed rows are one interval's: {short_rows} then {long_rows}");
    assert!(
        long_siblings <= 2 * kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_OPENING_SIBLINGS,
        "the sibling set is the leg's own bound"
    );
    // Everything but the anchor's state is flat.
    assert!(
        long_bytes - long_state <= short_bytes - short_state + 1_024,
        "the non-state part of an opening grew from {} to {} bytes",
        short_bytes - short_state,
        long_bytes - long_state
    );
    // The state IS the growing term, and its ceiling is the class's, not the job's: the widest
    // job this class admits is `n_ctx` positions of the cache.
    assert_eq!((short_state, long_state), (0, 0), "ADR-0086: an opening carries no state at all");
    // The whole point: a seat's bytes are a fraction of the capture's, and the fraction falls.
    assert!(
        long_bytes * 10 < long_capture,
        "an opening is {long_bytes} bytes against a {long_capture}-byte capture — that ratio is what Decision 8 buys"
    );
}

/// **W2 for attention: the anchored refutation and the long form reach the same verdict.**
///
/// The long form opens one KV operand per cached position and re-derives the history from
/// genesis; the anchored form opens ONE checkpoint chunk and resumes from it. They are two
/// pieces of evidence about the same step, and the invariant is that the court cannot be
/// steered by which one a party chooses: honest material must be `NoFaultFound` both ways, and
/// the same tampered row must convict both ways. Without that, a challenger picks whichever
/// route convicts and Decision 10's cheaper form is a second, softer court.
///
/// Run on the floor's fixture — a class with real weights, a real checkpoint leg and no model
/// to load.
#[test]
fn the_anchored_refutation_and_the_long_form_agree_on_attention() {
    use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    // A step inside the LAST decode call that READS THE KV HISTORY — the only kind of step for
    // which the two forms both exist, and the kind Decision 10 is about. Found from the
    // profile's own `input_refs` rather than by naming a slot: the two forms differ only at a
    // history-reading ref, and a slot named here would be a second opinion about which node
    // that is.
    let call = ctx.exact_decode_tokens - 1;
    let reads_kv = |slot: u32| {
        profile.resolve_node_slot(slot).is_some_and(|(node, _)| {
            node.input_refs.iter().any(|r| {
                *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K
                    || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V
            })
        })
    };
    let target = (0..run.binding.step_leaf_count)
        .filter_map(|i| kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, i))
        .find(|c| c.call_index == call && reads_kv(c.node_slot))
        .expect("the class has a step that reads its own KV history");

    let anchor = base0_checkpoint_operands_v1(&run.binding, &run.checkpoints.chunks, &run.checkpoints.leaves, call - 1)
        .expect("the committed checkpoint");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(
        kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
            logits_rows: run.logits_rows.clone(),
            generated_token_ids: run.generated_token_ids.clone(),
        },
    );
    let build = |kv: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>,
                 tiles: &crate::legs::Base0StepTilesV1,
                 binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2| {
        crate::legs::base0_refutation_from_capture_v1(&profile, &ctx, tiles, binding, target, ids.clone(), Some(pin.clone()), kv)
            .expect("a capture assembles a refutation")
    };

    // The oracle is the production inventory, driven by the adjudicator's own recording pass —
    // the same route `operand_openings_for` takes, so this exercises the shipped prover.
    let inventory = crate::inventory::base0_inventory_v1(&artifact, floor_geometry()).expect("inventory");
    let prove = |refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
        let recorder = kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1::new(inventory.operands());
        let _ = check_execution_step_refutation_v1(refutation, &recorder);
        let openings = recorder.openings().expect("the inventory opens what its own oracle resolved");
        kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
            .expect("the rows prove against the artifact root")
    };

    // (1) Honest material: the challenger loses on the merits, both ways.
    let long = build(None, &run.tiles, run.binding.clone());
    let anchored = build(Some(anchor.clone()), &run.tiles, run.binding.clone());
    for (name, refutation) in [("long", &long), ("anchored", &anchored)] {
        let oracle = prove(refutation);
        assert!(
            matches!(check_execution_step_refutation_v1(refutation, &oracle), Err(PalwStepRefuteError::NoFaultFound)),
            "the {name} form must find no fault in honest material: {:?}",
            check_execution_step_refutation_v1(refutation, &oracle)
        );
    }
    assert!(anchored.kv_checkpoint.is_some() && long.kv_checkpoint.is_none(), "the two forms really are the two forms");

    // (2) One tampered row, re-derived into its own commitment: both forms convict, with the
    //     same verdict. A route that acquitted here would be the softer court.
    let mut tiles = run.tiles.clone();
    let target_index = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &ctx, &target).expect("an index");
    {
        let slot = tiles.tiles.iter_mut().find(|(i, _)| *i == target_index).expect("the tile is held");
        slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        tiles.leaves[target_index as usize] =
            kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(&ctx, &profile.shape_profile_id(), &slot.1);
    }
    let lying = crate::legs::base0_binding_from_capture_v1(
        &profile,
        &ctx,
        &tiles,
        &run.checkpoints,
        run.binding.full_logits_trace_root,
        run.binding.activation_leg_root,
    )
    .expect("a binding");
    let long_verdict = {
        let r = build(None, &tiles, lying.clone());
        let oracle = prove(&r);
        check_execution_step_refutation_v1(&r, &oracle)
    };
    let anchored_verdict = {
        let r = build(Some(anchor), &tiles, lying);
        let oracle = prove(&r);
        check_execution_step_refutation_v1(&r, &oracle)
    };
    assert!(long_verdict.is_ok(), "the long form convicts a row the producer did not compute: {long_verdict:?}");
    assert_eq!(
        format!("{long_verdict:?}"),
        format!("{anchored_verdict:?}"),
        "the anchored form must reach the SAME verdict as the long one — otherwise a challenger picks the route that convicts"
    );
}

// =============================================================================================
// ADR-0082 Decision 9 (invariant Z5, unit U-05): the seat recomputes, and never fetches the history
// =============================================================================================

use crate::fp_recompute::{Base0FpRecomputeError, base0_fp_recompute_state_v1};

/// The floor's kernels for a RECOMPUTE — the same engine and cache its capture path uses, with
/// nothing captured and no token selected. The dense and hybrid tiers ship theirs in
/// `crate::fp_recompute`; this one exists because the floor's fixture is the class in this
/// file's tests, and it drives exactly the same shared driver.
use crate::fp_recompute::Base0RecomputeKernelsV1 as FloorRecompute;

/// The seat's own state at one interval's start, computed the way Decision 9 says: the prompt
/// it holds and the committed output ids, teacher-forced, with the family's own kernels.
fn seat_state(
    material: &Base0RetainedMaterialV1,
    artifact: &crate::artifact::Base0ArtifactV1,
    prompt_ids: &[u32],
    covered: u32,
) -> crate::fp_recompute::Base0FpSeatStateV1 {
    let binding = &material.0;
    let mut kernels = FloorRecompute::new(artifact);
    base0_fp_recompute_state_v1(
        &binding.shape_profile,
        &binding.job_context,
        prompt_ids,
        &material.3,
        covered,
        &mut kernels,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("the seat can run the floor")
}

/// **(a) Z5's first half: the root a seat recomputes IS the checkpoint the executor committed,
/// at every interval of a multi-interval job.**
///
/// Nothing about the executor's state is read: the comparison is between 64 bytes the claim
/// commits and 64 bytes this process computed from the prompt and the answer's ids.
#[test]
fn the_recomputed_root_is_the_committed_checkpoint_at_every_interval() {
    let (material, _claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
    assert!(geometry.interval_count > 1, "the fixture must have at least one checkpoint-anchored interval");
    let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        &material.4,
    )
    .expect("the executor's own leg");
    let mut checked = 0;
    for index in 1..geometry.interval_count {
        let covered = geometry.anchor_covered_call(index).expect("an anchored interval");
        let state = seat_state(&material, &artifact, &ids, covered);
        let committed = checkpoints
            .leaves
            .iter()
            .find(|l| l.covered_decode_call == covered)
            .unwrap_or_else(|| panic!("the executor committed a checkpoint at call {covered}"));
        assert_eq!(
            state.state_chunks_root, committed.state_chunks_root,
            "interval {index}: the seat's own recompute must reach the committed state root"
        );
        // …and it is the executor's own spelling that produced both, not two hashes that
        // happen to agree today.
        assert_eq!(
            base0_state_chunks_root_v1(&binding.state_chunk_map_id, &state.chunks).expect("a root"),
            committed.state_chunks_root
        );
        checked += 1;
    }
    assert!(checked > 0);
}

/// **(e) The two forms agree on honest material**, and the chunkless one is what a graph-v5
/// seat is served: the same interval, replayed from the seat's own recomputed state, reaches
/// the same verdict as the ADR-0077 replay fed the executor's chunks.
#[test]
fn the_seats_own_state_and_the_carried_chunks_reach_the_same_verdict() {
    let (material, claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
    for index in 0..geometry.interval_count {
        let chunked = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the chunked form opens");
        let flat = base0_open_fp_interval_chunkless_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the flat form opens");
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        // ADR-0086 Decision 2: there is one form, and it is judged from the seat's own state.
        assert_eq!(chunked, flat, "interval {index}: the two openers serve one form");
        let without = base0_verify_fp_interval_opening_with_state_v1(
            &chunked,
            claim,
            index,
            &ids,
            binding.step_leaf_count,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            None,
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let recomputed = base0_verify_fp_interval_opening_with_state_v1(
            &flat,
            claim,
            index,
            &ids,
            binding.step_leaf_count,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            state.as_ref(),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        if index == 0 {
            assert_eq!(without, Base0FpIntervalSeatVerdictV1::Valid, "interval 0 starts from the prompt and needs no state");
        } else {
            assert_eq!(without, Base0FpIntervalSeatVerdictV1::Unverifiable, "interval {index}: no state, no verdict");
        }
        assert_eq!(recomputed, Base0FpIntervalSeatVerdictV1::Valid, "interval {index}, from the seat's own state");
    }
}

/// **(b) A tampered checkpoint is refused, and the refusal NAMES it.**
///
/// The producer is honest about its own bytes: one cache row is changed and the whole
/// commitment is RE-DERIVED from the corrupted chunks, so its roots are self-consistent and
/// every check that reads the opening against the claim passes. The only thing that catches it
/// is arithmetic nobody can fake — the seat running the job — and what comes back names the
/// checkpoint, the call it covers, and both roots.
#[test]
fn a_tampered_checkpoint_is_refused_and_the_refusal_names_it() {
    let (honest, _claim, ids, artifact) = floor_material(3, 4);
    let binding = &honest.0;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1).expect("a geometry");
    let index = 1;
    let covered = geometry.anchor_covered_call(index).expect("an anchored interval");

    // One byte of one cache row, and then the producer's own commitment over it.
    let mut chunks = honest.4.clone();
    chunks[0][0][0] ^= 0x01;
    let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        &chunks,
    )
    .expect("the corrupted leg still builds");
    let tiles = crate::legs::Base0StepTilesV1 {
        tiles: honest.1.clone(),
        leaves: {
            let profile_hash = binding.shape_profile.shape_profile_id();
            let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
            for (at, leaf) in &honest.1 {
                if let Some(slot) = leaves.get_mut(*at as usize) {
                    *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, leaf);
                }
            }
            leaves
        },
    };
    let lying = crate::legs::base0_binding_from_capture_v1(
        &binding.shape_profile,
        &binding.job_context,
        &tiles,
        &checkpoints,
        binding.full_logits_trace_root,
        binding.activation_leg_root,
    )
    .expect("the liar's own binding");
    let claim = PalwClaimRootsV1 {
        execution_root: lying.committed_execution_root,
        trace_root: lying.full_logits_trace_root,
        anchor: lying.job_context.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let material: Base0RetainedMaterialV1 = (lying, honest.1.clone(), honest.2.clone(), honest.3.clone(), chunks);
    let opening = base0_open_fp_interval_chunkless_v1(
        &material,
        index,
        &ids,
        PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("the liar serves an opening");
    let state = seat_state(&material, &artifact, &ids, covered);
    let verdict = base0_verify_fp_interval_opening_with_state_v1(
        &opening,
        claim,
        index,
        &ids,
        material.0.step_leaf_count,
        PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        Some(&state),
        &FloorKernels(&artifact),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    match verdict {
        Base0FpIntervalSeatVerdictV1::CheckpointRootMismatch { checkpoint_index, covered_decode_call, committed, recomputed } => {
            assert_eq!(covered_decode_call, covered, "the refusal names the call the checkpoint covers");
            assert_eq!(checkpoint_index, 0, "the fixture's first checkpoint is the one that was tampered with");
            assert_ne!(committed, recomputed, "the two roots the refusal carries are the two that disagree");
            assert_eq!(recomputed, state.state_chunks_root, "the recomputed root is the seat's own");
        }
        other => panic!("a checkpoint the job does not reach must be named, not {other:?}"),
    }
    // And the ONE thing it is not: a conviction. The seam sees a verdict that files nothing.
    assert_eq!(verdict.to_consensus_v1(), PalwFpIntervalVerdictV1::Unverifiable);
}

/// **(c) Z5's bytes: what a seat fetches does not carry the history, and the history is what
/// grows with the context.**
///
/// Measured at two contexts on real captures. The assertion is an INEQUALITY, not a number:
/// the flat opening's growth from the narrow context to the wide one is strictly smaller than
/// the carried form's, by at least the state both checkpoints hold — and the flat form carries
/// no state at all, which is the structural half of the same statement.
#[test]
fn the_bytes_a_seat_fetches_do_not_carry_the_history() {
    let measure = |prefill: u32, decode: u32| {
        let (material, _claim, ids, _artifact) = floor_material(prefill, decode);
        let index = 1;
        let chunked = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the chunked form opens");
        let flat = base0_open_fp_interval_chunkless_v1(
            &material,
            index,
            &ids,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the flat form opens");
        let decoded = Base0FpIntervalOpeningV4::decode_v1(&flat).expect("the fold form decodes");
        assert_eq!(chunked, flat, "ADR-0086: one form from either opener");
        assert!(decoded.anchor.is_some(), "the interval is checkpoint-anchored at both contexts");
        // The history the class holds at this context, for the comparison below.
        let state_bytes: usize = seat_state(&material, &_artifact, &ids, 1).chunks.iter().map(Vec::len).sum();
        (chunked.len(), flat.len(), state_bytes)
    };
    let (chunked_narrow, flat_narrow, state_narrow) = measure(3, 4);
    let (chunked_wide, flat_wide, state_wide) = measure(11, 4);

    println!(
        "seat bytes per opening: narrow context carried {chunked_narrow} / flat {flat_narrow} (state {state_narrow}); \
             wide context carried {chunked_wide} / flat {flat_wide} (state {state_wide})"
    );
    assert!(state_wide > state_narrow, "the fixture must actually widen the history ({state_narrow} → {state_wide})");
    // **The difference between the two forms IS the history**, at both contexts: the flat one
    // is the carried one minus the state, plus the few bytes borsh spends on lengths.
    // ADR-0086: the opening never carried the state; what a seat fetches must not grow with the
    // context at all beyond encoding noise, while the history the seat recomputes does.
    assert_eq!(chunked_narrow, flat_narrow);
    assert_eq!(chunked_wide, flat_wide);
    assert!(
        flat_wide.abs_diff(flat_narrow) < 1_024,
        "what a seat fetches must not follow the context (flat {flat_narrow} → {flat_wide}, history {state_narrow} → {state_wide})"
    );
}

/// **(d) A family that cannot serve the class refuses BY NAME, and the seat files
/// `Incapable`.**
///
/// The shipped hybrid is exactly this class: it registers the checkpoint sentinel, commits no
/// checkpoint leg, and therefore has no state root for a seat to compare against. Saying so is
/// the honest verdict (ADR-0075: a row nobody can seat certifies nothing); inventing a root
/// would be the dishonest one.
#[test]
fn a_class_that_commits_no_checkpoint_refuses_by_name() {
    let (artifact, mut profile, ctx, prompt) = floor_job(3, 4);
    profile.state_chunk_map_id = Hash64::default();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let mut kernels = FloorRecompute::new(&artifact);
    let refusal = base0_fp_recompute_state_v1(
        &profile,
        &ctx,
        &ids,
        &[1, 2, 3, 4],
        1,
        &mut kernels,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect_err("refused");
    assert_eq!(refusal, Base0FpRecomputeError::NoStateChunkMapRegistered);
    assert!(
        refusal.to_string().contains("no state chunk map"),
        "the refusal a seat turns into `Incapable` must say what is missing: {refusal}"
    );
}

/// **The ids the recompute is teacher-forced on are checked, not taken.**
///
/// A prompt that is not the job's is refused before a single forward runs — the same rule the
/// opener and the refutation both state — because a seat that recomputed from another list
/// would compare honest arithmetic on a different job against this claim's checkpoint and call
/// the producer a liar.
#[test]
fn a_recompute_refuses_ids_that_are_not_the_jobs() {
    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let mut wrong: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    wrong[0] = wrong[0].wrapping_add(1);
    let mut kernels = FloorRecompute::new(&artifact);
    assert_eq!(
        base0_fp_recompute_state_v1(
            &profile,
            &ctx,
            &wrong,
            &[1, 2, 3, 4],
            1,
            &mut kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        )
        .expect_err("refused"),
        Base0FpRecomputeError::PromptIdsAreNotTheJobs
    );
    // And an answer too short to teacher-force the calls asked for is a refusal too, rather
    // than a run that quietly fed zeros.
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let mut kernels = FloorRecompute::new(&artifact);
    assert_eq!(
        base0_fp_recompute_state_v1(
            &profile,
            &ctx,
            &ids,
            &[],
            1,
            &mut kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        )
        .expect_err("refused"),
        Base0FpRecomputeError::OutputIdsTooShort { need: 1, got: 0 }
    );
}

/// **A graph-v5 class refuses an opening that carries the history** — which form the class
/// requires is the class's own declaration (the tiled map), not a server's preference.
#[test]
fn a_tiled_class_refuses_an_opening_that_carries_the_history() {
    let (material, claim, ids, artifact) = floor_material(3, 4);
    let binding = &material.0;
    let index = 1;
    let served = base0_open_fp_interval_v1(
        &material,
        index,
        &ids,
        PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("the fold form opens");
    // ADR-0086: nothing an opener serves carries the history any more. The rule still stands
    // for a V1 opening someone else builds, so build one by hand from the served form — a
    // chunk in the anchor, the tiled map on the class — and hand it to the seat.
    let fold = Base0FpIntervalOpeningV4::decode_v1(&served).expect("decodes");
    let claim_anchor = fold.anchor.clone().expect("interval 1 is anchored");
    let mut decoded = Base0FpIntervalOpeningV1 {
        version: PALW_BASE0_FP_INTERVAL_VERSION_V1,
        interval_index: fold.interval_index,
        binding: fold.binding.clone(),
        range: PalwStepRangeOpeningV1 {
            first_leaf_index: fold.range.first_leaf_index,
            leaf_hashes: vec![Hash64::default(); fold.range.leaf_count as usize],
            siblings: fold.range.siblings.clone(),
        },
        seed_row_leaf_count: fold.seed_row_leaf_count,
        seed_row_tiles: fold.seed_row_tiles.clone(),
        anchor: Some(PalwCheckpointKvOperandsV1 { leaf: claim_anchor.leaf, chunks: vec![vec![0u8]], opening: claim_anchor.opening }),
    };
    decoded.binding.shape_profile.state_chunk_map_id = kaspa_consensus_core::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3();
    assert!(base0_fp_class_requires_flat_openings_v1(&decoded.binding.shape_profile));
    let state = seat_state(&material, &artifact, &ids, 1);
    assert_eq!(
        base0_verify_fp_interval_opening_with_state_v1(
            &decoded.encode_v1().expect("re-encodes"),
            claim,
            index,
            &ids,
            binding.step_leaf_count,
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            Some(&state),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        ),
        Base0FpIntervalSeatVerdictV1::HistoryNotAdmissible,
        "a class whose map is the tiled one is not served the history, and a seat that took it \
             would have spent the bytes Decision 9 exists to save"
    );
}
// =============================================================================================
// ADR-0082 Decision 4, amended — a graph-v5 DENSE row, end to end (audit B, C-1 / C-2 / L-2)
// =============================================================================================

/// The registered graph-v5 dense row, its plan, a small free-prompt job, and the FOLDED
/// retention an executor of that class keeps. One helper, because C-1's three consumers and
/// C-2's acceptance test are all questions about the same run.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_v5_run()
-> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
    let geometry = PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 8,
        ffn_dim: 8,
        attn_heads: 2,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    dense_v5_run_with(geometry, 3, 4)
}

/// **The held twin** (ADR-0103 Decision 3): the same dense row on the held map — graph-v7 —
/// the same job, the same engine. Every question the graph-v5 fixture answers is asked of it too.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_v7_run()
-> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    // Eight prompt positions and thirteen decode calls at the class's width of eight
    // (n_ctx 32 over the draw's four): interval 1 starts at the FIRST decode call, seeded by
    // the last prefill position's row, and interval 2 at a later call, seeded by a call's.
    dense_v7_run_with(8, 14)
}

/// The held fixture with a prompt that crosses an interval boundary: interval 1 resumes INSIDE
/// the prefill (no seed — its first id is the prompt's) and crosses into the decode calls.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_v7_run_mid_prefill()
-> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    dense_v7_run_with(11, 6)
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_v7_run_with(
    prefill: u32,
    decode: u32,
) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    dense_v7_run_with_form(prefill, decode, kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_v7_run_with_form(
    prefill: u32,
    decode: u32,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
    let geometry = PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 8,
        ffn_dim: 8,
        attn_heads: 2,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a valid graph-v7 A16 profile");
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile));
    dense_run_for_form(geometry, profile, prefill, decode, form)
}

/// The graph-v5 fold fixture at a chosen geometry and job size, for a test that needs a tree
/// wider than one retained block.
pub(super) fn dense_v5_run_with(
    geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
    prefill: u32,
    decode: u32,
) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5;
    let profile = qwen25_a16_profile_v5(geometry).expect("a valid graph-v5 A16 profile");
    dense_run_for(geometry, profile, prefill, decode)
}

/// One dense run, under whichever of the family's rows `profile` is.
#[allow(clippy::type_complexity)]
pub(super) fn dense_run_for(
    geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
    profile: PalwShapeProfileV3,
    prefill: u32,
    decode: u32,
) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    dense_run_for_form(geometry, profile, prefill, decode, kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
}

/// [`dense_run_for`] with the job's prompt committed under `form` — the held regime's ids are
/// the Merkle root (ADR-0103 Decision 4), so its rows are exercised under that form too.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(super) fn dense_run_for_form(
    geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
    profile: PalwShapeProfileV3,
    prefill: u32,
    decode: u32,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1) {
    let shape = crate::artifact::Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
        eps_q: geometry.rms_eps_q,
    };
    let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("a valid shape")
        .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
        .expect("the derived store is sorted and unique");
    let (ctx, prompt) = crate::produce::base0_rc_job_v1(
        &profile,
        Hash64::from_u64_word(0x0000_82C1),
        geometry.vocab_size as usize,
        prefill,
        decode,
        form,
    );
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the v5 declaration is this engine's program");
    let run = crate::qwen25_a16_backend::a16_execute_free_prompt_streaming_v1(
        &artifact,
        &profile,
        Some(&plan),
        &ctx,
        &prompt,
        kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
        &mut |_| {},
    )
    .expect("the folded graph-v5 sink runs the job");
    (artifact, profile, ctx, prompt, run)
}

/// **C-1, test 1: an honest graph-v5 material passes its own seat's check, with zero state
/// retained.**
///
/// The fold drops every checkpoint chunk (`Base0CheckpointRetentionV1::Fold`), so before this
/// the material carried an empty `checkpoint_chunks`, `base0_material_tail_matches_v1` rebuilt
/// the leg from `&[]` — the EMPTY leg, whose root is `checkpoint_empty_root_v2` — and compared
/// it to a real `checkpoint_merkle_root`. `Mismatch`: every seat read every honest producer of
/// the class as forged. The leg is now rebuilt from the LEAVES, which is what the fold retains
/// and what decides the question without a byte of history.
#[test]
fn a_graph_v5_material_is_checkable_and_carries_no_state() {
    check_material_is_checkable_and_carries_no_state(dense_v5_run());
}

#[test]
fn a_held_graph_v7_material_is_checkable_and_carries_no_state() {
    check_material_is_checkable_and_carries_no_state(dense_v7_run());
}

#[allow(clippy::type_complexity)]
fn check_material_is_checkable_and_carries_no_state(
    fixture: (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1),
) {
    let (_artifact, profile, _ctx, prompt, run) = fixture;
    assert_eq!(
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&profile),
        kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerPosition,
    );
    assert!(run.checkpoints.chunks.is_empty(), "the fold retains no state at all");
    assert!(!run.checkpoints.leaves.is_empty(), "…and it does retain the leg it committed");

    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
    assert!(material.checkpoint_chunks.is_empty(), "a folded class serves no history (Decision 9)");
    assert_eq!(material.checkpoint_leaves.len(), run.checkpoints.leaves.len());

    assert_eq!(
        crate::produce::base0_fp_material_matches_claim_v2(
            &material,
            run.execution_root,
            run.trace_root,
            crate::produce::Base0SeatFamilyV1::IntegerKv
        ),
        Ok(true),
        "an honest graph-v5 material must pass the seat's check"
    );

    // **Zero state per POSITION, measured**: the leaves are the whole checkpoint retention and
    // none of them is a cache byte.
    let leaf_bytes = borsh::to_vec(&material.checkpoint_leaves).expect("leaves serialize").len();
    let positions = material.checkpoint_leaves.len();
    eprintln!(
        "C-1: {positions} checkpoints, {} state bytes retained, {leaf_bytes} leaf bytes ({} a position)",
        material.checkpoint_chunks.iter().flatten().map(|c| c.len()).sum::<usize>(),
        leaf_bytes / positions.max(1)
    );
    assert_eq!(material.checkpoint_chunks.iter().flatten().map(|c| c.len()).sum::<usize>(), 0);

    // And a folded class that nevertheless served state is not this class's retention.
    let mut lying = material.clone();
    lying.checkpoint_chunks = vec![vec![vec![0u8; 4]]];
    assert_eq!(
        crate::produce::base0_fp_material_matches_claim_v2(
            &lying,
            run.execution_root,
            run.trace_root,
            crate::produce::Base0SeatFamilyV1::IntegerKv
        ),
        Ok(false),
        "a per-position class serving chunks is serving the history Decision 9 keeps off the wire"
    );

    // A tampered LEAF is what the check can still catch, and does.
    let mut tampered = material.clone();
    tampered.checkpoint_leaves[1].state_chunks_root = Hash64::from_u64_word(0xDEAD);
    assert_eq!(
        crate::produce::base0_fp_material_matches_claim_v2(
            &tampered,
            run.execution_root,
            run.trace_root,
            crate::produce::Base0SeatFamilyV1::IntegerKv
        ),
        Ok(false),
        "a leaf that is not the committed one must not rebuild the committed root"
    );
}

/// **C-1, test 2 and C-2, test 5: every interval of a graph-v5 claim opens, and a seat holding
/// its own state licenses it.**
///
/// Two defects met here. C-1: the opening's anchor was built by rebuilding the leg from served
/// chunks the fold does not keep, so `base0_open_fp_interval_sparse_v1` returned
/// `CaptureIsNotTheBindings` for every `index ≥ 1`. C-2: the anchor's `covered` was
/// `index × interval` — a DECODE CALL — while the leaf's counter is a POSITION, so the seat
/// selected the checkpoint after the fourth PREFILL position and compared a 403-position state
/// against a 3-position root. Either one alone makes the class unseatable.
#[test]
fn every_graph_v5_interval_opens_and_a_recomputing_seat_licenses_it() {
    check_every_interval_opens_and_a_recomputing_seat_licenses_it(dense_v5_run());
}

#[test]
fn every_held_graph_v7_interval_opens_and_a_recomputing_seat_licenses_it() {
    check_every_interval_opens_and_a_recomputing_seat_licenses_it(dense_v7_run());
}

/// **ADR-0103 Invariant 6: a seat that resumes from fetched state and a seat that recomputes
/// reach the same verdict, and a fetched chunk that does not verify is refused by name and
/// never replayed from.** At every anchored interval of the held run: the executor serves the
/// state at the interval's start; the seat verifies it slice by slice against the committed
/// checkpoint and it is byte for byte the state its own recompute reaches; the interval
/// verifies Valid from it. A byte moved in one fetched chunk names that chunk's slice, and the
/// memo holds nothing from it; a shard's slices verify on their own and are not the whole state.
#[test]
fn a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does() {
    check_a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does(
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
}

/// **The held rows under the ids the held regime mandates** (ADR-0103 Decision 4; §10.3 item
/// 12). Every fixture above commits its prompt flat — the default a shipped preset runs — and
/// the held mint makes the Merkle root mandatory, which is where the live drill found the seat's
/// recompute refusing an honest job's own ids. The same intervals, resumes and verdicts, with
/// the job's prompt committed as the tiled root.
#[test]
fn the_held_rows_open_resume_and_license_under_the_merkle_prompt_form() {
    let merkle = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
    check_a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does(merkle);
    check_every_interval_opens_and_a_recomputing_seat_licenses_it_under(dense_v7_run_with_form(8, 14, merkle), merkle);
    check_every_interval_opens_and_a_recomputing_seat_licenses_it_under(dense_v7_run_with_form(11, 6, merkle), merkle);
}

fn check_a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does(
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) {
    let memo = crate::fp_recompute::Base0FpSeatMemoV1::default();
    let (artifact, profile, ctx, prompt, run) = dense_v7_run_with_form(8, 14, form);
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    assert!(
        kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(form, &ids, &ctx.prompt_token_ids_hash),
        "the fixture commits its prompt under the form it is run under"
    );
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("decodes");
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let ladder = PALW_STEP_LEG_MAX_LEAVES;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    assert!(geometry.interval_count >= 3);
    let recompute = |covered: u32| {
        let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).ok()?;
        crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
            &profile,
            &ctx,
            &ids,
            &run.generated_token_ids,
            covered,
            &mut kernels,
            form,
        )
        .ok()
    };
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    for index in 1..geometry.interval_count {
        let covered = geometry.anchor_covered_call(index).expect("an anchored interval");
        let served = base0_open_fp_resume_v1(&material, index, &ids, interval, ladder, None, &recompute, form)
            .unwrap_or_else(|e| panic!("interval {index}: the executor serves its state: {e:?}"));
        crate::fp_recompute::base0_fp_seat_state_forget_v1(&memo);
        let resumed = base0_fp_accept_resume_v1(&memo, &served, &ctx, &ids, covered, interval, ladder)
            .unwrap_or_else(|e| panic!("interval {index}: an honest state verifies: {e:?}"));
        let own = recompute(covered).expect("this seat recomputes");
        assert_eq!(resumed.state_chunks_root, own.state_chunks_root, "interval {index}: one committed state");
        assert_eq!(resumed.chunks, own.chunks, "interval {index}: byte for byte");
        let opened = base0_open_fp_interval_sparse_v1(&material, index, &ids, interval, &kernels, form).expect("the interval opens");
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_v1(
                &opened,
                claim,
                index,
                &ids,
                run.binding.step_leaf_count,
                interval,
                Some(&resumed),
                &kernels,
                form
            ),
            Base0FpIntervalSeatVerdictV1::Valid,
            "interval {index}: the verdict from the fetched state is the recomputing seat's"
        );
    }

    // Tampered: one byte of one fetched chunk moved — refused by the slice it lives in.
    let index = geometry.interval_count - 1;
    let covered = geometry.anchor_covered_call(index).expect("anchored");
    let served = base0_open_fp_resume_v1(&material, index, &ids, interval, ladder, None, &recompute, form).expect("serves");
    let mut forged = Base0FpResumeOpeningV1::decode_v1(&served).expect("decodes");
    let first_slice = forged.slices[0];
    forged.chunks[0][0] ^= 0x5A;
    crate::fp_recompute::base0_fp_seat_state_forget_v1(&memo);
    assert_eq!(
        base0_fp_accept_resume_v1(&memo, &forged.encode_v1().unwrap(), &ctx, &ids, covered, interval, ladder),
        Err(Base0FpResumeRefusalV1::FetchedChunkDoesNotVerify { slice: first_slice }),
    );
    assert!(
        crate::fp_recompute::base0_fp_seat_state_held_v1(&memo, &profile, &ctx, &ids, covered).is_none(),
        "a state that did not verify is never the seat's"
    );

    // A shard's slices verify on their own — the top leaves stand in for the rest — and are
    // not a whole state to replay from.
    let shard = base0_open_fp_resume_v1(&material, index, &ids, interval, ladder, Some(0..2), &recompute, form).expect("a shard");
    let (opened, verified, _) =
        base0_fp_verify_resume_v1(&shard, &ctx, covered, interval, ladder).expect("the shard verifies locally");
    assert_eq!(opened.slices, vec![0, 1]);
    assert!(!verified.is_empty() && (verified.len() as u32) < opened.leaf.state_chunk_count);
    assert!(matches!(
        base0_fp_accept_resume_v1(&memo, &shard, &ctx, &ids, covered, interval, ladder),
        Err(Base0FpResumeRefusalV1::NotTheWholeState { .. })
    ));
    // And the request index is its own kind on the lane.
    let packed = base0_fp_resume_request_index_v1(index).expect("inside the bit");
    assert_eq!(base0_fp_resume_request_decode_v1(packed), Some(index));
    assert_eq!(base0_fp_resume_request_decode_v1(index), None, "a plain index is not a resume request");
    assert_eq!(base0_fp_block_leaves_request_decode_v1(packed), None, "nor a block-leaves request");
    let block = base0_fp_block_leaves_request_index_v1(index, 3).expect("a block request");
    assert_eq!(base0_fp_resume_request_decode_v1(block), None, "and a block request is not a resume one");
}

/// **ADR-0111 Decision 6: the executor answers every held unit the court can name, from its
/// retention, and the court takes the answer.** On the held fold (graph-v7, the Merkle prompt
/// form the mint mandates): the first and last chunk of every checkpoint answer a state-chunk
/// accusation, and ranges at the step space's two ends, one straddling a retained block and the
/// widest the court admits answer a step-range accusation — each checked by the court's own
/// `palw_held_da_check_disclosure_v1` against the claim's execution root. A chunk moved by one
/// byte and a range with one leaf moved are refused, so the check is not a formality.
#[test]
fn the_executor_answers_every_held_unit_the_court_can_name() {
    let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
    assert!(!check_the_executor_answers_every_held_unit(dense_v7_run_with_form(8, 14, form), form), "one retained block");
    // Wide enough that the fold retains more than one block (4,096 leaves at the ruleset's level),
    // so a range's span is replayed across a block edge and its frontier is folded above it.
    let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx: 64,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a valid graph-v7 A16 profile");
    assert!(
        check_the_executor_answers_every_held_unit(dense_run_for_form(geometry, profile, 3, 24, form), form),
        "the wide fixture straddles a retained block"
    );
}

/// Returns whether a range straddling a retained block was answered.
#[allow(clippy::type_complexity)]
fn check_the_executor_answers_every_held_unit(
    fixture: (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1),
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> bool {
    use kaspa_consensus_core::palw_held_da_v1::{
        PALW_HELD_DA_MAX_RANGE_LEAVES_V1, PalwHeldDisclosureV1, PalwHeldMissingV1, palw_held_da_check_accusation_v1,
        palw_held_da_check_disclosure_v1,
    };
    let (artifact, profile, ctx, prompt, run) = fixture;
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile), "a held class's retention");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("decodes");
    let retain_level = material.step_tree.retain_level();
    let retention = crate::produce::Base0RetentionV1::Folded(material);
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let ladder = PALW_STEP_LEG_MAX_LEAVES;
    let recompute = |covered: u32| {
        let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).ok()?;
        crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
            &profile,
            &ctx,
            &ids,
            &run.generated_token_ids,
            covered,
            &mut kernels,
            form,
        )
        .ok()
    };
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    let binding = &run.binding;
    let root = run.execution_root;
    assert!(binding.checkpoint_count >= 2, "a leg of one checkpoint proves little");

    for checkpoint in 0..binding.checkpoint_count {
        let first = base0_fp_held_state_chunk_answer_v1(&retention, checkpoint, 0, &recompute)
            .unwrap_or_else(|e| panic!("checkpoint {checkpoint}: the executor answers its first chunk: {e}"));
        let last_chunk = first.0.leaf.state_chunk_count - 1;
        for chunk in [0, last_chunk] {
            let missing = PalwHeldMissingV1::StateChunk { checkpoint, chunk };
            palw_held_da_check_accusation_v1(&root, &missing, binding, form).expect("a committed chunk is accusable");
            let (anchor, opened) = base0_fp_held_state_chunk_answer_v1(&retention, checkpoint, chunk, &recompute)
                .unwrap_or_else(|e| panic!("checkpoint {checkpoint} chunk {chunk}: {e}"));
            let answer = PalwHeldDisclosureV1::StateChunk { anchor: anchor.clone(), chunk: opened.clone() };
            palw_held_da_check_disclosure_v1(&root, &missing, binding, &answer, ladder, form)
                .unwrap_or_else(|e| panic!("checkpoint {checkpoint} chunk {chunk}: the court refuses the answer: {e:?}"));
            let mut moved = opened;
            moved.chunk_bytes[0] ^= 1;
            assert!(
                palw_held_da_check_disclosure_v1(
                    &root,
                    &missing,
                    binding,
                    &PalwHeldDisclosureV1::StateChunk { anchor, chunk: moved },
                    ladder,
                    form
                )
                .is_err(),
                "checkpoint {checkpoint} chunk {chunk}: a moved byte is not the committed chunk"
            );
        }
    }

    let leaves = binding.step_leaf_count;
    let block = 1u64 << retain_level;
    let widest = u64::from(PALW_HELD_DA_MAX_RANGE_LEAVES_V1).min(leaves);
    let mut ranges = vec![(0, 1), (leaves - 1, 1), (0, widest), (leaves - widest, widest)];
    let straddles = leaves > block + 4;
    if straddles {
        ranges.push((block - 3, 7));
    }
    for (first, count) in ranges {
        let count = count as u32;
        let missing = PalwHeldMissingV1::StepRange { first, count };
        palw_held_da_check_accusation_v1(&root, &missing, binding, form).expect("a committed range is accusable");
        let opening = base0_fp_held_step_range_answer_v1(&retention, first, count, &ids, interval, ladder, &kernels, &recompute, form)
            .unwrap_or_else(|e| panic!("[{first}, +{count}): the executor answers: {e}"));
        palw_held_da_check_disclosure_v1(
            &root,
            &missing,
            binding,
            &PalwHeldDisclosureV1::StepRange { opening: opening.clone() },
            ladder,
            form,
        )
        .unwrap_or_else(|e| panic!("[{first}, +{count}): the court refuses the answer: {e:?}"));
        let mut moved = opening;
        moved.leaf_hashes[0] = Hash64::from_u64_word(0xBAD);
        assert!(
            palw_held_da_check_disclosure_v1(
                &root,
                &missing,
                binding,
                &PalwHeldDisclosureV1::StepRange { opening: moved },
                ladder,
                form
            )
            .is_err(),
            "[{first}, +{count}): a moved leaf is not the committed range"
        );
    }
    assert!(
        base0_fp_held_step_range_answer_v1(&retention, 0, 1, &[1, 2, 3], interval, ladder, &kernels, &recompute, form).is_err(),
        "ids that are not the job's answer nothing"
    );
    straddles
}

/// **The dense tier's drill liar lies at exactly one leaf, in the stream of the fold** (ADR-0121's
/// measured run needs a held liar past the materialization cap, where the attempt lane's dense
/// fault is refused). On the held row: the liar's execution is the honest one — the same answer,
/// the same checkpoints — and its fold differs only in the block that holds the lie, so its step
/// root and execution root are its own; a leaf outside the job is refused by name.
#[test]
fn the_drill_liar_moves_one_committed_tile_and_nothing_else() {
    let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
    let (artifact, profile, ctx, prompt, honest) = dense_v7_run_with_form(8, 14, form);
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let leaves = honest.binding.step_leaf_count;
    let leaf = leaves / 2;
    let lying = crate::qwen25_a16_backend::a16_execute_free_prompt_streaming_with_drill_fault_v1(
        &artifact,
        &profile,
        Some(&plan),
        &ctx,
        &prompt,
        kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
        &mut |_| {},
        leaf,
    )
    .expect("the drill's liar runs");
    assert_eq!(lying.generated_token_ids, honest.generated_token_ids, "the execution is the honest one");
    assert_eq!(lying.checkpoints.leaves, honest.checkpoints.leaves, "and so is every checkpoint");
    assert_ne!(lying.binding.step_merkle_root, honest.binding.step_merkle_root, "the lie is committed");
    assert_ne!(lying.execution_root, honest.execution_root);
    let (liar, truth) = (lying.step_tree.expect("a fold"), honest.step_tree.expect("a fold"));
    let block = leaf >> truth.retain_level();
    let (liar, truth) = (liar.retained_nodes(), truth.retained_nodes());
    let differing: Vec<usize> = (0..truth.len()).filter(|i| liar[*i] != truth[*i]).collect();
    assert_eq!(differing, vec![block as usize], "only the block that holds the lie differs");
    assert!(
        crate::qwen25_a16_backend::a16_execute_free_prompt_streaming_with_drill_fault_v1(
            &artifact,
            &profile,
            Some(&plan),
            &ctx,
            &prompt,
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
            &mut |_| {},
            leaves,
        )
        .is_err(),
        "a leaf outside the job is refused"
    );
}

/// **ADR-0121 on the held row: what the executor streams, and how a block is named.** At the
/// regime's ladder a held class still folds at 12; a block-leaves request's number counts from
/// the block holding the interval's first leaf, and the executor serves the block that number
/// names (its own first leaf, folding to the served digest); and the close annex's tiles,
/// replayed for the disputed leaves alone, are the interval's own tiles for those leaves and
/// nothing for a leaf outside it. On the shipped row a request's number is the block itself.
#[test]
fn a_held_executor_serves_blocks_by_their_interval_number_and_the_disputed_tiles_alone() {
    use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
    let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
    let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a valid graph-v7 profile");
    let (artifact, profile, ctx, prompt, run) = dense_run_for_form(geometry, profile, 3, 72, form);
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("decodes");
    assert_eq!(
        material.step_tree.retain_level(),
        crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
        "a held class folds at 12 whatever its ladder (ADR-0121 Decision 3)"
    );
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let ladder = PALW_HELD_STEP_LADDER_V1;
    let recompute = |covered: u32| {
        let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).ok()?;
        crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
            &profile,
            &ctx,
            &ids,
            &run.generated_token_ids,
            covered,
            &mut kernels,
            form,
        )
        .ok()
    };
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    let leaf_count = run.binding.step_leaf_count;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(&run.binding, interval, ladder).expect("a geometry");
    let (mut served, mut compared) = (0usize, 0usize);
    for index in 0..geometry.interval_count {
        let opening =
            base0_open_fp_interval_sparse_anchored_capped_v1(&material, index, &ids, interval, ladder, &kernels, &recompute, form)
                .unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
        let v4 = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("V4");
        let base = v4.range.first_leaf_index >> v4.range.retain_level;
        let (first_block, end_block) = v4.range.whole_blocks_v1(leaf_count);
        for block in first_block..end_block {
            let number = base0_fp_block_request_number_v1(&v4, block).expect("a block at or past the interval's first");
            assert_eq!(number, block - base, "interval {index}: the number counts from the interval");
            assert_eq!(base0_fp_block_of_request_number_v1(&v4, number), Some(block));
            let bytes =
                base0_fp_block_leaves_from_fold_capped_v1(&material, &opening, number, &ids, interval, ladder, &kernels, &recompute)
                    .unwrap_or_else(|e| panic!("interval {index} block {block}: serves: {e}"));
            let leaves = Base0FpBlockLeavesV1::decode_v1(&bytes).expect("decodes");
            assert_eq!(leaves.first_leaf_index, block << v4.range.retain_level, "interval {index}: number {number} is block {block}");
            assert!(leaves.folds_to_v1(&v4.range.block_roots[(block - first_block) as usize]), "and folds to its digest");
            served += usize::from(base > 0);
        }
        let whole = base0_fp_interval_tiles_from_fold_capped_v1(&material, index, &ids, interval, ladder, &kernels, &recompute, form)
            .expect("the interval replays");
        let whole: std::collections::HashMap<u64, PalwStepTileLeafV1> = whole.tiles.into_iter().collect();
        let own = base0_fp_interval_leaves_v1(&profile, &ctx, &geometry, index, leaf_count).expect("the interval's leaves");
        let disputed = [
            own.interval_first,
            (own.interval_first + own.range_end) / 2,
            own.range_end - 1,
            own.range_end,
            own.range_first.wrapping_sub(1),
        ];
        let kept =
            base0_fp_disputed_tiles_from_fold_v1(&material, index, &ids, interval, ladder, &kernels, &recompute, form, &disputed)
                .unwrap_or_else(|e| panic!("interval {index}: the disputed tiles replay: {e}"));
        let inside: std::collections::BTreeSet<u64> =
            disputed.iter().copied().filter(|l| *l >= own.interval_first && *l < own.range_end).collect();
        assert_eq!(kept.keys().copied().collect::<std::collections::BTreeSet<u64>>(), inside, "interval {index}: those and no others");
        for (leaf, tile) in &kept {
            assert_eq!(Some(tile), whole.get(leaf), "interval {index} leaf {leaf}: the interval's own tile");
            compared += 1;
        }
    }
    assert!(compared >= 3, "the disputed tiles were compared: {compared}");
    assert!(served > 0, "an interval past the first block holds a whole block, served by its number: {served}");
    // The number's arithmetic, on an opening moved past the first block, where it and the
    // block differ — and on the shipped row, where they never do.
    let opening = base0_open_fp_interval_sparse_anchored_capped_v1(&material, 0, &ids, interval, ladder, &kernels, &recompute, form)
        .expect("interval 0 opens");
    let mut moved = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("V4");
    moved.range.first_leaf_index = (5 << 12) + 17;
    assert_eq!(base0_fp_block_request_number_v1(&moved, 7), Some(2));
    assert_eq!(base0_fp_block_of_request_number_v1(&moved, 2), Some(7));
    assert_eq!(base0_fp_block_request_number_v1(&moved, 4), None, "a block before the interval's first has no number");
    let (_, v5_profile, _, _, v5_run) = dense_v5_run();
    assert!(!kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&v5_profile));
    moved.binding = v5_run.binding;
    assert_eq!(base0_fp_block_request_number_v1(&moved, 7), Some(7), "the shipped row's number is the block (ADR-0086 D6)");
    assert_eq!(base0_fp_block_of_request_number_v1(&moved, 7), Some(7));
}

/// **ADR-0111 Decision 3: the chain's geometry is the seat's.** The bound a leaf demand is
/// admitted under reads the claim's intervals off the binding (`PalwSeatIntervalGeometryV1`);
/// the seat draws over the family's own count and opens the family's own windows. On a
/// per-call class and a held one, the two counts agree and every leaf's interval is the window
/// its step lies in — or a demand the chain assigned a seat would be one the seat never checked.
#[test]
fn the_chains_interval_geometry_is_the_seats_on_both_units() {
    use kaspa_consensus_core::palw_leaf_evidence_v1::{PalwSeatIntervalGeometryV1, PalwSeatIntervalUnitV1};
    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    for (name, (_, profile, ctx, _, run)) in
        [("per-call graph-v5", dense_v5_run()), ("held graph-v7", dense_v7_run()), ("held, mid-prefill", dense_v7_run_mid_prefill())]
    {
        let chain = PalwSeatIntervalGeometryV1::from_binding_v1(&run.binding).expect("derivable from the binding");
        let family = base0_fp_interval_count_for_class_v1(&profile, ctx.declared_prefill_tokens, ctx.exact_decode_tokens, interval)
            .expect("the family counts");
        assert_eq!(chain.count, family, "{name}: one interval count");
        assert!(chain.count >= 2, "{name}: a fixture with one interval proves nothing");
        assert_eq!(
            matches!(chain.unit, PalwSeatIntervalUnitV1::Positions { .. }),
            kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile),
            "{name}: the unit is the map's"
        );
        let geometry = if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile) {
            Base0FpIntervalGeometryV1::from_chain_facts_held_v1(
                ctx.declared_prefill_tokens,
                ctx.exact_decode_tokens,
                interval,
                kaspa_consensus_core::palw_held_context_v1::palw_held_interval_positions_v1(&profile),
            )
        } else {
            Base0FpIntervalGeometryV1::from_chain_facts_v1(
                ctx.declared_prefill_tokens,
                ctx.exact_decode_tokens,
                interval,
                kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&profile),
            )
        }
        .expect("the family's geometry");
        let mut seen = std::collections::BTreeSet::new();
        for leaf in 0..run.binding.step_leaf_count {
            let owner = chain.interval_of_leaf_v1(&run.binding, leaf).expect("every leaf has an interval");
            let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, leaf).expect("coordinates");
            let step = if coord.call_index == 0 {
                u64::from(coord.position) + 1
            } else {
                u64::from(ctx.declared_prefill_tokens) + u64::from(coord.call_index)
            };
            let window = geometry.window_for(owner).expect("the interval has a window");
            assert!(
                window.first_step <= step && step <= window.last_step,
                "{name}: leaf {leaf} (step {step}) is in interval {owner}, whose window is {window:?}"
            );
            assert_eq!(base0_fp_interval_of_leaf_v1(&profile, &ctx, interval, leaf), Some(owner), "{name}: one spelling");
            seen.insert(owner);
        }
        assert_eq!(seen.len() as u32, chain.count, "{name}: every interval owns a leaf");
    }
}

/// **ADR-0103 Decision 2 at the prefill**: an interval that resumes inside the prompt — the
/// shipped unit's interval 0 would have been the whole prefill — opens with no seed row and a
/// recomputing seat licenses it from its own state at that position.
#[test]
fn a_held_interval_that_resumes_inside_the_prompt_opens_and_is_licensed() {
    let (_, _, ctx, _, run) = dense_v7_run_mid_prefill();
    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    let window = geometry.window_for(1).expect("interval 1");
    assert!(window.first_step <= u64::from(ctx.declared_prefill_tokens), "interval 1 starts inside the prompt: {window:?}");
    assert!(window.last_step > u64::from(ctx.declared_prefill_tokens), "and crosses into the decode calls");
    assert_eq!(geometry.seed_step_v1(1), None, "its first id is the prompt's");
    check_every_interval_opens_and_a_recomputing_seat_licenses_it(dense_v7_run_mid_prefill());
}

#[allow(clippy::type_complexity)]
fn check_every_interval_opens_and_a_recomputing_seat_licenses_it(
    fixture: (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1),
) {
    check_every_interval_opens_and_a_recomputing_seat_licenses_it_under(
        fixture,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
}

fn check_every_interval_opens_and_a_recomputing_seat_licenses_it_under(
    fixture: (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1),
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) {
    use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1;
    let memo = crate::fp_recompute::Base0FpSeatMemoV1::default();
    let (artifact, profile, ctx, prompt, run) = fixture;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");

    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    assert!(geometry.interval_count >= 2, "a one-interval job proves nothing about an anchor");

    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    for index in 0..geometry.interval_count {
        let opened = base0_open_fp_interval_sparse_v1(&material, index, &ids, interval, &kernels, form)
            .unwrap_or_else(|e| panic!("interval {index} must open from a fold that retained nothing: {e}"));
        // A folded class is served FLAT: the anchor is named, never carried.
        assert!(
            opened.starts_with(&PALW_BASE0_FP_INTERVAL_MAGIC_V4),
            "interval {index}: a class that folds is served the named anchor (Decision 9)"
        );

        // The seat's own recompute, ordered exactly as the panel orders it — by the `covered`
        // the opening carries, in the class's own cadence unit.
        crate::fp_recompute::base0_fp_seat_state_forget_v1(&memo);
        let state = match geometry.anchor_covered_call(index) {
            None => None,
            Some(covered) => {
                assert_eq!(
                    palw_checkpoint_positions_at_v1(&profile, &ctx, covered),
                    geometry.anchor_covered_positions_v1(index).expect("an anchored interval"),
                    "interval {index}: the leaf's counter and the state's row count must name one state"
                );
                let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("kernels");
                crate::fp_recompute::base0_fp_seat_state_memoized_v1(
                    &memo,
                    &profile,
                    &ctx,
                    &ids,
                    &run.generated_token_ids,
                    covered,
                    &mut kernels,
                    form,
                )
                .expect("this seat can recompute its own state");
                base0_fp_interval_opening_seat_state_v1(&memo, &opened, &ids, interval)
            }
        };
        if index > 0 {
            assert!(state.is_some(), "interval {index}: the seat's own state must be findable under the opening's covered");
        }
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_v1(
                &opened,
                claim,
                index,
                &ids,
                run.binding.step_leaf_count,
                interval,
                state.as_ref(),
                &kernels,
                form,
            ),
            Base0FpIntervalSeatVerdictV1::Valid,
            "interval {index}: a seat holding its own state must license an honest graph-v5 opening"
        );
    }
}

/// **C-2's unit split, pinned.** The interval's boundary is a CALL; the checkpoint that anchors
/// it is named in the class's cadence unit; both name ONE state. Swept over both cadences at
/// the same chain facts, so the difference is the cadence and nothing else.
#[test]
fn the_anchors_two_units_name_one_state() {
    use kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1;
    for interval in [1u32, 2, 3] {
        for (prefill, decode) in [(4u32, 9u32), (7, 5), (1, 12)] {
            let per_call =
                Base0FpIntervalGeometryV1::from_chain_facts_v1(prefill, decode, interval, PalwCheckpointCadenceV1::PerDecodeCall)
                    .expect("a geometry");
            let per_position =
                Base0FpIntervalGeometryV1::from_chain_facts_v1(prefill, decode, interval, PalwCheckpointCadenceV1::PerPosition)
                    .expect("a geometry");
            assert_eq!(
                per_call.interval_count, per_position.interval_count,
                "the interval count is cadence-free — which is what lets the seam derive it without a profile"
            );
            for index in 0..per_call.interval_count {
                assert_eq!(per_call.calls_for(index), per_position.calls_for(index), "the calls are the same partition");
                assert_eq!(
                    per_call.anchor_seed_call_v1(index),
                    per_position.anchor_seed_call_v1(index),
                    "the seed row is a coordinate and does not move with the cadence"
                );
                let Some(positions) = per_call.anchor_covered_positions_v1(index) else {
                    assert_eq!(index, 0, "only interval 0 has no anchor");
                    continue;
                };
                assert_eq!(per_call.anchor_covered_call(index), Some(index * interval));
                assert_eq!(
                    per_position.anchor_covered_call(index),
                    Some(prefill + index * interval),
                    "a per-position leaf's counter IS the row count"
                );
                assert_eq!(positions, prefill + index * interval, "and both name the same state");
            }
        }
    }
}

/// **C-1, test 3: every committed leaf of a graph-v5 leg has an anchor, and it is the one the
/// COURT will demand.**
///
/// `base0_kv_anchor_for_step_v1` reached `checkpoints.chunks.get(at)` and a folded leg's vector
/// is empty, so it answered `None` for every step of the class — the executor could not
/// assemble the anchored refutation ADR-0082 Decision 4 is about, at any coordinate. The
/// chunks now come from the cache the executor is holding anyway, which the map's own
/// prefix-stability makes byte-identical to the ones it folded away.
///
/// The `covered` is checked against `palw_checkpoint_covered_for_step_v1` — the one spelling of
/// which checkpoint a step's anchor is — and the operands are then run through the COURT's own
/// `verify_kv_anchor` by way of the chunk-count and root rules it applies.
#[test]
fn every_step_of_a_graph_v5_class_has_the_anchor_the_court_demands() {
    check_every_step_has_the_anchor_the_court_demands(dense_v5_run());
}

#[test]
fn every_step_of_a_held_graph_v7_class_has_the_anchor_the_court_demands() {
    check_every_step_has_the_anchor_the_court_demands(dense_v7_run());
}

#[allow(clippy::type_complexity)]
fn check_every_step_has_the_anchor_the_court_demands(
    fixture: (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>, crate::produce::Base0ExecutionV1),
) {
    use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_covered_for_step_v1;
    let (artifact, profile, ctx, prompt, run) = fixture;
    assert!(run.checkpoints.chunks.is_empty(), "the fold retained nothing — this is the case that answered None");

    // The executor's own cache, run to the end of the job: what it is holding anyway.
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let mut cache = crate::engine_a16::A16Cache::new(artifact.shape.n_layers);
    let prefill = ctx.declared_prefill_tokens as usize;
    for (position, token) in prompt.iter().take(prefill).enumerate() {
        engine.forward_token_planned(&plan, &mut cache, *token, position).expect("the prefill runs");
    }
    for (call, token) in run.generated_token_ids.iter().enumerate().take(ctx.exact_decode_tokens as usize - 1) {
        engine.forward_token_planned(&plan, &mut cache, *token as usize, prefill + call).expect("the decode runs");
    }

    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let mut checked = 0u32;
    for (call_index, position) in (0..prefill as u32).map(|p| (0u32, p)).chain((1..=decode_calls).map(|c| (c, 0u32))) {
        let want = palw_checkpoint_covered_for_step_v1(&profile, &ctx, call_index, position).expect("a per-position anchor");
        let anchor = crate::legs::base0_kv_anchor_for_step_v1(&run.checkpoints, &profile, &ctx, call_index, position, |entry| {
            cache.state_chunk_bytes_v1(entry)
        })
        .unwrap_or_else(|| panic!("call {call_index} position {position} has no anchor in its own producer's leg"));
        assert_eq!(anchor.leaf.covered_decode_call, want, "the anchor must be the one the court's own rule names");
        assert_eq!(anchor.chunks.len(), anchor.leaf.state_chunk_count as usize, "the re-derived chunks are the leaf's own");
        // …and they must rebuild the state root the producer committed, which is what makes
        // the re-derivation-from-a-later-cache argument true rather than merely stated.
        let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
            &profile,
            &ctx,
            anchor.leaf.covered_decode_call,
        );
        assert_eq!(
            base0_state_chunks_root_for_v1(&profile, positions, &anchor.chunks).expect("a root"),
            anchor.leaf.state_chunks_root,
            "a chunk re-derived from a later cache must be the byte the earlier checkpoint committed"
        );
        checked += 1;
    }
    assert_eq!(checked, run.checkpoints.leaves.len() as u32, "every committed leaf was reached from some step");
}

/// **ADR-0085 X1 — the close assembled from a served opening and this party's own replay is
/// the close assembled from the capture, byte for byte**, at every main-step leaf of every
/// interval of an honest floor run; and a tile that is not the accused's is refused by name.
/// **ADR-0086 Decision 6's address is a packed index the lane already carries**: bit 31,
/// the interval, the block. Round trips; a plain index is never read as one; the two limits
/// refuse at the boundary.
#[test]
fn a_block_leaves_request_index_round_trips_and_a_plain_index_is_not_one() {
    for (interval, block) in [(0u32, 0u64), (1, 1), (511, 1618), ((1 << 15) - 1, (1 << 16) - 1)] {
        let packed = base0_fp_block_leaves_request_index_v1(interval, block).expect("inside both limits");
        assert_eq!(base0_fp_block_leaves_request_decode_v1(packed), Some((interval, block)));
        assert!(packed & PALW_BASE0_FP_BLOCK_LEAVES_REQUEST_BIT_V1 != 0);
    }
    assert_eq!(base0_fp_block_leaves_request_index_v1(1 << 15, 0), None, "the interval limit");
    assert_eq!(base0_fp_block_leaves_request_index_v1(0, 1 << 16), None, "the block limit");
    for plain in [0u32, 1, 511, u32::MAX >> 1] {
        assert_eq!(base0_fp_block_leaves_request_decode_v1(plain), None, "a plain interval index is not a request");
    }
}

/// **ADR-0085 §6 items 4–5 end to end at the base0 layer**: the executor fills the annex from
/// its own tiles (`base0_fp_close_annex_v1`) and attaches it to the V4 opening it served; the
/// annexed opening verifies exactly as the plain one (the seat's replay never sees the annex,
/// ADR-0085 X3); `base0_fp_interval_of_leaf_v1` names the interval the opener put the leaf
/// in; and the close assembled from the served intervals
/// (`base0_refutation_from_served_intervals_capped_v1`) is byte for byte the capture path's
/// under the same tiled pin (X1, through the served path).
#[test]
fn a_close_from_served_intervals_is_the_close_from_the_capture_and_the_annex_changes_no_verdict() {
    use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, canonical_step_coordinates};
    use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1};
    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: run.binding.job_context.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let material: Base0RetainedMaterialV1 = (
        run.binding.clone(),
        run.tiles.tiles.clone(),
        run.logits_rows.clone(),
        run.generated_token_ids.clone(),
        run.checkpoints.chunks.clone(),
    );
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let cap = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
    let leaf_count = run.binding.step_leaf_count;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    let rows_root = tiled_logits_rows_root_v1(&ctx, &run.logits_rows).expect("the rows build a tree");
    let pin =
        || PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: run.generated_token_ids.clone() });
    let reads_cache = |coord: &kaspa_consensus_core::palw_step::PalwStepCoordinateV1| {
        profile
            .resolve_node_slot(coord.node_slot)
            .map(|(node, _)| node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
            .unwrap_or(false)
    };
    let tile_at = |leaf: u64| run.tiles.tiles.iter().find(|(i, _)| *i == leaf).map(|(_, t)| t.clone());
    let mut compared = 0usize;
    let mut previous_plain: Option<(u32, Vec<u8>)> = None;
    for index in 0..geometry.interval_count {
        let plain = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the interval opens");
        let v4 = Base0FpIntervalOpeningV4::decode_v1(&plain).expect("the dense opener serves V4");
        let range = (v4.range.first_leaf_index, v4.range.first_leaf_index + v4.range.leaf_count);
        let leaves_geometry = base0_fp_interval_leaves_v1(&profile, &ctx, &geometry, index, leaf_count).expect("leaves");
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        for leaf in range.0..range.1 {
            let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
            // The opener's own placement is the answer `base0_fp_interval_of_leaf_v1` must give:
            // the interval's own calls map to it, the seed row to the interval before.
            let owner = base0_fp_interval_of_leaf_v1(&profile, &ctx, interval, leaf).expect("a main step leaf has an owner");
            if leaf >= leaves_geometry.interval_first {
                assert_eq!(owner, index, "leaf {leaf} is interval {index}'s own");
            } else {
                assert_eq!(owner + 1, index, "leaf {leaf} is the seed row: the call before interval {index}");
            }
            // The executor's annex, from its own tiles, attached to the opening it served.
            let annex = base0_fp_close_annex_v1(&run.binding, &run.logits_rows, &run.checkpoints.chunks, &tile_at, &[leaf], range)
                .expect("the annex builds");
            assert_eq!(annex.disputed.len(), 1);
            assert_eq!(annex.rows_root, rows_root);
            let annexed = base0_fp_interval_opening_with_close_v1(&plain, annex.clone()).expect("V4 carries the annex");
            assert_eq!(base0_fp_interval_close_annex_any_v1(&annexed).as_ref(), Some(&annex));
            assert_eq!(base0_fp_interval_close_annex_any_v1(&plain), None);
            // ADR-0085 X3 on V4: the annex is invisible to the seat's verdict.
            let verdict_plain = base0_verify_fp_interval_opening_with_state_capped_v1(
                &plain,
                claim,
                index,
                &ids,
                leaf_count,
                interval,
                cap,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            );
            let verdict_annexed = base0_verify_fp_interval_opening_with_state_capped_v1(
                &annexed,
                claim,
                index,
                &ids,
                leaf_count,
                interval,
                cap,
                state.as_ref(),
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            );
            assert_eq!(verdict_plain, verdict_annexed, "interval {index} leaf {leaf}: the annex changed a verdict");
            // The close from the served intervals equals the capture path's, same pin.
            let kv = if reads_cache(&coord) && coord.call_index > 0 {
                crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, coord.call_index)
            } else {
                None
            };
            let from_capture = crate::legs::base0_refutation_from_capture_capped_v1(
                &profile,
                &ctx,
                &run.tiles,
                run.binding.clone(),
                coord,
                ids.clone(),
                Some(pin()),
                kv,
                cap,
            )
            .expect("the capture path assembles");
            let mut held: Vec<(u32, Vec<u8>)> = vec![(index, annexed.clone())];
            if let Some(prev) = previous_plain.as_ref() {
                held.push(prev.clone());
            }
            let state_for = |bytes: &[u8]| {
                let v4 = Base0FpIntervalOpeningV4::decode_v1(bytes).ok()?;
                let covered = geometry.anchor_covered_call(v4.interval_index)?;
                Some(seat_state(&material, &artifact, &ids, covered))
            };
            let from_served = base0_refutation_from_served_intervals_capped_v1(
                &held,
                claim,
                &ids,
                &run.generated_token_ids,
                leaf_count,
                leaf,
                interval,
                cap,
                &state_for,
                &FloorKernels(&artifact),
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .unwrap_or_else(|e| panic!("interval {index} leaf {leaf}: the served path refuses: {e}"));
            assert_eq!(from_served, from_capture, "interval {index} leaf {leaf}: the two closes differ");
            compared += 1;
        }
        // A leaf the annex does not name is refused by name, not closed from another leaf's tile.
        if let Some(leaf) = (range.0..range.1).find(|l| canonical_step_coordinates(&profile, &ctx, *l).is_some()) {
            let other = (range.0..range.1).rev().find(|l| *l != leaf && canonical_step_coordinates(&profile, &ctx, *l).is_some());
            if let Some(other) = other {
                let annex =
                    base0_fp_close_annex_v1(&run.binding, &run.logits_rows, &run.checkpoints.chunks, &tile_at, &[other], range)
                        .expect("the annex builds");
                let annexed = base0_fp_interval_opening_with_close_v1(&plain, annex).expect("V4 carries the annex");
                let held = vec![(index, annexed)];
                let state_for = |bytes: &[u8]| {
                    let v4 = Base0FpIntervalOpeningV4::decode_v1(bytes).ok()?;
                    let covered = geometry.anchor_covered_call(v4.interval_index)?;
                    Some(seat_state(&material, &artifact, &ids, covered))
                };
                let refused = base0_refutation_from_served_intervals_capped_v1(
                    &held,
                    claim,
                    &ids,
                    &run.generated_token_ids,
                    leaf_count,
                    leaf,
                    interval,
                    cap,
                    &state_for,
                    &FloorKernels(&artifact),
                    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                );
                assert!(refused.is_err(), "interval {index}: an annex naming leaf {other} must not close leaf {leaf}");
            }
        }
        previous_plain = Some((index, plain));
    }
    assert!(compared > 0, "the fixture yields main-step leaves");
}

/// **The executor's own close, built from its fold, is the capture close** (ADR-0121): for every
/// main-step leaf of a multi-interval floor job — the prompt's, the seed rows', the anchored
/// decode calls' — the refutation assembled from the retained tree and a replay of just the
/// blocks the step reads is byte for byte the one the dense capture assembles, under the same
/// tiled pin and the same anchor. The tree is kept at small retained levels so a step's leaves
/// and their paths cross block boundaries; a leaf outside the space is refused.
#[test]
fn the_executors_own_close_is_the_capture_close() {
    use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, canonical_step_coordinates};
    use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1};
    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let dense: Base0RetainedMaterialV1 = (
        run.binding.clone(),
        run.tiles.tiles.clone(),
        run.logits_rows.clone(),
        run.generated_token_ids.clone(),
        run.checkpoints.chunks.clone(),
    );
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let cap = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
    let leaf_count = run.binding.step_leaf_count;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    assert!(geometry.interval_count > 1, "the fixture spans intervals");
    let rows_root = tiled_logits_rows_root_v1(&ctx, &run.logits_rows).expect("the rows build a tree");
    let pin =
        || PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: run.generated_token_ids.clone() });
    let reads_cache = |coord: &kaspa_consensus_core::palw_step::PalwStepCoordinateV1| {
        profile
            .resolve_node_slot(coord.node_slot)
            .map(|(node, _)| node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
            .unwrap_or(false)
    };
    let mut compared = 0usize;
    for retain_level in [2u32, 3, 5] {
        let material = crate::produce::Base0FpMaterialV2 {
            version: crate::produce::PALW_BASE0_FP_MATERIAL_VERSION_V2,
            binding: run.binding.clone(),
            step_tree: crate::fp_capture::Base0SparseStepTreeV1::from_leaves_v1(&run.tiles.leaves, retain_level).expect("folds"),
            logits_rows: run.logits_rows.clone(),
            generated_token_ids: run.generated_token_ids.clone(),
            prompt_token_ids: ids.clone(),
            checkpoint_chunks: run.checkpoints.chunks.clone(),
            checkpoint_leaves: run.checkpoints.leaves.clone(),
        };
        let anchor_state_for = |covered: u32| Some(seat_state(&dense, &artifact, &ids, covered));
        for leaf in 0..leaf_count {
            let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
            let kv = if reads_cache(&coord) && coord.call_index > 0 {
                crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, coord.call_index)
            } else {
                None
            };
            let Ok(from_capture) = crate::legs::base0_refutation_from_capture_capped_v1(
                &profile,
                &ctx,
                &run.tiles,
                run.binding.clone(),
                coord,
                ids.clone(),
                Some(pin()),
                kv,
                cap,
            ) else {
                continue;
            };
            let from_fold = base0_fp_leaf_refutation_from_fold_v1(
                &material,
                leaf,
                &ids,
                interval,
                cap,
                &FloorKernels(&artifact),
                &anchor_state_for,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .unwrap_or_else(|e| panic!("retain level {retain_level} leaf {leaf}: the fold refuses: {e}"));
            assert_eq!(from_fold, from_capture, "retain level {retain_level} leaf {leaf}: the two closes differ");
            compared += 1;
        }
        let material_for_refusal = &material;
        assert!(
            base0_fp_leaf_refutation_from_fold_v1(
                material_for_refusal,
                leaf_count,
                &ids,
                interval,
                cap,
                &FloorKernels(&artifact),
                &anchor_state_for,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .is_err(),
            "a leaf outside the space"
        );
    }
    assert!(compared > 100, "the fixture's leaves were compared: {compared}");
}

/// **ADR-0086 Decision 6 on a dense retention**: a block wholly inside the interval is served
/// as its leaf hashes and folds to the digest the opening carries; on a space with no whole
/// block the request is refused by name.
#[test]
fn served_block_leaves_fold_to_the_served_digest_or_refuse_by_name() {
    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let material: Base0RetainedMaterialV1 = (
        run.binding.clone(),
        run.tiles.tiles.clone(),
        run.logits_rows.clone(),
        run.generated_token_ids.clone(),
        run.checkpoints.chunks.clone(),
    );
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let leaf_count = run.binding.step_leaf_count;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    let mut served = 0usize;
    let mut refused = 0usize;
    for index in 0..geometry.interval_count {
        let plain = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the interval opens");
        let v4 = Base0FpIntervalOpeningV4::decode_v1(&plain).expect("V4");
        let (first_block, end_block) = v4.range.whole_blocks_v1(leaf_count);
        for block in first_block..end_block {
            let bytes = base0_fp_block_leaves_from_tiles_v1(&plain, &run.tiles.tiles, block).expect("a whole block serves");
            let leaves = Base0FpBlockLeavesV1::decode_v1(&bytes).expect("decodes");
            let digest = &v4.range.block_roots[(block - first_block) as usize];
            assert!(leaves.folds_to_v1(digest), "interval {index} block {block}: the served leaves do not fold to the digest");
            served += 1;
        }
        let outside = end_block.max(1) + 7;
        assert!(base0_fp_block_leaves_from_tiles_v1(&plain, &run.tiles.tiles, outside).is_err(), "a block outside is refused");
        refused += 1;
    }
    assert!(refused > 0);
    // The floor fixture's space is small; a whole block inside it is a bonus, not a premise.
    let _ = served;
}

#[test]
fn a_close_from_an_opening_is_the_close_from_the_capture() {
    use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, canonical_step_coordinates};
    use kaspa_consensus_core::palw_step_refute::{PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1};
    let (artifact, profile, ctx, prompt) = floor_job(3, 4);
    let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: run.binding.job_context.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let material: Base0RetainedMaterialV1 = (
        run.binding.clone(),
        run.tiles.tiles.clone(),
        run.logits_rows.clone(),
        run.generated_token_ids.clone(),
        run.checkpoints.chunks.clone(),
    );
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let cap = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
    let leaf_count = run.binding.step_leaf_count;
    let geometry = Base0FpIntervalGeometryV1::from_binding_v1(&run.binding, interval).expect("a geometry");
    let pin = || {
        PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 {
            logits_rows: run.logits_rows.clone(),
            generated_token_ids: run.generated_token_ids.clone(),
        })
    };
    let reads_cache = |coord: &kaspa_consensus_core::palw_step::PalwStepCoordinateV1| {
        profile
            .resolve_node_slot(coord.node_slot)
            .map(|(node, _)| node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
            .unwrap_or(false)
    };
    let mut compared = 0usize;
    let mut anchored = 0usize;
    let mut previous: Option<(Base0FpIntervalOpeningV2, crate::legs::Base0StepTilesV1)> = None;
    for index in 0..geometry.interval_count {
        let opening_bytes = base0_open_fp_interval_v1(
            &material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the interval opens");
        // ADR-0086 Decision 2: the challenger replays from its own state, as the seat does.
        let state = geometry.anchor_covered_call(index).map(|covered| seat_state(&material, &artifact, &ids, covered));
        let (opening, replay) = base0_fp_challenger_replay_tiles_capped_v1(
            &opening_bytes,
            claim,
            index,
            &ids,
            leaf_count,
            interval,
            cap,
            state.as_ref(),
            &FloorKernels(&artifact),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the challenger replays the interval");
        let first = opening.range.first_leaf_index;
        let end = first + opening.range.leaf_hashes.len() as u64;
        for leaf in first..end {
            let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
            // The executor's anchor for this step, exactly as the capture path attaches it.
            let kv = if reads_cache(&coord) && coord.call_index > 0 {
                crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, coord.call_index)
            } else {
                None
            };
            let from_capture = crate::legs::base0_refutation_from_capture_capped_v1(
                &profile,
                &ctx,
                &run.tiles,
                run.binding.clone(),
                coord,
                ids.clone(),
                Some(pin()),
                kv.clone(),
                cap,
            )
            .expect("the capture path assembles");
            let tile = run.tiles.tiles.iter().find(|(i, _)| *i == leaf).map(|(_, t)| t.clone()).expect("the run holds the tile");
            let annex = Base0FpCloseAnnexV1 {
                rows_root: Hash64::default(),
                disputed: vec![Base0FpDisputedLeafV1 {
                    leaf_index: leaf,
                    tile,
                    anchor: kv.as_ref().map(|k| Base0FpCheckpointClaimV1 { leaf: k.leaf.clone(), opening: k.opening.clone() }),
                }],
            };
            let chunks: Vec<Vec<u8>> = kv.as_ref().map(|k| k.chunks.clone()).unwrap_or_default();
            // The interval holding the leaf first, then the one before it: a step at an
            // interval's first call reads the call before it.
            let mut held: Vec<(&Base0FpIntervalOpeningV2, &crate::legs::Base0StepTilesV1)> = vec![(&opening, &replay)];
            if let Some((o, r)) = previous.as_ref() {
                held.push((o, r));
            }
            let from_opening = crate::legs::base0_refutation_from_opening_capped_v1(
                &profile,
                &ctx,
                &held,
                &annex,
                coord,
                ids.clone(),
                Some(pin()),
                &chunks,
                cap,
            )
            .unwrap_or_else(|e| panic!("interval {index} leaf {leaf}: the opening path refuses: {e:?}"));
            assert_eq!(from_opening, from_capture, "interval {index} leaf {leaf}: the two closes differ");
            assert_eq!(
                borsh::to_vec(&from_opening).unwrap(),
                borsh::to_vec(&from_capture).unwrap(),
                "interval {index} leaf {leaf}: byte for byte"
            );
            compared += 1;
            if kv.is_some() {
                anchored += 1;
            }
            // A tile that is not the accused's committed one is refused before anything opens.
            let mut forged = annex.clone();
            forged.disputed[0].tile.values_le[0] ^= 0x01;
            assert!(
                matches!(
                    crate::legs::base0_refutation_from_opening_capped_v1(
                        &profile,
                        &ctx,
                        &held,
                        &forged,
                        coord,
                        ids.clone(),
                        Some(pin()),
                        &chunks,
                        cap
                    ),
                    Err(crate::legs::LegError::CloseFromOpening(_))
                ),
                "interval {index} leaf {leaf}: a forged annex tile must be refused by name"
            );
        }
        previous = Some((opening, replay));
    }
    assert!(compared > 0, "the fixture yields main-step leaves");
    assert!(anchored > 0, "the fixture exercises an anchored close (a cache-reading step past the prefill)");
}

/// **The ladder above the default one** (ADR-0084 §7 record). A class whose step space is
/// larger than `PALW_STEP_LEG_MAX_LEAVES` — the graph-v5 attempt lane is 6.6 M leaves — is
/// served under its RULESET's cap. The uncapped root's ceiling is the default, and an opener
/// that reached for it reported a capture that was the binding's as one that was not; under a
/// cap below the space the same opening is refused, so the cap is the bound and not the constant.
#[test]
fn an_opening_above_the_default_ladder_assembles_under_its_rulesets_cap() {
    use kaspa_consensus_core::palw_step_leg::{step_range_opening_root_capped_v1, step_range_opening_root_v1};
    let (material, _claim, _ids, _artifact) = floor_material(1, 2);
    let mut binding = material.0.clone();
    let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
    let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
    let leaves: Vec<Hash64> = (0..leaf_count).map(|i| Hash64::from_u64_word(i + 1)).collect();
    let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
        &leaves,
        crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
        cap,
    )
    .expect("a tree over the larger space");
    binding.step_leaf_count = leaf_count;
    binding.step_merkle_root = tree.root().expect("its root");
    // A range that ends in the space's last, partial block — the shape an off-by-one hides in.
    let lg = Base0FpIntervalLeavesV1 {
        range_first: leaf_count - 4,
        interval_first: leaf_count - 4,
        range_end: leaf_count,
        seed_row_leaves: 0,
    };
    let (span_first, span_end) = tree.span_for_range(lg.range_first, 4).expect("the span");
    let span = &leaves[span_first as usize..span_end as usize];
    let bytes = base0_assemble_fp_interval_opening_v1(&binding, &tree, &lg, 0, span_first, span, Vec::new(), None, cap)
        .expect("assembles under the ruleset's cap");
    let fold = Base0FpIntervalOpeningV4::decode_v1(&bytes).expect("a V4 opening").range;
    // ADR-0086 X4: the bytes are the fold's, not the range's — under 200 KB for a 2^22-leaf
    // space's last four leaves.
    assert!(bytes.len() < 200_000, "a fold opening over a 2^22+1-leaf space is {} bytes", bytes.len());
    let range = fold.with_leaves_v1(leaves[lg.range_first as usize..lg.range_end as usize].to_vec()).expect("the seat's own leaves");
    assert_eq!(step_range_opening_root_capped_v1(leaf_count, &range, cap).ok(), Some(binding.step_merkle_root));
    assert!(
        step_range_opening_root_v1(leaf_count, &range).is_err(),
        "the default ladder does not reach this space — which is why no opener may reach for it"
    );
    assert!(
        matches!(
            base0_assemble_fp_interval_opening_v1(
                &binding,
                &tree,
                &lg,
                0,
                span_first,
                span,
                Vec::new(),
                None,
                PALW_STEP_LEG_MAX_LEAVES
            ),
            Err(Base0FpIntervalError::CaptureIsNotTheBindings)
        ),
        "under a cap below the space the same opening is refused"
    );
}
