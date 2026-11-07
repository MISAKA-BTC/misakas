//! **ADR-0176 amendment — ユーザー判断 2026-10-15: bond式の再設計** (`palw_bond_formula_v2`, dormant), on its pure functions and the
//! budget engine.
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS.** Every value is a TEST value, never a
//! production one.
//!
//! | test | required by the ruling |
//! |---|---|
//! | `rotation_by_early_valid_is_blocked` | "rotation via early Valid blocked" (items 1c, 2) |
//! | `the_decomposition_is_consistent` | "decomposition consistency" (item 3) |
//! | `the_reservation_is_never_below_the_collectable_loss_at_any_rho` | "reservation never below collectable loss"; loss never shrinks with ρ (item 4) |
//! | `kernel_escrow_is_separate_from_the_producer_liability` | "kernel escrow separation" (item 6) |
//! | `each_claim_type_is_priced_separately` | "per-type pricing" (item 6) |
//! | `the_concurrent_check_and_the_period_caps_both_bind` | "u=0.5 concurrent check with period caps both enforced" (items 1a, 1b) |
//! | `a_model_class_takes_the_reduced_reservation_only_with_a_measured_capacity_record` | item 5 |
//! | `the_fence_is_dormant_hashed_some_only_and_refused` | dormant fence, A-2 (no live change) |
//!
//! Run (validation phase): `cargo test -p kaspa-consensus-core --test adr0176_bond_formula_v2`

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_bond_budget_v1::*;
use kaspa_consensus_core::palw_bond_formula_v2::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

const KAS: u64 = 100_000_000;
const DAY: u64 = PALW_BOND_HOLD_DAA_PER_DAY_V1;

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes([n; 64]), 0))
}

fn id(n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[..8].copy_from_slice(&n.to_le_bytes());
    Hash64::from_bytes(b)
}

/// TEST budget policy (V3, slicing rights): per 1,000 BILI per day — 30 claims at ρ, 30 blocks, 300 BILI, 30,000 weight.
fn budget(rho: u32) -> PalwBondBudgetPolicyV1 {
    let mut p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    p.capital_unit_sompi = 1_000 * KAS;
    p.rho = rho;
    p.claims_per_unit = 30;
    p.block_units_per_unit = 30 * PALW_BUDGET_BLOCK_UNIT_V1;
    p.reward_per_unit_sompi = 300 * KAS;
    p.final_weight_per_unit = 30_000;
    p.max_open_claims_per_bond = 1_000_000;
    p.slice_rights_by_rho = true;
    p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 };
    p.hold.as_mut().unwrap().concurrent_liability_permille = 500;
    p.validate().expect("a well-formed V3 test budget");
    p
}

/// TEST formula policy: v_F = 1,000 sompi per weight; per type a distinct floor, court and retention cost.
fn formula() -> PalwBondFormulaPolicyV2 {
    let price = |floor_kas: u64, court_kas: u64, retention_kas: u64, kappa: u32| PalwClaimTypePricingV2 {
        loss_floor_sompi: floor_kas * KAS,
        loss_multiplier_permille: kappa,
        court_cost_sompi: court_kas * KAS,
        da_retention_cost_sompi: retention_kas * KAS,
        retention_hold_daa: 2 * DAY,
    };
    let mut f = PalwBondFormulaPolicyV2::unapproved_probe_v2();
    f.weight_value_sompi_per_weight = 1_000;
    f.attempt = price(20, 5, 2, 1_500);
    f.free_prompt = price(8, 3, 1, 1_200);
    f.kernel_panel_one = price(15, 5, 3, 1_500);
    f.kernel_panel_zero = price(40, 10, 6, 2_000);
    f.native_funded = price(25, 8, 4, 1_500);
    f.validate().expect("a well-formed test formula");
    f
}

fn input(t: PalwClaimTypeV2, reward_kas: u64, reduced: bool) -> PalwBondFormulaInputV2 {
    PalwBondFormulaInputV2 {
        claim_type: t,
        accepted_daa: 1_000,
        reward_ask_sompi: reward_kas * KAS,
        final_weight_ask: u128::MAX,
        block_units_ask: PALW_BUDGET_BLOCK_UNIT_V1,
        pwu_exposure_sompi: 0,
        reduction_allowed: reduced,
    }
}

