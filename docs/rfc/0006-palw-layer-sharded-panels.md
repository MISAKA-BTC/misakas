# RFC-0006: PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with independent public-verifier localization and exact court on dispute

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Draft, 2026-10-01 |
| Author(s) | MISAKA core (drafted with Claude, lane M3) |
| Created | 2026-10-01 |
| Normative dependencies | RFC-0002 (PALW-TIR v1: commit points, cones, the step space of an IR class, the IR one-move court) |
| Affects | spec/palw 04b (§10: a cell), 07 (licence by parts, Final), 08 (draw, receipts, quorum, recount, outsider), 10 (locks and pay per cell), 16 (fences) · IR classes only, all networks (dormant until armed) · `consensus/core` (`palw_shard_panel_v1`, `palw_panel_v2`, `palw_state_v2`, `palw_verification_v2`, `palw_receipt`, `palw_tir_class_v1`), kaspad's panel worker, `misaka-palw-tir-exec` (a cell verifier) |
| Branch | `rfc6/gpu-shard` (text; an off-chain prototype in `misaka-palw-tir-gpu/tests/layer_shard.rs`) |
| Related | ADR-0098 (coverage is a number), **ADR-0099 / ADR-0100** (a seat holds a shard: the plan, the stratified draw, `ShardCourtAccused`, `ShardReceiptLicensed` — built for legacy classes, dormant), ADR-0103 (held context; D2 interval, D7 a seat holds a shard of the model and its state), ADR-0111 (leaf demand), ADR-0062 (DA court), ADR-0117 (a draw is one forward), ADR-0133 (verification is its own clock; segment-scoped receipts), **ADR-0147** (independence is drawn: the outsider seat), ADR-0152 (Q-1…Q-7, `basis_k`), ADR-0160 (claim capacity), RFC-0002 Phase F (`TirShardCourtAccused`, `TirStepLeaf`/`TirStepNode`), **RFC-0007** (lane M4: batched verification certificates and algebraic checks — receipt aggregation and cheaper per-shard checks are its subject, not this RFC's), `docs/design/palw/tir/gpu-integer-backend.md` |

## Current verification boundary — 2026-10-06

The cell partition, authenticated boundaries and exact localization baseline below remain useful.
For new probabilistic profiles, [RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)
and [RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md) replace routine whole-cell
replay with approved small constraint checks. [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
requires versioned kernels/typed plans, not a model VM or arbitrary circuit interpreter.

Each receipt binds class/kernel/plan/suite, checked scope, openings and transcript. Tally coverage
includes all required relations, initial/output state, cross-cell boundaries, memory and routing,
with composed whole-claim soundness. Randomly selecting a few raw cells is insufficient: one faulty
cell is found only with its selection probability even if its checker is perfect. The first-
divergence replay lemma below is a localization/conformance baseline, not algebraic-check soundness.
A cell checker needs reviewed relation binding, integer/field rules, sparse-error bounds and an exact
bounded court path. Missing evidence/scope, DA failure or silent seats cannot produce Final. Legacy
replay claims keep their old rules; a new receipt/coverage profile needs a separately reviewed fence.

## 概要(日本語)

- **問題。** claim の throughput を縛っているのは検証の供給(panel の receipt)。t12 の backlog(10-01)では未 licence の
  claim が 340 → 820 に増え、licence 数は 50 DAA あたり 273 → 60 に落ちた。供給は 600 receipt/h、需要は 380〜640/h で、
  余裕が無かった。今の seat は claim の job を **丸ごと** replay する(SEAT-R)。だから seat は「モデル全体+その job の
  KV」を ledger に予約できなければ席に着けず、replay 枠(2)は数十分ふさがる。モデルが大きくなるほど、検証できる host は
  減り、1 件あたりの検証も長くなる。
- **IR class では、層の区間 × 位置の区間(cell)を、commit 済みの値だけで検証できる。** PALW-TIR は block の carry-out
  (層の境界の行)、History の行、Fixed state の checkpoint を **すべて step leaf として commit** している(NF-20/21、
  phase-f §2.5)。だから層 `[a, b)` × 位置 `[p, q)` の seat が必要とするのは次だけ:
  - 位置ごとの層 `a` の入力(前の occurrence の carry-out)
  - 位置 `p` より前の、自分の層の K/V 行
  - 位置 `p` の state checkpoint
  - **自分の層の重みだけ**

  これらを step root に対して開き、cell 内のすべての commit point を再計算して照合する。
- **検出の補題。** honest な実行と最初に食い違う commit 値 `v` を考える。`v` の cone の入力はすべて `v` より前にあるので
  honest な値であり、再計算すると honest な値が出て、commit された `v` と食い違う。つまり **嘘は、`v` を含む cell を
  全対象commit pointを検査する verifier に見える**。この補題は、必要な認証materialを実際に取得して全対象を照合した場合に限る。honest seatの存在だけで検出を保証しない。境界の不正も、その境界を生成したcellの検査で扱う。
  既存IR one-move court(`TirShardCourtAccused`、tag 62)とH縮約のdissectionは裁定の土台である。ただし、普通の非Panel public bondが公開openingから証拠を生成して提出できるかは別gateであり、取得・局所化・terminal filingに不足があれば追加実装が必要である。
- **panel。** 既存の S1(segment・full seat + partial seat・segment ごとに 2 attest・mask 付き receipt)を
  **層 shard ごとに** 回す。cell = (層 shard, S1 segment)。
- **legacy の shard 設計(ADR-0099/0100、休眠)を t12 で止めていた二つの blocker を解く。**
  - ① R-core+ の `basis_k` 再集計に「部分 licence」の規則が無い → `basis_k` を cell 上で数える(Q-3 の自然な一般化)。
  - ② 層別の draw に outsider がいない(ADR-0147)→ **shard ごとに outsider を 1 席**、network 全体から引く。outsider が
    shard の重みだけを取ってきて検証できるのは、本 RFC の検証がモデル全体を要しないから。これは ADR-0147 §3 が
    「漏れを塞ぐもの」として名指ししたもの(モデルを持たずに claim を検証できる outsider)そのもの。
- **何が増えるか(1.5B、4 shard)。**
  - seat が持つ重みは全体の 26 %(最も重い shard)。
  - 層の境界転送は 1 位置 6 KiB。
  - 検証は 1 位置ずつの逐次 replay から、**層ごとに全位置まとめての GEMM** に変わる。GPU 実測で ×11〜19(同じ device の
    逐次比、256〜512 位置)、CPU kernel の逐次比で ×40 以上。
  - claim あたりの replay 量は 5 full replay(SEAT-R)から 2 replay 相当(cell ごとに 2 attest)に減る。
- **費用。** 1 claim の receipt が shard 数倍になる(4 shard で約 77 KB)。receipt の集約は RFC-0007 の主題。
- **有効化。** 休眠 fence `palw_tir_shard_v1` を 1 本(Some-only hash、`never()` collapse、未使用の高さ)。fence を跨ぐ
  drill(cell の嘘・境界行の隠匿・shard だけ持てる seat)を出荷 binary で回してから。
- **prototype(D4、実装済み)。** 小さな TIR fixture を producer として 1 回実行し、trace を Merkle root に commit する。
  層 `[a, b)` × 位置区間の cell を、root に対して開いた境界行・過去の History 行・checkpoint と、その層の重みだけで
  検証する(参照評価器の `eval_cone`)。
  - honest trace は全 cell が受理する。cell 内の改ざんは、その cell がその leaf で検出する。
  - 境界行の「つじつま合わせ」の嘘は、下流 cell が素通しし、上流 cell が検出する(補題どおり)。
  - state の嘘は、それを書く segment が checkpoint で検出する。
  - 8 層 4 shard では、1 cell が持つ重みは 25〜27 %、読む trace は 0〜1.6 %、照合するのは 1/8(§9)。

## Summary

A panel seat of an IR class today replays the claim's whole job. It must hold the whole model and the
job's whole history, and its replay is a sequence of single-position steps. This RFC lets a seat verify
a **cell** of a claim instead: a contiguous range of layers crossed with a position segment. It reads
only committed material: the carry-in of its first layer at every position of the segment, its own
layers' history rows before the segment, their state checkpoint at the segment's start, and **its own
layers' weights**. It recomputes every commit point inside the cell, as one batched pass per layer over
the segment's positions.

The **first-divergence lemma** (§2) applies when a verifier obtains authenticated material and checks every relevant commit point in the cell. It does not guarantee detection merely because an honest seat exists. Existing `TirShardCourtAccused` and dissection kernels provide a terminal-court baseline; an ordinary non-Panel public bond must also be able to acquire evidence, localize and file a bounded exact proof. Missing acquisition or prover capability requires additional work under RFC14.

The panel is ADR-0100's stratified panel, with three additions:

- Verification V2's S1 segments run inside each layer shard.
- An **outsider per shard**, drawn from the network as ADR-0147 draws one per claim.
- A **recount over cells**: `basis_k` is counted over cells instead of whole segments.

The last two remove the two refusals that keep ADR-0100's sharded licensing off testnet-12. Everything
sits behind one dormant fence.

## Motivation

### 1. Verification supply is the binding limit of claim throughput

On testnet-12 the panel, not the producers, decides how many claims become Final. The 2026-10-01
backlog (`docs/design/palw/t12-panel-backlog-1001.md` on `rcore/int-10-p1`):

- Open claims rose from ~340 to ~820 between DAA ~2,600 and ~3,090, while issuance rose from 0.6 to
  5.3 claims a DAA.
- `panel_bound` stood at 261 against ~17 in normal running.
- Licences per 50 DAA fell 273 → 126 → 60 while binds held at ~290.

Two node faults caused it:

- Two hosts' ledgers counted mimalloc's lazily-freed pages as used and refused every duty.
- One overcommitted host ran replays of 7–50 minutes instead of ~5 seconds. An 8k replay reserved 3.37
  of its seat's 3.5 GiB and held one of the seat's two replay slots for most of an hour, on seats drawn
  into 65 % of panels.

The faults are fixed (int-10.1, int-11), but the margin is not. Supply was ~600 receipts an hour against
demand of 380–640.

The structure behind the margin is what this RFC changes:

- **A seat replays the whole job.** Under SEAT-R a seat signs `Valid` only for arithmetic it ran
  itself. So on testnet-12 the full seat, every partial seat outside C7 and the outsider all replay the
  claim's whole job (`kaspad/src/palw_panel.rs:220-243`). A claim on the coverage door costs about five
  replays.
- **A seat must hold the whole model.** Readiness is filed only if a FULL-seat replay fits the ledger
  (`palw_panel.rs:4144-4151`). A partial seat must hold the whole model too, and the K/V prefix up to its
  segment's end (`palw_resource_profile_v1.rs:262-263`).
- **A replay is sequential.** A replay is one position at a time. On a device that is a chain of
  matrix–vector products, and the shape that wastes most of a GPU: §8 measures a batch of 256–512
  positions through the same weights at 11–19× the per-position speed on the same device.

Every larger class makes all three worse. A Qwen2.5-32B-shaped IR class is ~30.5 GiB of `i8` weights,
and a Llama-3-70B shape ~65.7 GiB (§8). No testnet-12 seat holds either, so no seat can replay them,
and supply for them is zero.

### 2. What already exists

ADR-0099 and ADR-0100 designed seats that hold a shard, for legacy classes, and built most of it
consensus-inert:

- a shard is a contiguous layer range, and the plan is derived (min–max of the widest seat; embedding
  first, logits last);
- a stratified panel (`derive_stratified_panel_v2`), with bonds declaring their shards
  (`BondShardsDeclared`) and the registrant declaring the count (`ClassShardPlanDeclared`);
- the one-move court a shard seat files (`ShardCourtAccused`);
- licensing by parts (`ShardReceiptLicensed`, a progress bitmap, the licence on the last part).

All of it is behind `palw_shard_court` and `palw_shard_licensing`, dormant everywhere.

PALW-TIR (RFC-0002) supplies the rest:

- **IR claims commit exactly what a shard reads.** Every block carry-out — the residual between layers
  — is a commit point (NF-21). So is every `HistAppend` row (NF-20). `Fixed` state is committed every
  `C` positions and `Hist` rows every `h_tile` positions, as leaves of the one step tree (phase-f §2.5).
  "Every leaf is adjudicated from leaves that precede it."
- **The court an IR shard seat needs is armed on testnet-12.** `TirShardCourtAccused` (tag 62, under
  `palw_tir_v1`) carries an IR close proof and is adjudicated by `adjudicate_close_proof_v2`. A leaf
  that reduces over the history opens an F7 dissection session.
- **The DA units for an IR leaf are scheduled.** `TirStepLeaf` and `TirStepNode` come with
  `palw_tir_fence2` at DAA 3,600.

**What is missing** is the verification mode, a seat checking a cell from committed rows, and two rules
that `validate_palw_v2` names as the reasons sharded licensing may not be armed on testnet-12:

1. *"palw_rcore_plus is armed beside palw_shard_licensing: Q-3's basis recount has no rule for a licence
   by parts"* (`config/params.rs:7116-7121`);
2. *"palw_admission_independence is armed beside palw_shard_licensing: a stratified panel seats no
   outsider, so a bought class licensed by parts would be judged by its own population alone (ADR-0147)"*
   (`config/params.rs:4174-4188`).

ADR-0147 §3 also names what closes its own residual: *"an outsider that can check a claim WITHOUT holding
the model"*. A cell check is that.

## Goals and non-goals

**Goals.**

- G1. A seat verifies a cell of an IR claim with nothing but committed material and its own layers'
  weights; no seat needs the whole model.
- G2. A lie anywhere in a claim is detected by the seat holding the cell where it is computed, and
  convicted by the court that exists.
- G3. The panel keeps today's guarantees per cell: two attestations per cell, the recount, and an
  outsider who is not the class's population.
- G4. Verification becomes batched over positions (a layer at a time), the shape both a CPU and a GPU
  run fastest.
- G5. One dormant fence; no change to any legacy class, any receipt below the fence, or any fingerprint
  while dormant.

**Non-goals.**

- Legacy classes (ADR-0099/0100 stay as they are).
- Receipt aggregation, Narwhal-style certificates, or algebraic shortcuts (Freivalds, weight sketches) —
  RFC-0007's subject. Its cheaper per-shard checks would plug in as a verification mode of a cell
  without changing anything here.
- Splitting a single layer across seats (tensor parallelism).
- Changing what a producer computes or commits: **no new commitment**. The cell is a reading of the step
  tree that exists.

## 1. Cells

### 1.1 The layer plan (derived, as ADR-0099 Decision 2)

An IR class's program has occurrences `o_0 = pre`, `o_(1+l)` for layers `l ∈ [0, L)`, and
`o_(L+1) = post` (spec 04b §3.3). A **layer plan** of `S_L` shards is the contiguous partition of
`[0, L)` that minimises the widest shard's **weight**:

- the bytes of every per-layer param instance its layers read;
- plus the bytes of every global param its layer blocks read (RoPE tables, a shared expert), counted on
  every shard that reads it;
- plus the `Hist` and `Fixed` state its layers hold at the class's `max_context` (the history in lanes,
  4 bytes a value — what a seat holds).

`pre` and its global params ride with shard 0 and `post` with shard `S_L − 1`, as ADR-0099 pins the
embedding and the logits. The plan is a pure function of the registered program and `S_L`, so two
nodes derive the same plan. `S_L` is the class's, declared once by its registrant (`ClassShardPlanDeclared`,
ADR-0100 Decision 4; for an IR class it carries `S_L` and `S_P`, below) and immutable.

