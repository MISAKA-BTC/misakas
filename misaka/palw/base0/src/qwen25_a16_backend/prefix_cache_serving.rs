//! **`qwen25_a16_backend`'s `prefix_cache_serving` module**, moved out of `qwen25_a16_backend.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;
use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PALW_FP_V4_VERSION, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn fixture() -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    let geometry = PalwQwen25GeometryV1 {
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
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a held graph-v7 profile");
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: geometry.rms_eps_q,
    };
    let artifact = std::sync::Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    (artifact, profile)
}

fn backend(artifact: &std::sync::Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), b"misaka-palw-rc".to_vec(), profile.clone(), (64, 8))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(1 << 12)
}

fn job(profile: &PalwShapeProfileV3, prompt: usize, decode: u32) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::from_u64_word(0x70),
        prompt_token_ids_hash: Hash64::from_u64_word(0x71),
        prompt_tokens: prompt as u32,
        decode_token_limit: decode,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    }
}

fn prompt(len: usize, salt: usize) -> Vec<usize> {
    (0..len).map(|i| (i * 7919 + 1013 + salt) % 128).collect()
}

fn answer(b: &Qwen25A16Backend, job: &PalwFreePromptJobV3, p: &[usize]) -> kaspa_consensus_core::palw_backend::PalwFpAnswerV1 {
    b.answer_free_prompt_v1(job, p, &[], &mut |_| {}).expect("the answer runs")
}

/// **The answer-only path selects the tokens a committed run commits** — the claim every other
/// serving optimisation rests on: one decoder, one engine, so the ids a user is shown are the
/// ids a claim would have committed. Greedy, sampled (a seed and a temperature) and a V4 job
/// with penalties.
#[test]
fn an_answer_only_run_selects_the_tokens_a_committed_run_commits() {
    let (artifact, profile) = fixture();
    let b = backend(&artifact, &profile);
    let p = prompt(20, 0);
    let greedy = job(&profile, p.len(), 6);
    let mut sampled = job(&profile, p.len(), 6);
    sampled.sampling_seed = [0x33; 32];
    sampled.temperature_q = 1 << 24;
    let mut v4 = job(&profile, p.len(), 6);
    v4.version = PALW_FP_V4_VERSION;
    v4.decode = Some(DecodeConfigV4 { repeat_penalty_q: 98_304, penalty_window: 8, ..DecodeConfigV4::NOOP });
    v4.sampling_seed = [0x44; 32];
    v4.temperature_q = 1 << 24;
    for (label, job) in [("greedy", &greedy), ("sampled", &sampled), ("v4 penalties", &v4)] {
        let committed = b.execute_free_prompt(job, &p).expect("the committed run").output_token_ids;
        let answered = answer(&b, job, &p);
        assert_eq!(answered.output_token_ids, committed, "{label}: the answer is the committed ids");
        assert_eq!(answered.cached_prefix_tokens, 0, "{label}: no cache, no reuse");
    }
}

/// **GOLDEN: prefix-resume == fresh**, at every split of the prompt. For each prefix length the
/// cache is filled by running the prefix as its own prompt, then the whole prompt is answered
/// from that resume point; the ids must equal a cache-less backend's, the hit must be real
/// (`cached_prefix_tokens` is the prefix), and with `verify_every = 1` every hit is re-checked
/// logits-and-K/V against a fresh prefill — which must leave the cache enabled.
#[test]
fn a_run_resumed_from_a_cached_prefix_is_bit_identical_to_a_fresh_run() {
    let (artifact, profile) = fixture();
    let fresh = backend(&artifact, &profile);
    let full = prompt(24, 5);
    let mut sampled = job(&profile, full.len(), 8);
    sampled.sampling_seed = [0x21; 32];
    sampled.temperature_q = 1 << 24;
    let expected = answer(&fresh, &sampled, &full).output_token_ids;
    for split in [1usize, 2, 7, 8, 9, 16, 23] {
        let cached = backend(&artifact, &profile).with_prefix_cache_v1(1 << 26, 1);
        assert!(!cached.prefix_cache_stats_v1().unwrap().disabled, "the boot self-check passes");
        // Fill the cache with the first `split` ids as a prompt of their own.
        let head = &full[..split];
        let _ = answer(&cached, &job(&profile, head.len(), 2), head);
        let hit = answer(&cached, &sampled, &full);
        assert_eq!(hit.cached_prefix_tokens as usize, split, "split {split}: the cache served the whole prefix");
        assert_eq!(hit.output_token_ids, expected, "split {split}: resumed == fresh");
        let stats = cached.prefix_cache_stats_v1().unwrap();
        assert!(!stats.disabled, "split {split}: the sampled verification found no difference");
        assert_eq!(stats.hits, 1);
    }
}

