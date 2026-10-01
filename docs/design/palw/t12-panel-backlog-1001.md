# testnet-12 panel backlog, 2026-10-01 — root cause, fixes (int-10.1), verification

Status: analysis done on the live fleet (read-only); fixes F1–F4 on `rcore/int-10-p1` (node-only, the
t12 identity does not move); consensus proposal for int-11 below. Written by lane P.

## 1. What was observed

* Network-wide claims not Final and not void: ~340 → ~820 between DAA ~2,600 and 3,090
  (tracker `ledger/pub/nethealth.tsv`, one row per 5 min).
* **The stall is `panel_bound`, not the pipeline.** Per-claim phases from b6's local RPC
  (`getPalwClaims`, all ten producer bonds, 3,554 claims, 2026-10-01 09:17 CEST, DAA 3,089):

  | phase | claims | normal (Little's law) | waiting |
  | --- | --- | --- | --- |
  | provisional | 199 | 5.3/DAA × 20 DAA bind window ≈ 106 | bind window, normal (age ≤ 38 DAA) |
  | panel_bound | 261 | ≈ 17 (bind → licence is 2–6 DAA, median per 100-DAA cohort 400–2,900) | median 61 DAA since bind, p90 79, max 171 |
  | receipt_licensed | 354 | licence → Final is a fixed 121 DAA window | only 17 are past 121 DAA |

  Issuance is **5.3 claims/DAA since DAA ~2,750** (0.6 at DAA 2,000): the per-claim `acceptedDaa`
  histogram, 289 per 50 DAA (the tracker's 0.7–1.7 is the newest-500-per-bond listing window). The two
  fixed windows (20 + 121 DAA) alone hold ~790 claims at that rate: **most of the 820 is normal pipeline
  occupancy**. The excess is ~245 claims waiting for a quorum they should have had in 2–6 DAA.
* Producers by issuance since DAA 3,000 (claims/DAA): 9f76…:0 1.56, e136…:0 1.65, ad22 0.61, 5e0d…:1 0.51,
  a198 0.37, 8ad9 (P) 0.25, e346 0.17, 5e0d…:6 (b6) 0.10, 5e0d…:0 (b0) 0.09 — five bonds that are not the
  operator fleet's make 80 % of it.
* **Licensing tracked issuance until DAA ~3,000 and then collapsed** (claims licensed per 50 DAA, from the
  claim table, DAA 2,850 → 3,050: 273, 245, 287, then 126, 60) while binds stayed ~290 per 50 DAA.

## 2. Evidence: the seat-receipt supply

Receipts filed per hour (journal `filed a "Valid" receipt`, CEST; every node):

| | 04:00 | 05:00 | 06:00 | 07:00 | 08:00 |
| --- | --- | --- | --- | --- | --- |
| five seats on 5.104 (b2 b3 b4 b5 b7) | 365 | 39 | 25 | 42 | 23 |
| b6 (.113) | 51 | 35 | **0** | **0** | **0** |
| b0 (ibm) | 84 | 68 | 70 | 77 | 63 |
| b1 (ibm) | 96 | 70 | 72 | 94 | 24 |

(5.104 ran 90–410 receipts/h from 12:00 to 04:00; b0/b1 are steady at 60–95.)

**Why the slow seats gate everything.** The five 5.104 seats are on 65 % of all seat draws (2,140–2,228
panels each against 1,806–1,820 for b0/b1/b6), and a licence needs a quorum of 3 of 5. Of the 255
`panel_bound` claims **every one** needs at least one receipt from a 5.104 seat or b6 (b0 + b1 are at most
2 of 5 on any panel): the slowest seats are the licence.

### 2.1 b6 — the memory ledger refuses everything on a host with 18.7 GB `MemAvailable`

Every deferral line on b6 reads `… host memory ledger cannot cover 0.50 GiB: 0.00 GiB available (declared
share 8.00 GiB, host headroom 0.00 GiB, … less 0.00 GiB already reserved)`: nothing is reserved, the
share is 8 GiB, and the *live* axis is zero.

* The live axis is `memory.max − memory.current − 1 GiB` (`palw_backends::cgroup_headroom_from_v1`).
  b6: `memory.max` 18,253,611,008, `memory.current` 17,975,808,000 (98.5 %).
* Of its 17.1 GiB of anonymous memory **9.26 GiB is `LazyFree`** (`/proc/<pid>/smaps_rollup`, summed from
  `smaps`: 9,256,356 kB; 7.9 GiB is `Private_Dirty`, the live set). The allocator frees with `MADV_FREE`:
  `utils/alloc` sets mimalloc's `purge_decommits = false` ("smallest RSS"), so freed pages stay mapped and
  charged until the kernel needs them — and it only needs them at `memory.max` (`memory.events`:
  `max 173`, `pgsteal_direct 587,096`). `memory.stat` agrees: `anon` 17.6 GB but `active_anon` 3.3 GB
  and `inactive_file` 13.2 GB (the lazily-freed pages sit on the file LRU).
* The host is not under pressure (`MemAvailable` counts those pages as available); the cgroup term did not.
* Effect: 0 receipts since 06:00 CEST (40–77/h before), 6,000–7,300 producer-hold lines/h from 05:00,
  the node's own claims stuck (b6 bond: 99 open, 81 licensed-waiting).
* b1: the producer is refused by 0.1 GiB (`host headroom 3.81 GiB, less 3.37 GiB already reserved by
  full-seat` → 0.40 GiB available for a 0.50 GiB attempt): 4.8 GiB of lazily-freed memory uncounted, and an
  8k replay's 3.37 GiB counted twice (the ledger's deliberate double count, `palw_memory_ledger` docs).
* Growth with uptime is the allocator's high-water mark, **not a leak and not a function of the backlog**:
  b0/b1 anon oscillates 7.5–12.6 GiB flat while open claims went 330 → 830; seats oscillate 1.8–5.2 GiB
  (seat3, `NRestarts=0`); b6 rose 7.5 → 16.4 GiB over 36 h and then plateaued at its cgroup limit
  (16.3–16.5 GiB since 06:00 CEST). Evidence stores (captures, foreign materials) are on disk and pruned
  by the retention janitor; no RAM structure keyed by open claim was found.

### 2.2 5.104 — five seats on an over-committed host

* 8 cores, 24 GiB, five kaspad seats (`memory.max` 9 GiB each = 45 GiB of caps), 20 GiB swap of which 13–17 GiB
  used. `sar` all night: CPU idle 0.3–5 %, user 60–80 %, **system 15–36 %**, `pswpin/s` 300–6,000,
  `pgscand` and major faults in the hundreds a second, load average 35–40 on 8 CPUs.
* Replays on 5.104 take 7–50 minutes (seat4 05:00–08:00 hourly mean 375–701 s; single replays on seat2 2,729–5,005 s);
  on b0/b1/b6 they take 3–8 s (mean), 280–380 s for the 8k class. Same code, same claims.
* Each seat has `IN_FLIGHT = 2` replay slots (hard-coded); both are held by slow replays (`replay of claim …
  waits: every replay slot is taken`, ~30 a hour = every minute), so a seat files 1–15 receipts an hour
  against the ~60–95 it files when it is not thrashing.
* An 8k (C7) replay reserves 3.37 GiB of the 3.5 GiB share: while it runs (5 minutes on a quiet host, 10–80 on
  5.104) every other duty on that seat waits on the ledger (`ledger_deferred` ≈ 25–35 an hour, continuous).

### 2.3 Zero margin and no backpressure

* Healthy supply ≈ 600 receipts/h (9 seats × 60–95); demand is 3–5 receipts per claim × 127 claims/h =
  380–640. There was no margin before the collapse, and nothing connects the rate claims are issued at to
  the rate seats can license them.
* **Correction to the first report: F-N (`palw_capacity_network_room`) is armed on testnet-12** — it is
  `PALW_T12_CAPACITY_NETWORK_ROOM_V1`, the 8th entry of `PALW_T12_CAPACITY_RHO10_FENCES_V1`, armed at
  `PALW_T12_POST_LAUNCH_FENCE_V3_DAA = Some(1_700)` (`params.rs` 19320–19584). The `None` I read was the
  struct default of the base presets (`params.rs` 14122, 21215), not the t12 fence list. §5 is the real
  question: why it did not bind.

## 3. Root cause, in order

1. **Demand 10×, supply flat, no feedback.** Issuance went 0.6 → 5.3 claims/DAA with the receipt supply
   at its ceiling; claims are admitted by capital (J-6), the bond's exposure and the network room's
   capital/carriage level — none of which sees verification throughput.
2. **Verification supply fell ~3× at DAA ~3,000** for two reasons that are both node-side:
   (a) b6's ledger read a cgroup full of freed memory as full (`MADV_FREE`); (b) the 5.104 seats, which
   gate 65 % of panels, replay 100× slower on a thrashing host and hold their two slots with it.
3. **Waste where it hurts.** Every seat of a panel replayed every claim, oldest receipt deadline first. The
   fast seats put 2 receipts in the pool within seconds; the slow seats — whose slots are the scarcest
   resource on the network — then replayed claims that were already one receipt short of a licence from
   somebody else's work or already licensed. By panel composition about 60 % of the slow seats' replays were
   not needed for a quorum (a panel of 1 fast + 1 b6 + 3 slow needs 1 slow receipt, not 3).

Not the cause: memory growth with the backlog (hypothesis tested, refuted — §2.1); the audit door (only 17
licensed claims are past their 121-DAA window); carriage (licence queue 429, 3 licences/carrier, 143 blocks
to drain at ~6 blocks/DAA).

## 4. Fixes on `rcore/int-10-p1` (node-only)

**F1 — the ledger counts what the kernel can give back as free** (`palw_backends`):
the cgroup term is `max − (current − credit)` at every level, `credit = min(LazyFree, current, 60 % of the
limit)`. `LazyFree` is sampled from `smaps_rollup` on its own thread every 15 s (0.63 s of system time a read on
b6: never on a caller). b6's reading becomes 8.7 GiB. The cap (60 %) and the 15 s sample age are what a
stale figure is spent against; the share, every reservation and the 1 GiB reserve still bind.
*Allocator*: kept as is — `MADV_FREE` is the cheaper mode for a node that reuses its memory (no fault + zero page
per reuse), and on a CPU-saturated host switching it would add system time; the ledger must be right either
way. `utils/alloc` now honours `MIMALLOC_PURGE_DECOMMITS` when the operator sets it (the launch scripts refuse
`KASPAD_*`/`PALW_*` environments, not `MIMALLOC_*`), so `MIMALLOC_PURGE_DECOMMITS=1` in a unit drop-in returns
memory at once (RSS = live set) if an operator wants it; the soak should compare the two.

**F2 — seats do the work their panel still needs from them** (`palw_seat_schedule`, `PalwSeatReplaysV1`):
* order: **the two oldest due claims short of their quorum first** (by rank, not by age — under a standing
  backlog every claim is old and an age threshold would promote them all and switch the scheduler off; this
  is the starvation guard: FIFO at the head, and a claim whose primaries are dead reaches the head when the
  claims before it are done); then tier 0 = quorum short and this seat is one of the first `short + 1`
  unanswered seats in a hash ranking every seat computes alike, closest to quorum first, then oldest
  deadline; tier 1 = backup; tier 2 = quorum already pooled (checked receipts only — nothing a peer can
  forge moves the order); duties not due last. Nothing is skipped: lower tiers run when a slot is free, and
  a very old claim promotes itself as a backstop (backup 120 DAA, satisfied 72 DAA after the bind). Exactly
  one seat in five defers a fresh claim.
* slots: `--palw-seat-replay-slots` (1–4, default 2); one replay at a time while the host's load per CPU is over
  3.0 (released under 2.0); a light replay past 4× its class's timing on this host (min 8 DAA, untimed 24 DAA)
  stops holding a slot (it keeps its thread and reservation; the held total stays ≤ 2× slots; C7 replays are
  never relaxed). A claim past its receipt deadline already leaves the duties and detaches its replay.

**F3 — producer backpressure** (`palw_producer_backpressure`): every 10 s the producer counts its own bond's
claims `PanelBound` for ≥ **T = 30 DAA** (5× the longest healthy wait); at **K = 8** it holds the attempt lane,
and resumes at ≤ **2** (hysteresis). Anchor duty is never held (`facts.binder_due`: an operator's attempt binds
other bonds' claims, and a fleet that stopped anchoring would void what it was trying not to add to); the
receipt lane is not held. *Limit, stated plainly*: it bounds the producers that run it. On this chain that is
every operator bond between binds and every non-operator bond that runs int-10.1; a bond on another binary
keeps issuing. The network-wide answer is consensus's (§5).

**F4 — the debt is observable**: `getPalwNodeStatus.verification` (wire version 5, proto field 33) carries
`mem_available_mib cgroup_naive_mib cgroup_credited_mib lazyfree_mib host_load_milli`,
`seat_duties seat_oldest_wait_daa seat_receipts_1h sched_needed sched_backup sched_satisfied slots running overdue
detached load_limited`, `own_provisional own_panel_bound own_panel_bound_aged own_oldest_wait_daa debt_hold`;
the periodic `[palw-host] memory at periodic` line now splits anon into live and reclaimable;
`t12check.py` prints and flags it and `lib.sh cgroup_memory` reports the credited headroom.

Tests: b6's reading replayed (`a_cgroup_full_of_lazily_freed_pages_is_not_out_of_headroom`), the accumulation
simulated (`the_headroom_does_not_fall_as_freed_memory_accumulates`: the kernel's figure falls to nothing, the
credited one stays at `max − live`), scheduler tiers/aging/rotation/permutation, checked-only receipts, slots/
load/overdue, hysteresis, wire v4↔v5.

## 5. F-N (network room) against the live chain, and the int-11 proposal

**Why F-N did not bind.** `L_net = min(L_seat, L_carry, L_anchor)`, admission iff `L_net − U ≥ 1` (plus the
registered-bond share). On the live chain (estimates from the tracker's per-seat room and the shadow RPC;
`getPalwCapacityShadow`, DAA 3,104):

* `L_seat` = floor J-6 room (`⌊Σ_seats ⌊room_s / eligibility⌋ / 5⌋`, Σ room-panels = 497+514+584+578+583+580+507+578+82
  = 4,503 → **~900**) + floor claims bound and unlicensed (262) + the 8k class share (tens) ≈ **1,200**;
* `L_carry` = 21 × 64 (F-B batch) × max(1, ā_op) = **≥ 1,344**; `L_anchor` = 100 × max(1, ā_op) × 20 = **≥ 2,000**;
* `U` (Provisional + PanelBound, network-wide) = 429–439 (`licenceQueue 429`): **free ≈ 770, 36 % of the level**.
  No `NetworkRoomExhausted` in b1's last 12 h of journal; the 8k class's F-R room (`66,668,494,675,968 of replay
  in flight against a budget of 60,480,000,000,000 over 3 spans`) refused 3 merged attempts in 24 h.

So F-N bounds the queue by **capital and carriage**, not by verification throughput: the shadow's
`seatCapacityTodayMilliPerDaa` = 12,025 (12 claims/DAA of "seat capacity" at ρ = 10, q = 250 ‰) against a
measured licensing rate that fell to 1.2–2.5/DAA. With `U` pinned at `L_net ≈ 1,200`, a claim waits `U / μ`
DAA for a licence (μ = licences/DAA): 1,200 / 5.3 = 226 DAA at healthy supply (inside the 600-DAA receipt
window), but 1,200 / 1.7 = **706 DAA at the collapsed supply — past the window**: the claims at the tail redraw
and a second timeout voids them and slashes their honest producers. F-N is safe only while `μ ≥ L_net /
(window − anchor delay − margin) ≈ 2.4 claims/DAA`; the measured collapsed supply was 1.2–2.5.

**int-11 proposal (a), dormant until its flag day — a verification term in the level**:
`L_net = min(L_seat, L_carry, L_anchor, L_ver)` with
`L_ver = max(L_min, ⌈μ_obs × W_safe⌉)`, `W_safe = (window_receipt − anchor_delay) / 2 = 290 DAA`,
`μ_obs` = licences landed per DAA over a rooted 128-DAA ring (the same shape as `operator_ring`), counted
only on DAA where `U ≥ L_min` (a quiet network measures its demand, not its capacity), and
`L_min = 128` so the room cannot strangle to zero (an empty ring reads `L_ver = ∞` until the first window
fills). Properties to test: work-conserving and split-neutral exactly as lane N (L_ver is a level), `U` settles
at `μ × 290` so a claim's wait stays at 290 DAA < window at any μ, recovery is automatic (μ rises → the level
rises), and no consensus rule calls the shadow (S-I1). Seat-side capacity declarations stay as the upper bound.
Lane A owns the arithmetic; the measured numbers for it are in this note.
**(b)** is F3 above, node-side, shipped now.

## 6. Verification on the fleet (after int-10.1 is rolled out)

Order: **b6 first** (the starved node; one `upgrade`, in place), then ibm, then 5.104 once it is rebalanced.
Everything is read from `getPalwNodeStatus.verification`, `t12check.py`, the journal and the tracker.

| check | where | pass |
| --- | --- | --- |
| ledger headroom no longer 0 | b6 `cgroup_credited_mib`, `deferred: a replay needs` lines/h | credited ≥ share + 1 GiB; deferral lines < 5/h (was 58/h throttled, 6–7 k producer holds/h) |
| b6 receipts/h | `seat_receipts_1h`, journal | 40–80 within 30 min (was 0) |
| memory flat | `[palw-host] memory at periodic`: *live* anon (anon − lazyfree) | live ≤ 9 GiB and flat ±1 GiB over 24 h across ≥ 3 reclaim cycles (`memory.events max` may rise: the kernel reclaiming lazy pages is the design); no `oom_kill` |
| scheduler works | seats: `sched_satisfied` > 0 while `sched_needed` shrinks; the `seat schedule:` line | slow seats' useful fraction ↑: receipts/h per 5.104 seat ≥ 2× the pre-update 8–42, with `load_limited` false after the rebalance |
| slots not held hostage | `overdue`, `slots`, `running` | `overdue` 0 in steady state; `load_limited=true` only while the host is over 3.0 load/CPU |
| backlog drains | tracker `nethealth.tsv`: `panel_bound`, median wait | `panel_bound` < 100 within 6 h of the 5.104 rebalance, < 40 within 12 h; median bind→licence ≤ 10 DAA; licensed per 50 DAA ≥ accepted per 50 DAA (≥ 265) |
| producers behave | producer `debt_hold`, `own_panel_bound_aged` | non-operator bonds on int-10.1 hold only while `aged ≥ 8`; operators keep binding (BindTimeout voids stay at `last_void_daa=749`) |
| nothing regressed | `scripts/t12-repin.sh --drift-only`, journal | identity unchanged; no new voids, no `PanelBound` claim past 300 DAA |

Durations: 30 min on b6 (receipts, headroom), 2 h on ibm, 6 h for the drain, 24 h for the memory soak.
Rollback trigger: any node with `live` anon rising monotonically 6 h, `oom_kill` > 0, or receipts/h below the
pre-update level after 1 h.

**Soak on a drill chain** (before the fleet, node-only change): the int-10 drill kit's 8-node chain on the Mac,
int-10.1 binaries, no fence: 2 h at the drill's issuance with `--palw-seat-replay-slots` at 2; pass = licences
per DAA equal to the int-10 baseline, no void, the schedule counters populated, F3 holding no healthy bond, and
the producer's `verification` line present. F1 is Linux-only (cgroup + `smaps_rollup`): its soak is the b6
canary above, and the allocator comparison (`MIMALLOC_PURGE_DECOMMITS=1` vs default on b6's twin) is a 6 h
run on a Linux host.

## 7. Recommendation: the 5.104 host

Measured: 8 cores, 24 GiB; each seat is ~3 GiB live (2.7–3.2 GiB `Private_Dirty`) + 1.7 GB of artifact shared
across the host + a 3.4 GiB peak while an 8k replay runs; a busy seat averages ~0.6–0.75 core. Five seats
need ~28 GiB at the peak and every core: the host swaps all night.

* **Move two seats off 5.104** — one to .113 (b6's host: load 1.6 on 8 cores, 83 % idle, no swap, 18.7 GB
  `MemAvailable` once F1 stops it hoarding) and one to ibm (load 2–3 on 8 cores, 79 % idle, 19 GB available) — leaving
  3 seats on 5.104, 3 on ibm, 2 on .113. The rule: **seats per host ≤ ⌊(RAM − 4 GiB) / (3.2 + 3.4 GiB)⌋ = 3** on a
  24 GiB host (live set + one heavy replay each), and ≤ 1 per 2 cores.
* Until they move: `--palw-seat-replay-slots=1` on the five (halves the replay threads on the host; at load/CPU > 3 the
  node does it by itself with int-10.1), `MemoryHigh=5G` per seat unit (reclaim before the global pressure, not
  `MemorySwapMax=0`: with five seats the 8k replays would then OOM). Do not lower `--palw-host-memory-share`
  below 3.5 GiB — an 8k full seat needs 3.37.
* After the move each remaining 5.104 seat has ~2.5 cores and ~7 GiB: replays should return to the 3–8 s range.

## 8. Commands used (all read-only)

* `ssh … 'python3 -' < fleetclaims.py` (the tracker's `rpc.py` client over each host's local JSON-wRPC port):
  `getPalwVesting` → producer bonds → `getPalwClaims {includeTerminal, limit 0}` per bond;
  `getPalwNodeStatus`, `getPalwCapacityShadow {steps: [], includeClaims: false}`.
* Per unit: `/sys/fs/cgroup/system.slice/<unit>/{memory.max,memory.current,memory.stat,memory.events}`,
  `/proc/<pid>/smaps` (`LazyFree`, `Private_Dirty`, `Anonymous`), `/proc/meminfo`, `sar -u -q -S -B -W -r`.
* Journal, per unit and hour: `filed a "Valid" receipt for claim` (receipts), `deferred: a replay needs`,
  `every replay slot is taken`, `audit replay waits`, `this attempt needs … ledger cannot cover` (producer holds),
  replay duration = `replaying the anchor's job off the loop` → `licensed by replay`, `memory at periodic`.
