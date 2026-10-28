# G14 lane r5-f02-wire — F02 wire, post-Final default / hold / recovery, per-claim index, bottom evidence (2026-10-22)

Branch `g14p/r5-f02-wire`, base `64bc87dcd`. **Status: written, not compiled, not run** (lane rule: no cargo). Nothing is armed.

## NEW OBJECT TAG: **178** (for central collision check)

`PalwConsensusObjectV2::SharedCourtOpV1 { op: Box<PalwSharedCourtOpV1> } = 178`, owned by `palw_tir_public_conviction_v1`
(`PalwLifecycleKindFenceV1::TirPublicConvictionV1`). Registered in: `PALW_LIFECYCLE_NEW_KINDS_V1` (178),
`palw_lifecycle_kind_owner_v1` (TirPublicConvictionV1 arm), `PALW_A2_KIND_FENCE_TABLE_V1` (`ObjectTags{178,178}` row),
`PALW_A2_TAG_ALLOCATIONS_V1` (`(178,178,…)`), `zero_filled_kind(178)` hand sample, wire pin `(178, 0x1d759cbe80301463)` derived by
`python3 -I scripts/drills/a2u_new_kind_wire_pins.py` (the Rust test `every_landed_kind_is_pinned_to_its_wire_form` is the authority;
re-pin from its listing if it differs). The script's positional `--check` reports pre-existing unpinned rows 245/175 (not this lane's).

## Implemented

