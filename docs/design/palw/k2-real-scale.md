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
every part of position p: position root + its path to segment root i (10 siblings)
every chunk of a part:   C(value) + its path to the position root (⌈log2 N⌉ siblings) + the whole value, or a row-leaf range with its range proof
court:                  one output element e of (p, s, n) + one leaf per input dependency line (§3)
```

A fresh outsider **checks positions of its choice** (the detection layer: sampling, a suspicion, or all of them). To check position `p` it
needs the committed values of `p` (its own derived windows included) and of `p − 1` (the `Fixed` states it reads and the previous windows,
§3.3), plus the artifact. It never needs the history rows of earlier positions: a window is judged against the previous window and the new
row. So **one prosecution reads two positions**, whatever the context:

| Per prosecution | Bound | 9B-8k (measured, `k2s_huihui_qwen35_9b_8k_passes_the_real_scale_gate_and_the_carriers`) |
|---|---|---|
| public bytes | `2 · M_pos + A + 2 · N · 64 + filing` | **8,193,657,124 B** (2 × 2,341,827,961 + 3,508,121,330 + …) |
| on-chain rounds | one demand round (both positions at once, multi-part) + one filing | 2 |
| concurrent sessions | 2 position sessions | 2 |
| largest response object | one part, ≤ `SEG_PART_BYTES_V4` = 1 MiB | 1,048,576 B (2,294 parts a position) |
| largest filing | the worst element court (§3) | **170,432 B** (+ 4 KiB header) |

`M_pos` (a position's material) = every committed value, derived windows included, at the worst `H` the claim can reach. Detection of a lie
in ONE position by an outsider that checks `m` random positions is `m / P`; a whole-claim check reads `P · M_pos` (19.2 TB at 9B-8k) — that is the Panel's / a
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

**A producer duty the soundness argument rests on.** "An honest trace has every leaf equal to the reference value" assumes the class's v3
param commitments are commitments of the public artifact. Registration does not check that: the registrant supplies digests. If a
class's commitment is not reproducible from the artifact (for example its row and column trees disagree), no producer can make a claim
that survives: the prover opens whichever tree covers the dependency line, and two disagreeing param leaves convict as an inconsistent
operand. So a producer recomputes the class's v3 param commitments from the artifact bytes before it produces for the class. A class
that fails that check is a trap, not a class. The class-level remedy is the artifact binding (onboarding; GAP 1 of the readiness
matrix), which would let anyone refute such a class from two disagreeing openings.

**Decode**: greedy token `t` at round `r` is wrong iff some `j` has `logits[j] > logits[t]` (or `=` with `j < t`): two leaves of the logits.

**Bound**: per relation, `court bytes = Σ leaves (≤ T elements + path) + Σ node openings`, priced by running the court's own evaluator over
zero operands at the first and the last element at `H = min(window, max_positions)` (`element_court_cost_v1`), carried in the plan's
`worst_court_bytes`, checked against the descriptor (`max_court_bytes`, 16 MiB: `BOUNDS_EXCEEDED court bytes` at `check_plan`) and, on the
node, against the carrier (`carrier_fit_v1` at registration). Measured at 9B-8k: **170,432 B** (block 3, node 41); the 2 GB `Gather` court of
v2 opens one index leaf and one 4,096-element table leaf. A history product whose `k = H` is very long (an attention `P·V` over a 2^18
window: ≈ 1.7 MB) prices past the carrier and the class is refused by the carrier fit: such a context needs a history-chunked lowering.

### 3.1 Finding (2026-10-10): the price must be the bytes a prover files

The K2-TIR-v5 ledger test filed, for every honest element of a tiny BERT encoder, the prover's filing and compared it with the court's
price. The first one failed: 2,673 B filed against a price of 2,592 B (`hf-enc/bert`, the embedding `Gather`, element 0). The cause is
general, not the encoder's:

* **The leaf's wire form.** `LeafOpeningV3` derived borsh, so its values went on the wire as `i128`, 16 bytes an element, while every
  price (`byte_len`, `element_court_cost_v1`) counted them at the dtype's width. Params are at most 8 bytes wide and committed values
  at most 4, so a court of long operand lines filed at least 2× its price: 4× for committed lines, 16× for `i8` lines (derived). The gate holds the price against the carrier. At the
  FFN frontier of §12 the price was 1,579,240 B against the carrier's 1,583,616 B, so the real filing would have been several times the
  carrier: a lie the gate called prosecutable could not have been filed. **Fixed**: the leaf's wire form carries its values at the
  dtype's width, little endian, exactly as the leaf hash reads them (`merkle3.rs`). A value outside its dtype, or an unknown dtype, has no
  wire form (serializing it is an error, never a truncation), and a count the bytes do not carry is refused before it is allocated. The
  encoded length is `94 + 8·rank + count·width + 64·siblings`, two bytes under `byte_len`.
* **What the price covered.** It is now an upper bound of every filing, by construction:
  - the prover and the price choose a leaf's tree by one rule, the one whose covering leaves file fewer bytes (`cheaper_cover`); before,
    the prover chose by leaf count, the price by bytes, and the output leaf was always a row leaf;
  - every operand's opening is priced whether or not the element reads it: the node opening of a committed operand, and the 5-byte
    shell of a const, zeros, a public scalar or a job input. Before, an operand the element did not read cost nothing, though its node
    opening is filed;
  - a `Concat` prices a leaf of every input, since an element reads one of them and the first and last elements read only the first and
    last input;
  - the per-position token's tile is priced at the tile tree's real depth for `max_positions`, the job tile of a K2-TIR-v5 class at its
    `L` ids, and a K2-TIR-v5 count at its shell;
  - **the decode court is priced** (`decode_court_cost_v1`: the logits' node opening and two full row leaves), and the plan's worst court
    is the maximum over the element courts and the decode court. Before, a decode filing was never priced.

Tested: `element::tests::leaf_openings_file_their_values_at_the_dtype_width` (the wire form of every dtype, both trees, round trip,
refusals), `element::tests::every_filing_is_within_its_priced_bound` (dense MoE: the first, a middle and the last element of every
value at the first and last positions, tiled prompt, and a decode filing), and the K2-TIR-v5 ledger test (every honest element of the
two tiny encoders, and each relation's price within the registered plan's worst court).

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

### 4.1 Court scope under ADR-0177 D2: claim-specific demands, the model from the verifier's own copy

ADR-0177 D2 limits a court's demands to the claim's own evidence (state, trace, output, witness). A demand must never ask the producer
for weight rows or file ranges, and repeated demands must never add up to the model. A model operand the court needs comes from the
verifier's own copy, authenticated against the registered root. Every K2-TIR-v4/v5 unit, against that rule:

| Unit | What it opens | From whom | D2 |
|---|---|---|---|
| `FileDemand { claim, stage 0, position }` and its `Respond` parts | the claim's committed values at one position (`ChunkV1`: node values, with their node and position paths), derived windows included | the producer | claim-specific: no part carries a param |
| element court (`ElementFaultV1`), param operand | ≤ 1 leaf (≤ 4,096 elements of one row or column) per dependency line, authenticated against the class's registered v3 param commitments | **the accuser's own copy** (`artifact` in `check_positions_v1`, `prove_element_v1`) | no demand; see the flag below |
| element court, committed operand / output | leaves of committed values, with node openings | served or published claim material | claim-specific |
| element court, token or K2-TIR-v5 ids | a prompt tile authenticated against the job's prompt root | the chain (posted tiles) | the job's public input |
| decode court | ≤ 2 leaves of the committed logits | claim material | claim-specific |
| route B (`seg_detect`) | position paths of the first divergent segment, then two positions | claim material; the verifier re-executes with its own artifact | claim-specific |

**No K2-TIR-v4/v5 unit demands model bytes from the producer.** Nothing in this lane needs to be restricted on the demand side.

**Flag for DA16's court-scope inventory.** An element court's filing puts param leaves **on chain**: the accuser supplies them from its
own copy, because the court has no copy. A filing is not a request, so D2's ban does not reach it. Still, every filing publishes up to the
class's worst court in model bytes, and that includes an honest element filed only to be dismissed, at the dismissed-proof fee. Repeated
filings could therefore publish a closed model's weights cumulatively. K2 does not bound that today. It belongs in DA16's central
predicate as a cumulative disclosure scope. When DA16 lands the predicate, K2 will call it at `FileProof` acceptance for a segmented
claim. (The predicate is not on its branch yet.)

**A second flag: the gate's `weights_public` input.** The v4 gate takes `ProfileMaterialV1::kernel_route(weights_public)`, and the route
passes `true` for a class whose artifact root is attested (an onboarding binding that is Matured or Final). Under ADR-0177 that flag can
only mean that the registered root is bound. It cannot mean the bytes are obtainable, because the chain guarantees no acquisition (D1).
The gate's G14 verdict is therefore **conditional on the verifier holding the model** (D7). The Lead decides whether to rename the
input, or to have the read API state the condition next to each G14-complete class.

### 4.2 Under DA16b's court scope (`palw_court_scope_v1`, `misaka_palw_kernel::scope`): hiding leaves — DESIGN, staged

DA16b's §7.8–§7.9 bind K2-TIR-v4/v5 to two rules. **Level 1:** a `ModelCopy` or `ModelOnly` value (an embedding row gathered by the
token, a transposed weight) is never owed in the clear. **The clear budget:** `court_clear_position_budget_v1` is 0 for every class
with an elementwise model relation (every LM: a norm gain is revealed by one position), so in practice every model-dependent value is
owed only as commitment structure, as leaf hashes of 64 elements (`COURT_HIDING_LEAF_ELEMENTS_V1`). The court then recomputes ONE leaf
and compares its hash.

What K2-TIR-v4/v5 does today, against that:
- **Done at this merge.** A segmented position is a `KernelPosition` unit (`palw_kernel_demand_unit_v1`), so the fold's admission and
  the per-requester tally (≤ 16 units) cover segmented demands. One prosecution needs 2 positions, plus ≤ 10 probes under withholding.
- **Not done: a served position still carries every value in the clear.** That includes the model-dependent values, and an embedding
  row is a `ModelCopy`.

The change, in order. Each step is one milestone, and none of them touches a live id:
1. **Leaf width.** A masked value's v3 row leaves become 64 elements wide (`TILE_V3` is 4,096 today; v3 is not armed). A line shorter
   than 64 elements (wide128's `d` = 16) cannot fill a 64-element leaf, and a short leaf brute-forces. Such a value needs a flat-tiled
   leaf layout for masked values: 64-element tiles of the row-major order, with the ragged tail merged into the previous tile. This is
   **DESIGN, the Lead's choice**: either the flat layout (a new leaf rule inside v3), or "masked values must have lines of at least 64
   elements" (a registration refusal).
2. **DA.** `seg_da::{declared_values, position_parts_v1, classify_part_v1, assemble_position_v1}` drop the masked values' elements and
   owe their row-leaf hashes and column root instead (a new `ChunkBodyV1` variant).
3. **The court.** An `ElementFaultV1` whose output is masked carries the output leaf's HASH and path, not its elements. The court
   recomputes all ≤ 64 elements of that leaf from the operands. Masked operands are supplied by the filer from its own re-execution;
   they authenticate only if they equal the producer's, because the leaf hashes must match. Params come from the filer's copy. The court
   convicts iff the recomputed leaf's hash differs. The price becomes the union of the leaf's dependency lines (a `MatMul` output leaf:
   one `X` row and 64 `W` columns).
4. **The verifier.** Route A (spot-checking a position from its clear values) is lost for masked values; DA16b accepts that. Route B
   (re-execute, compare roots, descend to the first divergent position, then to the first divergent node commitment, then to the
   first divergent leaf hash) is the detector. It already holds every value it needs.

Until then, a K2-TIR-v4/v5 class must not be reward-bearing on the ADR-0177 D2 ground as well: a demand of a v4 position compels
model-dependent and model-copy values in the clear.

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

## 7. Numbers (measured; shape level — real weights are H1's)

9B-8k from the shipped fixture (`fixtures/g14/shipped/huihui-qwen3.5-9b-8k.json`, the same program lane D registers), 838 relations,
13,323 committed values a position (`k2s_huihui_qwen35_9b_8k_passes_the_real_scale_gate_and_the_carriers`):

| 9B-8k | K2-TIR-v2 (whole-claim gate) | K2-TIR-v4 |
|---|---|---|
| `check_plan_v1` | PASS (ε ≤ 2^-150) | PASS (ε ≤ 2^-150) |
| public bytes | 12,597,678,100,210 > 2^40 — REFUSED | **8,193,657,124** per prosecution — PASS |
| retained state | 6,985,614,336 > 2^32 — REFUSED | **4,608 B** on chain per claim (8 segment roots + fixed) — PASS |
| concurrent sessions | 8,192 > 1,024 — REFUSED | **2** per prosecution — PASS |
| worst opening / filing | 2,034,245,636 B | **170,432 B** / 174,528 B — PASS |
| response | 1.54 GB in one object | **1,048,576 B** parts (2,294 a position) |
| `carrier_fit_v1` at 1,583,616 B | REFUSED | **PASS** (filing, response, commitment) |
| position material / claim material | — | 2,341,827,961 B / 19,184,254,656,512 B (the producer's DA obligation) |
| verifier RAM | — | 8,191,777,252 B |

Beyond 8k (informational, same test): the fixture's program declares an 8,192-position window, so at **262,144** positions the gate
passes with the same per-prosecution bytes and court (the window slides), 256 segment roots on chain (16 KB) and 613,896,149,008,384 B of
claim material; this is the class's declared window, not full-context attention. At **2,097,152** `check_plan_v1` refuses
`BOUNDS_EXCEEDED positions 2,097,152 > 262,144` (the program's history bound) — the 2M row of `coverage-p1p2-record.md` §1.2(c) stands.
Prompts: 262,144 ids = 64 tiles, 2,097,152 = 512 tiles, each ≤ 64 KiB on chain.

SmolVLM text class: see §10 (the SDK probe's lowering of the committed headers).

Real node (wide128 under v4, `real_scale.rs`): a lie at position 1,200 is filed in **3,359 B** (class bound 22,760) after the outsider read
**2,208 B** of material — exactly positions 1,199 and 1,200 — against a per-prosecution bound of 29,448 B.

## 8. Node work, the real-node E2E, and allocations

**The real-node E2E** (`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/real_scale.rs`; the class is the sketch's
wide128 layer under K2-TIR-v4 at 8,192 positions, registered under OPV through the mempool, the template and the fold; the outsider is
built from the read API's segmented record and public material only):

| Test | What it shows |
|---|---|
| `g14_k2s_a_tiled_prompt_past_4096_ids_commits_as_a_multi_segment_claim_on_the_real_node` | a 4,500-id prompt posted in two tiles; a 4,501-position claim in five segments with a row under 4,416 B; one position of every segment checks clean; an honest element filed anyway is dismissed (no conviction, no slash); Final at the window's end with no Panel; a replaying node agrees |
| `g14_k2s_a_lie_in_one_segment_is_localized_and_convicted_with_bounded_bytes` | the lie at position 1,200 is found by checking position 1,200, filed in 3,359 B after reading exactly positions 1,199 and 1,200; convicted, the real bond slashed |
| `g14_k2s_a_withheld_segment_is_demanded_and_defaults_never_a_conviction` | the check demands exactly the two positions it reads; a wrong-root response is classified; at the deadline the claim is Unavailable, the default charged, never a conviction, the demand bonds returned |
| `g14_k2s_a_demanded_position_served_on_chain_is_checked_from_the_blocks_and_convicted` | nothing published off-chain; both positions demanded, served on chain in parts, both demands closed; the outsider reads the `Respond` parts back from the blocks (the ledger keeps none of their bytes), assembles the positions and convicts (filing 3,359 B after reading 2,208 B back from the blocks) |
| `g14_k2s_the_mempool_runs_the_kernel_acceptance_gate` | GAP 10 below |
| `g14_k2s_the_producer_and_every_other_bond_collude_and_one_outside_bond_convicts_through_the_real_path` | **G14's own statement.** Every bond but the last genesis card colludes. Each signs a passing receipt for the lying claim; the mempool refuses, by name, those of bonds the route never saw, and the rest are mined and folded with no effect, since an OPV claim has no seats and no receipt counts. Nobody publishes, nobody prosecutes. The outsider, a bond the route has never seen, demands positions 1,199 and 1,200, reads the served parts back from the blocks, files one element court within the class's priced bound, and the claim is convicted. The slash splits exactly into the accuser's reward and the burn, and the next coinbase pays the reward. A second lie is withheld: Unavailable at the deadline, never a conviction, never Final. A third lie is in the output (a delivered id that is not the decode of honest logits): the decode court convicts it from the positions the producer had to serve (GAP-30's decode lie). A replaying node agrees |

G14C's matrix (GAP-30, GAP-31) asks for the remaining lie types, filers and recovery on the node. They are in the same module:

| Test | GAP | What it shows |
|---|---|---|
| `g14_k2s_input_borrowed_and_garbage_traces_are_convicted_on_the_node` | 30 | an **input** lie (the producer computes position 4,200 over an id that is not the posted tile's; the embedding court opens tile 1 against the prompt root), a **borrowed** trace (another prompt's honest trace; tile 0) and a **garbage** trace (in-range noise), each found from one position and convicted |
| `g14_k2s_a_lie_is_convicted_after_final_within_the_liability_horizon` | 30 | Final with no Panel and no prosecution, then a **post-Final** conviction inside the liability horizon; the reservation slashed |
| `g14_k2s_spam_demands_never_preempt_a_proof_and_simultaneous_filers_get_one_conviction` | 30 (C6) | the coalition holds every session its bonds may open; a direct proof is not pre-empted; two outsiders in ONE block → one conviction, one duplicate, one slash, one reward (slash = reward + burn); the spam settles moot and its bonds return; a later duplicate changes nothing |
| `g14_k2s_a_reorg_across_live_tiled_job_and_progress_rows_restores_them_exactly` | 30 (C8) | a reorg across a live tiled job (table 20) and open demands (table 21): back to the fork's rows exactly, the slash and reward undone; forward again, identical |
| `g14_k2s_survives_a_node_restart_with_live_tiled_job_and_progress_rows` | 30 (C8) | a restart over the same database with a half-served demand: tables 20/21 off disk, service and conviction after the restart, replay agrees |
| `g14_k2s_survives_a_pruned_import_with_live_tiled_job_and_progress_rows` | 30 (C8) | a pruned join at P with tables 20/21 live: the carriage (tail 0xEC) installs them, and the importer folds the service and conviction to A's roots |
| `g14_k2s_v5_an_encoder_class_on_the_node_convicts_its_lies_and_finalizes_an_honest_claim` | 40 | a **K2-TIR-v5** class (a real tiny BERT encoder with its weights, lowered by `lower::bidir`) registered under OPV on the node; tiled jobs that generate nothing; a lie in the embedding lookup (the court opens the job's prompt tile) and a lie in the result, each demanded (position 0), served on chain, read back and convicted; an honest claim's element dismissed, Final with no Panel |
| `g14_k2s_a_history_bearing_class_held_at_8192_positions_convicts_a_continuity_lie_across_segments` | 31 | a **history-bearing** class (rotary attention over `Hist` windows, router, experts) at **8,192 positions** on the node, committed by a streaming producer (`trace_streaming_v1`); a **continuity** lie in the k window at position 4,096 (segment 4), whose previous window is in segment 3, demanded, served on chain, read from the blocks and convicted |

**GAP 8 — a cached ledger instead of a rebuild per object.** The route state carries a non-state cache of the ledger its rows describe
(`Arc`, never hashed, never encoded, never in a delta, equal for every comparison). The fold's flush sets it; every other write of a
ledger row or of the header clears it (a delta applied or reverted, a row written outside the flush); a state decoded from a store, a
snapshot or a carriage starts without it. A load clones the cached ledger instead of rebuilding it: no row decode, no program decode, no
gate per class. A debug build checks the cache against the rows on every hit, so a write path that forgets to clear it fails the tests
(lane D's whole `g14_kernel_route_e2e` suite ran with it). What is left: the block's first load rebuilds from the rows (the fold starts
every block from the decoded tip), and that is where a big class costs — a 9B-8k class row rebuilds in 1.7 ms (the program decode, 32,640 B) plus 11.3 ms (the v4 gate), debug build (§8.1); and every object still
clones the ledger, serializes it to rows and diffs them (O(rows) bytes, no decode). Removing those needs either a cache across blocks
keyed by the route's rows root or dirty-row tracking in the kernel ledger. G14 does not need either; recorded as the residual.

**GAP 10 — the mempool and the template run the acceptance gate.** A `KernelRouteV1` carrier is put through the fold's own
acceptance checks at admission and again at every template: the fence, an Active signer, ML-DSA-87 under the signer's registered key,
the kernel's strict canonical decode, and the OPV fence for a mode registration. A `KernelConstraintReceiptV1` is checked against an
assigned, Active seat's key. A refusal is `TxRuleError::PalwKernelRouteRefused(why)`. It is a node policy, never a block rule: a block
carrying such a carrier folds exactly as before (the fold drops the same objects). Below the fence, which is every live height, a
tag-110 carrier is refused at admission by the node ("not in force"); a block from another node that carries one is unchanged. Chunks
are judged at the completing chunk, as before. Tested: a forged signature (refused by name), bytes changed under their signature, the
same non-canonical bytes **genuinely signed** (the signature verifies; the strict decode refuses them by name), and the genuine carrier
admitted and folded.

**GAP 6 — the pipeline header's wire form.** `getPalwKernelClaim.recordHeader` for a pipeline claim is borsh
`(PipelineHeaderV1, PipelineClassV1)`: the header a pipeline verifier is built with, and the class binding it checks the record against.
With the record, that is exactly the triple the kernel's own outsider builds `FreshPipelineVerifierV1::from_public_bytes_in_mode` from
(`ledger.rs`, `OutsiderV1::check_pipeline`), and the verifier itself checks that the header's class is the binding's under the mode.
Tested: the wire form round-trips (consensus-core). No pipeline claim runs on the real-node E2E: the toy pipeline fixture lives in
`misaka-palw-tir`'s tests and needs `misaka-palw-gen`; the kernel's OPV pipeline tests cover the verifier. Recorded as the residual.

**Allocations** (granted in the lane brief; all inside tag 110 — no consensus tag, aux table, delta, tail or root block): inner
kernel-route discriminants **16** `CommitSegmentedClaim`, **17** `PostTiledJob`, **18** `PostPromptTile` (15 is G14-R4's accuser seal);
`ProsecutionV1` variant **3** `Segmented`; `ClaimBodyV1` variant **2** `Segmented`; kernel ledger tables **20** tiled jobs and **21**
segmented demand progress (15–19 are G14-R4's; in the ledger root only when non-empty, so every existing root is unchanged). Not in the
brief, so listed for the Lead: `TxRuleError::PalwKernelRouteRefused` (a node error, no wire form; next to `PalwH1CarrierRefused`). The new
kinds ride the K2 fence `palw_probabilistic_constraints_v1` with the rest of tag 110 (A2U's kind→fence table).

### 8.1 Finding: the position layout was the fold's cost at real scale

The first 9B-8k measurement of the class-row rebuild was 1.7 ms for the program decode and **14.9 s for the v4 gate** (debug build).
Almost all of the 14.9 s was `seg_da::position_parts_v1`, the deterministic layout of one position's material into parts. It built
every row leaf's element list only to count its bytes, at 2.34 GB of position material. The layout does not run only at registration.
It runs at every block's first ledger load (once per v4 class), at every `FileDemand` and at every `Respond` part (`classify_part_v1`
recomputes it). A served 9B-8k position is 2,294 parts, so the fold would have spent seconds per part, against a per-block budget that
charges a part zero work.

**Fixed.** The layout now counts a row's leaves arithmetically: every leaf of a row is a full tile but its last, so whole rows are
taken at once and only the ragged ends are visited (`row_leaves_fitting`). The layout is byte-for-byte the old one
(`seg_da::tests::the_fast_layout_is_the_reference_layout` compares it, part for part, with the element-counting version). It also
refuses, by name, a value whose single row leaf and overhead exceed a part, where the old loop would never end (unreachable while TIR
caps the rank at 4, but no longer a hang). Re-measured on the 9B-8k fixture (debug): the v4 gate takes **11.3 ms** (was 14.9 s), with the
same 2,294 parts and the same bounds.

### 8.2 Rewards and weight: the ADR-0176 hook (named, not built)

A K2-TIR-v4/v5 claim can give its producer exactly two things: the Final reward (`LedgerPolicyV1::claim_reward`, settled as
`SettlementKindV1::FinalReward` in the ledger tick's `Final` transition) and the OPV work credit the consumer releases at Final
(`OpvEconomicsV1::work_credit_per_claim`). Both are INTERIM and dormant. Under ADR-0176 they draw from the producer bond's Q/B/R/F
budget of lane BUDGET (`palw_bond_budget_v1`). The hook is:

| Point | Call |
|---|---|
| `CommitSegmentedClaim` accepted (`seg_ledger.rs`, at `d` = the claim's DAA) | reserve `Q` = 1 claim and the claim's maximum `R` (its Final reward) and `F` (its work credit) against the producer bond over the window `W`. A refusal refuses the claim (`B` = 0: a kernel-route claim mints no block) |
| the tick's `Final` transition (`ledger.rs`, `FinalReward`) and the consumer's work-credit release | re-check the reservation and consume it; pay nothing that was not reserved |
| conviction, Unavailable, TimedOut, void | the reservation is spent, not returned: `reuse_not_before = d + W` (no early recovery) |
| the accuser's reward | not issuance: a share of the producer's slash. Outside Q/B/R/F. The route's INTERIM policy (`palw_kernel_route_policy_v1`) sets `accuser_reward_permille` = 500, so a self-reporting producer recovers 50% and loses a net 50% of the collected slash. That is above ADR-0032's 4,900 bps (the R-core reporter share, behind `palw_reporter_share_v2`); whether the kernel route adopts 4,900 bps is the Lead's decision |

Until BUDGET's engine exists, every reward or weight fence on this route stays refused. Nothing here is armed.

## 9. What this does not do

Real 9B weights and a real 9B trace (H1); the DA-griefing economics (§4, proposed); Panel coverage of v4 claims (none: OPV only);
pipelines under v4 (K2-TIR-v3 keeps v1 commitments); 262k/2M attention courts without a history-chunked lowering; the IR route's J5b;
the residuals of GAP 8 and GAP 6 (§8); and the parts of detection §11 lists as POLICY, CODE elsewhere, or DESIGN_GAP.

## 10. SmolVLM-256M's text class under K2-TIR-v4

The SDK probe (`misaka-palw-sdk/tests/coverage_p1_huihui_9b.rs`, `COV_P1_K2=1 COV_P1_K2_ONLY=1`) over the committed header-only
fixture of SmolVLM-256M-Instruct (`misaka-palw-tir-lower/tests/fixtures/vlm-generic/smolvlm-256m-instruct`, revision `7e3e67e…`). The
class is the text decoder only, a partial-task class (`coverage-p1p2-record.md` §2.1), with 8,031 committed values a position:

| SmolVLM-256M text class | K2-TIR-v1 / v2 | K2-TIR-v4 |
|---|---|---|
| @512: `check_plan_v1` (armed) | PASS | PASS (ε ≤ 2^-156) |
| @512: gate | PASS (public 24,240,078,625 B, 512 sessions) | PASS: **258,871,987 B** per prosecution, 2 sessions, 4,160 B on chain |
| @512: worst opening / filing | 56,771,716 B / 113,608,968 B | **169,856 B / 173,952 B** |
| @512: `carrier_fit_v1` at 1,583,616 B | REFUSED | **PASS** |
| @512: position / claim material | — | 82,036,553 B in 97 parts / 42,002,715,136 B |
| @8,192: gate | REFUSED (public 2,153,078,148,385 B > 2^40; 8,192 sessions > 1,024) | PASS: **1,751,863,987 B** per prosecution, worst court 169,856 B, 8 segments, 4,608 B on chain |
| @8,192: `carrier_fit_v1` | — | **PASS** |
| @8,192: position / claim material | — | 828,532,553 B in 848 parts / 6,787,338,674,176 B |

The probe also reports `TOKENIZER_MISSING`: the fixture carries no tokenizer file. That is a limit of the fixture, not a K2 refusal.
Log: `~/Downloads/MISAKA-wt-b/k2s-m1-smolvlm.log`.

## 11. Detection at real scale (SOUND's SG-06, reviewer question Q-01)

§2–§7 bound what a prosecution costs **once a fault is known**. They do not say how the fault is found. At 9B-8k a claim's material is
`P · M_pos` = 19,184,254,656,512 B (19.2 TB; SG-06's "≈ 16 TB, v4 estimate" is this measured figure — v4 commits the derived windows
too). SG-06: a verifier that reads `m` of `P` positions finds a one-position lie with probability `m / P` (8 of 8,192: 1/1,024); the
per-relation `2^-150` is then irrelevant and only deterrence is left, at a reservation of `gain · P / m / (1 − a)` = 40,960 BILI.

**The answer.** Detection at real scale is by **re-execution against the roots** (route B below), run on a post-commit sample of
claims. Reading positions at random (route A) is not a detection route at this scale. A check that costs less than a re-execution and
still finds every lie needs aggregated proofs: **sublinear-read proofs are a DESIGN_GAP** (§11.4).

### 11.1 Route A — sample positions and read their material

A verifier that checks a run of `m` positions reads `(m + 1) · M_pos + A` (scattered samples read up to twice that) and finds a lie
confined to one position with probability `m / P`. At 9B-8k (`M_pos` = 2,341,827,961 B, `A` = 3,508,121,330 B):

| `m` | per-lie detection | material read |
|---|---|---|
| 8 | 1/1,024 | 24.6 GB |
| 819 | 0.1 | 1.92 TB |
| 4,096 | 0.5 | 9.6 TB |
| 8,192 | 1 | 19.2 TB |

Detection is linear in the bytes read, so one bit of it costs about 9.6 TB. At this scale route A is a spot check (and the right
tool when a specific position is suspect), not coverage.

### 11.2 Route B — re-execute, compare roots, descend (`misaka-palw-kernel::seg_detect`)

The reference semantics are exact integers. A verifier that re-executes the claim's fed ids (the prompt is public on chain in tiles;
the delivered ids are in the claim; the artifact is public) computes every value an honest producer must have committed. It hashes
them into position roots and segment roots, and compares those with the claim's on-chain segment roots:

* **every root equal**: every committed value is the verifier's own (collision resistance). Only the decode relation is left, and the
  verifier checks it on its own logits (`first_decode_mismatch_v1`); a mismatch is a decode fault filed from one position;
* **a segment differs**: the verifier descends that segment's tree with the producer's **position paths** (a position root and its
  ≤ 10 siblings, authenticated against the on-chain root). Each probe shows both children of every node on its path, so the descent
  reaches the **first** divergent position `q` with at most `⌈log2 S⌉` = 10 probes. Every position before `q` is the verifier's own,
  so every input of `q`'s first wrong value is right, and the element courts' check of `q` (§3) returns a filing that convicts. That
  check reads positions `q − 1` and `q`.

What one check reads at 9B-8k, whatever `P`:

| Read | Bytes |
|---|---|
| the artifact | 3,508,121,330 |
| the segment roots and the prompt tiles (on chain) | 8 × 64 + ≈ 32 KB |
| ≤ 10 position paths | ≤ 10 × 704 |
| positions `q − 1` and `q`, and their node lists | 2 × 2,341,827,961 + 2 × 13,323 × 64 |
| **total** | **≈ 8.19 GB**: the per-prosecution read of §2, plus ≤ 7 KB of paths and the prompt |

What it costs: one re-execution of the claim in the reference semantics, plus hashing its values into commitments (`P · M_pos` =
19.2 TB hashed locally). That is the producer's own work: the producer hashes the same bytes to commit. Within that cost the check is
not a sample. Any wrong committed value or wrong delivered id is found with certainty, if the check runs.

Measured at the ledger level (wide128 at 4,501 positions,
`k2s_a_reexecuting_verifier_finds_any_lie_with_certainty_reading_two_positions`): an honest claim checks Clean with no material
read and no probe; a lie at position 3,000 (segment 2) is found after **7** position paths, reading exactly positions 2,999 and 3,000
(2,208 B), and is filed in 3,487 B and convicted; a delivered id that is not the rule's, over honest values, is found with no probe and
convicted by the decode court.

Under withholding, a probe the producer does not publish becomes a demand of that position. That is the whole position, served in
parts (§4), so the worst case is 10 more demand rounds. A demand for part 0 only (the position root and its path, ≈ 1 KB) would make a
probe cheap. That is a proposal; it needs an allocation (a demand stage value).

### 11.3 Coverage: a post-commit sample of claims, each one fully checked

Route B costs one re-execution per claim. Running it on every claim is the "everyone re-executes" requirement that PRINCIPLES §2 rules
out. The form that fits the principle runs route B on a post-commit sample of claims:

1. after a claim commits, a grinding-resistant beacon (the sealed-source v3, OPVB) draws whether the claim is checked (probability `q`)
   and by which bonded watcher(s);
2. a drawn watcher runs route B. A checked claim's detection is exact (no per-position loss), so the per-claim detection probability
   is `P_dc = q · P_run`. `P_run` is the probability that a drawn watcher **holds the registered model** (or acquires it in time),
   runs the check and files in time (A-HV). Route B re-executes from the artifact, so it needs the model;
3. the collateral follows SOUND's P-ECON: `reservation ≥ ⌈max(gain + default_penalty, ⌈gain / P_dc⌉) / (1 − a)⌉`.

**`P_run` includes acquisition (ADR-0177 D7).** The chain does not interfere with model acquisition and guarantees none (D1). A watcher
holds the model only if it obtains it off-chain. For a closed model whose holders all refuse to distribute it, every drawn watcher fails
for the same reason, and `P_run` (so `P_dc`) is **0**. Then no finite reservation deters: `gain / P_dc` is unbounded. Two consequences
for this design:
* `P_run` is one joint probability (holds the model, runs, files in time). It is never a product of separately estimated factors. Several
  drawn watchers are never counted as independent (`1 − (1 − p)^k`), because acquisition is a common-mode failure: a closed model fails
  all of them at once;
* every `P_run` below is an illustrative input, not a property the chain provides. A class whose `P_run` cannot be shown positive (a
  closed model with no outside holder) stays registered and judgeable, but has no detection, so it earns no reward and no consensus
  weight (PRINCIPLES §6, ADR-0177 D7: "unproven" is reported as unproven).

At the interim terms (gain 20 BILI, default penalty 100 BILI, `a` = ½):

| Detection | `P_dc` | Reservation |
|---|---|---|
| route A, 8 positions a claim (SG-06, row K) | 1/1,024 | 40,960 BILI |
| route B on every claim, `P_run` = ½ | 1/2 | 240 BILI |
| route B, `q` = 1/16, `P_run` = ½ | 1/32 | 1,280 BILI |
| route B, `q` = 1/64, `P_run` = ½ | 1/128 | 5,120 BILI |

The interim reservation of 1,000 BILI covers `P_dc ≥ 1/25`, i.e. `q ≥ 8 %` at `P_run` = ½. The expected verification work is `q`
re-executions per claim. An honest claim pays no accuser reward, so a drawn watcher needs a fee for the check (PRINCIPLES §6
condition 6).

This is how coverage can be achieved. It is not achieved today, and until each item below holds a K2-TIR-v4 class may stay registered
but earns no reward and no consensus work weight (PRINCIPLES §6):

| Needed | Kind | Where |
|---|---|---|
| the post-commit claim draw and watcher assignment, on the sealed-source v3 beacon | CODE | OPVB |
| a fee for a drawn check, from the user's escrow | ECON | G14-R4 |
| `q`, and a derivation of `P_run` (today `assumed_detection_permille` is an input, SG-13) | POLICY | Lead / user |
| the lazy watcher: a drawn watcher can report "clean" without running, and `P_run` cannot be read from the chain (forced-error audit claims, redundant watchers) | DESIGN | reviewer question |
| a drawn watcher's acquisition of the model: off-chain, guaranteed by nothing (ADR-0177 D1/D7); `P_run` = 0 for a closed model with no outside holder, and watchers fail together | ECON / POLICY | Lead / user (ADR-0177 D6/D7) |
| an OPV window that contains a route-B check of the class (`T_check` measured per class) | MEASUREMENT | MEAS |
| a part-0-only demand (a cheap probe under withholding) | proposal, needs an allocation | Lead |

### 11.4 Detection cheaper than re-execution — DESIGN_GAP

A check that finds every lie (or finds it with probability ≥ `1 − 2^-k` on every claim) at a cost well below re-execution needs an
aggregated proof over the whole trace. That means sum-check / GKR over a **polynomial** commitment of every committed value, with lookup
arguments for the non-linear primitives (`IntExp`, `IntRsqrt`, `IntLn`, `Compare` / `Select`, `TopK`, `Gather`) and the exact-integer
rules (RFC-0007 §V.4). K2-TIR-v4's commitments are hash trees: they localize and adjudicate (G14), but they do not aggregate.
Freivalds-style projections (K2-TIR-v1/v2) reduce a MatMul's compute, but they read all of its operands, so a whole-claim check still
reads `P · M_pos`. **Sublinear-read proofs for real-scale classes: DESIGN_GAP.** Following the appendix of PRINCIPLES: where the
collateral of §11.3 is unaffordable, the answer is these proofs, not a lower assumed detection rate.

### 11.5 Status

| Item | Status |
|---|---|
| post-detection bounds: one prosecution reads two positions and files one element court | IMPLEMENTED_AND_TESTED (§2–§8) |
| the re-execution detector: roots compared, ≤ ⌈log2 S⌉ probes, two positions read, decode checked | IMPLEMENTED_AND_TESTED (`seg_detect`; ledger level) |
| per-claim detection probability `q · P_run`, derived | POLICY + DESIGN (§11.3) |
| claim draw and watcher assignment on a grinding-resistant beacon | CODE (OPVB) |
| a fee for drawn checks | ECON (G14-R4) |
| sublinear-read proofs | DESIGN_GAP |

## 12. Encoders and heads: one position, element courts (K2-TIR-v5; HFX m3)

### 12.0 The finding, and why tiling rows alone does not fix it

HFX's m3 census: with testnet-12's ceilings, arming `palw_task_heads_v1` admits **no** encoder or head class on the generative route
(RFC-0003, tag 68). Of 40 embedding classes, 24 are refused at the close-sizing work cap (`2^26 + 1` against `2^26`) and 15 at
`max_position_macs`; `stsb-bert-tiny` is refused at the tile MACs (`2^25` against `2^24`). 0 of 5 heads are admitted.

The cause is the unit. A bidirectional encoder is lowered as **one position over a padded token axis** (`lower::bidir`): every value is a
`[L, …]` tensor of all rows at once. On the generative route a position is the unit of every bound:
- a Panel seat replays it (`max_position_macs`: bge-large at 512 tokens is 1.68·10^11 MACs against 2^37);
- a close walks its cone tile by tile across the whole sequence, so the sizing work passes `2^26` before it finishes.

Cutting the position into tiles of rows does not shrink the cone below one layer. Under bidirectional attention, row `i` at layer `ℓ` reads the
keys and values of every row at `ℓ`. A close that re-executes a tile of rows from the layer's inputs therefore still reads every row's
`K` and `V` (or re-derives them from every row's input) and the layer's weights. One layer is the smallest closed unit of re-execution.

### 12.1 What K2 does instead: commit every value, judge one element

Under K2-TIR-v4 every value of the position is a leaf-tiled commitment under the position root: `Q`, `K`, `V`, the scores, the
softmax's terms, the attention output and every FFN intermediate. A court never re-executes a layer or a tile of rows. It re-executes
**one output element of one value** from the committed operands along that element's dependency line (§3):

| Relation (one bidirectional layer, per head `h`) | One element court reads |
|---|---|
| `S[h,i,j] = Σ_c Q[h,i,c] · K[h,j,c]` | one row leaf of `Q` and one of `K` (`head_dim` elements each) |
| the mask `j < count ? S : MIN` | one element of `S`, and `count` (the job's) |
| the softmax along `j` (max, `IntExp`, sum, divide) | the line `S'[h,i,·]`: `L` elements, one leaf |
| `O[h,i,c] = Σ_j P[h,i,j] · V[h,j,c]` | row `i` of `P` and column `c` of `V`: `L` elements each, one leaf each |
| projections, FFN up / down, output projection | one row of the input (`d` or `ff` elements) and one column of the weight |
| layer norm (mean, variance, `IntRsqrt`) | the row (`d` elements) |
| the embedding `Gather` | one id (the job's prompt tile) and one row of the table |
| pooling (`[CLS]` row, or the mean over `count` rows), a classifier or span head | one row / one column of `L` |

"A row tile depends on all rows" is true of the computation, not of the court. The dependence on all rows passes through committed
tensors (`K`, `V`, `P`), and each of them is judged by its own relation. **A lie localizes to the first wrong value in canonical node order
and to one element of it**, exactly as in §3. The completeness argument is unchanged: every operand that element reads is an earlier
value, a param, a const, or the job's input, so it is right, and the element court on it convicts. The "close per tile of rows" of the
question is therefore finer here: the court is per element, and the tiles are the commitment's leaves (≤ 4,096 elements of one row or column).

### 12.2 Numbers at 512 tokens

Shape level (`misaka-palw-sdk/tests/k2s_encoder_court.rs`). The configurations are as published, lowered by `lower::bidir` with no
weight read. Each is judged as ONE position under the v4 plan, `check_plan_v1`, the per-prosecution gate and the node's carrier.

| Class @ tokens | relations | ε | MACs a position | position material B / parts | public B a prosecution | verifier RAM B | worst court B (where) | filing B | on chain B |
|---|---|---|---|---|---|---|---|---|---|
| bge-base-en-v1.5 @512 | 219 | 2^-166 | 48,318,382,080 | 9,053,268,756 / 8,770 | 18,162,950,859 | 18,162,634,619 | 47,568 (block 1 node 119, quant-range) | 51,664 | 4,160 |
| bge-large-en-v1.5 @512 | 219 | 2^-165 | 167,503,724,544 | 24,045,046,292 / 23,310 | 48,168,480,971 | 48,167,898,491 | 55,760 (block 1 node 119, quant-range) | 59,856 | 4,160 |
| bge-reranker-large @512 | 237 | 2^-165 | 167,504,774,144 | 24,045,130,340 / 23,310 | 48,621,320,020 | 48,620,735,044 | 55,952 (block 1 node 119, quant-range) | 60,048 | 4,160 |
| bert-base-cased-squad2 (encoder) @512 | 219 | 2^-166 | 48,318,382,080 | 9,053,268,756 / 8,770 | 18,160,593,189 | 18,160,276,949 | 47,568 (block 1 node 119, quant-range) | 51,664 | 4,160 |
| at tokens L (d 1024, ff 4096, 1 layer) = 4096 | 219 | 2^-169 | 85,899,345,920 | 34,113,572,948 / 33,874 | 68,319,848,011 | 68,319,646,203 | 169,680 (block 1 node 119, quant-range) | 173,776 | 4,160 |
| huihui-qwen3.5-9b @8192 (K2-TIR-v4) | 838 | 2^-150 | 8,524,398,592 | 2,341,827,961 / 2,294 | 8,193,657,124 | 8,191,777,252 | 170,432 (block 3 node 41, quant-range) | 174,528 | 4,608 |
| at hidden d (L 512, ff 4·d, 1 layer) = 27200 | 219 | 2^-165 | 4,559,837,593,600 | 29,038,337,108 / 28,245 | 68,677,244,483 | 68,676,813,307 | 399,048 (block 1 node 125, dense-matrix) | 403,144 | 4,160 |
| at FFN width ff (L 512, d 1024, 1 layer) = 439808 | 219 | 2^-169 | 463,856,467,968 | 17,160,783,444 / 17,139 | 35,300,729,443 | 35,299,118,075 | 1,579,240 (block 1 node 125, dense-matrix) | 1,583,336 | 4,160 |
| at vocabulary (L 512, d 1024, 1 layer) = 16777216 | 219 | 2^-169 | 6,979,321,856 | 1,093,467,732 / 1,060 | 36,712,778,049 | 36,712,690,929 | 54,992 (block 1 node 119, quant-range) | 59,088 | 4,160 |
| at layers at L 512 (bge-large) = 34 | 219 | 2^-164 | 237,296,943,104 | 34,023,993,492 / 32,984 | 68,126,590,603 | 68,125,792,891 | 55,952 (block 1 node 119, quant-range) | 60,048 | 4,160 |
| at layers at L 4096 (bge-large) = 1 | 219 | 2^-169 | 85,899,345,920 | 34,113,572,948 / 33,874 | 68,319,848,011 | 68,319,646,203 | 169,680 (block 1 node 119, quant-range) | 173,776 | 4,160 |

Every number in the table is **derived**: the gate computes it from the plan and the program's shapes (`derive_budgets`,
`public_prosecution_complete_v4`, `carrier_fit_v1`), and `check_plan_v1` recomputes it, so no registrant can declare its own. Log:
`~/Downloads/MISAKA-wt-b/k2s-e3b-encoders.log`. Refused, by name:
- BGE-M3 at 8,192 tokens does not lower (a score `MatMul` over 2^28 elements);
- BGE-M3 at 4,096 tokens: one prosecution would read 1,602,842,453,219 B, above the 2^40 limit, and needs 1,602,841,756,051 B of verifier
  RAM, above the 2^36 limit.

