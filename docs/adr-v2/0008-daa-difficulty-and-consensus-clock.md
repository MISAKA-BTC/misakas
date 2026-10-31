# ADR-v2-0008 — DAA・difficulty・retarget・合意時計

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

difficultyの価格対象と合意clockの進行条件を同じ場所で確認する。

## 2. 統合した判断の読み方

### 2.1

retargetは価格付けされた対象を数え、heartbeatなど別価格の行をbitsの観測へ混ぜない。[旧ADR-0076](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) [旧ADR-0083](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md)

### 2.2

DAA scoreをanchorのclockとして使う条件を、単なるheaderの到着や時刻経過から分離する。[旧ADR-0138](../adr/0138-the-daa-score-is-the-anchors-clock.md)

### 2.3

合意clockをcursorとして扱い、heartbeatがslotを消費する場合やclockを進めないblockの扱いを明示する。変更は該当fence・networkに限定する。[旧ADR-0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)

## 3. 維持する境界・後続改定

旧120秒cadence等の数値は適用network・版に紐づく記録。新しい本番値として再採択しない。

## 4. 隣接する判断との分担

heartbeatのliveness責務は[ADR-v2-0009](0009-heartbeat-floor-and-liveness.md)、inference ticketとhash lotteryの採否は[ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) | L1–L215 | L18–L28 |
| [0083-the-difficulty-window-counts-only-rows-priced-by-bits.md](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) | L1–L115 | L12–L23 |
| [0138-the-daa-score-is-the-anchors-clock.md](../adr/0138-the-daa-score-is-the-anchors-clock.md) | L1–L142 | L12–L15 |
| [0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) | L1–L358 | L12–L18, L259–L262 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0076-the-attempt-lanes-seed-is-the-retargets-equilibrium"></a>

<details>
<summary>0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md — 原文 L1–L215</summary>

[原文を開く](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) · SHA-256: `8b1145cbe3164084a0825d93941b8b9cee67130128e5c325d87398dd1be880f6`

<!-- ADR_SOURCE_BEGIN:0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md -->
~~~~~~~~markdown
# ADR-0076: The attempt lane's seed is the retarget's own equilibrium

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

