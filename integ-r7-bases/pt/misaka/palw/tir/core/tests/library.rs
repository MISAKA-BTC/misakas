//! **`tir_library_v1`, template by template**: every template builds, validates, is ADMISSIBLE
//! under the range rules of spec 04b §7 with its params at their full dtype ranges, evaluates, and
//! agrees with a float reference of the function it stands for within a stated tolerance (or, for
//! the exact ones — selections, head maps, the conv window — with an integer reference exactly).
//!
//! Byte identity of the LEGACY templates with the live kernels is the conformance crate's
//! (`misaka-palw-tir-conformance`); here they appear only where a new template is built on them.

#![allow(clippy::needless_range_loop)]

mod common;

use common::Lcg;
use misaka_palw_tir::arith::{K, LN2_Q, ONE};
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::library::act::q24;
use misaka_palw_tir::library::attn::{AttnCfg, MlaCfg};
use misaka_palw_tir::library::moe::GroupScore;
use misaka_palw_tir::library::recur::{ScanCfg, Wkv6Cfg, Wkv7Cfg};
use misaka_palw_tir::library::rope::{AngleSet, RopeStyle};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS};
use misaka_palw_tir::{DType, Dim, Interpreter, MapParams, Ref, RunState, Tensor, TensorType, TirProgramV1};

// ---- the harness -------------------------------------------------------------------------------

struct P {
    name: &'static str,
    dtype: DType,
    shape: Vec<u32>,
    values: Vec<i128>,
}

fn p(name: &'static str, dtype: DType, shape: &[u32], values: Vec<i128>) -> P {
    assert_eq!(values.len(), shape.iter().map(|d| *d as usize).product::<usize>(), "{name}");
    P { name, dtype, shape: shape.to_vec(), values }
}

