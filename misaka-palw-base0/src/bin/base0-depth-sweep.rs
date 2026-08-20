//! **How deep can PALW-BASE-0 go?** — the measurement the second class's geometry waits on.
//!
//! ADR-0040 Decision D fixes `AddElem` at `i8 -> i32`, so the residual stream between layers is
//! `int8` and widening it is a new kernel, i.e. the one change that re-opens the catalog. The
//! artifact carries ONE `residual_requant`, applied at both residual adds in every layer, and the
//! scale algebra leaves exactly two settings with no third option:
//!
//! * **gain 1/2** (`shift = 1`, what `derive_deterministic` picks): `sigma_h` doubles at every add,
//!   so the stream never saturates — and a layer's contribution is attenuated by a further factor
//!   of four for every layer above it. Depth is spent on nothing.
//! * **gain 1** (`shift = 0`): `sigma_h` is constant and early layers keep their weight — until
//!   `h + projected` leaves `[-128, 127]` and `Saturate8` clips the stream.
//!
//! So "how deep" is not rhetorical, it is a number, and it decides whether a Qwen-scale BASE-0
//! class (24-28 layers) is even arithmetically reachable before any question of model quality.
//!
//! # What is measured, and what these numbers are NOT
//!
//! The weights are `derive_deterministic`'s seeded LCG, because no trained BASE-0 artifact exists.
//! That makes this a measurement of **the arithmetic's headroom, not of a model's quality**: real
//! weights have outliers and structured directions a uniform fill does not, and a real PTQ
//! calibration can choose better requantisation parameters than the fixture's fan-in heuristic.
//! Read the result as *"the plumbing carries at most this far"* — a bound on what an honest
//! pipeline could reach, not a prediction of what one will.
//!
//! Two metrics, because the first one alone cannot see the failure that matters:
//!
//! 1. **`residual_peak` per layer** — catches SATURATION (pinned at 127) and COLLAPSE (driven to
//!    0). It cannot catch decay: under gain 1/2 the stream stays healthy-looking at every depth
//!    while the early layers' contribution is being divided away underneath it.
//! 2. **Influence by ablation** — for each layer, zero that layer's two writes into the residual
//!    (`w_o` and `w_down`) and measure how far the final logits move, **as a magnitude**.
//!
//! The magnitude is the whole point, and the first version of this harness got it wrong by asking
//! whether the movement was nonzero. It always is, and for a reason worth stating: `RmsNorm` is
//! scale-invariant, so every layer RENORMALISES the residual stream back to full Q7 range before
//! projecting it. Under gain 1/2 an early layer's contribution becomes a smaller and smaller
//! FRACTION of `h`, but the norm keeps rescaling what remains, and rounding keeps it off zero. So
//! "is this layer dead" has the answer "no" at every depth and tells you nothing. "How much of the
//! output does this layer still control" is the question, and its answer is a curve.
//!
//! Run with `--release`: the engine is scalar by construction and a debug build turns this from
//! seconds into minutes.

use kaspa_consensus_core::palw_base0_ops::QuantParams;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::engine::{Base0Engine, ForwardProbe, KvCache};

/// One weight fill for every configuration, so a difference between two rows is the configuration
/// and never the dice.
const SEED: u64 = 0xBA5E_0DEE_D0D0_0001;

/// Tokens pushed through before anything is read. Attention has to have a history for
/// `attention_spread` to mean anything, and the residual stream has to have been written to more
/// than once for its peak to be a steady state rather than an artefact of position 0.
const TOKENS: usize = 8;

/// `int8` codes at Q7, so a stream pinned here is one `Saturate8` is clipping.
const CODE_MAX: i32 = 127;

struct Width {
    name: &'static str,
    n_heads: usize,
    d_head: usize,
    d_ff: usize,
}

const WIDTHS: &[Width] = &[
    // The RC floor's own width, so the numbers are directly comparable to the class that ships.
    Width { name: "rc-256", n_heads: 4, d_head: 64, d_ff: 512 },
    // Four times wider. Present to answer "is the depth limit a function of width?", which decides
    // whether a Qwen-scale class can buy depth by being wider.
    Width { name: "wide-1024", n_heads: 8, d_head: 128, d_ff: 2048 },
];

const DEPTHS: &[usize] = &[4, 8, 16, 24, 32];

/// What to do at the residual add. The third mode is not a third setting of `residual_requant` —
/// it is the same unity setting with the two projections that WRITE the residual attenuated
/// instead, which is what a real calibration does and what the fixture cannot express by itself.
///
/// The distinction matters: halving `h + projected` attenuates the accumulated stream AND the new
/// contribution together, while attenuating the projection leaves `sigma_h` fixed and only shrinks
/// what is being added. Only the second preserves an early layer's share of the output.
#[derive(Clone, Copy)]
enum Mode {
    /// `residual_requant` gain 1/2. `sigma_h` doubles per add.
    Halve,
    /// `residual_requant` gain 1, projections untouched. `sigma_h` constant, and `h + projected`
    /// is free to leave `[-128, 127]`.
    Unity,
    /// Gain 1, with `requant[3]` (attention out) and `requant[6]` (FFN down) shifted one further
    /// so the added term is halved rather than the stream.
    UnityAttenuated,
}

