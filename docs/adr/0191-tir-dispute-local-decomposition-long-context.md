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

## Unmet conditions

1. Shape constants of graph-v7@2097152 (kv row, state block, matmul blocks) from the program, not declared.
2. The M = 1 admission rule (serialising adjudications per claim without a `(K−1)·span` wait) and the owner queue after the split (`≤ 1,621`) are derived; the derived prep (16) is a declared-parameter value, to be measured.
3. Span slope re-measured on the shipped binary.
4. Adversarial tests: whole collusion + one external verifier; default on missing answers; restart and reorg identity across the fence.

## Decision requested

Adopt the log-round, block-bounded dispute as the design for the long-context door (replacing the per-claim read set of ADR-0190),
after the unmet conditions 1–3 are closed. No arming is requested by this ADR.
