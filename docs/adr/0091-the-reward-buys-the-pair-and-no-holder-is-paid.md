# ADR-0091 — The reward buys the pair, and no holder is paid

* Status: PROPOSED 2026-09-06 (design first, at the operator's word; the implementation follows
  on `palw-adr0088-0089-impl` and is recorded in §8 when it lands)
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 3 (a
  move that is the chain's own, on no lane), Decision 4 (no leg on it) and Decision 8 (two more
  words on the row); [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  §10's open item "a per-line service fee … a share of a claim's escrowed reward to the owner"
  (decided: no — the reward's share goes to the pair, not to a person); [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decision 2 (the AMM window's `market()` gains two words); [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the reserve now grows by the reward as well as by buys) and §5's conservation
  identity (a third source of MSK).
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the worker reward is a
  carve of the subsidy, ESCROWED at the accepting block and named as a payout at `Final`, never
  minted before), 0088 Decision 4 (a claim is attributed to the version whose root it named),
  0090 (the seeded pair, the whole position, the product that never falls).
* Supersedes nothing.

## 0. The sentence this ADR is

Five percent of the worker reward a block earns with a model buys positions from that model's
pair; the chain keeps what it buys, for good; the other ninety-five percent is paid to the miner
as it always was; and **no holder is ever handed anything** — the only thing mining does for a
holder is raise the price the curve will pay them when they sell.

## 1. What the operator asked, in the operator's words

> 今の設計はそのモデルを使用したマイナー報酬の一部をモデルの position 保有者に分配する仕組みだけど、
> それを分配からマイナー報酬でモデルと MSK のペア流動性から MSK で買い上げるようにして価格が上がる
> 仕組みに変更して　そっちの方が証券などに近くなくなる
>
> またそのモデルのマイナー報酬全体のうちの 5% で買い上げるとすること　95% はマイナーに支払うこと
>
> ADR から記述して

The design had been read as "a part of the mining reward from using the model is *distributed* to
the model's position holders". Change that: instead of a distribution, the mining reward **buys**
from the model–MSK pair's liquidity with MSK, so that the price rises — that is further from a
security. Of the model's whole mining reward, **5 % buys**; **95 % is paid to the miner**. Write
the ADR first.

## 2. What exists, and where the distribution did not fit

Nothing in ADR-0087–0090 distributes a reward to a holder; the fold has no such move. What exists
is the channel the operator's first reading assumed and the one this ADR builds on:

