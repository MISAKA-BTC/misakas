//! **Quantisation arithmetic** shared by the lowering (Gate 2a): multiplier/shift pairs, per-row
//! weight codes, and code scales from calibration statistics.
//!
//! Everything here runs at registration time, in floating point, and produces the INTEGERS that
//! become TIR params: once an artifact exists, no float is left anywhere. The rounding of a float to
//! an integer code is half away from zero (`f64::round`), the same rule the IR's
//! `Div(HalfAwayFromZero)` applies at run time.

use rayon::prelude::*;

/// A float ratio as an integer pair: `r ≈ m / 2^s`, `s ∈ [0, 62]`.
///
/// The mantissa is normalised into `[2^30, 2^31)` whenever the shift range allows it — 30
/// significant bits, far below anything the codes can resolve — so `x · m` stays well inside the
/// narrowing's `i128` product for every `i64` accumulator. A ratio below `2^−62` rounds to `m = 0`
/// (the output is then zero, which is what such a ratio means at code resolution); one above
/// `2^31` keeps `s = 0` and an integer mantissa.
pub fn mul_shift(r: f64) -> (i64, i8) {
    if !r.is_finite() || r == 0.0 {
        return (0, 0);
    }
    let a = r.abs();
    let e = a.log2().floor() as i32;
    let s = (30 - e).clamp(0, 62);
    let m = (a * 2f64.powi(s)).round().min((1u64 << 62) as f64) as i64;
    (if r < 0.0 { -m } else { m }, s as i8)
}

/// The value an `(m, s)` pair represents.
pub fn ratio_of(m: i64, s: i8) -> f64 {
    m as f64 / 2f64.powi(s as i32)
}

/// Codes of a `[rows, cols]` matrix at one scale per ROW (per output channel): the row's absmax
/// maps to ±127. `col_scale`, when given, multiplies column `c` first — the smoothing migration of
/// a per-channel activation scale into the weights.
#[derive(Clone, Debug)]
pub struct RowCodes {
    pub rows: usize,
    pub cols: usize,
    pub codes: Vec<i8>,
    /// Float value of one code, per row. An all-zero row has scale 1 and zero codes.
    pub scales: Vec<f64>,
}

/// The scale of one row's per-row codes: its absmax over `code_max` (127 for `i8` codes, 32767 for
/// `i16`); 1 for an all-zero or non-finite row. The one definition [`quantize_rows`],
/// [`quantize_rows16`] and [`row_scales`] share, so a scale computed without the codes is the scale
/// the codes were made with.
pub fn row_scale(row: &[f32], code_max: f64) -> f64 {
    let amax = row.iter().fold(0f64, |m, v| m.max((*v as f64).abs()));
    if amax == 0.0 || !amax.is_finite() { 1.0 } else { amax / code_max }
}

/// The per-row scales of a `[rows, cols]` block, without its codes.
pub fn row_scales(w: &[f32], rows: usize, cols: usize, code_max: f64) -> Vec<f64> {
    assert_eq!(w.len(), rows * cols, "row_scales: {} values for [{rows}, {cols}]", w.len());
    w.par_chunks(cols.max(1)).map(|row| row_scale(row, code_max)).collect()
}

pub fn quantize_rows(w: &[f32], rows: usize, cols: usize, col_scale: Option<&[f64]>) -> RowCodes {
    assert_eq!(w.len(), rows * cols, "quantize_rows: {} values for [{rows}, {cols}]", w.len());
    let per_row: Vec<(Vec<i8>, f64)> = w
        .par_chunks(cols.max(1))
        .map(|row| {
            let v = |c: usize| row[c] as f64 * col_scale.map_or(1.0, |s| s[c]);
            let amax = (0..cols).fold(0f64, |m, c| m.max(v(c).abs()));
            if amax == 0.0 || !amax.is_finite() {
                return (vec![0i8; cols], 1.0);
            }
            let scale = amax / 127.0;
            debug_assert!(col_scale.is_some() || scale == row_scale(row, 127.0));
            ((0..cols).map(|c| (v(c) / scale).round().clamp(-127.0, 127.0) as i8).collect(), scale)
        })
        .collect();
    let mut codes = Vec::with_capacity(rows * cols);
    let mut scales = Vec::with_capacity(rows);
    for (c, s) in per_row {
        codes.extend_from_slice(&c);
        scales.push(s);
    }
    RowCodes { rows, cols, codes, scales }
}

/// Per-row `i16` codes of a table (`scale = absmax / 32767` per row) — what a gathered table
/// (an embedding, and the head that reads the same tensor) is stored as: a lookup costs no MAC, and
/// an embedding's rows carry outliers that per-row `i8` codes cannot hold (Mamba-370m's tied
/// embedding: 4.2 % relative weight error at 8 bits, the whole of its fidelity loss).
#[derive(Clone, Debug, PartialEq)]
pub struct RowCodes16 {
    pub rows: usize,
    pub cols: usize,
    pub codes: Vec<i16>,
    /// Float value of one code, per row. An all-zero row has scale 1 and zero codes.
    pub scales: Vec<f64>,
}

