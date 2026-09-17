# ADR-0130 — BPS 1 is hardened before it is widened

* Status: **ACCEPTED 2026-09-17, implementation in progress** on `feat/palw-exec-lane-and-validator-retirement`.
  Decisions 2–5 ride testnet-11's DAA 7,001 flag day with ADR-0124/0125/0126/0128. Decision 1 is built
  dormant: arming it on testnet-11 needs the capital §3 measures, and that is the operator's call.
* Operator's direction, in the operator's words: "BPSは当分1で固定するなら、今は高速化ではなく『BPS1でも壊れ
  ない経済性・選出・settlement』を固める段階"; "一番大きい変更を1つだけ選ぶなら、まず
  `panel_exposure = max(3×claim.reserved, λ×seat_reward)`"; "その次が BPS1でのcross-round operator連続禁止、
  その次が claim由来seedから『snapshot後のfuture DAG anchor由来seed』への変更"; "代替operatorがいない →
  round miss"; "安全性のためのルールを、参加者不足だからこっそり解除するのは危険".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel's pay and a seat's exposure), [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (the lane and its schedule), [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) (settlement
  counts anchors), [0072](0072-the-ticket-is-the-execution.md) (a block's draw is its header position).

## 0. The sentence this ADR is

**While the execution lane runs at one block a second, a panel seat risks more than it can earn on the
claim it judges, the draw gives an operator one entry however its collateral is split, no operator holds
two consecutive rounds, and a span's producers are chosen by a PALW anchor mined after the set they are
chosen from was fixed — with a missed round, never a relaxed rule, where nobody qualifies.**

## 1. What exists, and where it fails at width 1

* **A seat earns about 320 times what it risks.** Past DAA 7,001 a testnet-11 claim escrows 3,200.85 MSK;
  its panel pool is a fifth, 128.03 MSK a seat. The seat reserves `3 × claim.reserved`, and a claim reserves
  one inference's worth at the network's slash value: 5 sompi × 2,685,360 pwu = 0.134 MSK for Qwen3.6, so
  0.40 MSK a seat (the slash value read from the live node's registration terms). A seat that licenses
  anything loses at most 0.40 MSK and earns 128 MSK for being counted.
* **A bond split is a ticket bought.** Past ADR-0124 every eligible bond draws one ticket; one seat per
  operator per panel stops a quorum, not the odds: ten bonds are ten chances to be among the five.
* **"One permit an operator a round" is vacuous at width 1.** A round has one permit, so one operator can
  hold rounds N, N+1, N+2… The domain parity rule stops a *domain* holding consecutive rounds; an operator
  with bonds in both parities is not stopped.
* **The schedule's seed is the finals' claim ids and execution roots.** A producer that can choose among
  executions can choose among seeds before the set is fixed.
* **A span publishes an hour of producers.** At 30 DAA a span, every round's permit holder is computable an
  hour ahead.

## 2. Decisions

**Decision 1 — a seat reserves at least λ times what it can earn (dormant).**
`Params::palw_panel_exposure_floor: Option<PalwPanelExposureFloorV1 { activation, reward_multiple_permille }>`.
Past it a seat reserves `max(3 × claim.reserved, λ × max_seat_reward)`, where `max_seat_reward` is the
per-seat share of the pool the claim's escrow holds (work pricing only lowers it) — resolved at the claim's
anchor with the rest of the draw policy, used by the eligibility headroom and by the reservation alike, and
stored on the seat's duty so release and the dissent slash move exactly what was reserved. The panel floor
(ten producer floors: 100,000 MSK on a mainnet card) stays a separate participation floor. `None` on every
preset; not stated on a mainnet card (λ = 5–10 is under consideration there). §3 is why testnet-11 does not
arm it at 7,001.

**Decision 2 — one entry per operator.** Past `palw_panel_economy` the draw groups the eligible bonds by
operator; an operator's candidate is its eligible bond with the lowest bond ticket, and the operator draws
one ticket `H(anchor ‖ claim ‖ operator)`. Splitting collateral across bonds buys nothing; an entry needs an
operator key whose bond can reserve the seat's exposure, so more entries need more capital (with Decision 1,
λ times the seat reward each). The fingerprint names the rule (`palw_panel_draw/operator_ticket_v1`).

**Decision 3 — an operator never holds two consecutive rounds.** The schedule gives each operator one parity —
that of the domain carrying most of its credit — and drops its bonds from domains of the other parity. Rounds
alternate parity, so no operator holds adjacent rounds of a span at any width, and a round whose parity has no
eligible operator has no permit: a miss, counted, not a relaxed rule.

**Decision 4 — participants first, then a future anchor.** At the first chain block of span `n`:
the pending snapshot (quotas, parities, bonds — everything but the seed), taken at the start of span `n − 1`
from the finals of span `n − 2`, becomes span `n`'s schedule, seeded with
`H(A ‖ n ‖ safe frontier)` where `A` is the last chain block carrying an admitted attempt during span `n − 1`
— mined after the snapshot, and re-rolled only by a new winning draw (ADR-0072); then the finals of span
`n − 1` become the pending snapshot for span `n + 1`. No anchor, or a stale snapshot, is no schedule: the lane
idles that span. Credits are spendable two spans after they are earned, and a span's producers are known only
once its first block exists. No beacon. The fingerprint names the rule set (`palw_execution_lane/scheduler_v2`).

**Decision 5 — testnet-11's span is 5 DAA** (about ten minutes at 120 s).

**Decision 6 — the width stays 1.** The stage table keeps its ceiling of ten and every widening its own
fenced height; none is scheduled.

**Decision 7 — a receipt's verdict may resolve, not reverse (recorded, not built).** `Unavailable → Valid` and
`Unavailable → Invalid` within the receipt window stay honest (a restarted seat can sign both); a seat that
signs `Valid` and `Invalid` for one claim contradicts itself and should be slashable. Built after BPS 1 has run.

## 3. testnet-11's capital, measured (2026-09-17, DAA 5,774)

Read from the explorer node (`getPalwClaims`, role `seat`, each genesis bond):

| | |
|---|---|
| genesis bonds | 8, each 10,000.00 MSK |
| claims a bond is seated on, not yet terminal | 296 on bond 0; 500+ on each of the other seven (the node's row cap) |
| of those, `panel_bound` | 177–480 a bond (the receipt window is 600 DAA) |
| exposure a bond reserves for them today | 88.5–115.3 MSK |
| reservation ceiling | 500 ‰ of collateral = 5,000 MSK a bond |

With λ = 2 each seat reserves at least 256.07 MSK, so ~500 concurrent seats need ~128,000 MSK reserved a bond
— ~256,000 MSK of collateral at the 500 ‰ ceiling, twenty-five times what a genesis bond holds. Armed on today's
bonds, the draw would find too few operators with headroom, `PanelBound` could not be built, and claims would
stop reaching `Final` — and the lane, whose schedule is made of finals, would stop with them. Arming it on
testnet-11 therefore needs one of: collateral scaled to the concurrency (new bonds of ~300,000 MSK, or more
operators), a shorter claim life (the receipt window drives the concurrency), or both; it is not armed at
7,001 until the operator chooses.

## 4. Security amendments and residuals

* **SA-1 — a seat's downside is priced on the claim, not the network.** λ multiplies the claim's own seat
  share, so a class paid less reserves less, and the reservation is released exactly as reserved.
* **SA-2 — the operator key is still free.** Decision 2 prices an entry in collateral only through the
  reservation; below Decision 1's floor an extra key with a small bond is an extra entry. Decision 1 is what
  makes an entry cost capital proportional to what it can earn.
* **SA-3 — span boundaries.** Two schedules meet at a boundary; an operator may hold the last round of one
  span and the first of the next.
* **SA-4 — the anchor's producer chooses among its own winning draws.** Biasing a seed costs one winning draw
  a try, and the anchor must be the last attempt block of its span.
* **SA-5 — misses are the signal.** A span with no attempt anchor, a parity with no eligible operator, and a
  panel with too few eligible operators all fail visibly; none falls back to a weaker rule.

## 5. Deferred, by name

The panel share derived from verification cost (10–30 %); a DA/availability reward; a 33 % domain cap or a
two-round domain cooldown; widths 2–10; the base-DAG carrier; a rolling schedule that names producers only
seconds ahead (before mainnet); Decision 7's slash.

## 6. What comes next (M3, M4)

* **Settlement under attack**, as property tests and simulations: one operator with 80 % of the compute, one
  domain with 90 %, half the producers offline, two and three of five seats malicious, a permit signed twice,
  one output spent on two branches, a claim withheld before an anchor, a bond split a hundred ways, a
  producer DDoSed out of its rounds — each asserting that no number of execution blocks moves settlement depth.
* **What BPS 1 must show over weeks**, readable over RPC: rounds total/produced/missed, permit conflicts and
  double signs, operator and domain shares (p50/p95/max), consecutive-round rejections, settlement latency and
  reorgs, panel response/correct/slash rates, panel and producer reward per reserved sompi, MSK per canonical
  compute by class.

## 7. Number hygiene

0130 was free when written; the next free number is 0131.
