//! Pure expanded coverage geometry over unchanged, sparse original TIR v1.
//! No material decoder, arithmetic acceptance, signature or credit is exercised.
#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1, builder::ProgramBuilder, demand::history_length_v1, prim::Rounding};
use misaka_palw_tir_exec::node::positive_check::expanded_plan::{
    ExpandedPlan, ExpandedPlanError, ExpandedPlanLimits, ExpandedRelationKind,
};
use std::collections::{BTreeMap, BTreeSet};

fn limits() -> ExpandedPlanLimits {
    ExpandedPlanLimits {
        max_positions: 32,
        max_plan_bytes: 1 << 24,
        max_geometry_visits: 1 << 24,
        max_legacy_leaves: 1 << 20,
        max_relations: 1 << 20,
        max_target_lanes: 1 << 30,
        max_peak_target_bytes: 1 << 24,
        uncommitted_tile_lanes: 3,
        max_elements_per_relation: 1 << 20,
        max_terms_per_relation: 1 << 24,
        max_total_elements: 1 << 60,
        max_total_terms: 1 << 60,
    }
}
struct Fixture {
    class: PalwTirClassV1,
    artifact: Hash64,
    job: PalwJobContextV2,
}
fn fixture(program: TirProgramV1, prompt: Vec<u32>, decode: u32) -> Fixture {
    let program = node_common::tiled(program);
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        layout: node_common::layout(&program, 3, 2, 2, 64),
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    // Geometry needs only the rooted identity; it never reads parameter values.
    let artifact = Hash64::from_u64_word(29);
    let mut job =
        node_common::job(&program, prompt.len() as u32, decode, class.class_id(&artifact), &prompt, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    Fixture { class, artifact, job }
}
fn wide_fixture() -> Fixture {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 2], false);
    let scale = p.konst(DType::I64, &[2], &[1 << 32, 1 << 32]);
    let mut pre = p.block("pre", vec![]);
    let x = pre.gather(embed, Ref::Input(0), 0, 0);
    let x = pre.cast(x, DType::I64);
    let wide64 = pre.mul(x, scale, DType::I64);
    let wide128 = pre.mul(wide64, wide64, DType::I128);
    // Lane1 remains part of both original operations although final output
    // uses only lane0. For an input row [2,3], wide128 is [2^66,9*2^64].
    let first = pre.slice(wide128, 0, 0, 1);
    let first = pre.shr(first, 66, Rounding::Floor, DType::I128);
    let narrowed = pre.clamp(first, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let pre = pre.finish(&[narrowed]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    fixture(p.finish(pre, vec![], post, logits), vec![0, 1], 2)
}
fn window_fixture() -> Fixture {
    let (_, mut program, _) = node_common::h_program();
    let state = program.states.len() as u16;
    program.states.push(misaka_palw_tir::program::StateDecl {
        name: "last_embedding".into(),
        kind: misaka_palw_tir::program::StateKind::Fixed { lo: -128, hi: 127 },
        dtype: DType::I8,
        shape: vec![4],
        per_layer: false,
    });
    let pre = &mut program.blocks[program.schedule.pre as usize];
    pre.nodes.push(misaka_palw_tir::program::Node {
        prim: misaka_palw_tir::Prim::StateWrite { state },
        inputs: vec![Ref::Node(0)],
        out: TensorType::fixed(DType::I8, &[4]),
        commit: false,
    });
    fixture(program, vec![3, 8, 13], 6) // Eight positions cross the five-row window.
}

// Independent enumeration of original node lanes. It does not call the new
// plan's relation lookup, nor use legacy committed leaves as a proxy for nodes.
fn assert_complete_original_geometry(f: &Fixture, cap: ExpandedPlanLimits) {
    let before = f.class.clone();
    let original_program = f.class.program.clone();
    let space = PalwTirStepSpaceV1::new(&f.class).unwrap();
    let program = f.class.decode_program().unwrap();
    let positions = f.job.declared_prefill_tokens + f.job.exact_decode_tokens - 1;
    let occurrences = program.occurrences();
    let mut expected = BTreeMap::new();
    let mut node_lanes = 0;
    let mut node_instances = 0;
    let mut expected_extra_relations = 0;
    for pos in 0..positions {
        for (occurrence, &(block, layer)) in occurrences.iter().enumerate() {
            if block == program.schedule.post && pos + 1 < f.job.declared_prefill_tokens {
                continue;
            }
            let h = history_length_v1(&space.info, block, pos).unwrap();
            for (node, value) in program.blocks[block as usize].nodes.iter().enumerate() {
                let shape = value.out.resolve(h);
                let count = shape.iter().product::<usize>();
                node_instances += 1;
                node_lanes += count as u64;
                if value.commit {
                    continue;
                }
                expected_extra_relations += (count as u64).div_ceil(cap.uncommitted_tile_lanes as u64);
                for lane in 0..count {
                    assert!(
                        expected
                            .insert((pos, occurrence as u16, node as u16, lane as u64), (block, layer, value.out.dtype, shape.clone()))
                            .is_none()
                    );
                }
            }
        }
    }
    let old_count = space.leaf_count_capped(&f.job, cap.max_legacy_leaves).unwrap();
    let old_leaves: Vec<_> = (0..old_count).map(|i| space.leaf_at(&f.job, i).unwrap()).collect();
    let state_lanes =
        old_leaves.iter().filter(|l| matches!(l.kind, PalwTirLeafKindV1::State { .. })).map(|l| l.value_count as u64).sum::<u64>();
    let hist_lanes =
        old_leaves.iter().filter(|l| matches!(l.kind, PalwTirLeafKindV1::HistTile { .. })).map(|l| l.value_count as u64).sum::<u64>();
    let plan = ExpandedPlan::derive(&f.class, f.artifact, &f.job, cap).unwrap();
    assert_eq!(f.class, before);
    assert_eq!(f.class.program, original_program, "do not rewrite or add commitments to the registered program");
    let summary = plan.summary();
    assert_eq!(summary.positions, positions);
    assert!(summary.plan_metadata_bytes > 0 && summary.plan_metadata_bytes <= cap.max_plan_bytes);
    assert_eq!(summary.node_instances, node_instances);
    assert_eq!(summary.node_lanes, node_lanes);
    assert_eq!(summary.state_lanes, state_lanes);
    assert_eq!(summary.hist_lanes, hist_lanes);
    assert_eq!(summary.target_lanes, node_lanes + state_lanes + hist_lanes);
    assert_eq!(summary.legacy_relations, old_count);
    assert_eq!(summary.uncommitted_relations, expected_extra_relations);
    assert_eq!(summary.relations, old_count + expected_extra_relations);
    assert_eq!(summary.reserved_elements, summary.relations * cap.max_elements_per_relation);
    assert_eq!(summary.reserved_terms, summary.relations * cap.max_terms_per_relation);
    assert!(summary.geometry_visits <= summary.geometry_visit_bound);
    assert!(summary.geometry_visit_bound <= cap.max_geometry_visits);
    let mut seen_old = Vec::new();
    let mut frontier = 0;
    let mut previous_order = None;
    let mut max_lanes = 0;
    for (index, relation) in plan.iter().enumerate() {
        assert_eq!(relation.index, index as u64);
        assert_eq!(plan.relation_at(index as u64), Some(relation.clone()));
        assert_eq!(relation.legacy_frontier, frontier);
        let order = match relation.kind {
            ExpandedRelationKind::Legacy(leaf) => {
                assert_eq!(leaf.index, frontier);
                max_lanes = max_lanes.max(leaf.value_count as u64);
                seen_old.push(leaf);
                frontier += 1;
                match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                        (leaf.position, occurrence, node as u32, first_element)
                    }
                    PalwTirLeafKindV1::State { .. } => (leaf.position, occurrences.len() as u32, 0, leaf.index),
                    PalwTirLeafKindV1::HistTile { .. } => (leaf.position, occurrences.len() as u32 + 1, 0, leaf.index),
                }
            }
            ExpandedRelationKind::RecomputeNode { ctx, block, layer, node, dtype, shape, first_lane, lane_count } => {
                assert!(lane_count > 0 && lane_count <= cap.uncommitted_tile_lanes);
                max_lanes = max_lanes.max(lane_count as u64);
                assert!(!program.blocks[block as usize].nodes[node as usize].commit);
                for lane in first_lane..first_lane + lane_count as u64 {
                    assert_eq!(
                        expected.remove(&(ctx.pos, ctx.occurrence, node, lane)),
                        Some((block, layer, dtype, shape.clone())),
                        "a missing, duplicate or wrongly located original tensor lane"
                    );
                }
                (ctx.pos, ctx.occurrence as u32, node as u32, first_lane)
            }
        };
        if let Some(previous) = previous_order {
            assert!(previous < order, "original position/occurrence/node/lane order");
        }
        previous_order = Some(order);
    }
    assert!(expected.is_empty(), "even output-unused original lanes are mandatory");
    assert_eq!(seen_old, old_leaves, "legacy leaf identities/order must remain exact");
    assert_eq!(summary.peak_target_bytes, max_lanes * 16, "evaluation vectors store all dtypes as i128");
    assert_eq!(plan.relation_at(summary.relations), None);
    assert_eq!(plan.relation_at(u64::MAX), None);
}

