//! **`fp_interval`'s `the_rulesets_ladder` module**, moved out of `fp_interval.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;
use kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES;
use kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
use kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;

/// The genesis row's own canonical job, as a binding — the roots are not this test's subject
/// (nothing here recomputes them) and the two fields that are, `shape_profile` and
/// `job_context`, are the registered row's and the job the chain would pay for.
fn v5_512_binding() -> (PalwStepBindingV2, u64) {
    let row = crate::classes::a16_graph_v5_row_v1().expect("the graph-v5 512 row is in this build");
    let (prefill, decode) = row.canonical_job;
    let (job_context, _prompt) = crate::produce::base0_rc_job_v1(
        &row.profile,
        Hash64::from_u64_word(0x0000_082B_2512),
        row.artifact_shape.vocab,
        prefill,
        decode,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    let step_leaf_count =
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&row.profile, &job_context, COURT_MAX_STEP_LEAVES)
            .expect("the canonical job of an admitted class fits the ruleset it was admitted under");
    let binding = PalwStepBindingV2 {
        version: 2,
        job_context,
        shape_profile: row.profile.clone(),
        checkpoint_profile: kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
            PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        ),
        state_chunk_map_id: row.profile.state_chunk_map_id,
        full_logits_trace_root: Hash64::default(),
        activation_leg_root: Hash64::default(),
        step_leaf_count,
        step_merkle_root: Hash64::default(),
        checkpoint_count: 0,
        checkpoint_merkle_root: Hash64::default(),
        committed_execution_root: Hash64::default(),
    };
    (binding, step_leaf_count)
}

/// **The class the genesis registers is priced at the ruleset's ladder and NAMED at the
/// executor's.**
///
/// The real `a16_graph_v5_row_v1()` profile and the real canonical job — no fixture, no
/// scaling. Under the ruleset the row was admitted under, a seat derives the same 6,630,544
/// leaves the binding declares and proceeds. Under the executor's constant it refuses BY NAME,
/// with both numbers in the error: not a panic, not an allocation, and not the silent
/// `Mismatch` that made the class unlicensable.
#[test]
fn the_512_rows_honest_price_clears_the_rulesets_ladder_and_is_named_at_the_executors() {
    let (binding, leaves) = v5_512_binding();
    assert!(
        leaves > PALW_STEP_MAX_LEAVES && leaves <= COURT_MAX_STEP_LEAVES,
        "the row is only interesting because it is between the two: {leaves} leaves, executor {PALW_STEP_MAX_LEAVES}, ruleset {COURT_MAX_STEP_LEAVES}"
    );
    eprintln!(
        "graph-v5 512 row: canonical job ({}, {}) prices {leaves} step leaves; executor constant {PALW_STEP_MAX_LEAVES}, RC ruleset ladder {COURT_MAX_STEP_LEAVES}",
        binding.job_context.declared_prefill_tokens, binding.job_context.exact_decode_tokens,
    );

    assert_eq!(base0_fp_binding_step_space_v1(&binding, COURT_MAX_STEP_LEAVES), Ok(leaves));
    assert_eq!(
        base0_fp_binding_step_space_v1(&binding, PALW_STEP_MAX_LEAVES),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
        "at the executor's constant the refusal must name the ladder that refused, and the price it refused"
    );

    // And the same at the seat's own door, which is where the class's licensability lives.
    assert!(
        Base0FpIntervalGeometryV1::from_binding_capped_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, COURT_MAX_STEP_LEAVES)
            .is_ok(),
        "a seat handed the ruleset's ladder must build a geometry for the row the genesis registers"
    );
    assert_eq!(
        Base0FpIntervalGeometryV1::from_binding_capped_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1, PALW_STEP_MAX_LEAVES),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
    );
    // The un-capped name is the executor's default and must still BE the executor's default —
    // a caller with no ruleset in scope keeps exactly the behaviour it had.
    assert_eq!(
        Base0FpIntervalGeometryV1::from_binding_v1(&binding, PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: PALW_STEP_MAX_LEAVES }),
    );

    // A price that is not the geometry's is still refused under its own name at either ladder —
    // raising the ceiling must not have turned the equality into an inequality.
    let mut lying = binding.clone();
    lying.step_leaf_count = leaves - 1;
    assert!(
        matches!(
            base0_fp_binding_step_space_v1(&lying, COURT_MAX_STEP_LEAVES),
            Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared, .. }) if declared == leaves - 1
        ),
        "a binding that under-prices its own geometry is refused as a price, not as a ladder"
    );
    let mut inflated = binding.clone();
    inflated.step_leaf_count = leaves + 1;
    assert_eq!(
        base0_fp_binding_step_space_v1(&inflated, COURT_MAX_STEP_LEAVES),
        Err(Base0FpIntervalError::PriceIsNotTheGeometrys { declared: leaves + 1, derived: leaves }),
    );
    // …and one above the ladder never reaches the enumeration at all.
    let mut absurd = binding.clone();
    absurd.step_leaf_count = u64::MAX;
    assert_eq!(
        base0_fp_binding_step_space_v1(&absurd, COURT_MAX_STEP_LEAVES),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: u64::MAX, max: COURT_MAX_STEP_LEAVES }),
        "the ladder is checked before the count, so no allocation is sized from a stranger's u64"
    );
}

