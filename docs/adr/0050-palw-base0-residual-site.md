# ADR-0050: The BASE-0 residual site — the narrowing that was never declared, and the amplification that was

Status: **Proposed.** Changes no op, amends no catalog, and activates nothing. It fixes a declared
graph that cannot be adjudicated at its residual sites, and settles whether the residual add may
amplify.

Date: 2026-08-21

Relates to: ADR-0040 (`PALW-BASE-0`; Decision D's op set is **unchanged** by this, and Decision H is
the precedent that decides the amplification question), ADR-0049 (the adjudication contract — this
ADR is an instance of its Decision F, found by looking for one), `palw_base0_profile.rs`,
`misaka-palw-base0/src/engine.rs`, `consensus/core/src/palw_step_refute.rs`.

**The premises are untouched.** Block production stays a hash-independent PALW lottery; the
free-prompt lane stays. This is arithmetic inside one class's graph.

---

## Context — the question asked, and the larger one found underneath it

The question put to this ADR was: BASE-0's residual stream is `int8` because ADR-0040 Decision D
fixes `AddElem` at `i8 → i32`, a `Requantize` can only ever attenuate (its gain is
`multiplier / 2^shift` with the multiplier a Q31 fraction), and per-layer calibration against a real
checkpoint bottomed out at `shift: 0` on every layer but one while the residual peak still fell to
5 of 127. A decayed stream needs amplification. Does BASE-0 get an amplifying residual, and is that
an op-set amendment?

Investigating it found that **the residual site in the declared graph cannot be adjudicated at all
today**, which is a larger and more urgent problem than the one asked about.

### The residual path has no narrowing node, and the court reads it as `int8`

`palw_base0_profile.rs:246` declares the attention residual as a bare add:

```rust
plain(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, hidden, vec![9, PALW_STEP_INPUT_LAYER_IN]),
```

Node 11, the FFN norm, consumes it directly (`:249-257`, `input_refs: vec![10]`). And the court's
`RmsNorm` arm reads its input through `as_i8` (`palw_step_refute.rs:354-359`), where

```rust
let as_i8 = |row| i8::try_from(*v as i32).map_err(|_| InputSetNotCanonical("base0 int8 lane out of range"));
```

`AddElem` returns the **`i32` sum of two `int8` codes**, range `[-256, 254]`. So the moment the
residual carries anything past `±127` — which is exactly when it is carrying signal — the court
returns `InputSetNotCanonical` and the step does not adjudicate.

The FFN residual is worse: node 17 is `AddElem(16, 10)` where node 16 is a raw `MatMulQuant`
accumulator and node 10 is the raw sum above, so **both** operands are out of the lane the arm reads.

The engine does not have this problem, because the engine narrows:
`engine.rs:267` is `requantize_row_uniform(&add_elem(&h, &projected)?, residual_requant)`. The
engine performs a narrowing the profile does not declare and the court does not know about — which
is ADR-0049 Decision F's defect, and this is where it bites first.

### The amplification question, answered

The catalog already contains every op needed. `KDESC_BASE0_ADD_ELEM`, `KDESC_BASE0_RESCALE` and
`KDESC_BASE0_REQUANTIZE` are rows of the single-source `KERNEL_CATALOG`
(`palw_step_refute.rs:176-193`), each with a live adjudicator arm, and `kernel_can_serve_node_v1`
already admits a `Rescale` node in the same arm as `Requantize` and `Rope` (`:262-268`). The
step-leg machinery carries this shape elsewhere in the same graph: `input_refs` below
`PALW_STEP_INPUT_SENTINEL_MIN` are intra-table node indices, and BASE-0's lane is
`PalwStepLaneV1::Int32`, so a node consuming another node's `i32` output is ordinary.

The op set is frozen at ten in one place (`KDESC_BASE0_ALL`, held by an assertion at
`palw_step_refute.rs:2330`) and the graph is a per-class function of geometry in another
(`base0_profile_v1`). **A residual `Rescale` changes the second and not the first.**

Decision D's own text settles it: op 8 is *"`AddElem` — exact `i32` add (**scales pre-aligned at
registration**)"*. A per-layer gain that is a registration artifact **is** scale pre-alignment. And
Decision H already ruled on this exact class of change when it added `attn_logit_scale` and
`ffn_gate_scale`, calling the gain targets *"calibration, not consensus"*. A residual scale is the
third member of that set.

