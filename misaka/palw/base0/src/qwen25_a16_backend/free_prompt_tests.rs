//! **`qwen25_a16_backend`'s `free_prompt_tests` module**, moved out of `qwen25_a16_backend.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;
use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
use crate::engine_a16::derived_a16_store;
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_execution_root_v3, palw_fp_job_context_v3};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_a16_profile_v2};
use kaspa_consensus_core::palw_state_chunk_map as map;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const NETWORK: &[u8] = b"misaka-palw-rc";

/// A class small enough to run in a unit test, built from ONE geometry so the artifact and the
/// profile cannot describe different models — which is the failure a hand-written pair invites.
fn class(map_id: Hash64) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    class_from(map_id, false)
}

/// `corrected` builds the class `qwen25_a16_profile_v2` describes: the pre table names the
/// embed-lift requant and the state map is the four-byte one.
fn class_from(map_id: Hash64, corrected: bool) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
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
    let mut profile = if corrected {
        qwen25_a16_profile_v2(geometry).expect("a valid corrected A16 profile")
    } else {
        qwen25_a16_profile_v1(geometry).expect("a valid A16 profile")
    };
    if !corrected {
        profile.state_chunk_map_id = map_id;
    }
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("a valid shape")
        .with_a16_params(derived_a16_store(&shape))
        .expect("the derived store is sorted and unique");
    (std::sync::Arc::new(artifact), profile)
}

fn job(profile: &PalwShapeProfileV3, prompt_tokens: u32, decode: u32) -> PalwFreePromptJobV3 {
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
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::from_u64_word(0x71),
        prompt_tokens,
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

/// **The corrected class runs a caller's prompt and commits a root the derivation recomputes.**
///
/// This is the property everything else was for: a language model's own execution, under a
/// class whose graph names what the engine does and whose state map is the width the cache
/// holds, producing a free-prompt commitment a court could recompute. The two defects that
/// stopped it were measured, not guessed, and correcting either one moves the class id — so
/// this is a class to register, and the test is what says it would work once registered.
/// **The runtime profile is invisible to every root, byte and verdict** (ADR-0151 follow-up,
/// item 7). The same class and the same job, one backend holding `A16-KV-i32` and one
/// `A16-KV-i16` — on the v2, v5 and v7 graphs — commit the same `execution_root`, `trace_root`
/// and `output_root`, the same retained material to the byte, the same replay roots for a
/// verdict, and therefore the same canonical work id (`palw_execution_canonical_work_id_v1`)
/// and the same quanta: 16 GiB or 7 GiB of cache, the same reward. And the court's half: a
/// capture the `i32` producer retained is resumed by the `i16` seat and by the `i32` one
/// (`replay_accused_segment_v1`), segment for segment, with the same leaf hashes and the same
/// `matches` — the held attempt's fold included, which serves its segments since SEAT-S4.
#[test]
fn the_backend_commits_the_same_roots_under_every_runtime_profile() {
    use kaspa_consensus_core::palw_execution_quanta_v1::palw_execution_canonical_work_id_v1;
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_profile_v5, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1;
    use kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2;
    let (artifact, v2) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
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
    let v5 = qwen25_a16_profile_v5(geometry).expect("the v5 row projects");
    let v7 = qwen25_a16_profile_v7(geometry).expect("the v7 row projects");
    for (name, profile) in [("v2", v2), ("v5", v5), ("v7", v7)] {
        let build = |runtime: PalwRuntimeProfileV1| {
            Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (5, 3))
                .expect("the fixture's declaration is this engine's program")
                .with_runtime_profile(runtime)
        };
        let oracle = build(PalwRuntimeProfileV1::A16KvI32);
        let compact = build(PalwRuntimeProfileV1::A16KvI16);
        assert_eq!(oracle.runtime_profile_v1(), Some(PalwRuntimeProfileV1::A16KvI32));
        assert_eq!(compact.runtime_profile_v1(), Some(PalwRuntimeProfileV1::A16KvI16));
        let anchor = Hash64::from_u64_word(0x1616_2026);
        let (job, prompt) = oracle.job_for_anchor(anchor).expect("the anchor implies a job");
        assert_eq!(compact.job_for_anchor(anchor).expect("job").0.context_hash(), job.context_hash(), "{name}: one job");

        let a = oracle.execute(&job, &prompt).expect("the i32 backend executes");
        let b = compact.execute(&job, &prompt).expect("the i16 backend executes");
        assert_eq!(a.execution_root, b.execution_root, "{name}: the execution root");
        assert_eq!(a.trace_root, b.trace_root, "{name}: the trace root");
        assert_eq!(a.output_root, b.output_root, "{name}: the output root");
        assert_eq!(a.trace_manifest_root, b.trace_manifest_root, "{name}: the manifest root");
        assert_eq!(a.trace_chunk_count, b.trace_chunk_count);
        assert_eq!(a.material, b.material, "{name}: the retained material, byte for byte");
        assert_eq!(
            palw_execution_canonical_work_id_v1(a.execution_root),
            palw_execution_canonical_work_id_v1(b.execution_root),
            "{name}: the canonical work id, and with it every quantum"
        );

        let ra = oracle.execute_for_verdict(&job, &prompt).expect("the i32 verdict replay");
        let rb = compact.execute_for_verdict(&job, &prompt).expect("the i16 verdict replay");
        assert_eq!(
            (ra.execution_root, ra.trace_root, ra.work_leaves),
            (rb.execution_root, rb.trace_root, rb.work_leaves),
            "{name}: the verdict roots"
        );
        assert_eq!(ra.execution_root, a.execution_root, "{name}: and they are the claim's");

        // The court's half: the i32 producer's capture, resumed by both seats. A held class's attempt
        // folds (`a_held_classs_attempt_folds_and_stays_adjudicable`) and serves its S1 opening from
        // the fold — genesis, proven at its retained level (SEAT-S4); the dense capture of the same
        // job, as a server holding one would serve it, proves each segment exactly. Every segment of
        // either resumes under both representations to a match, with the same leaf hashes.
        let seats = 4u16;
        let folds = kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&profile);
        let mut captures = vec![a.material.clone()];
        if folds {
            let dense =
                a16_execute_for_attempt_capped_v1(&artifact, &profile, oracle.plan.as_ref(), &job, &prompt, oracle.step_ladder_cap())
                    .expect("the dense capture of the same job");
            assert_eq!(dense.execution_root, a.execution_root, "{name}: the dense capture is the same execution");
            captures.push(crate::produce::base0_material_encode_v1(&dense).expect("encodes"));
        }
        let mut resumed = 0usize;
        for segment_capture in &captures {
            for segment in 0..palw_segment_count_v2(seats) {
                let x = compact.replay_accused_segment_v1(segment_capture, seats, segment, &job, &prompt);
                let y = oracle.replay_accused_segment_v1(segment_capture, seats, segment, &job, &prompt);
                let (x, y) = (x.unwrap_or_else(|e| panic!("{name} segment {segment}: {e}")), y.expect("and under i32"));
                assert!(x.matches && y.matches, "{name} segment {segment}: the resumed segment matches the capture");
                assert_eq!(x.leaf_hashes, y.leaf_hashes, "{name} segment {segment}: the same leaf hashes");
                assert_eq!(x.window, y.window);
                resumed += 1;
            }
        }
        assert!(resumed >= palw_segment_count_v2(seats) as usize, "{name}: every segment resumes");
    }
}

/// **Every prefill run width a node can choose commits the same bits, on the producer's path and the
/// replay's** (int-10.2 A2; ADR-0117 Decision 2). A node narrows the width on the instance it reserved
/// memory for (`set_prefill_run_positions_v1`, the trait door it uses) when the ledger cannot grant the
/// default's trace scratch; that instance must then commit what the default would. Over the v2, v5 and
/// v7 graphs, on a 13-position prefill — so 64, 32 and 16 walk it in one run and 8, 4, 2 and 1 in two to
/// thirteen — at every width the node may choose: the attempt (`execute`: the four roots, the manifest
/// and the retained material byte for byte), the verdict replay (`execute_for_verdict`), the free
/// prompt (`execute_free_prompt`: the outcome, the facts and the output ids), and every segment of
/// the default-width producer's capture resumed by a seat at that width (`replay_accused_segment_v1`:
/// the same leaf hashes, a match) are the default width's. And the resource profile prices exactly the
/// width the instance runs: the trace scratch is proportional to it and nothing else moves.
#[test]
fn the_backend_commits_the_same_roots_at_every_prefill_run_width() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_profile_v5, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1;
    use kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2;
    let (artifact, v2) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
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
    let v5 = qwen25_a16_profile_v5(geometry).expect("the v5 row projects");
    let v7 = qwen25_a16_profile_v7(geometry).expect("the v7 row projects");
    let widths = [64u32, 32, 16, 8, 4, 2, 1];
    // The free-prompt job builder, named apart from the anchor's `job` below.
    let fp_job_of = job;
    let mut checked = 0usize;
    for (name, profile) in [("v2", v2), ("v5", v5), ("v7", v7)] {
        let build = || {
            Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (13, 3))
                .expect("the fixture's declaration is this engine's program")
        };
        let reference = build();
        assert_eq!(reference.prefill_run_positions_v1(), Some(A16_PREFILL_RUN_POSITIONS as u32), "{name}: the default width");
        let anchor = Hash64::from_u64_word(0x2026_1001);
        let (job, prompt) = reference.job_for_anchor(anchor).expect("the anchor implies a job");
        assert_eq!(job.declared_prefill_tokens, 13, "the premise: a prefill narrower widths split");
        let want = reference.execute(&job, &prompt).expect("the default width executes");
        let want_verdict = reference.execute_for_verdict(&job, &prompt).expect("the default width replays");
        let fp_prompt: Vec<usize> = (0..11).map(|i| (i * 7 + 3) % 64).collect();
        let fp_ids: Vec<u32> = fp_prompt.iter().map(|t| *t as u32).collect();
        let fp = PalwFreePromptJobV3 {
            prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                reference.prompt_ids_form(),
                &fp_ids,
            )
            .expect("the ids commit"),
            ..fp_job_of(&profile, 11, 3)
        };
        let want_fp = reference.execute_free_prompt(&fp, &fp_prompt).expect("the default width runs a free prompt");
        let want_profile = reference.resource_profile_v1(Some(&job), PalwResourceRoleV1::FullSeat).expect("a profile");
        let seats = 4u16;
        for width in widths {
            let mut b = build();
            b.set_prefill_run_positions_v1(width);
            assert_eq!(b.prefill_run_positions_v1(), Some(width), "{name} {width}: the instance runs at the width it was set to");
            let at = format!("{name} at a prefill run of {width}");
            let got = b.execute(&job, &prompt).unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(
                (got.execution_root, got.trace_root, got.output_root, got.trace_manifest_root, got.trace_chunk_count),
                (want.execution_root, want.trace_root, want.output_root, want.trace_manifest_root, want.trace_chunk_count),
                "{at}: the attempt's roots"
            );
            assert_eq!(got.material, want.material, "{at}: the retained material, byte for byte");
            let verdict = b.execute_for_verdict(&job, &prompt).unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(
                (verdict.execution_root, verdict.trace_root, verdict.work_leaves, verdict.output_root),
                (want_verdict.execution_root, want_verdict.trace_root, want_verdict.work_leaves, want_verdict.output_root),
                "{at}: the verdict replay"
            );
            let run = b.execute_free_prompt(&fp, &fp_prompt).unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(run.outcome.execution_root, want_fp.outcome.execution_root, "{at}: the free prompt's execution root");
            assert_eq!(run.outcome.output_root, want_fp.outcome.output_root, "{at}: the free prompt's output root");
            assert_eq!(run.outcome.material, want_fp.outcome.material, "{at}: the free prompt's material");
            assert_eq!(run.facts, want_fp.facts, "{at}: the free prompt's facts");
            assert_eq!(run.output_token_ids, want_fp.output_token_ids, "{at}: the answer");
            for segment in 0..palw_segment_count_v2(seats) {
                let resumed = b
                    .replay_accused_segment_v1(&want.material, seats, segment, &job, &prompt)
                    .unwrap_or_else(|e| panic!("{at} segment {segment}: {e}"));
                let by_default = reference
                    .replay_accused_segment_v1(&want.material, seats, segment, &job, &prompt)
                    .expect("the default width resumes it");
                assert!(resumed.matches, "{at} segment {segment}: the producer's capture, resumed");
                assert_eq!(resumed.leaf_hashes, by_default.leaf_hashes, "{at} segment {segment}: the same leaf hashes");
            }
            let priced = b.resource_profile_v1(Some(&job), PalwResourceRoleV1::FullSeat).expect("a profile");
            // Since d3ff09371 the scratch also holds the job's output material, which no width moves: only the run's rows
            // scale with the width.
            let output = kaspa_consensus_core::palw_resource_profile_v1::palw_output_material_bytes_v1(&profile, &job);
            assert_eq!(
                (priced.trace_scratch_bytes - output) * u64::from(A16_PREFILL_RUN_POSITIONS as u32),
                (want_profile.trace_scratch_bytes - output) * u64::from(width),
                "{at}: the trace scratch is the width's"
            );
            assert_eq!(
                priced.working_set_bytes() - priced.trace_scratch_bytes,
                want_profile.working_set_bytes() - want_profile.trace_scratch_bytes,
                "{at}: and nothing else moves with it"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 3 * widths.len(), "every graph at every width");
}

/// **A seat's replay of the real 8k row, timed at the widths a node narrows to** (int-10.2 A2). Off unless
/// `MISAKA_PALW_8K_ARTIFACT` names the converted `qwen25-1.5b-a16-8k.palwart` — 1.7 GiB of weights are not a
/// unit test's input. The artifact is mapped the way the dense lineage maps it, the class is testnet-12's
/// `graph-v7@8192` row at its canonical (1023, 2), and the anchor's job is replayed for a verdict
/// (`execute_for_verdict`, the SEAT-R full seat's call) at each width in `MISAKA_PALW_8K_WIDTHS` (default
/// `64,32,16,8`) — every one must commit the same roots — with the time and the width's derived need printed:
///
/// ```text
/// MISAKA_PALW_8K_ARTIFACT=/path/qwen25-1.5b-a16-8k.palwart RAYON_NUM_THREADS=4 \
///   cargo test --release -p misaka-palw-base0 --lib -- a_real_8k_replay_at_every_width --nocapture
/// ```
#[test]
fn a_real_8k_replay_at_every_width_commits_one_set_of_roots() {
    use kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1;
    let Ok(path) = std::env::var("MISAKA_PALW_8K_ARTIFACT") else {
        eprintln!("8k replay: skipped — set MISAKA_PALW_8K_ARTIFACT to the 8k .palwart to time it");
        return;
    };
    let widths: Vec<u32> = std::env::var("MISAKA_PALW_8K_WIDTHS")
        .ok()
        .map(|v| v.split(',').filter_map(|w| w.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![64, 32, 16, 8]);
    let map =
        std::sync::Arc::new(crate::mmap::ReadOnlyMap::open(std::path::Path::new(&path)).unwrap_or_else(|e| panic!("{path}: {e}")));
    let started = std::time::Instant::now();
    let artifact =
        std::sync::Arc::new(crate::artifact::decode_artifact_file_mapped_v1(map).unwrap_or_else(|e| panic!("{path}: {e:?}")));
    eprintln!("8k replay: mapped and decoded in {:.1?} (the digest pass)", started.elapsed());
    let row = crate::classes::a16_graph_v7_row_v1(8_192, crate::classes::A16_GRAPH_V7_8K_MODEL_ID).expect("the 8k row");
    let mut backend = Qwen25A16Backend::new(artifact, b"misaka-palw-rc".to_vec(), row.profile.clone(), row.canonical_job)
        .expect("the real artifact is the row's");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x8000_1001)).expect("the anchor's job");
    let mut first: Option<kaspa_consensus_core::palw_backend::PalwReplayRootsV1> = None;
    for width in widths {
        backend.set_prefill_run_positions_v1(width);
        let need = backend.resource_profile_v1(Some(&job), PalwResourceRoleV1::FullSeat).expect("a profile");
        let started = std::time::Instant::now();
        let roots = backend.execute_for_verdict(&job, &prompt).expect("the replay runs");
        let took = started.elapsed();
        eprintln!(
            "8k replay at a prefill run of {width:>2}: {:.1?} for {} prefill positions ({:.1} ms a position), working set {:.3} GiB \
                 (trace scratch {:.3} GiB), {} rayon threads",
            took,
            job.declared_prefill_tokens,
            took.as_secs_f64() * 1e3 / f64::from(job.declared_prefill_tokens.max(1)),
            need.working_set_bytes() as f64 / (1u64 << 30) as f64,
            need.trace_scratch_bytes as f64 / (1u64 << 30) as f64,
            rayon::current_num_threads()
        );
        match &first {
            None => first = Some(roots),
            Some(want) => assert_eq!(
                (roots.execution_root, roots.trace_root, roots.work_leaves, roots.output_root),
                (want.execution_root, want.trace_root, want.work_leaves, want.output_root),
                "width {width}: the roots of the first width"
            ),
        }
    }
}

