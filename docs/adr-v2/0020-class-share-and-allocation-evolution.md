# ADR-v2-0020 — Class share・epoch配分・経済係数の変遷

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 7本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0004](../rfc-v2/0004-model-tiers-and-annual-catalog.md) / [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)

## 1. この文書の役割

class shareとモデル配分の設計変遷をまとめ、互いに異なる経済式を重ねない。

## 2. 統合した判断の読み方

### 2.1

初期のchain上share・epoch budget、実生産量による増減、Final仕事量を条件にする修正、未使用予算の段階解放を時系列で保存する。[旧ADR-0045](../adr/0045-palw-class-economy-on-chain.md) [旧ADR-0054](../adr/0054-palw-share-follows-production.md) [旧ADR-0107](../adr/0107-a-share-grows-on-work-that-reached-final.md) [旧ADR-0123](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md)

### 2.2

LLMを主役としfloorを最低限にする目標と、work targetからshareを結果として読む別案を区別する。[旧ADR-0068](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md) [旧ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)

### 2.3

係数の正当化は、計算・帯域・費用と裁定可能性に依存する。計測・探索だけの結果を採択済み価格表にしない。現在のモデル二階層は[ADR-v2-0019](0019-model-tiers-and-annual-catalog.md)から参照する。[旧ADR-0146](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)

## 3. 維持する境界・後続改定

この文書は経済式の変遷を持つ。異なる版のshare増加、work-target、bond配分を一つの新式に合成しない。旧数値は適用network／epochの履歴である。

## 4. 隣接する判断との分担

新しい配分係数は[ADR-v2-0019](0019-model-tiers-and-annual-catalog.md)、仕事の数え方は[ADR-v2-0026](0026-canonical-work-and-answer-accounting.md)、総発行上限は[ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0045-palw-class-economy-on-chain.md](../adr/0045-palw-class-economy-on-chain.md) | L1–L207 | L12–L21 |
| [0054-palw-share-follows-production.md](../adr/0054-palw-share-follows-production.md) | L1–L137 | L18–L20 |
| [0068-the-llm-primary-economy-and-the-floors-minimum.md](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md) | L1–L207 | L15–L64 |
| [0107-a-share-grows-on-work-that-reached-final.md](../adr/0107-a-share-grows-on-work-that-reached-final.md) | L1–L167 | L23–L34 |
| [0123-the-epoch-progressively-releases-unused-class-budget.md](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md) | L1–L62 | L12–L16 |
| [0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) | L1–L723 | L17–L24 |
| [0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) | L1–L293 | L12–L17 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0045-palw-class-economy-on-chain"></a>

<details>
<summary>0045-palw-class-economy-on-chain.md — 原文 L1–L207</summary>

[原文を開く](../adr/0045-palw-class-economy-on-chain.md) · SHA-256: `fa2ed1b821c117beafcf960fc6b3831c086dabe78038dff08031887d94bb4c8d`

<!-- ADR_SOURCE_BEGIN:0045-palw-class-economy-on-chain.md -->
~~~~~~~~markdown
# ADR-0045: The class economy is chain state — derived PWU, block-denominated epoch budgets, and the registration-granted share table

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Accepted (implemented on the V2 lineage; consensus-inert — no shipped preset carries
`ConsensusV2`).**
Date: 2026-08-20
Relates to: ADR-0038 Decision D (per-class difficulty domains), ADR-0039 Decision 5 and its
2026-08-17 amendment (the four defects and the blocked enforcement point), ADR-0042 Decisions
1/5/6 (atomic bundle, candidate-scoped state, admission list), ADR-0043 (state-root ordering),
`consensus/core/src/palw_pwu.rs` (the derivation identity and `check_pwu_claim_v1`),
`consensus/core/src/palw_class_daa.rs` (the V1 budget derivation and its `StarvedClass`
refusal), `consensus/core/src/palw_state_v2.rs`, `consensus/core/src/palw_admission_v2.rs`,
`consensus/core/src/palw_mode_v2.rs`, threat model rows H3 and "Per-class DAA / lifecycle".

> **Amended by later ADRs (index reconciliation, 2026-09-02).** Decision 3's deferred "automatic
> share re-allocation" is [ADR-0054](0054-palw-share-follows-production.md) Decision 1 (share
> follows production; [ADR-0056](0056-palw-permissionless-class-admission-and-share-economy.md)
> Decision 4's competing walk was withdrawn in its favour), and ADR-0056 adds registration
> exposure, permissionless admission and reclaim on the same table. [ADR-0058](0058-palw-merged-work-is-counted.md)
> makes merged (mergeset) work count toward every counter this ADR's budgets read. Decision 1's
> `pwu_per_inference` is the canonical job's counted step leaves and now also prices a class's
> attempt-lane seed ([ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)),
> while `DerivedV1`'s expected-attempts term is drawn once per inference under
> [ADR-0072](0072-the-ticket-is-the-execution.md) (the nonce sweep is gone). Decision 2's
> block-denominated budget stands; its known gap — a class registered mid-epoch has no attempt-lane
> budget until the next boundary (`ensure_epoch_budgets`) — is kept for state-root compatibility.
> The Status line's "no shipped preset carries `ConsensusV2`" is the 2026-08-20 state: testnet-11
> and devnet ship it (`palw_rc_shipped_params`, `devnet_shipped_params`). Map: [`README.md`](README.md).

## Thesis

Three numbers decide whether many classes can share one chain without collapsing into a
monoculture or wedging: what one block's work is **worth** (pwu), how many blocks a class may
put into an epoch (the Decision 5 cap), and who holds what fraction of cadence (the share
table). Before this ADR the first was a self-declared value under a ceiling, the second was
frozen at ruleset-mint time in a currency that structurally starves heavy classes, and the
third was a params constant no chain event could move — so registering a second model was a
release, not a transaction. Each of the three is a fact about the chain, and this ADR moves
each to the altitude where the chain can check it:

```text
pwu       — derived, per candidate point:  claim == palw_pwu_v1(class_target, pwu_per_inference)
budget    — derived, per epoch boundary:   ⌊tol · E · s_c / (1000 · denom_c)⌋ blocks, frozen in state
shares    — granted, per registration:     conserved to 1000‰ by donation arithmetic, rooted
```

## Decision 1 — pwu has exactly one legal value (the ADR-0039 derivation item, V2 face)

`PalwPwuRuleV2` gains its promised second variant:

```rust
enum PalwPwuRuleV2 {
    MaxPerAttempt(u64),                    // pre-derivation scaffolding, unchanged
    DerivedV1 { pwu_per_inference: u64 },  // this decision
}
```

Under `DerivedV1`, admission item 6 is **equality, not a bound**: the attempt's `pwu` must equal
`palw_pwu_v1(class_target, pwu_per_inference)` where `class_target` is the class's target **at
the candidate chain point** — the exact identity `palw_pwu.rs` states and the V1 lineage already
enforces via `check_pwu_claim_v1`. The ADR-0039 amendment's objection to an admission-time rule
— "the only altitude at which a header is validated has no legal source for a class's DAA
target" — is void on the V2 lineage: the class target is rooted, candidate-scoped chain state
(PR-03), which is the altitude admission already reads bonds and classes from. Neither factor is
a miner input, so any other value is a mistake or a weight-inflation attempt, and both are
`PwuClaimNotDerived`.

`pwu_per_inference` must be ≥ 1 at registration (`ZeroPwuPerInference`), and is normatively the
**counted step-leaf count of the class's canonical inference** — the same number the court's
bisection ladder must walk, which is why `PalwCourtParamsV2::max_step_leaf_count` is its
network-wide ceiling. The genesis loader that verifies `class_catalog_root` preimages must
verify the declared count against the catalog's counted one — the same landing H2's still-open
note already assigns `max_step_leaf_count` to. One currency for weight and adjudication: work
that cannot be walked cannot be worth anything.

`MaxPerAttempt` survives for fixtures and pre-derivation nets; ADR-0042's "the derivation is
required before any class carries weight" is now dischargeable — a value network registers only
`DerivedV1` classes, and the register says so.

## Decision 2 — the epoch budget's currency is the block

The 2026-08-17 amendment elected pwu as the cap's currency (defect (a)) and then derived, in
defect (e), that the election cannot work across epochs: with `W_e = L · Σ s_k · pwu_k`, the
share cancels out of its own inequality and what remains is `tol · mean(pwu) ≥ pwu_c` — a
comparison of pwu magnitudes across classes, which is the cross-class price this fork rejects
everywhere else, and which caps every above-mean class below its own cadence. The starvation is
the **currency's** fault, not the tolerance's. This decision supersedes the election:

**The budget is denominated in blocks.**

```text
budget_c(e) = ⌊ tol‰ · E · s_c / (1000 · denom_c) ⌋      blocks, for epoch e
```

* **Nothing the pwu currency enforced is lost.** Targets move only at epoch boundaries (PR-09)
  and Decision 1 makes per-block pwu a constant of (class, epoch) — so within any epoch the two
  caps are the same cap, entry for entry. They differ only across epochs, which is exactly where
  the pwu currency manufactures defect (e).
* **Every defect of the amendment stays closed.** (a) a block count is even more immutable than
  pwu; (b) the budget is frozen at the boundary, in rooted state (below); (c) it is charged to
  the producing block's own class counter along its own selected chain — the accumulator the
  V2 state already carries — and never to a merger; (d) dissolves outright: no pwu magnitude
  crosses a class boundary anywhere in the cap; (e) `budget_c ≥ expected_c ⟺ tol ≥ 1000‰`,
  which is the tolerance floor — **there is no configuration in which a class is capped below
  its own cadence**, so `StarvedClass` has nothing left to refuse in V2.
* **The cap still bounds what it exists to bound.** The threat is a transiently mis-tuned DAA
  flooding the DAG — a block-count, bandwidth and verification-load quantity. The weight side
  needs no cap once pwu is derived: a too-easy target mints many blocks that each weigh
  proportionally less, so a class's Σpwu per epoch tracks the compute it actually spent
  regardless of tuning; inflating per-block pwu requires tightening one's own target and paying
  the inferences (bounded per boundary by `max_factor`).