**Status:** ACCEPTED (2026-09-02), implemented for the Relaunch 5e re-genesis (every preset's
fingerprint moves). Written against a measurement, not a design review: testnet-11 Relaunch 5d ran
its first hour with the liveness floor at 249 blocks and both model tiers at **zero**, against a
share table that allots them 489‰ each.
**Builds on:** ADR-0038 Decision D (per-class difficulty domains; wall-clock never enters fork
choice), ADR-0045 Decision 1 (`pwu_per_inference` is the canonical job's counted step leaves),
ADR-0071 Decision 1 as amended (block interval is `calculate_difficulty_bits`' job; the per-class
retarget only redistributes share), ADR-0072 (the ticket is the execution — one canonical
inference is one draw).
**Makes reachable:** ADR-0075 (a stranger's model, certified on-chain, has to be able to produce
once it is seated).

> **§8 restated (2026-09-02)** — see the last section: the processor already pins a post-genesis entrant's `initial_target` to the base class's live target (M2-12), so the field is not free; what stays open is that the pinned price is the floor's, and the fork-choice consequence for an uncertified entrant is closed by ADR-0069 Decision 7.

## 1. What was measured

The three classes testnet-11 registers, with the share each actually holds after the genesis
grants dilute, and the counted work of one canonical inference:

| class | share | `pwu_per_inference` | 5d seed |
|---|---|---|---|
| PALW-BASE-0 `f1c5635c…` | 22‰ | 7,708 | `MAX/2` |
| PALW-QWEN36 `5bd9ae3d…` | 489‰ | 2,685,360 | `MAX/2` |
| PALW-QWEN25-A16 `71bbb755…` | 489‰ | 1,589,424 | `MAX/2` |

One seed for all three, because the model rows were built from `(slash_value, initial_target)`
read off the floor's own registration — one tuple standing for "the floor's economics", of which
the first half really is per-network by rule (one slash value for the network, because it sets
weight-per-collateral) and the second half is per-class by construction.

With one seed, a class's block rate is its DRAW rate. The floor completes a canonical inference in
~50 ms on the fleet's x86; Qwen3.6 takes ~9 s. So the floor drew ~180 tickets for every one the
hybrid drew, at identical odds, and took ~98.5 % of the blocks — which is what the live chain
reported: 249 floor blocks, zero from either tier, in the first hour.

Predicted from the same arithmetic before it was checked: at genesis `bits` (~1/20) the three
classes offer 10.15 hits/s of which the floor is 10.0 — a 30 blocks/min start burst that is 98.5 %
floor. The chain's own first minute: 27 blocks, all floor.

## 2. Why no retarget repairs it

Both correction paths decline, each for a correct local reason:

* `retarget_over_span_v1` expects `share_c · total` and divides by observed. A class that produced
  **nothing** is skipped — silence is not evidence of trying, and easing on silence would let a
  registrant buy cadence with patience.
* `converge_idle_target_v1` (ADR-0071 Decision 1 as amended) moves an idle class toward
  `floor_price`, the hardest target any class that actually produced is paying. Seeded AT the
  floor's price, an idle tier is already there: `current_target >= floor_price` returns unchanged.

A shared seed is therefore not a slow start. It is a permanent one, and the module's own doc had
already named the shape — "a class whose target is too hard to win even one block per epoch stays
too hard forever".

## 3. Decision 1 — the seed is `share · pwu`, against one pinned scale

`retarget_over_span_v1` converges to `observed_c / total = share_c`. A class produces
`r_c · P_bits · T_c / 2^128` blocks per second, so the fixed point is `T_c ∝ share_c / r_c`.

`r_c` — inferences per second — is the one term consensus may not measure: a wall-clock second is
a fact about a host, and ADR-0038 Decision D refuses to let one reach fork choice. What consensus
holds instead is `pwu_per_inference`, the normative step-leaf count of the class's canonical job,
frozen at registration. A producer working at any fixed rate completes inferences in inverse
proportion to it, so `r_c ∝ 1/w_c` is the only hardware-free reading of the same quantity:

```text
    T_c = MAX · (share_c · w_c) / PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1
```

Each class is priced from **its own two numbers** against one pinned scale — never against the
other classes — so a table's seeds do not move when a class joins or leaves it, a network that
registers only the floor prices that floor exactly as a three-class network would, and a stranger
registering post-genesis is priced by the rule that priced the incumbents.

The counted-work proxy is not exact: the measured wall-clock ratio between the floor and the
hybrid is 180×, the counted-work ratio is 348×. The seed is therefore within a factor of ~1.9 of
the equilibrium, which one closed span of the retarget removes — as against the nine days of
×4-per-epoch crawling the shared seed required, and the permanent stall it actually produced.

## 4. Decision 2 — the scale is spent on headroom

Only ratios of class targets carry meaning: the aggregate interval is `bits`' job and the two
retargets are wired so they cannot fight over one cadence. The absolute scale is therefore free,
and free means it should be spent on the thing scaling can still get wrong.

A class seeded at `u128::MAX` has no headroom: no class dearer per unit of share can ever be
seeded above it, and no retarget can lift it, because a target may not exceed the ticket space.
Anchoring the table's dearest class AT the ceiling would make a network's genesis the permanent
upper bound on every model a stranger could bring later — which is the door ADR-0075 opens.

`PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1 = 2^31`, and what testnet-11 gets for it:

| class | `share · pwu` | 5e seed |
|---|---|---|
| PALW-QWEN36 | 1,313,141,040 | `MAX / 1.63` |
| PALW-QWEN25-A16 | 777,228,336 | `MAX / 2.76` |
| PALW-BASE-0 | 169,576 | `MAX / 12,663` |

The dearest class draws about every other ticket, the ceiling stays 1.63× away, and the floor sits
7,744× below the hybrid — the ratio their shares and their counted work require.

**Mainnet is the case this decision is really for.** Mainnet mints floor-only, and every model
class it will hold arrives afterwards by registration. A floor left at `MAX/2` leaves a factor of
two above it and no model tier could ever be seeded into room that is not there. Seeded, a
floor-only network's floor sits at `MAX/278` and the room above it is the 278× a tier needs.

## 5. Decision 3 — the seed is written at genesis assembly, against the EFFECTIVE share table

A registration's declared `share_permille` is not the share it holds: a grant funds an entrant by
scaling every incumbent, so the floor declares 1000‰ and holds 22‰, and the hybrid declares 957‰
to hold 489‰ after the dense tier dilutes it. Pricing against the declared figures is a 23× error
on the floor alone.

So the genesis assembly FOLDS its own registration list through `apply_palw_transition_v2` — the
same call `assemble_palw_rc_identity_v2` uses to prove a list applies — and seeds each row from
the table that fold produces. A second, hand-rolled spelling of the dilution would be a second
place for it to drift.

The seed lands in the `ClassRegistered` object, so it is inside `consensus_params_id`: a node
built before this change and a node built after it announce different fingerprints and refuse each
other at the handshake, rather than agreeing on a name and disagreeing about which class may
produce.

## 5b. Decision 4 — a class being SEATED is a class being priced

ADR-0075 seats a weightless class by an object: it registers holding no share, and a
`ClassLaneCertified` on the attempt lane grants it `min_grantable_share_permille` once the chain
has graded the drill that covers it. Its registration was priced for the cadence it held then —
none — so granting the share without re-seeding would leave the one number that decides how often
it may produce entirely unrelated to the share just granted, in either direction. A stranger who
declared `MAX/2` would sit thousands of times easier than the floor it is joining; one who declared
conservatively would sit locked out, with no retarget able to reach it, because an idle class only
converges toward a price a PRODUCING class pays and it produces nothing.

So the grant writes the price with the seat: `attempt_target_seed_v1(granted share, the class's own
`pwu_per_inference`)`, from the table the transition has just written. This is the clause that makes
ADR-0075's permissionless path end in a class that can actually produce, rather than one that holds
a share and no way to use it.

## 6. What this does NOT change

**Not the weight split.** Weight per second is `w_c · r_c · P_bits`, in which `T_c` cancels:
`pwu(B) = expected_attempts(T_c) · w_c` rises exactly as fast as the block count falls. A seed
moves the BLOCK COUNT split — cadence, which is what the share table allots — and leaves fork
choice where it was. `pwu` magnitude still may not be read as a cross-class price.

**Not collateral.** A claim reserves `palw_exposure_pwu_v1 = pwu_per_inference`, with no target in
it, and the genesis bind-window gate reads the same target-free quantity. `GENESIS_CLASS_TARGET` in
`palw_fp_devnet_v3` survives as what it always arithmetically was for that call — a 2× margin on
derived collateral — and is re-documented rather than moved, because moving it would re-size every
shipped registry to chase a factor that is not in the reservation it funds.

**Not the receipt lane.** The receipt lane seeds at `PALW_RECEIPT_TARGET_SEED_V1` and has since
the 5e-prep commit that decoupled the two: a receipt draw is one quantum of real demand, whose
supply has nothing to do with how fast a class runs its canonical job.

## 7. Consequences

* Every V2 preset's fingerprint moves. Relaunch 5e is a wipe, like every relaunch on this chain.
* `PALW_RC_FLOOR_DERIVED_PWU` moves 15,416 → 97,606,404 (12,664 expected executions × 7,708). A
  floor block weighing six thousand times more is the identity in §6 working, not a regression.
* A devnet floor is seeded at `MAX/278` instead of `MAX/2`: ~279 draws per block where a drill
  used to need two. At the floor's ~50 ms inference that is ~14 s per block on a network whose
  `bits` is otherwise trivial.
* The first-block cadence lands near target instead of bursting: at genesis `bits` the three
  classes now offer ~0.137 hits/s against 10.15 before, i.e. ~0.41 blocks/min against the 0.5/min
  the cadence set asks for. The 500-block start burst that ADR-0071's record measured against a
  264-block span window does not happen.

## 8. What is deliberately left open

A post-genesis registrant still DECLARES its own `initial_target`, and the transition still writes
it verbatim at registration. Nothing refuses a value the rule would not have produced — a free
field, which this codebase has twice found to be a free draw.

Decision 4 removes the reachable half of that: a class registering weightless is re-priced the
moment it is granted a share, so a declared value buys nothing on the ADR-0075 path. What remains
is a registrant that takes a share at registration and declares its own seed, and the closure is a
one-line equality at the transition — the derived seed is computable by the registrant (its share
is `min_grantable_share_permille`, a ruleset constant, and its `pwu` is counted from the carriage
it already carries), so there is no fixed point to solve. It is left out of this change only
because it invalidates every fixture in the tree that constructs a `ClassRegistered` with a chosen
target, and Relaunch 5e is a deployment rather than a refactor.

## Amendment (2026-09-02) — §8 restated against the shipped rule

§8 says a post-genesis registrant "still declares its own `initial_target` … and nothing refuses a
value the rule would not produce". The processor refuses a value that is not the base class's live
target (`processor.rs`, audit M2-12: "the entrant's difficulty is the chain's, not the
registrant's"), so the field is pinned — to the floor's price, not to this ADR's seed. What remains
open is exactly that: an entrant sits at the floor's price until `ClassLaneCertified` re-seeds it
(Decision 4), and until then its blocks claim the floor's expected draws against its own
`pwu_per_inference`. For a certified entrant that is honest work (it needs the draws). For an
uncertified one it is the fork-choice hole ADR-0069 Decision 7 closes (its blocks weigh nothing).
The equality closure at registration (`initial_target == attempt_target_seed_v1(share_granted,
pwu_per_inference)`) stays deferred for the fixture reason given; with share 0 it would yield the
hardest target, which is also acceptable — a weightless class that cannot produce attempt blocks
loses nothing the network needs.
~~~~~~~~
<!-- ADR_SOURCE_END:0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md -->

</details>

<a id="source-0083-the-difficulty-window-counts-only-rows-priced-by-bits"></a>

<details>
<summary>0083-the-difficulty-window-counts-only-rows-priced-by-bits.md — 原文 L1–L115</summary>

[原文を開く](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) · SHA-256: `dc07f9058da676ee8b98a6d760be9fb2c45f17a0f131205524aad6b41b075dfa`

<!-- ADR_SOURCE_BEGIN:0083-the-difficulty-window-counts-only-rows-priced-by-bits.md -->
~~~~~~~~markdown
# ADR-0083: The difficulty window counts only rows priced by `bits` — heartbeat emitters are not work

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** PROPOSED (2026-09-04), implemented on `palw-daa-bits-priced-rows` with the fence
dormant on every preset; **not armed anywhere**. How it reaches testnet-11 — a scheduled
`ForkActivation` on the live chain (fingerprint moves, genesis does not; a flag-day restart of the
seats, no wipe) or folded into a Relaunch 5g re-mint — is the operator's decision, recorded in §5.
**Builds on:** ADR-0060 Decision 1 (the heartbeat lane), ADR-0066 Decisions 1–3 (the lane's price
leaves `header.bits`; its slot rule; attempt blue work is a constant), ADR-0071 (`bits` keeps the
block-interval control that target-freezing removed), ADR-0072 (one inference, one draw: both the
class ticket and the Layer-0 digest are functions of the execution commitment).
**Amends:** ADR-0066 Decision 1, one sentence — *"Heartbeat headers carry the global expected bits
like every other lane, so they enter the difficulty window as ordinary rows and F1 and F3b both
disappear."* They carry the bits and they enter the window; they must not be **counted** as rows the
retarget slows down. F1 did not disappear; it moved from the lane's price to the lane's count.

## 1. What was measured (testnet-11 Relaunch 5f, genesis `ad30b5cb…`, 2026-09-04 02:00–04:10Z)

Read from node0 over wRPC (`getBlocks` from genesis, 829 blocks, sink DAA 826), reproduced
independently by a second session against its own node's copy of the chain (841 blocks):

| quantity | value |
|---|---|
| block time | 120 s — `DnsParams … with_two_minute_cadence()`, not the preset comment's 10 s |
| selected chain | genesis + 255 heartbeats (algo 8), interval 122 s at p10 = p50 = p90 |
| non-chain rows | 569 heartbeats, 3 attempt blocks (algo 6) at DAA 40 / 80 / 226 |
| rows per slot | 3.1–3.4: the chain heartbeat plus 2–3 sibling heartbeats (`PALW_HEARTBEAT_MAX_PER_MERGESET` = 4) |
| heartbeat emitters | 5 (two per host on ibm and .113, one on seat2) |
| `bits` | `0x207fffff` (p = 0.5) to DAA 150, then halving every ~50 slots: `0x1f65bd04` (p = 1.5e-3, difficulty 320.7) at DAA 826 |
| floor class | target 7.9e-5 · u128::MAX = 22 · 7,708 / 2³¹ to the digit; facts available, budget 22, produced 0 |
| floor chance per draw | 7.9e-5 × 1.5e-3 ≈ 1.2e-7 — ~18 days per block per node at 5 draws/s, halving every two hours |

Emulating `SampledDifficultyManager::calculate_difficulty_bits` over the last 264 rows in mergeset
order with `target_time_per_block` = 120 s predicts the next chain block's bits within 2–3 % at four
consecutive chain blocks (DAA 815/819/821/823); with 10 s the same emulation predicts easing.

## 2. The mechanism

`new_target = average_target · measured_span / (target_time_per_block · rows)`. The window's span is
real elapsed time; its row count is not real work. One heartbeat emitter per slot is exactly on
pace. Every additional emitter adds a row per slot **without adding time**, and by ADR-0066
Decision 1 its rows are priced by the constant 2²⁴, so the tighter `bits` costs them nothing. The
retarget therefore reads "three times too fast", tightens `bits` ×3 per window, and only the lanes
priced by `bits` — the floor and every model class, whose Layer-0 digest must land under
`target(bits)` — pay. The heartbeat miner's own doc has the blind spot in one line: *"the more that
run, the harder the lane's own retarget"* — after Decision 1 the lane has no retarget of its own;
the extra emitters harden the **global** `bits` instead.

## 3. Why the chain does not recover

`heartbeat_interval_ms` is one cadence after a heartbeat parent and backs off only after a
**bonded** parent: the lane runs at full cadence *because* the bonded lanes are silent, and they are
silent *because* `bits` is tight. Cutting the emitters to one stops the tightening (ratio 122/120)
and recovers 1.7 % per window — ~340 windows to undo ×320. A pause makes a gap, and a gap eases
linearly, once. Nothing an operator can do with the lane restores `bits`; the rule has to.

## 4. Decision 1 — count only bits-priced rows; an empty count answers MAX

Past a top-level fence `Params::palw_difficulty_priced_rows: Option<ForkActivation>`
(`None` on every shipped preset; Some-only in `consensus_params_id`, normalised `never() → None`,
visited by `for_each_fence`, so a scheduled height still peers — the D4 discipline):

1. `expected_duration = target_time_per_block · sample_rate · (rows whose lane satisfies
   `algo_id_is_priced_by_bits`)`. The predicate is the exact complement of the constant-target arm
   in `kaspa_pow::State::new` — today, everything but algo 8.
2. Heartbeat rows still enter the average (they carry the global bits, as ADR-0066 says) and still
   bound the span (they are elapsed time). Only the count changes.
3. A window with **no** priced row answers `max_difficulty_target` — ADR-0066's own sentence, *"a
   V2 network runs at MAX because the class lottery, not the hash target, is its throttle"* — not
   the selected parent's bits, which would pin a chain whose attempt lanes died at the bits that
   killed them. This clause is what makes arming the fence on the live chain a recovery: its window
   holds no priced row, so its first block carries MAX.
   The retarget stays multiplicative in the window's average: a window that still holds priced
   rows under tight bits eases by the priced-rate ratio per window (unit test: ×2.95 for attempts
   every third slot), and only a window with no priced row jumps. Testnet-11's window at the height
   of arming holds none — its last attempt block is at DAA 226 — which is why arming is a recovery
   there and not merely a slope change.
4. Before the fence, and on any window where every row is priced (a hash network; a V2 chain with
   the heartbeat silent), the arithmetic is byte-for-byte the legacy retarget — same min-row
   removal, same tie-break — so old and new builds agree on every header until the height.

What the rule keeps: the interval control ADR-0071 kept. Priced rows denser than the cadence still
tighten `bits` (unit test `heartbeat_rows_no_longer_tighten_the_bits_past_the_fence`, fourth case).
What it removes: the number of heartbeat emitters from the meter.

`retarget_bits_from_rows` is public and takes rows as `getBlock` reports them, so a node's own
history can be replayed through both rules — the check a second reader can run against this patch
and its author cannot.

## 5. How it ships — the operator's decision

| path | fingerprint | genesis | fleet action | when the floor can win again |
|---|---|---|---|---|
| (a) scheduled fence on the live chain, height H | moves at H (Some-only write) | unchanged | build + restart the six seats before H; a peer on the old build peers until H and forks at H | the first block at H (its window has no priced row → MAX) |
| (b) fold into Relaunch 5g with `Some(ForkActivation::always())` on testnet-11 | new | new | wipe + re-mint per the 5f card | block 1 |

Either way the announcement's known-state line stands until then. The stop-the-bleed move — one
heartbeat emitter — is worth nothing under (b), and under (a) only spares the fence's first window
from inheriting bits another 2¹⁰ tighter; it is not a fix and this ADR does not depend on it.

## 6. What this does not decide

Whether the heartbeat lane should back off when the *sibling* count is high (the emitter count is
still visible in mergeset width, and ADR-0066's finding 3a — siblings share one admissible
timestamp — is still open); and whether `min_difficulty_window_size` should count priced rows (it
counts all rows here, deliberately: a fresh chain's first 150 blocks stay at genesis bits, which on
a V2 network is MAX already).
~~~~~~~~
<!-- ADR_SOURCE_END:0083-the-difficulty-window-counts-only-rows-priced-by-bits.md -->

</details>

<a id="source-0138-the-daa-score-is-the-anchors-clock"></a>

<details>
<summary>0138-the-daa-score-is-the-anchors-clock.md — 原文 L1–L142</summary>

[原文を開く](../adr/0138-the-daa-score-is-the-anchors-clock.md) · SHA-256: `9e67e21cb0b02ba54472c3ff8c15578f0cd1fc70dcf52080d3484701f7d6718a`

<!-- ADR_SOURCE_BEGIN:0138-the-daa-score-is-the-anchors-clock.md -->
~~~~~~~~markdown
# ADR-0138 — The DAA score is the anchor's clock: a block advances it iff `bits` priced it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built and fenced**
(`Params::palw_anchor_clock`, `Some(6,001)` on testnet-11 — the single lottery's own day — `None`
elsewhere). Found by the operator's DAA-clock audit (`docs/palw-daa-clock-audit-2026-09-18.md`),
which held the rollout of main and testnet-11 until this was closed.

## 1. The defect

ADR-0132 S takes the attempt lane out of `bits`: from 6,001 an attempt header's Layer-0 digest is
admitted unconditionally and the class ticket `CCU/W` is the whole lottery. But the DAA score kept
counting attempt blocks (`calc_daa_score_and_mergeset_non_daa_blocks` excluded only the round lane),
and ADR-0137's work target states its expectation in DAA — `expected = epoch_length ×
fp_attempt_share_permille / 1000 = 900` model blocks per 1,000-DAA epoch. Nine of every ten DAA in
an epoch would then be model blocks, the anchor's 120 seconds would be shared over ten DAA, and one
DAA would be **12 seconds** in steady state (3.3 s in a burst: no in-epoch cap, `p → 1` when
`CCU ≥ W`). Every window the chain measures in DAA — `t_leak_daa` 5,040 (sized as seven days),
`window_receipt` 600 (twenty hours), `window_challenge` 120 (four hours), `withdrawal_delay` 7,500
(ten days) — would run ten times fast. The receipt and heartbeat lanes, unpriced since ADR-0083, had
been ticking the clock all along; the attempt lane at compute speed is what made it a blocker.

## 2. Decision

**A block advances the DAA score exactly when `bits` priced it.** Past `palw_anchor_clock`,
`internal_calc_daa_score` subtracts from the mergeset count every block whose lane is not priced at
its own DAA score — the same three-generation predicate the difficulty window already reads
(`algo_id_is_priced_by_bits` → `_v2` past `palw_receipt_rows_unpriced` → `_v3` past the single
lottery). Round blocks were already out (ADR-0125); attempt, receipt and heartbeat blocks join them.
The rule lives in one place (`SampledDifficultyManager::lane_advances_daa_at`) and both DAA paths —
the template's `calc_daa_score_and_mergeset_non_daa_blocks` and validation's `block_daa_window` →
`calc_daa_score` — end in the same arithmetic, so a header that claims otherwise is refused by
`check_difficulty_and_daa_score`.

**Not through `mergeset_non_daa`.** That set means "outside the DAA window": the coinbase pays no
block in it (`coinbase.rs:233`) and the PALW fold skips it (`palw_v2_merged_works(.., &merged_non_daa, ..)`).
An attempt block must keep its subsidy — the escrow is a carve of it — and its claim must keep
folding. The exemption is arithmetic on the score only: exempt blocks stay merged, blue where their
lane is blue, paid, folded. The test `adr0138_past_the_anchor_clock_a_heartbeat_block_ticks_no_daa`
pins all three.

**One decision, one setter.** `Params::set_palw_single_lottery` arms or clears the lottery and the
clock together; `validate_palw_v2` refuses the pair at two heights. The clock may stand alone (it has
receipts and heartbeats to exempt), the lottery may not run ahead of it on a shipped preset — the
flag-day table pins both at 6,001.

## 3. What follows

* The DAA score is the anchor lane's clock, retargeted to 120 s. Every DAA-denominated window in
  the audit's §3 keeps its nominal column, whatever the model lane does.
* The work target's expectation becomes what the number always meant: 900 model blocks for every
  1,000 anchors — nine model blocks for ten anchors, one every ~133 s network-wide — instead of a
  runaway that the model lane's own blocks feed.
* The DAA score is non-decreasing along a chain, never strictly increasing (`NonMonotonicContext`
  allows equality), and many attempt blocks may share one score: a clock, not an index.
* Blue-score depths (finality, merge, pruning) are still counted in blue blocks, and attempt blocks
  stay blue with a fixed 2^20 of work. Reaching finality depth sooner in wall-clock makes the chain
  harder to reorg, not easier; the release report measures the blue rate beside the DAA rate so
  the number is seen, not assumed.
* The devnet flag `--palw-anchor-clock-devnet` (must equal `--palw-single-lottery-devnet`) and the
  drill scripts' `ANCHOR_CLOCK_AT` arm it on a private chain; the drill's release report states
  anchor DAA/s, attempt DAA/s and receipt DAA/s from the logs.

## 3b. What the fix itself broke, and closed (the 2026-09-18 re-audit)

Two holes of its own, both found by re-auditing the fix rather than the bundle:

* **The freeze.** The first cut exempted every lane `bits` does not price, the heartbeat among them
  — and the heartbeat is the chain's liveness tick (ADR-0066 took it out of `bits` so a stock of
  certified quanta could not tighten the attempt lane's target, not because the chain does not need
  it). A chain whose hash lane stopped would have kept producing while its DAA score froze, taking
  every fence, deadline, retention and leak window with it. This was not a thought experiment: the
  registry drill froze at virtual DAA 20 with 183 blocks accepted, on a devnet whose only producers
  are PALW lanes.
* **The heartbeat as a stand-in, not a second clock.** Counting the heartbeat unconditionally is the
  opposite error: on a chain whose hash lane runs, a beat beside every anchor tick would pace the
  clock twice as fast, which is the very error this ADR exists to remove. The rule is therefore per
  MERGESET, not per block: `palw_lane_advances_daa_v1` answers `false` for the heartbeat lane, and
  `daa_exempt_count` returns one fewer exemption only when the mergeset carries no priced block and
  at least one heartbeat. Where a priced lane runs the heartbeat adds nothing; where it stops,
  exactly one heartbeat a mergeset keeps the clock alive.
  `the_clock_is_the_anchor_and_the_heartbeat_and_nothing_else_past_the_fence` pins both directions,
  and the drill's fault phase stops the hash miner on a running chain and requires the DAA to keep
  moving.

## 3c. Why this is NOT armed at 6,001 — the premise is false on testnet-11

This ADR assumes an anchor lane paces the chain and the model lanes ride alongside it. **On
testnet-11 there is no anchor lane at all**, and the fence would therefore leave the DAA with no
source but the heartbeat.

Measured from the chain on 2026-09-18 with `scripts/drills/misaka-t11-lane-walk.py`: of the last 60
selected-chain blocks, 60 are algo 6 and none is algo 3; of the last 300, 261 are algo 6, 39 are
algo 8 and none is algo 3. There cannot be one: the template declares `bundle.algorithm_id`, which
`PalwRulesetV2::validate` requires to be the committed attempt id on every ConsensusV2 network, so a
Layer-0 hash miner is handed an algo-6 template and refuses it.

Past the fence `palw_lane_advances_daa_v1` is true only for a `bits`-priced lane, and on testnet-11
that set is empty. The clock would fall from 275 s per DAA to the heartbeat's nominal 3,600 s — and
to nothing across a stretch like the most recent 4.58 hours, which carried no beat — resting entirely
on one unbonded producer on one host. The arithmetic and its consequences are in §2 of
[the clock audit](../audit/palw/palw-daa-clock-audit-2026-09-18.md).

**The params in this tree still arm it.** `Params::set_palw_single_lottery` arms the lottery and the
clock together at 6,001, and `validate_palw_v2` refuses the pair at two heights, so the clock cannot
simply be dropped from the preset — disarming it disarms ADR-0132 S with it. That coupling is why
this is a decision and not an edit: **no build carrying this preset may be rolled out to testnet-11**
until the operator picks one of the two paths below and the flag day is moved above the tip.

The decision this ADR still needs is where a PALW-only network's clock comes from. Two candidates:
give such a network a priced lane, which is a params change because ConsensusV2 templates always
name the attempt id; or make the heartbeat the designed clock by setting `HEARTBEAT_NOMINAL_INTERVAL_MS`
to the target block time behind its own fence, which is one constant but puts the chain's clock on an
unbonded lane. Neither is decided here.
* **The DNS leak's units.** ADR-0128 Decision 3 walks the evidence window in BLUE score and adds
  `t_leak_daa` into that blue total, which was the same thing while every lane ticked both clocks.
  With the DAA clock slower than the blue clock, a blue-bounded walk reaches back fewer DAA than the
  leak is decided over (5,244 blue ≈ 2,760 DAA at the work target's steady state), the lower-edge
  fallback under-states every silence, and **no bond would ever have been leaked**. The window now
  covers both spans — `DnsBftRulesV1::in_evidence_window`, the union of the blue span and the DAA
  span, whose start is the deeper of the two — and the runtime walk ends only where both are behind
  it. Where the clocks agree the window is the one ADR-0128 shipped, block for block. Pinned by
  `a_silent_bond_is_leaked_whether_blue_runs_with_the_daa_clock_or_twice_as_fast` (ratios 1, 2, 4).

**The blue axis this fence does NOT close**, and the operator accepted with the numbers in
`docs/palw-daa-clock-audit-2026-09-18.md` §9: finality (360 blue), merge depth (30), pruning (~900)
and the DNS attestation epoch (100) are counted in blue score, which the attempt lane still paces.
None is a split risk — blue score is a deterministic function of the DAG — and the effects are
liveness and availability, with finality moving in the safe direction.

## 4. Fingerprint

`c472a17b…` → `48d1e31f…` on testnet-11 (the fence is hashed Some-only). The schedule is unchanged:
6,001 was already a scheduled height.
~~~~~~~~
<!-- ADR_SOURCE_END:0138-the-daa-score-is-the-anchors-clock.md -->

</details>

<a id="source-0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot"></a>

<details>
<summary>0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md — 原文 L1–L358</summary>

[原文を開く](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) · SHA-256: `7bfd0c0875196aad7a887058778f12eb46307f3c8a74d09ddf700cf870eda4eb`

<!-- ADR_SOURCE_BEGIN:0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md -->
~~~~~~~~markdown
# ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the clock may not postpone it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Consensus
change, built and drilled, NOT ARMED on any preset.** The four checks of §6a all pass; arming is the
operator's call and wants a reorg drill and a target-cadence drill first (§8).
**Amended 2026-09-24 (§9): the clock floor.** §6a check 4 and §6 property 6 did not hold once the
carried cursor was deleted — the step's timestamp was unconstrained, so the clock could run fast and a
future-stamped sibling could delay it. `Params::palw_clock_floor` (testnet-12 only) makes them true.
It replaces the heartbeat lane's admissibility rule. It held the 7,101 rollout.

**Builds on:** ADR-0060 (the liveness doctrine), ADR-0064 (silence is not checkable), ADR-0066
Decision 2 (the one-block-deep slot rule, which this supersedes), ADR-0105 (the miner's yield),
ADR-0138 (the anchor clock), ADR-0140 (the lane's five claims — this is C5).

## 1. The defect

Past `palw_anchor_clock` the heartbeat is the only lane that can advance the DAA score on a network
with no `bits`-priced producer, which is every ConsensusV2 network. Its admissibility is

```
heartbeat.timestamp >= selected_parent.timestamp + interval
```

and the selected parent is replaced by every new chain block. **An attempt block advances no clock
and yet moves the next opportunity to advance it.** So a chain producing faster than the interval
suppresses the heartbeat lane entirely, and the score stops while blocks keep coming.

Measured on the registry drill, 2026-09-18, in testnet-11's lane composition:

| chain | selected-chain interval | 120-second slot opens? | outcome |
|---|---|---|---|
| the drill, eight producers | **21 s** (DAA 0→20 in 419 s) | never | **DAA frozen at 20, zero heartbeats minted** |
| testnet-11 today | 302 s | for ~182 s of each gap | survives |
| testnet-11 under this bundle's own projection | **3.3 s** (clock audit §3, `p → 1` when `CCU ≥ W`) | never | **would freeze** |

The last row is what makes this a release blocker rather than a devnet artefact. testnet-11 survives
at today's cadence, and this bundle exists to raise that cadence.

**And it is the ADR-0060 failure exactly.** The doctrine is that time is permissionless and weight is
bonded, because four separate incidents had one root: the clock was hostage to a stuck actor. Under
the rule above the clock is hostage to a *busy* one — any producer can stop it by producing.

## 2. The invariant

> **A block that does not advance the consensus clock MUST NOT postpone the next opportunity to
> advance the consensus clock.**

Everything below is a way of holding that sentence true, and the property tests in §6 are it,
restated as code.

Two concerns were travelling through one value, `selected_parent.timestamp`, and separating them is
the whole fix:

```
chain attachment   → the selected parent          (unchanged; a beat still builds on the tip)
clock eligibility  → the consensus clock cursor   (new; only a beat moves it)
```

## 3. The three candidates

**A. `selected_parent.timestamp + interval`** — today's rule. Rejected: §1 is a liveness failure,
not a tuning problem.

**B. `last_heartbeat.timestamp + interval`** — starvation goes away and the change is small. Rejected
as the destination, for two reasons. It puts the beat's own timestamp on the clock, so a producer
picks where the next opportunity falls, bounded only by the future-drift rule. And "the last
heartbeat" is ancestor evidence: answering it needs a walk, which is what ADR-0066 Decision 2
abandoned when a pruned node and an archival node computed different verdicts for one header.

**C. A consensus clock cursor.** ADOPTED. The chain carries the next slot boundary, per block, as
derived data in the same sense as the DAA score beside it. A heartbeat consumes a slot and advances
the cursor; every other lane leaves it untouched. No walk at read time, because the answer is stored;
no timestamp on the clock, because the cursor advances in whole slots.

It was first built in the rooted PALW state and moved, which is worth recording. The state is folded
in the virtual processor, so a cursor living there can gate a heartbeat's CHAIN validity but not its
contribution to the DAA score, which header processing decides. Gating only chain validity leaves the
rate bounded by the chain-block rate rather than by time — the very thing ADR-0138 exists to bound.
The move is also what opened §6a check 3: rooted state is carried and committed, and derived data is
neither.

C is also the only one of the three in which the invariant of §2 is *structural* rather than
maintained. A and B both re-derive the deadline from a block; C's cursor is only writable by the one
transition that advances the clock.

## 4. The rule

```rust
pub struct PalwClockCursorV1 {
    /// The earliest timestamp the next heartbeat may carry.
    pub next_slot_ms: u64,
    /// Slots consumed since the cursor opened. Telemetry and a reorg cross-check; no rule reads it.
    pub slots_consumed: u64,
}
```

**Admissibility.** `header.timestamp >= cursor.next_slot_ms`. Nothing else. The selected parent's
lane and timestamp do not appear.

**Transition.** Only an admitted heartbeat writes the cursor:

```
slots_skipped = (header.timestamp − cursor.next_slot_ms) / interval        // whole slots, floor
next_slot_ms  = cursor.next_slot_ms + (1 + slots_skipped) × interval
slots_consumed += 1
```

Every other lane: **unchanged**, which is §2 in one word.

**Why the skip term.** Without it, an outage leaves a backlog of owed slots that a returning miner
consumes back to back, running the clock arbitrarily fast just when the windows it feeds are most
stretched. With it, the cursor lands on the first slot boundary strictly after the beat's timestamp:
**missed slots are lost, not banked.** A beat that is late costs the chain the DAA it did not tick,
which is the honest accounting — the DAA is a counter of elapsed slots, not a debt.

**Why whole slots.** The beat's timestamp selects a slot, never a point inside one. A producer can
therefore move the next opportunity only by whole intervals and only within the future-drift bound
the timestamp rules already impose, and two beats with different timestamps in the same slot leave
the cursor in the same place.

**Opening the cursor.** `None` until the fence. At the first heartbeat past `palw_clock_cursor`, it
opens at that block's own timestamp — so the rule starts where the chain is, not at a boundary
derived from history no node need still hold.

## 5. One definition, four callers

ADR-0066 Decision 2 already required construction and validation to read one answer, and ADR-0138
§3c broke it anyway: the validator moved to a new interval and the template builder kept the old one,
so a node stamped a slot its own validator would not grant. The requirement was written down and
nothing enforced it.

So the rule is **one function**, and everything asks it:

```rust
pub fn palw_clock_slot_admits_v1(cursor: &PalwClockCursorV1, proposed_ms: u64) -> Result<(), ClockSlotTooEarly>
```

and the decision that grants the DAA exemption and the decision that moves the cursor are one call,
`palw_clock_step_v1`: the DAA window carries both, the header processor stages what it commits, and
`daa_exempt_count` returns the other half. A workspace guard fails the build if the superseded entry
points are called outside their module (`only_this_module_may_ask_the_pre_fence_slot_rule`).

**And the slot rule itself retires behind this fence.** It existed to bound the lane's width; past
the cursor the clock is bounded instead, by something the economic lane cannot move. What was left
for the slot rule to do was only harm. Width stays bounded where it always was — a fixed hash price
and at most four beats a mergeset — so a beat may be minted whenever its producer can pay for it, and
earns a DAA only where a slot is open. The template retires the same rule on the same fence, so the
two sides cannot disagree about a wait that no longer exists.

## 6. What must be proved, as properties and not as examples

The bug that produced this ADR is invisible to example tests: every example passed. These are
properties over generated sequences.

1. **For any sequence of blocks that do not advance the clock, at any spacing — 1 s, 3.3 s, 20 s,
   119 s — the cursor does not move.** This is §2 and it is the one that failed.
2. **Past the cursor, a heartbeat template is admissible.** No liveness hole between "the slot opened"
   and "a beat may be built".
3. **Validation and the template builder agree** on every input. Same function, asserted anyway.
4. **Only a heartbeat advances the cursor**, over every lane and every fence combination.
5. **A reorg drags nothing**: a block's cursor is a function of its own selected parent's row and
   its own mergeset, so a competing branch cannot reach a cursor already written.
6. **A producer cannot lock the clock with a future timestamp**: the cursor moves by whole slots and
   the drift rule bounds how many.
7. **One beat normalises a long outage**: after any silence, a single heartbeat leaves the cursor at
   the first boundary after it, with no backlog to consume.

## 6a. The operator's four checks, answered

Asked after the implementation passed, and worth recording as asked, because the fourth answer is a
defect and not a property.

**1. Several heartbeats in one mergeset — who consumed the slot?** The NEWEST by timestamp, and
`max` is order-independent, so nodes walking an unordered mergeset in different orders land on the
same cursor. Two beats sharing the newest timestamp give the same answer, which is why the rule is
stated over a timestamp and not over a block identity that would need a tie-break. Newest and not
oldest deliberately: a mergeset carrying a stale beat beside a current one must still grant, or a
chain could be starved by merging an old beat. Pinned over 500 shuffles.

**2. Does a reorg drag the old fork's value?** No, structurally. A block's cursor is a function of
its own selected parent's row and its own mergeset; there is no shared mutable cursor to drag. The
pipeline test mints a sibling of the last beat and asserts every cursor already written is
unchanged, and that the sibling reads its own parent's row.

**3. Does an IBD or pruned node rebuild the same values? YES — after the fix this check forced.**

It did not, at first. The cursor was stored per block, and an absent row read as "no cursor yet",
which granted the exemption unconditionally; an archival node holding the row did not grant. The two
computed different DAA scores for one block, so the node without the row would reject a header the
network accepted. **That is ADR-0066 finding 4 in a new place**, and this check is what found it.

The fix removed the store. What the cursor held was never private: the DAA score is in every header,
so the block that took the score to its current value is **the one at the selected parent's score
with the lowest blue score**, and the DAA window already in hand holds it. Every node that can
process the block at all has that window, so a pruned node, a node that joined by pruning proof and
an archival node compute the same answer. No carriage to add, no commitment to verify, no column
family to grow.

Selected by blue score and not by timestamp, deliberately: timestamps are not monotonic across a
DAG, so a minimum over them could be pulled backwards by a merged block carrying an old but
admissible timestamp, and an early reference opens a slot early. Blue score is monotonic along the
chain, so the minimum picks the block that actually advanced the score and nothing can impersonate
it. `the_reference_is_the_block_that_advanced_the_score_and_nothing_else_moves_it` asserts both,
including that a timestamp minimum would have taken the bait.

`validate_palw_v2` refuses the fence on a network whose difficulty window is sampled, since a
sampled window can omit the block that advanced the score. The rule names its own precondition
rather than assuming it.

**One behaviour follows and is worth stating.** A beat cannot cause an advance and then consume the
slot that advance opened. Where a beat's own score came from a priced block it merged, that beat IS
the reference, so a block merging only it does not tick again. Before the cursor it did,
unconditionally, which is the clock running free.

**4. Can a producer send the clock into the future?** *(Superseded by §9.2: without the carried
cursor this answer did not hold — a step's timestamp was unconstrained, so the clock could be run
FAST, and a sibling step stamped ahead could DELAY it by up to the drift tolerance. §9.3 rules 3 and 4
restore it past `palw_clock_floor`.)* Only by a bounded, quantised amount. The cursor
lands on a slot BOUNDARY, never at `beat + interval`, so a timestamp buys whole slots and nothing
finer — 132 s of drift and 1 ms of drift cost the chain the same two slots. The beat's own timestamp
is bounded by `check_block_timestamp_in_isolation` at `now + TIMESTAMP_DEVIATION_TOLERANCE`, so with
the shipped constants the worst a beat buys is `1 + 132/120 = 2` slots: the clock can be slowed to
about 2×, per slot, at the price of a beat's work every slot, and it can never be stopped or moved
backwards. The arithmetic is pinned so a change to either constant is a decision rather than a
side effect.

## 7. Scope, and what this does not touch

* The interval is ADR-0138 §3c's: the recovery cadence where the parent paces no clock, the nominal
  hour where it does. That question is settled and this ADR does not reopen it.
* Fork-choice weight, the carrier policy and the difficulty window are untouched. ADR-0140's C1, C3
  and C4 hold as they did; this is C5.
* The cursor is `Option`, absent until the fence, so a chain on which the rule is dormant commits
  byte-identical state roots to a build without the field — the same discipline `model_markets` uses.
* No zero-knowledge machinery, and no new cryptographic primitive.

## 8. Rollout

Not into 6,301. Its own fence, above the tip, after: this ADR, the properties of §6, a drill at a
**fast** producer cadence (the one that exposed this), a reorg drill, and a drill at testnet-11's own
cadence. ADR-0140's release gate applies to it as to anything else — the fence must be *crossed* by a
drill whose lane composition and cadence are the target network's.

Until it lands, `palw_single_lottery` and `palw_anchor_clock` stay dormant, which is how they ship in
the 7,101 bundle. They arm together (`Params::set_palw_single_lottery`), and arming either without
this rule is the liveness failure in §1.

## 9. Amendment (2026-09-24): the clock floor — H1, H2, H3, H5 of the heartbeat audit

**Status:** H3 and H5 are consensus, behind their own fence `Params::palw_clock_floor` — `Some(0)` on
testnet-12, `None` on every other preset (testnet-11, devnet and mainnet fingerprint byte-identically
to the build before the field: `consensus/core/tests/palw_clock_floor_is_t12_only.rs`). H1 and H2 are
node policy and move no fingerprint. Built on `hb/t12-heartbeat`, for the testnet-12 regenesis.

### 9.1 What was measured

The 2026-09-24 audit of testnet-12 (live and a private replica) measured the tick as

```
tick = 120 s + a + b + c        observed 163–261 s per DAA
a = cursor opens → the miner has a fresh template (it never waited for the slot)
b = the 2^24 grind on one thread
c = the grant → the block that merges the beat and steps the clock
```

with heartbeats 74% of all blocks, **11% of beats granted a tick**, and blue score running 3–6× the
design rate. Two of this ADR's own claims were found false on the way.

### 9.2 Two claims this ADR made that did not hold

**§6a check 4 / §6 property 6 ("a producer cannot lock the clock with a future timestamp") and
`pre_pow_validation`'s "the DAA cannot run fast however many beats are minted".** Both relied on the
carried cursor's whole-slot advance, which §6a deleted. What runs is "the reference is the block that
advanced the score", and nothing constrained THAT block's timestamp:

* **Acceleration.** The future-drift tolerance (132 s) exceeds the interval (120 s). A beat stamped
  `ref + 120 s` is admissible the moment the reference exists, so it is granted at once, and the block
  that merges it — stamped "now", `ref + ε` — becomes the next reference. Nothing floored the spacing
  between two references; the clock could tick every few seconds for as long as someone paid 2^24 a
  beat.
* **Delay.** The reference was `min(blue_score, hash)` over the blocks at the parent's score. Two steps
  merging the same beat have the same parents, hence the same blue score, and the HASH chose between
  them — so a sibling step stamped up to 132 s in the future won half the time and pushed the next slot
  back by up to 132 s. Measured: +31 s at DAA 8, +18 s at DAA 24. The doc's "a future timestamp can
  never open a slot early" was true; "delays only its own lane" was not.

And §5's "a beat may be minted whenever its producer can pay for it, and earns a DAA only where a slot
is open" was the lane's whole waste: every beat minted between two slots was a valid block that could
never be granted.

### 9.3 The rules (past `palw_clock_floor`)

All read the ONE decision the DAA score is computed from — `palw_clock_step_v1`, now returned whole as
`PalwClockStepV1 { governs, cursor, granted, floor }` and carried in `DaaWindow::clock` — for the
header's own parents, so construction and validation cannot disagree (§5).

1. **H3 — a heartbeat is stamped at or after its slot.** `header.timestamp ≥ cursor.next_slot_ms`,
   where the cursor is the one the header's own window derives at its selected parent's score, else
   `HeartbeatBeforeItsSlot`. No margin below the slot: a beat below it can never be granted, and no
   honest node builds one — the adapter stamps `max(own clock, slot)` and the slot is a function of the
   beat's parents, so clock skew costs a miner a wait (or a stamp up to its skew in its own future,
   inside the 132 s tolerance), never a refusal.
2. **H3 — F5's chain exemption is paced.** A mergeset holding more than
   `PALW_HEARTBEAT_MAX_PER_MERGESET` heartbeats must still form one chain, AND hold at most
   `bound + 2·⌈span / interval⌉` of them (`heartbeat_chain_capacity_v1`), `span` being the spread of their
   timestamps, else `MergeSetHeartbeatChainUnpaced`. An honest chain holds two beats a slot (the one
   into the open slot, the step over it) with slots an interval apart, so it always fits while two
   miners' clocks disagree by under two intervals; a burst hung off heavier blocks at one score — each
   beat stamped for the same slot and valid alone — gets the flat bound. Templates apply the same
   predicate (`heartbeat_set_admissible`), judged at the selected parent's score plus one.
3. **H5 — a step is stamped at or after the slot it consumed.** A block whose mergeset is granted a
   beat's tick (`granted`) becomes the next slot's reference, so `header.timestamp ≥
   cursor.next_slot_ms`, else `ClockStepBeforeItsSlot`. The template builder stamps a step at
   `max(now, median + 1, slot)`. With this, every reference at score `d + 1` is at least one interval
   after some reference at `d`, so the minimum reference over any set of branches advances by at least
   one interval a tick: the 132 s drift buys a producer at most one slot of lead, once, and never a
   rate.
4. **H5 — the reference is the EARLIEST of the tied steps.** Among the blocks at the parent's score,
   the one with the lowest blue score is still the block that advanced it (§6a), but a tie on blue score
   is now broken by the lower TIMESTAMP, and only then by hash (`palw_clock_reference_v2`). §6a's
   objection to a timestamp minimum — an old but admissible timestamp pulling the reference back — no
   longer applies inside the tie: every tied step obeys rule 3, so none of them predates the slot it
   consumed. A future-stamped sibling can no longer delay the next slot.

### 9.4 Node policy (no fork)

* **H1 — the miner waits for its slot.** The adapter returns `earliest = max(template time, the
  cursor's next slot)` for the template's own parents; the yield hint answers `SlotTaken(next)` while
  the next slot is in the future and no beat in the virtual holds it (a granted beat waiting in the
  virtual is NOT "taken": the next block steps the clock and on a heartbeat-only chain only this lane
  builds it). The grind checks every 2^18 nonces for shutdown and for a moved virtual; the log says
  granted / holding / not granted instead of "the clock ticked".
* **H2 — the relay announces one beat per slot.** The first heartbeat validated per `(DAA score,
  selected parent's DAA score)` is announced, any other is kept and not announced; each peer has a
  heartbeat allowance (a burst of 12, one per 10 s). Orphan roots are exempt.

### 9.5 Expected effect

A slot now takes two beats, and the tick is the step's timestamp minus the previous step's:
`120 s + (the first finished grind among the miners) + propagation`. With one miner that is
`120 s + b + δ`; with `n` miners racing, `120 s + b/n + δ`. For `b` = 5–15 s on the fleet's cores, **≈ 126–134
s a tick and 2 beats a slot** (plus stragglers within a propagation delay, which H2 does not
re-announce), against 163–261 s and ~9 beats a slot measured. Blue score then runs at ~2 per tick,
the structural minimum for a heartbeat-only chain.

**Left for a decision.** (i) A beat is its own slot's step only if the DAA rule counts a beat into its
OWN score; that would make a tick one beat, not two, and is a larger consensus change than this
amendment. (ii) The miner could grind ahead of the slot and stamp the slot itself, taking `b` out of
the tick; it is not done because it means stamping into this node's own future by design.
~~~~~~~~
<!-- ADR_SOURCE_END:0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md -->

</details>

