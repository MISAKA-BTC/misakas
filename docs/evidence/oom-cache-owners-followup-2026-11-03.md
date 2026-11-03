# Remaining cache owners and reachability copy correction

Read-only audit of integration `12ae97094d151c7e8eb75e23776ead6e6a6cfc9f`, followed by the narrow reachability correction in this commit. Production95's reported 9.6 GiB memory plus swap was not reproduced locally. No SSH, Node operation or heavy Cargo build was performed. The reachability correction changes no EVM/native-admission/consensus API behavior. A subsequent EVM cache retention correction is recorded below; neither change is in frozen integration12.

## Reachability: an avoidable deep copy on every cached update

`consensus/src/model/stores/reachability.rs::DbReachabilitySet::{append,insert,replace}` previously called `cache.get`, then `Arc::make_mut` on the returned Arc, then reinserted it. `database/src/cache.rs::Cache::get` clones the Arc while the cache retains its original Arc. Consequently `make_mut` necessarily copied the full set even when no external reader existed. With N successive additions to one large cached set this introduces O(N²) copied hash elements. This is allocation churn, not evidence of an infinitely retained set or attribution of production OOM.

The correction updates the cache-owned Arc through the existing `Cache::update_if_entry_exists`. Its write lock, old/new tracked-size accounting and eviction remain in force. A cache-only Arc can mutate its Vec without cloning; external readers still trigger copy-on-write and retain their immutable snapshot. DB write/delete operations and ordering remain unchanged. Read-side cold loads still collect and sort a complete set, and an external live reader still legitimately requires a copy.

Two real Cache/DB regression tests cover append, insert and replace: sufficient reserved Vec capacity preserves its buffer address with no reader; a held reader keeps the old ordered contents while the cache advances. They also check persisted membership and tracked eviction without dropping DB members. These tests are added for central Cargo execution and have **not** been executed locally.

A lightweight rustc harness extracts the three actual production mutation methods verbatim, substituting u64 for BlockHash, a small mutex-backed Arc cache and no-op DB writes. Both after-change ownership tests pass; the before-change code fails the cache-only no-copy assertion. This establishes the Arc/Vec ownership correction, not RocksDB behavior or production cache eviction. Artifacts are `/tmp/misaka-t12-oom-20261102/reachability_cow_harness.py`, `reachability-cow-{before,after}.rs` and corresponding logs. rustfmt and `git diff --check` also pass.

## EVM full-state cache: concrete remaining byte-unbounded retention

The following are source facts in integration12, not measurements of production95's EVM account count or memory:

- `consensus/src/consensus/storage.rs:414` creates `DbEvmStateStore` with `max_items(64).untracked()`. `cache_policy_builder.rs::noise` adds 0–15 items, so the effective count cap is **64–79**, not an exact64. There is no byte ceiling on this cache.
- `consensus/src/model/stores/evm.rs:92` holds `CachedDbAccess<BlockHash, EvmStateSnapshot, BlockHasher>` by value. Its `get` delegates to `access.read`; `insert_batch` passes the full snapshot into `access.write`.
- `database/src/access.rs::CachedDbAccess::read` calls `cache.get` on a hit. `database/src/cache.rs::Cache::get` invokes `TData::clone`; for this value the accounts Vec, each code Vec and each storage Vec are deeply cloned. On a cold read, the full snapshot is decoded first and then cloned into the cache. Concurrent cold misses are not single-flight at this generic boundary.
- `consensus/core/src/evm/mod.rs::EvmStateSnapshot` derives Clone and contains `Vec<EvmAccountSnapshot>`. Its existing byte estimator is `size_of(snapshot) + accounts.capacity()*size_of(account) + sum(code.capacity() + storage.capacity()*size_of((EvmU256,EvmU256)))`. This is capacity-based account/code/storage ownership, not wire length.
- `consensus/src/pipeline/virtual_processor/processor.rs:3607` stores a full `staged.snapshot` for each accepting block when `evm_retire_206` is false. Thus the cache can hold many full historical account-state generations independently of model producer roles. `kaspad/src/args.rs` defaults both shadow state backend and retire206 to false; actual production flags remain a root-operated observation.
- Parent snapshot reads also occur in the shadow seed path (`processor.rs:3120`) and other EVM execution/read paths. Making model producer roles off does not disable EVM state caching. This does not establish that production95's particular DB contained large EVM snapshots.

