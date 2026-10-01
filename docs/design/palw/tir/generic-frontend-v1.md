# PALW-TIR — the generic frontend: ModelSpec V1, the feature vocabulary, Levels, states, static shapes

| Field | Value |
| --- | --- |
| Status | design of what `misaka-palw-tir-lower` implements (RFC-0002 lane G); the code is the reference, this text is the reasoning |
| Rule | **a new model is a COMBINATION of a finite, versioned feature vocabulary — never a new primitive, a new runtime or a new court kernel.** When a model needs something the vocabulary cannot express, the gap is a named *capability* with the smallest GENERAL primitive that would close it, and the model is refused by name until the chain decides |
| Companions | [`model-adapter-v1.md`](model-adapter-v1.md) (the adapter file format), [`hf-coverage.md`](hf-coverage.md) (what lowers, with numbers; §21–§22 are this design's evidence), [`frontend-as-data-v1.md`](frontend-as-data-v1.md) (weights and routes as data) |

## 1. The pipeline

```text
 HF config.json (+ safetensors headers)
        │  hf_schema::read_model          a built-in or user adapter (DATA) · or the standard-decoder template
        ▼
   ModelSpec V1  ──── features() ───▶  [FeatureId]  ──▶  report (check-architecture): SUPPORTED / MISSING, Level, adapter
        │  hl::build_program              the math, frontend-neutral: HL ops, params, states, carries
        ▼
   HlProgram  ──── float_ref ───▶  the float reference (== transformers' logits to float noise)
        │  lower::lower                   generic lowerers: HL op → TIR nodes, a fill per param
        ▼
   TirProgramV1 + fills ──▶ calibration ──▶ integer artifact ──▶ tir_admit_v1 · the executor · the court
```

Three things are DATA and none is Rust per model: the **adapter** (config keys → a `ModelSpec`), the **weight mapping**
(role → tensor-name template, and per-param expressions; [`frontend-as-data-v1.md`](frontend-as-data-v1.md)) and the
**checkpoint**. Rust holds *features*: a feature is a spec field with a default, an HL decomposition, a lowerer and a test
— written once and available to every model that combines it.

## 2. `ModelSpec` V1

`spec::ModelSpec` (the type formerly called `ArchSpec`; the name remains as an alias) is the normalised description every
frontend produces: the embedding, the layers (each a *mixer*, an *FFN* and a *residual wiring*), the final norm, the head,
the output kind (logits or an embedding row), the residual's streams (`hyper`) and the storage description (`hf`). Rules:

- **`model_type` is informational.** Two models with equal specs compute the same function whatever they are called, and a
  model nobody has heard of is a new combination, not a new code path. An adapter claims a class by its `architectures`
  entry; `check-architecture` prints the `model_type` for information only ("it selects no code") and a test renames it to
  prove it.
- **Additive-only evolution.** A field is added with a default that reproduces every spec written before it; an adapter
  written for V1 keeps working, and a feature that did not exist when a model was read is simply absent from its report.
  A change of *meaning* is a new `FeatureId` (`…_V2`), never an edit.
- **The spec is serde JSON.** An adapter's `spec` template instantiates it (`misaka.palw.model-spec.v1`); a user adapter and a
  built-in one produce the same JSON for the same config, and the program lowered from either is byte-identical
  (`tests/architecture_report.rs`).
- **The HL builder never reads `hf`.** The tensor-name side of the spec is read only by the weight mapping, so an importer for
  another format (GGUF, ONNX) fills its own storage description and produces the identical HL graph.

## 3. The feature vocabulary (`model::REGISTRY`)

A **feature** is identified by `<AREA>_<NAME>_V<n>` (`RESIDUAL_GATED_HC_V1`, `ATTN_SPARSE_BLOCK_V1`, `EMBED_NGRAM_PLE_V1`,
`CONV_DEPTHWISE_CAUSAL_V1`, `MIXER_GDN_V1`, …). Each registry entry (`FeatureInfo`) states

| field | meaning |
| --- | --- |
| `id`, `area`, `title` | stable id; where it sits in a model (embedding, norm, position, attention, mixer, FFN, residual, head, storage) |
| `lowering` | `Implemented` (lowered, calibrated and run on the three implementations by the named tests), `Specified` (the spec describes it and the float reference runs it; the lowerer refuses with the id), `Missing` (not describable: a Level C finding) |
| `primitives` | the TIR primitives its lowering emits beyond the base every model shares; **checked**: a lowered program may use no primitive that none of its features declares, and a declared primitive must be emitted by some fixture that uses the feature |
| `protocol` | `None`, or `Capability { id, general_primitive, new_primitive, new_court_kernel }`: what the *chain* would need, with the smallest general addition that closes it — never a per-model primitive |
| `tests` | `<test file>::<test or fixture>`; for an implemented feature each must exist (`tests/feature_registry.rs`) |
| `doc` | the semantics in one paragraph, including the numeric decisions that are part of the feature's meaning (a tie rule, a unit) |

`ModelSpec::features()` reads a spec and lists the features it uses with their layers and a detail string. The registry is
*honest* by construction: `tests/feature_registry.rs` fails when a fixture's spec uses an unregistered or unimplemented
feature, when a lowered program uses an undeclared primitive, when a declared primitive is emitted by no fixture, or when a
test id names nothing.

**Adding a feature** (the whole job, in order): (1) the spec field with a default; (2) `features()` detection and the registry
entry with the semantics and the decomposition; (3) the HL ops (and params/states) in `hl/build.rs`; (4) the float reference
arm — it must equal transformers' logits (the fixture generator records them); (5) the lowerer in `lower/` (the program) and
its fills (the integers); (6) the adapter variable that turns it on; (7) a tiny fixture generated from transformers' OWN class
and the named tests. A feature that cannot be decomposed into the 25 primitives is not added: it is a Level C finding (§4).

## 4. Levels and the report

| Level | meaning |
| --- | --- |
| **A** | automatic: the standard keys and tensor names suffice (the `standard-decoder` template: sliding window, partial rotary, tensor-driven biases and q/k norm, multipliers, soft-caps); no adapter file was needed |
| **B** | a thin **adapter** — a data file (`misaka.palw.model-adapter.v1`) mapping the config to a `ModelSpec`; no protocol change. Every architecture the crate lowers is one (the pack is ~60 files) |
| **C** | a capability is missing: a feature that cannot be lowered, or a protocol gap; the report names it |

`palw-class check-architecture <hf dir | config.json> [--adapter <file | builtin:ID | none>] [--json]` prints, before the
admission numbers: the `model_type` (informational), the features each `SUPPORTED` or `MISSING (capability)`, the Level, the
adapter used (a built-in file with its hash, the caller's file with its hash, or none), the keys the class defaults supplied
("confirm against the class"), the config keys no rule accounts for (refused, never ignored), the missing items with the
general primitive that would close each, and **"No new consensus primitive required" / "No new court kernel required"** (or the
opposite) — then lowering and `tir_admit_v1` under the network's ceilings. The JSON adds `architecture_report`
(`misaka.palw.architecture-report.v1`). `palw-tir-check` prints the same report; `--adapter` is also on `palw-tir-fidelity` and
`palw-tir-convert`, so a model added with DATA only runs the whole chain with no rebuild.

Two decisions of the coordinator on the reader (2026-10-01, from the corpus lane): **(FR-25)** a user-supplied adapter may
override a built-in *refusal* (`adapters/refusals.json`: an architecture refused on purpose by the features it lacks) provided
it passes the same validation as any adapter; the report then says "user adapter overrides built-in refusal: …". **(FR-26)** a
Level A reading without a reference check is labelled **"A (unconfirmed)"** until a transformers/diffusers class confirms it
(`float_vs_hf` on the same weights): the standard keys are a *guess about a class the pack has never seen*.

## 5. States (`StateSpec`)

A program computes ONE position: the logits for `token` at `pos`, reading and writing its **states**. HL declares them
(`hl::StateDecl { name, kind, shape, per_layer, init }`) and the lowerer maps each to a TIR `StateDecl`. The rules are the
program's normal form (spec 04b NF-15, NF-19) and the court's replay model:

| property | rule |
| --- | --- |
| kind | **`Fixed`**: a recurrence — the whole tensor is carried from position to position and *saturates on write* (`lo..hi`, a range that contains 0). **`Hist`**: an append-only history of rows, read through a window `W` (`H = min(pos + 1, W)`), the appended row a commit point |
| dtype | `i8`, `i16` or `i32` (never wider: a state is committed and checkpointed) |
| initial value | **zero** — a fresh sequence reads zeros. A feature whose natural initial value is not zero stores a difference: the n-gram window keeps `token − eos`, so `0` is "no token yet" (`eos`) |
| reads | `Ref::State(j)` is the value at the START of the block, even after a `StateWrite` in the same block |
| writes | one `StateWrite` per state per block (NF-19); a per-layer state is written by its layer's one occurrence; `post` writes nothing; the written value is clamped to `lo..hi` |
| per layer | a state is per-layer iff the block that touches it is a layer block (NF-15: a layer block reads only per-layer states); a global state lives in `pre` |
| replay | the court recomputes a state at position `p` from the nearest checkpoint (every `C` positions) and the committed leaves; admission charges the replay (`checkpoint interval C = min C_j`), so a state whose update is expensive limits `C` |
| size | at most 2^28 elements; ≤ 16 per-layer states a program |

The states of the generic features: `gdn.S` (`i32 [heads, dv, dk]`), the conv windows (`i16 [(k−1)·dilation, ch]`, zero = the
left pad), `ple.ngram.win` (`i32 [n − 1]`, range `[−eos, vocab − 1 − eos]`), `attn.idx.sum` (`i32 [dim]`, range
`±32767·ratio`: the running sum of a block's raw keys, restarted by `Select` at the block's first position), `attn.idx.keys`
(`i16 [blocks, dim]`: the pooled, normed, rotated key of every complete block, written by a one-hot `Select`), `attn.k_hist`
and `attn.v_hist` (`Hist`). A feature is registered with the states it adds in its `doc`.

## 6. The static-shape rule

> **Data-dependent VALUES are fine. Data-dependent SHAPES never.**

Every tensor in a TIR program has a type fixed by the program (the only dynamic dimension is `H`, the history length, and
nothing may select, gather or concatenate *along* `H`). The reason is the court: a commit point's cone must be evaluable from
committed leaves with a bounded, program-determined amount of work, and a history reduction must be *dissectable* — a
reduction over `H` splits into partials over ranges of positions whose sum or max is the whole. A shape that depended on the
data would make both unbounded. The consequences, as the features obey them:

- **Sparse attention selects by masking, not by slicing.** `ATTN_SPARSE_BLOCK_V1` scores EVERY block the program keeps keys
  for (`[blocks]`), an incomplete block scores a floor (`−1`: every real score is `≥ 0`), a fixed-`K` `TopK` over all of them
  picks `K` ids, and the attention logits of the keys outside the picked blocks (and outside the incomplete tail) are set to the
  softmax's floor by a `Select` — `exp` of the clamped difference is exactly 0. `K = min(budget / ratio, blocks)` is a
  *program* constant; when fewer than `K` blocks are complete the ids include the lowest-index incomplete blocks, whose tokens
  are in the always-visible tail anyway, so the visible set equals the float reference's. Nothing is gathered along `H`: the
  per-block mask is a `[blocks]` vector looked up by each key's block (`Gather(sel, ⌊j / ratio⌋)`).
- **Routing is `TopK` plus `Gather` of weights.** `K` experts always; the expert axis is a `Fixed` axis.
- **A table above 2^24 rows is chunked, not dynamic.** The row lookup of a hashed embedding is, per hash head, `⌈size / 2^24⌉`
  axis-0 `Gather`s of that head's chunk params and a `Select` by the chunk index (§9.4) — a layout a runtime can address by row.
- **A token indexer selects by counting, not by `TopK` over `H`.** `ATTN_TOKEN_INDEXER_V1` (§9.6): the window is `H` long and
  `TopK` needs a `Fixed` axis, so the `k` best tokens are the mask `κ_t ≥ τ'` of a threshold found by a radix search of
  reductions over `H`.
- **A window is a state, not a slice.** A dilated convolution keeps `(k − 1)·dilation` rows and reads fixed rows of
  `state ++ row`.

## 7. `TopK` ties

`TopK(x, axis, k)` is always a commit point (PALW-TIR-11) and its tie rule is pinned by spec 04b §6.6: **the higher value
first; among equal values the lower index first; the selected set is returned in ascending index order.** Consequences the
registry states for each feature that selects:

- The float references use the same rule (`float_ref::top_k_indices`), so a tie is the same in both worlds. Block scores of
  sparse attention tie constantly (a ReLU makes exact zeros common; an all-zero indexer makes EVERY complete block tie), and
  a router's near-ties are the dominant source of integer-vs-float disagreement on MoE models.
- transformers' own `torch.topk` has no specified tie order; the fixtures record what it did. The all-zero-indexer fixture
  (`qwen4_qsa_tie`) selects the lowest-index blocks in transformers too, so the pinned rule is the observed behaviour, not a
  convention imposed on it; a model whose reference breaks ties another way is a fidelity question, never a reason to change
  the primitive.
- Masked entries use a floor *strictly below every real score* (`−1` against scores `≥ 0`), so an incomplete block can be
  selected only when fewer than `K` complete blocks exist — in which case it is harmless (§6).

## 8. Number formats and `LOGITS_Q24_V1` (lowering version 2)

Activations are `i16` codes at a static scale per site and layer (A16, headroom over the calibrated absmax); weights are `i8`
per output row (W8; an embedding table and a head tied to it are `i16` per row); norm unit rows, softmax probabilities,
decays and gates are Q24 `i32`; attention and router logits are Q14 `i32`; every change of scale is the A16 narrowing
`N(x; m, s, z)` with `m`, `s`, `z` three separately typed per-channel params. A **scale key** (`ScaleKey`: a base — the
residual, a calibrated site, Q24, a fixed number, a power of two — and a factor) names the float value of one integer unit;
keys resolve per occurrence at materialisation, so a recalibration moves the artifact and never the program.

**`LOGITS_Q24_V1`.** Until version 2 the logits left the head at a calibrated static scale — an arbitrary real
(`2^−27.5 … 2^−23.7` on the fixtures) — so a sampler had to read the scale from the pack and a temperature meant something
different for every class. Version 2 fixes the unit: **a text program's logits are natural-log units × 2^24 in an `i32`,
whatever the model.**

- *The change.* The head's last narrowing lands on `2^−24` (`ScaleKey::q24`) instead of the site's calibrated scale. The nodes
  are the same, the `(m, s)` params differ: the program digest, node count and param count are identical to version 1's and
  only the artifact changes. `LOWERING_VERSION = 2` is recorded in the convert/fidelity provenance.
- *The bound.* `i32` at `2^−24` holds `|logit| < 128`. A model whose calibrated logits reach 120 is refused
  (`NOT_LOWERABLE`, naming `LOGITS_Q24_V1`), never clipped; an input beyond 128 at run time saturates. The margin keeps ordinary
  inputs from saturating; the encoders' embedding output keeps its own units (Q30 normalised, a power-of-two unit otherwise).
- *Cost.* None on the logits tile: the close carries `i32` logits as before, so close sizes and the court's cone for the logits
  node are unchanged. The regression gate holds the version-1 *programs* byte-identical (their artifacts moved on purpose) and
  pins version-2 artifacts separately.
- *Evidence.* `tests/logits_q24.rs`: the logits scale is exactly `2^−24`; the integer logits divided by `2^24` agree with
  transformers' on eight head shapes within the quantisation's error, top-1 agreeing wherever transformers leads by more than 8×
  that error; a head scaled to hundreds is refused by name.

## 9. Decompositions

Each is a combination of the 25 primitives. Node counts are measured on the lowered tiny fixtures (a tile's width does not
change them); court numbers are from `tir_admit_v1` and the demand evaluator.

### 9.1 Hyper-connection residuals (`RESIDUAL_GATED_HC_V1`)

Semantics (transformers' `qwen4_exp`, verified against its modules): the residual is `S` streams of the hidden width
`D` (flattened, `S·D` lanes; the embedding is repeated into every stream; one stream is the plain residual). Each half-layer:

```text
x̂ = RMSNorm_per_stream(h) · (1 + w)             grouped norm, `S` groups, a full-length gain
w = σ( up( silu( down(x̂) / S ) ) )              low-rank pair, a sigmoid gate of the full width
y = block( mean_s ( w ⊙ x̂ ) )                   the mixer or the FFN reads the gated mean, [D]
g = 2·σ( inject(x̂) / S )                        [S] injection weights
h ← h + y ⊗ g                                    every stream takes the block's output with its own weight
```

The head reads one more mix (norm, down, up, sigmoid, mean — no injection). Decomposition: the existing grouped RMS template, the
existing narrowing-fed `Linear`, activation *tables* (SiLU, sigmoid: 65,536-entry `i16` params, gathered by `code + 32768`),
an elementwise `Mul`, and two small ops with their own lowerings: **`StreamMean`** = `Reshape [S, D]`, `ReduceSum` over the stream
axis in `i32` (exact), one narrowing whose ratio carries the `1/S`; **`StreamOuter`** = `Reshape [S,1] × [1,D]`, an exact `i32`
`Mul`, one narrowing straight to the residual's scale (so the residual `Add` needs no further conversion). A half is about 80
nodes; a layer is its mixer half and its FFN half (two blocks: NF-12 caps a block at 512 nodes), so the widest block (sparse
attention's mixer half) is 450. The carry is `S·D` `i32` lanes and every layer block reads and writes it; `S ≤ 64` and `S·D ≤ 2^24`.

### 9.2 Sparse block attention (`ATTN_SPARSE_BLOCK_V1`)

Semantics: a token indexer scores *blocks* of `ratio` positions. Per position: `qk = index_qk_proj(x)` splits into index
queries (`n` heads of `dim`, per-head RMS `(1 + w)`, rotated at the position) and ONE raw key; the raw keys of a block are
summed (state), the mean of a *complete* block is normed, rotated **at the block's first position**, and stored as the block's
key. A query scores `Σ_h relu(q_h · key_b)` over the complete blocks; the top `min(budget / ratio, complete)` blocks and the
incomplete tail are visible. Decomposition:

| step | primitives |
| --- | --- |
| block position arithmetic (made once per block, only what is read: a node nothing reads is a dead node, which normal form forbids) | `Div`, `Mul`, `Sub`, `Compare` on `pos` (`blk = ⌊pos/r⌋`, `start = blk·r`, `pm = pos − start`, `completing = (pm = r−1)`, `after = ⌊(pos+1)/r⌋`) |
| pooled key | state `attn.idx.sum`; `Select(pm = 0, 0, State) + k`, clamp to `±32767·r`, `StateWrite`; one narrowing with `1/r` in its ratio |
| rotation at the block start | the RoPE angle tables read at `start` (`rope_angles_at`); the usual half/interleaved rotate on slices |
| block-key matrix | `Select(Compare(Iota[blocks,1], blk) · completing, row[1,dim], State[blocks,dim])`, then `StateWrite` — the matrix *after* this position's write is what the scores read, so the block completing now scores without a separate path |
| scores | `MatMul(keys[blocks,dim], qᵀ[dim,heads])` in `i64`, `Clamp(0, ·)` (the ReLU), `ReduceSum` over heads, shifted right by `⌈log2(heads·dim)⌉` into `i32` (the ranking needs no more) and **committed**: a `TopK` cone is then `blocks` leaves, each a small matmul |
| selection | `Select(Compare(Iota, after) < , score, −1)`; `TopK` (committed) → `[K]` ids |
| mask | `[blocks]` selection vector = `ReduceMax(Compare(Iota[blocks,1], idsᵀ[1,K]))`; per key `j` of the window, `Gather(sel, ⌊j/r⌋)` OR `j ≥ ⌊(pos+1)/r⌋·r`; logits `Select(visible, logits, i32::MIN)` before the softmax |

The `1/√dim` of the float score moves no rank and is dropped. The block-key write is `O(blocks·dim)` a position (small beside
the matmuls at published sizes; a dynamic-row write primitive would make it `O(dim)`: recorded in the registry as evidence, not a
requirement). Measured: the sparse mixer half is 450 nodes; the worst court terminal is 4,096 MACs.

### 9.3 Dilated depthwise causal convolution (`CONV_DEPTHWISE_CAUSAL_V1`)

`y[c] = act( Σ_t w[c,t] · x[p − (k−1−t)·d][c] )`, zeros before the sequence. The state keeps `(k − 1)·d` rows;
`win = Concat(State, row)`; the new state is `Slice(win, 1, (k−1)·d)`; the taps are `Gather(win, [0, d, 2d, …, (k−1)d])` — one
`Gather` by a *constant* index vector — times the `[k, C]` taps, `ReduceSum` over taps, one per-channel narrowing, then the
activation table. `d = 1` is the library's contiguous template, byte-identical to before; the n-gram embedding's convolution has
`d = ngram_size`. Up to `(k−1)·d` positions the left context is the zero pad and the first positions are held to the same fidelity
as the rest (`PLE-05`). `k ≤ 64` and `(k−1)·d ≤ 4,096`.

### 9.4 Hashed n-gram per-layer embedding (`EMBED_NGRAM_PLE_V1`)

Semantics (verified against `Qwen4ExpTextNGramEmbedding`): for each order `n = 2..N`, `heads_per_ngram` hash heads map the last `n`
tokens of the position's *segment* (an `eos` ends one and still belongs to the segment it ends; slots before the start read
`eos`) to `mixed_n = t₀·m₀ ⊕ … ⊕ t_{n−1}·m_{n−1}` (64-bit; the multipliers are odd, `2·(splitmix64(seed_layer + γ·(i+1)) mod
⌊(i64::MAX / vocab) / 2⌋) + 1`, so `t·m` never overflows) and `id = mixed_n mod prime_h + offset_h`, where `prime_h` is the
`(layer·heads + h + 1)`-th prime above `vocab_base − 1`. The rows are concatenated, projected to a per-stream key and a shared
value; the normed streams gate the value (`σ(signed √(k·q / √D))`, signed-sqrt with a `10⁻⁶` floor); a dilated depthwise
convolution of the gated, normed value adds local context; the result joins every stream.

**The hash in `i64` arithmetic — no consensus change.** XOR is not a primitive. The program computes it by bit decomposition:

1. the `n` tokens (`this` and the window state) times the `n` multipliers (a `Clamp` states the multipliers' range `[0, ⌊i64::MAX/vocab⌋]`
   for the range analysis: a param takes its dtype's full range) give `a_i < 2^63`;
2. bits: `q = a / [2^0 … 2^62]` (one `Div` by a constant vector), `bit = q − 2·⌊q/2⌋`, a never-fires `Clamp(0, 1)` that gives the
   analysis the interval it cannot derive (it is not relational);
3. the XOR of the first `m` products' bits (`m = 2..n`) is the parity of their sum: **one `MatMul` by a 0/1 triangular constant**
   gives every order's sums, `par = sum − 2·⌊sum/2⌋`;
4. recomposition `Σ_k par_k·2^k` is a `MatMul` by `[2^0 … 2^62]` accumulated into `i128` (the analysis bounds a 63-term sum of
   terms up to `2^62` by `2^68`), clamped back to `i64`;
5. each head picks its order's value (`Gather` by a constant), and `mixed mod size` is a floor-`Div`, a `Mul` in `i128` and a
   `Sub`, clamped into `idx [0, size_max − 1]` and **committed**.

**Measured: 26 nodes for the ids of a trigram layer; the table read is 14 more; the whole PLE block is 187 nodes** with its three
RMS norms. With general integer bit primitives (`XOR`, shifts, a wrapping multiply, a remainder) the ids would be about 6 nodes —
recorded in the registry as evidence for a possible one-time primitive-set extension, decided later, **not required**. The
arithmetic is unit-tested against the reference function at the published scale (a vocabulary of 248,320, head tables of 20 M
rows, 16 heads, layers 0 and 37) *through the range analysis*, with the function alone — never a table.

**Tables above 2^24 rows (NF-8) are a lowering matter, not a capability — and the table is what a runtime can address by row.**
Each hash head owns a contiguous range of the layer's table, and the lowering cuts the table **per head**: one param
`[rows, dim]` of `i16` codes (at ONE scale per layer; one `narrow` to the consumer's scale afterwards) for each head, a head's
table taller than `2^24` rows cut into chunks of `2^24` rows (`LowerOpts::table_chunk_rows`) with a `Select` by the chunk index,
and **every param read by `Gather { axis: 0, batch_dims: 0 }` of the param itself** (the head's id, a `Clamp` stating its range
for the range analysis). Published sizes (20 M rows a head) take two chunks a head — 32 params of `[16,777,216, 128]` and
`[3.22 M, 128]` — and the PLE block is 349 nodes (the real-shape program: 9 blocks, the largest 450 nodes, 187 cones, cone work
6,561, admitted). Cutting at 16 rows (`PLE-06`, 24 chunks) gives logits bit-identical to the unchunked lowering's, equal on the
three implementations, with the court reproducing every node. What *is* large at published size is the artifact (hundreds of GB
of `i16` codes for the PLE layers), a matter of the conversion's streamed fill and of what a registration may carry, not of the IR.

*An earlier lowering read a layer's table as ONE batched gather (`batch_dims = 1`) over `[heads, rows, dim]`. It was changed on
purpose (`tests/golden/lowering_v2*.json`, `INTENDED`): see below.*

**The layout contract with the runtime residency (lane M2, `runtime-residency.md`).** A residency never holds a table of hundreds
of GB; it reads the rows a forward gathers. It tells a table from a dense weight by the PROGRAM's dataflow alone: a param is
**row-addressed** when every use is a `Gather { axis: 0, batch_dims: 0 }` of the param itself or of an uncommitted, uncarried
`Reshape` chain of it (a row of a row-major view of a contiguous tensor is `unit` elements at `row × unit`), and is then served by
rows — *gathered* when every index is a function of the inputs, *routed* when an index is computed from params (a param taints it:
the n-gram hash's per-layer multipliers and head sizes are params, because the PLE block is shared by the PLE layers, so M2's rule
calls an n-gram table routed; either tier reads a row at its offset) — and *pinned* otherwise. So the rule on what this lowerer
writes, for every table and every stack a program gathers (embedding, per-layer, n-gram and position tables, a mixture's expert
stacks and their per-row scales):

* **one tensor, axis 0 = the gather unit.** One row — `dim` codes, 256 B at 128; an expert's matrix — is one gather unit and the
  container holds the rows in order, so row `r` is at `r × row bytes` of the instance. Never a batched gather over a stack of
  tables, a transposed copy, a blocked or interleaved layout, a layout whose row is not contiguous: a layout a runtime cannot
  address by row offset reads the table densely — a pinned 51 GB table;
* **a table over `2^24` rows is cut along axis 0** (chunks, a `Select` by the chunk index) and never along another axis;
* **the container's bytes of a table are its artifact rows**, so the streaming writer below and a residency agree on what row `r`
  of a param is without either knowing a model.

`tests/row_addressing.rs` holds it with M2's own classifier (`tests/common/m2_tiers.rs`, vendored verbatim from `tir/residency`; the
real module replaces it when that branch merges): at Qwen4-Exp's published shape the 32 n-gram head tables of every PLE layer are
served by rows (none pinned) with the 512-expert stacks routed and the token embedding gathered — 750 GiB of weights, **2.55 GiB
pinned, a floor of 3.77 GiB** — at any chunking of the fixtures; the mixtures' expert stacks are routed and every embedding
gathered at the real shapes of Qwen3-30B-A3B, DeepSeek-V2-Lite and gpt-oss-20b.

**The streamed fill — the writer API (final).** A registration may carry any artifact size: seat resources gate readiness (staged
enablement), never admission, and the preflight reports the seat need (`palw-class check-architecture` prints `seat need: … GiB`;
`artifact_bytes` in its JSON). What stands between the IR and a published-size artifact is the CONVERSION, and it is bounded by one
block of rows and one chunk, whatever the model (`palw-tir-convert`, `tests/streaming_budget.rs`). The surface a writer is built on:

| piece | what it is | contract |
| --- | --- | --- |
| `Lowered::row_params` | the params that are row-wise codes of ONE HL param (`RowParam { hl, kind }`), by TIR param name | a conversion produces these by blocks of rows; every other param comes with its occurrence |
| `RowKind::{W8, T16, SplitMain, Mapped}` | per-row `i8` codes (a projection, an expert stack), per-row `i16` codes (an embedding), a split-outlier main matrix, and the **mapped** table | codes are per row, so a block of rows is the whole tensor's rows byte for byte |
| `RowKind::Mapped(MapRef)` / `trait RowMap { key, runs, used_rows }` | the artifact rows are NOT the checkpoint's rows in order: `runs(hl, layer)` lists `RowRun { dest, src, len }` (artifact rows `dest..dest + len` are HL rows `src..src + len`; rows no run covers are zero), `used_rows` the HL rows `0..used` that carry the table | **row-major, ascending, one run's rows contiguous**; the n-gram chunk of head `h`, chunk `k` is ONE run (`src = head_offset[h] + k·chunk_rows`); a map never reorders within a block, so a writer reads exactly the checkpoint rows a block needs and nothing else |
| `FillCtx::{table_amax, inject_amax, with_block, block_range}` | the table-wide scale as a first pass: `max|x| / 32767` over the used rows keeps nothing but the maximum, handed to the block fills ready-made | the whole-tensor path computes the same number, so the two agree to the byte; every chunk param of a layer reads the ONE scale |
| `materialise_stream(lowered, hl, loader, stats, policy, &StreamOpts, &mut dyn TensorSink, progress)` | produces every param of every occurrence, handing each tensor to the sink | the result does not depend on `StreamOpts { defer_min_elems, block_elems }`, the block size, the chunk size or the thread count |
| `trait TensorSink { begin(param, layer, bytes), push(bytes), end() }` and `ChunkSink` | where the tensors go, one instance at a time, in pieces; `ChunkSink` cuts what arrives into canonical chunks of a content-addressed store and is the recipe `write_container_v1_chunked` assembles a `PALWTIR1` container from | an instance's bytes are its row-major elements: a runtime addresses row `r` at `r × unit × width`; a sink may be a file, a chunk store or a multipart upload |
| `Streamed::new(hl, binding, checkpoint)` (an `OccParams`) and `weights::stream::{src_row_space, eval_src_rows, row_blocks}` | the source: only the occurrence being filled has its float params loaded; a deferred param's rows are read through the binding's `Src` by ranges (`weights::remote` serves them over HTTP ranges) | nothing here knows a tensor name beyond the binding's |

Invariants (held by `tests/streaming_convert.rs`, over every fixture and every block size from one row up; `tests/streaming_ple.rs`
for the mapped tables; `tests/dsa.rs` for a layer split in two blocks): **W-1** the streamed container is the whole conversion's file,
byte for byte (so the same inventory root and class id); **W-2** a table-wide scale is a first pass that keeps only the maximum;
**W-3** what is resident is a block and a chunk; **W-4** a mapped table's bytes in the container are its artifact rows in order
(the residency contract above); **W-5** nothing in the writer depends on a model's name. The lowering never asks for a whole table:
the chunk fills read only through the row map, the hash's tables (`NgramTables`) are tiny (primes and offsets), and the cost at
published size is two reads of the rows used (`16 × 20 M × 128` values per layer: ≈ 82 GB of codes written, about twice that read
per pass from bf16), a property of the model, not of the IR.

### 9.5 The gate activations

A gated norm's gate is data (`Op::GatedRmsNorm.act`: SiLU or sigmoid), a table over the `i16` grid like any activation;
`Act::SignedSqrt` (`sign(x)·√max(|x|, 10⁻⁶)`) is one more table. Nothing here needs a primitive.

### 9.6 DeepSeek sparse attention (`ATTN_TOKEN_INDEXER_V1`, FR-09)

**Semantics** (`DeepseekV32Indexer`, `GlmMoeDsaIndexer` of transformers 5.17): a learned scorer chooses which of the visible tokens
an MLA layer attends over. The indexer reads the MLA's own **q-latent** `qr = q_a_norm(q_a x)`, projects it to `index_n_heads`
query heads of `index_head_dim` (`wq_b`), rotates the FIRST `qk_rope_head_dim` lanes of each head (the opposite of MLA's nope-first
layout; rotate-half for DeepSeek-V3.2, interleaved for GLM-MoE-DSA), and scores every token by
`s_t = Σ_h w_h · ReLU(dim^-½ q_h · k_t)` with `w = weights_proj(x)·heads^-½` and `k_t = rotate(LayerNorm(wk x_t))` (one head, cached
per position). The MLA softmax then runs over the `min(index_topk, H)` best tokens. HF pins nothing about **ties**: `torch.topk`
returns, for a vector whose seven best scores are all 0.0, the indices `[5, 1, 8, 3]` — and ReLU makes exact zeros common. The IR's
`TopK` rule (04b §6.6, lowest index first) is the pinned choice, over the history window.

**Why not `TopK` and not a gather.** `TopK` needs a `Fixed` axis and the window is `H` long; a `TopK`/`Gather` along `H` would
break "dissectability is structural". The selection is the exact mask of a threshold found **by counting** (`lower/dsa.rs`):

```
  s_t   the head-weighted score, narrowed to SB bits at a calibrated scale (ReLU zeros stay exact zeros)
  κ_t = (s_t + 2^SB) · 2^b + (2^b − 1 − t)         distinct: the low b bits break ties toward the lowest t
  τ'  = the k-th largest κ  (0 when the window holds fewer than k tokens)
  vis_t = [κ_t ≥ τ']                                exactly min(k, H) tokens, then Select(vis, logits, MIN) before the softmax
```

`τ'` is a 16-ary radix search of `B/4` passes (`B = SB + 1 + b`): pass `j` counts `κ ≥ prefix + (i+1)·step_j` for 15 candidates
(`Compare` + `ReduceSum` over `H`), and the next prefix is the largest candidate with at least `k` tokens at or above it (`Select` +
`ReduceMax` over the 15). **`B/4 ≤ 10` reductions over `H` in one cone** (04b §9.5.1 allows 16); `τ'` is a **commit point** of two
`i32` lanes (a commit point is at most 32 bits wide: `hi · 2^28 + lo`), so every masked row compares against one value. The
selection is masked-dense: no arithmetic is saved, the function is exact. With `SCORE_BITS = 16` (resolution one part in 65,000 of
the largest score — as fine as the 15-bit codes it comes from) a window of `2^18` takes 9 passes, `2^13` takes 8.

**Block size.** The selection is about a hundred nodes beside an MLA mixer that is already a large block, so a DSA layer runs as its
**mixer half and its FFN half** (the residual carried between them, as a sandwich layer does): 479–497 nodes in the mixer half, under
512. Layers without an indexer are unchanged (`tests/golden_lowering.rs`).

**Evidence** (`tests/dsa.rs`; the tiny DeepSeek-V3.2 fixture, `index_topk` 4 over sequences of 10 to 24 tokens, so the selection
selects from the fifth position on): the float reference equals `transformers` to `1e-6` at all ten positions **with the IR's tie rule**
(`tools/gen_dsa_ties_fixture.py` replaces the one line `index_scores.topk(...)` of transformers' own model by a stable descending sort;
`torch.topk`'s own order differs from it at the last position, where seven tokens tie at zero for four places, and at no other); the
integer program against transformers' logits: top-1 1.000, KL `3·10⁻⁵`; against its own float reference at `index_topk` 1, 2, 3, 4, 6, 10:
top-1 ≥ 0.97, KL ≤ 0.008 — and against the *dense* float model KL 0.03–0.19, so the selection is applied; the radix search equals a sort
(ties to the lowest index) on windows of 1 to 300, `k` from 1 to past the window, over vectors of ties, extremes and negatives, through
the range analysis; the same bytes at every commit point on the reference evaluator, `ref2` and the typed backend; streamed = whole.
**Admission at the real V3.2 shape** (61 layers, 128 heads, indexer 64 × 128, top 2,048) at windows `2^13` and `2^16`: ADMITTED, the
worst terminal tile 5.25 M MACs (of 16 Mi), at most 9–10 reductions over `H` in a cone, cone work 4.3 K, 6.27·10¹¹ MACs a position at
`2^16` (the indexer adds 5.7 %); at the default `2^18` window it is refused by the position's MACs exactly as DeepSeek-V3 is.

**Spec and adapter.** `MlaSpec.indexer: Option<TokenIndexerSpec { heads, head_dim, topk, rope, k_norm }>`; adapter `deepseek-v32`
(data: extends `deepseek-v3`, `layer_types`/`mlp_layer_types` read, the indexer's tensors `self_attn.indexer.{wq_b, wk, k_norm,
weights_proj}` named). `glm_moe_dsa` differs only in `rope.style` (interleaved): an adapter, no code.

### 9.7 Convolutional networks (`CNN_FROM_SPEC_V1`, `CONV_DENSE_V1`, `BN_FOLD_V1`, `POOL_MAX_2D_V1`, `RESIDUAL_ADD_ACT_V1`, FR-19)

A convolutional network is a `CnnSpec` (`lower/cnn.rs`): a tree of `Conv` (kernel, stride, zero padding, dilation, groups, its batch
norm and activation), `MaxPool`, `Act` and `Residual { main, shortcut, act }` ops, a normalisation, and an output (`Map`: the last
feature map as rows, `GlobalAvg`: its mean). An adapter of kind `cnn` instantiates it from `config.json` with `$map`/`$range`
(`adapters/resnet.json`: ResNet-18 to -152, basic and bottleneck, `downsample_in_bottleneck`, the 1x1 projection shortcut where the
width or the stride changes — equal, op for op, to the ResNet that `tests/cnn.rs` writes out in Rust, on the fixtures and on the real
configurations of ResNet-18, -50 and -152). No network's name is in the lowering; ConvNeXt and MobileNet v1/v2 are adapters
that use the depthwise path below and four more features of the spec (§9.9).

**Layout.** An activation is `[P, C]` — a row per spatial position, a column per channel — of `i16` codes at a calibrated scale per
site, as a vision tower's rows are. The input is the class's canonical image (`u8` HWC, `input.image`); its HWC order *is* the rows
`[H·W, 3]`, and `(x/255 − mean)/std` per channel is one per-channel narrowing (not folded into the first convolution: a zero-padded
stem sees the padding in the *normalised* space).

**A convolution is one linear map.** The rows get one zero row appended (the padding's value); a pinned `Idx` table `[P_out, k·k]`
names, for every output position and tap, the row it reads (the zero row where the tap falls in the padding); one `Gather` makes
`[P_out, k·k, C_in]`, one `MatMul` against the `i8` weight `[k·k·C_in, C_out]` gives the exact `i64` accumulators, narrowed per
output channel. **Stride, padding and dilation are the table's content**; the table is clamped into range before the gather (a
`Clamp` that never fires, so the range analysis is total for any registered table). A depthwise convolution (`groups = C_in = C_out`)
is a `Mul` against `[k·k, C]` and a `ReduceSum` over the taps; any other grouping is refused by name. No primitive is added; a strided
window view `Unfold` (kind S) would only replace the table and is not needed.

**Batch norm costs no node** (`BN_FOLD_V1`). At inference `y = γ(x − μ)/√(σ² + ε) + β` is `W' = W·γ/√(σ² + ε)`,
`b' = β − γμ/√(σ² + ε) + (γ/√(σ² + ε))·b`; the weight codes are those of the folded rows (the per-row scale absorbs the positive
factor) and the bias joins the narrowing's offset. **A residual unit** narrows both branches to `i32` at one calibrated scale, adds
them exactly, applies the activation and narrows to `i16` codes (ReLU is the narrowing's clamp at 0; any other activation is the
table after it). **Max pooling** gathers the windows with a padding row of the code floor and takes a `ReduceMax` over the taps —
exact on the codes. **Global average pooling** is an exact sum and one rounded division.

**Blocks, and the one carry.** A program has at most 16 blocks of 512 nodes, so the top-level units (a residual unit is never split)
are packed into blocks by an estimate of their node count (22 per convolution, 12 per pool or activation, 10 per residual sum;
`BLOCK_BUDGET` 400, a unit over 440 is refused by name). But a program has **one carry signature** — every layer block reads and
writes the same type — and a feature map changes shape from stage to stage (`[3136, 64]`, `[784, 128]`, `[196, 256]`, `[49, 512]`
at 224 px). The carry is therefore the activation **flattened and zero-padded** to `E` elements, `E` the largest boundary's: a block
`Slice`s its first `rows·C` elements and `Reshape`s them (two nodes), and writes `Reshape`, `Broadcast` of a zero, `Concat` (three).
No arithmetic, and the padding is committed zeros (7/8 of the last block's carry at 224 px). FR-22's per-occurrence carry signatures
would make the padding unnecessary; this needs neither a primitive nor a format change.

**Evidence** (`tests/cnn.rs`). Three tiny HF ResNets with random weights and batch-norm statistics (basic, bottleneck, and a deep one
that needs a layer block, so its carry changes shape between blocks): the float reference equals `transformers` to `10⁻⁶`, the
integer program's cosine to it is 0.9997 – 0.99999 (relative error 0.4 % – 2.6 %) for the feature map and for the pooled vector, the
class's one-stage pipeline binds the image through `JobImage` and gives the program's bytes, and every program is admitted. A network
transformers has no tiny fixture for — a depthwise-separable one with a dilated depthwise convolution, a depthwise convolution with a
bias and no batch norm, an identity residual, and SiLU both fused and standalone — is held against a **naive direct convolution** the
lowering shares nothing with: float `10⁻⁷`, integer cosine 0.9999. **Admission at the real shapes at 224 × 224** (no weights read,
`tir_admit_pipeline_v1` at the open job ceilings): ResNet-18 3 blocks, 364 nodes, 1.81·10⁹ MACs, cone work 935; ResNet-50 5 blocks,
886 nodes, 4.09·10⁹ MACs, cone work 2,284; ResNet-152 12 blocks, 2,620 nodes, 1.15·10¹⁰ MACs, cone work 6,752 (of 65,536) — the
class's own job ceiling decides which of them it takes.

### 9.8 Encoder-decoders beyond text: Whisper, an encoder alone, and the weights stage (`EMBED_FRAMES_CONV1D_V1`, `OUTPUT_ROWS_V1`, FR-18 phase 2)

**What phase 2 was.** The design (`frontend-as-data-v1.md` §3.3) had phase 2 replace the hand-written encoder and decoder block
builders of `lower/encdec.rs` by the generic rows lowerer and an `Op::CrossAttention`. Reading the route before writing any of it
showed that the blocks are no longer per family: since phase 1 five adapters instantiate one `EncDecSpec`, and the blocks are
spec-driven code over the helpers the generic encoder and decoder lowerers use (`linear_rows`, `norm_rows_kind`, `add_rows`,
`softmax_committed`, `lower_attention` with its `RelBias` for the decoder's causal bias, `lower_table_named`). What the family kinds
still lacked were *features of the spec*, not a lowerer, so phase 2 was built as features, all data: an encoder that reads feature
frames, a position table of the encoder's own length, a fixed-length source, a key projection without a bias, and an encoder alone.
A decoder-only text model with cross-attention LAYERS inside its stack (Llama-3.2-Vision's text decoder, FR-21) is a different
thing — the cross-attention would live in the main layer stack's HL — and is not built here.

**Whisper** (`adapters/whisper.json`, data only; no Rust reader of the family). The encoder reads the normalised log-mel
`[bins, 2·L]` as `i16` codes at the fixed unit `2^-13` (a normalised log-mel lies in about `[-1, 1.5]`; the range is ±4), through
two `Conv1d` (`k3 s1`, `k3 s2`, padding 1, a bias and the stack's activation each) — the convolution lowering of §9.7 with a
kernel along the width (`k 1, kw 3`: `ConvOp::conv1d`) — adds the checkpoint's table of positions (1,500 rows: `enc_pos_rows`),
and runs pre-LN layers whose `k_proj` has no bias (`k_bias: false`; the stacked cross keys' offsets are zeros). The source is a
fixed 30 s window: no count input and no mask (`fixed_source`), the decoder reads every one of the encoder's rows. The decoder is
mBART's shape with learned positions at offset 0 and a head tied to the token table. The log-mel front end (STFT, mel filterbank,
log) is NOT in the program: a class needs the frames bound as an input, and the protocol has no audio binding yet
(`Binding::JobAudio`, FR-23) — a consensus-side decision, and the model's two programs run standalone until it exists.

**Evidence** (`tests/whisper.rs`; the tiny model of `tools/gen_hf_encdec_fixtures.py`, frames exact on the code grid). The float
encoder and decoder equal `transformers` to `3·10⁻⁷`; the integer decoder, fed the INTEGER encoder's own cross keys and values,
has logits within 1.3 % of HF's at every position of two streams and agrees on the top-1 id at all 32; the three implementations
are bit-identical and the court replays every commit point of both stages (33 + 210). **Real shapes at 1,500 source rows**
(`tests/configs/encdec/whisper-*.json`, no weights): the decoder of every size is admitted; the encoder of tiny (2.0·10¹⁰ MACs,
1.67 M step leaves) and base (4.8·10¹⁰, 3.30 M) is ADMITTED; small and medium are REFUSED by name (9.75 M and 25.8 M step leaves of
4.19 M: the committed logits `[h, L, L]` of a 1,500-frame attention) and large-v3 by `max_position_macs` (1.29·10¹² of 1.10·10¹²) —
the figures FR-23's research derived independently.

**Levers for the sizes the default ceilings refuse** (measured, deferred by decision: none is built into a class, none needs a
primitive; `tests/whisper.rs::a_longer_tile_admits_whisper_small_and_the_larger_sizes_need_stages` holds the numbers). The
step-leaf count of the encoder is its committed values divided by the admission input `tile_len` (the values one leaf holds: 64 in
`default_inputs`, declared by the class), and the committed logits `[h, L, L]` of a 1,500-frame attention dominate it.

| encoder at 1,500 rows | leaves at tile 64 | at 128 | at 256 | what bounds it |
|---|---|---|---|---|
| tiny / base | 1.67 M / 3.30 M | 0.84 M / 1.65 M | 0.42 M / 0.82 M | admitted at every tile up to 256 |
| small | 9.75 M | 4.87 M | **2.44 M, admitted** | a tile of 512 is refused: `max_tile_transcendentals` 1.54 M of 1.05 M (a tile of the softmax recomputes its rows' exponentials: about `tile · L`), so the longest tile is about 340 |
| medium | 25.8 M | 12.9 M | 6.45 M | refused at every tile the exponentials allow (4.8 M at 340): the committed values themselves must shrink |
| large-v3 | 43 M (est.) | | 10.8 M (est.) | `max_position_macs` 1.29·10¹² of 1.10·10¹² at EVERY tile: a position is the whole encoder |

So: **small** needs nothing but the class declaring `tile_len = 256`; **medium** needs the committed values reduced — a stat-only
softmax (commit the row maximum and sum, recompute the logits inside the tile that reads them: `ATTN_STAT_COMMIT_V1`, MACs for
leaves) or the encoder cut across two stage programs (the leaf cap and the MAC cap are per position, and a stage is a position);
**large-v3** needs the stage split in any case (two stages bring the MACs under 2^40, three the leaves at tile 256) — the pipeline
already runs stage programs in order, and the cut is a layout of the same blocks. None of this changes a primitive or the court.

**An encoder alone** (`adapters/t5-encoder.json`, extends `t5`): `dec_layers: 0`; the final-normed rows are the program's `Final`
output, `i32` at a calibrated power-of-two unit (`OUTPUT_ROWS_V1`). The text encoder of Flux, Stable Diffusion 3, PixArt, Wan and
Sana is this model: cosine 0.9997 against HF on the fixture, court-replayed. T5-XXL's encoder is over the leaf cap at 256 and
512 tokens (`tests/configs/encdec/t5-v1_1-xxl-encoder.json`; FR-18's capacity table).

**The weights stage.** Every encoder-decoder, not only the new ones, now runs the stages the corpus harness runs on a decoder:
the float stages against HF, the integer stages against it, **the three implementations and the court on both stage programs**
(`tests/encdec.rs`: t5, t5_gated, bart, mbart, marian, pegasus — 29–33 commit points of the encoder and 210–246 of the decoder each,
19–20 primitives reached — and the decoder is fed the integer encoder's output). The lowering declares a stage's inputs as params;
the second implementation, the typed backend and the court's demand evaluator all see that version-1 view (the version-2 stage lifts
them), with the inputs as leaves — the evaluator `eval_demanded_v2` is `eval_demanded` over the same view. The same step runs on
every route this branch added: the ResNets, the six vision towers, the nine encoder families and the DSA program
(`tests/{cnn,vision,encoders,dsa}.rs`).

### 9.9 ConvNeXt and MobileNet as data (`NORM_LAYER_V1` over channels, `LAYER_SCALE_FOLD_V1`, `CONV_PAD_TF_SAME_V1`, `ACT_CLAMP_FIXED_UNIT_V1`)

The coordinator's condition was that channel-norm, ReLU6 and HardSwish compose from primitives the lowering already has, as
features, or the family is a prim-set question. They do, and no primitive, runtime or court kernel is added: the family is three
adapters (`adapters/{convnext,mobilenet-v2,mobilenet-v1}.json`, data only) and these features of the `CnnSpec` (all serde
defaults: every earlier spec, adapter and golden byte is unchanged).

* **A LayerNorm over channels** (`CnnOp::ChannelNorm`, ConvNeXt's `LayerNorm` in `channels_first` or `channels_last` form) is the
  encoder's LayerNorm over the rows `[P, C]` the activation already is (`norm_rows_kind`: the exact centring, the Q24 unit row, one
  per-channel narrowing with the gain as its multiplier). The registry feature is the existing `NORM_LAYER_V1`. The pooled output
  through a LayerNorm (`CnnOut::GlobalAvgNorm`, ConvNeXt's `pooler_output`) is the exact mean at the residual unit and the same norm.
* **A layer scale** (`ConvOp.layer_scale`, ConvNeXt's `layer_scale_parameter`) multiplies the output channels of the projection
  that precedes it: `W' = γ·W`, `b' = γ·b`, folded into the weight rows before they are quantised, like a batch norm
  (`LAYER_SCALE_FOLD_V1`: no node).
* **ReLU6** (`Act::Relu6`) is `min(max(x, 0), 6)`. Its range is exact, so its output unit is a FIXED `6/32767` instead of a
  calibrated one and the narrowing's own clamp at 0 and at 32767 *is* the activation (`ACT_CLAMP_FIXED_UNIT_V1`): ReLU6 fused into
  a convolution, standalone, or after a residual sum is a requantisation, not a node. **HardSwish** and **HardSigmoid** are tables
  like every other activation that is not a clamp (`ACT_TABLE_V1`).
* **TensorFlow "SAME" padding** (`ConvOp.tf_same`, MobileNet's `tf_padding`) is asymmetric and depends on the input's extent
  (`max(k − s, 0)` when the extent is a multiple of the stride, else `max(k − extent mod s, 0)`, the smaller half before): the
  window table's content, computed per convolution from the geometry it reads (`CONV_PAD_TF_SAME_V1`). The windows are now
  rectangular with a before and an after per axis (`Win`), which also made the 1-D stem of §9.8 a special case of one code path.

**Structure as data.** `adapters/convnext.json`: the patch convolution and its norm, the stages (a channel norm and a 2x2
stride-2 convolution between them), blocks of a 7x7 depthwise convolution, a channel norm, the pointwise expansion with the
activation, the pointwise projection with the layer scale; `num_stages`, `patch_size` (an int or a pair) and the layer scale's absence
(`layer_scale_init_value <= 0`) are read from the config. `adapters/mobilenet-v2.json`: the stem, sixteen inverted residuals (the
residual only where `stride = 1` and the width is unchanged), the head; HF's `make_divisible(round(c·depth_multiplier))` is written in the
adapter's arithmetic and refuses the one input where its `round` (half away from zero) and Python's (half to even) differ, an exact
half; a dilated backbone (`output_stride` 8 or 16) is refused by name. `adapters/mobilenet-v1.json`: the stem and thirteen
depthwise-separable pairs.

**Evidence** (`tests/cnn.rs`, with `tools/gen_hf_vision_fixtures.py` and `tools/gen_cnn_shapes.py`). Against `transformers` on tiny
models with random weights, bf16-exact: the float reference equals HF to `3·10⁻⁷` (ConvNeXt, MobileNetV1) and `2–9·10⁻⁶` (MobileNetV2);
the integer program's cosine is 0.9997 – 0.9999 for ConvNeXt (one and six layer blocks) and MobileNetV1 and 0.987 – 0.997 for
MobileNetV2; each program is admitted, bound through `JobImage`, and its three implementations are bit-identical with the court
replaying every commit point (45 – 87 for ConvNeXt, 61 – 62 for MobileNetV2, 32 for MobileNetV1). The 0.99 of MobileNetV2 is the **weight
codes**, not the lowering: the weights are `i8` per output row, each convolution adds that rounding, and fifty of them in a row cost
about 1 % in cosine. The same networks with every weight row ON the int8 grid (`*_grid` fixtures, `snap_rows_to_int8_grid`) have none
of it, and their integer programs are within 0.99987 (MobileNetV2) and 1.0000 (ConvNeXt) of HF — what remains is the activations' 16-bit
codes. (A class that wants fifty-layer networks closer than 1 % would take wider weight codes for convolutions: a policy, not a
primitive.) Features HF has no fixture for — TF padding at a stride of 3 over odd extents, HardSwish and HardSigmoid, ReLU6 fused,
standalone and after a residual sum, a channel norm, a layer-scaled convolution, the pooled output through a norm — are held against a
naive direct reference: float `10⁻⁷`, integer cosine 0.99999.

**The real architectures** (no weights): the architecture report reads the real configs of ConvNeXt tiny, base and large-384,
MobileNetV2 at depth multipliers 1.0, 0.75 and 1.4 and MobileNetV1 at 1.0 and 0.75 against the tensor names and SHAPES of the model
transformers' own code builds from each (the meta device: `tests/configs/cnn/*.shapes.json`) — Level B, every tensor accounted for, every
parameter at the shape the lowering needs; this found that a wrapped checkpoint's `num_batches_tracked` carried the alias prefix the
ignore list did not (the real `ResNetForImageClassification` and every MobileNet classifier would have been Level C), fixed in
`hl_program`. **Admission at the real shapes** (`tir_admit_pipeline_v1` at the open job ceilings): ConvNeXt-T at 224 is 7 blocks, 1,796
nodes, 4.35·10⁹ MACs, 431 k step leaves, cone work 4,618; ConvNeXt-B at 224 is 13 blocks, 3,380 nodes, 1.51·10¹⁰ MACs, 931 k leaves;
ConvNeXt-L at 384 is 1.00·10¹¹ MACs and 4.10 M step leaves (**98 % of the 2^22 cap**: the next size up does not fit); MobileNetV2 at 224 is 5
blocks, 829 nodes, 2.8·10⁸ MACs (1.4 depth: 5.5·10⁸), 113 k – 161 k leaves; MobileNetV1 at 224 is 3 blocks, 375 nodes, 5.5·10⁸ MACs, 86 k
leaves — cone work 957 – 8,692 of 65,536.

**The registrant's choice of weight codes** (`LowerOpts::conv_weight_bits`, `lower_cnn_opts`; 8 by default). A convolution's weights are
`i8` per output row unless the registrant asks for `i16` per output row: the same gather and matmul (or product and sum) over wider
integers, the weight parameter's dtype the whole difference — no primitive, no court change (the three implementations and the court
run on the wide programs too; the TIR range analysis and the narrowing's 128-bit product take the larger accumulators). The registrant
chooses fidelity against size: the weight bytes double and the fifty-layer rounding goes.

| tiny MobileNetV2 (fifty convolutions) | `i8` codes (default) | `i16` codes |
|---|---|---|
| integer cosine to transformers, last feature map | 0.987 – 0.992 (min), 0.990 – 0.995 (mean) | 0.999996 – 0.999997 (min) |
| integer cosine, pooled vector | 0.9957, 0.9974 | 0.999999 |
| relative error, feature map / pooled | 1.0 – 1.4·10⁻¹ / 0.7 – 0.9·10⁻¹ | 2.1 – 2.5·10⁻³ / 1.2 – 1.6·10⁻³ |
| convolution weight bytes | 150,656 | 301,312 (exactly 2x) |

(ConvNeXt, which is shallow and normalised, goes from 0.9997 – 0.9999 to 1.0000.) The option lives with the lowering options, not the
calibration policy: the width is the program's parameter dtype, so it is part of the program a pack names (a runtime pack pins it
next to `max_window`). Whisper's two-convolution stem and the vision towers' patch projections keep `i8`; any width but 8 or 16 is
refused by name.

**What this leaves in the family.** MobileNetV3 (squeeze-and-excite: a pooled gate multiplied back over the map), EfficientNet (the same
gate, SiLU, dynamic padding), RegNet (grouped, non-depthwise convolutions) and ConvNeXt V2 (a global response normalisation) each add an
op to the CNN spec composed of the same primitives (a `ReduceSum` over positions, a table, a broadcast `Mul`; block-diagonal `MatMul`s per
group), none a primitive; grouped convolutions are refused by name today. Weight-standardised convolutions and GroupNorm (BiT) are
the same kind of composition.

## 10. The gates that keep it honest

| gate | what it holds |
| --- | --- |
| `tests/golden_lowering.rs` | every lowering is byte-identical to a recorded baseline under both math modes (`std`, `libm-v1`): version-1 *programs* (92 fixtures + 63 real configs, recorded before the generic frontend) and version-2 artifacts (119 fixtures + 63 configs) — a refactor that moves a byte fails; a family that changes on purpose is listed with the commit that explains it |
| `tests/adapters.rs` (`legacy-oracle`) | every family adapter reads the `ModelSpec` the Rust parser it replaced produced, on 224 configs and ~15,000 single-key mutants |
| `tests/feature_registry.rs` | the vocabulary is honest (§3) |
| `tests/{encdec,whisper,cnn,vision,encoders,dsa}.rs` (court) | the weights stage on every route: the three implementations are bit-identical and the court's demand evaluator reproduces every commit point of every position from committed leaves — encoder-decoder stages, Whisper, the CNNs, the vision towers, the encoders, DSA (536 commit points, 144 reductions over `H` dissected) |
| `tests/cnn.rs` | a convolutional network is a data spec: the HF ResNet fixtures (basic, bottleneck, one that crosses a block boundary), ConvNeXt (one and six layer blocks), MobileNetV1/V2 and the same deep networks with weights on the int8 grid (0.9995 floor), a depthwise / dilated / TF-padded / hard-activation network against a naive direct convolution, the real ResNet-18/50/152, ConvNeXt-T/B/L and MobileNetV1/V2 admitted at their sizes, every real tensor accounted for at its real shape (meta-device shapes), the ResNet adapter equal to the Rust structure |
| `tests/qwen4_exp.rs`, `tests/common` | the acceptance matrix for a model that is a combination: float reference ↔ transformers, integer ↔ transformers, the in-program ids ↔ transformers' ids, the program's selected blocks ↔ the indexer's, reference ↔ ref2 ↔ exec on every commit point, the court's demand evaluator reproducing every node of every occurrence (all 25 primitives, state replay, the dissection arithmetic) |
| admission | every fixture admitted; the largest block ≤ 512 nodes, ≤ 16 blocks |
| hardening | the mutated-config sweep and the hostile-number test (§11) |

## 11. Hardening: what a program may declare

A config is untrusted input. Every size a feature reads from it is bounded **before** anything is sized on it, and a refusal names
the bound: rotary dimension ≤ 4,096; hyper-connection streams ≤ 64 with `S·D ≤ 2^24` and rank ≤ 65,536; n-gram order ≤ 8, ≤ 1,024
hash heads, ≤ 65,536 primes to search, tables under 2^32 rows; a convolution ≤ 64 taps over ≤ 4,096 rows; a sparse-attention
block-key matrix and scores ≤ 2^27 elements; a convolutional network ≤ 4,096 ops, kernels ≤ 63, strides ≤ 64, dilations ≤ 32,
paddings ≤ 2,016, ≤ 2^20 channels, feature maps and carried activations ≤ 2^24 positions and elements, window tables ≤ 2^26
entries (`tests/cnn.rs::hostile_numbers_in_a_spec_are_refused_by_arithmetic_not_allocated`); every TIR dimension ≤ 2^24 and every
param ≤ 2^40 elements (NF-8). The tests that
exercise them run under an allocator that aborts on one allocation over 512 MiB, their mutants are small numbers, and each lowered
mutant is sized by arithmetic over what it declares (a deleted key falls back to the class default, which can be the published
size: such a program is lowered, never materialised). The sweep found two panics in code that existed before it (a zero-tap
convolution, a hash base of 0) and an 85 GB allocation reachable through `head_dim = 10^12`.

## 12. What follows

The corpus lane's ranked requests (FR-01 … FR-29) are generic additions to this vocabulary; the order is FR-01 weights as data →
FR-18 encoder–decoders as data → FR-17 rows-mode encoders → FR-19 vision towers and convolutions → FR-02 post-rotation q/k norm →
FR-09 DeepSeek sparse attention. The first two are designed in [`frontend-as-data-v1.md`](frontend-as-data-v1.md).

## 13. Merging tir/generic with tir/onboard (notes for the integration lane)

Merge base `1a4964205`. Lane F's `tests/golden/adapter_pins_v1.json` (on tir/onboard) pins 79 built-in adapters by the hash of the
effective adapter; this branch's pack has **108**. Compared with those pins (computed from the manifest at the branch tip, the
adapters' text unchanged by anything but the commits named):

* **77 unchanged** — the data pack's earlier adapters are byte-stable under this branch.
* **2 changed on purpose**, to be listed in `INTENDED` of `tests/adapter_pins.rs` with the commit that explains them: `qwen4-exp`
  (`fbbef69c6`, the generic feature lowerers: the model now declares the features it uses) and `refusals` (`239179132`, the
  coordinator's decision after CP2: `dbrx`, `granitemoehybrid` and `ernie4-5-moe` are built-in adapters, so the stale Granite-hybrid
  refusal is deleted).
* **29 new**, to be recorded with `PALW_PINS_UPDATE=1` (it refuses to touch a changed row that is not in `INTENDED`): `albert`, `bart`,
  `clip-vision`, `convnext`, `dbrx`, `deberta-v2`, `deepseek-v32`, `encdec-frame`, `ernie4-5-moe`, `granitemoehybrid`, `llava-vision`,
  `marian`, `mbart`, `mixin-bart-lineage`, `mobilenet-v1`, `mobilenet-v2`, `modernbert`, `nomic-bert`, `pegasus`, `qwen2-5-vl-vision`,
  `qwen2-5-vl-vision-in-vlm`, `qwen2-vl-vision`, `qwen2-vl-vision-in-vlm`, `resnet`, `siglip-vision`, `t5`, `t5-encoder`, `vit`, `whisper`.

**The pack hash therefore changed**, and it changes with any adapter edit (it is the hash of the sorted `(id, hash)` pairs): at the tip of
this branch it is `2bfb01c611fecd14fdd4e128672007747190cfc803996234e1e3efd7a1ffd641f80335885e33a87792d48d662776984a305b8dcd74cecc1c84eae5b8fe79ddd0`
(`cargo test --release --test adapters -- --nocapture the_pack_parses` prints the current one). A runtime pack that pinned the pack hash
of an earlier line names an older pack; one that pinned only its own adapter's `{id, hash}` (the 77 and the changed two aside) is
unaffected. The merge needs `tests/adapter_pins.rs` re-run and `adapter_pins_v1.json` updated for the 29, nothing else of lane F's.

What else the merge touches in this crate: `src/adapter/builtin.rs` (the pack list: both sides append ids — keep every id, one line each;
`the_pack_parses_and_every_file_is_listed` fails if a file and the list disagree), `src/model/features.rs` (the registry: ids are unique
and the `an_implemented_features_tests_exist` test names a test that exists), `tests/golden/*` (this branch added the libm and version-2
baselines and moved the seven PLE rows on purpose, listed in `INTENDED`; the golden corpus is `hf`, `hf-quant` and `gguf`, so the
encoder-decoder, CNN, vision and encoder fixtures are not in it, and FR-18 phase 2 and §9.9 moved no golden row), and
`tests/real_configs.rs`. tir/onboard was not edited from this branch.
