# ADR-0036: PALW mainnet activation — lineage reconciliation and the model that governs

> **Body moved (2026-09-27).** Normative rules → [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0036-palw-mainnet-activation-model.md](../design/palw/archive/0036-palw-mainnet-activation-model.md); the reasoning is summarised in [design/palw/lineage.md](../design/palw/lineage.md).

* Status: Proposed (governance decision), 2026-08-17. **Decision 4's hash floor is superseded** by
  ADR-0039 D1/D2 (W6′): mainnet ships no hash floor. Block production is PALW work on every network,
  and the liveness floor is the portable integer class `PALW-BASE-0`. Decision 2's adoption of ADR-0041's
  conclusions stands. Its mechanism (`palw_spam`, `palw_algo4_accept`, `palw_compute_work_scale`) was
  not adopted.
* Date: 2026-08-17

## Context

Two lineages shared the name "ADR-0041": the live `palw_credit` lineage, and a non-ancestral
`main-backup-8107bfb` snapshot whose ADRs 0039–0048 decided a different mainnet mechanism. Mainnet
activation needed one governing model, and ADR numbering needed to be settled.

## Decision

- **D1 — The live `palw_credit` lineage governs.**
- **D2 — The snapshot's ADR-0041 is superseded as to mechanism.** Two of its conclusions are adopted:
  a new network identity, and land → accept → mint.
- **D3 — Mainnet activation is gated** behind the full credit ladder, the §12 gate and the audit's
  blockers.
- **D4 — The hash floor.** *Superseded by ADR-0039.* Its testnet half, no floor on testnet-11 or devnet
  (a loud halt beats a silent fork), survives, refined by ADR-0060.
- **D5 — Namespace.** The live lineage owns the numbers. The snapshot's 0039–0048 reserve nothing.

## Consequences

- ADR-0028's mainnet clause became self-consistent, and the §12 gate ledger gained a "Wired?" column.
- Landed with it (2026-08-17): libm became part of the class identity (runtime manifest v3,
  `libm_arithmetic_digest`), with no fingerprint change.
- Mainnet parameters became a matter of filling in measured values within this frame. Today's mainnet
  rule is spec 16 PALW-NP-7.

## Links

- Spec: [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/lineage.md](../design/palw/lineage.md)
- Full text as written: [design/palw/archive/0036-palw-mainnet-activation-model.md](../design/palw/archive/0036-palw-mainnet-activation-model.md)
