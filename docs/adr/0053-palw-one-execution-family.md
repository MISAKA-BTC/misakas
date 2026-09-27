# ADR-0053: One execution family — Family M is withdrawn, and the court is not optional

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0053-palw-one-execution-family.md](../design/palw/archive/0053-palw-one-execution-family.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Accepted (2026-08-26). **It supersedes ADR-0051**, re-scoped by ADR-0067: the registered
  profile is the authority. It moved `PALW_STATE_V2_VERSION` 7 → 8, and every network re-minted.
* Date: 2026-08-26

## Context

The Metal/GGUF family's motive expired: Qwen3.6 ran through the integer runtime. Then its mechanisms
turned out not to exist. The 500 ‰ cap was never written. The per-class panel was checked and ignored.
The admission arm skipped every check that makes a class prosecutable, and it could not have run.

## Decision

- **D1 — There is one execution family, and it is not a value.** → spec 04 PALW-EX-1.
- **D1a — What "one gate" does and does not claim, at genesis.**
- **D2 — `PalwClassTermsV2` is deleted,** and the class record shrinks.
- **D3 — One panel: the network's.**
- **D4 — `--palw-register-class` survives,** pointed at a class the court can judge.
- **D5 — ADR-0034's `Metal` routing family becomes reserved.**
- **D6 — What is deleted,** in full.

## Consequences

- The court is not optional. ADR-0026's thesis is restored in full.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0053-palw-one-execution-family.md](../design/palw/archive/0053-palw-one-execution-family.md)
