# PALW-TIR — weights and routes as data: `WEIGHTS_EXPR_V1` (FR-01), `ENCDEC_FROM_SPEC_V1` (FR-18), and the queue behind them

| Field | Value |
| --- | --- |
| Status | design (RFC-0002 lane G), written during the 2026-10-01 quiet window; FR-01, FR-25 and FR-26 are also written as code (uncompiled until the window lifts, see §9) |
| Source | the corpus lane's `docs/design/palw/tir/feature-requests.md` (branch `tir/corpus`; FR-01 … FR-32), measured on the 100-entry corpus and the census |
| Rule | **permissionless means a registrant needs no Rust.** What is still Rust per family: the enumerated weight layouts of the binder (`QkvLayout` ×4, `MlpLayout` ×4, `GdnLayout` ×2, conv1d, table shards), and three hand-written routes (`lower/bidir.rs` 892 lines, `lower/encdec.rs` 1,677, `lower/vision.rs` 1,440). Each item below removes one of them or shows exactly how |
| Companions | [`generic-frontend-v1.md`](generic-frontend-v1.md) (the pipeline, the feature vocabulary, the Levels), [`model-adapter-v1.md`](model-adapter-v1.md) (the adapter file format) |

## 1. The order, and why it is this order

1. **FR-01 weights as data.** Any model whose *only* novelty is how its tensors are laid out cannot be added without a core
   developer today (`dbrx`, `granitemoehybrid`'s shared expert, `ernie4_5_moe`'s `[1, E]` bias). Everything after depends on
   it: an encoder–decoder, a vision tower or a convolutional net is mostly a *weight table*.
2. **FR-18 encoder–decoders as data** — the biggest hand-written route that is *also* the best specified: five families
   lower today, so the migration has an oracle.
3. **FR-17 rows-mode encoders** — the lowerer FR-18's encoder stage and FR-19's towers both need.
4. **FR-19 vision towers and convolutions** (also the audio feature set's base).
5. **FR-02** post-rotation q/k norm (an ordering flag; two families).
6. **FR-09** DeepSeek sparse attention (a token-level indexer; five families share the scorer).

FR-25 and FR-26 (§6) are the reader's honesty fixes the coordinator accepted; they ride with FR-01 because they touch the same
files and the same report.

## 2. FR-01 — `WEIGHTS_EXPR_V1`

### 2.1 What exists and what is missing

An HL graph names its parameters (`moe.experts.gate`, `attn.q.w`, `mamba2.in.z.w`, …) and declares each one's shape
(`ParamDecl { name, shape, per_layer }`). A **binding** gives every parameter a `weights::Src`: a total, deterministic
expression tree over checkpoint tensors — `Tensor`, `Take { axis, Pick::{Range, Strided, PerLayer} }`, `Transpose`, `Stack`,
`Map`, `Reshape`, `PadRows`, `Quant`. The conversion evaluates it (whole, or by row ranges for the streaming loader), the
shape check compares it with the declaration, and every checkpoint tensor no expression touches is reported unread.

`Src` is already the right abstraction; what is missing is a way for DATA to write one. Today an adapter's `hf` block carries
`names` (role → tensor-name template) and a handful of *enumerated layouts*; `hf_weights::bind` knows what each enumerator
means. A layout the enumeration lacks is a Rust change. FR-01 gives `Src` a surface in the adapter language.

### 2.2 The surface

`spec.hf.weights` is a map **HL parameter name → weight expression**. A weight expression is a JSON array: a *source*, then
*steps* applied in order (a left fold: each step wraps the value so far).

```jsonc
"hf": {
  "names": { … as today … },
  "weights": {
    // dbrx: w1/v1/w2 are flat [E·I, D]; expert e owns rows [e·I, (e+1)·I) — a reshape, no copy
    "moe.experts.gate": ["transformer.blocks.{L}.ffn.experts.mlp.w1", {"reshape": [8, 16, 32]}],
    "moe.experts.up":   ["transformer.blocks.{L}.ffn.experts.mlp.v1", {"reshape": [8, 16, 32]}],
    "moe.experts.down": ["transformer.blocks.{L}.ffn.experts.mlp.w2", {"reshape": [8, 16, 32]}, "transpose"],
    // granitemoehybrid: one fused shared-expert tensor, gate rows first
    "moe.shared.gate.w": ["{p}layers.{L}.shared_mlp.input_linear.weight", {"rows": [0, 32]}],
    "moe.shared.up.w":   ["{p}layers.{L}.shared_mlp.input_linear.weight", {"rows": [32, 32]}],
    // ernie4_5_moe: the correction bias is stored [1, E]
    "moe.sel_bias": ["{p}layers.{L}.mlp.moe_statics.e_score_correction_bias", {"reshape": "param"}]
  }
}
```

(In a real adapter the numbers are `{"$var": …}` expressions: the template is evaluated before it becomes a `ModelSpec`, so
by the time the binder sees the expression every number is a literal.)

**Source.** A string: the checkpoint tensor's **complete** name, with the variables below. No suffix is added (`.weight` is
part of the name when the tensor has it — dbrx's experts are bare parameters and Granite's modules are not).

