# PALW-BASE-0 PTQ pipeline — scope

**Status:** scoping only. Nothing here is a decision; §9 lists the decisions this document exists
to surface. Measured against `misaka-palw-base0` and `consensus/core/src/palw_base0*.rs` at
`palw-mainnet-rc-integration` HEAD, 2026-08-20.

**Why this exists.** `artifact.rs` states the gap in one sentence — *"Producing weights that are
good — quantising a trained model into `i8` at these shapes — is a separate data pipeline and is
not in this crate."* ADR-0040 agrees and calls it "a tooling question". That pipeline is the last
missing piece of P0-8b once BASE-0 is chosen as the weight-bearing class, and this is its shape.

The central finding is that **the pipeline is not the hard part.** The hard part is that the engine
as written cannot faithfully represent any pretrained transformer, for three reasons that are
architectural rather than numerical (§5), plus one that bounds how large a model can ever be (§6).
All of them are fixable inside the existing kernel catalog — no ADR-0040 amendment — but all of
them change the step space, so they must be settled **before** the `PalwShapeProfileV3` is written,
not after.

---

## 1. What the pipeline must produce

Exactly one value: a `Base0ArtifactV1` accepted by `Base0ArtifactV1::from_parts`, whose
`execution_class_id()` is the class the chain registers. Everything else is upstream tooling.

```
from_parts(shape, embed, unembed, layers, norm_requant, residual_requant) -> Base0ArtifactV1
```

The artifact carries **no scales for the weights themselves**. Weight tensors are raw `i8`; the
scale that makes them mean something is folded into the requantisation parameters. That is the
single most important fact about the format and §3 is its consequence.

---

## 2. The graph the pipeline targets, verbatim

Read out of `engine.rs::forward_token_probed`. This is fixed — the pipeline calibrates *into* it,
it does not get to choose op order.

```
h = EmbedLookup(embed, token)                                   # i8, Q7

per layer:
  normed = Requantize(RmsNorm(h, eps_q), norm_requant)          # Qk -> i8
  q = Requantize(MatMul(wq, normed), requant[0])
  k = Requantize(MatMul(wk, normed), requant[1])
  v = Requantize(MatMul(wv, normed), requant[2])
  per head: q,k = Requantize(RopeTable(·, cos, sin), CODE_CLAMP)
  cache.push(k, v)
  per head:
    raw   = [DotI8(q_h, k_h[j]) for j in history]
    probs = SoftMax(Rescale(raw, attn_logit_scale))              # -> Qk
    p8    = Requantize(probs, QK_TO_CODE)                        # Qk -> i8
    out_i = Requantize(DotI8(p8, v_column_i), CODE_PRODUCT_TO_CODE)
  projected = Requantize(MatMul(wo, attn), requant[3])
  h = Requantize(AddElem(h, projected), residual_requant)

  normed = Requantize(RmsNorm(h, eps_q), norm_requant)
  gate_q = Rescale(MatMul(w_gate, normed), ffn_gate_scale)       # -> Qk
  gate   = Requantize(SiLU(gate_q), QK_TO_CODE)
  up     = Requantize(MatMul(w_up, normed), requant[5])
  gated  = Requantize(MulElem(gate, up), CODE_PRODUCT_TO_CODE)
  down   = Requantize(MatMul(w_down, gated), requant[6])
  h = Requantize(AddElem(h, down), residual_requant)

logits = MatMul(unembed, Requantize(RmsNorm(h, eps_q), norm_requant))   # i32, accumulator scale
```

Notes that matter for calibration:

* `requant[4]` is **unused** — the gate path amplifies through `ffn_gate_scale` instead of
  narrowing. The array keeps the slot so indices line up with projection order.
* `QK_TO_CODE`, `CODE_PRODUCT_TO_CODE` and `CODE_CLAMP` are **engine constants, not artifact
  fields**. The softmax→`i8` narrowing and the attention output narrowing are not calibratable.
* The free parameters are therefore exactly: `requant[0..3,5,6]` and the two `ScaleParams`
  per layer, plus `norm_requant`, `residual_requant`, `eps_q`, `ln_theta_gen_q` globally.
* Greedy decode breaks argmax ties to the **lowest** token id (`argmax_lowest`).

---

