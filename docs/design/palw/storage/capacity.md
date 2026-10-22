# Pruned-node storage capacity: inventory, budget manager, reclaim, low-space stop (G14 capacity lane)

Status: **implemented, node-local, default OFF** (`--storage-cap-bytes` unset ⇒ every code path behaves as before).
Branch `storage/capacity` off `b1eb33960`. No consensus rule, wire format, fingerprint or on-disk format changes;
no RFC/ADR is needed for this lane (G14 storage brief: the capacity lane is node-local). The numbers in §2–§3 are a
**design budget and code-derived bounds, not measurements** — §8 is the plan that turns them into measurements.

Policy (G14 brief, 2026-10-22): a pruned node keeps its TOTAL footprint ≤ 30,000,000,000 bytes while it completes
IBD, follow, restart and reorg. The low-space stop is the last-resort safety measure, not the mechanism.

## 1. Code map

| Piece | Where |
| --- | --- |
| Budget model: categories, reservations, scan, reclaim, pause | `utils/src/storage_budget.rs` (`kaspa_utils::storage_budget`) |
| RocksDB footprint bounds | `database/src/db/storage_bounds.rs` (`StorageDbBounds`, `ConnBuilder::with_storage_bounds`) |
| Daemon wiring, monitor service, CLI parsing | `kaspad/src/storage_capacity.rs`, `kaspad/src/daemon.rs`, `kaspad/src/args.rs` |
| Staging DB reservation, orphan consensus dir reclaim, WAL fsync hook | `consensus/src/consensus/factory.rs` |
| Post-prune EVM RPC index GC | `consensus/src/model/stores/evm.rs` (`gc_pruned_block_rpc_indexes_batch`), `consensus/src/pipeline/pruning_processor/processor.rs` |
| IBD spill reservation + managed temp dir | `protocol/flows/src/palw_snapshot_chunks.rs` (`SnapshotSpill`) |
| Intake gates (IBD start / chunk, relay, RPC submit) | `protocol/flows/src/ibd/flow.rs`, `protocol/flows/src/v7/blockrelay/flow.rs`, `rpc/service/src/service.rs` |
| Retention janitor under the budget | `kaspad/src/palw_retention.rs` (`pass`, `reclaim_for_budget`) |

## 2. Storage location inventory

Managed root = `<appdir>/<network>/` (default `~/.rusty-kaspa/<network>/`). Category = the budget category the scan
assigns (longest path prefix wins).

