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
//!
//! Gate C for every tier: `palw_rfc0018_v1/tests.rs` `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt`.

use std::collections::BTreeMap;

use super::*;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
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
    PalwBondKeyV2(TransactionOutpoint::new(crate::Hash::from_bytes([n; 32]), 0))
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

#[test]
fn only_an_annual_fork_listing_makes_1_0() {
    let mut f = fence();
    let fork_at = FENCE + YEAR_DAA;
    f.annual.push(PalwApprovedModelsForkV1 {
        activation: ForkActivation::new(fork_at),
        models: vec![PalwApprovedModelV1 { model_id: h(1), artifact_root: h(2) }],
    });
    f.validate().unwrap();
    let mirror = f.mirror();
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, fork_at - 1), PalwModelTierV1::Registered);
    let listed = palw_model_tier_of_v1(Some(&mirror), &h(1), &h(2), false, fork_at);
    assert_eq!(listed, PalwModelTierV1::Approved(PalwTierSourceV1::AnnualFork { index: 0, from_daa: fork_at }));
    assert_eq!(palw_model_tier_coefficient_permille_v1(Some(&mirror), listed, fork_at), 1_000);
    // Another model is untouched by the listing.
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(3), &h(4), false, fork_at), PalwModelTierV1::Registered);
}

#[test]
fn an_annual_list_only_adds_and_never_re_roots_a_model() {
    let entry = |id: u64, root: u64| PalwApprovedModelV1 { model_id: h(id), artifact_root: h(root) };
    let mut f = fence();
    f.genesis.push(entry(1, 2));
    f.annual.push(PalwApprovedModelsForkV1 { activation: ForkActivation::new(FENCE + 10), models: vec![entry(3, 4)] });
    f.validate().unwrap();
    // Re-listing a model id — with its own root or another — is refused: a list never overwrites or re-roots.
    for again in [entry(1, 2), entry(1, 9), entry(3, 9)] {
        let mut g = f.clone();
        g.annual.push(PalwApprovedModelsForkV1 { activation: ForkActivation::new(FENCE + 20), models: vec![again] });
        assert!(g.validate().is_err(), "{again:?}");
    }
    // Out-of-order and empty lists are refused.
    let mut late = f.clone();
    late.annual.push(PalwApprovedModelsForkV1 { activation: ForkActivation::new(FENCE + 5), models: vec![entry(5, 6)] });
    assert!(late.validate().is_err());
    let mut empty = f.clone();
    empty.annual.push(PalwApprovedModelsForkV1 { activation: ForkActivation::new(FENCE + 30), models: Vec::new() });
    assert!(empty.validate().is_err());
    // Membership needs the class's own root: an entry naming another root never approves (or replaces) the registered class.
    let mirror = f.mirror();
    assert_eq!(palw_model_tier_of_v1(Some(&mirror), &h(3), &h(99), false, FENCE + 10), PalwModelTierV1::Registered);
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
/// large N is; below the cap nothing changes, and what the cap removes is never handed to the approved tier.
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
    // The cap factor itself.
    let w = PalwAllocationWeightV1::from_u64(1_000);
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w, PalwAllocationWeightV1::from_u64(100), 300), 1_000_000, "within the cap");
    assert_eq!(palw_model_tier_cap_factor_ppm_v1(w, PalwAllocationWeightV1::from_u64(600), 300), 500_000, "600 → 300 of 1000");
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
