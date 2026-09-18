# ADR-0141 — Can an inference be the ticket without a hash lottery?

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **This ADR decides
nothing about the lottery, changes no rule and builds nothing.** It states the question precisely,
records why it is the one place a hash is still worth reopening, lists what would have to be answered
before anything replaced it, and specifies the counters that would settle the first half of the
argument — to be built when the 6,701 rollout is behind us, not before.

**Builds on:** ADR-0071 (the attempt lane's price and the ticket's bound), ADR-0072 (the ticket is
the execution), ADR-0076 (the class target seed), ADR-0117 (one forward, one draw), ADR-0132 S (the
single lottery), ADR-0137 (one work target `W`), ADR-0140 §6 (the inventory this follows from).

**Why this one and not the heartbeat.** ADR-0140 keeps the heartbeat's hash: it prices a lane that
should be idle almost always, and swapping it for a sequential-work primitive would buy a new
cryptographic dependency for something that runs when the lights are out. The hashing that actually
shapes *who produces blocks* is here, on the economic lane, and it is the only one of the three
worth reopening.

## 1. The question

A producer runs one deterministic inference, commits its execution, derives a ticket digest, and the
block is admitted when that digest falls under a target. Past 6,701 the target is `CCU / W` — the
class's counted compute against the network's one work target — and the Layer-0 digest is no longer
compared to `bits` at all (ADR-0132 S). The comparison is still a comparison against a target.

So: **the producer has already paid for the inference before the comparison happens.** Most draws
lose, and every losing draw is a forward pass the network bought and discarded. The lottery is not
grinding — ADR-0117 fixed one forward to one draw, so a producer cannot search — but it is still a
target comparison, and its job is rate control rather than weight.

Weight does not come from it. An attempt block's fork-choice work is a constant `2²⁰`, set by the
lane and not by the draw. The bond is what stands behind the claim. ADR-0071 §1 records the residual
coupling in its own words: the quantity that ought to describe LLM work is derived from a hash
lottery.

Hence the question in the title. If rate control is the only job, something that is not a lottery
could do it:

```
today        eligible producer → one inference → digest → digest < target → block
the question eligible producer → one inference → valid proof → block
```

## 2. Why this is the biggest of the three, and still not first

ADR-0140 §6 inventories the three surviving uses of a hash: identity and commitments, the
heartbeat's spam price, and this. The first is not going anywhere and nothing proposes it should.
The second is kept deliberately — it prices the emergency generator, which should be asleep. This
one prices nothing and secures nothing: it meters, on the lane that decides who gets paid.

It is nevertheless not the first thing to change, for a reason that has nothing to do with its
difficulty. **Removing the lottery means answering "who produces slot N?", and that question has no
small answer.** Everything the lottery currently makes moot comes back at once:

* a producer that is offline when its turn arrives,
* several producers eligible for one slot, and the DAG concurrency that follows,
* grinding of producer identity to land on favourable slots,
* Sybil bonds buying eligibility,
* manipulation of whatever beacon assigns turns — and ADR-0074's beacon is retired for exactly this
  class of reason, so a new randomness source would need its own justification,
* how a class's share becomes a number of slots,
* the receipt lane, which is scheduled against the attempt lane,
* how a newcomer with a fresh bond gets its first turn without an invitation,
* censorship, when the eligible producer is the one refusing a transaction,
* an empty slot, and what the chain does with it.

That is ADR-0071, ADR-0072 and ADR-0076 rewritten together with the class economy. It is a season of
work, not a change.

## 3. What would have to be true first

**M1. The waste has to be measured, not assumed.** The argument for removing the lottery is that it
burns inference. Nobody has counted it. Required, per class and fleet-wide: draws per accepted
block, forwards spent on losing draws, wall-clock between winning draws, and the gap between a
class's bonded share and its realised production share. If the waste is small, the whole question is
an aesthetic one and should be closed.

**M2. A replacement must not reintroduce grinding.** ADR-0117's "one forward, one draw" is what makes
the current lane honest. Any eligibility rule has to be a function of facts a producer cannot cheaply
vary — and "cheaply" has to be measured against the cost of one forward pass, which is seconds, not
microseconds.

**M3. A replacement must survive an offline producer without a silence condition.** ADR-0064 Fact A
binds here exactly as it binds the clock: a branch cannot prove that the producer whose turn it was
did not produce. Any "if the scheduled producer is absent, then…" rule is unlockable by anyone. This
is the constraint that kills the obvious designs, and it should be met before a design is drawn, not
after.

**M4. It must not need a new beacon.** ADR-0074's beacon retired. A scheme whose first requirement is
"a fresh randomness source" is proposing that retirement be reversed, and must say so.

**M5. ADR-0140's four claims must be in force first.** Slot assignment is stated in time, and any
scheme that changes who may produce also changes what the heartbeat lane sees. C1 to C5 hold the line
between the two lanes; reopening this one while that line is only believed rather than guarded would
mean debugging both at once.

## 4. Decision

**D1. The lottery is not changed, and no design is adopted here.** The question is recorded, its
cost stated, and its preconditions named.

**D2. The waste gets counted before it gets argued about.** M1's counters are specified here and
built after the 6,701 rollout: draws, wins, inference spent on losing draws, and the longest run of
consecutive losses, which is the wait a producer actually feels and which an average erases. They
cost nothing, they are useful to an operator regardless of this question, and no rule reads them.
Until they exist, every claim about how much this lottery wastes — including the ones in this
document — is an estimate.

**D3. This ADR does not merge into ADR-0140.** The clock and the lottery are separate mechanisms
with separate failure models, and a single document covering both would let a conclusion about one
carry a conclusion about the other. That is the specific mistake this pair of ADRs exists to avoid.

**D4. Nothing here is scheduled.** No fence, no height, no dependency for the 6,701 bundle.

## 5. What this ADR does not decide

Whether an inference can be the ticket. That is the title, and the honest answer today is that the
data to argue it does not exist yet. D2 creates the data.