**The frontier: the worst geometry the ceilings admit** (from bge-large's geometry, one axis at a time, one layer where the axis is per
layer; derived):

| Axis | Largest admitted | What refuses the next value up |
|---|---|---|
| tokens `L` (d 1,024, ff 4,096) | 4,096 | the axis's own cap: v5 reads the ids from one prompt tile |
| hidden `d` (L 512, ff 4·d) | 27,200 | verifier RAM 68,859,342,331 B, above 2^36 |
| FFN width `ff` (L 512, d 1,024) | 439,808 | the carrier: a worst filing of 1,583,663 B against a `FileProof` of 1,583,616 B |
| vocabulary (L 512, d 1,024) | 16,777,216 | the axis's own cap |
| layers at L 512 | 34 | verifier RAM 70,121,582,331 B |
| layers at L 4,096 | 1 | verifier RAM 135,018,369,659 B |

At every admitted geometry the worst filing fits the carrier: the gate refuses any class whose priced court does not. The largest
court any v5 class can have is therefore 1,579,240 B, and its filing is 1,583,336 B (the FFN frontier). The largest verifier RAM is
2^36.

**Measured** (the K2-TIR-v5 ledger test on two real tiny encoders, `hf-enc/bert` and `hf-cls/xlmr_rerank`, lowered with their weights at
`L` = 12): the prover filed the first, a middle and the last element of every committed value, 1,155 and 1,209 filings. The court
dismissed every one, and each was within its relation's price (the largest at 0.892 of it). Every price was within the registered
plan's worst court. Lies in the embedding lookup, a projection, the key mask, an attention product and the result were each found at
the lying value and convicted, filed in 1,136–3,831 B.

