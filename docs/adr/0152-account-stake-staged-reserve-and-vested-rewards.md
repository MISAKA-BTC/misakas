# ADR-0152 — A bond is standing stake, a claim reserves until its licence, and a reward vests until its conviction window closes (R-core+)

> **Body moved (2026-09-27).** This file first reached `main` in its short form. The full v3.1 text
> (469 KB: rules, analysis, tests, reviews) is kept verbatim in
> [design/palw/archive/0152/](../design/palw/archive/0152/README.md), from branch
> `docs/adr-0152-v31-postedits` at `9ed1adced`. The normative rules are in
> [spec/palw](../spec/palw/00-index.md) chapters 07–10, 13 and 16. The reasoning is summarised in
> [design/palw/collateral.md](../design/palw/collateral.md).

* Status: **Accepted for testnet-12** by the operator, 2026-09-24 (v3.1, with its 13 post-edits and
  the integration amendments IA-1…IA-15). **In force on testnet-12 from genesis** through
  `Params::palw_rcore_plus` (DAA 0, genesis only). Not armed on any other network. Amended after launch
  by the SW-8 seed correction of 2026-09-25 and by two flag days, [ADR-0154](0154-testnet-12-flag-day-daa-750.md)
  (DAA 750) and [ADR-0155](0155-testnet-12-flag-day-daa-1300.md) (DAA 1,300).
* Date: 2026-09-24
* Decided by: the operator. The binding decisions (v1, v2, v3, v3.1, the post-edits and the decisions
  after v3.1) are listed in the archive's
  [§1.5](../design/palw/archive/0152/02-motivation-and-overview.md).
* Supersedes / amends: on testnet-12 only, ADR-0061's collateral model (with ADR-0151) and ADR-0124 D5
  with ADR-0130's one-ticket-per-operator panel lottery (by SW). ADR-0062 is amended by DA.

## Context

Before R-core+, a floor claim reserved its whole reward-sized exposure for its whole life. The
reservation protected the interval that needed it least, and a seat's capital sat idle in locks.
Pure no-reserve fails without attribution: nothing then ties a lie to the bond that told it.
testnet-12 is the last testnet before mainnet. The operator decided to start it on a collateral model
where cumulative work is unlimited, only concurrent unresolved risk is reserved, and the most an
absconder can extract never exceeds what the chain can recover, given a conviction inside its window.
The launch gate was F1–F4 GREEN (attribution, the false-`Valid` adjudicator, the DA court, and quorum
counting), plus the fold, reorg and restart tests and a short drill.

## Decision

Each item names the rule family (0152's own labels, kept as sources) and where the Spec states it.

- **D1 — The bond is one account of slashable stake** (B-1…B-5): floors of 13,000 MSK to produce and
  register and 130,000 MSK to sit; a bounded exit; no ejection status and no tombstone; a producer
  floor gate; Eq capped on testnet-12. → spec 10 §10.1.
- **D2 — A claim reserves `w + E` until its licence** and `w` after a licence that every seat served
  with `basis_k ≥ 2`; otherwise to Final, and always for C7 (the 2M row) (SR-1…SR-10). → spec 10 §10.2.
- **D3 — Charging at launch:** a first failed panel redraws uncharged. A second failed panel forfeits
  the commitment (S0′). Capacity voids are never charged (SR-5). → spec 07 §7.7.
- **D4 — Attribution** (F1/F2, J-1…J-8): the claim records its job identity; one adjudicator,
  `palw_check_panel_false_valid_v2`, binds a false `Valid` by root; `ExecutorRefuted` and
  contradictions 9–13; admission pins the canonical job. → spec 09 §9.4.
- **D5 — The panel draw is stake-weighted** (SW-1…SW-10): one key `L/W` per operator, W its one bond's
  posted collateral capped at 1,000,000 MSK; bind only in the anchor block, on one state; no bind below
  875 ‰ of the base weight eligible. → spec 08 §8.2, 07 §7.3.
- **D6 — The DA court is redesigned** (DA-1…DA-9): sessions per `(claim, accuser)`, drawn units inside
  the committed run, pause credit for seat sessions, re-keyed rows and locks. → spec 08 §8.6.
- **D7 — Quorum counting** (Q-1…Q-7): `Sampled` never counts; `basis_k` is recounted over masks; an
  optimistic (S2) licence is a fast path and never the basis of Final. → spec 08 §8.4.
- **D8 — One deadline function** (DL-1). → spec 07 §7.5.
- **D9 — Rewards vest in rows** (V-1…V-8) that mature on the lock's two clocks, and conviction objects
  ride heartbeat blocks (H-1). → spec 10 §10.6, 13 §13.3.
- **D10 — The seat lock** (L-1…L-4b), **one committed ledger** (A-1…A-6), the **action-based slash
  schedule** and the **reporter reward** (S, R-1…R-7), **throughput caps** (T-1…T-4) and
  **withholding** (W). → spec 10.
- **D11 — Fences and schema v22** (§6): `palw_offence_attribution`, `palw_rcore_plus`,
  `palw_rcore_conservative_classes`; 14 prerequisites; state version 22. → spec 16, 02.

## Consequences

- testnet-12's genesis identity moved. Model attempt roots and free-prompt roots moved once (`CoreV1`
  and the unified FP `output_root` rule). No build of this lineage runs testnet-11. The testnet-11
  rollback stays on `1f98d3bf`.
- An honest reward waits about F + 3,000 DAA plus the second clock before it mints.
- Residuals the ADR names: 2M fails R-6's chain-kept invariant and is closed at launch until ADR-0153;
  an `AttnFused` arithmetic lie on a held-context class has no conviction route (post-edit 8); X10
  (attributed-only charging) is not declared; the trickle and halt costs of V-8; the FP lane's
  execution-quantum maturity (F + 1,200) runs ahead of the row's F + 3,000.
- **Amended after launch.**
  - 2026-09-25: SW-8's seed became the anchor attempt's execution commitment, and lane A anchored
    panels on operator attempts ([t12-panel-seed-2026-09-25.md](../t12-panel-seed-2026-09-25.md)).
  - [ADR-0154](0154-testnet-12-flag-day-daa-750.md) (DAA 750): both of the above, plus registry
    resilience, the anchor at the exposure ceiling, a resolved lock carried by the whole collateral,
    and the F + 1,000 lock life.
  - [ADR-0155](0155-testnet-12-flag-day-daa-1300.md) (DAA 1,300): eligibility refusals re-anchor
    instead of voiding, and older locks are re-dated to F + 1,000.

## Links

- Spec: [07 claim lifecycle](../spec/palw/07-claim-lifecycle.md) · [08 verification](../spec/palw/08-verification.md) ·
  [09 court and offences](../spec/palw/09-court-and-offences.md) · [10 collateral and economics](../spec/palw/10-collateral-and-economics.md) ·
  [16 network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [collateral.md](../design/palw/collateral.md), [verification.md](../design/palw/verification.md)
- Full text: [design/palw/archive/0152/](../design/palw/archive/0152/README.md)
- Code: `consensus/core/src/palw_state_v2.rs` (`palw_claim_commitment_v1`, `palw_bond_committed_v1`,
  `palw_rcore_lock_v1`, `palw_rcore_deadline_v1`), `palw_panel_v2.rs` (the stake draw),
  `palw_offence_attribution_v1.rs`, `palw_da_rcore_v1.rs`, `palw_vesting_v1.rs`;
  `config/params.rs` (`palw_rcore_plus`, `validate_palw_rcore_plus_v1`)
