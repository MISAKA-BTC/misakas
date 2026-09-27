# ADR-0046 — PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts ride their evidence

> **Body moved (2026-09-27).** Normative rules → [spec/palw/02](../spec/palw/02-state-objects-and-carriage.md); the full text as written → [design/palw/archive/0046-palw-v2-consensus-object-carriage.md](../design/palw/archive/0046-palw-v2-consensus-object-carriage.md); the reasoning is summarised in [design/palw/state-and-carriage.md](../design/palw/state-and-carriage.md).

* Status: Accepted (2026-08-20). **Implemented with one change:** D1's subnetwork band `0x50`–`0x55`
  was not used. Lifecycle objects ride `SUBNETWORK_ID_PALW_LIFECYCLE` (`0x4b`) as tagged
  `PalwConsensusObjectV2` values (spec 02 PALW-ST-7; divergences.md).
* Date: 2026-08-20

## Context

The V2 ruleset (ADR-0042) needed its consensus objects — bond registrations, retirements, bindings,
licences and court moves — to ride transactions. The carriage had to be unambiguous about which
validator reads which payload, and whether a failed object invalidates its block. The chain already
had the right precedents in the stake-bond rail and the DNS overlay.

## Decision

- **D1 — One subnetwork id per kind, a Borsh body, no magic.** Timeout edges are never carried. They
  are functions of the deadline index. *(In code, one lifecycle id carries a tagged object enum.)*
- **D2 — Two validation layers.** Stateless shape is checked at isolation, and failure invalidates the
  block. Stateful admission is checked at acceptance, and failure skips the carrier without
  invalidating the block. The transition receives only accepted carriers.
- **D3 — The bond is its collateral output.** The operator co-signs its adoption of the executor key.
  Withdrawal pays at most `collateral − slashed`, and the slashed part goes to the PALW burn script,
  never to fee.
- **D4 — Panels are derived; receipts are carried** with their evidence.
- **D5 — Order is acceptance order.**
- Mass: every object fits the 480,000 standard cap.

## Consequences

- A transition error on a carried object is unreachable. Only the header's attempt can fail a block.
- In code, a registration declares its collateral, and the carrier's output must hold at least that
  amount (`palw_bond_registration_binds_its_carrier_v2`). D3 had "read, not declared".

## Links

- Spec: [02 State, objects and carriage](../spec/palw/02-state-objects-and-carriage.md)
- Design: [design/palw/state-and-carriage.md](../design/palw/state-and-carriage.md)
- Full text as written: [design/palw/archive/0046-palw-v2-consensus-object-carriage.md](../design/palw/archive/0046-palw-v2-consensus-object-carriage.md)
