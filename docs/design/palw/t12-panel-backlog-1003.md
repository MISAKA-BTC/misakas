# testnet-12 panel starvation, 2026-10-03 — measurements, causes, the node-only fixes (int-10.4)

Lane P2, branch `rcore/int-10-p4` off `rcore/int-10-p2` 2483c570c (the fleet's int-10.3). Read-only on the fleet (journalctl,
`/proc`, `perf`, the nodes' own JSON-wRPC). Nothing was restarted, edited or sent. DAA at the time of the measurements ≈ 3,960.

## 1. What was measured

**Who the stuck claims are.** 114 `panel_bound` claims (b0's `getPalwClaims`, 8 producers, 500 rows each). Their ages
(DAA since the bind) run 0-211; the 22 older than 100 DAA **all** carry seats 2, 4 and 5 — the three seats still on 5.104
(a panel is 5 of the 8 genesis bonds, quorum 3, so a panel with all of 2/4/5 needs at least one of them). Aged 30-100: seats 2/4/5 are on
35/36/41 of 42. Receipts per hour per seat (`seat_receipts_1h`): b0 55, b1 30-38, b3 57, b6 50, b7 62-64, **b2 10, b4 9-12, b5 7-16**.
Those three seats owe 213-252 duties each; b0 owes 82, b6 63.

**The seats are not idle because they have nothing to do; the snapshot of `running` is the wrong gauge.** A floor replay
takes 2-8 s (b0, 23:07: four replays in 14 s, two slots), so a `running=0` reading is the usual one on every seat; the number that
matters is how many replays a seat STARTS and HARVESTS per iteration of its panel loop. A seat starts at most its slots' worth per
iteration (`PalwSeatReplaysV1::has_room`) and files a finished one only when an iteration polls its duty, so

    receipts per hour  ≈  slots × 3600 / iteration seconds.

**The loop period, measured from the throttled `seat schedule` line (a log line at most once a minute, so its spacing is
max(60 s, iteration)), 3 h of journal:**

| unit | iterations | mean spacing | max | spacings > 75 s |
| --- | --- | --- | --- | --- |
| b0 (ibm) | 167 | 64 s | 140 s | 9 |
| b6 (.113) | 167 | 64 s | 113 s | 12 |
| b7 (.113) | 167 | 65 s | 115 s | 13 |
| b3 (ibm) | 164 | 66 s | 122 s | 19 |
| b1 (ibm) | 82 | 131 s | 360 s | 65 |
| **b2 (5.104)** | 27 | **386 s** | 684 s | 27 |
| **b4 (5.104)** | 28 | **368 s** | 694 s | 28 |
| **b5 (5.104)** | 25 | **418 s** | 728 s | 25 |

A 5.104 seat's loop iterates every ~6 minutes; with `--palw-seat-replay-slots=1` that is one receipt per iteration = the observed 10 an hour. Its
replay "durations" in the journal (`replaying the anchor's job` → `licensed by replay`) are 250-540 s for every claim (mean 380 s, 88 replays) against 2-5 s for
fresh claims on b0/b6/b7 — the same claims: `acfc43711c41` took 205 s on an ibm seat, 8 s on b7 and 415 s on b4. A duration is the time to the
next poll, not the work (the replay thread itself is a spawn_blocking task; the `perf` profile of a running one is BLAKE2b over the replay's trace, 100 % of one core).

The iteration period grows with the duties a seat owes, linearly at ~0.4 s a duty on an ibm seat (b3: 100 s at 240 duties, 122 s at 280; b0 at 82 duties hides
under the 60 s throttle) and ~1.5 s a duty on 5.104:

| host | CPU share the VP + loop get | SHA-256 4 KB, 3 s, single thread (python) |
| --- | --- | --- |
| .113 (load 0.15) | idle | 862k |
| ibm (load 0.86) | | 715k |
| 5.104 (load 7.5 on 8 vCPU, swap 8 GB) | saturated | 270-286k |

**What saturates 5.104: the virtual processor of each seat** (`virtual-process` thread: 186 % on b4, 185 % on b5, 121 % on b1, 12 % on b0, 0 % on the idle ones;
`top`: 43 % system time). `perf trace` on b4: **79 `pread64`s of 59,984,525 bytes in 8 s (10 a second)** — the same 57 MB range of one SST (`055687.sst`) — and
the thread is 70 % kernel (`rep_movs_alternative`, `filemap_read`). That block is the data block that carries the PALW chain state's **tip row** (a ~57 MB
borsh carriage, rewritten at every virtual commit: a new 60 MB L0 file every flush, 44.7 GB flushed since 03:44). The rows beside it
in key order are the per-block overlay rows (`rewarded_epochs`, `block_quality_pool`, `reserve_balance`, the epoch accumulator — the SST's block starts with
prefix 0xc6 = `EpochAccumulator` rows 1760, 1761, …). Every virtual commit walks the selected chain back `overlay_window_walk_bound` (≈ 600 DAA at the 120 s cadence:
`selected_chain_overlay_window`) and reads two of those rows for each ancestor — through caches of `block_data_cache_size` = 200 × `bps()` (= 1 here) = **200 rows**, so every
walk misses, and a miss that lands in the big block costs a 57 MB read. And RocksDB cannot cache that block at all: its built-in cache is 32 MB in 64 shards of 512 KB, and
**an entry bigger than one shard is never kept** (measured in the new database test: a 40 MB row leaves 289 bytes resident under the default, 41.9 MB under the fix).

Evidence this is the VP and not the panel: `getPalwNodeStatus` answers in 10-90 ms on every node (the consensus session is not blocked); 32-40 `SendPingsFlow … timeout expired after 120s`
a node on 5.104 (the localhost peers too, i.e. the node's async runtime itself is starved), 31 on b6, 11/19 on b0/b1; b2 sat 24 DAA behind the tip.

## 2. Causes, in order of confidence

1. **(certain) The slowest seats gate every licence.** Panels with seats 2+4+5 cannot reach quorum without one of them, and those seats file 7-16 receipts an hour.
   Licensing keeps pace with binding elsewhere (b0, 3 h, 30-minute bins: licences carried 29-99, binds 15-72; queue 109-135 stable) — the backlog is the claims behind the slow seats.
2. **(certain) A seat's throughput is slots × 3600 / loop period**, and the 5.104 loop period is ~6 minutes with 1 slot.
3. **(high) The 5.104 hosts are CPU-saturated, mostly by the three virtual processors** re-reading 57 MB through a block cache that cannot hold it, and by the per-commit overlay
   walk missing a 200-row cache. A 2.6-3× slower per-thread speed (generic EPYC, 2.8 GHz, 3 nodes on 8 vCPUs) makes it worse.
4. **(open) Which phase of the loop takes the six minutes is not yet attributed.** The throttled logs cannot say. int-10.4 adds the stopwatch (§3, F3); the first canary names the phase.

## 3. What int-10.4 changes (node-only; no consensus rule, no on-disk format; `scripts/t12-repin.sh --drift-only` clean)

* **F1 — a block cache that holds the block** (`database/src/db/rocksdb_preset.rs`, `consensus/src/consensus/factory.rs`, `kaspad/src/daemon.rs`): the default preset's consensus databases
  open with a 256 MiB LRU cache in **two shards** (`BLOCK_CACHE_SHARD_BITS = 1`, 128 MiB each); `--rocksdb-cache-size` (MB) now also applies to the default preset (it used to be HDD only).
  The address-manager/meta/utxo-index databases keep RocksDB's default. Filled lazily: resident = what the node reads. Test:
  `a_consensus_sized_block_cache_keeps_a_block_bigger_than_rocksdbs_default_and_the_default_does_not`.
* **F2 — the overlay row caches cover the overlay window** (`consensus/src/consensus/storage.rs`): `rewarded_epochs`, `block_quality_pool`, `reserve_balance` hold at least
  `OVERLAY_PER_BLOCK_CACHE_MIN_ITEMS` = 16,384 rows (under 2 MB each), so a steady-state walk reads no database.
* **F3 — the loop's stopwatch** (`kaspad/src/palw_panel.rs`, `PalwTickGuardV1`): `getPalwNodeStatus.verification` gains `tick_last_s=… tick_max20_s=… tick_slowest=<phase>:<s>` (check-fleet prints it
  on the F4 line) and an iteration over 20 s logs its phases of a second or more, once a minute. Phases: setup, challenger, court+da, duty-read, receipt-pool, schedule, duty-sweep, resend, accusations, tail.

Expected gain: the VP stops preading 57 MB (b4/b5: 185 % → single digits; the ibm/.113 nodes are already at 0-120 %), the 5.104 hosts get back ~3-4 cores, and the
loop and replay threads run at the speed the other hosts have. **This is a prediction: the canary reading is `tick_last_s` and the VP's CPU before/after on b2.**

## 4. What int-10.4 does NOT do, and what needs a consensus change

* **The PALW tip row is still 57 MB and rewritten every virtual commit** (60 MB L0 file per flush, compaction every 1-2 min, 2.2 GB/h flushed). A lagging snapshot (every K commits, recovery by the delta walk that already exists at
  `processor.rs:1881`) or blob separation would end it; both touch crash recovery and the readers' "the row is the sink" assumption — a storage redesign for its own lane, not a canary change. The state root is a full
  recompute over every collection (`state_root`), also O(state) per commit; an incremental root is a consensus-neutral change only if the bytes stay identical (a new golden-vector gate).
* **Duplicate licence sets are not what delays licences.** b0 over 3 h: 1,316 `a PALW lifecycle object was dropped, and the block stands` (990 "seat … is already credited on this claim", 309 optimistic
  receipts that do not verify, 13 no quorum, 3 refused) against 457 licences carried (343 `ReceiptLicensed`, 114 `OptimisticLicensed`), 319 `PanelBound`, 255 audit batches. Blocks carry 1-4 PALW objects (290 of 346 PALW-carrying blocks;
  the largest had 18) and there is no per-block cap in play; the drops are parallel blocks crediting a seat first and a collector's single in-flight carrier (`MAX_INFLIGHT_CARRIERS = 1`) spent on a loser. They cost block mass and a
  carrier slot, not licence throughput. A deterministic collector per claim would save the waste; it is node policy but the gain is small, so it is not in this lane.
* **`PALW V2 state tip stood at X while the template selected parent is Y`** (37 in 20 minutes on b0): a template over a parent that is not the stored tip re-derives the state over the chain path
  (`processor.rs:6369`) — parallel blocks, not a fault; each pays a `load_tip` (a full 57 MB decode). Gone with the lagging-snapshot redesign, not with a cache.
* **Seat draws are by bond, not by host**: nothing in the draw can prefer fast hosts. That is a consensus change (capacity-weighted draws) and is the lever behind "a panel needs one of the slow seats" — F-N / L_ver in int-11 bounds issuance instead.

## 5. Deploy notes and operations

* int-10.4 is a binary swap on every node: no new flag is needed. `--rocksdb-cache-size=<MB>` overrides the 256 MiB. Memory: up to +224 MiB resident per node over today's 32 MB (lazily); on 5.104 three nodes = +0.7 GB of ~14.7 GB available.
* Roll b2 first (5.104, the worst), watch `tick_last_s` and the VP's CPU (`top -H -p <pid>`, `virtual-process`) for 30 minutes, then the rest. A restart loses nothing a tick does not rebuild.
* **No code change needed for this one, and it is the larger lever: put the seats where the CPU is.** .113 runs two nodes at a load of 0.15 and files 50-64 receipts an hour per seat; 5.104 runs
  three at 7.5 and files 7-16. Moving b4 and b5 to .113 (b2 stays) with the existing `move-seat.sh` makes every panel's quorum reachable at the 50-60 an hour rate — the three seats that gate licensing become fast, and the
  .113 host is still at one core a node. `--palw-seat-replay-slots` on the seats left on 5.104: leave 1 until the VP is fixed (a floor replay is a core's worth of BLAKE2b; more slots on a saturated host starve the VP further).

## 6. The duty sweep: `artifact_digest()` per resolve (found on the b2 canary, int-10.4 49c2fbb1c72b)

b2 with F1 deployed: virtual-processor 0 % CPU, but `tick_last_s=592`, `tick_slowest=duty-sweep:540.8` (the warn: `setup 6.8 s, duty-sweep 540.8 s, tail 44.4 s`; an earlier tick 819 s = `duty-sweep 706 s`).
The node is IDLE while the sweep runs (workers parked in `futex`), except ONE tokio worker at 100 % (the loop itself: no `task.rs`/spawn_blocking frame under it).
A `perf --call-graph dwarf` of that thread (frames mapped to source through the binary's panic-location statics): BLAKE2b (`blake2b_simd`) under `palw_artifact.rs` / base0
`artifact.rs` under `palw_panel.rs` — 385 of 385 samples. That is `Base0ArtifactV1::artifact_digest()`: it streams the embedding and unembedding slabs (~1 GB of the 1.5B class) through BLAKE2b, and the sweep
calls it for EVERY duty, several times: `resolve_backend` → `dense_artifact_by_digest` (a digest per dense holding, to compare with the class root), the backend constructors (`shape_id`),
`a16_inventory_digest_key_v1` (every inventory read). ~2 s a call on 5.104 x ~240 duties = the nine minutes; ~0.4 s on an ibm seat — the 0.4 s/duty slope measured in §1, and why the loop period
rose with the duty count. On .113 b7 the hot thread is a genuine replay (`palw_step_leg` under `task.rs`: an 8k claim), because its ticks are short (64 s) — the same digest cost is there, but 4x cheaper.

**F4 (this commit)**: `artifact_digest_shared` / `artifact_digest_memoised` (misaka-palw-base0): the digest is computed once per held allocation. The registry keeps a `Weak` (the allocation's address stays
reserved, so no other artifact can be found at it; an evicted holding still frees its 1.7 GB; `Arc::get_mut` refuses while it exists), at most 16. Used at the three per-resolve sites. Test:
`a_held_artifacts_digest_is_memoised_on_its_allocation_and_is_always_the_digest`; `misaka-palw-sdk` lib 56/0, `misaka-palw-base0` inventory 11/0. Node-only; the digest value is unchanged.
Expected: the sweep drops from ~2 s a duty to the cost of the duty's own work (state reads, receipt-pool checks): the tick should fall to the 60 s throttle floor seen on b0/b6/b7, i.e. ~6x the receipts on a 5.104 seat.

## 7. The tail: the licence collector asks every pooled claim for three doors every tick (int-10.5 b2: tail 206.7 s of 282 s)

After §6 the b2 tick is `duty-sweep 75 s, tail 207 s`; the tail ends with the supplementary collector's log line. What runs in the tail: the audit duty, readiness proofs, the fee-chain resolve, then the **licence loop**
(`for claim in claims`: coverage set, V1 quorum, optimistic set, per pooled claim) and the **supplementary loop**. Only the supplementary loop had a "this claim came to nothing" memo
(`palw_supplementary_idle_v1`, 10 DAA). The licence loop asked every pooled claim (88-250, most of them one or two `Valid`s short of any door) at every tick: each ask is an ML-DSA-87 check per candidate and
up to a few folds, and **a fold clones the whole ~57 MB PALW state** (`palw_v2_apply_one_object_v1` → `TransitionBuilder::new(parent.clone())`, `checkpoint()` clones again). The profile of the loop thread on b2 is
BLAKE2b/state work under `palw_state_v2.rs` with no replay frame (385+ samples of ~100 % of a worker), and `perf trace` shows every other thread parked.

**F5 (int-10.5 + this commit)**: `licence_idle` (the licence loop's twin of `supplementary_idle`): a claim whose candidate fingerprint is unchanged and whose last ask came to no object is left alone for
`LICENCE_IDLE_REPLAN_DAA` = 2 DAA (~2 min; a receipt arriving or leaving asks at once); and one tick spends at most `COLLECTOR_ASSEMBLE_BUDGET` = 20 s asking assemblers in each of the two loops. An asked-and-idle claim
drops out of the order, so the next tick starts further down the same oldest-first list: a long list is walked over a few ticks instead of freezing the loop. Phases split: `tail-pre-licence`, `licences`, `supplementary`, `tail`.
Tests: `the_licence_collector_rests_a_claim_whose_candidates_came_to_nothing_and_a_tick_has_a_budget`; kaspad lib `-- palw` 446/0. Node-only; a licence whose candidates did not change and whose state did can wait at most 2 DAA longer.
Expected: tail ≤ ~45 s (20 s + 20 s + the carrier work) on a 5.104 seat, the tick toward 100-120 s with the sweep's 75 s (the sweep is the next phase: its per-duty cost is now state reads and the receipt pool).
Not done, and the structural fix: a fold should not clone the state (copy-on-write rows in `TransitionBuilder`) — consensus-core, node-only, its own lane.
