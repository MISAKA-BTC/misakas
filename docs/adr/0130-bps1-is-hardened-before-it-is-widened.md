# ADR-0130 — BPS 1 is hardened before it is widened

* Status: **ACCEPTED 2026-09-17; Decisions 2–6 IMPLEMENTED the same day, Decision 1 built dormant, Decisions 7–8 not built** — on `feat/palw-exec-lane-and-validator-retirement` (§7).
  Decisions 2–5 ride testnet-11's DAA 6,001 flag day with ADR-0124/0125/0126/0128. Decision 1 is built
  dormant and computed in shadow (Decision 8); the operator decided (2026-09-17) that λ is off at 6,001 and
  gets a height only once the shadow shows the capacity and a shorter liability lifetime exists (§3).
* **ADR-0144 alignment (2026-09-21).** §5 deferred widenings stay deferred under 0144 item 0. See the end section.
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

* **A seat earns one to five orders of magnitude more than it risks, and a producer more still.** Past DAA
  6,001 a testnet-11 claim escrows 3,200.85 MSK before ADR-0124's work price; the pool is a fifth, 128.03 MSK
  a seat at full price. A claim reserves one inference's worth at the network's slash value, 5 sompi a pwu
  (read from the live node's registration terms), and a seat three times that. A model class is priced by
  its pwu against the heaviest weight-bearing model class (the unit), so its reward-to-exposure ratio does
  not depend on the class, only on the unit; the floor is not priced:

  | role | model class (unit = 9,000,776 pwu: `Qwen3.8-27B/graph-v3`, share 1 ‰) | floor (7,708 pwu, unpriced) |
  |---|---|---|
  | panel seat: `128.03 MSK × price ÷ (3 × pwu × 5 sompi)` | ~95× | ~110,700× (128.03 / 0.0012 MSK) |
  | producer: `2,560.68 MSK × price ÷ (pwu × 5 sompi)` | ~5,700× | ~6,640,000× |

  The unit is the heaviest *weight-bearing* model class, and a class bears weight once its share is above
  zero: testnet-11's `Qwen3.8-27B/graph-v3` (registered at DAA 1,165, 1 ‰) is therefore the unit, pricing
  `Qwen/Qwen2.5-1.5B/graph-v5@512` (6,630,544 pwu) at 73.7 % and `Qwen3.6-35B-A3B/graph-v3` (2,685,360) at
  29.8 %. (Were the unit 6,630,544 the seat ratio would be ~129×; at 2,685,360, ~318×.) The panel side is what
  the operator asked to fix first; the producer side is the same shape and larger (Decision 8 reports both).
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
(ten producer floors: 130,000 MSK on a mainnet card since `a3c5db22`, 2026-09-24; 100,000 before) stays a
separate participation floor. Mainnet's λ is **5** (the user's 2026-09-25 decision, from the 5–10 this
paragraph recorded as under consideration): testnet-12 arms it at `reward_multiple_permille = 5_000` from
genesis; the mainnet card does not state it yet, and testnet-11 and devnet keep `None`. §3 is why
testnet-11 does not arm it at 6,001.

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

**Decision 8 — λ in shadow, and the economics in one table (node-local, no consensus).** Until λ has a
height the node computes, for the live claims, what λ = 2 would require: each seat's required exposure,
whether each operator would be eligible, whether each panel would fail to draw, each operator's capacity and
utilization; it records each claim's lifecycle (bound, licensed, final or voided, exposure released); and it
reports reward per reserved sompi for the producer and the seat by class. λ gets a height only when the shadow
shows p95 operator utilization under 70 %, eligible operators beyond the seats after excluding the executor,
would-fail draws under 0.1–1 %, receipt latency p99 well inside the new deadline, an acceptable round-miss rate,
and several complete claim lifecycles observed. Once armed λ is a fixed consensus parameter: it is never lowered
because operators are scarce — a panel that cannot draw leaves its claim short of `Final`, and scarcity is
answered with operators and collateral.

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

