# RFC-0005: PALW ML VM — optional Layer-2 automation over the Model Improvement Protocol: bounded improvement workflows (Phase B, PALW-BVM) and permissionless research programs on a Turing-complete VM (Phase C, PALW-GVM)

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-29 — design only. **Nothing here starts before RFC-0004 (Phase A) is fully implemented** (the user's order of 2026-09-29). Part I is built only if its gate (§I.1.4) opens. Part II's rung A is built when contracts ask for model calls; its rung B only after PALW-TIR is mainnet-safe and workflow demand (M4, §I.1.2) is measured |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 (as RFC-0004 and RFC-0005); merged 2026-09-29 |
| Normative dependencies | **RFC-0004** (the Model Improvement Protocol, which this RFC automates and extends), RFC-0002, RFC-0003 |
| Affects | spec/palw 03 (registry: the program kinds `Bvm` and `Gvm`), new chapters 04d (bounded control) and 04e (the general VM), 05 (canonical work: credited per executed call), 07/08/09 (claims, verification, court: control traces, the bisection game, one-step proofs), 11 (free-prompt lane: workflow jobs), 16 (fences), 17 (RFC-0004's chapter: the EXEC, CRITIC and PROOF verification types); spec/evm (a job precompile and a settlement op) · all networks (dormant until armed) · `consensus/core`, new crates `misaka-palw-bvm` and `misaka-palw-gvm`, `kaspa-evm` (rung A), `misaka-palw-sdk`, a guest toolchain |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | ADR-0135 D7 (a new operation is a protocol upgrade), ADR-0020 (the selected-parent EVM lane), ADR-0089 (the fold is the truth, the EVM is its window and its hand), ADR-0139 (the lanes' gas budget), ADR-0023 (the three-lane proposal), ADR-0144 P1–P7, ADR-0145 (canonical work), ADR-0069 (weight), ADR-0082 and ADR-0103 (dissection, the held regime), ADR-0072 |

## 概要(日本語)

**位置づけ**

- **Layer 2 の RFC。** RFC-0004(Phase A、IR だけの Model Improvement Protocol)が Layer 1。この RFC はその上に載る
  **任意の** VM による自動化で、Part I = Phase B(有界 VM による有界な改良ワークフロー)、Part II = Phase C(Turing 完全な
  VM による permissionless な研究プログラム)。旧 RFC-0004(有界 VM)と旧 RFC-0005(Turing 完全 VM)を 1 本にまとめた。
- **順序。** ユーザーの決定(2026-09-29)により、RFC-0004 を全部実装してから、この RFC の作業を始める。
- **RFC-0004 の規則は変えない。** 候補モデルが VM で作られたことを要求してはならない(RFC-0004 PALW-MIP-1)。VM プログラムが
  作った候補も、VM を使わずに作った候補も、同じ評価と promotion を受ける。この RFC が足すのは、候補と dataset の新しい作り手
  (Phase B・C)と、実行を要する検証の型(Phase C の EXEC・CRITIC・PROOF)だけ。

**Part I — Phase B:有界 VM(PALW-BVM)と有界な改良ワークフロー**

- **用途。** 例:`FOR generation IN 0..8 { dataset を生成; 学習; 評価; 最良を残す }`。データ生成(PALW job)、RFC-0004 の採点
  stage による選別、局所評価、最良の保持を、有界なプログラムとしてチェーン上で検証可能に回す。学習の段は、RFC-0004 §12 の
  IR 学習 profile ができるまでは off-chain で、その結果は通常の候補として RFC-0004 に入る(float の学習は fraud proof できない)。
- **言語。** 制御フローだけの層:IF、静的な最大回数つき FOR、非再帰 CALL、型付きレジスタ、1 要素の `READ`、TIR segment の
  `TCALL`。`while`・再帰・動的確保・生ポインタ・IO は禁止。テンソル演算は常に TIR の evaluator・backend・court に委ねる。
- **登録の梯子(consensus が強制)。** 純 TIR → AOT 展開で TIR → それでも天井を超えるときだけ BVM。有界な制御は表現力を
  増やさない(全展開と if-conversion で必ず TIR に直せる)。変わるのは費用だけで、効くのはモデル間の排他的な大きな分岐。
- **court。** claim の step tree は 2 段:外側の制御 trace(≤ 8 KiB の VM 状態 leaf と call record)と、各 `TCALL` の
  Phase F step tree。外側の食い違いは 1 命令の one-step 証明で、call の中身は既存の TIR court で裁く。
- **測定の gate は残す。** 2026-09-28 の hub の数で、RFC-0002 だけで登録できるのは text-generation リポジトリの約 64 %
  (約 27 万 / 41.6 万、HF 全 310 万件の約 9 %)。未登録の約 36 % は GGUF・事前量子化・parser 未実装などの形式と lowerer の穴で、
  制御フローを要するものは 0。よって VM に帰属する差は今日 0。約 91 % という数は safetensors の decoder checkpoint に対する
  アーキテクチャの被覆率で、64 % の因子の一つにすぎない。

**Part II — Phase C:Turing 完全 VM(PALW-GVM)と permissionless な研究プログラム**

- **用途。** agent、router、tool planner、探索、複数モデルのワークフロー、そして RFC-0004 のための EXEC 検証(コードを
  テストで評価する、実行した反例、証明 checker)。誰でも研究プログラムを登録して走らせ、その成果物を RFC-0004 に候補や
  dataset として出せる。
- **二つの段。** rung A = 既存 EVM レーンを非同期の呼び出し役に使う(PALW job を依頼し、`Final` の後に callback。fee-only)。
  rung B = off-chain の **RISC-V RV32IM** VM と TIR precompile。checkpoint + k-ary 二分探索 + 1 命令の one-step 証明 +
  呼び出し先の TIR court への下降の 4 段で裁く。EVM レーンに同期の TIR precompile を足す案は不可能(全ノードが推論を再実行
  することになる)。WASM は次点。
- **gas と報酬。** gas は状態の一部で、one-step 証明が毎命令検査する。報酬は実行された TIR 呼び出しの仕事量だけで、VM 命令は
  0。外部の応答は oracle transcript として入力になり、真偽は裁かない(P2)。

**工数と推奨**

- **工数。** Part I:約 17〜25 EM、agent 実装 2〜4 週、mainnet-safe 9〜12 か月。Part II:rung A 約 3〜4 EM(mainnet-safe
  6〜9 か月、mainnet の EVM レーン有効化が別に要る)、rung B 約 36〜52 EM(agent 実装 1〜2 か月、**mainnet-safe 24〜30 か月**)、
  RFC-0004 のための EXEC checker が rung B の後に 10〜16 engineer-weeks。どれも RFC-0004 の完了後、PALW-TIR の mainnet-safe 後。
- **推奨。** まず RFC-0004 を完成させる。その後、Part I は gate が開いたときだけ(Part II と両方は作らない)。rung A は
  コントラクト側の需要が出たら。rung B は PALW-TIR が mainnet-safe になり、M4 がワークフロー需要を示してから。

## Summary

This RFC is **Layer 2** of the improvement roadmap: optional automation over RFC-0004's Model
Improvement Protocol, in two parts. **Part I (Phase B)** is a bounded control-flow VM that runs bounded
improvement workflows — `FOR generation IN 0..8 { generate a dataset; train; evaluate; keep the best }`
— with every tensor computation delegated to PALW-TIR. **Part II (Phase C)** is a Turing-complete VM
for permissionless research programs: agents, planners, search and multi-model workflows. It also
brings the verification types that need execution (EXEC, CRITIC and PROOF) into RFC-0004's evaluation.

Neither part changes RFC-0004. A candidate produced by a VM program is evaluated and promoted exactly as
any other, and a candidate produced without one is never disadvantaged (RFC-0004 PALW-MIP-1). The VM
only adds producers of candidates and datasets, and scoring kinds.

- **Part I** keeps the bounded VM's design: structured control, a register file and no memory; a
  consensus-enforced ladder (TIR, then the AOT expansion, then the VM); a one-step control court with a
  descent into the TIR court. It also keeps the measurement gate: bounded control adds no expressiveness
  over TIR, only cost savings.
- **Part II** keeps the two rungs. Rung A uses the EVM lane as an asynchronous orchestrator of PALW jobs.
  Rung B is an off-chain RISC-V RV32IM VM with TIR precompiles, adjudicated by a checkpointed trace, an
  interactive bisection to one instruction, a one-step proof and a descent into the TIR court. The
  substrate evaluation (custom VM, WASM, RISC-V, MIPS, the EVM lane) stands.

Both parts are optional and expensive. Part I costs 17–25 engineer-months and 9–12 months to
mainnet-safe use. Part II's rung B costs 36–52 engineer-months and 24–30 months. **None of it starts
before RFC-0004 is fully implemented.**

## How this RFC relates to RFC-0004

| RFC-0004 (Phase A) provides | Part I (Phase B) adds | Part II (Phase C) adds |
| --- | --- | --- |
| epochs, the candidate registry, evaluation jobs, promotion, rewards | programs that run bounded loops of PALW jobs and scoring stages, and submit their results as candidates or datasets | programs without bounds (gas-metered): agents, search, research pipelines |
| verification types EXACT, LIKELIHOOD, JUDGED, HUMAN | nothing new (control over the same TIR stages) | EXEC (tests and checkers), CRITIC (executed counterexamples), PROOF (a proof checker) — §II.11 |
| artifact kinds Answer, PreferencePair, SyntheticProblem, HardCaseVariant, RewardSignal, Critique | — | TestCase, Counterexample, VerifiedCode, ToolTrace, FormalProof |
| training outside consensus; a possible future IR training profile | a loop may call that profile once it exists (RFC-0004 §12); until then training stays off chain | the same |

Rules that hold throughout:

- A VM program never promotes a head. Only RFC-0004's rule does.
- A VM-produced candidate pays the same fees and faces the same hold-out as any other.
- Removing this RFC, or never arming it, leaves RFC-0004 whole.

---

# Part I — Phase B: the Bounded VM (PALW-BVM) and bounded improvement workflows

## Phase B in one page

A Phase B program is a bounded loop over RFC-0004's own building blocks: PALW jobs of the head, the
scoring stages of RFC-0004 §7.3, and a selection. For example:

```
FOR g < 8 UNTIL stalled {                                     // at most 8 generations
  TCALL rollouts    (Decode, head, prompts from dataset_root, seed_g) → outputs   // PALW jobs of the head
  TCALL score_exact (outputs, keys)                          → scores             // RFC-0004 §7.3 stages
  TCALL select_top  (scores)                                 → dataset_g          // a TopK stage
  TCALL train_step  (dataset_g, adapter_g)                   → adapter_g+1        // only with RFC-0004 §12's profile
  TCALL eval_local  (adapter_g+1, a held-out slice)          → score_g            // the same scoring stages
  IF score_g > best { best := score_g; keep adapter_g+1 }
}
EMIT the best adapter and the datasets                        // submitted to RFC-0004 as a candidate and registered datasets
```

- **What it adds over RFC-0004 alone is modest, and this part says so.** RFC-0004 already verifies
  every rollout and every score individually (`SELF_PLAY` `RewardSignal` artifacts). A Phase B program
  adds verified *selection and composition* — which rollouts were kept, by which rule, in which order —
  and it saves cost where branches are exclusive (§I.0.2).
- **The training step exists only once RFC-0004 §12's IR training profile exists.** Until then, a
  Phase B program automates the data side and the bookkeeping, and training happens off chain between
  epochs. A VM cannot fraud-prove float training.
- **A program never promotes.** Its local evaluation picks what to submit. Only RFC-0004's epoch decides
  the head.
- **The gate stands.** Build Part I only if §I.1.4's gate opens — on model architectures that need
  bounded control, or on measured demand for verified selection that RFC-0004's single jobs cannot give.

Sections §I.0–§I.8 are the bounded VM's design, carried over from the draft of 2026-09-28. Section
numbers in Part I carry the prefix `I.`.

## I.0 Background: what a bounded VM would add to TIR

### I.0.1 What PALW-TIR cannot do, precisely

PALW-TIR v1 is static by design: a program has no program counter, branch, jump, loop or call, and
its only iteration is the scan over positions whose trip count is the job's length (04b
PALW-TIR-18). RFC-0003's pipelines keep that property one level up: stages run in declared order,
each over a trip count fixed at acceptance (PALW-GEN-1). So a computation whose *amount* depends
on data — how many layers a token needs, how many experts it wants, how many refinement rounds it
takes, which of several models should answer — must be written as "compute the maximum, then
select". That is correct and adjudicable: in demand evaluation a `Select` reads only its chosen
operand (04b §9.4). But it is paid in full elsewhere:

- the executor computes every arm that contains a commit point, and commits it;
- every seat that re-executes the claim computes it again;
- admission prices and bounds the **sum** of the arms, not the longest of them;
- a loop unrolled to its maximum is copied into the program, against the 88,000-byte, 2^16-node
  program ceilings of testnet-12 (`PALW_T12_TIR_CEILINGS_V1`).

### I.0.2 The architectures that would want control flow

| # | Pattern | Control shape | Written in TIR today | What a VM would change |
| --- | --- | --- | --- | --- |
| C9 | Early exit (per-token confidence exit after layer `e < L`) | per position: a loop over layers with a data-dependent break | all `L` layers, `Select` freezes the carry after the exit | average compute `≈ E[e]/L`; worst case unchanged (`L`) |
| C10 | Mixture-of-depths (a per-token router skips a block) | per position and block: an `IF` | the block always runs; `Select` passes the residual through | average compute; worst case unchanged |
| C11 | Dynamic expert count (top-p or threshold routing, null experts) | per position: `k(x) ≤ k_max` | `TopK(k_max)` and zero weights for the unused | average; worst case unchanged (`k_max`) |
| C12 | Learned halting, looped depth (ACT/PonderNet-style, recurrent-depth models with an adaptive exit) | a loop of `≤ N` rounds with a data-dependent stop | `N` rounds on the layer schedule, `Select` freezes after the stop | average; worst case unchanged (`N`) |
| C13 | Model calls from models (router over `M` models, a cascade small → large, tool dispatch, draft/verify) | an exclusive choice among large arms | **every** arm runs: `Σ` over arms | **worst case** falls from `Σ arms` to `max arm`; up to `M×` |
| C14 | Iterative refinement with a learned stopping rule (adaptive-step denoising, iterative decoding) | a loop of `≤ S` steps with a data-dependent stop | `S` steps (RFC-0003: one step = one position) | average; worst case unchanged |

Only C13 changes the worst case. C9–C12 and C14 change the average. That distinction drives the
measurement plan and the pricing rule.

### I.0.3 Why an insurance layer should exist on paper

- **The boundary moves with a release.** ADR-0135 D7 makes a new operation a protocol upgrade; new
  *control* is the same. If a control-flow family became popular with no designed path, the
  pressure would be to add it ad hoc — a model-specific kernel again, which RFC-0002 exists to end.
- **A design makes the measurement decidable.** With the language, the expansion and the cost
  model fixed, the tool can say for any model whether the VM would change its verdict, and by how
  much.
- **Versioned program kinds are cheapest to fix now.** Fixing how a class names its execution
  model before a second one exists lets Part II supersede or extend this layer without touching a
  registered class (§I.8).

### I.0.4 The lemma that frames everything: bounded control adds no expressiveness

**Lemma (AOT completeness).** Every admissible BVM v1 program `P` has a finite TIR pipeline
`X(P)` — its *AOT expansion* — whose output equals `P`'s on every job.

*Construction.* Inline every `CALL` (the call graph is acyclic, depth ≤ 16). Unroll every `FOR` to
its static maximum `N`, with an *active* bit per iteration that is 1 until the loop's exit
predicate first holds. If-convert every `IF`: both arms are expanded under their guard, and every
register and handle the arms write is merged with `Select(guard, then, else)`. A `TCALL` becomes a
stage (or, for a single-position segment, inline nodes) whose inputs are the merged handles, and
whose result is discarded by `Select` when its guard is 0. Scalar registers become rank-0 TIR
tensors; `READ` becomes `Gather` over a committed tensor. Every step of the construction preserves
values, and it terminates because every bound is static.

One call mode sits outside the lemma. A `Decode` call that applies RFC-0001's exact selection rule
(§I.3.4) has no pipeline form, because RFC-0003 keeps R's domain 0 out of programs. Greedy decoding
and class-defined sampling do have one — a `Rows` program whose `post` selects with `TopK` and feeds
the id back through a `Fixed` state. A workflow that needs RFC-0001's exact rule inside it is
therefore `NeedsVm(decode_rule)` by construction: a limit of the pipeline format, not of
expressiveness.

