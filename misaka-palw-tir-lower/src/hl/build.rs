//! **`ArchSpec` → `HlProgram`.** One generic builder for every family: the residual wiring, the
//! mixers (attention, MLA, GDN, Mamba, Mamba2, RWKV) and the FFNs (MLP, MoE, RWKV channel mix)
//! are each one function, and a layer block is their composition. Equal `LayerSpec`s share a
//! block, which is how the per-layer schedule of block kinds arises.
//!
//! **Frontend-neutral.** The builder reads only the math of the spec — never `ArchSpec::hf` — and
//! names params by role (`attn.q.w`, `gdn.A`, …). Filling them from a checkpoint is the frontend's
//! weight mapping (`crate::hf_weights`).

use super::*;
use crate::error::{LowerError, Result};
use crate::spec::*;
use std::collections::BTreeMap;

pub fn build_program(spec: &ArchSpec) -> Result<HlProgram> {
    let carries = carries_of(spec)?;
    let mut b = Builder {
        s: spec,
        params: vec![],
        pidx: BTreeMap::new(),
        states: vec![],
        sidx: BTreeMap::new(),
        ropes: vec![],
        blocks: vec![],
        carries: carries.clone(),
        carry_out: BTreeMap::new(),
    };
    let pre = b.pre_block()?;
    let mut kinds: Vec<(LayerSpec, Vec<u16>)> = Vec::new();
    let mut schedule = Vec::with_capacity(spec.layers.len());
    let mut layer_of = Vec::with_capacity(spec.layers.len());
    for (li, ls) in spec.layers.iter().enumerate() {
        // Layers that differ only in constants the program reads as data (a PLE layer's hash
        // constants follow from its index) run the same block.
        let ls = &block_kind(ls);
        let bis = match kinds.iter().find(|(s, _)| s == ls) {
            Some((_, bis)) => bis.clone(),
            None => {
                let bis = b
                    .layer_blocks(ls, kinds.len())?
                    .into_iter()
                    .map(|bi| u16::try_from(bi).map_err(|_| LowerError::not_lowerable("more than 65535 blocks")))
                    .collect::<Result<Vec<u16>>>()?;
                kinds.push((ls.clone(), bis.clone()));
                bis
            }
        };
        for bi in bis {
            schedule.push(bi);
            layer_of.push(li);
        }
    }
    let post = b.post_block()?;
    // The layers that run each n-gram block, with their index among the PLE layers.
    for (li, ls) in spec.layers.iter().enumerate() {
        if let Residual::HyperConnection { ple: Some(p) } = &ls.residual {
            for (slot, l) in layer_of.iter().enumerate() {
                if *l != li {
                    continue;
                }
                let bi = schedule[slot] as usize;
                for n in &mut b.blocks[bi].nodes {
                    if let Op::NgramIds { layers, .. } = &mut n.op
                        && !layers.contains(&(li, p.layer_index))
                    {
                        layers.push((li, p.layer_index));
                    }
                }
            }
        }
    }
    let carries = b.carries.clone();
    let p = HlProgram {
        architecture: spec.architecture.clone(),
        output: match spec.output {
            OutputSpec::Logits => HlOutput::Logits,
            OutputSpec::Embedding { normalize, .. } => HlOutput::Embedding { normalized: normalize },
        },
        vocab: spec.vocab_size,
        hidden: spec.hidden_size,
        carries,
        params: b.params,
        states: b.states,
        rope_tables: b.ropes,
        blocks: b.blocks,
        pre,
        post,
        schedule,
        layer_of,
    };
    p.validate().map_err(|e| LowerError::eval(format!("internal: built program is malformed: {e}")))?;
    Ok(p)
}

struct Builder<'a> {
    s: &'a ArchSpec,
    params: Vec<ParamDecl>,
    pidx: BTreeMap<String, u32>,
    states: Vec<StateDecl>,
    sidx: BTreeMap<String, u32>,
    ropes: Vec<crate::rope::RopeFreqs>,
    blocks: Vec<Block>,
    /// The carries (the residual first, then each KV-sharing slot's key and value rows).
    carries: Vec<CarryDecl>,
    /// What the block under construction writes to a carry past the residual.
    carry_out: BTreeMap<usize, Ref>,
}

/// A layer spec as the block builder sees it: the hash constants of a PLE layer (its index among
/// the PLE layers) are data the lowering fills per layer, not part of the block.
fn block_kind(ls: &LayerSpec) -> LayerSpec {
    let mut k = ls.clone();
    if let Residual::HyperConnection { ple: Some(p) } = &mut k.residual {
        p.layer_index = 0;
    }
    k
}

/// The carries a spec needs: the residual stream (`streams` copies of the hidden width under
/// hyper-connections), then a key row and a value row for each KV slot a [`KvShare::Source`] layer fills.
fn carries_of(spec: &ArchSpec) -> Result<Vec<CarryDecl>> {
    // What a program carries and a block holds: the streams of a gated residual are a few, and their
    // carry (`streams × hidden`) is one tensor under NF-8's `2^24`.
    if let Some(h) = &spec.hyper
        && (h.streams == 0
            || h.streams > 64
            || h.lowrank == 0
            || h.lowrank > 1 << 16
            || h.streams.saturating_mul(spec.hidden_size) > 1 << 24)
    {
        return Err(LowerError::not_lowerable(format!(
            "RESIDUAL_GATED_HC_V1: {} streams of {} with a low rank of {} are past what a program carries (1..=64 streams, `streams × hidden` ≤ 2^24, rank ≤ 65,536)",
            h.streams, spec.hidden_size, h.lowrank
        )));
    }
    let streams = spec.hyper.as_ref().map_or(1, |h| h.streams);
    let mut out = vec![CarryDecl { name: "h".into(), shape: vec![spec.hidden_size * streams] }];
    // slot → the (kv heads, head width, value width) of the one layer that fills it.
    let mut slots: BTreeMap<usize, (usize, usize, usize)> = BTreeMap::new();
    for (li, ls) in spec.layers.iter().enumerate() {
        let Mixer::Attention(a) = &ls.mixer else { continue };
        let dims = (a.kv_heads, a.head_dim, a.v_head_dim);
        match a.kv_share {
            Some(KvShare::Source { slot }) => {
                if slots.insert(slot, dims).is_some() {
                    return Err(LowerError::eval(format!("internal: KV slot {slot} is filled by two layers")));
                }
            }
            Some(KvShare::Consumer { slot }) => match slots.get(&slot) {
                None => return Err(LowerError::eval(format!("internal: layer {li} reads KV slot {slot} before any layer fills it"))),
                Some(d) if *d != dims => {
                    return Err(LowerError::eval(format!("internal: layer {li} reads KV slot {slot} at other head shapes")));
                }
                Some(_) => {}
            },
            None => {}
        }
    }
    let slots: BTreeMap<usize, (usize, usize)> = slots.into_iter().map(|(s, (kv, hd, vd))| (s, (kv * hd, kv * vd))).collect();
    for (i, (slot, (k, v))) in slots.into_iter().enumerate() {
        if slot != i {
            return Err(LowerError::eval("internal: KV slots are not numbered from 0"));
        }
        out.push(CarryDecl { name: format!("kv{slot}.k"), shape: vec![k] });
        out.push(CarryDecl { name: format!("kv{slot}.v"), shape: vec![v] });
    }
    Ok(out)
}

/// A block under construction.
struct Bk {
    nodes: Vec<Node>,
}

impl Bk {
    fn push(
        &mut self,
        op: Op,
        inputs: Vec<Ref>,
        outs: Vec<Vec<usize>>,
        types: Vec<HlType>,
        site: Option<&str>,
        writes: Vec<u32>,
    ) -> NodeId {
        self.nodes.push(Node { op, inputs, outs, out_types: types, site: site.map(str::to_string), writes });
        (self.nodes.len() - 1) as NodeId
    }
    /// One f32 vector output of length `n`, with a site.
    fn f(&mut self, op: Op, inputs: Vec<Ref>, n: usize, site: &str) -> Ref {
        Ref::Node(self.push(op, inputs, vec![vec![n]], vec![HlType::F32], Some(site), vec![]), 0)
    }
    /// A structural op (no requantisation, no site).
    fn st(&mut self, op: Op, inputs: Vec<Ref>, n: usize) -> Ref {
        Ref::Node(self.push(op, inputs, vec![vec![n]], vec![HlType::F32], None, vec![]), 0)
    }
    /// An op with one f32 output that also writes states.
    fn fw(&mut self, op: Op, inputs: Vec<Ref>, n: usize, site: Option<&str>, writes: Vec<u32>) -> Ref {
        Ref::Node(self.push(op, inputs, vec![vec![n]], vec![HlType::F32], site, writes), 0)
    }
    fn append(&mut self, x: Ref, st: Ref) {
        let Ref::State(i) = st else { return };
        self.push(Op::HistAppend, vec![x, st], vec![vec![]], vec![HlType::Unit], None, vec![i]);
    }
}

fn sid(r: Ref) -> u32 {
    if let Ref::State(i) = r { i } else { u32::MAX }
}

const W_STD: f32 = 0.08;

