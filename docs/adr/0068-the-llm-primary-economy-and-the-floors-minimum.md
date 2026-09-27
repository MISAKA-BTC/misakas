# ADR-0068: The LLM-primary economy — the floor retires to the doctrine's minimum

> **Body moved (2026-09-27).** Normative rules → [spec/palw/10](../spec/palw/10-collateral-and-economics.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md), [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0068-the-llm-primary-economy-and-the-floors-minimum.md](../design/palw/archive/0068-the-llm-primary-economy-and-the-floors-minimum.md); the reasoning is summarised in [design/palw/lineage.md](../design/palw/lineage.md).

* Status: Accepted. **Phase 1 implemented and drilled on 2026-09-01.** F2 and F3a are closed in code.
  Phase 2 (the floor's minimum, 20 ‰, and the clock lane armed from genesis) shipped with testnet-11
  Relaunch 5. Shares as lottery inputs were later superseded by ADR-0137.
* Date: 2026-09-01

## Context

The floor class (BASE-0) held a 500 ‰ reserve of the cadence. The economy was meant to be carried by
models, and the floor was meant to be only the liveness minimum that the doctrine (ADR-0060) requires.

## Decision

- **Three phases:** measure, close F2/F3a (Phase 1), then re-mint with the floor at the doctrine's
  minimum and the clock lane armed from genesis (Phase 2).
- **F2 — the attempt lane's blue work leaves `calc_work(bits)`.** It is the constant
  `1 << PALW_ATTEMPT_BLUE_WORK_LOG` behind `palw_attempt_work`. → spec 06, 13.
- **F3a — sibling heartbeat width is bounded** where `mergeset_size_limit` lives: at most four
  heartbeats per mergeset or chain. → spec 13.
- The floor retires to 20 ‰ (Relaunch 5). *Share mechanics were superseded by ADR-0137.*

## Consequences

- testnet-11 Relaunch 5 onward armed the heartbeat lane and the attempt-work constant from genesis,
  and testnet-12 inherits them.
- The rejected alternatives and the invariants to verify at each step are in the full text.

## Links

- Spec: [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md) · [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/lineage.md](../design/palw/lineage.md)
- Full text as written: [design/palw/archive/0068-the-llm-primary-economy-and-the-floors-minimum.md](../design/palw/archive/0068-the-llm-primary-economy-and-the-floors-minimum.md)
