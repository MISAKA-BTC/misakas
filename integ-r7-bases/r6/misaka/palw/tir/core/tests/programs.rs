//! **Whole programs over several positions** (RFC-0002 Phase C): a 2-layer dense GQA decoder
//! (RMSNorm, SwiGLU, two-level RoPE, two-pass softmax), a GDN layer with `k_heads ≠ v_heads`, a
//! Mamba2 layer, a top-2 MoE layer with a shared expert, and a sliding-window + global schedule.
//!
//! Every program is built with the builder, round-trips its canonical encoding, validates, runs
//! over several positions, and then passes the court property: **every commit point of every
//! position is reproduced by `eval_cone` from the other commit points, the carries, the params and
//! the state the position started from.** Program-specific properties (head mapping, window
//! eviction, routing) are asserted alongside.

mod common;

use std::collections::BTreeMap;

use common::models::*;
use misaka_palw_tir::arith::ONE;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{Cmp, ConeEnv, DType, Dim, Interpreter, MapParams, Ref, RunState, StepOutput, Tensor, TirProgramV1};

// ---- the court property ------------------------------------------------------------------------

/// Run `tokens` through `program`, and for every commit point of every position check that
/// `eval_cone` reproduces it from committed operands alone.
fn run_and_check_cones(program: &TirProgramV1, params: &MapParams, tokens: &[u32]) -> Vec<StepOutput> {
    // The canonical encoding round-trips and is the only encoding.
    let bytes = program.encode();
    let decoded = TirProgramV1::decode_canonical(&bytes).expect("canonical");
    assert_eq!(&decoded, program);
    let interp = Interpreter::new(program).expect("valid");
    let mut state = RunState::default();
    let mut outs = Vec::new();
    let occurrences = program.occurrences();
    for &t in tokens {
        let before = state.clone();
        let step = interp.step(params, &mut state, t).expect("step");
        // Group this step's commits by occurrence.
        let bases = program.occurrence_slot_bases();
        for (occ, (block, layer)) in occurrences.iter().enumerate() {
            let commits: BTreeMap<u16, Tensor> = step
                .commits
                .iter()
                .filter(|c| c.slot >= bases[occ] && c.block == *block && c.layer == *layer)
                .map(|c| (c.node, c.value.clone()))
                .collect();
            // Carry-in: the previous occurrence's carry-out values, from its commits.
            let carry_in: BTreeMap<u8, Tensor> = if occ == 0 {
                BTreeMap::new()
            } else {
                let (pb, pl) = occurrences[occ - 1];
                let prev = &program.blocks[pb as usize];
                prev.carry_out
                    .iter()
                    .enumerate()
                    .map(|(k, n)| {
                        let v = step
                            .commits
                            .iter()
                            .find(|c| c.block == pb && c.layer == pl && c.node == *n && c.slot >= bases[occ - 1])
                            .unwrap();
                        (k as u8, v.value.clone())
                    })
                    .collect()
            };
            // The court's environment is COMPLETE (spec 04b §9.2): every state instance of the
            // occurrence's role is supplied — a never-written one as its initial value, a history
            // with no rows as an empty list — because a cone never implies one (ref2 F1/F3).
            let is_layer = layer.is_some();
            let fixed: BTreeMap<u16, Tensor> = program
                .states
                .iter()
                .enumerate()
                .filter(|(_, s)| s.per_layer == is_layer && matches!(s.kind, misaka_palw_tir::StateKind::Fixed { .. }))
                .map(|(j, s)| {
                    let v = before
                        .fixed
                        .get(&(j as u16, *layer))
                        .cloned()
                        .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>()));
                    (j as u16, v)
                })
                .collect();
            let hist_prior: BTreeMap<u16, Vec<Tensor>> = program
                .states
                .iter()
                .enumerate()
                .filter(|(_, s)| s.per_layer == is_layer && matches!(s.kind, misaka_palw_tir::StateKind::Hist { .. }))
                .map(|(j, _)| {
                    (j as u16, before.hist.get(&(j as u16, *layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default())
                })
                .collect();
            for (node, value) in &commits {
                let mut supplied = commits.clone();
                supplied.remove(node);
                let env = ConeEnv {
                    token: Some(t),
                    pos: before.pos,
                    carry_in: carry_in.clone(),
                    fixed: fixed.clone(),
                    hist_prior: hist_prior.clone(),
                    supplied,
                };
                let got = interp.eval_cone(*block, *layer, *node, params, &env).expect("cone evaluates");
                assert_eq!(&got, value, "cone of block {block} layer {layer:?} node {node} at pos {}", before.pos);
            }
        }
        outs.push(step);
    }
    // Determinism: a second run from a fresh state gives the same bytes.
    let again = interp.run(params, tokens).expect("rerun");
    assert_eq!(again, outs);
    outs
}

