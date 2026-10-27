//! ADR-0179 (user decisions 2026-10-15, rounds 4–5) — the model tiers over the pure rules and the budget engine's allocation roll.
//! **Unverified source: written, not built and not run — no result here is a PASS.** Every value is a TEST or design value.
//!
//! | test | requirement |
//! |---|---|
//! | `the_fence_is_dormant_some_only_and_refused_when_armed` | dormant fence, A-2, refused when armed |
//! | `a_registered_model_is_0_3_from_registration_with_no_review_and_forever` | 0.3 at once with no review; still 0.3 after any time; no registry state is an input |
//! | `only_an_annual_fork_listing_makes_1_0` | the only route to 1.0 |
//! | `an_annual_list_only_adds_and_never_re_roots_a_model` | add-only (ADR-0175) |
//! | `c_m_is_applied_once_never_0_09` | `A_m = c_m · S̄^α` once; a claim's reward is never × 0.3 again |
//! | `n_registrations_never_raise_the_registered_tier_total` | the registered tier's aggregate cap |
//! | `the_tiered_roll_is_the_same_after_reorg_ibd_and_restart` | reorg / IBD / restart consistency |
//! | `p4_registered_capital_cannot_dilute_the_approved_tier_beyond_its_scaled_weight` | ΣA of the scaled weights (2026-10-22) |
//! | `p4_canonical_only_and_unbound_allocations_are_unchanged` | no change where κ does not bind |
//! | `p4_registered_models_receive_exactly_their_scaled_share` | each registered row is c·S̄^α scaled by κ's factor |
//!
//! Gate C for every tier: `palw_rfc0018_v1/tests.rs` `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt`.

use std::collections::BTreeMap;

use super::*;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
use crate::palw_annual_catalog_v1::ApprovedModelV1;
use crate::network::{NetworkId, NetworkType};
use crate::palw_bond_budget_v1::*;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::tx::TransactionOutpoint;

const FENCE: u64 = 10_000;
const KAS: u64 = 100_000_000;
const YEAR_DAA: u64 = 365 * 720;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x7135_0000 + n)
}

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
}

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn fence() -> PalwModelTierFenceV1 {
    PalwModelTierFenceV1::at(ForkActivation::new(FENCE))
}

#[test]
fn the_fence_is_dormant_some_only_and_refused_when_armed() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, t12()] {
        assert_eq!(p.palw_model_tier_v1, None, "None on every preset");
        assert_eq!(p.palw_model_tier_v1_from_daa(), None);
        p.validate_palw_model_tier_v1().unwrap();
        assert!(p.palw_fences_v1().contains(&("palw_model_tier_v1", None)), "the exhaustive fence list names it");
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert!(bundle.state.model_tier_v1().is_none(), "every shipped mirror is None");
        }
    }
    let mut p = t12();
    p.palw_model_tier_v1 = Some(PalwModelTierFenceV1::at(ForkActivation::never()));
    p.validate_palw_model_tier_v1().unwrap();
    assert_eq!(p.palw_model_tier_v1_from_daa(), None, "never() is not a height");
    p.palw_model_tier_v1 = Some(fence());
    p.sync_palw_model_tier_v1();
    assert!(p.validate_palw_model_tier_v1().is_err(), "refused when armed: design values, no measured record");
    // A-2: below the fence every model weighs 1.0.
    let mirror = fence().mirror();
    assert_eq!(palw_model_tier_coefficient_permille_v1(Some(&mirror), PalwModelTierV1::Registered, FENCE - 1), 1_000);
    assert_eq!(palw_model_tier_coefficient_permille_v1(None, PalwModelTierV1::Registered, u64::MAX), 1_000);
}

/// Round 5: a registered model is 0.3 from its registration — no exam, probation or renewal — and stays 0.3 however long it runs;
/// no review outcome and no registry state (`Active` included) is an input of the tier.
#[test]
fn a_registered_model_is_0_3_from_registration_with_no_review_and_forever() {
    let mirror = fence().mirror();
    for daa in [FENCE, FENCE + 1, FENCE + YEAR_DAA, FENCE + 50 * YEAR_DAA, u64::MAX] {
        let tier = palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, daa);
        assert_eq!(tier, PalwModelTierV1::Registered, "at {daa}");
        assert_eq!(palw_model_tier_coefficient_permille_v1(Some(&mirror), tier, daa), PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1);
    }
    // A genesis-assembly class is approved without a list.
    assert_eq!(
        palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), true, FENCE),
        PalwModelTierV1::Approved(PalwTierSourceV1::GenesisAssembly)
    );
}

