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

/// **A LoRA candidate of `parent`** (RFC-0004 §6.2–§6.3, PALW-MIP-15's composite artifact): the
/// parent's spec with the PEFT adapter (`adapter_config.json`'s text) attached, lowered with the
/// adapter's params last (`lower::adapter_params_last`). Returns the candidate and `P`, the parent's
/// param count: the candidate's params `0..P` are the parent program's, declaration for declaration
/// (refused otherwise), so materialised with the parent's calibration plus the adapter's own sites
/// ([`candidate_stats`]) its first `P` params' tensors are the parent artifact's, byte for byte, and
/// its params `P..` are the adapter's section. Its weights are the parent checkpoint under the
/// adapter's tensors (`weights::Overlay`).
pub fn prepare_candidate(parent: &Prepared, adapter_config: &str, opts: &LowerOpts) -> Result<(Prepared, usize)> {
    let mut spec = parent.spec.clone();
    crate::lora::attach(&mut spec, adapter_config)?;
    let hl = crate::hl::build_program(&spec)?;
    let binding = crate::hf_weights::bind(&spec, &hl)?;
    let mut lowered = lower(&hl, opts)?;
    let p = crate::lower::adapter_params_last(&mut lowered)?;
    if lowered.program.params[..p] != parent.lowered.program.params[..] {
        return Err(LowerError::bad(format!(
            "the candidate's first {p} params are not the parent's {}: the adapter changed a parent declaration",
            parent.lowered.program.params.len()
        )));
    }
    Ok((Prepared { spec, hl, binding, lowered }, p))
}

/// **A candidate's calibration**: the parent's statistics for every parent site, and the
/// candidate's own (a float run of parent + adapter) only at the adapter's sites (keys holding
/// [`crate::lower::LORA_MARK`]) — every parent tensor then keeps the scales it has in the parent's
/// artifact.
pub fn candidate_stats(parent: &BTreeMap<String, SiteStat>, candidate: BTreeMap<String, SiteStat>) -> BTreeMap<String, SiteStat> {
    let mut out = parent.clone();
    out.extend(candidate.into_iter().filter(|(k, _)| k.contains(crate::lower::LORA_MARK)));
    out
}

/// Per-site statistics of the float reference over `seqs` (keys `pre.embed`, `L3.attn.q`, …).
pub fn calibrate(
    hl: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
    progress: &dyn Fn(usize, usize),
) -> Result<BTreeMap<String, SiteStat>> {
    Ok(run_layer_major(hl, loader, seqs, true, false, progress)?.stats)
}

