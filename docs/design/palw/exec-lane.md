# The execution lane — design

> **Not normative.** This document explains why the rules in
> [spec/palw/12-execution-lane.md](../../spec/palw/12-execution-lane.md) are what they are.

**Decisions recorded in:** ADR-0125, 0129, 0130, 0139, [ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md) (D13).
**Last revised:** 2026-09-27

## 1. Problem

People want transactions confirmed in seconds. PALW anchors come every 120 seconds, because each is a
verified inference. Speeding up the PALW clock would multiply verification load and weaken every
window. A fast lane must not become a second clock or a source of finality.

## 2. The design in one paragraph

Every second is a round, served by round blocks that are always red and outside the DAA set, so the
chain is exactly the chain without them. Permits go, one per operator, to bonds whose attempts reached
Final in the span before last. Each is weighted by the compute it certified, capped per security
domain, and scheduled by an anchor mined after the participants were fixed. A round buys a fixed gas
budget. Finality is settlement depth in PALW anchors, never a count of round blocks. The IBD audit of
2026-09-26 found that tied round-lane blocks could be applied child-before-parent. That led to the
parents-first rule and the ban on EVM payloads in round blocks (DAA 750).

## Source texts (archived ADR bodies)

- [ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time](archive/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
- [ADR-0130 — BPS 1 is hardened before it is widened](archive/0130-bps1-is-hardened-before-it-is-widened.md)
