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

fn sigmoid() -> Act {
    Act::Sigmoid
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn infinity() -> f64 {
    f64::INFINITY
}

/// An `Option` that must be written (`null` for `None`): a missing key is an error, not `None`.
fn present_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
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
    /// A `trust_remote_code` module shipped with the checkpoint (pinned by revision in practice). **`REFERENCE_REMOTE_CODE_V1`**: `pin` is the
    /// sha256 of the modelling file the lowering follows, as the adapter DECLARES it (`remote_code_pin`, the hash `tools/remote_reference.py` records when
    /// a registrant builds the tiny reference from that file); `None` when the adapter names no file. Declared, never attested: no part of the chain runs
    /// remote code, so a model of this kind is `LOWERABLE_UNVERIFIED` pinned or not.
    RemoteCode {
        module: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<String>,
    },
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
    /// `min(max(x, 0), 6)` (MobileNet v1/v2) — a clamp, so exact on codes: the narrowing's own clamp in a fixed `6/32767` unit.
    Relu6,
    /// `x·relu6(x+3)/6` (MobileNet v3, LeViT) — a table.
    HardSwish,
    /// `relu6(x+3)/6` (squeeze-excite gates of MobileNet v3) — a table.
    HardSigmoid,
    /// `relu(x)²` (Nemotron, RWKV channel mix).
    Relu2,
    Sigmoid,
    Tanh,
    Softplus,
    Identity,
    /// `sign(x)·√max(|x|, 10⁻⁶)` — the gate of Qwen4-Exp's n-gram embedding.
    SignedSqrt,
    /// **xIELU** (Apertus, `ACT_LEARNED_POINTWISE_V1`; NOT in [`Act::from_hf`]: an adapter that knows its four tensors names it
    /// by this variant, every other reader still refuses `hidden_act = "xielu"` by name): `x > 0 ? αp·x² + β·x : (expm1(min(x, ε)) − x)·αn + β·x` with
    /// `αp = softplus(p)`, `αn = β + softplus(n)` and `p`, `n`, `β`, `ε` the layer's own tensors. It reads parameters, so only a
    /// dense MLP lowers it (`Op::Xielu`); anywhere else it is refused by name — `float_ref::act` of it is NaN, never a number.
    Xielu,
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
            "relu6" => Act::Relu6,
            "hardswish" | "hard_swish" => Act::HardSwish,
            "hardsigmoid" | "hard_sigmoid" => Act::HardSigmoid,
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
    /// **`ATTN_QK_NORM_POST_ROPE_V1`**: the q/k norms act AFTER the rotation (Hunyuan:
    /// `q = rope(q); q = RMSNorm_head(q)`); Qwen3's order, the default, is the reverse. Rotation preserves a head's
    /// L2 norm but a per-channel gain does not commute with it, so the two orders are different functions. The
    /// history keeps the normed, rotated key either way.
    #[serde(default, skip_serializing_if = "is_false")]
    pub qk_norm_after_rope: bool,
    /// **`SUBLAYER_NORMS_V1`**: a norm over the attention output (all heads concatenated) before `o_proj` — BitNet's
    /// `attn_sub_norm` (`RMSNorm(hidden)`), the sub-LN of MAGNETO/RetNet-style models. Tensors `attn.sub_norm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub o_norm: Option<NormSpec>,
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
    /// **`ATTN_OUTPUT_GATE_SEPARATE_V1`**: the attention output is multiplied by `act(gate_proj(x))` from a projection of
    /// its OWN (`attn.gate`), per element or per head (AfMoE: sigmoid per element; Laguna: softplus per head). Distinct
    /// from `output_gate`, Qwen3-Next's gate fused into `q_proj`; the two are exclusive.
    #[serde(default)]
    pub gate: Option<SeparateGateSpec>,
    /// **`ATTN_VALUE_SCALE_V1`**: a constant on the values after their projection (`attention_value_scale`, MiMo-V2-Flash).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub v_scale: f64,
    /// The width of the attention's input when it is not the hidden size (`ATTN_SHARED_BLOCK_V1`: Zamba2's shared block reads
    /// `[hidden state | embedding]`, `2·hidden` wide); `q`, `k`, `v` project from it and `o` returns to the hidden width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_dim: Option<usize>,
    /// **`ATTN_DIFFERENTIAL_V1`**: differential attention (DiffLlama): every head pair `(i, i + H/2)` shares one value read, the second
    /// head's context is subtracted from the first's with a learned `λ`, and the result is RMS-normed over the pair's width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub differential: Option<DiffSpec>,
    /// **`MIXER_MOA_V1`**: mixture of attention (JetMoE): per token a router picks `top_k` of `experts` and each chosen expert
    /// supplies its own query projection (`W_in[e]`, `[kv_heads·head_dim, D]`) and output projection (`W_out[e]`); keys and values
    /// are shared (`attn.kv`, `[2·kv_heads·head_dim, D]`) and tiled across the slots. `heads` is `kv_heads · top_k`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moa: Option<MoaSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoaSpec {
    pub experts: usize,
    pub top_k: usize,
    pub router: RouterSpec,
    /// A `[D]` bias added to the weighted sum of the output projections (`self_attention.experts.bias`).
    #[serde(default)]
    pub out_bias: bool,
}

