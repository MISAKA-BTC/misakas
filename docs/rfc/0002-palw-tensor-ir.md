# RFC-0002: PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Draft — implementation in progress: Phases A–C passed Gate 1 on 2026-09-28 (see *Implementation status*) |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry), 04/04a (execution, arithmetic), 05 (canonical work), 08 (verification), 09 (court), 14 (node), 16 (fences) · all networks (dormant until armed) · `consensus/core` (admission, court), `misaka-palw-base0` (engines), `misaka-palw-sdk`, `misaka-palw-extension` |
| Working branch | `codex/rfc02-implementation` (2026-10-10 revision); the original text-only branch was `rfc/0002-tensor-ir` |
| Related | ADR-0135 (a model is data; Decision 7 the kernel boundary, historically called VM), ADR-0038 A4 (100 % catalog coverage), ADR-0040 (BASE-0; Decision E order-free accumulation), ADR-0047 (A16), ADR-0049 (adjudication contract), ADR-0052 (QWEN36 ops), ADR-0057 (backends below the semantic boundary), ADR-0069 (e2e adjudicability), ADR-0082/0093 (fused attention and dissection), ADR-0102 (fenced kernels), ADR-0103/0116 (held context, history bound), ADR-0145 (canonical work), ADR-0171/0172 (new verification and kernel-only extension) |
| Part II | Model onboarding (header-only preflight, streaming loader, runtime packs, quant registry, feature scope, staged enablement) — appended 2026-10-01; §II.7.5 records decided rules, not yet built. §§II.10–II.12 add Hub evidence, future-family extension and local-LLM ≥90% coverage; revised 2026-10-06 to remove the VM residual, all open acceptance work |

## Current requirements and historical records — 2026-10-10

