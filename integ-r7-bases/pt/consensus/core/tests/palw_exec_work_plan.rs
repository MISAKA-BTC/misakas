//! RFC8 WORK acceptance reference: independent position walk, partition conservation and job scope.
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1};
use kaspa_consensus_core::palw_exec_work_plan_v1::{
    PalwExecWorkCostV1, PalwExecWorkPlanErrorV1, palw_exec_work_plan_from_boundaries_v1, palw_exec_work_plan_v1,
    palw_exec_work_slice_scope_v1,
};
use kaspa_consensus_core::palw_tir_work_v1::{
    PalwTirWorkKindV1, palw_tir_canonical_work_v1, palw_tir_node_work_v1, palw_tir_work_kinds_v1,
};
use misaka_palw_tir::TirProgramV1;
use std::{ops::Range, path::PathBuf};

fn programs() -> Vec<(String, TirProgramV1)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs");
    let mut files: Vec<_> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let hex = value["program_borsh_hex"].as_str().unwrap();
            let bytes: Vec<_> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            (value["name"].as_str().unwrap().into(), TirProgramV1::decode_canonical(&bytes).unwrap())
        })
        .collect()
}

// No affine shape or plan helper: enumerate the independently validated program's actual positions.
fn walk(program: &TirProgramV1, prefill: u32, positions: Range<u32>) -> PalwCanonicalWorkVectorV1 {
    let info = misaka_palw_tir::validate::validate(program).unwrap();
    let occurrences = program.occurrences();
    let mut v = PalwCanonicalWorkVectorV1::default();
    for position in positions {
        for (index, (block, _)) in occurrences.iter().enumerate() {
            if index == occurrences.len() - 1 && position + 1 < prefill {
                continue;
            }
            let bi = *block as usize;
            let h = info.blocks[bi].window.map(|w| (position as u64 + 1).min(w as u64)).unwrap_or(1);
            for (node, kind) in palw_tir_work_kinds_v1(program, bi).iter().enumerate() {
                let [mac, weight, read, write] = palw_tir_node_work_v1(program, bi, node, *kind, h);
                match kind {
                    PalwTirWorkKindV1::DenseMatmul => v.dense_matmul += mac,
                    PalwTirWorkKindV1::RoutedExpertMatmul => v.routed_expert_matmul += mac,
                    PalwTirWorkKindV1::Normalization => v.normalization += mac,
                    PalwTirWorkKindV1::Recurrence => v.recurrence += mac,
                    PalwTirWorkKindV1::Attention if position < prefill => v.attention_prefill += mac,
                    PalwTirWorkKindV1::Attention => v.attention_decode += mac,
                    PalwTirWorkKindV1::Other => v.other_verified_ops += mac,
                }
                v.weight_traffic_bytes += weight;
                v.kv_read_bytes += read;
                v.kv_write_bytes += write;
            }
        }
    }
    v
}

#[test]
fn every_corpus_plan_cell_matches_an_independent_absolute_position_walk() {
    let corpus = programs();
    assert_eq!(corpus.len(), 7, "all checked-in architectures must participate");
    for (name, program) in corpus {
        for prefill in [1, 5, 19] {
            for prefix in [1, 3] {
                let ends = [prefix + 1, prefix + 3, prefix + 6];
                let plan = palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, 100, prefill, prefix, &ends).unwrap();
                assert_eq!(plan.prefix(), walk(&program, prefill, 0..prefill + prefix - 1), "{name}: prefix");
                for cell in plan.slices() {
                    assert_eq!(cell.vector(), walk(&program, prefill, cell.positions()), "{name}: suffix");
                }
                assert_eq!(plan.whole(), walk(&program, prefill, 0..prefill + prefix + 5), "{name}: whole");
                plan.check_declared_work(plan.total_work(), plan.boundaries()).unwrap();
            }
        }
    }
}

#[test]
fn fine_and_coarse_partitions_preserve_the_full_vector_and_credit_in_each_existing_cost_version() {
    for (name, program) in programs() {
        for cost in [PalwExecWorkCostV1::AllNodesV1, PalwExecWorkCostV1::MinSelectArmsV2] {
            let coarse = palw_exec_work_plan_v1(&program, cost, 100, 19, 2, &[8]).unwrap();
            let fine = palw_exec_work_plan_v1(&program, cost, 100, 19, 2, &[3, 4, 5, 6, 7, 8]).unwrap();
            assert_eq!(coarse.cost(), fine.cost());
            assert_eq!(coarse.prefix(), fine.prefix(), "{name}");
            assert_eq!(coarse.whole(), fine.whole(), "{name}");
            assert_eq!(coarse.total_work(), fine.total_work(), "{name}");
            assert_eq!(coarse.boundaries()[0], fine.boundaries()[0], "{name}");
        }
    }
}

