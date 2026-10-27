//! **testnet-12's full-activation flag day** (lane g14p/r7-fence-16000, 2026-10-22) — ONE list, [`PALW_T12_FULL_ACTIVATION_FENCES_V1`],
//! that arms every G14 / RFC fence of the full-activation release at ONE height, [`PALW_T12_FULL_ACTIVATION_DAA`] — which is `None`:
//! **this file arms nothing**. The target is DAA 16,000; the height is set only after every arming record the list's validation asks
//! for is pinned in the binary and the user approves (`docs/evidence/palw/g14-parallel-2026-10-22/r7-fence-16000.md`).
//!
//! The pattern is the int-13 list's ([`super::PALW_T12_INT13_FENCES_V1`]) exactly: each entry sets its field AND the mirrors the fold
//! reads, through its own `set`; the list is in the order its entries' prerequisites need (each entry's validation names what must be
//! at or below it, and every such prerequisite is either earlier on this list or already in force on testnet-12); dormant is set in the
//! reverse order; every entry is hashed Some-only, so while the height is `None` every shipped id is the int-12 release's, byte for byte.

use super::*;

/// **Every mirror an entry of this list may move**, re-derived from the fences. Each `sync_*` is a pure function of `Params` into the
/// V2 bundle's copy (idempotent), so calling all of them after any one entry's field changes keeps every mirror the fold reads in step —
/// including the G14 route mirror, which reads many of this list's fences at once. No `sync_*` moves a fence (the drill's mover checks
/// that every unlisted fence stays where it was).
fn palw_full_activation_sync_mirrors_v1(params: &mut Params) {
    params.sync_palw_legacy_public_filer_v1();
    params.sync_palw_2026_10_12_ruling_fences_v1();
    params.sync_palw_court_capacity_v1();
    params.sync_palw_settlement_once_v1();
    params.sync_palw_native_liability_horizon_v1();
    params.sync_palw_unresolved_bound_v1();
    params.sync_palw_claim_local_settlement_v1();
    params.sync_palw_memory_routing_court_v2();
    params.sync_palw_bond_budget_v1();
    params.sync_palw_rfc0016_v1();
    params.sync_palw_rfc0018_v1();
    params.sync_palw_model_tier_v1();
    // Last: the route mirror reads the resolved heights of the fences above.
    params.sync_palw_g14_gates_v1();
}

/// One bare-height entry: the field, then every mirror.
macro_rules! palw_full_activation_bare_entry_v1 {
    ($const_name:ident, $field:ident) => {
        #[doc = concat!("`", stringify!($field), "` as an entry of [`PALW_T12_FULL_ACTIVATION_FENCES_V1`]: the bare height, then every mirror.")]
        pub const $const_name: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
            name: stringify!($field),
            set: |params, at| {
                params.$field = at;
                palw_full_activation_sync_mirrors_v1(params);
            },
        };
    };
}

/// `palw_bond_budget_v1` (ADR-0176) as an entry: the height with the lane VAL 2026-10-26 approved V3 policy (the 2026-10-22 V12-b
/// 目安, `palw_bond_budget_approved_policy_digests_v1`, replaced by ADR-0176 Stage 1 measurement), then every mirror. Settlement once
/// needs it at or below (lane CLOCK, coordinator 2026-10-26).
pub const PALW_T12_FULL_BOND_BUDGET_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_bond_budget_v1",
    set: |params, at| {
        params.palw_bond_budget_v1 = at.map(|activation| crate::palw_bond_budget_v1::PalwBondBudgetFenceV1 {
            activation,
            policy: crate::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1(),
        });
        palw_full_activation_sync_mirrors_v1(params);
    },
};

