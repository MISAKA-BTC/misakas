# K2 at real scale — segmented commitments, element courts, per-segment DA, prompt tiles (lane K2S, 2026-10-08)

Branch `k2/real-scale` (base `86dbc1fc2`). Goal: RFC-0011's K2 route (`palw_probabilistic_constraints_v1`, dormant) must work for real
models at their declared tasks and contexts — the measured 9B-8k and SmolVLM refusals of `coverage-p1p2-record.md` §1.2/§2.1 — **without
weakening G14**: one ordinary bond outside any Panel, public material only, bounded bytes and time, reaches a conviction or a correctly
classified default for every family.

Everything here is a **new kernel descriptor, K2-TIR-v4**, behind the dormant K2 fence. K2-TIR-v1/v2/v3 classes keep their rules, roots
and objects byte for byte (a descriptor's meaning is append-only). No `Params` field, no consensus object tag, delta, tail or root block
moves; the t12 `public_consensus_params_id` and schedule id are untouched (every new rule lives inside tag 110's bytes and the route's
existing row deltas).

## 0. What failed, and why it is structural

| Measured refusal (9B-8k, K2-TIR-v2) | Root cause | v4 fix |
|---|---|---|
| public bytes 12.6 TB > 2^40 | the gate bounds **the whole claim** (`E_pos · P + A`), as if every prosecutor read everything | bound **one prosecution** (two positions + the artifact); whole-claim material is the producer's DA obligation, reported separately |
| retained 6.99 GB > 2^32 | `CommitClaim` carries every node commitment of every position (`P · N · 64`) | the claim carries **one root per segment** (≤ 1,024 positions); position roots and node commitments are DA material |
| sessions 8,192 > 1,024 | the session bound counts every position, and a response is a whole position in one object | a prosecution needs **two** position sessions; responses are multi-part (≤ 1 MiB each); per-demander caps make sessions starvation-free |
| `carrier_fit` 2 GB worst opening (9B), 113.6 MB (SmolVLM) | `InstanceRecompute` opens a primitive's **whole** inputs and output (the 248,320 × 4,096 embedding table for a `Gather`) | **element courts**: one output element, one Merkle leaf (≤ 4,096 elements) per input dependency line |
| 262k/2M: prompt 32,767 / 262,143 ids > J5b's 4,096 inline; evidence > 2^50 | jobs carry the prompt inline; the whole-claim evidence bound | **prompt tiles** posted on chain under a prompt root; per-position bounds replace the whole-claim one |

## 1. Commitment structure

### 1.1 Tensor commitment v3 (tiled dual-root)

v2's dual-root commitment (a row tree and a column tree over the same elements, `merkle.rs`) is kept, with every **line tiled**: a leaf is
at most `T = 4,096` elements of one line.

```text
rows  = len / n            (n = last dim; rank ≤ 1: one row of len)      row leaf (r, t) = t-th T-chunk of row r
cols  = batch · n          (m = second-to-last dim)                      col leaf (c, t) = t-th T-chunk of column c = x[b, :, j]
leaf  = H(LEAF_V3; dtype, axis, line u64, tile u64, count u64, elements at width)
tree  = binary over leaves in (line, tile) order, an unpaired last node carried up (v2's rule)
C(x)  = H(TENSOR_V3; dtype, rank, dims, row_root, col_root)
```

A leaf opening (`LeafOpeningV3`) is ≤ 4,096 elements + ≤ 40 siblings + the other root, whatever the tensor's size. Params are committed
the same way (`ParamCommitmentsV1::of_v3`): a v4 class registers v3 param commitments.

### 1.2 Position, segment and claim roots

```text
node leaf     = H(POS_LEAF; occurrence u16, node u16, C(value))          one per committed node value, canonical (occurrence, node) order
position root = H(POS_ROOT; p u32, N u64, merkle(node leaves))           N = committed values per position (13,323 for the 9B)
segment leaf  = H(SEG_LEAF; p u32, position root)
segment root  = H(SEG_ROOT; index u32, first u32, end u32, merkle(segment leaves))     segments of S = 1,024 positions (last shorter)
claim root    = H(CLAIM_ROOT; positions u32, S u32, count u32, merkle(segment roots))
```

**Every committed value is in the tree, derived ones included** (a `Hist` window and its views): v4 has no "omitted" value. A window is
served only on demand (§4), never in the off-chain stream, so the public stream stays linear in the context.

### 1.3 What is on chain, what the producer keeps, and for how long

| Where | What | 9B-8k |
|---|---|---|
| `CommitSegmentedClaim` (inner tag 16, §8) | the claim, `SegmentedEvidenceV2` (header, job-input root, positions, S, claim root, suite) and the **segment roots** | 8 × 64 B + ~0.7 KB |
| the claim row (ledger) | the same: `ClaimBodyV1::Segmented { claim, evidence, segment_roots }` | ~1.3 KB |
| producer, until the claim's liability horizon ends | enough to **re-serve any position** within one court deadline: the token stream (public), every appended `Hist` row (linear: `P × hist row bytes`), the `Fixed` states at checkpoints every `C` positions (the rest recomputed), the artifact | KV rows 8 layers × 8,192 × 2 × 1,024 el.; GDN states at C = 64: 128 × 24 × 2 MB |
| off-chain public stream (producer, DA providers) | every non-derived value, every position | `E_pos = 1.54 GB` a position |

The chain cannot check a segment's internal structure at inclusion (it has no node commitments): what it checks there is O(segments) —
the segment count, contiguity, `claim root = merkle(segment roots)`, the job binding, the header and suite. A wrong tree is a public fault
like any wrong value (§3): an opening that does not authenticate is unavailable (§4), and every authenticated leaf is judged by its relation.

`EvidenceV2` drops v1's per-segment entry/exit **state roots** and the output root: continuity is by wiring (a court opens the predecessor
position's committed write), so the boundary is never a separate statement the producer could fabricate.

## 2. Localization: claim → segment → position → node → element, bounded per prosecution

```text
on chain:  segment roots ──────────────► segment i of position p (index arithmetic: i = p / S)
part 0 of position p:   position root + its path to segment root i (10 siblings)
any part:               C(value) + its path to the position root (⌈log2 N⌉ siblings) + leaves / leaf ranges of the value
court:                  one output element e of (p, s, n) + one leaf per input dependency line (§3)
```

A fresh outsider **checks positions of its choice** (the detection layer: sampling, a suspicion, or all of them). To check position `p` it
needs the committed values of `p` (its own derived windows included) and of `p − 1` (the `Fixed` states it reads and the previous windows,
§3.3), plus the artifact. It never needs the history rows of earlier positions: a window is judged against the previous window and the new
row. So **one prosecution reads two positions**, whatever the context:

| Per prosecution | Bound | 9B-8k (estimate; pinned by `k2_real_scale` at the end) |
|---|---|---|
| public bytes | `2 · M_pos + A + 2 · N · 64 + filing` | 2 × ~2.0 GB + 3.5 GB ≈ 7.5 GB |
| on-chain rounds | one demand round (both positions at once, multi-part) + one filing | 2 |
| concurrent sessions | 2 position sessions | 2 |
| largest response object | one part, ≤ `SEG_PART_BYTES_V4` = 1 MiB | 1 MiB |
| largest filing | the worst element court (§3) | ~0.3–1.2 MB |

`M_pos` (a position's material) = non-derived values + derived windows at the worst `H`. Detection of a lie in ONE position by an outsider
that checks `m` random positions is `m / P`; a whole-claim check reads `P · M_pos` (16 TB at 9B-8k) — that is the Panel's / a
well-resourced outsider's choice and was never what G14 bounds. G14 bounds what happens **after** a check finds a fault or a withholding.

## 3. Element courts (row/column-tiled exact openings)

`CourtIdV1::ElementRecompute` (= 4) replaces `InstanceRecompute` and `MatMulScalar` in v4. A fault names one output element `e` of one
committed value `(p, s, n)`; the filing carries:

* the output's node opening (C + path to the position root + position root path to the segment root) and **one leaf containing `e`** (row
  or column tree — either is a committed statement about `e`);
* per input, the source's opening (a node at `p` or `p − 1`, a param against the registered v3 commitments, a const from the program bytes,
  zeros, the public token through a prompt tile, §5) and **the leaves covering the dependency line of `e`**:

