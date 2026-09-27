# ADR-0101 — A membership is proven by the chain and served by anyone, and a Position never moves between holders

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../design/palw/archive/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-10. **D2, D3 and D5 implemented, consensus-inert.** **ADR-0144 alignment
  (2026-09-21):** D4, D6 and D7's steps 2–4 (anyone-serves discovery and public serving) are withdrawn
  as unimplemented PALW work. The membership pins stay.
* Date: 2026-09-10

## Context

The membership half of ADR-0100's boundary. The line controls the product, providers control the
serving, and the chain proves the membership.

## Decision

- **D1 — Product, serving and membership are three parties' business.**
- **D2 — Which grant needs whom** (`palw_benefit_server_v1`).
- **D3 — The signed service descriptor,** and the check a client runs.
- **D4 — Discovery is not chain state.** *Withdrawn as PALW work.*
- **D5 — A Position is a membership and never money.**
- **D6 — Serving stays unadjudicated.** *Withdrawn as PALW work.*
- **D7 — Order of work.** *Steps 2–4 withdrawn.*

→ spec 15 PALW-MK-10.

## Consequences

- Positions never move between holders. Memberships are checked against chain facts.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../design/palw/archive/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md)
