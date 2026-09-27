# ADR-0034: PALW re-verification routing — four execution-class families, five model bands, one deciding binding

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0034-palw-execution-class-model-band-routing.md](../design/palw/archive/0034-palw-execution-class-model-band-routing.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Accepted (architecture; activates nothing). **Amended 2026-08-26 by ADR-0053 D5:** the
  `Metal` routing family is reserved beside `Cuda` and `Rocm`, and only the CPU integer family
  adjudicates.
* Date: 2026-08

## Context

A receipt must say which execution class and which model size it attests, so work can be routed to
seats that can re-execute it. A draft proposed monopoly claiming and finalizing without replay.

## Decision

- **D1 — Three keys:** the execution-class family, the model band, and the deciding binding. Each key
  may touch only its own decisions. The CPU family is not the adjudicator by default.
- **D2 — The draft's vocabulary is mapped onto this fork's identifiers.**
- **D3 — The registry holds the definitions and bindings** (layout generation 2).
- **D4 — The model band is derived from the binding** and capped by the pruning horizon.
- **D5 — Receipts carry the keys; the registry, not the miner, gives them meaning.** → spec 08
  PALW-VF-31.
- **D6 — Verifier capability has two layers,** and an agent that registers itself.
- **D7 — Every credited receipt gets an assigned, funded panel.** Escalation replaces monopoly
  claiming. `FINALIZED_WITHOUT_REPLAY` is rejected.

## Consequences

- Routing decides who replays, never whether a lie is proven.
- With one execution family (ADR-0053), routing reduces to capability and band on testnet-12.

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0034-palw-execution-class-model-band-routing.md](../design/palw/archive/0034-palw-execution-class-model-band-routing.md)