/// **`ATTN_DIFFERENTIAL_V1`** (`modular_diffllama.py:DiffLlamaAttention`). The attention runs once over the keys and a DOUBLE-WIDE value
/// `[V_g | V_{g + Hkv/2}]` per kv head (`Hkv` must be even), giving every head a context `[o1; o2]` of `2·head_dim`; the result is
/// `(1 − λ_init)·RMSNorm_{2d}(o_{i,first half of the heads} − λ·o_{i + H/2,second half})`, `λ = exp(Σ λq1⊙λk1) − exp(Σ λq2⊙λk2) + λ_init`
/// a function of the layer's weights only (a Q24 constant per layer at conversion), `λ_init = base − amp·e^{−rate·layer}`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffSpec {
    pub lambda_init: crate::weights::LambdaInit,
    pub norm_eps: f64,
}

/// A separate attention output gate: `o ·= act(gate_proj(x))`, the projection `[H·v_dim, D]` per element or `[H, D]` per head.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeparateGateSpec {
    pub act: Act,
    #[serde(default)]
    pub per_head: bool,
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

/// **`ATTN_TOKEN_INDEXER_V1`** — DeepSeek sparse attention (DSA): a learned scorer chooses which `topk` of the visible
/// tokens an MLA layer attends over (`DeepseekV32Indexer`, `GlmMoeDsaIndexer`).
///
/// For a query at position `p` the indexer reads the MLA's **q-latent** `qr = q_a_norm(q_a x)` (so the layer must have
/// `q_lora_rank`), projects it to `heads` query heads of `head_dim` (`wq_b`), rotates the first `rope.rotary_dim` lanes of
/// each (the rotated lanes come FIRST, the opposite of MLA's nope-first layout; `rope.style` is rotate-half for
/// DeepSeek-V3.2 and interleaved for GLM-MoE-DSA), and scores every visible token `t` by
///
/// ```text
/// s_t = Σ_h w_h · ReLU( head_dim^-½ · q_h · k_t ),   w = weights_proj(x) · heads^-½
/// k_t = rotate( LayerNorm(wk x_t) )                     one head, cached per position
/// ```
///
/// The attention is the layer's ordinary MLA softmax restricted to the `min(topk, p + 1)` best tokens — **ties to the
/// lowest index** (the IR's `TopK` rule; `torch.topk` leaves its tie order unspecified, and ReLU makes exact-zero scores
/// common). When the window holds at most `topk` tokens every token is selected and the layer is dense MLA.
///
/// Not [`SparseBlockSpec`]: that scores block means of raw keys with the layer input as the query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TokenIndexerSpec {
    /// `index_n_heads`.
    pub heads: usize,
    /// `index_head_dim`.
    pub head_dim: usize,
    /// `index_topk`.
    pub topk: usize,
    /// The rotation of the first `rotary_dim` lanes of the indexer's query and key (`offset` 0).
    pub rope: RopeSpec,
    /// The key's LayerNorm (gain, bias; eps 1e-6 in both implementations).
    pub k_norm: NormSpec,
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
    /// The rotation of the rope slice of q and of the shared key. **`MIXER_MLA_NOPE_V1`** (Kimi-Linear's MLA layers): `None` is no rotation at
    /// all — the shared key's slice is the raw projection and q's slice is not rotated either. Written `null` in an adapter, never omitted.
    #[serde(deserialize_with = "present_option")]
    pub rope: Option<RopeSpec>,
    pub scale: f64,
    /// **`ATTN_TOKEN_INDEXER_V1`**: DeepSeek sparse attention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexer: Option<TokenIndexerSpec>,
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