| variable | bound by |
| --- | --- |
| `{L}` | the param's layer (a per-layer param; refused in a global param's expression) |
| `{E}`, `{H}`, `{S}` and any other single capital but `L` | a `stack` step: `0..count` |
| `{p}` | the adapter's tensor-name prefix (variable `p`), substituted when the spec is built, exactly as in `names` |

**Steps** (each is an exact copy or re-indexing; **no arithmetic on weights except the three named maps**):

| step | meaning | `Src` |
| --- | --- | --- |
| `"transpose"` | swap the last two axes | `Transpose` |
| `{"reshape": [n, …]}` | row-major reshape (element count must match) | `Reshape` |
| `{"reshape": "param"}` | reshape to the declared shape of the parameter being bound (the *squeeze* of `[1, E]` → `[E]`) | `Reshape` (shape from the declaration) |
| `{"rows": [start, len]}` | rows `start..start+len` of axis 0 | `Take { axis: 0, Range }` |
| `{"take": {"axis": a, "start": s, "len": n}}` | a range of axis `a` | `Take { Range }` |
| `{"take": {"axis": a, "block": b, "offset": o, "len": n, "groups": g}}` | for each of `g` groups, `len` indices at `group·block + offset` (NeoX/BLOOM per-head q/k/v interleaving) | `Take { Strided }` |
| `{"take": {"axis": a, "per_layer": n}}` | the layer's own `n` indices (`layer·n …`), a tensor packed over the layers | `Take { PerLayer }` |
| `{"stack": "E", "count": n}` | stack `n` instances of the value so far, with `{E}` bound to `0..n`, on a new axis 0 | `Stack` |
| `{"pad_rows": n}` | flatten the leading axes into rows, append zero rows up to `n` | `PadRows` |
| `{"map": "neg_exp"}` | `−exp(x)` (`A_log` → `A`) | `Map(NegExp)` |
| `{"map": {"scale": c}}` | `x·c` | `Map(Scale)` |
| `{"map": {"rescale_by_layer": n}}` | RWKV's `x / 2^(layer / n)` | `Map(RescaleByLayer)` |

**Not in V1** (nothing in the corpus needs them; each is additive when one does): `concat` (an HL parameter built from
several checkpoint tensors), a general `permute`, a pre-quantised source (a quantised projection keeps its own path,
[`quantfmt`](model-adapter-v1.md); an expression over a quantised module is refused by name), a cast, any weight arithmetic
beyond the three maps. A step key the grammar does not know is an error, never ignored.

### 2.3 Binding: precedence, tolerance, validation

`hf_weights::bind` still derives every parameter's default binding from `names` and the enumerated layouts. **Then** every
parameter that has an entry in `weights` takes the expression instead. Three details make this usable:

- **The default binding of an overridden parameter may be impossible.** An adapter that reads dbrx's experts as expressions
  has no `moe.gate` role for the enumerated `Separate` layout to ask for. The binder therefore treats a role missing from
  `names` as a *marker* while it derives defaults; a default that reaches `put` carrying the marker is an error **unless that
  parameter is overridden**, in which case it is dropped. A parameter that is not overridden keeps exactly today's error
  (`internal: no HF tensor name for role …`).
- **An override of a parameter the program does not have is an error**, naming the key and the declared parameters that share
  its prefix. A typo can never be a silent no-op. (An adapter whose parameter exists only in some configurations
  builds its `weights` map with `$if`, the way it builds `names`.)
- **Expressions are checked like any binding**: `check_weights` evaluates the shape of each against its declaration, and
  the report names a size-1 mismatch with its fix: *"checkpoint gives [1, 8], graph needs [8]: the shapes differ only by size-1
  axes — add `{"reshape": "param"}` to `moe.sel_bias`"*. Shapes are never silently coerced: an implicit squeeze would be the
  same class of defect as a dropped flag (FR-26).

Bounds, because an adapter is untrusted input: at most 16 steps; at most 4,096 override entries; `reshape` dimensions
≤ 2^40 elements in all (NF-8); `stack` count ≤ 65,536; a variable is a single ASCII capital other than `L`. The JSON is
parsed once per binding and the result is an ordinary `Src`.

**Identity.** Expressions are part of the adapter and hence of its canonical hash; two adapters that differ only in a weight
expression have different hashes, and the artifact records the adapter hash it was lowered from.

### 2.4 Streaming

The streaming conversion evaluates an expression by row ranges when `streamable` says it can. The new steps inherit the
existing rule: `rows`/`take` on axis 0 or the last axis, `stack`, `map`, `pad_rows` and a `reshape` that keeps the last axis
stream; `transpose` and a `take` on a middle axis do not (the param is then loaded whole, as every transposed expert tensor
already is). dbrx's gate and up experts (`reshape` keeping `D`) stream; its down experts (`reshape`, then `transpose`) are
whole-tensor, the same as Llama-4's and gpt-oss's. A streaming `Stack ∘ Transpose` would close that; it is an optimisation, not
a semantic.

### 2.5 The enumerated layouts are shorthand for expressions