/// **A held class's attempt folds, and everything the court reads is the dense capture's** (ADR-0151
/// follow-up; the 2026-09-23 kills). On a small held fixture: `execute` retains v2 folded material
/// with no tiles and a tree; its four roots equal a dense capture's of the same job
/// (`a16_execute_for_attempt_capped_v1`) and the verdict replay's; the seat check reads `Matches`
/// from the folded material; a rung's prefix state and a leaf's refutation are answered from it and
/// are the dense material's; the drill's tile tamper is refused by name; and the resource profile
/// prices the attempt as a fold. A class outside the held regime keeps its dense tiles, so the
/// attempt-lane court that reads them is unchanged there.
#[test]
fn a_held_classs_attempt_folds_and_stays_adjudicable() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7;
    use kaspa_consensus_core::palw_resource_profile_v1::{PalwCaptureRetentionV1, PalwResourceRoleV1, palw_attempt_capture_folds_v1};
    let (artifact, v2) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
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
    let held = qwen25_a16_profile_v7(geometry).expect("the held row projects");
    assert!(palw_attempt_capture_folds_v1(&held) && !palw_attempt_capture_folds_v1(&v2));

    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), held.clone(), (6, 3)).expect("servable");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0151_F01D)).expect("a job");
    let folded = backend.execute(&job, &prompt).expect("the held attempt executes");
    let material = crate::produce::base0_material_decode_any_v1(&folded.material).expect("decodes");
    let crate::produce::Base0RetentionV1::Folded(m) = &material else { panic!("a held attempt retains the fold") };
    assert!(material.tiles().is_none(), "no tiles were retained");
    assert!(m.step_tree.retained_len() >= 1);
    assert_eq!(m.prompt_token_ids, prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(), "the anchor's prompt rides the material");

    // The roots are the dense sink's, to the bit.
    let dense = a16_execute_for_attempt_capped_v1(&artifact, &held, backend.plan.as_ref(), &job, &prompt, backend.step_ladder_cap())
        .expect("the dense capture of the same job");
    assert_eq!(folded.execution_root, dense.execution_root, "the fold's execution root is the dense capture's");
    assert_eq!(folded.trace_root, dense.trace_root);
    assert_eq!(folded.output_root, dense.output_root);
    assert_eq!(folded.trace_manifest_root, dense.trace_manifest_root);
    let verdict = backend.execute_for_verdict(&job, &prompt).expect("the verdict replay");
    assert_eq!((verdict.execution_root, verdict.trace_root), (folded.execution_root, folded.trace_root));

    // The seat check and the court verbs read the folded material.
    let claim = PalwClaimRootsV1 {
        execution_root: folded.execution_root,
        trace_root: folded.trace_root,
        anchor: job.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    assert_eq!(backend.verify_material(&folded.material, claim), PalwMaterialVerdictV1::Matches);
    let dense_material = crate::produce::base0_material_encode_v1(&dense).expect("the dense material encodes");
    let leaves = dense.binding.step_leaf_count;
    for index in [0u64, leaves / 3, leaves / 2, leaves - 1] {
        assert_eq!(
            backend.bisect_prefix_state(&folded.material, index),
            backend.bisect_prefix_state(&dense_material, index),
            "a rung at {index} is answered from the fold as from the tiles"
        );
    }
    let refutation = backend.refutation_for_index(&folded.material, leaves / 2 + 1).expect("a leaf opens from the fold");
    let from_dense = backend.refutation_for_index(&dense_material, leaves / 2 + 1).expect("and from the tiles");
    assert_eq!(refutation.binding.committed_execution_root, folded.execution_root);
    assert_eq!(refutation.output_preimage, from_dense.output_preimage, "the same leaf, the same preimage");

    // The drill tampers a DENSE capture even on the held class: its lie is a retained tile, the
    // roots move, the seat check reads `Mismatch` against the honest claim, and the leaf opens
    // from the tiles while this instance's replays stay honest.
    let guilty = backend.execute_with_injected_fault(&job, &prompt, 3).expect("the drill's dense tamper commits");
    assert_ne!(guilty.execution_root, folded.execution_root, "the lie moved the commitment");
    assert!(matches!(crate::produce::base0_material_decode_any_v1(&guilty.material), Ok(crate::produce::Base0RetentionV1::Dense(_))));
    assert_eq!(backend.verify_material(&guilty.material, claim), PalwMaterialVerdictV1::Mismatch, "a lie is not the honest claim");
    assert!(backend.refutation_for_index(&guilty.material, 3).is_ok(), "the tampered leaf opens from the retained tiles");
    assert_eq!(
        backend.execute_for_verdict(&job, &prompt).expect("replays").execution_root,
        folded.execution_root,
        "the replays stay honest"
    );
    let profile = backend.resource_profile_v1(Some(&job), PalwResourceRoleV1::Producer).expect("derives");
    assert!(matches!(profile.capture, PalwCaptureRetentionV1::Fold { .. }), "{:?}", profile.capture);
    assert_eq!(profile.leaves, leaves);
    // The fold prices its retained digests plus the one open block it buffers (`palw_fold_capture_bytes_v1`, d3ff09371:
    // 512 B a digest, 256 B a leaf of the block): a small job is that buffer and almost nothing else.
    let PalwCaptureRetentionV1::Fold { retain_level } = profile.capture else { unreachable!() };
    assert_eq!(
        profile.capture_retained_bytes,
        kaspa_consensus_core::palw_resource_profile_v1::palw_fold_capture_bytes_v1(leaves, retain_level),
        "the fold is priced by the one formula"
    );
    assert!(
        leaves <= 1u64 << retain_level,
        "a small job folds into one block, so its fold is that block's buffer: {leaves} leaves, level {retain_level}"
    );

    // A class outside the regime keeps its tiles, priced as such.
    let dense_backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), v2, (6, 3)).expect("servable");
    let (job2, prompt2) = dense_backend.job_for_anchor(Hash64::from_u64_word(0x0151_DE45)).expect("a job");
    let outcome = dense_backend.execute(&job2, &prompt2).expect("executes");
    assert!(matches!(crate::produce::base0_material_decode_any_v1(&outcome.material), Ok(crate::produce::Base0RetentionV1::Dense(_))));
    let dense_profile = dense_backend.resource_profile_v1(Some(&job2), PalwResourceRoleV1::Producer).expect("derives");
    assert!(matches!(dense_profile.capture, PalwCaptureRetentionV1::DenseTiles { tile_len: 4 }), "{:?}", dense_profile.capture);
    assert!(dense_profile.capture_retained_bytes >= dense_profile.leaves * (64 + 16 + 56));
}

/// **A partial seat replays its segment's prefix, and resumes at its start when it is served the
/// checkpoint there** (ADR-0133 S1's runtime half; ADR-0151 follow-up, item 4) — pinned against
/// the resource profile's `(resume_rows, end_rows)`, on both runtime profiles.
///
/// On a prefill-only job of the held class (the shape of the 2M attempt), cut into three:
/// * a genesis opening of segment `i` replays steps `1 ..= end_rows(i)` — its own end, not the
///   whole prefill — and its attested hashes are the capture's;
/// * an opening carrying the per-position checkpoint at segment 1's first position (built here
///   from a reference walk's cache, the way a cache-backed server would build it) replays steps
///   `resume_rows + 1 ..= end_rows` only, attests the same hashes, and holds `end_rows` rows —
///   the minimum state a dense segment has.
#[test]
fn a_partial_seat_replays_its_segments_prefix_and_resumes_at_its_start() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7;
    use kaspa_consensus_core::palw_resource_profile_v1::{PalwResourceRoleV1, PalwRuntimeProfileV1, palw_role_rows_v1};
    use kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2;
    let (artifact, _) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
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
    let profile = qwen25_a16_profile_v7(geometry).expect("the held row projects");
    assert!(map::palw_map_is_held_v4(&profile.state_chunk_map_id));
    let (prefill, seats) = (12u32, 4u16);
    let k = palw_segment_count_v2(seats);
    let backends: Vec<Qwen25A16Backend> = PalwRuntimeProfileV1::ALL
        .into_iter()
        .map(|runtime| {
            Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (prefill, 1))
                .expect("servable")
                .with_runtime_profile(runtime)
        })
        .collect();
    let producer = &backends[0];
    let (job, prompt) = producer.job_for_anchor(Hash64::from_u64_word(0x0133_0004)).expect("a job");
    // A held class's attempt FOLDS and a fold serves no S1 opening, so the openings this seat is
    // served come from a dense capture of the same job — what a server holding one publishes. The
    // producer's own (folded) roots are the dense capture's, which is the claim the seat attests.
    let folded = producer.execute(&job, &prompt).expect("the producer executes");
    let dense =
        a16_execute_for_attempt_capped_v1(&artifact, &profile, producer.plan.as_ref(), &job, &prompt, producer.step_ladder_cap())
            .expect("the dense capture of the same job");
    assert_eq!(dense.execution_root, folded.execution_root, "one execution, two retentions");
    let capture = crate::produce::base0_material_encode_v1(&dense).expect("encodes");
    let leaf_count = crate::produce::base0_material_decode_v1(&capture).expect("decodes").0.step_leaf_count;

    // (a) Every genesis segment stops at its own end. The seat's claim is the producer's roots.
    let claim_of = |segment_index: u16| kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1 {
        execution_root: folded.execution_root,
        trace_root: folded.trace_root,
        seat_count: seats,
        segment_index,
    };
    let mut genesis_replays = Vec::new();
    for segment in 0..k {
        let (resume, end) = palw_role_rows_v1(
            &profile,
            &job,
            leaf_count,
            PalwResourceRoleV1::PartialSeat { seat_count: seats, segment_index: segment },
        )
        .expect("the derivation names the segment");
        let opening = producer.open_segment_checkpoint_v1(&capture, seats, segment).expect("a dense retention serves its segments");
        let opened = crate::segment_opening::Base0SegmentOpeningV2::decode_v2(&opening).expect("decodes");
        assert!(opened.anchor.is_none(), "segment {segment}: a held class's retention carries no checkpoint state");
        for backend in &backends {
            let replay = backend.replay_segment_from_checkpoint_v1(&job, &prompt, &opening, claim_of(segment)).expect("replays");
            assert!(replay.matches, "segment {segment}: the seat's hashes are the capture's");
            assert_eq!(u64::from(replay.calls_replayed), end, "segment {segment}: a genesis replay runs steps 1..=end_rows ({end})");
            assert_eq!(resume, if segment == 0 { 0 } else { resume }, "the derivation's resume point is the segment's first position");
        }
        genesis_replays
            .push(backends[0].replay_segment_from_checkpoint_v1(&job, &prompt, &opening, claim_of(segment)).expect("replays"));
    }
    assert!(
        u64::from(genesis_replays[0].calls_replayed) < u64::from(prefill),
        "segment 0 no longer replays the whole prefill: {} of {prefill} steps",
        genesis_replays[0].calls_replayed
    );

    // (b) Segment 1, served the checkpoint at its first position: loads [0, a), replays [a, b).
    let segment = 1u16;
    let (a, b) =
        palw_role_rows_v1(&profile, &job, leaf_count, PalwResourceRoleV1::PartialSeat { seat_count: seats, segment_index: segment })
            .expect("derives");
    assert!(a > 0 && a < b, "segment 1 starts inside the prefill: [{a}, {b})");
    // The checkpoint's chunks at `a` rows, from a reference walk — what a server holding the
    // producer's cache re-derives (`base0_checkpoint_chunks_at_v1`).
    let engine = A16Engine::new(&artifact).expect("engine");
    let plan = engine.plan_from_profile(&profile).expect("plan");
    let mut reference = A16Cache::new(artifact.shape.n_layers);
    for (position, token) in prompt.iter().enumerate().take(a as usize) {
        engine.forward_token_planned(&plan, &mut reference, *token, position).expect("walks");
    }
    let chunks = crate::legs::base0_checkpoint_chunks_at_v1(&profile, &job, a as u32, |entry| reference.state_chunk_bytes_v1(entry))
        .expect("the checkpoint at a rows chunks");
    // The checkpoint the claim COMMITS at `a` rows — its leaf and its opening against the leg, read
    // off the producer's fold (the leg's leaves are what a held retention keeps) — with the state.
    let dense_retention = crate::produce::base0_material_decode_any_v1(&capture).expect("decodes");
    let crate::produce::Base0RetentionV1::Folded(fold) =
        crate::produce::base0_material_decode_any_v1(&folded.material).expect("decodes")
    else {
        panic!("a held attempt retains the fold")
    };
    let anchor_at = |covered: u32, chunks: Vec<Vec<u8>>| {
        let mut anchor =
            crate::fp_interval::base0_checkpoint_operands_v1(dense_retention.binding(), &[], &fold.checkpoint_leaves, covered)
                .expect("the leg commits a checkpoint at every position");
        anchor.chunks = chunks;
        anchor
    };
    let resuming = crate::segment_opening::base0_segment_opening_v2(
        &dense_retention,
        seats,
        segment,
        crate::segment_opening::Base0SegmentAnchorV1::Given(anchor_at(a as u32, chunks)),
        Some(&profile),
        producer.step_ladder_cap(),
    )
    .expect("the producer opens the segment at the committed checkpoint")
    .encode_v2()
    .expect("encodes");
    let whole = &genesis_replays[segment as usize];
    for backend in &backends {
        let replay = backend.replay_segment_from_checkpoint_v1(&job, &prompt, &resuming, claim_of(segment)).expect("resumes");
        assert!(replay.matches, "{}: the resumed segment attests the capture's hashes", backend.runtime_profile_v1().unwrap().name());
        assert_eq!(u64::from(replay.calls_replayed), b - a, "the seat re-executed exactly [a, b)");
        assert_eq!(
            replay.attested_hashes().collect::<Vec<_>>(),
            whole.attested_hashes().collect::<Vec<_>>(),
            "the same attested hashes as the walk from the prompt"
        );
        assert!(replay.leaf_hashes.len() < whole.leaf_hashes.len(), "and fewer leaves were recomputed to get them");
        assert!(replay.leaf_hashes[0].0 > whole.leaf_hashes[0].0, "nothing before the segment's start was re-executed");
    }
    // The held map's state authenticates the anchor (SEAT-S4): one flipped byte of the served
    // rows is not the committed checkpoint's state, and nothing is replayed from it.
    let mut forged = crate::segment_opening::Base0SegmentOpeningV2::decode_v2(&resuming).expect("decodes");
    forged.anchor.as_mut().expect("resumed").chunks[0][0] ^= 1;
    assert_eq!(
        backends[0].replay_segment_from_checkpoint_v1(&job, &prompt, &forged.encode_v2().unwrap(), claim_of(segment)).err(),
        Some(crate::segment_opening::Base0SegmentRefusalV1::AnchorNotCommitted.to_string()),
    );
    // A committed checkpoint past the segment's start — its own state, its own opening — is a
    // named refusal, not a silent empty window.
    for (position, token) in prompt.iter().enumerate().take(b as usize + 1).skip(a as usize) {
        engine.forward_token_planned(&plan, &mut reference, *token, position).expect("walks");
    }
    let past_chunks =
        crate::legs::base0_checkpoint_chunks_at_v1(&profile, &job, b as u32 + 1, |entry| reference.state_chunk_bytes_v1(entry))
            .expect("the checkpoint past the segment chunks");
    let mut opened = crate::segment_opening::Base0SegmentOpeningV2::decode_v2(&resuming).expect("decodes");
    opened.anchor = Some(anchor_at(b as u32 + 1, past_chunks));
    let past = opened.encode_v2().expect("encodes");
    let refused = backends[1].replay_segment_from_checkpoint_v1(&job, &prompt, &past, claim_of(segment)).expect_err("refused");
    assert_eq!(refused, crate::segment_opening::Base0SegmentRefusalV1::AnchorPastTheRange.to_string());
}

