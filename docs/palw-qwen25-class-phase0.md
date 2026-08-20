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
| RMSNorm, learned gain | — | **no — G1** |
| q/k/v projection | `MatMulQuant` | **yes** |
| q/k/v bias add | — | **no — G2** |
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

*Category:* resolvable with no new op, by the standard bias-column construction — append a
constant lane to the input row and a bias column to the weight matrix, so `MatMulQuant` computes
`Wx + b` directly. It is exact in integers **provided the constant lane and the bias share the
weight's int8 scale**, which is a quantization-quality question Phase 2 must measure rather than
assume. If the measured bias range does not fit, the fallback is a new registered-vector add op,
i.e. an ADR-0040 extension.

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

*What closing it needs:* (a) `MatMul` takes its second operand from `inputs[1]` when the node
names no weight, (b) the kv length reaches `base0_row`, and (c) a profile-level check that a
`MatMulQuant` node is one of those two shapes and not a third, so an unadjudicable profile is
refused at registration instead of at the first dispute.

### G4 — GQA is not a gap

`PalwShapeProfileV3` carries `attn_kv_heads` separately from `attn_heads`, and the profile's
`input_refs` name the K and V series through `PALW_STEP_INPUT_KV_K` / `_KV_V`. Head grouping is a
question of which cache rows a step's inputs resolve to, which is the profile's to state — no op
is missing. (BASE-0's own profile sets `attn_kv_heads = attn_heads`; Qwen's will not.)

## What Phase 0 concludes

1. `Qwen2.5-2B` does not exist; the work is parameterized and the size choice is the user's.
2. Of the fourteen Qwen2.5 steps, ten map onto BASE-0 ops unchanged.
3. G1, G2 and G3 are **exact PTQ-time transformations** — they need no new consensus op, and each
   must be recorded in the artifact's quantization semantics so a verifier reproduces it. G2's
   exactness is conditional on a measurement Phase 2 owes.
4. **G5 blocks both classes and is the first thing to fix.** Until BASE-0's `MatMulQuant` can
   multiply two activations and the coverage gate can see operand shapes, neither the RC floor nor
   Qwen has an adjudicable attention layer.
