# RFC-0004: PALW Bounded ML VM (PALW-BVM) — a bounded control-flow layer over PALW-TIR for what the tensor IR cannot express; TIR stays the protagonist and the VM handles the exceptions

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-28 — design only. **Not to be built until the measurement gate of §1 opens** |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry: a program kind), 04b (unchanged; TIR programs are the callees), new chapter 04d (bounded control), 05 (canonical work: path-credited), 07/08/09 (claims, verification, court: the control trace and the control court), 16 (fences) · all networks (dormant until armed) · `consensus/core`, a new crate `misaka-palw-bvm`, `misaka-palw-sdk`, `palw-class check-architecture` |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | RFC-0002 (PALW-TIR; spec [04b](../spec/palw/04b-tensor-ir.md) on `tir/phase-f-f7`; the Phase F integration design on `tir/phase-f`), RFC-0003 (`TirProgramV2`, pipelines, R, canonical outputs), **RFC-0005** (the Turing-complete VM that may supersede or extend this one), ADR-0135 D7 (a new op is a protocol upgrade), ADR-0144 P1–P7, ADR-0145 (canonical work), ADR-0069 (weight needs e2e adjudicability), ADR-0072 (one inference, one ticket), ADR-0082 (dissection), ADR-0103 (held regime) |

## 概要(日本語)

- **目的。** PALW-TIR(RFC-0002)で書けないモデルのための「保険」。**制御フローだけの層** で、テンソル演算は常に
  TIR の evaluator・backend・court に委ねる。許すのは IF、静的な最大回数つき FOR、非再帰 CALL、型付き状態、TIR segment の
  呼び出し(`TCALL`)。`while`・再帰・動的確保・生ポインタ・IO は禁止。**TIR が主役で、VM は例外処理。**
- **登録の梯子(consensus が強制)。** 純 TIR で書ける → TIR class。書けなければ BVM program を決定論的に AOT 展開
  (CALL の inline、FOR を最大回数まで unroll、IF を if-conversion)して TIR pipeline にし、admission を通れば TIR class として
  登録する(BVM としての登録は `ExpandableToTir` で拒否)。展開が天井を超えるときだけ BVM class になる。
- **正直な結論 1 — 有界な制御は表現力を増やさない。** 上限つき FOR と非再帰 CALL は必ず有限に展開できるので、BVM で
  書けるものは全部 TIR(`Select` による predication)でも書ける(§Motivation 4 の補題)。BVM が変えるのは **費用** だけ:
  (a) 実行・commit・court の費用が「全分岐の和」でなく「実際に通った経路」に比例する、(b) admission の天井が「分岐の和」でなく
  「最長経路」に掛かる、(c) ループ本体を複製しないので program が小さい。効くのは **排他的で大きな分岐**(モデル間の router、
  cascade、tool dispatch)。early exit・mixture-of-depths・動的 expert 数・learned halting は最悪ケースが TIR と同じなので、
  平均費用しか減らない。
- **正直な結論 2 — NEEDS_PRIMITIVE は BVM では解けない。** 数学は TIR に委譲するので、足りない演算は prim set の改訂でしか
  埋まらない。lowerer の parser 不足(`NOT_LOWERABLE`)も tooling の仕事で、VM の仕事ではない。
- **測定が先(§1)。** 今日の HF コーパス(tiny fixture 57 本、`architectures[0]` 54 種)は全部 TIR で表現でき、hub の
  decoder-only checkpoint を件数で約 91 % カバーする。残り約 9 % は parser 未実装と少数の演算候補で、**制御フローを要するものは
  0**。よって「VM に帰属する coverage の差」は今日 0 で、**いま BVM を作る根拠はない**。発動条件(gate)を数値で先に固定する:
  VM に帰属する差が downloads か件数で ≥ 3 ポイント、または利用 ≥ 1 % のファミリーで AOT 展開の平均浪費 ρ_a ≥ 2、
  かつより安い TIR 拡張(guarded stage)では埋まらないこと。ユーザーの例(TIR 92 %・TIR+VM ≥ 99 % なら正当、TIR ≈ 99 % なら
  ほぼ不要)を、lowerer の差と演算の差を除いた「VM に帰属する差」で測る形にした。
- **設計(gate が開いたとき)。** 構造化制御だけのバイトコード。VM 状態は ≤ 8 KiB の固定レコード(pc、ループ枠 ≤ 8、呼び出し枠
  ≤ 16、スカラー 64、tensor handle 64)で、メモリも確保もない。`TCALL` は TIR segment を 1 VM step として呼ぶ。claim の step tree は
  **2 段**:外側 = 制御 trace(VM 状態 leaf と call record)、内側 = 各 `TCALL` の Phase F step tree そのもの。
- **v1 の粒度。** `TCALL` はモデル呼び出し単位だけ(`Scan` と、RFC-0001 の選択規則で生成する `Decode`)。呼び出しを
  またいで状態を持つ session は無い。token ごとの制御(early exit・MoD・動的 expert 数)は最悪ケースが TIR と同じ(ρ_w = 1)
  なので、必要になっても VM の session ではなく **TIR の guarded occurrence**(commit された guard で occurrence を飛ばす)で
  扱う方が安い。BVM v1 が受け持つのは、モデル間の分岐(C13)、呼び出し単位の反復(C14)、系列単位の halting(C12)。
