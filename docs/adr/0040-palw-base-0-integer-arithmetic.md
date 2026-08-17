# ADR-0040: `PALW-BASE-0` — the integer-only arithmetic normative specification

Status: **Proposed.** Activates nothing and registers no class. Specifies the arithmetic a
`PALW-BASE-0` implementation must reproduce bit-for-bit, and the op set it is allowed to contain.
Implementation follows this ADR; the class registers only after two independent implementations
agree and its kernel catalog is complete.

Date: 2026-08-17
Relates to: ADR-0039 (which requires this class and puts it FIRST), ADR-0030 (the step space and
shape profile this op set instantiates), ADR-0031 (canonical transcendentals — superseded *for
this class* by having none), ADR-0027/0028 (the court this class exists to make reachable),
`consensus/core/src/palw_step.rs` (`PalwStepOpKindV1`, the 17-kind vocabulary the float classes
need), `consensus/core/src/palw_pwu.rs` (`pwu_per_inference`, which this makes countable).

## Context — why an integer class, and why it is built first

ADR-0039 makes `PALW-BASE-0` the permanent liveness floor and, more importantly, requires that
**no class may carry fork-choice weight until its kernel catalog is complete**. Under optimistic
verification, conviction is the only thing standing between a fabricated block and full weight, so
a class whose lies terminate `Unadjudicable` cannot safely carry weight.

The float classes cannot close their catalog soon. Today it resolves 6 of 17 op kinds, and the
11 outstanding include the ones that carry the computation — `MatMulQuant`, `MatMulF16`,
`SoftMax`, `RopeImrope`. Each needs an exact-bits transcription that must also pin glibc `expf`,
`logf`, `sinf`, `cosf`, FMA contraction, and the reduction order of every threaded sum. That work
is real and slow, and the 2026-08-17 audit found the class identity did not even *name* libm.

An integer class removes the entire category rather than transcribing it.

## Decision A — Integer-only means integer-only

