# G14 lane r6-d1-release — D1 resolved-binding release rule (2026-10-22)

Branch `g14p/r6-d1-release`, base `3c6e4c09d`. **Status: written, not compiled, not run** (lane rule: no cargo). Nothing armed.

User decision D1 (2026-10-22, `docs/design/palw/integration/g14-t12-16000-decisions-2026-10-22.md`): a claim bound to the
court service book goes Final on its normal schedule; after Final the per-block supply recording can still write the book; a fully
decided binding is released (capacity freed, binding retired) with a redesigned, rooted, journalled, reorg-safe supply invariant.

## What was implemented

| Item | Where | What |
|---|---|---|
| Stall fix | `palw_court_service_state_v2.rs::CourtServiceStateV2::check_context` | The refusal "unresolved service claim cannot already be Final" is removed (the claim record must still exist). `observe_court_service_block_v1`'s book write no longer fails a block once a bound claim is Final. |
| Final not delayed (D1) | `palw_court_service_state_v2.rs::court_service_holds_retirement_v1`; `palw_state_v2.rs::palw_rcore_deadline_v1`; `transition_builder.rs::{arm_deadline, finalize_claim}`; `deadlines.rs::sweep_deadlines`; `chain_state.rs` (both deadline-consistency rebuilds, `palw_accusation_pending_v1`); `palw_staged_court_fold_v1.rs::write_staged_row_v1` (deadline retain) | The binding now holds only a TERMINAL claim's retirement (DL-1 row, arm site, sweep, both consistency checks agree). `finalize_claim` no longer returns early for a bound claim. A binding is no longer an "accusation pending" (open courts / shared demands still are, via their own rows). The writer drops only terminal bound claims' deadlines (it used to drop every bound claim's, which also stopped Final). `retire_claim`, collateral/bond holds, material holds and default refusal are unchanged while bound. |
| Released row form | new `consensus/core/src/palw_court_service_release_v1.rs`: `CourtServiceRowV3 {version, state, resolved, causal}`, `ResolvedServiceBindingV1`, `ServiceBindingScopeV1`; row variant `PalwStagedCourtRowV1::CourtServiceV3` (appended row tag 36, SAME key `CourtService`) | Legacy `CourtService` (row tag 18, state version 2) keeps its exact encoding and stays readable; the V3 variant is written only past `palw_tir_public_conviction_v1` (writer refuses it below). Embedded state carries `version = 3`. `CourtServiceStateV2::check` is unchanged (v2 only); `check_live_v1` accepts v3 with retired (unowned) packets; `CourtServiceRowV3::check` is the strict check (every packet live-owned or retained by exactly one released record; records consistent; causal edges in order). |
| Supply invariant redesign | `CourtServiceRowV3::{release_v1, compact_v1}` | Release removes the binding, writes a compact record (scope, packets/served/missing counts, release DAA), drops its causal edges, then compacts every DAA slot holding ONLY retired packets: its packets, assignments, residual quota and supply record leave together (so `ServiceReservations::check`'s per-slot `supply.reserved == budget − remaining` is untouched). Retired packets sharing a slot with a live packet stay (`retained_packet_ids`) until that slot is compacted. |
| Writer transition rule | `palw_staged_court_fold_v1.rs::write_staged_row_v1` (CourtService branch) + `CourtServiceRowV3::check_transition_from_v1` | V2→V2: legacy `check_retains_unresolved`. →V3 (from V2 lifted by `from_v2`, or V3): either exactly the canonical release of the bindings that left (recomputed from the old row at its clock), or a retaining update (legacy rules, records unchanged, accepted edges unchanged, edges only for new packets). V3→V2 and removal of a V3 row are refused. Shared-context / duty-clock cross checks use the live state of either variant. Helper `write_court_service_state_v1` writes the live state in the variant the row already has (used by recovery, observation, ordinary binding install). |
| Release step | `palw_court_service_lifecycle_fold_v1.rs::release_resolved_court_service_bindings_v1`, called at the end of `observe_court_service_block_v1` (after recovery) | Only on the first block at a DAA (book clock == block DAA), so inside the existing conviction-fence gate. Eligible (`PalwChainStateV2::court_service_release_eligible_v1`): every packet decided (`OpportunityProvided`/`MissingSupply`) with the book clock ≥ hard, no open court of the claim, every duty clock past its response deadline; AND no dependent row of the claim remains (one scan of staged rows, only when a candidate exists). ≤ 64 releases per block, claim order. One journalled row write; then `rearm_claim_deadline_dl1_v1` per released claim (its retirement). |
| Dependents leave first | `court_service_holds_material_v1` / new `court_service_dependent_claim_v1` | Original material (Claim/Owner/TirClaim/TirAcquisition/TirPreparation/TirWitness/TirRawChunk/TirAcceptedBody) stays held until the release; dependent rows (duty clocks, shared court rows, archives, close receipts, canonical material) are no longer held once the binding is release-eligible, so they leave at their own TTL and the release never strands a row whose context check reads the frame/binding (the staged sweep is 64 rows/block and would otherwise delete the frame before its clocks). |
| (a) causal order on recovery | `palw_court_service_v1.rs::ServiceReservations::repair_missing_ordered_v1` (+ `ServiceRefusal::CausalOrder`); `CourtServiceStateV2::repair_missing_supply_ordered_v1`; `recover_court_service_supply_v1` passes the row's edges | Missed packets are placed in topological order (ready by deadline then id = the legacy order without edges), each ≥ 1 + every predecessor's slot; afterwards every edge touching the claim must run strictly forward, else the whole claim's move is refused (`CausalOrder`; nothing moves; MissingSupply → no-slash hold). `repair_missing` = ordered with no edges (legacy behaviour byte for byte). Edges are recorded at funded admission: `SourceServiceProjectionV3::causal_edges_v3`, `palw_funded_original_admission_v1::funded_admission_causal_edges_v1`, written with the book in its V3 form by the funded install (`palw_funded_original_coordinator_fold_v1.rs::install_original_admission_v1`). |
| (b) court capacity at funded admission | `palw_court_capacity_fold_v1.rs::{check_funded_court_reserve_v1, palw_funded_court_reserve_claims_v1}`; called at the end of the funded install; counted in `check_court_claim_admission_v1` and in the prosecution's `reserve` decision | Past `palw_court_capacity_v1`, a funded original with an open, unsettled liability is a reserve-holding claim: its admission is refused (`CourtClaimAdmissionFull`) when its hard-deadline bucket is full; it is counted by every later bounded admission; its first court row runs on the reserve pool. Dormant below the fence. |
| Load check | `chain_state.rs` (the staged consistency assert) | Runs `CourtServiceRowV3::check` on a loaded V3 row. |

## Design decision (please confirm)

**(b) "queue rows" are realised as the capacity design's per-claim reserve, not as `PalwReservedCourtV1` rows.** A court-capacity row
(`PalwDisputeClaimV1::courts`) is keyed by a challenger and "exists only beside its reserver's live reservation" (sweep, settlement
and escrow all assume it); no challenger exists at admission, so writing one would break those invariants. The capacity design's
per-claim reservation is (i) the bounded admission's bucket slot and (ii) the claim's one reserve row (`court_reserve_used`). The lane
makes a funded original take (i) at admission and be entitled to (ii). If literal queue rows are required, a new row owner (e.g. an
admission-owned reserve row) needs an adopted design. Scope: RFC-0035 A §7 (funded admission reserves all resources), audit G-07 /
g14-court-capacity (bounded admission); no new RFC/ADR.

Other decisions, within D1 / RFC-0026 / RFC-0028 / ADR-0192 A:
- Release waits for every dependent row to be gone (not just "last deadline passed"), because those rows' context checks read the
  frame and the binding; this delays only the release (collateral/material/retirement hold), never Final.
- The binding no longer counts as an accusation (`palw_accusation_pending_v1`): with D1 it must not pause/defer Final. Consequence:
  claim-local reward holds and V3 Final deferral no longer wait on a bare binding (they still wait on open courts / shared demands).
- `ResolvedServiceBindingV1` records are kept forever in the row (small, one per bound claim). Pruning them is a later decision.

## Tests (written, not run)

- `kaspa-consensus-core` lib — `cargo test -p kaspa-consensus-core --lib r6_d1`:
  - `palw_court_service_release_v1::tests::r6_d1_release_needs_every_packet_decided_past_hard`
  - `palw_court_service_release_v1::tests::r6_d1_legacy_state_encoding_is_unchanged_and_v3_is_refused_in_the_legacy_check`
  - `palw_court_service_release_v1::tests::r6_d1_recovery_keeps_causal_order`
  - `palw_court_service_release_v1::tests::r6_d1_causal_edges_are_checked_and_immutable`
  - `palw_tir_public_fold_v1::…::r6_d1_bound_claim_goes_final_supply_continues_and_the_binding_releases` (stall regression: Final
    then supply writes succeed; release after all decided; reorg undo/redo across the release; snapshot carriage; dormant below fence)
  - `palw_state_v2::funded_escrow::tests::r6_d1_funded_admission_reserves_court_capacity_under_the_fence`
- Changed existing test (D1 supersedes its premise): `palw_tir_public_fold_v1::…::rooted_service_hold_retains_final_collateral_and_material_past_hard`
  no longer asserts that a bound claim cannot go Final / that a stale deadline is skipped; it still asserts retirement, collateral and
  material are held.

## Fences

None new. Reuses `palw_tir_public_conviction_v1` (observation, release, V3 row writes, funded admission) and `palw_court_capacity_v1`
(funded reserve). Bindings exist only past the conviction fence (F02 with a pinned `PALW_COURT_SERVICE_PROFILE_V1`, or a funded
admission), so every Final-schedule change is inert where no binding exists.

## Central steps needed

- Regenerate `palw_native_resource_v1/generated.rs`, `wire_generated.rs`, `private/*`: new row variant `PalwStagedCourtRowV1::CourtServiceV3`
  (borsh tag 36) and new types `CourtServiceRowV3`, `ResolvedServiceBindingV1` (derive borsh; no manual codec). Without it the crate
  does not compile (`native_enum!` / `native_struct!` coverage) and the wire scanner refuses row tag 36.
- Row tag 36 collision check against other lanes appending to `PalwStagedCourtRowV1`.

## Open / uncertain (honest)

1. Existing tests that relied on "a bound claim never goes Final" or "a binding is a pending accusation" may now fail
   (`palw_tir_public_fold_v1` held-claim tests ~3700–4200, r5-f02-wire post-Final tests); they should be re-pointed to D1 semantics.
2. The fold test needs the fixture's `finalize_claim` to reach a terminal phase from `demanded_world`'s claim phase; if not, adjust
   the fixture (the assertion is explicit).
3. The release's dependent scan is O(staged rows) on blocks with an eligible candidate; a per-claim index (like r5-f02-wire's) would
   bound it.
4. Retired packets in a slot shared with a long-lived binding stay until that slot is compacted (bounded by the live book).
5. The funded install's V3 write and capacity check are exercised only in the central E2E (as r5 noted for the whole funded install).
