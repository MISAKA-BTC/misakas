# ADR-0107 — A class's share grows on work that reached Final, not on blocks that were accepted

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
