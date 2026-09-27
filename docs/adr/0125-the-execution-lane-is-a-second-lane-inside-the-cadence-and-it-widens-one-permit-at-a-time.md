# ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time

> **Body moved (2026-09-27).** Normative rules → [spec/palw/12](../spec/palw/12-execution-lane.md); the full text as written → [design/palw/archive/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md](../design/palw/archive/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md); the reasoning is summarised in [design/palw/exec-lane.md](../design/palw/exec-lane.md).

* Status: **Implemented 2026-09-17**, dormant at first. Armed on testnet-11 at 7,101 (the `6001` bundle),
  and its span went from 5 DAA to 1 at 7,300. On testnet-12 it is open from genesis at one permit per
  round. Hardened by ADR-0130. Gas per round by ADR-0139. **Amended by ADR-0154:** tied round lanes
  are applied parents first, and a round block carries no EVM payload, from DAA 750.
* Date: 2026-09-17

## Context

The operator asked for 1 BPS on testnet, with 10 BPS as the target, for transactions without giving the
120-second PALW chain a fast clock.

## Decision

- **D1 — A round block is a sidecar:** algo 10, never a selected parent, always red, outside the DAA
  set.
- **D2 — A round block hangs from an anchor on the chain.**
- **D3 — A round is a second,** and its permits come from the previous span's finalized attempts.
- **D4 — The permit travels signed in the header** (`PXR1`).
- **D5 — The merging block decides the permit,** from its parent state.
- **D6 — The lane's mergeset rule and its fees.**
- **D7 — Widening is a stage table,** and a span keeps the width it opens with.
- **D8 — The chain merges the lane, and a node produces it.**

→ spec 12 §12.1–§12.2.

## Consequences

- The security amendments, the corrections, and the drill run on 2026-09-17 are in the full text.

## Links

- Spec: [12 Execution lane](../spec/palw/12-execution-lane.md)
- Design: [design/palw/exec-lane.md](../design/palw/exec-lane.md)
- Full text as written: [design/palw/archive/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md](../design/palw/archive/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
