# RFC0006 native ownership source checkpoint (2026-10-15)

This records implementation source, not executed acceptance evidence. The user requested all code changes before consolidated validation; no build, test, benchmark or node run was performed for this checkpoint. Formatting and source generation do not establish correctness, 100k capacity, physical resource approval or model coverage.

## Ownership and compatibility

`NativeRetainedV1<T>` retains its allocation ticket while the value or any containing shared Arc remains live. Its Borsh serializer delegates directly to T, and no consensus field, state root, codec version, public preset or activation changes. Deep clones reserve a separate typed copy before construction. Public Deref is immutable; there is no DerefMut. Legacy owned extraction refuses detaching a ticket when either explicit native capacity capability is active. Core-only audited mutation reserves the source copy plus the specialized owner's actual typed growth before cloning/changing; remote callers cannot supply a generic growth quote through that API.

Cold native tip, pruning snapshot and delta readers use borrowed bincode record views over RocksDB pinned bytes. Their actual Borsh payload undergoes structural preflight before native decode/restoration. The canonical recorded state root remains mandatory. A cache hit shares the owner, and replacing the cache cannot release a ticket still held by a reader. The original serialized record layout is retained. Compact identity subkeys under the existing singleton prefixes are committed with the payload in one batch; cache-hit identity reads do not checksum the full large record. Existing databases fall back to the borrowed payload view until a trusted writer creates the sidecar. Schema retirement/deletion clears sidecars in the same batch. Legacy bare record APIs remain available with native capabilities disabled and refuse strict unowned record extraction.

The candidate state, read snapshots, pruning shared carriage, process walk and Rule E cache retain the owner. `NativeCarriageSnapshotV22` changes only its two Arc element types to retain the same owner; no Claude-owned RFC0016/RFC0017/ADR policy or behavior is changed.

## Replay and tracked writes

`native_delta_replay_requests_v1` counts the actual typed delta, old state copy and per-row decode temporaries. Vertex, improvement, EXEC and budget opaque row codecs follow the concrete map selected by production replay. Kernel rows use the actual standalone ledger/aux scanner; existing rooted ledger/aux consumer projections and finite composite references are counted separately. Existing restoration source cardinalities are taken from the current state. New memberships use the maximum of concrete primary/derived/index node layouts, with checked arithmetic. Two index generations cover replacement coexistence. This is a conservative logical request quote, not RSS or a calibrated fee.

`try_delta_v1` reserves before apply/revert and returns a new retained state. Source tests cover malformed row rejection before mutation and restoration-count overflow. Native field visits now count nested typed memberships separately from raw byte-vector elements.

Tracked tip, pruning and delta writers return `NativeStoreWriteTicketV1`; processor consumers hold it through DB commit or the synchronized import barrier. Legacy untracked writer calls refuse strict use. Normal singleton/delta writers stage the unchanged bincode record directly and retain no optimistic serialized payload cache. Tip/pruning write admission uses the database-aware install quote, including its separate pre-carriage-copy reservation. Delta serialization uses an allocation-free canonical count, actual typed delta workspace, record framing and explicit simultaneous encoded-vector lifetimes.

## Typed source closure additions

The generator now reaches actual state delta and transition inputs, native settlement evidence, fork-choice delta, exact provider store, held/kernel/segmented bundle and response types and new RFC07/RFC08 rows. Handwritten FP job version tails and dtype-width v3 leaves have dedicated wire adapters; pinned source review detectors prevent silently treating a changed custom decoder as a derive layout. The v3 leaf serializer streams identical lane bytes from fixed stack scratch instead of creating a full extra encoded Vec. Aux134 dispatch distinguishes root64 contract/terminal rows from key69 provider checkpoint rows. Generated layout coverage is a source inventory; its count is not proof every consumer retains the correct ticket.

Flat7/9 persisted programs and typed/composite spec roots have structural/derived workspace visitors. Composite work counts actual components and the non-recursive primary decoded pool. The source26 private fields have an exhaustive read-only owner bridge; field visibility and consensus encoding are unchanged. Actual table30 source rows separately bound the borrowed pending-source restore BTreeSet.

## Explicitly unfinished code/acceptance work

