//! **`CONV_DENSE_V1`** — a dense 2-D convolution as the one linear map it is (spec RFC-0003 §6, the lowerer
//! table): im2col by a pinned index table, one `MatMul`, one narrowing.
//!
//! The input `x:[Cin, H, W]` `i16` codes is flattened and a zero element appended (the padding's value); a pinned
//! `Idx` table `[Ho·Wo, Cin·k·k]` names, for every output position and every tap `(c, kh, kw)`, the flat index it
//! reads — `Cin·H·W` (the zero) where the tap falls in the padding. One `Gather` makes the columns
//! `[Ho·Wo, K]`, and [`super::linear`]'s projection (`MatMul` into exact `i64`, one narrowing per output channel)
//! gives `[Ho·Wo, Cout]`, transposed back to `[Cout, Ho, Wo]`.
//!
//! The table is a param, so a class can carry any geometry; it is clamped into the data's range before the gather,
//! a `Clamp` that never fires and makes the range analysis total for any registered table (the library's
//! convention). Stride and padding are the table's; the kernel is square; groups are 1 (depthwise is a later
//! lowerer).

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Ref};

use super::linear::{QLinear, QLinearRefs, lower_linear};
use super::sink::ParamSink;

/// A convolution's geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvSpec {
    pub cin: usize,
    pub cout: usize,
    pub k: usize,
    pub stride: usize,
    pub pad: usize,
    /// The input's height and width.
    pub h: usize,
    pub w: usize,
}

impl ConvSpec {
    /// The output's height and width (`⌊(H + 2p − k) / s⌋ + 1`).
    pub fn out_hw(&self) -> (usize, usize) {
        ((self.h + 2 * self.pad - self.k) / self.stride + 1, (self.w + 2 * self.pad - self.k) / self.stride + 1)
    }

    /// The taps per output position, `Cin · k · k`.
    pub fn taps(&self) -> usize {
        self.cin * self.k * self.k
    }