### 1.2 Position segments

A job of `T` positions (absolute positions `a = 0 … T−1`, phase-f §2.5) is cut into `S_P` segments at
multiples of `G = lcm(C, h_tile)`, where:

- `C` is the class's checkpoint interval;
- `h_tile` is its history tile.

The cut makes every segment start at a position where every `Fixed` state of every layer has a
checkpoint leaf and every history row before it lies in whole history tiles. A segment boundary then
costs no state replay and no partial tile. The segments are Verification V2's segments (PALW-VF-33),
moved from leaf ranges to position ranges for an IR class: segment `j` is positions
`[⌊j·T/S_P⌋_G, ⌊(j+1)·T/S_P⌋_G)`, where `⌊x⌋_G` rounds down to a multiple of `G` and the last segment ends
at `T`.

A **cell** is `(shard i, segment j)`: occurrences `1 + l` for `l ∈ shard i` (with `pre` on shard 0 and
`post` on the last), at positions `a ∈ segment j`.

### 1.3 What a cell reads, and what it checks

**Inputs, every one a committed leaf** (opened against the claim's step root; the court's own openings):

| input | leaf | when |
| --- | --- | --- |
| the carry-in of the shard's first occurrence at every position `a` of the segment | the previous occurrence's carry-out commit tiles at `(a, slot)` (NF-21) | shard `i > 0`; shard 0 reads the job's tokens instead |
| every `Hist` row of the shard's layers at positions before the segment, inside the window | the rows' commit tiles (NF-20) or whole Hist tile leaves (every `h_tile`) | the segment does not start at 0 |
| every `Fixed` state of the shard's layers at the segment's first position | the checkpoint leaves at `a = p − 1` (`(a + 1) % C == 0`) | the segment does not start at 0 |
| the job's tokens — the prompt's, and at decode positions the generated ids as committed (the decode-token door, phase-f §2.5) | the job and its committed ids | shard 0 (and `Input(1)` everywhere); the LAST shard also checks each generated id against its own logits |
| the shard's params | inventory leaves under `artifact_root` (row pieces ≤ 32 KiB, closed-form indices) | always |