- **Control-flow Court。** 外側で最初に食い違う leaf が VM 状態なら、1 つ前の状態から 1 命令を再計算する one-step 証明で裁く
  (分岐の向き、ループカウンタ、レジスタ演算、`READ` した値 — 値は commit 済み tensor の開示と PALW-TIR-33 の区間検査つき)。
  call record なら呼び先と入力を one-step で確かめ、出力 root だけが違えば内側の step tree に降りて既存の TIR court
  (`TirCone`・H dissection)がそのまま裁く。held regime では名指しの leaf を一手で告発する。壊れた状態 leaf は executor の負け。
- **仕事量と報酬。** admission の天井は最長経路(DoS のため)、**報酬は実際に通った経路**(P4・P5:やっていない仕事に報酬を
  出さない)。経路は commit された trace にあるので争える。副産物として、TIR 自体でも「commit 点のない、選ばれない `Select` の腕」
  が仕事量に数えられる一方で backend は計算を省略できる、という P5 上の論点を見つけた(未決事項 4)。
- **費用(正直に)。** consensus 面は TIR を 1.0 として約 +0.3(4〜5 千行)。court は 3 段(制御 trace → segment → cone/dissection)。
  新しいインタプリタ・境界解析・court arm・step space を独立の第 2 実装で確かめる必要がある。
- **工数。** agent による実装は 2〜4 週間(drill 通過まで)。**mainnet-safe は 9〜12 か月**(監査・soak・bounty は縮まない)で、
  しかも PALW-TIR 自体が mainnet-safe になった後。
- **RFC-0005 との関係。** class の program kind を版付き(`Tir` / `TirPipeline` / `Bvm` / `Gvm`)にし、登録済みの BVM class は
  永久に BVM court で裁く。RFC-0005 は fence で新規 BVM 登録を閉じ、正準変換で GVM に移せる。**BVM と GVM の両方を作ることは
  勧めない**:gate が開いた理由が「モデル内の制御」なら BVM、「ワークフロー」なら RFC-0005。

## Summary

Define, and hold in reserve, **PALW Bounded VM v1 (PALW-BVM)**: a control-flow layer whose only
instructions are structured control (`IF`, `FOR` with a static maximum trip count and an optional
early exit, non-recursive `CALL`), a fixed file of typed scalar and tensor-handle registers, `READ`
of one committed tensor element, and `TCALL` of a PALW-TIR segment. It has no memory, no allocation,
no pointers, no IO and no unbounded loop. **All tensor arithmetic stays in PALW-TIR** — its
evaluator, its backends and its court. The VM decides *which* TIR segments run, *how often* and on
*which* committed tensors.

Registration is a ladder that consensus enforces: pure TIR; else the deterministic AOT expansion
of the BVM program into a TIR pipeline; else, and only else, a BVM class. The court adds one arm:
a one-step transition proof over an at-most-8 KiB VM state, with a descent into the unchanged TIR
court at every `TCALL`.

This RFC is equally a **measurement plan**. A bounded VM adds no expressiveness over TIR (every
program in it has a finite predicated TIR expansion); it changes cost. Whether that cost difference
is worth a second execution model in consensus is an empirical question, and on today's corpus
the answer is no: the 57 Hugging Face fixtures all lower to TIR, and none of the uncovered
checkpoints needs control flow. §1 fixes the gate before any code is written.

## Motivation

### 1. What PALW-TIR cannot do, precisely

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

### 2. The architectures that would want control flow

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

### 3. Why an insurance layer should exist on paper

- **The boundary moves with a release.** ADR-0135 D7 makes a new operation a protocol upgrade; new
  *control* is the same. If a control-flow family became popular with no designed path, the
  pressure would be to add it ad hoc — a model-specific kernel again, which RFC-0002 exists to end.
- **A design makes the measurement decidable.** With the language, the expansion and the cost
  model fixed, the tool can say for any model whether the VM would change its verdict, and by how
  much.
- **Versioned program kinds are cheapest to fix now.** Fixing how a class names its execution
  model before a second one exists lets RFC-0005 supersede or extend this layer without touching a
  registered class (§8).

### 4. The lemma that frames everything: bounded control adds no expressiveness

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
(§3.4) has no pipeline form, because RFC-0003 keeps R's domain 0 out of programs. Greedy decoding
and class-defined sampling do have one — a `Rows` program whose `post` selects with `TopK` and feeds
the id back through a `Fixed` state. A workflow that needs RFC-0001's exact rule inside it is
therefore `NeedsVm(decode_rule)` by construction: a limit of the pipeline format, not of
expressiveness.

**Corollary.** A BVM adds no function that TIR cannot compute. It changes three costs: the
executed and committed work (path versus sum of arms), the admission bound (longest path versus
sum of arms), and the program bytes (a loop body once versus `N` times). Everything below — the
gate, the pricing rule, the recommendation — follows from sizing those three differences.

## Goals and non-goals

**Goals.**

- G1. TIR first, as a rule: a model that TIR expresses within the ceilings is a TIR class, whoever
  registers it and however it was written.
- G2. The VM is control only. Every tensor value is computed by PALW-TIR and adjudicated by the TIR
  court; the VM adds no arithmetic on tensors.
- G3. Every admissible BVM program is bounded by construction: steps, calls, compute, leaves and
  live handles are derived exactly at registration, at the longest path.
- G4. Every admissible BVM program is adjudicable by construction: every committed value is
  convictable by one-step recomputation or by the TIR court.
- G5. Rewards follow verified, executed work (P4, P5); admission bounds follow the worst case.
- G6. Versioned program kinds: RFC-0005 can supersede or extend this layer, and no registered class
  ever changes meaning.
- G7. Measure before building (§1). The gate is fixed before any implementation.

