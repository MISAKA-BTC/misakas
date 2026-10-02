# MISAKA EVM State Backend — Design v0.1 (audit C-01 remediation)

**Status:** Design. Multi-session build. Consensus-NEUTRAL (no fork).
**Addresses:** security audit C-01 (Critical) + H-03 (High) + M-01 (Medium).
**Builds on:** the §12 archive substrate (`EvmStateDiffV2` / `EvmStateCheckpointV1` /
`reconstruct_evm_state`) already implemented this session.

---

## 1. Problem (audit C-01)

Every DAG block persists a **full** `EvmStateSnapshot` (DB prefix 206), keyed by block
hash (`consensus/src/model/stores/evm.rs`, written in `commit_utxo_state`,
`virtual_processor/processor.rs:~1140`). Consequences:

- **Storage O(state × blocks).** With 10 BPS and a ~1.08M-block minimum retention, a
  1 MiB state ⇒ ~1.03 TiB raw; a 10 MiB state ⇒ ~10 TiB. The §12 pruning GC only bounds
  *unbounded* growth past retention — within the retention window the full-snapshot-per-block
  duplication remains.
- **Empty blocks deep-clone the entire state** (`snapshot_from_cachedb` extracts the full
  account/code/storage set even when nothing changed).
- **Per-block O(state) CPU.** `seed_cachedb` (full parent state → revm `CacheDB`),
  `state_root` (full keccak-MPT recompute over every account), and `snapshot_from_cachedb`
  (full extraction) are each O(state) per block.

This is the #1 mainnet-EVM release blocker.

## 2. The consensus-neutrality invariant (the design's spine)

