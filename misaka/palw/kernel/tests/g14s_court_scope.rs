//! **G14-S: the cumulative court scope enforced by the ledger** (`misaka_palw_kernel::court_scope`; ADR-0177 D2, DA16b §7.3 / §7.9,
//! C4R4 F-C4R4-17). The scope is consumer-injected past `palw_provider_court_v1` (here set on the genesis and the live ledger alike,
//! so a replay from genesis reaches the same root); below it every rule is the historical one.
//!
//! * the OPV route (the new reward channel): a K2-TIR-v1/v2 claim of a model with model relations owes model-dependent values in the
//!   clear, so past the scope its demands are refused past the model's clear budget (0 with an elementwise relation) and the class
//!   takes no new claim — F-C4R4-17's extraction has nothing to read;
//! * every claim: a requester names at most 16 distinct units of one claim (demands and segmented filings), and an accuser has at
//!   most 4 dismissed filings on one claim — refused by name, free, before any court runs; no other bond's allowance is touched, so an
//!   honest prosecutor still convicts.

#![allow(warnings)]
mod common;

use common::chain::T;
use common::ledger_world::*;
use misaka_palw_kernel::court_scope::{COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1, CourtScopePolicyV1};
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1};
use misaka_palw_kernel::rows::TABLE_COURT_SCOPE_V1;
use misaka_palw_kernel::scope::COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1;

fn scoped(w: &mut World, activation_daa: u64) {
    let p = Some(CourtScopePolicyV1 { activation_daa });
    w.genesis.court_scope_policy = p;
    w.l.court_scope_policy = p;
}

fn opened(ev: &[E]) -> usize {
    ev.iter().filter(|e| matches!(e, E::DemandOpened { .. })).count()
}

/// **F-C4R4-17 on the OPV route, under the court scope.** A claim committed before the scope keeps the historical rule (a demand is
/// served in the clear); past it, a new position of that model is refused (`COURT_SCOPE_CLEAR_BUDGET`: the clear budget of a model with
/// a norm gain is 0), joining the open demand still is not, and a NEW claim of the class is refused at the per-claim reward gate
/// (`COURT_SCOPE_INCOMPLETE`). No sequence of demands rebuilds a weight past the scope: none is served.
#[test]
fn g14s_f_c4r4_17_on_the_opv_route_the_clear_budget_refuses_and_the_class_takes_no_new_claim() {
    let mut w = World::new_opv();
    scoped(&mut w, 15);
    let prompt: Vec<u32> = (0..20u32).map(|i| (i * 7 + 3) % 32).collect();
    let job = w.post_job(2, &prompt, 1, 9);
    let h = w.honest(&job, 1);
    let id = h.claim.id();
    let ev = w.block(10, vec![h.tx]);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { claim } if *claim == id)), "{ev:?}");
    // Below the scope: the historical rule.
    let ev = w.block(11, vec![T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 0 }]);
    assert_eq!(opened(&ev), 1, "{ev:?}");
    assert!(w.l.court_scope.is_empty(), "nothing is written below the scope");
    // Past it: the model's clear budget is spent — a new clear position is refused by name, before anything is reserved.
    let reserved = w.l.bonds[&SPAM1].reserved;
    let ev = w.block(16, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 }]);
    let why = refused(&ev).expect("refused");
    assert!(why.contains("COURT_SCOPE_CLEAR_BUDGET"), "{why}");
    assert_eq!(w.l.bonds[&SPAM1].reserved, reserved);
    // Joining the open demand discloses nothing it does not already owe.
    let ev = w.block(17, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandJoined { .. })), "{ev:?}");
    assert_eq!(w.l.court_scope[&id].requested.len(), 1, "SPAM1's one unit (SPAM2 named its unit below the scope)");
    // A new claim of the class past the scope: the per-claim reward gate refuses it.
    let job2 = w.post_job(18, &prompt, 1, 10);
    let h2 = w.honest(&job2, 1);
    let ev = w.block(20, vec![h2.tx]);
    let why = refused(&ev).expect("the claim is refused");
    assert!(why.contains("COURT_SCOPE_INCOMPLETE"), "{why}");
    assert!(!w.l.claims.contains_key(&h2.claim.id()));
    // Table 28 is in the rows and the root, and a node that replays the chain from genesis reaches the same state.
    let rows = w.l.to_rows();
    assert!(rows.keys().any(|(t, _)| *t == TABLE_COURT_SCOPE_V1));
    let rebuilt = KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &rows).unwrap();
    assert_eq!(rebuilt.to_rows(), rows);
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.root(), w.l.root());
    assert_eq!(
        misaka_palw_kernel::rows::root_of_rows(&w.l.policy, w.l.config_root(), w.l.scalars(), w.l.opv.policy.as_ref(), &rows),
        w.l.root(),
        "rows and memory agree"
    );
}