| Path | Writer | Category | Growth / bound | Reclaimed by |
| --- | --- | --- | --- | --- |
| `datadir/consensus/consensus-NNN/` (active) | consensus (RocksDB) | active-db | pruned state + pruning window; see §2.1 for what outlives pruning | pruning processor; G14 EVM index GC |
| `datadir/consensus/consensus-MMM/` (staging, during IBD-with-headers-proof) | `Factory::new_staging_consensus` | staging-db | grows to ≈ the active DB's pruned size; coexists with the active DB until commit | `commit_if` → `delete_inactive_consensus_entries` (old active, immediately); `cancel` → `delete_staging_entry`; startup |
| `datadir/consensus/consensus-KKK/` (inactive / orphaned) | left by a crash between `remove_dir_all` and entry deletion, or an archival→pruned switch | active-db (until removed) | unbounded if never removed | startup `delete_inactive_consensus_entries`; **G14 reclaimer `obsolete-consensus-dirs`** (keys below the active key with no metadata entry) |
| `*/NNNNNN.log` inside each DB dir | RocksDB WAL | wal-maintenance (active DBs); staging-db (staging DB) | default: until memtables flush (`max_total_wal_size` defaults to 4× the write buffers); bounded mode 512 MiB (consensus) / 64 MiB (small) | RocksDB after flush |
| `*/LOG`, `*/LOG.old.<µs>` | RocksDB info log | wal-maintenance | **default keeps up to 1000 per DB — one more per restart, forever**; bounded mode 3 / 2 files × 32 / 8 MiB | RocksDB `keep_log_file_num` (bounded mode) |
| `*/MANIFEST-*` | RocksDB | active-db | default rolls at 1 GiB; bounded 64 / 16 MiB | RocksDB |
| obsolete SST after compaction | RocksDB | active-db | kept until the purge period: default **6 h**; bounded 5 min | RocksDB |
| compaction outputs in flight | RocksDB | active-db | per job ≤ `max_compaction_bytes` (default 25 × 64 MiB = 1.6 GiB); bounded 512 MiB × ≤4 jobs | RocksDB |
| `datadir/meta/` | meta DB: multi-consensus metadata, address store, chain participation, `PalwEconomicsLedgerV1` (230) | active-db | small; the economics ledger has no GC by design (only with the class ledger configured) | — |
| `datadir/utxoindex/` (`--utxoindex`) | UTXO index | active-db | current state only | — |
| external `--rocksdb-wal-dir/<db>/` | RocksDB WAL | wal-maintenance (measured where it lives) | as WAL above | as WAL above |
| `palw-retention/<claim>.material`, `.answer`, `*.partial` | producer, panel | palw-retained | per claim; owed captures pinned while the chain can demand them | janitor (time rule + space rule; G14: space rule against the budget, never owed material) |
| `palw-retention/foreign/<claim>.material` | panel | palw-retained | 72 h + 4096-file cap | panel; janitor space rule (unowed) |
| `palw-retention/public-prefix-owned/`, `records/`, `references`, flat/float public files | panel | palw-retained | lease expiry at the pruning point — **only while the panel runs** | panel |
| `palw-retention/tir-public-prepared/` | TIR custodian | palw-retained | ≤ 4096 sealed, journal retirement | custodian |
| `palw-retention/<claim>.witness`, `.witness.chunks` | producer (sketch) | palw-retained | **no GC** (janitor scan matches only `.material`/`.answer`/`.partial`) | none — see §7 U-2 |
| `palw-retention/improve/<claim>.capture` | improve lane | palw-retained | **no GC** | none — see §7 U-2 |
| `palw-retention/tir-public-owner/<claim>/` | TIR owner | palw-retained | files unlinked on drop; empty dirs accumulate (inodes only) | none |
| `palw-panel/*` (fee outpoint, signed rounds, traps, filer books, custodian state, `exec-slices/`) | panel | palw-retained | constant or capped; exec slices removed when Done | panel |
| `logs/rusty-kaspa.log`, `rusty-kaspa_err.log` + `.N.gz` (N = 1..8) | log4rs | logs | 2 appenders × (100 MB live + 8 archives) ≤ 1.8 GB worst case (incompressible); a roll writes one gz next to the live file | log4rs window; **G14 `log-archives`** (archives beyond the window; under pressure oldest first) |
| `storage-tmp/misaka-palw-snapshot-*.spill` (G14; uncapped: `$TMPDIR`) | snapshot send/receive | ibd-spill | ≤ 384 MiB per session (`PALW_PRUNING_STATE_MAX_BYTES`), ≤ 2 sessions (`GLOBAL_SESSIONS`); unlinked at creation on unix, so only `statvfs` sees it | session drop; **G14 `abandoned-temp-files`** |
| `$TMPDIR/rusty-kaspa/*` | pruning-proof temp DBs (`create_temp_db!`) | outside the root (physical volume of `$TMPDIR`) | deleted on drop; a crash can orphan | **G14 `os-temp-leftovers`** (older than 1 h) |
| `validator-state.json`, `palw-resync-report.json` | daemon, verified resync | other | constant | overwritten |
| `datadir.pre-resync-<time>` (`--palw-verified-resync`) | verified resync | other (counted) | a full copy of the old datadir — **never deleted by the node** | operator |

### 2.1 What outlives pruning (found in this lane)

Per-block rows the pruning processor did NOT delete (store audit of `database/src/registry.rs` against
`pruning_processor/processor.rs`):

