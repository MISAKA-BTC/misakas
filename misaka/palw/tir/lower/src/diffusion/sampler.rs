//! **The sampler and the latent's layout** (RFC-0003 §II.2–3): the flow-matching Euler schedule as pinned tables,
//! the initial latent selected at the first position, `GEN_SAMPLER_AFFINE_V1`'s update, and the unpatchify that turns
//! the denoiser's token rows back into the canonical `[C, H, W]` latent.
//!
//! * **The schedule** ([`flow_match_schedule`]) is diffusers' `FlowMatchEulerDiscreteScheduler.set_timesteps` for
//!   the default configuration (static `shift`, no dynamic shifting, no Karras/exponential/beta sigmas), evaluated
//!   at registration for every offered step count: the timesteps `σ·1000` the sinusoid embeds, the sigmas, and the
//!   Euler coefficients `Δσ_i = σ_{i+1} − σ_i` in Q24 (`i32`), laid out by `(steps index, position)` like the
//!   timestep table ([`super::embed::TimestepTable`]; one `base[steps] + pos` row index serves both).
//! * **The initial latent** is the stage's `Random` input (`PALW_GAUSS_Q24_V1` words, Q24): rescaled to the latent's
//!   fixed point `2^-q` by `Div_HAFZ(noise, 2^(24−q))` and selected at position 0 (`x = Select(pos == 0, noise,
//!   State(latent))`: only the chosen operand is read, so later steps never touch the noise).
//! * **The update** `x' = StateWrite(x + HAFZ(Δσ_i · v / 2^24 · s_v / s_x))`: one narrowing, one lossy site.
//! * **Unpatchify** is one `Gather` by a pinned index table (rank 4 is the IR's limit, so diffusers'
//!   `nhwpqc→nchpwq` einsum is not a `Reshape`/`Transpose` pair): `[N, p·p·C]` rows to `[C, gh·p, gw·p]`.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::program::INPUT_POS;
use misaka_palw_tir::{Cmp, DType, Ref, Rounding};

use super::sink::ParamSink;

/// diffusers' `FlowMatchEulerDiscreteScheduler.set_timesteps(steps)` for `num_train_timesteps = n_train` and a static
/// `shift`: `(timesteps, sigmas)`, `steps` timesteps and `steps + 1` sigmas (a trailing zero).
///
/// The scheduler's own construction shifts its training sigmas (`σ = shift·s / (1 + (shift − 1)·s)` for
/// `s = k/n_train`), takes `σ_max = 1` and `σ_min = σ(1/n_train)` from them, and `set_timesteps` lays `steps` timesteps
/// linearly from `σ_max·n_train` down to `σ_min·n_train`, divides by `n_train` and shifts AGAIN — the two shifts are
/// the scheduler's, and the golden table of the fixture (`sigmas.json`, the scheduler's own) is what this must equal.
pub fn flow_match_schedule(steps: u32, shift: f64, n_train: f64) -> (Vec<f64>, Vec<f64>) {
    let shifted = |s: f64| shift * s / (1.0 + (shift - 1.0) * s);
    let sigma_max = shifted(1.0);
    let sigma_min = shifted(1.0 / n_train);
    let (t0, t1) = (sigma_max * n_train, sigma_min * n_train);
    let n = steps as usize;
    let lin = |i: usize| if n == 1 { t0 } else { t0 + (t1 - t0) * i as f64 / (n - 1) as f64 };
    let mut sigmas: Vec<f64> = (0..n).map(|i| shifted(lin(i) / n_train)).collect();
    let timesteps: Vec<f64> = sigmas.iter().map(|s| s * n_train).collect();
    sigmas.push(0.0);
    (timesteps, sigmas)
}

/// The sampler's tables for every offered step count.
#[derive(Clone, Debug)]
pub struct SamplerTables {
    pub counts: Vec<u32>,
    /// The scheduler's timesteps per count.
    pub timesteps: Vec<Vec<f64>>,
    /// The scheduler's sigmas per count (`steps + 1` each).
    pub sigmas: Vec<Vec<f64>>,
    /// `[rows]`: row `base[s] + i` is `round((σ_{i+1} − σ_i) · 2^24)` of count `s` (the same layout as the timestep table).
    pub dsigma_q24: Vec<i32>,
}