#[test]
fn replaying_the_prompt_is_physical_overhead_and_cannot_inflate_credited_suffix_work() {
    for (name, program) in programs() {
        let plan = palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, 100, 5, 1, &[2, 3]).unwrap();
        let cell = &plan.slices()[0];
        assert_eq!(cell.positions(), 5..6, "the prefix's last emitted token is consumed first");
        assert_eq!((cell.prompt_tokens(), cell.generated_tokens()), (6, 1));
        let replay = palw_tir_canonical_work_v1(&program, &PalwCanonicalExecutionFactsV1::uncached(6, 1)).unwrap();
        assert!(replay.arithmetic_mac_eq() > cell.vector().arithmetic_mac_eq(), "{name}: rebuilding history costs extra");
        assert_eq!(plan.boundaries()[1] - plan.boundaries()[0], cell.vector().arithmetic_mac_eq() as u64, "{name}");
        plan.check_slice_job(0, 6, 1).unwrap();
    }
}

#[test]
fn a_tiny_run_or_free_declared_work_cannot_authorize_a_large_slice() {
    let (_, program) = programs().remove(0);
    let plan = palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, 100, 5, 1, &[7]).unwrap();
    assert_eq!(plan.check_slice_job(0, 6, 1), Err(PalwExecWorkPlanErrorV1::SliceShape));
    assert_eq!(plan.check_slice_job(0, 5, 6), Err(PalwExecWorkPlanErrorV1::SliceShape));
    assert_eq!(plan.check_slice_job(1, 6, 6), Err(PalwExecWorkPlanErrorV1::SliceShape));
    plan.check_slice_job(0, 6, 6).unwrap();
    let mut inflated = plan.boundaries().to_vec();
    inflated[0] += 1;
    assert_eq!(plan.check_declared_work(plan.total_work(), &inflated), Err(PalwExecWorkPlanErrorV1::DeclaredWork));
    assert_eq!(plan.check_declared_work(plan.total_work() + 1, plan.boundaries()), Err(PalwExecWorkPlanErrorV1::DeclaredWork));
}

#[test]
fn malformed_partitions_and_out_of_class_or_overflowing_positions_are_refused() {
    let (_, program) = programs().remove(0);
    let derive = |max, prompt, prefix, ends: &[u32]| {
        palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, max, prompt, prefix, ends)
    };
    assert_eq!(derive(100, 0, 1, &[2]), Err(PalwExecWorkPlanErrorV1::EmptyPrefix));
    assert_eq!(derive(100, 1, 0, &[2]), Err(PalwExecWorkPlanErrorV1::EmptyPrefix));
    for ends in [&[][..], &[1][..], &[2, 2][..], &[3, 2][..]] {
        assert_eq!(derive(100, 1, 1, ends), Err(PalwExecWorkPlanErrorV1::SuffixEnds));
    }
    let many: Vec<u32> = (2..=130).collect();
    assert_eq!(derive(1000, 1, 1, &many), Err(PalwExecWorkPlanErrorV1::SuffixEnds));
    assert_eq!(derive(5, 5, 1, &[2]), Err(PalwExecWorkPlanErrorV1::Positions));
    assert_eq!(derive(u32::MAX, u32::MAX, 1, &[2]), Err(PalwExecWorkPlanErrorV1::Positions));
    derive(6, 5, 1, &[2]).unwrap();
}

#[test]
fn large_execution_cost_and_out_of_artifact_positions_are_bounded_without_a_position_loop() {
    let mut program = misaka_palw_tir_sketch::fixture::wide128_v1(23).program;
    assert_eq!(
        palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, u32::MAX, 1, 1, &[u32::MAX]),
        Err(PalwExecWorkPlanErrorV1::Positions)
    );
    // A normal-form, 1024-layer wide-matmul IR with a valid 2^21 history bound. Its useful
    // arithmetic exceeds 2^48 even though its positions fit. No weights or traces are allocated.
    program.history_bound = misaka_palw_tir::program::HISTORY_BOUND_V1_HELD;
    program.schedule.layers = vec![program.schedule.layers[0]; 1024];
    for param in &mut program.params {
        for dim in &mut param.shape {
            if *dim == 16 {
                *dim = 512;
            }
        }
    }
    for block in &mut program.blocks {
        let types = block.carry_in.iter_mut().chain(block.nodes.iter_mut().map(|node| &mut node.out));
        for tensor in types {
            for dim in &mut tensor.shape {
                if *dim == misaka_palw_tir::Dim::Fixed(16) {
                    *dim = misaka_palw_tir::Dim::Fixed(512);
                }
            }
        }
    }
    misaka_palw_tir::validate::validate(&program).expect("a valid large program, not malformed IR");
    assert_eq!(
        palw_exec_work_plan_v1(&program, PalwExecWorkCostV1::AllNodesV1, program.history_bound, 1, 1, &[program.history_bound]),
        Err(PalwExecWorkPlanErrorV1::WorkBound)
    );
}