#[test]
fn the_corrected_a16_class_commits_the_root_the_derivation_recomputes() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let digest = artifact.artifact_digest();
    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
        .expect("the fixture's declaration is this engine's program");
    let prompt: Vec<usize> = vec![3, 9, 17, 33];
    let job = job(&profile, prompt.len() as u32, 3);

    let run = backend.execute_free_prompt(&job, &prompt).expect("the corrected class runs a caller's prompt");

    let class_facts = PalwFpClassFactsV3 {
        model_profile_id: digest,
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: digest,
        shape_profile_id: profile.shape_profile_id(),
        cu_ruleset_id: Hash64::default(),
    };
    let ctx = palw_fp_job_context_v3(&job, &class_facts, &run.facts, NETWORK).expect("the finished run implies a context");
    assert_eq!(
        palw_fp_execution_root_v3(&ctx, &run.facts),
        run.outcome.execution_root,
        "the derivation and the run must agree, or the court convicts the honest"
    );

    // All four legs measured — four zero roots would satisfy the equality above and mean
    // nothing — and the answer, which is the other half of the one inference.
    assert_ne!(run.facts.full_logits_trace_root, Hash64::default());
    assert_ne!(run.facts.step_leg_root, Hash64::default());
    assert_ne!(run.facts.checkpoint_leg_root, Hash64::default());
    assert_eq!(run.facts.stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
    assert_eq!(run.output_token_ids.len(), job.decode_token_limit as usize);

    // And it is a different class from the one testnet-11 carries, which is the cost.
    let (_, registered) = class_from(map::integer_kv_state_chunk_map_id_v1(), false);
    assert_ne!(profile.shape_profile_id(), registered.shape_profile_id());
}

/// **The v1 graph's undeclared requant: refused where the two authorities are, executed as
/// DECLARED where there is only one.**
///
/// This test used to assert one thing: that `::new` refused a v1-graph class by name. ADR-0049
/// Decision F requires a class's profile to name every narrowing its engine performs, and the
/// v1 pre table declares the embedding gather while `A16Engine::forward_token_traced` also
/// performs the requant that lifts it onto the A16 stream. Two authorities, and the probe was
/// what compared them.
///
/// Since ADR-0082 audit E's H-1 the constructor compiles `plan_from_profile`, so on that route
/// there is no second authority left to disagree with: the declaration IS the program, the
/// undeclared requant is simply not performed, and the v1 graph executes exactly what it says.
/// That is not a softened refusal — it is the refusal becoming unnecessary, and it is what
/// `from_registered_profile` has always done with the same profile, so the two constructors
/// now agree instead of one of them being the lenient one.
///
/// What is still pinned, and pinned here:
/// * the plan-LESS route — `a16_execute_for_attempt_v1(.., None, ..)`, where the two
///   authorities still exist — refuses by name, and names the missing requant;
/// * the v1 MAP is refused whichever route runs, because a one-byte map cannot describe an
///   `i32` cache (the sibling test below).
#[test]
fn a16_refuses_the_free_prompt_path_until_its_graph_is_reconciled() {
    for map_id in [map::integer_kv_state_chunk_map_id_v2(), map::integer_kv_state_chunk_map_id_v1()] {
        let (artifact, profile) = class(map_id);
        let prompt: Vec<usize> = vec![3, 9, 17, 33];

        // The route with two authorities still compares them, and still names the gap.
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
        let error = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt)
            .err()
            .expect("a graph that does not name what the traced engine computes must not commit a step leg");
        assert!(error.contains("registered graph"), "the refusal names the gap: {error}");
        assert!(error.contains("requant"), "and the node it is missing: {error}");

        // The route with ONE authority executes the declaration — the requant the profile does
        // not name is not performed, so there is nothing left to disagree about.
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the v1 declaration is a graph this build serves — a worse model, not a mis-executed one");
        let job = job(&profile, prompt.len() as u32, 3);
        match (map_id == map::integer_kv_state_chunk_map_id_v1(), backend.execute_free_prompt(&job, &prompt)) {
            (true, Ok(_)) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
            (true, Err(_)) => {}
            (false, Ok(run)) => assert!(run.facts.step_leg_root != Hash64::default(), "an executed declaration commits a leg"),
            (false, Err(e)) => panic!("the declaration this backend compiled is the one it must be able to run: {e}"),
        }
    }
}

/// **The executor prices the job against the RULESET's ladder, not a module literal.**
///
/// This is W1b's whole claim. `a16_execute_for_attempt_streaming_v1` derived its leaf count
/// with `step_leaf_count`, which hardcodes `PALW_STEP_MAX_LEAVES` and never reads
/// `PalwCourtParamsV2::max_step_leaf_count` — so the number of tokens a user got was a
/// build-time fact, a class registered against a deeper frozen ladder was refused by its own
/// executor, and every width ADR-0080's split close buys the court was worth nothing here.
///
/// Stated as a boundary rather than as a ratio because the boundary is what the ladder IS: one
/// leaf short of this job's own price the executor refuses it, and at exactly its price it
/// runs — same artifact, same graph, same job, one number of ruleset apart. (The size of the
/// move is pinned where the arithmetic lives:
/// `palw_step::tests::the_decode_budget_is_the_rulesets_ladder_not_the_context` measures a
/// 26-token prefill at 12 decode tokens under `2^22` and 11 120 under `2^32`.)
#[test]
fn the_executor_prices_the_job_against_the_rulesets_ladder() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let anchor = Hash64::from_u64_word(0xA16C0117);
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let (ctx, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
    let price = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, u64::MAX)
        .expect("the canonical job has a step space");
    assert!(price > 1, "a one-leaf job cannot demonstrate a boundary");

    // A ruleset one leaf short of this job's price refuses it — in the executor, and named.
    let tight = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(price - 1);
    assert_eq!(tight.step_ladder_cap(), price - 1);
    let err = match tight.execute(&ctx, &prompt) {
        Err(e) => e,
        Ok(_) => panic!("a job past the ruleset's ladder is not executable"),
    };
    assert!(err.contains("TooManyLeaves"), "the refusal must name the ladder, got: {err}");
    assert!(err.contains(&format!("{}", price - 1)), "and the ladder it was measured against, got: {err}");

    // The same job, the same geometry, one more leaf of ladder — and it runs, committing the
    // price the ruleset admitted.
    let exact = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(price);
    let Ok(outcome) = exact.execute(&ctx, &prompt) else {
        panic!("the ruleset that admits this job's price must execute it");
    };
    let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
    assert_eq!(binding.step_leaf_count, price, "the binding commits the price the ruleset admitted");
}

/// **The A16 step space, end to end: every leaf of a captured attempt adjudicates, and a
/// tampered one convicts** — the theorem this family's court capability rests on.
///
/// One captured run of the corrected class; then, for EVERY leaf of its step space, the
/// backend's own prover assembles the refutation, the backend's own inventory answers for the
/// operands through real Merkle openings against its root, and the court finds no fault. A
/// single leaf that reads `Unadjudicable` is a step nobody can police — the
/// coverage-clean-but-unprosecutable shape ADR-0049 exists to refuse — so the sweep is
/// exhaustive rather than sampled. The same prover then convicts a run with one tampered lane
/// at representative kernels, including a decode call (the tiled pin) and an anchored
/// attention step (the v2-map checkpoint geometry).
#[test]
fn every_a16_leaf_adjudicates_and_a_tampered_one_convicts() {
    use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    assert!(backend.supports_court(), "the corrected class takes a court's turn");

    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xA16C0117)).expect("the anchor implies a job");
    let outcome = backend.execute(&job, &prompt).expect("the corrected class runs the attempt lane");
    let (binding, _tiles, _logits, _generated, _chunks) =
        crate::produce::base0_material_decode_v1(&outcome.material).expect("the captured material decodes");
    assert_eq!(outcome.execution_root, binding.committed_execution_root, "the claim commits the binding's own root");

    // The seat's half, against this very claim.
    let claim = PalwClaimRootsV1 {
        execution_root: outcome.execution_root,
        trace_root: outcome.trace_root,
        anchor: Hash64::from_u64_word(0xA16C0117),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    assert_eq!(backend.verify_material(&outcome.material, claim), kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches);

    // One proven oracle over the whole inventory — the production path a close takes.
    let inventory = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the corrected class yields an inventory");
    let openings: Vec<_> = (0..inventory.operands().len())
        .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
        .collect();
    let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
        .expect("every inventory row proves against its own root");

    // The sweep: every leaf of the step space clears the honest capture.
    let mut anchored_seen = 0u32;
    for index in 0..binding.step_leaf_count {
        let refutation = backend
            .refutation_for_index(&outcome.material, index)
            .unwrap_or_else(|e| panic!("leaf {index} must open from an honest capture: {e}"));
        anchored_seen += u32::from(refutation.kv_checkpoint.is_some());
        let got = check_execution_step_refutation_v1(&refutation, &oracle);
        let named = profile
            .resolve_node_slot(refutation.output_preimage.coord.node_slot)
            .map(|(n, l)| format!("{} (layer {l:?})", n.weight_name))
            .unwrap_or_default();
        assert!(
            matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
            "an honest execution must clear itself at leaf {index} (coord {:?}, node {named}): got {got:?}",
            refutation.output_preimage.coord
        );
    }
    assert!(anchored_seen > 0, "the sweep exercised the checkpoint-anchored attention path (v2 map geometry)");

    // The other direction: one tampered lane convicts, at kernels of different shapes —
    // the embedding gather, a matmul tile, and a decode-call leaf (whose adjudication rides
    // the tiled decode-token pin).
    let coords = [0u64, binding.step_leaf_count / 3, binding.step_leaf_count - 1];
    for &index in &coords {
        let lying = backend.execute_with_injected_fault(&job, &prompt, index).expect("a tampered capture still commits");
        let refutation = backend.refutation_for_index(&lying.material, index).expect("a tampered capture opens too");
        let openings = backend.operand_openings_for(&refutation).expect("the prover opens what the court resolves");
        let proven = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
            .expect("recorded openings prove");
        assert!(
            check_execution_step_refutation_v1(&refutation, &proven).is_ok(),
            "a tampered lane at leaf {index} must convict, not read as no fault"
        );
    }
}

/// **Two authorities, one commitment.** The ledger-compiled backend and the chain-registered
/// one (whose plan is compiled FROM the profile) must capture the same job to the same roots
/// and the same material — otherwise the same class's claims would be prosecutable or not
/// depending on WHICH node produced them, and "the class is adjudicable" would be a property
/// of a deployment rather than of the class.
#[test]
fn both_authorities_capture_one_job_to_one_commitment() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let compiled = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let registered =
        Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile, (4, 3)).expect("the corrected profile plans");
    assert!(compiled.supports_court() && registered.supports_court());

    let anchor = Hash64::from_u64_word(0x2AA16);
    let (job, prompt) = compiled.job_for_anchor(anchor).expect("a job");
    let a = compiled.execute(&job, &prompt).expect("the compiled walk runs");
    let b = registered.execute(&job, &prompt).expect("the planned walk runs");
    assert_eq!(a.execution_root, b.execution_root, "one binding, whichever authority walked");
    assert_eq!(a.trace_root, b.trace_root);
    assert_eq!(a.material, b.material, "and one retained material, byte for byte");
}

/// **Under the map the class declares TODAY, the same run refuses.**
///
/// `integer_kv_state_chunk_map_id_v1` describes one byte per element and this cache holds
/// `i32`. The refusal is the whole point: the alternative implementation truncates, passes
/// every downstream check, and commits a checkpoint that opens to a state the producer never
/// had.
#[test]
fn a16_refuses_to_commit_under_a_map_that_cannot_describe_its_cache() {
    let (artifact, profile) = class(map::integer_kv_state_chunk_map_id_v1());
    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
        .expect("the fixture's declaration is this engine's program");
    let prompt: Vec<usize> = vec![3, 9, 17, 33];
    let job = job(&profile, prompt.len() as u32, 3);

    let error = match backend.execute_free_prompt(&job, &prompt) {
        Err(e) => e,
        Ok(_) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
    };
    // **Which gate fires has moved, and the message with it.** The Decision-F probe used to
    // refuse this class before the map was ever consulted; a planned backend executes its
    // declaration, so the MAP is now what stops it — at the checkpoint it cannot serialise,
    // rather than at the class's door. The refusal is still a refusal, and nothing is
    // committed. That it is `CheckpointStateUnavailable` and not a NAMED map refusal is
    // recorded here rather than papered over: the predicate `a16_court_capable_v1` already
    // spells "can this map describe this cache", and nothing calls it as a door.
    assert!(
        error.contains("CheckpointStateUnavailable") || error.contains("state map") || error.contains("registered graph"),
        "the error names the defect it hit first: {error}"
    );
}

