# G14 parallel lane r5-original-admission (2026-10-22)

Branch `g14p/r5-original-admission`, base `64bc87dcd`. **Status: written, not compiled, not run** (lane rule: no cargo). No fence armed.
Scope: G14 item 1 (handover §5 item 1) — at funded original admission derive and atomically reserve all legal branches,
responsible actors' authority, deadlines, resources, fee and required capital.

## What was implemented

| Item | Where |
| --- | --- |
| Pure all-branch derivation: `FundedBranchV1 {SignerTurn, SourceNode, Retention}`, `FundedResourceDemandV1`, `derive_funded_admission_v1`, `funded_admission_book_candidate_v1` (returns `(after_signer_step, full)`), `funded_required_amount_v1`, `funded_liability_resolved_v1`, `FundedAdmissionScheduleV1` (private fields, `#[cfg(test)]`-only constructor) | `consensus/core/src/palw_funded_original_admission_v1.rs` (new, `pub mod` in lib.rs) |
| Schedule: `PalwFundedOriginalScheduleV1.admission` (`#[cfg(test)] with_admission_v1`, `admission_v1()`, `fee_sompi()`, `capital_floor_sompi()`); typed-missing `PalwFundedOriginalMissingV1::AdmissionSchedule` | `palw_funded_original_coordinator_v1.rs` |
| Coordinator: after the source graph — admission values (typed-missing), **actor authority** (every liable signer and drawn Panel member: bond exists, `Active`, operator = own key; producer pledge signature already re-verified by the catalog), **derivation** of every branch, **dry-run reservation** of all branches in a clone of the rooted book, **capital** = `fee + max(floor, Σ branch capital) − producer deposit` (checked) as `required_amount_sompi`, row installs with **`liability_open = true`** | `palw_funded_original_coordinator_fold_v1.rs::coordinate_funded_original_v1`, `admission_refusal`, `funded_book_candidate_v1` |
| Install (same paired checkpoint as FP/frame/escrow): re-derive the book candidate against the CURRENT book, `install_native_v1`, require the book unchanged or equal to exactly court-f02's ordinary signer binding (prefix), write the full book row via `write_staged_row_v1(Key::CourtService, ..)` (re-runs `check_retains_unresolved` + `check_context`, journalled), then `install_funded_escrow_v1`. Any error → checkpoint restore of everything | `install_original_admission_v1` (Funded arm) |
| Release at resolution: `close_resolved_funded_liabilities_v1(daa)` — open → closed (the one unsettled-row transition `palw_escrow_row_transition_v1` already allows), journalled | `palw_state_v2/funded_escrow.rs`; called in `palw_state_v2.rs` after `observe_court_service_block_v1`, only if `extras.tir_public_conviction_active` |
| Supply observation also runs when funded rows exist (else funded packets stay `Pending` forever) | `palw_court_service_lifecycle_fold_v1.rs::observe_court_service_block_v1` gate |

### Branch derivation (all from native preparation)
1. liable signer × turn — exactly `court_service_packets_v1` (court-f02 ids), so an ordinary F02 binding is a prefix;
2. every node of the complete native `NativeSourceMaterialGraphV3` (all 7 query families × role slots = the hostile source
   branches), packet id = `absolute_packet_id_v3`, causal predecessors kept (`bind_projection_v3` + `reserve_v3`);
3. one retention/heap holding packet (`funded_retention_packet_id_v1`), bytes = Σ heap + Σ retention.
Windows `[a+1, a+10,000)`; all reserved in one candidate (clone) → all-or-nothing.