palw_full_activation_bare_entry_v1!(PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY, palw_probabilistic_constraints_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_KERNEL_ROUTE_PANEL_ONE_ENTRY, palw_kernel_route_panel_one_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_SIGNED_REGISTRATION_ENTRY, palw_signed_registration_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_PROVIDER_COURT_ENTRY, palw_provider_court_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY, palw_legacy_public_filer_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_LEGACY_HELD_DA_V2_ENTRY, palw_legacy_held_da_v2);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY, palw_staged_public_court_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_STAGED_TIR_ROUTE_ENTRY, palw_staged_tir_route_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_STAGED_FLOAT_CONSTRAINT_ENTRY, palw_staged_float_constraint_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_TIR_CONTAINER_IDENTITY_ENTRY, palw_tir_container_identity_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_TIR_PUBLIC_CONVICTION_ENTRY, palw_tir_public_conviction_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_TIR_DESIGNATED_ANSWERS_ENTRY, palw_tir_designated_answers_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_TIR_BOUNDED_DOOR_ENTRY, palw_tir_bounded_door_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_COURT_OUTSIDER_LANE_ENTRY, palw_court_outsider_lane_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_COURT_CLOCK_V2_ENTRY, palw_court_clock_v2);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_COURT_MODEL_SCOPE_ENTRY, palw_court_model_scope_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_PUBLIC_EVIDENCE_REWARD_ENTRY, palw_public_evidence_reward_v1);
// Compute-claim audit CX-01/CX-02 (lane compute-exec, 2026-10-22): a decoded answer is not paid twice.
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_FP_ANSWER_ACCOUNTING_ENTRY, palw_fp_answer_accounting_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_SETTLEMENT_ONCE_ENTRY, palw_settlement_once_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_NATIVE_LIABILITY_HORIZON_ENTRY, palw_native_liability_horizon_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_REWARD_LIABILITY_HORIZON_ENTRY, palw_reward_liability_horizon_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_UNRESOLVED_BOUND_ENTRY, palw_unresolved_bound_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_GATE_C_ENTRY, palw_independently_work_certified_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_DAG_SIBLING_ATTRIBUTION_ENTRY, palw_dag_sibling_attribution_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_PANEL_ANCHOR_PERMISSIONLESS_ENTRY, palw_panel_anchor_permissionless_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_MEMORY_ROUTING_COURT_V2_ENTRY, palw_memory_routing_court_v2);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_ELASTIC_FINAL_CAPACITY_ENTRY, palw_elastic_final_capacity_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_G14_ROUTE_REQUIRED_ENTRY, palw_g14_route_required_v1);
palw_full_activation_bare_entry_v1!(PALW_T12_FULL_COURT_COMPLETION_ATTEMPT_ENTRY, palw_court_completion_attempt_v1);