/// **The registered row, through the constructor a dense-tier demonstration actually uses.**
///
/// Every other test in this module builds its class with `rms_eps_q: 1` in BOTH halves and so
/// agrees with itself. The shipped ledger does not: `misaka-palw-base0::classes` gives this
/// family a profile from `QWEN25_1_5B` (`rms_eps_q: 1`) and an `artifact_shape` at the
/// converter's `eps_q: 1 << 8` — "the A16 engine norms at the shipped 1 << 8" — and
/// `every_canonical_class_agrees_with_its_own_profile` EXEMPTS `ConvertedA16` from the
/// equality that would have caught it. So a class whose artifact executes what
/// `qwen25-convert` writes is refused by its own declaration:
/// `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }`.
///
/// It went unseen because the shipped worker took [`Qwen25A16Backend::new`], which compiled no
/// plan and let the artifact's epsilon execute, while the ADR-0080 ladder row, the SDK and any
/// chain-registered class go through [`Qwen25A16Backend::from_registered_profile`]. That
/// asymmetry is closed — since ADR-0082 audit E's H-1 `::new` compiles the same plan and
/// refuses the same mismatch — and this test keeps driving the registered constructor over an
/// artifact built at the converter's epsilon, which is where the split was first made visible.
///
/// Both directions are pinned: the corrected projection PLANS, and a declaration carrying the
/// frozen `rms_eps_q: 1` still refuses on exactly that field and no other — the dense twin of
/// `qwen36_plan`'s "a rms_eps_q 17 declaration refuses against the artifact's 1".
#[test]
fn a_row_served_over_a_converter_built_artifact_plans_and_the_frozen_declaration_refuses() {
    use crate::engine_a16::A16PlanErrorV1;
    use kaspa_consensus_core::palw_qwen25_profile::{
        QWEN25_A16_ARTIFACT_EPS_Q, qwen25_a16_artifact_row_profile_v1, qwen25_geometry_artifact_eps,
    };

    // The ledger's own pairing, at a size a unit test can hold: the profile from the geometry,
    // the artifact at the epsilon `qwen25-convert` writes.
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
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        // What the converter writes. `classes.rs` spells this same constant for every
        // `ConvertedA16` row's `artifact_shape`.
        eps_q: QWEN25_A16_ARTIFACT_EPS_Q,
    };
    assert_eq!(shape.eps_q, 1 << 8, "the artifact half of the shipped pairing");
    let artifact = std::sync::Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );

    // The frozen declaration: refused, on rms_eps_q and nothing else.
    let frozen = qwen25_a16_profile_v2(geometry).expect("the frozen projection is a valid profile");
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("the artifact is an A16 class");
    match engine.plan_from_profile(&frozen) {
        Err(A16PlanErrorV1::GeometryMismatch { what: "rms_eps_q", profile, artifact: got }) => {
            assert_eq!((profile, got), (1, 1 << 8), "the two epsilons, named");
        }
        other => panic!("a declaration the artifact does not execute must refuse on rms_eps_q and nothing else, got {other:?}"),
    }
    Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), frozen, (4, 3))
        .map(drop)
        .expect_err("and the backend a demonstration uses must refuse it too");

    // The corrected row: plans, and runs.
    let served = qwen25_a16_artifact_row_profile_v1(geometry).expect("the corrected projection is a valid profile");
    assert_eq!(
        qwen25_geometry_artifact_eps(geometry).rms_eps_q,
        QWEN25_A16_ARTIFACT_EPS_Q,
        "the correction is the epsilon and nothing else"
    );
    // The constructor FIRST, so the failure this pins is the refusal itself and not a
    // comparison standing in for it.
    let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), served.clone(), (4, 3))
        .expect("the row the artifact executes must be servable through the registered-profile constructor");
    assert_eq!(served.base0_rms_eps_q, shape.eps_q, "declared is executed");
    assert!(backend.supports_court(), "the corrected row still takes a court's turn");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xE95)).expect("the anchor implies a job");
    let outcome = backend.execute(&job, &prompt).expect("and the planned walk runs over the artifact's own epsilon");
    assert_eq!(
        backend.verify_material(
            &outcome.material,
            PalwClaimRootsV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                anchor: Hash64::from_u64_word(0xE95),
                attempt_draw: None,
                output_root: None,
                job_pin: None,
            }
        ),
        kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches
    );
}

/// **The fold and the dense capture are ONE commitment** (ADR-0082 Decision 7).
///
/// This is the test that makes Decision 7 safe to ship: the free-prompt lane stopped keeping
/// its tiles, and the only thing that must not move is what the job COMMITS. Both sinks run
/// the same loop over the same rows, and every root the chain sees — execution, trace, output,
/// manifest, and the step leg's own root and leaf count inside the binding — is compared.
///
/// Root equality is also what pins the fold's ENUMERATION. The step tree's leaves are
/// `step_merkle_leaf_v1(index, hash)`, so a leaf placed at a different index is a different
/// tree: the cursor the fold walks (call-major, position-major, slot-major, tile-major) agrees
/// with `canonical_step_leaf_index` for every leaf of this job, or this assertion fails.
#[test]
fn the_folded_capture_commits_the_dense_captures_roots() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xF01D)).expect("the anchor implies a job");
    let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;

    let dense = a16_execute_for_attempt_streaming_capped_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
        .expect("the dense sink runs the job");
    let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
        .expect("the folded sink runs the job");

    assert_eq!(dense.binding, folded.binding, "the two sinks commit the same binding, field for field");
    assert_eq!(dense.execution_root, folded.execution_root);
    assert_eq!(dense.trace_root, folded.trace_root);
    assert_eq!(dense.output_root, folded.output_root);
    assert_eq!(dense.trace_manifest_root, folded.trace_manifest_root);
    assert_eq!(dense.generated_token_ids, folded.generated_token_ids, "one execution, one answer");
    assert_eq!(dense.checkpoints.merkle_root, folded.checkpoints.merkle_root);

    // And the retention is the thing that changed: no tiles, one node per 2^retain_level leaves.
    let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
    assert!(folded.tiles.tiles.is_empty() && folded.tiles.leaves.is_empty(), "the fold keeps no tiles");
    assert_eq!(tree.leaf_count(), dense.tiles.leaves.len() as u64);
    assert_eq!(tree.root().expect("the tree is its own shape"), dense.binding.step_merkle_root);
    let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
    assert_eq!(tree.retain_level(), level, "the level is the ruleset ladder's derivation, not a constant");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let folded_bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains").len();
    let dense_bytes = crate::produce::base0_material_encode_v1(&dense).expect("the dense sink retains").len();
    assert!(folded_bytes < dense_bytes, "the fold must retain less than the tiles it replaced ({folded_bytes} against {dense_bytes})");
    eprintln!(
        "Decision 7 on the A16 fixture: {} leaves, retention {folded_bytes} bytes folded against {dense_bytes} dense ({:.1}x)",
        tree.leaf_count(),
        dense_bytes as f64 / folded_bytes.max(1) as f64
    );
}

/// **An opening re-derived by replay IS the opening the dense capture would have served** —
/// byte for byte, at every interval index of a multi-interval job (ADR-0082 Z6's second half).
///
/// The executor kept no tiles, so the span's leaf hashes come from running the interval again
/// from its checkpoint. If that replay were a different execution — a different seed id, a
/// different resume point, a different order — the bytes would differ here, and a seat would
/// have read the difference as this producer's fault.
/// **The verdict replay is the producer's run, minus the tiles** (ADR-0084 Decision 7): the
/// fold sink reproduces the dense run's execution root, trace root and priced work for the
/// job an anchor implies — what a seat that holds no material compares a claim against.
#[test]
fn the_verdict_replay_reproduces_the_dense_runs_roots_and_work() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0D7)).expect("the anchor implies a job");
    let dense = backend.execute(&ctx, &prompt).expect("the producer's run");
    let (binding, ..) = crate::produce::base0_material_decode_v1(&dense.material).expect("the dense material decodes");
    let replay = backend.execute_for_verdict(&ctx, &prompt).expect("the seat's replay");
    assert_eq!(replay.execution_root, dense.execution_root, "the fold's execution root is the dense run's");
    assert_eq!(replay.trace_root, dense.trace_root, "and its trace root");
    assert_eq!(replay.work_leaves, Some(binding.step_leaf_count), "and it prices the same work");
}

#[test]
fn a_replayed_interval_opening_is_the_dense_ones() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0BEE)).expect("the anchor implies a job");
    let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();

    let dense = a16_execute_for_attempt_streaming_capped_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
        .expect("the dense sink runs the job");
    let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
        .expect("the folded sink runs the job");
    let dense_material: crate::produce::Base0RetainedMaterialV1 = (
        dense.binding.clone(),
        dense.tiles.tiles.clone(),
        dense.logits_rows.clone(),
        dense.generated_token_ids.clone(),
        dense.checkpoints.chunks.clone(),
    );
    let folded_bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains");
    let folded_material = crate::produce::base0_fp_material_decode_v2(&folded_bytes).expect("its own retention decodes");

    let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let count = crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(&dense.binding, interval)
        .expect("the capture has an interval geometry")
        .interval_count;
    assert!(count >= 2, "a one-interval job cannot show that a replayed anchor is the committed one");
    let claim = PalwClaimRootsV1 {
        execution_root: dense.execution_root,
        trace_root: dense.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let geometry = crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(&dense.binding, interval).expect("a geometry");
    for index in 0..count {
        // ADR-0086 Decision 2: the anchor is named; the seat holds the state it recomputed.
        backend.fp_forget_seat_state_v1();
        if let Some(covered) = geometry.anchor_covered_call(index) {
            let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, None).expect("recompute kernels");
            crate::fp_recompute::base0_fp_seat_state_memoized_v1(
                &backend.seat_memo,
                &profile,
                &ctx,
                &ids,
                &dense.generated_token_ids,
                covered,
                &mut kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("this seat can recompute its own state");
        }
        let from_tiles = crate::fp_interval::base0_open_fp_interval_v1(
            &dense_material,
            index,
            &ids,
            interval,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .unwrap_or_else(|e| panic!("interval {index} opens from the tiles: {e}"));
        let from_replay = crate::fp_interval::base0_open_fp_interval_sparse_v1(
            &folded_material,
            index,
            &ids,
            interval,
            &A16IntervalKernels {
                artifact: &artifact,
                plan: None,
                fault: None,
                storage: crate::engine_a16::KV_STORAGE_SHIPPED_V1,
                run_positions: A16_PREFILL_RUN_POSITIONS,
            },
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .unwrap_or_else(|e| panic!("interval {index} opens from the fold: {e}"));
        assert_eq!(from_replay, from_tiles, "interval {index}: the replayed opening is not the retained one");
        assert_eq!(
            backend.verify_fp_interval_opening(&from_replay, claim, index, &ids, dense.binding.step_leaf_count),
            kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1::Valid,
            "interval {index}: a seat must license the replayed opening"
        );
    }
}

/// **Z6: what the fold retains is a fixed fraction of the leaf count — of nothing else.**
///
/// Two jobs of different length on one class: the retained vector is `⌈leaves / 2^level⌉`
/// nodes in both, so the retention per leaf is the same number whatever the context, and the
/// only thing that moves it is the ladder the level is derived from. The dense capture's own
/// retention is printed beside it, because the ratio is what ADR-0082 §1.5 is about.
///
/// What this does NOT claim: that the whole retained MATERIAL is flat per position. It is not,
/// and the reason is named — the checkpoint chunks hold the cache at every checkpoint
/// (Decision 4's prefix-stability is not implemented here), and on a graph-v3 class the leaves
/// per position themselves grow with `kv_len` (Decision 1). Both are other units' work, and a
/// test that asserted flatness today would be asserting something this tree does not do.
#[test]
fn the_folds_retention_is_a_fixed_fraction_of_the_leaf_count() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
    let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
    let mut seen = Vec::new();
    for (prefill, decode) in [(4u32, 3u32), (9, 6)] {
        let ctx = {
            let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
            ctx.job_id = Hash64::from_u64_word(0x2626 + prefill as u64);
            let prompt: Vec<u32> = (0..prefill).map(|i| i % profile.vocab_size).collect();
            ctx.prompt_token_ids_hash = kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt);
            ctx
        };
        let prompt: Vec<usize> = (0..prefill as usize).map(|i| i % profile.vocab_size as usize).collect();
        let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
            .expect("the folded sink runs the job");
        let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
        let leaves = tree.leaf_count();
        let nodes = tree.retained_nodes().len() as u64;
        assert_eq!(nodes, leaves.div_ceil(1 << level), "the retained vector is the leaf count over the block, and nothing else");
        seen.push((leaves, nodes));
    }
    assert!(seen[0].0 != seen[1].0, "two jobs of the same length prove nothing about the context");
    // 64 bytes a node, and the budget the level was derived from bounds it — on both jobs.
    for (leaves, nodes) in seen {
        assert!(
            nodes * 64 <= crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES,
            "{leaves} leaves retained {} bytes, past the budget the level derives from",
            nodes * 64
        );
    }
}

// -- W1b: the EXECUTOR's own ladder -------------------------------------------------------

/// **The decode budget a fixed prefill buys is the RULESET's number, and it was a constant.**
///
/// `a16_execute_for_attempt_streaming_v1` priced every job through `step_leaf_count`, which
/// hardcodes `PALW_STEP_MAX_LEAVES = 2^22`. So every width the court gained at ADR-0080's
/// close ceiling — dense A16 admitted out to `n_ctx` 39 against the shipped ladder and 1002
/// against `2^32` — bought a user nothing: the executor refused the job, or truncated its
/// budget, before the first forward pass.
///
/// Measured here on the SHIPPED dense geometry (`QWEN25_1_5B`, `tile_len` 128) through the
/// very call the capture path makes. The two prefills are the ones the launch brief measured
/// on the real artifact, and the leaf counts are why: 26 prefill tokens leave room for 12
/// decode tokens and 4,074,040 of the 4,194,304 leaves. **38 total positions is what the
/// shipped committed path admits — a prompt, and a sentence of answer.**
///
/// At `2^32` the same prefill buys 4,070, and the number that bounds it is no longer the
/// ladder at all: it is the declared context (4,096 − 26). That is the whole point of the
/// thread — the ladder stops being what decides what a user gets.
#[test]
fn the_decode_budget_a_fixed_prefill_buys_moves_with_the_ruleset() {
    use kaspa_consensus_core::palw_step::step_leaf_count_capped_v1;

    let geometry = PalwQwen25GeometryV1 { n_ctx: 4_096, ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B };
    let profile = qwen25_a16_profile_v2(geometry).expect("the dense class's registered graph");

    // The widest decode budget a ladder admits at this prefill: the job's own leaf count,
    // counted the way the executor counts it, walked up until it refuses.
    let widest = |prefill: u32, cap: u64| -> (u32, u64) {
        let mut best = (0u32, 0u64);
        for decode in 1..=(geometry.n_ctx - prefill) {
            let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
            match step_leaf_count_capped_v1(&profile, &ctx, cap) {
                Ok(leaves) => best = (decode, leaves),
                Err(_) => break,
            }
        }
        best
    };

    const SHIPPED: u64 = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
    const DEEP: u64 = 1 << 32;
    assert_eq!(SHIPPED, 4_194_304, "the shipped ladder moved without a fence");

    assert_eq!(widest(26, SHIPPED), (12, 4_074_040), "the shipped ladder's budget at prefill 26");
    assert_eq!(widest(24, SHIPPED), (14, 4_112_072), "the shipped ladder's budget at prefill 24");

    let (deep_26, _) = widest(26, DEEP);
    let (deep_24, _) = widest(24, DEEP);
    assert_eq!((deep_26, deep_24), (4_070, 4_072), "at 2^32 the DECLARED CONTEXT binds, not the ladder");
    assert!(deep_26 > 300 * 12, "the budget must move by orders of magnitude, not by rounding: {deep_26}");
}

