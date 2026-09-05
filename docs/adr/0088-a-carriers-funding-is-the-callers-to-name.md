# ADR-0088: A carrier's funding is the caller's to name, and a day's budget is a share of the ceiling

**Status:** ACCEPTED (2026-09-05). Implemented in this commit; see §6.
**Consensus-inert by construction:** no consensus object, rule, parameter or fingerprint changes.
Both decisions are about HOST-side tools — the wallet CLI and the free-prompt gateway — asking the
chain's own numbers a question they were not the answer to.
**Builds on:** ADR-0077 SA-1/SA-8 (the gateway's binding limits: one slot, a bounded queue, a
budget tied to exposure), ADR-0079 Decision 4 (the gateway holds no key, so something else signs
and pays for every carrier), ADR-0044 (the free-prompt lane).
**Amends:** nothing. It corrects two implementations against decisions that were already right.

## 1. What was measured

A pool slot ran the free-prompt lane for its own owner: a chat in MISAKA Studio, answered by the
slot's gateway, committed under the slot's bond, and carried to the chain by a submitter holding
the slot's seed. Two things stopped it, neither of them the chain.

**(a) The wallet cannot fund a second carrier inside one block interval.** `misaka wallet send`
selects over `getUtxosByAddressPage`, which answers from the UTXO SET. Asked twice before a block
it selects the same input, rebuilds the same transaction, and the node answers:

```
Rejected transaction c552440e…: transaction c552440e… is already in the mempool
```

On testnet-11 a block is twenty to forty minutes, so this is one carrier per block. The submitter
worked around it by chaining: it read the previous carrier's change output and passed it as the
next funding. That works until a node restarts — a mempool is not persisted — and the parent is
gone, at which point the child is refused by the node's own policy
(`mining/errors/src/mempool.rs`):

```
transaction e39e660e… is an orphan where orphan is disallowed
```

Three submission attempts were spent on that before the claim was renamed out of the queue. The
claim was never at fault.

**(b) The gateway's day-budget charges every open claim twice.** `PublicJobBudget::daily_budget`
was `room_sompi × permille / 1000`, and `room = ceiling − reserved`. Every claim the gateway
commits is reserved by the chain — so it comes out of the room — and is ALSO added to the
gateway's own `spent_sompi` for the window. The condition to admit claim N+1 is therefore

```
N·claim + claim ≤ ceiling − N·claim      ⟺      (2N+1)·claim ≤ ceiling
```

A bond deliberately sized for four concurrent claims stops at two. Measured on pool slot-04, whose
bond was registered with collateral for exactly four:

```
gateway : the public-job budget for this window is spent (66,305,440 of 99,265,460 sompi)
chain   : bondExposureCeiling 132,610,880 | bondReservedExposure 33,383,960 | available: true
```

The chain said the bond had room for three more. The gateway refused because it had subtracted the
same two claims a second time. `spent_sompi` is process-local, so restarting the gateway cleared
it — which is the plainest possible evidence that this was never a chain rule.

## 2. Why neither is the chain's constraint

Stated because both failures read like consensus refusing something, and an operator who believes
that changes the wrong thing:

* **The orphan refusal is a NODE policy**, and a correct one: a transaction whose parent nothing
  knows cannot be validated. What was wrong was building a spend that depends on a mempool
  surviving, not the node's refusal to accept it afterwards.
* **The budget is the gateway's own.** Its check runs in `misaka-palw-gateway`, against its own
  counter, in its own process. Consensus does not read `spent_sompi` and has no equivalent; the
  chain's guard is `has_exposure_room`, which the gateway also consults and which was answering
  correctly the whole time.

## 3. Decision 1 — the wallet may be told which output to spend, and may spend its own pending change

`misaka wallet send` gains two flags. Neither changes what a transaction IS; both change which
inputs the wallet is allowed to consider.

**`--funding-outpoint <txid>:<index>`** spends exactly that output and selects nothing. It is for a
caller that staged the output itself and knows which one this transaction must consume — a fee
chain, a carrier's change. The output is looked for among everything the address holds, confirmed
AND pending, because the whole point of naming one is that selection could not have found it. A
named outpoint the node reports as locked bond collateral is refused with the same guard
largest-first selection has: naming is not a way around M1-3.

