# Audited memory admission and remaining coverage — 2026-11-02

This change bounds audited allocations; it is **not a bound on every node allocation or a finding that all anonymous RSS is a leak**. No remote heap measurement or combined production OOM soak was performed by this work item. Consensus roots, model arithmetic, model tiers, claim certification and court decisions are unchanged. Capacity refusals use existing local-resource error paths, not invalid-block verdicts.

## Accounting

`kaspa_core::memory_budget` owns one total and four ceilings inside that total (Transport, Snapshot, Native, Cache). The default total is 70% of the minimum physical host RAM / finite cgroup-v2 ancestor limit, including the cgroup mount root. Unknown platforms use 2 GiB. `MISAKA_MEMORY_BUDGET_BYTES` may only lower this total. Native/Snapshot leave up to 128 MiB within the total for small transport work; Transport is capped at 1.5 GiB. The total is an audited-byte admission policy, not a promise that the unaudited remainder always fits the 30% margin.

Reserve before construction with `try_reserve`, then call `materialized` after construction. Only pending bytes subtract from the cgroup-aware live headroom; retained materialized bytes remain in the total without being subtracted from physical free RAM again. Clones share the same lease. Splits/merges transfer accepted ownership without another charge; independent copies need independent tickets. All refusal paths are immediate, so this bank cannot create an internal forever-wait queue. A caller can still remain unable to serve an operation larger than available capacity; incremental/disk-backed construction is a separate task.

Daemon startup installs a live probe and a host bridge. With `--palw-host-ledger-dir`, the audited bank and PALW execution estimates use that shared ledger. Otherwise Linux nodes use `/tmp/misaka-node-memory-<euid>` for the audited bank only. Participating processes must see the same directory/mount namespace; different users and nonparticipating applications are not collectively reserved by this file. Process identity includes PID/start-time/instance, and dead holders are pruned.

Each process replaces one audited total plus one **subset** pending row atomically under the host file lock. Pending rows are excluded from total bytes, so they do not double the charge. PALW execution estimates and audited pending promises both subtract from the observed live bound in either admission order. Audited retained owners count against aggregate host capacity rather than being subtracted a second time from live headroom. Host growth fails closed on ledger I/O errors; releases and phase changes are retried by a 5-second reconciler to repair stale live-process rows. An existing PALW estimate remains its own reservation; it is not also inserted into the process bank. Older PALW execution estimates retain their conservative estimate-vs-resident policy.

LazyFree observations record sampling start time, success/failure and bytes. Read failure clears credit, and age >=30 seconds gives zero credit, including a slow read or a stalled sampler. The existing 60% credit cap remains. Ancestor headroom now includes a finite limit at the cgroup mount root. Hard native quota evidence also scans ancestors, so an unlimited leaf does not hide an enforced parent limit. Swap is not capacity.

## Coverage

| Allocation / owner | This change |
| --- | --- |
| A's P2P/RPC payload, codec and snapshot owners | Consume the shared bank through A's independent integration; codec/payload copies remain distinct allocations. |
| Recognized canonical native carriage/value cold decode, strict mode enabled | Existing native checker plus shared bank, pending before decode; `NativeRetainedV1` seals materialization and retains bytes through cache/Arc lifetime. |
| Recognized canonical native carriage/value cold decode, ordinary node | Always-on process ticket without enabling strict schema/DB capability; known wire preflight derives construction bound before decode. |
| Unknown legacy native schema, ordinary node | Preserves existing decoder semantics. No invented structural bound; remains unaccounted by this bank. |
| Reviewed native projection/clone/mutation/delta replay | Known typed bound uses pending reservation; `from_owner_reserved_v1` seals construction. Unknown legacy typed bounds remain untracked rather than imposing strict schema acceptance. |
| Native tip and pruning carriage caches | Shared cold-reader gate prevents duplicate simultaneous materialization, with key reread after waiting. Cache hits retain Arc ownership and do not wait on the cold gate. DB writers do not acquire the cold gate; decode errors release it for retries. |
| Strict native WriteBatch and installation | Existing allocation tickets already remain through commit/abandonment. The checker now joins the process bank. Disk quota remains separate. |
| Ordinary native DB batches / bare state extraction | **Not fully covered.** Legacy bare-value extraction releases its wrapper ticket. Ordinary untracked writers can retain serialized batch copies; no blanket strict-mode enablement was introduced. |
| General native request/growth APIs | Existing general request API remains disabled without strict capability, because some legacy consumers flatten failures into semantic strings. When armed, known bytes join the bank as resident accounting. Default admission is limited to explicit new pending APIs and recognized cold-wire/retained boundaries. A materialization boundary was not guessed for existing callers. |
| PALW model/replay workspace | Existing PALW execution/share/host ledger retained; model workspace is not charged again into the process bank. This is not complete retained-allocation coverage for model caches. |
| RocksDB native caches, ordinary DAG/UTXO caches, allocator retained arenas, all SDK allocations | **Not fully covered.** Their RSS consumes live headroom, but unaudited retained allocations do not each own process-bank tickets. |