/// **The executor reads the ladder it is HANDED — the whole of W1b in one assertion.**
///
/// The capped entry point is given one leaf less than the job needs and must refuse; given
/// exactly what the job needs it must run. Before this change the argument did not exist and
/// the module constant decided both answers, so the first half of this test passed a job the
/// caller's ruleset had no room for.
///
/// It is stated as `TooManyLeaves` on the way in rather than a failure three layers down: the
/// leaf count is the first line of the capture, and the 268 MB the leaf vector costs at the
/// shipped ladder is exactly what must not be allocated for a job that cannot be committed.
#[test]
fn the_a16_executor_prices_its_capture_at_the_ladder_the_caller_states() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let prompt: Vec<usize> = vec![3, 9, 17, 33];
    let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
    let needed = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");
    assert!(needed > 1, "a one-leaf job could not tell the two ladders apart");

    // A ladder one leaf short of this job. **Refused, and named.**
    let refused = a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &ctx, &prompt, needed - 1);
    let message = refused.err().expect("a job the caller's ladder has no room for must be refused");
    assert!(message.contains("TooManyLeaves"), "the refusal must name the ladder, not something downstream: {message}");

    // The same job, at the ladder it actually needs. **Runs, and commits.**
    let run = a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &ctx, &prompt, needed)
        .expect("the same job runs under a ladder that admits it");
    assert_eq!(run.binding.step_leaf_count, needed);
    assert_ne!(run.execution_root, Hash64::default());

    // **And the budget the executor ACTUALLY grants is the arithmetic one, at two different
    // ladders.** This is the half `the_decode_budget_a_fixed_prefill_buys_moves_with_the_ruleset`
    // computes and cannot run: there, the class is the real 1.5B geometry and a forward pass
    // needs the artifact; here the class is small enough to execute, so the two ladders can be
    // walked end to end. A budget that came from a constant would grant the same number twice.
    let prefill = prompt.len() as u32;
    let arithmetic_budget = |cap: u64| -> u32 {
        let mut best = 0;
        for decode in 1..=8u32 {
            let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
            match kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, cap) {
                Ok(_) => best = decode,
                Err(_) => break,
            }
        }
        best
    };
    let leaves_for = |decode: u32| -> u64 {
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
        kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space")
    };
    let (shallow, deep) = (leaves_for(2), leaves_for(5));
    assert!(shallow < deep, "two ladders that admit the same budget prove nothing");
    assert_eq!((arithmetic_budget(shallow), arithmetic_budget(deep)), (2, 5));

    for (cap, budget) in [(shallow, 2u32), (deep, 5u32)] {
        let admitted = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, budget);
        assert!(
            a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &admitted, &prompt, cap).is_ok(),
            "the executor must grant the whole budget its ladder pays for ({budget} decode tokens at cap {cap})"
        );
        let one_more = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, budget + 1);
        assert!(
            a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &one_more, &prompt, cap).is_err(),
            "the executor must stop exactly where its ladder does ({} decode tokens at cap {cap})",
            budget + 1
        );
    }
}

/// **The dormant default is byte-identical, proven by measurement rather than by argument.**
///
/// Every shipped construction site passes the leg's own constant, so nothing on any live
/// network moves: the uncapped names delegate with `PALW_STEP_LEG_MAX_LEAVES` and must return
/// the same commitment, leaf for leaf and root for root, as the capped name given that value.
/// The whole retained material is compared, not just the execution root — a capture that
/// agreed on the root and differed in a tile would be a producer that cannot answer a
/// challenge it has already been convicted by.
#[test]
fn the_uncapped_executor_is_the_capped_one_at_the_shipped_ladder() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let prompt: Vec<usize> = vec![3, 9, 17, 33];
    let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);

    let plain = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt).expect("the default path runs");
    let capped = a16_execute_for_attempt_capped_v1(
        &artifact,
        &profile,
        None,
        &ctx,
        &prompt,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
    .expect("the capped path at the default runs");

    assert_eq!(plain.execution_root, capped.execution_root);
    assert_eq!(plain.trace_root, capped.trace_root);
    assert_eq!(plain.output_root, capped.output_root);
    assert_eq!(plain.trace_manifest_root, capped.trace_manifest_root);
    assert_eq!(
        crate::produce::base0_material_encode_v1(&plain).expect("encodes"),
        crate::produce::base0_material_encode_v1(&capped).expect("encodes"),
        "the retained material must be the same bytes, or the default was not dormant"
    );

    // And the backend's own default is that same constant, so an instance nobody told about a
    // ruleset behaves exactly as it did.
    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, (4, 2))
        .expect("the fixture's declaration is this engine's program");
    assert_eq!(backend.step_ladder_cap(), kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES);
}

/// **An instance TOLD a shallower ladder refuses the job its default would have run** — the
/// field is wired to the executor, not merely stored beside it.
///
/// `with_step_ladder_cap` is the seam a caller holding `PalwCourtParamsV2` uses. It already
/// bounded the served-capture guards (`bisect_prefix_state`, `refutation_with_prompt`); it did
/// not reach the run, which is the half that decides what gets produced in the first place.
#[test]
fn an_instance_told_a_ladder_produces_against_it() {
    let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let prompt: Vec<usize> = vec![3, 9, 17, 33];
    let fp = job(&profile, prompt.len() as u32, 3);
    let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
    let needed = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");

    let default = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 2))
        .expect("the fixture's declaration is this engine's program");
    assert!(default.execute_free_prompt(&fp, &prompt).is_ok(), "the shipped ladder admits this job");

    let shallow = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, (4, 2))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(needed - 1);
    let message = shallow.execute_free_prompt(&fp, &prompt).err().expect("a shallower ruleset must refuse it");
    assert!(message.contains("TooManyLeaves"), "the refusal must name the ladder: {message}");
}

/// **DoS audit 2026-09-24, #4: the materialization cap is the host's, not the held ladder.**
/// Testnet-12 mints its network ladder at the held regime's `2^40`, and every backend there is
/// built at it. The held row walks at `2^40` as before; it no longer MATERIALIZES there — the cap
/// is the historical `2^26` — so a capture between the two is refused by the whole-capture
/// prover by name and goes to the streamed routes. Before the fix `materialize_cap()` was the
/// network's ladder, `2^40`, and the refusal below was `None` for every count the class admits.
#[test]
fn at_t12s_ladder_a_held_class_materializes_at_the_hosts_cap() {
    use crate::fp_interval::base0_whole_capture_refusal_v1 as refusal;
    use kaspa_consensus_core::palw_resource_profile_v1::PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1;
    use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let held = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry).expect("v7");
    let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), held, (20, 4))
        .expect("servable")
        .with_step_ladder_cap(PALW_HELD_STEP_LADDER_V1);
    assert_eq!(backend.step_ladder_cap(), PALW_HELD_STEP_LADDER_V1, "the class still walks at the regime's ladder");
    assert_eq!(backend.materialize_cap(), PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, "and materializes at 2^26, not 2^40");
    // The t12 held canonical jobs (`dos_repro_0`): 105,518,224 and 27,002,967,184 leaves.
    for leaves in [PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1 + 1, 105_518_224, 27_002_967_184, PALW_HELD_STEP_LADDER_V1] {
        let why = refusal(leaves, backend.materialize_cap(), backend.step_ladder_cap()).expect("refused by name");
        assert!(why.contains("materialization cap") && why.contains("streamed routes"), "{why}");
    }
    assert_eq!(refusal(PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, backend.materialize_cap(), backend.step_ladder_cap()), None);
}

/// **DoS audit 2026-09-24, #4, the CPU half: a sampled capture is re-executed ONCE, not once per
/// draw.** On the held fixture's folded attempt, the free-prompt leaf prover answers every leaf
/// the per-index verb answers, byte for byte, after exactly one dense re-execution of the fold;
/// the per-index verb pays one per leaf. Without the family's override the trait default IS the
/// per-index verb, and the count below reads `indices.len()`, not 1.
#[test]
fn a_sampled_fold_is_re_executed_once_whatever_the_draw_count() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7;
    let (artifact, _) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
    let held = qwen25_a16_profile_v7(v5_geometry()).expect("the held row projects");
    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), held, (6, 3)).expect("servable");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0924_0004)).expect("a job");
    let folded = backend.execute(&job, &prompt).expect("the held attempt folds");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let leaves = crate::produce::base0_material_decode_any_v1(&folded.material).expect("decodes").binding().step_leaf_count;
    let indices = [0u64, 1, leaves / 3, leaves / 2 + 1, leaves - 1];

    FOLD_REEXECUTIONS_V1.with(|n| n.set(0));
    let per_index: Vec<_> = indices
        .iter()
        .map(|i| backend.refutation_for_free_prompt_index(&folded.material, *i, &ids).map_err(|e| e.to_string()))
        .collect();
    assert_eq!(FOLD_REEXECUTIONS_V1.with(|n| n.get()), indices.len() as u64, "the per-index verb re-executes per leaf");

    FOLD_REEXECUTIONS_V1.with(|n| n.set(0));
    let prover = backend.free_prompt_leaf_prover_v1(&folded.material, &ids).expect("prepares");
    let prepared: Vec<_> = indices.iter().map(|i| prover.refutation_for_index(*i)).collect();
    drop(prover);
    assert_eq!(FOLD_REEXECUTIONS_V1.with(|n| n.get()), 1, "the prover re-executes the fold once for every leaf it opens");
    assert!(per_index.iter().any(|r| r.is_ok()), "the fixture opens some leaf");
    assert_eq!(prepared, per_index, "the same answer at every leaf, refusals included");

    // A prompt that is not the binding's is refused before the re-execution, not after it.
    FOLD_REEXECUTIONS_V1.with(|n| n.set(0));
    let foreign: Vec<u32> = ids.iter().map(|t| (t + 1) % 64).collect();
    assert!(backend.free_prompt_leaf_prover_v1(&folded.material, &foreign).is_err());
    assert_eq!(FOLD_REEXECUTIONS_V1.with(|n| n.get()), 0, "a foreign prompt costs no forward pass");
}

/// The geometry the ADR-0082 dense rows are built from here, at a size a unit test can hold.
/// One geometry for both rows, so the v2 and v5 profiles differ ONLY where the graph does.
fn v5_geometry() -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 {
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
    }
}

/// The artifact those rows are SERVED over: built at the epsilon `qwen25-convert` writes,
/// which is the pairing `qwen25_a16_artifact_row_profile_v*` exists to project.
fn v5_artifact(geometry: PalwQwen25GeometryV1) -> std::sync::Arc<Base0ArtifactV1> {
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q,
    };
    std::sync::Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    )
}

/// **ADR-0093 as built, on a REAL fused capture** (§5 invariants 2–4, at the phase level).
///
/// The dense tier's graph-v5 row executed for real; the fused leaves of its last position —
/// both layers, both heads, a history of two court tiles — answered from the backend's own
/// evidence:
///
/// * the root the evidence computes finalizes to the COMMITTED output tile, so the phase opens;
/// * at EVERY tile, every round folds, the challenger's recompute names nothing, and the
///   bottom built out of the anchor recomputes the claim exactly — `ChallengerDefeated`. This is
///   the drill vector ADR-0093 §7 asked for: the instrumentation question ("is it exact?")
///   answered on real committed rows, not on a fixture's hand-built series;
/// * forged — one lane of a fused output tile moved, the commitment re-derived — the honest
///   root no longer finalizes (the forger must lie), the least lie that does is found, the
///   challenger's evidence names the child it hides in, and the bottom, opened out of the
///   ACCUSED capture's own commitments, convicts.
#[test]
fn a_real_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted() {
    let geometry = v5_geometry();
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry)
        .expect("the v5 projection is a valid profile");
    real_fused_capture_answers_and_convicts(geometry, profile);
}

/// **ADR-0118 Decision 3: a backend keeps its CLASS's prompt-ids form.** Handed a network's flat
/// form — testnet-11's — the held row (graph-v7) keeps the tiled Merkle root its seats and
/// courts read, and derives its jobs under it; the shipped row (graph-v5) keeps the flat digest,
/// byte for byte as before.
#[test]
fn a_backend_keeps_its_classs_prompt_ids_form_whatever_the_networks() {
    use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    for (profile, form) in [
        (
            kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry).expect("v7"),
            PalwPromptIdsFormV1::MerkleV1,
        ),
        (
            kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry).expect("v5"),
            PalwPromptIdsFormV1::Flat,
        ),
    ] {
        let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile, (20, 4))
            .expect("servable")
            .with_prompt_ids_form(PalwPromptIdsFormV1::Flat);
        assert_eq!(backend.prompt_ids_form(), form);
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0118)).expect("the anchor implies a job");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        assert_eq!(job.prompt_token_ids_hash, prompt_token_ids_commitment_v1(form, &ids).expect("a commitment"), "{form:?}");
    }
}

/// **ADR-0121 Decision 1: two knobs where there was one.** Built at a network's ladder —
/// testnet-11's `2^26` — the held row (graph-v7) prices and walks at the regime's `2^40` and
/// materializes at the network's; the shipped row (graph-v5) has the network's for both. A
/// capture between the two is refused by the whole-capture prover by name, before a leaf vector
/// is built from its count: the count arrived in a blob, and `2^40` hashes would be 64 TiB.
#[test]
fn a_held_class_walks_at_its_ladder_and_materializes_at_the_networks() {
    use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let network = 1u64 << 26;
    for (profile, walked) in [
        (
            kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry).expect("v7"),
            PALW_HELD_STEP_LADDER_V1,
        ),
        (kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry).expect("v5"), network),
    ] {
        let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile, (20, 4))
            .expect("servable")
            .with_step_ladder_cap(network);
        assert_eq!(backend.step_ladder_cap(), walked);
        assert_eq!(backend.materialize_cap(), network);
    }
    use crate::fp_interval::base0_whole_capture_refusal_v1 as refusal;
    assert_eq!(refusal(network, network, PALW_HELD_STEP_LADDER_V1), None, "at the network's ladder, whole as before");
    let past = refusal(network + 1, network, PALW_HELD_STEP_LADDER_V1).expect("past the materialization cap");
    assert!(past.contains("materialization cap") && past.contains("ADR-0121"), "{past}");
    for outside in [0, PALW_HELD_STEP_LADDER_V1 + 1, u64::MAX] {
        let why = refusal(outside, network, PALW_HELD_STEP_LADDER_V1).expect("outside the class's ladder");
        assert_eq!(why, "the binding's leaf count is outside the ruleset's ladder");
    }
}

/// **The same drill on the held row** (ADR-0103 Decisions 3 and 5): graph-v7 registers the held
/// map, so the bottom's anchor chunk proves into its slice's sub-root and the top tree — the
/// two-level path the court reads against the class's own layout — and the dissection is
/// exact on real committed rows and convicts the forged one, exactly as on graph-v5.
#[test]
fn a_real_held_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted() {
    let geometry = v5_geometry();
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry)
        .expect("the v7 projection is a valid profile");
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile));
    real_fused_capture_answers_and_convicts(geometry, profile);
}