pub fn quantize_rows16(w: &[f32], rows: usize, cols: usize) -> RowCodes16 {
    assert_eq!(w.len(), rows * cols, "quantize_rows16: {} values for [{rows}, {cols}]", w.len());
    let per_row: Vec<(Vec<i16>, f64)> = w
        .par_chunks(cols.max(1))
        .map(|row| {
            let amax = row.iter().fold(0f64, |m, v| m.max((*v as f64).abs()));
            if amax == 0.0 || !amax.is_finite() {
                return (vec![0i16; cols], 1.0);
            }
            let scale = amax / 32767.0;
            debug_assert_eq!(scale, row_scale(row, 32767.0));
            (row.iter().map(|v| (*v as f64 / scale).round().clamp(-32767.0, 32767.0) as i16).collect(), scale)
        })
        .collect();
    let mut codes = Vec::with_capacity(rows * cols);
    let mut scales = Vec::with_capacity(rows);
    for (c, s) in per_row {
        codes.extend_from_slice(&c);
        scales.push(s);
    }
    RowCodes16 { rows, cols, codes, scales }
}

/// How calibration statistics become code scales.
#[derive(Clone, Debug, PartialEq)]
pub struct QuantPolicy {
    /// Headroom over the calibrated absmax for `i16` codes: the absmax lands at `32767 / h`.
    pub headroom16: f64,
    /// Headroom for `i32` values (logits, wide sites): they have bits to spare.
    pub headroom32: f64,
    /// Headroom for the residual stream's single `i32` scale.
    pub headroom_resid: f64,
}

impl Default for QuantPolicy {
    fn default() -> Self {
        Self { headroom16: 2.0, headroom32: 4.0, headroom_resid: 4.0 }
    }
}

pub const CODE16_MAX: f64 = 32767.0;
pub const CODE32_MAX: f64 = 2147483647.0;

/// The float value of one code for a site whose calibrated absmax is `amax`.
pub fn code_scale(amax: f64, code_max: f64, headroom: f64) -> f64 {
    // A site that was exactly zero on the calibration set (a zero bias, an unused lane) still
    // needs a positive scale; any one represents zero, and a fine one keeps the ratios sane.
    let a = if amax.is_finite() && amax > 0.0 { amax } else { 1e-6 };
    a * headroom / code_max
}

/// `eps · 2^24 / unit²` as the `(eps_zero, eps_shift)` pair of the wide RMS template:
/// `eps_zero · 2^eps_shift`, `eps_zero ≤ 2^30`, `eps_shift ∈ [0, 96]`.
pub fn eps_pair(eps_q: f64) -> (i64, i8) {
    if !(eps_q.is_finite() && eps_q > 0.0) {
        return (0, 0);
    }
    let mut shift = 0i32;
    while eps_q / 2f64.powi(shift) > (1u64 << 30) as f64 && shift < 96 {
        shift += 1;
    }
    let zero = (eps_q / 2f64.powi(shift)).round().clamp(0.0, (1u64 << 30) as f64) as i64;
    (zero, shift as i8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_shift_keeps_thirty_bits() {
        for r in [1.0, 0.5, 3.75e-3, 1.234e-9, 7.0e5, -0.03125] {
            let (m, s) = mul_shift(r);
            let back = ratio_of(m, s);
            assert!(((back - r) / r).abs() < 1e-8, "{r} → ({m}, {s}) → {back}");
            assert!((0..=62).contains(&s));
        }
        // Below 2^−32 the shift saturates at 62 and the mantissa shrinks with the ratio.
        let (m, s) = mul_shift(2.5e-15);
        assert_eq!(s, 62);
        assert!(((ratio_of(m, s) - 2.5e-15) / 2.5e-15).abs() < 1e-4);
        assert_eq!(mul_shift(0.0), (0, 0));
    }

    #[test]
    fn rows_map_their_absmax_to_127() {
        let w = [0.5f32, -1.0, 0.25, 0.0, 0.0, 0.0, 2.0, 1.0, -0.5];
        let q = quantize_rows(&w, 3, 3, None);
        assert_eq!(&q.codes[0..3], &[64, -127, 32]);
        assert_eq!(&q.codes[3..6], &[0, 0, 0]);
        assert_eq!(q.scales[1], 1.0);
        assert_eq!(&q.codes[6..9], &[127, 64, -32]);
        let s = quantize_rows(&w, 3, 3, Some(&[2.0, 1.0, 1.0]));
        assert_eq!(&s.codes[0..3], &[127, -127, 32]);
    }

    #[test]
    fn a_scale_without_the_codes_is_the_scale_with_them() {
        let w: Vec<f32> = (0..96).map(|i| ((i * 37 % 101) as f32 - 50.0) * if i % 7 == 0 { 0.0 } else { 0.013 }).collect();
        let mut z = w.clone();
        z[32..64].iter_mut().for_each(|v| *v = 0.0);
        for w in [w, z] {
            let (q8, q16) = (quantize_rows(&w, 3, 32, None), quantize_rows16(&w, 3, 32));
            assert_eq!(row_scales(&w, 3, 32, 127.0), q8.scales);
            assert_eq!(row_scales(&w, 3, 32, 32767.0), q16.scales);
            // Blocks of rows give the same scales as the whole.
            let blocked: Vec<f64> = (0..3).flat_map(|r| row_scales(&w[r * 32..(r + 1) * 32], 1, 32, 127.0)).collect();
            assert_eq!(blocked, q8.scales);
        }
    }

    #[test]
    fn eps_pairs_reassemble() {
        for e in [3.0, 1.0e6, 7.2e10, 1.8e13, 4.0e30] {
            let (z, s) = eps_pair(e);
            let back = z as f64 * 2f64.powi(s as i32);
            assert!(((back - e) / e).abs() < 1e-8, "{e}: {z}·2^{s}");
            assert!(z <= 1 << 30);
        }
    }
}
