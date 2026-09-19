# ADR-0146 — A coefficient is justified by the arbitrage it permits, not by being right

**Status:** DESIGN, 2026-09-19. No code, no fence. ADR-0145 named the coefficient table its hardest
open question and deferred it here.

---

## 1. The trap this ADR exists to avoid

ADR-0145 says canonical work is a vector and that "the coefficients that turn a vector into an
economic unit are protocol-set". That sentence hides a problem: **a coefficient table is a place
where somebody decides numbers**, and the operator's standing rule forbids hand-written per-model
multipliers. A table of hand-picked weights is the same object one level up — it just spreads the
decision across operations instead of across models.

The existing table already understood this and took a position (`palw_economic_compute_v1.rs`):

> Chosen from the arithmetic each kernel performs at the integer engine's batch of one, **not from
> any host's timing**.

Every entry is a **count**, not a preference. That is why it has survived review, and it is also
exactly why it fails: arithmetic counts do not see memory traffic, so a 35B mixture activating 3B
and a 1.5B dense row can execute the same MAC-equivalents and cost very different amounts.

So the table cannot be fixed by picking better numbers. It has to be fixed by deciding **what makes
any number legitimate.**

---

## 2. Most dimensions need no coefficient at all

The first result is that the free-parameter problem is much smaller than it looks, because most of
the vector is derivable from facts already on chain.

**The existing unit already doubles as a byte count.** `matmul_mac` is "one 8-bit weight × integer
activation multiply–accumulate, which at the integer engine's batch of one is also **one weight byte
streamed**". So for 8-bit weights, arithmetic and traffic coincide — and they diverge exactly in
proportion to the quantisation format, which the canonical class descriptor carries (ADR-0145 §3).

```
traffic_term = MAC-eq × real_bits_per_weight / 8
```

Q4_K_M streams half a byte per weight; W8A16 streams one. **No new trusted input, no hand-picked
number** — the format is a declared-and-verified property of the artifact, not a preference. This is
the red-team's C9 and it closes the one gap the current table is known to have.

The same holds elsewhere. A routed expert row's cost follows from how many experts the row activates
— a graph fact. Attention follows from heads × head dimension × true KV length — an execution fact.
KV read and write follow from the cache the receipt commits to (ADR-0145 §6).

**Rule R1.** A dimension that can be derived from the canonical graph, the artifact's declared and
verified format, or the execution facts MUST be derived. A coefficient is permitted only where no
derivation exists.

That leaves far fewer free parameters than "a coefficient table" suggests: essentially the
**relative price between dimensions** when the vector is collapsed into one economic unit.

---

## 3. The coefficients that remain cannot be validated by being "right"

Nobody can demonstrate that a dense MAC is worth 1.0 and a KV byte 0.31. Any such claim is a
measurement on one host, one runtime and one batch size, and ADR-0038 Decision D already refuses to
let a wall-clock second reach fork choice.

So this ADR does not ask for correct coefficients. It asks for a **bound**.

**The requirement is not that reward tracks cost. It is that no participant can choose a
configuration whose reward-per-real-cost materially beats another's.**

That is a property of the table *over the space of admissible configurations*, and unlike
correctness it is checkable — by exactly the method the 2026-09-19 red-team used: enumerate the
admissible space and search it for the extreme.

**Rule R2.** A coefficient table is accompanied by its **measured arbitrage bound**: the maximum
ratio of reward-per-unit-real-cost between any two admissible configurations, found by adversarial
search over profiles, quantisations, canonical jobs and execution modes. The table is proposed
together with the search that produced the bound, and the search is committed and re-runnable.

The table is not "the right numbers". It is "numbers whose worst case somebody measured, and here is
the program that measures it".

For scale, the bounds on the basis in force today, from the audit's own probes:

| basis | measured spread | what the spread is over |
|---|---|---|
| STEP leaves | **427×** | admissible canonical jobs, one graph |
| STEP leaves | **101×** | tilings of one graph |
| STEP leaves | **6.8×** | the four live classes, monotone in model width |

A candidate table is not an improvement because it is better reasoned. It is an improvement when its
number in that column is smaller, measured the same way.

---

## 4. What the bound must be, and what happens if none is reachable

**Rule R3.** An arbitrage bound is acceptable only if it is **smaller than the efficiency gains the
protocol intends to reward**. If representation choice can move reward-per-cost by 10× while a
genuinely better model moves it by 3×, the economy rewards representation over engineering, and the
table fails no matter how principled its derivation.

This gives a concrete target rather than a wish: the bound must sit below the spread of real
efficiency across the models the network expects to run. That spread is itself measurable, and
measuring it is part of proposing a table.

**If no coefficient vector reaches an acceptable bound, the vector must not be collapsed.** The
fallback is to keep the dimensions separate and give each its own eligibility budget, so a
configuration that is extreme in one dimension exhausts that dimension's budget rather than
converting it into a general claim on the reward pool. Collapsing to one number is a convenience,
not a requirement, and this ADR declines to assume it is achievable.

---

## 5. Governance