Every layout `hf_weights.rs` enumerates is an instance of the grammar. This is the check that the expression surface is
*general*, and it is a test (`tests/weights_expr.rs`): for each fixture whose adapter uses an enumerated layout, the
expression form of the same layout parses to a `Src` equal (`==`) to the one `bind` derives.

| enumerated layout | as an expression |
| --- | --- |
| `QkvLayout::FusedConcat` | `["{p}…qkv_proj.weight", {"rows": [0, H·hd]}]`, `[H·hd, KV·hd]`, `[(H+KV)·hd, KV·hd]` |
| `QkvLayout::FusedPerHead` (NeoX, BLOOM) | `{"take": {"axis": 0, "block": 3·hd, "offset": 0, "len": hd, "groups": H}}` and offsets `hd`, `2·hd` |
| `QkvLayout::FusedPerKvGroup` (Falcon, InternLM2) | blocks `(g+2)·hd`, `len` `g·hd` / `hd` / `hd` at offsets `0` / `g·hd` / `(g+1)·hd`, `groups: KV` |
| `MlpLayout::FusedGateFirst` | `{"rows": [0, I]}` and `{"rows": [I, I]}` |
| `MlpLayout::FusedGateFirstInOut` (Llama-4 experts) | `"transpose"`, then `{"take": {"axis": 1, "start": 0, "len": I}}` / `start: I` |
| `MlpLayout::FusedInterleaved` (gpt-oss experts) | `"transpose"`, then `{"take": {"axis": 1, "block": 2, "offset": 0|1, "len": 1, "groups": I}}` |
| `GdnLayout::FusedPerKeyHead` (Qwen3-Next) | strided rows with block `2·dk + 2·rep·dv` |
| GPT-2 `conv1d_weights` | `"transpose"` |
| `table_shards` (Qwen4-Exp) | `{"stack": "S", "count": n}`, `{"pad_rows": R}` |
| Mamba `A_log` | `{"map": "neg_exp"}` |
| RWKV time-mix vectors | `{"reshape": [D]}` |

The enumerations stay as *named shorthands* (a one-word `qkv: "FusedPerHead"` is easier to read than five expressions and the
golden gates pin them); the day one of them is wrong for a model, the model writes the expression instead of waiting for a
release.

### 2.6 Registry and report

`WEIGHTS_EXPR_V1` (area *storage*, `Implemented`, protocol `None`, no primitive: weights are bound at conversion). Detection:
a spec whose `hf.weights` is non-empty lists the feature with the parameter count. `check-architecture` prints the
overridden parameters under the feature (`WEIGHTS_EXPR_V1  SUPPORTED  14 params bound by expression: moe.experts.gate, …`).

### 2.7 Acceptance

`dbrx.json`, `granitemoehybrid.json` and `ernie4_5_moe.json` of the corpus lane pass on the **unpatched** fixtures with their
`names`-only workarounds replaced by `weights` entries (`ernie4_5_moe` also needs the softmax selection bias of §6.3). The
corpus harness is the judge: `PALW_CORPUS_ONLY=<id> cargo test -p misaka-palw-tir-lower --test corpus_v2 corpus_v2_full -- --ignored`
shows `level B` and no failed stage, court included. In this crate: the subsumption test of §2.5, the parser's negative
tests (every bound, every unknown key, a typo'd parameter, a role-less default of a non-overridden parameter), a
shape-hint test, and the streaming equivalence test run over the new expressions.

## 3. FR-18 — encoder–decoders as data (`ENCDEC_FROM_SPEC_V1`)

### 3.1 What the Rust route is

`lower/encdec.rs` is two things at once. **(a)** A *reader*: `parse_encdec` hard-wires five families (T5/mT5, BART, mBART,
Marian, Pegasus) into `EncDecSpec`, and `names`/`sublayers`/`qkvo`/`push_ffn`/`push_embedding` hold each family's tensor
names — data in disguise. **(b)** A *lowering*: synthetic HL programs (one anchor node per block, so the params have a home)
plus hand-written TIR block builders and hand-written float references for the two stages. The model it lowers is a
two-stage pipeline: stage 0, the encoder, ONE position over the padded source axis `L`, whose `Final` output is every decoder
layer's cross-attention keys and values `i16 [D, 2, L, inner]` (the encoder's last rows through all `2·D` projections as one
`MatMul`); stage 1, the decoder, the text stage with one position per target id, reading the stage-0 output through a
per-layer `Gather` and attending over the source axis (a `Fixed` axis, not `H`) with the keys at or past the source length
masked.

Every field of `EncDecSpec` already maps onto a `ModelSpec` field:

| `EncDecSpec` | `ModelSpec` |
| --- | --- |
| `d_kv ≠ d/h` (`enc_head_dim`, `dec_head_dim`) | `AttnSpec.head_dim` |
| `act`, `gated` (`wi_0` gate, `wi_1` up) | `MlpSpec.act`, `gated` |
| `rms`, `bias`, `eps` | `NormSpec { kind, bias, eps }`, `AttnSpec.*_bias` |
| `pre_norm` | `Residual::Sequential` vs `Residual::PostNorm` |
| `positions: Learned { rows, offset: 2 }` | `EmbeddingSpec.positions` with an offset |
| `positions: Sinusoidal` (Marian computed, Pegasus loaded) | `EmbeddingSpec.positions.kind: Sinusoid { computed }` (new) |
| `positions: Relative { buckets, max_distance }` | `Position::Bucketed { kind: T5, buckets, max_distance, scope }` (new, FR-17) |
| `attn_scale` (T5: 1.0) | `AttnSpec.scale` |
| `embed_scale`, `embed_norm`, `final_norm` | existing fields |
| `head_scale` (`d^−½`), `logits_bias` | `HeadSpec.pre_scale` (new), `lm_head_bias` (exists) |
| `decoder_start` | the class template, outside the spec |

