//! **Pinned tables** — registration-time data (PALW-EX-5: a transcendental at registration is data, not a
//! primitive). Each is computed once, in floating point, when a class is lowered, rounded to integers and carried
//! as a param or a const; the integer program only gathers from it.

/// SiLU, `x · σ(x)`.
pub fn silu(x: f64) -> f64 {
    x / (1.0 + (-x).exp())
}

/// GELU with the tanh approximation (`gelu_pytorch_tanh`, SD3's feed-forward activation).
pub fn gelu_tanh(x: f64) -> f64 {
    0.5 * x * (1.0 + (0.797_884_560_802_865_4 * (x + 0.044_715 * x * x * x)).tanh())
}

/// A 65,536-entry table of `i16` codes: the entry at `c + 32768` is `f(c · in_scale) / out_scale`, rounded half
/// away from zero and clamped to `±32767` — an activation as data on the A16 grid
/// (`BlockBuilder::act_table`).
pub fn act_table_i16(f: impl Fn(f64) -> f64, in_scale: f64, out_scale: f64) -> Vec<i16> {
    (0..65_536u32)
        .map(|i| {
            let code = i as i64 - 32_768;
            let v = f(code as f64 * in_scale) / out_scale;
            v.round().clamp(-32_767.0, 32_767.0) as i16
        })
        .collect()
}

/// The same table in Q24 (`i32`): the entry at `c + 32768` is `f(c · in_scale) · 2^24`, rounded and clamped.
pub fn act_table_q24(f: impl Fn(f64) -> f64, in_scale: f64) -> Vec<i32> {
    (0..65_536u32)
        .map(|i| {
            let code = i as i64 - 32_768;
            (f(code as f64 * in_scale) * (1u64 << 24) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
        })
        .collect()
}

/// **diffusers' `get_timestep_embedding`** for one timestep: `dim` values, `[sin(t·ω_i)…, cos(t·ω_i)…]` with
/// `ω_i = exp(−ln(max_period) · i / (dim/2 − downscale_freq_shift))`, the halves swapped when `flip_sin_to_cos`
/// (SD3: `dim = 256`, flipped, shift 0, `max_period = 10000`). An odd `dim` is padded with one zero.
pub fn timestep_embedding(t: f64, dim: usize, flip_sin_to_cos: bool, downscale_freq_shift: f64, max_period: f64) -> Vec<f64> {
    let half = dim / 2;
    let mut sin = Vec::with_capacity(half);
    let mut cos = Vec::with_capacity(half);
    for i in 0..half {
        let omega = (-max_period.ln() * i as f64 / (half as f64 - downscale_freq_shift)).exp();
        sin.push((t * omega).sin());
        cos.push((t * omega).cos());
    }
    let mut out = if flip_sin_to_cos { [cos, sin].concat() } else { [sin, cos].concat() };
    if dim % 2 == 1 {
        out.push(0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activations_are_the_textbook_ones() {
        assert_eq!(silu(0.0), 0.0);
        assert!((silu(1.0) - 0.731_058_578_630_004_9).abs() < 1e-12);
        assert!((silu(-1.0) + 0.268_941_421_369_995_1).abs() < 1e-12);
        assert_eq!(gelu_tanh(0.0), 0.0);
        assert!((gelu_tanh(1.0) - 0.841_191_990_608_276_8).abs() < 1e-9);
        // gelu_tanh(x) - x → 0 for large x, and → 0 for large negative x.
        assert!((gelu_tanh(8.0) - 8.0).abs() < 1e-9 && gelu_tanh(-8.0).abs() < 1e-9);
    }

    #[test]
    fn a_table_is_the_function_on_the_code_grid() {
        let (sx, sy) = (1.0 / 1024.0, 1.0 / 2048.0);
        let t = act_table_i16(silu, sx, sy);
        assert_eq!(t.len(), 65_536);
        assert_eq!(t[32_768], 0, "f(0) = 0");
        for c in [-30_000i64, -1024, -1, 1, 777, 30_000] {
            let want = (silu(c as f64 * sx) / sy).round().clamp(-32_767.0, 32_767.0) as i16;
            assert_eq!(t[(c + 32_768) as usize], want, "code {c}");
        }
        let q = act_table_q24(gelu_tanh, sx);
        assert_eq!(q[32_768], 0);
        let c = 2048i64;
        assert_eq!(
            q[(c + 32_768) as usize] as f64 / (1u64 << 24) as f64,
            (gelu_tanh(c as f64 * sx) * (1u64 << 24) as f64).round() / (1u64 << 24) as f64
        );
    }

    #[test]
    fn the_timestep_sinusoid_is_diffusers() {
        // dim 4, flipped, shift 0: [cos(t), cos(t·0.01), sin(t), sin(t·0.01)].
        let e = timestep_embedding(2.0, 4, true, 0.0, 10_000.0);
        let w = [2.0f64.cos(), (2.0 * 0.01f64).cos(), 2.0f64.sin(), (2.0 * 0.01f64).sin()];
        for (a, b) in e.iter().zip(w) {
            assert!((a - b).abs() < 1e-12, "{e:?}");
        }
        // Unflipped swaps the halves; t = 0 is cos = 1, sin = 0.
        let u = timestep_embedding(0.0, 8, false, 1.0, 10_000.0);
        assert_eq!(&u[..4], &[0.0; 4]);
        assert_eq!(&u[4..], &[1.0; 4]);
        // An odd width pads with one zero.
        assert_eq!(timestep_embedding(1.0, 5, true, 0.0, 10_000.0).len(), 5);
    }
}