/// **A filer past its cumulative scope is refused; nobody else is.** Past the scope one bond may name at most 16 distinct units of a
/// claim (its 17th demand is refused, nothing reserved) and have at most 4 dismissed filings on it (its 5th filing is refused before the
/// court runs, no fee). Another bond's demand is admitted and an outsider convicts the lie: the bounds are per requester (G14 C6).
/// Below the scope, the same 17 demands are all admitted (the historical rule).
#[test]
fn g14s_a_filer_past_its_cumulative_scope_is_refused_and_an_honest_prosecutor_still_convicts() {
    let prompt: Vec<u32> = (0..20u32).map(|i| (i * 7 + 3) % 32).collect();
    let demands = |id| {
        (0..=COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32)
            .map(|p| T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: p })
            .collect::<Vec<_>>()
    };
    // Below the scope: 17 demands, 17 sessions.
    {
        let mut w = World::new();
        let job = w.post_job(2, &prompt, 1, 9);
        let (_, lie) = w.lying(&job, 1);
        let id = lie.claim.id();
        w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
        assert_eq!(opened(&w.block(11, demands(id))), 17);
        assert!(w.l.court_scope.is_empty());
    }
    let mut w = World::new();
    scoped(&mut w, 0);
    let pol = w.l.policy;
    let job = w.post_job(2, &prompt, 1, 9);
    let (_, lie) = w.lying(&job, 1);
    let id = lie.claim.id();
    let ev = w.block(10, vec![lie.tx.clone(), T::PanelCovered { claim: id }]);
    assert!(
        ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })),
        "a Panel-licensed v1 claim: the clear budget binds the OPV route only"
    );
    let ev = w.block(11, demands(id));
    assert_eq!(opened(&ev), COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1);
    let why = refused(&ev).expect("the 17th is refused");
    assert!(why.contains("COURT_SCOPE_UNITS"), "{why}");
    assert_eq!(w.l.bonds[&SPAM1].reserved, COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u64 * pol.demand_bond);
    // Another bond's allowance is its own.
    let ev = w.block(12, vec![T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 16 }]);
    assert_eq!(opened(&ev), 1, "{ev:?}");
    // Dismissed filings: four, each paying its fee, then refused before the court runs.
    for i in 0..COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1 {
        let ev =
            w.block(13 + i as u64, vec![T::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![i as u8; 9]) }]);
        assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { .. })), "{ev:?}");
    }
    let collateral = w.l.bonds[&SPAM1].collateral;
    let ev = w.block(17, vec![T::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![9; 9]) }]);
    let why = refused(&ev).expect("the fifth is refused");
    assert!(why.contains("COURT_SCOPE_FILINGS"), "{why}");
    assert_eq!(w.l.bonds[&SPAM1].collateral, collateral, "a refusal is free");
    assert_eq!(w.l.court_scope[&id].dismissed, vec![(SPAM1, COURT_SCOPE_MAX_DISMISSED_PER_REQUESTER_V1)]);
    // The outsider convicts from published material.
    let da = Da::publishing(&lie.trace, &[]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the lie is found") };
    let ev = w.block(18, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted(&ev).is_some(), "{ev:?}");
}
