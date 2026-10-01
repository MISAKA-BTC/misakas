//! **Rescaling between sites**: the one narrowing that carries a tensor from one code scale to another — the
//! residual stream (`i32` at its own scale) to the `i16` codes a LayerNorm or a MAC reads, the latent state to the
//! patch embedding's input, a block's `i16` output into the stream. A site's scale is data (calibrated at
//! registration); the ratio is `mul_shift(from_scale / to_scale)`.

use misaka_palw_tir::builder::BlockBuilder;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::{DType, Ref};

/// `N[lo, hi](x; m, s)`: `x` at one scale to another by the registered ratio `(m, s)`.
pub fn lower_requant(b: &mut BlockBuilder<'_>, x: Ref, ratio: (i64, i8), lo: i64, hi: i64, dtype: DType) -> Ref {
    let m = b.c(DType::I64, ratio.0 as i128);
    let s = b.c(DType::I8, ratio.1 as i128);
    b.narrow(x, &Narrowing::new(m, s, None), lo, hi, dtype)
}

/// [`lower_requant`] into `i16` codes (`±32767`).
pub fn lower_to_codes(b: &mut BlockBuilder<'_>, x: Ref, ratio: (i64, i8)) -> Ref {
    lower_requant(b, x, ratio, -32_767, 32_767, DType::I16)
}

/// [`lower_requant`] into the `i32` rail.
pub fn lower_to_stream(b: &mut BlockBuilder<'_>, x: Ref, ratio: (i64, i8)) -> Ref {
    lower_requant(b, x, ratio, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;
    use crate::quant::mul_shift;

    #[test]
    fn a_requantisation_is_the_ratio_of_the_scales() {
        let mut rng = Lcg(61);
        let (from, to) = (1.0 / 1_048_576.0, 1.0 / 2048.0);
        let x: Vec<i128> = (0..32).map(|_| rng.range(-1 << 27, 1 << 27)).collect();
        let x2 = x.clone();
        let y =
            run_one_block(|pb, sink| sink.put(pb, "x", DType::I32, &[32], x2), |b, xs| lower_to_codes(b, xs, mul_shift(from / to)));
        for (g, v) in y.data.iter().zip(&x) {
            let want = (*v as f64 * from / to).clamp(-32_767.0, 32_767.0);
            assert!((*g as f64 - want).abs() <= 1.0, "{g} vs {want}");
        }
        // Up the rail: codes back into the stream's scale.
        let x3: Vec<i128> = (0..8).map(|_| rng.range(-30_000, 30_000)).collect();
        let x4 = x3.clone();
        let y =
            run_one_block(|pb, sink| sink.put(pb, "x", DType::I16, &[8], x4), |b, xs| lower_to_stream(b, xs, mul_shift(to / from)));
        for (g, v) in y.data.iter().zip(&x3) {
            assert!((*g as f64 - *v as f64 * to / from).abs() <= 1.0, "{g} vs {}", *v as f64 * to / from);
        }
    }
}
