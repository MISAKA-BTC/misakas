//! Candidate pricing only. No fixture grants node activation, a new court or
//! real-checkpoint task/fidelity/Final/population coverage.
#![allow(warnings)]
use misaka_palw_kernel::{flat_point9::point_relation_scope_v9, flat_price15::*, merkle4::FlatLayoutV4};
use misaka_palw_tir::{DType, Ref, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};

#[test]
fn every_contiguous_row_and_merged_tail_is_within_the_new_bound() {
    for rows in [1usize, 2, 3, 7, 14] {
        for width in [1usize, 2, 31, 63, 64, 65, 127, 128, 129, 511, 512, 513, 8192, 32768] {
            let shape = [rows, width];
            let Some(bound) = contiguous_reduction_leaf_bound_v15(&shape) else { continue };
            let layout = FlatLayoutV4::of(&[rows as u64, width as u64]).unwrap();
            for row in 0..rows {
                let first = layout.leaf_of((row * width) as u64).unwrap();
                let last = layout.leaf_of(((row + 1) * width - 1) as u64).unwrap();
                assert!(last - first < bound, "{shape:?} row{row}:{}>{bound}", last - first + 1);
            }
        }
    }
}
fn reduction(rows: u32, width: u32, axis: u8) -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(if axis == 1 { rows } else { width }, HISTORY_BOUND_V1_SMALL);
    let source = b.param("source", DType::I32, &[rows, width], false);
    let mut pre = b.block("pre", vec![]);
    let x = pre.cast(source, DType::I32);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![misaka_palw_tir::TensorType::fixed(DType::I32, &[rows, width])]);
    let x = post.cast(Ref::CarryIn(0), DType::I64);
    let x = post.reduce_max(x, axis as usize);
    let x = post.cast(x, DType::I32);
    post.commit(x);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 2)
}
#[test]
fn frozen_v9_is_preserved_and_candidate_contiguous_price_has_a_new_root() {
    let p = reduction(14, 32768, 1);
    assert!(point_relation_scope_v9(&p, 1, None, 0, 1, 1).is_err());
    let candidate = point_relation_price_v15(&p, 1, None, 0, 1, 1).unwrap();
    assert_eq!(candidate.version, 15);
    assert!(candidate.operand_leaves[0] <= 513);
    assert_eq!(candidate.operand_reads[0], 32768);
    assert_ne!(candidate.court_id, misaka_palw_kernel::flat_point9::ordinary_point_court_id_v9());
    assert!(candidate.filing_bytes <= misaka_palw_kernel::flat_court4::FLAT_FAULT_MAX_BYTES_V4);
}
#[test]
fn strided_middle_axis_and_oversized_contiguous_lines_do_not_get_a_waiver() {
    let p = reduction(8192, 128, 0);
    assert!(point_relation_scope_v9(&p, 1, None, 0, 1, 1).is_err());
    assert!(point_relation_price_v15(&p, 1, None, 0, 1, 1).is_err());
    let p = reduction(14, 131072, 1);
    assert!(point_relation_price_v15(&p, 1, None, 0, 1, 1).is_err());
}
#[test]
fn unchanged_nonreduction_prices_keep_identical_bytes_work_and_scratch() {
    let p = reduction(14, 128, 1);
    let old = point_relation_scope_v9(&p, 1, None, 0, 0, 0).unwrap();
    let new = point_relation_price_v15(&p, 1, None, 0, 0, 0).unwrap();
    assert_eq!(
        (old.filing_bytes, old.work, old.scratch_bytes, old.operand_leaves),
        (new.filing_bytes, new.work, new.scratch_bytes, new.operand_leaves)
    );
}
#[test]
fn diagnostic_over_cap_cost_does_not_relax_the_bounded_price() {
    let p = reduction(14, 131072, 1);
    let quotes = all_point_relation_quotes_v15(&p, 1, None, 0).unwrap();
    let quote = quotes.into_iter().find(|(s, n, _)| *s == 1 && *n == 1).unwrap().2.unwrap();
    assert_eq!(quote.operand_reads, [131072]);
    assert_eq!(quote.operand_leaves, [2049]);
    assert!(!price_within_frozen_court_caps_v15(&quote));
    assert!(quote.filing_bytes > misaka_palw_kernel::flat_court4::FLAT_FAULT_MAX_BYTES_V4);
    assert!(point_relation_price_v15(&p, 1, None, 0, 1, 1).is_err());
    assert!(
        all_point_relation_prices_v15(&p, 1, None, 0).unwrap().into_iter().find(|(s, n, _)| *s == 1 && *n == 1).unwrap().2.is_err()
    );
}