**Boundary freezing.** Crossing into epoch `e` derives every eligible class's budget from the
boundary's own facts and writes them into rooted state (`PalwEpochBudgetsV2 { epoch_index,
budget_blocks }`). Admission reads the snapshot; for the crossing block itself (whose apply has
not happened yet) it derives from the parent state — the same pure function over the same
inputs, so the stored snapshot and the crossing block's own admission cannot disagree. The
epoch's budgets are constant for the epoch, whatever registrations land mid-epoch: a class
registered mid-epoch enters the table at the next boundary.

**The denominator is the H1 census.** `denom_c = Σ shares over the competing set`: the closed
epoch's producers among unfrozen share-bearing classes; a non-producer that re-enters is
measured against the set plus itself; an empty census (fresh chain, gap epochs) competes
everyone unfrozen. This is the retarget's own absence rule — audit H1 — applied to the second
consumer of the same census: a class whose permille sits idle must not strangle the classes
that are actually producing, and a cap that ignored absence would reintroduce H1's unbounded
walk as a hard refusal instead of a slow one.

**Grant floor.** A share is grantable only if `tol · E · s ≥ 10⁶` — the smallest share whose
worst-case budget (`denom = 1000‰`) is still ≥ 1 block. Derived budgets are therefore never
zero, mid-flight, by construction; the V1 derivation's `ZeroBudget` refusal becomes a
registration-time property instead of an epoch-time cliff. The tolerance is fenced
`[1000‰, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE]` — the V1 ceiling, reused.

The V1 pwu-denominated derivation (`class_epoch_budgets_v1`) stands as the V1 lineage's record
of the amendment; the V2 lineage does not call it.

## Decision 3 — the share table is chain state, granted at registration, conserved by arithmetic

`class_shares: BTreeMap<class_id, u16>` joins `PalwChainStateV2`'s primary data, rooted and
carriage-moved like everything else the root commits to. The params table dissolves:
`PalwClassDaaV2Params` is deleted, and `PalwStateParamsV2` carries what actually is a network
constant — `base_class_id`, `class_daa_max_factor`, `budget_tolerance_permille`. The bundle
names `base_class_id` too, and the startup gate requires the two to agree (the C5 pattern: the
value the id commits to must be the value the machine enforces).

* **The first registration funds the floor.** The first `ClassRegistered` on a chain must be
  the base class, at exactly 1000‰ — the liveness floor exists before anything else does, and
  ADR-0039 W6′'s "may never be absent, may never be zero" becomes a property of the only
  sequence of objects the machine accepts.
* **Every later registration pays for its seat.** `ClassRegistered` carries `share_permille`;
  the entrant's share is donated proportionally by every incumbent — largest-remainder,
  class-id order, the V1 freeze arithmetic reused as arithmetic — so `Σ = 1000‰` holds at every
  mutation by construction, not by a boot-time assertion. Refused: an entrant below the grant
  floor, or a donation that would push any incumbent (the base class included) below it.
* **Freeze and unfreeze move no share.** A frozen class's permille stays in the table and out
  of the census; both consumers (retarget expectation, budget denominator) renormalize over who
  actually competes. This supersedes the register's "redistribution on freeze" expectation
  deliberately: a freeze that moved permille would hand every unfreeze a burst and make a
  temporary status a table mutation, and the census already answers absence at the only two
  places absence is measured.
* **No self-declaration.** Decision 5's "enters at a share set by finalized class-health
  transition rather than by self-declaration" is honored structurally: the share rides the same
  authorized registration object the class itself does — whoever may register a class may fund
  it, and nobody else may move a permille at all.

**Startup gate movement.** Deleted from the bundle gate: table-sums-to-1000, base-share-nonzero,
and both share↔budget coherence loops — their subjects no longer exist as params. What replaces
them is stronger: conservation and the floor hold at **every transition**, exercised by the
differential suite, instead of once at boot. `PALW_STATE_V2_VERSION` bumps 1 → 2: the root
preimage gains `class_shares` and `epoch_budgets` in declared-field order (ADR-0043's rule for
a consensus change to the root: a new version, never a silent re-reading).

## What this ADR does not decide

* **Automatic share re-allocation from class health.** An idle class's permille sits; the
  census routes around it. Decaying it on-chain is a future object with its own authorization
  story.
* **Permanent class removal** and where its share returns.
* **The counting tool** that produces `pwu_per_inference` for accelerated classes; BASE-0's
  count lands with the catalog loader, beside `max_step_leaf_count`, which already owes the
  same check.
* **Reward's relation to shares.** Reward stays claim-scoped (Decision 10); nothing here touches
  the escrow.

## Consequences

* Defect (e) stops being a tuning problem: no share table, tolerance, or pwu spread can cap a
  class below its own cadence, and the proof is one line (`tol ≥ 1000‰`).
* Registering a model becomes a chain event: a `ClassRegistered` object with a funded share,
  admissible mid-flight, producing from the next epoch boundary — not a params release.
* The ruleset id changes shape (fields moved and removed). Permitted: nothing shipped carries
  `ConsensusV2`, and Decision 11's promise is about RC == mainnet, both of which mint after
  this ADR.
* New named tests land with the implementation: the derivation-equality attack
  (`u64::MAX` claim refused, off-by-one refused, drift across a retarget boundary re-derived),
  budget frozen under mid-epoch registration, the (B+1)-th block refused while a sibling class
  still admits, donation conservation under permutation, and the H1-census denominator for a
  sole producer.
~~~~~~~~
<!-- ADR_SOURCE_END:0045-palw-class-economy-on-chain.md -->

</details>

<a id="source-0054-palw-share-follows-production"></a>

<details>
<summary>0054-palw-share-follows-production.md — 原文 L1–L137</summary>

[原文を開く](../adr/0054-palw-share-follows-production.md) · SHA-256: `a77edf9564652f8257af0c0a956ff222dc1ab3b6149a323c36ec206960953527`

<!-- ADR_SOURCE_BEGIN:0054-palw-share-follows-production.md -->
~~~~~~~~markdown
# ADR-0054: A class's cadence share follows its own production

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Status amendment (2026-09-18, ADR-0137):** past `Params::palw_work_target` — testnet-11's DAA 6,001 flag day —
> a class's *share* is a **result**, not a lottery input. The class target, the class DAA, the epoch budget, the
> admission-derived share and the seat price this ADR specifies are **not read** past the fence: a block draws
> against `CCU / W` (one network-wide work target) and the readers report the rolling share of finalized work.
> Below the fence the text stands as the rule it was; see ADR-0137 §3–§5 and §22.

Status: **Accepted.** Adds the share-raise path ADR-0045 deferred ("automatic share re-allocation
from class health") and closes the half of ADR-0045 Decision 3 that had no writer. Moves
`palw_ruleset_id_v2` (two new `PalwStateParamsV2` fields), so every network re-mints.

Date: 2026-08-27

Relates to: ADR-0045 (the share table as chain state — this is its deferred item), ADR-0049
Decision H (an entrant takes the minimum grantable share and cannot name its own), ADR-0038
Decision D (per-class difficulty; prices are discovered, never maintained), ADR-0039 W6′ (the
liveness floor may never be zero — restated here as a quantity), audit H1 (a class that produced
nothing is skipped by the retarget).

---

> **Amended (index reconciliation, 2026-09-02).** Decision 2's floor reserve
> (`base_class_reserve_permille = 500‰`) is [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)'s
> 20‰ since Relaunch 5 — the floor retires to the doctrine's minimum and the growth walk stops
> there. Decision 1's walk is the one share rule ([ADR-0056](0056-palw-permissionless-class-admission-and-share-economy.md)
> Decision 4 was withdrawn in its favour), it counts merged work ([ADR-0058](0058-palw-merged-work-is-counted.md)),
> and it may not grow a class whose family is uncertified ([ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md):
> a zero share is a certification state, not a small one). Map: [`README.md`](README.md).

## Context — the measurement that forced this

A post-genesis entrant takes `min_grantable_share_permille` and nothing else: `verify_class_admission_v2`
refuses any other value, and `write_share` had exactly one caller — the activation grant. A share,
once granted, was a constant for the life of the chain.

That would be a policy question if the number were arbitrary. It is not. The grant floor is
`⌈10⁶ / (tol · E)⌉`, chosen so its holder's worst-case epoch budget is at least one block, and at
`tol = 1000‰` that makes **`expected = E · s / 1000 = 1` for every entrant on every network**,
whatever the epoch length. ADR-0045 Decision 2's budget then caps the same class at 1 block per
epoch. So a minimum-share class has exactly two reachable states per epoch:

| observed | what the retarget does |
|---|---|
| 0 | **skips it** (audit H1: a span a class sat out says nothing about its difficulty) |
| 1 | `expected == observed` — an exact no-op |

Measured on a two-class chain carrying the real `PALW-QWEN36` class (its own `shape_profile_id`,
its own counted `pwu_per_inference` of 1,865,520), over four epochs, in both states: **the target
never moved**. The difficulty loop was not broken. It was starved of the only quantity that could
feed it — the class's share.

The same measurement on testnet-11 shows the other half: one class holding the whole table, three
closed epochs, `class_target` bit-for-bit the genesis constant. Both halves have the same cause.

## Decision 1 — production earns cadence, silence returns it

At each closed epoch boundary, in the same transition slot as the retarget and against the same
census:

* a class that produced **every block its epoch budget allowed** takes `max(1‰, share × g / 1000)`
  permille from the base class;
* a class that produced **nothing** returns a step of the same size to it;
* a class that produced something but not all of it is left alone — it is running at its natural
  rate and its share is already right.

`g` is `class_growth_permille`, a network constant. The base class is the reservoir: every permille
moved comes from it or returns to it, so conservation is a construction, not an assertion.

**Why "filled its budget" is the signal.** With `tol = 1000‰` a class's budget IS its share of the
cadence. A class that produced all of it was stopped by its share and not by its ability, which is
the only on-chain evidence that it wants more. A class that produced none of it is not using what it
holds. Nothing else is consulted: no vote, no operator, no key.

**Why this is derived and not granted.** ADR-0045 called the missing rule "a future object with its
own authorization story". This has no authorization story, which is the point: nobody submits it, so
there is nothing to forge, nobody to bribe, and no key whose loss freezes the table. A class grows
only by performing work its own difficulty prices, and the growth is bounded per epoch.

## Decision 2 — the floor keeps a reserve, and it is a number

ADR-0039 W6′ says the liveness floor's share "may never be zero". Against a rule that moves permille
every epoch that bound is worth nothing: a table walked down to one permille has ended liveness while
satisfying the letter of it. `base_class_reserve_permille` is the quantity — growth stops when the
floor reaches it, and a growth rule may not be enabled without one (`with_class_share_growth_v1`
refuses `reserve = 0` while `g > 0`).

Symmetrically, decay stops at the grant floor: a class never loses its seat, because a share below
the grant floor is a zero epoch budget, and a class that cannot produce is a frozen class wearing
the wrong status.

## Decision 3 — where it runs, and what it reads

Immediately after `apply_class_retargets` and before the block's objects and `ensure_epoch_budgets`:

* **after the retarget**, because both measure the same closed span from the same counters — and
  in that order, so the retarget judges the span against the share that was in force DURING it
  while the growth sets the share the next span will be judged against;
* **before the budgets**, because the new epoch's budgets must be derived from the table this
  leaves behind — otherwise a class's new share would not reach its allowance for a whole epoch;
* it reads the **closed epoch's own budget table** (still in state at that point, since
  `ensure_epoch_budgets` has not yet run for the new epoch). A budget stamped with another epoch
  caps a different span and is not evidence about this one.

A span in which nothing at all was produced moves nothing in either direction: an outage is not any
class's fault, and decaying the whole table for it would hand the floor everything for free.

## Consequences

* **Every network re-mints.** `PalwStateParamsV2` gains two fields, so `palw_ruleset_id_v2` — which
  is `hash(borsh(bundle))` — changes, and with it `consensus_params_id`. testnet-11 re-mints anyway
  for the Qwen3.6 class (its deployed build is `PALW_STATE_V2_VERSION` 7 against 8 in this lineage),
  so the cost is already being paid.
* **`PALW_STATE_V2_VERSION` does not move.** No field enters the state-root preimage: `class_shares`
  was already rooted, and this rule only writes to it.
* **Off is the identity.** `g = 0` is what `PalwStateParamsV2::new` builds and what every fixture
  predating this ADR runs at; no share moves outside an activation grant, byte for byte as before.
* **The RC bundle turns it on** at `g = 250‰` with a `500‰` floor reserve. Measured trajectory for a
  class producing its full budget every epoch: 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 15, 18, 22, 33
  permille over fourteen epochs — roughly a fortnight at testnet-11's epoch rate to reach 3 % of
  cadence, and the same rate back down if it stops.
* **The entrant's difficulty becomes measurable.** Once a class holds more than 1‰ its expectation
  exceeds one block, so `observed < expected` is reachable and the retarget has an input. The two
  loops compose: the share rule decides how much cadence a class is allowed to want, the DAA decides
  how hard each block of it is.
* Left undecided, deliberately: **permanent class removal**. Decay stops at the grant floor, so a
  dead class still holds a seat; reclaiming it is a separate question about the class record, not
  about the share table.
~~~~~~~~
<!-- ADR_SOURCE_END:0054-palw-share-follows-production.md -->

</details>

<a id="source-0068-the-llm-primary-economy-and-the-floors-minimum"></a>

<details>
<summary>0068-the-llm-primary-economy-and-the-floors-minimum.md — 原文 L1–L207</summary>

[原文を開く](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md) · SHA-256: `4ad8fa05f497bea65b44756a2324d7b5848914a70bb2b4e73c40f4f13ebd2066`

<!-- ADR_SOURCE_BEGIN:0068-the-llm-primary-economy-and-the-floors-minimum.md -->
~~~~~~~~markdown
# ADR-0068: The LLM-primary economy — the floor retires to the doctrine's minimum

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted; **Phase 1 implemented AND drilled 2026-09-01**. F3a and F2 are closed in
  code (fences shipped OFF everywhere but devnet, which arms them as the standing drill
  network); the deterministic drill (`the_heartbeat_clock_sweeps_a_stopped_chain_back_to_life`)
  releases the block-600 wedge in 4,300 clock ticks; the live two-node drill
  (`scratchpad hb-drill/REPORT.md` of the implementing session) PROVED the lane on real
  processes — a zero-bond devnet born and bootstrapped over heartbeats, the nominal hour held
  to the second twice, 120 s recovery cadence, ε = 1 vs 2²⁰ priced exactly as declared, and
  unattended producer re-entry twice. Drill findings: **F1** (producer/heartbeat-miner SIGTERM
  hang — fixed on this branch, the panel's `tick()` copied to both), **F2** (no cadence pacing
  at genesis-permissive targets — first-epoch mint runaway on fresh nets, Phase 2 item),
  **F3** (a dying producer accepts its own block after its P2P server stops — narrowed by the
  F1 fix, residual noted), **F4** (the V2 assembly violates upstream pruning depth-consistency,
  and LIVE testnet-11 does at 6,600 = 11 × 600 — devnet fixed, t11's correction rides the
  Phase 2 flag day), **F5** (a heartbeat CHAIN deeper than `max_per_mergeset` strands
  permanently once a heavier bonded fork puts it in the anticone — **closed the same day by
  amending the width rule**: at most four heartbeats, or any number provided they form ONE
  chain (sorted by blue score, every adjacent pair ancestor-related). A chain is the lane
  doing its job through a long outage, already rate-priced by the slot ladder; width —
  siblings, and the tree shape that would fool a chain-HEAD count — is what F3a is about, and
  the pairwise total order refuses both. The rule's semantic version rides the fingerprint
  label (`palw_heartbeat/width-chain-exempt-v2`), so only armed presets moved: t11's
  scheduled fingerprint 05df4e5e… → 0533c8ee…, devnet's 47ba789e… → 280db9e1…. The drill's
  exact strand shape is the regression test).
  **F6 (the launch audit's finding, fixed before the train left):** the heartbeat MINER
  consulted `should_mine`, which folds in `is_nearly_synced` — and the sync-rate escape that
  clause leans on itself expires once the finality point is over three finality durations old.
  So the longer a chain had been stopped (or the older a fresh re-genesis's timestamp), the
  more firmly the clock refused to start: the ADR-0060 §1 self-referential hostage, wearing a
  mining-heuristic costume, masked in the drill by `--enable-unsynced-mining` on the H node.
  The miner now holds only on what a stalled chain can still supply — peer connectivity, the
  chain-participation gate, and the transitional-IBD check — and its first tick is what makes
  every unmodified producer's `should_mine` pass flag-free again.
  Phase 0 is operations and began the same day. **Phase 2 is implemented on this branch
  (`palw-adr0068-phase2`, the Relaunch 5 identity `d7510c7a…`)**: the floor reserve
  500‰ → 20‰ (one field, every V2 network), genesis shares 489/489 so the floor lands
  exactly on its reserve, both fences armed from genesis (mainnet's assembly arms the same
  way the day its card is pinned), and F4's depth-consistency nudge moved into the shared
  assembly. Note the measured correction to Phase 0's own text: the LIVE reserve was 500‰
  (`BASE_CLASS_RESERVE_PERMILLE`, which the bundle builder writes over the struct default of
  100), so the walk could never pass the half-table on the running chain — the 90% figure was
  only ever reachable through this train. Execution = `docs/history/testnet11-relaunch5-runbook.md`,
  gated on the Phase 1 fleet deploy and the operator's re-genesis go (every host wipes;
  seat 5 re-keys off the lost 160.16 host, returning unmanned seats to ≤ 2).
- Date: 2026-09-01
- Depends on: ADR-0045 (class economy: block-denominated budgets, share table),
  ADR-0054 (share follows production), ADR-0058 (merged work is counted),
  ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery), ADR-0066 (heartbeat out of
  bits — findings F2 and F3a, which this ADR closes)
- Amends: testnet-11's de-facto floor-majority cadence (share table 550/200/250) and
  `min_base_class_share_permille = 100`.

> **Deployed (index reconciliation, 2026-09-02).** The Status above records Phase 2 as
> "implemented on this branch … gated on the operator's re-genesis go". The go was given: Relaunch
> 5 shipped the LLM-primary table from genesis (`fe8a7284`), and testnet-11 has run it through
> Relaunch 5c, 5d and 5e — floor reserve 20‰, genesis shares 489/489/22, heartbeat lane and
> attempt-work constant armed from genesis (`palw_rc_arm_phase1`; the Phase 1 sentence "fences
> shipped OFF everywhere but devnet" is the pre-train state). What later ADRs added on top of this
> table: [ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (a class holds that share
> only while its family is certified end to end), [ADR-0072](0072-the-ticket-is-the-execution.md)
> (the lottery priced in inferences, so F2's constant is compared against a per-inference draw) and
> [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) (each class's
> attempt-lane target seeded from its own share and work — the 5d measurement that a shared seed
> gave the floor 98.5 % of blocks against this table). Map: [`README.md`](README.md).

## 1. Goal

Rewards AND block production come from LLM computation by default; base (model-free)
computation exists only for the minimum the liveness doctrine requires. End-state on a
three-class network:

| axis | floor (BASE-0) | LLM classes | mechanism |
|---|---|---|---|
| time (clock) | — | — | heartbeat lane (armed): ε-weight, fee-only, ramps on silence |
| blocks / issuance | 2% | 98% | share table (blocks × the same subsidy carve) |
| chain weight | ~0.01% | ~99.99% | pwu (unchanged) |

## 2. What was already true (measured, not designed here)

* **Issuance follows block counts.** The subsidy carve is class-independent (ADR-0042 D10:
  a carve of the fixed emission, never an addition) and the epoch budgets are "blocks, never
  pwu" (`derive_epoch_budgets_v2`). The share table is therefore THE issuance lever, and pwu
  is chain weight — security, reorg depth, slash exposure — not pay.
* **Liveness does not need the floor's share.** Three shipped mechanisms carry it: the census
  denominator (an idle class's budget redistributes), S-04 (over-budget blocks are merged but
  unpaid — the floor can advance DAA past a dying epoch at zero issuance cost), and the
  heartbeat lane (ADR-0060 D1 / ADR-0066) once armed.
* **Emergency full pay needs no new rule.** If every LLM class dies, the next epoch's census
  hands the floor ~the whole block budget at full pay — the ambulance wage emerges from
  ADR-0045 arithmetic.

## 3. The three phases

**Phase 0 — walk the table, don't fork it (operations, no consensus change).** Run the LLM
producers persistently; ADR-0054 walks the floor 550‰ → 100‰ (its current minimum) as LLM
production sustains. The floor producer keeps running through its unpaid over-budget tail —
that tail IS its liveness readiness. *(Begun 2026-09-01: node0 restored to the QWEN36
producer class; C's seat 2 doubles as the QWEN25-A16 producer, because node-b — the class's
producer and panel seat 5 — is dark, which is also why the void rate is elevated: three dark
seats where ADR-0065 assumed at most two.)*

**Phase 1 — arm the clock before shrinking the reserve.** Close ADR-0066's two leftovers in
code (§4), then arm `palw_heartbeat` and `palw_attempt_work` on testnet-11 **by a rolling
preset update**: both fences follow the fence discipline — a scheduled activation keeps
`consensus_identity_id`, so old and new builds stay peers until the fence fires, and the
locked values (`work_log2`, `max_per_mergeset`) refuse to start a binary that does not
implement them. Run the bondless heartbeat miner (one flag) on ≥ 2 independent hosts. Then
the drill, on a devnet clone: kill every producer AND the floor; verify the ramp
(1 h → 120 s), re-entry transactions riding heartbeats, epoch closure, census hand-back, and
unattended recovery. **Phase 2 is gated on this drill passing.**

**Phase 2 — the floor's minimum drops 100‰ → 20‰ (coordinated re-mint).**
`min_base_class_share_permille` is inside the V2 bundle and therefore inside the identity, so
this is testnet-11's next re-mint train, not a fence. With the clock armed, the 10% reserve
defends nothing the census + heartbeat do not already defend; 20‰ (the grant floor admits
≥ 1‰) keeps the floor as a permissionless entry ramp (one floor block funds ~10⁵ minimum
collaterals), the artifact-less KAT class for the dispute machinery, and the census's
expansion seed. Mainnet preset: genesis share table {floor 20‰, LLM 980‰},
`min_base_class_share_permille = 20`, both fences armed from genesis.

**The floor is never withdrawn.** ADR-0053 measured what withdrawal costs; the floor is the
one class with no artifact to lose and the one class every seat verifies by construction. It
shrinks; it does not leave.

## 4. Phase 1's two closures (implemented on this branch)

### F2 — the attempt lane's blue work leaves `calc_work(bits)`

`Params::palw_attempt_work` (`PalwAttemptWorkV1 { activation, work_log2 }`): under the fence
an algo-6 block's blue work is the constant `1 << PALW_ATTEMPT_BLUE_WORK_LOG2` (2²⁰), maxed
with `level_work`, at every proof level (threaded through the pruning-proof managers the same
way ε is). The audit's finding: on a V2 preset the ambient bits price every bonded block at
2 — parity with two ε = 1 sibling heartbeats for ~280 kH/s.

**A constant, deliberately NOT the envelope's claimed pwu.** The claim is verified against
class state, class state lives on the selected chain, and GHOSTDAG holds only the header. A
claim-derived work would let a shape-valid header that never faces the lottery mint
fork-choice weight with a number; the constant keeps the spam/honest ratio exactly where it
is today while fixing the one ratio F2 is about. Per-class weight (QWEN36 vs floor) is not
this layer's job either — that is the pwu-verified PALW chain weight (`safe(C)`, ADR-0058),
which only counts what the chain actually checked. The pruning-proof level argument is
undisturbed in structure: per-block work is still a constant across the lane, only larger,
and level assignment (digest zeros) never depended on it.

### F3a — sibling heartbeat width is bounded where `mergeset_size_limit` lives

`PalwHeartbeatV1` grows `max_per_mergeset` (locked to
`PALW_HEARTBEAT_MAX_PER_MERGESET = 4`): a valid block's mergeset — selected parent included —
holds at most four heartbeat blocks, **or any number of them provided they form ONE chain**
(F5's amendment: sorted by blue score, every adjacent pair ancestor-related; a blue-score tie
is never ancestor-related and fails). `RuleError::MergeSetTooManyHeartbeats`, checked beside
`check_mergeset_size_limit`; POV-independent, no walk, no window — over the flat bound it
costs one reachability query per adjacent pair. The template builder decides the same
predicate over its accumulated set (`heartbeat_set_admissible`) and chunks sibling floods:
four this block, the rest against later blocks' fresh mergesets — while an outage CHAIN of
any depth is absorbed whole, which is what un-strands the drill's F5 shape. A chain is
rate-priced by the slot ladder already; width — siblings, and the tree shape that would fool
a chain-HEAD count — is what this bound is for, and the pairwise total order refuses both.

What the bound does NOT claim: relay/storage of never-merged siblings stays priced by the
2²⁴ header cost, as before. The bound removes their *consensus* influence — DAA, mergesets,
blue sets — which is what "unbounded valid blocks at a permanently fixed price" was about.

## 5. Considered and rejected

* **Claim-derived attempt work** — rejected above (weight minted by a number).
* **Per-class pay discount** (`base_class_pay_permille`, don't-mint the remainder): at a 20‰
  share the floor is already 2% of issuance; a second lever on the same axis buys < 2pp of
  purity for a new consensus mechanism and audit surface. One lever per axis.
* **Zero-pay floor**: an unpaid backbone is an assigned duty (re-centralizes to staff), and
  the entry ramp dies. The heartbeat lane is the correct zero-pay liveness instrument.
* **Removing BASE-0**: the heartbeat is fee-only and ε-weight — on a young chain it pays
  ~nothing (no faucet role) and weighs nothing (no weight-flow through an outage); and the
  dispute machinery loses its CI-runnable class.

## 6. Invariants to verify at each step

* I1 (seat capacity): at 98% LLM cadence ≈ 1 claim / 2 min sustained; seat artifact rollout
  precedes each share step (the 2026-08-28 slash incident generalizes: at LLM-majority
  cadence, seat coverage is chain-critical).
* I2 (escrow float): ~all issuance becomes Final-gated. Document miner cashflow; collateral
  derivation already prices claim lifetime.
* I3 (mid-epoch death): floor budget exhausts in a dying epoch → unpaid floor blocks + the
  heartbeat ramp close the epoch; census restores paid floor cadence next epoch. Drill it.
* I4 (finality during outage): weight-flow pauses on a heartbeat-only chain; the inactivity
  leak (ADR-0060 §6 / ADR-0066 D4) covers the overlay. No new rule.
* I5 (walk rate): 550 → 100 takes multiple epochs under the retarget clamp — fine on t11;
  new networks write the end-state table into genesis instead of walking to it.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0068-the-llm-primary-economy-and-the-floors-minimum.md -->

</details>

<a id="source-0107-a-share-grows-on-work-that-reached-final"></a>

<details>
<summary>0107-a-share-grows-on-work-that-reached-final.md — 原文 L1–L167</summary>

[原文を開く](../adr/0107-a-share-grows-on-work-that-reached-final.md) · SHA-256: `49cdada121e234b5cb53cb5a4e46e28ae04254029240f6b1a29c96b268df258d`

<!-- ADR_SOURCE_BEGIN:0107-a-share-grows-on-work-that-reached-final.md -->
~~~~~~~~markdown
# ADR-0107 — A class's share grows on work that reached Final, not on blocks that were accepted

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> **Status amendment (2026-09-18, ADR-0137):** past `Params::palw_work_target` — testnet-11's DAA 6,001 flag day —
> a class's *share* is a **result**, not a lottery input. The class target, the class DAA, the epoch budget, the
> admission-derived share and the seat price this ADR specifies are **not read** past the fence: a block draws
> against `CCU / W` (one network-wide work target) and the readers report the rolling share of finalized work.
> Below the fence the text stands as the rule it was; see ADR-0137 §3–§5 and §22.

* Status: PROPOSED 2026-09-11 on `fix/share-growth-counts-final-work` (from `main` at `3c78e898`).
  **IMPLEMENTED behind `Params::palw_share_growth_final`, `None` on every shipped preset**: no
  fingerprint, identity, schedule or fork id moves on any network. Arming it on testnet-11 changes
  the share table that a boundary block writes into the PALW state root, so it is a flag day (§5).
* Builds on: [ADR-0054](0054-palw-share-follows-production.md) (a class's cadence share follows its
  own production), [ADR-0056](0056-palw-permissionless-class-admission-and-share-economy.md) (its
  attack table: "fake demand growth costs full production price"), [ADR-0045](0045-palw-class-economy-on-chain.md)
  Decision 2 (the per-class epoch budget), [ADR-0058](0058-palw-merged-work-is-counted.md) (merged
  blues and reds are applied to the PALW state).
* Amends: ADR-0054's growth condition — one clause of it (§2). The budget, the decay arm, the
  retarget and reclamation are unchanged.
* Supersedes nothing.

## 0. The sentence this ADR is

**A class that fills its epoch budget with blocks whose claims never reach `Final` — never bound,
never licensed, voided — grows its cadence share exactly as a class whose every block was
verified.** The growth rule reads `produced_blocks`, which the transition increments when an
attempt is accepted and never decrements. Past this ADR's fence, growth also requires that the
class's attempt claims which reached `Final` inside the closed epoch number at least that epoch's
budget.

## 1. What the code does today (verified 2026-09-11, `main` `3c78e898`)

* `apply_attempt` writes the claim `Provisional` and increments the class's `produced_blocks` in
  the accepting block's epoch (`palw_state_v2.rs`, `apply_attempt`). This applies to the block's own
  attempt and to every merged blue and red that passes admission.
* `apply_class_share_growth` → `derive_class_share_growth_v1` grows a class with
  `budget != 0 && produced >= budget && share != 0` by `max(1‰, share × 250‰)`, taken from the
  floor down to its reserve.
* `void_claim` and `void_and_slash` never touch the counter. The test
  `abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget` pins this: "voiding releases
  exposure, never production".
* Nothing between acceptance and the panel checks an attempt's execution. The class ticket hashes
  roots the producer writes, so the only gates on a bogus block are the class ticket, Layer-0 and
  the bond's exposure headroom.

Reproduced on the real fold (`consensus/core/tests/palw_adr0107_share_growth_final.rs`,
`dormant_a_voided_block_still_grows_the_class_it_filled`): an entrant at 1‰, budget 1, produces one
block. Its claim voids at `BindTimeout`, and the boundary still grows the entrant to 2‰. In a probe
of the same fold, 16 epochs of entrant blocks that all voided took the share from 1‰ to 51‰.

**The budget half is right and stays.** A void that refunded its block would be a free re-roll of
the budget, which is what the counter exists to stop. The defect is that the same counter is also
the growth signal.

## 2. Decision

Past `Params::palw_share_growth_final` (resolved at the block that crosses the boundary), a class
grows at a boundary only if:

```
budget(E) != 0  &&  produced(E) >= budget(E)  &&  finalized(E) >= budget(E)  &&  share != 0
```

`finalized(E)` is the number of the class's **attempt** claims whose `Final` happened inside the
closed epoch E (`final_daa / epoch_length == E`). It is read off the claim records at the
boundary, which means:

* **No new state and no layout change.** `epoch_counters`, the delta enum and the snapshot
  encoding are untouched. The count is a pure function of claims the state already holds.
* **Readable only while the records last.** `validate_palw_v2` refuses arming unless
  `claim_retirement_daa` is 0 or longer than an epoch. Otherwise a claim finalized early in E would
  be retired before E's boundary counted it. Testnet-11 (3,000 against 1,000) qualifies.
* **Counted by when the work finalized, not by when its block was accepted.** In a steady state the
  two agree. In a transient, growth lags acceptance by the lattice's windows (bind + receipt +
  challenge), so a class grows later than it does today, and sometimes not at all:
  * a claim accepted at the end of E that finalizes in E+1 counts toward E+1's total, and E+1's
    boundary grows the class only if E+1's budget was filled too. A class that fills one epoch and
    then stops does not grow on that epoch's work. Today it does;
  * in a sustained ramp, E+1's total is partly E's smaller budget's worth of work, so it can fall
    short of E+1's larger budget. Growth then comes every other epoch rather than every epoch.

  Both are the conservative direction: the rule can refuse growth to real work, and it never grants
  growth to work that did not reach `Final`.

**What does not change.** Decay still reads `produced_blocks`, because a class that is producing is
not idle, whether or not its claims have finished. So the fence changes who **grows** and nothing
else. The budget, the retarget, reclamation and the receipt lane's census are untouched, and so is
the pinned void test.

## 3. What this does not close, stated so nobody reads it as closed

* **Merged work is paid before it is verified.** ADR-0058 Decision 5 pays a merged blue (and an
  entitled in-window red) its full worker share in the merging block's coinbase, with no escrow.
  Its justification was `reserved ≫ carve`, and on the shipped economy the reverse holds by many
  orders of magnitude: a testnet-11 claim reserves on the order of 10⁻³–1 BILI against a carve of
  ≈2,756 BILI. So a bogus merged block is not "full production price" (ADR-0056's attack table): it
  is paid. That is a reward rule, separate from this ADR and larger than it. It needs its own
  decision (escrow merged work to `Final` like the chain block's, or price the exposure against the
  carve).
* **Holding a share still costs one accepted block per epoch.** Decay reads `produced_blocks`, and
  only a class with no accepted attempt in the epoch decays. So a class kept alive by blocks that
  void keeps its share. Past the fence it can no longer grow on them.
* **Operation count is not economic cost.** `pwu_per_inference` is a graph's normative leaf count,
  not measured FLOPs. A class whose canonical semantics admit a cheap shortcut is priced by its own
  per-class DAA, not by this rule.

## 4. Implementation

* `Params::palw_share_growth_final: Option<ForkActivation>` — top level (a bundle field would move
  `palw_ruleset_id_v2` and refuse every old/new pair at the handshake). It is hashed into both ids
  only when set, visited by `for_each_fence`, named in `palw_fences_v1` and the fork-id probe, and
  collapses `Some(never())` to `None`. Read through `palw_share_growth_final_fence()`.
* `PalwTransitionExtrasV1::share_growth_final_active`, resolved at the block's DAA by the virtual
  processor and by the state sync walk (the latter is only reached by its own tests today; it is
  threaded so the two can never disagree).
* `PalwClassEpochUseV1::finalized: Option<u64>` — `None` below the fence, byte-identical to before.
  `derive_class_share_growth_v1` refuses growth when it is `Some` and short of the budget.

## 5. Activation

Dormant everywhere. Arming it on testnet-11 needs:

1. the build on every fleet node before the height, since the share table at the first boundary past
   it differs;
2. the height announced, and the fork-id gate's schedule line
   (`Consensus fence schedule: …`) showing it.

Arming from genesis is the natural setting for a new network. The mainnet card does not arm it in
this change: the card's armed set is a hand-maintained list that `docs/adr/README.md`'s activation
table is written from, and adding to it is a decision for that table's owner.

## 6. Tests

* `consensus/core/tests/palw_adr0107_share_growth_final.rs` — the real fold, dormant and armed:
  * a `BindTimeout` void grows the class while the fence is dormant (the limitation, pinned), and
    earns nothing when armed, with no decay;
  * Final work earns the same step either way;
  * a claim still `Provisional` at the boundary is not evidence when armed, and it counts in the
    epoch it finalizes in.
* `palw_class_daa.rs` — `past_the_fence_a_filled_budget_grows_only_on_its_finalized_work`.
* `params.rs` —
  `the_share_growth_final_fence_is_dormant_everywhere_and_refused_where_it_cannot_read_the_epochs_finals`:
  * dormant on every preset;
  * `never()` is absence;
  * a scheduled height moves the fingerprint and the schedule id but not the identity;
  * refused off ConsensusV2;
  * refused where claims retire within an epoch.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0107-a-share-grows-on-work-that-reached-final.md -->

</details>

<a id="source-0123-the-epoch-progressively-releases-unused-class-budget"></a>

<details>
<summary>0123-the-epoch-progressively-releases-unused-class-budget.md — 原文 L1–L62</summary>

[原文を開く](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md) · SHA-256: `0c6fdd3d986082eada57e9d3b701d93e7b319571658d445cad98ce3db5190ea2`

<!-- ADR_SOURCE_BEGIN:0123-the-epoch-progressively-releases-unused-class-budget.md -->
~~~~~~~~markdown
# ADR-0123 — The epoch progressively releases unused class budget

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0123-epoch-budget-release`, at the
  operator's request after the testnet-11 budget stall. The rule is dormant on every shipped
  preset until `Params::palw_epoch_budget_release` is activated.*
