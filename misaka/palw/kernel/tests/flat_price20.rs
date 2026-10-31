#![allow(warnings)]
use misaka_palw_kernel::{flat_price15::all_point_relation_quotes_v15, flat_price20::*, merkle4::FlatLayoutV4};
use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
#[test]
fn scalar_columns_cover_every_flat_line_and_merged_tail_without_covering_wide_columns() {
    for groups in [1usize, 2, 7, 14] {
        for k in [1usize, 2, 31, 63, 64, 65, 127, 128, 129, 8192, 32768] {
            let shape = [groups, k, 1];
            let Some(bound) = scalar_column_leaf_bound_v20(&shape) else {
                assert!(groups * k < 64);
                continue;
            };
            let flat = FlatLayoutV4::of(&[groups as u64, k as u64, 1]).unwrap();
            for g in 0..groups {
                let a = flat.leaf_of((g * k) as u64).unwrap();
                let b = flat.leaf_of(((g + 1) * k - 1) as u64).unwrap();
                assert!(b - a < bound);
            }
            assert!(scalar_column_leaf_bound_v20(&[groups, k, 2]).is_none());
        }
    }
}
fn history_program() -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(6, HISTORY_BOUND_V1_HELD);
    let a = b.param("q", DType::I16, &[2], false);
    let v = b.param("v", DType::I16, &[3], false);
    let hs = b.hist_state("qh", DType::I16, &[2], 32768, false);
    let vs = b.hist_state("vh", DType::I16, &[3], 32768, false);
    let mut pre = b.block("pre", vec![]);
    let a = pre.cast(a, DType::I16);
    let v = pre.cast(v, DType::I16);
    let ah = pre.hist_append(hs, a);
    let ah = pre.transpose(ah, &[1, 0]);
    let bh = pre.hist_append(vs, v);
    let mm = pre.matmul(ah, bh, DType::I64);
    let x = pre.cast(mm, DType::I32);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2, 3])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 0)
}
#[test]
fn new_price_id_reduces_only_scalar_column_charge_and_still_refuses_large_combined_proof() {
    let original = history_program();
    let pass = misaka_palw_tir_lower::layout_pass15::transpose_history_contractions_v15(&original).unwrap();
    for positions in [8192, 32768] {
        let old = all_point_relation_quotes_v15(&pass.program, positions, None, positions - 1).unwrap();
        let new = all_point_relation_quotes_v20(&pass.program, positions, None, positions - 1).unwrap();
        let mut changed = 0;
        for ((s, n, a), (ss, nn, b)) in old.into_iter().zip(new) {
            assert_eq!((s, n), (ss, nn));
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(b.version, 20);
            assert_ne!(a.court_id, b.court_id);
            assert_eq!(a.operand_reads, b.operand_reads);
            if a.operand_leaves != b.operand_leaves {
                changed += 1;
                assert_eq!(b.operand_leaves, vec![positions as u64 / 64 + 1; 2]);
                assert!(b.filing_bytes < a.filing_bytes);
                assert!(b.work < a.work);
                assert!(b.scratch_bytes < a.scratch_bytes);
                assert_eq!(b.within_frozen_court_caps(), positions == 8192);
            } else {
                assert_eq!((a.filing_bytes, a.work, a.scratch_bytes), (b.filing_bytes, b.work, b.scratch_bytes));
            }
        }
        assert_eq!(changed, 1);
    }
}