fn real_fused_capture_answers_and_convicts(
    geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1,
    profile: PalwShapeProfileV3,
) {
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCourtVerdictV1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
        check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
    };
    use kaspa_consensus_core::palw_attn_dissect::PalwAttnDissectRoundV1;
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};

    let artifact = v5_artifact(geometry);
    // Twenty prompt positions and four decode calls: the last position reads 23 rows of
    // history, two court tiles, so the phase plays a round before its bottom.
    let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), (20, 4))
        .expect("the fused row is servable");
    assert!(backend.supports_dissection(), "the dense tier takes a dissection's turn");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0093)).expect("the anchor implies a job");
    let honest = backend.execute(&job, &prompt).expect("the fused class runs");
    let binding = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
    let last_call = job.exact_decode_tokens.saturating_sub(1);
    let fused_at_last: Vec<u64> = (0..binding.step_leaf_count)
        .filter(|i| {
            let c = canonical_step_coordinates(&profile, &binding.job_context, *i).expect("a coordinate");
            c.call_index == last_call
                && profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused)
        })
        .collect();
    assert_eq!(fused_at_last.len(), 4, "two layers, two heads, one output tile each");
    let session = Hash64::from_u64_word(0x5E55);
    let choose = |phase: &mut PalwAttnDissectPhaseV1, child: u8, daa: u64| {
        let choice =
            PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: session, round: phase.round(), child };
        phase.apply_choice(&choice, daa, 10).expect("a legal choice");
    };

    for &narrowed in &fused_at_last {
        let evidence =
            backend.attn_site_evidence(&honest.material, narrowed, None, None).expect("the honest capture yields its evidence");
        let site = evidence.site_v1(root, false).expect("the site derives");
        let anchored = evidence.site_v1(root, true).expect("and with its anchor");
        assert!(anchored.site.every_position_is_checkpointed, "the v5 row registers the tiled map: the anchor route");
        let root_claim = evidence.root_claim_v1(&site).expect("the root computes");
        let committed = palw_attn_opened_lanes_v1(&evidence.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
        let tiles = (site.history_positions as u64).div_ceil(site.tile_positions as u64);
        assert_eq!(tiles, 2, "the last position's history is two court tiles");
        for target in 0..tiles {
            let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
                session,
                &root_claim,
                site.head_lanes,
                site.history_positions,
                &committed,
                site.site.params.values,
                2,
                site.tile_positions,
                0,
                10,
                true,
            )
            .expect("the honest root finalizes to the committed tile");
            let mut daa = 1;
            while phase.turn() != PalwBisectTurnV1::Terminal {
                let round = evidence.round_v1(&site, &phase).expect("a round computes");
                phase.apply_round(&round, daa, 10).expect("an honest round folds");
                assert_eq!(evidence.divergent_child_v1(&site, &phase).expect("recomputes"), None, "nothing to name");
                let child = phase.child_ranges().iter().position(|&(f, c)| target >= f && target < f + c).expect("covered") as u8;
                choose(&mut phase, child, daa + 1);
                daa += 2;
            }
            let bottom = evidence.bottom_v1(&anchored, &phase).expect("the bottom builds");
            assert_eq!(
                check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true),
                Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
                "leaf {narrowed}, tile {target}: the evidence must be exact on real committed rows"
            );
        }
    }

    // ---- negative control: the check is not vacuous ----
    // One K code of the responder's INPUTS moved (the committed rows untouched): its claims are
    // no longer what the bottom recomputes from the commitments, and it convicts ITSELF — the
    // hazard ADR-0093 Decision 4 names, which is why inputs must be the capture's exactly.
    {
        let narrowed = fused_at_last[0];
        let mut skewed = backend.attn_site_evidence(&honest.material, narrowed, None, None).expect("evidence");
        let site = skewed.site_v1(root, false).expect("site");
        let anchored = skewed.site_v1(root, true).expect("site");
        let committed = palw_attn_opened_lanes_v1(&skewed.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
        let honest_root = skewed.root_claim_v1(&site).expect("root");
        // A V code whose position carries weight: a row whose requantized probability is zero
        // moves nothing (the first draft of this control skewed one and proved nothing), so
        // the perturbation is chosen as the first one that changes the whole-history claim.
        let tiles = (site.history_positions as u64).div_ceil(site.tile_positions as u64);
        let scale = (honest_root.claim.max, honest_root.claim.exp_sum);
        let lanes = site.head_lanes.2 as usize;
        let offset = (0..site.history_positions as usize * lanes)
            .map(|n| (n / lanes) * site.site.kv_dim + site.site.kv_off + site.head_lanes.1 as usize + n % lanes)
            .find(|&at| {
                let mut probe = skewed.inputs.clone();
                probe.v_series[at] ^= 0x40;
                probe.range_claim_v1(&site, 0, tiles, scale).expect("computes") != honest_root.claim
            })
            .expect("some V code carries weight");
        skewed.inputs.v_series[offset] ^= 0x40;
        let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
            session,
            &honest_root,
            site.head_lanes,
            site.history_positions,
            &committed,
            site.site.params.values,
            2,
            site.tile_positions,
            0,
            10,
            true,
        )
        .expect("the honest root opens");
        let mut daa = 1;
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let honest_round = skewed.round_v1(&site, &phase).expect("computes");
            // The skewed responder's round no longer folds to the honest root it filed.
            assert!(phase.clone().apply_round(&honest_round, daa, 10).is_err(), "a skewed input breaks the fold");
            let exact = backend
                .attn_site_evidence(&honest.material, narrowed, None, None)
                .expect("evidence")
                .round_v1(&site, &phase)
                .expect("computes");
            phase.apply_round(&exact, daa, 10).expect("the exact round folds");
            let last = (phase.child_ranges().len() - 1) as u8;
            choose(&mut phase, last, daa + 1);
            daa += 2;
        }
    }

    // ---- forged: the output tile of the last fused leaf moved by one lane ----
    let narrowed = *fused_at_last.last().expect("a fused leaf");
    let lying = backend.execute_with_injected_fault(&job, &prompt, narrowed).expect("a forged capture commits");
    let accused = backend.attn_site_evidence(&lying.material, narrowed, None, None).expect("the accused capture's commitments open");
    let challenger = backend.attn_site_evidence(&honest.material, narrowed, None, None).expect("the challenger's own evidence");
    let site = accused.site_v1(root, false).expect("the site derives");
    let anchored = accused.site_v1(root, true).expect("and with its anchor");
    let honest_root = accused.root_claim_v1(&site).expect("the inputs are the history's");
    let forged_tile = palw_attn_opened_lanes_v1(&accused.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
    let values = site.site.params.values;
    let open = |root: &kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1| {
        PalwAttnDissectPhaseV1::open_with_arity(
            session,
            root,
            site.head_lanes,
            site.history_positions,
            &forged_tile,
            values,
            2,
            site.tile_positions,
            0,
            10,
            true,
        )
    };
    assert!(open(&honest_root).is_err(), "an honest root cannot finalize to a forged tile: the forger must lie");
    // The least lie: one lane of V* moved until it finalizes to the forged tile.
    let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
    let lane = (0..forged_tile.len()).find(|l| finalize(&honest_root.claim.v_acc)[*l] != forged_tile[*l]).expect("a moved lane");
    let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
    let target = forged_tile[lane];
    let at = |delta: i64| {
        let mut v = honest_root.claim.v_acc.clone();
        v[lane] += delta;
        finalize(&v)[lane]
    };
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if at(mid) < target { lo = mid + 1 } else { hi = mid }
    }
    let delta = lo;
    let mut lying_root = honest_root.clone();
    lying_root.claim.v_acc[lane] += delta;
    assert_eq!(finalize(&lying_root.claim.v_acc), forged_tile, "the least lie finalizes to the forged tile");
    let mut phase = open(&lying_root).expect("the lie opens the phase");
    let mut daa = 1;
    while phase.turn() != PalwBisectTurnV1::Terminal {
        // The forger's round: the honest children with the lie pushed into the first, so the
        // disclosure still folds to its own root.
        let mut round: PalwAttnDissectRoundV1 = accused.round_v1(&site, &phase).expect("computes");
        round.children[0].v_acc[lane] += delta;
        phase.apply_round(&round, daa, 10).expect("the forger's disclosure folds to its own lie");
        let named = challenger.divergent_child_v1(&site, &phase).expect("the challenger recomputes").expect("the lie is somewhere");
        assert_eq!(named, 0, "the challenger names the child the lie hides in");
        choose(&mut phase, named, daa + 1);
        daa += 2;
    }
    let bottom = accused.bottom_v1(&anchored, &phase).expect("the bottom opens the accused's own commitments");
    assert_eq!(
        check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true),
        Ok(PalwAttnCourtVerdictV1::ExecutorGuilty),
        "the forged row is convicted at the tile its lie hides in"
    );
}

/// **ADR-0093 Decision 7: a forged FOLD is bottomed without one row the accused must serve.**
///
/// The free-prompt lane retains a fold — no rows — and a forged fold's re-execution is the
/// honest execution, so the capture alone opens nothing it committed. What the challenger has
/// instead is enough: its own rows before the disputed leaf (the court narrowed to the first
/// leaf it could not reproduce) and the accused's own opened output tile, which the root claim
/// carried on chain. Pinned on one forged execution retained both ways:
///
/// * the fold without the tile is refused by name — the limitation, with its remedy in the words;
/// * the fold WITH the tile yields the dense capture's evidence exactly, opening for opening;
/// * a tile that is not the accused's (the honest one, at the same leaf) opens nothing;
/// * the least lie's dissection, bottomed out of the fold's evidence, convicts.
#[test]
fn a_forged_fold_is_bottomed_from_the_honest_prefix_and_the_root_claims_tile() {
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCourtVerdictV1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
        check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
    };
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;
    use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), (20, 4))
        .expect("the fused row is servable");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0093)).expect("the anchor implies a job");
    let honest = backend.execute(&job, &prompt).expect("the fused class runs");
    let binding = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
    let last_call = job.exact_decode_tokens.saturating_sub(1);
    let narrowed = (0..binding.step_leaf_count)
        .rfind(|i| {
            let c = canonical_step_coordinates(&profile, &binding.job_context, *i).expect("a coordinate");
            c.call_index == last_call
                && profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused)
        })
        .expect("a fused leaf at the last call");

    // One forged execution, retained both ways: the drill's dense capture, and its fold.
    let lying = backend.execute_with_injected_fault(&job, &prompt, narrowed).expect("a forged capture commits");
    let cap = backend.step_ladder_cap();
    let mut run = a16_execute_for_attempt_capped_v1(&artifact, &profile, backend.plan.as_ref(), &job, &prompt, cap).expect("runs");
    {
        let (ctx_hash, profile_hash) = (job.context_hash(), profile.shape_profile_id());
        let slot = run.tiles.tiles.iter_mut().find(|(i, _)| *i == narrowed).expect("the tile");
        slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        run.tiles.leaves[narrowed as usize] =
            kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &slot.1);
    }
    run.binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
        &profile,
        &job,
        &run.tiles,
        &run.checkpoints,
        &kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
            kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        ),
        run.trace_root,
        crate::produce::base0_activation_leg_root_v1(&job),
        cap,
    )
    .expect("the forged binding");
    run.step_tree = Some(
        crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
            &run.tiles.leaves,
            crate::fp_capture::palw_base0_sparse_retain_level_v1(cap),
            cap,
        )
        .expect("the fold of the forged leaves"),
    );
    let prompt_ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let fold = crate::produce::base0_fp_material_encode_v2(&run, &prompt_ids).expect("the fold retains");
    assert_eq!(
        crate::produce::base0_material_decode_any_v1(&fold).expect("decodes").binding(),
        crate::produce::base0_material_decode_any_v1(&lying.material).expect("decodes").binding(),
        "one forged execution, two retentions"
    );

    // Without the accused's tile: refused by name.
    let refused = backend.attn_site_evidence(&fold, narrowed, None, None).expect_err("a fold keeps no rows");
    assert!(refused.contains("root claim"), "the refusal names the remedy: {refused}");
    // With it: the dense capture's evidence, exactly.
    let from_dense = backend.attn_site_evidence(&lying.material, narrowed, None, None).expect("the dense capture opens");
    let from_fold =
        backend.attn_site_evidence(&fold, narrowed, None, Some(&from_dense.out_tile)).expect("the fold opens through the prefix");
    assert_eq!(from_fold, from_dense, "the fold's evidence IS the dense capture's, opening for opening");
    // A tile that is not the accused's opens nothing.
    let honest_tile = backend.attn_site_evidence(&honest.material, narrowed, None, None).expect("the honest capture opens").out_tile;
    let wrong = backend.attn_site_evidence(&fold, narrowed, None, Some(&honest_tile)).expect_err("not the accused's tile");
    assert!(wrong.contains("do not root") || wrong.contains("not the accused"), "{wrong}");

    // The least lie's dissection, bottomed out of the FOLD's evidence.
    let session = Hash64::from_u64_word(0x5E57);
    let site = from_fold.site_v1(root, false).expect("the site derives");
    let anchored = from_fold.site_v1(root, true).expect("and with its anchor");
    let honest_root = from_fold.root_claim_v1(&site).expect("the inputs are the history's");
    let forged_tile = palw_attn_opened_lanes_v1(&from_fold.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
    let values = site.site.params.values;
    let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
    let lane = (0..forged_tile.len()).find(|l| finalize(&honest_root.claim.v_acc)[*l] != forged_tile[*l]).expect("a moved lane");
    let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
    let at = |delta: i64| {
        let mut v = honest_root.claim.v_acc.clone();
        v[lane] += delta;
        finalize(&v)[lane]
    };
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if at(mid) < forged_tile[lane] { lo = mid + 1 } else { hi = mid }
    }
    let mut lying_root = honest_root.clone();
    lying_root.claim.v_acc[lane] += lo;
    let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
        session,
        &lying_root,
        site.head_lanes,
        site.history_positions,
        &forged_tile,
        values,
        2,
        site.tile_positions,
        0,
        10,
        true,
    )
    .expect("the lie opens the phase");
    let mut daa = 1;
    while phase.turn() != PalwBisectTurnV1::Terminal {
        let mut round = from_fold.round_v1(&site, &phase).expect("computes");
        round.children[0].v_acc[lane] += lo;
        phase.apply_round(&round, daa, 10).expect("the forger's disclosure folds to its own lie");
        let choice = PalwAttnDissectChoiceV1 {
            version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
            session_id: session,
            round: phase.round(),
            child: 0,
        };
        phase.apply_choice(&choice, daa + 1, 10).expect("a legal choice");
        daa += 2;
    }
    let bottom = from_fold.bottom_v1(&anchored, &phase).expect("the bottom opens the accused's commitments through the prefix");
    assert_eq!(
        check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true),
        Ok(PalwAttnCourtVerdictV1::ExecutorGuilty),
        "the forged fold is convicted at the tile its lie hides in"
    );
}