* Builds on: [0045](0045-palw-class-economy-on-chain.md)
  (per-class epoch budgets), and the existing `palw_epoch_boundary_budget` fence.

## 1. The problem

The census budget correctly limits a non-floor class to its epoch share, but it can turn a
temporary slowdown into a liveness stall. A class may consume its own share while the other
classes produce fewer blocks than their share. If the floor does not win a block, no DAA progress
is made and the next epoch — which would refill the budget — cannot begin.

## 2. Decision

For a class with budget `B`, epoch length `L`, and current position `p = DAA mod L`, define the
capacity released to it as:

```text
R = max(0, ceil(p × (L − B) / L) − Σ(other classes' attempt blocks in this epoch))
```

Admission accepts one more attempt when `produced + 1 ≤ B + R`. Only attempt counters are used;
receipt counters do not consume an attempt-class budget. Counters from older epochs contribute zero.

This rule has three invariants:

1. At the first slot, `p = 0`, so no future capacity can be borrowed.
2. Release is measured against `L`, not the sum of rounded budgets, so flooring cannot create a
   second deadlock at the end of the epoch.
3. The same pure function is used by admission, the fold's merged-attempt re-admission, and
   producer readiness.

The release is independently height-gated by `palw_epoch_budget_release`. The boundary derivation
remains independently gated by `palw_epoch_boundary_budget`; both values are carried to admission
as named `PalwEpochBudgetFencesV1` fields so they cannot be transposed as adjacent booleans.

