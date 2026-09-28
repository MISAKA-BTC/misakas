//! **The f32 reference of the HL graph** — one position per step, with KV histories and
//! recurrent states, faithful to the `transformers` implementation of each architecture.
//!
//! Storage is f32 like HF's fp32 runs; dot products, norms and softmax sums accumulate in f64
//! (HF accumulates in f32 in its own order, so agreement is to ~1e-6 relative, not bit-exact —
//! bit identity is a TIR property, fidelity is this module's). Every node with a site records
//! its output statistics when enabled; that is what Gate 2 calibrates from.
//!
//! Semantics chosen where HF leaves a choice:
//! * top-k ties break to the LOWEST index and the set is returned in index order (PALW-TIR-11);
//!   `torch.topk` does not specify ties, which random logits never hit;
//! * position-dependent rope (dynamic NTK, LongRoPE) uses the decode-path reading `seq_len = pos+1`;
//! * a Hist window keeps the last `window` rows including the current one (`kv > q − window`).

use crate::error::{LowerError, Result};
use crate::hl::*;
use crate::rope::{RopeStyle, bf16_round};
use crate::spec::{Act, Glu, GroupScore, HeadMap, NormKind, RouterSpec, Scoring};
use crate::weights::{Binding, Resolver, Tensor, TensorSource, eval_src, layers_of_param};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

/// Bound params: globals, and per-layer params by `(param, layer)`.
#[derive(Default, Clone)]
pub struct ParamStore {
    global: BTreeMap<u32, Tensor>,
    layered: BTreeMap<(u32, usize), Tensor>,
}

impl ParamStore {
    pub fn get(&self, p: u32, layer: Option<usize>) -> Result<&Tensor> {
        match layer {
            Some(l) => self.layered.get(&(p, l)).or_else(|| self.global.get(&p)),
            None => self.global.get(&p),
        }
        .ok_or_else(|| LowerError::eval(format!("param {p} (layer {layer:?}) is not bound")))
    }

    /// Bind every param from checkpoint tensors; returns the store and the checkpoint tensors
    /// the program never read.
    pub fn from_source(prog: &HlProgram, binding: &Binding, source: &dyn TensorSource) -> Result<(ParamStore, Vec<String>)> {
        if binding.srcs.len() != prog.params.len() {
            return Err(LowerError::eval("binding does not match the program's params"));
        }
        let r = Resolver::new(source, &binding.aliases);
        let mut st = ParamStore::default();
        let none = BTreeMap::new();
        for (pi, d) in prog.params.iter().enumerate() {
            let pi = pi as u32;
            if d.per_layer {
                for l in layers_of_param(prog, pi) {
                    let t = eval_src(&binding.srcs[pi as usize], &r, Some(l), &none).map_err(|e| LowerError::weights(format!("param `{}` layer {l}: {e}", d.name)))?;
                    check_shape(&d.name, &t, &d.shape)?;
                    st.layered.insert((pi, l), t);
                }
            } else {
                let t = eval_src(&binding.srcs[pi as usize], &r, None, &none).map_err(|e| LowerError::weights(format!("param `{}`: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.global.insert(pi, t);
            }
        }
        Ok((st, r.untouched()))
    }

    /// Seeded synthetic weights from each param's `Init` hint (tests only).
    pub fn synthetic(prog: &HlProgram, seed: u64) -> ParamStore {
        let mut st = ParamStore::default();
        for (pi, d) in prog.params.iter().enumerate() {
            let pi = pi as u32;
            let mk = |salt: u64| -> Tensor {
                let mut rng = ChaCha8Rng::seed_from_u64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (pi as u64) << 32);
                let n: usize = d.shape.iter().product();
                let data = (0..n).map(|_| sample(&mut rng, d.init)).collect();
                Tensor::new(d.shape.clone(), data)
            };
            if d.per_layer {
                for l in layers_of_param(prog, pi) {
                    st.layered.insert((pi, l), mk(l as u64 + 1));
                }
            } else {
                st.global.insert(pi, mk(0));
            }
        }
        st
    }
}

fn check_shape(name: &str, t: &Tensor, want: &[usize]) -> Result<()> {
    if t.shape != want {
        return Err(LowerError::weights(format!("param `{name}`: checkpoint gives {:?}, graph needs {want:?}", t.shape)));
    }
    Ok(())
}

fn unit(rng: &mut ChaCha8Rng) -> f64 {
    ((rng.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
}

fn sample(rng: &mut ChaCha8Rng, init: Init) -> f32 {
    match init {
        Init::Normal(s) => {
            let (u1, u2) = (unit(rng), unit(rng));
            ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos() * s as f64) as f32
        }
        Init::Ones => 1.0,
        Init::Zeros => 0.0,
        Init::Uniform(a, b) => (a as f64 + (b - a) as f64 * unit(rng)) as f32,
    }
}

/// Activation statistics of one site over a run (what calibration reads).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SiteStat {
    pub absmax: f64,
    pub sum_sq: f64,
    pub count: u64,
}

impl SiteStat {
    fn observe(&mut self, v: &[f32]) {
        for x in v {
            let a = (*x as f64).abs();
            if a > self.absmax {
                self.absmax = a;
            }
            self.sum_sq += a * a;
        }
        self.count += v.len() as u64;
    }
    pub fn rms(&self) -> f64 {
        if self.count == 0 { 0.0 } else { (self.sum_sq / self.count as f64).sqrt() }
    }
}

/// One sequence being run position by position.
pub struct Session<'a> {
    prog: &'a HlProgram,
    params: &'a ParamStore,
    fixed: BTreeMap<(u32, usize), Vec<f32>>,
    hist: BTreeMap<(u32, usize), Vec<Vec<f32>>>,
    pos: usize,
    pub sites: Option<BTreeMap<String, SiteStat>>,
}

impl<'a> Session<'a> {
    pub fn new(prog: &'a HlProgram, params: &'a ParamStore) -> Self {
        Session { prog, params, fixed: BTreeMap::new(), hist: BTreeMap::new(), pos: 0, sites: None }
    }
    pub fn with_site_stats(mut self) -> Self {
        self.sites = Some(BTreeMap::new());
        self
    }
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Logits for `token` at the current position; advances the position.
    pub fn step(&mut self, token: usize) -> Result<Vec<f32>> {
        if token >= self.prog.vocab {
            return Err(LowerError::eval(format!("token {token} ≥ vocab {}", self.prog.vocab)));
        }
        let pre = self.prog.pre;
        let mut carries = self.eval_block(pre, None, &[], token)?;
        for (l, k) in self.prog.schedule.clone().into_iter().enumerate() {
            carries = self.eval_block(k as usize, Some(l), &carries, token)?;
        }
        let post = self.prog.post;
        let out = self.eval_block(post, None, &carries, token)?;
        self.pos += 1;
        out.into_iter().next().ok_or_else(|| LowerError::eval("post block has no output"))
    }