enum S {
    Fixed(&'static str, DType, Vec<u32>, i64, i64),
    Hist(&'static str, DType, Vec<u32>, u32),
}

struct Run {
    #[allow(dead_code)]
    program: TirProgramV1,
    /// The observed node's value at each position.
    outs: Vec<Tensor>,
}

/// A program whose ONE layer block runs `build` once per position, over global params and
/// per-layer states; the node `build` returns is committed (narrowed to `i32` first if it is not
/// committable) and observed at every position. The program must validate and be admissible.
fn run_layer(params: Vec<P>, states: Vec<S>, positions: u32, build: impl FnOnce(&mut BlockBuilder<'_>, &[Ref], &[u16]) -> Ref) -> Run {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let refs: Vec<Ref> = params.iter().map(|a| pb.param(a.name, a.dtype, &a.shape, false)).collect();
    let sts: Vec<u16> = states
        .iter()
        .map(|s| match s {
            S::Fixed(n, dt, sh, lo, hi) => pb.fixed_state(n, *dt, sh, *lo, *hi, true),
            S::Hist(n, dt, row, w) => pb.hist_state(n, *dt, row, *w, true),
        })
        .collect();
    let carry = TensorType::fixed(DType::I16, &[1]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let c = b.iota(DType::I16, &[Dim::Fixed(1)], 0, 0, 0);
        b.finish(&[c])
    };
    let (layer, node) = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let r = build(&mut b, &refs, &sts);
        let dt = b.ty(r).dtype;
        let r = if dt.committable() { r } else { b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32) };
        b.commit(r);
        let Ref::Node(i) = r else { panic!("the observed value is a node") };
        let c = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        (b.finish(&[c]), i)
    };
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    let program = pb.finish(pre, vec![layer], post, 0);
    let interp = Interpreter::new(&program).unwrap_or_else(|e| panic!("the program validates: {e}"));
    if let Err(e) = analyze_ranges(&program) {
        panic!("the program is admissible under 04b §7 at full param ranges: {e}");
    }
    let mut mp = MapParams::default();
    for (j, a) in params.iter().enumerate() {
        let shape = a.shape.iter().map(|d| *d as usize).collect();
        mp.tensors.insert((j as u16, None), Tensor::new(a.dtype, shape, a.values.clone()).expect("a value of its dtype"));
    }
    let mut st = RunState::default();
    let mut outs = Vec::new();
    for _ in 0..positions {
        let step = interp.step(&mp, &mut st, 0).unwrap_or_else(|e| panic!("step {}: {e}", st.pos));
        let v = step.commits.iter().find(|c| c.block == layer && c.node == node).expect("the observed node is committed");
        outs.push(v.value.clone());
    }
    Run { program, outs }
}

/// A one-position evaluation of a stateless template over rank-1 params.
fn eval1(params: Vec<P>, build: impl FnOnce(&mut BlockBuilder<'_>, &[Ref]) -> Ref) -> Vec<i128> {
    run_layer(params, vec![], 1, |b, r, _| build(b, r)).outs.remove(0).data
}

fn f(v: i128) -> f64 {
    v as f64 / ONE as f64
}

fn q24v(xs: &[f64]) -> Vec<i128> {
    xs.iter().map(|v| q24(*v)).collect()
}

fn narrowing(b: &mut BlockBuilder<'_>, m: i128, s: i128) -> Narrowing {
    let m = b.c(DType::I64, m);
    let s = b.c(DType::I8, s);
    Narrowing::new(m, s, None)
}

/// `erf` to ~1e-12 on the whole line: the Taylor series below 2.5, the asymptotic expansion of
/// `erfc` above (a test reference; the template under test uses Abramowitz–Stegun 7.1.26).
fn erf(z: f64) -> f64 {
    let a = z.abs();
    let v = if a < 2.5 {
        let mut term = a;
        let mut sum = a;
        for n in 1..120 {
            term *= -a * a / n as f64;
            sum += term / (2 * n + 1) as f64;
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else {
        let mut s = 1.0;
        let mut t = 1.0;
        for n in 1..12 {
            t *= -((2 * n - 1) as f64) / (2.0 * a * a);
            s += t;
        }
        1.0 - (-a * a).exp() / (a * std::f64::consts::PI.sqrt()) * s
    };
    v.copysign(z)
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

// ---- activations -------------------------------------------------------------------------------

fn act_inputs() -> Vec<f64> {
    let mut xs: Vec<f64> = (-120..=120).map(|i| i as f64 * 0.083).collect();
    xs.extend([-127.9, -40.0, -12.5, 12.5, 40.0, 127.9, 0.0, 1e-6, -1e-6]);
    xs
}

fn check_act(name: &str, build: impl Fn(&mut BlockBuilder<'_>, Ref) -> Ref, want: impl Fn(f64) -> f64, tol: impl Fn(f64) -> f64) {
    let xs = act_inputs();
    let mut x = q24v(&xs);
    // The rails too: a template must be total and admissible there.
    x.extend([i32::MIN as i128, i32::MAX as i128]);
    let got = eval1(vec![p("x", DType::I32, &[x.len() as u32], x.clone())], |b, r| build(b, r[0]));
    for (i, xv) in xs.iter().enumerate() {
        let (g, w) = (f(got[i]), want(*xv));
        assert!((g - w).abs() <= tol(*xv), "{name}({xv}) = {g}, want {w}");
    }
}

#[test]
fn tanh_gelu_quick_gelu_relu2_and_the_clamped_swiglu_match_their_float_functions() {
    // ~1e-3 absolute: `IntExp`'s polynomial (2.7e-4 relative) doubled by `2σ(2x) − 1`.
    check_act("tanh", |b, x| b.tanh_q24(x), f64::tanh, |_| 1.5e-3);
    check_act(
        "gelu_tanh",
        |b, x| b.gelu_tanh_q24(x),
        |x| 0.5 * x * (1.0 + ((2.0 / std::f64::consts::PI).sqrt() * (x + 0.044715 * x * x * x)).tanh()),
        |x| 2e-3 * x.abs().max(1.0),
    );
    check_act(
        "gelu_erf",
        |b, x| b.gelu_erf_q24(x),
        |x| 0.5 * x * (1.0 + erf(x / std::f64::consts::SQRT_2)),
        |x| 2e-3 * x.abs().max(1.0),
    );
    check_act("quick_gelu", |b, x| b.quick_gelu_q24(x), |x| x * sigmoid(1.702 * x), |x| 2e-3 * x.abs().max(1.0));
    // Observed at 2^−8 so the square of every tested input fits the committed i32 lane.
    check_act(
        "relu2",
        |b, x| {
            let r = b.relu2_q24(x);
            let r = b.shr(r, 8, misaka_palw_tir::Rounding::Floor, DType::I64);
            b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32)
        },
        |x| x.max(0.0).powi(2).min(127.0 * 256.0) / 256.0,
        |x| 1e-6 * x * x + 1e-6,
    );
    // gpt-oss: (clamp(up) + 1) · g · σ(1.702 g), g = min(gate, 7). up is the SAME row here.
    check_act(
        "swiglu_clamped",
        |b, x| b.swiglu_clamped_q24(x, x, 7.0, 1.702),
        |x| {
            let g = x.min(7.0);
            (x.clamp(-7.0, 7.0) + 1.0) * g * sigmoid(1.702 * g)
        },
        |x| 2e-3 * x.abs().max(1.0) * 8.0,
    );
}

#[test]
fn an_activation_table_is_a_gather_by_code() {
    // The table holds f(c) = 3c − 7 on the code grid, saturated to i16.
    let table: Vec<i128> = (-32768i128..32768).map(|c| (3 * c - 7).clamp(-32768, 32767)).collect();
    let codes: Vec<i128> = vec![-32768, -32767, -1, 0, 1, 1234, 32767];
    let got =
        eval1(vec![p("x", DType::I16, &[codes.len() as u32], codes.clone()), p("table", DType::I16, &[65536], table)], |b, r| {
            b.act_table(r[0], r[1])
        });
    assert_eq!(got, codes.iter().map(|c| (3 * c - 7).clamp(-32768, 32767)).collect::<Vec<_>>());
    // A wide input is narrowed to codes first: x·1/2^4, then the table.
    let wide: Vec<i128> = vec![-1 << 30, -16, 0, 16, 160, 1 << 30];
    let table: Vec<i128> = (-32768i128..32768).map(|c| c.clamp(-100, 100)).collect();
    let got = eval1(vec![p("x", DType::I32, &[6], wide.clone()), p("table", DType::I16, &[65536], table)], |b, r| {
        let n = narrowing(b, 1, 4);
        b.act_table_wide(r[0], &n, r[1])
    });
    assert_eq!(got, vec![-100, -1, 0, 1, 10, 100]);
}

// ---- norms ---------------------------------------------------------------------------------------

#[test]
fn layer_norm_group_norm_and_l2_norm_are_the_float_norms_at_any_width() {
    let mut rng = Lcg(3);
    for n in [5u32, 7, 96, 2880] {
        let x: Vec<i128> = (0..n).map(|i| rng.range(-20000, 20000) + if i == 0 { 900 } else { 0 }).collect();
        let got = eval1(vec![p("x", DType::I16, &[n], x.clone())], |b, r| {
            let ez = b.c(DType::I64, 1);
            let es = b.c(DType::I8, 0);
            b.layer_norm_exact(r[0], ez, es)
        });
        let xf: Vec<f64> = x.iter().map(|v| *v as f64).collect();
        let mu = xf.iter().sum::<f64>() / n as f64;
        let var = xf.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n as f64;
        for (g, v) in got.iter().zip(&xf) {
            let w = (v - mu) / var.sqrt();
            assert!((f(*g) - w).abs() < 1e-4, "LN n={n}: {} vs {w}", f(*g));
        }
    }
    // GroupNorm: 4 groups of 6, each normalised on its own.
    let x: Vec<i128> = (0..24).map(|_| rng.range(-3000, 3000)).collect();
    let got = eval1(vec![p("x", DType::I16, &[24], x.clone())], |b, r| {
        let ez = b.c(DType::I64, 1);
        let es = b.c(DType::I8, 0);
        b.group_norm_exact(r[0], 4, ez, es)
    });
    for g in 0..4 {
        let xs: Vec<f64> = x[g * 6..(g + 1) * 6].iter().map(|v| *v as f64).collect();
        let mu = xs.iter().sum::<f64>() / 6.0;
        let sd = (xs.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / 6.0).sqrt();
        for i in 0..6 {
            assert!((f(got[g * 6 + i]) - (xs[i] - mu) / sd).abs() < 1e-4, "group {g}");
        }
    }
    // L2 with eps inside the root: eps = 5·2^10 at the input scale².
    for n in [1u32, 8, 128] {
        let x: Vec<i128> = (0..n).map(|_| rng.range(-32767, 32767)).collect();
        let got = eval1(vec![p("x", DType::I16, &[n], x.clone())], |b, r| {
            let ez = b.c(DType::I64, 5);
            let es = b.c(DType::I8, 10);
            b.l2_norm_eps(r[0], ez, es)
        });
        let ss: f64 = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() + 5.0 * 1024.0;
        for (g, v) in got.iter().zip(&x) {
            assert!((f(*g) - *v as f64 / ss.sqrt()).abs() < 2e-6, "L2 n={n}");
        }
    }
}

#[test]
fn the_exact_wide_norm_agrees_with_the_lowerers_form_wherever_both_apply() {
    let mut rng = Lcg(5);
    for (ez, es) in [(0i128, 0i128), (1, 0), (1 << 30, 62), (12345, 40), (7, 3)] {
        let x: Vec<i128> = (0..64).map(|_| rng.range(i32::MIN as i128, i32::MAX as i128) >> rng.range(0, 30)).collect();
        let run = |exact: bool| {
            eval1(vec![p("x", DType::I32, &[64], x.clone())], |b, r| {
                let z = b.c(DType::I64, ez);
                let s = b.c(DType::I8, es);
                if exact { b.rms_norm_wide_q36_exact(r[0], z, s) } else { b.rms_norm_wide_q36(r[0], z, s) }
            })
        };
        assert_eq!(run(true), run(false), "eps {ez}·2^{es}");
    }
}

// ---- softmax variants ----------------------------------------------------------------------------

#[test]
fn a_sink_takes_its_share_of_the_denominator_and_the_soft_cap_is_cap_tanh() {
    let mut rng = Lcg(9);
    for up in [0u32, 2] {
        let xs: Vec<f64> = (0..11).map(|_| rng.range(-8000, 8000) as f64 / 1000.0).collect();
        for sink in [-30.0f64, 0.0, 3.5, 20.0] {
            let got = eval1(vec![p("x", DType::I32, &[11], q24v(&xs)), p("sink", DType::I32, &[1], vec![q24(sink)])], |b, r| {
                b.softmax_with_sink(r[0], r[1], up)
            });
            let scale = (1u64 << up) as f64;
            let m = xs.iter().cloned().fold(sink, f64::max);
            let den: f64 = xs.iter().map(|x| ((x - m) * scale).exp()).sum::<f64>() + ((sink - m) * scale).exp();
            for (g, x) in got.iter().zip(&xs) {
                let w = ((x - m) * scale).exp() / den;
                assert!((f(*g) - w).abs() < 1e-3, "sink {sink} up {up}: {} vs {w}", f(*g));
            }
        }
    }
    for cap in [30.0f64, 50.0] {
        let xs: Vec<f64> = (-40..=40).map(|i| i as f64 * 2.5).collect();
        let got =
            eval1(vec![p("x", DType::I32, &[xs.len() as u32], q24v(&xs)), p("cap", DType::I32, &[1], vec![q24(cap)])], |b, r| {
                b.softcap_q24(r[0], r[1])
            });
        for (g, x) in got.iter().zip(&xs) {
            let w = cap * (x / cap).tanh();
            assert!((f(*g) - w).abs() < 1.5e-3 * cap, "cap {cap}: softcap({x}) = {} vs {w}", f(*g));
        }
    }
}

// ---- rotary ---------------------------------------------------------------------------------------

fn angles(theta: &[f64], pos: f64) -> (Vec<i128>, Vec<i128>) {
    (theta.iter().map(|w| q24((pos * w).cos())).collect(), theta.iter().map(|w| q24((pos * w).sin())).collect())
}

#[test]
fn rotate_half_interleaved_and_partial_rotary_turn_the_right_lanes() {
    let mut rng = Lcg(21);
    let (heads, hd, rot) = (3u32, 8u32, 4u32);
    let theta: Vec<f64> = (0..rot / 2).map(|j| 10000f64.powf(-2.0 * j as f64 / rot as f64)).collect();
    let (c, s) = angles(&theta, 5.0);
    let x: Vec<i128> = (0..heads * hd).map(|_| rng.range(-30000, 30000)).collect();
    for style in [RopeStyle::Half, RopeStyle::Interleaved] {
        let got = eval1(
            vec![
                p("x", DType::I16, &[heads * hd], x.clone()),
                p("cos", DType::I32, &[rot / 2], c.clone()),
                p("sin", DType::I32, &[rot / 2], s.clone()),
            ],
            |b, r| {
                let x2 = b.reshape_fixed(r[0], &[heads, hd]);
                b.rope_partial(x2, rot, r[1], r[2], style, -32767, 32767)
            },
        );
        for h in 0..heads as usize {
            let row = &x[h * hd as usize..(h + 1) * hd as usize];
            let out = &got[h * hd as usize..(h + 1) * hd as usize];
            for j in 0..(rot / 2) as usize {
                let (ia, ib) = match style {
                    RopeStyle::Half => (j, j + (rot / 2) as usize),
                    RopeStyle::Interleaved => (2 * j, 2 * j + 1),
                };
                let (a, bb) = (row[ia] as f64, row[ib] as f64);
                let (cw, sw) = ((5.0 * theta[j]).cos(), (5.0 * theta[j]).sin());
                assert!((out[ia] as f64 - (a * cw - bb * sw)).abs() <= 2.0, "{style:?} re");
                assert!((out[ib] as f64 - (a * sw + bb * cw)).abs() <= 2.0, "{style:?} im");
            }
            assert_eq!(&out[rot as usize..], &row[rot as usize..], "the lanes past the rotary width pass through");
        }
    }
}

#[test]
fn position_dependent_frequency_sets_switch_at_their_threshold() {
    // Two sets (LongRoPE's short and long factors): positions 0..4 use ω, 4.. use ω/8; two-level
    // tables of 2^2 rows each, so positions up to 15 are addressable.
    let lo_bits = 2u32;
    let omegas = [[0.9f64, 0.1], [0.9 / 8.0, 0.1 / 8.0]];
    let mut params = Vec::new();
    for (k, om) in omegas.iter().enumerate() {
        let hi_rows: Vec<f64> = (0..4).map(|h| (h * 4) as f64).collect();
        let lo_rows: Vec<f64> = (0..4).map(|l| l as f64).collect();
        let tab = |rows: &[f64], cos: bool| -> Vec<i128> {
            rows.iter().flat_map(|pv| om.iter().map(move |w| q24(if cos { (pv * w).cos() } else { (pv * w).sin() }))).collect()
        };
        let names = [["ch0", "sh0", "cl0", "sl0"], ["ch1", "sh1", "cl1", "sl1"]][k];
        params.push(p(names[0], DType::I32, &[4, 2], tab(&hi_rows, true)));
        params.push(p(names[1], DType::I32, &[4, 2], tab(&hi_rows, false)));
        params.push(p(names[2], DType::I32, &[4, 2], tab(&lo_rows, true)));
        params.push(p(names[3], DType::I32, &[4, 2], tab(&lo_rows, false)));
    }
    let positions = 12u32;
    let run = run_layer(params, vec![], positions, |b, r, _| {
        let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
        let sets = [
            AngleSet { from_position: 0, cos_hi: r[0], sin_hi: r[1], cos_lo: r[2], sin_lo: r[3], lo_bits },
            AngleSet { from_position: 4, cos_hi: r[4], sin_hi: r[5], cos_lo: r[6], sin_lo: r[7], lo_bits },
        ];
        let (c, s) = b.rope_angles_by_position(pos, &sets);
        b.concat(&[c, s], 0)
    });
    for (pos, out) in run.outs.iter().enumerate() {
        let om = if pos < 4 { omegas[0] } else { omegas[1] };
        for j in 0..2 {
            assert!((f(out.data[j]) - (pos as f64 * om[j]).cos()).abs() < 4e-7, "pos {pos} cos");
            assert!((f(out.data[2 + j]) - (pos as f64 * om[j]).sin()).abs() < 4e-7, "pos {pos} sin");
        }
    }
}

// ---- attention -----------------------------------------------------------------------------------

/// A GQA layer over `positions`: q, k, v rows gathered from position-indexed tables (codes at
/// scale 1/1024), keys and values appended to a window of `window`, and `attention` with the
/// options given; the float reference is the same attention on the same code values.
#[allow(clippy::too_many_arguments)]
fn gqa_case(
    heads: u32,
    kv: u32,
    d: u32,
    window: u32,
    positions: u32,
    softcap: Option<f64>,
    alibi: bool,
    sink: Option<f64>,
    seed: u64,
) {
    let mut rng = Lcg(seed);
    let qd = heads * d;
    let kd = kv * d;
    let qt: Vec<i128> = (0..positions * qd).map(|_| rng.range(-3000, 3000)).collect();
    let kt: Vec<i128> = (0..positions * kd).map(|_| rng.range(-3000, 3000)).collect();
    let vt: Vec<i128> = (0..positions * kd).map(|_| rng.range(-3000, 3000)).collect();
    let slopes: Vec<f64> = (0..heads).map(|h| 2f64.powf(-(h as f64 + 1.0))).collect();
    let sinks: Vec<f64> = (0..heads).map(|h| sink.unwrap_or(0.0) + h as f64 * 0.25).collect();
    // Only what the options use is declared (normal form: every param is used, NF-11).
    let mut params = vec![
        p("qt", DType::I16, &[positions, qd], qt.clone()),
        p("kt", DType::I16, &[positions, kd], kt.clone()),
        p("vt", DType::I16, &[positions, kd], vt.clone()),
    ];
    let at = |params: &mut Vec<P>, x: P| {
        params.push(x);
        params.len() - 1
    };
    let slopes_at = alibi.then(|| at(&mut params, p("slopes", DType::I32, &[heads], q24v(&slopes))));
    let sinks_at = sink.map(|_| at(&mut params, p("sinks", DType::I32, &[heads], q24v(&sinks))));
    let cap_at = softcap.map(|c| at(&mut params, p("cap", DType::I32, &[1], vec![q24(c)])));
    // logits = q·k /√d at real scale: (1/1024)² · 2^24 / √d = 16/√d.
    let score_m = ((16.0 / (d as f64).sqrt()) * (1u64 << 20) as f64).round() as i128;
    let run = run_layer(
        params,
        vec![S::Hist("k", DType::I16, vec![kd], window), S::Hist("v", DType::I16, vec![kd], window)],
        positions,
        |b, r, st| {
            let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
            let q = b.gather(r[0], pos, 0, 0);
            let k = b.gather(r[1], pos, 0, 0);
            let v = b.gather(r[2], pos, 0, 0);
            b.commit(q);
            let kw = b.hist_append(st[0], k);
            let vw = b.hist_append(st[1], v);
            let score = narrowing(b, score_m, 20);
            let value = narrowing(b, 1, 24);
            let cfg = AttnCfg {
                heads,
                kv_heads: kv,
                head_dim: d,
                score,
                softcap: cap_at.map(|i| r[i]),
                alibi: slopes_at.map(|i| r[i]),
                sink: sinks_at.map(|i| r[i]),
                up_bits: 0,
                value,
            };
            b.attention(q, kw, vw, &cfg)
        },
    );
    let g = heads / kv;
    for (pos, out) in run.outs.iter().enumerate() {
        let lo = (pos + 1).saturating_sub(window as usize);
        for h in 0..heads as usize {
            let kh = h / g as usize;
            let q = &qt[pos * qd as usize + h * d as usize..][..d as usize];
            let mut logits: Vec<f64> = (lo..=pos)
                .map(|t| {
                    let k = &kt[t * kd as usize + kh * d as usize..][..d as usize];
                    let dot: f64 = q.iter().zip(k).map(|(a, b)| (*a as f64 / 1024.0) * (*b as f64 / 1024.0)).sum();
                    dot / (d as f64).sqrt()
                })
                .collect();
            if let Some(cap) = softcap {
                logits.iter_mut().for_each(|l| *l = cap * (*l / cap).tanh());
            }
            if alibi {
                logits.iter_mut().enumerate().for_each(|(j, l)| *l += slopes[h] * j as f64);
            }
            let mut m = logits.iter().cloned().fold(f64::MIN, f64::max);
            if sink.is_some() {
                m = m.max(sinks[h]);
            }
            let mut den: f64 = logits.iter().map(|l| (l - m).exp()).sum();
            if sink.is_some() {
                den += (sinks[h] - m).exp();
            }
            for i in 0..d as usize {
                let want: f64 = (lo..=pos)
                    .enumerate()
                    .map(|(j, t)| (logits[j] - m).exp() / den * (vt[t * kd as usize + kh * d as usize + i] as f64))
                    .sum();
                let got = out.data[h * d as usize + i] as f64;
                assert!(
                    (got - want).abs() <= 3.0 + 3e-3 * want.abs(),
                    "pos {pos} head {h} lane {i}: {got} vs {want} (window {window}, cap {softcap:?}, alibi {alibi}, sink {sink:?})"
                );
            }
        }
    }
}

#[test]
fn grouped_query_attention_over_a_window_with_soft_cap_alibi_and_sinks_is_the_float_attention() {
    gqa_case(4, 2, 8, 1 << 18, 6, None, false, None, 1);
    gqa_case(4, 1, 8, 3, 7, None, false, None, 2); // MQA, sliding window of 3
    gqa_case(4, 4, 4, 4, 6, Some(3.0), false, None, 3); // MHA, soft-capped
    gqa_case(4, 2, 8, 1 << 18, 5, None, true, None, 4); // ALiBi
    gqa_case(2, 1, 8, 2, 5, None, false, Some(0.5), 5); // a sink, window 2
}

#[test]
fn absorbed_latent_attention_chains_its_two_contractions() {
    let mut rng = Lcg(31);
    let (h, dn, dr, r, dv, positions) = (2u32, 4u32, 2u32, 6u32, 4u32, 5u32);
    let qn: Vec<i128> = (0..positions * h * dn).map(|_| rng.range(-2000, 2000)).collect();
    let qr: Vec<i128> = (0..positions * h * dr).map(|_| rng.range(-2000, 2000)).collect();
    let ck: Vec<i128> = (0..positions * r).map(|_| rng.range(-2000, 2000)).collect();
    let kr: Vec<i128> = (0..positions * dr).map(|_| rng.range(-2000, 2000)).collect();
    let wkb: Vec<i128> = (0..h * dn * r).map(|_| rng.range(-127, 127)).collect();
    let wvb: Vec<i128> = (0..h * r * dv).map(|_| rng.range(-127, 127)).collect();
    let params = vec![
        p("qn", DType::I16, &[positions, h * dn], qn.clone()),
        p("qr", DType::I16, &[positions, h * dr], qr.clone()),
        p("ck", DType::I16, &[positions, r], ck.clone()),
        p("kr", DType::I16, &[positions, dr], kr.clone()),
        p("wkb", DType::I8, &[h, dn, r], wkb.clone()),
        p("wvb", DType::I8, &[h, r, dv], wvb.clone()),
    ];
    // Units: codes are real·1024; weights real·64 (so W·x is real·2^16). q̃ codes = W·q/64 (m=1,s=6);
    // logits Q24 = (q̃·c)/1024²·2^24/√(dn+dr) and likewise for the rope part; ctx = Σp·c (s=24);
    // out = W·ctx/64.
    let sc = ((16.0 / ((dn + dr) as f64).sqrt()) * (1u64 << 20) as f64).round() as i128;
    let run = run_layer(
        params,
        vec![S::Hist("c", DType::I16, vec![r], 1 << 18), S::Hist("kr", DType::I16, vec![dr], 1 << 18)],
        positions,
        |b, rr, st| {
            let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
            let qn = b.gather(rr[0], pos, 0, 0);
            let qn = b.reshape_fixed(qn, &[h, dn]);
            let qr = b.gather(rr[1], pos, 0, 0);
            let qr = b.reshape_fixed(qr, &[h, dr]);
            let c = b.gather(rr[2], pos, 0, 0);
            let k = b.gather(rr[3], pos, 0, 0);
            let cw = b.hist_append(st[0], c);
            let kw = b.hist_append(st[1], k);
            let cfg = MlaCfg {
                heads: h,
                q_latent: narrowing(b, 1, 6),
                score_latent: narrowing(b, sc, 20),
                score_rope: narrowing(b, sc, 20),
                up_bits: 0,
                ctx: narrowing(b, 1, 24),
                out: narrowing(b, 1, 6),
            };
            b.mla_absorbed(qn, qr, cw, kw, rr[4], rr[5], &cfg)
        },
    );
    for (pos, out) in run.outs.iter().enumerate() {
        for hh in 0..h as usize {
            let q_n: Vec<f64> = (0..dn as usize).map(|i| qn[pos * (h * dn) as usize + hh * dn as usize + i] as f64).collect();
            let q_r: Vec<f64> = (0..dr as usize).map(|i| qr[pos * (h * dr) as usize + hh * dr as usize + i] as f64).collect();
            // The integer path rounds q̃ to codes; the float reference follows the same rounding point.
            let qt: Vec<f64> = (0..r as usize)
                .map(|j| {
                    ((0..dn as usize).map(|i| wkb[hh * (dn * r) as usize + i * r as usize + j] as f64 * q_n[i]).sum::<f64>() / 64.0)
                        .round()
                })
                .collect();
            let logits: Vec<f64> = (0..=pos)
                .map(|t| {
                    let a: f64 = (0..r as usize).map(|j| qt[j] * ck[t * r as usize + j] as f64).sum();
                    let bb: f64 = (0..dr as usize).map(|j| q_r[j] * kr[t * dr as usize + j] as f64).sum();
                    (a + bb) / (1024.0 * 1024.0) / ((dn + dr) as f64).sqrt()
                })
                .collect();
            let m = logits.iter().cloned().fold(f64::MIN, f64::max);
            let den: f64 = logits.iter().map(|l| (l - m).exp()).sum();
            let ctx: Vec<f64> = (0..r as usize)
                .map(|j| (0..=pos).map(|t| (logits[t] - m).exp() / den * ck[t * r as usize + j] as f64).sum::<f64>())
                .collect();
            for o in 0..dv as usize {
                let want: f64 =
                    (0..r as usize).map(|j| wvb[hh * (r * dv) as usize + j * dv as usize + o] as f64 * ctx[j]).sum::<f64>() / 64.0;
                let got = out.data[hh * dv as usize + o] as f64;
                assert!((got - want).abs() <= 4.0 + 5e-3 * want.abs(), "pos {pos} head {hh} lane {o}: {got} vs {want}");
            }
        }
    }
}

// ---- routing -------------------------------------------------------------------------------------

/// The integer reference of group-limited routing: stable ordering, lowest index on ties.
fn topk_ref(v: &[i128], k: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|a, b| v[*b].cmp(&v[*a]).then(a.cmp(b)));
    let mut kept = idx[..k].to_vec();
    kept.sort_unstable();
    kept
}

#[test]
fn grouped_top_k_keeps_the_best_groups_and_weights_come_from_the_unbiased_scores() {
    let mut rng = Lcg(41);
    for rule in [GroupScore::Max, GroupScore::Top2Sum] {
        for trial in 0..20 {
            let scores: Vec<i128> = (0..16).map(|_| rng.range(0, ONE) / if trial % 3 == 0 { 1 << 20 } else { 1 }).collect();
            let bias: Vec<i128> = (0..16).map(|_| rng.range(-(ONE / 8), ONE / 8)).collect();
            let run = run_layer(
                vec![p("s", DType::I32, &[16], scores.clone()), p("b", DType::I32, &[16], bias.clone())],
                vec![],
                1,
                |b, r, _| {
                    let sel = b.selection_bias(r[0], r[1]);
                    let idx = b.grouped_topk(sel, 4, 2, 3, rule, i32::MIN as i64);
                    let w = b.gather(r[0], idx, 0, 0);
                    let w = b.renormalize_div(w);
                    let idx32 = b.cast(idx, DType::I32);
                    b.concat(&[idx32, w], 0)
                },
            );
            let sel: Vec<i128> = scores.iter().zip(&bias).map(|(s, b)| s + b).collect();
            let gs: Vec<i128> = (0..4)
                .map(|g| {
                    let grp = &sel[g * 4..(g + 1) * 4];
                    match rule {
                        GroupScore::Max => *grp.iter().max().unwrap(),
                        GroupScore::Top2Sum => topk_ref(grp, 2).iter().map(|i| grp[*i]).sum(),
                    }
                })
                .collect();
            let groups = topk_ref(&gs, 2);
            let masked: Vec<i128> = (0..16).map(|e| if groups.contains(&(e / 4)) { sel[e] } else { i32::MIN as i128 }).collect();
            let idx = topk_ref(&masked, 3);
            let ws: Vec<i128> = idx.iter().map(|i| scores[*i]).collect();
            let sum: i128 = ws.iter().sum::<i128>().max(1);
            let want: Vec<i128> = idx.iter().map(|i| *i as i128).chain(ws.iter().map(|w| (w * ONE).div_euclid(sum))).collect();
            assert_eq!(run.outs[0].data, want, "{rule:?} trial {trial}");
        }
    }
}

// ---- recurrences ---------------------------------------------------------------------------------

#[test]
fn the_head_maps_group_and_tile() {
    let x: Vec<i128> = (0..6).collect(); // [k = 2, d = 3]
    let g = eval1(vec![p("x", DType::I16, &[6], x.clone())], |b, r| {
        let x2 = b.reshape_fixed(r[0], &[2, 3]);
        b.map_heads_group(x2, 3)
    });
    assert_eq!(g, vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 3, 4, 5, 3, 4, 5, 3, 4, 5], "value head vh reads key head vh / r");
    let t = eval1(vec![p("x", DType::I16, &[6], x)], |b, r| {
        let x2 = b.reshape_fixed(r[0], &[2, 3]);
        b.map_heads_tile(x2, 3)
    });
    assert_eq!(t, vec![0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5], "value head vh reads key head vh % k");
}

#[test]
fn the_causal_conv_window_and_the_token_shift_carry_their_rows() {
    let mut rng = Lcg(51);
    let (c, w, positions) = (5u32, 4u32, 7u32);
    let rows: Vec<i128> = (0..positions * c).map(|_| rng.range(-32767, 32767)).collect();
    let taps: Vec<i128> = (0..w * c).map(|_| rng.range(-127, 127)).collect();
    let run = run_layer(
        vec![p("rows", DType::I16, &[positions, c], rows.clone()), p("taps", DType::I8, &[w, c], taps.clone())],
        vec![S::Fixed("conv", DType::I16, vec![w - 1, c], -32767, 32767), S::Fixed("shift", DType::I16, vec![c], -32767, 32767)],
        positions,
        |b, r, st| {
            let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
            let row = b.gather(r[0], pos, 0, 0);
            let acc = b.causal_conv(st[0], row, r[1]);
            let prev = b.token_shift(st[1], row);
            let prev = b.cast(prev, DType::I64);
            b.concat(&[acc, prev], 0)
        },
    );
    for (pos, out) in run.outs.iter().enumerate() {
        for ch in 0..c as usize {
            let want: i128 = (0..w as usize)
                .map(|t| {
                    let src = pos as i64 - (w as i64 - 1) + t as i64;
                    if src < 0 { 0 } else { rows[src as usize * c as usize + ch] * taps[t * c as usize + ch] }
                })
                .sum();
            assert_eq!(out.data[ch], want.clamp(i32::MIN as i128, i32::MAX as i128), "conv pos {pos} ch {ch}");
            let prev = if pos == 0 { 0 } else { rows[(pos - 1) * c as usize + ch] };
            assert_eq!(out.data[c as usize + ch], prev, "shift pos {pos} ch {ch}");
        }
    }
}

#[test]
fn exponentials_of_either_sign_and_the_double_exponential() {
    let ys: Vec<f64> = (-80..=80).map(|i| i as f64 * 0.25).chain([-1e-4, 1e-4, 0.0, 26.0]).collect();
    // Observed three times, scaled so each range fits a committed i32 lane with precision to spare:
    // `e / 2^8` up to exp(10), `e / 2^20` up to exp(18), `e / 2^34` above.
    let shifts = [8u32, 20, 34];
    let got = eval1(vec![p("y", DType::I32, &[ys.len() as u32], q24v(&ys))], |b, r| {
        let e = b.exp_q24(r[0]);
        let parts: Vec<Ref> = shifts
            .iter()
            .map(|k| {
                let v = b.shr(e, *k, misaka_palw_tir::Rounding::Floor, DType::I64);
                b.clamp(v, 0, i32::MAX as i64, DType::I32)
            })
            .collect();
        b.concat(&parts, 0)
    });
    let n = ys.len();
    for (i, y) in ys.iter().enumerate() {
        let w = y.exp();
        let which = if *y <= 10.0 {
            0
        } else if *y <= 18.0 {
            1
        } else {
            2
        };
        let gv = got[which * n + i] as f64 * (1u64 << shifts[which]) as f64 / ONE as f64;
        assert!((gv - w).abs() <= 1e-4 * w + 2e-5, "exp({y}) = {gv} vs {w}");
    }
    let got = eval1(vec![p("y", DType::I32, &[ys.len() as u32], q24v(&ys))], |b, r| b.exp_neg_exp_q24(r[0]));
    for (g, y) in got.iter().zip(&ys) {
        let w = (-y.exp()).exp();
        assert!((f(*g) - w).abs() <= 2e-4, "exp(-exp({y})) = {} vs {w}", f(*g));
        assert!(*g <= ONE, "a decay never exceeds one");
    }
}

/// Mamba-2 and Mamba-1 steps against a float simulation of the same recurrence.
#[test]
fn the_selective_scans_follow_the_float_recurrence() {
    let mut rng = Lcg(61);
    let (nh, pd, n, ng, positions) = (2u32, 3u32, 4u32, 1u32, 8u32);
    let x: Vec<i128> = (0..positions * nh * pd).map(|_| rng.range(-4000, 4000)).collect();
    let bm: Vec<i128> = (0..positions * ng * n).map(|_| rng.range(-4000, 4000)).collect();
    let cm: Vec<i128> = (0..positions * ng * n).map(|_| rng.range(-4000, 4000)).collect();
    let dt: Vec<f64> = (0..positions * nh).map(|_| rng.range(20, 600) as f64 / 1000.0).collect();
    let a: Vec<f64> = vec![-0.8, -2.5];
    let d: Vec<f64> = vec![0.5, -1.25];
    let params = vec![
        p("x", DType::I16, &[positions, nh * pd], x.clone()),
        p("b", DType::I16, &[positions, ng * n], bm.clone()),
        p("c", DType::I16, &[positions, ng * n], cm.clone()),
        p("dt", DType::I32, &[positions, nh], q24v(&dt)),
        p("a", DType::I32, &[nh], q24v(&a)),
        p("d", DType::I32, &[nh], q24v(&d)),
    ];
    // Units: codes real·1024; h real·2^20; y real·1024.
    let run = run_layer(
        params,
        vec![S::Fixed("h", DType::I32, vec![nh, pd, n], -(i32::MAX as i64), i32::MAX as i64)],
        positions,
        |b, r, st| {
            let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
            let xs = b.gather(r[0], pos, 0, 0);
            let xs = b.reshape_fixed(xs, &[nh, pd]);
            let bs = b.gather(r[1], pos, 0, 0);
            let bs = b.reshape_fixed(bs, &[ng, n]);
            let cs = b.gather(r[2], pos, 0, 0);
            let cs = b.reshape_fixed(cs, &[ng, n]);
            let dts = b.gather(r[3], pos, 0, 0);
            let cfg = ScanCfg { input: narrowing(b, 1, 24), out_c: narrowing(b, 1, 20), out_d: narrowing(b, 1, 24) };
            b.mamba2_step(st[0], xs, bs, cs, dts, r[4], r[5], &cfg)
        },
    );
    let mut h = vec![0f64; (nh * pd * n) as usize];
    for (t, out) in run.outs.iter().enumerate() {
        for hh in 0..nh as usize {
            let dtv = dt[t * nh as usize + hh];
            let da = (dtv * a[hh]).exp();
            for pi in 0..pd as usize {
                let xv = x[t * (nh * pd) as usize + hh * pd as usize + pi] as f64 / 1024.0;
                let mut y = 0.0;
                for ni in 0..n as usize {
                    let bv = bm[t * (ng * n) as usize + ni] as f64 / 1024.0;
                    let cv = cm[t * (ng * n) as usize + ni] as f64 / 1024.0;
                    let k = (hh * pd as usize + pi) * n as usize + ni;
                    h[k] = h[k] * da + dtv * xv * bv;
                    y += h[k] * cv;
                }
                y += d[hh] * xv;
                let got = out.data[hh * pd as usize + pi] as f64 / 1024.0;
                assert!((got - y).abs() <= 1e-2 + 2e-3 * y.abs(), "mamba2 t {t} head {hh} p {pi}: {got} vs {y}");
            }
        }
    }

    // Mamba-1: per-channel dt and a per-(channel, state) A.
    let (ich, n1) = (3u32, 4u32);
    let x: Vec<i128> = (0..positions * ich).map(|_| rng.range(-4000, 4000)).collect();
    let bv: Vec<i128> = (0..positions * n1).map(|_| rng.range(-4000, 4000)).collect();
    let cv: Vec<i128> = (0..positions * n1).map(|_| rng.range(-4000, 4000)).collect();
    let dt: Vec<f64> = (0..positions * ich).map(|_| rng.range(20, 600) as f64 / 1000.0).collect();
    let a: Vec<f64> = (0..ich * n1).map(|i| -0.1 - 0.37 * i as f64).collect();
    let d: Vec<f64> = vec![1.0, 0.25, -0.5];
    for refined in [true, false] {
        let params = vec![
            p("x", DType::I16, &[positions, ich], x.clone()),
            p("b", DType::I16, &[positions, n1], bv.clone()),
            p("c", DType::I16, &[positions, n1], cv.clone()),
            p("dt", DType::I32, &[positions, ich], q24v(&dt)),
            p("a", DType::I32, &[ich, n1], q24v(&a)),
            p("d", DType::I32, &[ich], q24v(&d)),
        ];
        let run = run_layer(
            params,
            vec![S::Fixed("h", DType::I32, vec![ich, n1], -(i32::MAX as i64), i32::MAX as i64)],
            positions,
            |b, r, st| {
                let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
                let xs = b.gather(r[0], pos, 0, 0);
                let bs = b.gather(r[1], pos, 0, 0);
                let cs = b.gather(r[2], pos, 0, 0);
                let dts = b.gather(r[3], pos, 0, 0);
                let cfg = ScanCfg { input: narrowing(b, 1, 24), out_c: narrowing(b, 1, 20), out_d: narrowing(b, 1, 24) };
                b.mamba1_step(st[0], xs, bs, cs, dts, r[4], r[5], &cfg, refined)
            },
        );
        let mut h = vec![0f64; (ich * n1) as usize];
        for (t, out) in run.outs.iter().enumerate() {
            for ci in 0..ich as usize {
                let dtv = dt[t * ich as usize + ci];
                let xv = x[t * ich as usize + ci] as f64 / 1024.0;
                let mut y = 0.0;
                for ni in 0..n1 as usize {
                    let k = ci * n1 as usize + ni;
                    h[k] = h[k] * (dtv * a[k]).exp() + dtv * xv * bv[t * n1 as usize + ni] as f64 / 1024.0;
                    y += h[k] * cv[t * n1 as usize + ni] as f64 / 1024.0;
                }
                y += d[ci] * xv;
                let got = out.data[ci] as f64 / 1024.0;
                // Unrefined, every decay carries `IntExp`'s 3.5e-3 and the error compounds over the
                // steps — the documented reason the refined form is the default.
                let tol = if refined { 1e-2 + 2e-3 * y.abs() } else { 1e-1 + 3e-2 * y.abs() };
                assert!((got - y).abs() <= tol, "mamba1 (refined {refined}) t {t} ch {ci}: {got} vs {y}");
            }
        }
    }
}

#[test]
fn the_wkv_recurrences_follow_their_float_forms() {
    let mut rng = Lcg(71);
    let positions = 7u32;
    // RWKV-4, per channel.
    let c = 4u32;
    let k: Vec<f64> = (0..positions * c).map(|_| rng.range(-3000, 3000) as f64 / 1000.0).collect();
    let v: Vec<i128> = (0..positions * c).map(|_| rng.range(-20000, 20000)).collect();
    let u: Vec<f64> = vec![0.3, -0.5, 1.0, 0.0];
    let w: Vec<f64> = vec![-0.05, -0.5, -2.0, -0.01];
    let params = vec![
        p("k", DType::I32, &[positions, c], q24v(&k)),
        p("v", DType::I32, &[positions, c], v.clone()),
        p("u", DType::I32, &[c], q24v(&u)),
        p("w", DType::I32, &[c], q24v(&w)),
    ];
    let lo = -(i32::MAX as i64);
    let run = run_layer(
        params,
        vec![
            S::Fixed("num", DType::I32, vec![c], lo, i32::MAX as i64),
            S::Fixed("den", DType::I32, vec![c], lo, i32::MAX as i64),
            S::Fixed("max", DType::I32, vec![c], lo, i32::MAX as i64),
        ],
        positions,
        |b, r, st| {
            let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
            let ks = b.gather(r[0], pos, 0, 0);
            let vs = b.gather(r[1], pos, 0, 0);
            b.rwkv4_step(st[0], st[1], st[2], ks, vs, r[2], r[3])
        },
    );
    // The float reference with the state's initial max = 0 (the integer state starts at zeros).
    let (mut num, mut den, mut mx) = (vec![0f64; c as usize], vec![0f64; c as usize], vec![0f64; c as usize]);
    for (t, out) in run.outs.iter().enumerate() {
        for ch in 0..c as usize {
            let (kk, vv) = (k[t * c as usize + ch], v[t * c as usize + ch] as f64);
            let ww = kk + u[ch];
            let p = mx[ch].max(ww);
            let (e1, e2) = ((mx[ch] - p).exp(), (ww - p).exp());
            let want = (e1 * num[ch] + e2 * vv) / (e1 * den[ch] + e2);
            let ww2 = mx[ch] + w[ch];
            let p2 = ww2.max(kk);
            let (f1, f2) = ((ww2 - p2).exp(), (kk - p2).exp());
            num[ch] = f1 * num[ch] + f2 * vv;
            den[ch] = f1 * den[ch] + f2;
            mx[ch] = p2;
            let got = out.data[ch] as f64;
            assert!((got - want).abs() <= 2.0 + 2e-3 * want.abs(), "rwkv4 t {t} ch {ch}: {got} vs {want}");
        }
    }

    // RWKV-6 and RWKV-7 over two heads of 3.
    let (nh, hs) = (2u32, 3u32);
    let n = (positions * nh * hs) as usize;
    let rr: Vec<i128> = (0..n).map(|_| rng.range(-2000, 2000)).collect();
    let kk: Vec<i128> = (0..n).map(|_| rng.range(-2000, 2000)).collect();
    let vv: Vec<i128> = (0..n).map(|_| rng.range(-2000, 2000)).collect();
    let ww: Vec<f64> = (0..n).map(|_| rng.range(500, 999) as f64 / 1000.0).collect();
    let uu: Vec<f64> = (0..(nh * hs) as usize).map(|i| 0.1 * i as f64 - 0.2).collect();
    let aa: Vec<f64> = (0..n).map(|_| rng.range(0, 1000) as f64 / 1000.0).collect();
    let khat: Vec<i128> = (0..positions * nh)
        .flat_map(|i| {
            let row: Vec<f64> = (0..hs as usize).map(|j| kk[i as usize * hs as usize + j] as f64).collect();
            let norm = row.iter().map(|x| x * x).sum::<f64>().sqrt().max(1.0);
            row.into_iter().map(move |x| (x / norm * 16384.0).round() as i128)
        })
        .collect();
    let base = |b: &mut BlockBuilder<'_>, r: &[Ref], i: usize| {
        let pos = b.clamp(Ref::Input(INPUT_POS), 0, positions as i64 - 1, DType::Idx);
        let g = b.gather(r[i], pos, 0, 0);
        b.reshape_fixed(g, &[nh, hs])
    };
    // `[r, k, v, w]` then the step's own: `u` for RWKV-6; `a`, `k̂` for RWKV-7 (NF-11: all used).
    let common_params = |seven: bool| {
        let mut v = vec![
            p("r", DType::I16, &[positions, nh * hs], rr.clone()),
            p("k", DType::I16, &[positions, nh * hs], kk.clone()),
            p("v", DType::I16, &[positions, nh * hs], vv.clone()),
            p("w", DType::I32, &[positions, nh * hs], q24v(&ww)),
        ];
        if seven {
            v.push(p("a", DType::I32, &[positions, nh * hs], q24v(&aa)));
            v.push(p("khat", DType::I16, &[positions, nh * hs], khat.clone()));
        } else {
            v.push(p("u", DType::I32, &[nh, hs], q24v(&uu)));
        }
        v
    };
    // Units: codes real·1024 (k̂ real·16384); S real·2^20 (k·v codes² = real·2^20, so N_kv is the
    // identity); y codes = r·S / 2^20.
    let st = || vec![S::Fixed("S", DType::I32, vec![nh, hs, hs], -(i32::MAX as i64), i32::MAX as i64)];
    let run6 = run_layer(common_params(false), st(), positions, |b, r, st| {
        let (rs, ks, vs, ws) = (base(b, r, 0), base(b, r, 1), base(b, r, 2), base(b, r, 3));
        let cfg = Wkv6Cfg { kv: narrowing(b, 1, 0), y: narrowing(b, 1, 20) };
        b.rwkv6_step(st[0], rs, ks, vs, ws, r[4], &cfg)
    });
    let run7 = run_layer(common_params(true), st(), positions, |b, r, st| {
        let (rs, ks, vs, ws, kh) = (base(b, r, 0), base(b, r, 1), base(b, r, 2), base(b, r, 3), base(b, r, 5));
        let a_s = base(b, r, 4);
        // sa = S·k̂ is real·2^20·16384 → to real·2^20: s = 14; ab = sa·(a⊙k̂) (a Q24 times k̂ = k̂
        // codes) → real·2^20·16384 → s = 14; vk identity; y = S·r / 2^20.
        let cfg = Wkv7Cfg { sa: narrowing(b, 1, 14), ab: narrowing(b, 1, 14), vk: narrowing(b, 1, 0), y: narrowing(b, 1, 20) };
        b.rwkv7_step(st[0], rs, ws, ks, vs, kh, a_s, &cfg)
    });
    let at = |xs: &[i128], t: usize, h: usize, i: usize| xs[t * (nh * hs) as usize + h * hs as usize + i] as f64;
    let mut s6 = vec![0f64; (nh * hs * hs) as usize];
    let mut s7 = vec![0f64; (nh * hs * hs) as usize];
    for t in 0..positions as usize {
        for h in 0..nh as usize {
            let idx = |a: usize, b: usize| (h * hs as usize + a) * hs as usize + b;
            // RWKV-6: y = r·(S + diag(u)·kᵀv); S ← diag(w)S + kᵀv  (S[k][v], real units).
            for j in 0..hs as usize {
                let y: f64 = (0..hs as usize)
                    .map(|i| {
                        let kv = at(&kk, t, h, i) * at(&vv, t, h, j) / 1024.0 / 1024.0;
                        at(&rr, t, h, i) / 1024.0 * (s6[idx(i, j)] + uu[h * hs as usize + i] * kv)
                    })
                    .sum();
                let got = run6.outs[t].data[h * hs as usize + j] as f64 / 1024.0;
                assert!((got - y).abs() <= 3e-3 + 3e-3 * y.abs(), "rwkv6 t {t} h {h} j {j}: {got} vs {y}");
            }
            for i in 0..hs as usize {
                for j in 0..hs as usize {
                    let kv = at(&kk, t, h, i) * at(&vv, t, h, j) / 1024.0 / 1024.0;
                    s6[idx(i, j)] = ww[t * (nh * hs) as usize + h * hs as usize + i] * s6[idx(i, j)] + kv;
                }
            }
            // RWKV-7: S[v][k] ← S·diag(w) − (S·k̂)⊗(a⊙k̂) + v⊗k; y = S·r.
            let kh: Vec<f64> = (0..hs as usize).map(|i| at(&khat, t, h, i) / 16384.0).collect();
            let sa: Vec<f64> = (0..hs as usize).map(|i| (0..hs as usize).map(|j| s7[idx(i, j)] * kh[j]).sum()).collect();
            let mut next = s7.clone();
            for i in 0..hs as usize {
                for j in 0..hs as usize {
                    let wj = ww[t * (nh * hs) as usize + h * hs as usize + j];
                    let aj = aa[t * (nh * hs) as usize + h * hs as usize + j];
                    next[idx(i, j)] = s7[idx(i, j)] * wj - sa[i] * aj * kh[j] + at(&vv, t, h, i) / 1024.0 * at(&kk, t, h, j) / 1024.0;
                }
            }
            s7 = next;
            for i in 0..hs as usize {
                let y: f64 = (0..hs as usize).map(|j| s7[idx(i, j)] * at(&rr, t, h, j) / 1024.0).sum();
                let got = run7.outs[t].data[h * hs as usize + i] as f64 / 1024.0;
                assert!((got - y).abs() <= 5e-3 + 5e-3 * y.abs(), "rwkv7 t {t} h {h} i {i}: {got} vs {y}");
            }
        }
    }
}

/// The catalogue names every template once, and the constants the tests read are the spec's.
#[test]
fn the_catalogue_is_complete_and_the_constants_are_the_specs() {
    use misaka_palw_tir::library::LIBRARY_V1;
    let mut names: Vec<&str> = LIBRARY_V1.iter().map(|e| e.0).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), LIBRARY_V1.len(), "no template is listed twice");
    assert_eq!((K, ONE, LN2_Q), (24, 1 << 24, 11_629_080));
}

