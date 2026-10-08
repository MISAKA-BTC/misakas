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
        e0: if spec.embed_carry { Some(carries.len() - 1) } else { None },
        sharing: false,
        lowrank: BTreeMap::new(),
    };
    let pre = b.pre_block()?;
    let mut kinds: Vec<(LayerSpec, Vec<u16>)> = Vec::new();
    let mut schedule = Vec::with_capacity(spec.layers.len());
    let mut layer_of = Vec::with_capacity(spec.layers.len());
    for (li, ls) in spec.layers.iter().enumerate() {
        // `ATTN_CROSS_V1`, text-only: a cross-attention layer reads no states and HF skips it (`layer_of` keeps the model's numbering).
        if matches!(ls.mixer, Mixer::CrossAttention(_)) && spec.cross_states.is_none() {
            continue;
        }
        // Layers that differ only in constants the program reads as data (a PLE layer's hash
        // constants follow from its index) run the same block.
        let ls = &block_kind(spec, ls);
        let bis = match kinds.iter().find(|(s, _)| s == ls) {
            Some((_, bis)) => bis.clone(),
            None => {
                let start = b.blocks.len();
                let made = b.layer_blocks(ls, kinds.len())?;
                // A block another kind already built (the FFN site of an mHC layer does not depend on the attention kind) is that block:
                // a program is capped at 16 blocks.
                // (Only the mHC layers: every other model's program is as it was, block for block.)
                let contiguous = spec.mhc.is_some() && made.iter().enumerate().all(|(k, bi)| *bi == start + k);
                let made: Vec<usize> = if contiguous {
                    let fresh = b.blocks.split_off(start);
                    let mut out = Vec::with_capacity(fresh.len());
                    for blk in fresh {
                        let same = b.blocks.iter().position(|o| o.role == blk.role && o.nodes == blk.nodes && o.outputs == blk.outputs);
                        match same {
                            Some(j) => out.push(j),
                            None => {
                                b.blocks.push(blk);
                                out.push(b.blocks.len() - 1);
                            }
                        }
                    }
                    out
                } else {
                    made
                };
                let bis = made
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
    // The activation sparsity of every layer that runs a block with a Gaussian top-k: `z`, or none for a dense layer.
    for (li, ls) in spec.layers.iter().enumerate() {
        let Ffn::Mlp(m) = &ls.ffn else { continue };
        if !spec.layers.iter().any(|l| matches!(&l.ffn, Ffn::Mlp(m) if m.sparsity.is_some())) {
            continue;
        }
        let z = match m.sparsity {
            Some(p) => Some(crate::detmath::norm_inv_cdf(p).ok_or_else(|| {
                LowerError::not_lowerable(format!("FFN_ACTIVATION_SPARSITY_V1: a sparsity of {p} (it is a fraction in (0, 1))"))
            })?),
            None => None,
        };
        for (slot, l) in layer_of.iter().enumerate() {
            if *l != li {
                continue;
            }
            let bi = schedule[slot] as usize;
            for n in &mut b.blocks[bi].nodes {
                if let Op::GaussianTopK { layers } = &mut n.op
                    && !layers.iter().any(|(ml, _)| *ml == li)
                {
                    layers.push((li, z));
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
            OutputSpec::Classify { .. } | OutputSpec::TokenLogits { .. } => HlOutput::Embedding { normalized: false },
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
    /// The carry that holds the embedding block's output (`EMBED_CARRY_V1`).
    e0: Option<usize>,
    /// Building a shared branch (`ATTN_SHARED_BLOCK_V1`): the weights and norm gains declared meanwhile are global (one tensor for every
    /// layer of the group); the adapters, states and every activation stay the occurrence's.
    sharing: bool,
    /// Linears that carry a low-rank adapter (`LINEAR_LOWRANK_ADAPTER_V1`): role → (rank, base name of the adapter's params).
    lowrank: BTreeMap<String, (usize, String)>,
}

/// A layer spec as the block builder sees it: the hash constants of a PLE layer (its index among
/// the PLE layers) are data the lowering fills per layer, not part of the block.
fn block_kind(spec: &ArchSpec, ls: &LayerSpec) -> LayerSpec {
    let mut k = ls.clone();
    // The activation sparsity of a layer is data (`Op::GaussianTopK::layers`): a model that sparsifies some of its layers
    // gives every layer's MLP the same block.
    if spec.layers.iter().any(|l| matches!(&l.ffn, Ffn::Mlp(m) if m.sparsity.is_some()))
        && let Ffn::Mlp(m) = &mut k.ffn
    {
        m.sparsity = Some(0.5);
    }
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
    // AltUp: the streams and one more slot — the layer's intermediate between its two blocks.
    let lanes = match &spec.altup {
        Some(a) => {
            if spec.hyper.is_some() {
                return Err(LowerError::not_lowerable("RESIDUAL_ALTUP_V1 and RESIDUAL_GATED_HC_V1 in one model"));
            }
            if a.streams < 2 || a.streams > 7 || (a.streams + 1).saturating_mul(spec.hidden_size) > 1 << 24 {
                return Err(LowerError::not_lowerable(format!(
                    "RESIDUAL_ALTUP_V1: {} streams of {} (2..=7 streams, `(streams + 1) × hidden` ≤ 2^24)",
                    a.streams, spec.hidden_size
                )));
            }
            (a.streams + 1) * spec.hidden_size
        }
        None => spec.hidden_size * streams,
    };
    // Manifold-constrained hyper-connections: the streams, and room for what a layer's blocks hand to each other (its mixing weights, the
    // normed input of the mixer, the query and its latent) — all at the residual scale, packed after the streams.
    let lanes = match &spec.mhc {
        Some(m) => {
            if spec.hyper.is_some() || spec.altup.is_some() {
                return Err(LowerError::not_lowerable("RESIDUAL_MHC_SINKHORN_V1 beside another multi-stream residual"));
            }
            if m.streams == 0 || m.streams > 16 || m.iters == 0 || m.iters > 64 {
                return Err(LowerError::not_lowerable(format!(
                    "RESIDUAL_MHC_SINKHORN_V1: {} streams, {} Sinkhorn iterations (1..=16 streams, 1..=64 iterations)",
                    m.streams, m.iters
                )));
            }
            let d = spec.hidden_size;
            let extras = spec
                .layers
                .iter()
                .filter_map(|l| match &l.mixer {
                    Mixer::SharedKv(a) => {
                        let base = m.streams + m.streams * m.streams + d;
                        // After the first block: the latent and the query; after the compressors: the query, the entry row and the indexer's rows.
                        let first = base + a.q_rank + a.heads * a.head_dim;
                        let second = a.compressed.as_ref().map_or(0, |c| {
                            base + a.heads * a.head_dim
                                + a.head_dim
                                + c.indexer.as_ref().map_or(0, |i| i.head_dim + i.heads * i.head_dim + i.heads)
                        });
                        Some(first.max(second))
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let total = (m.streams * d).saturating_add(extras);
            if total > 1 << 24 {
                return Err(LowerError::not_lowerable(format!("RESIDUAL_MHC_SINKHORN_V1: a carry of {total} lanes is past 2^24")));
            }
            total
        }
        None => lanes,
    };
    let mut out = vec![CarryDecl { name: "h".into(), shape: vec![lanes], resid: false }];
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
        out.push(CarryDecl { name: format!("kv{slot}.k"), shape: vec![k], resid: false });
        out.push(CarryDecl { name: format!("kv{slot}.v"), shape: vec![v], resid: false });
    }
    // `FFN_SHORTCUT_MOE_V1`: the MoE of the layer that produces it rides to the layer that consumes it, at the residual's scale.
    let produces = |ls: &LayerSpec| matches!(&ls.ffn, Ffn::MlpShortcut(s) if matches!(s.side, ShortcutSide::Produce(_)));
    let consumes = |ls: &LayerSpec| matches!(&ls.ffn, Ffn::MlpShortcut(s) if matches!(s.side, ShortcutSide::Consume));
    if spec.layers.iter().any(consumes) && !spec.layers.iter().any(produces) {
        return Err(LowerError::not_lowerable("FFN_SHORTCUT_MOE_V1: a layer consumes a side value that no layer produces"));
    }
    if spec.layers.iter().any(produces) {
        if spec.hyper.is_some() || spec.altup.is_some() || spec.mhc.is_some() {
            return Err(LowerError::not_lowerable("FFN_SHORTCUT_MOE_V1 beside another multi-stream residual is not modelled"));
        }
        out.push(CarryDecl { name: "side".into(), shape: vec![spec.hidden_size], resid: true });
    }
    // `EMBED_CARRY_V1`: the embedding block's output, past every layer.
    if spec.embed_carry {
        if spec.hyper.is_some() {
            return Err(LowerError::not_lowerable("EMBED_CARRY_V1 beside hyper-connection streams"));
        }
        out.push(CarryDecl { name: "e0".into(), shape: vec![spec.hidden_size], resid: false });
    }
    Ok(out)
}

/// What the compressors of a compressed shared-KV layer hand to its attention block: the entry row and — with an indexer — the indexer's key
/// row, query and head weights.
struct CompRows {
    ent: Ref,
    index: Option<(Ref, Ref, Ref)>,
}

/// The next `len` lanes of the carry past the streams' hand-over, as the value they were before they were packed: a slice, and — given a
/// site — a sited identity (no node in the integer program) so the value is narrowed to codes with its own statistics, not the carry's.
fn mhc_take(bk: &mut Bk, at: &mut usize, len: usize, site: Option<&str>) -> Ref {
    let r = bk.st(Op::Slice { start: *at, len }, vec![Ref::Carry(0)], len);
    *at += len;
    match site {
        Some(s) => bk.f(Op::Scale { c: 1.0 }, vec![r], len, s),
        None => r,
    }
}

/// A block under construction.
#[derive(Default)]
struct Bk {
    nodes: Vec<Node>,
    /// `WEIGHT_ROTATION_HADAMARD_V1`: the rotated activations already built in this block — `(input, param, rotated)` — so the
    /// projections that read one activation share its rotation (q, k and v; gate and up), as the producer's graph does.
    rot: Vec<(Ref, String, Ref)>,
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
        let weight_per_layer = per_layer && !self.sharing;
        let x = self.rotated_input(bk, x, name, inp)?;
        let w = self.param(&format!("{name}.w"), vec![out, inp], weight_per_layer, Init::Normal(W_STD))?;
        let mut ins = vec![x, w];
        if bias {
            ins.push(self.param(&format!("{name}.b"), vec![out], weight_per_layer, Init::Uniform(-0.1, 0.1))?);
        }
        // `LINEAR_LOWRANK_ADAPTER_V1`: a low-rank adapter of the BASE model — `A [r, in]`, `B [out, r]`, the occurrence's own tensors,
        // `y = W x + B (A x)` — the unmerged path an external LoRA takes, with scale 1.
        if let Some((rank, base)) = self.lowrank.get(name).cloned() {
            if self.s.adapter.as_ref().and_then(|a| a.role(name)).is_some() {
                return Err(LowerError::not_lowerable("a LoRA adapter over a projection that already carries a low-rank adapter"));
            }
            ins.push(self.param(&format!("{base}.lora_a"), vec![rank, inp], true, Init::Normal(W_STD))?);
            ins.push(self.param(&format!("{base}.lora_b"), vec![out, rank], true, Init::Normal(W_STD))?);
            return Ok(bk.f(Op::Linear { bias, lora: Some(LoraOp { rank, num: 1, den: 1 }) }, ins, out, site));
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

    /// **`WEIGHT_ROTATION_HADAMARD_V1`**: the activation a projection of role `role` reads — `x` itself, or, where the checkpoint
    /// stores that projection's weights in a rotated basis ([`crate::spec::HfStorage::input_rotations`]), `R·x` (`Op::BlockLinear`
    /// over the param that holds `R`'s blocks), built once per activation in the block.
    fn rotated_input(&mut self, bk: &mut Bk, x: Ref, role: &str, width: usize) -> Result<Ref> {
        let Some(r) = self.s.hf.input_rotations.get(role).copied() else { return Ok(x) };
        if r.width != width || r.block == 0 || width % r.block != 0 {
            return Err(LowerError::not_lowerable(format!(
                "a rotation of block {} over width {} on `{role}`, which reads {width} values",
                r.block, r.width
            )));
        }
        let name = r.fwd_param();
        if let Some((_, _, y)) = bk.rot.iter().find(|(a, p, _)| *a == x && *p == name) {
            return Ok(*y);
        }
        let p = self.param(&name, vec![width, r.block], false, Init::Normal(W_STD))?;
        let site = format!("{role}.rotated");
        let y = bk.f(Op::BlockLinear { block: r.block }, vec![x, p], width, &site);
        bk.rot.push((x, name, y));
        Ok(y)
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
            ins.push(self.param(&format!("{name}.gain"), gain_shape.clone(), per_layer && !self.sharing, init)?);
        }
        if spec.bias {
            ins.push(self.param(&format!("{name}.bias"), gain_shape, per_layer && !self.sharing, Init::Uniform(-0.1, 0.1))?);
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
        let mut bk = Bk::default();
        let table = self.param("embed.table", vec![s.vocab_size, e.dim], false, Init::Normal(0.5))?;
        let mut x = bk.f(Op::Embedding, vec![Ref::Token, table], e.dim, "embed");
        // `WEIGHT_ROTATION_HADAMARD_V1`: a table stored in the rotated basis — the looked-up row restored, `h = R⁻¹·z`.
        if let Some(r) = self.s.hf.embed_rotation {
            if r.width != e.dim || r.block == 0 || e.dim % r.block != 0 {
                return Err(LowerError::not_lowerable(format!("an embedding rotation of block {} over width {} on a {}-wide table", r.block, r.width, e.dim)));
            }
            let p = self.param(&r.inv_param(), vec![e.dim, r.block], false, Init::Normal(W_STD))?;
            x = bk.f(Op::BlockLinear { block: r.block }, vec![x, p], e.dim, "embed.unrotated");
        }
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
        // Hyper-connections with a mixing matrix: the embedding repeated into every stream, the rest of the carry zeros.
        if let Some(mh) = &s.mhc {
            let pad = self.carries[0].shape[0] - d * mh.streams;
            let mut parts = vec![x; mh.streams];
            if pad > 0 {
                parts.push(bk.st(Op::Zeros, vec![], pad));
            }
            x = bk.st(Op::Concat, parts, d * mh.streams + pad);
        }
        // AltUp: stream 0 is the embedding; stream `i` is a projection of it brought to its magnitude; the last slot repeats stream 0.
        if let Some(au) = &s.altup {
            let mut parts = vec![x];
            for i in 1..au.streams {
                let p = self.linear(&mut bk, x, &format!("altup.proj{i}"), d, d, false, false, &format!("altup.proj{i}"))?;
                parts.push(bk.f(Op::RmsMatch { floor: au.floor }, vec![p, x], d, &format!("altup.stream{i}")));
            }
            parts.push(x);
            x = bk.st(Op::Concat, parts, d * (au.streams + 1));
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
        for (ci, c) in self.carries.clone().iter().enumerate().skip(1) {
            let n: usize = c.shape.iter().product();
            if Some(ci) == self.e0 {
                // `EMBED_CARRY_V1`: the embedding block's output itself (the width is the hidden one; a model with a projected
                // table has the projection behind it).
                if n != self.s.hidden_size {
                    return Err(LowerError::not_lowerable("EMBED_CARRY_V1: the embedding block's output is not the hidden width"));
                }
                outputs.push(x);
            } else {
                outputs.push(bk.f(Op::Zeros, vec![], n, &format!("carry.{}", c.name)));
            }
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
        let mut bk = Bk::default();
        let mut x = Ref::Carry(0);
        // Hyper-connections: one more gated mix brings the streams down to the hidden width (its norm
        // stands where the final norm would).
        if s.hyper.is_some() {
            x = self.hc_mix(&mut bk, x, "final", false, false)?.0;
        }
        // Manifold-constrained hyper-connections: one weighted collapse of the streams (`HyperHead`), before the final norm.
        if let Some(mh) = &s.mhc {
            let n = mh.streams * d;
            let hs = bk.st(Op::Slice { start: 0, len: n }, vec![x], n);
            let norm = NormSpec { kind: NormKind::Rms, eps: mh.norm_eps, gain: Gain::None, bias: false };
            let flat = self.norm(&mut bk, hs, norm, "mhc.head.flat", n, 1, vec![n], false, "mhc.head.flat")?;
            let mixes = self.linear(&mut bk, flat, "mhc.head.fn", mh.streams, n, false, false, "mhc.head.mixes")?;
            let base = self.param("mhc.head.base", vec![mh.streams], false, Init::Uniform(-0.5, 0.5))?;
            let scale = self.param("mhc.head.scale", vec![1], false, Init::Uniform(0.5, 1.5))?;
            let pre = bk.f(Op::MhcPre { eps: mh.eps }, vec![mixes, base, scale], mh.streams, "mhc.head.pre");
            x = bk.f(Op::StreamMix { n_in: mh.streams, n_out: 1, transpose: false }, vec![hs, pre], d, "mhc.head.collapsed");
        }
        // AltUp: the other streams come back through their own projections at stream 0's magnitude, and the streams are averaged.
        if let Some(au) = &s.altup {
            let h0 = bk.st(Op::Slice { start: 0, len: d }, vec![x], d);
            let mut parts = vec![h0];
            for i in 1..au.streams {
                let hi = bk.st(Op::Slice { start: i * d, len: d }, vec![x], d);
                let p = self.linear(&mut bk, hi, &format!("altup.unembed{i}"), d, d, false, false, &format!("altup.unembed{i}"))?;
                parts.push(bk.f(Op::RmsMatch { floor: au.floor }, vec![p, h0], d, &format!("altup.merge{i}")));
            }
            let cat = bk.st(Op::Concat, parts, d * au.streams);
            x = bk.f(Op::StreamMean { streams: au.streams }, vec![cat], d, "altup.mean");
        }
        if let Some(n) = s.final_norm {
            x = self.full_norm(&mut bk, x, n, "final_norm", d, false)?;
        }
        // A sequence classifier: the final row through the optional dense + activation and the classification layer; no head.
        if let OutputSpec::Classify { labels, bias, pre } = &s.output {
            if h.transform.is_some() {
                return Err(LowerError::not_lowerable("HEAD_TRANSFORM_V1 under a classification output: the transform belongs to a language-model head"));
            }
            if let Some(p) = pre {
                x = self.linear(&mut bk, x, "classifier.pre", d, d, p.bias, false, "classifier.pre")?;
                x = bk.f(Op::Act(plain_act(p.act, "a classifier head")?), vec![x], d, "classifier.act");
            }
            x = self.linear(&mut bk, x, "classifier.out", *labels, d, *bias, false, "classifier.out")?;
            if !matches!(x, Ref::Node(..)) {
                x = bk.f(Op::Scale { c: 1.0 }, vec![x], *labels, "classifier.logits");
            }
            self.blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: bk.nodes, outputs: vec![x] });
            return Ok(self.blocks.len() - 1);
        }
        // Per-token logits (`OUTPUT_TOKEN_LOGITS_V1`): the position's final row through the classification layer; no pooling, no head.
        if let OutputSpec::TokenLogits { labels, bias } = &s.output {
            if h.transform.is_some() {
                return Err(LowerError::not_lowerable("HEAD_TRANSFORM_V1 under per-token logits: the transform belongs to a language-model head"));
            }
            x = self.linear(&mut bk, x, "classifier.out", *labels, d, *bias, false, "classifier.out")?;
            if !matches!(x, Ref::Node(..)) {
                x = bk.f(Op::Scale { c: 1.0 }, vec![x], *labels, "classifier.logits");
            }
            self.blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: bk.nodes, outputs: vec![x] });
            return Ok(self.blocks.len() - 1);
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
            x = bk.f(Op::Act(plain_act(t.act, "a head transform")?), vec![x], d, "head.transform.act");
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
        let x = self.rotated_input(&mut bk, x, "head", width)?;
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
        if let Residual::AltUp { .. } = &ls.residual {
            return self.altup_blocks(ls, kind_index);
        }
        if let Residual::Mhc { .. } = &ls.residual {
            return self.mhc_blocks(ls, kind_index);
        }
        if let Residual::Sandwich { pre_mixer, post_mixer, .. } = &ls.residual {
            let d = self.s.hidden_size;
            let mut bk = Bk::default();
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
            let mut bk = Bk::default();
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
            let mut bk = Bk::default();
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
                let mut bk = Bk::default();
                let h = self.ple_ngram(&mut bk, p, Ref::Carry(0))?;
                let outputs = self.layer_outputs(h);
                self.blocks.push(Block { name: format!("{name}.ple"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
                blocks.push(self.blocks.len() - 1);
            }
            let mut bk = Bk::default();
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

    fn altup(&self) -> Result<AltUpSpec> {
        self.s.altup.ok_or_else(|| LowerError::not_lowerable("RESIDUAL_ALTUP_V1: an AltUp layer needs the model's streams (ModelSpec::altup)"))
    }

    /// AltUp's router: `m(x) = tanh(modality_router(router_norm(x)·D⁻¹))`, `[K]`.
    fn altup_router(&mut self, bk: &mut Bk, x: Ref, norm: NormSpec, tag: &str) -> Result<Ref> {
        let (k, d) = (self.altup()?.streams, self.s.hidden_size);
        let n = self.full_norm(bk, x, norm, "altup.router_norm", d, true)?;
        let s = bk.f(Op::Scale { c: 1.0 / d as f64 }, vec![n], d, &format!("{tag}.router_in"));
        let r = self.linear(bk, s, "altup.router", k, d, false, true, &format!("{tag}.router"))?;
        Ok(bk.f(Op::Act(Act::Tanh), vec![r], k, &format!("{tag}.modalities")))
    }

    /// **`RESIDUAL_ALTUP_V1`** (Gemma-3n): a layer is two blocks. The mixer half predicts the streams, runs the (LAuReL-joined) mixer on the
    /// first and carries out the predictions and the joined stream `a` — `(K + 1)·D` lanes. The FFN half runs the FFN on `a`, corrects the
    /// predictions by the innovation, and adds the per-layer input to streams `1..K`.
    fn altup_blocks(&mut self, ls: &LayerSpec, kind_index: usize) -> Result<Vec<usize>> {
        let Residual::AltUp { pre_mixer, post_mixer, pre_ffn, post_ffn, router_norm, laurel, ple } = &ls.residual else {
            return Err(LowerError::eval("internal: altup_blocks of a layer that is not AltUp"));
        };
        if ls.ffn == Ffn::None || matches!(ls.ffn, Ffn::MlpMoe(_)) || matches!(ls.mixer, Mixer::None) {
            return Err(LowerError::not_lowerable("RESIDUAL_ALTUP_V1: a layer is a mixer and an FFN (Mlp or Moe)"));
        }
        let au = self.altup()?;
        let (k, d) = (au.streams, self.s.hidden_size);
        let n = k * d;
        let name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
        // ── the mixer half ──
        let mut bk = Bk::default();
        let c0 = Ref::Carry(0);
        let h0 = bk.st(Op::Slice { start: 0, len: d }, vec![c0], d);
        let m = self.altup_router(&mut bk, h0, *router_norm, "altup.pred")?;
        let coefs = self.linear(&mut bk, m, "altup.pred_coefs", k * k, k, false, true, "altup.pred_coefs")?;
        let hs = bk.st(Op::Slice { start: 0, len: n }, vec![c0], n);
        let mix = bk.f(Op::StreamMix { n_in: k, n_out: k, transpose: false }, vec![hs, coefs], n, "altup.mix");
        let pred = bk.f(Op::Add, vec![hs, mix], n, "altup.pred");
        let p0 = bk.st(Op::Slice { start: 0, len: d }, vec![pred], d);
        let an = self.full_norm(&mut bk, p0, *pre_mixer, "norm.mix", d, true)?;
        let lo = match laurel {
            Some(l) => {
                let a = self.linear(&mut bk, an, "laurel.left", l.rank, d, false, true, "laurel.left")?;
                let b = self.linear(&mut bk, a, "laurel.right", d, l.rank, false, true, "laurel.right")?;
                let nb = self.full_norm(&mut bk, b, l.post_norm, "laurel.norm", d, true)?;
                Some(bk.f(Op::Add, vec![an, nb], d, "laurel.out"))
            }
            None => None,
        };
        let att = self.mixer(&mut bk, &ls.mixer, an)?;
        let att = self.full_norm(&mut bk, att, *post_mixer, "norm.post_mix", d, true)?;
        let ag = bk.f(Op::Add, vec![p0, att], d, "altup.attn_sum");
        let joined = match lo {
            Some(lo) => {
                let s = bk.f(Op::Add, vec![ag, lo], d, "altup.laurel_sum");
                bk.f(Op::Scale { c: std::f64::consts::FRAC_1_SQRT_2 }, vec![s], d, "altup.joined")
            }
            None => ag,
        };
        let out = bk.st(Op::Concat, vec![pred, joined], n + d);
        let outputs = self.layer_outputs(out);
        self.blocks.push(Block { name: format!("{name}.mix"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
        let first = self.blocks.len() - 1;
        // ── the FFN half ──
        let mut bk = Bk::default();
        let c0 = Ref::Carry(0);
        let pred = bk.st(Op::Slice { start: 0, len: n }, vec![c0], n);
        let p0 = bk.st(Op::Slice { start: 0, len: d }, vec![c0], d);
        let a = bk.st(Op::Slice { start: n, len: d }, vec![c0], d);
        let n2 = self.full_norm(&mut bk, a, *pre_ffn, "norm.ffn", d, true)?;
        let f = self.ffn(&mut bk, &ls.ffn, n2)?;
        let f = self.full_norm(&mut bk, f, *post_ffn, "norm.post_ffn", d, true)?;
        let act = bk.f(Op::Add, vec![a, f], d, "altup.activated");
        let m2 = self.altup_router(&mut bk, act, *router_norm, "altup.cor")?;
        let cc = self.linear(&mut bk, m2, "altup.correction_coefs", k, k, false, true, "altup.correction_coefs")?;
        let inn = bk.f(Op::Sub, vec![act, p0], d, "altup.innovation");
        // `innovation · (coef + 1)` for every stream: the outer product with the coefficients, plus the innovation itself.
        let outer = bk.f(Op::StreamOuter { streams: k }, vec![inn, cc], n, "altup.correction");
        let rep = bk.st(Op::Concat, vec![inn; k], n);
        let t = bk.f(Op::Add, vec![pred, outer], n, "altup.cor_a");
        let cor = bk.f(Op::Add, vec![t, rep], n, "altup.corrected");
        let c0s = bk.st(Op::Slice { start: 0, len: d }, vec![cor], d);
        let y = match ple {
            Some(p) => {
                let mut first = c0s;
                if au.correct_scale {
                    let sp = self.param("altup.correct_scale", vec![d], true, Init::Uniform(0.8, 1.2))?;
                    first = bk.f(Op::ScaleParam, vec![first, sp], d, "altup.scaled_first");
                }
                let pv = self.ple(&mut bk, p)?;
                let g = self.linear(&mut bk, first, "ple.gate", p.dim, d, false, true, "ple.gate")?;
                let g = bk.f(Op::Act(plain_act(p.act, "a per-layer embedding gate")?), vec![g], p.dim, "ple.act");
                let gm = bk.f(Op::Mul, vec![g, pv], p.dim, "ple.gated");
                let o = self.linear(&mut bk, gm, "ple.out", d, p.dim, false, true, "ple.out")?;
                Some(self.full_norm(&mut bk, o, p.post_norm, "ple.post_norm", d, true)?)
            }
            None => None,
        };
        let mut parts = vec![c0s];
        for i in 1..k {
            let ci = bk.st(Op::Slice { start: i * d, len: d }, vec![cor], d);
            parts.push(match y {
                Some(y) => bk.f(Op::Add, vec![ci, y], d, &format!("altup.stream{i}")),
                None => ci,
            });
        }
        parts.push(c0s);
        let out = bk.st(Op::Concat, parts, n + d);
        let outputs = self.layer_outputs(out);
        self.blocks.push(Block { name: format!("{name}.ffn"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
        Ok(vec![first, self.blocks.len() - 1])
    }

    fn mhc(&self) -> Result<MhcSpec> {
        self.s.mhc.ok_or_else(|| LowerError::not_lowerable("RESIDUAL_MHC_SINKHORN_V1: an mHC layer needs the model's streams (ModelSpec::mhc)"))
    }

    /// The carry's lanes past the streams are the layer's own: a block's outputs packed in order and zero-padded to the carry's width.
    fn mhc_pack(&mut self, bk: &mut Bk, parts: Vec<(Ref, usize)>) -> Result<Ref> {
        let width = self.carries[0].shape[0];
        let used: usize = parts.iter().map(|(_, n)| *n).sum();
        if used > width {
            return Err(LowerError::eval(format!("internal: a block hands on {used} lanes in a carry of {width}")));
        }
        let mut parts = parts;
        if used < width {
            let pad = width - used;
            parts.push((bk.st(Op::Zeros, vec![], pad), pad));
        }
        // A node takes at most eight inputs: a longer list is a tree of concatenations.
        Ok(self.concat_tree(bk, parts).0)
    }

    /// One site of an mHC layer: the mixing weights of the streams `h` (`pre`, `post`, `comb`), the collapse of the streams by `pre`, and the
    /// norm of the collapsed row. Returns `(post, comb, normed)`.
    fn mhc_site(&mut self, bk: &mut Bk, h: Ref, tag: &str, norm: NormSpec, norm_name: &str) -> Result<(Ref, Ref, Ref)> {
        let mh = self.mhc()?;
        let (hc, d) = (mh.streams, self.s.hidden_size);
        let n = hc * d;
        let mix = (2 + hc) * hc;
        let flat_norm = NormSpec { kind: NormKind::Rms, eps: mh.norm_eps, gain: Gain::None, bias: false };
        let flat = self.norm(bk, h, flat_norm, &format!("mhc.{tag}.flat"), n, 1, vec![n], true, &format!("mhc.{tag}.flat"))?;
        let mixes = self.linear(bk, flat, &format!("mhc.{tag}.fn"), mix, n, false, true, &format!("mhc.{tag}.mixes"))?;
        let base = self.param(&format!("mhc.{tag}.base"), vec![mix], true, Init::Uniform(-0.5, 0.5))?;
        let scale = self.param(&format!("mhc.{tag}.scale"), vec![3], true, Init::Uniform(0.5, 1.5))?;
        let map = bk.push(
            Op::MhcMap { streams: hc, iters: mh.iters, eps: mh.eps },
            vec![mixes, base, scale],
            vec![vec![hc], vec![hc], vec![hc * hc]],
            vec![HlType::F32, HlType::F32, HlType::F32],
            Some(&format!("mhc.{tag}.map")),
            vec![],
        );
        let (pre, post, comb) = (Ref::Node(map, 0), Ref::Node(map, 1), Ref::Node(map, 2));
        let col = bk.f(Op::StreamMix { n_in: hc, n_out: 1, transpose: false }, vec![h, pre], d, &format!("mhc.{tag}.collapsed"));
        let xn = self.full_norm(bk, col, norm, norm_name, d, true)?;
        Ok((post, comb, xn))
    }

    /// The site's output joins every stream: `post ⊗ o + comb^T·h`.
    fn mhc_apply(&mut self, bk: &mut Bk, h: Ref, post: Ref, comb: Ref, o: Ref, tag: &str) -> Result<Ref> {
        let hc = self.mhc()?.streams;
        let n = hc * self.s.hidden_size;
        let xo = bk.f(Op::StreamOuter { streams: hc }, vec![o, post], n, &format!("mhc.{tag}.delta"));
        let hm = bk.f(Op::StreamMix { n_in: hc, n_out: hc, transpose: true }, vec![h, comb], n, &format!("mhc.{tag}.mixed"));
        Ok(bk.f(Op::Add, vec![xo, hm], n, &format!("mhc.{tag}.out")))
    }

    /// A concatenation of any number of values as a tree of at most eight (a node takes eight inputs).
    fn concat_tree(&mut self, bk: &mut Bk, mut parts: Vec<(Ref, usize)>) -> (Ref, usize) {
        while parts.len() > 8 {
            parts = parts
                .chunks(8)
                .map(|c| {
                    if c.len() == 1 {
                        c[0]
                    } else {
                        let n: usize = c.iter().map(|(_, n)| *n).sum();
                        (bk.st(Op::Concat, c.iter().map(|(r, _)| *r).collect(), n), n)
                    }
                })
                .collect();
        }
        let n: usize = parts.iter().map(|(_, n)| *n).sum();
        if parts.len() == 1 { parts[0] } else { (bk.st(Op::Concat, parts.iter().map(|(r, _)| *r).collect(), n), n) }
    }

    /// **`RESIDUAL_MHC_SINKHORN_V1`** over a shared-KV attention (DeepSeek-V4): a layer is three blocks. The first reads the mixer site's mixing
    /// weights, collapses the streams, norms the row and makes the query; the second is the attention proper (the key/value row, the
    /// compressed entries and their indexer, the softmax over the window and the entries, the grouped output projection) and joins its output
    /// into the streams; the third is the FFN site. What a block hands the next rides in the carry past the streams.
    fn mhc_blocks(&mut self, ls: &LayerSpec, kind_index: usize) -> Result<Vec<usize>> {
        let Residual::Mhc { pre_mixer, pre_ffn } = &ls.residual else {
            return Err(LowerError::eval("internal: mhc_blocks of a layer that is not mHC"));
        };
        let Mixer::SharedKv(a) = &ls.mixer else {
            return Err(LowerError::not_lowerable("RESIDUAL_MHC_SINKHORN_V1 is modelled over the shared-KV attention (ATTN_KV_SHARED_ROTATED_V1) only"));
        };
        if ls.ffn == Ffn::None || matches!(ls.ffn, Ffn::MlpMoe(_)) {
            return Err(LowerError::not_lowerable("RESIDUAL_MHC_SINKHORN_V1: a layer is a mixer and an FFN (Mlp or Moe)"));
        }
        let mh = self.mhc()?;
        let (hc, d) = (mh.streams, self.s.hidden_size);
        let n = hc * d;
        let (heads, hd, lat) = (a.heads, a.head_dim, a.q_rank);
        let name = format!("{}{}", block_name(ls), if kind_index > 0 { format!("#{kind_index}") } else { String::new() });
        // ── block 1: the mixer site's weights, the collapse, the query ──
        let mut bk = Bk::default();
        let h = bk.st(Op::Slice { start: 0, len: n }, vec![Ref::Carry(0)], n);
        let (post, comb, xn) = self.mhc_site(&mut bk, h, "attn", *pre_mixer, "norm.mix")?;
        let qa = self.linear(&mut bk, xn, "attn.wq_a", lat, d, false, true, "attn.q_a")?;
        let qr = self.full_norm(&mut bk, qa, a.q_a_norm, "attn.q_a_norm", lat, true)?;
        let qb = self.linear(&mut bk, qr, "attn.wq_b", heads * hd, lat, false, true, "attn.q_b")?;
        let q = self.norm(&mut bk, qb, a.q_b_norm, "attn.q_b_norm", heads * hd, heads, vec![hd], true, "attn.q_b_norm")?;
        let tq = self.rope_table(&a.rope.freqs);
        let q = bk.f(
            Op::Rope { heads, head_dim: hd, rotary_dim: a.rope.rotary_dim, offset: a.rope.offset, style: a.rope.style, table: tq },
            vec![q, Ref::Pos],
            heads * hd,
            "attn.q_rope",
        );
        let out = self.mhc_pack(&mut bk, vec![(h, n), (post, hc), (comb, hc * hc), (xn, d), (qr, lat), (q, heads * hd)])?;
        let outputs = self.layer_outputs(out);
        self.blocks.push(Block { name: format!("{name}.mhc_attn"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
        let first = self.blocks.len() - 1;
        // ── block 2 (compressed layers): the compressors and the indexer's query — the states they write and the rows they hand on ──
        let mut blocks_out = vec![first];
        let comp_in = if let Some(c) = &a.compressed {
            let mut bk = Bk::default();
            let mut at = 0usize;
            let h = mhc_take(&mut bk, &mut at, n, None);
            let post = mhc_take(&mut bk, &mut at, hc, Some("carry.post"));
            let comb = mhc_take(&mut bk, &mut at, hc * hc, Some("carry.comb"));
            let xn = mhc_take(&mut bk, &mut at, d, Some("carry.xn"));
            // The query's latent is read only by the indexer (a node nothing reads is dead, which normal form forbids).
            let qr = if c.indexer.is_some() {
                Some(mhc_take(&mut bk, &mut at, lat, Some("carry.qr")))
            } else {
                at += lat;
                None
            };
            let q = mhc_take(&mut bk, &mut at, heads * hd, Some("carry.q"));
            let ci = self.shared_kv_comp(&mut bk, a, c, xn, qr)?;
            let mut parts = vec![(h, n), (post, hc), (comb, hc * hc), (xn, d), (q, heads * hd), (ci.ent, hd)];
            if let (Some(ix), Some((ient, iq, iw))) = (&c.indexer, ci.index) {
                parts.extend([(ient, ix.head_dim), (iq, ix.heads * ix.head_dim), (iw, ix.heads)]);
            }
            let out = self.mhc_pack(&mut bk, parts)?;
            let outputs = self.layer_outputs(out);
            self.blocks.push(Block { name: format!("{name}.comp"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
            blocks_out.push(self.blocks.len() - 1);
            true
        } else {
            false
        };
        // ── block 3: the attention proper ──
        let mut bk = Bk::default();
        let mut at = 0usize;
        let h = mhc_take(&mut bk, &mut at, n, None);
        // What a value was before it was packed keeps its own calibration: a sited identity (no node in the integer program) says which
        // statistics narrow it to codes — not the whole carry's, whose range is the streams'.
        let post = mhc_take(&mut bk, &mut at, hc, Some("carry.post"));
        let comb = mhc_take(&mut bk, &mut at, hc * hc, Some("carry.comb"));
        let xn = mhc_take(&mut bk, &mut at, d, Some("carry.xn"));
        let comp = if comp_in {
            let q = mhc_take(&mut bk, &mut at, heads * hd, Some("carry.q"));
            let ent = mhc_take(&mut bk, &mut at, hd, Some("carry.ent"));
            let index = match a.compressed.as_ref().and_then(|c| c.indexer.as_ref()) {
                Some(ix) => {
                    let ient = mhc_take(&mut bk, &mut at, ix.head_dim, Some("carry.ient"));
                    let iq = mhc_take(&mut bk, &mut at, ix.heads * ix.head_dim, Some("carry.iq"));
                    let iw = mhc_take(&mut bk, &mut at, ix.heads, Some("carry.iw"));
                    Some((ient, iq, iw))
                }
                None => None,
            };
            (q, Some(CompRows { ent, index }))
        } else {
            at += lat;
            let q = mhc_take(&mut bk, &mut at, heads * hd, Some("carry.q"));
            (q, None)
        };
        let (q, rows) = comp;
        let attn_out = self.shared_kv_core(&mut bk, a, xn, q, rows)?;
        let h1 = self.mhc_apply(&mut bk, h, post, comb, attn_out, "attn")?;
        let out = self.mhc_pack(&mut bk, vec![(h1, n)])?;
        let outputs = self.layer_outputs(out);
        self.blocks.push(Block { name: format!("{name}.attn"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
        blocks_out.push(self.blocks.len() - 1);
        // ── block 3: the FFN site ──
        let mut bk = Bk::default();
        let h = bk.st(Op::Slice { start: 0, len: n }, vec![Ref::Carry(0)], n);
        let (post, comb, xn) = self.mhc_site(&mut bk, h, "ffn", *pre_ffn, "norm.ffn")?;
        let f = self.ffn(&mut bk, &ls.ffn, xn)?;
        let h2 = self.mhc_apply(&mut bk, h, post, comb, f, "ffn")?;
        let out = self.mhc_pack(&mut bk, vec![(h2, n)])?;
        let outputs = self.layer_outputs(out);
        self.blocks.push(Block { name: format!("{name}.ffn"), role: BlockRole::Layer, nodes: bk.nodes, outputs });
        blocks_out.push(self.blocks.len() - 1);
        Ok(blocks_out)
    }

    /// One compressor (`ATTN_COMPRESSED_KV_V1`) over the normed input `x`: the window buffers, the pooled entry of the window just closing,
    /// its norm and rotation, and the store of entries. Returns `(entry row, store state)`; the entry row is written to the store at the
    /// end of the window by [`Op::BlockWrite`] (which this call has NOT emitted: the indexer reads the store before its own write).
    #[allow(clippy::too_many_arguments)]
    fn compressor(
        &mut self,
        bk: &mut Bk,
        c: &CompressedKvSpec,
        dim: usize,
        x: Ref,
        pfx: &str,
        norm_name: &str,
        norm: NormSpec,
        rope: &crate::rope::RopeSpec,
    ) -> Result<Ref> {
        let d = self.s.hidden_size;
        let m = c.ratio;
        let cin = dim * if c.overlap { 2 } else { 1 };
        let ckv = self.linear(bk, x, &format!("{pfx}.wkv"), cin, d, false, true, &format!("{pfx}.kv"))?;
        let cg = self.linear(bk, x, &format!("{pfx}.wgate"), cin, d, false, true, &format!("{pfx}.gate"))?;
        let kvb = self.state(&format!("{pfx}.kv_buf"), StateKind::Fixed, vec![m, cin], 0.0)?;
        let gb = self.state(&format!("{pfx}.gate_buf"), StateKind::Fixed, vec![m, cin], 0.0)?;
        bk.push(Op::WindowWrite { ratio: m }, vec![ckv, kvb, Ref::Pos], vec![vec![]], vec![HlType::Unit], None, vec![sid(kvb)]);
        bk.push(Op::WindowWrite { ratio: m }, vec![cg, gb, Ref::Pos], vec![vec![]], vec![HlType::Unit], None, vec![sid(gb)]);
        let ape = self.param(&format!("{pfx}.ape"), vec![m, cin], true, Init::Normal(0.5))?;
        let mut ins = vec![kvb, gb, ape, Ref::Pos];
        let mut writes = vec![];
        if c.overlap {
            let pk = self.state(&format!("{pfx}.prev_kv"), StateKind::Fixed, vec![m, dim], 0.0)?;
            let pg = self.state(&format!("{pfx}.prev_gate"), StateKind::Fixed, vec![m, dim], 0.0)?;
            ins.extend([pk, pg]);
            writes = vec![sid(pk), sid(pg)];
        }
        let pooled = Ref::Node(
            bk.push(
                Op::WindowPool { ratio: m, dim, overlap: c.overlap },
                ins,
                vec![vec![dim]],
                vec![HlType::F32],
                Some(&format!("{pfx}.pooled")),
                writes,
            ),
            0,
        );
        let ent = self.full_norm(bk, pooled, norm, norm_name, dim, true)?;
        let t = self.rope_table(&rope.freqs);
        Ok(bk.f(
            Op::RopeAtBlock {
                heads: 1,
                head_dim: dim,
                rotary_dim: rope.rotary_dim,
                offset: rope.offset,
                style: rope.style,
                table: t,
                ratio: m,
            },
            vec![ent, Ref::Pos],
            dim,
            &format!("{pfx}.entry"),
        ))
    }

    /// The compressors of a compressed shared-KV layer (the first of the layer's attention blocks): the entry row of the window just closing
    /// and — with an indexer — its key row, its query and its head weights.
    fn shared_kv_comp(&mut self, bk: &mut Bk, a: &SharedKvSpec, c: &CompressedKvSpec, xn: Ref, qr: Option<Ref>) -> Result<CompRows> {
        let (hd, d) = (a.head_dim, self.s.hidden_size);
        // CSA and HCA layers differ in the widths of their compressors: params and states of their own.
        let pfx = if c.overlap { "attn.csa" } else { "attn.hca" };
        let ent = self.compressor(bk, c, hd, xn, pfx, &format!("{pfx}.norm"), c.norm, &c.rope)?;
        let index = match &c.indexer {
            Some(ix) => {
                let (hi, di) = (ix.heads, ix.head_dim);
                let ient = self.compressor(bk, c, di, xn, "attn.idx.comp", "attn.idx.comp.norm", ix.norm, &ix.rope)?;
                let qr = qr.ok_or_else(|| LowerError::eval("internal: an indexer without the query's latent"))?;
                let iq = self.linear(bk, qr, "attn.idx.wq_b", hi * di, a.q_rank, false, true, "attn.idx.q")?;
                let ti = self.rope_table(&ix.rope.freqs);
                let iq = bk.f(
                    Op::Rope { heads: hi, head_dim: di, rotary_dim: ix.rope.rotary_dim, offset: ix.rope.offset, style: ix.rope.style, table: ti },
                    vec![iq, Ref::Pos],
                    hi * di,
                    "attn.idx.q_rope",
                );
                let iw = self.linear(bk, xn, "attn.idx.weights_proj", hi, d, false, true, "attn.idx.w")?;
                let iw = bk.f(Op::Scale { c: (hi as f64).powf(-0.5) }, vec![iw], hi, "attn.idx.w_scaled");
                Some((ient, iq, iw))
            }
            None => None,
        };
        Ok(CompRows { ent, index })
    }

    /// The attention of a shared-KV layer from the normed input `xn`, the rotated query `q` and — in a compressed layer — the rows its
    /// compressors made: the output `[D]`.
    fn shared_kv_core(&mut self, bk: &mut Bk, a: &SharedKvSpec, xn: Ref, q: Ref, comp: Option<CompRows>) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (heads, hd) = (a.heads, a.head_dim);
        // The one key/value head: normed, rotated, appended to the window's history.
        let kv = self.linear(bk, xn, "attn.wkv", hd, d, false, true, "attn.kv")?;
        let kv = self.full_norm(bk, kv, a.kv_norm, "attn.kv_norm", hd, true)?;
        let tk = self.rope_table(&a.rope.freqs);
        let kv = bk.f(
            Op::Rope { heads: 1, head_dim: hd, rotary_dim: a.rope.rotary_dim, offset: a.rope.offset, style: a.rope.style, table: tk },
            vec![kv, Ref::Pos],
            hd,
            "attn.kv_rope",
        );
        let hist = self.state(&format!("attn.kv_hist.w{}", a.window), StateKind::Hist { window: Some(a.window) }, vec![hd], 0.0)?;
        bk.append(kv, hist);
        let sinks = if a.sinks { Some(self.param("attn.sinks", vec![heads], true, Init::Normal(1.0))?) } else { None };
        let ctx = match (&a.compressed, comp) {
            (None, _) => {
                let mut ins = vec![q, hist, hist];
                if let Some(s) = sinks {
                    ins.push(s);
                }
                let op = Op::Attention {
                    heads,
                    kv_heads: 1,
                    head_dim: hd,
                    v_head_dim: hd,
                    scale: a.scale,
                    softcap: None,
                    window: Some(a.window),
                    alibi: None,
                    sinks: a.sinks,
                    chunk: None,
                    blocks: None,
                };
                bk.f(op, ins, heads * hd, "attn.ctx")
            }
            (Some(c), Some(rows)) => {
                let sinks = sinks.ok_or_else(|| LowerError::not_lowerable("ATTN_COMPRESSED_KV_V1: the softmax over the window and the entries carries the per-head sink"))?;
                let m = c.ratio;
                let blocks = self.s.max_position_embeddings.unwrap_or(self.s.hidden_size).div_ceil(m).max(1);
                if blocks > 1 << 22 {
                    return Err(LowerError::not_lowerable(format!("ATTN_COMPRESSED_KV_V1: {blocks} entries")));
                }
                let pfx = if c.overlap { "attn.csa" } else { "attn.hca" };
                let store = self.state(&format!("{pfx}.entries"), StateKind::Fixed, vec![blocks, hd], 0.0)?;
                bk.push(Op::BlockWrite { ratio: m, blocks }, vec![rows.ent, store, Ref::Pos], vec![vec![]], vec![HlType::Unit], None, vec![sid(store)]);
                let mut ins = vec![q, hist, store, sinks, Ref::Pos];
                let mut select = false;
                if let (Some(ix), Some((ient, iq, iw))) = (&c.indexer, rows.index) {
                    let di = ix.head_dim;
                    let keys = self.state("attn.idx.keys", StateKind::Fixed, vec![blocks, di], 0.0)?;
                    let ids = Ref::Node(
                        bk.push(
                            Op::EntrySelect { heads: ix.heads, dim: di, ratio: m, blocks, top: ix.topk },
                            vec![iq, iw, ient, keys, Ref::Pos],
                            vec![vec![ix.topk]],
                            vec![HlType::Idx],
                            None,
                            vec![],
                        ),
                        0,
                    );
                    bk.push(Op::BlockWrite { ratio: m, blocks }, vec![ient, keys, Ref::Pos], vec![vec![]], vec![HlType::Unit], None, vec![sid(keys)]);
                    ins.push(ids);
                    select = true;
                }
                bk.f(Op::EntryAttention { heads, head_dim: hd, ratio: m, blocks, scale: a.scale, select }, ins, heads * hd, "attn.ctx")
            }
            (Some(_), None) => return Err(LowerError::eval("internal: a compressed layer's attention block without the rows of its compressors")),
        };
        // K = V carries the rotation: the output's rope slice is rotated back by the query's position.
        let tb = self.rope_table(&a.rope_back.freqs);
        let o = bk.f(
            Op::Rope { heads, head_dim: hd, rotary_dim: a.rope_back.rotary_dim, offset: a.rope_back.offset, style: a.rope_back.style, table: tb },
            vec![ctx, Ref::Pos],
            heads * hd,
            "attn.ctx_back",
        );
        // The block-diagonal low-rank projection: group `g` of the heads' lanes to `o_rank`, then one projection to the hidden width.
        let g = a.o_groups;
        if g == 0 || (heads * hd) % g != 0 {
            return Err(LowerError::not_lowerable(format!("ATTN_OUT_GROUPED_LOWRANK_V1: {g} groups over {} lanes", heads * hd)));
        }
        let per = heads * hd / g;
        let mut parts = Vec::with_capacity(g);
        for gi in 0..g {
            let sl = bk.st(Op::Slice { start: gi * per, len: per }, vec![o], per);
            let y = self.linear(bk, sl, &format!("attn.wo_a{gi}"), a.o_rank, per, false, true, &format!("attn.o_a{gi}"))?;
            parts.push((y, a.o_rank));
        }
        let (cat, width) = self.concat_tree(bk, parts);
        self.linear(bk, cat, "attn.wo_b", d, width, false, true, "attn.out")
    }

    fn layer_block(&mut self, ls: &LayerSpec, kind_index: usize) -> Result<usize> {
        let d = self.s.hidden_size;
        if ls.pre_branch.is_some() && !matches!(ls.residual, Residual::Sequential { .. }) {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1 is modelled under the sequential pre-norm residual only"));
        }
        if matches!(ls.ffn, Ffn::MlpShortcut(_)) && !matches!(ls.residual, Residual::Sequential { .. }) {
            return Err(LowerError::not_lowerable("FFN_SHORTCUT_MOE_V1 is lowered under a sequential pre-norm residual only"));
        }
        let mut bk = Bk::default();
        let x = Ref::Carry(0);
        let mut h = match &ls.residual {
            Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } => {
                // `LAYER_FFN_ONLY_V1`: no mixer — the layer is `x + ffn(norm(x))`.
                let mut h = if matches!(ls.mixer, Mixer::None) {
                    if ls.ffn == Ffn::None || pre_mixer.is_some() || post_mixer.is_some() {
                        return Err(LowerError::eval("internal: a layer with no mixer needs an FFN and no mixer norms"));
                    }
                    x
                } else {
                    // `LAYER_PRE_BRANCH_V1`: the mixer reads `norm(h + branch)`; the residual add below keeps `h`.
                    let xin = match &ls.pre_branch {
                        Some(pb) => {
                            if pre_mixer.is_none() {
                                return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1: the mixer's input norm is where the branch joins"));
                            }
                            let t = self.pre_branch(&mut bk, pb, x)?;
                            bk.f(Op::Add, vec![x, t], d, "pb.sum")
                        }
                        None => x,
                    };
                    let n1 = match pre_mixer {
                        Some(n) => self.full_norm(&mut bk, xin, *n, "norm.mix", d, true)?,
                        None => xin,
                    };
                    let mut m = self.mixer(&mut bk, &ls.mixer, n1)?;
                    if let Some(n) = post_mixer {
                        m = self.full_norm(&mut bk, m, *n, "norm.post_mix", d, true)?;
                    }
                    if *multiplier != 1.0 {
                        m = bk.f(Op::Scale { c: *multiplier }, vec![m], d, "mix.scaled");
                    }
                    // `ATTN_CROSS_V1`: the attention branch enters through `tanh(attn_gate)`.
                    if matches!(&ls.mixer, Mixer::CrossAttention(c) if c.gated) {
                        let g = self.param("xattn.attn_gate", vec![1], true, Init::Uniform(0.3, 0.9))?;
                        m = bk.f(Op::ScaleParam, vec![m, g], d, "xattn.attn_gated");
                    }
                    bk.f(Op::Add, vec![x, m], d, "resid.mix")
                };
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
                    if matches!(&ls.mixer, Mixer::CrossAttention(c) if c.gated) {
                        let g = self.param("xattn.mlp_gate", vec![1], true, Init::Uniform(0.3, 0.9))?;
                        f = bk.f(Op::ScaleParam, vec![f, g], d, "xattn.mlp_gated");
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
                    let g = bk.f(Op::Act(plain_act(p.act, "a per-layer embedding gate")?), vec![g], p.dim, "ple.act");
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
            Residual::AltUp { .. } => return Err(LowerError::eval("internal: an AltUp layer is built by `altup_blocks`")),
            Residual::Mhc { .. } => return Err(LowerError::eval("internal: an mHC layer is built by `mhc_blocks`")),
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

    /// **`LAYER_PRE_BRANCH_V1`** (Zamba2's shared transformer): `t = out(mlp(mid_norm(attn(in_norm(concat[h, e0])))))`, no residual inside.
    /// The weights are the group's (global params `pb{group}.*`, `ATTN_SHARED_BLOCK_V1`); the adapters (`LINEAR_LOWRANK_ADAPTER_V1`), the KV
    /// history and the `out` projection are this layer's.
    fn pre_branch(&mut self, bk: &mut Bk, pb: &PreBranch, h: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let Some(e0) = self.e0 else {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1 reads the embedding carry (EMBED_CARRY_V1), and the model has none"));
        };
        if pb.attn.in_dim != Some(2 * d) {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1: the attention reads [hidden | embedding], in_dim = 2 * hidden"));
        }
        if pb.attn.kv_share.is_some() || pb.attn.sparse.is_some() || pb.attn.output_gate || pb.attn.gate.is_some() {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1: a plain attention (no KV sharing, sparse blocks or output gate)"));
        }
        if !pb.mlp.gated || pb.mlp.inner_norm.is_some() || pb.mlp.act == Act::Xielu || !matches!(pb.mlp.glu, Glu::Standard) {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1: a plain gated MLP"));
        }
        let mut attn = pb.attn.clone();
        attn.param_prefix = Some(pb.attn_prefix());
        let mut mlp = pb.mlp.clone();
        mlp.name = Some(pb.mlp_name());
        let (apfx, mpfx) = (pb.attn_prefix(), pb.mlp_name());
        let base = pb.adapter_base();
        self.sharing = true;
        self.lowrank.clear();
        if let Some(lr) = pb.lowrank {
            if lr.rank == 0 {
                return Err(LowerError::bad("LINEAR_LOWRANK_ADAPTER_V1: a rank of zero"));
            }
            if lr.attn {
                for r in ["q", "k", "v"] {
                    self.lowrank.insert(format!("{apfx}.{r}"), (lr.rank, format!("{base}.attn.{r}")));
                }
            }
            for r in ["gate", "up"] {
                self.lowrank.insert(format!("{mpfx}.{r}"), (lr.rank, format!("{base}.mlp.{r}")));
            }
        }
        let built = (|| -> Result<Ref> {
            // `h` and `e0` side by side, the hidden state first. `pb.h` is a site of its own so the row has a scale to be coded at.
            let hs = bk.f(Op::Scale { c: 1.0 }, vec![h], d, "pb.h");
            let u = bk.f(Op::Concat, vec![hs, Ref::Carry(e0 as u8)], 2 * d, "pb.concat");
            let un = self.norm(bk, u, pb.in_norm, &pb.in_norm_name(), 2 * d, 1, vec![2 * d], true, "pb.in_norm")?;
            let a = self.attention(bk, &attn, un)?;
            let m = self.norm(bk, a, pb.mid_norm, &pb.mid_norm_name(), d, 1, vec![d], true, "pb.mid_norm")?;
            self.mlp(bk, &mlp, m, &mpfx)
        })();
        self.sharing = false;
        self.lowrank.clear();
        let f = built?;
        // The layer's own projection of the branch (`Zamba2HybridLayer.linear`).
        self.linear(bk, f, "pb.out", d, d, false, true, "pb.out")
    }

    fn mixer(&mut self, bk: &mut Bk, m: &Mixer, x: Ref) -> Result<Ref> {
        match m {
            Mixer::Attention(a) => self.attention(bk, a, x),
            Mixer::Mla(a) => self.mla(bk, a, x),
            Mixer::GatedDeltaNet(g) => self.gdn(bk, g, x),
            Mixer::Kda(k) => self.kda(bk, k, x),
            Mixer::Mamba(mm) => self.mamba(bk, mm, x),
            Mixer::Mamba2(mm) => self.mamba2(bk, mm, x),
            Mixer::RwkvTime(r) => self.rwkv_time(bk, r, x),
            Mixer::ShortConv(c) => self.short_conv(bk, c, x),
            Mixer::None => Err(LowerError::eval("internal: a mixer was asked of a layer with none")),
            Mixer::Parallel(branches) => self.parallel(bk, branches, x),
            Mixer::CrossAttention(c) => self.cross_attention(bk, c, x),
            Mixer::SharedKv(_) => Err(LowerError::eval("internal: a shared-KV attention is built by its layer's blocks (`mhc_blocks`)")),
        }
    }

    /// **`MIXER_PARALLEL_BRANCH_V1`**: `Σ out_scale · mixer(in_scale · x)` over the branches, in order. The scales are
    /// `Op::Scale` — a change of the value's scale key, not a node of the integer program. A branch of a kind a layer already has would share
    /// its param names, so at most one of each is accepted; KV sharing across layers does not reach into a branch.
    fn parallel(&mut self, bk: &mut Bk, branches: &[Branch], x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        if branches.len() < 2 || branches.len() > 4 {
            return Err(LowerError::not_lowerable(format!("MIXER_PARALLEL_BRANCH_V1: {} branches (2 to 4 are modelled)", branches.len())));
        }
        let kind = |m: &Mixer| std::mem::discriminant(m);
        for (i, b) in branches.iter().enumerate() {
            if matches!(b.mixer, Mixer::None | Mixer::Parallel(_)) {
                return Err(LowerError::not_lowerable("MIXER_PARALLEL_BRANCH_V1: a branch is a mixer, not an empty layer or another parallel mix"));
            }
            if matches!(&b.mixer, Mixer::Attention(a) if a.kv_share.is_some()) {
                return Err(LowerError::not_lowerable("MIXER_PARALLEL_BRANCH_V1: KV sharing inside a branch"));
            }
            if branches[..i].iter().any(|o| kind(&o.mixer) == kind(&b.mixer)) {
                return Err(LowerError::not_lowerable("MIXER_PARALLEL_BRANCH_V1: two branches of one kind share their params"));
            }
        }
        let mut sum: Option<Ref> = None;
        for (i, b) in branches.iter().enumerate() {
            let xi = if b.in_scale != 1.0 { bk.f(Op::Scale { c: b.in_scale }, vec![x], d, &format!("branch{i}.in")) } else { x };
            let mut y = self.mixer(bk, &b.mixer, xi)?;
            if b.out_scale != 1.0 {
                y = bk.f(Op::Scale { c: b.out_scale }, vec![y], d, &format!("branch{i}.out"));
            }
            sum = Some(match sum {
                None => y,
                Some(acc) => bk.f(Op::Add, vec![acc, y], d, &format!("branches.{i}")),
            });
        }
        sum.ok_or_else(|| LowerError::eval("internal: no branches"))
    }

    fn ffn(&mut self, bk: &mut Bk, f: &Ffn, x: Ref) -> Result<Ref> {
        match f {
            Ffn::None => Err(LowerError::eval("internal: ffn of a layer without one")),
            Ffn::Mlp(m) => self.mlp(bk, m, x, m.name.as_deref().unwrap_or("mlp")),
            Ffn::Moe(m) => self.moe(bk, m, x),
            Ffn::RwkvChannel(c) => self.rwkv_channel(bk, c, x),
            Ffn::MlpMoe(_) => Err(LowerError::eval("internal: an MLP+MoE block outside a sandwich layer")),
            Ffn::MlpShortcut(sc) => {
                let d = self.s.hidden_size;
                let side = self
                    .carries
                    .iter()
                    .position(|c| c.name == "side")
                    .ok_or_else(|| LowerError::eval("internal: a shortcut layer without the side carry"))?;
                let f = self.mlp(bk, &sc.mlp, x, sc.mlp.name.as_deref().unwrap_or("mlp"))?;
                match &sc.side {
                    // The MoE reads the same normed vector as the dense MLP; its output is carried, not added.
                    ShortcutSide::Produce(m) => {
                        let s = self.moe(bk, m, x)?;
                        self.carry_out.insert(side, s);
                        Ok(f)
                    }
                    ShortcutSide::Consume => Ok(bk.f(Op::Add, vec![f, Ref::Carry(side as u8)], d, "ffn.side")),
                }
            }
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
        // `ATTN_SHARED_BLOCK_V1`: the projections read a wider input (`[hidden | embedding]`) and `o` returns to the hidden width.
        let din = a.in_dim.unwrap_or(d);
        let (h, kv, hd, vd) = (a.heads, a.kv_heads, a.head_dim, a.v_head_dim);
        let (qn, kn, vn) = (h * hd, kv * hd, kv * vd);
        let pf = a.param_prefix.clone().unwrap_or_else(|| "attn".into());
        let n = |s: &str| format!("{pf}.{s}");
        // `ATTN_DIFFERENTIAL_V1`: the values are double wide in the history, the context is combined before anything reads it.
        if a.differential.is_some() {
            if h % 2 != 0 || kv % 2 != 0 || kv == 0 || vd != hd || a.sinks || a.sparse.is_some() || a.kv_share.is_some() || a.output_gate {
                return Err(LowerError::not_lowerable(format!(
                    "ATTN_DIFFERENTIAL_V1 needs an even number of heads and of kv heads and values as wide as keys ({h} heads, {kv} kv heads, {hd}/{vd}), and has no sinks, sparse blocks, KV sharing or fused output gate"
                )));
            }
        }
        if let Some(moa) = &a.moa {
            return self.moa_attention(bk, a, moa, x);
        }
        let vw = if a.differential.is_some() { 2 * vd } else { vd };
        let mut q = self.linear(bk, x, &n("q"), qn, din, a.q_bias, true, &n("q"))?;
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
            let mut o = bk.f(op, vec![q, ks, vs], h * vd, &n("ctx"));
            if let Some(on) = a.o_norm {
                o = self.full_norm(bk, o, on, &n("sub_norm"), h * vd, true)?;
            }
            return self.linear(bk, o, &n("o"), d, h * vd, a.o_bias, true, &n("out"));
        }
        let mut k = self.linear(bk, x, &n("k"), kn, din, a.k_bias, true, &n("k"))?;
        // Gemma-4's `attention_k_eq_v`: the values are the raw key projection.
        let mut v = if a.v_from_k {
            if vd != hd {
                return Err(LowerError::eval("internal: values from keys of another width"));
            }
            k
        } else {
            self.linear(bk, x, &n("v"), vn, din, a.v_bias, true, &n("v"))?
        };
        let gate = if a.output_gate { Some(self.linear(bk, x, &n("gate"), qn, din, false, true, &n("gate"))?) } else { None };
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
                let gl = self.linear(bk, x, &n("gate"), width, din, false, true, &n("gate"))?;
                let ga = bk.f(Op::Act(plain_act(g.act, "an attention output gate")?), vec![gl], width, &n("gate_act"));
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
        // `ATTN_DIFFERENTIAL_V1`: kv head `j` reads the value `[V_{j mod m} | V_{j mod m + m}]`, `m = Hkv/2` (HF's two
        // `repeat(1, 2, ..)` of the value halves, then `repeat_kv`): head `i` and head `i + H/2` land on the same pair.
        let vn = if a.differential.is_some() {
            let m = kv / 2;
            let piece = |bk: &mut Bk, from: usize| (bk.st(Op::Slice { start: from * hd, len: hd }, vec![v], hd), hd);
            let mut parts: Vec<(Ref, usize)> = Vec::with_capacity(4 * m);
            for _ in 0..2 {
                for j in 0..m {
                    parts.push(piece(bk, j));
                    parts.push(piece(bk, m + j));
                }
            }
            // A node takes at most 8 inputs (NF-14): a tree of concatenations.
            while parts.len() > 1 {
                parts = parts
                    .chunks(8)
                    .map(|c| {
                        if c.len() == 1 {
                            c[0]
                        } else {
                            let w: usize = c.iter().map(|p| p.1).sum();
                            (bk.st(Op::Concat, c.iter().map(|p| p.0).collect(), w), w)
                        }
                    })
                    .collect();
            }
            v = parts[0].0;
            kv * vw
        } else {
            vn
        };
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
            v_head_dim: vw,
            scale: a.scale,
            softcap: a.softcap,
            window: a.window,
            alibi,
            sinks: a.sinks,
            chunk: a.chunk,
            blocks,
        };
        let mut o = bk.f(op, ins, h * vw, &n("ctx"));
        // `ATTN_DIFFERENTIAL_V1`: `(1 − λ_init)·RMS_{2d}(o_first_half_of_heads − λ·o_second_half)`, the subtraction on the
        // attention's own codes (one narrowing of each half, then the exact difference).
        if let Some(df) = &a.differential {
            let hw = (h / 2) * vw;
            let first = bk.st(Op::Slice { start: 0, len: hw }, vec![o], hw);
            let second = bk.st(Op::Slice { start: hw, len: hw }, vec![o], hw);
            let lam = self.param(&n("diff.lambda"), vec![1], true, Init::Uniform(0.2, 0.9))?;
            let scaled = bk.f(Op::ScaleParam, vec![second, lam], hw, &n("diff.lambda_ctx"));
            let diff = bk.f(Op::Sub, vec![first, scaled], hw, &n("diff.sub"));
            let nspec = NormSpec { kind: NormKind::Rms, eps: df.norm_eps, gain: Gain::None, bias: false };
            let normed = self.norm(bk, diff, nspec, &n("diff.norm"), hw, h / 2, vec![vw], true, &n("diff.norm"))?;
            let sc = self.param(&n("diff.scale"), vec![1], true, Init::Uniform(0.2, 0.9))?;
            o = bk.f(Op::ScaleParam, vec![normed, sc], hw, &n("diff.out"));
        }
        if let Some(sg) = sep_gate {
            o = bk.f(Op::Mul, vec![o, sg], h * vd, &n("gated"));
        }
        if let Some(g) = gate {
            let s = bk.f(Op::Act(Act::Sigmoid), vec![g], qn, &n("gate_act"));
            o = bk.f(Op::Mul, vec![o, s], qn, &n("gated"));
        }
        // `SUBLAYER_NORMS_V1`: BitNet's `attn_sub_norm` over the heads' concatenated output, before `o_proj`.
        if let Some(on) = a.o_norm {
            o = self.full_norm(bk, o, on, &n("sub_norm"), h * vd, true)?;
        }
        self.linear(bk, o, &n("o"), d, h * vd, a.o_bias, true, &n("out"))
    }

    /// **`ATTN_CROSS_V1`** (Mllama's `cross_attn`): `q = RMS_head(q_proj x)`; the attention over the declared states; `o_proj`. The
    /// rows' keys and values are not computed here (the float reference computes them from the states; the integer program reads
    /// stage 0's stack).
    fn cross_attention(&mut self, bk: &mut Bk, c: &CrossAttnSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let rows = self.s.cross_states.ok_or_else(|| LowerError::eval("internal: a cross-attention layer without states"))?.rows;
        let (h, kv, hd) = (c.heads, c.kv_heads, c.head_dim);
        if h == 0 || kv == 0 || h % kv != 0 || hd == 0 || rows == 0 {
            return Err(LowerError::not_lowerable(format!("ATTN_CROSS_V1: {h} heads over {kv} kv heads of {hd}, {rows} rows")));
        }
        let slots: Vec<usize> =
            self.s.layers.iter().enumerate().filter(|(_, l)| matches!(l.mixer, Mixer::CrossAttention(_))).map(|(i, _)| i).collect();
        let q = self.linear(bk, x, "xattn.q", h * hd, d, false, true, "xattn.q")?;
        let qk = QkNorm { norm: c.q_norm, scope: QkNormScope::PerHeadShared };
        let q = self.qk_norm(bk, q, &qk, "xattn.q_norm", h, hd)?;
        let wk = self.param("xattn.k.w", vec![kv * hd, d], true, Init::Normal(W_STD))?;
        let wv = self.param("xattn.v.w", vec![kv * hd, d], true, Init::Normal(W_STD))?;
        let mut ins = vec![q, wk, wv];
        if c.k_norm.gain != Gain::None {
            ins.push(self.param("xattn.k_norm.gain", vec![hd], true, Init::Uniform(0.6, 1.4))?);
        }
        let ctx = bk.f(
            Op::CrossAttention { heads: h, kv_heads: kv, head_dim: hd, scale: 1.0 / (hd as f64).sqrt(), rows, k_norm: c.k_norm, slots },
            ins,
            h * hd,
            "xattn.ctx",
        );
        self.linear(bk, ctx, "xattn.o", d, h * hd, false, true, "xattn.out")
    }

    /// **`MIXER_MOA_V1`** (JetMoE): the router picks `k` experts; slot `j`'s query is `W_in[e_j]·x`; keys and values are one shared
    /// projection; the output is `Σ_j g_j W_out[e_j]·ctx_j + bias`. Heads are laid out `(kv head, slot)` so that the attention's
    /// `head / k` reads kv head `h` (HF tiles K and V `k` times: head `(j, h)` reads kv head `h`).
    fn moa_attention(&mut self, bk: &mut Bk, a: &AttnSpec, moa: &MoaSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (kvh, hd, k, e, h) = (a.kv_heads, a.head_dim, moa.top_k, moa.experts, a.heads);
        let inner = kvh * hd;
        let plain = a.qk_norm.is_none()
            && a.v_norm.is_none()
            && a.clip_qkv.is_none()
            && a.gate.is_none()
            && !a.output_gate
            && !a.sinks
            && a.sparse.is_none()
            && a.kv_share.is_none()
            && a.differential.is_none()
            && a.q_temperature.is_none()
            && a.chunk.is_none()
            && !a.v_from_k
            && a.o_norm.is_none()
            && a.v_scale == 1.0
            && !(a.q_bias || a.k_bias || a.v_bias || a.o_bias);
        if h != kvh * k || a.v_head_dim != hd || k == 0 || k > e || !plain {
            return Err(LowerError::not_lowerable(format!(
                "MIXER_MOA_V1 needs heads = kv heads × top-k ({h} vs {kvh}×{k}), values as wide as keys, top-k within the experts, and no norms, gates, biases, clipping, sinks, sparse or shared keys"
            )));
        }
        let r = &moa.router;
        if r.scoring != Scoring::TopKThenSoftmax || r.selection_bias || r.groups.is_some() || r.per_expert_scale || r.normalize || r.scale != 1.0 {
            return Err(LowerError::not_lowerable("MIXER_MOA_V1 routes by the top-k of the logits then a softmax (no bias, groups, scale or renormalisation)"));
        }
        let logits = self.linear(bk, x, "moa.router", e, d, r.linear_bias, true, "moa.router")?;
        let route = bk.push(
            Op::Route { router: r.clone(), experts: e, top_k: k, zero: 0 },
            vec![logits],
            vec![vec![k], vec![k]],
            vec![HlType::Idx, HlType::F32],
            Some("moa.route"),
            vec![],
        );
        let (ids, gates) = (Ref::Node(route, 0), Ref::Node(route, 1));
        let w_in = self.param("moa.experts.input", vec![e, inner, d], true, Init::Normal(W_STD))?;
        let q = bk.f(Op::ExpertLinear { top_k: k, per_slot: false, wide: false }, vec![x, ids, w_in], k * inner, "moa.q");
        let mut q = bk.st(Op::Transpose01 { n0: k, n1: kvh, n2: hd }, vec![q], k * inner);
        let kv = self.linear(bk, x, "attn.kv", 2 * inner, d, false, true, "attn.kv")?;
        let mut kk = bk.st(Op::Slice { start: 0, len: inner }, vec![kv], inner);
        let vv = bk.st(Op::Slice { start: inner, len: inner }, vec![kv], inner);
        let mut alibi = None;
        match &a.position {
            Position::Rope(rp) => {
                let t = self.rope_table(&rp.freqs);
                q = bk.f(
                    Op::Rope { heads: h, head_dim: hd, rotary_dim: rp.rotary_dim, offset: rp.offset, style: rp.style, table: t },
                    vec![q, Ref::Pos],
                    h * hd,
                    "attn.q_rope",
                );
                kk = bk.f(
                    Op::Rope { heads: kvh, head_dim: hd, rotary_dim: rp.rotary_dim, offset: rp.offset, style: rp.style, table: t },
                    vec![kk, Ref::Pos],
                    inner,
                    "attn.k_rope",
                );
            }
            Position::Alibi(al) => alibi = Some(al.clone()),
            Position::None => {}
        }
        let suffix = a.window.map(|w| format!(".w{w}")).unwrap_or_default();
        let ks = self.state(&format!("attn.k_hist{suffix}"), StateKind::Hist { window: a.window }, vec![inner], 0.0)?;
        let vs = self.state(&format!("attn.v_hist{suffix}"), StateKind::Hist { window: a.window }, vec![inner], 0.0)?;
        bk.append(kk, ks);
        bk.append(vv, vs);
        let op = Op::Attention {
            heads: h,
            kv_heads: kvh,
            head_dim: hd,
            v_head_dim: hd,
            scale: a.scale,
            softcap: a.softcap,
            window: a.window,
            alibi,
            sinks: false,
            chunk: None,
            blocks: None,
        };
        let o = bk.f(op, vec![q, ks, vs], h * hd, "attn.ctx");
        let o = bk.st(Op::Transpose01 { n0: kvh, n1: k, n2: hd }, vec![o], h * hd);
        let w_out = self.param("moa.experts.output", vec![e, d, inner], true, Init::Normal(W_STD))?;
        let y = bk.f(Op::ExpertLinear { top_k: k, per_slot: true, wide: true }, vec![o, ids, w_out], k * d, "moa.out_rows");
        let mut ins = vec![y, gates];
        if moa.out_bias {
            ins.push(self.param("moa.bias", vec![d], true, Init::Uniform(-0.1, 0.1))?);
        }
        Ok(bk.f(Op::WeightedSum { top_k: k, out_bias: moa.out_bias }, ins, d, "attn.out"))
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
        // `MIXER_MLA_NOPE_V1`: with no rotation the slices stay what the projections made them.
        let q = match &a.rope {
            Some(rp) => {
                let t = self.rope_table(&rp.freqs);
                bk.f(
                    Op::Rope { heads: h, head_dim: qd, rotary_dim: rope, offset: nope, style: rp.style, table: t },
                    vec![q, Ref::Pos],
                    h * qd,
                    "mla.q_rope",
                )
            }
            None => q,
        };
        let c = self.linear(bk, x, "mla.kv_a.latent", r, d, a.a_bias, true, "mla.kv_a")?;
        let c = self.norm(bk, c, a.kv_a_norm, "mla.kv_a_norm", r, 1, vec![r], true, "mla.latent")?;
        let kr_site = if a.rope.is_some() { "mla.k_rope_in" } else { "mla.k_rope" };
        let kr = self.linear(bk, x, "mla.kv_a.rope", rope, d, a.a_bias, true, kr_site)?;
        let kr = match &a.rope {
            Some(rp) => {
                let t = self.rope_table(&rp.freqs);
                bk.f(Op::Rope { heads: 1, head_dim: rope, rotary_dim: rope, offset: 0, style: rp.style, table: t }, vec![kr, Ref::Pos], rope, "mla.k_rope")
            }
            None => kr,
        };
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
            Op::GatedDelta { k_heads: nk, v_heads: nv, dk, dv, head_map: g.head_map, q_scale: 1.0 / (dk as f64).sqrt(), channel_decay: false },
            vec![q, k, v, gl, beta, st],
            nv * dv,
            Some("gdn.core"),
            vec![sid(st)],
        );
        let w = self.param("gdn.norm.gain", vec![dv], true, Init::Uniform(0.6, 1.4))?;
        let o = bk.f(Op::GatedRmsNorm { eps: g.norm_eps, groups: nv, gate_first: false, act: g.gate_act }, vec![o, z, w], nv * dv, "gdn.normed");
        self.linear(bk, o, "gdn.out", d, nv * dv, false, true, "gdn.out")
    }

    /// **`MIXER_KDA_V1`** (Kimi delta attention): the gated delta rule of [`Self::gdn`] with the forget gate per KEY CHANNEL. q, k, v are
    /// three projections with a depthwise causal convolution each (the library's one convolution over the concatenation is the same
    /// function: depthwise channels are independent), the forget gate and the output gate are low-rank (`f_a`, `f_b`; `g_a`, `g_b`), the
    /// output norm gates by `gate_act` (sigmoid) and `kda.A` is the per-channel decay rate `-exp(A_log[head])`.
    fn kda(&mut self, bk: &mut Bk, g: &KdaSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (h, dd, r) = (g.heads, g.head_dim, g.gate_rank);
        let n = h * dd;
        let mut qkv = [Ref::Token; 3];
        for (i, role) in ["q", "k", "v"].into_iter().enumerate() {
            let p = self.linear(bk, x, &format!("kda.{role}"), n, d, false, true, &format!("kda.{role}"))?;
            qkv[i] = self.conv_act(bk, p, n, g.conv_kernel, false, &format!("kda.{role}_conv"), Some(g.conv_act))?;
        }
        let [q, k, v] = qkv;
        let q = bk.f(Op::L2Norm { groups: h, eps: g.l2_eps }, vec![q], n, "kda.q_l2");
        let k = bk.f(Op::L2Norm { groups: h, eps: g.l2_eps }, vec![k], n, "kda.k_l2");
        let b = self.linear(bk, x, "kda.b", h, d, false, true, "kda.b")?;
        let beta = bk.f(Op::Act(Act::Sigmoid), vec![b], h, "kda.beta");
        let fa = self.linear(bk, x, "kda.f_a", r, d, false, true, "kda.f_a")?;
        let a = self.linear(bk, fa, "kda.f_b", n, r, false, true, "kda.f_b")?;
        let dtb = self.param("kda.dt_bias", vec![n], true, Init::Uniform(-1.0, 1.0))?;
        let t = bk.f(Op::Add, vec![a, dtb], n, "kda.dt");
        let sp = bk.f(Op::Act(Act::Softplus), vec![t], n, "kda.dt_softplus");
        // `kda.A` is the (negative) decay rate, one value per channel (the head's `-exp(A_log)` repeated over its channels).
        let aa = self.param("kda.A", vec![n], true, Init::Uniform(-4.0, -0.3))?;
        let gl = bk.f(Op::Mul, vec![sp, aa], n, "kda.log_decay");
        let st = self.state("kda.S", StateKind::Fixed, vec![h, dd, dd], 0.0)?;
        let o = bk.fw(
            Op::GatedDelta { k_heads: h, v_heads: h, dk: dd, dv: dd, head_map: HeadMap::Group, q_scale: 1.0 / (dd as f64).sqrt(), channel_decay: true },
            vec![q, k, v, gl, beta, st],
            n,
            Some("kda.core"),
            vec![sid(st)],
        );
        let ga = self.linear(bk, x, "kda.g_a", r, d, false, true, "kda.g_a")?;
        let gate = self.linear(bk, ga, "kda.g_b", n, r, false, true, "kda.g_b")?;
        let w = self.param("kda.norm.gain", vec![dd], true, Init::Uniform(0.6, 1.4))?;
        let o = bk.f(Op::GatedRmsNorm { eps: g.norm_eps, groups: h, gate_first: false, act: g.gate_act }, vec![o, gate, w], n, "kda.normed");
        self.linear(bk, o, "kda.out", d, n, false, true, "kda.out")
    }

    /// Depthwise causal conv under role `name` (`name.w [C, K]`, `name.b`), SiLU activated.
    fn conv(&mut self, bk: &mut Bk, x: Ref, ch: usize, kernel: usize, bias: bool, name: &str) -> Result<Ref> {
        self.conv_act(bk, x, ch, kernel, bias, name, Some(Act::Silu))
    }

    /// [`Self::conv`] with the activation the convolution applies (`None`: the plain convolution of LFM2's short conv).
    #[allow(clippy::too_many_arguments)]
    fn conv_act(&mut self, bk: &mut Bk, x: Ref, ch: usize, kernel: usize, bias: bool, name: &str, act: Option<Act>) -> Result<Ref> {
        let w = self.param(&format!("{name}.w"), vec![ch, kernel], true, Init::Normal(0.3))?;
        let st = self.state(&format!("{name}.window"), StateKind::Fixed, vec![kernel.saturating_sub(1), ch], 0.0)?;
        let mut ins = vec![x, st, w];
        if bias {
            ins.push(self.param(&format!("{name}.b"), vec![ch], true, Init::Uniform(-0.1, 0.1))?);
        }
        Ok(bk.fw(Op::CausalConv1d { channels: ch, kernel, bias, act, dilation: 1 }, ins, ch, Some(name), vec![sid(st)]))
    }

    /// **`MIXER_SHORT_CONV_V1`** (LFM2): `[B|C|x] = in_proj(x)`, `u = B·x`, `v = conv(u)` (a causal depthwise convolution, no
    /// activation), `out_proj(C·v)`. The three chunks of `in_proj` are three linears (their rows sliced at load).
    fn short_conv(&mut self, bk: &mut Bk, c: &ShortConvSpec, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let b = self.linear(bk, x, "shortconv.in.b", d, d, c.bias, true, "shortconv.b")?;
        let g = self.linear(bk, x, "shortconv.in.c", d, d, c.bias, true, "shortconv.c")?;
        let xx = self.linear(bk, x, "shortconv.in.x", d, d, c.bias, true, "shortconv.x")?;
        let u = bk.f(Op::Mul, vec![b, xx], d, "shortconv.bx");
        let v = self.conv_act(bk, u, d, c.kernel, c.bias, "shortconv.conv", None)?;
        let y = bk.f(Op::Mul, vec![g, v], d, "shortconv.gated");
        self.linear(bk, y, "shortconv.out", d, d, c.bias, true, "shortconv.out")
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
        let (z, xs, bb, cc, dt) = if let Some(cs) = m.chunk_scales {
            // `MAMBA2_MUP_V1`: each of the five chunks of the projection `[z | x | B | C | dt]` is scaled before the convolution. A depthwise
            // convolution is channel-wise, so the x | B | C channels run as three convolutions of their own (the same function), each on its
            // chunk's scaled input; `Op::Scale` is a change of scale key, not a node of the integer program.
            let z = self.linear(bk, x, "mamba2.in.z", inner, d, m.proj_bias, true, "mamba2.z")?;
            let z = bk.f(Op::Scale { c: cs[0] }, vec![z], inner, "mamba2.z_scaled");
            let mut parts = Vec::with_capacity(3);
            for (k, (name, ch)) in [("x", inner), ("b", gn), ("c", gn)].into_iter().enumerate() {
                let p = self.linear(bk, x, &format!("mamba2.in.{name}"), ch, d, m.proj_bias, true, &format!("mamba2.{name}_in"))?;
                let p = bk.f(Op::Scale { c: cs[k + 1] }, vec![p], ch, &format!("mamba2.{name}_scaled"));
                parts.push(self.conv(bk, p, ch, m.conv_kernel, m.conv_bias, &format!("mamba2.conv.{name}"))?);
            }
            let dt = self.linear(bk, x, "mamba2.in.dt", m.heads, d, m.proj_bias, true, "mamba2.dt_in")?;
            let dt = bk.f(Op::Scale { c: cs[4] }, vec![dt], m.heads, "mamba2.dt_scaled");
            (z, parts[0], parts[1], parts[2], dt)
        } else {
            let z = self.linear(bk, x, "mamba2.in.z", inner, d, m.proj_bias, true, "mamba2.z")?;
            let xbc = self.linear(bk, x, "mamba2.in.xbc", conv_dim, d, m.proj_bias, true, "mamba2.xbc")?;
            let dt = self.linear(bk, x, "mamba2.in.dt", m.heads, d, m.proj_bias, true, "mamba2.dt_in")?;
            let xbc = self.conv(bk, xbc, conv_dim, m.conv_kernel, m.conv_bias, "mamba2.conv")?;
            let xs = bk.st(Op::Slice { start: 0, len: inner }, vec![xbc], inner);
            let bb = bk.st(Op::Slice { start: inner, len: gn }, vec![xbc], gn);
            let cc = bk.st(Op::Slice { start: inner + gn, len: gn }, vec![xbc], gn);
            (z, xs, bb, cc, dt)
        };
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
        // `MAMBA2_GATE_NORM_VARIANTS_V1`: the gate and the norm in either order, or the gate alone.
        let y = match m.norm_mode {
            Mamba2Norm::GateFirst | Mamba2Norm::NormFirst => {
                let w = self.param("mamba2.norm.gain", vec![inner], true, Init::Uniform(0.6, 1.4))?;
                let gate_first = m.norm_mode == Mamba2Norm::GateFirst;
                bk.f(Op::GatedRmsNorm { eps: m.norm_eps, groups: m.norm_groups, gate_first, act: Act::Silu }, vec![y, z, w], inner, "mamba2.normed")
            }
            Mamba2Norm::Ungated => {
                let g = bk.f(Op::Act(Act::Silu), vec![z], inner, "mamba2.gate");
                bk.f(Op::Mul, vec![y, g], inner, "mamba2.gated")
            }
        };
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
    /// The activation node over `x` (`n` wide). xIELU reads four scalars of the layer (`{name}.alpha_p`, `.alpha_n`, `.beta`, `.eps`):
    /// the only activation with parameters, so the only one that is not `Op::Act`.
    fn activation(&mut self, bk: &mut Bk, act: Act, x: Ref, n: usize, name: &str) -> Result<Ref> {
        if act != Act::Xielu {
            return Ok(bk.f(Op::Act(act), vec![x], n, name));
        }
        let p = self.param(&format!("{name}.alpha_p"), vec![1], true, Init::Uniform(0.0, 0.5))?;
        let q = self.param(&format!("{name}.alpha_n"), vec![1], true, Init::Uniform(0.0, 0.5))?;
        let beta = self.param(&format!("{name}.beta"), vec![1], true, Init::Uniform(0.3, 0.7))?;
        let eps = self.param(&format!("{name}.eps"), vec![1], true, Init::Uniform(-1e-3, -1e-6))?;
        Ok(bk.f(Op::Xielu, vec![x, p, q, beta, eps], n, name))
    }

    fn mlp(&mut self, bk: &mut Bk, m: &MlpSpec, x: Ref, pfx: &str) -> Result<Ref> {
        let d = self.s.hidden_size;
        let i = m.intermediate;
        let u = self.linear(bk, x, &format!("{pfx}.up"), i, d, m.up_bias, true, &format!("{pfx}.up"))?;
        let mut hdn = if m.gated {
            let mut g = self.linear(bk, x, &format!("{pfx}.gate"), i, d, m.up_bias, true, &format!("{pfx}.gate"))?;
            // `FFN_ACTIVATION_SPARSITY_V1`: only what lies above `mean + z·std` of the gate row survives the activation.
            if m.sparsity.is_some() {
                if !matches!(m.glu, Glu::Standard) {
                    return Err(LowerError::not_lowerable("FFN_ACTIVATION_SPARSITY_V1 under a clamped SwiGLU"));
                }
                g = bk.f(Op::GaussianTopK { layers: vec![] }, vec![g], i, &format!("{pfx}.sparse"));
            }
            match m.glu {
                Glu::Standard => {
                    let a = self.activation(bk, m.act, g, i, &format!("{pfx}.act"))?;
                    bk.f(Op::Mul, vec![a, u], i, &format!("{pfx}.hidden"))
                }
                // `MLP_GLU_LIMITED_V1`: `act(min(gate, L)) · clamp(up, −L, L)`.
                Glu::LimitedGlu { limit } => {
                    let gc = bk.f(Op::Clamp { lo: -1.0e30, hi: limit }, vec![g], i, &format!("{pfx}.gate_limited"));
                    let uc = bk.f(Op::Clamp { lo: -limit, hi: limit }, vec![u], i, &format!("{pfx}.up_limited"));
                    let a = self.activation(bk, m.act, gc, i, &format!("{pfx}.act"))?;
                    bk.f(Op::Mul, vec![a, uc], i, &format!("{pfx}.hidden"))
                }
                Glu::ClampedSwiGlu { alpha, limit } => {
                    if m.act == Act::Xielu {
                        return Err(LowerError::not_lowerable("xIELU in a clamped SwiGLU"));
                    }
                    bk.f(Op::ClampedSwiGlu { alpha, limit }, vec![g, u], i, &format!("{pfx}.hidden"))
                }
            }
        } else {
            self.activation(bk, m.act, u, i, &format!("{pfx}.act"))?
        };
        // `SUBLAYER_NORMS_V1`: BitNet's `ffn_sub_norm` over the hidden activation, before the down projection.
        if let Some(n) = m.inner_norm {
            hdn = self.full_norm(bk, hdn, n, &format!("{pfx}.sub_norm"), i, true)?;
        }
        self.linear(bk, hdn, &format!("{pfx}.down"), d, i, m.down_bias, true, &format!("{pfx}.out"))
    }

    fn moe(&mut self, bk: &mut Bk, m: &MoeSpec, x: Ref) -> Result<Ref> {
        self.moe_split(bk, m, x, x)
    }

    /// The MoE block with its router reading `rx` and its experts `x` (Gemma-4 feeds them apart).
    fn moe_split(&mut self, bk: &mut Bk, m: &MoeSpec, rx: Ref, x: Ref) -> Result<Ref> {
        let d = self.s.hidden_size;
        let (e, i) = (m.experts, m.intermediate);
        plain_act(m.act, "an expert MLP")?;
        // `MOE_EXPERTS_PLAIN_V1`: a plain expert is `down(act(up(x)))`; the gated-only options do not apply to it.
        if !m.gated && (m.expert_bias || m.input_scaled || !matches!(m.glu, Glu::Standard)) {
            return Err(LowerError::not_lowerable("MOE_EXPERTS_PLAIN_V1: plain experts carry no biases, no clamped GLU and no input scaling"));
        }
        // `MOE_LATENT_PROJ_V1`: the routed experts work in a latent space; the router and the shared expert read the layer's input.
        let dl = m.latent.unwrap_or(d);
        if m.latent == Some(0) {
            return Err(LowerError::not_lowerable("MOE_LATENT_PROJ_V1: a latent width of zero"));
        }
        // A spec flag the lowering would not apply is a refusal, never a silent no-op (FR-26): a model whose router
        // carries one of these would otherwise compute a different function with no error anywhere.
        let r = &m.router;
        if r.selection_bias && !matches!(r.scoring, Scoring::Sigmoid | Scoring::Softmax | Scoring::SqrtSoftplus) {
            return Err(LowerError::not_lowerable(format!(
                "router selection bias with {:?} scoring: the bias joins the selection scores of the sigmoid, softmax and √softplus routers only, so this lowering would drop it",
                r.scoring
            )));
        }
        if r.jitter_eps != 0.0 && r.scoring != Scoring::SparseMixer {
            return Err(LowerError::not_lowerable(format!(
                "router jitter noise {} with {:?} scoring: only sparsemixer reads it, so this lowering would drop it",
                r.jitter_eps, r.scoring
            )));
        }
        // `MLP_MOE_ZERO_EXPERT_V1`: the router (and its bias) run over the real experts plus the identity ones; the tensors of the
        // experts keep `e` rows.
        let z = m.zero_experts;
        if z > 0 && (m.router.per_expert_scale || m.input_scaled || m.latent.is_some() || !matches!(m.glu, Glu::Standard) || m.router.groups.is_some()) {
            return Err(LowerError::not_lowerable(
                "MLP_MOE_ZERO_EXPERT_V1 with a per-expert scale, input-scaled or latent experts, a clamped GLU or group-limited routing is not modelled",
            ));
        }
        let ne = e + z;
        let logits = self.linear(bk, rx, "moe.router", ne, d, m.router.linear_bias, true, "moe.router")?;
        let mut rins = vec![logits];
        if m.router.selection_bias {
            rins.push(self.param("moe.sel_bias", vec![ne], true, Init::Uniform(-0.05, 0.05))?);
        }
        // `MLP_MOE_ROUTER_HASH_V1`: the frozen token table whose row is the token's experts (read by the token id).
        if m.router.scoring == Scoring::SqrtSoftplusHash {
            rins.push(self.param("moe.tid2eid", vec![self.s.vocab_size, m.top_k], true, Init::Zeros)?);
        }
        if m.router.per_expert_scale {
            rins.push(self.param("moe.expert_scale", vec![e], true, Init::Uniform(0.5, 1.5))?);
        }
        let (outs, types) = if z > 0 {
            (vec![vec![m.top_k], vec![m.top_k], vec![1]], vec![HlType::Idx, HlType::F32, HlType::F32])
        } else {
            (vec![vec![m.top_k], vec![m.top_k]], vec![HlType::Idx, HlType::F32])
        };
        let route = bk.push(
            Op::Route { router: m.router.clone(), experts: ne, top_k: m.top_k, zero: z },
            rins,
            outs,
            types,
            Some("moe.route"),
            vec![],
        );
        let xe = match m.latent {
            Some(l) => self.linear(bk, x, "moe.latent_in", l, d, false, true, "moe.latent_in")?,
            None => x,
        };
        let mut ins = vec![xe, Ref::Node(route, 0), Ref::Node(route, 1)];
        if m.gated {
            ins.push(self.param("moe.experts.gate", vec![e, i, dl], true, Init::Normal(W_STD))?);
        }
        ins.push(self.param("moe.experts.up", vec![e, i, dl], true, Init::Normal(W_STD))?);
        ins.push(self.param("moe.experts.down", vec![e, dl, i], true, Init::Normal(W_STD))?);
        if m.expert_bias {
            ins.push(self.param("moe.experts.gate_b", vec![e, i], true, Init::Uniform(-0.1, 0.1))?);
            ins.push(self.param("moe.experts.up_b", vec![e, i], true, Init::Uniform(-0.1, 0.1))?);
            ins.push(self.param("moe.experts.down_b", vec![e, dl], true, Init::Uniform(-0.1, 0.1))?);
        }
        if z > 0 {
            ins.push(Ref::Node(route, 2));
        }
        if m.out_bias {
            ins.push(self.param("moe.bias", vec![d], true, Init::Uniform(-0.1, 0.1))?);
        }
        let mut y = bk.f(
            Op::MoeExperts { top_k: m.top_k, act: m.act, glu: m.glu, bias: m.expert_bias, input_scaled: m.input_scaled, gated: m.gated, identity: z > 0, out_bias: m.out_bias },
            ins,
            dl,
            "moe.routed",
        );
        if let Some(l) = m.latent {
            y = self.linear(bk, y, "moe.latent_out", d, l, false, true, "moe.latent_out")?;
        }
        if let Some(sh) = &m.shared {
            let spec = MlpSpec {
                intermediate: sh.intermediate,
                act: m.act,
                gated: m.gated,
                glu: if matches!(m.glu, Glu::LimitedGlu { .. }) { m.glu } else { Glu::Standard },
                up_bias: false,
                down_bias: false,
                inner_norm: None,
                name: None,
                sparsity: None,
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

fn mixer_name(m: &Mixer) -> String {
    match m {
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
        Mixer::Kda(_) => "kda".into(),
        Mixer::Mamba(_) => "mamba".into(),
        Mixer::Mamba2(_) => "mamba2".into(),
        Mixer::ShortConv(_) => "shortconv".into(),
        Mixer::RwkvTime(r) => format!("rwkv{}", r.version),
        Mixer::None => String::new(),
        Mixer::CrossAttention(_) => "xattn".into(),
        Mixer::Parallel(bs) => bs.iter().map(|b| mixer_name(&b.mixer)).collect::<Vec<_>>().join("|"),
        Mixer::SharedKv(a) => match &a.compressed {
            None => "skv".into(),
            Some(c) if c.overlap => "skv.csa".into(),
            Some(_) => "skv.hca".into(),
        },
    }
}

fn block_name(ls: &LayerSpec) -> String {
    let mix = mixer_name(&ls.mixer);
    let ffn = match &ls.ffn {
        Ffn::None => String::new(),
        Ffn::Mlp(_) => "+mlp".into(),
        Ffn::Moe(_) => "+moe".into(),
        Ffn::RwkvChannel(_) => "+cmix".into(),
        Ffn::MlpMoe(_) => "+mlp|moe".into(),
        Ffn::MlpShortcut(_) => "+mlp|shortcut".into(),
    };
    // A layer with no mixer is its FFN alone: `mlp`, `moe`.
    if mix.is_empty() { ffn.trim_start_matches('+').to_string() } else { format!("{mix}{ffn}") }
}

/// An activation a node applies to its input alone. xIELU reads parameters of the layer (`Op::Xielu`, built only by a dense MLP's
/// activation): anywhere else it would be silently another function, so it is refused by name.
fn plain_act(a: Act, what: &str) -> Result<Act> {
    if a == Act::Xielu {
        return Err(LowerError::not_lowerable(format!("xIELU in {what}: it is a learned activation, lowered in a dense MLP only")));
    }
    Ok(a)
}
