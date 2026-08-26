# ADR-0052: `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid graph

Status: **Proposed.** Registers no class and activates nothing. Specifies the ops a Qwen3.6
implementation must reproduce bit-for-bit, records what has been measured about them, and states
what is still missing before the class could carry weight.

Date: 2026-08-26
Relates to: ADR-0040 (the integer arithmetic this extends), ADR-0047 (the A16 activation tier),
ADR-0039 (which forbids weight without a complete catalog), ADR-0051 (execution families),
ADR-0027/0028/0030 (the court this class does not yet reach),
`consensus/core/src/palw_qwen36_ops.rs`, `misaka-palw-base0/src/qwen36.rs`.

## Context — what ADR-0040 declined, and why the bill came due

ADR-0040 named the three things it was leaving out: "integerising GatedDeltaNet, interleaved
multimodal RoPE and fused SwiGLU would reproduce the catalog problem". That was the right call for
a liveness floor whose whole selling point is a catalog that can close.

It is also exactly the bill for Qwen3.6-35B-A3B. Its forty layers are thirty GatedDeltaNet arms and
ten gated-attention arms, each followed by a 256-expert mixture:

```
layer_types      30 × linear_attention, 10 × full_attention (every 4th)
d_model          2048          vocab 248,320 (padded)
full attention   16 q heads over 2 kv heads, head_dim 256, partial_rotary 0.25 (64 rotated lanes),
                 QK-norm per head before the rotation, attn_output_gate: true
linear attention 16 key heads / 32 value heads @ 128, conv kernel 4, recurrent state per head
MoE              256 experts, top-8 + 1 shared, intermediate 512
```

The court's step space already names `GatedDeltaNet`, `SsmConv`, `RopeImrope`, `Glu` and `L2Norm`
— it was designed for this architecture. What has never existed is the integer arithmetic under
them.

## Decision A — Everything ADR-0040 says still holds

Integer-only, no libm, no float on the execution path. Activations are A16 codes (ADR-0047):
`i16` values in `i32` lanes. Scales are `(multiplier, shift, zero)` triples frozen at registration.
Every lossy site is named and each names its own rule. Reduction order is free, conditional on the
no-overflow bound, which every op below proves at its own entry.

## Decision B — The router is a SELECTION, and its tie rule is normative

Every op in ADR-0040's catalog is a total function of its input row. Two implementations that
disagree, disagree by a value, and the court localises it to one arithmetic step.

A top-8-of-256 router is not that. It makes a discrete choice, and a different choice means a
different expert's weights enter the next matmul — the outputs are then unrelated, no bisection
converges on an arithmetic step, and the disagreement reads as fraud on both sides.

**Ties break to the LOWEST expert index**, the rule the class's argmax already uses. This is not an
exotic case: Q[K] has 24 fractional bits, 256 experts routinely produce probabilities that underflow
it, and on a confident token the tail of the kept set is chosen among exact zeros by the index rule
alone. The kept set is returned in index order, not weight order — the combine is a sum and integer
addition does not care, but the committed row must have one order and index order does not change
when two weights are equal.

## Decision C — The combine has one accumulator

`Σ_e w_e · y_e` in `i64` across all `k` experts, narrowed once. Requantizing per expert and adding
afterwards rounds `k` times and makes the result depend on how the caller grouped the experts.

## Decision D — `IntLn`, the fourth transcendental

ADR-0040 Decision F gives the class `IntExp`, `IntRsqrt` and `IntRecip`. The decay gate is
`exp(−exp(A_log) · softplus(a))`, and with `c = exp(A_log)` and `u = sigmoid(−a)` the identity
`exp(−c·softplus(a)) = (1 + e^a)^(−c) = u^c` turns it into a power. At `c = 1` that is
`int_sigmoid`; at any other `c` it is `exp(c · ln u)`, and there was no logarithm.

`ln x = ln M + s·ln2` with `M ∈ [1, 2)`, and `ln M` by the atanh series in `t = (M−1)/(M+1) ∈
[0, 1/3]`, truncated after `t¹¹` — an error under two units of Q[K]. `t` is an exact `i128`
division rather than `int_recip`, whose three Newton steps would otherwise be the dominant error in
the series' own argument.

