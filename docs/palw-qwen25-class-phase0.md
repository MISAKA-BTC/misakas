# Qwen2.5 as a PALW deterministic execution class — Phase 0

**What this is:** the measured architecture, the op-by-op diff against `PALW-BASE-0`, and the
blockers that must be answered before Phase 1. Nothing here is from memory: every number was read
from Hugging Face's own `config.json` and from the `safetensors` header of the real weight file
on 2026-08-21.

---

## Blocker 0 — `Qwen2.5-2B` does not exist

Measured, not assumed:

```
GET https://huggingface.co/api/models/Qwen/Qwen2.5-2B    → {"error":"Invalid username or password."}
GET https://huggingface.co/api/models/Qwen/Qwen2.5-1.5B  → {"id":"Qwen/Qwen2.5-1.5B","private":false,…}
```

Hugging Face returns that error for repositories that do not exist. The Qwen2.5 dense base family
is **0.5B, 1.5B, 3B, 7B, 14B, 32B, 72B** — there is no 2B.

**Category:** the goal names a model that is not available, so its "actual architecture/config"
cannot be fixed. **Not simplified away.**

**How Phase 0 proceeds anyway.** All three small members are byte-identically the same
architecture (`Qwen2ForCausalLM`, `model_type: qwen2`); they differ only in geometry. So every
structural question this phase asks has one answer for all of them, and the size choice changes
concrete numbers, not the design. The work below is parameterized by geometry for that reason.

**The substitution this document assumes, stated so it can be overruled:** `Qwen2.5-1.5B`
(1.54 B params) as the nearest existing member to "2B", and the smaller of the two candidates
that bracket it — which matters because the artifact must be produced, hashed and shipped.
`Qwen2.5-3B` is the other reading. **This choice is the user's, not mine**; if 3B is intended,
only the geometry constants move.

## The measured architecture

All values from `config.json`. Identical across the three sizes unless a column differs.

| | 0.5B | **1.5B** | 3B |
| --- | --- | --- | --- |
| `hidden_size` | 896 | **1536** | 2048 |
| `intermediate_size` | 4864 | **8960** | 11008 |
| `num_hidden_layers` | 24 | **28** | 36 |
| `num_attention_heads` | 14 | **12** | 16 |
| `num_key_value_heads` | 2 | **2** | 2 |
| head dim (`hidden/heads`) | 64 | **128** | 128 |
| GQA group | 7 | **6** | 8 |
| `vocab_size` | 151936 | 151936 | 151936 |
| `rms_norm_eps` | 1e-06 | 1e-06 | 1e-06 |
| `rope_theta` | 1e6 | 1e6 | 1e6 |
| `hidden_act` | silu | silu | silu |
| `tie_word_embeddings` | true | true | true |
| `torch_dtype` | bfloat16 | bfloat16 | bfloat16 |

## The measured tensor table (1.5B)

Read from the `safetensors` header — 338 tensors, **every one BF16**:

| tensor | count | shape |
| --- | --- | --- |
| `model.embed_tokens.weight` | 1 | (151936, 1536) |
| `model.layers.{L}.input_layernorm.weight` | 28 | (1536,) |
| `model.layers.{L}.self_attn.q_proj.weight` | 28 | (1536, 1536) |
| `model.layers.{L}.self_attn.q_proj.bias` | 28 | (1536,) |
| `model.layers.{L}.self_attn.k_proj.weight` | 28 | (256, 1536) |
| `model.layers.{L}.self_attn.k_proj.bias` | 28 | (256,) |
| `model.layers.{L}.self_attn.v_proj.weight` | 28 | (256, 1536) |
| `model.layers.{L}.self_attn.v_proj.bias` | 28 | (256,) |
| `model.layers.{L}.self_attn.o_proj.weight` | 28 | (1536, 1536) |
| `model.layers.{L}.post_attention_layernorm.weight` | 28 | (1536,) |
| `model.layers.{L}.mlp.gate_proj.weight` | 28 | (8960, 1536) |
| `model.layers.{L}.mlp.up_proj.weight` | 28 | (8960, 1536) |
| `model.layers.{L}.mlp.down_proj.weight` | 28 | (1536, 8960) |
| `model.norm.weight` | 1 | (1536,) |

Three facts the table settles that a config file does not:

* **QKV carry a bias; O and the MLP do not.** Q/K/V each have a `.bias`; `o_proj`, `gate_proj`,
  `up_proj` and `down_proj` have none.
* **There is no `lm_head.weight`.** `tie_word_embeddings: true` is real — the output projection
  reuses the embedding matrix.
* **kv width is 256** = 2 kv heads × 128, against a query width of 1536 = 12 × 128.

