//! Typed window graph v37 (unverified source checkpoint, 2026-10-15): the selection
//! must read a continuity-bound image; the ordinary step check refuses a substituted
//! KV row, a reset, a skipped or an unshifted window. Authenticated-opening
//! conviction runs on real trace openings in the consolidated validation phase.
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
