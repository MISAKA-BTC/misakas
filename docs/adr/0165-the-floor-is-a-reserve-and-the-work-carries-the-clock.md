# ADR-0165 — Useful work carries the clock: the floor is the idle-only bonded fallback (A″), a REAL attempt carries the slot's tick (B); the bonded FALLBACK-V1 block is the next fence

**Status:** PROPOSED 2026-10-03 on `rcore/real-share` (lane RS), for the DAA-5,300 flag day (the Useful Work
Transition). **Revision 3** (coordinator decisions of 2026-10-03: FALLBACK-V1 is the NEXT fence, 5,300 ships A″ + B;
and, after P2's live measurement, K = 20 slots). Consensus change behind two fences, **dormant on every shipped
preset**: `Params::palw_floor_reserve_v1` (A″) and `Params::palw_real_clock_tick_v1` (B).

**Builds on:** ADR-0060/0064 (the liveness doctrine), ADR-0066 (the heartbeat lane), ADR-0105 (heartbeat
transparency, `LaneColoring`), ADR-0138 (the anchor clock), ADR-0142 (the cursor; §9 the floor), ADR-0045/0135
(classes, registry), ADR-0149 (an attempt's pwu is its weight), ADR-0152/0160 (the share and room gates, J-1's
per-bond weight cap).

## 00. The DAA-5,300 rule: A″ + B

### 00.1 A″ — the floor is the idle-only bonded fallback (`palw_floor_reserve_v1`)

Past the fence a `PALW-BASE-0` attempt is accepted **only when the idle ledger says idle** — no REAL attempt accepted
in the last K = 20 slots; an empty ledger is idle — and is refused by name (`FloorNotIdle`) otherwise. The refusal
is in the fold's class gate (`check_class_admits_claim`), which the own attempt, a merged attempt and the producer's
pre-check (`palw_producer_facts_v2` → `class_admission_refusal` → `ready_to_produce`) all ask, so honest floor
producers hold before they spend a draw. It comes before any write and joins the pre-write skip arm, so the block
stands and **no claim is written**: no reward is escrowed (the worker carve is withheld and burned, as for every
skipped own attempt) and no weight is credited. **When the floor IS accepted (idle) it is today's bonded floor
attempt, unchanged** — the same bond, producer-floor, exposure, share and room checks, the same reservation,
weight and reward: it is the bonded fallback. A fallback-only stretch therefore has exactly today's rewrite cost;
the rule adds a refusal and relaxes none. Claims taken before the fence settle as before. A REAL attempt is
any attempt of a registered class other than the base class (the registry's lifecycle, the panel room and the
bond's share already decide whether it is accepted).

The idle ledger (`real_work`, rooted; root and carriage tail `0xEA` only when non-empty; delta variant 104) holds one
number: **the DAA of the block that last ACCEPTED a REAL attempt**, plus one. Written by `apply_attempt` for any
accepted attempt of a class other than the base, own or merged, at the accepting block's DAA.

### 00.2 The live finding, and what A″ answers

**Evidence** (P2, `lanes/evidence/8k-red-1003/`, the last 300 DAA of testnet-12): 72 attempts of the 8k class, **71
RED and 1 BLUE; every RED caused by floor attempts** (141 counted peers, all floors; heartbeats counted 0). The 8k
producer infers for 342 s at the median (p95 418 s, max 447 s — 2.9 to 3.7 slots) while floors — 3.0 a slot, 64 %
of them external miners — extend the chain.

**Mechanism.** ADR-0105's `LaneColoring::Weighted` makes heartbeats invisible to a bonded candidate; a floor
attempt is another attempt and is counted classically. Testnet-12's `ghostdag_k` is 1, so two floor-lane blocks in
an attempt's anticone make it RED, and 340 s holds about eight. The colouring is header-stage: it reads the lane,
not the class and not the ledger.

**What A″ does.** While the chain is busy (a REAL attempt accepted within K slots) the floor is refused by the
fold and by the producer's pre-check, so honest floor producers hold; only heartbeats extend the chain under the
slow attempt, and it is BLUE. `t12_real_share` runs both sides on testnet-12's own GHOSTDAG through the real
pipeline: before the fence a slow REAL attempt with two or three floors in its anticone is RED (the finding
reproduced) and with the heartbeats of one, two or three slots it is the selected parent; past the fence, busy,
with the floor refused, it is BLUE and accepted.

**What it does not do — the residual, stated.** A producer that ignores the pre-check (a rogue or unupgraded
binary) still mines floor-lane blocks, and those colour classically: the slow attempt goes RED
(`…a_rogue_floor_still_colours_classically…`). What changed is what those blocks are: valid blocks the fold skips
— **no claim, no weight, no reward** (the miner forgoes the worker carve) — so griefing costs the griefer a bond's
production and the carve. They still add the header-stage `2^20` GHOSTDAG blue work every attempt-lane header
adds (the fork-choice attack review's verdict 1, `hb_probe_verdict1_…`; not widened here).

### 00.3 K = 20 slots, from the measurement

K is **one constant**, `PALW_REAL_IDLE_K_SLOTS_V1` (hashed with the fence, so changing it is a new network id), and it
is checked against its derivation: `K = ⌈p95 gap between accepted REAL attempts / slot⌉ + margin` =
`⌈17.3⌉ + 2 = 20` (`palw_real_idle_k_for_v1`; the margin is one slot for the accepting block's lag — a REAL
attempt is accepted at the block that merges it, after it was drawn — and one for rounding and jitter). The gap's p50
is 3.1 slots, p95 17.3 and max 30.6 (the long gaps are panel and backpressure holds). **The inference (3.5 slots at
p95) is inside the gap** — the gap runs from one acceptance to the next, and the next attempt is drawn within it — so
the inference is not added to it. K is capped at 30 slots (one hour, the merge-depth duration) so that no
measurement can ask for a longer unbonded stretch.

**The condition K buys.** A floor lands in a REAL attempt's anticone only if it was produced between the attempt's
template and its landing, and is refused if the previous REAL acceptance was within K slots of it. So **a REAL
attempt that lands within K slots of the previous acceptance meets no floor under it.** At P2's distribution that is
every attempt but the ≤ 5 % whose gap exceeds the p95. (Cross-check on the evidence TSV, gaps by the merging block's
DAA between consecutive attempts: 70 gaps, p50 3, p95 13, max 23; **2 of 70 (2.9 %) exceed 20 slots** and 1 exceeds 22 — so
on the same 300 DAA at most those two attempts and the first of the run would have met floors under A″, against 71 of 72
today.) **The first REAL attempt after a gap longer than K can still
go RED**: the floor resumed and extended the chain under it. It is accepted all the same — a RED attempt is
applied (00.5) — and its acceptance holds the floor again, so the next one is BLUE
(`…after_an_idle_stretch_longer_than_k…`, which walks exactly K slots).

### 00.4 The price

* **After REAL work stops, up to K = 20 slots (40 minutes) are heartbeat-only** — unbonded, weight ε a block —
  before the bonded floor resumes. A transaction confirmed only inside that stretch is as exposed as inside any
  heartbeat-only stretch today (`hb_probe_b_*`, `hb_probe_d_only_the_finality_depth_stops_a_heartbeat_only_reorg`):
  a rewriter needs more beats a slot than the honest chain and no bond, until the finality depth or a Settlement Anchor
  stops it. A REAL attempt accepted before the stretch sits in both forks' common history if the fork starts after it,
  so it cannot be dropped without out-weighing it. The stretch is bounded by K (and K by the cap), not open-ended.
* **While REAL work flows, the chain is better secured than by floors, not worse.** An 8k claim weighs
  1,388,926,972,416 pwu against a floor's 6,042,506,112 (229.8×). Three floors a slot is 18.1e9 a slot; the 8k stream at
  its median gap is 448e9 a slot (24.7×) and even at its p95 gap 80e9 a slot (4.4×).
* **Weight concentrates in the REAL producers' bonds** while it flows, because the floors that used to add to it are
  refused. That is the intent (the floor is a fallback), and it is the cost: the rewrite cost of a busy stretch is the
  REAL claims' weight, bonded and panel-verified, which a rewriter must out-weigh with REAL work of its own.
* **The keep-alive, priced and accepted here.** A producer holding a bond and any Active REAL class can keep the
  floor refused indefinitely by landing one REAL attempt every K − 1 slots; the chain's weight then grows by at least
  that attempt's weight and the stretch is *not* heartbeat-only (each REAL attempt is bonded and panel-verified), but a
  cheap class could make that weight small. **Open, recommended follow-up (a decision, not done here):** note the ledger
  only for a REAL attempt whose pwu is at least a stated multiple of the floor's, so a token class cannot suppress
  the floor. Without it the guard is the class's registration burn, its panel verification and the bond's capacity.
* **Withholding REAL work to force the fallback gains nothing:** a REAL producer that withholds goes idle after K
  slots, from where the floor — bonded weight, reward as today — is what it could have earned anyway, and it forgoes
  the REAL attempts' weight and reward.

### 00.5 Reds count: the idle ledger hears of a RED REAL attempt

`palw_v2_merged_works` returns the work of the whole mergeset — blues and reds alike, the selected parent excluded
(its own work is applied by its own fold); its comment: "at `ghostdag_k = 1`, any block whose anticone holds two or more
blocks is a red by construction … A blues-only rule would measure nothing". A RED REAL attempt is therefore applied
by the merging block's fold (`apply_attempt` → the ledger note, at the merging block's DAA), exactly as a BLUE one.
Tested through the pipeline (`…a_rogue_floor…` and `…after_an_idle_stretch_longer_than_k…`: the attempt is RED, its
claim exists, the ledger holds the merging block's DAA, the floor is held again). A non-DAA block (outside the window or a
round block) is the one exclusion, as it is for the coinbase.

### 00.6 B — a REAL attempt carries the slot's tick (`palw_real_clock_tick_v1`)

**What cannot be done, and what is done instead.** The DAA score is derived at **header** processing from the window and
the mergeset's HEADERS (ADR-0142 §6a: no stored cursor, so a pruned and an archival node agree). A block's class and bond
live in its body and are judged at the virtual stage. A tick conditioned on "an accepted REAL attempt" would make a
header-stage value depend on virtual state — circular, and divergent between a node holding the bodies and one that
does not. So the consensus tick source is the **lane**, which a header states: an attempt-lane block
(`is_palw_attempt_algo_id`: algos 6 and 9), beside the heartbeat lane. The preference for REAL work is the product of
A″ (no honest producer mines a floor while a REAL attempt is recent) and of the miner's policy, not of a clock rule.

**The rule.** In `palw_clock_step_v1`, past the fence: a mergeset with nothing `bits` prices and at least one tick source
(a heartbeat or an attempt-lane block) is `granted` when the NEWEST source is stamped at or past the cursor's slot.
`granted` removes exactly ONE exemption (`palw_clock_tick_source_v1`), so a mergeset advances the DAA by one however many
attempts it holds — *never each attempt +1*. The cursor, the reference (earliest tied step), H3, H5 and the lead cap are
unchanged; the lead cap now also covers an attempt-lane header. **Node policy**: `heartbeat_slot_hint_v1` — the heartbeat
miner holds until `slot + 20 s` (`PALW_REAL_TICK_GRACE_MS_V1`) for an attempt to carry the tick, and then mints as before.

| property | holds because |
|---|---|
| a producer cannot **speed up** the clock | a tick needs a source stamped ≥ the cursor's slot and the step that consumes it is stamped ≥ that slot (H5) and ≤ receiver clock + 132 s (lead cap): references are ≥ one interval apart. Attempts are cheap at the header stage and a heartbeat costs 2^24, but cost never bounded the RATE — the cursor does. A burst is `⌊132/120⌋ + 1 = 2` ticks, once. `a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner` simulates adversarial mixes of up to 100 attempts and 3 beats per instant. |
| cannot **stall** it beyond a heartbeat miner | the reference is the earliest tied step; any honest block merging a granted source is a step. |
| one tick per slot however many attempts | `granted` removes one exemption: `a_hundred_attempts_in_one_slot_advance_the_score_by_one`; through the pipeline, `…a_real_attempt_carries_its_slots_tick_and_a_slot_ticks_once` (an attempts-only chain: one tick a slot with no heartbeat anywhere; the unarmed control ticks not at all). |
| DAA-denominated windows keep wall meaning | the DAA still moves at most once per 120 s. |
| reorg / pruned node | `attempt_ticks`, `granted` and the cursor are functions of the block's own parents and window; no new state; headers only. |

### 00.7 Node and RPC

* `getBlock` verboseData carries **`blockKind`** (JSON only; the borsh wire and the gRPC proto are unchanged): `REAL` (a
  non-floor attempt), `FALLBACK` (a floor attempt past the fence), `LEGACY_FLOOR` (before it), `LEGACY_HEARTBEAT` (algo
  8), `EXEC` (a round block), empty otherwise (`palw_block_kind_v1`, tested against a real `palw_commitment`).
* **Drill:** `--palw-drill-useful-work-at=H` arms both fences at a low height on a salted chain; and
  **`--palw-drill-real-submit-delay-s[=N]`** holds a REAL (non-floor) attempt's submission until N seconds after its
  template (default 340 s, the 8k p50; 1..=3,600; refused without the salt and re-checked against the chain the node runs),
  to emulate an 8k inference's minutes on a drill-sized class.

### 00.8 Tests

`consensus/core` — `palw_real_share_v1` (K and its derivation and cap, idle detection, the tick-source rule, the clock-speed
simulation), `real_work_reserve_v1` (busy refuses the floor, idle accepts it and it carries weight at Final, a fallback claim
is bonded exactly like today's floor claim, claims won before the fence settle, an all-floor network is idle and live,
reorg determinism of every delta, the carriage), `palw_useful_work_fences` (dormant on every preset, fingerprint and fork
id, prerequisites). Pipeline, on testnet-12's own GHOSTDAG — `t12_real_share` (00.2, 00.3, 00.5, 00.6). `kaspad` — the
drill flag (refusals, the wait rule, wiring). `rpc-service` — `blockKind`.

## 0. Where the implementation stands

| piece | state |
|---|---|
| **B** — a REAL attempt carries the slot's tick, one tick per slot (`palw_real_clock_tick_v1`) | built, tested (00.6) |
| **A″** — the floor is the idle-only bonded fallback, K = 20 (`palw_floor_reserve_v1`) | built, tested (00.1–00.5) |
| **FALLBACK-V1** — the bonded fallback block, its weight, reward, cap, idle rule, retiring the heartbeat | **designed here (§5–§7), the next fence, not built**: §8 |

## 1. The defect

The 600 blocks of testnet-12 at DAA 3,939–4,071: 67 % attempts, 33 % heartbeats, and almost every attempt the
`PALW-BASE-0` floor class. Two block kinds exist to keep the chain alive — the floor attempt and the heartbeat — and
together they were two thirds of the chain. Neither is a model. And the floor was not only idle weight: it buried the
models that did run (00.2).

## 2. The block kinds

| kind | what it is | claim / panel / court | clock | weight | reward |
|---|---|---|---|---|---|
| **REAL** | an attempt of a registered, Active class other than `PALW-BASE-0` | yes (unchanged) | carries the slot's tick (B) | its pwu (ADR-0149) | unchanged |
| **EXEC** | the execution lane's round block (ADR-0125) | permit | none (outside the DAA set) | unchanged | unchanged |
| **FALLBACK** (5,300) | a floor attempt past the fence, accepted only while idle (A″) | yes — it IS the floor claim | carries the tick like any attempt | today's floor weight | today's |
| **FALLBACK-V1** (next fence) | a bonded, idle-only block that is not a model class (§5) | **none** | carries the slot's tick when nothing REAL did | bond-derived `w_fb`, one per bond per slot | fee-only |
| **LEGACY_HEARTBEAT / LEGACY_FLOOR** | the pre-fence heartbeat / floor attempt | as before | as before | as before | as before |

## 3. A′ — superseded by A″

Revision 2's A′ retired the floor outright: a `PALW-BASE-0` attempt past the fence was refused whatever the ledger said.
It is not safe alone — with few REAL producers a stretch would be heartbeat-only (unbonded, ~0 weight, cheap to rewrite) —
so 5,300 ships A″ (00.1): the floor stays, bonded, as the fallback, and is refused only while REAL work is recent.
Revision 1's hysteresis ledger (dormant at ≥ 12 Final real claims in 60 DAA) was withdrawn earlier for a single number, the
DAA of the last accepted REAL attempt.

## 4. B — see 00.6

## 5. FALLBACK-V1 (the next fence; design, not built)

A **block kind**, not a model class: no claim, no panel, no court, no TIR. It is the heartbeat lane (algo 8, its fixed 2^24
canonical hash puzzle, ε `blue_work`, declared subsidy 0, the stamp/slot/lead-cap rules of ADR-0142) with three additions and
one retirement:

1. **Bond-required.** The block carries a fallback envelope `{ version, operator bond outpoint, pubkey, ML-DSA-87 signature
   over (domain "MISAKA-FALLBACK-V1", selected parent, DAA score, bond) }`, verified statelessly against the embedded pubkey
   (a malformed or unsigned algo-8 block is invalid past the fence — **the heartbeat's retirement**). The fold checks the bond:
   registered, the pubkey is the bond's, not `Retiring`, not frozen, `palw_bond_may_take_work_v2` — *exactly the attempt
   producer's eligibility*. It reuses the attempt signature context; a **new** context is not an option, because
   `signature_contexts_root` is part of testnet-12's ruleset id and adding one would re-mint the network.
2. **Idle-only.** Valid for weight and reward only when the idle ledger of 00.1 says idle (K = 20), read from the block's own
   parent state. A fallback that fails any fold check is *skipped*; its header-level tick is unaffected (00.6: lane-based).
3. **Capped.** One credited fallback block per bond per slot (J-1's per-bond cap restated), in a rooted `bond → last credited
   DAA` map.

## 6. Weight, reward and the order of kinds (FALLBACK-V1)

* **Credit.** A valid fallback adds `w_fb` to a rooted `fallback_weight`, and the economic fork-choice key becomes `(safe_frontier,
  safe_weight + fallback_weight, immature)` — a chain whose frontier advances beats a fallback-only chain on the first key.
  `w_fb` is one floor claim's canonical weight (604,250,611, ADR-0160 Appendix A).
* **REAL > FALLBACK by construction.** A REAL attempt's credited weight is `max(its pwu-derived weight, 2·w_fb)`.
* **Reward.** Fee-only; REAL pays its subsidy share. **Useful-work accounting: 0.**

## 7. Security analysis (FALLBACK-V1)

* **A fallback-only stretch is as expensive to rewrite as today's floor stretch**: it weighs `S·B·w_fb` for `B` bonds credited a
  slot, each eligible exactly as a producer and credited once a slot — the same collateral and the same time as the honest
  stretch, and not cheaper by any hash trick: the puzzle is the heartbeat's fixed 2^24, which secures nothing by itself
  (t12's PoW target is 1); the security was always the bonds.
* **Withholding REAL work to force the fallback gains nothing:** it gives up REAL's weight and reward for strictly less of both.
* **Free tick sources** can move the DAA at most once per slot (00.6); bond, idleness and cap are fold rules about WEIGHT.
* **Reorg determinism.** Idle, bond eligibility, the per-bond slot and the credit are functions of the parent state and the block;
  each write is a delta entry reverted exactly.

## 8. What FALLBACK-V1 costs

Three rooted fields (the idle ledger exists; `fallback_weight` and the per-bond map are new: root, carriage tails, delta
variants), a stateless envelope check on algo-8 headers past the fence (`palw_commitment` is hashed only for PALW algos, so either
the envelope rides in the coinbase payload or the hashing gate widens behind the fence), the fold step and its refusals, the
fork-choice key change, the `max(…, 2·w_fb)` weight floor, a producer and miner change, tests and a drill; about 20 hours and two
re-pins. It is the next fence by decision; 5,300 is a sound release without it (00.1–00.6).

## 9. Prerequisites and the release

`validate_palw_useful_work_v1`: A″ needs ConsensusV2 and `palw_model_registry` at or below it, and the bundle's mirror
(`sync_palw_floor_reserve_v1`); B needs ConsensusV2 and `palw_anchor_clock`, `palw_single_lottery`, `palw_clock_cursor`,
`palw_clock_floor` and `palw_clock_lead_cap` at or below it. Release entries: `PALW_T12_FLOOR_RESERVE_ENTRY`,
`PALW_T12_REAL_CLOCK_TICK_ENTRY` (`PALW_T12_USEFUL_WORK_FENCES_V1`); the fence name `palw_floor_reserve_v1` is kept for A″.
State: delta variant 104, carriage tail `0xEA`. K and the ledger are in the fingerprint (`palw_floor_reserve_value_v1`).