**Corollary.** A BVM adds no function that TIR cannot compute. It changes three costs: the
executed and committed work (path versus sum of arms), the admission bound (longest path versus
sum of arms), and the program bytes (a loop body once versus `N` times). Everything below — the
gate, the pricing rule, the recommendation — follows from sizing those three differences.

### I.0.5 Goals and non-goals of Part I

**Goals.**

- GB1. TIR first, as a rule: a model that TIR expresses within the ceilings is a TIR class, whoever
  registers it and however it was written.
- GB2. The VM is control only. Every tensor value is computed by PALW-TIR and adjudicated by the TIR
  court; the VM adds no arithmetic on tensors.
- GB3. Every admissible BVM program is bounded by construction: steps, calls, compute, leaves and
  live handles are derived exactly at registration, at the longest path.
- GB4. Every admissible BVM program is adjudicable by construction: every committed value is
  convictable by one-step recomputation or by the TIR court.
- GB5. Rewards follow verified, executed work (P4, P5); admission bounds follow the worst case.
- GB6. Versioned program kinds: Part II can supersede or extend this layer, and no registered class
  ever changes meaning.
- GB7. Measure before building (§I.1). The gate is fixed before any implementation.

**Non-goals.** Expressiveness beyond TIR (by the lemma there is none to gain). Unbounded loops,
recursion, memory, allocation, pointers, IO, clocks — those are Part II's. New tensor
arithmetic: a missing operation is a prim-set revision (RFC-0002), never a VM feature. Dynamic
shapes. Floating point. Data-dependent admission ceilings. Changes to the ladder, panels, collateral
or the lottery. Sampling (it stays in the FP decode rule, RFC-0001).

## I.1 The measurement plan and the gate

This section is Part I's acceptance test, as RFC-0002 §1 is for PALW-TIR. It is fixed before any
implementation. It needs no consensus work, so it may run at any time, even while RFC-0004 is built.

### I.1.1 Verdicts that separate what a VM can fix from what it cannot

`palw-class check-architecture` (RFC-0002 §8, Phase F §2.11) gains a control verdict:

| Verdict | Meaning | Fixed by |
| --- | --- | --- |
| `ADMISSIBLE`, `ADMISSIBLE_GENERIC` | TIR as it is | — |
| `NOT_LOWERABLE(reason)` | no lowerer parser or template for the family | tooling — **never a VM** |
| `NEEDS_PRIMITIVE(name)` | an operation outside the 25 primitives | a prim-set revision — **never a VM** (GB2) |
| `NEEDS_CONTROL(kind)` → `EXPANDABLE(ρ_w, ρ_a)` | data-dependent control whose AOT expansion passes admission | TIR (the expansion); a VM would only remove the waste `ρ` |
| `NEEDS_CONTROL(kind)` → `NEEDS_VM(limit)` | the expansion breaks a ceiling that the longest path does not | **the only verdict a BVM changes** |
| `EXCEEDS(limit)` | even the longest path breaks a ceiling | neither |

With `X(P)` the AOT expansion: `ρ_w = cost(X(P)) / cost(longest path)` is the worst-case waste and
`ρ_a = cost(X(P)) / E[cost(executed path)]` the average waste over a fixed evaluation set. Cost is
reported twice: MACs per job, and committed lanes per job.

### I.1.2 The corpora

- **M1 — the RFC-0002 corpus as it stands.** The 57 tiny fixtures over 54 `architectures[0]` names,
  which match transformers 5.17, and the refused list of hf-coverage §4 (Llama-4, Gemma-3n,
  GLM/ChatGLM, Phi-3.5-MoE, DBRX, JetMoE, ERNIE-4.5, HunYuan, MiniMax, LFM2, RecurrentGemma, xLSTM,
  DiffLlama, BitNet, RWKV-5/6/7, the Mamba-2 hybrids, …), each assigned a cause.
- **M2 — the control-flow corpus C9–C14** (§I.0.2). Per pattern:
  - a representative public implementation where one exists — identified during the measurement,
    not asserted here;
  - reduced configurations, for bit identity;
  - the path statistics on a fixed evaluation set: executed layers per token, experts per token,
    rounds, steps, and for C13 the branch frequencies.
- **M3 — usage weights.** Hub-wide repository counts were taken on 2026-09-28 (§I.1.5). M3 adds
  downloads and per-`architectures[0]` counts, replacing hf-coverage §2's offline estimate (±30 %
  relative). It **needs network access that the operator approves explicitly**, as the Gate 2
  checkpoints were.
- **M4 — workflows**, shared with Part II: routers, cascades, tool planners and agents, with the
  model calls per task, the branching, the loop bounds and the share of work in the largest arm.

### I.1.3 Metrics

Each is reported by checkpoint count and by downloads, over hf-coverage's decoder-only denominator:

- `cov_TIR`: the share with verdict `ADMISSIBLE*`;
- `cov_AOT = cov_TIR + EXPANDABLE`;
- `cov_BVM = cov_AOT + NEEDS_VM`;
- `gap_lowerer` (`NOT_LOWERABLE`) and `gap_primitive` (`NEEDS_PRIMITIVE`), reported separately —
  **neither counts for the VM**;
- `ρ_w` and `ρ_a` for every `EXPANDABLE` family with at least 1 % usage.

### I.1.4 The gate

Build PALW-BVM only if **all** of the following hold:

1. **A VM-attributable gap.** `cov_BVM − cov_AOT ≥ 3` percentage points by downloads or by count,
   **or** a family with at least 1 % of downloads is `EXPANDABLE` with `ρ_a ≥ 2` — the network would
   pay at least twice the work it needs, at every execution and at every replica.
2. **No cheaper fix.** Guarded TIR (conditional stages, *Alternatives*) does not close the gap. It
   suffices when the control sits between whole segments (C13) and the unrolled guard DAG fits.
3. **No Part II decision pending.** If workflows (M4) are the driver, the decision belongs to
   Part II, not to a bounded layer that Part II would supersede (§I.8).

The user's illustrative thresholds translate as follows (§I.1.5 gives today's measured figures). With `cov_TIR ≈ 92 %`, a VM that lifts coverage to
at least 99 % is justified; with `cov_TIR ≈ 99 %` it hardly is. The refinement here is that only the
VM-attributable part counts: a lowerer gap or a missing primitive raises neither `cov_AOT` nor
`cov_BVM`.

Re-measure at every prim-set revision, at every lowerer release that closes a parser gap, and when
a family with data-dependent control reaches 1 % of new checkpoints in a quarter.

### I.1.5 Today's numbers

