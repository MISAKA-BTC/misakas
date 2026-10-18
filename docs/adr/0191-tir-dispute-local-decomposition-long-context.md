# ADR-0191 — TIR court: dispute-local decomposition for long-context claims (Proposed)

- **Decision:** Proposed (2026-10-18). No fence is armed, no constant is pinned, no shipped rule changed. Step B (designated answers
  and tenure split) is implemented DORMANT behind a sibling fence in the same change; the bounded-round path of ADR-0190 is untouched.
- **Number:** 0191 (next free after 0190 in `docs/maintenance/number-registry.json`).
- **Scope:** every legal TIR job up to ctx 2,097,152 (graph-v7@2097152), not one 32K class. The 3,120 DAA door bound is kept, not raised.
  No class is excluded; 32K classes keep their IDs and semantics.
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0190](0190-tir-door-sparse-fetch-designated-answers-tenure-split.md),
  [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md). Closure bound over 32K..2M: lane on `release/t12-16000`
  (`closure-bound-1018`); its numbers are to be substituted into the cone-free terms below when it lands.
- **Principles:** no cryptographic proof of computation is used (PRINCIPLES §8). A hash, a checkpoint commitment or a Merkle opening
  identifies data and authenticates it; it is never treated as evidence that the computation was correct. Correctness is established
  by a deterministic recomputation of ONE dispute, and by the public evidence that localises it.

## Problem

ADR-0190 gave the door per claim, and the serial structure of the court makes it grow with the run count: the cone term is
`cone_runs × 2,346 DAA` (448 serial turns at the 32K cap). Its bounded-round variant removes the run count from the rounds but
still ships the whole read set in one body. The read set grows with the context (the attention history, the GDN state, the
operand closure), so the transport term grows with ctx: at 32K the derived window is 4,915,476 leaves = 40 GB (ADR-0190), and
no constant of the round structure can fix a term that is linear in bytes.

A door that must stay under 3,120 for every legal job therefore needs (a) a number of dependent rounds that is logarithmic in the
context, and (b) a per-round body that is bounded by a constant independent of the context, not by the claim.

## Mechanism

### (1) Locate the dispute in logarithmic dependent rounds

The owner and the challenged party hold committed positions. Every committed state (checkpoint, history tile, GDN block, logits tile)
is a leaf of a fixed-arity commitment tree over the claim's program. The dispute is localised by **b-ary bisection** (b = 16) over:

- positions of the claim (`N` positions): `R_pos = ceil(log_b N)`;
- history tiles of the attention reduction (`N / h` tiles, `h` = history tile size): `R_attn = ceil(log_b (N/h))`;
- the operations of one step (at most 2^10 leaves, the step-tree depth `PALW_TIR_STEP_NODE_DEPTH_V1`): `R_op = ceil(log_b 1024)`;
- the vocabulary tiles of the output (`V / 4,096` lanes, `PALW_LOGITS_TILE_LANES`): `R_tile = ceil(log_b (V/4096))`.

Each bisection round sends the `b-1` sibling digests of one level (a constant body, about 1 KB). The number of rounds is the sum of
the four terms and is independent of the run count. The checkpoint interval `C` does not add a round: the per-block commitments
published at every position replace the replay from the last checkpoint (see (2)).

### (2) Adjudicate ONE dispute by a small deterministic recomputation

After localisation the disputed operation has operand blocks of constant size, and the adjudication is one recomputation of that
operation from them:

- **Attention** is a reduction over history tiles. Bisection stops at one tile of `h` positions; the tile's partial softmax
  (running max, sum, weighted value) is checked against the committed partials of its neighbours. Body: `h × kv_row` bytes.