### 3.2 The design

**An encoder–decoder is two `ModelSpec`s mirroring the pipeline.**

- The **decoder** is an ordinary causal spec with one new field, `LayerSpec.cross: Option<CrossSpec { attn, norm }>`,
  placed between the mixer and the FFN (the order in all five families); the self-attention is the existing attention.
- The **encoder** is `ModelSpec.encoder: Option<Box<ModelSpec>>`, a bidirectional stage (`AttnSpec.bidirectional`, FR-17)
  whose `output` is a new kind `OutputSpec::CrossKv` — the stacked `xattn.k/v` projections of the decoder's layers,
  as `encdec.rs:1406-1505` builds them today (one `MatMul` against the stacked weights, narrowed per channel to each
  decoder layer's own code scale).
- **Roles** use the decoder's vocabulary (`embed`, `pos_embed`, `embed_norm`, `rel_bias`, `attn.q|k|v|o`, `norm.mix`,
  `xattn.q|k|v|o`, `norm.cross`, `mlp.gate|up|down`, `norm.ffn`, `final_norm`, `lm_head`, `lm_head_bias`), once for each
  stack: `hf.names` for the decoder and `hf.encoder.names` for the encoder. A shared embedding is the same tensor name in
  both. Per-family differences are adapter variables — and, where a layout is odd, FR-01 expressions.
- **Policies, not spec fields.** The wide `q`/`k` (`i16` weights for unscaled scores, which T5 needs) is a lowering policy keyed
  on `scale == 1.0 && !bias`; the stage-0 → stage-1 edge (`StageFinal`, `JobTokens`, `JobTokenCount`) is the program
  interface of RFC-0003 §II.2.1 and stays in the lowerer; `decoder_start` and an mBART target-language prefix are the class
  template's (`TokenSource::Source` for the source ids and a forced decoder prefix are consensus-side, outside the spec).
- **Features** (each registered, each with a fixture generated from transformers' own class): `ATTN_CROSS_V1` (FR-21: the mechanism of
  `mllama`, `idefics`, `t5gemma`, `blt`, `dia`, `canary`, `udop` as well), `POS_SINUSOID_V1`, `POS_RELATIVE_BIAS_V1` (T5 buckets,
  bidirectional or causal, shared or per layer — UMT5 is per layer), `ATTN_BAND_V1` + blocked local attention and
  `ATTN_TRANSIENT_GLOBAL_V1` (LongT5), `OUTPUT_ROWS_V1` (the encoder alone: `t5_encoder`, the text encoder of diffusion pipelines).

### 3.3 Two phases, so that the migration always has an oracle

**Phase 1 — the reader and the weight table become data; the TIR block builders stay.** `parse_encdec` and the name functions are
replaced by an adapter-built `EncDecSpec`: the adapter supplies the spec fields above and the two names maps (plus FR-01
expressions for the stacked cross-attention keys/values, which are `Stack('E') ∘ Reshape` over the decoder layers' tensors — the
expression language already says it). The five families become five adapter files (`t5`, `t5-gated`, `bart`, `mbart`, `marian`;
Pegasus is a Marian variant). The Rust parsers survive behind the same `legacy-oracle` feature as the decoder families' did
(`tests/adapters.rs`): an adapter and its parser must produce equal `EncDecSpec`s on every fixture and ~15,000 single-key
mutants. After Phase 1, **BART/Marian/T5-lineage families that differ only in these fields are data**: `blenderbot`, `m2m_100`/`nllb`,
`plbart`, `mvp`, `pegasus`, `opus-mt` variants, the T5 family (`umt5`, `flan`, `byt5` by config) — the bulk of the ~35 seq2seq
families of 5.17.

**Phase 2 — the block builders become generic lowerers.** The hand-written `encoder_block`/`decoder_block` (and their float
references) are replaced by the generic ones of FR-17 (rows mode: `Linear`/`Norm`/`Act`/`Attention` without history over the
padded axis) and an `Op::CrossAttention` lowerer taken from the existing `decoder_block` code (a `Fixed` source axis, the count
mask, per-layer slices of the stage-0 output). The acceptance is *equal fidelity and equal court coverage*, not byte-identical
programs: same admission verdicts, same float-vs-transformers error budget, 25/25 primitives and every commit point replayed.

### 3.4 What decides registrability is the ceilings, not the lowering

