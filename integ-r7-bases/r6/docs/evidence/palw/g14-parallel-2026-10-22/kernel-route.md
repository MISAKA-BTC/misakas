# Lane kernel-route (U01–U04) — 2026-10-22

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/kernel-route`, base 8b036ed38.

## What was implemented

### U01 — kernel-route acceptance rule / arm gate
- What "the kernel-route acceptance rule" means: the rule is already built. Where `palw_probabilistic_constraints_v1` is in force the
  virtual processor folds tags 110–119 (and the onboarding fold, 104–107) into `PalwChainStateV2`
  (`palw_kernel_route_fold_v1`, and through it v3/v7/v9/v11/v23/v28 and `palw_onboarding_fold_v1`). A claim's acceptance is the kernel
  ledger's predicate: committed claim → post-commit seat draw (`draw_seats`) → receipts reaching the assignment's quorum, with no open
  filing or demand and the window passed → Final/settlement (ADR-0171 §2). The old message ("this binary has no kernel-route
  acceptance rule") was stale. What is missing is the **evidence** ADR-0171 §5 / RFC-0011 §15.7 require, so the refusal is now by
  record, not by name.
- New module `consensus/core/src/palw_kernel_route_arming_v1.rs`:
  `PalwKernelRouteGatedFenceV1` {ProbabilisticConstraintsV1, SignedRegistrationV1, ProviderCourtV1}, requirement lists
  `PALW_KERNEL_ROUTE_ARMING_REQUIREMENTS_V1` (8: fold suite, node E2E, adversarial suite, soundness/beacon review, shadow comparison,
  9B-8k/2M reports, court resource bounds, fence-crossing drill), `PALW_SIGNED_REGISTRATION_ARMING_REQUIREMENTS_V1` (3),
  `PALW_PROVIDER_COURT_ARMING_REQUIREMENTS_V1` (4, incl. measured lease terms and claim-scoped-only suite);
  `PalwKernelRouteArmingRecordV1` (version, fence, genesis, activation_daa, source_commit, evidence digests, review) with the same
  form checks as `palw_g14_gates_v1` (reuses `PalwG14ArmingRefusalV1`); `PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 = &[]` (EMPTY → every armed
  height refused); `Params::validate_palw_kernel_route_fence_record_v1 / _in_v1`. Isolated G14 test genesis skips the record pin
  (as G14 gates). The isolated calibration34 branch in `validate_palw_probabilistic_constraints_v1` is unchanged.
- `palw_probabilistic_constraints_v1.rs::validate_palw_probabilistic_constraints_v1` now calls the record gate.
- Design choice: a separate enum (not new variants of `PalwG14GatedFenceV1`) because adding variants to `PalwG14GatedFenceV1::ALL`
  would break exhaustive matches/assumptions in `consensus/core/tests/g14_close_gates.rs` (`set`, the G14-from-genesis test that
  expects every ALL fence at 0), `palw_court_policy_v1.rs` tests and `kaspad/src/private_integration/g14_drill_factory.rs`.
  Consequence: `validate_palw_v2_with_arming_records_v1(records)` does NOT inject kernel-route records; tooling must use
  `validate_palw_kernel_route_fence_record_in_v1`. Central step may want a `_with_kernel_route_records` path for drills.

### U02 — Panel=1 seat policy (USER DECISION PENDING: 1 seat vs keep 3-of-3)
- New Params field `palw_kernel_route_panel_one_v1: Option<ForkActivation>` (config/params.rs; None on all 4 presets; carried in
  `override_params`; accessor `palw_kernel_route_panel_one_v1_fence`). Hashed Some-only in `consensus_schedule_id` and
  `consensus_params_id` (config/params/identity.rs, mirrored on `palw_court_outsider_lane_v1`; destructures updated).
- Mirror `PalwStateParamsV2::kernel_route_panel_one_from_daa` (`#[borsh(skip)]`), written by `Params::sync_palw_g14_gates_v1`;
  accessor `kernel_route_seats_and_quorum_v1(daa)`.
