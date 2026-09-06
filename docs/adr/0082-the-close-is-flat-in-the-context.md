# ADR-0082: The close is flat in the context — attention is refuted by dissection, the capture is a fold, and the answer is what earns

**Status:** IMPLEMENTED (2026-09-04) on `palw-adr0082-impl`, pending the testnet-11 Relaunch 5f
cut. **§10 is the implementation record**, and it is where every figure below that the measurement
MOVED is corrected in place — struck, restated, with the configuration and the stream that moved
it. ~~PROPOSED (2026-09-03).~~ Written against two things: the refutation of ADR-0080 and
ADR-0081 (their own status blocks, verified line by line against the tree), and the state of
`palw-testnet-5f` at `0c299e7a` read beside `palw-adr0080-verification-segment` at `ee1a7582`.
The goal it serves is the one ADR-0077 R0 states and ADR-0080/0081 tried to reach: a person types a
prompt of thousands of tokens, receives an answer of thousands of tokens at the runtime's own speed,
and the chain holds ONE claim whose verification costs a bounded number of bytes wherever bytes are
spent and a bounded amount of compute wherever compute cannot be avoided. That is the shape Ambient
reached with a tolerant proof model (ADR-0026's audit); this ADR reaches it with the exact one.
Nothing here is armed by this document. Every consensus change below is a ruleset move and lands
with a re-genesis on testnet-11, as every relaunch has; mainnet ships PALW off and is untouched.
**Builds on:** ADR-0026 (borrow Ambient's architecture, refuse its proof model), ADR-0028 (sampling
decides nothing; only the court convicts), ADR-0030 §3 (the checkpoint leg and its state chunk map),
ADR-0040 Decision E (exact integer accumulation: the order of accumulation cannot change the
result — the associativity this ADR spends), ADR-0044 F4–F6 (beacon, panel, ticket), ADR-0049
Decisions C/E/F (admission bounds the court from the graph; the tiled logits pin; one description
for engine, profile, adjudicator and inventory), ADR-0072 (the ticket is the execution), ADR-0074
Decision 5 (a quantum is a fraction of the canonical job's leaves), ADR-0075 (a class is seated by
`ClassLaneCertified`), ADR-0077 (R0, R1, Decision 8's sampled interval, Decisions 10–14, SA-4's
derived deadline), ADR-0078 (what was made from the answer is committed, never carried), ADR-0080 §1
and Decision 3 (the measurement and the economic invariant — the two things that survived), ADR-0081
§1.2 and Decision 3 (the prompt ids as a Merkle root — the one decision that survived), and ADR-0080
"design A" as landed on `palw-testnet-5f` (the chunk-group close ceiling).
**Amends:** ADR-0080 §3 and ADR-0081 §3 — withdrawn and replaced (§7). ADR-0077 Decision 8 (the seat
no longer fetches the history), Decision 11 (attention is no longer priced by an interval), Decision
12 (the ladder's depth stops binding), Decision 13 (the rows are sized by three derived bounds, not by
a rotary table). ADR-0080 design A (the chunk count is re-derived from the flat terms, and its
unlanded half on 5f is named as a prerequisite).

## 1. What was measured

Line numbers below are those of the two branches the status block names: `palw-testnet-5f` for
the transport, worker, kernel, profile and economics facts (§1.2, §1.4–1.6), and
`palw-adr0080-verification-segment` for the refutation, U-00 and U-04 facts (§1.1–1.2). Every
citation names the item as well as the line, so a moved line is still findable. **And every
figure in this ADR names the CONFIGURATION it was measured under** — which court (the shipped
binary ladder, or the dissection court with its arity), which prompt-id form, which evidence
route, which host load — because the same defect bit three subsystems on the day this was
written: a number measured under one configuration and quoted as if it held under another, never
caught by the person who wrote it down. A figure without its configuration is not a measurement
here; it is a hope with a decimal point.

### 1.1 The refutation of ADR-0080/0081 stands, and it was correct

Read against the tree, every finding in the two status blocks holds. `PalwDerivedArtifactV1` names
a singular `claim_id` and a singular `output_root` and the transition enforces the equality
(`palw_derived_v1.rs`); `output_commitment_v2` is a flat keyed hash under a per-job
`job_context_hash`, so no concatenation relation between two claims is checkable; `fp_work_id_v1`
keys on `(class_id, prompt_token_ids_hash, decode_tokens_executed, executor_bond)` with no state and
no parent, so a second segment is `DuplicateWork` (`palw_freeprompt_v3.rs:286`);
`checkpoint_genesis_prev_v2` derives the chain's genesis link from `job_context_hash` alone
(`palw_step_leg.rs:627`) and `verify_kv_anchor` is intra-claim in every check. The economic census
(`palw_economic_locus_v1.rs`) measured the rest: a `PalwSeatReceiptV2` is 4,772 bytes on the wire,
a `ReceiptLicensed` carrying three is 14,385, a 37-segment answer is 513,597 bytes of signatures
inside 532,245 bytes of consensus objects as a FLOOR, and the quanta band is reachable at the shipped
cap — 51,200 leaves weigh 12,800 pwu as one claim and 51,200 as four. **Segmentation of one answer
into N claims is not a design this chain can verify or afford, and nothing below re-opens it.**

What survives is what this ADR is built on: ADR-0080 §1's measurement (the 512 residue is the
context's WIDTH, not the history's length), ADR-0080 Decision 3's invariant (a job's reward,
ticket, weight and quanta are functions of its leaves and of nothing a producer can restructure),
and ADR-0081 Decision 3's Merkle prompt ids (`palw_prompt_ids_v1.rs`, behind the dormant
`palw_prompt_ids_merkle` fence: 2,048 → 472 bytes at `n_ctx` 512, 131,072 → 856 at 32,768, one
64-byte element more per doubling).

### 1.2 The graph-v4 tile flattens the opening and not the close, and the residue has a name

The U-00 stream measured the v3 map against the anchored court (`palw_context_ladder.rs`, tests
`the_v3_tile_flattens_the_opening_and_not_the_close`,
`with_the_interval_held_the_residue_is_the_probability_row`,
`the_tile_moves_the_widest_dense_row_from_thirty_to_two_hundred_and_twenty_three`,
`the_v3_map_is_not_priced_by_the_ladder_rule`):

| what | measured |
|---|---|
| the v3 checkpoint OPENING, dense, at `n_ctx` 1,000 / 4,096 / 32,768 | 18,432 / 18,432 / 18,432 — flat |
| the v2 opening at 1,000 / 4,096 | 1,026,048 / 4,196,352 — the whole history |
| the anchored v3 CLOSE at 1,000 / 2,000 / 4,096 | 228,769 / 414,033 / 806,577 — one straight line, slopes 185.3 then 187.3 bytes a position |
| the same close with the interval held at the tile (256 / 512 / 1,024 / 4,096) | 123,889 / 136,817 / 162,609 / 317,105 — slope `attn_heads × 4` ≈ 48–51 bytes a position |
| what the shipped ladder rule CHARGES a v3 class for the opening | 526,336 — the v2 price, 28.6× the honest 18,432; the rule has no `_for_map_v1` twin on the cache half |
| widest dense row inside 81,920: unfenced / anchored v2 / honest v3 | 21 / 30 / 223 |

The slope's name is the graph's. On both families the attention site is four committed rows per
position: `ATTN_SCORES` (`MatMulQuant`, out `KvPerHead`), `SoftMax` (out `KvPerHead`), the
probability requantization (`MulElem`, out `KvPerHead`) and `ATTN_VALUES` (`MatMulQuant`, reading
`Step(9)` and `CachedV`) — `palw_base0_profile.rs:310-313`, `palw_qwen36_profile.rs:467-470`.
`KvPerHead` resolves to `KvScaled { multiplier: attn_heads }`, and the cost walk resolves a
`KvScaled` width at the CONTEXT maximum (`palw_class_admission_v2.rs:134-140`) because a leaf of
`ATTN_VALUES` opens the whole probability row of its head. That row is `attn_heads × n_ctx` lanes
wide at every position. **Three context-wide rows are committed per attention site per position,
and one of them is opened whole by the node after it.** No state chunk map reaches a committed row;
a map addresses the CACHE, and the cache is not what is carried.

### 1.3 The same rows are why the leaf count is quadratic

A committed row of `attn_heads × kv_len` lanes is `attn_heads × kv_len / tile_len` leaves. Per
position that is linear in the position, so a job of `n` positions commits `O(n²)` attention
leaves on top of the `~99 k` (dense) / `~298 k` (hybrid) leaves a position costs today (ADR-0077
§1). At 131,072 positions and a 128-lane tile the dense tier's three rows alone are
`3 × 28 × 12 × 131,072 / 128 ≈ 1.0 M` leaves at the last position — ten times the base — and the
job, summed over its positions, is `~8 × 10¹⁰` leaves: sixteen times the `2^32` ladder, and past
any hashing budget. The `2^32` ladder ADR-0077
Decision 12 sized — "~14,000 positions of the hybrid and ~43,000 of the dense tier" — was sized
against the BASE count, which is the count a job has only if no context-wide row is committed.

### 1.4 What 5f landed, and what it did not

ADR-0080 "design A" on `palw-testnet-5f` prices a close in chunk-group parts:
`DEFAULT_MAX_CLOSE_CHUNKS = 27`, `DEFAULT_MAX_CLOSE_BYTES = 27 × 100,000 × 10 / 12 = 2,250,000`
(`palw_mode_v2.rs:259-269`). Under it the admission gate admits dense at 39 positions on the
shipped `2^22` ladder and **1,002** on a `2^32` one; the hybrid at 12 and **514**
(`palw_class_admission_v2::tests::the_widest_context_each_family_admits`). And the transport does
not carry it:

* `apply_object`'s `ObjectChunk` arm decodes the assembled group and admits ONLY
  `FamilyCertified` (`palw_state_v2.rs:7417`, `ChunkedObjectKindNotAllowed`); a `CourtClosed` split
  across carriers is refused in the block that completes it.