- **GDN state** is a reduction over state rows. The state at position `p` is the committed block at `p-1` plus one bounded update
  (the token's own k, v, decay, beta); the output `o = Sᵀq` is a reduction over row blocks, bisected to one block of `b_s × d_v`.
  Replay from a checkpoint is NOT used: the commitment at every position makes the state an input of one update, so `C` is absent
  from the transport.
- **Matmul** is decomposed into row and column blocks of the operand; one block of each operand is the body.
- **Gather** (embedding rows, index-selected KV rows) reads one row; **Select / TopK** branch on a value that is inside the adjudicated
  block, so the value-dependent follow-up is the second round of (1), not an extra term.
- **Output / stop rule** checks its own tile (`4,096` lanes × 2 bytes).

The adjudication body is therefore `B_adj = h·kv_row + b_s·d_v·(state elem) + (matmul blocks) + leaf + logits tile`, a constant that
depends on the model shape and the tile sizes, not on ctx. Only one dispute is adjudicated at a time per claim; the others queue.

### (3) Sparse fetch, batching, designated custodians

- Only the blocks named by the current round are requested. Independent requests of the same round are batched into one carrier.
- Answers are **designated**: each request has a designated custodian, obligation, deadline, handover order and default in consensus
  state (`palw_tir_designated_answers_v1`, Step B below). Non-designated silence is never punished. All designated silent →
  finite DA default and refusal of reward/credit at a finite deadline.
- Authenticated bodies are reused only for the same claim, root and coordinates. Signers of a false Valid stay liable even when
  they did not carry the body or were handed over.

### (4) Reorg recovery

A reorg replays only the adjudication round and the rounds whose commitments are no longer on the chain. Authenticated material of
the bisection (digests, opened paths) that is still valid under the current root is reused, so the restart term is one adjudication,
not the whole path (ADR-0190 priced it at the widest span, which is replaced here by the adjudication's own span).

## Door formula

Notation: `span(B)` is the turn span of one request whose answer body is `B` bytes (`tir_public_turn_span_v1`). Its slope is taken from
the two measured one-round points of ADR-0190 (97 leaves → 907 DAA, 256 leaves → 2,313 DAA), i.e. `span(B) ≈ 44 + ceil(B / 920)`.
This is a fit and a declared assumption; it must be re-measured on the shipped binary before any arming.

```text
Door(N, h, V, b, B_bis, B_adj, B_out, M) =
      prep                                   (legacy preparation door, counted, not re-derived here)
    + owner_queue                            (1,536 at 32K, legacy; tenure split may reduce it, not derived)
    + (R_pos + R_attn + R_op + R_tile) · span(B_bis)          bisection rounds, each carrying b-1 digests
    + span(M · B_adj)                        adjudication (one per dispute; M concurrent claims share the carrier)
    + span(B_out)                            output / stop rule (one tile)
    + span(B_adj)                            reorg restart: adjudication only (bisection reused)
    + uncovered_terms                        (none left in the operand closure by construction; see Unmet)
```

`B_bis = 1,024`, `B_adj = h·kv_row + b_s·d_v·2 + matmul blocks + leaf + logits tile`. For the declared shape of the Huihui-2B class
(`kv_row = 4,096` B, GDN block `16 × 128 × 2 B` per head at 16 heads = 65,536 B, matmul blocks 131,072 B, leaf 8,192 B, logits tile
8,192 B, `h = 16`, `V = 248,320`, `b = 16`, `M = 1`) the adjudication body is `B_adj = 278,528 B`.

| term | 32K (N = 32,768) | 2M (N = 2,097,152) |
|---|---:|---:|
| rounds R_pos, R_attn, R_op, R_tile | 4, 3, 3, 2 | 6, 5, 3, 2 |
| bisection = rounds × span(1,024) = rounds × 46 | 552 | 736 |
| adjudication span(278,528 B) (also the restart) | 347 | 347 |
| output span(8,192 B) | 53 | 53 |
| reorg restart (adjudication only) | 347 | 347 |
| owner queue (legacy, not reduced here) | 1,536 | 1,536 |
| **sum, excluding prep and concurrency** | **2,835** | **3,019** |
| headroom to 3,120 for prep, M > 1, and unmeasured terms | 285 | 101 |

Arithmetic from `door-arith` in this ADR; no measurement. **Superseded by the table in "Derived terms" below**, which adds prep, the M-admission and the owner queue. `C` appears only in the commitment count (`N / C` checkpoint commitments
and `N` block commitments of the claim), not in the rounds or the body. At 2M, `C = 65,536` and `N / C = 32`.

Concurrency: `M` concurrent disputes of one claim enlarge the adjudication carrier to `M · 278,528` B, i.e. about `303·M` DAA; with
the 1,536 owner queue this leaves room for `M = 1` only at 2M. **The mechanism therefore needs an admission rule that serialises
adjudications per claim** (the owner queue already admits at most `K_owner` owners; each admitted owner adjudicates one dispute).
That rule is not derived here and is listed as unmet.

## Comparison with ADR-0190

| | ADR-0190 bounded-round (2 rounds) | ADR-0191 dispute-local (log rounds) |
|---|---|---|
| serial rounds | 2 (cone), independent of runs | ceil(log_b) per dimension, 14 at 32K, 18 at 2M (incl. adjudication and output) |
| per-round body | read set of the whole cone, grows with ctx | constant (1 KB bisection; `B_adj` 278,528 B) |
| transport at 32K | 322,141 DAA (40 GB shared) | 303 DAA (adjudication) |
| 2M | not bounded (window grows linearly) | 3,019 at M = 1, excluding prep |
| dependence on C | replay from checkpoint | none in transport (per-block commitments) |
| uncovered | gather / param operand bytes | model shape constants (`kv_row`, `b_s`, matmul block); no unbounded term |

ADR-0190's 390 MB threshold no longer applies: the transport is bounded by a block, not by the claim.

## Uncovered terms and what is counted

- **Counted, not hidden:** detection (the public evidence that localises the dispute), preparation (`tir_public_preparation_door_v1`),
  signatures and control carriers (14,320 B per carrier), retention, designated-answer deadlines (handover), the owner queue.
- **Not derived:** `prep` (its value at 32K and 2M); `kv_row`, `b_s`, the matmul block and the gather rows of the shipped Huihui and
  graph-v7 programs (the constants above are declared assumptions, to be replaced by the program's own shape bound); M and its
  admission rule; the owner queue after the tenure split; the span slope (fit, not measured on the shipped binary).
- **Closure bound:** the other lane's derivation over 32K..2M supplies the operand closure of a dispute; this ADR uses the block
  bound instead, which is independent of it. If its closure shows a block larger than the one above, the door changes by
  `span(ΔB)` per adjudication and is re-counted.

## Derived terms (update, 2026-10-18; arithmetic with declared assumptions)

Each missing term is a function. The inputs marked "declared" are the parameters of the existing frame (`inclusion_margin_daa = 4`,
`acquisition_daa = 1`, `control_bytes = 14,320`, `parallel_bytes_per_daa = 125,000`, `reorg_replay_moves = 1`, `K_owner = 8`,
`PALW_TIR_FRAME_PREPARATION_DEBITS_PER_PASS_V1 = 1`). They are not measured on the shipped binary.

- **prep(maxc)** = `acq + margin · S + ceil(maxc · S · control_bytes / BPD)` with `S = (reorg + 1)·1 + 1 = 3` serial units.
  At `maxc = 6` or `8`: `1 + 12 + 3 = 16` DAA. It does not depend on ctx, C or h.
- **Concurrency**: with `M` adjudication carriers admitted per claim at once, the adjudication and the restart each cost
  `span(M · B_adj)`. Non-queue door `D(R, M) = R · span(1,024) + span(M·B_adj) + span(B_out) + span(M·B_adj)`, `R = R_pos+R_attn+R_op+R_tile`.
  - 32K (R = 12): `M = 1` → 1,299; `M = 2` → 1,905; `M = 8` → 5,537.
  - 2M (R = 16): `M = 1` → 1,483; `M = 2` → 2,089; `M = 8` → 5,721.
  - So at 2M, `M = 1` is the only admissible value under 3,120. A rule that admits one adjudication carrier per claim at a time is
    required. The waiting it imposes on the other `K−1` owners is `(K−1) · span(B_adj) = 2,429` DAA if serialised naively, which
    exceeds the budget. Admission therefore must be a queue whose wait is bounded by the owner queue term, not by stacking carriers.
    That admission rule is NOT derived; it is the open design point.
- **Owner queue after the tenure split**: the split changes the tenure from `max(grace, window / 2K)` to `grace + span(B_adj)` for an
  owner that is awaiting (it never goes idle while awaiting, and it rotates when its request completes). The queue term is still
  `ceil((q+1)/K) · tenure'` with `q = open_window · (parallel / control)`. Its value depends on `open_window` and `grace`, neither
  of which is derived. The condition it must meet at 2M with `M = 1` is `queue' ≤ 3,120 − 16 − 1,483 = 1,621`. The legacy 1,536 meets
  it with 85 DAA of headroom. Whether the split keeps it there is not shown.

### Table with all terms (2M, `M = 1`; the 32K column is the same, excluding the 32K round count)

| term | 32K | 2M |
|---|---:|---:|
| prep (derived above) | 16 | 16 |
| bisection rounds | 12 rounds × 46 = 552 | 16 × 46 = 736 |
| adjudication (`M = 1`) | 347 | 347 |
| output | 53 | 53 |
| restart (adjudication only) | 347 | 347 |
| owner queue (legacy value; condition `≤ 1,621` at 2M) | 1,536 | 1,536 |
| **total at `M = 1`** | **2,851** | **3,035** |
| headroom to 3,120 | 269 | **85** |

The headroom at 2M is 85 DAA, which is conditional: it holds only if the queue after the split stays at or below 1,621, and only
with `M = 1` enforced by an admission rule that is not yet derived. It is not a guarantee.

## Admission and the owner queue: derived (2026-10-19, supersedes the queue headroom above)

**Correction.** The table above counted the owner queue at its legacy 1,536 and the headroom of 85 DAA at 2M. That is withdrawn. Once
adjudications are serialised per claim, an owner that queues waits for every earlier holder's budget, so the queue term is not the
legacy term. The derivation below replaces it.

- **Admission (implemented, dormant, pure: `palw_tir_adjudication_slot_v1`)**: one slot per claim (`M = 1`). A holder that is silent
  past the adjudication budget is reclaimed AT its deadline and the next owner takes the slot there; a reclaimed owner re-queues at the
  tail at most once more, then is refused the slot on that claim. Budget `B_budget = span(B_adj)`.
- **Queue after the split**: position `p` (1-based) waits `(p − 1) · span(B_adj)`, worst `(K − 1) · span(B_adj)` with `K = 8`
  (`PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1`). The legacy `ceil((q+1)/K) · tenure` term is not used by this admission.
- **Door** (declared inputs as above, `prep = 16`, `R_32K = 12`, `R_2M = 16`, bisection 46 DAA per round, output 53):

  `D = prep + R · 46 + 53 + span(B_adj) (own) + span(B_adj) (restart) + (K − 1) · span(B_adj)`
  `  = prep + R · 46 + 53 + (K + 1) · span(B_adj)`

| | 32K | 2M |
|---|---:|---:|
| `B_adj = 278,528` (current block shape), `span = 347` | **3,744** | **3,928** |
| headroom to 3,120 | **−624** | **−808** |
| largest `B_adj` with `K = 8` that fits 3,120 | 214,000 B (span 277) | 195,000 B (span 256) |
| `K = 4` at `B_adj = 278,528` | — | **2,540** (fits) |

**Result: the door as declared does NOT meet 3,120 at 32K or 2M.** It fits only if (a) the adjudication block is at most about
195 KB at 2M (214 KB at 32K), or (b) at most 4 owners contend per claim (`K = 4` passes at 2M with 580 DAA headroom). Both are design
choices and are NOT adopted here. The span fit is an assumption; the conclusion depends on it.

Test evidence: `palw_tir_adjudication_slot_v1` (7 tests, M = 1, FIFO grant, duplicate refused, silent holder reclaimed at its deadline,
re-occupation capped, worst wait `(K−1)·budget`, pure in events and time).

## Final terms: block shrink under K_owner = 8 (2026-10-19)

Decision (goal authority): `K_owner = 8` is kept (fewer owners would reduce outside verifiers' access). The adjudication block is
shrunk instead by bisecting deeper. Each extra level divides every reduction block (attention tile, GDN state block, matmul block) by
16 (rounded up), adds one round of 46 DAA, and never divides the floor (one leaf + one logits tile = 16,384 B).

Door (declared fit `span(B) = 44 + ceil(B / 920)`; prep 16; output 53; `K = 8`):

`D(levels) = 16 + 53 + (R + levels)·46 + (K + 1)·span(B(levels))`, `B(levels) = 16,384 + ceil(65,536/16^L) + ceil(65,536/16^L) + ceil(131,072/16^L)`

| extra levels L | B (bytes) | span | 32K door (R = 12) | 2M door (R = 16) |
|---:|---:|---:|---:|---:|
| 0 | 278,528 | 347 | 3,744 | 3,928 |
| 1 | 32,768 | 80 | 1,387 | 1,571 |
| **2** | **17,408** | **63** | **1,280** | **1,464** |
| 3 | 16,448 | 62 | 1,317 | 1,501 |
| 7 | 16,385 | 62 | 1,501 | 1,685 |

**Minimum at L = 2 for both sizes.** At 2M: `D = 1,464 ≤ 3,120` with headroom 1,656. Dominant term at L = 2: the carrier count
`(K+1)·span = 9·63 = 567`, then the rounds `18·46 = 828`. The shortfall of the no-shrink design (808 DAA at 2M) is closed by two levels.

Implemented as a pure function (dormant, derivation only): `palw_tir_dispute_door_fit_v1` and `palw_tir_dispute_door_minimum_v1`
in `palw_tir_frame_cost_v1` (test `the_dispute_door_is_within_3120_at_32k_and_2m_at_the_minimising_level`).

**Not proven:** the span fit at block sizes near 17 KB (the fit is from two points at 0.8 MB and 2.1 MB; its intercept of 44 DAA
dominates at this size, so the result depends on it); the recomputation of the 17 KB block from authenticated operands is the
existing step machinery and is not re-derived here; no check is skipped and no hash or checkpoint is taken as proof.

## Unmet conditions

1. Shape constants of graph-v7@2097152 (kv row, state block, matmul blocks) from the program, not declared.
2. **Met by the final terms above (two extra levels, B = 17,408 B, 2M door 1,464):** the earlier shortfall at `K = 8`, `B_adj = 278,528` was 3,744 / 3,928 (see "Admission and the owner queue"). Either the block shrinks to about 195 KB at 2M or `K ≤ 4` is adopted. Both are design decisions for the user; the derived prep (16) is a declared-parameter value, to be measured.
3. Span slope re-measured on the shipped binary.
4. Adversarial tests: whole collusion + one external verifier; default on missing answers; restart and reorg identity across the fence.

## Decision requested

Adopt the log-round, block-bounded dispute as the design for the long-context door (replacing the per-claim read set of ADR-0190),
after the unmet conditions 1–3 are closed. No arming is requested by this ADR.
