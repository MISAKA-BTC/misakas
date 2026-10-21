# G14 parallel lane escrow-rfc08 — RFC-0035 A rooted escrow + RFC-0008 native liability deadline

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/escrow-rfc08`. Generated files
(`palw_native_resource_v1/generated.rs`, `wire_generated.rs`, `private/*.rs`, manifests) were NOT edited and must be regenerated
centrally: `PalwChainStateV2` gained a field, `PalwStateCarriageV2` gained a field, `PalwTransitionExtrasV1` gained a field and
`PalwDeltaEntryV2` gained a variant.

## Task A — RFC-0035 A / ADR-0202 (G14 §6)

Scope: the user-approved isolated implementation of A. **No amount, fee, draw limit or payee share is adopted**; the mechanism
refuses every path that needs one.

### Implemented

| What | Where |
| --- | --- |
| Row `PalwFundedEscrowRowV1` (contract id, claim, actual funding outpoint, amount, payer key id, native return payload, accepting DAA, derived hard deadline, `paid/burned/returned`, `liability_open`, once-only `settled_by`) | `consensus/core/src/palw_funded_escrow_v1.rs` |
| Conservation `amount = paid + burned + returned + unreleased`, checked (`unreleased_v1`, `check_v1`); settled ⇒ unreleased 0 and no liability; unsettled ⇒ nothing moved | same, `PalwFundedEscrowRowV1::{unreleased_v1, check_v1}` |
| Only two transitions: install fresh / settle once (`palw_escrow_row_transition_v1`); removal, resettle, immutable edits refused | same |
| Hard deadline derived from adopted X3 mirror at actual accepting DAA (`a + prepare + observe + resolve` = RFC-0014 `a+10,000`), checked, unsourced if X3 does not govern `a` | `palw_funded_escrow_hard_deadline_v1` |
| Typed-missing fee schedule: `PalwEscrowFeeScheduleV1 { unadopted: Infallible }` — no value can exist | same |
| Qualified spend `palw_escrow_qualified_spend_v1`: exactly one input on the row outpoint, entry amount = rooted amount; before hard deadline or with liability open ⇒ `SettlementScheduleMissing`; payer return ⇒ output 0 = exact unreleased to rooted P2PKH payload, non-escrow inputs fund all other outputs (escrow funds no fee/change) | same |
| Same-mergeset provisional lock `palw_tfo1_provisional_lock_v1`: only the TFO1 carrier's OWN funding output, only when `inspect_carrier_v1` pairs literal payload/index/amount/payer payload | same |
| Rooted state field `funded_escrows` (Some-only root block `funded_escrow/v1`, carriage tail `0xFB`, decoder refuses empty tail / invariant-violating row), delta `FundedEscrow = 235`, `apply_delta_entry` arm (revert swaps old/new ⇒ row and lock restored) | `consensus/core/src/palw_state_v2.rs`, `palw_state_v2/chain_state.rs` (`funded_escrows_v1`, root) |
| One writer `write_funded_escrow_v1`, `install_funded_escrow_v1` (no caller), `settle_funded_escrow_v1` (fact's `old` must equal rooted row) | `consensus/core/src/palw_state_v2/funded_escrow.rs` |
| Extras `funded_escrow_settlements`, facts accessor, applied in `prepare_block_objects_v1` right after Round incomes | `palw_state_v2.rs`, `input_views.rs`, `block_preparation.rs` |
| `TxRuleError::SpendsLockedPalwEscrow(outpoint)` | `consensus/core/src/errors/tx.rs` |
| `BondSpendFilter.escrow: Option<&PalwEscrowSpendLocksV1>` + `escrow_refusal`; checked in `validate_transaction_in_utxo_context` after the burn check, BEFORE and IN ADDITION TO the ordinary script check (no SkipScriptChecks, no sig-cache write) | `consensus/src/pipeline/virtual_processor/utxo_validation.rs` |
| ctx `palw_escrow_locks {rows, provisional}`, `palw_escrow_settlements`; in `calculate_utxo_state_with_funded_candidates_v1` each accepted paired TFO1 carrier's own output joins `provisional` after its merged block (applies to every later merged block of that mergeset only); each accepted rooted-row spend re-derives its plan from the same row + actual entries and records a settlement fact | same |
| Rows filled from selected-parent state (chain walk) and tip state (virtual) via `palw_escrow_locked_rows_v1`; settlement facts copied into the fold's extras; delta goes through the existing `PalwUtxoCommitV2` single WriteBatch | `processor.rs`, `processor/palw.rs` |

### Fence

Reused `palw_tir_public_conviction_v1` (owner of the public TFP1 original route that TFO1 wraps; G14 G-10 gate
`TirPublicConvictionV1`). No new Params field. Below it: rows map empty, no provisional lock, extras empty, filter built exactly as
before. **Needs arming: `palw_tir_public_conviction_v1`** (and, for the hard deadline, `palw_claim_hard_deadline_v1` / X3). Even
armed, nothing installs a row today (see below), so the only live effect is the same-mergeset provisional lock on paired TFO1
carriers' own outputs.

### Tests (written, not run)

- `kaspa-consensus-core` lib, `palw_funded_escrow_v1::tests::`
  `funded_escrow_conservation_violations_are_refused`, `funded_escrow_payer_return_after_hard_deadline_is_the_only_path_now`,
  `funded_escrow_payer_return_must_return_exactly_and_not_fund_others`, `funded_escrow_transitions_are_install_or_settle_only`,
  `funded_escrow_hard_deadline_is_derived_from_x3_or_refused`, `funded_escrow_provisional_lock_names_only_a_paired_tfo1_own_output`.
- `kaspa-consensus-core` lib, `palw_state_v2::funded_escrow::tests::`
  `funded_escrow_settlement_reverts_to_the_locked_row` (install → settle → revert restores row + lock + root; revert install ⇒
  parent root), `funded_escrow_rows_round_trip_the_carriage`.
- `kaspa-consensus` lib, `pipeline::virtual_processor::utxo_validation::…` (the bond-filter tests module):
  `funded_escrow_locks_refuse_unqualified_spends_and_leave_the_payer_fallback` (rooted lock refuses theft in a later block, admits
  qualified payer return; pre-deadline refused; provisional lock refuses in same mergeset; no lock ⇒ payer fallback spendable).

### Not done / honest gaps

1. **No installer is reachable.** `palw_funded_original_candidate_v1` never returns admitted, and `palw_v2_objects_of_block` still
   refuses every TFO1. Wiring `install_funded_escrow_v1` into the paired FP checkpoint (RFC-0035 §9.1: after
   `prepare_paired_original_v1`, before `reserve_for_claim`) belongs to the full-graph coordinator, not this lane.
2. **Payee settlement paths** are refused (`SettlementScheduleMissing`) until a user decision adopts amounts/fees/draw limits.
   Burn path likewise. The struct carries `paid/burned` fields for that future rule.
3. **Third-party relay without payer signature** (RFC-0035 §7.4) is not implemented: it needs a per-input script authorization bound
   to the exact escrow input, which the transaction validator has no seam for. Today the escrow output's own P2PKH signature check
   always runs; only the payer can execute the return. This only matters for payee paths, which are refused anyway.
4. **Mempool** (`palw_mempool_bond_gate`, processor.rs ~5435) does not yet ask `escrow_refusal`; a locked-escrow spend may be
   relayed and then skipped at merge (no consensus split; block construction uses the same `calculate_utxo_state`).
5. **Rehearsal pre-object base** (`palw_accept.rs` `fee_extras`) does not receive the settlement facts; only `funded_escrows`
   differs there and no object reads it.
6. Settled rows are kept (once-only memory) — unbounded growth until a retirement rule is decided.
7. Tests in the existing `palw_state_v2/tests.rs` that enumerate delta variants by name had the new arm added; any golden root /
   manifest test that hashes the full field list will need the central regeneration.
8. Tail byte `0xFB` and delta number `235` chosen as free in this tree; other lanes may collide — check at merge.

## Task B — RFC-0008 (RFC08-2): native claim liability deadline

The admission side already derived native hard deadlines (`admit_native_claim_liability_v1`, signed deadline + `window_court`). The
remaining "no signed hard deadline" refusal was the **native mining receipt leg** in
`consensus/core/src/palw_native_mining_fold_v39.rs::apply_native_receipt_block_v39` (`PalwLegLiabilityV1::Unsourced`).

Implemented in `consensus/core/src/palw_settlement_liability_fold_v1.rs`:
- `PalwLegLiabilityV1::NativeClaim { budget_id, accepted_daa, liability_until, draw, paid_now }` and its arm in
  `leg_liability_terms_v1`: reservation row by `budget_id`, bond terms, R book from the row, no material covenant (as at admission),
  `retire_not_before = liability_until`.
- `native_claim_signed_hard_deadline_v1(params, accepted_daa)`: adopted X3 mirror (`palw_claim_hard_deadline_v1`, RFC-0014
  amendment `a + 10,000`) at the claim's actual accepting DAA, checked add; `Err` (unsourced, by name) where X3 does not govern.
- Mining receipt leg now `NativeClaim { budget_id: native_mining_budget_id_v39(&c.id), accepted_daa: c.admitted_daa,
  liability_until: palw_native_mining_liability_until_v39(c.admitted_daa, &policy, &certification), draw: carve, paid_now: carve }`;
  stays `Unsourced` (named) when RFC-0018's certification policy is absent.

Fences: `palw_settlement_once_v1` (the leg is not read below it) **and** `palw_claim_hard_deadline_v1` (X3) for the hard deadline.
No new fence.

Uncertain / to verify centrally:
- If `liability_until` (cert lifetime + receipt lifetime + prosecution window) exceeds `a + 10,000`, the combined predicate will
  refuse the leg (`liability_until > signed_hard_deadline`). That is a real finding about the mining policy numbers, not masked.
- Admission (`plan_native_mining_budget_v39`) still uses `hard = admitted + window_court`; receipts now use X3's `a + 10,000`.
  Whether admission should switch to the same X3 derivation is a design question left open (not changed to avoid moving an existing
  deadline).
- `PALW_RFC0608_OPEN_BLOCKERS_V1` text still lists native mining among unsourced legs; the other legs (verifier fees outside paid
  V2, material provider fees, kernel outside paid V2, beacon payouts, pre-trap audit answers) remain unsourced, so the blocker stays.
- No new test written for Task B (the existing `palw_state_v2/tests/settlement_liability_v1.rs` suite is the place; a fixture with
  X3 + settlement_once + RFC-0018 + mining39 armed is required).

## Decisions within adopted scope

- Reusing `palw_tir_public_conviction_v1`: lane rules prefer an owning dormant fence; TFO1 is a funded variant of the TFP1 route.
- Hard deadline = X3 policy at the actual accepting DAA: RFC-0031/0032/0026 fix hard at original `a + 10,000`; reading it from the
  armed X3 mirror avoids hard-coding a number.
- Payer return requires output 0 exact and other inputs to fund the rest: RFC-0035 §7.5 (escrow never funds the relay's fee).
- Provisional lock limited to the carrier's own paired output: RFC-0035 §7.3 / §9.1 last paragraph.
