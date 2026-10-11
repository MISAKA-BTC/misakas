//! **RFC-0002 P0 residual (lane RFC02-T): the kernel route's own FileProof admits copy-by-re-encoding theft — and the sealed-bounty
//! rule that closes it** (`misaka_palw_kernel::sealed_bounty`).
//!
//! A `Kernel` fault proof's `scalar` field is read only for a MatMul scalar fault: any other kind convicts identically with any
//! `scalar`, so one conviction has many byte encodings. GAP-R7 pays the earliest sealer of the EXACT bytes, else the filer.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::opv_world::opv_example;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1, proof_seal_v1};
use misaka_palw_kernel::public::FaultProofWireV1;
use misaka_palw_kernel::sealed_bounty::SealedBountyPolicyV1;

const COPYIST: Digest = SPAM1;

/// The same conviction under other bytes: a non-MatMul fault proof with another (unread) `scalar`.
fn reencode(proof: &ProsecutionV1) -> ProsecutionV1 {
    let ProsecutionV1::Kernel(bytes) = proof else { panic!("a kernel fault proof") };
    let mut wire: FaultProofWireV1 = borsh::from_slice(bytes).unwrap();
    if wire.kind == 2 {
        // A MatMul scalar court reads only the three openings: an extra (unread) prior row is another encoding.
        let extra = wire.output.clone();
        wire.prior_rows.push(extra);
    } else {
        // Every other kind ignores `scalar`.
        wire.scalar = (7, 7, 7);
    }
    let other = ProsecutionV1::Kernel(wire.to_bytes());
    assert_ne!(&other, proof);
    other
}

/// What the consumer's bond book has paid `bond` so far (accuser rewards; these bonds earn nothing else here).
fn paid(w: &World, bond: &Digest) -> u64 {
    w.consumer.paid(bond)
}

/// A lying claim, the outsider's authentic proof and its re-encoding; the finder's seal is mature at DAA 12.
fn world(policy_in_force: bool) -> (World, Digest, ProsecutionV1, ProsecutionV1) {
    let mut w = World::with_opv(policy(), opv_example());
    if policy_in_force {
        w.l.sealed_bounty_policy = Some(SealedBountyPolicyV1 { activation_daa: 0, contest_daa: 20 });
    }
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the lie is provable") };
    let copy = reencode(&proof);
    w.block(11, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal: proof_seal_v1(&id, &OUTSIDER, &proof) }]);
    (w, id, proof, copy)
}

#[test]
fn rfc02t_frozen_gap_r7_a_reencoded_copy_filed_first_takes_the_sealed_finders_bounty() {
    let (mut w, id, proof, copy) = world(false);
    // The finder files its sealed bytes; the copyist read them and its re-encoding is ordered first in the same block.
    let ev = w.block(
        13,
        vec![
            T::FileProof { accuser: COPYIST, claim: id, proof: copy },
            T::FileProof { accuser: OUTSIDER, claim: id, proof },
        ],
    );
    let (_, reward, _) = convicted(&ev).expect("convicted");
    assert!(reward > 0);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { accuser, .. } if *accuser == COPYIST)), "{ev:?}");
    assert_eq!(paid(&w, &COPYIST), reward, "COUNTEREXAMPLE: the copyist is paid");
    assert_eq!(paid(&w, &OUTSIDER), 0, "and the finder, whose seal matured first, gets nothing");
    assert!(ev.iter().any(|e| matches!(e, E::Duplicate { .. })), "the finder's own filing is a duplicate");
}

#[test]
fn rfc02t_sealed_bounty_holds_an_unsealed_conviction_and_the_earlier_sealer_takes_it() {
    let (mut w, id, proof, copy) = world(true);
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(
        13,
        vec![
            T::FileProof { accuser: COPYIST, claim: id, proof: copy.clone() },
            T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() },
        ],
    );
    // The conviction is unchanged (liveness): the copyist's authentic re-encoding convicts and the producer is slashed once.
    let (slashed, reward, _) = convicted(&ev).expect("convicted");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - slashed);
    // Nobody sealed the copy's bytes: its share is held, not paid.
    assert_eq!(paid(&w, &COPYIST), 0, "the copyist is not paid");
    // (Held at the conviction; the finder's contest later in the same block resolved it, so the row is already gone.)
    assert!(
        ev.iter().any(|e| matches!(e, E::BountyHeld { filer, amount, until_daa, .. }
            if *filer == COPYIST && *amount == reward && *until_daa == 13 + 20)),
        "{ev:?}"
    );
    // The finder's filing, ordered after the conviction in the same block, contested it: its seal predates the conviction.
    assert_eq!(paid(&w, &OUTSIDER), reward, "the earlier sealer's own authentic proof takes the held share");
    assert!(ev.iter().any(|e| matches!(e, E::HeldBountyPaid { accuser, .. } if *accuser == OUTSIDER)), "{ev:?}");
    assert!(w.l.held_bounties_v1().is_empty() && w.l.proof_seals.is_empty(), "paid once; the claim's seals are spent");
    // Nothing more to take: a further filing is a duplicate, no second payment.
    let ev = w.block(14, vec![T::FileProof { accuser: COPYIST, claim: id, proof: copy }]);
    assert!(ev.iter().any(|e| matches!(e, E::Duplicate { .. })), "{ev:?}");
    assert_eq!((paid(&w, &COPYIST), paid(&w, &OUTSIDER)), (0, reward), "no second payment");
}

#[test]
fn rfc02t_sealed_bounty_a_late_seal_cannot_contest_and_an_unsealed_honest_filer_is_paid_after_the_window() {
    let (mut w, id, proof, copy) = world(true);
    // Here the copyist convicts first, alone; the finder does not reveal. A seal posted AFTER the conviction cannot contest.
    let ev = w.block(13, vec![T::FileProof { accuser: COPYIST, claim: id, proof: copy.clone() }]);
    let (_, reward, _) = convicted(&ev).expect("convicted");
    assert_eq!(paid(&w, &COPYIST), 0);
    let ev = w.block(14, vec![T::SealProof { accuser: SPAM2, claim: id, seal: proof_seal_v1(&id, &SPAM2, &proof) }]);
    assert!(refused(&ev).is_some(), "a convicted claim takes no new seal: {ev:?}");
    let ev = w.block(16, vec![T::FileProof { accuser: SPAM2, claim: id, proof: proof.clone() }]);
    assert!(refused(&ev).is_some_and(|why| why.contains("seal")), "no seal of its own predating the conviction: {ev:?}");
    // A wrong proof under the finder's seal is refused free (it does not open the seal).
    let ev = w.block(17, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof: copy }]);
    assert!(refused(&ev).is_some_and(|why| why.contains("open")), "{ev:?}");
    // The window closes with no contest: the original filer is paid (one window late), once.
    for daa in 18..=33 {
        w.block(daa, vec![]);
    }
    assert_eq!(paid(&w, &COPYIST), reward);
    assert!(w.l.held_bounties_v1().is_empty());
}

#[test]
fn rfc02t_sealed_bounty_leaves_gap_r7_alone_when_the_convicting_bytes_were_sealed() {
    let (mut w, id, proof, _) = world(true);
    // The copyist files the finder's EXACT bytes first: the finder's mature seal of them takes the bounty at once, nothing is held.
    let ev = w.block(13, vec![T::FileProof { accuser: COPYIST, claim: id, proof }]);
    let (_, reward, _) = convicted(&ev).expect("convicted");
    assert_eq!((paid(&w, &OUTSIDER), paid(&w, &COPYIST)), (reward, 0));
    assert!(w.l.held_bounties_v1().is_empty());
}