/// The least lie a forger of `evidence`'s committed tile can open a dissection with, played to
/// the bottom with the forger's rounds (the honest children, the lie pushed into the first) and
/// the challenger naming that child — and the bottom's verdict, out of `evidence`.
fn least_lie_verdict_v1(
    evidence: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1,
    root: Hash64,
) -> kaspa_consensus_core::palw_attn_court_v1::PalwAttnCourtVerdictV1 {
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1, check_attn_dissect_bottom_v1,
        palw_attn_opened_lanes_v1,
    };
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    let session = Hash64::from_u64_word(0x5E58);
    let site = evidence.site_v1(root, false).expect("the site derives");
    let anchored = evidence.site_v1(root, true).expect("and with its anchor");
    let honest_root = evidence.root_claim_v1(&site).expect("the inputs are the history's");
    let forged = palw_attn_opened_lanes_v1(&evidence.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
    let values = site.site.params.values;
    let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
    let lane = (0..forged.len()).find(|l| finalize(&honest_root.claim.v_acc)[*l] != forged[*l]).expect("a moved lane");
    let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
    let at = |delta: i64| {
        let mut v = honest_root.claim.v_acc.clone();
        v[lane] += delta;
        finalize(&v)[lane]
    };
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if at(mid) < forged[lane] { lo = mid + 1 } else { hi = mid }
    }
    let mut lie = honest_root.clone();
    lie.claim.v_acc[lane] += lo;
    let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
        session,
        &lie,
        site.head_lanes,
        site.history_positions,
        &forged,
        values,
        2,
        site.tile_positions,
        0,
        10,
        true,
    )
    .expect("the lie opens the phase");
    let mut daa = 1;
    while phase.turn() != PalwBisectTurnV1::Terminal {
        let mut round = evidence.round_v1(&site, &phase).expect("computes");
        round.children[0].v_acc[lane] += lo;
        phase.apply_round(&round, daa, 10).expect("the forger's disclosure folds to its own lie");
        let choice = PalwAttnDissectChoiceV1 {
            version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
            session_id: session,
            round: phase.round(),
            child: 0,
        };
        phase.apply_choice(&choice, daa + 1, 10).expect("a legal choice");
        daa += 2;
    }
    let bottom = evidence.bottom_v1(&anchored, &phase).expect("the bottom builds");
    check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true).expect("the bottom adjudicates")
}

/// **ADR-0093 Decisions 7 and 8 on the dense tier: a bottom from the accused's FILING alone.**
///
/// * The drill's forgery (a committed tile altered, the execution honest): the filing — the
///   accused's binding and opened tile, no anchor — yields the dense capture's own evidence,
///   opening for opening, without a byte of the accused's capture: the case of a capture over
///   the material cap, which no challenger ever pulls.
/// * A forger whose execution FOLLOWED its lie (its checkpoints after the disputed call differ):
///   its own dense capture cannot bottom it (this node's leg is not its leg), the filing
///   without an anchor is refused naming Decision 8's anchored root claim, and the filing WITH
///   its anchor — the forger's own checkpoint, opened against its own leg — convicts.
#[test]
fn a_filing_bottoms_the_accused_without_its_capture_and_its_anchor_convicts_a_deep_forger() {
    use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnCourtVerdictV1, PalwAttnRowOpeningV1};
    use kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1;
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;
    use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};
    use kaspa_consensus_core::palw_step_leg::{
        PalwStepOpeningV1, checkpoint_leaf_hash_v2, step_merkle_path_capped_v1, step_merkle_root_capped_v1,
    };

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), (20, 4))
        .expect("the fused row is servable");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0093)).expect("the anchor implies a job");
    let honest = backend.execute(&job, &prompt).expect("the fused class runs");
    let binding = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
    // A fused leaf at the FIRST decode call, so the job has checkpoints after its anchor for a
    // forger's execution to differ in.
    let narrowed = (0..binding.step_leaf_count)
        .find(|i| {
            let c = canonical_step_coordinates(&profile, &binding.job_context, *i).expect("a coordinate");
            c.call_index == 1 && profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused)
        })
        .expect("a fused leaf at the first decode call");
    let cap = backend.step_ladder_cap();

    // ---- the drill's forgery, bottomed from its filing alone ----
    let lying = backend.execute_with_injected_fault(&job, &prompt, narrowed).expect("a forged capture commits");
    let from_capture = backend.attn_site_evidence(&lying.material, narrowed, None, None).expect("the dense capture opens");
    let filing = PalwAttnAccusedFilingV1 {
        binding: crate::produce::base0_material_decode_any_v1(&lying.material).expect("decodes").binding().clone(),
        out_tile: from_capture.out_tile.clone(),
        anchor: None,
    };
    let from_filing = backend.attn_site_evidence_from_filing(&filing, narrowed, None).expect("the filing opens through the prefix");
    assert_eq!(from_filing, from_capture, "the filing's evidence IS the dense capture's, opening for opening");
    assert_eq!(least_lie_verdict_v1(&from_filing, root), PalwAttnCourtVerdictV1::ExecutorGuilty);

    // ---- a forger whose execution followed its lie ----
    let honest_evidence = backend.attn_site_evidence(&honest.material, narrowed, None, None).expect("the honest capture opens");
    let anchored_site = honest_evidence.site_v1(root, true).expect("the site with its anchor");
    let a = honest_evidence.anchor.as_ref().expect("the v5 row anchors its bottom").anchor.leaf.checkpoint_index as usize;
    let mut run = a16_execute_for_attempt_capped_v1(&artifact, &profile, backend.plan.as_ref(), &job, &prompt, cap).expect("runs");
    {
        let (ctx_hash, profile_hash) = (job.context_hash(), profile.shape_profile_id());
        let slot = run.tiles.tiles.iter_mut().find(|(i, _)| *i == narrowed).expect("the tile");
        slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        run.tiles.leaves[narrowed as usize] =
            kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &slot.1);
    }
    // Every checkpoint after the anchor states another cache — what a run that followed its lie
    // commits — re-chained, re-hashed and re-rooted exactly as a producer would.
    let b = &anchored_site.binding;
    assert!(a + 1 < run.checkpoints.leaves.len(), "the job has checkpoints after the anchor to differ");
    for i in a + 1..run.checkpoints.leaves.len() {
        let prev = run.checkpoints.leaf_hashes[i - 1];
        let leaf = &mut run.checkpoints.leaves[i];
        leaf.state_chunks_root = Hash64::from_u64_word(0xDEE9_0000 + i as u64);
        leaf.prev_checkpoint_leaf_hash = prev;
        run.checkpoints.leaf_hashes[i] =
            checkpoint_leaf_hash_v2(&b.job_context_hash, &b.checkpoint_profile_hash, &b.state_chunk_map_id, leaf);
    }
    run.checkpoints.merkle_root = step_merkle_root_capped_v1(&run.checkpoints.leaf_hashes, cap).expect("the leg roots");
    run.binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
        &profile,
        &job,
        &run.tiles,
        &run.checkpoints,
        &kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
            kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        ),
        run.trace_root,
        crate::produce::base0_activation_leg_root_v1(&job),
        cap,
    )
    .expect("the deep forger's binding");
    let deep = crate::produce::base0_material_encode_v1(&run).expect("the deep forger retains");
    let own = backend.attn_site_evidence(&deep, narrowed, None, None).expect_err("this node's leg is not the deep forger's");
    assert!(own.contains("checkpoint leaves do not root"), "{own}");
    let out_tile = PalwAttnRowOpeningV1 {
        leaf: run.tiles.tiles.iter().find(|(i, _)| *i == narrowed).expect("the tile").1.clone(),
        opening: PalwStepOpeningV1 {
            leaf_index: narrowed,
            leaf_hash: run.tiles.leaves[narrowed as usize],
            siblings: step_merkle_path_capped_v1(&run.tiles.leaves, narrowed as usize, cap).expect("a path"),
        },
    };
    let mut deep_filing = PalwAttnAccusedFilingV1 { binding: run.binding.clone(), out_tile, anchor: None };
    let unanchored = backend.attn_site_evidence_from_filing(&deep_filing, narrowed, None).expect_err("no anchor, no bottom");
    assert!(unanchored.contains("ADR-0093 Decision 8"), "the refusal names the anchored root claim: {unanchored}");
    deep_filing.anchor = Some(PalwAttnCheckpointAnchorV1 {
        leaf: run.checkpoints.leaves[a].clone(),
        opening: PalwStepOpeningV1 {
            leaf_index: a as u64,
            leaf_hash: run.checkpoints.leaf_hashes[a],
            siblings: step_merkle_path_capped_v1(&run.checkpoints.leaf_hashes, a, cap).expect("a path"),
        },
    });
    let evidence = backend.attn_site_evidence_from_filing(&deep_filing, narrowed, None).expect("the filed anchor opens the bottom");
    assert_eq!(least_lie_verdict_v1(&evidence, root), PalwAttnCourtVerdictV1::ExecutorGuilty, "the deep forger is convicted");
    // And a filed anchor that is not the accused's committed checkpoint opens nothing.
    let mut forged_anchor = deep_filing.clone();
    if let Some(anchor) = forged_anchor.anchor.as_mut() {
        anchor.leaf.state_chunks_root = Hash64::from_u64_word(0xBAD);
    }
    assert!(backend.attn_site_evidence_from_filing(&forged_anchor, narrowed, None).is_err(), "a forged anchor is refused");
}

/// **ADR-0103 Decision 2, through the seam a node holds: a held class is seated in runs of
/// positions, and a lie in the PROMPT is a fault in the interval that holds it.**
///
/// The graph-v7 row at `n_ctx` 32 is cut into eight-position intervals, so a twenty-position
/// prompt is three intervals of the job's own — where the shipped unit made it ONE interval 0
/// that a seat either replayed whole or never checked. The chain's count (`fp_interval_count_for`)
/// is the capture's; the honest interval 0 is licensed; and the same interval of a capture whose
/// first embedding gather was moved and re-committed is a fault at that leaf.
/// **ADR-0085 Decision 3 against a liar: a close from served intervals convicts once the block
/// the lie sits in is served** (ADR-0086 Decision 6's block-leaves lane).
///
/// A V4 opening carries no leaf hashes, so the challenger's replay supplies its own. Under the
/// served frontier those walk to the committed step root only when they ARE the committed
/// leaves, which a lie inside the interval rules out. The close used to require that walk and
/// refused every lying interval, and its only test ran an honest executor. With the executor's
/// own leaves for the block substituted (`base0_fp_range_with_served_block_v1`), the range is
/// the committed one and the close is about the leaf the lie is in. Pinned on the graph-v5 dense
/// row, whose logits are tiled so the served close applies, at a width whose interval 0 holds
/// whole retained blocks (the drill's capture is dense, and a dense tuple opens interval 0):
///
/// * without the block, the close is refused by name, and the name says what is missing;
/// * with it, the close assembles, is the capture path's object for the same leaf (ADR-0085
///   X1), and the one-move verdict on it is `ExecutorGuilty`;
/// * the same close against the honest capture at the same leaf needs no block and is
///   `FalseAccusation`.
#[test]
fn a_close_from_served_intervals_convicts_a_lying_interval_once_its_block_is_served() {
    use kaspa_consensus_core::palw_shard_court_v1::{PalwLeafEvidenceV1, PalwShardCourtVerdictV1};
    use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};
    // Wide enough that interval 0 holds whole 4,096-leaf blocks.
    let geometry = PalwQwen25GeometryV1 {
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
    let artifact = v5_artifact(geometry);
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry)
        .expect("the v5 projection is a valid profile");
    let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), (40, 4))
        .expect("the dense row is servable");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0085)).expect("a job");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let honest = backend.execute(&job, &prompt).expect("the class runs");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let ladder = backend.step_ladder_cap();

    // A leaf inside a whole block of interval 0 that is not a fused site: its close is
    // arithmetic, and its block is one the executor can serve.
    let interval_0 = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(
        &backend.open_fp_interval(&honest.material, 0, &ids).expect("interval 0 opens"),
    )
    .expect("V4");
    let leaf_count = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().step_leaf_count;
    let (first_block, end_block) = interval_0.range.whole_blocks_v1(leaf_count);
    assert!(end_block > first_block + 1, "the fixture's interval 0 holds whole blocks ({first_block}..{end_block})");
    let block_len = 1u64 << interval_0.range.retain_level;
    let leaf = ((first_block + 1) * block_len..end_block * block_len)
        .find(|l| {
            canonical_step_coordinates(&profile, &job, *l)
                .is_some_and(|c| profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind != PalwStepOpKindV1::AttnFused))
        })
        .expect("a whole block past the first holds a leaf that is not a fused site");
    assert_eq!(backend.fp_interval_of_leaf_v1(&job, leaf), Some(0), "the leaf is interval 0's");

    let close_of = |outcome: &PalwExecutionOutcomeV1, with_block: bool| {
        let claim = PalwClaimRootsV1 {
            execution_root: outcome.execution_root,
            trace_root: outcome.trace_root,
            anchor: job.job_id,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        let annexed = backend.open_fp_interval_with_close(&outcome.material, 0, &ids, &[leaf]).expect("the annex");
        let generated = backend.fp_committed_output_ids(&outcome.material).expect("the answer's ids");
        let mut held = vec![(0u32, annexed)];
        if with_block {
            let block = leaf >> interval_0.range.retain_level;
            let served = backend.open_fp_block_leaves(&outcome.material, 0, block, &ids).expect("the executor serves the block");
            let index = crate::fp_interval::base0_fp_block_leaves_request_index_v1(0, block).expect("a block request index");
            held.push((index, served));
        }
        backend.refutation_from_served_intervals(&held, claim, &ids, &generated, leaf_count, leaf)
    };
    let verdict_of = |refutation: kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
        let artifact_openings = backend.operand_openings_for(&refutation).expect("the rows the step reads open");
        let (refutation, prompt_ids_opening) =
            kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(backend.prompt_ids_form, refutation)
                .expect("the carriage");
        PalwLeafEvidenceV1 { refutation, artifact_openings, prompt_ids_opening }.verdict_v1(profile.shape_profile_id(), root, ladder)
    };

    let lying = backend.execute_with_injected_fault(&job, &prompt, leaf).expect("a lie commits");
    let close = close_of(&lying, true).expect("with the block served, the lying interval closes");
    let refused = close_of(&lying, false).expect_err("without the block, the lying interval does not close");
    assert!(refused.contains("block"), "the refusal names what is missing: {refused}");
    let by_capture = backend.refutation_for_index(&lying.material, leaf).expect("the capture path closes the same leaf");
    assert_eq!(close, by_capture, "ADR-0085 X1: one object by either route, against a liar too");
    assert_eq!(verdict_of(close), Ok(PalwShardCourtVerdictV1::ExecutorGuilty), "the liar's close convicts");

    let clear = close_of(&honest, false).expect("the honest interval closes without a block");
    assert_eq!(clear, close_of(&honest, true).expect("and with one"), "an honest range needs no block");
    assert_eq!(verdict_of(clear), Ok(PalwShardCourtVerdictV1::FalseAccusation), "the honest close clears");
}