| Prefix | Store | Key | Before G14 | Now |
| --- | --- | --- | --- | --- |
| 217 | `EvmRawTransaction` | tx hash → full raw tx bytes (+ payload block) | never deleted (`delete_batch` had no caller) — a full second copy of every EVM tx after the payload (211) is pruned | deleted when the carrying block is pruned (if still pointing at it) |
| 205 | `EvmLogs` posting index | (kind, selector) → (number, l1 hash, tx, log) | never deleted (no delete API) — up to 5 rows per log, side branches included | the pruned block's postings deleted, reconstructed from its receipts |
| 204 | `EvmTxLookup` | tx hash → inclusions/acceptances | never deleted | deleted when no reference in it is still live (not rewritten otherwise) |
| 210 | `EvmBlockHashMap` | 32-byte rpc id → L1 hash | never deleted | released if it still maps to the pruned block |
| 213 | `EvmNumberIndex` | evm number → L1 hash | deleted only on reorg | released if it still points at the pruned block |
| 232 | `EvmBlockStateRoot` (`--evm-shadow-state-backend` only) | block → root | never deleted | **unchanged** (shadow backend, opt-in; §7 U-4) |
| 222 | `EvmCode` | code hash | never deleted (shared by design) | unchanged (needs reference counting; §7 U-4) |
| 223 ‖ `0xFF` ‖ `PH` ‖ `l/x/c/y` | PALW prefix-history leases / expiry index | seed / claim | deleted only by the panel tick (`acknowledge_*_gc_v1`) — a node without the panel grows | **unchanged** (consensus-written rows; §7 U-3) |
| 198 | `EpochAccumulator` | epoch | never pruned (comment at the prune site) | unchanged (inert today; §7 U-5) |
| 196 | `StakeBonds` | outpoint | reorg-revert only | unchanged (permanent registration semantics) |
| 226 | `PalwClassCarriages` | class id | schema bump only | unchanged (fee-gated registrations; permanent) |

The EVM rows are "RPC index only — never part of any commitment" by each store's own contract; their GC is on with
`--storage-gc-pruned-evm-indexes` and implied by `--storage-cap-bytes` (never on an archival node, never in
`--evm-history-mode=archive`). Effect: eth-rpc no longer answers lookups for blocks the node has pruned the bodies of —
which it already could not serve correctly (the payload and receipts were gone).

## 3. Concurrent peak per phase

Symbols: `A` live active consensus DB (measured baseline: 7.5 GB at DAA ≈ 13.7k on t12 node1, ≈ 0.6 MB/DAA overall);
`S` staging DB; `M` meta + utxoindex; `R` retained PALW material; `L` logs; `P` spill; `W` WAL tail; `C` compaction
outputs in flight; `O` obsolete-but-unpurged SSTs. Bounded = under `--storage-cap-bytes` (`StorageDbBounds`).

| Phase | What coexists | Default RocksDB | Bounded |
| --- | --- | --- | --- |
| Normal follow | `A + M + R + L + W + C + O` | `W` ≈ 4 × write buffers, `C` ≤ jobs × 1.6 GiB (jobs = core count), `O` up to 6 h of rewrites — the ~57 MB PALW tip row is rewritten every virtual commit, so `W`/`O` churn is real | `W ≤ 512 MiB`, `C ≤ 4 × 512 MiB`, `O ≤ 5 min` |
| Pruning (pruning point moves) | follow + delete tombstones until compacted; EVM index GC adds tombstones | tombstones hold space until a compaction rewrites their range | same, purged within 5 min of the compaction |
| IBD with headers proof (staging) | `A + S + W_A + W_S + C_A + C_S + P_recv (+ P_serve) + M + R + L` — the **peak**: just before commit `S ≈ A` | unreserved: two independent DBs, two compaction pools, spill in `$TMPDIR` | `S` reserved as an envelope (staging-db budget) BEFORE creation; spill reserved (≤ 2 × 384 MiB); old active removed at commit |
| IBD catch-up (no staging; `sync_new_utxo_set`, headers, bodies) | follow + imported pruning UTXO set + spill | as follow | as follow; intake stops at chunk boundaries when paused |
| Reorg | follow + reverted/applied diffs in WAL | no new directories; `W` | same |
| Manual compaction (e.g. legacy-206 bulk reclaim `bulk_delete_all_and_compact`) | follow + output of the whole compacted range | up to the range size | bounded per job by `max_compaction_bytes`, but a manual `compact_range` can still schedule several jobs — measure (§8) |
| Restart | follow + WAL replay flush (`W` becomes L0) + one more `LOG.old` per DB; startup deletes staging / inactive dirs | `LOG.old` accumulates (≤ 1000 per DB) | ≤ 3 info logs per DB |