/// **`MIXER_KDA_V1`** (Kimi delta attention, `modeling_kimi_linear.py:KimiLinearDeltaAttention`): `heads` heads of width `head_dim`. q, k, v are
/// projections of the input, each through its own depthwise causal convolution of `conv_kernel` taps (no bias, `conv_act`); q and k are
/// L2-normalised per head (`l2_eps`) and q is scaled by `head_dim^-1/2`. The forget gate is **channel-wise**,
/// `g[h,c] = -exp(A_log[h]) * softplus(f_b(f_a(x))[h,c] + dt_bias[h,c])` (`f_a: D -> head_dim`, `f_b: head_dim -> heads*head_dim`), and the write
/// strength is `beta = sigmoid(b_proj x)` per head; per head `S <- S * exp(g)[:, None]`, `u = beta (v - S^T k)`, `S += k u^T`, `o = S^T q`.
/// The output is `o_norm(o, gate)` — a per-head RMS norm (gain `[head_dim]`, `norm_eps`) times `gate_act(g_b(g_a(x)))` — then `o_proj`.
/// Tensors: roles `kda.q`, `kda.k`, `kda.v`, `kda.q_conv`, `kda.k_conv`, `kda.v_conv` (`[H*d, 1, K]` each), `kda.f_a`, `kda.f_b`, `kda.dt_bias`,
/// `kda.A_log`, `kda.b`, `kda.g_a`, `kda.g_b`, `kda.norm`, `kda.out`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KdaSpec {
    pub heads: usize,
    pub head_dim: usize,
    pub conv_kernel: usize,
    /// The width of the low-rank forget-gate and output-gate projections (`f_a`, `g_a`): `head_dim` in the library's release.
    pub gate_rank: usize,
    #[serde(default = "silu")]
    pub conv_act: Act,
    pub l2_eps: f64,
    pub norm_eps: f64,
    /// The activation of the output norm's gate: the library's `KimiLinearRMSNormGated` is sigmoid.
    #[serde(default = "sigmoid")]
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
    /// **`MAMBA2_GATE_NORM_VARIANTS_V1`**: how the output gate and the RMS norm combine ([`Mamba2Norm`]).
    #[serde(default, skip_serializing_if = "Mamba2Norm::is_default")]
    pub norm_mode: Mamba2Norm,
    /// **`MAMBA2_MUP_V1`** (Falcon-H1): multipliers of the five chunks of the projection `[z | x | B | C | dt]`, applied before the convolution
    /// (HF's `mup_vector`). The convolution is channel-wise, so with scales the x, B and C channels are three linears and three convolutions
    /// of their own, each on its scaled chunk (the same function); the scales are `Op::Scale`, a change of scale key and no node of the integer program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_scales: Option<[f64; 5]>,
}

/// How a Mamba-2 mixer's output gate `z` and its RMS norm combine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mamba2Norm {
    /// `norm(y · silu(z))` — the gate first (Mamba-2, Zamba2, Bamba, Nemotron-H, Granite-hybrid).
    #[default]
    GateFirst,
    /// `norm(y) · silu(z)` — the norm first (Falcon-H1 with `mamba_norm_before_gate`).
    NormFirst,
    /// `y · silu(z)`, no norm and no gain (Falcon-H1 without `mamba_rms_norm`).
    Ungated,
}

impl Mamba2Norm {
    pub fn is_default(&self) -> bool {
        *self == Mamba2Norm::GateFirst
    }
}