Let S be the sum of the actual capacity estimate above for one representative state generation. Similar-sized cached generations retain approximately 64S–79S, before cache-map overhead, current execution state, readers' deep clones, serialized batch buffers and allocator retention:

| One full snapshot S | Cache alone at64–79 entries |
| --- | --- |
| 16 MiB | 1.00–1.23 GiB |
| 32 MiB | 2.00–2.47 GiB |
| 128 MiB | 8.00–9.88 GiB |
| 160 MiB | 10.00–12.34 GiB |

For unequal generations the relevant quantity is the sum of their actual capacities. A warm read adds another full independently owned snapshot while its cache generation remains live; several simultaneous readers add several such copies. Count bounding alone therefore does not bound bytes. This is a concrete remaining owner and a next measurement target, **not** proof that it caused production95's OOM. EVM payload/checkpoint/trace caches also use count policies, and payload deep copies are already recorded in `docs/evidence/oom-getblocks-followup-2026-11-02.md`; they are not fixed here.

No EVM cache policy or API was changed by the reachability correction. A subsequent focused change needs actual state capacity/cached-generation evidence, a byte-budget choice consistent with total memory ownership, meaningful cold/warm/concurrent tests, and preservation of DB/root semantics. Changing only the count or wrapping a post-clone value in Arc would not admit cold decode or existing independent copies.

## Native PALW and DHAT limits

`DbPalwStateV2Store` in integration12 keeps one decoded tip and one decoded pruning snapshot, uses distinct shared cold-load mutexes, drops its cache-only stale generation before replacement and returns Arc clones on hot reads. Those facts do not demonstrate an unbounded decoded cache. Other live Arc owners and cold materialization still require the existing reservation accounting; they are not made free by cache eviction. Reachability's children/future-covering caches use tracked units with min_items0, so they have a logical retained-element bound; that bound does not pre-admit a cold bucket Vec or all concurrent copies.

The delayed DHAT65 R3 report completed180 seconds and sums to33,286,961 live tracked bytes at its end. Its timer starts after full constructor completion, leaving pre-arm allocations untracked, and its unsigned/private-DB System-allocator execution is not production95's MiMalloc process or workload. It cannot account for native C++/mmap allocations and cannot rule out large pre-arm cache/baseline owners. The report does show native carriage/state BTreeMap decodes/clones among tracked sites, but neither that ranking nor this source review identifies the 9.6 GiB production owner. Further attribution must preserve that distinction.

## Follow-up: exact byte retention cap for EVM full-state snapshots

A separate follow-up change replaces only the full-state cache's Count64–79 policy with `CachePolicy::Tracked { max_size: scaled(40_000_000), min_items: 0, mem_mode: MemMode::Bytes }`. It uses the existing `EvmStateSnapshot` capacity estimator and normal random eviction. The direct policy is deliberate: `PolicyBuilder::tracked_bytes().build()` adds up to7,680 positive bytes of noise, whereas this cache's declared ceiling is exact. The generic builder and other EVM caches are unchanged. An individually oversized snapshot is evicted rather than being retained as a minimum entry.

The declared consensus byte-budget sum gains `scaled(40_000_000)`: 1,480,000,000 bytes at scale1.0 and333,000,000 at scale0.225. These remain declared object/cache estimates, excluding cache-map/allocator overhead and independently owned reader allocations. Existing declared-budget tests were updated and pass in a lightweight rustc harness using the actual production function and existing test module.

Two real DB regressions are added for central Cargo execution: an oversized snapshot at zero/small budgets is never cached and repeated/restarted reads preserve canonical snapshot bytes; eight large generations respect a two-snapshot byte budget after insertion and cold/warm reads while every DB answer remains byte-identical. They are not run locally. These tests deliberately use exact-length-capacity fixtures for probing cache retained weights.

This follow-up bounds cache retention only. Generic cold decode still allocates before insertion/eviction; generic warm reads still deeply clone the snapshot, and simultaneous readers remain separate owners. No allocation refusal, state validity rule, serialization, persisted root or Consensus API is changed. Frozen integration12 and production are not changed by preparing this later commit.
