//! **`ArchSpec`: one normalised description of a Hugging Face decoder.**
//!
//! Every architecture parser (`crate::arch`) produces this, and everything downstream — the HL
//! graph builder, the weight mapper, the float reference, the cost estimate — reads only this. Two
//! checkpoints with equal `ArchSpec`s (up to naming) compute the same function.
//!
//! The spec is fully expanded per layer (`layers[i]`). The HL builder groups equal layers into
//! block kinds, which is where the RFC's `schedule.kinds` comes from.

use crate::rope::{AlibiSpec, RopeSpec};
use serde::Serialize;
use std::collections::BTreeMap;

/// Where the reference semantics come from. A checkpoint that needs `trust_remote_code` is
/// modelled only for remote-code modules this lowerer names; everything else is refused.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Reference {
    /// `transformers`' own modeling file for `model_type`.
    Native,
    /// A `trust_remote_code` module shipped with the checkpoint (pinned by revision in practice).
    RemoteCode { module: String },
    /// A third-party library the checkpoint names (e.g. flash-linear-attention for RWKV-7).
    ExternalLibrary { name: String },
}

/// How sure this lowerer is of the semantics. `Unsure` lists what to confirm against HF fixtures.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Confidence {
    /// From the `transformers` implementation as written; confirmed by fixtures when present.
    Known,
    /// Semantics or naming reconstructed with gaps; each string is one thing to check.
    Unsure(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum NormKind {
    /// `x * rsqrt(mean(x²) + eps)`.
    Rms,
    /// `(x - mean) * rsqrt(var + eps)` with the biased variance.
    Layer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Gain {
    /// Non-parametric (OLMo-1, FalconMamba's B/C/dt norms).
    None,
    /// `w * x̂`.
    W,
    /// `(1 + w) * x̂` (Gemma, Qwen3-Next, Nemotron's LayerNorm1P).
    OnePlusW,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct NormSpec {
    pub kind: NormKind,
    pub eps: f64,
    pub gain: Gain,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum QkvLayout {
    /// `q_proj`, `k_proj`, `v_proj`.
    Separate,
    /// One tensor, rows `[q (H·d) | k (KV·d) | v (KV·d)]` (Phi-3, MPT, GPT-2, GPTBigCode MQA).
    FusedConcat,
    /// One tensor, rows `[head][q,k,v][d]` (GPT-NeoX, BLOOM, Falcon MHA, GPTBigCode MHA).
    FusedPerHead,
    /// One tensor, rows `[kv_head][q_0..q_{g−1}, k, v][d]` (Falcon new decoder, InternLM2).
    FusedPerKvGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum QkNormScope {
    /// One gain of `head_dim`, shared by every head (Qwen3, Gemma3, Qwen3-Next).
    PerHeadShared,
    /// Per-head gains `[heads, head_dim]` (Cohere `use_qk_norm`, StableLM `qk_layernorm`).
    PerHeadSeparate,
    /// One norm over the whole projection `[heads·head_dim]` (OLMo-2, OLMoE).
    Whole,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct QkNorm {
    pub norm: NormSpec,
    pub scope: QkNormScope,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Position {
    /// No positional term in attention (Jamba, Cohere2/Exaone4/SmolLM3 global layers, …).
    None,
    Rope(RopeSpec),
    Alibi(AlibiSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AttnSpec {
    pub heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub v_head_dim: usize,
    pub q_bias: bool,
    pub k_bias: bool,
    pub v_bias: bool,
    pub o_bias: bool,
    pub qk_norm: Option<QkNorm>,
    /// `clamp(q|k|v, −c, c)` after projection (OLMo `clip_qkv`, MPT).
    pub clip_qkv: Option<f64>,
    pub position: Position,
    /// Softmax scale applied to `q·k` (already includes every multiplier).
    pub scale: f64,
    /// `tanh(s / c) · c` on the scaled scores (Gemma-2).
    pub softcap: Option<f64>,
    /// Keys visible to a query: the last `window` positions including itself.
    pub window: Option<usize>,
    /// One learned logit per head joined to the softmax and dropped (gpt-oss).
    pub sinks: bool,
    /// `q_proj` also emits a per-head gate (`[q, gate]` per head); output `*= σ(gate)` (Qwen3-Next).
    pub output_gate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
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
    pub a_bias: bool,
    pub rope: RopeSpec,
    pub scale: f64,
}

/// Which key head a value head reads when `v_heads > k_heads`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HeadMap {
    /// `kh = vh / (v_heads / k_heads)` — `repeat_interleave`, what HF Qwen3-Next does.
    Group,
    /// `kh = vh % k_heads` — `repeat`/tile, what the live MISAKA Q36 kernel hard-codes.
    Tile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GdnLayout {
    /// `in_proj_qkvz` / `in_proj_ba`, grouped per key head (Qwen3-Next).
    FusedPerKeyHead,
    /// `in_proj_qkv`, `in_proj_z`, `in_proj_b`, `in_proj_a`, plain concatenation (Qwen3.5).
    Split,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GdnSpec {
    pub k_heads: usize,
    pub v_heads: usize,
    pub k_dim: usize,
    pub v_dim: usize,
    pub conv_kernel: usize,
    pub head_map: HeadMap,
    pub norm_eps: f64,
    pub l2_eps: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MambaSpec {
    pub inner: usize,
    pub state: usize,
    pub conv_kernel: usize,
    pub dt_rank: usize,
    pub conv_bias: bool,
    pub proj_bias: bool,
    /// Norms on `dt`, `B`, `C` after `x_proj` (Jamba: weighted RMS; FalconMamba: unweighted).
    pub bcdt_norm: Option<NormSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mamba2Spec {
    pub heads: usize,
    pub head_dim: usize,
    pub groups: usize,
    pub state: usize,
    pub conv_kernel: usize,
    pub conv_bias: bool,
    pub proj_bias: bool,
    pub norm_eps: f64,
    /// Groups of the gated RMSNorm (1 = over the full inner width).
    pub norm_groups: usize,
    pub dt_min: f64,
    pub dt_max: f64,
    /// Leading `2·d_mlp` rows of `in_proj` that HF discards.
    pub d_mlp: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RwkvChannelSpec {
    pub version: u8,
    pub intermediate: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Mixer {
    Attention(AttnSpec),
    Mla(MlaSpec),
    GatedDeltaNet(GdnSpec),
    Mamba(MambaSpec),
    Mamba2(Mamba2Spec),
    RwkvTime(RwkvTimeSpec),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MlpLayout {
    /// `gate_proj`, `up_proj`, `down_proj` (or `fc1`/`fc2` when not gated).
    Separate,
    /// One `gate_up` tensor, rows `[gate | up]` (Phi-3, GraniteMoE experts).
    FusedGateFirst,
    /// One `gate_up` tensor, rows interleaved `g0,u0,g1,u1,…` (gpt-oss).
    FusedInterleaved,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Glu {
    /// `act(gate) · up`.
    Standard,
    /// gpt-oss: `(clamp(up,−l,l)+1) · g·σ(α·g)` with `g = min(gate, l)`.
    ClampedSwiGlu { alpha: f64, limit: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MlpSpec {
    pub intermediate: usize,
    pub act: Act,
    pub gated: bool,
    pub glu: Glu,
    pub up_bias: bool,
    pub down_bias: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Scoring {
    /// `softmax` over all experts, then top-k of the probabilities.
    Softmax,
    /// `sigmoid` per expert, then top-k (DeepSeek-V3).
    Sigmoid,
    /// top-k of the raw logits, then `softmax` over the k (gpt-oss, GraniteMoE).
    TopKThenSoftmax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GroupScore {
    /// DeepSeek-V2: the max score in the group.
    Max,
    /// DeepSeek-V3: the sum of the group's top-2 (bias-corrected) scores.
    Top2Sum,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GroupRouting {
    pub n_group: usize,
    pub topk_group: usize,
    pub score: GroupScore,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouterSpec {
    pub scoring: Scoring,
    /// Bias of the router linear (gpt-oss).
    pub linear_bias: bool,
    /// A per-expert bias added for SELECTION only (DeepSeek-V3 `e_score_correction_bias`).
    pub selection_bias: bool,
    pub groups: Option<GroupRouting>,
    /// Renormalise the k selected weights to sum to one.
    pub normalize: bool,
    pub norm_eps: f64,
    /// Multiplier on the final weights (`routed_scaling_factor`).
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SharedExpertSpec {
    pub intermediate: usize,
    /// Qwen-MoE: `σ(shared_expert_gate · x)` scales the shared expert.
    pub sigmoid_gate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MoeSpec {
    pub experts: usize,
    pub top_k: usize,
    pub intermediate: usize,
    pub act: Act,
    pub glu: Glu,
    pub expert_bias: bool,
    pub router: RouterSpec,
    pub shared: Option<SharedExpertSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Ffn {
    None,
    Mlp(MlpSpec),
    Moe(MoeSpec),
    RwkvChannel(RwkvChannelSpec),
}

/// The residual wiring of one layer.
#[derive(Clone, Debug, PartialEq, Serialize)]
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
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LayerSpec {
    pub mixer: Mixer,
    pub ffn: Ffn,
    pub residual: Residual,
    /// Multiplier on the layer output (RWKV `rescale_every`: 0.5 every N layers; else 1).
    pub post_scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LearnedPositions {
    pub rows: usize,
    /// Row = position + offset (OPT: 2).
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmbeddingSpec {
    /// Width of the token table (OPT-350m's `word_embed_proj_dim` differs from `hidden_size`).
    pub dim: usize,
    /// Multiplier on the embedding (Gemma `√d`, Granite `embedding_multiplier`, MiniCPM `scale_emb`).
    pub scale: f64,
    pub positions: Option<LearnedPositions>,
    /// LayerNorm on the embedding (BLOOM `word_embeddings_layernorm`, RWKV `pre_ln`).
    pub norm: Option<NormSpec>,
    /// `project_in` (OPT-350m).
    pub proj_in: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeadSpec {
    pub tied: bool,
    pub bias: bool,
    /// Multiplier on the final hidden state before the head (MiniCPM `1/(d/dim_model_base)`).
    pub pre_scale: f64,
    /// `project_out` to the embedding width (OPT-350m).
    pub proj_out: bool,
    /// Multiplier on the logits (Cohere `logit_scale`, Granite `1/logits_scaling`).
    pub logit_scale: f64,
    /// `tanh(z / c) · c` on the logits (Gemma-2).
    pub softcap: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArchSpec {
    /// `architectures[0]` as given.
    pub architecture: String,
    pub model_type: String,
    /// RFC-0002 §1.1 corpus families this architecture exercises.
    pub families: Vec<&'static str>,
    pub reference: Reference,
    pub confidence: Confidence,
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub max_position_embeddings: Option<usize>,
    pub embedding: EmbeddingSpec,
    pub layers: Vec<LayerSpec>,
    pub final_norm: Option<NormSpec>,
    pub head: HeadSpec,
    /// A LoRA adapter over this model (`crate::lora`): the candidate = parent + adapter.
    pub adapter: Option<crate::lora::LoraAdapter>,
    /// How the checkpoint stores the weights. Read only by `crate::hf_weights`; the HL builder
    /// never looks at it, so the HL graph is the same whichever frontend produced the spec.
    pub hf: HfStorage,
    /// Things a reader of this spec should know (HF quirks followed on purpose, …).
    pub notes: Vec<String>,
}

impl ArchSpec {
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }
}

/// **Hugging Face weight storage** — tensor names and fused layouts. Everything HF-specific about
/// the weights lives here and in `crate::hf_weights`; an ONNX or GGUF importer would fill its own
/// storage description and produce the identical HL graph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HfStorage {
    /// Tensor-name templates by role (`{L}` = layer, `{E}` = expert, `{H}` = head).
    pub names: BTreeMap<String, String>,
    /// Alternative spellings of a name prefix (`transformer.` vs none for the hub's `gpt2`).
    pub prefix_aliases: Vec<(String, String)>,
    /// GPT-2's `Conv1D` stores weights `[in, out]`.
    pub conv1d_weights: bool,
    pub qkv: QkvLayout,
    pub mlp: MlpLayout,
    pub experts: MlpLayout,
    pub gdn: GdnLayout,
    /// Checkpoint tensors a text-only lowering does not read by design (a VLM's vision tower
    /// and projector, multi-token-prediction heads). Every OTHER unread tensor is reported.
    pub ignored_prefixes: Vec<String>,
}

impl HfStorage {
    pub fn name(&self, role: &str) -> Option<&str> {
        self.names.get(role).map(String::as_str)
    }
}
