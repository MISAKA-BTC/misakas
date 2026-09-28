# PALW-TIR v1 — the architecture corpus (RFC-0002 Phase A)

> **Design record, Gate 1.** Every corpus family of RFC-0002 §1.1 and the common Hugging Face decoder
> architectures, decomposed per position into PALW Canonical Tensor IR primitives in INTEGER semantics;
> the primitive list those decompositions force, argued against freeze criteria 1–3; the per-family
> fidelity thresholds for Phase E. The normative definitions are [spec 04b](../../../spec/palw/04b-tensor-ir.md);
> the reference evaluator is `misaka-palw-tir`.
>
> **Ground truth.** Semantic claims about a Hugging Face architecture cite the installed `transformers`
> 5.17.0 modeling code as `models/<arch>/modeling_<arch>.py:<Class.function>` (read-only, under
> `tir-venv/lib/python3.14/site-packages/transformers/`). RWKV-6 and RWKV-7 are not in that tree (it
> has RWKV-4 only); their rows cite the published formulations and are marked so.
>
> **Status of the claims.** Five corpus programs are built and run in `tests/programs.rs` (dense GQA,
> sliding + global, GDN with unequal heads, Mamba2, top-2 MoE with a shared expert); all five pass the
> range rules of spec 04b §7 (`tests/intervals.rs`: no exact primitive of theirs can overflow on any
> weights, token or position) and run to completion under full-range hostile weights
> (`tests/totality.rs`). Every integer kernel of the live court's catalogue is byte-identical to its
> `tir_library_v1` segment against the LIVE code (`misaka-palw-tir-conformance`; the BASE-0 KAT digest
> in `tests/kat_base0.rs`), and every template of the library is admissible and checked against a float
> reference (`tests/library.rs`). Everything else in this file is decomposition on paper, which is what
> Phase A asks for; Phase E turns each row into a lowering and a fidelity run.

## 0. Scope

**In scope:** decoder-only language models evaluated one position at a time — every family of C1–C8
and the architectures of §4.

**Out of scope for v1:** encoder-only models (BERT, RoBERTa, ModernBERT — bidirectional attention is
not a position scan), encoder-decoder models (T5, BART, Whisper — cross-attention over an encoder
output that is not a history), vision and audio towers (patch embeddings, 2-D/3-D RoPE, M-RoPE over
image grids — Qwen-VL, Gemma-3 vision, Llama-4 vision), speculative/assistant heads, and sampling
(stays in the logits scheme, RFC-0001). A multimodal model's *text decoder* is in scope when its
inputs are token embeddings.

**Method.** For each family: (1) the per-position high-level graph, from the HF modeling code;
(2) its decomposition into the §2 vocabulary, with the dtype at every edge, every lossy site and its
rule, every state (`Fixed`/`Hist`) and the required commit points; (3) what the family forces on the
primitive set.

## 1. The primitive set this corpus arrived at

Twenty-five primitives (spec 04b §6). Kinds: **S** Reshape, Transpose, Slice, Concat, Broadcast,
Iota, Gather · **E** Cast, Add, Sub, Mul, MatMul, ReduceSum, ReduceMax · **L** Div
(Floor | HalfUp | HalfAwayFromZero), Clamp, Log2Floor · **T** IntExp, IntRsqrt, IntLn (Q24) ·
**X** Compare, Select, TopK · **State** StateWrite, HistAppend. §7 is the family × primitive matrix,
§8 the minimality argument.

## 2. The integer vocabulary every family is written in

### 2.1 Types at the edges

| name | dtype | meaning |
| --- | --- | --- |
| code | `i16`, values in `±32767` | an A16 activation (ADR-0047); the carry, K/V rows, most commit points |
| w8 | `i8` param | a weight code; per-channel scale in the narrowing params |
| acc | `i64` | an exact accumulator (a `MatMul`, a `ReduceSum`) |
| wide | `i128` | a fixed-point product before its rounding (`x·m` with `m` an `i64` multiplier) |
| q24 | `i32` (or `i64` inside) | a Q24 value: norm unit rows, softmax probabilities, gates, decays |
| idx | `idx` | tokens, positions, selections |

### 2.2 Building blocks (library templates, spec 04b §11)

Written as functions; each expands into the listed primitives.

| template | expansion | lossy rule(s) |
| --- | --- | --- |
| `N[lo,hi](acc; m, s, z)` — the A16 narrowing | `Mul→i128`, `Div_HAFZ(·, 2^s)`, `Clamp→i64`, `Add z→i128`, `Clamp[lo,hi]` | HalfAwayFromZero, Saturate ×2 |
| `Lin(x; W, m, s, z)` | `MatMul(W:w8[out,in], x:code[in,1]) → acc`, then `N` | as `N` |
| `RMS(x; eps)` | `Mul`, `ReduceSum→acc`, `Mul 2^24→i128`, `Div_Floor(n)`, `Clamp→i64`, `Add eps`, `IntRsqrt`, `Mul`, `Clamp→i32` | Floor, IntRsqrt, Saturate |
| `RMSw(x; eps_m, eps_s)` (wide rows) | as `RMS` in `i128`, plus `Log2Floor` + even shift into `[2^24, 2^26)` and back | Floor, IntRsqrt, Saturate |
| `LN(x; eps)` — LayerNorm | `c = Sub(Mul(n, x), ReduceSum(x))` (exact centring, no division), then `RMSw(c; n²·eps)` | as `RMSw` |
| `Sig(x)` | `Select/Sub` for `−|x|`, `IntExp`, `Add ONE`, `IntRecip`, `Select`, `Mul`, `Div_Floor(2^24)` | IntExp, IntRsqrt, Floor |
| `SiLU(x)` | `Mul(x, Sig(x))`, `Div_Floor(2^24)`, `Clamp` | + Floor |
| `Tanh(x)` | `2·Sig(2x) − ONE` | as `Sig` |
| `GELUtanh(x)` | `x · Sig(2·√(2/π)·(x + 0.044715·x³))` with the constants as Q24 integers | as `Sig` + Floor |
| `Table(x; T)` | `Gather(T, Cast_idx(x + 32768))` — an activation on codes as a 65,536-entry table (PALW-EX-5: a transcendental at registration is data) | none (data) |
| `Softmax_up(x)` | `ReduceMax`, `Sub`, `Clamp`, `Mul 2^up`, `Clamp`, `IntExp`, `ReduceSum→acc`, `IntRecip`, `Mul`, `Div_Floor(2^24)` | IntExp, IntRsqrt, Floor |
| `RoPE(x; cos, sin)` | pairs by `Reshape/Slice` (adjacent) or by halves (`rotate_half`), `Mul`, `Sub/Add`, `Div_Floor(2^24)`, `Clamp`, `Concat` | Floor, Saturate |
| `Angles(pos)` | two-level tables: `Div_Floor(pos, 2^b)`, `Sub`, 4 `Gather`s, angle addition, `Div_Floor(2^24)` (spec 04b §11.3) | Floor |
| `Attn(q, K, V)` | `HistAppend` K and V, `Transpose`, `MatMul` over `d`, `N`, `Softmax_up`, `N` to Q15 codes, `MatMul` over `H`, `N` | as above |
| `L2(x)` | `ReduceSum` of squares, `Log2Floor`, shift into `[2^24, 2^26)`, `IntRsqrt`, `Mul`, `Div_Floor(2^(9+e))`, `Clamp` | Floor, IntRsqrt, Saturate |
| `Softplus`, `ExpRef`, `Decay` | the ADR-0052 compositions of `IntExp` and `IntLn` | IntExp, IntLn, Floor, Saturate |
| `Pow2(s)` | `Gather([2^0 … 2^62], Clamp_idx(s, 0, 62))` — a variable shift is a gather and a `Mul`/`Div` | Saturate (the clamp) |
| `Conv_w(x; state, taps)` | `Concat(State[w−1,C], x[1,C])`, `Slice` last `w−1` → `StateWrite`, `Mul` by taps, `ReduceSum` over the window | none (exact) + the narrowing after |
| `MapHeads_g / _t(x)` | grouping `Reshape[k,1,d]→Broadcast[k,r,d]→Reshape[v,d]`; tiling `Reshape[1,k,d]→Broadcast[r,k,d]→Reshape[v,d]` | none |