Design budget for a 30 GB cap (G14 brief; sums to exactly 30,000,000,000; `DESIGN_BUDGET_30GB`):

| Category | Bytes | Reservation mode |
| --- | ---: | --- |
| active-db | 10,497,346,816 | additive |
| staging-db | 10,497,346,816 | envelope |
| wal-maintenance | 4,000,000,000 | additive |
| palw-retained (reclaim target) | 2,000,000,000 | additive |
| ibd-spill | 805,306,368 (384 MiB × 2) | envelope |
| logs | 2,000,000,000 | additive |
| other | 200,000,000 | additive |

Planned transient bound of the bounded active DBs (`StorageDbBounds::transient_bound`): consensus 2.85 GB + 3 small
DBs 0.70 GB = 3.56 GB ≤ the 4 GB WAL/maintenance budget (tested). Summing configuration values is not a guarantee —
§8 measures it.

## 4. The budget manager

* **One managed root, one cap.** `--storage-cap-bytes=<n>` (recommended `30000000000`); the root should be a dedicated
  quota-capable volume (XFS project quota) so the kernel enforces what the budget plans against. Per-category budgets
  are the design budget scaled to the cap; `--storage-category-budget=<category>=<bytes>` overrides one (repeatable).
* **Accounting = periodic scan + outstanding reservations.** Every 15 s the monitor (`StorageBudgetService`) walks the
  root (allocated blocks, symlinks not followed) and assigns each file by longest prefix; RocksDB WAL / info-log files
  of the active DBs count as wal-maintenance. Charge per category: envelope `max(scanned, reserved)`; additive
  `scanned + reserved`.
* **Reservation API.** `reserve(category, bytes) → StorageReservation` (released on drop; `ensure(bytes)` grows it)
  refuses when `cap − safety_margin − charge < bytes`, when the category's reservations would exceed its budget, or
  when the physical volume minus its margin and all not-yet-materialised reservations has less than `bytes`.
  `reserve_after_reclaim` runs the obsolete-data reclaimers first, and under-pressure reclaim if still short.
* **Users of reservations:** staging consensus DB creation (envelope = the staging-db budget, held until commit/cancel;
  refusal → `LocalNativeCapacityUnavailable`, a local condition that condemns no peer or chain); every snapshot/IBD
  spill session (384 MiB, held for the session); log rotation headroom (a standing 100 MB reservation, because log4rs
  rotates inside the logger); retained material goes through the janitor's plan (below). DB switch = the staging
  commit: the old active dir is removed in the same call that releases the staging reservation.

## 5. Reclaim before use

Registered reclaimers, in order (each contractually never deletes material the node is still liable for):

