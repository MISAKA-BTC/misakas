# PALW spec — 04a. Integer arithmetic (BASE-0 and its tiers)

> **Normative, transcribed.** The sections below are the Decision sections of ADR-0040, ADR-0031,
> ADR-0047, ADR-0050 and ADR-0052, copied verbatim on 2026-09-27. Their headings are demoted by one level,
> and relative links are rewritten to resolve from here. Those ADRs were written as the arithmetic's
> normative specification, so their decision text is kept whole rather than paraphrased. Their
> rationale and measurements stay in the archived texts under
> [design/palw/archive/](../../design/palw/archive/). The implementation is authoritative:
> `consensus/core/src/palw_base0.rs`, `palw_base0_ops.rs`, `palw_base0_a16.rs`, `palw_qwen36_ops.rs`
> and `palw_transcendental.rs`. Any difference between the text and the code goes into
> [divergences.md](divergences.md). This file is part of chapter [04](04-execution-semantics.md).

Known at transcription (2026-09-27):

- `palw_base0_ops.rs` names the op set "nine ops" (ADR-0040 D), and ADR-0040 H adds `Rescale` as the
  tenth. The code carries `rescale_row`. Reconcile the count in Phase 2's divergence pass.

## ADR-0040 Decisions A–F: the BASE-0 integer arithmetic

*Source:* [0040-palw-base-0-integer-arithmetic.md](../../adr/0040-palw-base-0-integer-arithmetic.md) (full text in [archive](../../design/palw/archive/0040-palw-base-0-integer-arithmetic.md)).

### Decision A — Integer-only means integer-only