### 2.3 Lossy sites and their rules

Every lossy site in every decomposition below is one of:

| site | primitive and rule | where |
| --- | --- | --- |
| rounding a fixed-point product | `Div` HalfAwayFromZero by `2^s` | every A16 narrowing, state decays, residual gains |
| the internal `>> k` | `Div` Floor | norm means, RoPE, softmax normalisation, gates |
| gemmlowp SRDHM | `Div` HalfUp by `2^31` | BASE-0 `Requantize` only |
| saturation | `Clamp` | every narrowing to codes/i32/i64, state range (`StateWrite`) |
| transcendental | `IntExp`, `IntRsqrt`, `IntLn` (Q24) | softmax, sigmoid/SiLU/tanh/GELU, norms, decays, softplus |
| exponent extraction | `Log2Floor` | wide norms, L2 norms, precision-preserving rescales |
| selection | `TopK` (committed), `Compare`/`Select` | routing, masks, ReLU, branches of total definitions |

The legacy kernels use exactly these rules (spec 04b §11.1): `RoundingShiftRight` is `Div_HAFZ` by
`2^s`, `SRDHM` is `Clamp_i32(Div_HalfUp(a·b, 2^31))`, the A16 `a16_scale_round` is the `N` template.

### 2.4 Commit-point patterns

| pattern | commit points |
| --- | --- |
| every block | carry-out (the residual stream, `code[D]`), logits (post) |
| attention | the K and V rows appended (`HistAppend` inputs) |
| routing | every `TopK` |
| recurrence (`Fixed` state) | the per-position inputs of the update cone (e.g. the conv row, the gates), so the court can replay from a checkpoint (spec 04b §10.1) |
| cone size | lowerers add commit points (e.g. after each projection) until every cone fits `derive_court_cost_v1` |

## 3. The corpus families

### C1 — Dense transformer (Llama 3.x, Mistral 7B, Qwen2/2.5, Qwen3 dense, Gemma 1/2/3)

**HF.** `models/llama/modeling_llama.py`: `LlamaRMSNorm.forward` (`x·rsqrt(mean(x²)+eps)·w`),
`LlamaMLP.forward` (`down(act(gate(x))·up(x))`), `rotate_half` (half-split pairs `(i, i + d/2)`),
`repeat_kv` (GQA by `repeat_interleave`). Qwen2: q/k/v biases
(`models/qwen2/modeling_qwen2.py:Qwen2Attention.__init__`, `bias=True`). Qwen3: per-head QK-norm over
`head_dim` (`models/qwen3/modeling_qwen3.py:Qwen3Attention.__init__`, `q_norm`, `k_norm`). Gemma:
`(1 + w)` gain (`models/gemma/modeling_gemma.py:GemmaRMSNorm.forward`), embedding scale `√D`
(`GemmaScaledWordEmbedding`). Gemma 2: sandwich norms and soft-capping
(`models/gemma2/modeling_gemma2.py:Gemma2DecoderLayer`, `eager_attention_forward` softcap,
`Gemma2ForCausalLM.forward` `final_logit_softcapping`), `query_pre_attn_scalar`. Gemma 3: QK-norm
and one RoPE per layer type (`models/gemma3/modeling_gemma3.py:Gemma3RotaryEmbedding`, `q_norm`,
`k_norm`).

**Per position** (`x: code[D]` carried):

```
pre:    e = Gather(tok_embd: w8[V,D], token: idx) : w8[D]          -- Gemma: × √D folded into the lift
        x0 = N[±32767](e; lift_m[D], 0, 0) : code[D]                -- commit (carry-out)
layer:  u  = RMS(x; eps) : q24[D];   h = N(u; γ as m[D], 24, 0) : code[D]      -- γ (Gemma: 1+w) in m
        q  = Lin(h; Wq[Hq·d, D]) : code[Hq·d]   (+ bias as z — Qwen2)
        k  = Lin(h; Wk[Hkv·d, D]) : code[Hkv·d];  v = Lin(h; Wv) : code[Hkv·d]
        (Qwen3/Gemma3: q, k ← RMS per head, reshape [heads, d], then N)
        (c, s) = Angles(pos) : q24[d/2]
        q, k ← RoPE(·; c, s) : code                                 -- rotate_half = a permutation of lanes
        K = HistAppend(k) : code[H, Hkv, d]  (k committed);  V = HistAppend(v) : code[H, Hkv, d]
        o = Attn(q, K, V) : code[Hq·d]                              -- scale 1/√d (Gemma: query_pre_attn_scalar) in N
        (Gemma2: scores ← cap·Tanh(scores/cap) before the softmax)
        a  = Lin(o; Wo) : code[D];   x1 = Clamp(x + a) : code[D]   (Gemma2/3: RMS·(1+w) on a first)
        u2 = RMS(x1); h2 = N(u2; γ2) : code[D]
        g  = Lin(h2; Wg) → q24[F] (i32 rail);  up = Lin(h2; Wu) : code[F]
        m  = N(SiLU(g) · up; mul_m) : code[F]         (Gemma: GELUtanh instead of SiLU)
        d_ = Lin(m; Wd) : code[D];  x2 = Clamp(x1 + d_) : code[D]   -- commit (carry-out)
post:   uF = RMS(x); hF = N(uF; γF);  logits = Lin(hF; W_lm or tok_embdᵀ) : i32[V]   -- commit
        (Gemma2: logits ← cap·Tanh(logits/cap); Cohere: × logit_scale in N)
```

**Lossy sites:** every `N` (HalfAwayFromZero + Saturate), the norm (`Div_Floor` by `n`, `IntRsqrt`,
Saturate), RoPE (Floor + Saturate), the softmax (`IntExp`, `IntRecip`=`IntRsqrt`+Floor, Floor),
SiLU (`IntExp`, `IntRsqrt`, Floor), residual saturation. **States:** `Hist` K and V per layer,
window `history_bound` (or the sliding window, C2). **Commit points:** carry-outs, K/V rows,
logits, plus projection outputs as the court ceiling needs (a 7B layer's `Wd` cone is `D·F` MACs per
output row — tiled per `MatMul` row, spec 04b §10.3).

**Forces:** `MatMul` with batch dims (GQA as `[Hkv, G, d] × [Hkv, d, H]`), `HistAppend`, `Transpose`
with `H`, the softmax's `ReduceMax`/`IntExp`/`ReduceSum`, `IntRsqrt`, `Div` (Floor, HAFZ), `Clamp`,
`Gather` (embedding, RoPE tables). Tied embeddings are the same `Param` in pre and post. Gemma's
`1 + w` and every constant multiplier are registration-time data.

**Long context (forces §6.6).** Legacy pins one table row per position (`qwen36_row`'s `RopePartial`
arm reads `true_position · pairs · 8` bytes), i.e. `n_ctx × rope_dims/2 × 8` bytes — 1 GiB at
`2^21 × 64` pairs. The `Angles` template needs two tables of `2^b` rows (§6.6); "llama3", YaRN,
NTK, linear and LongRoPE scalings only change the frequencies and an attention factor
(`modeling_rope_utils.py:_compute_llama3_parameters`, `_compute_yarn_parameters`) — data.

### C2 — Attention variants (GQA, MQA, sliding + global)

**HF.** `repeat_kv` groups: q head `h` reads kv head `h // (Hq/Hkv)` (`modeling_llama.py:repeat_kv`,
"the equivalent of `torch.repeat_interleave(x, dim=1, repeats=n_rep)`"). Sliding window keys are
`kv_idx > q_idx − W` (`masking_utils.py:sliding_window_overlay`): `W` keys including the query's own.
Gemma 2/3 alternate local/global with two RoPE bases (`Gemma3RotaryEmbedding` per `layer_types`).

**Decomposition.** GQA/MQA are shapes: `q: code[Hkv, G, d]` against `K: [Hkv, d, H]` (MQA: `Hkv = 1`).
Sliding window: a `Hist` state with `window = W` gives `H = min(pos + 1, W)` — exactly the HF mask.
Alternation: two layer blocks (`local` with window `W` and its tables, `global` with
`history_bound`), and `schedule.layers` lists which runs where (Gemma 3: 5 local : 1 global). Falcon
MQA and PaLM-style parallel blocks are C1 graphs with a different DAG (§4).