/// **RFC-0001 §2.6 stage 2: a prefix state is a function of the ids alone** — a cached producer and a cacheless seat
/// derive the same state root, a job naming it verifies, and a job naming another state (or another prefix length, or
/// a prefix that is not proper) is refused by name.
#[test]
fn a_prefix_state_is_the_same_cached_or_recomputed_and_a_wrong_one_is_refused() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpJobTailV1, PalwFpPrefixStateV1};
    let (artifact, profile) = fixture();
    let class = profile.shape_profile_id();
    let tok = Hash64::from_u64_word(0x70);
    let full = prompt(20, 3);
    let ids: Vec<u32> = full.iter().map(|t| *t as u32).collect();
    let cached = backend(&artifact, &profile).with_prefix_cache_v1(1 << 26, 0);
    let seat = backend(&artifact, &profile);
    // The producer served a prompt that ended at 8 ids, so its cache holds that exact state.
    let _ = answer(&cached, &job(&profile, 8, 2), &full[..8]);
    let from_cache = cached.prefix_state_v1(class, tok, &ids[..8]).unwrap();
    let recomputed = seat.prefix_state_v1(class, tok, &ids[..8]).unwrap();
    assert_eq!(from_cache, recomputed, "cache and recomputation agree");
    assert_eq!(from_cache.prefix_tokens, 8);
    assert_ne!(
        seat.prefix_state_v1(class, tok, &ids[..9]).unwrap().state_root,
        recomputed.state_root,
        "another prefix, another state"
    );
    let mut other = ids.clone();
    other[3] ^= 1;
    assert_ne!(seat.prefix_state_v1(class, tok, &other[..8]).unwrap().state_root, recomputed.state_root, "other ids, another state");
    assert!(seat.prefix_state_v1(class, tok, &[]).is_err());
    assert!(seat.prefix_state_v1(class, tok, &[9_999]).is_err(), "outside the vocabulary");

    let mut j = job(&profile, ids.len(), 4);
    j.version = kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION;
    j.decode = Some(DecodeConfigV4::NOOP);
    j.tail = Some(PalwFpJobTailV1::Prefix(recomputed));
    seat.verify_prefix_state_v1(&j, &ids).expect("the named state is the derived one");
    let mut wrong = j.clone();
    wrong.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { state_root: Hash64::from_u64_word(1), ..recomputed }));
    assert!(seat.verify_prefix_state_v1(&wrong, &ids).unwrap_err().contains("derives"));
    let mut whole = j.clone();
    whole.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { prefix_tokens: ids.len() as u32, ..recomputed }));
    assert!(seat.verify_prefix_state_v1(&whole, &ids).unwrap_err().contains("proper prefix"));
    // A job that names no state is not this check's business.
    seat.verify_prefix_state_v1(&job(&profile, ids.len(), 4), &ids).expect("no state named");
}

