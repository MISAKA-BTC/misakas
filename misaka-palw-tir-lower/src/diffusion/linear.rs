//! **A quantised linear map** — the lowering every projection of the profile shares (the DiT's q/k/v/out, the
//! feed-forward, the modulation, the embedders; the VAE's attention).
//!
//! `y = W·x (+ b)` with `W` `[out, in]`: `i8` weight codes at one scale per OUTPUT CHANNEL (the row's absmax maps to
//! ±127), stored transposed `[in, out]` so one `MatMul` over the rows `x:[R, in]` gives the exact `i64`
//! accumulator `[R, out]`; then ONE narrowing per channel, `N[lo,hi](acc; m_c, s_c, z_c)`, where
//! `m_c / 2^s_c = x_scale · w_scale_c / y_scale` carries the input scale, the channel's weight scale and the output
//! scale in one multiplier, and `z_c = round(b_c / y_scale)` is the bias at the output scale (the narrowing's own
//! additive term: no `i64` bias add whose range depends on a param). That is the lowering's one lossy site per
//! projection.
//!
//! The params are admissible at their full dtype ranges: `W` is `i8`, `m` and `z` are `i64` (the narrowing takes
//! their product through `i128`), `s` is clamped into `[0, 62]` by the narrowing itself.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::{DType, Ref};

use super::sink::ParamSink;
use crate::quant::{mul_shift, quantize_rows};

/// A quantised linear map, as integers.
#[derive(Clone, Debug)]
pub struct QLinear {
    pub inn: usize,
    pub out: usize,
    /// `W` as `i8` codes, `[in, out]` row-major.
    pub w: Vec<i8>,
    /// The float value of one weight code, per output channel.
    pub w_scale: Vec<f64>,
    /// Per output channel: the narrowing's multiplier and shift.
    pub m: Vec<i64>,
    pub s: Vec<i8>,
    /// The bias at the output scale, per output channel.
    pub z: Option<Vec<i64>>,
    /// The float value of one input / output code.
    pub x_scale: f64,
    pub y_scale: f64,
}

impl QLinear {
    /// Quantise `W` (`[out, in]` row-major `f32`) and `bias` for inputs at `x_scale` and an output at `y_scale`.
    pub fn new(w: &[f32], out: usize, inn: usize, bias: Option<&[f32]>, x_scale: f64, y_scale: f64) -> Self {
        assert!(x_scale > 0.0 && y_scale > 0.0, "a code scale is positive");
        let q = quantize_rows(w, out, inn, None);
        let mut wt = vec![0i8; inn * out];
        for o in 0..out {
            for i in 0..inn {
                wt[i * out + o] = q.codes[o * inn + i];
            }
        }
        let (mut m, mut s) = (Vec::with_capacity(out), Vec::with_capacity(out));
        for c in 0..out {
            let (mc, sc) = mul_shift(x_scale * q.scales[c] / y_scale);
            m.push(mc);
            s.push(sc);
        }
        let z = bias.map(|b| {
            assert_eq!(b.len(), out, "one bias per output channel");
            b.iter().map(|v| (*v as f64 / y_scale).round().clamp(-(1i64 << 62) as f64, (1i64 << 62) as f64) as i64).collect()
        });
        Self { inn, out, w: wt, w_scale: q.scales, m, s, z, x_scale, y_scale }
    }

    /// Declare the params on `pb` (named `<name>.w`, `.m`, `.s` and `.z`) and keep their values in `sink`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QLinearRefs {
        let w = sink.put(
            pb,
            &format!("{name}.w"),
            DType::I8,
            &[self.inn as u32, self.out as u32],
            self.w.iter().map(|v| *v as i128).collect(),
        );
        let m = sink.put(pb, &format!("{name}.m"), DType::I64, &[self.out as u32], self.m.iter().map(|v| *v as i128).collect());
        let s = sink.put(pb, &format!("{name}.s"), DType::I8, &[self.out as u32], self.s.iter().map(|v| *v as i128).collect());
        let z = self
            .z
            .as_ref()
            .map(|z| sink.put(pb, &format!("{name}.z"), DType::I64, &[self.out as u32], z.iter().map(|v| *v as i128).collect()));
        QLinearRefs { w, m, s, z }
    }
}