## 3. The scale algebra the pipeline solves

Let a weight tensor's real values be `W = w · s_w` with `w : i8`, and let an activation code `a`
at some site stand for real value `A = a · σ`, where `σ` is that site's *code scale*. Activations
are nominally Q7 (`127 ≈ 1.0`) but `σ` is free at every requantisation point, so the useful way to
think about it is a chain of scales the pipeline chooses.

**MatMul then Requantize.** `acc = Σ w·a` is exact `i32`. The real product is `acc · s_w · σ_in`.
Requantize applies gain `g = (mult / 2^31) · 2^-shift`, so

```
σ_out = s_w · σ_in / g          ⇔          g = s_w · σ_in / σ_out
```

**`g ≤ 1` always.** `QuantParams` reads its multiplier as a Q31 fraction and its shift is
unsigned, so requantisation can only attenuate. Amplification is `ScaleParams`/`Rescale`, where
`shift = 31` is unity and **below 31 amplifies**.

**Where absolute scale actually matters.** Most of the graph is scale-covariant and a constant
factor simply propagates into the next `g`. Only four sites pin the scale:

| site | requirement |
|---|---|
| `SoftMax(Rescale(raw, attn_logit_scale))` | logits must arrive at true Qk, or the distribution is uniform (ADR-0040 H) |
| `SiLU(Rescale(·, ffn_gate_scale))` | pre-activation must arrive at true Qk, or `IntSigmoid` returns ≈0.5 and the gate degenerates to `x/2` |
| `AddElem(h, projected)` | **both operands must share one σ** — this is the only *binding* constraint between two different sites |
| `RmsNorm` | **none** — the input scale cancels exactly (`x/rms(x)` is invariant under `x → c·x`), which is why the op takes no input scale |

That last row is a genuine simplification: `norm_requant` can stay a single global parameter and be
correct, because whatever `σ_h` has drifted to, the norm erases it. `residual_requant` cannot —
see §6.

**Do not model `Requantize` in float during calibration.** It rounds twice (at bit 31 inside
`SRDHM`, then again at `shift`) while `Rescale` rounds once; ADR-0040 H records that the two differ
by up to one unit and are not interchangeable. `SRDHM` also rounds half **up** (toward +∞), not
half-away-from-zero, and rounds the magnitude before reapplying the sign. The calibrator must call
the reference implementation, not a `f64` approximation of it.

---

## 4. Hard constraints, read from the code

| constraint | value | source |
|---|---|---|
| `n_layers, n_heads, d_head, d_ff, vocab, max_position` each ≤ | **131,071** | `Base0ShapeV1::validate`, bound = `MAX_DOT_LEN` |
| longest reduction (`max(d_model, d_ff)`) ≤ | **131,071** | same |
| `d_model` | **must equal `n_heads · d_head`** | `Base0ShapeV1::d_model` |
| `d_head` | **must be even** | `validate` (RoPE pairs) |
| grouped-query attention | **not representable** — `wk`/`wv` are `[d_model][d_model]` | `from_parts` length checks |
| RMSNorm gain vector | **does not exist** | `rms_norm(x, eps_q)` takes no weight |
| biases | **do not exist** anywhere | artifact has no bias field |
| attention `1/√d_head` | **not present** — must be absorbed into `attn_logit_scale` | `engine.rs` logits path |
| residual stream precision | **`i8`** — `AddElem` takes `&[i8]` | `palw_base0_ops::add_elem` |
| KV cache | `i8`, full `d_model` per position per layer | `KvCache` |
| tied embeddings | expressible only by **carrying equal bytes twice** | `embed` / `unembed` separate |
| weight codes | full `i8` is legal, `-128` included (see §10) | `MAX_DOT_LEN` premise |

**`vocab ≤ 131,071` is a real filter on tokenizers.** A 151,936-entry vocabulary (Qwen2.5/Qwen3
family) is refused by `validate` outright. Llama-3-class vocabularies (128,256) fit.

---

## 5. Three architectural gaps, and that all three are catalog-compatible

No pretrained transformer loads into the engine as written. The gaps are not quantisation choices:

**5.1 No RMSNorm gain.** Every real RMSNorm has a learned per-channel weight. Two ways out:

