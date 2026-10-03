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
use crate::prequant::QWeight;
use crate::rope::{RopeStyle, bf16_round};
use crate::spec::{Act, Glu, GroupScore, HeadMap, NormKind, RouterSpec, Scoring};
use crate::weights::{Binding, Resolver, Src, Tensor, TensorSource, eval_qsrc, eval_src, layers_of_param};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;
use std::ops::{Deref, Range};
use std::sync::{Arc, Mutex};

pub mod stream;

/// **A param instance read from its source when it is asked for** (RFC-0002 Part II §II.9 L1: the float reference's calibration, row-streamed).
///
/// A layer-major run holds one layer's weights resident; a model whose single layer does not fit a host's memory — a stack of 128 experts,
/// a 150k-row projection — needs the weights themselves read on demand. A lazy param is the source's expression behind two reads: the
/// whole tensor ([`load`](Self::load)) for an op that reads all of it, and a block of rows ([`rows`](Self::rows), every axis but the last
/// flattened) for an op that reads a few (the experts a router selected). The value is the one a resident param holds, element for element.
pub trait LazyParam: Send + Sync {
    fn shape(&self) -> &[usize];
    fn load(&self) -> Result<Tensor>;
    fn rows(&self, rows: Range<usize>) -> Result<Tensor>;
}

/// What a lazy param's cache may hold, and what it holds.
#[derive(Default)]
struct LazyCache {
    budget_bytes: usize,
    held_bytes: usize,
    held: BTreeMap<(u32, Option<usize>), Arc<Tensor>>,
}

/// **A param as an op reads it**: borrowed from the store, or read from its source and handed over (kept in the lazy cache while its
/// budget has room, dropped with the op otherwise).
pub enum PView<'a> {
    Ref(&'a Tensor),
    Owned(Arc<Tensor>),
}

impl Deref for PView<'_> {
    type Target = Tensor;
    fn deref(&self) -> &Tensor {
        match self {
            PView::Ref(t) => t,
            PView::Owned(t) => t,
        }
    }
}

/// Bound params: globals, and per-layer params by `(param, layer)`.
#[derive(Default, Clone)]
pub struct ParamStore {
    global: BTreeMap<u32, Tensor>,
    layered: BTreeMap<(u32, usize), Tensor>,
    /// A pre-quantised param's stored integers (its float tensor above is their dequantisation);
    /// one per expert for a stack of experts.
    quant: BTreeMap<(u32, Option<usize>), Arc<Vec<QWeight>>>,
    /// Params read from their source when an op asks (L1), by `(param, occurrence layer)`; a lazy instance has no tensor above.
    lazy: BTreeMap<(u32, Option<usize>), Arc<dyn LazyParam>>,
    /// The tensors the lazy params keep between asks, within a budget (shared by every clone and every session of the store).
    lazy_cache: Arc<Mutex<LazyCache>>,
}

/// One param instance from its source: the float tensor, and the stored integers when the param
/// is pre-quantised (the float tensor is then their dequantisation, read once).
pub(crate) fn bind_one(src: &Src, r: &Resolver, layer: Option<usize>) -> Result<(Tensor, Option<Vec<QWeight>>)> {
    let none = BTreeMap::new();
    if src.is_quant() {
        let q = eval_qsrc(src, r, layer, &none)?.ok_or_else(|| LowerError::eval("internal: a quantised source read no integers"))?;
        let t = if q.len() == 1 {
            q[0].dequant()
        } else {
            // A stack of experts: `[E, out, in]`.
            let mut data = Vec::new();
            for w in &q {
                data.extend(w.dequant().data);
            }
            Tensor::new(vec![q.len(), q[0].out, q[0].inp], data)
        };
        Ok((t, Some(q)))
    } else {
        Ok((eval_src(src, r, layer, &none)?, None))
    }
}

impl ParamStore {
    /// Bind a param instance LAZILY: the source is read when an op asks (the whole tensor, or the rows it needs).
    pub fn insert_lazy(&mut self, p: u32, layer: Option<usize>, lazy: Arc<dyn LazyParam>) {
        self.lazy.insert((p, layer), lazy);
    }

    /// The bytes the lazy params may keep resident between asks (0: none — every ask reads the source again).
    pub fn set_lazy_cache_bytes(&mut self, bytes: usize) {
        self.lazy_cache.lock().unwrap_or_else(|e| e.into_inner()).budget_bytes = bytes;
    }

    fn lazy_entry(&self, p: u32, layer: Option<usize>) -> Option<&Arc<dyn LazyParam>> {
        match layer {
            Some(l) => self.lazy.get(&(p, Some(l))).or_else(|| self.lazy.get(&(p, None))),
            None => self.lazy.get(&(p, None)),
        }
    }