| Reclaimer | Obsolete mode | Under pressure |
| --- | --- | --- |
| `obsolete-consensus-dirs` | orphaned `consensus-NNN` dirs below the active key that no metadata entry names (inactive entries stay with the factory's own post-commit / startup deletion: one may still be open while a commit drops it) | same |
| `abandoned-temp-files` | managed temp dir entries older than 1 h | same |
| `os-temp-leftovers` | `$TMPDIR/misaka-palw-snapshot-*.spill` and `$TMPDIR/rusty-kaspa/*` older than 1 h | same |
| `log-archives` | archives beyond the rotation window, `.tmp`/`.partial` | remaining archives, oldest first; never the live log |
| `palw-retention-unowed` | the janitor's time rule + abandoned `.partial` | the janitor's space rule for `need` bytes: unowed attempt captures oldest first, then unowed foreign copies; **every capture the chain can still demand is pinned** (DA-owed, material-income hold); a shortfall is reported, not met |

Post-prune EVM index GC (§2.1) runs inside the pruning batch itself, so it reclaims as pruning happens rather than at
reservation time. Without a cap the janitor keeps its old volume rule (`max(8 GiB, 5 %)` free); under a cap it plans
against the budget's headroom (floor = resume margin + retained bytes above the palw-retained target).

## 6. Low-space stop

When the headroom (`min(cap − charge, physical free − pending − margin)`) drops under the safety margin (default 1 % of
the cap, ≥ 64 MiB; `--storage-safety-margin-bytes`): reclaim under pressure; if still short, enter PAUSED — run the pause
hooks (fsync the WAL of every consensus DB, the meta DB and the UTXO index: `flush_wal(true)`), then:

* IBD does not start (the flow waits for the resume, the peer's offer kept);
* a running IBD stops at the next chunk boundary with `LocalNativeResourceUnavailable` (local, never a verdict on the
  peer or its blocks); a staging consensus is cancelled (which frees space); blocks already handed to consensus finish
  and commit normally;
* relayed blocks are not requested; RPC `submitBlock` returns an error (the block is not judged);
* reservations are refused (`Paused`); growth of an in-flight reservation is still allowed within limits.

**Nothing is marked invalid**: the gates sit upstream of consensus; no validation path reads the budget. Resume happens
at headroom ≥ resume margin (2 × safety margin), waking every waiter. Retained liability material is never deleted to
leave the pause.

## 7. Unresolved / out of this lane

* **U-1** The physical bound is a plan, not an enforcement: a filesystem quota on the root is what makes ENOSPC
  impossible above the cap. RocksDB hitting ENOSPC still enters its read-only background-error state (it does not
  corrupt), which the node treats as fatal — the margin exists so that is not reached.
* **U-2** `palw-retention/*.witness(.chunks)` and `improve/*.capture` have no GC at all. Whether they are liability
  material (court/DA evidence) is not classified in code; this lane counts them and does NOT delete them. Owner: the
  retention/court lane must classify them and extend the janitor's plan.
* **U-3** The 223 sub-space lease rows are GC'd only by the panel tick, so a node that never runs the panel grows them.
  They are consensus-written rows; moving their GC is a consensus-adjacent change for the state lane, not a capacity
  hack.
* **U-4** `EvmBlockStateRoot` (232, shadow backend) and `EvmCode` (222, shared by hash) still outlive pruning; code
  needs reference counting.
* **U-5** `EpochAccumulator` (198) and `StakeBonds` (196) grow with epochs / bonds; permanent-registration semantics —
  state lane (authenticated rows fetched on demand).
* **U-6** Manual whole-store compactions (`bulk_delete_all_and_compact`) are not reserved; measure their transient (§8)
  and wrap them in a wal-maintenance reservation if it exceeds the per-job bound.
* **U-7** Budgets above are the brief's design values; the staging envelope equals the staging-db budget, so a node whose
  active DB already exceeds ≈ half the usable cap cannot run an IBD-with-headers-proof under the cap. The state lane's
  "not holding everything locally" is what removes that ceiling.

## 8. Measurement plan for the real 30 GB budget (not run here)

Goal: replace each design number in §3 with a measured peak, per phase, with headroom. Devnet only; no live node is
touched. Run the node with `--storage-cap-bytes=30000000000` on a dedicated 40 GB XFS volume with a project quota of
30 GB (so a planning error surfaces as a measured pause, not a full disk).

Instrumentation (all read-only, 1 s sampling into CSV):

1. The node's own `[storage]` summary line (per-category charge, headroom, paused) — raise its period to 1 s for the run.
2. `df -B1` of the volume (physical truth incl. unlinked spill files) and `du -sb` of each category path.
3. RocksDB properties per DB via a debug RPC or `ldb`: `rocksdb.total-sst-files-size`, `rocksdb.live-sst-files-size`
   (difference = obsolete SST `O`), `rocksdb.estimate-pending-compaction-bytes`, `rocksdb.cur-size-all-mem-tables`,
   `rocksdb.num-running-compactions`; WAL bytes = sum of `*.log`.
4. Count of `LOG.old.*` per DB; number of `consensus-NNN` dirs.

Scenarios (each run twice: default RocksDB options and bounded):

| # | Scenario | Measure | Pass condition |
| --- | --- | --- | --- |
| M1 | Normal follow 6 h at the t12 block rate with PALW load | steady-state `A`, `W`, `O`, `C`; growth/DAA | total ≤ cap − margin; `O` purge ≤ 5 min (bounded) |
| M2 | Pruning point moves ×3 (devnet with short pruning depth) | tombstone bytes, time to reclaim, EVM index rows before/after | no growth of 204/205/210/213/217 for pruned blocks (bounded) |
| M3 | Fresh node IBD-with-headers-proof from a peer whose `A` ≈ 9–10 GB (synthesised by mining) | peak `A + S + P + W + C` just before commit; spill bytes via `df` − `du` | peak ≤ cap − margin, no pause; staging reservation ≥ measured `S` |
| M4 | Same as M3 with the source's `A` grown until the staging reservation is refused | refusal logged as local capacity; peer not banned | node keeps following on its old chain |
| M5 | Induced reorg (partition 2 miner sets, rejoin at depth just under finality) | `W`, transient during the virtual reorg | no pause; total ≤ cap − margin |
| M6 | Manual compaction (`compact_range` over the consensus DB; legacy-206 reclaim) | peak `C` | ≤ wal-maintenance budget; else U-6 |
| M7 | `kill -9` under load ×5, restart | WAL replay flush size, `LOG.old` count, leftover temp DBs | info logs ≤ 3 per DB; leftovers reclaimed in ≤ 1 h |
| M8 | Low-space drill: shrink the quota to `charge + 2 × margin` while following | pause entered, WALs fsynced, intake stopped, no block status other than valid written during the pause, resume after `rm` of a ballast file | resume within one scan period; no invalid-status rows; DB opens clean after `kill -9` during the pause |
| M9 | Log storm (`--loglevel=trace` for 1 h) | logs bytes | ≤ 2 GB |

Outputs: a table per scenario of measured peak per category vs. budget, and a proposed adjustment of
`DESIGN_BUDGET_30GB` (parameter-ledger entry: old/new values, measurement file, approval), per `docs/AGENTS.md`.

## 9. Tests (evidence)

`cargo test --release -p kaspa-utils storage_budget` — scaled-down 50 MB cap (same category shape):

* `reservations_prevent_double_use_of_the_same_free_bytes`
* `ibd_like_staging_next_to_the_active_db_stays_within_the_cap`
* `low_space_pauses_intake_then_resumes_when_space_is_reclaimed` (hooks run once, waiters woken, hysteresis)
* `gc_frees_obsolete_data_before_the_reservation_is_taken`
* `retained_liability_material_is_never_deleted_to_fit`
* `the_physical_volume_limits_reservations_below_the_cap`, `log_reclaim_drops_archives_beyond_the_window_first_and_never_the_active_log`,
  `rocksdb_wal_and_info_logs_of_the_active_db_are_maintenance_and_growth_is_bounded_by_ensure`

`cargo test --release -p kaspa-database storage_bounds` — info logs bounded across restarts, bounded DB read back by an
unbounded open; planned transient bound fits the budget.

`cargo test --release -p kaspa-consensus --lib g14_` — the pruned block's EVM RPC index rows go, a kept block's stay; only unnamed consensus dirs below the active key are orphans.

`cargo test --release -p kaspad storage_capacity` — CLI budget parsing.
