//! EXEC's consumer-frozen horizon must survive computation Final, default, restore and root fee settlement.
mod common;
use common::chain::T;
use common::ledger_world::*;
use misaka_palw_kernel::ledger::{KernelLedgerV1, OutsiderFindingV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;

#[test]
fn an_early_piece_keeps_collectible_collateral_past_its_original_kernel_horizon() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).unwrap();
    w.block(60, vec![]);
    assert_eq!(w.l.claims[&id].liability_until, Some(600), "Final cannot overwrite the consumer floor with 260");
    for at in [261, 499] {
        w.block(at, vec![]);
        assert_eq!(w.l.claims[&id].reserved, 1000);
        assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000);
        assert_eq!(w.consumer.paid(&PRODUCER), 0);
    }
    // Root fee settlement spends poster escrow, and never releases the producer's fraud reservation.
    let mut events = Vec::new();
    assert_eq!(w.l.finish_exec_reward([0xE1; 64], id, 3, &mut events).unwrap(), 3);
    w.consumer.book.apply_events(&events).unwrap();
    w.consumer.check_view(&w.l);
    assert!(w.l.exec_reward_holds.is_empty());
    w.l = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &w.l.to_rows()).unwrap();
    w.block(600, vec![]);
    assert_eq!(w.l.claims[&id].reserved, 1000, "a proof is actionable on the inclusive last day");
    w.block(601, vec![]);
    assert_eq!(w.l.claims[&id].reserved, 0, "retention is finite after the protected horizon");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0);
}

#[test]
fn a_public_lie_is_still_convicted_after_the_old_horizon_from_a_restored_bound_ledger() {
    for settle_root_fee in [false, true] {
        let mut w = World::new();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let (_, lie) = w.lying(&job, 3);
        let id = lie.claim.id();
        let da = Da::publishing(&lie.trace, &[]);
        w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
        let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("public material proves the lie") };
        w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).unwrap();
        w.block(60, vec![]);
        if settle_root_fee {
            let mut events = Vec::new();
            w.l.finish_exec_reward([0xE1; 64], id, 7, &mut events).unwrap();
            w.consumer.book.apply_events(&events).unwrap();
        }
        w.block(300, vec![]);
        assert_eq!(w.l.claims[&id].reserved, 1000);
        let root = w.l.root();
        w.l = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &w.l.to_rows()).unwrap();
        assert_eq!(w.l.root(), root);
        w.block(301, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
        assert!(w.l.claims[&id].convicted);
        assert_eq!(w.l.bonds[&PRODUCER].collateral, 4000);
        assert_eq!(w.consumer.paid(&OUTSIDER), 500);
        assert_eq!(w.consumer.paid(&PRODUCER), if settle_root_fee { 7 } else { 0 });
    }
}

#[test]
fn a_demand_after_the_old_horizon_can_still_default_withheld_material() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).unwrap();
    w.block(60, vec![]);
    w.block(300, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    assert!(!w.l.demands.is_empty(), "the complete DA/court path fits the extended horizon");
    w.block(321, vec![]);
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Final { .. }));
    assert_eq!(w.l.claims[&id].reserved, 0);
    assert_eq!(w.l.claims[&id].liability_until, Some(600));
    assert!(w.l.finish_exec_reward([0xE1; 64], id, 7, &mut Vec::new()).is_err());
    assert_eq!(w.consumer.paid(&PRODUCER), 0);
}

#[test]
fn released_or_expired_standalone_work_cannot_recreate_reservations_by_late_binding() {
    let mut p = policy();
    p.claim_reward = 0;
    let mut w = World::with(p);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    w.block(60, vec![]);
    assert!(!w.l.claims[&id].rewarded);
    w.l.begin_block(261).unwrap();
    let root = w.l.root();
    assert!(w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).is_err());
    assert_eq!(w.l.root(), root, "an expired-but-unswept reservation cannot reopen its proof window");
    w.block(261, vec![]);
    assert_eq!(w.l.claims[&id].reserved, 0);
    let root = w.l.root();
    assert!(w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).is_err());
    assert_eq!(w.l.root(), root);
}

#[test]
fn prefinal_default_keeps_the_consumer_floor_for_a_later_fraud_court() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx]);
    w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).unwrap();
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 }]);
    w.block(32, vec![]);
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Unavailable { .. }));
    assert_eq!(w.l.claims[&id].liability_until, Some(600), "default cannot shorten the consumer floor to 232");
    assert!(w.l.claims[&id].reserved > 0);
}

/// **R8-WINDOW (2026-10-15): a consented extension lengthens a live EXEC dependency horizon** — only from the exact live value, never
/// past the claim-fixed deadline, never reopened once expired; the extended last day still holds the collateral and the next releases.
#[test]
fn a_consented_extension_lengthens_a_live_exec_horizon_inside_its_hard_deadline_only() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    w.l.bind_exec_reward_until([0xE1; 64], id, 500, 600).unwrap();
    w.block(60, vec![]);
    assert!(w.l.extend_claim_liability_by_consent_v1(&id, 599, 700, 800).is_err(), "a stale horizon never applies");
    assert!(w.l.extend_claim_liability_by_consent_v1(&id, 600, 600, 800).is_err(), "never equal or shorter");
    assert!(w.l.extend_claim_liability_by_consent_v1(&id, 600, 801, 800).is_err(), "never past the claim-fixed deadline");
    w.l.extend_claim_liability_by_consent_v1(&id, 600, 700, 800).unwrap();
    assert_eq!(w.l.claims[&id].liability_until, Some(700));
    w.l.validate_exec_reward_holds().unwrap();
    let root = w.l.root();
    w.l = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &w.l.to_rows()).unwrap();
    assert_eq!(w.l.root(), root, "the extension is in the rows");
    w.block(700, vec![]);
    assert_eq!(w.l.claims[&id].reserved, 1000, "the extended inclusive last day still holds the collateral");
    w.block(701, vec![]);
    assert_eq!(w.l.claims[&id].reserved, 0, "retention stays finite");
    assert!(w.l.extend_claim_liability_by_consent_v1(&id, 700, 750, 800).is_err(), "an expired horizon is never reopened");
}
