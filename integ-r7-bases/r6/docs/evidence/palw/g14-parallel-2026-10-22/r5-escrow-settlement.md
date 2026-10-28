# G14 lane r5-escrow-settlement — RFC-0035 A settlement paths, liability release, retirement, mempool chaining, Mining39 leg

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/r5-escrow-settlement`, base `64bc87dcd`.
Generated files were NOT edited; regenerate centrally: `PalwFundedEscrowRowV1` gained `settled_daa: u64` (last field; the row
exists on no network: no installer is reachable in production), `PalwTransitionExtrasV1` gained `funded_escrow_retention_daa`.

## 1. Every settlement path (G14 item 5.1)

`consensus/core/src/palw_funded_escrow_v1.rs`:

| What | Where |
| --- | --- |
| Paths: `PayerReturnAfterHardDeadline` (unchanged rule) and new `Settlement` | `PalwEscrowSpendPathV1`; rule id `scheduled-settlement` in `palw_escrow_settlement_rule_id_v1` |
| Dues: `PalwEscrowDueV1 { kind: WorkPayment \| CourtFee \| Burn, payee, amount }` | same file |
| Schedule `PalwEscrowFeeScheduleV1 { contract_id, dues }` — private fields, **only constructor `#[cfg(test)] test_only_v1`**; production source `palw_escrow_fee_schedule_v1(row)` always `None` | same file |
| `palw_escrow_qualified_spend_v1` rewritten: entry must equal the row's **unreleased** value (not the original amount); once-only = the row's `paid + burned` must end exactly on a due boundary of the schedule, every later due is discharged (paying due → next output, exact payee P2PKH payload + amount, in order; burn → withheld, no output); remaining value → next output to the rooted payer payload: **change back to escrow** while `liability_open` (row continues on `(tx.id(), index)`), **return** once resolved (row settles). Refusals `SettlementRuleMismatch`, `PayeeOutputMismatch`, `NothingToSettle` (a bare change-back), `Conservation` (dues > unreleased). Per-spend conservation `unreleased = paid_now + burned_now + returned_now + continued_now` checked; outputs after the escrow's funded by other inputs; relay template per path | same file |
| Row: `funding_outpoint` is the CURRENT outpoint; `check_v1`: unsettled ⇒ unreleased > 0, returned 0, `settled_daa` 0; settled ⇒ unreleased 0, no liability, `settled_daa ≥ accepted_daa` | same file |
| Transitions (`palw_escrow_row_transition_v1`): install fresh / clear liability (flag only) / continuation (outpoint moves, consumed grows, no return, liability unchanged) / settle (outpoint unchanged) / retire settled (`Some → None`); accounting never decreases | same file |
| Processor: the plan's `burned_now` joins `owed` (withheld from the collectible fee, RFC-0035 §7.1), the Round preflight uses the same (`BondSpendFilter::escrow_burned`), a continuation's change-back joins the provisional lock for the rest of the mergeset; `PalwEscrowSpendLocksV1::refusal_with_bond_burn` refuses unless `F ≥ bond burn + escrow burn` (checked) in block filter and mempool | `consensus/src/pipeline/virtual_processor/utxo_validation.rs` |

In production the schedule never exists, so every payee/court/burn/partial path is refused (`SettlementScheduleMissing`) exactly
as before; only the payer return after the hard deadline qualifies.

## 2. Liability clearing + collateral release (item 5.2)

- Pure predicate `palw_escrow_liability_resolved_v1(row, view, now)` over `PalwEscrowClaimLiabilityViewV1 { open_courts,
  claim_live, claim_voided, post_final_liability }`: never with an open court; resolved past the hard deadline, on a court verdict
  (`Voided`, no post-Final liability record), or on discharge (no claim record, no liability record — RFC-0034 S1 / retirement).
- `TransitionBuilder::{clear_funded_escrow_liability_v1, sweep_funded_escrows_v1}` (`palw_state_v2/funded_escrow.rs`): view built
  from rooted state only (`open_courts_of`, `claims`, `panel_liabilities`); clearing writes the flag and, if the claim's ADR-0176
  reservation row is still open, closes it via the existing `bond_budget_close_v1` in the same delta. Called once per block in
  `prepare_block_objects_v1` right after the settlements (shared by the chain fold and the pre-object base).
- **Assumption to verify at merge with r5-original-admission:** the bond reservation is keyed by `row.claim` (the claim id passed to
  `reserve_for_claim`). If that lane reserves under another id, change the one lookup in `clear_funded_escrow_liability_v1`.
- Early return before the hard deadline after a verdict still needs the schedule (unpaid dues may exist); typed-missing ⇒ the
  payer's return waits for the hard deadline in production.

## 3. Pruning (item 5.3)

Settled rows retire when `now > settled_daa + funded_escrow_retention_daa` (`palw_escrow_row_retirable_v1`, checked). The
retention is `2 × pruning_depth` (new processor field `palw_escrow_pruning_depth`, set in the one extras builder in
`palw_accept.rs`, `None` below `palw_tir_public_conviction_v1`). Design decision: the fold cannot read the actual pruning point, and a
per-block pruning-point value would have to be threaded identically into template, validation and rehearsal; a network constant is
deterministic everywhere. ×2 because DAA advances at least as fast as blue score. Retirement uses the row's own `settled_daa`
(new field). Carriage: the rows go through the existing tail `0xFB`; a pruned node imports them and retires them at the same DAA.
Risk: after retirement a contract id could be re-installed; claim/FP uniqueness at admission blocks the same original.

