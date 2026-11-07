//! **The fast MatMul kernels at their edges.** The random suite reaches the vec4 GEMV and the tiled
//! GEMM only now and then; here every case is built for one of them, at the boundaries where a
//! tiled kernel goes wrong: row counts that are not a multiple of the rows a workgroup holds (32 for
//! the GEMV, 64 for the GEMM), column counts past a tile, contractions that are not a multiple of
//! the K tile, contractions long enough that the `i32` partials are widened mid-row (the chunk of
//! `i8 × i16` terms is 516, so the GEMV widens every 32 loads of sixteen and the GEMM every 16
//! K-tiles), batches, and the rails (`−128`/`127` weights, `±32767`/`−32768` codes) where a partial
//! is closest to the `i32` bound. Weights are packed params, activations are lanes (a computed
//! node's form) — the forms the fast kernels serve.

mod common;

use misaka_palw_tir::{DType, Dim, Prim, Tensor, TensorType};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use common::device;
use common::one_node::{NodeCase, Tally, run_case_forms};

fn tensor(rng: &mut ChaCha8Rng, d: DType, shape: &[usize], rails: bool) -> Tensor {
    let n: usize = shape.iter().product();
    let (lo, hi) = (d.min_value(), d.max_value());
    let data = (0..n)
        .map(|_| if rails && rng.gen_bool(0.7) { if rng.gen_bool(0.5) { lo } else { hi } } else { rng.gen_range(lo..=hi) })
        .collect();
    Tensor { dtype: d, shape: shape.to_vec(), data }
}

fn mm(rng: &mut ChaCha8Rng, batch: &[usize], m: usize, k: usize, n: usize, rails: bool) -> NodeCase {
    let mut sa = batch.to_vec();
    sa.extend([m, k]);
    let mut sb = batch.to_vec();
    sb.extend([k, n]);
    let mut out = batch.to_vec();
    out.extend([m, n]);
    let a = tensor(rng, DType::I8, &sa, rails);
    let b = tensor(rng, DType::I16, &sb, rails);
    NodeCase {
        prim: Prim::MatMul,
        inputs: vec![a, b],
        out: TensorType::new(DType::I64, out.iter().map(|d| Dim::Fixed(*d as u32)).collect()),
    }
}

fn run(name: &str, cases: Vec<NodeCase>, want: &str) {
    let Some(dev) = device() else { return };
    let mut tally = Tally::default();
    for (i, c) in cases.iter().enumerate() {
        // Weights packed (operand 0 as a param), activations as lanes (operand 1 computed).
        let r = run_case_forms(&dev, c, 0b10);
        let r = r.expect("a program node");
        r.assert_agree(&format!("{name} case {i}: {:?}", c.inputs.iter().map(|t| &t.shape).collect::<Vec<_>>()));
        tally.add(&Some(r));
    }
    eprintln!("{name}: {tally:?}");
    let hits: usize = tally.kernels.iter().filter(|(k, _)| k.starts_with(want)).map(|(_, n)| n).sum();
    assert!(hits * 10 >= cases.len() * 9, "{name}: only {hits} of {} cases ran {want}", cases.len());
}

#[test]
fn the_vec4_gemv_equals_the_cpu_executor_at_every_row_count_and_past_the_chunk() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let mut cases = Vec::new();
    for m in [1usize, 2, 3, 4, 5, 31, 32, 33, 63, 64, 65, 200] {
        for k in [16usize, 48, 512, 528, 1536, 2064] {
            cases.push(mm(&mut rng, &[], m, k, 1, m % 2 == 0));
        }
    }
    for batch in [vec![2usize], vec![3, 2]] {
        cases.push(mm(&mut rng, &batch, 37, 1040, 1, true));
    }
    run("gemv:vec4", cases, "gemv:vec4");
}

#[test]
fn the_tiled_gemm_equals_the_cpu_executor_across_tiles_and_chunk_flushes() {
    let mut rng = ChaCha8Rng::seed_from_u64(12);
    let mut cases = Vec::new();
    for (m, n) in [(64usize, 64usize), (65, 64), (64, 65), (100, 70), (600, 8), (8, 600), (63, 127)] {
        for k in [1usize, 31, 32, 33, 100, 515, 517, 1100] {
            cases.push(mm(&mut rng, &[], m, k, n, k % 2 == 1));
        }
    }
    cases.push(mm(&mut rng, &[3], 70, 300, 66, true));
    run("gemm", cases, "gemm");
}