/// `palw_court_capacity_v1` (audit G-07): a value-carrying fence. `Some(at)` RE-TIMES a policy a reviewed change already installed; with
/// none installed it installs the codebase's placeholder (`PalwCourtCapacityFenceV1::placeholder_at`), which validation refuses on every
/// real network ("a measured (non-placeholder) policy" + a pinned record) — a typed refusal, never a silent zero. `None` clears it.
pub const PALW_T12_FULL_COURT_CAPACITY_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_court_capacity_v1",
    set: |params, at| {
        params.palw_court_capacity_v1 = match (at, params.palw_court_capacity_v1) {
            (None, _) => None,
            (Some(at), Some(mut installed)) => {
                installed.activation = at;
                Some(installed)
            }
            (Some(at), None) => Some(crate::palw_court_capacity_v1::PalwCourtCapacityFenceV1::placeholder_at(at)),
        };
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_claim_hard_deadline_v1` (X3): value-carrying. `Some(at)` re-times an installed (user-approved, measured) policy; with none it
/// installs `PalwClaimHardDeadlineFenceV1::unapproved_probe_at`, which validation refuses (no record; the X3 policy values are the user's
/// decision). `None` clears it.
pub const PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_claim_hard_deadline_v1",
    set: |params, at| {
        params.palw_claim_hard_deadline_v1 = match (at, params.palw_claim_hard_deadline_v1) {
            (None, _) => None,
            (Some(at), Some(mut installed)) => {
                installed.activation = at;
                Some(installed)
            }
            // G14 lane CLOCK (review part 2, B-1): the DECIDED policy — the windows' composition with the user's 2026-10-22 目安
            // (`PALW_X3_DECIDED_DELTA_TERMS_V1`, `c_retain` 36,058; replaced by Codex's M-X3 measurement) — never the probe, wherever
            // a V2 bundle supplies the windows. Without one (no V2 consensus) the probe stays and validation refuses it.
            (Some(at), None) => Some(match &params.palw_consensus_mode {
                crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                    match crate::palw_ruling_fences_v1::palw_x3_decided_policy_v1(&bundle.state, at) {
                        Ok(policy) => crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1 { activation: at, policy },
                        Err(_) => crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(at),
                    }
                }
                _ => crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(at),
            }),
        };
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_model_tier_v1` (ADR-0179): value-carrying. `Some(at)` re-times installed tiers/catalogs; with none it installs
/// `PalwModelTierFenceV1::at` (design values, empty genesis catalog — "NOT A PROPOSAL"), which validation refuses when armed.
pub const PALW_T12_FULL_MODEL_TIER_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_model_tier_v1",
    set: |params, at| {
        params.palw_model_tier_v1 = match (at, params.palw_model_tier_v1.clone()) {
            (None, _) => None,
            (Some(at), Some(mut installed)) => {
                installed.activation = at;
                Some(installed)
            }
            (Some(at), None) => Some(crate::palw_model_tier_v1::PalwModelTierFenceV1::at(at)),
        };
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_monetary_material_v1` (RFC08): value-carrying, its policy is money (collateral / fee floors) — the user's decision. `Some(at)`
/// re-times an installed policy; with none it installs an ALL-ZERO policy, which `PalwMonetaryMaterialPolicyV1::validate` refuses by name
/// ("missing or excessive capacity/cost bounds"): a typed refusal, never a value this lane chose.
pub const PALW_T12_FULL_MONETARY_MATERIAL_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_monetary_material_v1",
    set: |params, at| {
        params.palw_monetary_material_v1 = match (at, params.palw_monetary_material_v1.clone()) {
            (None, _) => None,
            (Some(at), Some(mut installed)) => {
                installed.activation = at;
                Some(installed)
            }
            (Some(at), None) => Some(crate::palw_monetary_material_v1::PalwMonetaryMaterialFenceV1 {
                activation: at,
                policy: crate::palw_monetary_material_v1::PalwMonetaryMaterialPolicyV1 {
                    version: 1,
                    max_contracts: 0,
                    max_contracts_per_provider: 0,
                    max_bytes_per_contract: 0,
                    max_bytes_per_provider: 0,
                    minimum_collateral_sompi: 0,
                    minimum_fee_sompi: 0,
                    window_daa: 0,
                    response_window_daa: 0,
                    max_enrollment_daa: 0,
                    cost_report_root: crate::Hash64::default(),
                },
            }),
        };
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// **testnet-12's full-activation list** — the int-13 list's four entries, then every fence the G14 / RFC lanes need, in the order the
/// entries' prerequisites need (each "needs X at or below it" of validation names an EARLIER entry, or a rule already in force on
/// testnet-12). The prerequisite each position answers:
///
/// 1–4. the int-13 list (audit 1004, the range twin over `palw_gen_v1` at 5,300, the model court window over the genesis k-ary court,
///      receipt spend v4 over the audit fences);
/// 5. `palw_probabilistic_constraints_v1` — the kernel route: Panel=1, Gate C, elastic capacity, settlement-once all need it below them;
/// 6. `palw_kernel_route_panel_one_v1` (needs 5);
/// 7–8. `palw_signed_registration_v1`, `palw_provider_court_v1` (monetary material needs the provider court below it);
/// 9–10. LG14-A `palw_legacy_public_filer_v1`, LG14-B `palw_legacy_held_da_v2`;
/// 11–13. the staged court (needs 9, G-18), the staged TIR route and the staged float constraint (need 11);
/// 14–17. TIR container identity, public conviction (needs 12, 11), designated answers and the bounded door (need 15);
/// 18–19. outsider lane and court clock v2 (need 9); 20. court capacity (needs 9); 20a. the liveness fix L-COURT-01's
///      `palw_court_completion_attempt_v1` (needs the genesis 2026-09-11 audit; after 20, whose cost path it charges too);
///      21. D2 court model scope (needs 10);
/// 22. N4 public evidence reward; 23. X3 hard deadline; 24. settlement once (needs 23, 5 — and the ADR-0176 bond budget, NOT on this
///     list: see the notes); 25. native liability horizon (needs 24); 26. the reward liability horizon; 27. RFC-0023 unresolved bound
///     (RFC-0023 stage 2's claim-local settlement is NOT on this list: its arming needs measurements, audit fix-cd);
/// 28–30. RFC-0018: Gate C (needs 5), DAG sibling attribution, permissionless Panel anchor;
/// 31. ADR-0179 model tiers (needs 28 — and `palw_model_bond_allocation_v1`, NOT on this list); 32. memory/routing court v2 (its parent
///     `palw_shard_court` is a genesis rule of testnet-12's held regime); 33. monetary material (needs 8 — and `palw_material_income_v3`,
///     NOT on this list); 34. elastic final capacity (needs 5 and the genesis model registry);
/// 35. `palw_g14_route_required_v1` LAST: its mirror is the resolved height of every route fence above.
pub const PALW_T12_FULL_ACTIVATION_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    // The int-13 list, unchanged and first.
    crate::palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY,
    crate::palw_gen_range_twin_v1::PALW_DRILL_GEN_RANGE_TWIN_ENTRY,
    PALW_T12_MODEL_COURT_WINDOW_ENTRY,
    crate::palw_receipt_v4::PALW_T12_RECEIPT_SPEND_V4_ENTRY,
    PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY,
    PALW_T12_FULL_KERNEL_ROUTE_PANEL_ONE_ENTRY,
    PALW_T12_FULL_SIGNED_REGISTRATION_ENTRY,
    PALW_T12_FULL_PROVIDER_COURT_ENTRY,
    PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY,
    PALW_T12_FULL_LEGACY_HELD_DA_V2_ENTRY,
    PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY,
    PALW_T12_FULL_STAGED_TIR_ROUTE_ENTRY,
    PALW_T12_FULL_STAGED_FLOAT_CONSTRAINT_ENTRY,
    PALW_T12_FULL_TIR_CONTAINER_IDENTITY_ENTRY,
    PALW_T12_FULL_TIR_PUBLIC_CONVICTION_ENTRY,
    PALW_T12_FULL_TIR_DESIGNATED_ANSWERS_ENTRY,
    PALW_T12_FULL_TIR_BOUNDED_DOOR_ENTRY,
    PALW_T12_FULL_COURT_OUTSIDER_LANE_ENTRY,
    PALW_T12_FULL_COURT_CLOCK_V2_ENTRY,
    PALW_T12_FULL_COURT_CAPACITY_ENTRY,
    // Liveness fix L-COURT-01: a refused completion spends the slot / court cost it was attempted against (needs only the
    // 2026-09-11 audit, a genesis rule of testnet-12; after court capacity, whose cost path it also charges).
    PALW_T12_FULL_COURT_COMPLETION_ATTEMPT_ENTRY,
    PALW_T12_FULL_COURT_MODEL_SCOPE_ENTRY,
    PALW_T12_FULL_PUBLIC_EVIDENCE_REWARD_ENTRY,
    PALW_T12_FULL_FP_ANSWER_ACCOUNTING_ENTRY,
    PALW_T12_FULL_BOND_BUDGET_ENTRY,
    PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY,
    PALW_T12_FULL_SETTLEMENT_ONCE_ENTRY,
    PALW_T12_FULL_NATIVE_LIABILITY_HORIZON_ENTRY,
    PALW_T12_FULL_REWARD_LIABILITY_HORIZON_ENTRY,
    PALW_T12_FULL_UNRESOLVED_BOUND_ENTRY,
    // `palw_claim_local_settlement_v1` (RFC-0023 stage 2) is NOT on this list (audit fix-cd): its validation refuses every armed
    // height ("material retention / transition drills are unmeasured"), which no code change can supply, so with it the list could
    // never validate as a whole. Rule E's side frontier now reads the claim-local irreversible anchors past that fence (audit D-1),
    // but the measurements remain the user's; it rejoins (before Gate C) with its measured record behind the standard gate.
    PALW_T12_FULL_GATE_C_ENTRY,
    PALW_T12_FULL_DAG_SIBLING_ATTRIBUTION_ENTRY,
    PALW_T12_FULL_PANEL_ANCHOR_PERMISSIONLESS_ENTRY,
    PALW_T12_FULL_MODEL_TIER_ENTRY,
    PALW_T12_FULL_MEMORY_ROUTING_COURT_V2_ENTRY,
    PALW_T12_FULL_MONETARY_MATERIAL_ENTRY,
    PALW_T12_FULL_ELASTIC_FINAL_CAPACITY_ENTRY,
    PALW_T12_FULL_G14_ROUTE_REQUIRED_ENTRY,
];

/// **The full-activation list's height — `None`.** Target: DAA 16,000 (a height no other list uses: not 750, 1,000, 1,300, 1,700,
/// 2,000, 3,600, 5,300, 5,395, 5,490, 5,585 — the fork id names heights, not fences). It is set to `Some(PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1)` (17,000, user goal 2026-10-26) ONLY after every
/// arming record each entry's validation asks for is pinned in the binary (G14 arm-readiness records, TIR frame-cost records, RFC-0608
/// measured records, approved X3 / court-capacity / monetary-material / model-tier policies) AND the user approves; the same change
/// updates `contrib/t12-deploy-kit/fleet.env.example` (pinned by `t12_deploy_kit_constants`) and re-pins the ids. One constant the preset,
/// the drill flag `--palw-drill-full-activation-at` and the kit share.
///
/// 2026-10-22: the value pin is in the parameter register. This height stays `None`. No gated fence has one evidence digest per
/// arming requirement (suite, node E2E, shipped-binary crossing drill). Those digests are not filled in from estimates.
/// `docs/evidence/palw/studio-values-2026-10-22/fence-readiness-2026-10-22.md`.
pub const PALW_T12_FULL_ACTIVATION_DAA: Option<u64> = None;

/// The intended height, for tests and the notes only — NEVER read by the preset.
pub const PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1: u64 = 16_600; // the value is 16,600 (user moved the flag day from 17,000, 2026-10-27); the name is kept

/// **The full-activation flag day, armed** — the list at [`PALW_T12_FULL_ACTIVATION_DAA`] on the ASSEMBLED ruleset, after the int-13
/// flag day; a no-op while the height is `None` (it leaves the list exactly as the earlier lists set it — in particular it does not clear
/// the int-13 entries this list shares, should the int-13 list ever take a height of its own).
pub(super) fn palw_t12_arm_full_activation_v1(params: &mut Params) {
    // **The G14 17,000 flag day (user decision 2026-10-27, drill skipped):** `PALW_T12_G14_17000_FENCES_V1` at
    // `PALW_T12_G14_17000_FLAG_DAY_DAA`, the int-13 entries left at 15,500. The FULL list (which carries fences this release does not
    // arm, e.g. monetary material, settlement once) keeps `PALW_T12_FULL_ACTIVATION_DAA = None`.
    if let Some(at) = PALW_T12_G14_17000_FLAG_DAY_DAA {
        palw_t12_arm_after_int13_at_v1(params, PALW_T12_G14_17000_FENCES_V1, Some(at));
    }
    let Some(at) = PALW_T12_FULL_ACTIVATION_DAA else { return };
    palw_t12_arm_after_int13_at_v1(params, PALW_T12_FULL_ACTIVATION_FENCES_V1, Some(at));
}

/// **The G14 17,000 flag day's height on the shipped testnet-12 preset — ARMED** (user decision 2026-10-27). One constant the preset
/// and the kit (`G14_FLAG_DAY_DAA` in `contrib/t12-deploy-kit/fleet.env.example`, pinned by `t12_deploy_kit_constants`) share.
pub const PALW_T12_G14_17000_FLAG_DAY_DAA: Option<u64> = Some(PALW_T12_G14_17000_DAA_V1);

/// Is `fence` one of the int-13 list's entries while that list has its own height ([`PALW_T12_INT13_DAA`], DAA 15,500 on the public
/// fleet)? Those entries keep that height: a later list must neither re-arm them at its own height nor set them dormant.
pub fn palw_t12_fence_held_by_int13_v1(fence: &PalwPostLaunchFenceV1) -> bool {
    PALW_T12_INT13_DAA.is_some() && PALW_T12_INT13_FENCES_V1.iter().any(|f| f.name == fence.name)
}

/// **A list that carries the int-13 entries, on the shipped testnet-12 ruleset** — like [`palw_t12_arm_fence_list_at_v1`], but the
/// int-13 entries are left exactly as the int-13 flag day set them (DAA 15,500). With the slice at 17,000 this moves only the slice's
/// remaining six entries, so every id below 17,000 is the fleet's. Drills that move the whole list to a low height keep using
/// [`palw_t12_arm_full_activation_at_v1`] / [`palw_t12_arm_fence_list_at_v1`].
pub fn palw_t12_arm_after_int13_at_v1(params: &mut Params, list: &[PalwPostLaunchFenceV1], at: Option<u64>) {
    match at {
        Some(at) => list
            .iter()
            .filter(|f| !palw_t12_fence_held_by_int13_v1(f))
            .for_each(|fence| (fence.set)(params, Some(ForkActivation::new(at)))),
        None => list.iter().rev().filter(|f| !palw_t12_fence_held_by_int13_v1(f)).for_each(|fence| (fence.set)(params, None)),
    }
}

/// **The full-activation flag day at `at`** (`None`: dormant) — the one body the preset, a drill and the tests share. Dormant is set in
/// the reverse order, so an entry is never the one a later entry stands on. With `None` the int-13 list's four entries (which this list
/// carries first) are set dormant too — exactly as [`palw_t12_arm_int13_flag_day_at_v1`] with `None` does.
pub fn palw_t12_arm_full_activation_at_v1(params: &mut Params, at: Option<u64>) {
    match at {
        Some(at) => {
            for fence in PALW_T12_FULL_ACTIVATION_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at)));
            }
        }
        None => {
            for fence in PALW_T12_FULL_ACTIVATION_FENCES_V1.iter().rev() {
                (fence.set)(params, None);
            }
        }
    }
}

/// **The vertical slice (coordinator plan 2026-10-26)** — the subset of [`PALW_T12_FULL_ACTIVATION_FENCES_V1`] that closes G14 for ONE
/// slice: the kernel route with Panel=1, its claim deadline, settlement once, Gate C (per-claim drawn approval), and the route-required
/// fence that leaves every other family route-absent (no new reward, G-01 (b)). Same order as the full list, so each entry's prerequisite
/// stays earlier. The bond budget is on it (settlement once needs it at or below; lane VAL's approved policy). Which genesis class and job shape the slice admits is NOT a fence and NOT a record field: records are keyed by (fence,
/// network, height) only, so the class must be carried by the route's own policy (lane SPINE names it). Nothing here arms anything.
pub const PALW_T12_KERNEL_SLICE_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    crate::palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY,
    crate::palw_gen_range_twin_v1::PALW_DRILL_GEN_RANGE_TWIN_ENTRY,
    PALW_T12_MODEL_COURT_WINDOW_ENTRY,
    crate::palw_receipt_v4::PALW_T12_RECEIPT_SPEND_V4_ENTRY,
    PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY,
    PALW_T12_FULL_KERNEL_ROUTE_PANEL_ONE_ENTRY,
    // G14 lane CLOCK (review part 2, M-1): the court scope's 4-dismissed-per-accuser-per-claim cap (`palw_provider_court_v1`), the
    // same record gate as the kernel route above it.
    PALW_T12_FULL_PROVIDER_COURT_ENTRY,
    // ADR-0176 bond budget with the approved V3 目安 policy: settlement once needs it at or below.
    PALW_T12_FULL_BOND_BUDGET_ENTRY,
    // X3 with the decided policy; past it the kernel route's claims run on its notice / hard clock (`misaka_palw_kernel::claim_clock`).
    PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY,
    PALW_T12_FULL_SETTLEMENT_ONCE_ENTRY,
    PALW_T12_FULL_GATE_C_ENTRY,
    PALW_T12_FULL_G14_ROUTE_REQUIRED_ENTRY,
];

/// **The FP constraint / prefix family list (lane FPCP, 2026-10-26)** — the subset of [`PALW_T12_FULL_ACTIVATION_FENCES_V1`] under which a
/// constraint (FP Job V6) or prefix-state (11 / 12) job can be admitted and earn again under option (b): `FpConstraint` / `FpPrefix`
/// (staged public court + LG14-A), and — because one FP claim spans several families — `FpTokenizer` (the same fences) and
/// `FpSelection` (the TIR public frame: `palw_tir_public_conviction_v1`, whose resolved fence needs the staged TIR route and the staged
/// court). Same order as the full list. `palw_legacy_held_da_v2` is NOT here: no FP family's route reads it. Nothing here arms anything;
/// every entry's own validation (records, measured values) still applies when a height is set.
pub const PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    crate::palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY,
    crate::palw_gen_range_twin_v1::PALW_DRILL_GEN_RANGE_TWIN_ENTRY,
    PALW_T12_MODEL_COURT_WINDOW_ENTRY,
    crate::palw_receipt_v4::PALW_T12_RECEIPT_SPEND_V4_ENTRY,
    PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY,
    PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY,
    PALW_T12_FULL_STAGED_TIR_ROUTE_ENTRY,
    PALW_T12_FULL_TIR_CONTAINER_IDENTITY_ENTRY,
    PALW_T12_FULL_TIR_PUBLIC_CONVICTION_ENTRY,
    PALW_T12_FULL_G14_ROUTE_REQUIRED_ENTRY,
];

/// **The DAA 17,000 testnet-12 arming list (lane ACT2, user decision 2026-10-27: arm the Panel=1 kernel slice and all six G14 route
/// families — LegacyPanelAttempt, LegacyFusedLeaf, FpSelection, FpConstraint, FpPrefix, FpTokenizer — together; the drill is waived by
/// the user, never recorded as run).** The union of [`PALW_T12_KERNEL_SLICE_FENCES_V1`], every name
/// [`crate::palw_g14_gates_v1::palw_g14_route_family_fences_v1`] returns for those families, and the fences their validation requires
/// at or below them, in [`PALW_T12_FULL_ACTIVATION_FENCES_V1`]'s order (prerequisites first) so one height satisfies every "at or below".
/// `palw_panel_da_disclosure_v1` (not on the full list) sits right before route-required (PanelDa added
/// to scope by the user 2026-10-27). RFC07 `palw_claim_challenge_v2` is NOT here. The preset does NOT read this list: arming it is [`PALW_T12_G14_17000_DAA_V1`] through [`palw_t12_arm_after_int13_at_v1`]
/// after review, and every entry's own validation (records, measured values) still applies.
pub const PALW_T12_G14_17000_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    crate::palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY,
    crate::palw_gen_range_twin_v1::PALW_DRILL_GEN_RANGE_TWIN_ENTRY,
    PALW_T12_MODEL_COURT_WINDOW_ENTRY,
    crate::palw_receipt_v4::PALW_T12_RECEIPT_SPEND_V4_ENTRY,
    PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY,
    PALW_T12_FULL_KERNEL_ROUTE_PANEL_ONE_ENTRY,
    PALW_T12_FULL_PROVIDER_COURT_ENTRY,
    PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY,
    PALW_T12_FULL_LEGACY_HELD_DA_V2_ENTRY,
    PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY,
    PALW_T12_FULL_STAGED_TIR_ROUTE_ENTRY,
    PALW_T12_FULL_TIR_CONTAINER_IDENTITY_ENTRY,
    // The frame cost value (lane VAL estimate). The handshake identity drops it while neither frame fence is active at genesis
    // (`normalize_values_a_scheduled_fence_drags_with_it`, ACT2 2026-10-27), so scheduling it keeps the fleet's identity.
    PALW_T12_G14_TIR_FRAME_COST_ESTIMATE_ENTRY,
    PALW_T12_FULL_TIR_PUBLIC_CONVICTION_ENTRY,
    PALW_T12_FULL_COURT_OUTSIDER_LANE_ENTRY,
    PALW_T12_FULL_COURT_CLOCK_V2_ENTRY,
    // Court capacity with the user-approved V11 estimate (2026-10-26) as the production policy (no measurement record).
    PALW_T12_G14_COURT_CAPACITY_ESTIMATE_ENTRY,
    PALW_T12_FULL_BOND_BUDGET_ENTRY,
    PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY,
    // Model tier, user decision 2026-10-27: c = 300 permille, κ = 1000 permille from 17,000, catalog edition 0 empty (classes
    // the genesis assembly registered are 1.0 as a chain fact). Gate C needs it at or below.
    // ADR-0177 model allocation with the lane ACT2 estimate policy (derived from user decisions; needs the bond budget at or below),
    // at or below the model tier (c_m is applied once, in the allocation weight).
    PALW_T12_G14_MODEL_BOND_ALLOCATION_ENTRY,
    PALW_T12_G14_MODEL_TIER_ENTRY,
    // `palw_settlement_once_v1` (RFC08-2) is NOT here (lane ACT2 recommendation (ii), 2026-10-27): its open blocker
    // (`PALW_RFC0608_OPEN_BLOCKERS_V1`, unsourced liability terms) is a safety refusal no waiver lifts, and armed it would refuse
    // native mining receipts and other legs with no signed deadline. No entry of this list needs it at or below; each pay point keeps
    // its own once-guard (escrow row removal / phase / paid flag), as testnet-12 runs today.
    PALW_T12_FULL_GATE_C_ENTRY,
    // Slot for lane ROLES: the wide128 test-class root pinned as "public attest" (a constant ROLES adds here, before route-required).
    // `palw_fp_tokenizer_match` (FpTokenizer) is NOT here: testnet-12 already runs it from DAA 5,300; re-arming it at 17,000
    // would move a live fence below 17,000. The FpTokenizer family's other fences are above.
    // PanelDa (user 2026-10-27, added to scope): `palw_panel_da` is already in force on testnet-12 (RC DA court fence); the
    // disclosure needs it and `palw_court_clock_v2` at or below.
    PALW_T12_G14_PANEL_DA_DISCLOSURE_ENTRY,
    PALW_T12_FULL_G14_ROUTE_REQUIRED_ENTRY,
];

palw_full_activation_bare_entry_v1!(PALW_T12_G14_PANEL_DA_DISCLOSURE_ENTRY, palw_panel_da_disclosure_v1);

/// The TIR public frame's cost model (not a fence: a value the frame reads) — the lane VAL estimate, user-approved as a value
/// 2026-10-26; its record (`PALW_TIR_FRAME_COST_RECORDS_V1`) waives the measurement. `None` clears it.
pub const PALW_T12_G14_TIR_FRAME_COST_ESTIMATE_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_tir_frame_cost_v1",
    set: |params, at| {
        params.palw_tir_frame_cost_v1 = at.map(|_| crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1);
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_model_bond_allocation_v1` with the lane ACT2 estimate policy (`palw_model_bond_allocation_estimate_v1`). `None` clears it.
pub const PALW_T12_G14_MODEL_BOND_ALLOCATION_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_model_bond_allocation_v1",
    set: |params, at| {
        params.palw_model_bond_allocation_v1 = at.map(|activation| crate::palw_bond_budget_v1::PalwModelBondAllocationFenceV1 {
            activation,
            policy: crate::palw_bond_budget_v1::palw_model_bond_allocation_estimate_v1(),
        });
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// The user-decided registered-tier cap κ (permille) from DAA 17,000 on testnet-12 (user 2026-10-27).
pub const PALW_T12_G14_MODEL_TIER_CAP_PERMILLE_V1: u32 = 1_000;
/// The registered coefficient c (permille), frozen premise (PRINCIPLES §9).
pub const PALW_T12_G14_MODEL_TIER_REGISTERED_PERMILLE_V1: u32 = 300;

/// `palw_model_tier_v1` with the user-decided values: c 300‰, κ 1000‰, catalog edition 0 empty. `None` sets it dormant.
pub const PALW_T12_G14_MODEL_TIER_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_model_tier_v1",
    set: |params, at| {
        params.palw_model_tier_v1 = at.map(|at| crate::palw_model_tier_v1::PalwModelTierFenceV1 {
            activation: at,
            catalogs: crate::palw_annual_catalog_v1::PalwAnnualCatalogChainV1::genesis_only(Vec::new()),
            registered_permille: PALW_T12_G14_MODEL_TIER_REGISTERED_PERMILLE_V1,
            registered_cap_permille: PALW_T12_G14_MODEL_TIER_CAP_PERMILLE_V1,
        });
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_court_capacity_v1` with the user-approved V11 estimate policy (`PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1`, approved
/// 2026-10-26) and no measurement record; validation accepts it only with a `UserDecision` waiver in the record's measured slot.
pub const PALW_T12_G14_COURT_CAPACITY_ESTIMATE_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_court_capacity_v1",
    set: |params, at| {
        params.palw_court_capacity_v1 = at.map(|activation| crate::palw_court_capacity_v1::PalwCourtCapacityFenceV1 {
            activation,
            policy: crate::palw_g14_values_v1::PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1,
            measurement: None,
        });
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// The height [`PALW_T12_G14_17000_FENCES_V1`] is meant for — read by tests and the arming report only, NEVER by the preset.
pub const PALW_T12_G14_17000_DAA_V1: u64 = 16_600; // the value is 16,600 (user moved the flag day from 17,000, 2026-10-27); the name is kept

/// **One list at `at`** (`None`: dormant, set in reverse) — the body shared by the full list and the slice, for drills and tests.
pub fn palw_t12_arm_fence_list_at_v1(params: &mut Params, list: &[PalwPostLaunchFenceV1], at: Option<u64>) {
    match at {
        Some(at) => list.iter().for_each(|fence| (fence.set)(params, Some(ForkActivation::new(at)))),
        None => list.iter().rev().for_each(|fence| (fence.set)(params, None)),
    }
}

/// **testnet-12 as its int-13 build ships it** — [`palw_t12_shipped_params`] with the full-activation list set back to dormant: the
/// baseline the full-activation flag day is judged against (the fork-id comparison, the "only this list moved it" twin). While both the
/// int-13 height and this list's height are `None`, it equals [`palw_t12_release_v6_params`] (int-12, `5ee7fd8e…`), byte for byte.
pub fn palw_t12_release_v7_params() -> Params {
    let mut params = palw_t12_shipped_params();
    // The int-13 entries stay at PALW_T12_INT13_DAA (15,500): this baseline is the public fleet's ruleset.
    palw_t12_arm_after_int13_at_v1(&mut params, PALW_T12_FULL_ACTIVATION_FENCES_V1, None);
    // The G14 17,000 list too (its value entries — allocation, frame cost — are not on the full list).
    palw_t12_arm_after_int13_at_v1(&mut params, PALW_T12_G14_17000_FENCES_V1, None);
    params
}