/// The objective prefix court's digest matches the actual A16 engine's cache, for both
/// ordinary and inherited receipts. The proof reads only committed public output tiles;
/// its tree is reconstructed here from retained tiles or a replay matching the retained root.
#[test]
fn public_prefix_court_reconstructs_the_actual_a16_cache_and_refutes_a_false_named_root() {
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpJobTailV1, PalwFpPrefixStateV1};
    use kaspa_consensus_core::palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_prefix_state_fault_v1};
    use kaspa_consensus_core::palw_prefix_state_court_v1::{PalwPrefixKvPlanV1, PalwPrefixKvTileV1, palw_prefix_kv_state_root_v1};
    use kaspa_consensus_core::palw_step_leg::{PalwStepMerkleTreeV1, PalwStepOpeningV1, step_tile_leaf_hash_ctx_v1};
    let (artifact, _) = fixture();
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(PalwQwen25GeometryV1 {
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
    })
    .unwrap();
    let producer = backend(&artifact, &profile).with_step_ladder_cap(1 << 22);
    let ids = prompt(4, 3);
    let ids32: Vec<u32> = ids.iter().map(|id| *id as u32).collect();
    let true_state = producer.prefix_state_v1(profile.shape_profile_id(), Hash64::from_u64_word(0x70), &ids32[..2]).unwrap();
    for inherited in [false, true] {
        for false_root in [false, true] {
            let mut j = job(&profile, ids.len(), 2);
            j.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                &ids32,
            )
            .unwrap();
            j.version = if inherited {
                kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_INHERIT_VERSION
            } else {
                kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION
            };
            j.decode = Some(DecodeConfigV4::NOOP);
            j.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 {
                state_root: if false_root { Hash64::from_u64_word(0xBAD17) } else { true_state.state_root },
                ..true_state
            }));
            assert_eq!(
                producer.verify_prefix_state_v1(&j, &ids32).is_err(),
                false_root,
                "the seat checks the named prefix even though full replay remains available"
            );
            let run = producer.execute_free_prompt(&j, &ids).unwrap();
            let retained = crate::produce::base0_material_decode_any_v1(&run.outcome.material).unwrap();
            let binding = retained.binding();
            let step_tiles = producer.tiles_from_material_v1(&retained).expect("replay reproduces the retained execution root");
            let raw = &step_tiles.tiles;
            let by_index: std::collections::BTreeMap<_, _> = raw.iter().map(|(index, tile)| (*index, tile)).collect();
            let mut hashes = vec![Hash64::default(); binding.step_leaf_count as usize];
            for (index, tile) in raw {
                hashes[*index as usize] = step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile.shape_profile_id(), tile);
            }
            let tree = PalwStepMerkleTreeV1::build_capped_v1(&hashes, 1 << 22).unwrap();
            assert_eq!(tree.root(), binding.step_merkle_root, "the public fixture reproduces the committed tree");
            let plan = PalwPrefixKvPlanV1::of_profile(&profile, 2).unwrap();
            let mut tiles = Vec::new();
            for ordinal in 0..plan.tile_count {
                let (coord, _) = plan.coordinate(&profile, ordinal).unwrap();
                let leaf = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &binding.job_context, &coord).unwrap();
                tiles.push(PalwPrefixKvTileV1 {
                    opening: PalwStepOpeningV1 {
                        leaf_index: leaf,
                        leaf_hash: hashes[leaf as usize],
                        siblings: tree.path_v1(leaf as usize).unwrap(),
                    },
                    preimage: (*by_index[&leaf]).clone(),
                });
            }
            assert_eq!(
                palw_prefix_kv_state_root_v1(binding, 2, &tiles, 1 << 22).unwrap(),
                true_state.state_root,
                "inherited={inherited}, false_root={false_root}: the court agrees with the actual cache"
            );
            let target = PalwOffenceTargetV1 {
                claim_id: Hash64::from_u64_word(0xC17),
                class_id: profile.shape_profile_id(),
                artifact_root: crate::inventory::a16_inventory_v1(&artifact, &profile).unwrap().root(),
                executor_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(j.executor_bond),
                execution_root: binding.committed_execution_root,
                lane: Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt),
                segment_count: Some(4),
                phase: None,
                job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
                trace_root: binding.full_logits_trace_root,
                output_root: run.outcome.output_root,
                registered_semantics: None,
            };
            assert_eq!(palw_prefix_state_fault_v1(&target, binding, &j, &tiles).unwrap(), false_root);
            let mut forged_job = j.clone();
            forged_job.job_nonce[0] ^= 1;
            assert!(palw_prefix_state_fault_v1(&target, binding, &forged_job, &tiles).is_err());
        }
    }
}

