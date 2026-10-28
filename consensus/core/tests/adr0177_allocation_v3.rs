//! **ADR-0177 / RFC-0015 §8.5.5 — the allocation as decided 2026-10-14** (`α = 1.5`, the 7-day average (2026-10-16; 30 days before) of miner + claim-bound
//! verifier capital, `P_m` for block opportunity and coinbase), on the budget engine (`roll_epoch_v3`, `model_blocks_available_v1`).
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS.** Values are TEST values.
//!
//! Run (validation phase): `cargo test -p kaspa-consensus-core --test adr0177_allocation_v3`

#![allow(warnings)]
use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bond_budget_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

const KAS: u64 = 100_000_000;

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes([n; 64]), 0))
}

fn model(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn policy() -> PalwModelAllocationPolicyV1 {
    let p = PalwModelAllocationPolicyV1::decided_unapproved_v3();
    p.validate().expect("the decided form validates");
    p
}

fn state() -> PalwBondBudgetStateV1 {
    PalwBondBudgetStateV1::new(0, &PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1())
}

#[test]
fn the_decided_terms_are_pinned() {
    let p = policy();
    assert_eq!(p.curve.alpha_halves, 3, "alpha = 1.5");
    assert_eq!(p.average_epochs(), Some(7), "7 day-equivalents over 1-day epochs (user decision 2026-10-16)");
    let bad = |f: &dyn Fn(&mut PalwModelAllocationPolicyV1)| {
        let mut q = policy();
        f(&mut q);
        q.validate().is_err()
    };
    assert!(bad(&|q| q.curve.alpha_halves = 4), "alpha is decided");
    assert!(bad(&|q| q.average.as_mut().unwrap().average_days = 30), "the superseded 30-day average");
    assert!(bad(&|q| q.average.as_mut().unwrap().verifier_capital = false), "miner + verifier capital");
    // `false` is the V+ form (pin 2026-10-22 ADJUST 1): the policy validates; the fence checks it against V+.
    assert!(!bad(&|q| q.average.as_mut().unwrap().block_opportunity = false), "the V+ form is a valid policy");
    assert!(bad(&|q| q.epoch_daa = 1_000), "the epoch divides the span (1,000 does not divide 5,040)");
    assert!(bad(&|q| q.epoch_daa = 0), "a zero epoch is refused, never a division by zero");
    assert!(bad(&|q| q.average = None));
    // V2 bytes untouched.
    let v2 = PalwModelAllocationPolicyV1::unapproved_probe_v1();
    let mut v2_with = v2.clone();
    v2_with.average = Some(PalwAllocationAverageV1::decided_v1());
    assert_eq!(borsh::to_vec(&v2).unwrap(), borsh::to_vec(&v2_with).unwrap());
}

#[test]
fn miner_and_verifier_capital_count_once_per_bond() {
    // Bond 1 assigns 600 to model A and verifies A for 600 more on 1,000 of capital: clipped to 1,000 in total, never 1,200.
    let mut miner = BTreeMap::new();
    miner.insert(bond(1), vec![(model(0xA), 600 * KAS)]);
    let mut verifier: BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>> = BTreeMap::new();
    verifier.entry(bond(1)).or_default().insert(model(0xA), 600 * KAS);
    // Bond 2 only verifies model B (a pure verifier's capital counts for B).
    verifier.entry(bond(2)).or_default().insert(model(0xB), 300 * KAS);
    let s = palw_allocation_snapshot_v3(&miner, &verifier, |b| if *b == bond(1) { (1_000 * KAS, true) } else { (5_000 * KAS, true) });
    assert_eq!(s.get(&model(0xA)).copied(), Some((1_000 * KAS) as u128), "the same funds counted once");
    assert_eq!(s.get(&model(0xB)).copied(), Some((300 * KAS) as u128));
    // An inactive bond counts nothing.
    let none = palw_allocation_snapshot_v3(&miner, &verifier, |_| (1_000 * KAS, false));
    assert!(none.is_empty());
}

#[test]
fn the_average_spans_seven_epochs_and_a_new_model_ramps_in() {
    let p = policy();
    let mut s = state();
    let mut j = Vec::new();
    let empty = BTreeMap::new();
    // Model A is assigned 900 at epoch 0 (counts from epoch 2, after seasoning); model B joins with 900 at epoch 15 (from 17).
    for epoch in 0..60u64 {
        if epoch == 0 {
            s.assign(&p, bond(1), &[(model(0xA), 900 * KAS)], 1, 2_000 * KAS, epoch, |_| true, &mut j).unwrap();
        }
        if epoch == 15 {
            s.assign(&p, bond(1), &[(model(0xA), 900 * KAS), (model(0xB), 900 * KAS)], 2, 2_000 * KAS, epoch, |_| true, &mut j)
                .unwrap();
        }
        s.roll_epoch_v3(&p, epoch, epoch * p.epoch_daa, |_| (2_000 * KAS, true), &empty, |_, c| c, &mut j);
        let a = s.model_row(&model(0xA)).map(|r| r.capital).unwrap_or(0);
        let b = s.model_row(&model(0xB)).map(|r| r.capital).unwrap_or(0);
        assert!(a <= (900 * KAS) as u128 && b <= a, "epoch {epoch}: averages never exceed the capital, the newcomer ramps");
        if epoch >= 17 + 6 {
            assert_eq!(b, a, "after a full span the newcomer's average equals the incumbent's");
            assert_eq!(a, (900 * KAS) as u128);
        }
    }
    // History rows never outlive the span: at most 7 per model, and no block rows of past epochs.
    let history = s.models.values().filter(|r| r.reserved_sompi == PALW_ALLOCATION_HISTORY_MARKER_V1).count();
    assert!(history <= 2 * 7, "{history} history rows");
    s.check_consistency().expect("consistent");
}

#[test]
fn block_opportunity_follows_p_m_and_is_never_returned_early() {
    let p = policy();
    let mut s = state();
    let mut j = Vec::new();
    let empty = BTreeMap::new();
    // Models A and B with equal averages: each half the blocks of the epoch so far.
    s.assign(&p, bond(1), &[(model(0xA), 500 * KAS), (model(0xB), 500 * KAS)], 1, 1_000 * KAS, 0, |_| true, &mut j).unwrap();
    for epoch in 0..3u64 {
        s.roll_epoch_v3(&p, epoch, epoch * p.epoch_daa, |_| (1_000 * KAS, true), &empty, |_, c| c, &mut j);
    }
    let start = 2 * p.epoch_daa;
    let now = start + 99; // 100 DAA of the epoch → 100 blocks accrued
    let a = s.model_blocks_available_v1(&model(0xA), now);
    let b = s.model_blocks_available_v1(&model(0xB), now);
    assert_eq!(a, b);
    assert!(a <= 50 * PALW_BUDGET_BLOCK_UNIT_V1 && a + b <= 100 * PALW_BUDGET_BLOCK_UNIT_V1, "Σ P_m · blocks ≤ the blocks");
    s.reserve_model_blocks_v1(&model(0xA), a, &mut j);
    assert_eq!(s.model_blocks_available_v1(&model(0xA), now), 0, "used up until more DAA accrue");
    assert_eq!(s.model_blocks_available_v1(&model(0xB), now), b, "the other model is untouched");
    assert_eq!(s.model_blocks_available_v1(&model(0xC), now), 0, "a model with no capital gets no block opportunity");
}

#[test]
fn rfc0017_gap_e1_a_registered_or_lapsed_model_leaves_the_allocation() {
    // RFC-0017 GAP-E1: the roll passes each model's capital through `palw_eligibility_allocation_capital_v1`. Model B is Lapsed from
    // epoch 5 (its stage maps its capital to 0): it is neither in the denominator nor given block opportunity, and A takes all of
    // both; with no eligibility mirror (the identity hook) both stay — the rule below the fence is unchanged.
    use kaspa_consensus_core::palw_model_eligibility_v1::{PalwEligibilityStageV1, palw_eligibility_allocation_capital_v1};
    let p = policy();
    let empty = BTreeMap::new();
    let run = |lapse_b: bool| {
        let mut s = state();
        let mut j = Vec::new();
        s.assign(&p, bond(1), &[(model(0xA), 500 * KAS), (model(0xB), 500 * KAS)], 1, 1_000 * KAS, 0, |_| true, &mut j).unwrap();
        for epoch in 0..8u64 {
            let eligible = |m: &Hash64, c: u128| {
                let stage = (lapse_b && *m == model(0xB) && epoch >= 5).then_some(PalwEligibilityStageV1::Lapsed);
                palw_eligibility_allocation_capital_v1(stage, c)
            };
            s.roll_epoch_v3(&p, epoch, epoch * p.epoch_daa, |_| (1_000 * KAS, true), &empty, eligible, &mut j);
        }
        s
    };
    let gated = run(true);
    let now = 7 * PALW_BOND_HOLD_DAA_PER_DAY_V1 + 99;
    assert!(gated.model_row(&model(0xB)).is_none(), "a lapsed model has no live allocation row");
    assert_eq!(gated.model_blocks_available_v1(&model(0xB), now), 0, "and no block opportunity");
    assert_eq!(gated.model_blocks_available_v1(&model(0xA), now), 100 * PALW_BUDGET_BLOCK_UNIT_V1, "A holds the whole denominator");
    let ungated = run(false);
    assert!(ungated.model_row(&model(0xB)).is_some(), "below the fence nothing changes");
    assert!(ungated.model_blocks_available_v1(&model(0xA), now) < 100 * PALW_BUDGET_BLOCK_UNIT_V1);
}
