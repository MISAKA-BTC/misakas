# Beyond weight files — PALW as verifiable useful AI computation (direction note)

Status: **direction, not an implementation gate** (user, 2026-10-08). Nothing here joins the "every RFC implemented, then one activation"
scope; it records how future work must be shaped so that the current designs do not hard-wire "AI model = one fixed weight file".

## Principle

MISAKA is a chain that **registers, executes and settles independently verifiable useful AI computation** — not a registry of LLM weight
files. Model families will change (Transformer → SSM/hybrid → models with memory updated during inference → retrieval- and tool-centred
systems of small models); the invariant that does not change is **G14**: an ordinary bonded verifier outside any Panel, from public
authenticated material only, reaches an objective conviction or a correctly classified DA/default. A computation that cannot meet that
is not rewardable, whatever its architecture.

User's outlook (2026-10, subjective, for planning only): weights + external memory/retrieval/tools — very likely widespread; models that
update memory or some parameters during inference — likely; many small models + external data replacing one giant checkpoint for many
uses — likely; weight-free methods replacing general LLMs — unlikely in 5–10 years; today's Transformer + weight file as the sole
standard — unlikely.

## The generalisation: an authenticated computation specification plus its state set

A class binds a versioned specification (kernel/plan ids, RFC-0005 / ADR-0172) and **typed roots** for whatever the computation reads:

| Kind | Bound | What G14 needs |
| --- | --- | --- |
| Weight model (today) | artifact (weight) root | as today: commitments, localisation, exact court |
| Inference-time memory | memory root + an update rule expressible in the active kernel | pre/post-state roots public per step; a fault localised to one update step and judged exactly |
| Retrieval | a reference-data snapshot root + a deterministic retrieval rule | the snapshot's public availability (DA); a retrieval result judged per item |
| Composite (models + tools) | a pipeline of such specifications; tool stages only as verified kernels | per-stage localisation (RFC-0003 edge courts, extended) |

Already in place: TIR `StateWrite` / `HistAppend`, fixed recurrent state and held history judged by the court (Qwen3.5 GDN, Mamba);
RFC-0003 pipeline classes with edge courts; RFC-0004 parent/candidate classes; versioned typed ids for kernels, plans and policies.

## What stays forbidden

Arbitrary external APIs, unverified Python, or any step an outsider cannot re-derive from public material. A new semantic that the
active kernel cannot express is a versioned kernel extension (a coordinated, fenced protocol change), never a per-model escape hatch.

## For current work

New object and state layouts should carry typed, versioned root kinds rather than assuming exactly one weight root, where that costs
nothing today; census reclassification must not drop a repository merely because it is not a weight checkpoint once such a kind exists.