* **ADR-0042 Decision 10 — the worker reward is an escrow.** A chain block's attempt earns the
  worker carve of its own subsidy (`⌊subsidy × worker_carve_permille / 1000⌋`, 620 ‰ on every
  shipped preset, which is the network's whole §F worker BASE share — `subsidy_worker_base_bps =
  6200`). The child's coinbase WITHHOLDS that carve from the block's reward output; the fold names
  it as a `PalwPayoutV2` to the bond's payout payload only when the claim reaches `Final`, and the
  next block's coinbase pays it. A claim that voids pays nobody — the carve was never minted.
  Fees are not part of the carve: they are paid at once, to the miner, as the block's own.
* **ADR-0090 Decision 2 — the pair.** A line's market is a reserve of real MSK (the seed and
  every buy's net leg) against 500,000 whole positions on `reserve × units = K`, the product taken
  from the row at every move so it never falls. MSK that enters the reserve leaves it only through
  a holder's sell, at the curve's price, less the legs.

A distribution would have taken MSK out of the escrow and handed it, pro rata, to every holder —
income that arrives without a sale, from the work of others, which is exactly the shape a
security has. The operator withdrew it before it was built. What replaces it needs no new channel:
the reward is already withheld and only *named* at `Final`, so a slice of it that is never named
as a payout is a slice that was never minted; and the pair already turns MSK that enters it into a
higher price for every position equally. Put the slice into the pair and nothing is distributed:
nothing is owed to a holder, nothing is paid to one, and a holder who wants the value sells a
position back to the curve like anyone else.

## 3. Decisions

**Decision 1 — the slice is five percent of the claim's escrowed worker reward, and the escrow
prices it.** `PALW_MODEL_BUYBACK_PERMILLE_V1 = 50`; `slice = ⌊escrowed_reward × 50 / 1000⌋`. The
escrow is the worker carve snapshotted when the claim was accepted (ADR-0042 Decision 10, the E3
rule: a fact about the block that produced the claim), so the slice is priced there too, with no
new field and no new collection. On every shipped preset the carve IS the miner's whole subsidy
reward (620 ‰ against a 6200 bps base share), so "5 % of the model's whole mining reward" is
exactly this; fees, which the carve never held, stay the block's and are untouched. A network that
set its carve under its base share would be choosing that the difference is hash reward, not model
reward — the slice follows the carve. The number is the operator's, set when the fences are armed;
the tests carry 50 ‰.

**Decision 2 — at `Final` the slice buys from the line's pair, and the chain keeps what it
buys.** The line is the one the claim was attributed to when it was accepted (ADR-0088
Decision 4, the root the fold recorded in `claim_roots`), resolved through EVERY version of the
class's lines, in force or superseded — attribution was a fact of the accepting block and does
not lapse with a version. If that line's market exists and is open to buys, the fold performs the
chain's own move on the row: `reserve′ = reserve + slice`; `units′ = min(units, ⌈K / reserve′⌉)`
with `K` the row's `reserve × units`; `retired_units += units − units′`;
`buyback_sompi += slice`; `model_moves += 1`. **No leg and no burn** — the whole slice is
liquidity, as the seed was — and **no holder**: the positions the curve gives up are *retired*,
held by the chain for good; no object sells them, the EVM shows no balance for them, and the
supply stays 500,000 with them counted (Decision 4). The payout the fold names for the claim is
`escrowed_reward − slice`. A slice under one position's price gives up no position and still
raises the price — the reserve rose and the product with it — which at the least seed and
testnet-11's subsidy is every block (§4); the price rises by the reserve's growth, and positions
retire when a single slice is worth one.

**Decision 3 — where there is no pair, the miner is paid in full; nothing is burned "for the
model".** No market on the line (unseeded), a market closed to buys (the class left `Active`,
ADR-0087 Decision 7), a claim the registry attributed to no version, or the fences below the
claim: the fold names the whole escrow as before. The seed is what switches the buyback on; a
model without a pair is a model whose miners keep everything, and a miner's reward never depends
on which line of a class it ran except through the pair that line's people made.

**Decision 4 — retired positions are the chain's and never move.** ADR-0087's M1 becomes
`position_units + Σ holders' units + retired_units = supply` for every line. There is no object
whose effect is a change of `retired_units` but this move; the EVM facade's `totalSupply()` still
answers the supply and its `balanceOf` answers no address for them. With every holder sold out,
the curve holds `supply − retired` positions and the reserve is at or above
`K / (supply − retired) ≥ seed`: the seed floor of ADR-0090 P1 holds and tightens.

**Decision 5 — nothing else changes at `Final`, and nothing changes for a claim that voids.**
The escrow withheld at the accepting block is the same number; the payout is smaller by the
slice; the coinbase renders the queue as before. A voided or retired claim buys nothing and pays
nobody (don't-mint, as before). A merged blue's attempt (escrow 0, ADR-0058) and a free-prompt
claim (escrow 0, ADR-0044) have no slice.

**Decision 6 — what a participant reads.** The row gains `buyback_sompi` (MSK the reward has put
into the curve, cumulative) and `retired_units` (positions the chain holds for good).
`getPalwModelMarket` carries both (wire version 4; proto fields 19–20); `misaka palw model-show`
prints them; the AMM window's `market()` gains two words at its END — `buybackSompi`,
`retiredUnits` — so every earlier word keeps its offset; the site shows "bought by mining" and
"retired" on every market and states the rule on the add-model page; a claim's payout row shows
the reduced amount. No event: the move is the fold's, on no lane, and the row is its record.

**Decision 7 — same fences, no new one.** Under `palw_model_market` (the row) and
`palw_model_lines` (the attribution); both are `None` on every shipped preset and the drill arms
them. Where the fences are armed later, this is the move that runs at every `Final`.

**Decision 8 — the emission identity.** ADR-0042 Decision 10's "never an addition to the
schedule" holds unchanged: what the accepting block's coinbase withheld equals the payout named
at `Final` plus the slice; the slice is never minted; what a later sell can mint from the reserve
is bounded by the reserve; so, over a pair's life,
`seed + Σ buys' net + Σ slices = reserve + Σ sells' gross` and every sompi ever minted from the
market was first withheld or sunk.

## 4. The arithmetic, worked

From a market seeded with the least seed (100,000 MSK, `K = 10^13 × 500,000` sompi·positions), on
testnet-11's pre-deflationary subsidy of 370,468,345 sompi a block and a 620 ‰ carve:

| | sompi | MSK |
|---|---|---|
| the block's escrowed worker reward | 229,690,373 | 2.29690373 |
| the slice (5 %) | 11,484,518 | 0.11484518 |
| named for the miner at `Final` (95 %) | 218,205,855 | 2.18205855 |

| after | reserve (sompi) | positions in the curve | retired | price (sompi) | vs 0.2 MSK |
|---|---|---|---|---|---|
| the seed | 10,000,000,000,000 | 500,000 | 0 | 20,000,000 | — |
| one block's `Final` | 10,000,011,484,518 | 500,000 | 0 | 20,000,022 | +0.0001 % |
| a day of such blocks (720) | 10,008,268,852,960 | 500,000 | 0 | 20,016,537 | +0.083 % |
| a month (21,600) | 10,248,065,588,800 | 500,000 | 0 | 20,496,131 | +2.48 % |
| a year (262,800) | 13,018,131,330,400 | 500,000 | 0 | 26,036,262 | +30.2 % |

A slice of 0.11 MSK is under the 0.2 MSK a position costs, so at this size the curve gives up no
position: the reserve rises, the product rises, the price rises. On a network whose subsidy were
100 MSK a block (escrow 62 MSK, slice 3.1 MSK) one block retires 15 positions and lifts the price
to 20,001,220. And the holder of ADR-0090 §4's first buy (4,656 positions for 1,000 MSK at
20,377,757 after) who holds through a year of such blocks sells them all at 26,470,758 a position:
1,221.00166948 MSK gross, 1,147.74156932 MSK net after the legs — and the reserve after that sale
is 129,900.31 MSK on 500,000 positions, above the seed by everything the reward put in. Nothing
was paid to that holder while they held; the price was.

## 5. Security — the four principles, checked

* **Nothing is minted.** The slice is a part of an escrow the coinbase already withheld; it
  becomes a reserve entry, never an output; the identity of Decision 8 is a test (B2). The four
  ways MSK reaches a market — seed, buy, slice, and nothing else — are each a sink or a withhold.
* **Nothing is distributed.** No object moves MSK to a holder by virtue of holding; the payout
  table of the fold has one payee for the reward, the bond's payload, and its amount is smaller.
  The word "holder" does not appear in the move (B4).
* **Nobody steers it.** The line is the claim's (attribution at accept, the same rule the court
  replays against); the amount is the escrow's; the miner cannot pick a richer pair, the owner
  cannot take a fee from it (ADR-0088 §10's service fee is decided *no*), the seeder gets nothing.
  Two lines of one class sharing a root resolve to the first in the registry's order, as
  attribution already does (ADR-0088 A10).
* **Determinism and reorgs.** The move is a fold write with delta entries (`ModelMarket`) like a
  buy; the reorg-equivalence suite covers it because it runs at `Final`, which that suite already
  drives through both a chain and a rewind (B6).

Attacks considered:

| | threat | why it is not one |
|---|---|---|
| A1 | a miner mines a line to pump positions they hold | they forgo 5 % of their reward to raise a price they share with every holder — identical to buying 5 % of their reward's worth from the curve, which they could do anyway, minus the legs they would have paid; there is no lever here that a buy did not already give them |
| A2 | a line's people seed a pair to capture the whole class's reward | the slice follows the LINE the claim ran, not the class; a miner who runs another line's version feeds that line's pair (or none) |
| A3 | a frozen class's pair keeps taking reward | a frozen class produces no chain blocks and its market is closed to buys; the claims accepted before the freeze pay their miners in full (Decision 3) |
| A4 | a reward slice makes the reserve fall under the seed | it adds; the product never falls; the seed floor tightens (Decision 4) |
| A5 | a node that finalizes a claim without the lines fence pays the whole escrow, another with it pays 95 % | a `claim_roots` entry exists only where the fence was armed at accept, so both nodes read the same record and name the same amount; the fence enters the fingerprint (ADR-0088 Decision 11) |

## 6. Invariants the tests must hold

* **B1 (the split).** With a seeded, open pair on the claim's line, `Final` names
  `escrow − ⌊escrow × 50 / 1000⌋` for the miner and adds exactly `⌊escrow × 50 / 1000⌋` to the
  reserve; `buyback_sompi` and `model_moves` advance. `palw_state_v2::tests::model_market::the_reward_buys_the_pair_at_final_and_the_miner_is_paid_the_rest`.
* **B2 (conservation).** ADR-0090 P2 with the slice as a source: `seed + paid_in + Σ slices =
  reserve + paid_out + burned + legs` at every state the fold reaches. `model_market::invariants`.
* **B3 (no pair, no slice).** An unseeded line, a closed market, an unattributed claim: the whole
  escrow is named; the market (if any) is unchanged. `…_and_a_miner_of_an_unseeded_line_is_paid_in_full`.
* **B4 (no holder).** After the move no position row changed and no payout row but the miner's
  was written. Asserted in B1.
* **B5 (retired, M1).** With a slice worth positions, `retired_units` rises by exactly the
  positions the curve gave up, `position_units + Σ holders + retired_units = supply`, and no
  object can move them. `…_and_what_the_reward_buys_is_retired`.
* **B6 (arithmetic).** `palw_model_market_v1::tests::the_reward_slice_is_worked_as_the_adr_table`
  pins §4's numbers; `the_product_never_falls_under_the_reward_too` extends P1 to the new move.
* **B7 (the window).** `market()`'s two new words are the row's, and every earlier word keeps its
  offset. `kaspa_evm::model_market::tests::adr0091_market_words`.
* **B8 (the drill).** On the armed devnet the seeded founding line's `buyback_sompi` rises on
  every node with no buy, `retired_units` stays consistent with M1, and every node agrees.

## 7. Order of work

1. This text; the README rows; banners on 0087, 0089 and 0090.
2. The constant, the row's two fields, the buyback quote and its goldens (B6).
3. The fold: the attribution lookup that ignores in-force, the move at `Final`, the smaller
   payout (B1–B5); the consistency re-derivation of M1 with `retired_units`.
4. RPC (wire v4, proto 19–20, both converters, the service), CLI, the AMM window's two words and
   the Solidity interface (B7), the site's row, market panel and add-model text.
5. The devnet drill: a check that the reward alone moved the reserve (B8).
6. Arming on testnet-11 with ADR-0090's release (§7 item 6 there): the same three fences, one
   fingerprint.

## 8. Implementation record (2026-09-06, `palw-adr0088-0089-impl`)

Items 1–5 landed the same day; item 6 is the operator's release, shared with ADR-0090's.

* `consensus/core/src/palw_model_market_v1.rs` — `PALW_MODEL_BUYBACK_PERMILLE_V1 = 50`, the row's
  `buyback_sompi` and `retired_units`, `palw_model_buyback_slice_v1`, `palw_model_buyback_quote_v1`
  (the whole slice into the reserve, what the curve gives up retired, `None` where no pair takes
  it), and the sell quote's new bound — the curve holds at most `supply − retired`. Two tests with
  §4's numbers: `the_reward_slice_is_worked_as_the_adr_table` (B6),
  `the_product_never_falls_under_the_reward_too` (B6/P1, buys and buybacks interleaved).
* `consensus/core/src/palw_state_v2.rs` — `PalwChainStateV2::model_line_of_root` (attribution that
  does not lapse with a version), `TransitionBuilder::model_buyback_at_final`, and `finalize_claim`
  naming `escrow − slice` for the miner. M1 and M2 in the market tests now count the retired
  positions and the slices. Three tests: `the_reward_buys_the_pair_at_final_and_the_miner_is_paid_the_rest`
  (B1/B4), `a_miner_of_an_unseeded_line_is_paid_in_full` (B3),
  `what_the_reward_buys_is_retired_and_the_supply_stays_whole` (B5).
* RPC: `buybackSompi` / `retiredUnits` on `getPalwModelMarket` (wire version 4, backward-loading;
  proto fields 19–20; both gRPC converters; the service reads them off the row). CLI: `model-show`
  prints "mining bought" with the positions retired, and the JSON carries both.
* `kaspa-evm/src/model_market.rs` — `market()` answers eleven words, the two appended so every
  earlier offset holds; an unknown line is eleven zeros. `adr0091_market_words` (B7).
  `contracts/misaka-model/IMisakaModelAMM.sol` and the README say the same.
* `web/misaka-options/` — `curve.buybackSlice` / `curve.buyback` (the chain's arithmetic, for the
  self-test and the mock only), the two words read from both lanes (a pre-ADR-0091 node serves
  neither and the site shows a dash, never a zero), "Bought by mining" on the market bar, the line
  page and the leaderboard (sortable), the docs page's "Mining buys the pair" section and fee-table
  row, the add-model step 3 text, nine self-test checks against these goldens, and a mock world
  whose seeded pairs have been mined on.
* `scripts/misaka-palw-model-market-devnet-e2e.sh` — step 1g/1h: with no trade in flight the
  reward alone moves the pair, and the reserve is exactly `seed + buyback` with the price not
  falling (B8). The node-agreement row now compares `traded = reserve − buyback`, which the
  reward's own move leaves alone; the reserve, the buyback and the retired count are logged for
  the reader.

**What the drill's own runs taught, which was not about the market (2026-09-06).** Two defects in
the drill itself surfaced the moment a step depended on the reward being *paid*, and both had been
there for ten runs:

* **`misaka evm wallet` lives behind a non-default feature** (`--features evm-send`). A CLI built
  without it has no such subcommand, and run 10 died three steps in — six nodes up, the founding
  line already passed — on clap's "unrecognized subcommand", which names neither the feature nor
  the build. The script now asks the binary for the subcommand before it starts anything, and its
  header carries both cargo lines.
* **The nodes never passed `--palw-panel`, so no claim had ever reached `Final`.** Nothing verified
  material, signed a receipt or submitted a quorum; `final_claims=0` while `unresolved` climbed.
  Every escrow this drill ever withheld was released to nobody, and ten green runs said nothing
  about the reward path because they never walked it. With the flag on (run 11) the panels file
  receipts within two minutes and license claims steadily.
* **A claim is `Final` 180 DAA after it is accepted** (bind 40 + receipt 40 + challenge 100), and
  this devnet grows about two DAA a minute — an hour and a half before the first reward is paid to
  anyone, against every other step's few minutes. The check therefore runs LAST, after the rest of
  the drill has grown the chain, on its own clock (`BUYBACK_WAIT`, default 5,400 s). Run 11 with
  the check placed early failed it for exactly this reason and passed the other thirteen.

## 9. What is deliberately not decided

* The permille. 50 is the operator's number of 2026-09-06; it is a constant the operator sets
  when the fences are armed.
* Whether small slices should accrue in a per-line pot until they buy a whole position. Not here:
  the product rule already turns every sompi into price, and a pot is a balance somebody would
  ask to withdraw.
* A service fee to the line's owner from the reward (ADR-0088 §10). Decided *no* for the reward;
  the owner's income is the 1 % leg on trades (ADR-0088 Decision 8), which this ADR leaves alone.
* Whether the slice should be taken from a network whose carve is under its base share
  differently. The slice follows the carve; a network that wants it to follow the base share sets
  its carve to the base share, as every shipped preset does.

## 10. Number hygiene

0091 was the README's next free number on 2026-09-06 (0090 was taken on 2026-09-05 by this
branch's own ADR-0090); the next free number is 0092.