**Non-goals.** Expressiveness beyond TIR (by the lemma there is none to gain). Unbounded loops,
recursion, memory, allocation, pointers, IO, clocks — those are RFC-0005's. New tensor
arithmetic: a missing operation is a prim-set revision (RFC-0002), never a VM feature. Dynamic
shapes. Floating point. Data-dependent admission ceilings. Changes to the ladder, panels, collateral
or the lottery. Sampling (it stays in the FP decode rule, RFC-0001).

## 1. The measurement plan and the gate

This section is the RFC's acceptance test, as RFC-0002 §1 is for PALW-TIR. It is fixed before any
implementation, and it is the only part of this RFC recommended for work now.

### 1.1 Verdicts that separate what a VM can fix from what it cannot

`palw-class check-architecture` (RFC-0002 §8, Phase F §2.11) gains a control verdict:

| Verdict | Meaning | Fixed by |
| --- | --- | --- |
| `ADMISSIBLE`, `ADMISSIBLE_GENERIC` | TIR as it is | — |
| `NOT_LOWERABLE(reason)` | no lowerer parser or template for the family | tooling — **never a VM** |
| `NEEDS_PRIMITIVE(name)` | an operation outside the 25 primitives | a prim-set revision — **never a VM** (G2) |
| `NEEDS_CONTROL(kind)` → `EXPANDABLE(ρ_w, ρ_a)` | data-dependent control whose AOT expansion passes admission | TIR (the expansion); a VM would only remove the waste `ρ` |
| `NEEDS_CONTROL(kind)` → `NEEDS_VM(limit)` | the expansion breaks a ceiling that the longest path does not | **the only verdict a BVM changes** |
| `EXCEEDS(limit)` | even the longest path breaks a ceiling | neither |

With `X(P)` the AOT expansion: `ρ_w = cost(X(P)) / cost(longest path)` is the worst-case waste and
`ρ_a = cost(X(P)) / E[cost(executed path)]` the average waste over a fixed evaluation set. Cost is
reported twice: MACs per job, and committed lanes per job.

### 1.2 The corpora

- **M1 — the RFC-0002 corpus as it stands.** The 57 tiny fixtures over 54 `architectures[0]` names,
  which match transformers 5.17, and the refused list of hf-coverage §4 (Llama-4, Gemma-3n,
  GLM/ChatGLM, Phi-3.5-MoE, DBRX, JetMoE, ERNIE-4.5, HunYuan, MiniMax, LFM2, RecurrentGemma, xLSTM,
  DiffLlama, BitNet, RWKV-5/6/7, the Mamba-2 hybrids, …), each assigned a cause.
- **M2 — the control-flow corpus C9–C14** (§Motivation 2). Per pattern:
  - a representative public implementation where one exists — identified during the measurement,
    not asserted here;
  - reduced configurations, for bit identity;
  - the path statistics on a fixed evaluation set: executed layers per token, experts per token,
    rounds, steps, and for C13 the branch frequencies.
- **M3 — usage weights.** Checkpoint counts and downloads per `architectures[0]` on the hub.
  hf-coverage §2 estimated the shares offline (±30 % relative). M3 measures them, which **needs
  network access that the operator approves explicitly**, as the Gate 2 checkpoints were.
- **M4 — workflows**, shared with RFC-0005: routers, cascades, tool planners and agents, with the
  model calls per task, the branching, the loop bounds and the share of work in the largest arm.

### 1.3 Metrics

Each is reported by checkpoint count and by downloads, over hf-coverage's decoder-only denominator:

- `cov_TIR`: the share with verdict `ADMISSIBLE*`;
- `cov_AOT = cov_TIR + EXPANDABLE`;
- `cov_BVM = cov_AOT + NEEDS_VM`;
- `gap_lowerer` (`NOT_LOWERABLE`) and `gap_primitive` (`NEEDS_PRIMITIVE`), reported separately —
  **neither counts for the VM**;
- `ρ_w` and `ρ_a` for every `EXPANDABLE` family with at least 1 % usage.

### 1.4 The gate

Build PALW-BVM only if **all** of the following hold:

1. **A VM-attributable gap.** `cov_BVM − cov_AOT ≥ 3` percentage points by downloads or by count,
   **or** a family with at least 1 % of downloads is `EXPANDABLE` with `ρ_a ≥ 2` — the network would
   pay at least twice the work it needs, at every execution and at every replica.
2. **No cheaper fix.** Guarded TIR (conditional stages, *Alternatives*) does not close the gap. It
   suffices when the control sits between whole segments (C13) and the unrolled guard DAG fits.
3. **No RFC-0005 decision pending.** If workflows (M4) are the driver, the decision belongs to
   RFC-0005, not to a bounded layer that RFC-0005 would supersede (§8).

The user's example thresholds translate as follows. With `cov_TIR ≈ 92 %`, a VM that lifts coverage to
at least 99 % is justified; with `cov_TIR ≈ 99 %` it hardly is. The refinement here is that only the
VM-attributable part counts: a lowerer gap or a missing primitive raises neither `cov_AOT` nor
`cov_BVM`.

Re-measure at every prim-set revision, at every lowerer release that closes a parser gap, and when
a family with data-dependent control reaches 1 % of new checkpoints in a quarter.

### 1.5 Today's numbers