/// RFC-0019: `ApprovedModels` is the annual catalog chain — a fork listing is catalog edition `e ≥ 1`.
#[test]
fn only_an_annual_fork_listing_makes_1_0() {
    let mut f = fence();
    let fork_at = FENCE + YEAR_DAA;
    f.catalogs = f.catalogs.clone().with_edition(fork_at, vec![ApprovedModelV1::of(h(1), h(2))]);
    f.validate().unwrap();
    let mirror = f.mirror();
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, fork_at - 1), PalwModelTierV1::Registered);
    let listed = palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, fork_at);
    assert_eq!(listed, PalwModelTierV1::Approved(PalwTierSourceV1::AnnualFork { edition: 1, from_daa: fork_at }));
    assert_eq!(palw_model_tier_coefficient_permille_v1(Some(&mirror), listed, fork_at), 1_000);
    // Another model is untouched by the listing.
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(3), &h(4), false, fork_at), PalwModelTierV1::Registered);
}

#[test]
fn an_annual_list_only_adds_and_never_re_roots_a_model() {
    let entry = |id: u64, root: u64| ApprovedModelV1::of(h(id), h(root));
    let mut f = fence();
    f.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![entry(1, 2)]).with_edition(FENCE + 10, vec![entry(3, 4)]);
    f.validate().unwrap();
    // Re-listing a model id — with its own root or another — is refused: an edition never overwrites or re-roots.
    for again in [entry(1, 2), entry(1, 9), entry(3, 9)] {
        let mut g = f.clone();
        g.catalogs = g.catalogs.clone().with_edition(FENCE + 20, vec![again]);
        assert!(g.validate().is_err(), "{again:?}");
    }
    // Out-of-order and empty editions are refused.
    let mut late = f.clone();
    late.catalogs = late.catalogs.clone().with_edition(FENCE + 5, vec![entry(5, 6)]);
    assert!(late.validate().is_err());
    let mut empty = f.clone();
    empty.catalogs = empty.catalogs.clone().with_edition(FENCE + 30, Vec::new());
    assert!(empty.validate().is_err());
    // An edition below the tier fence is refused.
    let mut early = fence();
    early.catalogs = early.catalogs.clone().with_edition(FENCE - 1, vec![entry(7, 8)]);
    assert!(early.validate().is_err());
    // Membership needs the class's own root: an entry naming another root never approves (or replaces) the registered class.
    let mirror = f.mirror();
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(3), &h(99), false, FENCE + 10), PalwModelTierV1::Registered);
    // The genesis catalog (edition 0) approves from the fence.
    assert_eq!(
        palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, FENCE),
        PalwModelTierV1::Approved(PalwTierSourceV1::GenesisList)
    );
}

fn allocation_world(registered: &[Hash64], approved: Hash64) -> (PalwModelAllocationPolicyV1, PalwBondBudgetStateV1) {
    let ap = PalwModelAllocationPolicyV1::decided_unapproved_v3();
    let mut s = PalwBondBudgetStateV1::new(0, &PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1());
    let mut j = Vec::new();
    s.assign(&ap, bond(0), &[(approved, 1_000 * KAS)], 1, 1_000 * KAS, 0, |_| true, &mut j).unwrap();
    for (i, m) in registered.iter().enumerate() {
        s.assign(&ap, bond(1 + i as u8), &[(*m, 1_000 * KAS)], 1, 1_000 * KAS, 0, |_| true, &mut j).unwrap();
    }
    (ap, s)
}

fn roll(
    ap: &PalwModelAllocationPolicyV1,
    s: &mut PalwBondBudgetStateV1,
    approved: Hash64,
    cap: Option<u32>,
    epochs: std::ops::Range<u64>,
) {
    let empty = BTreeMap::new();
    let mut j = Vec::new();
    for epoch in epochs {
        s.roll_epoch_v3_tiered(
            ap,
            epoch,
            epoch * ap.epoch_daa,
            |_| (1_000 * KAS, true),
            &empty,
            |_, c| c,
            |m: &Hash64| {
                if *m == approved {
                    (false, PALW_MODEL_TIER_APPROVED_PERMILLE_V1)
                } else {
                    (true, PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1)
                }
            },
            cap,
            &mut j,
        );
    }
}