/// A conversation: turn 2 extends turn 1's prompt AND its answer, and resumes from the whole
/// of turn 1's state (prompt + answer ids).
#[test]
fn the_next_turn_resumes_from_the_whole_of_the_last_one() {
    let (artifact, profile) = fixture();
    let cached = backend(&artifact, &profile).with_prefix_cache_v1(1 << 26, 0);
    let fresh = backend(&artifact, &profile);
    let turn1 = prompt(14, 9);
    let first = answer(&cached, &job(&profile, turn1.len(), 5), &turn1);
    let mut turn2 = turn1.clone();
    turn2.extend(first.output_token_ids.iter().map(|t| *t as usize));
    turn2.extend(prompt(5, 77));
    let j2 = job(&profile, turn2.len(), 5);
    let resumed = answer(&cached, &j2, &turn2);
    // The cache holds prompt + the answer ids that were fed forward (all but the last).
    assert_eq!(resumed.cached_prefix_tokens as usize, turn1.len() + first.output_token_ids.len() - 1);
    assert_eq!(resumed.output_token_ids, answer(&fresh, &j2, &turn2).output_token_ids);
}

/// **A resume that is not the fresh run is caught and turns the cache off.** The stored snapshot
/// is corrupted (one key code moved); the next hit's verification compares it with a fresh
/// prefill, serves the FRESH answer, and disables the cache for good.
#[test]
fn a_corrupted_snapshot_is_caught_and_disables_the_cache() {
    let (artifact, profile) = fixture();
    let cached = backend(&artifact, &profile).with_prefix_cache_v1(1 << 26, 1);
    let fresh = backend(&artifact, &profile);
    let full = prompt(20, 3);
    let head = &full[..10];
    let _ = answer(&cached, &job(&profile, head.len(), 2), head);
    cached.prefix_cache.as_ref().unwrap().lock().unwrap().corrupt_all_for_test();
    let j = job(&profile, full.len(), 6);
    let served = answer(&cached, &j, &full);
    assert_eq!(served.output_token_ids, answer(&fresh, &j, &full).output_token_ids, "the answer is the fresh one");
    assert_eq!(served.cached_prefix_tokens, 0, "the corrupted resume was discarded");
    assert!(cached.prefix_cache_stats_v1().unwrap().disabled, "and the cache is off for good");
    let again = answer(&cached, &job(&profile, full.len(), 2), &full);
    assert_eq!(again.cached_prefix_tokens, 0);
}

/// The decoder's own stop ends an answer-only run, and an EOG id ends it at that id.
#[test]
fn the_run_ends_at_a_stop_id_and_at_the_decoders_stop() {
    let (artifact, profile) = fixture();
    let b = backend(&artifact, &profile);
    let p = prompt(12, 1);
    let j = job(&profile, p.len(), 8);
    let all = answer(&b, &j, &p).output_token_ids;
    assert_eq!(all.len(), 8, "no stop id: the budget");
    let stop_at = all[3];
    let cut = b.answer_free_prompt_v1(&j, &p, &[stop_at], &mut |_| {}).unwrap();
    let first = all.iter().position(|t| *t == stop_at).unwrap();
    assert_eq!(cut.output_token_ids, all[..=first], "the run ends at the first stop id, which is its last id");
    assert!(cut.ended_on_stop_id);
    // A V4 stop sequence ends it too, with the sequence in the ids.
    let mut v4 = job(&profile, p.len(), 8);
    v4.version = PALW_FP_V4_VERSION;
    v4.decode = Some(DecodeConfigV4 { stop_sequences: vec![vec![all[1], all[2]]], ..DecodeConfigV4::NOOP });
    let stopped = answer(&b, &v4, &p);
    assert_eq!(stopped.output_token_ids, all[..3]);
    let committed = b.execute_free_prompt(&v4, &p).unwrap().output_token_ids;
    assert_eq!(stopped.output_token_ids, committed, "the committed run stops where the answer does");
}

/// The refusals a committed job names are the answer path's too, by name.
#[test]
fn the_answer_path_refuses_what_the_committed_path_refuses() {
    let (artifact, profile) = fixture();
    let b = backend(&artifact, &profile);
    let p = prompt(8, 0);
    let j = job(&profile, p.len(), 2);
    assert!(b.answer_free_prompt_v1(&j, &[], &[], &mut |_| {}).is_err(), "no prompt");
    let mut wrong = j.clone();
    wrong.prompt_tokens = 7;
    assert!(b.answer_free_prompt_v1(&wrong, &p, &[], &mut |_| {}).unwrap_err().contains("declares 7 prompt tokens"));
    let mut outside = p.clone();
    outside[0] = 5_000;
    assert!(b.answer_free_prompt_v1(&j, &outside, &[], &mut |_| {}).unwrap_err().contains("outside this class's vocabulary"));
}

