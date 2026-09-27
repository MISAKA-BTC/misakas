# ADR-0072 — The ticket is the execution: both lotteries priced in inferences

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0072-the-ticket-is-the-execution.md](../design/palw/archive/0072-the-ticket-is-the-execution.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: **Implemented (2026-09-02).** Decision 7's rollout was decided the same day: it went live
  inside Relaunch 5's regenesis, with no fence. A same-day review added Decision 8. Security
  amendments were added in three passes (2026-09-02/03).
* Date: 2026-09-02

## Context

A producer could draw a ticket per nonce over one execution. The priced bytes were not the execution,
so a winning ticket could be ground without running more inference.

## Decision

- **D1 — The priced bytes are the execution:** `execution_commitment_v3`, with only the challenge
  blanked.
- **D2 — The anchor is derived, never carried** (`execution_anchor_v3`).
- **D3 — Both lotteries draw from it:** the class ticket and the Layer-0 digest.
- **D4 — The class lottery moves beside the position** (`check_palw_class_lottery_v3`).
- **D5 — One draw is one execution:** `pwu = max(1, expected_draws) × per_inference`.
- **D6 — The producer: one template, one inference, one draw.** The nonce search is deleted.
- **D7 — Version:** `PALW_ATTEMPT_V2_VERSION` 5 → 6.
- **D8 — Every field inside the priced bytes is pinned, or it is the challenge.**

→ spec 06 §6.2.

## Consequences

- Grinding a ticket costs one inference per try.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0072-the-ticket-is-the-execution.md](../design/palw/archive/0072-the-ticket-is-the-execution.md)