The accepted revision in [§II.14](#ii14-completion-contract--2026-10-10) applies the 2026-10-10
design review: complete the public compiler interface, preserve efficient sparse/stateful computation,
derive bounded resources, and bind the whole computation through public verification and terminal court.
It keeps the deterministic tensor IR and versioned kernel boundary. It does not declare any unfinished
implementation, model, resource envelope, coverage target or activation gate complete.

| Current requirement | Authoritative sections | Historical material retained below |
| --- | --- | --- |
| Typed bounded tensor/state graph; **25 frozen v1 primitives**; reusable versioned extensions when necessary | spec 04b and §II.14.2 | The original 30–50 primitive estimate and Gate 1 development sequence |
| Reference/independent/backend bit equality; separate source-checkpoint fidelity | §1.2, §II.4–II.5, §II.14.3 | Architecture fixtures and old conversion measurements prove only their recorded scope |
| Approved probabilistic whole-claim verification, bounded dispute replay and public conviction | Current design boundary, §II.13, mission amendment, §II.14.6; RFC07/14 | Routine full replay in the original admission/verification profile |
| Model acquisition is outside consensus; fixed model identity and claim-specific evidence remain required | ADR-0177 and §II.14.6 | Old model-availability/serving gates are not reinstated |
| Resource ceilings, canonical work and bond-bounded economic credit are separate quantities | ADR-0176, §II.14.4–II.14.5 | Historical PWU measurements are not authorization to enlarge B/R/F rights |
| Completion needs the public extension path and real-checkpoint staged evidence | §II.10–II.12, §II.14; [implementation ledger](../design/palw/tir/rfc0002-implementation.md) | Old “not yet built” annotations record their date, not a current code audit |

## Current design boundary — 2026-10-06

[ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
selects versioned kernels and bounded declarative plans, not BVM/GVM or a universal VM.
TIR is a typed tensor/state graph with bounded scan, not an uploaded ISA or arbitrary verifier.
Existing layer-template code sometimes called a “canonical VM” is a kernel evaluator. New combinations
of active primitives need a frontend and plan; missing semantic, memory, checker or court relations
require a reusable kernel extension and coordinated activation. “SegWit-class” means versioned
extension, not automatic soft-fork compatibility; unknown rewarded operations fail closed.

For new large-model profiles, [RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)
and [RFC11](0011-permissionless-model-and-long-context-onboarding.md) govern ordinary verification:
bind encoded constraints, perform approved small probabilistic checks covering the whole claim,
then require positive receipts, DA and the challenge window before Final. Exact bounded cone replay
is the dispute backstop, not compulsory routine replay of every selected segment. Admission checks
plan structure, coverage and bounded verifier/court resources using kernel rules; it does not prove
a future inference by recomputing it. Reference evaluators, integer semantics and legacy static-
admission/replay profiles below remain conformance and historical baselines. This amendment changes
no live class or active checker. §§II.11–II.12 replace the former residual-GVM route; ≥90% coverage
remains an unmeasured acceptance target.

## 概要(日本語)

- **目的。** active Kernelで表現できる新アーキテクチャの登録を、モデル名ごとのcoreリリース待ちから解放する。
  新モデルの全演算・taskがactive Kernelで表現できれば、HF config → lowerer → Canonical IR / plan → 登録へ進められる。
  本当に新しい演算は汎用Kernel更新を待つ。モデル名ごとのallowlistやVM fallbackを使わない。
- **今の境界。** canonical kernel evaluator は「層テンプレート + 45 個の kernel id」(`PalwShapeProfileV3`)で、GDN・fused attention・router の
  ような **層単位の kernel がそのまま consensus の命令** になっている。新しい種類の演算は kernel 追加 = リリース = protocol upgrade。
- **v1 の完成条件は「モデル」でなく「アーキテクチャ群」(§1)。** Dense・GQA/MQA/sliding・MoE・線形 attention(GDN)・SSM(Mamba/Mamba2)・
  recurrent(RWKV)・hybrid・routing 変種の代表モデルを **全部いったん分解し**、モデル固有 op を 1 個も足さずに表現できる最小の
  primitive 集合を逆算してから v1 を凍結する。Gate 1と正式仕様04bのv1は25個で、30〜50個は当初の見積り。
  必要な共通意味論は新versionで拡張できる。Qwen3.8(K 16 / V 48)は適合テストの1本にすぎない。
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
- **順序。** 意味論を固定し、参照実装・範囲／コスト検証・lowerer・checker・courtを揃える。
  高速実装と証拠生成の性能測定は並行し、実サイズで実現不可能なloweringを後から発見しない。
  既存kernelはIR patternのfused実装としてビット一致する範囲で利用する。
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

- **Part II(10-01 追記)。** モデル受け入れの標準経路 — header だけで読む preflight、streaming 変換と runtime pack、量子化 descriptor、feature scope、convert / register / mine の段階的な有効化(consensus に触れる §II.7.5 は決定済み・未実装)。本文は末尾の Part II。

## Summary

Replace the consensus meaning of a PALW class — today a layer-template graph whose nodes name one of
45 hand-written kernels — with **PALW Canonical Tensor IR v1 (PALW-TIR v1)**: the frozen set of 25
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

ADR-0135 Decision 7: a graph the active kernel evaluator expresses is permissionless; a new op is
a protocol upgrade. The original ADR called this the “canonical VM”; it is not a future model VM.
The problem is how coarse the existing kernel relations are.

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
   perplexity difference — against per-family thresholds set in Phase A. The reference is pinned:
   the `transformers` version the fixtures name (5.17 at Gate 1), eager attention, and per-position
   decode — `transformers` disagrees with itself across attention backends and between prefill and
   decode (soft-capping under sdpa, dynamic NTK and LongRoPE frequency switching). **This is not a bit-identity
   requirement**: static integer quantization changes outputs by design, and ADR-0053 withdrew
   tolerance from consensus. Fidelity decides whether a lowering is *useful*, never whether a claim is
   *valid*.
6. **Legacy conformance.** Each of today's integer catalog kernels (the 45 less the 7 float kernels,
   plus the fenced `RequantizeByToken`) is expressed as an IR segment and is byte-identical to it (so today's semantics are a subset of v1, and today's kernels can become fused
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
consts ≤ 64 KiB; states ≤ 16 per layer; step leaves ≤ 2^22 (existing). The registration price (1 BILI
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

**Runtime residency (node software, like the fused kernels).** Where a class's weights are held is the
node's decision, under ADR-0112's budget, with the tiers read off the program's dataflow — pinned
params read whole every forward, routed rows a route selects (a mixture's experts), gathered rows an
input selects (embeddings, n-gram tables) — and nothing read through a page fault:
[`docs/design/palw/tir/runtime-residency.md`](../design/palw/tir/runtime-residency.md). It changes
no byte any commitment sees (the identity is tested at the floor, at a fifth and against the page
cache), appears in no object, id or fingerprint, and ships as a node release.

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
| `LOWERABLE_UNVERIFIED` | lowered from an architecture whose float reference is remote code the tool cannot run offline; the verdict above it holds, the fidelity column is empty |
| `EXCEEDS(limit, value, cap)` | a size, range, cost or court ceiling fails |
| `NEEDS_PRIMITIVE(name)` | outside active kernel semantics: report `KERNEL_EXTENSION_REQUIRED` with relation and limits, then propose a reusable semantic/checker/court extension (§II.12); no VM fallback |
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

## Implementation status

- **Gate 1 (2026-09-28): Phases A–C.**
  - **Corpus:** `docs/design/palw/tir/corpus-v1.md`, with every semantic claim cited to the `transformers` modeling code.
  - **Normative text:** `docs/spec/palw/04b-tensor-ir.md`.
  - **Crate `misaka-palw-tir`:** the reference evaluator with golden vectors `consensus-vectors/tir-v1/`. The BASE-0 frozen KAT and the A16/Q36 kernels are byte-identical as IR segments.
  - **Crate `misaka-palw-tir-lower`:** HF `config.json` + safetensors → frontend-neutral high-level graph → f32 reference. It matches `transformers` 5.17 on 57 tiny architectures and is estimated to cover about 91 % of decoder-only HF checkpoints by count.
  - **The primitive set came out at 25**, below the 30–50 target: minimality removed every composition. `Requantize`, `Rescale`, the shifts, `IntRecip` and `IntSigmoid` are library segments, byte-identical to the legacy kernels. `Scatter`, `Sort`, `BoundedScan` and `BoundedMap` are out, and a user-body `BoundedReduce` is refused. The additions are an internal-only `i128`, `Log2Floor`, and a `Div` with a tensor divisor, which carries every rounding shift.
  - **Deviations recorded in 04b §14:**
    - `HistAppend` returns the window.
    - Fixed state is committed at checkpoints rather than at every position (a per-position GDN state is far above the step-leaf cap).
    - `token_bound`.
    - Committed operands are checked against their proven intervals (PALW-TIR-33: out of interval is a malformed commitment and the producer loses).
    - Params get a 2^40 sanity bound instead of the 2^28 cap.
- **Gate 2 (running):** the composite library, conformance against the live kernels, `tir_admit_v1`, quantisation and fidelity, the independent second implementation, and the Phase F integration design.
- **Found on the way (legacy code, outside this RFC):**
  - A court arm that could panic on hostile committed lanes. It is being fixed as a node update.
  - The fenced Kimi court arms disagree with the model: zeroed KDA state, equal-head-dim MLA, and a softmax router where HF uses a grouped sigmoid.
  - For GDN, HF maps value head `vh` to key head `vh / (v_heads / k_heads)` (grouping). The live kernel's `vh % k_heads` equals it only after a value-head reordering at conversion, which Phase 0 checks.

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
| Bounded extension programs for what the IR cannot express | Withdrawn under ADR-0172: use a generic versioned semantic/checker/court kernel extension, not a second ISA execution model or guest fallback (§II.12) |
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
- **Registration DoS.** Linear-time admission with capped sizes; the 1 BILI burn and the 4-per-block
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

---

# Part II — Model onboarding: a standard path from a Hugging Face directory to a registered, reproducible and honestly staged class

*Added 2026-10-01 (lane F, branch `tir/onboard`). Part I defines the IR, the court and the fused-kernel rule; Part II
defines everything between a model someone downloaded and a class that earns. **Part II adds no primitive and changes
no consensus rule.** §II.7.5 records the consensus rules *decided* on 2026-10-01 under a dormant fence; they are specified there and
not yet built. Everything else in Part II is tooling, data formats and node-side reporting, outside consensus (P7).*

## Part II in one page

Part I makes a model *expressible as data*. It does not make *getting a model there* a standard path, and today each
model is refused for a different reason, late: after hundreds of GB were downloaded and converted. Part II specifies eight
things, one per requirement of §II.0.1.

1. **A header-only preflight (§II.2).** Reading only the Hugging Face config files plus the safetensors headers, or the
   GGUF header, it reports the architecture and its features, every tensor's storage type, the vision/audio/MTP parts the
   class would not compute, the artifact size, the testnet-12 conditions the chain would judge (admission sizing, close
   sizes, court cost, DA ladder, the court window, the canonical job), and the memory a seat needs, and then names, per
   stage (**convert**, **register**, **mine**), what is missing. It answers before a single weight is fetched.
2. **The converter as a function with a streaming loader (§II.3) and versioned runtime packs (§II.4).** The converter holds
   one block of a tensor at a time. A *runtime pack* is one manifest pinning the source, the frontend, the quantisation
   descriptors, the profile, the converter and its math, the executors, the logit convention, the result and the
   conformance vectors, so that anyone can rebuild the same artifact root and check it on three independent executors.
   Adding a model never inherits another model's conditions: every condition is in the pack and in its digest.
3. **Quantisation as data (§II.5).** A checkpoint's quantisation is a descriptor file (`misaka.palw.quant-format.v1`)
   read by one interpreter. An unknown type is refused by name, with the safe paths that exist; a new one is a file with
   test vectors, not a release.
4. **Feature scope (§II.6).** A class is one function. What the model has beyond it (a vision tower, an audio tower, a
   projector, MTP heads) is named with its evidence in the pack, the listing and the preflight; the class either computes
   a part entirely or says it does not.
5. **Staged enablement (§II.7).** Registration is existence; mining is a separate, gated stage. The chain's lifecycle
   already separates them; §II.7 states what it does and does not guarantee against the requirement, gives the registry a
   structured `blocking` field (stage, code, what is missing, how much), and records the consensus rules, decided but not yet built,
   that close what it lacks.
6. **Hub-wide end-to-end coverage (§II.10).** The 50–100 architecture corpus measures the lowerer, not how many Hugging Face
   repositories can register, obtain independent seats, produce a valid claim and reach `Final`. A pinned Hub census and a
   stratified, real-checkpoint cohort measure every gate separately. The largest measured failure categories determine the
   next generic frontend, format, modality, admission or verification work; a declaration of broad coverage needs the
   end-to-end evidence, including panel capacity.
7. **Future-model permissionlessness (§II.11).** A new family whose computation fits the armed TIR primitive set and
   canonical job profiles can supply its own compiler or declarative frontend pack and register canonical program and
   artifact bytes without a `main` release or model-name allowlist. Missing semantics require a reusable versioned
   kernel extension, checker/court/resource evidence and coordinated activation before registration.
8. **The local-LLM kernel-coverage objective (§II.12).** Aim for at least 90 % of a pinned, publicly runnable local-LLM cohort
   to reach registration, independent seats and `Final` through active tensor/verification kernels. Publish the
   residual extension/resource/source gaps; do not presume that a VM covers the remainder.

## 概要(Part II、日本語)

- **目的。** 「このモデルは登録できるか、何が足りないか」を、数百 GB を落とす **前** に答える。今は一つ一つのモデルが別の理由で、遅く拒否される。
- **preflight(§II.2)。** HF の config と safetensors の header(GGUF は header だけ)しか読まない。architecture と feature(Level A/B/C)、
  tensor ごとの量子化型、vision/audio/mmproj/MTP、artifact の推定サイズ、t12 の条件(admission の大きさ、close の大きさ ≤ 搬送上限、
  court cost ≤ 2^26、DA の ladder、court window、canonical job)、seat の必要メモリを出し、**convert / register / mine** のどの段階で何が欠けるかを名指しする。
- **runtime pack(§II.3–II.4)。** 変換は streaming(常駐メモリは最大 chunk のみ)。pack は source・frontend・量子化 descriptor・profile・converter(数学は
  `libm-v1`)・executor・conformance を 1 つの manifest に固定し、誰でも同じ artifact root を再構築して、独立 3 実装で確認できる。他モデルの条件は黙って継承されない。
- **量子化はデータ(§II.5)。** GGUF/GPTQ/AWQ/FP8/compressed-tensors/MXFP4/bitsandbytes は descriptor ファイル。未知の型は **名指しで拒否** し、安全な経路を示す。新型は release でなくファイル+テストベクタ。
- **feature scope(§II.6)。** text-only はすべての面に出る。vision / audio / projector は「未対応の feature」であり、部分対応にはしない。
- **段階的な有効化(§II.7)。** 登録は存在、mining は別の段階。現行 lifecycle の穴(ready seat は bond 数・probation の claim は報酬が出る・実行系の認証 gate が無い・seat のメモリ不足は無言)を洗い出し、
  node 側だけで直せる部分(registry の構造化 `blocking`)は実装し、consensus に触れる部分は dormant fence(`palw_class_seating`、全 class 種別に 1 つの着席規則)として 2026-10-01 に決定済み(未実装)。
- **完成基準(§II.8)。** 50〜100 の代表 architecture、既存 feature で ≥ 90 % 表現可能、到達可能な TIR op への court coverage 100 %。
- **HF 全体への到達(§II.10)。** 上の 90 % は代表 architecture の下ろしやすさであり、HF のモデル登録率ではない。公開モデルの
  repo と revision を固定して、取得可能性、変換、実サイズの admission、独立 seat、claim の `Final` を別々に測る。失敗を
  format・feature・modality・court・座席/処理能力に分類し、件数の大きい汎用的な欠落から閉じる。実走なしに「大半が登録・Final」と言わない。
- **将来のモデル(§II.11)。** 既存 TIR primitive と canonical job で意味を記述できる新型モデルは、第三者の frontend pack または
  直接生成した TIR / plan を使って、`main` のモデル別更新なしに登録できる経路を必須とする。未対応演算・checker・courtは
  汎用Kernelの合意更新が必要で、VMでは迂回しない。有効化と実サイズの実走までは対応済みに数えない。
- **ローカル LLM のKernel coverage目標(§II.12)。** 公開・取得可能な固定cohortで、active tensor/verification Kernel経路による
  ≥ 90 % の登録・独立 seat・`Final` を目標にする。残りはKernel拡張・format・resource等の欠落を公開し、未実装を成功と数えない。

## II.0 Requirements, acceptance and the three examples

### II.0.1 The requirements

The user's, in the user's order. R1–R5 are the onboarding path; R6 adds the 2026-10-03 Hub-majority outcome;
R7 makes future-family onboarding independent of a `main` release where the armed semantics suffice;
R8 sets the local-LLM active-kernel coverage target, replacing the historical 90/10 VM split.
The acceptance criteria are §II.0.2.

- **R1. A header-only preflight.** Read only the Hugging Face config files plus the safetensors headers, or the GGUF
  header (metadata KV and tensor infos, never the data section). Report: supported architecture and features; per-tensor
  quant types; vision/audio components; estimated artifact size; the testnet-12 court and canonical-job conditions
  (admission v10 sizing; close sizes ≤ the carried cap; court cost ≤ 2^26; DA step leaves against the ladder, 2^22 below
  `palw_tir_fence2` and the class's own ladder up to 2^30 past it; the court window); and the seat resources needed.
- **R2. Versioned runtime packs.** One manifest identifies the model config, weight format, profile, converter version,
  executor and conformance vectors. Adding a model never silently inherits another model's conditions, and registration
  is reproducible.
- **R3. An extensible quant registry.** Unknown types are refused explicitly, never misread. New types can be added as
  decoder modules with tests. Refusals present the safe paths that exist.
- **R4. Feature scope shown.** A text-only artifact says so in the manifest, the listing and the preflight. A model that
  needs extra files (a vision projector, an `mmproj`) is shown as an unsupported feature, never as partial support.
- **R5. Registration separated from mining enablement.** Mining claims and rewards stay off until the class is
  adjudicable, the executor is certified (conformance), and enough ready seats from distinct operators have possession
  proofs. A failure says which stage (convert / register / mine) lacked what, in both the CLI and the chain registry.
- **R6. Most Hub models can complete that path, measured as repositories rather than architecture examples.** The
  system must name each blocking gate, close the largest generic gaps without a model-specific consensus path, and prove
  that a representative set of real checkpoints can register, be independently seated and reach `Final` (§II.10).
- **R7. Permissionless extension within an armed semantic envelope.** An unknown model family that can be expressed
  with the armed TIR primitives and job profiles must be registerable through a third-party frontend or direct canonical
  TIR submission, with no model-name allowlist, built-in feature-registry entry or `main` code change (§II.11).
- **R8. Local-LLM kernel coverage.** Target at least 90 % of a dated, reproducible cohort of publicly runnable local
  LLM checkpoints through active tensor/verification kernels. The residual requires explicit extension/resource
  work, not RFC05's withdrawn VM fallback; publish real-size registration-to-Final evidence (§II.12).

### II.0.2 Acceptance

- **A1.** A model with a custom quantisation gets its verdict, and the features it lacks, before its full weights are
  fetched.
- **A2.** The artifact root, the converter version and the profile are reproducible and pass an independent conformance
  check.
- **A3.** The testnet-12 preflight (shape depth and above) shows the court window and the canonical-job conditions.
- **A4.** Mining starts only after the ready seats of distinct operators and their possession proofs.
- **A5.** Failures are visible per stage, in the CLI and in the registry.
- **A6.** A pinned, auditable Hub census and real-checkpoint cohort meet the quantitative end-to-end claim rule of
  §II.10.5. The architecture-corpus percentage alone cannot satisfy A6.
- **A7.** An independently authored, previously unknown family and weight layout pass the no-`main`-change
  registration-to-`Final` exercise in §II.11.4; unsupported semantics refuse by name and never execute as an unknown op.
- **A8.** Meet §II.12's ≥90 % active-kernel threshold and confidence requirement, with a complete residual table.
  Residual failures stay in the denominator; A8 does not establish 100 % coverage or support for an inactive
  extension. A proposed kernel route is not a measured result.

### II.0.3 The three examples

| Example | What it needs | Where Part II answers it |
| --- | --- | --- |
| **`qwen4_exp`** — QSA (sparse block attention), a hashed n-gram per-layer embedding, a gated residual; 125 B parameters with 6 B active plus 51 B of n-gram tables; about 360 GB | feature lowerers that exist as data/generic code, not a Qwen4 parser; a streamed conversion; a verdict from headers | §II.1 (Levels), §II.3, §II.2: the architecture is a *combination of features* (lane G); the preflight reads a 360 GB checkpoint's headers (kilobytes) and says Level A/B/C, the artifact size, and the admission sizing |
| **`Mitsuba-ComfyUI-27B-GGUF`** — a Qwen3.8-27B-family GGUF with custom types `PQ2_0` / `PTQ1_0` and an `mmproj` | a quant registry that refuses a custom type by name and takes a descriptor file; feature scope for the `mmproj` | §II.5 (the refusal names the type id and the tensors, lists what is described, and gives the safe paths); §II.6 (`mmproj` is a modality the class does not take; the class is the text decoder). The two custom types stay refused until their layout is specified; a descriptor file can add them with no code change |
| **A Qwen3.5-9B refused for its court window** — "the court needs 5,102 DAA, the t12 limit is 3,000" | the court window as a *named, early* condition | §II.2.3(7) (the preflight computes the window and the limit at the asked height) and §II.2.6 (the plug point for the class-specific window that release int-10 carries) |

### II.0.4 Where each requirement lives

| Requirement | Part II | State on `tir/onboard` |
| --- | --- | --- |
| R1 preflight | §II.2 | specified here; the command is task 7. Its inputs exist: `TensorSource` headers (§II.3), `model::analyze`/`ArchitectureReport` (lane G), `model::scope_of`, `check_ir_config_v1`, the descriptor registry |
| R2 packs | §II.3, §II.4 | implemented: `palw-class pack build \| verify \| show` (`468ae4404`); `libm-v1` (`537ca553f`); streaming converter (`b0d799769`, `0876a1744`, `ef1f8ef40`); content-addressed chunks (`e6fbb3ba2`); streamed inventory root (`e502567ba`) |
| R3 quant registry | §II.5 | implemented: 29 ggml types (`957d8e26d`), GPTQ, AWQ, FP8 block, three compressed-tensors formats (`1db3c430f`), MXFP4 from safetensors (`096e88de5`), bitsandbytes nf4/fp4/int8 (`f74d1a333`); known-undescribed list (`17febcd9d`) |
| R4 feature scope | §II.6 | implemented (`d7c0d776a`); shown by the report, the pack and `misaka model inspect` |
| R5 staged enablement | §II.7 | analysed here; the registry's `blocking` field and its CLI display are node-only and specified in §II.7.4; the consensus rules of §II.7.5 are decided (2026-10-01) and not yet built |
| R6 Hub-majority outcome | §II.10 | new acceptance program, **not an implementation claim**; requires a live Hub census, real checkpoints and an adequately seated end-to-end drill |
| R7 future-family extension | §II.11 | proposed direct-TIR and third-party frontend contract; existing data adapters and quant descriptors cover only their current languages, so this is **not yet a proven end-to-end capability** |
| R8 local-LLM kernel coverage | §II.12 and [Kernel design §§K.0–K.8](../design/palw/versioned-kernels.md) | unmeasured target; real-size checker/court/panel resources need evidence; no VM route |
| A1–A5 | §II.8 | A2 is met by the pack; A1, A3, A4, A5 complete with the preflight (task 7), `blocking` and the decision on §II.7.5 |
| A6 | §II.10.5 | open until the measured `register` and `Final` gates pass; neither the 92 % A/B corpus reading nor the earlier Hub estimates close it |
| A7 | §II.11.4 | open until third-party direct-TIR and frontend-pack paths pass with unchanged node/consensus binaries and independent seats |
| A8 | §II.12 | open until the dated cohort, active-kernel end-to-end funnel, confidence requirement and residual table meet the target |

## II.1 Principles

These come from lane G's frontend (`misaka-palw-tir-lower`, branch `tir/generic`;
`docs/design/palw/tir/model-adapter-v1.md` §§1–6, `docs/design/palw/tir/hf-coverage.md`; commits `c52b8b3a7` ModelSpec V1
and the feature registry, `f2a364b74` Gemma-4 as data with no per-family Rust in the default build, `1a4964205` the Qwen4-Exp
fixtures through the data adapter). Part II depends on them and restates them because every part of the onboarding path
must obey them.

- **P1. `model_type` is informational.** Nothing is selected by it. Two models whose `ModelSpec`s are equal compute the same
  function whatever they are called, and the court never sees the name.
- **P2. A finite, versioned feature vocabulary.** A model is a *combination* of features (`FeatureId`s of `model::REGISTRY`:
  attention variants, MoE routing, linear recurrences, state-space blocks, per-layer inputs, KV sharing, hyper-connection
  streams, hashed n-gram embeddings, …). A feature is lowered once, by a generic lowerer, to the existing primitives.
  Adding a feature adds a lowerer and tests, never a primitive and never a per-model code path.
- **P3. Levels A, B and C, and adapters are data.**
  - **A**: the standard keys and tensor names suffice; the reader's own template reads the configuration.
  - **B**: an adapter file (`misaka.palw.model-adapter.v1`, a JSON document with a bounded, pure expression language)
    maps the class's keys onto features; no protocol change. The adapter is pinned by the BLAKE2b-512 hash of its
    effective canonical JSON, and the built-in pack by a pack hash, so a runtime pack names exactly which reader ran.
  - **C**: a capability is missing: a feature that cannot be lowered yet, or a protocol gap, **named, together with the
    smallest *general* primitive that would close it** — never a model-specific one. A Level C verdict is a statement about
    the vocabulary, not about the model.
- **P4. Static shapes, nothing ignored.** Every dimension is constant except the history length `H` (§3.1). A configuration
  key that no rule reads is refused (`NOT_LOWERABLE`), never ignored: a key that can change the math is understood or the
  model is refused.
- **P5. Selection is in the IR, not in the model's code.** `TopK` breaks ties to the lowest index and returns its set in
  index order (PALW-TIR-11, ADR-0052 B); routing variants (grouped, shared + routed, bias terms) are data over `TopK`,
  `Gather` and `Select`.
- **P6. State is declared.** Every recurrence and every cache is a state declaration of §4.1 (`Fixed` with its range, or
  `Hist` with its window), derived by the lowerer from the spec's mixers. Nothing about state is implicit in a kernel.
- **P7. The court sees only TIR nodes.** The court knows primitives, committed values, params and the primitive semantics. It
  never sees a model name, a feature, an adapter, a quantisation descriptor or a pack. Therefore **everything in Part II
  lies outside consensus** (§2): a class's identity is `H(graph_ir_root, layout, artifact_root, tokenizer)` (§4.1), and
  how the artifact was produced, or by whom, confers no protocol authority. The one exception is the lifecycle of §II.7,
  which is registry behaviour and is analysed, not changed, here.
- **P8. The legacy converter is frozen.** `qwen36-convert` serves the three architectures it already serves and points every
  other model at `palw-class check-architecture` and the TIR path (commit `0e5f56ac7`). New architectures are never
  added to it.
- **P9. A limit of the tool is not a verdict on the model.** A refusal states what the *tool* lacks (a feature, a
  descriptor, a primitive, a fence) and what exists instead. The preflight reports an unknown as unknown and a block as a
  block; it never turns "this build cannot" into "this model cannot" (ADR-0108's rule for verification depths).
- **P10. Reproducible by construction.** A conversion depends on nothing but the pack's inputs: no clock, no thread count,
  no platform libm (§II.3.3). Two builds from one pack give one artifact root.

## II.2 The header-only preflight (R1)

`palw-class preflight <input>` and `misaka model preflight <input>` answer, for a model nobody has downloaded yet, whether it can be
registered and what is missing. Both print the same report, human or `--json`.

### II.2.1 Inputs, and what is read

The command dispatches on the kind of its input and **says which mode ran in the first line of its output**.

| Input | Mode | Read | Never read |
| --- | --- | --- | --- |
| a directory with `config.json` (a Hugging Face snapshot, possibly with the weight files absent or sparse) | model | `config.json`; the presence and size of the tokenizer files (the tokenizer files are read whole: they are megabytes) and of `generation_config.json`, `preprocessor_config.json` and any `*mmproj*` file beside the checkpoint; `model.safetensors.index.json`; **the header of each safetensors shard** (8-byte length and the JSON header) | the data region of any shard |
| a `config.json` file | model | the same, beside it; shard headers if `--headers <dir>` supplies them (`*.safetensors` files or their header prefixes) | |
| a `.gguf` file | model | the magic, version, tensor count, the metadata key/values and the tensor infos (name, dimensions, type, offset) | the data section: the file may be truncated after the header, and the tool only uses the file size |
| a model repository id (cargo feature `remote`, off by default) | model | the same ranges by HTTP range requests, one request per header | everything else; no test uses the network |
| a `.palwtir` artifact | artifact admission | as today (`palw-class preflight` / `misaka model preflight <artifact>`) | |

A download that follows a preflight is therefore a *decision*, and the report says how many bytes it would fetch and how many
of them the class needs: a tensor the feature scope leaves out (§II.6) is listed with its size and need not be downloaded.

### II.2.2 Depths and the first line

The verification depths of ADR-0108 (Structural, Vectors, Full) apply, renamed for a model:

| Depth | Computes | Needs |
| --- | --- | --- |
| `headers` | architecture and features, scope, per-tensor storage types and their descriptors, the tensor and shape checks against the program's parameters, the artifact-size estimate; the convert stage's verdict | the files of §II.2.1 |
| `shape` (the default when a network is named) | adds the **shape-only lowering** (no weights: the program depends on the configuration, and the parameters' shapes come from the headers), the admission sizing, the chain's conditions at a height, the seat's resource needs and the lifecycle forecast; the register and mine verdicts | a network (`--network`, default `testnet-12`) and a height (`--height <DAA>`; default the node's tip when `--node` is given, else the first height at which every fence the network schedules is in force, and the choice is printed) |
| `full` | the artifact is built (or supplied) and verified: the existing artifact admission and `pack verify` | the weights and a pack |

The first line is of the form

```
preflight: model · Hugging Face directory · depth shape (requested shape) · testnet-12 at DAA 7,150 · 61 KiB of 17.6 GB read
```

or `preflight: artifact admission · .palwtir` for an artifact. When a depth stops early the report says `depth_reached`,
`depth_requested` and `stopped_at` (for example `depth_reached: headers, stopped_at: "network not given"`), and every verdict that
needs a deeper depth is `unknown`, never `ok`.

### II.2.3 What the shape depth computes

Each item names the function that answers it, so the preflight is a composition of existing, tested parts and invents no rule.

1. **The model.** `hf_schema::read_model_with` reads the configuration (and, when headers are given, the tensor names) into a
   `ModelSpec`; `model::analyze` gives the `ArchitectureReport`: the Level (A, B or C), the adapter (built-in, user file, none)
   and its hash, the features used with their lowering status, the assumed defaults and the refusals by named feature.
2. **The scope.** `model::scope_of` over the configuration, the tensor index and the files beside it (§II.6): what the class
   computes and what it leaves out, with evidence and the bytes the left-out tensors hold.
3. **The storage.** A histogram of the checkpoint's tensors by storage type (safetensors dtype; GGUF type id and name), each
   mapped to a descriptor of the registry (§II.5): described and supported; **no descriptor** (the id, the name the metadata
   gives it, the tensors that need it); known and not yet described (§II.5.6); or refused by the descriptor's own rule (for
   example LLM.int8 with an outlier threshold). Parameters a descriptor reads from `quantization_config` are resolved and
   its shape checks run on the header-only role tensors (plus the few small role tensors a descriptor reads for its own
   parameters, at most 4 KiB each).
4. **The tensors.** Every parameter of the lowered program is bound to a checkpoint tensor (or a served virtual tensor, §II.5.3)
   and its shape equals the header's; a missing tensor, a shape mismatch and a checkpoint tensor nothing reads (outside the
   scope's left-out parts) are reported before any download.
5. **The artifact.** The estimated size of the `PALWTIR1` container: the parameters by the quantisation policy (the integer
   width each weight lowers to, per-row scales, tables), the program, the tokenizer; and the *download* size, the bytes of the
   tensors the class needs.
6. **Admission.** `tir_admit_v1` (the function consensus admission calls) over the shape-only program: node counts against the
   caps, ranges, costs, court cones; the verdict is one of Part I §8's (`ADMISSIBLE`, `ADMISSIBLE_GENERIC`, `EXCEEDS(limit, value,
   cap)`, `NEEDS_PRIMITIVE(name)`, `NOT_LOWERABLE(reason)`).
7. **The chain's conditions at the height**, each as *needed against limit*:
   - **close sizes**: the worst carried close of every terminal tile (PALW-TIR-38, `palw_tir_close_size_v1`) against the carried cap;
   - **court cost** (`derive_court_cost_v1`): terminal MACs, operands and close chunks against 2^26 elements / terms and the
     per-tile ceilings of §5.4;
   - **the DA ladder**: the class's step leaves against the ladder in force — 2^22 below `palw_tir_fence2`, the class's own
     ladder, up to 2^30, past it;
   - **the court window**: the DAA the court needs to adjudicate the class (the dissection ladder's rungs, their deadlines and
     the close) against the window the network gives it (§II.2.6);
   - **the canonical job**: the declared prefill and decode token counts, `max_context` and the history bound against the
     network's canonical-job bounds and the class's profile;
   - **the fences**: every fence the class needs (`palw_tir_v1`, `palw_tir_fence2`, …) against what the network has armed at the
     height (`FENCE_NOT_ARMED(name)`), and the registration's price (the 1 BILI burn and the carrier fee).
8. **The seat.** The memory one seat needs to replay the class — the artifact (mapped) plus the replay working set, which
   grows with the context (the K/V history and the scratch of the widest cone) — against the memory share a seat declares
   (`--palw-host-memory-share`), and the time to page the artifact in. A seat whose share is below the replay never becomes
   ready and, today, says so nowhere (§II.7.3 F4); the preflight is where it is said first.
9. **The forecast.** The path the lifecycle (§II.7.1) would take on the network: an admission jury at the next audit
   (`palw_admission_audit_period_daa`), `required_ready_seats` ready seats, `probation_claims` probe claims, `stable_epochs`
   stable spans, how many independent operators the network has against the seating floor (§II.7.5), and the earliest time to the
   first paid claim. It is informational: the chain decides.

### II.2.4 Verdicts, stages and codes

The verdict is one table, one row per stage, in the order the user meets them:

| Stage | The question | Blocked by |
| --- | --- | --- |
| **convert** | Can an artifact be built from this source, reproducibly, by anyone? | architecture and features (Level C), config keys, quantisation, tensors, tokenizer, completeness of the source |
| **register** | Would the chain admit this class at this height? | admission sizing, the court's conditions, the court window, the canonical job, the fences |
| **mine** | If it is registered, will it be admitted to claims and paid? | seat resources, ready seats, the lifecycle path, conformance (full depth) |

Each stage is `ok`, `blocked` or `unknown` (depth too shallow). A blocker is
`{ stage, code, what, evidence, have, need, safe_paths }`; `have` and `need` are present when the condition is a count or a
size. Codes are stable once published; the initial set (`SCREAMING_SNAKE`, the registry's own `PalwModelRegistrationCodeV1`
style, and the registration codes it already has are reused where they mean the same):

| Stage | Codes |
| --- | --- |
| convert | `ARCH_NEEDS_FEATURE(feature)`, `ARCH_NEEDS_PRIMITIVE(name)`, `ARCH_REFUSED(feature, why)`, `CONFIG_KEY_UNREAD(key)`, `CONFIG_INVALID`, `REMOTE_CODE(module)`, `QUANT_NO_DESCRIPTOR(scheme, id, name, tensors)`, `QUANT_KNOWN_UNDESCRIBED(method)`, `QUANT_REFUSED(descriptor, why)`, `TENSOR_MISSING(name)`, `TENSOR_SHAPE(name, want, got)`, `TOKENIZER_MISSING`, `SOURCE_INCOMPLETE(file)` |
| register | `ADMISSION_EXCEEDS(limit, value, cap)`, `CLOSE_SIZE_OVER_CAP`, `COURT_COST_OVER_CEILING`, `DA_LADDER_EXCEEDED`, `COURT_WINDOW_EXCEEDED`, `CANONICAL_JOB_OUT_OF_BOUNDS`, `FENCE_NOT_ARMED(name)`, `ARTIFACT_ROOT_KNOWN` (full depth) |
| mine | `SEAT_MEMORY_SHORT`, `READY_SEATS_INSUFFICIENT` (a class already on the chain), `PACK_NOT_VERIFIED` (full depth) |

Not every note is a blocker. The feature scope (§II.6) is reported as a note, not a block: a vision-language model is registrable
as its text decoder, and the report says so (`text stage only on testnet-12; vision needs RFC-0003's generative class`).

### II.2.5 Safe paths

A refusal says what exists instead, taken from data rather than prose: for `QUANT_NO_DESCRIPTOR`, the model's original
safetensors, or re-quantising to a described type (listed), or supplying a descriptor file with `--quant-format` (§II.5.5); for
`ARCH_NEEDS_FEATURE`, the feature's name and the general primitive that would close it (lane G's `FeatureInfo`) and whether a
data adapter can supply a missing key mapping; for `COURT_WINDOW_EXCEEDED`, a smaller `max_context`, the class-specific window
where the build has it, and the numbers each would give; for `SEAT_MEMORY_SHORT`, the context at which the class fits the seat.

### II.2.6 Plug points

- **The court window.** The derivation of a class's court window arrives with release int-10 (lane R, `rcore/int-10`). Until a
  build has it, the preflight prints the *global* rule (`window_court` against the dissection depth the class needs) and says
  `court window: global rule; this build has no class-specific window`. The window is a provider (`needed`, `limit`,
  `source`) so that merging int-10 replaces one function.
- **The network.** Parameters (`Params`), fences and registration terms come from the named network preset or, with `--node`, from
  the node (`getPalwRegistrationTerms`), so a live chain's terms — which differ from genesis after a retarget — are the ones
  judged.
- **The seat share.** The default tiers are the testnet-12 fleet's: a memory share of 3.5 GiB (the 5.104 seats, MemoryMax 9 GiB) and 8 GiB (the ibm/.113 seats, MemoryMax 16–20 GiB); `--seat-share [name=]GiB` (repeatable) replaces them.

### II.2.7 The JSON form

`--json` prints `misaka.palw.preflight.v1`: the mode, depths, the network and height; `source` (files, sizes, SHA-256 of the
config and each header); `model` (architectures, Level, adapter id and hash, features); `scope`; `storage` (histogram, descriptors
by name and digest); `artifact` (estimates); `chain` (each condition with `needed`, `limit`, `source`); `seat`; `forecast`;
`verdict` (the stage table); and `registries` (the adapter-pack hash, the feature-registry digest, the quant-registry digest and
the tool's version), so a verdict is attributable to the build that gave it. It contains no timestamp and no path: the same
inputs give the same bytes.

### II.2.8 What a preflight does not claim

It does not claim fidelity (that needs the weights: §II.4's reference fit), it does not claim the artifact root (that needs the
data), and it does not claim that the chain *will* admit the class: it computes the chain's own functions at a height, and the
chain decides when the object is carried. It claims only what the headers and the rules say, and says which.

### II.2.9 As built (2026-10-01), and what this slice leaves out

`misaka_palw_sdk::preflight` is the library; `palw-class preflight <model>` and `misaka model preflight <model>` are two faces
of it (the same report, `--json` the same bytes). Where the build differs from, or decides within, the text above:

- **Dispatch.** A directory with `config.json`, a `config.json` file, a `.gguf` file (or a directory holding only a
  `model.gguf`) is a model; a `.palwtir` file is an artifact and keeps its old meaning in both commands (`palw-class preflight`
  runs the artifact admission; `misaka model preflight` asks the live chain). The first line says which ran.
- **Exit code.** 0 when no stage is blocked and the convert stage is `ok`, 2 when a blocker exists, 1 for an input the command
  cannot read.
- **Network and height.** `--network` defaults to `testnet-12` for a model; `--depth headers` needs none. With no `--height` the
  height is the last DAA `Params::fence_schedule_v1` lists (every fence the network schedules is in force there) and the report
  prints how it was chosen. `palw_tir_v1` is read from the network's real schedule: below its activation, or on a network that
  has not armed it, the register stage carries `FENCE_NOT_ARMED(palw_tir_v1)` and **every other condition is judged as if it
  were armed** (at the fence's own height, or DAA 1), so a model learns what else stands in its way before the flag day.
- **The declared context is searched.** A class is declared at a context; `declare-layout` defaults to the widest the program
  and the network admit, which no one can register (past 32,783 positions the canonical prompt exceeds J5b's 4,096 inline ids).
  The preflight, given no `--max-context`, declares the **widest context at which admission v10 admits the class** (the court
  window and the inline bound cap the search, then halving and bisection on the gate itself) and says so, with the refusal the
  program's own widest context meets. `--max-context N` judges exactly N.
- **Every wall at once.** The gate refuses at the first wall it meets; the preflight also computes the court window, the canonical
  job and the sizing on their own, and any that is over its limit is a blocker too, so fixing one wall is not mistaken for
  fixing the model (ADR-0097's rule). Each condition is `needed / limit / unit / ok`.
- **Shape-only lowering.** The program is the lowering of the configuration (`prepare_spec`, which reads no weights); the
  parameters' shapes are checked against the headers by the frontend's own binding (`check_weights`; `check_names` when only the
  index is present). A described format that derives a shape from a small role tensor (at most 4 KiB) whose data is not on disk
  is reported as `not checkable` with the tensor to fetch, never as a wrong shape.
- **The artifact estimate** is `palw_tir_work_shape_v1(program).param_bytes()` — the registry's own figure for the class's
  parameters — plus the program and the tokenizer files; the download figure is the tensors the class reads, minus what the
  scope leaves out. The inventory's leaf count is an estimate (it only sets the depth of a close's paths).
- **The seat.** `needed = artifact (mapped) + state at the declared context + one position's peak live bytes + the widest tile a
  close opens`; the report gives the need against each seat tier (the fleet's two, or `--seat-share`) and says which tiers hold the class and which never become ready for it; `SEAT_MEMORY_SHORT` is raised only when no tier holds it, with the context at which each fits; a class only some tiers hold is a note.
- **The forecast** is `palw_derive_profile_v1` over `palw_tir_model_work_v2`: the registry's own derivation of the window,
  prefetch, ready seats, in-flight claims and registration bond. It does not know how many independent operators the network has.
- **Tokenizer.** `TOKENIZER_MISSING` is raised for a directory (no `tokenizer.json`, `tokenizer.model` or `vocab.json`) and for a
  GGUF without `tokenizer.ggml.tokens`; a lone `config.json` says nothing about it.

**Not in this slice**, each with where it plugs in: `--node` (the live chain's registration terms and tip, and with them
`READY_SEATS_INSUFFICIENT` for a class already on the chain); a repository id by HTTP ranges (the `remote` feature of the
lowerer's `RemoteCheckpoint` is the reader; no network is used by any test); the `full` depth, which reports what it needs
(`palw-tir-fidelity`, `pack build`, `pack verify`) and `PACK_NOT_VERIFIED` / `ARTIFACT_ROOT_KNOWN`; and the class-specific court
window, which replaces one function (`window_at` in `preflight::chain`) when release int-10 merges.

## II.3 The streaming loader and the converter (R2)

A conversion that must hold a checkpoint in memory cannot be run on a 360 GB model, so the converter is built around one rule:
**the resident memory of a conversion is a block of a tensor, never a tensor and never the checkpoint.**

### II.3.1 `TensorSource`

The loader's face is a trait (`weights::TensorSource`). Beyond reading a whole tensor it serves the three things streaming needs:
`metadata(name)` (a header lookup, no data), `read_slice(name, range)` (a byte range of the tensor's stored data) and
`load_rows(name, rows)` (a row range decoded to `f32`); `serves_row_ranges()` says whether the last reads only what it is asked
for. The sources are:

| Source | Serves ranges by |
| --- | --- |
| sharded safetensors (`Checkpoint`: `model.safetensors.index.json` and the shards) | `pread` of the shard file at the header's offsets |
| GGUF | the header's data offsets; block-quantised tensors decode through their descriptor (§II.5) |
| a *described* source (`DescribedSource`) | a wrapper that hides tensors a descriptor stores packed and serves the float tensors it declares in their place (§II.5.3), decoding any element range without materialising the rest |
| remote (`CurlFetcher`, cargo feature `remote`, off by default) | HTTP range requests; the library never opens a connection by itself and no test does; the feature is tested against a loopback server |

### II.3.2 Streaming materialisation and the converter

- **By blocks of rows.** `materialise_stream` produces each parameter of the program a block of rows at a time, evaluating a
  source row-wise where its form allows (a slice, a transpose, a stack of experts); projections whose input has outlier channels
  split their rows by the calibration statistics in the same pass. What a fill cannot evaluate row-wise (a few small tensors) is
  read whole, and the loader counts them.
- **Content-addressed chunks.** The container (`PALWTIR1`) is assembled from canonical 4 MiB chunks in a chunk store, streamed:
  the same bytes are the same chunk. A composite class (a parent and an unmerged LoRA adapter, RFC-0004) therefore *shares its
  parent's chunks*, and an updated adapter rewrites only its own.
- **The inventory root by leaf.** The inventory root is computed leaf by leaf over a container or a chunk store (`derive_streamed`),
  equal to the consensus root; a 74 MiB container is rooted with 4 MiB resident.
- **Calibration** produces the activation statistics (`misaka.palw.calib-stats.v1`) the quantisation policy needs. They are a
  bit-exact file (a decimal does not round-trip through every JSON reader), a pinned input of the pack (§II.4), and re-measurable
  from the calibration sequences the pack carries.
- **`convert_model(ConvertRequest) -> ConvertOutcome`** is the converter as a function; `palw-tir-convert` and `palw-class pack
  build` are thin wrappers. The container's meta records what read the model — the frontend, the scope, the descriptors, the math
  — and enters the file digest, never the roots.

### II.3.3 Reproducibility

A conversion is a function of the pack's inputs and of nothing else:

- **Math.** Every transcendental the lowerer evaluates (activation and RoPE tables, scales, query temperatures, SSM decay) and
  the log2 of its scale arithmetic come from `detmath`: the pinned pure-Rust `libm` 0.2.8 (`math: "libm-v1"`, the default of a
  new pack), so a seat on any platform rebuilds the same root. A class registered before it records `math: "std"` and the
  platform it was built on (os/arch), and stays reproducible there. Measured on the 68 fixtures on macOS arm64 (`std` against
  `libm-v1`, one set of statistics): **0 of 14,688,768** activation-table entries and **0 of 4,141,056** RoPE-table entries
  differ; 2,146 of 6,577,152 entries of every other tensor differ, all in the query-temperature table of Llama-4, Llama-4-VLM
  and Ministral-3, by at most 8 codes (one binary32 ulp of `ln_1p` at Q24). The platforms' `f64` libms agree on every table
  here; their binary32 ones do not, and another platform's `f64` is untested, which is why the math is pinned.
- **Threads.** The conversion uses rayon; `tests/determinism.rs` rebuilds nine families (RoPE variants, hybrid, SSM, MoE,
  LongRoPE, YaRN) under pools of 1 and 7 threads, and the statistics and container bytes are equal.
- **Equality with the whole-tensor path.** The streamed artifact is byte-identical to `materialise` on all 68 Hugging Face
  fixtures, and `tests/golden_lowering.rs` pins the program and artifact digests of 92 checkpoints and 63 real configurations: a
  change in the lowering that moves a root is noticed by that test, and `LOWERING_VERSION_V1` is bumped when it is meant.
- **Memory.** `tests/streaming_budget.rs` converts a synthetic sharded Llama-shaped checkpoint (default 24 layers, about 270 MB of
  BF16 in many shards; `TIR_BUDGET_LAYERS=100` is a 2 GB-disk run) under a counting allocator: the peak above the process's own
  memory stays inside a budget (default 32 MiB) that is below the embedding table alone as `f32`, and no tensor deferred to the
  streaming path was loaded whole.

The known limits of this section (the float reference of calibration is not row-streamed; conformance on a very large model
loads the artifact whole on each executor) are in §II.9.

## II.4 Runtime packs (R2)

A **runtime pack** (`misaka.palw.runtime-pack.v1`) is the one document that says what is needed to rebuild a class's artifact
from public source and to judge the result. It is a directory: `pack.json` and the sidecars it pins by hash (the calibration
statistics, the calibration tokens, the Hugging Face reference logits, a user-supplied adapter, user-supplied descriptors).

### II.4.1 Identity and form

- **Identity.** `pack_digest` is BLAKE2b-256, keyed (`misaka.palw.runtime-pack.v1`), over the canonical JSON of `pack.json` (keys
  sorted, compact, integral floats as integers). The file may be pretty-printed; the digest does not depend on it. Nothing in the
  manifest depends on the machine, the time or the thread count.
- **Numbers.** Every number that is not an integer — the quantisation headroom, the logit scale, the tolerances, the measured
  fit — is stored as the 16 hexadecimal digits of its IEEE-754 binary64 bits, because a decimal does not round-trip through
  every JSON reader (serde_json's default parser is one ulp off on some values) and a digest must not depend on the reader.
- **Strictness.** Unknown fields are an error (`deny_unknown_fields`); a sidecar named by a path with `..` or an absolute path is
  refused; hash lengths and character sets are checked before anything is trusted.

### II.4.2 The sections

| Section | Pins | Why it is there |
| --- | --- | --- |
| `model` | the source: format (`safetensors` / `gguf`), label and revision (informational), architectures, `model_type` (informational), the digest of the configuration the frontend read, and every file's size and SHA-256 | the source is public, so anyone can fetch it and the hashes say it is the same bytes |
| `frontend` | the adapter (`none` for Level A, `built-in` or `user-file`, by id and BLAKE2b-512 hash; the template for Level A), the built-in pack's hash, the digest of the `ModelSpec` the lowering started from, the Level, and the configuration keys the class defaults supplied | a different reader is a different pack |
| `features` | the digest of the feature vocabulary of the build, the features used (`{id, layers, detail}`), and the scope (§II.6) | what the class computes and what it leaves out |
| `quant` | the descriptors, by name and digest (built-in, or a file inside the pack) | the weights are read the way the descriptor says (§II.5) |
| `profile` | the quantisation policy (headrooms), the maximum window, the context, the calibration (statistics digest and file, where measured, optionally the sequences) | the numbers that decide the integer program |
| `converter` | name, crate version, the lowering version (`misaka.palw.lowering.v1`), the math (`libm-v1`, or `std` with its platform) | the tool and the arithmetic |
| `executor` | the program format, `prim_set_id`, and the implementations that must agree byte for byte | what "correct" means (§II.4.4) |
| `logits` | the convention (`legacy-greedy-only`, or `q24-natural-v1`: code / 2^24 are natural-log logits, the scale exactly 2^-24), the scale and the tolerance (slope, correlation, top-1, KL) | what the integer logits are in |
| `result` | the container's file digest and size, `graph_ir_root`, the inventory root, the leaf count, the tokenizer id, the program size | what the build produced |
| `conformance` | the vectors: a prompt, the greedily decoded tokens, the number of positions, and two digests — over every position's logits and over every commit point | what any executor must reproduce |
| `hf_reference` | the Hugging Face reference (its file and digest, what produced it, sizes) and the fit the integer program measured against it (slope, intercept, correlation, max-abs, RMSE, top-1, KL) | the fidelity claim (never a bit-identity claim: Part I §1.2(5)) |
| `declared` | per network: the layout digest, the class id the layout gives, `max_context`, checkpoint interval, `h_tile`, and the file digest of the container written with it | the class the artifact becomes on that network |
| `files` | the sidecars, by BLAKE2b-256 | |

Adding a model never inherits another model's conditions because **there is nowhere to inherit them from**: every condition that
shapes the artifact — adapter, descriptors, policy, statistics, math, executors — is a field of this model's pack and so of its
digest, and verifying a pack reads nothing from any other.

### II.4.3 The commands

```
palw-class pack build  --model <hf dir | .gguf> --out <artifact.palwtir> --pack <dir> (--calib <tokens.json> | --stats-in <stats.json>) …
palw-class pack verify <pack dir> [--model <dir | .gguf>] [--artifact <file>] [--rebuild] [--no-ref2] [--no-exec] [--strict] [--json]
palw-class pack show   <pack dir> [--json]
```

`build` converts (streaming, `libm-v1` by default), writes the artifact and the pack, runs the conformance vectors on every
executor, holds the program to the reference within the tolerance, and, per `--declare <network>`, declares the class.
`verify` checks every claim it can from what is at hand and reports **PASS**, **FAIL** or **SKIPPED** (not checkable here, and
why) for each: the manifest and sidecars by hash; the source files by hash; the adapter and the descriptors by hash; the
frontend by reading the model again (the spec digest, the features, the scope); the artifact, rebuilt from the source with the
pack's profile and statistics (`--rebuild`) or the one supplied (`--artifact`), by file digest, inventory root, graph root and
tokenizer; the declared classes by class id; the conformance vectors on every executor; and the program against the Hugging Face
reference within the tolerance. A pack is **VERIFIED** only when nothing failed and nothing was skipped.

### II.4.4 Independent conformance

The conformance vectors are the independent check of A2. Each is run on three implementations, which must give the same bytes at
every position's logits and at every commit point `(slot, block, layer, node, values)`:

1. the reference evaluator (`misaka-palw-tir`, Part I §6: the meaning of "correct");
2. the typed backend that nodes run (`misaka-palw-tir-exec`);
3. the independent second implementation (`misaka-palw-tir-ref2`), written from the specification alone, decoding the canonical
   bytes with its own codec.

The Hugging Face reference fit is a *unit check* as much as a fidelity measure: the least-squares slope of the reference logits on
the program's `code × scale` must lie in `[slope_min, slope_max]`, which fails a program whose logits are in other units than
natural-log logits (a slope far from 1).

### II.4.5 What a pack is not

A pack is not a chain object and no consensus code reads it (P7). It is the evidence a person gives so that others can reproduce
a class without trusting them; the chain's evidence is the court. A registration's artifact root is the same value whether or not
a pack exists; the pack is what lets a seat, an auditor or a competing registrant rebuild that root and check that the executors
agree before spending a fee or a gigabyte on it.

## II.5 Quantisation as data (R3)

A model's weights reach the lowerer in whatever storage format its publisher chose. Each format used to be a Rust decoder, one
more release per format. They are now **descriptors**: JSON files (`misaka.palw.quant-format.v1`, `quant-formats/*.json`) that say
where a tensor's fields are, which tables it indexes and, as expressions, what each element is, read by **one** interpreter
(`quantfmt`). The registry refuses what it does not describe, by name; a new format is a file with test vectors and no change
to the crate.

### II.5.1 The descriptor

**Status: draft.** Additive changes are allowed until the first runtime pack is published; then the schema freezes and a later
change is `quant-format.v2`. The identity of a descriptor is the BLAKE2b-256 digest of its canonical JSON (the text's layout does
not matter).

```jsonc
{
  "schema": "misaka.palw.quant-format.v1", "name": "BNB_NF4", "doc": "…",
  "ids":    [ { "scheme": "ggml", "id": 2 },                                            // a GGUF tensor type
              { "scheme": "config", "method": "bitsandbytes",                           // a quantization_config
                "when": [ { "path": "load_in_4bit", "one_of": [true] },                 // …told apart by other keys
                          { "path": "bnb_4bit_quant_type", "one_of": ["nf4"] } ] } ],
  "params": { "bits": { "config": "bits", "default": 4, "allowed": [2, 4, 8] },         // from the configuration
              "bs":   { "from_role": { "role": "qstate", "path": "blocksize" } } },     // or from a tensor's JSON document
  "config": { "inert": [ … ], "skip": "llm_int8_skip_modules", "skip_match": "path",
              "lm_head": "unless_skipped", "checks": [ { "path": …, "one_of": [ … ], "message": "…" } ] },
  "tables": { "name": { "bits": 8, "signed": true, "hex": "…" } },                      // constants the format indexes
  "layout": { "kind": "blocks" | "tensors" | "virtual", … },
  "decode": { "target": "integers" | "floats" | "tensor", … },
  "tests":  [ { …a vector… } ]                                                           // mandatory
}
```

**Vectors are mandatory and are checked when a descriptor is loaded.** A vector is a valid random input and the expected output as
computed by an *independent* implementation; `QuantFormat::from_json` decodes every vector and refuses the descriptor if one
element differs by a bit. A descriptor that fails its own vectors does not enter the registry.

### II.5.2 The three layouts

| Layout | A unit is | Decodes to | Used for |
| --- | --- | --- | --- |
| `blocks` | a block of `elems` elements in `bytes` bytes, fields at byte offsets (`u8`, `f16`, …) | the stored integers under group scales (`q`, `scale`, `zero`, `min`, code range, offset term) or floats | GGUF's ggml types |
| `tensors` | a *module's* tensors, each a **role** (a name, a suffix, accepted dtypes, a rank, required or not) | the stored integers under group scales (`group.size`, `group.index`, `q`, `scale`, `zero`, `min`, `code`, `order`, `offset_term`) or floats (`value`) | GPTQ, AWQ, FP8, compressed-tensors, bitsandbytes |
| `virtual` | tensors a checkpoint stores packed, **served as float tensors under another name**, of any rank up to four (the first lane may be an expert axis) | a tensor: `axes`, a `shape` expression per axis, and one `value` expression | MXFP4 as Hugging Face stores it (`<name>_blocks` + `<name>_scales` served as `<name>`) |

A target that is `integers` is *lowered from the stored integers* (nothing is re-quantised: the codes are the weights, and each
row's group scales become exact integers at the row's unit, `lower/qlinear.rs`); a target that is `floats` or `tensor` decodes to
floats and the weight takes the ordinary W8 path of Part I's lowerer. Which a format is follows from what its elements are, not
from how it is stored: a codebook (NF4, FP4, FP8) is floats; an integer code under a scale is integers.

### II.5.3 Virtual tensors

A `virtual` layout is the answer to a format whose stored tensors are not the tensors the model uses. `DescribedSource` wraps a
checkpoint, lists the *served* names instead of the packed ones, answers a shape from the role tensors' headers, and evaluates any
element range of the served tensor without materialising the rest. OCP MXFP4 as Hugging Face stores it (gpt-oss) is one line:

```
roles  blocks  <name>_blocks  U8 [E, out, in/32, 16]      scales  <name>_scales  U8 [E, out, in/32]
axes   e, i, o          shape  [dim_blocks[0], dim_blocks[2] * 32, dim_blocks[1]]
value  fp4x2[(blocks[e, o, i / 32, (i % 32) / 2] >> (4 * (i % 2))) & 15] * 0.5 * e8m0(scales[e, o, i / 32])
```

A leading expert axis and per-block scales over a 3-D tensor are a lane `e` and a read `scales[e, o, i / 32]`: nothing in it is
specific to a model, and every binding that reads the float export reads the served tensor unchanged.

### II.5.4 The expression language

Closed, total and deterministic; the whole of what a descriptor can say (`quantfmt/expr.rs`).

- **Values.** Integers are `i64` and every operation is checked (an overflow, a division by zero, a shift out of range is an
  error, never a wrap; `/` and `%` truncate toward zero, as C). Floats are IEEE `f64` with `+ − × ÷` only and every result must
  be finite. There is no loop and no recursion, so every expression ends, and no transcendental function, so a decoded value does
  not depend on a platform's libm.
- **Grammar.** `c ? a : b`, `|| && | ^ &`, comparisons, shifts, `+ - * / %`, unary `- ~ !`, `name[i, j]`, `f(a, b)`, literals. A ternary
  evaluates each arm only for the lanes that take it, so an out-of-range read in the arm not taken is never an error.
- **Names.** the *lanes* (`e`, `i`, `o`, `g`, `blk`, …: columns evaluated a batch at a time), the format's *parameters*, a block's
  *fields* or a tensor's *roles* read by index (`qweight[i / 8, o]`), `dim_<role>[axis]` (a role's dimension, known from its header
  alone), `has_<role>` (an optional role is present), `float_<role>` (a role is stored in a floating dtype, which tells apart a
  role that accepts bytes or floats), and *tables*.
- **Functions.** `float int abs min max floor sext bits pow2`; the small float formats' decoders `f16 bf16 f32 e8m0 fp8e4m3
  fp8e5m2`; and the **rounding functions** `rnd32 rndbf16 rndf16 rnd(x, kind)` (to nearest even to binary32 / bfloat16 / binary16;
  `kind` 0, 1, 2, a constant a descriptor can take from the checkpoint), by which a descriptor reproduces a library's float32
  arithmetic *exactly*: a float32 `+ − × ÷` is the correctly rounded float32 result of the float64 operation on float32 operands
  (a format with `p' ≥ 2p + 2` significand bits, here 53 ≥ 50, does not double-round), so `rnd(a * b, 0)` is bit-for-bit a float32
  multiplication.
- **Parameters read from a tensor.** A parameter may be read from a role that holds a UTF-8 JSON document (`from_role`: a dotted
  path, an `int`, `float` or `string` through the parameter's `map`, a default for an absent key). The JSON is read by an exact
  reader that keeps numbers as text, so a decimal is parsed once and correctly rounded. bitsandbytes stores each module's logical
  shape, block size, dtype and double-quantisation offset this way.

### II.5.5 Reading `quantization_config`

Every top-level key of the configuration is *read* (a parameter's `config` path), declared *inert* (it does not change what the
stored tensors mean), named by a *check*, or refused by name: a key the descriptor does not know may change the meaning, so a model
that carries one is refused with the key's name. `skip` names the key that lists the modules kept in float, and `skip_match` how
an entry names a module — `exact`, `contains`, `path` (transformers' bitsandbytes rule: the module's last component, its whole
name, or a path prefix) or `regex` (the subset `^ $ . .*` and escapes, which is what Hugging Face's `re.match` entries use; anything
else is refused); `lm_head` says whether the head is stored in the format. A `quant_method` can announce several descriptors,
told apart by `when` conditions on other keys (bitsandbytes: `load_in_4bit`, `bnb_4bit_quant_type`, `load_in_8bit`); another
descriptor under the same conditions is refused, one under others is added.

### II.5.6 Refusals, and what is known and not yet described

An unknown type is never misread. A GGUF type no descriptor describes, a `quant_method` nothing reads, or a configuration a
descriptor refuses is a refusal that names it (the scheme, the type id, the name the metadata gives it, how many tensors and which
need it), lists what *is* described, and gives the safe paths: use the model's original safetensors, re-quantise it to a described
type, or supply a descriptor file (`--quant-format <file>`, or a pack's `quant.descriptors`) — no code change is needed.

`quant-formats/known-undescribed.json` lists the methods that are known, so that nobody mistakes a refusal for a verdict on the
model: HQQ, AQLM, VPTQ, SpQR, EETQ, FBGEMM-FP8 and BitNet (descriptors are expressible in principle; none written), QuIP/QuIP# and
HIGGS (the weights are decoded and then rotated by Hadamard transforms at run time, which is not a static decode; refused until a
transform feature exists), torchao (tensors serialised as Python objects, not safetensors tensors: refused, not described). The
preflight prints the entry beside the refusal.

### II.5.7 The descriptors this build carries, and their evidence

39 built-in descriptors, each with vectors:

| Descriptors | Layout, target | The vectors' reference |
| --- | --- | --- |
| 29 ggml types: `F32 F16 BF16 F64`, `Q4_0 Q4_1 Q5_0 Q5_1 Q8_0`, `Q2_K Q3_K Q4_K Q5_K Q6_K`, `IQ4_NL IQ4_XS`, `TQ1_0 TQ2_0`, `MXFP4 NVFP4`, `Q1_0 Q2_0`, `IQ2_XXS IQ2_XS IQ2_S IQ3_XXS IQ3_S IQ1_S IQ1_M` | `blocks`; floats for the four plain types, integers (grid codebooks are constant tables) for the rest | gguf-py's own numpy dequantisers on 8,700 random blocks (`Q1_0`, `Q2_0`: a numpy transcription of the C); the tables are parsed out of `ggml-common.h`, not typed |
| `GPTQ` (v1 and v2, 2/4/8 bits, act-order, per-row) | `tensors`, integers | a torch re-implementation of AutoGPTQ's dequantisation |
| `AWQ` (GEMM, 4-bit) | `tensors`, integers | a torch re-implementation of AutoAWQ's `dequantize_gemm` |
| `FP8_BLOCK` (DeepSeek-V3, Qwen3-FP8) | `tensors`, floats | the reference `weight_dequant` over torch's float8 |
| `CT_PACK_QUANTIZED`, `CT_FP8_CHANNEL`, `CT_INT8_CHANNEL` (compressed-tensors) | `tensors`, integers / floats / integers | a torch re-implementation of compressed-tensors' `unpack_from_int32` and `dequantize` |
| `MXFP4_HF` (gpt-oss) | `virtual`, tensor | `transformers.integrations.mxfp4.convert_moe_packed_tensors`, the library's own function |
| `BNB_NF4`, `BNB_FP4` (also double-quantised), `BNB_INT8` | `tensors`: floats, floats, integers | `tools/bnb_ref.py`, a numpy/torch re-implementation of bitsandbytes' stored format and of its CUDA kernel's arithmetic |

**Evidence, stated plainly.** Only the ggml types (gguf-py) and `MXFP4_HF` (transformers) are checked against the maintainers' own
code. For GPTQ, AWQ, compressed-tensors and bitsandbytes the quantisation libraries are not installed in the offline environment
and the vectors come from re-implementations of their documented behaviour and source. The end-to-end fixtures (a tiny random
decoder quantised in numpy per each format's specification, `tools/gen_quant_fixtures.py`, 33 fixtures) are checked against
`transformers` with the dequantised weights substituted; that checks the *lowering* of a format, not the library's output. A
first checkpoint from the real quantiser belongs in the corpus (§II.8) and would settle it.

### II.5.8 What is exact, and what the formats cost

- **Integer targets are exact in their integers.** The lowering carries the stored codes as they are (nothing is re-quantised); a
  row's group scales become integers at one unit per row (`|a| ≤ 2^20`). An fp16 scale has 11 significant bits, so every scale
  within 2^9 of its row's largest is exact there (every checkpoint seen so far). A float32 scale (bitsandbytes' `SCB`) has 24, so it
  is rounded at 2^-20 of its row's largest; any scale that is not exact is counted (`quant_inexact`), a relative error of at most
  one part in a million in that scale.
- **A 4-bit code costs a byte in v1** (the program's weight dtypes are `i8`/`i16`); a packed `i4` dtype is a v2 candidate in
  `freeze-v1.md`. A 4-bit checkpoint's artifact is therefore about twice its packed size.
- **Float targets take the W8 path**: the dequantised weights are quantised per row by the lowerer's policy. For a format whose rows
  were quantised row-wise by absmax this recovers the same integers; for a codebook format (NF4, FP4, FP8) it is a re-quantisation,
  measured by the pack's reference fit.
- **bitsandbytes, in detail.** The 4-bit weight is a `uint8 [⌈N/2⌉, 1]` of the flattened row-major weight, the even element in the
  high nibble, with one `absmax` per `blocksize` flat elements and a 16-entry code table *stored in the checkpoint*, so the format
  is table-agnostic. **Double quantisation is exact**: the nested absmax is `nested_quant_map[absmax] · nested_absmax[block / 256] +
  nested_offset`, two float32 operations, each a `rnd(·, 0)` of a float64 operation on float32 operands. An element is the float32
  product of its code and its absmax, rounded once to the module's dtype (bitsandbytes' CUDA kernel's arithmetic; its pure-PyTorch
  fallback rounds the code table to that dtype first and differs by up to one bfloat16 ulp). For FP4 the kernel's literal for code 1
  (0.00520833) differs from the stored table's (0.0625 / 12 in float32) in the seventh digit; the descriptor follows the stored
  table. **Refused by name**: `bnb_4bit_quant_storage` other than `uint8`, and a hardware-reordered int8 `weight_format`.
  **`llm_int8_threshold` is inert (decided 2026-10-01).** Above 0, LLM.int8 splits every matmul at run time (activation columns past
  the threshold go through float16 against the dequantised weight columns, the rest through int8); that is how bitsandbytes *runs* a
  layer, not what is stored, and `BitsAndBytesConfig`'s default is 6.0, so most int8 checkpoints published carry it. The stored
  weights define the model, whatever the threshold. The pack's fidelity measure against the Hugging Face reference (§II.4.4) says how
  far the integer program is from the library's own run, and it must still pass; the `SCB` rounding stays counted
  (`quant_inexact`).

### II.5.9 Adding a format

1. Write the descriptor: `ids`, `params` and `config` (what the configuration says), `layout` and `decode`.
2. Write an *independent* reference for the vectors (the library's own function where it can be imported; else a transcription from
   its source, said so in the descriptor's `doc`), generate random valid inputs and the expected values, and add them as `tests`.
3. `QuantFormat::from_json` checks the vectors; the registry refuses a redefinition of a type, a method or conditions it holds.
4. Supply it with `--quant-format <file>` (or a pack's `quant.descriptors`) for a conversion that needs it; promote it to
   `quant-formats/` and the built-in list when it is general.

No code change is needed unless the language itself cannot say the format; a gap in the language is a general addition (`from_role`
and `float_<role>` and `rnd` were added for bitsandbytes, `virtual` for MXFP4), tested once, and available to every later format.

## II.6 Feature scope (R4)

A registered class is **one function**: token ids in, next-token logits out (for an encoder, a pooled embedding). A model that also
takes images or audio, or proposes several tokens at once, has parts the class leaves out. The *scope* says which, with the
evidence and the reason, wherever the class is shown, so that a text-only class is never presented as the whole model — and never
as a partly multimodal one: **a class computes a part entirely or says that it does not.**

- **The object** (`misaka.palw.feature-scope.v1`): `task` (`text-generation` or `text-embedding`), `input`, `output`, `text_only`
  (the model has a modality the class does not take), and `excluded`: for each left-out part its `kind`, `what`, `why`, whether it
  is `modal` (an input the class cannot take, as against a head it can do without), the `evidence` that showed it
  (`config.<key>`, `tensors <prefix>*`, `file <name>`) and the tensors and bytes it holds.
- **The rules are data** (`scope/rules.v1.json`, `misaka.palw.scope-rules.v1`): configuration keys, tensor-name prefixes or
  substrings and file names beside the checkpoint that are evidence of a part. Nothing selects code by model name. The kinds are
  `vision` (the tower; `vision_config`, `image_token_id`, …; `*mmproj*.gguf`, `preprocessor_config.json`), `projector` (the module
  that maps vision or audio features into the decoder's space), `audio`, `mtp` (multi-token-prediction heads) and `draft`
  (a speculative-decoding draft network, Medusa/Eagle heads).
- **Where it is shown.** The architecture report; the container's provenance meta; the runtime pack's `features.scope`
  (§II.4.2); `misaka model inspect`; the listing; and the preflight (§II.2.3 item 2), with the bytes of the tensors the class does
  not need, which a download may skip.
- **What it says for a vision-language model.** *Text stage only on testnet-12; vision needs RFC-0003's generative class
  (`palw_gen_v1`).* The class is the text decoder (RFC-0003 §II.2.1 specifies the placement of image rows an LM reads); the tower
  and the projector are named, and an `mmproj` file is an unsupported feature, not a partial one. A user who needs the modality
  sees at once that this class cannot give it.
- **It is a note, not a block.** A vision-language model is registrable as its text decoder; the report says what it is and what
  it is not. It becomes a block only where a user requires a modality (`preflight --require vision`).

## II.7 Staged enablement: convert, register, mine (R5)

The requirement is that **registering a model and enabling mining for it are different acts**, and that when either does not
happen the user is told which stage lacked what. §II.7.1 states the lifecycle the chain has today; §II.7.2 what it already
guarantees; §II.7.3 where it falls short of R5 (found by reading the code, with the functions named); §II.7.4 the node-only change
that makes every stage visible; §II.7.5 the consensus rules that were decided to close what is missing.

### II.7.1 The lifecycle today

`PalwModelLifecycleV1` (`palw_model_registry_v1.rs`) is stepped once per span by the registry fold from chain-visible facts
(`step_model_registry`, `palw_lifecycle_step_v1`). On testnet-12 a panel has 5 seats, `required_ready_seats` is the panel plus
2 spare, 7 (`palw_seat_gate_possession`), and the audit period is 100 DAA.

| State | Admits claims | Leaves when | The stage that lacks something |
| --- | --- | --- | --- |
| `Registered` | no | the manifest verdict is `Valid` (the graph names only operations the VM defines and derives work) | **convert** — the artifact's graph is the problem |
| `Candidate` (a bought class, past `palw_admission_independence`) | no | an admission jury seats: `seat_count` operators drawn from the *network's* population, not the class's, excluding the registrant's bond and operator, and a majority of them hold the class ready (ADR-0147); one audit per period | **register** — the network has not admitted it |
| `Prefetching` (a genesis class starts here) | no | `ready_seats ≥ required_ready_seats` and the collateral is there | **mine** — possession |
| `Probation { probes_passed }` | 50 ‰ of the derived admission | `probation_claims` (10) probe claims reach `Final` with none failing (a failure resets it; past `palw_registry_resilience` only two distinct bonds' failures do) | **mine** — verified service |
| `ActiveLimited { stable_epochs }` | 100 ‰ | `stable_epochs` (3) stable spans and the class's cap utilization under the ceiling | **mine** — stability |
| `Active` | 1,000 ‰ | falls to `ActiveLimited` if the cap is exceeded, to `Held` if the panel cannot be drawn or the class overloads | — |
| `Held` | no | the seats are back and the class is not overloaded; it re-enters `Probation` | **mine** — seats or the receipt window |

A *ready seat* is a bond with a fresh readiness row: a possession proof of a challenge leaf of the artifact's inventory that rotates
every span (readiness V2; 24 spans on testnet-12), an active bond above the floor, and free collateral for the readiness multiple
(3 seat exposures). One predicate, `palw_seat_not_ready_reason_net_v1`, counts ready seats for the lifecycle, the jury's votes, the
panel room and the registry RPC.

### II.7.2 What it already guarantees

- **Registration is existence.** `Registered`, `Candidate` and `Prefetching` admit no claim. The claim gate
  (`class_lifecycle_refusal`) refuses a claim to a class whose state does not admit claims, the model market follows the same
  gate, and the producer asks it before it spends an inference. A class nobody serves earns nothing and carries no weight.
- **Static court feasibility is checked at registration.** Admission v10 checks types, ranges, costs and court cones (Part I §5); existing weight rules also require a certified family covering the program's primitives (`NotEndToEndCertified`; `palw_uncertified_weightless`). These are the existing gate's scope, not proof that a fresh public bond can acquire evidence and prosecute every admitted profile. Future reward/weight admission additionally requires RFC14's public-verifier dispute-completeness gates under ADR173.
- **Independence, once.** A bought class passes a jury of operators the network drew (ADR-0147), at one audit per period, so a
  registrant is admitted alone only with the probability that its share of the operator lottery wins a majority of the jury.
- **Possession.** Ready seats hold fresh possession proofs with collateral behind them.
- **Volume.** A new class is admitted at 50 ‰ and then 100 ‰ of its derived admission before it is `Active`.

### II.7.3 Where it falls short of R5

**F1. "Ready seats from distinct operators" is a count of bonds after `Candidate`.** The registry fold observes `ready_seats` as the
number of *bonds* with a ready row (`model_registry_ready_seats`), and `Prefetching → Probation` needs it to reach
`required_ready_seats`. Distinctness of operators is enforced in one place only: the admission jury. On testnet-12,
`palw_operator_id_unique` ("one operator identity, one bond", with a proof of possession of the operator key in the registration)
makes a bond and an operator identity one thing, so the bond count there *is* a count of distinct operator identities. Three
things remain. (i) On a network without that fence the lifecycle over-counts, while the panel room's own count
(`palw_panel_ready_eff_of_bonds_v1`) already groups by operator and caps weights: the two disagree about what a ready seat is. (ii)
The jury's independence is a snapshot at one audit: after `Candidate` nothing asks again whether the ready seats are anyone but the
registrant's own keys, so a registrant that holds seven bonds, each a distinct key, keeps its class in `Probation` alone once its
jury has once been seated and the outsiders' readiness has lapsed (24 spans). (iii) The chain cannot tell two keys of one party from
two parties: independence is economic (the collateral behind each key), never an identity. And the rule is now **two rules**: lane D's
RFC-0003 lane admits a tensor claim only once `seat_count` distinct operators, never the executor's, hold the class with a fresh readiness
V2 proof (`GenClassNotReady`, RFC-0003 §I.4.5, `rfc3/gen-claim`), while an attempt class counts bonds and a composite class follows the
attempt lane. How many *independent* operators should gate the first claims, and one rule for every class kind, is §II.7.5 Proposal A.

**F2. Probe claims are paid.** `Probation` admits claims at 50 ‰ of the derived admission and a probe *is* an ordinary attempt claim:
one that reaches `Final` is counted as a pass (`note_model_probe`) and is paid and weighed as any other claim of the class: the
lifecycle only scales how many claims are admitted (`admission_permille`). `admits_claims()` is the only gate and the registry has
no unpaid state (ADR-0145 §7: "probation bounds volume, not price"). So rewards begin the span the class enters `Probation`, as soon as `ready_seats ≥ required_ready_seats`, before one claim
of the class has been verified and before any evidence about a seat's *executor* beyond a possession proof.

**F3. There is no executor-certification gate.** What the chain checks about whether a class can be executed correctly: statically at
registration (above), and by the primitive-set certificate (`certify_tir_e2e_family_v1`: the court acquits an honest run and convicts
planted faults at every primitive the program uses). Readiness is *possession* plus collateral and says nothing about whether a
seat's executor reproduces the class. The first evidence is the probe claims, in band and paid (F2). A divergent executor is *safe*
(the court convicts the party that computed wrongly, Part I §6) but not *free*: an artifact that does not match its own program, or an
executor that mis-implements a primitive, voids claims, resets probation, holds the class and wastes the honest producers' work. The
pack's conformance vectors (§II.4.4) catch these before registration; nothing makes registration, or readiness, depend on them.

**F4. A seat's resource failure is silent.** A seat whose memory share is below the class's replay writes a line to its own log and
never becomes ready (the 8k class needs 3.37 GiB per full-seat replay: 1.68 of artifact and 1.67 of trace scratch). The class never
leaves `Prefetching`, and the registry says only `ready 0 < 7 required (seats prove possession to be counted)`.

**F5. The registry's reasons are prose.** `RpcPalwModelLifecycle.reason` is a sentence from `palw_lifecycle_reason_v2`: no stage, no
code, no structured count, and the `Candidate` sentence describes the jury rule loosely. `getPalwModelRegistrationStatus` has stable
codes for the carrier's pipeline (constructed, submitted, accepted, included, folded) and nothing after `folded`.

### II.7.4 The node-only change: a structured `blocking` in the registry

`getPalwModelRegistry` gains, for each class, **what is blocking it and at which stage**. It reads only what the node already
serves, so it needs no consensus change, and its derivation is one pure function over a class's row and the registry's globals
(`palw_registry_blocking`, in `rpc/core`), used by the node to fill the response and by the CLI to show an older node's registry the
same way.

```
RpcPalwClassBlocking {
  class_id, stage: "convert" | "register" | "mine" | "",     // "" : nothing blocks
  code,                                                         // stable, SCREAMING_SNAKE
  what,                                                         // one sentence: what is missing
  have, need,                                                   // present where the condition is a count
  next,                                                         // what lifts it
}
```

| State (and condition) | Stage | Code | have / need |
| --- | --- | --- | --- |
| no row, or the base class | — | (none: legacy, never gated) | |
| `Registered`, `ops_supported` false | convert | `KERNEL_EXTENSION_REQUIRED` | |
| `Registered`, no artifact bytes or no derived work | convert | `NO_WORK` | |
| `Candidate` | register | `ADMISSION_JURY` | ready seats now / the jury quorum (`seat_count / 2 + 1`); the jury counts only its drawn operators |
| `Prefetching` | mine | `READY_SEATS` | `ready_seats_now` / `required_ready_seats` |
| `Probation { probes_passed }` | mine | `PROBATION` | `probes_passed` / `probation_claims` |
| `ActiveLimited`, spans below target | mine | `STABLE_SPANS` | stable spans / `stable_epochs` |
| `ActiveLimited`, spans reached | mine | `CAP_SATURATED` | |
| `Held`, ready seats below a panel | mine | `PANEL_NOT_DRAWABLE` | `ready_seats_now` / `seat_count` |
| `Held`, below the required seats | mine | `READY_SEATS` | `ready_seats_now` / `required_ready_seats` |
| `Held`, seats back, window does not fit | mine | `WINDOW_DOES_NOT_FIT` | |
| `Active` | — | (none) | |

`KERNEL_EXTENSION_REQUIRED` waits for the unsupported operation's reviewed kernel extension to be
implemented and activated; then the model must be lowered and registered as a new class (ADR-0172).

`what` carries the numbers a reader needs (`3 of 7 seats hold a fresh possession proof; 14 bonds on the network have the
collateral headroom to be a seat`; `4 of 10 probe claims have reached Final; 0 failed since entry`), and `next` says the act that
lifts the block (`run a seat for this class: a seat needs 3.37 GiB`, `the next admission audit is within 100 DAA`). The wire:
`GetPalwModelRegistryResponse` moves from version 1 to 2 and **appends** `blocking: Vec<RpcPalwClassBlocking>` after its last field;
an element of `classes` is not extended, because it is read as a vector element and a longer one would misalign an older reader,
while a trailing field of the response is not read by one. The CLI shows it in `misaka palw registry` (a line under each class
that has one) and in `misaka model status <class>` (`stage`, `blocked by`, `have/need`, `next`), and it is in `--json`.

Reserved for Proposal A (§II.7.5): `INDEPENDENT_OPERATORS` (`have / need` independent operators) and the class's licensable share,
served once the consensus read that carries the seating facts exists.

This closes F5 and the *reporting* half of F4 (the registry names `READY_SEATS` and the preflight, §II.2.3 item 8, names the
memory). It changes nothing the chain decides.

### II.7.5 The consensus rules — historical decision and implementation status at 2026-10-01

**Current reading (2026-10-10):** the seating predicate and dormant fence now exist in
`palw_class_seating_v1.rs` and `palw_class_seating_fence_v1.rs`. The following “not built” statements
are the original decision record. Presence of those modules does not prove activation or A7's
independent registration-to-Final exercise; see the implementation ledger for verified evidence.

The coordinator decided on 2026-10-01: **Proposal A approved** (one seating rule for every class kind, under the new dormant fence
`palw_class_seating`; the independence floor a fence parameter, default 3; armed with the RFC-0003/0004 flag day, not with int-10);
**Proposal B: go**; **Proposal C: C1**; and **`misaka model add` requires a `pack verify` PASS by default**, with an explicit expert
override flag that prints a warning (the chain does not enforce it). The consensus rules below are built by the consensus lanes
(lane D owns the shared predicate; this lane specifies it here, writes the scenarios the drills must include, and builds the
node-side reporting and the seat duty). None of the consensus rules is built yet.

All of them are **fences**: dormant on every preset (`None`, hashed Some-only, a dormant network fingerprints as if the rule did not
exist), armed on testnet-12 through a flag-day entry like `PALW_T12_TIR_FENCE2_ENTRY`, with a drill that crosses the fence on the
shipping binary.

#### Proposal A — one seating rule for every class kind (approved)

A fence `palw_class_seating`. Past it there is **one predicate**, asked at every door through which a class's work is admitted.

> `class_seated(class, executor, span)`. Let `Ready(class, span)` be the set of **operators** — an operator counted once, however many
> bonds it holds — with a fresh readiness V2 possession proof for the class, by the registry's own five-clause predicate (an active
> bond above the floor, a fresh row, free collateral for the readiness multiple), and `Base(span)` the network's base-class
> population as the admission jury and the outsider seat draw it (active bonds above the floor that serve the liveness floor,
> matured, registered before the span's cut). The class is **seated** for a claim whose executor is `e` iff
>
> 1. **possession floor** — `|Ready \ {operator(e)}| ≥ seat_count`: a panel can be drawn without the executor;
> 2. **independence floor** — `|(Ready ∩ Base) \ {operator(registrant), operator(e)}| ≥ independent_floor`, where `independent_floor`
>    is a **parameter of the fence** (default `quorum(seat_count)` = `⌊seat_count / 2⌋ + 1` = **3** on testnet-12): operators that are
>    neither the claim's executor nor the class's registrant and that the network's own lottery could have drawn. A genesis class has no
>    registrant to exclude.

**The parameter.** `independent_floor` lives in the fence's ceilings (`PalwClassSeatingCeilingsV1`), hashed into the params
fingerprint with the fence (Some-only), so it can be raised by a later flag-day entry and never moves under a running chain.
`validate_palw_v2` refuses a floor of 0 or above `seat_count` (a floor no panel could meet). Three is the default because it is the
number of operators the network already accepts as evidence of independence when it admits a bought class (the jury's strict
majority), made a *continuing* condition; the reasoning is below.

**Where it is asked** (one function, so the doors cannot disagree):

| Door | Today | Under Proposal A |
| --- | --- | --- |
| claims of an IR or legacy attempt class, bought or genesis | the lifecycle state admits claims (`Prefetching → Probation` at `required_ready_seats` ready *bonds*); a claim no panel can be drawn for is voided *after* the bind window as `NoCapablePanel`, the executor's reservation held meanwhile | `class_seated` at claim admission: a refusal (`ClassNotSeated`, naming the floor that failed) before the reservation is taken |
| a tensor claim of an RFC-0003 generative class | lane D's `GenClassNotReady`: `seat_count` distinct operators, never the executor's, hold the class with a fresh readiness V2 proof (`rfc3/gen-claim`, `9a25e6d20` / `28f60cf24`); its lifecycle row stays `Registered`, so this is its only gate | the same function: D's rule *is* condition 1; condition 2 is added |
| claims of a composite class (a parent and an unmerged adapter) | as an attempt class, on the composite's own class id and artifact root | the same function, on the composite's readiness rows |
| the lifecycle | `Prefetching → Probation` and `Held → Probation` read the bond count against `required_ready_seats` | they read `|Ready| ≥ required_ready_seats` (`seat_count` + spare, **7** on testnet-12) **and** condition 2: the class enters `Probation`, and its probe claims begin to be paid, only when seven distinct operators, three of them independent, are ready |
| the registry, the CLI and the preflight | `blocking` (§II.7.4) names `READY_SEATS` with a bond count | `blocking` reports `have / need` for each condition (`INDEPENDENT_OPERATORS`), and the registry reports the class's licensable share; the preflight's forecast says how many independent operators the network has |

**The shared function — the API proposed to lane D.** One implementation in `consensus/core` (`palw_class_seating_v1.rs`), pure over the
state, with no side effect. Lane D's inline loop in `apply_gen_tensor_commitment_v1` becomes a call to it.

```rust
/// The fence's terms in force at a DAA (`Params::palw_class_seating_terms_at`): `None` below the fence.
pub struct PalwClassSeatingTermsV1 { pub independent_floor: u16 }          // default 3

/// What the predicate counted: every number a refusal, the lifecycle, the registry RPC and the preflight need.
pub struct PalwClassSeatingV1 {
    pub ready_operators: u32,        // |Ready \ {operator(executor)}|                       condition 1, have
    pub needed_operators: u32,       // seat_count                                           condition 1, need
    pub independent_operators: u32,  // |(Ready ∩ Base) \ {operator(registrant), operator(executor)}|   condition 2, have
    pub needed_independent: u32,     // terms.independent_floor                              condition 2, need
    pub base_operators: u32,         // |Base \ {operator(registrant)}|                      the licensable share's denominator
}
pub enum PalwClassNotSeatedV1 {
    Possession { ready: u32, needed: u32 },
    Independence { independent: u32, needed: u32 },
}
impl PalwClassSeatingV1 {
    pub fn verdict(&self) -> Result<(), PalwClassNotSeatedV1>;     // condition 1 first: possession, then independence
    pub fn licensable_share_permille(&self) -> u16;                // independent_operators × 1000 / base_operators; 0 for an empty base
}
pub fn palw_class_seating_v1(
    read: &PalwFoldReadV1<'_>,                 // the fold's reader at the block, of which `model_registry_seat_is_ready` is a method
    fold: &PalwModelRegistryFoldV1,            // the registry fold: `globals.seat_count`, the readiness horizon
    class_id: &Hash64,
    executor: Option<&PalwBondKeyV2>,          // the claim's executor bond; `None` for the lifecycle's observation and the registry read
    daa: u64,                                  // the block's DAA score
    terms: PalwClassSeatingTermsV1,
) -> PalwClassSeatingV1;
```

- The **population** `Base` is one function, extracted from `admission_jury_v1`'s filter (`Active`, `palw_bond_may_take_work_v2`,
  `palw_bond_may_judge_class_v2`, registered before the span's cut and matured, the registrant's bond and operator removed), so the
  jury, the per-claim outsider draw and the seating floor read **one** population and cannot drift apart.
- **The refusal** is one error, `PalwStateV2Error::ClassNotSeated { class, floor, have, need }`, with `GenClassNotReady` kept as the
  generative lane's name for the possession floor if lane D prefers (`Possession` maps to it one to one).
- **The lifecycle** reads it with `executor: None`: `PalwLifecycleObservationV1.ready_seats` becomes `ready_operators` and a new field
  `independence_floor_met` (true below the fence, so no existing transition changes); `Prefetching → Probation` and `Held →
  Probation` require `ready_enough && independence_floor_met`.
- **The registry read** (`PalwModelRegistryClassReadV1`) gains `seating: Option<PalwClassSeatingV1>` (`None` below the fence); the RPC
  and the node's `blocking` (§II.7.4) serve it, and `GetPalwModelRegistryResponse` gains the class's licensable share.
- It is `O(bonds)` per call over a `BTreeMap`, deterministic, and called once per claim at admission and once per class per span.

**Why these numbers.**

- *Possession floor = `seat_count`.* It is the least for which a panel exists without the executor: lane D's structural floor, kept.
- *Independence floor = `quorum(seat_count)`, 3 on testnet-12.* It is the number of operators the network already accepts as evidence
  of independence when it admits a bought class (the jury's strict majority), made a *continuing* condition instead of one audit's
  snapshot, and it is the least that stops the registrant and the executor, between them, from completing a first panel's quorum.
  Fewer (1) leaves one operator's outage between a class and `Incapable` voids; more (`seat_count`) makes every new class depend on
  the whole fleet of a network that has a few dozen operators. Independence beyond the quorum is carried by the lottery and by the
  collateral behind each key, not by a larger count — and the count is a parameter, so a larger network can raise it.
- *Operators, not bonds.* As the panel room's `ready_eff` already counts. On testnet-12, where `palw_operator_id_unique` makes an
  operator one bond, it changes no number; elsewhere it removes an over-count.
- *What it cannot do.* The chain cannot tell two keys of one party from two parties (§II.7.3 F1(iii)): a registrant that funds three
  more keys, each with the collateral of a panel-eligible bond (130,000 BILI on testnet-12), passes condition 2. The independence is
  bought and priced in collateral locked, which is the most that a rule reading only the chain can ask.

**The ADR-0147 outsider seat (lane D's caveat).** A bought class's claim has an outsider seat, drawn **per claim** from `Base`, which
**must hold the class to file `Valid`**: else it pleads `Incapable` and the claim voids at its receipt deadline (ADR-0147 §2.1). Condition
2 counts only operators of `Base` that hold the class, so the outsider pool is never empty; it does **not** make the outsider capable,
because the draw is over all of `Base`, not over its holders. The probability that a claim's outsider holds the class is
`|Ready ∩ Base \ {registrant}| / |Base \ {registrant}|`; three holders in a base of twenty license 15 % of the claims. That is
ADR-0147's price and it is kept on purpose: drawing the outsider from the *holders* would make its composition the registrant's choice,
which is exactly what ADR-0147 refuses. What Proposal A adds is **visibility**: the registry and the preflight report that share (the
class's *licensable share*), so a producer sees before its first claim what fraction of its claims can be licensed. No gate reads it.

**Arming.** With the RFC-0003/0004 flag day (the same consensus release as `palw_gen_v1` and the improvement protocol), **not** with
int-10. `palw_class_seating` is `None` on every preset until that flag day's entry sets its height and ceilings.

Cost: a class nobody else serves is admitted no claim, which is the independence the jury demanded once, kept; adoption is how a model
gets claims, and the registry says how far it is (`blocking`: `have 2 / need 3` independent operators). Risk: one function now guards
every door, so a defect in it is a defect everywhere; the predicate is pure over the state, is tested against lane D's gate and the
lifecycle's, and the fence is crossed by a drill on the shipping binary.

##### Scenarios the drills must include

The seating rule is exercised in the RFC-0003/0004 drills (lane C's D-M and lane D's step 8). Each scenario applies to every class
kind (an attempt class, a generative class, a composite class) unless it says otherwise, and states what must be observed. **Where
each runs** (int-11, S = 240 on the salted drill chain, seven seat nodes and one node-less bond): in the chain, SEAT-2 together with
the SEAT-5 lapse (one class, the executor among exactly five ready operators after one node stops serving it, one node down at a time),
SEAT-4 (six ready, a seventh starts serving), SEAT-7 (a genesis class admitting claims), SEAT-9 (the fence's edge, with a relay started
without the flag) and, where the class exists, SEAT-11; as fold-level tests, SEAT-3, SEAT-6, SEAT-8's statistics (the chain checks only
that the registry reports the licensable share) and SEAT-10; the generative variants of SEAT-1 to SEAT-5 in lane D's chain.

| # | Setup | Observed |
| --- | --- | --- |
| SEAT-1 | **Too few operators.** Fewer than `seat_count` distinct operators, the executor's excluded, hold the class ready (4 of 5) | the claim is refused, `ClassNotSeated { Possession, have 4, need 5 }` (a generative claim: `GenClassNotReady`); no reservation is taken and no `NoCapablePanel` void follows; the registry's `blocking` says `READY_SEATS` with `have / need`; the CLI shows stage `mine`. *For an attempt class the lifecycle holds a class below `seat_count` ready operators at the next span boundary, so the refusal is by seating within the span of the lapse and by the state (`Held`, `PANEL_NOT_DRAWABLE`) after it; the pure possession refusal is the generative lane's, whose lifecycle stays `Registered`* |
| SEAT-2 | **The executor does not count.** Exactly `seat_count` ready operators, the executor's among them | refused with `have = seat_count − 1`; the same class and claim with a different executor (not among the ready operators) is admitted |
| SEAT-3 | **Enough operators, too few independent.** `seat_count` ready operators of which only two are independent (the others: the registrant's, the executor's, or registered after the span's cut). *A fold-level test: where every operator is in `Base`, only the registrant and the executor are excluded, so five ready operators excluding the executor contain at least four independent ones and condition 2 cannot bind alone* | refused, `ClassNotSeated { Independence, have 2, need 3 }`; `blocking: INDEPENDENT_OPERATORS 2/3`; when a third independent operator proves readiness the next claim is admitted |
| SEAT-4 | **Enough.** At least `seat_count` ready operators, three independent. For an attempt class in `Prefetching`: with six ready operators it stays `Prefetching` (`READY_SEATS 6/7`); with a seventh (three independent) it enters `Probation` at the next span boundary; likewise `Held → Probation` | claims admitted; the lifecycle transitions as stated; the first probe claim is paid |
| SEAT-5 | **A lapse.** One independent operator's readiness expires (the readiness age) while the class is in `Probation` | the next claim is refused (`Independence`, 2/3), the state stays `Probation` until the existing rule holds it (`ready < seat_count`), and claims flow again when the operator re-proves; no probe is counted for a refused claim |
| SEAT-6 | **Operators, not bonds** (a devnet without `palw_operator_id_unique`). One operator with three ready bonds | counts once: five bonds from three operators are three ready operators, and the claim is refused |
| SEAT-7 | **A genesis class** (no registrant). The floor excludes only the executor | with the eight genesis operators ready: admitted; with four: refused |
| SEAT-8 | **The outsider.** A bought class held by 3 of a base of 20 | the registry reports a licensable share of 150 ‰ before the first claim; over at least 400 claims the licensed fraction is within tolerance of it, and the rest void as `Incapable`, as ADR-0147 says |
| SEAT-9 | **The fence's edge.** A claim accepted just below the fence height | judged by the old rules for its whole life (the claim's own `accepted_daa`, as ADR-0147 §2.4: the draw, the bind and every licensing arm read one answer); a claim accepted above it by the seating rule; below the height every rule is byte for byte the release's |
| SEAT-10 | **The parameter.** `independent_floor` raised from 3 to 4 at a later flag-day entry | a class with three independent operators is admitted below the new height and refused (`Independence`, 3/4) above it |
| SEAT-11 | **A composite class.** The parent held by seven operators, the composite (parent + adapter) by two | the composite is refused (readiness is per class id and artifact root); the parent's claims are unaffected |

#### Proposal B — executor conformance by the seat (go)

**Decided: go.** To be built by this lane after the preflight (§II.2), node-only, no fence. A seat, before it first proves readiness
for a class, runs the class's canonical self-test on its own executor against the reference evaluator — a few positions, or one tile
of each primitive kind for a large class (§II.9 L2: sampled conformance) — and posts a possession proof only if they agree. When it
does not, it writes why, and `getPalwNodeStatus` (what this node is doing) shows it. A dishonest seat can skip the self-test and sign a false assertion; readiness alone does not prevent harm or establish detection. Responsibility follows only when authenticated evidence objectively refutes the assertion within its signed scope. Future admission must therefore also pass independent public-bond evidence acquisition, localization and conviction under RFC14/ADR173. The registry cannot tell from a readiness declaration alone which seats actually tested.

#### Proposal C — what probation pays (C1 decided)

**Decided: C1.** Probation stays paid, and the gates move in front of it (A, and B at the seats). Probation then begins only with
seven ready *distinct* operators, three of them independent of the registrant and the executor, each of which ran the class's
conformance on its own executor, and it admits 50 ‰ of the derived volume until ten probe claims have been verified. The first paid
claim follows every condition of R5 that a chain can enforce. (C2, unpaid probation, was not chosen: it needs a probe-claim marker or
a state-dependent payout in the claim accounting, touches conservation properties of the escrow and the vesting, and asks producers to
run inferences unpaid for a class they may not trust.)

#### The registration policy of the CLI (decided)

`misaka model add` **requires a `palw-class pack verify` PASS by default**: it takes the pack with `--pack <dir>`, verifies it against
the artifact, and registers only on PASS. An explicit expert override, `--skip-pack-verify`, registers without one and prints a
warning that nothing the pack would have checked was checked. This is a CLI policy, not consensus: the chain does not enforce it, and a
registration made any other way is valid.

#### Open items

1. **The API with lane D** (above): to confirm or amend the signature, the error and the lifecycle observation's new field; one
   implementation for IR, generative and composite classes.
2. **The flag-day height** for `palw_class_seating` (the RFC-0003/0004 flag day).
3. **The drills** (lane C's D-M, lane D's step 8) to include SEAT-1 to SEAT-11.

## II.8 Acceptance criteria and corpus completion

### II.8.1 The acceptance criteria as checks

| Criterion | The check that closes it | State |
| --- | --- | --- |
| **A1** a custom quantisation gets a verdict before the weights are fetched | `palw-class preflight` over a synthetic GGUF header with custom type ids and an `mmproj`, in a file truncated after the header: the verdict names the type ids, the tensors and the unsupported feature, and the source never reads past the data offset (instrumented) | with the preflight (task 7). The refusal's content exists (§II.5.6) |
| **A2** root, converter version and profile reproducible; independent conformance | `pack build` then `pack verify --rebuild` rebuilds the artifact under pools of 1 and 7 threads to one root and runs the vectors on the reference evaluator, the typed backend and the independent implementation; the Hugging Face fit is within the tolerance | met on the fixtures (`tests/runtime_pack.rs`, `tests/determinism.rs`); cross-platform reproduction is §II.9 L7 |
| **A3** the testnet-12 preflight shows the court window and the canonical-job conditions | a Qwen3.5-9B-shaped configuration: the window needed and the limit, and the canonical job against the network's bounds, at a height | with the preflight (task 7) and the court-window plug point (§II.2.6) |
| **A4** mining starts only after ready seats of distinct operators with possession proofs | today: ready bonds with possession proofs (the lifecycle, §II.7.2); after Proposal A and C1: for every class kind, seven distinct ready operators, three of them independent of the registrant and the executor | partly met today; Proposal A (approved) closes the rest |
| **A5** failures are visible per stage in the CLI and the registry | the preflight's stage table (convert, register, mine); the registry's `blocking` and its CLI display (§II.7.4), one test per lifecycle state | with `blocking` (this lane) and the preflight (task 7) |
| **A6** a majority of Hub model repos can register and reach `Final` | the pinned Hub census, real-checkpoint cohort and end-to-end gate evidence meet every threshold in §II.10.5 | open; architecture-corpus percentages and earlier Hub estimates do not close it |
| **A7** a new family within armed semantics needs no `main` update | third-party direct TIR and a declarative frontend pack each complete §II.11.4 with the same node/consensus binaries; a novel primitive is refused | open; current adapter and quant-descriptor support do not prove this full route |
| **A8** local LLMs meet kernel coverage | pinned cohort shows ≥90 % real-size registration-to-Final success with the confidence requirement; all residual blockers remain counted | open; no measured denominator or active-kernel cohort drill |

### II.8.2 Corpus completion

The preflight and the frontend are only as good as the architectures they have met, so Part II adds a *completion* criterion to
Part I's freeze criterion (§1.2), for the corpus-and-coverage lane. The corpus is complete when:

1. **Breadth.** It holds **50 to 100** representative architectures, counted by *feature set*, not by name (two models with equal
   `ModelSpec`s are one entry), each with a real published configuration and a tiny random-initialised fixture that matches
   `transformers` for its reference.
2. **Expressibility.** **At least 90 %** of the corpus lowers with the features and primitives that exist, as Level A or Level B —
   no new feature, no new primitive. The remainder are Level C, each naming its missing feature or its smallest *general*
   primitive.
3. **Court coverage.** For every TIR primitive *reachable* from the lowered corpus programs (appearing in some commit cone), the court
   has a conviction and an acquittal vector: the family certificate covers **100 %** of them.
4. **No model names in the path.** No branch on `model_type` or an architecture name outside adapters, fixtures and tests; the gate
   is a search over the crates.
5. **Reproducible verdicts.** Every corpus entry has a golden preflight (`--json`, no timestamp) and a golden program and artifact
   digest (`tests/golden_lowering.rs`); a change that moves one is deliberate and bumps the version it belongs to.

## II.9 Known limits and open questions

**Limits of what exists.**

- **L1. Calibration's float reference is not row-streamed.** It runs one layer's weights resident at a time before the budgeted
  window of the conversion and is reported, not budgeted (`tests/streaming_budget.rs`). A model whose single layer does not fit a
  host's memory needs the float reference itself streamed.
- **L2. Conformance loads the artifact whole on each of three executors.** For a very large model, `pack build` and `pack verify`
  need a streamed or **sampled** conformance mode (the commit points of a few positions, or one tile of each primitive kind), which
  Proposal B also needs. Not built.
- **L3. The lowerer emits the `legacy-greedy-only` logit convention** until lane G's `LOGITS_Q24_V1` (lowering version 2, `af8d35a86`
  on `tir/generic`) reaches this line; a pack records either, and `pack verify` checks the unit by the slope of the fit.
- **L4. Quant vectors from re-implementations** (§II.5.7): only the ggml types and `MXFP4_HF` are checked against the maintainers'
  own code. A real GPTQ, AWQ, compressed-tensors and bitsandbytes checkpoint, with the library's own dequantised output, belongs in the
  corpus.
- **L5. An LLM.int8 checkpoint is read at any threshold** (§II.5.8), so the integer program omits the run-time outlier decomposition the
  library applies at a threshold above 0; the pack's reference fit is what measures the difference, and a pack that fails its tolerance is
  not VERIFIED. No real int8 checkpoint has been measured (the library is not installed offline).
- **L6. A 4-bit code costs a byte** (§II.5.8); a packed `i4` is a v2 candidate in `freeze-v1.md`.
- **L7. `libm-v1` is argued platform-independent (pure Rust, no `std` math) and measured on one platform** (§II.3.3); a Linux
  x86-64 rebuild of a pack built on macOS arm64 has not been run.
- **L8. Remote header fetching** (cargo feature `remote`) is tested against a loopback server only.
- **L9. Vision and audio towers are not computed** by any class (RFC-0003), and are named in the scope.

**Open questions.**

1. **Should a pack's digest be committed in the class record?** It would let a seat check that the artifact it holds was built by a
   verifiable pack. The container's meta already carries the provenance and the sidecar binds file digest and inventory root, so this
   is not proposed: a class record is consensus data, and a pack is evidence.
2. **Where do conformance vectors for a permissionless class come from?** The registrant's pack is one source; every seat can also
   regenerate the vectors from the program on the reference evaluator, which is Proposal B.
3. **The court window** (§II.2.6): whether the class-specific window of int-10 replaces the global rule everywhere the preflight
   reads it, and what the preflight reports for a class registered under the old rule.

## II.10 Hugging Face majority: from a lowerable architecture to a class that can reach `Final`

*Added 2026-10-03 for R6/A6. This is an acceptance and implementation plan for Part II. It does not arm a fence, relax
`tir_admit_v1` or the court, or assert that the present build already meets the target. RFC-0003 supplies non-text job and
output profiles; RFC-0006 and RFC-0007 supply the smaller per-seat verification and cheaper receipt carriage needed to
serve large classes. RFC-0008 changes block work, not the model-onboarding denominator.*

### II.10.1 The gap and the unit of measurement

The 92 % A/B reading of the 100-architecture corpus (§Implementation status, `hf-coverage.md` §23) means that those
fixtures' feature combinations read. It is **not** the share of Hub repositories whose real weights convert, whose
full advertised task is supported, whose real-size program passes admission, or whose claims can be licensed and
finalised. `hf-coverage.md` §19.2's approximate registrable counts are estimates over incomplete modality and format
coverage, not a live census or a `Final` measurement. An unsupported image, audio, encoder, adapter or quantised repo
cannot disappear from the denominator just because the decoder corpus passes.

The census unit is one public **model repository at one pinned commit SHA** (`repo_id@revision`) in a dated Hub snapshot.
One repo counts once in the headline, even if it publishes many formats. Its selected artifact path, every base-model
reference and their SHAs, task, intended inputs/outputs and weight digest are recorded. A derived LoRA or other adapter
counts only if its base resolves at a pinned revision and the complete composite can be checked. Also report unique
artifact roots and unique feature sets separately, so a popular architecture copied into thousands of repos cannot
hide missing families. Later revisions enter the next snapshot, never rewrite the old result.

Use three visible denominators, and publish the exclusions rather than silently shrinking them:

| Denominator | Meaning |
| --- | --- |
| `D_all` | Every public Hub model repo enumerated at the snapshot, including gated listings and repos with no usable weights. Private repos are not enumerable and are stated outside the claim. |
| `D_files` | The subset with publicly readable, complete weight files or a fully resolvable base-plus-adapter chain. An unknown architecture, format or modality stays **in** this denominator. |
| `D_rights` | Repos from `D_files` for which the registrant has established permission for the proposed download, redistribution/serving and on-chain use. A card's license field is evidence to review, not an automatic permission decision; unknown terms remain `RIGHTS_UNCONFIRMED`. |

Publish both repo-weighted and download-weighted rates, but the repo-weighted `D_all` rate is the headline. Downloads
are a secondary demand signal, never a substitute for breadth. Report each task (`pipeline_tag`), library, model-size
band, format, age and feature family, including an `unknown` stratum. A repo that advertises a multimodal or generative
task does not count as covered because its text decoder alone lowers: the **whole declared task** must have a canonical
job, inputs, output, court path and artifact. A smaller text-only class may be listed as a separately scoped class,
without crediting the original repo's full task.

### II.10.2 A repeatable Hub census and real-checkpoint cohort

At a recorded UTC instant, enumerate model repos through the version-pinned Hub API. Save the response, repo id,
commit SHA, `pipeline_tag`, `library_name`, card license/gating fields, downloads, `siblings` and the recursive file
inventory with sizes and content identifiers. Resolve the repository and every referenced component at those SHAs.
Fetch only configs, model/pipeline indices, tokenizer metadata and safetensors or GGUF **headers** for the census's
first pass. File access failure, missing weights, an ambiguous base reference and unsupported files are results, not
repos dropped from the table. Respect Hub pagination and rate limits; the census is reproducible from its saved
manifest without asking the Hub to retain the same mutable `main` branch.

The Hub exposes `list_models`, revision-pinned `model_info` and `list_repo_tree`; model cards carry task and license
metadata. `snapshot_download` can fetch only named files at a revision. Standard Diffusers pipelines have a
`model_index.json`, and modular pipelines may have `modular_model_index.json`; treating every repository as a
`transformers` text `config.json` is therefore a measurable coverage loss. See the primary API descriptions:
[Hub API](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api),
[model cards](https://huggingface.co/docs/hub/model-cards),
[revision-pinned downloads](https://huggingface.co/docs/huggingface_hub/en/guides/download), and
[Diffusers loading](https://huggingface.co/docs/diffusers/main/using-diffusers/loading).

Run the header-only preflight for **every** `D_all` entry that exposes readable metadata. For deeper gates, draw a
published, reproducible stratified sample of real checkpoints from each material task × format × size × feature
stratum, with a random tail sample as well as the most downloaded repos. Pin the sample seed, inclusion probability,
all repo SHAs and reasons for nonresponse. A finite corpus of 100 architecture fixtures remains a regression suite;
it is not this probability sample. Publish estimates with confidence intervals and raw counts, and show the failure
list so the numerator cannot improve by reclassifying a hard repo as `unknown` or `inaccessible`.

### II.10.3 The gates whose failures must be counted

The preflight, pack and chain observations use one machine-readable row per repo and one stable `blocking` code per
failed gate. A downstream gate is `NOT_RUN_AFTER_<gate>` when an earlier one failed, never `PASS` by inference.

| Gate | PASS evidence | Examples of named blockers |
| --- | --- | --- |
| `source` | Pinned files, components, task and access/rights decision; complete artifact or resolvable adapter base | `GATED_ACCESS`, `MISSING_WEIGHTS`, `BASE_UNPINNED`, `RIGHTS_UNCONFIRMED` |
| `lower` | Full task lowers to a versioned `ModelSpec`/TIR program and every semantic key and weight is consumed | `FEATURE_C`, `CUSTOM_CODE_UNMODELLED`, `MODALITY_PROFILE_MISSING`, `PARTIAL_TASK_ONLY` |
| `pack` | Real checkpoint converts by a bounded-memory stream; format decode matches an independent implementation; reference and integer outputs meet the pinned fidelity rule; rebuilt roots agree | `QUANT_DESCRIPTOR_MISSING`, `QUANT_LIBRARY_UNVERIFIED`, `FIDELITY_FAIL`, `ARTIFACT_MISMATCH` |
| `admit` | The real-size graph, cost, court cone/window, DA close, capacity and artifact pass the actual `tir_admit_v1` and registry path at a named ruleset/height | `COURT_BUDGET`, `CLOSE_TOO_LARGE`, `CONTEXT_BOUND`, `CLASS_SEATING` |
| `seat` | Required distinct, independent operators prove possession and each seat's executor conformance for **this** class root; required shard/witness data are available | `READY_SEATS_SHORT`, `INDEPENDENCE_SHORT`, `SEAT_MEMORY`, `WITNESS_UNAVAILABLE` |
| `claim` / `Final` | A real job makes a valid PALW claim, obtains counted independent receipts, survives the court window and reaches `Final` with its expected reward/weight; the claim and all lifecycle objects remain in the accepted fold | `PANEL_BACKLOG`, `RECEIPT_TIMEOUT`, `OBJECT_DROPPED`, `COURT_FAULT` |

`registration-ready` means `source` through `admit` passed offline against the named ruleset; `registered` requires
the class's transaction to be accepted on the chain. `seat-ready` and `Final-proven` are later, different facts.
A produced block, a `PanelBound` object, a `Capped` class, or a successful tiny fixture is not a `Final` verdict.
Report stage counts as a funnel for `D_all`, `D_files`, `D_rights` and every stratum, with the exact stage at which a
repo stopped. Never use an unverified pack, an unavailable independent seat or an optimistic receipt as a pass.

### II.10.4 Close the largest measured blockers without weakening verification

1. **Generic frontend and full-task profiles.** Rank `FEATURE_C` and `MODALITY_PROFILE_MISSING` by lost repos and
   downloads. Add a feature to the data adapter/generic lowerer once for all matching architectures (II.1 P1–P4),
   and require a full real-checkpoint fixture plus the one-move court battery. Route image, embedding, vision input,
   audio and video through RFC-0003's canonical jobs, stage edges and outputs. Keep a missing profile blocked; no
   text-decoder-only success can silently stand for a vision or audio model.
2. **Formats and custom code.** Add common safetensors, GGUF and other measured weight layouts through the descriptor
   registry (§II.5), checking real upstream quantiser output, not only synthetic bytes (§II.9 L4–L5). A repo's Python
   `trust_remote_code` or custom Diffusers pipeline never runs in a consensus node or seat. An offline, revision-pinned,
   isolated reference run may help author a declarative adapter and fixtures; independent implementations must then
   reproduce the canonical program, weights and output. Unknown behaviour remains a named refusal. Hub custom code
   is executable code, as [Diffusers documents](https://huggingface.co/docs/diffusers/using-diffusers/custom_pipeline_overview).
3. **Real-size admission.** Triage actual `COURT_BUDGET`, close, context and fidelity failures before changing a
   ceiling. For example, `hf-coverage.md` §19.1c records Qwen3.5's real-size `C_j` refusal and proposes bounding
   elementwise work in the same generic cost proof. Measure the new bound and court vectors; never waive a court
   ceiling, a range proof or an unknown quantisation to improve a coverage percentage.
4. **Seats and verification supply.** Proposal A's class seating and Proposal B's seat conformance (§II.7.5) remain
   binding. RFC-0006 can make a large class feasible per seat by verifying committed layer/position cells with only
   that shard's weights; RFC-0007 can lower verifier work where its algebraic checker is sound and batch verdict
   carriage. Their dormant fences need independent crossing drills before their gains count. A capped class under
   RFC-0007 is measured separately: it does not pass `Final` until full holder coverage and re-verification. At the
   target claim rate, measure receipt supply, `PanelBound` age, queue growth, licences, `Final` rate and dropped
   lifecycle objects. Increase qualified seat supply or bound intake when the queue grows; do not turn missing
   receipts into `Valid` or reduce independence/quorum to meet a target.
5. **Iterate from the census.** For each release, publish the top blocker buckets with counts, one generic change per
   bucket, its before/after cohort, and regressions in other strata. Re-run the census on new pinned snapshots.
   An improvement in A/B architecture coverage that does not move `registration-ready`, `seat-ready` and `Final`
   remains a frontend improvement, not completion of R6.

### II.10.5 Exit criteria and the permitted coverage claim

The phrase **"most Hugging Face models can be brought to PALW"** is permitted only when a dated report shows all of
the following. The first target is literal majority; the engineering target is at least 80 % of `D_files` at
`registration-ready`, without treating `D_files` as the headline denominator.

1. The estimated repo-weighted `registration-ready / D_all` share has a **one-sided 95 % lower confidence bound above
   50 %**, with exclusions and nonresponses included as failures in `D_all`. Give the exact point estimate, bound,
   denominator and snapshot. Report all material task/format/size strata even if some remain below 50 %; never
   advertise an unrestricted modality claim from a text-only result.
2. From the same stratified real-checkpoint cohort, the estimated `Final`-capable share of `D_all` also has a lower
   bound above 50 %. Each sampled success needs a confirmed registration, independent seat evidence, a PALW claim
   and its on-chain `Final` under the shipping binary and the target ruleset. A sample without enough seats is a
   measured `seat` failure, not a waived trial. The report includes enough independent operators to exercise the
   seating rule; a four-node fence-crossing drill cannot close this item.
3. The panel runs at the intended sustained issuance rate for at least two receipt windows without an increasing
   `PanelBound` queue, an unaccounted dropped lifecycle object or a stalled `Final` rate. Every failure is retained
   in the cohort's gate table. Corpus coverage (§II.8.2), `Capped` entry, block production and isolated unit tests
   are supporting evidence, never replacements for this run.

If any condition fails, publish the measured narrower claim (for example, *decoder-only float checkpoints lower*)
and the blocker distribution. This keeps the goal visible without declaring Hub-majority registration or `Final`
from the 90–92 % architecture corpus or the earlier unmeasured Hub estimates.

## II.11 Future families without a `main` release (R7/A7)

*Added 2026-10-03. This is the target contract for permissionless onboarding, not a claim that the current frontend,
registry or seats already pass it. “No `main` update” means no consensus/node binary upgrade and no privileged
model-family approval. Registrants and seats may install third-party **off-chain** tooling and fetch class artifacts;
the class must still satisfy the armed admission, seating, execution and court rules.*

### II.11.1 Decide by the missing semantic, not by the model's name

| What a future model needs | Route | `main`/protocol change? |
| --- | --- | --- |
| New config keys, tensor names, layer schedule or a **static** combination of already armed TIR primitives | A third-party frontend pack (§II.11.2), or direct canonical TIR (§II.11.3) | No, if its graph and real-size class pass admission |
| New storage or quantisation layout expressible by `quant-format.v1`, or a custom offline importer that emits the canonical artifact | A descriptor with independent decode vectors (§II.5), or an independently reproducible converter | No; unknown layouts remain refused until described and checked |
| A new task made solely from already armed canonical job/input/output kinds and pipeline stages | A class-declared RFC-0003 pipeline and complete task scope | No; a new **consensus** job or output meaning needs a versioned RFC-0003 extension |
| A tensor operation not expressible by active primitives within checker/court limits | `KERNEL_EXTENSION_REQUIRED`: generic semantic/constraint/checker/court family, independent vectors, resource/soundness review and fence (§1.2, §9; ADR-0172) | Yes, coordinated kernel upgrade; no guest fallback |
| Bounded data-dependent control | Finite AOT/`Select` within active grammar/limits, or an explicitly defined bounded routing/state kernel extension | No for admitted composition; yes for missing relations; no BVM/GVM |
| Runtime-dependent agent/tool workflow or arbitrary program logic | Off-chain orchestration may submit supported jobs; arbitrary uploaded programs are outside the model-plan grammar | No consensus VM is planned; a new bounded task relation needs a separate kernel/task proposal |

Thus a frozen finite TIR can support **new combinations indefinitely**, but cannot promise every future computation.
There is deliberately no universal instruction substrate. Missing semantic/memory/checker/court relations remain
extension gaps until a reusable kernel upgrade becomes active. Arbitrary circuits, `CustomOp`, EVM precompiles
and frontend scripts cannot bypass that boundary. Bind semantics, whole-statement constraint coverage, bounded
resources and dispute localization; confidence is not registrant-selected. Importers and job/input/output profiles
remain separate problems. The old RFC05 decoder survey is not measured residual-model coverage (§II.12).

### II.11.2 An open, content-addressed frontend-pack interface

The built-in `FeatureId` registry and `model-adapter.v1` are conveniences for the reference lowerer, **not** an
allowlist for consensus. The present adapter maps keys onto features that the installed lowerer already knows;
therefore that adapter alone cannot express every future feature without a tool update. Add a versioned,
user-supplied `tir-frontend-pack` format for the common no-release path. Its declarative language can construct a
bounded static TIR subgraph from existing primitives, dimensions, layer schedules and declared `Fixed`/`Hist` state;
map config keys and tensor roles; include quant descriptors; and declare the full task scope and source revisions.
It may compose named template fragments but has no runtime branch, IO, clock, remote fetch, native code or operation
definition. The compiler expands it under explicit size/fuel limits, type-checks every tensor and refuses an
unconsumed math-affecting config key, tensor, component or custom-code behavior. Its output is ordinary canonical
TIR plus the artifact and runtime pack, not a new class kind. A pack may be published anywhere and identified by
its content hash; no default repository, signer or maintainer approval is needed for admission. The reference CLI
may offer a curated index for discovery, but index membership cannot decide validity.

The runtime pack pins the frontend-pack hash, compiler/tool version, config and source SHAs, quant descriptors,
generated `ModelSpec`/TIR digest, artifact root, complete/partial task scope, conformance vectors and HF-reference
fidelity evidence (§II.4). A seat verifies **the submitted TIR and artifact** with the armed generic evaluator and
class conformance rules; it never has to execute a registrant's Python, `trust_remote_code` or frontend pack to
decide a claim. A third-party pack's own test vectors do not grant authority: malformed output fails the common
verifier, and a false HF-equivalence claim is distinguished from the on-chain integer-program verdict.

**Implementation increment (2026-10-10):** [tir-frontend-pack.v1](../design/palw/tir/tir-frontend-pack-v1.md)
now composes the frozen primitive/state grammar without ModelSpec dispatch, consumes config/tensor
bindings, and streams exact integer or explicitly rounded IEEE imports into the ordinary container.
`palw-tir-frontend` emits replayable receipts; SDK `pack build-frontend`/`verify-frontend` pins source
SHAs/compiler/executor revisions and uses common inventory and three-way conformance. Fixtures
include direct-byte equality and offline class admission. Bounded inline **virtual**, **tensors**
and **blocks** quant descriptors now feed the same compiler/SDK route, with explicit raw roles,
mandatory decode vectors, nested-config checks, bounded pages and before/after source hashes.
Tensor shape/JSON metadata is acquired after all binding-header checks, with per-plan and aggregate
limits, exact-number parsing and source-change pins. MXFP4 expert axes, all built-in saved layouts
and unknown third-party formats are exercised. Storage decode is explicitly binary32 before the
declared integer rounding. Native whole/split GGUF acquisition now exposes raw names/layouts to
the same generic CLI/SDK, including supplied unknown GGML IDs, strict native metadata and embedded
tokenizer identity. Standard split names and a public index allow independent part selection;
part numbers, shared declarations, primary metadata and global tensor uniqueness are checked.
Aggregate header/allocation/count budgets, checked extents and snapshot revalidation protect
this producer-side reader across every part and index. SDK artifact publication follows all receipt/inventory/conformance/source
checks. Independent companions now attach public reference logits under predeclared revision/task/
context/units/error criteria, and verify independently repeats a portable fit. The existing exact-class
binding and beacon commit/run/fresh-verify tools accept both formats through one artifact/layout/
static-admission/challenge/evidence path; independent imports use typed absent calibration.
Conformance binds the node's kernel program-root domain, separately from the container graph root,
and derives the same plan. Single-program selection uses the node prosecution/carrier/block
limits for v1/v2/segmented v4 without shortening requested context; v4 still requires separate
OPV admission/economics. No tool report supplies availability, activation or approved policy.
Reference-logit gates stay separate from source identity, routing/state fidelity and task quality.
These synthetic fixtures do not close §II.11.4: real-checkpoint source fidelity, real-node beacon and
independent-seat
claim/Final/redemption evidence remain open in the [implementation ledger](../design/palw/tir/rfc0002-implementation.md).

### II.11.3 Direct-TIR escape hatch and exact boundary of permissionlessness

Any independent compiler may submit canonical `TirProgramV1` bytes (or an armed RFC-0003 pipeline), a compatible
artifact and the ordinary class registration. The registry must judge them by `prim_set_id`, program version,
canonical encoding, type/range/cost/court admission, artifact commitment and class lifecycle only. It must not
require a recognised `model_type`, built-in `FeatureId`, official pack, HF repository, preferred compiler or
model-family signature. This path is essential when the declarative frontend language is too narrow but the
**resulting TIR is already expressible**. It also keeps an extension-language revision outside consensus.

Direct TIR proves the submitted integer program can be adjudicated; it does **not** prove that it faithfully
imports the named HF checkpoint. Without independently reproducible source conversion and fidelity evidence,
show `SOURCE_EQUIVALENCE_UNVERIFIED` in onboarding reports and do not count the repo as an A6 full-task success.
Likewise, an active kernel execution path alone does not solve a missing canonical input, output or job profile;
the full advertised task stays blocked under §II.10.1 until that profile is armed. A class that exceeds static
ceilings remains `EXCEEDS`, however popular its model family is. New semantics must use a new `prim_set_id` or
program/job version and fence; previously registered classes retain their exact old meaning and court.

### II.11.4 Acceptance before calling this permissionless

With **unchanged** consensus/node binaries, unchanged armed `prim_set_id` and no model-name allowlist:

1. An independent party names a previously unknown, static feature combination, publishes a frontend pack and
   a real checkpoint, then completes header preflight, streamed conversion, reproducible pack verification,
   registration, independent seat possession/conformance, a claim and on-chain `Final`. Another party rebuilds
   the same roots without the first party's executable code.
2. Another independent party emits the same canonical TIR bytes directly with a different compiler. Admission and
   court give the same verdict. A third party adds a quant descriptor and its independent vectors without a
   node release; bad vectors and ambiguous tensor bindings are refused.
3. Mutation cases cover an unknown op, hidden config key, missing tensor/component, false shape, unbounded
   expansion, unsupported job kind, bad artifact root and a deliberately wrong fused backend. Each fails at the
   named gate; an untrusted pack cannot bypass admission, seating or court. The old registered class still runs
   byte-identically after a newer frontend-pack version is published.

Record node/consensus binary digests, frontend/compiler and pack digests, class and artifact roots, seat operators,
claim id, `Final` reference and every refusal. Passing a small direct-TIR fixture establishes the protocol
interface; a **real checkpoint, full task and independent seats** establish the onboarding claim. Feed these
results into A6's Hub cohort, so the no-release extension mechanism demonstrably improves the real `Final` funnel.

Implement in this order: arm and exercise generic TIR admission/execution/court and the needed RFC-0003 job profiles;
prove direct third-party TIR registration; add the bounded declarative frontend-pack interpreter and open discovery;
run A7 and the Hub-wide A6 cohort. Measure the local-LLM residual (§II.12) before prioritizing or claiming a new
kernel extension. The public extension interface, Hub-wide majority and local-LLM coverage target are distinct milestones;
none may be inferred from another.

## II.12 Local-LLM coverage: active kernels and an explicit extension queue (R8/A8)

*Added 2026-10-03; revised 2026-10-06 under ADR-0172. The historical 90/10 VM split is withdrawn.
This is not a present 90 % measurement. It is narrower than A6's all-Hub denominator and broader than the old
decoder-only safetensors corpus. [Kernel design §§K.0–K.8](../design/palw/versioned-kernels.md) define kernel-only extension.*

### II.12.1 Define “90 % of local LLMs” before counting

Publish a dated, reproducible frame `L_listed` of publicly discoverable models offered for **local text generation**,
including local chat/reasoning/coding checkpoints, MoE and hybrid models, safetensors and GGUF-only releases,
pre-quantised weights and adapter-plus-base chains. The initial discovery frame is the Hugging Face Hub's pinned
text-generation and GGUF listings plus the local-model registry entries in Ollama's library; add other public
registries with a named snapshot and method, and disclose missing catalogues. HF's [Hub API](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api)
supports model listing and revision metadata; its [GGUF catalogue](https://huggingface.co/docs/hub/gguf) is a
separate format signal. [Ollama's library](https://ollama.com/library) is an additional local-distribution source.
No finite catalogue is literally every model “in the world”; the claim must name the surveyed sources and date.

The counted unit is one pinned model checkpoint or complete base-plus-adapter chain, with its intended local task;
exact mirrors of the same weights collapse by digest, while changed weights remain distinct. Multiple quant files
of one checkpoint are a **format-coverage** stratum, not extra votes in the headline. `L_files` is the subset whose
complete weights and task metadata are publicly readable; inaccessible gated entries, missing files and unresolved
bases stay in `L_listed` with named blockers. Unknown use rights remain in `L_files` and fail the `source` gate as
`RIGHTS_UNCONFIRMED`; they are never silently removed from the denominator. Report both denominators, feature-family
and size strata, and repo-weighted and demand-weighted views. A vision-chat model counts for its advertised full
input task only when the needed RFC-0003 profile exists; a decoder-only extraction is separately labelled.

### II.12.2 Actual kernel routes and counted gaps

For every `L_files` member, run §II.10.3's source → lower → pack → admit → seat → claim/`Final` funnel at its real
size. Name the route `ACTIVE_KERNEL`, `KERNEL_EXTENSION_REQUIRED` or another exact blocker; publish kernel/suite/
plan ids and retain earlier blockers if an upgrade later succeeds. A pass requires a registered whole-task class,
independent capable seats and actual `Final` under the active binary. New probabilistic profiles require approved
encoded/algebraic checks, whole-claim error accounting, positive receipts, DA and the elapsed challenge window,
with bounded exact court on dispute—not routine full segment replay. An inactive proposal, a tiny fixture or a
`Final` for a partial task is not a pass.

The target is **at least 90 % active-kernel passes over `L_files`**, with a one-sided 95 % lower confidence bound at or above
90 % when a probability sample is used. Report the exact numerator, denominator, weights, sample design and date.
Publish the residual extension/resource queue: `KERNEL_EXTENSION_REQUIRED`, `CHECKER_LIMIT`, `COURT_LIMIT`,
`DA_LIMIT`, `SEAT_CAPACITY`, `NEEDS_JOB_PROFILE`, `SOURCE` or the actual first failing gate. Count a later extension
only after activation and the same full-task drill. Unsupported, untested and resource-limited models remain
failures in the denominator; there is no automatic route for “the other 10 %”. Report each kernel family, the
aggregate end-to-end rate and the still-uncovered fraction.

### II.12.3 Engineering order and the upgrade rule

Close frequent format, importer, quantisation, static-feature, real-size admission and seat-throughput failures in
RFC-0002 first. Group missing relations into reusable versioned kernels rather than one extension per model.
Add semantic/checker/court vectors and whole-statement soundness/resource evidence, shadow-test, coordinate
fingerprinted activation, then rerun the cohort. Missing importers stay off chain; missing canonical jobs remain
RFC03 tasks. A heavy operation outside active limits is **not** covered: measure tiling/composition improvements,
then propose a bounded kernel/limit change with checker, DA, court and panel evidence. Do not implement a VM
fallback. A frontend pack cannot change consensus semantics or bypass a cap.

A8 closes with published `L_listed`/`L_files`, ≥90 % active-kernel success meeting the confidence requirement,
real-checkpoint `Final` references, sustained panel evidence and a complete residual table. This is not 100 %
coverage: remaining kernel/resource/source/rights failures prevent an unrestricted coverage claim. RFC11's
broader all-HF target retains `D_all`; neither target implies the other.

## II.13 Immutable challenge-policy binding and later conformance evidence (2026-10-08)

[RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the sole
post-commit challenge protocol. For a new versioned class/profile, bind the active KernelDescriptor digest,
VerificationPlan root, `challenge_policy_id`, program/artifact/tokenizer/input-schema/layout/context commitments
and RFC14 dispute-plan digest before any challenge randomness. Resolve checker/challenge/soundness as the approved
immutable tuple in the [Kernel design](../design/palw/versioned-kernels.md); a declarative plan cannot select weaker randomness, omit relations or upload a checker.

| Immutable identity/statement bindings | Later conformance or activation evidence |
| --- | --- |
| Kernel descriptor, semantic/checker/court/soundness suites, challenge policy, plan and material/layout identities | Actual source references, beacon/lock descriptor, challenge seed, selected queries/vectors, transcript, outcomes |
| Canonical class/profile/candidate id fixed before future work | RFC13 ConformanceCommitment submission provenance and BeaconConformanceEvidence tied to that fixed id |

Do not put an actual beacon or sampled row into class identity: a new beacon does not rehash the same class. Changing
artifact/layout/plan/policy creates the appropriate new class/profile commitment under a separately versioned encoding;
legacy hashes and already-bound claims keep their old meaning. No new codec/hash domain or network activation is
assigned by this document. Claims/receipts/cache keys/material manifests bind the applicable identity and policy.

RFC11 §17 defines Static Admission → Beacon Conformance → Active Eligibility. RegisteredDormant can precede the
beacon, but neither conformance agreement nor an unknown-op sample promotes it to Active. Full semantic/constraint/
court/public-material/resource admission remains mandatory; the beacon selects tests, not semantics. Independent
runtime-pack conformance follows RFC13, while per-claim verification and public prosecution use the same RFC07 policy.
All newly proposed bindings/lifecycle gates need an explicit implementation/migration before use.

## Mission alignment amendment — 2026-10-07

§5、§II.7、§II.10–II.12のadmissionにpublic-verifier dispute completenessを追加する。静的cone/cost検査、kernel catalog、conformance、readyな独立seat、admission jury、FamilyCertifiedだけでは報酬・weightを有効にしない。boundary/history/checkpoint、fused reductionのdissectionとcloseも、fresh non-seat verifierが公開取得した証拠から作れる必要がある。「登録時にadjudicabilityが確定する」はこの追加gateを含む場合に限る。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

canonical TIRの実行statement、実仕事量、承認済みkernel/planは正確に保持する。容量倍率による経済的権利の細分化を、計算量を小さく偽装する意味へ読み替えない。
全reward profileのclaim/block/reward/Final weightは同じproducer資本・期間の予算へ束縛し、正当な計算と公開検証の条件を維持する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

## II.14 Completion contract — 2026-10-10

This revision incorporates the model-extensibility design review. All six requirements below are
part of RFC02 completion, together with the existing A1–A8 criteria. The implementation ledger
records evidence and unresolved work; passing a newly added unit test cannot close the whole RFC.

### II.14.1 Scoped support and the public extension interface

The support record key is **checkpoint revision × complete task × context range × storage format
and arithmetic version × kernel/verification/dispute plan**. Record structure/lowering, actual
weight import, source fidelity, real-size execution, ordinary verification, public localization/
terminal court and independent real-node Final/redemption as distinct stages. Each has measured
evidence, `FAIL`, or `NOT_RUN`; a downstream pass is never inferred. Record partial tasks, fixtures,
and reduced contexts explicitly. They cannot count as real-size/full-task coverage in A6/A8.

Complete both §II.11 paths through the same node binary: independent canonical TIR and a
content-addressed declarative frontend pack. They consume all math-affecting config keys, tensors
and components, use bounded expansion before allocation, and cannot depend on an official signer,
model name, registry entry or compiler. Unknown semantics fail the shared admission gate. Source
equivalence remains unverified until independently reproducible conversion and fidelity pass.

### II.14.2 Bounded dimensions, sparse computation and state

Preserve v1 bytes, meanings and courts. New dimension/index/state semantics require a versioned
program/kernel contract covering the entire execution, checker and court path. Token length,
patch count and audio-frame count have declared maxima and job-committed actual lengths. No claim
or later slice can enlarge them. Sparse access specifies index length/range, duplicate policy,
selection and tie rules, and the binding of selected values to the original state/weights.

Long-context acceptance must derive cost and evidence bounds at the requested length; raising
the v1 history constant alone is insufficient. MoE lowering must preserve selected-expert work;
sparse attention must preserve sparse access; recurrent updates must preserve state reuse. For
any unavoidable asymptotic or constant-factor expansion, report the reason, real-size ratio and
resource failure separately from structural representability. Measure execution, proof creation,
hashing, retention and delivery early, alongside bit-exact backend conformance.

### II.14.3 Storage, arithmetic and fidelity

Storage decoding (including GGUF, MXFP4 and FP8), consensus arithmetic (width, rounding, saturation,
normalization), and backend scheduling are independent identities. Decoding a format does not
establish fidelity; a backend changing committed results requires new semantics, not a tolerance.
Fix the evaluation corpus, contexts, metrics and acceptance thresholds before the run. Measure
logits and task quality, routing agreement for MoE/sparse selection, long-context degradation,
and state saturation/continuity where applicable. Keep failed and missing dimensions visible.

### II.14.4 One derived resource contract

Derive from the admitted program/plan and the committed job dimensions a resource vector for:
maximum execution operations; executor working memory; state/history bytes; evidence-generation
work; ordinary verifier work; on-chain checker work; court bytes/operations/rounds; evidence
retention/delivery; and aggregate concurrent claim/session load. Registration, claim admission,
proof ingestion and court processing must read the applicable same derived contract, rather than
trusting independently declared cheap values. Arithmetic is checked; reject overflow, false
shapes, enormous broadcasts and compiler expansion before their large allocation/work begins.
Per-block, per-bond and network-wide aggregate limits are required in addition to per-claim caps.

An executor's chosen memory budget does not set consensus capacity. Validator/court memory bounds
remain independently enforced even when a producer owns a large GPU. New resource-contract or
limit semantics require an explicit version/fence; legacy admission keeps its exact meaning.

The pre-activation `palw_probabilistic_constraints_v1` route now prices structural claim
admission from the registered commitment envelope and public schema, using the same byte-work
tariff and persisted block budget as public courts. Program, pipeline, segmented and typed
claims check their class envelope before copying traces; registrations must fit admission's
share and a worst court must fit the work reserved for proofs. Onboarding judgements and kernel
objects use the same budget transition. This changes hypothetical admission behind that dormant
fence; it does not change the live legacy route or the frozen TIR/plan grammar. SDK program
preflight applies the same admission/proof-work limits. This is **partial resource accounting**:
the byte tariff does not certify hardware instruction counts, and the complete executor,
evidence-generation, aggregate state/network and semantic checker resource vector remains open.

### II.14.5 Work and economic conservation

Resource bounds, canonical work and economic credit are separate. Apply defined canonicalization
to dead/duplicate/redundant expressions; do not claim to decide semantic usefulness in general.
Preserve work identity across roots, slices, riders and model copies, and charge derived rights
to the same principal and window. Excess PWU, redundant computation, model duplication, splitting
and resubmission cannot enlarge B/R/F ceilings. A faster correct evaluation is valid: claims bind
results, state and constraints, not a count of physical GPU instructions. Test role swapping and
the complete accounting path, not only a work-count helper.

### II.14.6 Whole-statement public verification and completion evidence

A new reusable feature ships as **semantics + type/range rules + costs + ordinary checker +
authenticated evidence binding + localization + terminal court**. Execution-only support cannot
enable rewards. Bind source weights, routing, previous state, KV positions, checkpoint/slice
continuity and outputs across the full graph. Sampling guarantees require whole-claim probability
accounting; primitive court coverage alone does not establish detection probability.

Required adversarial drills include shape/product overflow and huge broadcasts; tiny frontend
expansion bombs; substituted/duplicate experts and weights; reset state, shifted KV and skipped
slices; silently ignored config/tensors; wrong backends and artifact roots; floods of invalid
proofs/courts against an honest producer; duplicate work under different labels; and aggregate
concurrent overload. Attribute invalid requests' costs to the attacker and preserve the honest
producer's opportunity to answer. A fresh non-seat verifier must reach objective conviction using
public authenticated material, and an honest independent participant must reach Final/redemption.

The public onboarding verifier now derives the attempt, bound kernel program, Finals and v3
source seals from one complete op-211 row snapshot. Every page must retain the same DAA, roots,
header and total count; ordered rows, advancing cursors, completeness and both reconstructed
roots are checked before verification. The candidate, descriptor, program, plan and parameter
link must agree. This checks consistency with the served roots, not their canonical-chain
authentication: the caller still needs its node/state-proof trust. The 128 MiB retained wire-byte
ceiling is local reader policy, not a bound on total verifier RAM or a consensus capacity.
Complete-check attempts now dispatch from the same snapshot to exhaustive file replay, with the
chain's domain bounds checked before tensor collection. Pinned pagination on a continuously
advancing node, real-checkpoint public conviction and independent Final/redemption remain required
completion evidence. `palw-class kernel-preflight` also accepts canonical TIR without a frontend
or model configuration, reports the shipping schedule and applies the node's hypothetical bounds
at exactly the requested positions; it grants no registration or reward eligibility.

Use pinned real checkpoints for dense baselines, GDN/KDA and unequal QK/V heads, MoE/routing,
Mamba/hybrid state, sparse/compressed attention and complete multimodal tasks. The review's model
names are discovery candidates, not a supported list; pin revisions and verify their exact required
features before choosing tests. RFC03 supplies encoder/image/audio stage bindings; RFC07/11/14
supply whole-claim verification and G14. Conditional verification (A) remains: obtain the correct
model externally; do not reintroduce model-distribution consensus. Missing source access or a
zero detection floor remains a failed security condition, never repaired by type/cost checks.

#### 14.7 Bounded flat Gather courts and a real-program static regression

The dormant segmented decoder/encoder kernels identify whole-value pricing revision 2 in new
v4/v5 descriptor digests. A rank-one axis-0, unbatched Gather with output shape equal to its index
shape uses allocation-free direct lookup, with the same index refusal, operand reads and exact
result as the reference. Only this evaluator's per-output price changes (32 to 16 abstract units,
then the existing factor 64); authenticated bytes and both commitment hashes retain their prices.
The previous conservative DA16b withholding selection remains fixed. No resource limit, primitive
semantics, v1–v3 descriptor or activation fence changes.

The exact real Qwen2.5 pilot's 32-position TIR is committed as static regression material; its
worst court work is 473,589,248, below the unchanged guaranteed proof reservation 536,870,912.
Its direct TIR route is hypothetically eligible and remains inactive in the shipping schedule.
This closes the pilot's previous static court-price refusal only. A vocabulary-sized synthetic
court and differential/reference/root tests verify the optimization; they do not prove the
real-weight node path, source fidelity, long context, model security or Final/redemption. Evidence,
commands and remaining full-scope gates are in the implementation ledger's
[whole-value Gather pricing revision](../design/palw/tir/rfc0002-implementation.md).

#### 14.8 Generic bounded preparation of real parameter commitments

The SDK and `palw-class kernel-params` prepare the existing segmented-kernel v3 parameter map
from canonical artifact byte ranges without model/config/frontend selection. The raw-width
builder reads every element once and reproduces both commitment trees using checked bounded
buffers; it allocates no complete tensor or i128 expansion. Every model-instance workspace and
the aggregate payload cap are checked before any payload read. Decoder/encoder parameter roles
follow the explicit descriptor memory model; unknown memory models refuse, and v5 job-input slots are
excluded only under encoder semantics. Partial preparation publishes no map.

On the full real Qwen2.5 pilot artifact, all 1,636 instance roots and the complete param map agree
with decoded reference hashing. Streaming measured 57.985 s and 38,223,872-byte maximum process
RSS, versus 223.809 s and 4,131,471,360 bytes for the decoded diagnostic. These local debug
measurements concern producer/verifier **root preparation**, with explicit off-chain caps. They
change no wire preimage, kernel identity, resource price, network activation or economic right.
They do not prove source-model fidelity, execution, public node prosecution or Final. Commands,
bounds, raw evidence and the remaining full-scope work are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).


### 14.9 Exact context and actual-weight node execution evidence

Local executor conformance must compare exactly the requested positive number of positions,
within the canonical program's history bound. It must refuse zero or oversized requests before
reference parameter acquisition, rather than silently substitute a shorter context.

The generic `check-kernel-node-execution` SDK example first checks every actual decoder
parameter instance against a supplied prepared v3 map under explicit off-chain limits, then
compares the node's typed CPU executor with the integer reference. It requires no ModelSpec,
model name or official compiler. This is a local diagnostic, separate from registered-model
public verification and chain admission.

On the full real Qwen2.5 pilot artifact, the predeclared two-position protocol authenticated
all 1,636 instances and compared 790 commit points and both complete vocabulary logits;
every comparison matched. Four node conformance tests passed. Comparison elapsed was
381.881 s; whole-process maximum RSS was 6,442,958,848 bytes. That process includes reference
and native execution plus preparation; it does not establish native producer speed, ordinary
verifier memory bounds, source-model fidelity, full context/task support, public conviction or
Final. The same-node-binary registration and actual-weight public claim/court path remain
required. Protocol, commands, scope, raw results and hashes are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).


### 14.10 Native segmented roots and real delivery-court evidence

A canonical decoder artifact can now produce existing v3 node and segmented roots through the
node's typed executor, without a ModelSpec or compiler registry. The local SDK path rederives
its exact plan and reuses unchanged node static prosecution/carrier/block ceilings. It streams
one successful complete position at a time, checks native values against declared wire types,
and keeps only position roots unless private capture is explicitly requested. Local hash/trace
caps exclude native buffers, mapped weights and caller retention; activation and economic
rights remain independent chain facts.

The fixed real Qwen2.5 lowered pilot completed 32 positions, committing 245,056 node values.
Native computation plus node hashing/capture took 169.916 s; whole-process maximum RSS was
2,143,010,816 bytes. Its public reproduction bundle is 161,675 bytes, with no private position
values or raw weights. A 17,924-byte existing Decode court proof convicts a substituted output
token, while honest delivery is dismissed. A separate offline test consumes only that public
witness to reproduce the outcomes and refuse copied/tampered openings. Eight relevant tests
and five refusal checks passed.

This establishes actual-weight trace construction and offline terminal delivery-court evidence.
It does not establish registration, public RPC acquisition, full computational integrity,
independent source/task fidelity, original full context, outsider node G14 or Final. Same-node
registration/public claim/prosecution/settlement and the remaining full-scope requirements are
still required. Protocol, commands, measurements, raw evidence and hashes are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).


### 14.11 Requested-claim identity and actual-weight node delivery path

An outsider consuming a segmented public record must bind it to the **requested claim id**,
not only the class header and trace roots. The final delivered id is not fed into the trace.
`SegmentedClaimRecordV1::view_for_claim` recomputes the canonical claim id from its job,
producer, all delivered ids and evidence root; it also checks canonical parameter ordering,
prompt authentication, exact output length/vocabulary, context and evidence input binding.
Its header must be anchored in authenticated class/chain state. This local identity check is
not a substitute for chain inclusion or an authenticated RPC state proof.

The actual-weight 32-position Qwen2.5 delivery witness now has a node integration test:
registration data in 32,000-byte ordinary chunks, prompt tiles, salted claim carriers,
production RPC op 210 JSON, outsider proof, block court, reservation slash, honest Final,
coinbase redemption and a second node's replay. Activation validation, parameter attestation
and OPV eligibility are still the parent's explicit test seams. Excluding the eligibility
hook, the class is `NotOnboarded`; the stateful program cannot use the small-stateless complete
check. The separate V2 inventory root is not registered or confused with the kernel root.
These mechanics do not establish model onboarding, public DA acquisition or whole-claim G14.
The full requirement and these gates remain in force.

The segmented bootstrap currently has a cycle: only OPV can register v4/v5, OPV eligibility
requires kernel binding/conformance, and kernel binding requires a registered class. A
conformance-only candidate path must break this cycle **without granting executable claims,
rewards, Final weight or beacon-source rights** before the existing independent gates pass.
The legacy Panel bootstrap is not an allowed fallback for segmented classes. Sampled
conformance remains a non-reward signal pending its digest/refutation courts; its signal or
a test hook cannot substitute for release eligibility.

Allocation-free v3 leaf counts are now used for court byte pricing. Rows and strided columns
retain exactly their existing leaf coverage, wire sizes and tree-choice tie rule, and the
actual 32-position plan root is unchanged. This eliminates transient index vectors created
merely to count elements during repeated ledger reconstruction. No descriptor, price,
commitment preimage, resource ceiling or activation flag changes. Final test evidence and
remaining full-scope work are recorded in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).