use kaspa_consensus_core::palw_backend::PalwFpAnswerJobV1;

/// **ENGINE: a batched decode step is each sequence's own step** — the logits a row gets and the
/// cache it leaves are exactly what `forward_token_planned` gives it alone, for sequences at
/// different positions with different caches, in every order the rows can be in.
#[test]
fn a_batched_decode_step_is_each_sequences_own_step() {
    use crate::engine_a16::A16DecodeRowV1;
    let (artifact, profile) = fixture();
    let b = backend(&artifact, &profile);
    let engine = A16Engine::new(&artifact).unwrap();
    let plan = b.plan.as_ref().unwrap();
    let prompts = [prompt(9, 1), prompt(17, 2), prompt(5, 3), prompt(12, 4)];
    let tokens = [11usize, 99, 3, 64];
    let mut base: Vec<A16Cache> = Vec::new();
    for p in &prompts {
        let mut c = A16Cache::with_storage(artifact.shape.n_layers, b.runtime_profile);
        engine.forward_prefill_planned(plan, &mut c, p, 0, true).unwrap();
        base.push(c);
    }
    // Each alone.
    let mut alone_logits = Vec::new();
    let mut alone_caches = Vec::new();
    for (i, c) in base.iter().enumerate() {
        let mut c = c.clone();
        alone_logits.push(engine.forward_token_planned(plan, &mut c, tokens[i], prompts[i].len()).unwrap().0);
        alone_caches.push(c);
    }
    for order in [[0usize, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]] {
        let mut caches: Vec<A16Cache> = order.iter().map(|i| base[*i].clone()).collect();
        let mut rows: Vec<A16DecodeRowV1<'_>> = caches
            .iter_mut()
            .zip(order)
            .map(|(cache, i)| A16DecodeRowV1 { cache, token: tokens[i], position: prompts[i].len() })
            .collect();
        let logits = engine.forward_decode_batch_planned(plan, &mut rows).unwrap();
        drop(rows);
        for (k, i) in order.iter().enumerate() {
            assert_eq!(logits[k], alone_logits[*i], "order {order:?}: row {k} (sequence {i}) has its own logits");
            assert_eq!(caches[k].contents_as_i32(), alone_caches[*i].contents_as_i32(), "order {order:?}: and its own cache");
        }
    }
    // A batch of one is the single step.
    let mut c = base[1].clone();
    let one = engine
        .forward_decode_batch_planned(plan, &mut [A16DecodeRowV1 { cache: &mut c, token: tokens[1], position: prompts[1].len() }])
        .unwrap();
    assert_eq!(one[0], alone_logits[1]);
    // Refusals, by name.
    let mut wrong = base[0].clone();
    let err =
        engine.forward_decode_batch_planned(plan, &mut [A16DecodeRowV1 { cache: &mut wrong, token: 1, position: 3 }]).unwrap_err();
    assert!(format!("{err:?}").contains("does not hold exactly the positions before it"), "{err:?}");
    assert!(engine.forward_decode_batch_planned(plan, &mut []).is_err());
}

