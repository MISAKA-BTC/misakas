//! **G14R round 3 — ECON's M\*-49 verifier pay and the held default share (O2)** at the kernel's reference level, behind the
//! consumer-injected `palw_verifier_pay_v1` terms (readiness §3f). Every block runs through the shared `Consumer`, whose book must
//! equal the ledger after every block (escrow, fee, burn and payouts conserve exactly).

mod common;

use common::chain::T;
use common::ledger_world::*;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, OutsiderFindingV1, proof_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::verifier_pay::VerifierPayPolicyV1;

const FEE: u64 = 3;
const CAP: u64 = 40;
const SRC: u64 = 29;

/// A world at ADR-0032's 49 % with the verifier-pay terms in force (`F` = 3, `m` = 4, `B_cap` = 40).
fn world() -> World {
    let mut pol = policy();
    pol.accuser_reward_permille = 490;
    let mut w = World::with(pol);
    w.l.verifier_pay_policy = Some(VerifierPayPolicyV1 {
        activation_daa: 0,
        check_fee: FEE,
        slots: 4,
        bounty_cap: CAP,
        source_deposit: SRC,
        max_open_attempts_per_class: 2,
    });
    w
}

/// **A fresh outsider under the verifier-pay terms**: a node that replays the chain with the same injected terms (the draws are the
/// consumer's calls, taken from the live ledger as a node takes them from its draw), then checks `claim` from `da` alone.
fn outsider(w: &World, claim: Digest, da: &Da) -> OutsiderFindingV1 {
    let mut fresh = w.genesis.clone();
    fresh.verifier_pay_policy = w.l.verifier_pay_policy;
    for b in &w.blocks {
        fresh.apply_block(b);
    }
    for (k, row) in &w.l.verifier_pay {
        if k.0 == misaka_palw_kernel::verifier_pay::VERIFIER_PAY_DRAW_V1 {
            fresh.verifier_pay.insert(*k, row.clone());
        }
    }
    assert_eq!(fresh.root(), w.l.root(), "a fresh node under the same terms reaches the same state");
    misaka_palw_kernel::ledger::OutsiderV1 { ledger: &fresh, claim, material: da, artifact: &w.params, salt: [0x5A; 64] }
        .check()
        .unwrap()
}

/// Feed a consumer-called ledger method's events to the book (as the node's fold applies them).
fn absorb(w: &mut World, ev: Vec<E>) -> Vec<E> {
    w.consumer.absorb(&w.l, &ev);
    w.consumer.check_view(&w.l);
    ev
}

fn held_events(ev: &[E]) -> Vec<(u64, &'static str)> {
    ev.iter().filter_map(|e| if let E::DefaultShareHeld { held, outcome, .. } = e { Some((*held, *outcome)) } else { None }).collect()
}

/// **ECON's measured case (§2.2, the 441-vs-490 residual), closed by O2.** The producer's Sybil demands at once and the producer
/// stays silent: the default collects only its burned part (51); the demander's share (49) stays reserved on the producer's bond. An
/// honest outsider's proof inside the horizon convicts, and the one pool is the share of EVERYTHING collected: the honest accuser is
/// paid 490 — as if no default had come first — and the Sybil demander nothing; every reporter together ≤ 49 % of 1,000.
#[test]
fn o2_after_a_self_inflicted_default_the_honest_accuser_is_paid_490_and_the_coalition_recovers_at_most_49_percent() {
    let mut w = world();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, full) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &full) else { panic!() };
    w.block(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 }]);
    let producer0 = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(31, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { penalty: 100, .. })), "{ev:?}");
    assert_eq!(held_events(&ev), vec![(49, "held")], "the demander's share is held");
    assert_eq!(w.consumer.paid(&SPAM1), 0, "nothing paid at the default");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 51, "only the burned part is collected now");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 949, "the held share stays reserved with the rest");
    let ev = w.block(60, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((949, 490, false)), "{ev:?}");
    assert_eq!(held_events(&ev), vec![(49, "joined_the_pool")]);
    assert_eq!(w.consumer.paid(&OUTSIDER), 490, "the honest accuser: as if no default had come first");
    assert_eq!(w.consumer.paid(&SPAM1), 0, "the Sybil demander: nothing");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 1000, "the whole reservation collected");
    assert!(held_events(&w.block(300, vec![])).is_empty(), "nothing left to pay at the horizon");
}