/// A gated short convolution (LFM2's `Lfm2ShortConv`): the kernel of the depthwise causal convolution, and one flag that gives biases to
/// the convolution, `in_proj` and `out_proj` together (`conv_bias`). Tensors: roles `shortconv.in` (`[3D, D]`: B, C, x rows),
/// `shortconv.conv` (`[D, 1, K]`) and `shortconv.out`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortConvSpec {
    pub kernel: usize,
    #[serde(default)]
    pub bias: bool,
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
    /// **`MIXER_KDA_V1`** (Kimi-Linear): the gated delta rule with a channel-wise forget gate ([`KdaSpec`]).
    Kda(KdaSpec),
    Mamba(MambaSpec),
    Mamba2(Mamba2Spec),
    RwkvTime(RwkvTimeSpec),
    /// **`MIXER_SHORT_CONV_V1`** (LFM2): `[B | C | x] = in_proj(x)`, `u = B ⊙ x`, `v = causal_depthwise_conv(u)` (no activation),
    /// `out_proj(C ⊙ v)`.
    ShortConv(ShortConvSpec),
    /// **`LAYER_FFN_ONLY_V1`** (Nemotron-H's `mlp` and `moe` layers): no mixer — the layer is its FFN under its own pre-norm. Only under
    /// [`Residual::Sequential`] with no mixer norms, and the layer must have an FFN.
    None,
    /// **`MIXER_PARALLEL_BRANCH_V1`** (Falcon-H1): the layer's mixer is the sum of several branches reading the same normed input,
    /// each with its own input and output scale. At most one branch of each kind; no KV sharing inside.
    Parallel(Vec<Branch>),
    /// **`ATTN_CROSS_V1`** (Mllama's `cross_attn`): a layer whose attention reads another sequence's states (a vision tower's rows),
    /// never the token history. HF skips such a layer entirely when no states are given and the cache is empty
    /// (`MllamaTextModel.forward`), so the text-only stage this lowering builds DROPS these layers from the schedule (their tensors are
    /// dormant, not read); binding image rows to a spec that has them is refused by name (`fidelity::prepare_spec`).
    CrossAttention(CrossAttnSpec),
    /// **`ATTN_KV_SHARED_ROTATED_V1`** (DeepSeek-V4): multi-query attention whose keys ARE its values ([`SharedKvSpec`]).
    SharedKv(SharedKvSpec),
}

/// **DeepSeek-V4's attention** — one key/value head (`K = V = rope(norm(wkv x))`, so the output's rope slice is counter-rotated at the
/// query's position), a low-rank query with a weightless per-head norm (**`ATTN_Q_LOWRANK_V1`**), the output through block-diagonal
/// low-rank groups (**`ATTN_OUT_GROUPED_LOWRANK_V1`**), a learned sink per head, a sliding window and — in compressed layers — the
/// compressed entries ([`CompressedKvSpec`]) the query also attends to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedKvSpec {
    pub heads: usize,
    /// The width of the one key/value head.
    pub head_dim: usize,
    /// Keys visible to a query: the last `window` positions including itself.
    pub window: usize,
    pub scale: f64,
    /// `q = q_b_norm(wq_b(q_a_norm(wq_a x)))`: the latent's width, its (weighted) norm and the per-head norm of `q` (weightless).
    pub q_rank: usize,
    pub q_a_norm: NormSpec,
    pub q_b_norm: NormSpec,
    /// The norm of the key/value row (gain `w`).
    pub kv_norm: NormSpec,
    /// The rotation of the trailing `rotary_dim` lanes of `q` and of the key/value row, and the same reversed (by `−θ`), applied to
    /// the attention output at the query's position.
    pub rope: RopeSpec,
    pub rope_back: RopeSpec,
    /// `o_groups` block-diagonal projections of `heads·head_dim / o_groups` lanes to `o_rank` each, then one projection of
    /// `o_groups·o_rank` lanes to the hidden width.
    pub o_groups: usize,
    pub o_rank: usize,
    /// One learned logit per head that joins the softmax and is dropped.
    pub sinks: bool,
    /// **`ATTN_COMPRESSED_KV_V1`**: compressed entries beside the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed: Option<CompressedKvSpec>,
}

/// **`ATTN_COMPRESSED_KV_V1`** — every `ratio` tokens close a window whose rows (`kv`) are pooled into one **entry**:
/// `softmax` over the window of `gate + ape` per channel weights the `kv` rows, `norm` and a rotation at the window's first position
/// finish it. With `overlap` (CSA) `kv` and `gate` carry two series of `dim` lanes: entry `w` pools the PREVIOUS window's first
/// series with this window's second (`2·ratio` rows; window 0's previous half has weight 0). A query at `p` attends to the entries
/// `w < (p + 1) / ratio` — all of them (HCA), or the `topk` the indexer scores best (CSA) — beside its window, in one softmax.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompressedKvSpec {
    pub ratio: usize,
    pub overlap: bool,
    pub norm: NormSpec,
    /// The rotation of the entry's trailing lanes (at the window's first position).
    pub rope: RopeSpec,
    /// **`ATTN_ENTRY_INDEXER_V1`**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexer: Option<EntryIndexerSpec>,
}

