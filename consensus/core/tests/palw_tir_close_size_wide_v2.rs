//! Explicit LOCAL V2 sizing tests. These test bounds and real step-leaf bytes,
//! not an unimplemented wide Court close, admission, lease or native activation.
//! Source-only addition: execution is delegated to the parent-owned test run.
use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_close_size_v1::{
        PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PalwTirClosePriceV1, PalwTirCloseSizingV1, PalwTirParamFormV1,
        palw_tir_worst_closes_work_v1,
    },
    palw_tir_close_size_v2::{
        PalwTirCloseBoundsV2, PalwTirCloseSizingV2, palw_tir_worst_closes_range_work_v2, palw_tir_worst_closes_work_v2,
    },
    palw_tir_court_v1::PalwTirInventoryIndexV1,
    palw_tir_step_v1::{
        PalwTirLeafKindV1, PalwTirStepError, PalwTirStepSpaceV1, PalwTirStepSpaceV2, palw_tir_leaf_preimage_v1,
        palw_tir_leaf_preimage_v2,
    },
    palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use kaspa_consensus_core::{palw_tir_close_range_v1 as range_v1, palw_tir_close_size_v1 as size_v1};
use misaka_palw_tir::demand::DemandContext;
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1, builder::ProgramBuilder, program::LOGITS_I64_Q24_SCHEME_V2};

type Sizer = fn(&PalwTirStepSpaceV2, &PalwJobContextV2, &PalwTirCloseSizingV2) -> Result<PalwTirCloseBoundsV2, String>;
const SIZERS: [Sizer; 2] = [palw_tir_worst_closes_work_v2, palw_tir_worst_closes_range_work_v2];

fn sizing(form: PalwTirParamFormV1, court: bool) -> PalwTirCloseSizingV2 {
    PalwTirCloseSizingV2 { form, court, cap: 1 << 24, stop_above: None }
}

// Adapted from the independently checked mixed-width step fixture. Pre's I8
// carry, a committed I32 intermediate and only the final post I64 are retained.
fn mixed_program(reduction: bool) -> TirProgramV1 {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let head = reduction.then(|| p.param("head", DType::I8, &[4, 4], false));
    let scale = p.konst(DType::I64, &[4], &[1 << 32, 1 << 33, 1 << 34, 1 << 35]);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = if let Some(head) = head {
        let row = post.reshape_fixed(Ref::CarryIn(0), &[1, 4]);
        let product = post.matmul(row, head, DType::I32);
        post.commit(product);
        post.reshape_fixed(product, &[4])
    } else {
        let narrow = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(narrow);
        narrow
    };
    let logits = post.mul(narrow, scale, DType::I64);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::validate::validate(&program).unwrap();
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    program
}

fn constant_program(wide: bool) -> TirProgramV1 {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let data = p.konst(DType::I32, &[4], &[7, -8, 9, -10]);
    // Every block needs a live node. A committed constant read is a root in
    // normal form even with no carry; an unread uncommitted node would be dead.
    let mut pre = p.block("pre", vec![]);
    let pre_value = pre.cast(data, DType::I32);
    pre.commit(pre_value);
    let pre = pre.finish(&[]);
    let mut post = p.block("post", vec![]);
    let logits = post.cast(data, if wide { DType::I64 } else { DType::I32 });
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = if wide { LOGITS_I64_Q24_SCHEME_V2 } else { tiled_logits_scheme_id_v1().as_bytes() };
    misaka_palw_tir::validate::validate(&program).unwrap();
    program
}