    pub fn run(&mut self, tokens: &[usize]) -> Result<Vec<Vec<f32>>> {
        tokens.iter().map(|t| self.step(*t)).collect()
    }

    fn fixed_mut(&mut self, s: u32, layer: usize) -> &mut Vec<f32> {
        let d = &self.prog.states[s as usize];
        let n: usize = d.shape.iter().product();
        let init = d.init;
        self.fixed.entry((s, layer)).or_insert_with(|| vec![init; n])
    }

    fn eval_block(&mut self, bi: usize, layer: Option<usize>, carries: &[Vec<f32>], token: usize) -> Result<Vec<Vec<f32>>> {
        let prog = self.prog;
        let block = &prog.blocks[bi];
        let prefix = match (block.role, layer) {
            (BlockRole::Pre, _) => "pre.".to_string(),
            (BlockRole::Post, _) => "post.".to_string(),
            (_, Some(l)) => format!("L{l}."),
            _ => String::new(),
        };
        let mut vals: Vec<Vec<Vec<f32>>> = Vec::with_capacity(block.nodes.len());
        for node in &block.nodes {
            let out = self.eval_node(node, &vals, carries, layer, token, &prefix)?;
            if let (Some(site), Some(stats)) = (&node.site, self.sites.as_mut()) {
                stats.entry(format!("{prefix}{site}")).or_default().observe(&out[0]);
            }
            vals.push(out);
        }
        block.outputs.iter().map(|r| operand(*r, &vals, carries).map(<[f32]>::to_vec)).collect()
    }

    fn sub_site(&mut self, prefix: &str, site: &Option<String>, sub: &str, v: &[f32]) {
        if let (Some(s), Some(stats)) = (site, self.sites.as_mut()) {
            stats.entry(format!("{prefix}{s}.{sub}")).or_default().observe(v);
        }
    }

