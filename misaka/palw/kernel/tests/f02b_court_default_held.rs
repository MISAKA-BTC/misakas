//! G14 lane F02b: a court-service-bound claim whose consumer adopted no inclusion premise is HELD at its demand deadline — no
//! `ProducerDefault`, no collateral taken, not Unavailable — and the ordinary default applies once the hold is lifted.
mod common;

use common::chain::T;
use common::ledger_world::{OUTSIDER, PRODUCER, World};
use misaka_palw_kernel::{ledger::LedgerEventV1, lifecycle::ClaimStateV1};

#[test]
fn f02b_held_default_charges_nothing_and_resumes_when_lifted() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 1, 1);
    let honest = w.honest(&job, 1);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    w.l.court_default_held.insert(id);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
    let due = 11 + w.l.policy.court_deadline_daa + 1;
    let events = w.block(due, vec![]);
    assert!(
        !events.iter().any(|e| matches!(e, LedgerEventV1::ProducerDefault { .. } | LedgerEventV1::PostFinalDefault { .. })),
        "{events:?}"
    );
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral, "no slash while held");
    assert!(!matches!(w.l.claims[&id].life.state, ClaimStateV1::Unavailable { .. } | ClaimStateV1::Final { .. }));
    // Negative control: lifted, the same due demand defaults ordinarily.
    w.l.court_default_held.clear();
    let events = w.block(due + 1, vec![]);
    assert!(events.iter().any(|e| matches!(e, LedgerEventV1::ProducerDefault { .. })), "{events:?}");
    assert!(w.l.bonds[&PRODUCER].collateral < collateral);
}
