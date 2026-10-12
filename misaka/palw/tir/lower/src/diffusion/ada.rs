//! **`MOD_ADALN_V1`** — adaptive LayerNorm modulation (MMDiT's `AdaLayerNormZero` and `AdaLayerNormContinuous`) and
//! the gated residual it feeds (RFC-0003 §6).
//!
//! The conditioning `c` (the timestep-and-pooled embedding, `[d]` codes) goes through `SiLU` (an activation table)
//! and ONE linear to the modulation vector (`[6d]` for the zero variant: shift/scale/gate of the attention and of
//! the MLP; `[2d]` for the continuous one: scale, shift) — the caller's [`super::linear`] projection, committed,
//! so every consumer's cone reads a modulation tile, not the embedding's MACs. Here:
//!
//! * [`lower_ada_layer_norm`]: `x̂ = LN(x)` (the library's exact LayerNorm: `c = n·x − Σx`, a Q24 unit row), then
//!   `x̂·(1 + scale) + shift` in Q24 (the modulation codes are at the power-of-two scale `2^-e`, so their Q24 value
//!   is a multiply by `2^(24−e)`), narrowed to `i16` codes at the consumer's site scale — the lossy sites are the
//!   LayerNorm's (`IntRsqrt`) and that narrowing, with the modulation's own beside them;
//! * [`lower_gated_residual`]: `h' = h + gate · f` for the residual stream `h` (`i32` at `s_h`), the branch `f`
//!   (`i16` codes at `s_f`) and the gate (`[1, d]` codes at `2^-e`): an exact `i64` product narrowed to `s_h`, added,
//!   saturated — the residual stream keeps its own scale across blocks.

use misaka_palw_tir::arith::{K, ONE};
use misaka_palw_tir::builder::BlockBuilder;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::{DType, Ref};

use crate::quant::{eps_pair, mul_shift};

/// The constants of one modulated LayerNorm: the LayerNorm's `ε` pair, the modulation's Q24 multiplier and the
/// narrowing of the result to codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdaNormConsts {
    /// `ε` of the LayerNorm as the wide RMS template's `(eps_zero, eps_shift)`.
    pub eps: (i64, i8),
    /// `2^(24 − e)`: a modulation code at scale `2^-e` times this is its Q24 value.
    pub mod_to_q24: i64,
    /// `m/2^s = 2^-24 / out_scale`: the Q24 result to codes at the consumer's site scale.
    pub out: (i64, i8),
}

impl AdaNormConsts {
    /// For LayerNorm over `d` lanes of codes at `x_scale` with `eps` (diffusers: `1e-6`), a modulation at
    /// `2^-mod_exp` and a result at `out_scale`.
    pub fn new(d: usize, x_scale: f64, eps: f64, mod_exp: u32, out_scale: f64) -> Self {
        assert!(mod_exp <= 24, "a modulation scale of 2^-{mod_exp} has no integer Q24 multiplier");
        // `Σc² = n³·Var`, so the wide RMS of `c = n·x − Σx` sees `mean(c²) = n²·Var`; `ε` joins it at `n²·ε/sx²` and
        // enters the template in Q24 (`· 2^24`).
        let n = d as f64;
        let eps_q = n * n * eps / (x_scale * x_scale) * (1u64 << K) as f64;
        Self { eps: eps_pair(eps_q), mod_to_q24: 1i64 << (24 - mod_exp), out: mul_shift(1.0 / (1u64 << K) as f64 / out_scale) }
    }
}

/// **`LN(x)·(1 + scale) + shift`**: `x:[N, d]` `i16` codes, `scale`, `shift:[1, d]` modulation codes; out `[N, d]`
/// `i16` codes at the consumer's scale.
pub fn lower_ada_layer_norm(b: &mut BlockBuilder<'_>, x: Ref, scale: Ref, shift: Ref, k: &AdaNormConsts) -> Ref {
    let ez = b.c(DType::I64, k.eps.0 as i128);
    let es = b.c(DType::I8, k.eps.1 as i128);
    let unit = b.layer_norm_exact(x, ez, es); // [N, d] Q24 i32
    let mult = b.c(DType::I64, k.mod_to_q24 as i128);
    let scale_q = b.mul(scale, mult, DType::I64);
    let shift_q = b.mul(shift, mult, DType::I64);
    let one = b.c(DType::I64, ONE);
    let factor = b.add(one, scale_q, DType::I64);
    let modulated = b.mul_q24(unit, factor, DType::I64);
    let sum = b.add(modulated, shift_q, DType::I64);
    let m = b.c(DType::I64, k.out.0 as i128);
    let s = b.c(DType::I8, k.out.1 as i128);
    b.narrow(sum, &Narrowing::new(m, s, None), -32_767, 32_767, DType::I16)
}

