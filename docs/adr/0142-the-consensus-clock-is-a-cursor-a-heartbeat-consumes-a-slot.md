# ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the clock may not postpone it

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
