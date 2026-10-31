# ADR-v2-0022 — 流動性seed・分割入金・virtual reserve

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)

## 1. この文書の役割

市場を開く条件、実入金、curveの仮想reserveを別々の会計として整理する。

## 2. 統合した判断の読み方

### 2.1

実BILIをseedして固定する初期案と、複数transactionに分けて入金する拡張を同じ市場lifecycleとして読む。[旧ADR-0090](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) [旧ADR-0094](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)

### 2.2

最低seedの変更は適用fenceで解決し、既に開いた市場や入金済みの残高を後の値で無断再評価しない。[旧ADR-0120](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md)

### 2.3

virtual reserveで市場を開く後続案では、価格計算用のvirtual値と実際に支払えるreal reserveを区別する。[旧ADR-0162](../adr/0162-the-pair-opens-on-a-virtual-reserve.md)

## 3. 維持する境界・後続改定

100,000／1,000,000／virtual reserveの各記録は異なる版の条件。統合本文はどれかを新たに全networkへ有効化しない。

## 4. 隣接する判断との分担

Positionの意味は[ADR-v2-0021](0021-positions-and-membership.md)、報酬からの買付と手数料は[ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | L1–L217 | L18–L23 |
| [0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | L1–L215 | L14–L19 |
| [0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | L1–L78 | L14–L25 |
| [0162-the-pair-opens-on-a-virtual-reserve.md](../adr/0162-the-pair-opens-on-a-virtual-reserve.md) | L1–L425 | L18–L33 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole"></a>

<details>
<summary>0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md — 原文 L1–L217</summary>

[原文を開く](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) · SHA-256: `73978cc6bc1acb1e441756fdcdf0e0525725cd4a111cf35d249d38b03c172213`

<!-- ADR_SOURCE_BEGIN:0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md -->
~~~~~~~~markdown
# ADR-0090 — The pair is seeded with real BILI, locked for good, and a position is whole

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — Positionと市場の対象を固定する

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。市場の`line_id`は固定されたmodel registration IDであり、Position、reserve、seed、reward buybackとEVMの参照先を改善版へ付け替えない。改善版は自身の登録・検証責任・報酬資格・独立AMMを持つ。既存市場の残高、価格、virtual reserveや実reserveを新登録へ継承・自動移動させない。新規登録が他モデルの市場を変更してはならない。以下に残るversion移行への期待は旧設計の記録であり、新規則では権利を与えない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-05; IMPLEMENTED 2026-09-05 on `palw-adr0088-0089-impl` (§8)
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decisions 1, 2,
  3 (a third move), 4 (no leg on the seed) and 8; [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decisions 3, 5 and 6 (a third action, a third event).
* Builds on: 0087 (the market), 0088 (the line the market is keyed by), 0089 (the EVM's hand).
* Supersedes nothing.

> **Amended (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past the fence no seed opens a market — every line's market opens at the line's creation on a virtual reserve of 10,000,000 MSK — and the seed becomes optional depth: any amount, accumulated, locked for good, taken only before the first trade (Decisions 2, 3 and 5). A market a seed opened before the fence keeps its row and quotes exactly as before; ADR-0162 §3 shows that a locked seed `S` and a virtual reserve `V = S` quote identically, so nothing this ADR's floor promised is lost.

> **Amended (2026-09-07, implemented the same day).** [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md): a hundred thousand MSK does not fit in one post-quantum transaction (fifteen ML-DSA-87 inputs is the mass cap's most, about a fifth of the floor from mining income), so Decision 2's "one seed a line" becomes an ACCUMULATION: each payment is locked in the sink on arrival, the market opens on the payment that carries the total across the floor, and until then the row has no positions, no price and no moves. Nothing here loosens — a payment never comes back, and it now cannot come back from the first instalment.

> **Amended (2026-09-06, design first).** [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md): the reserve grows by a third source — five percent of every model block's escrowed worker reward, at the claim's `Final`, with the positions the curve gives up retired — so Decision 2's floor tightens to `K / (supply − retired) ≥ seed` and §5's identity gains the slices as a source. Nothing here loosens: no seed leaves, no holder is paid.

## 0. The sentence this ADR is

A model's market is a pair the model's people make by locking at least one hundred thousand BILI
into it, BILI that no one — not the one who locked it, not the chain — ever pays out again; the
pair holds five hundred thousand whole positions, no fraction of one, and trades on a curve whose
product never falls, so buying raises the price and the seed stays under everything.

## 1. What the operator asked, in the operator's words

Adding a model to the site (misakaoptions.com) must make a *model–BILI pair*. To make it the user
prepares at least 100,000 BILI, and that BILI is fully locked: the user who made the pair cannot
withdraw it. The model is then approved as one that earns mining rewards. After that the pair
trades on the site as model positions and the price rises as people buy. The positions number
500,000, whole numbers only, fixed at issue. People buy them and the price goes up. That is the
goal, and this ADR is the rule set that makes each clause true.

## 2. What ADR-0087 had, and where it did not match

ADR-0087 opened every market by itself on the class's first buy, with **no BILI** in it and a
*virtual* reserve `V = 1,000 BILI` that set the first price and the curve's steepness; its supply
was 100,000 positions of `10^6` units each, so a position was divisible to a millionth; and the
constant `K = V × supply` was fixed at opening. Three clauses of §1 fail against that:

* there is no pair anybody *makes* — the curve conjures its liquidity from a constant;
* nothing is locked, because nothing was paid in;
* positions are fractional.

ADR-0088 and ADR-0089 stand as they are: the market is keyed by a **line**, the founding line's
id is the class id, and the EVM reaches the market through the writer and the facades.

## 3. Decisions

**Decision 1 — a position is a whole number, and there are five hundred thousand of them.**
`PALW_MODEL_POSITION_UNITS_V1 = 1` (the name survives so every reader that multiplies by it keeps
reading; the value is one), `PALW_MODEL_POSITION_SUPPLY_V1 = 500_000`. Every line opens with the
whole supply in the curve. The facade's `decimals()` is `0`. A buy that would release less than
one position releases nothing and is refused; a sell names whole positions.

**Decision 2 — a market opens by a seed and by nothing else; the seed is the reserve, and it
never leaves.** `PALW_MODEL_SEED_MIN_SOMPI_V1 = 100,000 BILI`. A line's market does not exist
until a seed of at least that reaches its sink. The whole seed becomes `msk_reserve` — no burn, no
leg, no position minted to anyone: the seeder holds nothing. There is no virtual reserve
(`PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1` is kept at zero so old arithmetic adds nothing). The curve
is `msk_reserve × position_units = K` with `K` taken from the row at every move — a buy adds the
net leg to the reserve and releases `units − ⌈K / reserve′⌉` positions; a sell returns positions
and pays `reserve − ⌈K / units′⌉`, never more than the reserve — so the product never falls. Hence
the invariant the operator asked for in words: **with every position back in the curve the
product puts the reserve at the seed or above; the reserve never falls under the seed.** No object
pays a reserve out but a holder's sell, and a sell cannot reach the seed. The first price is
`seed / 500,000` (0.2 BILI at the least seed). One seed a line: a second is refused. A line whose
class is frozen takes no seed; a class still waiting for its activation does — the pair is made
when the model is added, before approval (§1's order) — and its buys wait for `Active` as before.

**Decision 3 — a third move, on both lanes, unsigned like a buy.** On the carrier lane
`ModelSeed { line_id, seeder (a payout payload, for the record), msk_seed, sink_index }`, bound to
its carrier's sink output exactly as a buy is (the whole seed under the line's sink script). On
the EVM lane the writer's action id `3`, `data = 0x01 ‖ 0x000003 ‖ lineA ‖ lineB`, `msg.value` the
seed; the facade's `seed()` payable is the same action with the line filled in. The writer
refuses at the call a value under the floor (`SeedTooSmall()`), so a too-small seed never queues.
A filled seed settles at `C` like a filled buy — the escrow burns into the line's sink output —
and the facade emits `Seeded(holder, mskIn, priceAfter)`; a refused seed (already seeded, class
frozen, line missing) is a `Refused(holder, 3, amount, reason)` settlement whose escrow the child
refunds. Refusal reasons gain `ALREADY_SEEDED = 10`, `SEED_TOO_SMALL = 11`, `CLASS_CLOSED = 12`.
The settlement row names its move by `action` (buy 1 / sell 2 / seed 3) rather than a boolean;
`carries_escrow()` is true for a buy and a seed.

**Decision 4 — the fees are ADR-0087's, and the seed pays none.** 5 % burned and 1 % to the
line's owner (split with an adopted contributor, ADR-0088 Decision 8) on every buy's and sell's
BILI leg; nothing on the seed, which is liquidity, not a trade.

**Decision 5 — what a participant reads.** `getPalwModelMarket(lineId)` gains `seedSompi`,
`seededBy` (empty while unseeded) and `seedMinSompi`; `virtualSompi` is served as zero. The EVM
AMM window's `constants()` carries the least seed in the word that carried the virtual reserve.
CLI: `misaka palw model-seed --line --msk --key-file --yes` and `model-evm-seed --line --msk`
beside `model-buy/sell` and `model-evm-buy/sell`, whose `--min-positions` / `--positions` now
mean what they say. The site (`web/misaka-options/`) shows a seed panel for an unseeded line, an
"Add model" checklist (register the class, seed the pair, approval, trade), whole positions
everywhere, and "seed (locked)" on every market.

**Decision 6 — approval is what it already was.** "Approved as a model that earns mining
rewards" is the class reaching `Active` at its activation DAA and its lanes being certified
(ADR-0054, ADR-0075). Nothing here adds a vote or a gate; the seed simply may precede it.

**Decision 7 — same fences, no new one.** Everything above is under `palw_model_market`,
`palw_model_lines` and `palw_model_evm`, all still `None` on every shipped preset; the constants
and the object change under fences nothing has armed. Where the fences are armed later, this is
the market that opens. The drill flag `--palw-model-devnet` arms all three on a private devnet.

## 4. The arithmetic, worked

From a market seeded with the least seed (100,000 BILI; `K = 10^13 × 500,000`):

| move | BILI leg | positions | reserve after | price after |
|---|---|---|---|---|
| seed | 100,000 in, no fee | 500,000 in the curve | 100,000 | 0.2 |
| buy 1,000 BILI | 50 burned, 10 to the owner, 940 in | 4,656 out | 100,940 | 0.20377757 |
| buy 1,000 BILI | 50, 10, 940 | 4,570 out | 101,880 | 0.20759045 |
| sell 9,226 | 1,879.88976 gross; 1,767.0963744 net | 500,000 back | 100,000.11024 | 0.20000022 |

Selling everything ever bought leaves the reserve **above** the seed by the rounding the curve
keeps: the seed is not withdrawable by trading either. A buy of 0.1 BILI releases nothing; 0.22 BILI
releases one position.

## 5. Security — the four principles, checked

* **Nothing is minted.** `seed + Σ buys = reserve + Σ sells' net + burned + legs` at every state
  (test P2). A seed mints no position; the supply is the curve's plus every holder's, always.
* **Nothing is withdrawn by the seeder.** There is no unseed, no LP token, no admin; the seeder's
  payload is a record, not a key. The curve's product bound keeps the reserve ≥ seed under any
  sequence of buys and sells (test P1, at every size tried).
* **No transfer.** Unchanged from ADR-0087 Decision 5: positions are wallet-held and sold back;
  the facade's `transfer/approve/allowance` revert.
* **A user-input fault is a revert, a chain fault is a refusal, never a block fault.** A small seed
  reverts at the writer; an already-seeded line refuses at the fold as a settlement.

## 6. Invariants the tests must hold

* **P1 (the seed floor).** After every move `reserve × units ≥` the product before it, and
  `reserve ≥ seed`. `palw_model_market_v1::tests::the_product_never_falls_and_the_seed_never_leaves`.
* **P2 (conservation).** `seed + paid_in = reserve + paid_out + burned + legs` at every state the
  fold reaches. `palw_state_v2::tests::model_market::invariants`.
* **P3 (whole positions).** `PALW_MODEL_POSITION_UNITS_V1 == 1`; a dust buy releases nothing; the
  smallest buy that releases one releases exactly one. `a_position_is_whole_and_a_dust_buy_releases_nothing`.
* **P4 (one seed, at least the floor, no leg, nothing for the seeder).**
  `a_seed_is_once_at_least_the_floor_and_takes_no_leg` (fold), `a_seed_from_the_evm_opens_the_market_and_is_once_and_at_least_the_floor` (EVM).
* **P5 (no market before a seed; a seed before approval).** A buy on an unseeded line is
  `ModelMarketMissing`; a `Registered` class takes its seed and no buy.
  `a_seed_opens_the_market_and_buys_and_sells_move_the_curve_with_the_invariants_held`,
  `a_registered_class_takes_its_seed_before_activation_but_no_buys_and_a_registrant_is_paid`.
* **P6 (ADR-0087 M1–M8, ADR-0088 L1–L10, ADR-0089 E1–E11 hold unchanged** over the new rows.

## 7. Order of work

1. The curve and the constants; the row's `seed_sompi` / `seeded_by`; the goldens.
2. The object, its binding, the fold arm, the EVM action and settlement kind, the refusal codes.
3. Acceptance, RPC, CLI, the intercept's door and event, the Solidity interfaces.
4. The site: the seed panel, the add-model checklist, whole positions.
5. The devnet drill with a seed on the EVM lane and a refused second seed on the carrier lane.
6. Arming on testnet-11 (a release: the three fences enter the fingerprint). The edit is three
   lines in `consensus/core/src/config/params.rs`'s testnet-11 preset — `palw_model_market`,
   `palw_model_lines`, `palw_model_evm` from `None` to `Some(ForkActivation::new(<DAA>))` at one
   score a few hundred DAA ahead of the fleet's restart — then `cargo test -p kaspa-consensus-core
   --lib -- fence fingerprint` (the fingerprint tests pin the new value), a release build, and
   every node of the fleet restarted on it before the score (a node without the fences refuses
   the others at the handshake: fail-safe, but a partition until it is updated). The site reads
   the market the moment the score passes; the first pair is whoever seeds first.

## 8. Implementation record (2026-09-05, `palw-adr0088-0089-impl`)

Items 1–5 landed the same day; item 6 is the operator's release. `palw_model_market_v1.rs`
(constants, `seed_v1`, per-move `k()`, the guard that a zero-reserve row quotes nothing, five tests
with the §4 goldens), `palw_state_v2.rs` (`ModelSeed` appended to the object enum, `model_seed_v1`,
the buy's lazy open removed, the EVM `Seed` action and the settlement's `action`, three errors and
three refusal codes; nine fold tests re-based on seeded markets, two added), `palw_lifecycle_objects_v2.rs`
(the binding shared with the buy), `evm/model_market.rs` (action id 3, `send_action_seed_calldata`,
`carries_escrow`), the processor's acceptance arm, the panel's name, `processes/evm` (sinks for
every escrow-bearing settlement), `kaspa-evm` (the `Seed` action, the facade's `seed()` door,
`decimals() == 0`, `constants()` with the least seed, the `Seeded` event, `SeedTooSmall`, the
executor's `carries_escrow`), RPC (`seedSompi`, `seededBy`, `seedMinSompi`; wire version 3), CLI
(`model-seed`, `model-evm-seed`, the seed on `model-show`), `misaka/evm/contracts/misaka-model/` (the
interfaces and README), and the drill (`scripts/drills/misaka-palw-model-market-devnet-e2e.sh`: no market
before a seed, the EVM seed of 100,000 MSK opening the market at 0.2 MSK, the seeder holding
nothing, a second seed refused, buys in whole positions raising the price). The site's changes are
recorded in `web/misaka-options/README.md`.

## 9. What is deliberately not decided

* Whether a seed above the floor should ever be partially released to its seeder. Not here: the
  operator said "completely locked".
* Whether the seeder should receive positions for the seed. Not here: a seeder who wants
  exposure buys like anyone else, at the curve's price, and pays the legs.
* Whether a frozen class's market should refund holders. ADR-0087 Decision 7 stands: sells
  continue at the curve's price; the seed stays.

## 10. Number hygiene

0090 was the next free number on 2026-09-05 (README §"next free number"); the next is 0091.

## Item 6 executed, 2026-09-06 — the three fences are scheduled on testnet-11

`palw_model_market`, `palw_model_lines` and `palw_model_evm` are `Some(ForkActivation::new(1900))`
on `palw_rc_base_params`, at `PALW_RC_DA_COURT_FENCE_DAA` — the same height as ADR-0062's court and
ADR-0077 Decision 16's private prompt, because the gate keys on heights and three flag days would
cost three fleet restarts. The fingerprint moves (71b35c25… → b511dd1e…) and the identity does not,
so a seat on the previous build peers until the height and forks off at it; every seat must run the
new build before DAA 1,900. Until the height the market is dormant and the site's "market dormant"
line is still the true one.
~~~~~~~~
<!-- ADR_SOURCE_END:0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md -->

</details>

<a id="source-0094-a-seed-is-paid-in-as-many-transactions-as-it-takes"></a>

<details>
<summary>0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md — 原文 L1–L215</summary>

[原文を開く](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) · SHA-256: `15e27a3b29fb6599b0921e2ab259ffbd5905ff8b376297d929b38b0aa985ed49`

<!-- ADR_SOURCE_BEGIN:0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md -->
~~~~~~~~markdown
# ADR-0094 — A seed is paid in as many transactions as it takes

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-07; IMPLEMENTED 2026-09-07 (§8)
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2's last clause ("One seed a line: a second is refused") and Decision 5's row.
* Builds on: 0090 (the seed IS the reserve and never leaves), 0087 (the curve), 0091 (the reward
  buys the pair).
* Supersedes nothing.

> **Amended (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past the fence a pledge waits for no floor — a line whose pledges were short when the fence crossed opens at the fence on the virtual reserve with its pledges as its seed, and every later seed is any amount, added before the market's first trade and refused after it.

## 0. The sentence this ADR is

A hundred thousand BILI does not fit in one post-quantum transaction, so the seed is allowed to
arrive in several: every payment into the line's sink is locked the moment it lands, the market
opens on the payment that carries the total across the floor, and nothing in between is a market —
no price, no positions, no buys.

## 1. What was measured, on the live chain

Seeding the first pair on testnet-11 from mining rewards failed twice, and neither failure was
about the money being absent:

```
$ misaka palw model-seed --line 1c87442e… --msk 100000
error: no mature, unbonded UTXO at misakatest:qf6hf5v0… holds 100000.00000000 MSK plus a fee
```

The address held **190,049 MSK**. It held it in **202 coinbase outputs of about 1,400 MSK each**,
because that is how a producer is paid: one output a block. Consolidating them ran into the second
wall:

```
Rejected transaction …: transaction transient (storage) mass of 585524
is larger than max allowed size of 480000
```

An ML-DSA-87 input is large. **Fifteen inputs is the most one transaction fits** under the
480,000-mass cap (measured: 15 accepted, 20 refused). Fifteen coinbase outputs is about
**21,000 MSK** — a fifth of the floor. So:

* the CLI funds a carrier from ONE utxo (`palw_model.rs`'s `candidates.…find(|(_, e)| e.amount >
  msk_seed + fee)`), which is a defect on its own; and
* **even a perfect multi-input carrier cannot carry a 100,000 MSK seed from mining income**, because
  the mass cap stops it at roughly a fifth of the way.

The wall is therefore structural, and it is selective in a way worth saying out loud: **the
operator never hits it.** The premine sits in one large utxo, so the person who set the floor at
100,000 MSK can pay it in one transaction. Only a participant funded by mining — the participant
the floor exists to admit — meets the wall. A rule whose cost falls entirely on the newcomer is
the shape this project has twice decided is a defect (ADR-0054, ADR-0095's newcomer bond).

## 2. What was rejected

* **Lower the floor.** The floor is the operator's decision about what a pair must be worth; it is
  not the thing that is wrong.
* **Raise the mass cap.** It bounds a block; a signature scheme's size is not a reason to widen it.
* **Consolidate first, always.** It works — it is what the live seeding did — but it is fourteen
  rounds of one transaction a block, an hour of waiting, and it must be rediscovered by every
  participant. A workaround that everyone must find is a missing feature.
* **Let the CLI consolidate automatically.** Same hour, hidden. It also spends a participant's
  money on fees to work around a rule rather than obeying it.

## 3. Decisions

**Decision 1 — the seed accumulates, and each payment locks on arrival.** A `ModelSeed` on a line
whose market is not yet open ADDS its `msk_seed` to what that line has already collected. The
payment reaches the sink exactly as before — an unspendable `OP_RETURN` output — so **every sompi
is locked the moment it lands**, before the market exists and whether or not it ever does. There
is no refund, no withdrawal and no partial unwind: ADR-0090's "locked for good" is unchanged, and
it now begins earlier.

**Decision 2 — the market opens on the payment that crosses the floor, with the whole supply.**
While the collected total is under `PALW_MODEL_SEED_MIN_SOMPI_V1` the line has a row and no
market: `position_units` is zero, there is no price, and a buy or a sell is refused
(`ModelMarketNotOpen`). On the payment that brings the total to the floor or past it, the row
opens: `position_units` becomes `PALW_MODEL_SUPPLY_UNITS_V1`, `msk_reserve` is the WHOLE collected
total (the crossing payment's excess included, exactly as a single over-floor seed already
behaved), and the first price is `total / 500,000`. Opening happens once; a `ModelSeed` on an open
market is refused as it is today (`ModelMarketAlreadySeeded`).

**Decision 3 — anyone may pay, and the row names who started it.** The sink takes BILI from any
key: a line's people can fund a pair together, and nothing about the fold depends on them being
one wallet. `seeded_by` keeps its meaning as a record and names the **first** payer — the one who
opened the pledge — because that is the fact a reader wants ("who is behind this pair") and
because naming the last one would let a stranger's final sompi claim it. The count of payments is
not kept: it is not a fact any rule reads, and every payment is already visible as a sink output.

**Decision 4 — what a participant reads.** `getPalwModelMarket` gains `seed_pledged_sompi` (what
has been collected) beside `seed_sompi` (which stays the seed the market OPENED with, and is zero
until it does) and `opened`. `misaka palw model-show` prints the progress toward the floor. The
AMM window's `market()` keeps its word order and gains `seedPledged` at the END. The site shows a
progress line on an unopened pair and asks for the remainder, not the floor.

**Decision 5 — the CLI stops asking for one utxo.** `model-seed` funds its carrier from as many
mature utxos as the mass cap fits (measured at fifteen ML-DSA-87 inputs), pays what those inputs
can carry rather than refusing, and says what is left to pay. `line-found` and the other carriers
get the same multi-input funding, because the single-utxo `find` is the same defect everywhere.

**Decision 6 — same fences, no new one.** All of this is under `palw_model_market`, armed at DAA
1,900 on testnet-11 and `None` on every other preset.

## 4. The arithmetic, worked

A producer earning 1,400 BILI a block, seeding the least pair:

| | before | after |
|---|---|---|
| what one transaction can carry | 1 utxo (~1,400 BILI), and the seed is refused | 15 utxos (~21,000 BILI) |
| transactions to a 100,000 BILI pair | 14 consolidation rounds, then 1 seed | **5 seeds** |
| wall-clock at 240 s a block | ~1 hour of consolidation, then the seed | ~20 minutes |
| BILI locked before the market exists | 0 (nothing is paid until it all is) | each payment, on arrival |

The last row is the trade this ADR makes: a participant who stops halfway has locked what they
paid and has no market. That is the same bargain ADR-0090 already struck for a single seed — the
seed never comes back — applied to each instalment. It is stated on the CLI and on the site before
the first payment, because a rule that surprises is worse than a rule that costs.

## 5. Security — the four principles, checked

* **Nothing is minted.** A pledge is a sink output like any seed; the reserve at opening is exactly
  the sum of the sink outputs that fed it. `seed + Σ buys + Σ slices = reserve + Σ sells + burned +
  legs` (ADR-0090 P2, ADR-0091 B2) holds with the sum in place of the single seed.
* **Nothing is withdrawn.** There is no object that pays a pledge back, opened or not. A pledge on a
  line that never opens is burned BILI, which is what paying into an `OP_RETURN` means.
* **No market before the floor.** A buy or a sell on an unopened row is refused by the fold, and
  the row carries no positions to sell — the supply does not exist until the market does.
* **A user-input fault is a revert, a chain fault is a refusal, never a block fault.** Unchanged.

Attacks considered:

| | threat | why it is not one |
|---|---|---|
| A1 | a stranger pledges 1 sompi to a line to be named its seeder | `seeded_by` names the FIRST payer, and the first payer is the one who chose to start; a later sompi names nobody |
| A2 | a griefer pledges to a line they dislike, to open it at a price they choose | opening at a HIGHER total is a higher first price and more locked BILI — they pay for the privilege, and the line's people keep every sompi of it |
| A3 | a pledge sits unopened forever, BILI locked | the same as any under-floor payment into the sink, which ADR-0090 already burns; the CLI states it before the first payment |
| A4 | the accumulated row is read as a market by an old node | the fences are the same; below them there is no row at all, and past them a row with `position_units == 0` is refused by every move |

## 6. Invariants the tests must hold

* **S1 (accumulation).** Three pledges under the floor leave `position_units == 0`, no price, and a
  buy refused; their sum is the collected total.
* **S2 (the crossing).** The payment that reaches the floor opens the market with
  `PALW_MODEL_SUPPLY_UNITS_V1` in the curve and `msk_reserve` equal to the WHOLE collected total,
  including the crossing payment's excess.
* **S3 (once).** A `ModelSeed` on an open market is refused, as before.
* **S4 (locked from the first).** No object pays a pledge out; the reserve after opening is at or
  above the floor for every sequence of buys and sells (ADR-0090 P1 over the accumulated seed).
* **S5 (the record).** `seeded_by` is the first payer, unchanged by later ones.
* **S6 (the reader).** `seed_pledged_sompi` and `opened` are served by RPC and the AMM window, and
  every earlier word of `market()` keeps its offset.

## 7. Order of work

1. This text; the README rows; a banner on ADR-0090.
2. The row's `seed_pledged_sompi`, the open predicate, and the arithmetic's goldens.
3. The fold: `model_seed_v1` accumulates, opens at the floor, refuses on an open market; buys and
   sells refuse an unopened row.
4. RPC, CLI (`model-show`'s progress, `model-seed`'s multi-input funding), the AMM window's word.
5. The site: the progress line and the remainder.
6. Deployed with the next fleet release; the fences are already armed.

## 8. Implementation record

Written 2026-09-07 on `feat/adr-0094-accumulating-seed`, from a live failure to seed the first
testnet-11 pair. The implementation record is filled in as the work of §7 lands.

## 9. What is deliberately not decided

* Whether a pledge should expire and burn explicitly rather than sit. It is already burned — it is
  in a sink — and an expiry would only change the bookkeeping.
* Whether the count of payments or the list of payers should be kept. Neither is read by a rule.
* Whether the floor should differ for a pledged pair. It does not: the floor is the floor.

## 10. Number hygiene

0092 and 0093 were taken while ADR-0091 was being written; the README's "next free number" line
was stale and is corrected with this row. This is ADR-0094; the next free number is 0095.

## Amendment (2026-09-07): this needed a fence, and its field needed to stay derivable

The version of this ADR that shipped on 2026-09-06 had **no activation**, and that was a defect
found while ADR-0095 was being written. Two separate problems, both of which would have forked
testnet-11:

* **The encoding.** `seed_pledged_sompi` was a new field on `PalwModelMarketV1`, and
  `collection_root` borsh-serializes each row into the state root. Adding it changed the bytes of
  EVERY existing market, so a node running this build computes a different state root than one that
  is not — on a chain whose market opened at DAA 1,947, that is a fork on the next block, and no
  fence could have prevented it because the encoding never consults one. A self-delimiting tail does
  not fix it either: a market row is a value inside a `BTreeMap`, so a reader peeking one byte past
  its row steals the next row's first byte. **The fix is that the field is not new information**: an
  open market has `seed_pledged_sompi == seed_sompi`, and a pledged one has `seed_sompi == 0` with
  its whole reserve being the pledge. So the encoding writes the pre-ADR-0094 fields exactly and the
  reader derives the pledge, with the invariant asserted on the way out rather than assumed.
* **The behaviour.** Accepting a sub-floor payment is a rule change: below the fence such an object
  is refused, so a node that took one would build a block its peers reject. The accumulate arms now
  ride `Params::palw_model_benefits` — ADR-0095's fence, scheduled on testnet-11 at DAA 2,400 —
  because the two ship in one build and should cross at one height. Below it, this behaves exactly
  as it did before this ADR, and the tests that stay below the fence are what pin that.

The lesson generalises and is why ADR-0095 §4.11 reads as it does: **a row that enters the state
root cannot be extended under a live chain, fence or no fence.** Ask what the encoding does before
asking what the rule does.
~~~~~~~~
<!-- ADR_SOURCE_END:0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md -->

</details>

<a id="source-0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height"></a>

<details>
<summary>0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md — 原文 L1–L78</summary>

[原文を開く](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) · SHA-256: `f3f575c898928935f2cada5bda2a01823047993632c36275d0fd88d0a2e77b11`

<!-- ADR_SOURCE_BEGIN:0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md -->
~~~~~~~~markdown
# ADR-0120 — The least seed is one million BILI, and it arrives at a height

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-12 at the operator's request ("misaka position のモデル追加の際に流動性として
  ロックして引き出せないようにする misaka の数を 1M として　今の 100000 枚から引き上げて"). **IMPLEMENTED
  the same day** on `feat/adr-0120-seed-min-1m`, and **scheduled on testnet-11 at DAA 6,900** by the
  operator the same day, to ship with the 7,000 release at a height of its own (unchanged on 2026-09-17,
  when the operator moved the 7,000 heights to 6,000: 6,900 now follows them):
  `PALW_RC_MODEL_SEED_V2_FENCE_DAA`. Every other preset keeps `palw_model_seed_v2 = None`.
  **6,900 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the least seed, 100,000 BILI) and [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
  (the floor a pledge collects toward).
* Builds on: [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) (a market rule
  that changes what the fold writes arrives by activation, read through one resolved fence).

> **Amended (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past `palw_model_virtual_v1` the least seed is zero — no seed opens anything, every line's market opens at its creation on a virtual reserve — and `Params::palw_model_seed_min_sompi_at` answers 0 there whatever `palw_model_seed_v2` says.

## 1. What is asked, and why it is a consensus rule

A line's market opens only once BILI is paid into the line's sink and locked there for good — the seed
(ADR-0090). ADR-0094 lets the seed arrive in instalments: every payment is collected in
`seed_pledged_sompi`, and the pair becomes a market the moment the collected total reaches the least
seed. The operator raises that least seed from 100,000 BILI to 1,000,000 BILI.

The floor is read by the fold (`model_seed_v1`): it decides whether a payment opens the pair
(`seed_v1` / `open_from_pledge_v1`) or is only collected (`pledge_v1`). That decision is written into
the market row, which is in the state root. Changing the constant in place would re-fold history
differently on every node that re-validates it — so, like ADR-0114, the new floor arrives at a height.

## 2. Decision

1. **`Params::palw_model_seed_v2`, a bare fence, read through `palw_model_seed_v2_fence`** — `Some` only
   where the market is armed too. Below it the floor is `PALW_MODEL_SEED_MIN_SOMPI_V1` (100,000 BILI),
   at and past it `PALW_MODEL_SEED_MIN_SOMPI_V2` (1,000,000 BILI); `palw_model_seed_min_sompi(active)` is
   the one spelling, and `Params::palw_model_seed_min_sompi_at(daa)` resolves it at a height.
2. **The fold reads the floor at the block's own DAA** through
   `PalwTransitionExtrasV1::model_seed_v2_active`, written explicitly by the virtual processor beside
   ADR-0114's leg. `false` by `Default`, so every caller that does not set it keeps the floor every
   existing row was opened under.
3. **Nothing already paid is lost or re-judged.** An open market stays open whatever the floor is now
   (`seed_remaining_sompi_under` owes nothing once `is_open`). A line whose pledges were short when the
   fence crossed keeps them — every sompi is already locked in its sink — and opens when the collected
   total reaches the new floor.
4. **The EVM window names the floor in force** (`constants()`'s third word, through
   `PalwEvmMarketFencesV1::seed_v2_active`). The writer keeps ADR-0094's rule — any non-zero seed is
   taken and the fold collects it — so no handler or address changes.
5. **The RPC serves the floor at the virtual's DAA** in `getPalwModelMarket`'s `seedMinSompi`, for an
   unseeded line as well (it answered 0 there before), so the CLI's `model-market`, the site and the
   Studio print what the fold will open the pair at.
6. **The fork id sees it.** 6,900 is a height no other fence uses: `fork_id_v1` digests fired heights,
   not fence sets, so a fence sharing 6,000 with held context and the audit's deep fixes would let a
   build without it peer through the gate and part silently there.

## 3. Consequences

* Opening a model's market costs ten times what it did; the seed is still wholly the reserve and still
  nobody's to withdraw. The curve starts at `seed / supply`, so the first price is ten times higher too.
* A line that has pledged between 100,000 and 1,000,000 BILI before 6,900 and has not opened by then
  waits for more pledges; nothing refunds it (a pledge was never refundable).
* The t11 fingerprint moves at the 7,000 release, which re-pins once for all of its fences.

## 4. Alternatives not taken

* **A policy floor in the tools only** (site, CLI, Studio refuse less than 1,000,000 BILI): the chain
  would still open any pair at 100,000 BILI paid by any other client, so the rule would be a suggestion.
* **Sharing 6,000**: one height for four fences, and a build missing one of them invisible to the gate
  (see Decision 6).
~~~~~~~~
<!-- ADR_SOURCE_END:0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md -->

</details>

<a id="source-0162-the-pair-opens-on-a-virtual-reserve"></a>

<details>
<summary>0162-the-pair-opens-on-a-virtual-reserve.md — 原文 L1–L425</summary>

[原文を開く](../adr/0162-the-pair-opens-on-a-virtual-reserve.md) · SHA-256: `1d050f11843da18706f3f12a5ab6b083168185884abc97a4ec86fd83f51367be`

<!-- ADR_SOURCE_BEGIN:0162-the-pair-opens-on-a-virtual-reserve.md -->
~~~~~~~~markdown
# ADR-0162 — The pair opens on a virtual reserve

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — Positionと市場の対象を固定する

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。市場の`line_id`は固定されたmodel registration IDであり、Position、reserve、seed、reward buybackとEVMの参照先を改善版へ付け替えない。改善版は自身の登録・検証責任・報酬資格・独立AMMを持つ。既存市場の残高、価格、virtual reserveや実reserveを新登録へ継承・自動移動させない。新規登録が他モデルの市場を変更してはならない。以下に残るversion移行への期待は旧設計の記録であり、新規則では権利を与えない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-10-01 at the operator's request; **IMPLEMENTED the same day** on
  `position/virtual-reserve` (§10), behind its own dormant fence `Params::palw_model_virtual_v1` —
  `None` on every preset, so no network's fingerprint, schedule or fold moves.
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 2 (the
  curve is over `X = V + reserve` again, and the market of a line exists from the line's creation);
  [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decisions 2, 3 and 5 (no seed opens a market; the seed is optional depth, taken before the first
  trade; `constants()`' third word is the virtual reserve again);
  [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) (a pledge opens at the fence,
  with the pledge as its seed); [0120](0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md)
  (the least seed is zero past this fence); [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  Decision 3 (the reward buys only a pair that trades); [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decision 2 (the window's `market()` gains a twelfth word, `quoteSell`'s first word is the gross).
* Builds on: 0087 §0 (a position is not a share), 0088 (the line), 0114 (the fee schedules), 0152
  IMPL-15 (the buyback in a seat's lock price).
* Supersedes nothing.

## 0. The sentence this ADR is

A model's market opens the moment the model is added, on ten million BILI of virtual reserve that
prices every position and is never paid to anyone, so nobody has to lock a million BILI for a market
to exist; trading starts when the chain approves the model, a seed is optional depth paid before the
first trade, and every sompi a seller receives is a sompi a buyer, or the reward, paid in.

## 1. What the operator asked

Relayed by the coordinating session on 2026-10-01, approved by the operator, and revised by the
operator twice the same day:

1. **Model Positions open on a virtual reserve, pump.fun-style**, so that a model's market no longer
   needs a large locked BILI seed: `V = 10,000,000 BILI`, the curve over `X = V + reserve`, 500,000 whole
   positions, a first price of 20 BILI, the fees and the reward's buyback unchanged.
2. **The seed is optional.** It may still be paid, in any amount and in instalments; it is locked for
   good and raises the price floor. **Seeds are accepted only before the first trade.**
3. No anti-spam is added and no opening fee: a market exists only for a registered line, and
   registration already costs BILI and bonds.
4. **The market opens automatically at the moment the model is added.** No later opening step, no
   seed step, no first-buy opening; a line that exists when the fence activates is open from the
   activation height.
5. **Buying and selling start only when the class is Active, i.e. approved** — `ActiveLimited` does
   not count. Before approval every surface shows the market, its price and its floor, with the
   status "承認待ち / trading starts at approval". A class rejected or frozen before approval never
   trades, and a seed paid into it stays locked: a seed is at the opener's risk.

## 2. What ADR-0090 had, and why it changes

ADR-0090 retired ADR-0087's virtual reserve because "the curve conjures its liquidity from a
constant": a market had to be MADE by somebody locking real BILI, 100,000 BILI and, past ADR-0120, one
million. That made every pair a pair someone had paid a permanent million BILI to open — before the
model had earned anything, and whether or not it ever would. On testnet-12, where ADR-0120's floor is
in force from genesis, no line has a market today.

The objection to the virtual reserve was never arithmetic: ADR-0087's `V` was never paid out either
(its sell's gross was capped by the reserve). It was that nothing stood behind the opening price. §3
shows that a locked seed does not stand behind it either.

## 3. The equivalence

**A locked seed `S` quotes exactly like a virtual reserve `V = S`, because nobody is ever paid out
of a seed.** Under ADR-0090 the curve is `reserve × units = K` with the reserve at the seed and every
position in the curve at the opening, and the checked refusal of ADR-0090 Decision 2 (audit M-12)
keeps every sell's gross inside `reserve − seed`. Write `reserve = S + r`, where `r` is everything
buyers and the reward paid in, less what sellers were paid: the curve is `(S + r) × units = K`, a
sell may take at most `r`, and the seed's `S` never moves. Now replace the locked `S` by a virtual
`V = S` that nobody locked: the curve is `(V + r) × units = K`, a sell may take at most `r` (the real
reserve above a seed of zero), and `V` never moves. Every buy releases the same units for the same
BILI, every sell pays the same gross, every reward slice retires the same positions, and the two
markets' real reserves differ by exactly `S` at every step. The seed's ONLY effect was its opener's
permanent cost.

Pinned by `a_virtual_market_quotes_every_move_as_a_pair_seeded_with_v` (I-V4): random buys, sells,
reward slices and pre-trade seeds under both fee schedules, every quote equal, every product equal,
the reserves `V` apart.

## 4. Decisions

**Decision 1 — every line's market is open from the line's creation.** Past the fence a line's market
is the row the fold wrote, or else the opening every reader synthesizes: `X = V`, the whole supply in
the curve, no BILI, `opened_daa` the line's founding height (the class's registration for a founding
line, the `ModelLineFounded` block for the others) — or the fence's height for a line older than the
fence. The first quote is `V / 500,000` = 20 BILI. The row is written lazily, by the first move that
changes it (a seed, a buy, the reward's buy), as ADR-0087 §7 learnt: the same observable market at a
fraction of the fold, and no edge at the activation. ONE function opens a line —
`PalwChainStateV2::model_market_in_force_v2` over `palw_model_market_in_force_v2` — and the fold,
`getPalwModelMarket`, the EVM window and ADR-0152's lock price all call it. The floor class has no
line (`ModelLineOnFloor`, ADR-0088), so the fence opens nothing for it.

**Decision 2 — the curve.** `PALW_MODEL_MARKET_VIRTUAL_SOMPI_V2 = 10,000,000 BILI`. The curve is over
`X = virtual_sompi + msk_reserve` and the positions in the curve, `K = X × units` taken from the row
at every move, exactly as before: a buy adds its net leg to the real reserve and releases
`units − ⌈K / X′⌉` whole positions; a sell pays `X − ⌈K / units′⌉`, and **never more than the real
reserve above the locked seed** — a checked refusal, whole, never a partial fill (ADR-0087 M5); a
reward slice adds to the real reserve and retires what `⌈K / X′⌉` gives up. The price is
`X / units` and its floor `⌈K / (supply − retired)⌉ / (supply − retired)`, which only rises; at the
opening it is `(V + seed) / supply`. The virtual reserve is **stored in the row it opened**
(`PalwModelMarketV1::virtual_sompi`), so a later constant moves no market that exists, and a market
opened before the fence has `V = 0` and its ADR-0090 curve. It is encoded behind bit 1 of the row's
`closed_to_buys` byte, with the word at the row's end: a row with `V = 0` has its old bytes, so the
state root, the carriage and the deltas of every market any network holds are unchanged; the encoding
is one-to-one.

**Decision 3 — a pledge opens at the fence.** A line still collecting ADR-0094's floor when the fence
crosses opens on `V` with its whole pledge as its seed — every sompi of it was locked in the line's
sink already — and the first payer stays the record. ADR-0120's least seed is zero past the fence.

**Decision 4 — the seed is optional, and taken only before the first trade.** Any amount, in as many
payments as it takes, from the line's creation until its market's first trade (`sold_units == 0`):
the whole payment joins the real reserve and the locked seed, fee-free; no position is minted to
anyone; the floor rises by `seed / supply`. After the first trade a seed is refused —
`ModelSeedAfterTrade` at the fold, refusal reason 14 (`SEED_AFTER_TRADE`) on the EVM lane with the
escrow refunded, `SeedAfterTrade()` at the writer's call — because **a seed paid while positions are
out raises their price, and their holders sell part of it back out of the curve** (§5.3 measures it).
A seed asks no registry lifecycle: it is the opener's bet, and a class that is never approved never
trades — its seed stays locked for good. A frozen class takes no seed, nor does a retired line.

*Paid with the registration's own carrier?* A carrier carries ONE object
(`PalwLifecycleTxPayloadV2 { version, object }`), so a registration that also seeds is a new object
version, which this ADR does not take. A seed is its own carrier, paid any time from the registration
on; since no trade can come before the class's approval (Decision 5), no buy can pre-empt it.

**Decision 5 — trading starts at approval.** A buy is applied only where the class's status is
`Active` AND, where the model registry is in force, its lifecycle is exactly `Active` — `Probation`,
`ActiveLimited`, `Held` and `Candidate` admit claims or exist and still admit no buy
(`ModelClassNotActive`, `ModelClassNotTrading`: both the EVM's reason 3, "class or line not
active"). The gate is asked live; the row's `closed_to_buys` is a record of its last move, not the
gate — which also closes the 2026-09-25 Position review's #4 (a class that came back to `Active` found
its market shut by the flag the last sell wrote). **A sell is never gated**: no position exists before
the class's first `Active`, so none can be sold before it, and a holder can always leave a class that
has since been demoted, frozen or retired — sells drain, as ADR-0087 Decision 7 has always said.
Buying before approval was a bet on approval; past this fence it cannot be placed.

**Decision 6 — the reward buys a pair that trades.** ADR-0091's rule is unchanged — five percent of a
claim's escrowed worker reward buys from the line's pair at the claim's `Final` "if that line's market
exists and is open to buys" — and past the fence "open to buys" is Decision 5's gate. Before approval
the miner is paid the whole escrow (ADR-0091 Decision 3's "a market closed to buys"), so a class
rejected before approval has locked no miner's BILI in a pair that never trades, and trading opens
exactly at `(V + seed) / supply`. ADR-0152's lock price reads the same function
(`model_buyback_market_v2`), so a seat's lock and the fold never disagree about the slice.

**Decision 7 — the fees are unchanged.** ADR-0087's 5 % burn and 1 % leg below ADR-0114's fence, 5 % +
5 % past it, on every buy and every sell; nothing on a seed and nothing on the reward's buy.

**Decision 8 — the fence, and what it takes precedence over.** `Params::palw_model_virtual_v1:
Option<ForkActivation>`, read through `palw_model_virtual_v1_fence` (folded with the market's own
fence: a virtual reserve for a market that does not exist is meaningless). Some-only in the params id,
in the schedule id and in `for_each_fence` (lane sink's shape), its `Some(never())` collapsed, named in
`palw_fences_v1`, the fork id's table and the ruleset candidate's; the fold reads it as
`PalwTransitionExtrasV1::model_virtual_v1`, the height where it is in force at the block (the height
is the opening an older line records). **`None` on every preset.** testnet-11 schedules the market at
1,900 and testnet-12 arms it from genesis; while this fence is `None` both fold exactly as they did
(I-V8). Past it: the least seed is zero whatever `palw_model_seed_v2` says; ADR-0094's pledging is moot
(every line is open); ADR-0114's schedule applies to virtual markets as to seeded ones; P-B3's
lifecycle gate (`palw_audit_2026_09_23`) stops applying to seeds and is replaced, for buys, by
Decision 5's stricter one.

**Decision 9 — what a participant reads.** `getPalwModelMarket` adds no field: `virtualSompi` is the
row's own reserve (`V`, or 0 for a pre-fence market — what every node served), `seedMinSompi` is zero,
`opened` is true from the line's creation, and `marketRefusal` is the buy's ("…trades from the class's
approval"). The EVM window: `constants()`' third word is `V` (ADR-0090 had put the least seed there),
`market()` appends `virtualSompi` as a twelfth word (`IMisakaModelAMMVirtual`), the quotes include `V`,
and `quoteSell`'s first word is the gross `mskOut` (the 2026-09-25 Position review's #2, fixed with
this fence so no execution result moves on a network that has not crossed it); the writer reverts a
buy of a class that does not trade yet, never a seed for its lifecycle, and a seed after the first
trade. The CLI: `model-show` prints the virtual and real reserves, the price, the floor, the positions
out and whether the market trades; `model-seed` is optional deepening and previews the floor it
raises; `misaka model market open` reports the market open and pays an optional seed. The site
(`web/misaka-options/`): add a model = register → the market is open at once → trading at approval;
the seed is an optional step; every market shows the virtual reserve (for price calculation only),
the real reserve, the price, the floor and the positions out, with the approval status beside it.

## 5. The arithmetic, worked

### 5.1 The four moves, no fee

From the opening, `X = 10,000,000 BILI`, 500,000 positions (pinned by
`the_adr_0162_table_is_the_virtual_curves_arithmetic`):

| move | positions | real reserve after | price after (BILI) |
|---|---|---|---|
| the opening | 500,000 in the curve | 0 | 20.00000000 |
| A buys with 100,000 BILI | 4,950 out | 100,000 | 20.40197959 |
| B buys with 1,000,000 BILI | 44,599 out | 1,100,000 | 24.64196993 |
| A sells all 4,950 | paid **120,651.90897692** BILI | 979,348.09102308 | 24.10918748 |
| B sells all 44,599 | paid **979,335.89102307** BILI | **12.20000001** | 20.00002440 |

Every position is back in the curve and the real reserve holds 12.2 BILI: the rounding's dust (whole
positions round every buy down, ceilings round every sell down), at most a position's worth a move.
`120,651.91 + 979,335.89 + 12.20 = 1,100,000`: nobody was paid out of `V`.

### 5.2 The same four moves under the fee schedules

| schedule | A gets | B gets | A is paid (net) | B is paid (net) | reserve ends |
|---|---|---|---|---|---|
| ADR-0087 (5 % burn + 1 % owner) | 4,656 | 42,198 | 105,486.31452026 | 866,449.31315975 | 25.92800001 |
| ADR-0114 (5 % burn + 5 % owner) | 4,459 | 40,581 | 95,999.44495113 | 794,981.83504888 | 20.80000001 |

The curve is scale-free: 94,000 BILI into ten million releases ADR-0090 §4's 4,656, which was 940 BILI
into a hundred thousand.

### 5.3 What a late seed would do

After A's and B's buys (no fee), a seed of 1,000,000 BILI paid by someone else would let A sell for
**131,521.45** BILI what was worth 120,651.91 — 10,869.54 BILI of the seed's price taken out by A — and
B could then not leave at all: the curve would owe B 1,067,564.35 BILI against 968,478.55 above the
seed, so B's sell is refused, and paid unguarded it would have taken the reserve **99,085.80 BILI under
the seed** that was "locked for good". Before the first trade the same seed is safe: no position is
out for anyone to sell it to. Pinned by `a_late_seed_would_be_sold_out_of_the_curve_so_it_is_refused`.

### 5.4 Depth

The price is `X² / K`, so from the opening (net legs, no fee): doubling it takes
`(√2 − 1) × V ≈ 4.14 M BILI`, four times it takes exactly `V` = 10 M BILI (half the positions leave), and
ten times it takes `(√10 − 1) × V ≈ 21.6 M BILI`
(`doubling_the_first_price_takes_four_million_msk_and_ten_times_it_takes_twenty_one`).

## 6. Invariants the tests hold

Property tests over random histories (seeded splitmix64, so a failure names its case), under both fee
schedules, in `palw_model_market_v1::adr0162_virtual_reserve`; the fold's half in
`palw_state_v2::tests::model_market_virtual` and `adr0135`.

* **I-V1** The real reserve is at or above the locked seed at every step.
* **I-V2** Σ sellers' gross ≤ Σ buys' net + Σ reward slices; per move, a sell's gross ≤ the real
  reserve above the seed. Nobody is paid out of `V` or a seed.
* **I-V3** The price is at or above the floor, the floor at or above `(V + seed) / supply`, and the
  floor never falls (it rises with every retired position).
* **I-V4** A virtual market with seed `s` quotes every move exactly as ADR-0090's pair seeded with
  `V + s` (§3).
* **I-V5** Whole positions; every rounding leaves the product at or above where it was.
* **I-V6** With every position back in the curve, the real reserve is the seed, plus at least what the
  retired positions pin at the floor (`(V + seed) × retired / (supply − retired)`) and at most the
  reward's slices, plus the rounding's dust (the moves' slack spread over the curve).
* **I-V7** A seed after the first trade is refused, at the arithmetic, the fold and the EVM lane, and
  §5.3's extraction is what it prevents.
* **I-V8** Fence off ⇒ byte-identical: the pre-ADR quote functions kept verbatim in the test agree
  with the live ones on every pre-fence row; a pre-fence row's bytes are the pre-fence encoding; a pair
  seeded under ADR-0120 before the fence folds after it to the same state root; every existing golden
  and vector is untouched; no preset's params id, identity or fork id moves
  (`the_virtual_reserve_fence_is_dormant_everywhere_and_moves_no_id_while_it_is`; the t12 repin shows
  no drift).
* **I-V9** Every registered line has an open market quote at every DAA after its creation (or after
  the fence), and the first quote is `V / 500,000`.
* **I-V10** No buy is ever applied while the class is not approved (status and lifecycle exactly
  `Active`); no sell can be applied before the class's first `Active`, because no position exists; a
  sell after a demotion, a freeze or a retirement drains as before.

## 7. What stays true of ADR-0087 §0

Every row of §0's table holds, and the virtual reserve adds none:

* **No issuer.** A line is a `(class, owner, name)` row; the market opening at its creation makes no
  one an issuer — the opening is the chain's arithmetic, not anybody's offer.
* **Nothing is paid to a holder.** The fold still has no move that pays one; the only BILI a holder
  receives is what the curve pays for a position they sell back, and that is BILI buyers or the reward
  paid in (I-V2).
* **No claim on anything.** Not on the seed, not on the reserve — and not on `V`, which is not an
  asset, a debt or a pool: it is a constant in a price formula, and no object can move a sompi of it.
* **No vote, no seat, no transfer, no maturity, no promise of a price.** Unchanged.
* **The words.** Positions, holders, a line, a membership — never shares, stock, equity, dividends,
  investors, 株, 配当 or 出資.

## 8. User protection

* **Before approval nobody can buy**, so nobody can hold a position in a model the chain is still
  trying out; the site and the CLI say "承認待ち / trading starts at approval" beside the price.
* **A seed is at the opener's risk**, and every surface says so before it is paid: locked for good,
  no position, and a class rejected or frozen before approval never trades.
* **The floor is real**: the price never falls under `(V + seed) / supply`, because the product never
  falls; what the floor does not promise is that the reserve can pay everyone that price — it cannot,
  and §5.1 shows the last seller paid at the curve's own slope.

## 9. Attacks considered

| | threat | why it is not one |
|---|---|---|
| A1 | register lines to get markets | a market is a pure function of a registered line; registration already costs BILI and bonds; a market nobody trades costs the chain nothing (no row is written) |
| A2 | seed after holders are in, to let some of them extract it | refused (Decision 4, §5.3) |
| A3 | buy before approval, dump at approval | there is no buy before approval (Decision 5) |
| A4 | the reward's buy before approval locks miners' BILI in a pair that never trades | the reward buys only a pair that trades (Decision 6) |
| A5 | the fence reprices an existing market | `V` is per row; a pre-fence market keeps `V = 0` (I-V8) |
| A6 | a sell paid out of `V` or the seed | a checked refusal at the arithmetic (I-V2), whatever the rounding |
| A7 | a market on the floor class | the floor has no line, so nothing opens (Decision 1) |

## 10. Implementation record (2026-10-01, `position/virtual-reserve`)

* `consensus/core/src/palw_model_market_v1.rs` — the constant, the row's `virtual_sompi` and its
  encoding, `curve_x`, the price floor, `positions_out`, the opening (`open_virtual_v2`,
  `open_virtual_from_pledge_v2`, `palw_model_market_in_force_v2`), the seed
  (`palw_model_seed_deepen_v2`), `X`-based buy/sell/buyback quotes with the sell's real-reserve
  refusal, `palw_model_seed_min_sompi_v2`; the module `adr0162_virtual_reserve` (I-V1..I-V8).
* `consensus/core/src/palw_state_v2.rs` — the extras' `model_virtual_v1`, `ModelSeedAfterTrade`,
  `ModelClassNotTrading`, `model_market_in_force_v2`, the trading gate, the buyback's market, the
  seed/buy/sell arms past the fence, the outside readers; the tests `model_market_virtual` and the
  two in `adr0135`.
* `consensus/core/src/config/params.rs`, `fork_id_v1.rs`, `misaka-palw-extension` — the fence;
  `consensus/core/src/evm/model_market.rs` — `virtual_v1_from`, reason 14;
  `consensus/src/…/processor.rs` — the fence at the block's DAA, the EVM window's refused set;
  `consensus/src/consensus/mod.rs` — the read.
* `misaka/evm/core/src/model_market.rs`, `executor.rs` — the window and the writer; `rpc/service` — the
  answer; `misaka-cli` — `model-show`, `model-seed`, `model-buy`/`model-sell`, `model-evm-seed`,
  `misaka model market open`; `misaka/evm/contracts/misaka-model/` — the interfaces; `web/misaka-options/` — the
  site.

### 10a. The one fold that takes default model extras has no caller

`consensus/src/processes/palw_state_v2_sync.rs` (`PalwStateSyncV2::advance`) folds every step with
`..Default::default()` for the model fences — `model_lines_active`, `model_benefits_active`,
`evm_market_active`, `model_leg_v2_active`, `model_seed_v2_active`, this ADR's `model_virtual_v1`,
the EVM actions and the carrier refunds. ADR-0114 §6 names this gap for the leg. On testnet-12, which
arms the market, the lines, the EVM window, the owner's 5 % leg and the million-MSK floor from
genesis, that walk WOULD fold a buy at the 1 % leg instead of 5 %, judge a seed against the
100,000 MSK floor instead of the million, and pay no carrier refund. It would write a different
state root from the network's once a market had moves. The coordinator asked on 2026-10-02 whether
any node reaches that path. **None does**, whichever fences are armed:

1. **Nothing constructs the walk.** `PalwStateSyncV2` and `PalwChainStepV2` appear in no source file
   but their own module. `processes/mod.rs` declares the module, and its own header says "Nothing
   constructs this on any preset".
2. **Every block a node folds, live or in IBD from genesis, goes through the virtual processor's chain
   walk.** That walk is `apply_palw_transition_v7` (`processor.rs`, the selected-chain walk) with
   `palw_transition_extras_for_objects(&point, &objects)`, the EVM step's `market_actions` and the
   filter's `carrier_market_refunds`. The extras builder writes every model fence explicitly from the
   block's own DAA (`palw_model_lines_active_at`, `…_benefits_…`, `…_evm_…`, `…_leg_v2_…`,
   `…_seed_v2_…`, `palw_model_virtual_v1_from_at`). The acceptance filters
   (`palw_v2_apply_one_object_v1`), the registration's drop check and the genesis fold take the same
   builder.
3. **A pruning-point sync folds nothing.** `import_pruning_point_palw_state` installs a carriage
   only if it matches the root committed by the pruning point's selected-chain child. From there the
   chain walk of (2) folds every later block.
4. **A reorg folds nothing either.** The processor moves between chain states with
   `processes/palw_state_walk.rs`, which reverts and applies the STORED deltas (`revert_delta_v2` /
   `apply_delta_v2`, which check the value each entry replaces). The chain walk of (2) wrote those
   deltas, and the reorg never re-runs a transition.

So the defaults are harmless today. The day someone wires the walk, it must take the model fences the
way the processor's builder resolves them, at each step's DAA, before it can be called on a network
that arms the market. The test
`model_market_virtual::the_sync_walk_that_folds_with_default_model_extras_has_no_caller_and_the_chain_walk_resolves_every_model_fence`
pins both halves: it walks the workspace for any name of the walk outside its module, and it reads
the chain walk's builder for every model fence and the chain walk for its actions and refunds. A new
caller, or a builder that stops writing a fence, fails it before any network sees the result.

### 10b. The 2026-09-25 Position review, ported where this ADR's surfaces touch it

The review's work branch (`rcore/position-fixes` `b15237848`, unbuilt and unreviewed, never merged
whole) was read item by item. Each item below was ported with its test as a commit of its own on this
branch, except #3 and #4 (see each):

* **#2** `quoteSell`'s first word is the GROSS `mskOut` (it answered the net twice). It is fixed past
  this ADR's fence only, where every quote of the window moves anyway; below it the word stays the
  same byte for byte, so no execution result moves on a network that has not crossed the fence.
* **#3** the interface documents' stale numbers (`decimals()` 6, units of 10^6, a 1 % leg, `V` = 1,000
  MSK, `SeedTooSmall()` still promised as a revert, the least seed). These are rewritten together
  with this ADR's own interface text, in one commit, because both rewrite the same paragraphs.
* **#4** `closed_to_buys` was left closed by the last move. Past the fence it is a record of the last
  move, not the gate: the gate is asked live (Decision 5). This is part of the consensus commit.
* **#5** the CLI's refusal texts: an unseeded line is named as one, with what it holds and what it
  needs, and a short seed no longer advises `--msk 0`.
* **#6, the CLI's half** `model-show` prints no curve and no opening height for an unopened line.
* **N1** the seed preview counts what is already pledged. **N2** `model-evm-position` stopped dividing
  units by 10^6. **N3** the P-B3 refusal no longer says the MSK "would not come back". **N4** the
  owner's leg is named as the chain pays it (burned with no owner, the contributor's share).

Left, with the reason:

* **#1** (the model sink binding) has its own fence and its own lane.
* **#6, the RPC's half**, **#7** (`getPalwModelVersion`'s `tipDaa`), **#8** (`model-positions` JSON
  v1), **#9** (Debug strings on the wire) and **V1** (a retired line's empty `marketRefusal`) change
  answers this ADR does not otherwise touch, and each needs its own wire review.
* **#10**, the `model-evm-*` commands in the shipped CLI, is a release-build switch (`evm-send`), not
  code.
* **N5–N7** and **V2–V3** are deposit-claim UX, a bond-registration CLI, the retired-line policy
  (it needs a fence), trace replay and role display. None of them is this market's.
* The work branch's `evm-send` slippage floors are left: they need the EVM quote's semantics settled
  first, and #2 has only just settled them past the fence.

## 11. What is deliberately not decided

* **The height.** Nothing is armed; a network schedules the fence like any other (a flag day, every
  node on a build that carries it before the height).
* **A different `V` per network or per class.** One constant; a later one would be a new constant
  recorded in the rows it opens.
* **A seed with the registration's carrier** (Decision 4): a new object version, not taken.

## 12. Number hygiene

0160 is resident on `rcore/cap-spec` and 0161 is held by the capacity lane's untracked "a claim is not
an emission" (`wt-cap-s1`); this is ADR-0162.

## 2026-10-16 改定(ユーザー決定)— genesis・年次フォークのモデルは Position 化できない

**Model Position を持てるのは、追加モデル(add model)だけである。** 追加モデルとは、RFC-0004 以降の登録経路で後から登録されたモデル(ADR-0179 の登録階層、0.3)を指す。
genesis のモデルと、年次フォークで指定されたモデル(`ApprovedModels`、ADR-0179 の 1.0 階層、RFC-0019 のカタログ)は、Position 化できない。

**理由:先行者利益を生むため。** genesis とフォークのモデルは、誰かが競って勝ち取ったモデルではなく、プロトコルが正規モデルとして指定するものである。
その Position を売ると、最初に買った者が、正規モデルの指定そのものから先行者利益を得る。これは、validator の先行者利益や運営母体を無くすという方針([ADR-0180](0180-palw-only-consensus-no-bft-validators.md))にも反する。

**規則**(tier fence `palw_model_tier_v1` 以後のみ。fence より前と、登録モデルは従来どおり byte 同一)
- `ApprovedModels` のモデル(genesis assembly、genesis カタログ、有効な年次 edition)への**買いを拒否する**:`ModelPositionOnApprovedModel`。
- 同じモデルに**新しい line を作ることも拒否する**(line は Position の市場である)。
- 既に保有している units の売却は止めない(拒否するのは新規の参加だけ)。

**実装**
- `TransitionBuilder::refuse_position_on_approved_model_v1`(`consensus/core/src/palw_state_v2/transition_builder.rs`)。
- 呼び出しは `model_buy_v1`(carrier と EVM の両経路が通る唯一の買い)と `apply_model_line_founded`。
- 試験:`palw_state_v2::tests::model_market_virtual::genesis_and_fork_models_cannot_be_positioned_only_added_models_can`
  - fence 以後、genesis モデルの買いと line の新設は拒否され、追加モデルは取引できる。
  - fence より前は、genesis モデルも従来どおり取引できる。
~~~~~~~~
<!-- ADR_SOURCE_END:0162-the-pair-opens-on-a-virtual-reserve.md -->

</details>

