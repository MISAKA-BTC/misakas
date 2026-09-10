# ADR-0098 — The panel's coverage is a number, and a seat that found a lie files nothing else

* Status: PROPOSED 2026-09-10. **Decisions 1–3 IMPLEMENTED the same day** (§9): Decision 1 is a
  report over the draw the seats run; Decisions 2 and 3 change what a seat FILES, which is the
  seat's own duty — no consensus object, acceptance rule, fence, parameter or fingerprint moves,
  and a fleet takes them by an ordinary rolling rebuild. Decisions 4–6 are stated and not built.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 8
  (a seat replays `k` intervals drawn from the beacon; "a row unequal: the seat files nothing and
  opens a court at that leaf"; a sampled verdict convicts nobody),
  [0082](0082-the-close-is-flat-in-the-context.md) Decision 9 (the seat recomputes the state it
  resumes from), [0085](0085-the-close-is-assembled-from-what-was-served.md) Decision 4 (a seat's
  fault is prosecuted by the challenger's half), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  Decision 6 (a leaf once named), ADR-0065 Decision 4 (`Unavailable` abstains),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (a receipt is 4,772 bytes on the
  wire), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) Decision 4,
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the missing
  responder), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  Decision 5 (whose first row — a seat that holds a shard, measured first — this is).
* Answers: ADR-0081 Decision 8's question and its U-03 — "what fraction of a job's segments the
  panel collectively covers per claim and what a producer's expected cost is of a single forged
  link" — which ADR-0082 withdrew with ADR-0081 §3 before anyone took the number. The question was
  never about segments; it is about ADR-0077 Decision 8's live draw, and it is answered there.
* Amends nothing in consensus. **Restores** ADR-0077 Decision 8's "the seat files nothing" in the
  node, where three paths had lost it (§1.3).
* Supersedes nothing.

## 0. The sentence this ADR is

**On the 300-token claim testnet-11 licensed on 2026-09-05, the panel's sampling catches a
one-token lie 6.51 % of the time with five replaying seats and 3.96 % with three; a producer can
file 35 such claims before the odds that one is caught reach 90 %. And until this ADR, a seat
that did catch one could go on to certify the claim, or to accuse its producer of withholding the
data it had just replayed.** The number is now measured with the draw the seats run. A seat that
finds a lie files nothing else about the claim, and every fault it proves is recorded for the
court. A network whose seats cannot hold the model (ADR-0097) learns what the number costs it. A
panel stratified by shard keeps the coverage and multiplies the seats. Its licensing object fits
one transaction only up to eight shards. And it still needs a court that a seat holding one shard
can open.

## 1. What was measured

Read on `feat/adr-0097-model-fit` at `65ee18cd` (itself on `feat/adr-0096-everyday-lane`'s
`421cc92e`) on 2026-09-10. Every figure is what `misaka-palw-base0 --bin palw-seat-coverage`
printed on that tree (recorded in `docs/palw-seat-coverage-2026-09-10.md`); the binary is the
authority (ADR-0092 §5).

### 1.1 The draw, as it ships

* A free-prompt seat draws **`k = 4`** intervals (`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`,
  `palw_fp_interval_v1.rs`) with `palw_fp_interval_draw_v1(network, beacon, claim, seat_index, k,
  N)`: distinct within a seat, a keyed BLAKE2b over the four inputs reduced `u64 % N`, the seat
  index keyed in so seats draw independently. The beacon is the panel's anchor — the block at
  `bind_base_daa + anchor_delay`, which did not exist when the claim was committed.
* **`N`** is `max(1, ⌈(D − 1) / checkpoint_interval⌉)` for a claim of `D` decode tokens
  (`base0_fp_interval_count_for_v1`). The dense A16 and BASE-0 families checkpoint every decode
  call, so **`N = max(1, D − 1)`**; the hybrid checkpoints at `n_ctx`, so its claims have
  **`N = 1`** and every seat replays the whole job.