/// Float logits per sequence and position.
pub fn float_logits(
    hl: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<Vec<Vec<f32>>>> {
    Ok(run_layer_major(hl, loader, seqs, false, true, progress)?.logits)
}

/// Logits of the integer program for one sequence, by the reference evaluator, as floats
/// (`code · logits_scale`). `progress(pos)` after each position.
pub fn int_logits(
    program: &tir::TirProgramV1,
    params: &IntParams,
    seq: &[usize],
    logits_scale: f64,
    progress: &dyn Fn(usize),
) -> Result<Vec<Vec<f64>>> {
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

/// [`int_logits`] on the typed backend (`misaka-palw-tir-exec`, byte-identical to the reference
/// evaluator: freeze criterion 4, `tests/three_way.rs`) — the same logits, about 300× faster at
/// 1.5B. The params are borrowed, not copied.
pub fn int_logits_exec(
    program: &tir::TirProgramV1,
    params: &IntParams,
    seq: &[usize],
    logits_scale: f64,
    progress: &dyn Fn(usize),
) -> Result<Vec<Vec<f64>>> {
    use crate::lower::IntData;
    use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};
    use std::borrow::Cow;
    let fail = |e: tir::TirError| LowerError::eval(format!("typed backend: {e}"));
    let plan = TirPlan::compile(program).map_err(fail)?;
    let mut xp = TirParams::new(&plan);
    for ((j, layer), t) in &params.tensors {
        let data = match &t.data {
            IntData::I8(v) => ParamData::I8(Cow::Borrowed(v)),
            IntData::I16(v) => ParamData::I16(Cow::Borrowed(v)),
            IntData::I32(v) => ParamData::I32(Cow::Borrowed(v)),
            IntData::I64(v) => ParamData::I64(Cow::Borrowed(v)),
            IntData::Idx(v) => ParamData::Idx(Cow::Borrowed(v)),
        };
        xp.insert(&plan, *j, *layer, data).map_err(fail)?;
    }
    let mut exec = TirExecutor::new(&plan, &xp).map_err(fail)?;
    let mut out = Vec::with_capacity(seq.len());
    for (p, t) in seq.iter().enumerate() {
        exec.step(*t as u32, &mut NoSink).map_err(|e| LowerError::eval(format!("position {p}: {e}")))?;
        let (_, l) = exec.logits();
        out.push(l.to_i128s().iter().map(|v| *v as f64 * logits_scale).collect());
        progress(p);
    }
    Ok(out)
}

/// **The calibration-length rule** (freeze-v1 §5.2): a program with a recurrence — any `Fixed`
/// state: a selective scan, a gated delta rule, a WKV state, a conv window, a token shift — is
/// calibrated on at least one sequence as long as the longest context it is evaluated or served at.
/// A recurrence's state and long-context activations otherwise get static scales sized on
/// short-context magnitudes: Jamba-tiny-dev drifted ×193 at 4,096 positions with a 128-token
/// calibration, ×0.45 with one 4,096-token sequence added. `Ok(None)` when the program has no
/// recurrence (the rule does not apply), `Ok(Some(longest))` when it is met.
pub fn check_calibration_length(hl: &HlProgram, calib: &[Vec<usize>], context: usize) -> std::result::Result<Option<usize>, String> {
    let recurrent = hl.states.iter().any(|s| matches!(s.kind, crate::hl::StateKind::Fixed));
    if !recurrent {
        return Ok(None);
    }
    let longest = calib.iter().map(Vec::len).max().unwrap_or(0);
    if longest < context {
        return Err(format!(
            "a recurrent program is calibrated on at least one sequence as long as the context it is evaluated at: \
             the longest calibration sequence has {longest} tokens, the context {context} (freeze-v1 §5.2)"
        ));
    }
    Ok(Some(longest))
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

/// [`compare`] and [`drift`] one position at a time, so a long evaluation never holds the integer
/// side's rows (4,096 positions × a 248,320-entry vocabulary is 8 GB of `f64`).
#[derive(Clone, Debug, Default)]
pub struct Accumulator {
    agree: usize,
    kl_sum: f64,
    nll_f: f64,
    nll_i: f64,
    m: Metrics,
    early: (usize, usize),
    late: usize,
    e: (f64, usize),
    l: (f64, usize),
    late_window: (usize, usize),
}

impl Accumulator {
    pub fn new(early: (usize, usize), late: usize) -> Self {
        Self { early, late, late_window: (usize::MAX, 0), ..Default::default() }
    }

    /// Position `p` of a sequence of `len` tokens whose next token is `next`.
    pub fn push(&mut self, p: usize, len: usize, f: &[f32], i: &[f64], next: Option<usize>) {
        let (lf, li) = (log_softmax(f), log_softmax(i));
        let kl: f64 = lf.iter().zip(&li).map(|(a, b)| a.exp() * (a - b)).sum();
        self.kl_sum += kl;
        self.m.kl_max = self.m.kl_max.max(kl);
        if argmax(f) == argmax(i) {
            self.agree += 1;
        }
        self.m.positions += 1;
        if let Some(t) = next {
            self.nll_f -= lf[t];
            self.nll_i -= li[t];
            self.m.scored += 1;
        }
        if p >= self.early.0 && p < self.early.1 {
            self.e = (self.e.0 + kl, self.e.1 + 1);
        }
        let late_from = len.saturating_sub(self.late);
        if p >= late_from {
            self.l = (self.l.0 + kl, self.l.1 + 1);
            self.late_window = (self.late_window.0.min(late_from), self.late_window.1.max(len));
        }
    }

    pub fn metrics(&self) -> Metrics {
        let mut m = self.m.clone();
        if m.positions > 0 {
            m.top1_agreement = self.agree as f64 / m.positions as f64;
            m.kl_mean = self.kl_sum / m.positions as f64;
        }
        if m.scored > 0 {
            m.ppl_float = (self.nll_f / m.scored as f64).exp();
            m.ppl_int = (self.nll_i / m.scored as f64).exp();
            m.ppl_delta = m.ppl_int / m.ppl_float - 1.0;
        }
        m
    }

    pub fn drift(&self) -> Drift {
        let (kl_early, kl_late) = (self.e.0 / self.e.1.max(1) as f64, self.l.0 / self.l.1.max(1) as f64);
        Drift {
            early_window: self.early,
            late_window: self.late_window,
            kl_early,
            kl_late,
            ratio: if kl_early > 0.0 { kl_late / kl_early } else { f64::NAN },
        }
    }
}

/// [`int_logits_exec`] handing each position's row to `row(p, logits)` instead of keeping it.
pub fn int_logits_exec_each(
    program: &tir::TirProgramV1,
    params: &IntParams,
    seq: &[usize],
    logits_scale: f64,
    row: &mut dyn FnMut(usize, &[f64]),
) -> Result<()> {
    use crate::lower::IntData;
    use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};
    use std::borrow::Cow;
    let fail = |e: tir::TirError| LowerError::eval(format!("typed backend: {e}"));
    let plan = TirPlan::compile(program).map_err(fail)?;
    let mut xp = TirParams::new(&plan);
    for ((j, layer), t) in &params.tensors {
        let data = match &t.data {
            IntData::I8(v) => ParamData::I8(Cow::Borrowed(v)),
            IntData::I16(v) => ParamData::I16(Cow::Borrowed(v)),
            IntData::I32(v) => ParamData::I32(Cow::Borrowed(v)),
            IntData::I64(v) => ParamData::I64(Cow::Borrowed(v)),
            IntData::Idx(v) => ParamData::Idx(Cow::Borrowed(v)),
        };
        xp.insert(&plan, *j, *layer, data).map_err(fail)?;
    }
    let mut exec = TirExecutor::new(&plan, &xp).map_err(fail)?;
    let mut buf = Vec::new();
    for (p, t) in seq.iter().enumerate() {
        exec.step(*t as u32, &mut NoSink).map_err(|e| LowerError::eval(format!("position {p}: {e}")))?;
        let (_, l) = exec.logits();
        buf.clear();
        buf.extend(l.to_i128s().iter().map(|v| *v as f64 * logits_scale));
        row(p, &buf);
    }
    Ok(())
}

/// **Recurrence drift** (corpus-v1 §9's column for C4–C7): the mean KL over positions
/// `[early.0, early.1)` and over the last `late` positions, across every sequence, and their ratio
/// (the criterion is `late ≤ 1.5 × early` at 4,096 against 128).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Drift {
    pub early_window: (usize, usize),
    pub late_window: (usize, usize),
    pub kl_early: f64,
    pub kl_late: f64,
    pub ratio: f64,
}