/// **`ATTN_ENTRY_INDEXER_V1`** — the lightning indexer: its own compressor (at `head_dim`, same windows) makes keys; entry `t` scores
/// `Σ_h w_h · ReLU(q_h · k_t) / √head_dim` with `q = rope(wq_b(q-latent))` and `w = weights_proj(x) / √heads`; the `topk` best
/// among the visible entries are kept (ties to the lowest index).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryIndexerSpec {
    pub heads: usize,
    pub head_dim: usize,
    pub topk: usize,
    pub norm: NormSpec,
    pub rope: RopeSpec,
}

/// `q = RMS_head(q_proj x)`, `k = RMS_head(k_proj states)`, `v = v_proj states`, no rotation, grouped heads, scale `head_dim^-½`;
/// the layer's residual adds `tanh(attn_gate)·o_proj(·)` and `tanh(mlp_gate)·mlp(·)` when `gated`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossAttnSpec {
    pub heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub q_norm: NormSpec,
    pub k_norm: NormSpec,
    #[serde(default)]
    pub gated: bool,
}

/// One branch of a [`Mixer::Parallel`]: `out_scale · mixer(in_scale · x)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub mixer: Mixer,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub in_scale: f64,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub out_scale: f64,
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
    /// **`MLP_GLU_LIMITED_V1`** (DeepSeek-V4's `swiglu_limit`): `act(min(gate, L)) · clamp(up, −L, L)`.
    LimitedGlu { limit: f64 },
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
    /// **`SUBLAYER_NORMS_V1`**: a norm over the hidden activation (the gated product, or the activation of a plain MLP) before
    /// `down_proj` — BitNet's `ffn_sub_norm` (`RMSNorm(intermediate)`). Tensors `mlp.sub_norm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_norm: Option<NormSpec>,
    /// The HL name of the MLP's params (`mlp` unless given): layers whose MLPs differ in width
    /// (Gemma-4's double-wide KV-sharing layers, Gemma-3n's per-layer widths) need their own.
    #[serde(default)]
    pub name: Option<String>,
    /// **`FFN_ACTIVATION_SPARSITY_V1`** (Gemma-3n's `activation_sparsity_pattern`): the target sparsity `p ∈ (0.5, 1)` of the
    /// gate activation. Before the activation the gate row keeps only what lies above `mean + z·std` of its own width
    /// (`z = Φ⁻¹(p)`, the biased standard deviation): `gate ← relu(gate − (mean + z·std))`. `z` is a registration-time
    /// constant the HL builder computes from `p` with a fixed algorithm ([`crate::detmath::norm_inv_cdf`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparsity: Option<f64>,
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
    /// **`MLP_MOE_ROUTER_SQRTSOFTPLUS_V1`** (DeepSeek-V4): `√softplus(logit)` per expert; the top-k of the scores (plus the selection
    /// bias, when the router has one) are kept and their UNBIASED scores renormalised and scaled.
    SqrtSoftplus,
    /// **`MLP_MOE_ROUTER_HASH_V1`**: the scores of [`Scoring::SqrtSoftplus`], but the experts a token uses are a frozen table's row
    /// `tid2eid[token]` (`hash_moe` layers): the learned gate only weights them.
    SqrtSoftplusHash,
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
    /// **`MOE_EXPERTS_PLAIN_V1`** (Nemotron-H): `false` — the experts (and the shared expert) are plain MLPs, `down(act(up(x)))`, with no gate
    /// projection. Experts have no biases then.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gated: bool,
    /// **`MOE_LATENT_PROJ_V1`** (Nemotron-H's `moe_latent_size`): the routed experts run in a latent space of this width — `fc1_latent_proj`
    /// before them, `fc2_latent_proj` after — while the router and the shared expert read the layer's input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latent: Option<usize>,
    /// **`MLP_MOE_ZERO_EXPERT_V1`** (LongCat-Flash's `zero_expert_num`): this many extra router outputs after the real experts,
    /// each the IDENTITY — a token routed to one gets `w·x` from it. The router, its selection bias and its top-k run over
    /// `experts + zero_experts` outputs; the expert tensors keep `experts` rows.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub zero_experts: usize,
    /// A `[D]` bias added after the experts' weighted sum (JetMoE's `mlp.bias`, `moe.bias`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub out_bias: bool,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
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

