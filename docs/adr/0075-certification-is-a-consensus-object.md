# ADR-0075: Certification is a consensus object

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0075-certification-is-a-consensus-object.md](../design/palw/archive/0075-certification-is-a-consensus-object.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Proposed 2026-09-02, implemented for the Relaunch 5e regenesis (state version 16), and
  governing. Armed on testnet-12 from genesis. **ADR-0144 alignment (2026-09-21):** the gateway
  remainder is the local `/v1` reading genesis ∪ chain.
* Date: 2026-09-02

## Context

A certificate lived in the binary: a certified set compiled into the build. Adding a model therefore
needed a release, which contradicted "a model is data".

## Decision

- **D1 — Two lifecycle objects**, `FamilyCertified` and `ClassLaneCertified`, carried by ordinary
  transactions.
- **D2 — The court grades; nothing else vouches.** The transition re-runs the shipped grader.
- **D3 — The chain's certified sets are state.**
- **D4 — Every gate reads genesis ∪ chain.**
- **D5 — A class is bound by kernel coverage,** in both lanes.
- **D6 — The genesis free-prompt set** is derived by the same rule.
- **D7 — The route has tooling** (`palw-certify drill`).
- **D8 — The mainnet route:** floor-only genesis, with every model arriving by registration, drill and
  binding.
- **D9–D14 — Rules of operation:** anyone may submit; there is no revocation object; upgrades by a
  second certificate; no expiry by time; state version 16; oversized objects ride in chunks.

→ spec 03 §3.4.

## Consequences

- The security amendment of 2026-09-02 (who may submit, and at what cost) is in the full text.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0075-certification-is-a-consensus-object.md](../design/palw/archive/0075-certification-is-a-consensus-object.md)
