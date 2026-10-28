#![allow(warnings)]
mod common;
use common::chain::{POSTER, T};
use common::ledger_world::*;
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::{
    hash::Digest,
    ledger::KernelLedgerV1,
    rows::{config_root_of, root_of_rows},
};

fn held() -> (World, Digest, Digest) {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let claim = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim }]);
    w.l.bind_exec_reward([0xE1; 64], claim, 500).unwrap();
    let before = w.l.bonds[&POSTER].clone();
    let ev = w.block(60, vec![]);
    assert!(matches!(w.l.claims[&claim].life.state, ClaimStateV1::Final { .. }));
    assert!(!w.l.claims[&claim].rewarded);
    assert_eq!(w.consumer.paid(&PRODUCER), 0);
    assert_eq!(w.l.bonds[&POSTER], before);
    assert_eq!(w.l.job_escrows[&job.id()].amount, 7);
    assert!(ev.iter().any(|e| matches!(e, misaka_palw_kernel::ledger::LedgerEventV1::Final { reward: 0, .. })));
    (w, job.id(), claim)
}

fn restored(w: &World) -> KernelLedgerV1 {
    let rows = w.l.to_rows();
    assert_eq!(
        root_of_rows(&w.l.policy, config_root_of(&w.l.schedule, &w.l.known), w.l.scalars(), w.l.opv_policy(), &rows),
        w.l.root()
    );
    let back = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &rows).unwrap();
    assert_eq!(back.root(), w.l.root());
    assert_eq!(back.to_rows(), rows);
    back
}

#[test]
fn computation_final_holds_funds_then_root_share_caps_actual_debit_and_refunds_rest() {
    for cap in [0, 3, 7, 100] {
        let (mut w, job, claim) = held();
        w.l = restored(&w);
        let before = w.l.bonds[&POSTER].clone();
        let mut events = Vec::new();
        let paid = w.l.finish_exec_reward([0xE1; 64], claim, cap, &mut events).unwrap();
        assert_eq!(paid, cap.min(7));
        w.consumer.book.apply_events(&events).unwrap();
        w.consumer.check_view(&w.l);
        assert_eq!(w.l.bonds[&POSTER].collateral, before.collateral - paid);
        assert_eq!(w.l.bonds[&POSTER].reserved, before.reserved - 7);
        assert_eq!(w.consumer.paid(&PRODUCER), paid);
        assert!(!w.l.job_escrows.contains_key(&job));
        assert!(w.l.exec_reward_holds.is_empty());
        restored(&w);
        let root = w.l.root();
        assert!(w.l.finish_exec_reward([0xE1; 64], claim, cap, &mut events).is_err());
        assert_eq!(w.l.root(), root, "duplicate Final has no second debit or payout");
    }
}

#[test]
fn wrong_owner_nonfinal_and_expired_roots_cannot_debit_but_cancellation_refunds_once() {
    let (mut w, job, claim) = held();
    let before = w.l.root();
    assert!(w.l.finish_exec_reward([0xE2; 64], claim, 7, &mut Vec::new()).is_err());
    assert!(w.l.bind_exec_reward([0xE2; 64], claim, 500).is_err());
    assert!(w.l.cancel_exec_reward([0xE2; 64], claim, &mut Vec::new()).is_err());
    assert_eq!(w.l.root(), before);
    w.l.daa = 500;
    let before = w.l.root();
    assert!(w.l.finish_exec_reward([0xE1; 64], claim, 7, &mut Vec::new()).is_err());
    assert_eq!(w.l.root(), before);
    let collateral = w.l.bonds[&POSTER].collateral;
    let mut events = Vec::new();
    w.l.cancel_exec_reward([0xE1; 64], claim, &mut events).unwrap();
    w.consumer.book.apply_events(&events).unwrap();
    w.consumer.check_view(&w.l);
    assert_eq!(w.l.bonds[&POSTER].collateral, collateral);
    assert_eq!(w.consumer.paid(&PRODUCER), 0);
    assert!(!w.l.job_escrows.contains_key(&job));
    let root = w.l.root();
    w.l.cancel_exec_reward([0xE1; 64], claim, &mut Vec::new()).unwrap();
    assert_eq!(w.l.root(), root);
}

