# ADR-0133 — Verification is its own clock: a class verifies over spans, and a starved class stops only itself

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md](../design/palw/archive/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Proposed 2026-09-17, with the profile, capacity arithmetic, simulation and observability
  built in shadow. **Verification V2 (S1, §11.1) and readiness V2 (§11.2) were built and fenced at
  testnet-11 DAA 6,100.** The class hold (§11.3) and the class receipt window (`palw_class_receipt_window`,
  testnet-11 DAA 8,160) were built later. On testnet-12 all are armed from genesis, with the readiness
  horizon at 24 spans (ADR-0152) and class-derived deadlines (ADR-0152 §4-quater).
* Date: 2026-09-17

## Context

No claim had ever had to verify within 120 seconds, because a panel holds its claim for hundreds of
anchors. What bounds a large model is its panel's capacity over the class's window, and the bytes a
seat must move before it can replay. A starved class must stop only itself.

## Decision

- **D1 — Three clocks, kept apart:** the execution round (1 s), the PALW anchor (120 s), and
  verification. → spec 08 PALW-VF-32.
- **D2 — `PalwVerificationProfileV1`, derived and never chosen.** *Superseded the same day by
  ADR-0135.*
- **D3 — Class-local fail-closed:** a class whose bound claims cannot be served stops itself. → spec 08
  PALW-VF-29, 03.
- **D4 — Capacity is sized by Little's law at 60–70 %.**
- **D5 — Collateral:** in-flight duties hold `inflight × seats × seat_exposure`.
- **D6 — The artifact is a runtime problem,** priced apart from compute.
- **D7 — A seat is a verdict, not a GPU.**
- **D8 — Credits come only from `Final`.**
- **§11.1 — Verification V2:** segments and V3 receipts. → spec 08 PALW-VF-33.
- **§11.2 — Readiness V2:** a 16-leaf multiproof over the whole artifact. → spec 08 PALW-VF-27.
- **§11.3 — A class whose window does not fit is held**, and past `palw_class_receipt_window` it is
  judged by its own window.

## Consequences

- A Kimi-class model's activation procedure (§9) and the grid simulation (§6) are in the full text.
- Readiness stopped being satisfiable by holding one eight-leaf window (audit finding H-6).

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md) · [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md](../design/palw/archive/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)
