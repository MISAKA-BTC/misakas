# Lane d2-finish (U08–U10) — 2026-10-22

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/d2-finish`, base `8b036ed38`.

## U08 — tag 230 seat readiness root (ADR-0188 B / ADR-0200 C1)

The B codec (`SeatReadinessRootV1` root + count, `ReadinessDigestPathV1` path, `check_path`, `refute`) already existed in
`consensus/core/src/palw_court_model_scope_routes_v1/readiness_root.rs`. This lane made it a lifecycle kind:

- `consensus/core/src/palw_state_v2.rs`: `PalwConsensusObjectV2::SeatReadinessRootV1 { root, signature } = 230`. Its borsh bytes
  are `encode_tagged()` followed by the signature (tested). New error variant `PalwStateV2Error::ReadinessRootRefused`.
- `consensus/core/src/palw_lifecycle_objects_v2.rs`: new local fence slot `PalwLifecycleKindFenceV1::CourtModelScopeV1 = 19`
  (`palw_court_model_scope_v1`, resolved straight from `Params::palw_court_model_scope_v1`); owner arm; `PALW_LIFECYCLE_NEW_KINDS_V1`
  row `(230, "SeatReadinessRootV1", CourtModelScopeV1)`; the A-2 row for 230 flipped to `landed: true`; allocation comment updated;
  may-ride shape check (nonzero count, signature present); wire pin `(230, 0x9cfea943bbe78428)` from
  `python3 -I scripts/drills/a2u_new_kind_wire_pins.py` (the Rust test is authority).
- `consensus/core/src/palw_state_v2.rs` (fence mirror for the fold): `K::Fence(F::CourtModelScopeV1)` reads
  `params.court_model_scope_from_daa()`.
- Validator `consensus/src/pipeline/virtual_processor/processor/palw_validate/court.rs` (dispatch added in `palw_validate.rs`):
  dormant below the fence; past it `palw_d2_readiness_root_admit_v1(root, PALW_D2_READINESS_DRAW_CAP_V1)` first (cap is `None` ⇒
  always refused), then the seat bond's ML-DSA-87 signature over `palw_d2_readiness_root_message_v1(network_domain, root)`.
- Fold `consensus/core/src/palw_state_v2/objects.rs`: same cap admission (refuses). Pure registry fold
  `readiness_root::palw_d2_readiness_root_fold_v1` (one root per (beacon, class, seat, span)) exists and is tested but is **not
  persisted as a state field** — no root-only ready/slash door is opened (ADR-0200), and the persistent registry needs the cap.
- Names: `processor.rs` object name, `kaspad/src/palw_panel.rs` name, heartbeat carriers (`false`), t12_a2u fence arming arm.
- Doc strings in `palw_court_model_scope_routes_v1.rs` updated (no longer "not landed").

Tests (consensus-core lib, `cargo test -p kaspa-consensus-core --lib readiness_root`):
`tag230_unadopted_draw_cap_refuses_admission_and_fold`, `tag230_fold_writes_once_per_seat_and_draw_inside_the_cap`,
`tag230_lifecycle_object_round_trips_and_refuses_boundary_bytes` (plus the existing two codec tests). Existing reconciliation
tests that now cover 230 generically: `the_new_kind_table_is_the_owner_function`, the A-2 table test (row landed ⇔ kind in table),
`every_landed_kind_is_pinned_to_its_wire_form`, `every_tag_row_is_inside_its_allocation`, and in `kaspa-consensus`
`t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_*` (zero-filled 230 rides undecided below its fence).

Pre-existing at base (not this lane): `PALW_A2_NEW_KIND_WIRE_V1` lacks pins for 245 and 175 (the drill script reports them), so
`every_landed_kind_is_pinned_to_its_wire_form` should already fail at base; central step should re-pin from `[a2u-pins]`.

## U09 — R-D2-NODE: node filer claim-part route

`kaspad/src/palw_fraud_filer.rs`:
- `palw_fraud_filer_d2_governs_v1(params, accepted_daa)` — node mirror of `palw_court_model_scope_governs_claim_v1`.
- `palw_fraud_filer_d2_route_v1(d2, step, witness_leaf, view, answer_read)` — pure; on a D2 claim a held `StepLeaf` demand
  (`Terminal`), `HeldRoute`, or a terminal without the leaf's witness becomes the claim-only `KernelWitness` demand/await/learn;
  the terminal only once that leaf's witness is held. Applied in the main loop right after every step source (bootstrap,
  constraint, legacy k-ary, G14-V subtree).
- LegacyTerminal handler: on D2 and a non-fused leaf, `PalwLegacyReplicaV2::claim_part_terminal` (new, `kaspad/src/palw_legacy_held_v2.rs`)
  builds `ShardCourtAccused` from witness + this verifier's own rows, filed only if `palw_d2_one_move_on_claim_part_v1` says
  `ExecutorGuilty`. Fused leaves keep the existing fused terminal (witness + own rows already).
Tests (kaspad bin/lib tests in `palw_fraud_filer::tests`): `d2_governs_only_claims_accepted_at_or_past_an_armed_fence`,
`d2_claims_take_the_claim_part_route_instead_of_the_held_step_leaf`.
Node-level e2e already exists from the test harness side: `d2_a_full_collusion_on_a_new_claim_is_convicted_before_final_on_the_node`
and `d2_the_pursuit_survives_a_restart` (consensus `lg14a_legacy_filer_e2e`, un-ignored at 0988d4279) — they drive a test Verifier,
not the kaspad filer; a kaspad-filer e2e on a D2 claim is still to write.
Requirement: the KernelWitness unit is LG14-B's, so the D2 node route needs `palw_legacy_held_da_v2` armed with D2.

## U10 — X3 reorg replay with pre-cutoff reservation and session

Already done at base: `e27771b96` replaced the IGNORED `x3_req_5c` with node-clock-based shallow-tie tests
`x3_req_5c_a_reorg_moves_the_open_session_hold_exactly_with_the_branch` and `x3_req_5d_a_reorg_moves_the_reservation_exactly_with_the_branch`
(non-ignored; fork asserted `<= open_cutoff_daa`; reservation + open session on A, rollback on B, restore on return). Recorded as
4 passed in `docs/design/palw/integration/codex-g14-progress-2026-10-20.md`. No change made; the survey row is stale.

## Fences to arm centrally
`palw_court_model_scope_v1` (D2; with `sync_palw_2026_10_12_ruling_fences_v1`), and `palw_legacy_held_da_v2` for the node route.
Note `validate_palw_2026_10_12_ruling_fences_v1` currently refuses an armed D2 — central arming must account for that.

## Open / user decisions
- Readiness draw cap: `PALW_D2_READINESS_DRAW_CAP_V1 = None` (unadopted) ⇒ tag 230 is always refused past the fence. User decision.
- Persistent readiness-root registry (state field, delta, root, carriage) and refutation kind/qualified effects (ADR-0200 C1 supply)
  not implemented.
- Native/resource generated tables (`palw_native_resource_v1/generated.rs`, `wire_generated.rs`) need regeneration for the new variant.