With λ = 2 a full-price seat reserves at least 256.07 MSK (a Qwen3.6 seat priced at 40.5 % about 104 MSK, a
floor seat 256.07 MSK), so ~500 concurrent seats need on the order of 100,000–128,000 MSK reserved a bond —
some 200,000–256,000 MSK of collateral at the 500 ‰ ceiling, twenty to twenty-five times what a genesis bond
holds. At 256 MSK a seat a 10,000 MSK bond holds ~19 seats. Armed on today's
bonds, the draw would find too few operators with headroom, `PanelBound` could not be built, and claims would
stop reaching `Final` — and the lane, whose schedule is made of finals, would stop with them. Arming it on
testnet-11 therefore needs one of: collateral scaled to the concurrency (new bonds of ~300,000 MSK, or more
operators), a shorter claim life (the receipt window drives the concurrency), or both; it is not armed at
6,001 (the operator's decision).

**Why the concurrency is 500: the liability lifetime, measured** (1,311 distinct claims read from four bonds,
newest first, at DAA 5,774):

| segment | p50 | p90 | p99 |
|---|---|---|---|
| accepted → bound | 641 | 642 | 646 — 739 of 1,292 were redrawn: the first panel did not license in 600 DAA |
| bound → licensed (the 91 licensed) | 75 | 252 | 562 |
| age of the 824 still `panel_bound` | 257 | 471 | 590 |
| accepted → `Final` (48) | 2,016 | 2,121 | 2,200 — the challenge window is 1,200 DAA before 6,000 |
| accepted → voided (320, all `receipt_timeout`) | 1,242 | 1,244 | 1,245 — two receipt windows |

By class: the 2,685,360-pwu class (Qwen3.6, 488 ‰) had 500 claims sampled and none licensed; the
6,630,544-pwu class (491 ‰) licensed at a median 82 DAA and ended 48 `Final` against 320 voided; the floor
licensed at a median 6 DAA. **The window is not the root cause — licensing is.** A 30-DAA receipt window
today would void nearly every model-class claim (licence p50 75–82 DAA, and never for Qwen3.6). The order is
therefore: find why model-class panels do not reach quorum in time; then shorten the receipt window and the
challenge window to what licensing actually takes (6,000 already cuts the challenge window to 120 DAA); then
size bonds to the concurrency that remains (the operator's first trial: ~30 DAA with 20,000 MSK panel bonds
→ ~39 seats of capacity against ~25 in use); then arm λ. The lane is exposed to the same cause: its schedule
is made of finals, so past 6,001 spans in which only the floor and the 6,630,544-pwu class finalize schedule
only their domains, and spans with no final idle.

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
  producer DDoSed out of its rounds — each asserting that no number of execution blocks moves settlement depth;
  and four the operator added: anchor grinding (withholding a last attempt block to re-roll a span's seed),
  operator-parity gaming (moving credit between domains to choose a parity), λ-shadow exhaustion (replaying a
  claim stream to the DAA each operator's free collateral runs out under λ = 2), and one top operator offline
  under the consecutive-round rule (round misses and settlement progress).
* **What BPS 1 must show over weeks**, readable over RPC: rounds total/produced/missed, permit conflicts and
  double signs, operator and domain shares (p50/p95/max), consecutive-round rejections, settlement latency and
  reorgs, panel response/correct/slash rates, panel and producer reward per reserved sompi, MSK per canonical
  compute by class.

## 7. Implementation record (2026-09-17)

Merged into `feat/palw-exec-lane-and-validator-retirement` as M1 (`fc4325d9`, the seat exposure floor and the
operator lottery) and M2 (`da837d62`, the scheduler). testnet-11's fingerprint is the union pin
`ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2`; M1 alone measured `236cdb56…`, M2 alone
`a4df92df…`, and the first 7,001 build `4787b92a…` — none of the three was deployed. The identity and the fence
schedule (…, 6000, 6001, 6201, 6900, 2125000) did not move between them, so the fork-id gate cannot tell these builds apart: the fleet
carries one pin or it forks silently at 6,001.

* **Decision 1 — built, dormant.** `Params::palw_panel_exposure_floor: Option<PalwPanelExposureFloorV1
  { activation, reward_multiple_permille }>`, hashed `Some`-only, `None` on every preset and card, refused where
  it cannot floor (`adr0130_the_seat_exposure_floor_is_dormant_everywhere_and_refused_where_it_cannot_floor`,
  `adr0130_the_exposure_floor_is_hashed_some_only_and_scheduled_like_a_fence`). Past it a seat reserves
  `max(3 × claim.reserved, λ × max_seat_reward)`, the draw's headroom asks for that amount, and the duty row
  stores what binding reserved, so the release and the dissent slash move that amount across a fence crossing
  (`adr0130_what_a_release_returns_is_what_binding_reserved_across_the_fence`,
  `adr0130_the_dissent_slash_takes_the_stored_exposure`,
  `adr0130_no_floor_is_the_shipped_reservation_and_the_shadow_ledger_prices_another`). The seat exposure, the
  eligibility, the lottery and the stored/hypothetical seat ledgers are pure public functions for a shadow read.
* **Decision 2 — at 6,001.** Past `palw_panel_economy` the draw groups the eligible bonds by operator, an
  operator's candidate is its bond with the lowest bond ticket, and each operator draws one ticket under its own
  domain; the rule is named in the fingerprint as `palw_panel_draw/operator_ticket_v1` beside the fence's height
  (`palw_panel_v2.rs`, `palw_panel_economy_v1.rs`, `palw_state_v2.rs`, `palw_state_v2_sync.rs`, the virtual
  processor). Where it does not reach (`51330459`): the stratified shard draw takes no economy, so neither
  ADR-0124's floor and headroom nor this lottery and exposure floor ride it; the operator counter the maturity
  warning prints is the lottery's unit; the seat economy carries three numbers, not two.
* **Decisions 3, 4, 5 — at 6,001.** `palw_execution_lane_v1.rs` scheduler v2: the snapshot/seed split, one
  parity an operator (`adr0130_an_operator_spread_over_both_parities_misses_one_paritys_rounds`,
  `adr0130_no_operator_holds_permits_in_two_consecutive_rounds_of_a_span`), the pending-snapshot and
  seed-anchor rows in `palw_state_v2.rs` (`adr0130_a_snapshot_is_seeded_only_by_an_anchor_recorded_after_it_was_taken`,
  `adr0130_a_gap_drops_stale_finals_and_snapshots`,
  `adr0130_the_attempt_carrying_block_that_opens_a_span_anchors_the_next_target`,
  `adr0130_the_seed_moves_with_the_anchor_span_and_frontier_and_nothing_the_finals_choose`), and the pipeline
  (`adr0130_a_span_is_seeded_at_its_first_block_from_the_anchor_of_the_span_before`,
  `adr0130_a_round_block_whose_permit_the_previous_round_held_is_not_granted`,
  `adr0125_a_finalized_attempt_schedules_the_span_two_on_and_a_permit_is_accepted_once`). The rule set
  `palw_execution_lane/scheduler_v2` rides the fingerprint and the schedule id; testnet-11's
  `schedule_span_daa` is 5 (was 30). Readers: `getPalwRoundLane` names the span its finals are for
  (`finals_span + 2`), `misaka mining status` prints "N finals toward span s+2" or "no finals gathered yet",
  the round producer's doc names the two-span delay, and the devnet drill waits for two span boundaries with a
  default span of 10 DAA (`LANE=0,2,10`). **The drill has not been re-run on the merged branch.**
* **Decision 6 —** unchanged: width 1, no widening scheduled. **Decision 7 —** recorded, not built.
  **Decision 8 —** not built: no node computes the λ = 2 table yet; ADR-0131's op 185 carries the per-class
  census (claims by state, escrow, compute), not the seat-exposure shadow.
* **Residuals the implementation named.** An operator is the registry's operator id, so the parity rule and
  the lottery inherit ADR-0125's Sybil residual (SA-2: until Decision 1 has a height, a second operator id is a
  second entry and a second parity). SA-3 stands: two schedules meet at a span boundary. The seed anchor is the
  execution key of the latest attempt-carrying chain block of the span before, chosen among that producer's own
  winning draws (SA-4). The fork-id gate does not see the rule change (above).
* **Verification of the union branch (M1 + M2 + ADR-0131's shadow, `1b0bc958`), 2026-09-17:** consensus-core
  2,133 / consensus (evm) 305 / kaspad 96 / mining 92 / misaka-cli 175 / pq-validator-core 12 + pq-validator 42 /
  rpc-core 146 / rpc-service 1 / palw-extension 2 / integration `rpc_tests::sanity_test` 1 — all green; clippy
  `-D warnings` over the twelve crates clean; `shipped_presets_have_pinned_fingerprints` and
  `the_moved_heights_refuse_the_deployed_7000_release_from_6000_and_keep_it_below` green on the union pin.

## 8. Number hygiene

0130 was free when written; the next free number is 0131.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) §6 item 0: do not widen
the current economy until accounting and admission land.

§5's deferred list stays deferred, and is no longer "after BPS 1 is hardened":

* widths 2–10, a rolling schedule, a derived panel share, a DA/availability reward, tighter domain
  caps, Decision 7's slash — each extends reward, admission or throughput. They wait on ADR-0144
  items 2–3 (and on 0146 before any new economic scalar). Decision 8's λ shadow may keep measuring;
  it does not arm a new pay table.
* Decisions 2–6 as already implemented (width 1, the scheduler, the snapshot/seed split) stand.
  Width stays 1.
