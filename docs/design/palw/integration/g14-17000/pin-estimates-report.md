# Integrator value pins from the approved estimates (lane LEGACY, 2026-10-26)

Branch `claude/g14-pin-estimates` (base `d8aded6de`). The pins follow the same pattern as the 500 MSK pin (`ff28c237f`). Nothing pushed.

## (1) Classification and pins

| Estimate (`palw_g14_values_v1`) | Item | Production site | Status |
|---|---|---|---|
| `PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1` | V1 | `PALW_COURT_SERVICE_PROFILE_V1` | **pinned** |
| `PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1` (4) | inclusion delay | `PALW_COURT_SERVICE_INCLUSION_PREMISE_V1` (version 1) | **pinned** |
| `PALW_G14_COURT_INGRESS_ESTIMATE_V1` (1.1 M sompi/session) | V4 | `palw_court_ingress_schedule_v1()` | **pinned** |
| `PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1` | V5 | `material_intake_schedule_v1()` | **pinned** |
| `PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1` | V11 | no constant: `Params::palw_court_capacity_v1` + `PalwCourtMeasurementV1` record | approved but **not pinnable here**; the fence requires a measurement record (left for the integrator / record) |
| V2 funded-original / admission, V3 escrow split | V2, V3 | `palw_funded_original_schedule_v1`, `palw_escrow_fee_schedule_v1` | approved; values_v1 has constructors, but production wiring is outside this task's list, so they stay **typed-missing** |
| — | kernel court admission | `PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1` | **not set** (no estimate). Proposal: `{version 1, profile: V1 estimate, follow_up_branches_per_turn: 2 (PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1), capital_per_packet_sompi: 3.4 M (V2 capital per signer turn estimate)}`. Needs approval |
| V8 position budgets | V8 | `PALW_TIR_POSITION_WITNESS_BUDGET_V2` and related | not approved → typed-missing |

## (2) Identity

`t12_fleet_identity_247ade1f9::fleet_ids_match_247ade1f9` passes with the pins: params `ef379378…`, identity `5de80e64…`, schedule `9ef2b1a7…`. The constants are not part of `Params` and do not move the identity.

## (3) Tests with the pins

- `validate_palw_court_policy_completeness_v1`: **F02 now clears.** F03 clears when `palw_court_outsider_lane_v1` is in force at or below the fence (`court_f02_completeness_gate_clears_each_half_on_its_own_evidence` was updated and passes). A ruleset without the outsider lane still refuses with `OneOutsiderCapacityIncomplete` only.
- Slice node tests (`g14_f03k2_*` ×3, `lane_seed_*` ×2, `g14_spine_s3c`, stage table): these fail **with the same messages on the base values too** (interim assignment, class registration, s3c defaults without a premise), so the pins are not the cause. With the pins, s3c (the no-premise hold) can no longer happen in a node build, because production now has a premise; that stage needs redefining. The clock lane (`claude/g14-clock2`) is not in base and was not run.
- Tests that assumed typed-missing and now need fixture updates (not edited):
  - `palw_state_v2::tests::court_f02_lifecycle_is_dormant_below_its_fences`: F02 is now active past an armed fence.
  - `r5_f03_the_ingress_fee_is_charged_once…`: the fixture opener has 10,000 collateral, less than the 1.1 M ingress fee.
  - `r5_f03_colluders_refilling…`: asserts the ingress is `None`.
  - legacy tests on this branch (fixed on `claude/g14-fam-legacy2`).
- Failing on base and unrelated to the pins: `fork_id_v1::…every_scheduled_palw_fence…` (`palw_selected_source_bootstrap_v1` missing from the probe), `palw_paid_material_generation_v5::identity_controls` ×2, `routing_v2::routed_retrieval…`.

## (4) Stale pin in `testnet12_identity_did_not_move`

The identity is unchanged. The 5ee7fd8e/1678e073 values were pinned in `fdb5d41cf`. The merge `d2214e057` (the source of the public fleet 247ade1f9, int-13 list at DAA 15,500) moved params and schedule to `ef379378`/`9ef2b1a7`. Both tests now use those values and pass.