| Quantity | Value (2026-09-28) | Source |
| --- | --- | --- |
| Architectures lowered and matching transformers 5.17 | 54 `architectures[0]` names in 57 tiny fixtures; max \|Δlogit\| ≤ 2.8·10^−5 | hf-coverage §3.9 |
| Admitted by `tir_admit_v1` | every fixture and the real configurations, except DeepSeek-V3 at full size (`EXCEEDS`: 2.26·10^12 MACs per position against `2^40`) | tir-lower Gate 2a |
| `cov_TIR` by count (offline estimate) | ≈ 91 % (≈ 92 % with three remote-code families, unverified) | hf-coverage §2 |
| `gap_lowerer` | ≈ 7 %: parsers not written (GLM, hybrids of existing ops, the long tail) | hf-coverage §4 |
| `gap_primitive` candidates | a few, each to be confirmed: Llama-4's chunked attention (a window shape; a masked window in v1 today), Gemma-3n's activation sparsity (a top-k-by-value gate; `TopK` exists), xLSTM's exponential gating, BitNet's ternary weights (codes fit `i8`; an artifact question) | hf-coverage §4 |
| `NEEDS_CONTROL` | **0**: no covered or refused decoder needs data-dependent control | this RFC's reading of hf-coverage §3–4 |
| VM-attributable gap `cov_BVM − cov_AOT` | **0 points** | — |

**Verdict today: the gate is closed.** The roughly 9 % that a VM might be hoped to cover is a lowerer
and primitive gap, which a VM cannot close (G2).

### 1.6 Plausible future architectures and their expected verdicts

| Pattern | Expected verdict | Why |
| --- | --- | --- |
| C9 early exit | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ L / E[e]` (about 1.3–2 where exits are frequent) | the expansion is the full model, so a VM saves only the average. The history rows of skipped layers must still be defined — by the model's own rule (propagate the exit state, or compute K and V) — which is a segment either way |
| C10 mixture-of-depths | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ 1 / capacity` (about 2 at 50 %) | a `Select` per block; a skipped position still appends a masked history row |
| C11 dynamic expert count | `EXPANDABLE`, `ρ_w = 1`, `ρ_a ≈ k_max / E[k]` | expert MACs are paid at `k_max` |
| C12 halting, looped depth | `EXPANDABLE` while `N` rounds fit the schedule (at most 1,024 layers); `ρ_a ≈ N / E[n]`. `NEEDS_VM(program)` only when `N` runs into the thousands | the repeated block is one schedule entry per round with global params; only the schedule grows |
| C13 router, cascade or tool dispatch over `M` whole models | often `NEEDS_VM(macs_per_job)`, since the expansion runs all `M`; `ρ_w` up to `M` | **the one pattern whose worst case changes** |
| C14 adaptive steps | `EXPANDABLE`, `ρ_a ≈ S / E[s]` | steps are positions (RFC-0003) |

The gate therefore most plausibly opens on **C13**, and C13 is a workflow pattern — the first rung of
RFC-0005's value case. That is the reason for gate condition 3.

### 1.7 What the measurement costs

The measurement needs no consensus work:

- a non-consensus prototype of the expander and the control verdicts in `misaka-palw-tir-lower`
  (`check-architecture --control`);
- reduced fixtures for M2;
- the M3 hub survey.

That is about 1–1.5 engineer-months, or 2–3 days of agent time plus the operator's approval for
network access.

## 2. Architecture: program kinds and the ladder

### 2.1 Program kinds

```
PalwProgramKindV1 = Tir(1)           // RFC-0002: one TirProgramV1 (Phase F)
                  | TirPipeline(1)   // RFC-0003: a TirPipelineV1 of TirProgramV2 stages
                  | Bvm(1)           // this RFC
                  | Gvm(1)           // RFC-0005 (reserved here)
```

- **The kind is in the class id's domain key** (`misaka-palw/<kind>/class-id/v1`), so no two kinds share
  an id and no class ever changes kind.
- **Each kind has its own admission function, step space, court arms and court version**, each behind
  its own fence.
- **A kind is never reinterpreted.** New semantics is a new version of a kind, as `TirProgramV2` is of
  `TirProgramV1`.

### 2.2 The ladder

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
- **Why enforce it.** G1 then holds as a rule rather than a hope: the BVM court is exercised only by
  programs that need it, and the network's exposure to a second execution model is exactly the
  measured gap.
- **What enforcing costs.** `bvm_expand_v1` becomes consensus code. Its correctness matters for
  *agreement* — every node must reach the same refusal — and not for the soundness of any verdict. A
  BVM program refused by mistake is a liveness failure. One admitted by mistake is still adjudicable.
- **It is bounded.** The expander never materialises more than the TIR ceilings allow. It stops at the
  first ceiling the expansion passes, and that ceiling is recorded in the class as the witness
  `NeedsVm { limit, value, cap }`. Cost ceilings are evaluated analytically (§4.3), without expanding.

### 2.3 Layering

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

## 3. The language

### 3.1 Values and registers

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

### 3.2 Instructions

| Group | Instructions | Semantics |
| --- | --- | --- |
| scalar | `CONST r, v` · `MOV r, s` · `ADD`/`SUB`/`MUL r, a, b` · `DIV r, a, b, rule` · `MIN`/`MAX` · `CMP r, a, b, cmp` · `AND`/`OR`/`NOT` · `SEL r, p, a, b` | exact integers; `DIV` by 04b §6.4's three rules, a divisor below 1 is an error |
| tensor → scalar | `READ r, h, i` | element `i` (a constant or an `idx` register) of handle `h`, flattened row-major; `i` out of range is an error. **The only way data reaches control** |
| segment | `TCALL seg, (h_in…), (r_in…) → (h_out…)` | run segment `seg` (§3.3); one VM step |
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
  absent (§4.2): a fault is a committed, adjudicable outcome, never a panic.
