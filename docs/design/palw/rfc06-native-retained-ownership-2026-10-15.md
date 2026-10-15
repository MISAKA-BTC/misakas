# RFC0006 native ownership source checkpoint (2026-10-15)

This records implementation source, not executed acceptance evidence. The user requested all code changes before consolidated validation; no build, test, benchmark or node run was performed for this checkpoint. Formatting and source generation do not establish correctness, 100k capacity, physical resource approval or model coverage.

## Ownership and compatibility

`NativeRetainedV1<T>` retains its allocation ticket while the value or any containing shared Arc remains live. Its Borsh serializer delegates directly to T, and no consensus field, state root, codec version, public preset or activation changes. Deep clones reserve a separate typed copy before construction. Public Deref is immutable; there is no DerefMut. Legacy owned extraction refuses detaching a ticket when either explicit native capacity capability is active. Core-only audited mutation reserves the source copy plus the specialized owner's actual typed growth before cloning/changing; remote callers cannot supply a generic growth quote through that API.

Cold native tip, pruning snapshot and delta readers use borrowed bincode record views over RocksDB pinned bytes. Their actual Borsh payload undergoes structural preflight before native decode/restoration. The canonical recorded state root remains mandatory. A cache hit shares the owner, and replacing the cache cannot release a ticket still held by a reader. The original serialized record layout is retained. Legacy bare record APIs remain available with native capabilities disabled and refuse strict unowned record extraction.

The candidate state, read snapshots, pruning shared carriage, process walk and Rule E cache retain the owner. `NativeCarriageSnapshotV22` changes only its two Arc element types to retain the same owner; no Claude-owned RFC0016/RFC0017/ADR policy or behavior is changed.

## Replay and tracked writes

`native_delta_replay_requests_v1` counts the actual typed delta, old state copy and per-row decode temporaries. Vertex, improvement, EXEC and budget opaque row codecs follow the concrete map selected by production replay. Kernel rows use the actual standalone ledger/aux scanner; existing rooted ledger/aux consumer projections and finite composite references are counted separately. Existing restoration source cardinalities are taken from the current state. New memberships use the maximum of concrete primary/derived/index node layouts, with checked arithmetic. Two index generations cover replacement coexistence. This is a conservative logical request quote, not RSS or a calibrated fee.

`try_delta_v1` reserves before apply/revert and returns a new retained state. Source tests cover malformed row rejection before mutation and restoration-count overflow. Native field visits now count nested typed memberships separately from raw byte-vector elements.

Tracked tip, pruning and delta writers return `NativeStoreWriteTicketV1`; processor consumers hold it through DB commit or the synchronized import barrier. Legacy untracked writer calls refuse strict use. Normal singleton/delta writers stage the unchanged bincode record directly and retain no optimistic serialized payload cache. Tip/pruning write admission uses the database-aware install quote, including its separate pre-carriage-copy reservation. Delta serialization uses an allocation-free canonical count, actual typed delta workspace, record framing and explicit simultaneous encoded-vector lifetimes.

## Typed source closure additions

The generator now reaches the actual state delta, native settlement evidence, fork-choice delta, exact provider store, held/kernel/segmented bundle types and new RFC08 recovery rows. Aux134 dispatch distinguishes root64 contract/terminal rows from key69 provider checkpoint rows. Generated layout coverage is a source inventory; its count is not proof every consumer retains the correct ticket.

Flat7/9 persisted programs and typed/composite spec roots have structural/derived workspace visitors. Composite work counts actual components and the non-recursive primary decoded pool. The source26 private fields have an exhaustive read-only owner bridge; field visibility and consensus encoding are unchanged. Actual table30 source rows separately bound the borrowed pending-source restore BTreeSet.

## Explicitly unfinished code/acceptance work

- Fresh block folds and accepted-object rehearsals still construct the result/workspace before `retain_existing_v1`; that method covers later lifetime only. Complete pre-construction growth/temporary bounds remain necessary.
- Mutable `ReservedKernelLedgerV1` consumers still need growth-aware admission for every mutation; the retained native state owner does not authorize arbitrary ledger growth.
- The legacy `PalwStateSyncV2` helper accepts a caller-owned multi-step WriteBatch and still needs a tracked batch lifetime API. Strict untracked setters now refuse it; refusal is not completion.
- Other complete DB/history iterators, path construction vectors, read projections, readiness memo/transport and private provider transport allocations require explicit consumer accounting. Native settlement row/facts work is a separate root-owned scope.
- Provider recovery/mutation/publication must connect exact outer and inner codec scans, simultaneous recovery copies, specialized mutation growth and retained ownership. A raw provider material capacity is not native RAM authority.
- Known recurrence/state-map and multi-router/Kimi downstream consumer coverage, actual all-model boundary/cost admission, and authenticated legacy post-genesis canonical identity migration remain separate RFC0006 work. Rejecting unknown geometry or missing identity does not close these requirements. No model acquisition gate is introduced.
- Actual 100k accepted population, whole-node resources, complete snapshot wire budget and pruned P2P restart/reorg acceptance still need the consolidated, predeclared validation phase. Prior timed-out/incomplete runs remain incomplete.
- Kernel hard quota/backend coverage and explicit unsupported platforms remain independent from logical typed requests. No numeric price, calibration, hard-quota approval or C1–C5 decision is inferred from this source checkpoint.

## Validation source prepared, not run

Borrowed record tests assert exact existing bincode decoding and in-frame payload borrowing, including an impossible length rejection. Retained owner tests assert old readers keep their ticket after cache replacement and delegated Borsh bytes equal the original value bytes. Replay tests assert malformed opaque rows and overflow refuse before state mutation. Existing default-off native walk/root/reorg tests remain the compatibility baseline. Consolidated compilation and test execution are still pending.
