# ADR-0145 — Canonical work is derived, not declared; admission is earned, not registered

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md), [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0145-canonical-work-is-derived-and-admission-is-earned.md](../design/palw/archive/0145-canonical-work-is-derived-and-admission-is-earned.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: Design, 2026-09-19. **§1–§5 are implemented and fenced** by ADR-0147, 0148 and 0149 (the
  economic bundle: `palw_canonical_work`, `palw_fp_derived_work`, `palw_admission_independence`, armed
  together or not at all). **§6, the prefix-state object family (`PalwFpPrefixStateV1`), was added
  2026-09-21.** All of it is armed on testnet-12 from genesis.
* Date: 2026-09-19

## Context

The 2026-09-19 reward audit found that the unit price of a claim was whatever a registrant declared:
`claim.pwu = expected_attempts × pwu_per_inference`, with the leaf count set by a registrant-chosen
`tile_len`. This is the accounting and admission half of ADR-0144.

## Decision

- **§2 — Four invariants, ahead of any mechanism:** P4 and P5 as testable properties.
- **§3 — Canonical class identity** is derived. → spec 03 PALW-CL-2.
- **§4 — `CanonicalWorkVector`:** work is a vector derived from the graph and the execution facts.
  → spec 05 PALW-WK-1.
- **§5 — One derivation for both lanes.** → spec 05 PALW-WK-2.
- **§6 — Cache is an execution fact, never a claim** (prefix state). → spec 11 §11.4.
- **§7 — Admission: registered is not eligible.** → spec 03 §3.3.

## Consequences

- What must be proved adversarially (§8), and "done means" (§10), are in the full text.
- The coefficients were left to ADR-0146.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md) · [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0145-canonical-work-is-derived-and-admission-is-earned.md](../design/palw/archive/0145-canonical-work-is-derived-and-admission-is-earned.md)