/// **BACKEND, GOLDEN: in-batch == alone.** Jobs of different prompt lengths, budgets, seeds,
/// temperatures and a V4 stop sequence, answered together (batch width 2, so five jobs are served
/// round-robin and finish at different times), each give the ids, the stop and the K/V the job
/// gives alone — and the batching is real (more rows than steps).
#[test]
fn in_batch_equals_alone() {
    let (artifact, profile) = fixture();
    let alone = backend(&artifact, &profile);
    let batched = backend(&artifact, &profile).with_decode_batch_width_v1(2);
    let prompts: Vec<Vec<usize>> = (0..5).map(|k| prompt(6 + 3 * k, 10 * k)).collect();
    let mut jobs: Vec<PalwFreePromptJobV3> = Vec::new();
    for (k, p) in prompts.iter().enumerate() {
        let mut j = job(&profile, p.len(), 3 + 2 * k as u32);
        j.sampling_seed = [k as u8 + 1; 32];
        j.temperature_q = if k % 2 == 0 { 1 << 24 } else { 0 };
        jobs.push(j);
    }
    // Job 3 carries a stop sequence taken from its own greedy answer.
    let greedy3 = alone.answer_free_prompt_v1(&jobs[3], &prompts[3], &[], &mut |_| {}).unwrap().output_token_ids;
    jobs[3].version = PALW_FP_V4_VERSION;
    jobs[3].decode = Some(DecodeConfigV4 { stop_sequences: vec![vec![greedy3[1], greedy3[2]]], ..DecodeConfigV4::NOOP });
    let expected: Vec<_> =
        jobs.iter().zip(&prompts).map(|(j, p)| alone.answer_free_prompt_v1(j, p, &[], &mut |_| {}).unwrap()).collect();
    let batch: Vec<PalwFpAnswerJobV1<'_>> =
        jobs.iter().zip(&prompts).map(|(job, prompt_tokens)| PalwFpAnswerJobV1 { job, prompt_tokens }).collect();
    let mut streamed: Vec<Vec<u32>> = vec![Vec::new(); jobs.len()];
    let got = batched.answer_batch_free_prompt_v1(&batch, &[], &mut |i, id| streamed[i].push(id)).unwrap();
    for k in 0..jobs.len() {
        assert_eq!(got[k].output_token_ids, expected[k].output_token_ids, "job {k}: in-batch == alone");
        assert_eq!(streamed[k], expected[k].output_token_ids, "job {k}: and it streamed exactly those ids");
    }
    assert_eq!(got[3].output_token_ids.len(), 3, "the stop sequence ended job 3's answer");
    // The same again with every job in a different position and a wider batch.
    let wide = backend(&artifact, &profile).with_decode_batch_width_v1(8);
    let reversed: Vec<PalwFpAnswerJobV1<'_>> = batch.iter().rev().copied().collect();
    let got = wide.answer_batch_free_prompt_v1(&reversed, &[], &mut |_, _| {}).unwrap();
    for (k, g) in got.iter().rev().enumerate() {
        assert_eq!(g.output_token_ids, expected[k].output_token_ids, "job {k}: order does not matter");
    }
}

/// The scheduler batches (rows > steps), is fair when the set is wider than the batch, and a
/// sequence admitted mid-flight gets exactly its own answer.
#[test]
fn the_scheduler_batches_is_fair_and_admits_mid_flight() {
    use crate::decode_scheduler::{DecodeSchedulerV1, DecodeSequenceV1};
    let (artifact, profile) = fixture();
    let b = backend(&artifact, &profile);
    let engine = A16Engine::new(&artifact).unwrap();
    let plan = b.plan.as_ref().unwrap();
    let start = |id: u64, p: &[usize], limit: u32| {
        let j = job(&profile, p.len(), limit);
        let (kv, _, logits) = b.answer_prefill_v1(&engine, &j, p).unwrap();
        DecodeSequenceV1::start(id, kv, p.len(), &logits, j.decoder_v1(), limit as usize, vec![])
    };
    let alone = |p: &[usize], limit: u32| {
        b.answer_free_prompt_v1(&job(&profile, p.len(), limit), p, &[], &mut |_| {}).unwrap().output_token_ids
    };
    let (pa, pb, pc) = (prompt(8, 1), prompt(10, 2), prompt(7, 3));
    let mut sched = DecodeSchedulerV1::new(2);
    sched.admit(start(0, &pa, 6));
    sched.admit(start(1, &pb, 6));
    let mut seen: std::collections::BTreeMap<u64, Vec<u32>> = Default::default();
    let mut collect = |sched: &mut DecodeSchedulerV1| {
        for (id, tok) in sched.drain_events() {
            seen.entry(id).or_default().push(tok);
        }
    };
    collect(&mut sched);
    assert_eq!(sched.step(&engine, plan).unwrap(), 2);
    collect(&mut sched);
    // A third sequence arrives while two are mid-answer; the batch is 2 wide so it waits its turn.
    sched.admit(start(2, &pc, 6));
    collect(&mut sched);
    assert_eq!(sched.running(), 3);
    let mut guard = 0;
    while !sched.is_idle() {
        sched.step(&engine, plan).unwrap();
        collect(&mut sched);
        guard += 1;
        assert!(guard < 64, "the scheduler makes progress");
    }
    let finished: std::collections::BTreeMap<u64, Vec<u32>> =
        sched.take_finished().into_iter().map(|f| (f.id, f.output_token_ids)).collect();
    assert_eq!(finished[&0], alone(&pa, 6));
    assert_eq!(finished[&1], alone(&pb, 6));
    assert_eq!(finished[&2], alone(&pc, 6));
    for (id, ids) in &finished {
        assert_eq!(&seen[id], ids, "sequence {id}: the events are the answer");
    }
    let (steps, rows) = sched.counters();
    assert!(rows > steps, "the steps were batched: {rows} rows in {steps} steps");
}

