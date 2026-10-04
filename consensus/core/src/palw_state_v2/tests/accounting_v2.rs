//! **ADR-0172 stage 4 through the fold's own builder: the per-DAA allocation ledger.**
//!
//! Claims enter their DAA's open row where `write_claim` creates them; the first block of a later DAA closes the row (a chain's DAA never falls, so nothing more
//! can be accepted into it); a `Final` is paid `min(escrow, ⌊P_d · W / ΣW⌋)` from the CLOSED row, the same whatever the order of `Final`s; a void leaves `ΣW` alone
//! and nobody else is paid for it; a floor, an E-BLUE round or a FALLBACK makes no row. Every write is one delta entry, so a reorg reverts it exactly; the carriage
//! carries the ledger (a pruning snapshot) against the state root. Dormant below the fence: the same builder with no mirror writes nothing.

use super::*;
use crate::palw_accounting_v2::{PalwAccountingKeyV2 as K, PalwAccountingRowV2 as R, palw_emission_pool_v2};

const BUDGET: u64 = 4_445_600_000_000;

fn armed() -> PalwStateParamsV2 {
    params().with_accounting_v2_from_daa(Some(0))
}

fn claim(pwu: u64, daa: u64, class: u64) -> PalwClaimStateV2 {
    PalwClaimStateV2 { pwu, ..palw_claim_template_v1(h64(class), bond_key(1), daa, 0, palw_emission_pool_v2(BUDGET)) }
}

fn builder<'a>(state: &PalwChainStateV2, p: &'a PalwStateParamsV2, extras: &'a PalwTransitionExtrasV1) -> TransitionBuilder<'a> {
    let mut b = TransitionBuilder::new(state, p, false, false, false, false, extras);
    b.accounting_budget = BUDGET;
    b
}

fn terminal(c: &PalwClaimStateV2, phase: PalwClaimPhaseV2) -> PalwClaimStateV2 {
    PalwClaimStateV2 { phase, ..c.clone() }
}

#[test]
fn the_allocation_closes_once_pays_by_w_in_any_order_and_a_void_is_not_redistributed() {
    let (p, extras) = (armed(), PalwTransitionExtrasV1::default());
    let s0 = PalwChainStateV2::genesis();
    let mut b = builder(&s0, &p, &extras);
    // DAA 10: 1,000 REAL claims of different weights, and floors (the base class h64(1)) that make no row. DAA 11: 16 claims.
    let mut ids = Vec::new();
    for i in 0..1_000u64 {
        let id = h64(10_000 + i);
        b.write_claim(id, Some(claim(1 + (i % 17) * 3, 10, 100 + (i % 5))));
        ids.push(id);
    }
    for i in 0..40u64 {
        b.write_claim(h64(50_000 + i), Some(claim(9, 10, 1)));
    }
    let late: Vec<Hash64> = (0..16u64).map(|i| h64(60_000 + i)).collect();
    for (i, id) in late.iter().enumerate() {
        b.write_claim(*id, Some(claim(5 + i as u64, 11, 100)));
    }
    let open = |b: &TransitionBuilder, d: u64| match b.state.accounting_v2.get(&K::DaaOpen(d)) {
        Some(R::Daa(row)) => Some(*row),
        _ => None,
    };
    let row10 = open(&b, 10).expect("DAA 10's open row");
    assert_eq!((row10.count, row10.open, row10.budget), (1_000, 1_000, BUDGET), "floors made no row entry");
    let want: u128 = (0..1_000u64).map(|i| (1 + (i % 17) * 3) as u128).sum();
    assert_eq!(row10.sum_w, want);
    assert!(b.state.daa_allocation_v2(10).is_none(), "nothing is allocated before the closure");
    // The first block of DAA 11 closes DAA 10 — and only it; the block of DAA 12 closes 11.
    b.close_daa_rows_v2(10);
    assert!(open(&b, 10).is_some(), "a block of the DAA itself closes nothing");
    b.close_daa_rows_v2(11);
    assert!(open(&b, 10).is_none() && open(&b, 11).is_some());
    let closed = b.state.daa_allocation_v2(10).expect("closed");
    assert_eq!(closed, row10, "the closed row is the open row, frozen");
    // Payouts: the same in any order, within P_d; a floor is paid nothing.
    let pool = palw_emission_pool_v2(BUDGET) as u128;
    let pay = |b: &TransitionBuilder, order: &mut dyn Iterator<Item = usize>| -> Vec<u64> {
        let mut out = vec![0u64; ids.len()];
        for i in order {
            let c = claim(1 + (i as u64 % 17) * 3, 10, 100 + (i as u64 % 5));
            out[i] = b.accounting_v2_final_escrow(&c, c.escrowed_reward);
        }
        out
    };
    let up = pay(&b, &mut (0..ids.len()));
    let down = pay(&b, &mut (0..ids.len()).rev());
    assert_eq!(up, down, "a claim's amount does not depend on when it finalises");
    assert!(up.iter().map(|x| *x as u128).sum::<u128>() <= pool, "Σ passed P_d");
    assert_eq!(b.accounting_v2_final_escrow(&claim(9, 10, 1), 123), 0, "a floor accepted past the fence earns no subsidy");
    // Half the claims void: ΣW is untouched, the others' amounts do not move, nobody takes the voided shares.
    for id in ids.iter().step_by(2) {
        let c = claim(0, 0, 0);
        let _ = c;
        let live = b.state.claims.get(id).cloned().unwrap();
        b.write_claim(*id, Some(terminal(&live, PalwClaimPhaseV2::Voided { voided_daa: 40, reason: PalwVoidReasonV2::BindTimeout })));
    }
    let after = b.state.daa_allocation_v2(10).unwrap();
    assert_eq!((after.sum_w, after.open, after.count), (row10.sum_w, 500, 1_000));
    assert_eq!(pay(&b, &mut (0..ids.len())), up, "a void's share is not redistributed");
    // The rest finalise (any order): the closed row is dropped with its last claim.
    for id in ids.iter().skip(1).step_by(2) {
        let live = b.state.claims.get(id).cloned().unwrap();
        b.write_claim(*id, Some(terminal(&live, PalwClaimPhaseV2::Final { final_daa: 60 })));
    }
    assert!(b.state.daa_allocation_v2(10).is_none(), "dropped when nothing can read it again");
    assert!(b.state.daa_allocation_v2(11).is_none() && open(&b, 11).is_some(), "DAA 11 is still open (no block of DAA 12 yet)");
    // A Final claim later convicted (Final → Voided) releases nothing twice; a retirement (Final → None) neither.
    let id = ids[1];
    let fin = b.state.claims.get(&id).cloned().unwrap();
    b.write_claim(id, Some(terminal(&fin, PalwClaimPhaseV2::Voided { voided_daa: 70, reason: PalwVoidReasonV2::CourtFraud })));
    b.write_claim(ids[3], None);
    assert_eq!(open(&b, 11).unwrap().open, 16, "the other DAA's row is untouched");
    // Reorg: every write was one delta entry — replay reproduces the state, the reverse restores the parent; the carriage round-trips.
    let mut forward = s0.clone();
    for e in &b.entries {
        apply_delta_entry(&mut forward, e, false).unwrap();
    }
    assert_eq!(forward.accounting_v2, b.state.accounting_v2);
    let mut back = forward.clone();
    for e in b.entries.iter().rev() {
        apply_delta_entry(&mut back, e, true).unwrap();
    }
    assert!(back.accounting_v2.is_empty(), "reverts to the parent: no ledger at all");
    let carried: PalwStateCarriageV2 = borsh::from_slice(&borsh::to_vec(&PalwStateCarriageV2::from_state(&b.state)).unwrap()).unwrap();
    assert_eq!(carried.accounting_v2, b.state.accounting_v2, "the carriage carries the ledger");
}

