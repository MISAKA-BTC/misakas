# ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the clock may not postpone it

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Consensus
change, fenced.** It replaces the heartbeat lane's admissibility rule. It held the 6,301 rollout.

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

**C. A consensus clock cursor.** ADOPTED. The chain carries the next slot boundary as state.
A heartbeat consumes a slot and advances the cursor; every other lane leaves it untouched. No walk,
because the cursor is already at hand wherever the PALW state is; no timestamp on the clock, because
the cursor advances in whole slots.

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

called by header validation, by `heartbeat_adapt_block_template`, by the miner's wait, and by the
tests. A workspace guard already fails the build if the superseded entry points are called outside
their module (`only_this_module_may_ask_the_pre_fence_slot_rule`); it gains the v1 slot rule.

## 6. What must be proved, as properties and not as examples

The bug that produced this ADR is invisible to example tests: every example passed. These are
properties over generated sequences.

1. **For any sequence of blocks that do not advance the clock, at any spacing — 1 s, 3.3 s, 20 s,
   119 s — the cursor does not move.** This is §2 and it is the one that failed.
2. **Past the cursor, a heartbeat template is admissible.** No liveness hole between "the slot opened"
   and "a beat may be built".
3. **Validation and the template builder agree** on every input. Same function, asserted anyway.
4. **Only a heartbeat advances the cursor**, over every lane and every fence combination.
5. **A reorg restores the cursor exactly**, and a delta revert equals a fresh walk of the winning
   branch — the equality every other rooted field is already held to.
6. **A producer cannot lock the clock with a future timestamp**: the cursor moves by whole slots and
   the drift rule bounds how many.
7. **One beat normalises a long outage**: after any silence, a single heartbeat leaves the cursor at
   the first boundary after it, with no backlog to consume.

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
the 6,301 bundle. They arm together (`Params::set_palw_single_lottery`), and arming either without
this rule is the liveness failure in §1.