T5-base fits (48 GMAC at 512 source tokens, 2.1 M step leaves of 2^22); v1.1-large is over the leaf cap at 512 and fits at 256;
T5-XXL (the encoder of Flux and SD3) is not registrable as one position at 256 or 512 tokens under the present ceilings. The
levers are per-commit tile lengths (`phase-f-integration.md` F6) and FR-22's per-occurrence carry signatures — admission and
layout choices, **not** primitives. LongT5's point is long input and its `a·L·d` terms alone pass the leaf cap near
`L ≈ 1,300` at `tile_len` 64 for 12 layers; the report says so by name.

### 3.5 Acceptance

`t5`, `t5_gated`, `bart`, `mbart`, `marian` first (the Rust route reproduced by data), then `longt5` and `t5_encoder`.

## 4. FR-17 — rows-mode encoders (`ENC_ROWS_V1`), the shared lowerer

One lowerer over the HL graph the decoder already uses, vectorised over the padded token axis, replaces the three hand-written
layer blocks (`bidir.rs`, `encdec.rs`, `vision.rs`): `Linear` → `linear_rows`, `Norm` → `norm_rows_kind`, `Act`/`Add`/`Mul` → the
existing table and row helpers, `Rope` → the vision route's pinned Q24 `[1, L, dh]` tables generalised to one dimension,
`Attention` without `HistAppend` (`softmax_rows`/`split_softmax`).

**First, the guard (FR-26's class of defect).** `bidir::arch_of` never reads `at.position`, `spec.final_norm`,
`embedding.proj_in`, `embedding.scale`, `softcap` or `clip_qkv`, so a naively written RoPE-encoder adapter lowers to a *wrong
program with no error*. Make `arch_of` a strict allow-list: any unread field that is not its default is a `NOT_LOWERABLE`
naming the field. ALBERT's panic (`Add operands do not broadcast`: the embedding block at hidden width where the factorised
embedding is `E`) becomes a refusal first, then a feature (`EmbeddingSpec.proj_at`).

New spec fields (all additive, `serde(default)`): `AttnSpec.bidirectional` (no causal mask; `window` becomes a *half-width*);
`Position::Bucketed { kind, buckets, max_distance, scope }` (T5 / DeBERTa-log / 2-D tables) and `Position::Disentangled`
(DeBERTa); `EmbeddingSpec.proj_in_bias`, `proj_at` (`AfterLookup | AfterSum | AfterNorm`); `ModelSpec.share_layers`
(layer weight codes become global params, the narrowing params stay per occurrence); `OutputSpec::Rows`.
Registry ids with their extra primitives: `ENC_ROWS_V1`, `ATTN_BIDIR_V1`, `ATTN_BAND_V1`, `ROPE_ROWS_V1`,
`EMBED_PROJ_ORDER_V1`, `LAYER_WEIGHTS_SHARED_V1`, `POS_DISENTANGLED_V1`, `POS_RELATIVE_BIAS_V1`. Order: strict `arch_of`,
spec-driven `bidir` (bidirectional/band, rope rows, gated/pre-norm/RMS/no-bias, `final_norm`, GQA, `share_layers`) so that
ModernBERT, nomic-BERT, jina-v3, EuroBERT, ALBERT and ELECTRA are adapters, then the single rows lowerer, then
`Disentangled`/`Bucketed`. The leaf-cap verdicts of the corpus note (ModernBERT-large over 2^22 at 512 tokens, fitting at 320)
are admission facts, reported by name.

## 5. FR-19 — vision towers and convolutions (`VISION_FROM_SPEC_V1`, `CONV_2D_V1`, `POOL_2D_V1`)

A ViT-style tower is the rows-mode stack of §4 plus a patch embedding (`EmbeddingSpec.patches`: a non-overlapping `Conv2d`, which is
`Reshape`/`Transpose` and one `MatMul`, as `vision.rs:1085-1095` does), learned or `Rope2d` positions, and an output kind (rows, CLS,
mean-pool, attention-pool, merger, projector). Two folds cost **no node**: LayerScale joins the per-output-channel narrowing
multiplier and bias of `attn.o` and `mlp.down`; BatchNorm's scale and shift join the same two numbers (per-row quantisation
absorbs positive row scales, so the codes equal those of the unfolded weights).

