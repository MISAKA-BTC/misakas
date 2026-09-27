# ADR-0140 — The heartbeat is the emergency generator: it must not touch the economy or the difficulty while the chain is producing

> **Body moved (2026-09-27).** Normative rules → [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0140-the-heartbeat-is-the-emergency-generator.md](../design/palw/archive/0140-the-heartbeat-is-the-emergency-generator.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: Proposed 2026-09-18. **It changes no consensus rule and arms no fence.** It states the goal,
  and the non-interference invariant as five claims.
* Date: 2026-09-18

## Context

The goal "remove the hash from the heartbeat" was wrong. The goal is that the heartbeat cannot touch
the economy, the difficulty, fork choice or the clock while the chain is producing, and that it can
carry all four alone when the chain is not.

## Decision

- **D1 — The hash heartbeat stays.** It is the emergency generator's price.
- **D2 — The goal is non-interference while producing, and a reliable start when stopped.** → spec 13
  PALW-FC-13.
- **D3 — Consensus admissibility stays unconditional.**
- **D4 — When to mine is policy, and stays policy.**
- **D5 — C1–C5 get one standing guard,** and the lane gets counted.
- **D6 — The attempt lottery is the real hash question** (ADR-0141).
- **D7 — No zero-knowledge machinery in this line of work.**

## Consequences

- PoSW and VDF alternatives were rejected. The reasons are in the full text.

## Links

- Spec: [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0140-the-heartbeat-is-the-emergency-generator.md](../design/palw/archive/0140-the-heartbeat-is-the-emergency-generator.md)
