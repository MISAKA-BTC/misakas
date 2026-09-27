# ADR-0119 — A held class is walked at the regime's ladder, and the chain records which classes those are

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md](../design/palw/archive/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed and, for its consensus half, **implemented 2026-09-12**, riding testnet-11's held
  flag day. testnet-12 has it from genesis.
* Date: 2026-09-12

## Context

ADR-0118 §4 measured that testnet-11's frozen ladder admitted a held dense class only to about a
fraction of its context.

## Decision

- **D1 — The held ladder is `2^40`** (`PALW_HELD_STEP_LADDER_V1`).
- **D2 — The chain records which classes are held** (`class_step_ladders`).
- **D3 — Admission prices and counts a held class at its ladder.**
- **D4 — The courts read the claim's ladder.**
- **D5 — The extraction walk bounds a commitment by its class.**
- **D6 — The transaction door is ADR-0087 D6's pair.**
- **D7 — The private free-prompt path runs.**

→ spec 03 PALW-CL-22, 09 PALW-CT-23.

## Consequences

- Every other class keeps the network's ladder.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md](../design/palw/archive/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
