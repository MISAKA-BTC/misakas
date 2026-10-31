# ADR-v2-0014 — Versioned kernel・拡張境界・runtime backend

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0002](../rfc-v2/0002-tensor-ir-and-shared-query-data.md) / [RFC-v2-0003](../rfc-v2/0003-model-onboarding-and-artifacts.md)

## 1. この文書の役割

モデル追加と実行engine追加を分離し、合意に見える意味だけをversion管理する。

## 2. 統合した判断の読み方

### 2.1

最適化の境界はkernel semanticsであり、backendは宣言domain全体でreferenceとbyte単位に一致することを要求する。許容誤差を導入したものを同じ最適化と呼ばない。[旧ADR-0057](../adr/0057-palw-base0-runtime-acceleration.md)

### 2.2

classをchain dataとして登録し、buildは許されたkernelを実装する。compiled model tableを恒久的な登録許可者にしない。[旧ADR-0067](../adr/0067-classes-are-chain-data-kernels-are-the-build.md)

### 2.3

embeddingの読取りと有効化されたkernelのidentityを整合させる。拡張はversioned kernelで行い、汎用ML VMへ置き換える方針は採らない。[旧ADR-0102](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) [旧ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)

## 3. 維持する境界・後続改定

汎用VMの不採用は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)に一本化して掲示する。GPU等の利用可否と計算意味の変更を混同せず、backendの速さを合意上の正しさの代わりにしない。

## 4. 隣接する判断との分担

算術意味は[ADR-v2-0013](0013-tir-arithmetic-and-determinism.md)、hostのsandboxは[ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md)、classの登録は[ADR-v2-0015](0015-permissionless-model-registration.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0057-palw-base0-runtime-acceleration.md](../adr/0057-palw-base0-runtime-acceleration.md) | L1–L153 | L12–L14 |
| [0067-classes-are-chain-data-kernels-are-the-build.md](../adr/0067-classes-are-chain-data-kernels-are-the-build.md) | L1–L761 | L17–L34 |
| [0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) | L1–L252 | L12–L29 |
| [0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | L1–L204 | L15–L16 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0057-palw-base0-runtime-acceleration"></a>

<details>
<summary>0057-palw-base0-runtime-acceleration.md — 原文 L1–L153</summary>

[原文を開く](../adr/0057-palw-base0-runtime-acceleration.md) · SHA-256: `6a7d991f76f5cc2ee57e6e1b39c869fb8ca9dfe3ee5e813324e4c2b7747c8a84`

<!-- ADR_SOURCE_BEGIN:0057-palw-base0-runtime-acceleration.md -->
~~~~~~~~markdown
# ADR-0057: BASE-0 runtime acceleration — backends below the semantic boundary

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Accepted.** Consensus-inert: no class id, no catalog entry, no ruleset field and no
fingerprint moves under anything this ADR permits. A change that would move one is, by that fact,
outside this ADR's scope.

Date: 2026-08-27

Relates to: ADR-0040 Decision E (integer associativity — the property every decision here spends),
ADR-0049 (the canonical IR, which is where dispatch lives), ADR-0051/0053 (the float-runtime family
and its withdrawal — the road this ADR exists not to retake), ADR-0052 (the Qwen3.6 hybrid class,
the workload being accelerated), ADR-0038 Decision D (wall-clock never enters consensus).

---

## Context — the question, and the wrong answer already taken once

The engines are correct and slow. The reference-shaped Rust runs the real 33 GiB Qwen3.6-35B-A3B at
~9 s per canonical (8, 2) job on the reference M4 Pro; the A16 dense tier reached 30 tok/s only
after a first round of hand-written CPU kernels (13.5× over the scalar reference, measured on
Qwen2.5-1.5B). A public network feels this in four places: producer cadence, panel seats answering
duties for the model tiers, court replay cost, and the from-genesis IBD headroom multiple — every
one of them a wall-clock quantity that consensus deliberately does not price (ADR-0038 D), which is
exactly why making them faster is free.

An external survey (2026-08-27) recommends the obvious sources: llama.cpp's backends, FlashInfer's
CUDA kernels, MLC/TVM compilation, MoE expert-streaming runtimes. The naive reading — swap the
runtime — is a road this repository has already taken and withdrawn: Family M rode a pinned
llama.cpp + Metal build, and ADR-0053 removed it because a per-vendor float class verifies by
tolerant replay, and tolerant replay can never convict. Whatever is adopted from that survey must
not reintroduce the thing ADR-0053 buried.

What makes adoption possible at all is ADR-0040 Decision E. BASE-0's arithmetic is integer and
exact: `i32`/`i64` accumulation is associative and commutative, so the order a dot product is
reduced in — across SIMD widths, tile shapes, thread counts, GPUs, vendors — **cannot change the
result**, within the overflow bounds the catalog already makes premises. A float kernel library is
fast because it renegotiates arithmetic; an integer backend can be fast while performing,
bit-for-bit, the same function. `misaka/palw/base0/src/optimized.rs` already proves the property in
miniature (interleaved, blocked and reversed reductions, all bit-identical to the scalar fold), and
`kernels.rs` already ships fast grouped matmuls pinned by
`the_fast_kernel_is_bit_identical_to_the_reference`. This ADR is that precedent, generalized and
given rules.

## Decision 1 — the semantic boundary is the catalogued kernel

The unit of optimization is one `kernel_semantics_id`. A backend may implement a kernel any way it
likes — SIMD, GPU, fused, tiled, out of order — provided its output is **byte-identical to the
reference for every input in the kernel's declared domain**. Equality is exact; there is no
tolerance, no epsilon, no rank correlation, no "p95 within bounds". A backend that needs tolerance
is a different class wearing an optimization's name, and ADR-0053 already decided what happens to
those.

Everything above the boundary is untouched: the class id (the graph's), the artifact root, the
step-leaf trace, receipts, the court, the catalog. **A conforming backend is consensus-invisible.**
Which host ran which backend is not a chain fact, needs no registration, and appears in no object.

## Decision 2 — no kernel certificates

The survey proposes certifying optimized kernels on-chain and replaying the reference only in
disputes. Rejected. A certificate is a declaration, and this repository's recurring lesson is that
declared facts drift from derived ones. Here the derived fact is checkable locally (equality
against the reference) and the failure mode is already priced: a backend that ever diverges
produces a wrong committed row, which is a refutable step fault — the existing court convicts it
and the existing bond pays for it. The chain does not need to know backends exist, because a wrong
one is indistinguishable from a dishonest producer, and that machinery is built.

For the same defense-in-depth reason the split stays asymmetric: **producers and panel seats may
run any conforming backend; the court's adjudicator keeps the reference path** (and `ref2`, the
author-independent second implementation, keeps existing beside it). The party whose output becomes
a verdict is the one party that never takes the fast path.

## Decision 3 — the gate is differential, per backend, and it must fire

A backend merges only with its equality gate wired into the default-member test set:

* the full KAT corpus (17,881 vectors) run through the backend and compared byte-for-byte;
* the structural sweeps `optimized.rs` pioneered — interleaved, blocked, reversed — at the
  backend's real tile shapes;
* adversarial vectors at the edges the catalog names: saturation boundaries, SRDHM rounding
  midpoints (the vendored gemmlowp is the spec), zero-point extremes, maximum-K reductions,
  ragged final tiles;
* one end-to-end job per registered class, backend vs reference, compared at every committed row.

A gate that exists but is not in the default members is the gate this repository has been burned by
four times; wiring it is part of the backend, not a follow-up.

## Decision 4 — the order of work

1. **CPU SIMD** for the one hot loop every class shares: `matmul-quant/i8xi8-i32-exact` in its
   grouped and wide forms (Qwen3.6's eight-expert concatenation is a single `[4096 × 2048]` GEMM by
   construction — the IR already shaped the workload for exactly this). x86 AVX2 / AVX-512-VNNI and
   ARM NEON `sdot`/`i8mm`, plus the fused requantize (legal under Decision 6). This is where the
   A16 tier found 13.5×, applied to the tier that matters most.
2. **Apple Metal, integer compute.** Not simdgroup float matrices — `i8` products accumulated in
   `i32`, where GPU-parallel reduction is exact for the same reason CPU-parallel reduction is. The
   target host is the M-class producer already running the 33 GiB artifact; residency stays mmap.
3. **CUDA `dp4a`/IMMA.** Int8 tensor-core paths accumulate in `i32` exactly. FlashInfer is a
   *scheduling* reference here — paged KV layouts, MoE grouping, graph capture — never a kernel
   source: its kernels are float-centric, and anything transcribed must pass Decision 3 whole.
4. **Residency** (the FreeToken-adjacent idea, kept): expert page cache and NVMe streaming for the
   33 GiB artifact on small-RAM hosts. Pure memory locality; touches nothing committed; interacts
   only with the existing host lease.
5. **Research, explicitly not decided:** batched verification of decode steps (admissible only if
   every committed step leaf still materializes bit-identically — the draft-model half of
   speculative decoding is a foreign forward pass and stays out), and codegen from the canonical IR
   (the MLC/TVM direction; ADR-0049 built the IR it would consume, but a compiler toolchain inside
   consensus-adjacent code is a dependency decision this ADR does not make).

## Decision 5 — what the survey suggested that is refused by name

* **Runtime substitution** (llama.cpp, ik_llama.cpp, FreeToken as the engine): re-runs the
  ADR-0051 → ADR-0053 arc. llama.cpp remains a design reference for backend structure; its
  arithmetic does not enter.
* **New quantizations** (IQK, Trellis, TurboQuant…): a different weight encoding is a different
  artifact root and usually a different graph — a **class registration**, with ADR-0052's whole
  burden (catalog coverage, adjudicators, court cost), never an optimization. The Qwen2.5 PTQ
  degeneration (`[11,11,11,11]`) is the standing reminder of what "just requantize it" costs.
* **Tolerant equivalence** in any form. The Ambient PoL survey (ADR-0026) already concluded
  tolerance belongs to systems that cannot convict; this one can.

## Decision 6 — fusion stops at the committed row

The observable set is what the trace commits: every step-leaf row, checkpoint row and logits
opening the class's profile declares. Below that, backends may fuse freely — matmul + requantize in
one pass, norm + projection, whole-layer megakernels — because an intermediate nobody committed is
nobody's business. At the boundary, the committed row must materialize byte-identically. A fusion
that changes which rows *exist* is a profile change, i.e. a new class.

## Consequences

* No re-mint, ever, from anything conforming: backends land per-host, per-release, incrementally,
  and hosts on different backends interoperate silently.
* Producer cadence, seat latency, court replay and IBD headroom all improve without a consensus
  variable moving; `pwu` stays the counted step-leaf cost (ADR-0038 D), so acceleration changes
  who can afford to participate, not what work is worth.
* CI grows a per-backend differential matrix; that cost is the price of Decision 2's "no
  certificates" and is paid in tests rather than in trust.
* A divergent backend in the wild is a slashed producer, not a chain event. The safety argument is
  unchanged because the court never learned backends exist.

## What this ADR does not decide

Per-class hardware requirements, a serving/scheduling stack above the engine, remote attestation of
backends (deliberately nothing — Decision 2), and the compiler question (Stage 5 research).
~~~~~~~~
<!-- ADR_SOURCE_END:0057-palw-base0-runtime-acceleration.md -->

</details>

<a id="source-0067-classes-are-chain-data-kernels-are-the-build"></a>

<details>
<summary>0067-classes-are-chain-data-kernels-are-the-build.md — 原文 L1–L761</summary>

[原文を開く](../adr/0067-classes-are-chain-data-kernels-are-the-build.md) · SHA-256: `28bfa9a545e16e25f06a0129bca3b0b3e793d7bad591bbd2abac0f7da4d4fb9c`

<!-- ADR_SOURCE_BEGIN:0067-classes-are-chain-data-kernels-are-the-build.md -->
~~~~~~~~markdown
# ADR-0067 — Classes are chain data; only kernels are the build

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.

> **Distribution boundary, 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) restores model supply to optional off-chain activity and abolishes MISAKA Torrent / dedicated Seeder rewards. Immutable identity and claim-specific computation evidence remain separate; model acquisition is not a reward/weight gate. Model-bond allocation and conditional public prosecution require new economic/implementation validation; old tests retain scope.


