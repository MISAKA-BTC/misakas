# ADR-0070: The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0070-the-model-tiers-step-spaces-are-adjudicable.md](../design/palw/archive/0070-the-model-tiers-step-spaces-are-adjudicable.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Accepted (2026-09-01; numbered 0070 on merge). Implemented on `palw-step-space-e2e`. Arming is
  a deployment decision, and testnet-12 arms the model tiers from genesis.
* Date: 2026-09-01

## Context

For a model tier (a real LLM class rather than the floor), the chain had to be able to adjudicate the
tier's step space end to end, including the hybrid's routed experts.

## Decision

- **D1 — The acceptance test is the property:** a sweep over the step space proves adjudicability.
- **D2 — The commitments a model-tier claim carries** are fixed. → spec 04 §4.4.
- **D3 — The consensus changes are one version,** and they move together.
- **D4 — The routed experts** are the hybrid's one genuinely new court surface.
- **D5 — Registration obligations:** what a court-capable registration must supply. → spec 03 PALW-CL-6.

## Consequences

- A model tier is weight-bearing only when certified (ADR-0069 D5, spec 03 PALW-CL-15).

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0070-the-model-tiers-step-spaces-are-adjudicable.md](../design/palw/archive/0070-the-model-tiers-step-spaces-are-adjudicable.md)