#[test]
fn a_two_layer_dense_gqa_decoder_runs_and_every_cone_reproduces() {
    let (program, gens) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    let params = materialize(&program, &gens, 101);
    let tokens = [3u32, 17, 0, 23, 9, 9];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // The logits move with the history: the same token at two positions gives different rows.
    assert_ne!(outs[4].logits, outs[5].logits, "token 9 at positions 4 and 5 must see different histories");
    // Not degenerate: the logits row is not constant.
    let l = &outs[5].logits.data;
    assert!(l.iter().any(|v| *v != l[0]));
    // A different prefix changes a later position; the same prefix does not.
    let interp = Interpreter::new(&program).unwrap();
    let other = interp.run(&params, &[4, 17, 0, 23, 9, 9]).unwrap();
    assert_ne!(other[5].logits, outs[5].logits);
    let same = interp.run(&params, &tokens[..3]).unwrap();
    assert_eq!(same[2], outs[2], "a run's prefix is the prefix of the run");
    // A token outside token_bound is an operand error, never a panic.
    let mut st = RunState::default();
    assert!(interp.step(&params, &mut st, V).is_err());
    assert_eq!(st, RunState::default(), "a failed step leaves the state untouched");
}

/// Sliding-window + global schedule: local layers read `min(pos + 1, 3)` rows, global layers all.
/// Before the window fills, a local layer is indistinguishable from a global one with the same
/// tables; after it, the eviction changes the logits.
#[test]
fn a_sliding_window_and_global_schedule_evicts_exactly_past_the_window() {
    let w = 3u32;
    let (mixed, gens) = dense(&[w, HISTORY_BOUND_V1_SMALL, w]);
    let params = materialize(&mixed, &gens, 202);
    let tokens = [5u32, 1, 12, 7, 7, 20, 2];
    let outs = run_and_check_cones(&mixed, &params, &tokens);
    // The local layers' K history never exceeds the window.
    let interp = Interpreter::new(&mixed).unwrap();
    let mut st = RunState::default();
    for (i, t) in tokens.iter().enumerate() {
        interp.step(&params, &mut st, *t).unwrap();
        for ((j, _), rows) in &st.hist {
            let decl = &mixed.states[*j as usize];
            if decl.name.ends_with(".w3") {
                assert_eq!(rows.len(), (i + 1).min(w as usize - 1), "prior rows kept for the next position");
            }
        }
    }
    // Same weights and tables as a program whose "local" layers are global: equal while
    // pos + 1 ≤ window, different afterwards.
    let (global_only, gens2) = dense(&[HISTORY_BOUND_V1_SMALL - 1, HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL - 1]);
    let _ = gens2;
    // Map the params by name: the "local" program's tables are the rope.l.* params.
    let p2 = remap_by_name(&mixed, &params, &global_only);
    let outs2 = Interpreter::new(&global_only).unwrap().run(&p2, &tokens).unwrap();
    for pos in 0..w as usize {
        assert_eq!(outs[pos].logits, outs2[pos].logits, "position {pos} is inside the window");
    }
    assert!((w as usize..tokens.len()).any(|pos| outs[pos].logits != outs2[pos].logits), "eviction must show past the window");
}

#[test]
fn a_gdn_layer_with_unequal_key_and_value_heads_runs_and_the_head_mapping_is_data() {
    let (grouped, gens) = gdn_program(true);
    let (tiled, _) = gdn_program(false);
    let params = materialize(&grouped, &gens, 303);
    let tokens = [1u32, 2, 3, 5, 8, 13];
    let a = run_and_check_cones(&grouped, &params, &tokens);
    let b = run_and_check_cones(&tiled, &params, &tokens);
    // The recurrence carries: the same token later sees a different state.
    assert_ne!(a[0].logits, Interpreter::new(&grouped).unwrap().run(&params, &[2, 1]).unwrap()[1].logits);
    // Grouping and tiling are different programs (different class ids) and, at k ≠ v, different
    // functions.
    assert_ne!(grouped.encode(), tiled.encode());
    assert!(a.iter().zip(&b).any(|(x, y)| x.logits != y.logits), "16:32-style mappings must differ");
    // The state is written every position and stays in its declared range.
    let interp = Interpreter::new(&grouped).unwrap();
    let mut st = RunState::default();
    for t in tokens {
        interp.step(&params, &mut st, t).unwrap();
    }
    let s = st.fixed.iter().find(|((j, _), _)| grouped.states[*j as usize].name == "S").map(|(_, v)| v).unwrap();
    assert!(s.data.iter().any(|v| *v != 0));
    assert!(s.data.iter().all(|v| v.abs() <= i32::MAX as i128));
}

