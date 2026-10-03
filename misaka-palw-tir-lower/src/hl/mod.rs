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
use crate::spec::{Act, Glu, HeadMap, NormSpec, QTemperature, RouterSpec};
use serde::Serialize;

pub use build::build_program;

fn is_true(b: &bool) -> bool {
    *b
}

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

fn is_zero_usize(n: &usize) -> bool {
    *n == 0
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
        /// The gate's activation (SiLU; Qwen4-Exp's `output_gate_type` may say sigmoid).
        act: Act,
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
    /// [`Op::Rope`] by the position at the START of the position's block of `ratio` positions
    /// (`pos − pos mod ratio`): the rotation of a pooled key. In: `[x, Pos]`.
    RopeAtBlock {
        heads: usize,
        head_dim: usize,
        rotary_dim: usize,
        offset: usize,
        style: RopeStyle,
        table: u32,
        ratio: usize,
    },
    /// Llama-4's attention temperature: `x · t(pos)`, `t` the spec's [`QTemperature`]. In: `[x, Pos]`.
    PosScale {
        temp: QTemperature,
    },
    /// `x · p[0]` for a learned per-layer scalar `p` (Gemma-4's `layer_scalar`), or — when `p` has as many elements as `x` — `x ⊙ p`
    /// (Gemma-3n's `correct_output_scale`). In: `[x, p [1] or [n]]`.
    ScaleParam,
    /// **xIELU** (`ACT_LEARNED_POINTWISE_V1`, Apertus): `x > 0 ? αp·x² + β·x : (expm1(min(x, ε)) − x)·αn + β·x` with
    /// `αp = softplus(p)` and `αn = β + softplus(n)`, the layer's own scalars. In: `[x, p [1], n [1], β [1], ε [1]]`.
    Xielu,
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
        /// Chunked attention: only the keys of the query's own `chunk`-position chunk.
        chunk: Option<usize>,
        /// Sparse block attention (`ATTN_SPARSE_BLOCK_V1`): the tokens of the blocks the extra last
        /// input names (`BlockSelect`'s ids, blocks of this many positions) plus the incomplete tail.
        blocks: Option<usize>,
    },
    /// Multi-head latent attention over a compressed history (DeepSeek). Keys/values are
    /// `kv_b · latent` per head plus the shared rotary key. In: `[q, State(latent), State(k_rope), kv_b]`.
    ///
    /// With an `indexer` (`ATTN_TOKEN_INDEXER_V1`, DeepSeek sparse attention) the softmax runs over the `topk` best
    /// tokens of the indexer's score only (ties to the lowest index of the history window). In: `[q, State(latent),
    /// State(k_rope), kv_b, iq [heads·dim], iw [heads], State(idx_keys)]`: the indexer's rotated query, its head
    /// weights and the history of its rotated keys (this position's row already appended).
    MlaAttention {
        heads: usize,
        nope: usize,
        rope: usize,
        v_dim: usize,
        kv_lora: usize,
        scale: f64,
        indexer: Option<TokenIndexDims>,
    },
    // ── recurrences (Fixed states) ──
    /// Depthwise causal convolution over the last `kernel` inputs; the state keeps `kernel−1`
    /// rows. In: `[x, State(window), w [C,K], (b)]`.
    CausalConv1d {
        channels: usize,
        kernel: usize,
        bias: bool,
        act: Option<Act>,
        /// Distance between taps (1: contiguous); the state keeps `(kernel − 1)·dilation` rows.
        dilation: usize,
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
        /// **`MIXER_KDA_V1`**: the decay `g` is channel-wise — `[v_heads·dk]`, one forget gate per key channel of each head
        /// (`S[i, :] ← S[i, :]·exp(g[vh, i])`), not one per head (`[v_heads]`).
        channel_decay: bool,
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
    // ── multi-stream residuals (RESIDUAL_GATED_HC_V1) ──
    /// Mean over `streams` equal groups: `out[d] = (1/S) Σ_s x[s·D + d]`. In: `[x]`.
    StreamMean {
        streams: usize,
    },
    /// A vector scaled per stream: `out[s·D + d] = o[d] · w[s]`. In: `[o [D], w [S]]`.
    StreamOuter {
        streams: usize,
    },
    /// Per-group dot product: `out[g] = Σ_{j in group g} a[j]·b[j]`. In: `[a, b]`.
    GroupDot {
        groups: usize,
    },
    /// **A data-dependent mix of streams** (`RESIDUAL_ALTUP_V1`): `out[i·D + d] = Σ_j C[i,j]·x[j·D + d]` for the coefficients `C` the second
    /// input holds — row-major `[n_out, n_in]`, or with `transpose` stored `[n_in, n_out]` (`C[i,j]` is then element `j·n_out + i`).
    /// In: `[x [n_in·D], C [n_out·n_in]]`.
    StreamMix {
        n_in: usize,
        n_out: usize,
        transpose: bool,
    },
    /// **Manifold-constrained hyper-connections' weights** (`RESIDUAL_MHC_SINKHORN_V1`) from the mix logits `m = fn·RMS(h)` of the `H = streams`
    /// streams: `pre = σ(m₀·s₀ + b₀) + ε`, `post = 2σ(m₁·s₁ + b₁)` and `comb = sinkhorn(softmax_rows(m₂·s₂ + b₂) + ε)` — the row-softmax of the
    /// `H × H` logits, `+ ε`, one division by the column sums (`+ ε`), then `iters − 1` times a division by the row sums and one by the column
    /// sums (every sum `+ ε`). In: `[m [(2 + H)·H], base [(2 + H)·H] (Param), scale [3] (Param)]`. Out: `pre [H]`, `post [H]`, `comb [H·H]`
    /// row-major (`comb[j, k]` at `j·H + k`).
    MhcMap {
        streams: usize,
        iters: usize,
        eps: f64,
    },
    /// The final collapse's weights (`HyperHead`): `σ(m·s + b) + ε`. In: `[m [H], base [H] (Param), scale [1] (Param)]`.
    MhcPre {
        eps: f64,
    },
    /// **A window buffer** (`ATTN_COMPRESSED_KV_V1`): row `pos mod ratio` of a `[ratio, w]` `Fixed` state becomes the input. In:
    /// `[row [w], State(buf), Pos]`; writes the state.
    WindowWrite {
        ratio: usize,
    },
    /// **The pooled entry of the window just closing** (`ATTN_COMPRESSED_KV_V1`): per channel, the softmax over the window's rows of
    /// `gate + ape` weights the `kv` rows and they are summed — `[dim]`. Without `overlap` the buffers hold `dim` lanes per row. With it they
    /// hold `2·dim`: the entry pools the previous window's first series (`prev_kv`, `prev_gate`, `[ratio, dim]` states, weight 0 for window 0)
    /// with this window's second, and — when this position closes a window — the states become this window's first series (after being read).
    /// Only the value at a position that closes a window is meaningful. In: `[State(kv), State(gate), ape [ratio, cin] (Param), Pos]`, with
    /// `overlap` `[…, State(prev_kv), State(prev_gate)]`.
    WindowPool {
        ratio: usize,
        dim: usize,
        overlap: bool,
    },
    /// **Attention over the window and the compressed entries** (`ATTN_COMPRESSED_KV_V1`): one head of keys (= values), the `window` last rows
    /// of the history and the entries `t < (pos + 1)/ratio` of the `[blocks, head_dim]` store — only those `ids` name, when a selection is given —
    /// in one softmax with the per-head sink. In: `[q [heads·head_dim], State(window history), State(entries), sinks (Param [heads]), Pos]`,
    /// then `ids [topk]` (an [`Op::EntrySelect`]'s) when selecting. The entry that closes at this position is already in the store (its
    /// [`Op::BlockWrite`] comes before).
    EntryAttention {
        heads: usize,
        head_dim: usize,
        ratio: usize,
        blocks: usize,
        scale: f64,
        select: bool,
    },
    /// **The lightning indexer's selection** (`ATTN_ENTRY_INDEXER_V1`): entry `t < (pos + 1)/ratio` scores `Σ_h w_h·ReLU(q_h·k_t)/√dim`
    /// (the entry that closes at this position is the candidate row, not yet in the store); the `top` best, ties to the lowest index, as
    /// a fixed-size id vector. In: `[q [heads·dim], w [heads], cand [dim], State(keys), Pos]`.
    EntrySelect {
        heads: usize,
        dim: usize,
        ratio: usize,
        blocks: usize,
        top: usize,
    },
    /// **A magnitude match** (`RESIDUAL_ALTUP_V1`): `x · rms(r) / √max(mean(x²), floor)` with `rms(r) = √mean(r²)`. In: `[x, r]`.
    RmsMatch {
        floor: f64,
    },
    /// **`FFN_ACTIVATION_SPARSITY_V1`**: `relu(x − (mean(x) + z·std(x)))` over the whole row, `std` the biased one — for the layers
    /// of `layers` that name a `z`; the others (`None`: a dense layer of a model that sparsifies some) pass the row through. The
    /// constants are DATA of the layer, so layers that differ only in them run one block (the block count of a program is capped).
    /// `(model layer, z)` of every layer that runs this block. In: `[x]`.
    GaussianTopK {
        layers: Vec<(usize, Option<f64>)>,
    },
    /// Each element repeated `size` times: `out[g·size + j] = a[g]` (a per-head gate over the head's width). In: `[a [groups]]`.
    GroupRepeat {
        groups: usize,
        size: usize,
    },
    // ── hashed n-gram per-layer embedding (EMBED_NGRAM_PLE_V1) ──
    /// The table row of every hash head for this position's token and the segment-masked tokens
    /// before it ([`crate::ngram`]): an `Idx` vector of `(ngram_size − 1)·heads_per_ngram` ids.
    /// `layers` lists `(model layer, its index among the PLE layers)` for every layer that runs
    /// this block (the hash constants depend on the index); `ple.layer_index` is not read.
    /// In: `[Token, State(window [ngram_size − 1])]`; the state holds the last tokens, `eos` at the start.
    NgramIds {
        ple: crate::spec::NgramPleSpec,
        layers: Vec<(usize, usize)>,
    },
    /// Rows of a table by an index vector: `out = concat_h table[ids[h]]` (`heads` rows of `dim`).
    /// In: `[ids (Idx), table [R, dim]]`. The lowering reads the table per hash head (each head owns a
    /// contiguous range of rows, cut into chunks under NF-8's `2^24` rows), so the ids must be a
    /// [`Op::NgramIds`] node's, which says how the table splits.
    GatherRows {
        heads: usize,
        dim: usize,
    },
    // ── sparse block attention (ATTN_SPARSE_BLOCK_V1) ──
    /// The running sum of the keys of the position's block, and the mean of what the block holds so
    /// far (`sum / ratio`: exact once the block is complete). The sum restarts at the block's first
    /// position. In: `[k [dim], State(sum [dim])]`; writes the state.
    BlockMean {
        ratio: usize,
    },
    /// The block-key matrix `[blocks, dim]`: row `pos / ratio` becomes the input when the position
    /// completes its block (`(pos + 1) mod ratio = 0`). In: `[row [dim], State(keys), Pos]`; writes the state.
    BlockWrite {
        ratio: usize,
        blocks: usize,
    },
    /// The blocks a query attends over. Block scores `Σ_h relu(q_h · key_b)` over the complete blocks
    /// (this position's own block counts when it completes now: its row is the input `cand`), the
    /// `top` best by score (ties → lower index; a block not complete scores lowest); out: the `top`
    /// block ids, a fixed-size vector (`Idx`). In: `[q [heads·dim], cand [dim], State(keys), Pos]`.
    BlockSelect {
        heads: usize,
        dim: usize,
        ratio: usize,
        blocks: usize,
        top: usize,
    },
    // ── routing ──
    /// Expert selection. Out 0: `top_k` expert ids in index order (ties → lowest index);
    /// out 1: their weights. In: `[logits, (selection bias)]`.
    ///
    /// `zero > 0` (`MLP_MOE_ZERO_EXPERT_V1`): `experts` counts the real experts PLUS `zero` identity ones (the router, its bias and
    /// the top-k run over all of them); out 0 holds ids below `experts − zero` only (a slot that chose an identity expert reads
    /// expert 0 with weight 0), out 1 the weights with those slots zeroed, out 2 (`[1]`) the sum of the weights of the identity slots.
    Route {
        router: RouterSpec,
        experts: usize,
        top_k: usize,
        #[serde(skip_serializing_if = "is_zero_usize")]
        zero: usize,
    },
    /// **`MIXER_MOA_V1`**: one projection per selected expert, `y_j = W[e_j] · x_j`. In: `[x, ids, W [E, R, C]]`; `x` is `[C]`, read
    /// by every slot (`per_slot: false`, the queries), or `[k·C]`, a row per slot (`per_slot`, the outputs). Out `[k·R]`, slot-major.
    /// `wide`: the result keeps `i32` precision (it is summed by [`Op::WeightedSum`] straight away).
    ExpertLinear {
        top_k: usize,
        per_slot: bool,
        wide: bool,
    },
    /// `Σ_j w_j · y_j (+ bias)` over `[k·D]` rows: the experts' outputs in one exact accumulator, narrowed once. In: `[y, weights, (bias)]`.
    WeightedSum {
        top_k: usize,
        /// A third input, a `[D]` param, is added to the sum.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        out_bias: bool,
    },
    /// A `[n0, n1, n2]` value with its first two axes swapped (`[n1, n0, n2]`): the reorder between slot-major and head-major heads.
    Transpose01 {
        n0: usize,
        n1: usize,
        n2: usize,
    },
    /// `Σ_j w_j · down_e(glu(gate_e x, up_e x))` over the selected experts — or, `input_scaled`
    /// (Llama-4), `Σ_j down_e(glu(gate_e x_j, up_e x_j))` with `x_j = w_j · x`.
    /// In: `[x, ids, weights, gate [E,I,D], up [E,I,D], down [E,D,I], (gate_b, up_b, down_b)]`. A plain expert (`gated: false`,
    /// `MOE_EXPERTS_PLAIN_V1`) is `down_e(act(up_e x))`: In `[x, ids, weights, up [E,I,D], down [E,D,I]]`, no biases.
    MoeExperts {
        top_k: usize,
        act: Act,
        glu: Glu,
        bias: bool,
        input_scaled: bool,
        #[serde(skip_serializing_if = "is_true")]
        gated: bool,
        /// `MLP_MOE_ZERO_EXPERT_V1`: one more input after the expert params, a `[1]` weight `z` (the Route's out 2): the output
        /// adds `z · x` (the identity experts' share).
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        identity: bool,
        /// One more input, last: a `[D]` param added after the experts' weighted sum (JetMoE's `mlp.bias`).
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        out_bias: bool,
    },
}