### Resources
CPU work / IO / transfer bytes = Σ reserved packet costs (deterministic, from the schedule); heap and retention = per-branch
schedule value × service branches, held in the SAME rooted court service book through the retention packet (no new table,
no borsh/wire change). Capacity "release": every slot lies before hard, so reserved per-DAA capacity is released by the
accepting clock; the binding stays as supply history (the book has no packet-removal rule: removing a supplied packet would
break `ServiceReservations::check`'s supply/residual invariant).

### Liability predicate (exact) — `funded_liability_resolved_v1`
`liability_open` clears at `daa` iff ALL:
1. `daa >= row.hard_deadline_daa`;
2. frame present ⇒ `daa >= frame.plan.terms.material_until_daa` (retention discharged);
3. every `CourtDutyClockV1` of the claim: `daa >= response_deadline_daa` and `daa >= material_until_daa`;
4. every packet of the claim's binding is `OpportunityProvided` or `MissingSupply` (ADR-0192 A: no-slash after hard); any
   `Pending`/`UnknownObligation` keeps it open.

### Capital (RFC-0035 §7.1)
`required = fee + max(capital_floor, Σ_turns cap_turn + Σ_nodes cap_node) − min(deposit, ·)`; fee counted once, from the escrow
only (escrow output is output-side of the carrier, so never also inside its fee `F`). "Bond reservation" = the frame's full
deposit, which `install_original_v1` already requires inside the producer's native work room. Payer must have signed
`funding_amount ≥ required` (existing `prepare_funding_v1`).

## Tests (written, not run)
- `kaspa-consensus-core` lib: `palw_funded_original_admission_v1::tests::r5_liability_predicate_needs_every_term`,
  `palw_funded_original_admission_v1::tests::r5_required_amount_counts_fee_once_and_credits_held_deposit`.
- `kaspa-consensus-core` lib: `palw_state_v2::funded_escrow::tests::r5_funded_liability_closes_only_at_resolution_and_reverts`.
- `kaspa-consensus-core` lib, inside `palw_tir_public_fold_v1::conviction_e2e_tests::g14_native_funded_candidate_fixture_generator_v1`
  (include `palw_funded_original_coordinator_native_test_block.rs`, extended): liability opens; branch count = signers×turns +
  graph nodes + 1; full candidate binds exactly the derived packet set; dry run leaves the book untouched; refusals: no admission
  values (`AdmissionSchedule`), court capacity too small, book under another profile, retiring actor, capital not covered,
  short funding vs the derived required amount.
  Run: `cargo test -p kaspa-consensus-core --lib r5_` and `--lib g14_native_funded_candidate_fixture_generator_v1`.

## Fences
None new. Reuses `palw_tir_public_conviction_v1` (coordinator, liability close, observation) and `palw_claim_hard_deadline_v1` (X3).
Production still refuses every funded original (`Schedule`, now also `AdmissionSchedule`).

## Open / uncertain (honest)
1. **Full install not exercised**: the funded `install_original_admission_v1` path (frame install + book write + escrow) runs only
   in the central E2E. The book write's `check_context` needs the frame's signed custody horizon ≥ hard + reorg retention
   (court-f02 risk 4) — may refuse short-custody frames.
2. **Critical (inherited from court-f02, now reachable for funded claims in tests):** `CourtServiceStateV2::check_context` refuses
   any CourtService write while a bound claim is `Final`, and `observe_court_service_block_v1` writes every block. A bound claim
   that finalizes before hard would make later blocks' observation fail. Also the writer drops ordinary `deadlines` of bound claims.
   Needs a decision: block Final while bound, or allow a resolved-binding release rule (book supply invariant must be redesigned).
3. `repair_missing_supply` (court-f02 recovery) moves missed packets non-causally; source packets were reserved causally — a
   repaired dependent may land before its predecessor. Not addressed.
4. `court_capacity_v1` queue rows are not reserved (no adopted mapping from branches to queue rows); only the court service book.
5. Panel members' "signed consent" = their active bond registration; no per-claim panel signature exists. Role contracts beyond
   that stay the schedule's adopted-role bits (unchanged).
6. `derive_funded_admission_v1` requires `graph.source_v3() == plan.source_id` (believed equal; verify centrally).
7. Retention/heap is one packet in one DAA slot of the book (capacity check), not a standing occupancy across the horizon.
8. **User decisions:** service profile, per-branch packet cost, capital per turn / per source node, heap and retention bytes per
   branch — all typed-missing (`FundedAdmissionScheduleV1` is test-only).