    fn param(&self, r: Ref, layer: Option<usize>) -> Result<&'a Tensor> {
        match r {
            Ref::Param(p) => self.params.get(p, layer),
            other => Err(LowerError::eval(format!("expected a param, got {other:?}"))),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn eval_node(&mut self, node: &Node, vals: &[Vec<Vec<f32>>], carries: &[Vec<f32>], layer: Option<usize>, token: usize, prefix: &str) -> Result<Vec<Vec<f32>>> {
        let ins = &node.inputs;
        let x = |i: usize| -> Result<&[f32]> { operand(ins[i], vals, carries) };
        let lyr = layer.unwrap_or(0);
        let pos = self.pos;
        let one = |v: Vec<f32>| Ok(vec![v]);
        match &node.op {
            Op::Embedding => {
                let t = self.param(ins[1], layer)?;
                let d = t.shape[1];
                one(t.data[token * d..(token + 1) * d].to_vec())
            }
            Op::PosEmbedding { offset } => {
                let t = self.param(ins[1], layer)?;
                let (rows, d) = (t.shape[0], t.shape[1]);
                let r = pos + offset;
                if r >= rows {
                    return Err(LowerError::eval(format!("position {pos} beyond the learned table ({rows} rows)")));
                }
                one(t.data[r * d..(r + 1) * d].to_vec())
            }
            Op::Slice { start, len } => one(x(0)?[*start..start + len].to_vec()),
            Op::Concat => {
                let mut v = Vec::new();
                for i in 0..ins.len() {
                    v.extend_from_slice(x(i)?);
                }
                one(v)
            }
            Op::Zeros => one(vec![0.0; node.outs[0].iter().product()]),
            Op::Linear { bias } => {
                let w = self.param(ins[1], layer)?;
                let b = if *bias { Some(self.param(ins[2], layer)?) } else { None };
                one(linear(x(0)?, w, b.map(|t| t.data.as_slice())))
            }
            Op::Add | Op::Sub | Op::Mul => {
                let a = self.operand_any(ins[0], vals, carries, layer)?;
                let b = self.operand_any(ins[1], vals, carries, layer)?;
                let n = a.len().max(b.len());
                let at = |v: &[f32], i: usize| if v.len() == 1 { v[0] } else { v[i] };
                if !(a.len() == b.len() || a.len() == 1 || b.len() == 1) {
                    return Err(LowerError::eval(format!("elementwise lengths {} and {}", a.len(), b.len())));
                }
                one((0..n)
                    .map(|i| match node.op {
                        Op::Add => at(&a, i) + at(&b, i),
                        Op::Sub => at(&a, i) - at(&b, i),
                        _ => at(&a, i) * at(&b, i),
                    })
                    .collect())
            }
            Op::Scale { c } => one(x(0)?.iter().map(|v| (*v as f64 * c) as f32).collect()),
            Op::Act(a) => one(x(0)?.iter().map(|v| act(*a, *v)).collect()),
            Op::Clamp { lo, hi } => one(x(0)?.iter().map(|v| (*v as f64).clamp(*lo, *hi) as f32).collect()),
            Op::Softcap { cap } => one(x(0)?.iter().map(|v| ((*v as f64 / cap).tanh() * cap) as f32).collect()),
            Op::Lerp => {
                let (a, b) = (x(0)?, x(1)?);
                let t = self.operand_any(ins[2], vals, carries, layer)?;
                one((0..a.len()).map(|i| a[i] + (b[i] - a[i]) * t[i]).collect())
            }
            Op::DecayExpNegExp => one(x(0)?.iter().map(|v| (-(*v as f64).exp()).exp() as f32).collect()),
            Op::ClampedSwiGlu { alpha, limit } => {
                let (g, u) = (x(0)?, x(1)?);
                one(g.iter().zip(u).map(|(g, u)| clamped_swiglu(*g, *u, *alpha, *limit)).collect())
            }
            Op::Norm { spec, groups } => {
                let gain = if ins.len() > 1 { Some(self.param(ins[1], layer)?) } else { None };
                let bias = if ins.len() > 2 { Some(self.param(ins[2], layer)?) } else { None };
                one(norm(x(0)?, spec.kind, spec.eps, spec.gain, gain, bias, *groups))
            }
            Op::GatedRmsNorm { eps, groups, gate_first } => {
                let w = self.param(ins[2], layer)?;
                one(gated_rms_norm(x(0)?, x(1)?, &w.data, *eps, *groups, *gate_first))
            }
            Op::L2Norm { groups, eps } => {
                let v = x(0)?;
                let g = v.len() / groups;
                let mut out = Vec::with_capacity(v.len());
                for c in v.chunks(g) {
                    let ss: f64 = c.iter().map(|a| (*a as f64) * (*a as f64)).sum();
                    let inv = 1.0 / (ss + eps).sqrt();
                    out.extend(c.iter().map(|a| (*a as f64 * inv) as f32));
                }
                one(out)
            }
            Op::Rope { heads, head_dim, rotary_dim, offset, style, table } => {
                let f = &self.prog.rope_tables[*table as usize];
                let (c, s) = f.cos_sin(pos);
                one(rope(x(0)?, *heads, *head_dim, *rotary_dim, *offset, *style, &c, &s))
            }
            Op::HistAppend => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("HistAppend without a state")) };
                let row = x(0)?.to_vec();
                self.hist.entry((s, lyr)).or_default().push(row);
                Ok(vec![vec![]])
            }
            Op::Attention { heads, kv_heads, head_dim, v_head_dim, scale, softcap, window, alibi, sinks } => {
                let (Ref::State(ks), Ref::State(vs)) = (ins[1], ins[2]) else { return Err(LowerError::eval("attention without states")) };
                let q = x(0)?.to_vec();
                let sinks_v = if *sinks { Some(self.param(ins[3], layer)?.data.clone()) } else { None };
                let keys = self.hist.get(&(ks, lyr)).cloned().unwrap_or_default();
                let values = self.hist.get(&(vs, lyr)).cloned().unwrap_or_default();
                let n = keys.len();
                let start = window.map(|w| n.saturating_sub(w)).unwrap_or(0);
                let group = heads / kv_heads;
                let mut out = vec![0f32; heads * v_head_dim];
                let mut all_scores = Vec::new();
                let mut all_probs = Vec::new();
                for h in 0..*heads {
                    let kvh = h / group;
                    let qh = &q[h * head_dim..(h + 1) * head_dim];
                    let mut sc: Vec<f64> = (start..n)
                        .map(|j| {
                            let kj = &keys[j][kvh * head_dim..(kvh + 1) * head_dim];
                            let dot: f64 = qh.iter().zip(kj).map(|(a, b)| *a as f64 * *b as f64).sum();
                            match alibi {
                                Some(al) => {
                                    let slope = al.slopes[h];
                                    let bias = if al.bf16_bias { bf16_round(bf16_round(slope as f32) * j as f32) as f64 } else { slope * j as f64 };
                                    if al.scaled_by_softmax_scale { (dot + bias) * scale } else { dot * scale + bias }
                                }
                                None => dot * scale,
                            }
                        })
                        .collect();
                    if let Some(c) = softcap {
                        sc.iter_mut().for_each(|s| *s = (*s / c).tanh() * c);
                    }
                    let sink = sinks_v.as_ref().map(|s| s[h] as f64);
                    let p = softmax_with_sink(&sc, sink);
                    if self.sites.is_some() {
                        all_scores.extend(sc.iter().map(|v| *v as f32));
                        all_probs.extend(p.iter().map(|v| *v as f32));
                    }
                    for (jj, j) in (start..n).enumerate() {
                        let vj = &values[j][kvh * v_head_dim..(kvh + 1) * v_head_dim];
                        for (o, vv) in out[h * v_head_dim..(h + 1) * v_head_dim].iter_mut().zip(vj) {
                            *o += (p[jj] * *vv as f64) as f32;
                        }
                    }
                }
                self.sub_site(prefix, &node.site, "scores", &all_scores);
                self.sub_site(prefix, &node.site, "probs", &all_probs);
                one(out)
            }
            Op::MlaAttention { heads, nope, rope, v_dim, kv_lora, scale } => {
                let (Ref::State(ls), Ref::State(rs)) = (ins[1], ins[2]) else { return Err(LowerError::eval("MLA without states")) };
                let q = x(0)?.to_vec();
                let kvb = self.param(ins[3], layer)?;
                let lat = self.hist.get(&(ls, lyr)).cloned().unwrap_or_default();
                let kr = self.hist.get(&(rs, lyr)).cloned().unwrap_or_default();
                one(mla(&q, &kvb.data, &lat, &kr, *heads, *nope, *rope, *v_dim, *kv_lora, *scale))
            }
            Op::CausalConv1d { channels, kernel, bias, act: a } => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("conv without state")) };
                let w = self.param(ins[2], layer)?.data.clone();
                let b = if *bias { Some(self.param(ins[3], layer)?.data.clone()) } else { None };
                let xv = x(0)?.to_vec();
                let st = self.fixed_mut(s, lyr);
                let out = causal_conv(&xv, st, &w, b.as_deref(), *channels, *kernel, *a);
                one(out)
            }
            Op::GatedDelta { k_heads, v_heads, dk, dv, head_map, q_scale } => {
                let Ref::State(s) = ins[5] else { return Err(LowerError::eval("GDN without state")) };
                let (q, k, v, g, beta) = (x(0)?.to_vec(), x(1)?.to_vec(), x(2)?.to_vec(), x(3)?.to_vec(), x(4)?.to_vec());
                let st = self.fixed_mut(s, lyr);
                let out = gated_delta(&q, &k, &v, &g, &beta, st, *k_heads, *v_heads, *dk, *dv, *head_map, *q_scale);
                let snap = if self.sites.is_some() { self.fixed[&(s, lyr)].clone() } else { vec![] };
                self.sub_site(prefix, &node.site, "state", &snap);
                one(out)
            }
            Op::SelectiveScan { inner, state } => {
                let Ref::State(s) = ins[6] else { return Err(LowerError::eval("scan without state")) };
                let (xv, dt, bb, cc) = (x(0)?.to_vec(), x(1)?.to_vec(), x(2)?.to_vec(), x(3)?.to_vec());
                let a = self.param(ins[4], layer)?.data.clone();
                let d = self.param(ins[5], layer)?.data.clone();
                let h = self.fixed_mut(s, lyr);
                let out = selective_scan(&xv, &dt, &bb, &cc, &a, &d, h, *inner, *state);
                let snap = if self.sites.is_some() { self.fixed[&(s, lyr)].clone() } else { vec![] };
                self.sub_site(prefix, &node.site, "state", &snap);
                one(out)
            }
            Op::Ssd { heads, head_dim, groups, state } => {
                let Ref::State(s) = ins[6] else { return Err(LowerError::eval("ssd without state")) };
                let (xv, dt, bb, cc) = (x(0)?.to_vec(), x(1)?.to_vec(), x(2)?.to_vec(), x(3)?.to_vec());
                let a = self.param(ins[4], layer)?.data.clone();
                let d = self.param(ins[5], layer)?.data.clone();
                let h = self.fixed_mut(s, lyr);
                let out = ssd_step(&xv, &dt, &bb, &cc, &a, &d, h, *heads, *head_dim, *groups, *state);
                let snap = if self.sites.is_some() { self.fixed[&(s, lyr)].clone() } else { vec![] };
                self.sub_site(prefix, &node.site, "state", &snap);
                one(out)
            }
            Op::TokenShift => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("token shift without state")) };
                let xv = x(0)?.to_vec();
                let st = self.fixed_mut(s, lyr);
                let prev = std::mem::replace(st, xv);
                one(prev)
            }
            Op::Wkv4 => {
                let states: Vec<u32> = ins[4..7].iter().map(|r| if let Ref::State(s) = r { *s } else { u32::MAX }).collect();
                let (k, v) = (x(0)?.to_vec(), x(1)?.to_vec());
                let w = self.param(ins[2], layer)?.data.clone();
                let u = self.param(ins[3], layer)?.data.clone();
                let mut num = self.fixed_mut(states[0], lyr).clone();
                let mut den = self.fixed_mut(states[1], lyr).clone();
                let mut mx = self.fixed_mut(states[2], lyr).clone();
                let out = wkv4(&k, &v, &w, &u, &mut num, &mut den, &mut mx);
                *self.fixed_mut(states[0], lyr) = num;
                *self.fixed_mut(states[1], lyr) = den;
                *self.fixed_mut(states[2], lyr) = mx;
                one(out)
            }
            Op::Wkv6 { heads, head_size } => {
                let Ref::State(s) = ins[5] else { return Err(LowerError::eval("wkv6 without state")) };
                let (r, k, v) = (x(0)?.to_vec(), x(1)?.to_vec(), x(2)?.to_vec());
                let w = self.operand_any(ins[3], vals, carries, layer)?;
                let u = self.operand_any(ins[4], vals, carries, layer)?;
                let st = self.fixed_mut(s, lyr);
                one(wkv6(&r, &k, &v, &w, &u, st, *heads, *head_size))
            }
            Op::Wkv7 { heads, head_size } => {
                let Ref::State(s) = ins[6] else { return Err(LowerError::eval("wkv7 without state")) };
                let vs: Vec<Vec<f32>> = (0..6).map(|i| x(i).map(<[f32]>::to_vec)).collect::<Result<_>>()?;
                let st = self.fixed_mut(s, lyr);
                one(wkv7(&vs[0], &vs[1], &vs[2], &vs[3], &vs[4], &vs[5], st, *heads, *head_size))
            }
            Op::Route { router, experts, top_k } => {
                let logits = x(0)?.to_vec();
                let sel_bias = if ins.len() > 1 { Some(self.param(ins[1], layer)?.data.clone()) } else { None };
                let (idx, w) = route(&logits, sel_bias.as_deref(), router, *experts, *top_k);
                self.sub_site(prefix, &node.site, "logits", &logits);
                Ok(vec![idx.iter().map(|i| *i as f32).collect(), w])
            }
            Op::MoeExperts { top_k, act: a, glu, bias } => {
                let xv = x(0)?.to_vec();
                let idx: Vec<usize> = x(1)?.iter().map(|v| *v as usize).collect();
                let w = x(2)?.to_vec();
                let g = self.param(ins[3], layer)?;
                let u = self.param(ins[4], layer)?;
                let d = self.param(ins[5], layer)?;
                let biases = if *bias { Some((self.param(ins[6], layer)?, self.param(ins[7], layer)?, self.param(ins[8], layer)?)) } else { None };
                let (e_i, e_d) = (g.shape[1], g.shape[2]);
                let mut y = vec![0f64; e_d];
                let mut hidden_all = Vec::new();
                for (j, &e) in idx.iter().enumerate().take(*top_k) {
                    let slice = |t: &Tensor, e: usize| -> Vec<f32> {
                        let per: usize = t.shape[1..].iter().product();
                        t.data[e * per..(e + 1) * per].to_vec()
                    };
                    let (gw, uw, dw) = (slice(g, e), slice(u, e), slice(d, e));
                    let gb = biases.map(|(a, _, _)| slice(a, e));
                    let ub = biases.map(|(_, b, _)| slice(b, e));
                    let db = biases.map(|(_, _, c)| slice(c, e));
                    let gv = linear_raw(&xv, &gw, e_i, gb.as_deref());
                    let uv = linear_raw(&xv, &uw, e_i, ub.as_deref());
                    let hv: Vec<f32> = match glu {
                        Glu::Standard => gv.iter().zip(&uv).map(|(g, u)| act(*a, *g) * *u).collect(),
                        Glu::ClampedSwiGlu { alpha, limit } => gv.iter().zip(&uv).map(|(g, u)| clamped_swiglu(*g, *u, *alpha, *limit)).collect(),
                    };
                    if self.sites.is_some() {
                        hidden_all.extend_from_slice(&hv);
                    }
                    let ov = linear_raw(&hv, &dw, e_d, db.as_deref());
                    for (yy, o) in y.iter_mut().zip(&ov) {
                        *yy += w[j] as f64 * *o as f64;
                    }
                }
                self.sub_site(prefix, &node.site, "hidden", &hidden_all);
                one(y.into_iter().map(|v| v as f32).collect())
            }
        }
    }

    /// An operand that may also be a param (elementwise ops read biases/gates from params).
    fn operand_any(&self, r: Ref, vals: &[Vec<Vec<f32>>], carries: &[Vec<f32>], layer: Option<usize>) -> Result<Vec<f32>> {
        match r {
            Ref::Param(_) => Ok(self.param(r, layer)?.data.clone()),
            _ => operand(r, vals, carries).map(<[f32]>::to_vec),
        }
    }
}