| Quantity | Value (2026-09-28) | Source |
| --- | --- | --- |
| Architectures lowered and matching transformers 5.17 | 54 `architectures[0]` names in 57 tiny fixtures; max \|Δlogit\| ≤ 2.8·10^−5 | hf-coverage §3.9 |
| Admitted by `tir_admit_v1` | every fixture and the real configurations, except DeepSeek-V3 at full size (`EXCEEDS`: 2.26·10^12 MACs per position against `2^40`) | tir-lower Gate 2a |
| `cov_TIR`, by repository count | **≈ 64 % of text-generation repositories** (≈ 270,000 of 416,122), which is **≈ 9 % of all 3,102,646 models** on the hub. It counts safetensors text-generation repositories (325,599) × architecture coverage (≈ 0.91) × not pre-quantised (≈ 0.9) | hub filter counts, 2026-09-28 |
| architecture coverage | ≈ 91 % of safetensors decoder-only checkpoints by `architectures[0]` (≈ 92 % with three remote-code families, unverified): **one factor of the line above, not a share of the hub** | hf-coverage §2 |
| the rest of text-generation, ≈ 36 % | GGUF-only and pre-quantised repositories (an importer and the lowerer's quantisation path), weights in other formats, and architectures without a parser — ≈ 7 % of safetensors checkpoints (GLM, hybrids of existing ops, the long tail) | hub counts; hf-coverage §4 |
| `gap_primitive` candidates | a few, each to be confirmed: Llama-4's chunked attention (a window shape; a masked window in v1 today), Gemma-3n's activation sparsity (a top-k-by-value gate; `TopK` exists), xLSTM's exponential gating, BitNet's ternary weights (codes fit `i8`; an artifact question) | hf-coverage §4 |
| `NEEDS_CONTROL` | **0**: no covered or refused decoder needs data-dependent control | this part's reading of hf-coverage §3–4 |
| VM-attributable gap `cov_BVM − cov_AOT` | **0 points** | — |

**Verdict today: the gate is closed.** The roughly 36 % of text-generation repositories that RFC-0002
does not register yet are format, importer, lowerer and primitive gaps, and a VM closes none of them
(GB2).

### I.1.6 Plausible future architectures and their expected verdicts

| Pattern | Expected verdict | Why |
| --- | --- | --- |
| C9 early exit | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ L / E[e]` (about 1.3–2 where exits are frequent) | the expansion is the full model, so a VM saves only the average. The history rows of skipped layers must still be defined — by the model's own rule (propagate the exit state, or compute K and V) — which is a segment either way |
| C10 mixture-of-depths | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ 1 / capacity` (about 2 at 50 %) | a `Select` per block; a skipped position still appends a masked history row |
| C11 dynamic expert count | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ k_max / E[k]` | expert MACs are paid at `k_max` |
| C12 halting, looped depth | `EXPANDABLE` while `N` rounds fit the schedule (at most 1,024 layers); `ρ_a ≈ N / E[n]`. `NEEDS_VM(program)` only when `N` runs into the thousands | the repeated block is one schedule entry per round with global params; only the schedule grows |
| C13 router, cascade or tool dispatch over `M` whole models | often `NEEDS_VM(macs_per_job)`, since the expansion runs all `M`; `ρ_w` up to `M` | **the one pattern whose worst case changes** |
| C14 adaptive steps | `EXPANDABLE`, `ρ_a ≈ S / E[s]` | steps are positions (RFC-0003) |

The gate therefore most plausibly opens on **C13**, and C13 is a workflow pattern — the first rung of
Part II's value case. That is the reason for gate condition 3.

### I.1.7 What the measurement costs

The measurement needs no consensus work:

- a non-consensus prototype of the expander and the control verdicts in `misaka-palw-tir-lower`
  (`check-architecture --control`);
- reduced fixtures for M2;
- the M3 hub survey.

That is about 1–1.5 engineer-months, or 2–3 days of agent time plus the operator's approval for
network access.

## I.2 Architecture: program kinds and the ladder

### I.2.1 Program kinds

```
PalwProgramKindV1 = Tir(1)           // RFC-0002: one TirProgramV1 (Phase F)
                  | TirPipeline(1)   // RFC-0003: a TirPipelineV1 of TirProgramV2 stages
                  | Bvm(1)           // Part I of this RFC
                  | Gvm(1)           // Part II of this RFC
```

- **The kind is in the class id's domain key** (`misaka-palw/<kind>/class-id/v1`), so no two kinds share
  an id and no class ever changes kind.
- **Each kind has its own admission function, step space, court arms and court version**, each behind
  its own fence.
- **A kind is never reinterpreted.** New semantics is a new version of a kind, as `TirProgramV2` is of
  `TirProgramV1`.

### I.2.2 The ladder

```
a model, or a BVM program P
   │
   ├─(1) lowers to TIR within the ceilings? ─────────────────────► TIR class
   │
   ├─(2) X(P) = bvm_expand_v1(P) passes pipeline admission? ────► TIR class of X(P)
   │                                                               (a Bvm registration of P is refused:
   │                                                                ExpandableToTir { graph_ir_root(X(P)) })
   │
   └─(3) otherwise, naming the ceiling X(P) broke ────────────────► Bvm class
```

- **Consensus enforces rung 2.** `bvm_admit_v1` runs the expansion and refuses a BVM registration
  whose expansion is admissible. The tool walks the ladder and registers whichever rung applies, so a
  registrant sees the refusal only if it bypasses the tool.
- **Why enforce it.** GB1 then holds as a rule rather than a hope: the BVM court is exercised only by
  programs that need it, and the network's exposure to a second execution model is exactly the
  measured gap.
- **What enforcing costs.** `bvm_expand_v1` becomes consensus code. Its correctness matters for
  *agreement* — every node must reach the same refusal — and not for the soundness of any verdict. A
  BVM program refused by mistake is a liveness failure. One admitted by mistake is still adjudicable.
- **It is bounded.** The expander never materialises more than the TIR ceilings allow. It stops at the
  first ceiling the expansion passes, and that ceiling is recorded in the class as the witness
  `NeedsVm { limit, value, cap }`. Cost ceilings are evaluated analytically (§I.4.3), without expanding.

### I.2.3 Layering

```
BVM program — control only           admission: bvm_admit_v1 (types, structure, bounds, the ladder)
   │  TCALL = one VM step              court: BvmStep, BvmCall, BvmEnd (one-step transitions)
   ▼
PALW-TIR segments                     admission: tir_admit_v1 / pipeline admission (unchanged)
(TirProgramV2, pipeline stages)       court: TirCone, H dissection, TirLogits, … (Phase F, unchanged)
   ▼
backends (generic, fused)             node software, byte-identical at every commit point
═══════════════════════ only the two upper layers are consensus ═══════════════════════
```

## I.3 The language

### I.3.1 Values and registers

| Kind | Type | Notes |
| --- | --- | --- |
| scalar | `i64` | exact: a result outside `i64` is an error, as under 04b's exact-result rule |
| predicate | `bool` | produced by `CMP`, `AND`, `OR`, `NOT` |
| index | `idx` (`u32`) | `READ` indices and runtime trip counts |
| tensor handle | `T<dtype, shape>` | an immutable reference to a **committed** tensor — a job input or a `TCALL` output — with a static dtype and shape |

- A program declares at most 64 scalar registers and 64 handle registers, each with a static type.
- Scalars start at 0. A handle register starts `Empty`, and the verifier proves that no path reads an
  `Empty` handle.
- **There is no memory.** The register file is the VM's whole mutable state. The *typed state* of the
  brief is this file plus the `Fixed` and `Hist` states inside TIR segments.

### I.3.2 Instructions

| Group | Instructions | Semantics |
| --- | --- | --- |
| scalar | `CONST r, v` · `MOV r, s` · `ADD`/`SUB`/`MUL r, a, b` · `DIV r, a, b, rule` · `MIN`/`MAX` · `CMP r, a, b, cmp` · `AND`/`OR`/`NOT` · `SEL r, p, a, b` | exact integers; `DIV` by 04b §6.4's three rules, a divisor below 1 is an error |
| tensor → scalar | `READ r, h, i` | element `i` (a constant or an `idx` register) of handle `h`, flattened row-major; `i` out of range is an error. **The only way data reaches control** |
| segment | `TCALL seg, (h_in…), (r_in…) → (h_out…)` | run segment `seg` (§I.3.3); one VM step |
| control | `IF p … [ELSE …] END` · `FOR i < N [UNTIL p] … END` · `BREAK_IF p` · `CALL f` · `RET` | structured only: no jump, no computed target |
| end | `EMIT h` · `EMIT_SCALAR r` · `HALT` | name the job's outputs, then stop |

- **`FOR i < N [UNTIL p]`.** `N` is a program constant, `1 ≤ N ≤ 2^16`. The optional `UNTIL p` is
  evaluated before each iteration and ends the loop when it holds: this is the bounded `while`.
  `BREAK_IF` leaves the innermost `FOR`. Loops nest at most 8 deep.
- **`CALL f`.** A function uses a register window the verifier assigns; there are no frames and no
  stack memory. The call graph is a DAG of depth at most 16, so there is no recursion to bound.
- **Errors.** An overflow, a divisor below 1, a `READ` index out of range, or a scalar argument outside
  its segment input's interval ends the job in the terminal state `Faulted(class)`. As in 04b §9.3,
  success versus failure is normative and the class is a label. Admission does not try to prove these
  absent (§I.4.2): a fault is a committed, adjudicable outcome, never a panic.
- **No other effect exists.** No clock, no IO, no message to the chain, and no randomness except R
  (RFC-0003) entering as a segment's `Random` input (§I.3.3).

### I.3.3 Segments and `TCALL`

A **segment** is a TIR program (`TirProgramV2`, RFC-0003) or a pipeline stage. The class's segment
table names each by `graph_ir_root`, with its commitment layout, and the programs ride the
registration as RFC-0003's pipelines do. A `TCALL`:

- binds each `External` input of the segment to a handle register of the same dtype and shape, or to
  a scalar register for a rank-0 input. A scalar outside the input's declared `[lo, hi]` is a runtime
  fault before the segment runs (§I.4.2), so the segment's range analysis always holds;
- runs the segment over its scan, in `Scan` or `Decode` mode (§I.3.4), whose trip count is a segment
  constant or an `idx` register with a static maximum;
- starts from the initial state (every `Fixed` zero, every `Hist` empty): v1 has no segment whose state
  persists across calls (§I.3.4);
- writes its output handles — the segment's output node (`Rows`, `Final` or `Logits`, RFC-0003
  §I.2.3), committed in the callee's own step tree;
- keys every randomness the segment draws — its `Random` inputs, and `Decode` mode's selection — by
  the **per-call seed** `seed_c = H64(key "misaka-palw/bvm/call-seed/v1", seed ‖ le32(ordinal))`, where
  `ordinal` is the `TCALL`'s index in the trace. Every domain's layout, D11's included, stays byte for
  byte what RFC-0003 and RFC-0001 define; only the key material differs per call (open question 5).

A `READ` of a `Logits` handle reads the committed logits row. The VM never implements a sampler:
generation is a `Decode` call (§I.3.4), which applies RFC-0001's rule, and a branch on a token `READ`s
the generated ids, or a `TopK` computed inside a segment (committed by NF-18).

### I.3.4 Granularity: coarse calls in v1, and why per-token control is left to guarded TIR

A v1 `TCALL` runs a whole segment scan from the initial state, in one of two modes:

- **`Scan`** — the segment's positions take their tokens from its inputs (RFC-0003 `TokenRule`s and
  `External` inputs); the output is its `Rows`, `Final` or `Logits` node;
- **`Decode { max_new, stop_ids }`** — a `Logits` segment generates: each position after the prompt
  takes the token RFC-0001 §A's selection rule chose at the previous one, with R's domain 0 under
  the per-call seed `seed_c` (§I.3.3). The output handle is the generated ids (`idx`, length
  `max_new`, padded after the stop). The commitments and court arms are FP V4's and Phase F's
  `TirDecodeToken*`, reused.

v1 has **no sessions** (segments whose state persists across `TCALL`s). Per-token control — C9–C11
and per-token C12 — would need them: one `TCALL` per position and layer group, with a segment's
`Hist` rows spread over thousands of callee trees and an index from positions to calls in the VM
state. That is the most expensive part of any design of this layer, and it buys only average
savings (`ρ_w = 1` for those patterns, §I.1.6). If the measurement shows per-token families with large
`ρ_a` and real usage, the cheaper instrument is **guarded occurrences** in a TIR program version —
a committed guard decides whether an occurrence runs at a position, inside one step tree
(*Alternatives*) — not sessions in a VM. So v1 covers what only a VM covers well: control between
whole model calls (C13), refinement at call granularity (C14) and halting at sequence granularity
(C12).

A single primitive is called as a one-node segment (the tool writes the wrapper program).

## I.4 Static verification: `bvm_admit_v1`

Admission is a pure function of the class's canonical bytes, its segments' admissions and the
network's `palw_bvm_v1` ceilings. Each step refuses by name; the cheapest refusals come first.

### I.4.1 Encoding and normal form

```
BvmProgramV1 {
  version:   u16 = 1,
  bvm_set_id: [u8; 64],              // H64 of the instruction-set descriptor, as prim_set_id (04b §6.0)
  segments:  [SegmentDeclV1],        // { graph_ir_root, layout, mode: Scan | Decode, max_trip }
  scalars:   [ScalarDeclV1],         // ≤ 64: { dtype: i64 | bool | idx }
  handles:   [HandleDeclV1],         // ≤ 64: { dtype, shape }
  inputs:    [InputBindingV1],       // job inputs → handle or scalar registers
  functions: [FunctionV1],           // { window, body: [Instr] }; function 0 is the entry
  outputs:   OutputSpecV1,           // RFC-0003 §I.3.2: kind, shape, meta
}
```

- Canonical Borsh with strict decoding and the re-encode identity, as 04b §4.4; at most 65,536 bytes;
  an unknown opcode or tag is refused (a new instruction is a new BVM version and a new `bvm_set_id`).
- Structured bodies: every `IF`/`FOR` closed by its `END`, `ELSE` only inside an `IF`, `BREAK_IF` only
  inside a `FOR`; `FOR` nesting ≤ 8; the call graph acyclic with depth ≤ 16; no unreachable
  instruction; `HALT` reachable on every path.
- Types: every operand's register type matches its instruction; every `TCALL`'s bindings match the
  segment's `External` inputs in dtype and shape, and its outputs the declared handle types.
- **Definite assignment**: no path reads an `Empty` handle.

### I.4.2 What admission proves and what it leaves to run time

Admission proves structure, types, definite assignment and every bound of §I.4.3. It does **not** try
to prove the absence of scalar overflow, of a `DIV` by a divisor below 1, of a `READ` index out of
range, or of a scalar argument outside its segment input's `[lo, hi]`. Each of those is a defined
**runtime fault**: the job ends in `Faulted(class)`, a committed terminal state that both parties
compute alike and the court adjudicates like any transition. This keeps admission linear and total
without a loop-invariant analysis, and it keeps TIR sound: a `TCALL` whose scalar is out of its
input's interval faults *before* the segment runs, so no segment ever sees a value its range
analysis did not assume.

### I.4.3 Bounds: one dynamic program over the structured body

For every resource `x` — VM steps, `TCALL`s, MACs, transcendentals, committed lanes, inner step
leaves, outer leaves, peak bytes of live handles — two costs are computed bottom-up over the
structured body:

| Construct | Longest path `W(x)` | Sum of arms `A(x)` (for the ladder) |
| --- | --- | --- |
| sequence | `Σ` of parts | `Σ` of parts |
| `IF p A ELSE B` | `1 + max(W(A), W(B))` | `1 + A(A) + A(B)` |
| `FOR i < N … END` | `N · (W(body) + 2) + 1` | `N · (A(body) + 2) + 1` |
| `CALL f` | `W(f) + 1` | `A(f) + 1` |
| `TCALL seg` | 1 step, plus the segment's per-call cost at its maximum trip, from `tir_admit_v1` or pipeline admission | the same |

The constants count VM steps; for every other resource they are 0. `W` is exact for the structured
body and an upper bound for every execution (an `UNTIL` or `BREAK_IF` only shortens a path). The ceilings of the fence value apply to `W`:
`max_vm_steps` (proposed `2^20`), `max_tcalls` (`2^14`), per-job MACs, lanes and inner leaves (the
per-profile ceilings of RFC-0003), `max_outer_leaves`, `max_live_handle_bytes`, and the court window
(§I.6.6). A program over any of them is refused as `Exceeds { limit, value, cap }`.

### I.4.4 The ladder check

`bvm_expand_v1` builds `X(P)` by the construction of §I.0.4 and runs pipeline admission on
it, stopping at the first ceiling it passes. Cost ceilings are compared with `A(x)` analytically, so
the expansion is materialised only when every `A(x)` fits. If `X(P)` is admissible, the registration
is refused `ExpandableToTir { graph_ir_root(X(P)) }`. Otherwise the class records the first broken
ceiling as its witness `NeedsVm { limit, value, cap }` — the fact that justifies its existence, shown
by explorers and the tool.

### I.4.5 The class and its identity

```
PalwBvmClassV1 {
  version:      u16 = 1,
  program:      Vec<u8>,            // canonical BvmProgramV1
  segment_programs: Vec<Vec<u8>>,   // canonical TirProgramV2 bytes, one per SegmentDeclV1 (carried across carriers, RFC-0003 decision 8)
  layout:       PalwBvmLayoutV1,    // outer-leaf tiling; the segments' layouts are in their decls
  tokenizer_id: Hash64,
}
bvm_class_id_v1 = H64(key "misaka-palw/bvm/class-id/v1",
                      H64(program) ‖ H64(segment roots in order) ‖ H64(layout) ‖ artifact_root ‖ tokenizer_id)
```

One artifact (one inventory root) holds every segment's params, as a pipeline's does. The
registration object is an appended `ClassRegisteredBvmV1`, dropped by name below the fence exactly as
Phase F's `ClassRegisteredTirV1` is (A-2 tolerance on older builds). Registration pays the existing
burn and counts against the per-block registration cap.

## I.5 Execution and the step space

### I.5.1 The VM state

```
BvmStateV1 {                         // canonical Borsh, at most 8,192 bytes
  status:   Running | Halted | Faulted(class),
  func: u16, pc: u32,                // the next instruction
  steps: u32, tcalls: u32,           // counters; steps ≤ W(steps)
  loops:    [LoopFrameV1],           // ≤ 8: { counter: u32, bound: u32, head_pc: u32 }
  calls:    [CallFrameV1],           // ≤ 16: { func: u16, return_pc: u32 }
  scalars:  [i64; 64],
  handles:  [HandleV1; 64],          // Empty | Input { root, dtype, shape } | Output { callee_root, output, dtype, shape }
}
bvm_state_digest_v1(σ) = H64(key "misaka-palw/bvm/state/v1", borsh(σ))
```

A step executes one instruction. `TCALL` is one step whose effect writes output handles naming the
callee's execution root. Nothing else is state: segments keep no state across calls in v1 (§I.3.4).

### I.5.2 Two levels of commitment

A BVM claim commits a **two-level step tree**:

- **The outer tree — the control trace**, in execution order:
  - `VmState { k, digest }` after every step `k`;
  - `CallRecord { ordinal, segment, seg_ctx, inputs, scalars, mode, trip, callee_root, callee_leaf_count }`
    at every `TCALL`, placed between the states before and after it;
  - `End { status, output_root }`, last.
- **The inner trees — one per `CallRecord`**: the callee's Phase F step tree (RFC-0003's per stage),
  unchanged, built under the callee's job context
  `seg_ctx = H64(key "misaka-palw/bvm/segment-ctx/v1", bvm_ctx ‖ ordinal ‖ segment root ‖ input roots ‖ scalars ‖ mode ‖ trip)`.
  Its root is `callee_root`, and its `External` inputs are the elements of the input handles, opened
  under their own roots.

```
bvm_execution_root_v1 = H64(key "misaka-palw/bvm/execution-root/v1",
                            ctx_hash ‖ outer_merkle_root ‖ outer_leaf_count ‖ output_root)
```

- **The invariant that makes the tree adjudicable** is Phase F's, one level up: every outer leaf is a
  function of the outer leaves before it (and of the inner trees of earlier `CallRecord`s); every inner
  leaf is a function of the inner leaves before it in its own tree and of its inputs, which are earlier
  callees' outputs or job inputs. The ladder's first divergent leaf is therefore always adjudicable
  from agreed predecessors.
- **The path is data, and the claim states it.** The outer leaf count depends on the path. The static
  bound `W(outer leaves)` sizes the ladder. A trace that is too short or too long diverges from the
  honest one at its end, where `BvmEnd` or `BvmStep` decides it (§I.6.2).
- **Cost.** A `VmState` leaf is 64 bytes, a `CallRecord` under 1 KiB. Even at `2^20` steps the outer
  tree is tens of MB, small beside the inner trees, which cost exactly the executed calls' TIR
  commitments — the saving the VM exists for.
- **Resume** (ADR-0133) restarts from the last `VmState` whose preimage the seat holds, and from the
  inner trees of completed calls.

## I.6 The Control-flow Court

The tensor court (Phase F: `TirCone`, H dissection, `TirLogits`, `TirDecodeToken*`, one-move
accusations) is unchanged. The control court sits above it and asks only three questions: was this
transition of the VM right, was this call made with the right segment and inputs, and did the trace
end where it should. Everything else descends into the tensor court.

### I.6.1 What can be false in a BVM claim

| Lie | Example | Where it is decided |
| --- | --- | --- |
| a VM transition | the wrong branch; a loop counter advanced wrongly or a loop left early or late; wrong register arithmetic; a `READ` value that is not the committed element | `BvmStep` (§I.6.3) |
| a call | a different segment, input handle, scalar, trip count or context than the state before it dictates | `BvmCall` (§I.6.4) |
| a callee's computation | any tensor value inside a segment | the inner tree → the Phase F arms, bound to the callee (§I.6.4) |
| the end | a missing or early `End`; a leaf after `End`; outputs or `output_root` other than the halted state's | `BvmEnd` (§I.6.5) |
| a malformed statement | a `VmState` preimage that is not a well-formed state, a `CallRecord` that is not well formed | convicts the executor on its face (§I.6.7) |

### I.6.2 Locating the lie: the ladder, one level down at a time

- **Bisection chains.** The existing ladder (binary or k-ary, `PALW_BISECT_MAX_ROUNDS = 48`) runs over
  the **outer** leaves and narrows to the first divergent outer leaf `j`; every outer leaf before `j`
  agrees with the challenger's honest trace.
  - `j` is a `VmState` → the terminal is a `BvmStep` close.
  - `j` is an `End`, or one trace has `End` where the other has a `VmState` (a length mismatch) → a
    `BvmEnd` close, or a `BvmStep` close of the transition out of leaf `j − 1`.
  - `j` is a `CallRecord` → first the deterministic fields: if they differ from what state `j − 1`
    dictates, a `BvmCall` close convicts. If they agree and only `callee_root` or `callee_leaf_count`
    differs, the dispute **descends**: the ladder continues over the callee's inner tree (a new phase of
    the same session), narrowing to its first divergent inner leaf, whose terminal is a Phase F close
    (`TirCone`, a dissection root claim, `TirLogits`, `TirDecodeToken*`) under the callee binding.
- **The held regime** (ADR-0103 Decision 1; testnet-12). No bisection is played; the accuser names the
  first divergent leaf in one move, as `TirShardCourtAccused` does:
  - an outer leaf, with its `BvmStep`, `BvmCall` or `BvmEnd` proof;
  - an inner leaf, with the opening of its `CallRecord` under the outer root and the Phase F proof
    under the callee binding. A dissected inner tile opens a session at `Terminal` exactly as RFC-0002
    F7 specifies; the executor's root claim is the first move.

### I.6.3 `BvmStep` — the one-step transition proof

```
BvmStepV1 { binding: PalwBvmStepBindingV1, j: u32,
            pre: BvmStateV1,                          // preimage of the last VmState before leaf j
            record: Option<CallRecordV1>,             // leaf j − 1, when leaf j follows a TCALL
            openings: [leaf],                         // from that VmState to leaf j, adjacent, under the outer root
            read: Option<PalwTirElementOpeningV1> }   // for READ: the element under its handle's root
```

1. Binding: the class's program is put back from `bvm_classes` (the chain never trusts a carried
   copy, as Phase F does for IR programs); the execution root and the outer root are the claim's.
2. `bvm_state_digest_v1(pre)` equals that `VmState` leaf — leaf `j − 1`, or leaf `j − 2` when leaf
   `j − 1` is the `CallRecord` of a `TCALL`. `pre` is well formed (§I.6.7).
3. Execute the one instruction at `pre.pc`. For a `TCALL`, its output handles are the record's
   `callee_root` and output types, and its other fields are checked as `BvmCall` checks them (§I.6.4).
   For `READ`, the element is taken from the opening, verified under the handle's root, and checked
   against its node's proven interval (PALW-TIR-33): an element outside it convicts the executor, who
   committed it.
4. Compare the digest of the resulting state with leaf `j`: different → `ExecutorGuilty`; equal →
   `ChallengerDefeated`.

Its work is one instruction on at most 8 KiB of state and one element opening: a close of a few
kilobytes, carriable by construction (PALW-TIR-38), and cheaper than any tensor terminal. This is the
whole of what the brief calls "was the branch correct, the loop counter, the VM state transition":
each is a field of `pre`, recomputed.

### I.6.4 `BvmCall` and the descent into the tensor court

`BvmCallV1 { binding, j, pre, record }` recomputes, from `pre` and the program, what a `TCALL` at
`pre.pc` must record: the segment, `seg_ctx`, the input handle roots, the scalars (a scalar outside its
interval means the transition is a fault, not a call) and the trip. A mismatch convicts. The state
after the call is a `BvmStep` whose evidence includes the record: its output handles must name
`callee_root`.

The descent binds the callee exactly as a standalone IR execution is bound:

- the callee's step binding is Phase F's `PalwTirStepBindingV1` with the job context `seg_ctx`, the
  segment's class, and `callee_root` as its committed execution root;
- `External` inputs read by a cone are opened under the input handles' roots — RFC-0003's
  "`External` inputs are opened as upstream leaves", with *upstream* meaning any earlier callee or the
  job's input root;
- `Random` inputs are recomputed by the court from the per-call seed `seed_c`, itself derived from the
  claim's job and the record's ordinal (RFC-0003 PALW-RND-2/5);
- the verdict is the Phase F arm's, charged to the BVM claim's executor.

So the tensor court never learns that a VM exists. It adjudicates a TIR execution whose job context
happens to be derived from a control trace.

### I.6.5 `BvmEnd`

The halted state names the outputs. `BvmEnd` checks that leaf `j` is `End` exactly when `pre.status`
is `Halted` or `Faulted`, that no leaf follows it, and that `output_root` is RFC-0003 §I.3.2's digest
of the emitted handles' canonical bytes, tile by tile (the `TirOutputDigestMismatch` pattern: two
openings, one move).

### I.6.6 The window

A dispute may now take outer rounds, then inner rounds, then dissection rounds. Admission checks,
with RFC-0002's O-5 formula extended by one term,

```
(2 · (B_outer + B_inner + R) + t + 1) · D + 2 · 4 · max_close_chunks  <  window_court
```

where `B_outer` is the rounds of the ladder over `W(outer leaves)`, `B_inner` those over the largest
callee's leaves, `R` the dissection rounds, `t` the terminal rounds and `D` the rung window. The k-ary
court is a prerequisite (it already is for `palw_tir_v1`): at `k = 8`, `2^20` outer leaves take 7 rounds
and a `2^32`-leaf callee 11, well inside the 48-round cap.

### I.6.7 Malformed statements: PALW-TIR-33 carried one level up

Every committed outer leaf is the executor's statement. A `VmState` preimage that is not a
well-formed state — a register of the wrong type, a loop counter at or above its bound, a `pc` that is
not an instruction of `func`, an `Empty` handle where definite assignment proved one, `steps` above
`W(steps)` — or a `CallRecord` that is not well formed (a segment not in the table, a trip above its
maximum) is a **malformed commitment**, and it convicts the executor whichever leaf the challenger
disputed (`BvmMalformedState`). A challenger cannot manufacture one: the digest binds the preimage.

### I.6.8 Why a lie is always convictable (informative)

The outer leaves are totally ordered, and each is a deterministic function of the leaves before it, the
job, the params and the inner trees of earlier calls. The honest trace is therefore unique, and a
dishonest one first differs from it at some leaf `j`, all of whose predecessors agree. A wrong branch
first shows at the `VmState` after it, because its `pc` differs; a wrong loop exit shows at the
`VmState` after the `UNTIL` test; a wrong `READ` at the state that holds the read value. Each is
recomputed from the agreed `j − 1` by `BvmStep`. A lie inside a callee leaves the `CallRecord`'s
deterministic fields agreed and its root different, so the descent reaches the first divergent inner
leaf, which Phase F convicts (04b §9.5.7 and Phase F §2.5). An honest executor's every leaf is an
evaluation, so every close acquits it.

### I.6.9 What the control court adds, as objects

- Close proofs appended to `PalwCourtVerdictProofV2`: `BvmStep`, `BvmCall`, `BvmEnd`.
- One ladder phase: `CourtBvmDescended { session, ordinal }`, which moves a session from the outer tree
  to a callee's inner tree.
- One-move accusation variants for the held regime (`BvmShardCourtAccused`).
- One step fault: `BvmMalformedState`.
- One table: `bvm_classes` (the program, segment roots, witness and layout), rooted only once written.

The brief's "Control-flow Court beside the Tensor Court" is thus three small arms, one phase, one
fault and one table. Its size is the smallest part of Part I's cost. The larger part is the
admission logic, the step space and the second implementation that must agree with the first on every
transition (§*Effort*).

## I.7 Work, pricing and the principles

- **Admission bounds the longest path; the claim is credited the path it executed.** A BVM claim's
  credited canonical work is `Σ` over its `CallRecord`s of the callee's structural work (Phase F §2.9,
  RFC-0003) at the call's executed trip. VM steps credit nothing: control is not inference (P1), and
  it is cheap to produce. The calls are in the committed trace, so the credited work is adjudicable — a
  lie about the path is a lie about outer leaves.
- **Why not credit the envelope.** Crediting the longest path pays for work nobody did. That breaks P5
  ("making the same work merely look larger MUST leave them nothing"), and P7 too: a class designed
  with a long envelope and a short typical path would move its economic position against unrelated
  models.
- **Per-job crediting is not new.** A free-prompt job is already credited its canonical work over the
  positions it actually ran (PALW-FP-5), which varies from job to job.
- **The attempt lane** needs a class constant: Phase F's admission checks the canonical job's
  `pwu_per_inference == counted`. A BVM class's canonical job has a data-dependent path. BVM classes
  therefore enter the lanes that credit per job first, and the attempt lane waits for a per-attempt
  pwu design (open question 3). As for any class, weight stays at 0‰ until a family certificate
  covers it (open question 8).
- **Replicas.** Every seat that re-executes a BVM claim runs the executed path, so the network pays
  `(1 + replicas) ×` the path — which is the saving this layer exists for.
- **An observation about TIR itself.** Nothing here proposes to change it. The structural work vector
  counts every node, including both operands of a `Select`. RFC-0003 §II.1.2 lets a backend skip an
  unchosen operand that no committed value depends on. An if-converted arm without commit points is
  therefore **credited as executed and legally skippable** — the same "credited but not done" exposure,
  inside TIR. Rung 2 of the ladder makes such arms common. For RFC-0002's work derivation, this part
  suggests crediting the `max` rather than the `Σ` of `Select`-exclusive uncommitted subgraphs, or
  crediting an arm only through a commit point in it (open question 4).

## I.8 Program kinds, and how Part II supersedes or extends this layer

- **A registered `Bvm(1)` class never changes meaning.** It keeps its admission record and its court
  version (fixed in the fence value) for as long as it has claims.
- **Part II may, without touching such a class:**
  1. add `Gvm(1)` beside `Bvm(1)`;
  2. close new `Bvm(1)` registrations at a fenced height, as legacy admission v9 stays for existing
     lineages while v10 admits new ones;
  3. define `bvm_to_gvm_v1`, a canonical translation of BVM programs into GVM images, so that tooling
     can re-register a BVM program as a GVM class — a new class id beside the old.
- **Compatibility is designed in.** The BVM outer trace is a special case of the GVM's checkpointed
  trace: a checkpoint after every instruction, and a state without memory. `BvmStep` is a GVM one-step
  proof with an empty memory. A `CallRecord` is the GVM's `TIR_CALL` record, and the descent is the
  same. A GVM court could adjudicate BVM classes — but only after a fence and golden-vector
  equivalence, and nothing requires it.
- **Do not build both.** If the gate (§I.1.4) opens on intra-model control, build this layer (or guarded
  TIR). If it opens on workflows, go to Part II, which subsumes this layer.

---

# Part II — Phase C: the Turing-complete VM (PALW-GVM) and permissionless research programs

## Phase C in one page

- **Research programs.** Anyone may register a GVM class — a research program that orchestrates model
  calls, search, tools (through oracle transcripts, in the free-prompt lane only) and bookkeeping — and
  run it as one claim. Its outputs enter RFC-0004 as any producer's do: candidates, registered datasets,
  teaching artifacts. Nothing it produces promotes a head by itself.
- **EXEC, CRITIC and PROOF for RFC-0004.** Rung B brings the verification types that need execution:
  code evaluated by tests, executed counterexamples, and proof checkers (§II.11).
- **The two rungs** are unchanged from the draft of 2026-09-28: rung A, the EVM lane as an asynchronous
  orchestrator (§II.3); rung B, the fraud-proven RV32IM VM (§II.4–§II.7).

Section numbers in Part II carry the prefix `II.`.

## II.0 Background: future computation, not only future models

### II.0.1 Future computation, not only future models

RFC-0002 made a model data. The next frontier is not another architecture but what people build
*from* models:

- **agents** — an LM that plans, calls tools, reads the results and loops until the task is done;
- **routers and cascades** — a small model decides which large one answers, or whether one is needed;
- **tool planners and program executors** — a model writes a program, the program runs, and its
  output feeds the model again;
- **search** — best-of-N with a verifier, tree search with pruning, self-consistency;
- **multi-model pipelines whose structure depends on the data** — retrieval, then reranking, then
  generation, with branches.

What they share: control that is data-dependent with no useful static bound ("until the answer
parses"); memory (assembling contexts, parsing tool output, bookkeeping); many model calls per task;
and glue written in ordinary programming languages.

### II.0.2 What the layers below cannot do

| Layer | Control | Memory | Model calls per task |
| --- | --- | --- | --- |
| PALW-TIR (RFC-0002) | none: a static DAG and a position scan | `Fixed`/`Hist` states only | one program |
| Pipelines (RFC-0003) | stages in a fixed order | edges between stages | a fixed list |
| PALW-BVM (Part I) | bounded `IF`/`FOR`/`CALL` between whole calls | a register file | bounded by static loops |
| **This RFC** | general, metered by gas | byte-addressed, up to 4 GiB | unbounded under gas |

An agent loop that runs until its output parses, manipulating strings in between, is none of the
first three.

### II.0.3 What MISAKA already has

- **PALW**: execution off chain, adjudication on chain; integers only; the court recomputes one cone.
- **The EVM lane** (ADR-0020): every node re-executes it; it is general and gas-priced for L1
  re-execution — Shanghai rules on revm, a chain block ceiling of 390 M gas (30 M plus 120 rounds of
  3 M, ADR-0139). It is live on testnet-11 and testnet-12 from genesis and inert on mainnet.
- **A bridge between them** (ADR-0089): contracts read the PALW fold through precompiles
  (`0x…F010`–`0x…F012`), and a writer (`0x…F013`) escrows value and queues actions that the fold
  settles in the selected child (`MarketSettle`).

A general AI compute platform needs general control *over* PALW's model calls. The two sites of
§II.1 are the two ways to get it from what exists.

### II.0.4 The constraint: ADR-0144

The constitution's P1 says that rewardable inference is the user's own, run on their own machine, and
that no part of the reward path may assume a remote GPU marketplace. P3 says the execution the user
reads is the one the chain rewards. So:

- a workflow the user runs on their own machine — their agent, their tools, their models — is their
  own inference, and claiming it is within P1 and P3;
- a contract paying a remote executor for a model call (rung A) is **fee-for-service**, outside the
  reward path: it earns the fee and no eligibility. Making it a reward path would be a constitutional
  change. This RFC does not propose one.

### II.0.5 Goals and non-goals of Part II

**Goals.**

- GC1. General control — `while`, recursion, memory — metered by gas.
- GC2. Every tensor computation is a TIR precompile: the fast path stays PALW-TIR, adjudicated by the
  TIR court, which never learns that a VM exists.
- GC3. One claim per workflow, whose whole trace is adjudicable: by bisection to one instruction and a
  one-step proof, or by descent into the TIR court at a model call.
- GC4. Determinism: no floating point, no clock, no thread, no IO except committed oracles.
- GC5. Gas soundness: gas bounds the trace, the memory, the executor's work and the dispute game.
- GC6. Composition with the EVM lane, asynchronously, with results delivered only after `Final`.
- GC7. Versioned program kinds: Part I's classes, if any exist, are untouched.
- GC8. Reuse of PALW's court machinery: clocks, bonds, carriers, the ladder and the held regime.

**Non-goals.** Floating point. Parallel or nondeterministic execution. Network or filesystem access
from inside the VM. zk proofs in v1 (§II.2 keeps the door open). Running ML inside the L1 EVM. Replacing
the EVM lane. A remote GPU marketplace as a reward path (ADR-0144 P1). Synchronous TIR precompiles in
the L1 EVM (§II.2).

## II.1 Two execution sites

| | Site A — on chain (the EVM lane) | Site B — off chain, fraud-proven (PALW-GVM) |
| --- | --- | --- |
| Who executes the control | every node, in the selected-parent EVM lane | the executor, as one PALW job |
| How the control is verified | re-execution by every node | bisection and one-step proofs, only when disputed |
| Model calls | asynchronous: each is a PALW job settled at `Final`, then a callback | synchronous: each is a `TIR_CALL` inside the job |
| Latency per model call | one job lifecycle (claim, panel, challenge window) | none beyond the job's own |
| Data between calls | digests and small inline results (block space) | unbounded: held off chain, opened in disputes |
| Cost of control | EVM gas for every node | the executor's and the seats' CPU; gas only meters it |
| New consensus machinery | a precompile, an action and a settlement op (ADR-0089's shape) | a VM, a one-step prover, a bisection game, a descent |
| Fits | contracts that need a few model calls: a classifier gate, a scored oracle, a mint conditioned on a model's answer | agents, planners, search: tens to thousands of calls per task |

The recommendation is to build both, in that order, each when its demand is shown. Rung A is small
and nearly free of new risk. Rung B is the real VM, and it carries the real cost.

RFC-0004 needs neither rung. Contracts on rung A may read its finalized facts as optional add-ons, and
the EXEC checkers it gains in Phase C — tests, graders and tool-trace replays — run in rung B (§II.11).

## II.2 The substrate: evaluation and recommendation

### II.2.1 The criteria

For Site B the substrate must be adjudicable by a **one-step prover that lives in consensus**. Five
criteria follow from that, and two more from who will write the programs:

1. **The machine state and one step must be small.** The prover recomputes one step from a Merkleized
   state. Every register, stack, frame or table the machine has is something a proof must open.
2. **Determinism by construction**, with no implementation-defined corner to pin down.
3. **An independent reference** for the second implementation and for differential testing.
4. **Precedent** in production fault-proof systems.
5. **Fit with TIR**: tensors live outside the VM, behind handles; the VM needs only integer glue.
6. **Toolchains**: workflows are written in Rust, C or anything that compiles to the target —
   including interpreters for other languages, run as guests.
7. **A path to validity proofs** for the control part later, since ML itself will stay optimistic.

### II.2.2 The options

| Option | State and one step | Determinism | Reference / formal model | Fault-proof precedent | Toolchain | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| **EVM lane + synchronous TIR precompiles** | — | — | — | — | Solidity | **impossible**: the L1 lane is re-executed by every node, so a synchronous TIR precompile would make every node run the inference |
| **Off-chain EVM + an EVM one-step prover** | large: 256-bit stack, memory expansion, call frames, the storage trie, precompiles, gas rules | good (Shanghai is pinned) | revm, other clients; formal models exist but are partial | Optimism abandoned its EVM-level fraud-proof design in favour of a MIPS VM running the node's own code | Solidity, Vyper | **no**: the largest prover of all, and a gas schedule built for L1 storage, not for glue |
| **EVM lane, asynchronous** (rung A) | none new: every node re-executes | pinned | revm | — (no fraud proof needed) | Solidity | **yes, for Site A** (§II.3) |
| **Custom VM** (Part I plus `while`, memory and gas) | the smallest possible, tailored to handles | by construction | none: we would write both implementations | none | none: a DSL and a compiler to build and to trust | **no**: every bug is ours, and nobody can write programs for it |
| **WASM** (core, integer subset) | medium-large: value stack, control stack, locals, globals, tables, memory | good once floats are refused and stack limits pinned | the spec interpreter; mechanised semantics exist | Arbitrum compiles WASM into a WASM-derived format designed for one-step proofs | excellent | **second choice** |
| **MIPS32** (Cannon-style) | small: registers, `pc`, memory | good | vendor manuals only | production (OP Stack) | legacy (delay slots, a shrinking ecosystem) | **no**: precedent without a future |
| **RISC-V RV32IM** | small: 32 registers, `pc`, memory; fixed 32-bit instructions | by construction (division by zero and overflow have defined results; no floats; no CSRs in the profile) | **Sail**, RISC-V International's formal golden model, as an oracle | RISC-V fault-proof VMs (Asterisc in the OP Stack ecosystem, Cartesi) and most zkVMs (RISC Zero, SP1, Jolt) | excellent (`riscv32im-unknown-none-elf` in Rust; C, Zig; interpreters as guests) | **recommended** |

### II.2.3 The recommendation: the PALW-RV32IM profile for Site B, the EVM lane for Site A

- **RISC-V RV32IM wins on the criteria that decide consensus risk.** Fewer than fifty instructions
  (RV32I's forty and M's eight), fixed-width decoding, defined results for every input, a state of 32
  registers and a program counter, and an official formal model that serves as an independent
  reference. Its prover is the smallest of the realistic options, and the ecosystem's zkVMs keep open
  a later move of the *control* part to validity proofs.
- **WASM is the honest second.** Its toolchains are as good and its structured control is friendlier to
  analysis, but its machine state (value and control stacks, frames, tables) makes every one-step proof
  larger. Arbitrum's experience is that WASM must first be transformed for provability.
- **The EVM lane is the right Site A, and the wrong Site B.** It already runs contracts that everyone
  re-executes. What it cannot do is run inference, or be fraud-proven without a new EVM prover. Its role
  is orchestration by callback (§II.3).
- **EVM semantics inside Site B, if wanted, come as a guest.** revm compiles to RISC-V — zkVMs run it
  that way to prove Ethereum blocks — so a Solidity workflow can run inside PALW-GVM at an interpreter's
  slowdown. The converse is not possible.

## II.3 Rung A: the EVM lane as an asynchronous orchestrator

The EVM already runs general control, re-executed by every node. What it lacks is a way to ask PALW
for an inference and to act on the answer. ADR-0089 built the same bridge for the model market — read
precompiles, a writer that escrows and queues, and a settlement the selected child carries — and rung A
extends it from trading to inference.

### II.3.1 The flow

```
contract ──call──► 0x…F014 ModelJobs.requestJob{value: fee}(class, input, seed, deadline, callbackGas, valueAtRisk)
                     │ escrows the fee; logs JobRequested (the writer's normalised-record pattern)
                     ▼
PALW fold ── EvmJobRequestedV1 { request_id, requester, class_id, input (inline, ≤ 4 KiB), seed, … }
                     │ any bonded executor claims it; panel; court — exactly as any claim
                     ▼
Final / convicted / deadline ─► the fold decides the settlement
                     ▼
selected child's EVM payload ── system op JobSettled { request_id, Delivered { output_root, inline_result } | Refunded }
                     │ pays the executor (or refunds the requester); stores the result
                     ▼
                  callback(request_id, output_root, inline_result) on the requester, with callbackGas
```

### II.3.2 The rules

- **Registration and fences.** `0x…F014` is a call-frame intercept of the writer's shape, registered
  through `register_all_misaka_precompiles` only past `palw_evm_jobs_v1`. Below the fence it is an
  empty account — the F003 idiom — so execution and state roots are byte-identical.
- **Encoding.** 64-byte ids cross the ABI as two `bytes32` words, high half first (ADR-0089's rule).
  Inputs are inline, at most 4 KiB (token ids, a small tensor, a canonical image thumbnail). A larger
  input needs a data-availability lane, which rung A does not add.
- **The job is a pure function.** The class, the inline input and the requester's seed (RFC-0003 R)
  determine the output. **No oracle transcript** (§II.4.2) is allowed in a requested job: the requester, not
  the executor, chooses every input, so the executor cannot steer the answer.
- **Results only after `Final`.** The callback carries an adjudicated result. A convicted claim is
  re-offered or refunded. A job still open at its deadline is refunded.
- **Value at risk.** A contract may move value on a model's answer. The claim-collateral rule (a false
  claim's largest gain must not exceed what can be recovered) therefore includes the requester's
  declared `valueAtRisk`: a claim on a requested job reserves collateral of at least that amount until
  it is `Final`. A contract that moves more than it declared accepts the residual court risk itself.
- **Fee for service, never a reward path.** The escrowed fee pays the executor. The job earns no quantum,
  ticket or eligibility (ADR-0144 P1: a remote executor serving a contract is a marketplace).
- **Bounded load.** Per-block and per-account caps on requests, as the market's actions have
  (`MAX_MARKET_ACTIONS_PER_EVM_BLOCK`), and a per-request callback gas ceiling inside ADR-0139's budget.
- **Callbacks cannot block settlement.** A reverting callback is recorded. The result stays readable
  through a read precompile (`resultOf(request_id)`), and the fee is settled all the same.

### II.3.3 What rung A is, and is not

It gives contracts model calls with the full PALW guarantee, a workflow language (Solidity) with mature
tooling and audit practice, and composability with the market and DeFi. It adds a precompile, an
object, a settlement op and collateral accounting — about the size of one ADR-0089 decision. It is not a
VM for agents: every model call costs one claim lifecycle of latency, gas at every node, and block space
for results. **Mainnet caveat:** the EVM lane is inert on mainnet (`evm_activation_daa_score = u64::MAX`),
and ADR-0023's precondition — an authoritative incremental EVM state backend before the EVM's load
grows — stands. Rung A on mainnet waits for both.

## II.4 Rung B: the VM — PALW-GVM v1 on the PALW-RV32IM profile

### II.4.1 The machine

- **ISA.** RV32I plus M, at a ratified version named by `isa_id` (the keyed hash of a descriptor, as
  `prim_set_id` is). Nothing else: no C, A, F, D or V extension, and no Zicsr. `FENCE` executes as a
  no-op (one hart, no cache). `FENCE.I`, the CSR instructions, `EBREAK`, `WFI` and every privileged
  instruction end the job in `Faulted(Illegal)`.
- **Registers.** `x0` is hard-wired to 0; `x1`–`x31` are 32-bit; `pc`.
- **Memory.** 2^32 bytes, little-endian, zero except where the image places bytes. Instructions are
  4-byte aligned. A misaligned load or store ends the job in `Faulted(Misaligned)`, so every access
  touches exactly one memory leaf.
- **Defined arithmetic.** RV32M already defines every case: division by zero gives −1 (`DIVU`: 2^32 − 1)
  and a remainder equal to the dividend; `−2^31 / −1` gives `−2^31`, remainder 0. There is no trap and
  no implementation-defined result.
- **Start.** Memory holds the image; `pc` is its entry; `sp` is its stack top; every other register is 0;
  the handle table holds the job's input handles; gas and TIR budget are unused.
- **Ends.** `Halted(code)`, `Faulted(class)`, `OutOfGas`, `OutOfTirBudget`. Every end is a committed,
  adjudicable state; none is a panic.

### II.4.2 System calls (`ECALL`, the number in `a7`)

| # | Call | Effect | Gas | One-step evidence |
| --- | --- | --- | --- | --- |
| 0 | `HALT(code)` | ends the job | 0 | — |
| 1 | `INPUT_READ(off, len, dst)` | copies job input bytes (≤ 4,096 per call) | 64 + len | input tiles opened under `input_root`; the destination range's memory proof |
| 2 | `ORACLE_READ(off, len, dst)` | copies bytes of the claim's **oracle transcript** (below) | 64 + len | tiles opened under `oracle_root`; memory proof |
| 3 | `OUTPUT_WRITE(src, len)` | appends bytes to the output stream | 64 + len | the source range's memory proof |
| 4 | `OUTPUT_TENSOR(h, kind)` | emits a handle as a canonical output (RFC-0003 §I.3) | 64 | the handle's table entry |
| 5 | `TIR_CALL(desc)` | runs a segment (§II.4.3); appends its output handles | 256; the segment's static cost is debited from the TIR budget | the descriptor's memory proof; the `CallRecord` |
| 6 | `TENSOR_READ(h, i)` | `a0` = lane `i` of handle `h` | 64 | the element opened under the handle's root, checked against its proven interval (PALW-TIR-33) |
| 7 | `TENSOR_FROM_MEM(src, dtype, shape)` | a new handle over at most 64 KiB of memory | 64 + len | the source range and its proof; the court re-derives the root |
| 8 | `RAND(coords, dst)` | one digest of R (RFC-0003) in the domain `GVM_UNIFORM_V1`, keyed by the job's seed | 128 | none: the court recomputes it |
| 9 | `GAS_LEFT`, `TIR_BUDGET_LEFT` | the meters | 16 | — |

- **The oracle transcript** is how agents use tools without IO. Everything an agent learns from outside
  — a web page, a tool's answer, a user's reply — is appended by the executor to a transcript, committed
  in the claim as `oracle_root`, and read by offset. The court never judges whether a transcript is
  *true*, exactly as it never judges a prompt (P2). It judges that the computation over it is. A
  transcript is allowed only in a job the user runs for themselves (the free-prompt lane). It is refused
  in a requested job (§II.3.2), where it would let the executor steer the answer.
- **Handles.** `HandleV1 { root, dtype, shape (rank ≤ 4), origin: Input | Call { ordinal, output } |
  Memory { instret } }`. The table is append-only, holds at most 2^16 entries, and is committed as a
  Merkle tree whose root is in the state. Tensor data never enters VM memory except through
  `TENSOR_READ`, one lane at a time, or leaves it except through `TENSOR_FROM_MEM`.

### II.4.3 `TIR_CALL`: the precompile

The descriptor names a segment from the class's segment table, a mode (`Scan`, or `Decode` with
`max_new` and stop ids, as §I.3.4), a trip count, input handles and scalars. A segment is either:

- **a carried TIR program** (`TirProgramV2` or a pipeline), admitted with the class; or
- **a registered TIR class, named by its class id.** A workflow can call models other registrants
  listed; their programs are in `tir_classes` and their weights in their artifacts.

The call runs from the segment's initial state, keys its randomness by a per-call seed
(`H64(key "misaka-palw/gvm/call-seed/v1", seed ‖ le32(ordinal))`, as §I.3.3 does), is committed
as its own Phase F step tree under
`seg_ctx = H64(key "misaka-palw/gvm/segment-ctx/v1", gvm_ctx ‖ ordinal ‖ segment ‖ input roots ‖ scalars ‖ mode ‖ trip)`,
and appends one handle per output. Its static cost at its trip (MACs, lanes, leaves) is debited from the
TIR budget before it runs; a call the budget cannot cover ends the job in `OutOfTirBudget`. A primitive is
called as a one-node segment.

### II.4.4 Memory and state commitments

- **Memory** is a binary Merkle tree over 32-byte leaves, of depth 27 (4 GiB). A node is
  `H64(key "misaka-palw/gvm/mem-node/v1", left ‖ right)`, and a fixed table gives the root of an all-zero
  subtree of every height, so an untouched region costs nothing. A one-leaf proof is 32 bytes and 27
  siblings, about 1.8 KB. A contiguous range is proved by its bytes plus at most two sibling paths.
- **The state** is small enough to open whole:

  ```
  GvmStateV1 { status, pc: u32, regs: [u32; 32], mem_root: Hash64, instret: u64,
               gas_used: u64, gas_limit: u64, tir_used: TirBudgetV1, tir_limit: TirBudgetV1,
               handle_root: Hash64, handle_count: u32, tcalls: u32,
               output_acc: Hash64, output_len: u64 }                     // about 400 bytes
  gvm_state_root_v1(s) = H64(key "misaka-palw/gvm/state/v1", borsh(s))
  ```

### II.4.5 The image, the class and admission

- **The image** `GvmImageV1 { entry, stack_top, segments: [(vaddr, bytes)] }` is canonical (sorted,
  non-overlapping, leaf-aligned). Its initial memory root is `image_root`. Its bytes ride in the class's
  artifact, whose availability is already the condition for serving and verifying any class; the chain
  carries `image_root`, `image_len` and the header. The court never needs the whole image: every
  one-step proof opens the one instruction word it executes.
- **The class:**

  ```
  PalwGvmClassV1 { version, image_root, image_len, entry, stack_top,
                   segments: [SegmentRefV1],         // RegisteredClass { tir_class_id } | Carried { graph_ir_root, layout }
                   layout: { c_vm, arity },          // checkpoint interval (a power of two) and bisection arity
                   gas_limit_max, tir_limit_max, tokenizer_id }
  gvm_class_id_v1 = H64(key "misaka-palw/gvm/class-id/v1", borsh(class) ‖ artifact_root)
  ```

- **Admission** (`gvm_admit_v1`) is short, because a Turing-complete program cannot be bounded
  statically and is not asked to be: canonical decoding; header sanity (the entry and the stack inside
  the image's range, `image_len` within the cap); every carried segment through `tir_admit_v1` or pipeline
  admission; every registered segment existing; `gas_limit_max` and `tir_limit_max` within the network's
  ceilings; `c_vm` in `[2^10, 2^24]`; and the court window (§II.6.6). Gas bounds everything else at run time.

### II.4.6 The gas schedule (v1, calibrated before the fence)

| Item | Gas |
| --- | --- |
| every instruction | 1 |
| the first touch of a 4 KiB page | 4,096 — so a job touches at most `gas_limit / 4,096` pages: 4 GiB at the v1 ceiling |
| system calls | the table of §II.4.2 |
| refunds | none |

v1 ceilings, proposed: `max_gas_per_job = 2^32`; `c_vm = 2^20`; at most `2^14` `TIR_CALL`s per job; the
TIR budget per job within RFC-0003's per-profile ceilings.

## II.5 Execution and the trace commitment

A claim commits a two-level step tree, as Part I's does, with one change: the control trace is
checkpointed rather than recorded at every step.

- **Outer leaves, in execution order:**
  - `Checkpoint { instret, state_root }` at every multiple of `c_vm` instructions, and immediately before
    and after every `TIR_CALL`;
  - `CallRecord { ordinal, segment, seg_ctx, inputs, scalars, mode, trip, callee_root, callee_leaf_count,
    outputs }` between the two checkpoints around each call;
  - `End { status, output_root, oracle_root }`, last.
- **Inner trees:** one Phase F step tree per `CallRecord`, unchanged, built under `seg_ctx`.
- **The execution root:**
  `gvm_execution_root_v1 = H64(key "misaka-palw/gvm/execution-root/v1", ctx_hash ‖ outer_root ‖ outer_leaf_count ‖ output_root ‖ oracle_root)`.
- **Between checkpoints nothing is committed.** Committing every instruction would mean a memory root
  per store: about 27 hashes each, some 10^10 hashes for a 10^9-instruction run. At checkpoints the
  executor rehashes only dirty pages, and it keeps a snapshot at each one, so that it can answer any
  bisection move by re-executing at most `c_vm` instructions.
- **Size.** At the v1 ceilings the outer tree has at most `2^32 / 2^20 + 3 · 2^14 + 1 ≈ 53,000` leaves.
  The inner trees cost exactly the executed calls' TIR commitments.
- **The invariant is Phase F's, again.** Every outer leaf is a function of the outer leaves before it,
  the job, the oracle transcript and the inner trees of earlier calls. Every inner leaf is a function of
  the inner leaves before it and of its inputs. So the first divergent leaf is always adjudicable.
- **Resume** (ADR-0133) restarts from the last checkpoint whose state and dirty pages the seat holds.

## II.6 The court: four levels

```
claim ─ ladder over outer leaves ─┬─ Checkpoint ─► VM bisection (≤ ⌈log_k c_vm⌉ rounds) ─► GvmOneStep (one instruction or one system call)
        (level 1)                 │                 (level 2)                                  (level 3)
                                  ├─ CallRecord ─► GvmCall (its fields) ─┬─ a field differs → convicted
                                  │                                      └─ only the root differs → descend (level 4):
                                  │                                         ladder over the callee's leaves ─► TirCone | dissection | TirLogits | TirDecodeToken*
                                  ├─ Checkpoint after a call ─► GvmCallReturn
                                  └─ End ─► GvmEnd
```

Everything below the descent is Phase F, unchanged. Everything above it is new.

### II.6.1 Level 1: the ladder over the outer leaves

The existing ladder (binary or k-ary, at most 48 rounds) narrows to the first divergent outer leaf `j`,
all of whose predecessors agree with the challenger's honest trace.

- `j` is a `Checkpoint` that follows another checkpoint → level 2, over the instructions between them.
  The initial state, a function of the image and the job, counts as checkpoint zero.
- `j` is the `Checkpoint` right after a `CallRecord` → `GvmCallReturn`: the state after a call is the state
  before it, plus the new handles, `a0` and the meters — one step.
- `j` is a `CallRecord` → `GvmCall` recomputes its deterministic fields from the checkpoint before it
  (the descriptor, read from memory under its proof; the input handles, from the table). A mismatch
  convicts. If only `callee_root` or `callee_leaf_count` differs → level 4.
- `j` is `End`, or one trace ends where the other continues → `GvmEnd`: the status, `output_root` (the
  canonical digest of the output stream and emitted tensors) and `oracle_root`.

### II.6.2 Level 2: the interactive VM bisection

- **The dispute** is an instruction interval `(a, b]` whose start root `S_a` both parties hold and whose
  end root `S_b` the challenger disputes.
- **Responder's move** — `CourtGvmBisected { session, round, roots }`: the executor's state roots at the
  `k − 1` interior points of the pinned cut of `(a, b]` (the `palw_attn_dissect` cut arithmetic).
- **Challenger's move** — `CourtGvmChildChosen { session, round, child }`: a child whose start root it
  accepts and whose end root it disputes. One always exists while the interval's start is agreed and its
  end is not.
- **Rounds**: at most `⌈log_k c_vm⌉` — 7 at `k = 8` and `c_vm = 2^20`. The bottom is one instruction
  `(s, s + 1]`.
- **Clocks and signatures** are ADR-0082's: one rung window per move, and silence loses. Moves are signed
  with ML-DSA-87 under the responder's and challenger's contexts, over messages that open with their own
  domains (`misaka-palw/gvm/bisect/round/v1`, `…/choice/v1`).
- **There is no fold check**, unlike a sum's dissection: intermediate roots cannot be checked against each
  other, and they need not be. A responder that lies somewhere has a first lying root, and the challenger
  can always follow the child that contains it.

### II.6.3 Level 3: the one-step proof

```
GvmOneStepV1 { binding, s,
               pre: GvmStateV1,                       // its root is the agreed S_s
               fetch: (u32 word, MemProofV1),         // the instruction at pre.pc, under pre.mem_root
               access: Option<(leaf, MemProofV1)>,    // the one leaf a load or a store touches
               syscall: Option<SyscallEvidenceV1> }   // §II.4.2's column
```

1. `gvm_state_root_v1(pre)` equals the agreed `S_s`, and `pre` is well formed.
2. The fetched word is proved under `pre.mem_root` at `pre.pc`.
3. Exactly one instruction or system call is executed by the profile's semantics. A store recomputes
   `mem_root` from the same path. Gas and the meters are updated and checked; a step that would pass
   `gas_limit` produces the state `OutOfGas`.
4. `gvm_state_root_v1(post)` is compared with the claimed `S_{s+1}`: different → `ExecutorGuilty`; equal →
   `ChallengerDefeated`.

- **Size.** About 400 bytes of state, two proofs of about 1.8 KB, and at most about 70 KB of evidence
  (`TENSOR_FROM_MEM`'s 64 KiB range): always one carrier (PALW-TIR-38).
- **Either party may file it.** The root `S_s` is agreed, so the challenger can supply the preimage and
  the proofs from its own honest memory.
- **One step function.** The emulator and the prover are one Rust function over a memory trait (full
  memory, or proofs). Cannon and Arbitrum wrote the emulator and the prover in different languages;
  here both are Rust in consensus, which removes that class of divergence. The independent second
  implementation and the Sail oracle guard the shared function itself (§*Effort*).

### II.6.4 Level 4: the descent into the tensor court

The descent is §I.6.4, unchanged. The callee's binding is Phase F's `PalwTirStepBindingV1`, with job
context `seg_ctx`, the segment's class and `callee_root`. `External` inputs are opened under the input
handles' roots: earlier callees, job inputs, or memory-made handles whose roots the one-step proof of
`TENSOR_FROM_MEM` fixed. `Random` inputs are recomputed from the per-call seed. The Phase F arms decide,
charged to the GVM claim's executor. The tensor court never learns that a VM exists.

### II.6.5 The held regime

No bisection is played on testnet-12's held regime (ADR-0103), and the accuser names a leaf in one move.

- A `CallRecord`, a call return or an `End` is decided in that move.
- An inner leaf is decided as §I.6.2 describes.
- A `Checkpoint` cannot be decided in one move, since the instructions between checkpoints are not
  committed. It opens a session at the VM bisection phase, and the executor's intermediate roots are its
  first move — the pattern RFC-0002 F7 set for dissected tiles.

### II.6.6 The window

Admission checks the extended O-5 inequality:

```
(2 · (B_outer + R_vm + B_inner + R) + t + 1) · D + 2 · 4 · max_close_chunks  <  window_court
```

where `R_vm = ⌈log_k c_vm⌉`. At `k = 8`: `B_outer ≤ 6` (about 53,000 leaves), `R_vm = 7`, and `B_inner ≤ 11`
for a `2^32`-leaf callee, plus any dissection rounds — inside the 48-round cap.

### II.6.7 Malformed statements

A `Checkpoint` preimage that is not a well-formed state — `gas_used` above `gas_limit`, a handle count
beyond the table, a running status followed by `End` — or a `CallRecord` naming no segment of the class,
convicts the executor whichever leaf was disputed (`GvmMalformedState`). This is PALW-TIR-33's rule one
level up; the root binds the preimage, so a challenger cannot manufacture one.

### II.6.8 Why a lie is always convictable (informative)

The honest trace is unique: a function of the job, the transcript, the params and the image. A
dishonest claim first differs at some outer leaf with agreed predecessors.

- A checkpoint interval whose end differs contains a first differing instruction. Bisection reaches it,
  because the challenger always follows a child with an agreed start and a disputed end, and the one-step
  proof recomputes it from the agreed state.
- A call whose fields differ is recomputed directly.
- A callee whose root differs contains a first divergent inner leaf, which Phase F convicts (04b §9.5.7,
  Phase F §2.5).

An honest executor's every root and leaf is an evaluation, so every close acquits it.

### II.6.9 What the court adds, as objects

- Close proofs appended to `PalwCourtVerdictProofV2`: `GvmOneStep`, `GvmCall`, `GvmCallReturn`, `GvmEnd`.
- Ladder phases: `CourtGvmBisected`, `CourtGvmChildChosen`, `CourtGvmDescended`.
- A one-move accusation, `GvmShardCourtAccused`.
- A step fault, `GvmMalformedState`.
- Two tables, `gvm_classes` and `gvm_bisections`, rooted only once written.

## II.7 Gas-accounting soundness, denial of service and determinism

### II.7.1 Gas soundness

1. **Termination.** Every instruction costs at least 1 gas, so `instret ≤ gas_limit`. The trace is finite,
   and its bound is known before the job starts.
2. **Checked at every step.** `gas_used` is in the state, and every one-step proof checks both the
   increment and the limit. A trace that runs past its limit has a first step whose honest post-state is
   `OutOfGas` and whose claimed one is not, and that step convicts.
3. **Memory is bounded.** First-touch gas limits a job to `gas_limit / 4,096` pages.
4. **The game is bounded.** At most `gas_limit / c_vm + 3 · tcalls + 1` outer leaves, at most
   `⌈log_k c_vm⌉` VM rounds, and one-step proofs of bounded size (every system call's evidence is capped).
5. **TIR work is bounded separately.** The TIR budget is debited at the segment's static cost before the
   call runs. Inside the call, the callee's own admission bounds hold.
6. **No refunds.** Gas only grows, which keeps 1–4 simple. EVM gas refunds are a known source of
   complexity and bugs, and this VM has no reason to copy them.
7. **What gas does not claim.** It is not the price of inference (TIR work is), not a reward unit, and not a
   bound on a JIT's wall-clock time — only on the reference emulator's work, up to a constant.

### II.7.2 Denial of service

- **Registration** is linear in the carried bytes. The image stays off chain, and segments are admitted as
  TIR.
- **Acceptance** checks that the declared `gas_limit` and TIR budget are within the class's and the
  network's ceilings.
- **Verification.** A seat that re-executes a job pays its gas and its TIR work, both bounded and known
  at acceptance. The VM's share is capped per job, so VM-heavy jobs cannot starve panels (open question 14).
- **The court** has bounded rounds and bounded proofs, and one adjudication slot per block as today.
- **Oracle transcripts** are bounded per job (proposed 16 MiB), committed by root, and never on chain.

### II.7.3 Determinism

- The profile leaves nothing implementation-defined. Every system call is a pure function of the state
  and of committed data. There is one hart, no clock, and no randomness but R.
- An implementation may interpret, translate or JIT, but it must equal the reference step function bit
  for bit at every checkpoint — RFC-0002's F-1 rule, applied to the VM.
- The toolchain is outside consensus. The chain judges the image, not the source: a miscompiled guest is
  its author's problem, as a mis-lowered TIR program is.

## II.8 Work, rewards and the principles

- **Credited work is executed inference, and nothing else.** A GVM claim is credited `Σ` over its
  `CallRecord`s of the callee's structural work at the executed trip — §I.7's rule. VM
  instructions credit nothing: glue is not inference (P1), and it is cheap to multiply. The calls are in
  the committed trace, so the credited work is adjudicable.
- **Lanes.** A user's own workflow, run on their machine, is claimed in the free-prompt lane, which
  already credits per job (PALW-FP-5). The attempt lane is not for GVM classes: a canonical job of a
  Turing-complete program has no static work to price.
- **Gas pays nobody** in the free-prompt lane. It is a declared limit that bounds the trace, the game and
  the seats' verification work.
- **Rung A is fee-only** (§II.3.2).
- **Calls into other registrants' classes are verified use of those classes.** Whether that use counts
  toward the called class's weight or share (ADR-0137) is open question 17.

## II.9 Program kinds, and Part I

- `Gvm(1)` joins the program kinds of §I.2.1. No other kind's class changes.
- **If PALW-BVM exists**, `palw_gvm_v1`'s value carries a height after which new `Bvm(1)` registrations are
  refused. Existing BVM classes keep the BVM court. `bvm_to_gvm_v1` translates a BVM program into a GVM
  image with a small runtime (the register file in memory, `TCALL` as `TIR_CALL`, `READ` as
  `TENSOR_READ`), so tooling can re-register it as a new GVM class beside the old one.
- **If PALW-BVM was never built** — the recommended outcome — there is nothing to do.
- The formats line up by design: a BVM trace is a GVM trace with a checkpoint at every step and no
  memory, and a `CallRecord` and its descent are the same in both.

## II.10 "Strong against future computation": the honest scope

- **Control: anything computable, within gas.** Agents, planners, search, tool use, program execution,
  routers and cascades are all programs.
- **Missing tensor arithmetic gets a slow path, not a fast one.** A guest can read lanes with
  `TENSOR_READ`, compute in RV32IM and write back with `TENSOR_FROM_MEM` (64 KiB per call). That serves
  small operations: a new activation over a 4,096-wide row costs on the order of 10^5 instructions per
  token. It does not serve heavy ones: a new attention variant over 10^4 positions and 32 heads is
  10^9 instructions or more per token. **Heavy new operations still need a TIR prim-set revision.**
- **Data-dependent control inside a model** is expressible, but per-token control through calls pays a
  trace per position. Guarded TIR (*Alternatives*) remains the better tool for it.
- **Future proof systems.** The RISC-V choice leaves open proving the control part with a zkVM later,
  while the ML stays under the optimistic TIR court.

---

## II.11 EXEC, CRITIC and PROOF for the Model Improvement Protocol

Rung B makes three verification types available to RFC-0004. Each is a GVM job, and the court
adjudicates its outcome like any GVM claim's.

| Type | What decides it | Label |
| --- | --- | --- |
| **EXEC** | a checker program — a test harness or a grader — run in PALW-GVM on a subject's or an artifact's output, fraud-proven | V |
| **CRITIC** | a counterexample input on which an artifact or a head output fails, shown by an EXEC run | V |
| **PROOF** | a proof checker, as a GVM guest | V |

With them RFC-0004 gains:

- **artifact kinds**:
  - `TestCase` — EXEC: the reference solution passes it and a known wrong one fails it;
  - `Counterexample` — CRITIC;
  - `VerifiedCode` — EXEC;
  - `ToolTrace` — an EXEC replay over its oracle transcript: the computation is V, the transcript's
    truth T (§II.4.2);
  - `FormalProof` — PROOF;
- **a scoring kind, `Tests`**: a subject's generated code is run against the item's hidden tests in a GVM
  job whose input is the subject claim's final output (RFC-0004's `FinalizedOutput` binding). The result
  is a pass or a fail, fed to the same paired sign test (RFC-0004 §7.5);
- **S1 bounties** for EXEC-verified answers, and critic rewards for valid counterexamples.

### II.11.1 A worked example: hard case #583927, "fix this Rust bug"

- **The case.** A user who opted in asked v12 to fix a failing crate. v12's patch fails the case's
  hidden tests (EXEC, V), so the case enters the line's pool with a bounty.
- **Teacher A** (`LICENSED_DISTILL`, under a `TeacherLicence`) submits an `Answer`, a patch. EXEC runs the
  hidden tests: pass. When the critique window closes with no valid counterexample, A is paid the
  bounty (S1, V), and the patch is admitted.
- **Teacher B** (`OPEN_DISTILL`) submits a patch that fails the tests. B earns nothing, keeps its bond (an
  honest failure is not spam) and pays its verification fee.
- **Teacher C** (`HUMAN`) submits a `Critique` of v12's original patch with a `Counterexample`: an input on
  which that patch panics. EXEC confirms it (CRITIC, V). C earns the critic reward, and the
  counterexample joins the case's tests.
- **The caveat.** The tests run on a binary. That the binary was built from the patch's source by the
  pinned toolchain is B, not V, until compiling inside the GVM is affordable — that needs gas ceilings
  far above v1's `2^32`.

### II.11.2 Effort

About **10–16 engineer-weeks** once rung B exists: checker runtimes as GVM guests (a WASM interpreter, a
scripting-language interpreter, a test harness), the `Tests` scoring kind, the five artifact kinds, and
the pinned-toolchain path for compiled languages. Agents: about 1–2 weeks. Mainnet-safe: rung B's
24–30 months, then about 2–3 months for an audit of the checkers and at least two epochs of code
contests.

---

# Rules, activation, effort and decisions (both parts)

## Proposed Spec text

### Part I — new chapter `spec/palw/04d-bounded-control.md`

Applies past `palw_bvm_v1`. PALW-TIR (04b) and pipelines (04c) are unchanged; segments are ordinary
TIR programs.

- **PALW-BVM-1 (control only).** A BVM class's tensor values MUST be computed by PALW-TIR segments
  only. The VM MUST NOT operate on a tensor except to `READ` one committed element.
- **PALW-BVM-2 (the instruction set).** A BVM program MUST use only the instructions of §I.3.2 under
  `BVM_SET_ID_V1`. No other instruction has consensus meaning.
- **PALW-BVM-3 (bounded structure).** Control MUST be structured. Every `FOR` bound MUST be a constant
  in `[1, 2^16]`, loops MUST nest at most 8 deep, and the call graph MUST be acyclic with depth at most
  16. A program MUST NOT contain recursion, memory, allocation, pointers, IO or a clock.
- **PALW-BVM-4 (canonical form and identity).** Admission MUST refuse bytes that are not the unique
  encoding of a program in normal form. The class id MUST be `bvm_class_id_v1`.
- **PALW-BVM-5 (types).** Every operand MUST have its instruction's type, and no path MAY read an
  `Empty` handle.
- **PALW-BVM-6 (bounds).** The longest-path cost `W(x)` of every resource MUST be within the network's
  ceilings.
- **PALW-BVM-7 (TIR first).** Admission MUST refuse a BVM program whose AOT expansion is admissible as
  TIR (`ExpandableToTir`), and MUST record the witness `NeedsVm` of every program it admits.
- **PALW-BVM-8 (runtime faults).** Overflow, a divisor below 1, a `READ` out of range, or a scalar
  argument outside its segment input's interval MUST end the job in `Faulted(class)`, a committed
  terminal state. Success versus failure is normative; the class is a label.
- **PALW-BVM-9 (calls).** A `TCALL` MUST run its segment from the initial state, in `Scan` mode or in
  `Decode` mode. `Decode` MUST be RFC-0001 §A's selection rule, with R's domain 0 under the per-call seed
  `seed_c`.
- **PALW-BVM-10 (randomness).** Every draw of R inside a call MUST use the per-call seed
  `seed_c = H64(key "misaka-palw/bvm/call-seed/v1", seed ‖ le32(ordinal))`, with each domain's layout
  unchanged. No other randomness MAY enter.
- **PALW-BVM-11 (the trace).** A claim MUST commit the outer tree — a `VmState` after every step, a
  `CallRecord` at every `TCALL`, `End` last — and one Phase F inner tree per call, built under its
  `seg_ctx`.
- **PALW-BVM-12 (one step).** A terminal on a `VmState` MUST recompute one instruction from the
  preimage of the last `VmState` before it and, after a `TCALL`, from that call's `CallRecord`.
- **PALW-BVM-13 (calls in court).** A `CallRecord`'s deterministic fields MUST equal what the preceding
  state dictates. A dispute over its root MUST descend into its inner tree, where it MUST be decided by
  the Phase F arms under the callee binding.
- **PALW-BVM-14 (malformed statements).** A malformed `VmState` preimage or `CallRecord` MUST convict
  the executor.
- **PALW-BVM-15 (the end).** `End` MUST follow a halted or faulted state and nothing else, and it MUST
  be the last leaf. Its `output_root` MUST be PALW-OUT-3's digest of the emitted handles.
- **PALW-BVM-16 (the window).** Admission MUST check §I.6.6's inequality.
- **PALW-BVM-17 (credited work).** A claim's credited work MUST be `Σ` over its calls of the callee's
  structural work at the executed trip. VM steps MUST credit nothing. Admission ceilings MUST use `W`.
- **PALW-BVM-18 (versioned kinds).** A class's program kind and court version MUST NOT change. New
  semantics MUST be a new version of the kind.

### Part II — new chapter `spec/palw/04e-general-vm.md` (applies past `palw_gvm_v1`)

- **PALW-GVM-1 (the machine).** A GVM class's control MUST be the execution of its image on the
  PALW-RV32IM profile named by `isa_id`. No other instruction has consensus meaning.
- **PALW-GVM-2 (defined results).** Every instruction MUST have the profile's result. An illegal instruction
  and a misaligned access MUST end the job in `Faulted(class)`.
- **PALW-GVM-3 (system calls).** Only the system calls of §II.4.2, under `syscall_set_id`, MAY exist.
- **PALW-GVM-4 (tensor arithmetic is TIR).** The VM MUST NOT compute on a tensor except through
  `TENSOR_READ` and `TENSOR_FROM_MEM`. Every `TIR_CALL` MUST be a Phase F execution under its `seg_ctx`.
- **PALW-GVM-5 (gas).** Every instruction MUST cost at least 1 gas under `gas_schedule_id`. There MUST be
  no refund. A step that would pass `gas_limit` MUST produce `OutOfGas`.
- **PALW-GVM-6 (the TIR budget).** A `TIR_CALL` MUST debit its segment's static cost at its trip before
  the segment runs, and MUST produce `OutOfTirBudget` if the budget cannot cover it.
- **PALW-GVM-7 (state and memory).** The state MUST be `GvmStateV1`, and memory MUST be committed by
  §II.4.4's tree.
- **PALW-GVM-8 (the image).** `image_root` MUST be the initial memory root. The image's bytes MUST be in
  the class's artifact.
- **PALW-GVM-9 (canonical form and identity).** Admission MUST refuse bytes that are not a canonical
  encoding. The class id MUST be `gvm_class_id_v1`.
- **PALW-GVM-10 (admission).** Admission MUST check the header, admit every carried segment as TIR,
  resolve every registered segment, apply the network's ceilings and check §II.6.6's inequality.
- **PALW-GVM-11 (the trace).** A claim MUST commit a `Checkpoint` at every multiple of `c_vm` instructions
  and around every `TIR_CALL`, a `CallRecord` for every call, `End` last, and one inner tree per call.
- **PALW-GVM-12 (bisection).** A disputed checkpoint interval MUST be bisected by §II.6.2's game: the pinned
  cut, a responder's roots, a challenger's choice, a clock per move, and silence losing.
- **PALW-GVM-13 (one step).** The bottom MUST be decided by recomputing one instruction or one system call
  from the agreed pre-state.
- **PALW-GVM-14 (calls and descent).** A `CallRecord`'s deterministic fields MUST equal what the state
  before it dictates. A dispute over its root MUST descend into its inner tree and be decided by the
  Phase F arms.
- **PALW-GVM-15 (malformed statements).** A malformed checkpoint preimage or `CallRecord` MUST convict the
  executor.
- **PALW-GVM-16 (oracle transcripts).** A transcript MUST be committed by `oracle_root` and read only by
  offset. No rule MAY judge its truth. It MUST be refused in a requested job (PALW-EVJ-3).
- **PALW-GVM-17 (randomness).** Randomness MUST be R under the per-call seed or the domain
  `GVM_UNIFORM_V1`. Nothing else MAY enter.
- **PALW-GVM-18 (credited work).** A claim's credited work MUST be `Σ` over its calls of the callee's
  structural work at the executed trip. VM steps MUST credit nothing.
- **PALW-GVM-19 (implementations).** Any implementation MUST equal the reference step function at every
  checkpoint.
- **PALW-GVM-20 (versioned kinds).** As PALW-BVM-18.

### Part II — additions to `spec/evm` (applies past `palw_evm_jobs_v1`)

- **PALW-EVJ-1 (the door).** `0x…F014` MUST be registered only past the fence. Below it, it is an empty
  account.
- **PALW-EVJ-2 (the request).** A request MUST escrow its fee, log its normalised record, and carry an
  inline input of at most 4 KiB and a seed.
- **PALW-EVJ-3 (a pure job).** A requested job MUST be a function of its class, its input and its seed. An
  oracle transcript MUST be refused.
- **PALW-EVJ-4 (settlement).** Only the fold's decision, carried as a system op by the selected child, MAY
  settle a request: `Delivered` after `Final`, and `Refunded` after a conviction or at the deadline.
- **PALW-EVJ-5 (callbacks).** A callback MUST follow its settlement. Its failure MUST NOT revert the
  settlement, and the result MUST stay readable.
- **PALW-EVJ-6 (value at risk).** A claim on a requested job MUST reserve collateral of at least the
  request's declared `valueAtRisk` until it is `Final`.
- **PALW-EVJ-7 (not a reward path).** A requested job MUST NOT earn a quantum, a ticket or eligibility.
- **PALW-EVJ-8 (bounded load).** Requests MUST be capped per block and per account, and callback gas MUST
  fit ADR-0139's budget.

## Activation plan

Nothing below starts before RFC-0004 is fully implemented and armed on a testnet (the user's order of
2026-09-29). Each part then has its own gate and its own fence.

### Part I (Phase B): `palw_bvm_v1`

Only if the gate of §I.1.4 opens. The fence has Phase F D1's shape:

```
palw_bvm_v1: Option<PalwBvmFenceV1 { activation, bvm_set_id, court_version, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()` as a whole option, is visited
activation-only by `for_each_fence`, and sits at an unused height. `validate_palw_v2` refuses to arm
it without `palw_tir_v1` (and `palw_gen_v1`, whose `TirProgramV2` segments it calls) at or below it,
without `palw_kary_court`, or on a ruleset without the A-2 tolerance.

| Step | Work | Consensus change | Exit gate |
| --- | --- | --- | --- |
| 0 | The measurement (§I.1) | none | the gate decision, by the user |
| A | Spec chapter 04d; vector plan | none | reviewed text |
| B | Reference interpreter and expander (`misaka-palw-bvm`) | none | golden vectors `consensus-vectors/bvm-v1/`: every instruction, every fault, `W` and `A` of test programs, expansions |
| C | Independent second implementation, from the text only | none | the vectors, and at least 10^6 random programs agreeing on values, faults, bounds and expansions |
| D | Admission `bvm_admit_v1`, `ClassRegisteredBvmV1` | dormant fence | a mutation corpus refused by name; ladder refusals; admission CPU inside the DoS budget |
| E | Step space and court arms | dormant fence | a court battery: a planted lie at every instruction kind, every call field, every end case and inside callees under each Phase F arm, plus malformed states — all convicted; honest runs acquitted |
| F | Node side: executor, trace builder, responder, resume, SDK, `check-architecture --control` | node release | executor equals the reference on the corpora |
| G | Drills | — | D-B1 the court battery on a salted t12 chain; D-B2 the forged-output red-team (8/8) on a BVM class; D-B3 the fence crossing on the shipping binary; D-B4 a real C13 class (a router over two small LMs) from registration to `Final` |
| H | The live testnet | fence at an unused height | registration → panel → `Final`; at least 3 months armed |
| I | Mainnet | fence at genesis or at a flag day | after the audits, the soak and the bounty (§*Effort*) |

### Part II (Phase C), rung A: `palw_evm_jobs_v1`

`Option<PalwEvmJobsFenceV1 { activation, request_caps, callback_gas_ceiling }>`, refused by
`validate_palw_v2` unless the EVM lane, ADR-0089's market fence (`palw_model_evm`) and `palw_tir_v1` are
active at or below it.

| Step | Work | Exit gate |
| --- | --- | --- |
| 1 | spec/evm additions; the precompile, the request object, the settlement op, the collateral reservation | executor ↔ `eth_call` parity (the one registration seam, `register_all_misaka_precompiles`) |
| 2 | Drills: D-EJ1 a contract requests a TIR job and is called back after `Final`; D-EJ2 a convicted claim is re-offered and refunded; D-EJ3 a deadline refund; D-EJ4 value-at-risk collateral held and released; D-EJ5 the fence crossing on the shipping binary | all pass |
| 3 | testnet-12 (or its successor) at an unused height; one audit; at least 3 months armed | no unresolved defect |
| 4 | mainnet | only once the EVM lane is active on mainnet and ADR-0023's state-backend precondition is met |

### Part II (Phase C), rung B: `palw_gvm_v1`

`Option<PalwGvmFenceV1 { activation, isa_id, syscall_set_id, gas_schedule_id, court_version, ceilings,
bvm_admission_closed_at }>`, refused unless `palw_tir_v1`, `palw_gen_v1` and `palw_kary_court` are active
at or below it and the ruleset has the A-2 tolerance.

| Step | Work | Consensus change | Exit gate |
| --- | --- | --- | --- |
| 0 | M4 (§I.1.2): workflow demand | none | the decision to build, by the user |
| A | Spec chapter 04e | none | reviewed text |
| B | Reference emulator with checkpointing (`misaka-palw-gvm`) | none | golden vectors `consensus-vectors/gvm-v1/`: every instruction on edge operands, every system call, gas, every fault |
| C | Independent second implementation; differential against Sail | none | at least 10^9 instruction-level steps against Sail with no disagreement; the vectors |
| D | One-step prover and proof formats | none | the prover equals the emulator on every step of fuzzed programs; hostile proofs refused totally |
| E | Admission, objects, the game, the descent | dormant fence | a court battery: a planted lie at every instruction class, every system call, every call field, every end case and inside callees; delay and silence cases; honest runs acquitted |
| F | Node side and guest SDK | node release | executor equals the reference; the responder answers every move within its rung window |
| G | Drills: D-G1 the court battery on a salted t12 chain; D-G2 the forged-output red-team on a GVM class; D-G3 the fence crossing on the shipping binary; D-G4 an agent (an LM, a tool model and a transcript) end to end; D-G5 a worst-case dispute at the gas ceiling inside the window | — | all pass |
| H | The live testnet, staged: 0‰ weight; gas at most `2^28` and a small TIR budget at first; ceilings raised by fence values after each soak stage | fence | 6–9 months or more with no unresolved court defect |
| I | Mainnet | fence | audits closed, the bounty run, the formal results in |

## Effort to mainnet-safe use

### Calibration

- **What compresses.** RFC-0002 estimated 6–9 months with two to four people to reach its Phase F.
  With agents, Phases A–C passed Gate 1 about half a day after the RFC was drafted, and most of D–G
  landed the same day: Phase F was planned at about 65 agent-days and built in hours by four to six
  agents in parallel. Implementation compresses by one to two orders of magnitude.
- **What does not.** The same day also showed where safety comes from:
  - the independent second implementation found 15 specification defects, one of them a soundness
    bug (a court that would have "recomputed" the disputed value by reading it back);
  - it found an admission disagreement that changed verdicts in 22 cases (A1);
  - drills and review found a court arm that panicked on hostile lanes, registered parameters that
    let a lie end with nobody slashed, fenced court arms that disagree with the model, and a
    wrong-token lie escaping the bisection terminal on bisection chains.

  Each would have been a mainnet incident. Each was found by independent reimplementation,
  differential testing or drills — work whose output is bounded by review and soak time, not by
  typing speed.
- **Outside practice.** Optimistic-rollup fault-proof systems took multiple years from design to
  permissionless mainnet: Optimism's permissionless fault proofs arrived in 2024, after an earlier
  EVM-level design had been abandoned, and Arbitrum's permissionless BoLD in 2025, years after its
  first fraud-proof design. An audit of a consensus component of this size typically takes 4–8 weeks
  per engagement plus a fix-review cycle, and credible soak and bounty windows are measured in months.

**Two things make Part II heavier than Part I.**

- **The prover defines the truth.** A bug in the one-step prover or in the bisection game is not a crash.
  It is a verdict: an honest executor convicted, or a liar acquitted. That is why fault-proof systems
  spend most of their calendar on independent implementations, formal work, audits and staged rollouts.
- **Scale and follow-up.** Optimism's fault proofs came about two years after its MIPS VM began, and needed
  a corrective upgrade within months of launch. Both teams above were larger than two to four people.

### Part I (Phase B) — phases

| Phase | Work | Engineer-months (2–4 experienced people) | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| 0 Measurement | §I.1 tooling, M1–M4, the report | 1–1.5 | 4–6 weeks | 2–3 days, plus the operator's approval for hub access | partly |
| Spec | 04d, vector plan, review | 1.5–2 | 4–6 weeks | 1–2 days to draft | review does not |
| Reference interpreter + expander | `misaka-palw-bvm` | 1–1.5 | 3–5 weeks | 1–2 days | yes |
| Independent second implementation | from the text only | 1–1.5 | 3–5 weeks, in parallel | about 1 day | the writing does; triage of its findings does not |
| Program verifier | normal form, types, the `W`/`A` programs, the ladder, the class id | 1.5–2 | 4–6 weeks | 2–3 days | yes |
| Court arms and step space | outer tree, `BvmStep`/`BvmCall`/`BvmEnd`, descent, one-move, window, objects, table | 2.5–3.5 | 6–10 weeks | 3–5 days | yes |
| Node side | executor, trace builder, responder, resume, SDK, tool | 2–3 | 6–8 weeks | 3–5 days | yes |
| Fuzzing and differential testing | three-way (reference, second implementation, backend), mutation corpus, hostile-input totality, court battery | 2 | 6–10 weeks, then continuous | 2–4 days of harness, weeks of CPU | the CPU does; triage does not |
| Formal methods (targeted) | soundness of the `W` bound; the first-divergence lemma for the two-level tree; determinism of one step; a mechanised VM step (Lean or Coq) checked against the vectors | 1.5–3 | 2–3 months | assists only | mostly not |
| External audits | two engagements (VM + admission; court + integration) and fix review | 1–2 (fixes) | 3–5 months | — | no |
| Testnet soak with fences and drills | dormant, then armed at an unused height; D-B1…D-B4; at least 3 months armed | 1–2 | 3–4 months or more | — | no |
| Bug bounty | scoped to the BVM surface, overlapping the soak | 0.5 | 2–3 months or more | — | no |
| Mainnet activation | fence value, runbooks, rollback plan | 0.5 | about 1 month | — | no |
| **Total** | | **≈ 17–25 engineer-months** | human-only ≈ 12–18 months | implementation ≈ 15–25 agent-days | |

### Part I — the two figures

- **Implementation with AI agents: about 2–4 calendar weeks** from an opened gate to a BVM class
  passing D-B1…D-B4 on a salted testnet-12 chain, with two or three agents and a lead who reviews and
  integrates.
- **Mainnet-safe: about 9–12 months** from an opened gate, whatever the implementation speed. The
  critical path is the triage of the second implementation and of fuzzing (1–2 months), two audits
  with fixes (3–5 months, partly in parallel), at least 3 months armed on a testnet with drills, and a
  bounty window overlapping them. And it starts only **after PALW-TIR itself is mainnet-safe**: a BVM
  class is exactly as sound as the TIR court under it, plus its own arms.

### Part II (Phase C), rung A — the asynchronous EVM orchestrator

| Phase | Work | Engineer-months | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| Spec | spec/evm additions, the object, the settlement | 0.5 | 2–3 weeks | about 1 day | review does not |
| Implementation | precompile, request object, settlement op, collateral reservation, `eth_call` parity | 1–1.5 | 4–6 weeks | 3–5 days | yes |
| Drills and differential testing | D-EJ1…D-EJ5 | 0.5–1 | 3–4 weeks | 2–3 days | mostly |
| External audit | one engagement and fix review | 0.5 | 1.5–2 months | — | no |
| Testnet soak and bounty | at least 3 months armed | 0.5 | 3 months or more | — | no |
| **Total** | | **≈ 3–4 engineer-months** | human-only ≈ 7–10 months | implementation ≈ 1–2 weeks | |

### Part II (Phase C), rung B — PALW-GVM

| Phase | Work | Engineer-months (2–4 experienced people) | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| Spec | ISA profile, system calls, memory, state, gas, the game, the image, the precompile bridge | 3–4 | 2–3 months | 2–4 days to draft | review does not |
| Reference emulator | with checkpointing and snapshots | 2–3 | 6–8 weeks | 2–4 days | yes |
| Independent second implementation | from the text; differential against Sail | 2–3 | 2–3 months | 3–5 days | the writing does; triage does not |
| Program verifier | image and class admission; gas calibration | 1–1.5 | 4–6 weeks | 2–3 days | yes |
| One-step prover | the shared step function over proofs; proof formats | 3–4 | 2–3 months | 3–5 days | yes |
| Court arms | game objects and clocks, checkpoint leaves, descent, held regime, window, bonds | 5–7 | 3–4 months | 1–2 weeks | yes |
| Node side | executor, snapshots, responder, memory cache, guest SDK, the Rust target, TIR bindings, the tool | 6–8 | 3–4 months | 2–3 weeks | yes |
| Fuzzing and differential testing | instruction level against Sail; random programs; proof round trips; adversarial game simulation | 3–4 | 4–6 months, then continuous | about a week of harness, months of CPU | the CPU does; triage does not |
| Formal methods | the step function against the ISA semantics, mechanised against Sail; soundness of memory proofs; termination and liveness of the game | 4–8 | 4–6 months | assists only | mostly not |
| External audits | three or four engagements: emulator and prover; game and court; integration and economics; guest SDK | 3–4 (fixes) | 6–9 months | — | no |
| Testnet soak with staged caps and drills | D-G1…D-G5, the staged ceilings of step H | 2–3 | 6–9 months or more | — | no |
| Bug bounty | large, before and after activation | 1 | at least 6 months before activation | — | no |
| Mainnet activation | staged caps | 1 | 2–3 months | — | no |
| **Total** | | **≈ 36–52 engineer-months** | human-only ≈ 30–40 months | implementation ≈ 40–70 agent-days | |

### Part II — the two figures

- **Implementation with AI agents.** Rung A: about 1–2 calendar weeks to passing drills. Rung B: about
  1–2 calendar months to a GVM class passing D-G1…D-G5 on a salted testnet-12 chain, with three or four
  agents and a lead who reviews and integrates.
- **Mainnet-safe.** Rung A: about 6–9 months, and only once the EVM lane is active on mainnet. Rung B:
  **about 24–30 months** from its start, whatever the implementation speed, and only after PALW-TIR is
  mainnet-safe. The critical path is: implementation (1–2 months), differential and formal work (4–6
  months), three or four audits with fixes (6–9 months, overlapping a staged soak of at least 6–9
  months), a bounty window of at least 6 months before activation, and a staged activation (2–3 months).

### Part II — EXEC checkers for RFC-0004

About 10–16 engineer-weeks once rung B exists (§II.11.2).

## Comparison: TIR only, TIR + Bounded VM, TIR + EVM-class VM

RFC-0004 (Phase A) is the baseline under all three columns: it needs no VM.

| | TIR only (RFC-0002/0003, with RFC-0004) | TIR + Bounded VM (Part I) | TIR + EVM-class VM (Part II) |
| --- | --- | --- | --- |
| Coverage | RFC-0002 alone: ≈ 64 % of text-generation repositories, ≈ 9 % of all hub models (repository counts, 2026-09-28); 100 % of the 54 modelled architectures; the rest are format, lowerer and primitive gaps | + the `NEEDS_VM` share: **0 today**; in future, routing between whole models (C13) and very long loops | + everything computable within gas: agents, tool use, planners, search; a slow path for small missing operations |
| Workflows | static pipelines (RFC-0003) | bounded and data-dependent, between whole model calls | unbounded under gas, with memory and recursion |
| Consensus surface (index; TIR = 1.0 ≈ 14k lines of consensus-path code plus 04b's 1,800 lines) | 1.0 | ≈ 1.3 (+ 4–5k lines, chapter 04d) | ≈ 2.2–2.5 (+ 15–20k lines and a second dispute game) |
| Court complexity | ladder → cone or H dissection (2 levels) | + an outer control trace of one-step transitions over ≤ 8 KiB states, and a descent (3 levels) | + interactive bisection inside checkpoints, one-step RV32IM proofs with memory proofs, syscall and precompile descent (4 levels) |
| Attack surface | interpreter, admission, dissection | + VM interpreter, the bounds program, the ladder's agreement, the step space | + prover ≡ emulator, memory proofs, the gas schedule, game liveness, oracles, EVM callbacks |
| Effort to mainnet-safe, after TIR | TIR's own path; RFC-0004 then adds 8–11 months | + 9–12 months (agents: 2–4 weeks to drill-passing code) | + 24–30 months (agents: 1–2 months); the asynchronous EVM rung alone ≈ 6–9 months |
| Recommendation | build and ship (under way), then RFC-0004 in full | keep this design; build it only if the §I.1 gate opens; never build both BVM and GVM | the asynchronous EVM rung when contracts ask for it; the fraud-proven VM only after TIR is mainnet-safe and M4 shows the demand |

## Alternatives

### Part I

| Alternative | Why not, or when |
| --- | --- |
| **Status quo: predication in TIR** | The default. It costs the waste `ρ` and nothing in consensus; the gate measures whether `ρ` matters |
| **Guarded TIR** — conditional stages in pipelines (a `Switch` over stages) and guarded occurrences in the layer schedule, each guard a committed scalar, with no program counter | The control becomes a finite DAG of guarded segments, and the court adds only a one-move guard check: recompute a predicate from a committed scalar. It is about half Part I's surface, and **the first instrument to try if the gate opens on exclusive choices between whole calls (C13) or on per-token control (C9–C11)**. Its limits: a loop unrolls into guard copies (bytes grow with the iterations), and a guarded-off occurrence needs a defined history row. Where those bind, the BVM is the answer |
| `BoundedScan` / `BoundedMap` in TIR | Rejected by RFC-0002 (PALW-TIR-36), and not data-dependent in any case |
| A WASM or eBPF subset as the control language | Memory, tables, validation rules and a toolchain far beyond some forty control instructions. That is Part II's substrate question, not this layer's |
| Sessions (segments whose state persists across calls) in v1 | §I.3.4: the most expensive part of any design of this layer, for average savings only. Guarded occurrences serve per-token control better |
| Crediting the envelope | §I.7: it pays for work nobody did (P5, P7) |
| Data-dependent admission ceilings | Denial of service: a program must be bounded before anyone runs it |
| Letting registrants choose BVM freely (no enforced ladder) | The VM court would be exercised by programs that do not need it, and GB1 would be a hope rather than a rule |
| Skipping this layer and building Part II directly | Right when workflows are the driver (gate condition 3). Wrong when the driver is intra-model control only: a Turing-complete VM is about 2.5–3× this surface and takes years |
| A model-specific protocol feature (a native "early exit") | Refused by RFC-0002's principle: no model-specific semantics in consensus |

### Part II

| Alternative | Why not, or when |
| --- | --- |
| **Stop at pipelines and, if its gate opens, Part I** | Right if M4 shows no demand for agents and workflows. Nothing in this RFC should be built on speculation |
| **Synchronous TIR precompiles in the L1 EVM** | Impossible: every node re-executes the lane, so every node would run every inference |
| **Off-chain EVM with an EVM one-step prover** | The largest prover of all (256-bit stack, memory expansion, frames, the storage trie, precompiles), and a gas schedule built for L1 storage. Optimism abandoned this path |
| **WASM** | The second choice (§II.2): as good a toolchain, a larger machine state and heavier one-step proofs |
| **A custom VM** | No toolchain, no reference model, and every bug is ours |
| **MIPS (Cannon-style)** | Production precedent, but a legacy ISA without RISC-V's toolchains, formal model or zkVM future |
| **Validity proofs (zkVM) for the whole job** | Proving ML is orders of magnitude costlier than re-executing it (RFC-0002, RFC-0003). The control part alone could move to a zkVM later, and the RISC-V choice keeps that open |
| **Committing every instruction** | About 10^10 hashes for a 10^9-instruction run (§II.5) |
| **Fully lazy commitment, with no checkpoints** | One-move localisation under the held regime would be lost, and every dispute would bisect the whole trace |
| **A remote-compute marketplace as a reward path** | ADR-0144 P1 forbids it. Not proposed; rung A is fee-for-service |
| **Gas refunds, EVM-style** | They complicate soundness (§II.7.1) for no benefit here |
| **Oracle transcripts in requested jobs** | They would let the executor steer the answer the requester pays for (§II.3.2) |

## Security and economic analysis

### Part I

- **Unbounded work.** Every resource is bounded by `W` at admission. A runtime fault terminates the
  job. The expander stops at the ceilings, and admission is linear in the program's bytes plus its
  segments' admissions.
- **Unadjudicable control (A4, one level up).** Every outer leaf has an arm. The one-step close is a
  few kilobytes and always carriable, and the program is referenced by the class, never carried.
- **Near-threshold branch and loop games.** A decision is a `READ` of a committed element — a callee's
  output node, always a commit point — checked against its proven interval (PALW-TIR-33) and compared
  in exact integers. There is no tolerance: a near-threshold confidence is settled by integer arithmetic,
  as `TopK` ties are (ADR-0052 B).
- **Trace-length games.** The end rules and the bound `W` make a short or long trace diverge from the
  honest one at a leaf the court can decide.
- **Credited work.** Path-credited, so nothing is paid for work not done, and a lie about the path is
  convictable.
- **Randomness.** Every coordinate of R is a function of the job: the per-call seed derives from the
  job's seed and the call's ordinal, and the ordinal from the path, which the job determines. The
  executor controls none of them (ADR-0044 F6).
- **Court denial of service.** Round counts are bounded by §I.6.6's check. One-step closes are tiny, and
  the descent reuses Phase F's limits.
- **Availability under the held regime.** The executor must serve outer-leaf preimages as well as inner
  leaves. Silence loses, as today.
- **The expander is consensus code.** If two implementations expand differently, nodes disagree on a
  BVM registration. Hence the expansion vectors, the second implementation and the drill on the shipping
  binary.
- **The real cost is the surface.** A new interpreter whose every transition, fault and bound must be
  bit-identical across implementations; a bounds program; a step space; three arms and a descent.
  Mitigations: a minimal instruction set, no memory, an independent second implementation, fuzzing,
  and a mechanised model of one step.
- **Collateral** rules are unchanged: a claim's collateral must cover the largest gain of a false claim,
  and a BVM class's outputs are outputs like any other.
- **Principles.** P1 and P3: a BVM job is the user's own inference, as with any class. P2: no rule reads
  what a branch means. P4 and P5: work is credited along the verified path. P7: registration stays
  permissionless, and when the ladder refuses a BVM program its refusal names the TIR route.

### Part II

- **The prover is the truth.** A defect in the step function, the memory proofs or the game convicts
  honest executors or acquits liars. Mitigations: a minimal profile; one step function shared by the
  emulator and the prover; an independent second implementation; differential testing against Sail at
  instruction level; a mechanised proof of the step function against the ISA semantics; three or four
  audits; a staged soak with value caps; a large bounty.
- **Emulator and prover divergence** would convict honest executors (unfair) or strand disputes (a
  liveness failure). Sharing the step function removes the cross-language class of divergence; the
  vectors and the Sail differential cover the rest.
- **Game attacks.** Delay is bounded by the round counts and the rung clocks, and silence loses. Griefing
  is priced by the existing accusation charges. Resource exhaustion is bounded by the proof sizes. Several
  challengers follow the existing session rules.
- **Data availability.** The image rides in the artifact, available exactly when weights are. Inputs,
  transcripts and dirty pages are the executor's to serve in a dispute, and silence loses, as today.
- **Oracle transcripts** are inputs, never judged for truth (P2). They are confined to jobs the user runs
  for themselves and refused in requested jobs.
- **Handles cannot be forged.** They come only from job inputs, `TIR_CALL` outputs and `TENSOR_FROM_MEM`,
  and the table is committed.
- **Gas mispricing** could let jobs impose verification costs out of proportion to their declared limits.
  Mitigations: calibration before the fence, per-job ceilings, no refunds, and a cap on the VM's share of a
  job's verification work (open question 14).
- **Memory denial of service** is priced by first-touch gas.
- **Determinism risks** are the profile's to exclude, and it leaves nothing implementation-defined. A JIT
  must match the reference at every checkpoint.
- **Rung A callbacks.** Reentrancy is the contract author's concern, as for any call. Settlement cannot be
  blocked by a failing callback. Results arrive only after `Final`. The value-at-risk reservation brings
  the claim-collateral rule to the value a callback can move.
- **Post-quantum posture.** PALW-GVM and every PALW object are signed with ML-DSA-87. The EVM lane's
  secp256k1 stays confined to that lane (ADR-0020).
- **Economics.** Credited work is executed TIR work only (§II.8). A busy loop earns nothing, and a workflow's
  inference earns what that inference would earn called directly.
- **The principles.** P1 and P3: a user's own workflow is their own inference, and rung A is outside the
  reward path. P2: no rule reads what a transcript or a branch means. P4 and P5: credit follows verified
  executed work. P7: registration stays permissionless, and calls into other registrants' classes are
  verified use of those classes.

## Compatibility and migration

- RFC-0004's classes, epochs and rules are unchanged by this RFC, and so are TIR, pipeline and legacy
  classes. Every fence here is dormant until armed. Every new object is appended, dropped by name below
  its fence and skipped by older builds under A-2.
- Part II's rung A adds one system address, an empty account below its fence (the F003 idiom). The EVM
  lane is inert on mainnet, so rung A on mainnet waits for the lane's own activation there and for
  ADR-0023's precondition.
- If Part I is ever built and Part II later arrives, §II.9 describes how Part I's admissions close and how
  its programs translate.

## Open questions

### Part I (Phase B)

1. **The gate's thresholds**: 3 percentage points of VM-attributable coverage, `ρ_a ≥ 2`, and 1 % usage
   (§I.1.4). A user decision.
2. **Enforce the ladder in consensus** (recommended), or leave rung 2 to tooling and to price.
3. **The attempt lane for BVM classes**: a per-attempt pwu design, or per-job-crediting lanes only at
   first (recommended).
4. **TIR's credited work for `Select`-exclusive, uncommitted arms** — credited as executed, yet legally
   skippable (§I.7). For RFC-0002's owners, independently of this RFC.
5. **R per call**: a per-call seed derived from the job's seed and the call ordinal (recommended; every
   domain's layout, D11's included, stays byte-identical), or a per-call domain.
6. **Per-token control**: guarded occurrences in TIR (recommended), or sessions in BVM v2.
7. **A `VmState` leaf at every step** (recommended; at most tens of MB), or every `C` steps with replay.
8. **Weight (ADR-0069)**: a BVM family certificate drilled once for the instruction set, plus per-class
   readiness, as Phase F decided for TIR (recommended).
9. **Guarded TIR first**: if the gate opens on C13 with few arms, or on per-token control, build guarded
   TIR instead of this layer (recommended).
10. **One of the two, never both**: if the gate opens on workflows, go to Part II (recommended).

### Part II (Phase C)

11. **Build order and evidence**: rung A when contracts ask for it; rung B only after PALW-TIR is
   mainnet-safe and M4 shows workflow demand (recommended).
12. **The ISA**: RV32IM (recommended), or RV64IM — native 64-bit arithmetic, but a larger state, the `W`
   instructions, and a mostly 32-bit zkVM ecosystem.
13. **The checkpoint interval and the arity**: `c_vm = 2^20` and `k = 8` (recommended), to be measured.
14. **Verification cost of the VM part**: a per-job cap on gas relative to TIR work, or a fee to seats.
15. **Oracle transcripts**: free-prompt lane only (recommended), or never.
16. **Image carriage**: in the artifact with the root on chain (recommended), or on chain across carriers.
17. **Calls into registered classes of other registrants**: allowed (recommended); whether that use counts
   toward the called class's weight or share (ADR-0137).
18. **Rewards**: executed TIR work only (recommended); VM steps credit nothing.
19. **Rung A on mainnet**: tied to the EVM lane's mainnet activation and ADR-0023's precondition.
20. **Validity proofs for the control part later**: a zkVM over the same profile, with the TIR court
    unchanged.
21. **Superseding Part I**, if it was built: close BVM admissions at `palw_gvm_v1` and ship
    `bvm_to_gvm_v1`.
22. **Value at risk in rung A**: the declared-cap rule (recommended), or a network-wide cap per request.

## Decision

<Open.> The drafter's recommendation:

- **First, RFC-0004.** Nothing in this RFC starts before RFC-0004 is fully implemented (the user's order).
- **Part I (Phase B)**: build it only if §I.1.4's gate opens. Run the §I.1 measurement at any time: it
  needs no consensus work.
- **Part II (Phase C), rung A**: build it when a contract use case asks for model calls. It is small,
  reuses ADR-0089's machinery, and adds no fraud proof.
- **Part II (Phase C), rung B**: do not start it before PALW-TIR is mainnet-safe and M4 shows the demand.
  When it starts, budget 24–30 months to mainnet-safe use, whatever the implementation speed. Then add
  RFC-0004's EXEC checkers (§II.11).
- **Never both Part I and rung B.** If the workflow case is proven, rung B subsumes the bounded layer.
