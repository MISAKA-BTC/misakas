//! **A seat's verdict is a function of its own backend instance, and of nothing else in the
//! process** (ADR-0082 Decision 9; ADR-0110 §9.5).
//!
//! The regression test for the flake of 2026-09-24:
//! `qwen25_a16_backend::held_real_row_probe::a_lie_in_a_block_that_straddles_an_intervals_edge_is_named_from_the_edges`
//! failed once in a full parallel run with `left: Unverifiable`,
//! `right: FaultInRange { first_leaf_index: 15104, leaf_count: 1280 }`, and passed every time
//! alone. The seat's row check read its state from ONE process-wide slot
//! (`fp_recompute::SEAT_STATE_MEMO`, with the dense walk in `A16_WALK` beside it), so any other
//! computation in the process — a neighbouring test's recompute, its
//! `base0_fp_seat_state_forget_v1()`, or in a node another duty or an executor's opening — could
//! evict the state between the seat's recompute and its row check, and an honest producer was
//! filed `Unverifiable`. The state and the walk are now owned by the backend instance
//! (`fp_recompute::Base0FpSeatMemoV1`), so no other instance can reach them.
//!
//! This replays the flaky test's setup and then, deterministically, performs each eviction a
//! neighbour used to perform by chance, on a SECOND instance of the same class: its forget, its
//! executor fold and recompute of another job, its recompute of the same job at another covered
//! call, and a thread that loops forget-and-recompute while this one verifies. Every one of those
//! turned the verdict `Unverifiable` under the process-wide slot; none of them may move it now. The
//! instance's OWN forget still does, which is what says the verdict is read from the instance.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3, fp_job_id_v3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;

const FIXTURE_NETWORK: u64 = 1 << 12;

/// `held_real_row_probe::fixture()`: the held graph-v7 class at fixture scale.
fn fixture() -> (Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
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
    let artifact = Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    (artifact, profile)
}

/// `held_real_row_probe::fixture_backend()`: one instance of the class at the fixture's network
/// ladder. Every call is a separate instance with its own memo.
fn fixture_backend(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), b"misaka-palw-rc".to_vec(), profile.clone(), (64, 8))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(FIXTURE_NETWORK)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
}

/// The straddle test's job, with the nonce free so a neighbour can run another one.
fn job_with_nonce(profile: &PalwShapeProfileV3, form: PalwPromptIdsFormV1, ids: &[u32], nonce: u8) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [nonce; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, ids).expect("the ids commit"),
        prompt_tokens: 64,
        decode_token_limit: 8,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    }
}