const MODES: &[(&str, Mode)] =
    &[("halve", Mode::Halve), ("unity", Mode::Unity), ("unity+atten", Mode::UnityAttenuated)];

fn apply_mode(artifact: &mut Base0ArtifactV1, mode: Mode) {
    match mode {
        Mode::Halve => artifact.residual_requant = QuantParams { multiplier: i32::MAX, shift: 1, zero: 0 },
        Mode::Unity => artifact.residual_requant = QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 },
        Mode::UnityAttenuated => {
            artifact.residual_requant = QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 };
            for layer in artifact.layers.iter_mut() {
                for slot in [3usize, 6] {
                    layer.requant[slot].shift += 1;
                }
            }
        }
    }
}

fn shape(w: &Width, n_layers: usize) -> Base0ShapeV1 {
    Base0ShapeV1 {
        n_layers,
        n_heads: w.n_heads,
        // MHA: the sweep is about the residual stream's depth, and grouping the kv heads would
        // change the attention arithmetic underneath it without changing what is being measured.
        n_kv_heads: w.n_heads,
        d_head: w.d_head,
        d_ff: w.d_ff,
        vocab: 4_096,
        max_position: 512,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        // The RC geometry's epsilon, at Qk.
        eps_q: 1 << 8,
    }
}

/// Run `TOKENS` positions and return the last position's logits beside the last probe.
///
/// The prompt is a fixed arithmetic walk over the vocabulary rather than a constant token: a
/// repeated token makes every position's attention read identical keys, which flatters
/// `attention_spread` for a reason that has nothing to do with the arithmetic under test.
fn run(artifact: &Base0ArtifactV1) -> (Vec<i32>, ForwardProbe) {
    let engine = Base0Engine::new(artifact);
    let mut cache = KvCache::new(artifact);
    let mut last = None;
    for position in 0..TOKENS {
        let token = (position * 37 + 11) % artifact.shape.vocab;
        last = Some(engine.forward_token_probed(&mut cache, token, position).expect("the fixture's shapes are valid by construction"));
    }
    last.expect("TOKENS is non-zero")
}

/// Zero layer `victim`'s two writes into the residual stream.
///
/// `w_o` and `w_down` are the only paths by which a layer reaches `h`, so zeroing both removes the
/// layer's entire contribution while leaving the graph, the cache shape and every other layer's
/// arithmetic identical. Anything that still moves in the output moved because of that layer.
fn ablate(artifact: &Base0ArtifactV1, victim: usize) -> Base0ArtifactV1 {
    let mut a = artifact.clone();
    a.layers[victim].wo.iter_mut().for_each(|w| *w = 0);
    a.layers[victim].w_down.iter_mut().for_each(|w| *w = 0);
    a
}

/// L1 distance between two logit vectors. Absolute, not relative: the question is whether the
/// output moved AT ALL, and a relative measure hides "moved by one unit out of a billion" as zero.
fn l1(a: &[i32], b: &[i32]) -> i64 {
    a.iter().zip(b).map(|(x, y)| (*x as i64 - *y as i64).abs()).sum()
}

