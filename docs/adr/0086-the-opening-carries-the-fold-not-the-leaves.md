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

## 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`)

* **Landed** (`3d56b57a`, `62fec794`): the V4 form and codec; `Base0FpFoldRangeOpeningV1` with
  the whole-block rule (a tail block counts as whole when the range reaches the tree's end);
  both openers serve V4 and the dense opener builds its tree at the ruleset's level; the V4 seat
  verifier over the seat's own leaves with `FaultInRange`; the panel and the seat harness carry
  the new verdict; the challenger's replay hands ADR-0085's builder a V2 view over its own leaves;
  `Base0FpBlockLeavesV1` with `cut_v1`, `folds_to_v1`, `name_the_leaf_v1` and
  `base0_fp_range_with_served_block_v1` (Decision 6's library half); the seat ceiling in the
  form's units; the RC floor's recompute kernels lifted into a backend seam so the floor holds
  its own state like every class. X1–X7 pass; the full base0 suite is 343 green.
* **One deviation from Decision 1's letter.** The verifier accepts any `retain_level` in
  `[PALW_BASE0_SPARSE_RETAIN_LEVEL_V1, PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1]` rather than the
  ruleset's exact level: the digests' level is the producer's retention level, which the form
  describes, and a seat handed a different ladder than the producer (the fixture at
  `COURT_MAX_STEP_LEAVES` against a fold made at the default) must still walk the root, which is
  level-independent. The floor at 12 keeps the digests to one per 4,096 leaves; the ceiling keeps
  the size. Byte-identity between the dense and the fold route (X2) holds when both are built
  under one cap, which is what production does.
* **What the tests taught.** The V4 dispatch was first inserted inside the V3 branch of the
  any-form decoder by a regex that swallowed a doc comment, so every V4 opening fell to the V1
  decoder and came back `Unverifiable` — found by a probe that decoded a served opening directly.
  The floor backend's verifier passed no seat state at all (it had relied on carried chunks), so
  every anchored floor interval was `Unverifiable` under Decision 2 until it learned the seam the
  other families use. A floor free-prompt run can stop at EOG before its budget, so its binding's
  context is not the one `fp_job_context_v1` derives from the job; the memo is keyed by the
  binding's, and the test memoizes under it.
* **Decision 6's transport landed (2026-09-05, with ADR-0085 items 4–5).** A block-leaves request
  rides the interval lane under a packed index — bit 31, the interval in bits 30–16, the block in
  bits 15–0 (`base0_fp_block_leaves_request_index_v1`; a plain interval index never has bit 31
  set) — so the lane's solicitation, slots and byte cap apply unchanged and no message was added.
  The executor's resolver decodes the address and answers `Base0FpBlockLeavesV1` from the same span
  replay the opener runs (`open_fp_block_leaves`; a dense retention hashes its tiles). The seat, on
  `FaultInRange`, asks for the block and — once held — names the leaf against its own replay
  (`fp_name_the_leaf_v1`: the served block must fold to the digest the opening carries, else it is
  refused by name; `None` says every served leaf is the seat's own, and the executor served a block
  it did not commit), records the leaf for the challenger's half (ADR-0085 Decision 4) and files
  nothing, as Decision 3 says. **Not landed:** the seed row's replacement by the seat's own
  selection (§3, not taken); the fold opener's edge-only replay.
* **Devnet run 9 (2026-09-05 10:15–11:02 JST, 300 tokens, the manifest fix on).** The commitment
  landed at 10:52:10 and the drill's `WAIT` stopped the nodes at 11:02 — before the panel binding
  (`anchor_delay` 20 DAA) at that hour's cadence of one block per ~3 minutes. Neither the run 8
  defect (openings never arriving) nor its absence was observed; the seat never drew. The attempt
  lane on the same run licensed every v5 claim by Decision 7's replay. Run 10 waits long enough.
* **Devnet run 10 (2026-09-05 11:25–12:54 JST, 300 tokens, `d7c7ce06b11d6fa3`): the run 8 defect
  reproduced and is understood.** Node-1 drew `[198, 110, 132, 72]` of 299 and asked node-0 every
  ~100 s for 22 minutes; node-0 logged neither "opened" nor "does not open" for the claim; the
  claim ended `Unavailable`. The cause is arithmetic, and run 7 already showed it: a fold
  retention carries no checkpoint chunks (a per-call KV state of this class is ~34 MB, so 300 of
  them cannot ride a 183 MB material — the bulk is the logits rows), so the executor's span
  replay (`base0_replay_span_leaves_v1`) starts at GENESIS for every interval and captures tiles
  as it goes; run 7 served interval 17 in 94 s, 10 in ~60 s, 4 and 0 in ~30 s — about 5 s per
  decode call — and interval 198 costs ~18 minutes, interval 298 ~27, past the seat's 120 s
  solicitation window many times over. The offline probe (`probe_open_interval`, release, the real
  artifact) on run 10's material measured it: interval 0 **21 s**, interval 1 **22 s**, interval
  57 **298 s** (≈5.2 s per call), interval 187 **1,991 s** (≈10.6 s per call — the cache grows with
  the position, so the slope steepens), RSS past 4 GB from the captured tiles; interval 253 was
  stopped after 55 minutes. Two things land here: refusals on the interval lane now log at the default level
  (`[palw-interval] refused …: NoAllowance|Stale|NotBonded|NotHeld|Oversized`; a throttled re-ask
  stays quiet), so the next run reads what node-0 answered. **The fix is the next cut, not this
  commit:** the executor must open an interval from the anchor state it RECOMPUTES without tile
  capture and memoizes — `base0_fp_seat_state_memoized_v1`, the seat's own machinery (Decision 2
  for the executor's side) — and replay only the interval's call(s) with tiles; a plain forward
  pass is ~0.15 s per call (the D7 replay licenses 64 positions in 7–10 s), so interval 198 opens
  in ~30 s. Until then a free-prompt claim past ~20 decode tokens on a fold class cannot be
  licensed by the interval lane, and Studio's 256-token cap is the honest limit for a different
  reason than the manifest.
* **The cut landed (`594015b9`, 2026-09-05 13:30 JST).** `base0_open_fp_interval_sparse_anchored_capped_v1`
  takes an anchor-state closure (`Base0FpAnchorStateForV1`); when the retention carries no
  checkpoint chunks the span replay resumes from the recomputed state of the span's STARTING
  interval's anchor (the span covers whole blocks and may start in the call before), else from the
  prompt as before. The three backends build the closure over their recompute kernels and
  `base0_fp_seat_state_memoized_v1` — a plain forward pass, no capture, memoized under the
  binding's context. The same probe on run 10's material: interval 0 **17 s**, 1 **17 s**, 57
  **19 s** (was 298), 187 **69 s** (was 1,991), 253 **90 s**, 298 **54 s** — every interval inside
  the 120 s window, the interval's own tile replay (~17 s a call) now the floor. Pinned by
  `an_interval_opened_from_the_recomputed_anchor_is_the_interval_opened_from_genesis` on a fold
  fixture wide enough to straddle retained blocks: byte-identical to the genesis walk, the closure
  consulted exactly where the span's starting interval has an anchor, a seat licenses it. The
  interval lane also refuses a re-ask for a `(claim, interval)` whose opener is still in flight:
  a seat re-asks every ~100 s and `served_recently` covered 10 s, so each re-ask had started
  another minute-long opener for the same pair.
* **Devnet run 11 (2026-09-05 13:35–14:48 JST, 300 tokens, `afa75b9e`): Y6 at 300 tokens.** The
  claim `7ed38205…` (prefill 16, decode 300) bound at 14:37; node-1 drew `[34, 105, 134, 209]` of
  299 and node-0 opened them in **17 s, 43 s, 49 s and 65 s** (1.11 MB each), node-1 held all
  four, "4 interval(s) replayed against this seat's own recomputed state — no history fetched",
  and **filed a `Valid` receipt at 14:47:50** — ten minutes after the binding, where runs 8 and 10
  had nothing after thirty. Node-2 (no artifact) filed `Incapable`, as every run.
* **Deployed to testnet-11 (2026-09-05 04:50–05:06Z):** kaspad `56f77a3d75f376de` on seat2, the
  .113 node / seat4 / pool slots 01–04, ibm node1 and node0 (SIGINT, respawn, 21–150 s each); the
  pool's `palw-a16-fp-worker` `69c0e194955cc41c` (the manifest fix) on .113 with `misaka-pool-fp@04`
  restarted. **What testnet then showed is a wall this ADR does not own:** a 300-token free-prompt
  claim on graph-v5 is ~41 M leaves, and exposure is `pwu × slash 5` ≈ 207 M sompi — a bond may
  hold half its collateral, so one such claim needs ≥ 414 M sompi of collateral plus whatever is
  already in flight. The pool's slot-04 bond was sized by the class's CANONICAL job (6.6 M leaves
  → 265 M for four claims): its gateway answered a 300-token request in 229 s and queued no
  commitment (`bond_active false`, room 32.7 M), and the two long jobs it had submitted earlier
  that day (03:06Z, 03:11Z) never reached the chain for the same reason. The pool now accepts a
  caller-sized bond (`POST /pool/v1/slots {"mode":"fp","bond_collateral":…}`) and slot-05 was
  created at 600 M sompi and funded 8.5 MSK from the faucet wallet; the 300-token proof on testnet
  rides that slot.
* **Devnet Y, live (run 7, 2026-09-05 07:17–08:14 JST, `62fec794`, three nodes on this Mac).** A
  graph-v5 free-prompt claim of 40 tokens (`d9ad65d0…`) staged a **24,423,539-byte** fold
  capture — over `PALW_MATERIAL_MAX_BYTES`, the drill's own line: "ADR-0084 Y1 is live". The
  panel bound on every node at 08:08; node-1 (holds the artifact) drew intervals `[17, 4, 0, 10]`
  of 39, asked node-0, and held four served V4 openings: interval 17 **1,110,705 bytes**,
  interval 4 **1,110,513**, interval 0 **33,655**, interval 10 **1,110,449** — the seed row
  (the vocabulary-wide logits row) is the decode intervals' whole size, as §4 said, and interval
  0 is the prefill's digests and the binding. Then "4 interval(s) replayed against this seat's
  own recomputed state — no history fetched" and **a `Valid` receipt** at 08:13:46, five minutes
  after the draw. Node-2 (no artifact) filed `Incapable`. On the attempt lane the same build
  served interval 0 of a v5 claim as **108,791 bytes** where run 5 had served 424,359,978, and
  Decision 7's replay kept licensing (the line now reads "6630544 leaves replayed, priced 0, 7s").
  Nothing over the cap moved on either lane.

* **Found alongside, not this ADR's:** the free-prompt lane's retained-trace manifest was the
  attempt lane's (`attempt_trace_manifest_root_v1(trace_root, 1)`, chunk count 1), so every
  free-prompt run past 256 tokens was refused by the verifier's chunk-count rule and every shorter
  one carried a root of the wrong lane's function; `0001f34c` gives the lane its own derivation
  (card §10p). It bears on this ADR only in that a 300-token claim is now the devnet's check for
  both (run 8).

## 8. What is deliberately not decided

* The seed row's replacement by the seat's own selection (above).
* Edge-only replay in the fold opener.
* Raising `PALW_INTERVAL_OPENING_MAX_BYTES`, which nothing here needs.
* The court's refutation walkers' ladder bound (ADR-0084 U-08) — landed behind
  `Params::palw_court_ladder` since (ADR-0084 §7.5).

## 9. Number hygiene

This is ADR-0086. The README's next free number was 0086 after 0085's row; it becomes 0087
with this row.