| Family / prim | Leaves per input for one output element |
|---|---|
| elementwise (Cast, Add, Sub, Mul, Div, Clamp, Log2Floor, IntExp, IntRsqrt, IntLn, Compare, Select, StateWrite), broadcasting | 1 (the leaf holding the broadcast-mapped element) |
| structure (Reshape, Transpose, Slice, Concat, Broadcast) | 1 (the mapped element); Iota none |
| Gather | 1 index leaf + 1 data leaf (the data address read from the index) |
| ReduceSum / ReduceMax / TopK along `axis` | the line along `axis`: `⌈n/T⌉` row leaves (last axis), column leaves (second-to-last), else `n` leaves |
| MatMul `Y[b,i,j]` | `⌈k/T⌉` row leaves of `X` row `i`, `⌈k/T⌉` column leaves of `W` column `j` |
| HistAppend (a window) | the new row's leaf (last slot) or the **previous position's window** leaf (slot `h + δ`, δ = 1 once the window is full) |

The court authenticates every leaf, checks the leaves cover the dependency line (two authenticated leaves of one operand that disagree on an
element are an inconsistent commitment: **convicted**), evaluates `e` with the reference semantics on the reduced operands
(`eval_primitive` on the line / the 1×k·k×1 product / a scalar — the exact-result rule, `Cast` fit, `Div` by ≥ 1, `Gather` bounds all
included), and convicts iff the committed element differs or the semantics refuse. A committed value whose header (dtype, shape) is not the
node's declared type at `H(p)` is **Malformed** from any one authenticated leaf.

