# ADR-0141 — Can an inference be the ticket without a hash lottery?

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md](../design/palw/archive/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: Proposed 2026-09-18. **It decides nothing, changes no rule, and builds nothing.** It reads as
  an RFC (listed in [rfc/README.md](../rfc/README.md)).
* Date: 2026-09-18

## Context

With ADR-0140 keeping the heartbeat's hash deliberately, the attempt lottery is the only hashing left
that shapes who produces blocks. A producer pays for one full inference before the target comparison,
and most of those inferences are discarded.

## Decision

- **D1 — The lottery is not changed,** and no design is adopted here. The question is recorded.
- **D2 — The waste is counted before it is argued about:** the M1 counters are specified.
- **D3 — This ADR does not merge into ADR-0140:** the clock and the lottery are separate mechanisms.
- **D4 — Nothing is scheduled.**

## Consequences

- An inference-bound ticket without a hash lottery remains an open question. Reopening it means
  filing an RFC.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md](../design/palw/archive/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md)