#[test]
fn a_held_class_seats_its_prompt_in_intervals_and_a_lie_in_it_is_a_fault() {
    use kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1;
    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry)
        .expect("the v7 projection is a valid profile");
    let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile.clone(), (20, 4))
        .expect("the held row is servable");
    let width = kaspa_consensus_core::palw_held_context_v1::palw_held_interval_positions_v1(&profile);
    assert_eq!(width, 8, "n_ctx 32 over the draw's four");
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0103)).expect("a job");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let steps = u64::from(job.declared_prefill_tokens) + u64::from(job.exact_decode_tokens - 1);
    let chain_count = backend.fp_interval_count_for(job.declared_prefill_tokens, job.exact_decode_tokens).expect("a count");
    assert_eq!(u64::from(chain_count), steps.div_ceil(u64::from(width)), "⌈(prefill + decode calls) / P⌉");
    assert!(u64::from(width) < u64::from(job.declared_prefill_tokens), "interval 0 is P positions, not the prompt");

    let honest = backend.execute(&job, &prompt).expect("the held class runs");
    assert_eq!(backend.fp_interval_count(&honest.material), Some(chain_count), "the capture's count is the chain's");
    let claim = |o: &PalwExecutionOutcomeV1| PalwClaimRootsV1 {
        execution_root: o.execution_root,
        trace_root: o.trace_root,
        anchor: job.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let leaves = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().step_leaf_count;
    let opened = backend.open_fp_interval(&honest.material, 0, &ids).expect("interval 0 opens");
    assert_eq!(backend.verify_fp_interval_opening(&opened, claim(&honest), 0, &ids, leaves), PalwFpIntervalVerdictV1::Valid);

    let lying = backend.execute_with_injected_fault(&job, &prompt, 0).expect("a lie in the prompt commits");
    let opened = backend.open_fp_interval(&lying.material, 0, &ids).expect("the liar's interval 0 opens");
    // A V4 opening names the BLOCK a fault is in (ADR-0086 Decision 6); the block-leaves lane
    // names the leaf from there.
    match backend.verify_fp_interval_opening(&opened, claim(&lying), 0, &ids, leaves) {
        PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {
            assert!(first_leaf_index == 0 && leaf_count > 0, "the block that holds leaf 0: {first_leaf_index} + {leaf_count}")
        }
        PalwFpIntervalVerdictV1::Fault { leaf_index } => assert_eq!(leaf_index, 0),
        other => panic!("the lie in the prompt must be a fault in the interval that holds it, got {other:?}"),
    }
}

/// **A `graph-v5` row is court-capable, and it says so through the constructor the chain uses.**
///
/// `court_capable` was `state_chunk_map_id == integer_kv_state_chunk_map_id_v2()`, written out
/// in both constructors. ADR-0082 Decision 4 has the v5 row register the TILED map — the whole
/// reason the dense tier's close is flat, because the dissection's bottom opens ONE history
/// tile — so the registered row answered `supports_court() == false` over its own profile.
/// That is not a cosmetic label: ADR-0069 Decision 5 admits a class that cannot take a court's
/// turn WEIGHTLESS, so the row the genesis registers would have been admitted and earned
/// nothing, and the failure is silent because a `false` here is a legal answer for the v1 row.
///
/// Both directions are pinned. A v2-mapped row must STILL be court-capable — the predicate got
/// wider, not moved — because that row is a live chain fact.
#[test]
fn the_graph_v5_row_is_court_capable_and_the_v2_row_it_replaces_still_is() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_artifact_row_profile_v1, qwen25_a16_artifact_row_profile_v5};

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);

    let v5 = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    assert!(
        map::palw_map_addresses_history_tiles_v1(&v5),
        "the row under test must be the tiled one, or this test is about the wrong class"
    );
    let v5_backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), v5.clone(), (4, 2))
        .expect("the v5 row compiles a plan over the artifact's epsilon");
    assert!(v5_backend.supports_court(), "a tiled-map class takes a court's turn — ADR-0069 D5 weightlessness turns on this");

    let v2 = qwen25_a16_artifact_row_profile_v1(geometry).expect("the v2 projection is a valid profile");
    assert_eq!(v2.state_chunk_map_id, map::integer_kv_state_chunk_map_id_v2(), "the v2 row keeps its own map");
    let v2_backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), v2.clone(), (4, 2))
        .expect("the v2 row still compiles a plan");
    assert!(v2_backend.supports_court(), "widening the predicate must not drop the row already registered");

    assert_ne!(v5.shape_profile_id(), v2.shape_profile_id(), "a class IS its graph: these are two classes");
}

/// **The v5 row plans over the converter's epsilon, and executes.**
///
/// `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }` is the refusal that
/// stopped EVERY dense row from being built from its own registered profile — declared 1,
/// executed `1 << 8` — and it was found only because a test drove `from_registered_profile`
/// instead of `::new`. `qwen25_a16_artifact_row_profile_v1` closed it for the v2 row; this
/// pins that the v5 row inherits the closure rather than re-opening it, and that the plan it
/// compiles actually runs a job rather than merely type-checking.
#[test]
fn the_graph_v5_row_plans_over_the_converters_epsilon_and_runs_a_job() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_artifact_row_profile_v5, qwen25_a16_profile_v5};

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);

    // The DECLARED epsilon, unprojected: still refused, on that field and no other. Without
    // this half the test above would pass on a row whose epsilon nobody had corrected.
    let frozen = qwen25_a16_profile_v5(geometry).expect("the frozen v5 projection is a valid profile");
    let message = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), frozen, (4, 2))
        .err()
        .expect("a row declaring an epsilon its artifact does not execute must refuse");
    assert!(message.contains("rms_eps_q"), "the refusal names the field: {message}");

    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
        .expect("the served v5 row compiles a plan");
    let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0000_0825)).expect("the anchor implies a job");
    let outcome = backend.execute(&ctx, &prompt).expect("the compiled v5 plan executes its own canonical job");
    let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
    assert_eq!(binding.shape_profile.shape_profile_id(), profile.shape_profile_id(), "it produced for the class it was given");
    assert!(binding.step_leaf_count > 0, "an execution with no committed steps is not one");
}

/// **The shipped constructor runs a graph-v5 job — which is H-1 of ADR-0082's audit E.**
///
/// `Qwen25A16Backend::new` is the constructor the SDK's dense lineage and the shipped
/// free-prompt worker take for a class this build's own table names. It stored `plan: None`,
/// so it executed through `forward_token_traced` — the compiled twenty-seven-row v2 program —
/// and the moment the dense tier moves to ADR-0082's fused site the producer refused its own
/// class by name (the refusal is pinned by the test below). Not a consensus break: a shipping
/// break, and precisely the row this ADR exists to ship.
///
/// The close is one authority rather than two: `new` compiles `plan_from_profile`, exactly as
/// `from_registered_profile` does, so a declaration is executed as declared or refused with
/// the node named. This test is the difference measured on the same fixture the refusal test
/// uses — same artifact, same graph, one constructor apart.
#[test]
fn the_shipped_constructor_compiles_the_plan_and_runs_a_graph_v5_job() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    assert_eq!(profile.attn_nodes.len(), 24, "a v5 layer declares twenty-four nodes — the traced route records 27");

    let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
        .expect("the constructor compiles the declared graph instead of assuming a v2 program");
    let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0000_8201)).expect("the anchor implies a job");
    let outcome = backend.execute(&ctx, &prompt).expect("the shipped constructor executes its own class's fused graph");
    let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
    assert_eq!(
        binding.shape_profile.shape_profile_id(),
        profile.shape_profile_id(),
        "it produced for the class it was given, not for a v2 stand-in"
    );
    assert!(binding.step_leaf_count > 0, "an execution with no committed steps is not one");
    assert!(backend.supports_court(), "and the tiled-map row it serves takes a court's turn");

    // A declaration this artifact cannot serve is now a REFUSAL rather than a silent
    // execution under the artifact's own constants: the frozen v5 row declares `rms_eps_q` 1
    // where the converter writes 256, and that is the asymmetry H-1 closed.
    let frozen = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(geometry).expect("a valid profile");
    let message = Qwen25A16Backend::new(v5_artifact(geometry), NETWORK.to_vec(), frozen, (4, 2))
        .err()
        .expect("a row declaring an epsilon its artifact does not execute must refuse here too");
    assert!(message.contains("rms_eps_q"), "the refusal names the field: {message}");
}

/// **The plan-less route is the v2 reference, and a graph-v5 row is refused there by name.**
///
/// `A16Engine::forward_token_traced` is a COMPILED program — twenty-seven rows a layer, the
/// attention site spelled as scores / softmax / requant / values — and ADR-0082 Decision 1
/// replaces those four with one. So the traced route cannot serve a v5 row and must not
/// pretend to: teaching it the fusion would put a second authority beside
/// `plan_from_profile`, which is exactly what ADR-0067 Decision 2 merged. The Decision-F probe
/// is what states the boundary, and this pins that it states it on the fused row rather than
/// mis-executing it.
#[test]
fn the_plan_less_route_is_the_v2_reference_and_refuses_a_fused_row() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

    let geometry = v5_geometry();
    let artifact = v5_artifact(geometry);
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    let (ctx, prompt) = crate::produce::base0_rc_job_v1(
        &profile,
        Hash64::from_u64_word(0x0000_8200),
        geometry.vocab_size as usize,
        3,
        2,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );

    let message = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt)
        .err()
        .expect("the twenty-seven-row traced program cannot execute a twenty-four-node declaration");
    println!("plan-less refusal on a v5 row: {message}");
    assert!(message.contains("registered graph"), "the refusal names the boundary: {message}");
    assert!(message.contains("per-layer declares 24 against 27 recorded"), "and the count that disagrees: {message}");
}

/// **A class whose artifact declares no tokenizer is refused at the lane's entrance, by name.**
///
/// The shipped `qwen25-1.5b-a16.palwart` carries 64 zero bytes there (read off the file at
/// offset 1,777,209,032), so every job it produced published `tokenizer_id` 0 — a value that
/// pins nothing, inside `PalwJobContextV2::context_hash`, which
/// `PalwFreePromptJobV3::tokenizer_id` "MUST equal". A replay under a different `tokenizer.json`
/// then computes different ids and a different context hash and reproduces nothing, and the
/// producer is defaulted for work it performed correctly. `Undeclared` was reported and
/// stepped over; here it is a refusal.
///
/// The exemption is the LANE the caller resolved (audit D M-6): `from_registered_profile` is
/// the fixture/drill door and reads the artifact's own seed tag, so every fixture in this
/// module keeps working, while the chain-registered door
/// (`from_registered_profile_in_lane_v1(.., ConvertedA16)`, what `resolve_chain_registered`
/// calls) has no exemption at all — see the test below it.
#[test]
fn an_artifact_that_declares_no_tokenizer_cannot_serve_a_registered_dense_class() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

    let geometry = v5_geometry();
    let derived = v5_artifact(geometry);
    assert_eq!(derived.tokenizer_commitment, Hash64::default(), "a derived fixture declares none, and may");
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    assert!(
        Qwen25A16Backend::from_registered_profile(derived.clone(), NETWORK.to_vec(), profile.clone(), (4, 2)).is_ok(),
        "a derived artifact is a fixture, not a registered class"
    );

    // The same weights with the derived marker gone — the shape a CONVERTED artifact has, and
    // the shape the file on disk has.
    let converted = Base0ArtifactV1::from_parts(
        derived.shape,
        derived.embed.clone(),
        derived.unembed.clone(),
        derived.layers.clone(),
        derived.norm_requant,
        derived.residual_requant,
    )
    .expect("the parts of a valid artifact rebuild one")
    .with_a16_params(derived_a16_store(&derived.shape))
    .expect("the derived store is sorted and unique");
    assert!(!converted.is_derived(), "this is the converted-artifact case");

    let message =
        Qwen25A16Backend::from_registered_profile(std::sync::Arc::new(converted.clone()), NETWORK.to_vec(), profile.clone(), (4, 2))
            .err()
            .expect("a converted artifact with a zero tokenizer commitment must be refused");
    assert!(message.contains("tokenizer_commitment"), "the refusal names the field: {message}");
    assert!(message.contains("qwen25-convert"), "and the command that fixes it: {message}");
    assert!(message.contains("palw-class bind-tokenizer"), "and the cheap command, first (ADR-0096 D10): {message}");

    // Bound: it serves, and the job it builds publishes THAT identity rather than zero.
    let bound =
        std::sync::Arc::new(converted.with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{\"model\":\"qwen2.5\"}")));
    let backend = Qwen25A16Backend::from_registered_profile(bound.clone(), NETWORK.to_vec(), profile, (4, 2))
        .expect("an artifact that declares its tokenizer serves");
    let (ctx, _) = backend.job_for_anchor(Hash64::from_u64_word(0x70CE_9155)).expect("the anchor implies a job");
    assert_eq!(ctx.tokenizer_id, bound.tokenizer_commitment, "the job publishes the artifact's own tokenizer identity");
    assert_ne!(ctx.tokenizer_id, Hash64::default(), "and it is not the zero that pinned nothing");
}

/// **Audit D M-6: the tokenizer exemption is a property of the LANE, and flipping
/// `derived_seed` alone moves nothing.**
///
/// The guard waived the refusal for `artifact.is_derived()`, on the ground that "a derived
/// artifact must never be reported as a registered class". Nothing enforces that:
/// `derived_seed` is absent from `artifact_digest()` AND from the operand inventory, so the
/// same bytes with and without the seed tag have the same `artifact_root`, a chain row
/// resolves to either, and `dense_artifact_by_registered_root` matches on that root alone. One
/// flipped byte therefore bought a pass, and every job the node then produced published
/// `tokenizer_id` 0 — the un-pinned state the guard exists to make unrepresentable.
///
/// This asserts the two halves the finding rests on (the roots do not move) and the fix (the
/// chain lane's answer does not move either).
#[test]
fn flipping_the_derived_seed_does_not_move_the_root_or_the_chain_lanes_tokenizer_refusal() {
    use crate::classes::ArtifactSourceV1;
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

    let geometry = v5_geometry();
    let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
    let seeded = v5_artifact(geometry);
    assert!(seeded.is_derived(), "the fixture carries the seed tag");
    assert_eq!(seeded.tokenizer_commitment, Hash64::default(), "and declares no tokenizer");

    // The same bytes with the tag gone — one field, nothing else.
    let cleared = Base0ArtifactV1::from_parts(
        seeded.shape,
        seeded.embed.clone(),
        seeded.unembed.clone(),
        seeded.layers.clone(),
        seeded.norm_requant,
        seeded.residual_requant,
    )
    .expect("the parts of a valid artifact rebuild one")
    .with_a16_params(derived_a16_store(&seeded.shape))
    .expect("the derived store is sorted and unique");
    assert!(!cleared.is_derived());

    // **The finding's premise, measured**: neither root the chain can name moves with the tag.
    assert_eq!(seeded.artifact_digest(), cleared.artifact_digest(), "the digest does not cover derived_seed");
    assert_eq!(
        crate::inventory::a16_inventory_v1(&seeded, &profile).expect("roots").root(),
        crate::inventory::a16_inventory_v1(&cleared, &profile).expect("roots").root(),
        "the operand inventory does not cover derived_seed either"
    );

    // **The fix**: on the CHAIN lane the refusal is the same on both, so the tag buys nothing.
    for (what, artifact) in [("seeded", seeded.as_ref()), ("cleared", &cleared)] {
        let err = Qwen25A16Backend::check_tokenizer_declared_v1(artifact, ArtifactSourceV1::ConvertedA16)
            .expect_err("{what}: a chain-registered dense class must declare its tokenizer");
        assert!(err.contains("tokenizer_commitment"), "{what}: the refusal names the field: {err}");
    }
    let err = Qwen25A16Backend::from_registered_profile_in_lane_v1(
        seeded.clone(),
        NETWORK.to_vec(),
        profile,
        (4, 2),
        ArtifactSourceV1::ConvertedA16,
    )
    .err()
    .expect("the chain lane refuses the seeded artifact exactly as it refuses the cleared one");
    assert!(err.contains("qwen25-convert"), "and names the command that fixes it: {err}");

    // The seeded FLOOR is what the exemption is for, and it still passes — the lane says so,
    // not the file.
    Qwen25A16Backend::check_tokenizer_declared_v1(&seeded, ArtifactSourceV1::Derived(0x5A16))
        .expect("the derived floor declares no tokenizer and may");
}