    /// The param as an op reads it: the resident tensor, or the lazy one read now.
    pub fn view(&self, p: u32, layer: Option<usize>) -> Result<PView<'_>> {
        if let Ok(t) = self.get(p, layer) {
            return Ok(PView::Ref(t));
        }
        let Some(lazy) = self.lazy_entry(p, layer) else {
            return Err(LowerError::eval(format!("param {p} (layer {layer:?}) is not bound")));
        };
        let key = (p, layer);
        {
            let c = self.lazy_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = c.held.get(&key) {
                return Ok(PView::Owned(t.clone()));
            }
        }
        let t = Arc::new(lazy.load()?);
        let bytes = t.data.len() * std::mem::size_of::<f32>();
        let mut c = self.lazy_cache.lock().unwrap_or_else(|e| e.into_inner());
        if c.held_bytes + bytes <= c.budget_bytes {
            c.held_bytes += bytes;
            c.held.insert(key, t.clone());
        }
        Ok(PView::Owned(t))
    }

    /// The shape of a param instance, resident or lazy, without reading it.
    pub fn shape_of(&self, p: u32, layer: Option<usize>) -> Result<Vec<usize>> {
        if let Ok(t) = self.get(p, layer) {
            return Ok(t.shape.clone());
        }
        self.lazy_entry(p, layer)
            .map(|l| l.shape().to_vec())
            .ok_or_else(|| LowerError::eval(format!("param {p} (layer {layer:?}) is not bound")))
    }

    /// Rows `rows` of a param instance (every axis but the last flattened): sliced from a resident tensor, read from the source for a lazy one.
    pub fn rows_of(&self, p: u32, layer: Option<usize>, rows: Range<usize>) -> Result<Tensor> {
        if let Ok(t) = self.get(p, layer) {
            return crate::weights::slice_rows(t, rows);
        }
        let key = (p, layer);
        if let Some(t) = self.lazy_cache.lock().unwrap_or_else(|e| e.into_inner()).held.get(&key) {
            return crate::weights::slice_rows(t, rows);
        }
        self.lazy_entry(p, layer)
            .ok_or_else(|| LowerError::eval(format!("param {p} (layer {layer:?}) is not bound")))?
            .rows(rows)
    }

    pub fn get(&self, p: u32, layer: Option<usize>) -> Result<&Tensor> {
        match layer {
            Some(l) => self.layered.get(&(p, l)).or_else(|| self.global.get(&p)),
            None => self.global.get(&p),
        }
        .ok_or_else(|| LowerError::eval(format!("param {p} (layer {layer:?}) is not bound")))
    }

    /// A pre-quantised param's stored integers (`None`: the param is float); one per expert for a
    /// stack of experts.
    pub fn get_q(&self, p: u32, layer: Option<usize>) -> Option<&Arc<Vec<QWeight>>> {
        match layer {
            Some(l) => self.quant.get(&(p, Some(l))).or_else(|| self.quant.get(&(p, None))),
            None => self.quant.get(&(p, None)),
        }
    }

    pub(crate) fn insert(&mut self, p: u32, layer: Option<usize>, t: Tensor, q: Option<Vec<QWeight>>) {
        if let Some(q) = q {
            self.quant.insert((p, layer), Arc::new(q));
        }
        match layer {
            Some(l) => {
                self.layered.insert((p, l), t);
            }
            None => {
                self.global.insert(p, t);
            }
        }
    }

    /// Bind every param from checkpoint tensors; returns the store and the checkpoint tensors
    /// the program never read.
    pub fn from_source(prog: &HlProgram, binding: &Binding, source: &dyn TensorSource) -> Result<(ParamStore, Vec<String>)> {
        if binding.srcs.len() != prog.params.len() {
            return Err(LowerError::eval("binding does not match the program's params"));
        }
        let r = Resolver::new(source, &binding.aliases).with_ignored(&binding.ignored_prefixes);
        let mut st = ParamStore::default();
        for (pi, d) in prog.params.iter().enumerate() {
            let pi = pi as u32;
            if d.per_layer {
                for l in layers_of_param(prog, pi) {
                    let (t, q) = bind_one(&binding.srcs[pi as usize], &r, Some(prog.model_layer(l)))
                        .map_err(|e| LowerError::weights(format!("param `{}` layer {}: {e}", d.name, prog.model_layer(l))))?;
                    check_shape(&d.name, &t, &d.shape)?;
                    st.insert(pi, Some(l), t, q);
                }
            } else {
                let (t, q) = bind_one(&binding.srcs[pi as usize], &r, None)
                    .map_err(|e| LowerError::weights(format!("param `{}`: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.insert(pi, None, t, q);
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
        return Err(LowerError::weights(format!(
            "param `{name}`: checkpoint gives {:?}, graph needs {want:?}{}",
            t.shape,
            crate::weights::size_one_hint(name, &t.shape, want)
        )));
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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SiteStat {
    pub absmax: f64,
    pub sum_sq: f64,
    pub count: u64,
    /// Absmax at position 0 alone, and over every later position: a first-token "sink" with
    /// massive activations shows up as `pos0_absmax ≫ rest_absmax`.
    pub pos0_absmax: f64,
    pub rest_absmax: f64,
    /// Per-channel absmax, when every observation of the site had the same length (empty for
    /// ragged sites such as attention scores over a growing history).
    pub chan_absmax: Vec<f32>,
    pub ragged: bool,
}

impl SiteStat {
    /// Fold another run's statistics of the same site into this one.
    pub fn merge(&mut self, o: &SiteStat) {
        self.absmax = self.absmax.max(o.absmax);
        self.sum_sq += o.sum_sq;
        self.pos0_absmax = self.pos0_absmax.max(o.pos0_absmax);
        self.rest_absmax = self.rest_absmax.max(o.rest_absmax);
        if self.count == 0 {
            self.chan_absmax = o.chan_absmax.clone();
            self.ragged = o.ragged;
        } else if o.count > 0 {
            if self.ragged || o.ragged || self.chan_absmax.len() != o.chan_absmax.len() {
                self.ragged = true;
                self.chan_absmax.clear();
            } else {
                for (a, b) in self.chan_absmax.iter_mut().zip(&o.chan_absmax) {
                    *a = a.max(*b);
                }
            }
        }
        self.count += o.count;
    }
    fn observe(&mut self, v: &[f32], pos: usize) {
        let mut row = 0f64;
        for x in v {
            let a = (*x as f64).abs();
            row = row.max(a);
            self.sum_sq += a * a;
        }
        self.absmax = self.absmax.max(row);
        if pos == 0 {
            self.pos0_absmax = self.pos0_absmax.max(row);
        } else {
            self.rest_absmax = self.rest_absmax.max(row);
        }
        if self.count == 0 {
            self.chan_absmax = v.iter().map(|x| x.abs()).collect();
        } else if !self.ragged {
            if self.chan_absmax.len() == v.len() {
                for (a, x) in self.chan_absmax.iter_mut().zip(v) {
                    *a = a.max(x.abs());
                }
            } else {
                self.ragged = true;
                self.chan_absmax.clear();
            }
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
    /// Every site's output of the current step, when tracing (cleared by each `step`).
    pub trace: Option<BTreeMap<String, Vec<f32>>>,
    /// Rows that replace the pre block's output at their positions (an image's rows in a
    /// multimodal LM, RFC-0003 II.4).
    pub overrides: BTreeMap<usize, Vec<f32>>,
    /// M-RoPE position components `(t, h, w)` by position (Qwen2-VL's `get_rope_index`); a
    /// position absent here reads its own index in all three.
    pub mrope_pos: BTreeMap<usize, [usize; 3]>,
    /// The hash constants of each PLE layer (`crate::ngram`), by model layer.
    ngram_tables: BTreeMap<usize, crate::ngram::NgramTables>,
}

impl<'a> Session<'a> {
    pub fn new(prog: &'a HlProgram, params: &'a ParamStore) -> Self {
        Session {
            prog,
            params,
            fixed: BTreeMap::new(),
            hist: BTreeMap::new(),
            pos: 0,
            sites: None,
            trace: None,
            overrides: BTreeMap::new(),
            mrope_pos: BTreeMap::new(),
            ngram_tables: BTreeMap::new(),
        }
    }
    pub fn with_site_stats(mut self) -> Self {
        self.sites = Some(BTreeMap::new());
        self
    }
    /// Record every site's output of each step (keys as in the statistics: `L3.attn.q`).
    pub fn with_trace(mut self) -> Self {
        self.trace = Some(BTreeMap::new());
        self
    }
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Evaluate one block occurrence at an explicit position, for drivers that run a sequence
    /// LAYER by layer (every position through layer `l` before layer `l+1`): positions of one
    /// occurrence must arrive in increasing order, because histories and recurrent states are
    /// this session's. Returns the block's outputs (carries, or `[logits]` for `post`).
    pub fn eval_occurrence(
        &mut self,
        bi: usize,
        layer: Option<usize>,
        carries: &[Vec<f32>],
        token: usize,
        pos: usize,
    ) -> Result<Vec<Vec<f32>>> {
        if token >= self.prog.vocab {
            return Err(LowerError::eval(format!("token {token} ≥ vocab {}", self.prog.vocab)));
        }
        self.pos = pos;
        self.eval_block(bi, layer, carries, token)
    }

    /// Logits for `token` at the current position; advances the position.
    pub fn step(&mut self, token: usize) -> Result<Vec<f32>> {
        if let Some(t) = self.trace.as_mut() {
            t.clear();
        }
        if token >= self.prog.vocab {
            return Err(LowerError::eval(format!("token {token} ≥ vocab {}", self.prog.vocab)));
        }
        let pre = self.prog.pre;
        let mut carries = self.eval_block(pre, None, &[], token)?;
        if let Some(row) = self.overrides.get(&self.pos) {
            carries[0] = row.clone();
        }
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
        if let Some(stats) = self.sites.as_mut() {
            // The block's inputs get a site of their own (`carry0`, …): a lowering that needs the
            // residual stream at code resolution reads its range here.
            for (k, c) in carries.iter().enumerate() {
                stats.entry(format!("{prefix}carry{k}")).or_default().observe(c, self.pos);
            }
        }
        let mut vals: Vec<Vec<Vec<f32>>> = Vec::with_capacity(block.nodes.len());
        for node in &block.nodes {
            let out = self.eval_node(node, &vals, carries, layer, token, &prefix)?;
            if let (Some(site), Some(stats)) = (&node.site, self.sites.as_mut()) {
                stats.entry(format!("{prefix}{site}")).or_default().observe(&out[0], self.pos);
            }
            if let (Some(site), Some(tr)) = (&node.site, self.trace.as_mut()) {
                tr.insert(format!("{prefix}{site}"), out[0].clone());
            }
            vals.push(out);
        }
        block.outputs.iter().map(|r| operand(*r, &vals, carries).map(<[f32]>::to_vec)).collect()
    }

    fn sub_site(&mut self, prefix: &str, site: &Option<String>, sub: &str, v: &[f32]) {
        if let (Some(s), Some(stats)) = (site, self.sites.as_mut()) {
            stats.entry(format!("{prefix}{s}.{sub}")).or_default().observe(v, self.pos);
        }
    }

    fn param(&self, r: Ref, layer: Option<usize>) -> Result<PView<'a>> {
        match r {
            Ref::Param(p) => self.params.view(p, layer),
            other => Err(LowerError::eval(format!("expected a param, got {other:?}"))),
        }
    }

    /// The shape of a param operand, without reading a lazy one.
    fn param_shape(&self, r: Ref, layer: Option<usize>) -> Result<Vec<usize>> {
        match r {
            Ref::Param(p) => self.params.shape_of(p, layer),
            other => Err(LowerError::eval(format!("expected a param, got {other:?}"))),
        }
    }

    /// Rows of a param operand (a stack of experts' expert `e` is rows `e·out .. (e+1)·out` of the flattened `[E·out, in]`).
    fn param_rows(&self, r: Ref, layer: Option<usize>, rows: Range<usize>) -> Result<Tensor> {
        match r {
            Ref::Param(p) => self.params.rows_of(p, layer, rows),
            other => Err(LowerError::eval(format!("expected a param, got {other:?}"))),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn eval_node(
        &mut self,
        node: &Node,
        vals: &[Vec<Vec<f32>>],
        carries: &[Vec<f32>],
        layer: Option<usize>,
        token: usize,
        prefix: &str,
    ) -> Result<Vec<Vec<f32>>> {
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
            Op::Linear { bias, lora } => {
                let w = self.param(ins[1], layer)?;
                let b = if *bias { Some(self.param(ins[2], layer)?) } else { None };
                let mut y = linear(x(0)?, &w, b.as_ref().map(|t| t.data.as_slice()));
                if let Some(l) = lora {
                    // Unmerged: y += (num/den)·B·(A·x), in f64 over the f32 operands.
                    let at = if *bias { 3 } else { 2 };
                    let (a, bm) = (self.param(ins[at], layer)?, self.param(ins[at + 1], layer)?);
                    let ax = linear(x(0)?, &a, None);
                    self.sub_site(prefix, &node.site, "lora_a", &ax);
                    let bax = linear(&ax, &bm, None);
                    let s = l.num as f64 / l.den as f64;
                    for (yo, d) in y.iter_mut().zip(&bax) {
                        *yo = (*yo as f64 + s * *d as f64) as f32;
                    }
                }
                one(y)
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
            Op::Xielu => {
                let sc = |i: usize| -> Result<f64> { Ok(self.param(ins[i], layer)?.data[0] as f64) };
                let (p, n, beta, eps) = (sc(1)?, sc(2)?, sc(3)?, sc(4)?);
                one(x(0)?.iter().map(|v| xielu(*v as f64, p, n, beta, eps) as f32).collect())
            }
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
                one(norm(x(0)?, spec.kind, spec.eps, spec.gain, gain.as_deref(), bias.as_deref(), *groups))
            }
            Op::GatedRmsNorm { eps, groups, gate_first, act: gate_act } => {
                let w = self.param(ins[2], layer)?;
                let gate: Vec<f32> = x(1)?.iter().map(|z| act(*gate_act, *z)).collect();
                self.sub_site(prefix, &node.site, "gate", &gate);
                one(gated_rms_norm_act(x(0)?, x(1)?, &w.data, *eps, *groups, *gate_first, *gate_act))
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
                let (c, s) = match (f.mrope, self.mrope_pos.get(&pos)) {
                    // M-RoPE: each frequency at its own position component.
                    (Some(mr), Some(tri)) => {
                        let parts: Vec<(Vec<f32>, Vec<f32>)> = tri.iter().map(|p| f.cos_sin(*p)).collect();
                        (0..parts[0].0.len()).map(|j| (parts[mr.component(j)].0[j], parts[mr.component(j)].1[j])).unzip()
                    }
                    _ => f.cos_sin(pos),
                };
                one(rope(x(0)?, *heads, *head_dim, *rotary_dim, *offset, *style, &c, &s))
            }
            Op::RopeAtBlock { heads, head_dim, rotary_dim, offset, style, table, ratio } => {
                let f = &self.prog.rope_tables[*table as usize];
                let (c, s) = f.cos_sin(pos - pos % ratio);
                one(rope(x(0)?, *heads, *head_dim, *rotary_dim, *offset, *style, &c, &s))
            }
            Op::StreamMean { streams } => {
                let v = x(0)?;
                let d = v.len() / streams;
                one((0..d).map(|j| ((0..*streams).map(|k| v[k * d + j] as f64).sum::<f64>() / *streams as f64) as f32).collect())
            }
            Op::StreamOuter { streams } => {
                let (o, w) = (x(0)?, x(1)?);
                if w.len() != *streams {
                    return Err(LowerError::eval(format!("StreamOuter: {} weights for {streams} streams", w.len())));
                }
                one((0..*streams).flat_map(|k| o.iter().map(move |v| *v * w[k]).collect::<Vec<f32>>()).collect())
            }
            Op::GroupDot { groups } => {
                let (a, b) = (x(0)?, x(1)?);
                let g = a.len() / groups;
                one((0..*groups).map(|k| (0..g).map(|j| a[k * g + j] as f64 * b[k * g + j] as f64).sum::<f64>() as f32).collect())
            }
            Op::GroupRepeat { groups, size } => {
                let a = x(0)?;
                if a.len() != *groups {
                    return Err(LowerError::eval(format!("GroupRepeat: {} values for {groups} groups", a.len())));
                }
                one(a.iter().flat_map(|v| std::iter::repeat_n(*v, *size)).collect())
            }
            Op::GatherRows { heads, dim } => {
                let ids = x(0)?.to_vec();
                let t = self.param(ins[1], layer)?;
                if ids.len() != *heads || t.shape[1] != *dim {
                    return Err(LowerError::eval("GatherRows: ids or table of the wrong shape"));
                }
                let mut out = Vec::with_capacity(heads * dim);
                for id in ids {
                    let r = id as usize;
                    if r >= t.shape[0] {
                        return Err(LowerError::eval(format!("GatherRows: row {r} of {}", t.shape[0])));
                    }
                    out.extend_from_slice(&t.data[r * dim..(r + 1) * dim]);
                }
                one(out)
            }
            Op::NgramIds { ple, layers } => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("NgramIds without a window state")) };
                // `layer` is the occurrence; the hash constants follow the model layer.
                let l = self.prog.model_layer(layer.ok_or_else(|| LowerError::eval("NgramIds outside a layer"))?);
                let ple_index = layers
                    .iter()
                    .find(|(ml, _)| *ml == l)
                    .map(|(_, i)| *i)
                    .ok_or_else(|| LowerError::eval(format!("NgramIds: layer {l} is not a PLE layer of this block")))?;
                if !self.ngram_tables.contains_key(&l) {
                    let mut spec = ple.clone();
                    spec.layer_index = ple_index;
                    self.ngram_tables.insert(l, crate::ngram::NgramTables::new(&spec));
                }
                let t = self.ngram_tables[&l].clone();
                let st = self.fixed_mut(s, lyr);
                let window: Vec<i64> = st.iter().map(|v| *v as i64).collect();
                let ids: Vec<f32> = t.ids(token as i64, &window).into_iter().map(|v| v as f32).collect();
                // the next position's window: this token, then the older ones unless this token ends a segment
                let eos = t.eos;
                for k in (1..st.len()).rev() {
                    st[k] = if token as i64 == eos { eos as f32 } else { st[k - 1] };
                }
                st[0] = token as f32;
                one(ids)
            }
            Op::BlockMean { ratio } => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("BlockMean without a state")) };
                let k = x(0)?.to_vec();
                let sum = self.fixed_mut(s, lyr);
                if pos % ratio == 0 {
                    sum.iter_mut().for_each(|v| *v = 0.0);
                }
                for (a, b) in sum.iter_mut().zip(&k) {
                    *a += *b;
                }
                one(sum.iter().map(|v| *v / *ratio as f32).collect())
            }
            Op::BlockWrite { ratio, blocks } => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("BlockWrite without a state")) };
                let row = x(0)?.to_vec();
                let dim = row.len();
                if (pos + 1) % ratio == 0 && pos / ratio < *blocks {
                    let b = pos / ratio;
                    self.fixed_mut(s, lyr)[b * dim..(b + 1) * dim].copy_from_slice(&row);
                }
                Ok(vec![vec![]])
            }
            Op::BlockSelect { heads, dim, ratio, blocks, top } => {
                let Ref::State(s) = ins[2] else { return Err(LowerError::eval("BlockSelect without a state")) };
                let (q, cand) = (x(0)?.to_vec(), x(1)?.to_vec());
                let keys = self.fixed_mut(s, lyr).clone();
                let complete_old = pos / ratio;
                let completing = (pos + 1) % ratio == 0;
                let score = |key: &[f32]| -> f64 {
                    (0..*heads).map(|h| (0..*dim).map(|j| q[h * dim + j] as f64 * key[j] as f64).sum::<f64>().max(0.0)).sum::<f64>() / (*dim as f64).sqrt()
                };
                let scores: Vec<f64> = (0..*blocks)
                    .map(|b| {
                        if b < complete_old {
                            score(&keys[b * dim..(b + 1) * dim])
                        } else if b == complete_old && completing {
                            score(&cand)
                        } else {
                            f64::NEG_INFINITY
                        }
                    })
                    .collect();
                one(top_k_indices(&scores, *top).into_iter().map(|i| i as f32).collect())
            }
            Op::PosScale { temp } => {
                let t = temp.at(pos);
                one(x(0)?.iter().map(|v| v * t).collect())
            }
            Op::ScaleParam => {
                let c = self.param(ins[1], layer)?.data[0];
                one(x(0)?.iter().map(|v| v * c).collect())
            }
            Op::HistAppend => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("HistAppend without a state")) };
                let row = x(0)?.to_vec();
                self.hist.entry((s, lyr)).or_default().push(row);
                Ok(vec![vec![]])
            }
            Op::Attention { heads, kv_heads, head_dim, v_head_dim, scale, softcap, window, alibi, sinks, chunk, blocks } => {
                let (Ref::State(ks), Ref::State(vs)) = (ins[1], ins[2]) else {
                    return Err(LowerError::eval("attention without states"));
                };
                let q = x(0)?.to_vec();
                let sinks_v = if *sinks { Some(self.param(ins[3], layer)?.data.clone()) } else { None };
                // Borrowed, not cloned: a clone would copy the whole history every layer and step.
                let keys: &[Vec<f32>] = self.hist.get(&(ks, lyr)).map(Vec::as_slice).unwrap_or(&[]);
                let values: &[Vec<f32>] = self.hist.get(&(vs, lyr)).map(Vec::as_slice).unwrap_or(&[]);
                // Sparse block attention: only the rows of the selected blocks and of the incomplete
                // tail take part (a masked softmax is the softmax over the rows left).
                let (keys_sel, values_sel);
                let (keys, values) = match blocks {
                    Some(ratio) => {
                        let ids = x(ins.len() - 1)?;
                        let n = keys.len();
                        let complete = n / ratio;
                        let sel: std::collections::BTreeSet<usize> = ids.iter().map(|v| *v as usize).collect();
                        let vis: Vec<usize> = (0..n).filter(|j| sel.contains(&(*j / ratio)) || *j >= complete * ratio).collect();
                        keys_sel = vis.iter().map(|j| keys[*j].clone()).collect::<Vec<_>>();
                        values_sel = vis.iter().map(|j| values[*j].clone()).collect::<Vec<_>>();
                        (keys_sel.as_slice(), values_sel.as_slice())
                    }
                    None => (keys, values),
                };
                let shape = AttnShape { heads: *heads, kv_heads: *kv_heads, head_dim: *head_dim, v_head_dim: *v_head_dim };
                let want = self.sites.is_some();
                // A chunk keeps the last `p mod c + 1` keys: the query's own chunk.
                let window = match chunk {
                    Some(c) => {
                        let own = keys.len().saturating_sub(1) % c + 1;
                        Some(window.map_or(own, |w| w.min(own)))
                    }
                    None => *window,
                };
                let (out, scores, probs) =
                    attention(&q, keys, values, shape, *scale, *softcap, window, alibi.as_ref(), sinks_v.as_deref(), want);
                self.sub_site(prefix, &node.site, "scores", &scores);
                self.sub_site(prefix, &node.site, "probs", &probs);
                one(out)
            }
            Op::MlaAttention { heads, nope, rope, v_dim, kv_lora, scale, indexer } => {
                let (Ref::State(ls), Ref::State(rs)) = (ins[1], ins[2]) else { return Err(LowerError::eval("MLA without states")) };
                let q = x(0)?.to_vec();
                let kvb = self.param(ins[3], layer)?;
                // DeepSeek sparse attention: the indexer's scores over the window and the tokens it keeps.
                let keep: Option<Vec<bool>> = match indexer {
                    Some(ix) => {
                        let (iq, iw) = (x(4)?.to_vec(), x(5)?.to_vec());
                        let Ref::State(ihs) = ins[6] else { return Err(LowerError::eval("a token indexer without its key history")) };
                        let keys: &[Vec<f32>] = self.hist.get(&(ihs, lyr)).map(Vec::as_slice).unwrap_or(&[]);
                        let scores = token_index_scores(&iq, &iw, keys, ix.heads, ix.dim);
                        let sv: Vec<f32> = scores.iter().map(|v| *v as f32).collect();
                        self.sub_site(prefix, &node.site, "idx_score", &sv);
                        Some(token_index_keep(&scores, ix.topk))
                    }
                    None => None,
                };
                let lat: &[Vec<f32>] = self.hist.get(&(ls, lyr)).map(Vec::as_slice).unwrap_or(&[]);
                let kr: &[Vec<f32>] = self.hist.get(&(rs, lyr)).map(Vec::as_slice).unwrap_or(&[]);
                let (mut qt, mut ctx) = (Vec::new(), Vec::new());
                let out = mla_traced_keep(&q, &kvb.data, lat, kr, *heads, *nope, *rope, *v_dim, *kv_lora, *scale, keep.as_deref(), &mut qt, &mut ctx);
                // The absorbed query and the latent context: where an integer MLA narrows.
                self.sub_site(prefix, &node.site, "qt", &qt);
                self.sub_site(prefix, &node.site, "latent_ctx", &ctx);
                one(out)
            }
            Op::CausalConv1d { channels, kernel, bias, act: a, dilation } => {
                let Ref::State(s) = ins[1] else { return Err(LowerError::eval("conv without state")) };
                let w = self.param(ins[2], layer)?.data.clone();
                let b = if *bias { Some(self.param(ins[3], layer)?.data.clone()) } else { None };
                let xv = x(0)?.to_vec();
                let st = self.fixed_mut(s, lyr);
                let pre = causal_conv_dilated(&xv, st, &w, b.as_deref(), *channels, *kernel, *dilation, None);
                // The pre-activation is a sub-site: an integer conv narrows there before its table.
                self.sub_site(prefix, &node.site, "pre", &pre);
                one(match a {
                    Some(a) => pre.iter().map(|v| act(*a, *v)).collect(),
                    None => pre,
                })
            }
            Op::GatedDelta { k_heads, v_heads, dk, dv, head_map, q_scale, channel_decay } => {
                let Ref::State(s) = ins[5] else { return Err(LowerError::eval("GDN without state")) };
                let (q, k, v, g, beta) = (x(0)?.to_vec(), x(1)?.to_vec(), x(2)?.to_vec(), x(3)?.to_vec(), x(4)?.to_vec());
                if g.len() != if *channel_decay { v_heads * dk } else { *v_heads } {
                    return Err(LowerError::eval(format!("internal: a gated delta's decay has {} values, not {}", g.len(), if *channel_decay { v_heads * dk } else { *v_heads })));
                }
                let st = self.fixed_mut(s, lyr);
                let mut deltas = Vec::new();
                let out =
                    gated_delta_traced(&q, &k, &v, &g, &beta, st, *k_heads, *v_heads, *dk, *dv, *head_map, *q_scale, &mut deltas);
                let snap = if self.sites.is_some() { self.fixed[&(s, lyr)].clone() } else { vec![] };
                self.sub_site(prefix, &node.site, "state", &snap);
                self.sub_site(prefix, &node.site, "delta", &deltas);
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
                let sel_bias = if router.selection_bias { Some(self.param(ins[1], layer)?.data.clone()) } else { None };
                let (idx, mut w) = route(&logits, sel_bias.as_deref(), router, *experts, *top_k);
                // Gemma-4: a learned per-expert scale on each selected weight.
                if router.per_expert_scale {
                    let pes = self.param(ins[1 + usize::from(router.selection_bias)], layer)?;
                    for (wj, e) in w.iter_mut().zip(&idx) {
                        *wj *= pes.data[*e];
                    }
                }
                self.sub_site(prefix, &node.site, "logits", &logits);
                Ok(vec![idx.iter().map(|i| *i as f32).collect(), w])
            }
            Op::MoeExperts { top_k, act: a, glu, bias, input_scaled, gated } => {
                let xv = x(0)?.to_vec();
                let idx: Vec<usize> = x(1)?.iter().map(|v| *v as usize).collect();
                let w = x(2)?.to_vec();
                // A gated expert: gate, up, down (inputs 3, 4, 5); a plain one (`MOE_EXPERTS_PLAIN_V1`): up, down (3, 4).
                let (g, u, d) = if *gated { (Some(ins[3]), ins[4], ins[5]) } else { (None, ins[3], ins[4]) };
                let biases = if *bias { Some((ins[6], ins[7], ins[8])) } else { None };
                // The stacks are read by EXPERT (the rows a router selected), never whole: a lazy stack (L1) is a few rows from its source.
                let u_shape = self.param_shape(u, layer)?;
                let (e_i, e_d) = (u_shape[1], u_shape[2]);
                let mut y = vec![0f64; e_d];
                // Sub-sites over the selected experts: what a lowering scales each stage by.
                let (mut gate_all, mut up_all, mut act_all, mut hidden_all, mut out_all) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
                for (j, &e) in idx.iter().enumerate().take(*top_k) {
                    let slice = |t: Ref, e: usize| -> Result<Vec<f32>> {
                        // Expert `e` of a `[E, a.., last]` stack is rows `e·rpe .. (e+1)·rpe` of its every-axis-but-the-last flattening.
                        let shape = self.param_shape(t, layer)?;
                        let rpe: usize = shape[1..shape.len() - 1].iter().product();
                        Ok(self.param_rows(t, layer, e * rpe..(e + 1) * rpe)?.data)
                    };
                    let (uw, dw) = (slice(u, e)?, slice(d, e)?);
                    let gw = g.map(|g| slice(g, e)).transpose()?;
                    let gb = biases.map(|(a, _, _)| slice(a, e)).transpose()?;
                    let ub = biases.map(|(_, b, _)| slice(b, e)).transpose()?;
                    let db = biases.map(|(_, _, c)| slice(c, e)).transpose()?;
                    // Llama-4: the expert reads `w · x` (in float32, as transformers scales it).
                    let xs: Vec<f32> = if *input_scaled { xv.iter().map(|v| v * w[j]).collect() } else { xv.clone() };
                    let uv = linear_raw(&xs, &uw, e_i, ub.as_deref());
                    let (gv, hv): (Vec<f32>, Vec<f32>) = match &gw {
                        Some(gw) => {
                            let gv = linear_raw(&xs, gw, e_i, gb.as_deref());
                            let hv = match glu {
                                Glu::Standard => gv.iter().zip(&uv).map(|(g, u)| act(*a, *g) * *u).collect(),
                                Glu::ClampedSwiGlu { alpha, limit } => {
                                    gv.iter().zip(&uv).map(|(g, u)| clamped_swiglu(*g, *u, *alpha, *limit)).collect()
                                }
                            };
                            (gv, hv)
                        }
                        // A plain expert: the activation of the up projection is the hidden vector.
                        None => (Vec::new(), uv.iter().map(|u| act(*a, *u)).collect()),
                    };
                    let ov = linear_raw(&hv, &dw, e_d, db.as_deref());
                    if self.sites.is_some() {
                        gate_all.extend_from_slice(&gv);
                        up_all.extend_from_slice(&uv);
                        match gw {
                            Some(_) => act_all.extend(gv.iter().map(|g| act(*a, *g))),
                            None => act_all.extend_from_slice(&hv),
                        }
                        hidden_all.extend_from_slice(&hv);
                        out_all.extend_from_slice(&ov);
                    }
                    let wj = if *input_scaled { 1.0 } else { w[j] as f64 };
                    for (yy, o) in y.iter_mut().zip(&ov) {
                        *yy += wj * *o as f64;
                    }
                }
                for (sub, v) in [("gate", &gate_all), ("up", &up_all), ("act", &act_all), ("hidden", &hidden_all), ("out", &out_all)] {
                    if !(v.is_empty() && !*gated && sub == "gate") {
                        self.sub_site(prefix, &node.site, sub, v);
                    }
                }
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
        Ref::Node(i, o) => vals
            .get(i as usize)
            .and_then(|v| v.get(o as usize))
            .map(Vec::as_slice)
            .ok_or_else(|| LowerError::eval(format!("node {i}.{o} not evaluated"))),
        Ref::Carry(c) => carries.get(c as usize).map(Vec::as_slice).ok_or_else(|| LowerError::eval(format!("carry {c}"))),
        other => Err(LowerError::eval(format!("operand {other:?} is not a value"))),
    }
}