## 3. Consequences

* A fastest available producer can fill every slot through the epoch boundary, including the
  floor's unused capacity, while a producer cannot take slots that have not elapsed.
* Existing networks are byte-identical while the fence is absent.
* The producer facts path also derives the candidate epoch's budget at a boundary when the boundary
  fence is active, so a producer does not hold a block admission would accept.

## 4. Verification

The core tests cover a spent class borrowing unused capacity, no borrowing ahead of the epoch, a
slow companion class, stale counters, zero-length epochs, and the full-epoch liveness property.
The processor and core crates compile with the named fence API, and the existing PALW test suites
remain green.
~~~~~~~~
<!-- ADR_SOURCE_END:0123-the-epoch-progressively-releases-unused-class-budget.md -->

</details>

<a id="source-0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input"></a>

<details>
<summary>0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md — 原文 L1–L723</summary>

[原文を開く](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) · SHA-256: `22169577750cf06531949dcf6578a6ff994c24b6d8175ba8be7d408da638c709`

<!-- ADR_SOURCE_BEGIN:0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md -->
~~~~~~~~markdown
# ADR-0137 — A block buys one unit of work from any model, and a share is a result, not an input

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`, design with a
reproducible simulation (`scripts/sim/palw-share-sim.py`); **the shadow (§17 step 1) built the same
day** — `palw_work_target_v1.rs`, the state's `work_target_shadow` and `final_work` carried outside
the root, op 186's work-target block and per-class `CCU / W`, ticket, room and finalized-work
shares (§22); nothing armed, no fingerprint moves. The interim commit `804af11d` (a class the
registry seats is priced by `attempt_target_seed_v1`) stays as a shadow of the shipped rule and is
**not** the final design — §6 says why. **Arming ADR-0135 D5's shares on testnet-11 is stopped**
(§3.5, §17): the registry's rows, proofs and gate may arm; the share → target rule may not.

**Supersedes the share-as-input reading of:** ADR-0045 D3 (the grant table), ADR-0039 D5 (epoch
budgets), ADR-0071 D1 / ADR-0076 (the class DAA and its seed), ADR-0054 / ADR-0107 (share growth,
dormant), ADR-0135 D5 (shares from admission) and ADR-0132 Upgrade C's cap rule (ADR-0133 Fence 3).
It keeps: the registry's lifecycle as a *verifiability* gate (ADR-0135 D6), the panel share rule
(ADR-0132 C's `clamp(α·C_V/(C_P+α·C_V), 10 %, 30 %)`), the execution lane's Final-credit quotas
(ADR-0125), the economic compute measure (ADR-0131) and the free-prompt lane split.

**Asked (2026-09-18):** redesign "model share" itself. Anyone may register a model; a model that
brings more real compute should hold more of the network, without a person setting `Qwen = 40 %`;
a small and a large model must earn the same BILI per Economic CCU; share should mean *what fraction
of PALW compute the model actually provided*; suspect first whether share must be a lottery input
at all; find the positive feedback, the cold-start trap and the registration Sybil; compare at
least three designs with simulation; define "usage"; check the issuance identities; keep execution
permits apart; say whether the `share` consensus parameter can be deleted.

**Armed for testnet-11 (prepared 2026-09-18, on `wip/arm-6001-registry-payout`, merged only on the
operator's go after the devnet drill):** `palw_work_target` at the 6,001 flag day — **the same height
as `palw_model_registry` and `palw_economic_payout`, and it has to be**. §3.5 is the reason: the
registry's shares-from-admission hand the model classes 980 ‰ of a cadence they cannot fill and the
floor's class DAA hardens toward its 20 ‰, so a network that arms the registry one height and the
work target a later one runs the cadence collapse for the gap. Armed together, from 6,001 a model
class draws against `MAX · min(1, CCU / W₀)` with `W₀ = escrow / rate` off the block's own subsidy
(306.25 G MAC-eq at the payout's 9 BILI a G MAC-eq, against forwards of 21–199 G), no share, model
class target or epoch budget is read, the registry keeps its rows, proofs and lifecycle gate, and one
network-wide verification budget replaces the per-class in-flight cap. The fingerprint moves from
`135b6ee0…` to `32c2e8e3…`, and on 2026-09-18 to `8af89f85…` when the single lottery and the short challenge window (ADR-0132 S, §7.6) joined the same day; the fork id's height set does not (6,001 was already scheduled), so the
two builds are told apart by the fingerprint alone — and the fleet is refused from 6,000 by the held
and deep fences the same release moves there.

## 0. The sentence this ADR is

**Every block pays the same escrow for the same expected amount of work, from whichever model did
it; the network holds one work target `W` (CCU per block) the way it holds `bits`; a model's ticket
is `CCU_m / W` and needs no share, no class target, no budget and no seat price; the floor fills
whatever cadence the models leave and is paid nothing for it; and "share" is the fraction of
finalized work a reader computes, never a number the chain feeds back into anyone's odds.**

## 1. Where share sits today — the formulas as shipped

Notation: `CCU_m` the economic compute of one forward of class `m` (MAC-eq, derived from the graph:
`PalwModelWorkV1::economic_ccu_per_claim`, ADR-0131); `r_m` forwards per second its producers run;
`C_m = r_m · CCU_m` the compute it supplies; `E` the block's escrow (the worker carve of the subsidy,
2,756.28 BILI on testnet-11); `p_m` the class ticket's probability a forward; `p_net` the network
draw against `bits`; `s_m` the class share in permille.

1. **The ticket** (`check_palw_class_lottery_v3`): a forward admits iff
   `class_ticket_v3(attempt) ≤ T_m`, so `p_m = (T_m + 1) / 2^128`; the block then also passes the
   Layer-0 digest against `bits`, `p_net = (target + 1) / 2^256` (ADR-0132 §1.1: two draws, one
   credited).
2. **The class DAA** (`retarget_over_span_v1`, every epoch): `expected_m = s_m · total_blocks`,
   `T_m ← clamp(T_m · expected_m / observed_m, ÷4, ×4)`; a class with `observed = 0` is skipped and
   only `converge_idle_target_v1` moves it, toward the hardest price a *producing* class pays and
   never past it. Shares are renormalised over the classes that produced in the closed epoch.
3. **The epoch budget** (`derive_epoch_budgets_v2`): `budget_m = max(1, epoch · s_m · tol /
   census)` blocks, census = the shares of the classes that produced last epoch; a block past the
   budget is refused (`EpochBudgetExceeded`); ADR-0123 releases what the others were due and did
   not produce, progressively.
4. **The grant table** (`granted_share_table_v2`): the first class is the floor at 1000 ‰, every
   later registration is funded by every incumbent pro rata, at least `min_grantable_share_permille`
   (`⌈10⁶ / (tol · epoch)⌉`), the floor never below `min_base_class_share_permille`.
5. **The registry's shares** (ADR-0135 D5, `step_model_registry`): every governed boundary,
   `s_m = room · admission_m / Σ admission`, `admission_m = min(budget_ccu / CCU_m, inflight_cap_m /
   window_m) · {50, 100, 1000} ‰ by state`, `room = 1000 − floor`; the floor holds the rest.
6. **The seat price** (ADR-0076, and `804af11d` for the registry's admission):
   `T_m = MAX · s_m · pwu_m / 2^31`.
7. **The reward**: the block's escrow `E` at 80/20 (ADR-0124); under Upgrade C
   `min(E, attempted_m · rate)` with `attempted_m = CCU_m / (p_m · p_net)` in expectation, the panel's
   share by `α·C_V / (C_P + α·C_V)`, and a class whose `attempted · rate / E > 800 ‰` is not
   activatable (Fence 3).

So the reward a model earns per unit of compute is, from 1, 2 and 7:

```text
ρ_m = MSK / CCU = 0.8 · E · p_m · p_net / CCU_m                      (escrow whole)
    = 0.8 · min(rate, E · p_m · p_net / CCU_m)                       (Upgrade C)