/// **No computation on another instance moves a seat's verdict; its own forget does.** Under the
/// process-wide slot, steps (1)–(4) each turned the straddle verdict `Unverifiable`: this is the
/// flake of `a_lie_in_a_block_that_straddles_an_intervals_edge_is_named_from_the_edges`, made
/// deterministic.
#[test]
fn a_seats_verdict_is_moved_by_no_computation_but_its_own_backends() {
    let (artifact, profile) = fixture();
    let honest = fixture_backend(&artifact, &profile);
    let form = honest.prompt_ids_form();
    let vocab = artifact.shape.vocab;
    let prompt: Vec<usize> = (0..64usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();

    // ---- The straddle test's setup: the honest run, its middle interval and that interval's anchor.
    let job = job_with_nonce(&profile, form, &ids, 0x5A);
    let run = honest.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let (capture, leaves) = (run.outcome.material.clone(), run.facts.step_leaf_count);
    let intervals = honest.fp_interval_count(&capture).expect("intervals");
    let mid = intervals / 2;
    let opening = honest.open_fp_interval(&capture, mid, &ids).expect("the interval opens");
    let v4 = Base0FpIntervalOpeningV4::decode_v1(&opening).expect("V4");
    let anchor = v4.anchor.as_ref().expect("the middle interval resumes from a checkpoint").leaf.covered_decode_call;
    let ctx = v4.binding.job_context.clone();
    let (left, right) = v4.range.edges_v1(leaves);
    assert!(left.0 < left.1 && right.0 < right.1, "the middle interval's range starts and ends inside a block: {left:?} {right:?}");
    let expected = PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index: left.0, leaf_count: left.1 - left.0 };

    // The liar, on the right edge, as the straddle test builds it: its own instance.
    let leaf = right.0 + (right.1 - right.0) / 2;
    let liar = fixture_backend(&artifact, &profile);
    let lying = liar.execute_free_prompt_with_injected_fault(&job, &prompt, leaf).expect("the drill's liar runs");
    let lying_capture = lying.outcome.material.clone();
    let lying_claim = PalwClaimRootsV1 {
        execution_root: lying.outcome.execution_root,
        trace_root: lying.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        // SEAT-S2's field (merged after this test was written): a fixture with no claim output.
        output_root: None,
    };
    let lie = liar.open_fp_interval(&lying_capture, mid, &ids).expect("the liar serves its interval");

    // The seat's two questions, on the honest instance only.
    let recompute = || {
        honest.checkpoint_root_for_context_v1(&ctx, &ids, &run.output_token_ids, anchor).expect("the seat recomputes");
    };
    let verify = || honest.verify_fp_interval_opening(&lie, lying_claim, mid, &ids, leaves);

    // ---- (0) Control: the seat recomputes, and nothing runs in between.
    recompute();
    assert_eq!(verify(), expected, "(0) control: the straddle verdict is addressed at the left edge");

    // ---- (1) A neighbour instance forgets (tests that count forward passes did; so did kaspad).
    let neighbour = fixture_backend(&artifact, &profile);
    neighbour.fp_forget_seat_state_v1();
    assert_eq!(verify(), expected, "(1) a neighbour's forget does not reach this seat's state");

    // ---- (2) The neighbour runs another job, opens it as an executor (the fold warms its own
    // walk), and recomputes a seat state for it.
    let job2 = job_with_nonce(&profile, form, &ids, 0x77);
    let run2 = neighbour.execute_free_prompt(&job2, &prompt).expect("the neighbour's job runs");
    let mid2 = neighbour.fp_interval_count(&run2.outcome.material).expect("intervals") / 2;
    let opening2 = neighbour.open_fp_interval(&run2.outcome.material, mid2, &ids).expect("the neighbour's interval opens");
    let v4_2 = Base0FpIntervalOpeningV4::decode_v1(&opening2).expect("V4");
    let anchor2 = v4_2.anchor.as_ref().expect("a checkpoint").leaf.covered_decode_call;
    let ctx2 = v4_2.binding.job_context.clone();
    assert_ne!(ctx2.context_hash(), ctx.context_hash(), "a different job is a different context");
    neighbour.checkpoint_root_for_context_v1(&ctx2, &ids, &run2.output_token_ids, anchor2).expect("the neighbour's seat recomputes");
    assert_eq!(verify(), expected, "(2) another instance's executor fold and recompute of another job do not evict this seat's");

    // ---- (3) The neighbour recomputes THIS job at another interval's anchor. The other anchor is
    // found through the neighbour, so the honest instance runs nothing but its own two questions.
    let other_covered = (0..intervals)
        .filter(|i| *i != mid)
        .filter_map(|i| neighbour.open_fp_interval(&capture, i, &ids).ok())
        .filter_map(|o| Base0FpIntervalOpeningV4::decode_v1(&o).ok())
        .filter_map(|o| o.anchor.map(|a| a.leaf.covered_decode_call))
        .find(|c| *c != anchor)
        .expect("another interval resumes from another checkpoint");
    neighbour.checkpoint_root_for_context_v1(&ctx, &ids, &run.output_token_ids, other_covered).expect("another covered recomputes");
    assert_eq!(verify(), expected, "(3) the same job's other anchor, on another instance, does not evict this seat's");

    // ---- (4) A real second thread: the neighbour loops {forget; recompute another job} while
    // this thread verifies, with no recompute in between. The first verify waits for the
    // neighbour's first round, so every round below runs after at least one foreign eviction.
    let stop = AtomicBool::new(false);
    let rounds_done = AtomicU64::new(0);
    let neighbour_rounds = std::thread::scope(|scope| {
        let looping = scope.spawn(|| {
            loop {
                neighbour.fp_forget_seat_state_v1();
                neighbour
                    .checkpoint_root_for_context_v1(&ctx2, &ids, &run2.output_token_ids, anchor2)
                    .expect("the neighbour's seat recomputes");
                rounds_done.fetch_add(1, Ordering::SeqCst);
                if stop.load(Ordering::SeqCst) {
                    break;
                }
            }
        });
        while rounds_done.load(Ordering::SeqCst) == 0 && !looping.is_finished() {
            std::thread::yield_now();
        }
        let verdicts: Vec<_> = (0..8).map(|_| verify()).collect();
        stop.store(true, Ordering::SeqCst);
        looping.join().expect("the neighbour thread");
        for (round, verdict) in verdicts.into_iter().enumerate() {
            assert_eq!(verdict, expected, "(4) round {round}: a racing neighbour's forget and recompute do not reach this seat");
        }
        rounds_done.load(Ordering::SeqCst)
    });
    assert!(neighbour_rounds > 0, "(4) the neighbour must actually have run while this seat verified");

    // ---- (5) The instance's OWN forget is what drops the state — the verdict is read from here.
    honest.fp_forget_seat_state_v1();
    assert_eq!(
        verify(),
        PalwFpIntervalVerdictV1::Unverifiable,
        "(5) with its own state forgotten the seat cannot judge, and says so honestly"
    );
    recompute();
    assert_eq!(verify(), expected, "(5) and its own recompute restores the verdict");
}
