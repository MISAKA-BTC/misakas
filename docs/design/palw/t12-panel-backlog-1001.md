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
* **The 05:00 collapse, seat5, by class** (replay start → `licensed by replay`, claims matched to their class
  from the chain table): 04:00 hour — 93 floor replays, mean 48–59 s; **05:00 — two 8k (`ebf44d0a`) replays of
  2,197 s and 3,149 s and nothing else finished** (receipts 94 → 2); 06:00–07:41 — floor replays at 480–960 s
  (a thrash episode); 08:26 — another 8k replay, 3,313 s. Every deferral line in the 05:00 window reads `…
  less 3.37 GiB already reserved by full-seat of class … (3.37 GiB)` with `0.00–0.09 GiB available`: **an 8k
  replay reserves 3.37 of the 3.5 GiB share, so while it runs (5 minutes on a quiet host, 37–55 on 5.104) every
  floor replay (0.5 GiB) and audit (0.5 GiB) on that seat waits on the ledger** (`slot_waits` 0 in that hour:
  the slots were free, the ledger was not). 8k claims arrive at ~2 an hour and every one engages five seats:
  at 45 minutes each the slow seats are in an 8k replay for more than the whole hour.

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
   gate 65 % of panels, replay 10–100× slower on a thrashing host, and each 8k replay there (37–55 minutes)
   reserves 3.37 of the seat's 3.5 GiB share, so for its whole run nothing else on that seat can start — at ~2 8k
   claims an hour, five seats each, that is the 05:00 collapse (§2.2).
3. **Waste where it hurts.** Every seat of a panel replayed every claim, oldest receipt deadline first. The
   fast seats put 2 receipts in the pool within seconds; the slow seats — whose slots are the scarcest
   resource on the network — then replayed claims that were already one receipt short of a licence from
   somebody else's work or already licensed. By panel composition about 60 % of the slow seats' replays were
   not needed for a quorum (a panel of 1 fast + 1 b6 + 3 slow needs 1 slow receipt, not 3).

### 3.1 Who pays when a claim times out — has any honest bond been slashed?

From `sweep_deadlines` / `void_and_slash_at` (consensus/core `palw_state_v2.rs`):

* **First `ReceiptTimeout`** (`bound + window_receipt`, 600 DAA): the claim is **redrawn** — back to `Provisional`, a new
  panel anchored on the sweep, the first panel's seats released. No charge to anyone.
