# ADR-0147 — Independence is drawn, not declared

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0147-independence-is-drawn-not-declared.md](../design/palw/archive/0147-independence-is-drawn-not-declared.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: **Implemented behind `palw_admission_independence`**, one of the three fences of the
  ADR-0145 bundle, which are armed together or not at all. Dormant on testnet-11's preset. **Armed on
  testnet-12 from genesis.** Addendum (2026-09-20): a registration does not straddle the fence. On
  testnet-12 the outsider seat is stake-weighted (ADR-0152 SW-5), while the jury stays this ADR's.
* Date: 2026-09-20

## Context

The first repair checked a declared identity: the registrant said who it was not. Independence cannot
be declared.

## Decision

- **§2.1 — The outsider seat:** every claim of a bought class seats one seat from outside the class's
  own population, and its `Valid` is required to license.
- **§2.2 — The population is fixed before its randomness.**
- **§2.3 — The admission jury** is drawn per audit.
- **§2.4 — One claim, one answer.**
- **§2.5 — What the stratified draw does not have.**

→ spec 03 PALW-CL-13, 08 PALW-VF-5.

## Consequences

- The bound, stated at its real strength, is in the full text: a registrant's Sybils can capture a
  jury, but not the stake-weighted outsider.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0147-independence-is-drawn-not-declared.md](../design/palw/archive/0147-independence-is-drawn-not-declared.md)
