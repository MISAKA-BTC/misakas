# ADR-0060: The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay

> **Body moved (2026-09-27).** Normative rules → [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0060-the-liveness-doctrine.md](../design/palw/archive/0060-the-liveness-doctrine.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: Accepted as doctrine. It was implemented on 2026-08-30 and shipped off the same day, after
  the audit. **D1/D2/D4 were re-implemented in ADR-0066's form** (`algo_id` 8, fixed target, fences)
  and armed from genesis on testnet-11 Relaunch 5 and on testnet-12 through ADR-0068. The inactivity
  leak (D4) stays dormant on testnet-12.
* Date: 2026-08-30

## Context

A family of measured failures had the same root: a PALW chain whose producers stopped could not
restart, because every lane that could advance time also needed bonded work or a vote.

## Decision

- **The doctrine: time is permissionless, weight is bonded, finality is an overlay.** → spec 13.
- **D1 — A heartbeat lane.** *Re-implemented by ADR-0066.*
- **D2 — An emergency ramp.** *Re-implemented by ADR-0066.*
- **D3 — Producer-bond self-healing is unconditional.**
- **D4 — A finality inactivity leak,** overlay-scoped. It needs committed per-validator state
  (ADR-0066 D4) and is dormant on testnet-12.
- **D5 — Refusal gates decay** (partly landed).

## Consequences

- The degradation ladder (§10) and the audit's same-day changes (§12) are in the full text.
- Every later liveness rule (0066, 0068, 0105, 0140, 0142, 0151) is judged against this doctrine.

## Links

- Spec: [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0060-the-liveness-doctrine.md](../design/palw/archive/0060-the-liveness-doctrine.md)