**A Newton refinement was tried and removed.** `y ← y − 1 + x·e^(−y)` from `int_exp` and
`int_recip` looked elegant and made the answer **fourteen times worse**: at `x ≈ 0.0045` the series
lands 494 Q[K] units out and the refined result lands 7,202 out. A Newton step squares an error only
when the function it evaluates is more accurate than the estimate it corrects, and `int_exp` is
4e−4 where the series is 5e−6.

## Decision E — The gated delta rule, and why an integer state is stable

```
S ← decay · S          (the gate)
w  = S k               (what the state already predicts for this key)
u  = β · (v − w)       (the correction, in v's units)
S ← S + u kᵀ           (rank-one write)
o  = S q
```

The worry with a recurrence in fixed point is that rounding compounds. It does not, and the reason
is structural: error injected at step `t` is carried forward **through the decay**, so by step `T`
it is worth `decay^(T−t)`. The recurrence is a contraction and its fixed point is not the errors'
fixed point.

**Measured** against an `f64` reference of the same rule: worst relative output error 9.1e−4 at 128
steps, 8.8e−4 at 512, 1.1e−3 at 2048 — flat in the sequence length. A sixteen-fold longer run costs
30 % more error, not sixteen times more.

Two consequences are load-bearing:

* **The gate must be a real multiply**, not a shift. A shift-only decay quantizes the contraction
  rate and, at `decay` near 1, quantizes it to 1 — which is where the argument above stops holding.
* **`‖k‖ = 1` is part of the definition**, not conditioning. The state's magnitude bound is
  `max‖v‖ · β / (1 − decay)`, derived from it, and that bound is what sets how many bits the state
  scale carries above the value scale.

The state's two narrowings are `i64` rather than the tier's `i128` `a16_scale_round`. A decay
touches `d_v · d_k` lanes per head, which at this geometry is 15.7 million narrowings per token; at
`i128` that is the whole token. The bound is proved at the op's entry instead of bought with a
width.

## Decision F — Partial rotation is not an optimisation

`partial_rotary_factor: 0.25` with `head_dim: 256` means 64 rotated lanes and 192 carried through
untouched. The unrotated lanes are position-independent by design; rotating them makes every one a
different number. A full-rotation implementation is a different model, not a slower one.

## What has been measured about the frozen arithmetic

Recorded rather than repaired — these values are the class id, and every op built on them inherits
their accuracy:

| primitive | measured relative error | note |
| --- | --- | --- |
| `int_exp` | up to **3.0e−3** | worst near `x = −0.53`, where its quadratic is weakest |
| `int_rsqrt` | 2.4e−5 near `2^25`, **1.0e−3** at `1e11` | depends on the argument's MAGNITUDE: the answer is Q[K] and a small answer has few significant bits there |
| `int_sigmoid` | 6.8e−5 high at 0, and **not monotone across the origin** | both follow from `int_exp(0)` overshooting `ONE` |

Two callers have to know the second one. `rms_norm` divides by the row length before calling
`int_rsqrt`; `L2Norm` takes the exponent out itself and applies it to the product, which is the
difference between a 0.22 % norm and a 0.02 % one.

And the third one is why `u^c` clamps to `[0, ONE]`: the argument reaches zero whenever `c · ln u`
rounds away, which on real weights is any head whose `dt` is strongly negative, and `int_exp(0)`
sits above `ONE`. The first run on a real checkpoint refused at the very first GatedDeltaNet head
for exactly that reason.

## What this ADR does not decide

**Calibration.** The scales in the converter today are derived from each site's fan-in, which is
the right shape — a random dot product grows like the square root of its length — and is not a
measurement. Fidelity needs a float reference of the hybrid graph and the ranges it produces. Until
then the class runs and its output is not claimed to be faithful.

**The court.** There is no step space for the hybrid graph: no coordinates, no tile leaves, no
refutation prover. `Qwen36Backend` takes the trait's honest defaults for `bisect_prefix_state` and
`refutation_for_index`, and ADR-0039 already says what follows — **no class may carry fork-choice
weight until its kernel catalog is complete**. A Qwen3.6 class registered on this arithmetic is
admissible for liveness and must not carry weight.