fn main() {
    println!("# PALW-BASE-0 depth sweep");
    println!("# seed={SEED:#x} tokens={TOKENS} vocab=4096 — fixture weights, arithmetic headroom only");
    println!();

    for width in WIDTHS {
        for (mode_name, mode) in MODES {
            println!("## width={} residual={mode_name}", width.name);
            println!(
                "| layers | peak lo..hi | rail | influence L0 | L(n/2) | L(last) | last/first | attn spread (min%) | gate asym |"
            );
            println!("|---|---|---|---|---|---|---|---|---|");

            for &n_layers in DEPTHS {
                let mut artifact =
                    Base0ArtifactV1::derive_deterministic(shape(width, n_layers), SEED).expect("the swept shapes are valid");
                apply_mode(&mut artifact, *mode);

                let (base_logits, probe) = run(&artifact);
                let peaks = &probe.residual_peak;
                let (lo, hi) = (peaks.iter().min().copied().unwrap_or(0), peaks.iter().max().copied().unwrap_or(0));
                // A layer whose peak reached the rail had at least one code clipped by Saturate8.
                let railed = peaks.iter().filter(|p| **p >= CODE_MAX).count();

                // The influence curve: how far the logits move when each layer is removed.
                let influence: Vec<i64> =
                    (0..n_layers).map(|victim| l1(&base_logits, &run(&ablate(&artifact, victim)).0)).collect();
                let first = influence[0].max(1);
                let last = *influence.last().expect("n_layers > 0");
                // Attention health, as a fraction of the uniform distribution it degenerates to.
                let uniform = ForwardProbe::uniform_probability(TOKENS).max(1) as i64;
                let min_spread_pct = probe.attention_spread.iter().map(|s| *s as i64 * 100 / uniform).min().unwrap_or(0);
                // A working SiLU floors at -0.278 and passes positives: |min| should be well under
                // max. The degenerate linear x/2 is symmetric, so a ratio near 100% is the tell.
                let gate_asym_pct = median(
                    &probe.gate_extremes.iter().map(|(l, h)| l.unsigned_abs() as i64 * 100 / (*h).max(1) as i64).collect::<Vec<_>>(),
                );

                println!(
                    "| {n_layers} | {lo}..{hi} | {railed}/{n_layers} | {} | {} | {} | {:.4} | {min_spread_pct} | {gate_asym_pct} |",
                    influence[0],
                    influence[n_layers / 2],
                    last,
                    last as f64 / first as f64,
                );
            }
            println!();
        }
    }

    println!("## Residual memory — what one add does to a value already in the stream");
    println!();
    println!("| gain | decay of a maximal code, add by add | adds to 1 unit | floor |");
    println!("|---|---|---|---|");
    for shift in [0u8, 1, 2] {
        let (trace, fixed) = residual_decay(QuantParams { multiplier: i32::MAX, shift, zero: 0 });
        let to_unit = trace.iter().position(|c| c.abs() <= 1).map(|i| i.to_string()).unwrap_or_else(|| ">24".into());
        let floor = if fixed { format!("±{} forever", trace.last().expect("non-empty")) } else { "0".to_string() };
        let seq: Vec<String> = trace.iter().map(|c| c.to_string()).collect();
        println!("| 2^-{shift} | {} | {to_unit} | {floor} |", seq.join(" → "));
    }
    println!();

    println!("## The other side of unity gain — how far each layer's write must be attenuated");
    println!("| width | layers | extra shift needed | resulting peak | bits left per write |");
    println!("|---|---|---|---|---|");
    for width in WIDTHS {
        for &n_layers in DEPTHS {
            match minimum_write_attenuation(width, n_layers) {
                Some((extra, peak)) => {
                    println!("| {} | {n_layers} | +{extra} | {peak} | {} |", width.name, 7i32 - extra as i32)
                }
                None => println!("| {} | {n_layers} | **>7 — still railing** | 128 | <0 |", width.name),
            }
        }
    }
}

/// **How many residual adds a value survives** — the number the whole question reduces to.
///
/// `h` passes through `Requantize(h + projected, residual_requant)` at every add. Ignore the added
/// term and the operation on what is already there is exactly `requantize(code, mult, shift)`
/// applied again and again, so a feature written by an early layer is a geometric sequence with
/// `int8` rounding underneath it. This counts, with the real primitive rather than a model of it,
/// how many adds the LARGEST representable code survives before it rounds to nothing.
///
/// Nothing empirical can override this: it is what the arithmetic does to the residual stream's
/// memory regardless of weights, calibration or model quality.
fn residual_decay(params: QuantParams) -> (Vec<i32>, bool) {
    let mut code = CODE_MAX;
    let mut trace = vec![code];
    for _ in 0..24 {
        let next = kaspa_consensus_core::palw_base0::requantize(code, params.multiplier, params.shift) as i32;
        if next == code {
            // A fixed point. Under gain 1/2 this is ±1 and NOT zero, because ADR-0040 C1 rounds
            // half AWAY from zero on the magnitude: `RSR(1, 1)` is 1, not 0. The rule was chosen
            // for gemmlowp agreement, and a side effect is that the residual stream never
            // completely forgets — it forgets everything except a sign.
            return (trace, true);
        }
        code = next;
        trace.push(code);
        if code == 0 {
            return (trace, false);
        }
    }
    (trace, false)
}

/// The smallest extra attenuation on the two projections that WRITE the residual which keeps the
/// stream off the `Saturate8` rail, under a unity residual gain.
///
/// This is the other half of the trade. Unity gain buys unbounded memory; what it costs is that
/// every layer's write must be small enough that the accumulated sum never clips, and this
/// measures how small — in bits, which is the currency that matters, because a write attenuated by
/// `2^b` has `7 - b` bits of the code range left to say anything with.
fn minimum_write_attenuation(width: &Width, n_layers: usize) -> Option<(u8, i32)> {
    for extra in 0..=7u8 {
        let mut artifact =
            Base0ArtifactV1::derive_deterministic(shape(width, n_layers), SEED).expect("the swept shapes are valid");
        artifact.residual_requant = QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 };
        for layer in artifact.layers.iter_mut() {
            for slot in [3usize, 6] {
                layer.requant[slot].shift += extra;
            }
        }
        let (_, probe) = run(&artifact);
        let peak = probe.residual_peak.iter().max().copied().unwrap_or(0);
        if peak < CODE_MAX {
            return Some((extra, peak));
        }
    }
    None
}

/// Median of a slice, by sorting a copy. Median rather than mean because one degenerate layer
/// should not be able to hide behind thirty-one healthy ones, nor vice versa.
fn median(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    v[v.len() / 2]
}