**`--spend-unconfirmed`** lets ordinary selection also consider the wallet's own outputs that are
in a mempool and not yet in a block. Off by default, because an input whose parent is still
pending is refused if that parent is ever evicted, and a wallet that reached for one unasked would
turn a rare failure into a surprising one. Ignored under `--coinbase-only`, where a pending output
is by definition not settled coinbase.

Both are served by one reader, `wallet::pending_outputs`, over
`getMempoolEntriesByAddresses`. Two things it must get right:

* an output another pending transaction already spends is not offered — a mempool holding both a
  parent and its spender would otherwise hand out the parent's output twice;
* a transaction with no verbose data is skipped rather than guessed at: the outpoint needs the
  transaction's id, and deriving one from a partial reply would be inventing an input.

The entries are marked mature (a non-coinbase output has no maturity floor) and never bonded (a
bond consensus locks is registered, and a registration is not in the mempool). `block_daa_score` is
zero rather than the virtual score because nothing reads it: the signature covers the amount and
the script.

**What this replaces.** A submitter no longer chains outpoints by hand. It asks the wallet for a
carrier per job — with `--spend-unconfirmed` when it needs a second one inside a block — and a node
restart costs it a retry rather than a queue.

## 4. Decision 2 — the day's budget is a share of the CEILING

`ChainFacts` carries `bond_exposure_ceiling_sompi` beside `exposure_room_sompi`, and
`PublicJobBudget::daily_budget` is a share of the ceiling. The room keeps its own separate job,
unchanged: `claim_sompi > room_sompi` is still refused at the entrance (ADR-0077 SA-7), because
that is the question the room answers — *may this claim fit right now*. The ceiling answers the
other one the budget was always asking — *how much of this bond may public jobs use in a day*.

An operator who pins the room by hand (`--bond-exposure-room-sompi`) with no chain reading to
correct it has told the gateway what this bond may hold; there the room is the only ceiling there
is, and the budget uses it. That case is unchanged, which is why the existing SA-1/SA-8 test still
passes untouched.

## 5. What this does not change

* No consensus object, rule, parameter, activation or fingerprint. Both binaries are host tools.
* The gateway still holds no key (ADR-0079 D4) and still cannot submit; the carrier is still
  signed and paid for by a separate process.
* The room guard, the single job slot, the in-flight queue, `--answer-never-commit` and the
  per-source rate are all as ADR-0077 left them.
* A wallet with neither new flag behaves exactly as before.

## 6. Implementation record

| what | where |
| --- | --- |
| `pending_outputs` over `getMempoolEntriesByAddresses` | `misaka-cli/src/wallet.rs` |
| `--funding-outpoint`, `--spend-unconfirmed` | `misaka-cli/src/wallet.rs`, `misaka-cli/src/main.rs` |
| `bond_exposure_ceiling_sompi` carried from `getPalwProducerFacts` | `misaka-palw-gateway/src/chain.rs` |
| `ExposurePrice::ceiling_sompi`; `daily_budget` off the ceiling | `misaka-palw-gateway/src/main.rs` |
| regression: a four-claim bond admits four | `misaka-palw-gateway/src/main.rs` tests |

The regression test was confirmed to FAIL against the old arithmetic before it was kept: with
`daily_budget` reading the room, claim 3 of 4 is refused and the test panics on that line. A test
that passes either way would have recorded nothing.

## 7. What is still open

* **The submitter's chain is only mitigated, not removed, until it is rewritten against Decision 1.**
  It now verifies that a chained parent is still known to the node and falls back to the wallet
  when it is not; the chaining itself should go once the deployed CLI carries these flags.
* **A pending input is still a bet on the mempool.** `--spend-unconfirmed` narrows the window in
  which a caller must wait for a block; it does not make an unconfirmed parent durable. A carrier
  built on one and then orphaned must be rebuilt, and the caller must expect that.
