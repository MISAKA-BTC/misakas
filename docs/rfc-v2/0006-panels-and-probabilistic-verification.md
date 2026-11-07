# RFC-v2-0006 — Panel参加・抽選・確率的検証・認証Receipt

編集日: 2026-10-29 / 入力: `misakas-pre(5).zip`

[15本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

**原文収録元:** [旧0006](../rfc/0006-palw-layer-sharded-panels.md)、[旧0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)、[旧0010](../rfc/0010-permissionless-palw-panel-and-claim-completion.md)。

**関連RFC:** [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [13 公開Bootstrap・Genesis・有効化とRelease接続](0013-permissionless-bootstrap-and-release.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 通常時の検証を一本化する

旧0006のlayer／cell分割、旧0007の確率的検証・post-commit challenge、旧0010のpermissionless Panel bindingを統合する。通常検査と認証receiptを担当し、不一致後の訴追手続きは14に渡す。

2026-11-07 の [公開裁定の方針 v0.1](../design/palw/consensus/public-adjudication-policy-v0.1.md) は、新しい Batch 公開時計の claim 裁定について、下の原文に優先する。原文は履歴として残す。古い claim は古い規則のままである。原文が Panel の定足数で `ReceiptLicensed` へ進めると書く箇所は、この設計の評決ではない。Panel の署名は署名者を識別する。合意ノードはモデル計算を端から端まで再実行しない。検証は、署名、ハッシュ、Merkle opening、および元の claim に束縛した局所化した一つの primitive の決定論的再計算である。計算の暗号学的証明は、将来の作業としても使わない。

## 2. 公開参加と抽選

適格なbond、認証済みsnapshot、commit後の乱数、draw policy、担当scope、署名者を拘束する。固定operator名簿やgenesis身分を隠れた参加条件にしない。抽選結果、応答義務、交代条件、受理点は再実行で復元できなければならない。

## 3. 検査の対象と限界

cell、boundary、部分再実行、直接の代数的spot checkを、検査した関係に限定して扱う。receiptの署名は検査者と対象の認証であって、全モデル計算の暗号学的証明ではない。対象外演算を検査済みにしない。

旧文書にある計算validity proof等の将来案は15の禁止台帳に従う。検出率や期待利益はPRINCIPLESの後続改定に従って評価し、撤回された古いgateを復活させない。

## 4. 結果と次の処理

有効な認証receiptは10のGate C判定へ接続する。不一致は14へ提出する。Unavailable、無回答、通信・収録の問題は計算不正と同義ではない。帰責可能な義務違反かどうかは14の契約で判定する。

Panel=1の運用条件とPanel=0の休眠案は分離する。後者の未有効化は15にまとめ、quorumをゼロにして認証を省略する入口を設けない。

## 5. 受入確認

抽選の再現、commit前後の境界、別claim／別slot署名、無回答・交代、false clean、悪意あるPanel、正直な非Panelの告発、再起動・reorgを確認する。検査費と担当範囲、保持期間、報酬条件は11／14と照合する。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の15本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0006 | 1–1328 | [RFC-0006: PALW layer-sharded panels — bounded cell verification from authenticated boundaries, ](#source-v1-0006-L00001) |
| 旧0007 | 1–507 | [RFC-0007: PALW probabilistic detection — direct checks, authenticated receipts, and determinist](#source-v1-0007-L00001) |
| 旧0007 | 533–1497 | [II.9 Where it pays (measured)](#source-v1-0007-L00533) |
| 旧0010 | 1–254 | [RFC-0010: Permissionless PALW Panel binding and claim completion](#source-v1-0010-L00001) |

---

<a id="source-v1-0006-L00001"></a>

### 旧RFC-0006 / 原文1–1328行

出典: [0006-palw-layer-sharded-panels.md](../rfc/0006-palw-layer-sharded-panels.md)。配置理由: 主題別統合。

### RFC-0006: PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with independent public-verifier localization and exact court on dispute

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **Status update 2026-10-08 (supersedes every "dormant" / "None on every preset" wording below that predates it):** `palw_tir_shard_v1` is **ARMED on testnet-12 at the int-11/12 flag day, DAA 5,300** (`PALW_T12_INT11_FENCES_V1`); it stays dormant on every other preset. Fold, cells, receipts by parts, the exact court (`TirShardCourtAccused`, any Active bond) and the kaspad e2e are IMPLEMENTED_AND_TESTED; per-segment resource pricing, a non-seat public cell watcher, a processor-level reorg/duplicate-receipt test, the V3 per-shard draw and the D-S1…D-S6 drill evidence are open (`docs/design/palw/remaining-rfc-integration-matrix.md`). Any change to an armed rule needs a new versioned fence.
>

> **Implementation update 2026-10-11:** The non-seat watcher, dormant segment-resident pricing and per-shard V3 engine are implemented; their implementation is not activation approval. V3 restore now derives its seal/entropy commitments and retains the verified output after shared-epoch pruning under explicit engine carriage version 2. Live shard v1 at DAA 5,300 is preserved. See [SHARD §1.4](../design/palw/verification/shard-rfc6-10.md#14-restore-time-seal-and-entropy-provenance-2026-10-11) and the [four-RFC audit](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md). D-S drill, public source/soundness, economic and worst-resource approvals remain open.
>

> **Spec closures 2026-10-15:** normative rules and named parameters for R6-CELL/DRAW/OUTSIDE/PRICE/MIGRATION and ALL-MODEL are in [Spec closures — 2026-10-15](../rfc/0006-palw-layer-sharded-panels.md#spec-closure-2026-10-15). No PASS, activation or change to live v1 is implied.
>

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Revised Draft, 2026-10-10後続改定 — off-chain model acquisition; model-bond coinbase allocation and conditional prosecution; historical implementation/measurements retain scope; new economics/implementation/activation pending |
| Author(s) | MISAKA core (drafted with Claude, lane M3) |
| Created | 2026-10-01 |
| Normative dependencies | RFC-0002 (PALW-TIR v1: commit points, cones, the step space of an IR class, the IR one-move court) |
| Affects | spec/palw 04b (§10: a cell), 07 (licence by parts, Final), 08 (draw, receipts, quorum, recount, outsider), 10 (locks and pay per cell), 16 (fences) · IR classes only, all networks (dormant until armed) · `consensus/core` (`palw_shard_panel_v1`, `palw_panel_v2`, `palw_state_v2`, `palw_verification_v2`, `palw_receipt`, `palw_tir_class_v1`), kaspad's panel worker, `misaka-palw-tir-exec` (a cell verifier) |
| Branch | `rfc6/gpu-shard` (text; an off-chain prototype in `experiments/palw-tir-gpu/tests/layer_shard.rs`) |
| Related | ADR-0098 (coverage is a number), **ADR-0099 / ADR-0100** (a seat holds a shard: the plan, the stratified draw, `ShardCourtAccused`, `ShardReceiptLicensed` — built for legacy classes, dormant), ADR-0103 (held context; D2 interval, D7 a seat holds a shard of the model and its state), ADR-0111 (leaf demand), ADR-0062 (DA court), ADR-0117 (a draw is one forward), ADR-0133 (verification is its own clock; segment-scoped receipts), **ADR-0147** (independence is drawn: the outsider seat), ADR-0152 (Q-1…Q-7, `basis_k`), ADR-0160 (claim capacity), RFC-0002 Phase F (`TirShardCourtAccused`, `TirStepLeaf`/`TirStepNode`), **RFC-0007** (lane M4: batched verification certificates and algebraic checks — receipt aggregation and cheaper per-shard checks are its subject, not this RFC's), `docs/design/palw/tir/gpu-integer-backend.md` |

#### Current verification boundary — 2026-10-06

The cell partition, authenticated boundaries and exact localization baseline below remain useful.
For new probabilistic profiles, [RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)
and [RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md) replace routine whole-cell
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

#### 概要(日本語)

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

#### Summary

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

#### Motivation

##### 1. Verification supply is the binding limit of claim throughput

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

##### 2. What already exists

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

#### Goals and non-goals

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

#### 1. Cells

##### 1.1 The layer plan (derived, as ADR-0099 Decision 2)

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

##### 1.2 Position segments

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

##### 1.3 What a cell reads, and what it checks

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

##### 1.4 Two ways to check a cell, one receipt

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

#### 2. Why a cell's check is enough

##### 2.1 The detection lemma

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

##### 2.2 What a cell check does not see

A seat attests its cell, not the claim. A lie in a cell that no honest seat checks is not seen by the
panel. It stays convictable by anyone who later checks that cell within the court window, but a
corrupt quorum of that cell could license it first (§5).

#### 3. Availability of boundary rows

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
   DA-1..9). *(2026-10-15: demands are limited to claim-specific step material; demands for model weight bytes are
   prohibited for new rulesets — spec closures, ALL-MODEL item 3.)*

A cell's inputs are *runs*: one carry-out run per position, and history rows in whole tiles. So this RFC
adds one unit, **`TirStepRun { first, count }`**: a contiguous run of step leaves with its two boundary
paths, bounded by the carrier (100,000 bytes). One demand then serves about `⌊100,000 / (lanes × 4)⌋` positions of a segment's carry-ins, less the
paths (≈ 15 positions of a 1,536-wide residual per unit, against one per `TirStepLeaf`).

The ADR-0152 session limits (a seat: 1 open, 4 lifetime) stand. A demand names a run, not a cell, so a
seat whose producer withholds can force a default within its limits.

A seat that cannot obtain its inputs says `Unavailable { chunk, daa }`, as today: it abstains and is not
liable (PALW-VF-17). `seat_count − quorum + 1` such receipts redraw (SR-9).

##### 3.1 Off-chain model supply, identity and claim-specific evidence — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
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

#### 4. The panel

##### 4.1 The stratified draw, per shard (ADR-0100 Decision 4)

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

The recommended first step (open question 5) is `S_P = 1` *(historical; the Decision section governs v1)*. Each seat then attests its whole shard, and
a shard's two attesters are its first two seats. Segments switch on per class later.

##### 4.2 An outsider per shard (ADR-0147, generalised)

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
cannot license and the claim voids at its receipt deadline. *(2026-10-15: this is a non-fraud termination; it never
slashes, penalizes the registrant or suspends the class — spec closures, R6-OUTSIDE item 3, under ADR-0177 D1.)* This is ADR-0147's rule: *the cost of a
model the network does not run lands on the claims of that model*. What changes is that "the network
does not run" now means "no outsider can hold one shard", not "no outsider can hold the model".

This removes refusal 2: a stratified panel now seats an outsider in every part.

##### 4.3 Receipts with cell masks

A receipt for an IR-sharded claim is the V3 receipt with its mask widened from segments to cells:
**`ReceiptV4 { …V3, shard: u16, segment_mask: u64 }`**. The signature covers both
(`palw_receipt_message_v4`, a new context in the next signature-context set). Verdicts are Q-1's:

- `Valid` attests every leaf of every cell in the mask (§1.4);
- `Unavailable { chunk, daa }` names a missing input leaf;
- `Incapable` is unchanged;
- `Sampled` never counts.

##### 4.4 The recount over cells (Q-3, generalised)

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

##### 4.5 Licence by parts, and Final

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

#### 5. Coverage and collusion

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

#### 6. Economics

##### 6.1 Seat duty and locks, per cell

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

##### 6.2 The claim's liability and the class room

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

##### 6.3 Seat pay

The panel pool (the model-class share, `clamp(α·C_V/(C_P+α·C_V), 100 ‰, 300 ‰)`) is split by counted
receipts weighted by `Σ w_cell / w` of their masks, not `⌊pool / K⌋` per seat. A shard's outsider is
paid for its whole shard. That pays for the fetch the outsider had to make.

#### 7. Held context (ADR-0103) and position segments

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

#### 8. Cost model and what it buys

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

#### 9. The prototype (D4)

`experiments/palw-tir-gpu/tests/layer_shard.rs` is an off-chain test; it changes no consensus code. It runs
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

##### 9.1 Results (2026-10-01)

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

#### 10. Activation

*(Historical plan: `palw_tir_shard_v1` is armed on testnet-12 at DAA 5,300; later V3/segment-v2 profiles have their own fences.)*

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

#### Proposed Spec text (sketch)

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

#### Alternatives

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

#### Security and economic analysis

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

#### Compatibility and migration

- Dormant: no fingerprint, object, receipt or rule changes until the fence is armed (§10). *(Historical: v1 is armed on
  testnet-12 at DAA 5,300; see the 2026-10-08 status note and spec closures, R6-MIGRATION.)*
- Legacy classes are untouched, and so are ADR-0099/0100's fences and refusals.
- A flat IR class (no plan) is exactly today's: `S_L = 1`, the recount over cells is the recount over
  segments.
- A claim bound before a class's plan keeps the whole licence it was drawn for (ADR-0100,
  `palw_claim_licenses_by_parts_v1`).
- Node side: the panel worker gains a cell verifier (the prototype's `layer_shard` check over
  `misaka-palw-tir-exec`; batched on a device by `misaka-palw-tir-gpu`), the readiness proof over a
  shard's rows, and the ledger's need for a shard (`PalwRoleMemoryNeedV1` over the shard's bytes).

#### Historical open questions — initial baseline

The following list records the initial alternatives. The v1 implementation's constants record
adoption of their recommended defaults on 2026-10-03 (`palw_tir_shard_v1.rs`), and the implemented
baseline is described under **Decision** below. These are not unresolved choices for the live v1
rules. New probabilistic coverage, V3 strata and segment-resident pricing have their own later
review / activation requirements; this historical list does not authorize their activation.

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

#### Decision

The implemented v1 baseline adopts the nine recommended defaults above: one original shard fence;
three class seats with two required attesters per cell and an outsider over the whole shard;
`TirStepRun` DA; layers-only as the default with `S_P = 2` offered for the S1 plan; work-share lock / pay
with the existing floor; the binding shard's effective readiness for class room; the network-seat-budget
minimum shard count with at most twice the minimum; and the original measured-speed constant.
`palw_tir_shard_plan_shape_v1` enforces the offered position counts, 2..64 shards, the layer-count bound
and the derived under/over-sharding limits. These choices describe the existing implementation.

The v1 fence is live on testnet-12 at DAA 5,300. The later [RFC6/10 implementation record](../design/palw/verification/shard-rfc6-10.md)
adds V3 per-shard draws and non-seat watchers behind the permissionless-panel fence, and resident-aware
pricing behind the separate dormant `palw_tir_shard_segment_v2` fence. Live v1 work pricing is preserved.
The [four-RFC design audit](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md) tracks the remaining
coverage / public-court / worst-cell capacity / fee evidence. Model acquisition follows ADR-0177 and
the later §3.1 amendment; historical readiness is not approval of a new acquisition-based earning gate.

#### V3 shard assignment: retained derivation and restore contract — 2026-10-11

The later permissionless V3 shard draw has a pre-entropy frozen population and subject; this
contract does not amend live lane-A assignment at DAA 5,300. V3 assignment randomness remains
separate from RFC07 Part VI's computation challenge.

- The seal id must derive from network/ruleset, accepted immutable work subject and exposure,
  acceptance order, parent checkpoint, first future beacon epoch, frozen population root and
  strata. Lookup claim id and signature are not entropy. A matching copied id in a binding
  cannot substitute for this derivation.
- At readiness the engine copies the already verified epoch output into the claim's retained
  derivation material. Initial and retry bindings use that same output and subject. Terminal
  release/void may prune the shared epoch row, but must preserve the claim's output and binding
  history while that claim remains tracked. Live output must agree with the shared epoch row;
  missing material is refused, never filled with local randomness or a guessed output.
- Every restored `EntropyReady` and binding must rederive the seed/beacon id and exact policy
  readiness/assignment clocks. Frozen role/membership and per-stratum operator/retry exclusions
  remain mandatory. Whole carriage and keyed journal restoration enforce these checks. Import
  also requires its trusted consensus root; retained output alone is not a beacon certificate or
  authentication of an arbitrary self-consistent checkpoint. Historical collateral headroom and
  exact ranking require the authenticated transition history, not this material alone.
- The implementation uses explicit engine carriage/cursor version **2**, committed by the
  Some-only permissionless fence. Obsolete version-1 experimental states are refused; their
  reconstruction requires authoritative replay. This adds no new lifecycle object tag or delta
  number and arms no fence. Absent/never fences preserve public identities.
- The serialized import envelope must include population/stratum bits, every history binding and
  used operator, the current phase's binding copy, derived reservations and retained entropy.
  It must permit every state under the validated structural caps. The derived current bound is
  recorded in SHARD §1.4; a legal 4,096-seat flat checkpoint previously exceeded the old envelope.
  Serialized-size coverage is not a RAM/CPU/latency/service-rate or participation approval.

These rules close restore-time derivation omissions. They do not approve source independence,
grinding resistance, probabilistic coverage, public-court costs, fee/capital floors or network
migration. V3 is dormant on all public presets and real activation remains refused.

#### Shared verification-challenge boundary — 2026-10-08

New algebraic checks of cells/layers and their boundary relations bind [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
immutable challenge policy and complete statement before its future source window. Panel/role assignment stays a
separate versioned randomness contract; assigning a cell is not evidence of an unpredictable computation challenge.
No local seed formula or initially exposed seed for later adaptive proof messages is permitted. Partial cells still
need scope/boundary coverage and whole-claim error composition; repeated same-seed watchers do not add independent
trials. A fresh outsider reconstructs public queries/transcripts and follows mismatch through RFC14's bounded exact
court/DA rules. Source scarcity leaves checks pending under their deadlines without changing chain liveness.
This amendment adds no live policy, quorum change or implementation result.

#### Mission alignment amendment — 2026-10-07

§2のfirst-divergence lemmaは、必要な認証境界と全対象commit pointを取得・検査した場合の局所化の補題である。honest seatが存在するだけの検出保証でも、公開証拠の取得やcourt proof生成の完成証明でもない。cell/shardのopening・重み・stateと有界dissection/closeを、選出されていない普通のpublic bondにも提供する。outsiderの選出やcell quorumを、そのbondの訴追権の条件にしない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

shard・seat・coverageの増加はproducerのclaim容量と異なる検証供給である。将来の容量倍率を上げてもbond当たりのblock/reward/Final weight総量を増やさず、slice/shardに同じworkの権利を重複配分しない。
Panel有りのmodeでも共通予算・最早回復時計と残存責任を維持する。Panel=0はRFC15の別gate/activationを必要とし、本RFCの供給測定で代替しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（shardとPanel=1：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。

Panel=1暫定modeでは、単一seatで全shard・全検証scopeを覆える経路だけを対象にする。shard outsider、segment mask、`seats-1` 等の複数seat前提を一括定数置換しない（RFC-0010 2026-10-12改定の1-seat移行監査）。

<a id="spec-closure-2026-10-15"></a>

#### Spec closures — 2026-10-15

**Scope and reading rule.** This section closes specification gaps for R6-CELL, R6-DRAW, R6-OUTSIDE, R6-PRICE,
R6-MIGRATION and the cross-cutting ALL-MODEL group of the
[design audit's requirement index](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引).
It states rules that the [RFC6/10 implementation record](../design/palw/verification/shard-rfc6-10.md) and the 2026-10-13/14 RFC06
records implement but this RFC never stated, and marks stale text. [PRINCIPLES](../PRINCIPLES.md) and
ADR-0173/0176/0177/0178 rank above it; later user rulings prevail over older RFC text. Nothing here changes live
shard v1 at DAA 5,300, arms a fence, assigns a height or records a PASS. Mechanics or private-process results are not
independent review or economic evidence. "USER DECISION REQUIRED" marks a named parameter whose value needs the
user's decision; a recommendation appears only where an existing record justifies it.

##### R6-CELL — coverage of new cell profiles

The first-divergence lemma (§2.1) remains a localization/conformance lemma for a **complete** check of a cell's
commit points. For any new probabilistic or algebraic cell profile (RFC07 Part V/VI) the following are normative:

- **PALW-SH-10 (relation coverage).** A cell receipt's declared scope MUST name every relation of the cell it
  covers — products, quantization/rounding, nonlinear, routing/TopK, `Hist`/`Fixed` memory updates, carry-in and
  carry-out boundary rows, decode-token door — and the claim's licence MUST cover every relation of every cell.
  A relation without a checker leaves the claim unlicensable.
- **PALW-SH-11 (boundary equality).** Equality of a carry-out with the next shard's carry-in, and of a checkpoint with
  the next segment's input, MUST have a designated checker or an aggregate relation that covers it. A committed but
  unchecked boundary is not coverage.
- **PALW-SH-12 (whole-claim error).** The whole-claim false-accept bound is the union over all cells, relations,
  boundaries and selection loss, charged per RFC07's cumulative accounting. Checking a random subset of cells is
  not whole-claim coverage; one faulty cell is found only with its selection probability.
- **PALW-SH-13 (localization).** A failed check localizes through authenticated partitions to an existing exact
  terminal (`TirShardCourtAccused`, logits doors, H-dissection), with bounded total work, bytes and moves, and is
  fileable by any eligible public bond holding the authenticated material.
- Remaining (observations): proof of whole-claim soundness for small constraint checks, and boundary/memory/routing
  coverage per accepted model profile, are not established.

###### R6-CELL round 3 — claim-wide boundary, routing and memory coverage (2026-10-15)

**Status:** this is an argument plus a checker in source form. It is not compiled, not tested and not reviewed, and
it is no PASS. It refines the remaining observation above and does not close it.

**Committed trace.** Every value a TIR claim commits is one of five things:

- a commit-point node tile (`PalwTirLeafKindV1::Commit`) of an occurrence (pre, layer `l`, post) at a position;
- a `Fixed` state leaf (`State`);
- a `Hist` window tile (`HistTile`);
- a logits row;
- the delivered token.

Every node input is exactly one of the following:

- **(i)** a node of the same occurrence;
- **(ii)** a carry-in slot (`Ref::CarryIn`), whose bytes are the previous occurrence's committed carry-out tile, with
  no second commitment;
- **(iii)** a state (the previous position's `State` leaf or `HistTile` window);
- **(iv)** a param, authenticated against the registered artifact;
- **(v)** the job input or the previous position's delivered token.

**Lemma (claim-wide first divergence).** Assume the binding term (BLAKE2b-512 collisions) does not fail. Suppose a
committed trace differs from the reference evaluation of the registered program on the authenticated params and the
job input. Then the first differing value, taken in evaluation order (position, then occurrence, then node), is
exactly one of three things:

- **(a)** the output of one relation instance of one cell, whose own inputs are all correct committed values or
  correct recomputations;
- **(b)** one state update (a `StateWrite`/`HistAppend` row, or a window that is not the previous window plus the new
  row);
- **(c)** the decode door (a delivered token that is not the decode of its committed logits row).

*Proof sketch.* Take the first differing value. Every input it reads comes earlier in evaluation order, through
(i)–(v), so each input is either correct or itself the first divergence:

- inputs (i) and (iv) are correct;
- input (ii) is the same bytes as an earlier commit point;
- input (iii) is an earlier `State`/`HistTile` leaf;
- input (v) is the job's input or an earlier door.

The carry slot is not a separate relation: equality of a carry-out with the next carry-in holds by construction,
because the next cell reads the committed carry-out tile. This holds only if every carry-out node is a commit point,
which `validate` enforces and the checker re-checks. The same argument covers a shard cut, which is a carry edge
between occurrences held by different shards (`palw_tir_shard_partition_v1` is contiguous), and a position-segment
cut (`S_P = 2`), which is a state/`HistTile` edge between positions.

**Coverage requirement.** By the lemma, a claim is localizable when each of the following has a verifier that maps
it to a bounded exact terminal (`TirShardCourtAccused`, the logits/decode arms, H-dissection):

- every relation of every node, by its family's descriptor checker (PALW-SH-10). Routing (TopK, Gather, Compare,
  Select, ReduceMax) is the Selection family and is checked by exact recompute;
- every carry-out slot, through the cell verifier's read of committed tiles (PALW-SH-11);
- every `Fixed`/`Hist` state, through `StateContinuity` plus `State`/`HistTile` leaf recomputation;
- the declared shard shape;
- the door.

The whole-claim false-accept bound is then the PALW-SH-12 union:

- exact verifiers contribute `0`;
- probabilistic relations contribute their RFC-0007 per-instance terms (R7-RELATION items 4 and 7), over every
  position;
- the binding term is added;
- if fewer than all cells are checked, the selection loss is added (`⌊log₂(N/(N−s))⌋`), which a uniform sample of
  `s` of `N` cells pays.

A beacon mismatch only localizes. Conviction is always the exact terminal's.

**The checker** is `consensus/core/src/palw_cell_coverage_v1.rs`. It is pure, and no consensus path calls it. It
enumerates every boundary the code accepts and routes each one to its verifier.

`palw_tir_cell_coverage_v1(descriptor, program, (S_L, S_P))` enumerates:

- every node as a relation, using `family_of_prim` and the descriptor's `support`;
- every Selection node again as routing;
- every `carry_out` slot (it must name an existing commit-point node);
- every `StateKind::{Fixed, Hist}` (exhaustive match, so a new kind cannot compile unclassified);
- the shard shape. Only `2 ≤ S_L ≤ min(64, layers)` and `S_P ∈ {1, PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1}` are
  accepted; any other shape is `UnknownProfileKind`;
- the decode door.

`palw_legacy_cell_coverage_v1(profile)` enumerates, for a `PalwShapeProfileV3`:

- the profile version (V1–V3 only; any other version fails closed);
- every layer by `layer_kind` (exhaustive over `GatedDeltaNet` / `Attention`) and every layer carry;
- the MoE router of every layer table. Exactly one q36 router TopK is covered. More than one is uncovered, because
  the router slot is then not unique. A fenced Kimi router is uncovered;
- each memory section the layers need (attention cache, recurrence state) under the registered state chunk map:
  - **no map:** the long form covers the section;
  - **a known map:** the section is covered only if the map spells that section **and**
    `palw_state_chunk_count_at_v1` enumerates it at a checkpoint;
  - **an unknown map id:** `UnknownProfileKind`.

`palw_shipped_legacy_profiles_v1` enumerates the shipped ladder families (graph v1, v5 and v7, and the v8 hybrid)
at every row in `PALW_CONTEXT_LADDER_ROWS`. The tests drive the TIR checker over the sketch fixtures (dense + MoE,
windowed, wide128) under K2-TIR-v2 and v4.

**Gaps the checker names (from reading the code; not yet run).** These are recorded gaps, not passes:

- The v1/v2 hybrid maps enumerate only their attention half. The recurrence section of a hybrid profile under them
  is uncovered.
- The standalone recurrence maps (`gdn_state_chunk_map_id_v1`–`v3`) are not enumerable by the court.
- Outside the checker, the step leg still treats an unknown, non-held map as "no fault"
  (`palw_step_leg.rs`, `_ => {}`), and V1/V2 profiles accept any map id. Both are fail-open acceptance paths that
  need a versioned fence to close.
- MoE in the legacy graph exists only as kernel and weight-name conventions.
- Whether the legacy step court covers every operator of a layer is asserted by name here, not re-proved.

**Remaining:**

- compile and test the checker;
- decide how to close the fail-open map paths (USER DECISION REQUIRED for any fence);
- an external review of the lemma;
- extend the enumeration to every registered class row of a live network, once a catalog of registered TIR
  classes exists.

###### R6-CELL round 4 — the coverage closure fence (2026-10-15)

**Status:** dormant source. Not compiled, not tested, not reviewed, and no PASS. The fence is `None` on every preset,
and `validate_palw_cell_coverage_closure_v1` refuses every real activation. Nothing is armed.

**Fence.** `Params::palw_cell_coverage_closure_v1: Option<ForkActivation>`. It is hashed Some-only into both
fingerprints, listed in `palw_fences_v1`, and collapsed from `Some(never())`. It has a `fork_id_v1` probe arm and is
mirrored to `PalwStateParamsV2::cell_coverage_closure_from_daa` by `Params::sync_palw_cell_coverage_closure_v1`. It
has an A-2 central-table row (`Int12FoldPastFence`, tags 3 / 8 / 12) and an arm in the A-2 harness. It pays and
weights nothing, so it is **not** in `PALW_N3_GATED_REWARD_FENCES_V1`. The rule is in
`consensus/core/src/palw_cell_coverage_closure_v1.rs`.

**One predicate.** `palw_cell_closure_legacy_refusal_v1(profile)` returns `None` exactly when
`palw_legacy_cell_coverage_v1` covers every boundary. Otherwise it returns the refusal:

- an unknown kind first, failing closed by name;
- else the first uncovered kind.

The checker now tags every gap with a `PalwCellUncoveredKindV1`.

**Rule past the fence.** Below the fence, every hook answers "old rule".

| REQ | Where | Keyed on | Past the fence |
|---|---|---|---|
| REQ-CELL-1 | `object_preconditions_v1` (fold second lock), `ClassRegistered` with a carried profile | the registration block's DAA | an uncoverable profile is refused: `CellCoverageRefused`, code `CELL_UNCOVERABLE` |
| REQ-CELL-1b | `check_class_admits_claim` (attempt, free-prompt, evaluation lanes) | the claim's acceptance DAA | a class whose graph the fold holds (`fp_work_profile_of`) and that is uncoverable takes no claim: `ClassNotRewardable`, code `CELL_UNCOVERABLE` |
| REQ-CELL-2 | `adjudicate_court_close_v4` (all three processor sites), plus the pure `check_step_refutation_closure_capped_v1` | the claim's acceptance DAA | see the paragraph below |
| REQ-CELL-3 | everywhere | — | below the fence (every shipped network: the mirror is `None`), verdicts, writes and roots are byte-identical. A claim accepted before the fence keeps its old ruleset. `adjudicate_court_close_v3` is v4 with `None` |

**REQ-CELL-2 in detail.** This applies where admission cannot see the graph: a class registered before the fence, or
a genesis class whose graph the fold does not hold. A close of a claim accepted past the fence is handled in this
order:

1. **The class is uncoverable.** The legacy binding is first pinned to the registered class and to the claim's
   execution root, and `verify_binding_v1` is run on it. The close then adjudicates `ExecutorGuilty`, objectively,
   from the binding alone. This is never `ChallengerDefeated`.
2. **The pure checker's closure form.** The shape pass answers `ShapeProfileNotCanonical` (kind 0, leaf 0). A mapped
   checkpoint count the court cannot derive is `CheckpointIndexNotCanonical`, not "no fault".
3. **No map.** The long form keeps its `None`.

**The fail-open is closed.** `palw_step_leg::checkpoint_fault` read an unenumerable map as "no fault", and V1/V2
profiles accepted any map id. Past the fence, neither path can reach a producer win:

- a new class with such a map is refused at registration;
- a new claim on such a class is refused at the claim gate, or convicted in court.

Old claims keep the old rule, because the fence is not retroactive.

**Coverage matrix, round 4.** `palw_cell_uncovered_disposition_v1` is exhaustive. In round 4 every uncovered kind is
**refused at admission**, and no kind is newly covered. Each covered case is named by its existing verifier.

| Case (legacy `PalwShapeProfileV3`) | Round 4 | Why |
|---|---|---|
| no map (long form), one q36 router per table | covered | `LegacyLongForm`, `LegacyRouterTopk` |
| attention-only maps (integer KV v1/v2, tiled v3/v4) on attention-only layers, enumerable | covered | `LegacyStateMapEnumerated` |
| hybrid v3 / held v4 / P0a v5 (v5 under a version-3 profile), enumerable | covered | `LegacyStateMapEnumerated` (both sections) |
| unknown map id | refused at admission (`UnknownProfileKind`) | the court cannot enumerate it, so the old step court failed open |
| hybrid v1/v2 map with GDN layers | refused (`HybridRecurrenceUnenumerated`) | covering it needs the recurrence geometry in the consensus crate plus an anchored GDN arm. v3/v4/v5 *are* that coverage, so register one of them |
| solo recurrence map (`gdn_state_chunk_map_id_v1`–`v3`) | refused (`SoloRecurrenceMap`) | its geometry belongs to the executor crate (`base0_gdn_state_geometry_v*`) |
| a known map missing a needed section (e.g. attention-only map under GDN layers) | refused (`MapSectionNotSpelled`) | the section has no anchor |
| a known map not enumerable at a checkpoint of this profile (e.g. v5 under a non-v3 profile) | refused (`MapNotEnumerable`) | the count is not decidable |
| more than one q36 router in a table | refused (`MultiRouterTable`) | `qwen36_router_topk_slot` answers `None`, so a routed product is unadjudicable |
| a fenced Kimi router | refused (`KimiRouter`) | its own row recomputes (`kimi_row`), but routed gather/combine resolve routing only through the q36 slot |
| TIR classes | outside this fence | coverage is per descriptor (`palw_tir_cell_coverage_v1`). No shipped fixture has a gap. The TIR kinds' disposition is "refused" if ever wired |

**Tests (source only, not run):**

- `palw_cell_coverage_closure_v1/tests.rs`:
  - `req_cell_the_fence_is_dormant_everywhere_and_refused_when_armed`;
  - `req_cell_1_an_unknown_map_is_refused_at_registration_past_the_fence`;
  - `req_cell_1_every_uncovered_kind_is_refused_at_admission`;
  - `req_cell_1b_the_claim_gate_refuses_an_uncoverable_held_class`;
  - `req_cell_2_the_court_never_acquits_an_unknown_map_past_the_fence`;
  - `req_cell_3_below_the_fence_every_predicate_is_the_old_rule`.
- `palw_step_leg.rs`: `req_cell_2_an_unknown_map_is_never_no_fault_under_the_closure`. This uses the honest fixture,
  whose map is unknown, and checks two things:
  - below the fence it is `NoFaultFound`, identical to the live checker;
  - under the closure it is `ShapeProfileNotCanonical`.
- `palw_cell_coverage_v1.rs`: `round4_every_uncovered_kind_is_enumerated_with_its_disposition`.

**Remaining:**

- compile, run and review the round-4 sources;
- a court-level fixture test of `adjudicate_court_close_v4` on a live-shaped session;
- whether the Panel false-valid structural routes (`palw_offence_v1` / `palw_offence_attribution_v1`, which call
  `check_step_refutation_capped_v1`) should pass the closure flag. They convict Panel signers, not the producer;
  past the fence an uncoverable class takes no new claim, or is convicted by the court;
- the arming decision (USER DECISION REQUIRED).

##### R6-DRAW — the V3 per-shard draw

In addition to the 2026-10-11 restore contract above:

1. Per-stratum populations, seed and strata are frozen in the pre-entropy snapshot. Each stratum draws its class seats
   one per operator, plus an optional outsider whose operator holds no other outsider seat of the claim in this or an
   earlier round.
2. A retry excludes every operator that sat in that stratum in an earlier round; alternates are never reused per stratum.
3. **One reservation:** a bond seated in several strata of one round reserves its per-seat exposure once per claim and
   binding; retries do not accumulate reservations.
4. A stratum that cannot be filled ends the claim `NoCapablePanel` (non-fraud). A sharded class never falls back to a
   flat panel.
5. The per-block draw budget counts draws (one per stratum); the first due claim that does not fit stops that block's
   draws; no subset of strata is chosen.
6. **Named parameters** (V3 only; live v1 constants unchanged): per-claim retry bound, strata count bounds (code:
   2..64), class seats and outsider per stratum, and the retained candidate-population cap (the dormant mechanics refuse a
   fifth retained 100k population): **value: USER DECISION REQUIRED**.
7. **Snapshot transport.** Four retained 100k populations with 100,001 bonds encode to 437,719,699 bytes, above the
   unchanged legacy 384 MiB cap; that shape requires the negotiated protocol-107 compressed transport. Its experimental
   bounds (512 MiB canonical decoded, 384 MiB wire) are not approved: **value: USER DECISION REQUIRED**. Old peers keep
   protocol 106 and the legacy cap; a timeout never switches codec.
8. Remaining: V3 arming, historical collateral headroom, multi-process P2P/pruned IBD (validation-only).

##### R6-OUTSIDE — non-seat verification and pre-emption

1. A non-seat watcher holds no seat, signs no receipt and needs no outsider selection. It may file
   `TirShardCourtAccused` or a `TirStepRun` DA demand within the non-seat budget, using only public authenticated
   material; it never asks the producer with a seat's signature.
2. **Pre-emption.** While a DA session, court session or `DefaultDisputed` state is open on a claim, no non-fraud end
   (neutral void, receipt/bind timeout) and no Final may take effect; the engine's own decision is deferred, not
   re-rolled. An accusation carried in the same block as an already decided end is late, not pre-emptive.
3. **Outsider `Incapable` (reconciles §4.2 with ADR-0177 D1 supplement).** When an outsider cannot obtain or hold its
   shard, the part cannot license and the claim ends as a **non-fraud termination** for missing required verification.
   This MUST NOT slash anyone, penalize the registrant, suspend the class's registration or stop rewards of other valid
   claims, and it is not a judgement of fraud.
4. **No positive detection floor is assumed.** A watcher's existence is not evidence that a check happened; where no
   independent holder of the model checks, the effective detection probability may be zero (ADR-0177 D7 and its note).
5. Remaining: actual detection-floor evidence; watcher compensation (see R6-PRICE).

##### R6-PRICE — cell work, residency and capacity

1. Past the dormant `palw_tir_shard_segment_v2` fence, a seat's price share is `max(Σ work, Σ resident)` over its
   mask's cells (floored at 125 ‰, capped at 1,000 ‰), keyed on the claim's immutable `accepted_daa`; the lock a counted
   signer posts and the pay weight fixed at bind MUST use that same share. The slash term is unchanged.
2. Seat readiness past that fence is node policy: a node proves possession of a shard only when its ledger can hold the
   shard's heaviest cell. Node readiness, local resource admission flags and local quota evidence are **never** an
   on-chain capacity proof or a consensus input. Unknown resource bounds refuse (fail closed); they are never zero.
3. The class room (§6.2) and the measured-speed constant (2,000 ‰) are unchanged.
4. **Named parameters**, each requiring measurement on release hardware: worst-cell public material bytes, worst-cell
   fresh-verifier time, per-shard court capacity, and the non-seat watcher's compensation/fee. **value: USER DECISION
   REQUIRED** (no measured basis exists).

##### R6-MIGRATION — live v1, history and new profiles

1. The rules armed on testnet-12 at DAA 5,300 are preserved; any change requires a new versioned fence.
2. The **Decision** section is authoritative for v1. Where the historical questions differ (question 5's `S_P = 1`
   against the Decision's layers-only default with `S_P = 2` offered), `palw_tir_shard_plan_shape_v1` and the Decision
   govern.
3. **Superseded text (kept as history):** the 2026-10-08 status note's list of open items (watcher, segment pricing, V3)
   is superseded by the 2026-10-11 implementation update; the 概要 "有効化" bullet, §10's "One dormant fence", and the
   compatibility bullet "Dormant: no fingerprint …" describe the pre-activation plan, not t12 after DAA 5,300.
4. New profiles (V3 draw, segment pricing v2, probabilistic cells) need their own versioned fence and approval and stay
   dormant until an independent Work Beacon bootstrap is verified (N3, 2026-10-12 user ruling).

##### ALL-MODEL — shared rules for RFC6/7/8/12

1. **Identity.** A registered model's ID/root, weights, tokenizer/config and execution spec are immutable
   (ADR-0175); different content is a new Model ID. Claims bind that root/spec; fetched bytes are authenticated against it.
2. **No acquisition gate.** The chain does not manage model distribution, seeders or acquisition results. Miner and
   Panel MUST possess the correct model to participate under the applicable ruleset; a registrant owes no distribution.
   Acquisition failure alone never slashes, suspends registration or rewards, or extends a challenge window.
3. **Claim-specific evidence only.** New demand units target claim-specific witness/state/trace; requests that would
   enumerate or reconstruct model weights are prohibited. The four armed t12 court routes that can compel model bytes
   (`HeldStepLeaf`, `AttnRootClaimOperands`, `TirGenRootClaimParams`, `SeatReadinessLeaves`) are closed by a **dormant
   restriction fence** (2026-10-12 user ruling D2). A profile whose terminal needs model operands the court cannot
   authenticate under that restriction MUST NOT be presented as adjudicable.
4. **Non-public evidence paths** (e.g. V5 image/source IDs, `PanelDa`) are excluded from new reward and weight for
   claims accepted after a new fence (2026-10-12 user ruling N4).
5. **Allocation.** Model coinbase allocation uses distinct bound capital `S_m` without double counting, inside the fixed
   total budget and every per-bond Q/B/R/F cap; it changes no block frequency, DAA, fork choice or Final weight, and pays
   only valid claims. Allocation curve: **value: USER DECISION REQUIRED** (current provisional: `A_m = S_m^α`, α = 1.5,
   per the 2026-10-10 ruling; the 2026-10-12 `S_m·μ(S_m)`, μ ∈ [1.3, 2.0] proposal is undecided).
6. The effective detection probability of a closed model may be zero; this is an open safety gate, not closed by
   allocation economics.

---

<a id="source-v1-0007-L00001"></a>

### 旧RFC-0007 / 原文1–507行

出典: [0007-palw-verification-certificates-and-algebraic-checks.md](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)。配置理由: 主題別統合。

### RFC-0007: PALW probabilistic detection — direct checks, authenticated receipts, and deterministic court

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。
> **2026-10-15 恒久改定 — [PRINCIPLES §8](../PRINCIPLES.md#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定):** MISAKA はモデル計算の正しさの暗号学的証明を使用せず、将来の採用・研究候補にも置かない。ZK/SNARK/STARK/zkVM/zkML、PCS に基づく succinct validity argument、GKR、producer 送信の sum-check validity transcript、再帰・proof aggregation の方向は撤回する。以下の既存 wire 名・過去の測定は履歴であり、禁止方式の採用根拠にならない。
>
> 許容するのは認証済み operand を検証者が実際に読む直接 Freivalds・代数的スポットチェック、サンプリング再実行、決定論的再計算、公開証拠の局所化と exact court である。合意 credit は抽選された検証者の承認を使い、正直な割合・重み・相関・実参加・資料取得・期限の前提を明示する。署名・quorum・hash/Merkle は対象と承認者の認証であり、算術真実を証明しない。128-bit 等の条件付き検査誤差を全モデルの暗号学的健全性へ拡大しない。未知の参加率・検出率・市場数値は UNKNOWN、数値は検証後に決める。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Revised design, 2026-10-08: **Part VI is the sole post-commit challenge protocol for the proposed new Kernel/model/claim/slice/public-check route**; Part V remains the new Panel verification direction. Neither revision is implemented or activated by this document. Parts I–IV preserve their historical design/prototype results, not live status. |
| Author(s) | MISAKA core (drafted with Claude; lane M4) |
| Created | 2026-10-01 |
| Normative dependencies | **RFC-0002** (TIR semantics and court), **RFC-0011 §§13/15** (verification-plan admission, probabilistic acceptance), **ADR-0171** (selected direction); existing Panel/segment rules, ADR-0065/0098/0111 and spec 18 for compatible legacy paths. New receipts and coverage rules require the separate Part V fence. |
| Affects | spec/palw 07/08/18: scheme/profile/evidence-bound receipts, assignment, coverage and lifecycle; 09: bounded localization into compatible exact court, separately versioned extensions if needed; 14/16: verifier operation and new fence. Existing Parts I–IV are legacy baselines; Part V changes require coordinated SDK, node, fold, DA, RPC and replay work (§V.9). |
| Original prototype branch | `rfc7/algebraic` (off `rfc4/int` @ `ce04e5c22`); the 2026-10-06 design revision is audited against `main` commit `7bff920dc`. No claim that the new scheme runs on a deployed node. |
| Related | RFC-0006 (layer-sharded panels, lane M3), lane M2's runtime residency for IR classes (`TirRowSourceV1`, ADR-0112 for IR classes), lane P's root-cause report of the testnet-12 panel backlog (`rcore/int-10-p1:docs/design/palw/t12-panel-backlog-1001.md`), ADR-0029 (carriage), ADR-0038 (receipts are claims), ADR-0062 (the data-availability court), ADR-0069 (weight needs adjudicability), ADR-0072 (the ticket is the execution), ADR-0080 (a receipt is 4,772 bytes), ADR-0097/0099/0100 (shards, the stratified panel), ADR-0103 (held context), ADR-0112 (residency), ADR-0124 (supplementary receipts), ADR-0133 (verification is its own clock), ADR-0147 (the admission jury), ADR-0152 (collateral, ejection), ADR-0160 (capacity) |

> **Spec closures 2026-10-15:** normative rules and named parameters for R7-SUBJECT/SOURCE/RANDOM/RELATION/ECON are in [Spec closures — 2026-10-15](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#spec-closure-2026-10-15). They imply no PASS, approval or activation.

> **Revision precedence:** Part VI owns new post-commit source eligibility, ordering, seed derivation, sampling, interactive-round timing and recovery; other RFCs reference it instead of defining competing seeds. Part V owns checker/receipt coverage. Part II's private sketches and Parts III/IV's older assignment/audit sources are historical or legacy protocols, not substitutions for the new public challenge source. Existing consensus is governed by [spec 18](../spec/palw/18-verification-certificates.md), not retroactively changed by this RFC.

> **Kernel-only boundary (2026-10-06):** [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
> and [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) govern Part V. Checkers are approved versioned kernel
> relations; registration, claims, receipts and transcripts bind descriptor/plan identity. Direct checks do not authorize arbitrary uploaded guests or universal CPU circuits.
> Missing relations require a coordinated kernel upgrade and matching bounded court, not a VM
> fallback. Part V's probabilistic Final policy is unchanged.

#### 概要(日本語)

**通常のPanel検証は、割り当てられたsegmentを丸ごと再実行する方式から、commitされた計算のconstraintを安く検査する方式へ進める。** この方針をPart Vの中心要件とする。

- 同じweightを使う複数token/cellの行列積をまとめ、batched Freivaldsで検査する。それ以外の関係は、対応する決定論的再計算・局所検査で照合する。
- routing/TopK、量子化・丸め、非線形演算、入力・出力、memory/history、segment境界も検査範囲に含める。安いexact checkまたは登録 kernel の直接検査・決定論的再計算を使い、未検査の関係を残さない。
- receiptには方式・profile・challenge・検査範囲・証拠を署名でbindする。**区間の検査成功を、claim全体の成功へ無条件に格上げしない。** 部分receiptは被覆条件を満たした範囲にしか数えない。
- 異常時にだけ、食い違いを局所化して互換性のある既存courtへ渡す。check失敗だけでslashせず、nodeはterminal stepをexactに判定する。
- 正の証拠、必要な被覆/quorum、DA、challenge window完了が揃えば、RFC11の誤受理確率を許容してFinalにする。nodeは通常時にモデルを実行せず、署名・割当・commitment・期限・被覆を検証する。
- Kimi K3級への拡張を目指すが、速度向上は未実測。最初はTIR MatMulの再計算とbatched Freivaldsを、CPU時間・証拠生成・帯域・メモリ・cold/warmコスト込みで比較する。

##### 旧草案の概要（2026-10-01の設計・計測記録）

以下は元のPart I–IVの説明を保存したもの。新方式に対する規範は上記とPart Vであり、過去の速度推計、quorum、capped-mode、休眠状態の記述を新方式の実績として扱わない。

- **この RFC が扱うもの。** testnet-12 では検証の供給が claim の throughput を縛っている(lane P の 10-01 報告)。容量計画は bond あたり claim ×10 → ×100 → ×1000。
  この RFC は検証を安くする二つの梃子を定める。**(1) 集約を安くする**(Part I)と **(2) 検査そのものを安くする**(Part II)。そのうえで安全性の
  パラメータ(Part III)と、全 seat を使う監査網と新クラスの段階的立ち上げ(Part IV)を定める。
- **前提: 分割は仕事を減らさない。** 1-of-N の下で、ある 1 点の嘘が逃げる確率は f^m(m = その点を独立に検査する seat の数)。区間に分けても
  「誰がどこを見るか」が変わるだけで、区間あたり m を保てば総仕事は ≈ m × (1 job)。効く梃子は二つで、検査を安くすることと集約を安くすること。
- **Part I — seat の vertex と tally による licence。** 今の licence は claim ごとに ML-DSA-87 署名つき receipt を 3 枚(≈ 14.4 KB)運ぶ。
  t12 の 5.3 claim/DAA では 76 KB/DAA で、×100 なら 7.6 MB/DAA、×1000 なら 76 MB/DAA(block mass 0.5 MB の 150 ブロック分)。
  - seat は round ごとに **vertex を 1 枚** 署名する。中身はその round の判定すべてを leaf とした Merkle root と、その leaf 列。
  - vertex は 1 度だけ運ばれ、fold が leaf を数える(**licence by tally**)。panel seat の Valid が quorum に達した時点で ReceiptLicensed になる。
  - 同じ round に食い違う vertex を 2 枚署名すれば equivocation として slash する。Bullshark の順序づけは不要で、GHOSTDAG が順序を決める。
  - DA certificate は vertex の `Held` leaf として載せる。
  - 運搬量は claim あたり 14.4 KB から 66–330 B(参照の形と leaf 数による)に減り、×1000 でも数ブロック/DAA に収まる。
  - collector による licence 組み立て(V04 の重複運搬)は消える。
- **Part II — seat ローカルの代数検査(試作と実測あり)。** TIR の trace は MatMul の accumulator を commit しない(i64/i128、PALW-TIR-5)。
  - producer は seat に「witness」を配る。中身は検査対象の MatMul 出力すべて。
  - seat は weight 積を、自分だけの秘密ベクトル r による **sketch** s = W·r と Freivalds で照合する。検査コストは |out| + rows·K で、
    sketch は weight の 0.3–1.2 %。routed expert は expert ごとの sketch を使い、expert は seat 自身が再計算した TopK で選ぶ。
  - act×act は fresh r で照合する。P·V は per-row history sketch を使い、Q·Kᵀ は再計算する(score を配ると割に合わない)。
  - それ以外の node はすべて、court と同じ reference evaluator で厳密に再計算する。導いた commit 行を claim の root と照合する。
  - modulus は refined range proof の span で決める(P61 / P61·P64 / +P63)。1 段足りないと「誤差 = P61」が通ることをテストで示した。
  - r は決して公開しない。**sketch も r と同じだけ秘密**(W が公開なので、N ≤ K なら s から r が解ける)。
  - 検査の失敗は verdict ではない(witness が悪いだけかもしれない)。seat は Valid を出さず、厳密な court(不変)へ進む。
    失敗ノードの weight 行は、既定の block sketch(fetch 上限 F = 2 MiB。例: head は B = 256 で sketch +3.1 MB、fetch 1.8 MB)で絞った
    artifact opening で取るか、保持者の seat に回す。
  - 実測の結論: **効くのは「RAM に載らない class の decode を ≥ 100 Mbps で」**。
    - NVMe から weight を読む seat に対し、1 Gbps で 5–10 倍速い。
    - RAM 常駐の seat に対しては 1–2 Gbps 以上でないと得にならない。
    - prefill は 1.5–5.5 Gbps 未満では常に再計算が勝つ。
    - t12 の floor/8k class には効かない(梃子は Part I と scheduling)。
    - Part II の価値は実現可能性。qwen4_exp 相当を、常駐 2.3 GB(全体は 132–183 GB)の seat が検証できる。
- **Part III — 安全性の枠組み。** 1-of-N と厳密 court を保つ。「最大の互換クラスタが正しい」は採らない(ユーザー決定)。
  - 重みは bond と Sybil 耐性のある seat 資格、commit 後の無作為割当、permissionless challenge、無作為監査から来る。
  - 全区間の検査(代数か全再計算)が安い class では、m = panel 全員(5)。逃げる確率は f^5。
  - 標本検査(k = 4 区間)では 1 点の嘘の捕捉は ~6 %。抑止に要る slash は利得の約 17 倍になる。
- **Part IV.1 — 全 bonded seat からの無作為 leaf 監査。** 監査は one-move court の leaf check を off-chain で行うもの。
  - 監査人は class の ready seat に限らず、全 bonded seat から anchor beacon で commit 後に引く。
  - 必要なのは opening だけ(試作では tile あたり平均 1.1 KB)。
  - 全面的な捏造と部分的な手抜きは 1 − (1−q)^m で捕まる(試作で実測一致)。1 点の嘘は ≈ m/N でほぼ捕まらない。
    だからこれは sensor であって floor ではない。
  - 沈黙は Valid と見なさない。Verifier's dilemma には監査トラップ(D-F2 型の故障を植える)で対処する。
  - 小さな class(ready seat が 5–7)で最も起きやすい class 内談合を、全網から引くことで薄める。
- **Part IV.2 — 段階的立ち上げ(capped mode)。** 全範囲の保持者が揃う前でも、登録と「witness を配る」DA certificate の後、mesh 監査の下で
  上限つきで block を出せる。
  - 上限は「検出されない不正の期待利得 ≤ 検出確率 × slash 可能額」から導き、grinding の害も上限で抑える。
  - 報酬は保持者が揃うまで未確定(vest しない)。保持者は vesting 期間内に過去の capped claim を再検証し、嘘なら未確定報酬を没収・slash する。
  - weight 0 の lane が fork choice を hash にただで渡す危険に注意し、既存の pwu/receipt-ramp を使う。
  - 既存のライフサイクル(Candidate → jury → Prefetching → Probation → ActiveLimited → Active、Λ = 170 DAA は不変)に接続する。
- **有効化。** fence はすべて休眠で導入する(Some-only hash と never() 潰しを 4 箇所とも、独立した高さ)。各 fence に、出荷する binary で
  跨ぐ drill を付ける。

#### Summary

The selected design changes what a Panel receipt means: a seat validates the constraints of its assigned computation with direct batched Freivalds and bounded exact native checks, rather than replaying the entire segment. Part V specifies the new profile, scope, committed evidence, challenge schedule, tally and escalation obligations. A claim passes probabilistically only when its whole statement is covered under the declared error budget; a few individually sound sampled segments are not enough.

Part I's batched signatures remain a transport building block, and Part II's private-sketch code remains an implementation baseline. Their original carriage/performance estimates do not price the new evidence or prove its speedup. Parts III/IV supply the threat analysis and diagnostic audits. Raw audits remain distinct from licensing coverage. The exact court resolves localized disputes; it does not eliminate undetected fast-path error. Implementation starts with a measured MatMul checker and then integrates typed receipts and complete coverage before reward-bearing acceptance.

#### Motivation

##### 1. Verification supply binds

Lane P measured testnet-12 on 2026-10-01:

- Issuance is 5.3 claims/DAA.
- Licensing needs 3 of 5 seats' `Valid` receipts.
- Healthy receipt supply is ≈ 600 receipts per hour against a demand of 380–640, so there is no margin.
- When two hosts' replays slowed, licensing collapsed from 5.5 to 1.2–2.5 licences/DAA. The node fixes F1–F4 and the static `L_ver`
  term of int-11 respond to that.

The capacity plan (ADR-0160) multiplies claims per bond ×10, ×100, ×1000. Every one of those claims needs verification and a licence.
Two costs scale with them: the seat's work per claim and the chain's carriage per licence.

##### 2. Splitting is not saving

Under the 1-of-N model, a lie at one point escapes when every seat that checks that point is the adversary's or does not look. With `m`
independent checkers of the point and an adversary holding a fraction `f` of the eligible weight, the escape probability is about `f^m`.
Cutting a job into intervals and giving each seat a few changes *who* checks which interval. At a given `m` per interval, it does not
reduce the work: total work ≈ `m × (one job)`.

ADR-0098 measured the other side of this. With `k = 4` sampled intervals of `N = 299`, five seats catch a one-token lie 6.51 % of the time.
Raising that to 90 % costs each seat 37 % of the job. Sampling buys a weak catch cheaply, and a strong catch costs a replay.

##### 3. Two levers

- **Make aggregation cheaper** — Part I. Today each licence carries three 4,627-byte ML-DSA-87 signatures. At ×100 and ×1000 those
  signatures alone are more than the chain carries.
- **Make the check cheaper than the replay** — Part II, where the arithmetic allows it. Freivalds' algorithm checks a product `Y = X·W`
  in `O(|Y| + rows·K)` instead of `O(|Y|·K)`. A seat's sketch `S = W·v` needs `W` once per epoch; after that, the seat needs only `Y`,
  the served accumulators.

Both levers leave the court where it is. The court recomputes one leaf exactly from committed inputs and artifact openings, and it
convicts.

#### Goals and non-goals

**Goals.**

- G1. Amortize signatures through verification vertices and keep carriage within a measured budget. Part I's legacy tens-to-hundreds-of-bytes estimate must be recalculated for Part V's new evidence bindings and availability costs.
- G2. One signature per seat per round, not per claim, at the gossip door and in the fold. (Part I)
- G3. Assigned segments are verified by their constraints, with batched matrix checks and compatible aggregate proofs; full segment replay is an optional baseline, not the normal large-model duty. Price actual weight/input access and authenticated preprocessing. (Part V)
- G4. Derive the whole-claim conditional error bound from coverage, fields, repetition, aggregation, binding and adaptive-query assumptions, following RFC11's proposed `2^-128` check target. A per-product `1/p` is not the claim's bound. (Part V)
- G5. Bind an approved verification suite, profile, scope and evidence to new signed receipts. Kernel selection is seat-local; coverage and security obligations are protocol-defined. The terminal court remains exact. (Part V)
- G6. Preserve permissionless exact disputes and explicitly account for compromised or inactive Panel members. Full constraint coverage can be algebraic; quorum alone does not prove arithmetic truth. (Parts III/V)
- G7. Every ordinary public bond, including one outside the Panel, can obtain authenticated public openings, localize a detected fault and reach objective conviction without producer-private state. Silence is never a verdict. Completion requires RFC14's end-to-end gates. (Part IV.1; ADR-0173)
- G8. Large classes become feasible with qualified constraint verifiers and bounded DA/court resources; Part IV.2's separate capped experiment never replaces the new route's soundness and coverage gates. (Part V)

**Non-goals.** A probabilistic court: the court stays exact. Weight from cluster agreement ("the largest compatible cluster is
correct"): refused by the user's decision. Cryptographic computation-validity proofs, public or private, are prohibited by §8. A BFT ordering layer: GHOSTDAG orders. This document does not change shipped terminal semantics or ceilings. Part II remains optional for legacy claims; a Part V receipt must meet its explicitly assigned suite and scope.

#### Part I — Batched verification certificates

##### I.1 What a seat signs today, and what it costs

A V2 receipt (`PalwSeatReceiptV2`, `palw_panel_v2.rs`) is the claim id (64 bytes), a verdict (`Valid`,
`Unavailable { chunk_index, requested_daa }`, `Incapable`, `Sampled`), the seat bond (68 bytes), the signing DAA and an ML-DSA-87
signature of 4,627 bytes. On the wire it is 4,772 bytes (ADR-0080). `ReceiptLicensed { claim, receipts }` carries at least a quorum, 3
of them: ≈ 14.4 KB per licensed claim. Collectors assemble licences from a per-(claim, bond) receipt pool (`palw_receipt_pool.rs`) and
carry them, one carrier in flight per panel. Before V04 (`palw_licence_order.rs`), every collector carried the same claim and ~7 in 10
carriers were duplicates the fold dropped.

| claims/DAA | ×1 (5.3) | ×10 (53) | ×100 (530) | ×1000 (5,300) |
| --- | --- | --- | --- | --- |
| licence bytes/DAA | 76 KB | 763 KB | 7.6 MB | 76 MB |
| in 500,000-mass blocks | 0.15 | 1.5 | 15 | 153 |
| ML-DSA-87 verifications/DAA (fold) | 16 | 159 | 1,590 | 15,900 |
| receipt signatures at the gossip doors/DAA (5 per claim) | 27 | 265 | 2,650 | 26,500 |

Lane P observed ≈ 6 blocks per DAA on testnet-12. At ×100 the licences alone need more than twice that, and at ×1000 twenty-five
times.

##### I.2 The vertex

A **verification vertex** is one seat's signed statement of everything it decided in one round:

```text
VerificationVertexV1 {
  version:     u16 = 1,
  seat_bond:   PalwBondKeyV2,         // the signer; its registered ML-DSA-87 key verifies
  round:       u64,                    // ⌊signed_daa / round_daa⌋, round_daa a ruleset parameter
  signed_daa:  u64,
  leaves:      Vec<VertexLeafV1>,      // strictly ascending by (kind, claim_ref); ≤ 1,024 leaves, ≤ 80,000 bytes
  leaves_root: Hash64,                 // the binary Merkle root over leaf_hash(leaf) in order (odd nodes promoted)
  signature:   Vec<u8>,                // ML-DSA-87 over vertex_message_v1, context below
}
```

- **One vertex per `(seat_bond, round)`.** A seat with nothing to say signs none. A round is `round_daa` DAA; 1 DAA keeps licence
  latency where it is today. The leaf cap keeps a vertex inside one carrier.
- **The leaves are carried, not only their root.** The fold must read every leaf to tally, so a root-only vertex would need a Merkle
  proof per counted leaf in a separate licence. With `Hash64` paths over a 1,024-leaf vertex that proof is ≈ 750 bytes, or ≈ 2.3 KB
  per claim at quorum: 6× better than today, against 44–218× for carried leaves (§I.9). The root is still signed, so a single leaf
  is provable to a third party: a court, a slash, an audit trap.
- **No parent edges.** Narwhal's vertices reference their predecessors so that availability and order can be certified. Here the
  chain carries every vertex, which is availability, and GHOSTDAG orders the carriers, which is order. An edge would add bytes and
  certify nothing new.

##### I.3 Leaves

```text
VertexLeafV1 =
  | Verdict  { claim: ClaimRefV1, verdict: PalwReceiptVerdictV2 }             // tag 0 — today's receipt, unsigned
  | Held     { claim: ClaimRefV1, object: u8, first: u32, last: u32, digest: Hash64 }  // tag 1 — a DA attestation (§I.7)
  | Audited  { claim: ClaimRefV1, leaf: u64, result: u8 }                     // tag 2 — a mesh audit (Part IV.1)
ClaimRefV1 = Full(Hash64) | Compact { bound_daa: u32, id_prefix: [u8; 16] }  // 64 or 20 bytes
```

- **A `Verdict` leaf is today's receipt minus its signature.** Its verdict variants and their meanings are unchanged (`Valid`,
  `Unavailable` naming a chunk and a request DAA, `Incapable`, `Sampled`).
- **`Compact` references** name a claim by the DAA its panel bound at and a 16-byte prefix of its id. A prefix is unique among the
  claims bound at one DAA unless someone ground a 2^64 birthday collision. An ambiguous or unknown reference is ignored by the fold, so a
  seat can only lose by using one wrongly.
- **Leaf size.** A `Verdict` leaf is 66 bytes with a `Full` reference and 22 with a `Compact` one. A `Held` leaf is 138 (94), an
  `Audited` leaf 74 (30).

##### I.4 Signing

```text
vertex_message_v1 = keyed BLAKE2b-512("misaka-palw/verification-vertex-message/v1",
                       network_domain ‖ borsh(seat_bond) ‖ le64(round) ‖ le64(signed_daa) ‖ le32(|leaves|) ‖ leaves_root)
context           = "misaka-palw/verification-vertex/mldsa87/v1"
leaf_hash(leaf)   = keyed BLAKE2b-512("misaka-palw/verification-vertex-leaf/v1", borsh(leaf))
```

Both domains are new and are added to the domain-uniqueness tests every PALW family runs (`palw_receipt.rs`'s
`domains_are_unique_across_all_palw_families`). Over these, a signature can be neither a receipt's nor another court move's. The message
binds the leaf count and the round, so a relayer can neither truncate a vertex nor move it to another round.

##### I.5 Carriage and the tally (licence by reference)

- **Carriage.** A vertex is a consensus object (`VerificationVertex { vertex }`, the next free object tag) carried like every lifecycle
  object, any node carrying any seat's vertex. The acceptance layer checks:
  - the shape: version, sorted leaves, caps, root recomputed;
  - the seat bond's registration and key, then one signature;
  - that `(seat_bond, round)` has not been accepted before (§I.6).
- **The tally.** The fold reads every `Verdict` leaf of an accepted vertex. A leaf counts toward claim `c` only if all of these hold:
  - `c` is `PanelBound`;
  - the vertex's seat holds a seat on `c`'s panel;
  - `signed_daa` is inside `c`'s receipt window;
  - it is that seat's first counted verdict for `c`.

  This is exactly the set of conditions `validate_receipt_quorum_v2` checks per receipt today. When `c`'s counted `Valid` leaves reach
  the panel quorum, the fold applies the `ReceiptLicensed` transition itself: **the licence is the tally**, and no licence object is
  carried.
  - `Unavailable` leaves reaching quorum keep their present meaning under ADR-0065 Decision 4: they abstain and the claim redraws.
  - Supplementary `Valid` leaves past the licence credit seats as ADR-0124's supplementary receipts do.
- **What a verdict is, once counted.** It stands, as a carried receipt does today. A later vertex of the same seat that says otherwise
  is ignored for the tally: observation after the fact moves no obligation. A seat that has found a lie files no `Valid` (ADR-0098
  Decision 2) and opens the court through the challenger's half, as now.
- **Why the fold, not a collector.** The fold already knows the panels, the windows and the counts, so no node needs to assemble
  anything and no two nodes can carry the same licence. The duplicate-carrier waste of V04 and its ordering policy have nothing left to
  order.

##### I.6 Equivocation

Two validly signed vertices of one `(seat_bond, round)` with different `leaves_root` are an **equivocation**.

- The fold accepts the first carried one, in GHOSTDAG's accepted order, and refuses the second.
- Anyone may carry both headers, ≈ 9.8 KB without leaves, as `VertexEquivocation { a, b }`. The fold then:
  - slashes the bond `equivocation_penalty_permille` of its stake (proposed 100 ‰);
  - forfeits every lock the seat holds on any claim either vertex names;
  - ejects the bond (ADR-0152 ejection).

  Equivocation needs no court: two signatures over one round are the whole proof.
- **What equivocation would buy.** A seat could show one view to some peers and another to the chain, or license a claim in one
  sibling block and refuse it in another. Both are refused by the rule above, and both are now priced.

##### I.7 DA certificates

A `Held { claim, object, first, last, digest }` leaf attests: "I hold chunks `first..=last` of `object` of claim `claim`, whose digest
is `digest`, and I will serve them until the claim's challenge window closes." `object` names one of:

- the capture (the committed rows, `TirCaptureV1`);
- the witness (Part II);
- a trace-manifest chunk.

A **DA certificate** is `q` distinct seats' equal `Held` leaves (`q` a ruleset parameter, proposed 3). What it is used for:

- **Openings outlive their producer.** ADR-0111's slow path — a bounded on-chain availability request — may name an attester as well as
  the producer. An attester that fails such a request is slashed its `Held` exposure: it attested availability it did not provide.
- **Capped onboarding** (Part IV.2) requires one: the producer must have shown that it serves witnesses.
- **Audits** (Part IV.1) may fetch openings from attesters.

`Held` leaves are voluntary and are paid from the claim's DA fee share. Nothing in the license path depends on them.

##### I.8 The pool and the assembler

What changes in the node:

- **Gossip** relays vertices, not receipts. The door verifies one signature per vertex under its budget: a seat's hundreds of verdicts a
  round cost one verification, where today they cost hundreds.
- **The pool** keeps vertices by `(bond, round)`.
- **Block templates** include pending vertices, like any lifecycle object.
- **What disappears.** The per-(claim, bond) receipt pool and its eviction defences, the licence assembler and the licence ordering —
  there is nothing to assemble.
- **Order of migration.** Below the fence, nothing changes. Past it, receipts are still accepted for one receipt window, so claims bound
  before the fence finish on the old path.

##### I.9 Capacity

Per licensed claim:

- **today:** 3 receipts ≈ 14,385 bytes;
- **vertex, `Full` refs:** 3–5 leaves × 66 = 198–330 bytes;
- **vertex, `Compact` refs:** 66–110 bytes.

Fixed overhead per round: one header and signature (≈ 4.9 KB) per seat that has something to say.

| claims/DAA | ×1 | ×10 | ×100 | ×1000 |
| --- | --- | --- | --- | --- |
| today (licences) | 76 KB | 763 KB | 7.6 MB | 76 MB |
| vertices, `Full`, 5 leaves a claim | 1.7 KB | 17 KB | 175 KB | 1.75 MB |
| vertices, `Compact`, 5 leaves a claim | 0.6 KB | 6 KB | 58 KB | 583 KB |
| + headers: 200 active seats, `round_daa` = 1 | +0.98 MB | +0.98 MB | +0.98 MB | +0.98 MB |
| + headers: 200 active seats, `round_daa` = 4 | +0.25 MB | +0.25 MB | +0.25 MB | +0.25 MB |
| signatures verified by the fold, per DAA | ≤ 200 | ≤ 200 | ≤ 200 | ≤ 200 |

How to read it:

- At ×1000 with compact references and 4-DAA rounds, licensing costs ≈ 0.85 MB/DAA. That is less than two blocks' mass, against 153
  today, and 200 signature checks against 15,900.
- At ×1 the headers dominate. Seats then choose longer rounds, because licence latency costs nothing when the queue is short. The ruleset
  sets `round_daa`, and a node may skip rounds.

#### Part II — Algebraic verification (seat-local)

**Baseline scope:** this section preserves the private-sketch design and historical prototype results. Part V's new default uses committed evidence, profile-bound duties and a reviewed challenge protocol; its normal path need not recompute every non-MatMul node as this baseline does. Part II alone is not evidence that the new direct-check and receipt route is implemented.

The prototype is the crate `misaka-palw-tir-sketch`. Its soundness tests are `tests/soundness.rs` (16) and its unit tests 8 (Part IV.1's
audit tests are `tests/audit.rs`, 3). Its
measurements are `docs/design/palw/tir/algebraic-verification-measurements.md`. The crate depends on the IR and the typed backend and on
nothing of consensus.

##### II.1 What the trace commits, and what a seat lacks

A TIR execution commits its **commit points** as step leaves (spec 04b §10.1). The commit points are:

- carry-outs and the logits;
- every `TopK`;
- every `HistAppend` input row;
- the points a lowerer adds until every cone fits the court;
- `Fixed` checkpoints every `C` positions, and history tiles every `h_tile` positions.

The `MatMul` accumulators are never committed. They are `i64`/`i128` (PALW-TIR-5), and a capture (`TirCaptureV1`) carries only leaf
preimages. Accumulators are exactly what a seat without the weights cannot recompute.

##### II.2 The witness

The producer serves, per position, **the output of every `MatMul` the seat checks algebraically**, plus the committed rows a dense
capture already serves. What is served is policy (§II.6), and the canonical serving set is:

- every weight product whose contraction is at least 64;
- every `P·V` from a history of 1,024 positions.

A seat may ignore any of it.

The witness carries no commitment of its own and needs none for the seat. The seat accepts only if every served value passes its check
and the committed rows it derives from them equal the claim's. A wrong served value either fails its check or produces a derived row
that differs (§II.3). The witness does not reach the court (§II.8).

##### II.3 The check, and why it is sound

Positions in order; in each, occurrences in schedule order; in each, nodes in index order:

- **A served node** is first checked against its declaration: dtype, shape at the running `H`, and **every element inside its refined
  proven interval**. An element outside is a malformed witness, as an out-of-interval committed operand is a malformed commitment
  (PALW-TIR-33). Then the node is checked algebraically:
  - **Weight product** `out = MatMul(a, b)` with `W` the static operand and `X` the other. For every **A-row** `α` — an index over the
    axes along which `X` varies, plus `X`'s free matrix axis — compare

    ```text
    LHS(α) = Σ_f v[f] · out[α ⊕ f]        RHS(α) = Σ_t X[α, t] · S[α, t]        S[w, t] = Σ_f v[f] · W[w ⊕ f, t]
    ```

    The sum runs over the axes free for the weight: `W`'s own free matrix axis, and every batch axis along which `X` is broadcast and `W`
    is not gathered. `S` is the epoch's sketch.

    A **routed** weight is gathered along batch axes, which are never compressed, so each A-row names one expert. The sketch used is
    that expert's own, and the expert is the one the seat's own `TopK` selected. A producer that serves the product of a different expert
    therefore fails the check.
  - **Activation × activation product** (`Q·Kᵀ`, `P·V`, the MoE combine). The same layout with `W = b`. The vector is fresh: keyed by
    job, position, occurrence and node. The sketch is built from `b`'s verified value.

    *Per-row history sketches* (modelled, not built in the prototype): when `b` is a view of a history window, a seat keeps one sketch
    per appended row, `S_V[h] = Σ_d σ[d]·V_h[d]`. For `Q·Kᵀ` it keeps the running `Σ_h ρ[h]·K_h`, with `ρ` indexed by absolute position
    and fixed per job and layer. A position then pays one row's sketch instead of the window's. This is sound for the reason a weight's
    sketch is: the vector is secret, and every row is verified before it is sketched.
- **Every other node the check needs** is recomputed exactly by the reference evaluator, one node at a time with its operands supplied
  (`Interpreter::eval_cone`, spec 04b §9.2). It is never read from the producer, and it never reads a weight that a sketch stands for.
  `tests/soundness.rs` runs the checker on a param source that does not contain the weight matrices.
- **After each position**, the committed rows the seat derived are compared in slot order with the served ones. After the job, their
  root is compared with the claim's. The prototype's root stands in for the step-tree root and hashes the same 4-byte lanes.

**Soundness.** Induction over the evaluation order:

- If every value a node reads is honest, an exactly recomputed node is honest.
- A served node that passes its check is honest except with probability `1/p`. `LHS − RHS = Σ_f v[f]·E[α ⊕ f]` for the error
  `E = out − X·W`. If a row of `E` is nonzero modulo `p`, a uniform `v` makes the sum vanish with probability exactly `1/p`, and every
  A-row is compared.
- §II.5 guarantees that a nonzero integer error is nonzero modulo at least one of the node's moduli.

So if every check passes, every derived committed row is honest except with probability at most `1/p` per dishonest node. A claim whose
rows are not honest then fails the final comparison.

**Tested** (`tests/soundness.rs`, all green):

- an honest witness is accepted by a seat holding no weight matrix;
- the typed backend and a reference walk serve byte-identical witnesses;
- each of these lies, with every value downstream computed honestly from it, is refused:
  - an off-by-one accumulator, found at its own node;
  - a changed narrowed output;
  - a swapped routed expert;
  - a changed attention output (served, or recomputed and caught at the next product);
  - a changed key row;
  - a non-`argmax` token;
- out-of-interval, missing and extra witness values are refused;
- over 240 random lies at random nodes, a seat accepts only a witness whose claim is the honest one, and refuses every lie in a served
  product;
- a tamperer who knows `v` builds an error orthogonal to it and passes, and the same error fails another seat's `v`. **Soundness is the
  secrecy of `v` and nothing else;**
- at a toy prime `p = 101`, random errors pass 1/p of the time (expected 198 of 20,000; the test bounds the count to [140, 260]). At
  `P61`, none passes.

##### II.4 The seat secret, the sketches and their refresh

```text
epoch key  = BLAKE2b-256(key = seat secret, "misaka-palw/tir/sketch/epoch/v1" ‖ class id ‖ le64(epoch))
site seed  = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/site/v1"  ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
fresh seed = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/fresh/v1" ‖ job ‖ le32(pos) ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
vector     = ChaCha20(seed), each word masked to p's bit length and rejected until < p (exactly uniform)
```

- **The secret is never serialised.** It has no encoding, `Debug` prints no byte, and it is wiped on drop. A production node keeps it in
  its key store, beside its signing key, and may draw it per epoch.
- **A sketch is as secret as the secret.** The sketch `S = W·v` of a public `W` is `K` linear equations in the entries of `v`, one per
  index of the free axis. When the free extent is at most `K` (a down projection) they determine `v`. A sketch store therefore has no encoding either, and an epoch's store is dropped with
  its epoch.
- **What a producer learns.** A seat's only output about a claim is its verdict. A refused lie tells the producer only that its error
  was not orthogonal to `v` — a set of measure `1 − 1/p` — and the attempt is prosecuted in the exact court. Probing `v` costs one
  conviction per probe.
- **The build.** At each epoch, the store is built in one pass over the artifact:
  - each param instance's `[min, max]`, which refines the plan's intervals to the weights at hand;
  - the moduli, per site;
  - one sketch per weight site per expert.

  This prototype reads params through the reference `ParamSource`. A node reads them through its residency's row source (lane M2's
  `TirRowSourceV1`), one row at a time. A sketch is a sum over the rows of `W` (`S += v[r]·W[r, :]`), so it streams in row order and never
  holds more than a row. Measured at 1.09 G weights/s per core: an 80 GB class of `i8` weights takes ≈ 75 core-seconds per modulus.
- **Refresh.** Rebuild per epoch, or sooner after any conviction obtained through this seat's check (a conviction shows that someone
  was probing).

##### II.5 Moduli from the range proofs

The served value and the true value both lie in the node's refined interval `[lo, hi]`, so an error is an integer `e` with
`0 < |e| ≤ hi − lo`. A node takes the fewest rungs of the ladder whose product exceeds `hi − lo`:

| span `hi − lo` | moduli |
| --- | --- |
| `< 2^61 − 1` | `P61 = 2^61 − 1` (Mersenne: shifts and adds) |
| `< P61 · P64 ≈ 2^125` | `P61`, `P64 = 2^64 − 59` |
| otherwise | `P61`, `P64`, `P63 = 2^63 − 25` (product ≈ 2^188) |

- **Why a missing rung is a hole.** One rung too few is not merely weaker: an error of exactly `P61` passes the `P61` check with
  certainty. `tests/soundness.rs` raises an `i64` accumulator by `P61` and computes everything downstream from it. A seat forced onto
  `P61` alone accepts the false claim, and a seat using the ladder refuses it at `P64`.
- **What the measured classes need.** Every weight site of the four classes measured needs only `P61`: their `i64` accumulators are
  proven below 2^61. The wider rungs are for `i32`-wide activations, wide fixed-point products and `i128` nodes.
- **The kernel.** It is branchless. Signed `i128` lanes are sized by the operand's proven bit width: an `i16` activation sums 2^30
  terms (the cap) before one reduction. It runs at 1.30 G terms/s per core.

##### II.6 Policies: what is served

A seat's policy is its own and is not consensus. The measured default:

- **Weight products with contraction ≥ 64 are served.** Shorter ones are recomputed from a held weight: the A16 lowering's 16-column
  outlier products, and scalar gates. Their output would cost 8 bytes per element against `K ≤ 16` MACs.
- **`P·V` is served** from a history of 1,024, with per-row history sketches. It is `heads × d` values whatever `H` is, and it removes
  half the attention work.
- **`Q·Kᵀ` is recomputed.** Serving its scores costs 8 bytes per score against `d` = 128 MACs saved, which never pays below
  ≈ 10 Gbps (measurements §5).

The producer serves the canonical set of §II.2. A seat that wants less reads less.

##### II.7 The trace-serving obligation and its DA

The producer's duty is to **serve the witness** of a claim to its panel's seats during the receipt window: chunked per interval, on
request, through the interval lane's off-chain path.

- **Enforcement is the licence.** A seat that cannot obtain a witness either recomputes, if it holds the class, or files
  `Unavailable { chunk_index, requested_daa }`. Under ADR-0065 Decision 4, `Unavailable` abstains: a producer that withholds gets no
  licence, the claim redraws once and then voids at `ReceiptTimeout`, and nobody is slashed for silence.
- **Naming witness chunks.** Today `Unavailable` names a chunk of the committed trace manifest. To name witness chunks, the manifest
  gains a witness root (fence `palw_witness_manifest_v1`, §Activation): the producer commits the witness's chunk digests at claim time.
  Without the fence, Part II still works; seats simply cannot name which witness chunk was withheld.
- **Availability past the window** belongs to the capture, not the witness. The court never needs the witness (§II.8). `Held` leaves
  (§I.7) attest the capture's availability where a network wants it beyond the producer.

---

<a id="source-v1-0007-L00533"></a>

### 旧RFC-0007 / 原文533–1497行

出典: [0007-palw-verification-certificates-and-algebraic-checks.md](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)。配置理由: 主題別統合。

##### II.9 Where it pays (measured)

From the measurements document. Per-core rates on this Mac:

- typed-backend GEMV: 14.9 GMAC/s;
- `P61` check: 1.30 G terms/s;
- sketch build: 1.09 G weights/s;
- streaming read: 48 GB/s.

The modelled seat has 8 cores, and its NVMe reads 3 GB/s. Classes were lowered from their `config.json` without weights; decode is
priced per token at H = 1,024.

| class | held + sketches (of params) | served / token, raw (packed) | recompute, RAM / NVMe | check | break-even link, RAM / NVMe (raw) |
| --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | 525 MB + 10.5 MB (1.84 GB) | 6.7 MB (3.9) | 43 ms / 611 ms | 5.9 ms | 1.44 / 0.09 Gbps |
| Qwen3-30B-A3B | 871 MB + 249 MB (31.1 GB) | 16.6 MB (9.3) | 79 ms / 1.04 s | 17.5 ms | 2.16 / 0.13 Gbps |
| Qwen3-Next-80B-A3B | 1.38 GB + 924 MB (80.7 GB) | 20.0 MB (10.5) | 159 ms / 1.28 s | 86 ms | 2.18 / 0.13 Gbps |
| DeepSeek-V3 | 3.64 GB + 2.05 GB (674 GB) | 138 MB (84) | 861 ms / 12.4 s | 138 ms | 1.52 / 0.09 Gbps |
| qwen4_exp-like (analytic) | ≈ 1.4 GB + 0.93 GB (132–183 GB) | ≈ 26.5 MB (14) | ≈ 173 ms / 1.45 s | ≈ 90 ms | ≈ 2.6 / 0.16 Gbps |

The qwen4_exp row uses the user's shape: 48 layers × 512 experts, top-10, hidden 2,048, inter 512, a 51 G n-gram table, hyper-connections.
That is Qwen3-Next's MoE geometry plus analytic terms.

The verdicts:

- **Decode against NVMe:** algebra wins 5–10× at 1 Gbps and is about even at 100 Mbps (0.7–1.1× raw, 1.2–1.9× packed).
- **Decode against RAM:** algebra wins only above 1–2 Gbps.
- **Prefill** of 1,024 positions breaks even at 1.5–5.5 Gbps (packed–raw) even against NVMe, because a batched recompute reads each
  weight once.
- **At 5 Mbps** algebra never wins.
- **Long context does not rescue it.** `Q·Kᵀ` stays a recompute, and algebra halves attention at best.

**What this means.** For testnet-12's binding classes — Qwen2.5-class dense, the 8k class prefill-heavy — Part II does not cut a seat's
time. Their levers are Part I and scheduling (lane P's F2). Part II's value is **feasibility**: a seat holding 2–6 GB checks a class of
80–674 GB at decode speed over a 1 Gbps link. That is how a qwen4_exp-sized class can have enough seats at all.

##### II.10 Bandwidth pricing

- **Who pays what.** Serving the witness costs the producer upload, and the producer wants the licence. Fetching it costs the seat
  download, and the seat chooses its path by the table above.
- **The derived number.** A class's **witness bytes per position** are a deterministic function of its program and the canonical
  serving set. `cost::tir_position_cost_v1` computes them from the program alone. The class profile should carry them next to the
  canonical work vector (ADR-0145), for two uses:
  - the verification-window derivation (ADR-0133 §11.3), so that a class whose witness cannot reach its seats within the receipt window
    is held, not voided;
  - seat pay, in proportion to the seat's chosen cost (an open question).

  None of this is consensus until a fence makes the profile carry it.

#### Part III — Security parameters and escalation

##### III.1 The model

- **1-of-N with the exact court.** One honest seat that finds a lie files an accusation, and the one-move court recomputes one leaf
  exactly from committed inputs and artifact openings. A cluster majority never decides correctness (the user's decision). Weight comes
  from bonded, Sybil-resistant seat eligibility, post-commit random assignment (the anchor beacon), permissionless challenge and random
  audits.
- **The adversary** holds a fraction `f` of the eligible seat weight. Panels are drawn by weight after the claim commits.

##### III.2 `m` per interval

This historical illustration assumes independently drawn checkers, detection when an honest checker covers the fault, and timely challenge access. `f^m` is not a theorem for arbitrary quorum/correlated operators, nor does it include algebraic error. Part V uses an explicit composition and actual coverage/tally rules. “Coverage” means all required constraints are represented, not all arithmetic is re-executed.

`m` is the number of independent checkers of one interval:

| how seats check | `m` | escape of a one-point lie | at `f` = 0.1 / 0.2 / 0.33 |
| --- | --- | --- | --- |
| full replay or algebraic, all 5 seats, duty to `Final` | 5 | `f^5` | 0.001 % / 0.03 % / 0.4 % |
| full, licensed at quorum, the last 2 seats stop | 3 | `f^3` | 0.1 % / 0.8 % / 3.6 % |
| `k = 4` of `N = 299` intervals sampled (ADR-0098) | 5 × 4/299 | `(f + (1 − f)(1 − k/N))^5` | 94.1 % / 94.8 % / 95.6 % |

Two rules follow:

- **Duty runs to `Final`, not to the licence.** Seats keep checking after quorum, and a later finding opens the court inside the
  121-DAA licence-to-`Final` window. That keeps `m = 5` for every class a seat can check fully.
- **Full constraint coverage for Part V.** Seats discharge assigned segments with the new suite; aggregation may cover the whole claim. No full-replay seat is inherently required by the new route. Raw interval sampling remains a sensor and cannot fill a missing segment or proof obligation. Legacy assignment/replay rules stay effective below the new fence.

##### III.3 Audits and challenges

- **The global audit mesh** (Part IV.1) adds `1 − (1 − q)^m` detection of wholesale fabrication across the whole bonded seat set.
- **Permissionless challenge** stays: any bonded node may re-run a licensed claim and open the court. ADR-0098's watchdog is the full-
  coverage case.
- **Audit traps** (Part IV.1) keep auditors honest.

##### III.4 Bond economics

A conviction slashes the claim's reservation. A rational producer lies when `gain > P(detect) × slash`.

- With **full coverage**, `P(detect) ≈ 1 − f^5`, so a slash of about the gain deters.
- With **sampling**, `P(detect) ≈ 6 %` for the forger's best lie, so the slash must be ≈ 17× the gain. Collateral then has to be priced
  for the class's worst lie, not its average one. The ADR-0152 design bar (maximum profit from absconding ≤ recoverable value) applies at
  that multiple.
- **Equivocation** (Part I) and **trap misattestation** (Part IV.1) are slashed from the seat's bond. They need no court, and their
  evidence is two signatures, or a signature and a reveal.

#### Part IV — The global audit mesh and staged onboarding

##### IV.1 Global random leaf audits — a sensor, not the security floor

**The audit.**

- **What it is.** An audit is the one-move court's leaf check run off-chain, with exactly the court's arithmetic: the demand evaluator of
  spec 04b §9.4.
- **Assignment.** Auditors are drawn from **all bonded seats**, not only the class's ready seats. The draw is post-commit, by the
  panel-anchor beacon, weighted by eligibility and bond. Each draw names a seat, a claim and a committed leaf.
- **What the auditor opens.** The output tile; the cone's committed inputs, opened against the step root (other commit points of the
  occurrence, carry-ins, earlier history rows, checkpoints); and the artifact rows the cone reads, opened against `artifact_root` as
  ≤ 32 KiB inventory leaves with closed-form indices. The auditor holds no model.
- **Who serves.** The producer serves the openings. This is ADR-0111's leaf demand at scale: the off-chain fast path first; on failure a
  bounded on-chain availability request; a leaf still unopened at the deadline voids the claim. Openings may also come from `Held`
  attesters (§I.7).

**Prototype** (`misaka-palw-tir-sketch::audit`, `tests/audit.rs`). A `DemandSource` answers every question of the court's evaluator from
openings and counts what it opened. On the dense + MoE fixture claim (104 tiles of 8 lanes):

- **No false alarms.** An honest claim passes all 104 audits. An audit opens 1.1 KB of values on average and 2.4 KB at most, about 30
  openings (Merkle paths).
- **A fabricator is caught at the predicted rate.** The fabricator rewrote a fraction `q` of its tiles and computed on honestly from
  them:

  | `q` | caught at `m` = 1 / 4 / 16 | predicted `1 − (1 − q)^m` |
  | --- | --- | --- |
  | 9.6 % | 9.8 / 32.5 / 76.5 % | 9.6 / 33.3 / 80.2 % |
  | 30.8 % | 29.0 / 79.0 / 99.8 % | 30.8 / 77.0 / 99.7 % |
- **One lied tile is nearly invisible.** A single lied tile among 104 is found by 8 audits 6.45 % of the time (`m/N` ≈ 7.4 %).

**On real classes.** An audit opens the tile's cone:

- the output tile, plus up to `tile_len` rows of each weight the cone reads (`tile_len` × `K` bytes — 98 KB for a 64-row tile of a
  1,536-wide projection, 573 KB of the 8,960-wide down projection), plus the cone's committed inputs;
- the court's ceilings bound it: ≤ 16 Mi terminal MACs a tile, ≤ 8 operands.

So a typical audit opens 0.1–1 MB, and the ceilings cap any audit at ≈ 16 MB of `i8` weights. At ten typical audits per seat per DAA
that is ≤ 10 MB/DAA: 0.5 Mbps at 150 s a DAA. **The mesh is the work of every seat, the low-bandwidth ones included.**

**What it catches, and what it does not.**

- **Wholesale fabrication, and partial skipping at a fraction `q`:** caught at `1 − (1 − q)^m`.
- **Not a single-point lie.** Detection ≈ `m/N`, with `N` the job's committed tiles. For a 512-position Qwen2.5-1.5B job, `N` is
  ≈ 10.6 M at 64 lanes a tile, or ≈ 170 k at 4,096 lanes. **64 audits catch one lied tile with probability 0.0006–0.04 %.**
- **Why single-point lies matter: grinding.** A single late-layer lie is exactly cheap lottery grinding. The ticket is a function of the
  execution (ADR-0072), so a producer who may lie undetected in the last layer re-draws its ticket at the price of the last layer: up to
  ≈ `L`× cheaper per draw.
- **So full coverage stays with holders**: panels, RFC-0006 shards, Part II sketching seats. The mesh is a sensor and never the floor.

**Attestations are positive and batched.** An auditor reports through a Part I vertex as an
`Audited { claim, leaf, result }` leaf: `result` is 0 for a match and 1 for a mismatch, and a mismatch is also handed to the court as a
named leaf. **Silence is never `Valid`.** An assigned audit with no attestation counts as not done; it earns nothing and, like any
silence, is never charged.

**The verifier's dilemma, and audit traps.** An auditor who attests "match" without auditing saves work and is wrong only when the tile
is false — rarely. Audit traps make that bet lose.

- **Who plants.** A bonded **trap setter** — any bond, drawn by the beacon at a rate `trap_rate` of audit assignments (proposed 1 %) —
  produces a claim of the class with **a planted fault** of one of D-F2's commit-point kinds:
  - a carry-out, a `TopK`, a `HistAppend` row;
  - a state checkpoint, a history tile;
  - the logits, a dissected attention output.

  Before the claim is carried, the setter carries `TrapCommitted { H(claim id ‖ fault leaf ‖ salt) }`.
- **Indistinguishable.** A trap claim is an ordinary claim of the class: the setter runs the producer's code and corrupts one committed
  tile, exactly as D-F2's drill does. The audit draw that hits it is the same beacon draw as every other, and the commitment reveals
  nothing until opened.
- **Revealed and settled.** After the audit window the setter carries `TrapRevealed { claim, fault leaf, salt }`; the fold checks the
  commitment, and the fault is checkable by the court's own leaf check.
  - An auditor that attested **match** on the trapped leaf is slashed `trap_penalty` (proposed: the claim's reservation).
  - One that attested **mismatch** earns the trap bounty.
  - The trap claim itself is voided **without** slashing the setter: it was declared before it was carried.
  - A setter that never reveals forfeits its trap deposit.
- **Cost.** One inference, a deposit and a bounty per trap, from a trap pool funded by a share of audit pay. At 1 % of audits, an auditor
  who skips work is caught in ≈ 1 % of its skips and pays a reservation each time. Skipping pays only if a skipped audit is worth more than
  `0.01 ×` a reservation, which it is not by construction.

**Per-seat bandwidth budget.** A seat declares an audit budget in bytes/DAA as a readiness field, not consensus. The draw skips seats
whose budget is exhausted, and a seat's audit pay is proportional to the openings it fetched and attested.

**Niche classes.** A class with a ready pool of 5–7 seats is where per-class collusion is cheapest: three colluding operators are a
quorum. The mesh draws its auditors from the whole bonded population, so detecting such a class's fabrication needs only one honest
auditor anywhere in the network. It does not catch single-point lies (above). That is why niche classes are the first users of IV.2's
cap.

##### IV.2 Staged onboarding (capped mode)

**Legacy experiment boundary:** this section records the original mesh-only capped proposal; current implemented behavior is specified in spec 18. It is not the normal Part V path and its timing/cap estimates are not a security proof for probabilistic receipts. Part V requires §V.6's full claim coverage before licensing and §V.7's Final conditions; it does not inherit mesh-only credit, arbitrary random-subset vesting or these activation estimates.

**What it is.** A class may produce blocks under mesh audits **before full-coverage holders are seated**. Three things must hold:

- it is registered;
- it has passed the admission jury (ADR-0147);
- a **DA certificate** (§I.7) shows that its producer serves witnesses and openings: `q` mesh seats attest `Held` on the class's probe
  claims' captures.

Its production is **capped**, and its rewards are **provisional**.

**The lifecycle.** A new state `Capped { since_daa }` is appended last in `PalwModelLifecycleV1`'s Borsh order: inserting it earlier
would renumber every row, the 2026-09-10 failure. Its place in the walk is between `Prefetching` and `Probation`:

```text
Candidate ──jury──▶ Prefetching ──(DA certificate, ready < seat_count)──▶ Capped ──(full coverage seated, ready ≥ required)──▶ Probation ──▶ ActiveLimited ──▶ Active
                         └──────────────(ready ≥ required, as today)─────────────────────────────▶ Probation
```

`Probation`'s 10 probes and its finality Λ (170 DAA on testnet-12) are unchanged: a capped class still passes Probation once holders sit.
`Capped` admits at `capped_admission_permille` and **never** counts toward `panel_drawable`. Its claims are checked by the mesh, never
licensed by a class-local panel.

**Deriving the cap.** Let a capped class hold a fraction `w_c` of fork-choice weight and pwu, and `ρ_c` of rewards per DAA.

- **Unrecoverable gain.** An undetected lie gains at most its claim's reward plus its fork-choice influence. Rewards are unvested
  (below), so the reward part is recoverable whenever holders later detect the lie. What cannot be recovered is the fork-choice influence
  already spent.
- **The bound.** Require `E[unrecoverable gain per DAA] ≤ P(detect) × slashable bond per DAA`. Set `P(detect)` to the single-point rate,
  `m/N` — the mesh does not help there. Two caps follow:
  - **Weight cap.** Over all capped classes together, `w_c ≤ w_cap`, with `w_cap` (proposed 1 %) chosen so that the weight an adversary
    can mint with undetected lies cannot outrun the honest share: a fork-choice attack needs `> 1/2`, and `w_cap` keeps it far below.
  - **Grinding bound.** A single late-layer lie re-draws the ticket at ≈ `1/L` of an inference, so a capped class's tickets may be ground
    up to `L`× cheaper. The class's share of block production is capped at `w_cap`, so grinding gains at most `w_cap × L` times the
    honest rate within its own lane. The lane's per-class target retarget (ADR-0076) absorbs it within one retarget span.
- **The zero-weight hazard.** A lane whose blocks carry *no* PALW weight hands fork choice to the block hash for free: its blocks are
  ordered by the cheap lottery. A capped class therefore carries the ordinary pwu at the receipt ramp's first stage. Here the ramp
  matures on mesh `Audited` leaves instead of panel receipts, under ADR-0069's rule that weight needs adjudicability — every claim's
  leaves are court-checkable. The capped class gets **ρ_r's first stage and no later stage** until holders are seated.

**Provisional rewards.**

- Capped-mode rewards accrue unvested, in the vesting machinery the improvement protocol already uses (RFC-0004 §8.4).
- When holders are seated (`Capped → Probation`), they **re-verify a random subset — or all — of the capped claims inside the vesting
  window**. Each capped claim is a job the holders can replay; Part II sketching seats do it at decode speed.
- A re-verification that convicts slashes the producer and forfeits its unvested rewards.
- **Already-accepted blocks are not undone.** Fork choice moved, and the cap bounds by how much. Rewards are undone because they never
  vested. Fork-choice influence is not undone, which is why it is the capped quantity.

**Transition to full mode** happens when full coverage is seated. Full coverage means `required_ready_seats` holders of the class, each
of them one of: a full replica, a set of RFC-0006 layer shards covering every layer, or a Part II sketching seat with the class's
witness bytes inside its link budget.

**Time to first block.** At 150 s per DAA; prefetch at 1 Gbps.

| class | today: first block (first probe claim) | today: first non-probe block | staged: first capped block |
| --- | --- | --- | --- |
| Qwen2.5-1.5B (1.84 GB) | jury ≤ 100 DAA + prefetch 15 s + readiness ≤ 40 DAA ≈ 140 DAA (5.8 h) | + Probation Λ = 170 DAA ≈ 310 DAA (13 h) | jury ≤ 100 + DA certificate ≈ 10 DAA ≈ 110 DAA (4.6 h) |
| qwen4_exp-like (132–183 GB) | needs ≥ `seat_count` (5) ready seats, each holding 132–183 GB or a residency share with ≈ 1.45 s/token replay; prefetch 18–24 min a seat at 1 Gbps. **Possibly never on testnet-12** | — | ≈ 110 DAA (4.6 h). Holders follow: sketching seats need one 132–183 GB stream (18–24 min at 1 Gbps) and 2.3 GB resident |

For the t12 classes staging gains hours. For a qwen4_exp-sized class it is the difference between producing and not producing.

#### Part V — Constraint verification is the normal Panel path (2026-10-06)

##### V.1 Scope and the existing implementation boundary

For a claim using the new route, **the Panel MUST be able to discharge its assigned segment by verifying its constraints without replaying that segment end to end**. Direct batched Freivalds is the first check; other relations retain explicit deterministic reexecution and localization. Exact small operations may remain local. A full replay is useful for conformance, shadow comparison and small classes, but must not remain a hidden mandatory duty for the largest-model path.

Source audit at `7bff920dc`:

| Existing surface | Actual role and required extension |
| --- | --- |
| [`palw_verification_v2.rs`](../../consensus/core/src/palw_verification_v2.rs) | S1 partitions the job and assigns a full-replay seat plus partial seats, with coverage tally. Part V must version both the full-seat duty and coverage rule; changing a Panel command-line flag is insufficient. |
| [`PalwSeatReceiptV3`](../../consensus/core/src/palw_panel_v2.rs) | This name already means the V2 receipt plus a signed segment mask. Do not redefine it as the new scheme. `Sampled` is explicitly audit-only and does not count toward quorum/coverage. |
| [`palw_verification_profile_v1.rs`](../../consensus/core/src/palw_verification_profile_v1.rs) | A shadow timing/capacity profile, not a proof-suite registry or a soundness certificate. Keep resource scheduling distinct from the new cryptographic profile. |
| [`misaka-palw-tir-sketch`](../../misaka/palw/tir/sketch/src/lib.rs), [`Panel sketch integration`](../../kaspad/src/palw_panel/sketch.rs) | Private-sketch checker and integration provide the starting point. Their existence does not establish authenticated audit-history binding or the new receipt/tally. |
| [Spec 18](../spec/palw/18-verification-certificates.md) | Existing vertex, witness, audit and capped-mode wire rules. Reuse compatible machinery, reserve new tags/versions, and keep old claim interpretation unchanged. |

The attachment's name “Verification V3” is a conceptual label only; proposed names below are deliberately distinct from the already existing `PalwSeatReceiptV3`. Exact serialized fields/tags require a spec update and tests before activation. This RFC revision changes no Rust code or network parameters.

##### V.2 Profile, statement and constraint coverage

Propose a `PalwConstraintVerificationProfileV1` embedded by digest in RFC11's `VerificationPlanV1`. It binds:

* frozen program/artifact/tokenizer/input-output schema, full context, arithmetic and memory semantics, and constraint-compiler version;
* approved `verification_scheme_id` and version, field/modulus/range policy, repetition and aggregation rules, transcript/challenge construction;
* allowed scopes (`WholeClaim`, canonical `SegmentSet`, `AuditOnly`), the assignment and coverage policy, exact/nonlinear/lookup checker families and all boundary obligations;
* deterministic limits on evidence decoding, tensor/query/proof sizes, verifier work, localization, terminal court, DA retention and concurrent duties;
* the derivation of conditional soundness, including maximum relations/rounds/attempts, plus measured cold/warm timing and bandwidth inputs for the separate capacity profile.

This is a permissionless composition of reviewed primitive/verifier templates, not a hand-maintained allowlist of model brands. Qwen, Kimi/MoE and recurrent models may compose different templates, but cannot choose weaker security thresholds or omit a constraint. Unknown templates remain unsupported until versioned review/activation. Nodes recompute permitted parameters and coverage metadata; a producer-declared `soundness_bits` never establishes a bound.

Every execution-affecting relation has a checker: products, model-weight binding, quantization/rounding/carry, activation/normalization, TopK and expert selection, dynamic reads/writes, history and segment entry/exit, initial input and final output. TIR semantics stay exact. A correct `Y=XW` with fabricated `X`, substituted `W`, wrong expert or disconnected predecessor must fail the composed statement. Range/lookup and state-continuity checks cannot themselves be raw spot checks unless the suite includes their selection loss or a sound encoding/aggregation. Artifact-possession samples and Merkle membership prove neither computation nor complete state consistency.

##### V.3 Batch repeated work, not semantic context

Collect rows from multiple tokens/cells using the same weight root, layer/expert, dtype, shape and arithmetic policy into `X[b,k]`, `W[k,n]`, `Y[b,n]`. The canonical batch directory binds every row to `(claim, segment, global token, layer, operator, expert, row index)` before random coefficients are known. Sort keys, lengths and padding are specified; missing/duplicated/reordered rows cannot escape coverage accounting or create new PWU.

Two candidate checks, whose parameters belong to the suite:

```text
right projection:       X (W r) = Y r          r ∈ F_p^n
token-row aggregation: (aᵀ X) W = aᵀ Y         a ∈ F_p^b
```

For a fixed incorrect field product, either complete vector equality has miss probability at most `1/p` with a fresh uniform vector. Repeating independently gives at most `p^-t`. Do not reduce both sides to a single scalar and reuse that bound without a separate analysis. Do not multiply repetition counts from correlated rounds, seats or moduli; enough moduli to prevent integer aliasing do not automatically provide independent repetitions. Rounding and narrowing are separate constrained relations. The relevant source for these algebraic checks is [Slalom](https://arxiv.org/html/1806.03287v2); its trusted-hardware setup is not a MISAKA assumption.

This batches verification after rows exist; it does not parallelize autoregressive generation or erase sequential dependencies. Expert-routed rows are grouped per actual expert and the routing relation is verified. Require a bounded batching wait so a low-traffic class can still finish inside its window; when `b=1`, report the actual GEMV cost. No fixed 256-token minimum may stall all smaller jobs. Initial rollout batches **within one claim**; cross-claim batching needs a separate binding, deadline and exposure design.

Direct checks still read large operands: `O(bk+kn+bn)` work per repetition. Fresh public vectors require fresh projections or authenticated computation of them. A claimed `Wr` returned by a worker is not verified by its signature or a weight Merkle root alone. Charge a verified streaming pass, a reviewed evaluation-opening protocol or another sound check. Keep Part II's secret reusable sketches private and account for their distinct leakage, refresh and adaptive-probing assumptions. Do not obtain an apparent speedup by treating unverified projections as inputs.

##### V.4 Direct checking and explicit limits

Every checker reads the actual operand values authenticated to the bound model/claim. Direct Freivalds compares matrix-vector products; direct Add/Sub random-point evaluation compares the complete authenticated operands. Neither accepts a producer polynomial validity argument. Mul, ReduceSum, quantization, nonlinear operations, routing, state and pipeline boundaries use the registered deterministic native checker and objective court.

A checker reports its actual reads, operations, retained material and conditional detection error. Sampling a few positions covers those positions only. Full-scope duties may be expensive; no circuit proof or cryptographic argument is an allowed substitute. Where resource limits make a duty infeasible, refuse that profile explicitly and retain its coverage gap instead of claiming whole-model assurance.

##### V.5 Commitments and challenge ordering

Before deriving check positions or coefficients, bind the claim, class/profile/suite, initial/final and segment boundary states, trace/constraint/batch directory, complete witness commitments and artifact/input roots. Evidence openings must refer to those exact roots. Private-sketch baseline witnesses without a commitment cannot be adopted as the new public-challenge protocol.

Resolve `PostCommitChallengePolicyV1` and derive `sample_seed` exclusively under [Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol), from its future qualifying PALW work and the bound statement. Node validation recomputes its source selection, lock, seed and queries; neither seats nor registrants choose seeds or counts. Panel assignment and generation randomness have separate domains and security contracts and cannot replace this source.

Every sampled statement and audit-material root is fixed before its paid future challenge. Producer-supplied validity transcripts are prohibited. All nodes replay the fixed transcript/source facts, never freshly draw local consensus randomness. Part VI bounds identity changes, aborts, retries, reorgs and adaptive seed/audit-history queries; a new branch or carrier grants no free fresh draw.

##### V.6 Typed receipts and a coverage-aware tally

Propose `PalwConstraintReceiptV1` (conceptual schema; tag allocation pending):

```text
claim_id, class_id, verification_profile_hash, verification_scheme_id, scheme_version
assignment_root, scope_kind, scope_root, covered_relation_count
challenge_policy_id, commitment_root, challenge_anchor, beacon_evidence_root
sample_seed, sample_count, sampled_cells_root
field_policy_id, freivalds_rounds, soundness_policy_id, derived_soundness_bits
evidence_manifest_root, algebraic_check_root, transition_check_root, transcript_root
verdict, seat_bond, signed_daa, signature
```

`sample_count` measures the suite's queries, not an arbitrary number of tokens. `scope_root` commits to the full statement attested; `sampled_cells_root` commits to the actual query set, which may be much smaller for direct algebraic sampling. The plan fixes which fields are meaningful for each suite, with canonical encodings for unused fields. Round/field/security values must match derived policy; metadata is not evidence that a check actually ran and cannot replace the honest-verifier assumption.

Sign all fields with a fresh network-separated ML-DSA domain; Part I-style batching may carry the same signed meaning in a new versioned leaf. The network/version/ruleset must be bound by the message/domain. Do not append fields to existing receipt encodings, repurpose `Sampled` or tally a V1 bare `Valid` as a new scheme's pass. Bind the evidence locator to content hashes; retain material through challenges and ongoing disputes. Root-only carriage is acceptable only with separately specified availability duties, not as evidence that the checker ran.

The fold checks registration and scheme eligibility, signatures, assigned scope, anchor/seed, required counts/policy, caps and deadlines. It deduplicates by claim/seat/duty under the frozen assignment; duplicate or overlapping receipts from one seat never increase coverage. It applies the plan's **per-segment/relation** quorum/independence requirements and boundary coverage as well as any overall Panel quorum. Three receipts for the same small segment cannot license the remainder. A `WholeClaim` receipt counts as full coverage only when that suite actually attests the complete statement. A failed receipt followed by a same-duty pass cannot erase a dispute; tally and appeal follow explicit first-counted/equivocation rules.

The first implementation SHOULD preserve existing segment coverage strength while replacing replay with cheaper constraint verification: the assigned full-scope seat checks the whole statement algebraically, and partial seats check their assigned scopes. This removes its full-**replay** duty without removing its full-**scope** obligation. A later partition-only assignment requires measured equivalent coverage and explicit honest-verifier/independence assumptions under a separately pinned policy. Cross-segment state equality must have designated checkers or aggregate coverage; a merely committed but unchecked boundary is insufficient.

Nodes perform these deterministic structural and lifecycle checks; Panels run the heavy verification suite off chain. Publicly replayable evidence permits audits but its presence does not mean every node validated its arithmetic. Thus a malicious accepting Panel remains a security event. Node-verified computation-validity proofs are permanently prohibited by PRINCIPLES §8. Nodes authenticate the selected verifier approvals and exact receipt scope; direct checker reads, repeated arithmetic and objective court work are metered separately and cannot be replaced by a succinct proof. Consensus credit authentication depends on the prescribed randomly selected validators' approvals under an explicit honest-fraction assumption.

##### V.7 Error accounting, licence, Final and dispute

For `N` raw segments and `s` distinct uniform samples, a single bad segment is missed with probability `1-s/N`. If selected segments have conditional error `ε_local`, the single-fault miss bound is `1-(s/N)(1-ε_local)`, under the stated selection/check assumptions. Freivalds does not remove this selection term. Therefore an `AuditOnly` receipt cannot become full-security coverage, regardless of its small `ε_local`.

For the new qualifying route, derive the **conditional direct-check** `ε_check` from every checked relation, field representation, nonlinear/memory constraints, boundary binding, openings and any selection loss. Legacy numerical check targets are conditional evaluation parameters, not whole-model cryptographic guarantees. Report the actual detected scope and honest-verifier assumptions; numbers remain subject to consolidated validation and the latest user decision. A conservative composition sums error bounds unless a stronger theorem applies. The general `ε ≤ Σ_i ε_i` union bound needs no independence, whereas multiplying repeated-check errors does. Shared public vectors across seats do not create independent repetitions.

Publish separate compromised-Panel, biased-beacon, unavailable-data/censorship and grinding assumptions. Across `Q` adaptive attempts the conditional check contribution can grow to `Q·ε_check`; add the appropriately modeled bad-assumption probability instead of advertising network-wide 128-bit security. An `f^5` estimate is only valid under the actual independent drawing, duty and timely-challenge assumptions, not merely because the Panel has five seats. Economic audits/PoSP incentives complement this analysis; they do not replace it.

```text
committed evidence → bound challenge → constraint checks → positive scoped receipts
                                                        → coverage/quorum → licensed
                                                        → challenge window + DA → Final
                    mismatch → localized dispute → exact terminal verdict
                    missing evidence / no quorum → existing compatible DA/timeout path
```

No receipt silence, absent proof round, unchecked relation or missed DA duty counts as positive evidence. Licence is not immediate Final. Open accepted disputes block finalization under the versioned lifecycle. Bind pre-Final weight/escrow exposure to the existing compatible accounting and RFC11/RFC8 requirements; extra checks, batches and receipts mint no extra inference work. Retain evidence until the last relevant dispute/retention deadline, including across reorg/pruning.

A failing checker files no passing receipt. It names the bound failing relation/evidence; a correct claim with a bad served witness is not automatically producer fraud. Localize through authenticated batch partitions, matrix tiles, state steps or approved circuit relations to an exact **TIR/kernel terminal** supported by court, never a VM step. Bound **total** localization work, bytes, moves and concurrency, not just the last operation. A failing probabilistic check does not itself locate a leaf: each checker needs a tested deterministic localization path. Unknown/incompatible terminal semantics require a versioned kernel/court extension. No fallback may require complete Kimi-class inference on a seat or node.

Anyone with the required bond/material can challenge; exact court conviction, dismissal and DA/timeout outcomes keep their distinct evidence rules. Missing material is handled as availability; slashing requires the corresponding proven violation. Court catches filed disputes, leaving residual undetected error after Final. This proposal adds no automatic post-final rollback.

##### V.8 Delivery plan and acceptance measurements

| Stage | Work | Passing evidence |
| --- | --- | --- |
| P0 — one TIR MatMul | Compare independent exact execution, Part II private sketch and fresh committed batched Freivalds. Include batch 1, uneven sizes and multiple-token batches, cold/warm weights and field/range cases. | Same deterministic outputs; single-scalar/alias/substitution faults covered; toy-field experiments agree with the error model. Report CPU/GPU time, preprocessing, bytes, RSS/VRAM and amortization. No claimed 10–100× gain without measurements. |
| P1 — complete segment | Cover nonlinear, routing, quantization, memory and boundary relations; add authenticated batch/evidence directory and bounded failure localization. | A valid segment passes without routine whole-segment replay. Forged predecessor/input/expert and disconnected-but-locally-correct matrices fail; worst dispute fits budgets. |
| P2 — receipts and claim coverage | New profile, challenge schedule, receipt/vertex version, scope-aware tally, DA retention, licence/Final and state/RPC reporting. | Partial/duplicate/foreign-scheme receipts cannot license a whole claim; no hidden full replay; old claims retain old meaning; cold IBD, reorg and pruning reproduce decisions. |
| P3 — complete direct/exact coverage | Implement the native direct checks, deterministic relations and all boundary checks with explicit resources. | Sparse and boundary faults remain covered; adaptive draw tests, honest-verifier/DA assumptions, actual checker cost and bounded objective localization. No computation validity argument is permitted. |
| P4 — large models and activation | 9B-8k, validated 2M and source-pinned Kimi K3 cases; shadow comparisons followed by separate rollout decision. | Full task/context, qualified seats, measured cost/DA/court budgets, reviewed error and Panel assumptions, claim through Final. Missing implementation or benchmarks remain explicit blockers. |

Measure prefill and autoregressive decode separately, including MoE batch fragmentation and recurrent state. Report producer execution plus witness/proof generation, seat p50/p95/p99, verification service capacity, network transfer, cold preprocessing and worst-case attack-driven disputes. Compare equivalent security/coverage and the same hardware, not a GPU miner against an unrelated CPU baseline. The new mode must fit a predeclared resource/window budget and demonstrate its claimed advantage on its target workload. Extra witness/proof work can cancel arithmetic savings. Stop promotion for a workload if it does not meet its budget; adjust the scheme or retain its compatible replay path without claiming acceleration.

##### V.9 Activation and required spec changes

Use RFC11/ADR-0171's proposed `palw_probabilistic_constraints_v1` fence for the new route, with **no height selected here**. It is distinct from existing RFC7 vertex/witness/audit/capped fences. Profile and scheme versions enter class/claim identity, signature domains, evidence/receipt handling, replayable state and fingerprints as applicable. Pin the claim's rules at binding so a mid-window fence cannot reinterpret its duties. Mixed versions need explicit compatibility; a new receipt never silently upgrades an old sampled audit.

Update spec 07/08/18 and the relevant class/claim, Panel assignment, receipt/vertex pool, fold, DA, RPC and SDK paths together. Terminal court changes only when the chosen suite cannot localize to an existing supported step. Reserve wire tags centrally, append rather than renumber enum variants, charge bytes and node work before allocation, and cap pending transcript/dispute state. Recalculate Part I's carriage table with the new fields, roots, evidence availability and signatures; its legacy 66–330-byte estimate is not the new receipt cost.

Normative requirements for the new route:

* **PALW-CV-1:** an eligible immutable profile defines complete statement coverage, allowed suites and derived security/resource limits.
* **PALW-CV-2:** evidence and each interactive message are bound before their corresponding challenges; seeds and queries are reproducible and policy-valid.
* **PALW-CV-3:** all constraints in a receipt's declared scope pass the suite, and the signed receipt binds that exact scope, profile and evidence. `AuditOnly` never satisfies missing licence coverage.
* **PALW-CV-4:** licence requires scope-aware coverage and the specified quorum/independence; Final additionally requires the challenge/DA conditions and no unresolved accepted dispute.
* **PALW-CV-5:** failure escalates within bounded total resources to compatible exact court or the proper DA/timeout outcome; a failed probabilistic test alone is not a slashing verdict.
* **PALW-CV-6:** old receipt/claim semantics, unique work accounting, replay determinism and the explicit error/Panel assumptions are preserved by the migration. Unimplemented schemes stay ineligible.

These are proposed requirements. Implementations must attach a test/evidence artifact to each before activation. The execution blueprint follows [RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md#15-sampling-first--court-on-dispute-2026-10-06-decision) and [ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md); neither the sketches in Part II nor this text proves the new protocol complete.

<a id="post-commit-challenge-protocol"></a>

#### Part VI — Post-commit challenge protocol: the single source of truth (2026-10-08)

##### VI.1 Authority, scope and security boundary

This part is the sole proposed protocol for challenge randomness shared by Kernel differential conformance,
model onboarding, per-claim verification, EXEC work slices and public verification/prosecution. RFC02/05/11/13/14/15
bind or use it; they must not invent alternative beacon/seed formulas. Its names are proposed schemas, not current
Rust types, wire ids or activated rules. Existing claims, ticket draws and Panel assignments retain their old rules.

The beacon selects unpredictable checks of an already fixed statement. It does not approve a model, define an
unknown operator, prove constraint completeness, attest availability or authorize a Kernel upgrade. Require, in order:
semantic completeness, constraint coverage, exact-court completeness, public-material completeness, resource bounds,
then this challenge protocol and probabilistic checking. G14 remains conjunctive with all those obligations.

```text
VerificationPlanV1             = what is checked
PostCommitChallengePolicyV1     = how/when challenges are derived
PalwDisputePlanV1 (RFC14)       = public material and exact failure prosecution
```

These are bound together by the new class/profile. `PalwDisputePlanV1` is the public-prosecution plan; do not introduce
a second independent `PublicProsecutionPlan` codec. No BFT committee, validator ballot, threshold signature, DNS-final
or external attestation issues/locks a beacon. Nodes derive it from their PALW canonical history.

##### VI.2 Immutable policy and committed subject

```text
PostCommitChallengePolicyV1 {
    version, randomness_source_policy_id,
    anchor_delay_slots, work_count_k, source_eligibility_policy_id,
    anchor_settlement_policy_id, settlement_depth_D,
    binding_schema_id, hash_suite_id, seed_hash_domain,
    sampling_algorithm_id, field_sampling_algorithm_id, field_policy_id,
    repetition_policy_id, repetition_count, soundness_policy_id, security_bits,
    grinding_budget_policy_id, retry_limit, abort_policy_id, reorg_policy_id,
    interactive_mode, transcript_transform_id,
    resource_schedule_id, retention_policy_id
}
```

`challenge_policy_id` is the digest of the complete canonical descriptor. Checker suite, challenge policy and
soundness policy form one approved tuple; reject unknown/inconsistent ids or registrant-selected weaker repetitions.
The exact canonical codec/hash suite, integer widths, tagged optional fields and resource ceilings must be pinned in
the release; missing values are an activation blocker, not node-local defaults. No k, D, delay or retry value is
selected by this document. Changing a policy creates a new identity/version, never changes an in-flight subject.
*(2026-10-15: k, D, delay and retry are named parameters marked USER DECISION REQUIRED in the spec-closure section, R7-SOURCE.)*

`subject_kind` is one of `KERNEL_CONFORMANCE`, `MODEL_CONFORMANCE`, `CLAIM_VERIFICATION`, `WORK_SLICE`,
`PUBLIC_PROSECUTION`. Before challenge collection, bind its immutable subject id and roots: Kernel descriptor,
VerificationPlan, program/artifact/tokenizer/input schema, layout, input, initial/final/boundary state, all constraints,
evidence/oracle/batch directory, conformance scope, implementation set and the commitment object. A scope-specific
binding grammar uses explicit typed absence where a root is inapplicable, never an ambiguous zero wildcard.

Artifact/layout/plan/implementation/scope changes require a new pre-beacon commitment and a newly eligible future
window. Preserve the failed/aborted attempt in the retry ledger and resource accounting. Changing a candidate id,
job nonce, signature or carrier does not reset the allowed grinding budget.

##### VI.3 PALW Work Beacon source eligibility and non-circularity

The selected source direction is `PalwWorkBeaconV1`: aggregate k distinct future qualifying useful-work commitments.
An eligible source must be an independently valid useful-work claim of a pre-existing Active Kernel/model profile
whose complete verification plan, G14 public prosecution and required public DA/retention are established. Freeze
eligible source profiles from the subject's commitment state, check their applicable status/obligations at contribution,
and exclude the model/Kernel candidate under test and all work depending on its proposed semantics.

G14 is a release-completion criterion, not a registrant-provided `complete=true` or committee signature. The active
versioned eligibility rule must derive qualifying capability/class/claim/DA facts from authenticated state; runtime
validation never asks a maintainer to approve a source or fetches an off-chain test report as consensus authority.
The release review proves that the source profile rules implement the G14 and independent-validity requirements.

Sources must have their canonical work commitment accepted/sealed after the subject's commitment and the policy's
future start slot S, then reach claim Final and the policy's PALW-native settlement condition. A claim computed or
committed earlier that merely reaches Final after S is not a fresh source. Future inclusion alone does not prove
physical execution time or entropy; the source profile must demonstrate unpredictable execution inputs/results
under the stated adversary and that an alternative valid entropy sample requires additional paid useful computation.

Eligibility requires unique canonical work identity and independence of validity from the challenge consuming it.
The source/challenge dependency graph must be acyclic: the target claim cannot seed its own verification, two pending
claims cannot mutually certify each other, and a candidate cannot generate its own conformance randomness. Existing
independently verified source profiles/bootstrap evidence need a separate eligibility review; absence of such a
profile leaves this route pending, without a trusted genesis-operator exception or self-certification loop.

Exclude heartbeat, BASE-0 fallback, receipt-only/signature-only material, EXEC_TX/round carriers, provisional or merely
header-valid attempts, unverified slices and bare Panel receipts. A long claim may contribute at most once through
its eligible, independently Final root useful-work identity; its N EXEC carriers never become N samples. EXEC
weight/DAA stays zero. Finishing a slice does not make it an entropy source.

Do not use block hash, timestamp, nonce, signature, freely rewrappable claim id or settlement-anchor hash as entropy.
`source_ref`/settlement position are provenance and deterministic-order facts, not fresh randomness. Canonical
work identity and execution commitment must be stable under header/carrier reattachment; if a profile cannot prove
that changing entropy requires new useful computation, it is ineligible. Correctness/Final/G14 alone is not an entropy
theorem. Cheap unadmitted attempts and precomputed public deterministic outputs remain explicit source threats.

##### VI.4 Canonical collection, accumulator and lock

Derive S from the accepted commitment position and the policy's delay under the existing chain clock. Do not change
REAL/heartbeat/BASE-0 cadence to obtain randomness. On the canonical PALW selected history, enumerate eligible Final
events in `(settlement_position, occurrence_index, canonical_work_id)` order and use the first k distinct work identities.
Positions are chain-derived logical positions, not local arrival times, RPC ordering or producer-selected lists.
Duplicate/reincluded/reattached work cannot enlarge the set. Pin treatment of simultaneous events and dependency order.

Under the policy's canonical hash/encoding suite, define only here:

```text
entropy_item_i = H("MISAKA/PALW/WORK-BEACON/ITEM/V1",
                   source_profile_id, canonical_work_id_i, execution_commitment_i)
acc_0 = H("MISAKA/PALW/WORK-BEACON/V1",
          chain_genesis, ruleset_id, challenge_policy_id, commitment_root, challenge_epoch)
acc_i = H("MISAKA/PALW/WORK-BEACON/MIX/V1", acc_(i-1), i, entropy_item_i)
beacon_output = acc_k
```

The public evidence records each source reference, eligibility/Final/settlement proof, immutable work identity,
order, accumulator and policy. `anchor_settlement_policy_id` uses reviewed PALW-native settlement evidence/depth D,
never DNS/BFT finality. A raw count of heartbeat/EXEC blocks cannot manufacture useful-work settlement confidence.
`BeaconLocked` is a branch-relative protocol state after that predicate, not absolute finality or a new reorg veto.

Mixing several work items does not automatically yield unbiased randomness. One honest unpredictable contribution
helps only under the reviewed commitment/selection/withholding assumptions; an adaptive last contributor, producer
collusion, reordering, censorship, reorg and selective publication can bias a simple accumulator. Bound that bias,
grinding cost and adversarial retry exposure explicitly. Hashing more known work is not evidence of new entropy.
Claim Final is a lifecycle result with its applicable residual-check/Panel/DA assumptions, not certainty that a
source is correct. Compose source false-acceptance and correlated failure/bias into the environment/network error
model alongside downstream adaptive trials; a consumer's conditional checker epsilon does not cover those events.
VDF is not part of v1; a later VDF cannot repair an invalid or biased entropy source by itself.

##### VI.5 One seed derivation and domain-separated sampling

After the statement commitment and BeaconLocked predicate, define only here:

```text
challenge_seed = H("MISAKA/PALW/CHALLENGE/V1",
    chain_genesis, ruleset_id, challenge_policy_id,
    subject_kind, subject_id, kernel_id, verification_plan_root,
    program_root, artifact_root, tokenizer_or_schema_root, layout_root,
    input_root, state_root, constraint_root, commitment_root,
    challenge_anchor, beacon_output)
```

`challenge_anchor` is the normalized policy-bound window/epoch and stable ordered work-identity descriptor,
not a producer-chosen block hash. Header hashes/signatures and mutable provenance wrappers used to prove inclusion
or settlement remain outside seed-bearing bytes: rewrapping the same work or advancing a heartbeat cannot reroll
the challenge. Any seed-bearing anchor field must be fixed before the source randomness or change only through
new qualifying work/a recorded canonical reorg under the reviewed bias model.
The binding schema covers all additional subject-specific roots inside `commitment_root`. Resolve and validate
every root/anchor from authenticated public history; cached seeds or exporter metadata are not authoritative.

Derived streams have distinct domains for `query`, `segment`, `tensor-range`, `vector`, `freivalds`, `aggregation`
and `proof-round`, with subject/scope/relation, repetition and coordinate indices. Panel draw/ticket/generation R
have separate domains and contracts. Replaying a claim's original checks uses its original subject kind and seed;
`PUBLIC_PROSECUTION` is only for explicitly committed supplemental checks, not an alternate favorable reroll or a
prerequisite for filing an already authenticated exact fraud proof.

The policy fixes unbiased field and index sampling, no-replacement/query weighting and rejection behavior. Pin a
reviewed rejection sampler/expansion with golden vectors; modulo reduction without a proven uniform mapping is
not sufficient. Enforce bounded work and named exhaustion failure rather than biased fallback coefficients.
Uniform field vectors, repetitions and whole-statement composition must match the suite's reviewed soundness.
Substreams are computational derivations under their stated assumptions; N watchers of the same seed are not
N independent repetitions. Raw sampling of a few rows does not prove unchecked semantic/constraint families.

##### VI.6 Direct probabilistic checks and repeated audit rounds

Freivalds fixes the matrix statement before the source window, then derives indexed repetition vectors from the paid seed. Authenticate every read operand and include integer/field equivalence, relation and boundary coverage, repeated attempts and query-selection losses. Add/Sub direct random-point checks likewise read the operands and carry no producer validity messages. Other native relations are recomputed deterministically.

Repeated audit rounds may bind the previous signed material/check record and use a separately paid future source. Any retained `TranscriptBoundFiatShamir` legacy name identifies the canonical audit-history/seed transformation only; it cannot authorize a sum-check, polynomial commitment or other computation-validity argument. Its hash authenticates identity/history and derives randomness. It does not prove calculation. Missing rounds or exhausted streams cannot create passing receipts. No source label or review tuple can override the permanent ban.

The detection probability is conditional on honest capable selected verifiers performing these checks. Report their assumed fraction and stake/weight, correlated draws, actual participation, material acquisition and submission deadline. Never derive those participation assumptions from field size or a receipt signature. The applicable bond caps bound residual damage.

##### VI.7 Reorg, abort, scarcity and deterministic recovery

`Committed → BeaconCollecting → BeaconCandidate → BeaconLocked → Checked` is a semantic lifecycle, not a current enum.
Before lock, recompute the source list/accumulator under canonical reorg. After lock, a reorg removing commitment,
source Final/settlement or lock evidence rolls back all dependent challenge/conformance/activation/settlement state
with that branch. Do not preserve a stale seed or silently rewrite an already accepted result. Once changes are
canonical again, retry according to the same policy and bounded ledger. Lock neither forbids valid reorg nor adds
DNS/BFT finality. Final/post-Final liability follows the applicable lifecycle; this protocol adds no unbounded rollback.

Each changed commitment, unpublished attempt, abort, reattachment, alternate source window, transcript query and
permitted retry contributes to the stated grinding/exposure bound. Record enforceable on-chain limits and separately
analyze unobservable off-chain trials; a fee or retry ledger alone cannot bound all adversarial precomputation.
An honest timeout/reorg is not equivocation evidence. Slashing needs the actual proof/obligation rule.

With too little qualifying work, return `BEACON_UNAVAILABLE` / pending. Models can remain `RegisteredDormant`;
claims obey their explicit pending/deadline/DA rules without a false pass. Never fall back to heartbeat/BASE-0 hash,
EXEC hash, committee signature, local RNG or caller-chosen seed after a timeout. Chain, transactions, heartbeat and
BASE-0 continue under existing liveness; reserve their validation/relay resources even during collecting/backlog.
Cold/restarted/IBD/pruned nodes reconstruct the same policy, commitment, source facts, seed, queries and transcript.
Pruning cannot discard the public provenance/material needed before its last retention/liability deadline.

##### VI.8 Activation and evidence gates

All gates remain open; this text performs no new beacon, conformance run or soundness proof. A separate coordinated
version/fingerprint release must supply:

1. Complete policy/codec/hash/domain/sampling/transform vectors; per-field mutations and unknown/version mismatch refusal.
2. Independent source-cost, unpredictability, bootstrap/non-circularity, bias/last-contributor/withholding/reorg and
   adaptive-grinding analysis. k/D/delay selection and measured completion latency; no inference-cost-only entropy claim.
3. Candidate/kernel/source substitution, precommit/prefinal/cheap-header exclusion, duplicated root/slice/carrier,
   abort/retry and changed artifact/plan/layout tests; bad sources cannot activate a candidate or settle work.
4. Fixed-statement direct Freivalds and repeated audit/source ordering tests, soundness composition, failed-check-to-bounded
   exact court, DA/default distinction and cold outsider replay without producer-private state.
5. RFC11's three onboarding stages, RFC13's resumable commitment/evidence records, RFC14/G14 and RFC15's separate gates;
   no semantic admission or Panel=0 inferred from conformance alone.
6. Restart/IBD/pruning/reorg/activation-boundary equality and useful-work scarcity/flood tests showing unchanged main
   heartbeat/BASE-0/clock/EXEC_TX liveness. Existing ticket/Panel seed rules and old identities stay frozen for old subjects.

#### Relations

- **RFC-0006 (layer-sharded panels, lane M3).** Shards are the bandwidth-efficient way to use the global seat set for full coverage. A
  shard seat holds the weights of its layers and recomputes them, and the boundary rows are committed and opened; nothing is
  transferred per position beyond those rows. Part II and RFC-0006 are complementary:
  - **Shards** suit prefill-heavy jobs and classes whose seats can hold a layer range. No witness is needed.
  - **Sketching seats** suit decode of classes no seat can hold. No weights are held, but the witness crosses the link.
  - **Together.** A shard seat may itself check its layers algebraically, keeping its layers' sketches (1 % of them) and taking its
    layers' witness. RFC-0006's stratified panel then counts a sketching seat for a shard exactly as a holder.
- **Lane M2's residency (ADR-0112 for IR classes).** The sketch builder reads weights through the residency's row source
  (`TirRowSourceV1`), in row order, holding a row at a time. Under a residency, the RAM-versus-NVMe comparison of §II.9 is the seat's
  actual choice. A seat whose budget is a fifth of the artifact is in the NVMe column for most of a large class.
- **RFC-0002.** The witness is defined by the program alone. The moduli come from the admission range analysis, refined. The exact half
  of the check is the reference evaluator, and the audit is the demand evaluator. Nothing in this RFC changes a primitive, a commit point,
  a cone or the court.
- **RFC-0002's staged registration and mining enablement.** IV.2's `Capped` state is the middle stage between "registered and
  inspectable" (`Candidate`) and "panel-verified" (`Probation` onward). It is enabled by the same seat-readiness facts, with the mesh
  where today the class-local panel is required.

#### Proposed Spec text (new chapter `spec/palw/18-verification-certificates.md`)

**Historical Parts I–IV text:** the chapter now exists; its current wire/activation rules govern deployed behavior. Rules below describe the original proposal and are retained for context, not an override of that chapter. Part V adds PALW-CV-1…6 under its own future fence. In this older path, rules marked *(node)* are seat-local software; that does not exempt Part V's profile, receipt or coverage obligations from consensus.

- **PALW-VC-1 (one vertex per seat-round).** Past `palw_verification_vertex_v1`, the fold MUST accept at most one
  `VerificationVertexV1` per `(seat_bond, round)`, the first in accepted order, and MUST refuse a vertex whose leaves are not strictly
  ascending, whose count or size exceeds the caps, or whose `leaves_root` does not recompute.
- **PALW-VC-2 (the signature).** A vertex MUST verify under its seat bond's registered ML-DSA-87 key, over `vertex_message_v1`, with
  context `misaka-palw/verification-vertex/mldsa87/v1`.
- **PALW-VC-3 (the tally).** A `Verdict` leaf MUST count toward its claim only under the conditions `validate_receipt_quorum_v2` applies
  to a receipt. A claim whose counted `Valid` leaves reach the panel quorum MUST transition as `ReceiptLicensed` would, in the block that
  completes the quorum.
- **PALW-VC-4 (a verdict stands).** A seat's first counted verdict for a claim MUST NOT be replaced by a later leaf of the same seat.
- **PALW-VC-5 (equivocation).** Two vertices of one `(seat_bond, round)` with different roots, both validly signed, MUST be accepted as
  `VertexEquivocation` evidence. The fold MUST slash, forfeit locks and eject as §I.6 states.
- **PALW-VC-6 (DA attestations).** A `Held` leaf MUST bind its signer to serve the named chunks until the claim's challenge window
  closes. An ADR-0111 availability request MAY name an attester, and an attester that fails it MUST be slashed its `Held` exposure.
- **PALW-AV-1 (the witness)** *(node)*. A producer SHOULD serve, on request during the receipt window, the output of every `MatMul` of the
  canonical serving set at every position of a claim, with the claim's committed rows.
- **PALW-AV-2 (secrecy)** *(node)*. A seat MUST NOT disclose its sketch secret, its keys or its sketches.
- **PALW-AV-3 (moduli)** *(node)*. A seat checking a node algebraically MUST use moduli whose product exceeds the span of the node's
  refined proven interval, and MUST refuse a served value outside that interval.
- **PALW-AV-4 (failure is not a verdict)** *(node)*. A seat whose check fails MUST NOT file `Valid` for the claim. It escalates by §II.8.
- **PALW-AM-1 (audit assignment).** Past `palw_audit_mesh_v1`, audit assignments MUST be drawn from the anchor beacon over all bonded
  seats, by eligibility weight, after the claim commits.
- **PALW-AM-2 (positive attestation).** Only an `Audited` leaf counts as an audit. Absence MUST NOT count as a match.
- **PALW-AM-3 (traps).** A `TrapRevealed` that opens a prior `TrapCommitted` MUST void the trap claim without slashing its setter, MUST
  slash every auditor whose `Audited` leaf attested a match at the planted leaf, and MUST pay those who attested a mismatch.
- **PALW-AM-4 (capped mode).** Past `palw_capped_onboarding_v1`, a class in `Capped` MUST admit at most `capped_admission_permille`. All
  capped classes together MUST hold at most `w_cap` of fork-choice weight. Their claims' rewards MUST NOT vest before the class leaves
  `Capped` and the vesting window's re-verification closes.

#### Activation plan

**Legacy activation record:** the four-fence plan below was drafted before integration. Current spec 18 records the t12 DAA-5,300 activation; the old table is not a claim that those fences remain dormant today. New Part V uses §V.9's separate proposed fence, with no height. Preserve the existing **Some-only** fingerprint discipline for a new field: a preset leaving it `None` must retain its prior fingerprint. Integration includes all four places:

1. the `Params` field;
2. `for_each_fence`;
3. the Some-only fingerprint write;
4. the `never()` → `None` collapse in `normalize_values_a_scheduled_fence_drags_with_it`.

Each goes at an **independent height**: a fence at an already-scheduled height is invisible to the fork id.

| fence | what it arms | depends on | drill (on the shipping binary, crossing the fence) |
| --- | --- | --- | --- |
| `palw_verification_vertex_v1` | vertex objects, tally licensing, equivocation evidence, `Held` leaves; receipts accepted for one receipt window past it | — | a salted t12 drill chain. Licences by receipts below the fence and by tally above it; a claim bound across the fence licenses on the old path; an equivocating seat slashed; carriage per licence measured at the drill's issuance ×10 |
| `palw_witness_manifest_v1` | the witness root in the trace manifest, so that `Unavailable` names witness chunks | — | a producer withholding one witness chunk: seats file `Unavailable` naming it, and the claim voids at `ReceiptTimeout` with nobody slashed |
| `palw_audit_mesh_v1` | the audit draw, `Audited` leaves, `TrapCommitted` / `TrapRevealed`, audit pay | `palw_verification_vertex_v1` | a fabricating producer at `q` = 0.1 caught within the predicted number of audits; a lazy auditor slashed by a planted trap; the trap setter not slashed |
| `palw_capped_onboarding_v1` | the `Capped` lifecycle state, the caps, provisional rewards, the re-verification window | `palw_audit_mesh_v1` | a new class registered past the fence produces capped blocks under mesh audits; holders seated; one capped claim forged and convicted in re-verification with its unvested reward forfeited; the weight cap never exceeded |

Part II's original local checker needs no new receipt-policy fence: it is a node release. The original rollout called for a node flag, default off, with the soundness suite and a mirror
check — the checker run beside a full replay on every claim a seat already replays, with any disagreement logged and alarmed — before any
seat relies on it. That rollout does not activate Part V: its new receipts, duties and tally require the separate fence and shadow gates in §V.8–9.

#### Alternatives

| alternative | why not |
| --- | --- |
| Aggregate ML-DSA-87 signatures | They do not aggregate (ADR-0098 §3). A vertex amortises one signature over a round instead |
| Root-only vertices plus per-claim licences carrying Merkle proofs | ≈ 2.3 KB a claim with `Hash64` paths: 6× better than today, and a licence object, a collector and an ordering still exist. Carrying the leaves is 44–218× better and removes the collector |
| A Narwhal/Bullshark DAG of vertices with its own ordering | GHOSTDAG already orders, and the chain carries every vertex, which is availability. Parent edges would certify nothing new |
| Weight from cluster agreement ("the largest compatible cluster is correct") | Refused by the user's decision. A vote over executions is a vote on who holds the most seats; 1-of-N with the exact court is the model |
| Serve `Q·Kᵀ`'s scores | 8 bytes a score against `d` = 128 MACs saved; never pays below ≈ 10 Gbps (measurements §5) |
| A failed algebraic check as an immediate conviction | A served witness may be wrong even for a correct claim; localize and use exact court. Public vectors are unsafe if known before the claimed result is fixed; Part V defines post-commit public checks separately from Part II's secret reusable sketches |
| Direct Freivalds and exact native checks | Selected only for their declared operand/relation scope, under explicit honest-verifier assumptions; deterministic court remains mandatory |
| Cryptographic computation-validity proofs | Permanently prohibited by PRINCIPLES §8, independently of cost |
| Sample more intervals instead | 90 % catch of a one-point lie costs each seat 37 % of the job (ADR-0098), and grinding makes the uncaught 10 % valuable (ADR-0072) |
| Mesh audits as the security floor | `m/N` against single-point lies: 64 audits catch one lied tile of a 512-position job with probability ≤ 0.04 % |

#### Security and economic analysis (summary)

- **Part I.**
  - Equivocation is slashed by two signatures alone.
  - A verdict once counted stands.
  - Compact references can be ambiguous only after a 2^64 birthday search, and an ambiguous leaf is ignored.
  - The tally applies exactly the per-receipt conditions of today.
  - The fold's signature work falls from 3 per claim to 1 per seat-round, which also removes the gossip door's receipt-flood surface (the
    2026-09-24 pool flush).
- **Part II.**
  - Sound with probability `1 − 1/p` per dishonest node, provided `v` is secret. Sketches are as secret as `v`.
  - A forced single modulus is a demonstrated hole, closed by the ladder.
  - A failed check convicts no one, so the seat escalates to the exact court.
  - A producer gains nothing by serving a bad witness: it loses its licence.
  - Seats are heterogeneous by design, and the check is local.
- **Part III.** The historical independent-checker example gives `f^5` for full coverage and ≈ 95 % escape for raw interval sampling at `f = 0.2`. Part V requires actual coverage/tally and assumption analysis; these numbers are not its whole-claim soundness bound.
- **Part IV.**
  - The mesh catches fabrication at `1 − (1 − q)^m` and single-point lies at `m/N`. It is a sensor.
  - Traps price lazy auditing.
  - Capped mode bounds the unrecoverable part of a lie — fork-choice influence — by `w_cap`, and makes the recoverable part, rewards,
    revocable.

#### Historical open questions (Parts I–IV)

These were the original draft's questions. Spec 18 §§18.0/18.20 records subsequent implementation decisions; do not reopen settled wire/policy choices merely because this list is retained. Part V's remaining decisions follow this list.

1. **`round_daa` and the leaf caps.** 1 DAA (today's licence latency) or longer (less header overhead at low load)? Should a seat be
   allowed to sign mid-round when its leaves fill a vertex?
2. **`Compact` references in v1**, or `Full` first and `Compact` behind a second fence?
3. **The equivocation penalty.** 100 ‰ of the bond plus locks plus ejection, or the ADR-0152 per-act table?
4. **Part I migration.** Receipts accepted for one receipt window past the fence, or until every claim bound before the fence is
   licensed or void?
5. **Witness bytes in the class profile** (§II.10). Should they enter the verification-window derivation, and seat pay?
6. **Seat pay for algebraic checks.** Should a receipt filed by a sketching seat earn what a replay earns? It costs the network the same
   licence, but costs the seat less compute and more bandwidth.
7. **Audit-mesh parameters.** `trap_rate` (1 %?), `trap_penalty` (a reservation?), audits per claim, and audit pay.
8. **`w_cap` for capped mode** (1 % of fork-choice weight?) and `capped_admission_permille`. Re-verify all capped claims, or a random
   subset with a stated detection rate?
9. **Per-row history sketches** (§II.3) are modelled, not built. Build them before any long-context class relies on Part II?
10. **The canonical serving set** (§II.2). Keep it as node policy, or pin it in the class profile so that every producer serves the same
    bytes?

#### Decision

**Selected 2026-10-06:** adopt Part V's constraint-based Panel verification as the new large-model direction. Begin with batched Freivalds on a real TIR MatMul, extend to complete segment constraints and scope-bound receipts, then complete the remaining native relations and boundary checks by direct checking or deterministic reexecution. Preserve exact bounded court on dispute and RFC11's whole-claim security target.

Before implementation is eligible for activation, settle the exact suite/transcript and opening construction, canonical receipt encoding/tag allocation, assignment/coverage policy, derived parameter limits, total localization bound, evidence retention and measured capacity. §V.8 is the delivery order and §V.9 the migration contract. No deployment, benchmark success, Kimi compatibility or change to existing claim semantics is established by this decision.

#### Mission alignment amendment — 2026-10-07

G7およびPart IV/Vの外部監査主体を、Panelに選ばれたseatから普通のpublic bond全体へ拡張する。private sketches/preprocessingは通常検査の最適化に限り、それを持たない外部verifierにも独立した公開localization/conviction経路を用意する。scope-bound receiptsの集約やquorumは算術真実の根拠ではない。直接 Freivalds 等の確率的検査の失敗をそのままslashせず、認証されたbounded terminal proofへ落とす。raw cell samplingと未検出確率は別途扱う。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

確率的receiptとexact escalationに共通bond/DAA予算を追加する。claim容量×mでもblock/reward/Final weightの総配分は同じであり、受理予約とFinalで検査する。
全constraint coverage・error composition・challenge bindingを維持する。carrier/claim数の細分化だけで独立乱数sourceや追加work creditを作らず、source/Final/settlement readerを同じversioned会計へ接続する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### 改定 — 2026-10-12（Work Beaconの独立bootstrap：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §6と[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md) N3。

§VI.3の「既存の独立して有効なPALW workをsourceにする」規則に、自己依存しない根を追加する候補を設計する。`BootstrapExact`（仮称）は、小さく資源有界な実際のモデル/kernel計算を検証するsource専用経路で、全validatorが明示された入力・実行規則から厳密に検証でき、通常のWork Beacon・そのBeaconによるPanel選出・候補model自身のconformance・将来のPALW Finalを要求しない。source-only経路はoptimisticなPanel=0 modeではなく別の明示的exact-validation種別とし、初期案では追加の報酬・ブロック権・Final weightを発生させない。
§VI.8: `IndependentValidityVerified` と `EntropyEligibilityVerified` を別の判定にする。後者にはtargetのcommit後の予測困難性、候補の自由な選別、withholding、private fork、複数bond、隠れた未公表試行、reorg、source ID再包装への耐性が必要で、正しい確定的な計算でも事前計算可能なら不適格である。依存グラフは型付きの有向非循環グラフとし、candidateのsourceに自身やその新しいsemanticsに依存するworkを含めない（Final・settlement・Panel assignmentまで含めて検査）。既存block hash・管理者seed・heartbeat・BASE-0・委員会seed・local RNGへの隠れたfallbackは追加しない。十分なbootstrap sourceが設計・検証されない場合、新しい報酬profileはDormantに維持する（既存チェーンの通常動作は止めない）。

<a id="spec-closure-2026-10-15"></a>

#### Spec closures — 2026-10-15

**Scope and reading rule.** This section closes specification gaps for R7-SUBJECT, R7-SOURCE, R7-RANDOM,
R7-RELATION and R7-ECON of the
[design audit's requirement index](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引).
It states, as normative text, rules that the later design records implement but this RFC never stated, and
reconciles contradictions in favour of [PRINCIPLES](../PRINCIPLES.md), ADR-0173/0176/0177/0178 and later user
rulings. Superseded body text is annotated, not deleted. Detail lives in
[the paid consumer contract](../design/palw/execution/rfc07-paid-consumer-contract-2026-10-13.md),
[full approval accounting](../design/palw/execution/rfc07-full-approval-accounting-2026-10-13.md),
[the Panel paid consumer](../design/palw/verification/rfc07-panel-paid-consumer-2026-10-13.md),
[the ClaimVerification migration design](../design/palw/execution/rfc07-claim-paid-v2-migration-design-2026-10-13.md) and
[the paid ClaimVerification V2 checkpoint](../design/palw/execution/rfc07-claim-paid-v2-implementation-2026-10-14.md).
Nothing here is an implementation PASS, external 128-bit review, source-independence evidence, economic approval
or activation. Both approval registries stay empty; every new fence stays absent on public presets.
"USER DECISION REQUIRED" marks a named parameter whose value needs the user's decision; a recommendation appears
only where an existing record justifies it.

##### R7-SUBJECT — immutable subject binding

1. Every receipt, relation check, lock, interactive round message, tally and Final projection MUST bind the
   immutable subject frozen before its source window: `subject_kind`, subject id, class/kernel/plan/checker suite,
   scope, input/state/output/trace commitments and openings. A receipt for one subject, scope, policy, class or job
   MUST NOT count for another; a missing or narrower scope contributes no coverage. Inapplicable roots use typed
   absence, never a wildcard zero.
2. **Paid ClaimVerification V2** (signed kernel inner71 (renumbered from 64) `PaidClaimV2`, fence `palw_claim_challenge_v2`, dormant)
   has three separate signed authorities — class registrant, job payer, claim producer — plus an optional source
   sponsor who can sign only its own fee/capacity/context. New identities use new domains. An already committed V1
   claim MUST NOT be enrolled; after a job is enrolled, an ordinary V1 reveal of it is refused, and V1 receipts/tallies
   cannot cover the V2 identity. One canonical base-job settlement and one useful-work identity are preserved.
3. **Producer consent and clocks.** The producer signs a not-before anchor and a finite accepted-until interval.
   Actual inclusion inside it MUST NOT extend any checking, challenge, dispute or liability endpoint. The source
   window is derived from the actual native accepted DAA, including sparse DAA; the whole source window, native source
   Final floor, settlement depth and original checking rights must fit the signed checking deadline, otherwise
   admission is refused. Court and grace requests must fit the signed horizon; a late daemon cannot rebase it.
4. **Checker snapshot.** Eligible source profiles and checker bond/operator candidates are frozen at producer
   commitment; seats are selected from that snapshot with the verified future seed. Joining after entropy adds no
   candidate. Distinct operators are enforced but are not proof of independent economic control.
5. **Panel target.** Panel inner 70 (renumbered from 63) freezes the epoch snapshot, exact request, source/cost policy and named sponsor
   before entropy; assigned work remains excluded as a source after it becomes terminal. The source-contract `H`
   derived there is **not** the last possible Panel consumer position (retries, paused receipt clocks, bounded draw
   queues and sparse DAA can defer it). The absolute consumer expiry is specified by the signed Panel lifetime V2
   contract (RFC8 2026-10-15 section, R8-WINDOW item 6); its terminal result MUST NOT neutralize a timely open
   fraud/default demand.
6. **Named parameters** `PalwClaimChallengePolicyV2 {maximum_classes, maximum_jobs, maximum_live_claims,
   maximum_enrollments_per_job, maximum_stream_blocks, maximum_rounds}` — invariants: all non-zero;
   `maximum_rounds ≤ 64`; classes/jobs/live claims ≤ 65,536; enrollments per job ≤ 1,024;
   `maximum_jobs × maximum_enrollments_per_job ≤ 1,000,000`; a zero `lifetime_review_scope`,
   `source_independence_evidence` or `economic_evidence` means private mechanics only. **value: USER DECISION REQUIRED.**

##### R7-SOURCE — eligibility, order, distinct work and payment

1. §VI.3 stands. Added rules from the implemented collector:
   - The qualifying set MUST have pairwise distinct canonical work identities, and every source Final must be
     chronologically consistent with the reveal; a duplicate or an inconsistency **vetoes the whole attempt**.
     Dropping a source after reveal to select a different subset is forbidden.
   - The authenticated job payer (kernel ledger table 18 / the seal fact's poster) is the source's consumer
     identity. Sources sharing one payer count as one toward quorum. A historical job without an authenticated payer
     row stays `Absent`; it is never inferred from producer, registrant or genesis operator.
   - For a Panel target, sources must be Panel-independent.
2. **Paid opt-in.** Each target reserves its own source collateral (kernel table 30) and the recipient's fixed `R`
   before any source row is accepted. Withholding, reseal, deadline failure or public conviction creates a permanent
   veto and forfeits only the contracted collateral; fee principal returns once to the sponsor and cancels the
   corresponding `R` without income. A standing source Final pays nothing by itself; after the whole signed horizon
   the closing tick moves the same `R` into the payout queue, and `R` persists through actual payment + W.
   A target veto never confiscates an honest source's fulfilled prepaid fee.
3. **Sponsor permit.** A sponsor authorizes only its own free collateral for one exact pre-entropy context, fixed
   gross fee, capacity and expiry, frozen before the seal window starts. Replacing a payer or fee contract after
   entropy is visible is refused. No implicit pooling; an arbitrary class registrant is not automatically the payer.
4. **Bootstrap (N3, 2026-10-12 user ruling).** Until an independent bootstrap source is designed and verified,
   new reward profiles stay dormant; bootstrap through Panel=0 `OpvFinal` is not enabled. `IndependentValidityVerified`
   and `EntropyEligibilityVerified` are separate judgements.
5. **Independence evidence** requires an exact release identity, an external measurement and disjoint observed
   control identities. Distinct keys or operator ids alone are insufficient.
6. **Named parameters:**
   - `source_seal_deposit` `d_src`: **value: USER DECISION REQUIRED** (recommended: the derived parity value
     `d_src = c_ab·T_w/T_ab`, 28.6 BILI at the interim terms and 49% reporter share —
     [econ record B12/D-B12](../design/palw/econ-verifier-incentive-and-allocation.md); the honest-race seal cap of
     1 BILI is a different price, so the two must be split as that record's S3 states).
   - Producer/global concurrent source caps, source-Final slack (invariant: ≥ the ledger's challenge + court + grace
     sum), source fee floor: **value: USER DECISION REQUIRED**.
   - `work_count_k`, `settlement_depth_D`, `anchor_delay_slots`, `retry_limit` of §VI.2: **value: USER DECISION
     REQUIRED** (no recommendation; §VI.8 item 2 analysis absent).
7. **BootstrapExact object and fold stage (2026-10-15, round 3; dormant source, not compiled, not tested).** The N3
   bootstrap now has a carried object and a fold stage under its own dormant fence `palw_beacon_bootstrap_v1`
   (Some-only, `None` on every preset, refused when armed; requires `palw_probabilistic_constraints_v1` at or below it).
   Kernel inner **73** `BootstrapExactV1` carries one bond's `Seal {commitment}` or `Reveal {preimage}` for one round
   `(profile, target, slot)`; kernel route aux table **130** holds the rounds (key = round id). Normative rules:
   - The round's windows derive from the target kernel claim's own `committed_daa` (read from the ledger at the first seal,
     never from the contribution): seals in `(committed, committed + seal_window_daa]`, reveals in
     `(seal end, seal end + reveal_window_daa]`; one seal per contributor; `commitment = H(round id, contributor, preimage)`.
   - The fold judges a round once `now > reveal end` and either an input was withheld (the fold's withholding rule is
     `AbortRound`: the round yields no output) or `now − last input ≥ profile.min_settlement_depth`. Every fact is derived
     by the fold: content-only work identity `H(round, canonical commitments)`; exact output `H(round id, canonical
     (contributor, preimage))` re-executed (V1 combiner: a structural placeholder, not an approved BootstrapExact kernel);
     resource units = inputs; reads = {Genesis, KernelActive}; inputs-fixed position = the last seal (strictly after the
     target); independent contributors = distinct operators of the sealing bonds; selection = every seal in canonical
     `(sealed_daa, contributor)` order under the per-round cap; trials = on-chain seals; derivation = selected chain at
     `now − last input`. `palw_bootstrap_independent_validity_v1`, then (only if it holds)
     `palw_bootstrap_entropy_eligibility_v1`, run on those facts; the judgement is stored and the restore fence re-derives
     it. `palw_bootstrap_holds_in_state_v1` feeds `palw_bootstrap_holds_v1` the re-derived witnesses.
   - `approved_bootstrap_entropy_profiles_v1()` stays empty: the bootstrap holds nowhere and no new reward profile opens.
     The fence's `profile` field is what rounds are judged under, never an approval.
   - Named parameters `PalwBeaconBootstrapPolicyV1 {profile, seal_window_daa, reveal_window_daa,
     maximum_contributors_per_round, maximum_sources_per_target, maximum_open_rounds, judged_retention_daa,
     security_review_scope}` — invariants: finite non-zero windows, `maximum_contributors_per_round ≥
     profile.min_independent_contributors`, `maximum_sources_per_target ≥ profile.sources_required`; **value: USER DECISION
     REQUIRED** (the probe uses a SYNTHETIC profile only). The production BootstrapExact kernel, its resource bound and the
     entropy profile (bias analysis covering `AbortRound`) remain undecided.

##### R7-RANDOM — collection, composition and grinding

1. **Collection.** All qualifying seals in the window are mixed in a fixed canonical order; one withheld or invalid
   source vetoes the attempt; the result is independent of input order. `BEACON_UNAVAILABLE` stays pending, with no
   fallback seed (§VI.7).
2. **Effective bits (cumulative accounting).** A full approval MUST charge algebraic, commitment-binding, source and
   coverage failure events, each paying the retries, adaptive subjects, grinding and (for source failures) beacons per
   attempt, combined by union without independence, and unioned over every authorized current **and historical**
   suite/policy. An `Unknown` term contributes probability 1 (zero bits); nothing defaults to exact. Algebraic-only
   tuples cannot yield full approval. A new review digest, policy version or ruleset MUST NOT reset earlier failure
   terms. The target is an **effective** 128-bit bound (2026-10-08 user ruling); a short seal TTL/window MUST NOT be
   presented as 128-bit, and 2-bit settings are drill-only.
3. **Lifetime counter.** Each subject admission spends a canonical counter before entropy, inside a committed
   `LifetimeReviewScopeV3` (chain/ruleset/start/end, complete historical suite/policy registry, Q/G/beacon caps,
   independent review/source/economic roots). The counter survives retries, policy changes, restart and row
   retirement and reorgs with canonical state. Abandoned-fork and offline grinding (`G`) is a reviewed external
   bound; a node-local non-rollback counter is not consensus authority.
4. **Interactive modes.** The initial paid V2 contract is **NonInteractive**; `StagedBeacon` and Fiat–Shamir are
   refused at class registration and StageMessage until a separately versioned interactive compiler exists.
   The separately versioned full-material-opening plan (`CompiledClaimProofPlanV3`) fixes target, mode, 1–64 rounds,
   stream budget and an opening cap ≤ 64 MiB; it cannot authorize a computation-validity argument. Repeated audit rounds need a producer-
   signed StageMessage, a new source context from its own accepted DAA with a strictly later seal window, and a new
   sponsor reserve; Fiat–Shamir recomputes the transform over the statement, source anchor/output and all earlier
   messages and challenges. The initial seed is never reused as every later challenge (§VI.6 unchanged).
5. **Streams.** The V2 rejection stream has a finite block budget shared across scopes/stages, distinct labels and
   no biased fallback; exhaustion is `unavailable`, never success or fraud. The derived seed is passed directly to
   the checker and MUST NOT be written into the V1 beacon slot.
6. **Non-entropy bytes.** Header hashes, signatures, carriers, provider Ready signatures and executor seals of the
   PAV3 header carriage (RFC8 2026-10-15, R8-LANE) are outside seed-bearing and ticket-bearing bytes.
7. **Named parameters** `Q` (total adaptive-subject ceiling), retries per subject, staged-beacon ceiling, `G`, the
   global error allocation: **value: USER DECISION REQUIRED** (bound by the review; no recommendation).

##### R7-RELATION — relation coverage per target

1. Program and Pipeline use the seeded mathematical verifier, including decode, authenticated material and exact
   edges. Typed memory/composite use separately labelled seeded checks per committed step/stage; retrieval stays
   exact. Segmented uses its exact full replay with authenticated root comparison. Diagnostic subsets never
   constitute coverage.
2. A class/profile whose relation family (routing/TopK, history, gather, quantization, boundary) lacks a public
   refutation path to a bounded exact court with bounded costs MUST NOT be eligible for reward or weight
   (PRINCIPLES §6).
3. A locally verified success is a private type that cannot be deserialized from an asserted receipt. A receipt is
   seat assurance, not native recomputation.
4. Before admitting a reviewed scope, native code compares the review's raw algebraic bound with the registered
   plan, positions, repetitions and step/stage union; a source policy's M127 field label cannot overstate a CRT result.
5. Remaining (observations): v1/v2 node evidence exists; v4/v5 courts not reached; compact proof families need
   their own contracts; class-by-class public refutation of routing/history/gather is open.
6. **Permanent direct-check boundary (2026-10-15 user decision).** Historical family contract wire IDs are readable only for identification/rejection. No family contract, review or private fence may authorize a cryptographic computation-validity proof. `SumCheckHadamardV1` and `SumCheckReduceSumV1` tags remain frozen but cannot be newly compiled, generated, verified or credited. Their prover/Fiat–Shamir implementation is removed. Their computed detection credit is zero even for a zero-variable historical record.
7. **Permitted relation execution.** The native V4 contract derives coverage from the accepted class for all targets and binds its exact source/lifetime. `FreivaldsSeededV1` performs direct MatMul checks, `LinearEvaluationV1` performs direct Add/Sub checks, both over authenticated operands. Their frozen record grammar contains only empty producer payloads; it is an audit record, not a correctness proof. Mul, ReduceSum, Cast, Structure, QuantRange, Nonlinear, Selection, RecurrentState and MediaPipeline remain deterministically checked/localized. A forbidden tag or nonempty producer transcript fails admission/round validation before any receipt or Final credit. Old IDs are never reinterpreted as another checker.
8. **Conditional error accounting.** Whole-target unions include every permitted randomized check, positions/stages/steps, repeated attempts and grinding; hashes contribute an identity-binding assumption only. Exact relations contribute no random-check error when actually executed. The honest selected verifier fraction, correlated/operator-controlled seats, material access, timely challenge and collection conditions remain explicit separate assumptions. No nominal 128-bit number certifies all model computation. External review of allowed checks and actual consumer/node tests remain unperformed gates; forbidden proofs are not a remaining research item.

##### R7-ECON — costs, collection and lazy verification

1. The expected per-draw capital surcharge is not a per-epoch guarantee, and a prize cap is not a collection floor
   (ECON §5f / model §15 correction).
2. Economic evidence is admissible only with the exact release observation identity and reviewer, the measured
   interval, an actual native payment observation reconstructed to the coinbase transaction and queue output, the
   exact original entitlement and material receipt, prepaid fee terms and source operator. A promise, a FeePaid event,
   a queue amount or an opaque digest alone does not pass.
3. Gross slash is not the attacker's loss: evaluate ADR-0176 D6 with the reporter share (ADR-0032, dormant
   `palw_reporter_share_v2`) netted out.
4. ECON round-4 UNKNOWN/FAIL statuses are unchanged by this text.
5. Named parameters: per-attempt deposit binding against one-deposit-many-attempts and its concurrency cap `N_max`
   (econ record S4) — **value: USER DECISION REQUIRED**.
6. Remaining: measured costs, collection floor, funded minimum, lazy-verifier deterrence (validation/evidence).
7. **Dormant mechanics (2026-10-15, no activation).** M\*-49's drawn-check attestation is carried as kernel inner 39
   (`AttestDrawnCheckV1`, `palw_verifier_pay_v1`; signed by the drawn slot holder). The optional verification-attestation
   gate on the model allocation is `palw_model_verification_gate_v1` (inner 67, aux 125): per model-epoch `m` stake-drawn
   slots (with replacement) each attest one beacon-sampled Final claim; the epoch's allocation subsidy is payable only on
   `k` "verified" slots, withheld (never minted) only for unattested verification, and never withheld because a model had no
   Final claim to sample (ADR-0177 D1). Named parameters `PalwModelVerificationGatePolicyV1 {slots m, threshold k,
   sample_rule, attestation_window_daa, maximum_open_tallies, review scopes}` — invariant `0 < k <= m <= 255`, `2k > m`;
   **value: USER DECISION REQUIRED** (interim probe m = 47, k = 24 from the 2026-10-10 ruling, unapproved). The OPVB
   beacon wiring that opens a tally and the payout integration of its outcome remain open; T6 (lazy verifier) is not
   closed by an attestation.
8. **Beacon → tally → payout wiring (2026-10-15, round 3; dormant source, not compiled, not tested).** Past
   `palw_model_verification_gate_v1` (which now also requires `palw_probabilistic_constraints_v1` at or below it):
   - **Freeze.** At the block that rolls the allocation epoch, every closed `(model m, epoch e)` with a live gated claim
     (budget row naming `(m, e)`, accepted at or past the gate) gets a frozen OPVB context in aux 125 under
     `0x01 ‖ tally id`: v3 sealed-source `ClaimVerification`, `commitment_root = tally id = H(m, e)`,
     `commitment_position` = this block, `challenge_epoch = e`, eligible profiles = the OPV classes eligible now minus `m`,
     excluded = `{m}`, candidate `Absent`, policy = the network's sealed-source onboarding policy (interim, unapproved).
   - **Open.** When that context's beacon locks (`collect_sealed_work_beacon_v3` over the route's sealed sources), the tally
     opens from the VERIFIED beacon output: sample = one Final claim of `(m, e)` (`UniformFinalClaimV1` over the canonical
     set); pool = Active bonds with collateral ≥ the network floor, excluding the sampled claim's producer operator, keyed by
     kernel bond id with stake = collateral; `m` slots drawn by stake. Unavailable/vetoed/refused beacon or an empty pool
     marks the context unavailable. Capacity `maximum_open_tallies` bounds undecided tallies (excess contexts wait).
   - **Pay.** At a gated claim's Final, after the budget grant and before the export cap, buyback and splits, the amount is
     `palw_model_verification_subsidy_v1(outcome(m, e), amount)`: `Attested` / `NoFinalClaim` → paid; `Unattested` (k not
     reached by the deadline, or unreachable, or the beacon unavailable, or a closed epoch with no context) → withheld,
     never named or minted; undecided → deferred whole into aux 125 `0x02 ‖ claim id`. A decided tally is replaced by a
     decided row `0x03 ‖ tally id` (the same answer for every claim of `(m, e)`).
   - **Release.** Each block the gate's tick releases a deferred amount to the producer's payload as one payout (queue key
     `H("MODEL-VERIFICATION-DEFERRED-PAYOUT", claim)`) once `(m, e)` is Attested/NoFinalClaim, or drops it once Unattested;
     decided rows and contexts retire when no live gated claim or deferred reward reads them.
   - Claims, user escrow, Final, weight and slashing are untouched; below the fence and for unallocated claims the payout is
     byte-identical. **USER DECISION REQUIRED**: whether a deferred amount released later should keep the Panel/buyback
     split (the source releases it to the producer alone), and whether an unavailable beacon should withhold (current rule)
     or pay.

##### ALL-MODEL

RFC6's 2026-10-15 section holds the shared rules. For this RFC: challenge subjects bind the immutable model root;
model acquisition is not a source-eligibility or seed input.

---

<a id="source-v1-0010-L00001"></a>

### 旧RFC-0010 / 原文1–254行

出典: [0010-permissionless-palw-panel-and-claim-completion.md](../rfc/0010-permissionless-palw-panel-and-claim-completion.md)。配置理由: 主題別統合。

### RFC-0010: Permissionless PALW Panel binding and claim completion

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Revised Draft, 2026-10-10後続改定 — off-chain model acquisition; model-bond coinbase allocation and conditional prosecution; historical implementation/measurements retain scope; new economics/implementation/activation pending.
* Scope: testnet-12 PALW claims, their Panel binding, verification and reward completion. An RFC merge does not change the live rules.
* Related: [RFC-0009](../rfc/0009-palw-remote-miner.md), [RFC-0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md), [RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md), [ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md), and the [testnet-12 Panel-seed incident and lane A](../history/networks/testnet-12/t12-panel-seed-2026-09-25.md).

#### 0. Decision proposed

**Dormant implementation — audited against integration `3730cc90f`, 2026-10-10:** the transition engine, production virtual-state fold, receipt/court handoff, per-shard draw and RPC/CLI observation exist behind `palw_permissionless_panel_v1`. All networks remain `None`; attempted activation is refused. No production Panel beacon policy is approved; objective seal finality, frozen source-profile eligibility, numeric economics and the operator-free end-to-end release drill remain open. See [implementation, audit fixes, tests and release blockers](../design/palw/integration/rfc-0010-dormant-implementation.md). This does not assign an activation or remove the historical lane-A rule.

**A claim's Panel MUST be selected by public consensus rules without naming a genesis bond, an operator fleet, or another privileged identity.** The claim, the eligible Panel population, and the randomness commitment must be fixed in that order. A binding block only records the result; its producer, hash, signature, timestamp, execution commitment, lane, and arrival order MUST NOT change the Panel seed or the eligible population. Any valid selected-chain block that reaches the prescribed binding point may complete that transition, including a heartbeat if the new rule admits it.

The shortest safe implementation is therefore **permissionless binding with no separate anchor-producer election**. Producer bonds remain the public qualification for claim production; Panel-seat bonds and their collateral remain the public qualification for verification. The binding block needs no third bond kind or extra privileged role. If a later design elects a particular bonded binder, it must additionally satisfy §4; election is not a prerequisite of this RFC.

For one sealed claim on one canonical history, the target invariant is `Panel(claim, binding_block_A) = Panel(claim, binding_block_B)` for every otherwise valid choice of binding block. The claimant's and beacon contributor's ability to choose among **different sealed claims or different beacon outputs** needs the separate controls in §3.

This is a conditional proposal. The current chain has no demonstrated randomness source that meets §3.3 merely because a hash function is applied to public fields. The new rule MUST stay dormant until a concrete beacon, its implementation and its adversarial bias bound pass the activation gates in §9. In particular, removing the eight-bond predicate while continuing to seed the Panel from a cheaply replaceable attempt is forbidden.

##### 0.1 New verification profiles — 2026-10-06

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) supplies the
active-kernel/plan boundary; a new model VM is not part of open Panel admission. For new large-model
claims, [RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md) / [RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md)
select small encoded/algebraic checks, positive scope-bound receipts and exact bounded court on
dispute. Capabilities/readiness must identify the approved suite and its real verification/DA/court
resources; the legacy full-replay working set is not automatically the new checker's requirement.
Neither reduced resources nor extra bonds waive complete constraint coverage or Final conditions.

Panel-selection randomness, generation randomness and algebraic proof challenges are distinct
domains. This RFC's fixed Panel seed does not expose future GKR round challenges prematurely or
permit reusable public Freivalds vectors. Apply RFC11's post-commit/round/transcript policy separately.
The paper's “validator” in PoSP means a computation checker here, not a DNS finality validator or a
new privileged orchestrator set. Keep RFC12's PALW-native authority and this RFC's public bond rules.

##### 0.2 Model acquisition outside consensus — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
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

#### 1. Current rule and trust boundary

On testnet-12, lane F1 derives a Panel seed from `H(anchor attempt execution_commitment_v3 || claim_id)`. Lane A then lets only an attempt from one of the eight genesis operator bonds anchor a claim's Panel. A chain block that merges such an attempt may carry the bind, but the operator attempt supplies the seed. The [incident note](../history/networks/testnet-12/t12-panel-seed-2026-09-25.md) explains the price: an attacker could obtain fresh valid attempt wins with cheap junk work while P0-10 remained open, preview the Panel, and publish a favourable candidate. Lane A prevents an untrusted participant from controlling that source by trusting the operator position instead.

The privilege is consensus-visible in [`palw_operator_anchor_v1.rs`](../../consensus/core/src/palw_operator_anchor_v1.rs) and the post-launch testnet-12 fence in [`params.rs`](../../consensus/core/src/config/params.rs). [`VirtualStateProcessor::palw_chain_block_as_anchor_v1`](../../consensus/src/pipeline/virtual_processor/processor.rs), its anchor walk, SW-8 step 4c, the draw gate, and the one-state pre-check all consume that answer. If no operator attempt reaches a claim before its bind deadline, `BindTimeout` voids it. New public bonds do not become lane-A operators by maturing.

Running a node, registering a bond, producing an ordinary attempt, providing a Panel seat, and anchoring a Panel are distinct permissions. This RFC removes the last identity restriction. It does not claim that an empty network, a class with too few capable seats, or missing evidence can reach `Final`.

#### 2. Required properties and attack model

1. **Open admission.** A new participant who satisfies the same on-chain bond, maturity, collateral, class and capability rules as any other participant can produce a claim and serve a Panel. No genesis membership or operator-maintained allowlist participates in a post-fence bind.
2. **One Panel opportunity per accepted claim.** In one canonical history, a claim has one committed identity and one fixed Panel seed. Re-signing, changing header nonce/timestamp, choosing a different binding block, delaying a bind, or replacing a failed binder cannot produce another seed. Reorgs may change canonical history; replaying a claim on a new history must obey explicit replay and finality rules.
3. **No cheap claim search.** A claimant cannot choose many claim identities after learning their Panel. Merely writing `ticket = H(bond_id, claim_id, slot, R)` does not establish this if `claim_id`, `slot`, or `R` can be chosen or discarded cheaply.
4. **No cheap source search.** Whoever supplies the last randomness input cannot cheaply select among outputs by withholding or replacing it. A work-cost argument must price all valid alternatives, including junk attempts, different bonds, withheld contributions, private forks and delayed publication.
5. **Collateral-split neutrality.** If a selection mechanism assigns weight to bonds, dividing one amount among bonds must not increase an owner's aggregate first-choice probability merely by increasing the number of bond identifiers. Fallback and repeated selection need their own analysis; first-choice proportionality alone is insufficient. The protocol cannot infer distinct human ownership from distinct keys.
6. **Finite progress.** An offline actor cannot hold a claim in an unbounded waiting state. A chain that advances with adequate publicly eligible seats, evidence and an available beacon binds and resolves claims within stated windows. A total absence of those resources ends in a named, non-fraud outcome.
7. **Same answer on every node.** Candidate-chain validation, reorg, IBD and pruning-proof sync derive identical claim identity, snapshot, entropy, Panel, exposure locks and timeout. The transition is independent of a node's local RPC, fleet membership or material cache.

The adversary can control many bonds and Panel seats, split collateral, submit many syntactically valid claims, grind unadmitted headers while P0-10 is open, withhold blocks or evidence, displace blocks through DAG competition, and choose when to go offline. The security evaluation must state the maximum adversarial bonded weight and the assumed honest online capacity. No permissionless protocol can guarantee completion when all qualifying producers, seats or data providers are unavailable.

#### 3. Proposed state machine

##### 3.1 Seal a claim before its Panel entropy is known

An accepted claim enters `PendingSeal` with its existing executor bond, class, job and evidence commitments, payout, exposure, acceptance point and claim ID. A new `ClaimSealV1` identifies the **complete, immutable** claim content and its chain-specific acceptance. It MUST cover every claimant-controlled field that could change a draw; randomized signature bytes and equivalent encodings must not create new draw identities for the same claim. Duplicate work/claim identities follow an explicit once-per-history rule. The seal is committed by the chain before the entropy used by this claim can be learned.

The seal becomes `Sealed` only at a consensus-defined checkpoint or depth that is stable under the network's stated finality/reorg policy. The checkpoint must not depend on this claim's own Panel verdict, the eight genesis operators or a DNS validator set that may be inactive; the network's objective L1 fallback must be specified. A miner-selected `anchor_slot` or claimed checkpoint is not authoritative. A reorg before sealing rolls back the claim; a reorg after sealing must satisfy the existing finality rules. Binding deadlines are measured from the new seed-readiness point, not blindly from the old `accepted_daa + window_bind`, because waiting for a safe seal may outlast the current window. Collateral and fee exposure across this wait must be bounded.

This seal blocks **post-entropy** claim-ID search. It does not make many distinct pre-entropy claims free: each additional claim must face the existing execution, admission, exposure and fee rules, with a measured cap on simultaneous outstanding claims per economic weight. If P0-10 still permits a near-free claim, that is an activation blocker (§9).

##### 3.2 Freeze the eligible population and draw policy

For sealed claim `C`, consensus computes an immutable `PanelSnapshotV1` from the claim's designated pre-entropy chain checkpoint. It records a commitment to eligible seat bonds, amounts, maturity, capabilities, model readiness, exclusions (including the executor), stake weights, the draw policy and its rule version. Registration, top-up, split, withdrawal, readiness and capability changes **after** this checkpoint cannot change this claim's initial population or weights. A new bond can qualify for later claims once it matures; a genesis bond has no special status.

The snapshot is **before** the claim's entropy is fixed. If a class lacks enough capable seats, the claim stays in its existing no-panel/held path or reaches a named `NoCapablePanel` outcome; an operator cannot supply an ad hoc replacement list. A class-registration jury and readiness transition must likewise read public bonds and objective proofs. The current `Candidate` → `Prefetching` → `Probation` prerequisites remain capacity conditions, not a named-operator approval path. The implementation audit must check that a fresh cohort can supply the floor population and required model seats without genesis keys.

Draw-time collateral headroom also needs a fixed answer. At a deterministic `AssignmentPointV1` after entropy readiness, consensus processes due claims in canonical `(beacon epoch, sealed acceptance order, claim occurrence index)` order, derives their seat order from the frozen snapshot, and reserves live exposure atomically. The tie-break must come from consensus acceptance order, not a hash that the claimant can grind for queue priority. The binder cannot choose the base state or delay assignment until a desired seat becomes free. Processing limits and backlog order must be consensus parameters; a cap cannot let a block producer choose which due claims to skip. A claim whose frozen candidates lack reservable collateral follows a named, deterministic no-panel path.

##### 3.2.1 genesis bond の廃止と初期起動 — 2026-10-28 ユーザー決定

**genesis に事前登録する bond を廃止し、新しい立ち上げは bond ゼロから始める。** 後から参加できない genesis bond という役割・特権を残さず、最初の参加者も通常の登録・担保・成熟条件で bond を作る。

初期は PALW ブロックがない状態で約30分待ち、既存の BASE 回復規則を利用する。その後、**BASE で得た資金に加えて genesis に入っている UTXO** も使って通常の bond 登録を行い、資格を満たした参加者から PALW へ進む。genesis UTXO は廃止せず、その所有だけで検証席や anchor の特権を与えない。最初の30分間を BASE で稼働する特別モードではなく、無 PALW の待機後に回復経路を使う方針である。

仕様・既存実装との照合事項は [RFC-0041](../rfc/0041-genesis-bond-free-bootstrap.md)、判断は [ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md) に記録する。この追記は採択済み方針であり、起動試験の完了記録ではない。旧 genesis の履歴は保持する。

##### 3.3 Supply entropy that the binder cannot choose

For a sealed claim, `R` must be unpredictable under the stated adversary model until **after** the seal and Panel snapshot are fixed. Its specification must name: the producer set, exact input message, output/proof bytes, verification cost, contribution deadline, missing-contribution rule, chain/fork domain, and an adversary's maximum ability to bias or suppress it. The source may use permissionless mature bonds and certified execution tickets, provided every contributing opportunity has one canonical work identity and consensus can reject a second valid variant. It must not require a named operator or an active DNS validator set. A digest that merely hashes an arbitrary attempt or a producer-chosen signature is not a certified ticket.

The implementation must resolve P0-10's cheap unadmitted-attempt path if it uses PALW attempts as entropy. Header-stage pinning alone is documented as insufficient: execution roots can remain unchecked until later Panel work. A certified-execution source must be verifiable before its randomness is consumed, without asking the Panel being selected to certify its own source. Its bootstrap, computation bound, fraud/court latency and header-validation cost must be specified. A different beacon is acceptable only after equivalent unpredictability, uniqueness, withholding and liveness analysis.

`R` is the same for every binding carrier of a claim. The proposed domain-separated derivation is:

```text
panel_seed_v3 = H(
    "misaka-palw/panel-v3/seed" || network_id || ruleset_id ||
    claim_seal_id || anchor_slot || panel_snapshot_root ||
    beacon_epoch || beacon_output
)
```

These names describe the required binding; the exact encoding and hash function belong to a versioned wire specification. `anchor_slot` is derived by consensus at seal time. The beacon must not be selected *because* this seed yields a desired Panel. A last contributor's ability to suppress an output is part of the security budget, even when the output is unique if published. Hashing, a single VRF, unpenalized commit–reveal, a later raw block hash, or a VDF claim does not by itself establish the required property; the [incident analysis](../history/networks/testnet-12/t12-panel-seed-2026-09-25.md) already identifies their relevant last-mover or claim-grinding failures.

Until a source meets these requirements, the new fence stays `None` and lane A continues under its existing rule. `BeaconUnavailable` after a bounded deadline ends a claim without a fraud slash; silently substituting a cheap seed is forbidden.

##### 3.4 Bind on an ordinary chain transition

After `AssignmentPointV1`, the first selected-chain block at or after the prescribed bind point commits the already-derived `PanelBoundV3` transition **automatically in the state fold**. Its lane and producer do not supply a draw input. A valid heartbeat can bind when the chain's existing heartbeat rule advances the clock; a valid bonded attempt can bind under the same rule. The block cannot pick a subset of due claims or choose their order. If the protocol keeps a separate explicit Panel carrier, any peer may relay it and its payload must be completely determined and validated by consensus.

The state records `claim_seal_id`, `panel_snapshot_root`, `beacon_id`, `panel_seed_v3`, `assignment_point`, `binding_block`, the exact seat list and exposure reservations. `binding_block` is an inclusion witness; it is never a source of Panel randomness. Alternative valid blocks at the bind point compute the same Panel and reservations for the same canonical history. The pre-fence `PalwAnchorFactV2` and its historical block/execution seed remain valid for old claims.

##### 3.4a Verification challenges are a separate contract (2026-10-08)

[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) alone
governs the proposed post-commit algebraic/conformance challenges. RFC10's Panel draw, receipt-ticket sources and
redraw list remain separately versioned rules; their safety does not automatically prove Freivalds/GKR challenges
unbiased. Even if a future release shares physical source facts, use distinct domains, bindings, timing and independent
source/bias review. Panel assignment seeds never substitute for a missing verification beacon. This amendment does
not migrate existing Panel/claim seeds or reactivate DNS/BFT beacon authority. EXEC slices are neither chain binders
nor N beacon contributors; their root must separately meet the shared source eligibility rules.

##### 3.5 Unavailability and a finite fallback

If an assigned seat does not answer, any permitted redraw chooses the **next deterministic, non-reused seat** from the fixed snapshot using `H(panel_seed_v3 || retry_index || role)`. It never obtains new randomness from the timeout block, a candidate anchor, or a late bond. Retry count and windows are bounded; exhaustion is `NoCapablePanel` or the existing named non-fraud void outcome. A seat's silence may influence which precommitted alternate is reached, so the abuse bound must include adversarial seat weight and economic cost; the protocol must not claim perfect withholding resistance from a deterministic list alone.

No answer, a node's local fetch failure, or an unpublished candidate is not proof of fraud. A cryptographically proven conflicting signature, duplicate spend, invalid certified ticket or other objectively verifiable violation may be slashable under a separately specified evidence window. A participant that simply misses an opportunity loses that opportunity. The existing verifier-unavailability and producer-fraud cases must remain distinct ([ADR-0166](../adr/0166-verifier-unavailability-is-not-producer-fraud.md)).

#### 4. Bond weight and canonical tickets

No new `anchor bond` is introduced. The existing bond object may carry producer and Panel capabilities at separately specified collateral thresholds. If the chosen beacon needs contributors, their admission is based on mature, publicly registered bonds and one canonical contribution per `(network, bond, beacon epoch)` or narrower declared opportunity. The input must bind to a pre-entropy committed work identity, not a selectable signature, nonce, claim ID or block hash. A second contribution for the same opportunity is invalid or duplicate; mere nonpublication cannot be proved as equivocation.

Panel seats also have a Sybil limit: distinct bond keys are not evidence of distinct people. A single owner can split enough collateral into multiple seat-eligible bonds and may occupy several Panel positions. The security proof must calculate quorum-capture probability and minimum attack collateral under such splitting, including the class jury, outsider seat, redraw and court. Stake-weighted first-seat selection alone does not establish a multi-seat independence guarantee. State the honest-bonded-weight assumption and check that the required collateral and slashable exposure exceed the value a captured Panel can extract.

An elected-binder variant would add work without improving Panel fairness after §3.3: it needs a bond snapshot, stake-weighted candidate order, offline fallback and a split-invariance proof. If pursued, it MUST freeze the eligible set before the beacon, use one canonical score per bond and opportunity, make all fallback ranks public before the first timeout, and keep **the same Panel seed** across every rank. Where both bond sizes meet that role's threshold, the first weighted winner's aggregate probability for `130,000 BILI × 1` and `13,000 BILI × 10` should match when total effective collateral matches. The full fallback distribution and an actor controlling many bonds require separate adversarial analysis; different keys are not evidence of independent owners. This RFC's recommended binder rule avoids that extra lottery.

#### 5. Completing a claim without the existing fleet

The Panel-anchor change alone is insufficient for an end-to-end permissionless claim. The release may claim that property only when all of the following paths are live for a new participant using publicly obtainable funds and artifacts:

| Stage | Required open path | Implementation dependency |
| --- | --- | --- |
| Register and produce | Public bond registration, maturity and collateral rules; no genesis or fleet key. New model onboarding needs enough objectively eligible independent seats. | Existing registry plus §3.2 audit. |
| Submit | Miner signs a claim/attempt under its own bond and can send it through any validating node or relay. | [RFC-0009 §3](../rfc/0009-palw-remote-miner.md). |
| Bind | Public beacon, frozen Panel snapshot and any valid chain binder; no eight-bond lane A after the fence. | This RFC §§3–4. |
| Verify | Public seats and permitted non-seat auditors can fetch the same committed evidence and participate in Panel/court without the producer's continuously running node. Accusation and court entry use public conditions, not an operator fleet. | [RFC-0009 §4](../rfc/0009-palw-remote-miner.md) and [RFC-0007 Part IV](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md), with availability proofs and liability split. |
| Settle and redeem | `Final` and ordinary claim payout follow bond ownership; a winning free-prompt quantum can be carried by another eligible builder and still pay the miner. | [RFC-0009 §5](../rfc/0009-palw-remote-miner.md). |
| Observe | A miner can verify the relevant chain, bond, claim, Panel and payout facts without trusting one fleet RPC. | [RFC-0009 §6](../rfc/0009-palw-remote-miner.md). |

DNS-finality validators are a separate role and must not become a hidden prerequisite for PALW Panel selection or claim payout. A network with no willing independent Panel seats, beacon contributors or data providers cannot promise automatic `Final`; it must expose the blocking condition and bounded, non-fraud outcome. Testnet faucet funding from a premine is a current **operational** access barrier, even when the consensus entry rule itself is public. Launch claims about practical permissionlessness require a working funding route and measured participation concentration.

#### 6. Concrete implementation changes

| Area | Required change |
| --- | --- |
| `consensus/core/src/config/params.rs` | Add a versioned, initially dormant `palw_permissionless_panel_v1` fence and all timing, snapshot, queue and beacon parameters. Validate dependency ordering; hash the rule and values into the relevant params/schedule identities and normalize dormant values consistently. Revise `palw_anchor_at_ceiling`'s lane-A prerequisite for the new version without changing its historical meaning. Never arm at an already scheduled height that would hide a fork-ID change. |
| `consensus/core/src/palw_operator_anchor_v1.rs` | Keep the eight-genesis predicate solely for pre-fence historical validation. No post-fence code path may call `operator_of_v1` to decide binding or entropy eligibility. Do not rewrite old blocks or old pending claims under the new rule. |
| `consensus/core/src/palw_panel_v2.rs` | Add versioned snapshot, seed and deterministic alternate-seat derivation. Preserve v1/v2 domain separation. Pin exactly which snapshot fields affect stake weights, maturity, capability, exposure and class readiness. |
| `consensus/core/src/palw_state_v2.rs` | Add sealed/entropy-ready/assigned/bound states or an equivalent versioned transition, canonical pending-claim queue, bounded assignment, exposure reservations, `BeaconUnavailable` and no-panel outcomes. Change SW-8 step 4c and timeout handling only for claims admitted under the new fence. The old `PanelBound` representation must remain decodable. |
| `consensus/src/pipeline/virtual_processor/processor.rs` | Version `palw_v2_anchor_fact_of_candidate`, `palw_chain_block_as_anchor_v1`, `palw_v2_anchor_fact_with_seed_v1`, `palw_sw8_anchor_for`, the one-state pre-check, draw gate and fold to read one post-fence `PanelBindingFactV3`. Derive it from the seal, snapshot and beacon, then use the first eligible chain block solely as binding witness. Ensure mergeset displacement, reorg and pruning replay read the same inputs. |
| `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs` and PALW admission | If PALW work supplies the beacon, enforce a canonical, certified, bounded ticket before it can affect randomness. The existing header-pins fence alone does not certify an execution. Fix or bypass P0-10 under an auditable new rule before allowing such a source. |
| Node, Panel, RPC and CLI | Expose seal, snapshot, beacon, assignment, binder, retry and failure reasons. Remove operator-fleet assumptions from new claim instructions. Integrate RFC-0009's independent evidence and public redemption paths before advertising node-less end-to-end completion. |

The implementation specification must name exact wire objects, signature domains, bytes in every root, validation stage, state-root changes, size limits, bounded per-block work and network-specific activation. File paths above are the current seam map, not a license to change existing types in place without versioning.

#### 7. Migration and compatibility

The new rule applies to claims whose **acceptance/seal** is at or after its fence. A claim already accepted under lane A keeps the eight-operator rule until it reaches `Final` or void; an anchor that happens after the fence does not silently change its seed or responsibility. The release must document the maximum old-claim drain time. Only after that drain can the fleet be removed as a liveness requirement for *all* outstanding claims.

Versioned signatures and hash domains prevent replay across networks and old/new claim forms. Upgraded and unupgraded nodes must split at the stated fork boundary with an explicit fingerprint/schedule change. Mainnet remains unlaunched and receives no implied activation from a testnet RFC. Revised RFC-0008 places work slices in the existing EXEC class: they are not chain-eligible and do not become binders, beacon contributors or clock sources. The REAL root follows the chain's existing qualification rules; EXEC payload/root compatibility needs its own versioned review.

#### 8. Failure cases the design must settle

1. The claimant creates many IDs from one cheap computation, varies a slot/parent, or withholds all but a favourable claim.
2. One bonded actor creates many cheap valid attempts, or one entropy contributor withholds its result after previewing two possible outcomes.
3. The same collateral is split among bonds; first-choice chance, fallback ranks and repeated assignments are compared at equal total collateral.
4. A proposed binder is silent, a heartbeat wins the selected chain, or an honest bonded attempt is displaced into a mergeset. The Panel and exposure reservations stay the same.
5. A Panel seat becomes unavailable after the frozen snapshot; alternates come from one fixed list and the claim leaves pending state within a finite bound.
6. Claims compete for the same collateral; every node applies the same queue order and cannot double-reserve a seat.
7. A reorg crosses the seal, entropy or bind point, including IBD and pruning recovery. Historical lane-A claims keep their original rule.
8. A model is registered by outsiders while the old fleet refuses to carry its artifact or seat it. The protocol either supplies an open path with enough new seats or reports the exact capacity condition that blocks admission.
9. Evidence providers, Panel seats or receipt builders disappear after the miner turns off its PC. Missing data, fraud and unredeemed reward follow different objective rules.

#### 9. Activation gates

1. **Security argument:** a concrete, implemented entropy source with measured/analysed bias under withholding, stake splitting, private forks, claim-ID search and P0-10. The paper rule `H(fixed inputs)` is not sufficient. If no source meets this gate, retain lane A and do not claim permissionless Panel anchoring.
2. **Consensus replay:** two independent nodes reproduce the same sealed claims, snapshots, beacon, Panels, locks and outcomes across the §8 cases and a real chain replay. This includes existing legacy claims spanning the fence.
3. **Availability and economics:** bounded seal-to-bind delay, backlog, seat capacity, beacon failure and court windows; no unbounded lock or silent payout loss. Publish the distribution of operator-controlled bonds and effective Panel weight, not just the number of keys.
4. **End-to-end drill:** a new, non-genesis bonded participant registers, produces and submits a claim; non-genesis seats verify it; a non-operator block binds it; independent providers serve evidence; the claim reaches `Final` and its free-prompt receipt is redeemed by another builder, with payout to the miner. Repeat after removing the original eight operator nodes from the drill.
5. **Release:** audited versioned wire/state changes, explicit fence and fingerprint, migration runbook, rollback policy before activation, and public status metrics. Enabling this RFC and RFC-0009 may be staged, but the public claim of end-to-end permissionlessness waits for all required stages.

#### 10. Decision boundary

This RFC proposes replacing **operator identity as the Panel seed's trust anchor** with a fixed claim, frozen public Panel population, independently established entropy and binder-independent state transition. It deliberately leaves the new entropy primitive and numeric economics unset until §9. The eight-genesis rule is removed only at a fence where the replacement's safety and liveness have been demonstrated. This separates a permissionless consensus qualification from the measured availability of the people and machines that actually run it.

#### Mission alignment amendment — 2026-10-07

public bindingとgenesis-anchor特権の廃止は維持するが、binding成功だけでpermissionless prosecutionの完成としない。非Panelのpublic bondによる証拠取得、accusation、terminal proof、licensed claimのconvictionも完成条件に加える。現在のoperator-anchorは移行対象であり安全性の永続前提ではない。Panel=0はRFC14完成前に有効化せず、現在のbinding方式を恒久必須ともしない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

Panelのreceipt/licenceは共通のproducer経済枠を加算・早期回復する根拠にしない。同額bond・同期間・同倍率で正直な検査と共謀した早いValidから得られる最大機会は同じにする。
Final条件とFinal weight配分を分けて検査し、d+Wまでの消費履歴と未解決責任を保持する。旧claimのreceipt/seat責任は受理時rulesetに従う。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（Panel=1暫定mode・有界の交代・正の証拠を必須にしたFinal：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。

- **§0追記:** 通常の新reward-bearing modeは、1 claimにつき同時に有効なassigned verification seatを1つ持つ。Panel=0へのtimeout fallbackを禁止する。候補集合には複数の参加者を認め、non-seatの検査・反証権限を制限しない。Minerとassigned Panelは適用scopeの正しい登録モデルを保有し、実行/検査と認証に必要な能力を備える（ADR-0177 D1補足）。モデル登録者に一般第三者への提供義務は導入しない。合意判定は認証済み状態と証拠に基づき、特定nodeのHTTP結果・model取得速度を直接入力にしない。Panelは承認済みの確率的checker、固定scope、commit済みstatementへの適正なchallenge、必要な正の検証証拠に基づくreceiptを提出し、署名数1だけを計算正当性の証明としない。producer/registrantとの同一bond・operatorを除外する（同一実質所有者でないことの証明ではない）。
- **有界の交代:** snapshot、claim seal、選出policy、代替候補順序、最大交代数はclaimの規則に固定し、交代のために新しい乱数・候補集合・採掘枠・絶対期限を得ない。健全な検査に必要な残り時間がなければ交代を開始せず、候補か時間が尽きれば非不正・無報酬で終了する。検査不一致が出たら、bounded pursuitまたは完成したobjective proofへ進む。
- **Final:** `FinalEligible = OriginalRulesetAndIdentityBound ∧ NetworkModelBondBudgetsSatisfied ∧ AuthorizedSingleSeatPositiveEvidenceAccepted ∧ RequiredClaimEvidenceAndCourtObligationsSatisfied ∧ ObservationCutoffPassed ∧ NoUnresolvedAcceptedPursuit ∧ NotRefuted ∧ UniqueWorkAndSettlementSatisfied`。支払、vesting、ブロック権、Final weight、EVM/bridge等のreaderも同じ結論を使う。
- **1-seat移行監査:** `seat_count=1` の一括置換を禁止する。segment mask、`seats-1`、quorum、最低独立seat数、shard outsider、署名者担保、Panel fee配分、再抽選、FalseValid attribution、DA/court、全Final経路を点検し、単一seatで全検証scopeを担えないprofileはこのmodeで有効化しない（RFC-0002 seating・RFC-0006 shard・RFC-0003 readinessにも適用）。
- **モデル配分:** 正規Panel予約は1 claimにつき1 seatとし、無応答の旧seat・待機seat・交代回数から架空の追加元本を作らない（ADR-0177 2026-10-12改定）。
