# ADR-0140 — The clock sells elapsed protocol time, not hash work

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **This ADR changes
no consensus rule and arms no fence.** It decides what the heartbeat lane is *for*, separates the
four jobs it currently does into named seams, and sets the measurements that a later ADR — not this
one — would need before any clock primitive is replaced. Everything it builds is instrumentation,
an internal interface, a shadow calculation or a non-consensus prototype.

**Builds on:** ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery from a total stop),
ADR-0066 (the heartbeat out of `bits`), ADR-0083 Decision 1 (only priced rows price the window),
ADR-0105 (what a heartbeat miner is told), ADR-0138 §3c (the heartbeat's interval follows the clock).

## 1. What the heartbeat actually sells

ADR-0060 states the separation this chain is built on: **time is permissionless, weight is bonded,
finality is an overlay.** The heartbeat lane is the permissionless half. It exists because four
separate incidents — the testnet-12 block-600 exposure wedge, the floor-producer stall, the
quarantine runaway and testnet-10's 81-hour virtual wedge — had one root: *the clock was hostage to
a stuck actor*.

That doctrine settles two proposals before they are made, and both have been made:

* **A heartbeat signed by a bonded producer.** ADR-0060 §2's phrase is "accountability implies
  killability": a bond can be slashed, can hit its exposure ceiling, can exit, can be wrongly
  convicted. A clock that depends on one dies with it. The lane must not be bonded.
* **A heartbeat that is only admissible while the bonded lane is silent.** ADR-0064 Fact A is a
  theorem, not a caution: *silence is not checkable from a fork*. A branch sees only its own
  timestamps, so any branch can claim it was quiet. A right unlocked by silence is a right anyone
  can unlock. ADR-0105 §4 reached the same place from the other side and called candidate (b)
  fatal: a bond at its exposure ceiling, or a key that can emit a header-valid attempt, could stop
  the clock an hour at a time, and one block of depth cannot tell that from a good draw.

So the lane is **unconditionally permissionless**, and an unconditionally permissionless lane needs
a price. A signature is not a price. Today that price is a hash puzzle against a constant target.

**And the price is not incidental.** ADR-0064 Fact B: a lane whose weight is ~0 and whose cost is 0
can rebuild history for free. Respecting the median-time-past and future-drift rules, an attacker
could mint a heartbeat-only branch from genesis to now in microseconds and win the tie at equal
length. The hash price is what binds that forgery to `blocks × 2²⁴` of work.

**What the lane sells is elapsed protocol time. The hash is only the current way of pricing it.**
That sentence is the whole of this ADR.

## 2. What ADR-0138 §3c already changed, and why it moved the safe way

ADR-0138 §3c makes the heartbeat's interval follow the clock rather than the bond: past
`palw_anchor_clock` a heartbeat runs at the recovery cadence whenever its selected parent does not
advance the DAA. On testnet-11, where no lane is priced by `bits` at all, that turns the heartbeat
into the chain's clock at the target block time.

It is worth stating plainly that this moves **toward** ADR-0060, not away from it: the lane that now
carries testnet-11's clock is the bondless one, which is what "time is permissionless" means.

It also moves Fact B's number the safe way. Forging a heartbeat-only history that covers `T` seconds
needs `T / interval` blocks, each priced at the lane's constant target. Shortening the interval from
the nominal hour to the recovery cadence makes that forgery **thirty times dearer per unit of time
covered**, not cheaper.

## 3. The three places hash remains, and which one is the biggest

An inventory, because proposals to "remove mining" have twice aimed at the smallest of the three.

| # | where | what it is | can it go? |
|---|---|---|---|
| 1 | identity, Merkle roots, commitments | the hash function as a hash function | no, and nothing here proposes it |
| 2 | the heartbeat's constant target | the spam price of a permissionless lane | only if something else prices elapsed time |
| 3 | **the attempt lottery, `ticket digest < target(bits)`** | rate control on the model lane | the largest of the three, and its own ADR |

The third is the one to look at first if the goal is "no hash lottery". A producer has already paid
for one deterministic inference before the comparison happens; the comparison then throws most of
that work away. ADR-0071 §1 names the same coupling from the inside: the quantity that should
describe LLM work is derived from a hash lottery. That question is [ADR-0141](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md),
deliberately separate from this one.

## 4. Why an LLM inference cannot be the clock

A clock lane needs three properties at once: **permissionless**, **self-verifying** (a verifier pays
far less than a producer), and **expensive to generate**. A forward pass fails the second — the
verifier pays the same as the producer — so verification has to be pushed to a court or to sampling,
and the court is scheduled by the clock. The circle closes.

A hash is not a philosophy here. It is the cheapest function anyone has that holds all three at once.

A *micro-PALW clock class* — a tiny integer model, one prefill forward, trace root under a constant
target, re-executed by every node — would satisfy the three properties, and it is still exactly a
proof of work with the hash function swapped for a neural network. What it costs is worse than what
it buys: every full node would run the model runtime on the path that has to work when the network
is most broken, which inverts ADR-0060's dependency direction. A cross-host determinism bug would
then kill the clock, and the clock is what the repair needs.

**Rejected as a clock. Recorded as a candidate canary** — a determinism probe that carries no
consensus weight and no clock role, which is a genuinely useful thing and is not this ADR's.

## 5. The candidates, in one failure model

| candidate | permissionless | verify cost | spam price | cost of rewriting a total-stop history | post-quantum | build cost |
|---|---|---|---|---|---|---|
| hash heartbeat (today) | yes | µs | parallel | `blocks × 2²⁴`; ties broken by length | yes | shipped |
| bonded signed heartbeat | **no** | µs | **none** | **free** | yes | low |
| micro-PALW (all nodes re-execute) | yes | ms | parallel, i.e. the same shape as PoW | as today | yes | medium-high, and couples the clock to the runtime |
| VDF (class group / RSA) | yes | ms | **sequential** | cannot be produced faster than the sequential work allows | **no** | high |
| PoSW, hash-based sequential (Cohen–Pietrzak) | yes | ms | **sequential** | same | yes | high |
| lifecycle windows counted in time | orthogonal | — | — | — | — | medium |

Two entries need their claims stated carefully, because the case for them is usually overstated.

**Sequential work does not fix real time to the clock.** It removes the *parallel* advantage: a
thousand machines do not shorten `T` sequential steps. Faster single hardware still does. Chia's
timelords are the worked example, and ASIC timelords exist. So the honest claim is "an attacker
cannot buy speed by buying width", not "an attacker cannot go faster than wall-clock". Any ADR that
adopts this must say so in those words.

**Counting lifecycle windows in time is orthogonal, not an alternative.** It changes what the clock
is used *for*, not what prices it. And it raises the stake on the price rather than lowering it:
median-time-past is itself derived from timestamps the branch declares (ADR-0064 Fact A again), so
moving deadlines onto it makes "a heartbeat history must not be cheap to forge" matter *more*. The
two must be decided together or in that order, never the reverse.

## 6. Decision

**D1. The heartbeat lane sells elapsed protocol time.** The hash puzzle is its price, not its
purpose. Any replacement is judged on whether it prices elapsed time better, under §5's failure
model, and not on whether it removes a hash function.

**D2. The clock stays permissionless and unconditional.** No bonded signature, and no admissibility
condition keyed on the bonded lane being silent. ADR-0060 §2 and ADR-0064 Fact A, restated so that
the next proposal meets them at the door.

**D3. The lane's four jobs get four seams in the code, with behaviour unchanged.** The heartbeat
today advances the clock, carries transactions, becomes the parent of the next bonded block, and
contributes ε to fork ordering. These are four decisions and they do not have to share a primitive.
Separating them is what makes a later clock swap a local change, so it is done *before* any
prototype, not after. See §7.

**D4. Protocol time gets one name.** Every deadline in the system reads its time from one view
rather than reaching for a DAA score, a timestamp or a median-time-past on its own. Building the
view changes no rule; it makes the inventory in D5 possible at all.

**D5. Every deadline is classified before any of them moves.** A registry naming each deadline, the
basis it uses today, the basis it could use, its nominal duration and its safety margin. Moving
everything to elapsed time is not the goal — some deadlines belong on the block count. The output is
a table someone decides from.

**D6. The elapsed-time lifecycle is computed in shadow and never enforced.** Both answers are
computed, only the existing one decides, and the difference is logged. A migration is then argued
from a season of production data instead of from a projection.

**D7. Sequential-time primitives are prototyped outside consensus.** Their own crate, no dependency
from `kaspa-consensus`, with the generate-to-verify ratio, the parallel scaling curve, the
cross-architecture determinism and the worst-case verification cost of a malformed proof all
measured. No fence, no header field, no rule.

**D8. The attempt lottery is instrumented, not touched.** How many draws an accepted block costs,
how much inference the losing draws burn, how far each class's realised production sits from its
bonded share. [ADR-0141](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) is where
that data is argued from; this ADR only makes sure it exists.

**D9. No zero-knowledge machinery anywhere in this line of work.** Bounded deterministic
verification plus commitments is the existing doctrine and it is sufficient for everything above.

## 7. The seams D3 names

Four questions the heartbeat answers today in one place, given four names and their current
behaviour, so that changing one later does not mean reading all four:

```
HeartbeatAdmission   — may this header exist here?      (the slot rule, the width bound)
ClockContribution    — what time does it advance?       (ADR-0138's per-mergeset stand-in)
CarrierPolicy        — what may it carry?               (fee-only, no subsidy)
OrderingContribution — what does it weigh?              (ε = 1, ADR-0060 D1.2)
```

Each returns exactly what the code returns today. The value of the split is that a later ADR
replacing `ClockContribution`'s price leaves the other three untouched and provably so.

## 8. What this ADR does not decide

* Whether to adopt PoSW, a VDF, or neither. That needs D7's measurements and is a later ADR.
* Whether any lifecycle deadline moves to elapsed time. That needs D5's table and D6's season of
  shadow data.
* Whether the attempt lottery can go. That is ADR-0141, and it is a larger change than this one.
* Anything about the 6,001 bundle. This ADR is scheduled behind that rollout and behind arming
  ADR-0105, on the operator's sequence: **roll out, arm, measure, then write the follow-on ADR.**
  The earliest height anything here could be fenced is DAA 6,500, and nothing here asks for one.

## 9. Order of work

1. Heartbeat telemetry: the lane's counts, intervals, heartbeat-only episodes, and — separately —
   the clock it advanced, the transactions it carried and the orderings it entered.
2. The four seams of §7, behaviour unchanged.
3. The protocol-time view of D4.
4. The deadline registry of D5.
5. The shadow calculation of D6.
6. A total-stop, partition and recovery harness on simnet, with both bases recorded per scenario.
7. An automatic comparison report from that harness.
8. The non-consensus sequential-time crate of D7.
9. Its benchmark suite.
10. The attempt-lottery telemetry of D8.

Steps 1 through 7 change no rule. Steps 8 and 9 are not linked into the node at all. Step 10 is a
counter.