* *Fold it into the next matmul.* `W · (norm ⊙ g) = (W · diag(g)) · norm`, and the consumer of a
  normed vector is always a matmul (wq/wk/wv, w_gate/w_up, unembed), so folding is always legal.
  **But** it puts the gain's dynamic range into the weight columns, which is exactly the
  input-channel outlier pattern that per-tensor (and even per-output-channel) quantisation handles
  worst.
* *Represent it explicitly* as `MulElem(normed_code, gain_code)`. `MulElem` is catalog op 7. This
  is the faithful form and it keeps the outliers where a scale can address them.

Recommendation: represent it explicitly. Folding is a quality trap that only shows up after
calibration, when it is expensive to undo.

**5.2 No GQA.** `wk`/`wv` are square. Supporting `n_head_kv < n_heads` is a shape field, a
different `out_dim` on two matmuls, and an index change in the attention loop — **no new kernel**.
Without it, the source model must be MHA, which for current small models means going back a
generation.

**5.3 Per-tensor requantisation only.** The engine calls `requantize_row_uniform`. But
`requantize_row` — taking one `QuantParams` per element — already exists and op 2's own doc
describes it as *"`i32` accumulators → `int8`, per-channel."* Per-output-channel scales are
therefore available **without touching the catalog**; only the engine and the artifact change.
This is the single highest-leverage quality knob available.

**All three change the class id and the step space.** None changes `KERNEL_CATALOG`, so coverage
stays 100% and the A4 gate keeps passing. That is the whole reason to do them here rather than
discover them after the profile is frozen.

---

## 6. The binding risk: an `i8` residual stream with one global scale

`residual_requant` is **one `QuantParams` for the entire artifact**, applied at both residual adds
in every layer. From §3, `σ_h_new = σ_h_old / g_res`. So:

