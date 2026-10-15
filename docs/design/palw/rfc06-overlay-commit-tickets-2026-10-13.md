# RFC06: acknowledge overlay rows after durable batch commit — 2026-10-13

The [minimum absent-row cache](rfc06-overlay-negative-cache-2026-10-13.md) retained
every queued key forever, because staging alone could not prove DB durability.
That safe fallback disabled the cache after 16,384 distinct queued keys, limiting
its long-term effect. The formal production writers now use tracked batch methods
and acknowledge tickets immediately after the corresponding `db.write(batch)`
succeeds. Row serialization, consensus roots/hashes, public presets and genesis
are unchanged. Initially unwritten zero-contribution keys still use the same cache.

Each queued mutation adds one pending reference for its key under the shared lock.
The returned ticket is non-Clone, must-use, and consumed by `committed()`. Acknowledging
one of several same-key batches removes only its reference, regardless of commit
order; reads cannot cache that key's absence until all tracked batches commit and
are acknowledged. A generation change invalidates an independently cached sibling's
older misses. The writer's own cache removes only the acknowledged key and retains
unrelated stable misses, avoiding a whole-window flush on every block commit.

A failed DB write reaches no acknowledgement. Dropping a ticket, cancelling its
batch, enqueue failure or unwinding keeps its key pending. Both distinct pending
keys and references on one key are capped at 16,384. Saturation disables the negative
cache permanently for that invalidation domain and clears its pending map; ordinary
DB reads remain available. Acknowledging later tickets cannot reactivate an unsafe
domain whose abandoned references were discarded. Reopen/fresh store resets it.
Legacy untracked APIs retain their conservative pending marks. All current production
batch sites use tracked methods; tests/diagnostic legacy calls remain compatible.

| Formal batch writer | Staging | Successful commit acknowledgement |
| --- | --- | --- |
| Virtual `commit_utxo_state` | Optional rewarded-key and quality-pool insert tickets | After its DB write, before status guard release. |
| Pruning `prune` | Rewarded-key and quality-pool delete tickets | After its DB write, before reachability/status/relation guard release. |
| `import_pruning_point_overlay_snapshot` | Singleton snapshot ticket | After its DB write, with outer monetary/overlay guards held. |
| `capture_pruning_point_overlay_snapshot` | Singleton snapshot ticket | Immediately after its DB write. |

Existing direct methods invalidate within their shared lock over their direct DB
writer, requiring no queued-batch acknowledgement. Store `clone_with_new_cache`
shares the pending/generation domain. These tickets govern negative coherence;
they do not redefine generic caches' historical optimistic **present-value** update
order across independent cache instances. The singleton reverse-overwrite control
checks each committed DB result through a fresh positive cache. No inference about
arbitrary raw DB writers or general positive-cache transaction isolation is made.

## Validation

Direct matching-library compilation passed. All **23 leaf tests** passed with no
ignored cases (0.28 s test body; 21,643,264 bytes whole-process maximum RSS). The
suite includes actual RocksDB insert/delete batches committed in reverse order,
independent-reader missing-before-commit/present-after-commit, singleton overwrite
and abandon controls, failure/unwind/abandoned-ticket retention, per-key and distinct
key saturation, late-miss concurrency, delete/restore/reopen and prior store tests.
A control acknowledges 32,768 consecutive tracked mutations: pending state returns
to zero each time, the cache stays enabled, and an unrelated missing row performs
exactly one underlying read across all those commits.
[Plan](evidence/rfc06-overlay-commit-tickets-2026-10-13-plan.json),
[raw results](evidence/rfc06-overlay-commit-tickets-2026-10-13-tests.log),
[compile command](evidence/rfc06-overlay-commit-tickets-2026-10-13-command.json)
and [source/library/binary hashes](evidence/rfc06-overlay-commit-tickets-2026-10-13.json)
are retained. These leaf controls include production store sources; they do not
execute the asynchronous virtual/pruning processors. Their hook sites were audited
for successful-write-only ordering; full integrated compilation and runtime remain
pending. Existing active DNS/v2-reward and verified historic-overlay-import tests
are relevant pipeline controls after the new integrated binary builds. Real pruning
GC/capture must additionally be exercised in the node integration run.

The controlled adjacent-SST timing remains historical evidence from the preceding
minimum-cache source. This ticket follow-up has not yet passed renewed native100k
whole-node CPU/RSS/latency, high-entropy carriage wire capacity, genuine multi-process
pruning or all-model economic resource acceptance.