    /// The im2col table, `[Ho·Wo, Cin·k·k]` row-major: the flat input index of each tap, `Cin·H·W` for padding.
    pub fn im2col_index(&self) -> Vec<u32> {
        let (ho, wo) = self.out_hw();
        let n = (self.cin * self.h * self.w) as u32;
        let mut out = Vec::with_capacity(ho * wo * self.taps());
        for oh in 0..ho {
            for ow in 0..wo {
                for c in 0..self.cin {
                    for kh in 0..self.k {
                        for kw in 0..self.k {
                            let (ih, iw) = (
                                (oh * self.stride + kh) as isize - self.pad as isize,
                                (ow * self.stride + kw) as isize - self.pad as isize,
                            );
                            out.push(if ih < 0 || iw < 0 || ih >= self.h as isize || iw >= self.w as isize {
                                n
                            } else {
                                (c * self.h * self.w) as u32 + (ih as usize * self.w + iw as usize) as u32
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

/// A quantised convolution.
#[derive(Clone, Debug)]
pub struct QConv {
    pub spec: ConvSpec,
    pub lin: QLinear,
    pub idx: Vec<u32>,
}

impl QConv {
    /// Quantise `w:[Cout, Cin, k, k]` (row-major `f32`) and `bias:[Cout]` for inputs at `x_scale` and an output at `y_scale`.
    pub fn new(spec: ConvSpec, w: &[f32], bias: Option<&[f32]>, x_scale: f64, y_scale: f64) -> Self {
        assert_eq!(w.len(), spec.cout * spec.taps(), "a convolution weight is [Cout, Cin, k, k]");
        Self { spec, lin: QLinear::new(w, spec.cout, spec.taps(), bias, x_scale, y_scale), idx: spec.im2col_index() }
    }

    /// Declare the params (`<name>.w/.m/.s/.z` as a linear's and `<name>.idx`) on `pb`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QConvRefs {
        let lin = self.lin.declare(pb, sink, name);
        let (ho, wo) = self.spec.out_hw();
        let idx = sink.put(
            pb,
            &format!("{name}.idx"),
            DType::Idx,
            &[(ho * wo) as u32, self.spec.taps() as u32],
            self.idx.iter().map(|v| *v as i128).collect(),
        );
        QConvRefs { lin, idx }
    }
}

/// The declared params of a [`QConv`].
#[derive(Clone, Copy, Debug)]
pub struct QConvRefs {
    pub lin: QLinearRefs,
    pub idx: Ref,
}

/// **The convolution**: `x:[Cin, H, W]` `i16` codes to `[Cout, Ho, Wo]` `i16` codes.
pub fn lower_conv(b: &mut BlockBuilder<'_>, x: Ref, c: &QConvRefs, spec: &ConvSpec) -> Ref {
    lower_conv_as(b, x, c, spec, -32_767, 32_767, DType::I16)
}

/// [`lower_conv`] narrowed into `[lo, hi]` of `dtype` (the patch embedding's is the residual stream's `i32`).
pub fn lower_conv_as(b: &mut BlockBuilder<'_>, x: Ref, c: &QConvRefs, spec: &ConvSpec, lo: i64, hi: i64, dtype: DType) -> Ref {
    let y = lower_conv_rows(b, x, c, spec, lo, hi, dtype); // [Ho·Wo, Cout]
    let yt = b.transpose(y, &[1, 0]);
    let (ho, wo) = spec.out_hw();
    b.reshape_fixed(yt, &[spec.cout as u32, ho as u32, wo as u32])
}

/// The convolution as ROWS, `[Ho·Wo, Cout]` (one row per output position — the token layout of a patch embedding),
/// narrowed into `[lo, hi]` of `dtype`.
pub fn lower_conv_rows(b: &mut BlockBuilder<'_>, x: Ref, c: &QConvRefs, spec: &ConvSpec, lo: i64, hi: i64, dtype: DType) -> Ref {
    let n = (spec.cin * spec.h * spec.w) as u32;
    let flat = b.reshape_fixed(x, &[n]);
    let zero = b.pb.konst(DType::I16, &[1], &[0]);
    let padded = b.concat(&[flat, zero], 0);
    let idx = b.clamp(c.idx, 0, n as i64, DType::Idx);
    let cols = b.gather(padded, idx, 0, 0); // [Ho·Wo, K]
    lower_linear(b, cols, &c.lin, lo, hi, dtype)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    fn check(spec: ConvSpec, with_bias: bool, seed: u64) {
        let mut rng = Lcg(seed);
        let w: Vec<f32> = (0..spec.cout * spec.taps()).map(|_| rng.unit() as f32).collect();
        let bias: Vec<f32> = (0..spec.cout).map(|_| rng.unit() as f32).collect();
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 2048.0);
        let q = QConv::new(spec, &w, with_bias.then_some(&bias[..]), sx, sy);
        let x: Vec<i128> = (0..spec.cin * spec.h * spec.w).map(|_| rng.range(-15_000, 15_000)).collect();
        let x2 = x.clone();
        let y = run_one_block(
            |pb, sink| {
                let xs = sink.put(pb, "x", DType::I16, &[spec.cin as u32, spec.h as u32, spec.w as u32], x2);
                (xs, q.declare(pb, sink, "conv"))
            },
            |b, (xs, c)| lower_conv(b, xs, &c, &spec),
        );
        let (ho, wo) = spec.out_hw();
        assert_eq!(y.shape, vec![spec.cout, ho, wo]);
        for co in 0..spec.cout {
            let amax = w[co * spec.taps()..(co + 1) * spec.taps()].iter().fold(0f64, |m, v| m.max((*v as f64).abs()));
            for oh in 0..ho {
                for ow in 0..wo {
                    let mut f = if with_bias { bias[co] as f64 } else { 0.0 };
                    let mut bound = 1.6 * sy;
                    for c in 0..spec.cin {
                        for kh in 0..spec.k {
                            for kw in 0..spec.k {
                                let (ih, iw) = (
                                    (oh * spec.stride + kh) as isize - spec.pad as isize,
                                    (ow * spec.stride + kw) as isize - spec.pad as isize,
                                );
                                if ih < 0 || iw < 0 || ih >= spec.h as isize || iw >= spec.w as isize {
                                    continue;
                                }
                                let xv = x[c * spec.h * spec.w + ih as usize * spec.w + iw as usize] as f64 * sx;
                                f += xv * w[((co * spec.cin + c) * spec.k + kh) * spec.k + kw] as f64;
                                bound += xv.abs() * amax / 254.0;
                            }
                        }
                    }
                    let got = y.data[(co * ho + oh) * wo + ow] as f64 * sy;
                    assert!((got - f).abs() <= bound + 1e-9, "[{co},{oh},{ow}]: integer {got} vs float {f} (bound {bound})");
                }
            }
        }
    }

    #[test]
    fn the_convolution_is_the_float_one_to_the_weights_rounding() {
        check(ConvSpec { cin: 2, cout: 3, k: 3, stride: 1, pad: 1, h: 5, w: 4 }, true, 1);
        check(ConvSpec { cin: 3, cout: 2, k: 3, stride: 2, pad: 1, h: 6, w: 6 }, false, 2);
        check(ConvSpec { cin: 4, cout: 5, k: 1, stride: 1, pad: 0, h: 3, w: 3 }, true, 3);
    }

    #[test]
    fn the_im2col_table_names_the_taps_and_the_padding() {
        let spec = ConvSpec { cin: 1, cout: 1, k: 3, stride: 1, pad: 1, h: 2, w: 2 };
        let idx = spec.im2col_index();
        assert_eq!(idx.len(), 4 * 9);
        // Output (0, 0): taps (kh, kw) over rows -1..=1 and columns -1..=1: only (1,1)=0, (1,2)=1, (2,1)=2, (2,2)=3 are inside.
        let zero = 4u32;
        assert_eq!(&idx[0..9], &[zero, zero, zero, zero, 0, 1, zero, 2, 3]);
        assert!(idx.iter().all(|i| *i <= zero));
    }
}
