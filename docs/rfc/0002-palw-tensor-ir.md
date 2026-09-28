# RFC-0002: PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels

| Field | Value |
| --- | --- |
| Status | Draft |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry), 04/04a (execution, arithmetic), 05 (canonical work), 08 (verification), 09 (court), 14 (node), 16 (fences) · all networks (dormant until armed) · `consensus/core` (admission, court), `misaka-palw-base0` (engines), `misaka-palw-sdk`, `misaka-palw-extension` |
| Branch | `rfc/0002-tensor-ir` (text only; no prototype yet) |
| Related | ADR-0135 (a model is data; Decision 7 the VM boundary), ADR-0038 A4 (100 % catalog coverage), ADR-0040 (BASE-0; Decision E order-free accumulation), ADR-0047 (A16), ADR-0049 (adjudication contract), ADR-0052 (QWEN36 ops), ADR-0057 (backends below the semantic boundary), ADR-0069 (e2e adjudicability), ADR-0082/0093 (fused attention and dissection), ADR-0102 (fenced kernels), ADR-0103/0116 (held context, history bound), ADR-0145 (canonical work) |

## 概要(日本語)

- **目的。** 「モデル登録は permissionless だが、新しいアーキテクチャは core 開発者のリリース待ち」という弱点をなくす。
  Qwen4 が公開された日に、core 開発者が何もしなくても、誰かが HF config → lowerer → Canonical IR → 静的検証 PASS → class hash →
  登録まで進められ、チェーン側は「Qwen4 とは何か」を知らないまま court が完全に裁定できる状態を目指す。
- **今の境界。** canonical VM は「層テンプレート + 45 個の kernel id」(`PalwShapeProfileV3`)で、GDN・fused attention・router の
  ような **層単位の kernel がそのまま consensus の命令** になっている。新しい種類の演算は kernel 追加 = リリース = protocol upgrade。
- **v1 の完成条件は「モデル」でなく「アーキテクチャ群」(§1)。** Dense・GQA/MQA/sliding・MoE・線形 attention(GDN)・SSM(Mamba/Mamba2)・
  recurrent(RWKV)・hybrid・routing 変種の代表モデルを **全部いったん分解し**、モデル固有 op を 1 個も足さずに表現できる最小の
  primitive 集合(30〜50 個)を逆算してから v1 を凍結する。Qwen3.8(K 16 / V 48)は適合テストの 1 本にすぎない。
- **なぜ MISAKA で現実的か。** 汎用 VM の最難関は数値の決定性だが、MISAKA は既に **整数のみ**(ADR-0040)。丸めは名前付きの
  lossy site だけにあり、**丸め・saturation・shift・除算を含まず、全中間値が証明済みの範囲に収まる区間**(order-free region, §3.2)では
  加算順序が結果を変えない。規則は一つ: **fused kernel は order-free region の中だけを並べ替えてよく、lossy site を跨いだ融合で
  値を変えてはならず、commit 点の値は 1 bit も変えてはならない。** この下では GPU(int8 dp4a / IMMA / Metal)も許容誤差なしの厳密 backend になる。
- **有界。** ループ・ジャンプ・データ依存 shape は無い。時間方向の再帰は「位置軸上の bounded scan」(§4.2)として一箇所だけ。
  登録時に型・shape、範囲(overflow・不正 shift・0 除算・範囲外 index・過大確保が無いこと)、コスト(計算・メモリ・状態・court 最悪値)、
  court が 1 tile を上限内で裁けることを **すべて静的に証明** し、できなければ登録拒否。
- **court はモデルを知らない。** 知っているのは IR node・入力 commitment・param・出力 commitment・canonical 意味論だけ。争いは
  参照インタプリタで cone を 1 つ再計算して終わる。A4(カタログ 100 % カバー)は **primitive 集合に対して一度だけ** 満たせばよい。
- **ビット一致の範囲。** 参照インタプリタ == 独立の第 2 実装 == 出荷する全 backend はビット一致を要求する。HF の浮動小数モデルとの
  比較は **忠実度指標**(固定評価集合での top-1 一致率・KL・perplexity 差、ファミリーごとの閾値)で、ビット一致ではない(整数量子化は
  意図的に出力を変えるため)。
- **順序。** コーパス → 意味論 → 参照インタプリタ(遅くて明らかに正しい)→ 範囲・コスト検証器 → lowerer → court → fused kernel →
  移行。**fused kernel は最後。** 既存 45 kernel は捨てず、IR pattern の fused 実装としてビット一致すれば残す。
- **工数感。** 2〜4 人で 6〜9 か月。既存の Qwen3.6-35B(16/32)も踏んでいる GDN の k≠v 不具合の修正は、本 RFC とは別の小さな
  修正として独立に進められる(§7 Phase 0)。
- **名称と「VM ではない」こと(09-28)。** PALW-TIR は有限の静的グラフ + 静的な層スケジュール + 位置方向の scan だけで、実行中の
  分岐・ループ・ジャンプ・呼び出しは無い。reference evaluator はそれを評価するだけで、program counter を持つ VM ではない。
  題名を Canonical ML IR から **PALW Canonical Tensor IR** に改めたのはこの誤解を避けるため(crate 名 `misaka-palw-tir` は元から一致)。
- **frontend とコンパイラ基盤(09-28 評価)。** 主経路は HF の `config.json` + `transformers` の modeling コード → lowerer。
  ONNX・GGUF は v1 凍結後の任意の第 2 frontend(consensus 外)。MLIR は consensus にも参照ツールにも入れない — 誰が MLIR 等で
  TIR を生成してもよく、正準エンコーディングと検証器がそのまま接点になる。採らない理由は Alternatives の表。
  Rust 製の pliron(MLIR 風)も評価し、参照 lowerer の内部 IR には今は採らない(検証器・正準形・評価器は consensus 側が自前で持つ
  必要があり、pliron はその二重定義になる)。第三者の利用は自由、凍結後に任意の dialect adapter、lowerer の Gate 2 で再評価。

## Summary

Replace the consensus meaning of a PALW class — today a layer-template graph whose nodes name one of
45 hand-written kernels — with **PALW Canonical Tensor IR v1 (PALW-TIR v1)**: a closed set of 30–50
architecture-neutral integer primitives, and a class program that is a static, bounded DAG over them,
carried as chain data. **v1 is frozen on an architecture corpus, not on a model**: it is accepted only
when representatives of every major decoder family lower to it without a single model-specific
primitive. Native kernels, including every kernel we have today, become optional fused kernels: node
software that is byte-identical to the reference evaluator at every commit point and never part of
the identity. The court knows the primitives and nothing about models.