- Fresh transitions/rehearsals now reserve the initial typed parent copy and rollback copies before Clone and return separately retained state/delta/skip outputs. Bond construction, map/journal clone and registration lookup growth have fallible preallocation. The other shared writers, directly mutated indexes, per-operation derived temporaries and complete ledger `apply_object` workspace still require before-allocation admission. The final output bound check is a diagnostic/refusal, not permission for an earlier unaccounted allocation.
- Mutable `ReservedKernelLedgerV1` consumers still need growth-aware admission for every mutation; the retained native state owner does not authorize arbitrary ledger growth.
- The legacy `PalwStateSyncV2` helper accepts a caller-owned multi-step WriteBatch and still needs a tracked batch lifetime API. Strict untracked setters now refuse it; refusal is not completion.
- Other complete DB/history iterators, path construction vectors, read projections, readiness memo/transport and private provider transport allocations require explicit consumer accounting. Native settlement row/facts work is a separate root-owned scope.
- Provider recovery/mutation/publication must connect exact outer and inner codec scans, simultaneous recovery copies, specialized mutation growth and retained ownership. A raw provider material capacity is not native RAM authority.
- Known recurrence/state-map and multi-router/Kimi downstream consumer coverage, actual all-model boundary/cost admission, and authenticated legacy post-genesis canonical identity migration remain separate RFC0006 work. Rejecting unknown geometry or missing identity does not close these requirements. No model acquisition gate is introduced.
- Actual 100k accepted population, whole-node resources, complete snapshot wire budget and pruned P2P restart/reorg acceptance still need the consolidated, predeclared validation phase. Prior timed-out/incomplete runs remain incomplete.
- Kernel hard quota/backend coverage and explicit unsupported platforms remain independent from logical typed requests. No numeric price, calibration, hard-quota approval or C1–C5 decision is inferred from this source checkpoint.

## Validation source prepared, not run

Compact identity source tests cover batch abandonment, clone-new visibility, deletion, legacy fallback and unknown version refusal. Borrowed record tests assert exact existing bincode decoding and in-frame payload borrowing, including an impossible length rejection. Retained owner tests assert old readers keep their ticket after cache replacement and delegated Borsh bytes equal the original value bytes. Replay tests assert malformed opaque rows and overflow refuse before state mutation. Existing default-off native walk/root/reorg tests remain the compatibility baseline. Consolidated compilation and test execution are still pending.

## Fixed-membership ledger mutations (source only)

`ReservedKernelLedgerV1` now owns fallible insertion APIs for collateral sync, artifact attestation, OPV admission and the job pointer written by Panel/paid tallies. Each derives its one concrete BTree node from the actual absent key, extends the same retained ticket **before** mutation, and preserves existing-key/idempotent updates without another insertion request. The production kernel fold and paid tally callers use these APIs. Capacity refusal propagates before a new membership is installed. Source tests cover refusal preserving the map, idempotence, lease lifetime and semantic refusal without an insertion. They have not been executed.

This is a specific consumer correction. General `apply_object`, fold temporaries, injected consumer projections, `DerefMut` field access and the remaining `TransitionBuilder` writers/temporary construction remain separate preallocation gaps; these fixed insertion methods do not approve their workspace or make native resource closure complete.

## Retained fresh transition and local error propagation (source only)

The native processor uses retained V7, pre-object and one-object rehearsal APIs. Historical bare APIs extract only with native capacity flags disabled. The construction reservation is split into independently retained state/delta/skip owners; unused temporary reservation is dropped, and a final output beyond the already reserved amount refuses without a post-hoc reservation. The delta ticket reaches the UTXO commit. Genesis borrows its configured registration list, uses the retained transition and holds its exact canonical declaration workspace through the atomic commit. This does not establish complete before-allocation coverage for the still-unconverted writers.

`NativeDecodeErrorV1` separates capacity from decode/state errors. The store has a Core-independent `LocalResourceUnavailable(String)` category; retained walks preserve native coverage causes. The virtual walk propagates capacity through `Result` before setting any disqualified status or reporting a reached ancestor as success. Parent-owned startup/worker/result dispatch forwards that class; ordinary corruption remains a separate failure. Source tests for split ownership, unchanged reservation on failure, local classification and default-off canonical byte parity have been added, but not run.

The recovery Header/envelope/Quote roots and latest aux134 fields are included in generated native field/wire visitors. `Header` uses its actual derive layout, private compressed parent runs are visited beside their owner, and `Uint576` is the actual fixed nine-limb (72-byte) Borsh field. The 1,127-type inventory is a source closure count, not execution, cryptographic review, allocator approval or 100k acceptance evidence.


## Class and shard writer admission (source only)

Twelve additional shared writers now return local resource failure before a map or journal change: class, capability, class walk, attempt/receipt targets, share, canonical semantics, class/bond shard declarations, held step ladder, court window and TIR shard plan. Their common helper quotes the actual key/value heaps, absent typed BTree membership and the explicitly requested next journal allocation. Journal growth is geometric with checked arithmetic and `try_reserve_exact` after admission; existing and replacement allocations remain under the construction owner. The pinned BTree layout refuses unsupported pointer widths while tracking is active. Flags-off uses the historical allocation path.

Callers propagate these errors through the existing transition Result, including share growth, registry stepping and pooled receipt-target seeding. Bond-shard input cloning has a separate borrowed-input admission before construction. Source regression tests cover zero capacity preserving all five initial writer maps/journal, absent membership refusal even with spare journal capacity, and demand held through owner drop. These tests have not run. Other writers, caller-side TIR program/partition construction, direct index writes and their scratch remain open; this local helper does not approve those allocations.