- **No other effect exists.** No clock, no IO, no message to the chain, and no randomness except R
  (RFC-0003) entering as a segment's `Random` input (§3.3).

### 3.3 Segments and `TCALL`

A **segment** is a TIR program (`TirProgramV2`, RFC-0003) or a pipeline stage. The class's segment
table names each by `graph_ir_root`, with its commitment layout, and the programs ride the
registration as RFC-0003's pipelines do. A `TCALL`:

- binds each `External` input of the segment to a handle register of the same dtype and shape, or to
  a scalar register for a rank-0 input. A scalar outside the input's declared `[lo, hi]` is a runtime
  fault before the segment runs (§4.2), so the segment's range analysis always holds;
- runs the segment over its scan, in `Scan` or `Decode` mode (§3.4), whose trip count is a segment
  constant or an `idx` register with a static maximum;
- starts from the initial state (every `Fixed` zero, every `Hist` empty): v1 has no segment whose state
  persists across calls (§3.4);
- writes its output handles — the segment's output node (`Rows`, `Final` or `Logits`, RFC-0003
  §I.2.3), committed in the callee's own step tree;
- keys every randomness the segment draws — its `Random` inputs, and `Decode` mode's selection — by
  the **per-call seed** `seed_c = H64(key "misaka-palw/bvm/call-seed/v1", seed ‖ le32(ordinal))`, where
  `ordinal` is the `TCALL`'s index in the trace. Every domain's layout, D11's included, stays byte for
  byte what RFC-0003 and RFC-0001 define; only the key material differs per call (open question 5).

A `READ` of a `Logits` handle reads the committed logits row. The VM never implements a sampler:
generation is a `Decode` call (§3.4), which applies RFC-0001's rule, and a branch on a token `READ`s
the generated ids, or a `TopK` computed inside a segment (committed by NF-18).

### 3.4 Granularity: coarse calls in v1, and why per-token control is left to guarded TIR

A v1 `TCALL` runs a whole segment scan from the initial state, in one of two modes:

- **`Scan`** — the segment's positions take their tokens from its inputs (RFC-0003 `TokenRule`s and
  `External` inputs); the output is its `Rows`, `Final` or `Logits` node;
- **`Decode { max_new, stop_ids }`** — a `Logits` segment generates: each position after the prompt
  takes the token RFC-0001 §A's selection rule chose at the previous one, with R's domain 0 under
  the per-call seed `seed_c` (§3.3). The output handle is the generated ids (`idx`, length
  `max_new`, padded after the stop). The commitments and court arms are FP V4's and Phase F's
  `TirDecodeToken*`, reused.

v1 has **no sessions** (segments whose state persists across `TCALL`s). Per-token control — C9–C11
and per-token C12 — would need them: one `TCALL` per position and layer group, with a segment's
`Hist` rows spread over thousands of callee trees and an index from positions to calls in the VM
state. That is the most expensive part of any design of this layer, and it buys only average
savings (`ρ_w = 1` for those patterns, §1.6). If the measurement shows per-token families with large
`ρ_a` and real usage, the cheaper instrument is **guarded occurrences** in a TIR program version —
a committed guard decides whether an occurrence runs at a position, inside one step tree
(*Alternatives*) — not sessions in a VM. So v1 covers what only a VM covers well: control between
whole model calls (C13), refinement at call granularity (C14) and halting at sequence granularity
(C12).

A single primitive is called as a one-node segment (the tool writes the wrapper program).

## 4. Static verification: `bvm_admit_v1`

Admission is a pure function of the class's canonical bytes, its segments' admissions and the
network's `palw_bvm_v1` ceilings. Each step refuses by name; the cheapest refusals come first.

### 4.1 Encoding and normal form

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

### 4.2 What admission proves and what it leaves to run time

Admission proves structure, types, definite assignment and every bound of §4.3. It does **not** try
to prove the absence of scalar overflow, of a `DIV` by a divisor below 1, of a `READ` index out of
range, or of a scalar argument outside its segment input's `[lo, hi]`. Each of those is a defined
**runtime fault**: the job ends in `Faulted(class)`, a committed terminal state that both parties
compute alike and the court adjudicates like any transition. This keeps admission linear and total
without a loop-invariant analysis, and it keeps TIR sound: a `TCALL` whose scalar is out of its
input's interval faults *before* the segment runs, so no segment ever sees a value its range
analysis did not assume.

### 4.3 Bounds: one dynamic program over the structured body

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
(§6.6). A program over any of them is refused as `Exceeds { limit, value, cap }`.

### 4.4 The ladder check

`bvm_expand_v1` builds `X(P)` by the construction of §Motivation 4 and runs pipeline admission on
it, stopping at the first ceiling it passes. Cost ceilings are compared with `A(x)` analytically, so
the expansion is materialised only when every `A(x)` fits. If `X(P)` is admissible, the registration
is refused `ExpandableToTir { graph_ir_root(X(P)) }`. Otherwise the class records the first broken
ceiling as its witness `NeedsVm { limit, value, cap }` — the fact that justifies its existence, shown
by explorers and the tool.

### 4.5 The class and its identity

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

## 5. Execution and the step space

### 5.1 The VM state

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
callee's execution root. Nothing else is state: segments keep no state across calls in v1 (§3.4).

### 5.2 Two levels of commitment

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
  honest one at its end, where `BvmEnd` or `BvmStep` decides it (§6.2).
