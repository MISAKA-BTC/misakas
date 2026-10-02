# How deep can PALW-BASE-0 go? — measured 2026-08-21

Harness: `misaka-palw-base0/src/bin/base0-depth-sweep.rs`, run `--release`. Seed
`0xba5e0deed0d00001`, 8 tokens, vocab 4096, two widths (`d_model` 256 and 1024), depths 4/8/16/24/32.

**Why this was run.** ADR-0040 Decision D fixes `AddElem` at `i8 → i32`, so the residual stream
between layers is `int8` and widening it is a new kernel — the one change that re-opens the
catalog. The artifact carries **one** `residual_requant` for the whole model, applied at both
residual adds in every layer, and the scale algebra leaves two settings with no third. Whether a
Qwen-scale BASE-0 class (24–28 layers) is arithmetically reachable at all depends on what those two
settings cost, and that was a guess until now.

**What these numbers are not.** The weights are `derive_deterministic`'s seeded LCG, because no
trained BASE-0 artifact exists. This measures **the arithmetic's headroom, not a model's quality**.

---

## 1. The prediction was wrong, and the reason is `RoundingShiftRight`

The scoping note argued that under gain 1/2 a layer-0 feature is attenuated by `2^(2n)` and is
"annihilated by layer 24". It is not. ADR-0040 C1 rounds half **away from zero on the magnitude**,
so `RSR(1, 1) = 1` — and a decaying residual value reaches ±1 and **stays there forever**:

| gain | decay of a maximal code, add by add | adds to 1 unit | floor |
|---|---|---|---|
| 2^-0 | 127 | — | ±127 forever |
| **2^-1** | **127 → 64 → 32 → 16 → 8 → 4 → 2 → 1** | **7** | **±1 forever** |
| 2^-2 | 127 → 32 → 8 → 2 → 1 → 0 | 4 | 0 |

The rule was chosen for gemmlowp agreement (C1's 50 %-disagreement finding). A side effect is that
the residual stream never completely forgets — **it forgets everything except a sign.**

So the honest statement is not "early layers die". It is: **the residual highway carries about
7 adds ≈ 3.5 layers of magnitude, after which a feature survives as one unit.**

## 2. Halving is healthy at every depth measured — and that is a weaker result than it looks

At `d_model` 256 and 1024, depths 4 → 32, gain 1/2:

* `residual_peak` stays in **75..113**; **0 / 32 layers** ever reach the `Saturate8` rail; none collapse to 0.
* Attention never degenerates: the minimum spread across all `(layer, head)` pairs stays at
  **74–229 %** of the uniform distribution.
* The SiLU gate keeps its asymmetry at a stable **28 %** (`|min| / max`) — a degenerate linear
  `x/2` would sit near 100 %.
* Ablating any single layer moves the logits, and the influence is **flat in depth**:
  `last / first` is **0.86–0.97** at 32 layers.

**The flat influence curve does not mean the residual highway works.** Ablation removes a layer's
write to `h`, which also changes what every later layer computes, and `RmsNorm` — being
scale-invariant — restores that perturbation to full range at each step. So the measurement
confirms that information propagates layer-to-layer through 32 layers without saturating or
collapsing, and says nothing about whether a late layer can still read an early feature. §1 is what
answers that, and its answer is 7 adds.

## 3. Unity gain is the alternative, and its price is measured in bits

Unity gain gives unbounded memory. What it costs is that every layer's write must be small enough
that the accumulated sum never clips. The minimum extra attenuation on `requant[3]` (attention out)
and `requant[6]` (FFN down) that keeps the stream off the rail:

| depth | `d_model` 256 | `d_model` 1024 |
|---|---|---|
| 4 | +3 → **4 bits** per write | +3 → **4 bits** |
| 8 | +3 → **4 bits** | +4 → **3 bits** |
| 16 | +4 → **3 bits** | +5 → **2 bits** |
| 24 | +4 → **3 bits** | +5 → **2 bits** |
| 32 | +5 → **2 bits** | +5 → **2 bits** |

Two things fall out of this table:

* **At Qwen depth each layer writes with 2–3 bits** of the `int8` code range, plus a sign.
* **Width makes it worse, not better.** `d_model` 1024 needs one more bit of attenuation than 256
  at the same depth, because a wider fan-in produces a larger accumulated write. So "buy depth by
  being wider" is not available.

## 4. What this decides

**24–32 layers is arithmetically reachable.** Nothing saturates, nothing collapses, attention and
the gate stay non-degenerate at 32 layers and at both widths. The depth question is not a wall.

**But the residual is the binding constraint on quality**, and it is now quantified rather than
feared. A BASE-0 class picks one of:

| | residual highway | per-layer write |
|---|---|---|
| gain 1/2 | ~7 adds (≈3.5 layers) of magnitude, then a sign | full 7 bits |
| gain 1 + attenuation | unbounded | 2–3 bits at depth 24–32 |

Neither is fatal for a liveness floor. Both are serious for a model meant to produce good text.

**Per-layer `residual_requant` is a smaller win than the scoping note claimed.** It lets a
calibration *place* the budget across layers; it does not enlarge it. Downgrade from "required" to
"useful".

**The escape is `AddElem` at a wider type** — and it is now possible to say what it buys rather
than that it would be nice: `i16` moves the per-write budget from 2–3 bits to 10–11. That is a new
kernel and an ADR-0040 amendment, which is the decision this measurement exists to inform.

## 5. An actionable finding about the class that actually ships

`PALW_RC_BASE0_GEOMETRY` is 4 layers, and 4 layers is **8 residual adds** against a 7-add memory.
A feature written by layer 0's attention passes through 7 later adds and arrives at the output as
**±1** — a sign. Even the floor class sits exactly at the edge of the halving regime.

At 4 layers the unity table offers +3 attenuation for **4 bits** per write and unbounded memory.
For the RC floor that looks like the better side of the trade, and it costs a `residual_requant`
value, not a code change.

Recommend measuring it against the RC geometry with a real (or at least better-calibrated)
artifact before the class is frozen — the class id covers `residual_requant`, so this is not a knob
anyone can turn afterwards.