* The panel is **five seats with a quorum of three** (`PALW_V2_PANEL_SEATS` / `PALW_V2_PANEL_QUORUM`,
  both presets). The licensing object is carried as soon as three `Valid` receipts pool, and a
  seat's duty exists only while the claim is `PanelBound` — so **between three and five seats
  replay a claim before it is licensed**. Every table gives both.
* Before replaying interval `j` a seat recomputes the state that interval resumes from — the cache
  over every earlier position, from the prompt and the committed answer — and compares its root
  with the committed one (ADR-0082 Decision 9). So a lie that alters the cache is caught by a
  seat that draws ANY interval at or after it. A lie the cache never records — the selected token,
  or the last layer's work after its K/V rows — is caught only by the row replay of its own
  interval. That lie is also the one that changes the answer, so it is the forger's best, and its
  number is the floor under every other.

### 1.2 The numbers

**The forger's best lie** — one token, found only by a seat that draws its interval. Closed form
`1 − ((N − k)/N)^s`, and the draw itself over 5,000 beacons, which agrees on the mean and on every
interval within six binomial deviations (`palw_seat_coverage_v1`'s tests):

| D | N | s = 3 | s = 5 | forged claims before 90 % odds of one catch (s = 5) |
|---|---|---|---|---|
| 5 | 4 | 100 % | 100 % | 1 |
| 9 | 8 | 87.50 % | 96.88 % | 1 |
| 17 | 16 | 57.81 % | 76.27 % | 2 |
| 65 | 64 | 17.60 % | 27.58 % | 8 |
| 256 | 255 | 4.63 % | 7.60 % | 30 |
| **300** | **299** | **3.96 %** | **6.51 %** | **35** |
| 512 | 511 | 2.33 % | 3.85 % | 59 |
| 1024 | 1023 | 1.17 % | 1.94 % | 118 |

**A lie the cache records**, by where it sits (N = 299, s = 5): 100 % up to interval 149, 99.70 %
at 224, 88.07 % at 269, 49.53 % at 289, and 6.51 % at the last interval, where it is the row
replay's number. The forger's best cache-altering lie is therefore at the end of the answer.

**What a higher number costs** — intervals each seat must replay:

| N | s | 50 % | 90 % | 99 % |
|---|---|---|---|---|
| 299 | 5 | 39 | 111 | 180 |
| 299 | 3 | 62 | 161 | 235 |
| 1023 | 5 | 133 | 378 | 616 |

Ninety percent on the 300-token claim needs each of five seats to replay 111 of its 299
intervals — 37 % of the job each, plus the recompute to its latest draw. Sampling buys a weak
per-claim catch cheaply; a strong one costs most of an inference per seat.

**The capture arm's leaf sample.** A seat checking a served capture opens leaf 0 and three leaves
drawn uniformly (`fp_capture_samples_clear`). On the dense graph-v5 row that is a 1 in 565,750
chance per seat of landing on one forged leaf for an 8/8 job, and 1 in 13,919,200 for 51/300.

**A panel of seats that each hold a shard** (§3 Decision 5): stratified by shard, with a quorum of
three per shard, a one-leaf lie is seen only by its own shard's seats, so the coverage is the
s = 5 column at every shard count. The licensing object carries `3 × shards` receipts and fits one
standard transaction (120,000 bytes) up to **eight shards** — 114,597 bytes at eight, 128,913 at
nine. For the Kimi K3 stand-in of ADR-0097 (2.8 T parameters, one byte a weight as a floor) eight
shards is 326 GiB a seat.

### 1.3 What the measurement found in the seat

The question "does a seat that finds the lie make it count" had to be answered before the number
meant anything, and the code said no, in three ways.

1. **A seat that found a fault went on.** `interval_seat_outcome_v1` records a `Fault` /
   `FaultInRange` (`note_seat_fault_v1`) and returns `None`, and the verdict block fell through,
   in the same round, to the whole-capture arms. There the FPC1 arm could file **`Valid`**: its
   leaf sample misses the lie with near certainty (above). Otherwise the half-window tail filed
   **`Unavailable`**, a signed accusation of withholding against a producer that had served the
   data the seat just replayed. On a ruleset without ADR-0065 Decision 4 — the shipped devnet
   (`DEVNET_PARAMS`; `palw_rc_arm_phase1` arms it for testnet-11 and the mainnet card only) —
   `slash_dissenting_seats` charges that `Unavailable` `claim.reserved` when the other seats
   license the forged claim (`a_seat_that_reports_against_the_quorum_pays_what_it_tried_to_take`).
   The honest finder paid; the forger's claim was licensed. The seat also re-replayed the claim
   every round until its deadline.
2. **A state root that does not recompute was logged and dropped.** On a checkpoint mismatch the
   interval arm returned `None` without recording anything, so the challenger's half never
   prosecuted it. By §1.1 this is the path that catches every cache-altering lie before the drawn
   interval, which makes it the seat's strongest detector.
3. **A leaf of the claim's own capture that does not recompute was a `false`.** The capture arm's
   sampler returned the same `false` for "proved a fault" as for "could not check", so a proven
   fault was dropped and the seat drifted to the tail.

Two facts decide the fix and one more decides the design:

* **Silence is never charged.** `slash_silent_seats` has been a no-op since audit M2-7, because the
  chain cannot observe silence. So "files nothing" costs the finder nothing on any ruleset.
* **A seat whose interval openings have not arrived also falls through** to the capture arm in the
  same round. For a claim whose capture fits the 16 MiB material cap, the four-leaf check can then
  be the seat's whole verification, before its interval replay ever runs. This is not a fault
  once found, and closing it trades liveness; see Decision 6.
* **Every conviction needs a party that can run the whole job.** The challenger's half re-executes
  the claim (`execute_free_prompt` / `execute`), and only if it does not reproduce does it open a
  court. It opens that court over the ruleset's whole `StepLeaves` space, and answers every rung
  of the ladder from its own execution. A seat can NAME the leaf (ADR-0086 Decision 6) but cannot
  open a court at it. For a network whose seats hold shards, nobody could convict.

## 2. The requirement

> **R-cov — the panel's coverage of one claim is a stated number, measured with the draw the seats
> run; a seat that finds a lie files nothing else about that claim and hands it to the court; and
> a network whose seats hold shards knows, before it mints, what the number costs it.**

## 3. Decisions

**Decision 1 — the coverage is a number, and it is generated.** `palw_seat_coverage_v1`
(consensus-core, a report) holds the closed form for the row replay and for the cache-altering
lie, the measurement that runs `palw_fp_interval_draw_v1` itself, the inverse questions (the draw
or the panel a target needs), the capture arm's sample as the same arithmetic, and the stratified
panel's cost. `palw-seat-coverage` prints §1.2. Two things are said with the number every time,
because either alone misleads:

* **It is the half that costs a seat `k` intervals.** ADR-0077 Decision 8 is kept: a sampled
  verdict convicts nobody, and a claim stays disputable for its whole challenge window. Where a
  bonded watchdog (`--palw-challenge`) re-runs every licensed claim, a forged leaf is found with
  certainty, at one inference per claim.
* **It is the forger's best lie.** Any lie that alters the cache is caught with a higher
  probability, as §1.2 shows by position.

**Decision 2 — a seat that found a fault in a claim files nothing else about it.** The fault
ledger (`PalwSeatFaultLedgerV1`) is consulted at three points of the verdict block: before its
first arm, between the free-prompt interval arm and the capture arm, and before the half-window
tail. A claim in the ledger gets no receipt from that seat, in that round or any later one — not
a `Valid`, not an `Unavailable` — and it is not replayed again. The fault reaches the court
through the challenger's half, which prosecutes every claim in the ledger (ADR-0085 Decision 4).
This is ADR-0077 Decision 8's own sentence, put back where it had been lost.

**Decision 3 — every fault a seat proves is recorded.** A checkpoint state root that does not
recompute is recorded as a fault with no leaf address, `(0, 0)`, and the court's bisection finds
the leaf. The capture arm's sampler answers `Cleared`, `NotCleared` or `FaultAt(leaf)`, and the
last is recorded at its leaf. The ledger keeps the most specific address it is given: a named
leaf over a block over an unaddressed fault. A later, vaguer note never erases a better one.

**Decision 4 — the expected cost of a lie is the number times the claim's reservation.** A
conviction slashes exactly `claim.reserved` (`void_and_slash`). So a producer's expected cost for
one forged leaf, when detection leads to a conviction, is the coverage times `claim.reserved`:
6.51 % of it on the 300-token claim with five replaying seats. The chain deters a false answer
worth less than that. It does not deter one worth more, except through the watchdog. The coverage
rule a network wants — a draw `k`, a panel size — is a mint-time choice priced by §1.2's second
table. This ADR prices it and does not choose it: a coverage rule chosen before the measurement
was ADR-0081's warning, and one chosen by this ADR would be a guess with a table beside it.

**Decision 5 — what a network whose seats hold shards must have, named.** Not built, and each item
is measured before it is designed:

* **A stratified panel.** Seats are drawn per shard, `r` per shard, with a quorum per shard. The
  coverage of a one-leaf lie is then independent of the shard count (§1.2), so today's number is
  kept at `r = 5`. The panel is `5 × shards` seats, and its licensing object carries `3 × shards`
  receipts. That fits one standard transaction up to eight shards. Past eight, the licensing
  object must be split across transactions or aggregated. ML-DSA-87 signatures do not aggregate,
  so this is a design question with its own measurement.
* **A court a shard can open.** Opened at a leaf a seat NAMES, and adjudicated at the terminal step
  from what the seat already holds: its interval opening (the committed rows and the state the
  interval resumes from, bound to the claim's roots) and the artifact opening of that one node's
  weights. The terminal step never needed the whole model; only the ladder above it did, because a
  challenger answers each rung from its own whole execution. This court is a consensus object
  behind its own fence, and the sibling of ADR-0093's responder. Its first measurement is the
  close for one node of the target model at a ladder that holds it; ADR-0097's generator already
  prices it.
* **The seat's replay budget.** A seat's work is `k` interval replays plus the recompute to its
  latest draw, on its shard. It must fit `window_receipt` at the certification drill's measured
  rate (ADR-0082 Decision 9), which no model of this size has yet been measured at.

**Decision 6 — the pending fall-through is the operator's.** Closing it means a seat whose interval
openings are pending never certifies through the capture arm. That would make the interval draw
the seat's only check on small-capture claims, and a producer that serves captures but not interval
openings would get no receipts from seats that follow the rule. It is named, with its number (§1.2's
capture-arm row), and left open.

## 4. What this costs

* **Chain:** nothing. No object, field, fence or fingerprint.
* **A seat:** on a claim it found a lie in, it stops replaying and stops filing. On an honest claim
  nothing changes. On a forged claim the licensing quorum loses one seat, and the other four can
  still reach three. That is the point: licensing is not conviction, and the court decides.
* **devnet:** an honest finder is no longer charged `claim.reserved` for the `Unavailable` it
  should never have filed.

## 5. Invariants the tests hold

```
1  The closed form is the exact rational rounded down, never up (a floor).
2  The draw the seats run, over deterministic beacons, agrees with the closed form on the mean
   and on every interval within six binomial deviations — uniformity is what a forger choosing
   where to lie would exploit, and it is measured, not assumed.
3  A cache-altering lie is caught through any later draw; at the last interval its number is the
   row replay's; before it, never less.
4  A job no longer than the draw is replayed whole by every seat (the hybrid's N = 1).
5  A stratified panel's coverage does not depend on the shard count, and its licensing object
   fits one standard transaction for at most eight shards (pinned as the limitation).
6  The fault ledger: a fault once found stays found; a named leaf outranks its block; an
   unaddressed fault never erases an address; past its bound a NEW claim is refused.
7  The verdict block consults the ledger at its three points, the capture arm records the fault
   it proves, and the root that does not recompute is recorded — a source pin whose negative
   control (one gate removed) was run and observed red.
```

1–5 are `consensus/core/src/palw_seat_coverage_v1.rs`'s tests; 6 is `kaspad/src/palw_fp_seat.rs`'s
`a_fault_once_found_stays_found_and_the_most_specific_address_wins`; 7 is `kaspad/src/palw_panel.rs`'s
`a_seat_that_found_a_fault_consults_the_ledger_before_every_verdict`.

## 6. Order of work

1. Decision 1 — **done** (§9).
2. Decisions 2 and 3 — **done** (§9).
3. The fleet takes Decisions 2 and 3 by a rolling rebuild; nothing is scheduled. That is the
   operator's.
4. Decision 5's court a shard can open — its own ADR, measured first.
5. Decision 5's licensing past eight shards — its own ADR, measured first.
6. Decision 6 — the operator's.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0077 Decision 8 — "a row unequal: the seat files nothing" | kept, and restored in the code (Decisions 2–3) |
| ADR-0077 Decision 8 — "a sampled verdict convicts nobody" | kept, and priced: it is why Decision 1 states what the number is and what it is not |
| ADR-0081 Decision 8 / U-03 — the coverage number | answered for the live draw (Decision 1); the withdrawal of ADR-0081 §3 stands |
| ADR-0085 Decision 4 — the challenger's half prosecutes a seat's fault | kept; it now sees the state-root and capture-arm faults too (Decision 3) |
| ADR-0065 Decision 4 — `Unavailable` abstains | unchanged; this ADR removes the case where an honest finder needed it |
| ADR-0097 Decision 5, first row — a seat that holds a shard | its measurement (§1.2) and its two missing pieces (Decision 5) |

## 8. What is deliberately not decided

* **The draw `k` and the panel size a network wants** (Decision 4) — mint-time, priced by §1.2.
* **Whether seats should be paid.** A seat is never paid today, as the doc on `slash_silent_seats`
  says. The number above assumes every drawn seat replays. The economics of whether one does
  belong to ADR-0033's successor, not here.
* **The pending fall-through** (Decision 6).
* **The shard court's object, its fence, and its close** (Decision 5).

## 9. Number hygiene and implementation record

0098 is the next free number after ADR-0097 (whose README row says so). It is claimed on
`feat/adr-0098-seat-coverage`, branched from `feat/adr-0097-model-fit` at `65ee18cd`. A concurrent
claimant renumbers the later writer. **The next free number is 0099.**

* **2026-09-10** — ADR written and implemented the same day:
  * `consensus/core/src/palw_seat_coverage_v1.rs` — Decision 1, with seven tests.
  * `misaka-palw-base0/src/bin/palw-seat-coverage.rs` — the generator, classified in the
    float-free scan as a measurement tool.
  * `docs/palw-seat-coverage-2026-09-10.md` — the generator's output, as a dated record and not
    as a source.
  * `kaspad/src/palw_fp_seat.rs` — the fault ledger, with its test.
  * `kaspad/src/palw_panel.rs` — Decisions 2 and 3: the three gates, the recorded state-root
    fault, the sampler's three answers, and the source pin.

  kaspad's whole suite ran green, as did the targeted consensus-core and base0 suites. The
  mainnet card and testnet-11 behave as before except that a finder files nothing. On devnet the
  finder is no longer charged.