```

and the class DAA sets `p_m` so that `r_m · p_m · p_net = s_m · BPS`, i.e. **`ρ_m = 0.8 · E · s_m ·
BPS / C_m`**: a model is paid in proportion to its *share* and in inverse proportion to the compute
it brings. Two models with the same share and different supply are paid differently per CCU; two
models with the same supply and the shares the registry hands them (§3.3) are paid in inverse
proportion to their cost. Equal pay per CCU holds only where `s_m ∝ C_m` — which is the thing the
share table cannot know without measuring `C_m`, and measuring it is the feedback loop of §3.

## 2. Why the ticket was 3.589 × 10⁻³

Op 180's terms (`palw_v2_registration_terms_impl`) hand a registrant `initial_target = the floor's
current target`. The floor's target is what its own class DAA converged to for microsecond integer
draws — on the devnet `p = 3.589 × 10⁻³`, and node-3's own floor producer printed that exact number
(`class ticket p = 3.589e-3`) before it was restarted as the class producer and printed it again for
the class. A Qwen2.5 forward takes tens of seconds on that host, so the copied price meant hours a
claim. Nothing re-priced it: the registry holds a REGISTERED/PREFETCHING class at share 0 and
grants 980 ‰ at admission, but the class DAA skips a class that produced nothing, the idle
convergence moves an idle class only *toward* the incumbent's price (which it already had), and no
rule reads the share into the target. The earlier reading — over-production cutting the target —
was wrong: the fresh-genesis run with the budget fix showed the same ticket. `804af11d` prices the
class at its seating by ADR-0076's seed (`980 ‰ · 1,589,424 / 2^31`, saturating to `p = 1.0`), and
the drill's phase 2 accepted the class's first claim three minutes after the producer started.
That fix is correct *within* the share→target design; §3 is why the design is the problem.

## 3. The loops and traps in share → target

Simulated in `scripts/sim/palw-share-sim.py` (§20) with the shipped arithmetic: the profile globals,
the lifecycle machine, the class DAA with integer block counts, the competing-census budgets with
ADR-0123's release, the floor as its own class, a network-wide panel that replays every claim at
the reference rate and voids what waits past its window. Supply is stated relative to what the
panel can verify, because on testnet-11 verification binds long before compute does.

### 3.1 Cold start — permanent, not slow

A registration copies the floor's price (§2). `A0` in every scenario: the model's ticket stays at
`3.8 × 10⁻⁵` for 120 epochs, `10⁻⁵` claims a span, PROBATION forever (it never collects ten probes).
`converge_idle_target_v1` cannot help — its doc says so ("a class seeded at the floor's price is
already at it, so it does not move"). The trap is structural: the only rule that eases a silent
class eases it toward a price it was born at.

### 3.2 The lifecycle sawtooth

`palw_lifecycle_step_v1`: `utilization ≥ 1000 ‰ → HELD` from PROBATION, ACTIVE_LIMITED and ACTIVE
alike; `HELD → PROBATION` (admission ÷ 20, probes reset) `→ ACTIVE_LIMITED` (÷ 10, three stable
spans) `→ ACTIVE`. Utilization is claims in flight over the panel's window capacity, and the
cadence a share hands a class (`s_m · BPS`, 2.45 blocks a span at 490 ‰) is *unrelated* to the
capacity the share was derived from (about one claim a span for either Qwen on eight seats). So a
class the registry admits in full overloads its own panel by construction and is HELD; the surge
scenario (`Q36 supply × 10`) walks `ACTIVE → HELD → PROBATION` and pays Q36 `2.97` against Q25's
`7.20` BILI per G-CCU afterwards. A bang-bang controller with a ÷20 reset is an oscillator, and the
thing it oscillates is a model's income.

### 3.3 The inverse-cost share

`admission_m = min(budget_ccu / CCU_m, inflight_cap_m / window_m)`: both terms fall with `CCU_m`,
so **the cheaper the model, the larger its share**. A class of `0.5 G` MAC-eq is admitted at
`44` claims a span against Qwen2.5's `0.26` — 170 × the share for the same supply. With §1's
`ρ_m = 0.8 · E · s_m · BPS / C_m` that is 170 × the pay per CCU; the simulation prints a `ρ`
spread of `4.02` between the two Qwens (their cost ratio) and `169` with the tiny class. This is
exactly the "small model wins because it is fast" the redesign forbids, and it is the shipped
Upgrade A rule.

### 3.4 The cap trap (Upgrade C + the registry) — a positive loop

Fence 3: `attempted_m · rate / E > 800 ‰ → ACTIVE_LIMITED`. `attempted_m = CCU_m / (p_m · p_net)`.
ACTIVE_LIMITED divides the class's admission by ten, so its share and its expected blocks fall,
so the class DAA *hardens* `T_m` (fewer blocks expected of it), so `attempted_m` rises tenfold, so
its cap utilization rises tenfold, so it stays limited. The rule punishes a model for being heavy or
popular and then makes the punishment permanent. (The simulation's `A1` reproduces the mechanism
qualitatively; the arming branch's numbers put the dense class at 69.9 % of the escrow, one supply
surge from the ceiling.)

### 3.5 The cadence collapse

Shares from admission give the models `room = 980 ‰` whether or not their producers can fill it.
Once a model produces at all it enters the competing census, the floor's renormalised share is
`20 ‰`, and the floor's class DAA hardens its target by ×4 an epoch until it produces 2 % of the
total — of a total the models cannot fill. Simulated (`A1`, every steady scenario): `0.25–0.57`
blocks a span against a 5-block span, i.e. the chain at 5–11 % of its cadence, the floor at
`2.28` blocks an epoch, until the floor's expectation rounds to zero and its retarget stops. The
devnet at 0.05 BPS cannot show this inside the drill's window; a 1 BPS network would. **Arming
ADR-0135 D5 as shipped on testnet-11 is a liveness hazard**, independent of everything else here.

### 3.6 The dormant usage loop

ADR-0054 / ADR-0107 (`derive_class_share_growth_v1`, `palw_share_growth_final`, dormant) would add
`production → share`: with §1's target rule that is the loop the redesign asked about — `share ↑ →
target easier → more claims → more production → share ↑` — bounded only by the growth cap and the
floor. It is not armed; it should not be.

### 3.7 What is *not* a loop

The shipped share (5) does not read usage, so there is no positive feedback in `A0` — there are the
five traps above instead. The retarget itself is stable (its expectation sums to what happened,
ADR-0071's H1 fix), which the simulation confirms: `A1`'s block shares settle in ≤ 10 epochs where
nothing else intervenes.

## 4. Permissionless registration under the shipped rule — the floor-share Sybil

Register `N` classes (the same weights, `N` variants, `N` graphs a node apart). Each gets:

* a class target — the floor's price (§2), so each draws like a second floor;
* an epoch budget of **at least one block** (`derive_epoch_budgets_v2` floors at one, share 0
  included: "a weightless class still produces"); `N = 1000` is ten epochs of cadence a span;
* a gate at acceptance only where the registry gave the class a row: the transition refuses an
  attempt of a rowed class that does not admit (`ClassNotAdmitting`: REGISTERED, PREFETCHING,
  HELD) or is at its in-flight cap (`ClassInflightCapped`) — a real gate, corrected here from the
  first draft's "no gate" — but a class registered before the fence without a carriage has no row
  and is never gated, below the registry fence nothing is gated, and a rowed class in PROBATION
  admits at a twentieth; the block of an ungated class stands, its claim voids later
  (`NoCapablePanel`), the network draw hardened for everyone, and the honest classes' cadence was
  diluted;
* if admitted, share ∝ 1 / CCU (§3.3): a thousand cheap variants own the room.

Cost: the registration bond (`registration_bond_per_span · window`, 1,000 BILI a span-window) —
locked, released at reclamation. **A bond prices classes, not blocks**: it bounds how many classes
an attacker holds, not how much cadence each one takes, and a refundable bond is an interest cost.
It cannot close this on its own; only removing the per-class allocation does.

## 5. "Usage", defined — which CCU may mean what

| quantity | where it is on the chain | what distorts it | may consensus read it? |
|---|---|---|---|
| **attempted CCU** | not an event: an *expectation*, `CCU_m / (p_m · p_net)` from the target | none, but it is a model of the producer, not an observation | only as this expectation (ADR-0132's snapshot does exactly that) |
| **accepted CCU** | every attempt block | forgeries and no-panel claims count until they void | for a cadence census, not for money |
| **licensed CCU** | `ReceiptLicensed` | the panel's liveness (a dead seat licenses nothing) | yes, delayed |
| **Final CCU** | the claim's `Final` | the panel's liveness *and* the window; a HELD class finalizes nothing | yes — the strongest, the latest |
| **paid CCU** | the payout rows | the price (Upgrade C's min) — it measures BILI, not work | no, it is the answer, not the question |

**Decisions.** (a) The lottery reads none of them: a ticket is priced from `CCU_m` and `W` (§7).
(b) Under §7 every accepted claim represents `W` of work in expectation, so `attempted CCU` becomes
*exactly observable*: it is `W` at acceptance, snapshotted as ADR-0132's row already does.
(c) The model share a reader reports is **rolling Final work**: `share_m = Σ_{Finals of m in the
window} W_accept / Σ_{all Finals} W_accept` — the fraction of *verified* work, which is what the
question "what fraction of PALW compute did this model provide" means; a HELD or unverified class
drops out of it, correctly. (d) Nothing off-chain (a producer's own draw count, a seat's wall clock)
enters any of it.

## 6. The candidates

| | A — shipped, dynamic class share | A′ — `804af11d` + Upgrade C | B — global normalized lottery | C — B, share for statistics and execution only | **D — the work target (this ADR)** | E — D with headroom-scaled tickets |
|---|---|---|---|---|---|---|
| lottery input | share → class target (DAA) | share → seat price → DAA | `CCU_m / W` | `CCU_m / W` | `CCU_m / W`, `W ≥ W₀`, one DAA over model blocks | `CCU_m / W · (1 − load)` |
| reward a claim | `E` | `min(E, attempted · rate)` | `E` | `attempted · rate` (= `W · rate`) | `E` (and Upgrade C's formula degenerates to `E`) | `E` |
| BILI / CCU across models | `∝ s_m / C_m` — spread 4.02 (Qwens), 169 (tiny) | equal only while uncapped; the cap trap | equal, `E / W` | equal | **equal, `0.8 · E / W`, every scenario** | unequal for capacity-bound classes (2.02) |
| cold start | permanent (floor price) | seat price, then §3.2 | none needed | none | **none needed** | none |
| Sybil by classes | cadence per class (§4) | same, for unadmitted classes | none: a block costs `W` whatever the class | none | **none; 1000 classes = 1 class** | none |
| cadence | collapses to the models' rate / 0.98 | same | collapses without a residual floor (0.37 of 5 blocks a span) | as B | **held: the floor is the residual** | held |
| feedback | sawtooth, cap trap | same | none | none | **none (one global DAA)** | headroom loop (mild) |
| consensus state a class | share, target, budget, row | + priced share | row | row | **row only** (CCU is in the registration) | row + load |
| human-set numbers | shares, floors, seeds | + rate | `W₀` or nothing | rate | **one: `rate_max` (= `E / W₀`)** | one |

`B` is `D` without the residual floor: with `W` walked over every block and no floor rule, the
cadence at testnet-11's supply is 0.37 blocks a span. `C` is `D` with the reward written as
`attempted · rate`; under a work target `attempted = W` for every claim, so `C ≡ D` once `rate` is
read as `E / W` — a derived number, not a second constant. `E` scales a class's ticket by its
verification headroom instead of holding the producer: it under-pays exactly the classes the panel
cannot keep up with (their forwards mostly lose) and adds a loop; the producer's pre-check wastes
nothing. Rejected. **D is recommended.**

## 7. The design — one work target

**D1 — the ticket.** A class has no target. At a block with work target `W`,

```text
T_m = MAX · min(1, CCU_m / W)          p_m = min(1, CCU_m / W)
```

with `CCU_m` the registration's economic compute per claim (`PalwModelWorkV1::economic_ccu_per_claim`,
derived from the graph, the same on every node). The expected forwards a win is `W / CCU_m`, so the
expected *compute* a win is `W` for every model with `CCU_m ≤ W`.

**D2 — the work target.** `W` is one chain value, walked like `bits`, over model blocks only:

```text
at every epoch boundary:
    W ← max( W₀ , clamp( W · model_blocks / expected_blocks , ÷4 , ×4 ) )
    expected_blocks = BPS · epoch_seconds
    W₀ = E / rate_max