/// The declared params of a [`QLinear`].
#[derive(Clone, Copy, Debug)]
pub struct QLinearRefs {
    pub w: Ref,
    pub m: Ref,
    pub s: Ref,
    pub z: Option<Ref>,
}

/// **The projection**: `x:[.., in]` `i16` codes (rank ≥ 2) to `[.., out]`, narrowed into `[lo, hi]` of `dtype`.
/// One `MatMul` (exact `i64`) and one narrowing.
pub fn lower_linear(b: &mut BlockBuilder<'_>, x: Ref, l: &QLinearRefs, lo: i64, hi: i64, dtype: DType) -> Ref {
    let acc = b.matmul(x, l.w, DType::I64);
    b.narrow(acc, &Narrowing::new(l.m, l.s, l.z), lo, hi, dtype)
}

/// The `i16` codes of a [`lower_linear`] (`±32767`).
pub fn lower_linear_codes(b: &mut BlockBuilder<'_>, x: Ref, l: &QLinearRefs) -> Ref {
    lower_linear(b, x, l, -32_767, 32_767, DType::I16)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    /// The float answer and a bound on how far the integer one may be from it: the weights' rounding (half a code
    /// per element at the channel's scale), the bias's and the narrowing's (half an output code each), plus slack.
    fn check(rows: usize, inn: usize, out: usize, with_bias: bool, seed: u64) {
        let mut rng = Lcg(seed);
        let w: Vec<f32> = (0..out * inn).map(|_| rng.unit() as f32 * (1.0 + (rng.range(0, 3) as f32))).collect();
        let bias: Vec<f32> = (0..out).map(|_| rng.unit() as f32 * 2.0).collect();
        let (sx, sy) = (1.0 / 4096.0, 1.0 / 2048.0);
        let q = QLinear::new(&w, out, inn, with_bias.then_some(&bias[..]), sx, sy);
        let x: Vec<i128> = (0..rows * inn).map(|_| rng.range(-20_000, 20_000)).collect();
        let x2 = x.clone();
        let y = run_one_block(
            |pb, sink| {
                let xs = sink.put(pb, "x", DType::I16, &[rows as u32, inn as u32], x2);
                let l = q.declare(pb, sink, "lin");
                (xs, l)
            },
            |b, (xs, l)| lower_linear_codes(b, xs, &l),
        );
        assert_eq!(y.shape, vec![rows, out]);
        for r in 0..rows {
            for c in 0..out {
                let mut f = if with_bias { bias[c] as f64 } else { 0.0 };
                let mut bound = 1.6 * sy;
                for i in 0..inn {
                    let xi = x[r * inn + i] as f64 * sx;
                    f += xi * w[c * inn + i] as f64;
                    bound += xi.abs() * q_half_code(&w, c, inn);
                }
                let got = y.data[r * out + c] as f64 * sy;
                assert!((got - f).abs() <= bound + 1e-9, "[{r},{c}]: integer {got} vs float {f} (bound {bound})");
            }
        }
    }

    /// Half a weight code of row `c`: `amax / 254`.
    fn q_half_code(w: &[f32], c: usize, inn: usize) -> f64 {
        w[c * inn..(c + 1) * inn].iter().fold(0f64, |m, v| m.max((*v as f64).abs())) / 254.0
    }

    #[test]
    fn a_projection_is_the_float_one_to_the_weights_rounding() {
        check(3, 8, 5, false, 1);
        check(4, 16, 7, true, 2);
        check(1, 64, 12, true, 3);
    }

    #[test]
    fn the_params_are_named_and_the_sink_binds_them() {
        let mut rng = Lcg(9);
        let w: Vec<f32> = (0..6).map(|_| rng.unit() as f32).collect();
        let q = QLinear::new(&w, 2, 3, Some(&[0.5, -0.5]), 1.0 / 1024.0, 1.0 / 1024.0);
        assert_eq!((q.inn, q.out, q.w.len(), q.m.len(), q.s.len()), (3, 2, 6, 2, 2));
        assert!(q.z.is_some() && q.m.iter().all(|m| *m != 0));
    }
}