**Terminology: an IR, not a VM.** A PALW-TIR program is a finite static DAG, a static layer schedule
and a scan over positions whose trip count is the job's length. It has no program counter, branch,
jump, loop or call, and every cost is known at registration. The *reference evaluator* evaluates such
a program; it is an interpreter only in the sense that it walks the graph (in `misaka-palw-tir`).

## Motivation

### 1. Where the boundary is today

ADR-0135 Decision 7: *a graph the canonical VM expresses is permissionless; a new op is a protocol
upgrade.* The rule is right. The problem is how coarse the VM's instructions are.

- **The graph is one fixed schema.** `PalwShapeProfileV3` (`consensus/core/src/palw_step.rs:409-500`)
  holds four node tables (`pre / gdn / attn / post`, ≤ 64 nodes each), per-layer weight templates,
  geometry scalars, and a layer schedule that is either attention or GDN by `full_attention_interval`
  (two hard-coded variants, V1 `(i+1)%n` and V2 `i%n`, the second added for Kimi: `:752-765`).
- **An op is a kernel.** Each node names a `kernel_semantics_id`. The 18-variant `op_kind` tag
  (`:191-235`) is only a label (`MulElem` carries a requantize kernel, `SoftMax` a router top-k). The
  catalog holds 45 kernels: 7 float, 10 BASE-0, 12 A16, 16 Q36, plus 1 + 4 fenced
  (`palw_step_refute.rs:286-331`). BASE-0's ten are primitives; the Q36 and Kimi kernels are whole
  subgraphs (`GatedDeltaNet`, `AttnFused`, the router, the MoE combine).
- **Conventions live in code, not in the identity.** GDN value head `vh` reads key head
  `vh % k_heads`, hard-coded three times (engine `misaka-palw-base0/src/qwen36.rs:1239-1262`, float
  reference `qwen36_reference.rs:273-277`, court `palw_step_refute.rs:3218-3222`). The schema has one
  `gdn_heads` (`palw_step.rs:459`); the key-head count is not in the class id. The state chunk maps
  size the conv row as `(2k+v)·gdn_heads` (`palw_state_chunk_map.rs:1038-1039,1109-1111`) where the
  true width is `2·k_heads·hd + v_heads·hd` — correct only when `k_heads == v_heads`. Qwen3.6-35B
  (16/32) and Qwen3.8-27B (16/48) are both on the wrong side of it (`fp_recompute.rs:1296-1306`).
- **Semantics also live in names.** Economic compute detects MoE by the substrings `"router"` and
  `".routed"` in weight names (`palw_economic_compute_v1.rs:297-325`). `UnsupportedOp` is derived from
  `verification_ccu > 0` (`palw_model_registry_v1.rs:792`), a proxy. The manifest's `graph_ir_root`
  is a placeholder used only in tests (`palw_model_registry_v1.rs:29-37`).

### 2. What a new architecture costs today

| Lineage | Consensus code | New kernels | Runtime / tools |
| --- | --- | --- | --- |
| BASE-0 | ≈3,100 lines | 10 | `engine.rs`, `plan.rs` |
| Qwen2.5 A16 | ≈2,100 (+ A16 tier 1,237) | 0 | ≈13,000 lines of engine/backend/convert |
| Qwen3.6 | ≈4,450 incl. court arm | 16 + 1 fenced | ≈10,600 |
| Kimi K3 | ≈1,700 | 4 fenced | none; no e2e certificate |

Every family has needed a profile module, a court arm per kernel, an engine, a converter and an SDK
lineage (`docs/palw-model-onboarding-sdk.md:80-82`). That is a release per family, and each release
is a fence, a drill and a flag day. Adding `QWEN_GDN`, `MAMBA2` or `DEEPSEEK_MOE` as ops of a new IR
would reproduce the same problem a few years later.

### 3. Why this is tractable here and would not be elsewhere

The hard part of a general ML VM on a chain is numerical determinism: reduction order, FMA
contraction, rounding modes, vendor kernels. **MISAKA already removed it.** Every live class is
integer-only (ADR-0040 A: no libm, no float on the consensus path). Inside a proved no-overflow bound,
integer addition is exactly associative (ADR-0040 E). Information is lost only at *named* sites, each
with its own rule (ADR-0040 C1, ADR-0047, ADR-0052). ADR-0057 already lets a backend fuse and reorder
below a committed row provided the row is byte-identical.

The step from "a catalog of kernels" to "a catalog of primitives" therefore needs no new numerical
theory. It needs the kernels decomposed along the lossy sites they already have, and the checks the
kernels perform by hand (bounds, shapes, court cost) turned into static analyses over a program.

## Goals and non-goals

**Goals.**

- G1. A class program is data: a static DAG over a closed primitive set, hashed into the class id.
- G2. **v1 is complete for an architecture corpus (§1), with zero model-specific primitives.**
- G3. Every admissible program is adjudicable by construction; A4 is proved once, for the primitives.
- G4. Every admissible program's ranges, costs and court ceilings are derived at registration, exactly.
- G5. The court knows no model. It interprets primitives over committed values.
- G6. Native speed stays available through fused kernels outside the identity — built last.
- G7. The canonical work vector (ADR-0145) is derived structurally, with no name heuristics, and stays
  representation-neutral (PALW-WK-3).
- G8. Today's classes keep running unchanged; today's kernels become fused kernels of IR patterns.
- G9. A tool answers "is this model permissionless?" from a Hugging Face `config.json`, with the same
  function consensus admission calls.

**Non-goals.** Turing completeness; loops with data-dependent trip counts; jumps; recursion; dynamic
shapes. Floating point on the consensus path (ADR-0053 stands). Arbitrary user code (WASM, eBPF, CUDA)
as consensus semantics. zk proofs of inference. Changes to collateral, panels, the lottery or the
claim lifecycle. Sampling (it stays in the logits scheme and the FP job, RFC-0001).

## 1. The conformance corpus and the v1 freeze criterion

This section is the RFC's acceptance test and is fixed **before** any implementation.

### 1.1 The corpus