**Forces:** the per-block window (spec 04b §2.2 — one `H` per block), the schedule. Nothing else.

**Tested:** `tests/programs.rs::a_sliding_window_and_global_schedule_evicts_exactly_past_the_window`
— local layers keep `window − 1` prior rows, equal the global program until the window fills, and
differ after.

### C3 — Mixture of experts (Mixtral 8×7B, Qwen2-MoE, Qwen3-MoE, DeepSeek-V2/V3)

**HF.** Mixtral: softmax over experts, top-k, renormalise
(`models/mixtral/modeling_mixtral.py:MixtralTopKRouter.forward`: `softmax → topk → /= sum`).
Qwen2-MoE: `norm_topk_prob` optional, shared expert gated by `sigmoid(Linear(x, 1))`
(`models/qwen2_moe/modeling_qwen2_moe.py:Qwen2MoeSparseMoeBlock.forward`). Qwen3-MoE: softmax,
top-k, `norm_topk_prob` (`models/qwen3_moe/modeling_qwen3_moe.py:Qwen3MoeTopKRouter`). DeepSeek-V2:
softmax scores, `group_limited_greedy` = groups ranked by their MAX score
(`models/deepseek_v2/modeling_deepseek_v2.py`, router `topk_method`). DeepSeek-V3: `sigmoid` scores,
`e_score_correction_bias` added FOR SELECTION ONLY, group score = sum of the top-2 biased scores in the
group, `topk_group` groups kept, top-k among them, weights = the UNBIASED scores gathered, normalised,
× `routed_scaling_factor`; one shared expert always on
(`models/deepseek_v3/modeling_deepseek_v3.py:DeepseekV3TopkRouter.forward`,
`DeepseekV3MoE.forward`).

**Per position** (inside a layer, after the attention):

```
r  = Lin(h; Wr[E, D]) : code[E]                                  (gpt-oss: + bias as z)
p  = Softmax_up(r) : q24[E]            | DeepSeek-V3: p = Sig(r)          | gpt-oss: TopK on r, softmax over k
sel = p (+ bias_for_choice : Param q24[E], DeepSeek-V3)
[grouped] gs = ReduceSum(Gather(sel[G, E/G], TopK(sel[G,E/G], axis 1, 2), batch_dims 1)) : [G]   (V3; V2: ReduceMax)
          gi = TopK(gs, topk_group) : idx          -- commit
          mask = ReduceMax(Compare(Iota[G,1], gi[1,kg], Eq), axis 1) : i8[G]
          sel = Select(Broadcast(mask)[E], sel, MIN)
idx = TopK(sel, k) : idx[k]                         -- commit (PALW-TIR-11), index order
w   = Gather(p, idx) : q24[k];  (normalise: w·IntRecip(Σw) >> 24; × scaling factor as a Q24 Mul)
Wg_e = Gather(W_gate_exps: w8[E, F, D], idx) : w8[k, F, D]        -- routed_expert_matmul structurally
g  = MatMul(Wg_e, h[D,1]) : acc[k, F, 1] → N → q24;  up likewise;  m = N(SiLU(g)·up)
y  = MatMul(Gather(W_down_exps, idx) : w8[k, D, F], m[k, F, 1]) : acc[k, D, 1] → N → i32 rail [k, D]
out = N(MatMul(w[1,k], y[k,D]) : acc[1,D])          -- ONE accumulator (ADR-0052 C)
(+ shared expert FFN; Qwen2-MoE: × Sig(Lin(h; w_sg[1,D])))
```

**Lossy sites:** the router's softmax/sigmoid, the renormalisation (`IntRecip` + Floor, or exact
`Div` Floor with a data divisor — the fenced Kimi router), expert narrowings, the single combine
narrowing. **Selections:** `TopK` (committed); `Compare`+`ReduceMax` build the group mask (a
`Scatter` is not needed); `Select` with the dtype minimum is `masked_fill(-inf)`. **States:** none
beyond attention.