**Outputs, checked** — every leaf of the step tree the cell produces:

- the commit tiles of every commit point of the shard's occurrences at every position of the segment,
  including the last occurrence's carry-out (the next shard's input) and, on the last shard, the logits
  and the logits trace;
- the Fixed checkpoint leaves at positions inside the segment;
- the Hist tile leaves completed inside it.

The seat recomputes each leaf's preimage and checks it against the step root. The check uses either the
producer's leaf and its opening, or the run's sibling hashes for a whole contiguous run (a layer shard's
leaves at one position are one run of slots, ADR-0099 §1.1). A leaf that does not recompute is the
cell's finding.

### 1.4 Two ways to check a cell, one receipt

- **Sequential recompute.** Run the shard's occurrences over the segment from the cell's inputs alone,
  in position order, with commit points recomputed rather than read. This needs the fewest bytes: the
  inputs above, plus the step tree's siblings to place the recomputed leaves.
- **Local cone checks.** Evaluate every commit point's cone (spec 04b §9.2) from committed operands,
  the cell's internal commit points included. Every cone is independent, so the whole cell is one
  parallel batch, but the cell's committed values must be fetched.

Both establish the same fact: **every leaf the cell produces equals the PALW-TIR function of the
cell's committed inputs.** A `Valid` receipt for the cell attests that fact and nothing more. Which
mode a seat runs is node policy, like the transfer form ADR-0099 priced and did not choose.
Sequential recompute is batched as **one layer at a time over every position of the segment**
(ADR-0117's "one forward", base0's `forward_prefill_planned`): its projections are GEMMs over the
segment's positions, its attention is a batched causal kernel, and its recurrences step in position
order.

## 2. Why a cell's check is enough

### 2.1 The detection lemma

Order the leaves of a claim's step tree by the enumeration (phase-f §2.5: position-major, then slot,
then checkpoint and history leaves). The tree's invariant is that **every leaf is a function of
leaves that precede it, plus the params and the job's tokens**:

- a commit point's cone reads commit points of lower slots in the same position, carry-outs of
  earlier occurrences, and rows of earlier positions;
- a checkpoint leaf reads the previous checkpoint's leaves and the rows between;
- a Hist tile reads the rows it concatenates.

Call `F(ℓ)` the value PALW-TIR assigns leaf `ℓ` given the committed values of its predecessors.

> **Lemma.** Let a claim's committed tree differ from the honest execution's, and let `v` be the
> **first** leaf in the enumeration whose committed value differs from the honest value. Then
> `committed(v) ≠ F(v)`, and `v` lies in exactly one cell. A seat that checks that cell — by either
> mode of §1.4 — finds `v`, and finds no leaf of the cell before it.

*Proof.* Every predecessor of `v` agrees with the honest execution, by minimality. So `F(v)` evaluated
on committed predecessors is the honest value, which differs from `committed(v)`.

`v` is produced inside the occurrence and position that compute it, so it belongs to the one cell
containing that occurrence and position: checkpoint and Hist tile leaves belong to the cell of their
layer and position.

In local mode the seat evaluates `F(v)` from `v`'s committed operands, which are predecessors of `v`,
so honest, and sees the difference. In sequential mode the seat walks its cell in enumeration order.
Every leaf `u` of the cell before `v` is honest (minimality). It is recomputed from inputs that precede
it: the cell's committed inputs, which are honest, and the seat's own earlier recomputations, which
equal the committed honest values by induction. So the recomputed `u` matches and no leaf before `v`
is flagged. At `v` the recomputation is the honest value, and `committed(v)` differs. ∎

