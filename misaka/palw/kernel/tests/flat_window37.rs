//! Typed window graph v37 (unverified source checkpoint, 2026-10-15): the selection
//! must read a continuity-bound image; the ordinary step check refuses a substituted
//! KV row, a reset, a skipped or an unshifted window. Authenticated-opening
//! conviction runs on real trace openings in the consolidated validation phase.
#![allow(warnings)]
use misaka_palw_kernel::{
    flat_history8::{HistoryNodeV8, HistorySelectionPlanV8},
    flat_window37::*,
};
use misaka_palw_tir::{DType, Tensor};

fn graph(window: u32) -> WindowGraphV37 {
    let mut g = WindowGraphV37 {
        version: 37,
        program_root: [3; 64],
        nodes_per_position: 4,
        window,
        max_positions: 64,
        appends: vec![WindowAppendV37 {
            history: HistoryNodeV8 { occurrence: 0, node: 2, slot: 2 },
            writer: HistoryNodeV8 { occurrence: 0, node: 1, slot: 1 },
            dtype: DType::I32,
            row_shape: vec![2],
        }],
        selection: HistorySelectionPlanV8 {
            version: 8,
            graph_root: [0; 64],
            source: HistoryNodeV8 { occurrence: 0, node: 2, slot: 2 },
            output: HistoryNodeV8 { occurrence: 0, node: 3, slot: 3 },
            nodes_per_position: 4,
            dtype: DType::I32,
            row_shape: vec![2],
            window,
            selection_cap: 2,
            max_positions: 64,
        },
    };
    g.selection.graph_root = g.graph_root();
    g
}
fn image(rows: &[[i64; 2]]) -> Tensor {
    Tensor::from_i64(DType::I32, &[rows.len(), 2], &rows.concat()).unwrap()
}
fn row(r: [i64; 2]) -> Tensor {
    Tensor::from_i64(DType::I32, &[2], &r).unwrap()
}

#[test]
fn graph_binds_selection_to_a_continuity_bound_history() {
    let g = graph(4);
    g.validate().unwrap();
    assert!(g.scope().is_ok());
    let mut stray = g.clone();
    stray.selection.source = HistoryNodeV8 { occurrence: 0, node: 0, slot: 0 };
    assert!(stray.validate().is_err(), "selection over an image no append builds");
    let mut rebound = g.clone();
    rebound.appends[0].row_shape = vec![3];
    assert!(rebound.validate().is_err(), "outer graph root binds every append");
    let bytes = borsh::to_vec(&g).unwrap();
    assert_eq!(WindowGraphV37::from_bytes(&bytes).unwrap(), g);
}

#[test]
fn step_refuses_substituted_reset_skipped_and_unshifted_histories() {
    let g = graph(2);
    // p = 0: one row, the writer's.
    check_window_step_v37(&g, 0, 0, None, &image(&[[1, 1]]), &row([1, 1])).unwrap();
    assert!(check_window_step_v37(&g, 0, 0, None, &image(&[[9, 9]]), &row([1, 1])).is_err(), "substituted KV row");
    // p = 1: grows to the window.
    check_window_step_v37(&g, 0, 1, Some(&image(&[[1, 1]])), &image(&[[1, 1], [2, 2]]), &row([2, 2])).unwrap();
    assert!(
        check_window_step_v37(&g, 0, 1, Some(&image(&[[1, 1]])), &image(&[[0, 0], [2, 2]]), &row([2, 2])).is_err(),
        "reset older row"
    );
    // p = 2: full window slides by exactly one.
    let prev = image(&[[1, 1], [2, 2]]);
    check_window_step_v37(&g, 0, 2, Some(&prev), &image(&[[2, 2], [3, 3]]), &row([3, 3])).unwrap();
    assert!(check_window_step_v37(&g, 0, 2, Some(&prev), &image(&[[1, 1], [3, 3]]), &row([3, 3])).is_err(), "unshifted");
}

