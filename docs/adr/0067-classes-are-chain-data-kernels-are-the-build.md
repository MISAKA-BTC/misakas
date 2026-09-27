# ADR-0067 — Classes are chain data; only kernels are the build

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0067-classes-are-chain-data-kernels-are-the-build.md](../design/palw/archive/0067-classes-are-chain-data-kernels-are-the-build.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: **D1–D3 and D5 landed for the dense (A16) container (2026-08-31)**, fenced (the chain-class
  fence) and dormant at first; D4 needed no code; D6, with its cache bound, landed 2026-09-01. Armed on
  testnet-12 from genesis, where the permissionless registry (ADR-0135) builds on it.
* Date: 2026-08-31

## Context

The goal, stated as a test: a model nobody has heard of enters a live network through the same
generic path as one that shipped with it. The one thing that was not yet true was that the class
catalogue was compiled into the binary.

## Decision

- **D1 — The chain state is the class catalogue.** The compiled table is genesis bootstrap plus cache.
  → spec 03 PALW-CL-1.
- **D2 — A class executes from its registered profile.** → spec 04.
- **D3 — The kernel set is the consensus surface, and it is irreducible.** → spec 04 §4.3.
- **D4 — Distribution and service stay off-chain.** The chain pins only identity.
- **D5 — The interpreter ships behind a fence,** armed only after a fuzz gate.
- **D6 — Four storage tiers.** Node storage is never consensus state.

## Consequences

- ADR-0053's "the catalogue commits the classes" is re-scoped: the registered profile is the
  authority.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0067-classes-are-chain-data-kernels-are-the-build.md](../design/palw/archive/0067-classes-are-chain-data-kernels-are-the-build.md)
