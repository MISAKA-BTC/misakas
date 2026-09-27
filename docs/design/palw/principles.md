# PALW's principles — design

> **Not normative.** This document is the argument behind
> [spec/palw/01-principles.md](../../spec/palw/01-principles.md), the backbone of the PALW spec.

**Decision recorded in:** [ADR-0144](../../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)
(the constitution) and ADR-0127 (PALW settles on its own).
**Last revised:** 2026-09-27

## 1. Why the constitution was written

The mechanism for "your own inference is the consensus work" existed, and it carried none of the
economy. On testnet-11 between 2026-09-15 and 2026-09-19 the node log shows nine free-prompt
commitments, against a block mix that was entirely synthetic attempts. A protocol that meant to buy
useful inference and is buying synthetic inference has become an LLM-flavoured proof of work. It will
stay one until the useful lane is the profitable one.

## 2. The unit ADR-0144 left open, and what answered it

STEP leaves could be shaped 427× by a registrant's `tile_len`. MAC-equivalents carry no memory
traffic term, and were never shown to track real cost. ADR-0144 fixed only the direction: a vector
derived from the graph, converted by coefficients no miner sets.

- **ADR-0145** built the vector.
- **ADR-0146** searched for coefficients, found the arbitrage bound at 1.0×, and so adopted no table.
- **ADR-0148/0149** made both lanes read the one derivation.

## 3. Order of work, and where it stands (2026-09-27)

| Item | State |
| --- | --- |
| 0. Do not widen the economy | In force (spec 01 PALW-PR-7) |
| 1. The constitution | Done |
| 2. Reward accounting | Done on testnet-12 (the ADR-0145 bundle, from genesis) |
| 3. Class admission | Done on testnet-12 (independence, the lifecycle, the Activation Pool) |
| 4. Local FreePrompt as the standard path | Partly: the entrance is built; decode rules and constraints are dormant; RFC-0001 is pending |
| 5. Shrink synthetic Attempt | Open |

## 4. How we will know it worked

A person uses MISAKA Studio normally for a day, never once changes a prompt to suit mining, and some
of that real inference earns. Nobody can change the reward-to-work ratio of the same inference by how
they write a profile or register a model.

## Source texts (archived ADR bodies)

- [ADR-0144 — PALW pays for the inference you were going to run anyway](archive/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)