Three consequences:

1. **A consistent lie is caught upstream.** A producer that lies at a boundary row (a carry-out) and
   computes every later leaf honestly from the lie passes the downstream cells' checks: their inputs
   are the committed lie, and their outputs match it. The lemma puts `v` at the boundary row itself,
   which is an OUTPUT of the cell that computes it, and that cell does not verify. This is the
   argument the prototype exercises (§9).
2. **Cells compose.** If every cell verifies, the committed tree is the honest execution, by induction
   along the enumeration. A `Final` therefore needs every cell attested, exactly as the coverage door
   needs every segment attested today.
3. **The court is the existing one.** The cell's finding is a leaf and its committed inputs. That is
   the input of `TirShardCourtAccused`: a `TirCone` close proof for a commit point, the logits doors
   for the logits, and a Fixed or Hist leaf through its update cone. A leaf whose cone reduces over
   `H` is the F7 challenge: a session at `Terminal`, the responder's root claim first, then the
   dissection to one `h_tile` (spec 04b §9.5). Nothing about the court depends on who checked which
   cell. Its verdict depends only on the leaf, so *any* bonded party holding the cell's evidence can
   file it.

### 2.2 What a cell check does not see

A seat attests its cell, not the claim. A lie in a cell that no honest seat checks is not seen by the
panel. It stays convictable by anyone who later checks that cell within the court window, but a
corrupt quorum of that cell could license it first (§5).

## 3. Availability of boundary rows

A cell reads more of the producer's commitments than a replay does. A replay needs only the tokens and
compares roots; a cell needs its inputs' leaves (§1.3). These are the routes, in order:

1. **From the panel.** The previous shard's seats recompute the boundary rows of the same segment
   anyway, and its cell's check succeeds only if they equal the committed ones. A seat may take its
   inputs from any peer: every input is checked against the step root before use, so the source does
   not matter. A stratified panel can therefore run as a pipeline over shards, shard `i` starting each
   segment when shard `i − 1` has published its carry-outs. This is node policy and needs no consensus
   object.
2. **From the producer, off chain.** It is ADR-0111's fast path: a signed request and a 6-DAA wait
   (`PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`). The producer's served annex (evidence-transport-scope,
   option B) is the natural carrier: per cell, the carry-in rows and its opening siblings.
3. **On chain, enforced.** It uses the IR DA units past `palw_tir_fence2`: `TirStepLeaf { index }` for a
   leaf, and `TirStepNode` for a 10-level subtree frontier (64 KiB). Withholding past `W_disclose`
   (1,200 DAA on testnet-12) is a default under the DA court's rules: void and slash (ADR-0062, ADR-0152
   DA-1..9).

A cell's inputs are *runs*: one carry-out run per position, and history rows in whole tiles. So this RFC
adds one unit, **`TirStepRun { first, count }`**: a contiguous run of step leaves with its two boundary
paths, bounded by the carrier (100,000 bytes). One demand then serves about `⌊100,000 / (lanes × 4)⌋` positions of a segment's carry-ins, less the
paths (≈ 15 positions of a 1,536-wide residual per unit, against one per `TirStepLeaf`).

The ADR-0152 session limits (a seat: 1 open, 4 lifetime) stand. A demand names a run, not a cell, so a
seat whose producer withholds can force a default within its limits.

A seat that cannot obtain its inputs says `Unavailable { chunk, daa }`, as today: it abstains and is not
liable (PALW-VF-17). `seat_count − quorum + 1` such receipts redraw (SR-9).

## 4. The panel

### 4.1 The stratified draw, per shard (ADR-0100 Decision 4)

For a claim whose class has an IR shard plan, read at the claim's anchor:

- **Per shard, a panel.** The chain draws `derive_stratified_panel_v2` over `S_L` shards: per shard
  `s_shard` seats from the bonds that declared that shard (`BondShardsDeclared`), one seat per operator
  per shard, stake-weighted as the flat draw is.
- **Short shards.** A shard short of eligible operators refuses the draw by name, and the claim voids at
  `BindTimeout`. A sharded class never falls back to a flat panel.
- **The same operator may sit in several shards.** Its seats are distinct duties with distinct masks.

`s_shard = 3` is proposed (§5): one full-shard seat and two partial seats. The S1 assignment is
PALW-VF-33's run inside each shard: `K = s_shard − 1 = 2` segments, one drawn full-shard seat attesting
both, each partial seat attesting one by the drawn rotation. So `S_P = s_shard − 1` is the target, and
every cell is attested by two seats: the full-shard seat and its partial seat.

The recommended first step (open question 5) is `S_P = 1`. Each seat then attests its whole shard, and
a shard's two attesters are its first two seats. Segments switch on per class later.

### 4.2 An outsider per shard (ADR-0147, generalised)

For a claim of a bought class (`registrant_bond` is `Some`) at or past the fence, each shard's panel
gains an **outsider seat**. It is drawn from the network's base-class population exactly as ADR-0147
§2.1 draws the claim's outsider:

- the ticket is `H(outsider-ticket domain ‖ anchor ‖ claim ‖ shard ‖ operator_id)`;
- the registrant's bond and operator are excluded, as is the registration cut of §2.2;
- one operator holds at most one outsider seat of a claim.

The shard's outsider attests the **whole shard** (every segment). **No part licence stands without
its shard's outsider's `Valid`**, on every licensing arm.