/// **RFC-0001 §2.8: the embedding is the pooled last-layer hidden state, deterministic, and the
/// last-token form resumes from the prefix cache to the same bits.**
#[test]
fn an_embedding_is_the_pooled_last_layer_state_and_the_cache_does_not_change_it() {
    use kaspa_consensus_core::palw_embedding_pool_v1::{PalwEmbeddingPoolV1, palw_embedding_pool_v1};
    let (artifact, profile) = fixture();
    let plain = backend(&artifact, &profile);
    let cached = backend(&artifact, &profile).with_prefix_cache_v1(1 << 26, 0);
    let (class, tok) = (profile.shape_profile_id(), Hash64::from_u64_word(0x70));
    let p = prompt(15, 4);
    // Reference: the engine's own per-position last-layer rows, walked one position at a time.
    let engine = A16Engine::new(&artifact).unwrap();
    let plan = plain.plan.as_ref().unwrap();
    let mut c = A16Cache::with_storage(artifact.shape.n_layers, plain.runtime_profile);
    let mut rows = Vec::new();
    for (i, t) in p.iter().enumerate() {
        let (_, trace) = engine.forward_token_planned(plan, &mut c, *t, i).unwrap();
        rows.push(trace.attn.last().unwrap().last().unwrap().clone());
    }
    for pool in [PalwEmbeddingPoolV1::Mean, PalwEmbeddingPoolV1::LastToken] {
        let want = palw_embedding_pool_v1(&rows, pool).unwrap();
        let got = plain.embed_prompt_v1(&p, class, tok, pool).unwrap();
        assert_eq!(got.raw, want, "{pool:?}: the pooled one-pass prefill rows are the stepped walk's");
        assert_eq!(got.raw.len(), artifact.shape.d_model());
        assert_eq!(got.positions as usize, p.len());
        assert_eq!(plain.embed_prompt_v1(&p, class, tok, pool).unwrap(), got, "{pool:?}: deterministic");
    }
    // Last-token with a cache: the first call fills it, the second extends the prompt and resumes.
    let first = cached.embed_prompt_v1(&p, class, tok, PalwEmbeddingPoolV1::LastToken).unwrap();
    assert_eq!(first.cached_prefix_tokens, 0);
    let mut longer = p.clone();
    longer.extend(prompt(6, 90));
    let resumed = cached.embed_prompt_v1(&longer, class, tok, PalwEmbeddingPoolV1::LastToken).unwrap();
    assert_eq!(resumed.cached_prefix_tokens as usize, p.len(), "the longer prompt resumed from the shorter one");
    assert_eq!(
        resumed.raw,
        plain.embed_prompt_v1(&longer, class, tok, PalwEmbeddingPoolV1::LastToken).unwrap().raw,
        "resumed == fresh"
    );
    // Mean pooling never reads the cache (it needs every position's row).
    let mean = cached.embed_prompt_v1(&longer, class, tok, PalwEmbeddingPoolV1::Mean).unwrap();
    assert_eq!(mean.cached_prefix_tokens, 0);
    // Refusals, by name.
    assert!(plain.embed_prompt_v1(&[], class, tok, PalwEmbeddingPoolV1::Mean).unwrap_err().contains("no tokens"));
    assert!(
        plain
            .embed_prompt_v1(&[9_999], class, tok, PalwEmbeddingPoolV1::Mean)
            .unwrap_err()
            .contains("outside this class's vocabulary")
    );
}