**Soundness**: an honest trace has every leaf of every value equal to the reference value, so every element court dismisses. **Completeness**
(every family): if some committed leaf element is wrong, take the first wrong value in `(position, occurrence, node)` order and a wrong
element `e` of it; every input it reads is an earlier value (or a param/const/zeros/token, authenticated), so all of them are right, and the
element court on `e` convicts. Derived windows are no exception: the window at `p` is judged against the window at `p − 1` (already right) and
the new row; a view against its window. No court reads a value the producer withheld: the leaves come from served or demanded material.

**Decode**: greedy token `t` at round `r` is wrong iff some `j` has `logits[j] > logits[t]` (or `=` with `j < t`): two leaves of the logits.

**Bound**: per relation, `court bytes = Σ leaves (≤ T elements + path) + Σ node openings`, derived by the plan and checked against the
descriptor (`max_court_bytes`) and the consumer's carrier. A relation whose dependency line is too long for one carrier is refused at
registration by name (`BOUNDS_EXCEEDED court bytes`, block/node named). At 9B-8k the worst is a vocabulary-wide reduction (61 leaves) or
the FFN-down product (k = 12,288: 3 + 3 + 1 leaves); at 262k the P·V product's `k = H` is ≈ 1.3–1.8 MB: carriable only with a
history-chunked lowering (the frontend's h-chunk) — the honest refusal names it otherwise.

## 4. The DA/demand protocol per segment

* **Demand** — the existing `FileDemand { claim, stage 0, position }` on a segmented claim demands **everything committed at that
  position**: part 0 (position root + path to segment root), then every committed value (derived windows included) in canonical order,
  packed into parts of ≤ `SEG_PART_BYTES_V4` by a deterministic greedy layout of the program's shapes at `H(p)` (a value larger than a part
  is split at row-leaf boundaries, each slice authenticated by a **Merkle range proof** to its row root). Joinable; one session per position.