#[test]
fn original_sparse_i64_i128_and_output_unused_lanes_are_all_enumerated() {
    let f = wide_fixture();
    // Pure expanded geometry must not relax the existing derivation's ingress.
    use misaka_palw_tir_exec::node::positive_check::{CheckError, CheckLimits, CoveragePlan};
    let old_limits = CheckLimits {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 40,
        max_elements_per_leaf: 1 << 20,
        max_terms_per_leaf: 1 << 24,
        max_total_elements: 1 << 60,
        max_total_terms: 1 << 60,
    };
    assert_eq!(
        CoveragePlan::derive(&f.class, f.artifact, &f.job, PalwPromptIdsFormV1::Flat, old_limits).unwrap_err(),
        CheckError::UnsupportedProfile
    );
    let program = f.class.decode_program().unwrap();
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.out.dtype == DType::I64 && !n.commit));
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.out.dtype == DType::I128 && !n.commit));
    for tile in [1, 3, 7] {
        let mut cap = limits();
        cap.uncommitted_tile_lanes = tile;
        assert_complete_original_geometry(&f, cap);
        let plan = ExpandedPlan::derive(&f.class, f.artifact, &f.job, cap).unwrap();
        let hidden_wide_lanes = plan
            .iter()
            .filter(|relation| {
                matches!(&relation.kind,
            ExpandedRelationKind::RecomputeNode { dtype: DType::I128, shape, first_lane, lane_count, .. }
                if shape == &vec![2] && *first_lane <= 1 && *first_lane + *lane_count as u64 > 1)
            })
            .count();
        assert_eq!(hidden_wide_lanes, 3, "the unused square lane exists at all three pre positions");
    }
}