/// **O2 without a conviction**: the held share is collected at the liability horizon and paid to the demanders (equally, the
/// remainder burned) — a true DA default's demanders are paid, `liability_daa` later.
#[test]
fn o2_with_no_conviction_the_held_share_is_paid_to_the_demanders_at_the_horizon() {
    let mut w = world();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(
        11,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 1 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 },
        ],
    );
    let producer0 = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(31, vec![]);
    assert_eq!(held_events(&ev), vec![(48, "held")], "two demanders: 24 each, held");
    let until = w.l.claims[&id].liability_until.unwrap();
    assert!(held_events(&w.block(until, vec![])).is_empty(), "held through the horizon's last DAA");
    let ev = w.block(until + 1, vec![]);
    assert_eq!(held_events(&ev), vec![(48, "paid_to_demanders")]);
    assert_eq!((w.consumer.paid(&OUTSIDER), w.consumer.paid(&SPAM1)), (24, 24));
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer0 - 100, "the penalty, collected in two steps");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0, "and the rest released");
}

/// **C4R4's F-C4R4-15 conditions with O2 in force**: a Sybil seals the canonical proof early and a Sybil joins the outsider's demand.
/// The demanders get nothing (the share joined the pool) and the Sybil's earlier seal takes the bounty: the coalition recovers at most
/// 49 % of what the default and the conviction collected.
#[test]
fn o2_the_f_c4r4_15_coalition_recovers_at_most_49_percent() {
    let mut w = world();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, full) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &full) else { panic!() };
    w.block(11, vec![T::SealProof { accuser: SPAM2, claim: id, seal: proof_seal_v1(&id, &SPAM2, &proof) }]);
    w.block(
        12,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 },
        ],
    );
    let producer0 = w.l.bonds[&PRODUCER].collateral;
    w.block(32, vec![]);
    let ev = w.block(60, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted(&ev).is_some(), "{ev:?}");
    let collected = producer0 - w.l.bonds[&PRODUCER].collateral; // the default's burned part (at 32) and the conviction's slash
    let recovered = w.consumer.paid(&SPAM1) + w.consumer.paid(&SPAM2);
    assert_eq!(collected, 1000);
    assert!(recovered * 1000 <= collected * 490, "the coalition recovered {recovered} of {collected}");
}

/// **M\*-49: the check fee.** A job escrows `m × F` beside its reward; the draw keeps `F` per drawn slot and returns the rest; a
/// slot that attests is paid `F` from the poster's escrow; at the claim's Final an unattested slot's fee returns to the poster. The
/// poster is debited exactly the fees paid; nothing is issued.
#[test]
fn m49_the_check_fee_is_escrowed_paid_on_attestation_and_returned_otherwise() {
    let mut w = world();
    let poster0 = (w.l.bonds[&common::chain::POSTER].collateral, w.l.bonds[&common::chain::POSTER].reserved);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let poster = common::chain::POSTER;
    let reward_escrow = w.l.policy.claim_reward;
    assert_eq!(w.l.bonds[&poster].reserved, poster0.1 + reward_escrow + 4 * FEE, "m × F escrowed beside the reward");
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    let ev = w.l.assign_drawn_v1(&id, &[OUTSIDER, SPAM2]).unwrap();
    absorb(&mut w, ev);
    assert_eq!(w.l.bonds[&poster].reserved, poster0.1 + reward_escrow + 2 * FEE, "two undrawn slots returned");
    let ev = w.l.attest_drawn_v1(&id, &OUTSIDER).unwrap();
    let ev = absorb(&mut w, ev);
    assert!(ev.contains(&E::CheckFeePaid { claim: id, verifier: OUTSIDER, amount: FEE }), "{ev:?}");
    assert!(w.l.attest_drawn_v1(&id, &OUTSIDER).is_err(), "a slot is paid once");
    w.block(60, vec![]); // Final: SPAM2 never attested
    assert_eq!(w.consumer.paid(&OUTSIDER), FEE);
    assert_eq!(w.consumer.paid(&SPAM2), 0, "an unattested slot is not paid");
    assert_eq!(w.l.bonds[&poster].reserved, poster0.1, "every escrow spent or returned");
    let job_fee = w.l.policy.job_fee;
    assert_eq!(w.l.bonds[&poster].collateral, poster0.0 - job_fee - reward_escrow - FEE, "debited exactly the reward and one fee");
}

/// **M\*-49 pay on fate, and drawn sealers first inside the 49 %.** A lying claim drawn to two slots: the conviction before the
/// deadline pays both slots their fee; the drawn slot that sealed the convicting bytes takes `B_cap` first, the earliest sealer (here
/// the same slot) the rest; the bounty's total is unchanged (49 % of the slash).
#[test]
fn m49_a_conviction_pays_every_drawn_slot_and_drawn_sealers_share_the_cap_first() {
    let mut w = world();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, full) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &full) else { panic!() };
    let ev = w.l.assign_drawn_v1(&id, &[OUTSIDER, SPAM2]).unwrap();
    absorb(&mut w, ev);
    w.block(11, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal: proof_seal_v1(&id, &OUTSIDER, &proof) }]);
    let ev = w.block(20, vec![T::FileProof { accuser: SPAM1, claim: id, proof }]);
    let (slashed, reward, _) = convicted(&ev).expect("convicted");
    assert_eq!(reward, slashed * 490 / 1000, "the bounty's total is the 49 %");
    let fees: Vec<(Digest, u64)> = ev
        .iter()
        .filter_map(|e| if let E::CheckFeePaid { verifier, amount, .. } = e { Some((*verifier, *amount)) } else { None })
        .collect();
    assert_eq!(fees, vec![(OUTSIDER, FEE), (SPAM2, FEE)], "pay on fate: both drawn slots");
    assert_eq!(w.consumer.paid(&OUTSIDER), FEE + reward, "the drawn sealer: its fee, the cap, and (earliest sealer) the rest");
    assert_eq!(w.consumer.paid(&SPAM1), 0, "the copyist filer: nothing");
    assert!(matches!(w.state(&id), ClaimStateV1::Convicted { .. }));
}