**Forces:** `TopK` in index order with lowest-index ties; `Gather` over a 3-D param by a committed
index (the canonical-work rule "a `MatMul` whose `Param` passes through a `Gather` indexed by a
`TopK`"); `Gather` with `batch_dims` (per-group top-2); `MatMul` with a batch of experts; the
combine as one `MatMul`.

**Tested:** `tests/programs.rs::a_top2_moe_layer_with_a_shared_expert_routes_through_committed_selections`
and the router/combine conformance in `misaka-palw-tir-conformance` (ties among underflowing probabilities
decided by index alone, reproduced).

### C4 — Linear attention: the gated delta rule (Qwen3-Next, Qwen3.5/3.6/3.8)

**HF.** `models/qwen3_next/modeling_qwen3_next.py:Qwen3NextGatedDeltaNet` and
`models/qwen3_5/modeling_qwen3_5.py:Qwen3_5GatedDeltaNet`:

- projections: Qwen3-Next's `in_proj_qkvz` is laid out per KEY head
  (`fix_query_key_value_ordering`: `[k_heads, 2·dk + 2·r·dv]`, split into q, k, v, z, so the `r`
  value heads of key head `j` are contiguous); Qwen3.5 has flat `in_proj_qkv`, `in_proj_z`,
  `in_proj_b`, `in_proj_a`;
- a depthwise causal conv (kernel 4) over `[q | k | v]` of width
  `conv_dim = 2·k_heads·dk + v_heads·dv` (`self.conv_dim = self.key_dim * 2 + self.value_dim`), then
  SiLU;
- `β = sigmoid(b)`, `g = −exp(A_log)·softplus(a + dt_bias)`, `decay = exp(g)`, per VALUE head;
- **the head mapping:** `query.repeat_interleave(v_heads // k_heads, dim=2)` and the same for `key`
  — GROUPING: value head `vh` reads key head `vh // r`;
- `l2norm` of q and k (`x·rsqrt(Σx² + 1e−6)`), q scaled by `1/√dk`
  (`torch_recurrent_gated_delta_rule`);
- the state `S[v_heads, dk, dv]`: `S ← S·decay; kv_mem = Σ_k S·k; δ = (v − kv_mem)·β;
  S ← S + k ⊗ δ; o = Σ_k S·q`;
- output: `Qwen3NextRMSNormGated` — RMSNorm per value head, `× weight`, then `× silu(z)` (norm
  before gate).

**Head ratios.** 16:16, 16:32, 16:48, 24:72 and 32:128 are `r = 1, 2, 3, 3, 4`: shape data in
`MapHeads_g` — no kernel.

**Per position** (`x: code[D]`):

```
qkv = Lin(h; W_qkv) : code[conv_dim]                                -- commit (the conv row)
win = Concat(State(conv): code[3, conv_dim], qkv[1, conv_dim]);  StateWrite(conv, Slice(win, 1..4))
c   = N[i32](ReduceSum(win ⊙ taps: w8[4, conv_dim]ᵀ)) : q24;  a = N(SiLU(c)) : code[conv_dim]
q, k, v = Slice(a) : code[k_heads·dk], code[k_heads·dk], code[v_heads·dv]
q, k ← L2(reshape [k_heads, dk]) : code (Q15)
qv, kv = MapHeads_g(q), MapHeads_g(k) : code[v_heads, dk]
β  = Sig(Lin(h; W_b)) : q24[v_heads];   decay = Decay(Lin(h; W_a) + dt_bias; exp(A_log)) : q24[v_heads]
o  = GDN(S; kv, v, qv, decay, β) : i32[v_heads, dv]                  -- StateWrite(S) inside (§2.2)
o  = N(RMSw(o) per head; w) ⊙ SiLU(Lin(h; W_z)) → N : code[v_heads·dv]
y  = Lin(o; W_out) : code[D];   x' = Clamp(x + y)
```

**Lossy sites:** as C1, plus the state decay (`Div_HAFZ` by `2^24` + Saturate), the read/delta/out
narrowings, the rank-one write (`Mul 2^ws` + Saturate or `Div_HAFZ` by `2^−ws`), the `StateWrite`
range, `Log2Floor` in `L2` and `RMSw`, `IntLn` in the decay. **States:** `Fixed` conv
`code[3, conv_dim]`, `Fixed` `S: i32[v_heads, dv, dk]` range `±(2^31 − 1)`. **Commit points:** the
conv row, the gates `β` and `decay` (per-position inputs of the update cone), the carry-out; `S` at
checkpoints only (a per-position commitment of `S` is `v_heads·dv·dk` lanes per layer — 524,288 for
32×128×128 — against a step-leaf cap of 2^22 for the whole position; spec 04b §14.4).

**Forces:** `Fixed` state with a saturating write; `MatMul` batched over heads for `S·k` and `S·q`;
`Log2Floor`; `IntLn`; `i128` for the fixed-point products; the head mapping as data.

**Tested:** `tests/programs.rs::a_gdn_layer_with_unequal_key_and_value_heads_runs_and_the_head_mapping_is_data`
(2 key / 4 value heads; grouping and tiling are different programs and, at `k ≠ v`, different
functions; at `k = v` they coincide) and `misaka-palw-tir-conformance::q36::kdesc_q36_gdn_step`
(byte-identical to the live kernel over 36 positions, three seeds, with adversarial narrowings).

### C5 — State-space models (Mamba, Mamba2)

**HF.** Mamba: `models/mamba/modeling_mamba.py:MambaMixer.slow_forward`: in-proj to `(x, z)`,
causal conv + SiLU on `x`, `x_proj → (dt_low, B, C)`, `dt = softplus(dt_proj(dt_low))`,
`A = −exp(A_log)` per `(channel, state)`, `discrete_A = exp(A·dt)` (per channel AND state),
`h ← discrete_A·h + dt·B·x`, `y = h·C + D·x`, `y·act(z)`. Mamba2:
`models/mamba2/modeling_mamba2.py:Mamba2Mixer.forward` + `mamba2_selective_state_update`: in-proj to
`[z | xBC | dt]`, conv over `xBC`, `dt = softplus(dt + dt_bias)` clamped to `time_step_limit`, one
`A` per HEAD (`dA = exp(dt·A)` per head), B and C per GROUP and broadcast to heads
(`B.expand(…, num_heads // num_groups, …)` — grouping), `h ← dA·h + dt·(x ⊗ B)`, `y = h·C + D·x`,
then `MambaRMSNormGated`: **`x · silu(gate)` BEFORE the norm**.

**Per position (Mamba2):**

```
z  = Lin(h; W_z) → q24;   xbc = Lin(h; W_xbc) : code[C]     -- commit;  dtr = Lin(h; W_dt) → q24[nh]
win/conv as C4 (Fixed conv state) → SiLU → code;  x, B, C = Slice
dt = Clamp(Softplus(dtr + dt_bias)) : q24[nh];   dA = IntExp(Clamp(Div_Floor(dt·A, 2^24))) : q24[nh]
Bh, Ch = MapHeads_g(B), MapHeads_g(C) : code[nh, 1, N]
h' = Div_HAFZ(State(h)·dA, 2^24) + Div_HAFZ(x ⊗ Bh · dt, 2^s) ;  StateWrite(h, h')  -- Fixed i32[nh, P, N]
y  = MatMul(h', Chᵀ) + Div_Floor(x·D, 2^s) : acc → i32
y  = RMSw(Div_Floor(y · SiLU(z), 2^24)) → N : code;   out = Lin(y; W_out)
```

Mamba-1 differs only in shapes: `dt` and `A` per channel, `dA = IntExp` over `[d_inner, N]`
(`d_inner·N` transcendental evaluations per layer per position — 5,120 × 16 = 81,920 for a 2.8B
model), and `B`, `C` shared by all channels.

**Lossy sites:** `Softplus` (`IntExp`, `IntLn`), `IntExp` for `dA`, the decay and input products
(`Div_HAFZ` + Saturate in `StateWrite`), narrowings, the gated norm. **States:** `Fixed` conv, `Fixed`
`h`. **Commit points:** `xBC` (the conv row), `dt`, `z`, carry-out; `h` at checkpoints.

**Forces:** nothing new beyond C4 — the selective scan is elementwise per position; the chunked SSD
form is a fused kernel (RFC §4.2), not a primitive.

**Tested:** `tests/programs.rs::a_mamba2_layer_runs_its_selective_scan_over_positions`.

### C6 — Recurrent (RWKV-6 "Finch", RWKV-7 "Goose")

**Source.** Not in the installed `transformers` (it has RWKV-4: `models/rwkv/modeling_rwkv.py`,
`rwkv_linear_attention_cpu`). RWKV-6/7 rows follow the published formulations (Peng et al., *Eagle and
Finch*, 2024; *RWKV-7 "Goose"*, 2025); Phase E must pin a reference implementation (RWKV-LM or
`fla`) as the float ground truth.

**RWKV-6 per position:** token shift with data-dependent interpolation
`xx = x + (x_prev − x)·(μ + LoRA(x))` (LoRA = `tanh(x·A)·B`); `r, k, v, g` projections; decay
`w = exp(−exp(w0 + tanh(x·W1)·W2))` per channel (data-dependent); WKV per head
`[hs, hs]`: `y = r·(S + (u ⊙ k)ᵀv)`, `S ← diag(w)·S + kᵀv`; GroupNorm per head; `× SiLU(g)`;
channel mix `r·W_v(ReLU(W_k·xx)²)`.

**RWKV-7 per position:** token shift; `w = exp(−e^{−0.5}·σ(·))`-style decay per channel; in-context
learning rate `a = σ(·)`; `k̂ = L2(k)`; the generalised delta rule
`S ← S·(diag(w) − k̂ᵀ(a ⊙ k̂)) + vᵀk`; **value residual** `v ← v + (v_first − v)·σ(·)` where
`v_first` is layer 0's value; GroupNorm; bonus `(r ⊙ k ⊙ r_k)·v`.

**Decomposition.** Token shift: a `Fixed` state holding `x_prev` (`StateWrite(x)` each position).
`tanh` = `Tanh` (C1 table). `exp(+y)` for a positive `y` (RWKV-6's inner `exp`): `IntExp` is defined
for `x ≤ 0`, so `exp(y) = 2^n·exp(r)` with `n = Div_Floor(y, LN2_Q)` and `r = y − n·LN2_Q`, i.e.
`Mul(IntExp(r − LN2_Q)·2, Pow2(n))` — IntExp, Div, Gather, Mul; then `w = IntExp(−exp(y))`. WKV:
`Fixed S: i32[heads, hs, hs]` with `MatMul`s batched over heads, the decay as an elementwise `Mul` by
`w[·,1]`/`w[1,·]` and `Div_HAFZ`. GroupNorm = `LN` per head. **`v_first`** is a second carry:
`carry = [x: code[D], v_first: code[D]]`; layer 0 is its own block kind that writes `v_first`, the
others pass it through (the carry list of spec 04b §3.3). Squared ReLU = `Clamp(x, 0, max)` then `Mul`.

**Lossy sites:** decays (`IntExp`), `Tanh`/`Sig`, `L2`, the WKV narrowings and saturating state.
**States:** `Fixed` token-shift states (time-mix and channel-mix), `Fixed` WKV `S`.
**Forces:** multiple carries (already in the program structure); nothing primitive. RWKV-4 (in-tree)
additionally needs an elementwise maximum of two tensors (`torch.maximum(max_state, k + u)`) —
`Select(Compare(a ≥ b), a, b)` — and a division by a data denominator — `Div` Floor with a divisor
clamped `≥ 1`, or `IntRecip`.

### C7 — Hybrid (Qwen3.6-style attention + GDN + MoE, Jamba, Kimi Linear)

**HF.** Jamba: `models/jamba/configuration_jamba.py:JambaConfig.layers_block_type`
(`"attention" if i % attn_layer_period == attn_layer_offset else "mamba"`, period 8 offset 4) and
MoE every `expert_layer_period` (2) from `expert_layer_offset` (1); the Mamba mixer RMS-normalises
`dt`, `B`, `C` (`JambaMambaMixer`: `dt_layernorm`, `b_layernorm`, `c_layernorm`); the MoE router is
softmax → top-k with no renormalisation (`JambaSparseMoeBlock.route_tokens_to_experts`). Kimi
Linear: KDA layers and MLA layers (`models/kimi_linear/modeling_kimi_linear.py:KimiLinearDecoderLayer`,
`layer_types`). Qwen3-Next/3.5: `layer_types` alternating `linear_attention` and `full_attention`,
gated attention output (`sigmoid(gate)` from a double-width q projection).

**Decomposition.** A hybrid is a schedule: Jamba has four block kinds (attention+MLP,
attention+MoE, Mamba+MLP, Mamba+MoE) and `schedule.layers` lists them — the legacy two-variant
`full_attention_interval` (V1 `(i+1)%n`, V2 `i%n`) becomes data. Mixed state kinds coexist because
states are per block. Gated attention: `o ⊙ Sig(g)` with `g` the second half of the q projection
(`Slice`), narrowed.

**Kimi Linear KDA** (`recurrent_kimi_delta_attention`, `KimiLinearForgetGate.forward`): the gated delta
rule with a PER-CHANNEL decay `g[heads, dk] = −exp(A_log[h])·softplus(f_b(f_a(x)) + dt_bias)` — the
decay multiplies `S[h, dk, dv]` along `dk` (`Mul` by `decay[h, dk, 1]`); separate q/k/v projections
and conv; `l2norm` with `x / sqrt(Σx² + eps)`; output `KimiLinearRMSNormGated` with a SIGMOID gate. No
new primitive. **MLA** (C8/§4 DeepSeek).

**Forces:** the schedule and per-block states; nothing primitive.

### C8 — Routing variations

| variation | HF | decomposition |
| --- | --- | --- |
| top-1 | (k = 1 of any router) | `TopK(k = 1)` = argmax with lowest-index ties |
| top-2 | Mixtral | C3 |
| top-8 | Qwen3-MoE, DeepSeek | C3 |
| shared + routed | Qwen2-MoE (sigmoid-gated), DeepSeek (ungated), Kimi | a dense FFN added before the combine's narrowing or after |
| grouped top-k | DeepSeek-V2 (max per group), V3/Kimi (sum of top-2 per group) | `TopK` over groups (committed) → `Compare`/`ReduceMax` mask → `Select` → `TopK` |
| selection-only bias | DeepSeek-V3 `e_score_correction_bias` | `Add` before the `TopK`s only; the weights `Gather` the unbiased scores |
| expert biases | gpt-oss (`gate_up_proj_bias`, `down_proj_bias`) | `Gather(bias[E, ·], idx)`, `Add` |
| top-k before softmax | gpt-oss (`GptOssTopKRouter.forward`: `topk` on logits, softmax over k) | `TopK` on logits, `Gather`, `Softmax_up` over `k` |
| renormalised / not | Mixtral, Qwen3-MoE (`norm_topk_prob`) / Jamba, Qwen2-MoE default | the `IntRecip` renormalisation is present or absent |
| scaling factor | DeepSeek `routed_scaling_factor` | a Q24 `Mul` + `Div_Floor` |

**Ties.** Q24 probabilities underflow to exact zeros on confident tokens; `TopK`'s lowest-index rule
and index-order output make the committed selection a function of the values (ADR-0052 B). Float
`torch.topk` breaks ties in an implementation-defined order — the IR's rule is a (documented) choice,
and a fidelity matter only.

## 4. The common Hugging Face decoder architectures

| architecture | HF source | what differs from C1 | decomposition |
| --- | --- | --- | --- |
| Llama / Mistral / Qwen2 / Qwen3 dense | `modeling_llama.py`, `modeling_mistral.py`, `modeling_qwen2.py`, `modeling_qwen3.py` | biases (Qwen2), QK-norm per head (Qwen3), sliding window (Mistral v0.1) | C1/C2 exactly |
| Gemma 1/2/3 | `modeling_gemma.py:GemmaRMSNorm` (`1 + w`), `modeling_gemma2.py` (sandwich norms, `attn_logit_softcapping`, `final_logit_softcapping`), `modeling_gemma3.py` (per-type RoPE, QK-norm) | `1 + w`; `√D` embedding scale; soft-cap `cap·tanh(x/cap)`; sliding/global with two bases; GeGLU | `1 + w` and `√D` are data; `Tanh`; two layer blocks; `GELUtanh` or `Table` |
| Phi-2 | `modeling_phi.py:PhiDecoderLayer.forward` (`attn + mlp + residual` from one LayerNorm), partial rotary | parallel block, LayerNorm + bias, `gelu_new`, dense + bias, rotary on `partial_rotary_factor·d` lanes | `LN` (+ bias as `z`); the two branches in one DAG; `RoPE` on a `Slice`, the rest passed through; `GELUtanh` |
| Phi-3 | `modeling_phi3.py:Phi3MLP` (`gate_up_proj` fused), `Phi3Attention` (`qkv_proj` fused), LongRoPE (`modeling_rope_utils.py:_compute_longrope_parameters`, `longrope_frequency_update`) | fused projections; LongRoPE switches short/long factors when **the call's** `max(position_ids) + 1 > original_max_position_embeddings` | `Slice`; LongRoPE is two table sets; the HF switch depends on the forward call's length, not the position — the lowering chooses per position (`Select(pos ≥ original_max, long, short)`) and records it as a fidelity deviation |
| GPT-2 | `modeling_gpt2.py` (`Conv1D`, `wpe`), `activations.py:NewGELUActivation` | learned absolute positions; LayerNorm + bias; GELU-tanh; `Conv1D` = transposed linear | `Gather(wpe, pos)` + `Add` in pre; `LN`; `GELUtanh` or `Table`; the transpose at conversion |
| GPT-NeoX / Pythia | `modeling_gpt_neox.py` (`use_parallel_residual`: `x + attn(ln1 x) + mlp(ln2 x)`), `partial_rotary_factor` | parallel residual; partial rotary (0.25); exact-erf GELU | DAG; `Slice` + `RoPE`; erf-GELU as a `Table` on codes (no `erf` primitive) |
| Falcon | `modeling_falcon.py:FalconDecoderLayer.forward` (`parallel_attn`, `new_decoder_architecture` with `ln_attn`/`ln_mlp`, `num_kv_heads`), optional ALiBi | MQA/GQA, parallel attention, fused qkv laid out `[kv, G+2, d]` | shapes; `Slice`/`Reshape` of the fused qkv; ALiBi as below |
| BLOOM / MPT | `modeling_bloom.py:build_alibi_tensor` (bias `slope_h · key_position`), `modeling_mpt.py` (`build_mpt_alibi_tensor`, `clip_qkv`) | ALiBi instead of RoPE; embedding LayerNorm (BLOOM); `clip_qkv` (MPT) | ALiBi: `Add(scores, Mul(slope[h], Iota[H]))` — the HF bias `slope·j` and `slope·(j − pos)` differ by a per-row constant the softmax's max-subtraction removes EXACTLY in integers; `Iota` over `H` gives `j`; `clip_qkv` = `Clamp` |
| OPT | `modeling_opt.py:OPTLearnedPositionalEmbedding` (`position_ids + 2`), `do_layer_norm_before`, `project_in/out` | offset-2 learned positions; ReLU; pre- or post-LN; projection dims | `Add` on `idx`; ReLU = `Clamp(x, 0, max)`; the LN placement is DAG shape; `MatMul` for `project_in/out` |
| OLMo2 | `modeling_olmo2.py:Olmo2DecoderLayer` (`x + norm(attn(x))`), `Olmo2Attention` (`q_norm` over the whole q projection) | post-sublayer norm; QK-norm over `Hq·d` | DAG; `RMS` over the full row |
| Cohere / Command-R | `modeling_cohere.py:CohereLayerNorm` (mean-centred, no bias), rotary `repeat_interleave(freqs, 2)` (interleaved pairs), parallel block, `logit_scale` | LayerNorm without bias; interleaved RoPE pairs; parallel attn+MLP; logit scale; QK-norm (`use_qk_norm`) | `LN`; adjacent-pair `RoPE`; DAG; the scale in the logits `N` |
| Granite | `modeling_granite.py` (`embedding_multiplier`, `attention_multiplier`, `residual_multiplier`, `logits_scaling`) | four constant multipliers | all folded into narrowing multipliers (data) |
| MiniCPM / MiniCPM3 | `modeling_minicpm3.py` (`MiniCPM3ScaledWordEmbedding`, MLA, `yarn_get_mscale`) | `scale_emb`, depth-scaled residuals, MLA | constants in `N`; MLA below |
| Nemotron | `modeling_nemotron.py:NemotronLayerNorm1P` (`weight + 1`), `relu2`, partial rotary | LayerNorm1P; squared ReLU MLP (no gate) | `LN` with `1 + w` as data; `Mul(Clamp(x,0,max), Clamp(x,0,max))`; `Slice` + `RoPE` |
| Mixtral, Qwen2/3-MoE, DeepSeek-V2/V3 | C3 | — | C3 |
| DeepSeek MLA | `modeling_deepseek_v3.py:DeepseekV3Attention` (`q_a_proj → q_a_layernorm → q_b_proj`, `kv_a_proj_with_mqa → [kv_lora_rank | qk_rope_head_dim]`, `kv_a_layernorm`, `kv_b_proj`, `expand_kv`: `k_rot` shared by all heads, `qk_head_dim = nope + rope ≠ v_head_dim`, `yarn_apply_mscale`) | low-rank q and kv; a decoupled rope part shared across heads; HF caches the EXPANDED per-head k and v | two lowerings, both v1: (a) as HF — `Hist` per-head k `[H, h, nope+rope]` and v; (b) absorbed — `Hist` of the latent `[H, kv_lora_rank + rope]`, `q̃_h = W_kbᵀ q_nope_h` (`MatMul` batched over heads), scores = `q̃·c_kv + q_rope·k_rope`, output `W_vb (Σ p·c_kv)`. (b) moves rounding points (fidelity), costs no primitive |
| gpt-oss | `modeling_gpt_oss.py`: `GptOssExperts._apply_gate` (`gate, up = gate_up[..., ::2], gate_up[..., 1::2]`; `gate.clamp(max=7)`, `up.clamp(−7, 7)`, `glu = gate·σ(1.702·gate)`, `(up + 1)·glu`), `GptOssTopKRouter.forward` (bias, top-k on logits, softmax over k), `eager_attention_forward` (sinks), alternating sliding/full layers, expert biases | attention sinks; clamped SwiGLU with interleaved gate/up; router bias | sinks: `m = max(ReduceMax(scores), sink_h)`; denominator `= ReduceSum(IntExp(s − m)) + IntExp(sink_h − m)`; the sink's probability is dropped — no `Concat` along `H` needed. `::2` = `Reshape[F,2]` + `Slice`; the clamps are `Clamp`; `σ(1.702·x)` = `Sig` of a Q24 `Mul`; biases `Gather`ed by `idx` |
| Qwen3-Next / 3.5 / 3.6 / 3.8 GDN | C4 | — | C4 |
| Mamba, Mamba2 | C5 | — | C5 |
| RWKV-6/7 | C6 | — | C6 |
| Jamba | C7 | — | C7 |
| Kimi (KDA, MLA) | C7, MLA above | — | no new primitive |

**Limit found (not in the corpus).** A model whose layers all differ in shape (OpenELM's
layer-wise head and FFN scaling) needs one block kind per distinct layer shape; past 16 distinct
shapes (NF-2) it is not expressible in v1. Raising the block cap is a size decision, not a primitive.

## 5. The GDN value-head → key-head mapping

**What HF does.** Both Qwen3-Next and Qwen3.5 expand q and k with
`repeat_interleave(v_heads // k_heads, dim=2)` before the recurrence
(`modeling_qwen3_next.py:Qwen3NextGatedDeltaNet.forward`, `modeling_qwen3_5.py:Qwen3_5GatedDeltaNet.forward`):
**grouping**, `vh → vh // r`. Qwen3-Next's `fix_query_key_value_ordering` confirms the layout: the
`r` value heads of key head `j` are stored contiguously after it.

**What the live kernel does.** `vh % k_heads` — **tiling** — in four places
(`misaka-palw-base0/src/qwen36.rs:1242`, `qwen36_plan.rs:790`, `qwen36_reference.rs:277`,
`consensus/core/src/palw_step_refute.rs:3222`). The two readings are the same function only if the
artifact's value-head axis (and every per-value-head tensor: the v block of the conv, `z`, `b`,
`a`, `A_log`, `dt_bias`, the norm weight, `out_proj`'s input columns) was permuted from grouped to
tiled order when the checkpoint was converted. Whether the pinned GGUF was so permuted is Phase 0's
question (RFC §7): the reference comment says the other reading "pairs every value head with the
wrong key from layer 0 onward", which was measured against a float reference that reads the same
GGUF — consistent with a permuted artifact, not proof of it.

**Decision for PALW-TIR.** The mapping is data (`MapHeads_g` vs `MapHeads_t`), inside the program and
therefore inside the class id; a lowerer from HF emits **grouping** with HF's weight order. A
program that tiles is a different class with a different id. The legacy convention becomes explicit.

## 6. Resolutions and additions

### 6.1 The RFC's candidates

| candidate | verdict | why |
| --- | --- | --- |
| `Scatter` | **out** | Every use in the corpus is a mask or a one-hot: DeepSeek's `group_mask.scatter_` is `ReduceMax(Compare(Iota, idx, Eq))`; KV writes are `HistAppend`; state updates are whole-tensor. No family scatters data. |
| `Sort` | **out** | Only `TopK` (a partial selection) is needed; top-p / min-p are sampling (RFC-0001). The Kimi legacy router's "remainder to the last-selected expert" is recoverable with `Compare`/`Select` (the last selected is the minimum kept probability, highest index among equals). |
| `IntSigmoid` | **library** | `IntExp`, `IntRecip` (= `IntRsqrt` + `Mul` + `Div`), `Select`, `Mul`, `Div` — byte-identical to `int_sigmoid` (`misaka-palw-tir-conformance`), same transcendental count as a primitive would have. |
| `BoundedScan` | **out** | No family has a recurrence inside one position; every recurrence is over positions (`Fixed` state). Chunked prefill (SSD, chunked GDN) is a fused kernel. |
| `BoundedMap` | **out** | Heads, experts, groups and channels are tensor axes: `MatMul` batch dimensions, broadcasting, `Gather` with `batch_dims`. No family needs a per-element body. |
| `BoundedReduce` (user body) | **refused** | Order-dependent unless proved associative and exact, which breaks order-free regions (spec 04b §6.8); the exact reductions are `ReduceSum`, `ReduceMax`, `MatMul`. |

### 6.2 Additions the corpus forced

1. **`i128` as an internal dtype.** The A16 narrowing multiplies an `i64` accumulator by an `i64`
   multiplier (up to 2^126) before rounding; `IntRecip` squares a 2^36 value; the wide RMSNorm sums
   `i32` squares over 128 lanes (2^69); BASE-0's `rope_table` adds two `i32×i32` products (2^63);
   the decay multiplies an `i64` coefficient by a Q24 softplus. The live kernels compute all of these
   in `i128`. With `i128` as a type they are `Mul`/`Div`/`Clamp` segments; without it each would
   need a fused lossy primitive (`Rescale`, `IntRecip`, a wide norm) — three primitives more, each a
   composite. `i128` never crosses a commit point.
2. **`Log2Floor`.** `q36_l2_norm` and `q36_rms_norm_wide` take the exponent out of the sum before
   `IntRsqrt` (the Q24 output grid would otherwise leave five significant bits); the same device keeps
   any wide normalisation precise. The alternative (a 127-way `Compare` ladder) costs 127× per element.
3. **`Div` with a tensor divisor and three rules.** One primitive is every rounding shift of 04a
   (`Floor`, `HalfUp` = SRDHM, `HalfAwayFromZero` = `RoundingShiftRight`), every static shift (divisor
   a const `2^s`), every per-channel or **per-token** shift — the per-token lift of ADR-0102
   (`RequantizeByToken`) is `Div(·, Pow2(Gather(shift_table, token)))` — every mean (`÷ n`), and
   exact data renormalisation (the fenced Kimi router's `(w·ONE)/sum`). It replaces `ShiftRight`,
   `DivConst` and the fused `Requantize`/`Rescale`.
4. **`Iota` over `H`.** ALiBi (`slope·j`) and any position-dependent bias over the history.
5. **`HistAppend` returns the window** (`HistRead` merged): one node per history, dependency order
   explicit; the read always includes this position's row (RFC §4.2's `min(pos+1, window)`).
6. **Long-context RoPE without a per-position table**: two (or three) pinned tables and angle
   addition (spec 04b §11.3). No `IntSinCos`.
7. **LayerNorm without a lossy division**: `c = n·x − Σx` is exact, and `LN(x) = RMS(c)` with
   `eps' = n²·eps` (because `Σc² = n³·Var(x)`, so `c/√(Σc²/n) = (x − μ)/σ`). The only divisions are
   RMSNorm's own `÷ n` of the mean of squares (Floor) and `IntRsqrt`. Range: for A16 rows of 8,192
   lanes `|c| < 2^29` and `Σc² = n·(n·Σx² − (Σx)²) < 2^70` — the wide RMSNorm's `i128` accumulation
   (`RMSw`), not the `i64` one.
8. **`Gather` with `batch_dims`** (per-row gathers: DeepSeek-V3's top-2-per-group, routing weights of
   a batch of rows).

## 7. Family × primitive matrix

`●` needed by the family's decomposition; `○` needed by some members only. All families also use the
C1 skeleton (Gather embedding, the `N` narrowing, RMS/LN, residual Clamp), shown in the first row.

| primitive | C1 dense | C2 attn var. | C3 MoE | C4 GDN | C5 SSM | C6 RWKV | C7 hybrid | C8 routing | §4 others |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Reshape | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Transpose | ● | ● | ○ | ○ | ● | ○ | ● | ○ | ● |
| Slice | ○ (RoPE pairs) | ○ | ○ | ● | ● | ● | ● | ○ | ● (partial rotary, fused qkv, `::2`) |
| Concat | ● (RoPE) | ● | ○ | ● (conv) | ● (conv) | ○ | ● | ○ | ● |
| Broadcast | ○ | ● (GQA) | ● | ● (head map) | ● (groups) | ○ | ● | ● (masks) | ● |
| Iota | ○ | ○ | ○ | ○ | ○ | ○ | ○ | ● (group mask) | ● (ALiBi) |
| Gather | ● | ● | ● (experts) | ● | ● | ● | ● | ● | ● (wpe, tables) |
| Cast | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Add / Sub / Mul | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| MatMul | ● | ● | ● (batched experts, combine) | ● (S·k, S·q) | ● | ● (WKV) | ● | ● | ● |
| ReduceSum | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| ReduceMax | ● (softmax) | ● | ● | ○ | ○ | ○ | ● | ● (group mask) | ● (sinks) |
| Div | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Clamp | ● | ● | ● | ● | ● | ● | ● | ● | ● (ReLU, clip_qkv, gpt-oss) |
| Log2Floor | ○ (wide norms) | — | ○ | ● (L2, RMSw) | ● (RMSw) | ● (L2, RWKV-7) | ● | — | ○ |
| IntExp | ● (softmax, SiLU) | ● | ● | ● | ● (dA) | ● (decays) | ● | ● | ● |
| IntRsqrt | ● (norms, recip) | ● | ● | ● | ● | ● | ● | ● | ● |
| IntLn | — | — | — | ● (softplus, decay) | ● (softplus) | — | ● | — | — |
| Compare | ● (Sig) | ● | ● | ● | ● | ● | ● | ● | ● |
| Select | ● (Sig) | ● | ● | ● | ● | ● | ● | ● | ● |
| TopK | — | — | ● | — | — | — | ○ | ● | ○ |
| StateWrite | — | — | — | ● | ● | ● | ● | — | — |
| HistAppend | ● | ● | ● | ○ (hybrid attn) | — | — | ● | ○ | ● |

Every primitive is used by at least two families; every family is covered.

## 8. The final list and the freeze criteria on paper

### 8.1 Criterion 1 — no model-specific primitive

Every primitive is named by mathematics: array structure (Reshape, Transpose, Slice, Concat,
Broadcast, Iota, Gather), integer arithmetic (Cast, Add, Sub, Mul, MatMul, ReduceSum, ReduceMax, Div,
Clamp, Log2Floor), three fixed-iteration transcendentals (exp, reciprocal square root, natural log),
comparison and selection (Compare, Select, TopK), and the two state forms of a position scan
(StateWrite, HistAppend). No primitive reads an architecture's parameters or conventions;
`prim::tests::no_primitive_is_named_after_a_model` is the tripwire. RMSNorm, softmax, RoPE,
attention, the delta rule, the selective scan, WKV, routing and the MoE combine are all library.

### 8.2 Criterion 2 — minimality, per primitive

"Equal court cost" is read as: the same MAC and transcendental counts, an elementwise count within a
small constant factor, and no extra materialised tensor of the output's size (the memory term of the
cost vector, spec 04b §8). A primitive stays only if no composition meets that.

| primitive | why it is not a composition of the others |
| --- | --- |
| Reshape | every other structural primitive keeps element order; only Reshape reinterprets it. A `Gather` emulation needs a full-size index tensor (extra memory term). |
| Transpose | a `Gather`-with-index-arithmetic emulation needs a full-size index tensor and ~2·rank elementwise ops per element. |
| Slice | as Transpose (the `Gather` emulation's index tensor), and a slice is static where a gather's index range must be proved. |
| Concat | `Select` over broadcast inputs would read every input at every output position (k× the reads) and needs a mask tensor. |
| Broadcast | `Add(x, zeros)` needs a materialised zero tensor of the output's size. |
| Iota | the only source of index-dependent values (masks, ALiBi, one-hots); a const would be up to 2^28 elements and cannot have `H`. |
| Gather | the only data-dependent read (embeddings, experts, tables, variable shifts). |
| Cast | exact conversion; `Clamp` saturates and ends an order-free region, so it cannot stand in for an exact widening inside one. |
| Add, Sub, Mul | the exact arithmetic; `Sub(a,b) = Add(a, Mul(b, −1))` costs 2× and a `Mul` by a const — kept for an honest elementwise count. (`Neg` is `Sub(0, x)`: dropped.) |
| MatMul | a `Mul` + `ReduceSum` composition materialises the `[…, M, K, N]` product (K× memory). |
| ReduceSum | the exact order-free reduction; the dissection rule is stated over it. |
| ReduceMax | `Gather(TopK(x, 1))` computes the value at equal cost but is a selection, not a reduction with a fold; the H-dissection of the softmax needs the max as a reduction. |
| Div | the only rounding operation; its three rules are 04a's; a shift is `Div` by `2^s` (so `ShiftRight` is dropped). |
| Clamp | the only saturating narrowing; `Select`+`Compare` cannot narrow the dtype (the output interval would be the union of the branches). |
| Log2Floor | a `Compare` ladder against 127 powers of two costs 127× per element. |
| IntExp, IntRsqrt, IntLn | fixed-iteration algorithms whose integer steps are defined by 04a/ADR-0052; composed from the others they would cost a full algorithm's worth of nodes per element and could not be matched by the frozen KAT vectors as one site. `IntRecip` IS a composition (`IntRsqrt`, `Mul`, `Div`) at equal cost and is dropped. |
| Compare | the only producer of a predicate. |
| Select | the only data-dependent choice between values. |
| TopK | a selection with a normative tie rule and a committed output; `Sort` is not needed, `ArgMax` is `TopK(1)`. |
| StateWrite | the only way a value reaches the next position (recurrences). |
| HistAppend | the only way to read a history (attention). |

Dropped as compositions: `Split` (Slices), `Pad` (Concat with an Iota fill), `Neg`, `Abs`, `Min`,
`Max` (Select/Compare/Sub), `ShiftLeft` (`Mul` by `Pow2`), `ShiftRight`, `DivConst` (`Div`),
`Requantize`, `Rescale`, `Narrow`, `IntRecip`, `IntSigmoid`, `ArgMax` (`TopK(1)`), `ReduceMin`
(`−ReduceMax(−x)`), `BatchedMatMul` (MatMul batch dims), `Widen` (`Cast`), `StateRead`/`HistRead`
(a `Ref`; the merged `HistAppend`).

**Twenty-five is below the RFC's 30–50 target.** The target was a hypothesis; criterion 2 is the rule,
and every candidate the corpus needs is here. The RFC's granularity rule (a primitive boundary sits at
an exact operation or a named lossy site) is what fixes the count: finer primitives would not change
the commitment cost, coarser ones would be composites.

### 8.3 Criterion 3 — stability under the last family

The last families decomposed were RWKV-7 (value residual — a second carry, no primitive), Kimi
Linear KDA (per-channel decay — a broadcast, no primitive), gpt-oss (sinks — a denominator term, no
primitive) and DeepSeek MLA (absorbed or expanded — no primitive). The only additions came early
(Log2Floor from the legacy L2/RMS-wide kernels, `i128` from the A16 narrowing, `Iota` over `H` from
ALiBi, `batch_dims` from grouped routing) and each is general: used by at least two families (§7).
On paper the set is stable; criterion 3 is re-checked when Phase E lowers the last family for real.

## 9. Fidelity thresholds for Phase E (freeze criterion 5)

Measured against the family's Hugging Face float reference (bf16 weights, fp32 softmax as HF runs
it), teacher-forced on a fixed evaluation set — proposed: 256 sequences × 512 tokens from a pinned
multilingual corpus plus 16 sequences × 4,096 tokens for the recurrent families. Per position:
top-1 agreement, `KL(p_HF ‖ p_TIR)` in nats, and the perplexity ratio. Integer quantisation changes
outputs by design; these thresholds decide whether a lowering is useful, never whether a claim is valid.