/// `A_m = c_m · S̄_m^α`, ONCE: a registered model's weight is exactly 0.3 of an equally capitalised approved model's (never 0.09), its
/// coinbase share follows, and its accepted claim reserves its whole ask — the reward is never multiplied by the tier again.
#[test]
fn c_m_is_applied_once_never_0_09() {
    let approved = h(10);
    let registered = h(11);
    let (ap, mut s) = allocation_world(&[registered], approved);
    roll(&ap, &mut s, approved, None, 0..3);
    let w = s.model_row(&approved).unwrap().weight;
    let wr = s.model_row(&registered).unwrap().weight;
    assert_eq!(wr, palw_model_tier_weight_v1(w, 300), "0.3 · w");
    assert_ne!(wr, palw_model_tier_weight_v1(palw_model_tier_weight_v1(w, 300), 300), "never 0.09 · w");
    let mut j = Vec::new();
    s.accrue(1_300 * KAS, &mut j);
    let (a, r) = (s.model_available(&approved), s.model_available(&registered));
    assert!(a > 0 && r > 0);
    assert!((r as u128) * 1_000 / (a as u128) >= 299 && (r as u128) * 1_000 / (a as u128) <= 300, "{r} / {a} ≈ 0.3");
    // The registered model's claim reserves its whole ask (inside its P_m budget): not ask × 0.3.
    let p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: r / 2, final_weight: 0, liability_sompi: 0 };
    let start = 2 * ap.epoch_daa + 1;
    s.release_due(start, &mut j);
    let granted = s
        .reserve_claim_v1(&p, 1_000_000 * KAS, h(500), bond(1), Some(registered), start, ask, PalwBudgetOriginV1::Attempt, &mut j)
        .unwrap();
    assert_eq!(granted.reward_sompi, r / 2, "the block's reward is the claim's own; the tier was applied once, in A_m");
}

/// N registrations of equally capitalised 0.3 models never take more than the registered tier's cap κ of the allocation, however
/// large N is; below the cap nothing changes, and the approved tier keeps at least `1 − κ` (ΣA of the scaled weights, 2026-10-22).
#[test]
fn n_registrations_never_raise_the_registered_tier_total() {
    let approved = h(20);
    let cap = PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1;
    let mut last_share = 0u128;
    for n in [1u64, 10, 100] {
        let registered: Vec<Hash64> = (0..n).map(|i| h(1_000 + i)).collect();
        let (ap, mut s) = allocation_world(&registered, approved);
        roll(&ap, &mut s, approved, Some(cap), 0..3);
        let mut j = Vec::new();
        let total: u64 = 1_000_000 * KAS;
        s.accrue(total, &mut j);
        let tier_total: u128 = registered.iter().map(|m| s.model_available(m) as u128).sum();
        let share = tier_total * 1_000 / total as u128;
        assert!(share <= cap as u128, "N = {n}: the registered tier took {share} ‰ > κ = {cap} ‰");
        assert!(share >= last_share.min(cap as u128 - 1), "N = {n}: monotone up to the cap");
        last_share = share;
        // The approved model's share is its own A / ΣA whatever the cap removed (never inflated by the registered excess).
        let approved_share = s.model_available(&approved) as u128 * 1_000 / total as u128;
        assert!(approved_share + share <= 1_000);
        if n == 1 {
            // 0.3 / 1.3 ≈ 230 ‰ < κ: the cap does not bind.
            assert!((229..=231).contains(&share), "N = 1: {share}");
        }
    }
    // The cap factor itself (canonical, registered, κ): the scaled registered total is at most κ of the SCALED ΣA.
    let w = |n: u64| PalwAllocationWeightV1::from_u64(n);
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(900), w(100), 300), 1_000_000, "100 of 1000 is within κ");
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(700), w(300), 300), 1_000_000, "exactly κ: within");
    // 600 registered over 400 canonical → 400 · 0.3/0.7 = 171.43 → factor 285,714 ppm; 171 of 571 ≈ 0.3.
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(400), w(600), 300), 285_714);
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(400), w(600), 1_000), 1_000_000, "κ = 1000 never binds");
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(0), w(600), 300), 0, "no approved weight: the registered tier gets κ of nothing");
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w(400), w(0), 300), 1_000_000);
}