## 4. Mempool chained spend (item 5.4)

`mining/src/mempool/populate_entries_and_try_validate.rs::{chained_escrow_spend_v1, Mempool::refuse_chained_escrow_spend_v1}`, called
in `pre_validate_and_populate_transaction`: a tx spending a pending (in-pool) paired TFO1 carrier's own funding output
(`palw_tfo1_provisional_lock_v1(parent)`) is refused `RejectNonStandard`. Node policy, fence-agnostic (the mempool has no fence
view); below the fence it only delays a chained payer spend until its parent is accepted. After acceptance the consensus gate
(rooted rows) decides.

## 5. Undo / reorg (item 5.5)

All row edits ride `PalwDeltaEntryV2::FundedEscrow { old, new }` (revert swaps). Test walks install → continuation → liability
clear → settle → retire and reverts each, checking rows, lock map and root, and the carriage of each intermediate state.

## 6. Mining39 receipts (item 5.6, user decision)

escrow-rfc08's `NativeClaim` leg made the receipt quantum subject to the combined predicate; with `liability_until` (cert +
receipt lifetime + prosecution) > X3 `a + 10,000` it refused every quantum (`HorizonPastHardDeadline`), and inside `H` the at-once
carve also met the export cap — i.e. receipts became unspendable past `palw_settlement_once_v1`. Fixed:
`palw_native_mining_fold_v39::native_mining_receipt_leg_v39()` = `NoLiableClaim(..)` (issuance + weight, no requester fee). Once-only
settlement, the strict 0.51-export budget draw, Gate C and the open-accusation refusal are unchanged; post-Final conviction still
slashes and revokes receipt/weight/certification through G-N2; `collect` still waits for `H`. `NativeClaim` kept
(`#[allow(dead_code)]`) for a requester-fee native leg.

## 7. TFO1 rent (item 5.7) — verified, no double charge

`palw_record_accepted_carrier_fees_v1` stores the raw fee once and `rent_available = F − bond burn` (bond burn senior);
`palw_collectible_transaction_fee_v1` = `F − (bond burn + lifecycle rent + declared TIR rent)` — a TFO1 carrier is on the FP
subnetwork, so lifecycle rent is 0 and its literal TFP1's declared rent is subtracted exactly once (same as a TFP1 carrier). The
escrow funding output is an output, never fee. The native check (`prepare_paired_original_v1`) only compares the declaration with
the plan rent and reads `rent_available`; it subtracts nothing. Matches RFC-0035 §7.1/§9. With this lane, a settlement's escrow
burn joins the same allocation (`owed`) and `refusal_with_bond_burn` checks `F ≥ bond burn + escrow burn`. No code change for the
rent itself.

## Tests (written, not run)

- `kaspa-consensus-core` lib, `palw_funded_escrow_v1::tests::`
  `funded_escrow_settlement_pays_burns_continues_and_returns_once`,
  `funded_escrow_settlement_refuses_wrong_payees_prefixes_and_overdraws`,
  `funded_escrow_liability_resolves_on_verdict_discharge_or_hard_deadline_only`,
  `funded_escrow_rows_retire_only_settled_and_past_retention`; updated `funded_escrow_conservation_violations_are_refused`,
  `funded_escrow_transitions_are_install_or_settle_only`.
- `kaspa-consensus-core` lib, `palw_state_v2::funded_escrow::tests::`
  `funded_escrow_every_path_reverts_to_its_parent_row`, `funded_escrow_sweep_holds_live_and_pursued_claims`; updated
  `funded_escrow_rows_round_trip_the_carriage`.
- `kaspa-consensus-core` lib, `palw_state_v2::palw_native_mining_fold_v39::r5_mining_receipt_leg_tests::`
  `mining39_receipt_leg_is_not_held_to_the_liability_horizon`.
- `kaspa-mining` lib, `mempool::populate_entries_and_try_validate::chained_escrow_tests::`
  `mempool_refuses_only_a_chained_spend_of_a_pending_escrow_output`.

Run: `cargo test -p kaspa-consensus-core funded_escrow`, `cargo test -p kaspa-consensus-core mining39_receipt_leg`,
`cargo test -p kaspa-mining chained_escrow`, plus the existing `kaspa-consensus` utxo_validation escrow tests.

## Fences

No new fence. `palw_tir_public_conviction_v1` (locks, provisional, retention), `palw_claim_hard_deadline_v1` (X3),
`palw_settlement_once_v1` (Mining39 leg is read only past it). Arm centrally as before.

## Open / uncertain

1. Settlement paths are mechanism only: the production schedule (amounts, payees, derivation of dues from rooted fee receipts /
   court results) is a user decision; `palw_escrow_fee_schedule_v1` returns `None`.
2. `processor/evm.rs` template Round preflight does not add escrow burns (unreachable while no schedule exists); wire when adopted.
3. The `debug_assert_eq!(income.actual_sompi, block_fee)` relies on preflight (`escrow_burned`) and merge (`plan.burned_now`)
   computing the same plan — they call the same function on the same row/entries.
4. Generated `native_struct!` lists must pick up `settled_daa` and `funded_escrow_retention_daa`.
5. Bond-reservation key assumption (§2).
6. Mempool chained refusal is policy; an orphaned child whose parent later enters the pool is re-checked only if it goes through
   `pre_validate_and_populate_transaction` again (verify central).
