//! **ECON — GAP-B12: what one vetoing seal buys, measured against the sealed-source beacon v3 collector.**
//!
//! Design and derivation: `docs/design/palw/econ-verifier-incentive-and-allocation.md` §4; the derivation itself (stall, cost,
//! damage, the deposit) is the executable model `scripts/misaka-palw-econ-verifier-model.py`. This file measures the two facts about
//! consensus code that derivation reads:
//!
//! * **F-ECON-3 — one withheld seal vetoes every concurrent attempt** whose seal window contains it (sources are shared), so the
//!   price of a veto per victim is `d / N_c`.
//! * **T_ab — an abandoned mixed seal stalls longer than a withheld one.** The attacker reveals on the reveal window's last DAA, its
//!   own Sybil demands a position on the claim's window's last DAA, and the producer withholds: the claim defaults at the demand's
//!   deadline, the source turns `Failed`, and the attempt ends `BEACON_VETOED` there: `T_ab = anchor + 2W + window + court − 1 =
//!   151` DAA from the commitment to the next attempt (a withheld seal: `T_w = anchor + 2W + 1 = 83`). Its price to the coalition is
//!   the admission fee and the burned part of the default penalty, `D·β_d` (the Sybil's demand bond returns, the seal deposit returns
//!   at the reveal, the Sybil keeps the demanders' share).
//!
//! The ledger runs G14-R4's bonded, salted claim seals through the shared consumer; the facts are derived exactly as the node's
//! `beacon_sealed_sources_v1` does (positions = DAA); OPV-BOOT's `collect_sealed_work_beacon_v3` decides each attempt. The beacon
//! policy has the interim sealed policy's shape (`k = 2`, anchor delay 2, `W = 40`, `D = 2`). F-ECON-1/2 (a re-seal's free
//! withdrawal) were fixed by G14R (`9429128f8`, S1) and are tested there. The accuser share is the future ruleset's 490‰.
//!
//! Nothing here changes consensus code. Interim values are not production values.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::opv_world::{SQUATTER, opv_example};
use misaka_palw_challenge::{
    BeaconContextV1, RootV1, SealRevealV3, SealedBeaconStateV3, SealedSourceV3, SourceAttributionV1, SourceFateV3, SubjectKindV1,
    collect_sealed_work_beacon_v3, sealed_source_policy_v3,
};
use misaka_palw_kernel::hash::{Digest, object_id};
use misaka_palw_kernel::job::KernelJobV1;
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, LedgerPolicyV1, SaltedCommitV1, claim_seal_v2};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::opv::WorkFinalContextV1;

/// The interim sealed onboarding policy's shape (`palw_onboarding_sealed_policy_v1`): `k`, anchor delay, `W`, `D`.
const K: u32 = 2;
const ANCHOR: u64 = 2;
const W: u64 = 40;
const DEPTH: u64 = 2;

/// Two honest source producers and a second attacker bond (registered with room for several OPV reservations).
const H1: Digest = [0x71; 64];
const H2: Digest = [0x72; 64];
const ATT2: Digest = [0xA7; 64];

/// A conformance subject committed at `commitment` whose candidate is `candidate`; the world's OPV class is the one eligible source
/// profile.
fn ctx(w: &World, commitment: u64, candidate: u8) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: [0x11; 64],
        ruleset_id: [0x22; 64],
        policy: sealed_source_policy_v3(K, ANCHOR, W, DEPTH, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [candidate; 64],
        commitment_position: commitment,
        challenge_epoch: 0,
        eligible_profiles: [w.class].into_iter().collect(),
        excluded_profiles: [[candidate; 64]].into_iter().collect(),
        candidate_profile_id: RootV1::Present([candidate; 64]),
    }
}

/// **The seal facts a consumer derives from the ledger** — what the node's `beacon_sealed_sources_v1` builds from
/// `claim_beacon_seals_v1` (profile = the sealed job's class; the fate from the claim row and its Final receipt; positions = DAA).
fn facts(w: &World) -> Vec<SealedSourceV3> {
    w.l.claim_beacon_seals_v1()
        .into_iter()
        .filter_map(|s| {
            let profile = w.l.jobs.get(&s.job)?.class_binding_id;
            let reveal = s.revealed.map(|(claim, revealed_daa, salt)| {
                let fate = match (w.l.claims.get(&claim), w.l.final_receipt(&claim)) {
                    (_, Some(r)) => SourceFateV3::Final(
                        r.to_work_final_event(&WorkFinalContextV1 {
                            accepted_position: r.accepted_daa,
                            settlement_position: r.final_daa,
                            occurrence_index: 0,
                            validity_independent: true,
                            depends_on_profiles: vec![],
                            panel: None,
                        })
                        .unwrap(),
                    ),
                    (Some(row), None)
                        if row.convicted
                            || matches!(row.life.state, ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. }) =>
                    {
                        SourceFateV3::Failed
                    }
                    (Some(_), None) => SourceFateV3::Live,
                    (None, None) => SourceFateV3::Failed,
                };
                SealRevealV3 { reveal_position: revealed_daa, salt, fate }
            });
            Some(SealedSourceV3 {
                source_profile_id: profile,
                attribution: SourceAttributionV1 { producer_id: s.producer, consumer_id: RootV1::Absent },
                seal: s.seal,
                seal_position: s.sealed_daa,
                reveal,
            })
        })
        .collect()
}