/// **`FFN_SHORTCUT_MOE_V1`** (LongCat-Flash's shortcut-connected MoE): a logical layer is TWO layers of the spec; the first runs a
/// MoE on the same normed vector as its dense MLP but does not add the result — it is carried to the second, whose output adds it
/// (`h4 = h3 + mlps[1](n3) + moe(n1)`). The carried value rides at the residual scale (a carry of its own).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortcutSpec {
    pub mlp: MlpSpec,
    pub side: ShortcutSide,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShortcutSide {
    /// The MoE of this layer's FFN input; its output goes to the side carry.
    Produce(MoeSpec),
    /// Adds the side carry to this layer's FFN output.
    Consume,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Ffn {
    None,
    Mlp(MlpSpec),
    Moe(MoeSpec),
    RwkvChannel(RwkvChannelSpec),
    /// Gemma-4 (only under [`Residual::Sandwich`]).
    MlpMoe(Box<MlpMoeSpec>),
    /// LongCat-Flash: a dense MLP and a MoE carried to the next layer (only under [`Residual::Sequential`]).
    MlpShortcut(Box<ShortcutSpec>),
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
    /// **`RESIDUAL_MHC_SINKHORN_V1`** — manifold-constrained hyper-connections ([`MhcSpec`]). The residual is `streams` rows of the hidden
    /// width. Each of the layer's two sites (the mixer, then the FFN) reads
    ///
    /// ```text
    ///   [pre, post, comb] = fn · RMS(h.flatten())            pre = σ(·scale₀ + base) + ε,  post = 2σ(·scale₁ + base),
    ///                                                         comb = sinkhorn(softmax_rows(·scale₂ + base) + ε)
    ///   y = block(norm(Σ_i pre_i·h_i))
    ///   h'_k = post_k·y + Σ_j comb[j, k]·h_j
    /// ```
    ///
    /// `pre_mixer` norms the mixer site's collapsed input, `pre_ffn` the FFN site's.
    Mhc { pre_mixer: NormSpec, pre_ffn: NormSpec },
    /// **`RESIDUAL_ALTUP_V1`** (+ **`RESIDUAL_LAUREL_V1`**) — Gemma-3n's alternating updates over [`AltUpSpec::streams`] streams.
    /// Per layer, with `K` streams `h_i` and the router `m(x) = tanh(modality_router(router_norm(x)·D⁻¹))`:
    ///
    /// ```text
    ///   pred_i = h_i + Σ_j C[i,j]·h_j,   C = prediction_coefs(m(h_0))                      // K×K, per position
    ///   an     = pre_mixer(pred_0)
    ///   lo     = an + laurel_norm(laurel_right(laurel_left(an)))                            // LAuReL, when present
    ///   a      = (pred_0 + post_mixer(mixer(an)) + lo) / √2
    ///   f      = a + post_ffn(ffn(pre_ffn(a)))                                              // the activated stream
    ///   cor_i  = pred_i + (f − pred_0)·(correction_coefs(m(f))_i + 1)
    ///   y      = per_layer_gate(cor_0 ⊙ correct_output_scale)  →  act · ple  →  per_layer_out  →  post_norm
    ///   h'_0 = cor_0,   h'_i = cor_i + y            (i ≥ 1)
    /// ```
    ///
    /// The `K` streams ride in carry 0 (`(K + 1)·D` lanes: the extra slot holds the layer's intermediate between its two blocks).
    AltUp {
        pre_mixer: NormSpec,
        post_mixer: NormSpec,
        pre_ffn: NormSpec,
        post_ffn: NormSpec,
        /// The router's norm (`altup.router_norm`, gain `w`).
        router_norm: NormSpec,
        /// **`RESIDUAL_LAUREL_V1`**: the learned low-rank branch beside the mixer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        laurel: Option<LaurelSpec>,
        /// Gemma-3n's per-layer input, added to streams `1..K`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ple: Option<PleSpec>,
    },
}

/// The model-wide half of [`Residual::Mhc`]: the streams (the embedding is repeated into each), the Sinkhorn iterations of `comb`, the
/// epsilons, and the final collapse (`HyperHead`: `pre = σ(fn·RMS(h)·scale + base) + ε`, then one weighted sum) before the final norm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MhcSpec {
    pub streams: usize,
    pub iters: usize,
    /// `hc_eps`.
    pub eps: f64,
    /// The epsilon of the weightless RMS norm over the flattened streams (`rms_norm_eps`).
    pub norm_eps: f64,
}