// ───────────────────────────── kernels ─────────────────────────────

pub fn linear_raw(x: &[f32], w: &[f32], out: usize, b: Option<&[f32]>) -> Vec<f32> {
    use rayon::prelude::*;
    let n = x.len();
    let row = |o: usize| -> f32 {
        let r = &w[o * n..(o + 1) * n];
        let s: f64 = r.iter().zip(x).map(|(a, b)| *a as f64 * *b as f64).sum();
        (s + b.map(|b| b[o] as f64).unwrap_or(0.0)) as f32
    };
    // Rows are independent and each is summed in one fixed order, so the parallel result is the
    // sequential one bit for bit; small products stay on one thread.
    if out * n >= 1 << 16 { (0..out).into_par_iter().map(row).collect() } else { (0..out).map(row).collect() }
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
        let e = t
            * crate::detmath::exp(-a * a - 1.265_512_23
                + t * (1.000_023_68
                    + t * (0.374_091_96
                        + t * (0.096_784_18
                            + t * (-0.186_288_06
                                + t * (0.278_868_07
                                    + t * (-1.135_203_98 + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77)))))))));
        1.0 - e
    };
    if x < 0.0 { -r } else { r }
}

/// **xIELU** with the layer's scalars: `αp = softplus(p)`, `αn = β + softplus(n)`, `y = x > 0 ? αp·x² + β·x :
/// (expm1(min(x, ε)) − x)·αn + β·x` (`activations.py:XIELUActivation._xielu_python`; softplus at torch's threshold of 20).
pub fn xielu(x: f64, p: f64, n: f64, beta: f64, eps: f64) -> f64 {
    let softplus = |v: f64| if v > 20.0 { v } else { crate::detmath::ln_1p(crate::detmath::exp(v)) };
    let (alpha_p, alpha_n) = (softplus(p), beta + softplus(n));
    if x > 0.0 {
        alpha_p * x * x + beta * x
    } else {
        let z = x.min(eps);
        (expm1(z) - x) * alpha_n + beta * x
    }
}