Native request tickets can still carry conservative whole-operation peaks for an owner's lifetime; this may refuse additional work earlier than actual RSS alone would require. No model root, quantization, rounding, context length or KV approximation was changed.

## Legacy growth boundaries not converted in the first implementation

`reserve_native_requests_v1` preserves compatibility (`None` without explicit strict capability); it is not globally activated across its legacy consumers. `grow_memory_v1` remains resident accounting for compatibility. `grow_pending_memory_v1` explicitly reserves new promises and requires `materialized_v1` after the actual construction. After the follow-up migration below, these production call sites still require boundary review (not including historical source copies under docs or unit tests):

| Source | Additional allocation / owner |
| --- | --- |
| `misaka/palw/sdk/src/claim_paid_v2/provider_reader_v4.rs` | Cumulative provider acquisition/checker workspace. |
| `misaka/palw/sdk/src/claim_paid_v2/provider_handoff_v4.rs` | Producer material codec units and retained complete material. |
| `consensus/core/src/coinbase.rs` | Admitted coinbase payload growth wrapper; allocation is in its caller. |
| `consensus/core/src/palw_court_shared_prefix_builder_v1.rs` | Prefix replay and authenticated boundary leaves. |
| `consensus/core/src/palw_court_shared_prefix_family_v1.rs` | Prefix boundary dependency vector. |
| `consensus/core/src/palw_routed_material_v2.rs` | Accumulated routed material units / transferred source owners. |
| `consensus/core/src/palw_native_resource_v1/kernel_salted.rs` (2 sites) | Reserved sink growth for salted material and commitments. |
| `consensus/core/src/palw_native_resource_v1/transition.rs` | Transition workspace/decode growth wrapper. |
| `consensus/core/src/palw_native_resource_v1/kernel_material.rs` | Kernel material collection growth. |
| `consensus/core/src/palw_native_resource_v1/ledger_expansion.rs` | Kernel ledger expansion growth wrapper. |
| `consensus/core/src/palw_native_resource_v1/kernel_ledger_mutation.rs` | Ledger mutation workspace growth wrapper. |
| `consensus/core/src/palw_source_bootstrap_v1.rs` | Shared Arc/bootstrap roster envelope. |
| `consensus/core/src/palw_native_settlement_resources_v1.rs` | Settlement sweep collection/vector growth. |
| `consensus/core/src/palw_step_refute/memory_v2/capture.rs` | Recurrence capture evidence. |
| `consensus/core/src/palw_step_refute/routing_v2/capture.rs` | Routing capture evidence. |
| `consensus/core/src/palw_step_refute/routing_v2/workspace.rs` | Weight-oracle gathered bytes, code/exponent and parameter workspaces. |
| `consensus/core/src/palw_court_archive_discharge_v1.rs` | Archived close decode. |

## Reviewed growth follow-up

Seven additional large construction boundaries now use `grow_pending_memory_v1`: numerical geometry second-pass output; source graph vertex/edge output; shared semantic full and table-only output; parameter cover and original root-input cover; and carriage restoration/index workspace. Each returns through `NativeRetainedV1::from_owner_reserved_v1`, which seals its materialized owner. Failure before that return drops the pending ticket. These migrations do not turn on existing general request APIs for default consumers or alter semantic/decode verdicts.

## Validation

- Standalone std `rustc --test core/src/memory_budget.rs`: 7 tests pass (shared classes, clone lifetime, split/merge/growth rollback, concurrent last-byte admission, pending/materialized progress, host hook I/O recovery, ancestor/root cgroup capacity).
- Standalone LazyFree module harness: 2 tests pass (initial/failed/stale/recovered readings, backwards timestamp, slow sample).
- Standalone host-ledger harness using the production module with only logging/file-identity/libc bindings stubbed: 5 tests pass, including real separate child processes competing for the same free bytes, release then progress, replacement and I/O recovery, and both PALW-estimate/pending admission orders. This is not a full daemon build.
- Added Cargo-native tests for real DB concurrent cold reads/failed decode then repair, and ordinary recognized native carriage/projection ownership. Full Cargo/daemon tests are delegated to the parent integration checkout to avoid concurrent heavy builds.
- No remote operation or deployment performed by this work item. Combined slow-peer/snapshot/IBD/reorg and real LLM maximum-context soak remains required before claiming production OOM remediation.
