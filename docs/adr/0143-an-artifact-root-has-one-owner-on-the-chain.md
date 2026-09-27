# ADR-0143 — An artifact root has one owner on the chain, and competing weights stay permissionless

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0143-an-artifact-root-has-one-owner-on-the-chain.md](../design/palw/archive/0143-an-artifact-root-has-one-owner-on-the-chain.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: **Implemented 2026-09-18.** It was withdrawn from the 7,100 flag day and reworked after the
  adversarial audit of 2026-09-19 found two defects in its index. It runs behind its own fence,
  `palw_artifact_root_ownership`, which is armed on testnet-12 from genesis.
* Date: 2026-09-18

## Context

"Which line owns this artifact root" was answered by the first match in `BTreeMap` id order, so a
hash's byte order was a consensus-visible outcome. It decided who received usage attribution, the
buyback and the owner fee.

## Decision

- **D1 — An artifact root has one owner, and the chain stores it.**
- **D2 — The positional lookups retire.**
- **D3 — One source for every attribution.**
- **D4 — A founding root is reserved at registration, atomically.**
- **D5 — Every entrance refuses a duplicate.**
- **D6 — Activation canonicalizes what is already there,** deterministically.
- **D7 — Legacy duplicate rows stay.**
- **D8 — Nothing settled before the fence is recomputed.**
- **D9 — It has its own fence.**

→ spec 03 PALW-CL-17.

## Consequences

- Competing weights stay permissionless under their own roots.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0143-an-artifact-root-has-one-owner-on-the-chain.md](../design/palw/archive/0143-an-artifact-root-has-one-owner-on-the-chain.md)