#[test]
fn rotation_by_early_valid_is_blocked() {
    // A bond fills its Panel=1 day; colluding signers make every claim Valid → Final at once (the budget closes them). Under the
    // formula: capacity returns only at d + W_day (never at the licence / Final), and each closed claim's liability stays counted in
    // the concurrent check until its liability ends — the same collateral cannot back a rotated claim.
    let b = budget(1);
    let capital = 1_000 * KAS; // daily 30 claims; φ = 500 ‰ → 500 BILI of concurrent liability
    let mut s = PalwBondBudgetStateV1::new(0, &b);
    let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: KAS, final_weight: 1, liability_sompi: (100 * KAS) as u128 };
    let mut j = Vec::new();
    s.release_due(10, &mut j);
    for n in 0..5 {
        s.reserve_claim_moded_v1(
            &b,
            capital,
            id(n),
            bond(1),
            None,
            10,
            ask,
            PalwBudgetOriginV1::Attempt,
            PalwHoldModeV1::PanelOne,
            &mut j,
        )
        .expect("five claims of 100 BILI fill φ·C");
    }
    assert_eq!(
        s.preview_claim_moded_v1(&b, capital, bond(1), None, ask, PalwHoldModeV1::PanelOne),
        Err(PalwBudgetRefusalV1::LiabilityExceedsCapital)
    );
    // Colluding immediate Valid → Final at DAA 11: every claim closes, its liability kept until 11 + H_L.
    let until = 11 + b.liability_hold_daa;
    for n in 0..5 {
        s.close_keeping_liability_v1(&id(n), until, &mut j);
    }
    assert_eq!(s.kept_liability_v1(&bond(1)), (500 * KAS) as u128);
    for now in 12..until {
        s.release_due(now, &mut j);
        assert_eq!(
            s.preview_claim_moded_v1(&b, capital, bond(1), None, ask, PalwHoldModeV1::PanelOne),
            Err(PalwBudgetRefusalV1::LiabilityExceedsCapital),
            "no rotation of the same collateral at {now}"
        );
    }
    s.release_due(until, &mut j);
    assert_eq!(s.kept_liability_v1(&bond(1)), 0, "the liability ends at its own clock");
    s.preview_claim_moded_v1(&b, capital, bond(1), None, ask, PalwHoldModeV1::PanelOne).expect("collateral free again");
    // The capacity of the five is still in the day view until 10 + DAY (independent of the liability clock).
    assert_eq!(s.hold_view_v1(PalwHoldViewV1::Day, &bond(1)).claims, 5);
    s.check_consistency().expect("consistent");
}

#[test]
fn the_decomposition_is_consistent() {
    let b = budget(100);
    let f = formula();
    for t in [
        PalwClaimTypeV2::Attempt,
        PalwClaimTypeV2::FreePrompt,
        PalwClaimTypeV2::KernelPanelOne,
        PalwClaimTypeV2::KernelPanelZero,
        PalwClaimTypeV2::NativeFunded,
    ] {
        for reward in [0u64, 1, 3_200, 100_000] {
            let mut i = input(t, reward, true);
            i.pwu_exposure_sompi = (25 * KAS) as u128;
            let s = palw_bond_formula_split_v2(&f, &b, &i).unwrap();
            assert_eq!(s.total(), s.weight_sompi + s.right_sompi + s.penalty_sompi + s.other_sompi);
            assert_eq!(s.total(), s.weight_sompi + s.escrow_term());
            assert_eq!(s.weight_sompi, palw_bond_formula_weight_part_v2(&f, &b, i.pwu_exposure_sompi), "the ledger's weight hook");
            assert_eq!(s.capacity_recovery_daa, 1_000 + b.hold.unwrap().window_daa(t.mode()), "fixed at acceptance");
            assert_eq!(s.mode, t.mode());
            let json = s.audit_json();
            for key in [
                "\"weight\"",
                "\"right\"",
                "\"penalty\"",
                "\"other\"",
                "\"total\"",
                "\"weightHold\"",
                "\"otherHold\"",
                "\"capacityRecoveryDaa\"",
            ] {
                assert!(json.contains(key), "{key} in {json}");
            }
            assert!(
                matches!(s.other_hold, PalwComponentHoldV2::RetentionAndCourtClosed { not_before_daa } if not_before_daa == 1_000 + 2 * DAY)
            );
        }
    }
}

