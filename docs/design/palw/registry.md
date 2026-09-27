# Classes, registry and admission — design

> **Not normative.** This document explains why the rules in
> [spec/palw/03-classes-and-registry.md](../../spec/palw/03-classes-and-registry.md) are what they are.

**Decisions recorded in:** ADR-0049 H, 0056, 0067, 0069, 0070, 0075, 0099, 0100, 0135, 0143, 0145 §7,
0147, [ADR-0152](../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) (the Activation
Pool, deadlines), and ADR-0144 P7 (the constitution).
**Last revised:** 2026-09-27

## 1. Problem

P7 asks for two things that pull against each other:

- anyone can add a model without changing `main`;
- nobody can buy weight, reward or eligibility by registering one.

A third requirement came from the operator on 2026-09-25: a registration is a **listing**.
Listings are long-lived and asynchronous, and spam is priced in MSK, not limited by deadlines.

## 2. The design in one paragraph

- A class is chain data. Its identity, work and profile are derived by every node from the manifest,
  and the court must be able to try it before it is admitted.
- A registration starts as a `Candidate` that earns nothing.
- It advances only on verified events: a drawn jury's audit, ready seats the registrant does not hold,
  completed probe claims.
- Each state sets **how much** the class may contribute (50 / 100 / 1,000 ‰), never what a unit of
  work is worth.
- Independence is drawn, not declared: an admission jury, and an outsider seat on every claim.
- Certification is a consensus object, readable as genesis ∪ chain.
- The Activation Pool lets anyone fund the operators who prove they prepared a model, and pays for
  preparation, never for a vote.

## 3. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| A compiled class list | Adding a model needs a release, which contradicts P7 | archive 0067 |
| A certified set in the binary | Same | archive 0075 §1 |
| Declared independence (an identity test) | Identity is free, so it proves nothing | archive 0147 §1 |
| Shares granted at admission | A share becomes a thing to buy; replaced by "a share is a result" | ADR-0137 |
| Disarming the registry "for safety" | The registry does not gate registration; it is the only throttle, via the lifecycle (ADR-0144 §2) | ADR-0144 |
| A weighted admission jury | It refuses honest classes that genesis seats do not load | archive 0152/06 SW-5 |
| Deadlines before the first panel | A listing waits for the network, not the other way round (operator, 2026-09-25) | `palw_activation_pool_v1.rs` |

## 4. Residual risks

- A registrant's Sybils can capture a jury. They cannot capture the stake-weighted outsider seat,
  without whose `Valid` a claim does not license.
- A skipped audit is not deferred: about 41 % of a Candidate's audits never sit at testnet-12's anchor
  rate (`palw_activation_pool_v1.rs` TODO, v2).

## Source texts (archived ADR bodies)

- [ADR-0067 — Classes are chain data; only kernels are the build](archive/0067-classes-are-chain-data-kernels-are-the-build.md)
- [ADR-0069 — End-to-end adjudicability is the price of weight](archive/0069-e2e-adjudicability-is-the-price-of-weight.md)
- [ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its lifecycle](archive/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md)
- [ADR-0100 — A model is data: the one-move court, the held measurement and the licence per shard, and the boundary "permissionless" means](archive/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
- [ADR-0075: Certification is a consensus object](archive/0075-certification-is-a-consensus-object.md)
- [ADR-0099 — The adder measures, the chain recomputes, and a seat holds a shard](archive/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
- [ADR-0056: Permissionless class admission, and the share economy that survives it](archive/0056-palw-permissionless-class-admission-and-share-economy.md)
- [ADR-0070: The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them](archive/0070-the-model-tiers-step-spaces-are-adjudicable.md)
- [ADR-0143 — An artifact root has one owner on the chain, and competing weights stay permissionless](archive/0143-an-artifact-root-has-one-owner-on-the-chain.md)
- [ADR-0147 — Independence is drawn, not declared](archive/0147-independence-is-drawn-not-declared.md)