| # | Family | Representatives (at least one real checkpoint each, plus reduced configs) | What it exercises |
| --- | --- | --- | --- |
| C1 | Dense Transformer | Llama 3.x, Qwen2.5/Qwen3 dense, Mistral 7B, Gemma 2/3 | RMSNorm variants (Gemma's `1+w`), SwiGLU/GeGLU, RoPE variants (YaRN, partial), logit soft-capping, tied embeddings |
| C2 | Attention variants | Llama (GQA), Falcon/PaLM-style (MQA), Mistral / Gemma (sliding + global) | head grouping, windowed history, alternating layer schedules |
| C3 | MoE | Mixtral 8×7B, Qwen-MoE / Qwen3-MoE, DeepSeek-V2/V3 | top-k routing, shared experts, grouped (group-limited) routing, normalised and unnormalised gates |
| C4 | Linear attention | Qwen3.6 / Qwen3.8 GDN (16:16, 16:32, 16:48, 24:72, 32:128 head ratios) | gated delta rule, short causal conv, L2-normalised keys, unequal key/value heads |
| C5 | SSM | Mamba, Mamba2 | selective scan, `softplus(dt)`, `exp(A·dt)` decay, conv window, grouped B/C |
| C6 | Recurrent | RWKV-6/7 | token shift, data-dependent decay, WKV state |
| C7 | Hybrid | Qwen3.6 (attention + GDN), Jamba (attention + Mamba + MoE), Kimi-style schedules | per-layer block schedules, mixed state kinds |
| C8 | Routing variations | top-1/top-2/top-8, shared + routed, grouped top-k, expert bias terms | selection semantics and tie rules |

Each family is exercised at two scales: **reduced configurations** (same architecture, small widths
and depths, random and extreme-range weights — for bit identity and fuzzing) and **one real small
checkpoint** (≤ 3B where one exists — for fidelity).

### 1.2 The freeze criterion

PALW-TIR v1 is acceptable only if **all** of the following hold, and it is frozen (its `prim_set_id`
fixed) at the moment they first hold together:

1. **No model-specific primitive.** Every corpus family lowers to the primitive set. A primitive is
   named and defined by mathematics (`MatMul`, `Gather`, `IntExp`), never by an architecture or a
   model (`QwenGdn`, `Mamba2Scan`, `DeepSeekRoute` are refused by name).
2. **Minimality.** No primitive is expressible byte-identically by the others at equal court cost.
   The target is 30–50 primitives.
3. **Stability under the last family.** If lowering the last family added to the corpus forces a new
   primitive, v1 is not frozen: the new primitive must be general (useful to at least two families,
   or a standard mathematical operation), and the whole corpus is re-lowered.
4. **Three-way bit identity.** For every corpus program, on every generated input:
   `reference evaluator == independent second implementation == every backend that ships`, byte for
   byte, at every commit point.
5. **Fidelity against the float original.** The integer program is compared with the family's
   Hugging Face float reference on a fixed evaluation set — top-1 agreement, KL divergence and
   perplexity difference — against per-family thresholds set in Phase A. **This is not a bit-identity
   requirement**: static integer quantization changes outputs by design, and ADR-0053 withdrew
   tolerance from consensus. Fidelity decides whether a lowering is *useful*, never whether a claim is
   *valid*.
6. **Legacy conformance.** Each of today's 45 catalog kernels is expressed as an IR segment and is
   byte-identical to it (so today's semantics are a subset of v1, and today's kernels can become fused
   kernels).
7. **Static verifiability.** Every corpus program passes the range, cost and court analyses of §3
   within the proposed ceilings, or the ceiling is revised with a stated reason.

Qwen3.8 (K 16 / V 48) is one test in C4, not the goal. Under the criterion, `16:32`, `16:48`, `24:72`
and `32:128` are shape data, not kernels.

## 2. Architecture

```
HF config.json + transformers modeling code (primary) · ONNX / GGUF importers (optional) · custom
             │   (outside consensus: anyone may write these)
             ▼
   Architecture lowerer (per family, data-driven templates)
             ▼
   High-level ML graph (RMSNorm, attention, GDN, MoE … as library subgraphs)
             ▼
   PALW Canonical Tensor IR v1 (PALW-TIR) ── tensor shape/index · integer arithmetic · reductions
                                     routing/indexing · quantization boundaries · bounded state
═════════════════════ CONSENSUS BOUNDARY ═════════════════════
   Static verifier (types · ranges · costs · court cones)   → admission
   Reference evaluator (slow, obviously correct)            → the court, the meaning of "correct"
             │
   Backends (CPU · Metal · CUDA), generic primitive kernels + fused kernels   → node software
```

Consensus knows the IR and nothing above it. Who wrote a lowerer, an importer or a Metal kernel
confers no protocol authority.

**Frontends and compiler infrastructure.** The primary frontend is a Hugging Face checkpoint: its
`config.json` read conservatively (every key that can change the math is understood or the model is
refused, never silently ignored) and its semantics taken from the `transformers` modeling code, which a
new architecture ships with on release day. ONNX and GGUF importers are optional secondary frontends
after the v1 freeze; they target the same high-level graph and add nothing to consensus. No compiler
infrastructure is a dependency of consensus or of the reference tools: anyone may produce TIR with
MLIR, TVM or a script, because the canonical encoding (§4.1) and the verifier
(`palw-class check-architecture --tir`) are the whole interface.

## 3. The primitive set (starting hypothesis)

Phase A derives the final list from the corpus. The table below is the hypothesis Phase A starts from
and must confirm, extend or shrink.

**The granularity rule.** A primitive boundary sits at an exact integer operation or a named lossy
site; everything above that is library (§3.3). Finer primitives do not raise the commitment cost,
because commit points are chosen per cone (§6), not per node — the lower bound on granularity is
exactness at the lossy sites, the upper bound is that no primitive knows an architecture.

### 3.1 Types and shapes

Tensors are raw integers. Quantization parameters are op attributes (`Requantize(mult, shift)`) or
parameter tensors from the artifact (per-channel scales, ADR-0050 D); there are no float values and no
scale types.

| Type | Use | May be committed |
| --- | --- | --- |
| `i8`, `i16` | activations (BASE-0 int8, A16 codes), weights | yes |
| `i32` | narrowed accumulators, fixed point Q[k], logits, state | yes |
| `i64` | exact wide accumulators inside a segment | **no** (the A16 rule "i64 never crosses a step boundary", generalised) |
| `idx` (u32) | token ids, positions, selection indices | yes |

Rank ≤ 4. Every dimension is a constant except the symbolic `H` (history length,
`1 ≤ H ≤ min(pos + 1, window) ≤ history_bound`, PALW-EX-16).

### 3.2 The order-free region

**Definition.** An *order-free region* is a connected set of nodes in which (a) every node is an exact
primitive (§3.3 kinds S and E), (b) no node saturates, shifts right, divides or rounds, and (c) the
range analysis (§5.2) proves every intermediate value, **for every association and every order of
every sum in the region**, within its declared type. Only inside an order-free region may an
implementation reassociate, reorder, regroup, tile or parallelise arithmetic. A region ends at every
lossy primitive, every selection primitive, every commit point and every state write.

Condition (c) is the reason the range analysis bounds a sum by `len · max|term|` rather than by the
final value: a partial sum in any order is then also inside the bound.

### 3.3 Primitives

| Kind | Primitives | Notes |
| --- | --- | --- |
| **S — structure** (exact, no arithmetic) | `Reshape`, `Permute`, `Slice`, `Split`, `Concat`, `Broadcast`, `Pad(const)` | static bounds, checked at admission; head tiling and grouping are `Reshape`+`Broadcast` |
| **S — indexing / routing** | `Gather(axis)`, `Scatter(axis)` (candidate), `Iota` | `Gather` over a pinned table is also how narrow-input activations and RoPE tables are evaluated (data, not arithmetic) |
| **E — exact arithmetic** | `Widen`, `Add`, `Sub`, `Mul`, `Neg`, `Min`, `Max`, `Abs`, `ShiftLeft` | results in a declared type proved to fit |
| **E — linear algebra** | `MatMul(acc)`, `BatchedMatMul(acc)` | contraction length bounded by range analysis |
| **E — reductions** | `ReduceSum(axis, acc)`, `ReduceMax(axis)`, `ReduceMin(axis)` | |
| **L — quantization boundaries** (lossy, each names its rule in `R = {HalfAwayFromZero, HalfUpSRDHM, Floor, Saturate}`) | `Requantize`, `Rescale`, `Narrow(shift, rule, to)`, `Clamp(lo, hi)`, `ShiftRight(n, rule)`, `DivConst(d, rule)` | the only primitives that lose information |
| **L — integer transcendentals** | `IntExp`, `IntLn`, `IntRsqrt`, `IntRecip`, `IntSigmoid` (candidate: library) | the fixed-iteration algorithms of 04a Decision F and ADR-0052 D, constants pinned by hash; never libm |
| **X — selection** | `Compare(cmp)`, `Select`, `ArgMax`, `TopK(k)`, `Sort` (candidate) | ties break to the **lowest index**; `TopK` returns its set in index order (ADR-0052 B) |
| **T — bounded state** | `StateRead`, `StateWrite` (saturates to the declared range), `HistAppend`, `HistRead(window)` | the position-axis scan of §4.2 |
| **T — within-step scan** (candidate) | `BoundedScan(body, axis, len)` | only if the corpus shows a recurrence *inside* one position (see §4.2) |

About 40 primitives, in the target range. Four properties follow:

- **Activation functions on narrow inputs are data.** An `i8`/`i16` activation (GELU, SiLU, a new one
  next year) is `Gather(table, x)` over a 256- or 65,536-entry table pinned in the artifact — PALW-EX-5
  ("a transcendental evaluated at registration is data") applied to activations.
- **Head layouts are data.** `vh % k_heads` (tiling) and `vh / (v_heads / k_heads)` (grouping) are
  both `Reshape` + `Broadcast` + `Reshape`. Which one a model uses is in its program and its class id.
- **MoE is explicit.** Routing is `TopK` → `Gather` over expert weights; grouped routing is
  `TopK` over group scores → `Select` mask → `TopK`. Canonical work counts active experts from the
  graph, not from weight names.
- **Composite ops are library subgraphs**, not primitives: `RmsNorm`, `LayerNorm`, `SoftMax`, `Silu`,
  `GeGLU`, `L2Norm`, RoPE, attention, the gated delta rule, the selective scan step, the WKV step, the
  causal conv, the MoE combine. The library (`tir_library_v1`) is data used by lowerers and by fused
  kernel patterns. Consensus sees only expanded primitives.

`Scatter`, `Sort`, `IntSigmoid` and `BoundedScan` are marked *candidate*: Phase A keeps them only if a
corpus family needs them and the others cannot express them at equal court cost.

## 4. The program, `TirProgramV1`

### 4.1 Structure

```
TirProgramV1 {
  version: 1,
  prim_set_id: Hash64,                                            // PALW-TIR v1
  inputs:  { token: idx, pos: idx },                              // one position per step
  params:  [ParamDecl { name, dtype, shape, per_layer: bool }],   // bound to artifact inventory tensors
  consts:  [ConstDecl { name, dtype, shape, bytes }],             // small inline constants, ≤ 64 KiB total
  states:  [StateDecl { name, kind: Fixed | Hist, dtype, shape, range, window?, per_layer: bool }],
  blocks:  [Block { name, carry_in, nodes: [Node], carry_out }], // e.g. pre, attn_layer, gdn_layer, moe_layer, post
  schedule: { layers: u16, kinds: [u8; layers] },                 // which block each layer runs
  outputs: { logits: NodeRef, logits_scheme_id },
  history_bound: 2^18 | 2^21,
}
Node { prim: u8, attrs: canonical bytes, inputs: [Ref; ≤ 8], out: { dtype, shape }, commit: bool }
Ref  = Node(i < self) | CarryIn | Param(j) | Const(j) | State(j) | Input(j)
```

- **The layer loop is a schedule, not a loop.** `schedule.kinds` names, per layer, the block that
  layer runs (`layers ≤ 1024`). This generalises `full_attention_interval` V1/V2 to any pattern —
  Gemma's sliding/global alternation, Jamba's attention/Mamba/MoE mix, a model whose first layers are
  dense and the rest MoE — without a schema version. Per-layer params and states are indexed by layer.
- **Refs point strictly backward.** Node order is the canonical topological order; the node index is
  the node's identity in commitments (`input_refs` already enforces this, `palw_step.rs:640-652`).
- **Canonical encoding.** Borsh, in a normal form admission enforces (no dead nodes, no duplicate
  consts, canonical attribute order, shapes fully inferred and equal to the declared `out`), so the
  program hash is the program.
- **Identity.** `graph_ir_root = H(TirProgramV1)` becomes a real field (the ADR-0135 placeholder). The
  class id commits to `graph_ir_root`, `artifact_root`, the tokenizer and the logits scheme, and **not**
  to node-local knobs (`n_threads`, `repack_on`, which today are inside the class id,
  `palw_step.rs:466,478`).

### 4.2 Time: the bounded scan over positions

A program computes **one position**: the logits for `token` at `pos`, reading and writing its states.
A run of `T` positions is the scan of that step over `0..T-1`, bounded by `history_bound`. This is the
bounded scan, and it lives in exactly one place:

- `Fixed` states carry recurrences: the GDN state `S`, the Mamba state `h`, the RWKV WKV state, conv
  windows, token shift. `StateWrite` saturates to the declared range (a named lossy site), so the
  recurrence stays inside its interval whatever the data. (Its *real* bound is ADR-0052 E's
  contraction argument; the saturation makes the analysis total, not tighter.)
- `Hist` states are append-only histories: the KV cache. `HistRead(window)` yields the last
  `min(pos + 1, window)` rows, which covers sliding-window attention with no dynamic slice.

Prefill is the same step at positions `0..T-1`. A backend batches prefill however it likes
(PALW-EX-12's "one pass over the weights" is a scheduling rule and is unchanged), but its committed
values must equal the per-position semantics. For recurrences this means a chunked-parallel prefill
(Mamba2's SSD form, chunked GDN) is a fused kernel only if it reproduces every per-position lossy site
— possible but hard, and exactly the kind of work that comes last.

**Within-step scans.** If a corpus family has a recurrence *inside* one position (none of C1–C8 is
known to), Phase A adds `BoundedScan(body, axis, len)` with a static `len`, a body that is itself a
TIR block, and the body's cost multiplied by `len` in every analysis. Until then it is not in v1.

## 5. Static verification (admission)

Admission of an IR class runs four analyses, each one pass, linear in the unrolled node count. A
program that any analysis cannot prove is refused; there is no "probably fine".

### 5.1 Types and shapes

Every node's output type and shape is inferred and must equal its declared `out`. `H` propagates
symbolically. A shape error is a refusal, never a panic (PALW-EX-5's totality rule).

### 5.2 Ranges

Each tensor gets an integer interval. Inputs and params take their type's full interval (a weight
declared `i8` is `[-128, 127]` — weights are not trusted to be small); states take their declared
range; `H` takes its bound. Each primitive has a sound transfer function (for `MatMul`,
`len · max|a| · max|b|`, which also bounds every partial sum in every order, §3.2(c)). Admission must
prove, for every node:

- **no overflow** of any exact primitive's declared type, including every partial sum;
- **no illegal shift** (every shift amount static and within `[0, width)`);
- **no invalid divisor** (`DivConst` divisors are nonzero constants; division inside the transcendental
  algorithms is proved nonzero by their domains);
- **no out-of-bounds index** (`Slice` bounds static; `Gather`/`Scatter` index intervals within the
  gathered axis; `TopK` `k` ≤ axis length);
- **no oversized allocation** (every tensor and every state within the size caps of §5.4).

The result is a guarantee handed to every runtime: *inside an order-free region, overflow cannot
happen.* That guarantee is what makes reassociation and fusion safe.

### 5.3 Costs

Each primitive has closed cost formulas. The analysis reports, per position and at the worst case
(`H = history_bound`):

- **compute**: MACs, elementwise ops, transcendental evaluations;
- **memory**: bytes read and written, peak live bytes in canonical order;
- **state**: bytes of `Fixed` and `Hist` state per layer and in total;
- **court worst case**: the largest terminal recomputation any single disputed tile can require (§6).

The formulas upper-bound the reference evaluator's work; their coefficients are benchmarked with a
margin in Phase D. Admission refuses a program over any network ceiling.

### 5.4 Court feasibility and size caps

For every commit point, the *cone* of its output — the nodes back to the nearest commit points, params,
consts, states and inputs — must fit the court's existing ceilings (`derive_court_cost_v1`: ≤ 16 Mi
terminal MACs per tile, ≤ 8 operands, ≤ 27 close chunks; `palw_mode_v2.rs:551,616,620`). Large nodes
are adjudicable at three levels, **IR node → canonical chunk → tile**:

- a `MatMul` output is committed in tiles; one tile is `tile_len` dot products, each within the
  contraction bound;
- a reduction over `H` must be **dissectable**: between the `HistRead` and the reduction there are
  only exact primitives and lossy primitives that act on one history position (a per-position
  `IntExp` of a score is fine; rounding a running sum is not). The reduction is exact, so the court
  dissects it by partial sums over canonical chunks of `H`, and the partials add without error. This
  is ADR-0082's attention dissection stated as a property of the program, and it fixes the canonical
  softmax as two-pass (maximum first, then exact sums);
- a cone that crosses a `StateRead` stops there: the state is an operand opened from the checkpoint
  leg, or replayed from the nearest checkpoint. Admission derives each `Fixed` state's maximum
  checkpoint interval `C` (the canonical chunk of the position scan) so that replaying `C` positions of
  its update cone fits the ceiling.

Size caps (proposed; sized in Phase D): program ≤ 256 KiB; blocks ≤ 16; nodes per block ≤ 512;
layers ≤ 1024; inputs per node ≤ 8; rank ≤ 4; any dimension ≤ 2^24; elements per tensor ≤ 2^28;
consts ≤ 64 KiB; states ≤ 16 per layer; step leaves ≤ 2^22 (existing). The registration price (1 MSK
burned, ≤ 4 bought registrations per block, `palw_state_v2.rs:7411,7426`) bounds how often admission
runs.

## 6. Commitment and the court

- **Commit points.** A node with `commit = true` materialises its output as committed tiles in the
  step leg, as a node output does today (`(call, node_slot, position, tile)`,
  `palw_step_leg.rs:1207-1228`), with `node_slot` = the node's unrolled index. Lanes stay 4 bytes.
- **Required commit points:** every block's `carry_out`; the logits; every `idx` tensor produced by a
  selection primitive that feeds a `Gather` over params (a wrong selection makes everything
  downstream unrelated, and no arithmetic bisection converges — ADR-0052 B — so the selection itself is
  an opened value); the input of every `StateWrite` and `HistAppend`. Lowerers add further commit
  points until §5.4 passes.
- **A cone is the unit of adjudication.** The terminal refutation opens one committed output tile and
  its cone's committed inputs, proves params against the artifact root (as today), and **interprets
  the cone** with the reference evaluator. One court arm replaces the per-kernel arms (`qwen36_row`,
  `base0_row`, `kimi_row`, …).
- **The court knows no model.** It sees a node, its input commitments, its params, its output
  commitment and the primitive semantics. A4 becomes "the court covers PALW-TIR v1", proved once;
  `palw_court_catalog_root_v1` gains `prim_set_id`; the coverage certificate for an IR class is "every
  node's primitive is in v1". For an admitted program `Unadjudicable`
  (`palw_step_refute.rs:2514-2520`) can only mean an interpreter defect, which the second
  implementation and the golden vectors exist to catch.
- **Bisection is unchanged** (`palw_bisect.rs`: step leaves or trace events, ≤ 48 rounds, rung
  deadlines, silence loses) over the same step space.

## 7. Fused kernels (built last)

A fused kernel is `(pattern, implementation)`: an IR subgraph template with exact structural
constraints (primitives, attributes, shapes, ranges) and native code for it.

- **F-1 (byte identity).** For every input in the pattern's declared domain, a fused kernel must
  produce, at every commit point inside or at the edge of the pattern, the bytes the reference
  interpreter produces. There is no tolerance (ADR-0057 D1).
- **F-2 (what may move).** A fused kernel may reorder, regroup, tile, vectorise or parallelise only
  inside order-free regions (§3.2). It must reproduce every lossy site's result; fusing *across* a
  lossy site is allowed only if the lossy site is still computed on exactly the same value.
- **F-3 (outside the identity).** A fused kernel is node software. It appears in no object, class id
  or fingerprint, and needs no fence (ADR-0057 D1/D2). Shipping one is a node release.
- **F-4 (the gate).** A fused kernel ships only with a differential gate that fires: random and
  range-extreme inputs against the reference evaluator *and* the independent second implementation,
  per backend (ADR-0057 D3).
- **F-5 (matching).** A pattern matches by structural equality after canonicalisation. An unmatched
  subgraph runs on generic primitive kernels.
- **F-6 (GPU).** Every primitive is integer and every order-free sum is order-independent, so an integer
  GPU backend (int8 `dp4a`, tensor-core IMMA, Metal integer SIMD) is an *exact* backend under F-1, not a
  tolerant family. ADR-0053's objection was to tolerance, which this does not need.

**Today's kernels are kept, as fused kernels.** Freeze criterion 6 expresses each of the 45 kernels as
an IR segment. Where the segment is byte-identical on the differential gate, the kernel becomes that
pattern's fused implementation; today's GDN, router and attention engines keep their speed.

## 8. The tool: `palw-class check-architecture`

```
palw-class check-architecture --config config.json [--weights-index model.safetensors.index.json]
palw-class check-architecture --tir program.tir
```

A lowerer (non-consensus, `misaka-palw-sdk`) turns a Hugging Face config into a program using the
library templates; the verdict comes from `tir_admit_v1`, the same function consensus admission calls,
without network state.

| Verdict | Meaning |
| --- | --- |
| `ADMISSIBLE` | registrable as data; fused kernels cover every hot pattern |
| `ADMISSIBLE_GENERIC` | registrable as data; some patterns run on generic kernels (estimated slowdown printed) |
| `EXCEEDS(limit, value, cap)` | a size, range, cost or court ceiling fails |
| `NEEDS_PRIMITIVE(name)` | the graph needs an operation outside v1 — a protocol upgrade (ADR-0135 D7) |
| `NOT_LOWERABLE(reason)` | no lowerer template for this family; the model may still be expressible by hand |

A legacy mode reports the same verdicts against today's `PalwShapeProfileV3` catalog, so the tool is
useful before the IR is armed.

## 9. Proposed Spec text (new chapter `spec/palw/04b-tensor-ir.md`)

Applies past `palw_tir_v1`. Legacy `PalwShapeProfileV3` classes keep chapter 04 unchanged.

- **PALW-TIR-1 (the primitive set).** An IR class's execution MUST be the evaluation of its
  `TirProgramV1` over PALW-TIR v1. No other operation has consensus meaning.
- **PALW-TIR-2 (integers only).** Every value on the consensus path MUST be an integer of a declared
  type. ADR-0040 A applies to the interpreter and to every backend.
- **PALW-TIR-3 (named loss).** Only lossy primitives MAY lose information, each by its declared rule.
  Every other primitive MUST be exact.
- **PALW-TIR-4 (order-free regions).** An implementation MAY reassociate or reorder arithmetic only
  inside an order-free region (§3.2). Everywhere else it MUST follow the program's order.
- **PALW-TIR-5 (no wide commitment).** An `i64` tensor MUST NOT be a commit point.
- **PALW-TIR-6 (bounded structure).** A program MUST be a DAG with strictly backward refs and a static
  layer schedule of at most 1024 layers; no dimension other than `H` MAY be non-constant.
- **PALW-TIR-7 (canonical form and identity).** Admission MUST refuse a program not in normal form.
  The class id MUST commit to `graph_ir_root` and MUST NOT commit to node-local knobs.
- **PALW-TIR-8 (shapes).** Admission MUST infer every shape and refuse any mismatch.
- **PALW-TIR-9 (ranges).** Admission MUST prove from type-worst-case inputs that no exact primitive or
  partial sum overflows, no shift is illegal, no divisor can be zero, no index is out of bounds and no
  allocation exceeds its cap. A program it cannot prove is refused.
- **PALW-TIR-10 (state).** A `Fixed` state MUST declare its range and `StateWrite` MUST saturate to it.
  A `Hist` state MUST be append-only and bounded by `history_bound` and its window.
- **PALW-TIR-11 (selection).** Selection primitives MUST break ties to the lowest index; `TopK` MUST
  return its set in index order. A selection that feeds a param `Gather` MUST be a commit point.
- **PALW-TIR-12 (cost).** No worst-case cost of §5.3 MAY exceed the network's ceiling.
- **PALW-TIR-13 (court cones).** Every commit point's cone MUST fit the court ceilings; every reduction
  over `H` MUST be dissectable; admission MUST derive each `Fixed` state's checkpoint interval.
- **PALW-TIR-14 (commit points).** Block outputs, logits, selections that feed a param `Gather`, and
  every state write MUST be commit points.
- **PALW-TIR-15 (the court).** A terminal refutation of an IR class MUST recompute the disputed tile
  by interpreting its cone over PALW-TIR v1. `court_catalog_root` MUST commit to `prim_set_id`.
- **PALW-TIR-16 (canonical work).** The work vector (PALW-WK-1) of an IR class MUST be derived from the
  program's structure; commit points and `tile_len` MUST NOT change it (PALW-WK-3).
- **PALW-TIR-17 (backends).** A backend MAY implement any subgraph natively, provided every commit
  point is byte-identical to the reference evaluator. A fused kernel MUST NOT appear in any
  consensus object.

## Activation plan

The order is the point: **corpus → semantics → reference evaluator → verifiers → lowerers → court →
fused kernels → migration.** Fused kernels come last so that no performance need can pull a
model-specific semantic back into the IR.

| Phase | Work | Consensus change | Exit gate | Effort (2–4 people) |
| --- | --- | --- | --- | --- |
| **0** (independent, may run now) | (a) fix GDN `k_heads ≠ v_heads` in the *live* kernels — it already affects Qwen3.6-35B (16/32); (b) `check-architecture` legacy mode | (a) its own small fence on testnet-12; (b) none | (a) a 16/32 and a 16/48 fixture replay and adjudicate end to end | 2–4 weeks |
| **A. Corpus** | Lower every corpus family (§1.1) to a high-level graph and then, on paper and in a throwaway prototype, to candidate primitives; set fidelity thresholds per family | none | a primitive list that meets freeze criteria 1–3 on paper | 4–6 weeks |
| **B. Semantics** | The normative definition of every primitive (types, shapes, exact integer semantics, rounding rule, range transfer function, cost formula); golden vectors `consensus-vectors/tir-v1/` | none | reviewed spec text; vectors for every primitive incl. range extremes | 3–4 weeks |
| **C. Reference evaluator** | `misaka-palw-tir`: slow, obviously correct, no SIMD, no fusion; and an independent second implementation from the spec alone | none | both pass every golden vector; they agree on fuzzed programs | 4–6 weeks |
| **D. Verifiers** | `tir_admit_v1`: types, ranges, costs, court cones, normal form, size caps; cost coefficients benchmarked; admission CPU time measured | none | every corpus program verifies; mutation tests (overflowing, OOB, oversized, non-dissectable programs) are all refused | 4–6 weeks |
| **E. Lowerers** | Library templates and lowerers for C1–C8; quantisation/calibration tooling; fidelity runs; legacy conformance of the 45 kernels | none | freeze criteria 4–7; **v1 frozen, `prim_set_id` fixed** | 6–8 weeks (overlaps C/D) |
| **F. Court and admission** | Interpreter court arm, generic `H` dissection, generic checkpoint replay, structural work derivation, admission v10 — all behind dormant `palw_tir_v1`; drills | dormant fence | devnet drill registering **Qwen2.5-A16 as an IR program** (same weights): same logits as the legacy class, same verdicts on the forged-output battery (8/8); a drill that crosses the fence on the shipping binary | 6–8 weeks |
| **G. Fused kernels** | Legacy kernels as fused patterns first; then CPU/Metal/CUDA kernels for hot patterns | none (node releases) | F-4 gates fire and pass per backend | continuous |
| **H. Migration** | Arm `palw_tir_v1` on testnet-12 at a post-launch flag day; first new family as pure data; mainnet: IR is the admission path for new classes (`palw_tir_only_v1`) | fences | registration → panel → Final for a model no core developer wrote code for | — |

Total to Phase F: about 6–9 months with two to four experienced people. `palw_tir_v1` is a `Params`
fence like ADR-0102's: a dormant network fingerprints as if it did not exist; when armed, the params
fingerprint gains `prim_set_id` and the IR ceilings. On testnet-12 it would join the first fence list
after the capacity steps (ADR-0160), at a height chosen when Phase F's drill passes.

## Alternatives

| Alternative | Why not |
| --- | --- |
| Keep adding kernels (status quo) | A release, a fence and a drill per family; conventions hide in code and bite later (the GDN k≠v defect) |
| An IR that grows per model (`QwenGdn`, `Mamba2Scan`, `DeepSeekRoute` as ops) | The same problem, one level down, in a few years. Refused by freeze criterion 1 |
| Generalise kernels one by one | Worth doing for live bugs (Phase 0), never changes the nature of the boundary |
| A Turing-complete deterministic VM (WASM-like) | Halting and metering become runtime problems; the court must bound an arbitrary program; nothing an LLM needs requires it |
| A zkML VM | Proving cost is orders of magnitude above re-execution; the unilateral court already gives safety |
| ONNX / StableHLO as the consensus IR | Open op sets, float semantics, implementation-defined corners. Good lowerer *inputs*, not consensus |
| ONNX importer as the primary frontend (evaluated 2026-09-28) | A new architecture reaches ONNX export later than `transformers`, sometimes never, which defeats day-one registration; exporter-specific decompositions (attention, RoPE, caches) must be pattern-matched back into composites; QDQ carries float scales and ties-to-even rounding and is re-quantized anyway. Kept as an optional secondary frontend after the freeze |
| Upstream MLIR as the compiler infrastructure, a MISAKA dialect (evaluated 2026-09-28) | A C++/LLVM toolchain in a Rust node for an IR of ~40 primitives whose lowering is template expansion; consensus could not depend on it in any case. Allowed for third-party compilers (the encoding is the interface); revisit for Phase G code generation, outside the identity |
| pliron (Rust-native, MLIR-inspired: dialects, ops, regions, verification, printing, passes) as the reference lowerer's internal IR (evaluated 2026-09-28) | What it provides is what consensus must own itself: the typed DAG, the verifier, the canonical encoding and the evaluator live in `misaka-palw-tir`, and admission and the tool call the same verifier (G9). A pliron `palw.tir` dialect would be a second definition of every primitive's type and shape rules plus a second representation with translation layers, not less work; what the lowerer adds on top is domain logic (configs, weight layouts, float reference, calibration) it does not help with. Also 0.x with a breaking minor release about every six weeks and no declared MSRV, against this repository's pinned toolchain. Third-party compilers may use it; an optional dialect adapter (pliron ⇄ canonical bytes) after the freeze; re-evaluated at the lowerer's Gate 2 if the lowerer grows non-trivial rewrite passes |
| RLX (a Rust ML compiler/runtime: HIR → MIR → LIR, memory planning, ONNX/GGUF import, multi-backend) as the compiler or runtime | Float, JAX-shaped semantics and a fast-moving API; the byte-identity rule (F-1) needs integer kernels written against this IR. Reference material for Phase G backends and the secondary frontends |
| Cranelift as the IR | An SSA code-generation IR without tensor semantics; a candidate for CPU code generation of generic kernels in Phase G, outside the identity |
| TOSA's integer profile as the semantic base | The closest existing integer specification and useful prior art, but its rescale rounding is not the frozen rules the live kernels use (ADR-0040 C1/C2), and it has no state, position scan, commit points or court cost |
| Structured control inside a step (`BoundedMap`, `BoundedReduce` with a user body) | Heads, experts and channels are tensor axes, so batching needs no control flow; a user-body reduction is order-dependent unless proved associative and exact, which breaks §3.2. `BoundedScan`/`BoundedMap` enter only if the corpus needs them (Phase A) |
| Bounded extension programs for what the IR cannot express | A second execution model for the court to adjudicate; revisit only if `NEEDS_PRIMITIVE` proves frequent after v1 |
| A float IR with tolerances | ADR-0053: tolerance turns fraud proofs into votes on noise |
| Every primitive committed | Multiplies step leaves several-fold against the 2^22 cap; cones give the same adjudicability at today's commitment cost |

## Security and economic analysis

- **Unadjudicable programs (A4).** An attacker registers a program the court cannot try and lies in it
  for free. Closed: every node must be a v1 primitive and every cone within ceilings. Residual: an
  interpreter defect — two independent implementations, golden vectors, fuzzing, and the legacy
  conformance against kernels that already survived the red-team battery.
- **Overflow to break order-freedom.** A program whose partial sums overflow would make hosts with
  different reduction orders disagree, so an honest seat looks like a liar. Closed by §5.2 from
  type-worst-case inputs and partial-sum bounds; weights cannot escape a bound that does not trust them.
- **Cost under-metering.** A program cheap on paper and slow in reality would stall panels. Cost
  formulas upper-bound the reference evaluator, coefficients carry a benchmarked margin, and the
  worst-case step is capped. A registrant with a private fused kernel is faster than the network until
  the kernel is public — P5's "efficiency is rewarded" — while seats run generic kernels at the metered
  cost. Whether weight (ADR-0069) should also require a measured generic-backend throughput floor is
  open.
- **Mis-fusion.** A fused kernel applied to a subgraph it does not implement convicts whoever ran it,
  never the network: the court runs the interpreter. F-5 and F-4 make it unlikely.
- **Selection games.** Near-ties decided by rounding would make "which expert" a vote on noise.
  PALW-TIR-11 fixes the tie rule and commits the selection.
- **Identity grinding.** Encodings of one program would give free class ids. Normal form makes the
  encoding unique; node-local knobs leave the id.
- **Registration DoS.** Linear-time admission with capped sizes; the 1 MSK burn and the 4-per-block
  cap stay; admission CPU time is measured in Phase D and becomes a ceiling if needed.
- **Canonical work.** Derived structurally: `dense_matmul` = `MatMul` with a non-gathered `Param`;
  `routed_expert_matmul` = `MatMul` whose `Param` passes through a `Gather` indexed by a `TopK`
  (active experts only); `attention_*` = primitives over `H`; `recurrence` = the update cones of
  `Fixed` states; `normalization` = cones feeding `IntRsqrt`; the rest `other_verified_ops`; bytes from
  `HistRead`/`HistAppend` and the artifact layout. Commit points and `tile_len` feed none of it.
- **Overall surface.** One interpreter of ~40 small primitives replaces 45 bespoke court arms and
  their per-kernel shape rules.

## Compatibility and migration

- Live classes and kernels are untouched; legacy admission (v9) stays for existing lineages.
- After freeze criterion 6, each legacy kernel is also a fused kernel of an IR pattern. A legacy class
  may be re-registered as an IR program (new class id, same weights, same outputs); nothing forces it.
- The ADR-0135 manifest's `graph_ir_root` and `runtime_version` become real (`runtime_version` =
  `prim_set_id`); `UnsupportedOp` becomes `NEEDS_PRIMITIVE(name)` from the verifier.
- Economic compute's name heuristics are retired for IR classes.
- ADR-0135 Decision 7 is unchanged in wording; its scope widens because the VM expresses more.

## Open questions

1. **The final primitive list**, decided by Phase A and frozen in Phase E — in particular `Scatter`,
   `Sort`, `IntSigmoid`, `BoundedScan`, and whether Kimi's KDA or DeepSeek's MLA need anything new.
2. **Existing `AttnFused` vs "dissectable".** Does today's fused attention compute exact two-pass sums
   over the history? If it rescales a running sum, legacy conformance fails for it and the difference
   must be resolved before the freeze.
3. **Integer fidelity of the corpus.** Static int8/A16 quantisation with frozen activation scales may
   lose too much on some families (outlier activations in large dense models, long-context SSMs). The
   thresholds and the calibration tooling are Phase A/E work; a family that cannot meet them is a
   quantisation problem, not an IR problem, and must be reported as such.
4. **Program carriage**: inline in the admission carriage up to 256 KiB, or an artifact leaf with only
   `graph_ir_root` on chain.
5. **Weight for IR classes (ADR-0069)**: is the drilled primitive set enough, or does each program still
   need its own end-to-end drill (responder availability was ADR-0093's gap, not arithmetic)?
6. **A generic-backend throughput floor** as a condition of weight.
7. **Normalization** in the work vector: `IntRsqrt` cones, or folded into `other_verified_ops`.
8. **A8/A16**: separate families inside the IR, or dtypes of one family.
9. **The GDN head convention** in the live kernel (`vh % k_heads` tiling vs grouping): Phase 0 must
   check it against the model's reference semantics; the IR makes the choice explicit.

## Decision

<Open.>