No IEEE-754 value, no `libm` symbol, and no floating-point instruction may appear on the
consensus path of a `PALW-BASE-0` implementation: not in the kernels, not in the scale factors,
not in the activation functions, not in the rotary tables. Scales are integer
`(multiplier, shift)` pairs, never a float. A build that links `libm` for this class is not a
conforming implementation, and the class identity records the absence rather than a version
(contrast the float classes, where ADR-0031 makes glibc's `expf` normative arithmetic *inside the
PoW tag*, and where an unpinned libm is a false-conviction vector — the 2026-08-17 audit's B8).

## Decision B — Representation

```
weights      int8, per-output-channel scale as (multiplier: i32, shift: u8)
activations  int8, per-tensor scale as (multiplier: i32, shift: u8)
accumulator  i32
requantize   an EXPLICIT op (Decision D), never an implicit narrowing
```

Per-channel weight scales and per-tensor activation scales are the shape profile's, frozen at
registration. There is no dynamic (per-inference) rescaling anywhere: a scale computed from the
data would make the arithmetic depend on the data's range, and two implementations that disagree
by one ulp about a range would diverge on everything downstream.

## Decision C — The three arithmetic rules, stated once and used everywhere

**C1. Information is lost in exactly TWO places — `RoundingShiftRight` and `SRDHM` (C2) — and
they round by two DIFFERENT rules, deliberately.**

```
RoundingShiftRight(x, s) -> i32                   // s in 0..=31
    if s == 0 { return x }
    let magnitude = |x|                           // widened; |i32::MIN| does not fit i32
    let rounded   = (magnitude + 2^(s-1)) / 2^s   // exact division; the numerator is >= 0
    if x < 0 { -rounded } else { rounded }
```

`RoundingShiftRight` rounds half **away from zero**: `RSR(3,1) = 2`, `RSR(-3,1) = -2`, symmetric
about zero. Every integer operation *other than these two* is exact, so these are the only places a
value loses information — which is what makes an exact-bits second implementation tractable.

**`SRDHM` (C2) rounds half UP (toward +∞), not half-away — and this is intentional.** Its asymmetric
nudge `1 − 2^30` composed with truncation gives `SRDHM(-1, 2^30) = 0` where half-away would give
`-1`. The two rules diverge on exactly the negative exact-half products (`|a·b| ≡ 2^30 mod 2^31`),
which are freely constructible (take any `b = 2^30`), not statistically rare. `SRDHM` must round
this way because C2's entire purpose is bit-identity with gemmlowp, which rounds half-up; changing
it to half-away to match this heading would break the property C2 exists for. An earlier version of
this heading claimed a single round-half-away rule "and it happens only in `RoundingShiftRight`" —
a third party implementing `SRDHM` from that sentence would have produced half-away and disagreed
with gemmlowp (and with the reference) on every negative exact-half, which under ADR-0027's court is
a conviction, not a rounding difference. `misaka-palw-base0-ref2`'s differential cannot surface
this: both sides derive from the same gemmlowp, so both are half-up; only the normative text was
wrong. Verified independently by exhaustive comparison against the vendored upstream on the
exact-half family.

**Round the MAGNITUDE, then reapply the sign. Do not write `(x ± 2^(s−1)) >> s`.** That form was
this ADR's original pseudocode and the first implementation followed it, and it is wrong for every
negative input: an arithmetic shift floors, so for `x < 0` the nudge and the floor push the same
way instead of opposing. `RSR(−64, 1)` returns `−33` where the exact quotient is `−32` and needs no
rounding at all. Measured against gemmlowp's `RoundingDivideByPOT`, the two disagreed on **50 % of
random `(x, s)` pairs** — every negative one — and the same form overflowed `i32` on a further
3.2 %, wrapping the sign of the largest accumulators. Found by the second implementation
(`misaka-palw-base0-ref2`) on its first run; the rule as *stated* was always correct, only the
pseudocode under it was not.

**C2. `SaturatingRoundingDoublingHighMul` is the one fixed-point multiply.**

```
SRDHM(a: i32, b: i32) -> i32
    if a == i32::MIN && b == i32::MIN { return i32::MAX }   // the single saturating case
    let p: i64 = (a as i64) * (b as i64)                    // a·b — NOT 2·a·b
    let nudge: i64 = if p >= 0 { 1 << 30 } else { 1 - (1 << 30) }
    ((p + nudge) / (1 << 31)) as i32                        // TRUNCATING division, not a shift
```

This is gemmlowp's primitive verbatim, deliberately: it is already implemented identically in
several independent codebases, which is exactly the property a second implementation needs.

**The division truncates toward zero; it is not a shift.** The nudge is asymmetric — `1 − 2^30`
for negatives rather than `−2^30` — for exactly one reason: it compensates for truncation. Pairing
it with an arithmetic shift, which floors, applies the correction twice, and the first
implementation of this ADR did precisely that. Measured against upstream gemmlowp, `>> 31`
disagreed on **50.1 % of random `(a, b)` pairs**, every one a negative product, always one unit
further from zero: `SRDHM(−2^30, 2^30)` returned `−2^29 − 1` where the exact value is `−2^29`.

This mattered out of proportion to its size, and the reason is the paragraph below it: SRDHM was
chosen *because* it is already implemented identically elsewhere. A third party writing BASE-0
against real gemmlowp would have disagreed with the reference on half of all inputs — and under
optimistic verification a systematic disagreement is not a rounding difference, it is a conviction
and a slashed bond.

**The product is `a·b`, not `2·a·b`.** The "doubling" in the name describes the relationship to
the hardware `VQRDMULH` — `(a·b) >> 31` *is* `(2·a·b) >> 32` — it is not a factor to apply on top
of a 31-bit shift. A first draft of this ADR wrote the 2 explicitly and still shifted by 31,
which doubles every product: in Q31, `0.5 × 0.5` returns `0.5` instead of `0.25`, and
`~1.0 × ~1.0` overflows `i32` outright. Recorded because the error was made and because a second
implementation reading only the formula would reproduce it.

**C3. Overflow is impossible at accumulation and saturating at narrowing.**

Accumulation is `i32` and the shape profile must PROVE it cannot overflow. For `int8 × int8`,
`|product| ≤ 127 × 127 = 16_129`, so a dot product of length `K` is bounded by `K × 16_129`, and
the registration-time rule is:

```
K_max × 16_129  ≤  2^31 − 1    ⟹  K_max ≤ 133_144
```

A class whose graph exceeds that must accumulate in `i64` and declare it. Every narrowing
(`i32 → int8`) saturates to `[-128, 127]`; nothing wraps anywhere. Wrapping would turn a
one-unit error into a full-scale one, and — worse for this design — it would break Decision E.

## Decision D — The op set, closed and minimal

`PALW-BASE-0`'s graph is **not the float classes' graph in integers.** Integerising
GatedDeltaNet, interleaved-multimodal RoPE and fused SwiGLU would reproduce the catalog problem
this class exists to escape. The class is a plain decoder-only transformer whose op set is chosen
for closability:

| # | Op | Integer definition |
| --- | --- | --- |
| 0 | `EmbedLookup` | int8 row gather; no arithmetic |
| 1 | `MatMulQuant` | `i32 acc = Σ (int8 × int8)`, exact |
| 2 | `Requantize` | `Saturate8(RoundingShiftRight(SRDHM(acc, mult), shift))` |
| 3 | `RmsNorm` | integer mean of squares (i64 acc), then `IntRsqrt` (Decision F) |
| 4 | `RopeTable` | rotation by a **pinned integer sin/cos table** — see below |
| 5 | `SoftMax` | `IntExp` (Decision F) + integer sum + `IntRecip` |
| 6 | `Silu` | `x · IntSigmoid(x)`, where `IntSigmoid` reuses `IntExp` |
| 7 | `MulElem` | exact `i32` multiply, then `Requantize` |
| 8 | `AddElem` | exact `i32` add (scales pre-aligned at registration) |
| 9 | `Rescale` | `Saturate32(RoundingShiftRight64(acc · mult, shift))` — **added by Decision H**; unlike `Requantize` its gain may exceed 1 |

Ten kinds (nine as first frozen; see Decision H for the tenth and for why the nine could not compute), against the float vocabulary's seventeen. Two absences are deliberate and are the
whole point:

* **`RopeTable` has no `sinf`/`cosf`.** The rotary angles depend only on (position, dimension),
  both of which are bounded by the registered shape — so the table is *precomputed once and
  pinned as a registration artifact*, exactly like the model weights. A transcendental evaluated
  at registration is data; the same transcendental evaluated at inference is normative arithmetic
  that every implementation must reproduce. This converts ADR-0031's hardest surface into a hash.
* **`CpyF32F16` does not exist**, because no cache holds floats.

## Decision E — Reduction order is free, and this is the class's central property

Integer addition is associative and commutative **exactly**, with no rounding and no
contraction. Therefore:

> On `PALW-BASE-0`, the order in which a dot product, a norm sum, or a softmax denominator is
> accumulated **cannot change the result** — across thread counts, SIMD widths, tile shapes,
> compilers, or CPU vendors.

This is the single largest difference from every float class, where reduction order, FMA
contraction and threading are each an independent divergence source and each needs its own pin.
An entire category of cross-host disagreement is not mitigated here; it is *absent*.

**The property is conditional on C3 and that is why C3 is load-bearing.** Saturating addition is
NOT associative (`sat(sat(a+b)+c) ≠ sat(a+sat(b+c))` at the boundary), so associativity holds only
while accumulation cannot overflow. The registration-time bound is not a safety nicety; it is the
premise of Decision E. A class that needs `i64` accumulators must prove the bound there too.

## Decision F — The two integer transcendentals, as algorithms

Both are exact integer algorithms with a **fixed** iteration/term count. Fixed, not
convergence-tested: a loop that stops when it converges stops at different times on different
inputs, and "different times" is a divergence.

Both forms below were validated numerically before this ADR was written, at `k = 24`; the
measured accuracies are quoted so an implementation has a target to differential-test against
rather than a shape to guess at. **Two errors were found and corrected in that pass**, and both
are recorded because each would have shipped an algorithm that silently returns garbage.

**F1. `IntExp(x)` for `x ≤ 0`, in Qk fixed point.** Range-reduce by the pinned integer constant
`LN2_Q = round(ln 2 · 2^k)` (`= 11_629_080` at k = 24):

```
z = min(floor(-x / LN2_Q), Z_MAX)          // integer division, floor
p = x + z · LN2_Q                           // p ∈ (-LN2_Q, 0]
IntExp(x) = RoundingShiftRight(Poly2(p), z)

Poly2(p) = ((A · ((p + B)² >> k)) >> k) + C          // the SHIFTED-SQUARE form
           A = round(0.3585 · 2^k), B = round(1.353 · 2^k), C = round(0.344 · 2^k)
```

`Poly2` is `A(p + B)² + C`, **not** a Horner-form polynomial with coefficients `A, B, C`. The
distinction is the whole algorithm: the shifted-square form gives `Poly2(0) ≈ 1.0003` and
`Poly2(−ln2) ≈ 0.5000`, which are the two endpoints `exp` must hit, while reading the same three
numbers as `c₂p² + c₁p + c₀` gives `Poly2(0) = 0.344` — every result low by a factor of three,
uniformly enough to look like a scale bug rather than a wrong algorithm. *Measured*: max relative
error **0.0048** over `x ∈ [−16, 0]` wherever `exp(x) > 1e−5`, and **0.0021** across a
representative softmax row.

`Z_MAX` is pinned so the shift never exceeds 31; beyond it the result is 0, which is exact enough
because `exp(−Z_MAX·ln2)` is already below the Qk floor. Softmax subtracts the row max first, so
`x ≤ 0` always holds — that subtraction is part of the op, not an optimisation.

**F2. `IntRsqrt(v)` by Newton, with a pinned iteration count.** Normalise `v = m · 2^{2e}` with
`m ∈ [1, 4)`, iterate on `m`, then undo the normalisation by `e`:

```
y_0 = SEED[top-4-bits-of-m]                  // pinned table
y_{i+1} = (y_i · (3·2^k − ((m · ((y_i·y_i) >> k)) >> k))) >> (k + 1)
IntRsqrt(v) = y_N >> e         // N pinned; no early exit, no residual test
```

**The seed table is a correctness requirement, not an optimisation.** Newton for `1/√v`
converges only from `y₀ ≤ √(3/m)`; a seed above that basin diverges *to zero* rather than
oscillating, so the failure is silent and total. A first draft seeded from the leading bit alone
(`y₀ = 2^{(k−bit)/2}`) lands exactly on the boundary at `m = 3` and returned **0**. Every `SEED`
entry must therefore be the reciprocal square root of its bucket's **upper** end, which is
conservative by construction. *Measured with that table*: max relative error **6.4e−6** over
`v ∈ (0, 200]` at `N = 3`, and `N = 4` and `N = 5` are not better — so `N = 3` is the pinned
count, and a larger one buys nothing but time.

`IntRecip` for the softmax denominator is `IntRsqrt` composed with itself, or its own pinned
Newton iteration — chosen at implementation time and then frozen. Either is admissible; drifting
between them is not.

## Decision G — What this buys the catalog

The primitives a second implementation must reproduce are exactly seven:

```
integer add, integer multiply, RoundingShiftRight, RoundingShiftRight64, SRDHM, IntExp, IntRsqrt
```

(seven since Decision H; `RoundingShiftRight64` is the same rule at a wider type)

All six are total functions on `i32`/`i64` with no environment: no rounding mode, no denormals,
no errno, no libm version, no contraction flag. That is why this class's catalog can reach 100 %
while the float classes' cannot, and it is the reason ADR-0039 orders this class first.

`pwu_per_inference` (ADR-0038 D, `palw_pwu`) becomes exactly countable for the same reason: the op
count of one canonical inference is a property of the frozen graph, with no data-dependent
branches to estimate around.

## Decision H — `Rescale`, the tenth op: a scale change that is allowed to amplify

**Amended 2026-08-17, after building the engine. Decision D said nine ops; it is ten.**

### The defect

Decision D's op 2 is `Requantize(acc, mult, shift) = Saturate8(RoundingShiftRight(SRDHM(acc,
mult), shift))`. `SRDHM` *contains* a `>> 31` (C2). With `mult ≤ i32::MAX` and `shift ≥ 0`, the
composition's gain is therefore **at most 1** at every parameter setting — it can only attenuate.

But ops 5 and 6, `SoftMax` and `Silu`, are defined on **Qk** inputs, because both are built on
`IntExp` and `IntExp`'s domain is Qk (F1). And the accumulators that feed them do not reach Qk.
Measured on random `int8` rows:

| reduction | typical \|acc\| | in Qk |
| --- | --- | --- |
| attention logit, `d_head = 64` | 3.6e4 | **0.0022** |
| attention logit, `d_head = 128` | 5.0e4 | 0.0030 |
| FFN gate, `d_model = 2048` | 1.8e5 | 0.0110 |
| FFN down, `d_ff = 8192` | 3.9e5 | 0.0232 |

At those magnitudes the two ops degenerate:

* `SoftMax` over eight such logits returns `0.1248 … 0.1255` against a uniform `0.125` —
  **attention is flat and the keys are indistinguishable**.
* `IntSigmoid` returns `0.501`, so `Silu(x) = x · 0.501` — **SwiGLU's gate is linear and stops
  gating**.

So a conforming BASE-0 implementation, exactly as Decision D froze it, could be executed,
audited, bisected and convicted — and could not compute. The catalog was closed around a graph it
could not express.

This was found by writing the engine (`misaka-palw-base0`), not by review. It is worth recording
why review missed it: every op is individually correct, every op's own tests pass, and the
composition fails only through a *scale* relationship that no single op's contract mentions.

### The repair

`Requantize` composes two shifts, and the `>> 31` inside `SRDHM` is what makes the gain
one-sided. Doing the multiply and the shift **once**, in `i64`, removes that:

```
RoundingShiftRight64(x: i64, s: u8) -> i64      // the C1 rule, at 64 bits
    identical round-half-away-from-zero; C1 still describes ONE rule, at two widths

