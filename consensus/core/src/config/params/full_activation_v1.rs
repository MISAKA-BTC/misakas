//! **testnet-12's full-activation flag day** (lane g14p/r7-fence-16000, 2026-10-22) — ONE list, [`PALW_T12_FULL_ACTIVATION_FENCES_V1`],
//! that arms every G14 / RFC fence of the full-activation release at ONE height, [`PALW_T12_FULL_ACTIVATION_DAA`].
//! The user set that height to DAA 15,400 on 2026-10-25, after 15,100 had already passed on the unarmed binary. The arming-record lists stay empty: this file does not invent evidence digests.
//! `validate_palw_v2` accepts the public testnet-12 preset at exactly this height and still refuses every other height and genesis.
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
            (Some(at), None) => Some(crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(at)),
        };
        palw_full_activation_sync_mirrors_v1(params);
    },
};

/// `palw_model_tier_v1` (ADR-0179 / RFC-0019): value-carrying. `Some(at)` re-times installed tiers/catalogs; with none it
/// installs the design coefficients and RFC-0019's catalog: edition 0 empty, edition 1 at the same height adding Dolphin
/// ([`crate::palw_genesis8_catalog_v1::palw_t12_dolphin3_soft_fork_catalog_v1`]). The class is not registered. The edition
/// names it, and it joins `ApprovedModels` at this height. Arming records stay empty, so validation still refuses the fence
/// except through testnet-12's full-activation waiver at [`PALW_T12_FULL_ACTIVATION_DAA`].
pub const PALW_T12_FULL_MODEL_TIER_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_model_tier_v1",
    set: |params, at| {
        params.palw_model_tier_v1 = match (at, params.palw_model_tier_v1.clone()) {
            (None, _) => None,
            (Some(at), Some(mut installed)) => {
                installed.activation = at;
                Some(installed)
            }
            (Some(at), None) => {
                let mut fence = crate::palw_model_tier_v1::PalwModelTierFenceV1::at(at);
                fence.catalogs = crate::palw_genesis8_catalog_v1::palw_t12_dolphin3_soft_fork_catalog_v1(at.daa_score());
                Some(fence)
            }
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

/// **The full-activation list's height: DAA 15,400.** User order 2026-10-25, moved forward from 15,100 after the public tip had
/// already passed 15,100 on the unarmed binary. A height no other list uses (not 750, 1,000, 1,300, 1,700, 2,000, 3,600, 5,300,
/// 5,395, 5,490, 5,585, and not 16,000). The arming-record lists stay empty — G14 arm-readiness, TIR frame-cost, RFC-0608
/// measured records, and the X3 / court-capacity / monetary-material / model-tier evidence digests are not invented here. The policies
/// installed are the ones each entry's `set` already installs when no reviewed policy is present. `validate_palw_v2` accepts that exact
/// public testnet-12 preset and refuses the same list at any other height or on any other genesis. The same height is
/// `contrib/t12-deploy-kit/fleet.env.example`'s `FULL_ACTIVATION_FLAG_DAY_DAA`. One constant the preset, the drill flag
/// `--palw-drill-full-activation-at` and the kit share.
///
/// The fleet must be on this binary before the live tip reaches 15,400. A fence already behind the tip is a fired-fence mismatch.
pub const PALW_T12_FULL_ACTIVATION_DAA: Option<u64> = Some(15_400);

/// The intended height, for tests and the notes only — NEVER read by the preset.
pub const PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1: u64 = 16_000;

/// **The full-activation flag day, armed** — the list at [`PALW_T12_FULL_ACTIVATION_DAA`] on the ASSEMBLED ruleset, after the int-13
/// flag day; a no-op while the height is `None` (it leaves the list exactly as the earlier lists set it — in particular it does not clear
/// the int-13 entries this list shares, should the int-13 list ever take a height of its own).
pub(super) fn palw_t12_arm_full_activation_v1(params: &mut Params) {
    let Some(at) = PALW_T12_FULL_ACTIVATION_DAA else { return };
    palw_t12_arm_full_activation_at_v1(params, Some(at));
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

/// **testnet-12 as its int-13 build ships it** — [`palw_t12_shipped_params`] with the full-activation list set back to dormant: the
/// baseline the full-activation flag day is judged against (the fork-id comparison, the "only this list moved it" twin). While both the
/// int-13 height and this list's height are `None`, it equals [`palw_t12_release_v6_params`] (int-12, `5ee7fd8e…`), byte for byte.
pub fn palw_t12_release_v7_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_full_activation_at_v1(&mut params, None);
    params
}