#[test]
fn the_reservation_is_never_below_the_collectable_loss_at_any_rho() {
    let f = formula();
    let mut floors = Vec::new();
    for rho in [1u32, 10, 100, 1_000, 10_000] {
        let b = budget(rho);
        for t in [PalwClaimTypeV2::Attempt, PalwClaimTypeV2::KernelPanelZero, PalwClaimTypeV2::FreePrompt] {
            let mut i = input(t, 3_200, true);
            i.pwu_exposure_sompi = (24 * KAS) as u128;
            let s = palw_bond_formula_split_v2(&f, &b, &i).unwrap();
            assert!(s.total() >= s.collectable_loss_sompi, "{t:?} ρ={rho}: total below the collectable loss");
            assert!(s.collectable_loss_sompi >= s.gain_sompi, "{t:?} ρ={rho}: a smaller reservation must never mean a larger gain");
            assert!(s.collectable_loss_sompi >= f.pricing(t).loss_floor_sompi as u128, "the floor never shrinks with ρ");
            assert!(s.weight_sompi >= (24 * KAS) as u128, "the PWU slash exposure is never divided by ρ");
            if t == PalwClaimTypeV2::Attempt {
                // The weight VALUE `⌈f_c · v_F⌉` is the part of the gain beyond the reward right.
                floors.push((rho, s.total(), s.rights.reward_sompi, s.gain_sompi - s.rights.reward_sompi as u128));
            }
        }
    }
    // Rights shrink with ρ (the period total split over more claims); the reservation never falls below the floor — never "the same
    // reward at 1/1000 of the reservation". It does not fall with ρ either: per the formula (`L_penalty = L_coll − (L_right +
    // ⌈f_c·v_F⌉)`, the PWU part of `L_weight` never divided by ρ) only the weight VALUE offsets the collectable loss, so when the PWU
    // slash exposure dominates `L_weight` and `L_coll` sits at its floor, the reservation grows by exactly the weight value ρ took away.
    for w in floors.windows(2) {
        let ((_, t0, r0, v0), (_, t1, r1, v1)) = (w[0], w[1]);
        assert!(r1 <= r0 && v1 <= v0, "rights by rho: {floors:?}");
        assert!(t1 <= t0 + (v0 - v1), "the reservation grows at most by the weight value no longer offsetting the loss: {floors:?}");
        if r1 < r0 {
            assert!(t1 >= f.attempt.loss_floor_sompi as u128);
        }
    }
}

#[test]
fn kernel_escrow_is_separate_from_the_producer_liability() {
    let b = budget(100);
    let f = formula();
    for t in [PalwClaimTypeV2::KernelPanelZero, PalwClaimTypeV2::KernelPanelOne, PalwClaimTypeV2::NativeFunded] {
        let s = palw_bond_formula_split_v2(&f, &b, &input(t, 500, false)).unwrap();
        assert_eq!(s.right_sompi, 0, "{t:?}: the user's escrow is not producer collateral");
        assert_eq!(s.penalty_sompi, s.collectable_loss_sompi, "{t:?}: the whole collectable loss is the producer's own");
        let accounts = palw_kernel_accounting_v2(&s, 500 * KAS);
        assert_eq!(accounts.user_escrow_sompi, 500 * KAS, "the user's escrow is never clipped or reduced");
        assert!(accounts.producer_liability_sompi >= (f.pricing(t).loss_floor_sompi as u128), "prepayment never removes it");
        assert_eq!(accounts.producer_period_reward_sompi, 500 * KAS, "the producer's R right is the job's reward (unreduced)");
    }
}

#[test]
fn each_claim_type_is_priced_separately() {
    let b = budget(1);
    let f = formula();
    let totals: Vec<u128> = [
        PalwClaimTypeV2::Attempt,
        PalwClaimTypeV2::FreePrompt,
        PalwClaimTypeV2::KernelPanelOne,
        PalwClaimTypeV2::KernelPanelZero,
        PalwClaimTypeV2::NativeFunded,
    ]
    .into_iter()
    .map(|t| palw_bond_formula_split_v2(&f, &b, &input(t, 0, false)).unwrap().total())
    .collect();
    let mut unique = totals.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), totals.len(), "no common price across types: {totals:?}");
    // The own-work Attempt quote (~3,200 BILI of escrow right) is the Attempt's alone.
    let attempt = palw_bond_formula_split_v2(&f, &b, &input(PalwClaimTypeV2::Attempt, 3_200, false)).unwrap();
    let kernel = palw_bond_formula_split_v2(&f, &b, &input(PalwClaimTypeV2::KernelPanelZero, 0, false)).unwrap();
    assert!(attempt.right_sompi == (3_200 * KAS) as u128 && kernel.right_sompi == 0);
}