No IEEE-754 value, no `libm` symbol, and no floating-point instruction may appear on the
consensus path of a `PALW-BASE-0` implementation: not in the kernels, not in the scale factors,
not in the activation functions, not in the rotary tables. Scales are integer
`(multiplier, shift)` pairs, never a float. A build that links `libm` for this class is not a
conforming implementation, and the class identity records the absence rather than a version
(contrast the float classes, where ADR-0031 makes glibc's `expf` normative arithmetic *inside the
PoW tag*, and where an unpinned libm is a false-conviction vector — the 2026-08-17 audit's B8).

### Decision B — Representation

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

### Decision C — The three arithmetic rules, stated once and used everywhere

**C1. Every site that loses information is named, and each names its own rule. There is more than
one rule, and that is deliberate.**

| site | rule |
| --- | --- |
| `RoundingShiftRight` (below) | round half **away from zero** |
| `SRDHM` (C2) | round half **up** (toward +∞) — gemmlowp's |
| the internal `>> k` inside `RmsNorm`, `RopeTable`, `IntExp`, `IntRsqrt`, `Rescale` | **floor** (arithmetic shift, toward −∞) |

Everything else — the `int8 × int8` products, the accumulations, the adds — is exact.

An earlier version of this heading claimed a single round-half-away rule happening "only in
`RoundingShiftRight`", with "every other integer operation is exact". Both clauses were false, in
opposite directions, and each was load-bearing for someone: a third party implementing `SRDHM` from
the first clause rounds half-away and is convicted (see C2), and one reading the second clause looks
for exactness in the `>> k` steps that in fact floor. The table is the contract; the prose below
gives each rule's reason.

```
RoundingShiftRight(x, s) -> i32                   // s in 0..=31
    if s == 0 { return x }
    let magnitude = |x|                           // widened; |i32::MIN| does not fit i32
    let rounded   = (magnitude + 2^(s-1)) / 2^s   // exact division; the numerator is >= 0
    if x < 0 { -rounded } else { rounded }
```

`RoundingShiftRight` rounds half **away from zero**: `RSR(3,1) = 2`, `RSR(-3,1) = -2`, symmetric
about zero. What makes an exact-bits second implementation tractable is not that there is one rule,
but that the set of lossy sites is closed and each one's rule is pinned — a `>> k` that floors is
just as reproducible as a rounding one, provided the specification says which it is.

**`SRDHM` (C2) rounds half UP (toward +∞), not half-away — and this is intentional.** Its asymmetric
nudge `1 − 2^30` composed with truncation gives `SRDHM(-1, 2^30) = 0` where half-away would give
`-1`. The two rules diverge on exactly the negative exact-half products (`|a·b| ≡ 2^30 mod 2^31`),
which are freely constructible (take any `b = 2^30`), not statistically rare. `SRDHM` must round
this way because C2's entire purpose is bit-identity with gemmlowp, which rounds half-up; changing
it to half-away to match the other rule would break the property C2 exists for. A third party
implementing `SRDHM` from the old single-rule heading would have produced half-away and disagreed
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
(`misaka-palw-base0-ref2`) on its first run. Note the two failures are distinct: *this* one was the
pseudocode contradicting a correctly-stated rule (half-away for `RoundingShiftRight`), whereas the
heading's "one rule, one site" universal was the statement itself being wrong.

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

Accumulation is `i32` and the shape profile must PROVE it cannot overflow. Every narrowing
(`i32 → int8`) saturates to `[-128, 127]`; nothing wraps anywhere. Wrapping would turn a
one-unit error into a full-scale one, and — worse for this design — it would break Decision E.

The operand type is therefore the whole of `int8`, so `|product| ≤ 128 × 128 = 16_384`, a dot
product of length `K` is bounded by `K × 16_384`, and the registration-time rule is:

```
K_max × 16_384  ≤  2^31 − 1    ⟹  K_max ≤ 131_071
```

A class whose graph exceeds that must accumulate in `i64` and declare it.

**Amended 2026-08-20: this clause derived the bound from `127 × 127 = 16_129` and gave
`K_max ≤ 133_144`.** That contradicted the saturation sentence directly above it in the same
clause — the narrowing produces `-128`, and `(-128)²` is wider than `127²`. The wrong figure was
never reachable (`d_model` and `d_ff` are thousands, not a hundred thousand) but it was not a
premise Decision E could use: it held over a subset of the operand type, while nothing range-checks
an artifact's weight bytes and the refutation path decodes operands with `i8::try_from`. Narrowing
the operand range to `[-127, 127]` instead was considered and rejected — it would change frozen
catalog op 2 for every input that currently saturates to `-128`, and would leave every entry point
that does not pass through `Requantize` still open. `MAX_DOT_LEN` is `131_071`.

### Decision D — The op set, closed and minimal

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

### Decision E — Reduction order is free, and this is the class's central property

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

### Decision F — The two integer transcendentals, as algorithms

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

## ADR-0040 Decision H: `Rescale`, the tenth op

*Source:* [0040-palw-base-0-integer-arithmetic.md](../../adr/0040-palw-base-0-integer-arithmetic.md) (full text in [archive](../../design/palw/archive/0040-palw-base-0-integer-arithmetic.md)).

### Decision H — `Rescale`, the tenth op: a scale change that is allowed to amplify

**Amended 2026-08-17, after building the engine. Decision D said nine ops; it is ten.**

#### The defect

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

#### The repair

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

#### Consequences

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

## ADR-0031: canonical transcendentals

*Source:* [0031-palw-canonical-transcendentals.md](../../adr/0031-palw-canonical-transcendentals.md) (kept as a short record).

### Decision

* Transcendental identity = `transcendental_algorithm_id_v1(descriptor)` (ADR-0030's
  domain). Descriptors in the catalog today:
  - `source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1` — the vector exp, per-element.
  - `source-poly/ggml-v-silu/llama-030ebb558/per-lane/v1` — `x / (1 + v_expf(0 − x))`, true
    divide, `0 − x` transcribed literally (not negation).
  - `libm/glibc-2.39/expf/{fma,nofma}/v1`, `libm/glibc-2.39/logf/{fma,nofma}/v1` — the
    scalar sites (sigmoid `1/(1+expf(−x))`, softplus `x>20 ? x : logf(1+expf(x))`, the GDN
    decay `expf(g)`, vector-op tails). One of `{fma,nofma}` per class, by disassembly.
  - `libm/glibc-2.39/sinf/…`, `…/cosf/…` — **reserved, unimplemented** (RoPE gate).
* Programs are written in ruleset-v2 arithmetic only (soft f32/f64; integer bit steps are
  native integers), live in `consensus/core/src/palw_transcendental.rs`, and are frozen by
  golden vectors. New algorithm = new descriptor = new id; never an edit.
* NaN policy: committed bytes are finite (fail-closed), so a transcendental's NaN handling
  is transiently observable at most; programs canonicalize NaN outputs like the ruleset does
  and the divergence from glibc's payload-preserving `x + x` is recorded as unobservable in
  adjudication.
* Validation: local twins (hardware-fma expression mirror for v_expf; loose ≤1-ulp envelope
  against the host libm for the glibc programs — the host is Apple, glibc's 0.502-ulp budget
  makes exact agreement wrong to demand) now; **exact-bits differential against the fleet's
  actual `libm.so.6`/compiled `vec.h` on the class hosts is the ADR-0030 §5.1 registration
  gate** — a program that has not run against its kernel is not a candidate id.

## ADR-0047: the A16 activation tier

*Source:* [0047-palw-a16-activation-tier.md](../../adr/0047-palw-a16-activation-tier.md) (kept as a short record).

### Decision

1. **The A16 op set** (`palw_base0_a16`): `MatMulRequant`, `MatMulRequantRow`, `MatMulRescale`,
   `RmsNorm`, `Requant`, `AddElem`, `MulElem`, `Rope` — i8 weights per row, i16 activation
   codes, `i64` accumulation. Decision E carries to this width: `|w·x| ≤ 127·32767 < 2^22`,
   `A16_MAX_DOT_LEN` keeps every accumulation exact in `i64`, and the differential tests run
   interleaved/blocked/reversed reductions bit-identical.
2. **The boundary rule: `i64` never crosses a step boundary.** Step tiles ride 4-byte lanes, so
   every op whose intermediate exceeds 32 bits is FUSED with its narrowing — committed rows are
   A16 codes or Q24 `i32`, and the wide accumulator lives and dies inside one adjudicable op.
   This is why the tier's matmuls are fused rather than a bare dot, and why the committed output
   row (and therefore the class argmax, lowest index on ties) is defined over i16 logit CODES.
3. **Reuse over redefinition:** `rope_table`'s pinned table (head-tiled at the oracle; the tier's
   own `Rope` adds only the i16 saturation), `silu` (defined on the Q24 values `MatMulRescale`
   commits), the embedding gather, and op 5W (`softmax_shifted`) — which this ADR also
   registers in the catalog (`base0/softmax-shifted/...`, `up_bits` as a one-byte oracle row).