| family | top-1 agreement | mean KL (nats) | perplexity delta | recurrence drift |
| --- | --- | --- | --- | --- |
| C1/C2 dense, A16 W8 | ≥ 0.85 | ≤ 0.10 | ≤ +5 % | — |
| C3/C8 MoE | ≥ 0.80 | ≤ 0.15 | ≤ +8 % | — |
| C4 GDN | ≥ 0.80 | ≤ 0.15 | ≤ +8 % | KL at position 4,096 ≤ 1.5 × KL at 128 |
| C5 SSM | ≥ 0.80 | ≤ 0.15 | ≤ +8 % | as C4 |
| C6 RWKV | ≥ 0.75 | ≤ 0.20 | ≤ +10 % | as C4 |
| C7 hybrid | the loosest of its components | | | as C4 |

Anchors: the live A16 Qwen2.5-1.5B class measures top-1 0.877 calibrated and 0.917 held out (57/48
positions, `palw_base0_a16.rs` header); the GDN integer recurrence measures 9.1e−4 → 1.1e−3 relative
output error from 128 to 2,048 steps (ADR-0052 E) — flat, which is what the drift column requires. A
family that misses its row is a quantisation problem (calibration, per-group scales, wider codes), not
an IR problem, and is reported as such (RFC open question 3).