## Op-by-op against `PALW-BASE-0`

BASE-0's closed set (ADR-0040 Decision D + H): `EmbedLookup`, `MatMulQuant`, `Requantize`,
`RmsNorm`, `RopeTable`, `SoftMax`, `Silu`, `MulElem`, `AddElem`, `Rescale`.

| Qwen2.5 step | BASE-0 op | Expressible? |
| --- | --- | --- |
| embedding gather | `EmbedLookup` | **yes** |
| RMSNorm, normalize | `RmsNorm` | **yes** |
| RMSNorm, learned gain | — | **no — G1** (exact fold) |
| q/k/v projection | `MatMulQuant` | **yes** |
| q/k/v bias add | — | **no — G2** (needs an ADR amendment) |
| RoPE | `RopeTable` | **convention mismatch — G3** |
| Q·Kᵀ scores | `MatMulQuant` | **no — G5** |
| 1/√d scaling | `Rescale` | **yes** |
| causal softmax | `SoftMax` | **yes** (causality is the row width, not a mask op) |
| P·V | `MatMulQuant` | **no — G5** |
| o_proj, down_proj | `MatMulQuant` | **yes** |
| residual adds | `AddElem` | **yes** (both operands are node outputs) |
| SwiGLU (`silu(gate)⊙up`) | `Silu` + `MulElem` | **yes** (both operands are node outputs) |
| final norm + tied lm_head | `RmsNorm` + `MatMulQuant` | **yes**, given G1 |
| int8 requantization between stages | `Requantize` / `Rescale` | **yes** |

### G1 — `RmsNorm` has no gain

`Base0Op::RmsNorm` computes `rms_norm(x, eps_q)` and takes no weight. `MulElem` and `AddElem`
both require **two opened tiles** (`need(2)`), so neither can multiply a row by a *registered
vector*. Qwen applies a learned per-channel gain after every norm.

*Category:* resolvable at PTQ time, exactly, with no new op. A gain followed by a linear layer is
`W·diag(g)·x`, so `diag(g)` folds into `W`. Every norm in Qwen2.5 is consumed only by linear
layers — `input_layernorm` by q/k/v (all three see the same gain, so all three absorb the same
`diag(g)`), `post_attention_layernorm` by gate/up, `model.norm` by the tied lm_head. **This is an
exact algebraic transformation, not a simplification**, and it must be recorded in the artifact's
quantization semantics so a verifier can reproduce it.

### G2 — no op adds a registered vector (the QKV bias)

Same cause as G1 on the additive side.

*Category:* **BASE-0 semantics の拡張が必要.** *(Corrected — the first draft of this document said
"resolvable with no new op" and that is wrong.)*

The bias-column construction — append a constant lane to the input row, a bias column to the
weight — is exact and needs no new *kernel*. But it needs a row with that constant lane in it,
and **nothing in BASE-0 can produce one**. `rms_norm(x, eps)` returns exactly `x.len()` values;
there is no "append a constant" op; and the constant cannot be prepended before the norm, because
the norm sums over the whole row and would fold it into the scale. Checked, not assumed:
`QuantParams` is `{ multiplier, shift }` and `ScaleParams` likewise — **there is no zero-point
anywhere in the op set**, so no additive registered term exists in any of the ten ops.

So Qwen2.5 cannot be expressed in BASE-0's op set as it stands, and the blocker is the QKV bias.
Two amendments would close it:

1. **`QuantParams` gains a zero point**, so `Requantize` becomes
   `Saturate8(RoundingShiftRight(SRDHM(acc, mult), shift) + zero)`. This is the standard int8
   inference form, and its absence is itself worth noting — asymmetric quantization normally
   carries one. It closes the bias as a by-product of a change that also improves quantization
   quality, and it touches one op rather than adding one.
2. **A new registered-vector add.** Narrower, but it is an eleventh op in a set whose closedness
   is the class's selling point, and every op added is another kernel every implementation must
   reproduce bit-for-bit.

(1) is the smaller change to the *catalog* and the larger change to an *existing* op's semantics;
(2) is the reverse. **The choice is an ADR-0040 amendment.** Note that G1 is unaffected — a
multiplicative gain folds exactly into the next matmul and needs neither.

### G3 — RoPE convention

`KDESC_BASE0_ROPE` is `base0/rope/pinned-table-pairwise/v1`. Qwen2 is NEOX-style `rotate_half`:
the head dimension splits in half and pairs `(i, i + d/2)`, where pairwise pairs `(2i, 2i+1)`.
These are different permutations of the same rotation set.

