# ADR-0033: The credit gate, wired — how `credit(C)` becomes a consensus fact

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (design; activates nothing, and cannot be activated until its stated
preconditions are met).** ADR-0028 §1 defined `credit(C)` as a predicate. This ADR decides
**where it is evaluated, what state it reads, how it survives reorgs, and what happens the
moment it says yes** — the B14 wiring, specified so a future Stage-2 release implements one
design rather than inventing three.
Date: 2026-08-16
Relates to: ADR-0028 §1 (the predicate), §4 (the economics, with the 2026-08-16 leverage
amendment), ADR-0029 (the Stage-1 carriage and store the gate reads), ADR-0027 §6 (the stage
ladder), ADR-0032 (fee/bounty value flows), `docs/palw-class-activation-gate-status.md` (the
§12 ledger this promotion answers to), the capability credit walk in
`consensus/src/pipeline/virtual_processor/processor.rs` (the template).

## Preconditions — none of this may ship before all of them hold

1. Stage-1 carriage live: dedicated subnetwork ids, stateless validators, and the
   accept/revert/backfill store (ADR-0029 Stage 1).
2. ADR-0028 §4e's leverage remedy **chosen and encoded** — either the per-validator credited-job
   cap or a fractional `base(C)` (the B15 finding; the current parameters violate
   `max_leverage ≤ 1` by ~10⁴×, so wiring credit without this mints against nothing).
3. §12 gate items 2, 4, 5, 10, 11-external and 12-exercise met (`palw-class-activation-gate-status.md`).
4. A Stage-1 soak with zero unexplained class freezes.

## Decision

### 1. Where the gate is evaluated

`credit(C)` is evaluated **in the virtual processor's chain walk, at the block whose accepted
DAA score first reaches `challenge_close_daa(C)`** — never at commitment time, never by a
timer. The walk already visits every accepted transaction of every chain block (the capability
credit walk does exactly this); the PALW gate is a second consumer of the same walk over the
Stage-1 carriage store.

Consequences of that placement, all deliberate:

* **The gate is a pure function of chain state**, so every node computes the same answer at the
  same block — the property that lets credit be consensus rather than telemetry.
* **A job's credit is decided once**, at a specific block, and is thereafter history.
* Wall-clock never enters. Under a stall, DAA stalls, and every deadline stalls with it
  (ADR-0028 §3's stall rule) — the gate simply is not reached.

### 2. What it reads

Exactly four facts, all already carried or derivable:

| fact | source |
| --- | --- |
| the commitment `C` and its `daa(C)` | carriage store (kind 0x01), accepted-DAA-indexed |
| assigned panel at the anchor | `select_replay_panel_v1` over the bonded set at `daa(C)+Δ_bind` — derived, not stored |
| attestations against `C` | carriage store (kind 0x02), filtered to panel members, root-equal, on-time |
| refutations against `C` | carriage store (kind 0x05/0x06), any accepted one, adjudicated |

No off-chain input, no oracle, no timer. The panel is *derived* rather than stored precisely so
a stored panel cannot drift from the rule that produced it.

### 3. The predicate, verbatim from ADR-0028 §1

```
credit(C) ⟺ W_challenge(C) closed
          ∧ ≥1 assigned attestation with an independently recomputed root equal to C's
          ∧ no accepted refutation against C
```

Zero attestations ⇒ credit 0. The panel is never shrunk to make a job creditable, and a
refutation accepted at any point inside the window voids credit regardless of attestation
count. A refutation accepted **after** the window still convicts (slash is not window-bound)
but does not retroactively revoke credit — that asymmetry is deliberate and is why the
Stage-0 ledger counts "credited-and-later-refuted" as its own tail metric.

### 4. What "yes" does

`credit(C) = true` makes the job's `base(C)` (and the `q · ρ_v · base(C)` attester share)
**mintable in the coinbase of the crediting block**, subject to §4's caps. Concretely: the
crediting block's coinbase gains PALW outputs; a node validating that block recomputes the
gate from its own state and rejects a coinbase claiming credit the gate does not grant.

This is the ONLY consensus-visible effect. Per ADR-0027 §7's standing rule, no PALW outcome —
pass, fail, dispute, freeze, credit — touches block validity beyond its own coinbase claim,
fork choice, or any past block.

### 5. Reorg behavior

The gate follows the chain walk, so it inherits the store's accept/revert discipline: a
reverted chain block un-does its carriage inserts, and any credit decision made at that block
is un-made with it. Because the decision is a pure function of the state at that block, a
re-org that changes which transactions were accepted before `challenge_close` may legitimately
change the answer — and every node will change it identically. `Δ_bind` keeps the *anchor*
settled (ADR-0028 §2); `finality < W_challenge` (§3) keeps the *decision* inside the finality
horizon, so a credited job cannot be un-credited by a reorg deeper than finality.

### 6. Class freeze interaction

A frozen class credits nothing (`palw-class-activation-gate-status.md` §2): the gate reads
`class_active ∧ ¬class_frozen` from the registry before anything else, and a zero
`credited_ceiling` makes `credit(C) = 0` through the ceiling arithmetic itself with no
special case. The emergency rollback is therefore *inside* this gate, not bolted beside it.

## Consequences

* One design to implement, with its preconditions written down — the wiring cannot be
  "started early" without visibly violating item 2 or 3 above.
* The gate is a second consumer of the Stage-1 store and the existing chain walk; it adds no
  new state machine and no new consensus surface beyond the coinbase claim it authorizes.
* Because the gate is where the ceiling and the freeze are read, §12's rollback exercise and
  B15's leverage remedy both land here — which is why neither may be deferred past this ADR's
  implementation.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