impl Builder<'_> {
    fn param(&mut self, name: &str, shape: Vec<usize>, per_layer: bool, init: Init) -> Result<Ref> {
        if let Some(&i) = self.pidx.get(name) {
            let d = &self.params[i as usize];
            if d.shape != shape || d.per_layer != per_layer {
                return Err(LowerError::eval(format!("internal: param `{name}` declared as {:?} and {shape:?}", d.shape)));
            }
            return Ok(Ref::Param(i));
        }
        let i = self.params.len() as u32;
        self.params.push(ParamDecl { name: name.to_string(), shape, per_layer, init });
        self.pidx.insert(name.to_string(), i);
        Ok(Ref::Param(i))
    }

    fn state(&mut self, name: &str, kind: StateKind, shape: Vec<usize>, init: f32) -> Result<Ref> {
        if let Some(&i) = self.sidx.get(name) {
            let d = &self.states[i as usize];
            if d.shape != shape || d.kind != kind {
                return Err(LowerError::eval(format!("internal: state `{name}` declared twice differently")));
            }
            return Ok(Ref::State(i));
        }
        let i = self.states.len() as u32;
        self.states.push(StateDecl { name: name.to_string(), kind, shape, per_layer: true, init });
        self.sidx.insert(name.to_string(), i);
        Ok(Ref::State(i))
    }

    fn rope_table(&mut self, f: &crate::rope::RopeFreqs) -> u32 {
        if let Some(i) = self.ropes.iter().position(|r| r == f) {
            return i as u32;
        }
        self.ropes.push(f.clone());
        (self.ropes.len() - 1) as u32
    }

    /// `Linear` `[out, in]` under role `name` (params `name.w`, `name.b`).
    #[allow(clippy::too_many_arguments)]
    fn linear(
        &mut self,
        bk: &mut Bk,
        x: Ref,
        name: &str,
        out: usize,
        inp: usize,
        bias: bool,
        per_layer: bool,
        site: &str,
    ) -> Result<Ref> {
        let w = self.param(&format!("{name}.w"), vec![out, inp], per_layer, Init::Normal(W_STD))?;
        let mut ins = vec![x, w];
        if bias {
            ins.push(self.param(&format!("{name}.b"), vec![out], per_layer, Init::Uniform(-0.1, 0.1))?);
        }
        // A LoRA adapter on this role (a layer's projection): `A [r, in]`, `B [out, r]`.
        let lora = match self.s.adapter.as_ref().and_then(|a| a.role(name)) {
            Some(l) if per_layer => {
                ins.push(self.param(&format!("{name}.lora_a"), vec![l.rank, inp], true, Init::Normal(W_STD))?);
                ins.push(self.param(&format!("{name}.lora_b"), vec![out, l.rank], true, Init::Normal(W_STD))?);
                Some(LoraOp { rank: l.rank, num: l.num, den: l.den })
            }
            _ => None,
        };
        Ok(bk.f(Op::Linear { bias, lora }, ins, out, site))
    }

    /// A norm under role `name` over `n` values in `groups` groups, gain shaped `gain_shape`
    /// (`[n]` full, `[n/groups]` shared by the groups, `[groups, n/groups]` per group).
    #[allow(clippy::too_many_arguments)]
    fn norm(
        &mut self,
        bk: &mut Bk,
        x: Ref,
        spec: NormSpec,
        name: &str,
        n: usize,
        groups: usize,
        gain_shape: Vec<usize>,
        per_layer: bool,
        site: &str,
    ) -> Result<Ref> {
        let mut ins = vec![x];
        if spec.gain != Gain::None {
            let init = if spec.gain == Gain::OnePlusW { Init::Uniform(-0.3, 0.3) } else { Init::Uniform(0.6, 1.4) };
            ins.push(self.param(&format!("{name}.gain"), gain_shape.clone(), per_layer, init)?);
        }
        if spec.bias {
            ins.push(self.param(&format!("{name}.bias"), gain_shape, per_layer, Init::Uniform(-0.1, 0.1))?);
        }
        Ok(bk.f(Op::Norm { spec, groups }, ins, n, site))
    }

    fn full_norm(&mut self, bk: &mut Bk, x: Ref, spec: NormSpec, name: &str, n: usize, per_layer: bool) -> Result<Ref> {
        self.norm(bk, x, spec, name, n, 1, vec![n], per_layer, name)
    }

    // ───────────────────────────── pre / post ─────────────────────────────

    fn pre_block(&mut self) -> Result<usize> {
        let s = self.s;
        let e = &s.embedding;
        let d = s.hidden_size;
        let mut bk = Bk { nodes: vec![] };
        let table = self.param("embed.table", vec![s.vocab_size, e.dim], false, Init::Normal(0.5))?;
        let mut x = bk.f(Op::Embedding, vec![Ref::Token, table], e.dim, "embed");
        // The width positions, token types and the embedding norm act at: the hidden width, or the table's when the
        // projection comes after the norm (ALBERT).
        let dw = if e.proj_in && e.proj_after_norm { e.dim } else { d };
        if e.proj_in && !e.proj_after_norm {
            x = self.linear(&mut bk, x, "embed.proj_in", d, e.dim, false, false, "embed.proj_in")?;
        }
        if let Some(lp) = &e.positions {
            let t = self.param("embed.pos_table", vec![lp.rows, dw], false, Init::Normal(0.2))?;
            let pv = bk.f(Op::PosEmbedding { offset: lp.offset }, vec![Ref::Pos, t], dw, "embed.pos_row");
            x = bk.f(Op::Add, vec![x, pv], dw, "embed.pos");
        }
        if e.scale != 1.0 {
            x = bk.f(Op::Scale { c: e.scale }, vec![x], dw, "embed.scaled");
        }
        if let Some(n) = e.norm {
            x = self.full_norm(&mut bk, x, n, "embed.norm", dw, false)?;
        }
        if e.proj_in && e.proj_after_norm {
            x = self.linear(&mut bk, x, "embed.proj_in", d, e.dim, e.proj_in_bias, false, "embed.proj_in")?;
        }
        // Hyper-connections: the embedding is repeated into every stream.
        if let Some(hy) = s.hyper.as_ref().filter(|h| h.streams > 1) {
            x = bk.st(Op::Concat, vec![x; hy.streams], d * hy.streams);
        }
        // Declared for the binding only: a bidirectional encoder's lowering (`lower::bidir`) folds
        // row 0 into its position rows; a per-position program never reads it.
        if let Some(rows) = e.type_rows {
            self.param("embed.type_table", vec![rows, dw], false, Init::Normal(0.2))?;
        }
        // Declared for the binding only, like the token types: `lower::bidir` adds it to the scores.
        if let Some(rb) = e.rel_bias {
            self.param("attn.rel_bias", vec![rb.buckets, rb.heads], false, Init::Normal(0.2))?;
        }
        // DeBERTa's relative-position table and its norm: declared for the binding only; `lower::bidir` reads them.
        if let Some(dis) = e.disentangled {
            self.param("rel.table", vec![2 * dis.span, d], false, Init::Normal(0.2))?;
            if let Some(n) = dis.norm {
                self.param("rel.norm.gain", vec![d], false, Init::Uniform(0.6, 1.4))?;
                if n.bias {
                    self.param("rel.norm.bias", vec![d], false, Init::Uniform(-0.1, 0.1))?;
                }
            }
        }
        let mut outputs = vec![x];
        for c in self.carries.clone().iter().skip(1) {
            let n: usize = c.shape.iter().product();
            outputs.push(bk.f(Op::Zeros, vec![], n, &format!("carry.{}", c.name)));
        }
        self.blocks.push(Block { name: "pre".into(), role: BlockRole::Pre, nodes: bk.nodes, outputs });
        Ok(self.blocks.len() - 1)
    }

    /// A layer block's outputs: the residual, then every other carry — what this block wrote to
    /// it, else the carry passed through.
    fn layer_outputs(&mut self, h: Ref) -> Vec<Ref> {
        let written = std::mem::take(&mut self.carry_out);
        let mut out = vec![h];
        out.extend((1..self.carries.len()).map(|c| written.get(&c).copied().unwrap_or(Ref::Carry(c as u8))));
        out
    }

    fn post_block(&mut self) -> Result<usize> {
        let s = self.s;
        let h = &s.head;
        let d = s.hidden_size;
        let mut bk = Bk { nodes: vec![] };
        let mut x = Ref::Carry(0);
        // Hyper-connections: one more gated mix brings the streams down to the hidden width (its norm
        // stands where the final norm would).
        if s.hyper.is_some() {
            x = self.hc_mix(&mut bk, x, "final", false, false)?.0;
        }
        if let Some(n) = s.final_norm {
            x = self.full_norm(&mut bk, x, n, "final_norm", d, false)?;
        }
        // An encoder: the final row, projected and normalised as the class says; no head.
        if let OutputSpec::Embedding { proj, normalize } = &s.output {
            if h.transform.is_some() {
                return Err(LowerError::not_lowerable("HEAD_TRANSFORM_V1 under an embedding output: the transform belongs to a language-model head"));
            }
            let mut width = d;
            if let Some((w, bias)) = *proj {
                x = self.linear(&mut bk, x, "embed.proj", w, d, bias, false, "embed.proj")?;
                width = w;
            }
            if *normalize {
                x = bk.f(Op::L2Norm { groups: 1, eps: 1e-24 }, vec![x], width, "embed.normed");
            }
            if !matches!(x, Ref::Node(..)) {
                // The output is the carry itself (a bidirectional encoder pools in its own `post`).
                x = bk.f(Op::Scale { c: 1.0 }, vec![x], width, "embed.out");
            }
            self.blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: bk.nodes, outputs: vec![x] });
            return Ok(self.blocks.len() - 1);
        }
        // `HEAD_TRANSFORM_V1`: dense, activation, norm — then the vocabulary projection.
        if let Some(t) = &h.transform {
            x = self.linear(&mut bk, x, "head.transform.dense", d, d, t.bias, false, "head.transform.dense")?;
            x = bk.f(Op::Act(t.act), vec![x], d, "head.transform.act");
            x = self.full_norm(&mut bk, x, t.norm, "head.transform.norm", d, false)?;
        }
        if h.pre_scale != 1.0 {
            x = bk.f(Op::Scale { c: h.pre_scale }, vec![x], d, "head.pre_scaled");
        }
        let mut width = d;
        if h.proj_out {
            width = s.embedding.dim;
            x = self.linear(&mut bk, x, "head.proj_out", width, d, false, false, "head.proj_out")?;
        }
        // A tied head IS the embedding table: one param used twice.
        let w = if h.tied {
            self.param("embed.table", vec![s.vocab_size, s.embedding.dim], false, Init::Normal(0.5))?
        } else {
            self.param("head.w", vec![s.vocab_size, width], false, Init::Normal(W_STD))?
        };
        let mut ins = vec![x, w];
        if h.bias {
            ins.push(self.param("head.b", vec![s.vocab_size], false, Init::Uniform(-0.1, 0.1))?);
        }
        let mut logits = bk.f(Op::Linear { bias: h.bias, lora: None }, ins, s.vocab_size, "logits");
        if h.logit_scale != 1.0 {
            logits = bk.f(Op::Scale { c: h.logit_scale }, vec![logits], s.vocab_size, "logits.scaled");
        }
        if let Some(c) = h.softcap {
            logits = bk.f(Op::Softcap { cap: c }, vec![logits], s.vocab_size, "logits.softcapped");
        }
        self.blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: bk.nodes, outputs: vec![logits] });
        Ok(self.blocks.len() - 1)
    }

    // ───────────────────────────── layers ─────────────────────────────

    /// The blocks one layer runs as, in order: one, or — a [`Residual::Sandwich`] layer — its
    /// mixer half and its FFN half (with the per-layer input and the layer scalar), the residual
    /// carried between them.
    fn layer_blocks(&mut self, ls: &LayerSpec, kind_index: usize) -> Result<Vec<usize>> {
        if let Residual::Sandwich { pre_mixer, post_mixer, .. } = &ls.residual {
            let d = self.s.hidden_size;
            let mut bk = Bk { nodes: vec![] };
            let x = Ref::Carry(0);
            let n1 = self.full_norm(&mut bk, x, *pre_mixer, "norm.mix", d, true)?;
            let m = self.mixer(&mut bk, &ls.mixer, n1)?;
            let m = self.full_norm(&mut bk, m, *post_mixer, "norm.post_mix", d, true)?;
            let h = bk.f(Op::Add, vec![x, m], d, "resid.mix");
            let name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
            let outputs = self.layer_outputs(h);
            self.blocks.push(Block { name: format!("{name}.mix"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
            let a = self.blocks.len() - 1;
            let f = self.layer_block(ls, kind_index)?;
            return Ok(vec![a, f]);
        }
        // DeepSeek sparse attention (`ATTN_TOKEN_INDEXER_V1`): the indexer's selection (about a hundred nodes) joins an MLA
        // mixer that is already a large block, and the layer's block would pass the normal form's 512 nodes. The layer
        // runs as its mixer half and its FFN half, the residual carried between them (as a sandwich layer does).
        if let Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } = &ls.residual
            && matches!(&ls.mixer, Mixer::Mla(m) if m.indexer.is_some())
            && ls.ffn != Ffn::None
        {
            let d = self.s.hidden_size;
            let name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
            let mut bk = Bk { nodes: vec![] };
            let x = Ref::Carry(0);
            let n1 = match pre_mixer {
                Some(n) => self.full_norm(&mut bk, x, *n, "norm.mix", d, true)?,
                None => x,
            };
            let mut m = self.mixer(&mut bk, &ls.mixer, n1)?;
            if let Some(n) = post_mixer {
                m = self.full_norm(&mut bk, m, *n, "norm.post_mix", d, true)?;
            }
            if *multiplier != 1.0 {
                m = bk.f(Op::Scale { c: *multiplier }, vec![m], d, "mix.scaled");
            }
            let h = bk.f(Op::Add, vec![x, m], d, "resid.mix");
            let outputs = self.layer_outputs(h);
            self.blocks.push(Block { name: format!("{name}.mix"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
            let a = self.blocks.len() - 1;
            let mut bk = Bk { nodes: vec![] };
            let h = Ref::Carry(0);
            let n2 = match pre_ffn {
                Some(n) => self.full_norm(&mut bk, h, *n, "norm.ffn", d, true)?,
                None => h,
            };
            let mut f = self.ffn(&mut bk, &ls.ffn, n2)?;
            if let Some(n) = post_ffn {
                f = self.full_norm(&mut bk, f, *n, "norm.post_ffn", d, true)?;
            }
            if *multiplier != 1.0 {
                f = bk.f(Op::Scale { c: *multiplier }, vec![f], d, "ffn.scaled");
            }
            let mut out = bk.f(Op::Add, vec![h, f], d, "resid.ffn");
            if ls.post_scale != 1.0 {
                out = bk.f(Op::Scale { c: ls.post_scale }, vec![out], d, "resid.rescaled");
            }
            let outputs = self.layer_outputs(out);
            self.blocks.push(Block { name: format!("{name}.ffn"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
            return Ok(vec![a, self.blocks.len() - 1]);
        }
        // A hyper-connection layer: the per-layer n-gram embedding (its own block when the layer has
        // one — it is as large as a mixer), the mixer half (the gated mix of the streams, the mixer,
        // the injection back) and the FFN half the same way.
        if let Residual::HyperConnection { ple } = &ls.residual {
            let name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
            let mut blocks = Vec::new();
            if let Some(p) = ple {
                let mut bk = Bk { nodes: vec![] };
                let h = self.ple_ngram(&mut bk, p, Ref::Carry(0))?;
                let outputs = self.layer_outputs(h);
                self.blocks.push(Block { name: format!("{name}.ple"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
                blocks.push(self.blocks.len() - 1);
            }
            let mut bk = Bk { nodes: vec![] };
            let x = Ref::Carry(0);
            let (mixed, inj) = self.hc_mix(&mut bk, x, "mix", true, true)?;
            let o = self.mixer(&mut bk, &ls.mixer, mixed)?;
            let h = self.hc_inject(&mut bk, x, o, inj.ok_or_else(|| LowerError::eval("internal: no injection weights"))?, "mix")?;
            let outputs = self.layer_outputs(h);
            self.blocks.push(Block { name: format!("{name}.mix"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
            blocks.push(self.blocks.len() - 1);
            blocks.push(self.layer_block(ls, kind_index)?);
            return Ok(blocks);
        }
        Ok(vec![self.layer_block(ls, kind_index)?])
    }

    fn layer_block(&mut self, ls: &LayerSpec, kind_index: usize) -> Result<usize> {
        let d = self.s.hidden_size;
        let mut bk = Bk { nodes: vec![] };
        let x = Ref::Carry(0);
        let mut h = match &ls.residual {
            Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } => {
                let n1 = match pre_mixer {
                    Some(n) => self.full_norm(&mut bk, x, *n, "norm.mix", d, true)?,
                    None => x,
                };
                let mut m = self.mixer(&mut bk, &ls.mixer, n1)?;
                if let Some(n) = post_mixer {
                    m = self.full_norm(&mut bk, m, *n, "norm.post_mix", d, true)?;
                }
                if *multiplier != 1.0 {
                    m = bk.f(Op::Scale { c: *multiplier }, vec![m], d, "mix.scaled");
                }
                let mut h = bk.f(Op::Add, vec![x, m], d, "resid.mix");
                if ls.ffn != Ffn::None {
                    let n2 = match pre_ffn {
                        Some(n) => self.full_norm(&mut bk, h, *n, "norm.ffn", d, true)?,
                        None => h,
                    };
                    let mut f = self.ffn(&mut bk, &ls.ffn, n2)?;
                    if let Some(n) = post_ffn {
                        f = self.full_norm(&mut bk, f, *n, "norm.post_ffn", d, true)?;
                    }
                    if *multiplier != 1.0 {
                        f = bk.f(Op::Scale { c: *multiplier }, vec![f], d, "ffn.scaled");
                    }
                    h = bk.f(Op::Add, vec![h, f], d, "resid.ffn");
                }
                h
            }
            Residual::Parallel { norm, ffn_norm } => {
                let n1 = self.full_norm(&mut bk, x, *norm, "norm.mix", d, true)?;
                let m = self.mixer(&mut bk, &ls.mixer, n1)?;
                let n2 = match ffn_norm {
                    Some(n) => self.full_norm(&mut bk, x, *n, "norm.ffn", d, true)?,
                    None => n1,
                };
                let f = self.ffn(&mut bk, &ls.ffn, n2)?;
                let t = bk.f(Op::Add, vec![m, f], d, "resid.branches");
                bk.f(Op::Add, vec![x, t], d, "resid")
            }
            Residual::PostNorm { mixer_norm, ffn_norm } => {
                let m = self.mixer(&mut bk, &ls.mixer, x)?;
                let h = bk.f(Op::Add, vec![x, m], d, "resid.mix");
                let h = self.full_norm(&mut bk, h, *mixer_norm, "norm.mix", d, true)?;
                let f = self.ffn(&mut bk, &ls.ffn, h)?;
                let h = bk.f(Op::Add, vec![h, f], d, "resid.ffn");
                self.full_norm(&mut bk, h, *ffn_norm, "norm.ffn", d, true)?
            }
            // The FFN half: the mixer half ([`Builder::layer_blocks`]) carries in `x + mix`.
            Residual::Sandwich { pre_ffn, post_ffn, ple, layer_scalar, .. } => {
                let x1 = x;
                let f = match &ls.ffn {
                    Ffn::None => return Err(LowerError::eval("internal: a sandwich layer without an FFN")),
                    // Gemma-4's MoE block beside the MLP: the router and the experts read the residual.
                    Ffn::MlpMoe(mm) => {
                        let n2 = self.full_norm(&mut bk, x1, *pre_ffn, "norm.ffn", d, true)?;
                        let a = self.mlp(&mut bk, &mm.mlp, n2, mm.mlp.name.as_deref().unwrap_or("mlp"))?;
                        let a = self.full_norm(&mut bk, a, mm.mlp_post, "norm.post_mlp", d, true)?;
                        let rn = self.full_norm(&mut bk, x1, mm.router_norm, "moe.router_norm", d, true)?;
                        let rn = bk.f(Op::Scale { c: mm.router_scale }, vec![rn], d, "moe.router_in");
                        let en = self.full_norm(&mut bk, x1, mm.moe_pre, "norm.moe", d, true)?;
                        let e = self.moe_split(&mut bk, &mm.moe, rn, en)?;
                        let e = self.full_norm(&mut bk, e, mm.moe_post, "norm.post_moe", d, true)?;
                        bk.f(Op::Add, vec![a, e], d, "ffn.sum")
                    }
                    other => {
                        let n2 = self.full_norm(&mut bk, x1, *pre_ffn, "norm.ffn", d, true)?;
                        self.ffn(&mut bk, other, n2)?
                    }
                };
                let f = self.full_norm(&mut bk, f, *post_ffn, "norm.post_ffn", d, true)?;
                let mut h = bk.f(Op::Add, vec![x1, f], d, "resid.ffn");
                if let Some(p) = ple {
                    let pv = self.ple(&mut bk, p)?;
                    let g = self.linear(&mut bk, h, "ple.gate", p.dim, d, false, true, "ple.gate")?;
                    let g = bk.f(Op::Act(p.act), vec![g], p.dim, "ple.act");
                    let gm = bk.f(Op::Mul, vec![g, pv], p.dim, "ple.gated");
                    let o = self.linear(&mut bk, gm, "ple.out", d, p.dim, false, true, "ple.out")?;
                    let o = self.full_norm(&mut bk, o, p.post_norm, "ple.post_norm", d, true)?;
                    h = bk.f(Op::Add, vec![h, o], d, "resid.ple");
                }
                if *layer_scalar {
                    let sp = self.param("layer.scalar", vec![1], true, Init::Uniform(0.8, 1.2))?;
                    h = bk.f(Op::ScaleParam, vec![h, sp], d, "resid.scaled");
                }
                h
            }
            // The FFN half: the mixer half ([`Builder::layer_blocks`]) carries in the streams.
            Residual::HyperConnection { .. } => {
                if ls.ffn == Ffn::None {
                    return Err(LowerError::eval("internal: a hyper-connection layer without an FFN"));
                }
                let (mixed, inj) = self.hc_mix(&mut bk, x, "ffn", true, true)?;
                let f = self.ffn(&mut bk, &ls.ffn, mixed)?;
                self.hc_inject(&mut bk, x, f, inj.ok_or_else(|| LowerError::eval("internal: no injection weights"))?, "ffn")?
            }
        };
        if ls.post_scale != 1.0 {
            h = bk.f(Op::Scale { c: ls.post_scale }, vec![h], d, "resid.rescaled");
        }
        let mut name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
        if matches!(ls.residual, Residual::Sandwich { .. } | Residual::HyperConnection { .. }) {
            name.push_str(".ffn");
        }
        let outputs = self.layer_outputs(h);
        self.blocks.push(Block { name, role: BlockRole::Layer, nodes: bk.nodes, outputs });
        Ok(self.blocks.len() - 1)
    }

    fn mixer(&mut self, bk: &mut Bk, m: &Mixer, x: Ref) -> Result<Ref> {
        match m {
            Mixer::Attention(a) => self.attention(bk, a, x),
            Mixer::Mla(a) => self.mla(bk, a, x),
            Mixer::GatedDeltaNet(g) => self.gdn(bk, g, x),
            Mixer::Mamba(mm) => self.mamba(bk, mm, x),
            Mixer::Mamba2(mm) => self.mamba2(bk, mm, x),
            Mixer::RwkvTime(r) => self.rwkv_time(bk, r, x),
        }
    }

    fn ffn(&mut self, bk: &mut Bk, f: &Ffn, x: Ref) -> Result<Ref> {
        match f {
            Ffn::None => Err(LowerError::eval("internal: ffn of a layer without one")),
            Ffn::Mlp(m) => self.mlp(bk, m, x, m.name.as_deref().unwrap_or("mlp")),
            Ffn::Moe(m) => self.moe(bk, m, x),
            Ffn::RwkvChannel(c) => self.rwkv_channel(bk, c, x),
            Ffn::MlpMoe(_) => Err(LowerError::eval("internal: an MLP+MoE block outside a sandwich layer")),
        }
    }

    /// Gemma-3n/4's per-layer input of this layer ([`PleSpec`]), from the token: the token's
    /// scaled embedding projected by the layer's slice and normed, plus the layer's own table row.
    fn ple(&mut self, bk: &mut Bk, p: &PleSpec) -> Result<Ref> {
        let s = self.s;
        let d = s.hidden_size;
        let table = self.param("embed.table", vec![s.vocab_size, s.embedding.dim], false, Init::Normal(0.5))?;
        let mut e = bk.f(Op::Embedding, vec![Ref::Token, table], s.embedding.dim, "ple.embed");
        if s.embedding.scale != 1.0 {
            e = bk.f(Op::Scale { c: s.embedding.scale }, vec![e], d, "ple.embed_scaled");
        }
        let pr = self.linear(bk, e, "ple.proj", p.dim, d, false, true, "ple.proj")?;
        let pr = bk.f(Op::Scale { c: p.proj_scale }, vec![pr], p.dim, "ple.proj_scaled");
        let pr = self.full_norm(bk, pr, p.norm, "ple.norm", p.dim, false)?;
        let tt = self.param("ple.table", vec![p.vocab, p.dim], true, Init::Normal(0.5))?;
        let te = bk.f(Op::Embedding, vec![Ref::Token, tt], p.dim, "ple.token");
        let te = bk.f(Op::Scale { c: p.table_scale }, vec![te], p.dim, "ple.token_scaled");
        let sum = bk.f(Op::Add, vec![pr, te], p.dim, "ple.sum");
        Ok(bk.f(Op::Scale { c: p.combine_scale }, vec![sum], p.dim, "ple"))
    }

    // ───────────────────────────── hyper-connections ─────────────────────────────

    fn hyper(&self) -> Result<HyperSpec> {
        self.s
            .hyper
            .clone()
            .ok_or_else(|| LowerError::not_lowerable("RESIDUAL_GATED_HC_V1: a hyper-connection layer needs the model's streams (ModelSpec::hyper)"))
    }

    /// One gated mix of the residual's streams (`hc.{tag}.*`, `RESIDUAL_GATED_HC_V1`): the streams
    /// are normed per stream, a low-rank pair of projections with a SiLU between them (and a
    /// sigmoid after) gates them, and the mean over the streams is the block's input `[D]`. With
    /// `inject`, a third projection gives the weights `2σ(·/S)` each stream takes of the block's
    /// output (`hc_inject`).
    fn hc_mix(&mut self, bk: &mut Bk, x: Ref, tag: &str, inject: bool, per_layer: bool) -> Result<(Ref, Option<Ref>)> {
        let hy = self.hyper()?;
        let (s, d, r) = (hy.streams, self.s.hidden_size, hy.lowrank);
        let n = s * d;
        let t = |part: &str| format!("hc.{tag}.{part}");
        let normed = self.norm(bk, x, hy.norm, &t("norm"), n, s, vec![n], per_layer, &t("norm"))?;
        let down = self.linear(bk, normed, &t("down"), r, n, false, per_layer, &t("down"))?;
        let down = bk.f(Op::Scale { c: 1.0 / s as f64 }, vec![down], r, &t("down_scaled"));
        let act = bk.f(Op::Act(Act::Silu), vec![down], r, &t("silu"));
        let up = self.linear(bk, act, &t("up"), n, r, false, per_layer, &t("up"))?;
        let gate = bk.f(Op::Act(Act::Sigmoid), vec![up], n, &t("gate"));
        let gated = bk.f(Op::Mul, vec![gate, normed], n, &t("gated"));
        let mixed = bk.f(Op::StreamMean { streams: s }, vec![gated], d, &t("mixed"));
        let inj = if inject {
            let j = self.linear(bk, normed, &t("inject"), s, n, false, per_layer, &t("inject"))?;
            let j = bk.f(Op::Scale { c: 1.0 / s as f64 }, vec![j], s, &t("inject_scaled"));
            let j = bk.f(Op::Act(Act::Sigmoid), vec![j], s, &t("inject_sigmoid"));
            Some(bk.f(Op::Scale { c: 2.0 }, vec![j], s, &t("inject_w")))
        } else {
            None
        };
        Ok((mixed, inj))
    }

    /// The block's output `o [D]` joins every stream with its own weight: `x + o ⊗ w`.
    fn hc_inject(&mut self, bk: &mut Bk, x: Ref, o: Ref, w: Ref, tag: &str) -> Result<Ref> {
        let s = self.hyper()?.streams;
        let n = s * self.s.hidden_size;
        let delta = bk.f(Op::StreamOuter { streams: s }, vec![o, w], n, &format!("hc.{tag}.delta"));
        Ok(bk.f(Op::Add, vec![x, delta], n, &format!("resid.hc.{tag}")))
    }

    // ───────────────────────────── hashed n-gram per-layer embedding ─────────────────────────────

    /// `EMBED_NGRAM_PLE_V1`: this layer's hashed n-gram rows project to a key per stream and a shared
    /// value; the normed streams gate the value (`σ(signed √(k·q / √D))`), a dilated depthwise
    /// causal convolution adds local context, and the result joins every stream. Returns the streams
    /// `x + ple(x)`.
    fn ple_ngram(&mut self, bk: &mut Bk, p: &NgramPleSpec, x: Ref) -> Result<Ref> {
        let hy = self.hyper()?;
        let (s, d) = (hy.streams, self.s.hidden_size);
        let n = s * d;
        let heads = p.ngram_size.saturating_sub(1).saturating_mul(p.heads_per_ngram);
        if heads == 0 || p.embed_dim % heads != 0 || p.ngram_size < 2 {
            return Err(LowerError::bad(format!("EMBED_NGRAM_PLE_V1: {} embedding width over {heads} hash heads", p.embed_dim)));
        }
        let dpn = p.embed_dim / heads;
        // What a program can declare: the hash is `~n·63` nodes, every head is a table, and each head size is a
        // prime searched by trial division from the layer's position in the sequence of primes.
        let deepest = self
            .s
            .layers
            .iter()
            .filter_map(|l| match &l.residual {
                Residual::HyperConnection { ple: Some(q) } => Some(q.layer_index),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        if p.ngram_size > 8
            || heads > 1024
            || (deepest + 1).saturating_mul(heads) > 65_536
            || p.vocab_base as u64 >= 1 << 32
            || p.conv_kernel == 0
            || p.conv_kernel > 64
            || p.conv_dilation == 0
            || (p.conv_kernel - 1).saturating_mul(p.conv_dilation) > 4096
        {
            return Err(LowerError::not_lowerable(format!(
                "EMBED_NGRAM_PLE_V1: {}-grams over {heads} hash heads at {} layers deep, tables from {}, a convolution of {} taps dilated {} — past what a program declares (n ≤ 8, ≤ 1,024 heads, ≤ 65,536 primes, tables under 2^32 rows, ≤ 64 taps over ≤ 4,096 rows)",
                p.ngram_size,
                deepest + 1,
                p.vocab_base,
                p.conv_kernel,
                p.conv_dilation
            )));
        }
        // The table rows differ by layer (the head sizes are consecutive primes): the block's param
        // is as tall as the tallest, and a layer's rows past its own are never read.
        let rows_max = self
            .s
            .layers
            .iter()
            .filter_map(|l| match &l.residual {
                Residual::HyperConnection { ple: Some(q) } => Some(crate::ngram::NgramTables::new(q).padded_vocab as usize),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let win = self.state("ple.ngram.win", StateKind::Fixed, vec![p.ngram_size - 1], p.eos_id as f32)?;
        let ids = Ref::Node(
            bk.push(Op::NgramIds { ple: p.clone(), layers: vec![] }, vec![Ref::Token, win], vec![vec![heads]], vec![HlType::Idx], None, vec![sid(win)]),
            0,
        );
        let table = self.param("ple.ngram.table", vec![rows_max, dpn], true, Init::Normal(0.5))?;
        let rows = bk.f(Op::GatherRows { heads, dim: dpn }, vec![ids, table], heads * dpn, "ple.rows");
        let key = self.linear(bk, rows, "ple.key", n, p.embed_dim, false, true, "ple.key")?;
        let key = self.norm(bk, key, p.norm, "ple.norm_key", n, s, vec![n], true, "ple.norm_key")?;
        let val = self.linear(bk, rows, "ple.value", d, p.embed_dim, false, true, "ple.value")?;
        let qry = self.norm(bk, x, p.norm, "ple.norm_query", n, s, vec![n], true, "ple.norm_query")?;
        let dot = bk.f(Op::GroupDot { groups: s }, vec![key, qry], s, "ple.dot");
        let dot = bk.f(Op::Scale { c: 1.0 / (d as f64).sqrt() }, vec![dot], s, "ple.dot_scaled");
        let g = bk.f(Op::Act(Act::SignedSqrt), vec![dot], s, "ple.gate_sqrt");
        let g = bk.f(Op::Act(Act::Sigmoid), vec![g], s, "ple.gate");
        let gv = bk.f(Op::StreamOuter { streams: s }, vec![val, g], n, "ple.gated");
        let gvn = self.norm(bk, gv, p.norm, "ple.norm_conv", n, s, vec![n], true, "ple.norm_conv")?;
        let conv = self.conv_dilated(bk, gvn, n, p.conv_kernel, p.conv_dilation, "ple.conv")?;
        let out = bk.f(Op::Add, vec![gv, conv], n, "ple.out");
        Ok(bk.f(Op::Add, vec![x, out], n, "resid.ple"))
    }

    /// [`Builder::conv`] with taps `dilation` positions apart (the state keeps `(kernel − 1)·dilation` rows).
    fn conv_dilated(&mut self, bk: &mut Bk, x: Ref, ch: usize, kernel: usize, dilation: usize, name: &str) -> Result<Ref> {
        let w = self.param(&format!("{name}.w"), vec![ch, kernel], true, Init::Normal(0.3))?;
        let st = self.state(&format!("{name}.window"), StateKind::Fixed, vec![kernel.saturating_sub(1) * dilation, ch], 0.0)?;
        Ok(bk.fw(
            Op::CausalConv1d { channels: ch, kernel, bias: false, act: Some(Act::Silu), dilation },
            vec![x, st, w],
            ch,
            Some(name),
            vec![sid(st)],
        ))
    }

    // ───────────────────────────── sparse block attention ─────────────────────────────

    /// The token indexer of `ATTN_SPARSE_BLOCK_V1`: queries from `x`, one pooled key per block of
    /// `ratio` positions (the mean of the block's raw keys, normed, rotated at the block's first
    /// position), block scores, and the ids of the `top` best blocks.
    fn sparse_indexer(&mut self, bk: &mut Bk, sp: &SparseBlockSpec, rp: &crate::rope::RopeSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (n, dim, ratio) = (sp.index_heads, sp.index_dim, sp.ratio);
        if n == 0 || dim == 0 || ratio == 0 || sp.top_blocks == 0 || rp.rotary_dim > dim {
            return Err(LowerError::bad(format!("ATTN_SPARSE_BLOCK_V1: {n} index heads of {dim}, blocks of {ratio}, rotary {}", rp.rotary_dim)));
        }
        let max_pos = self
            .s
            .max_position_embeddings
            .ok_or_else(|| LowerError::not_lowerable("ATTN_SPARSE_BLOCK_V1 needs max_position_embeddings (the block-key matrix is sized on it)"))?;
        let blocks = max_pos.div_ceil(ratio);
        // The block-key matrix is a state and the scores are `[blocks, index heads]`: both inside NF-8's 2^28 elements.
        if (blocks as u64).saturating_mul(dim.max(n) as u64) > 1 << 27 {
            return Err(LowerError::not_lowerable(format!(
                "ATTN_SPARSE_BLOCK_V1: {blocks} blocks of {dim} (and {n} index heads) are past the 2^27 elements a program keeps as block keys and scores; use blocks of more positions or a shorter context"
            )));
        }
        let top = sp.top_blocks.min(blocks);
        let t = self.rope_table(&rp.freqs);
        let qk = self.linear(bk, x, "attn.idx.qk", (n + 1) * dim, d, false, true, "attn.idx.qk")?;
        let q = bk.st(Op::Slice { start: 0, len: n * dim }, vec![qk], n * dim);
        let kraw = bk.st(Op::Slice { start: n * dim, len: dim }, vec![qk], dim);
        let q = self.norm(bk, q, sp.q_norm, "attn.idx.q_norm", n * dim, n, vec![dim], true, "attn.idx.q_norm")?;
        let q = bk.f(
            Op::Rope { heads: n, head_dim: dim, rotary_dim: rp.rotary_dim, offset: 0, style: rp.style, table: t },
            vec![q, Ref::Pos],
            n * dim,
            "attn.idx.q_rope",
        );
        let sum = self.state("attn.idx.sum", StateKind::Fixed, vec![dim], 0.0)?;
        let pooled = bk.fw(Op::BlockMean { ratio }, vec![kraw, sum], dim, Some("attn.idx.pooled"), vec![sid(sum)]);
        let kn = self.norm(bk, pooled, sp.k_norm, "attn.idx.k_norm", dim, 1, vec![dim], true, "attn.idx.k_norm")?;
        let kb = bk.f(
            Op::RopeAtBlock { heads: 1, head_dim: dim, rotary_dim: rp.rotary_dim, offset: 0, style: rp.style, table: t, ratio },
            vec![kn, Ref::Pos],
            dim,
            "attn.idx.k_rope",
        );
        let keys = self.state("attn.idx.keys", StateKind::Fixed, vec![blocks, dim], 0.0)?;
        let ids = Ref::Node(
            bk.push(
                Op::BlockSelect { heads: n, dim, ratio, blocks, top },
                vec![q, kb, keys, Ref::Pos],
                vec![vec![top]],
                vec![HlType::Idx],
                None,
                vec![],
            ),
            0,
        );
        bk.push(Op::BlockWrite { ratio, blocks }, vec![kb, keys, Ref::Pos], vec![vec![]], vec![HlType::Unit], None, vec![sid(keys)]);
        Ok(ids)
    }

    // ───────────────────────────── attention ─────────────────────────────

    fn attention(&mut self, bk: &mut Bk, a: &AttnSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (h, kv, hd, vd) = (a.heads, a.kv_heads, a.head_dim, a.v_head_dim);
        let (qn, kn, vn) = (h * hd, kv * hd, kv * vd);
        let pf = a.param_prefix.clone().unwrap_or_else(|| "attn".into());
        let n = |s: &str| format!("{pf}.{s}");
        let mut q = self.linear(bk, x, &n("q"), qn, d, a.q_bias, true, &n("q"))?;
        // A KV-sharing layer: the query as ever, the keys and values an earlier layer's rows.
        if let Some(KvShare::Consumer { slot }) = a.kv_share {
            if a.output_gate || a.gate.is_some() || a.clip_qkv.is_some() || a.sinks || a.v_from_k {
                return Err(LowerError::not_lowerable("a KV-sharing layer with a gated, clipped, sinked or K = V attention"));
            }
            if let Some(qk) = &a.qk_norm
                && !a.qk_norm_after_rope
            {
                q = self.qk_norm(bk, q, qk, &n("q_norm"), h, hd)?;
            }
            match &a.position {
                Position::Rope(r) => {
                    let t = self.rope_table(&r.freqs);
                    q = bk.f(
                        Op::Rope { heads: h, head_dim: hd, rotary_dim: r.rotary_dim, offset: r.offset, style: r.style, table: t },
                        vec![q, Ref::Pos],
                        qn,
                        &n("q_rope"),
                    );
                }
                Position::None => {}
                Position::Alibi(_) => return Err(LowerError::not_lowerable("a KV-sharing layer under ALiBi")),
            }
            if let Some(qk) = &a.qk_norm
                && a.qk_norm_after_rope
            {
                q = self.qk_norm(bk, q, qk, &n("q_norm"), h, hd)?;
            }
            let (kc, vc) = (Ref::Carry((1 + 2 * slot) as u8), Ref::Carry((2 + 2 * slot) as u8));
            let suffix = a.window.map(|w| format!(".w{w}")).unwrap_or_default();
            let ks = self.state(&format!("{pf}.k_hist{suffix}"), StateKind::Hist { window: a.window }, vec![kn], 0.0)?;
            let vs = self.state(&format!("{pf}.v_hist{suffix}"), StateKind::Hist { window: a.window }, vec![vn], 0.0)?;
            bk.append(kc, ks);
            bk.append(vc, vs);
            let op = Op::Attention {
                heads: h,
                kv_heads: kv,
                head_dim: hd,
                v_head_dim: vd,
                scale: a.scale,
                softcap: a.softcap,
                window: a.window,
                alibi: None,
                sinks: false,
                chunk: a.chunk,
                blocks: None,
            };
            let o = bk.f(op, vec![q, ks, vs], h * vd, &n("ctx"));
            return self.linear(bk, o, &n("o"), d, h * vd, a.o_bias, true, &n("out"));
        }
        let mut k = self.linear(bk, x, &n("k"), kn, d, a.k_bias, true, &n("k"))?;
        // Gemma-4's `attention_k_eq_v`: the values are the raw key projection.
        let mut v = if a.v_from_k {
            if vd != hd {
                return Err(LowerError::eval("internal: values from keys of another width"));
            }
            k
        } else {
            self.linear(bk, x, &n("v"), vn, d, a.v_bias, true, &n("v"))?
        };
        let gate = if a.output_gate { Some(self.linear(bk, x, &n("gate"), qn, d, false, true, &n("gate"))?) } else { None };
        // `ATTN_VALUE_SCALE_V1`: a constant on the values (it joins the value narrowing's multiplier in the lowering).
        if a.v_scale != 1.0 {
            v = bk.f(Op::Scale { c: a.v_scale }, vec![v], vn, &n("v_scaled"));
        }
        // `ATTN_OUTPUT_GATE_SEPARATE_V1`: `act(gate_proj(x))`, per element or per head, from a projection of its own.
        let sep_gate = match &a.gate {
            Some(_) if a.output_gate => {
                return Err(LowerError::not_lowerable("ATTN_OUTPUT_GATE_SEPARATE_V1 beside the fused (q_proj) output gate"));
            }
            Some(g) => {
                let width = if g.per_head { h } else { h * vd };
                let gl = self.linear(bk, x, &n("gate"), width, d, false, true, &n("gate"))?;
                let ga = bk.f(Op::Act(g.act), vec![gl], width, &n("gate_act"));
                Some(if g.per_head { bk.f(Op::GroupRepeat { groups: h, size: vd }, vec![ga], h * vd, &n("gate_rep")) } else { ga })
            }
            None => None,
        };
        if let Some(c) = a.clip_qkv {
            q = bk.f(Op::Clamp { lo: -c, hi: c }, vec![q], qn, &n("q_clip"));
            k = bk.f(Op::Clamp { lo: -c, hi: c }, vec![k], kn, &n("k_clip"));
            v = bk.f(Op::Clamp { lo: -c, hi: c }, vec![v], vn, &n("v_clip"));
        }
        if let Some(qk) = &a.qk_norm
            && !a.qk_norm_after_rope
        {
            q = self.qk_norm(bk, q, qk, &n("q_norm"), h, hd)?;
            k = self.qk_norm(bk, k, qk, &n("k_norm"), kv, hd)?;
        }
        if let Some(vn_) = &a.v_norm {
            v = self.qk_norm(bk, v, vn_, &n("v_norm"), kv, vd)?;
        }
        let mut alibi = None;
        match &a.position {
            Position::Rope(r) => {
                let t = self.rope_table(&r.freqs);
                q = bk.f(
                    Op::Rope { heads: h, head_dim: hd, rotary_dim: r.rotary_dim, offset: r.offset, style: r.style, table: t },
                    vec![q, Ref::Pos],
                    qn,
                    &n("q_rope"),
                );
                k = bk.f(
                    Op::Rope { heads: kv, head_dim: hd, rotary_dim: r.rotary_dim, offset: r.offset, style: r.style, table: t },
                    vec![k, Ref::Pos],
                    kn,
                    &n("k_rope"),
                );
            }
            Position::Alibi(al) => alibi = Some(al.clone()),
            Position::None => {}
        }
        // Hunyuan: the per-head norms act on the rotated q and k (`ATTN_QK_NORM_POST_ROPE_V1`).
        if let Some(qk) = &a.qk_norm
            && a.qk_norm_after_rope
        {
            q = self.qk_norm(bk, q, qk, &n("q_norm"), h, hd)?;
            k = self.qk_norm(bk, k, qk, &n("k_norm"), kv, hd)?;
        }
        if let Some(t) = a.q_temperature {
            q = bk.f(Op::PosScale { temp: t }, vec![q, Ref::Pos], qn, &n("q_temp"));
        }
        let suffix = a.window.map(|w| format!(".w{w}")).unwrap_or_default();
        let ks = self.state(&format!("{pf}.k_hist{suffix}"), StateKind::Hist { window: a.window }, vec![kn], 0.0)?;
        let vs = self.state(&format!("{pf}.v_hist{suffix}"), StateKind::Hist { window: a.window }, vec![vn], 0.0)?;
        bk.append(k, ks);
        bk.append(v, vs);
        // The source of a KV slot: its rows, as appended, go out to the sharing layers.
        if let Some(KvShare::Source { slot }) = a.kv_share {
            self.carry_out.insert(1 + 2 * slot, k);
            self.carry_out.insert(2 + 2 * slot, v);
        }
        let mut ins = vec![q, ks, vs];
        if a.sinks {
            ins.push(self.param(&n("sinks"), vec![h], true, Init::Normal(1.0))?);
        }
        // Sparse block attention: the indexer picks the key blocks this query reads.
        let mut blocks = None;
        if let Some(sp) = &a.sparse {
            let Position::Rope(rp) = &a.position else {
                return Err(LowerError::not_lowerable("ATTN_SPARSE_BLOCK_V1: the indexer rotates by the attention's rope"));
            };
            if a.window.is_some() || a.chunk.is_some() || alibi.is_some() || a.softcap.is_some() || a.kv_share.is_some() {
                return Err(LowerError::not_lowerable(
                    "ATTN_SPARSE_BLOCK_V1 over a window, chunks, ALiBi, a soft-cap or shared keys is not modelled",
                ));
            }
            ins.push(self.sparse_indexer(bk, sp, rp, x)?);
            blocks = Some(sp.ratio);
        }
        let op = Op::Attention {
            heads: h,
            kv_heads: kv,
            head_dim: hd,
            v_head_dim: vd,
            scale: a.scale,
            softcap: a.softcap,
            window: a.window,
            alibi,
            sinks: a.sinks,
            chunk: a.chunk,
            blocks,
        };
        let mut o = bk.f(op, ins, h * vd, &n("ctx"));
        if let Some(sg) = sep_gate {
            o = bk.f(Op::Mul, vec![o, sg], h * vd, &n("gated"));
        }
        if let Some(g) = gate {
            let s = bk.f(Op::Act(Act::Sigmoid), vec![g], qn, &n("gate_act"));
            o = bk.f(Op::Mul, vec![o, s], qn, &n("gated"));
        }
        self.linear(bk, o, &n("o"), d, h * vd, a.o_bias, true, &n("out"))
    }

    fn qk_norm(&mut self, bk: &mut Bk, x: Ref, qk: &QkNorm, name: &str, heads: usize, hd: usize) -> Result<Ref> {
        let n = heads * hd;
        match qk.scope {
            QkNormScope::Whole => self.norm(bk, x, qk.norm, name, n, 1, vec![n], true, name),
            QkNormScope::PerHeadShared => self.norm(bk, x, qk.norm, name, n, heads, vec![hd], true, name),
            QkNormScope::PerHeadSeparate => self.norm(bk, x, qk.norm, name, n, heads, vec![heads, hd], true, name),
        }
    }

    // ───────────────────────────── MLA ─────────────────────────────

    fn mla(&mut self, bk: &mut Bk, a: &MlaSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (h, nope, rope, vd, r) = (a.heads, a.qk_nope_head_dim, a.qk_rope_head_dim, a.v_head_dim, a.kv_lora_rank);
        let qd = nope + rope;
        let mut qlatent = None;
        let q = match a.q_lora_rank {
            Some(qr) => {
                let qa = self.linear(bk, x, "mla.q_a", qr, d, a.a_bias, true, "mla.q_a")?;
                let qa = self.full_norm(bk, qa, a.q_a_norm, "mla.q_a_norm", qr, true)?;
                qlatent = Some((qa, qr));
                self.linear(bk, qa, "mla.q_b", h * qd, qr, false, true, "mla.q")?
            }
            None => self.linear(bk, x, "mla.q", h * qd, d, false, true, "mla.q")?,
        };
        let t = self.rope_table(&a.rope.freqs);
        let q = bk.f(
            Op::Rope { heads: h, head_dim: qd, rotary_dim: rope, offset: nope, style: a.rope.style, table: t },
            vec![q, Ref::Pos],
            h * qd,
            "mla.q_rope",
        );
        let c = self.linear(bk, x, "mla.kv_a.latent", r, d, a.a_bias, true, "mla.kv_a")?;
        let c = self.norm(bk, c, a.kv_a_norm, "mla.kv_a_norm", r, 1, vec![r], true, "mla.latent")?;
        let kr = self.linear(bk, x, "mla.kv_a.rope", rope, d, a.a_bias, true, "mla.k_rope_in")?;
        let kr = bk.f(
            Op::Rope { heads: 1, head_dim: rope, rotary_dim: rope, offset: 0, style: a.rope.style, table: t },
            vec![kr, Ref::Pos],
            rope,
            "mla.k_rope",
        );
        let ls = self.state("mla.latent_hist", StateKind::Hist { window: None }, vec![r], 0.0)?;
        let rs = self.state("mla.k_rope_hist", StateKind::Hist { window: None }, vec![rope], 0.0)?;
        bk.append(c, ls);
        bk.append(kr, rs);
        let kvb = self.param("mla.kv_b.w", vec![h * (nope + vd), r], true, Init::Normal(W_STD))?;
        // DeepSeek sparse attention (`ATTN_TOKEN_INDEXER_V1`): the indexer's query from the MLA's q-latent, its key from
        // the layer input, its head weights, and the history of its keys.
        let mut indexer = None;
        let mut extra: Vec<Ref> = Vec::new();
        if let Some(ix) = &a.indexer {
            let Some((qa, qr)) = qlatent else {
                return Err(LowerError::not_lowerable("a token indexer needs the MLA's q-latent (q_lora_rank): its query is read from it"));
            };
            let (hi, di) = (ix.heads, ix.head_dim);
            if ix.rope.offset != 0 || ix.rope.rotary_dim > di || ix.rope.rotary_dim == 0 {
                return Err(LowerError::not_lowerable("a token indexer's rotation must cover the first lanes of its head (offset 0, 0 < rotary_dim <= head_dim)"));
            }
            let it = self.rope_table(&ix.rope.freqs);
            let iq = self.linear(bk, qa, "mla.idx.q", hi * di, qr, false, true, "mla.idx.q")?;
            let iq = bk.f(
                Op::Rope { heads: hi, head_dim: di, rotary_dim: ix.rope.rotary_dim, offset: 0, style: ix.rope.style, table: it },
                vec![iq, Ref::Pos],
                hi * di,
                "mla.idx.q_rope",
            );
            let ik = self.linear(bk, x, "mla.idx.k", di, d, false, true, "mla.idx.k")?;
            let ik = self.norm(bk, ik, ix.k_norm, "mla.idx.k_norm", di, 1, vec![di], true, "mla.idx.k_normed")?;
            let ik = bk.f(
                Op::Rope { heads: 1, head_dim: di, rotary_dim: ix.rope.rotary_dim, offset: 0, style: ix.rope.style, table: it },
                vec![ik, Ref::Pos],
                di,
                "mla.idx.k_rope",
            );
            let iw = self.linear(bk, x, "mla.idx.wp", hi, d, false, true, "mla.idx.wp")?;
            let ih = self.state("mla.idx.k_hist", StateKind::Hist { window: None }, vec![di], 0.0)?;
            bk.append(ik, ih);
            indexer = Some(TokenIndexDims { heads: hi, dim: di, topk: ix.topk });
            extra = vec![iq, iw, ih];
        }
        let mut ins = vec![q, ls, rs, kvb];
        ins.extend(extra);
        let o = bk.f(
            Op::MlaAttention { heads: h, nope, rope, v_dim: vd, kv_lora: r, scale: a.scale, indexer },
            ins,
            h * vd,
            "mla.ctx",
        );
        self.linear(bk, o, "mla.o", d, h * vd, false, true, "mla.out")
    }

    // ───────────────────────────── GDN ─────────────────────────────

    fn gdn(&mut self, bk: &mut Bk, g: &GdnSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (nk, nv, dk, dv) = (g.k_heads, g.v_heads, g.k_dim, g.v_dim);
        let q = self.linear(bk, x, "gdn.q", nk * dk, d, false, true, "gdn.q")?;
        let k = self.linear(bk, x, "gdn.k", nk * dk, d, false, true, "gdn.k")?;
        let v = self.linear(bk, x, "gdn.v", nv * dv, d, false, true, "gdn.v")?;
        let z = self.linear(bk, x, "gdn.z", nv * dv, d, false, true, "gdn.z")?;
        let b = self.linear(bk, x, "gdn.b", nv, d, false, true, "gdn.b")?;
        let a = self.linear(bk, x, "gdn.a", nv, d, false, true, "gdn.a")?;
        let ch = 2 * nk * dk + nv * dv;
        let qkv = bk.st(Op::Concat, vec![q, k, v], ch);
        let c = self.conv(bk, qkv, ch, g.conv_kernel, false, "gdn.conv")?;
        let q = bk.st(Op::Slice { start: 0, len: nk * dk }, vec![c], nk * dk);
        let k = bk.st(Op::Slice { start: nk * dk, len: nk * dk }, vec![c], nk * dk);
        let v = bk.st(Op::Slice { start: 2 * nk * dk, len: nv * dv }, vec![c], nv * dv);
        let q = bk.f(Op::L2Norm { groups: nk, eps: g.l2_eps }, vec![q], nk * dk, "gdn.q_l2");
        let k = bk.f(Op::L2Norm { groups: nk, eps: g.l2_eps }, vec![k], nk * dk, "gdn.k_l2");
        let beta = bk.f(Op::Act(Act::Sigmoid), vec![b], nv, "gdn.beta");
        let dtb = self.param("gdn.dt_bias", vec![nv], true, Init::Uniform(-1.0, 1.0))?;
        let t = bk.f(Op::Add, vec![a, dtb], nv, "gdn.dt");
        let sp = bk.f(Op::Act(Act::Softplus), vec![t], nv, "gdn.dt_softplus");
        // `gdn.A` is the (negative) decay rate A = −exp(A_log).
        let aa = self.param("gdn.A", vec![nv], true, Init::Uniform(-4.0, -0.3))?;
        let gl = bk.f(Op::Mul, vec![sp, aa], nv, "gdn.log_decay");
        let st = self.state("gdn.S", StateKind::Fixed, vec![nv, dk, dv], 0.0)?;
        let o = bk.fw(
            Op::GatedDelta { k_heads: nk, v_heads: nv, dk, dv, head_map: g.head_map, q_scale: 1.0 / (dk as f64).sqrt() },
            vec![q, k, v, gl, beta, st],
            nv * dv,
            Some("gdn.core"),
            vec![sid(st)],
        );
        let w = self.param("gdn.norm.gain", vec![dv], true, Init::Uniform(0.6, 1.4))?;
        let o = bk.f(Op::GatedRmsNorm { eps: g.norm_eps, groups: nv, gate_first: false, act: g.gate_act }, vec![o, z, w], nv * dv, "gdn.normed");
        self.linear(bk, o, "gdn.out", d, nv * dv, false, true, "gdn.out")
    }

    /// Depthwise causal conv under role `name` (`name.w [C, K]`, `name.b`), SiLU activated.
    fn conv(&mut self, bk: &mut Bk, x: Ref, ch: usize, kernel: usize, bias: bool, name: &str) -> Result<Ref> {
        let w = self.param(&format!("{name}.w"), vec![ch, kernel], true, Init::Normal(0.3))?;
        let st = self.state(&format!("{name}.window"), StateKind::Fixed, vec![kernel.saturating_sub(1), ch], 0.0)?;
        let mut ins = vec![x, st, w];
        if bias {
            ins.push(self.param(&format!("{name}.b"), vec![ch], true, Init::Uniform(-0.1, 0.1))?);
        }
        Ok(bk.fw(Op::CausalConv1d { channels: ch, kernel, bias, act: Some(Act::Silu), dilation: 1 }, ins, ch, Some(name), vec![sid(st)]))
    }

    // ───────────────────────────── Mamba ─────────────────────────────

    fn mamba(&mut self, bk: &mut Bk, m: &MambaSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (i, n, r) = (m.inner, m.state, m.dt_rank);
        let xs = self.linear(bk, x, "mamba.in.x", i, d, m.proj_bias, true, "mamba.x_in")?;
        let z = self.linear(bk, x, "mamba.in.z", i, d, m.proj_bias, true, "mamba.z")?;
        let xc = self.conv(bk, xs, i, m.conv_kernel, m.conv_bias, "mamba.conv")?;
        let mut dtr = self.linear(bk, xc, "mamba.x.dt", r, i, false, true, "mamba.dt_in")?;
        let mut bb = self.linear(bk, xc, "mamba.x.B", n, i, false, true, "mamba.B")?;
        let mut cc = self.linear(bk, xc, "mamba.x.C", n, i, false, true, "mamba.C")?;
        if let Some(ns) = m.bcdt_norm {
            dtr = self.norm(bk, dtr, ns, "mamba.dt_norm", r, 1, vec![r], true, "mamba.dt_normed")?;
            bb = self.norm(bk, bb, ns, "mamba.b_norm", n, 1, vec![n], true, "mamba.B_normed")?;
            cc = self.norm(bk, cc, ns, "mamba.c_norm", n, 1, vec![n], true, "mamba.C_normed")?;
        }
        let dt = self.linear(bk, dtr, "mamba.dt", i, r, true, true, "mamba.dt")?;
        let dt = bk.f(Op::Act(Act::Softplus), vec![dt], i, "mamba.dt_softplus");
        let aa = self.param("mamba.A", vec![i, n], true, Init::Uniform(-3.0, -0.3))?;
        let dd = self.param("mamba.D", vec![i], true, Init::Uniform(0.5, 1.5))?;
        let st = self.state("mamba.h", StateKind::Fixed, vec![i, n], 0.0)?;
        let y =
            bk.fw(Op::SelectiveScan { inner: i, state: n }, vec![xc, dt, bb, cc, aa, dd, st], i, Some("mamba.scan"), vec![sid(st)]);
        let gz = bk.f(Op::Act(Act::Silu), vec![z], i, "mamba.gate");
        let y = bk.f(Op::Mul, vec![y, gz], i, "mamba.gated");
        self.linear(bk, y, "mamba.out", d, i, m.proj_bias, true, "mamba.out")
    }

    fn mamba2(&mut self, bk: &mut Bk, m: &Mamba2Spec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let inner = m.heads * m.head_dim;
        let gn = m.groups * m.state;
        let conv_dim = inner + 2 * gn;
        let z = self.linear(bk, x, "mamba2.in.z", inner, d, m.proj_bias, true, "mamba2.z")?;
        let xbc = self.linear(bk, x, "mamba2.in.xbc", conv_dim, d, m.proj_bias, true, "mamba2.xbc")?;
        let dt = self.linear(bk, x, "mamba2.in.dt", m.heads, d, m.proj_bias, true, "mamba2.dt_in")?;
        let xbc = self.conv(bk, xbc, conv_dim, m.conv_kernel, m.conv_bias, "mamba2.conv")?;
        let xs = bk.st(Op::Slice { start: 0, len: inner }, vec![xbc], inner);
        let bb = bk.st(Op::Slice { start: inner, len: gn }, vec![xbc], gn);
        let cc = bk.st(Op::Slice { start: inner + gn, len: gn }, vec![xbc], gn);
        let dtb = self.param("mamba2.dt_bias", vec![m.heads], true, Init::Uniform(-2.0, 0.0))?;
        let dt = bk.f(Op::Add, vec![dt, dtb], m.heads, "mamba2.dt_biased");
        let mut dt = bk.f(Op::Act(Act::Softplus), vec![dt], m.heads, "mamba2.dt");
        if m.dt_min > 0.0 || m.dt_max.is_finite() {
            dt = bk.f(Op::Clamp { lo: m.dt_min, hi: m.dt_max }, vec![dt], m.heads, "mamba2.dt_clamped");
        }
        let aa = self.param("mamba2.A", vec![m.heads], true, Init::Uniform(-3.0, -0.3))?;
        let dd = self.param("mamba2.D", vec![m.heads], true, Init::Uniform(0.5, 1.5))?;
        let st = self.state("mamba2.h", StateKind::Fixed, vec![m.heads, m.head_dim, m.state], 0.0)?;
        let y = bk.fw(
            Op::Ssd { heads: m.heads, head_dim: m.head_dim, groups: m.groups, state: m.state },
            vec![xs, dt, bb, cc, aa, dd, st],
            inner,
            Some("mamba2.scan"),
            vec![sid(st)],
        );
        let w = self.param("mamba2.norm.gain", vec![inner], true, Init::Uniform(0.6, 1.4))?;
        let y =
            bk.f(Op::GatedRmsNorm { eps: m.norm_eps, groups: m.norm_groups, gate_first: true, act: Act::Silu }, vec![y, z, w], inner, "mamba2.normed");
        self.linear(bk, y, "mamba2.out", d, inner, m.proj_bias, true, "mamba2.out")
    }

    // ───────────────────────────── RWKV ─────────────────────────────

    fn token_shift(&mut self, bk: &mut Bk, x: Ref, name: &str) -> Result<Ref> {
        let d = self.s.hidden_size;
        let sh = self.state(name, StateKind::Fixed, vec![d], 0.0)?;
        Ok(bk.fw(Op::TokenShift, vec![x, sh], d, None, vec![sid(sh)]))
    }

    /// RWKV-4 time mix. The lerps are `prev + (x − prev)·μ` (HF's `x·μ + prev·(1−μ)`).
    fn rwkv_time(&mut self, bk: &mut Bk, r: &RwkvTimeSpec, x: Ref) -> Result<Ref> {
        if r.version != 4 {
            return Err(LowerError::not_lowerable(format!("RWKV-{} time mix has no config lowering", r.version)));
        }
        let d = self.s.hidden_size;
        let a = r.attn_dim;
        let prev = self.token_shift(bk, x, "rwkv.att.shift")?;
        let mk = self.param("rwkv.att.mix_k", vec![d], true, Init::Uniform(0.0, 1.0))?;
        let mv = self.param("rwkv.att.mix_v", vec![d], true, Init::Uniform(0.0, 1.0))?;
        let mr = self.param("rwkv.att.mix_r", vec![d], true, Init::Uniform(0.0, 1.0))?;
        let xk = bk.f(Op::Lerp, vec![prev, x, mk], d, "rwkv.att.xk");
        let xv = bk.f(Op::Lerp, vec![prev, x, mv], d, "rwkv.att.xv");
        let xr = bk.f(Op::Lerp, vec![prev, x, mr], d, "rwkv.att.xr");
        let k = self.linear(bk, xk, "rwkv.att.k", a, d, false, true, "rwkv.att.k")?;
        let v = self.linear(bk, xv, "rwkv.att.v", a, d, false, true, "rwkv.att.v")?;
        let rr = self.linear(bk, xr, "rwkv.att.r", a, d, false, true, "rwkv.att.r")?;
        let rr = bk.f(Op::Act(Act::Sigmoid), vec![rr], a, "rwkv.att.r_sigmoid");
        // `rwkv.att.w` is the log-decay per step, w = −exp(time_decay); `u` the bonus.
        let w = self.param("rwkv.att.w", vec![a], true, Init::Uniform(-3.0, -0.05))?;
        let u = self.param("rwkv.att.u", vec![a], true, Init::Normal(0.5))?;
        let num = self.state("rwkv.att.num", StateKind::Fixed, vec![a], 0.0)?;
        let den = self.state("rwkv.att.den", StateKind::Fixed, vec![a], 0.0)?;
        let mx = self.state("rwkv.att.max", StateKind::Fixed, vec![a], -1e38)?;
        let wkv = bk.fw(Op::Wkv4, vec![k, v, w, u, num, den, mx], a, Some("rwkv.att.wkv"), vec![sid(num), sid(den), sid(mx)]);
        let o = bk.f(Op::Mul, vec![rr, wkv], a, "rwkv.att.rwkv");
        self.linear(bk, o, "rwkv.att.o", d, a, false, true, "rwkv.att.out")
    }

    fn rwkv_channel(&mut self, bk: &mut Bk, c: &RwkvChannelSpec, x: Ref) -> Result<Ref> {
        if c.version != 4 {
            return Err(LowerError::not_lowerable(format!("RWKV-{} channel mix has no config lowering", c.version)));
        }
        let d = self.s.hidden_size;
        let f = c.intermediate;
        let prev = self.token_shift(bk, x, "rwkv.ffn.shift")?;
        let mk = self.param("rwkv.ffn.mix_k", vec![d], true, Init::Uniform(0.0, 1.0))?;
        let mr = self.param("rwkv.ffn.mix_r", vec![d], true, Init::Uniform(0.0, 1.0))?;
        let xk = bk.f(Op::Lerp, vec![prev, x, mk], d, "rwkv.ffn.xk");
        let xr = bk.f(Op::Lerp, vec![prev, x, mr], d, "rwkv.ffn.xr");
        let k = self.linear(bk, xk, "rwkv.ffn.k", f, d, false, true, "rwkv.ffn.k")?;
        let k = bk.f(Op::Act(Act::Relu2), vec![k], f, "rwkv.ffn.k_act");
        let v = self.linear(bk, k, "rwkv.ffn.v", d, f, false, true, "rwkv.ffn.v")?;
        let r = self.linear(bk, xr, "rwkv.ffn.r", d, d, false, true, "rwkv.ffn.r")?;
        let r = bk.f(Op::Act(Act::Sigmoid), vec![r], d, "rwkv.ffn.r_sigmoid");
        Ok(bk.f(Op::Mul, vec![r, v], d, "rwkv.ffn.out"))
    }

    // ───────────────────────────── FFN ─────────────────────────────

    /// Dense MLP under a role prefix (`mlp` or `moe.shared`).
    fn mlp(&mut self, bk: &mut Bk, m: &MlpSpec, x: Ref, pfx: &str) -> Result<Ref> {
        let d = self.s.hidden_size;
        let i = m.intermediate;
        let u = self.linear(bk, x, &format!("{pfx}.up"), i, d, m.up_bias, true, &format!("{pfx}.up"))?;
        let hdn = if m.gated {
            let g = self.linear(bk, x, &format!("{pfx}.gate"), i, d, m.up_bias, true, &format!("{pfx}.gate"))?;
            match m.glu {
                Glu::Standard => {
                    let a = bk.f(Op::Act(m.act), vec![g], i, &format!("{pfx}.act"));
                    bk.f(Op::Mul, vec![a, u], i, &format!("{pfx}.hidden"))
                }
                Glu::ClampedSwiGlu { alpha, limit } => {
                    bk.f(Op::ClampedSwiGlu { alpha, limit }, vec![g, u], i, &format!("{pfx}.hidden"))
                }
            }
        } else {
            bk.f(Op::Act(m.act), vec![u], i, &format!("{pfx}.act"))
        };
        self.linear(bk, hdn, &format!("{pfx}.down"), d, i, m.down_bias, true, &format!("{pfx}.out"))
    }

    fn moe(&mut self, bk: &mut Bk, m: &MoeSpec, x: Ref) -> Result<Ref> {
        self.moe_split(bk, m, x, x)
    }

    /// The MoE block with its router reading `rx` and its experts `x` (Gemma-4 feeds them apart).
    fn moe_split(&mut self, bk: &mut Bk, m: &MoeSpec, rx: Ref, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (e, i) = (m.experts, m.intermediate);
        // A spec flag the lowering would not apply is a refusal, never a silent no-op (FR-26): a model whose router
        // carries one of these would otherwise compute a different function with no error anywhere.
        let r = &m.router;
        if r.selection_bias && !matches!(r.scoring, Scoring::Sigmoid | Scoring::Softmax) {
            return Err(LowerError::not_lowerable(format!(
                "router selection bias with {:?} scoring: the bias joins the selection scores of the sigmoid and softmax routers only, so this lowering would drop it",
                r.scoring
            )));
        }
        if r.jitter_eps != 0.0 && r.scoring != Scoring::SparseMixer {
            return Err(LowerError::not_lowerable(format!(
                "router jitter noise {} with {:?} scoring: only sparsemixer reads it, so this lowering would drop it",
                r.jitter_eps, r.scoring
            )));
        }
        let logits = self.linear(bk, rx, "moe.router", e, d, m.router.linear_bias, true, "moe.router")?;
        let mut rins = vec![logits];
        if m.router.selection_bias {
            rins.push(self.param("moe.sel_bias", vec![e], true, Init::Uniform(-0.05, 0.05))?);
        }
        if m.router.per_expert_scale {
            rins.push(self.param("moe.expert_scale", vec![e], true, Init::Uniform(0.5, 1.5))?);
        }
        let route = bk.push(
            Op::Route { router: m.router.clone(), experts: e, top_k: m.top_k },
            rins,
            vec![vec![m.top_k], vec![m.top_k]],
            vec![HlType::Idx, HlType::F32],
            Some("moe.route"),
            vec![],
        );
        let gp = self.param("moe.experts.gate", vec![e, i, d], true, Init::Normal(W_STD))?;
        let up = self.param("moe.experts.up", vec![e, i, d], true, Init::Normal(W_STD))?;
        let dp = self.param("moe.experts.down", vec![e, d, i], true, Init::Normal(W_STD))?;
        let mut ins = vec![x, Ref::Node(route, 0), Ref::Node(route, 1), gp, up, dp];
        if m.expert_bias {
            ins.push(self.param("moe.experts.gate_b", vec![e, i], true, Init::Uniform(-0.1, 0.1))?);
            ins.push(self.param("moe.experts.up_b", vec![e, i], true, Init::Uniform(-0.1, 0.1))?);
            ins.push(self.param("moe.experts.down_b", vec![e, d], true, Init::Uniform(-0.1, 0.1))?);
        }
        let mut y = bk.f(
            Op::MoeExperts { top_k: m.top_k, act: m.act, glu: m.glu, bias: m.expert_bias, input_scaled: m.input_scaled },
            ins,
            d,
            "moe.routed",
        );
        if let Some(sh) = &m.shared {
            let spec = MlpSpec {
                intermediate: sh.intermediate,
                act: m.act,
                gated: true,
                glu: Glu::Standard,
                up_bias: false,
                down_bias: false,
                name: None,
            };
            let mut s = self.mlp(bk, &spec, x, "moe.shared")?;
            if sh.sigmoid_gate {
                let g = self.linear(bk, x, "moe.shared_gate", 1, d, false, true, "moe.shared_gate")?;
                let g = bk.f(Op::Act(Act::Sigmoid), vec![g], 1, "moe.shared_gate_act");
                s = bk.f(Op::Mul, vec![s, g], d, "moe.shared.gated");
            }
            y = bk.f(Op::Add, vec![y, s], d, "moe.out");
        }
        Ok(y)
    }
}

fn block_name(ls: &LayerSpec) -> String {
    let mix = match &ls.mixer {
        Mixer::Attention(a) => {
            let mut s = "attn".to_string();
            match &a.position {
                Position::None => s.push_str(".nope"),
                Position::Alibi(_) => s.push_str(".alibi"),
                Position::Rope(_) => {}
            }
            if let Some(w) = a.window {
                s.push_str(&format!(".w{w}"));
            }
            s
        }
        Mixer::Mla(_) => "mla".into(),
        Mixer::GatedDeltaNet(_) => "gdn".into(),
        Mixer::Mamba(_) => "mamba".into(),
        Mixer::Mamba2(_) => "mamba2".into(),
        Mixer::RwkvTime(r) => format!("rwkv{}", r.version),
    };
    let ffn = match &ls.ffn {
        Ffn::None => String::new(),
        Ffn::Mlp(_) => "+mlp".into(),
        Ffn::Moe(_) => "+moe".into(),
        Ffn::RwkvChannel(_) => "+cmix".into(),
        Ffn::MlpMoe(_) => "+mlp|moe".into(),
    };
    format!("{mix}{ffn}")
}