```

`rate_max` is the one constant a network states: the most it will ever pay for a unit of compute
(the arming branch's `rate_sompi_per_giga`, 9 BILI per G MAC-eq, is exactly this number and needs no
recalibration). Below `W₀ · BPS` of supply the network pays `rate_max` and the models fill
`C / (W₀ · BPS)` of the cadence; above it `W = C / BPS`, the models fill the cadence and the rate is
`E · BPS / C`. `W` is a fold value (computed from the epoch counters the state already keeps), not a
new consensus object.

**D3 — the floor is the residual and it is unpaid.** The floor class draws against its own target
as today, walked to fill `BPS − model block rate`; the existing `DifficultyManager` on `bits` already
does this once model blocks are counted in the cadence (§17 keeps the network draw for now, so
`bits` needs no change). A floor block carries transactions and fees and no escrow. Nobody grinds
the floor for money, so a floor grinder cannot inflate `W`; the floor is liveness, not income.
(ADR-0039 W6′ wanted "no spam-hash lane could ever take the network": under D the floor cannot
take *issuance* at all, which is the stronger statement.)

**D4 — the reward.** A Final pays `E`, split producer / panel by ADR-0132 C's panel share with
`C_P = W` at acceptance. `ρ_m = 0.8 · E / W` for every `m` — the fairness goal by construction.
Upgrade C's `min(E, attempted · rate)` with `attempted = W` and `rate = E / W₀ ≥ E / W` is `E`, so
the shipped payout code is unchanged and its snapshot rows remain the observability of "how much
work this claim stood for". Fence 3's cap rule is retired (under D its utilization is `W / W₀ ≥
1000 ‰` for everyone: it would limit every class).

**D5 — the verification budget replaces the per-class caps.** A claim of class `m` is accepted iff
the class is in an admitted lifecycle state and

```text
room_m = ( ready_seats · reference_work · utilization · H  −  Σ_k inflight_k · CCU_k · seats ) / (CCU_m · seats)  >  0
H = the SHORTEST window among the admitted classes
```

— one network-wide replay budget over one common, conservative horizon `H`, less what every class
already holds in flight, in units of this class's claims. Per-class in-flight caps do not sum to
the panel (§3.2, and the simulation at ten times capacity shows the cheaper class starving while
both are "under their cap"); the budget does. The horizon must be common (with each class's own
window as its horizon a long-window class — Kimi, 31 spans — holds a budget the 4- and 10-span
classes never see, and the simulation locked both Qwens out) and it must be the shortest window: a
budget sized to the longest window admits claims the panel cannot finish inside the shorter
windows, and those void — at ten times capacity that alone spread the pay per CCU 1.7–4.3 ×
between classes; at the shortest window the spread is 1.26–1.31 with the panel at 63 % (§20.1).
What remains under saturation is a scheduling residue of the void rule, not a share: it is the
same for a hundred classes as for two, and the cure for saturation is seats (ADR-0133's grid), not
a table. Under saturation the free budget is taken in proportion to arrivals — no per-class
allocation, because an allocation is a share. The producer pre-checks the room (no forward is
wasted; without the pre-check the capacity-bound class's pay per CCU falls to 2.6–5.2, the
`--no-hold` run); `NoCapablePanel` and the window void as today.

**D6 — the lifecycle is a gate, not a price.** REGISTERED / PREFETCHING / HELD: claims refused at
acceptance (the registry's `ClassNotAdmitting`, kept as is, and extended to a class the fence
found without a row — every class past the work-target fence has a row or is refused). PROBATION:
one claim in flight, ten Finals to leave — a probe costs `W` of real compute, so it is
Sybil-resistant. ACTIVE_LIMITED / ACTIVE: D5's room only, replacing the per-class
`ClassInflightCapped`. `admission_permille` (50 / 100 / 1000) has no reader.

**D7 — share is a reader's number.** `share_m` is §5(c), computed by op 185/186 and the CLI from the
Finals in any window the reader chooses; it is not written to the state and nothing consensual reads
it. The execution lane keeps its own Final compute credit and quotas (ADR-0125: 45 % a domain, no
consecutive rounds) — economic share and execution share are different numbers about different
things, and `Economic share = 70 %, Execution share = 45 %` is allowed by construction.

**D8 — the network draw stays, for now.** Two draws waste forwards (ADR-0132 S) but do not bias
between models (`p_net` is common), so D is fair with or without it; folding it away is a separate,
fork-choice-touching change (§17) and is not needed for this ADR's goals.

## 8. Issuance — what can and cannot be fixed at once

Let `I` be issuance a second, `ρ_m` BILI per CCU of model `m`, `C = Σ C_m`.

1. **Equal pay:** `ρ_m = ρ` for all `m`.
2. **Fixed issuance:** `I = const`.
3. **Free compute:** `C` is whatever registrants and producers bring, unbounded.

`I = Σ ρ_m · C_m = ρ · C` under (1). With (2) and (3), `ρ = I / C` must fall as `C` grows, so a
*fixed* `ρ` (a rate constant that never moves) is incompatible with (2) + (3) — Bitcoin's arithmetic.
(1) is compatible with both: it is an equality *across models at an instant*, not a constancy over
time. Hence:

* the controller is one global variable — `W` (equivalently the effective rate `E / W`) — and it is
  a *feedback controller on model block count*, the same shape as the difficulty adjustment;
* a per-model share can never be that controller: it moves the *split* of a fixed cadence, and the
  split is a different quantity from `ρ`; making `ρ` equal through the split requires `s_m ∝ C_m`,
  i.e. measuring `C_m` with a lag and feeding it back (§3);
* `admission rate` and `global economic rate` are one number here: `rate = E / W`, `W ≥ W₀`.

Regimes: `I = min( rate_max · C , E · BPS )` (the floor's blocks are unpaid and complete the cadence
in the first regime). On testnet-11 today (`C ≈ 4.5 G MAC-eq/s` verifiable, `W₀ = 306 G`) the models
fill ~1.5 % of the cadence and issuance is `rate_max · C ≈ 40 BILI/s`; the simulation's
`issuance/epoch` column prints it (4,032 BILI an epoch at 1 × capacity, 403 at 0.1 ×, 20,000 at 10 ×
supply of the cheap class).

## 9. Fairness — the proof and the one limit

For `CCU_m ≤ W`: a forward wins with `p_m = CCU_m / W`, the expected compute a win is `W`, every win
pays `E`, so `ρ_m = 0.8 · E / W` — independent of `m`, of `r_m` and of how many classes exist. The
simulation prints `7.200` for every model in every steady scenario of every design D row, and a
`ρ` spread of `1.00`.

The limit: a model whose single forward exceeds `W` (`CCU_m > W`) wins every draw and is paid `E`
for `CCU_m > W` of compute — under-paid by `W / CCU_m`. One block cannot pay for more than one
block's work. It binds only while the network is small (`W = W₀` and a forward heavier than
`306 G` MAC-eq, i.e. no shipped class today; Qwen3.8-27B at 198.7 G is inside); it recedes as `C`
grows (`W = C / BPS`) and with BPS (ADR-0125). It is stated, not hidden: op 186 prints `CCU_m / W`.

Claim frequency differs by design and is fine: Qwen3.6 claims four times as often as Qwen2.5 and
is paid the same per claim; a 300 G Kimi claims a fifth as often as Qwen2.5 and is paid the same
per claim; per CCU all three are equal (the `20/30/50` scenario).

## 10. A new model — no bootstrap, no free share

Its ticket is `CCU_m / W` from its first admitted forward; the `new model at epoch 60` scenario
prints Kimi at its compute share (`0.200`) and `ρ = 7.200` in the epoch it is admitted. Registration
buys nothing but a row and a bond; PROBATION costs ten claims' worth of `W` compute — the same
compute an honest class spends producing — so it is Sybil-resistant without any share. Under the
shipped rule the same scenario prints Kimi in PROBATION at `ρ = 0.474` (A′) or at the floor's price
forever (A).

## 11. Windows — no longer consensus-critical

Under D no consensus rule reads a rolling share, so the window is a reader's choice: op 185/186
should print Final work over the last 10 and 100 spans and an EMA (`α = 1/32`), which the
scenarios (`0 → 50 %`, `50 → 0 %`, a new model, a `× 10` surge, an outage, HELD and back) show as
monotone and history-free after one window. The only consensual window that remains is the
execution lane's Final credit window (ADR-0125), which this ADR does not touch.

## 12. Execution permits

Unchanged. The lane reads Final compute credit and caps a domain at 45 % with no consecutive
rounds; that is *consensus power*, bounded by construction. Economic share (§5c) is *income*, and
may be 70 %. The two must never be one number, and under D there is no share number that could be
mistaken for both.

## 13. A hundred models

State a class: the registration (already), a lifecycle row (already), no target, no share, no
budget: ~200 bytes less than today. The boundary: one `W` update and `N` room checks (`O(N)`), no
grant-table sort, no per-class retarget. The panel: one budget over `N` classes (D5), so a hundred
cheap classes cannot each claim "under my cap" and jointly drown the seats. `W` does not depend on
`N`. The simulation's `Q36 as 100 classes` and `1000 tiny classes` rows are numerically identical
to their one-class rows under D; under A′ the hundred copies of Qwen3.6 push both Qwens into
PROBATION at `ρ` 0.18 / 1.27.

## 14. Attacks

| attack | shipped rule | D |
|---|---|---|
| register 100 / 1000 classes | cadence per class (§4), share ∝ 1/CCU | nothing: a block costs `W` from any class; a class without a panel is refused at acceptance (D6) |
| minor variants, graph a node apart | as above, each a new floor-price class | each priced by its own `CCU_m`; a variant that claims more CCU than it runs is a *graph*, and the panel replays the graph |
| cheap floor grinding to depress model pay | the floor holds 2.2 % by share | floor blocks are unpaid and outside `W`'s DAA: no effect on `ρ` |
| a surge to overload a rival's panel | HELD → PROBATION for the rival (§3.2) | the surge's own claims are refused by the budget (D5); a rival is throttled, not reset |
| a heavy model to pull the rate | Fence 3 limits it, then traps it (§3.4) | its ticket is `CCU_m / W`; it earns `E / W` like everyone |
| forged CCU (a graph that says more than it does) | as today | as today: `CCU_m` is derived from the graph the panel replays; a claim's work is what the replay costs |
| a producer's private draw count | never on chain | never on chain (`W` is the only work number) |

## 15. Migration — at a height, nothing regenerated

At the fence (`Params::palw_work_target: Option<ForkActivation>`, dormant everywhere; the same
family as `palw_model_registry`):

1. `W := E / rate_max` at the fence block (the payout fence's `rate_sompi_per_giga`).
2. The state's `class_targets`, `class_shares`, `epoch_budgets` are no longer written or read past
   the fence; they stay in the encoding and the root (a field never leaves — the DnsParams rule);
   readers print them as "pre-fence".
3. The registry step stops deriving shares and re-seeding budgets (`804af11d`'s seating and the
   sixth finding's budget refresh become unreachable past the fence and are removed at the next
   cleanup).
4. The floor's class DAA keeps running for the floor only (its target is the residual).
5. `check_palw_class_lottery_v3` reads `W` and the class's CCU; a floor attempt reads the floor's
   target.
6. Fence 3's cap rule is not evaluated past the fence.
7. Old nodes: the fork-id gate names the height (ADR-0120's rule: a fence at a scheduled height is
   invisible to the fork-id unless its height is distinct — pick one).

No regenesis; the devnet drill re-runs from genesis as it does for every fence.

## 16. Consensus change scope

* `palw_state_v2.rs`: the epoch boundary (a `W` update; skip class retarget / idle convergence /
  budget derivation / share growth for non-floor classes past the fence); the attempt acceptance
  (the lifecycle gate as it is, extended to unrowed classes; the room check in place of the
  per-class cap); the registry step (no shares past the fence); a `work_target` fold value in the
  state (in the root).
* `palw_admission_v2.rs`: `check_palw_class_lottery_v3` reads `W` (a class's target is derived); the
  budget check bypassed past the fence.
* `palw_model_registry_v1.rs`: no arithmetic change; `admission_permille` unread; the row loses
  nothing (the `priced_share_permille` of `804af11d` becomes "pre-fence").
* `config/params.rs`, `fork_id_v1.rs`: the fence, its validation (needs `palw_model_registry` and
  `palw_economic_payout` at or below it), the fork-id row.
* `kaspad/src/palw_producer.rs` / `palw_panel.rs`: the pre-check reads the room; the producer
  prints `CCU_m / W`.
* RPC: op 186 prints `W`, `CCU_m / W`, the room; op 185 prints rolling Final work shares.

Untouched: the ticket hash, the panel, licensing, the court, the execution lane, the free-prompt
lane, ADR-0132's snapshot and payout arithmetic, the memory budget (ADR-0136).

## 17. The smallest implementation, in order

1. **Shadow (no fence, this week):** `W` computed in the fold and printed (op 186) beside every
   class's `CCU_m / W`; the rolling Final work share on op 185; the pre-check's room as a log line.
   Measures on testnet-11 what §8 predicts before anything is armed.
2. **The fence:** D1, D2, D4 (retire Fence 3), D6's acceptance gate, the frozen fields (§15). ~400
   lines in `palw_state_v2.rs` / `palw_admission_v2.rs`, one param, pins.
3. **D5's budget** replacing the per-class in-flight cap (~150 lines; can ship with 2).
4. **Later, separately:** ADR-0132 S (one draw) — touches block validation and blue work; not
   needed for fairness.

Do not ship §15 on testnet-11's 6,001 flag day: ADR-0135 D5 as shipped is the §3.5 hazard, and D
is the replacement. Keep the registry fence (Upgrade A's rows, proofs and gate) and arm the work
target with it, or arm neither.

## 18. What becomes legacy

| legacy | today | after D |
|---|---|---|
| `class_shares`, `granted_share_table_v2`, `min_grantable_share_permille`, `min_base_class_share_permille` | consensus | frozen at the fence, encoding kept, unread |
| `class_targets`, `retarget_over_span_v1`, `converge_idle_target_v1`, `attempt_target_seed_v1`, `class_daa_max_factor` | consensus | the floor only |
| `epoch_budgets`, `derive_epoch_budgets_v2`, ADR-0123's release, `budget_tolerance_permille`, `EpochBudgetExceeded` | consensus | frozen, unread |
| ADR-0054 / ADR-0107 share growth, `palw_share_growth_final` | dormant | delete |
| ADR-0135 D5 shares from admission, `admission_permille`, `palw_class_shares_from_admission_v1` | consensus (dormant fence) | unread past the fence |
| `804af11d`'s `priced_share_permille` and the seating | consensus (dormant fence) | shadow only; frozen past the fence |
| `ClassRegistered.share_permille`, `initial_target`, op 180's `initial_target` | registration | ignored past the fence (a registration carries its CCU) |
| Upgrade C's `min(E, attempted · rate)`, the snapshot | consensus (dormant fence) | kept — degenerates to `E`; the snapshot stays as the work record |
| Fence 3's cap rule, `cap_utilization_permille` | consensus (dormant fence) | retired at the fence |
| the network draw against `bits` for model blocks | consensus | kept (D8); ADR-0132 S later |

## 19. Can the `share` consensus parameter be deleted?

Yes. Every consensus reader of a share is one of: the class DAA's expectation, the epoch budget,
the grant table's conservation, the registry's derivation, the seed. Under D each has either no
reader or a reader that is the floor's residual rule. What remains that *looks* like a share:

* the **floor's cadence** — not a share but the residual `BPS − model rate`, and unpaid;
* the **free-prompt lane split** (`fp_attempt_share_permille`) — a lane split between attempt and
  receipt blocks, not a model share; out of this ADR's scope and unchanged;
* the **execution quotas** — Final credit with a 45 % cap, consensus *power*, not income.

The number a person could still set is `rate_max` (equivalently `W₀`), and it is a ceiling on pay,
not a split between models. No rule says `Qwen = 40 %`; the fraction a model holds is what its
producers did, read off the chain by whoever asks. A network that in five years holds a hundred
models holds a hundred rows and one `W`.

## 20. Simulation

`scripts/sim/palw-share-sim.py` — a fluid model with integer block semantics where the class DAA reads
counts; the shipped arithmetic for A (§1), the interim seating and Upgrade C for A′, D as §7, E and
B as §6. `python3 scripts/sim/palw-share-sim.py --supply-x 1` prints the table below (testnet-11's
escrow, eight ready seats, the profile globals, `rate_max = 9 MSK / G`); `--supply-x 10` the
capacity-bound regime; `--no-hold` the waste without the producer's pre-check. Columns: MSK per
G MAC-eq to the producer, the spread of that across models, block and Final-work shares, claims a
span, the ticket, supply utilisation, the block-count oscillation (CV over ten epochs), the HELD
fraction, the floor's block share, blocks a span, issuance an epoch, epochs to converge.

Headline rows at supply = the panel's capacity (the full table is the script's output):

| scenario | design | model | MSK/G-CCU | ρ spread | Final work share | blocks a span | note |
|---|---|---|---|---|---|---|---|
| Q25/Q36 = 50/50 | A | Q25 / Q36 | 9.9e-4 / 4.0e-3 | 4.02 | 0.50 / 0.50 | 0.11 | both at the floor's price, PROBATION for 120 epochs |
| | A′ | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | **0.49** | fair while uncapped; the cadence collapsed (§3.5) |
| | **D** | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | |
| Q25/Q36/Kimi = 20/30/50 | A | ×3 | 9.9e-4 / 4.0e-3 / 2.8e-4 | 14.2 | 0.2 / 0.3 / 0.5 | 0.11 | |
| | A′ | ×3 | 6.26 / 7.20 / 7.20 | 1.15 | | 0.27 | |
| | **D** | ×3 | 7.20 ×3 | 1.00 | 0.2 / 0.3 / 0.5 | 5.00 | claims a span 0.015 / 0.022 / 0.037 |
| new model at epoch 60 | A′ | Kimi | 0.474 | 15.2 | 0.20 | 0.57 | PROBATION at the end |
| | **D** | Kimi | 7.20 | 1.00 | 0.20 | 5.00 | at its share the epoch it is admitted |
| Q36 as 100 classes | A′ | Q25 / Q36#* | 0.18 / 1.27 | 7.04 | | 0.12 | both in PROBATION |
| | **D** | Q25 / Q36#* | 7.20 / 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | identical to the one-class row |
| 1000 tiny classes, Q25's compute | A | S#* vs Q25 | 0.168 vs 9.9e-4 | **169** | | 0.11 | the cheap class paid 170 × per CCU |
| | **D** | S#* vs Q25 | 7.20 / 7.20 | 1.00 | 0.40 / 0.40 | 5.00 | |
| Q36 supply × 10 at 60 | A′ | Q25 / Q36 | 7.20 / 2.97 | 2.42 | 0.09 / 0.91 | 0.34 | Q36 through HELD → PROBATION |
| | **D** | Q25 / Q36 | 7.20 / 7.20 | 1.00 | 0.09 / 0.91 | 5.00 | issuance 4,032 → 20,000 an epoch |
| Q25 panel outage 60–70 | **D** | Q25 | 7.20 | 1.00 | 0.50 | 5.00 | claims refused during, back at once |
| supply × 0.1 at 60 | **D** | both | 7.20 | 1.00 | 0.50 / 0.50 | 5.00 | issuance 4,032 → 403; `W` at `W₀` |

### 20.1 Ten times the panel's capacity — the regime where verification binds

`--supply-x 10`: the models' producers offer ten times the compute the eight seats can replay. The
lottery is unchanged; what binds is D5's budget and the window's void rule.

| scenario | design | model | MSK/G-CCU | ρ spread | Final work share | claims a span | Finals a span | supply used | note |
|---|---|---|---|---|---|---|---|---|---|
| Q25/Q36 = 50/50 | A′ | Q25 / Q36 | 1.12 / 0.67 | 1.67 | 0.64 / 0.36 | | | | HELD 70 % / 72 % of the time (§3.2), cadence 3.65 of 5 |
| | D, `H` = longest | Q25 / Q36 | 4.94 / 2.95 | 1.68 | 0.63 / 0.37 | 0.34 / 0.34 | 0.23 / 0.14 | 0.92 | Q36's 4-span claims void behind Q25's 10-span ones |
| | **D, `H` = shortest** | Q25 / Q36 | 6.90 / 5.49 | **1.26** | 0.56 / 0.44 | 0.23 / 0.23 | 0.22 / 0.18 | 0.63 | producers hold at the budget; no HELD |
| | E | Q25 / Q36 | 4.19 / 4.09 | 1.02 | 0.51 / 0.49 | | | 1.00 | equal, but both at 58 % of D's pay: the scaled ticket wastes forwards |
| | D, `--no-hold` | Q25 / Q36 | 5.21 / 0 | | 1.00 / 0 | | | 1.00 | without the pre-check the refused forwards are the loss |
| Q25/Q36/Kimi = 20/30/50 | A′ | ×3 | 0.82 / 0.43 / 1.00 | 2.30 | 0.29 / 0.25 / 0.45 | | | | Q36 and Kimi HELD 40–72 % of the time |
| | D, `H` = longest | ×3 | 2.18 / 1.09 / 4.68 | 4.30 | 0.14 / 0.11 / 0.75 | | | 0.30 | Kimi's 31-span window survives the drain, the Qwens' claims void |
| | **D, `H` = shortest** | ×3 | 6.90 / 5.49 / 7.20 | **1.31** | 0.21 / 0.25 / 0.54 | 0.026 / 0.039 / 0.065 | | 0.18 | |
| Q36 supply × 10 at 60 | A′ | Q25 / Q36 | 4.39 / 0.55 | 7.93 | 0.82 / 0.19 | | | | Q36 HELD 80 % of the time |
| | **D, `H` = shortest** | Q25 / Q36 | 6.90 / 5.49 | 1.26 | 0.11 / 0.89 | 0.09 / 0.93 | 0.09 / 0.71 | 0.25 / 2.53 | Q36's producers hold nine tenths of the time: the panel is the limit, and it says so |
| 1000 tiny classes | D, `H` = longest | Q25 / Q36 / S#* | 6.00 / 4.01 / 2.78 | 2.16 | 0.56 / 0.19 / 0.26 | | | 1.00 | identical to the one-class row (below): class count does nothing |
| | D, `H` = longest | Q25 / Q36 / S | 6.00 / 4.01 / 2.78 | 2.16 | 0.56 / 0.19 / 0.26 | | | 1.00 | |

Read: (i) under D the *lottery* stays fair at any load; the residual spread under saturation is the
void rule acting on claims the panel cannot finish inside their window, bounded at 1.3 × with the
conservative horizon and zero at or below capacity; (ii) the shipped rule at the same load is HELD
half to four fifths of the time with the pay per CCU spread 1.7–7.9 ×; (iii) E buys equality by
under-paying everyone; (iv) the pre-check is load-bearing; (v) a thousand classes behave as one.
The regime itself — supply ten times what the panel verifies — is testnet-11 today (ADR-0132: Final
13 %, 87 % `receipt_timeout`), and its cure is seats, which no share table supplies.

## 22. What the shadow built, and what it found

Built 2026-09-18 (`consensus/core/src/palw_work_target_v1.rs`; `palw_state_v2.rs`;
`processor.rs`; op 186; `misaka palw registry`):

* the arithmetic: `palw_work_floor_v1` (`W₀ = escrow · 10⁹ / rate`), `palw_work_target_step_v1`
  (the clamped epoch step, floored), `palw_work_ticket_target_v1` (`MAX · min(1, CCU / W)`),
  `palw_work_ratio_permille_v1`, `palw_expected_forwards_q32_v1`, `palw_effective_work_v1`,
  `palw_panel_room_v1` (D5), `palw_final_work_shares_v1` (§5c);
* the state carries `work_target_shadow` (`W`, `W₀`, the boundary block's network draws in Q32
  and their product, the epoch, the closed epoch's model blocks against the cadence's attempt-lane
  slice) and `final_work` (finalized work by closed epoch and class, a hundred epochs kept) in one
  carriage tail (`0xAC`) that never enters the state root — a state folded with the shadow has the
  root of the same state folded without it (pinned); the delta carries them (54, 55) so a reorg
  reverts them (pinned);
* the fold's input rides `PalwTransitionExtrasV1::work_target` (the rate: the payout fence's where
  armed, else 9 MSK/G; the block's `bits`; the class DAA's clamp; every class's work), built by
  the node for every block of a ConsensusV2 network;
* a Final of a model class writes `W` of work — the Upgrade C snapshot's attempted compute where
  one was taken, the shadow's `W` otherwise; the floor and the receipt lane write nothing;
* op 186 prints the shadow (`W`, `W₀`, network draws, effective work, epoch, census, rate, the
  panel's in-flight replay, the budget's horizon) and, a class, `CCU / W` in permille, the expected
  forwards a win, the ticket the work target would set beside the class target the shipped rule
  sets, the panel room in that class's claims, and the finalized-work share over ten and a hundred
  epochs; the CLI prints them as columns.

Found while building it — corrections to this text:

1. **Under the shipped double draw, `W`'s own DAA is redundant.** The network draw against `bits`
   already holds the cadence; model blocks can never exceed the attempt lane's slice of it, so
   §7 D2's step sits at `W₀` and the second draw is the controller above the cap (`rate ·
   p_net`). The fence therefore needs no new DAA state: the ticket reads `W₀` off the block's own
   escrow and the rate — stateless — and `bits` does the rest. The epoch step stays in the module
   for the single lottery (ADR-0132 S), where it is the controller; the shadow folds it against the
   cadence (`epoch_length · split`) so it is meaningful in both worlds, and prints the effective
   work `W · network draws` beside it.
2. **The registry does gate at acceptance** (`ClassNotAdmitting`, `ClassInflightCapped`), for rowed
   classes under the fold; §4 and D6 are corrected above. What the gate does not cover — unrowed
   classes, and everything below the registry fence — stands.
3. **The fixtures' `W₀` is tiny** (620 000 sompi at 10 000 sompi/G: 62 G MAC-eq), so a toy class of
   800 000 MAC-eq prints `CCU / W` as 0 ‰ and 77 500 forwards a win; testnet-11's `W₀` is 306 G
   MAC-eq against forwards of 21–199 G (69–650 ‰). The shadow's numbers on a live network are what
   §17's measurement is for.

### 22.1 The dormant fence, built (2026-09-18, §17 step 2 and 3 together)

`Params::palw_work_target: Option<ForkActivation>` — `None` on every shipped preset (pinned:
`adr0137_the_work_target_fence_is_dormant_everywhere_arms_by_height_and_is_refused_alone`), hashed
Some-only, in the fork-id's arm-by-name list, refused by `validate_palw_v2` unless
`palw_model_registry` and `palw_economic_payout` are armed at or below it (it prices the registry's
rows at the payout's rate), `never` normalised to dormant; `--palw-work-target-devnet=<daa>` for a
drill (devnet/simnet only; needs the registry and payout flags). **No activation height is set, and
none is to be set before the devnet drill has passed.** Past the fence, at a block:

1. **The lottery** (`check_palw_class_lottery_v4`): a model class's ticket target is
   `MAX · min(1, CCU_m / W₀)` with `CCU_m` its registry row's `economic_ccu_per_claim` and
   `W₀ = escrow_of_this_block · 10⁹ / rate` (`palw_work_floor_for_block_v1`; a block with no escrow
   prices the hardest floor, so nothing wins on a block that pays nothing); the floor class keeps
   its class target; a class without a row has no price (`ClassWorkUnknown`). The producer draws
   against the same ticket (`palw_producer_facts_v2` takes the floor). Stateless, by finding 1.
2. **The epoch budget is not read** for a model class (`PalwEpochBudgetFencesV1::work_target_floor`
   is `Some`); the floor's stays.
3. **A model class's target is not walked** (`apply_class_retargets` skips it; the floor's class
   DAA and the receipt lane are untouched).
4. **The registry derives no share, refreshes no budget, seats no price** (`step_model_registry`
   returns after the rows; `804af11d`'s seating is unreachable); the rows, proofs and the lifecycle
   machine run as before.
5. **Fence 3's cap rule is off** (`cap_ok` is true; the reading is still taken and printed).
6. **An unrowed class is refused at acceptance** (`ClassNotAdmitting { state: "no row" }`) — below
   the fence it was never gated.
7. **One verification budget replaces the per-class cap**: `PanelRoomExhausted` where this class's
   ready seats' replay over the shortest admitted window, less every model class's replay in
   flight, holds no more of its claims (`panel_room_v1`); and the lifecycle's utilization reads the
   same budget (replay in flight over budget), so a class the room admits is not a class the
   lifecycle holds — the first draft's count-based utilization held the class at the span boundary
   after nine admitted claims.
8. **Nothing else moves**: the reward (Upgrade C's `min(E, attempted · rate)`, which the work
   target makes `E`), the panel share, the snapshot, the network draw, the frozen fields' encoding.

Pinned in `palw_state_v2.rs` (`adr0137_past_the_fence_*`) and `palw_admission_v2.rs`: the lottery
reads `CCU / W₀` (a class heavier than `W₀` draws the whole ticket; `W₀ = MAX` prices 800 000 of the
space; no floor at hand is the shipped verdict; the floor keeps its class target; a rowless class
has no price and is refused at acceptance while the same class is never gated below the fence); the
epoch budget is not read for a spent entrant; a governed boundary past the fence moves no share, no
target and no budget, seats no price, and a saturating rate does not limit the class; a panel of
seven seats at 4 M MAC-eq a span holds exactly its budget of claims (nine, where the per-class cap
is 4.7 M), the tenth is `PanelRoomExhausted`, the reverted ninth gives its room back, and folded
twice the fenced boundary has one root.


### 22.2 The rooted work target, built for S (2026-09-18, commit 7c4f350e)

§22.1 read `W₀` statelessly because under the double draw `bits` is the controller. ADR-0132 S removes the
second draw, and then W has to move: past `palw_work_target` the chain now holds `PalwChainStateV2::work_target`
(`PalwWorkTargetV2` — work, floor, epoch index, the closed epoch's model blocks against its expected blocks;
carriage tail 0xAD, delta 56, `work_target/v1` in the root). It steps at every epoch boundary the way `bits`
does — closed model blocks over `epoch × fp_attempt_share‰`, clamped by `class_daa_max_factor`, never below
`W₀` — and the single lottery's floor is `max(W₀, W)` (`palw_work_lottery_floor_v1`). Below S nothing reads it
but the reader (op 186 / `misaka palw economics`); the pin is the shadow's: the root moves only past the fence,
the delta reverts, and a quiet epoch returns W to the floor. testnet-11 arms S with the work target at 6,001
(2026-09-18), so the lottery reads the rooted W from the flag day.


### 22.4 Audit finding (2026-09-18): the class target the fence retires still priced `pwu`

The pre-arming audit (`docs/palw-audit-2026-09-18-6001.md`, finding C-2) found the rule this ADR
retires still read one line away from the lottery it replaced. `check_palw_class_lottery_v4` priced
the draw at `MAX · min(1, CCU/W)`, but the admission's `DerivedV1` arm still demanded
`attempt.pwu == palw_pwu_v1(state.class_target, pwu_per_inference)` — a registrant-declared value no
rule re-prices past the fence. Two consequences, both live at 6,001: every honest model block was
disqualified, because the producer's facts derived pwu from the new ticket (an eight-node devnet
drill logged `PwuClaimNotDerived` fifteen times on each node while the class never opened a claim);
and an attacker registering with `initial_target = 1` could claim `pwu = u64::MAX`, which the chain
then demanded, and buy `safe_weight` and `bounded_immature` — fork choice — for a collateral that
does not scale with it (`palw_exposure_pwu_v1` reserves `pwu_per_inference`).

`palw_effective_class_target_v1` is now the one spelling of "the target that governs this class at
this block", read by the lottery, by the pwu derivation and by the producer's facts. A rule that
reads a class's target and does not read it there is a bug by construction.

### 22.3 Drill finding (2026-09-18): the producer's pre-check read the budget past the fence

The devnet drill with the registry, the payout and the work target armed at 20 passed its first phase
(rows opened, proofs landed, the grace passed, a restarted node held the rows) and then stalled at the
class's first claim: node-3 held `this class's epoch budget is already spent` for an hour while admission
past the fence prices a claim against `CCU / W` and reads no class budget at all — the pre-check was
holding on a rule the chain had stopped reading. `PalwProducerFactsV2::epoch_budget_read` (false past
the fence) makes `has_epoch_room` true where nothing is read; pinned in the producer's own test. The
same drill, re-run on the fixed build with all four fences at 20, is the release's evidence (§7.5 of
ADR-0132 for the fourth).

