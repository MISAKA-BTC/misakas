# ADR-0038: PALW is the consensus work — sampled-verified LLM PoW, a receipt-licensed weight ramp, and a hash anti-stall floor

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0038-palw-is-the-consensus-work.md](../design/palw/archive/0038-palw-is-the-consensus-work.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: Accepted (architecture decision), amended.
  - It **supersedes ADR-0037 D1**: PALW is the consensus work, not an asynchronous credit beside a
    hash floor.
  - **Amended:** W4/W6 by ADR-0039 (W4′, W6′: PALW-only production and two derived weights). "No hash
    lane at all" by ADR-0060/0066/0068: a bounded, near-weightless clock lane re-enters as the clock
    and nothing else.
* Date: 2026-08-17

## Context

ADR-0037 kept hash PoW as the primary consensus work and made PALW never block-critical. That secured
the chain by making the chain's thesis optional.

## Decision

- **A — The layer inversion:** PALW is the consensus work. → spec 06.
- **B — Acceptance now, weight later:** a receipt-licensed weight ramp.
- **C — Verification:** assigned sampling is the alarm, and the court is the truth. → spec 08, 09.
- **D — Multi-class difficulty:** per-class DAA, with static pwu only inside a class. *Superseded by
  ADR-0137's one work target.*
- **E — What hash still does.** → spec 06 PALW-EL-15.
- **F — Fork choice, IBD and the fabrication problem.** → spec 13.
- **G — What survives from ADR-0037,** re-seated: the state machine, binding, panels, classes and mint
  hygiene.
- **H — Block cadence is frozen at one block per 120 seconds,** on testnet and mainnet. → spec 06
  PALW-EL-13.

## Consequences

- The implementation status of Decision A, C's assigned duty and C's freeze clause (as wired) are in
  the full text.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0038-palw-is-the-consensus-work.md](../design/palw/archive/0038-palw-is-the-consensus-work.md)