fn operand<'v>(r: Ref, vals: &'v [Vec<Vec<f32>>], carries: &'v [Vec<f32>]) -> Result<&'v [f32]> {
    match r {
        Ref::Node(i, o) => vals.get(i as usize).and_then(|v| v.get(o as usize)).map(Vec::as_slice).ok_or_else(|| LowerError::eval(format!("node {i}.{o} not evaluated"))),
        Ref::Carry(c) => carries.get(c as usize).map(Vec::as_slice).ok_or_else(|| LowerError::eval(format!("carry {c}"))),
        other => Err(LowerError::eval(format!("operand {other:?} is not a value"))),
    }
}

// ───────────────────────────── kernels ─────────────────────────────

pub fn linear_raw(x: &[f32], w: &[f32], out: usize, b: Option<&[f32]>) -> Vec<f32> {
    let n = x.len();
    (0..out)
        .map(|o| {
            let row = &w[o * n..(o + 1) * n];
            let s: f64 = row.iter().zip(x).map(|(a, b)| *a as f64 * *b as f64).sum();
            (s + b.map(|b| b[o] as f64).unwrap_or(0.0)) as f32
        })
        .collect()
}

fn linear(x: &[f32], w: &Tensor, b: Option<&[f32]>) -> Vec<f32> {
    linear_raw(x, &w.data, w.shape[0], b)
}