#[test]
fn dynamic_history_reused_layers_and_fixed_history_leaves_preserve_all_geometry() {
    let f = window_fixture();
    assert_complete_original_geometry(&f, limits());
    let plan = ExpandedPlan::derive(&f.class, f.artifact, &f.job, limits()).unwrap();
    let mut history_shapes = BTreeMap::new();
    let mut layers = BTreeSet::new();
    for relation in plan.iter() {
        if let ExpandedRelationKind::RecomputeNode { ctx, layer: Some(layer), node: 0, shape, .. } = relation.kind {
            layers.insert(layer);
            history_shapes.insert((ctx.pos, layer), shape);
        }
    }
    assert_eq!(layers, BTreeSet::from([0, 1, 2]));
    for layer in 0..3 {
        for pos in 0..8 {
            assert_eq!(history_shapes[&(pos, layer)], vec![(pos as usize + 1).min(5), 4]);
        }
    }
    assert!(plan.summary().state_lanes > 0 && plan.summary().hist_lanes > 0);
}

#[test]
fn expanded_geometry_rejects_wrong_identity_zero_jobs_and_every_finite_reservation_cap() {
    let f = window_fixture();
    let summary = ExpandedPlan::derive(&f.class, f.artifact, &f.job, limits()).unwrap().summary();
    assert!(matches!(ExpandedPlan::derive(&f.class, Hash64::from_u64_word(999), &f.job, limits()), Err(ExpandedPlanError::Binding)));
    for prefill in [0, u32::MAX] {
        let mut job = f.job.clone();
        job.declared_prefill_tokens = prefill;
        assert!(ExpandedPlan::derive(&f.class, f.artifact, &job, limits()).is_err());
    }
    let mut zero_decode = f.job.clone();
    zero_decode.exact_decode_tokens = 0;
    assert!(ExpandedPlan::derive(&f.class, f.artifact, &zero_decode, limits()).is_err());
    for variant in 0..11 {
        let mut cap = limits();
        match variant {
            0 => cap.max_positions = summary.positions - 1,
            1 => cap.max_geometry_visits = 0,
            2 => cap.max_legacy_leaves = summary.legacy_relations - 1,
            3 => cap.max_relations = summary.relations - 1,
            4 => cap.max_target_lanes = summary.target_lanes - 1,
            5 => cap.max_peak_target_bytes = summary.peak_target_bytes - 1,
            6 => cap.uncommitted_tile_lanes = 0,
            7 => cap.max_total_elements = summary.reserved_elements - 1,
            8 => cap.max_total_terms = summary.reserved_terms - 1,
            9 => cap.max_elements_per_relation = 0,
            _ => cap.max_plan_bytes = summary.plan_metadata_bytes - 1,
        }
        assert!(ExpandedPlan::derive(&f.class, f.artifact, &f.job, cap).is_err(), "cap variant {variant}");
    }
    for terms in [false, true] {
        let mut cap = limits();
        if terms {
            cap.max_terms_per_relation = u64::MAX;
            cap.max_total_terms = u64::MAX;
        } else {
            cap.max_elements_per_relation = u64::MAX;
            cap.max_total_elements = u64::MAX;
        }
        assert!(matches!(ExpandedPlan::derive(&f.class, f.artifact, &f.job, cap), Err(ExpandedPlanError::Overflow)));
    }
}