/// The shape of a token indexer ([`crate::spec::TokenIndexerSpec`]) as the HL op carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TokenIndexDims {
    pub heads: usize,
    pub dim: usize,
    pub topk: usize,
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
            Op::RopeAtBlock { .. } => "RopeAtBlock",
            Op::StreamMean { .. } => "StreamMean",
            Op::StreamOuter { .. } => "StreamOuter",
            Op::GroupDot { .. } => "GroupDot",
            Op::GroupRepeat { .. } => "GroupRepeat",
            Op::StreamMix { .. } => "StreamMix",
            Op::MhcMap { .. } => "MhcMap",
            Op::MhcPre { .. } => "MhcPre",
            Op::WindowWrite { .. } => "WindowWrite",
            Op::WindowPool { .. } => "WindowPool",
            Op::EntryAttention { .. } => "EntryAttention",
            Op::EntrySelect { .. } => "EntrySelect",
            Op::RmsMatch { .. } => "RmsMatch",
            Op::GaussianTopK { .. } => "GaussianTopK",
            Op::NgramIds { .. } => "NgramIds",
            Op::GatherRows { .. } => "GatherRows",
            Op::BlockMean { .. } => "BlockMean",
            Op::BlockWrite { .. } => "BlockWrite",
            Op::BlockSelect { .. } => "BlockSelect",
            Op::PosScale { .. } => "PosScale",
            Op::ScaleParam => "ScaleParam",
            Op::Xielu => "Xielu",
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
            Op::ExpertLinear { .. } => "ExpertLinear",
            Op::WeightedSum { .. } => "WeightedSum",
            Op::Transpose01 { .. } => "Transpose01",
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
    /// A carry past the residual that rides at the RESIDUAL's scale (`i32`), not as `i16` codes of one layer's site
    /// (`FFN_SHORTCUT_MOE_V1`'s side value, written by many layers): a value several layers write cannot have one site's scale.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub resid: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HlProgram {
    pub architecture: String,
    /// What `post` produces: logits, or an encoder's embedding.
    pub output: HlOutput,
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
    /// The model layer each occurrence of `schedule` belongs to, for reading its weights (`{L}`):
    /// the identity unless a layer runs as more than one block (a Gemma-4 layer is its mixer half
    /// and its FFN half, NF-12's 512 nodes being too few for one).
    pub layer_of: Vec<usize>,
}

/// What a program's `post` produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HlOutput {
    /// Logits, through the head.
    Logits,
    /// An encoder's embedding row, L2-normalised or not.
    Embedding { normalized: bool },
}

impl HlProgram {
    /// Block occurrences between `pre` and `post` (a layer split in two counts twice).
    pub fn num_layers(&self) -> usize {
        self.schedule.len()
    }
    /// The model layer of occurrence `l` (see [`HlProgram::layer_of`]).
    pub fn model_layer(&self, l: usize) -> usize {
        self.layer_of.get(l).copied().unwrap_or(l)
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