**Rule R4 — nobody in the economy sets them.** Not miners, not class registrants, not panel seats,
not model owners. A coefficient is never a field in any object a participant signs. This is
ADR-0145's I1 and I2 applied to the table itself.

**Rule R5 — a table is versioned whole, never edited.** The current table already states this ("a
change to any entry is a new version, never an edit") and it stays. A claim records the table
version it was priced under, so history stays re-verifiable and no repricing is retroactive.

**Rule R6 — a new version is a consensus change.** Its own fence, its own drill that crosses that
fence, its own adversarial review, and its arbitrage bound re-measured and published with it. Per
ADR-0144 §6 item 0 and this repo's own working rule: the height is chosen after the evidence, not
before.

**Rule R7 — the bound is a gate, not a note.** A version that does not ship with a re-runnable
search and a bound does not arm. "We reasoned about it carefully" is what the current table did, and
the current table is 427× wide.

---

## 6. The experiment that has not been run

ADR-0145 says any coefficient today is "a guess wearing a number", and that remains true until this
runs:

> Both live classes' draw jobs replayed warm on **one host**, with the artifact fully resident, and
> then not resident. If the gap survives residency, MAC-equivalents need the traffic term of §2.

**It must be run on one host** precisely because a comparison across hosts measures the hosts. And
the figures that looked like this experiment were not: the red-team established that the 13.7× gap
cited earlier came from `#[cfg(test)]` fixtures in `palw_verification_profile_v1.rs`, and that the
production path overrides a measured p99 with a reference-rate estimate. That correction is why this
section exists rather than a table.

### It cannot be run on this fleet, and that is the more useful result

Measured 2026-09-19 across every host the network runs on:

| host | RAM | `qwen36.palwq36` | `qwen25-1.5b-a16.palwart` |
|---|---|---|---|
| 169.58.232.113 | 23 G | 34.0 G | 1.7 G |
| 5.104.81.23 | 23 G | 34.0 G | 1.7 G |
| 169.58.39.220 (ibm) | 23 G | present | present |
| 95.111.236.186 | 11 G | absent | 1.7 G |

**No host can hold the hybrid class resident.** The artifact is 34 G on machines with 23 G, and
ADR-0112's loader takes at most a fifth of the artifact's weights within what the host has past a
reserve — so on this fleet roughly four fifths of every Qwen3.6 inference is read from disk, every
time, while the 1.7 G dense class is resident on every host.

So the residency question §6 poses is not open on this network: **residency is binary here and the
two live classes sit on opposite sides of it.** The cost difference between them is not a subtle
memory-traffic term to be calibrated, it is RAM against disk — and neither the leaf basis nor the
MAC basis contains a term that can express it. ADR-0131's table says as much about itself: its
entries are counts of arithmetic, "not from any host's timing".

Two consequences for this ADR:

* the traffic term of §2 is necessary and still not sufficient — it distinguishes Q4 from W8 but not
  resident from paged, and `artifact_bytes` against a host's cache budget is the quantity that does.
  Whether a HOST-dependent quantity may reach the price at all is a real question: ADR-0038 Decision
  D refuses wall-clock, and artifact size is not wall-clock, but the budget it is compared against
  is a property of a machine. **This is the open design question the experiment would have refined
  and did not create.**
* §4's fallback moves from contingency toward the likely answer. When two classes differ by whether
  they fit in memory, one scalar relating their work is unlikely to bound arbitrage below the
  efficiency spread the protocol wants to reward.

**And the experiment was not run rather than run badly.** The bench harness exists
(`palw-worker-iso-bench --mode v2-replay-bench`, fleet-measured at p50 2,885 ms for 16 decode tokens
and 84,513 ms for 512 — a marginal 164.6 ms per decode token on a 2B model). Running it against the
34 G artifact would mean thrashing the page cache of a host carrying two live testnet-11 nodes,
while a third fleet node was already 330 DAA behind. A measurement that costs the network its
liveness is not evidence, and the number would have described one machine anyway.

**What it needs:** a host that can hold 34 G resident and carries no fleet node. Until one exists,
§2's derived terms ship and §4's bound is measured over the admissible space — which needs no host
at all, because it is arithmetic over profiles.

**What the experiment decides.** If residency explains the gap, §2's derived traffic term is
sufficient and the collapse may have no free parameters at all. If it does not, there is a real
dimension neither arithmetic nor bytes captures, and §4's fallback — separate budgets — becomes the
likely answer rather than the contingency.

---

## 7. What this ADR does not decide

The dimensions (ADR-0145 §4 holds them provisional), the numbers, and any height. It decides what a
proposal must carry to be considered: a derivation for every dimension that admits one, a search
program, a measured bound, and a comparison of that bound against real efficiency spread.

---

## 8. Done means

> No coefficient is set by anyone in the economy. Every dimension that can be derived is derived.
> The few that cannot ship with a committed program that searches the admissible space and reports
> the worst arbitrage they permit, and that number is smaller than the efficiency differences the
> protocol means to reward. If no such table exists, the vector is not collapsed.

The test of this ADR is not whether the numbers look reasonable. It is whether, a year from now,
somebody can re-run one command and find out whether they still are.