## 21. Number hygiene

Written 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement` as 0137, the next free
number in the index. A concurrent claimant renumbers the later writer.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md -->

</details>

<a id="source-0146-a-coefficient-is-justified-by-the-arbitrage-it-permits"></a>

<details>
<summary>0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md — 原文 L1–L293</summary>

[原文を開く](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) · SHA-256: `5f590e5e1c46f90f9376f5a43368e48bf8118b099a3362f1cb4ef19a1480bdd5`

<!-- ADR_SOURCE_BEGIN:0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md -->
~~~~~~~~markdown
# ADR-0146 — A coefficient is justified by the arbitrage it permits, not by being right

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** SEARCH IMPLEMENTED 2026-09-21. No coefficient table, no fence, no `Params` field.
`palw_arbitrage_search_v1` is the committed program Rule R7 asked for (`cargo test -p
kaspa-consensus-core palw_arbitrage_search`). The bound it reports for the derivation is
1.000000× across the four shipped classes and across the admissible declaration space of one
class, so there is no table to write and no scalar to arm. §6 (resident vs paged) stays an open
cost question, not a price lever.

ADR-0145 named the coefficient table its hardest open question and deferred it here.

---

## 1. The trap this ADR exists to avoid

ADR-0145 says canonical work is a vector and that "the coefficients that turn a vector into an
economic unit are protocol-set". That sentence hides a problem: **a coefficient table is a place
where somebody decides numbers**, and the operator's standing rule forbids hand-written per-model
multipliers. A table of hand-picked weights is the same object one level up — it just spreads the
decision across operations instead of across models.

The existing table already understood this and took a position (`palw_economic_compute_v1.rs`):

> Chosen from the arithmetic each kernel performs at the integer engine's batch of one, **not from
> any host's timing**.

Every entry is a **count**, not a preference. That is why it has survived review, and it is also
exactly why it fails: arithmetic counts do not see memory traffic, so a 35B mixture activating 3B
and a 1.5B dense row can execute the same MAC-equivalents and cost very different amounts.

So the table cannot be fixed by picking better numbers. It has to be fixed by deciding **what makes
any number legitimate.**

---

## 2. Most dimensions need no coefficient at all

The first result is that the free-parameter problem is much smaller than it looks, because most of
the vector is derivable from facts already on chain.

**The existing unit already doubles as a byte count.** `matmul_mac` is "one 8-bit weight × integer
activation multiply–accumulate, which at the integer engine's batch of one is also **one weight byte
streamed**". So for 8-bit weights, arithmetic and traffic coincide — and they diverge exactly in
proportion to the quantisation format, which the canonical class descriptor carries (ADR-0145 §3).

```
traffic_term = MAC-eq × real_bits_per_weight / 8
```

Q4_K_M streams half a byte per weight; W8A16 streams one. **No new trusted input, no hand-picked
number** — the format is a declared-and-verified property of the artifact, not a preference. This is
the red-team's C9 and it closes the one gap the current table is known to have.

The same holds elsewhere. A routed expert row's cost follows from how many experts the row activates
— a graph fact. Attention follows from heads × head dimension × true KV length — an execution fact.
KV read and write follow from the cache the receipt commits to (ADR-0145 §6).

**Rule R1.** A dimension that can be derived from the canonical graph, the artifact's declared and
verified format, or the execution facts MUST be derived. A coefficient is permitted only where no
derivation exists.

That leaves far fewer free parameters than "a coefficient table" suggests: essentially the
**relative price between dimensions** when the vector is collapsed into one economic unit.

---

## 3. The coefficients that remain cannot be validated by being "right"

Nobody can demonstrate that a dense MAC is worth 1.0 and a KV byte 0.31. Any such claim is a
measurement on one host, one runtime and one batch size, and ADR-0038 Decision D already refuses to
let a wall-clock second reach fork choice.

So this ADR does not ask for correct coefficients. It asks for a **bound**.

**The requirement is not that reward tracks cost. It is that no participant can choose a
configuration whose reward-per-real-cost materially beats another's.**

That is a property of the table *over the space of admissible configurations*, and unlike
correctness it is checkable — by exactly the method the 2026-09-19 red-team used: enumerate the
admissible space and search it for the extreme.

**Rule R2.** A coefficient table is accompanied by its **measured arbitrage bound**: the maximum
ratio of reward-per-unit-real-cost between any two admissible configurations, found by adversarial
search over profiles, quantisations, canonical jobs and execution modes. The table is proposed
together with the search that produced the bound, and the search is committed and re-runnable.

The table is not "the right numbers". It is "numbers whose worst case somebody measured, and here is
the program that measures it".

For scale, the bounds on the basis in force today, from the audit's own probes:

| basis | measured spread | what the spread is over |
|---|---|---|
| STEP leaves | **427×** | admissible canonical jobs, one graph |
| STEP leaves | **101×** | tilings of one graph |
| STEP leaves | **6.8×** | the four live classes, monotone in model width |

A candidate table is not an improvement because it is better reasoned. It is an improvement when its
number in that column is smaller, measured the same way.

---

## 4. What the bound must be, and what happens if none is reachable

**Rule R3.** An arbitrage bound is acceptable only if it is **smaller than the efficiency gains the
protocol intends to reward**. If representation choice can move reward-per-cost by 10× while a
genuinely better model moves it by 3×, the economy rewards representation over engineering, and the
table fails no matter how principled its derivation.

This gives a concrete target rather than a wish: the bound must sit below the spread of real
efficiency across the models the network expects to run. That spread is itself measurable, and
measuring it is part of proposing a table.

**If no coefficient vector reaches an acceptable bound, the vector must not be collapsed.** The
fallback is to keep the dimensions separate and give each its own eligibility budget, so a
configuration that is extreme in one dimension exhausts that dimension's budget rather than
converting it into a general claim on the reward pool. Collapsing to one number is a convenience,
not a requirement, and this ADR declines to assume it is achievable.

---

## 5. Governance

**Rule R4 — nobody in the economy sets them.** Not miners, not class registrants, not panel seats,
not model owners. A coefficient is never a field in any object a participant signs. This is
ADR-0145's I1 and I2 applied to the table itself.

**Rule R5 — a table is versioned whole, never edited.** The current table already states this ("a
change to any entry is a new version, never an edit") and it stays. A claim records the table
version it was priced under, so history stays re-verifiable and no repricing is retroactive.

**Rule R6 — a new version is a consensus change.** Its own fence, its own drill that crosses that
fence, its own adversarial review, and its arbitrage bound re-measured and published with it. Per
ADR-0144 §6 item 0 and this repo's own working rule: the height is chosen after the evidence, not
before.

**Rule R7 — the bound is a gate, not a note.** A version that does not ship with a re-runnable
search and a bound does not arm. "We reasoned about it carefully" is what the current table did, and
the current table is 427× wide.

---

## 6. The experiment that has not been run

ADR-0145 says any coefficient today is "a guess wearing a number", and that remains true until this
runs:

> Both live classes' draw jobs replayed warm on **one host**, with the artifact fully resident, and
> then not resident. If the gap survives residency, MAC-equivalents need the traffic term of §2.

**It must be run on one host** precisely because a comparison across hosts measures the hosts. And
the figures that looked like this experiment were not: the red-team established that the 13.7× gap
cited earlier came from `#[cfg(test)]` fixtures in `palw_verification_profile_v1.rs`, and that the
production path overrides a measured p99 with a reference-rate estimate. That correction is why this
section exists rather than a table.

