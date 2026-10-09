# ADR-0054: A class's cadence share follows its own production

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
