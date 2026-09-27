# ADR-0039: PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0039-palw-only-block-production.md](../design/palw/archive/0039-palw-only-block-production.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: Proposed and governing.
  - **D5 is superseded in part:** the epoch cap's currency by ADR-0045 D2 (blocks), and the share by
    ADR-0045 D3 → ADR-0054 → ADR-0137 (a share is a result).
  - D1–D4 and D6 govern.
* Date: 2026-08-17

## Context

"PALW is the consensus work" (ADR-0038) still left a hash floor and a hash term in fork choice, the
last two paths from hashing to consensus participation. The impossibility is stated rather than
finessed: a chain whose every block is inference halts when inference stops.

## Decision

- **D1 — The floor is a class, not a hash:** `PALW-BASE-0`, portable and integer-only, held Active.
  → spec 06 PALW-EL-16.
- **D2 — W6′: PALW-only liveness.** Total PALW unavailability halts loudly, instead of degrading to
  hashes. → spec 06 PALW-EL-15.
- **D3 — W4′: two derived weights, one fork choice.** → spec 13.
- **D4 — The ticket is not a hash puzzle.**
- **D5 — Per-class share and epoch caps.** *Superseded in part.*
- **D6 — Bonded is not permissioned.**

## Consequences

- ADR-0060 later re-admitted a bounded, near-weightless clock lane as the chain's clock only. That is
  not a production path.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0039-palw-only-block-production.md](../design/palw/archive/0039-palw-only-block-production.md)
