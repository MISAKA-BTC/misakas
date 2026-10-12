//! **`CONV_DENSE_V2`** — a dense 2-D convolution as the one linear map it is (spec RFC-0003 §6, the lowerer
//! table): im2col by STATIC index maps, one `MatMul`, one narrowing.
//!
//! The columns `[Ho·Wo, Cin·k·k]` (taps in `(c, kh, kw)` order, a row per output position) are made from the input
//! `x:[Cin, H, W]` `i16` codes by `Reshape`, `Slice`, `Concat`, `Broadcast` and `Transpose` alone — every element of
//! a column is a fixed element of `x` or the padding's zero, named by the PROGRAM. [`super::linear`]'s projection
//! (`MatMul` into exact `i64`, one narrowing per output channel) gives `[Ho·Wo, Cout]`, transposed back to
//! `[Cout, Ho, Wo]`.
//!
//! **Why no index table** (`CONV_DENSE_V1` gathered through a pinned `Idx` param): a gather's data element sits at a VALUE's
//! index, so what a court must open for a column is a function of the registered table, which the chain cannot read when it
//! prices a close. Priced soundly (PALW-GEN-20) a tile of `K` columns that gather is `K` leaves of the input whatever the
//! table says — an adversarial table makes it so — and a close that carries `K` leaves is past a carrier for any real `K`.
//! A static map is read by the gate exactly (the twin sizes `Slice`/`Concat`/`Reshape` as the court evaluates them), carries no
//! table in the artifact, and cannot be made worse by a registrant. The integers are the same: the columns are the table's
//! gather, element for element ([`ConvSpec::im2col_index`] is the reference the tests hold them to).
//!
//! Three geometries are static maps: a pointwise convolution (`k = 1`), a patchify (`stride = k`, no padding, whole
//! patches — one `Reshape` and `Transpose`s), and any other (`Slice` of the padded input, one per tap, strided taps through a
//! `Reshape` to `[.., Ho, s, ..]`); groups are 1 (depthwise is a later lowerer).

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Dim, Ref};

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
}

impl QConv {
    /// Quantise `w:[Cout, Cin, k, k]` (row-major `f32`) and `bias:[Cout]` for inputs at `x_scale` and an output at `y_scale`.
    pub fn new(spec: ConvSpec, w: &[f32], bias: Option<&[f32]>, x_scale: f64, y_scale: f64) -> Self {
        assert_eq!(w.len(), spec.cout * spec.taps(), "a convolution weight is [Cout, Cin, k, k]");
        Self { spec, lin: QLinear::new(w, spec.cout, spec.taps(), bias, x_scale, y_scale) }
    }

    /// Declare the params (`<name>.w/.m/.s/.z` as a linear's) on `pb`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QConvRefs {
        QConvRefs { lin: self.lin.declare(pb, sink, name) }
    }
}

/// The declared params of a [`QConv`].
#[derive(Clone, Copy, Debug)]
pub struct QConvRefs {
    pub lin: QLinearRefs,
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
    let cols = lower_conv_cols(b, x, spec); // [Ho·Wo, K]
    lower_linear(b, cols, &c.lin, lo, hi, dtype)
}

/// `Concat` of any number of parts (a node takes at most 8, NF-14): groups of eight, then the groups.
fn concat_wide(b: &mut BlockBuilder<'_>, parts: &[Ref], axis: usize) -> Ref {
    match parts.len() {
        0 => unreachable!("a concatenation of nothing"),
        1 => parts[0],
        2..=8 => b.concat(parts, axis),
        _ => {
            let groups: Vec<Ref> = parts.chunks(8).map(|g| concat_wide(b, g, axis)).collect();
            concat_wide(b, &groups, axis)
        }
    }
}

/// A block of zeros of `shape` (a `Broadcast` of one zero: no constant of the block's size is stored).
fn zeros(b: &mut BlockBuilder<'_>, shape: &[u32]) -> Ref {
    let zero = b.pb.konst(DType::I16, &[1], &[0]);
    b.broadcast(zero, &shape.iter().map(|d| Dim::Fixed(*d)).collect::<Vec<_>>())
}

