//! **The fidelity harness** (Gate 2a): the integer program, run by the `misaka-palw-tir`
//! reference evaluator on its artifact, against the float reference of the same checkpoint.
//!
//! Teacher-forced on fixed token sequences. Per position: top-1 agreement, `KL(p_float ‖ p_int)`
//! in nats, and the next-token negative log-likelihood of both (so the perplexity ratio). These
//! decide whether a lowering is USEFUL (corpus-v1 §9); they never decide whether a claim is valid —
//! that is bit identity, which the evaluator gives by construction.
//!
//! The pipeline, every stage streaming one occurrence at a time so a 1.5B checkpoint fits in a few
//! GB: [`prepare`] (config → ArchSpec → HL → TIR), [`calibrate`] (float reference over the
//! calibration set, per-site statistics), [`crate::lower::materialise`] (the integer artifact),
//! [`float_logits`] and [`int_logits`], then [`compare`].

use crate::error::{LowerError, Result};
use crate::float_ref::SiteStat;
use crate::float_ref::stream::{OccParams, run_layer_major};
use crate::hl::HlProgram;
use crate::lower::{IntParams, LowerOpts, Lowered, lower};
use crate::spec::ArchSpec;
use crate::weights::Binding;
use misaka_palw_tir as tir;
use serde::Serialize;
use std::collections::BTreeMap;

/// Everything derived from a config: the spec, the HL program, its weight binding and the TIR
/// lowering.
pub struct Prepared {
    pub spec: ArchSpec,
    pub hl: HlProgram,
    pub binding: Binding,
    pub lowered: Lowered,
}

pub fn prepare(config_text: &str, opts: &LowerOpts) -> Result<Prepared> {
    let spec = crate::parse_config_str(config_text)?;
    let hl = crate::hl::build_program(&spec)?;
    let binding = crate::hf_weights::bind(&spec, &hl)?;
    let lowered = lower(&hl, opts)?;
    Ok(Prepared { spec, hl, binding, lowered })
}

/// Per-site statistics of the float reference over `seqs` (keys `pre.embed`, `L3.attn.q`, …).
pub fn calibrate(hl: &HlProgram, loader: &dyn OccParams, seqs: &[Vec<usize>], progress: &dyn Fn(usize, usize)) -> Result<BTreeMap<String, SiteStat>> {
    Ok(run_layer_major(hl, loader, seqs, true, false, progress)?.stats)
}

/// Float logits per sequence and position.
pub fn float_logits(hl: &HlProgram, loader: &dyn OccParams, seqs: &[Vec<usize>], progress: &dyn Fn(usize, usize)) -> Result<Vec<Vec<Vec<f32>>>> {
    Ok(run_layer_major(hl, loader, seqs, false, true, progress)?.logits)
}

/// Logits of the integer program for one sequence, by the reference evaluator, as floats
/// (`code · logits_scale`). `progress(pos)` after each position.
pub fn int_logits(program: &tir::TirProgramV1, params: &IntParams, seq: &[usize], logits_scale: f64, progress: &dyn Fn(usize)) -> Result<Vec<Vec<f64>>> {
    let interp = tir::Interpreter::new(program).map_err(|e| LowerError::eval(format!("evaluator refused the program: {e}")))?;
    let mut state = tir::RunState::default();
    let mut out = Vec::with_capacity(seq.len());
    for (p, t) in seq.iter().enumerate() {
        let step = interp.step(params, &mut state, *t as u32).map_err(|e| LowerError::eval(format!("position {p}: {e}")))?;
        out.push(step.logits.data.iter().map(|v| *v as f64 * logits_scale).collect());
        progress(p);
    }
    Ok(out)
}

/// Fidelity of one set of sequences.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Metrics {
    pub positions: usize,
    pub top1_agreement: f64,
    pub kl_mean: f64,
    pub kl_max: f64,
    /// Next-token positions scored (every position but each sequence's last).
    pub scored: usize,
    pub ppl_float: f64,
    pub ppl_int: f64,
    /// `ppl_int / ppl_float − 1`.
    pub ppl_delta: f64,
}

fn log_softmax<T: Copy + Into<f64>>(v: &[T]) -> Vec<f64> {
    let m = v.iter().map(|x| (*x).into()).fold(f64::NEG_INFINITY, f64::max);
    let z = v.iter().map(|x| ((*x).into() - m).exp()).sum::<f64>().ln() + m;
    v.iter().map(|x| (*x).into() - z).collect()
}

fn argmax<T: Copy + Into<f64>>(v: &[T]) -> usize {
    let mut best = 0;
    for (i, x) in v.iter().enumerate() {
        if (*x).into() > v[best].into() {
            best = i;
        }
    }
    best
}

pub fn compare(float: &[Vec<Vec<f32>>], int: &[Vec<Vec<f64>>], seqs: &[Vec<usize>]) -> Metrics {
    let mut m = Metrics::default();
    let (mut agree, mut kl_sum, mut nll_f, mut nll_i) = (0usize, 0f64, 0f64, 0f64);
    for ((fs, is), toks) in float.iter().zip(int).zip(seqs) {
        for (p, (f, i)) in fs.iter().zip(is).enumerate() {
            let (lf, li) = (log_softmax(f), log_softmax(i));
            let kl: f64 = lf.iter().zip(&li).map(|(a, b)| a.exp() * (a - b)).sum();
            kl_sum += kl;
            m.kl_max = m.kl_max.max(kl);
            if argmax(f) == argmax(i) {
                agree += 1;
            }
            m.positions += 1;
            if p + 1 < toks.len() {
                let t = toks[p + 1];
                nll_f -= lf[t];
                nll_i -= li[t];
                m.scored += 1;
            }
        }
    }
    if m.positions > 0 {
        m.top1_agreement = agree as f64 / m.positions as f64;
        m.kl_mean = kl_sum / m.positions as f64;
    }
    if m.scored > 0 {
        m.ppl_float = (nll_f / m.scored as f64).exp();
        m.ppl_int = (nll_i / m.scored as f64).exp();
        m.ppl_delta = m.ppl_int / m.ppl_float - 1.0;
    }
    m
}

/// Seeded pseudo-random token sequences (tiny models have no text).
pub fn random_sequences(vocab: usize, count: usize, len: usize, seed: u64) -> Vec<Vec<usize>> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    (0..count).map(|_| (0..len).map(|_| rng.gen_range(0..vocab)).collect()).collect()
}