An outsider does not declare the shard, so it has to obtain the shard's weights. It **fetches them**:
the shard's inventory rows under `artifact_root` (`palw_shard_inventory_rows_v1`, ADR-0100 Decision 2),
each a row piece with its opening, from any holder (the registrant's served artifact, a peer, a mirror).
It verifies every row against the class root, as a seat does at readiness.

Its cost is the shard's bytes, not the model's. That is §8's 26 % for a 4-shard 1.5B class, and 7.7 GiB
of a 32B shape at 4 shards. An outsider that cannot fetch or hold its shard says `Incapable`; the part
cannot license and the claim voids at its receipt deadline. This is ADR-0147's rule: *the cost of a
model the network does not run lands on the claims of that model*. What changes is that "the network
does not run" now means "no outsider can hold one shard", not "no outsider can hold the model".

This removes refusal 2: a stratified panel now seats an outsider in every part.

### 4.3 Receipts with cell masks

A receipt for an IR-sharded claim is the V3 receipt with its mask widened from segments to cells:
**`ReceiptV4 { …V3, shard: u16, segment_mask: u64 }`**. The signature covers both
(`palw_receipt_message_v4`, a new context in the next signature-context set). Verdicts are Q-1's:

- `Valid` attests every leaf of every cell in the mask (§1.4);
- `Unavailable { chunk, daa }` names a missing input leaf;
- `Incapable` is unchanged;
- `Sampled` never counts.

### 4.4 The recount over cells (Q-3, generalised)

The licence records

```
basis_k = min(3, min over cells (i, j) of #distinct counted Valid signers whose receipt covers (i, j))
```

over the union of every part's receipts. Final requires `basis_k ≥ 2` (`PALW_RCORE_FINAL_BASIS_K_V1`),
unchanged.

For a flat (unsharded) claim, `S_L = 1` and this is today's `palw_receipt_set_basis_k_v1` exactly: one
shard, segments as cells.

A supplementary receipt may raise `basis_k`, never lower it (Q-3). Each counted signer locks
`lock_{max(basis_k, 2)}` with its mask (Q-4, §6). A `Segmented` receipt is liable by its mask and the
fault's site (Q-6): a `Leaf` fault is charged to the receipts whose mask covers the leaf's cell, and a
`Whole` fault to every counted receipt of the claim.

This removes refusal 1: the recount has a rule for a licence by parts, and it is the one it already
had, read over cells.

### 4.5 Licence by parts, and Final

ADR-0100 Decision 4 unchanged:

- each shard's quorum is a `ShardReceiptLicensed` part: its `s_shard` seats plus its outsider, the
  outsider's `Valid` required;
- the claim licenses in the block that lands its last part;
- `basis_k` is recounted then (§4.4);
- a shard whose quorum says `Unavailable` voids the claim (where ADR-0065 D4 still has that door);
- `ReceiptLicensed` and `ProducerDefaulted` are refused for a claim that licenses by parts.

Final is PALW-LC's rule: `ReceiptLicensed` with `basis_k ≥ 2`, no open court and no open seat DA
session, at `max(licensed + 120, last_daa)`.

A part is `(s_shard + 1) × 4,772` bytes of receipts plus the claim and shard fields: about 19,100
bytes, one carrier, at any shard count. A 4-shard claim is about 77 KB against ~15–24 KB today. That
growth is what RFC-0007's batched certificates are for.

## 5. Coverage and collusion

Let:

- `h` be the honest share of the stake eligible for a shard;
- `f` be the registrant's share of the network's floor population (ADR-0147 §3);
- `s` be the counted seats of a cell (two attestations per cell under §4.1, three counting the shard's
  outsider).

A lie is committed with the claim, **before** the anchor that seeds its panel exists (the seed is the
anchor attempt's execution commitment, PALW-VF panel seed). So the lie's cell is fixed before its
seats are drawn, and a lie's chance of escaping is independent of the number of cells. This is ADR-0098's
"a one-leaf lie's coverage is independent of the shard count", at cell grain.

A lie escapes the panel only if every seat attesting its cell is corrupt or silent:

| per lie, the panel misses it with | `h = 2/3`, `f = 0.26` (ADR-0147's fixture) | `h = 0.9`, `f = 0.05` |
| --- | --- | --- |
| today on testnet-12: a 5-seat flat panel in which every seat replays the whole job (SEAT-R): `(1−h)^5` | 0.41 % | 0.001 % |
| today on paper (Verification V2: each segment attested twice) with the claim's outsider: `(1−h)^2 · f` | 2.9 % | 0.05 % |
| this RFC: the cell's two attesters (the shard's full seat and the segment's partial seat): `(1−h)^2` | 11.1 % | 1 % |
| …and the shard's outsider, who attests the whole shard: `(1−h)^2 · f` | **2.9 %** | **0.05 %** |
| four attesters a cell (`S_P = 1`, `s_shard = 4`: every seat attests its whole shard) and the outsider: `(1−h)^4 · f` | 0.32 % | 0.0005 % |

Reading:

- **The panel's detection is a parameter, not a property of sharding.**
  - The number of attesters a cell has is set by the S1 assignment: two per segment
    (`PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT`), plus the shard's outsider.
  - A network that wants testnet-12's de-facto five whole replays sets four attesters a cell. It then
    pays the same work as today (§8), in shard-sized, batched duties.
  - The default (`s_shard = 3`) is exactly Verification V2's paper guarantee, at cell grain.
  - The user decides the point (open question 2).
- **The licence still needs quorum.** A corrupt quorum of one cell plus a corrupt outsider of its shard
  licenses a lie. The one-move court convicts it later if anyone checks the cell before the court
  window closes (3,000 DAA on testnet-12). The economic deterrent is ADR-0098 D4's coverage ×
  `claim.reserved`, now per cell.
- **Correlation across shards** helps the verifier. A lie in several cells is caught if any of their
  seats is honest. A producer that lies once is caught with the cell's probability; one that lies
  everywhere is caught with near certainty.
- **Stake per shard.** A shard's eligible population is the bonds that declared it. A class few
  operators shard has thin shards, and `h` per shard can be lower than the network's. The outsider per
  shard is the floor: a bought class cannot fill a shard's panel without also drawing the network's
  outsider.

## 6. Economics

### 6.1 Seat duty and locks, per cell

A counted `Valid` signer locks `lock_{max(basis_k, 2)}` for its mask (ADR-0152 lock_v2), and is liable
by its mask's cells.

The proposal scales a seat's duty and lock by its cells' share of the claim's work:

```
w_cell(i, j)        = the IR class's structural work (PALW-TIR-16) of shard i's occurrences over segment j's positions
duty_bind(seat)     = min( max(λ-term, lock_2) · Σ_{cells ∈ mask} w_cell / w,  ⌊commitment / seats⌋ )
lock(seat)          = lock_v2(…) · Σ_{cells ∈ mask} w_cell / w      (floored at the network's seat-lock floor)
```

With the floor, the panel's total lock over a claim is unchanged by sharding. Every cell is attested
twice, so the sum over a cell's two attesters is what one cell's work owes, and the sum over cells is
the claim's.

What changes is the **per-seat** lock: a 4-shard seat locks about a quarter of a full seat's. A bond
can therefore seat about four times as many duties within ADR-0160's aggregate liability. That is the
supply this RFC exists for.

The slash for a false `Valid` (S4: the lock plus `min(25 % C, 3G)`) is unchanged, so a seat that
attested a lie loses its bond's penalty term whatever its cell's size. **The deterrent does not shrink
with the cell.**

### 6.2 The claim's liability and the class room

The claim's `reserved` and its commitment `w + E + rr` are unchanged. Sharding changes who verifies,
not what a claim stakes.

Past F-R, a class's verification room (`palw_verify_capacity_v1`) counts two replays per claim
(`2 × eccu`) at the measured-speed constant. For an IR-sharded class it becomes the binding shard's:

```
room(c) = min over shards i of ⌊ ready_eff(c, i) × per_seat × window / (2 × eccu × w_i / w) ⌋
```

`ready_eff(c, i)` counts the seats ready for shard `i`. Readiness for a shard is the readiness proof
(tag 50) over the shard's inventory rows only, and the node's gate becomes "a full-SHARD replay fits
the ledger". A class whose shards are a quarter of its weight, held by four times the seats, has up
to **sixteen times** the room.

The measured-speed constant `PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1` (2,000 ‰) is consensus. A batched
cell verifier (§8) is faster than the replay that constant was measured on, but raising it is a
separate decision under its own fence, not part of this RFC.

### 6.3 Seat pay

The panel pool (the model-class share, `clamp(α·C_V/(C_P+α·C_V), 100 ‰, 300 ‰)`) is split by counted
receipts weighted by `Σ w_cell / w` of their masks, not `⌊pool / K⌋` per seat. A shard's outsider is
paid for its whole shard. That pays for the fetch the outsider had to make.

## 7. Held context (ADR-0103) and position segments

An IR class on testnet-12 runs under the held regime from genesis:

- the context is held off chain;
- the chain carries roots, openings and a logarithm;
- `CourtOpened` is refused, and every accusation is one move;
- `Fixed` state is checkpointed every `C`, and `Hist` rows every `h_tile` (`h_tile ≤ 4,096`).

Cells change nothing there. They read the same leaves and accuse at the same named leaf.

What they change is ADR-0103 D7, *"a seat holds a shard of the model and of its state"*. That becomes
literal for an IR class: a shard seat holds its layers' weights and its layers' history, never another
layer's.

The position axis is ADR-0103 D2's interval, made cheap. A late segment of a long job still needs every
earlier history row of its layers (attention reads all of them), so position segments bound a cell's
**compute** but not its history **read**. Layer shards are what bound the history a seat holds: its
`|shard| / L` of the model's. A 2M-position class (C7) thus needs layer shards; position segments alone
would leave every seat holding every layer's 2M rows.

Checkpoint alignment (`G = lcm(C, h_tile)`, §1.2) is what keeps a segment boundary free. `C` comes from
admission (`C_j` per state, spec 04b §10.3) and `h_tile` from the class layout. A layout whose `G`
exceeds `T / S_P` simply gets fewer segments.

## 8. Cost model and what it buys

**Weights a seat holds** (`i8` codes, the plan of §1.1; the embedding and the head untied, as in
`tir-exec-bench`'s programs):

| model shape (layers, d, ff, vocab) | whole | widest shard at `S_L` = 2 / 4 / 8 |
| --- | --- | --- |
| Qwen2.5-1.5B (28, 1,536, 8,960, 151,936): 44.6 MiB a layer, embedding and head 222.6 MiB each | 1.67 GiB | 0.83 / 0.44 / 0.26 GiB (50 % / 26 % / 16 %) |
| Qwen2.5-32B shape (64, 5,120, 27,648, 152,064): 465 MiB a layer | 30.5 GiB | 15.3 / 7.7 / 3.9 GiB |
| Llama-3-70B shape (80, 8,192, 28,672, 128,256): 816 MiB a layer | 65.7 GiB | 32.9 / 16.4 / 8.2 GiB |

A 24 GiB-class seat holds a 4-shard part of the 70B shape and a 2-shard part of the 32B. Today it holds
neither.

**Trace a cell reads.** Per position, a shard boundary is one carry-out row: `d × 4` bytes (6 KiB at
1.5B, 32 KiB at 70B). The whole committed trace of a 1.5B position is about 4.6 MiB (≈ 143 KiB of
commit tiles per layer plus 593 KiB of logits). A cell's carry-in is therefore 0.13 % of the trace its
segment commits. Its history read is its layers' K/V rows before the segment: `2 × kvd × 4` bytes per
row per layer (2 KiB at 1.5B), which is what bounds a late segment (§7).

**Work.**

| per claim | today (SEAT-R, t12) | this RFC (`s_shard = 3`, two attestations a cell) |
| --- | --- | --- |
| replay-equivalents | ≈ 5 (every seat outside C7 replays the job) | 2 (every cell twice) plus each shard's outsider's whole shard: 3 in all |
| per duty | the whole job | `≈ 1/S_L` of it (`1/(S_L·S_P)` for a partial seat) |
| memory per duty | the model and the job's history | the shard's weights and its layers' history |
| shape | `T` sequential single-position steps | `|shard|` layer passes, each a batch of the segment's positions |

**The batched shape, measured** (`tir-gpu-bench --layer-batch P`, `misaka-palw-tir-gpu`; design
`docs/design/palw/tir/gpu-integer-backend.md` §6). Setup: Apple M1 Max, a shared host at load 48–78.
The seven projections of one 1.5B layer, each batched result byte-checked against the CPU executor's
kernel before timing:

| positions | batched | per position | one position at a time | batching gain |
| --- | --- | --- | --- | --- |
| 256 | 36.7 ms | 0.144 ms | 1.59 ms | **×11.1** |
| 512 | 67.2 ms | 0.131 ms | 2.44 ms | **×18.6** |

The CPU kernel's per-position replay of the same projections took 5.7–33 ms a position on this host (3
threads, loaded), so a batched device check of a cell runs **×40–250** faster than the per-position
CPU replay a seat runs today. Every one of these results is byte-identical to the CPU executor's.

**Supply, put together.** At `S_L = 4` the projected effect is the product of three factors, each a
measured or a structural number:

- **1.7–2.5× fewer replay-equivalents.** A claim costs 2 replay-equivalents instead of 5, about 3
  with the outsiders.
- **About 4× more duties per ledger.** A shard's weights are a quarter, and so is its history.
- **×11–19 on the same device, ×40–250 against the CPU replay** (§8's measurement) for any seat that
  verifies in batches rather than replays.

Not projected:

- the receipt bytes (§4.5), which grow by `S_L` until RFC-0007;
- the boundary transfer, which is small (MiB per cell).

## 9. The prototype (D4)

`misaka-palw-tir-gpu/tests/layer_shard.rs` is an off-chain test; it changes no consensus code. It runs
on the reference evaluator only and needs no GPU.

1. **Produce.** A small PALW-TIR program runs once as a producer would. Every commit point of every
   occurrence at every position becomes a leaf, in slot order, with every `Fixed` instance
   checkpointed after each position `a` where `(a + 1) % C == 0`. The leaves become a Merkle tree
   (BLAKE2b-256), whose root is the claim's commitment. The producer is cross-checked against the
   reference evaluator's own run (`Interpreter::run`), commit point for commit point.
2. **Verify a cell.** A seat verifies occurrences `[a, b)` over positions `[p, q)`. It holds the root
   and a params source that serves exactly the cell's param instances and refuses every other one, so
   a check that needed another shard's weight fails. It opens every input against the root:
   - each position's carry-in;
   - the history rows before `p`;
   - the checkpoints at `p`.

   It recomputes every commit point with `Interpreter::eval_cone` — the court's own function — in
   position order, and checks each recomputed leaf by hashing it up the committed path. No producer
   value is used as an output.
3. **Fixtures:**
   - a four-layer decoder in `tir-exec-bench`'s conventions (`d` 64, GQA over full-attention histories,
     the wide RMS norm, a GLU MLP; 2 shards × 3 segments);
   - the same at eight layers and `d` 256 (4 shards × 2 segments);
   - the golden vector's gated-delta program `gdn-k2-v4-grouped` (two layers of per-layer `Fixed` state,
     `C = 2`; 2 shards × 4 segments).

### 9.1 Results (2026-10-01)

- **The honest trace verifies cell by cell**: 6 cells, 14 cells (with the 8-layer fixture) and 8 cells.
  No cell asked for a param outside its shard. The cells' checks **tile** the committed leaves: every
  leaf is checked by exactly one cell.
- **A tampered leaf inside a cell** (a commit point of layer 2 at position 5, one value moved by one)
  is found by that cell, **at that leaf**. The other five cells verify.
- **A consistent lie at a boundary row.** Layer 1's carry-out at position 6 is moved, and every later
  leaf is computed honestly from it. The **downstream** cell (layers 2–3, positions 4–8) verifies:
  its inputs and outputs agree. The **upstream** cell (layers 0–1, positions 4–8) finds the lie at
  the boundary row. This is the detection lemma's first consequence (§2.1), exactly.
- **A consistent lie in a state.** The GDN layer 1's state write at position 3 is moved, and the lie
  rides into the checkpoint the next segment starts from. The cell that writes it finds it at the
  checkpoint leaf. The next segment's cell starts from the lying checkpoint and verifies. Shard 0
  never reads it.
- **What a cell touches** (8 layers, 4 shards × 2 segments, `d` 256):

  | | per cell |
  | --- | --- |
  | weights | 25.3–26.8 % of all param bytes (`pre`'s embedding and `post`'s head ride with the end shards) |
  | committed trace read, as opened inputs | 0 % (the first segment of shard 0: tokens only) to 1.6 % (a late segment of a middle shard: its carry-ins and its layers' earlier history rows) |
  | committed trace checked | 12.2–13.1 % (one eighth) |

  At four layers and two shards the cells held 54 % of the weights, read up to 9.4 % of a 12-position
  trace, and checked 15.6–17.8 %. The history a late segment reads grows with its position (§7). At
  real widths it is a small fraction next to the weights, which dominate these toy models less than
  the per-layer 65,536-entry activation tables do.

## 10. Activation

**One dormant fence, `palw_tir_shard_v1`.** Past it, for IR classes only:

- `ClassShardPlanDeclared` may name an IR class (`S_L`, `S_P`);
- the stratified draw (ADR-0100 D4) draws per shard with an outsider per shard (§4.2);
- `ReceiptV4` is accepted (§4.3);
- the recount runs over cells (§4.4);
- `ShardReceiptLicensed` licenses an IR claim by parts (§4.5);
- `TirStepRun` is a DA unit (§3);
- the class room is the binding shard's (§6.2).

Legacy classes keep `palw_shard_court` / `palw_shard_licensing` and both refusals unchanged. The new
fence does not lift them for a legacy class.

`validate_palw_v2` requires these armed at or below the fence's height:

| prerequisite | why |
| --- | --- |
| `palw_tir_v1` | the IR one-move court that convicts what a cell finds |
| `palw_tir_fence2` | the IR DA units a cell's inputs are demanded by |
| `palw_verification_v2` | the S1 segments and masks |
| `palw_rcore_plus` | Q-1…Q-7, the recount the cells generalise |
| `palw_admission_independence` | the outsider rule the per-shard outsider generalises |

It requires a signature-context set carrying `palw_receipt_message_v4` and the outsider ticket's
domain.

**Fingerprinting**, the four places every fence has (memory note *a-some-only-fence-needs-its-never-collapse*):

1. an `Option<ForkActivation>` field;
2. its entry in `for_each_fence`;
3. **Some-only** writes in `consensus_params_id` and `consensus_schedule_id`, so a dormant network
   fingerprints as if the fence did not exist;
4. the **`never()` collapse** in `normalize_values_a_scheduled_fence_drags_with_it`, so a build that
   schedules it at `never()` and a build that lacks it agree.

Its height must be one **no other fence uses**, or the fork id (which hashes sorted, de-duplicated
heights) cannot tell a build that has it from one that does not.

**Drills, on the shipping binary, crossing the fence** (memory note *a-flag-day-needs-a-drill-that-crosses-it*):

| drill | what it shows |
| --- | --- |
| D-S1 cross | a TIR class with a 2-shard × 2-segment plan on a local devnet; a claim bound before the fence licenses whole; one bound after licenses by parts with an outsider in each; both go Final |
| D-S2 cell lie | the producer commits a wrong leaf inside shard 1 at one position; shard 1's seats find it; `TirShardCourtAccused` convicts; the claim voids and the bond is slashed |
| D-S3 consistent lie | the producer lies at the shard-0/shard-1 boundary row and computes shard 1 honestly from it; **shard 0** finds it (shard 1 verifies); conviction as D-S2 |
| D-S4 withholding | the producer refuses boundary rows; a seat files `TirStepRun`; silence is a default; void and slash |
| D-S5 a seat that holds a shard | a host whose ledger share cannot hold the whole class but holds a shard declares it, is drawn, verifies, and its receipt counts; duties per host before and after, measured |
| D-S6 outsider fetch | an outsider with no copy of the class fetches its shard's inventory rows and attests within the receipt window |

## Proposed Spec text (sketch)

New section 08 §8.7, "IR-sharded panels" (PALW-SH-*), applying past `palw_tir_shard_v1` to IR classes
with a declared plan:

- **PALW-SH-1 (the plan).** An IR class's shard plan is the derived layer partition of §1.1 for its
  declared `S_L`, `pre` on shard 0 and `post` on the last, and the position segments of §1.2 for its
  declared `S_P`. Two nodes derive the same plan.
- **PALW-SH-2 (a cell).** A cell is a shard's occurrences at a segment's positions. Its inputs are the
  leaves of §1.3 and the shard's inventory rows. Its outputs are every step leaf those occurrences
  produce at those positions.
- **PALW-SH-3 (Valid).** A `Valid` cell receipt attests that every output leaf of every cell in its mask
  equals the PALW-TIR function of the cell's committed inputs. A seat MUST NOT sign `Valid` for a cell
  it did not check.
- **PALW-SH-4 (the draw).** Per shard, `s_shard` seats from the bonds that declared it, one per operator,
  with the S1 assignment of PALW-VF-33 inside the shard. A shard short of operators refuses the draw.
- **PALW-SH-5 (the outsider).** For a bought class, each shard's panel has an outsider drawn from the
  network's base-class population (ADR-0147's draw with the shard in the ticket), attesting the whole
  shard. No part licence stands without its outsider's `Valid`.
- **PALW-SH-6 (the recount).** `basis_k = min(3, min over cells of distinct counted Valid signers
  covering it)`, over every part's receipts. Final requires `basis_k ≥ 2`.
- **PALW-SH-7 (liability).** A `Leaf` fault is charged to the receipts whose mask covers the leaf's
  cell; a `Whole` fault to every counted receipt.
- **PALW-SH-8 (inputs are opened).** Every input a seat uses is checked against the claim's step root
  (or the class root, for params) before use, whoever served it.
- **PALW-SH-9 (no new commitment).** A cell reads the step tree of RFC-0002 Phase F unchanged. Nothing
  a producer commits depends on the plan.

## Alternatives

| alternative | why not |
| --- | --- |
| Keep whole-job replay and add seats | supply scales with hosts that hold the whole model; for a model no seat holds it is zero |
| Position segments only (today's S1, with resume) | bounds a seat's compute, not what it holds: every layer's weights and history (§7) |
| Tensor parallelism (a layer split across seats) | a layer's matmuls would need cross-seat partial sums, and those are not committed; a new commitment per split, and no court input |
| Random leaf sampling (a seat checks `k` random leaves) | ADR-0098's numbers: 6.51 % per 300-token claim at `s = 5`; a cell check sees every leaf of its cell |
| ADR-0099's recompute form (the previous shard's rows) for IR classes as-is | it is this RFC's sequential mode; what ADR-0099 lacked for legacy classes was committed boundary rows for every node, which IR classes have |
| A per-shard flat licence without parts | ADR-0098 §1.2: one transaction holds the whole object only up to 8 shards |
| Outsider = a full-replay seat | an outsider for a model nobody else holds pleads `Incapable` and the claim voids (ADR-0147 §3's residual); a shard-sized outsider is what an honest stranger can hold |
| Aggregated or algebraic per-shard checks | RFC-0007 |

## Security and economic analysis

| failure | what happens |
| --- | --- |
| a lie inside a cell | that cell's seats find it (§2.1); the one-move court convicts; the claim voids, the bond is slashed, the reporter is paid |
| a consistent lie at a boundary row | the upstream cell finds it (§2.1 consequence 1); D-S3 |
| a corrupt quorum of one cell and a corrupt outsider of its shard | the part licenses a lie (probability §5); anyone who later checks the cell convicts within the court window; the attesting seats are slashed (S4) |
| the producer withholds boundary rows | the seats demand them (`TirStepRun`, `TirStepLeaf`); a default voids and slashes; the seats abstain `Unavailable` meanwhile |
| a seat serves a wrong boundary row to a peer | refused against the step root (PALW-SH-8); the receiving seat fetches from elsewhere |
| a shard nobody holds | the draw refuses; the claim voids at `BindTimeout` (ADR-0100); a class whose shards are thin has little room (§6.2) and stays small |
| an outsider who cannot fetch its shard | `Incapable`; the part does not license; the claim voids at its deadline (ADR-0147's cost on the claims of that model) |
| grinding the cell of a lie | the lie is committed before the anchor seeds the draw; the plan is derived and immutable; the cell is fixed before the seats |
| a plan the registrant chooses to its advantage | the plan is derived from the program and `S_L`; `S_L` and `S_P` are declared once and immutable, like the graph |
| a large operator in many shards | one seat per operator per shard; its correlated failure is in many cells, which raises detection of a single lie elsewhere and is bounded by the outsiders |
| `Fixed` replay at an unaligned segment | not possible: segments start at multiples of `lcm(C, h_tile)` |
| a long history read at a late segment | priced: it is the cell's history bytes, bounded by its layers; the H-dissection court adjudicates an attention leaf in `h_tile`-sized pieces |
| receipt bytes | grow `S_L`-fold (≈ 77 KB a 4-shard claim); RFC-0007 |

## Compatibility and migration

- Dormant: no fingerprint, object, receipt or rule changes until the fence is armed (§10).
- Legacy classes are untouched, and so are ADR-0099/0100's fences and refusals.
- A flat IR class (no plan) is exactly today's: `S_L = 1`, the recount over cells is the recount over
  segments.
- A claim bound before a class's plan keeps the whole licence it was drawn for (ADR-0100,
  `palw_claim_licenses_by_parts_v1`).
- Node side: the panel worker gains a cell verifier (the prototype's `layer_shard` check over
  `misaka-palw-tir-exec`; batched on a device by `misaka-palw-tir-gpu`), the readiness proof over a
  shard's rows, and the ledger's need for a shard (`PalwRoleMemoryNeedV1` over the shard's bytes).

## Open questions

Each question carries the **recommended default** (marked so) that this text assumes until the user
decides.

1. **One fence or two?** A plan-and-draw fence and a licensing fence would let a network draw stratified
   panels and attest cells before licensing by parts (shadow mode).
   *Recommended default:* **one fence**, `palw_tir_shard_v1`, armed only after a shadow period in which
   the node's cell verifier runs beside the whole-licence replay on IR claims and their verdicts are
   compared (node-only, no fence).
2. **The attesters per cell and the detection point (§5).** Two (`s_shard = 3`, Verification V2's own
   rule, plus the outsider: 2.9 % per-lie miss at `h = 2/3`, `f = 0.26`) or four (`S_P = 1`,
   `s_shard = 4`: 0.32 %, testnet-12's de-facto five replays' detection at its work). The choice trades
   supply against detection per lie; the court window stays the backstop.
   *Recommended default:* **two attesters a cell (`s_shard = 3`) plus the shard's outsider** on
   testnet-12, where supply binds. Mainnet's point is set by its own measured `h`.
3. **The outsider's span.** The whole shard, or one cell (cheaper: `1/S_P` of the compute, weaker
   independence).
   *Recommended default:* **the whole shard**. The outsider fetches the shard's weights anyway, and a
   batched pass over all positions costs little more than one segment.
4. **`TirStepRun`.** A new DA unit, or peer serving plus `TirStepNode` only.
   *Recommended default:* **the new unit**. A segment's carry-ins are runs, and one `TirStepLeaf` per
   position cannot be demanded within the session limits.
5. **2-D at first, or layers only?**
   *Recommended default:* **layers only at first (`S_P = 1`; each seat attests its whole shard)**,
   with position segments switched on per class once batch verification is the node's common path.
   `S_P = s_shard − 1` (S1 inside the shard) is the target.
6. **Locks and pay by `w_cell / w` (§6)**, or per seat as today with the floor doing the work.
   *Recommended default:* **by `w_cell / w`, with the floor**. It is what lets a bond seat more shard
   duties; the slash term keeps the deterrent whole.
7. **The class room (§6.2)** as the binding shard's `ready_eff`, and the readiness proof over a shard's
   rows. Both change ADR-0160's lane-verify arithmetic.
   *Recommended default:* **adopt both**, together with the licensing fence; until then the class room
   stays ADR-0160's.
8. **Who declares `S_L`?** The registrant, once (ADR-0100), or derived from a network-wide seat budget
   (ADR-0099 Decision 2's `palw_shard_plan_for_seat_v1`), so that a class cannot be over-sharded to thin
   its panels.
   *Recommended default:* **derived from the network's seat budget** (the fewest shards whose widest
   shard fits it). The registrant may declare MORE shards only up to twice that number, so that a class
   cannot thin its own panels.
9. **The measured-speed constant (§6.2).** Batch verification is faster than the replay the constant was
   measured on.
   *Recommended default:* **leave it**. Raise it under its own fence, after a drill measures the cell
   verifier on the network's hosts.

## Decision

<Open.>

## Mission alignment amendment — 2026-10-07

§2のfirst-divergence lemmaは、必要な認証境界と全対象commit pointを取得・検査した場合の局所化の補題である。honest seatが存在するだけの検出保証でも、公開証拠の取得やcourt proof生成の完成証明でもない。cell/shardのopening・重み・stateと有界dissection/closeを、選出されていない普通のpublic bondにも提供する。outsiderの選出やcell quorumを、そのbondの訴追権の条件にしない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
