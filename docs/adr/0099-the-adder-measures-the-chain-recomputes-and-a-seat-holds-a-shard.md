# ADR-0099 — The adder measures, the chain recomputes, and a seat holds a shard

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md](../design/palw/archive/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Proposed 2026-09-10. **D1–D4 and D6 implemented, consensus-inert.** D5's fence
  (`palw_shard_court`) was declared `None` and is refused at assembly. Per-shard licensing is dormant on
  testnet-12.
* Date: 2026-09-10

## Context

Anyone should be able to add a model: they hand the network a manifest, and the network's own tool
measures everything that can be computed from it. A model too large for one seat needs shards.

## Decision

- **D1 — The manifest is the model definition.** The Measured Model Artifact is what the tool derives.
- **D2 — A shard is a contiguous layer range,** and the plan is derived.
- **D3 — A shard is a capability, and the panel is stratified.** *(Corrected by ADR-0100.)*
- **D4 — Licensing past eight shards** is named, not built.
- **D5 — The court a shard can open,** behind `palw_shard_court`.
- **D6 — Admission recomputes, and never believes a rate.** → spec 03 PALW-CL-5.
- **D7 — The adder measures first, with the tool** (`palw-shard-plan`).
- **D8 — The ruleset is still the ruleset:** a plan admits no class.

## Consequences

- Shards stay dormant on testnet-12, which is the shard rule spec 08 PALW-VF-30 states.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md](../design/palw/archive/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
