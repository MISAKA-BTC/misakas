# ADR-0090 — The pair is seeded with real MSK, locked for good, and a position is whole

* Status: PROPOSED 2026-09-05; IMPLEMENTED 2026-09-05 on `palw-adr0088-0089-impl` (§8)
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decisions 1, 2,
  3 (a third move), 4 (no leg on the seed) and 8; [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decisions 3, 5 and 6 (a third action, a third event).
* Builds on: 0087 (the market), 0088 (the line the market is keyed by), 0089 (the EVM's hand).
* Supersedes nothing.

> **Amended (2026-09-06, design first).** [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md): the reserve grows by a third source — five percent of every model block's escrowed worker reward, at the claim's `Final`, with the positions the curve gives up retired — so Decision 2's floor tightens to `K / (supply − retired) ≥ seed` and §5's identity gains the slices as a source. Nothing here loosens: no seed leaves, no holder is paid.

## 0. The sentence this ADR is

A model's market is a pair the model's people make by locking at least one hundred thousand MSK
into it, MSK that no one — not the one who locked it, not the chain — ever pays out again; the
pair holds five hundred thousand whole positions, no fraction of one, and trades on a curve whose
product never falls, so buying raises the price and the seed stays under everything.

## 1. What the operator asked, in the operator's words

Adding a model to the site (misakaoptions.com) must make a *model–MSK pair*. To make it the user
prepares at least 100,000 MSK, and that MSK is fully locked: the user who made the pair cannot
withdraw it. The model is then approved as one that earns mining rewards. After that the pair
trades on the site as model positions and the price rises as people buy. The positions number
500,000, whole numbers only, fixed at issue. People buy them and the price goes up. That is the
goal, and this ADR is the rule set that makes each clause true.

## 2. What ADR-0087 had, and where it did not match

ADR-0087 opened every market by itself on the class's first buy, with **no MSK** in it and a
*virtual* reserve `V = 1,000 MSK` that set the first price and the curve's steepness; its supply
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
never leaves.** `PALW_MODEL_SEED_MIN_SOMPI_V1 = 100,000 MSK`. A line's market does not exist
until a seed of at least that reaches its sink. The whole seed becomes `msk_reserve` — no burn, no
leg, no position minted to anyone: the seeder holds nothing. There is no virtual reserve
(`PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1` is kept at zero so old arithmetic adds nothing). The curve
is `msk_reserve × position_units = K` with `K` taken from the row at every move — a buy adds the
net leg to the reserve and releases `units − ⌈K / reserve′⌉` positions; a sell returns positions
and pays `reserve − ⌈K / units′⌉`, never more than the reserve — so the product never falls. Hence
the invariant the operator asked for in words: **with every position back in the curve the
product puts the reserve at the seed or above; the reserve never falls under the seed.** No object
pays a reserve out but a holder's sell, and a sell cannot reach the seed. The first price is
`seed / 500,000` (0.2 MSK at the least seed). One seed a line: a second is refused. A line whose
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
MSK leg; nothing on the seed, which is liquidity, not a trade.

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

From a market seeded with the least seed (100,000 MSK; `K = 10^13 × 500,000`):

| move | MSK leg | positions | reserve after | price after |
|---|---|---|---|---|
| seed | 100,000 in, no fee | 500,000 in the curve | 100,000 | 0.2 |
| buy 1,000 MSK | 50 burned, 10 to the owner, 940 in | 4,656 out | 100,940 | 0.20377757 |
| buy 1,000 MSK | 50, 10, 940 | 4,570 out | 101,880 | 0.20759045 |
| sell 9,226 | 1,879.88976 gross; 1,767.0963744 net | 500,000 back | 100,000.11024 | 0.20000022 |

Selling everything ever bought leaves the reserve **above** the seed by the rounding the curve
keeps: the seed is not withdrawable by trading either. A buy of 0.1 MSK releases nothing; 0.22 MSK
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
(`model-seed`, `model-evm-seed`, the seed on `model-show`), `contracts/misaka-model/` (the
interfaces and README), and the drill (`scripts/misaka-palw-model-market-devnet-e2e.sh`: no market
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
