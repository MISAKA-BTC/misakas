# RFC0008 settlement and provider GC source closure, 2026-10-15

Source only on `codex/rfc-completion-integration`, based on the integrated round 3 / round 4b code. No build, test, benchmark or node run was performed. Earlier passes do not validate these changes. RFC0016, RFC0017, ADR policy and public activation are outside this work.

## Minimum-price settlement

The old expiry sweep removed a signed minimum-price row when `retention_until_daa` passed, even when its worker's accepted claim still held a live job escrow. A delayed Final could then lose its minimum payout floor. The existing settlement consumer already enforces that floor; expiry must preserve its authority until escrow settlement or neutral release.

`palw_minimum_price_retirement_v1` now preserves the exact signed floor while the contracted worker holds the job and its escrow exists. Removing escrow retires the row. An unused/failed claim can still expire. This does not extend provider service, shorten any deadline, change earned cash, or approve a price. The public minimum-price policy remains dormant and refused when armed.

Prepared controls: a core state/codec/branch-rollback regression and `kernel/tests/minimum_price_settlement.rs`, which executes an actual typed dense/MoE kernel claim through the settlement bond book, checks a positive exact minimum once, restores the pre-Final branch snapshot, and checks the signed neutral outcome when the later branch cannot fund the floor. This is kernel/consumer evidence after execution, not a native coinbase or multiprocess result.

## Provider GC discovery and proof clocks

The old discovery cursor counted blocks only. If the per-tick witness budget was smaller than the number of observed consent generations, it repeatedly asked the first tuple forever. It also repeatedly scanned the first `scan_limit` blocks, so a later release was unreachable. Sorted contract traversal could starve later contracts.

The version 2 local journal records a flattened `(block, observed tuple)` cursor, a page anchor, and the next contract. Completed pages advance; a complete round restarts from the last authenticated present point so newly arrived finality is reconsidered. Contract traversal rotates. Old version 1 journals reset their block cursor safely before migration. A revoked, unexecuted intent is discarded while the bundle remains; it cannot pin discovery forever to one stale tuple. Every file read and nested journal allocation is bounded before decoding.

Op 242 proves the parent-state committed by a header. The remote GC oracle therefore checks the release header for the before-state and its selected-chain child for the after-state, requiring that child to be finalized too. Its `last_present` point is the pinned header's selected parent. Otherwise discovery skipped a release performed by the pinned header itself. Remote finality still depends on the explicitly configured finality authority; no remote authority is inferred from absence.

Prepared controls use an actual typed dense/MoE kernel bundle and the durable provider store: a one-call discovery budget across multiple generations, pagination past the first page, snapshot reopen and GC publication, old-journal migration, nested count bombs, and the remote before/after proof clocks. Their finality oracle is explicitly injected and is not native finality evidence.

## Authority-index cleanup and crash recovery

The node's provider authority index previously retained every claim forever. It now bounds and preflights fixed-width rows before allocation, rejects duplicate/unknown/incomplete authorities, and cleans only claims belonging to exact stored consents removed under verified native finality. Claims another retained consent still needs remain indexed.

A durable completed-GC receipt carries the exact witness and native claim set. The write-ahead marker saves that set before data deletion; receipt publication precedes marker removal. Restart can finish authority cleanup after the data generation was already removed. The node acknowledges a receipt only after the authority index is fsynced. No missing bundle or missing current covenant is treated as cleanup authority. Signing and GC both acquire the store before the authority index, closing a race where cleanup could erase an authority recorded just before a Ready waited on the store lock.

Prepared controls cover receipt replay/ack persistence and index bounds, duplicate rows, preservation of unrelated claims, and reopen. The completed receipt queue is finite; an application that never acknowledges metadata cleanup eventually refuses further native cleanup rather than silently forgetting receipts.

## Remaining verification and explicit limits

Run the scoped core/kernel/remote/provider controls in the later combined validation phase, then actual positive native funds through payment + W, restart/reorg and provider release/GC with real selected-chain finality. The existing native all-positive-money test and signed covenant test sources remain necessary. This audit does not call their pending execution a pass.

A provider that never observed a covenant's accepted generation/tail still pins it, as before. Safe automatic discovery of such entirely missed historical release authority is not supplied by absence. Economic collection probabilities, unbounded payment/recovery time costs, source independence and cross-host availability remain separately measured/reviewed requirements; these source fixes do not approve them.