## 10. Findings about the legacy kernels

1. **GDN conv-row width (live, known).** `PALW_GDN_STATE_CHUNK_MAP_NAME_V1` sizes the conv row
   `(2·gdn_head_k_dim + gdn_head_v_dim)·gdn_heads` and V2's gather is
   `[q:h·k, k:heads·k + h·k, v:2·heads·k + h·v]`
   (`consensus/core/src/palw_state_chunk_map.rs:1038–1039, 1109–1111`), with one `gdn_heads`. HF's
   width is `2·k_heads·dk + v_heads·dv` (`Qwen3NextGatedDeltaNet.__init__`: `conv_dim = key_dim·2 +
   value_dim`), and the k block starts at `k_heads·dk`, not `heads·dk`. Correct only at
   `k_heads = v_heads`; Qwen3.6-35B (16/32) and Qwen3.8 (16/48) are on the wrong side. The key-head
   count is not in `PalwShapeProfileV3` at all.
2. **GDN head mapping (live, convention).** §5: the kernel tiles, HF groups; correct only for a
   V-permuted artifact. Phase 0 must check the pinned GGUF against HF.
3. **`q36_rope_partial` can overflow `i64` on the court path (live, looks like a defect).**
   It never checks that its row is A16 codes (`palw_qwen36_ops.rs:536–576`: no `check_a16(x)`), and
   the court arm passes committed lanes straight in (`palw_step_refute.rs:1772–1798`: `as_i32(&inputs[0])`).
   With a row pair `(i32::MIN, i32::MIN)` and a registered table entry `cos = sin = i32::MIN`,
   `a·s + b·c = 2^63` overflows the `i64` product at line 564, which panics under the release
   profile's `overflow-checks = true`. It needs an adversarial table (a registrant) and a crafted
   committed row (a committer) — one party can be both. `a16_rope` and `rope_table` widen to `i128`
   and are safe. I did not trace whether registration bounds rope-table values; if it does not, this
   is a remote-halt vector of the kind the 2026-09 audit closed elsewhere.
