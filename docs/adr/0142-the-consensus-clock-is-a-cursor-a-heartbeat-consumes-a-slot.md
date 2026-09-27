# ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the clock may not postpone it

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md](../design/palw/archive/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: Proposed 2026-09-18. **A consensus change, built and drilled**, armed nowhere on testnet-11.
  On testnet-12 the cursor (`palw_clock_cursor`) and the clock floor (amendment of 2026-09-24,
  `palw_clock_floor`) are armed from genesis: both are prerequisites of `palw_rcore_plus`.
* Date: 2026-09-18

## Context

A heartbeat's admissibility was measured against its selected parent, which every new chain block
replaces. Past the anchor clock, a block that advanced no clock therefore moved the next opportunity
to tick it.

## Decision

- **The invariant:** a block that does not advance the clock may not postpone it.
- **The rule:** a cursor (`PalwClockCursorV1`). A heartbeat is admissible iff `timestamp ≥
  next_slot_ms`. Only an admitted heartbeat writes the cursor, and missed slots are lost, not banked.
  → spec 06 PALW-EL-12.
- **One definition, four callers.**
- **§9 — The clock floor** (amendment of 2026-09-24).

## Consequences

- The properties to prove, and the operator's four checks (§6, §6a), are in the full text. Arming
  wants a reorg drill.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md](../design/palw/archive/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)
