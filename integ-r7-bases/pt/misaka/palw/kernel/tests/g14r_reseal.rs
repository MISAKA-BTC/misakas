//! **G14-R: an identical proof re-seal is a no-op** (a pursuit that restarted with its seal in flight re-sends the same bytes).
//!
//! Before G14-R the second copy replaced the row and restarted its clock: the restarted verifier's own filing lost its precedence
//! window, and a copyist who sealed the same bytes in between took the bounty. A re-seal of ANOTHER proof still replaces the row and
//! restarts the clock (the rule the earliest-seal bounty relies on).

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::opv_world::opv_example;
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1, proof_seal_v1};

#[test]
fn g14r_an_identical_proof_reseal_keeps_the_first_seal_and_its_precedence() {
    let mut w = World::with_opv(policy(), opv_example());
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the lie is provable") };
    let seal = proof_seal_v1(&id, &OUTSIDER, &proof);

    w.block(11, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal }]);
    assert_eq!(w.l.proof_seals[&(id, OUTSIDER)].daa, 11);
    let seals = w.l.proof_seals.clone();
    // The restarted verifier's second copy of the same seal: nothing changes, not even the root.
    let ev = w.block(12, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal }]);
    assert!(refused(&ev).is_none(), "an identical re-seal is accepted as a no-op: {ev:?}");
    assert_eq!(w.l.proof_seals[&(id, OUTSIDER)].daa, 11, "the first seal's DAA stands");
    assert_eq!(w.l.proof_seals, seals, "an identical re-seal writes no seal row");

    // A seal of OTHER bytes still replaces it and restarts the clock.
    let other = proof_seal_v1(&id, &OUTSIDER, &ProsecutionV1::Kernel(vec![1, 2, 3]));
    w.block(13, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal: other }]);
    assert_eq!(w.l.proof_seals[&(id, OUTSIDER)].daa, 13, "another proof's seal replaces it");
    // Back to the true proof (a re-seal of other bytes again): its clock restarts, then it files and takes the bounty.
    w.block(14, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal }]);
    assert_eq!(w.l.proof_seals[&(id, OUTSIDER)].daa, 14);
    let ev = w.block(16, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted(&ev).is_some(), "convicted: {ev:?}");
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { accuser, .. } if *accuser == OUTSIDER)), "{ev:?}");
}
