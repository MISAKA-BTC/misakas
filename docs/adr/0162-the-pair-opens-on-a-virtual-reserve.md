# ADR-0162 — The pair opens on a virtual reserve

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
* `kaspa-evm/src/model_market.rs`, `executor.rs` — the window and the writer; `rpc/service` — the
  answer; `misaka-cli` — `model-show`, `model-seed`, `model-buy`/`model-sell`, `model-evm-seed`,
  `misaka model market open`; `contracts/misaka-model/` — the interfaces; `web/misaka-options/` — the
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