/// **ADR-0086 Decision 2 for the executor's side: an interval opened from a recomputed anchor
/// state is byte for byte the interval opened by the genesis walk.** The fold retains no
/// state, so the opener used to replay from the prompt with tiles captured for every interval
/// — 1,991 s for interval 187 of a 300-token claim on the devnet. With the family's recompute
/// kernels the executor resumes from the memoized state of the interval's named anchor and
/// replays only the interval's own calls; the opening it serves must be the same object, or a
/// seat would refuse an honest executor. Pinned on every interval past the first, and the
/// closure must actually have been consulted.
#[test]
fn an_interval_opened_from_the_recomputed_anchor_is_the_interval_opened_from_genesis() {
    // Wide enough that the fold retains more than one block (4,096 leaves at the ruleset's
    // level), so a span starts inside an earlier call and the anchored branch is walked.
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
    let (artifact, profile, ctx, prompt, run) = super::tests::dense_v5_run_with(geometry, 3, 24);
    eprintln!("the wide fixture prices {} leaves over {} checkpoints", run.binding.step_leaf_count, run.checkpoints.leaves.len());
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
    assert!(material.checkpoint_chunks.is_empty(), "the fixture is a fold with no state, the case that replayed from genesis");
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let wide = COURT_MAX_STEP_LEAVES;
    let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(&material.binding, interval, wide).expect("a geometry");
    assert!(geometry.interval_count >= 3, "the fixture has intervals past the first: {}", geometry.interval_count);
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let consulted = std::cell::Cell::new(0u32);
    let memo = crate::fp_recompute::Base0FpSeatMemoV1::default();
    let anchor_state_for = |covered: u32| {
        consulted.set(consulted.get() + 1);
        let mut recompute = crate::fp_recompute::A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("recompute kernels");
        crate::fp_recompute::base0_fp_seat_state_memoized_v1(
            &memo,
            &material.binding.shape_profile,
            &material.binding.job_context,
            &ids,
            &material.generated_token_ids,
            covered,
            &mut recompute,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .ok()
    };
    let leaf_count = material.binding.step_leaf_count;
    let mut anchored_intervals = 0u32;
    for index in 0..geometry.interval_count {
        let from_genesis = base0_open_fp_interval_sparse_capped_v1(
            &material,
            index,
            &ids,
            interval,
            wide,
            &kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the genesis walk opens");
        let before = consulted.get();
        let from_anchor = base0_open_fp_interval_sparse_anchored_capped_v1(
            &material,
            index,
            &ids,
            interval,
            wide,
            &kernels,
            &anchor_state_for,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the anchored walk opens");
        assert_eq!(from_anchor, from_genesis, "interval {index}: the two walks must serve one object");
        // The anchored branch is taken exactly when the span's STARTING interval has a named
        // anchor — the span covers whole retained blocks, so it may start in an earlier call.
        let leaves_geometry = base0_fp_interval_leaves_v1(&profile, &ctx, &geometry, index, leaf_count).expect("leaves");
        let (span_first, _) = material
            .step_tree
            .span_for_range(leaves_geometry.range_first, leaves_geometry.range_end - leaves_geometry.range_first)
            .expect("a span");
        let start_call = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, span_first)
            .expect("a main step leaf")
            .call_index;
        let expects_anchor = geometry.anchor_covered_call(interval_of_call_v1(&geometry, start_call)).is_some();
        assert_eq!(
            consulted.get() > before,
            expects_anchor,
            "interval {index}: consulted={} expected={expects_anchor}",
            consulted.get() > before
        );
        anchored_intervals += expects_anchor as u32;
        assert_eq!(
            base0_verify_fp_interval_opening_with_state_capped_v1(
                &from_anchor,
                claim,
                index,
                &ids,
                material.binding.step_leaf_count,
                interval,
                wide,
                geometry.anchor_covered_call(index).and_then(&anchor_state_for).as_ref(),
                &kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            Base0FpIntervalSeatVerdictV1::Valid,
            "interval {index}: a seat licenses the anchored opening"
        );
    }
    assert!(anchored_intervals > 0, "the fixture must exercise the anchored branch at least once");
    eprintln!("{anchored_intervals} of {} intervals resumed from a recomputed anchor", geometry.interval_count);
}

/// **A seat licenses an honest graph-v5 opening at the ladder it is HANDED, and refuses it by
/// name at a narrower one.**
///
/// The 512 row's own artifact is 1.7 GiB and its canonical job allocates a 424 MB capture, so
/// the end-to-end half runs on the tiny graph-v5 fixture (`dense_v5_run`, the same class
/// declaration at a fixture geometry) — and the LADDER is scaled instead of the leaf count.
/// That is the same predicate from the other side: the rule is `count > ladder`, and a fixture
/// whose count is above the handed ladder exercises exactly the comparison a 6.6M-leaf job
/// makes against `2^22`. What it proves is the thing the constant hid — that the seat reads the
/// number it is handed, and that the refusal when the number is too small is a NAME rather than
/// a panic or an allocation.
#[test]
fn a_seat_licenses_an_honest_v5_opening_at_the_ladder_it_is_handed() {
    let (artifact, profile, ctx, prompt, run) = super::tests::dense_v5_run();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let bytes = crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains");
    let material = crate::produce::base0_fp_material_decode_v2(&bytes).expect("its own retention decodes");
    let engine = crate::engine_a16::A16Engine::new(&artifact).expect("an A16 artifact");
    let plan = engine.plan_from_profile(&profile).expect("the plan");
    let kernels = crate::qwen25_a16_backend::a16_interval_kernels_for_tests_v1(&artifact, Some(&plan));
    let interval = PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
    let claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };

    let leaves = run.binding.step_leaf_count;
    let wide = COURT_MAX_STEP_LEAVES;
    let narrow = leaves - 1;
    assert!(leaves <= wide);
    eprintln!("the graph-v5 fixture prices {leaves} leaves; licensed at {wide}, refused at {narrow}");

    // The honest opening, produced at the wide ladder.
    let opened = base0_open_fp_interval_sparse_capped_v1(
        &material,
        0,
        &ids,
        interval,
        wide,
        &kernels,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
    .expect("an honest graph-v5 interval opens at the ruleset's ladder");

    assert_eq!(
        base0_verify_fp_interval_opening_with_state_capped_v1(
            &opened,
            claim,
            0,
            &ids,
            leaves,
            interval,
            wide,
            None,
            &kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        Base0FpIntervalSeatVerdictV1::Valid,
        "a seat handed the ladder the class was admitted under must license its honest opening"
    );

    // The same bytes, the same seat, one number narrower: refused, and refused BY NAME at the
    // price rule rather than by panicking or by sizing a capture from the field.
    assert_eq!(
        base0_fp_binding_step_space_v1(&run.binding, narrow),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: narrow }),
    );
    assert_eq!(
        base0_verify_fp_interval_opening_with_state_capped_v1(
            &opened,
            claim,
            0,
            &ids,
            leaves,
            interval,
            narrow,
            None,
            &kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        Base0FpIntervalSeatVerdictV1::Unverifiable,
        "a seat whose ladder cannot hold the class declines to replay it — a limit is not a verdict, so never Mismatch"
    );
    // The executor cannot serve one either, and says so with the numbers.
    assert_eq!(
        base0_open_fp_interval_sparse_capped_v1(
            &material,
            0,
            &ids,
            interval,
            narrow,
            &kernels,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        ),
        Err(Base0FpIntervalError::LeafCountOutOfRange { got: leaves, max: narrow }),
    );
}

/// **ADR-0085 X3: a v3 opening is a v2 opening to every seat path, annex or not** — and the
/// annex is read only by the closer's own function. The binding is the genesis row's, so the
/// framing is exercised on a real class shape; nothing here replays an interval.
#[test]
fn a_v3_opening_reads_as_its_v2_opening_and_the_annex_is_the_closers_alone() {
    let (binding, _) = v5_512_binding();
    let v2 = Base0FpIntervalOpeningV2 {
        version: PALW_BASE0_FP_INTERVAL_VERSION_V2,
        interval_index: 7,
        binding,
        range: kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1 {
            first_leaf_index: 40,
            leaf_hashes: vec![Hash64::from_u64_word(1), Hash64::from_u64_word(2)],
            siblings: vec![Hash64::from_u64_word(3)],
        },
        seed_row_leaf_count: 0,
        seed_row_tiles: Vec::new(),
        anchor: None,
    };
    let tile = PalwStepTileLeafV1 {
        version: 1,
        coord: kaspa_consensus_core::palw_step::PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
        value_count: 2,
        values_le: vec![1, 0, 0, 0, 2, 0, 0, 0],
    };
    let annex = Base0FpCloseAnnexV1 {
        rows_root: Hash64::from_u64_word(9),
        disputed: vec![Base0FpDisputedLeafV1 { leaf_index: 41, tile, anchor: None }],
    };
    let with =
        Base0FpIntervalOpeningV3 { version: PALW_BASE0_FP_INTERVAL_VERSION_V3, opening: v2.clone(), close: Some(annex.clone()) }
            .encode_v1()
            .unwrap();
    let without =
        Base0FpIntervalOpeningV3 { version: PALW_BASE0_FP_INTERVAL_VERSION_V3, opening: v2.clone(), close: None }.encode_v1().unwrap();
    assert_eq!(&with[..8], &PALW_BASE0_FP_INTERVAL_MAGIC_V3);
    for bytes in [&with, &without] {
        match base0_fp_interval_opening_decode_any_v1(bytes).unwrap() {
            Base0FpIntervalOpeningAnyV1::Recomputed(seen) => assert_eq!(*seen, v2, "the seat sees the v2 opening"),
            other => panic!("a v3 opening must read as Recomputed, got {other:?}"),
        }
    }
    assert_eq!(base0_fp_interval_close_annex_v1(&with), Some(annex));
    assert_eq!(base0_fp_interval_close_annex_v1(&without), None);
    assert_eq!(base0_fp_interval_close_annex_v1(&v2.encode_v1().unwrap()), None, "a v2 opening carries no annex");
    assert!(Base0FpIntervalOpeningV3::decode_v1(&v2.encode_v1().unwrap()).is_err(), "and is not a v3");
}
