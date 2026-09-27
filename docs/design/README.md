# Design index

Design documents explain why the rules are what they are: the rationale, the rejected alternatives,
the threat models, the measurements and the trade-offs. They are not normative. The rules are in
[`spec/`](../spec/README.md). [INDEX.md](../INDEX.md) §3.2 gives the shape of a design document.

## PALW

Written in Phase 2 (2026-09-27), except `claim-capacity.md`, which waits for ADR-0160. Each file ends
with the list of archived ADR texts it summarises. The archive is [`palw/archive/`](palw/archive/),
with ADR-0152 in [`palw/archive/0152/`](palw/archive/0152/README.md) and the old ADR index in
[`palw/archive/adr-index-2026-09-27.md`](palw/archive/adr-index-2026-09-27.md).

| File | Topic | Spec chapters it explains |
| --- | --- | --- |
| [`palw/principles.md`](palw/principles.md) | ADR-0144's measurements and argument, the unit it left open, how success is judged | 01 |
| [`palw/lineage.md`](palw/lineage.md) | How PALW got here: algo 4 → V2 → the RC ruleset → testnet-11 → testnet-12 (R-core+), and the superseded decisions in order | all |
| [`palw/state-and-carriage.md`](palw/state-and-carriage.md) | Object carriage, validation layers, state-root ordering, mass budgets | 02 |
| [`palw/registry.md`](palw/registry.md) | Classes as chain data, permissionless admission, certification, lifecycle, artifact ownership | 03 |
| [`palw/execution.md`](palw/execution.md) | BASE-0 and its tiers, kernels, the step function, runtime backends | 04 |
| [`palw/held-context.md`](palw/held-context.md) | Held context, capture folds, attention history, context limits | 04, 09, 14 |
| [`palw/work.md`](palw/work.md) | Canonical work, coefficients and the arbitrage bound, the work target, economic-compute shadows | 05 |
| [`palw/lottery.md`](palw/lottery.md) | The beacon, the ticket, the single lottery, the clock | 06 |
| [`palw/lifecycle.md`](palw/lifecycle.md) | Claim states, anchoring and binding, licence, finality, settlement | 07 |
| [`palw/verification.md`](palw/verification.md) | Panels, the stake-weighted draw, readiness, receipts, replay, DA | 08 |
| [`palw/court.md`](palw/court.md) | Adjudication, bisection, the two-tile refutation, attribution | 09 |
| [`palw/collateral.md`](palw/collateral.md) | Bonds, reservations, locks, slashing, vesting, emission, "collateral covers fraud" | 10 |
| `palw/claim-capacity.md` | ADR-0160 v3: claim capacity separated from collateral price | 10 |
| [`palw/free-prompt.md`](palw/free-prompt.md) | The free-prompt lane, the local entrance, served answers, prefix state | 11 |
| [`palw/exec-lane.md`](palw/exec-lane.md) | Round blocks, permits, gas per round, anchors not blocks | 12 |
| [`palw/liveness.md`](palw/liveness.md) | The liveness doctrine, heartbeat, clock cursor, fork choice, reorg authority | 13 |
| [`palw/node.md`](palw/node.md) | Node duties, operator interface, artifacts and memory, host sandbox | 14 |
| [`palw/market.md`](palw/market.md) | Model lines, the store curve, seeds, memberships, the owner's leg | 15 |

**ADR-0160 (claim capacity v3)** is an accepted design being implemented (the `rcore/cap-*` lanes).
Its text is on branch `rcore/cap-spec2` (`ccd5499c8`), with its arithmetic under
`docs/adr/0160-capacity/`. When it lands, the body becomes `palw/claim-capacity.md`, a short ADR-0160
records the decision, and the dormant fences join spec chapter 16.

## Other domains (Phase 3)

`evm/`, `dns-bft/`, `bridge/`, `network/` and `wallet/`. Existing design documents at the top of
`docs/` (for example `misaka-evm-design-v0.4.md` and `misaka-palw-slash-protocol-design-v0.1.md`)
move here when their domain is done.
