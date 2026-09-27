# ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md); the full text as written → [design/palw/archive/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md](../design/palw/archive/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: Proposed 2026-09-17. **D1–D2 implemented in shadow** (node-local; no consensus rule, parameter
  or fingerprint moves). **ADR-0144 alignment (2026-09-21):** the shadow may measure, but D3–D6 (arming
  a scalar CCU) are not to be armed until ADR-0146, which found no coefficient table justified.
* Date: 2026-09-17

## Context

testnet-11 paid claims in leaves. The measured gap was 86.4 %: the hybrid was paid more per
MAC-equivalent it ran than the dense class.

## Decision

- **D1 — Measure first:** per-class economics over RPC (op 185, `misaka palw economics`).
- **D2 — Kernel-weighted compute:** deterministic and hardware-independent (shadow).
- **D3 — The reward basis, when it moves,** is expected attempts × draw compute + job compute.
  *Withheld.*
- **D4 — A panel is paid for verification compute, measured separately.** *Withheld.*
- **D5 — The unit is not the heaviest class:** pricing needs a rate. *Withheld.*
- **D6 — A new model earns under the rate only after a shadow period.** *Withheld.*

→ spec 05 PALW-WK-6.

## Consequences

- The economics census is observability. ADR-0132 (Upgrade C) and ADR-0137 carried the pricing that
  did arm.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md](../design/palw/archive/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md)
