//! Composite templates that are NEW (not legacy kernels): their defining exact properties.

mod common;

use common::{Lcg, arg, eval_graph};
use misaka_palw_tir::DType;

/// `c = n·x − Σx` is exactly invariant under `x → x + k`, so the exact-centring LayerNorm returns
/// the SAME bytes for a row and its shift — the property a lossy mean (`Σx / n` rounded) loses.
#[test]
fn layer_norm_exact_is_exactly_shift_invariant_and_zero_on_a_constant_row() {
    let mut rng = Lcg(41);
    for n in [2usize, 7, 64, 1024] {
        let x: Vec<i128> = (0..n).map(|_| rng.range(-20000, 20000)).collect();
        let k = rng.range(-10000, 10000);
        let shifted: Vec<i128> = x.iter().map(|v| v + k).collect();
        let ln = |row: Vec<i128>| {
            eval_graph(&[arg("x", DType::I16, row), arg("ez", DType::I64, vec![1]), arg("es", DType::I8, vec![0])], |b, r| {
                b.layer_norm_exact(r[0], r[1], r[2])
            })
            .unwrap()
        };
        let a = ln(x.clone());
        let b = ln(shifted);
        assert_eq!(a, b, "n {n}: a shift by {k} must not move a single bit");
        // Centred: the output sums to (nearly) zero — exactly zero before the per-lane rounding.
        let sum: i128 = a.data.iter().sum();
        assert!(sum.abs() <= n as i128, "n {n}: Σ LN(x) = {sum}");
        // Unit RMS in Q24: Σ y² ≈ n · 2^48.
        let ss: i128 = a.data.iter().map(|v| v * v).sum();
        let want = n as i128 * (1i128 << 48);
        assert!((ss - want).abs() * 100 < want, "n {n}: Σy² = {ss}, want ≈ {want}");
    }
    // A constant row has no direction: c = 0, and with eps > 0 the output is exactly zero.
    let z =
        eval_graph(&[arg("x", DType::I16, vec![123; 16]), arg("ez", DType::I64, vec![1]), arg("es", DType::I8, vec![0])], |b, r| {
            b.layer_norm_exact(r[0], r[1], r[2])
        })
        .unwrap();
    assert!(z.data.iter().all(|v| *v == 0));
}