#[test]
fn partial_poster_collateral_pays_only_the_actual_debit() {
    let (mut w, _, claim) = held();
    w.block(61, vec![T::RegisterBond { bond: POSTER, collateral: 3 }]);
    let mut events = Vec::new();
    assert_eq!(w.l.finish_exec_reward([0xE1; 64], claim, 7, &mut events).unwrap(), 3);
    w.consumer.book.apply_events(&events).unwrap();
    w.consumer.check_view(&w.l);
    assert_eq!(w.consumer.paid(&PRODUCER), 3);
    assert_eq!(w.l.bonds[&POSTER].reserved, 0);
}

#[test]
fn nonfinal_or_postfinal_defaulted_claim_cannot_spend_held_funds() {
    let (mut w, _, claim) = held();
    let final_state = w.l.claims[&claim].life.state.clone();
    w.l.claims.get_mut(&claim).unwrap().life.state = ClaimStateV1::ChallengeBound { anchor_daa: 10 };
    let root = w.l.root();
    assert!(w.l.finish_exec_reward([0xE1; 64], claim, 7, &mut Vec::new()).is_err());
    assert_eq!(w.l.root(), root);
    w.l.claims.get_mut(&claim).unwrap().life.state = final_state;
    w.l.claims.get_mut(&claim).unwrap().reserved = 0;
    let root = w.l.root();
    assert!(w.l.finish_exec_reward([0xE1; 64], claim, 7, &mut Vec::new()).is_err());
    assert_eq!(w.l.root(), root);
}

#[test]
fn already_paid_standalone_work_cannot_attach_and_orphan_hold_cannot_restore() {
    let mut w = World::new();
    let (claim, _, _, _) = final_lying_claim(&mut w);
    let root = w.l.root();
    assert!(w.l.bind_exec_reward([0xE1; 64], claim, 500).is_err());
    assert_eq!(w.l.root(), root);
    let (mut w, _, claim) = held();
    w.l.claims.remove(&claim);
    assert!(KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &w.l.to_rows()).is_err());
}

#[test]
fn a_sibling_final_cannot_spend_pending_exec_job_escrow_and_cancellation_refunds_it() {
    for own_coverage_daa in [10, 20] {
        for cancel_daa in [70, 400] {
            let mut w = World::new();
            let job = w.post_job(2, &[3, 17, 9], 3, 1);
            let own = w.honest(&job, 3);
            let own_id = own.claim.id();
            let sibling = w.produce(&job, OUTSIDER, own.claim.generated.clone(), &w.params, |_| {});
            let sibling_id = sibling.claim.id();
            let mut objects = vec![own.tx, sibling.tx, T::PanelCovered { claim: sibling_id }];
            if own_coverage_daa == 10 {
                objects.push(T::PanelCovered { claim: own_id });
            }
            w.block(10, objects);
            w.l.bind_exec_reward([0xE1; 64], own_id, 500).unwrap();
            if own_coverage_daa == 20 {
                w.block(20, vec![T::PanelCovered { claim: own_id }]);
            }
            assert_eq!(w.l.job_claims[&job.id()], sibling_id, "the first Panel pass retains job ownership");
            assert!(matches!(w.l.claims[&own_id].life.state, ClaimStateV1::Checking { .. }));
            let poster = w.l.bonds[&POSTER].clone();
            let events = w.block(60, vec![]);
            assert!(matches!(w.l.claims[&sibling_id].life.state, ClaimStateV1::Final { .. }));
            assert!(!w.l.claims[&sibling_id].rewarded, "another producer cannot consume the held job's escrow");
            assert_eq!(w.consumer.paid(&OUTSIDER), 0);
            assert_eq!(w.l.bonds[&POSTER], poster);
            assert_eq!(w.l.job_escrows[&job.id()].amount, 7);
            assert!(events.iter().any(|event| matches!(event,
                misaka_palw_kernel::ledger::LedgerEventV1::Final { claim, reward: 0 } if *claim == sibling_id)));
            w.l = restored(&w);
            w.block(cancel_daa, vec![]);
            assert!(matches!(w.l.claims[&own_id].life.state, ClaimStateV1::Checking { .. } | ClaimStateV1::TimedOut { .. }));
            assert_eq!(w.l.bonds[&POSTER], poster, "repeated ticks after restore keep the job escrow held");
            assert_eq!(w.l.job_escrows[&job.id()].amount, 7);
            let root = w.l.root();
            let mut refused_events = Vec::new();
            assert!(
                w.l.finish_exec_reward([0xE1; 64], own_id, 3, &mut refused_events).is_err(),
                "a pending or timed-out claim cannot settle the root fee"
            );
            assert_eq!(w.l.root(), root);
            assert!(refused_events.is_empty());

            let mut events = Vec::new();
            w.l.cancel_exec_reward([0xE1; 64], own_id, &mut events).unwrap();
            w.consumer.book.apply_events(&events).unwrap();
            w.consumer.check_view(&w.l);
            assert_eq!(w.l.bonds[&POSTER].collateral, poster.collateral);
            assert_eq!(w.l.bonds[&POSTER].reserved, poster.reserved - 7);
            assert_eq!(w.consumer.paid(&PRODUCER), 0);
            assert_eq!(w.consumer.paid(&OUTSIDER), 0);
            assert!(!w.l.job_escrows.contains_key(&job.id()));
            assert!(w.l.exec_reward_holds.is_empty());
            restored(&w);
        }
    }
}

