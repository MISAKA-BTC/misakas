# ADR-v2-0006 — Fork choice・参加重み・merged work・settlement

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 6本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

どの枝を選び、どの仕事をsettlementに数えるかを、単なるblock個数と分離する。

## 2. 統合した判断の読み方

### 2.1

IBD候補や署名経路に対する参加条件、chain位置を得る仕事、mergesetから取り込むclaimを一つの状態遷移として読む。[旧ADR-0025](../adr/0025-chain-participation-and-ibd-candidate-selection.md) [旧ADR-0055](../adr/0055-palw-position-is-earned-not-declared.md) [旧ADR-0058](../adr/0058-palw-merged-work-is-counted.md)

### 2.2

PALW自身のsettlement用語とanchorの扱いをDNSの最終性と混同しない。double-spendの検討はblock数だけでなく対応するanchorと依存を対象にする。[旧ADR-0127](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) [旧ADR-0129](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md)

### 2.3

partition回復では枝ごとの排他的なpastのbonded participationを評価するRule Eの採択経緯を継承する。合意creditを持つclaimの条件はGate Cの後続改定と合わせる。[旧ADR-0178](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)

## 3. 維持する境界・後続改定

新しいproposalや測定例を有効なfork-choice規則に繰り上げない。DNS overlayを未来の安全性前提として復活させず、旧networkの履歴は保存する。

## 4. 隣接する判断との分担

EXECのanchorは[ADR-v2-0007](0007-evm-exec-lane-and-anchors.md)、clockは[ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md)、未解決claimの独立進行は[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0025-chain-participation-and-ibd-candidate-selection.md](../adr/0025-chain-participation-and-ibd-candidate-selection.md) | L1–L89 | L12–L14 |
| [0055-palw-position-is-earned-not-declared.md](../adr/0055-palw-position-is-earned-not-declared.md) | L1–L97 | L15–L19 |
| [0058-palw-merged-work-is-counted.md](../adr/0058-palw-merged-work-is-counted.md) | L1–L132 | L15–L20 |
| [0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) | L1–L100 | L15–L26 |
| [0129-a-double-spend-needs-the-anchors-not-the-blocks.md](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md) | L1–L139 | L14–L28 |
| [0178-fork-choice-heals-a-partition-on-bonded-participation.md](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) | L1–L284 | L12–L16 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0025-chain-participation-and-ibd-candidate-selection"></a>

<details>
<summary>0025-chain-participation-and-ibd-candidate-selection.md — 原文 L1–L89</summary>

[原文を開く](../adr/0025-chain-participation-and-ibd-candidate-selection.md) · SHA-256: `c6e45674aab5b34f282c64914fafaaa7d6ca078210b0e6ae49b3c74c21bd7566`

<!-- ADR_SOURCE_BEGIN:0025-chain-participation-and-ibd-candidate-selection.md -->
~~~~~~~~markdown
# ADR-0025 — Chain participation and IBD candidate selection

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** proposed
**Date:** 2026-08-09
**Supersedes nothing. Constrains:** IBD source selection, the sync-status RPCs, every signing path.

## What went wrong

Testnet-22 split. A node ran IBD against the first peer that relayed to it, adopted that peer's
chain wholesale, and then mined and attested on it. Other peers offering a heavier chain were
discarded at the relay guard — unfetched, uncounted, unremembered. Which chain a node ended up on
was decided by arrival order.

Two failures stacked. The node had no way to compare chains, and no way to withhold participation
while it worked out which one was right. Either alone is survivable; together they turn one
unlucky race into a fork with miners and validators committed to both sides.

## What this changes

**A node that adopts someone else's chain does not participate until it has reviewed it.** One
gate — `ChainParticipationGate` — with four states (Ready, IbdRunning, CandidateReview,
Quarantined), consulted by mining, both validator paths, compute, and the three sync-status RPCs.
It closes when an IBD starts, not when one finishes, because `staging.commit()` happens partway
through: waiting for success leaves a window where the node is already running the new chain and
still signing on it. It is persisted, because a quarantine a restart clears is not a quarantine.

**Chains are compared on verified evidence, never on claims.** Peers' offers are collected in a
registry keyed by chain rather than by peer, so losing a connection is a source failover rather
than a reason to redo the decision. What a peer says about its chain arrives as `ClaimedBlueWork`,
a newtype with no ordering against verified work: the compiler enforces that a claim may decide
what order to CHECK candidates in and nothing else. Adoption requires a validated pruning proof
and compares verified tip work under the canonical `(blue_work, hash)` fork-choice order.

**Two permits, not one.** `CandidateValidationPermit` authorises spending resources to check a
candidate — reversible, cheap evidence. `CandidateAdoptionPermit` authorises replacing the chain —
issued only against verified superiority, and binding on the defender it was compared with, so a
permit cannot be redeemed after the situation has moved.

**DNS scores never rank chains.** A verified trusted checkpoint (`--trusted-checkpoint`) is a hard
admissibility constraint: a chain that does not descend from it is refused outright. Within what
is admissible, GHOSTDAG decides. A DNS seed can therefore restrict the set of acceptable chains
and cannot choose among them.

## What this does not claim

This is convergence after a fork, not prevention of one. It does not address UTXO, overlay, or
registry consensus determinism — if two nodes disagree about the result of applying the same
blocks, everything here still runs, and everything here still converges them onto the same
*chain* while they continue to disagree about its *state*. That is a separate problem and this
ADR does not touch it.

## Consequences

**A node held back is a node not mining.** The gate fails closed, so every bug in it costs
availability rather than safety. That is the intended direction and it is not free: the review
floor is 180 s after an IBD, extended while any proof-backed challenger is still being weighed.

**Restarting mid-IBD quarantines the node.** An interrupted IBD leaves the active consensus in a
state nothing can vouch for, so the node comes back Quarantined and stays there until an operator
looks. This is deliberate and it is operator-visible toil; the alternative is guessing.

**The switch cap is 5 and it persists.** Two branches trading the latch is a different failure
from being on the wrong one, and a node cannot tell them apart alone. At the cap it quarantines.

## What convinced us

Unit tests cover the policy. They are not what found the defects.

The defects were found by a randomized soak (20 rounds, seeded per round, latency 5–500 ms,
peer order and arrival gap randomized, link cut and healed mid-flight) against two independently
mined chains past pruning depth — and by one round on real infrastructure across a 267 ms
intercontinental hop, which crashed the node within a minute through a state loopback is too fast
to produce.

Five defects in this work shared one shape: **the information needed to make the decision existed,
and nothing was driving the step that would have used it.** Discovery, expiry, adoption, and
verification each had an edge-triggered path with no level-triggered fallback. The fourth instance
sharpened the rule enough to state it: a driver placed where work is *scheduled* rather than where
the resource is *free* never runs for the peer whose resource is busiest — which is reliably the
peer that matters. That rule is now recorded next to the tick it explains.
~~~~~~~~
<!-- ADR_SOURCE_END:0025-chain-participation-and-ibd-candidate-selection.md -->

</details>

<a id="source-0055-palw-position-is-earned-not-declared"></a>

<details>
<summary>0055-palw-position-is-earned-not-declared.md — 原文 L1–L97</summary>

[原文を開く](../adr/0055-palw-position-is-earned-not-declared.md) · SHA-256: `d11892da8694085b7581da28efa0940cc91198069f7afb606243c0f1a3b6fb67`

