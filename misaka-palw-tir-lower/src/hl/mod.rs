//! **The high-level ML graph (HL)**, organised the way `TirProgramV1` will be (RFC-0002 §4.1).
//!
//! * A program computes **one position**: inputs `token` and `pos`, logits out.
//! * `blocks[pre]` embeds, `blocks[post]` produces logits, and every other block is a *layer
//!   kind*; `schedule[l]` names the block layer `l` runs. Equal layers share a block.
//! * Blocks exchange **carries** (the residual stream `h`).
//! * **Params** are math-level roles with shapes (`attn.q.w [H·d, D]`, `gdn.A [vh]` = the negative
//!   decay rate, `moe.experts.gate [E, I, D]` …); `per_layer` params exist once per layer that runs
//!   a block referencing them. How a frontend fills them (slicing a fused HF tensor, `−exp(A_log)`)
//!   is not part of the graph — see `crate::hf_weights`.
//! * **States** are `Fixed` (recurrences: conv windows, token shift, GDN/Mamba/RWKV state) or
//!   `Hist` (append-only KV history, read through a window).
//! * Every op boundary where the integer program will requantise carries a named **site**;
//!   composite ops also report internal sub-sites (`attn.ctx.scores`, `gdn.core.state`, …).
//!
//! Ops have float semantics (the `transformers` computation). Gate 2 expands each op through the
//! TIR composite library; nothing here is a consensus object.

pub mod build;
pub mod cost;

use crate::rope::{AlibiSpec, RopeFreqs, RopeStyle};
use crate::spec::{Act, Glu, HeadMap, NormSpec, RouterSpec};
use serde::Serialize;

pub use build::build_program;

pub type NodeId = u32;

/// An operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Ref {
    /// Output `.1` of node `.0` (strictly earlier in the same block).
    Node(NodeId, u8),
    /// Carry-in slot of the block.
    Carry(u8),
    Param(u32),
    State(u32),
    Token,
    Pos,
}

