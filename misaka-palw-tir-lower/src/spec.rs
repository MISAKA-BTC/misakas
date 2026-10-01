//! **`ArchSpec`: one normalised description of a Hugging Face decoder.**
//!
//! Every architecture parser (`crate::arch`) produces this, and everything downstream — the HL
//! graph builder, the weight mapper, the float reference, the cost estimate — reads only this. Two
//! checkpoints with equal `ArchSpec`s (up to naming) compute the same function.
//!
//! The spec is fully expanded per layer (`layers[i]`). The HL builder groups equal layers into
//! block kinds, which is where the RFC's `schedule.kinds` comes from.

use crate::rope::{AlibiSpec, RopeSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn one() -> f64 {
    1.0
}

fn silu() -> Act {
    Act::Silu
}

fn infinity() -> f64 {
    f64::INFINITY
}

fn f64_or_infinity<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::INFINITY))
}

/// Where the reference semantics come from. A checkpoint that needs `trust_remote_code` is
/// modelled only for remote-code modules this lowerer names; everything else is refused.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Reference {
    /// `transformers`' own modeling file for `model_type`.
    #[default]
    Native,
    /// A `trust_remote_code` module shipped with the checkpoint (pinned by revision in practice).
    RemoteCode { module: String },
    /// A third-party library the checkpoint names (e.g. flash-linear-attention for RWKV-7).
    ExternalLibrary { name: String },
}