/// `exp(z) − 1`, accurate near 0 (a series below 1e-5, where the difference of two nearly equal numbers loses digits).
fn expm1(z: f64) -> f64 {
    if z.abs() < 1e-5 { z * (1.0 + z * (0.5 + z / 6.0)) } else { crate::detmath::exp(z) - 1.0 }
}

pub fn act(a: Act, v: f32) -> f32 {
    let x = v as f64;
    let sig = |z: f64| 1.0 / (1.0 + crate::detmath::exp(-z));
    (match a {
        Act::Silu => x * sig(x),
        Act::Gelu => 0.5 * x * (1.0 + erf(x / std::f64::consts::SQRT_2)),
        Act::GeluTanh => 0.5 * x * (1.0 + crate::detmath::tanh((2.0 / std::f64::consts::PI).sqrt() * (x + 0.044715 * x * x * x))),
        Act::QuickGelu => x * sig(1.702 * x),
        Act::Relu => x.max(0.0),
        Act::Relu6 => x.clamp(0.0, 6.0),
        // torch: `x * relu6(x + 3) / 6` and `relu6(x + 3) / 6`
        Act::HardSwish => x * (x + 3.0).clamp(0.0, 6.0) / 6.0,
        Act::HardSigmoid => (x + 3.0).clamp(0.0, 6.0) / 6.0,
        Act::Relu2 => {
            let r = x.max(0.0);
            r * r
        }
        Act::Sigmoid => sig(x),
        Act::Tanh => crate::detmath::tanh(x),
        // torch.nn.functional.softplus(beta=1, threshold=20)
        Act::Softplus => {
            if x > 20.0 {
                x
            } else {
                crate::detmath::ln_1p(crate::detmath::exp(x))
            }
        }
        Act::Identity => x,
        // xIELU reads the layer's parameters (`Op::Xielu`); an activation of the input alone is none (the lowering refuses it).
        Act::Xielu => f64::NAN,
        // torch: `x.abs().clamp_min(1e-6).sqrt() * x.sign()`
        Act::SignedSqrt => {
            let sign = if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            };
            x.abs().max(1e-6).sqrt() * sign
        }
    }) as f32
}