* `g_res = 1/2` (the fixture's choice): σ doubles at every add. Over `2·n_layers` adds the stream's
  scale grows by `2^(2n)` — layer 0's contribution is attenuated to nothing by layer 24.
* `g_res = 1` (unity): σ is constant, and `h + projected` must stay inside `±127` at *every* layer
  or `Saturate8` clips the residual stream.

There is no third option and no per-layer knob. This is the classic reason int8 transformer
implementations keep the residual in wider precision — and BASE-0 cannot, because `AddElem` is
`i8 → i32` by ADR-0040 Decision D. Widening it is a **new kernel**, i.e. the one change that would
re-open the catalog.

**Measured 2026-08-21 — see `docs/palw-base0-depth-measurement-2026-08-21.md`. Two corrections
to the paragraph above.**

*The `g_res = 1/2` claim was wrong.* A feature is not attenuated to nothing: C1 rounds half away
from zero on the magnitude, so `RSR(1, 1) = 1` and the decay `127 → 64 → … → 2 → 1` **floors at ±1
forever**. The residual highway carries ~7 adds ≈ 3.5 layers of magnitude, after which a feature
survives as a sign. Nothing saturates or collapses at 32 layers, at either width.

*The trade is now quantified.* `g_res = 1` needs every layer's residual write attenuated to keep
off the rail — **2–3 bits of the code range at depth 24–32**, and width makes it worse rather than
better. So:

| | residual highway | per-layer write |
|---|---|---|
| gain 1/2 | ~7 adds, then a sign | full 7 bits |
| gain 1 + attenuation | unbounded | 2–3 bits at depth 24–32 |

**Depth is not a wall.** 24–32 layers is arithmetically reachable; the residual binds quality, not
liveness. Mitigation 1 (per-layer `residual_requant`) survives as *useful* rather than *required* —
it places the budget, it does not enlarge it. The only thing that enlarges it is `AddElem` at a
wider type (`i16` buys 10–11 bits per write), which is a new kernel and an ADR-0040 amendment.

---

## 7. Pipeline stages

**Stage 0 — target selection.** Run the §4 table as a checklist against candidate models. The
criteria, not a name: SwiGLU FFN, RMSNorm, RoPE, no biases, `vocab ≤ 133,144`, `d_head` even, and
MHA unless §5.2 is done first. Depth from §6's experiment. Llama-architecture small models are the
natural family; the exact config of any candidate must be checked against the table rather than
assumed.

**Stage 1 — engine and artifact extensions.** §5.1, §5.2 (if needed), §5.3, §6.1. Each is a
`Base0ArtifactV1` field plus an engine edit; each moves `execution_class_id`. Nothing is registered
yet, so the churn is free — *now*, and not after the profile exists.

**Stage 2 — a float twin of the exact graph.** A `f32` implementation of §2's graph, same op order,
same tie-breaks. Not optional: without it there is no way to separate "quantisation error" from
"we built a different model", and every later number is uninterpretable.

**Stage 3 — calibration.** Feed a calibration corpus through the twin; collect per-site
distributions: residual range per layer, each projection's accumulator range, attention-logit
range, gate pre-activation range. Then solve the §3 chain:
`s_w` per tensor (or per output channel, after §5.3) → each `g` → the two `ScaleParams` per layer →
`residual_requant` per layer. Emit `QuantParams` through the reference `requantize`, never a float
model of it.

**Stage 4 — emit and run.** `from_parts` → digest → `Base0Engine::generate`. Compare greedy output
and per-position logits against the twin.

**Stage 5 — acceptance.** §8.

**Stage 6 — the profile.** Only now is `PalwShapeProfileV3` writable against a frozen graph, and
`verify_catalog_coverage_v1` should pass on the first try because every node names a
`KERNEL_CATALOG` descriptor by construction. This is P0-8b step (1).

**Stage 7 — instrumentation.** Per-node tile capture. Under BASE-0 this is emitting tiles from
`engine.rs` — 651 lines of our own Rust — rather than instrumenting llama.cpp kernels. P0-8b
step (2), substantially smaller than it was.

---

## 8. Acceptance gates — the objective function already exists

`ForwardProbe` was built to detect exactly the failures calibration causes, and its doc explains
why each is invisible from the outside (*"a degenerate pass still returns logits, still returns the
same logits every run, and still returns different logits for different weights"*). Use it as the
gate, not as debug output:

| probe | failure it catches | gate |
|---|---|---|
| `attention_spread` | flat softmax — attention selecting nothing | ≫ 0 relative to `ForwardProbe::uniform_probability(n)`, at every `(layer, head)` |
| `gate_extremes` | degenerate SiLU: below its Qk domain `IntSigmoid` → ≈0.5 and SiLU becomes linear `x/2`. The peak alone cannot see this; the **asymmetry** can — a working gate has `\|min\| ≪ max` | asymmetry ratio bounded, per layer |
| `residual_peak` | collapse (all zero) or saturation (pinned at 127) | inside a band, per layer, and non-monotonic in depth |

Plus, above the probe: perplexity on held-out text against the float twin, and greedy top-1
agreement rate over a fixed prompt set.

---

## 9. Decisions this document needs

1. **Source model / target shape** — after §6's depth experiment, not before.
2. **Explicit norm gain vs. folding** (§5.1). Recommendation: explicit `MulElem`.
3. **Whether to add GQA** (§5.2) — decides whether the candidate pool includes current small models
   or only MHA-era ones.
4. **Per-output-channel requantisation** (§5.3). Recommendation: yes; it is free of catalog risk
   and it is the main quality lever.
5. **Per-layer `residual_requant`** (§6). Recommendation: yes.
6. **Pipeline host language.** The calibrator is ordinary ML tooling (PyTorch); the emitter must
   produce bytes `from_parts` accepts. Either Python emits a blob Rust parses, or the calibrator
   exports statistics and a Rust binary does the solve and the emit. The second keeps every call to
   `requantize` on the reference implementation, which §3 argues for.

## 10. Noted in passing — resolved

`MAX_DOT_LEN` was `133_144`, derived from `i32::MAX / (127 · 127)`, while `requantize` clamps to
`[-128, 127]` and can therefore emit `-128`, whose worst-case product is `128 · 128 = 16_384`.
Unreachable at any realistic shape, but not a premise ADR-0040 Decision E could use: it held over a
subset of the operand type while nothing range-checks an artifact's weight bytes.

**Fixed 2026-08-20**: `MAX_DOT_LEN = 131_071`, derived from the whole of `int8`, with ADR-0040 C3
amended to match. Restricting the operand range instead was rejected — it would have changed frozen
catalog op 2. The pipeline therefore does **not** need to avoid `-128`; the full `i8` range is
legal for weight codes.
