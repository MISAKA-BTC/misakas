//! Independent full-sort oracle for the unchanged v1 TopK semantics.
use misaka_palw_tir::{DType, Prim, Tensor, eval::eval_primitive};
fn oracle(t: &Tensor, axis: usize, k: usize) -> Tensor {
    let mut shape = t.shape.clone();
    shape[axis] = k;
    let suffix: usize = shape[axis + 1..].iter().product();
    let rows = t.data.len() / t.shape[axis];
    let mut data = vec![0; rows * k];
    for row in 0..rows {
        let (before, after) = (row / suffix, row % suffix);
        let at = |j: usize| t.data[(before * t.shape[axis] + j) * suffix + after];
        let mut all: Vec<_> = (0..t.shape[axis]).collect();
        all.sort_by(|a, b| at(*b).cmp(&at(*a)).then(a.cmp(b)));
        let mut ids = all[..k].to_vec();
        ids.sort_unstable();
        for (slot, id) in ids.into_iter().enumerate() {
            data[(before * k + slot) * suffix + after] = id as i128;
        }
    }
    Tensor::new(DType::Idx, shape, data).unwrap()
}
#[test]
fn bounded_selection_matches_independent_full_sort_for_all_axes_ranks_ties_and_extreme_dtypes() {
    for shape in [vec![1], vec![17], vec![3, 7], vec![2, 5, 3], vec![2, 3, 5, 2]] {
        for dtype in DType::ALL {
            let mut data: Vec<_> =
                (0..shape.iter().product()).map(|i| if dtype == DType::Idx { (i % 7) as i128 } else { (i % 7) as i128 - 3 }).collect();
            data[0] = dtype.min_value();
            let last = data.len() - 1;
            data[last] = dtype.max_value();
            let t = Tensor::new(dtype, shape.clone(), data).unwrap();
            for axis in 0..shape.len() {
                for k in 1..=shape[axis] {
                    let wanted = oracle(&t, axis, k);
                    let actual = eval_primitive(
                        &Prim::TopK { axis: axis as u8, k: k as u32 },
                        std::slice::from_ref(&t),
                        DType::Idx,
                        &wanted.shape,
                    )
                    .unwrap();
                    assert_eq!(actual, wanted, "shape {shape:?} axis {axis} k {k} dtype {dtype:?}");
                }
            }
        }
    }
}
#[test]
fn large_axis_small_k_and_equal_scores_preserve_the_lowest_ids() {
    let n = 131_071;
    let data = (0..n).map(|i| ((i * 7919) % 65521) as i128 - 32760).collect();
    let t = Tensor::new(DType::I32, vec![n], data).unwrap();
    for k in [1, 2, 8, 64, 65, 128, 513, 2048, n - 1, n] {
        let wanted = oracle(&t, 0, k);
        assert_eq!(eval_primitive(&Prim::TopK { axis: 0, k: k as u32 }, std::slice::from_ref(&t), DType::Idx, &[k]).unwrap(), wanted);
    }
    let t = Tensor::new(DType::I128, vec![n], vec![i128::MIN; n]).unwrap();
    assert_eq!(eval_primitive(&Prim::TopK { axis: 0, k: 8 }, &[t], DType::Idx, &[8]).unwrap().data, (0..8).collect::<Vec<_>>());
}