#[test]
fn original_matmul_and_reduction_work_is_counted_at_every_executed_position() {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 2], false);
    let weight = p.param("weight", DType::I8, &[2, 3], false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[2])]);
    let row = post.reshape_fixed(Ref::CarryIn(0), &[1, 2]);
    let dots = post.matmul(row, weight, DType::I64);
    let sum = post.reduce_sum(dots, 1, DType::I64);
    let narrowed = post.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let logits = post.reshape_fixed(narrowed, &[1]);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let f = fixture(p.finish(pre, vec![], post, logits), vec![0, 1], 2);
    assert_complete_original_geometry(&f, limits());
    let work = ExpandedPlan::derive(&f.class, f.artifact, &f.job, limits()).unwrap().summary().structural_work;
    // P=2,D=2 executes three pre gathers (2 lanes each), and exactly TWO
    // posts. Each post does 1x2 by 2x3 = 6 MACs, plus reshape2, reduction3,
    // clamp1, reshape1 = 7 elementwise operations. No node_cost oracle is used.
    assert_eq!(work.elementwise, 3 * 2 + 2 * 7);
    assert_eq!(work.macs, 2 * 6);
    assert_eq!(work.transcendentals, 0);
    // Gather reads two i8 lanes plus one u32 token; each post reads
    // 2 + (2+6) + 24 + 8 + 4 bytes and writes 2+24+8+4+4 bytes.
    assert_eq!(work.bytes_read, 3 * 6 + 2 * 46);
    assert_eq!(work.bytes_written, 3 * 2 + 2 * 42);
}
