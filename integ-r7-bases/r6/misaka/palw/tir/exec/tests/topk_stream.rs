//! Native strided storage selection, independently checked by a full-sort oracle.
//! Synthetic kernels only; not real-checkpoint, job/native/Final acceptance.
use misaka_palw_tir::{DType, Prim};
use misaka_palw_tir_exec::{
    Buf,
    kernels::{Opd, Scratch, misc::topk},
    layout::{Layout, row_major},
    plan::{Acc, NodePlan},
};
fn oracle(x: &Opd<'_>, axis: usize, k: usize) -> Vec<i128> {
    let shape = x.shape();
    let rm = row_major(shape);
    let suffix = shape[axis + 1..].iter().product::<usize>();
    let rows = x.numel() / shape[axis];
    let mut out = vec![0; rows * k];
    for row in 0..rows {
        let base = (row / suffix) * shape[axis] * suffix + row % suffix;
        let mut ids: Vec<_> = (0..shape[axis]).collect();
        ids.sort_by(|a, b| x.get_linear(base + b * rm[axis]).cmp(&x.get_linear(base + a * rm[axis])).then(a.cmp(b)));
        ids.truncate(k);
        ids.sort_unstable();
        for (j, id) in ids.iter().enumerate() {
            out[(row / suffix) * k * suffix + j * suffix + row % suffix] = *id as i128;
        }
    }
    out
}
fn scratch_lengths(s: &Scratch) -> Vec<(usize, usize)> {
    s.w64.iter().map(|v| (v.len(), v.capacity())).chain(s.w128.iter().map(|v| (v.len(), v.capacity()))).collect()
}
#[test]
fn all_native_widths_axes_and_offset_transpose_slice_broadcast_views_match_full_sort_without_widening() {
    for dtype in DType::ALL {
        for shape in [vec![11], vec![3, 11], vec![2, 11, 3], vec![2, 3, 11, 2], vec![2, 131, 3]] {
            let count: usize = shape.iter().product();
            let data: Vec<_> = (0..count)
                .map(|i| match i % 7 {
                    0 => dtype.min_value(),
                    1 => dtype.max_value(),
                    _ => (i as i128 % 5).clamp(dtype.min_value(), dtype.max_value()),
                })
                .collect();
            let mut padded = vec![dtype.min_value(); 7];
            padded.extend(data);
            let buf = Buf::from_i128s(dtype, &padded);
            let base = Layout::contiguous_at(&shape, 7);
            let perm: Vec<_> = (0..shape.len() as u8).rev().collect();
            let mut views = vec![base, base.transposed(&perm)];
            #[allow(clippy::needless_range_loop)]
            for axis in 0..shape.len() {
                views.push(base.sliced(axis, 1, shape[axis] - 1));
            }
            views.push(base.sliced(0, 0, 1).broadcast_to(&shape));
            for layout in views {
                let input = Opd { data: buf.slice(), layout };
                for axis in 0..layout.rank as usize {
                    for k in 1..=input.shape()[axis] {
                        let node = NodePlan::for_kernel(Prim::TopK { axis: axis as u8, k: k as u32 }, DType::Idx, Acc::Fast128);
                        let mut out = Buf::Idx(vec![]);
                        let mut scratch = Scratch::default();
                        let before = scratch_lengths(&scratch);
                        topk(&node, &input, &mut out, &mut scratch).unwrap();
                        assert_eq!(out.to_i128s(), oracle(&input, axis, k), "{dtype:?} {layout:?} axis={axis} k={k}");
                        assert_eq!(scratch_lengths(&scratch), before, "selection must not gather/widen the input");
                    }
                }
            }
        }
    }
}
#[test]
fn actual_ten_million_native_scores_keep_exact_ties_without_axis_scratch() {
    let n = 10_485_760usize;
    let mut scores = vec![0i32; n];
    scores[n - 1] = 1;
    let buf = Buf::I32(scores);
    let input = Opd { data: buf.slice(), layout: Layout::contiguous(&[n]) };
    let node = NodePlan::for_kernel(Prim::TopK { axis: 0, k: 2 }, DType::Idx, Acc::Fast64);
    let mut out = Buf::Idx(vec![]);
    let mut scratch = Scratch::default();
    let before = scratch_lengths(&scratch);
    let started = std::time::Instant::now();
    topk(&node, &input, &mut out, &mut scratch).unwrap();
    assert_eq!(out, Buf::Idx(vec![0, (n - 1) as u32]));
    assert_eq!(scratch_lengths(&scratch), before);
    println!(
        "TopK native synthetic axis={n} K=2 source_bytes={} output_bytes=8 no widening/axis scratch elapsed_ms={}; no checkpoint/maximum job/native/Final acceptance",
        n * 4,
        started.elapsed().as_millis()
    );
}