/// A world with explicit capital per model (one bond per model, each bond's capital = its assignment).
fn capital_world(models: &[(Hash64, u64)]) -> (PalwModelAllocationPolicyV1, PalwBondBudgetStateV1, BTreeMap<PalwBondKeyV2, u64>) {
    let ap = PalwModelAllocationPolicyV1::decided_unapproved_v3();
    let mut s = PalwBondBudgetStateV1::new(0, &PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1());
    let mut j = Vec::new();
    let mut capital = BTreeMap::new();
    for (i, (m, c)) in models.iter().enumerate() {
        s.assign(&ap, bond(i as u8), &[(*m, *c)], 1, *c, 0, |_| true, &mut j).unwrap();
        capital.insert(bond(i as u8), *c);
    }
    (ap, s, capital)
}

fn roll_capital(
    ap: &PalwModelAllocationPolicyV1,
    s: &mut PalwBondBudgetStateV1,
    capital: &BTreeMap<PalwBondKeyV2, u64>,
    approved: &[Hash64],
    cap: Option<u32>,
    epochs: std::ops::Range<u64>,
) {
    let empty = BTreeMap::new();
    let mut j = Vec::new();
    for epoch in epochs {
        s.roll_epoch_v3_tiered(
            ap,
            epoch,
            epoch * ap.epoch_daa,
            |b| (capital.get(b).copied().unwrap_or(0), true),
            &empty,
            |_, c| c,
            |m: &Hash64| {
                if approved.contains(m) {
                    (false, PALW_MODEL_TIER_APPROVED_PERMILLE_V1)
                } else {
                    (true, PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1)
                }
            },
            cap,
            &mut j,
        );
    }
}

fn shares_permille(s: &PalwBondBudgetStateV1, models: &[Hash64], total: u64) -> Vec<u128> {
    models.iter().map(|m| s.model_available(m) as u128 * 1_000 / total as u128).collect()
}

/// **The dilution counterexample (P4, 2026-10-22) no longer dilutes.** One approved model with 1,000 KAS, one registered model with
/// 100,000 KAS: before the correction the header kept the unscaled `ΣA`, so the approved model's share fell to ≈ 3 ‰ while the
/// registered model was cut to κ and the rest went unminted. With `ΣA` the sum of the scaled weights the approved model keeps
/// `1 − κ` and the registered model takes exactly κ.
#[test]
fn p4_registered_capital_cannot_dilute_the_approved_tier_beyond_its_scaled_weight() {
    let (approved, registered) = (h(40), h(41));
    let (ap, mut s, capital) = capital_world(&[(approved, 1_000 * KAS), (registered, 100_000 * KAS)]);
    let cap = PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1;
    roll_capital(&ap, &mut s, &capital, &[approved], Some(cap), 0..9); // epochs 2..=8: a full 7-epoch average
    let total: u64 = 1_000_000 * KAS;
    let mut j = Vec::new();
    s.accrue(total, &mut j);
    let sh = shares_permille(&s, &[approved, registered], total);
    assert!(sh[0] >= 699, "the approved tier keeps 1 − κ: {} ‰", sh[0]);
    assert!(sh[1] <= cap as u128 && sh[1] >= cap as u128 - 1, "the registered tier takes κ: {} ‰", sh[1]);
    // The unscaled denominator (the old rule) would have left the approved model ≈ 3 ‰: A_c / (A_c + 0.3·A_r).
    let wa = PalwAllocationCurveV1 { alpha_halves: 3 }.weight((1_000 * KAS) as u128);
    let wr = palw_model_tier_weight_v1(PalwAllocationCurveV1 { alpha_halves: 3 }.weight((100_000 * KAS) as u128), 300);
    let old = palw_model_available_v1(total, &wa, &wa.saturating_add(wr), 0) as u128 * 1_000 / total as u128;
    assert!(old <= 4, "the counterexample: {old} ‰ under the unscaled ΣA");
    s.check_consistency().expect("consistent");
}