**The weight width.** `int8` weights make this model 33 GiB, which is more RAM than most machines
that would produce a block. A four-bit tier with power-of-two group scales would halve it and is
the same Decision E argument; it is not specified here.

## Consequences

* The arithmetic for the hybrid graph exists and is tested, including the recurrence, which was the
  one part that could have made the architecture impossible rather than merely expensive.
* A producer can run Qwen3.6 and commit to it. A court cannot yet adjudicate it, and the gate that
  keeps that from mattering already exists.
* Two ops in this set — the router's tie rule and the decay's clamp — are places where "close" is
  not a degraded answer but a different model. Both are stated as rules rather than left to an
  implementation's judgement.

## Amendment (2026-08-26, same day): the two "not decided" items above are now decided

Both undecided sections closed the same day, by measurement rather than by plan.

### Calibration → the class is faithful, and the check that proved it is now part of the tool

The converter calibrates from a float reference of the hybrid graph over a real prompt, and the
40-layer class measures **cosine 0.9967 / rank correlation 0.9598 / top-1 151/155** against it —
after which text in, text out works ("日本で二番目に高い山は北岳です。", stopped on the stop
token). The decisive instrument was not any per-site cosine: **an architecture error shared by the
reference and the engine is invisible to every differential**, and the whole model sat at chance
(median rank of the true next token 123,653 of 248,320) while every site read 0.99+. The converter
therefore prints, on every run, whether the f32 reference predicts its own calibration text
(top-1 72.1 %, median rank 0 once two tensor-layout misreadings were fixed). `--reference-only`
runs that check in five minutes without writing the artifact.

### The court → the catalog is complete and the admission gate passes

The paragraph above said "no step space: no coordinates, no tile leaves, no refutation prover."
All three exist now:

* **Twenty-three descriptors** — the A16 tier's nine and this graph's fourteen — in
  `palw_step_refute`'s catalog, each naming only its accumulator width, because integer addition
  leaves an op no other degree of freedom. The adjudicator calls the SAME functions the engine
  calls; there is no second implementation to diverge.
* **The profile is projected from an IR that transcribes the engine's own step order**
  (`palw_qwen36_profile`), 48 nodes per recurrent layer and 47 per attention layer. The mixture is
  six nodes: eight experts against one input row are arithmetic-identical to one concatenated
  matrix, and which eight is the committed router row's answer.
* **The recurrence is adjudicated by genesis-anchored replay**, exactly as the float `GdnCore` is:
  five committed rows per position, the court replays the state. The registration-opaque
  checkpoint sentinel stays refused; a registered state chunk map later makes replay
  checkpoint-anchored. The conv window is the last four positions' projections, a per-ref
  position set like the KV arms'.
* **Per-head structure lives inside the kernels** (L2, the wide norm, the recurrence): a one-head
  node would declare a step space that does not contain heads 1..31 — the exact defect the
  `KvPerHead` width fixed for attention.

`the_admission_gate_admits_this_class` runs the whole of `verify_class_admission_v2`'s
deterministic branch: shape validation, both coverage gates, ladder depth, court-cost ceilings and
the PWU recount. Two profile constants were set by that gate, not by preference: **tile_len 512**
(at 256 the worst case is 4,198,428 leaves against the ladder's 2^22 — over by 0.1 %) and
**n_ctx 256** (the step space is linear in context; the runtime's rotary table still covers 512,
and the bound prices the JOB a claim may declare, which the canonical 8+2 job is nowhere near).

`qwen36_registration_v1` derives the profile, catalog entry and genesis-form `ClassRegistered`
from one geometry. The court catalog is consensus — growing it moved testnet-11's
`consensus_params_id`, declared as a coordinated upgrade in the pin's own comment.

### Still open

The producer-side step-leg capture (the material an honest producer answers a bisection with) is
not yet emitted by `Qwen36Engine` — the profile above is its specification. And the weight-width
question (a four-bit tier) remains unspecified, now with the measured note that expert residency
plus the NEON grouped kernels took the 33 GiB class from 0.4 to 1.75 tok/s on a 24 GiB machine.
