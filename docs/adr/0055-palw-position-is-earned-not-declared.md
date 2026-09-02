# ADR-0055 — Chain position is earned, and the question is set by the block

- Status: Accepted
- Date: 2026-08-27
- Supersedes: nothing. Extends ADR-0044 Decision 6; reverses one relaxation recorded in
  `palw_state_v2.rs` (the court's opening rung).
- Declared by: `PALW_ATTEMPT_V2_VERSION` 4 → 5.

> **Note (index reconciliation, 2026-09-02).** "No shipped preset carries `ConsensusV2`" was
> written on 2026-08-27 and is no longer so: testnet-11 and devnet ship the V2 bundle
> (`palw_rc_shipped_params`, `devnet_shipped_params`), so Decision 3's window is the RC genesis's.
> Decision 1's "a receipt header buys no chain position" is the state [ADR-0073](0073-real-demand-work-bears-the-weight.md)
> Phase ④ proposes to change (open). Map: [`README.md`](README.md).

## Context

The 2026-08-21/08-27 adversarial audit returned eleven criticals. Three of them are the same
mistake told three ways: **a party was allowed to supply the thing it was going to be judged
against.**

1. A receipt-lane (algo-7) header's digest is free to re-roll — nothing in it costs anything to
   produce. `calc_block_level_check_pow_layer0` already refused to sell pruning-proof LEVEL for
   that price. Nothing refused to sell fork-choice WEIGHT: every merged receipt block still added
   `calc_work(bits)` to its descendants' blue work. The lane's meter, the ADR-0044 quantum ticket,
   draws against a beacon derived from the candidate's own chain — so it can only run on a chain
   candidate and cannot gate DAG entry at all. A merged-but-never-candidate receipt block never
   faced it.

2. A capture states its own job id. The seat compared the capture's roots against the claim's
   roots; the challenger re-executed the anchor **named inside the capture** and compared. Both
   agreed for material that answered a question no block ever asked, so one gossiped capture was a
   re-usable asset: mine a fresh block, announce the borrowed roots, and neither half objects. One
   inference, unlimited blocks, by parties that ran nothing.

3. The court's opening rung ran on the whole session budget rather than the rung clock. That was
   right when it was written — `CourtDisclosed` was constructed nowhere, so silence there could not
   fairly convict — but by the time a responder shipped, the backstop's effect had inverted: it
   ended the dispute on the CHALLENGER's side, which makes silence the winning move for a guilty
   producer.

## Decision

**D1. A receipt header buys no chain position of either kind.** One predicate,
`pow_layer0::algo_id_carries_no_chain_position`, answers for both the block level and the blue
work, and both sites call it. A receipt block contributes zero added blue work.

**D2. The anchor is derived from the claim's block, never read off the material.** Its four inputs
— network domain, the accepted block's pre-PoW hash, the class id, the executor bond outpoint — are
all facts a third party reads from the chain, so every verifier derives the value the producer was
forced to use. `PalwClaimRootsV1` carries it; `verify_material` refuses material whose
`job_context.job_id` is not it; the challenger re-executes the derived anchor. A verifier that
cannot derive it (pruned block, family with no canonical job) declines to judge rather than judging
without it.

**D3. The opening rung takes `turn_deadline_daa` like every other rung.** A silent responder loses
the claim, as it does at every later rung. This is a genesis-time obligation on the bundle: the
window must be wider than the responder's own cadence (the panel wakes on a 2-second tick and
answers from a capture it already holds), and a bundle whose window is tighter than its own
software convicts honest producers.

**D4. A production close carries the operand rows the court will read** — recorded by running the
real adjudicator through `PalwRecordingOracleV1` against the full inventory and opening exactly
what it resolved. The enumeration is asked of the adjudicator rather than written a second time on
the prover side, because a second enumeration agrees today and diverges the first time a kernel
changes which operand it touches — in the direction where an honest producer cannot close.

## Consequences

- D1 and D3 change consensus outcomes without changing any params field, so neither would have
  moved `palw_ruleset_id_v2` on its own. An old binary and a new one would have agreed on every
  block and disagreed on whether a producer was slashed and on which chain was heavier — a silent
  fork rather than a refused handshake. `PALW_ATTEMPT_V2_VERSION` 4 → 5 is where that is declared;
  it moves the fingerprint of every network carrying `ConsensusV2` and re-mints them.
- D2 adds `accepted_block` to the seat-duty, disputable-claim and court-duty views, and one header
  read per claim under judgement on the panel's cadence.
- No shipped preset carries `ConsensusV2`; a V2 bundle arrives with a network's genesis, so D3's
  window is chosen there.