fn state(w: &World, c: &BeaconContextV1, tip: u64) -> SealedBeaconStateV3 {
    collect_sealed_work_beacon_v3(c, &facts(w), tip).unwrap()
}

/// A block of harness transactions applied as they are (no automatic seal, no automatic salt: the tests place every seal).
fn raw(w: &mut World, daa: u64, txs: Vec<T>) -> Vec<E> {
    let txs = txs.into_iter().flat_map(|t| t.into_txs(PRODUCER)).collect();
    w.block_raw(daa, txs)
}

fn salt(tag: u64) -> Digest {
    object_id(b"econ/test/seal-salt", &tag)
}

/// The salted seal of a produced claim.
fn seal_of(p: &Produced, s: &Digest) -> T {
    T::SealClaim { producer: p.claim.producer_bond, job: p.claim.job_id, seal: claim_seal_v2(&p.claim.id(), s) }
}

/// The salted reveal of a produced claim.
fn reveal_of(p: &Produced, s: Digest) -> T {
    let T::CommitClaim { claim, evidence, commitments } = p.tx.clone() else { unreachable!() };
    T::CommitClaimSalted { salt: s, commit: SaltedCommitV1::Claim { claim, evidence, commitments } }
}

/// What a set of bonds holds: locked collateral plus what the consumer paid out to their owners.
fn wealth(w: &World, bonds: &[Digest]) -> i128 {
    bonds.iter().map(|b| w.l.bonds.get(b).map_or(0, |r| r.collateral) as i128 + w.consumer.paid(b) as i128).sum()
}

/// A world (accuser share 490‰) with the two honest source producers and the second attacker bond registered at DAA 2, and `jobs`
/// jobs posted from DAA 3.
fn world(jobs: u8) -> (World, Vec<KernelJobV1>) {
    let mut w = World::with_opv(LedgerPolicyV1 { accuser_reward_permille: 490, ..policy() }, opv_example());
    raw(&mut w, 2, vec![bond(H1, 20_000), bond(H2, 20_000), bond(ATT2, 20_000)]);
    let posted = (0..jobs).map(|n| w.post_job(3 + n as u64, &[3, 17, 9], 3, 0x40 + n)).collect();
    (w, posted)
}

/// **F-ECON-3: one withheld seal vetoes every attempt whose seal window contains it.** Three conformance subjects of three classes,
/// committed at 0, 10 and 25 (seal windows `[2, 42)`, `[12, 52)`, `[27, 67)`), share the source pool: two honest producers seal at 30
/// and reveal at 70 (inside every window), and ONE attacker seal at 31 is never revealed. All three attempts end `BEACON_VETOED`
/// (at 82, 92 and 107); without it none is. One deposit — three vetoes: a veto's price per victim is `d / N_c`.
#[test]
fn one_withheld_seal_vetoes_every_attempt_whose_window_contains_it() {
    let (mut w, jobs) = world(3);
    let p: Vec<Produced> = [(H1, 0), (H2, 1)].into_iter().map(|(b, j)| w.honest_by(&jobs[j], b, 3)).collect();
    raw(&mut w, 30, p.iter().enumerate().map(|(n, x)| seal_of(x, &salt(n as u64))).collect());
    raw(&mut w, 31, vec![T::SealClaim { producer: SQUATTER, job: jobs[2].id(), seal: [0x77; 64] }]);
    raw(&mut w, 70, p.iter().enumerate().map(|(n, x)| reveal_of(x, salt(n as u64))).collect());
    let subjects = [(0u64, 0xC2u8), (10, 0xC3), (25, 0xC4)];
    for (commitment, candidate) in subjects {
        let c = ctx(&w, commitment, candidate);
        let tip = commitment + ANCHOR + 2 * W;
        raw(&mut w, tip, vec![]);
        assert_eq!(state(&w, &c, tip), SealedBeaconStateV3::Vetoed { withheld: 1, failed: 0 }, "subject at {commitment}");
        let honest_only: Vec<SealedSourceV3> = facts(&w).into_iter().filter(|f| f.attribution.producer_id != SQUATTER).collect();
        assert!(!matches!(collect_sealed_work_beacon_v3(&c, &honest_only, tip).unwrap(), SealedBeaconStateV3::Vetoed { .. }));
    }
    assert_eq!(w.l.bonds[&SQUATTER].reserved, w.l.policy.seal_deposit, "one deposit behind three vetoes");
    eprintln!("[ECON F-ECON-3] one withheld seal (deposit {}) vetoed 3 concurrent attempts", w.l.policy.seal_deposit);
}