/// **The lean forms are as lean as tir/lower needs them** (NF-12's 512 nodes a block): the narrowing
/// without a zero term is three nodes past its `Pow2` gather, the unit rows 21 and 17 — against the
/// templates' 5, 38 and 25 (counted here as the nodes a call appends). Their values are the templates' (`misaka-palw-tir-conformance`).
#[test]
fn the_lean_forms_have_the_node_counts_the_lowering_needs() {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::library::Narrowing;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    // The count is the index the next node takes.
    let n = |build: &dyn Fn(&mut misaka_palw_tir::builder::BlockBuilder<'_>, &[Ref]) -> Ref| {
        let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
        let x32 = pb.param("x32", DType::I32, &[16], false);
        let x16 = pb.param("x16", DType::I16, &[16], false);
        let m = pb.param("m", DType::I64, &[16], false);
        let s = pb.param("s", DType::I8, &[16], false);
        let e = pb.param("e", DType::I64, &[], false);
        let mut b = pb.block("pre", vec![]);
        let _ = build(&mut b, &[x32, x16, m, s, e]);
        let probe = b.iota(DType::I32, &[misaka_palw_tir::Dim::Fixed(1)], 0, 0, 0);
        let Ref::Node(i) = probe else { unreachable!() };
        i as usize
    };
    assert_eq!(n(&|b, r| b.narrow_codes(r[0], &Narrowing::new(r[2], r[3], None))), 2 + 3, "Pow2 (2) + mul, div, clamp");
    assert_eq!(n(&|b, r| b.narrow_codes(r[0], &Narrowing::new(r[2], r[3], Some(r[2])))), 2 + 5, "with a zero term: the template");
    assert_eq!(n(&|b, r| b.rms_unit_q24(r[0], r[4])), 21);
    assert_eq!(n(&|b, r| b.l2_unit_q15(r[1])), 17);
    assert_eq!(n(&|b, r| b.rms_norm_wide_q36(r[0], r[2], r[3])), 38, "the template it replaces");
    assert_eq!(n(&|b, r| b.l2_norm_q15(r[1])), 25, "the template it replaces");
}