#[test]
fn below_the_fence_the_same_calls_write_nothing_and_the_escrow_is_whole() {
    let (p, extras) = (params(), PalwTransitionExtrasV1::default());
    let s0 = PalwChainStateV2::genesis();
    let mut b = builder(&s0, &p, &extras);
    let c = claim(7, 10, 100);
    b.write_claim(h64(1), Some(c.clone()));
    b.close_daa_rows_v2(99);
    assert!(b.state.accounting_v2.is_empty() && b.entries.iter().all(|e| !matches!(e, PalwDeltaEntryV2::AccountingV2 { .. })));
    assert_eq!(b.accounting_v2_final_escrow(&c, 12_345), 12_345);
    assert_eq!(b.state.state_root(), {
        let mut plain = builder(&s0, &p, &extras);
        plain.write_claim(h64(1), Some(c));
        plain.state.state_root()
    });
    // A fence at a height never reached: the same.
    let far = params().with_accounting_v2_from_daa(Some(1_000_000_000));
    let mut f = builder(&s0, &far, &extras);
    f.write_claim(h64(1), Some(claim(7, 10, 100)));
    assert!(f.state.accounting_v2.is_empty());
}

#[test]
fn a_fallback_is_credited_once_a_slot_per_bond_and_a_round_once_per_claim_and_both_revert() {
    let (p, extras) = (armed(), PalwTransitionExtrasV1::default());
    let s0 = PalwChainStateV2::genesis();
    let mut b = builder(&s0, &p, &extras);
    assert_eq!(b.state.fallback_weight_v2(), 0);
    assert!(b.accounting_v2_credit_fallback(bond_key(1), 50, 7));
    assert!(!b.accounting_v2_credit_fallback(bond_key(1), 50, 7), "one credit a bond a slot");
    assert!(b.accounting_v2_credit_fallback(bond_key(2), 50, 7), "another bond");
    assert!(b.accounting_v2_credit_fallback(bond_key(1), 51, 7), "the next slot");
    assert_eq!(b.state.fallback_weight_v2(), 21);
    assert_eq!(b.accounting_v2_credit_round(h64(5), 3, 10), 10);
    assert_eq!(b.accounting_v2_credit_round(h64(5), 3, 10), 0, "once per (claim, round)");
    assert_eq!(b.accounting_v2_credit_round(h64(5), 4, 0), 0, "σ = 0 writes nothing");
    // The comparator reads the fallback weight beside `safe_weight`.
    assert_eq!(b.state.safe_weight(), 0);
    let order = b.state.candidate_order(h64(9));
    assert_eq!(order.safe_weight, 21, "the economic key is safe_weight + fallback_weight");
    let mut replay = s0.clone();
    for e in &b.entries {
        apply_delta_entry(&mut replay, e, false).unwrap();
    }
    assert_eq!(replay.state_root(), b.state.state_root());
    for e in b.entries.iter().rev() {
        apply_delta_entry(&mut replay, e, true).unwrap();
    }
    assert_eq!(replay.state_root(), s0.state_root());
}
