//! **The embeddings of the denoiser's input** (RFC-0003 §6): `PATCH_EMBED_V1` (the latent to tokens plus the learned
//! position table cropped to the grid), `EMBED_TIMESTEP_TABLE_V1` (the timestep's sinusoid, evaluated at
//! registration and gathered by the step index), the two-layer conditioning embedders and the sum that makes the
//! conditioning vector.
//!
//! diffusers' SD3 `PatchEmbed` is a strided convolution (`kernel = stride = patch`) followed by a flatten and the
//! position table cropped to the latent's grid (`cropped_pos_embed`: the table is `[max, max, d]`, the grid is
//! cropped from its centre); the integer program is [`super::conv`]'s im2col table, one `MatMul` and ONE narrowing
//! into the residual stream's `i32`, then the table added (it is a param at the stream's own scale, so the add is
//! exact). The timestep path is a table lookup by `(steps index, position)`: the sinusoid of each step's timestep
//! is a registration-time value (PALW-EX-5), carried as `i16` codes at `1/32767` and gathered by
//! `base[steps_index] + pos`; the embedder after it is two projections and a SiLU table.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::INPUT_POS;
use misaka_palw_tir::{DType, Ref};

use super::act::QAct;
use super::conv::{ConvSpec, QConv, QConvRefs, lower_conv_rows};
use super::linear::{QLinear, QLinearRefs, lower_linear_codes};
use super::sink::ParamSink;
use super::tables::timestep_embedding;

/// diffusers' `cropped_pos_embed`: the `[max·max, d]` table (row-major `f32`) cropped to a `gh × gw` grid taken from
/// its centre (`top = (max − gh) / 2`, `left = (max − gw) / 2`, floors), as `[gh·gw, d]`.
pub fn cropped_pos_embed(pos: &[f32], max: usize, d: usize, gh: usize, gw: usize) -> Vec<f32> {
    assert_eq!(pos.len(), max * max * d, "the position table is [max², d]");
    assert!(gh <= max && gw <= max, "the grid {gh}×{gw} does not fit the table's {max}×{max}");
    let (top, left) = ((max - gh) / 2, (max - gw) / 2);
    let mut out = Vec::with_capacity(gh * gw * d);
    for i in 0..gh {
        for j in 0..gw {
            let at = ((top + i) * max + left + j) * d;
            out.extend_from_slice(&pos[at..at + d]);
        }
    }
    out
}

/// A quantised patch embedding: the convolution, and the cropped position table at the stream's scale.
#[derive(Clone, Debug)]
pub struct QPatchEmbed {
    pub conv: QConv,
    /// `[gh·gw, d]` row-major, `round(pos / s_h)`.
    pub pos: Vec<i32>,
    /// The float value of one residual-stream code.
    pub stream_scale: f64,
}