### It cannot be run on this fleet, and that is the more useful result

Measured 2026-09-19 across every host the network runs on:

| host | RAM | `qwen36.palwq36` | `qwen25-1.5b-a16.palwart` |
|---|---|---|---|
| 169.58.232.113 | 23 G | 34.0 G | 1.7 G |
| 5.104.81.23 | 23 G | 34.0 G | 1.7 G |
| 169.58.39.220 (ibm) | 23 G | present | present |
| 95.111.236.186 | 11 G | absent | 1.7 G |

**No host can hold the hybrid class resident.** The artifact is 34 G on machines with 23 G, and
ADR-0112's loader takes at most a fifth of the artifact's weights within what the host has past a
reserve — so on this fleet roughly four fifths of every Qwen3.6 inference is read from disk, every
time, while the 1.7 G dense class is resident on every host.

So the residency question §6 poses is not open on this network: **residency is binary here and the
two live classes sit on opposite sides of it.** The cost difference between them is not a subtle
memory-traffic term to be calibrated, it is RAM against disk — and neither the leaf basis nor the
MAC basis contains a term that can express it. ADR-0131's table says as much about itself: its
entries are counts of arithmetic, "not from any host's timing".

Two consequences for this ADR:

* the traffic term of §2 is necessary and still not sufficient — it distinguishes Q4 from W8 but not
  resident from paged, and `artifact_bytes` against a host's cache budget is the quantity that does.
  Whether a HOST-dependent quantity may reach the price at all is a real question: ADR-0038 Decision
  D refuses wall-clock, and artifact size is not wall-clock, but the budget it is compared against
  is a property of a machine. **This is the open design question the experiment would have refined
  and did not create.**
* §4's fallback moves from contingency toward the likely answer. When two classes differ by whether
  they fit in memory, one scalar relating their work is unlikely to bound arbitrage below the
  efficiency spread the protocol wants to reward.

**And the experiment was not run rather than run badly.** The bench harness exists
(`palw-worker-iso-bench --mode v2-replay-bench`, fleet-measured at p50 2,885 ms for 16 decode tokens
and 84,513 ms for 512 — a marginal 164.6 ms per decode token on a 2B model). Running it against the
34 G artifact would mean thrashing the page cache of a host carrying two live testnet-11 nodes,
while a third fleet node was already 330 DAA behind. A measurement that costs the network its
liveness is not evidence, and the number would have described one machine anyway.

**What it needs:** a host that can hold 34 G resident and carries no fleet node. Until one exists,
§2's derived terms ship and §4's bound is measured over the admissible space — which needs no host
at all, because it is arithmetic over profiles.

**What the experiment decides.** If residency explains the gap, §2's derived traffic term is
sufficient and the collapse may have no free parameters at all. If it does not, there is a real
dimension neither arithmetic nor bytes captures, and §4's fallback — separate budgets — becomes the
likely answer rather than the contingency.

---

## 7. What this ADR does not decide

The dimensions (ADR-0145 §4 holds them provisional), the numbers, and any height. It decides what a
proposal must carry to be considered: a derivation for every dimension that admits one, a search
program, a measured bound, and a comparison of that bound against real efficiency spread.

---

## 8. Done means

> No coefficient is set by anyone in the economy. Every dimension that can be derived is derived.
> The few that cannot ship with a committed program that searches the admissible space and reports
> the worst arbitrage they permit, and that number is smaller than the efficiency differences the
> protocol means to reward. If no such table exists, the vector is not collapsed.

The test of this ADR is not whether the numbers look reasonable. It is whether, a year from now,
somebody can re-run one command and find out whether they still are.

---

## 9. The bound was measured, and the answer was not a table (2026-09-20)

This ADR was written expecting to need coefficients and a search that bounds what they permit.
It got neither, because the spread it set out to bound turned out not to be a coefficient problem.

§3's table recorded the basis in force: 427× over admissible canonical jobs, 101× over tilings
(corrected by the red team to a 134.9× band — re-tiling the shipped row can only go DOWN, since
tiles finer than 24 push the worst case past 2^26 and admission refuses them), and 6.8× across the
four live classes. §2's first result was that most dimensions need no coefficient at all. The
second result, which this section records, is that the collapse needed none either.

**Measured past the ADR-0145 economic bundle**, across the whole declaration space of one class and
across all four shipped classes:

| quantity | before | after |
|---|---|---|
| fork-choice weight per executed MAC-equivalent | **24,572×** | **1.000000×** |
| pay per giga-MAC-equivalent executed | 6.8× spread, monotone in width | **899,999,999 sompi, identical** |

The pay figure is ADR-0132's `rate_sompi_per_giga` — one protocol constant, kept one by the
operator's standing rule that there are no per-model multipliers.

**Why there was no table to write.** Leaves count ACTIVATIONS and the cost table counts WEIGHTS, so
the declared basis diverges from arithmetic monotonically in model width. A coefficient vector
would have been an attempt to *correct* that divergence. Pricing on the arithmetic directly
*removes* it. §4's fallback — keep the dimensions separate, give each its own budget — was written
as the likely answer if no vector reached an acceptable bound. It is not needed, and not because a
vector was found.

**R2 and R7 are satisfied differently than they expected.** The bound is not the output of an
adversarial search over a candidate table; it is a property of the derivation, asserted at every
point of the admissible space by a test that runs in the suite
(`the_better_model_is_paid_less_is_neutralised`, `no_registrant_writable_field_increases_the_weight`,
`legal_representations_of_one_inference_must_price_identically`). The re-runnable program R7 demands
exists; it is `cargo test`.

**§6 stays open, and stays honest.** MAC-equivalents carry no memory-traffic term. Two classes
running equal arithmetic against unequal residency cost their operators unequally, and on this
fleet that is RAM against disk. §2's derived `real_bits_per_weight` distinguishes Q4 from W8 and
does not distinguish resident from paged. What changed is the shape of the question: that residual
is a difference between classes in COST, not a lever a registrant can pull in PRICE. An arbitrage
would block the bundle. A cost asymmetry the protocol does not price is an open design question,
and it is recorded here rather than closed.

---

## 10. Implementation (2026-09-21)

The committed program Rule R7 asked for is `palw_arbitrage_search_v1`
(`cargo test -p kaspa-consensus-core palw_arbitrage_search`). It reports a 1.000000× bound
across the four shipped classes and across the admissible declaration space of one class, and
the provisional scalar did not swallow traffic. There is therefore **no coefficient table to
write and no scalar to arm**. ADR-0144 P4 is satisfied: 0147–0149 may arm without contradicting
representation-neutral accounting. They stay `None` on every shipped preset — choosing a height
is a different commit from knowing the height would be legal. §6 (resident vs paged) remains
the open cost question this ADR already recorded.
~~~~~~~~
<!-- ADR_SOURCE_END:0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md -->

</details>