<!-- ADR_SOURCE_BEGIN:0055-palw-position-is-earned-not-declared.md -->
~~~~~~~~markdown
# ADR-0055 — Chain position is earned, and the question is set by the block

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted
- Date: 2026-08-27
- Supersedes: nothing. Extends ADR-0044 Decision 6; reverses one relaxation recorded in
  `palw_state_v2.rs` (the court's opening rung).
- Declared by: `PALW_ATTEMPT_V2_VERSION` 4 → 5.

> **Note (index reconciliation, 2026-09-02).** "No shipped preset carries `ConsensusV2`" was
> written on 2026-08-27 and is no longer so: testnet-11 and devnet ship the V2 bundle
> (`palw_rc_shipped_params`, `devnet_shipped_params`), so Decision 3's window is the RC genesis's.
> Decision 1's "a receipt header buys no chain position" is the state [ADR-0073](0073-real-demand-work-bears-the-weight.md)
> Phase ④ proposes to change (open). Map: [`README.md`](README.md).

## Context

The 2026-08-21/08-27 adversarial audit returned eleven criticals. Three of them are the same
mistake told three ways: **a party was allowed to supply the thing it was going to be judged
against.**

1. A receipt-lane (algo-7) header's digest is free to re-roll — nothing in it costs anything to
   produce. `calc_block_level_check_pow_layer0` already refused to sell pruning-proof LEVEL for
   that price. Nothing refused to sell fork-choice WEIGHT: every merged receipt block still added
   `calc_work(bits)` to its descendants' blue work. The lane's meter, the ADR-0044 quantum ticket,
   draws against a beacon derived from the candidate's own chain — so it can only run on a chain
   candidate and cannot gate DAG entry at all. A merged-but-never-candidate receipt block never
   faced it.

2. A capture states its own job id. The seat compared the capture's roots against the claim's
   roots; the challenger re-executed the anchor **named inside the capture** and compared. Both
   agreed for material that answered a question no block ever asked, so one gossiped capture was a
   re-usable asset: mine a fresh block, announce the borrowed roots, and neither half objects. One
   inference, unlimited blocks, by parties that ran nothing.

3. The court's opening rung ran on the whole session budget rather than the rung clock. That was
   right when it was written — `CourtDisclosed` was constructed nowhere, so silence there could not
   fairly convict — but by the time a responder shipped, the backstop's effect had inverted: it
   ended the dispute on the CHALLENGER's side, which makes silence the winning move for a guilty
   producer.

## Decision

**D1. A receipt header buys no chain position of either kind.** One predicate,
`pow_layer0::algo_id_carries_no_chain_position`, answers for both the block level and the blue
work, and both sites call it. A receipt block contributes zero added blue work.

**D2. The anchor is derived from the claim's block, never read off the material.** Its four inputs
— network domain, the accepted block's pre-PoW hash, the class id, the executor bond outpoint — are
all facts a third party reads from the chain, so every verifier derives the value the producer was
forced to use. `PalwClaimRootsV1` carries it; `verify_material` refuses material whose
`job_context.job_id` is not it; the challenger re-executes the derived anchor. A verifier that
cannot derive it (pruned block, family with no canonical job) declines to judge rather than judging
without it.

**D3. The opening rung takes `turn_deadline_daa` like every other rung.** A silent responder loses
the claim, as it does at every later rung. This is a genesis-time obligation on the bundle: the
window must be wider than the responder's own cadence (the panel wakes on a 2-second tick and
answers from a capture it already holds), and a bundle whose window is tighter than its own
software convicts honest producers.

**D4. A production close carries the operand rows the court will read** — recorded by running the
real adjudicator through `PalwRecordingOracleV1` against the full inventory and opening exactly
what it resolved. The enumeration is asked of the adjudicator rather than written a second time on
the prover side, because a second enumeration agrees today and diverges the first time a kernel
changes which operand it touches — in the direction where an honest producer cannot close.

## Consequences

- D1 and D3 change consensus outcomes without changing any params field, so neither would have
  moved `palw_ruleset_id_v2` on its own. An old binary and a new one would have agreed on every
  block and disagreed on whether a producer was slashed and on which chain was heavier — a silent
  fork rather than a refused handshake. `PALW_ATTEMPT_V2_VERSION` 4 → 5 is where that is declared;
  it moves the fingerprint of every network carrying `ConsensusV2` and re-mints them.
- D2 adds `accepted_block` to the seat-duty, disputable-claim and court-duty views, and one header
  read per claim under judgement on the panel's cadence.
- No shipped preset carries `ConsensusV2`; a V2 bundle arrives with a network's genesis, so D3's
  window is chosen there.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0055-palw-position-is-earned-not-declared.md -->

</details>

<a id="source-0058-palw-merged-work-is-counted"></a>

<details>
<summary>0058-palw-merged-work-is-counted.md — 原文 L1–L132</summary>

[原文を開く](../adr/0058-palw-merged-work-is-counted.md) · SHA-256: `ed797e37e602f19d1d47dfb54452990ddfe9a17af5cf062995ddc0f5b04fb15a`

<!-- ADR_SOURCE_BEGIN:0058-palw-merged-work-is-counted.md -->
~~~~~~~~markdown
# ADR-0058: Merged work is counted — the mergeset carries claims, not just the chain

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted
- Date: 2026-08-27
- Depends on: ADR-0038 (PALW is consensus work), ADR-0039 (per-class DAA), ADR-0045/0056
  (share economy), ADR-0054 (share follows production)
- Supersedes: nothing; amends the transition's step 4 and the meaning of "produced" everywhere
  a counter reads it

## The defect, measured

On testnet-11 (`bb0a3ad3…`, 2026-08-27), the Qwen3.6 class produced 15 blocks by real
inference. Zero of them are on the selected chain; 743 floor blocks are (542 of 546 chain
slots). The margin each Qwen block lost by — 2M to 14M blue work, i.e. 2 to 10 floor blocks —
is its own inference latency: at 5–19 minutes per block against the floor's ~1 per minute, a
Qwen tip is always stale by the time it exists. The probability of losing 12 of 12 by chance
is ~4×10⁻⁷. This is structure, not luck.

The structure: a block's PALW attempt is applied to chain state only when the block joins the
virtual selected chain, and ordinary tip selection on a V2 network is deliberately blue-work
ordered (the pre-validation heap cannot evaluate the PALW comparator — see the wedge history in
`palw_tip_weights_v1` — so the one comparator runs at the deep-reorg gate, which a tip that
*extends* the sink never reaches). Both halves are individually correct. Together they mean a
slow-cadence class **cannot create claims at all**:

* no claim → `epoch_counters` never move → the per-class retarget's `observed` is 0 forever
  (`observed == 0 → continue`), so difficulty never eases;
* no counter → ADR-0054's growth walk sees "produced nothing" forever, so the share step the
  class was promised for filling its budget can never fire;
* no claim → no weight, so the work secures nothing and pays nothing.

Difficulty and share were designed to follow *measured production*, and the measurement was
wired to a race a slow class structurally loses. Every entrant class heavier than the floor
starves on arrival, at any share, at any difficulty.

## The decision

**A chain block applies the PALW work of its whole mergeset — blues and reds alike — not just
its own.**