- `palw_kernel_route_v1.rs`: `PALW_KERNEL_ROUTE_PANEL_ONE_SEATS_V1 = 1`, `..._QUORUM_V1 = 1`,
  `palw_kernel_route_seats_and_quorum_v1(panel_one_from, daa)` → (1,1) past the fence, else interim (3,3). Interim constants kept.
- Consumers: `palw_kernel_route_fold_v1::draw_seats` (new `draw_daa` arg = `ctx.daa_score`), `palw_claim_challenge_fold_v2::lock`
  and `::round_lock`. The upper-bound checks `seats.len() > PALW_KERNEL_ROUTE_INTERIM_SEATS_V1` in `palw_claim_challenge_v2.rs:818`
  and `palw_claim_interactive_v3.rs:45,70` are left as is (1 ≤ 3 passes; they are structural maxima).
- Validation `Params::validate_palw_kernel_route_panel_one_v1` (called from validation.rs after the provider court): needs
  `palw_probabilistic_constraints_v1` at or below it and the bundle mirror to agree. No record of its own (the kernel route's record
  gates it). Default unchanged.
- Not done: routing the draw through Gate C's seat/beacon (survey suggestion) — the interim seed is unchanged; only the count changes.

### U03 / U04 — signed registration and provider court
- `palw_onboarding_v1.rs::validate_palw_signed_registration_v1` and `palw_provider_court_v1.rs::validate_palw_provider_court_v1`
  now call the same record gate; additionally each needs `palw_probabilistic_constraints_v1` at or below it (tag 108 and the
  provider objects ride the kernel route's fold). The node-side connection already exists
  (`consensus/src/pipeline/virtual_processor/processor/palw.rs` ~8180/8290/8316/8341/8589 read both fences; `palw_state_v2.rs:18888`).
  Claim-scope only: unchanged code (Artifact subject withdrawn, `palw_court_demand_allowed_v1`); the requirement list makes the suite
  showing it a precondition.
- Open question for the user: the survey says registration "should be armable without kernel-route seat economics"; this lane
  still requires the kernel route fence at or below it because tag 108 is folded only by that route. Decoupling would need a fold change.

## Tests added (consensus-core lib, `cargo test -p kaspa-consensus-core --lib palw_kernel_route_arming_v1`)
- `palw_kernel_route_arming_v1::tests::kernel_route_fences_are_dormant_and_refused_without_a_record`
- `palw_kernel_route_arming_v1::tests::a_pinned_record_for_exactly_this_network_and_height_admits_the_kernel_route`
- `palw_kernel_route_arming_v1::tests::panel_one_seat_policy_is_dormant_by_default_and_selects_one_seat_when_armed`
- `palw_kernel_route_arming_v1::tests::registration_and_provider_court_need_the_kernel_route_at_or_below_them`
Existing tests that should still hold: `palw_probabilistic_constraints_v1::tests::the_fence_is_dormant_everywhere_and_refused_when_armed`,
`palw_provider_court_v1::tests::*`, `tests/rfc0015_panel_free.rs`, `tests/c4r3_fork_id_at_int13.rs`, r4x `da16_scope.rs`.
Not written: a fold-level test that a Panel=1 network's `draw_seats` writes a 1-seat/quorum-1 assignment (node E2E needed; the
g14_kernel_route_e2e harness is the place).

## Fences needed (central arming on t12 @16000)
`palw_probabilistic_constraints_v1`, `palw_signed_registration_v1`, `palw_provider_court_v1` (each needs a pinned record in
`PALW_KERNEL_ROUTE_ARMING_RECORDS_V1`, or an isolated G14 genesis), and `palw_kernel_route_panel_one_v1` only if the user chooses Panel=1.
Arming t12 needs `sync_palw_g14_gates_v1` after setting the panel-one fence.

## Risks
- Uncompiled. Watch: identity.rs destructure counts, the `ForkActivation` import still used in onboarding/provider files, `b.params`
  type in the challenge fold.
- `fork_id_v1.rs` name→field list (~line 787/950) does not include the new panel-one field (outsider lane isn't there either); check
  whether `for_each_fence`/fork-id must visit it.
