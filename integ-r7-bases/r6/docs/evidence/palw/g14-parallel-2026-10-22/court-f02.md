# G14 parallel lane court-f02 (2026-10-22)

Branch `g14p/court-f02`, base `494ef1e5e`. **Status: written, not compiled, not run** (lane rule: no cargo). No fence is armed.

## What was implemented

| Item | Where | What |
|---|---|---|
| RFC-0026 F02 attribution (ADR-0192 A) | `consensus/core/src/palw_court_service_binding_v1.rs::attribute_service_default_v1`; frame-scoped wrapper `palw_court_service_state_v2.rs::CourtServiceStateV2::default_attribution_v1` | Decides from the RECORDED reserved supply: all of the claim's packets `OpportunityProvided` + explicit inclusion premise + each unpaid actor has a native `CourtDutyClockV1` whose eligible window contains a supplied unit -> `PartyDefault` (the existing default path charges). Any `MissingSupply`, or an actor with no supplied unit in its window -> `NoSlashHold` (default refused, claim/collateral/evidence stay held, recovery sweep moves the slot). Pending/unknown/inconsistent/premise-off -> `Err` (undecided, never default). `default_hold` is unchanged and still used below the fence. |
| F02 call site | `palw_tir_public_fold_v1.rs` default branch (former `service.default_hold(&frame)`, ~line 1094) | Past `court_service_f02_active_v1` uses the attribution; otherwise the legacy guard. |
| Actual supply recording | `palw_court_service_lifecycle_fold_v1.rs::observe_court_service_block_v1`, called in `palw_state_v2.rs` after the block's object loop | `ServiceReservations::observe_block` with the block's deterministic court-capacity spend as unreserved cost; `ReservedCapacitySpent` records no supply (block stands; later `MissingSupply` -> no slash); then `recover_court_service_supply_v1`. |
| RFC-0028 binding at admission | pure `palw_court_service_binding_v1.rs::{court_service_packets_v1, court_service_binding_candidate_v1}`; fold `palw_court_service_lifecycle_fold_v1.rs::install_court_service_binding_v1` (+ `_with_profile_v1`) | Canonical packets per liable signer x turn, reserved all-or-nothing in the rooted book; written through `write_staged_row_v1` (re-checks `check_retains_unresolved`/`check_context`, journals -> reorg undo). Called at the end of `install_original_v1` (ordinary admission; also reached by `install_prepared_original_v1`). The funded seam in `objects.rs` / funded coordinator was NOT edited; the funded lane can call `install_court_service_binding_v1` or the pure candidate. |
| RFC-0029 lifecycle dispatch + bottom effect | `palw_court_service_lifecycle_fold_v1.rs::{apply_shared_court_op_v1, SharedCourtOpV1, apply_shared_bottom_conviction_v1}` | Single dormant-gated dispatcher over `request / share_body / share_ids / numerical_transport`; a NEWLY stored bottom with `NativeFault` applies, atomically, the same funnel as `apply_conviction_v1`: filer rule, once-check, exit holds, credited seats' S4 locks charged (`charge_covering_signers_v1`), producer via `convict_by_court_verdict_as_v1(.., CourtHeldVerdict)` (void / Final reversal / Gate C / CourtConviction / reporter reward / reward refusal). No by-root forfeit (a bottom fault may concern a numerical statement). `fold::bottom` itself is unchanged (its existing test asserts no effect). |
| RFC-0031 notice intake | `palw_court_shared_tir_fold_v1.rs::request_inner`; pure `palw_court_service_binding_v1.rs::classify_shared_notice_v1`; accessor `chain_state.rs::court_shared_initial_notice_daa_v1` | Past the shared lifecycle fences a new `CourtSharedQuery` row is the native notice: first one per claim must satisfy `a <= d <= a+3,120`; later ones are follow-ups bound to it (`d >= first`, `d < a+10,000`); original must be accepted at `a` already in the fold (same-DAA causal order = object order). Signature (existing ML-DSA check) and dedupe (`Coalesced`) precede it; the row is journalled (reorg via rooted delta). |
| Completeness gate | `palw_g14_gates_v1.rs::validate_palw_court_policy_completeness_v1` | Splits the refusal: `OneOutsiderCapacityIncomplete` clears iff `palw_court_outsider_lane_v1` is armed at or below the fence; `ServiceOpportunityIncomplete` clears iff `palw_court_service_opportunity_complete_v1()` (= a pinned profile). Both still refuse today (profile is `None`). |
| Accessor | `chain_state.rs::court_duty_clocks_for_claim_v1` | Scan of staged rows (clocks keyed by duty id). |