A convolution is a `Gather` by a **pinned index table**: `x_ext = Concat([x, zero_row])`, `cols = Gather(x_ext, idx)` →
`[P, taps, C]` → `Reshape [P, taps·C]` → `MatMul` → the usual narrowing (ReLU is the narrowing's `Clamp` with `lo = 0`, BatchNorm folded).
Stride, padding, dilation, asymmetric `same` padding, transposed convolution (a convolution over a zero-stuffed input) and
nearest upsampling are all *index-table content*; depthwise is `Gather` plus a `ReduceSum` over the taps; max-pool gathers with a sentinel
row and `ReduceMax`es; average pool is `ReduceSum` then `Div`. No primitive. The Conv1d of the audio set is the same lowering with one spatial axis
(and is what `CONV_DEPTHWISE_CAUSAL_V1` already is for the causal depthwise case).

What blocks a CNN is the **program format**, not the lowering: one carry signature per program (a feature map that changes shape per
stage needs a fixed worst-case flat carry), tensor rank ≤ 4, ≤ 16 blocks. FR-22's per-occurrence carry signatures lift the first and last;
a worst-case padded carry is the interim. The minimal general primitive, if one were ever wanted, is a strided window view
`Unfold { axis, size, stride, dilation }` (kind S) that removes the index params and makes demand exact — recorded as evidence in the
registry, **not needed now**. ResNet as data is a `ConvNetSpec` whose stages an adapter generates with `$map`/`$range`.

## 6. The reader's honesty items (FR-25, FR-26, FR-07)

### 6.1 FR-25 — a user adapter may override a built-in refusal

`hf_schema::read_model` used to consult `adapters/refusals.json` *before* choosing any adapter, so a refusal could never be
overridden even by an adapter that proves the architecture expressible. **Decision (coordinator, 2026-10-01):** a user-supplied
adapter (`AdapterChoice::Text`) may override a built-in refusal provided it passes the same validation as any adapter, and the
report says so. Implemented as: the refusal applies to `Auto`, `BuiltIn` and `None`; a user adapter proceeds, and the read carries
`overrides_refusal: Some("<architecture>: <the refusal's why>")`; `check-architecture` prints
`user adapter overrides built-in refusal: <the refusal>` and the JSON field. The override is **not a proof of correctness**: the adapter can only emit a `ModelSpec` from the existing vocabulary, so it cannot
name a feature the vocabulary lacks, but it can still read a class wrongly (a sloppy adapter reads defaults). The report therefore
says so (*"the override passed the same validation as any adapter; whether the reading is right is not checked here — confirm with
palw-tir-fidelity"*), exactly as it labels a Level A reading unconfirmed. A refusal that has become stale — a built-in
adapter now exists for the architecture — is **deleted from `refusals.json`** in the commit that adds the adapter, so the data
never contradicts itself: Granite-hybrid's went with the built-in `granitemoehybrid` adapter (together with `dbrx` and `ernie4-5-moe`,
the three corpus entries FR-01 unlocked; coordinator, 2026-10-01); MiniCPM3's stays, as the worked example of an override
(`tests/fixtures/fr25/minicpm3`).

### 6.2 FR-26 — Level A is a guess until a reference says otherwise

The standard decoder template reads a class no adapter claims. Three of the twelve such classes in the corpus were misread by it
(Arcee: gated MLP assumed; Helium: interleaved rotary pairs assumed half-split, a 28 % logit error with no error anywhere in the
pipeline; BitNet: four sub-norm tensors per layer pair unread). So:

1. **A tensor index, when given, is checked.** `analyze` runs the binder on the tensor names (and shapes, when the headers have
   them — `TensorIndex` implements a shapes-only `TensorSource`), and a missing tensor, a shape mismatch or an **unread
   tensor** is a Level C finding (`MissingItem`: "N checkpoint tensors the reading never uses — a feature the template does not
   know") and `NOT_LOWERABLE`. This is the check that caught Arcee and BitNet.
2. **Level A is labelled `A (unconfirmed)`** unless a reference check passed (`reference_confirmed`, set by the tools that ran
   `float_vs_hf` on the same weights). The human report prints *"rope pairing, norm placement and MLP gating are class code,
   not configuration"*; the JSON carries `level_label`.
3. **No flag is dropped.** A spec flag the lowerer would not apply is a refusal naming the flag (§6.3), and a panic on a legal spec
   is a bug that becomes a refusal.

### 6.3 FR-07 — the selection bias under softmax routing, and the guard

`RouterSpec.selection_bias` was applied to the *choice* scores of sigmoid routers only; a softmax router with the flag silently
dropped it (ERNIE-4.5-MoE selects `topk(softmax(logits) + e_score_correction_bias)` and weights the selected experts by the
**unbiased** probabilities, renormalised by `max(Σ, moe_norm_min)`). Fixed on both sides: the float reference and `lower_route`
add the bias to the choice scores for `Scoring::Softmax` as for `Sigmoid` (one `Add` before the `TopK`; the weights stay the
unbiased probabilities, and group masking uses `−∞` rather than `0` when a bias is present — a negative biased score must not
lose to a masked expert). The guard: `selection_bias` with `TopKThenSoftmax`/`TopKThenSigmoid`/`SparseMixer`, and `jitter_eps ≠ 0`
with anything but `SparseMixer`, are `NOT_LOWERABLE` at HL build time, naming the flag.

## 7. FR-02 and FR-09

**FR-02 `ATTN_QK_NORM_POST_ROPE_V1`.** `AttnSpec.qk_norm_after_rope: bool` (default false, additive; on the attention rather than on `QkNorm`
because it orders the *pair* of q/k norms against the rotation). Hunyuan applies the per-head RMS norms
*after* the rotation (`q = rope(q); q = RMSNorm_head(q)`); Qwen3's order is the reverse. Rotation preserves the L2 norm but the
per-channel gain does not commute with it, so the orders differ (measured: 8.9e-2 and 4.6e-2 of the logit scale on the two
fixtures). The HL builder places the existing per-head norm node after `Rope` when the flag is set; the lowerer needs no new code
for the node itself, but the **history keeps the normed, rotated k** (the rotated k is what it keeps today, so only the
node order changes) and `features()` reports the flag. No primitive. Acceptance: `hunyuan_v1_dense` and `hunyuan_v1_moe` (adapters
exist; flip `after_rope`). Hunyuan's `rope_type: dynamic` with `alpha` is a *static* NTK (`base = θ·α^(d/(d−2))`), expressible by
`$pow` in the adapter; beyond `max_position_embeddings` HF recomputes with the factor-only rule and drops `alpha`, an inconsistency
inside HF — lower the window only.

**FR-09 `ATTN_TOKEN_INDEXER_V1` — DeepSeek sparse attention.** Not `ATTN_SPARSE_BLOCK_V1` at block size 1: that feature scores *block
means* of raw keys with the layer input as the query and an unweighted head sum; DSA scores *tokens* with a latent query
(`wq_b(q_a_layernorm(q_a x))`), a LayerNorm'd one-head key (cached per position) and learned per-head weights
`w = weights_proj(x)·Hi^−½`: `s_t = Σ_h w_h·ReLU(Di^−½ q_h·k_t)`. The selection is the top `min(index_topk, T)` positions, ties to the
lowest index (the IR rule). A `TopK` needs a `Fixed` axis (04b §6.6) and `TopK`/`Gather` along `H` would break "dissectability is
structural" (§10.3), so the selection is an exact **top-k mask by counting**: scores `κ_t = (s_t + off)·2^b + (2^b − 1 − t)` are
distinct, so the k-th largest `τ'` is unique and is found by a 16-ary radix search of `⌈(31 + b)/4⌉ = 12` passes of
`ReduceSum_H(Compare(κ, cand, Ge))`; `τ'` is committed (two `i32` lanes; `τ' = 0` when `H < k`); `vis = Compare(κ, τ', Ge)` selects
exactly `k` positions; `Select(vis, logits, MIN)` precedes the library softmax. The selection is masked-dense — **no compute saving**
over dense MLA, only the exact function. When `max_window ≤ index_topk` the indexer is vacuous and plain MLA is lowered with no
indexer nodes (exact, and Level B at once). A court tile for the `τ'` cone has 12 reductions over `H` (O-1 allows 16); the root
claim carries `V = 12·15 = 180` values (O-5 allows 4,096). Because five families reuse the scorer (`glm_moe_dsa`, `hy_v4`, `axk2`,
`glm5_next`, `qwen4_exp`'s block variant is different), it is built once as a library template over `MlaSpec.indexer`. A
prototype is the first step (a hand-checked 12-pass search over a small `H` against `torch.topk` on the fixture's own scores).

## 8. How every item is verified

Every request in this document is judged by the same four gates and nothing else: **(1)** the float reference equals the
transformers class on the item's fixtures (`float_vs_hf`); **(2)** the integer program agrees with transformers within the
quantisation budget and with the three implementations bit for bit (reference ↔ ref2 ↔ exec); **(3)** the court's demand evaluator
reproduces every node of every occurrence (all 25 primitives, state replay, dissection arithmetic) and admission accepts the
program under the network's ceilings (or refuses it by name); **(4)** the regression gates — golden lowering under both math modes,
the adapter oracle, the feature registry — hold, with any family that moves on purpose listed with its commit. A request that
cannot pass all four is not a feature yet; it is a Level C finding with the general primitive named.

## 9. State of the code (2026-10-01, quiet window)

The quiet window (the release drill needs ≥ 20 GB of RAM) forbids building and testing, so the code below was **written without
being compiled or run**; the first action after the window lifts is a build, then each item's tests, then the full release suite.

| item | written | tests (to run first) |
| --- | --- | --- |
| FR-01 `WEIGHTS_EXPR_V1` | `weights/expr.rs`, the binder's override and tolerance, `{p}` inside expressions, the size-1 hint | `tests/weights_expr.rs` (re-laid dbrx/granite/ernie storage; every built-in `Src` round-trips; the three corpus families from their tensor headers), `weights::expr` unit tests |
| FR-25, FR-26 | `hf_schema::read_model` (override), `model::report` (label, tensor-index check), buffers are not weights | `tests/architecture_report.rs` (+3) |
| FR-07 | `float_ref::route`, `lower::lower_route`, the HL guard | the corpus entry `ernie4_5_moe` once its fixture runs; the golden gates must not move |
| FR-02 | `AttnSpec.qk_norm_after_rope`, the HL order | `tests/qk_norm_post_rope.rs`; the corpus entries `hunyuan_v1_dense`, `hunyuan_v1_moe` |
| FR-18 Phase 1 | `EncDecSpec` as data (`EncDecNames`, `family_names`, validation), adapters of kind `encdec` (`encdec-frame`, `mixin-bart-lineage`, `t5`, `bart`, `mbart`, `marian`, `pegasus`), `hf_schema::read_encdec`, the report, the SDK's two-stage check | `tests/encdec_adapters.rs` (the oracle: adapter = `parse_encdec` on every fixture, real config and single-key mutant), `tests/encdec.rs` unchanged and green |
| FR-17 step 1 / FR-26 | `lower::bidir::arch_of` is a strict allow-list: a partial or interleaved rope, learned positions with rope, an unread attention feature is `NOT_LOWERABLE` naming the field, never a wrong program | `tests/encoders.rs::the_bidirectional_lowering_refuses_a_spec_field_it_does_not_read`; the encoder fixtures must still lower |
| FR-17 steps 2-4 (built 2026-10-01) | the encoder rows lowering is per layer kind and spec-driven: rotate_half rope tables per position, gated MLP, pre-norm or post-LN (a norm absent where the checkpoint has none), optional biases, final norm, band window (`ENC_BAND_WINDOW_V1`), ALBERT's factorised embedding (`EMBED_PROJ_IN_AFTER_NORM_V1`, a biased projection after the norm; the shared layer group is bound to every layer) and DeBERTa's disentangled attention (`ATTN_DISENTANGLED_V1`: a log-bucketed relative table, normed, projected by the layer's own key and query weights stored once, c2p and p2c read through batched gathers). Adapters `nomic-bert`, `modernbert`, `albert`, `deberta-v2` are data only | `tests/encoders.rs::{nomic_bert,modernbert,albert,deberta_v2}_*` (float against transformers 2e-7; integer cosine 0.9998-0.99996; admitted), the real configurations of ModernBERT-base, nomic-embed-v1.5, ALBERT-base and DeBERTa-v3-base admitted at 128/256/512 tokens (cone work at most 1,582 of 65,536) |
| FR-19, ViT family (built 2026-10-01) | `VisionSpec` is serde data; adapters of kind `vision` (`clip-vision`, `siglip-vision` — equal to `parse_vision_rust` on every field — and `vit`, data only, with a `Rows` output beside the class embedding); `hf_schema::read_vision`, the report and `palw-tir-check` for towers; features `VISION_FROM_SPEC_V1`, `EMBED_PATCH_CONV_V1`, `EMBED_CLS_TOKEN_V1`, `HEAD_POOL_ATTENTION_V1`, `OUTPUT_ROWS_NORMED_V1` | `tests/vision.rs::{the_vision_adapters_agree_with_the_rust_reader,vit_class_embedding_and_rows_match_their_hf_fixture}` |
| coordinator decisions (2026-10-01) | built-in `dbrx`, `granitemoehybrid`, `ernie4-5-moe` (the stale Granite-hybrid refusal and its registry entry deleted; MiniCPM3 is the worked override example); `--source-len`/`--target-len` and the seat need (`artifact_bytes`) in `palw-class check-architecture` | `tests/weights_expr.rs` (the pack reads them like the files), `tests/architecture_report.rs` (FR-25 on MiniCPM3) |
| built (2026-10-01) | the PLE streamed-fill hooks (row map, table-wide scale as a first pass; `generic-frontend-v1.md` §9.4) for lane F's writer: `RowKind::Mapped` | byte-for-byte against the whole-tensor path (`tests/streaming_convert.rs`, all 84 fixtures), `tests/streaming_ple.rs` |
| built (2026-10-01) | FR-02, FR-07, FR-25/26, FR-29 (`HEAD_TRANSFORM_V1`), FR-35 (`ROPE_REVERSED_V1`), FR-17 step 1, FR-18 Phase 1, lane D's lowerers as features | `tests/{weights_expr,qk_norm_post_rope,architecture_report,head_transform,rope_reversed,encdec_adapters,encoders,encdec}.rs`; the full release suite and the golden gates |
| FR-09 (built 2026-10-02) | `ATTN_TOKEN_INDEXER_V1`: `MlaSpec.indexer`, the `deepseek-v32` adapter, the indexer's HL nodes and float reference, the selection as an exact mask by counting (`lower/dsa.rs`: a 16-ary radix search for the k-th largest composite key, `B/4 <= 10` reductions over `H` in one cone, a committed lane pair, ties to the lowest index of the window), a DSA layer as a mixer half and an FFN half | `tests/dsa.rs` (float = transformers to 1e-6 with the IR's tie rule; integer KL 3e-5 against transformers; the selection applied at k = 1..10; reference = ref2 = exec; streamed = whole; the real V3.2 shape admitted at windows 2^13 and 2^16), `lower::dsa::tests` (the radix search = a sort), `generic-frontend-v1.md` §9.6 |
| the residency contract (built 2026-10-02) | the n-gram table is one axis-0 `[rows, dim]` param per hash head and chunk, each read by `Gather { axis: 0, batch_dims: 0 }` of the param itself (it was one batched gather over `[heads, rows, dim]`, a dense use to lane M2's residency); the writer API (`RowKind::Mapped`, `RowMap`, `FillCtx` hooks, `materialise_stream`, `TensorSink`) documented as final in `generic-frontend-v1.md` §9.4 | `tests/row_addressing.rs` (M2's own classifier, vendored: at Qwen4-Exp's published shape 750 GiB of weights, 2.55 GiB pinned, floor 3.77 GiB), `tests/golden_lowering.rs` (`INTENDED`: the seven PLE rows), `tests/streaming_ple.rs`, `tests/qwen4_exp.rs` |
| not started | FR-18 Phase 2, FR-19 convolutional towers (ResNet) and the Qwen2-VL / Qwen2.5-VL / LLaVA towers as adapters | — |

Until the build, the commit is a design with a reviewed draft, nothing more.