pub fn drift(float: &[Vec<Vec<f32>>], int: &[Vec<Vec<f64>>], early: (usize, usize), late: usize) -> Drift {
    let (mut e, mut ne, mut l, mut nl) = (0f64, 0usize, 0f64, 0usize);
    let mut late_window = (usize::MAX, 0);
    for (fs, is) in float.iter().zip(int) {
        let n = fs.len().min(is.len());
        let late_from = n.saturating_sub(late);
        late_window = (late_window.0.min(late_from), late_window.1.max(n));
        for (p, (f, i)) in fs.iter().zip(is).enumerate() {
            let (lf, li) = (log_softmax(f), log_softmax(i));
            let kl: f64 = lf.iter().zip(&li).map(|(a, b)| a.exp() * (a - b)).sum();
            if p >= early.0 && p < early.1 {
                e += kl;
                ne += 1;
            }
            if p >= late_from {
                l += kl;
                nl += 1;
            }
        }
    }
    let (kl_early, kl_late) = (e / ne.max(1) as f64, l / nl.max(1) as f64);
    Drift { early_window: early, late_window, kl_early, kl_late, ratio: if kl_early > 0.0 { kl_late / kl_early } else { f64::NAN } }
}

/// The error of one site of one occurrence: every committed node that holds an HL site's value,
/// decoded with its scale, against the float reference's value at the same position.
#[derive(Clone, Debug, Serialize)]
pub struct SiteError {
    pub key: String,
    /// `‖int − float‖ / ‖float‖` over every position.
    pub rel_l2: f64,
    pub max_abs: f64,
    pub float_absmax: f64,
}