impl QPatchEmbed {
    /// `spec` is the convolution (`cin` the latent channels, `cout = d`, `k = stride = patch`, no padding), `w` its
    /// weight `[d, C, p, p]`, `pos` the CROPPED table `[gh·gw, d]`; latent codes at `x_scale`, the stream at
    /// `stream_scale`.
    pub fn new(spec: ConvSpec, w: &[f32], bias: Option<&[f32]>, pos: &[f32], x_scale: f64, stream_scale: f64) -> Self {
        assert_eq!((spec.k, spec.stride, spec.pad), (spec.k, spec.k, 0), "a patch embedding is a non-overlapping convolution");
        let (gh, gw) = spec.out_hw();
        assert_eq!(pos.len(), gh * gw * spec.cout, "the cropped position table is [gh·gw, d]");
        let q = |v: f32| (v as f64 / stream_scale).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
        Self { conv: QConv::new(spec, w, bias, x_scale, stream_scale), pos: pos.iter().map(|v| q(*v)).collect(), stream_scale }
    }

    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QPatchEmbedRefs {
        let conv = self.conv.declare(pb, sink, name);
        let (gh, gw) = self.conv.spec.out_hw();
        let pos = sink.put(
            pb,
            &format!("{name}.pos"),
            DType::I32,
            &[(gh * gw) as u32, self.conv.spec.cout as u32],
            self.pos.iter().map(|v| *v as i128).collect(),
        );
        QPatchEmbedRefs { conv, pos }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QPatchEmbedRefs {
    pub conv: QConvRefs,
    pub pos: Ref,
}

/// **`PATCH_EMBED_V1`**: the latent `x:[C, H, W]` `i16` codes to the image tokens `[gh·gw, d]` `i32` at the stream's
/// scale: the strided convolution as rows (one narrowing), plus the position table (exact, saturated).
pub fn lower_patch_embed(b: &mut BlockBuilder<'_>, x: Ref, r: &QPatchEmbedRefs, spec: &ConvSpec) -> Ref {
    let y = lower_conv_rows(b, x, &r.conv, spec, i32::MIN as i64, i32::MAX as i64, DType::I32); // [N, d]
    let sum = b.add(y, r.pos, DType::I64);
    b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

/// The timestep sinusoid rows for every offered step count, at `1/32767` per code.
#[derive(Clone, Debug)]
pub struct TimestepTable {
    /// The offered step counts, ascending.
    pub counts: Vec<u32>,
    /// `base[k]`: the row of step 0 of the `k`-th offered count. The job's steps scalar is this INDEX `k` (the position
    /// of its step count among the class's offered counts), never the count itself.
    pub base: Vec<u32>,
    /// `[rows, dim]` row-major: row `base[k] + i` is the embedding of the `i`-th timestep of the `k`-th count.
    pub rows: Vec<i16>,
    pub dim: usize,
}

/// The value of one code of the sinusoid table: `1/32767` (the sinusoid is in `[−1, 1]`).
pub const TIMESTEP_CODE_SCALE: f64 = 1.0 / 32_767.0;

impl TimestepTable {
    /// `timesteps[k]` is the timestep run of the `k`-th offered step count `counts[k]` (`counts[k]` values — the
    /// scheduler's `timesteps`, `σ·1000`); `dim`, `flip`, `shift` and `max_period` as diffusers' `Timesteps`
    /// (SD3: 256, true, 0, 10000).
    pub fn new(counts: &[u32], timesteps: &[Vec<f64>], dim: usize, flip: bool, shift: f64, max_period: f64) -> Self {
        assert_eq!(counts.len(), timesteps.len());
        assert!(counts.windows(2).all(|w| w[0] < w[1]) && !counts.is_empty(), "ascending, non-empty step counts");
        let mut base = vec![0u32; counts.len()];
        let mut rows: Vec<i16> = Vec::new();
        for (k, c) in counts.iter().enumerate() {
            assert_eq!(timesteps[k].len(), *c as usize, "{c} steps carry {c} timesteps");
            base[k] = (rows.len() / dim) as u32;
            for t in &timesteps[k] {
                for v in timestep_embedding(*t, dim, flip, shift, max_period) {
                    rows.push((v / TIMESTEP_CODE_SCALE).round().clamp(-32_767.0, 32_767.0) as i16);
                }
            }
        }
        Self { counts: counts.to_vec(), base, rows, dim }
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len() / self.dim
    }

    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> TimestepRefs {
        let rows = sink.put(
            pb,
            &format!("{name}.rows"),
            DType::I16,
            &[self.total_rows() as u32, self.dim as u32],
            self.rows.iter().map(|v| *v as i128).collect(),
        );
        let base = sink.put(
            pb,
            &format!("{name}.base"),
            DType::Idx,
            &[self.base.len() as u32],
            self.base.iter().map(|v| *v as i128).collect(),
        );
        TimestepRefs { rows, base }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimestepRefs {
    pub rows: Ref,
    pub base: Ref,
}

/// **The row index of this position**: `base[steps_index] + pos`, clamped into `[0, rows)` (the clamp never fires for
/// an offered count; it states the range to the analysis). `steps_index` is the job's steps scalar — the position of
/// its step count among the offered counts, `[0, counts − 1]` — a rank-0 `idx`.
pub fn lower_step_row_index(b: &mut BlockBuilder<'_>, steps_index: Ref, base: Ref, max_index: u32, total_rows: u32) -> Ref {
    let s = b.clamp(steps_index, 0, max_index as i64, DType::Idx);
    let at = b.gather(base, s, 0, 0); // scalar
    let sum = b.add(at, Ref::Input(INPUT_POS), DType::I64);
    b.clamp(sum, 0, total_rows as i64 - 1, DType::Idx)
}

/// **`EMBED_TIMESTEP_TABLE_V1`**: this position's timestep sinusoid, `[1, dim]` `i16` codes at `1/32767`.
pub fn lower_timestep_row(b: &mut BlockBuilder<'_>, steps: Ref, r: &TimestepRefs, t: &TimestepTable) -> Ref {
    let at = lower_step_row_index(b, steps, r.base, t.counts.len() as u32 - 1, t.total_rows() as u32);
    let row = b.gather(r.rows, at, 0, 0); // [dim]
    b.reshape_fixed(row, &[1, t.dim as u32])
}

/// A two-layer embedder: `Linear → SiLU → Linear` (diffusers' `TimestepEmbedding` and `PixArtAlphaTextProjection`).
#[derive(Clone, Debug)]
pub struct QEmbedder {
    pub l1: QLinear,
    pub act: QAct,
    pub l2: QLinear,
}

impl QEmbedder {
    /// `w1:[d, in]`, `w2:[d, d]` (`[out, in]` row-major) and biases; input codes at `x_scale`, the hidden codes at
    /// `h_scale` (the SiLU's input), the activation's output at `a_scale`, the result at `y_scale`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        w1: &[f32],
        b1: &[f32],
        w2: &[f32],
        b2: &[f32],
        inn: usize,
        d: usize,
        x_scale: f64,
        h_scale: f64,
        a_scale: f64,
        y_scale: f64,
    ) -> Self {
        Self {
            l1: QLinear::new(w1, d, inn, Some(b1), x_scale, h_scale),
            act: QAct::silu(h_scale, a_scale),
            l2: QLinear::new(w2, d, d, Some(b2), a_scale, y_scale),
        }
    }

    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> QEmbedderRefs {
        QEmbedderRefs {
            l1: self.l1.declare(pb, sink, &format!("{name}.l1")),
            act: self.act.declare(pb, sink, &format!("{name}.act")),
            l2: self.l2.declare(pb, sink, &format!("{name}.l2")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QEmbedderRefs {
    pub l1: QLinearRefs,
    pub act: Ref,
    pub l2: QLinearRefs,
}

/// The embedder over rows `x:[R, in]` `i16` codes: `[R, d]` `i16` codes.
pub fn lower_embedder(b: &mut BlockBuilder<'_>, x: Ref, r: &QEmbedderRefs) -> Ref {
    let h = lower_linear_codes(b, x, &r.l1);
    let a = b.act_table(h, r.act);
    lower_linear_codes(b, a, &r.l2)
}

/// **The conditioning vector** `temb + pooled`, two `[1, d]` `i16` code rows at ONE scale, saturated into codes.
pub fn lower_cond_sum(b: &mut BlockBuilder<'_>, temb: Ref, pooled: Ref) -> Ref {
    let sum = b.add(temb, pooled, DType::I32);
    b.clamp(sum, -32_767, 32_767, DType::I16)
}

#[cfg(test)]
mod tests {
    use super::super::tables::silu;
    use super::super::testkit::{Lcg, run_one_block, run_steps};
    use super::*;

    #[test]
    fn the_position_table_is_cropped_from_the_centre() {
        // A 6×6 table of one channel whose value names its cell; a 2×4 grid starts at (2, 1).
        let pos: Vec<f32> = (0..36).map(|i| i as f32).collect();
        let c = cropped_pos_embed(&pos, 6, 1, 2, 4);
        assert_eq!(c, vec![13.0, 14.0, 15.0, 16.0, 19.0, 20.0, 21.0, 22.0]);
        // The full grid is the table.
        assert_eq!(cropped_pos_embed(&pos, 6, 1, 6, 6), pos);
    }

    #[test]
    fn the_patch_embedding_is_the_strided_convolution_plus_the_position_row() {
        let mut rng = Lcg(51);
        let spec = ConvSpec { cin: 3, cout: 8, k: 2, stride: 2, pad: 0, h: 4, w: 6 };
        let (gh, gw) = spec.out_hw();
        let n = gh * gw;
        let w: Vec<f32> = (0..spec.cout * spec.taps()).map(|_| rng.unit() as f32).collect();
        let bias: Vec<f32> = (0..spec.cout).map(|_| rng.unit() as f32).collect();
        let pos: Vec<f32> = (0..n * spec.cout).map(|_| rng.unit() as f32 * 3.0).collect();
        let (sx, sh) = (1.0 / 4096.0, 1.0 / 65_536.0);
        let q = QPatchEmbed::new(spec, &w, Some(&bias), &pos, sx, sh);
        let x: Vec<i128> = (0..spec.cin * spec.h * spec.w).map(|_| rng.range(-12_000, 12_000)).collect();
        let x2 = x.clone();
        let y = run_one_block(
            |pb, sink| (sink.put(pb, "x", DType::I16, &[3, 4, 6], x2), q.declare(pb, sink, "pe")),
            |b, (xs, r)| lower_patch_embed(b, xs, &r, &spec),
        );
        assert_eq!(y.shape, vec![n, spec.cout]);
        for t in 0..n {
            let (oh, ow) = (t / gw, t % gw);
            for o in 0..spec.cout {
                let mut f = bias[o] as f64 + pos[t * spec.cout + o] as f64;
                let mut bound = 3.0 * sh;
                for c in 0..spec.cin {
                    for kh in 0..2 {
                        for kw in 0..2 {
                            let xi = x[(c * spec.h + oh * 2 + kh) * spec.w + ow * 2 + kw] as f64 * sx;
                            let wi = w[((o * spec.cin + c) * 2 + kh) * 2 + kw] as f64;
                            f += xi * wi;
                            bound += xi.abs() * 0.5 / 127.0 * 1.0;
                        }
                    }
                }
                let got = y.data[t * spec.cout + o] as f64 * sh;
                assert!((got - f).abs() <= bound + 1e-6, "[{t},{o}]: integer {got} vs float {f} (bound {bound})");
            }
        }
    }

    #[test]
    fn the_timestep_row_is_the_sinusoid_of_this_steps_timestep() {
        let (dim, counts) = (16usize, [2u32, 4]);
        // The scheduler's timesteps for 2 and 4 steps (any values will do for the lookup).
        let ts = vec![vec![1000.0, 750.0], vec![1000.0, 900.0, 700.0, 300.0]];
        let table = TimestepTable::new(&counts, &ts, dim, true, 0.0, 10_000.0);
        assert_eq!((table.total_rows(), table.base.clone()), (6, vec![0, 2]));
        for (k, steps) in counts.iter().enumerate() {
            let t2 = table.clone();
            let rows = run_steps(
                |pb, sink| {
                    let r = t2.declare(pb, sink, "ts");
                    (sink.put(pb, "steps", DType::Idx, &[], vec![k as i128]), r)
                },
                |b, (s, r)| lower_timestep_row(b, s, &r, &t2),
                *steps as usize,
            );
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row.shape, vec![1, dim]);
                let want = timestep_embedding(ts[k][i], dim, true, 0.0, 10_000.0);
                for (g, w) in row.data.iter().zip(&want) {
                    assert!((*g as f64 * TIMESTEP_CODE_SCALE - w).abs() <= 0.6 * TIMESTEP_CODE_SCALE, "count {steps} step {i}");
                }
            }
        }
    }

    #[test]
    fn an_embedder_is_linear_silu_linear() {
        let mut rng = Lcg(53);
        let (inn, d) = (8usize, 12usize);
        let w1: Vec<f32> = (0..d * inn).map(|_| rng.unit() as f32).collect();
        let b1: Vec<f32> = (0..d).map(|_| rng.unit() as f32 * 0.2).collect();
        let w2: Vec<f32> = (0..d * d).map(|_| rng.unit() as f32 * 0.5).collect();
        let b2: Vec<f32> = (0..d).map(|_| rng.unit() as f32 * 0.2).collect();
        let (sx, sh, sa, sy) = (1.0 / 8192.0, 1.0 / 4096.0, 1.0 / 8192.0, 1.0 / 4096.0);
        let e = QEmbedder::new(&w1, &b1, &w2, &b2, inn, d, sx, sh, sa, sy);
        let x: Vec<i128> = (0..inn).map(|_| rng.range(-8_000, 8_000)).collect();
        let x2 = x.clone();
        let y = run_one_block(
            |pb, sink| (sink.put(pb, "x", DType::I16, &[1, inn as u32], x2), e.declare(pb, sink, "emb")),
            |b, (xs, r)| lower_embedder(b, xs, &r),
        );
        for o in 0..d {
            let h: Vec<f64> = (0..d)
                .map(|j| silu(b1[j] as f64 + (0..inn).map(|i| x[i] as f64 * sx * w1[j * inn + i] as f64).sum::<f64>()))
                .collect();
            let f = b2[o] as f64 + (0..d).map(|j| h[j] * w2[o * d + j] as f64).sum::<f64>();
            let got = y.data[o] as f64 * sy;
            assert!((got - f).abs() <= 0.05 + 0.02 * f.abs(), "[{o}]: integer {got} vs float {f}");
        }
    }

    #[test]
    fn the_conditioning_sum_saturates() {
        let y = run_one_block(
            |pb, sink| {
                (
                    sink.put(pb, "a", DType::I16, &[1, 3], vec![100, 30_000, -30_000]),
                    sink.put(pb, "b", DType::I16, &[1, 3], vec![-250, 20_000, -20_000]),
                )
            },
            |b, (a, c)| lower_cond_sum(b, a, c),
        );
        assert_eq!(y.data, vec![-150, 32_767, -32_767]);
    }
}
