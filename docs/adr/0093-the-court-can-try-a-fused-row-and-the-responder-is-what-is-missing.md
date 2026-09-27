# ADR-0093 — The court can try a fused row; the responder is what is missing

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md](../design/palw/archive/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Proposed 2026-09-06 (design). **§6 steps 2–4 implemented on 2026-09-11:** both A16 families
  answer a fused dissection end to end, drilled. D6–D8 were added on 2026-09-11 behind their own fences
  (`palw_fused_dissectable`, and the anchored root claim), which are armed on testnet-12 from genesis.
* Date: 2026-09-06

## Context

Every move of the fused-attention dissection existed in consensus, but no shipped binary could produce
the first move. On a fused class an honest producer was convicted by silence, and a dishonest one
could not be tried.

## Decision

- **D1 — The responder's backend obligation is a single verb over one history tile.** As built: a
  family reads, and the court's kernels compute.
- **D2 — Once a responder exists, the mercy that excused its silence is narrowed**
  (`palw_court_responder_coverage`).
- **D3 — The panel files the moves; the backend never sees a session.**
- **D4 — A wrong tile claim must cost as much as no claim.**
- **D6 — Admission refuses a fused class no dissection can try**, past its own fence. → spec 09
  PALW-CT-15.
- **D7 — A forged fold is bottomed** from the challenger's honest prefix and the root claim.
- **D8 — The root claim carries the anchor its bottom will need**, past its own fence.

## Consequences

- Fused attention rows are adjudicable, and responding is a node duty (spec 14).

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md) · [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md](../design/palw/archive/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