* `PALW_OBJECT_CHUNK_MAX_COUNT = 8` is enforced by the cutter (`:2917`) and the transition
  (`:7362`); the ceiling's own doc says so (`palw_mode_v2.rs` "What is NOT yet derived from this,
  and is a wall rather than a rounding").
* `misaka-cli palw court-close` (W13) refuses a split close by name and lists the three consensus
  changes it waits on (`misaka-cli/src/palw_court.rs:84-125`); one of the three — `max_close_chunks`
  — landed with W3, the other two did not.
* The free-prompt worker builds its court from `PALW_STEP_MAX_LEAVES` (`2^22`) directly
  (`misaka-palw-base0/src/bin/palw-a16-fp-worker.rs:94`); the W1b fix that prices a job against the
  ruleset's ladder (`bb4f145b`) is not an ancestor of 5f. On the shipped commit path that is about
  38 positions, prompt and answer together, whatever the row admits.

So on 5f at `01163cd3` a 512 row is admitted, unprosecutable past one carrier, and unexecutable
past 38 positions. This is the defect class the tree keeps recording — a gate widened where the rule
did not move — and it is a prerequisite here, not a decision. **Who lands it (settled 2026-09-03
with the 5f integrator):** the transport is ADR-0080 W5 (`palw-adr0080-w5-closegroups`) — a court
close rides its OWN chunk-group table `court_close_groups` keyed `(session_id, side)`, deliberately
NOT ADR-0075 D14's `pending_chunks`, whose `PALW_OBJECT_CHUNK_MAX_COUNT = 8` bounds the certification
lane and prices its slot rent; the court's count is the ruleset's `max_close_chunks`; state v17 → 18
is W5's. **The implementation added two more version bumps beside it: v18 → 19 for the court
session's dissection phase (stream I) and v19 → 20 for the class state's `fused_attention` (fixer
FA2, §10.6); each moved the ADR-0043 state goldens, which are re-pinned at the cut.** W5 carries
four requirements its adversarial judges added, recorded here so they are the
ADR's and not a private brief: the declaration's clock is the session BACKSTOP, refused unless
`daa + 4 × count ≤ deadline` and never extended; a failed declaration loses on its OWN side (a lapse
at Terminal must not route to the accuser's slash); the chunk bytes live in ROOTED state, never a
node-local store; and `PALW_COURT_CLOSE_MAX_PER_BLOCK = 1`, counted in the acceptance walk in
transaction order, so "close bytes in this block" and "court CPU in this block" stay one number. The
executor-side half — the worker pricing a job against the ruleset's ladder (W1b, `bb4f145b`) — is
merged on this ADR's integration branch.

### 1.5 The practical wall is the capture, and no ADR has priced it

Measured on the dense tier's shipped commit path (the model-gate run of 2026-09-03): **5.66 s per
token captured against 0.060 s per token un-captured — 94×.** The capture holds every tile of every
node of every position, ~50 MB a position, ~25 GB at 512 (`fp_capture.rs` header — the header's own
figures; **U-01 then measured the real dense artifact at 13.46 MB a position retained by the capture
against 0.74 MB by the fold**, Decision 7, and fixer FB measured what the amended Decision 4's
per-position cadence SERIALISES, §10.3). The fold that
replaces it — `Base0SparseStepAccumulatorV1`, one retained node per `2^12` leaves, 64 KiB at `2^22`
and 64 MiB at `2^32`, tiles re-derived by replay — is written, tested against
`step_merkle_root_v1` for every leaf count, and **called by nothing outside its own module** (one
doc reference in `palw_step_leg.rs:238`). A person waiting 5.66 s a token is not using a practical
runtime, and no context width changes that.

### 1.6 What earns, what is chosen, what the lane carries

* **Leaves are credited whole, prefill included.** A commitment's `work_leaves` is the capture's
  `step_leaf_count` (`palw_freeprompt_v3.rs:1225`), the transition prices it as carried
  (`palw_state_v2.rs`, the free-prompt arm: `fp_quanta_v3(*work_leaves, quantum, cap)`), and the
  seat's replay is what binds it to the enumeration (`palw_backend.rs:379`). A pinned integer model
  is deterministic and causal: every leaf of a prefix is a pure function of the prefix, so the same
  bond re-submitting a 32,000-token prefix with one new token recomputes nothing and is credited
  everything. Weight is not yet on this lane (ADR-0073 Phase ④ is pending), which is exactly when
  the rule has to exist.
* **Decoding is `base0_decode_token_select_v1`: the argmax of the integer logits row, ties to the
  lowest index** (`palw_step_refute.rs:2825`). There is no sampler. The tree's own measurement of
  what a small model does at temperature zero is on record: "collapses to a handful of attractors
  (measured min-entropy ~3.1 bits over 60 seeds …)" (`params.rs:4307`). A thousand-token greedy
  answer is a repetition, not an artifact.
* **The lane's carriage is per claim:** five seats, three receipts of 4,627 bytes each, four
  interval replays per seat, one payout row drained at `PALW_V2_MAX_PAYOUTS_PER_BLOCK = 8` under a
  premise stated at the constant — "at most one new claim per block" (`palw_state_v2.rs:231`). At
  the frozen 120 s cadence that is 5,760 finalized claims a day before the queue stops draining, a
  ceiling that no context width moves.

### 1.7 Ambient at large context, from ADR-0026's audit

ADR-0026 measured Ambient's binary rather than its paper: the verifier is `/ambient/v1/inference/verify`
inside their llama.cpp fork; it takes a token window (`start_token` / `end_token`), teacher-forces
the sequence (`ambient_forced_tokens`), recomputes the window from the context it holds, and checks
the Merkle commitment; the comparison is tolerant (`logit_min_rbo_score`, `mlp_output_p95_abs_diff`).
Two things in that shape are right at any context and are taken here: **the verifier verifies a
window, and the verifier holds the context and recomputes — nothing context-sized is shipped to
it.** One thing is absent from it because Ambient never needs it: an exact DISPUTE. A tolerant
verdict is consumed as a probabilistic slash and is never bisected, so Ambient has no close to keep
small. PALW convicts by bisection to one leaf (ADR-0028), and the leaf's neighbourhood at an
attention site is the whole history — which is why "verify a window" alone does not make PALW
flat, and why §3 adds the one mechanism Ambient does not have.

## 2. The requirement

> **R4 — nothing a court carries, a seat fetches, or an executor commits grows with the context
> except through a logarithm; the context a class admits is bounded by compute and by the windows,
> never by bytes.** ADR-0080's R2 stands inside it: the length of the answer, the width of one
> verified unit and the input a class admits are three numbers — and the second of them is now a
> TILE, at every context, for every node of the graph.

```text
   a 100,000-token prompt ─┐                                the answer, streamed
   the answer, 4,000 tokens ┘   ONE inference, ONE claim    (kept by the user, ADR-0078)
                                       │
          ┌────────────────────────────┼────────────────────────────┐
          ▼                            ▼                            ▼
   the executor                   a seat (×5)                  the court
   commits O(1) rows per         holds the prompt,            bisects the leaf space k-ary,
   attention site — no row       RECOMPUTES the cache,        then DISSECTS the history k-ary:
   is context-wide; folds        replays k intervals,         each round k triples, each
   leaf hashes, keeps the        compares EXACTLY;            ~8 KiB; the bottom opens
   tree above level L;           fetches O(k·interval·row     ONE tile of K, ONE of V,
   retains the KV cache          + log leaves) bytes,         the query row and the output
   for the claim's life          never the history            tile — ~25–42 KB, at any n
```

## 3. Decisions

### Part A — the court is flat in the context

**Decision 1 — attention is ONE fused node per site, and its committed row is the output.** The
four nodes of §1.2 become one node kind, `AttnFused`, whose inputs are the rotated query row, the
K cache and the V cache, and whose committed row is `out` (`heads × d_head` codes, the row
`ATTN_VALUES` commits today) and nothing else. The scores, the row max, the exponent sum, the
probabilities and the requantized probabilities are INTERNAL to the op: computed by the executor in
any order it likes, never committed, never carried. The op's semantics is the composition of the
four shipped kernels — `a16_attn_scores` (W9), `a16_softmax_rows` (W11, `softmax_shifted` per
head), the `attn_probs` requantization and `a16_attn_values` (W10), `palw_base0_a16.rs:329-430` —
so its `kernel_semantics_id` is derived from theirs and an adjudicator that holds the four holds the
fifth. A class IS its graph (ADR-0049 Decision F), so this is graph v5: a NEW class id per family,
registered through ADR-0075's route or minted at the relaunch; the shipped v2/v3 rows are untouched
and stay exactly as narrow as they are. Consequence for the leaf count: an attention site costs
`⌈heads × d_head / tile_len⌉` leaves a position at every context — 24 on the dense tier at a 64-lane
tile, 32 on the hybrid at 128 — and the job's leaf count returns to the BASE count ADR-0077
Decision 12 was sized against: `~43,000` dense and `~14,000` hybrid positions under `2^32`
(~~measured by U-04 at the fence's ladder: dense 41,695, hybrid 13,105~~ — **corrected 2026-09-04,
fixer FD, audit D H-5: swept through the gate's own door instead of derived beside it
(`the_widest_context_each_family_admits`, `2^32` context-ladder fence, dissection court at the
derived arity, Merkle prompt ids, the joint clock), the graph-v5 families admit `n_ctx` 16,384
dense and 13,105 hybrid, and the refusal one position wider is the LADDER or the WINDOW and never
the close — which is the property Decisions 1–4 exist to buy, asserted rather than hoped**).
**A width clears the close
AND the ladder, and the two are different caps**: the RC bundle the 5f genesis freezes carries a
`2^26` ladder, not the fence's `2^32`, and at `2^26` the widest admissible dense **graph-v2** row is
**574** positions (`palw_class_admission_v2.rs`'s own ladder table, the
`palw_a16_context_row_profile_v1` sweep). The first draft quoted that number for the v5 row it was
not measured on; it stays here as the v2 figure it always was, and the v5 sweep is the corrected
line above. 512 fits with about twelve per cent of margin,
and 1,024 is not the next rung on the ladder side but a different cap (`2^28`). So on the close
side 1,024 costs 64 bytes more than 512; on the ladder side it needs a re-genesis. Every figure in
this ADR that quotes a 32-element Merkle path is the fence's number; the ruleset's is read from the
bundle, never typed.

**Decision 2 — a fused attention leaf is refuted by DISSECTION over the history, and nothing
context-wide is ever carried.** This is the mechanism Ambient does not have (§1.7) and the one the
tree itself named a year of ADRs ago: "carrying a model at that scale needs bisection WITHIN a
step's reduction" (`palw_mode_v2.rs:208`). The terminal adjudication of an `AttnFused` leaf is not
a one-shot recompute but a short interactive protocol over the history positions `0..p` of the
disputed head:

* The responder opens the committed `out` tile and states a ROOT CLAIM for the head:
  `(m*, S*)` — the row max and the exponent sum — and the partial value sums `V*[lane]` for the
  disputed tile's lanes. The court checks that the shipped finalization
  (`clamp16(a16_scale_round(V*[lane], m, shift) + zero)`) reproduces the opened `out` tile; a
  root claim that does not is refused before any round is played.
* Each round the responder discloses, for each of the `k` children of the disputed range, the
  triple `(max_c, S_c, V_c[lanes])`, and the court checks the FOLD before the challenger moves:
  `max = max(max_c)`, `S* = Σ S_c`, `V* = Σ V_c` over the children, all exact integer operations
  (ADR-0040 Decision E). A disclosure that does not fold to the parent's claim is a conviction. The
  challenger names the child it disagrees with.
* At the bottom — one history TILE of `PALW_ATTN_HISTORY_TILE_V4 = 16` positions — the court opens
  the head's query slice, the tile's K rows and V rows (the graph-v4 map, `tiled_kv_state_chunk_map_id_v3`,
  from the checkpoint at or before the tile, or the cache-write leaves after it) and RECOMPUTES the
  tile's triple with the shipped kernels: `s_j` by W9's dot and requantization; `max_j s_j`;
  `e_j = int_exp(((s_j − m*) << up).clamp(i32::MIN, 0))` and `Σ e_j`; `p_j = requant((e_j × int_recip(S*)) >> K)`
  and `Σ p_j × v_j[lane]`. Every one of those is per element given `(m*, S*)`
  (`palw_base0_ops.rs:306-338`: one max, one exp table, one reciprocal, then a per-element
  multiply and shift), which is what makes the tile's triple recomputable without the row.

If `m*` was lied about, the max fold finds the tile whose true max exceeds the claim; if `m*` is
honest and `S*` is not, the sum fold finds it; if both are honest and `out` is not, the value fold
finds it — with the SAME `(m*, S*)` used at every tile, so there is no rescaling and no rounding
between children. The exactness this rests on is the kernel's own premise:
`|p × v| ≤ 32,767² < 2^30`, so `2^32` terms stay inside `i64`'s `2^62`; the softmax sum is at most
`n × ONE = n × 2^24`, inside `i64` to `n < 2^39`. `A16_MAX_DOT_LEN = 2^18` bounds `kv_len` in
`a16_attn_values` today (`palw_base0_a16.rs:61, :396`) — "one power of two above the largest real
reduction in the family" — and moves with a wider row's class id, inside the `2^32` the premise
allows.

**Decision 3 — the court's dissection is k-ary, with k derived from the move budget.** The shipped
ladder is binary at a pinned midpoint (`palw_bisect.rs` header), and the clock runs per move:
`moves = 2 × ⌈log₂ leaves⌉ + terminal` (`PalwCourtParamsV2::worst_case_duration_daa`). At the
`2^32` ladder and the derived RC deadline of 45 DAA that is `(2 × 32 + 2) × 45 = 2,970` of the
`3,000`-DAA `window_court` (`PALW_RC_WINDOWS_V1`): the binary ladder already spends the whole
window, and Decision 2 adds rounds. A k-ary round discloses the `k` subtree roots below the current
node — `log₂ k` binary levels at once, authenticated by hashing them back up — so
`rounds = ⌈log₂ space / log₂ k⌉`, at `k × 64` bytes a move for the leaf space and
`k × (4 + 8 + 8 × tile_len)` for the history (a max, a sum and `tile_len` partial sums per child).
Worked at `k = 16`, the band the ADR was drafted against; the derivation below selects the
SMALLEST legal arity that fits, ~~which at the RC windows is **4** (measured by U-03's pin:
`2^32` in 16 rounds plus 131,072 positions in 7, `(2 × 23 + 2) × 45 = 2,160` DAA)~~ — **corrected
2026-09-04 (audit D H-2, audit A M-2, fixers FA and FD): only the HISTORY search is k-ary. A
session plays the shipped `PalwBisectLadderV1` over the leaf space, which is BINARY, and the
responder's root claim is a move nobody counted. With both corrected the honest count is
`2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1`, and what the RC's own configuration selects is
arity 2, not 4 or 16 — §10.4 has the derivation and the two configurations that disagree.** The
table below is the ADR's original worked band, with the two rows the correction moves restated
beneath the strike:

| space | binary rounds | 16-ary rounds |
|---|---|---|
| leaf space `2^32` (searched BINARY in the shipped ladder — the 16-ary column is what the first draft assumed) | 32 | ~~8~~ |
| history at 131,072 positions, 8,192 tiles | 13 | 4 |
| moves, both, plus terminal ~~92 / 26~~ → **93 / 75** (binary leaf ladder, plus the root claim) | 93 | 75 |
| DAA at the 45-DAA deadline ~~4,140 / 1,170~~ → **4,185 / 3,375 — BOTH over the 3,000-DAA window** | 4,185 | 3,375 |
| bytes a move: leaf space / history at a 64-lane tile | 128 / 1,048 | 1,024 / 8,384 |

The RC's own row is the configuration that ships, and it is a different one: `2^26` ladder, a
512-position history (32 tiles of 16), the RC's 42-DAA clock, an 8-lane disputed site →
**arity 2, 65 moves, 2,730 DAA, plus the 216-DAA assembly reserve = 2,946 of 3,000**
(`the_rcs_derived_deadline_selects_an_arity_for_its_own_row_and_none_for_the_fences`).

`k` is a `PalwCourtParamsV2` quantity (`dissection_arity`, binary on every court built today) —
inside `palw_ruleset_id_v2` like the ladder — and it is DERIVED at genesis as the smallest power of
two for which
`(2 × (⌈log_k L⌉ + ⌈log_k (n_max / tile)⌉) + terminal) × turn_deadline ≤ window_court` over the
widest row the ruleset will admit, with `turn_deadline` the SA-4 derivation. No preset writes a
chosen `k`. The bytes a move carries are inside one carrier at every `k ≤ 64` for a disputed tile
of 128 lanes and at every `k ≤ 32` for one of 256 (`palw_attn_dissect_arity_fits_carrier_v1` — a
property of the PAIR, applied by the derivation at the widest registered tile, never of the arity
alone). On a RUNNING chain the same value arrives by activation rather than by re-genesis: the
bare top-level fence `Params::palw_kary_court`, `None` on every shipped preset, under which the
court's arity becomes the derived value and the dissection arm becomes admissible — the
`palw_context_ladder` shape, which swaps a court parameter inside the ruleset id at its DAA. It
carries no companion value (the `palw_bond_maturity` hazard): what it selects is a pure function of
the ruleset every node already holds.

**Decision 4 — the bottom of the dissection opens tiles, so the anchor is tile-addressed and
priced as such.** ADR-0080 Decision 5 said the anchor must be tile-addressed; ADR-0081 §1.1 found
that on its own it flattens the opening and not the close; here it is exactly what the bottom of
Decision 2 needs and no more. Two things follow. A graph-v5 class registers the tiled map
(`tiled_kv_state_geometry_v3`) and the ladder rule prices the cache half by the CLASS's map — the
`_for_map_v1` twin the recurrence half already has and the cache half does not
(`the_v3_map_is_not_priced_by_the_ladder_rule`) — so a class is charged the 18,432-byte tile
opening it will carry, never the 526,336-byte history it will not. And the attention cache is
PREFIX-STABLE: a K or V row written at position `j` is the same bytes in every later checkpoint,
so the executor retains the cache once and recomputes any checkpoint's tiled root from it; no
per-checkpoint copy of the history exists to retain or to serve.

**Amended 2026-09-04 (stream K, audit A C-4/H-1, fixers FA and FB) — a tiled-map class commits a
checkpoint at EVERY position, prefill included, and the cache-write route is then REFUSED for it.**
The shipped cadence checkpoints per DECODE CALL, so no checkpoint covers any prefill position and
none covers a tile straddling the last checkpoint; at those positions the bottom's only route is
the cache-write one, and that route was measured at **149,953 bytes = 1.80 carriers** for a prefill
dispute (dense graph-v5 @ 512, RC `2^26` ladder, tiled v3 map, dissection court: 133,057 at `2^22`,
175,297 at the `2^32` fence). A checkpoint after every position takes the same dispute to
**40,461 bytes = 0.49 carriers at EVERY position class** — prefill, first decode, tile-aligned,
straddling, last — and the hybrid to 73,741. The cadence is read off the registered
`state_chunk_map_id` (`PalwCheckpointCadenceV1::PerPosition`), never declared, so a registrant
cannot buy a coarser leg. And the route is not the challenger's choice either: for a class whose
site says `every_position_is_checkpointed`, the cache-write route is refused by name
(`the_cache_write_route_is_refused_by_name_where_every_position_is_checkpointed`), because audit A
found a K/V SERIES SWAP admissible on it — a route that convicts an honest executor. The anchor is
at `p + 1`, not `p − 1` plus a residue: the residue is then empty and position 0 is anchored, which
is what took the dense close from 93,367 (2 chunks) to 82,719 (1) at arity 16. **What "the
executor retains the cache once" costs is §10.3, and it is not nothing**: retention is 0 state
bytes a position (142 leaf bytes), but the leaf hash binds the chunk INDEX, so a per-position
capture re-serialises 696,516,608 bytes a job at `n_ctx` 512 where the naive whole-cache form is
7,530,872,832.

**Decision 5 — the prompt ids ride as a Merkle root, armed with the first graph-v5 row.**
ADR-0081 Decision 3 as implemented (`palw_prompt_ids_v1.rs`), no longer optional: at 131,072 ids the
flat term is 524,288 bytes on EVERY close, and Decisions 1–4 leave it as the only context-linear
term of the PROMPT. ~~The fence `palw_prompt_ids_merkle` is armed in the same ruleset move as the
rows~~ — **corrected at the cut (2026-09-04): it is NOT armed with the first row.** At `n_ctx` 512
the flat ids are 82,080 bytes against an 83,333-byte carrier, so the registered row fits in either
form, and arming the fence moves every free-prompt job id for nothing. The Merkle form becomes
REQUIRED above about `n_ctx` 1,024, which is the width to arm it at; until then the fence refuses
arming until its inputs exist rather than sitting dormant and armable (fixer FD, audit D M-2), so
the dormancy is a rule and not an omission. Every moved id is re-pinned and listed when it does
arm, and `PalwFpMaterialV1` keeps carrying the ids whole
for the seats (ADR-0081 Y7). **Measured limit (U-04, Z0):** with the prompt ids Merkle-ized a
graph-v5 dense close is flat only to about `n_ctx` 4,096; at 32,768 the binding node becomes the
embedding gather, whose GENERATED-token ids (`decode × 4` bytes, the decode pin's flat id list)
are a second linear term this decision does not anchor. It is linear in the ANSWER's length, not
the prompt's, so "thousands of tokens" holds and "tens of thousands" does not until the output ids
ride the same tiled Merkle idiom — one more fence, the symmetric half of ADR-0081 Decision 3,
recorded in §8 as not decided here and not claimed.

**Decision 6 — the close ceiling is re-derived from the FLAT terms, and the chunk arm is a
prerequisite, not a hope.** With attention refuted by dissection, the widest term of a close is a
MODEL width, not a context: on the dense tier the SwiGLU down projection's operand
(`ffn_dim` 8,960 lanes; the ADR-0080 §1 binding node, 82,080 bytes with the flat ids and ~80,504
with the Merkle ones), on the hybrid one head's recurrence state (`k_dim × v_dim × 4 = 65,536`,
charged once since `4f859f9a`). `DEFAULT_MAX_CLOSE_CHUNKS` is therefore
`palw_close_chunks_for_bytes_v1(max over the registered families of the widest term)` — a
derivation, evaluated at genesis over the rows the genesis set registers. **Not at this cut
(2026-09-04 ruling, audit D M-5): the builder and its reporting test ship UNWIRED and
`DEFAULT_MAX_CLOSE_CHUNKS` stays the hand-supplied 27, because the genesis set now derives to 1
(dense) — and a ceiling tightened to the genesis set permanently refuses any later permissionless
class whose close exceeds 333,333 bytes, on a network whose whole admission story is that anyone
may register. Post-5f the builder reads `palw_shipped_court_rows_v1()`; the derivation is written,
tested and inert, which is the honest state and is §10.7's open item, not a claim in this
sentence.** Stated for BOTH genesis
sets, because two of them are on the table: for the graph-v2/v3 context rows design A was written
for, the widest term is still the context-linear attention close, and the derivation returns
design A's own numbers — **14** for `{floor, A16 dense 512}` (`ceil(1,154,673 × 1.2 / 100,000)`),
**27** with the QWEN36-v3 512 row, 1 for the floor alone; for graph-v5 rows (Decisions 1–5) the
widest term is a MODEL width and the derivation returns — measured by U-04's test
`the_close_ceiling_is_the_derivation_over_the_genesis_set` at `n_ctx` 512 under the dissection
court with Merkle prompt ids — ~~**80,504 bytes = 1 chunk** on the dense tier (binding node
`ffn_down`; 82,080 with flat ids) and **200,732 bytes = 3 chunks** on the hybrid (binding node the
recurrence `GatedDeltaNet`: its `interval × 5 refs` replay evidence, not attention), the set → 3~~
— **restated 2026-09-04 with the route and the arity that make each number true (stream K, stream
J, §10.1): the row the genesis registers closes at 81,599 bytes = ONE carrier, binding node
`attn[7]` `AttnFused`, at arity 2 (the arity the RC and devnet rulesets DERIVE with that row in
`genesis_objects`, off a `2^26` ruleset ladder and a 42-DAA clock), checkpoint evidence route,
tiled v3 map, Merkle ids, openings priced at the `2^32` ladder rules; the same
row at arity 16 is 82,719, and the whole 1,120-byte difference is one move's disclosure. The
binding node moved from `ffn_down` to the fused site because the bottom is now priced at the
checkpoint route (216,019 = 3 chunks was the same row priced at the cache-write route). The
hybrid is 274,460 bytes = 4 chunks at arity 16, still bound by the recurrence's replay evidence and
still unregistered.** The
fused dispute's bottom opening is ~~25,120 bytes dense and 42,016 hybrid on the checkpoint-tile
route (derived), 37,982 and 55,390 on the cache-write route~~ — **the first draft priced one head's
slice, and a checkpoint chunk holds the whole cache ROW (stream E): on the real registered row at
the `2^32` fence it is 41,997 dense and 75,277 hybrid on the checkpoint route, 175,297 and 139,777
on the cache-write one** (measured by borsh on the real object,
34 rows with their paths), flat at 512 / 4,096 / 32,768. "Flat" as a measurement rather than an
adjective (verified independently by a second session on a detached checkout): the dense graph-v5
close is ~~**80,504 bytes at `n_ctx` 512 and 80,696 at 4,096**~~ **82,719 at 512 and 82,911 at
4,096 under the amended Decision 4 at arity 16 (81,599 at 512 at the derived arity 2) — 192 bytes
for eight times the context either way: the SLOPE is what the measurement holds, and the intercept
moves with the evidence route and the arity**; at 32,768 it is 303,640 with the binding node moved
to `EmbedLookup` (Decision 5's
measured limit). Whatever it evaluates to, a row is admitted at a chunk count only when the transport
carries it — W5's own table (§1.4), never the certification lane's `pending_chunks` and its 8. The
CODE CONDITION, not a schedule: until W5 is in the ruleset, `max_close_chunks` is ONE and the
admission gate says so; an admitted row whose worst close no carrier can file is the 5f state §1.4
describes, and it is refused here by construction. The devnet carries its own count (1) and keeps
its minutes lattice, which is what turns `the_reserve_reads_the_rulesets_chunk_count` green
(`2694440c` on 5f: the assembly reserve is per-ruleset).

### Part B — the executor is flat in the context

**Decision 7 — the capture is a fold.** The free-prompt path folds each leaf hash the moment the
engine produces it (`Base0SparseStepAccumulatorV1`), retains the tree at
`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1` and throws every tile away; an opening asked for later is
re-derived by replay from the checkpoint chunks (`fp_interval`). The executor's per-position cost
becomes the forward pass plus the hashing of the rows it commits, and that ratio — not a context
width — is the practical lane's first number. **Measured (U-01, the real dense artifact, §1.5's own
job of 26 prefill + 12 decode = 4,074,040 leaves, 110,109 leaves a position, one process, three
phases):** on a quiet host, the un-captured forward 0.0938 s a token; the dense capture 4.527 s a
token (48.3×; 13.46 MB a position retained); **the fold 0.356 s a token — 3.8× the forward, 12.7×
faster than the capture, 0.74 MB a position retained, 18.1× less.** On the same host at load ~20
the three were 0.214 / 5.806 / 0.491 (27.2× and 2.3×), which brackets §1.5's 5.66 s a token and
says both ends of the ratio move with the host. What the fold leaves above the forward is
0.77 µs a leaf, the hashing the step leg commits, as §4 predicted; nothing is attributable to
tiles, allocation or I/O. Of what the fold retains, 73% is checkpoint chunks — the term Decision 4
and Decision 9 remove. Under Decision 1 the rows to hash per
position are the base count; the fold's retained set at `2^32` is 64 MiB, and a deeper ladder
raises the retained level by the same derivation (`retain_level = ⌈log₂ leaves⌉ − 20` keeps it at
64 MiB). What the executor RETAINS for the claim's life is the KV cache (Decision 4) and, for a
recurrent family, its state at a SPARSE set of checkpoints: the DA obligation is to SERVE any
opening the leg names, not to STORE it, and a checkpoint between two retained ones is re-derived
by replaying the recurrence over at most the retained spacing. The spacing is a class quantity,
derived so that one re-derivation costs no more than the interval replay a seat already performs.