Status: **Decisions 1–3 and 5 LANDED for the dense (A16) container (2026-08-31), fenced and
dormant; Decision 4 needed no code; Decision 6 including its cache bound LANDED (2026-09-01), as did
the pruning-point sidecar and the cross-architecture clause. The mmap (Qwen3.6) interpreter's blocker —
a measured defect in the class's registered graph — is CLOSED by the corrected `graph-v2` row, and the
interpreter itself is BUILT and differential-tested (2026-09-01), a pass that convicted a FOURTH
graph defect on the way; the mmap container's chain-resolve arm is BUILT behind the same fence
(same day — the registered root decides the container, and the mmap engine serves the declaration
bit-identically to its compiled path); the epsilon disposition is TAKEN (same day — the corrected
rows are `graph-v3`, declaring the artifact's measured epsilon, after the graph author's review
convicted a SIXTH defect that folded into the same id move); the mmap profile fuzz gate is BUILT
and run to saturation with zero findings (same day — and its design pass convicted a reachable
walk panic before the first iteration ran); only the operational arming remains PROPOSED.** See
"What landed" at
the foot of this document. Builds on ADR-0049 (the adjudication contract — whose admission
carriage is the load-bearing half of this design and is ALREADY LIVE), ADR-0054 (share follows
production — the economics that make permissionless classes survivable), and ADR-0034 (capability
declarations — the opt-in that makes them safe to serve). Consistent with the standing doctrine
that consensus changes ship by activation, never by re-genesis.

> **Security amendment appended (2026-09-02)** — see the last section: Decision 5's fence is also a security fence (ADR-0079 Decision 11's two arming conditions); resolution is off the consensus thread and fails closed to "cannot serve"; re-entry after eviction re-verifies from bytes; R-7 (unpaid seats) is a security item.

## The goal, stated as a test

MISAKA is meant to be a permissionless chain. Today it fails one specific permissionless test:
**a model cannot join the network without a central party cutting a release and every node
updating.** The class catalog (`canonical_classes_v1`) is compiled into the binary, and
registration, production and adjudication all read it — so "add a model" is, operationally,
"open a PR against this repository, wait for a maintainer to merge it, wait for a release, wait
for the fleet to upgrade." Whoever controls `main` controls the model set. That is a permission,
held by a central party, over the chain's defining activity.

The test this ADR wants the chain to pass: **a stranger with weights, a converter and a bond can
put a new class on the chain, and nodes that never heard of it can validate, serve and adjudicate
it — with no release in the critical path.**

## What is already true (measured, not planned)

The investigation that produced this ADR found the design half-built, which is what makes it an
ADR rather than a research program:

1. **Admission is already data-driven.** `ClassRegistered` carries a
   `PalwClassAdmissionCarriageV2` — the FULL `PalwShapeProfileV3` (the graph) and the canonical
   job — and `verify_class_admission_v2` validates the class FROM THE WIRE: shape validation,
   the coverage gate, ladder depth, the court-cost ceiling, the PWU recount. `class_id` must
   equal `profile.shape_profile_id()`, so the identity is derived from the carried data, not
   looked up. A node that has never seen a class in any table can already refuse or accept its
   registration completely. (ADR-0049 Decision H put the carriage on the object; the gate
   enforcing it from the wire is live on testnet-11 today.)

2. **The coverage gate's reference set is the KERNEL table, not the class table.**
   `verify_catalog_coverage_v1` checks the profile's kernel ids against
   `catalogued_kernel_ids_v1()` — the set of kernels this build's adjudicator can compute. A
   profile that references only catalogued kernels passes on any node, whether or not that node's
   class table has a row for it. The boundary between "data" and "build" already runs through the
   kernel set at the one place consensus checks it.

3. **Panel service is already opt-in per class.** Sortition draws seats only from bonds whose
   ADR-0034 capability declaration names the claim's `runtime_class_id`
   (`eligible_seats_v3` filters on it). Nobody is ever drawn to judge a class they did not
   declare, so a permissionless class cannot conscript anyone.

4. **The economics of a permissionless registry are already decided.** ADR-0054: registration
   requires an active bond and a signature over the class-and-share (spam pays rent), an entrant
   starts at the minimum grantable share (1‰), share grows only by filling the budget that share
   admits, and silence returns it to the floor. A garbage class that certifies is still garbage —
   and decays to nothing without anyone adjudicating its quality.

5. **The execution arithmetic is cross-architecture deterministic on the real class.** Measured
   2026-08-31: the same prompt on the same 1.7 GiB artifact produced identical output ids, all
   four leg roots, execution root, CU and claim id on arm64 and x86-64. The kernel set is the
   thing that makes this true; nothing in this ADR touches it.

## The one thing that is not true

**Execution resolves through the build.** `PalwClassSdk::resolve(class_id, artifact_root)` walks
the compiled lineage impls, each of which matches against the compiled class table, and
constructs an engine for the rows it knows. A class the table lacks answers
`"this node cannot serve the registered class"` — from a node that just VALIDATED that class's
registration from the wire. The chain can admit what the node cannot run. Closing that one gap is
this ADR.

## Decision 1 — the chain state is the class catalog; the compiled table is genesis bootstrap plus cache

`resolve` gains a second arm: when the compiled table has no row for `(class_id, artifact_root)`,
the SDK reads the class's registration from chain state — the carriage's profile IS the row. The
compiled table keeps exactly two jobs: it seeds the genesis classes (which exist before any chain
state does), and it serves as a verified cache for classes it happens to know. Authority moves to
the chain; the table stops being a gate.

The known-weights rule, the sibling filters and `registration_candidate` read the same
chain-derived ledger, so "which classes exist" has one source. The rule that a class IS its graph
is unchanged — it is what makes this safe, because a chain-carried profile hashes to the same
`class_id` on every node or it is a different class.

## Decision 2 — execution from the registered profile

Each container family gains an engine mode constructed from a `PalwShapeProfileV3` rather than
from a compiled row: the profile's pre/attn/post node tables become the op sequence, the
geometry sizes the buffers, and the artifact supplies the weights. ADR-0049's canonical IR is the
substrate — the op sequence is already generated from IR for the catalogued classes; this
decision makes the interpreter the ONLY path for chain-registered classes, and the compiled
specializations an optimization for the rows the build knows.

Two properties fall out structurally rather than by discipline:

* **ADR-0049 Decision F becomes unviolatable for interpreted classes.** Decision F requires the
  profile to name every narrowing the engine performs; the A16 v1 class was refused on the
  free-prompt lane precisely because its profile omitted a requant its engine performed. An
  engine BUILT FROM the profile cannot perform a narrowing the profile does not name — the
  correspondence check becomes the constructor.
* **A registered class needs no per-class code review**, because there is no per-class code. The
  review surface collapses to the interpreter and the kernel set, both of which are the build's
  and both of which every class shares.

Scope, stated plainly: this covers any model expressible in the catalogued kernel set — new
checkpoints, new sizes, new context widths, new quantization maps of the families the kernels
serve. It does not cover new architectures; see Decision 3.

> **2026-10-06 future-profile clarification:** [ADR0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
> preserves this kernel/code versus model/data boundary. Typed TIR/VerificationPlan composition is
> data over defined kernels, not a universal uploaded instruction set. A new semantic/checker/court
> family requires a coordinated versioned kernel update. The legacy replay wording below does not
> override [ADR0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)'s normal small
> constraint checks and exact-dispute-only policy for new large-model profiles. Existing classes
> retain their historical rules until separately fenced transitions.

## Decision 3 — the kernel set is the consensus surface, and it is irreducible

Adjudication means a seat recomputes the same arithmetic and compares. Executable semantics
cannot ship as data — a "portable" op language would just be a VM whose instruction set is the
same boundary renamed, with a determinism proof owed for every op on every architecture. So the
honest boundary is the one the coverage gate already enforces: **kernels are code, and a new
kernel is a consensus change**, shipped the way this chain ships consensus changes — by release
and activation, never by re-genesis.

What this decision buys is the sharp statement of what a release is FOR. Today a release gates
every model. After this ADR a release gates only:

| change | needs a release? |
|---|---|
| new checkpoint / size / n_ctx / quant map of a served family | **no** — register it |
| new model family whose graph the kernel set expresses | **no** — register it |
| new kernel (new attention variant, new activation, new routing) | yes — consensus change |
| interpreter or gate changes | yes — they are the build |

A maintainer keeps authority over the ARITHMETIC the chain can adjudicate. A maintainer loses
authority over WHICH MODELS use it. That is the intended shape of a permissionless compute chain:
the rules are governed, the participants are not.

## Decision 4 — distribution and service stay off-chain, and the chain keeps pinning only identity

`ClassRegistered` continues to carry no URL. The chain pins WHAT (class id, artifact root); WHERE
is Hugging Face, a mirror, a torrent — anyone's problem and everyone's option, made safe by the
bit-reproducible conversion (two honest converters land on the same root, so any source that
serves the pinned bytes is as good as any other).

Seat service likewise stays a market: a chain-registered class still needs seats that chose to
download its artifact and declare its capability (ADR-0034), and a panel still needs the quorum's
worth of them. The protocol's job ends at making service POSSIBLE without a release; making it
HAPPEN is the registrant's distribution work. This is the same division ADR-0054 drew for
economics — the chain prices participation, it does not recruit participants.

## Decision 5 — the interpreter ships behind a fence, and the fence's arming condition is a fuzz gate

A profile interpreter is a larger attack surface than a compiled row: the adversarial input space
is now "every profile the admission gate accepts" rather than "the rows a reviewer merged". The
existing wire-enforced bounds already contain the blast radius — the court-cost ceiling caps what
adjudicating any admitted class can cost, the PWU recount caps what it can claim to be worth, the
ladder-depth check caps the dispute tree, and a bond must sign the registration — but "bounded"
is not "verified".

So, per this repository's fence discipline (ADR-0066's shape): the chain-registered-class arm of
`resolve` ships DORMANT behind a top-level fence, and the arming condition is stated now, before
any code exists to argue with it:

* a profile-space fuzzer that drives arbitrary gate-accepted profiles through the interpreter and
  the court's close path, run to saturation with zero panics, zero non-determinism between two
  architectures, and zero closes over the ceiling;
* the interpreter's output proven bit-identical to the compiled engines on every class the build
  also carries (the compiled rows become the interpreter's reference vectors — differential
  testing the catalog gives us for free);