4. **Fenced Kimi court arms adjudicate less than they claim (fenced, not live).**
   `kimi_row`'s `KdaStep` recomputes from a ZEROED state and a unit output scale
   (`palw_step_refute.rs:1010–1022`), so it is correct only at position 0; it also takes one scalar
   decay per call while HF's KDA decay is per channel (`KimiLinearForgetGate`). `MlaFused` uses
   `attn_head_dim` for both `d_qk` and `d_v` and a hard-coded `>> 8` score scale with `up_bits = 0`
   (`:1023–1033`, `palw_kimi_k3_ops.rs:257`), while MLA's `qk_head_dim = nope + rope` differs from
   `v_head_dim` (`DeepseekV3Attention`). The fenced Kimi router is softmax + exact division with the
   remainder to the last-selected expert; HF Kimi Linear routes by sigmoid + selection bias + groups
   (`KimiLinearTopkRouter.forward`). None of this is live; it should not be armed as is.
5. **Seven catalog kernels are float** (`KDESC_L2_NORM`, `KDESC_RMS_NORM_FUSED`, `KDESC_SWIGLU`, the
   two glibc sigmoid/softplus, `GdnCore` NEON/AVX2 — `palw_step_refute.rs:461–467`). An integer IR
   cannot express them byte-identically; freeze criterion 6 applies to the 38 integer kernels (and the
   5 fenced ones). See spec 04b §14.
