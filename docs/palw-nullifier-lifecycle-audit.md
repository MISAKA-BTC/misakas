# PALW Nullifier Lifecycle — Audit Record (2026-07-25)

Answer to the release-audit item "global nullifier: fork/reorg-safe implementation, duplicate
rejection across forks". Verdict up front: **there is deliberately no single global nullifier
set**, and after this audit that is recorded as the design rather than an omission — with its
invariants now pinned by tests, and its genuinely open edges listed at the bottom.

## Architecture (as found, with the load-bearing code)

Three nullifier kinds, three different enforcement coordinates:

1. **Ticket nullifier** (`palw_ticket_nullifier`, header field; commitment in the leaf).
   Dedup lives in **GHOSTDAG coloring**: each block's active-window set is seeded from its
   selected parent's persisted window (fail-closed if missing) plus the SP's own ticket; a blue
   mergeset candidate reusing an active nullifier is **recolored red** — the block stays in the
   DAG, its work is simply never credited (`processes/ghostdag/protocol.rs:215-250`). The
   per-block window is persisted (`model/stores/palw_nullifier.rs`), retention-bounded
   (`prune_below`), and carried across the pruning point in
   `PalwPrunedFrontierV1.active_nullifiers`.
2. **Job nullifier** (`leaf.job_nullifier`). Dedup lives at the **virtual/UTXO reward
   coordinate**: a duplicate within the bounded selected-chain paid-work walk is classed
   `ReplicaPalwDuplicateWork` — providers get nothing, the base is burned
   (`virtual_processor/utxo_validation.rs:1115`). The body-coordinate registry was deliberately
   REMOVED (ADR-0040 P1-9) and a test pins its absence.
3. **Receipt v3 execution nullifier** — off-chain (`mil/palw`) only. Consensus never stores or
   checks it; admission of receipts is the node lifecycle/settlement layer's job, not header
   consensus. Any claim that consensus enforces it is false and must not be made.

Duplicate handling is **credit denial, not block rejection**, everywhere. "Reject the block"
would turn a merge of honest anticone forks into an invalid block — recolor-red is the DAG-native
form of rejection.

## The conservation invariant (what "fork/reorg-safe" actually means here)

Per-block windows are immutable and past-relative, so a sink reorg never rewrites any set. The
same nullifier CAN be blue on two forks in mutual anticone — and each fork's selected chain
credits it at most once; the first block that merges both histories recolors the losing use red.
So the enforced invariant is:

> **Along any single selected chain, a nullifier is credited at most once.**

That is exactly what value settlement consumes (settlement reads the selected chain), and it is
now pinned end-to-end by `palw_algo4_sink_reorg_cross_fork_nullifier_replay_e2e`
(`virtual_processor/tests.rs`): fork A credits N once → real sink reorg onto fork B → fork B
credits N once (accepted by design, asserted) → the merge block recolors the fork-A use red and
credits nobody. Companion tests: same-mergeset reuse, buried-window reuse, job-nullifier dedup
within and across chain blocks.

## Status of the audit gaps

| Gap | Status |
|---|---|
| Same-mergeset / buried-window ticket reuse | enforced + tested (pre-existing) |
| Cross-fork reuse across a real sink reorg | enforced at merge; **now tested** (this audit's new e2e) |
| Set commitment in the header | v3: raw ticket only, set UNCOMMITTED — by design until the Header-v4 re-genesis (`overlay_commitment_root` path exists, inert); do not claim header-committed sets on v3 |
| Post-retention-window ticket replay | blocked indirectly by the clause-5 target-DAA-interval binding, not by the set — a COUPLING, recorded as such |
| Job-nullifier replay beyond `paid_work_walk_bound_daa` | **open edge**: outside the bounded walk the reward coordinate cannot see the earlier payment; bound sizing vs batch lifecycle needs an explicit parameter argument before any public/value activation |
| Prune-then-replay integration test | **missing**: frontier import consistency is unit-tested, but no integration test moves a real pruning point and replays an in-window nullifier; needs a long-chain harness |
| Whole lane dormant on shipped presets | `palw_algo4_accept = false` everywhere; every path above executes only with the lever forced open (tests) — activation is a re-genesis-scale decision |

The last three rows are the honest residue: two need engineering (a parameter argument and a
pruning harness), one is governance. Everything else is design-working-as-intended with tests.