/// Per-site errors along one sequence (position-major float run with a trace, so small models
/// only), worst first.
#[allow(clippy::too_many_arguments)]
pub fn site_errors(
    prep: &Prepared,
    params_f: &crate::float_ref::ParamStore,
    stats: &BTreeMap<String, SiteStat>,
    policy: &crate::quant::QuantPolicy,
    mat: &crate::lower::Materialised,
    seq: &[usize],
) -> Result<Vec<SiteError>> {
    use crate::lower::FillCtx;
    let hl = &prep.hl;
    let lw = &prep.lowered;
    let p = &lw.program;
    let interp = tir::Interpreter::new(p).map_err(|e| LowerError::eval(e.to_string()))?;
    let mut state = tir::RunState::default();
    let mut sess = crate::float_ref::Session::new(hl, params_f).with_trace();
    let empty = crate::float_ref::ParamStore::default();
    // (key) → (Σ(a−b)², Σb², max|a−b|, max|b|)
    let mut acc: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();
    for t in seq {
        sess.step(*t)?;
        let tr = sess.trace.clone().unwrap_or_default();
        let step = interp.step(&mat.params, &mut state, *t as u32).map_err(|e| LowerError::eval(e.to_string()))?;
        for c in &step.commits {
            let Some((site, key, len)) = lw.site_nodes.get(&(c.block, c.node)) else { continue };
            let prefix = if c.block == p.schedule.pre {
                "pre.".to_string()
            } else if c.block == p.schedule.post {
                "post.".to_string()
            } else {
                format!("L{}.", c.layer.unwrap_or(0))
            };
            let k = format!("{prefix}{site}");
            let Some(fv) = tr.get(&k) else { continue };
            if fv.len() != c.value.data.len() {
                continue;
            }
            let ctx = FillCtx::for_scales(hl, &empty, c.layer.map(|l| l as usize), &prefix, stats, mat.resid_scale, policy);
            let sv = if key.split() > 0 { ctx.scale_vec(key, *len)? } else { vec![ctx.scale(key)?; fv.len()] };
            let e = acc.entry(k).or_insert((0.0, 0.0, 0.0, 0.0));
            for (i, (iv, f)) in c.value.data.iter().zip(fv).enumerate() {
                let a = *iv as f64 * sv[i % sv.len()];
                let d = a - *f as f64;
                e.0 += d * d;
                e.1 += (*f as f64) * (*f as f64);
                e.2 = e.2.max(d.abs());
                e.3 = e.3.max((*f as f64).abs());
            }
        }
    }
    let mut out: Vec<SiteError> = acc
        .into_iter()
        .map(|(key, (dd, bb, mx, fm))| SiteError { key, rel_l2: (dd / bb.max(1e-300)).sqrt(), max_abs: mx, float_absmax: fm })
        .collect();
    out.sort_by(|a, b| b.rel_l2.partial_cmp(&a.rel_l2).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// **Per-site errors by position window** — the drift's diagnosis. The traced float session and
/// the typed backend run one sequence position by position (the backend hands its committed values
/// to a sink); a site's error in a window is `‖int − float‖ / ‖float‖` over the window's
/// positions. Returns `(site, error per window)`, in site order. Holds one position of either side
/// at a time, so a 4,096-position run needs only the two models.
#[allow(clippy::too_many_arguments)]
pub fn site_errors_windows(
    prep: &Prepared,
    params_f: &crate::float_ref::ParamStore,
    stats: &BTreeMap<String, SiteStat>,
    policy: &crate::quant::QuantPolicy,
    mat: &crate::lower::Materialised,
    seq: &[usize],
    windows: &[(usize, usize)],
    progress: &dyn Fn(usize),
) -> Result<Vec<(String, Vec<f64>)>> {
    use crate::lower::{FillCtx, IntData};
    use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
    use std::borrow::Cow;
    struct Grab<'s> {
        want: &'s BTreeMap<(u8, u16), (String, crate::lower::ScaleKey, usize)>,
        got: Vec<(u8, Option<u16>, u16, Vec<i128>)>,
    }
    impl StepSink for Grab<'_> {
        fn node(&mut self, v: &NodeValue<'_>) {
            if v.commit && self.want.contains_key(&(v.block, v.node)) {
                self.got.push((v.block, v.layer, v.node, v.data.to_i128s()));
            }
        }
    }
    let hl = &prep.hl;
    let lw = &prep.lowered;
    let program = &lw.program;
    let fail = |e: tir::TirError| LowerError::eval(format!("typed backend: {e}"));
    let plan = TirPlan::compile(program).map_err(fail)?;
    let mut xp = TirParams::new(&plan);
    for ((j, layer), t) in &mat.params.tensors {
        let data = match &t.data {
            IntData::I8(v) => ParamData::I8(Cow::Borrowed(v)),
            IntData::I16(v) => ParamData::I16(Cow::Borrowed(v)),
            IntData::I32(v) => ParamData::I32(Cow::Borrowed(v)),
            IntData::I64(v) => ParamData::I64(Cow::Borrowed(v)),
            IntData::Idx(v) => ParamData::Idx(Cow::Borrowed(v)),
        };
        xp.insert(&plan, *j, *layer, data).map_err(fail)?;
    }
    let mut exec = TirExecutor::new(&plan, &xp).map_err(fail)?;
    let mut sess = crate::float_ref::Session::new(hl, params_f).with_trace();
    let empty = crate::float_ref::ParamStore::default();
    // (site) → per window (Σ(a−b)², Σb²); scales cached per (block, layer, node).
    let mut acc: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut scales: BTreeMap<(u8, Option<u16>, u16), Vec<f64>> = BTreeMap::new();
    let last = windows.iter().map(|w| w.1).max().unwrap_or(0).min(seq.len());
    for (p, t) in seq[..last].iter().enumerate() {
        sess.step(*t)?;
        let mut grab = Grab { want: &lw.site_nodes, got: Vec::new() };
        exec.step(*t as u32, &mut grab).map_err(|e| LowerError::eval(format!("position {p}: {e}")))?;
        progress(p);
        let inside: Vec<usize> = windows.iter().enumerate().filter(|(_, w)| p >= w.0 && p < w.1).map(|(i, _)| i).collect();
        if inside.is_empty() {
            continue;
        }
        let tr = sess.trace.as_ref().ok_or_else(|| LowerError::eval("internal: no trace"))?;
        for (block, layer, node, vals) in grab.got {
            let Some((site, key, len)) = lw.site_nodes.get(&(block, node)) else { continue };
            let prefix = if block == program.schedule.pre {
                "pre.".to_string()
            } else if block == program.schedule.post {
                "post.".to_string()
            } else {
                format!("L{}.", layer.unwrap_or(0))
            };
            let k = format!("{prefix}{site}");
            let Some(fv) = tr.get(&k) else { continue };
            if fv.len() != vals.len() {
                continue;
            }
            let sv = match scales.get(&(block, layer, node)) {
                Some(v) => v,
                None => {
                    let ctx = FillCtx::for_scales(hl, &empty, layer.map(|l| l as usize), &prefix, stats, mat.resid_scale, policy);
                    let v = if key.split() > 0 { ctx.scale_vec(key, *len)? } else { vec![ctx.scale(key)?; fv.len()] };
                    scales.entry((block, layer, node)).or_insert(v)
                }
            };
            let e = acc.entry(k).or_insert_with(|| vec![(0.0, 0.0); windows.len()]);
            let (mut dd, mut bb) = (0.0, 0.0);
            for (i, (iv, f)) in vals.iter().zip(fv).enumerate() {
                let d = *iv as f64 * sv[i % sv.len()] - *f as f64;
                dd += d * d;
                bb += (*f as f64) * (*f as f64);
            }
            for w in &inside {
                e[*w].0 += dd;
                e[*w].1 += bb;
            }
        }
    }
    Ok(acc.into_iter().map(|(k, v)| (k, v.iter().map(|(dd, bb)| (dd / bb.max(1e-300)).sqrt()).collect())).collect())
}

/// Seeded pseudo-random token sequences (tiny models have no text).
pub fn random_sequences(vocab: usize, count: usize, len: usize, seed: u64) -> Vec<Vec<usize>> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    (0..count).map(|_| (0..len).map(|_| rng.gen_range(0..vocab)).collect()).collect()
}
