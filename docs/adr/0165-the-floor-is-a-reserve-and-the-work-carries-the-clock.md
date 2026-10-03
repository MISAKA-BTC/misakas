# ADR-0165 — The floor is a reserve, and the work carries the clock (the Useful Work Transition)

**Status:** PROPOSED 2026-10-03 on `rcore/real-share` (lane RS), for the DAA-5,300 flag day.
**Consensus change, two fences, both DORMANT on every shipped preset** until the release list arms them:
`Params::palw_floor_reserve_v1` (A) and `Params::palw_real_clock_tick_v1` (B). Lane PL's panel-liveness
rules (ADR-0166+) travel in the same flag day and are not part of this record.

**Builds on:** ADR-0060/0064 (the liveness doctrine), ADR-0066 (the heartbeat lane), ADR-0138 (the anchor
clock), ADR-0142 (the cursor, §9 the floor), ADR-0105 (the miner's yield), ADR-0045/0135 (classes,
registry), ADR-0149 (an attempt's pwu is its weight), ADR-0152/0160 (the share and room gates).

## 1. The defect

The 600 blocks of testnet-12 at DAA 3,939–4,071: 67 % attempts, 33 % heartbeats, and almost every attempt
the `PALW-BASE-0` floor class (`f1c5635c…`). One producer made the only Qwen2.5-8k blocks. The floor class
exists so a network can always produce; it was never meant to be what the network is for. Two rules
together keep it that way:

1. nothing pays the floor while real-model work is being finalised, and
2. the heartbeat — a block that exists only to move the clock — is minted only for a slot no work carried.

## 2. A — the floor is a reserve (`palw_floor_reserve_v1`)

### 2.1 The rule

Past the fence the fold's class gate (`check_class_admits_claim`, the one gate the own attempt, a merged
attempt and the producer's pre-check all ask) refuses a `PALW-BASE-0` attempt with `FloorDormant` while the
reserve is **dormant**. The refusal is before any write and joins the pre-write skip arm, so the block
stands and **no claim is written**: no reward is escrowed (the worker carve is withheld and burned, as for
every skipped own attempt) and no weight is credited (weight is the claim). Claims the floor won while the
reserve was active run their life and are paid and weighed as before — the rule never reaches back.

### 2.2 The mode, integer-only and with hysteresis

A rooted ledger `real_work` (`BTreeMap<u64, u64>`; root and carriage tail `0xEA` only when non-empty):

* bucket `daa / 10` → the number of **Final real-class attempt claims** whose Final fell in that bucket.
  *Real class* = any class other than the base class that held a claim through admission, which already
  required the registry's lifecycle `Active`, the panel room and the bond's share. A free-prompt or
  evaluation claim is not counted (the floor competes with attempts);
* two sentinel keys carry the mode (`1` dormant, absent = active) and the last bucket evaluated.

At the first block of each bucket (the fold's step 1e, before the sweeps and before any attempt) the mode
is re-evaluated over the **completed** window of the six buckets before it (60 DAA, two hours of clock):

```
active  → dormant   when  finals ≥ 12        (0.20 a DAA)
dormant → active    when  finals <  3        (0.05 a DAA)
```

between the two it keeps whatever it was. The mode is constant inside a bucket, so a block, its merged
blues and a producer reading the tip all see one answer; `palw_real_work_dormant_at_v1` is the pure read
(a producer, not yet in a new bucket, evaluates what the fold will) and `palw_real_work_roll_v1` the
writes. Each write is a delta entry (`RealWork`), so a reorg reverts it exactly like every other field.

**Why these numbers.** A Final trails the work it records by the replay and panel time, measured at 7–50
minutes on testnet-12 (4–25 DAA), so the window must hold several lags — 60 DAA — or a healthy producer
reads as stalled between its own Finals. The 4× gap is the hysteresis: the rate that switches the floor
off has to fall to a quarter before it returns, so a rate sitting on a threshold cannot flap the reserve
once per bucket. **The reserve starts ACTIVE** (an absent ledger is active): the flag day itself switches
nothing off, and the floor is dormant only after real work has proven a rate. The constants are hashed
with the fence's height, so changing one is a new network id, not a silent edit.

### 2.3 Threat analysis

* **A cheap class holds the reserve dormant.** One bond, one registered class and 12 Finals an hour switch
  the floor off. They cost: registration and its burn, collateral reserved per claim, escrowed reward at
  risk of the court, and a panel replay each. What they buy is the end of the floor's reward, not the
  chain's liveness (the heartbeat remains the clock, §3), and the floor is only a reserve. Left as an
  accepted, priced risk; if it matters the threshold can count distinct bonds in a later fence.
* **Flapping to grind the floor in.** The window and the 4× gap make a transition cost ≥ 3 buckets of
  Finals; the fold's own claim lifecycle bounds how quickly a producer can manufacture them.
* **Reorg determinism.** The mode is a function of the parent ledger and the block's DAA only; every write
  is a delta entry; there is no wall clock and no walk. `a_producers_read_equals_the_folds_roll_then_read`
  property-tests the read/roll agreement over generated ledgers.
* **Liveness.** If every producer is a floor producer the ledger stays empty and the reserve is active:
  the all-floor network is unchanged by the fence. If real work stalls, the reserve returns within the
  window.

## 3. B — the heartbeat is the clock's fallback carrier (`palw_real_clock_tick_v1`)

### 3.1 What cannot be done, and what is done instead

The DAA score is derived at **header** processing from the DAA window and the mergeset's HEADERS
(ADR-0142 §6a: no stored cursor, so a pruned node and an archival node agree). A block's class lives in
its body and is judged at the virtual stage, after the header. A tick conditioned on "accepted real-class"
would make a header-stage value depend on virtual state — circular, and divergent between a node holding
the bodies and one that does not. So the consensus tick source is the **lane**, which a header states: an
attempt-lane block (`is_palw_attempt_algo_id`, algos 6 and 9). The real-model preference is not a
consensus clock rule; it is the product of A (a floor attempt earns nothing while real work flows, so no
honest producer mines one) and the miner's policy below.

### 3.2 The rule

In `palw_clock_step_v1`, past the fence and under the cursor: a mergeset with nothing `bits` prices and at
least one tick source — a heartbeat **or an attempt-lane block** — is `granted` when the NEWEST such
source is stamped at or past the cursor's slot. `granted` removes exactly **one** exemption, so the mergeset
advances the DAA by one however many attempts it holds: *never each attempt +1*. The cursor, the reference
(earliest tied step), H3 (a beat is stamped at or after its slot), H5 (a step is stamped at or after the
slot it consumed) and the lead cap are unchanged, and the lead cap now also covers an attempt-lane header
(`PalwClockStepV1::lead_capped`), so a tick source is never stamped more than 132 s past a receiver's clock.

### 3.3 The miner's half (node policy, no fork)

`heartbeat_slot_hint_v1`: past the fence the heartbeat miner holds until `slot + 20 s`
(`PALW_REAL_TICK_GRACE_MS_V1`). In that time any block an attempt producer builds merges the open attempt
and steps the clock. After it, the ordinary hint applies: a beat is minted for a slot nothing carried, and a
tick source still unstepped is stepped. With no real producer a network ticks every 120 + 20 s at the
worst; with one it ticks at 120 s plus the producer's own latency and no heartbeat is mined.

### 3.4 Threat analysis (every ADR-0142 §9 property, restated for the new source)

| property | holds because |
|---|---|
| a producer cannot **speed up** the clock | a tick needs a source stamped ≥ the cursor's slot, and the step that consumes it is stamped ≥ that slot (H5) and ≤ receiver clock + 132 s (lead cap). Every reference is ≥ one interval after the last. Attempts are free at the header stage, a heartbeat costs 2^24 — the cost never bounded the RATE, the cursor does, so the rate bound is unchanged. A burst is `⌊132/120⌋ + 1 = 2` ticks, once. |
| a producer cannot **stall** it beyond what a miner can today | an attempt stamped late does not delay the slot: the reference is the earliest tied step, and any honest block that merges a granted source is a step. To stall, a producer would have to prevent every other node from building a block. |
| one tick per slot however many attempts | `granted` removes one exemption; `100 attempts → +1`. Pinned by a test over generated mergesets. |
| far-future attempt stamps | the lead cap now applies to attempt-lane headers; an uncapped one could not bank ticks anyway (one per cursor slot) but could fill the past-median window. |
| DAA-denominated windows keep their wall meaning | the DAA still moves at most once per 120 s (the receipt window of 600 DAA, the challenge and court windows are 600/… × 120 s as before). |
| reorg determinism | `attempt_ticks`, `granted` and the cursor are functions of the block's own parents and window (ADR-0142 §6a), as before; no new state. |
| pruned / IBD node | headers only; the new source is read from `pow_algo_id` and `timestamp`. |
| a free attempt-lane header as a tick source | accepted: it gives a producer a tick source that costs less than a beat, but no more than ONE tick a slot, and a beat remains available to anyone. The attempt lane's cost problem (C-1, `palw_attempt_header_pins`) is a separate, open item; this fence does not widen what a free header can do to the clock. |

## 4. Prerequisites and the release

`validate_palw_useful_work_v1` (called from `validate_palw_v2`): A needs ConsensusV2 and `palw_model_registry`
at or below it, and the bundle's mirror (`sync_palw_floor_reserve_v1`); B needs ConsensusV2 and
`palw_anchor_clock`, `palw_single_lottery`, `palw_clock_cursor`, `palw_clock_floor` and `palw_clock_lead_cap`
at or below it (`single_lottery` because an attempt is unpriced by `bits` only past it; before it an
attempt in a mergeset is a priced block and no tick source). The release list entries are
`PALW_T12_FLOOR_RESERVE_ENTRY` and `PALW_T12_REAL_CLOCK_TICK_ENTRY`; lane INT places them in the DAA-5,300
list. The fork id names heights: the two fences share the flag day's height.

## 5. What this does not do

* It does not raise the rate real producers mine at: the real share of blocks is bounded by the real
  classes' own budgets, shares and the producers' compute. A and B remove what displaced them.
* It does not change fork-choice weighting of existing claims, rewards of claims already written, or
  mainnet.
