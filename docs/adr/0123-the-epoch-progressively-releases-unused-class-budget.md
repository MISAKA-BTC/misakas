# ADR-0123 — The epoch progressively releases unused class budget

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