- **Cost.** A `VmState` leaf is 64 bytes, a `CallRecord` under 1 KiB. Even at `2^20` steps the outer
  tree is tens of MB, small beside the inner trees, which cost exactly the executed calls' TIR
  commitments — the saving the VM exists for.
- **Resume** (ADR-0133) restarts from the last `VmState` whose preimage the seat holds, and from the
  inner trees of completed calls.

## 6. The Control-flow Court

The tensor court (Phase F: `TirCone`, H dissection, `TirLogits`, `TirDecodeToken*`, one-move
accusations) is unchanged. The control court sits above it and asks only three questions: was this
transition of the VM right, was this call made with the right segment and inputs, and did the trace
end where it should. Everything else descends into the tensor court.

### 6.1 What can be false in a BVM claim

| Lie | Example | Where it is decided |
| --- | --- | --- |
| a VM transition | the wrong branch; a loop counter advanced wrongly or a loop left early or late; wrong register arithmetic; a `READ` value that is not the committed element | `BvmStep` (§6.3) |
| a call | a different segment, input handle, scalar, trip count or context than the state before it dictates | `BvmCall` (§6.4) |
| a callee's computation | any tensor value inside a segment | the inner tree → the Phase F arms, bound to the callee (§6.4) |
| the end | a missing or early `End`; a leaf after `End`; outputs or `output_root` other than the halted state's | `BvmEnd` (§6.5) |
| a malformed statement | a `VmState` preimage that is not a well-formed state, a `CallRecord` that is not well formed | convicts the executor on its face (§6.7) |

### 6.2 Locating the lie: the ladder, one level down at a time

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

### 6.3 `BvmStep` — the one-step transition proof

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
   `j − 1` is the `CallRecord` of a `TCALL`. `pre` is well formed (§6.7).
3. Execute the one instruction at `pre.pc`. For a `TCALL`, its output handles are the record's
   `callee_root` and output types, and its other fields are checked as `BvmCall` checks them (§6.4).
   For `READ`, the element is taken from the opening, verified under the handle's root, and checked
   against its node's proven interval (PALW-TIR-33): an element outside it convicts the executor, who
   committed it.
4. Compare the digest of the resulting state with leaf `j`: different → `ExecutorGuilty`; equal →
   `ChallengerDefeated`.

Its work is one instruction on at most 8 KiB of state and one element opening: a close of a few
kilobytes, carriable by construction (PALW-TIR-38), and cheaper than any tensor terminal. This is the
whole of what the brief calls "was the branch correct, the loop counter, the VM state transition":
each is a field of `pre`, recomputed.

### 6.4 `BvmCall` and the descent into the tensor court

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

### 6.5 `BvmEnd`

The halted state names the outputs. `BvmEnd` checks that leaf `j` is `End` exactly when `pre.status`
is `Halted` or `Faulted`, that no leaf follows it, and that `output_root` is RFC-0003 §I.3.2's digest
of the emitted handles' canonical bytes, tile by tile (the `TirOutputDigestMismatch` pattern: two
openings, one move).

### 6.6 The window

A dispute may now take outer rounds, then inner rounds, then dissection rounds. Admission checks,
with RFC-0002's O-5 formula extended by one term,

```
(2 · (B_outer + B_inner + R) + t + 1) · D + 2 · 4 · max_close_chunks  <  window_court
```

where `B_outer` is the rounds of the ladder over `W(outer leaves)`, `B_inner` those over the largest
callee's leaves, `R` the dissection rounds, `t` the terminal rounds and `D` the rung window. The k-ary
court is a prerequisite (it already is for `palw_tir_v1`): at `k = 8`, `2^20` outer leaves take 7 rounds
and a `2^32`-leaf callee 11, well inside the 48-round cap.

### 6.7 Malformed statements: PALW-TIR-33 carried one level up

Every committed outer leaf is the executor's statement. A `VmState` preimage that is not a
well-formed state — a register of the wrong type, a loop counter at or above its bound, a `pc` that is
not an instruction of `func`, an `Empty` handle where definite assignment proved one, `steps` above
`W(steps)` — or a `CallRecord` that is not well formed (a segment not in the table, a trip above its
maximum) is a **malformed commitment**, and it convicts the executor whichever leaf the challenger
disputed (`BvmMalformedState`). A challenger cannot manufacture one: the digest binds the preimage.

### 6.8 Why a lie is always convictable (informative)

The outer leaves are totally ordered, and each is a deterministic function of the leaves before it, the
job, the params and the inner trees of earlier calls. The honest trace is therefore unique, and a
dishonest one first differs from it at some leaf `j`, all of whose predecessors agree. A wrong branch
first shows at the `VmState` after it, because its `pc` differs; a wrong loop exit shows at the
`VmState` after the `UNTIL` test; a wrong `READ` at the state that holds the read value. Each is
recomputed from the agreed `j − 1` by `BvmStep`. A lie inside a callee leaves the `CallRecord`'s
deterministic fields agreed and its root different, so the descent reaches the first divergent inner
leaf, which Phase F convicts (04b §9.5.7 and Phase F §2.5). An honest executor's every leaf is an
evaluation, so every close acquits it.

### 6.9 What the control court adds, as objects

- Close proofs appended to `PalwCourtVerdictProofV2`: `BvmStep`, `BvmCall`, `BvmEnd`.
- One ladder phase: `CourtBvmDescended { session, ordinal }`, which moves a session from the outer tree
  to a callee's inner tree.
- One-move accusation variants for the held regime (`BvmShardCourtAccused`).
- One step fault: `BvmMalformedState`.
- One table: `bvm_classes` (the program, segment roots, witness and layout), rooted only once written.

