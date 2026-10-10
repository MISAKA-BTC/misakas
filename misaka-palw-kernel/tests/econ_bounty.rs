//! **ECON — the honest verifier's incentive (PRINCIPLES §6 condition 6) and the bounty's self-dealing bound, at ADR-0032's 49%.**
//!
//! Design and proofs: `docs/design/palw/econ-verifier-incentive-and-allocation.md` (§2 self-dealing, §3 the verifier's incentive).
//! The closed forms and every parameter sweep live in the executable model `scripts/misaka-palw-econ-verifier-model.py`; this file
//! holds only what is a claim about consensus code and so needs the ledger to prove it.
//!
//! The OPV test world with the accuser share set to the future ruleset's **490‰** (ADR-0032's 2026-10-10 amendment; the kernel
//! route's interim value is 500‰): reservation `K = 1,000`, default penalty `D = 100` burned at `β_d = max(1000 − a,
//! default_burn) = 510‰`, admission fee 3, Final reward 7, job fee 2 (the poster's). Amounts are the test ledger's whole units.
//!
//! What the ledger shows (MEASURED here):
//!
//! * **M0 — capture.** The earliest seal of the exact convicting bytes takes the bounty. A liar's own Sybil seals its proof one block
//!   after the commit (a proof seal has no deposit and no fee), so it takes the bounty of every conviction of that claim, on every
//!   path, and the honest verifier who found the lie is paid nothing. On an honest claim a verifier is paid nothing either.
//! * **E1 — the 49% does not bound a self-dealer on the default path.** After a self-inflicted pre-Final default the coalition
//!   recoups the demanders' share `D·(1 − β_d) = 49` AND the bounty `min(a·K, K − D) = 490`: 539 of the 1,000 collected, so its net
//!   loss is 461 = 46.1% of the collected amount, below ADR-0032's "at least 51%". Direct and post-Final paths lose exactly 51%.
//! * Conservation: on every path the burn plus the payouts equal what was collected plus the fees.
//!
//! Nothing here changes consensus code. Interim values are not production values.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::opv_world::opv_example;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerEventV1 as E, LedgerPolicyV1, OutsiderFindingV1, OutsiderV1, proof_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;

/// The future ruleset's reporter share (ADR-0032, 4,900 bps), as the kernel route's permille.
const A_PERMILLE: u16 = 490;

/// The OPV test world at the 49% share.
fn world_49() -> World {
    let w = World::with_opv(LedgerPolicyV1 { accuser_reward_permille: A_PERMILLE, ..policy() }, opv_example());
    assert_eq!(w.l.policy.accuser_reward_permille, A_PERMILLE);
    w
}

/// What a set of bonds holds: locked collateral plus what the consumer paid out to their owners (rewards, shares, bounties).
fn wealth(w: &World, bonds: &[Digest]) -> i128 {
    bonds.iter().map(|b| w.l.bonds.get(b).map_or(0, |r| r.collateral) as i128 + w.consumer.paid(b) as i128).sum()
}

/// How the coalition `{PRODUCER, SPAM1}` reaches its own conviction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Path {
    /// The lie is convicted inside its window (an honest verifier finds it; the Sybil files its earlier-sealed proof first).
    Direct,
    /// The Sybil demands a position and the producer withholds (a pre-Final default: the Sybil takes the demanders' share of the
    /// penalty); then the Sybil's sealed proof convicts in the default's liability horizon.
    AfterSelfDefault,
    /// The lie reaches Final (the producer is paid the poster's escrow); then the Sybil's sealed proof convicts post-Final.
    AfterFinal,
}

struct Outcome {
    honest_paid: u64,
    sybil_paid: u64,
    /// The coalition's net over the whole episode (admission fee, reward, slash, bounty, shares).
    coalition_net: i128,
    /// The proof bytes do not depend on the verifier's salt.
    canonical: bool,
    /// What the episode burned (with the poster's job fee and the producer's admission fee).
    burned: u64,
    /// What the claim's producer lost to the default and the conviction together (the collected amount).
    collected: u64,
}

