# ADR-0165 — Useful work carries the clock: the floor is retired, the heartbeat becomes a bonded FALLBACK block

**Status:** PROPOSED 2026-10-03 on `rcore/real-share` (lane RS), for the DAA-5,300 flag day (the Useful Work
Transition). **Revision 2** (user decision of 2026-10-03 ~13:00): revision 1 made `PALW-BASE-0` a *reserve* that
woke when real work stalled, and kept the heartbeat. Revision 2 replaces both with ONE fallback block kind.
Consensus change behind fences, **dormant on every shipped preset**.

**Builds on:** ADR-0060/0064 (the liveness doctrine), ADR-0066 (the heartbeat lane), ADR-0138 (the anchor
clock), ADR-0142 (the cursor; §9 the floor), ADR-0105, ADR-0045/0135 (classes, registry), ADR-0149 (an
attempt's pwu is its weight), ADR-0152/0160 (the share and room gates, J-1's per-bond weight cap).

## 00. The DAA-5,300 rule: A″ + B (coordinator decision of 2026-10-03, supersedes the A′ row below)

FALLBACK-V1 (§5–§8) is the **next fence**, not 5,300. A′ alone (floor retired outright) is not safe: with few REAL
producers a stretch would be heartbeat-only — unbonded, ~0 weight, cheap to rewrite. So 5,300 ships:

* **A″** — past `palw_floor_reserve_v1`, a `PALW-BASE-0` attempt is accepted **only when the idle ledger says idle**
  (no REAL attempt accepted for K = 3 slots; absent = idle) and refused by name (`FloorNotIdle`) otherwise, before any
  write, so a busy chain's floor attempt writes no claim: no reward, no weight. **When accepted it is today's bonded
  floor attempt, unchanged** — the same bond/producer-floor/exposure/share/room checks, the same reservation, weight and
  reward — i.e. it *is* the bonded fallback. Hence a fallback-only stretch has exactly today's rewrite cost (§7's argument
  with `w_fb` = today's floor weight, no new mechanism); the rule only adds a refusal and relaxes none. Claims taken
  before the fence settle as before.
* **B** — unchanged (§4): a REAL attempt carries the slot's tick; the heartbeat waits 20 s and remains the last-resort
  liveness block.
* **`blockKind`** in `getBlock` verboseData (JSON; the borsh wire is unchanged): `REAL` (non-floor attempt), `FALLBACK`
  (floor attempt past the fence), `LEGACY_FLOOR` (before it), `LEGACY_HEARTBEAT` (algo 8), `EXEC` (round block).
* Tests: busy → floor refused; idle → floor accepted and carries weight at Final; bonded-identical claim with and without
  the rule; reorg determinism (every delta reverts/re-applies to the same root); old floor claims settle past the fence.

## 0. Where the implementation stands (honest status)

| piece | state |
|---|---|
| **B** — a REAL attempt carries the slot's tick, one tick per slot (`palw_real_clock_tick_v1`) | built, tested (§4) |
| **A″** — see §00 (A′ below is superseded: the floor is the idle-only bonded fallback, not retired) | built (§3); revision 1's reserve/ledger machinery is replaced by the idle ledger below |
| **FALLBACK-V1** — the bonded fallback block, its weight, reward, cap, idle rule, retiring the heartbeat | **designed here (§5–§7), not yet built**: see §8 for what it costs and what it needs decided |

## 1. The defect

The 600 blocks of testnet-12 at DAA 3,939–4,071: 67 % attempts, 33 % heartbeats, and almost every attempt the
`PALW-BASE-0` floor class. Two block kinds exist to keep the chain alive — the floor attempt and the
heartbeat — and together they were two thirds of the chain. Neither is a model.

## 2. The block kinds past the fence

| kind | what it is | claim / panel / court | clock | weight | reward |
|---|---|---|---|---|---|
| **REAL** | an attempt of a registered, Active class other than `PALW-BASE-0` | yes (unchanged) | carries the slot's tick | its pwu (ADR-0149), never below `2·w_fb` (§6) | unchanged |
| **EXEC** | the execution lane's round block (ADR-0125) | permit | none (outside the DAA set) | unchanged | unchanged |
| **FALLBACK** | a bonded, idle-only liveness block (§5) | **none** — not a model class, outside PALW | carries the slot's tick when nothing REAL did | bond-derived `w_fb`, one per bond per slot | fee-only (zero subsidy) |
| **LEGACY_HEARTBEAT / LEGACY_FLOOR** | the pre-fence heartbeat / floor attempt | as before | as before | as before | as before |

## 3. A′ — the floor is retired (`palw_floor_reserve_v1`)

Past the fence the fold's class gate (`check_class_admits_claim`, the one gate the own attempt, a merged attempt
and the producer's pre-check all ask) refuses a `PALW-BASE-0` attempt with `FloorRetired`. The refusal is before
any write and joins the pre-write skip arm, so the block stands and **no claim is written**: no reward is escrowed
(the worker carve is withheld and burned, as for every skipped own attempt) and no weight is credited. Claims
the floor won before the fence run their whole life — panels, escrow, Final, payout, court — unchanged: the class
stays registered so they can settle, and nothing reaches back. The producer's pre-check reads the same gate, so an
honest floor producer holds instead of mining a block that earns nothing.