fn clamped_swiglu(g: f32, u: f32, alpha: f64, limit: f64) -> f32 {
    let g = (g as f64).min(limit);
    let u = (u as f64).clamp(-limit, limit);
    let glu = g * (1.0 / (1.0 + crate::detmath::exp(-(g * alpha))));
    ((u + 1.0) * glu) as f32
}

pub fn norm(
    x: &[f32],
    kind: NormKind,
    eps: f64,
    gain_kind: crate::spec::Gain,
    gain: Option<&Tensor>,
    bias: Option<&Tensor>,
    groups: usize,
) -> Vec<f32> {
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
    gated_rms_norm_act(x, z, w, eps, groups, gate_first, Act::Silu)
}

/// [`gated_rms_norm`] with the gate's activation named.
pub fn gated_rms_norm_act(x: &[f32], z: &[f32], w: &[f32], eps: f64, groups: usize, gate_first: bool, gate_act: Act) -> Vec<f32> {
    let n = x.len();
    let silu = |v: f32| act(gate_act, v);
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

#[derive(Clone, Copy, Debug)]
pub struct AttnShape {
    pub heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub v_head_dim: usize,
}

/// Softmax attention of one query position over the history rows `keys[j]`/`values[j]`
/// (row `j` = absolute position `j`). Query head `h` reads kv head `h / (heads/kv_heads)`
/// (`repeat_kv`). A window keeps the last `window` rows. ALiBi adds `slope_h · j` (a per-row
/// constant away from `−slope·(i−j)`), scaled with the scores for Falcon. Sinks join the softmax
/// and are dropped. Returns `(out, scores, probs)`; the last two only when `want_sites`.
#[allow(clippy::too_many_arguments)]
pub fn attention(
    q: &[f32],
    keys: &[Vec<f32>],
    values: &[Vec<f32>],
    sh: AttnShape,
    scale: f64,
    softcap: Option<f64>,
    window: Option<usize>,
    alibi: Option<&crate::rope::AlibiSpec>,
    sinks: Option<&[f32]>,
    want_sites: bool,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let AttnShape { heads, kv_heads, head_dim, v_head_dim } = sh;
    let n = keys.len();
    let start = window.map(|w| n.saturating_sub(w)).unwrap_or(0);
    let group = heads / kv_heads;
    let mut out = vec![0f32; heads * v_head_dim];
    let (mut all_scores, mut all_probs) = (Vec::new(), Vec::new());
    for h in 0..heads {
        let kvh = h / group;
        let qh = &q[h * head_dim..(h + 1) * head_dim];
        let mut sc: Vec<f64> = (start..n)
            .map(|j| {
                let kj = &keys[j][kvh * head_dim..(kvh + 1) * head_dim];
                let dot: f64 = qh.iter().zip(kj).map(|(a, b)| *a as f64 * *b as f64).sum();
                match alibi {
                    Some(al) => {
                        let slope = al.slopes[h];
                        let bias =
                            if al.bf16_bias { bf16_round(bf16_round(slope as f32) * j as f32) as f64 } else { slope * j as f64 };
                        if al.scaled_by_softmax_scale { (dot + bias) * scale } else { dot * scale + bias }
                    }
                    None => dot * scale,
                }
            })
            .collect();
        if let Some(c) = softcap {
            sc.iter_mut().for_each(|s| *s = (*s / c).tanh() * c);
        }
        let p = softmax_with_sink(&sc, sinks.map(|s| s[h] as f64));
        if want_sites {
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
    (out, all_scores, all_probs)
}

pub fn softmax_with_sink(s: &[f64], sink: Option<f64>) -> Vec<f64> {
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
pub fn mla(
    q: &[f32],
    kvb: &[f32],
    lat: &[Vec<f32>],
    kr: &[Vec<f32>],
    heads: usize,
    nope: usize,
    rope: usize,
    vd: usize,
    r: usize,
    scale: f64,
) -> Vec<f32> {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    mla_traced(q, kvb, lat, kr, heads, nope, rope, vd, r, scale, &mut a, &mut b)
}

/// [`mla`], also returning every head's absorbed query `W_kᵀ q_nope` and latent context.
#[allow(clippy::too_many_arguments)]
pub fn mla_traced(
    q: &[f32],
    kvb: &[f32],
    lat: &[Vec<f32>],
    kr: &[Vec<f32>],
    heads: usize,
    nope: usize,
    rope: usize,
    vd: usize,
    r: usize,
    scale: f64,
    qt_out: &mut Vec<f32>,
    ctx_out: &mut Vec<f32>,
) -> Vec<f32> {
    mla_traced_keep(q, kvb, lat, kr, heads, nope, rope, vd, r, scale, None, qt_out, ctx_out)
}

/// The indexer's scores of DeepSeek sparse attention over its key history (`ATTN_TOKEN_INDEXER_V1`):
/// `s_t = Σ_h w_h · ReLU(q_h · k_t)`. The positive factors `head_dim^-½` and `heads^-½` of the model's formula
/// move no rank and are left out, so a score is in the units the integer lowering calibrates.
pub fn token_index_scores(iq: &[f32], iw: &[f32], keys: &[Vec<f32>], heads: usize, dim: usize) -> Vec<f64> {
    keys.iter()
        .map(|k| {
            (0..heads)
                .map(|h| {
                    let dot: f64 = (0..dim).map(|j| iq[h * dim + j] as f64 * k[j] as f64).sum();
                    iw[h] as f64 * dot.max(0.0)
                })
                .sum()
        })
        .collect()
}

/// The tokens a top-`topk` selection keeps: the `min(topk, n)` best scores, **ties to the lowest index** (the IR's
/// `TopK` rule).
pub fn token_index_keep(scores: &[f64], topk: usize) -> Vec<bool> {
    let mut keep = vec![false; scores.len()];
    for i in top_k_indices(scores, topk.min(scores.len())) {
        keep[i] = true;
    }
    keep
}

/// [`mla_traced`] over the history rows `keep` marks (all of them when `None`): DeepSeek sparse attention's softmax.
#[allow(clippy::too_many_arguments)]
pub fn mla_traced_keep(
    q: &[f32],
    kvb: &[f32],
    lat: &[Vec<f32>],
    kr: &[Vec<f32>],
    heads: usize,
    nope: usize,
    rope: usize,
    vd: usize,
    r: usize,
    scale: f64,
    keep: Option<&[bool]>,
    qt_out: &mut Vec<f32>,
    ctx_out: &mut Vec<f32>,
) -> Vec<f32> {
    let qd = nope + rope;
    let mut out = vec![0f32; heads * vd];
    let rows: Vec<usize> = (0..lat.len()).filter(|j| keep.is_none_or(|k| k[*j])).collect();
    for h in 0..heads {
        let kv_rows = &kvb[h * (nope + vd) * r..(h + 1) * (nope + vd) * r];
        let (wk, wv) = kv_rows.split_at(nope * r);
        let qn = &q[h * qd..h * qd + nope];
        let qr = &q[h * qd + nope..(h + 1) * qd];
        let qt: Vec<f64> = (0..r).map(|c| (0..nope).map(|i| wk[i * r + c] as f64 * qn[i] as f64).sum()).collect();
        let sc: Vec<f64> = rows
            .iter()
            .map(|&j| {
                let a: f64 = qt.iter().zip(&lat[j]).map(|(x, y)| x * *y as f64).sum();
                let b: f64 = qr.iter().zip(&kr[j]).map(|(x, y)| *x as f64 * *y as f64).sum();
                (a + b) * scale
            })
            .collect();
        let p = softmax_with_sink(&sc, None);
        let ctx: Vec<f64> = (0..r).map(|c| rows.iter().enumerate().map(|(n, &j)| p[n] * lat[j][c] as f64).sum()).collect();
        qt_out.extend(qt.iter().map(|v| *v as f32));
        ctx_out.extend(ctx.iter().map(|v| *v as f32));
        for i in 0..vd {
            out[h * vd + i] = (0..r).map(|c| wv[i * r + c] as f64 * ctx[c]).sum::<f64>() as f32;
        }
    }
    out
}

pub fn causal_conv(x: &[f32], st: &mut Vec<f32>, w: &[f32], b: Option<&[f32]>, ch: usize, k: usize, a: Option<Act>) -> Vec<f32> {
    causal_conv_dilated(x, st, w, b, ch, k, 1, a)
}

/// [`causal_conv`] with the taps `dilation` positions apart: the state holds the last
/// `(k − 1)·dilation` rows of `ch` (oldest first), tap `t < k − 1` reads its row `t·dilation`, the
/// last tap the input. Zeros before the sequence start.
#[allow(clippy::too_many_arguments)]
pub fn causal_conv_dilated(x: &[f32], st: &mut Vec<f32>, w: &[f32], b: Option<&[f32]>, ch: usize, k: usize, dil: usize, a: Option<Act>) -> Vec<f32> {
    let mut out = vec![0f32; ch];
    for c in 0..ch {
        let mut s = 0f64;
        for t in 0..k {
            let v = if t + 1 == k { x[c] } else { st[t * dil * ch + c] };
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
pub fn gated_delta(
    q: &[f32],
    k: &[f32],
    v: &[f32],
    g: &[f32],
    beta: &[f32],
    s: &mut [f32],
    nk: usize,
    nv: usize,
    dk: usize,
    dv: usize,
    map: HeadMap,
    q_scale: f64,
) -> Vec<f32> {
    let mut unused = Vec::new();
    gated_delta_traced(q, k, v, g, beta, s, nk, nv, dk, dv, map, q_scale, &mut unused)
}

/// [`gated_delta`], also returning every head's delta `β(v − Sᵀk)` in `deltas`.
#[allow(clippy::too_many_arguments)]
pub fn gated_delta_traced(
    q: &[f32],
    k: &[f32],
    v: &[f32],
    g: &[f32],
    beta: &[f32],
    s: &mut [f32],
    nk: usize,
    nv: usize,
    dk: usize,
    dv: usize,
    map: HeadMap,
    q_scale: f64,
    deltas: &mut Vec<f32>,
) -> Vec<f32> {
    let rep = nv / nk;
    let mut out = vec![0f32; nv * dv];
    for vh in 0..nv {
        let kh = match map {
            HeadMap::Group => vh / rep,
            HeadMap::Tile => vh % nk,
        };
        let st = &mut s[vh * dk * dv..(vh + 1) * dk * dv];
        // A decay of `nv·dk` values is channel-wise (`MIXER_KDA_V1`): row `i` of the state (a key channel) decays by `exp(g[vh, i])`.
        if g.len() == nv * dk && dk > 1 {
            for i in 0..dk {
                let decay = (g[vh * dk + i] as f64).exp();
                st[i * dv..(i + 1) * dv].iter_mut().for_each(|x| *x = (*x as f64 * decay) as f32);
            }
        } else {
            let decay = (g[vh] as f64).exp();
            st.iter_mut().for_each(|x| *x = (*x as f64 * decay) as f32);
        }
        let kv = &k[kh * dk..(kh + 1) * dk];
        let qv = &q[kh * dk..(kh + 1) * dk];
        let vv = &v[vh * dv..(vh + 1) * dv];
        let mut delta = vec![0f64; dv];
        for (j, dj) in delta.iter_mut().enumerate() {
            let mem: f64 = (0..dk).map(|i| st[i * dv + j] as f64 * kv[i] as f64).sum();
            *dj = (vv[j] as f64 - mem) * beta[vh] as f64;
        }
        deltas.extend(delta.iter().map(|d| *d as f32));
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
pub fn selective_scan(
    x: &[f32],
    dt: &[f32],
    b: &[f32],
    c: &[f32],
    a: &[f32],
    d: &[f32],
    h: &mut [f32],
    inner: usize,
    n: usize,
) -> Vec<f32> {
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
pub fn ssd_step(
    x: &[f32],
    dt: &[f32],
    b: &[f32],
    c: &[f32],
    a: &[f32],
    d: &[f32],
    h: &mut [f32],
    heads: usize,
    p: usize,
    groups: usize,
    n: usize,
) -> Vec<f32> {
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

pub fn wkv4(k: &[f32], v: &[f32], w: &[f32], u: &[f32], num: &mut [f32], den: &mut [f32], mx: &mut [f32]) -> Vec<f32> {
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
    if r.scoring == Scoring::SparseMixer {
        return sparsemixer(&l, r.jitter_eps, r.scale);
    }
    let (scores, mut choice): (Vec<f64>, Vec<f64>) = match r.scoring {
        // The selection bias (ERNIE-4.5's `e_score_correction_bias`, DeepSeek-V3's) joins the CHOICE scores only; the
        // weights stay the unbiased probabilities (`scores`).
        Scoring::Softmax => {
            let p = softmax_with_sink(&l, None);
            let c = p.iter().enumerate().map(|(i, v)| v + sel_bias.map(|b| b[i] as f64).unwrap_or(0.0)).collect();
            (p, c)
        }
        Scoring::Sigmoid => {
            let s: Vec<f64> = l.iter().map(|x| 1.0 / (1.0 + (-x).exp())).collect();
            let c = s.iter().enumerate().map(|(i, v)| v + sel_bias.map(|b| b[i] as f64).unwrap_or(0.0)).collect();
            (s, c)
        }
        Scoring::TopKThenSoftmax | Scoring::TopKThenSigmoid | Scoring::SparseMixer => (l.clone(), l.clone()),
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
        // A masked expert scores 0 under plain softmax (every probability is ≥ 0); with a bias a kept expert can score
        // below 0, so the mask must be below every biased score.
        let fill = if r.scoring == Scoring::Softmax && sel_bias.is_none() { 0.0 } else { f64::NEG_INFINITY };
        for (i, c) in choice.iter_mut().enumerate() {
            if !keep.contains(&(i / per)) {
                *c = fill;
            }
        }
    }
    let idx = top_k_indices(&choice, k);
    let mut w: Vec<f64> = match r.scoring {
        Scoring::TopKThenSoftmax | Scoring::SparseMixer => softmax_with_sink(&idx.iter().map(|i| l[*i]).collect::<Vec<_>>(), None),
        // Softmax weights are the choice scores — masked experts (DeepSeek-V2's group-limited greedy) weigh 0 — unless a
        // selection bias made the choice scores something else: then the weights stay the unbiased probabilities.
        Scoring::Softmax if sel_bias.is_none() => idx.iter().map(|i| choice[*i]).collect(),
        Scoring::Softmax | Scoring::Sigmoid => idx.iter().map(|i| scores[*i]).collect(),
        Scoring::TopKThenSigmoid => idx.iter().map(|i| 1.0 / (1.0 + (-(l[*i] as f32)).exp()) as f64).collect(),
    };
    if r.normalize {
        let s: f64 = w.iter().sum::<f64>() + r.norm_eps;
        w.iter_mut().for_each(|x| *x /= s);
    }
    w.iter_mut().for_each(|x| *x *= r.scale);
    (idx, w.into_iter().map(|x| x as f32).collect())
}

/// Phi-3.5-MoE's `sparsemixer` at inference, as transformers computes it: the argmax `i1` (the
/// first on ties), weighted by the softmax at `i1` of the scores not past the threshold
/// `(m − s_j) / max(|s_j|, m) > 2ε`; then the same over the scores with `i1` masked, the threshold
/// still read from the ORIGINAL scores.
fn sparsemixer(s: &[f64], eps: f64, scale: f64) -> (Vec<usize>, Vec<f32>) {
    let pick = |row: &[f64]| -> (usize, f64) {
        let i = row.iter().enumerate().fold(0, |b, (j, v)| if *v > row[b] { j } else { b });
        let m = row[i];
        let gated: Vec<f64> =
            row.iter().zip(s).map(|(r, x)| if (m - x) / x.abs().max(m) > 2.0 * eps { f64::NEG_INFINITY } else { *r }).collect();
        (i, softmax_with_sink(&gated, None)[i])
    };
    let (i1, w1) = pick(s);
    let mut rest = s.to_vec();
    rest[i1] = f64::NEG_INFINITY;
    let (i2, w2) = pick(&rest);
    (vec![i1, i2], vec![(w1 * scale) as f32, (w2 * scale) as f32])
}

#[cfg(test)]
mod tests;
