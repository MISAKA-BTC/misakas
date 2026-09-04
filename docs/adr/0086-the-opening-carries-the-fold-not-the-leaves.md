# ADR-0086 — the opening carries the fold, not the leaves

**Status:** PROPOSED 2026-09-05, implemented consensus-inert on `palw-adr0084-served-answer`
(no fence; the fingerprint does not move). Supersedes the wire form of ADR-0077 Decision 8's
interval opening (V1–V3) for SERVING; the consensus root rule those openings walk
(`step_range_opening_root_capped_v1`) is untouched. Requested by the operator on 2026-09-05 as
ADR-0084 §7.2's first option.

## 1. What was measured (devnet runs 3–6 on `palw-adr0084-served-answer`, 2026-09-04/05)

* A graph-v5 attempt claim's interval 0 opened as **424,359,978 bytes** (node-0, 14 s to
  assemble) and the transport dropped it without a line; the asking seat stayed at "0 openings
  held" until its receipt deadline. The class has 103,008 leaves per prefill position (122,024 at
  the position that selects a token) and 122,024 per decode call; `PalwStepRangeOpeningV1`
  carries 64 bytes per leaf of its range, so interval 0 (the prefill and the first decode call)
  is 424 MB of leaf hashes and a decode interval 7.8 MB, against
  `PALW_INTERVAL_OPENING_MAX_BYTES = 4 MiB`.
* The same class's anchor is CHUNKED: its map is `integer_kv_state_chunk_map_id_v2`, its cadence
  per decode call, and a V1 opening's `anchor.chunks` is the whole KV state at the covered call —
  57 KB per position at 32-bit elements, ≈3.6 MB at the 63-token prefill and ≈33 MB at n_ctx 512.
  A decode interval's V1 opening on this class is therefore ≈12 MB at 16 tokens and ≈41 MB at 512.
* **The seat already computes both of those things itself.** Before it verifies any opening, the
  panel recomputes the checkpoint's state root from the claim's own ids
  (`checkpoint_root_for_context_v1`, memoized as `Base0FpSeatStateV1`, chunks included) and
  refuses the opening when the roots differ; then the verifier replays every leaf of the interval
  (`kernels.replay_interval`) and compares them one by one against the carried hashes. The 424 MB
  carries what the seat is about to compute anyway.
* What the seat cannot compute: the producer's Merkle frontier around the range (`siblings`,
  at most two per level — ≤ 46 hashes on this class), the producer's committed logits row at the
  anchor call (the seed row tiles, which decode to the token the interval starts from — 18,992
  tiles, ≈1 MB on this class, bounded by the vocabulary and not by the job), the checkpoint claim
  (a leaf and its path), and the binding.
* The seat's ceiling `palw_fp_interval_opening_ceiling_v1` prices an opening as
  `positions × row_bytes`; the transport cap's own comment says a family whose openings exceed it
  raises the cap before registration. Neither happened for graph-v5, and no cap reaches 424 MB.

## 2. The requirement

An interval opening's size is bounded by the tree's depth, the fold's block count over the
range, the class's seed row and the checkpoint claim — never by the range's leaf count, never by
the job's position count, and never by the state's size. A seat's verdict keeps its meaning:
Valid exactly when the producer's committed range is the honest recompute. A fault keeps an
address the court can act on. Nothing in consensus changes.

## 3. Decisions

**Decision 1 — the V4 opening carries the fold's digests and the frontier, and no leaf hashes.**
`Base0FpFoldRangeOpeningV1 { first_leaf_index, leaf_count, retain_level, block_roots, siblings }`
replaces `PalwStepRangeOpeningV1` in the served form: `siblings` is exactly the consensus range
opening's sibling sequence (left-then-right per level, bottom-up); `block_roots` are the fold's
retained nodes (ADR-0082 Decision 7's level, `palw_base0_sparse_retain_level_v1(cap)`) for the
blocks lying wholly inside the range, in order; `retain_level` names that level so the form
describes itself. `Base0FpIntervalOpeningV4 { version: 4, interval_index, binding, range,
seed_row_leaf_count, seed_row_tiles, anchor: Option<Base0FpCheckpointClaimV1>, close }`, magic
`MSKFPIV4`. On graph-v5 a decode interval is ≈2 KB of digests, ≤ 3 KB of siblings, the seed row
and the claim; interval 0 is ≈104 KB of digests and no seed row.

**Decision 2 — every V4 anchor is the named claim; chunks never ride.** ADR-0082 Decision 9's
rule for the flat classes becomes the rule for every class: the seat starts its replay from the
state it recomputed for the checkpoint check, which it holds with its chunks
(`Base0FpSeatStateV1.chunks`). `HistoryNotAdmissible` stays what it is for V1 openings; V4 has
nothing to be inadmissible.

**Decision 3 — the seat's leaves are its own.** Verification hashes the served seed row tiles
into the range's first leaves, replays the interval for the rest, assembles a
`PalwStepRangeOpeningV1` from those leaves and the served siblings, and walks the consensus root
rule unchanged. Before the walk it folds its own leaves at `retain_level` over the blocks wholly
inside the range and compares them with `block_roots`; a block that differs is the fault's
address — `Base0FpIntervalSeatVerdictV1::FaultInRange { first_leaf_index, leaf_count }`, the
block clipped to the range — and a root that differs with every digest equal names the range's
edges the same way. The panel treats `FaultInRange` as it treats `Fault`: it files nothing, logs
the address, and the court's bisection is what convicts (ADR-0085 Decision 4 unchanged).

**Decision 4 — both openers serve V4, and the dense tree is built at the ruleset's level.**
The dense opener rebuilt its tree at the constant level 12 while the fold retention keeps
`palw_base0_sparse_retain_level_v1(cap)`; a digest list at two levels would be two wire forms.
The dense opener now builds at the ruleset's level (the root is level-independent — ADR-0084
§7.1 proved the two spellings one rule), reads `block_roots` off `retained_nodes()`, takes
`siblings` from `range_opening_v1` and drops `leaf_hashes`. The fold opener's span replay is
unchanged (it replays the covering blocks' calls to derive the edge siblings); replaying only the
edge calls is an optimisation this ADR does not take.

**Decision 5 — V4 is served for every class from this build; V1–V3 stay decodable.** An old
seat decodes V4 as `Unverifiable` and files nothing, which is what it files today for a class
whose V1 opening never arrived; a class whose material fits the transport cap is licensed by the
whole-capture pull regardless. No request-side version is added — the executor cannot know the
asker's build, and the two outcomes it could produce are the same as the two it produces now.

**Decision 6 — the court's address is a block, then a leaf.** A `FaultInRange` becomes a court
case through the annex lane ADR-0085 Decision 1 defined: a block-leaves request (claim, interval,
block first index) is answered with that block's ≤ 4,096 leaf hashes (≤ 256 KB, inside the
opening cap), the challenger names the leaf from its own replay, and the disputed tile and its
path ride ADR-0085's annex as before. This ADR lands the request and response forms and the
assembly from own leaves plus a served block; the P2P wiring is listed with ADR-0085's items 4
and 5.