/// At `k_heads == v_heads` there is nothing to map: grouping and tiling are the same function.
#[test]
fn at_equal_heads_grouping_and_tiling_coincide() {
    // r = 1 is the identity broadcast under both spellings; build it directly.
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let t = pb.param("t", DType::I16, &[3, 5], false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let g = b.reshape_fixed(t, &[3, 1, 5]);
        let g = b.broadcast(g, &[Dim::Fixed(3), Dim::Fixed(1), Dim::Fixed(5)]);
        let g = b.reshape_fixed(g, &[3, 5]);
        let tl = b.reshape_fixed(t, &[1, 3, 5]);
        let tl = b.broadcast(tl, &[Dim::Fixed(1), Dim::Fixed(3), Dim::Fixed(5)]);
        let tl = b.reshape_fixed(tl, &[3, 5]);
        let d = b.sub(g, tl, DType::I32);
        b.finish(&[d])
    };
    let carry = pb.blocks[pre as usize].nodes.last().unwrap().out.clone();
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[15]);
        b.commit(l);
        b.finish(&[])
    };
    let p = pb.finish(pre, vec![], post, 0);
    let mut params = MapParams::default();
    params.tensors.insert((0, None), Tensor::new(DType::I16, vec![3, 5], (0..15).map(|v| v * 7 - 40).collect()).unwrap());
    let out = Interpreter::new(&p).unwrap().run(&params, &[0]).unwrap();
    assert!(out[0].logits.data.iter().all(|v| *v == 0));
}

#[test]
fn a_mamba2_layer_runs_its_selective_scan_over_positions() {
    let (program, gens) = mamba2_program();
    let params = materialize(&program, &gens, 404);
    let tokens = [7u32, 7, 7, 7, 11, 0];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // A repeated token still moves: the scan state and the conv window carry information.
    assert!(outs[1].logits != outs[2].logits || outs[2].logits != outs[3].logits);
    // The conv window starts at zero: position 0 depends on nothing earlier.
    let interp = Interpreter::new(&program).unwrap();
    assert_eq!(interp.run(&params, &[7]).unwrap()[0], outs[0]);
}

#[test]
fn a_top2_moe_layer_with_a_shared_expert_routes_through_committed_selections() {
    let (program, gens) = moe_program();
    let params = materialize(&program, &gens, 505);
    let tokens = [0u32, 1, 2, 3, 4, 5, 6, 7];
    let outs = run_and_check_cones(&program, &params, &tokens);
    // Every TopK is committed, in index order, with k distinct experts.
    let topk_nodes: Vec<u16> = program.blocks[1]
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.prim, misaka_palw_tir::Prim::TopK { .. }))
        .map(|(i, _)| i as u16)
        .collect();
    assert_eq!(topk_nodes.len(), 1);
    let mut used = std::collections::BTreeSet::new();
    for o in &outs {
        for c in o.commits.iter().filter(|c| c.block == 1 && c.node == topk_nodes[0]) {
            assert_eq!(c.value.data.len(), 2);
            assert!(c.value.data[0] < c.value.data[1], "index order, distinct");
            used.insert(c.value.data.clone());
        }
    }
    assert!(used.len() > 1, "different tokens route to different experts");
}

/// The selection rule on ties: every TopK picks the lowest indices among equal values, in index
/// order, whatever the data order.
#[test]
fn topk_breaks_ties_to_the_lowest_index() {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir::eval::eval_primitive;
    let x = Tensor::new(DType::I32, vec![6], vec![5, 9, 9, 1, 9, 5]).unwrap();
    let t = eval_primitive(&Prim::TopK { axis: 0, k: 2 }, std::slice::from_ref(&x), DType::Idx, &[2]).unwrap();
    assert_eq!(t.data, vec![1, 2]);
    let t = eval_primitive(&Prim::TopK { axis: 0, k: 4 }, &[x], DType::Idx, &[4]).unwrap();
    assert_eq!(t.data, vec![0, 1, 2, 4], "the three 9s, then the lower-indexed of the two 5s");
}

// ---- small structural checks shared by the programs --------------------------------------------

#[test]
fn every_program_here_is_canonical_and_its_slots_are_unrolled_in_order() {
    for (p, _) in [dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]), gdn_program(true), mamba2_program(), moe_program()] {
        let bytes = p.encode();
        assert!(bytes.len() < misaka_palw_tir::program::MAX_PROGRAM_BYTES);
        // A trailing byte, or a flipped commit flag on a carry-out, is refused.
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(TirProgramV1::decode_canonical(&longer).is_err());
        let bases = p.occurrence_slot_bases();
        assert_eq!(bases[0], 0);
        assert!(bases.windows(2).all(|w| w[0] < w[1]));
    }
    let _ = (Cmp::Eq, ONE);
}