*(Revision 1's hysteresis ledger — dormant at ≥12 Final real claims in 60 DAA, active below 3 — is withdrawn: with
no floor to switch back on, the only question left to the chain is "has REAL work been accepted lately?", which the
FALLBACK idle rule (§5.2) answers from the same rooted ledger.)*

## 4. B — one REAL attempt carries the slot (`palw_real_clock_tick_v1`)

**What cannot be done, and what is done instead.** The DAA score is derived at **header** processing from the DAA
window and the mergeset's HEADERS (ADR-0142 §6a: no stored cursor, so a pruned and an archival node agree). A
block's class and bond live in its body and are judged at the virtual stage, after the header. A tick conditioned on
"accepted REAL" would make a header-stage value depend on virtual state — circular, and divergent between a node
holding the bodies and one that does not. So the consensus tick source is the **lane**, which a header states: an
attempt-lane block (`is_palw_attempt_algo_id`: algos 6 and 9), beside the heartbeat lane. The preference for REAL work is
the product of A′ (no honest producer mines a floor attempt) and of the fallback's idle rule (§5.2), not of a
consensus clock rule.

**The rule.** In `palw_clock_step_v1`, past the fence: a mergeset with nothing `bits` prices and at least one tick
source (a heartbeat/fallback block **or an attempt-lane block**) is `granted` when the NEWEST source is stamped at or
past the cursor's slot. `granted` removes exactly ONE exemption (`palw_clock_tick_source_v1`), so the mergeset
advances the DAA by one however many attempts it holds — *never each attempt +1*. The cursor, the reference
(earliest tied step), H3, H5 and the lead cap are unchanged; the lead cap now also covers an attempt-lane header.

**Node policy.** `heartbeat_slot_hint_v1`: past the fence the liveness miner holds until `slot + 20 s`
(`PALW_REAL_TICK_GRACE_MS_V1`) for an attempt to carry the tick before it mints.

### 4.1 Clock safety, restated for the new source

| property | holds because |
|---|---|
| a producer cannot **speed up** the clock | a tick needs a source stamped ≥ the cursor's slot and the step that consumes it is stamped ≥ that slot (H5) and ≤ receiver clock + 132 s (lead cap): references are ≥ one interval apart. Attempts are cheap at the header stage and a heartbeat costs 2^24, but cost never bounded the RATE — the cursor does. A burst is `⌊132/120⌋ + 1 = 2` ticks, once. `a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner` simulates adversarial mixes of up to 100 attempts and 3 beats per instant over 300×(40..120) steps with the fence on and off. |
| cannot **stall** it beyond a heartbeat miner | the reference is the earliest tied step; any honest block merging a granted source is a step. |
| one tick per slot however many attempts | `granted` removes one exemption: `a_hundred_attempts_in_one_slot_advance_the_score_by_one`. |
| DAA-denominated windows keep wall meaning | the DAA still moves at most once per 120 s. |
| reorg / pruned node | `attempt_ticks`, `granted` and the cursor are functions of the block's own parents and window; no new state; headers only. |

## 5. FALLBACK-V1 (design; not yet built)

A **block kind**, not a model class: no claim, no panel, no court, no TIR. It is the heartbeat lane (algo 8, its
fixed 2^24 canonical hash puzzle, ε `blue_work`, declared subsidy 0, the stamp/slot/lead-cap rules of ADR-0142) with
three additions and one retirement:

1. **Bond-required.** The block carries a fallback envelope `{ version, operator bond outpoint, pubkey,
   ML-DSA-87 signature over (domain "MISAKA-FALLBACK-V1", selected parent, DAA score, bond) }`, verified statelessly
   against the embedded pubkey (a malformed or unsigned algo-8 block is invalid past the fence — **this is the heartbeat's
   retirement**: an unbonded heartbeat is no longer a block). The fold checks the bond: registered, the pubkey is the bond's,
   not `Retiring`, not frozen, `palw_bond_may_take_work_v2` — *exactly the attempt producer's eligibility*
   (`apply_attempt`'s checks). It reuses the attempt signature context; a **new** context is not an option, because
   `signature_contexts_root` is part of testnet-12's ruleset id and adding one would re-mint the network.
2. **Idle-only.** Valid for weight and reward only when no REAL attempt was accepted in the last `K` clock slots, read
   from the block's own parent state (§5.2). A fallback that fails any fold check is *skipped* (no weight, no reward), as a
   skipped attempt is; its header-level tick is unaffected (§4: the tick is lane-based).
3. **Capped.** One credited fallback block per bond per slot (J-1's per-bond cap restated: a bond buys at most `w_fb` of fork
   weight per slot), kept in a rooted `bond → last credited DAA` map.

**5.2 The idle rule.** A rooted ledger holds the DAA at which a REAL attempt (a registered, Active class other than
`PALW-BASE-0`) was last *accepted* (written as a claim) — a function of the chain, not of any clock. `idle(now) = now −
last ≥ K`, and absent = idle (a chain that never saw real work is idle, so an all-floor network crossing the fence is live).
**K = 3 clock slots (6 minutes)**: long enough that a REAL producer whose class win rate is a few a slot is never idle
between its own attempts (a REAL block in a slot makes the next three fallback-ineligible), short enough that a stall costs at
most 6 minutes of fallback weight. K does **not** bound the clock: the tick is lane-based, so the DAA moves in the idle
window regardless; K only decides when fallback WEIGHT starts.

## 6. Weight, reward and the order of kinds

* **Credit.** A valid fallback adds `w_fb` to a rooted `fallback_weight`, and the economic fork-choice key becomes
  `(safe_frontier, safe_weight + fallback_weight, immature)` — so a chain whose frontier advances (a Final REAL claim) beats a
  fallback-only chain on the first key, and two fallback-only chains compare by bond-derived weight. `w_fb` is one floor
  claim's canonical weight (604,250,611, ADR-0160 Appendix A), so a fallback stretch weighs what today's floor stretch does.
* **REAL > FALLBACK by construction.** A REAL attempt's credited weight is `max(its pwu-derived weight, 2·w_fb)`.
* **Reward.** Fee-only (declared subsidy 0, as the heartbeat's); REAL pays its subsidy share. **Useful-work accounting: 0.**

## 7. Security analysis

* **A fallback-only stretch is as expensive to rewrite as today's floor stretch.** Today a stretch of N floor attempts
  weighs `N·FCW` and each attempt needs an eligible bond and, under J-1, bond capacity. A fallback-only stretch of S slots weighs
  `S·B·w_fb` for `B` bonds credited per slot, with `w_fb = FCW`, each bond eligible exactly as a producer and credited at most
  once a slot. A rewriter must out-credit that: `B` bonds of the same eligibility and `S` slots of production — the same
  collateral and the same time as the honest stretch, and **not cheaper by any hash trick**: the puzzle is the same fixed 2^24
  the heartbeat already used, which secures nothing by itself (t12's PoW target is 1) — the security was always the bonds, and
  it still is. A reorg rewriting fallback-only history cannot exceed the honest `B` per slot without more bonds.
* **Withholding REAL work to force FALLBACK gains nothing.** A REAL producer that withholds goes idle after K slots; from
  there it can mint fallback blocks worth `w_fb` per slot per bond (< any REAL attempt's `2·w_fb`) and zero subsidy: it gives up
  REAL's weight and reward for strictly less of both. A cartel that withholds all REAL work lowers the chain's weight growth
  and cannot raise its own share of it.
* **Free tick sources.** An attempt-lane or algo-8 header is cheap at the header stage; as a tick source it can move the DAA at
  most once per slot (§4.1), which is all a heartbeat miner could. Bond, idleness and cap are fold rules about WEIGHT, so
  spam earns nothing and the tick it supplies is one the cursor would have granted anyway.
* **Reorg determinism.** Idle, bond eligibility, the per-bond slot and the credit are functions of the parent state and the
  block; each write is a delta entry reverted exactly.
* **Liveness.** All REAL work stops → idle after K slots → bonded fallbacks keep a weighted chain and the clock. All bonds
  unusable → the clock still ticks on any algo-8 block (lane-based), as the heartbeat's did.

## 8. What FALLBACK-V1 costs, and what is needed to close it

It adds: three rooted fields (idle ledger, `fallback_weight`, per-bond map: root, carriage tails, delta variants), a stateless
envelope check on algo-8 headers past the fence (header identity: `palw_commitment` is hashed only for PALW algos, so either
the envelope rides in the coinbase payload or the hashing gate widens behind the fence), the fold step and its refusals, the
fork-choice key change, the `max(…, 2·w_fb)` weight floor, a producer and a miner change (mint only when the node's own
view is idle; retire the unbonded heartbeat), `blockKind` in RPC, tests (idle detection, REAL-beats-FALLBACK order, an all-idle
network's liveness, no new floor claims while old ones settle, reorg determinism) and a drill. With the Mac at the load it is
at, a full build/test cycle is 20–30 minutes; the estimate is 20+ hours of work and 2 full re-pins. **If it cannot close by
the freeze, A′ + B ship alone and are a sound release by themselves** (every property of §4.1 and §3 holds without
FALLBACK; the heartbeat then remains the liveness block, unbonded as today).

## 9. Prerequisites and the release

`validate_palw_useful_work_v1`: A′ needs ConsensusV2 and `palw_model_registry` at or below it, and the bundle's mirror;
B needs ConsensusV2 and `palw_anchor_clock`, `palw_single_lottery`, `palw_clock_cursor`, `palw_clock_floor` and
`palw_clock_lead_cap` at or below it. Release entries: `PALW_T12_FLOOR_RESERVE_ENTRY`, `PALW_T12_REAL_CLOCK_TICK_ENTRY`
(`PALW_T12_USEFUL_WORK_FENCES_V1`); the drill flag is `--palw-drill-useful-work-at`. State: delta variant 104, carriage tail `0xEA`.