impl SamplerTables {
    pub fn new(counts: &[u32], shift: f64, n_train: f64) -> Self {
        assert!(counts.windows(2).all(|w| w[0] < w[1]) && !counts.is_empty(), "ascending, non-empty step counts");
        let (mut timesteps, mut sigmas, mut dsigma_q24) = (Vec::new(), Vec::new(), Vec::new());
        for c in counts {
            let (t, s) = flow_match_schedule(*c, shift, n_train);
            for i in 0..*c as usize {
                dsigma_q24.push(((s[i + 1] - s[i]) * (1u64 << 24) as f64).round() as i32);
            }
            timesteps.push(t);
            sigmas.push(s);
        }
        Self { counts: counts.to_vec(), timesteps, sigmas, dsigma_q24 }
    }

    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> Ref {
        sink.put(
            pb,
            &format!("{name}.dsigma"),
            DType::I32,
            &[self.dsigma_q24.len() as u32],
            self.dsigma_q24.iter().map(|v| *v as i128).collect(),
        )
    }
}

/// **The initial latent at position 0**: `noise:[C, H, W]` Q24 (`i32`) rescaled to `2^-q_lat` by `Div_HAFZ`, selected
/// where `pos == 0`, else `state` (the latent `Fixed` state's value at the start of this position).
pub fn lower_initial_latent(b: &mut BlockBuilder<'_>, noise: Ref, state: Ref, q_lat: u32) -> Ref {
    assert!((1..=24).contains(&q_lat), "a latent fixed point of 2^-{q_lat}");
    let scaled = b.shr(noise, 24 - q_lat, Rounding::HalfAwayFromZero, DType::I32);
    let zero = b.c(DType::Idx, 0);
    let first = b.compare(Ref::Input(INPUT_POS), zero, Cmp::Eq);
    b.select(first, scaled, state, DType::I32)
}

/// **`GEN_SAMPLER_AFFINE_V1`**: `x:[C, H, W]` `i32` at `s_x`, the velocity `v` (`i16` codes at `s_v`, same shape),
/// `dsigma` this position's Q24 coefficient (a scalar `i32`) and `ratio = mul_shift(s_v / 2^24 / s_x)`. Writes and
/// returns the new latent.
pub fn lower_euler_step(b: &mut BlockBuilder<'_>, x: Ref, v: Ref, dsigma: Ref, ratio: (i64, i8), state: u16) -> Ref {
    let p = b.mul(v, dsigma, DType::I64);
    let m = b.c(DType::I64, ratio.0 as i128);
    let s = b.c(DType::I8, ratio.1 as i128);
    let delta = b.narrow(p, &Narrowing::new(m, s, None), i32::MIN as i64, i32::MAX as i64, DType::I32);
    let sum = b.add(x, delta, DType::I64);
    let next = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
    b.state_write(state, next)
}

/// The pinned table of the unpatchify gather: for each element `(c, H, W)` of the `[C, gh·p, gw·p]` latent (row-major),
/// the flat index into the rows `[gh·gw, p·p·C]` it reads — diffusers' `nhwpqc → nchpwq`.
pub fn unpatchify_index(c: usize, gh: usize, gw: usize, p: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(c * gh * p * gw * p);
    for ch in 0..c {
        for hh in 0..gh * p {
            for ww in 0..gw * p {
                let (h, pi, w, qi) = (hh / p, hh % p, ww / p, ww % p);
                let n = h * gw + w;
                out.push((n * p * p * c + (pi * p + qi) * c + ch) as u32);
            }
        }
    }
    out
}