The brief's "Control-flow Court beside the Tensor Court" is thus three small arms, one phase, one
fault and one table. Its size is the smallest part of this RFC's cost. The larger part is the
admission logic, the step space and the second implementation that must agree with the first on every
transition (§*Effort*).

## 7. Work, pricing and the principles

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
  inside TIR. Rung 2 of the ladder makes such arms common. For RFC-0002's work derivation, this RFC
  suggests crediting the `max` rather than the `Σ` of `Select`-exclusive uncommitted subgraphs, or
  crediting an arm only through a commit point in it (open question 4).

## 8. Program kinds, and how RFC-0005 supersedes or extends this layer

- **A registered `Bvm(1)` class never changes meaning.** It keeps its admission record and its court
  version (fixed in the fence value) for as long as it has claims.
- **RFC-0005 may, without touching such a class:**
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
- **Do not build both.** If the gate (§1.4) opens on intra-model control, build this layer (or guarded
  TIR). If it opens on workflows, go to RFC-0005, which subsumes this layer.

## Proposed Spec text (new chapter `spec/palw/04d-bounded-control.md`)

Applies past `palw_bvm_v1`. PALW-TIR (04b) and pipelines (04c) are unchanged; segments are ordinary
TIR programs.

- **PALW-BVM-1 (control only).** A BVM class's tensor values MUST be computed by PALW-TIR segments
  only. The VM MUST NOT operate on a tensor except to `READ` one committed element.
- **PALW-BVM-2 (the instruction set).** A BVM program MUST use only the instructions of §3.2 under
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
- **PALW-BVM-16 (the window).** Admission MUST check §6.6's inequality.
- **PALW-BVM-17 (credited work).** A claim's credited work MUST be `Σ` over its calls of the callee's
  structural work at the executed trip. VM steps MUST credit nothing. Admission ceilings MUST use `W`.
- **PALW-BVM-18 (versioned kinds).** A class's program kind and court version MUST NOT change. New
  semantics MUST be a new version of the kind.

## Activation plan

Only if the gate of §1.4 opens. The fence has Phase F D1's shape:

```
palw_bvm_v1: Option<PalwBvmFenceV1 { activation, bvm_set_id, court_version, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()` as a whole option, is visited
activation-only by `for_each_fence`, and sits at an unused height. `validate_palw_v2` refuses to arm
it without `palw_tir_v1` (and `palw_gen_v1`, whose `TirProgramV2` segments it calls) at or below it,
without `palw_kary_court`, or on a ruleset without the A-2 tolerance.

| Step | Work | Consensus change | Exit gate |
| --- | --- | --- | --- |
| 0 | The measurement (§1) | none | the gate decision, by the user |
| A | Spec chapter 04d; vector plan | none | reviewed text |
| B | Reference interpreter and expander (`misaka-palw-bvm`) | none | golden vectors `consensus-vectors/bvm-v1/`: every instruction, every fault, `W` and `A` of test programs, expansions |
| C | Independent second implementation, from the text only | none | the vectors, and at least 10^6 random programs agreeing on values, faults, bounds and expansions |
| D | Admission `bvm_admit_v1`, `ClassRegisteredBvmV1` | dormant fence | a mutation corpus refused by name; ladder refusals; admission CPU inside the DoS budget |
| E | Step space and court arms | dormant fence | a court battery: a planted lie at every instruction kind, every call field, every end case and inside callees under each Phase F arm, plus malformed states — all convicted; honest runs acquitted |
| F | Node side: executor, trace builder, responder, resume, SDK, `check-architecture --control` | node release | executor equals the reference on the corpora |
| G | Drills | — | D-B1 the court battery on a salted t12 chain; D-B2 the forged-output red-team (8/8) on a BVM class; D-B3 the fence crossing on the shipping binary; D-B4 a real C13 class (a router over two small LMs) from registration to `Final` |
| H | The live testnet | fence at an unused height | registration → panel → `Final`; at least 3 months armed |
| I | Mainnet | fence at genesis or at a flag day | after the audits, the soak and the bounty (§*Effort*) |

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

### Phases

| Phase | Work | Engineer-months (2–4 experienced people) | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| 0 Measurement | §1 tooling, M1–M4, the report | 1–1.5 | 4–6 weeks | 2–3 days, plus the operator's approval for hub access | partly |
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

### The two figures

- **Implementation with AI agents: about 2–4 calendar weeks** from an opened gate to a BVM class
  passing D-B1…D-B4 on a salted testnet-12 chain, with two or three agents and a lead who reviews and
  integrates.
- **Mainnet-safe: about 9–12 months** from an opened gate, whatever the implementation speed. The
  critical path is the triage of the second implementation and of fuzzing (1–2 months), two audits
  with fixes (3–5 months, partly in parallel), at least 3 months armed on a testnet with drills, and a
  bounty window overlapping them. And it starts only **after PALW-TIR itself is mainnet-safe**: a BVM
  class is exactly as sound as the TIR court under it, plus its own arms.

## Comparison: TIR only, TIR + Bounded VM, TIR + EVM-class VM

The same table appears in RFC-0005.