* one full lattice walked on a devnet for a class that exists ONLY as chain data: registered from
  the wire, served by declared seats, a claim committed, replayed, licensed and finalized, and a
  receipt block minted — with no row for it in any binary.

## Decision 6 — four storage tiers, and node storage is never consensus state

Permissionless registration multiplies MODELS; it must not multiply every node's DISK. The failure
to refuse is mechanical: N registered classes at tens of GiB each, times a fleet that treats
"registered" as "must hold", is a fleet whose storage grows with strangers' decisions — and the
chain never asked for that. The chain's own design already refuses it (the registration carries
kilobytes and no URL); this decision makes the node side match, as four tiers with different
obligations:

| tier | holds | size | who must have it |
|---|---|---|---|
| ① chain state | class id, profile, canonical job, artifact root, admission facts | KB | every node — it IS consensus |
| ② validation artifact | a real-weights SLICE of the model plus its expected rows (the converter's `--layers N` shape), enough to prove THIS build's kernels reproduce THIS class's arithmetic bit-for-bit | MB | a node DECIDING whether to serve |
| ③ full model artifact | the weights the work runs on | GB–tens of GB | only nodes that CHOSE the class |
| ④ node cache | whichever ③s this operator serves, under an operator-set byte bound with demand eviction | bounded | nobody — it is local policy |

**Registration and possession are different acts, and nothing may couple them.** Registering a
class obligates no node to fetch anything; a node that never heard of your model validates your
registration from tier ① alone (the wire-enforced gate needs nothing else). Sortition already
draws seats only from bonds that DECLARED the class (ADR-0034), so a node that ignores a class is
never conscripted into judging it. "Every node holds every model" is not a degraded mode this
design tolerates — it is a bug in an operator's configuration.

Two honest boundaries, stated so the tiers do not over-promise:

* **Tier ② de-risks the fetch; it does not license the declaration.** A seat's duty is a REPLAY of
  the full job on the full weights, so a capability declaration backed by only the validation
  slice is the documented Incapable trap: the seat draws, cannot serve, files `Incapable`, and
  enough such seats make the claim's quorums unreachable — the declaration would kill the very
  claims it advertised for. Declaration remains "I hold ③ and can serve it"; tier ② is what lets
  an operator learn, for megabytes, whether fetching gigabytes would even be compatible — the
  plan compiles, the kernels land on the slice's expected rows — before spending the bandwidth.
* **Eviction must retract what it invalidates.** A cache that drops ③ while the bond's capability
  declaration stands re-creates the same trap on a timer. Declarations already carry a liveness
  window (`is_live_at`); an evicting node stops renewing the class's declaration FIRST and evicts
  after the window lapses, so sortition stops drawing it before the artifact is gone.

This is also where the economics point back at R-7: model count grows demand for serving and for
seats, and the supply is a MARKET — operators choosing classes worth their disk and their
compute — not a protocol obligation. A permissionless registry with conscripted storage would be
neither permissionless nor operable; a registry where holding is chosen needs holding to be worth
choosing, which is one more reason seat payment (R-7) is the next economic ADR this one leans on.

## What this does not do, said plainly

* **It does not remove releases from new arithmetic.** A new architecture still waits on a kernel,
  and a kernel is a consensus change. Permissionless has a floor, and the floor is "the network
  can recompute you".
* **It does not make quality permissionless — nothing does.** The gate proves adjudicability, not
  worth. ADR-0054's economics are the quality mechanism: a class nobody mines returns its share
  to the floor and its registration becomes rent paid to nothing.
* **It does not solve seat incentives, and it leans on the problem harder.** Panel seats are
  currently unpaid (the R-7 finding stands: nothing in the transition pays a
  `PalwPanelSeatV2`). A stranger's class needs five declared seats, and today those seats serve
  from goodwill. Fixing seat payment is its own ADR and becomes MORE urgent the moment classes
  are permissionless; this ADR knowingly increases demand for volunteer adjudication and does not
  supply it.
* **It does not change any consensus rule today.** Everything consensus-side that this design
  needs (the carriage, the wire-enforced gate, the kernel-set coverage check, capability-filtered
  sortition, ADR-0054's economics) is already live. The work is node-side: the SDK's resolve arm,
  the interpreter mode, the fuzz gate, the fence. Fingerprints do not move.

## The order of work

1. The interpreter mode for the dense (A16) container, differential-tested against the compiled
   rows the build already carries.
2. The `resolve` chain-state arm behind the fence, plus the ledger unification of Decision 1.
3. The fuzz gate of Decision 5, run to its stated saturation.
4. The devnet lattice walk with a chain-only class.
5. The validation-artifact format for the dense container (a `--layers N` slice plus expected
   rows), and the operator cache bound with declaration-first eviction (Decision 6).
6. Arm on a testnet re-launch or activation; the mmap (Qwen3.6) container follows the same path
   second, because its interpreter is a larger piece and the dense family proves the seam.


## What landed (2026-08-31)

* **The interpreter** (`A16Engine::plan_from_profile` / `forward_token_planned`): execution from
  the registered declaration, one committed row per declared node, refusals naming the node. The
  differential against the compiled engine holds logits, every table row and the cache state
  bit-identical — and CAUGHT A REAL DEFECT before any claim existed: the compiled engine emitted
  its SwiGLU trace rows out of the declared order, so the free-prompt capture was committing the
  silu row at the up-projection's slot; a court bisecting there would have convicted an honest
  producer. The engine now emits in the declared order.
* **The fence and the arm** (`PalwClassSdk::with_chain_classes_v1` /
  `resolve_chain_registered`): sealed by default, armed only by the greppable constructor;
  refusal order — fence, profile-hashes-to-id, canonical-names-the-class, artifact-held
  (Decision 6's "registration does not obligate possession"), plan-compiles.
* **PART of the fuzz gate** (`fuzz_a16_profiles_v1`, `palw-a16-profile-fuzz`): seeded, clockless,
  gate→plan→double-execution under `catch_unwind`. Its first 400 iterations found a
  gate-and-plan-accepted profile whose rewired refs fed a kv-width row to the q-rope and panicked
  the head slicing; the fix made the PLAN width-sound (every consumer's input width checked
  statically), eliminating the class. Saturation: 50,000 iterations, 6,207 executing, zero
  panics, zero nondeterminism.

  **The court's side, which the first version of this gate did not have.** Every gate-accepted
  profile now also has its worst close derived (`derive_court_cost_v1` — the gate's own function)
  and measured against `PALW_RC_COURT_MAX_CLOSE_BYTES`, with `closes_over_ceiling` and
  `max_close_bytes_seen` as columns, because a class the gate admits whose disputes cannot be
  carried executes, certifies and can never be policed — a defect of the adjudication half that
  no amount of executing can see. Measured at 400 iterations: 46 costed, worst close 20,257 bytes
  against a ceiling of 81,920, zero over.

  **The differential now runs over the classes this build carries**, not only synthetic
  geometries: every A16-family catalog row's real profile is compiled and the planner's answer
  pinned, and each servable row is compared row-for-row against the compiled engine at a runnable
  geometry. It pins the v1 rows' divergence as the finding it is — the compiled engine performs a
  narrowing their graph does not name, so an interpreter executing their declaration commits one
  pre row where the engine commits two, which is exactly why Decision F refuses them on this lane.

  Still honest about one clause: **"zero non-determinism between two architectures" is measured as
  two runs in one process.** The cross-architecture evidence this repo has is the manual
  arm64/x86-64 comparison of one real class (identical roots, CU and claim id), not a fuzz
  property — closing that means running the same seeded corpus on two machines and diffing the
  tallies, which is CI work, not code.
* **The chain-only lattice walk**: a class in NO table — wire-shaped registration with the
  carriage riding — applied, served through the armed arm, committed, bound (the duty naming the
  replay lane), licensed, Final, and the producer-built receipt-spend envelope admitted in full.
* **The registration index** (`PalwClassCarriages`, store prefix 226): consensus state retains a
  class's economic facts and drops the carriage, so the node keeps what the wire delivered —
  verbatim bytes, written where the lifecycle filter accepts the object (the path IBD replays),
  existence-gated against current state at every read. Exposed as
  `palw_registered_class_carriage_v1` through the consensus API and session.
* **The node half of the fence** (`--palw-chain-classes`): the panel's every duty resolve and the
  producer's class resolve route through `resolve_or_chain` — tables first, then (armed) the
  chain's own registration, and "the chain never registered it" never reads better than "this
  build cannot serve it".

* **The validation tier** (`palw-slice-kat emit|verify`): a validation artifact is an ordinary
  dense artifact converted with `--layers N` (real weights) plus a KAT binding a fixed token
  schedule to the bit-exact digest of every logits row. `verify` replays it on the candidate's
  machine and exits non-zero on a mismatch, naming the divergence; the same file carries the full
  model's identity facts machine-readably. A passing KAT licenses the FETCH, never a capability
  declaration — the tool says so on every success.

## The three remaining clauses, built (2026-09-01)

Three of the four items the section above listed as not landed are now code rather than prose.

* **The cache bound (Decision 6's ④).** `--palw-class-cache-bytes` is the number that makes
  "registration obligates no node to hold anything" true rather than aspirational. Artifacts load
  in the order the operator listed them — their priority, stated by them — and loading stops at the
  bound, measured from each file's size BEFORE it is read, because a bound that notices after
  loading is a bound that OOMs the node it was set to protect. What did not fit is NAMED in the
  log: a node quietly holding nine of ten artifacts would declare nine classes and look, to its
  operator, like it served ten. Declaration-first eviction then arrives structurally instead of as
  a rule somebody must remember — a class this node does not hold is one it cannot resolve, does
  not declare, and is never drawn to judge. Zero means unbounded, which is every node's behaviour
  before this and is right for one class on a dedicated box. Shared by the producer and the panel
  through one SDK entry point, so the two cannot drift.

* **The pruning-point sidecar.** `PruningPointPalwStateMessage` gained a repeated
  `PalwClassCarriageEntry` (additive field 3), the server fills it from the registered-carriage
  store, and the IBD client adopts each entry after the state it rides with. This is the
  unattended-fleet answer to the gap `--palw-class-carriage` covered by hand: a node syncing from
  a pruning point now receives the class carriages of classes registered before that point instead
  of needing an operator to carry them across.

* **The cross-architecture half of Decision 5.** Stated as a CI job over two machines, it is now a
  value in the artifact instead: the fuzz corpus digests its 400 seeded plans and their outcomes
  into `CORPUS_DIGEST_400`, pinned at
  `90939894923247d3e1eb18478b0495744e9ff0416bb9a20d16d01f0c411ff5eb`. Any machine running the suite
  asserts the same digest, so a divergence is a test failure on the machine that diverges rather
  than a report somebody has to compare by hand.

## Why the fourth is not merely unbuilt (2026-09-01)

The mmap (Qwen3.6) interpreter was staged second on the rollout above because it is the larger
piece. Building it turned up something the sequencing did not anticipate: **it cannot be built
correctly against the graph this class has registered, because that graph misdescribes the engine.**
Three of them are unambiguous, and all are measured mechanically by
`misaka/palw/base0/tests/qwen36_profile_conformance.rs` rather than argued from a reading:

* **The shared expert's gate is two different tensors under one name.** The engine reads
  `blk.N.ffn_shared_gate.weight` as the mixture's SCALAR gate — one output, through a sigmoid,
  applied to the whole shared row. The registered profile declares a node of that name **512
  outputs wide**, which is the shared expert's own gate projection. The artifact can supply
  exactly **one**. The engine already hit this collision and fixed its own side — `Qwen36Engine::expert`
  carries the scar, naming the expert's base `ffn_shared_expert` precisely so its gate does not land
  on the scalar's name — but the IR was never moved with it, so the chain's description of this
  class still carries the pre-fix naming. `ffn_shared_up.weight`, `ffn_shared_down.weight`,
  `ffn_shared_scalar.weight` and `ffn_shared_apply.a16` are declared and exist in no artifact.

* **An operand that chooses which experts run is named by no node.** `ffn_router_up.a16` is the
  router softmax's widening, read per layer. The engine's own comment measures the cost of getting
  it wrong: up to a factor of sixty-four in temperature, "enough to make the router select nearly
  uniformly or nearly one-hot" — it decides WHICH eight experts execute. ADR-0049 Decision F asks a
  profile to name every narrowing; this one does not merely narrow, it steers, and the profile does
  not name it.

A third measurement puts those two in proportion, because reporting them alone would understate
the gap. Counting the whole surface — quant params that ride with a declared weight excluded as
bookkeeping, global names kept whole — the registered profile names **53 operands**, an artifact of
this lineage carries **76**, and the two do not nest: **27 operands the engine reads are named by no
node**, and **14 names the profile declares are carried by no artifact**. Many of those are
defensible as FUSION — a node that stands for the eight chosen experts cannot name eight tensors,
and `ffn_gate_exps.routed` is a name for a computation rather than for bytes. That defence is also
the problem: a fused name resolves to bytes only under a rule, and no such rule is written anywhere.
An interpreter cannot follow a graph whose names it cannot resolve, and today the resolution lives
only in the compiled engine's hardcoded order — which is exactly the arrangement ADR-0067 exists to
end.

Neither of the two is breaking the live network today, and the reason is itself the point: this container
commits no step leg (`qwen36_roots_v1` says so in place — its `execution_root` is a composite over
the tiled logits trace), so nothing ever resolves these names against an artifact. They are inert
labels until an interpreter makes them the program. That is precisely the condition under which the
dense family's identical defect survived: the A16 differential found the SwiGLU rows out of declared
order only once something executed the declaration.

**The rule that was missing is now written, and it closes almost all of the gap.** The fusion was
never the problem — one node standing for "the eight chosen experts' gate projections" is a name for
a computation, not for a tensor anyone stored. The problem was that the mapping from a fused name to
the bytes it reads lived only inside the compiled engine's hardcoded order, which is precisely the
dependency this ADR exists to remove. `RESOLUTION` in the conformance test is that mapping, derived
by reading the engine's arms against the IR they claim to mirror and held to a real artifact: every
target must be a store an artifact of this lineage carries. With it, the 14 declared names no
artifact can deliver fall to **1**, and the 27 engine reads no node names fall to **0**.

**The one name left is not a name at all — it is a third finding.** The profile declares a
`VCacheWrite` node (`blk.N.attn_v_cache.a16`) that narrows the V projection as it enters the cache.
`Qwen36Engine::full_arm` pushes V in RAW — `cache.values[li].push(v)` — with no requant and no such
store in any artifact. K is normed and rotated before its cache write and the graph declares both;
the V path was written as though it were symmetric. This one cannot be closed by renaming: ADR-0030
gives a step leg one committed row per declared node, so a node with no computation behind it is a
slot that can never be filled, and every step leg of the class would be short by exactly one row.

**The corrected row now exists (2026-09-01, same day).** `qwen36_profile_v2` builds the corrected
tables: the shared-expert names the engine reads, the router widening named at the node that reads
it, the phantom V-cache node deleted with its `VCacheWrite` role moved onto the V projection — the
computation that actually feeds the cache. The GDN arm's own 24 nodes are COPIED from v1 at compile
time rather than transcribed, so they cannot drift; the attention arm's renumbering after the
deletion is hand-derived and machine-checked (`structural_diff_v1_v2` holds v2 to v1 node by node,
excusing exactly the corrections and requiring every step reference past the deletion to
shift by exactly one). Against the same artifact measurement that convicted v1, v2's residue is
**zero in both directions** with a resolution table reduced to the structural fusions alone, and
all three findings are asserted closed (`v2_leaves_no_residue`, `v2_closes_the_three_findings`).
The ledger carries `graph-v2` rows for all three lineage members. **Registered on no chain yet** —
registration is an operator action with a bond and a fee, and it stays one.

**The interpreter is built, and building it convicted a fourth defect (2026-09-01, same day).**
`Qwen36Engine::plan_from_profile` / `forward_token_planned`
(`misaka/palw/base0/src/qwen36_plan.rs`): the A16 seam for the mmap container — construction is
the admission check and it is width-sound; execution is one committed row per declared node in
the declared order, cache writes where the declared roles sit, and the `RESOLUTION_V2` fusion
rules (the conformance test's own table) implemented as the mapping from each fused name to the
stores it reads, cited rule by rule at the exec arms. The differentials: bit-identity against the
compiled engine on the hybrid and qwen3moe fixtures (logits, every cache, the recurrent state);
the committed row at every site the engine's probe can name, pinned to its v2 node index; a
per-ledger-row differential that shrinks each of the six rows to a runnable geometry keeping its
structure — the three `graph-v2` rows must serve and agree to the bit, the three v1 rows must be
refused by name; and a REAL-WEIGHTS differential (env-gated on an artifact path) that runs a
converted `.palwq36` through both paths, which is the one place the grouped per-32-exponent
kernels and calibrated clamps are actually exercised.

The fourth defect: **v1 declares the expert SwiGLU's wideness backwards** — the gate projection
narrow and the up wide, while the engine projects the gate WIDE (silu is nonlinear; its input
scale is part of the function) and the up narrow, in both the routed and the shared expert. The
name-conformance measurement cannot see this (the names are right), and the plain fixture
differential passed with the swap in place, because the fixture's derived scales keep every row
inside the 16-bit code rail where narrow and wide agree by construction — a differential with no
authority at that site. The conviction is `a_hot_gate_row_distinguishes_the_declared_wideness`:
heat the gate params until the products overrun the rail, and the declared-narrow interpreter
clamps where the engine carries (measured: logits diverge at position 0); the test also asserts a
lane actually left the rail, so the site cannot quietly go cold again. v2 now carries the
corrected labels (the four kernel parameters of the shared `moe_tail` macro; v1's spelling is
frozen with the rest of it), `structural_diff_v1_v2` excuses exactly those four in their exact
directions, and the v2 class id moved with its graph — pinned at `23ef487f…`, still registered on
no chain, which is what made the correction an edit instead of a third row.

**The superseded spelling reached the chain first (2026-09-01, 03:07 CEST — recorded, not
retracted, because a registration cannot be).** Thirteen minutes after the corrected labels were
pushed, a fleet node still running the previous build (`e74260cb`, whose ledger derives the OLD
v2 id) executed a name-based registration drop-in — `--palw-register-class=Qwen3.6-35B-A3B/graph-v2`
— and `069b9482…` was accepted in tx `6c3bdc67…`, block `01152888…`. What that row now is: a
permanent orphan carrying the fourth defect's backwards labels and the fifth finding's epsilon —
**structurally unservable on any build from the interpreter onward** (no current ledger derives
its id, so it resolves only through the chain arm, whose geometry gate refuses the declared 17
against the artifact's executed 1), servable only by the previous build's compiled table, idle
unless a producer names it, and decaying at ADR-0054's floor share. The damage is a sunk bond and
a squatted name. The corrected id `23ef487f…` remains registered nowhere, so the epsilon
disposition still lands as an edit to the ledger row — with one addition: when it lands, the
ledger row should take a NEW model_id (`graph-v3`) so that the name a chain row already answers
to stops meaning two different graphs in operator tooling; the known-weights seeding rule's
treatment of the suffix has to be checked when renaming. The order this forces is now doctrine
for the family: **measure, correct, re-pin, and only then register — a registration is the one
step that cannot be taken back, so it goes last.**

And the real-weights run surfaced a FIFTH, this one reaching past the graph into the geometry
constants: **the declared epsilon is not the executed one.** Every registered geometry of this
lineage pins `rms_eps_q = 17` (the integer spelling of the checkpoint's `1e-6`, "because the
court recomputes with the CLASS's constant"), and every real artifact carries `eps_q = 1` —
the converter hardcodes it (`qwen36-convert.rs`), and the engine normalizes with the ARTIFACT's
constant. `Qwen36CanonicalClassV1::shape_matches` even documents the split ("the engine's integer
epsilon is the artifact format's, the class constant is the court's") without noticing that it
describes a divergence: a court that ever recomputes a norm with 17 convicts an engine that ran
with 1. Nothing enforces it today — this container commits no step leg — which is the same
dormancy that hid the first three findings. The interpreter's geometry gate is where it stopped
being dormant: the graph-v2 row refuses to plan over its own class's artifact, on exactly that
field. Pinned in `the_interpreter_matches_the_engine_on_a_real_artifact`, whose differential then
runs under the declaration with the epsilon corrected to the artifact's and holds to the bit.
The disposition is NOT taken here: v2 is unregistered, so its geometry can still adopt the
artifact's epsilon (or the fleet's artifacts could someday be re-converted at 17, which moves
every artifact root) — but the 35B and Coder artifacts live on the fleet and their headers should
be read before any constant moves. Until then the finding stands as a refusal with a name.

**The epsilon disposition is taken, and the corrected rows are `graph-v3` (2026-09-01, later the
same day).** The fleet headers were read before the constant moved, as required above — three
`.palwq36` headers on the fleet (`original-from-hf` at 40 layers, `huihui-30b` at 48, and the
36.5 GB `qwen36.palwq36` the `6c3bdc67…` registration actually loaded) all carry `eps_q = 1`,
agreeing with the converter's hardcode and the local `Qwen3.5-2B` conversion: five measurements,
one value, no unknown left. So the geometries adopt it — `QWEN36_ARTIFACT_EPS_Q` and
`qwen36_geometry_artifact_eps` (`palw_qwen36_profile.rs`): the frozen consts stay exactly as v1
registered them and the corrected rows take the one-field update, transcribing nothing. This is
the dense family's own precedent replayed: `Qwen/Qwen2.5-1.5B` declared `rms_eps_q: 1` against an
artifact executing `1 << 8`, and its `graph-v2` row corrected the declaration to what the
converter builds — `classes.rs` calls that "the defect this class was born from".

**The graph author's review of the interpreter convicted a SIXTH defect, folded into the same id
move.** The mixture's scalar gate (`ffn_shared_scalar.weight` in v1, the engine's
`ffn_shared_gate.weight`) declares `KDESC_A16_MATMUL_RESCALE` — the UNGROUPED matmul, whose court
program (`palw_step_refute.rs`, `Qwen36Op::MatMulRescale`) reads no exponent table — while the
converter writes `.exp` for that tensor like every other projection (`qwen36-convert.rs`,
`with_exp(…ffn_shared_gate.weight…)`) and `Qwen36Engine::project` therefore runs the grouped-wide
kernel over it. Every existing measurement was structurally blind: the interpreter shares
`project()` with the engine, so their differential agrees at that node BY CONSTRUCTION; the
fixtures carry no `.exp`, so the ungrouped label is TRUE on fixtures; and the one real artifact
the differential has run on (`Qwen3.5-2B`) has no shared expert at all. The only party that would
ever see it is a court re-executing from the committed kernel id — anchoring the codes up to
`2^QWEN36_MAX_GROUP_EXP` below their scale and refuting an honest producer at every hybrid
layer's shared gate. The corrected arm declares `KDESC_Q36_MATMUL_GROUPED_WIDE` (a fifth kernel
parameter of the shared macro; v1 frozen, `structural_diff_v1_v2` excusing exactly five kernel
corrections in their exact directions), and the planner needs no change — it already treated the
rescale and grouped-wide ids as the same wideness, which is itself part of how the defect hid.

**The rows are `graph-v3`, and `graph_version: 2` is retired.** The chain answer to
"Qwen3.6-35B-A3B/graph-v2" is the orphaned `069b9482…`, so the corrected rows take a name no
chain row answers to; the seeding rule was checked as this section required — `base_model_id`
strips any wholly-numeric `/graph-vN` suffix, so registered weights admit a `graph-v3` revision
exactly as they did a v2 — and operator tooling must now spell
`--palw-register-class=…/graph-v3`. The corrected id is pinned at `5bd9ae3d…`
(`v3_shape_profile_id_golden_vector`): the corrected tables over the artifact-epsilon geometry,
the pair `classes.rs` actually registers. Registered on no chain, and registration stays an
operator action taken last. The teeth grew with the findings: the hot differential now heats
gates AND ups, routed AND shared, and every heated site must prove itself hot in BOTH arms
(`a_hot_gate_row_distinguishes_the_declared_wideness` — a wide row by lanes past the rail, a
narrow row by lanes saturated at it), because a global count would let a site go quietly cold,
which is how the fourth defect survived its first differential; the epsilon fact is pinned
unconditionally in a plain checkout (`the_corrected_rows_declare_the_artifact_epsilon`), not only
behind the env-gated real-weights run; and the real-weights differential now plans the row
directly while keeping the 17-refusal pinned as a refusal.

**The chain-resolve arm now serves this container too (2026-09-01, same day).**
`resolve_chain_registered`'s refusal order is unchanged — fence, profile-hashes-to-id,
canonical-names-the-class, artifact-held, plan-compiles — and its fourth step gained the rule
that was implicit when one family existed: **the registered root decides the container.** The
root is a digest over one artifact's own bytes, so it matches at most one holding, and that
holding's lineage — dense or mmap — is the engine that serves the declaration; nothing infers a
family from the profile, because the profile is what is being judged. A mapping match constructs
`Qwen36Backend::from_registered_profile`: the plan compiles at construction (the admission
decision, refused with the node or geometry field named) and every forward walks the declaration
through the interpreter's untraced entry — held bit-identical to the traced walk, and the whole
backend held root-for-root and material-for-material against the compiled path, including the
seat's verb (`verify_material` on the other authority's material answers `Matches`). The v1
declaration is refused by the arm with its defect named; a dense declaration paired with a mapped
root reaches the mmap engine and is refused by its geometry gate. kaspad's `resolve_or_chain` and
the `--palw-chain-classes` fence needed no change — both were already family-agnostic, which is
the shape the SDK seam was for.

**The fuzz gate now covers this container too (2026-09-01, same day), and its design pass
convicted a panic before the first iteration ran.** `fuzz_qwen36_profiles_v1`
(`misaka/palw/base0/src/fuzz_qwen36.rs`, saturation runner `palw-qwen36-profile-fuzz`) is the
dense harness's clause for the mmap space: mutate the corrected profile — structural, nominal,
and ROLE edits, because this family declares its cache writes per node, plus REAL operand names
landed on wrong nodes, which stress the width checks harder than foreign names the planner
refuses on sight — then the chain's own admission gate, the plan, the court cost against the
carrier ceiling, and a caller's prompt twice under `catch_unwind`. Walking that mutation space on
paper before running it found the panic: a convolution declared in the ATTENTION table lands on a
window the artifact's layer kinds never pre-filled, and the walk's `remove(0)` ran BEFORE the tap
tensor read that would have refused — measured pre-fix as `removal index (is 0) should be < len
(is 0)`, from a plannable table any gate-accepted registration could carry. The fix is
declaration-faithful: the window (and the recurrence's states, same rule, defense in depth) start
as the zero state a fresh sequence has, made on first demand; the refusal that follows names the
tensor an attention layer does not carry, and the regression test asserts the walk now gets PAST
the site it used to die on. Measured: 400 iterations (the CI slice, seed `0x0067_2026_09_01`) —
136 gate-accepted, 59 executed, zero panics, zero non-determinism, zero closes over the ceiling,
worst close 38,020 bytes against 81,920, corpus digest pinned (`5b9be0a7…`) so the
cross-architecture clause is met by the suite; and 50,000 iterations at an independent seed
(`0x67`, 93 s) — 17,779 past the gate, 7,289 executed, and the same three zeros. The fence's
arming condition for this container is now measurement plus one operator decision, which is the
only thing left of the fourth clause.

**The fix cannot be an edit.** `shape_profile_id` is the borsh of the whole profile, node tables
included, and the hybrid's id is pinned in-tree as a live chain fact — "the shipped hybrid class id
moved — every registered QWEN36 claim is now unreachable". Correcting the naming produces a
different class, so it ships the way `Qwen/Qwen2.5-1.5B/graph-v2` did: a new row registered
alongside, with the genesis-registered class left exactly as the chain committed to it. The
interpreter follows that row rather than preceding it.

## What an adversarial audit found afterwards (2026-08-31)

Eight lenses over the implementation, each finding faced by two skeptics whose job was to refute
it: 28 raised, 10 survived, plus 4 from a completeness critic. What that bought, worst first:

* **A seat certified work it never priced.** The replay arm compared roots and nothing else, while
  a claim's `pwu` comes from the job shape its payload DECLARES and its `execution_root` rides
  that payload verbatim — so a producer could declare a hundred-thousand-token job, serve a
  one-token material whose roots were honestly that material's, and collect the quanta. Block
  weight in this lane would have been purchasable with recycled collateral instead of inference,
  which is the one property the lane exists to establish. Fixed: `PalwSeatDutyV2` carries the
  claim's `pwu`/`quanta` and the seat re-prices what it actually ran.
* **Every honest free-prompt claim would have defaulted its own producer.** The worker stamped
  `misaka-palw-rc` into its job context (the FLOOR's baked-in constant) while a seat derives that
  context under the node's own network name — two different context hashes, so no replay could
  ever match, and a quorum of `Unavailable` burns the producer's reserve for work performed
  correctly. Fixed: the network id is the operator's, required, with no default.
* **A gossiped empty prompt panicked the panel task**, taking down every duty that seat held.
* **Two gate-accepted profiles reached a panic or the wrong parameters** through the planner.
* **`--palw-chain-classes` armed the panel and not the producer.**
* Plus the reorg/status gates on the carriage read, and the honest corrections in this section.

**The pruned-sync gap is closed by the third door.** A node that joins by a pruned sync receives
the class table wholesale and no carriage rows, so it would refuse to serve classes the chain
registered — safe, and still a gap, because on such a fleet only nodes that watched a registration
go by could judge its class, and judging decides quorums. The two doors this ADR first named (the
pruning-point sidecar; a p2p request) are both protocol work. The third needs none:
`--palw-class-carriage <class-id>:<file>` adopts a declaration from anywhere, because a carriage
is **self-authenticating against chain state** — the chain must currently hold that class unfrozen,
the profile must hash to the class id (which is what a class id MEANS), and the canonical job must
name the same class. A supplier who satisfies all three has handed over exactly the bytes the
accept path would have written; anything else is refused, so a hostile source wastes only its own
bandwidth. The sidecar remained the better answer for an unattended fleet, and was still unbuilt when this audit section was written; it has since been built (see "The three remaining clauses" above).

## Security amendment (2026-09-02) — before the interpreter is armed

**SA-1 — Decision 5's fence is a security fence, and its arming conditions gain two** (ADR-0079
Decision 11, on its branch): the profile-space fuzzer's corpus includes profiles built to exhaust
memory and to recurse, and the interpreted path runs under the worker memory ceiling with the
ceiling proven to bind. A chain-registered profile is a stranger's program.

*As built (2026-09-03), with the honest scope of the claim.*
`misaka_palw_base0::engine_a16::PALW_INTERPRETER_TRACE_BYTES_CEILING_V1` bounds one token's
committed trace, priced from the declaration before the plan allocates, in BOTH containers — a
ceiling that were it in only one would be a property of which container an attacker picked. The
mechanism binds: a ceiling one byte under a profile's own cost refuses it by name.

The VALUE is a separate claim and the first one shipped did not survive it. 1 GiB was 60x the
largest class this build serves and ~40,000x the largest gate-accepted profile in the adversarial
corpus, i.e. a number nothing measured chose. It is now 64 MiB, derived from the measured cost of
every class this build ships (BASE-0 182 KB, QWEN25-A16 9.4 MB, QWEN36 17.6 MB, and QWEN36 stretched
to a 4,096-position context 49.0 MB), and
`the_interpreter_ceiling_is_derived_from_what_this_build_actually_serves` fails in both directions —
too tight to be a second line, or far enough above the measurement to be arbitrary again.

And what actually binds FIRST, because SA-1 should not be read as more than it is: on the shipped
admission gate the leaf bound and the per-node width checks refuse every oversized shape the
adversarial corpus can generate (372 of 400 refused at the gate; the largest gate-ACCEPTED profile
costs 26,624 bytes). The ceiling is the second line — the one that survives a gate whose node counts
or row widths are ever loosened. It is evaluated after the scalar geometry comparisons, which are
free, so a profile that is merely the wrong shape for this artifact is reported as the wrong shape
rather than as an oversized one.

*Corrected (2026-09-03): the ceiling is this NODE's capacity, and the derivation above is
calibration against what this build compiles — not a bound the chain's caps imply.* Classes are
permissionless (ADR-0054), and the consensus shape caps do not bound a declared row's width at all:
`validate_shape` asks for a non-zero width and a tile inside `[MIN, MAX]_TILE_LEN`.
`the_consensus_shape_caps_admit_more_than_this_build_will_materialise` constructs the witness — one
extra node of 20 M elements at the widest admitted tile, 8,576 worst-case leaves against a
4,194,304 cap, 80,006,272 bytes of committed trace — so a shape the caps admit is over a 64 MiB
ceiling by construction, and the gap is measured rather than assumed. Deriving the ceiling from the
caps instead would put it at `PALW_STEP_MAX_LEAVES × PALW_STEP_MAX_TILE_LEN × 4`, a terabyte: the
"number nothing chose" problem again. So the value stays a capacity choice, and what is fixed is the
sentence an operator reads — both containers' `from_registered_profile` now distinguish "this build
cannot serve the registered graph" (a kernel this software does not carry) from "this node's
interpreted-execution capacity refuses it" (the chain admitted the class, a node with a larger
ceiling serves it, and the divergence is node-local servability — who produces and who judges —
never block validity).

**SA-2 — Resolution is off the consensus thread and fails closed to "cannot serve".** Resolving a
chain class (fetching bytes, verifying the digest, compiling the profile) runs lazily, off the
block-processing path; a failure marks the class unservable on this node and never rejects a block.
Otherwise a hostile profile is a way to stall every validator's pipeline with one registration.

**SA-3 — Re-entry after eviction re-verifies from bytes**, never from a remembered verdict —
ADR-0079 Decision 9's rule ("no artifact identity from metadata") applied to profiles; the cache
bound is per class and in total, and "eviction must retract" (above) stands.

*As built (2026-09-03).* The first implementation made eviction the mechanism — and
`evict_held_artifacts_v1` / `evict_all_held_artifacts_v1` have no production caller, so in a running
node the retraction rule this amendment states did not exist: the `(class_id, artifact_root)`
negative mark was a process-lifetime verdict that could outlive the fact it described. The rule is
enforced at the READ instead, where no caller has to remember it: each mark records the holdings
identity it was derived from and a read whose holdings do not match drops the mark and re-derives.
One registry's holdings can no longer decide another's.

*Corrected (2026-09-03): that identity is the holdings, not their files.* The first form re-stat'ed
every configured path — `fs::metadata` plus `fs::canonicalize`, one pair per holding — and did it
with the process-wide mark mutex held, which is a blocking syscall under a global lock on a node
whose duties all queue behind it; this project has watched that shape wedge a public node for 46
minutes while systemd still called it active. It also retracted on any `len`/`mtime` movement, so an
rsync in place, a backup or a `touch` dropped every remembered refusal and re-paid the compile SA-2
exists to prevent — for no possible change of answer, because `load_class_holdings_v1` runs once per
service construction and the running service still dispatches against the mapping it loaded at
startup. The identity is now the loaded holdings themselves (lineage, configured path, the lineage's
summary — which carries the root it derived — and the address of the loaded object), which is
exactly what `resolve_chain_registered` answers from, and it is computed with no lock held and no
filesystem touched.

What that does not do, stated rather than implied: supplying an artifact to a RUNNING service does
not retract anything, because it does not reach that service's mapping either. It never usefully
did — the re-derived verdict was the same sentence. Supplying an artifact is a restart, or an
eviction (which drops the holdings and clears the marks in one move); it was never a `touch`.

**SA-4 — R-7 is a security item, not only a liveness one.** An unpaid seat has no income to lose;
its only stake is its bond, and ADR-0065 measured what a post-genesis bond costs (400,000 sompi).
Arming chain-registered classes for weight before seats are paid leaves the panel as the cheapest
thing on the chain to buy.

## Record (2026-09-05) — the fourth item

The Qwen3.6 geometry disposition this section left open was overtaken: the class was
re-registered as graph-v3 with a profile that describes the engine (id `5bd9ae3d…`), which is the
disposition the paragraph above said it could still adopt. Nothing remains to build under this
heading.

## Mission alignment amendment — 2026-10-07

classesはchain data、approved kernelsはbuildという境界を維持する。chain-resolve/fuzz成功やproducerとseatの実行一致とは別に、外部public bondの公開opening/terminal proof生成を受入条件へ追加する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Model acquisition and capital-allocation amendment — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。


## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0067-classes-are-chain-data-kernels-are-the-build.md -->

</details>

<a id="source-0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity"></a>

<details>
<summary>0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md — 原文 L1–L252</summary>

[原文を開く](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) · SHA-256: `c53ba0842d041a3b6455f35545be058ba63a2594a23033ed543e7c30c4bb3e77`

<!-- ADR_SOURCE_BEGIN:0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md -->
~~~~~~~~markdown
# ADR-0102 — The embedding lift is read per token, and a kernel a network has not armed is not in its identity

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


* Status: PROPOSED 2026-09-10 on `feat/adr-0099-sharded-seat`; **Decisions 1–4 IMPLEMENTED the same
  day, consensus-inert on every shipped preset** (§9): the graph, the kernel, its fence, the
  admission refusal, the registration root, the manifest variant and the measurement. The fence
  `Params::palw_token_lift` is `None` everywhere and the kernel is outside `court_catalog_root`, so
  testnet-11's and devnet's fingerprints do not move (`shipped_presets_have_pinned_fingerprints`).
  Decision 5 (the table row, the drill family, the arming) is stated, not built.
* Builds on: [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement, whose §1.3 refused this artifact by name),
  [0082](0082-the-close-is-flat-in-the-context.md) (graph-v5's fused attention, which graph-v6
  keeps), [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md) §7(b) (an inventory refuses a
  store it cannot serve rather than inventing one), [0042](0042-palw-mainnet-candidate-ruleset.md)
  Decision 11 and [0052](0052-palw-qwen36-hybrid-class.md) (`court_catalog_root` is part of every V2
  ruleset id), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) /
  [0075](0075-certification-is-a-consensus-object.md) (weight needs a certified family),
  [0067](0067-classes-are-chain-data-kernels-are-the-build.md) Decision 5 (the chain-registered arm),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the fused
  site's responder, which no family implements yet).
* Amends: ADR-0100 §1.3 (its last row now has an answer). Supersedes nothing.

## 0. The sentence this ADR is

**The hybrid engine lifts each token's embedding row by that token's calibrated triple, and every
hybrid graph declared the lift per lane — so no calibrated hybrid artifact had an inventory, a
measurement or a court. Graph-v6 declares what the engine does.** Its kernel is new, and a kernel
appended to the court's catalog moves every V2 network's identity the day the binary ships; so the
kernel lives in a fenced table outside that identity, and a network admits it by arming
`Params::palw_token_lift` — the fence, not the build, is what enters the identity, and only there.

## 1. What was measured

### 1.1 The refusal

`palw-class measure --network testnet-11 qwen35-2b.palwq36` (ADR-0100 §1.3, 2026-09-10): the
inventory built under the graph-v5 profile the manifest projects refused the artifact —
`embed_lift.a16: 248320 triples serve neither one lane nor 2048`. A measurement that cannot place a
row refuses rather than estimates; that was correct, and it was the whole answer.

### 1.2 Why: three readings of one store

| who | reads `embed_lift.a16` as |
|---|---|
| `qwen36-convert` | writes ONE calibrated triple per vocabulary row (248,320 for the 2B) |
| the engine (`qwen36.rs`) | a one-row store lifts every token by that row; a longer one is indexed by the token (`lift.get(token_id)`) |
| graph-v1 / v3 / v5 (`KDESC_A16_REQUANTIZE`, lane-sliced over `Hidden`) | one triple tiled across the lanes, or one per lane (2,048) |

The only store all three read alike is the one-row broadcast — which only fixtures carry. Every
converted hybrid artifact fell outside the graph: the inventory refused it (so no measurement and no
registered inventory root), and the court, had it been asked, would have read per-lane triples the
store does not have. The latent half: a store of exactly `hidden_dim` rows is one the lane-sliced
inventory accepts and the engine indexes by token — the two disagree on it. No converter writes one,
and graph-v6 removes the disagreement for the lift rather than guarding it.

### 1.3 The flag day this does not take

The first build put the new kernel in the adjudication table beside the others. The fingerprint pin
went red on two presets:

| preset | pinned | with the kernel in the catalog |
|---|---|---|
| testnet-11 | `060e3597…` | `1c42e99b…` |
| devnet | `b40976b2…` | `14a2ba69…` |

`court_catalog_root` hashes `catalogued_kernel_ids_v1()` into every V2 bundle, so a kernel nobody's
class reaches still changes what peers compare at the handshake — and the 2026-09-09/10 partitions
came from identity-moving changes that shipped unannounced. Decision 2 is the answer; with it the
pin is green and unchanged.

### 1.4 The held measurement under graph-v6, on the real artifact

`palw-class measure --network testnet-11 --name Qwen/Qwen3.5-2B qwen35-2b.palwq36` (this Mac):

| what | value |
|---|---|
| the file | 2,484,508,160 bytes, lineage `qwen36-mmap-v1`, 24 layers |
| bytes measured from the inventory | 2,484,279,940 (the family formula says 2,388,705,280 — a floor, as ADR-0099 said) |
| the inventory root (what a graph-v6 registration pins) | `189c78352da5…` |
| the root computed over the mapping (what the v1/v3 rows register) | `d6d79a213180…` |
| the class at 512 | `410462e8451a…` — **admitted** |
| at 32,768 | `0b76ac67f56d…` — refused by the ladder, the court window, the PublicDa payload |
| at 131,072 | `717e320458e6…` — refused by those and the state chunks |
| at 1,048,576 | no profile — the geometry ceiling (ADR-0097) |
| `verify --artifact` | every field recomputed equal; the deterministic half agrees; exit 0 |
| `preflight --model-id …/graph-v6` on testnet-11 (run while a graph-v6 table row existed; Decision 5 removed it) | **refused by name**: "the class reaches the per-token lift kernel and this network has not armed palw_token_lift" |
| the document | id `045a3322f0c6…`, unsigned — re-measured after Decision 1's amendment (the fused tile at the head moved every class id; the inventory root, the bytes and every wall did not); before it, the same artifact measured byte-identically twice across the §9 recovery |
| cost of one measurement | cold: 303 s wall (20.8 s user — a read-bound pass over the file), 1.68 GB resident; with the file in the page cache: 18.8 s, 5.82 GB resident (the mapping's pages count, and the inventory copies every row) |

A temporary graph-v6 row at the table's own geometry (`n_ctx` 8) paired to the same inventory root
as the measurement at 512: the root is a function of the weights and the node tables, not of the
context.

### 1.5 The court over graph-v6

On the hybrid fixture with a non-uniform per-token lift (`a_graph_v6_hybrid_adjudicates_a_per_token_lift_and_a_tampered_lift_convicts`):
the graph-v5 inventory refuses the store by name; the graph-v6 inventory commits one leaf per
vocabulary row (and tiles a one-row store across the vocabulary, the engine's other reading); every
leaf of an honest capture the one-step court can try clears (the fused attention sites are the
dissection's, ADR-0082); a lane corrupted at a lift leaf convicts at a prompt position and at a
decode call — whose token is a GENERATED one — and the prover's opening is exactly one 17-byte
triple.

## 2. The requirement

A converted hybrid model — the family the K3 stand-in is written in — has to be measurable,
registrable under a root a close can prove against, and adjudicable at its lift, without moving the
identity of any network that does not want it.

## 3. Decisions

**Decision 1 — Graph-v6 is graph-v5 with the lift read per token.** One node differs: the lift after
the gather names `KDESC_A16_REQUANTIZE_BY_TOKEN` (`a16/requantize/token-row/i128-mul-rshift-sat16/v1`):
the same `a16_requant` arithmetic, with the ONE triple at the position's token's row — the token read
from the carried ids exactly as the gather reads it (the prompt's at call 0, the generated one at
call c ≥ 1). The inventory commits the store one row per token (a calibrated store verbatim, a
one-row store tiled across the vocabulary); the plan compiles the node to the op the engine already
runs. A class is its graph: a new class id over the same artifact, and every earlier class stays the
chain fact it is. **Amended 2026-09-11 (ADR-0093 §9): graph-v6's fused output row is cut at the
head.** The fusion inherits the tile of the attention-table node it replaces, budgeted per
geometry — so a graph-v5 hybrid's fused leaf is dissectable only where that budget happens to fall
inside a head (it does at the shipped geometries: 8 lanes at the 2B and 35B-A3B, 4 at the 27B, over
256-lane heads; it does not at the fuzz fixture's tiny geometry, where the court refuses it as
`FusedTileStraddlesHeads`); graph-v6 sets that one node's tile to `attn_head_dim`, which moved its
class ids (none was registered anywhere) and makes every graph-v6 fused leaf dissectable at every
geometry. *Corrected 2026-09-11 (ADR-0093 §10): this paragraph first said the inherited tile was
"512 lanes on every hybrid geometry", generalizing from the fixture.*

**Decision 2 — A kernel a network has not armed is not in its identity.** The adjudicator gains a
second table, `KERNEL_CATALOG_FENCED_V1`: resolved like any other kernel, excluded from
`catalogued_kernel_ids_v1()` and therefore from `court_catalog_root`. The admission gate
(`verify_class_admission_v7`, and v6 is v7 with the fence down) refuses a class that reaches a fenced
kernel unless `Params::palw_token_lift` is active at the block — by name,
`TokenLiftNeedsItsFence`, before any coverage walk — and judges the rest of the graph against the
identity catalog as before. `validate_palw_v2` refuses a genesis that registers such a class unless
the fence is armed from genesis (genesis rows do not pass through the gate; the ADR-0082 fused-row
precedent). The fence is Some-only in the fingerprint, the schedule id and the fork id.

Why two builds agree: on a network where the fence is dormant no class can reach the kernel — this
build refuses the registration by name and a build without the kernel refuses it as a coverage gap —
and no existing class reaches it, so every verdict and every root is the same on both. Where a network
arms it, the fence's height is in its fingerprint and the fork-id gate keeps an older build off.

**Decision 3 — A graph-v6 registration pins the operand-inventory root.**
`qwen36_registers_inventory_root_v1` is the one spelling: a hybrid graph that reads the lift per
token registers the root a close's openings prove against; every earlier hybrid row keeps the root
computed over the mapping, as the chain registered them. The SDK derives the inventory root once per
graph per holding and memoizes it (the derivation copies every row — 2.5 GB for the 2B); the
chain-registered arm finds a holding by its computed root (possession, as the dense lineage's digest
is) or by that inventory root under the same graph.

**Decision 4 — The manifest names the graph.** `PalwModelFamilyV1::HybridQwen36TokenLift`
(`"hybrid-qwen36-token-lift"`) projects graph-v6; appended last, so every earlier document's Borsh
bytes — its id, its signature — are what they were, and a `hybrid-qwen36` manifest keeps naming the
graph-v5 class it always named. `palw-class measure` states it for every held hybrid artifact.

**Decision 5 — Not the canonical table yet, and why that is right.** No graph-v6 row joins
`qwen36_canonical_classes_v1`. Two invariants this tree pins refuse one, and both are correct: the
SDK conformance battery (every row's kernels in the identity catalog) and "every catalog class is
certifiable by a committed family without a code change" (ADR-0075's mainnet route). No committed
family covers a fenced kernel, and widening one would move `court_e2e_root` — the identity again, and
a certificate for an adjudication nobody performed (ADR-0069 Decision 5). The row, a drill family
that convicts through the per-token lift (the §1.5 test is its unit-level form), and the arming go
together, as one ruleset move for the network that chooses it.

**Decision 6 — `supports_court()` is not narrowed.** It answers two turns — disclosure and
arithmetic — and the producer reads it for the first (the DA court's responder), which every hybrid
row has. The arithmetic half over a calibrated artifact under a v1/v3 row is refused where it is
asked (`operand_openings_for` names the store), and a seat reads that as a sample it could not
clear, never as a conviction.

## 4. What this costs

One node table, one kernel arm, one fence at the `Params` sites every fence has, one admission
refusal, one inventory branch, one manifest variant. A graph-v6 root costs one pass that copies the
artifact's rows: fine for the 2B (a 2.5 GB copy), 33 GiB for the 35B-A3B — a streaming inventory is
the prerequisite for measuring and serving K3-class artifacts this way (§8).

## 5. Invariants the tests hold

1. The fenced kernel resolves, is disjoint from the identity catalog, and is not in `KDESC_ALL` —
   `a_fenced_kernel_resolves_and_stays_out_of_the_identity`.
2. Shipping it moves no fingerprint — `shipped_presets_have_pinned_fingerprints`, unchanged.
3. The fence is dormant on every preset, visible in the fingerprint when armed, `never()` is
   absence, and a genesis registering a graph-v6 row needs it from genesis —
   `the_token_lift_fence_is_dormant_visible_when_armed_and_its_kernel_is_outside_the_catalog_root`.
4. Dormant: graph-v6 refused by name and graph-v5 untouched; armed: both admitted at the 2B geometry
   at 512 under the RC-shaped court, the v6 entry recording the fenced kernel —
   `the_per_token_lift_is_admitted_by_its_fence_alone`.
5. The court: honest leaves clear, a tampered lift lane convicts at a prompt position and a decode
   call, one 17-byte opening; the v5 inventory refuses the store, the v6 one serves it and a
   singleton — `a_graph_v6_hybrid_adjudicates_a_per_token_lift_and_a_tampered_lift_convicts`.
6. The registration root: the inventory root under v6, the computed root under v2, resolved twice
   (the second from the memo), possession by either — `a_chain_registered_graph_v6_class_resolves_by_its_inventory_root`.
7. The manifest: v6 is its own class over one geometry and one artifact, JSON names it, Borsh tags
   0/1/2 — `a_token_lift_manifest_names_graph_v6_and_moves_no_earlier_document`.

## 6. Order of work

1. Decisions 1–4 — **done** (§9).
2. A drill family for the per-token lift (the ADR-0069 route): a fixture drilled to a conviction
   through the lift, its kernel set, and the question of how a network commits it without a re-mint
   (ADR-0075's chain certification is the candidate).
3. The graph-v6 table row and a devnet flag (`--palw-token-lift-devnet`), then a devnet drill:
   register the 2B under graph-v6, produce, sample, convict at a lift leaf.
4. A streaming inventory, so a K3-class artifact's root is a pass, not a copy.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0100 §1.3, the 2B artifact "refused by name" | measured under graph-v6 (§1.4) |
| graph-v5 as the hybrid measurement's graph | kept for `hybrid-qwen36` manifests; a held artifact measures under graph-v6 |
| "a kernel the court adjudicates is in `court_catalog_root`" (ADR-0052) | kept for the identity table; a fenced kernel is adjudicated and admitted by its fence |

## 8. What is deliberately not decided

* **Arming** on any network, and when.
* **How a network commits a drill family for the fenced kernel** without a re-mint.
* **The streaming inventory's shape.**

## 9. Number hygiene and implementation record

0102 is the next free number after ADR-0101 (whose §9 says so). Claimed on
`feat/adr-0099-sharded-seat` on 2026-09-10. **The next free number is 0103.**

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_step_refute.rs` — `KDESC_A16_REQUANTIZE_BY_TOKEN`, `Qwen36Op::RequantizeByToken`
    and its arm, `KERNEL_CATALOG_FENCED_V1`, `fenced_kernel_ids_v1`, the resolver over both tables.
  * `consensus/core/src/palw_qwen36_profile.rs` — `QWEN36_PRE_IR_V6`, `qwen36_profile_v6`,
    `qwen36_artifact_row_profile_v6`.
  * `consensus/core/src/config/params.rs`, `fork_id_v1.rs` — `Params::palw_token_lift` at every
    site, its genesis rule, its test; `palw_class_admission_v2.rs` — `TokenLiftNeedsItsFence`,
    `verify_class_admission_v7`, `PalwAdmissionShapeV1::token_lift`,
    `palw_genesis_reaches_fenced_kernel_v1`; `consensus/src/pipeline/virtual_processor/processor.rs` —
    the cached fence and the acceptance call through v7.
  * `consensus/core/src/palw_measured_model_v1.rs` — `HybridQwen36TokenLift`; the manifest test.
  * `misaka/palw/base0/src/inventory.rs` — the per-token branch, `qwen36_registers_inventory_root_v1`;
    `qwen36_plan.rs` — the lift accepts the kernel; `qwen36_backend.rs` — the court test.
  * `misaka-palw-sdk` — the memoized root (`registered_root_of`), the chain-registered lookup by
    either root, the preflight through v7, the test; `palw-class measure` under graph-v6.
  * The working tree was lost to a scratchpad wipe at the session's restart (2026-09-11) before it
    was committed, and rebuilt by replaying this session's recorded edits onto `a9c9abe4`; the
    replay is what §5's tests and §1.4's measurement ran against.
~~~~~~~~
<!-- ADR_SOURCE_END:0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md -->

</details>

<a id="source-0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm"></a>

<details>
<summary>0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md — 原文 L1–L204</summary>

[原文を開く](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) · SHA-256: `ec7046522068d48b93281590bb8e367e1a40ef02df040494715cebc9e219f35c`

<!-- ADR_SOURCE_BEGIN:0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md -->
~~~~~~~~markdown
# ADR-0172 — Model extensibility uses versioned kernels, not a universal VM

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** Design direction selected by the operator, 2026-10-06. Documentation decision only;
implementation, cryptographic review and network activation are pending. No fence height assigned.

**Changes:** [RFC04](../rfc/0004-palw-model-improvement.md)'s future automation roadmap,
[RFC05](../rfc/0005-palw-ml-vm.md)'s BVM/GVM implementation direction, and
[RFC11](../rfc/0011-permissionless-model-and-long-context-onboarding.md)'s VM fallback for registration.
**Preserves:** [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md) and
[RFC07](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md): probabilistic constraint
verification, positive receipts, bounded exact court on dispute and explicit residual error.

## 日本語での決定

モデル対応のための **PALW-BVM・PALW-GVM・Universal VMは実装しない**。ISA、任意の分岐・ループ、
syscall、guestメモリ、gas、one-step VM court、toolchainを合意規則として維持する実装・監査・履歴再生の
コストを避ける。既存コードや実験用ブランチをこの文書で削除・無効化したとは扱わない。

代わりに、**少数の汎用的なversioned Kernel + 宣言的VerificationPlan + 合意更新によるKernel追加**を採用する。
既存演算の新しい組合せならfrontendとplanで対応し、本当に新しい演算・メモリ・検証原理だけをKernel更新にする。
モデル名ごとにQwen用・Kimi用Kernelを増やさない。未知演算を既存Kernelで表現できなければ、対応Kernelが
有効になるまで登録は明示的に待つ。CPUを増やすことや未検証コードのuploadでこの条件を迂回しない。

通常検証の方針は変更しない。**不正があれば極めて高い確率で検出されるよう計算をconstraintとして符号化・
集約し、小さい検査と必要なreceipt・DA・challenge windowを満たしてFinalする。** Panelが毎回全推論や
選ばれたsegment全部を再実行する設計へ戻さない。例外時だけ、拘束された証拠から有界のterminal courtへ落とす。
単なるtraceの抜取りやデータのerasure codingだけでは、単一点不正に対するこの保証は成立しない。

**追加の明示的禁止:** 参照論文にあるTEE/enclaveを正当性の信頼根拠にする方式、モデル検証・fraud-proof VM、
spML型BFT運営者委員会・trusted PKI・committee beaconは、この経路では採用しない。将来の代替依存としても
残さない。公開のidentity/bond規則、別途安全性を定義するpost-commit乱数、承認済みKernel検査器と有界PALW courtを使う。
論文から利用する検査・誘因・局所化の考え方と、利用しない信頼アーキテクチャを区別する。

## 1. Why remove the VM implementation programme

An existing production ISA still needs MISAKA-specific deterministic arithmetic, syscall bindings,
state commitments, metering, proof localization, replay and adversarial tests. Borrowing a VM does
not remove those costs. No measured implementation-cost saving is asserted here. Kernel extensions also require reference implementations,
independent review, court/resource tests and safe upgrades; their costs are not zero.

The deliberate trade-off is a narrower accepted language today in exchange for a smaller consensus
surface. A novel operation may wait for a protocol release. There is no promise that every future
finite program can be admitted under today's kernels or that upgrades are needed only once in a
particular number of years. Such cadence and coverage need evidence.

## 2. Selected architecture

| Conceptual layer (not allocated wire ids) | Responsibility |
| --- | --- |
| K0 — base consensus | Canonical commitments, activation/fingerprint rules, bounded extension envelope, state transitions, accounting and resource limits. No uploaded verifier execution. |
| K1 — existing tensor semantics | Versioned deterministic tensor operators, bounded recurrence/state, canonical input/output, material openings and exact terminal courts. Existing classes keep their existing identities/rules. |
| K2 — probabilistic constraint verification | Approved matrix, range/lookup, nonlinear, routing, authenticated-memory and boundary relations; Freivalds/GKR or another explicitly reviewed suite; soundness composition and bounded localization. This is a proposed extension, not a currently active kernel. |
| Later kernels | Reusable new semantic/verification families that cannot safely be expressed by the active set. Old semantics are not silently reinterpreted. |

`VerificationPlan` is **data in a bounded, typed grammar**, not native code, WASM, Python, an ISA
image or a programmable verifier VM. It names implemented primitive/constraint/checker ids and
dimensions, graph/state connections, commitments and budgets. The checker validates the plan's
coverage and composition; it never trusts a registrant-supplied assertion that a custom relation is
sound. An arbitrary bytecode interpreter disguised as `CustomOp` or a universal CPU circuit is not
an approved fallback. Abstract GKR circuit generality does not authorize any uploaded computation:
the accepted circuit construction must be tied to the kernel's defined tensor/state relations.

Producers may use any off-chain accelerator or orchestration tool that yields the same committed
semantics. Existing EVM contracts and the native EVM lane are not removed. Neither EVM nor a new
EVM job precompile may be used to smuggle a general model-verification VM back into this roadmap.

## 3. “SegWit-class” means a versioned protocol extension, not automatic soft-fork compatibility

The relevant precedents are versioned validation rules and committed witness data in
[BIP141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki), and explicit future extension
points in [BIP342](https://github.com/bitcoin/bips/blob/master/bip-0342.mediawiki). They do not prove
that adding a MISAKA kernel is a soft fork. In particular, an old node skipping unknown work checks
cannot safely price, reward or order that work.

Default to a coordinated, fingerprinted protocol upgrade. A future proposal may call itself a soft
fork only after proving valid-history inclusion, state/reward/fork-choice compatibility and old/new
node behavior for that concrete change. Never adopt unknown-version/unknown-op success for rewarded
PALW claims. Unsupported versions are explicitly unsupported; an old node must stop participating
in post-activation validation/production until upgraded, not continue “validating” by ignoring them.

The extension sequence is proposal → reference + independent implementation → vectors/resource and
soundness review → shadow validation → coordinated LOCKED_IN schedule → ACTIVE. These are release
states, not a newly invented validator ballot or a current implementation. Readiness signalling alone
is not validation, and there is no selected activation DAA in this ADR.

## 4. Immutable meanings and registration

New-format class identity binds a kernel descriptor/version, semantic/constraint/court suite ids,
the immutable challenge policy and approved checker/challenge/soundness tuple, plan root and all existing
program/material/task/context commitments. Actual future beacon/transcript is later evidence, never an identity
input. A hash of code is not its
correctness proof and never authorizes the node to download/execute a module. Legacy class ids and
in-flight claim semantics stay unchanged; an existing class is not rehashed or upgraded in place.

For a new model:

1. Resolve source, rights, task/context and deterministic lowering independently of deployment.
2. If the active kernel expresses the **whole** operation, submit a bounded plan with coverage,
   integer/field rules, DA and worst-dispute envelopes. Registration needs no whole inference replay
   by the registrar/node. Source fidelity and operator readiness remain distinct checks.
3. If only a frontend/format adapter is missing, implement that off-chain adapter and conformance
   vectors. It changes no consensus semantics.
4. If a semantic, verifier, memory or court capability is absent, return `KERNEL_EXTENSION_REQUIRED`
   with the exact relation and limits. Propose a reusable extension, implement and activate it, then
   resubmit. No VM fallback and no pretend successful registration during the wait.

No user may select a weaker confidence level, omit a constraint or raise a node budget through plan
metadata. The full-check error target remains RFC11's **conditional proposed** `ε_check ≤ 2^-128`,
not a demonstrated network security level. Sparse errors must affect a covered algebraic/encoded
relation. Bind evidence before challenges; include multi-round adaptivity, grinding, DA and Panel
compromise in their separate analyses. Exact terminal court remains a backstop for **detected** or
challenged faults, not a mechanism that retroactively removes undetected-error probability.

## 5. Supersession and implementation boundaries

| Document / component | Decision |
| --- | --- |
| RFC05 old Parts I/II, ladder TIR → BVM/GVM, Linux/ISA fallback and EXEC VM programme | Withdrawn; RFC05 retains only the reason. The [Kernel design](../design/palw/models/versioned-kernels.md) specifies the selected extension route. |
| RFC04 improvement, candidate/evaluation/promotion rules | Keep. Off-chain automation is unrestricted. Code-by-tests/EXEC is not promised via a future VM; a distinct reviewed non-VM checker extension would need its own specification. |
| RFC11 model registration and broad coverage | Replace all proposed VM fallback with active-kernel plan composition or an explicitly required future kernel. Preserve the real 9B/2M blockers and the all-HF denominator. |
| ADR0171 / RFC07 probabilistic verification | Unchanged security direction; reusable kernel-primitive templates replace references to VM templates. Full replay remains a conformance baseline or exact small-class option, not the required ordinary large-model path. |
| RFC02's residual-GVM route and RFC03's VM Tests reference | Withdrawn; RFC02 §§II.11–II.12 now specify active-kernel registration and an explicit extension queue. RFC03's future test relation needs a bounded non-VM kernel/task profile. Missing support stays uncovered until the full route passes. |
| Existing live EVM and any historical VM-coded objects | No change by documentation. Inventory actual activation/code before cleanup; retain required historical replay. Dormant prototypes need no new implementation or activation under this roadmap. |

The [RFC01–12 consistency audit](../evidence/rfc/0012-kernel-only-design-audit.md) records the
2026-10-06 cross-document dispositions. RFC01/03/06 distinguish legacy replay from the new checker;
RFC07/08/09/10 bind kernel/checker identities without reviving an interpreter. RFC12 removes DNS
authority, not the existing native EVM lane. RFC11 §15.9 specifies the research/benchmark basis
for probabilistic checks and the boundary between economic audits and cryptographic soundness.

## 6. Acceptance evidence and honest coverage

* Independent reference/optimized executor agreement, plan coverage checks and falsified-constraint
  tests across dense, MoE, recurrent, lookup, quantization and authenticated-state cases.
* A valid large claim reaches Final with the proposed small-check suite and no routine segment
  replay; a single bad scalar/routing choice/memory write is covered by the **reviewed** error bound.
  Missing evidence, weak suite ids, forged circuit bindings and absent families cannot finalize.
* A failure localizes within the specified worst-case court bytes/work/time. Compiler and verifier
  bugs and Panel collusion are not excused by probabilistic soundness of an ideal relation.
* Mixed old/new kernel classes, long-running claims across activation, reorg/IBD/pruning and unsupported
  peers have tested behavior. Cross-kernel pipelines require an explicit bounded composition profile.
* Report actual 9B-8k, validated long-context and full-modality large-model registration/Final results,
  producer/proof/verifier/DA/court cost separately. No benchmark or registration is performed by this ADR.
* Preserve RFC11's pinned all-HF `D_all` denominator, rights ceiling and one-sided confidence requirement.
  The attachment's 90–95% / 95–99% projections are **not measured coverage** and are not adopted as a
  guarantee. Without a VM, residual models may require an upgrade; count them as failures until their
  complete route is implemented, active and supported by the required registration evidence.

**Outcome:** no VM implementation programme for model extensibility; permissionless plans within
implemented kernels, explicit protocol upgrades outside them, and unchanged probabilistic-normal /
exact-dispute verification. This decision neither implements nor activates any of those extensions.

## 7. Beacon-backed Kernel/model conformance, after completeness (2026-10-08)

`KernelDescriptorV1.challenge_policy_id` resolves [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
PostCommitChallengePolicyV1 digest. RFC07 alone specifies sources, seed/sampling, transcript transforms and recovery;
other workstreams must not implement independent formulas. Approve checker/challenge/soundness policies together.

Before Kernel differential challenges, establish semantics completeness, full constraint coverage, exact court,
public-material/prosecution completeness and bounded resources. Then fix reference/checker/court/independent/
optimized implementation revisions and test scope, commit, wait for independently valid future PALW work, and run
canonical randomized vectors alongside required deterministic/adversarial tests. Agreement reduces fixture bias;
it proves neither Kernel semantics nor soundness. New unknown operations still require coordinated implementation,
review/shadow evidence and explicit Kernel activation. A beacon or Panel vote cannot authorize them.

Model onboarding follows RFC11 §17: Static Admission / RegisteredDormant, Beacon Conformance, then G14/public
availability/resource/actual chain Active Eligibility. RFC13 carries commitments, source/lock/transcript/results,
scoped error and resumable state. Changing implementations/artifact/layout/plan invalidates dependent evidence and
requires a fresh pre-beacon commitment with counted retries; actual beacon/query data does not rehash the class.

Use RFC07's candidate-independent qualifying Final useful work. Committee/DNS/BFT signatures, heartbeat/BASE-0,
round/EXEC headers and self-testing candidates supply no substitute entropy. Source absence waits without changing
main liveness. Bootstrap, source cost/unpredictability, last-contributor/withholding bias, staged GKR/Fiat–Shamir
security, retries/reorg and fresh-public-verifier exact escalation remain unpassed gates. This documentation adds
no runtime, source approval privilege, wire id, fingerprint or activation height.

## Mission alignment amendment — 2026-10-07

kernel-extension acceptanceとregistrationにpublic-verifier dispute completenessを追加する。approved bounded kernel courtが存在するだけでnew profileをmineableにしない。VM/TEE/BFT authorityなしの選択は維持し、missing public localization/openingをmissing kernel/profile capabilityとして閉じる。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## 追記(2026-10-16 ユーザー決定):RFC-0005(PALW ML-VM)を採用しない理由

**MISAKA は汎用 VM を採用しない。**
- VM は、実行の正しさと安全性を保証するためのコストが大きい。任意の program を合意の対象にすると、その実行を検証・裁定するための仕様・実装・資源の上限・court の範囲が大きくなりすぎる。
- そのため、誰でも参加できる(パーミッションレスな)検証には向かない。正直な外部の検証者 1 人が有罪まで進める G14 の要件を、汎用 VM の全範囲で満たすことは現実的でない。
- モデルの拡張は、本 ADR のとおり、版付きの kernel と PALW-TIR の固定 primitive 集合で行う。
- [RFC-0005](../rfc/0005-palw-ml-vm.md) は採用しない設計の記録として残す。
~~~~~~~~
<!-- ADR_SOURCE_END:0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md -->

</details>