Rescale(acc: i32, mult: i32, shift: u8) -> i32  // op 9
    Saturate32(RoundingShiftRight64(acc · mult, shift))
```

The gain is `mult · 2^−shift`. Because `mult` is read as a Q31 fraction, **`shift = 31` is unity
and any `shift < 31` amplifies**, up to `2^31`. No new arithmetic concept is introduced: an `i64`
multiply and one rounding shift, both already normative.

`Rescale` is **not** `Requantize` with the clamp removed, and the two are not interchangeable:
`Requantize` rounds twice (at bit 31 inside `SRDHM`, then again at `shift`) and `Rescale` rounds
once, so they differ by up to one unit. `Requantize` keeps its exact frozen behaviour on the
`int8` narrowing path — re-expressing it through `Rescale` would move the value of every
already-pinned narrowing, which is a different and worse change than adding an op.

### Consequences

* The catalog is **ten** ops. Decision D's closability argument is unaffected: `Rescale` is total,
  environment-free, and reproducible from an `i64` multiply and a shift.
* The primitives a second implementation must reproduce become **seven**, adding
  `RoundingShiftRight64` — which is the same rule as `RoundingShiftRight` at a wider type, so the
  differential surface grows by a width, not by an algorithm.
* An artifact must carry the amplifying scales (`attn_logit_scale`, `ffn_gate_scale` in
  `misaka-palw-base0`), and they belong **inside the class digest**: they move every logit and
  every gate, so an artifact whose digest omitted them could be retuned in place while still
  claiming the class.
* The gain targets are calibration, not consensus. Swept and measured for the reference fixture:
  at `2^22` the gate's `|min|/max` is 0.55 — SiLU still near-linear; at `2^23` it is 0.28, which
  is SiLU's own floor of −0.278; above `2^25` the softmax collapses to a hard argmax. `2^23` is
  what the reference artifact uses.
* **A closed catalog is not evidence that the class can compute.** Every property Decision D
  claims held while attention was flat. The engine is therefore instrumented (`ForwardProbe`) so
  attention spread and gate asymmetry are *measured* rather than assumed, and the degenerate
  configuration is pinned as a test rather than left as a comment.

## What this ADR does not decide

* **The model.** Depth, width, vocabulary, the quantized weight artifact and its hash are
  registration inputs, not consensus rules. Post-training quantization of an existing model is
  expected and is a tooling question.
* **The exact constants.** `k`, `Z_MAX`, `LN2_Q`, the `Poly2` triple, `N` and the `SEED` table are
  pinned by the implementation and frozen at registration; this ADR fixes their *form*, the
  accuracy they must reach, and that they are frozen, because a constant that can drift is a
  divergence with extra steps. The `k = 24` figures in F1/F2 are the validated reference point,
  not a mandate — an implementation choosing another `k` re-runs the same measurement.
* **Performance.** BASE-0 is the slow floor by design (ADR-0039 §2a: degrade throughput, never
  difficulty).
* **Whether the float classes are ever completed.** They may remain uncatalogued and therefore
  weightless; that is ADR-0039's rule, not a new decision here.

## Consequences

* **A second implementation is tractable, is required, and has now been built.**
  `misaka-palw-base0-ref2` re-derives all seven primitives with exact `i128` division and **no
  shift operator anywhere**, and `tests/differential.rs` compares them at exact equality over ~3M
  inputs — exhaustive on small windows, complete on the type boundaries, then sampled.

  It earned its cost on the first run, finding three defects that the first implementation's own
  tests had passed: the C1 rounding rule (above), the C2 truncation (above), and a
  `RoundingShiftRight64` that **panicked on overflow** for inputs near `i64`'s ends — a panic
  reachable from a public function, which is the remote-halt failure mode `palw_base0_ops` refuses
  by construction.

  Two lessons generalise. First, all three defects were on **negative or extreme inputs**, and all
  three survived tests that used positive mid-range values; the first implementation's own
  negative cases were all exact halves, where the defective and correct forms happen to agree.
  Second, the second implementation must **re-declare the pinned constants rather than import
  them**: sharing them made a mutation of `RSQRT_ITERS` invisible to the differential, because
  both sides moved together. Constants are specification (F2), so they are compared, not shared.

* **Authorship independence is now held for C1 and C2, and only for those.** Upstream gemmlowp is
  vendored byte-identically at commit `16e8662c34917be0065110bfcd9cc27d30f52fdf` and called through
  an `extern "C"` shim that contains no arithmetic. The specification now agrees with it exactly
  over ~1.8M inputs for `SRDHM` and `RoundingDivideByPOT`, so C2's justification — *that this
  primitive is already implemented identically elsewhere* — is a measured fact rather than an
  assumption. It was not one before: the reference would have been convicting third parties for
  being correct.

  gemmlowp's fixed-point layer is header-only, so nothing of upstream's is compiled and there is no
  build configuration to reproduce — a stronger position than the vendored SoftFloat, where one
  must be. "Byte-identical, no edits ever" is enforced by pinned SHA-256 digests checked at test
  time rather than asserted in a README, because an oracle this project has edited is not an
  oracle.

* **Five primitives still have no third party, and that is now the whole of the gap.** `IntExp`
  (F1), `IntRsqrt` and `IntRecip` (F2), `Rescale` (H) and `RoundingShiftRight64` are this ADR's own
  definitions — the `Poly2` triple and the `IntRsqrt` seed table have no upstream at all. For those
  five, one author wrote both sides, so a misreading of *this document* would be reproduced rather
  than caught. The nearest candidates are the I-BERT reference for `IntExp` and a published
  integer-Newton `rsqrt`. Both are one-way: they can refute, and their agreement would be evidence,
  but neither is normative for a class this ADR defines.
* **ADR-0031 does not apply to this class.** Its canonical-transcendental machinery exists to pin
  libm; there is no libm here. The class identity records "no libm" as a fact rather than
  recording a version — and the `libm_transcribed` registry flag is trivially satisfiable for
  BASE-0 precisely because it binds no libm site.
* **The audit's B8 cannot occur on this class.** Two honest implementations cannot disagree by
  one ulp, so the false-positive conviction that would cost an honest operator a bond and void
  block weight is unrepresentable here — which matters most on the class that is the floor.
* **`PalwStepOpKindV1` gains a BASE-0 arm or its own vocabulary.** Nine kinds that do not map
  onto the float seventeen; whichever encoding is chosen, the two op sets must not share
  discriminants, or a step refutation could name an op the other class never ran.
* **The first honest end-to-end conviction becomes reachable.** With a complete catalog, a lie in
  a BASE-0 matmul is localizable to one step and convictable — which is Layer 1's goal and the
  precondition ADR-0039 puts on carrying weight at all.
