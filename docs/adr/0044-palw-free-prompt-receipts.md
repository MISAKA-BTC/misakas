# ADR-0044: Free-prompt PALW — the user's own inference becomes the consensus work, certified before it mines

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0044-palw-free-prompt-receipts.md](../design/palw/archive/0044-palw-free-prompt-receipts.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Proposed 2026-08 as the engineering spec of the free-prompt lane, implemented on the V2 lineage
  and governing as amended:
  - D4's "the receipt lane is weightless" was amended by ADR-0073, and the beacon rule is kept as law
    (the attempt lane draws from it too, ADR-0074).
  - D6's receipt block was extended by ADR-0055 (earned position).
  - **D7's CU pricing was withdrawn** (ADR-0074 D5, then derived work, ADR-0148).
  - The certified set is chain state (ADR-0075).
* Date: 2026-08

## Context

What was asked: a person's own local inference becomes the consensus work, with a certificate before
it mines. The two flaws found in the first design are recorded in the full text.

## Decision

- **D1 — Two work sources, one atomic bundle:** attempts and free prompts.
- **D2 — The free-prompt job:** user tokens in, nothing appended, total binding. → spec 11 PALW-FP-1.
- **D3 — The execution commitment,** and certification through the existing lattice. → spec 11
  PALW-FP-2.
- **D4 — The beacon rule:** only attempt blocks carry randomness. → spec 06 PALW-EL-1.
- **D5 — Quantized one-shot tickets.** → spec 11 PALW-FP-5.
- **D6 — The receipt block (algo 7):** admission a full node runs with no model. → spec 11 PALW-FP-6.
- **D7 — CU pricing.** *Withdrawn.*
- **D8 — Data availability and privacy, v1.** → spec 11 PALW-FP-3.
- **D9 — Bundle extension, invariants,** and what moves.
- **D10 — The user pipeline:** one inference, an answer and a commitment. → spec 11 PALW-FP-4.

## Consequences

- "The user's own inference becomes the consensus work" is the data layer ADR-0144 later made the
  constitution.

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0044-palw-free-prompt-receipts.md](../design/palw/archive/0044-palw-free-prompt-receipts.md)
