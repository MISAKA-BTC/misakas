# ADR-0140 — The heartbeat is the emergency generator: it must not touch the economy or the difficulty while the chain is producing

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Changes no
consensus rule and arms no fence.** It settles what the goal is, keeps the hash puzzle, states the
non-interference invariant as four separate claims, and says which of them the code already
guarantees and which are only believed.

**Builds on:** ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery from a total stop),
ADR-0066 (the heartbeat out of `bits`), ADR-0083 Decision 1 (only priced rows price the window),
ADR-0105 (what a heartbeat miner is told), ADR-0138 §3c (the heartbeat's interval follows the clock).

## 1. The goal, corrected

An earlier draft of this ADR set out to replace the heartbeat's hash puzzle with something that
prices elapsed time without a hash. **That is the wrong goal**, and stating why is the most useful
thing this document does.

Removing a hash function is not an end in itself. Block ids, Merkle roots and commitments are hashes
and are not going anywhere. What this chain is built to avoid is the *other* thing, the one that made
"proof of work" a synonym for it:

> a large amount of hashing, where whoever holds the most hash power controls economic block
> production.

That is already not how MISAKA works. Economic block production is one deterministic inference, its
execution commitment, and the attempt or receipt that carries it. The heartbeat is a bondless,
fee-only, near-weightless liveness lane beside it, and its hash is not model work and not an
alternative way to mine.

So the goal is not "no hash". The goal is:

> **While the chain is producing normally, the heartbeat lane must be unable to touch the economy,
> the difficulty, the fork choice or the clock — and in an emergency it must be able to carry all
> four on its own.**

The hash puzzle is a fine price for that. It is simple, permissionless, verified in microseconds,
already shipped and already tested. **It stays.**

## 2. The shape this should take in operation

```
NORMAL
    PALW attempt / receipt ──► economic block production
    heartbeat producers    ──► asleep, ~0 hash spent

FAILURE
    PALW silence, observed LOCALLY by each miner
    heartbeat producers    ──► awake
                           ──► bounded hash puzzle
                           ──► clock advances, transactions carried, lifecycle sweeps
    PALW recovers
    heartbeat producers    ──► asleep again
```

This is the right shape and most of it is already built: ADR-0105's yield hint is exactly the "go
back to sleep when a bonded block lands" signal, and it is deliberately **policy**, not a rule.

## 3. The one distinction that must never be lost

**"When to mine a heartbeat" is a miner's local policy. "Whether a heartbeat is valid" is consensus,
and it must not depend on whether PALW was producing.**

A rule of the form

```
if a PALW block was seen recently:  heartbeat invalid
if no PALW block was seen:          heartbeat valid
```

cannot be written. ADR-0064 Fact A is the theorem: **silence is not checkable from a fork.** A branch
sees only what it contains, so two partitions disagree about whether PALW was producing, and
heartbeat eligibility becomes a function of which branch you are on. ADR-0105 §4 reached the same
conclusion from the attacker's side and called it fatal: a bond at its exposure ceiling, or any key
able to emit a header-valid attempt, could stop the clock an hour at a time, and one block of depth
cannot tell that from an honest draw.

The same theorem closes the other tempting shortcut. **A heartbeat signed by a bonded producer** is
forbidden by ADR-0060 §2 — "accountability implies killability" — because a bond can be slashed,
capped, exited or wrongly convicted, and a clock that depends on one dies with it.

So, in consensus, a heartbeat is **always admissible under the same fixed conditions**, and the
safety comes not from restricting *when* it may exist but from making it *worth nothing* while the
chain is producing. That is §4.

## 4. The non-interference invariant, as four separate claims

This is the substance of the ADR. Each claim is separate, each has its own mechanism, and three of
the four are already closed by earlier ADRs — which is worth writing down, because "we believe the
heartbeat is harmless" is not the same as "here is the rule that makes it harmless".

### C1 — It must not tighten the difficulty

**Closed, twice.** ADR-0066 took the lane's price out of `header.bits` and into a network constant,
so a heartbeat header carries the global expected bits like every other row. ADR-0083 Decision 1 then
made the retarget count only rows a price paced.

This one is not hypothetical. Measured over the shipped 264-row window before the fix: 255 bonded
plus 9 heartbeat rows still demanded work 2, while **0 bonded plus 263 heartbeat rows demanded
33,554,432**. After a bonded outage longer than the window, a returning producer needed tens of
millions of inferences for one block, so no bonded block could re-enter the window and the average
never re-mixed — the emergency generator locking out the engine it was there to restart.

### C2 — It must not pace the clock while something else is pacing it

**Closed by ADR-0138.** A block advances the DAA score only where `bits` priced it, and a heartbeat
gives back exactly one exemption only in a mergeset that carries no priced block at all. Where the
economic lane produces, beats add nothing to the score. §3c then made the *interval* follow the same
question, so the lane also stops competing for slots while a parent is pacing the clock.

### C3 — It must not take fork-choice weight from the economic lane

**Closed by ADR-0060 Decision 1.2.** A heartbeat weighs ε = 1 against an attempt block's fixed 2²⁰.
A million beats do not outweigh one bonded block, and among heartbeat-only branches ε × n still
orders the longer one first, which zero would not.

### C4 — It must not earn, and must not capture block production

**Closed by the lane's definition**: bondless, claimless, fee-only, no subsidy. There is nothing to
win by minting one while the chain is healthy, which is what makes the permissionlessness of §3 safe
rather than dangerous.

### C5 — and in an emergency it must actually start

The invariant has two halves, and C1–C4 are only the first. The generator also has to turn on.

This half failed on the drill while this ADR was being written, and the failure was in the sleep/wake
policy rather than in any rule. ADR-0105's yield exists so a beat does not bury an attempt block
waiting to be merged. Past the anchor clock an attempt block carries its weight but advances no DAA,
so standing aside for one waits on a block that will not move the clock — on a chain where nothing
else will either. Worse, the economic lane produces continuously, so the yield target kept sliding
forward and the lane stood aside from the very job it exists for, until its hour-long episode budget
drained. Observed directly: the fence crossed, the DAA frozen at 20, the miner running and never
minting.

The fix is §3c's principle applied to the mergeset scan rather than the parent: **yield to whoever is
pacing the clock, and to nobody else.** It is policy, not a rule, which is exactly where D4 says this
decision belongs — and it is why D4 is a decision rather than an observation.

The lesson generalises past this bug. C1–C4 are about the generator not running when the mains are
on; C5 is about it starting when they go off, and the two are easy to trade against each other
without noticing. A guard that only checks the first half would have passed this.

### What is NOT closed

The five claims above are each enforced somewhere, but **no test asserts them as a group**, and
nothing fails if a future change quietly breaks one. Every one has been broken at least once — C1 in
the first heartbeat implementation, C2 by ADR-0132 S leaving the DAA behind, C2 again twice inside
ADR-0138's own drafting, and C5 by the yield policy above. That is the work this ADR leaves behind:
not a new primitive, a standing guard.

## 5. Why not PoSW or a VDF

Both remove the *parallel* advantage: a thousand machines do not shorten `T` sequential steps. Neither
fixes real time — faster single hardware still goes faster, which is why Chia has ASIC timelords —
so the honest claim is "an attacker cannot buy speed by buying width", not "cannot go faster than
wall-clock". Class-group and RSA VDFs are also broken by a quantum adversary, which a chain that
picked ML-DSA-87 should not adopt; the post-quantum option is hash-based sequential work.

Against that, the cost of adopting one is a new cryptographic primitive, a new proof format, new
verification code, a new denial-of-service surface and new performance assumptions — **for a lane
that should be idle almost always.**

**Decision: not now, and the condition is a measurement, not an opinion.** If the heartbeat share of
blocks over a long run is under a few tenths of a percent, the hash puzzle stays and this question
closes. The measurement is D5.

## 6. Where the hash that matters actually is

Three uses survive, and the emergency generator is the smallest.

| # | where | what it is | verdict |
|---|---|---|---|
| 1 | identity, Merkle roots, commitments | a hash function used as a hash function | keep, nothing proposes otherwise |
| 2 | the heartbeat's constant target | the spam price of a lane that should be idle | **keep** — §1, §5 |
| 3 | **the attempt lottery, `ticket digest < target`** | rate control on the economic lane | the real question — [ADR-0141](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) |

A producer pays for one full inference *before* the third comparison happens, and most draws lose.
ADR-0071 §1 names the same coupling from the inside. If the aim is to reduce hashing that shapes who
produces blocks, that is where it lives — not in the generator that runs when the lights are out.

An LLM inference cannot replace either one as a clock, for a structural reason rather than a
preference: a clock lane must be permissionless, self-verifying and expensive to generate, and a
forward pass fails the second because the verifier pays what the producer paid. Pushing verification
to a court closes a circle, since the court is scheduled by the clock. A *micro-PALW clock class*
would be a proof of work with the hash swapped for a neural network, and it would put the model
runtime on the path that has to work when the network is most broken — the opposite of ADR-0060's
dependency direction. **Rejected as a clock; recorded as a candidate determinism canary**, which
carries no clock role and no weight.

## 7. Decision

**D1. The hash heartbeat stays.** It is the emergency generator's price. Replacing it is not a goal.

**D2. The goal is non-interference while the chain is producing, and reliable starting when it is
not**, stated as C1–C5 and enforced by the mechanisms named there.

**D3. Consensus admissibility stays unconditional.** No bonded signature, and no rule keyed on
whether PALW was producing. Safety comes from the lane being worth nothing, not from restricting when
it may exist.

**D4. When to mine is policy and stays policy.** ADR-0105's hint is the mechanism; a miner sleeps
while a bonded block is pacing the chain and wakes when none is. No consensus rule reads it.

**D5. C1–C5 get one standing guard, and the lane gets counted.** A test that asserts the five claims
together — including the second half, that the lane starts when nothing else is pacing the clock — so a future change that breaks one fails here rather than on a live network; and counters
for what share of blocks the lane actually produces, how often it is the clock, how many transactions
it carries and how long its episodes last. The counters are what close §5's question.

**D6. The attempt lottery is the real hash question**, and it is ADR-0141's.

**D7. No zero-knowledge machinery anywhere in this line of work.** Bounded deterministic verification
plus commitments is the existing doctrine and it suffices.

## 8. What this ADR does not decide

* Whether PoSW or a VDF is ever adopted. D5's measurement is the gate, and the expected answer is no.
* Whether any lifecycle deadline moves from DAA to elapsed time. That was explored while this ADR was
  drafted and is deliberately dropped: median time past is itself derived from timestamps a branch
  declares (ADR-0064 Fact A), so moving deadlines onto it would raise the stake on the clock lane's
  price rather than lower it. If it is ever revisited it needs its own ADR and this one's C1–C5 in
  force first.
* Whether the attempt lottery can go. ADR-0141.
* Anything in the 6,301 bundle. This ADR is scheduled behind that rollout and behind arming ADR-0105:
  **roll out, arm, measure, then decide.** Nothing here requests a height.