/// **The columns**: `x:[Cin, H, W]` to `[Ho·Wo, Cin·k·k]` (a row per output position, taps in `(c, kh, kw)` order) by
/// static maps alone — see the module doc.
pub fn lower_conv_cols(b: &mut BlockBuilder<'_>, x: Ref, spec: &ConvSpec) -> Ref {
    let (cin, h, w, k, s, p) = (spec.cin as u32, spec.h as u32, spec.w as u32, spec.k as u32, spec.stride as u32, spec.pad as u32);
    let (ho, wo) = spec.out_hw();
    let (ho, wo) = (ho as u32, wo as u32);
    // A pointwise convolution: the columns are the input's channels at every position.
    if k == 1 && s == 1 && p == 0 {
        let flat = b.reshape_fixed(x, &[cin, h * w]);
        return b.transpose(flat, &[1, 0]);
    }
    // A patchify: non-overlapping patches of the whole input. `[Cin·Ho, k, Wo, k]` → `[Cin·Ho, Wo, k, k]` → `[Cin, Ho, Wo, k·k]`
    // → `[Ho, Wo, Cin, k·k]` → `[Ho·Wo, Cin·k·k]`.
    if s == k && p == 0 && h == ho * k && w == wo * k {
        let r = b.reshape_fixed(x, &[cin * ho, k, wo, k]);
        let r = b.transpose(r, &[0, 2, 1, 3]);
        let r = b.reshape_fixed(r, &[cin, ho, wo, k * k]);
        let r = b.transpose(r, &[1, 2, 0, 3]);
        return b.reshape_fixed(r, &[ho * wo, cin * k * k]);
    }
    // Any other geometry: pad the input (zero blocks around it, and past the end enough that every tap's strided window is a
    // whole number of strides), slice one window per tap, and stack the taps behind the channel.
    let hp = (h + 2 * p).max(k - 1 + s * ho);
    let wp = (w + 2 * p).max(k - 1 + s * wo);
    let (bottom, right) = (hp - h - p, wp - w - p);
    let mut xp = x;
    if p > 0 || bottom > 0 {
        let mut parts = Vec::with_capacity(3);
        if p > 0 {
            parts.push(zeros(b, &[cin, p, w]));
        }
        parts.push(xp);
        if bottom > 0 {
            parts.push(zeros(b, &[cin, bottom, w]));
        }
        xp = b.concat(&parts, 1);
    }
    if p > 0 || right > 0 {
        let mut parts = Vec::with_capacity(3);
        if p > 0 {
            parts.push(zeros(b, &[cin, hp, p]));
        }
        parts.push(xp);
        if right > 0 {
            parts.push(zeros(b, &[cin, hp, right]));
        }
        xp = b.concat(&parts, 2);
    }
    let mut taps = Vec::with_capacity((k * k) as usize);
    for kh in 0..k {
        for kw in 0..k {
            // Rows `kh + s·oh`: a window of `s·Ho` rows, then every s-th.
            let mut t = b.slice(xp, 1, kh, s * ho);
            if s > 1 {
                t = b.reshape_fixed(t, &[cin, ho, s, wp]);
                t = b.slice(t, 2, 0, 1);
                t = b.reshape_fixed(t, &[cin, ho, wp]);
            }
            // Columns `kw + s·ow`, likewise.
            t = b.slice(t, 2, kw, s * wo);
            if s > 1 {
                t = b.reshape_fixed(t, &[cin * ho, wo, s]);
                t = b.slice(t, 2, 0, 1);
            }
            taps.push(b.reshape_fixed(t, &[cin, 1, ho * wo]));
        }
    }
    let stacked = concat_wide(b, &taps, 1); // [Cin, k·k, Ho·Wo]
    let t = b.transpose(stacked, &[2, 0, 1]); // [Ho·Wo, Cin, k·k]
    b.reshape_fixed(t, &[ho * wo, cin * k * k])
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    fn check(spec: ConvSpec, with_bias: bool, seed: u64) {
        let mut rng = Lcg(seed);
        let w: Vec<f32> = (0..spec.cout * spec.taps()).map(|_| rng.unit() as f32).collect();
        let bias: Vec<f32> = (0..spec.cout).map(|_| rng.unit() as f32).collect();
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 512.0);
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

    /// **The static columns are the table's gather, element for element** — for every geometry a static map covers.
    #[test]
    fn the_static_columns_are_the_im2col_tables_gather() {
        for (n, spec) in [
            ConvSpec { cin: 2, cout: 1, k: 3, stride: 1, pad: 1, h: 5, w: 4 },
            ConvSpec { cin: 3, cout: 1, k: 3, stride: 2, pad: 1, h: 6, w: 6 },
            ConvSpec { cin: 3, cout: 1, k: 3, stride: 2, pad: 1, h: 7, w: 5 },
            ConvSpec { cin: 2, cout: 1, k: 3, stride: 1, pad: 0, h: 4, w: 5 },
            ConvSpec { cin: 2, cout: 1, k: 5, stride: 2, pad: 2, h: 8, w: 9 },
            ConvSpec { cin: 4, cout: 1, k: 1, stride: 1, pad: 0, h: 3, w: 2 },
            ConvSpec { cin: 3, cout: 1, k: 2, stride: 2, pad: 0, h: 4, w: 6 },
            ConvSpec { cin: 1, cout: 1, k: 4, stride: 4, pad: 0, h: 8, w: 4 },
            ConvSpec { cin: 2, cout: 1, k: 3, stride: 3, pad: 0, h: 6, w: 6 },
        ]
        .into_iter()
        .enumerate()
        {
            let mut rng = Lcg(7 + n as u64);
            let x: Vec<i128> = (0..spec.cin * spec.h * spec.w).map(|_| rng.range(-15_000, 15_000)).collect();
            let x2 = x.clone();
            let cols = run_one_block(
                |pb, sink| sink.put(pb, "x", DType::I16, &[spec.cin as u32, spec.h as u32, spec.w as u32], x2),
                |b, xs| lower_conv_cols(b, xs, &spec),
            );
            let (ho, wo) = spec.out_hw();
            assert_eq!(cols.shape, vec![ho * wo, spec.taps()], "{spec:?}");
            let n_in = (spec.cin * spec.h * spec.w) as u32;
            let want: Vec<i128> = spec.im2col_index().iter().map(|i| if *i == n_in { 0 } else { x[*i as usize] }).collect();
            assert_eq!(cols.data, want, "geometry {n}: {spec:?}");
        }
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