/// **Today: the earliest seal of the convicting bytes takes the bounty** (`KernelLedgerV1::bounty_holder`).
fn run_m0(path: Path) -> Outcome {
    let mut w = world_49();
    let coalition = [PRODUCER, SPAM1];
    let c0 = wealth(&w, &coalition);
    let honest0 = wealth(&w, &[OUTSIDER]);
    let (burned0, producer0) = (w.l.burned, w.l.bonds[&PRODUCER].collateral);

    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);

    // The Sybil's proof (its own salt) and the honest outsider's (another salt), each from a fresh replay of the public chain.
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    let OutsiderFindingV1::Prosecute(sybil_proof) =
        OutsiderV1 { ledger: &fresh, claim: id, material: &da, artifact: &w.params, salt: [0x11; 64] }.check().unwrap()
    else {
        panic!("the lie is provable")
    };
    let OutsiderFindingV1::Prosecute(honest_proof) = outsider(&w, id, &da) else { panic!("the lie is provable") };
    let canonical = sybil_proof == honest_proof;

    // The Sybil seals at once (and, on the default path, demands the lying position).
    let mut txs = vec![T::SealProof { accuser: SPAM1, claim: id, seal: proof_seal_v1(&id, &SPAM1, &sybil_proof) }];
    if path == Path::AfterSelfDefault {
        txs.push(T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 });
    }
    w.block(11, txs);

    let (seal_at, file_at) = match path {
        Path::Direct => (30, 31),
        Path::AfterSelfDefault => {
            let ev = w.block(31, vec![]);
            assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id)), "{ev:?}");
            assert!(matches!(w.state(&id), ClaimStateV1::Unavailable { .. }));
            (40, 41)
        }
        Path::AfterFinal => {
            let ev = w.block(60, vec![]);
            assert!(ev.iter().any(|e| matches!(e, E::Final { claim, .. } if *claim == id)), "the lie reaches Final: {ev:?}");
            (61, 62)
        }
    };
    // The honest outsider seals (naming the claim) and files a block later — after the Sybil's filing in the same block.
    w.block(seal_at, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal: proof_seal_v1(&id, &OUTSIDER, &honest_proof) }]);
    let ev = w.block(
        file_at,
        vec![
            T::FileProof { accuser: SPAM1, claim: id, proof: sybil_proof },
            T::FileProof { accuser: OUTSIDER, claim: id, proof: honest_proof },
        ],
    );
    assert!(convicted(&ev).is_some(), "the lie is convicted: {ev:?}");
    assert!(ev.contains(&E::Duplicate { claim: id }), "the honest filing comes second: a duplicate, no fee: {ev:?}");
    assert_eq!(wealth(&w, &[OUTSIDER]), honest0, "the honest verifier neither paid a fee nor was paid anything");
    // The producer lost the admission fee, the default penalty (if any) and the conviction's slash; the reward it was paid after
    // Final is in `paid`, not in its collateral.
    let collected = producer0 - w.l.bonds[&PRODUCER].collateral - 3;
    Outcome {
        honest_paid: w.consumer.paid(&OUTSIDER),
        sybil_paid: w.consumer.paid(&SPAM1),
        coalition_net: wealth(&w, &coalition) - c0,
        canonical,
        burned: w.l.burned - burned0,
        collected,
    }
}

/// The closed form of the conviction loop (slash, bounty, demanders' share; fees and the reward excluded), the model's `loop_net`:
/// direct `−K + ⌊a·K⌋`; after a self-inflicted default `−K + (D − ⌊D·β_d⌋) + min(⌊a·K⌋, K − D)`.
fn loop_net(k: i128, a: i128, d: i128, beta_d: i128, after_default: bool) -> i128 {
    let bounty = k * a / 1000;
    if after_default { -k + (d - d * beta_d / 1000) + bounty.min(k - d) } else { -k + bounty }
}