**Decision 7 — the seat's ceiling is restated in the opening's own units.**
`palw_fp_interval_opening_ceiling_v1` prices `blocks × 64 + 2 · depth · 64 + seed row + claim +
header`; the transport cap stays 4 MiB. A ceiling in `positions × row_bytes` was shaped to agree
with a form that no longer exists.

**Not taken: the seed from the seat's own recompute.** The prefix recompute feeds the claim's
ids to `forward_no_capture` and never computes a logits row, so the boundary check "the seed is
the anchor call's argmax" needs the producer's row today. Having the recompute select at its last
call would drop the seed row (≈1 MB on graph-v5) for one logits projection per interval; it is
the next cut, not this one.

## 4. What this costs, stated before it is measured

* Opener: no per-leaf vector is serialised; the dense opener still rebuilds 6.6 M leaf hashes
  from tiles to build its tree for interval 0 (≈5 s in a release build), the fold opener still
  replays the covering calls. Memoising the dense tree is not taken here.
* Seat: the replay it already does, plus one fold over its own leaves (hashing ≈ leaves/2 nodes,
  a few hundred milliseconds on interval 0) and the same root walk over its own leaf hashes.
* Transport: graph-v5 decode interval ≈1.1 MB (the seed row dominates), interval 0 ≈110 KB;
  both inside the 4 MiB cap with the class at n_ctx 512 or 2048 — the digest count grows with the
  range, 64 bytes per 4,096 leaves.
* Court: a block-leaves answer is ≤ 256 KB; the on-chain refutation is unchanged (tile + path).

## 5. Invariants the tests must hold

* **X1 (frozen form).** `MSKFPIV4`, version 4; bytes of another family and of V1–V3 are refused
  by the V4 decoder; the any-form decoder returns `Digests` for V4 and the old variants for V1–V3.
* **X2 (one form from two retentions).** On the a16 fixture, the V4 opening from the dense
  tuple equals the V4 opening from the fold byte for byte, for every interval.
* **X3 (the seat's own leaves walk to the root).** Every interval of a floor capture verifies
  `Valid` through a V4 opening with the seat's replayed leaves and the served siblings; the
  served bytes carry no leaf hash.
* **X4 (size is the fold's, not the range's).** Over a synthetic space of 2^22+1 leaves an
  opening of the last 1,000,000 leaves is under 200 KB, and its size does not change when the
  range's leaves are re-randomised.
* **X5 (a fault has an address).** A leaf the producer did not compute yields
  `FaultInRange` naming the block that holds it, on the dense and on the fold route; a tampered
  sibling yields `FaultInRange` naming an edge.
* **X6 (the address becomes a leaf).** From a served block-leaves answer and the seat's own
  replay the challenger names the leaf, and the refutation assembled from it verifies
  (ADR-0085 X1's builder over V4 inputs).
* **X7 (the old seat is safe).** A V4 opening handed to the V1–V3 verifier is `Unverifiable`,
  never `Mismatch` and never `Fault`.
* **Y (the lane, live).** On the loopback devnet a graph-v5 free-prompt claim whose material is
  over `PALW_MATERIAL_MAX_BYTES` reaches `Valid` receipts from seats that fetched V4 openings
  and no material, and is licensed.

## 6. Order of work

1. Wire form, codec, any-form decoder, X1.
2. Assembler and both openers, the dense tree at the ruleset's level, X2, X4.
3. Seat verifier over V4 (own leaves, digests, `FaultInRange`), the panel's handling, X3, X5, X7.
4. Block-leaves annex forms and the challenger's assembly, X6.
5. Ceiling restated; docs; devnet Y; fleet.

## 7. Implementation record

(filled as the work lands)

## 8. What is deliberately not decided

* The seed row's replacement by the seat's own selection (above).
* Edge-only replay in the fold opener.
* Raising `PALW_INTERVAL_OPENING_MAX_BYTES`, which nothing here needs.
* The court's refutation walkers' ladder bound (ADR-0084 U-08), a validity rule.

## 9. Number hygiene

This is ADR-0086. The README's next free number was 0086 after 0085's row; it becomes 0087
with this row.
