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

## Round 2 (2026-10-22) — funded-original coordinator, mempool/rehearsal wiring, relay template

Status: **written, not compiled, not run** (no cargo). Builds on a23f865b5. Generated files still need central regeneration:
`PalwTransitionExtrasV1` gained `tir_funded_originals` and a `#[cfg(test)]` `funded_original_test_schedule_v1` (the
`native_struct!` list in `palw_native_resource_v1/generated.rs` must be regenerated).

### 1. Funded-original coordinator (G14 item 1)

| What | Where |
| --- | --- |
| Public inputs: `PalwFundedOriginalPairingV1` (private fields; only `palw_funded_original_pairing_v1(&Transaction)`: TFO1 framing, decode, literal TFP1, claim agreement, existing `inspect_carrier_v1` own-output match; keeps the actual tx), `PalwFundedOriginalMissingV1` (typed reasons), `PalwFundedOriginalScheduleV1` (fee/capital/retention/settlement profile ids + amounts, requirement budgets, adopted role contracts; private fields, **only constructor is `#[cfg(test)] test_only_v1`**), `palw_funded_original_schedule_v1(extras)` (production: always `Err(Schedule)`) | `consensus/core/src/palw_funded_original_coordinator_v1.rs` (new, `pub mod` in lib.rs) |
| Coordinator, included in the native fold: `admit_original_v1` (seam dispatcher: funded pairing ⇒ coordinator; plain TFP1 unchanged; funded pairing without preparation ⇒ refused), `coordinate_funded_original_v1` (reads only), `install_original_admission_v1`, `OriginalAdmissionV1 {Plain, Funded}`, `FundedOriginalRefusalV1 {Missing, Invalid, State, Resource}` | `consensus/core/src/palw_funded_original_coordinator_fold_v1.rs` (included by `palw_tir_public_fold_v1.rs` after the geometry include) |
| Seam: `objects.rs` FreePromptCommitted arm — `admit_original_v1` right after `prepare_paired_original_v1`, before `reserve_for_claim`; install via `install_original_admission_v1` at the existing original install point. Both inside `apply_object_with_pair_v1`'s paired checkpoint ⇒ any error restores FP/frame/preparation/escrow row together | `consensus/core/src/palw_state_v2/objects.rs` |

Coordinator order (all native): pairing ≡ literal TFP1 ≡ prepared original → schedule (typed-missing) → native geometry
(`enrich_native_geometry_v1`) → requirements (`prepare_requirements_v1`, budgets from schedule) → actor catalog (re-derives producer
current authority + pledge signature) and **each `missing_contracts_v1()` role must be adopted by the schedule, else that typed
reason** → complete source graph (`source_graph_v1`, needs the court service book: else `SharedServiceNamespace`) → X3 hard deadline
at the actual accepting DAA → native `OriginalFundingScopeV1` (claim/job/execution/source from the frame, `relative_graph_id =
graph.scope_v3()`, profile ids and `required_amount = fee + capital` from the schedule, `material_until = max(plan, hard)`) →
existing `inspect_carrier_v1` on the actual carrier + `prepare_funding_v1` (payer signature, interval, network, coverage) → fresh row
(contract id = terms id, actual outpoint/amount, payer key/payload, hard deadline) + uniqueness. Install: frame via the existing
`install_native_v1`, then `install_funded_escrow_v1`.

Design decisions (within RFC-0035 §3/§4/§7/§9.1, ADR-0202):
- "Capital reservation" = the escrow row locking `fee + capital` (payer-signed amount ≥ schedule requirement). Court-service book
  reservation of the source graph's packets (`SourceServiceProjectionV3::reserve_v3`) is **not** done: it needs packet costs that are
  part of the unadopted schedule. Rooted graph = graph identity bound into the contract id (re-derivable from the rooted frame, §3
  last bullet); no separate graph store was added.
- Row installs with `liability_open = false`: no rule clears an open flag yet, and setting it would freeze the payer's return
  forever. Liability is bounded by the hard deadline; payee paths stay `SettlementScheduleMissing`.
- TFO1 carriers now burn their literal TFP1's declared rent past the fence (`palw_tir_public_declared_rent_v1`), exactly as a TFP1
  carrier does, so the native rent check reads a burn that happened. Behaviour change only past `palw_tir_public_conviction_v1`.

Processor: `palw_v2_objects_of_block` keeps a TFO1 carrier past the fence only if it pairs (else refused, below fence refused as
before); its literal TFP1 view joins `tir_public_carriers` and the pairing rides `PalwCarriedObjectV1::funded_original`; the
rehearsal builds `tir_funded_originals` per object into all three rehearsal extras; accepted pairings return as the 5th tuple element
of `palw_v2_accepted_objects_and_originals` and reach the final fold's `extras.tir_funded_originals`. In production every funded
original is refused by the coordinator (typed `Schedule`), dropped as an object, carrier ordinary, payer fallback spendable next block.