`EvmExecutionHeader.state_root` is **already** the keccak secure-MPT root computed by
`kaspa_evm::state::state_root` (`state.rs:51-63`, via `alloy_trie`) over the account set.
It is one of 18 borsh fields whose digest is `EvmExecutionHeader.commitment_root()` (keyed
BLAKE2b-512), and that digest — *not* the raw `state_root* — is what the verifier checks
against `Header::evm_commitment_root` (`processes/evm/mod.rs:~728`).

**Therefore any state backend that produces the byte-identical `state_root` for every block
is a pure storage/performance refactor — NOT a consensus fork.** No activation fence, no
re-genesis, no coordinated rollout is *required* for correctness; the only obligation is
that the new backend reproduce the same root. This is the single most important property of
this design and the spine of its proof plan (§7).

A corollary: **migration is node-local.** Because the committed bytes are unchanged, each
node may switch backends independently and roll back independently, exactly like
`--evm-history-mode` (§12). There is no consensus parameter to coordinate.

## 3. Key technical finding: `alloy-trie` is full-rebuild-only

The naive plan ("incrementally update a persistent Merkle-Patricia trie from `parent_root` +
the changed leaves, writing only the touched nodes") is **not supported by the pinned
`alloy-trie 0.7.9`.** Its `HashBuilder` consumes leaves in strictly ascending key order
(`hash_builder/mod.rs:118 assert!(key > self.key)`) and rebuilds the root from the *full*
sorted leaf set in one pass; there is no `TrieWalker`/`TrieCursor`/node-store cursor for
incremental updates (those live in reth's separate `reth-trie`, not `alloy-trie`). The
`proof` module only *retains* nodes along requested paths during such a full pass.

**Implication:** a content-addressed persistent trie *can* solve the storage amplification
(structural sharing ⇒ only O(changed) new nodes persisted per block), but computing the root
**incrementally in O(changed)** requires either a custom/vendored persistent MPT updater (a
new, consensus-critical, non-trivial component) or reth-trie. We must NOT put that on the
critical path for the storage fix.

This finding drives the staging in §4: **fix the storage (the 1 TiB headline) first, with no
new trie engine; defer incremental root-CPU + `eth_getProof` to a later, optional stage.**

## 4. Target architecture — staged

### Stage 1 — kill O(state × blocks) storage (the C-01 headline). No custom MPT.

Replace "full snapshot per block (206)" with a **single latest-canonical flat state + the
§12 diff chain + a block→root index**. Reuses the §12 engine maximally.

| Component | Prefix | Key → Value | Role |
|---|---|---|---|
| **Flat account store** | 234 | `address[20]` → `borsh(AccountCore)` (nonce/balance/code_hash) | O(1) point lookup of the **latest canonical** state |
| **Flat storage store** | 235 | `address[20] ‖ slot[32]` → `value[32]` (non-zero only) | O(1) latest-canonical storage |
| **Block→state_root index** | 232 | `BlockHash` → `state_root[32]` | O(1) `eth_getBlockBy*` root; canonicality-filtered |
| **Latest-state pointer** | 231 | `()` singleton → `{canonical_head, state_root}` | the flat store's current root |
| Content-addressed code | 222 | `code_hash` → `code` | **(exists, §12)** shared, never per-block pruned |
| `EvmStateDiffV2` | 220 | `BlockHash` → forward diff | **(exists, §12)** reorg revert/replay + history |
| `EvmStateCheckpointV1` | 221 | `BlockHash` → full checkpoint | **(exists, §12)** reconstruction seed |

**Lifecycle (Stage 1):**

- *Execute.* The executor seeds from the **parent** state. For the canonical head this is the
  flat store (build a `CacheDB` view lazily from prefixes 234/235/222 — fast path). For any
  other block in the reorg/retention window it is `reconstruct_evm_state(parent)` (§12 — walk
  the nearest checkpoint forward through diffs). The executor interface
  (`execute_block_from_snapshot(parent_snapshot, …)`) is **unchanged**; only how
  `parent_snapshot` is obtained changes.
- *Commit.* Compute `state_root` exactly as today (`alloy_trie` HashBuilder over the post-state
  — O(state) CPU, **unchanged, no regression**). Write: the §12 diff (220), new code (222),
  the block→root index (232); and **when the block extends the canonical head**, apply the
  diff to the flat store (234/235) incrementally (O(changed)) and update the latest pointer
  (231). **No full snapshot (206) is written.**
- *Empty block.* The diff is empty ⇒ zero flat writes, `block→root = parent_root` (share). The
  existing `empty_acceptance_result` fast path stays O(1).
- *Reorg (no re-execution, design §2.3 preserved).* Switching the canonical head re-bases the
  flat store: revert the §12 diffs from the old head back to the common ancestor, then apply
  forward to the new head (the §12 inverse-delta engine already does both directions). Cost is
  O(reorg-depth × changed), not O(state). Results/headers are never recomputed.
- *RPC.* `eth_getBalance`/`getCode`/`getStorageAt`/`getTransactionCount` at **latest** →
  O(1) flat lookup (fixes audit **H-03**). At a historical block → `reconstruct_evm_state`
  (§12, already wired in `account_at`).
- *Pruning.* The per-block 206 rows are gone, so there is nothing to GC there; §12 retention
  (slice 9) already governs diff/checkpoint lifetime. The IBD pruning-point snapshot becomes
  "ship the flat store at the current pruning point" — one full state, current-PP only (fixes
  audit **M-01** by construction).

**What Stage 1 fixes:** storage O(state×blocks) → O(latest state + window diffs + checkpoints)
(the 1 TiB headline); empty-block deep clone; RPC full-state reads (H-03); the IBD-snapshot
amplification (M-01). **What it does NOT fix:** the per-block O(state) `state_root` recompute
CPU (it stays exactly as today — acceptable, since it is not a regression and storage was the
dominant cost). That is Stage 2.

### Stage 2 — incremental root-CPU + `eth_getProof` (optional, higher risk, deferred)

Introduce a **content-addressed persistent MPT** (prefix 230: `node_hash → encoded node`,
structural sharing) with a real incremental updater (vendored reth-trie-style walker, or a
minimal in-house persistent MPT). The flat store (234/235) becomes the leaf source; the trie
gives O(changed) root computation and enables `eth_getProof` (audit follow-up / §12.8). This
is **only** justified once Stage 1 is proven and the per-block root-CPU is measured to matter.
It is gated behind the same consensus-neutrality proof (same root) and remains node-local.

## 5. Why not "trie as the Stage-1 source of truth" (the judge's first instinct)

The design review's first synthesis made the content-addressed trie the Stage-1 source of
truth. Given §3 (`alloy-trie` cannot update incrementally), that would force either an O(state)
HashBuilder rebuild every block anyway (no CPU win) **plus** a new consensus-critical MPT
persistence layer (new risk) — i.e. maximal risk for the storage win that the far simpler
flat-store + §12-diff path already delivers. Staging defers the trie to where it actually pays
off (root-CPU + proofs), after the headline storage fix is shipped and proven.

## 6. Slice plan (Stage 1 first; each slice consensus-neutral + offline-verifiable)

1. **S1 — flat state stores + reader.** Prefixes 234/235/231/232 + `DbEvmFlatAccountStore` /
   `DbEvmFlatStorageStore` + readers. Wire into `ConsensusStorage`. Inert (no writer).
   *Offline.*
2. **S2 — flat ↔ snapshot equivalence engine.** Pure functions: apply a `EvmStateDiffV2` to the
   flat store; materialize an `EvmStateSnapshot` from the flat store; `flat_state_root(flat)`
   == `state_root(snapshot)`. Property test against the §12 diff engine. *Offline.*
3. **S3 — `FlatBackedCacheDB` revm adapter** (`kaspa-evm`): a revm `Database` that lazily loads
   accounts/storage from the flat store (canonical head) — the executor's fast seed path.
   Falls back to `reconstruct_evm_state` for non-head parents. *Offline (synthetic stores).*
4. **S4 — dual-write (shadow) mode.** Behind a node-local flag, in `commit_utxo_state` ALSO
   maintain the flat store + block→root index alongside the existing 206 snapshot, and assert
   `flat_state_root == committed state_root` every block (the live differential check). Still
   writes 206. *Differential, live; the safety gate before cutover.*
5. **S5 — reorg re-base.** On canonical-head change, revert/apply §12 diffs to the flat store;
   test against a synthetic DAG with sibling reorgs; assert the flat store matches the new
   head's reconstructed state. *Offline (fake DAG) + live shadow.*
6. **S6 — executor seed switch.** Seed from `FlatBackedCacheDB` (head) / reconstruct (others)
   instead of `state_store.get`. Still dual-writing 206 for the differential. *Live shadow.*
7. **S7 — RPC point-lookups.** `account_at` (latest) → flat O(1); historical → reconstruct
   (already wired). Fixes H-03. *Offline + live.*
8. **S8 — IBD pruning-point flat snapshot.** Ship the flat store at the current PP (current-PP
   only, fixes M-01); import path seeds the flat store. *Live.*
9. **S9 — cutover (stop writing 206).** Shipped in three node-local, gated, reversible-by-design
   sub-slices:
   - **S9a — flat-authoritative executor seed** (`--evm-flat-authoritative`): seed the executor from
     the flat/reconstruct parent **after** asserting it byte-identical to 206 (HALT on divergence;
     206 still written ⇒ reversible). NOTE: S9a seeds by **eagerly materializing** the full parent
     snapshot (`materialize_snapshot` → `seed_cachedb`), not via the lazy `FlatBackedCacheDB` of S3
     — the lazy seed is S9c.
   - **S9b — retire 206 writes** (`--evm-retire-206`, requires S9a + shadow): stop persisting the
     per-block 206 snapshot; the flat store is the sole persisted post-state. The seed path falls to
     a committed-root check when 206 is absent; reads fall back 206 → flat-materialize → §12.
   - **S9b-prune — one-shot legacy-206 bulk reclamation** (`--evm-prune-legacy-206`): at startup,
     `delete_range` + prefix-bounded compaction of the legacy 206 store, gated on retire-206 being
     effective, recent/archive history, and the flat backend verified current+faithful at the EVM
     head (recomputed root == committed root). IRREVERSIBLE ⇒ refuses unless all hold.
10. **S9c — lazy `FlatBackedCacheDB` seam (production wiring; live cutover DEFERRED to Stage 2).**
    `StoreFlatReader` (consensus) implements S3's `FlatStateReader` over the real stores (234 + 222),
    so `flat_backed_cachedb(StoreFlatReader)` is the drop-in lazy seed Stage 2 plugs into; an offline
    test proves its reads reproduce the eager `materialize_snapshot`. **The live cutover (seeding
    `execute_block_evm` from the lazy backend) is intentionally NOT done in Stage 1**, for two
    reasons: (a) the committed `state_root` is a keccak-MPT over the FULL post-state, but a lazy
    `CacheDB` holds only the **touched** accounts — so a correct root after lazy execution still
    needs a full O(state) flat enumeration, which **negates the lazy-seed win until Stage 2's
    incremental MPT** (this is R4 made concrete); and (b) generalizing `execute_block_evm` from its
    `EmptyDB` (`Infallible` DB errors) to a fallible backend is consensus-critical churn that belongs
    with that Stage-2 work. So Stage 1 keeps the eager-materialize seed (S9a), which is correct and
    whose O(state) seed cost is the same order as the unavoidable O(state) root recompute.
11. **S10 — docs + operator runbook** (`docs/misaka-evm-flat-backend-runbook-v0.1.md`):
    shadow → cutover → retire → prune → rollback; metrics: flat-hit rate, reconstruct latency,
    root-divergence / Unavailable counters.

Stage 2 (S11+): persistent MPT node store (prefix 230), incremental O(changed) root, the live lazy
seed cutover (S9c), and `eth_getProof` — separate design revision once Stage 1 is live and per-block
root-CPU is measured to matter (R4).

## 7. Consensus-neutrality proof plan

The obligation is exactly: **for every block, the new backend's `state_root` is byte-identical
to the snapshot path's, hence `commitment_root()` is unchanged, hence no fork.** Tests:

1. **Synthetic equivalence (S2).** For 1000 random multi-account/storage transitions:
   `state_root(seed_cachedb(snapshot))` == `flat_state_root(flat-after-diff)`. 0 divergences.
2. **Ethereum KAT.** Known go-ethereum state roots reproduced by the same `alloy_trie` path
   (already implicitly covered — `state_root` is unchanged; this guards the leaf-extraction).
3. **Full-chain replay (S4 shadow).** For every canonical block:
   `committed_root == state_root(snapshot) == flat_state_root`. Any divergence ⇒ log
   `(block, expected, got)` and **halt that node** (never serve a wrong root). ~minutes/1M
   blocks single-thread; run continuously on testnet during shadow.
4. **§12 reconstruction parity (S2/S5).** For blocks at diff-distance D ∈ {1,10,100,1000}:
   `state_root(reconstruct(checkpoint, diffs)) == committed_root`.
5. **Reorg consistency (S5).** Sibling-reorg DAG: after head switch, flat store == new head's
   reconstructed state; RPC returns the correct branch.
6. **Empty-block (S6).** Zero changes ⇒ zero flat writes, `block→root == parent_root`.
7. **Pre-cutover gate (S9).** A node refuses cutover unless its shadow differential has 0
   divergences over the whole retained window.

Failure mode: shadow divergence ⇒ the node stays on prefix-206 (no disruption, no fork — the
committed bytes never depended on the backend); fix + re-shadow. This is why the refactor is
*safe to attempt*: the built-in oracle (committed `state_root`) catches any backend bug before
cutover, and a bug can only ever cost that node availability, never chain integrity.

## 8. Risk register

- **R1 (P0) — leaf-extraction / root drift.** If the flat store materializes a snapshot that
  differs from `snapshot_from_cachedb`'s canonical form (sort order, EIP-161 empties, zero
  slots, code⇔code_hash), the root diverges. *Mitigation:* reuse the exact canonicalization in
  `snapshot_from_cachedb`/`validate_snapshot_canonical`; S2 property test; S4 live differential.
- **R2 (P0) — reorg re-base correctness.** A wrong revert/apply leaves the flat store off the
  canonical state. *Mitigation:* §12 inverse-delta engine is already tested; S5 fake-DAG tests;
  S4 differential catches it live (root mismatch).
- **R3 (P0) — atomicity.** Flat-store update, block→root, diff, and the UTXO diff must commit in
  one RocksDB `WriteBatch` (as today). *Mitigation:* keep the single-batch commit;
  crash-consistency test.
- **R4 (P1) — root-CPU unchanged in Stage 1.** Per-block O(state) HashBuilder remains; on a very
  large state this is a throughput ceiling. *Mitigation:* it is not a regression (today's cost);
  Stage 2 addresses it; measure before committing to Stage 2.
  - **MEASURED (2026-06-25, `kaspa-evm/benches/state_cost.rs`, macOS arm64 dev machine; Linux x86
    likely the same order).** Per NON-EMPTY block (empty mergesets skip all three passes via the
    executor's O(1) fast path), as a function of account count N:

    | N | `state_root` (R4) | `seed_cachedb` | `snapshot_from_cachedb` |
    |---|---|---|---|
    | 1k | 1.15 ms | 97 µs | 49 µs |
    | 10k (≈ current state) | 11.8 ms | 0.97 ms | 0.72 ms |
    | 100k | 128 ms | 17.5 ms | 14.2 ms |
    | 1M | 1.33 s | 252 ms | 166 ms |

    `state_root` is ~1.2–1.3 µs/account (≈ linear; mild super-linearity from the O(N log N) leaf
    sort) and **dominates the three passes ~7–9×**. Against the 10 BPS **~100 ms per-selected-block
    budget**, `state_root` alone reaches 100 ms at **≈ 77k accounts**, and the full per-block state
    cost (root + seed + snapshot) crosses 100 ms at **≈ 60k accounts**. At the current ~10k state it
    is ~12 % of the slot (root) / ~13 % (all three) — fine, but with only **~6–8× account-count
    headroom**, NOT the ~100× the earlier Fermi estimate assumed. IBD catch-up (many blocks/sec) is
    stricter still, and a mining node pays it twice (produce + verify).
  - **R4 trigger, sharpened:** start the Stage-2 incremental MPT when the canonical account count
    approaches **~50k with sustained non-empty blocks** (or sooner if IBD-of-such-a-chain throughput
    is the binding constraint). The incremental root removes ~73 % of the per-block state cost (the
    `state_root` pass); the lazy-seed cutover removes only the ~12 % `seed_cachedb` CPU pass — so for
    CPU, **incremental root ≫ lazy cutover** (the measurement confirms the priority). NOTE the
    `seed_cachedb` figure here is only the CPU half; the eager seed's RocksDB read of all 234 rows
    (consensus-side, unmeasured) makes the real eager-seed cost — and thus the lazy cutover's full
    win — larger than the CPU column shows.
- **R5 (P1) — non-head seed latency.** Seeding a side-branch/historical parent via reconstruct
  is O(window). *Mitigation:* bounded by checkpoint interval (2048); LRU cache; canonical head
  uses the O(1) flat fast path.
- **R6 (P2) — operational (shadow/cutover/rollback) complexity.** *Mitigation:* the S10 runbook;
  a divergence counter metric; cutover is per-node and reversible.

## 9. Relationship to §12 and to the other audit findings

- **§12 is the substrate, not redundant.** Stage 1 *is* "§12 diffs + a flat latest-state + a
  block→root index." `compute_state_diff` / `apply_state_diff` / `reconstruct_evm_state` /
  checkpoints / content-addressed code are reused unchanged; this design adds the flat store, the
  block→root index, the reorg re-base, and the executor seed switch.
- **H-03 (RPC full-state reads)** is fixed by the flat O(1) point lookups (S7).
- **M-01 (IBD snapshot amplification)** is fixed by shipping the current-PP flat state (S8).
- **I-01 (receiptsRoot)** is already resolved (the typed-receipt-root fork, separate).
- Independent of the PQ-bridge track (QR-C01): different subsystem.

## 10. Open questions (resolve before build)

1. **Flat store keying for the brief multi-head window.** Stage 1 keys the flat store by *address*
   (single canonical head) and re-bases on reorg. Confirm the reorg-depth distribution makes
   re-base cheaper than versioning by root (expected: yes — kaspa reorgs are shallow within the
   EVM acceptance window). If deep reorgs are common, consider a small root-versioned overlay.
2. **Stage-2 trie engine choice** — vendor reth-trie vs a minimal in-house persistent MPT — is a
   separate decision once Stage 1 is proven; both must reproduce the `alloy_trie` root exactly.
3. **Exact prefix assignment** (234/235 proposed) — confirm against the DB registry to avoid
   collision (the registry migration test enforces uniqueness).