---

## Decision A — the residual site gains its narrowing node, and this is a correctness fix

Each residual site becomes, in the profile and in the engine identically:

```
AddElem(h, projected)  →  Rescale(·, residual_scale[site])  →  Requantize(·, residual_requant[site])
```

The `Requantize` is **not optional and not new** — the engine has always performed it. Declaring it
is what makes the profile describe the computation the engine runs, and what stops the court from
reading an `i32` sum as an `int8` lane.

This is a defect fix, not a feature. A graph whose residual sites do not adjudicate is a graph whose
class cannot be weight-bearing, and BASE-0 is the liveness floor.

## Decision B — the residual may amplify, and its gain is a registration artifact

`Rescale` sits between the add and the narrowing so a decayed stream can be lifted before it is
re-quantized. The gain is per layer and per site, frozen at registration, inside the artifact
digest, and it is **calibration** in Decision H's sense: chosen by measuring what a class's own
layers produce, not computed from the data at inference time.

No op is added. ADR-0040's Decision D table is unchanged, and this ADR asks only for an additive
note under Decision H's consequences naming the residual as a third amplification site.

## Decision C — the gain is per tensor, per layer, per site — not per channel

The court's `Rescale` arm reads exactly one parameter triple for the whole node
(`palw_step_refute.rs:418-439`: `weight_row(name, layer, 0, 1)`, then `row.len() != 5` refuses
anything else). A per-channel residual gain is not expressible without changing that arm, and
changing it is an op-semantics change rather than a graph change.

Per-tensor is also the right granularity on the evidence: the measured collapse is a whole layer's
stream falling to 5 of 127, not a few channels diverging.

## Decision D — the new parameters join the artifact inventory as named tensors

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

## Decision E — this is blocked on ADR-0049 Decision A, and the block is not incidental

The `Rescale` arm asks the oracle for **one element** and then requires **five bytes**:

```rust
let row = weights.weight_row(node.weight_name.as_str(), layer, 0, 1)…;   // :420
if row.len() != 5 { return Err(InputSetNotCanonical("base0 rescale params are not 5 bytes")); }  // :421
```

`PalwProvenOperandsV1` returns `elements` **bytes** (`palw_artifact.rs:187`). So through the
production oracle this arm receives one byte and refuses every `Rescale` step. The existing test
`rescale_is_adjudicable_and_bounded` (`:1279-1315`) passes only because `FixedRow` ignores the size
it was asked for.

**Op 9 has never adjudicated through a real opening.** Landing Decisions A–D against the current
oracle would produce a graph whose residual sites are `Unadjudicable` for a second, unrelated
reason. ADR-0049 Decision A — byte addressing — is a prerequisite, and this is the concrete case
that proves it is not a tidiness concern.

---

## Consequences

* **`shape_profile_id` moves**, so `execution_class_id` moves, so the RC genesis artifact re-mints.
  Nothing is registered yet, which is why this is the moment. The court **ladder** is unaffected —
  gate 5 provisions at `PALW_STEP_MAX_LEAVES` regardless of the class.
* **Two nodes per layer are added to the step space**, so `step_leaf_count` rises and every leaf
  count, canonical `pwu_per_inference` and `tile_len`/`n_ctx` admissibility figure must be
  re-measured before a class using this graph is registered.
* **The residual calibration must be re-run**, because the parameter it solves for changed shape:
  it now chooses a gain and a narrowing per site rather than a narrowing alone. The prior finding —
  that per-layer narrowing alone bottoms out at `shift: 0` — is what motivates the gain, and does
  not predict its value.
* **Amplifying a decayed stream amplifies its quantization error by the same factor.** A stream that
  has fallen to `±1` carries about one bit, and a gain restores its range without restoring its
  information. The gain is therefore a remedy for a stream that is *small*, not for one that is
  *dead*, and calibration must be checked against `ForwardProbe` rather than against the peak alone.

## What this ADR does not decide

* **The value of any gain.** That is a measurement per class, and for the first real artifact it is
  part of the PTQ pipeline's output.
* **Whether `AddElem` should widen.** Widening the residual to `i16`/`i32` is a genuine op-set
  amendment with a much larger blast radius, and it is not needed if Decisions A–D hold. If
  calibration after this ADR still cannot keep a deep class's stream healthy, that is the point at
  which widening earns its own ADR.
