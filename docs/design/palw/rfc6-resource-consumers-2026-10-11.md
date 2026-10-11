# RFC06 cell resource consumers — 2026-10-11

This record extends the [four-RFC requirements index](rfc-0006-0007-0008-0012-design-audit-2026-10-10.md) and [SHARD implementation](shard-rfc6-10.md). It does not replace any RFC06 acceptance condition or approve a class, fee, soundness floor or activation.

## Concrete defects and implementation

A full-holder seat reserved host scratch and, when available, device memory. The shard-only seat and the non-seat public watcher started the same verifier without those reservations. Their work could overcommit a host while the producer, another seat or a court already held its budget. The shadow consumer also bypassed admission. The old seat released its admission immediately after checking cells, before building the exact court proof.

`kaspad/src/palw_panel/tir_shard_resource.rs` now owns the common admission. Full-holder seats, shard-only holders, public watchers and shadow runs use it. The CPU/device backend is constructed after host admission. If device memory cannot be reserved, CPU verification remains available under the host admission. A failed host reservation leaves the duty unanswered and retryable. The guard survives cell verification and accusation construction and releases on success, error or unwind. Scratch arithmetic is checked rather than saturated.

The common scratch envelope is the existing cell-seat node policy: three times the capture size plus 16 MiB. A decoded shard-only capture is counted without allocating another serialized copy. This is an admission mechanism and a bounded arithmetic expression; it is **not a verified worst-case RAM, work, elapsed-time or court-fee bound for every model**. Persistent fetched rows, material pools, model caches and exact prover workspace still need their own complete accounting and measurements.

A second defect defeated the intended shard memory scope. `TirExecutor::new_cell` initialized all `Fixed` state instances in the class, including every other layer. Restoring a segment repeated that full allocation. The executor now derives exactly the state instances touched by its immutable occurrence range, including read-only state. Other instances retain their indices with empty buffers. Checkpoint import/export respects that scope. An attempted wider cell, a whole-schedule step through a cell executor, or a checkpoint predicate naming another shard is refused before execution. Whole-schedule execution keeps its existing full state.

These changes are node execution/resource behavior. They allocate no consensus tag, delta, rooted field or new fence, and do not change live shard v1 pricing, draws, receipt rules, or public preset parameters.

## Evidence

`cargo test --locked -p misaka-palw-tir-exec --lib cell_resource_tests -- --nocapture` passed 2 tests with no failures or ignored cases, after a 3.12 s build; log `/tmp/rfc06-cell-memory-focused-final.log`. The tests verify scoped allocation after both restoration APIs, inclusion of read-only state, an actual cell step, unchanged full-state execution, and refusal of expanded scope or foreign-shard restoration. Initial test fixtures incorrectly used a noncanonical history bound and cross-role global state; those fixtures were corrected before this final run.

The common admission has additional tests for seat/watcher/holder contention, proof admission after release, checked overflow without mutation, error/unwind release, and refusal before backend construction. These require the centralized kaspad build and are not counted as passing before that build runs. Existing corpus cell verification, shard-only proof, non-seat conviction, device parity and node checks must also run against the integrated change.

## Full RFC06 closure conditions still tracked

| Requirement | Required remaining evidence or implementation |
| --- | --- |
| R6-CELL | Whole-claim coverage for every accepted versioned kernel/profile, including authenticated boundaries, memory and routing; sparse-error/algebraic soundness distinct from the first-divergence replay lemma; actual bounded public localization and terminal court for every covered relation. |
| R6-DRAW | Frozen population, retry/operator rules and one reservation have existing engine/fold tests. The actual production adapter truncates overfull populations by collateral then bond id and the dormant engine caps candidates at 4,096. A synthetic 100k ranking does not prove 100k eligibility or neutrality. The population policy, historical headroom, all actual consumers and transport require explicit closure. |
| R6-OUTSIDE | Authenticated public claim material, shard-only non-seat prosecution including dissected leaves, complete retention through the liability horizon, and pruned P2P recovery. Model acquisition remains off-chain under ADR177; unavailable models provide no positive detection floor. |
| R6-PRICE | Exact worst cell material/work/DA/court bounds and measured cost/capital terms, funded collectible fees, agreement between all lock/pay/readiness consumers, and real small/large/window-saturated results. Host readiness is node policy rather than an on-chain capacity proof. |
| R6-MIGRATION | Live t12 DAA 5,300 rules and dormant identities remain pinned. New profiles still require their own reviewed fence and activation evidence. Preserve historical open questions and distinguish existing v1 decisions from new-profile approval. |
| Resources/network | Release measurements at the actual intended 100k population and worst registered shape, with CPU/RSS/latency recorded; several actual processes exercising P2P/pruning/join/reorg. In-memory disclosure-pool and fold tests are not evidence of a pruned network join. |

All these conditions remain part of the full RFC06 objective. The memory fixes above do not turn any UNKNOWN/FAIL economics or public soundness gate into PASS.
