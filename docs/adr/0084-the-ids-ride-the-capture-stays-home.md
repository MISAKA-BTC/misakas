# ADR-0084: The ids ride, the capture stays home — a model-class claim serves its answer, never its history

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0084-the-ids-ride-the-capture-stays-home.md](../design/palw/archive/0084-the-ids-ride-the-capture-stays-home.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Proposed (2026-09-04), **implemented and consensus-inert**: no fence, no state field, no object
  variant, and the fingerprint does not move. It shipped by a rolling rebuild.
* Date: 2026-09-04

## Context

On testnet-11 Relaunch 5f a model-class claim's served material was its whole capture, which did not
fit a carrier, so seats could not be served.

## Decision

- **D1 — A third free-prompt payload,** the answer envelope `FPA1`. → spec 11 PALW-FP-8.
- **D2 — The resolver serves the envelope** whenever the capture does not fit.
- **D3 — Nothing over the cap is pushed,** and the block goes first.
- **D4 — The interval arm serves both lanes.**
- **D5 — One retention directory,** and the node names it.
- **D6 — The ids are read through the seam,** on either retention form.
- **D7 — The attempt lane's verdict is by execution** (added 2026-09-04).

## Consequences

- A claim serves its answer, never its history.

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0084-the-ids-ride-the-capture-stays-home.md](../design/palw/archive/0084-the-ids-ride-the-capture-stays-home.md)
