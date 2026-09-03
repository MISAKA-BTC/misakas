# ADR-0082: The close is flat in the context — attention is refuted by dissection, the capture is a fold, and the answer is what earns

**Status:** PROPOSED (2026-09-03). Written against two things: the refutation of ADR-0080 and
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
is W5's. W5 carries four requirements its adversarial judges added, recorded here so they are the
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
node of every position, ~50 MB a position, ~25 GB at 512 (`fp_capture.rs` header). The fold that
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
Decision 12 was sized against: `~43,000` dense and `~14,000` hybrid positions under `2^32`.

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
SMALLEST legal arity that fits, which at the RC windows is **4** (measured by U-03's pin:
`2^32` in 16 rounds plus 131,072 positions in 7, `(2 × 23 + 2) × 45 = 2,160` DAA — sixteen is the
band above it, and a smaller arity is a smaller move):

| space | binary rounds | 16-ary rounds |
|---|---|---|
| leaf space `2^32` | 32 | 8 |
| history at 131,072 positions, 8,192 tiles | 13 | 4 |
| moves, both, plus terminal | 92 | 26 |
| DAA at the 45-DAA deadline | 4,140 — over | 1,170 |
| bytes a move: leaf space / history at a 64-lane tile | 128 / 1,048 | 1,024 / 8,384 |

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

**Decision 5 — the prompt ids ride as a Merkle root, armed with the first graph-v5 row.**
ADR-0081 Decision 3 as implemented (`palw_prompt_ids_v1.rs`), no longer optional: at 131,072 ids the
flat term is 524,288 bytes on EVERY close, and Decisions 1–4 leave it as the only context-linear
term of the PROMPT. The fence `palw_prompt_ids_merkle` is armed in the same ruleset move as the
rows, every moved id is re-pinned and listed, and `PalwFpMaterialV1` keeps carrying the ids whole
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
derivation, evaluated at genesis over the rows the genesis set registers. Stated for BOTH genesis
sets, because two of them are on the table: for the graph-v2/v3 context rows design A was written
for, the widest term is still the context-linear attention close, and the derivation returns
design A's own numbers — **14** for `{floor, A16 dense 512}` (`ceil(1,154,673 × 1.2 / 100,000)`),
**27** with the QWEN36-v3 512 row, 1 for the floor alone; for graph-v5 rows (Decisions 1–5) the
widest term is a MODEL width and the derivation returns — measured by U-04's test
`the_close_ceiling_is_the_derivation_over_the_genesis_set` at `n_ctx` 512 under the dissection
court with Merkle prompt ids — **80,504 bytes = 1 chunk** on the dense tier (binding node
`ffn_down`; 82,080 with flat ids) and **200,732 bytes = 3 chunks** on the hybrid (binding node the
recurrence `GatedDeltaNet`: its `interval × 5 refs` replay evidence, not attention), the set → 3;
the fused dispute's bottom opening is 25,120 bytes dense and 42,016 hybrid on the checkpoint-tile
route (derived), 37,982 and 55,390 on the cache-write route (measured by borsh on the real object,
34 rows with their paths), flat at 512 / 4,096 / 32,768. "Flat" as a measurement rather than an
adjective (verified independently by a second session on a detached checkout): the dense graph-v5
close is **80,504 bytes at `n_ctx` 512 and 80,696 at 4,096 — 192 bytes for eight times the
context**; at 32,768 it is 303,640 with the binding node moved to `EmbedLookup` (Decision 5's
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
* **Court moves.** `2 × (⌈log_k L⌉ + ⌈log_k (n / 16)⌉) + 2`: 26 at `k = 16`, `L = 2^32`,
  `n = 131,072`; 1,170 DAA at the 45-DAA deadline against 3,000. A dispute at 131,072 positions
  costs the court FEWER moves than a dispute at 512 costs it today.
* **The bottom opening.** `4 × d_head` (the query slice) + `2 × 16 × 4 × kv_dim` (one K tile, one V
  tile — a checkpoint chunk holds the whole cache ROW, `attn_kv_heads × attn_head_dim`, because the
  map addresses `(kind, layer, position)` and not the head; the ADR's first draft priced one head's
  slice, which under-bounds the object by `kv_heads`) + `4 × tile_len` (the output tile) + paths at
  the ladder's depth. Measured on the real objects (U-03, borsh, `kv_heads` 1, 64-position history):
  the checkpoint route 19,027 bytes at `d_head` 128 and 36,435 at 256; the cache-write route
  (one leaf per row, for the rows after the last checkpoint) 37,985 and 55,393. Inside one carrier
  at every context on both routes; the derivation prices the larger.
* **Executor time.** The forward pass plus hashing the base leaf count — `~1.3 × 10¹⁰` leaf hashes
  for a 131,072-position dense job, the same order as the forward's own work. U-01 gives the ratio.
* **Executor retention.** The KV cache for the claim's life (`claim_retirement`, 3,000 DAA on the
  RC): 7.5 GB dense, 5.4 GB hybrid attention at 131,072 (Decision 9's arithmetic); plus a
  recurrent family's sparse checkpoints at `heads × k_dim × v_dim × 4` each — 65,536 bytes a
  head — at the derived spacing.
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
| U-07 | Decision 5 — the prompt-id fence armed with the rows; every moved id re-pinned and listed | the u04 blast-radius test passes at the new pins |
| U-08 | the graph-v5 rows registered through ADR-0075's route and drilled (`misaka-palw-fp-devnet-e2e.sh`): dense and hybrid at 512, then 8,192, then the width Z11 admits | ONE job id in three places (ADR-0077 §6's gate) at each width, with a dispute drilled to the bottom tile at the widest |
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
| ADR-0080 design A (`DEFAULT_MAX_CLOSE_CHUNKS = 27`) | kept as the BRIDGE and re-derived (Decision 6): the count is `chunks(widest term over the genesis set)` — 14/27 for the graph-v2/v3 context rows, one to three for graph-v5 rows; the transport is W5's own table, the 5f integrator's |
| ADR-0081 §3 Decisions 1, 2, 4–9 — the prompt as a prefill state chain | **withdrawn**. What a long prompt needed was never a chain of prefill segments; it was a court that never carries the history (Decision 2) and a seat that recomputes it (Decision 9) |
| ADR-0081 Decision 3 — Merkle prompt ids | kept and ARMED with the rows (Decision 5) |
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
