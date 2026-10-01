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
- **A table above 2^24 rows is chunked, not dynamic.** The row lookup of a hashed embedding is `⌈size / 2^24⌉` batched `Gather`s
  and a `Select` by the chunk index (§9.4).
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

**Tables above 2^24 rows (NF-8) are a lowering matter, not a capability.** Each hash head owns a contiguous range of the layer's
table, so the table becomes `[heads, rows, dim]` `i16` codes at ONE scale per layer (one `narrow` to the consumer's scale
afterwards), cut into chunks of at most `2^24` rows (`LowerOpts::table_chunk_rows`), **one batched `Gather` (`batch_dims = 1`)
per chunk** and a `Select` by the chunk index. Published sizes (20 M rows a head) take two chunks (`[16, 2^24, 128]` and
`[16, 3,225,183, 128]`; a 196-node PLE block, admitted). Cutting at 16 rows (`PLE-06`, 7 chunks) gives logits bit-identical to the
unchunked lowering's, equal on the three implementations. What *is* large at published size is the artifact (hundreds of GB of
`i16` codes for the PLE layers), a matter of the conversion's streamed fill and of what a registration may carry, not of the IR.

**The streamed fill of a table that large (the hooks lane F's writer calls).** A registration may carry any artifact size:
seat resources gate readiness (staged enablement), never admission, and the preflight reports the seat need
(`palw-class check-architecture` prints `seat need: … GiB`; `artifact_bytes` in its JSON). What stands between the IR and a
published-size `Qwen4-Exp` artifact is therefore the CONVERSION, and it is built from the loader lane F owns (a `TensorSource`
that serves row ranges and a chunked writer), which already generalises to any huge `Gather` table. The hooks this feature
provides, designed here and implemented after the quiet window (they touch `RowParam`, `lower/stream.rs` and `lower/fill.rs`, and each
step has a byte-for-byte test against the whole-tensor path):

1. **A row map.** `RowParam` says which HL rows feed which artifact rows. Today it is the identity (`row r ← row r`); the PLE
   chunks need *runs*: chunk `k` of the layer's table is `[heads, rows_k, dim]` and its row `(h, r)` comes from HL row
   `head_offset[h] + k·chunk_rows + r` — one run per hash head (`dest = h·rows_k`, `src = head_offset[h] + k·chunk_rows`,
   `len = min(rows_k, size_h − k·chunk_rows)`); rows no run covers (past a head's size) are zero.
2. **A table-wide scale as a first pass.** The table's `i16` codes share ONE scale per layer (a per-row scale would need a gather of
   `[heads, rows]` more parameters at every position). `RowKind::TableShared` makes the scale a reduction (`max|x|/32767` over the rows
   the heads use) computed in a first pass over the blocks that keeps nothing but the maximum — the shape of `SplitMain`'s meta pass —
   and handed to the block fills ready-made; the whole-tensor path computes the same number, so the two agree to the byte. The narrowing
   `(m, s, z)` that reads the table's scale takes it from the same pass.
3. **Streaming-friendly by construction.** The lowering never asks for the whole table: the chunk fills read only through the row
   map, the hash's tables (`NgramTables`) are tiny (primes and offsets), and the ids and the table read are already chunked by NF-8 at
   `2^24` rows. The cost at published size is two reads of the rows used (`16 × 20 M × 128` values per layer: ≈ 82 GB of codes written,
   about twice that read per pass from bf16), which is a property of the model, not of the IR.

### 9.5 The gate activations

A gated norm's gate is data (`Op::GatedRmsNorm.act`: SiLU or sigmoid), a table over the `i16` grid like any activation;
`Act::SignedSqrt` (`sign(x)·√max(|x|, 10⁻⁶)`) is one more table. Nothing here needs a primitive.

## 10. The gates that keep it honest

| gate | what it holds |
| --- | --- |
| `tests/golden_lowering.rs` | every lowering is byte-identical to a recorded baseline under both math modes (`std`, `libm-v1`): version-1 *programs* (92 fixtures + 63 real configs, recorded before the generic frontend) and version-2 artifacts (119 fixtures + 63 configs) — a refactor that moves a byte fails; a family that changes on purpose is listed with the commit that explains it |
| `tests/adapters.rs` (`legacy-oracle`) | every family adapter reads the `ModelSpec` the Rust parser it replaced produced, on 224 configs and ~15,000 single-key mutants |
| `tests/feature_registry.rs` | the vocabulary is honest (§3) |
| `tests/qwen4_exp.rs`, `tests/common` | the acceptance matrix for a model that is a combination: float reference ↔ transformers, integer ↔ transformers, the in-program ids ↔ transformers' ids, the program's selected blocks ↔ the indexer's, reference ↔ ref2 ↔ exec on every commit point, the court's demand evaluator reproducing every node of every occurrence (all 25 primitives, state replay, the dissection arithmetic) |
| admission | every fixture admitted; the largest block ≤ 512 nodes, ≤ 16 blocks |
| hardening | the mutated-config sweep and the hostile-number test (§11) |

## 11. Hardening: what a program may declare

A config is untrusted input. Every size a feature reads from it is bounded **before** anything is sized on it, and a refusal names
the bound: rotary dimension ≤ 4,096; hyper-connection streams ≤ 64 with `S·D ≤ 2^24` and rank ≤ 65,536; n-gram order ≤ 8, ≤ 1,024
hash heads, ≤ 65,536 primes to search, tables under 2^32 rows; a convolution ≤ 64 taps over ≤ 4,096 rows; a sparse-attention
block-key matrix and scores ≤ 2^27 elements; every TIR dimension ≤ 2^24 and every param ≤ 2^40 elements (NF-8). The tests that
exercise them run under an allocator that aborts on one allocation over 512 MiB, their mutants are small numbers, and each lowered
mutant is sized by arithmetic over what it declares (a deleted key falls back to the class default, which can be the published
size: such a program is lowered, never materialised). The sweep found two panics in code that existed before it (a zero-tap
convolution, a hash base of 0) and an 85 GB allocation reachable through `head_dim = 10^12`.

## 12. What follows

The corpus lane's ranked requests (FR-01 … FR-29) are generic additions to this vocabulary; the order is FR-01 weights as data →
FR-18 encoder–decoders as data → FR-17 rows-mode encoders → FR-19 vision towers and convolutions → FR-02 post-rotation q/k norm →
FR-09 DeepSeek sparse attention. The first two are designed in [`frontend-as-data-v1.md`](frontend-as-data-v1.md).
