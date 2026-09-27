# ADR-0118 — The held regime arrives at a height, and a held class carries its own prompt form

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/04](../spec/palw/04-execution-semantics.md), [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md](../design/palw/archive/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed and **implemented 2026-09-12**. testnet-11 took the held regime at its DAA 6,000
  flag day (later mapped to 7,100). testnet-12 has it from genesis.
* Date: 2026-09-12

## Context

A network minted before the held regime needed to take it at a height, and a held class needed its own
prompt form while every other class kept the network's.

## Decision

- **D1 — Past genesis, the fence commits the V4 signing contexts itself.**
- **D2 — The one-move court takes the fence's contexts** at the same height.
- **D3 — The prompt-ids form is the class's.** A held class uses the tiled Merkle root.
- **D4 — A prompt tile is demanded only of a claim whose ids are a tile tree.**
- **D5 — The carriers are the chain's.**
- **D6 — testnet-11's flag day is one line,** and it is the operator's.
- **D7 — A chain-registered class is served at the ruleset's ladder.**

→ spec 03 §3.7, 04 §4.5.

## Consequences

- The measurement of what arming buys on testnet-11 (§4) is in the full text.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md) · [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md](../design/palw/archive/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)