* **Second timeout** (a redrawn claim again not concluded within its window, ≥ 2 × (20 + 600) = 1,240 DAA after acceptance):
  `void_and_slash(ReceiptTimeout)` slashes **the producer's bond** by the claim's reservation + escrow term (past the 2026-09-23
  audit fence). **Seats are never charged for silence**: `slash_silent_seats` has an empty body ("the chain cannot observe
  silence"); a seat that signed nothing locked nothing and left duty at the redraw. The source comment says it plainly: the
  whole cost of two panels failing to conclude falls on the producer, whoever caused the silence.
* `BindTimeout` and `NoCapablePanel` voids are never charged.

On the live chain (b6's RPC, DAA 3,109, 18 bonds): **every bond's `bondSlashed` is 0 except two with 1 MSK** (`5e0d…:1` and
`8ad9…` = P; P's 1 MSK was taken at DAA ~508, `transitions.tsv`; b1's journal since 09-29 02:00 shows no slash line).
The floor class ledger reads voided 171 and **redrawn 0 (ever)**, all `BindTimeout` (the 12 in the tracker's window voided
at DAA 629–749); the 8k class voided 4 (`BindTimeout`) and redrawn 6 cumulative (none open now; the open ones are 171 DAA into
a 600-DAA window, so none belongs to this backlog). So **no honest bond has been slashed, no seat can be, and no open claim has
reached its first timeout.** The oldest `panel_bound` claims have their first-timeout deadline at **DAA 3,518** (≈ 17 h from
DAA 3,109; one is the 8k claim `c98d33d5…`, waiting for its third receipt): that is a redraw, not a slash. A producer can be
slashed only after a second timeout, DAA 4,138 at the earliest (≈ 42 h out). F1/F2/F3 land inside that.

Not the cause: memory growth with the backlog (hypothesis tested, refuted — §2.1); the audit door (only 17
licensed claims are past their 121-DAA window); carriage (licence queue 429, 3 licences/carrier, 143 blocks
to drain at ~6 blocks/DAA).

## 4. Fixes on `rcore/int-10-p1` (node-only)

**F1 — the ledger counts what the kernel can give back as free** (`palw_backends`):
the cgroup term is `max − (current − credit)` at every level, `credit = min(LazyFree, current, 60 % of the
limit)`. `LazyFree` is sampled from `smaps_rollup` on its own thread every 15 s (0.63 s of system time a read on
b6: never on a caller). b6's reading becomes 9.09 GiB (8.09 past the ledger's 1 GiB reserve). The cap (60 %) and the 15 s sample age are what a
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
* **big replays wait for lighter work**: a replay needing ≥ 2 GiB (the 8k's 3.37) is not STARTED while a due,
  not-yet-satisfied lighter claim waits, on a seat whose last replay of that class took ≥ 8 DAA (~21 min), and
  until the claim is within 120 DAA of its deadline (a fifth of the 600-DAA window, five times the longest
  measured replay); a class this seat has not timed, a fast seat (5 min), an idle seat and an urgent claim start
  it as before; a running replay is never touched. Such claims also take no oldest-first guard place until
  urgent. The cost is stated: on a slow seat an 8k claim's receipt arrives late (at most 480 DAA after its
  bind) — the fast seats license it meanwhile — in exchange for ~95 % of the demand (the floor) not waiting an
  hour behind each one.
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
`t12check.py` prints and flags it and `lib.sh cgroup_memory` reports the credited headroom. The live kit (`deploy-int10`, with Phase H) is not the tree's `contrib/t12-deploy-kit`: `contrib/t12-deploy-kit/patches/p1-f4-live-kit.patch` applies to it cleanly (`cd deploy-int10 && patch -p3 < …`, dry-run checked against a copy).

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
* `L_carry` = 21 × 64 (F-B batch) × max(1, ā_op) = **≥ 1,344** (3,494 at the live ā_op ≈ 2.6, `palw_network_room_v1` test); `L_anchor` = 100 × max(1, ā_op) × 20 = **≥ 2,000** (5,200 at 2.6);
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

**int-11 proposal (a) — STATIC, dormant until its flag day (user's staged rule: any consensus feedback loop is stage 6).**
A fourth term in the level, a constant sized from the measured supply:

    L_net = min(L_seat, L_carry, L_anchor, L_ver)
    L_ver = ⌊μ_floor × W_safe⌋ = ⌊1.5 licences/DAA × 290 DAA⌋ = 435      W_safe = (window_receipt − anchor_delay) / 2 = (600 − 20) / 2

* **Derivation.** Measured licences/DAA (claim table, 50-DAA buckets): healthy 5.5–5.8 (DAA 2,850–3,000); collapsed
  1.86 over DAA 3,000–3,100 (126 and 60 per 50 DAA: 2.5 and 1.2). `μ_floor` = 1.5: under the sustained collapsed rate,
  over the worst window by the margin `W_safe` gives. `W_safe` is half the receipt window: the other half is the redraw's
  margin against variance and bursts.
* **At the healthy rate it never binds**: `U` settles at `λ × (bind + wait)` ≈ 5.3 × 24 = 127 (simulated, `λ` = 5.3,
  μ = 5.7: peak queue 106, identical to the live level).
* **At the collapsed rate it is what protects honest producers.** The live level (`L_seat` ≈ 1,200) lets the queue climb
  until a claim waits `1,200 / 1.86` = 645 DAA — past the 620-DAA window (20 bind + 600 receipt): the tail redraws and the
  second timeout voids it and slashes the producer (§3.1). Pinned at 435 the wait is `435 / μ` + the bind: 234 + 20 DAA at 1.86,
  363 at the worst window's 1.2, inside the window for every μ ≥ 0.75 (the simulation test checks 0.8 and 0.6 on either side).
* **What it does to honest producers**: a refused attempt (`NetworkRoomExhausted`) is skipped, never charged, and registers
  the bond's demand for the work-conserving share — admissions equal licensings, split by stake. At the collapsed 1.86/DAA
  against 5.3/DAA issuance about two thirds of the attempts are refused (their draws wasted, nothing slashed); at the healthy
  rate none. Today's `U` of 439 is above 435: the room would be closed until it drains.
* **Code**: `rcore/int-11-lver` (off da6ac8bee, NOT on the int-10.1 branch): `palw_network_verify_level_v1`,
  `palw_network_level_with_verify_v1` and the fluid-queue simulation tests in `palw_network_room_v1.rs`; nothing calls them.
  Lane A integrates: a dormant fence entry, `network_level_v1` calling the `_with_verify_` form with `window_receipt` and
  `capacity_network_anchor_delay()`.
* **The adaptive form is the stage-6 design**, kept here and not built: `L_ver = max(L_min = 128, ⌈μ_obs × W_safe⌉)`,
  `μ_obs` = licences landed per DAA over a rooted 128-DAA ring (the shape of `operator_ring`), counted only on DAA where
  `U ≥ L_min` (a quiet network measures its demand, not its capacity). It follows a recovery up and a collapse down; the price is
  a consensus loop that moves with what it measures.

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

## 7. The 5.104 host: move two seats (approved) — the exact procedure

**Why.** Measured: 8 cores, 24 GiB; each seat is ~3 GiB live (2.7–3.2 GiB `Private_Dirty`) + 1.7 GB of artifact
shared across the host + a 3.4 GiB peak while an 8k replay runs; a busy seat averages ~0.6–0.75 core. Five seats need ~28 GiB
at the peak and every core: the host swaps all night. The rule: **seats per host ≤ ⌊(RAM − 4 GiB) / (3.2 + 3.4 GiB)⌋ = 3** on
a 24 GiB host (live set + one heavy replay each), and ≤ 1 per 2 cores. Target: 3 seats on 5.104, 3 on ibm, 2 on .113.

**Which.** **b7 → .113** (b6's host: load 1.6 on 8 cores, 83 % idle, no swap) and **b3 → ibm** (load 2–3, 79 % idle, 19 GB
available). b7 is the seat the kit made as its own unit (mode `new`, nothing of the route-matrix session's under it); b3 is the
heaviest seat on 5.104 (RES 6.4 GB). b2, b4, b5 stay (b2's borsh port 26313 is what `misaka-dnsseeder-t12` there asks).

**Interim, until they move (goes into the int-10.1 node update, not tonight's flag-day rollout):** `--palw-seat-replay-slots=1`
on the five (half the replay threads; with int-10.1 the node also does it by itself at load/CPU > 3.0). **Not** `MemoryHigh=5G`
(which I proposed earlier): five seats × any cap that fits the host (24 GiB / 5 = 4.8 GiB) is below an 8k replay's 6.6 GiB peak
(3.2 live + 3.4), so it would throttle the replay it is meant to protect; per-seat cgroups cannot fix a host that is over-committed
in aggregate. Do not lower `--palw-host-memory-share` under 3.5 GiB (an 8k full seat needs 3.37).

**Kit support (this branch, `contrib/t12-deploy-kit/`; none of the live kit's files change).** `move-seat.sh` (Mac),
`move-host.sh` (host) and `move-nodes.sh` (the table: `MOVES`, each seat's target spec, the hosts' reserves). They source the live kit's
`lib.sh` (RID, Phase H) and `fleet.env` on the hosts. Copy the three files next to the live kit on the Mac (`cp move-*.sh
~/Downloads/MISAKA-wt-b/deploy-int10/`) and `./move-seat.sh plan` prints everything below; `DRY_RUN=1 MOVE_YES=1 ./move-seat.sh run 7` prints
every command without running one (tested against a scratch copy of deploy-int10).

**Preconditions (all read-only; `preflight` repeats them):**
1. int-10.1 (or the release the fleet runs) is rolled out and **staged on the target hosts** (`$REL/bin/kaspad` with the release sha) —
   a moved seat runs the fleet's release; the 8k artifact (1.8 GB) and the IR artifact (1.87 GB) are present on .113 and ibm (checked
   2026-10-01) — `distribute-from-mac.sh artifact | artifact-ph` otherwise.
2. The three public nodes (b6 .113:26311, b0 ibm:26311, b1 ibm:26321) take a connection; the 8k class shows **more** ready seats than it
   requires (`ready=8/7`): a seat is away for the whole move and the class keeps a margin of one (`retire` refuses otherwise;
   `MOVE_READY_OK=1` overrides).
3. Memory: .113 = running shares 8,192 + b7 3,584 + reserve 4,096 = 15,872 of 24,031 MiB; ibm = 10,496 + 8,192 + b3 3,584 + reserve **1,536** =
   23,808 of 24,031 (the kit's 2,048 would be 24,320: the non-kaspad RSS there is 0.3 GiB measured, so `move-nodes.sh` sets 1,536). Disk ≥ 12 GB
   free (.113 53 GB, ibm 97 GB). Ports (all 127.0.0.1): b7 on .113 26351 / 26353 / 26354; b3 on ibm 26331 / 26333 / 26334 — free on both
   (`ss -ltn`, 2026-10-01).
4. The seats hold no round-signature record (`ls /root/.t12r-b{3,7}/misaka-testnet-12/palw-panel/` shows `palw-fee-outpoint` only, 2 KB, checked) — so
   copying only `palw-panel/` loses nothing a signature depends on. If a record ever appears there, use `MOVE_MODE=copy`.
5. Keys: `/etc/misaka/t12/t12-bond-{3,7}.key` (64 B, mode 0600) on 5.104. The node reads only the bond key (no flag names the `t12-operator-N.key`).

**Data mode, by the link** (measured 2026-10-01, 150 MB probes): 5.104 → Mac 13 MB/s, Mac → ibm 4.7 MB/s, Mac → .113 **0.64 MB/s** (60 MB in 94 s).
A seat's appdir is 4.5–4.9 GB (datadir 1.9, retention 2.5–2.9, panel state 2 KB, logs 80 MB): to ibm ≈ 6 min + 17 min + a 5 min checksum pass;
to .113 ≈ 2 h. So **b3 → ibm is a copy** (the seat resumes where it stopped, retention kept) and **b7 → .113 is fresh** (only `palw-panel/` is
copied; the node syncs the chain from its peers — the path every public joiner takes — and its old retention copies are not carried: a seat re-verifies a
foreign claim by replaying it, and DA answers for claims it covered come from the other covering signers). `MOVE_MODE=copy|fresh` overrides.

**Order: one seat at a time, b7 first.** Per seat, `./move-seat.sh run <id>` (it asks before every state-changing step; `MOVE_YES=1` does not):

| # | step | what runs | gate |
| --- | --- | --- | --- |
| 1 | push | `move-host.sh` + `move-nodes.sh` → both hosts' `/root/t12-rel/kit` | the live `lib.sh`, `fleet.env`, `t12check.py` are there |
| 2 | preflight | `move-host.sh preflight retire <id>` (source), `preflight add <id>` (target; the key and the appdir are expected missing: warnings) | release staged, artifacts, ports, memory, disk, public nodes up, 8k margin |
| 3 | **retire** (source) | `CONFIRM_MOVE_SEAT=yes move-host.sh retire <id>`: SIGINT stop (≤ 180 s), marker `/root/t12-rel/state/moved-b<id>`, drop-in `zzz-moved-b<id>.conf` (`ConditionPathExists=!marker`), `systemctl disable`; prints `MOVE_READY_WANT=<n>` | no process on the host holds the bond; unit inactive. The unit **cannot be started** (by hand, by a reboot, by `upgrade`) while the marker exists |
| 4 | verify | unit inactive, `pgrep` for the bond empty | — |
| 5 | data | copy: rsync source → Mac → target (excluding `logs/`), then `rsync -rnc` must list nothing; fresh: `palw-panel/` only | checksum pass identical |
| 6 | key | a pipe `ssh src cat key \| ssh dst 'umask 077; cat > key.incoming'` (never on the Mac's disk); sha256 on both ends compared (the digest only); `chmod 600; mv` | digests equal, else the incoming copy is removed and nothing starts |
| 7 | **add** (target) | `CONFIRM_SOURCE_STOPPED=yes move-host.sh add <id>`: `write_launch` + `write_unit` for this node only (`--check`), install `misaka-t12-seat<id>.service`, enable, start | fingerprint = EXPECT_FP, genesis + the kit's chain facts, duties ON (`panelRunning`), **synced + ≥ 1 peer** (≤ 900 s copy / 3,600 s fresh), **the 8k class's ready seats back to `MOVE_READY_WANT`** (≤ 3,600 s: the seat's readiness proof is carried again) |
| 8 | check | `move-host.sh check <id>` on the target: `t12check.py` (`verification`: `seat_receipts_1h` rising, `cgroup_credited_mib` ≥ share + 1 GiB, `load_limited=false`) | receipts flowing |

Seat offline per move: b7 ≈ 30–60 min (IBD), b3 ≈ 35 min. **Do not start the second seat until the first has `ready` back at its pre-move
count and has filed receipts for an hour.**

**After a successful move (by hand, in the kit):** move the seat's spec out of `install-5104.sh` `NODES` into `install-113.sh` / `install-ibm.sh`
(the printed spec), drop the moved seat's `127.0.0.1:…` peer entry from the remaining 5.104 specs' peer lists (the next `upgrade` of those seats then
needs `UPGRADE_ARGS_CHANGE_OK=1` for that one-line ARGS diff), re-push the kit. Leave `/root/.t12r-b<id>` on 5.104 for a day, then `mv` it aside (never `rm`).

**Rollback (any step ≥ 3):** `./move-seat.sh rollback <id>` = `move-host.sh unadd <id>` on the target (stop, disable, unit removed, appdir moved aside — never deleted)
then `CONFIRM_TARGET_STOPPED=yes move-host.sh unretire <id>` on the source (marker and drop-in removed, unit re-enabled if it was, started on its **untouched**
appdir, fingerprint-gated). The rule the whole thing is built on: **one bond is never in two processes** — `retire` refuses to finish while any process holds the
bond, `add` needs the operator's word that the source is stopped, `unretire` needs the target stopped. If the target started signing and then rolls back, the old
appdir is only behind by the time it was away: the seat re-syncs like any restarted node.

**What to watch after both moves (the verification plan, §6, plus):** 5.104 `load` per CPU ≤ 1.5 and no swap-in (`vmstat 1` si = 0); replays of the three remaining seats back in the
3–8 s range; the five moved/remaining seats' `seat_receipts_1h` ≥ 40; `panel_bound` draining.

## 8. Commands used (all read-only)

* `ssh … 'python3 -' < fleetclaims.py` (the tracker's `rpc.py` client over each host's local JSON-wRPC port):
  `getPalwVesting` → producer bonds → `getPalwClaims {includeTerminal, limit 0}` per bond;
  `getPalwNodeStatus`, `getPalwCapacityShadow {steps: [], includeClaims: false}`.
* Per unit: `/sys/fs/cgroup/system.slice/<unit>/{memory.max,memory.current,memory.stat,memory.events}`,
  `/proc/<pid>/smaps` (`LazyFree`, `Private_Dirty`, `Anonymous`), `/proc/meminfo`, `sar -u -q -S -B -W -r`.
* Journal, per unit and hour: `filed a "Valid" receipt for claim` (receipts), `deferred: a replay needs`,
  `every replay slot is taken`, `audit replay waits`, `this attempt needs … ledger cannot cover` (producer holds),
  replay duration = `replaying the anchor's job off the loop` → `licensed by replay`, `memory at periodic`.