* **Respond** — the existing `Respond { claim, stage 0, position, bytes }` carries **one part** (`SegPartResponseV1 { part, chunks, … }`).
  Each part is classified as today (`malformed`, `wrong_root`, `fake_opening`, `partial`, `oversized`); a served part is recorded in the
  position's progress row; **nothing served is stored in the ledger** (the bytes stay in the block; courts read filings, not state).
* **Served** — all parts in → the demand closes, the position is public (no re-demand), the proof grace starts (as today).
* **Default** — a position with any part missing at its deadline is the producer's availability default (the claim is Unavailable, the
  fixed penalty, demanders paid); **a withheld segment = a withheld position of it**. Never a conviction.
* **Starvation-freedom (G14)** — a demander may hold at most `SEG_OPEN_PER_DEMANDER_V4 = 4` open position sessions per claim; there is no
  global cap, so no set of other bonds can refuse an honest prosecutor its two sessions.
* **Producer-side DA griefing (G14-R4, economics)**: the demand bonds of a served position stay RESERVED until its proof grace ends and are settled there (refunded today; G14-R4 plugs in the burn). The problem: a bond can demand positions of an honest claim and force on-chain serving
  (`M_pos` ≈ 2 GB each at 9B-8k). Proposed for G14-R4 (economics): a demand bond **burned when the position is served and the claim is not
  convicted within the grace** (refunded on conviction, moot or default), sized so `positions × bond` exceeds the producer's carriage cost;
  and the court deadline sized to the chain's object bandwidth (`parts per position / parts per DAA`). G14 does not rest on it.

## 5. Authenticated prompt tiles (behind the K2 fence)

* `PostTiledJob { job: TiledJobV1 { class, prompt_len, prompt_root, max_new_tokens, decode, nonce } }` (inner tag 17):
  `prompt_root = H(PROMPT_ROOT; len, merkle(H(PROMPT_TILE; index, ids) for tiles of 4,096 ids))`.
* `PostPromptTile { job, index, ids, siblings }` (inner tag 18), any bond: ids `< token_bound`, the tile authenticates against the root;
  the ledger keeps a bitmap of posted tiles (64 B at 2M), never the ids. **A claim on a tiled job commits only once every tile is posted**:
  the input is public on chain before any claim exists (no DA assumption for the prompt).