/// **ECON §4.4 S3/S4 — one deposit, many attempts (F-ECON-3), closed.** A withheld beacon-source seal costs `d_src` (the race seal
/// `d` plus the opt-in's extra, both forfeited at expiry), feeds exactly ONE attempt (a second opt-in of the same seal is refused, so
/// one deposit vetoes at most one attempt), and a class's beacon-source seals feed at most `N_max` open attempts. The 1-BILI job-race
/// seal is no attempt's source.
#[test]
fn s3_s4_one_deposit_buys_at_most_one_attempts_worth_of_veto() {
    use common::opv_world::{HONEST, SQUATTER};
    use misaka_palw_kernel::ledger::claim_seal_v2;
    let mut w = World::new_opv();
    w.l.verifier_pay_policy = Some(VerifierPayPolicyV1 {
        activation_daa: 0,
        check_fee: FEE,
        slots: 4,
        bounty_cap: CAP,
        source_deposit: SRC,
        max_open_attempts_per_class: 2,
    });
    let d = w.l.policy.seal_deposit;
    let (a, b, c) = ([0xA1; 64], [0xB2; 64], [0xC3; 64]);
    let jobs: Vec<_> = (1..=3u8).map(|n| w.post_job(1 + u64::from(n), &[3, 17, 9], 3, n)).collect();
    let seal = |n: u8| claim_seal_v2(&[n; 64], &[0x77; 64]);
    w.block(
        10,
        vec![
            T::SealClaim { producer: SQUATTER, job: jobs[0].id(), seal: seal(1) },
            T::SealClaim { producer: HONEST, job: jobs[1].id(), seal: seal(2) },
            T::SealClaim { producer: SPAM1, job: jobs[2].id(), seal: seal(3) },
        ],
    );
    let (c0, r0) = (w.l.bonds[&SQUATTER].collateral, w.l.bonds[&SQUATTER].reserved);
    let ev = w.l.opt_in_beacon_source_v1(&SQUATTER, &jobs[0].id(), &a).unwrap();
    absorb(&mut w, ev);
    assert_eq!(w.l.bonds[&SQUATTER].reserved, r0 + (SRC - d), "the opt-in reserves d_src − d beside the race seal's d");
    let again = w.l.opt_in_beacon_source_v1(&SQUATTER, &jobs[0].id(), &b).unwrap_err();
    assert!(again.why.contains("one deposit, one attempt"), "{again:?}");
    // S4: the class feeds attempts A and B; a third open attempt is refused.
    let ev = w.l.opt_in_beacon_source_v1(&HONEST, &jobs[1].id(), &b).unwrap();
    absorb(&mut w, ev);
    let third = w.l.opt_in_beacon_source_v1(&SPAM1, &jobs[2].id(), &c).unwrap_err();
    assert!(third.why.contains("S4"), "{third:?}");
    let src = |w: &World, at: &Digest| w.l.beacon_source_seals_for_attempt_v1(at).iter().map(|s| s.producer).collect::<Vec<_>>();
    assert_eq!((src(&w, &a), src(&w, &b), src(&w, &c)), (vec![SQUATTER], vec![HONEST], vec![]), "each seal feeds one attempt");
    // The squatter withholds: at expiry it forfeits d_src in all, and its veto stays with attempt A alone.
    let ttl = w.l.policy.seal_ttl_daa;
    w.block(10 + ttl + 1, vec![]);
    assert_eq!(w.l.bonds[&SQUATTER].collateral, c0 - SRC, "the withheld beacon-source seal cost d_src");
    assert_eq!(w.l.bonds[&SQUATTER].reserved, r0 - d, "nothing of it is left reserved");
    assert_eq!(src(&w, &a), vec![SQUATTER], "forfeited, it vetoes attempt A");
    assert!(!src(&w, &b).contains(&SQUATTER) && src(&w, &c).is_empty(), "and no other attempt");
    // The race seal that never opted in (SPAM1's) is no attempt's source.
    assert!([a, b, c].iter().all(|at| !src(&w, at).contains(&SPAM1)));
}