/// **Unpatchify by static maps**: `rows:[gh·gw, p·p·C]` (element `(h·gw + w, (pi·p + qi)·C + ch)`) to `[C, gh·p, gw·p]`
/// (element `(ch, h·p + pi, w·p + qi)`) — diffusers' `nhwpqc → nchpwq`: `[gh, gw, p, p·C]` → `[gh, p, gw, p·C]` →
/// `[gh·p, gw, p, C]` → `[C, gh·p, gw, p]`, read as `[C, gh·p, gw·p]`. No table (see [`super::conv`] on why).
pub fn lower_unpatchify(b: &mut BlockBuilder<'_>, rows: Ref, c: usize, gh: usize, gw: usize, p: usize) -> Ref {
    let (c, gh, gw, p) = (c as u32, gh as u32, gw as u32, p as u32);
    let r = b.reshape_fixed(rows, &[gh, gw, p, p * c]);
    let r = b.transpose(r, &[0, 2, 1, 3]); // [gh, p, gw, p·C]
    let r = b.reshape_fixed(r, &[gh * p, gw, p, c]);
    let r = b.transpose(r, &[3, 0, 1, 2]); // [C, gh·p, gw, p]
    b.reshape_fixed(r, &[c, gh * p, gw * p])
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block, run_steps};
    use super::*;
    use crate::quant::mul_shift;

    #[test]
    fn the_schedule_runs_from_one_to_zero_in_the_scheduler_s_shape() {
        for steps in [2u32, 4, 28] {
            let (t, s) = flow_match_schedule(steps, 3.0, 1000.0);
            assert_eq!((t.len(), s.len()), (steps as usize, steps as usize + 1));
            assert!((s[0] - 1.0).abs() < 1e-12 && (t[0] - 1000.0).abs() < 1e-9, "starts at sigma 1");
            assert_eq!(*s.last().unwrap(), 0.0);
            assert!(s.windows(2).all(|w| w[0] > w[1]), "strictly decreasing");
            for i in 0..steps as usize {
                assert!((t[i] - s[i] * 1000.0).abs() < 1e-9, "the timestep is sigma x n_train");
            }
        }
        // One step: sigma_0 = 1, then 0.
        let (t, s) = flow_match_schedule(1, 3.0, 1000.0);
        assert_eq!((t, s), (vec![1000.0], vec![1.0, 0.0]));
    }

    /// The scheduler's own table (the fixture's `sigmas.json`, written by `tools/gen_diffusers_sd3_fixture.py
    /// scheduler`) is what [`flow_match_schedule`] must reproduce; the file is a generated fixture and a checkout
    /// without it skips the comparison.
    #[test]
    fn the_schedule_equals_the_schedulers_own_when_the_fixture_is_present() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hf-diff/sd3_tiny/scheduler/sigmas.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let (shift, n_train) = (v["shift"].as_f64().unwrap(), v["num_train_timesteps"].as_f64().unwrap());
        for (steps, rec) in v["steps"].as_object().unwrap() {
            let (t, s) = flow_match_schedule(steps.parse().unwrap(), shift, n_train);
            let got_s: Vec<f64> = rec["sigmas"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            let got_t: Vec<f64> = rec["timesteps"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            // The scheduler computes in float32 on the host; the table keeps 64-bit values of those.
            for (a, b) in s.iter().zip(&got_s).chain(t.iter().zip(&got_t).map(|(a, b)| (a, b))) {
                assert!((a - b).abs() <= 1e-4 * a.abs().max(1.0), "steps {steps}: {a} vs the scheduler's {b}");
            }
        }
    }

    #[test]
    fn the_dsigma_table_is_laid_out_by_count_and_position() {
        let t = SamplerTables::new(&[2, 4], 3.0, 1000.0);
        assert_eq!(t.dsigma_q24.len(), 6);
        for (k, c) in [2usize, 4].iter().enumerate() {
            let base: usize = if k == 0 { 0 } else { 2 };
            for i in 0..*c {
                let want = ((t.sigmas[k][i + 1] - t.sigmas[k][i]) * (1u64 << 24) as f64).round() as i32;
                assert_eq!(t.dsigma_q24[base + i], want);
                assert!(t.dsigma_q24[base + i] < 0, "sigma falls");
            }
            // The steps sum to -1 (sigma_0 = 1 down to 0).
            let sum: i64 = t.dsigma_q24[base..base + c].iter().map(|v| *v as i64).sum();
            assert!((sum + (1 << 24)).abs() <= *c as i64, "the coefficients sum to -sigma_0");
        }
    }

    /// The static unpatchify is diffusers' `nhwpqc → nchpwq`, and the table the float reference reads.
    #[test]
    fn unpatchify_is_the_einsum_nhwpqc_to_nchpwq() {
        let (c, gh, gw, p) = (3usize, 2usize, 3usize, 2usize);
        let n = gh * gw;
        // Rows whose entry names (token, p, q, c).
        let rows: Vec<i128> = (0..n * p * p * c).map(|i| i as i128).collect();
        let r2 = rows.clone();
        let y = run_one_block(
            |pb, sink| sink.put(pb, "rows", DType::I32, &[n as u32, (p * p * c) as u32], r2),
            |b, r| lower_unpatchify(b, r, c, gh, gw, p),
        );
        assert_eq!(y.shape, vec![c, gh * p, gw * p]);
        for ch in 0..c {
            for hh in 0..gh * p {
                for ww in 0..gw * p {
                    let (h, pi, w, qi) = (hh / p, hh % p, ww / p, ww % p);
                    // x[n, h, w, p, q, c] with n = h * gw + w flattened as rows[(h * gw + w), (pi * p + qi) * C + c].
                    let want = ((h * gw + w) * p * p * c + (pi * p + qi) * c + ch) as i128;
                    assert_eq!(y.data[(ch * gh * p + hh) * gw * p + ww], want, "({ch},{hh},{ww})");
                }
            }
        }
    }

    #[test]
    fn the_euler_loop_selects_the_noise_once_and_then_follows_the_state() {
        let mut rng = Lcg(71);
        let (c, h, w) = (2usize, 2usize, 2usize);
        let len = c * h * w;
        let (q_lat, s_v) = (20u32, 1.0 / 8192.0);
        let s_x = 1.0 / (1u64 << q_lat) as f64;
        let counts = [2u32, 4];
        let tables = SamplerTables::new(&counts, 3.0, 1000.0);
        let noise: Vec<i128> = (0..len).map(|_| rng.range(-60_000_000, 60_000_000)).collect();
        let v: Vec<i128> = (0..len).map(|_| rng.range(-20_000, 20_000)).collect();
        let ratio = mul_shift(s_v / (1u64 << 24) as f64 / s_x);
        for (k, steps) in counts.iter().enumerate() {
            let base = if k == 0 { 0u32 } else { 2 };
            let (n2, v2, t2) = (noise.clone(), v.clone(), tables.clone());
            let xs = run_steps(
                |pb, sink| {
                    let noise = sink.put(pb, "noise", DType::I32, &[c as u32, h as u32, w as u32], n2);
                    let vel = sink.put(pb, "v", DType::I16, &[c as u32, h as u32, w as u32], v2);
                    let dsig = t2.declare(pb, sink, "smp");
                    let st =
                        pb.fixed_state("latent", DType::I32, &[c as u32, h as u32, w as u32], i32::MIN as i64, i32::MAX as i64, true);
                    (noise, vel, dsig, st)
                },
                |b, (noise, vel, dsig, st)| {
                    let x = lower_initial_latent(b, noise, Ref::State(st), q_lat);
                    let off = b.c(DType::I64, base as i128);
                    let at = b.add(off, Ref::Input(INPUT_POS), DType::I64);
                    let at = b.clamp(at, 0, 5, DType::Idx);
                    let d = b.gather(dsig, at, 0, 0);
                    lower_euler_step(b, x, vel, d, ratio, st)
                },
                *steps as usize,
            );
            let mut x: Vec<f64> = noise.iter().map(|n| *n as f64 / (1u64 << 24) as f64).collect();
            for (i, got) in xs.iter().enumerate() {
                let ds = (tables.sigmas[k][i + 1] - tables.sigmas[k][i]) as f64;
                for e in 0..len {
                    x[e] += ds * v[e] as f64 * s_v;
                    let g = got.data[e] as f64 * s_x;
                    assert!((g - x[e]).abs() <= 3.0 * s_x + 2e-6, "count {steps} step {i} elem {e}: {g} vs {}", x[e]);
                }
                // Follow the integer state, not the float: resync so the check stays local to one step's rounding.
                for e in 0..len {
                    x[e] = got.data[e] as f64 * s_x;
                }
            }
        }
    }
}