### 2. Mempool + rehearsal wiring

- `PalwEscrowSpendLocksV1::refusal(tx, amounts, daa)` is now the single rule; `BondSpendFilter::escrow_refusal` calls it.
- Mempool: `PalwMempoolBondGate.escrow` (tip rooted rows via `palw_escrow_locked_rows_v1`, no provisional set — a mempool has no
  mergeset) is passed to `validate_mempool_transaction_in_utxo_context(.., palw_escrow)` and refused with
  `SpendsLockedPalwEscrow` before the script check (`processor.rs`, `processor/palw.rs`, `utxo_validation.rs`).
- Rehearsal: `palw_v2_accepted_objects_and_originals/_inner/_and_refunds` take `escrow_settlements`; the chain walk passes
  `ctx.palw_escrow_settlements`, and the rehearsal's pre-object base sets `fee_extras.funded_escrow_settlements` (same rows as the final fold).
- Not done: a mempool chained spend of a not-yet-accepted TFO1 output is not refused at the mempool (refused at the merge by the
  provisional lock). Template construction uses `calculate_utxo_state` (same filter) — unchanged.

### 3. Third-party relay (RFC-0035 §7.4) — narrowest form, no new script/opcode

- `palw_escrow_settlement_rule_id_v1(path)`, `palw_escrow_relay_template_payload_v1(contract_id, outpoint, path)` (`TER1 ‖ contract ‖
  txid ‖ index LE ‖ rule id`), refusal `RelayTemplateMismatch` (`palw_funded_escrow_v1.rs`).
- `palw_escrow_qualified_spend_v1`: payload must be empty (payer's own spend) or exactly this row's template with the escrow at input 0.
- Mechanism: the payer signs once, at funding time, a `SIGHASH_SINGLE|SIGHASH_ANYONECANPAY` ML-DSA signature for the escrow input of
  a template tx (input 0 = escrow, output 0 = exact return, template payload). The ML-DSA sighash commits the payload hash, the
  outpoint/amount/spk and output 0 only, so a relay adds its own fee input + change and the ordinary P2PKH check verifies the same
  signature. No signature-skip, no sig-cache write.
- **Precise limitation:** the pre-signature cannot live inside the TFO1 consent: the outpoint it must commit contains the carrier
  txid, which depends on the TFO1 payload (self-reference, RFC-0035 §7.1/§8). The payer distributes it off-chain after building the
  carrier (still a funding-time signature, not a future one). Only the payer-return rule exists; payee settlement relay needs the
  unadopted schedule. Whether native-subnetwork txs with a non-empty payload are standard/relayed must be checked centrally.

### Fences
Same as round 1: `palw_tir_public_conviction_v1` (all of the above), `palw_claim_hard_deadline_v1` (X3; coordinator refuses
`HardDeadline` without it). No new fence. Even armed, production refuses every funded original (`Schedule` typed-missing).

### Tests added (written, not run)
- `kaspa-consensus-core` lib: inside `palw_tir_public_fold_v1::conviction_e2e_tests::g14_native_funded_candidate_fixture_generator_v1`
  (new include `palw_funded_original_coordinator_native_test_block.rs`): production `Schedule` refusal with unchanged root/no row;
  funded pairing without preparation refused; un-adopted role → `CanonicalWholeBranchActorAssignment`; no service book →
  `SharedServiceNamespace`; success with injected schedule (row binds actual outpoint/amount/payer/hard/contract id); short funding
  and unsigned profile refused; operator rotation refused; checkpoint restore removes the installed row.
- `kaspa-consensus-core` lib, `palw_funded_escrow_v1::tests::funded_escrow_relay_template_binds_contract_outpoint_and_rule`,
  `funded_escrow_relay_presignature_survives_relay_inputs_and_change`.
- `kaspa-consensus` lib, utxo_validation bond-filter tests module: `funded_escrow_mempool_rule_is_the_block_filter_rule`.

### Open / uncertain
- Full `apply_object` success of a funded original (budget, Panel, deposit room) is not exercised end-to-end; only coordinate +
  install-row atomicity. Needs the central E2E with an injected schedule.
- The fixture test's success path depends on `source_graph_v1` succeeding with an empty `ServiceReservations` book (cost 1/1/1/1)
  and `unapproved_probe_at(0)` X3 policy; if `material_until` from the plan is below `a + DISPUTE_WINDOW_DAA`, `max(plan, hard)` covers it.
- User decisions needed (unchanged): fee/capital/retention/settlement amounts & profiles, requirement budgets, role contracts
  (whole-branch assignment, service operator consent, fee/capital bodies, semantic provider), packet-cost service reservation, and a
  liability-clearing rule.