## Tests added (not run)

- `kaspa-consensus-core` lib (`cargo test -p kaspa-consensus-core --lib court_f02`):
  - `palw_court_service_binding_v1::tests::court_f02_binding_reserves_canonical_packets_all_or_nothing`
  - `palw_court_service_binding_v1::tests::court_f02_binding_refuses_typed_missing_or_overlong_profile`
  - `palw_court_service_binding_v1::tests::court_f02_supplied_opportunity_inside_actor_window_defaults`
  - `palw_court_service_binding_v1::tests::court_f02_missing_supply_is_no_slash_hold`
  - `palw_court_service_binding_v1::tests::court_f02_actor_without_supplied_unit_in_window_is_no_slash_hold`
  - `palw_court_service_binding_v1::tests::court_f02_pending_or_unbound_is_undecided`
  - `palw_court_service_binding_v1::tests::court_f02_notice_intake_boundaries_and_followups`
  - `palw_state_v2::tests::court_f02_lifecycle_is_dormant_below_its_fences`
- `kaspa-consensus-core` integration test target `palw_court_policy_v1`: `court_f02_completeness_gate_clears_each_half_on_its_own_evidence`

## Fences

Reused only (no new fence): `palw_tir_public_conviction_v1` (via `extras.tir_public_conviction_active`) for F02/F02-binding/supply;
`palw_tir_public_conviction_v1` + `palw_court_outsider_lane_v1` (state-params mirror) for the shared lifecycle and notice intake.
Plus the typed-missing const `PALW_COURT_SERVICE_PROFILE_V1` (must be pinned before F02 does anything). To arm centrally: both
fences, outsider lane at or below the conviction fence, and a reviewed profile.

## Open / uncertain (honest)

1. **User decision needed:** `CourtServiceProfileV1` values (per-DAA reserved budget, packet cost, packets per liable signer, span).
   Left `None`; F02 is inert and the gate keeps refusing `ServiceOpportunityIncomplete`.
2. The packet set is per liable signer x turn — a candidate mapping, NOT the complete all-hostile-branch derivation RFC-0026/ADR-0192
   ask of the coordinator. The F02 "inclusion premise" is represented only by the fence + pinned profile (first block per DAA with
   unspent reserved quota = supply); the premise text itself still needs adoption with the profile.
3. Unreserved cost maps `PalwCourtCostV1{work,bytes}` into `ServiceCost{work,bytes}`; `io`/`controls` are 0 (no deterministic source).
4. Installing a binding runs `check_context`, which refuses if the frame's signed custody horizon < hard + reorg retention: with a
   profile pinned this could refuse ordinary admission for short-custody frames (intended by RFC-0028, but verify). The writer also
   drops ordinary `deadlines` for bound claims (existing candidate behaviour).
5. RFC-0029: **no wire object carries `SharedCourtOpV1`** — allocating a consensus object tag is a central step. The dispatcher is the
   entry that object must call; until then shared ops remain reachable from tests only. Court end-archive evidence for a shared-bottom
   conviction is recorded as `Uncaptured` (adding an evidence variant changes a borsh layout; not done).
6. RFC-0031: "initial notice" = the claim's earliest `CourtSharedQuery` row (O(rows) scan, no per-claim index). No separate notice row
   type was added (would extend the staged-court key/row enums and every exhaustive match).
7. F02 attribution reads all clocks of the claim; correctness of `CourtDutyClockV1` installation is the existing candidate's.