6. **Dead code, recorded:** `softmax`/`softmax_shifted`'s `sum ≤ 0` uniform branch and the router's
   uniform fallback are unreachable (the row maximum contributes `IntExp(0) = 16,781,800 > 0`, and the
   kept maximum's probability is positive); `int_rsqrt`'s normalisation loops never run (the first
   shift lands `m` in `[2^24, 2^26)`). The IR templates omit them and stay byte-identical.
7. **Non-monotonicity to respect, not fix:** `IntExp` drops across bucket edges
   (`IntExp(−LN2_Q + 1) = 8,390,167 < IntExp(−LN2_Q) = 8,390,900`) and `int_sigmoid(1) < int_sigmoid(0)`
   (documented in `q36_sigmoid_gate`). The range rules therefore use constant output intervals for the
   transcendentals (spec 04b §7).

## 11. Open questions this corpus does not settle

- The float reference for RWKV-6/7 (not in the installed `transformers`).
- Whether absorbed MLA (latent cache) or expanded MLA (HF's) is the default lowering — a fidelity and
  memory trade, both v1.
- LongRoPE's and dynamic-NTK's call-length-dependent frequencies (`longrope_frequency_update`,
  `_compute_dynamic_ntk_parameters`) have no per-position equivalent; the lowering's per-position
  choice must be measured in Phase E.
- Per-group weight scales (Q4_K-style, `q36_matmul_grouped`): expressible as a `MatMul` per group
  (`[out, G, 32] × [G, 32, 1]`, batch `G`) and a `Mul` by `Pow2(exp)` before the sum — Gate 2 checks
  byte identity with the legacy kernel.
- The Q24 activation `Table` for GELU/erf on codes costs 256 KiB per table as a param; whether such
  tables are params (per class) or a shared pinned artifact is a Gate 2 packaging question.