| Item | Where | What |
|---|---|---|
| 1 carriage | `consensus/core/src/palw_court_shared_op_carriage_v1.rs` (new) | `PalwSharedCourtOpV1 { Body, Ids, Request, Numerical, NativeHeldDefault }`, hand-written bounded `BorshDeserialize` (body ≤ 90,000 B, ids ≤ 22,500, parts 1..=5, request signature exactly ML-DSA-87 length — refused before allocation), `validate_shape_v1`, `validate_authority_v1` (request signer must be a CURRENT `Active` bond at the floor + ML-DSA-87 via the existing `SignedSharedRequestV1::validate_bond_signature`). |
| 1 mempool/template | `palw_lifecycle_objects_v2.rs` may-ride arm | Rides at every height with no stateless refusal (A-2, like 170/171); kind owner gates by fence; heartbeat carriers list (`palw_heartbeat_carriers_v1.rs`) treats it as court traffic. No miner-side change needed (generic lifecycle carriage). |
| 1 validation | `consensus/src/.../palw_validate.rs` (court list), `palw_validate/court.rs` `Obj::SharedCourtOpV1` | Below the kind fence: unjudged (A-2). Past it: outsider lane (`state_params.court_outsider_lane_active_v1`) for the shared lifecycle, or `native_liability_horizon_active_at` for `NativeHeldDefault`; then `validate_authority_v1` with `palw_network_domain_v2()`. |
| 1 fold dispatch | `palw_state_v2/objects.rs` arm → `palw_court_service_lifecycle_fold_v1::apply_shared_court_wire_op_v1` | Maps the borrowed wire op onto `apply_shared_court_op_v1` (unchanged semantics); `NativeHeldDefault` goes to the post-Final default. Names added in `processor.rs` / `kaspad/src/palw_panel.rs`. |
| 2 post-Final default | `consensus/core/src/palw_native_post_final_default_fold_v1.rs` (new child of `palw_state_v2`) | `decide_native_post_final_default_v1` (pure): text29/funded11 held row inside H, the claim's service binding, the actor's (= held producer's) duty clocks (via the new index), every window closed, family silence fact, then the existing `attribute_service_default_v1`. `apply_native_post_final_default_with_v1`: `PartyDefault` → atomic `refund_native_fee_v1` (requester refund; producer income cancelled; budget closed; text29 quota subject closed) + `slash_bond(producer, slash)`; `NoSlashHold` → nothing moves. Wire entry `apply_native_post_final_default_v1` refuses a no-slash hold with its reason. |
| 3 recovery / release | same file, `recover_native_post_final_defaults_v1`, called in `palw_kernel_route_fold_v1::tick_kernel_route_v1` before `release_held_native_fees_v1` | Each tick re-evaluates held text29/funded11 rows inside H whose claim is bound (≤64/block, key order); a hold whose slot the canonical `repair_missing_supply` sweep has since supplied resumes as a default. Past H nothing is decidable and the unchanged release sweep pays the fee **without slash**. While held, the fee row (evidence/quota subject retained to H) and the producer's budget reservation stay in place. |
| 4 index | field `staged_court_service_by_claim` (`palw_state_v2.rs`), projection `palw_staged_court_state_v1::court_service_claim_index_entry_v1`, maintenance in `palw_staged_court_fold_v1::update_tir_indices_v1` (reached by `update_indices` on every writer call AND delta apply/undo) + `rebuild_indices`; quote in `reserve_staged_indices_v1` (only for the two projected kinds); readers `chain_state.rs::{court_duty_clocks_for_claim_v1, court_shared_initial_notice_daa_v1}` | Derived from rooted `staged_courts` rows exactly like `staged_tir_by_claim`: as rooted and reorg-safe as the rows. `court_service_claim_scan_v1` keeps the old full scan as the specification for tests. |
| 5 bottom evidence | `palw_court_service_lifecycle_fold_v1::capture_shared_bottom_evidence_v1` (called before `apply_shared_bottom_conviction_v1`); `palw_court_end_archive_v1::adjudication` arm | The archive's evidence is `Native { point, SharedCourtOpV1::Numerical{manifest,parts} }` (the actual bundle), never `Uncaptured`; previous evidence restored after. A same-claim Bottom bundle reads as `TirClaimFault`. No borsh layout changed (existing `Native` variant reused). |

## Tests (written, not run) — `cargo test -p kaspa-consensus-core --lib r5_f02_wire`

- `palw_court_shared_op_carriage_v1::tests::r5_f02_wire_shared_op_borsh_round_trips_every_variant`
- `palw_court_shared_op_carriage_v1::tests::r5_f02_wire_shared_op_object_tag_is_178_and_round_trips`
- `palw_court_shared_op_carriage_v1::tests::r5_f02_wire_shared_op_boundary_decode`
- `palw_court_shared_op_carriage_v1::tests::r5_f02_wire_shared_op_authority_refuses_an_absent_bond`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_tag_178_is_dormant_below_its_fences`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_post_final_default_is_dormant_and_production_is_typed_missing`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_post_final_supplied_and_silent_refunds_the_requester_and_slashes_the_actor`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_post_final_open_window_answered_or_unknown_is_undecided`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_missing_supply_is_a_no_slash_hold_and_the_sweep_resumes_once_supplied`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_an_unrecovered_hold_releases_at_the_horizon_without_slash`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_claim_index_matches_the_scan_and_follows_writes_and_undo`
- `palw_state_v2::tests::r5_f02_wire_v1::r5_f02_wire_shared_bottom_captures_its_actual_end_evidence`
- Existing A-2 reconciliation tests in `palw_lifecycle_objects_v2::tests` must pass with the new rows (pin, table, allocation, owner).

## Fences to arm centrally (no new fence)

`palw_tir_public_conviction_v1` (kind owner), `palw_court_outsider_lane_v1` (shared lifecycle), `palw_native_liability_horizon_v1`
(+ `palw_settlement_once_v1` at/below it) for the post-Final default; plus the typed-missing `PALW_COURT_SERVICE_PROFILE_V1`.

## Central steps needed

- Regenerate `palw_native_resource_v1/generated.rs` / `wire_generated.rs` / `private/palw_state_v2.rs`: new enum variant
  `SharedCourtOpV1`, new type `PalwSharedCourtOpV1` (custom deserializer), new `PalwChainStateV2` field `staged_court_service_by_claim`.
- Tag 178 collision check against other lanes.

## User decisions / open (honest)

1. **`PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1 = None`** (money amount). While `None` the production default and sweep do nothing.
2. **Silence after Final is not recorded by text29 / funded v11** (rfc08-hold open 1: post-Final DA not reopened, no native duty
   clocks/answers are installed for those families). `native_post_final_duty_answered_v1` therefore refuses for both → production
   is undecided, never inferred. The mechanism is proven with an injected silence fact in tests; a family must install post-Final
   `CourtDutyClockV1` rows and record answers (design pass) before it can fire. The packet set and duty clocks are TIR-derived
   (RFC-0028 binding is installed only at TIR original admission); native families install no binding yet.
3. After a post-Final default, text29's book stays `Final` with its credit row naming the producer queue (never paid); its
   collect/GC readers were not re-reviewed for this terminal shape (same caveat as rfc08-hold open 5).
4. The TIR (`palw_tir_public_fold_v1`) default path still REFUSES a `NoSlashHold` (court-f02 behaviour); it resumes when the default
   control is re-filed after recovery, not by an automatic sweep.
5. `NativeHeldDefault` carries no filer signature and pays no finder (the default is objective from recorded state; the carrier fee
   is the spam cost). Shared ops are not priced by `palw_court_transport_cost_v1` (no capacity charge added).
6. The derived index is not serialized; like every `staged_*` index it is rebuilt by replay/`rebuild_indices`. Tests that insert
   `staged_courts` rows directly must call `rebuild_indices`.
7. Design scope: RFC-0029 (shared lifecycle carriage), RFC-0026 / ADR-0192 A (attribution from recorded supply), ADR-0184 §1 /
   RFC08 08-B (default → refund within H, release at H), RFC-0031 (initial notice). No new RFC/ADR.