| | TIR only (RFC-0002/0003) | TIR + Bounded VM (this RFC) | TIR + EVM-class VM (RFC-0005) |
| --- | --- | --- | --- |
| Coverage | ≈ 91 % of HF decoder checkpoints by count today; 100 % of the 54 modelled architectures; the rest are lowerer and primitive gaps | + the `NEEDS_VM` share: **0 today**; in future, routing between whole models (C13) and very long loops | + everything computable within gas: agents, tool use, planners, search; a slow path for small missing operations |
| Workflows | static pipelines (RFC-0003) | bounded and data-dependent, between whole model calls | unbounded under gas, with memory and recursion |
| Consensus surface (index; TIR = 1.0 ≈ 14k lines of consensus-path code plus 04b's 1,800 lines) | 1.0 | ≈ 1.3 (+ 4–5k lines, chapter 04d) | ≈ 2.2–2.5 (+ 15–20k lines and a second dispute game) |
| Court complexity | ladder → cone or H dissection (2 levels) | + an outer control trace of one-step transitions over ≤ 8 KiB states, and a descent (3 levels) | + interactive bisection inside checkpoints, one-step RV32IM proofs with memory proofs, syscall and precompile descent (4 levels) |
| Attack surface | interpreter, admission, dissection | + VM interpreter, the bounds program, the ladder's agreement, the step space | + prover ≡ emulator, memory proofs, the gas schedule, game liveness, oracles, EVM callbacks |
| Effort to mainnet-safe, after TIR | TIR's own path | + 9–12 months (agents: 2–4 weeks to drill-passing code) | + 24–30 months (agents: 1–2 months); the asynchronous EVM rung alone ≈ 6–9 months |
| Recommendation | build and ship (under way) | keep this design; build it only if the §1 gate opens; never build both BVM and GVM | the asynchronous EVM rung when contracts ask for it; the fraud-proven VM only after TIR is mainnet-safe and M4 shows the demand |

## Alternatives

| Alternative | Why not, or when |
| --- | --- |
| **Status quo: predication in TIR** | The default. It costs the waste `ρ` and nothing in consensus; the gate measures whether `ρ` matters |
| **Guarded TIR** — conditional stages in pipelines (a `Switch` over stages) and guarded occurrences in the layer schedule, each guard a committed scalar, with no program counter | The control becomes a finite DAG of guarded segments, and the court adds only a one-move guard check: recompute a predicate from a committed scalar. It is about half this RFC's surface, and **the first instrument to try if the gate opens on exclusive choices between whole calls (C13) or on per-token control (C9–C11)**. Its limits: a loop unrolls into guard copies (bytes grow with the iterations), and a guarded-off occurrence needs a defined history row. Where those bind, the BVM is the answer |
| `BoundedScan` / `BoundedMap` in TIR | Rejected by RFC-0002 (PALW-TIR-36), and not data-dependent in any case |
| A WASM or eBPF subset as the control language | Memory, tables, validation rules and a toolchain far beyond some forty control instructions. That is RFC-0005's substrate question, not this layer's |
| Sessions (segments whose state persists across calls) in v1 | §3.4: the most expensive part of any design of this layer, for average savings only. Guarded occurrences serve per-token control better |
| Crediting the envelope | §7: it pays for work nobody did (P5, P7) |
| Data-dependent admission ceilings | Denial of service: a program must be bounded before anyone runs it |
| Letting registrants choose BVM freely (no enforced ladder) | The VM court would be exercised by programs that do not need it, and G1 would be a hope rather than a rule |
| Skipping this layer and building RFC-0005 directly | Right when workflows are the driver (gate condition 3). Wrong when the driver is intra-model control only: a Turing-complete VM is about 2.5–3× this surface and takes years |
| A model-specific protocol feature (a native "early exit") | Refused by RFC-0002's principle: no model-specific semantics in consensus |

## Security and economic analysis

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
- **Court denial of service.** Round counts are bounded by §6.6's check. One-step closes are tiny, and
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

## Compatibility and migration

- TIR classes, pipeline classes and legacy classes are unchanged. `palw_bvm_v1` is dormant on every
  network until armed. `ClassRegisteredBvmV1` is an appended object, dropped by name below the fence
  and skipped by older builds under the A-2 tolerance, as Phase F's IR objects are.
- Nothing migrates: BVM classes are new.
- For the relationship with RFC-0005, see §8.

## Open questions

1. **The gate's thresholds**: 3 percentage points of VM-attributable coverage, `ρ_a ≥ 2`, and 1 % usage
   (§1.4). A user decision.
2. **Enforce the ladder in consensus** (recommended), or leave rung 2 to tooling and to price.
3. **The attempt lane for BVM classes**: a per-attempt pwu design, or per-job-crediting lanes only at
   first (recommended).
4. **TIR's credited work for `Select`-exclusive, uncommitted arms** — credited as executed, yet legally
   skippable (§7). For RFC-0002's owners, independently of this RFC.
5. **R per call**: a per-call seed derived from the job's seed and the call ordinal (recommended; every
   domain's layout, D11's included, stays byte-identical), or a per-call domain.
6. **Per-token control**: guarded occurrences in TIR (recommended), or sessions in BVM v2.
7. **A `VmState` leaf at every step** (recommended; at most tens of MB), or every `C` steps with replay.
8. **Weight (ADR-0069)**: a BVM family certificate drilled once for the instruction set, plus per-class
   readiness, as Phase F decided for TIR (recommended).
9. **Guarded TIR first**: if the gate opens on C13 with few arms, or on per-token control, build guarded
   TIR instead of this layer (recommended).
10. **One of the two, never both**: if the gate opens on workflows, go to RFC-0005 (recommended).

## Decision

<Open.> The drafter's recommendation: build nothing now. Run the §1 measurement (about 1–1.5
engineer-months, or days of agent time, once hub access is approved), keep this design as the
insurance it was asked to be, and revisit at the triggers of §1.4.
