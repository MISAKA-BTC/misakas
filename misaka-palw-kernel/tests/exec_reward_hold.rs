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