**Decision 8 — the executor prices a job against the ruleset's ladder.** W1b (`bb4f145b`) as
written: the worker reads `max_step_leaf_count` from the bundle it serves and counts a job without
walking it. A prerequisite (U-00): with it absent, no row wider than ~38 positions executes on any
class, whatever Parts A and C admit.

### Part C — the seat is bounded in bytes and priced in compute (Ambient's validator shape)

**Decision 9 — a seat recomputes the cache from the prompt it holds; it never fetches the
history.** ADR-0077 Decision 8 has the seat ask the executor for "the checkpoint chunk at the
interval's start". For an attention family that chunk IS the history — 7.5 GB on the dense tier
at 131,072 positions (`131,072 positions × 2 caches × kv_dim 256 lanes × 4 bytes × 28 layers`),
5.4 GB on the hybrid's ten attention layers (`kv_dim` 512) — and a seat whose bytes are the
history is a seat R1 and W10 forbid. The seat
holds the prompt ids already (on the commitment under `PublicDa`, in the served material under
`PanelDa`) and the committed output ids, so it can RECOMPUTE the job up to any interval's start with
the class's own kernels, exactly, and check the tiled root it computes against the checkpoint root
the executor committed — 64 bytes, not gigabytes. It then replays its `k` drawn intervals from its
own state and compares every committed row exactly, as Decision 8 says. Bytes per seat stay
`O(k × (interval × row + log₂ leaves))`; compute per seat becomes ONE forward pass of the job plus
`k` intervals — Ambient's validator cost, with Ambient's tolerance replaced by equality.
Consequences, stated as the derivations they are:

