# ADR-0094 — A seed is paid in as many transactions as it takes

* Status: PROPOSED 2026-09-07; IMPLEMENTED 2026-09-07 (§8)
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2's last clause ("One seed a line: a second is refused") and Decision 5's row.
* Builds on: 0090 (the seed IS the reserve and never leaves), 0087 (the curve), 0091 (the reward
  buys the pair).
* Supersedes nothing.

## 0. The sentence this ADR is

A hundred thousand MSK does not fit in one post-quantum transaction, so the seed is allowed to
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

**Decision 3 — anyone may pay, and the row names who started it.** The sink takes MSK from any
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

A producer earning 1,400 MSK a block, seeding the least pair:

| | before | after |
|---|---|---|
| what one transaction can carry | 1 utxo (~1,400 MSK), and the seed is refused | 15 utxos (~21,000 MSK) |
| transactions to a 100,000 MSK pair | 14 consolidation rounds, then 1 seed | **5 seeds** |
| wall-clock at 240 s a block | ~1 hour of consolidation, then the seed | ~20 minutes |
| MSK locked before the market exists | 0 (nothing is paid until it all is) | each payment, on arrival |

The last row is the trade this ADR makes: a participant who stops halfway has locked what they
paid and has no market. That is the same bargain ADR-0090 already struck for a single seed — the
seed never comes back — applied to each instalment. It is stated on the CLI and on the site before
the first payment, because a rule that surprises is worse than a rule that costs.

## 5. Security — the four principles, checked

* **Nothing is minted.** A pledge is a sink output like any seed; the reserve at opening is exactly
  the sum of the sink outputs that fed it. `seed + Σ buys + Σ slices = reserve + Σ sells + burned +
  legs` (ADR-0090 P2, ADR-0091 B2) holds with the sum in place of the single seed.
* **Nothing is withdrawn.** There is no object that pays a pledge back, opened or not. A pledge on a
  line that never opens is burned MSK, which is what paying into an `OP_RETURN` means.
* **No market before the floor.** A buy or a sell on an unopened row is refused by the fold, and
  the row carries no positions to sell — the supply does not exist until the market does.
* **A user-input fault is a revert, a chain fault is a refusal, never a block fault.** Unchanged.

Attacks considered:

| | threat | why it is not one |
|---|---|---|
| A1 | a stranger pledges 1 sompi to a line to be named its seeder | `seeded_by` names the FIRST payer, and the first payer is the one who chose to start; a later sompi names nobody |
| A2 | a griefer pledges to a line they dislike, to open it at a price they choose | opening at a HIGHER total is a higher first price and more locked MSK — they pay for the privilege, and the line's people keep every sompi of it |
| A3 | a pledge sits unopened forever, MSK locked | the same as any under-floor payment into the sink, which ADR-0090 already burns; the CLI states it before the first payment |
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