* Binding: `job_input_root_v2 = H(JOB_INPUT_V2; prompt_len, prompt_root, fed generated ids)` — O(generated), never O(prompt).
* Courts: the token at `p < prompt_len` is opened by a tile (16 KB + 9 siblings) in the filing; at `p ≥ prompt_len` it is the claim's.
* 262,144 ids = 64 tiles (1 MiB in 64 objects); 2,097,152 ids = 512 tiles. J5b (the IR route's canonical-prompt rule) is untouched; the K2
  route has no inline limit for tiled jobs.

## 6. New bounds in `public_prosecution_complete_v1` (v4 branch) and why G14 still holds

For a v4 plan the gate derives (all from the plan and the program, nothing declared):

| Bound | v4 meaning | ceiling |
|---|---|---|
| `max_public_bytes` | **one prosecution**: `2·M_pos + A + node lists + worst filing` | policy `max_public_bytes` (2^40) |
| `max_opening_bytes` / `max_filing_bytes` | the worst element court | descriptor `max_court_bytes`; carrier |
| `max_response_bytes` | one part (`SEG_PART_BYTES_V4` + envelope) | carrier |
| `max_localization_rounds` | 2 (one demand round, one filing) | — |
| `max_court_work` | worst element court work (elements + hashes + k MACs) | descriptor, block budget |
| `max_verifier_ram` | `A + 2·M_pos` | policy |
| `max_retained_state` | **on chain per claim**: `⌈P/S⌉·64 + evidence + progress rows` | policy (2^32); carrier (commit) |
| `max_concurrent_sessions` | **per prosecution**: 2 | policy `max_sessions_per_claim` |
| `claim_material_bytes` (new, reported) | the producer's whole-claim DA obligation `P · M_pos` | descriptor `max_claim_evidence_bytes` (v4: 2^60, a parse bound) |

G14, per criterion: (1) any bond files demands and proofs (no seat, no operator); (2) a fresh node builds the verifier from the claim row,
the class row and served/public material only; (3) every byte is authenticated against the on-chain segment roots, the class's v3 param
commitments, the program bytes or the prompt root; (4) a fault localizes to one element with ≤ 2 positions of material and one filing ≤ the
carrier; (5) withheld → default at the deadline, lie → conviction, honest → dismissal (soundness above); (6) per-demander caps and direct
proofs that are never pre-empted (unchanged); (7) the E2E runs the real path. **v4 classes register only under
OptimisticPublicVerification**: a Panel seat cannot cover a claim whose whole material is terabytes, and G14 is exactly OPV's premise.

## 7. Numbers (estimates; the test pins exact values and this table is updated with them)

| Case | v2 (measured) | v4 |
|---|---|---|
| 9B-8k retained on chain | 6.99 GB | ~1.3 KB / claim |
| 9B-8k sessions | 8,192 | 2 per prosecution |
| 9B-8k public bytes | 12.6 TB (whole claim) | ~7.5 GB per prosecution; 16 TB producer DA obligation |
| 9B-8k worst opening | 2.03 GB (`Gather` of the table) | ≤ ~1.2 MB (element court) |
| 9B-8k response | 1.54 GB in one object | ≤ 1 MiB parts, ~2,000 per position |
| SmolVLM-512 worst filing | 113.6 MB | ≤ ~0.3 MB |
| 262k / 2M claim on chain | 1.29e15 / 1.03e16 B evidence > 2^50 | 256 / 2,048 segment roots = 16 KB / 128 KB; prompt 64 / 512 tiles |

## 8. Node work and allocations

* **GAP 8 (cached ledger)**: the route state carries a non-state cache of the ledger its rows describe (`Arc`, never hashed, never in a
  delta, cleared by every row write outside the fold's flush and by every delta apply/revert); an object clones it instead of rebuilding
  (no program decode, no gate per class), and the fold diffs against the stored rows instead of re-serializing them.
* **GAP 10 (mempool)**: a `KernelRouteV1` / `KernelConstraintReceiptV1` carrier is put through the acceptance gate (fence, Active signer,
  ML-DSA-87, strict decode, OPV fence) at admission and again at the template, and refused with `PalwKernelRouteRefused` (chunks: judged at the
  completing chunk, as today).
* **GAP 6 (pipeline header)**: `getPalwKernelClaim.recordHeader` for a pipeline claim = borsh `(PipelineHeaderV1, PipelineClassV1)`.
* **Allocations requested** (all inside tag 110; no consensus tag, aux table, delta, tail or root block): inner kernel-route discriminants
  **16** `CommitSegmentedClaim`, **17** `PostTiledJob`, **18** `PostPromptTile` (15 is G14-R4's accuser seal); `ProsecutionV1` variant **3**
  `Segmented`; `ClaimBodyV1` variant **2** `Segmented`; kernel ledger tables **20** tiled jobs and **21** segmented demand progress (15–19 are G14-R4's; in the
  ledger root only when non-empty, so every existing root is unchanged); `TxRuleError::PalwKernelRouteRefused`.

## 9. What this does not do

Real 9B weights and a real 9B trace (H1); the DA-griefing economics (§4, proposed); Panel coverage of v4 claims (none: OPV only); pipelines
under v4 (K2-TIR-v3 keeps v1 commitments); 262k/2M attention courts without a history-chunked lowering; the IR route's J5b.
