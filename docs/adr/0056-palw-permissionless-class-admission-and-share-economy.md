# ADR-0056: Permissionless class admission, and the share economy that survives it

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0056-palw-permissionless-class-admission-and-share-economy.md](../design/palw/archive/0056-palw-permissionless-class-admission-and-share-economy.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Accepted and **implemented (2026-08-27)**. **D4 (the streak share walk) was withdrawn** in
  favour of ADR-0054, whose share mechanics ADR-0137 later superseded. **D6 was amended by ADR-0088
  D3:** an attempt whose root differs from every root in force is refused.
* Date: 2026-08-27

## Context

Permissionless class admission had been the design since ADR-0049 H. The share economy had to survive
it, so that anyone can list while nobody can buy weight.

## Decision

- **D1 — The constitution: admission is arithmetic, and only arithmetic.** → spec 03 PALW-CL-4.
- **D2 — The kernel boundary:** what needs a binary, and what does not.
- **D3 — Registration exposure:** entry is priced in bonded collateral. → spec 03 PALW-CL-7.
- **D4 — The share walk.** *Withdrawn.*
- **D5 — Reclamation:** dead classes give the network back their capacity. → spec 03 PALW-CL-8.
- **D6 — Duplicates are priced, not policed.**
- **D7 — What the chain does not judge.**

## Consequences

- State version 8 → 9, as predicted.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0056-palw-permissionless-class-admission-and-share-economy.md](../design/palw/archive/0056-palw-permissionless-class-admission-and-share-economy.md)
