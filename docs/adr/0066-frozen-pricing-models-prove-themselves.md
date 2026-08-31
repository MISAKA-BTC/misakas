# ADR-0066: The pricing is frozen; models prove themselves against it

**Status**: Accepted 2026-08-31 (operator direction). Activates at the next coordinated
identity move; the live testnet-11 stays on `f3bf86b4…` until then.

## Context

The free-prompt lane prices certified work in CU (`prompt·1 + decode·64`), divides it into
quanta (`⌊CU / QUANTUM_CU⌋`, one lottery draw each, jackpot-capped), and weighs it
(`quanta × PWU_PER_QUANTUM`). All three knobs sit inside `palw_ruleset_id`, so changing any of
them is a re-mint.

The first calibration chose `QUANTUM_CU = 1_000` against chat-shaped jobs (~16.5k CU). It could
not see a single court-admissible class: the 80 KiB close carrier caps a hybrid class's context
near 8 positions, whose largest job is 513 CU — zero quanta, permanently. The observed failure
mode was worse than the number: the correction ("this class is under 1,000 CU, shrink the
quantum") routes **every future model's size through a consensus parameter**, which is the
pattern this codebase already outlawed for genesis ("consensus changes by activation, not
regenesis") wearing a new coat. Adding a model must be data. A pricing change is a fork. The
two had merged.

## Decisions

**D1 — the constants are frozen, and this recalibration is the last.**
`QUANTUM_CU = 100`, `PWU_PER_QUANTUM = 10` (the rate, 0.1 pwu/CU, is unchanged — only the
granularity moved), CU weights `(1, 64)`, `MAX_QUANTA_PER_RECEIPT = 640` (the count scales 10×
so the bound's real economics — a 64,000-CU / 6,400-pwu per-receipt ceiling — stay
byte-identical; draws get smaller and more numerous under the same cap, which is the direction
the variance smoothing wants). Every admissible class of ≥2 context positions prices to ≥1
quantum at these values. Changing any of them is by construction a new ruleset id and therefore
an explicit fork decision — never a model-onboarding step.

**D2 — admission proves pricing reachability** (`PalwClassAdmissionError::PricingUnreachable`).
`verify_class_admission_v2` now requires that the largest job a class's declared `n_ctx`
admits (`max_admissible_cu_for_context`, both extreme prompt/decode assignments priced, the
larger taken) certifies at least one quantum. A model the frozen quantum cannot see is refused
at registration, before any fee — not accommodated by moving the quantum. This is the
inversion the operator named: *"protocol の quantum をモデルに合わせる"のではなく、
"モデルが protocol の quantum でちゃんと測定可能か確認する"*. The check runs after coverage
and the court ceilings, so an unadjudicable class still hears about the deeper problem first.

**D3 — weight stays quantum-uniform; the linear alternative was written and reverted.**
A CU-linear pwu (weight = `⌊CU × rate⌋`, quantum as pure accounting — the cleanest reading of
the direction) collides with a load-bearing invariant: the spend frontier advances by
`pwu / quanta` per spent quantum and the carriage check demands that division be exact
("free-prompt pwu … uniform non-zero quanta"). Replacing uniform slices with a remainder
schedule is real arithmetic risk purchased to remove an error the frozen quantum already
bounds at **one quantum's weight per receipt** (10 pwu ≈ one decode-token) — and D1 shrank
that bound 10×. Weight therefore tracks CU to within one quantum, the rate is frozen, and no
model's size ever argues about granularity again. Draws stay integer for the reason
`fp_quanta_v3` records: a fractional draw needs a scaled target, and a scaled target re-opens
the variable-lottery arithmetic the quantization exists to delete.

**D4 — the boundary, stated once**: model addition = a registration (data: profile, artifact
root, derived pwu, canonical job) admitted by frozen gates. Pricing / quantum / CU-weight
changes = a fork (identity move, every node, at once). A future model below the pricing floor
is refused; if the protocol ever *wants* such models, that is a D1 amendment — a fork decision
with this ADR's number on it, not a calibration.

## Consequences

* testnet-11's pin moves `f3bf86b4…` → `e89c6c39…` at the next identity move (this branch
  DOES NOT roll out on its own — an un-coordinated build refuses the live net at handshake).
* Every shipped class rung (n_ctx 8/9/12/16) prices to ≥1 quantum at the frozen values —
  pinned by `the_pricing_is_reachable_on_registered_classes_and_frozen_against_the_rest`,
  which also pins the refusal (n_ctx 1 → 65 CU → `PricingUnreachable`).
* The SDK conformance battery and the node's registration preflight inherit the gate with no
  new wiring (they already call `verify_class_admission_v2`), so a sub-quantum model is told
  so before a carrier, a signature, or a fee exists.
* Qwen 1.5B → 7B → 14B → 32B → 70B onboarding stays what ADR-0056 made it: a registration.