/// `erf` to ~1e-13 absolute (series below 3, Chebyshev `erfc` above).
pub fn erf(x: f64) -> f64 {
    let a = x.abs();
    let r = if a < 3.0 {
        let (mut term, mut sum, x2) = (a, a, a * a);
        let mut n = 0f64;
        loop {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() < 1e-17 * sum.abs() {
                break;
            }
            if n > 200.0 {
                break;
            }
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else {
        let t = 1.0 / (1.0 + 0.5 * a);
        let e = t * (-a * a - 1.265_512_23
            + t * (1.000_023_68
                + t * (0.374_091_96 + t * (0.096_784_18 + t * (-0.186_288_06 + t * (0.278_868_07 + t * (-1.135_203_98 + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
            .exp();
        1.0 - e
    };
    if x < 0.0 { -r } else { r }
}

pub fn act(a: Act, v: f32) -> f32 {
    let x = v as f64;
    let sig = |z: f64| 1.0 / (1.0 + (-z).exp());
    (match a {
        Act::Silu => x * sig(x),
        Act::Gelu => 0.5 * x * (1.0 + erf(x / std::f64::consts::SQRT_2)),
        Act::GeluTanh => 0.5 * x * (1.0 + ((2.0 / std::f64::consts::PI).sqrt() * (x + 0.044715 * x * x * x)).tanh()),
        Act::QuickGelu => x * sig(1.702 * x),
        Act::Relu => x.max(0.0),
        Act::Relu2 => {
            let r = x.max(0.0);
            r * r
        }
        Act::Sigmoid => sig(x),
        Act::Tanh => x.tanh(),
        // torch.nn.functional.softplus(beta=1, threshold=20)
        Act::Softplus => {
            if x > 20.0 {
                x
            } else {
                x.exp().ln_1p()
            }
        }
        Act::Identity => x,
    }) as f32
}

fn clamped_swiglu(g: f32, u: f32, alpha: f64, limit: f64) -> f32 {
    let g = (g as f64).min(limit);
    let u = (u as f64).clamp(-limit, limit);
    let glu = g * (1.0 / (1.0 + (-(g * alpha)).exp()));
    ((u + 1.0) * glu) as f32
}

pub fn norm(x: &[f32], kind: NormKind, eps: f64, gain_kind: crate::spec::Gain, gain: Option<&Tensor>, bias: Option<&Tensor>, groups: usize) -> Vec<f32> {
    let n = x.len();
    let g = n / groups.max(1);
    let mut out = Vec::with_capacity(n);
    for (gi, c) in x.chunks(g).enumerate() {
        let (mean, var) = match kind {
            NormKind::Rms => (0.0, c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / g as f64),
            NormKind::Layer => {
                let m = c.iter().map(|v| *v as f64).sum::<f64>() / g as f64;
                (m, c.iter().map(|v| (*v as f64 - m) * (*v as f64 - m)).sum::<f64>() / g as f64)
            }
        };
        let inv = 1.0 / (var + eps).sqrt();
        for (i, v) in c.iter().enumerate() {
            let mut y = ((*v as f64 - mean) * inv) as f32;
            // Gain/bias shapes: [n] (full), [g] (shared by groups) or [groups, g] (per group).
            let idx = |t: &Tensor| if t.data.len() == n { gi * g + i } else { i };
            if let Some(w) = gain {
                let wv = w.data[idx(w)];
                y = match gain_kind {
                    crate::spec::Gain::OnePlusW => y * (1.0 + wv),
                    _ => y * wv,
                };
            }
            if let Some(b) = bias {
                y += b.data[idx(b)];
            }
            out.push(y);
        }
    }
    out
}

pub fn gated_rms_norm(x: &[f32], z: &[f32], w: &[f32], eps: f64, groups: usize, gate_first: bool) -> Vec<f32> {
    let n = x.len();
    let silu = |v: f32| act(Act::Silu, v);
    let pre: Vec<f32> = if gate_first { x.iter().zip(z).map(|(a, b)| a * silu(*b)).collect() } else { x.to_vec() };
    let g = n / groups.max(1);
    let mut out = Vec::with_capacity(n);
    for (gi, c) in pre.chunks(g).enumerate() {
        let var = c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / g as f64;
        let inv = 1.0 / (var + eps).sqrt();
        for (i, v) in c.iter().enumerate() {
            let k = gi * g + i;
            let wv = if w.len() == n { w[k] } else { w[i] };
            let mut y = ((*v as f64) * inv) as f32 * wv;
            if !gate_first {
                y *= silu(z[k]);
            }
            out.push(y);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn rope(x: &[f32], heads: usize, head_dim: usize, rd: usize, off: usize, style: RopeStyle, cos: &[f32], sin: &[f32]) -> Vec<f32> {
    let mut out = x.to_vec();
    let half = rd / 2;
    for h in 0..heads {
        let base = h * head_dim + off;
        for i in 0..half {
            let (a, b) = match style {
                RopeStyle::Half => (base + i, base + i + half),
                RopeStyle::Interleaved => (base + 2 * i, base + 2 * i + 1),
            };
            let (x1, x2) = (x[a], x[b]);
            out[a] = x1 * cos[i] - x2 * sin[i];
            out[b] = x2 * cos[i] + x1 * sin[i];
        }
    }
    out
}

fn softmax_with_sink(s: &[f64], sink: Option<f64>) -> Vec<f64> {
    let mut m = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if let Some(k) = sink {
        m = m.max(k);
    }
    let e: Vec<f64> = s.iter().map(|v| (v - m).exp()).collect();
    let z: f64 = e.iter().sum::<f64>() + sink.map(|k| (k - m).exp()).unwrap_or(0.0);
    e.into_iter().map(|v| v / z).collect()
}

/// MLA in absorbed form: `q̃ = W_kᵀ q_nope`, scores over the latent history, context mapped back
/// through `W_v`. Equal to HF's expanded form (`kv_b · latent` per head), tested below.
#[allow(clippy::too_many_arguments)]
pub fn mla(q: &[f32], kvb: &[f32], lat: &[Vec<f32>], kr: &[Vec<f32>], heads: usize, nope: usize, rope: usize, vd: usize, r: usize, scale: f64) -> Vec<f32> {
    let qd = nope + rope;
    let mut out = vec![0f32; heads * vd];
    for h in 0..heads {
        let rows = &kvb[h * (nope + vd) * r..(h + 1) * (nope + vd) * r];
        let (wk, wv) = rows.split_at(nope * r);
        let qn = &q[h * qd..h * qd + nope];
        let qr = &q[h * qd + nope..(h + 1) * qd];
        let qt: Vec<f64> = (0..r).map(|c| (0..nope).map(|i| wk[i * r + c] as f64 * qn[i] as f64).sum()).collect();
        let sc: Vec<f64> = (0..lat.len())
            .map(|j| {
                let a: f64 = qt.iter().zip(&lat[j]).map(|(x, y)| x * *y as f64).sum();
                let b: f64 = qr.iter().zip(&kr[j]).map(|(x, y)| *x as f64 * *y as f64).sum();
                (a + b) * scale
            })
            .collect();
        let p = softmax_with_sink(&sc, None);
        let ctx: Vec<f64> = (0..r).map(|c| (0..lat.len()).map(|j| p[j] * lat[j][c] as f64).sum()).collect();
        for i in 0..vd {
            out[h * vd + i] = (0..r).map(|c| wv[i * r + c] as f64 * ctx[c]).sum::<f64>() as f32;
        }
    }
    out
}

fn causal_conv(x: &[f32], st: &mut Vec<f32>, w: &[f32], b: Option<&[f32]>, ch: usize, k: usize, a: Option<Act>) -> Vec<f32> {
    // state: (k−1) rows of `ch`, oldest first.
    let mut out = vec![0f32; ch];
    for c in 0..ch {
        let mut s = 0f64;
        for t in 0..k {
            let v = if t + 1 == k { x[c] } else { st[t * ch + c] };
            s += w[c * k + t] as f64 * v as f64;
        }
        if let Some(b) = b {
            s += b[c] as f64;
        }
        out[c] = s as f32;
    }
    if k > 1 {
        st.drain(..ch);
        st.extend_from_slice(x);
    }
    if let Some(a) = a {
        out.iter_mut().for_each(|v| *v = act(a, *v));
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn gated_delta(q: &[f32], k: &[f32], v: &[f32], g: &[f32], beta: &[f32], s: &mut [f32], nk: usize, nv: usize, dk: usize, dv: usize, map: HeadMap, q_scale: f64) -> Vec<f32> {
    let rep = nv / nk;
    let mut out = vec![0f32; nv * dv];
    for vh in 0..nv {
        let kh = match map {
            HeadMap::Group => vh / rep,
            HeadMap::Tile => vh % nk,
        };
        let st = &mut s[vh * dk * dv..(vh + 1) * dk * dv];
        let decay = (g[vh] as f64).exp();
        st.iter_mut().for_each(|x| *x = (*x as f64 * decay) as f32);
        let kv = &k[kh * dk..(kh + 1) * dk];
        let qv = &q[kh * dk..(kh + 1) * dk];
        let vv = &v[vh * dv..(vh + 1) * dv];
        let mut delta = vec![0f64; dv];
        for (j, dj) in delta.iter_mut().enumerate() {
            let mem: f64 = (0..dk).map(|i| st[i * dv + j] as f64 * kv[i] as f64).sum();
            *dj = (vv[j] as f64 - mem) * beta[vh] as f64;
        }
        for i in 0..dk {
            for j in 0..dv {
                st[i * dv + j] = (st[i * dv + j] as f64 + kv[i] as f64 * delta[j]) as f32;
            }
        }
        for j in 0..dv {
            out[vh * dv + j] = ((0..dk).map(|i| st[i * dv + j] as f64 * qv[i] as f64).sum::<f64>() * q_scale) as f32;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn selective_scan(x: &[f32], dt: &[f32], b: &[f32], c: &[f32], a: &[f32], d: &[f32], h: &mut [f32], inner: usize, n: usize) -> Vec<f32> {
    let mut y = vec![0f32; inner];
    for i in 0..inner {
        let mut acc = 0f64;
        for s in 0..n {
            let k = i * n + s;
            let da = (dt[i] as f64 * a[k] as f64).exp();
            let nh = da * h[k] as f64 + dt[i] as f64 * b[s] as f64 * x[i] as f64;
            h[k] = nh as f32;
            acc += h[k] as f64 * c[s] as f64;
        }
        y[i] = (acc + x[i] as f64 * d[i] as f64) as f32;
    }
    y
}

#[allow(clippy::too_many_arguments)]
pub fn ssd_step(x: &[f32], dt: &[f32], b: &[f32], c: &[f32], a: &[f32], d: &[f32], h: &mut [f32], heads: usize, p: usize, groups: usize, n: usize) -> Vec<f32> {
    let per = heads / groups;
    let mut y = vec![0f32; heads * p];
    for hh in 0..heads {
        let g = hh / per;
        let da = (dt[hh] as f64 * a[hh] as f64).exp();
        for pp in 0..p {
            let xi = x[hh * p + pp] as f64;
            let mut acc = 0f64;
            for s in 0..n {
                let k = (hh * p + pp) * n + s;
                let nh = h[k] as f64 * da + dt[hh] as f64 * b[g * n + s] as f64 * xi;
                h[k] = nh as f32;
                acc += h[k] as f64 * c[g * n + s] as f64;
            }
            y[hh * p + pp] = (acc + xi * d[hh] as f64) as f32;
        }
    }
    y
}

fn wkv4(k: &[f32], v: &[f32], w: &[f32], u: &[f32], num: &mut [f32], den: &mut [f32], mx: &mut [f32]) -> Vec<f32> {
    let mut out = vec![0f32; k.len()];
    for c in 0..k.len() {
        let (kc, vc) = (k[c] as f64, v[c] as f64);
        let (n0, d0, m0) = (num[c] as f64, den[c] as f64, mx[c] as f64);
        let ww = u[c] as f64 + kc;
        let p = m0.max(ww);
        let (e1, e2) = ((m0 - p).exp(), (ww - p).exp());
        out[c] = ((e1 * n0 + e2 * vc) / (e1 * d0 + e2)) as f32;
        let ww2 = m0 + w[c] as f64;
        let p2 = ww2.max(kc);
        let (e1, e2) = ((ww2 - p2).exp(), (kc - p2).exp());
        num[c] = (e1 * n0 + e2 * vc) as f32;
        den[c] = (e1 * d0 + e2) as f32;
        mx[c] = p2 as f32;
    }
    out
}

/// RWKV-5/6: `o_j = Σ_i r_i (u_i k_i v_j + S_ij)`, `S_ij ← k_i v_j + w_i S_ij` per head.
#[allow(clippy::too_many_arguments)]
pub fn wkv6(r: &[f32], k: &[f32], v: &[f32], w: &[f32], u: &[f32], s: &mut [f32], heads: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0f32; heads * n];
    for h in 0..heads {
        let st = &mut s[h * n * n..(h + 1) * n * n];
        for j in 0..n {
            let mut acc = 0f64;
            for i in 0..n {
                let kv = k[h * n + i] as f64 * v[h * n + j] as f64;
                acc += r[h * n + i] as f64 * (u[h * n + i] as f64 * kv + st[i * n + j] as f64);
            }
            out[h * n + j] = acc as f32;
        }
        for i in 0..n {
            for j in 0..n {
                let kv = k[h * n + i] as f64 * v[h * n + j] as f64;
                st[i * n + j] = (kv + w[h * n + i] as f64 * st[i * n + j] as f64) as f32;
            }
        }
    }
    out
}

/// RWKV-7: state `S[v][k]`; `S ← S·diag(w) + (S·a) bᵀ + v kᵀ`, `o = S·r` per head.
#[allow(clippy::too_many_arguments)]
pub fn wkv7(r: &[f32], w: &[f32], k: &[f32], v: &[f32], a: &[f32], b: &[f32], s: &mut [f32], heads: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0f32; heads * n];
    for h in 0..heads {
        let st = &mut s[h * n * n..(h + 1) * n * n];
        let o = h * n;
        let sa: Vec<f64> = (0..n).map(|vi| (0..n).map(|ki| st[vi * n + ki] as f64 * a[o + ki] as f64).sum()).collect();
        for vi in 0..n {
            for ki in 0..n {
                let x = st[vi * n + ki] as f64 * w[o + ki] as f64 + sa[vi] * b[o + ki] as f64 + v[o + vi] as f64 * k[o + ki] as f64;
                st[vi * n + ki] = x as f32;
            }
        }
        for vi in 0..n {
            out[o + vi] = (0..n).map(|ki| st[vi * n + ki] as f64 * r[o + ki] as f64).sum::<f64>() as f32;
        }
    }
    out
}

/// The `k` largest entries, ties to the lowest index, returned in index order.
pub fn top_k_indices(v: &[f64], k: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..v.len()).collect();
    // Stable sort by descending value: equal values keep ascending index order.
    order.sort_by(|a, b| v[*b].partial_cmp(&v[*a]).unwrap_or(std::cmp::Ordering::Equal));
    let mut sel: Vec<usize> = order.into_iter().take(k).collect();
    sel.sort_unstable();
    sel
}

/// Expert selection (see `crate::spec::RouterSpec`). Softmax routers mask non-selected groups
/// with 0 and weight by the masked scores (DeepSeek-V2); sigmoid routers mask with −∞ on the
/// bias-corrected choice scores and weight by the raw scores (DeepSeek-V3).
pub fn route(logits: &[f32], sel_bias: Option<&[f32]>, r: &RouterSpec, e: usize, k: usize) -> (Vec<usize>, Vec<f32>) {
    let l: Vec<f64> = logits.iter().map(|x| *x as f64).collect();
    let (scores, mut choice): (Vec<f64>, Vec<f64>) = match r.scoring {
        Scoring::Softmax => {
            let p = softmax_with_sink(&l, None);
            (p.clone(), p)
        }
        Scoring::Sigmoid => {
            let s: Vec<f64> = l.iter().map(|x| 1.0 / (1.0 + (-x).exp())).collect();
            let c = s.iter().enumerate().map(|(i, v)| v + sel_bias.map(|b| b[i] as f64).unwrap_or(0.0)).collect();
            (s, c)
        }
        Scoring::TopKThenSoftmax => (l.clone(), l.clone()),
    };
    if let Some(g) = &r.groups {
        let per = e / g.n_group;
        let gs: Vec<f64> = (0..g.n_group)
            .map(|gi| {
                let c = &choice[gi * per..(gi + 1) * per];
                match g.score {
                    GroupScore::Max => c.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    GroupScore::Top2Sum => {
                        let t = top_k_indices(c, 2.min(per));
                        t.iter().map(|i| c[*i]).sum()
                    }
                }
            })
            .collect();
        let keep = top_k_indices(&gs, g.topk_group);
        let fill = if r.scoring == Scoring::Softmax { 0.0 } else { f64::NEG_INFINITY };
        for (i, c) in choice.iter_mut().enumerate() {
            if !keep.contains(&(i / per)) {
                *c = fill;
            }
        }
    }
    let idx = top_k_indices(&choice, k);
    let mut w: Vec<f64> = match r.scoring {
        Scoring::TopKThenSoftmax => softmax_with_sink(&idx.iter().map(|i| l[*i]).collect::<Vec<_>>(), None),
        Scoring::Softmax => idx.iter().map(|i| choice[*i]).collect(),
        Scoring::Sigmoid => idx.iter().map(|i| scores[*i]).collect(),
    };
    if r.normalize {
        let s: f64 = w.iter().sum::<f64>() + r.norm_eps;
        w.iter_mut().for_each(|x| *x /= s);
    }
    w.iter_mut().for_each(|x| *x *= r.scale);
    (idx, w.into_iter().map(|x| x as f32).collect())
}

#[cfg(test)]
mod tests;