/// **T_ab measured: an abandoned mixed seal vetoes at the attacker's claim's default, `anchor + 2W + window + court − 1 = 151` DAA
/// after the commitment (to the next attempt), at a price of the admission fee plus `D·β_d`.** The subject commits at 10 (seal window
/// `[12, 52)`, reveal window `[52, 92)`). Two honest producers seal at 12 and reveal at 53. The attacker (`SQUATTER`) seals at 12 and
/// reveals on the reveal window's last DAA, 91, so its claim's window runs `[91, 141)`; its Sybil (`SPAM1`) demands a position at
/// 140, and the producer withholds: the claim defaults at 160. Until then the attempt is `Settling` (not vetoed, not lockable); at
/// 160 it is `Vetoed { failed: 1 }`; the next attempt may commit at 161. The coalition `{SQUATTER, SPAM1}` paid the admission fee 3
/// and the burned part of the penalty, `⌊100 · 510 / 1000⌋ = 51` (the Sybil kept the demanders' share 49 and its demand bond); the
/// seal deposit came back at the reveal; the rest of the reservation stays held until `160 + liability`.
#[test]
fn an_abandoned_mixed_seal_vetoes_at_its_claims_default_and_stalls_longer_than_a_withheld_one() {
    let (mut w, jobs) = world(3);
    let commitment = 10u64;
    let c = ctx(&w, commitment, 0xC5);
    let s = commitment + ANCHOR;
    let coalition = [SQUATTER, SPAM1];
    let c0 = wealth(&w, &coalition);
    let honest: Vec<Produced> = [(H1, 0), (H2, 1)].into_iter().map(|(b, j)| w.honest_by(&jobs[j], b, 3)).collect();
    let (at, lie) = w.lying_by(&jobs[2], SQUATTER, 3);
    let mut txs: Vec<T> = honest.iter().enumerate().map(|(n, p)| seal_of(p, &salt(n as u64))).collect();
    txs.push(seal_of(&lie, &salt(9)));
    raw(&mut w, s, txs);
    raw(&mut w, s + W + 1, honest.iter().enumerate().map(|(n, p)| reveal_of(p, salt(n as u64))).collect());
    // The attacker reveals on the reveal window's last DAA.
    let reveal_at = s + 2 * W - 1;
    raw(&mut w, reveal_at, vec![reveal_of(&lie, salt(9))]);
    let id = lie.claim.id();
    assert!(w.l.claims.contains_key(&id), "the attacker's claim is admitted at {reveal_at}");
    // Its own Sybil demands a position on the claim window's last DAA.
    let window = w.l.policy.challenge_window_daa;
    let demand_at = reveal_at + window - 1;
    let ev = raw(&mut w, demand_at, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 }]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { claim, .. } if *claim == id)), "{ev:?}");
    let default_at = demand_at + w.l.policy.court_deadline_daa;
    raw(&mut w, default_at - 1, vec![]);
    assert!(
        matches!(state(&w, &c, default_at - 1), SealedBeaconStateV3::Settling { .. }),
        "before the default the attempt waits: {:?}",
        state(&w, &c, default_at - 1)
    );
    let ev = raw(&mut w, default_at, vec![]);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id)), "{ev:?}");
    assert_eq!(state(&w, &c, default_at), SealedBeaconStateV3::Vetoed { withheld: 0, failed: 1 });
    // The stall: from the commitment to the next attempt's commitment.
    let t_ab = default_at + 1 - commitment;
    let (t_w, court) = (ANCHOR + 2 * W + 1, w.l.policy.court_deadline_daa);
    assert_eq!(t_ab, ANCHOR + 2 * W + window + court - 1);
    assert_eq!((t_ab, t_w), (151, 83));
    // The price.
    let paid = c0 - wealth(&w, &coalition);
    let (admission_fee, penalty, beta_d) = (3i128, w.l.policy.default_penalty as i128, 1_000i128 - 490);
    assert_eq!(paid, admission_fee + penalty * beta_d / 1000, "the coalition paid the admission fee and D·β_d");
    assert_eq!(paid, 54);
    assert_eq!(w.consumer.paid(&SPAM1), 49, "the Sybil kept the demanders' share");
    eprintln!("[ECON T_ab] abandoned mixed seal: veto at {default_at}, T_ab {t_ab} DAA (withheld T_w {t_w}); coalition paid {paid}");
}