/// **`h + gate · f`**: `h:[N, d]` `i32` at `s_h`, `f:[N, d]` `i16` codes at `s_f`, `gate:[1, d]` codes at `2^-e`;
/// `ratio` is `mul_shift(2^-e · s_f / s_h)`. Saturates into `i32`.
pub fn lower_gated_residual(b: &mut BlockBuilder<'_>, h: Ref, f: Ref, gate: Ref, ratio: (i64, i8)) -> Ref {
    let prod = b.mul(gate, f, DType::I64);
    let m = b.c(DType::I64, ratio.0 as i128);
    let s = b.c(DType::I8, ratio.1 as i128);
    let delta = b.narrow(prod, &Narrowing::new(m, s, None), i32::MIN as i64, i32::MAX as i64, DType::I32);
    let sum = b.add(h, delta, DType::I64);
    b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

/// A plain residual add `a + b` of two `i32` streams at one scale, saturated.
pub fn lower_residual_add(b: &mut BlockBuilder<'_>, a: Ref, c: Ref) -> Ref {
    let sum = b.add(a, c, DType::I64);
    b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    #[test]
    fn modulated_layer_norm_is_the_float_one() {
        let mut rng = Lcg(33);
        let (n, d) = (3usize, 16usize);
        let (sx, so, mod_exp) = (1.0 / 2048.0, 1.0 / 4096.0, 12u32);
        let sm = 1.0 / (1u64 << mod_exp) as f64;
        let eps = 1e-6;
        let k = AdaNormConsts::new(d, sx, eps, mod_exp, so);
        let x: Vec<i128> = (0..n * d).map(|_| rng.range(-9_000, 9_000)).collect();
        let sc: Vec<i128> = (0..d).map(|_| rng.range(-3_000, 3_000)).collect();
        let sh: Vec<i128> = (0..d).map(|_| rng.range(-3_000, 3_000)).collect();
        let (x2, sc2, sh2) = (x.clone(), sc.clone(), sh.clone());
        let y = run_one_block(
            |pb, sink| {
                (
                    sink.put(pb, "x", DType::I16, &[n as u32, d as u32], x2),
                    sink.put(pb, "scale", DType::I16, &[1, d as u32], sc2),
                    sink.put(pb, "shift", DType::I16, &[1, d as u32], sh2),
                )
            },
            |b, (xs, scale, shift)| lower_ada_layer_norm(b, xs, scale, shift, &k),
        );
        for r in 0..n {
            let row: Vec<f64> = (0..d).map(|i| x[r * d + i] as f64 * sx).collect();
            let mean = row.iter().sum::<f64>() / d as f64;
            let var = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / d as f64;
            for i in 0..d {
                let ln = (row[i] - mean) / (var + eps).sqrt();
                let want = ln * (1.0 + sc[i] as f64 * sm) + sh[i] as f64 * sm;
                let got = y.data[r * d + i] as f64 * so;
                assert!((got - want).abs() <= 3.0 * so + 1e-3 * want.abs(), "[{r},{i}]: integer {got} vs float {want}");
            }
        }
    }

    #[test]
    fn the_gated_residual_adds_the_scaled_branch() {
        let mut rng = Lcg(35);
        let (n, d, mod_exp) = (2usize, 8usize, 12u32);
        let (sh_, sf) = (1.0 / 65_536.0, 1.0 / 4096.0);
        let sg = 1.0 / (1u64 << mod_exp) as f64;
        let ratio = mul_shift(sg * sf / sh_);
        let h: Vec<i128> = (0..n * d).map(|_| rng.range(-1_000_000, 1_000_000)).collect();
        let f: Vec<i128> = (0..n * d).map(|_| rng.range(-20_000, 20_000)).collect();
        let g: Vec<i128> = (0..d).map(|_| rng.range(-4_000, 4_000)).collect();
        let (h2, f2, g2) = (h.clone(), f.clone(), g.clone());
        let y = run_one_block(
            |pb, sink| {
                (
                    sink.put(pb, "h", DType::I32, &[n as u32, d as u32], h2),
                    sink.put(pb, "f", DType::I16, &[n as u32, d as u32], f2),
                    sink.put(pb, "g", DType::I16, &[1, d as u32], g2),
                )
            },
            |b, (hs, fs, gs)| lower_gated_residual(b, hs, fs, gs, ratio),
        );
        for r in 0..n {
            for i in 0..d {
                let want = h[r * d + i] as f64 * sh_ + g[i] as f64 * sg * f[r * d + i] as f64 * sf;
                let got = y.data[r * d + i] as f64 * sh_;
                assert!((got - want).abs() <= 1.5 * sh_ + 1e-9, "[{r},{i}]: {got} vs {want}");
            }
        }
    }
}