*Category:* resolvable at PTQ time by folding the fixed permutation into the q and k projection
rows — exact, and it leaves the adjudicated kernel untouched, which is the point.

### G5 — **BASE-0 cannot adjudicate attention at all**

The blocking one, and it is not about Qwen.

`Base0Op::MatMul` resolves its second operand **only** through
`weights.weight_row(node.weight_name, …)` and returns `Unadjudicable` when the oracle has
nothing. Attention has no registered weight: Q·Kᵀ multiplies an activation by the **K cache**,
and P·V multiplies probabilities by the **V cache**. It also refuses `PalwStepOutLenV1::KvScaled`
outright — "the kv-scaled width needs the true kv length of THIS step, which the adjudicator does
not hold here" — and attention scores are exactly that shape.

So both attention matmuls are structurally unadjudicable, and **the BASE-0 shape profile shipped
on 2026-08-20 declares two such nodes per layer.** It passes `validate_shape` and it passes the
kernel-id coverage gate, because that gate compares IDs and never asks whether the kernel can
serve the node's operand shape. A class can therefore be certified "100% covered" while two of
its eighteen per-layer nodes can never be recomputed — which is the exact failure mode the
coverage gate exists to prevent.

*Category:* **実装不足** — not an ADR gap. ADR-0040 Decision D defines `MatMulQuant` as
`i32 acc = Σ (int8 × int8)`, exact, and says nothing about one operand being a weight; the
restriction is the adjudicator's. The information the refusal claims to lack is available one
frame up: the caller holds the coordinate, and `canonical_tile_values` already derives the kv
length from it.

*What closing it needs, and what turned out to be underneath.* (a) and (b) landed: `MatMul`
takes its second operand from `inputs[1]` when the node names no weight, and the kv length
reaches `base0_row` from the coordinate the caller already holds. (c) landed as
`kernel_can_serve_node_v1` + `verify_profile_coverage_v1` — coverage now asks the adjudicator,
node by node, whether it can serve that node's shape.

**And that gate immediately found that (a) and (b) do not reach attention.**
`canonical_input_leaves` answers `None` for `PALW_STEP_INPUT_KV_K` and `_KV_V` — its own comment
says "KV / checkpoint arms: registration-opaque today" — and a `None` there is `Unadjudicable`
before any kernel runs. So the attention nodes are unadjudicable for a second, independent
reason: the court cannot NAME the leaves a challenger would have to open.

Measured on the shipped BASE-0 profile, the gate refuses four of its twenty-one nodes:

| node | reason | status |
| --- | --- | --- |
| `attn/9` | a `Requantize` kernel with no registered parameters | **authoring bug, fixed** |
| `attn/5` (Q·Kᵀ) | KV sentinel is registration-opaque | **closed — G5c** |
| `attn/7` (P·V) | KV sentinel is registration-opaque | **closed — G5c** |
| `pre/0` (embedding) | `Embed` needs one input row; the pre table has no upstream | **closed — G5d (prefill)** |

### G5c — `canonical_input_leaves` could not name the KV series — **closed**

*Category:* **実装不足**, and the closure did not need what it first looked like it needed.