/// **PoC — capture and the E1 leak at 49%, on every self-dealing path.**
///
/// | path | coalition net | of which the loop | Sybil paid | collected | loss / collected |
/// |---|---|---|---|---|---|
/// | direct | −513 | −510 = −(1 − a)·K | 490 | 1,000 | 51.0% |
/// | after a self-inflicted default | −464 | −461 = −K + 49 + 490 | 539 | 1,000 | **46.1%** (E1) |
/// | after Final | −506 | −510, +7 reward from the poster's escrow | 490 | 1,000 | 51.0% (50.3% with the reward) |
///
/// The honest verifier is paid 0 on every path. Burn + payouts = collected + the job fee 2 + the admission fee 3 (+ the reward 7,
/// paid from the poster's escrow, after Final).
#[test]
fn at_49_percent_a_liars_own_sybil_takes_every_bounty_and_the_default_path_recoups_more_than_49_percent() {
    let (k, a, d, beta_d) = (1_000i128, A_PERMILLE as i128, 100i128, 1_000 - A_PERMILLE as i128);
    let (admission_fee, reward, job_fee) = (3i128, 7i128, 2u64);
    for (path, closed) in [
        (Path::Direct, loop_net(k, a, d, beta_d, false)),
        (Path::AfterSelfDefault, loop_net(k, a, d, beta_d, true)),
        (Path::AfterFinal, loop_net(k, a, d, beta_d, false) + reward),
    ] {
        let o = run_m0(path);
        assert!(o.canonical, "the convicting bytes do not depend on the verifier's salt");
        assert_eq!(o.honest_paid, 0, "{path:?}: the honest verifier is paid nothing");
        assert_eq!(o.coalition_net, closed - admission_fee, "{path:?}: the coalition's net is the model's closed form");
        assert!(o.coalition_net < 0, "{path:?}: self-dealing never pays");
        assert_eq!(o.collected, 1_000, "{path:?}: the producer lost its whole reservation (penalty + slash)");
        // Conservation: what was collected and the fees are either burned or paid to the Sybil.
        assert_eq!(
            o.burned + o.sybil_paid,
            o.collected + job_fee + admission_fee as u64,
            "{path:?}: burn + payouts = collected + fees"
        );
        let loop_loss = -(o.coalition_net + admission_fee - if path == Path::AfterFinal { reward } else { 0 });
        eprintln!(
            "[ECON M0 49%] {path:?}: honest paid {}, Sybil paid {}, coalition net {}, burned {}, loss/collected {:.1}%",
            o.honest_paid,
            o.sybil_paid,
            o.coalition_net,
            o.burned,
            100.0 * loop_loss as f64 / o.collected as f64
        );
        match path {
            Path::Direct | Path::AfterFinal => {
                assert_eq!(o.sybil_paid, 490);
                assert_eq!(loop_loss, 510, "{path:?}: exactly 51% of the collected amount");
            }
            Path::AfterSelfDefault => {
                assert_eq!(o.sybil_paid, 539, "bounty 490 + the demanders' share 49");
                // E1: the self-dealer recoups 53.9% of what was collected — more than ADR-0032's 49%.
                assert_eq!(loop_loss, 461);
                assert!(loop_loss * 1000 < 510 * o.collected as i128, "E1: the net loss is below 51% of the collected amount");
            }
        }
    }
}

/// **PoC — on an honest claim a verifier is paid nothing at all.** Three honest OPV claims, each checked clean by an outsider, reach
/// Final: the outsider's revenue is 0 whatever it spent. Today a verifier's only income is a bounty, i.e. a lie (T0's premise).
#[test]
fn checking_honest_claims_pays_the_verifier_nothing() {
    let mut w = world_49();
    let honest0 = wealth(&w, &[OUTSIDER]);
    let jobs: Vec<_> = (0..3u8).map(|n| w.post_job(2 + n as u64, &[3, 17, 9], 3, 10 + n)).collect();
    let mut ids = vec![];
    for (n, job) in jobs.iter().enumerate() {
        let p = w.honest(job, 3);
        let (id, da) = (p.claim.id(), Da::publishing(&p.trace, &[]));
        w.block(10 + n as u64, vec![p.tx]);
        assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Clean, "the outsider checks every claim");
        ids.push(id);
    }
    w.block(70, vec![]);
    for id in &ids {
        assert!(matches!(w.state(id), ClaimStateV1::Final { .. }));
    }
    assert_eq!(wealth(&w, &[OUTSIDER]), honest0, "three checks, zero revenue");
}