#[test]
fn the_concurrent_check_and_the_period_caps_both_bind() {
    let f = formula();
    let capital = 13_000 * KAS;
    // (a) u = 500 ‰: open + locks + accusation reserves + outstanding + new ≤ 6,500 BILI — each ledger counted once.
    assert!(palw_bond_formula_concurrent_admits_v2(
        &f,
        capital,
        3_000 * KAS as u128,
        2_000 * KAS as u128,
        1_000 * KAS as u128,
        0,
        500 * KAS as u128
    ));
    assert!(!palw_bond_formula_concurrent_admits_v2(
        &f,
        capital,
        3_000 * KAS as u128,
        2_000 * KAS as u128,
        1_000 * KAS as u128,
        1,
        500 * KAS as u128
    ));
    // (b) the period caps bind with collateral to spare: tiny reservations, the day view refuses at the daily count.
    let b = budget(1);
    let small = PalwBudgetAskV1 { block_units: 0, reward_sompi: 1, final_weight: 1, liability_sompi: 1 };
    let mut s = PalwBondBudgetStateV1::new(0, &b);
    let mut j = Vec::new();
    let daily = palw_bond_hold_daily_caps_v1(&b, capital).claims;
    let mut admitted = 0;
    for n in 0..10 * daily {
        let now = 1 + n; // one per DAA: the burst view spreads them, the day view caps them
        s.release_due(now, &mut j);
        if s.reserve_claim_moded_v1(
            &b,
            capital,
            id(n),
            bond(2),
            None,
            now,
            small,
            PalwBudgetOriginV1::Attempt,
            PalwHoldModeV1::PanelOne,
            &mut j,
        )
        .is_ok()
        {
            admitted += 1;
        }
        if now >= DAY {
            break;
        }
    }
    assert!(admitted <= daily, "the period cap binds although collateral is ample: {admitted} > {daily}");
}

#[test]
fn a_model_class_takes_the_reduced_reservation_only_with_a_measured_capacity_record() {
    let base = Hash64::from_bytes([1; 64]);
    let model = Hash64::from_bytes([2; 64]);
    assert!(palw_bond_formula_reduction_allowed_v2(&base, &base), "the Floor class's audit is the CPU replay");
    assert!(!palw_bond_formula_reduction_allowed_v2(&model, &base), "no measured record is pinned: a model class is not reduced");
    assert!(PALW_BOND_FORMULA_MEASURED_CAPACITY_PINNED_V2.is_empty());
    let b = budget(1_000);
    let f = formula();
    let reduced = palw_bond_formula_split_v2(&f, &b, &input(PalwClaimTypeV2::Attempt, 3_200, true)).unwrap();
    let full = palw_bond_formula_split_v2(&f, &b, &input(PalwClaimTypeV2::Attempt, 3_200, false)).unwrap();
    assert!(reduced.total() < full.total());
    assert_eq!(full.right_sompi, (3_200 * KAS) as u128, "unmeasured: the whole reward right is reserved");
    assert!(reduced.rights.reward_sompi < 3_200 * KAS, "measured: the right itself is split, not only the reservation");
}

#[test]
fn the_fence_is_dormant_hashed_some_only_and_refused() {
    let p =
        Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12));
    assert!(p.palw_bond_formula_v2.is_none());
    assert!(!p.palw_bond_formula_v2_active_at(u64::MAX));
    assert!(p.validate_palw_bond_formula_v2().is_ok());
    let mut armed = p.clone();
    armed.palw_bond_formula_v2 = Some(PalwBondFormulaFenceV2::at(ForkActivation::new(9_000_000)));
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id(), "Some-only: hashed when present");
    assert!(armed.validate_palw_bond_formula_v2().is_err(), "refused when armed");
    let mut never = p.clone();
    never.palw_bond_formula_v2 = Some(PalwBondFormulaFenceV2::at(ForkActivation::never()));
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id(), "never() collapses whole");
}