### 12.3 Against PRINCIPLES §6

* **Condition 1 (coverage).** Every node of the encoder program has a relation with an element court: the plan covers every node and
  `check_plan_v1` passes (the ε of the table). Nothing is left to a Panel.
* **Condition 5 (collateral against the maximum gain).** Detection is §11's route B: re-execute the job (one forward pass, the MACs of
  the table: the producer's own work) and compare ONE segment root. For an encoder that is cheap enough to run on **every** claim, so
  `q = 1` is affordable and `P_dc = P_run`. At the interim terms (gain 20 BILI, default penalty 100 BILI, `a` = ½, `P_run` = ½) the
  reservation is **240 BILI**, against 40,960 BILI for sampling a 9B-8k claim (§11.3). The detection cost is one job's compute, not a
  fraction of a terabyte-scale claim. `P_run` = ½ is an input, not a guarantee: it includes the watcher holding the model (§11.3,
  ADR-0177 D7), and it is 0 for a closed encoder nobody outside its holders can obtain.
* **Condition 6 (the verifier's resources).** A check costs the forward pass plus hashing the position's values (the material
  column). It reads the artifact and the job's ids. Only when the roots differ does it also read one position of material and file one
  element court, whose size is the table's worst-court column, far under the carrier. The per-prosecution bound is the gate's column.
* **The DA side.** The position's material is gigabytes a job. A producer does not need to keep it: the job is deterministic and cheap
  to recompute from the ids on chain and the artifact. But a demanded position is served on chain in 1 MiB parts (the parts column),
  so the demand-bond fate (G14-R4, §4) must price a demand of an encoder claim at that many parts.

### 12.4 What a K2 encoder class is: K2-TIR-v5, with no new allocation

* **The descriptor.** K2-TIR-v5 is K2-TIR-v4 (the same families, commitments, element courts, DA and gate) plus **job-bound inputs**.
  The program's last two params are `input.ids` (`idx [L]`) and `input.count` (`idx []`), and they are the job's input, not the
  artifact. This is exactly the version-1 view of `encoder::bidir_v2`'s program, where lifted inputs trail the params. The class has
  one position (`max_positions = 1`), `L ≤ 4,096` (one prompt tile), and no node reads the per-position token.
* **The job.** `PostTiledJob` (inner 17) and its `PostPromptTile`s (inner 18):
  - the prompt is the template-applied ids (`[CLS] ‖ text ‖ [SEP]`, which the gateway forms, as on the generative route), with
    `prompt_len` = `count` ≤ `L` and `max_new_tokens = 0`;
  - the semantics pad it to `L` with id 0. A pad row never reaches a real row or the pooling (keys at or past `count` are masked), so the
    pad id does not change the result.
* **The claim.** `CommitSegmentedClaim` (inner 16) with no delivered ids and one position: one segment root on chain. The result is the
  committed output node (the pooled vector, or a head's logits) at position 0. Anyone reads it from the producer's stream, with its
  node opening against the segment root.
* **Courts and detection.** These are §3's and §11's. The ids operand is opened by the job's prompt tile (`ElementFaultV1::token`), and the
  count is the job's.
* **Allocation: none.** It uses inner kinds 16–18, `ProsecutionV1::Segmented` and `ClaimBodyV1::Segmented`, which are lane K2S's. A
  descriptor is identified by its digest. Like v4, v5 registers under OPV only and is armed in the dormant route template beside v1–v4.

### 12.5 The alternative: encoder-sized ceilings on the generative route

To admit these classes as they are, the generative route would need:
- `max_position_macs ≥ ` the MACs column. That is a Panel replay of a whole forward pass per claim per seat.
- A close sizing that walks a tile **class** rather than every tile (PALW-GEN-20's recorded follow-up). Until then the gate cannot price
  the close at all, and a higher cap only moves the refusal.
- Tile MACs above `2^24`.

Even then the Panel replay is the route's detection, and the court's close for a layer-crossing lie still reads the layer. So this keeps the
Panel as the root of detection, which PRINCIPLES §1 forbids as the final basis. It is not recommended as the main route. It would remain a
throughput option once K2-TIR-v5 carries G14.

### 12.6 What stays open

* **The span head** (`OUTPUT_TOKEN_LOGITS_V1`) is HFX's branch, not this one. Its head adds one `[L, d] · [d, 2]` product (`k` = `d`)
  and `2 · L` values: no measurable change to the bounds above. Pair segments (`ENC_PAIR_SEGMENTS_V1`) are HFX's too.
* **8,192-token encoders** (the BGE-M3 / XLM-R lineage) do not lower: their scores exceed TIR's 2^28-element cap (HFX's RESOURCE item).
  v5's one-tile input (`L ≤ 4,096`) is a second limit, and lifting it means a multi-tile `token` field.
* **The Head profile** (`palw_task_heads_v1`) is the generative route's fence. A K2 encoder class is the kernel route (tag 110, OPV),
  under its own dormant fence. The Head profile's decode rule (`HEAD_DECODE_V1`) applies unchanged to the verified output.
* **A court for `HEAD_DECODE_V1` (G14C GAP-40).** No K2-TIR-v5 claim carries a decision. The claim delivers no id, and the decision
  (arg-max, multi-label, regression, best span) is a pure function of the committed output node. HFX's code says the same: "never
  consensus". Once the output's element courts have judged the output, there is nothing left to adjudicate. If a future profile puts
  a decision on chain (a label a user pays against), it will need a decision court shaped like the greedy decode court: an arg-max
  label and its rival are two leaves of the output, a multi-label decision is one element per label, a regression decision is the
  value, and a best span is the two logit rows of the span. That is DESIGN, and it needs an allocation (a claim field and a
  `SegFaultV1` variant). Until then a Head class's earning rests on the verified output alone.