#[test]
fn window_filing_envelope_and_selection_job_are_versioned_and_deterministic() {
    let g = graph(4);
    let mut raw = 37u16.to_le_bytes().to_vec();
    raw.push(3);
    raw.extend_from_slice(&3u32.to_le_bytes());
    raw.extend_from_slice(&[1, 2, 3]);
    assert_eq!(borrowed_window_filing_v37(&raw, 1024), Some((3, &[1u8, 2, 3][..])));
    let mut v25 = raw.clone();
    v25[..2].copy_from_slice(&25u16.to_le_bytes());
    assert!(borrowed_window_filing_v37(&v25, 1024).is_none(), "the frozen v25 envelope is not a window filing");
    raw[2] = 2;
    assert!(borrowed_window_filing_v37(&raw, 1024).is_none(), "only kinds 3/4");
    let a = window_selection_job_v37(&g, [5; 64], 10, [6; 64]);
    assert_eq!(a, window_selection_job_v37(&g, [5; 64], 10, [6; 64]));
    assert_ne!(a.root(), window_selection_job_v37(&g, [7; 64], 10, [6; 64]).root());
    a.check(&g.selection).unwrap();
}

#[test]
fn a_graph_cannot_attach_to_a_program_that_does_not_compute_it() {
    use misaka_palw_tir::{builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let x = b.param("x", DType::I32, &[2], false);
    let mut pre = b.block("pre", vec![]);
    let y = pre.cast(x, DType::I32);
    let pre = pre.finish(&[y]);
    let mut post = b.block("post", vec![misaka_palw_tir::TensorType::fixed(DType::I32, &[2])]);
    let z = post.cast(misaka_palw_tir::Ref::CarryIn(0), DType::I32);
    post.commit(z);
    let post = post.finish(&[]);
    let program = b.finish(pre, vec![], post, 0);
    // The synthetic graph names four nodes per position and windowed history shapes the
    // program never computes: attachment is refused.
    assert!(check_window_graph_program_v37(&graph(4), &program).is_err());
}

/// A valid frozen program can compute a history-shaped tensor at the proposed
/// selection coordinate. That does not make it the selection relation: neither
/// a copy of the scores nor an idx cast proves sorted top-k history ids.
fn history_shaped_selection(dtype: DType) -> (misaka_palw_tir::TirProgramV1, WindowGraphV37) {
    use misaka_palw_tir::{Ref, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let x = b.param("x", DType::I32, &[2], false);
    let state = b.hist_state("history", DType::I32, &[2], 4, false);
    let mut pre = b.block("pre", vec![]);
    let alternative = pre.cast(x, DType::I32);
    pre.commit(alternative);
    let writer = pre.cast(x, DType::I32);
    let history = pre.hist_append(state, writer);
    let fake_selection = pre.cast(history, dtype);
    pre.commit(fake_selection);
    let pre = pre.finish(&[writer]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let output = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(output);
    let post = post.finish(&[]);
    let program = b.finish(pre, vec![], post, 0);
    misaka_palw_tir::validate::validate(&program).unwrap();
    let mut g = graph(4);
    g.nodes_per_position = 5;
    g.selection.nodes_per_position = 5;
    g.program_root = misaka_palw_kernel::public::program_root_v1(&program.encode());
    g.appends[0].writer = HistoryNodeV8 { occurrence: 0, node: 1, slot: 1 };
    g.appends[0].history = HistoryNodeV8 { occurrence: 0, node: 2, slot: 2 };
    g.selection.source = g.appends[0].history;
    g.selection.output = HistoryNodeV8 { occurrence: 0, node: 3, slot: 3 };
    g.selection.graph_root = g.graph_root();
    (program, g)
}

#[test]
fn real_history_with_same_shaped_i32_or_idx_copy_is_not_a_selection_program() {
    for dtype in [DType::I32, DType::Idx] {
        let (program, g) = history_shaped_selection(dtype);
        g.validate().unwrap();
        let error = check_window_graph_program_v37(&g, &program).unwrap_err();
        assert!(error.contains("selection"), "{error}");
    }
}

#[test]
fn a_valid_history_graph_cannot_substitute_a_different_committed_writer() {
    let (program, mut g) = history_shaped_selection(DType::Idx);
    // Another committed value has exactly the writer's dtype/shape, but is not
    // the value supplied to HistAppend. Position-specific coordinates
    // and the actual operand, rather than dtype/shape alone, decide.
    g.appends[0].writer = HistoryNodeV8 { occurrence: 0, node: 0, slot: 0 };
    g.selection.graph_root = g.graph_root();
    g.validate().unwrap();
    let error = check_window_graph_program_v37(&g, &program).unwrap_err();
    assert!(error.contains("actual HistAppend input"), "{error}");
}
