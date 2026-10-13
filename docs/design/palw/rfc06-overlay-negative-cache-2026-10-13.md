# RFC06: bounded absent-overlay-row cache — 2026-10-13

The native four-population 100k test reached all 99,993 extra worker keys in
45.450 seconds under its predeclared opt3 crypto profile. Its whole native test
then exceeded the unchanged 900-second deadline (exit 124, 900.022 seconds),
without producing a carriage dump. It has not passed native100k, snapshot wire
capacity or real P2P acceptance. [Plan](evidence/rfc06-native-four-100k-opt3-2026-10-13-plan.json),
[result](evidence/rfc06-native-four-100k-opt3-2026-10-13-result.json) and
[raw log](evidence/rfc06-native-four-100k-opt3-2026-10-13-log) are retained.

A three-second macOS process sample taken during this run located 809 samples
under native `heartbeat` → block template → `compute_overlay_snapshot` →
`selected_chain_overlay_window` → `CachedDbAccess::read` → RocksDB SST block read
and checksum. The sample reports physical footprint 9.7 GiB / peak 10.8 GiB;
these are sampled OS footprint numbers, **not** the whole-run maximum RSS.
[Sample](evidence/rfc06-native-four-100k-opt3-2026-10-13-genesis-sample.txt)
and [sampling command](evidence/rfc06-native-four-100k-opt3-2026-10-13-genesis-sample-command.log)
are retained. This isolates a real native-path bottleneck; it is not a complete
per-stage CPU/IO attribution or a completed four-seal resource measurement.

`commit_utxo_state` omits rewarded-key rows for empty lists and quality rows for
zero pools. The generic DB cache stores only present values. Thus repeated
selected-chain walks read missing overlay keys again, even though an SST lookup
can read/checksum a much larger adjacent PALW state value. The pruning-overlay
singleton has the same repeated lookup while it is absent. Genesis fixture
registrations reside in native PALW state/delta/tip, not in the genesis header.

The fix opts only these three stores into a bounded absent-row cache. Each row
store follows Count policy, capped at 16,384 missing keys; Empty and tracked
policies add no cached missing rows. The singleton keeps at most one miss.
Only `StoreError::KeyNotFound` is remembered. DB and decoding failures are retried.
The shared lock covers read/DB lookup/miss publication and every store mutation;
there is no late miss publication after a direct insert or restore. Normal clones
share caches. `clone_with_new_cache` starts with an empty independent cache and
shares the mutation-generation/lock, including when its own cache is disabled.
Reopening a store starts empty. Raw DB writers bypassing the typed store are
outside the existing positive-cache and this new cache's writer discipline.

A queued batch is not a durable write. It marks its key in a shared bounded pending
set before enqueueing. That key never publishes another miss, so a new-cache
reader seeing absence before `db.write(batch)` cannot retain that miss after
commit. Failed or abandoned batches conservatively retain the pending mark.
There is no implicit completion inferred from an optimistic positive cache.
At 16,384 distinct pending keys, the next key disables negative caching for the
store's invalidation domain, clears the pending set, and falls back to ordinary
DB reads. It never evicts a dirty marker or grows an unbounded dirty-key list.
A new-cache clone inherits this disabled state; a fresh store/reopen resets it.

This minimum safe implementation therefore **does not establish sustained cache
performance**: its pending keys are not retired by explicit DB commit tickets.
Auditing all formal writer sites and adding tracked post-commit hooks is a separate
remaining step. Initially unwritten zero-contribution keys still benefit until
that fallback. Consensus rows/Borsh/roots, epoch windows, reorg traversal and
pruning-point merging are unchanged. The processor always rereads its current
pruning boundary; it has no assembled-window cache.

Direct matching-dependency compilation passed. All 16 leaf tests passed, no ignored
cases (0.10 s test body; 20,709,376 bytes process maximum RSS). They cover queued
batch → independently cached missing read → DB commit → present read for both
row stores and singleton; late read/direct-write race; delete/same-hash restore;
new-cache/disabled-cache sibling writes; reopen; failed writes; count bound;
pending saturation; and retried non-missing errors. The included modules are the
production source files, linked to the native test's matching database/core/hashes
libraries. [Results](evidence/rfc06-overlay-negative-cache-2026-10-13-tests.log)
and [source/library/binary hashes](evidence/rfc06-overlay-negative-cache-2026-10-13.json)
are retained. Full integrated compilation and renewed native100k resource and
wire-cap trials remain required; no model-size, fleet or security approval follows.

## Controlled adjacent-SST measurement

A separate predeclared 60-second leaf run passed in 17.25 seconds against the
actual production `CachedDbAccess` and revised rewarded-epochs store. It used the
Default RocksDB preset and consensus 256 MiB block cache (two 128 MiB shards),
with a 160 MiB deterministic pseudo-random adjacent value larger than one shard.
This is a controlled SST stress fixture, not native bonds or a model artifact.
Both implementations returned the same missing-key result on each read.

| Round, 12 reads of the same absent key | Legacy generic cache | Revised store |
| --- | ---: | ---: |
| First | 5,306,484 µs | 532,302 µs |
| Second | 4,912,598 µs | 1 µs |
| Third | 4,917,974 µs | 1 µs |

The revised store incurs the first DB read, then reuses its absence. Later rounds
reuse that same cache; microsecond figures have timer-resolution limits. This
confirms that the selected fix avoids repeated large-block reads/checksums in an
actual DB. It establishes neither native100k whole-node throughput nor sustained
performance once pending-key saturation disables this minimum cache. The whole
benchmark maximum RSS was 691,978,240 bytes, including DB/write/compaction setup;
it is not decoder/model/node RSS. Its owned DB totaled 167,821,131 bytes and was
removed after recording its file sizes. No other cache was touched.
[Plan](evidence/rfc06-overlay-negative-cache-bench-2026-10-13-plan.json),
[raw timing/RSS](evidence/rfc06-overlay-negative-cache-bench-2026-10-13-log),
[fixture source](evidence/rfc06-overlay-negative-cache-bench-2026-10-13-source.rs)
and [source hashes/results](evidence/rfc06-overlay-negative-cache-bench-2026-10-13.json)
are retained. A rebuilt, independently predeclared native100k run remains necessary.

The subsequent [tracked post-commit follow-up](rfc06-overlay-commit-tickets-2026-10-13.md)
connects all formal batch writers and retires successfully committed pending references.
Its separate source hashes and 23-test controls preserve the distinction from these
minimum-cache timing results; new integrated/native acceptance remains pending.