#[test]
fn standalone_final_uses_trusted_reward_grant_and_exec_substitution_uses_only_its_root_cap() {
    for cap in [0, 3, 7, 100] {
        let mut w = World::new();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let honest = w.honest(&job, 3);
        let claim = honest.claim.id();
        w.block(10, vec![honest.tx, T::PanelCovered { claim }]);
        let rows = w.l.to_rows();
        let root = w.l.root();
        w.l.consumer_final_reward_caps.insert(claim, cap);
        assert_eq!(w.l.root(), root, "trusted allowance is not transaction-owned state");
        assert_eq!(w.l.to_rows(), rows);
        let before = w.l.bonds[&POSTER].clone();
        w.block(60, vec![]);
        assert_eq!(w.consumer.paid(&PRODUCER), cap.min(7));
        assert_eq!(w.l.bonds[&POSTER].collateral, before.collateral - cap.min(7));
        assert_eq!(w.l.bonds[&POSTER].reserved, before.reserved - 7);
        assert!(!w.l.job_escrows.contains_key(&job.id()));
        w.block(61, vec![]);
        assert_eq!(w.consumer.paid(&PRODUCER), cap.min(7), "no second Final fee");
        assert!(restored(&w).consumer_final_reward_caps.is_empty(), "restore must re-inject current native budget");
        let cached = w.l.clone().as_rebuilt_from_v1(&w.l.to_rows());
        assert!(cached.consumer_final_reward_caps.is_empty());
    }
    let (mut w, _, claim) = held();
    // Computation Final closes standalone admission. Its unused R remains in
    // the shared window; actual bound funding draws only the native root cap.
    w.l.consumer_final_reward_caps.insert(claim, 0);
    let mut events = Vec::new();
    assert_eq!(w.l.finish_exec_reward([0xE1; 64], claim, 3, &mut events).unwrap(), 3);
    w.consumer.book.apply_events(&events).unwrap();
    w.consumer.check_view(&w.l);
    assert_eq!(w.consumer.paid(&PRODUCER), 3);
}

/// **R8-ECON at settlement (round 3)**: a contracted Final whose escrow, consumer cap or poster bond cannot fund the signed minimum is
/// neutral — the escrow is released whole to the poster and the producer is paid nothing out of it — never a smaller payment; at or
/// above the floor it pays exactly as before. The floor is consumer-injected, never a row, and a restore carries none.
#[test]
fn a_minimum_price_floor_pays_at_least_the_minimum_or_releases_the_escrow_whole() {
    for (cap, floor, paid) in [(7, 7, 7), (100, 5, 7), (3, 5, 0), (0, 1, 0), (6, 7, 0)] {
        let mut w = World::new();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let honest = w.honest(&job, 3);
        let claim = honest.claim.id();
        w.block(10, vec![honest.tx, T::PanelCovered { claim }]);
        let (rows, root) = (w.l.to_rows(), w.l.root());
        w.l.consumer_final_reward_caps.insert(claim, cap);
        w.l.consumer_minimum_payouts_v1.insert(claim, floor);
        assert_eq!(w.l.root(), root, "a floor is consumer-injected, never transaction-owned state");
        assert_eq!(w.l.to_rows(), rows);
        let before = w.l.bonds[&POSTER].clone();
        w.block(60, vec![]);
        assert_eq!(w.consumer.paid(&PRODUCER), paid, "cap {cap}, floor {floor}");
        assert_eq!(w.l.bonds[&POSTER].collateral, before.collateral - paid);
        assert_eq!(w.l.bonds[&POSTER].reserved, before.reserved - 7, "the escrow's reservation is released either way");
        assert!(!w.l.job_escrows.contains_key(&job.id()));
        assert_eq!(w.l.claims[&claim].rewarded, paid > 0);
        assert!(restored(&w).consumer_minimum_payouts_v1.is_empty(), "restore must re-inject the current floors");
    }
}