The KV aux series (`PalwKvChunkLeafV1`) is **f16** — `2 x head_dim x position_count` little-endian
f16 bytes — and ADR-0040 Decision D forbids a float cache outright ("`CpyF32F16` does not exist,
because no cache holds floats"). So that series could never carry an integer class's cache, and
mapping the sentinels onto it would have been wrong in the one direction that matters.

The cache contents are **already ordinary step tiles**. The K and V projection nodes carry
`KCacheWrite` / `VCacheWrite` and commit their output at every position — which is what those
roles are for. So the sentinel resolves to whichever node of this layer's table holds the
matching role, read over the position history, with no new leaf format and no float series.

The one structural change: the position set is a property of the input REF, not of the node. An
attention step reads its query at the CURRENT position and the cached keys at EVERY position up
to it, which a node-wide `required_positions` cannot express — and that is why the sentinels were
left opaque. The KV path groups ref-major (one concatenated row per input, which is the
`out_dim x in_dim` matrix `MatMulQuant` wants); the GDN path keeps its position-major grouping
untouched, because `gdn_core` consumes five rows per prior position and reordering them would be
a different program.

Two refusals guard the resolution: a layer with **no** node carrying the role names nothing, and a
layer with **two** makes "the K cache" ambiguous — a court that had to choose would be choosing
its own evidence. `palw_v2_the_kv_sentinels_resolve_to_the_cache_nodes_over_the_history` asserts
which node is named and which positions are spanned, not merely that a function returned `Some`.

### G5d — the embedding gather (**closed for prefill; decode is the remaining half**)

*Category:* **BASE-0 semantics の拡張が必要**, and it was extended rather than routed around.

`Base0Op::Embed` returned the identity of `inputs[0]` — an admission that a real gather could not
be checked — which also forced the node to declare an input row a pre table has no upstream to
supply. Checking one needs the TOKEN ID: the fault a court reads is "the committed tile differs
from the correct computation", and for a gather the correct computation is `token_embd[t]`. A
challenger may open any row; that proves fraud only if `t` is the right token. **The requirement
is irreducible**, so no amount of making the kernel cleverer closes it.

**Closure (1), implemented.** `PalwExecutionStepRefutationV1` carries `prompt_token_ids`, matched
against the job context's `prompt_token_ids_hash` **before a single one is read**, and the gather
reads the registered table at `token * width`. Unchecked ids would make the gather a false-slash
machine: the ids decide what "correct" means for the step, so a challenger would name whichever
ids make an honest producer's committed row look wrong. The node takes no opened row, which is
also what removes the pre-table-has-no-upstream problem.

**Decode is refused, deliberately.** A decode token is whatever the model generated, so it is in
no prompt and pinned by nothing here; a challenger naming it freely would convict an honest
producer, which is the one failure this court may never have. `base0_row` returns `Unadjudicable`
for `call_index != 0`.

**What that leaves open, stated plainly:** a producer may write decode-position embeddings freely,
because no dispute can address them. That is narrower than before — every position was
unadjudicable — but it is a hole, and it is the remaining half of G5d. **Closure (2)** shuts it:
a decode token is the argmax of the previous position's logits, which is a committed tile, so the
court can derive it from opened leaves. It costs an argmax in the adjudicator and a cross-position
dependency the step space does not otherwise have, which is why it is an ADR decision and not
this patch. It is not a coverage failure — the node's SHAPE is servable, which is what the gate
decides — so it will not be caught by a gate and must be tracked here.

### G4 — GQA is not a gap

`PalwShapeProfileV3` carries `attn_kv_heads` separately from `attn_heads`, and the profile's
`input_refs` name the K and V series through `PALW_STEP_INPUT_KV_K` / `_KV_V`. Head grouping is a
question of which cache rows a step's inputs resolve to, which is the profile's to state — no op
is missing. (BASE-0's own profile sets `attn_kv_heads = attn_heads`; Qwen's will not.)

## What Phase 0 concludes

1. `Qwen2.5-2B` does not exist; the work is parameterized and the size choice is the user's.
2. Of the fourteen Qwen2.5 steps, ten map onto BASE-0 ops unchanged.
3. G1 and G3 are **exact PTQ-time transformations** — a multiplicative norm gain folds into the
   next matmul, and RoPE's convention is a fixed permutation folded into the q/k rows. Neither
   needs a new consensus op; both must be recorded in the artifact's quantization semantics so a
   verifier reproduces them.
4. **G2 does need an ADR-0040 amendment** (corrected from this document's first draft). The
   bias-column construction needs a row carrying a constant lane, and no BASE-0 op produces one —
   there is no zero point in `QuantParams` and no additive registered term anywhere in the ten
   ops. Adding a zero point to `Requantize` is the smaller catalog change; a new vector-add op is
   the smaller semantic change. Either is an amendment.
5. **G5a/b/c are closed.** Attention is adjudicable now: `MatMulQuant` multiplies two activations,
   `KvScaled` widths are derived, and the KV sentinels name the cache-role nodes over the position
   history. The coverage gate asks what a kernel can SERVE rather than whether its id is listed,
   so an unadjudicable class is refused at registration.
6. **G5d is closed for prefill.** The refutation carries hash-checked prompt ids and the gather
   reads the registered table at the token's offset. Decode positions are refused, so a producer
   may still write decode embeddings freely — narrower than before, still a hole, and closure (2)
   (argmax of the previous position's committed logits) is the ADR decision that shuts it.
7. **Coverage is 100% on BASE-0's graph** — 21 of 21 nodes servable, checked against the
   adjudicator itself rather than a restated list.

## What Phase 1 needs before it can start

Phase 1 ("1-layer Qwen2.5 deterministic reference execution") is not blocked by G5d — a reference
executor can run without a court — so it can begin. What it cannot do is *register* the class.
The ordering that follows from this phase:

* Two ADR decisions are on the critical path: G2's additive term and G5d's token ids. The first
  gates whether Qwen is expressible at all; the second for both classes and should be made first, because
  it may change the step space (closure 2 adds a cross-position dependency).
* The size question (1.5B or 3B) gates the artifact but nothing before it.
* G2's bias-column exactness is a measurement Phase 2 owes before the artifact format freezes.