4. **One parameter store.** Every runtime parameter is an integer `(m: i64, shift ≤ 62,
   zero: i64)` triple (17 bytes on the wire), derived at conversion, held in the artifact's
   `a16_params` keyed exactly as the shape profile names them, digest-covered. The engine
   pre-resolves its tables FROM those bytes; the dispute oracle serves THOSE bytes. What the
   engine ran and what the court recomputes with cannot come apart.
5. **The sink lane is court-visible.** At position zero, a parameter name resolves with the
   `.sink0` suffix first (generic row on absence) — in `a16_row` and in the engine alike.
6. **The registry keeps its single-source invariant.** `KERNEL_CATALOG` holds the descriptors;
   `catalogued_kernel_ids_v1` (the coverage gate) and `a16_row` (the court) read the same table;
   `kernel_can_serve_node_v1` gained the tier's shape arms. `recompute_step_row_v1` exposes the
   dispatch for external verifiers.

## ADR-0050 Decisions A–D: the residual site

*Source:* [0050-palw-base0-residual-site.md](../../adr/0050-palw-base0-residual-site.md) (full text in [archive](../../design/palw/archive/0050-palw-base0-residual-site.md)).

### Decision A — the residual site gains its narrowing node, and this is a correctness fix

Each residual site becomes, in the profile and in the engine identically:

```
AddElem(h, projected)  →  Rescale(·, residual_scale[site])  →  Requantize(·, residual_requant[site])
```

The `Requantize` is **not optional and not new** — the engine has always performed it. Declaring it
is what makes the profile describe the computation the engine runs, and what stops the court from
reading an `i32` sum as an `int8` lane.

This is a defect fix, not a feature. A graph whose residual sites do not adjudicate is a graph whose
class cannot be weight-bearing, and BASE-0 is the liveness floor.

### Decision B — the residual may amplify, and its gain is a registration artifact

`Rescale` sits between the add and the narrowing so a decayed stream can be lifted before it is
re-quantized. The gain is per layer and per site, frozen at registration, inside the artifact
digest, and it is **calibration** in Decision H's sense: chosen by measuring what a class's own
layers produce, not computed from the data at inference time.

No op is added. ADR-0040's Decision D table is unchanged, and this ADR asks only for an additive
note under Decision H's consequences naming the residual as a third amplification site.

### Decision C — the gain is per tensor, per layer, per site — not per channel

The court's `Rescale` arm reads exactly one parameter triple for the whole node
(`palw_step_refute.rs:418-439`: `weight_row(name, layer, 0, 1)`, then `row.len() != 5` refuses
anything else). A per-channel residual gain is not expressible without changing that arm, and
changing it is an op-semantics change rather than a graph change.

Per-tensor is also the right granularity on the evidence: the measured collapse is a whole layer's
stream falling to 5 of 127, not a few channels diverging.

### Decision D — the new parameters join the artifact inventory as named tensors

`BASE0_TENSOR_NAMES` (`palw_base0_profile.rs:84-99`) gains, per layer and per site:

```
blk.{layer}.attn_residual.scale     ·  5 bytes  (i32 multiplier LE + u8 shift)
blk.{layer}.attn_residual.requant   ·  9 bytes  (i32 multiplier LE + u8 shift + i32 zero LE)
blk.{layer}.ffn_residual.scale      ·  5 bytes
blk.{layer}.ffn_residual.requant    ·  9 bytes
```

They must be **tensors** rather than struct fields because the court resolves a `Rescale` node's
parameters through `PalwWeightOracleV1` — the gain has to be openable against `artifact_root` or the
step is `Unadjudicable`. That is a constraint the design already imposes, and satisfying it is what
makes the gain a committed fact rather than a number a producer asserts.

## ADR-0052: the QWEN36 hybrid operations

*Source:* [0052-palw-qwen36-hybrid-class.md](../../adr/0052-palw-qwen36-hybrid-class.md) (full text in [archive](../../design/palw/archive/0052-palw-qwen36-hybrid-class.md)).

### Decision A — Everything ADR-0040 says still holds

Integer-only, no libm, no float on the execution path. Activations are A16 codes (ADR-0047):
`i16` values in `i32` lanes. Scales are `(multiplier, shift, zero)` triples frozen at registration.
Every lossy site is named and each names its own rule. Reduction order is free, conditional on the
no-overflow bound, which every op below proves at its own entry.

### Decision B — The router is a SELECTION, and its tie rule is normative

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

### Decision C — The combine has one accumulator

`Σ_e w_e · y_e` in `i64` across all `k` experts, narrowed once. Requantizing per expert and adding
afterwards rounds `k` times and makes the result depend on how the caller grouped the experts.

### Decision D — `IntLn`, the fourth transcendental

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

### Decision E — The gated delta rule, and why an integer state is stable

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

### Decision F — Partial rotation is not an optimisation

`partial_rotary_factor: 0.25` with `head_dim: 256` means 64 rotated lanes and 192 carried through
untouched. The unrotated lanes are position-independent by design; rotating them makes every one a
different number. A full-rotation implementation is a different model, not a slower one.