/// An unmerged LoRA path's shape and scale: rank `r`, and `alpha/r` (or `alpha/√r` for rsLoRA)
/// as the exact rational `num/den`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LoraOp {
    pub rank: usize,
    pub num: i64,
    pub den: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HlType {
    F32,
    /// Selection indices (token ids, expert ids).
    Idx,
    /// A node with no value (a state write).
    Unit,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Op {
    // ── structure ──
    /// `table[token]`. In: `[Token, table]`.
    Embedding,
    /// `table[pos + offset]`. In: `[Pos, table]`.
    PosEmbedding {
        offset: usize,
    },
    Slice {
        start: usize,
        len: usize,
    },
    Concat,
    Zeros,
    // ── arithmetic ──
    /// `W·x (+ b)`, `W` is `[out, in]`. In: `[x, W, (b)]`.
    Linear {
        bias: bool,
        /// A LoRA adapter on this projection (RFC-0004's candidate = parent + adapter): inputs
        /// `[x, W, (b), A, B]`, value `W·x (+ b) + (num/den)·B·(A·x)`, unmerged.
        lora: Option<LoraOp>,
    },
    /// Elementwise; an operand of one element broadcasts.
    Add,
    Sub,
    Mul,
    Scale {
        c: f64,
    },
    Act(Act),
    Clamp {
        lo: f64,
        hi: f64,
    },
    /// `tanh(x / cap) · cap`.
    Softcap {
        cap: f64,
    },
    /// `a + (b − a)·t`. In: `[a, b, t]`.
    Lerp,
    /// RWKV-5/6 per-channel decay `exp(−exp(x))`.
    DecayExpNegExp,
    /// gpt-oss: `(clamp(up,−l,l)+1) · g·σ(α·g)`, `g = min(gate, l)`. In: `[gate, up]`.
    ClampedSwiGlu {
        alpha: f64,
        limit: f64,
    },
    // ── normalisation ──
    /// `groups` equal groups normalised separately; gain `[n]`, `[n/groups]` (shared) or
    /// `[groups, n/groups]`. In: `[x, (gain), (bias)]`.
    Norm {
        spec: NormSpec,
        groups: usize,
    },
    /// RMSNorm with a SiLU gate `z`: `gate_first` ⇒ `norm(x·silu(z))·w` (Mamba2), else
    /// `norm(x)·w·silu(z)` (Qwen3-Next). In: `[x, z, gain]`.
    GatedRmsNorm {
        eps: f64,
        groups: usize,
        gate_first: bool,
    },
    /// `x · rsqrt(Σx² + eps)` per group (FLA's l2norm).
    L2Norm {
        groups: usize,
        eps: f64,
    },
    // ── position ──
    /// Rotates dims `[offset, offset+rotary_dim)` of each head by `pos`. In: `[x, Pos]`.
    Rope {
        heads: usize,
        head_dim: usize,
        rotary_dim: usize,
        offset: usize,
        style: RopeStyle,
        table: u32,
    },
    // ── history (Hist states) ──
    /// Append a row to a `Hist` state. In: `[x, State]`.
    HistAppend,
    /// Softmax attention over the last `min(pos+1, window)` rows of the K/V histories; query head
    /// `h` reads kv head `h / (heads/kv_heads)`. In: `[q, State(k), State(v), (sinks)]`.
    Attention {
        heads: usize,
        kv_heads: usize,
        head_dim: usize,
        v_head_dim: usize,
        scale: f64,
        softcap: Option<f64>,
        window: Option<usize>,
        alibi: Option<AlibiSpec>,
        sinks: bool,
    },
    /// Multi-head latent attention over a compressed history (DeepSeek). Keys/values are
    /// `kv_b · latent` per head plus the shared rotary key. In: `[q, State(latent), State(k_rope), kv_b]`.
    MlaAttention {
        heads: usize,
        nope: usize,
        rope: usize,
        v_dim: usize,
        kv_lora: usize,
        scale: f64,
    },
    // ── recurrences (Fixed states) ──
    /// Depthwise causal convolution over the last `kernel` inputs; the state keeps `kernel−1`
    /// rows. In: `[x, State(window), w [C,K], (b)]`.
    CausalConv1d {
        channels: usize,
        kernel: usize,
        bias: bool,
        act: Option<Act>,
    },
    /// Gated delta rule (Qwen3-Next): per value head `S ← S·exp(g); S += k (β(v − Sᵀk))ᵀ; o = Sᵀ(q·q_scale)`.
    /// In: `[q, k, v, g, beta, State(S [vh, dk, dv])]`.
    GatedDelta {
        k_heads: usize,
        v_heads: usize,
        dk: usize,
        dv: usize,
        head_map: HeadMap,
        q_scale: f64,
    },
    /// Mamba-1 selective scan step. In: `[x, dt, B, C, A [I,N], D [I], State(h [I,N])]`.
    SelectiveScan {
        inner: usize,
        state: usize,
    },
    /// Mamba-2 SSD step (scalar decay per head, grouped B/C). In: `[x, dt, B, C, A [H], D [H], State(h [H,P,N])]`.
    Ssd {
        heads: usize,
        head_dim: usize,
        groups: usize,
        state: usize,
    },
    /// RWKV token shift: outputs the previous position's `x` (zeros at 0) and stores `x`.
    /// In: `[x, State(prev)]`.
    TokenShift,
    /// RWKV-4 WKV with the (num, den, max) stabilised state. In: `[k, v, w, u, State(num), State(den), State(max)]`.
    Wkv4,
    /// RWKV-5/6 WKV: `o = r·(u⊙kᵀv + S)`, `S ← kᵀv + w⊙S` per head (w per channel).
    /// In: `[r, k, v, w, u, State(S [H,S,S])]`.
    Wkv6 {
        heads: usize,
        head_size: usize,
    },
    /// RWKV-7 WKV: `S ← S·diag(w) + (S·a)bᵀ + v kᵀ`, `o = S·r`. In: `[r, w, k, v, a, b, State(S [H,S,S])]`.
    Wkv7 {
        heads: usize,
        head_size: usize,
    },
    // ── routing ──
    /// Expert selection. Out 0: `top_k` expert ids in index order (ties → lowest index);
    /// out 1: their weights. In: `[logits, (selection bias)]`.
    Route {
        router: RouterSpec,
        experts: usize,
        top_k: usize,
    },
    /// `Σ_j w_j · down_e(glu(gate_e x, up_e x))` over the selected experts.
    /// In: `[x, ids, weights, gate [E,I,D], up [E,I,D], down [E,D,I], (gate_b, up_b, down_b)]`.
    MoeExperts {
        top_k: usize,
        act: Act,
        glu: Glu,
        bias: bool,
    },
}

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Op::Embedding => "Embedding",
            Op::PosEmbedding { .. } => "PosEmbedding",
            Op::Slice { .. } => "Slice",
            Op::Concat => "Concat",
            Op::Zeros => "Zeros",
            Op::Linear { .. } => "Linear",
            Op::Add => "Add",
            Op::Sub => "Sub",
            Op::Mul => "Mul",
            Op::Scale { .. } => "Scale",
            Op::Act(_) => "Act",
            Op::Clamp { .. } => "Clamp",
            Op::Softcap { .. } => "Softcap",
            Op::Lerp => "Lerp",
            Op::DecayExpNegExp => "DecayExpNegExp",
            Op::ClampedSwiGlu { .. } => "ClampedSwiGlu",
            Op::Norm { .. } => "Norm",
            Op::GatedRmsNorm { .. } => "GatedRmsNorm",
            Op::L2Norm { .. } => "L2Norm",
            Op::Rope { .. } => "Rope",
            Op::HistAppend => "HistAppend",
            Op::Attention { .. } => "Attention",
            Op::MlaAttention { .. } => "MlaAttention",
            Op::CausalConv1d { .. } => "CausalConv1d",
            Op::GatedDelta { .. } => "GatedDelta",
            Op::SelectiveScan { .. } => "SelectiveScan",
            Op::Ssd { .. } => "Ssd",
            Op::TokenShift => "TokenShift",
            Op::Wkv4 => "Wkv4",
            Op::Wkv6 { .. } => "Wkv6",
            Op::Wkv7 { .. } => "Wkv7",
            Op::Route { .. } => "Route",
            Op::MoeExperts { .. } => "MoeExperts",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Node {
    pub op: Op,
    pub inputs: Vec<Ref>,
    /// Output shapes (one per output).
    pub outs: Vec<Vec<usize>>,
    pub out_types: Vec<HlType>,
    /// The requantisation site at this node's output (`None` for structural ops).
    pub site: Option<String>,
    /// States this node writes.
    pub writes: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BlockRole {
    Pre,
    Layer,
    Post,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Block {
    pub name: String,
    pub role: BlockRole,
    pub nodes: Vec<Node>,
    /// Pre/Layer: the carries out (one per `HlProgram::carries`). Post: `[logits]`.
    pub outputs: Vec<Ref>,
}

/// Synthetic-weight hints: the distribution of the HL param value itself.
/// Tests only; never used with a real checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Init {
    Normal(f32),
    Ones,
    Zeros,
    Uniform(f32, f32),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParamDecl {
    /// A math-level role (`attn.q.w`, `gdn.A`, `moe.experts.gate` …), never a checkpoint name.
    pub name: String,
    pub shape: Vec<usize>,
    pub per_layer: bool,
    pub init: Init,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum StateKind {
    /// A recurrence carried from position to position.
    Fixed,
    /// An append-only history read through a window (`None` = all of it).
    Hist { window: Option<usize> },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StateDecl {
    pub name: String,
    pub kind: StateKind,
    /// Fixed: the whole state. Hist: one row.
    pub shape: Vec<usize>,
    pub per_layer: bool,
    /// Initial value of every element of a Fixed state (0, or −1e38 for RWKV-4's running max).
    pub init: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CarryDecl {
    pub name: String,
    pub shape: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HlProgram {
    pub architecture: String,
    pub vocab: usize,
    pub hidden: usize,
    pub carries: Vec<CarryDecl>,
    pub params: Vec<ParamDecl>,
    pub states: Vec<StateDecl>,
    pub rope_tables: Vec<RopeFreqs>,
    pub blocks: Vec<Block>,
    pub pre: usize,
    pub post: usize,
    /// Block index per layer.
    pub schedule: Vec<u16>,
}

impl HlProgram {
    pub fn num_layers(&self) -> usize {
        self.schedule.len()
    }
    /// Params referenced by a block.
    pub fn block_params(&self, b: usize) -> Vec<u32> {
        let mut v: Vec<u32> = self.blocks[b]
            .nodes
            .iter()
            .flat_map(|n| n.inputs.iter())
            .filter_map(|r| if let Ref::Param(p) = r { Some(*p) } else { None })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
    pub fn block_states(&self, b: usize) -> Vec<u32> {
        let mut v: Vec<u32> = self.blocks[b]
            .nodes
            .iter()
            .flat_map(|n| {
                n.inputs.iter().filter_map(|r| if let Ref::State(s) = r { Some(*s) } else { None }).chain(n.writes.iter().copied())
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
    /// All site names of a block, in node order.
    pub fn sites(&self, b: usize) -> Vec<&str> {
        self.blocks[b].nodes.iter().filter_map(|n| n.site.as_deref()).collect()
    }
    /// Checks the structural invariants the TIR program will need: refs point strictly backward,
    /// operand shapes are consistent with declarations, sites are unique within a block, and the
    /// schedule names layer blocks.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.blocks.get(self.pre).map(|b| b.role) != Some(BlockRole::Pre) {
            return Err("pre block".into());
        }
        if self.blocks.get(self.post).map(|b| b.role) != Some(BlockRole::Post) {
            return Err("post block".into());
        }
        for (l, k) in self.schedule.iter().enumerate() {
            if self.blocks.get(*k as usize).map(|b| b.role) != Some(BlockRole::Layer) {
                return Err(format!("layer {l} schedules a non-layer block {k}"));
            }
        }
        for (bi, b) in self.blocks.iter().enumerate() {
            let mut seen = std::collections::BTreeSet::new();
            for (ni, n) in b.nodes.iter().enumerate() {
                for r in &n.inputs {
                    match r {
                        Ref::Node(i, o) => {
                            if *i as usize >= ni {
                                return Err(format!("block {} node {ni} refers forward to {i}", b.name));
                            }
                            if *o as usize >= b.nodes[*i as usize].outs.len() {
                                return Err(format!("block {} node {ni} refers to missing output {o} of {i}", b.name));
                            }
                        }
                        Ref::Carry(c) => {
                            if bi == self.pre || *c as usize >= self.carries.len() {
                                return Err(format!("block {} node {ni} reads carry {c}", b.name));
                            }
                        }
                        Ref::Param(p) => {
                            if *p as usize >= self.params.len() {
                                return Err(format!("block {} param {p}", b.name));
                            }
                        }
                        Ref::State(s) => {
                            if *s as usize >= self.states.len() {
                                return Err(format!("block {} state {s}", b.name));
                            }
                        }
                        Ref::Token | Ref::Pos => {}
                    }
                }
                if let Some(s) = &n.site
                    && !seen.insert(s.clone())
                {
                    return Err(format!("block {} has site `{s}` twice", b.name));
                }
                if n.outs.len() != n.out_types.len() {
                    return Err(format!("block {} node {ni}: outs/types", b.name));
                }
            }
            let want = if b.role == BlockRole::Post { 1 } else { self.carries.len() };
            if b.outputs.len() != want {
                return Err(format!("block {} has {} outputs, want {want}", b.name, b.outputs.len()));
            }
        }
        Ok(())
    }

    /// A one-screen summary: blocks, schedule (run-length), params, states.
    pub fn summary(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "HL program for {}: {} layers, hidden {}, vocab {}",
            self.architecture,
            self.num_layers(),
            self.hidden,
            self.vocab
        );
        for (i, b) in self.blocks.iter().enumerate() {
            let mut ops: Vec<String> = Vec::new();
            for n in &b.nodes {
                let name = match &n.op {
                    Op::Act(a) => format!("Act({a:?})"),
                    o => o.name().to_string(),
                };
                if ops.last().map(|x: &String| x != &name).unwrap_or(true) {
                    ops.push(name);
                }
            }
            let sites = b.nodes.iter().filter(|n| n.site.is_some()).count();
            let _ =
                writeln!(s, "  block {i} `{}` ({:?}): {} nodes, {sites} sites: {}", b.name, b.role, b.nodes.len(), ops.join(" → "));
        }
        let mut runs: Vec<(u16, usize)> = Vec::new();
        for k in &self.schedule {
            match runs.last_mut() {
                Some((kk, c)) if kk == k => *c += 1,
                _ => runs.push((*k, 1)),
            }
        }
        let sched: Vec<String> = runs.iter().map(|(k, c)| if *c == 1 { format!("{k}") } else { format!("{k}×{c}") }).collect();
        let _ = writeln!(s, "  schedule: [{}]", sched.join(", "));
        let fixed: Vec<&StateDecl> = self.states.iter().filter(|d| d.kind == StateKind::Fixed).collect();
        let hist: Vec<&StateDecl> = self.states.iter().filter(|d| d.kind != StateKind::Fixed).collect();
        let _ = writeln!(
            s,
            "  params: {} decls ({} per-layer); states: {} Fixed, {} Hist; rope tables: {}",
            self.params.len(),
            self.params.iter().filter(|p| p.per_layer).count(),
            fixed.len(),
            hist.len(),
            self.rope_tables.len()
        );
        for d in &self.states {
            let _ = writeln!(s, "    state `{}` {:?} {:?}", d.name, d.kind, d.shape);
        }
        s
    }
}