/// Canonical-only allocation (and any allocation where κ does not bind) is unchanged: the same rows, the same header and roots
/// with the cap as without it.
#[test]
fn p4_canonical_only_and_unbound_allocations_are_unchanged() {
    let models = [(h(50), 1_000 * KAS), (h(51), 4_000 * KAS)];
    let approved = [h(50), h(51)];
    let (ap, mut capped, capital) = capital_world(&models);
    let (_, mut free, _) = capital_world(&models);
    roll_capital(&ap, &mut capped, &capital, &approved, Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 0..3);
    roll_capital(&ap, &mut free, &capital, &approved, None, 0..3);
    assert_eq!(capped.root_parts(), free.root_parts(), "canonical only: identical with or without κ");
    let sum: PalwAllocationWeightV1 =
        approved.iter().fold(PalwAllocationWeightV1::ZERO, |a, m| a.saturating_add(capped.model_row(m).unwrap().weight));
    assert_eq!(capped.header.allocation.unwrap().sum_a, sum, "ΣA is the rows' sum");
    // One registered model at 0.3 under κ (0.3 / 1.3 of an equal approved model ≈ 230 ‰ < 300 ‰): the cap does not bind, nothing moves.
    let models = [(h(52), 1_000 * KAS), (h(53), 1_000 * KAS)];
    let (ap, mut capped, capital) = capital_world(&models);
    let (_, mut free, _) = capital_world(&models);
    roll_capital(&ap, &mut capped, &capital, &[h(52)], Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 0..3);
    roll_capital(&ap, &mut free, &capital, &[h(52)], None, 0..3);
    assert_eq!(capped.root_parts(), free.root_parts(), "an unbound cap changes nothing");
}

/// Registered models still receive exactly their scaled share: each row's weight is `scale(c·S̄^α, factor)`, `ΣA` is the sum of the
/// rows, and each model's allocation is `⌊accrued · A_m / ΣA⌋`.
#[test]
fn p4_registered_models_receive_exactly_their_scaled_share() {
    let approved = h(60);
    let registered = [h(61), h(62), h(63)];
    let mut models = vec![(approved, 1_000 * KAS)];
    models.extend(registered.iter().map(|m| (*m, 3_000 * KAS)));
    let (ap, mut s, capital) = capital_world(&models);
    let cap = PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1;
    roll_capital(&ap, &mut s, &capital, &[approved], Some(cap), 0..9); // epochs 2..=8: a full 7-epoch average
    let curve = PalwAllocationCurveV1 { alpha_halves: 3 };
    let wa = curve.weight((1_000 * KAS) as u128);
    let wr = palw_model_tier_weight_v1(curve.weight((3_000 * KAS) as u128), PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1);
    let reg_sum = wr.saturating_add(wr).saturating_add(wr);
    let factor = palw_model_tier_cap_factor_ppm_v1(wa, reg_sum, cap);
    assert!(factor < 1_000_000, "the cap binds here");
    assert_eq!(s.model_row(&approved).unwrap().weight, wa, "the approved weight is never scaled");
    let scaled = palw_model_tier_scale_v1(wr, factor);
    for m in &registered {
        assert_eq!(s.model_row(m).unwrap().weight, scaled, "c·S̄^α, then the κ factor");
    }
    let sum_a = s.header.allocation.unwrap().sum_a;
    assert_eq!(sum_a, wa.saturating_add(scaled).saturating_add(scaled).saturating_add(scaled), "ΣA = Σ of the scaled rows");
    let total: u64 = 1_000_000 * KAS;
    let mut j = Vec::new();
    s.accrue(total, &mut j);
    for m in &registered {
        assert_eq!(s.model_available(m), palw_model_available_v1(total, &scaled, &sum_a, 0));
    }
    let tier: u128 = registered.iter().map(|m| s.model_available(m) as u128).sum();
    assert!(tier * 1_000 / total as u128 <= cap as u128, "the registered tier stays within κ");
}

/// The tiered roll is a pure function of the book and the tier read: a second node (IBD), a restart (the book's borsh round trip)
/// and a reorg (rolling the same epoch from the parent book again) reach the same rows and roots.
#[test]
fn the_tiered_roll_is_the_same_after_reorg_ibd_and_restart() {
    let approved = h(30);
    let registered = [h(31), h(32)];
    let (ap, mut a) = allocation_world(&registered, approved);
    let (_, mut b) = allocation_world(&registered, approved);
    roll(&ap, &mut a, approved, Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 0..2);
    roll(&ap, &mut b, approved, Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 0..2);
    assert_eq!(a.root_parts(), b.root_parts(), "IBD: another node replaying the same epochs");
    let parent = a.clone();
    roll(&ap, &mut a, approved, Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 2..3);
    let mut again = parent.clone();
    roll(&ap, &mut again, approved, Some(PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1), 2..3);
    assert_eq!(again.root_parts(), a.root_parts(), "reorg: the same epoch from the same parent");
    let bytes = borsh::to_vec(&a).unwrap();
    let back: PalwBondBudgetStateV1 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back.root_parts(), a.root_parts(), "restart: the book round-trips");
    for m in [approved, registered[0], registered[1]] {
        assert_eq!(back.model_row(&m), a.model_row(&m));
    }
}