/// **`RESIDUAL_LAUREL_V1`** — LAuReL's low-rank residual branch: `x + norm(right(left(x)))` with `left [rank, D]` and `right [D, rank]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaurelSpec {
    pub rank: usize,
    pub post_norm: NormSpec,
}

/// The model-wide half of [`Residual::AltUp`]: the number of streams (the embedding is the first; the others are projections of it
/// brought to its magnitude), and the floor under the mean square in the magnitude match `x·rms(h_0)/√max(ms(x), floor)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AltUpSpec {
    pub streams: usize,
    /// `epsilon_tensor` of `Gemma3nTextModel.forward` (1e-5).
    pub floor: f64,
    /// `altup_correct_scale`: the per-layer gate reads `cor_0 ⊙ correct_output_scale`.
    pub correct_scale: bool,
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
    /// **`LAYER_PRE_BRANCH_V1`**: an attention + MLP branch (Zamba2's shared transformer) computed from `[h | e0]` whose output is ADDED to the
    /// mixer's input: `h <- h + mixer(norm(h + branch))`. Only under [`Residual::Sequential`] with a mixer norm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_branch: Option<PreBranch>,
}

/// **`LAYER_PRE_BRANCH_V1`** (with `ATTN_SHARED_BLOCK_V1`, `EMBED_CARRY_V1`, `LINEAR_LOWRANK_ADAPTER_V1`): Zamba2's hybrid layer.
///
/// `u = in_norm(concat[h, e0])` (`e0` is [`ModelSpec::embed_carry`]'s carried embedding), `a = attn(u)`, `m = mid_norm(a)`, `f = mlp(m)`,
/// `t = out(f)`, and the layer's mixer reads `norm(h + t)`. The branch has NO residual of its own. Its weights (`in_norm`, the attention,
/// `mid_norm`, the MLP) are SHARED by every layer of the same `group`: one set of integer tensors in the artifact (global params named
/// `pb{group}.*`), each occurrence with its own activation scales, KV history and low-rank adapters. `out` and the adapters are per layer.
/// The attention's `in_dim` must be `2·hidden`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreBranch {
    pub in_norm: NormSpec,
    pub attn: AttnSpec,
    pub mid_norm: NormSpec,
    pub mlp: MlpSpec,
    /// Per-layer low-rank adapters (`LINEAR_LOWRANK_ADAPTER_V1`): `y = W x + B (A x)`, `A: [rank, in]`, `B: [out, rank]`, on the q, k, v
    /// projections of the attention (`attn`) and on the MLP's fused gate/up projection (`mlp`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lowrank: Option<LowRankSpec>,
    /// The sharing group: layers of one group read one set of branch weights.
    pub group: usize,
    /// The layer whose checkpoint module stores the group's weights (`{B}` in a tensor-name template), and this layer's ordinal among the
    /// layers with a pre-branch (`{O}`: its adapters' index). Data of the binding, not of the function; they make every pre-branch layer
    /// a block of its own (its adapters are its own tensors).
    pub weights_layer: usize,
    pub ordinal: usize,
}

