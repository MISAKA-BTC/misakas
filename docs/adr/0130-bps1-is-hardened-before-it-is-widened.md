# ADR-0130 — BPS 1 is hardened before it is widened

> **Body moved (2026-09-27).** Normative rules → [spec/palw/12](../spec/palw/12-execution-lane.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0130-bps1-is-hardened-before-it-is-widened.md](../design/palw/archive/0130-bps1-is-hardened-before-it-is-widened.md); the reasoning is summarised in [design/palw/exec-lane.md](../design/palw/exec-lane.md).

* Status: Accepted 2026-09-17. **Decisions 2–6 implemented**; D1 built dormant (λ); D7–D8 not built.
  Rode testnet-11's DAA 6,001 flag day. On testnet-12, λ = 5 is armed from genesis
  (`palw_panel_exposure_floor`), and D2's one entry per operator is replaced by the stake-weighted
  draw (ADR-0152 SW). **ADR-0144 alignment (2026-09-21):** widening, the derived panel share and the
  DA reward stay deferred under 0144 §6 item 0.
* Date: 2026-09-17

## Context

At one execution block per second, a panel seat risked more than it could earn on the claim it
judged. The draw gave an operator one entry however its collateral was split. Nothing stopped an
operator holding consecutive rounds. A span's producers were chosen by an anchor mined after their set
was fixed.

## Decision

- **D1 — A seat reserves at least λ times what it can earn** (dormant on testnet-11). → spec 08
  PALW-VF-39.
- **D2 — One entry per operator.**
- **D3 — An operator never holds two consecutive rounds.** Parity is assigned per operator, and a
  round with no eligible operator is a miss. → spec 12.
- **D4 — Participants first, then a future anchor**, for a span's schedule. → spec 12.
- **D5 — testnet-11's span is 5 DAA.** (testnet-12: 1.)
- **D6 — The width stays 1** (BPS 1).
- **D7 — A receipt's verdict may resolve, not reverse** (recorded, not built).
- **D8 — λ runs in shadow** (node-local economics).

## Consequences

- BPS 1 is hardened before any widening. Widening needs its own fence, and ADR-0144 defers it.

## Links

- Spec: [12 Execution lane](../spec/palw/12-execution-lane.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/exec-lane.md](../design/palw/exec-lane.md)
- Full text as written: [design/palw/archive/0130-bps1-is-hardened-before-it-is-widened.md](../design/palw/archive/0130-bps1-is-hardened-before-it-is-widened.md)
