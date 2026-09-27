# ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory

> **Body moved (2026-09-27).** Normative rules → [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md](../design/palw/archive/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: **D1, D2 and D4's fence landed (2026-08-31); D3 landed through ADR-0068 Phase 1**
  (`palw_attempt_work`). Armed from genesis on testnet-11 Relaunch 5 and on testnet-12, except the
  inactivity leak. **D2's slot rule is superseded by ADR-0142's cursor.** ADR-0083 amends D1 by one
  sentence: heartbeat rows are not counted by the difficulty window.
* Date: 2026-08-31

## Context

The first heartbeat implementation (ADR-0060 D1/D2) priced the lane in `header.bits`, which was
self-perpetuating, and walked node-relative evidence. The audit withdrew it on the day it shipped.

## Decision

- **D1 — The lane gets its own algorithm id (8), and its price never touches `bits`.** → spec 13
  PALW-FC-11.
- **D2 — The slot rule is one block deep, and the evidence walk is deleted.** *Slot rule superseded by
  ADR-0142.*
- **D3 — ε stops competing with a V2 block's work:** attempt blue work is a constant. → spec 13
  PALW-FC-3.
- **D4 — The inactivity leak needs committed per-validator state.** It is fenced, and dormant on
  testnet-12.

## Consequences

- The trap in the constant (why both fences must move together) and the costs are in the full text.

## Links

- Spec: [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md](../design/palw/archive/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