// Start from the valid Fixed/window-Hist step fixture. With no Fixed state the
// committed window is H-local; with Fixed state it exercises the general replay
// path. An optional committed reduction also asks the root/bottom sizing path.
fn history_program(with_fixed: bool, reduction: bool) -> (TirProgramV1, u16, Option<u16>) {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let scale = p.konst(DType::I64, &[4], &[1 << 32, 1 << 33, 1 << 34, 1 << 35]);
    let fixed = with_fixed.then(|| p.fixed_state("last_row", DType::I8, &[4], -128, 127, false));
    let hist = p.hist_state("rows", DType::I8, &[4], 3, false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    if let Some(fixed) = fixed {
        pre.state_write(fixed, row);
    }
    let window = pre.hist_append(hist, row);
    let window = pre.cast(window, DType::I32);
    pre.commit(window);
    let Ref::Node(window_node) = window else { unreachable!() };
    let reduction_node = reduction.then(|| {
        let sum = pre.reduce_sum(window, 0, DType::I32);
        pre.commit(sum);
        let Ref::Node(node) = sum else { unreachable!() };
        node
    });
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(narrow);
    let logits = post.mul(narrow, scale, DType::I64);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::validate::validate(&program).unwrap();
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    (program, window_node, reduction_node)
}

fn fixture(program: TirProgramV1, tile: u32) -> (PalwTirClassV1, PalwJobContextV2) {
    let wide = program.wide_logits_q24();
    let committed = program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
    let class = PalwTirClassV1 {
        version: 1,
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![tile; committed],
            state_tiles: vec![4; program.states.len()],
        },
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let zero = Hash64::from_bytes([0; 64]);
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_u64_word(41),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [2; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class.class_id(&Hash64::from_u64_word(29)),
        trace_scheme_id: if wide { kaspa_consensus_core::palw_logits_q24_v2::scheme_id() } else { tiled_logits_scheme_id_v1() },
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[0, 1]).unwrap(),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 2,
        max_context_tokens: 16,
    };
    (class, job)
}