Reds are not an edge case; on this network they are the point. The frozen 120 s cadence fixes
`ghostdag_k = 1`, so any block whose anticone holds two or more blocks is a red *by
construction* — which is every block of every class slower than the floor. All twelve real
Qwen3.6 blocks measured above are reds. A blues-only rule would have measured nothing.

1. **Step 4 of the transition** takes, after the block's own work, the work of every merged
   block of its mergeset (selected parent excluded — it applied its own work when it was the
   chain tip; non-DAA blocks excluded, matching the coinbase's pay set), in consensus mergeset
   order, reds included.
2. **Acceptance-context admission.** Each merged work passes the full stateful admission
   (`check_palw_attempt_admission_v2`) against the accepting block's live fold state — the same
   state, re-checked sequentially, so two merged blues cannot both take the last budget slot or
   the last sompi of exposure headroom. The stateless half (shape, challenge binding, executor
   signature) is checked once in the processor, against the carrying header.
3. **Refusal skips; it never disqualifies.** The accepting block did not author its anticone. A
   merged work the state refuses (budget exhausted, bond retired meanwhile, duplicate, exposure
   ceiling) is skipped deterministically — every node, same state, same order, same verdict —
   and the block stands. The block's OWN work still disqualifies on refusal, unchanged.
4. **The claim records the carrying block.** `accepted_block` is the merged blue itself — the
   panel derives the job anchor from the carrying header's pre-PoW hash, and the producer bound
   its material to that block. `accepted_daa`/`accepted_blue_score` are the accepting chain
   block's — deadlines and the safe frontier are chain-order facts.
5. **A merged claim escrows nothing (`escrowed_reward = 0`), this revision — and the coinbase
   pays an entitled in-window RED to its own miner script.** Blues were already paid to their
   own miners; entitled reds' worker shares were lumped into the *merging* miner's red reward,
   which under this ADR would have put the slash exposure on one key and the pay on another.
   The red's share now goes to the red's own script, through the same carve arithmetic, gated
   to `ConsensusV2` networks so every other network's coinbase stays byte-identical.
   Making merged claims escrow instead would require the coinbase to know the transition's
   outcome before the transition runs (they validate in that order). Today an entitled merged
   block is paid with *no claim, no verification and no slash exposure*; under this decision it
   is paid the same but its claim now exists — panel-verified, court-triable, bond-slashable
   (`reserved = pwu × slash_value_per_pwu ≫ carve`). Strictly tighter than the status quo.
   Symmetric escrow (withhold every applied merged block's carve, release at `Final`) is left
   as a follow-up that restructures coinbase validation ordering.
6. **Zero-escrow claims enqueue no payout.** A `Final` with `escrowed_reward = 0` writes no
   payout row, so no zero-value coinbase output can exist.

## What this closes

* **The retarget loop closes.** `apply_attempt` bumps `epoch_counters`; merged production now
  counts, `observed > 0`, the per-class DAA eases toward the class's budget share.
* **ADR-0054 closes.** The growth walk reads the same counters; a class that fills its budget
  from the anticone steps its share up exactly as promised.
* **ADR-0038's premise is restored.** PALW weight was "the network's whole fork choice", but
  only chain blocks minted weight, so a class that lost parent selection contributed no
  security. Merged claims mature into `safe_weight` on every chain that merges them; a private
  fork that excludes them now weighs less than the public chain that counts them.
* **Pay-without-verification narrows.** The entitled merged block's instant payment existed
  with no claim behind it; now the claim, panel duty and slash path exist for every applied
  work — and for reds, the payment finally lands on the key that carries the slash.

## Consensus impact — read before deploying

* `PALW_STATE_V2_VERSION` 9 → 10. Every state root moves from genesis.
* The params fingerprint now hashes `PALW_STATE_V2_VERSION` explicitly. Every previous bump
  moved the handshake only because it happened to ride a bundle-shape change; a semantic-only
  bump like this one would have left old and new nodes peering and then silently disqualifying
  each other's chains. The version is block-validity-relevant, so it is hashed.
* **testnet-11 must re-mint.** Old-fingerprint nodes refuse new ones at handshake (that is the
  point). Artifacts, class ids, pins and bonds' keys are untouched; genesis and the network
  fingerprint change.

## Rejected alternatives

* **Weigh the candidate tip's own attempt in fork choice.** Inverts the maturity principle
  (fork choice would trust unverified claimed pwu), and mirrors the bug instead of fixing it:
  if a Qwen tip always out-weighs floor tips, floor blocks stop being chain blocks and *their*
  production stops being counted.
* **Throttle the floor operationally.** The floor is permissionless; a fairness property that
  depends on volunteers slowing down is not a property.
* **Count merged blocks into the counters without applying claims.** Difficulty would ease and
  share would grow for work nobody verified and nobody can slash — production without
  accountability, worse than the defect.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0058-palw-merged-work-is-counted.md -->

</details>

<a id="source-0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms"></a>

<details>
<summary>0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md — 原文 L1–L100</summary>

[原文を開く](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) · SHA-256: `c762122c754cccec9c788db8796bf079e6770a4765b98996cb19475105938596`

<!-- ADR_SOURCE_BEGIN:0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md -->
~~~~~~~~markdown
# ADR-0127 — PALW settles on its own, and its terms are not DNS terms

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule changes and no fence: this ADR names things, states a property the code already has,
  and makes CI keep it.
* Operator's direction, in the operator's words: "PALW は DNS finality や BFT バリデーター依存ではないことを
  明記して"; "PALW Final ≠ DNS Final / PALW Settlement Anchor ≠ DNS Anchor / PALW Panel ≠ DNS Validator を
  コード・型・ドキュメント上でも完全に分けるべき — Anchor Finality という言葉も避けて PALW Settlement
  Finality / PALW Settlement Anchor / PALW Future Anchor くらいに統一"; "PALW settlement path から
  dns_confirmation、validator、beacon が一切参照されないことを CI で保証する".
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority),
  [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D2. The double-spend defence
  built on these terms is [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md); the DNS
  overlay that runs beside PALW is [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md).

## 0. The sentence this ADR is

**A PALW chain block that carries an attempt is a PALW Settlement Anchor; it is settled when its claim is
`Final`, and so is everything its selected chain accepted at or below it; that path — producer, claim,
future anchor, panel, receipts, court, `Final`, safe frontier, fork choice — reads no DNS confirmation,
no validator and no beacon, and a test fails the day it does.**

## 1. The names, kept apart

| PALW term | means | is not |
|---|---|---|
| **PALW Settlement Anchor** | a selected-chain block carrying an attempt (an attempt-class algorithm id); its claim is the anchor's claim | a DNS anchor (an epoch's canonical lagged block the overlay attests) |
| **PALW Settlement Finality** | the anchor's claim reached `Final`; the anchor is at or below the chain's safe frontier | DNS finality (a validator vote, ADR-0128) |
| **PALW Future Anchor** | the chain block at `accepted_daa + anchor_delay` whose hash seeds a claim's panel draw `H(anchor ‖ claim ‖ bond)` | a beacon (nothing is folded or attested) |
| **PALW Panel** | seats drawn from PALW bonds that re-execute a claim and sign receipts | DNS validators |
| **settlement depth** | settled anchors at or after the block that accepted a payment (ADR-0129) | confirmations, blue score or DAA distance |

"Anchor Finality" is not used. Execution blocks (ADR-0125) and heartbeats are never anchors.

## 2. Decisions

**Decision 1 — settlement is `Final`.** The safe frontier is the deepest anchor whose claim is `Final`
(`PalwChainStateV2::safe_frontier`); fork choice orders candidates by it first
(`compare_palw_candidates_v1`), and a deep reorg must strictly win that order (`decide_deep_reorg_v2`).

**Decision 2 — the terms are the documentation's and the operator surfaces'.** ADRs, RPC documentation
and the CLI say PALW Settlement Anchor, PALW Settlement Finality and PALW Future Anchor for these things,
and reserve "anchor", "confirmed" and "final" without the PALW prefix for nothing on the PALW path.

**Decision 3 — the guard.** A consensus-core test reads the PALW V2 settlement modules —
`palw_state_v2`, `palw_panel_v2`, `palw_attempt_v2`, `palw_admission_v2`, `palw_fork_choice`,
`palw_fork_authority_v2`, `palw_panel_economy_v1`, `palw_reward_v2`, `palw_producer_v2`,
`palw_execution_lane_v1` — with comments and string literals removed, and fails naming the file and line
if any mentions `dns_finality`, `DnsParams`, `dns_params`, `DnsState`, `dns_confirm`, `vlt`/`Vlt`,
`StakeAttestation`, `StakeBond`/`stake_bond`, `ActiveBondView`, `validator`/`Validator` or
`beacon`/`Beacon`. It passes on the tree it landed on, and CI runs the suite it lives in.

## 3. PALW does not depend on DNS finality or on BFT validators

A PALW network with no overlay, or an overlay whose validators have all stopped, produces, licenses,
finalizes, settles and orders exactly as it does with them. Where a network runs the DNS overlay, its
stake reorg gate — a bonded-stake BFT vote past ADR-0128's height — can refuse a reorg that abandons a
DNS-final anchor; it selects no tip, makes no claim `Final`, moves no safe frontier and decides no block's
validity, and it lapses when its validators stop reaching quorum. That veto is layered on PALW; PALW is
not layered on it.

## 4. Where the separation is not yet complete (named, not hidden)

* The PALW **V1** lineage modules (`palw_credit`, `palw_facts`, `palw_job_panel`, `palw_carriage`,
  `palw_block_commitment`) still read overlay bond records. They are not on the V2 settlement path the
  guard covers; whether any network still validates history with them decides whether they are deleted.
* `palw_registry` derives a runtime class id with a helper that lives in `vlt.rs`, and kaspad's PALW
  producer, panel and round producer load their ML-DSA key with `kaspa-pq-validator-core`'s loader.
  Both are shared utilities under an overlay name, not dependencies on validators; moving them under a
  neutral name is housekeeping this ADR records.
* `palw_state_v2.rs` sums a ConsensusV2 coinbase's output cap from `PALW_V2_MAX_VALIDATOR_PAYOUTS` and
  `PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS` — the slots the coinbase reserves for the overlay's §E
  participation and deferred quality-bonus payouts, since a V2 block's coinbase pays validators too
  (ADR-0126). No claim, frontier or settlement decision reads them. The CI guard found them where a
  case-sensitive scan had called the path clean, and pins their exact use counts (3 and 2,
  `KNOWN_UNSEPARATED`), so a new use fails as a removed one does; moving them beside the overlay's
  payout code is housekeeping.

## 5. Tests

The guard and its comment-and-literal stripper.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md -->

</details>

<a id="source-0129-a-double-spend-needs-the-anchors-not-the-blocks"></a>

<details>
<summary>0129-a-double-spend-needs-the-anchors-not-the-blocks.md — 原文 L1–L139</summary>

[原文を開く](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md) · SHA-256: `b44bcee7cecff32b7d275790d99922942f3f0c8fbad25089c5e3b2de5b0da471`

<!-- ADR_SOURCE_BEGIN:0129-a-double-spend-needs-the-anchors-not-the-blocks.md -->
~~~~~~~~markdown
# ADR-0129 — A double spend needs the anchors, not the blocks

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule and no fence of its own: it states which existing rules defend a payment, pins each
  with a test, and adds the read that counts confirmations in anchors. testnet-11 runs it from DAA 6,001
  with the execution lane at one block a second (ADR-0125 §7.5) and the seat exposure (ADR-0124) armed at
  the same height.
* Operator's direction, in the operator's words: "『モデルを交互にする』だけを二重支払い防止の主役にしない
  方がいい"; "Anchor Finality / Panel quorum certificate / Execution block の Finality weight = 0 / 同一 UTXO の
  競合を DAG 順序で一意化 / Producer・Panel equivocation slash / model・operator share cap / 高額決済の追加
  confirmation"; "10 BPS は UX/throughput、120 秒 PALW Anchor は security/finality と役割を完全に分ける";
  "これを ADR129 として記述して実装して — DAA 7001 から BPS1 になることを想定して進めている".
* Builds on: [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (execution blocks), [0127](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md)
  (the settlement anchor and its names), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (what a seat holds and loses), [0072](0072-the-ticket-is-the-execution.md) (an attempt is its header
  position), [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority).

## 0. The sentence this ADR is

**A payment is as final as the settled PALW anchors at or after the block that accepted it — never
the blocks — so a thousand execution blocks are zero confirmations; settling a conflicting branch takes
both the compute that produces its anchors and a licensing quorum of the seats drawn on it; spends that
conflict inside the DAG are ordered, not raced; and large payments wait for more anchors.** Alternating
models is not a defence: one operator can run every model.

## 1. The attacks

1. **The burst.** Produce many fast blocks and present them as confirmations.
2. **The private branch.** Pay a merchant on the public chain, build a branch that spends the same
   output elsewhere, and publish it once the merchant has delivered.
3. **The race.** Put two spends of one output into two blocks that arrive together.
4. **The double signature.** Sign two blocks for one permit, or two receipts that license two histories.
5. **The monopoly.** Hold enough of the lane to order its transactions at will.
6. **The large payment.** Any of the above, against a sum worth the attacker's capital.

## 2. Decisions

**Decision 1 — execution blocks carry no finality.** A round block (ADR-0125) is never a selected
parent, always red, outside the DAA set, of zero blue work and zero subsidy, and is never a claim; a
heartbeat is ADR-0066's. None of them moves a safe frontier, a safe weight, a live total, a blue work or
a DAA score, so no number of them is a confirmation. Pinned: merging permitted round blocks leaves the
sink's fork-choice inputs exactly as they are without them.

**Decision 2 — confirmations count settled anchors.** A PALW settlement anchor is a chain block carrying
an attempt; it is settled when its claim is `Final` (ADR-0127). A payment accepted by the chain block at
DAA `d` has **settlement depth** = the settled anchors at or after `d` on the node's chain.
`getPalwSettlement` (op 182) answers it with the anchors still pending; `misaka palw settlement --daa d
--min-depth N` prints it and exits non-zero until it is reached; `misaka wallet utxo list` shows each
output's depth. A thousand execution blocks and no new settled anchor is depth 0.

**Decision 3 — fork choice follows settlement.** The safe frontier (the deepest settled anchor) is the
comparator's first key and a deep reorg must strictly win the comparator (ADR-0042 Decision 9), so a
branch replaces the public chain only by settling deeper than it.

**Decision 4 — the panel quorum signs each anchor's state.** A seat's receipt signs the claim id; the id
is the attempt's; the attempt carries `challenge_v2(network, pre_pow_hash, timestamp, nonce, class,
bond)`, which admission checks against the carrying header; and `pre_pow_hash` covers the header's
transaction merkle root, accepted-id merkle root and UTXO commitment. A licensing quorum is therefore a
quorum over the anchor's transactions and the state they leave — at one block a second that is the
roughly 120 execution blocks the anchor merged at testnet-11's cadence — and it cannot be moved to a
block with other contents. Pinned (ADR-0127's binding test).

**Decision 5 — a settled double spend takes two resources.** To make a conflicting branch settle, an
attacker must (a) produce that branch's anchors — winning attempt draws, each an inference, because a
block's draw is its header position (ADR-0072) — and (b) license each of their claims with a quorum of
the seats drawn for it *on that branch*: honest seats judge only claims on the chain they follow, so an
unpublished branch settles only with three of five seats the attacker holds, claim after claim, until
its frontier passes the public one. The seats are drawn one ticket per bond above ten producer floors
(ADR-0124 Decisions 4–5; 100,000 BILI a seat on mainnet), so the second resource is bonded capital
across many bonds, and the draw's anchor on the branch is a block whose hash costs a winning draw to
re-roll.
*The limit, stated:* receipts for honest inferences on a private branch are not slashable on the public
chain — the seat capital is priced by the draw and the floor, not burned by it. What burns is a licence
of a bad inference (the court) and a seat that contradicts its quorum (three times the claim's exposure,
ADR-0124 Decision 3).

**Decision 6 — conflicting spends are ordered by the DAG.** A chain block accepts its mergeset's
transactions in GHOSTDAG order and the UTXO set admits the first valid spend; a round block's
transactions are accepted only through the chain block that merges it and only under a granted permit.
Two blocks carrying spends of one output produce one accepted spend. Pinned.

**Decision 7 — double signatures.** A permit signed twice is not relayed, is evidence, burns the permit
and slashes the bond's floor (ADR-0125 SA-2). A producer cannot sign one execution into two blocks: each
block is its own challenge and its own draw. A seat's receipts license one claim, which is one block
(Decision 4), so they cannot license two histories; contradicting verdicts are refused within one object
and unusable across objects (ADR-0124 SA-3) and are not slashed by name, because an honest seat can
sign both: the panel service answers a `(claim, bound_daa)` once but keeps that record in memory, so a
seat that answered `Unavailable` and restarted after the material arrived answers `Valid`, and a seat
redrawn onto a claim's second panel answers that panel afresh.

**Decision 8 — shares are capped, as auxiliaries.** A security domain holds at most 45 % of a span's
lane, a third of a round and never two consecutive rounds; an operator holds at most one permit a round
(tighter than the two in ten the design proposed). These bound a monopoly of ordering; they are not what
settles a payment (Decisions 2–5), and a model alternation would bind nothing an operator running every
model could not satisfy.

**Decision 9 — large payments wait for more anchors.** The depth a payment needs is its receiver's
policy, read from Decision 2. On testnet-11 past DAA 6,000 an anchor's claim settles a challenge window
(120 DAA) after its licence, so a payment's first settled anchor arrives some four hours after
acceptance at the 120-second cadence, and each later anchor adds one. A receiver of a large sum waits
for a depth whose anchors it would cost more to produce and license on a private branch than the sum is
worth; `--min-depth` is the switch a script or an exchange waits on.

## 3. What does not change

Every consensus rule, fingerprint and block; the lane's width (one permit a round on testnet-11 from DAA
6,001; nothing here depends on it); the DNS overlay, whose stake reorg gate (ADR-0128) can add a veto on
a network that runs it and is not needed for anything above.

## 4. Security amendments

* **SA-1 — depth is a lower bound beyond retention.** Settled claims retire from the state; past that
  horizon the read says the depth is at least what it counts.
* **SA-2 — the burst is inert by construction, and the pin is what keeps it so.** A change that gave an
  execution block blue work, a DAA score or a claim would turn Decision 1 off silently; the fork-choice
  pin fails first.
* **SA-3 — Decision 5 is only as strong as the draw is unpredictable.** On a private branch the panel's
  future anchor is the attacker's own block; re-rolling its hash is a new winning draw, which is compute
  per try, not a free nonce.
* **SA-4 — no validator is involved.** Nothing here reads the DNS overlay (ADR-0127's guard).

## 5. Tests

Execution blocks leave the fork-choice inputs unchanged (Decision 1); the binding chain from receipt to
UTXO commitment (Decision 4); two blocks spending one output accept one spend (Decision 6); the
settlement read, its RPC round trip and `--min-depth` (Decision 2); and the existing permit-equivocation,
one-ticket draw and frontier-first comparator tests (Decisions 3, 5, 7).
~~~~~~~~
<!-- ADR_SOURCE_END:0129-a-double-spend-needs-the-anchors-not-the-blocks.md -->

</details>

<a id="source-0178-fork-choice-heals-a-partition-on-bonded-participation"></a>

<details>
<summary>0178-fork-choice-heals-a-partition-on-bonded-participation.md — 原文 L1–L284</summary>

[原文を開く](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) · SHA-256: `18811fd0f13c44758eb53e8e9c712d783f2cbe79b83ef41b145847cb4c3bb6b9`

<!-- ADR_SOURCE_BEGIN:0178-fork-choice-heals-a-partition-on-bonded-participation.md -->
~~~~~~~~markdown
# ADR-0178 — The fork choice heals a partition on bonded participation: one search over every tip, economic keys counted over each branch's exclusive past, and participation first past `W_p`

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **INTERNAL (user decision 2026-10-08).** Finding V6 is kept out of every public text — docs, commit messages, explorer text, issues.
> This draft and its record live on the unpushed branch `fin/palw-finality-consistency`. V6 is named here only as "the stale-incumbent
> comparison"; its mechanics are in the internal record, §3.

**Numbering (2026-10-10):** drafted as ADR-0175; renumbered ADR-0178 because the user's ADR-0175 is "registered models are permanently
immutable". Code, tests and the record say ADR-0178.

**Status:** ACCEPTED — the user adopted rule E (2026-10-09). Implemented behind the dormant fence `palw_fork_choice_rule_e_v1`
(lane FINX successor, 2026-10-09; §9), refused when armed in this binary; it ships armed only in the single full-activation release,
which assigns its height. User decisions already taken (2026-10-08): no public advisory; no early dedicated fence; **the fix ships in
the single full-activation release**; `palw_dns_retirement_v1` may be armed only with rule E at or below it. Record:
[`docs/design/palw/finality-palw-consistency.md`](../design/palw/consensus/finality-palw-consistency.md).

**Builds on / amends (if accepted):** ADR-0042 Decision 9 (one comparator), the strict-win reorg rule (`palw_reorg_strict_economic_win`,
lane rcore/f1-strictwin-tie), the pruning-proof strict-economic commit (rcore/hf-pptake2), ADR-0065 D2 (frontier provenance — its "bonds
registered after the fork" set difference; note testnet-12 ships `palw_frontier_provenance: None`, so rule E carries its own copy),
ADR-0160 F-W (licence 1000‰, `Created` 0), ADR-0058 (merged work counts, blue or red).

## 0. The sentence this ADR is

**A node's sink is the best tip in one order that every honest node computes alike from the same blocks — bonded participation in each
branch's exclusive past first, then the economic keys counted over that exclusive past, then the incumbent — so an honest partition that
heals before the seal converges on the side more bonds worked on, and no branch wins by blue work, by arrival order or by being one
block fresher.**

## 1. Context

Devnet r1 (H1, 2026-10-08): a 41-minute partition healed on the wire and stayed split for good. FINX's question: are the GHOSTDAG
finality guard and the PALW fork choice consistent? They are not (record §0, §3; all MEASURED through the pipeline on testnet-12's
rules unless marked):

* **V1** the sink is GHOSTDAG's first acceptable candidate, not the comparator's maximum; a lighter branch is never weighed — not even
  UTXO-validated — though the node holds it and it is the comparator's maximum (`finx_p0_b`);
* **V2** a deep all-economic tie keeps the incumbent on both sides: an honest partition three DAA long with nothing economic anywhere
  never heals, symmetric or two producers against one (`finx_p0_a`);
* **V3** the finality point advances on the node's own heartbeats (600 blue: 154 slots at 3.9 blue a slot, 300 at 2.0 — 5.2–10.2 h)
  while the gate refuses; past the fork the gate is never asked again, even once the competitor strictly wins (`finx_p0_d`);
* **V4** IBD and relay decide the same pair oppositely: an old heavy-side datadir COMMITS the light chain its relay never weighs; an old
  light-side datadir refuses the heavy chain (`finx_p0_c`); **V5** arrival order decides for fresh nodes (`finx_p0_c`);
* **V6** (safety) the stale-incumbent comparison: the gate compares a candidate with the *previous* sink while the economic keys move
  with time; reversals past the shallow window without a bond (`finx_p0_e`, `finx_p0_f`; record §3). On the live chain the DNS BFT
  veto (ADR-0128, armed from DAA 0) bounds its reach to the DNS confirmation lag while the overlay is Active and confirming; RFC-0012's
  proposed retirement removes that bound — **so this fix must be active no later than the DNS retirement**;
* **V7** the relay skips the lighter side's blocks below the merge-depth root (code).

And a bound (record §2.3): no rule that reads only topology, blue work, hashes and the economic keys both refuses an attacker's private
branch and heals an economically tied honest partition — to the lighter node they are the same blocks. Healing needs an input a private
branch lacks: bonded participation is one the chain already has (an attempt is signed by its bond over a challenge that binds its
parents; it cannot be re-signed onto another branch; it is header data).

## 2. Options (MODEL, record §5; both applied challenge windows)

| | C1 | C2 | C3 | F | **E** | K |
|---|---|---|---|---|---|---|
| what | V2-max over every tip + relay below the merge root | + 4× blue-work override | portable weight out | hold finality on a refused heavier branch | C1 + C3 + participation first past `W_p` | E + 2/3-participation finality crossing |
| heals a tied partition | no | no | no | no | **yes, bonds active on both sides** | as E |
| heals r1 | yes, onto the 2-node side | no | no | no | **yes, onto the bond majority** | yes |
| heals past the seal | no | no | no | no | no | > 2/3-bond side only |
| V6 (stale incumbent) | reversed | reversed | **holds** | reversed | **holds** | holds |
| panel collusion | reversed | reversed | reversed | reversed | **holds** | holds |
| DA time bomb (court void after the seal) | holds | holds | holds | **reversed** | holds | holds |
| Sybil-fed fresh node | holds | captured | captured | captured | **holds** | holds |
| trusts blue work | no | **yes** | no | no | at an even split ≥ n/3 only | as E |

C2 is unsafe. F is a regression. C1 alone keeps V6. K changes finality's meaning; not needed now.

## 3. C3 alone against rule E — on V6, both forms, and on what else they do

V6 has two forms (record §3): a *carrier* form (a branch that carries a pending licence of a claim both branches hold, before the
incumbent does) and a *clock* form (a branch whose clock runs one slot ahead crosses the `Final` height of a claim both branches hold
first). Both need a claim **both branches hold** whose economic contribution changes with time.

| | C3 alone | rule E |
|---|---|---|
| V6, carrier form | **closed** — the claim is portable, its licence counts on neither side | **closed** — same keys, and participation outranks them past `W_p` |
| V6, clock form | **closed** — the claim's `Final` is portable | **closed** |
| V6 by merged borrowing: the attacker forks earlier and merges the public attempts made between the fork and the payment, so their claims are "above the fork" on both branches | **closed only if "portable" means "accepted in a block in both tips' past"**; reopened if it means "accepted at or below the fork's blue score" | **closed only with the same definition** for the keys AND for participation — counted over each tip's exclusive past, the merged honest attempts cancel; counted "above the fork", they inflate the attacker's participation |
| panel collusion (a quorum-holding attacker self-licensing its own branch's claims) | **reversed** — those claims are exclusive to its branch, and count | holds — participation first |
| liveness: tied partitions | split (as today) | heals where bonds were active on both sides |
| liveness: a portable licence on the heavier side (today: heals) | **split** — the licence no longer counts, the tie keeps the incumbent: a regression | heals |
| liveness: devnet r1 | split | heals onto the bond majority |
| cost | a walk of both tips' exclusive pasts per non-extension comparison (bounded by the finality depth); the search unchanged | the same walk, plus participation (attempt headers, same walk) and C1's search over every tip and relay below the merge root |
| fingerprint | one fence; no block-validity or state-root change | one fence; no block-validity or state-root change |

**Recommendation (PROPOSED): rule E**, with both definitions taken over each tip's **exclusive past** (blocks in one tip's past and not
the other's) — not "above the fork's blue score". On V6 the two rules are equal once that definition is used; C3 alone leaves panel
collusion open and makes one more class of honest partitions permanent, while rule E closes V6, closes panel collusion, and heals the
partitions P0 is about. Since the fix ships in one full-activation release either way, rule E's larger cost is paid once, behind the same
fence. Choose C3 alone only if the release must carry the smallest possible fork-choice change; then accept that partitions stay as
today or worse, and that panel collusion stays open.

## 4. Decision (ACCEPTED: rule E; as implemented)

Under one dormant fence, `Params::palw_fork_choice_rule_e_v1` (a height, read at the INCUMBENT's DAA), past which:

1. **One order.** For two tips `a`, `b` with common selected-chain ancestor `F` (chain reachability): each side's **exclusive
   claims** are the claims its own chain accepted above `F` (accepted blue score above `F`'s) whose id the other tip's state does not
   hold — so a claim both tips hold, or one accepted at or below `F`, decides nothing (this equals "carrying block outside the other
   tip's past" except where the other tip's fold refused a merged attempt this tip accepted; it is the form a header-verified
   client can check). **Participation** is the number of distinct executor bonds of the side's exclusive attempt claims that are
   registered in `F`'s registry — the common past (a losing draw and an unbonded header make no claim; a bond registered after `F`
   never counts). The **economic keys** `(frontier, safe, live)` are the fold's own per-claim expressions, priced at each tip, summed
   over the exclusive claims (F-W's per-bond cap applied over that set; the frontier by the resolved-prefix rule restricted to it;
   past `palw_bond_budget_v1`, each `(bond, window)` at most its `F_max` — §10). Every side goes through one accumulator over
   per-claim records, the one a client reading the fork-choice leaf v2 calls; a node streams the records from its state, with no
   claim-count refusal (§10.2). Order:
   participation first **once the lower tip stands at least `W_p` = 20 DAA above `F`** (symmetric in the pair; for an incumbent
   facing a branch at least as long, its own history since the fork), then the economic keys.
2. **The gate.** A non-extension candidate replaces the incumbent on a strict win; a tie goes to GHOSTDAG's order where
   participation counts and each side reaches `max(1, ⌈n/3⌉)` of `F`'s `n` bonds, else to strict-win's shallow question, else
   the incumbent. Unweighable refuses. ADR-0065 D2 still applies after an allow. The absolute orders are not read.
3. **One search.** After its first acceptable candidate (GHOSTDAG's heaviest the gate admits), the sink search goes on: up to 256
   heap entries are ranked by **header-level participation** (registered bonds' attempt headers above the fork and outside the first
   candidate's past — headers and reachability only); entries with none, slot races of the first (both within 2 DAA of the fork) and
   entries in its past are passed over without a validation; the top 8 are UTXO-validated, gated, and each admitted one replaces
   the best on a win in the order of point 1 (the gate's tie rule). Where the first stays best the answer is byte-identical to the
   status quo's; where a lighter tip wins, the virtual merges only what is lighter than it.
4. **Relay below the merge-depth root.** A non-heartbeat block below the virtual's merge-depth root is validated rather than skipped
   (never announced), within a per-peer budget (16 at once, one per 15 s); its missing ancestors arrive as orphan roots. A heartbeat
   below the root is still skipped.
5. **IBD commits by the same order.** `validate_staging_palw_order` asks `palw_rule_e_ibd_commit_v1`: the claim-set difference of
   the local sink's state and the staged pruning point's state (a claim older than the other state's retirement horizon plus a
   600-DAA merge slack is neither side's), participation over the bonds both states register (no fork block is known there) and
   counted (the headers-proof path's entry conditions put the fork far deeper than `W_p`), ties keep the incumbent. Fail closed as
   before.
6. **The ordering rule.** `validate_palw_v2` refuses `palw_dns_retirement_v1` unless rule E is armed at or below it, and refuses
   rule E armed at all in this binary — last, after every other refusal.
7. **Finality is unchanged.** `finality_depth` stays `window_challenge / 2`; no freeze.

## 5. Consequences

* **Heals** every honest partition shorter than the seal (154–300 slots, 5.2–10.2 h on testnet-12) in which bonds attempted on both
  sides — on the side more bonds attempted on, `max(0, W_p − D)` DAA after the heal (MODEL: +22 at D = 3, +5 at D = 20, +1 past `W_p`).
  r1 converges on the majority's branch, and B's old datadir commits it.
* **Holds** against every adversary modelled: tied heartbeat branches, junk headers, both V6 forms, panel collusion with ground panels,
  the DA time bomb, Sybil-fed fresh nodes. Assumption, stated: **the attacker holds fewer bonds than the honest bonds active in the
  public branch's exclusive past over `W_p`**. An attacker that ties honest participation at a third of the bonds or more is decided by
  GHOSTDAG (blue work).
* **Residuals, named.** (a) A partition in a heartbeat-only period (no bond attempts on either side) stays split — no unforgeable input
  exists then (record §2.3); the node-local partition watchdog (LIVE-R1's N2) and a resync with `--checkpoint` are the remedy; bonded slot clocks
  would remove it (out of scope). (b) A partition longer than the seal stays split, as on every Kaspa network; K is the protocol answer if
  ever wanted. (c) The assumption is about *active* bonds.
* **Cost.** No block-validity or state-root change. Per non-extension comparison: the exclusive pasts of both tips (bounded by the
  finality depth), their attempt headers, and the PALW deltas over them. The search validates lighter branches it used to skip — bounded by
  point 1's header-level pre-filter. **Not measured** (GAP): the cost on a node fed many junk branches.
* **Fingerprint.** One fence: scheduling moves the params id and the schedule id; activation moves the consensus identity id. Below it
  the build is byte-identical.

## 6. Ordering rule, and V6's live window under the DNS veto

**Hard prerequisite: the V6 fix (C3 or rule E) is active at or below the height that arms `palw_dns_retirement_v1`.** Today the DNS BFT
veto (ADR-0128, `dns_bft_gate`, armed on testnet-12 from DAA 0) is the only layer that bounds V6; RFC-0012's retirement removes it. A
schedule that arms the retirement without this fix at or below the same height must be refused — by `validate_palw_v2`, the way F-W
already refuses to arm without strict-win (`PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1`).

**V6's live window as a function of the DNS state.** Let `L` be the victim's anchor lag: its sink's DAA minus the DAA of its last
DNS-final anchor. The veto refuses every candidate that does not hold that anchor, and releases it once the anchor is stale on the
victim's own chain, `L > TTL` with `TTL = dns_veto_ttl_daa_score = 120` DAA (`confirmed_anchor_is_stale`). A reversal needs the fork at or
above the anchor (the anchor in both branches' history) and the payment above the fork, so a payment `d` DAA deep is reachable exactly
when `2 < d ≤ R`, with

| DNS state on the victim | reach `R` | at ~120 s a DAA |
|---|---|---|
| Active, anchor confirmed, `L ≤ 3` | none — the veto covers every depth past the shallow window | — |
| Active, anchor confirmed, `3 < L ≤ 120` | `R = L − 1` | up to 4 h at the TTL edge |
| anchor stale (`L > 120`: validators stalled), overlay not Active, no confirmed anchor, the gate abstaining, or the overlay retired | `R = F`, the finality depth in DAA (600 blue at 3.9–2.0 blue a slot: 154–300) | 5.2–10.2 h |

(Within `R`, the record's §3.1 practical bounds still apply: the carrier form closes once the public chain carries licences of its own
post-payment claims, ≈ 21–25 DAA while attempts flow; the clock form stays open until those claims turn `Final`, ≥ 141 DAA.) The live
`L` is not measured here (record §9); reading it off the fleet needs the user's approval.

## 7. What this ADR does not decide

`W_p` beyond testnet-12 (20 there; it must stay between the longest interval an active honest bond goes without an attempt and the
licence delay). K. The frontier-by-DAA follow-up. Bonded slot clocks. Stake-weighted participation (bonds count one each here).

## 8. Until the release (PROPOSED, node-local, no fence)

* LIVE-R1's N2 watchdog and the explained wedge warning — a wedged node says so and stops participating.
* The operator runbook for a sealed node: resync from an empty datadir **with `--checkpoint=<daa>:<hash>` of the network's chain**, so the
  fresh IBD cannot land on whichever side serves it first (V5).
* Internal operational guidance for the stale-incumbent comparison is in the record, §3 (not for publication).

## 9. Implementation and verification (2026-10-09)

Code (all behind the fence; below it the build is byte-identical):

* `consensus/core/src/palw_fork_choice_rule_e_v1.rs` — the fence's validation and the ordering rule, the side computation over a
  caller's exclusivity predicate, the order and the decision, the IBD commit over the claim-set difference, the relay's policy and
  its per-peer budget; unit tests.
* `consensus/src/pipeline/virtual_processor/palw_rule_e.rs` — the node's inputs (states, reachability exclusivity, the fork span,
  the slot-race test, header-level participation), the gate and the search's continuation. `dns_reorg_outcome` and
  `sink_search_algorithm` call them past the fence.
* `protocol/flows`: the IBD commit and the relay below the merge-depth root; `ConsensusApi::get_palw_rule_e_weighing_v1`.
* `consensus/core/src/palw_fork_choice_rule_e_leaf_v2.rs` — the fork-choice leaf v2 (agreed with lane L2FC2; the envelope and the
  header sites are theirs): window and registry trees, openings, provers and verifiers, and the client's pair. Note:
  `docs/design/palw/rule-e-leaf-v2-note.md`.
* Tests armed through the real pipeline: `hb_fork_choice_probe::finality_consistency::rule_e` (V1–V6 flipped, V7 by the relay
  policy's unit test, the merge-past attacker, a Sybil peer flood, Sybil bonds registered after the fork, the bounded search cost,
  three combined rejoin cases); the seven status-quo tests are unchanged and still pass unarmed. Results: the record, §10.

Residuals, named (in addition to §5's):

* **Merged work counts for neither side.** The GHOSTDAG-heavier side merges the lighter side's blocks while they are within its
  merge depth (≈ 16 slots of divergence on testnet-12's shapes), so those attempts become common and the merging side wins a short
  partition on its own exclusive attempts — the right chain (it holds both sides' work), but not "the bond majority" the model
  (which does not merge) reports for short partitions.
* **The continuation's bounds.** A lighter branch with no registered bond's attempt above the fork is never UTXO-validated, so a
  lighter branch whose only advantage would be exclusive free-prompt `Final`s is not weighed by the search (the gate still weighs
  it when it is the heavier). A flood can take the eight validations only with header-level participation at least the honest
  branch's — an attacker's own registered bonds (losing draws included, at the price of a signature each).
* **IBD ties keep the incumbent**, even at an even split (the relay decides those once the blocks arrive), and a network with no
  claims inside the retirement horizon cannot be told apart by the claim-set difference.

## 10. ADR-0176 and ADR-0177 applied to rule E (2026-10-10)

**ADR-0176 D3 / RFC-0014 §16.10 — Final weight is bounded per bond, and fork choice reads the same versioned allocation.**

* **Participation** counts a bond once, whatever number of claims it holds in an exclusive past, so speed buys none; a bond's
  attempt counts only once the fold accepted it, which past `palw_bond_budget_v1` means its claim budget `Q_max` was reserved.
  The header-level pre-filter of the search also counts bonds, not headers.
* **The economic keys** read each claim through `PalwRuleEBondBudgetV1` (the interface; lane BUDGET implements it, FINX does not).
  Where an allocation governs a claim, the record carries the allocated weights (Final: consumed at `Final`; live: the
  reservation's provisional weight) and the terms `(version, window, F_max)`; the side credits each `(bond, version, window)` at
  most `F_max` on the safe key and on the live total, whatever the engine reports. Where none governs it (below the fence; a claim
  accepted under an older ruleset, which completes under it — ADR-0176 §4) the record is priced as before. The selector
  `palw_rule_e_bond_budget_v1` is the one place every path (node, IBD, leaf v2) learns the allocation; today it returns "none".
* **Measured** (unit, `a_fast_forger_counts_once_and_its_budget_caps_its_credit`): one bond with 20,000 exclusive claims against an
  honest bond of the same budget that did its real work — participation 1 each, both credited exactly `F_max`, the pair ties; the
  same records without an allocation sum to 1,500× `F_max` (the pre-budget path ADR-0176 D3 forbids in the new ruleset).
* **Contract for BUDGET** (on the trait): `allocation` is a function of the state alone; `None` exactly where no allocation
  governs; `Some` gives the very weights the fold adds to the state's safe weight / bounded immature for that claim, so rule E's
  sum over a whole state equals the state's accumulators (BUDGET's acceptance test); it reads no availability input.

**A finding the budget work exposed (fixed here).** Rule E refused a comparison when a side's exclusive past held more than 16,384
claims. Under ADR-0160 S that is reachable by claim volume alone (at ρ = 1,000 a 13,000 MSK bond holds 2,000 outstanding claims and
issues 100 a DAA; the honest network's own volume at ρ ≥ 100 passes it inside the finality depth), and a refusal keeps the
incumbent — a veto bought with claim volume. A node now streams each side from its state (memory: bonds, budget groups, distinct
blue scores) and never refuses on volume; the bound stays only where records are materialized: a leaf v2 window (not built past it)
and a client's opening (the client STOPs). L2FC must decide the window's form at high ρ (the leaf-v2 note).

**ADR-0177 — nothing in fork choice depends on model availability.** Rule E's inputs are the fold's accepted claims (phase, bond,
attempt flag, weights), the bond registry, blue/DAA scores, headers and reachability. None is a model's distribution, a peer count,
an acquisition result or a lease. Two indirect readers, for the record: (a) a claim's phase is the fold's — an availability-based
void, if any dormant code still had one, would reach rule E through the phase; ADR-0177 removes such voids at the fold (lane DA16b),
and rule E adds none; (b) a claim's price reads class shares only under `palw_uncertified_weightless` (genesis-only: the devnet
and rc presets arm it at genesis, testnet-12 does not), and class shares derive from admitted claims (ADR-0132); a seat's readiness
(ADR-0135) proves the seat's own copy of the model it computes with — the producer's possession, not distribution to anyone else.
Whether ADR-0177 reaches seat readiness is a question for the Lead, not a fork-choice input. The participation key counts a voided attempt (its signature still lies above the fork), so no void — of any cause —
moves participation.

## 11. What blocks arming (2026-10-10)

* **Code:** BUDGET's engine behind the interface (`palw_bond_budget_v1`); L2FC's envelope and the leaf's form at high ρ; the relay
  below the merge-depth root has a unit test only — a real multi-node run (record §6) is the E2E.
* **Policy:** `W_p` beyond testnet-12; whether participation stays one-per-bond or becomes budget-weighted (a capital split into
  minimum bonds before the fork counts once per bond — the per-bond caps of ADR-0176 do not reach a count); the heights.
* **External:** review of the mechanism (internal) and the multi-node runs.

## 12. The 2026-10-10 drill stall is not this decision's problem (2026-10-10)

The 9-node drill on integration `0b73fd33f` stopped three times with "0 UTXO-validated blocks", each node on its own sink. The kept
datadirs hold one linear chain with one tip; the sinks are different blocks of it; the same binary, reopened on each datadir, moved
to the common sink at the first arriving block. It is a non-deterministic stall of virtual processing, not a fork choice: no
comparator is consulted where there is one tip, and rule E (armed) changes nothing there — pinned by
`finx_e_drill_1010_a_lagging_sink_on_one_chain_is_no_fork_choice_state`. Evidence and the next step (a stack of the stuck
virtual-processor thread): design record §10.5. Rule E's scope is unchanged — the comparator split of 10-08 (two branches).

## ユーザー方針 2026-10-15 による改定

* §10/§11の二つの未解決問題を[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §6のrule E v2(fence `palw_rule_e_participation_v2`、休眠)で閉じる: participationは`F`での資本で数え(分割で増えない)、voidされた・未認証のattemptを数えない(bondはVoided/未認証の判定の後でだけ集合に入る)。rule E v1の意味は変えない。
* DNS退役後の収束は**Gate Cとrule E v2の組**で行う。fork choiceの全keyは認証済みclaimだけから作る。両側が認証済みcreditを一切持たない区間はGHOSTDAG順で決める(PALWのfinalityがどちらにも報告されていないため、報告済みのfinalityは覆らない)。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。
~~~~~~~~
<!-- ADR_SOURCE_END:0178-fork-choice-heals-a-partition-on-bonded-participation.md -->

</details>