* The width a row admits on the seat side is bounded by the window a seat has to file in:
  `n_max ≤ window_receipt × rate_seat_prefill`, with `window_receipt` a ruleset window (600 DAA on
  the RC) and `rate_seat_prefill` the SA-4 measurement on the slowest fleet host for that class. No
  number is chosen. **The rule is enforced where seats are measured**: a row nobody can seat
  certifies nothing (ADR-0075), so the bound is checked by the certification drill that measures
  the replay floor (`palw-certify`'s route), which refuses to certify a width the slowest seat
  cannot recompute inside the window — not by `verify_class_admission`, which cannot read a fleet
  measurement and must not pretend to. The measured rate is recorded on the certification, beside
  the replay floor SA-4 already records there.
* A challenger in Decision 2's dissection needs the same state to name a child, and a challenger
  is a seat that recomputed (ADR-0077 Decision 8: "holding the refutation's inputs already").
  Nothing new is fetched for a dispute.
* `Incapable` stays the honest verdict for a seat without the artifact (the 5e stop form), and a
  seat without the compute for a row files it too: the panel's capability is a fact about the
  fleet, and a row nobody can seat certifies nothing (ADR-0075).

### Part D — what earns, what is chosen, what the lane can carry

**Decision 10 — on the free-prompt lane, quanta are earned by DECODE leaves; the prefill of a
user-chosen prompt is priced at zero.** The quantum is unchanged (ADR-0074 Decision 5:
`max(1, canonical_leaves / 8)`); the numerator of `fp_quanta_v3` becomes the leaves of the decode
calls, derived by the transition from the job it can enumerate — `prompt_tokens`,
`decode_tokens_executed` and the class profile — never from a count the executor carries. The
attempt lane's canonical job is untouched: its prompt is drawn from the beacon and cannot be
cached. Rationale is §1.6's arithmetic: a deterministic causal model makes every prefix leaf
recomputable at zero cost by the bond that computed it once, so prefill leaves are not scarce and
a subsidy on them is a subsidy on replay. Armed with the graph-v5 rows and not before: at today's
two-decode-token rows a decode-only numerator floors to `ZeroQuanta`, which is the census's lower
band edge and would refuse every honest job on the shipped classes. ADR-0077 Decision 14's
footprint rule is restated in the same unit: the canonical job's DECODE leaves are at least
`n_ctx / 8` positions' worth, so the widest admissible answer earns at most `8 × 8 = 64` quanta by
construction.

**Decision 11 — decoding is a seeded argmax, and the pin's refutation is unchanged.** The class
gains `decode_token_select_v2`: the committed token at a position is
`argmax_j (logit_j × T_ONE + T_q × G_j)`, ties to the lowest index, where `T_q` is the temperature
in Q24, `T_ONE = 2^24`, and `G_j` is a Gumbel variate read from a PINNED Q24 table indexed by the
top bits of a keyed hash of `(sampling_seed, position, j)` — a table in the class's kernel catalog
beside `int_exp`'s, so there is no transcendental in the rule. `T_q = 0` is byte-identical to
`base0_decode_token_select_v1`, which is what every shipped row keeps. Two properties make this the
ONLY sampler this court can carry: the key is a per-lane function, so the refutation stays the
two-disclosure form of the tiled logits pin — open the committed lane's tile and a beating lane's
tile, recompute two keys — at any vocabulary; and Gumbel-max samples exactly from
`softmax(logits / T)`, so a user gets a real distribution, not a heuristic. `(sampling_seed, T_q)`
are fields of `PalwFreePromptJobV3`, inside `fp_job_id_v3` and therefore inside the claim id: a
seed cannot be changed after the fact, and grinding one costs a whole inference per draw, which is
ADR-0072's rule kept (one inference, one ticket) rather than broken.

**Decision 12 — the lane's throughput ceiling is a derived, published number, and this ADR does
not raise it.** `min(PALW_V2_MAX_PAYOUTS_PER_BLOCK × blocks_per_day, panel replay capacity)` is
computed from the constants and the fleet's measured replay rate and printed on the practical
lane's page beside the stages a claim goes through. §1.6's 5,760 a day is the first term today;
raising it is a ruleset move whose premise the payout constant states at itself, owed its own
argument (`palw_economic_locus_v1`'s payout-queue row), and out of scope here.

### Part E — Ambient, named

**Decision 13 — what is borrowed and what is refused, so the next reader does not re-derive
it.** Borrowed, on ADR-0026's evidence: verification of a WINDOW (ADR-0077 Decision 8, kept); a
verifier that HOLDS the context and recomputes (Decision 9); a proof whose size does not depend on
the context (Decision 2 — Ambient's Merkle commitment per window, ours per tile); asynchronous
verification off the block path; bonds and slashing. Refused, as ADR-0026 refused them: the
tolerant comparison (every check here is equality inside a pinned integer class); "verify one token
is sufficient" (a seat's verdict convicts nobody, ADR-0028); a verifier compiled into the runtime
(the fused op's semantics is the composition of four golden-frozen kernels in `consensus-core`, and
the runtime is an adapter beneath it). Added, because exactness needs it and tolerance does not:
the dissection.

## 4. What this costs, stated before it is measured

* **Chain bytes.** Per claim: unchanged (`out` rows are the rows `ATTN_VALUES` commits today;
  three context-wide rows per site are REMOVED from the step space). Per close: bounded by
  Decision 6's flat terms — one or two carriers on the shipped families, re-derived at genesis.
* **Court moves.** ~~`2 × (⌈log_k L⌉ + ⌈log_k (n / 16)⌉) + 2`: 26 at `k = 16`, `L = 2^32`,
  `n = 131,072`; 1,170 DAA at the 45-DAA deadline against 3,000. A dispute at 131,072 positions
  costs the court FEWER moves than a dispute at 512 costs it today.~~ **Corrected 2026-09-04
  (audit D H-2, audit A M-2): the leaf ladder is binary and the root claim is a move, so it is
  `2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1` — 75 moves at `k = 16`, `L = 2^32`, `n = 131,072`,
  3,375 DAA at the 45-DAA deadline, which is PAST the 3,000-DAA window; the RC's shipped
  configuration (`2^26`, its 512 row, the 42-DAA clock) is 65 moves and 2,730 DAA, 2,946 with the
  assembly reserve. And the last sentence inverts: a dispute at 131,072 positions costs the court
  MORE moves than one at 512 — thirteen history rounds against five at arity 2 — what is bounded
  is the GROWTH, logarithmic in the history, and at the `2^32` fence that growth no longer fits the
  RC's window at any arity (§10.4).**
* **The bottom opening.** `4 × d_head` (the query slice) + `2 × 16 × 4 × kv_dim` (one K tile, one V
  tile — a checkpoint chunk holds the whole cache ROW, `attn_kv_heads × attn_head_dim`, because the
  map addresses `(kind, layer, position)` and not the head; the ADR's first draft priced one head's
  slice, which under-bounds the object by `kv_heads`) + `4 × tile_len` (the output tile) + paths at
  the ladder's depth. Measured on the real objects (U-03, borsh, `kv_heads` 1, 64-position history):
  the checkpoint route 19,027 bytes at `d_head` 128 and 36,435 at 256; the cache-write route
  (one leaf per row, for the rows after the last checkpoint) 37,985 and 55,393. ~~Inside one carrier
  at every context on both routes; the derivation prices the larger.~~ **Not on both routes, on the
  real registered row: at the `2^32` fence the cache-write route is 175,297 dense and 139,777
  hybrid, over the 83,333-byte carrier — Z3 was FALSE there, which is what the amended Decision 4
  answers. With a checkpoint at every position the whole prefill dispute is 40,461 bytes = 0.49
  carriers at every position class (RC `2^26` ladder), the cache-write route is refused for such a
  class, and the derivation prices the route the ruleset can actually play.**
* **Executor time.** The forward pass plus hashing the base leaf count — `~1.3 × 10¹⁰` leaf hashes
  for a 131,072-position dense job, the same order as the forward's own work. U-01 gives the ratio.
* **Executor retention.** The KV cache for the claim's life (`claim_retirement`, 3,000 DAA on the
  RC): 7.5 GB dense, 5.4 GB hybrid attention at 131,072 (Decision 9's arithmetic); plus a
  recurrent family's sparse checkpoints at `heads × k_dim × v_dim × 4` each — 65,536 bytes a
  head — at the derived spacing. **The amended Decision 4's per-position cadence retains 0 state
  bytes a position on top of that (142 leaf bytes) and SERIALISES 696,516,608 bytes a job at
  `n_ctx` 512 — a cost the first draft did not price at all (§10.3).**
* **Seat time.** One forward pass of the job plus `k` interval replays — at 131,072 positions on
  the hybrid's shipped CPU path, hours; on the dense tier, minutes. Decision 9's derivation makes
  that the width the row admits rather than a surprise the panel discovers.
* **Latency to the person.** Unchanged in kind: the answer streams from the runtime (ADR-0077
  Decision 2) and the chain's stages follow; Decision 7 is what makes the stream run at the
  runtime's speed rather than at the capture's.
* **Identity.** Graph v5 is a new class id per family; `k`, `max_close_chunks`, the prompt-id
  fence, the decode-leaf numerator and the sampling fields are ruleset moves. One re-genesis on
  testnet-11 carries all of them; mainnet is untouched.

## 5. Invariants the tests must hold

```
Z0   No committed row of a graph-v5 class has a context-shaped width: no node's out_len is
     KvScaled, and derive_court_cost_v1 charges every node a close independent of n_ctx except
     for the ⌈log₂⌉ prompt-id term (Decision 5). Swept at 512, 4,096, 32,768 and 131,072.
Z1   AttnFused is the four shipped kernels: for every (q, K, V) fixture the fused op's out equals
     ATTN_VALUES(requant(SoftMax(ATTN_SCORES(q, K))), V) byte for byte, at every kv_len up to the
     class's bound, on every backend (the ADR-0040 differential harness).
Z2   The dissection convicts every forgery and acquits every honest execution: a lie in m*, in
     S*, in any child triple or in any bottom tile is found by the fold or the recompute, and an
     honest responder is never convicted — swept over history lengths, tile boundaries (ragged
     last tile), and every k the derivation can select.
Z3   The bottom opening of an AttnFused dispute is inside one carrier at every context and every
     registered d_head and tile_len; the per-move disclosure is inside one carrier at every k the
     derivation selects.
Z4   The court's worst-case moves, with both search spaces, fit window_court at every preset:
     (2 × (⌈log_k L⌉ + ⌈log_k (n_max / 16)⌉) + terminal) × turn_deadline ≤ window_court, with k,
     turn_deadline and n_max read from the ruleset, never typed.
Z5   A seat verifies a claim fetching O(k × (interval × row + log₂ leaves)) bytes at every context,
     and the checkpoint root it recomputes equals the committed one on honest material; on
     tampered material the mismatch names the checkpoint.
Z6   The capture's per-position cost is independent of n_ctx and of retention: the fold's retained
     set is ≤ 64 MiB at every ladder the ruleset admits, and an opening re-derived by replay equals
     the opening a dense capture would have served, byte for byte.
Z7   Economic invariance (ADR-0080 Decision 3, as the census tests it): a job's quanta, pwu and
     ticket inputs are functions of its DECODE leaves alone on the free-prompt lane; two jobs with
     the same decode leaves and different prompt lengths earn identically; the attempt lane's
     canonical job earns exactly what it earns today.
Z8   decode_token_select_v2 at T_q = 0 equals decode_token_select_v1 on every logits row; at
     T_q > 0 the two-disclosure refutation convicts a token that is not the argmax of the keyed
     row and acquits the one that is; the seed and T_q are inside fp_job_id_v3.
Z9   ADR-0044 F4–F6 hold verbatim: neither k, nor a row's n_ctx, nor the sampling fields enter the
     beacon, the panel or the ticket; the existing golden vectors pass unchanged.
Z10  A row wider than the chunk arm carries is refused at admission BY NAME while the arm admits
     only FamilyCertified or caps the count below the ruleset's (Decision 6); a worker whose ladder
     is not the ruleset's refuses to serve a row the ruleset admits (Decision 8).
Z11  The class's admitted width satisfies all three bounds at once — the close (Decision 6), the
     ladder (Decision 1's count under the ruleset's L), and the seat's window (Decision 9) — and
     the refusal names which bound refused.
```

## 6. Order of work

Nothing below is built before the measurement above it returns; U-00 and U-01 are not this ADR's
design and are its precondition.

| unit | content | done when |
|---|---|---|
| U-00 | **5f's other half — owned by the 5f integrator, not this ADR.** ADR-0080 W5 lands the court's own chunk-group table with the four requirements §1.4 records; W1b is merged on this ADR's branch; until W5 is in the ruleset, `max_close_chunks = 1` on every preset | Z10 green; `misaka-cli palw court-close` files a two-carrier close on devnet and the transition applies it |
| U-01 | **Measure the capture.** Decision 7's fold wired into the free-prompt worker; the per-token ratio against the un-captured forward on the §1.5 host and job | the ratio is a number in this ADR; Z6 green |
| U-02 | Decision 1 — `AttnFused` in the IR, the profile projection, the engine (`engine_a16.rs`), the adjudicator, the fuzz gate; graph-v5 profiles for both families | Z1 green; no `KvScaled` row projects from a v5 profile (Z0's first half) |
| U-03 | Decision 2 — the dissection arm in `palw_step_refute` / the court object; Decision 3 — the k-ary ladder in `palw_bisect` and the move derivation in `PalwCourtParamsV2`; Decision 4 — the cache half of the ladder rule reads the class's map | Z2, Z3, Z4 green |
| U-04 | Decision 6 — `max_close_chunks` derived from the flat terms; `derive_court_cost_v1` prices `AttnFused` by its bottom opening and its per-move disclosure | Z0 green at all four contexts on both families |
| U-05 | Decision 9 — the seat's recompute path (`kaspad/src/palw_panel.rs`), the checkpoint-root check, the per-class `rate_seat_prefill` in the row and at admission | Z5, Z11 green on devnet with three seats |
| U-06 | Decisions 10, 11, 12 as TESTS before features: the decode-leaf numerator, `decode_token_select_v2`, the published ceiling | Z7, Z8 green; the census's rows re-asserted |
| U-07 | Decision 5 — the prompt-id fence armed with the rows; every moved id re-pinned and listed | the u04 blast-radius test passes at the new pins. **Landed as the FORM, not the arming: the fence is dormant at the 512 row and refuses arming until its inputs exist (§10.7)** |
| U-07b | Decision 5's symmetric half — the OUTPUT ids as a tiled Merkle root, without which the close stops being flat above `n_ctx` ~4,096 (§8) | **open; not in this cut** |
| U-08 | the graph-v5 rows registered through ADR-0075's route and drilled (`misaka-palw-fp-devnet-e2e.sh`): dense and hybrid at 512, then 8,192, then the width Z11 admits | ONE job id in three places (ADR-0077 §6's gate) at each width, with a dispute drilled to the bottom tile at the widest. **Partly: the DENSE 512 row is registered and the devnet drill's stages 1–3 pass; 4–8 are blocked by name on the shipped artifact's all-zero tokenizer commitment; the hybrid is not registered at all (§10.7)** |
| U-09 | the pages: `testnet11-join-mining.md`'s prompt section states the three bounds, the stages, the ceiling of Decision 12, and what "private unless disputed" means | no page promises a width a bound refuses |

**Done when** a person gives a graph-v5 row a prompt and receives an answer whose combined length
is the width Z11 admits — thousands of tokens on the dense tier, on the fleet as measured — streamed
at the runtime's speed; the chain holds one claim for it; five seats certified it having fetched a
bounded number of bytes each and recomputed the rest; a deliberately corrupted execution at that
width was convicted through a dissection whose every move fit one carrier; and the same leaves as
one claim and as any other arrangement of the same answer earn the same, because there is only one
arrangement left.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0080 §3 Decisions 1, 2, 4, 5, 6, 7 — one job as N verification segments | **withdrawn** (refuted before implementation; §1.1). Decision 5's "tile-addressed anchor" survives as Decision 4 here, in its honest role: the bottom of a dissection |
| ADR-0080 §1 and Decision 3 | kept: the measurement is §1.2's starting point and the invariant is Z7, as the census tests it |
| ADR-0080 design A (`DEFAULT_MAX_CLOSE_CHUNKS = 27`) | kept as the BRIDGE and re-derived (Decision 6): the count is `chunks(widest term over the genesis set)` — 14/27 for the graph-v2/v3 context rows, ~~one to three~~ **1 (dense, 81,599 B at arity 2) and 4 (hybrid, 274,460 B)** for graph-v5 rows; the transport is W5's own table, the 5f integrator's. **The constant stays 27 by hand at this cut and the derivation ships inert — Decision 6, audit D M-5** |
| ADR-0081 §3 Decisions 1, 2, 4–9 — the prompt as a prefill state chain | **withdrawn**. What a long prompt needed was never a chain of prefill segments; it was a court that never carries the history (Decision 2) and a seat that recomputes it (Decision 9) |
| ADR-0081 Decision 3 — Merkle prompt ids | kept; ~~ARMED with the rows~~ **implemented and DORMANT at the cut — the flat ids are 82,080 B against an 83,333-byte carrier at `n_ctx` 512, and the fence refuses arming until its inputs exist (Decision 5, corrected)** |
| ADR-0081 §1.1 — "graph-v4 makes a per-leaf attention opening tile-sized" | half right, as its own status block found: the OPENING is tile-sized, the CLOSE was not; Decision 2 is what makes the close tile-sized |
| ADR-0077 Decision 8 — the seat asks the executor for the interval's checkpoint chunk | amended by Decision 9: the seat recomputes the state and fetches the rows it compares; the draw, the exactness and the verdict's weight are unchanged |
| ADR-0077 Decision 11 — admission prices the checkpoint interval | kept for the recurrence; for attention the interval is not the price — the dissection's bottom is (Decision 4, 6) |
| ADR-0077 Decision 12 — `COURT_MAX_STEP_LEAVES = 2^32`, binary rounds | the constant stands; the ladder's DEPTH stops being the binding constraint because rounds are k-ary (Decision 3), and the leaf COUNT returns to the base count Decision 12 was sized against (Decision 1) |
| ADR-0077 Decision 13 — rows at 512, 2,048, 8,192 per family | re-sized: a row's width is the minimum of three derived bounds (Z11), not a rotary table; 512 is the first drill and no longer a ceiling |
| ADR-0077 Decision 14 — the canonical footprint is at least `n_ctx / 8` | restated in decode leaves (Decision 10) |
| ADR-0077 §8 — KV continuation across turns | still not decided (§8) |
| ADR-0074 Decision 5 — the quantum is an eighth of the canonical job | honoured; on the free-prompt lane the numerator is decode leaves (Decision 10) |
| ADR-0072 — the ticket is the execution | honoured: a sampling seed is inside the job id, so one inference is one draw (Decision 11) |
| ADR-0040 Decision E — the order of accumulation cannot change the result | spent, not amended: it is the premise that lets a dissection reorder a reduction across tiles and children |
| ADR-0026 — borrow Ambient's architecture, refuse its proof model | honoured and itemized (Decision 13); the dissection is the one part Ambient's shape lacks because tolerance never disputes |
| ADR-0028 — sampling decides nothing; only the court convicts | honoured: a seat's recompute convicts nobody; the court's dissection does |
| `palw_bisect` — "the pinned midpoint of the disputed interval; ≈ log₂(space) rungs" | amended by Decision 3: the midpoint becomes `k − 1` pinned cut points; the contract's pinned index space, round bound and terminal handoff are otherwise unchanged |
| `base0_decode_token_select_v1` — the argmax with lowest-index ties | kept as `T_q = 0` of `decode_token_select_v2` (Decision 11) |

## 8. What is deliberately not decided

* **The output ids as a tiled Merkle root — U-07b**, the symmetric half of ADR-0081 Decision 3
  (Decision 5 names it and does not take it). Measured limit, U-04's Z0 sweep at the `2^32` fence
  with Merkle prompt ids: the dense graph-v5 close is flat to about `n_ctx` 4,096 and is 303,640
  bytes at 32,768, where the binding node becomes `EmbedLookup` and the GENERATED ids (`decode × 4`
  bytes) are the term. Linear in the ANSWER, not the prompt, so "thousands of tokens" holds and
  "tens of thousands" does not. Not in this cut, not fenced, listed in §10.7.
* **KV continuation across jobs** (`ContinueFrom{parent_receipt}`). ADR-0077 §8's exclusion stands.
  Decision 10 removes the reason it looked urgent — a re-sent history no longer earns anything —
  and Decision 9 removes the reason it looked cheap: a seat would have to trust a parent's cache
  or recompute it anyway.
* **Who pays for a long prompt.** Decision 10 says the chain does not. Whether the requester pays
  the executor a fee for prefill, in what unit, and through what market, is a product decision
  outside consensus. ADR-0077 SA-1's exposure budget is the only bound a public gateway has today.
* **The value of `k`, of `max_close_chunks`, of a row's width.** All derived (Decisions 3, 6, 9,
  Z11); the derivations are written and the numbers are not.
* **Beyond a million positions.** Decision 1's base count puts the dense tier at `2^32` leaves
  near 43,000 positions and the hybrid near 14,000; `2^36` reaches eight and sixteen times that at
  ONE more 16-ary round each, but the executor's hashing and the seat's recompute grow linearly
  and the artifact's rotary table must be re-converted wider. Those are measurements, not
  decisions, and U-08's last row is where they are taken.
* **Whether prefill should earn a DISCOUNTED rate rather than zero.** Zero is the rule that
  cannot be gamed; a discount is a rule that can be, at the discount's rate. Not opened here.
* **A fleet mode in which seats FETCH the cache** over a private network. Refused as a protocol
  default (R4); a deployment may do it below the protocol and gains nothing in consensus.
* **Integer GPU kernels, encrypted DA, receipt transfer** — ADR-0077 §8's list stands.

## 9. Number hygiene

This is ADR-0082; ADR-0081 is the last on this branch (`palw-adr0080-verification-segment`), and
`main`'s README records 0080 as the next free number with 0080 and 0081 resident on branches. A
concurrent claimant renumbers the later writer, per ADR-0036 Decision 5.

## 10. Implementation record — 2026-09-03 / 2026-09-04

Numbered 10 and not 8: §8 and §9 were already spent, on what is not decided and on the numbering,
and renumbering a section other documents cite is how a reference stops resolving.

Built on `palw-adr0082-impl` by thirteen streams and seven fixers over two days, from a base of
`palw-testnet-5f` plus `palw-adr0080-verification-segment`. Head at writing `76b790f4`:
consensus-core **1,814 passed / 3 failed** — the three are the genesis pins the 5f integrator
re-pins at the freeze — misaka-palw-base0 **328 / 0 / 2**, cli 74/0, kaspad lib 67/0.

**§1's discipline applied to this section: every figure below names the COURT (shipped binary, or
the dissection court and its arity), the LADDER, the class's MAP, the EVIDENCE ROUTE, the PROMPT-ID
FORM, and the stream that measured it.** Several figures in the first draft were wrong for exactly
the reason §1 gives — quoted under a configuration they were not measured in — and every one of
them is struck in place above and restated, never overwritten: Decision 1's fence widths and its
574, Decision 3's selected arity and its move table, Decision 5's arming, Decision 6's close and
bottom-opening figures and its "flat" sweep, §4's move and bottom-opening bullets, §7's two rows.

### 10.1 The close: one carrier, and the arity is part of the number

Dense graph-v5, the row the genesis registers, all under: dissection court, tiled v3 state chunk
map, checkpoint evidence route, Merkle prompt ids, per-position checkpoints (Decision 4 as
amended), Merkle output ids NOT taken.

All five rows below price openings at the same ladder rules (`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`,
`2^32`); what differs is the ARITY and the route.

| row | arity | close | carriers | binding node |
|---|---|---|---|---|
| dense graph-v5 @ 512 — **the registered row**, priced under the ruleset that registers it | **2**, derived | **81,599 B** | **1** | `attn[7]` `AttnFused` |
| the same row, priced under the sweep's court | 16 | 82,719 B | 1 | `attn[7]` `AttnFused` |
| the same row at `n_ctx` 4,096 | 16 | 82,911 B | 1 | `attn[7]` `AttnFused` |
| the same row, bottom charged at the **cache-write** route (before stream K) | 16 | 216,019 B | 3 | — |
| hybrid graph-v5 @ 512 — **unregistered** | 16 | 274,460 B | 4 | the recurrence's replay evidence |

`82,719 − 81,599 = 1,120` bytes, and the row's own test attributes the whole of it to the per-move
disclosure at arity 16: the arity is not a footnote on a close figure, it is a term in it. The RC
and the devnet rulesets both DERIVE 2 with the v5 row in `genesis_objects` (window 3,000, turn
deadline 42, ruleset ladder `2^26`, 512 positions at a 16-position tile, an 8-lane disputed site) —
not the 4 an earlier sweep reported, not the 16 the ADR worked its table at — and the test prints
its whole configuration on the line beside the number
(`misaka-palw-base0/src/classes.rs`, `consensus/core/src/palw_context_ladder.rs`), so a reader who
moves the arity is told by how much.

Corrected above: Decision 6's 80,504 / 200,732 and its "flat" sweep; §4's bottom-opening bullet.

### 10.2 Decision 4 amended: a checkpoint at every position, and one route refused

Stream K, audit A, fixers FA and FB. Measured at the RC's `2^26` ladder on the dense graph-v5 row
with the tiled v3 map:

| the dispute's bottom opening at | route | bytes | carriers |
|---|---|---|---|
| a PREFILL position (the shipped per-decode-call cadence covers none of them) | cache-write | **149,953** | 1.80 |
| the same at the `2^22` / `2^32` ladders | cache-write | 133,057 / 175,297 | 1.60 / 2.10 |
| every position class — prefill, first decode, tile-aligned, straddling, last | **per-position checkpoint** | **40,461** | **0.49** |
| the hybrid, same | per-position checkpoint | 73,741 | 0.89 |

The anchor is at `p + 1` (residue empty, position 0 anchored), which took the close from 93,367
(2 chunks) to 82,719 (1) at arity 16. Retention is **0 state bytes a position** — the fold keeps
no chunk list at all (`the_folds_retention_is_constant_and_the_alternative_is_quadratic`; a
chunk-retaining capture at 16 positions would hold 1,114,112 B) — and 142 leaf bytes a position.

**And the cache-write evidence route is REFUSED for a class that checkpoints every position.**
Audit A found a K/V SERIES SWAP admissible on that route: the openings were bound to the query but
to no coordinate of their own, so an executor could answer with the wrong series and a correct
recompute would convict an honest responder. The fix binds the series to the `KCacheWrite` /
`VCacheWrite` roles and refuses the route by name where `every_position_is_checkpointed`
(`the_cache_write_route_is_refused_by_name_where_every_position_is_checkpointed`). This closed the
arming blocker on `palw_kary_court`: the checkpoint route is sound, and it is now the only route a
graph-v5 class plays. Z3, which was FALSE on the cache-write route at the `2^32` fence
(175,297 dense / 139,777 hybrid), holds.

### 10.3 What the capture costs — the half Decision 4 asserted and did not price

Fixer FB, audit B H-3, measured at `n_ctx` 512 on the dense shape (28 attention layers,
`kv_row = 2 × 128 × 4 = 1,024` B), `the_per_position_capture_touches_one_tile_a_position`:

| form | bytes serialised per job |
|---|---|
| whole-cache, one checkpoint's chunks re-serialised at every position | **7,530,872,832** |
| with per-chunk hash memoisation | **696,516,608** — 11× less |

**The residual is the MAP's, not the memo's.** The leaf hash binds the chunk INDEX
(`state_chunk_leaf_hash_v1(map_id, index, bytes)`) and the tiled map indexes
`(kind · layers + layer) · chunks_per_slice + block`, so when a slice grows a block — once every
`PALW_ATTN_HISTORY_TILE_V4 = 16` positions — every later index MOVES and a chunk whose bytes did
not change is a different leaf. Only a second copy of the cache or an index scheme that does not
move could remove it, and this capture is allowed neither. Decision 4's claim that the cadence
"costs the executor nothing" is corrected here and in the code's own doc: it costs 696.5 MB of
serialisation a job at 512, retains none of it, and the arithmetic that says so is asserted as an
ORDER against derived bounds rather than pinned to a constant.

### 10.4 The ladder, the clock and the arity, derived jointly

Fixers FA and FD, audit A M-2 / audit D H-2b/H-2c. Two corrections compose: the leaf ladder is
BINARY (a session plays the shipped `PalwBisectLadderV1`; only the history search is k-ary), and
the responder's root claim is a move. Honest count:
`2 × (⌈log₂ L⌉ + ⌈log_k (n / 16)⌉) + 2 + 1`, and the derivation selects on
`moves × deadline + assembly_reserve ≤ window_court` — the same inequality admission admits on,
which is what it was not before (the derivation handed admission a value admission then refused).

| configuration | arity | moves | DAA | verdict |
|---|---|---|---|---|
| **RC as it ships** — `2^26`, its own 512 row (32 tiles), 42-DAA clock, 27-carrier reserve | **2** | 65 | 2,730 + 216 = **2,946** | fits, 54 DAA of room |
| the RC at a 4,096 row | 4 | 63 | 2,646 + 216 | fits (arity 2 is 71 moves, 2,982 + 216 — misses by 198) |
| **`2^32` fence, 131,072 row** — the configuration §3's table is worked in | **none** | 73 (cheapest, arity 32/64) | 3,066 before the reserve | **refused, `None`** |
| every shipped preset, zero history | 2 | its own worst case | inside its own window | unchanged by this ADR |

The RC's 42-DAA clock is not typed anywhere: `palw_court_turn_deadline_for_history_v1(3,000, 2^26,
2, 27, 512, 16)` returns `(2, 42)` — the joint form, counting the history rounds and the root claim
in its divisor. The older `palw_court_turn_deadline_v1` returns 51 for the same window, which is
the clock for a ruleset with NO history to dissect, and at 51 no arity fits at all. Both are kept,
because a ruleset without a fused row genuinely derives the second one.

The class WIDTHS under this court (fixer FD, audit D H-5, `2^32` fence, swept through the gate's
own door): graph-v5 dense admits `n_ctx` **16,384**, hybrid **13,105**, and the refusal one wider
is `DeeperThanTheLadder` or `CourtWindowTooShort` — never the close. `574` is and always was the
**graph-v2** row's width at `2^26`.

### 10.5 What a fused row costs with NO dissection court

Measured by the 5f integrator at `2^26` through `derive_court_cost_shaped_v1` (the shaped entry;
the plain walk at `2^22` refused both rows, which is what made the `2^22` sweep of §10.6 D2
load-bearing):

| row | close under a court with no dissection | chunks | against the RC's 2,250,000-byte ceiling |
|---|---|---|---|
| graph-v2/v3 dense @ 512 | 1,999,729 | 24 | fits |
| **graph-v5 dense @ 512** | **3,446,708** | **42** | **over by fifteen carriers** |

So the sentence this ADR and the genesis card both carried — a fused row on a ruleset whose
`palw_kary_court` is dormant is "admitted and unprosecutable" — is not what the code does. It is
**refused at acceptance**, twice over: by name before anything is priced
(`FusedAttentionNeedsTheKaryCourt` — *"the class carries a fused attention site and this ruleset's
court has no dissection to try it with"*), and by fifteen carriers if it ever reached the walk.
`PricedForADifferentCourt { priced, court }` refuses the other direction, a row priced for an arity
the court does not run.

### 10.6 The audit of 2026-09-03, and what closed it

Five auditors on a read-only checkout of `89a991ab` — A court soundness, B capture and checkpoints,
C economics and the split close, D admission, fences and layering, E the fused op and the engine —
then the fixers FA and FA2, FB, FC, FD and FD2, FE. Criticals and their closures:

* **A (court), five criticals, all bindings that were missing.** The root claim's history length is
  DERIVED, never read off the wire (truncation acquitted, over-declaration was a DoS); `S*` is
  validated into a band and the kernel moved to `i128` (an unvalidated `S*` overflowed `e × recip`
  and PANICKED inside block validation — a one-block network kill); bottom openings carry full
  COORDINATES on query, K and V (decode rows addressed by call), so a query bound to nothing is
  refused; the anchor is DERIVED (`palw_checkpoint_covered_for_step_v1`, `WrongAnchor` and
  `RowsAfterOnAPerPositionClass`) rather than "any checkpoint in the leg"; and the root claim has a
  CLOCK at Terminal, so silence no longer wins challenger-side. Beside them: `MixedEvidenceRoutes`,
  `out_tile` verified, and the move count of §10.4.
* **B (capture), three criticals.** The fold now ANSWERS for chunks — `base0_checkpoint_chunks_at_v1`
  had no caller, so every graph-v5 seat's material check would have been `Mismatch`, no interval
  would open and no anchor would exist; ONE cadence unit — the interval, the seat and the panel
  counted DECODE CALLS where the leg counts POSITIONS, which is `CheckpointRootMismatch` on every
  honest v5 claim (a fifth site was found by the tests written for the first four); and the hybrid
  v5 producer PUSHES its checkpoints (`qwen36_push_checkpoint_v1`) instead of filing
  `CheckpointCaptureIncomplete`.
* **C (economics), one critical and the close's economics.** The court's one slot a block is spent
  only by an ADMITTED move — it was spent by an unauthenticated object before validation, so one
  transaction a block starved every close completion and convicted the honest declarer; the
  declaration is legal at Terminal only; the assembly deposit (`palw_close_assembly_deposit_v1(n)` =
  1,250,000 sompi × n) is charged on EVERY non-delivering ending, through the removal funnel rather
  than at two of five sites; chunk deltas are O(1) in the group (26 arrivals journal 2,611,201 bytes
  against 67,709,897 in the swap form); and one prompt is one live claim per bond —
  `fp_work_id_v1` is `(class, prompt, bond)` with `decode_tokens_executed` DROPPED, so one
  inference truncated twice is one work id and not two tickets.
* **D (admission and fences), no criticals, five highs.** One spelling of the arity; the v6 gate on
  the acceptance path AND on the genesis door (a fused row under a dormant fence is refused at
  both); and the two dormant fences `palw_fp_decode_rules` and `palw_prompt_ids_merkle` REFUSE
  arming until their inputs exist, rather than being armable against a transition that would refuse
  by name.
* **D2, the sweep that followed it: the executor's `2^22` no longer bounds the chain.** Six more
  sites carried `PALW_STEP_MAX_LEAVES` where they meant the ruleset's ladder — the free-prompt
  door, the walk's `validate_*_under_ruleset_v3`, the certificate count, the catalog entry and
  `derive_court_cost_v1`'s six callers, the schedule, and the step leg. The last one was
  CONVICTING an honest 512-row producer by name (`StepLeafCountNotCanonical` above `2^22`), and the
  registration helper counted the same way, so no chain could have registered the row at all
  ("4223328 exceeds 4194304").
* **E (fused op and engine), three criticals shared with A.** The shipped constructor now COMPILES
  THE PLAN: `Qwen25A16Backend::new` had `plan: None` and refused the v5 row by traced route
  ("per-layer declares 24 against 27 recorded"), so the fused node executed only through
  `from_registered_profile`. A second cross-machine determinism digest pins the fused corpus
  (a16 `236888781074c28a…`, qwen36 `7e5f14c2b0d66fc5…`).

**State version 19 → 20**: the class state records `fused_attention`, written at the one
construction site and carried on `ClassRegistered`, because the court has to answer "is the
terminal leaf `AttnFused`" from chain state to give the root claim its clock. The genesis door
refuses a fused row without an admission carriage, one disagreeing with the catalog, or one whose
carriage is not the class.

### 10.7 Status, and what is open — by name

The ADR is **IMPLEMENTED on `palw-adr0082-impl`, pending the testnet-11 Relaunch 5f cut.** The
genesis registers the **graph-v5 dense 512 row** (`a16_graph_v5_row_v1`, model id
`Qwen/Qwen2.5-1.5B/graph-v5@512`, canonical job (63, 2), 6,630,544 leaves, class id derived and
printed by its own test rather than typed). It REPLACES the graph-v2 `n_ctx` 16 dense row, which
can serve no free-prompt job at all — the shortest real prompt is 134 tokens — and it takes the
existing dense share constant; no new one is minted. The hybrid stays unregistered (4 chunks, and
the split acceptance is not in this cut). At genesis `palw_kary_court` is armed (`always()`;
without it §10.5 refuses the row), `palw_context_ladder` and `palw_uncertified_weightless` are
armed, and `palw_prompt_ids_merkle` and `palw_fp_decode_rules` are dormant and refuse arming.

Open, each by its own name, so none of them is discovered:

* **U-07b — the output ids as a tiled Merkle root** (§8's new bullet). Not in this cut.
* **Derive the worker's class from the artifact it loads.** The worker names it with a `MODEL_ID`
  constant today; the artifact is what decides which graph it can execute, and the constant is the
  sixth instance this cycle of one quantity spelled in two places.
* **Wire the close-chunk derivation to the shipped rows** (`palw_shipped_court_rows_v1()`).
  Decision 6's builder is written, tested and INERT; `DEFAULT_MAX_CLOSE_CHUNKS` stays 27 by hand
  for the reason recorded at Decision 6.
* **The seat's replay from the prompt under zero retention.** A folded interval's executor replays
  from the prompt for each opening it serves — one forward pass per opening. The alternative keeps
  the live cache across the claim's life, which is a retention shape change at the anchored branch
  of `base0_open_fp_interval_sparse_v1`, and it is not in this cut.
* **The drill's stages 4–8.** Stages 1–3 pass; 4 onward are blocked BY NAME on the shipped
  artifact's all-zero tokenizer commitment, and need the re-bound artifact.
* **The fourth family `PALW-QWEN25-A16-V5`.** Fusion replaces four kernels with one, so no profile
  reaches both kernel sets and a union family cannot be drilled; the family's kernel ids are read
  off the fused fixture's own profile. Until it merges, the row's coverage gate is red by an
  equality naming family `PALW-QWEN25-A16` / kernel `09b81d17ed5a73ef`, and the row would register
  weightless.
* **Two fixers were in flight when this section was written** (2026-09-04, 18:35 JST): the genesis
  registration BUILDER that mints the row's `ClassRegistered` with its admission carriage, and the
  remaining `2^22` sites on the base0 seat, replay, drill and tool paths.
* **U-09 — the dissection responder does not exist in any shipped binary.** `CourtAttnRootClaimed`
  is constructed nowhere outside `palw_state_v2`'s test module; `kaspad`'s panel has no arm that
  builds one. Decision 2's clock at a fused `Terminal` therefore convicts every accused dense-tier
  producer by silence, honest or guilty, for the price of one bond and a 42-DAA wait — and the
  opening-rung mercy beside it cannot apply, because a converged ladder has `round() > 0` by
  construction. Recorded as O-8 by the 2026-09-05 mainnet audit as "a passive liveness gap waiting
  on a feature", and re-found by the 2026-09-06 pass as a funded, steerable, evidence-free attack
  that a card ARMS (C-2/H-5). Mitigated, not closed, behind `Params::palw_court_responder_coverage`
  (`None` on testnet-11 and devnet, so both remain exposed): past that fence the fused terminal's
  silence ends the session without convicting or fining anyone. **The fence is a placeholder for
  this item and is retired by activation the day the responder ships**;
  `no_shipped_binary_files_a_court_attn_root_claimed` is the pin that goes red on that day.