impl PreBranch {
    /// The HL names of the branch's params (frontend-neutral; `crate::hl::build` and `crate::hf_weights` both read them here).
    pub fn attn_prefix(&self) -> String {
        format!("pb{}.attn", self.group)
    }
    pub fn mlp_name(&self) -> String {
        format!("pb{}.mlp", self.group)
    }
    pub fn in_norm_name(&self) -> String {
        format!("pb{}.in_norm", self.group)
    }
    pub fn mid_norm_name(&self) -> String {
        format!("pb{}.mid_norm", self.group)
    }
    /// The base of the HL names of this layer's adapters.
    pub fn adapter_base(&self) -> String {
        format!("pb{}.d{}", self.group, self.ordinal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowRankSpec {
    pub rank: usize,
    /// The attention's q, k, v projections carry an adapter too (`use_shared_attention_adapter`); the MLP's always does.
    #[serde(default)]
    pub attn: bool,
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
    /// `EMBED_PROJ_IN_AFTER_NORM_V1` (ALBERT's `embedding_hidden_mapping_in`): the projection comes LAST — positions, token
    /// types and the embedding norm all act at the token table's width, and the projection (with its bias, when
    /// `proj_in_bias`) lifts the normed row to the hidden width. OPT's projection, the default, comes first.
    #[serde(default, skip_serializing_if = "is_false")]
    pub proj_after_norm: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub proj_in_bias: bool,
    /// A token-type table of this many rows (BERT's `token_type_embeddings`); a single-segment
    /// encoder adds row 0 to every position.
    #[serde(default)]
    pub type_rows: Option<usize>,
    /// A bidirectional encoder's bias over bucketed relative positions, one table shared by every
    /// layer (MPNet: T5's bidirectional buckets). Read by `lower::bidir` only.
    #[serde(default)]
    pub rel_bias: Option<RelBiasSpec>,
    /// **`ATTN_DISENTANGLED_V1`** (DeBERTa-v2/v3): relative-position embeddings projected by the layer's own key and
    /// query projections (`share_att_key`) and added to the scores as content-to-position and position-to-content
    /// terms. Read by `lower::bidir` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disentangled: Option<DisentangledSpec>,
}

/// DeBERTa's disentangled attention (`position_buckets`, `max_relative_positions`, `pos_att_type`, `norm_rel_ebd`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisentangledSpec {
    /// `position_buckets`: the table has `2·span` rows and a relative position is log-bucketed into `[-span, span)`.
    pub span: usize,
    /// The largest relative distance the log buckets reach (`max_relative_positions`, else `max_position_embeddings`).
    pub max_position: usize,
    pub c2p: bool,
    pub p2c: bool,
    /// `norm_rel_ebd = layer_norm`: the table passes through this norm (a bias per `bias`) once.
    pub norm: Option<NormSpec>,
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
    /// **`HEAD_TRANSFORM_V1`**: a prediction head before the vocabulary projection — `dense → act → norm` (BERT's
    /// `cls.predictions.transform`, ModernBERT-decoder's `lm_head`, RoBERTa's `lm_head.dense`/`layer_norm`).
    #[serde(default)]
    pub transform: Option<HeadTransformSpec>,
}

/// `h → norm(act(dense(h)))`, the head's own transform; then the (tied) vocabulary projection and its bias.
/// Tensors: roles `head.transform.dense` (`.weight`, `.bias` when `bias`) and `head.transform.norm`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadTransformSpec {
    #[serde(default)]
    pub bias: bool,
    pub act: Act,
    pub norm: NormSpec,
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
    /// The residual's multiple streams, when the layers are [`Residual::Mhc`] (**`RESIDUAL_MHC_SINKHORN_V1`**).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mhc: Option<MhcSpec>,
    /// The residual's multiple streams, when the layers are [`Residual::AltUp`] (**`RESIDUAL_ALTUP_V1`**).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altup: Option<AltUpSpec>,
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
    /// **`ATTN_PREFIX_LM_V1`** (PaliGemma): the model attends bidirectionally over an image-and-prompt prefix when it is given
    /// `token_type_ids` (an image prompt). This spec lowers the text-only path — a prompt of token ids, causal, which is what HF
    /// computes without `token_type_ids` — so a lowering that binds image rows to it is REFUSED by name: causal attention over
    /// the image tokens would be another function (`fidelity::prepare_spec`; the prefix stage is FR-20's pipeline, not built).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prefix_lm: bool,
    /// **`EMBED_CARRY_V1`**: the embedding block's output rides through every layer as one more carry (`e0`, the hidden width), for
    /// the layers with a [`PreBranch`] to read (Zamba2 concatenates it to the hidden state).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embed_carry: bool,
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
    /// **`WEIGHTS_EXPR_V1` — weights as data.** HL parameter name → weight expression (a JSON array: the
    /// checkpoint tensor's complete name, then steps; [`crate::weights::expr`]). An entry replaces the
    /// parameter's default binding, so a checkpoint layout the enumerated ones above do not describe is
    /// written in the adapter, not in Rust. Evaluated and validated by [`crate::hf_weights::bind`]; absent from
    /// the serialised spec when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub weights: BTreeMap<String, serde_json::Value>,
}

fn one_shard() -> usize {
    1
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_one(x: &f64) -> bool {
    *x == 1.0
}

impl HfStorage {
    pub fn name(&self, role: &str) -> Option<&str> {
        self.names.get(role).map(String::as_str)
    }
}
