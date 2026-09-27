# ADR-0042: The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint

> **Body moved (2026-09-27).** Normative rules → [spec/palw/02](../spec/palw/02-state-objects-and-carriage.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/07](../spec/palw/07-claim-lifecycle.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md), [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0042-palw-mainnet-candidate-ruleset.md](../design/palw/archive/0042-palw-mainnet-candidate-ruleset.md); the reasoning is summarised in [design/palw/lifecycle.md](../design/palw/lifecycle.md).

* Status: Proposed 2026-08-20 as the RC engineering spec. **Implemented as the V2 lineage
  (`ConsensusV2`)**, which every PALW network has run since testnet-11 Relaunch 2. The index lists it as
  governing. Amendments A1–A4 (implementation time, 2026-08-20) are in the full text.
* Date: 2026-08-20

## Context

The 2026-08-19 external audit returned NO-GO: ten P0s and two blockers. Its findings included:

- the PoW did not consume the commitment;
- weight was read from node-local stores;
- the court fetched model rows the node did not have;
- rewards were spendable before a claim could still be voided;
- five independent `Option` fences could combine into states nobody had designed.

A PALW release candidate needed one ruleset that is either wholly on or off, is identical on the RC
and on mainnet, and is checkable by hash.

## Decision

- **D1 — One atomic activation bundle.** A single mode enum replaces the five fences; `ConsensusV2`
  carries all of the ruleset or none of it. → spec 02 PALW-ST-2, 16.
- **D2 — One block state machine,** in which the state only advances and weight is a function of the
  state. → spec 07 §7.1.
- **D3 — `PalwAttemptEnvelopeV2`, a new algo id, and identity by `attempt_id`.** The commitment binds
  the PoW. → spec 06 §6.2.
- **D4 — The full node runs no model.** It validates and adjudicates without an LLM. → spec 01, 09.
- **D5 — Candidate-scoped PALW state, and an authenticated commitment for pruning.** → spec 02
  PALW-ST-4, PALW-ST-18.
- **D6 — Admission is split** into stateless and stateful checks, with a per-bond exposure reserve.
  → spec 02 §2.3, 07 §7.2, 10.
- **D7 — The panel, data availability and no-show,** wired. → spec 08.
- **D8 — The BASE-0 court is complete and proof-carrying.** → spec 09.
- **D9 — One fork-choice authority:** a single pure comparator that every chain-selection site calls.
  → spec 13.
- **D10 — Reward is not spendable before `Final`.** It is escrowed, and PALW reward is a carve of the
  fixed subsidy, never an addition to it. → spec 07 §7.6, 10.
- **D11 — The ruleset fingerprint is committed to genesis** (RC == mainnet, by hash). → spec 16 §16.7.

## Consequences

- The release gate became the audit's twelve conditions as one checklist, and PR-00…PR-10 were the
  implementation order. Both are in the full text.
- Later decisions refined parts:
  - the escrow and payout of D10 became vesting rows on testnet-12 (ADR-0152);
  - D7's panel became stake-weighted on testnet-12 (ADR-0152 SW);
  - D9's comparator gained the strict economic win (ADR-0154).

## Links

- Spec: [02 State, objects and carriage](../spec/palw/02-state-objects-and-carriage.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [07 Claim lifecycle](../spec/palw/07-claim-lifecycle.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md) · [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/lifecycle.md](../design/palw/lifecycle.md)
- Full text as written: [design/palw/archive/0042-palw-mainnet-candidate-ruleset.md](../design/palw/archive/0042-palw-mainnet-candidate-ruleset.md)