/// **rfcgap-b (ADR-0179, RFC-0019 §16):** the fixed refusal is replaced by an arm-readiness record gate for exactly (network, height)
/// that must name the fence's own c, κ and catalog tip; the pinned list is empty, so every armed height is still refused, by name. A
/// later annual edition changes the tip root and needs its own record.
#[test]
fn rfcgap_b_tier_arm_record_gate_is_exact_and_the_pinned_list_is_empty() {
    assert!(
        PALW_MODEL_TIER_ARMING_RECORDS_V1
            .iter()
            .all(|r| r.activation_daa == 17_000 && r.genesis == crate::config::genesis::PALW_T12_GENESIS.hash)
    );
    assert_eq!(PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1.len(), 8);
    static EVIDENCE: [[u8; 32]; 8] = [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], [7; 32], [8; 32]];
    static REPEATED: [[u8; 32]; 8] = [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], [7; 32], [1; 32]];
    let mut p = t12();
    p.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(FENCE)));
    p.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1 {
        activation: ForkActivation::new(FENCE),
        policy: PalwModelAllocationPolicyV1::decided_unapproved_v3(),
    });
    p.palw_independently_work_certified_v1 = Some(ForkActivation::new(FENCE));
    p.palw_model_tier_v1 = Some(fence());
    p.sync_palw_model_tier_v1();
    let why = format!("{:?}", p.validate_palw_model_tier_v1().unwrap_err());
    assert!(why.contains("PALW_MODEL_TIER_ARMING_RECORDS_V1"), "the prerequisites hold, the record gate refuses by name: {why}");
    let tip = fence().catalogs.editions.last().cloned().unwrap();
    let record = PalwModelTierArmingRecordV1 {
        version: PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,
        genesis: p.genesis.hash,
        activation_daa: FENCE,
        registered_permille: PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1,
        registered_cap_permille: PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1,
        catalog_tip_edition: tip.edition,
        catalog_tip_root: tip.root(),
        source_commit: [9; 20],
        evidence: &EVIDENCE,
        review: [10; 32],
    };
    p.validate_palw_model_tier_with_records_v1(&[record]).unwrap();
    let with = |f: &dyn Fn(&mut PalwModelTierArmingRecordV1)| {
        let mut r = record;
        f(&mut r);
        p.validate_palw_model_tier_with_records_v1(&[r]).map_err(|e| format!("{e:?}"))
    };
    assert!(with(&|r| r.activation_daa = FENCE + 1).unwrap_err().contains("PALW_MODEL_TIER_ARMING_RECORDS_V1"), "another height");
    assert!(with(&|r| r.genesis = h(77)).unwrap_err().contains("PALW_MODEL_TIER_ARMING_RECORDS_V1"), "another network");
    assert!(with(&|r| r.registered_cap_permille = 250).unwrap_err().contains("another c or κ"));
    assert!(with(&|r| r.registered_permille = 299).unwrap_err().contains("another c or κ"));
    assert!(with(&|r| r.catalog_tip_root = h(78)).unwrap_err().contains("another catalog chain"));
    assert!(with(&|r| r.version = 2).is_err());
    assert!(with(&|r| r.source_commit = [0; 20]).is_err());
    assert!(with(&|r| r.review = [0; 32]).is_err());
    assert!(with(&|r| r.review = [3; 32]).is_err(), "review equal to an evidence digest");
    assert!(with(&|r| r.evidence = &EVIDENCE[..7]).is_err());
    assert!(with(&|r| r.evidence = &REPEATED).is_err());
    // A new annual edition moves the tip: the old record no longer admits.
    let mut next = fence();
    next.catalogs = next.catalogs.with_edition(FENCE + 720, vec![ApprovedModelV1::of(h(5), h(6))]);
    p.palw_model_tier_v1 = Some(next);
    p.sync_palw_model_tier_v1();
    let why = format!("{:?}", p.validate_palw_model_tier_with_records_v1(&[record]).unwrap_err());
    assert!(why.contains("another catalog chain"), "{why}");
    // Below the gate nothing changed: shipped presets pass, never() collapses.
    for q in [MAINNET_PARAMS, TESTNET_PARAMS, t12()] {
        q.validate_palw_model_tier_v1().unwrap();
    }
}