/// How sure this lowerer is of the semantics. `Unsure` lists what to confirm against HF fixtures.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Confidence {
    /// From the `transformers` implementation as written; confirmed by fixtures when present.
    #[default]
    Known,
    /// Semantics or naming reconstructed with gaps; each string is one thing to check.
    Unsure(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NormKind {
    /// `x * rsqrt(mean(x²) + eps)`.
    Rms,
    /// `(x - mean) * rsqrt(var + eps)` with the biased variance.
    Layer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gain {
    /// Non-parametric (OLMo-1, FalconMamba's B/C/dt norms).
    None,
    /// `w * x̂`.
    W,
    /// `(1 + w) * x̂` (Gemma, Qwen3-Next, Nemotron's LayerNorm1P).
    OnePlusW,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormSpec {
    pub kind: NormKind,
    pub eps: f64,
    pub gain: Gain,
    #[serde(default)]
    pub bias: bool,
}

impl NormSpec {
    pub fn rms(eps: f64) -> Self {
        NormSpec { kind: NormKind::Rms, eps, gain: Gain::W, bias: false }
    }
    pub fn rms_1p(eps: f64) -> Self {
        NormSpec { kind: NormKind::Rms, eps, gain: Gain::OnePlusW, bias: false }
    }
    pub fn layer(eps: f64) -> Self {
        NormSpec { kind: NormKind::Layer, eps, gain: Gain::W, bias: true }
    }
    pub fn layer_nobias(eps: f64) -> Self {
        NormSpec { kind: NormKind::Layer, eps, gain: Gain::W, bias: false }
    }
    pub fn short(&self) -> String {
        let k = match self.kind {
            NormKind::Rms => "rms",
            NormKind::Layer => "ln",
        };
        let g = match self.gain {
            Gain::None => "",
            Gain::W => "·w",
            Gain::OnePlusW => "·(1+w)",
        };
        format!("{k}{g}{}(eps={:e})", if self.bias { "+b" } else { "" }, self.eps)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    Silu,
    /// Exact, `0.5·x·(1+erf(x/√2))`.
    Gelu,
    /// `0.5·x·(1+tanh(√(2/π)(x+0.044715x³)))` (`gelu_new`, `gelu_pytorch_tanh`, `gelu_fast`).
    GeluTanh,
    /// `x·σ(1.702x)`.
    QuickGelu,
    Relu,
    /// `relu(x)²` (Nemotron, RWKV channel mix).
    Relu2,
    Sigmoid,
    Tanh,
    Softplus,
    Identity,
}

impl Act {
    /// HF's `ACT2FN` names. Anything else is refused by the caller.
    pub fn from_hf(name: &str) -> Option<Act> {
        Some(match name {
            "silu" | "swish" => Act::Silu,
            "gelu" => Act::Gelu,
            "gelu_new" | "gelu_pytorch_tanh" | "gelu_fast" | "gelu_accurate" => Act::GeluTanh,
            "quick_gelu" => Act::QuickGelu,
            "relu" => Act::Relu,
            "relu2" => Act::Relu2,
            "sigmoid" => Act::Sigmoid,
            "tanh" => Act::Tanh,
            "linear" | "identity" => Act::Identity,
            _ => return None,
        })
    }
}

/// HF storage: how q, k and v are stored. The HL graph always has three projections; a fused
/// tensor is sliced at load time (`crate::weights::Src`), which is exact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QkvLayout {
    /// `q_proj`, `k_proj`, `v_proj`.
    #[default]
    Separate,
    /// One tensor, rows `[q (H·d) | k (KV·d) | v (KV·d)]` (Phi-3, MPT, GPT-2, GPTBigCode MQA).
    FusedConcat,
    /// One tensor, rows `[head][q,k,v][d]` (GPT-NeoX, BLOOM, Falcon MHA, GPTBigCode MHA).
    FusedPerHead,
    /// One tensor, rows `[kv_head][q_0..q_{g−1}, k, v][d]` (Falcon new decoder, InternLM2).
    FusedPerKvGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QkNormScope {
    /// One gain of `head_dim`, shared by every head (Qwen3, Gemma3, Qwen3-Next).
    PerHeadShared,
    /// Per-head gains `[heads, head_dim]` (Cohere `use_qk_norm`, StableLM `qk_layernorm`).
    PerHeadSeparate,
    /// One norm over the whole projection `[heads·head_dim]` (OLMo-2, OLMoE).
    Whole,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct QkNorm {
    pub norm: NormSpec,
    pub scope: QkNormScope,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Position {
    /// No positional term in attention (Jamba, Cohere2/Exaone4/SmolLM3 global layers, …).
    None,
    Rope(RopeSpec),
    Alibi(AlibiSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttnSpec {
    pub heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub v_head_dim: usize,
    #[serde(default)]
    pub q_bias: bool,
    #[serde(default)]
    pub k_bias: bool,
    #[serde(default)]
    pub v_bias: bool,
    #[serde(default)]
    pub o_bias: bool,
    #[serde(default)]
    pub qk_norm: Option<QkNorm>,
    /// `clamp(q|k|v, −c, c)` after projection (OLMo `clip_qkv`, MPT).
    #[serde(default)]
    pub clip_qkv: Option<f64>,
    pub position: Position,
    /// Softmax scale applied to `q·k` (already includes every multiplier).
    pub scale: f64,
    /// `tanh(s / c) · c` on the scaled scores (Gemma-2).
    #[serde(default)]
    pub softcap: Option<f64>,
    /// Keys visible to a query: the last `window` positions including itself.
    #[serde(default)]
    pub window: Option<usize>,
    /// One learned logit per head joined to the softmax and dropped (gpt-oss).
    #[serde(default)]
    pub sinks: bool,
    /// `q_proj` also emits a per-head gate (`[q, gate]` per head); output `*= σ(gate)` (Qwen3-Next).
    #[serde(default)]
    pub output_gate: bool,
    /// Chunked attention (Llama-4): a query at `p` sees only the keys of its own chunk,
    /// `[p − p mod c, p]`. The history then keeps `c` rows (`window` is `Some(c)`).
    #[serde(default)]
    pub chunk: Option<usize>,
    /// Llama-4's attention temperature on its NoPE layers: `q ·= ln(1 + ⌊(p + 1) / floor⌋)·scale + 1`.
    #[serde(default)]
    pub q_temperature: Option<QTemperature>,
    /// A norm on each value head (Gemma-4: RMS without a gain).
    #[serde(default)]
    pub v_norm: Option<QkNorm>,
    /// The values are the key projection's output, before its norm and rotation (Gemma-4's
    /// `attention_k_eq_v`): no `v_proj`.
    #[serde(default)]
    pub v_from_k: bool,
    /// The HL name of this attention's params and histories (`attn` unless given): layers whose
    /// projections differ in shape (Gemma-4's wider global heads) need names of their own.
    #[serde(default)]
    pub param_prefix: Option<String>,
    /// Gemma-3n/4's KV sharing: this layer's keys and values ride to later layers, or are an
    /// earlier layer's.
    #[serde(default)]
    pub kv_share: Option<KvShare>,
    /// Block-sparse key selection (`ATTN_SPARSE_BLOCK_V1`): each query attends to the tokens of its
    /// top-scoring blocks of earlier keys, plus the tokens of the block still being filled.
    #[serde(default)]
    pub sparse: Option<SparseBlockSpec>,
}

/// **`ATTN_SPARSE_BLOCK_V1`** — a learned index chooses which blocks of the history a query reads.
///
/// For a query at position `p` with `p + 1` visible tokens, the complete blocks are the
/// `n = ⌊(p + 1) / ratio⌋` blocks `[b·ratio, (b + 1)·ratio)`. An *indexer* — `index_heads` query
/// heads of `index_dim` and one key head, a projection of the layer's input (`q` normed and rotated
/// at `p`; the raw key row of every position kept as a history) — scores each complete block `b`:
/// the mean of its raw keys, normed (`k_norm`) and rotated at its first position `b·ratio`, dotted
/// with each query head, `ReLU`-ed, summed over the heads and divided by `√index_dim`. The
/// `top_blocks` best (`min(top_blocks, n)`) are kept, **higher score first, equal scores to the
/// lower block index** (the TIR `TopK` rule, 04b §6.6). The attention then sees the tokens of the
/// kept blocks and every token of the incomplete tail `[n·ratio, p]` — itself included — and
/// nothing else; the rest is the layer's ordinary (causal) attention.
///
/// Data-dependent *values*, never data-dependent *shapes*: the lowered program takes a fixed-K
/// `TopK` over every block the window can hold, with the blocks not yet complete masked to the
/// lowest score; a pick of such a block selects only tokens the tail already selects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SparseBlockSpec {
    pub index_heads: usize,
    pub index_dim: usize,
    /// Tokens per block.
    pub ratio: usize,
    /// Blocks kept per query (`budget / ratio`).
    pub top_blocks: usize,
    /// RMS-type norm of each indexer query head (one gain of `index_dim`, shared by the heads).
    pub q_norm: NormSpec,
    /// The same on a block's pooled key.
    pub k_norm: NormSpec,
}

/// Gemma-3n/4's KV sharing (`num_kv_shared_layers`), through the carries between layers: carry
/// `1 + 2·slot` holds a key row and `2 + 2·slot` its value row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KvShare {
    /// The last layer of its kind before the sharing ones: its keys (normed and rotated) and values
    /// (normed) go out on the slot's carries.
    Source { slot: usize },
    /// A layer that projects no keys or values: it appends the slot's rows to a history of its
    /// own and attends over it — the source's history, row for row.
    Consumer { slot: usize },
}

/// A query temperature by position: Llama-4's `attn_temperature_tuning` (`floor_scale`,
/// `attn_scale`, over `p + 1`) and Ministral-3's `llama_4_scaling_beta` (over the original context
/// length, over `p`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct QTemperature {
    pub floor: usize,
    pub scale: f64,
    /// Added to the position before the division: 1 (Llama-4) or 0 (Ministral-3).
    pub offset: usize,
}

impl QTemperature {
    /// The factor at position `p`: `log1p(floor((p + offset) / floor)) · scale + 1` in float32 as
    /// transformers computes it. Its float32 quotient is exact below `2^23` positions, so the floor
    /// is the integer one.
    pub fn at(&self, p: usize) -> f32 {
        self.of_quotient((p + self.offset) / self.floor.max(1))
    }

    /// The factor for `floor((p + 1) / floor) = q`.
    pub fn of_quotient(&self, q: usize) -> f32 {
        crate::detmath::ln_1p_f32(q as f32) * self.scale as f32 + 1.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MlaSpec {
    pub heads: usize,
    pub q_lora_rank: Option<usize>,
    pub kv_lora_rank: usize,
    pub qk_nope_head_dim: usize,
    pub qk_rope_head_dim: usize,
    pub v_head_dim: usize,
    pub q_a_norm: NormSpec,
    pub kv_a_norm: NormSpec,
    /// Bias on `q_a_proj` / `kv_a_proj_with_mqa` (`attention_bias`).
    #[serde(default)]
    pub a_bias: bool,
    pub rope: RopeSpec,
    pub scale: f64,
}

/// Which key head a value head reads when `v_heads > k_heads`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadMap {
    /// `kh = vh / (v_heads / k_heads)` — `repeat_interleave`, what HF Qwen3-Next does.
    Group,
    /// `kh = vh % k_heads` — `repeat`/tile, what the live MISAKA Q36 kernel hard-codes.
    Tile,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GdnLayout {
    /// `in_proj_qkvz` / `in_proj_ba`, grouped per key head (Qwen3-Next).
    #[default]
    FusedPerKeyHead,
    /// `in_proj_qkv`, `in_proj_z`, `in_proj_b`, `in_proj_a`, plain concatenation (Qwen3.5).
    Split,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GdnSpec {
    pub k_heads: usize,
    pub v_heads: usize,
    pub k_dim: usize,
    pub v_dim: usize,
    pub conv_kernel: usize,
    pub head_map: HeadMap,
    pub norm_eps: f64,
    pub l2_eps: f64,
    /// The activation of the output norm's gate (`output_gate_type`): SiLU unless the config says
    /// sigmoid.
    #[serde(default = "silu")]
    pub gate_act: Act,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MambaSpec {
    pub inner: usize,
    pub state: usize,
    pub conv_kernel: usize,
    pub dt_rank: usize,
    #[serde(default)]
    pub conv_bias: bool,
    #[serde(default)]
    pub proj_bias: bool,
    /// Norms on `dt`, `B`, `C` after `x_proj` (Jamba: weighted RMS; FalconMamba: unweighted).
    #[serde(default)]
    pub bcdt_norm: Option<NormSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mamba2Spec {
    pub heads: usize,
    pub head_dim: usize,
    pub groups: usize,
    pub state: usize,
    pub conv_kernel: usize,
    #[serde(default)]
    pub conv_bias: bool,
    #[serde(default)]
    pub proj_bias: bool,
    pub norm_eps: f64,
    /// Groups of the gated RMSNorm (1 = over the full inner width).
    pub norm_groups: usize,
    pub dt_min: f64,
    /// `+∞` when unbounded: JSON has no infinity, so `null` (what serialisation writes for it) is read back as `+∞`.
    #[serde(default = "infinity", deserialize_with = "f64_or_infinity")]
    pub dt_max: f64,
    /// Leading `2·d_mlp` rows of `in_proj` that HF discards.
    pub d_mlp: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RwkvTimeSpec {
    pub version: u8,
    pub attn_dim: usize,
    pub heads: usize,
    pub head_size: usize,
    /// RWKV-5/6: the WKV output is divided by this before the group norm.
    pub head_size_divisor: f64,
    pub gn_eps: f64,
    /// RWKV-6: the token-shift LoRA width; RWKV-7: `[decay, a, v, gate]` LoRA widths.
    pub lora: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RwkvChannelSpec {
    pub version: u8,
    pub intermediate: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mixer {
    Attention(AttnSpec),
    Mla(MlaSpec),
    GatedDeltaNet(GdnSpec),
    Mamba(MambaSpec),
    Mamba2(Mamba2Spec),
    RwkvTime(RwkvTimeSpec),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MlpLayout {
    /// `gate_proj`, `up_proj`, `down_proj` (or `fc1`/`fc2` when not gated).
    #[default]
    Separate,
    /// One `gate_up` tensor, rows `[gate | up]` (Phi-3, GraniteMoE experts).
    FusedGateFirst,
    /// One `gate_up` tensor, rows interleaved `g0,u0,g1,u1,…` (gpt-oss).
    FusedInterleaved,
    /// Experts stored `[E, in, out]`: `gate_up_proj [E, D, 2I]` with the gate columns first,
    /// `down_proj [E, I, D]` (Llama-4).
    FusedGateFirstInOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Glu {
    /// `act(gate) · up`.
    #[default]
    Standard,
    /// gpt-oss: `(clamp(up,−l,l)+1) · g·σ(α·g)` with `g = min(gate, l)`.
    ClampedSwiGlu { alpha: f64, limit: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MlpSpec {
    pub intermediate: usize,
    pub act: Act,
    pub gated: bool,
    #[serde(default)]
    pub glu: Glu,
    #[serde(default)]
    pub up_bias: bool,
    #[serde(default)]
    pub down_bias: bool,
    /// The HL name of the MLP's params (`mlp` unless given): layers whose MLPs differ in width
    /// (Gemma-4's double-wide KV-sharing layers, Gemma-3n's per-layer widths) need their own.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scoring {
    /// `softmax` over all experts, then top-k of the probabilities.
    Softmax,
    /// `sigmoid` per expert, then top-k (DeepSeek-V3).
    Sigmoid,
    /// top-k of the raw logits, then `softmax` over the k (gpt-oss, GraniteMoE).
    TopKThenSoftmax,
    /// top-k of the raw logits, then `sigmoid` of each kept one (Llama-4).
    TopKThenSigmoid,
    /// Phi-3.5-MoE's `sparsemixer` at inference (top-2 only): the argmax `i1`, weighted by the
    /// softmax at `i1` of the logits within `jitter_eps` of the max — `(m − s_j) / max(|s_j|, m) ≤
    /// 2·jitter_eps`, the rest masked — then the argmax `i2` of the others, weighted the same way
    /// (the threshold on the original logits, `i1` masked).
    SparseMixer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupScore {
    /// DeepSeek-V2: the max score in the group.
    Max,
    /// DeepSeek-V3: the sum of the group's top-2 (bias-corrected) scores.
    Top2Sum,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupRouting {
    pub n_group: usize,
    pub topk_group: usize,
    pub score: GroupScore,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouterSpec {
    pub scoring: Scoring,
    /// Bias of the router linear (gpt-oss).
    #[serde(default)]
    pub linear_bias: bool,
    /// A per-expert bias added for SELECTION only (DeepSeek-V3 `e_score_correction_bias`).
    #[serde(default)]
    pub selection_bias: bool,
    #[serde(default)]
    pub groups: Option<GroupRouting>,
    /// Renormalise the k selected weights to sum to one.
    #[serde(default)]
    pub normalize: bool,
    #[serde(default)]
    pub norm_eps: f64,
    /// Multiplier on the final weights (`routed_scaling_factor`).
    #[serde(default = "one")]
    pub scale: f64,
    /// `sparsemixer`'s `router_jitter_noise` (0 for every other scoring).
    #[serde(default)]
    pub jitter_eps: f64,
    /// Gemma-4: each selected weight times a learned per-expert scale (`per_expert_scale[e]`), after
    /// the renormalisation.
    #[serde(default)]
    pub per_expert_scale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedExpertSpec {
    pub intermediate: usize,
    /// Qwen-MoE: `σ(shared_expert_gate · x)` scales the shared expert.
    #[serde(default)]
    pub sigmoid_gate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoeSpec {
    pub experts: usize,
    pub top_k: usize,
    pub intermediate: usize,
    pub act: Act,
    #[serde(default)]
    pub glu: Glu,
    #[serde(default)]
    pub expert_bias: bool,
    pub router: RouterSpec,
    #[serde(default)]
    pub shared: Option<SharedExpertSpec>,
    /// Llama-4: each selected expert reads `w · x` (its routing weight scales the INPUT) and the
    /// outputs are summed unweighted.
    #[serde(default)]
    pub input_scaled: bool,
}

/// Gemma-4's MoE block beside its MLP: `f = mlp_post(mlp(pre_ffn(x))) + moe_post(moe(moe_pre(x)))`,
/// the router reading `router_norm(x)·router_scale` — every branch from the layer's residual `x`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MlpMoeSpec {
    pub mlp: MlpSpec,
    pub moe: MoeSpec,
    pub mlp_post: NormSpec,
    pub moe_pre: NormSpec,
    pub moe_post: NormSpec,
    /// A weightless RMSNorm times a learned vector: a `Gain::W` norm whose gain is that vector.
    pub router_norm: NormSpec,
    pub router_scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Ffn {
    None,
    Mlp(MlpSpec),
    Moe(MoeSpec),
    RwkvChannel(RwkvChannelSpec),
    /// Gemma-4 (only under [`Residual::Sandwich`]).
    MlpMoe(Box<MlpMoeSpec>),
}

/// Gemma-3n/4's per-layer input (PLE), for layer `l` from the token and its scaled embedding `e`:
/// `ple = (norm(P_l·e·proj_scale) + T_l[token]·table_scale)·combine_scale`, then
/// `x += post_norm(W_out·(act(W_gate·x) ⊙ ple))`. `P_l` and `T_l` are the layer's slices of two
/// tensors packed over the layers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PleSpec {
    pub dim: usize,
    pub vocab: usize,
    pub table_scale: f64,
    pub proj_scale: f64,
    pub norm: NormSpec,
    pub combine_scale: f64,
    pub act: Act,
    pub post_norm: NormSpec,
}

/// The residual wiring of one layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Residual {
    /// `x += m·post_mix(mixer(pre_mix(x)))`, then `x += m·post_ffn(ffn(pre_ffn(x)))`.
    Sequential {
        pre_mixer: Option<NormSpec>,
        post_mixer: Option<NormSpec>,
        pre_ffn: Option<NormSpec>,
        post_ffn: Option<NormSpec>,
        multiplier: f64,
    },
    /// `x += mixer(n(x)) + ffn(n'(x))` with `n' = n` when `ffn_norm` is `None`.
    Parallel { norm: NormSpec, ffn_norm: Option<NormSpec> },
    /// Post-LN: `x = n1(x + mixer(x))`, `x = n2(x + ffn(x))` (OPT-350m).
    PostNorm { mixer_norm: NormSpec, ffn_norm: NormSpec },
    /// Gemma-4: Gemma's four norms around the mixer and the FFN (an [`Ffn::MlpMoe`] reads the
    /// residual itself), then the per-layer input as a branch of its own, then the layer's output
    /// times a learned per-layer scalar (`layer_scalar`).
    Sandwich {
        pre_mixer: NormSpec,
        post_mixer: NormSpec,
        pre_ffn: NormSpec,
        post_ffn: NormSpec,
        ple: Option<PleSpec>,
        layer_scalar: bool,
    },
    /// **`RESIDUAL_GATED_HC_V1`** — hyper-connections: the residual stream is [`HyperSpec::streams`]
    /// rows of the hidden width (flattened), and each of the layer's two halves (the mixer, then
    /// the FFN) reads a learned mix of the streams and writes back into every stream:
    ///
    /// ```text
    ///   x̂ = hc_norm(h)                           // per stream
    ///   w = σ(up(silu(down(x̂) / streams)))       // [streams, d] mixing weights
    ///   y = block(mean over streams of w ⊙ x̂)    // the mixer or the FFN
    ///   g = 2·σ(inject(x̂) / streams)             // [streams] injection weights
    ///   h ← h + g ⊗ y
    /// ```
    ///
    /// With a [`NgramPleSpec`] the layer first adds a hashed n-gram embedding to every stream
    /// (`EMBED_NGRAM_PLE_V1`).
    HyperConnection {
        #[serde(default)]
        ple: Option<NgramPleSpec>,
    },
}

/// The model-wide half of [`Residual::HyperConnection`]: how many streams the residual carries
/// (the embedding is repeated into each), the mixer's low rank, and the norm every stream gets.
/// Before the head the streams are mixed down to the hidden width by one more mix (no injection).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HyperSpec {
    pub streams: usize,
    pub lowrank: usize,
    /// An RMS norm over each stream (`groups = streams`), gain `1 + w` of the full flattened width.
    pub norm: NormSpec,
}

/// **`EMBED_NGRAM_PLE_V1`** — a per-layer embedding of hashed token n-grams.
///
/// For each order `n` in `2 ..= ngram_size`, `heads_per_ngram` hash heads map the last `n` tokens
/// (those since the last `eos`; earlier slots read `eos`) to a row of a table:
///
/// ```text
///   mixed_n = t_0·m_0  XOR  t_1·m_1  XOR … XOR  t_{n-1}·m_{n-1}      (64-bit; t_i = token i back)
///   id      = mixed_n mod prime_head + offset_head
/// ```
///
/// `m` are the layer's multipliers (derived from `seed`, the vocabulary and the layer's index among
/// the PLE layers) and `prime_head` the head's table size (the n-th prime after `vocab_base`); both
/// are functions of this spec ([`crate::ngram`]). The rows are concatenated (`embed_dim` wide),
/// projected to a per-stream key and a shared value, gated against the layer's normed streams, and
/// a dilated depthwise causal convolution (`conv_dilation = ngram_size`) adds local context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NgramPleSpec {
    pub ngram_size: usize,
    pub heads_per_ngram: usize,
    /// Width of the concatenated rows (`ple_embed_dim`, the hidden size when the config says none).
    pub embed_dim: usize,
    pub conv_kernel: usize,
    pub conv_dilation: usize,
    /// RMS norm (`1 + w`) of the key, the query and the convolution's input, each over `streams`
    /// groups of the hidden width.
    pub norm: NormSpec,
    /// The end-of-sequence id: it fills the slots before a sequence starts and ends a segment.
    pub eos_id: usize,
    pub seed: u64,
    /// The smallest head table size is the first prime above `vocab_base − 1`.
    pub vocab_base: usize,
    /// The table's rows are padded up to a multiple of this.
    pub vocab_divisor: usize,
    /// This layer's index among the layers that have one (it enters the hash constants).
    pub layer_index: usize,
    /// The token vocabulary (it bounds the multipliers).
    pub unigram_vocab: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerSpec {
    pub mixer: Mixer,
    pub ffn: Ffn,
    pub residual: Residual,
    /// Multiplier on the layer output (RWKV `rescale_every`: 0.5 every N layers; else 1).
    #[serde(default = "one")]
    pub post_scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LearnedPositions {
    pub rows: usize,
    /// Row = position + offset (OPT: 2).
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingSpec {
    /// Width of the token table (OPT-350m's `word_embed_proj_dim` differs from `hidden_size`).
    pub dim: usize,
    /// Multiplier on the embedding (Gemma `√d`, Granite `embedding_multiplier`, MiniCPM `scale_emb`).
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub positions: Option<LearnedPositions>,
    /// LayerNorm on the embedding (BLOOM `word_embeddings_layernorm`, RWKV `pre_ln`).
    #[serde(default)]
    pub norm: Option<NormSpec>,
    /// `project_in` (OPT-350m).
    #[serde(default)]
    pub proj_in: bool,
    /// A token-type table of this many rows (BERT's `token_type_embeddings`); a single-segment
    /// encoder adds row 0 to every position.
    #[serde(default)]
    pub type_rows: Option<usize>,
    /// A bidirectional encoder's bias over bucketed relative positions, one table shared by every
    /// layer (MPNet: T5's bidirectional buckets). Read by `lower::bidir` only.
    #[serde(default)]
    pub rel_bias: Option<RelBiasSpec>,
}

/// `table[bucket(j − i), head]` added to the scaled scores of query `i` and key `j`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelBiasSpec {
    pub buckets: usize,
    pub max_distance: usize,
    pub heads: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadSpec {
    pub tied: bool,
    #[serde(default)]
    pub bias: bool,
    /// Multiplier on the final hidden state before the head (MiniCPM `1/(d/dim_model_base)`).
    #[serde(default = "one")]
    pub pre_scale: f64,
    /// `project_out` to the embedding width (OPT-350m).
    #[serde(default)]
    pub proj_out: bool,
    /// Multiplier on the logits (Cohere `logit_scale`, Granite `1/logits_scaling`).
    #[serde(default = "one")]
    pub logit_scale: f64,
    /// `tanh(z / c) · c` on the logits (Gemma-2).
    #[serde(default)]
    pub softcap: Option<f64>,
}

/// What a program's `post` produces (RFC-0003 §I.2.3's output kinds are chosen per class, from it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OutputSpec {
    /// A language model's logits, through the head.
    #[default]
    Logits,
    /// An encoder's embedding: the hidden row after `final_norm`, then an optional projection
    /// (`width`, `bias`: CLIP's `text_projection`), then an optional L2 normalisation.
    Embedding { proj: Option<(usize, bool)>, normalize: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    /// `architectures[0]` as given.
    pub architecture: String,
    pub model_type: String,
    /// RFC-0002 §1.1 corpus families this architecture exercises.
    #[serde(default)]
    pub families: Vec<String>,
    #[serde(default)]
    pub reference: Reference,
    #[serde(default)]
    pub confidence: Confidence,
    pub vocab_size: usize,
    pub hidden_size: usize,
    #[serde(default)]
    pub max_position_embeddings: Option<usize>,
    pub embedding: EmbeddingSpec,
    pub layers: Vec<LayerSpec>,
    pub final_norm: Option<NormSpec>,
    pub head: HeadSpec,
    /// The residual's multiple streams, when the layers are [`Residual::HyperConnection`].
    #[serde(default)]
    pub hyper: Option<HyperSpec>,
    /// Logits, or an encoder's embedding.
    #[serde(default)]
    pub output: OutputSpec,
    /// A LoRA adapter over this model (`crate::lora`): the candidate = parent + adapter.
    #[serde(default, skip_deserializing)]
    pub adapter: Option<crate::lora::LoraAdapter>,
    /// How the checkpoint stores the weights. Read only by `crate::hf_weights`; the HL builder
    /// never looks at it, so the HL graph is the same whichever frontend produced the spec.
    pub hf: HfStorage,
    /// Things a reader of this spec should know (HF quirks followed on purpose, …).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ModelSpec {
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }
}

/// The name this type had before it became the feature-based [`ModelSpec`] V1; kept so existing
/// users (`misaka-palw-sdk`, the tests, the other lowerer lanes) keep compiling.
pub type ArchSpec = ModelSpec;

/// **Hugging Face weight storage** — tensor names and fused layouts. Everything HF-specific about
/// the weights lives here and in `crate::hf_weights`; an ONNX or GGUF importer would fill its own
/// storage description and produce the identical HL graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HfStorage {
    /// Tensor-name templates by role (`{L}` = layer, `{E}` = expert, `{H}` = head).
    pub names: BTreeMap<String, String>,
    /// Alternative spellings of a name prefix (`transformer.` vs none for the hub's `gpt2`).
    #[serde(default)]
    pub prefix_aliases: Vec<(String, String)>,
    /// GPT-2's `Conv1D` stores weights `[in, out]`.
    #[serde(default)]
    pub conv1d_weights: bool,
    #[serde(default)]
    pub qkv: QkvLayout,
    #[serde(default)]
    pub mlp: MlpLayout,
    #[serde(default)]
    pub experts: MlpLayout,
    #[serde(default)]
    pub gdn: GdnLayout,
    /// Checkpoint tensors a text-only lowering does not read by design (a VLM's vision tower
    /// and projector, multi-token-prediction heads). Every OTHER unread tensor is reported.
    #[serde(default)]
    pub ignored_prefixes: Vec<String>,
    /// A pre-quantised checkpoint (GPTQ, AWQ): which linears are stored as integers, and how.
    #[serde(default, skip_deserializing)]
    pub quant: Option<crate::prequant::QuantConfig>,
    /// A per-layer embedding table the checkpoint stores in this many equal row shards (`{S}` in its
    /// tensor-name template, concatenated in order): Qwen4-Exp's n-gram tables (`split_ngram_parts`).
    #[serde(default = "one_shard")]
    pub table_shards: usize,
}

fn one_shard() -> usize {
    1
}

impl HfStorage {
    pub fn name(&self, role: &str) -> Option<&str> {
        self.names.get(role).map(String::as_str)
    }
}