#[test]
fn fixed_and_windowed_history_size_first_late_local_and_reduction_paths_without_changing_leaf_widths() {
    let (mut fixed_leaves, mut history_leaves, mut early_h_leaves, mut late_h_leaves, mut reduction_bounds) = (0, 0, 0, 0, 0);
    for with_fixed in [false, true] {
        for reduction in [false, true] {
            for tile in [4, 8] {
                let (program, window_node, reduction_node) = history_program(with_fixed, reduction);
                let (mut class, mut job) = fixture(program, tile);
                class.layout.checkpoint_interval = 3;
                job.shape_profile_id = class.class_id(&Hash64::from_u64_word(29));
                let original = class.program.clone();
                let space = PalwTirStepSpaceV2::new(&class).unwrap();
                assert_eq!(space.fixed_instances().len(), usize::from(with_fixed));
                assert_eq!(space.hist_instances().len(), 1);
                // Two positions precede a full three-row history window; seven
                // positions exercise clipping, completed two-row history tiles,
                // checkpoint replay and both early/late H-tile alignments.
                for decode in [1, 6] {
                    job.exact_decode_tokens = decode;
                    let leaf_count = space.leaf_count_capped(&job, 128).unwrap();
                    assert!(leaf_count > 0);
                    for form in [PalwTirParamFormV1::PerLeaf, PalwTirParamFormV1::Multiproof] {
                        for court in [false, true] {
                            let limits = sizing(form, court);
                            let element = SIZERS[0](&space, &job, &limits).unwrap();
                            let range = SIZERS[1](&space, &job, &limits).unwrap();
                            assert!(!element.bounds().is_empty());
                            assert_eq!(element.bounds(), range.bounds());
                            assert!(element.sizing_work() > 0 && range.sizing_work() > 0);
                            let local = element
                                .bounds()
                                .iter()
                                .find(|b| b.block == space.program().schedule.pre && b.node == window_node)
                                .expect("the H-local committed window has a bound");
                            assert!(!local.dissected);
                            assert!(local.close_bytes > 0);
                            if let Some(node) = reduction_node {
                                let reduced = element
                                    .bounds()
                                    .iter()
                                    .find(|b| b.block == space.program().schedule.pre && b.node == node)
                                    .expect("the committed H reduction has a bound");
                                assert_eq!(reduced.dissected, court);
                                assert!(reduced.close_bytes > 0, "whole or bottom sizing must produce a bound");
                                assert_eq!(reduced.root_claim_bytes > 0, court);
                                reduction_bounds += usize::from(court);
                            }
                            let largest_close = element.bounds().iter().map(|b| b.close_bytes).max().unwrap();
                            let maximum_payload = space.maximum_leaf_payload_bytes_v2(&job).unwrap();
                            for index in 0..leaf_count {
                                let leaf = space.leaf_at(&job, index).unwrap();
                                let lanes: Vec<i128> = (0..leaf.value_count)
                                    .map(|n| if leaf.dtype == DType::I64 { (i128::from(n) - 2) << 35 } else { i128::from(n) - 4 })
                                    .collect();
                                let actual = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &lanes).unwrap();
                                let serialized = borsh::to_vec(&actual).unwrap().len() as u64;
                                assert_eq!(serialized, 26 + actual.values_le.len() as u64);
                                assert!(serialized <= 26 + u64::from(leaf.value_count) * 8);
                                assert!(actual.values_le.len() as u64 <= maximum_payload);
                                assert!(serialized <= largest_close);
                                match leaf.kind {
                                    PalwTirLeafKindV1::State { .. } => {
                                        fixed_leaves += 1;
                                        assert!(with_fixed);
                                        assert_eq!(actual.version, 1);
                                        assert_eq!(actual.values_le.len(), leaf.value_count as usize * 4);
                                        assert_eq!(actual, palw_tir_leaf_preimage_v1(&leaf, &lanes).unwrap());
                                    }
                                    PalwTirLeafKindV1::HistTile { .. } => {
                                        history_leaves += 1;
                                        assert_eq!(actual.version, 1);
                                        assert_eq!(actual.values_le.len(), leaf.value_count as usize * 4);
                                        assert_eq!(actual, palw_tir_leaf_preimage_v1(&leaf, &lanes).unwrap());
                                    }
                                    PalwTirLeafKindV1::Commit { node, block, .. }
                                        if node == window_node && block == space.program().schedule.pre =>
                                    {
                                        if leaf.position == 0 {
                                            early_h_leaves += 1;
                                        }
                                        if leaf.position >= 3 {
                                            late_h_leaves += 1;
                                        }
                                        assert_eq!(actual.version, 1);
                                        assert_eq!(actual.values_le.len(), leaf.value_count as usize * 4);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                assert_eq!(space.program().encode(), original);
                assert_eq!(class.program, original);
            }
        }
    }
    assert!(fixed_leaves > 0 && history_leaves > 0 && early_h_leaves > 0 && late_h_leaves > 0 && reduction_bounds > 0);
}

#[test]
fn mixed_width_nonempty_sizing_succeeds_and_bounds_actual_serialized_leaves() {
    for tile in [4, 8] {
        let (class, job) = fixture(mixed_program(false), tile);
        let original = class.program.clone();
        let space = PalwTirStepSpaceV2::new(&class).unwrap();
        let count = space.leaf_count_capped(&job, 64).unwrap();
        assert!(count > 2);
        for form in [PalwTirParamFormV1::PerLeaf, PalwTirParamFormV1::Multiproof] {
            for court in [false, true] {
                let limits = sizing(form, court);
                let element = SIZERS[0](&space, &job, &limits).unwrap();
                let range = SIZERS[1](&space, &job, &limits).unwrap();
                assert!(!element.bounds().is_empty());
                assert_eq!(element.bounds(), range.bounds());
                assert!(element.sizing_work() > 0 && range.sizing_work() > 0);
                // Independent mixed-fixture carriage count: 1567 B fixed
                // frame (three commit descriptors; depth-three step tree),
                // 32 B disputed lanes, and ONE narrow predecessor quoted
                // conservatively as 26+32 B plus its 4+3*64 B sibling run.
                // This catches a forgotten four-byte operand path even when
                // the final wide output itself was widened correctly.
                let narrow_close = element.bounds().iter().find(|b| b.block == space.program().schedule.post && b.node == 0).unwrap();
                assert_eq!(narrow_close.close_bytes, 1567 + 32 + (26 + 32) + (4 + 3 * 64));
                let (mut narrow, mut wide) = (0, 0);
                for i in 0..count {
                    let leaf = space.leaf_at(&job, i).unwrap();
                    let lanes = match leaf.dtype {
                        DType::I8 => {
                            narrow += 1;
                            vec![1i128; leaf.value_count as usize]
                        }
                        DType::I32 => {
                            narrow += 1;
                            vec![7i128; leaf.value_count as usize]
                        }
                        DType::I64 => {
                            wide += 1;
                            vec![1082i128 << 24; leaf.value_count as usize]
                        }
                        other => panic!("unexpected dtype {other:?}"),
                    };
                    let preimage = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &lanes).unwrap();
                    // Independent fixed-width Borsh arithmetic: u16 version,
                    // four u32 coordinates, u32 count, Vec<u8> length, lanes.
                    let lane_bytes = if leaf.dtype == DType::I64 { 8 } else { 4 };
                    let actual = borsh::to_vec(&preimage).unwrap().len() as u64;
                    assert_eq!(actual, 26 + u64::from(leaf.value_count) * lane_bytes);
                    assert_eq!(preimage.version, if lane_bytes == 8 { 2 } else { 1 });
                    let PalwTirLeafKindV1::Commit { block, node, .. } = leaf.kind else { panic!("unexpected state leaf") };
                    let bound = element.bounds().iter().find(|b| b.block == block && b.node == node).unwrap();
                    assert!(actual <= bound.close_bytes);
                }
                assert!(narrow > 0 && wide > 0);
            }
        }
        assert_eq!(space.program().encode(), original);
        assert_eq!(class.program, original);
    }
}

#[test]
fn each_twin_enforces_its_own_exact_work_cap_instead_of_accepting_two_errors() {
    let (class, job) = fixture(mixed_program(false), 4);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    for size in SIZERS {
        let mut limits = sizing(PalwTirParamFormV1::Multiproof, true);
        let full = size(&space, &job, &limits).unwrap();
        let work = full.sizing_work();
        assert!(work > 0);
        limits.cap = work;
        let exact = size(&space, &job, &limits).unwrap();
        assert_eq!(exact.bounds(), full.bounds());
        assert_eq!(exact.sizing_work(), work);
        limits.cap = work - 1;
        assert_eq!(size(&space, &job, &limits).unwrap_err(), PALW_TIR_CLOSE_SIZING_OVER_CAP_V1);
        limits.cap = 0;
        assert_eq!(size(&space, &job, &limits).unwrap_err(), PALW_TIR_CLOSE_SIZING_OVER_CAP_V1);
    }
}

#[test]
fn stop_above_keeps_the_first_excess_bound_and_uses_strict_byte_comparison() {
    // Dissection is specifically reduction over H, not a fixed-size MatMul.
    let (program, _, _) = history_program(false, true);
    let (class, job) = fixture(program, 4);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    for size in SIZERS {
        let mut limits = sizing(PalwTirParamFormV1::Multiproof, true);
        let full = size(&space, &job, &limits).unwrap();
        assert!(full.bounds().len() >= 3);
        let first = &full.bounds()[0];
        assert!(first.close_bytes > 0);
        limits.stop_above = Some((first.close_bytes, u64::MAX));
        let exact = size(&space, &job, &limits).unwrap();
        assert!(exact.bounds().len() > 1, "equality must not stop at the first bound");
        limits.stop_above = Some((first.close_bytes - 1, u64::MAX));
        let excess = size(&space, &job, &limits).unwrap();
        assert_eq!(excess.bounds(), &full.bounds()[..1]);
        let index = full.bounds().iter().position(|b| b.dissected && b.root_claim_bytes > 0).expect("a real reduction bound");
        let root = full.bounds()[index].root_claim_bytes;
        limits.stop_above = Some((u64::MAX, root));
        assert!(size(&space, &job, &limits).unwrap().bounds().len() > index + 1);
        limits.stop_above = Some((u64::MAX, root - 1));
        assert_eq!(size(&space, &job, &limits).unwrap().bounds(), &full.bounds()[..=index]);
    }
}

#[test]
fn unsupported_composite_is_not_reported_as_exhausted_sizing_work() {
    let (class, job) = fixture(mixed_program(false), 4);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    for size in SIZERS {
        let error = size(&space, &job, &sizing(PalwTirParamFormV1::Composite { p: 1 }, true)).unwrap_err();
        assert_eq!(error, "wide Q24 composite close sizing is not implemented");
        assert_ne!(error, PALW_TIR_CLOSE_SIZING_OVER_CAP_V1);
    }
}

#[test]
fn profile_and_original_validation_metadata_are_not_replaceable_width_flags() {
    let (class, _) = fixture(mixed_program(false), 4);
    let program = class.decode_program().unwrap();
    let mut info = misaka_palw_tir::validate::validate(&program).unwrap();
    info.blocks[0].window = Some(2);
    assert!(matches!(
        PalwTirStepSpaceV2::from_program(program, info, class.layout.clone()),
        Err(PalwTirStepError::Program(message)) if message.contains("metadata")
    ));
    let mut wrong = class.clone();
    let mut program = wrong.decode_program().unwrap();
    program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    wrong.program = program.encode();
    assert!(PalwTirStepSpaceV2::new(&wrong).is_err());
    let mut wrong = class.clone();
    wrong.version = 2;
    assert!(matches!(PalwTirStepSpaceV2::new(&wrong), Err(PalwTirStepError::Layout(_))));
    // Legacy readers must still refuse the original genuine wide program.
    assert!(matches!(PalwTirStepSpaceV1::new(&class), Err(PalwTirStepError::Layout(_))));
}

#[test]
fn invalid_job_metadata_and_overflowing_token_budget_fail_before_sizing() {
    let (class, job) = fixture(mixed_program(false), 4);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    for mode in 0..4 {
        let mut changed = job.clone();
        match mode {
            0 => changed.trace_scheme_id = tiled_logits_scheme_id_v1(),
            1 => changed.version = 3,
            2 => changed.exact_decode_tokens = 0,
            _ => {
                changed.declared_prefill_tokens = u32::MAX;
                changed.exact_decode_tokens = u32::MAX;
            }
        }
        assert!(matches!(space.job_shape(&changed), Err(PalwTirStepError::Job(_))));
        for size in SIZERS {
            let error = size(&space, &changed, &sizing(PalwTirParamFormV1::Multiproof, true)).unwrap_err();
            assert_ne!(error, PALW_TIR_CLOSE_SIZING_OVER_CAP_V1);
            assert_ne!(error, "wide Q24 composite close sizing is not implemented");
        }
    }
}

fn hash(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        h.update(part);
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}

#[test]
fn hand_derived_legacy_four_lane_price_work_bytes_and_id_grammar_are_unchanged() {
    let (class, job) = fixture(constant_program(false), 4);
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let inventory = PalwTirInventoryIndexV1::new(&space.program).unwrap();
    assert_eq!(PALW_V2_MAX_NETWORK_ID_BYTES, 64);
    assert_eq!(space.leaf_count_capped(&job, 5).unwrap(), 5);
    // Hand count: widest job 818 B, referenced class 100 B, binding 1184 B;
    // depth-three opening 268 B, empty leaf/rows/options 44 B, outer tags and
    // session 67 B. Frame=1563, plus four legacy i32 lanes=16 => 1579 B.
    // Each of TWO commits: request(32+4) + one-node context + four visits +
    // four constant reads = 45 work. No extra carried tensor or state exists.
    for form in [PalwTirParamFormV1::PerLeaf, PalwTirParamFormV1::Multiproof] {
        let price = PalwTirClosePriceV1::new(&space, &inventory, &job, form).unwrap();
        assert_eq!(price.frame(0), 1563);
        assert_eq!(price.frame(4), 1579);
        let (bounds, work) = palw_tir_worst_closes_work_v1(
            &space,
            &inventory,
            &job,
            &PalwTirCloseSizingV1 { form, court: true, cap: 90, stop_above: None },
        )
        .unwrap();
        assert_eq!(work, 90);
        assert_eq!(bounds.len(), 2);
        for bound in bounds {
            assert_eq!(bound.close_bytes, 1579);
            assert!(!bound.dissected);
            assert_eq!(bound.root_claim_bytes, 0);
        }
    }
    let leaf = space.leaf_at(&job, 0).unwrap();
    let preimage = palw_tir_leaf_preimage_v1(&leaf, &[7, -8, 9, -10]).unwrap();
    assert_eq!(preimage.version, 1);
    assert_eq!(preimage.values_le, [7i32, -8, 9, -10].into_iter().flat_map(i32::to_le_bytes).collect::<Vec<_>>());
    assert_eq!(borsh::to_vec(&preimage).unwrap().len(), 42);
    let mut layout = vec![];
    layout.extend_from_slice(&1u16.to_le_bytes());
    for value in [16u32, 2, 2, 2, 4, 4, 0] {
        layout.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(layout, borsh::to_vec(&class.layout).unwrap());
    let graph = hash(b"misaka-palw/tir/graph-ir-root/v1", &[&class.program]);
    let layout = hash(b"misaka-palw/tir/layout/v1", &[&layout]);
    let artifact = Hash64::from_u64_word(29);
    assert_eq!(
        class.class_id(&artifact),
        hash(
            b"misaka-palw/tir/class-id/v1",
            &[
                &1u16.to_le_bytes(),
                graph.as_byte_slice(),
                layout.as_byte_slice(),
                artifact.as_byte_slice(),
                class.tokenizer_id.as_byte_slice()
            ]
        )
    );
    let name = b"tiled-logits-v1";
    assert_eq!(tiled_logits_scheme_id_v1(), hash(b"misaka-palw/tiled-logits/scheme/v1", &[&(name.len() as u64).to_le_bytes(), name]));
}

#[test]
fn wide_constant_lane_bound_keeps_eight_bytes_without_repricing_legacy_frames() {
    let (class, job) = fixture(constant_program(true), 4);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    for size in SIZERS {
        let bounds = size(&space, &job, &sizing(PalwTirParamFormV1::Multiproof, true)).unwrap();
        assert_eq!(bounds.bounds().len(), 2);
        // Same fixed DTO frame as the explicit legacy oracle; only its four
        // disputed lanes grow from 16 to 32 B. No wide Court success asserted.
        for bound in bounds.bounds() {
            assert_eq!(bound.close_bytes, 1563 + 4 * 8);
        }
    }
}

fn assert_width_refusal<T>(result: Result<T, String>, expected: &str) {
    let error = match result {
        Ok(_) => panic!("a mutated wide program escaped a legacy sizing entrance"),
        Err(error) => error,
    };
    assert!(error.contains(expected), "wrong refusal: {error}");
    assert_ne!(error, PALW_TIR_CLOSE_SIZING_OVER_CAP_V1);
}

fn empty_gen_price() -> size_v1::PalwGenClosePricingV1 {
    size_v1::PalwGenClosePricingV1 { stage_depths: vec![], image_tiles: vec![], name_extra: 0, frame: 0, root_frame: 0 }
}

// All requests deliberately have an impossible occurrence/target and no work
// budget. The width guard must refuse before either can be interpreted. This
// is an attack on public mutable V1 data, not a valid V1 model or native case.
fn assert_all_legacy_entrances_refuse(
    space: &PalwTirStepSpaceV1,
    job: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    expected: &str,
) {
    let limits = PalwTirCloseSizingV1 { form: PalwTirParamFormV1::Multiproof, court: true, cap: 0, stop_above: None };
    let model = size_v1::PalwGenTwinStageV1::default();
    let request = size_v1::PalwTirCloseRequestV1 {
        ctx: DemandContext { pos: u32::MAX, occurrence: u16::MAX },
        target: u16::MAX,
        elements: &[usize::MAX],
        supplied: &[],
        range: None,
        both: true,
    };
    let ranges = range_v1::PalwTirRangesV1::single(usize::MAX - 1, usize::MAX);
    let range_request = range_v1::PalwTirCloseRangeRequestV1 {
        ctx: request.ctx,
        target: request.target,
        ranges: &ranges,
        supplied: &[],
        range: None,
        both: true,
    };
    let mut cache = range_v1::PalwTirRangeCacheV1::default();
    let sentinel = vec![(7u8, 8u16, 9u64)];
    let mut trace = sentinel.clone();
    assert_width_refusal(size_v1::PalwTirClosePriceV1::new(space, inventory, job, limits.form), expected);
    // No stage depth exists. A width refusal must still precede that error.
    assert_width_refusal(size_v1::PalwTirClosePriceV1::generative(space, inventory, usize::MAX, 0, empty_gen_price()), expected);
    assert_width_refusal(size_v1::palw_tir_close_reads_v1(space, job, inventory, &request, 0), expected);
    assert_width_refusal(size_v1::palw_tir_close_reads_split_v1(space, job, inventory, &request, 0, true, true), expected);
    assert_width_refusal(size_v1::palw_gen_close_reads_v1(space, job, inventory, &request, 0, &model), expected);
    assert_width_refusal(range_v1::palw_tir_close_reads_range_v1(space, job, inventory, &range_request, 0), expected);
    assert_width_refusal(
        range_v1::palw_tir_close_reads_range_split_v1(space, job, inventory, &mut cache, &range_request, 0, true, true),
        expected,
    );
    assert_width_refusal(
        range_v1::palw_tir_close_reads_range_split_gen_v1(
            space,
            job,
            inventory,
            &mut cache,
            &range_request,
            0,
            true,
            true,
            Some(&model),
        ),
        expected,
    );
    assert_width_refusal(size_v1::palw_tir_worst_closes_v1(space, inventory, job, &limits), expected);
    assert_width_refusal(size_v1::palw_tir_worst_closes_work_v1(space, inventory, job, &limits), expected);
    assert_width_refusal(size_v1::palw_tir_worst_closes_trace_v1(space, inventory, job, &limits, &mut trace), expected);
    assert_width_refusal(range_v1::palw_tir_worst_closes_range_work_v1(space, inventory, job, &limits), expected);
    assert_width_refusal(range_v1::palw_tir_worst_closes_range_trace_v1(space, inventory, job, &limits, &mut trace), expected);
    assert_width_refusal(
        size_v1::palw_gen_worst_closes_v1(space, inventory, job, &limits, usize::MAX, 0, &model, empty_gen_price()),
        expected,
    );
    assert_width_refusal(
        range_v1::palw_gen_worst_closes_range_v1(space, inventory, job, &limits, usize::MAX, 0, &model, empty_gen_price()),
        expected,
    );
    assert_eq!(trace, sentinel, "a refused width must not append a partial sizing trace");
}

#[test]
fn mutating_a_public_legacy_space_to_known_wide_cannot_reach_any_four_byte_sizer() {
    let (class, job) = fixture(constant_program(false), 4);
    let mut space = PalwTirStepSpaceV1::new(&class).unwrap();
    let inventory = PalwTirInventoryIndexV1::new(&space.program).unwrap();
    // Its private geometry and public validation metadata remain those of the
    // genuine narrow space. The replaced program is independently valid wide.
    space.program = mixed_program(false);
    assert_all_legacy_entrances_refuse(&space, &job, &inventory, "wide Q24 requires explicit v2 close sizing");
}

#[test]
fn raw_non_four_byte_commits_or_final_logits_are_not_relabelled_as_unknown_scheme_support() {
    let (class, job) = fixture(constant_program(false), 4);
    let original = PalwTirStepSpaceV1::new(&class).unwrap();
    let inventory = PalwTirInventoryIndexV1::new(&original.program).unwrap();
    for dtype in [DType::I64, DType::I128] {
        for unknown_scheme in [false, true] {
            for final_only in [false, true] {
                let mut space = original.clone();
                if unknown_scheme {
                    space.program.logits_scheme_id = [0xa5; 64];
                }
                let (block, node) = if final_only {
                    (space.program.schedule.post as usize, space.program.logits as usize)
                } else {
                    (space.program.schedule.pre as usize, 0)
                };
                space.program.blocks[block].nodes[node].out.dtype = dtype;
                // Isolate the final-node branch from the committed-node branch.
                if final_only {
                    space.program.blocks[block].nodes[node].commit = false;
                }
                assert_all_legacy_entrances_refuse(
                    &space,
                    &job,
                    &inventory,
                    "non-four-byte committed or final lanes cannot use legacy close sizing",
                );
            }
        }
    }
}

#[test]
fn a_narrow_unknown_scheme_does_not_silently_change_legacy_synthetic_geometry_or_prices() {
    let (class, job) = fixture(constant_program(false), 4);
    let original = PalwTirStepSpaceV1::new(&class).unwrap();
    let inventory = PalwTirInventoryIndexV1::new(&original.program).unwrap();
    let mut changed = original.clone();
    changed.program.logits_scheme_id = [0xa5; 64];
    // This is the already supported sizing-only synthetic input; no NF parse,
    // scheme reassignment, constructor/admission or arithmetic success claimed.
    let limits = PalwTirCloseSizingV1 { form: PalwTirParamFormV1::Multiproof, court: true, cap: 1 << 20, stop_above: None };
    assert_eq!(
        size_v1::palw_tir_worst_closes_work_v1(&changed, &inventory, &job, &limits).unwrap(),
        size_v1::palw_tir_worst_closes_work_v1(&original, &inventory, &job, &limits).unwrap()
    );
    assert_eq!(
        range_v1::palw_tir_worst_closes_range_work_v1(&changed, &inventory, &job, &limits).unwrap(),
        range_v1::palw_tir_worst_closes_range_work_v1(&original, &inventory, &job, &limits).unwrap()
    );
    let price = PalwTirClosePriceV1::new(&changed, &inventory, &job, limits.form).unwrap();
    assert_eq!(price.frame(4), 1579);
    let request = size_v1::PalwTirCloseRequestV1 {
        ctx: DemandContext { pos: 1, occurrence: 1 },
        target: 0,
        elements: &[0, 1, 2, 3],
        supplied: &[],
        range: None,
        both: false,
    };
    assert_eq!(
        size_v1::palw_tir_close_reads_v1(&changed, &job, &inventory, &request, 45).unwrap(),
        size_v1::palw_tir_close_reads_v1(&original, &job, &inventory, &request, 45).unwrap()
    );
    let mut gen_price = empty_gen_price();
    gen_price.stage_depths = vec![3];
    gen_price.frame = 100;
    let gen_price = PalwTirClosePriceV1::generative(&changed, &inventory, 0, 0, gen_price).unwrap();
    assert_eq!(gen_price.frame(4), 116);
}

#[test]
fn all_four_legacy_committable_dtypes_keep_four_byte_leaf_encoding_and_prices() {
    for dtype in [DType::I8, DType::I16, DType::I32, DType::Idx] {
        let mut p = ProgramBuilder::new(4, 1 << 18);
        let values = p.konst(DType::I32, &[4], &[1, 2, 3, 4]);
        let mut pre = p.block("pre", vec![]);
        let value = pre.cast(values, dtype);
        pre.commit(value);
        let pre = pre.finish(&[]);
        let mut post = p.block("post", vec![]);
        let value = post.cast(values, dtype);
        post.commit(value);
        let Ref::Node(logits) = value else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = p.finish(pre, vec![], post, logits);
        program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
        misaka_palw_tir::validate::validate(&program).unwrap();
        let (class, job) = fixture(program, 4);
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let inventory = PalwTirInventoryIndexV1::new(&space.program).unwrap();
        let price = PalwTirClosePriceV1::new(&space, &inventory, &job, PalwTirParamFormV1::Multiproof).unwrap();
        assert_eq!(price.frame(4), 1579);
        for index in 0..5 {
            let leaf = space.leaf_at(&job, index).unwrap();
            assert_eq!(leaf.dtype, dtype);
            let encoded = palw_tir_leaf_preimage_v1(&leaf, &[1, 2, 3, 4]).unwrap();
            assert_eq!(encoded.version, 1);
            assert_eq!(encoded.values_le, [1i32, 2, 3, 4].into_iter().flat_map(i32::to_le_bytes).collect::<Vec<_>>());
            assert_eq!(borsh::to_vec(&encoded).unwrap().len(), 42);
        }
    }
}