#[test]
fn signed_cost_boundaries_authenticate_the_exact_decode_plan_for_every_architecture_and_cost_policy() {
    for (name, program) in programs() {
        for cost in [PalwExecWorkCostV1::AllNodesV1, PalwExecWorkCostV1::MinSelectArmsV2] {
            let plan = palw_exec_work_plan_v1(&program, cost, 100, 5, 1, &[3, 5, 7]).unwrap();
            assert_eq!(palw_exec_work_plan_from_boundaries_v1(&program, cost, 100, 5, 1, plan.boundaries()).unwrap(), plan, "{name}");
            let mut fabricated = plan.boundaries().to_vec();
            fabricated[1] += 1;
            assert_eq!(
                palw_exec_work_plan_from_boundaries_v1(&program, cost, 100, 5, 1, &fabricated),
                Err(PalwExecWorkPlanErrorV1::DeclaredWork),
                "{name}: a MAC coordinate between real execution ends is not a plan"
            );
            for (index, cell) in plan.slices().iter().enumerate() {
                let boundaries = [plan.boundaries()[index], plan.boundaries()[index + 1]];
                palw_exec_work_slice_scope_v1(&program, cost, 100, 5, 1, boundaries, cell.prompt_tokens() as usize, 2).unwrap();
                assert!(
                    palw_exec_work_slice_scope_v1(&program, cost, 100, 5, 1, boundaries, cell.prompt_tokens() as usize, 1).is_err()
                );
                assert!(
                    palw_exec_work_slice_scope_v1(&program, cost, 100, 5, 1, boundaries, cell.prompt_tokens() as usize + 1, 2)
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn maximum_structural_size_context_and_slice_count_have_a_bounded_cost_inverse() {
    use misaka_palw_tir::program::{Block, MAX_BLOCKS, MAX_LAYERS, MAX_NODES_PER_BLOCK, Node, Ref, Schedule};
    use misaka_palw_tir::{DType, Prim, TensorType};
    let mut program = misaka_palw_tir_sketch::fixture::dense_moe_v1(1).program;
    program.params.clear();
    program.consts.clear();
    program.states.clear();
    program.history_bound = misaka_palw_tir::program::HISTORY_BOUND_V1_HELD;
    program.blocks = (0..MAX_BLOCKS)
        .map(|index| Block {
            name: format!("b{index}"),
            carry_in: vec![],
            carry_out: vec![],
            nodes: vec![
                Node { prim: Prim::Cast, inputs: vec![Ref::Input(0)], out: TensorType::scalar(DType::Idx), commit: true };
                MAX_NODES_PER_BLOCK
            ],
        })
        .collect();
    program.schedule = Schedule {
        pre: 0,
        post: (MAX_BLOCKS - 1) as u8,
        layers: (0..MAX_LAYERS).map(|index| (1 + index % (MAX_BLOCKS - 2)) as u8).collect(),
    };
    program.logits = 0;
    let program = TirProgramV1::decode_canonical(&program.encode()).expect("all structural caps fit the byte cap");
    let slice_count = kaspa_consensus_core::palw_exec_v2::PALW_EXEC_V2_MAX_SLICES_PER_ROOT;
    let ends: Vec<_> = (1..=slice_count)
        .map(|i| 1 + i * ((program.history_bound - 1) / slice_count) + i * ((program.history_bound - 1) % slice_count) / slice_count)
        .collect();
    let per_position = (MAX_LAYERS as u64 + 2) * MAX_NODES_PER_BLOCK as u64;
    let boundaries: Vec<_> = std::iter::once(per_position).chain(ends.iter().map(|g| per_position * u64::from(*g))).collect();
    for cost in [PalwExecWorkCostV1::AllNodesV1, PalwExecWorkCostV1::MinSelectArmsV2] {
        let started = std::time::Instant::now();
        let plan = palw_exec_work_plan_from_boundaries_v1(&program, cost, program.history_bound, 1, 1, &boundaries).unwrap();
        println!("maximum-size cost inverse {cost:?}: {:?}", started.elapsed());
        assert_eq!(plan.total_work(), per_position * u64::from(program.history_bound));
        assert_eq!(plan.slices().len(), slice_count as usize);
        assert_eq!(plan.slices().last().unwrap().positions().end, program.history_bound);
    }
}
