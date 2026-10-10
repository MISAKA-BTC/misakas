//! **C4 round 4 — kernel reference level.** Independent adversarial review of the G14-R4 fixes (escrow GAP-5, the bonded claim seals,
//! the served demand bonds, the accuser seals GAP-R7, the reserved prosecution runs) and of what the OPV beacon reads from the ledger.
//!
//! Convention (as in rounds 1–3): a defect's test asserts the SAFE property and is `#[ignore = "FAIL F-C4R4-nn …"]` until its fix;
//! an `obs_` test pins by-design or already-named behaviour, quantified. Every test drives the ledger through the shared `Consumer`, so
//! the bond book equals the ledger (settlement conservation) after every block.
//!
//! * F-C4R4-02 (OBS, watcher absence): an unverified OPV lie is paid, released and permanent; the loss is quantified.
//! * F-C4R4-03 (P1 where armed): a squatter's unrevealed claim seal froze the poster's job escrow — and so the poster's whole bond
//!   exit — for as long as the squatter kept re-sealing, at one seal deposit.
//! * F-C4R4-04 (held): the pre-Final-default liability horizon is inclusive at its edge.
//! * F-C4R4-05 (P1 where armed): one dismissed filing against a claim of the heaviest class spends the whole block's court work, so a
//!   valid proof is held out for ONE dismissal fee per block instead of the `max_adjudications × fee` the censorship relation prices.
//! * F-C4R4-06 (OBS, economics): with the served-demand-bond rule, checking an honest claim whose producer publishes nothing costs the
//!   verifier one burned demand bond per position.
//! * F-C4R4-07 (OBS, verifier incentive): a lying producer's Sybil captures the bounty of its own conviction — passively when the
//!   proof bytes are canonical, by front-running on the verifier's public seal otherwise.
//! * F-C4R4-08 (P2, beacon accounting): since the OPV caps count pre-Final claims only, a beacon window sees `hard × ⌈…/window⌉` Finals,
//!   not `max_live_claims_total`; and no ledger state names a Final job's poster (the distinct rule's consumer is always unknown).

mod common;

use common::chain::{POSTER, T};
use common::ledger_world::*;
use common::opv_world::{HONEST, SQUATTER};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelJobV1};
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, OutsiderFindingV1, OutsiderV1, ProsecutionV1, proof_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;

// ── Target 2 (watcher absence): quantify the unverified lie ───────────────────────────────────────────────────────────────

/// **OBS F-C4R4-02 (watcher absence; by design, quantified).** No honest verifier files a proof during the window OR the liability
/// horizon. The lying OPV claim finalizes by the window rule, the poster's escrow pays the producer, and once the horizon elapses
/// the reservation is returned IN FULL: the fraud is now permanent and nothing in the ledger records or penalizes it.
///
/// The economics are exactly as the readiness matrix states ("OPV still assumes at least one capable honest verifier within the
/// deadline"): the producer's only cost is the non-refundable admission fee, so an unverified lie nets `claim_reward − admission_fee`
/// and the consumer (the job's poster) has paid `claim_reward + job_fee` for a wrong answer. The watcher assumption is load-bearing,
/// not a rule the chain enforces — recorded so no one mistakes a silent window for safety.
#[test]
fn f_c4r4_02_obs_watcher_absence_leaves_an_unverified_lie_permanent_and_quantifies_the_loss() {
    let mut w = World::new_opv();
    let pol = w.l.policy.clone();
    let opv = w.l.opv.policy.expect("opv");
    let (reward, job_fee, admission_fee) = (pol.claim_reward, pol.job_fee, opv.economics.admission_fee);

    let poster0 = w.l.bonds[&POSTER].collateral;
    let producer_c0 = w.l.bonds[&PRODUCER].collateral;
    let burned0 = w.l.burned;

    // The producer answers the poster's honest job with a LIE. No outsider is online: no demand, no proof — ever.
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx]);
    // The lie IS objectively findable from public bytes — there is simply no one online to find it.
    assert!(matches!(outsider(&w, id, &da), OutsiderFindingV1::Prosecute(_)), "the lie is objectively provable");

    // It finalizes by the window rule (window_end 60) and is paid out of the poster's escrow.
    assert_eq!(w.block(60, vec![]), vec![E::Final { claim: id, reward }]);
    assert_eq!(w.consumer.paid(&PRODUCER), reward, "the fraudulent producer is paid the full reward");
    assert!(w.l.job_escrows.is_empty(), "the poster's escrow is spent");

    // The liability horizon (60 + 200 = 260) elapses with STILL no proof. On its last liable DAA nothing has moved; one DAA later
    // the reservation is returned in full and a late verifier's proof is refused for free: the fraud is now permanent.
    assert!(w.block(260, vec![]).is_empty(), "still reserved on the horizon's last DAA");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, opv.economics.reservation_per_claim, "held to the last day");
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let outsider_c0 = w.l.bonds[&OUTSIDER].collateral;
    let ev = w.block(261, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(refused(&ev).as_deref(), Some("past the liability horizon"), "too late: {ev:?}");
    assert!(ev.contains(&E::Released { claim: id }), "the reservation is returned in the same block");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 0, "the producer's whole reservation is returned — it risked nothing");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, producer_c0 - admission_fee, "its only cost was the admission fee");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, outsider_c0, "the late verifier is not even charged a fee");
    assert!(w.l.final_receipt(&id).is_some(), "the lie stands as a Final");

    let producer_profit = w.consumer.paid(&PRODUCER) as i128 - (producer_c0 - w.l.bonds[&PRODUCER].collateral) as i128;
    let poster_loss = (poster0 - w.l.bonds[&POSTER].collateral) as i128; // escrow paid out + posting fee burned
    assert_eq!(producer_profit, reward as i128 - admission_fee as i128, "unverified-lie profit = reward − admission fee");
    assert_eq!(poster_loss, reward as i128 + job_fee as i128, "the consumer paid reward + posting fee for a wrong answer");
    assert_eq!(w.l.burned - burned0, admission_fee + job_fee, "only the two fees were burned; nothing was slashed");
    eprintln!(
        "[F-C4R4-02 OBS] watcher absence: a provable lie finalized, paid the producer {reward}, returned its whole {} reservation, \
         and is permanent past the horizon. Producer profit {producer_profit} (reward − admission fee); consumer loss {poster_loss} \
         (reward + posting fee) for a wrong answer; nothing slashed. The honest-verifier assumption is load-bearing, not enforced.",
        opv.economics.reservation_per_claim
    );
}

// ── Target 5 (economics): F-C4R4-03 — a squatter seal freezes the poster's escrow and exit ────────────────────────────────

/// A seal of `job` by `bond` (the grief never reveals, so any 64-byte seal serves).
fn grief_seal(bond: Digest, job: Digest) -> T {
    T::SealClaim { producer: bond, job, seal: [0x77; 64] }
}

/// **F-C4R4-03 (P1 where armed): a squatter's unrevealed claim seal must not hold a poster's escrow — or its exit — past the escrow
/// TTL plus one seal TTL.**
///
/// GAP-5 returns a job's escrow once `job_escrow_ttl_daa` has passed, no live claim holds the job AND no producer seal of it is live
/// (`release_idle_job_escrows`), so "a sealed producer is never left working for an escrow that left". But a seal is an unbacked
/// promise any bond may make on any unclaimed job, and a re-seal restarts its clock while keeping the one deposit. So a squatter that
/// re-seals before each `seal_ttl_daa` keeps the escrow reserved forever; and because `Withdraw` needs `reserved == 0` (and V2's exit
/// gates read the same reservation on the node), the poster's WHOLE bond can never leave. Measured before the fix: one locked seal
/// deposit (1 BILI interim) froze the harness poster's 1,000,000-unit bond, exit requested, for as long as the squatter re-sealed.
///
/// A producer whose seal predates the escrow's TTL has one seal TTL to reveal (an unrevealed seal expires then anyway), so a seal is
/// honoured until `posted + escrow_ttl + seal_ttl` and never after: the control below shows such a producer is still paid.
#[test]
fn f_c4r4_03_a_squatter_seal_never_holds_a_posters_escrow_or_exit_past_the_ttl_and_one_seal_ttl() {
    let mut w = World::new();
    let pol = w.l.policy.clone();
    let (reward, ttl, seal_ttl, delay) = (pol.claim_reward, pol.job_escrow_ttl_daa, pol.seal_ttl_daa, pol.exit_delay_daa);
    let poster_c0 = w.l.bonds[&POSTER].collateral;

    // The poster posts one job at 2, then asks to leave at 3 (it posts nothing more).
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let jid = job.id();
    w.block(3, vec![T::RequestExit { bond: POSTER }]);
    let bound = 2 + ttl + seal_ttl;

    // The squatter seals at 5 and re-seals well inside every seal TTL, up to and past the bound.
    let step = seal_ttl - 5;
    let mut t = 5;
    while t <= bound {
        w.block(t, vec![grief_seal(SPAM1, jid)]);
        t += step;
    }
    assert!(w.l.seals.contains_key(&(jid, SPAM1)), "the squatter's seal is live at the bound");
    assert_eq!(w.l.bonds[&SPAM1].reserved, pol.seal_deposit, "the squatter locks one seal deposit");

    // One DAA past the bound the escrow is returned although the squatter's seal is still live.
    let ev = w.block(bound + 1, vec![]);
    assert!(
        ev.contains(&E::JobEscrowReturned { job: jid, poster: POSTER, amount: reward }),
        "the escrow must return once the TTL and one seal TTL have passed, whatever a squatter re-seals: {ev:?}"
    );
    // The squatter keeps re-sealing; the poster leaves anyway.
    w.block(bound + 2, vec![grief_seal(SPAM1, jid)]);
    assert_eq!(w.l.bonds[&POSTER].reserved, 0, "nothing of the poster's bond is reserved any more");
    let ev = w.block((bound + 3).max(3 + delay), vec![T::Withdraw { bond: POSTER }]);
    assert!(
        ev.contains(&E::Withdrawn { bond: POSTER, amount: poster_c0 - pol.job_fee }),
        "the poster's whole bond leaves despite the squatter: {ev:?}"
    );

    // Control: a producer that sealed BEFORE the escrow's TTL and reveals inside its seal TTL is still paid from the escrow.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 2);
    let honest = w.honest(&job, 3);
    let id = honest.claim.id();
    // `World::block` seals at the ledger's current DAA (2 + ttl − 1, before the TTL), then reveals at `2 + ttl + 50`.
    w.block(2 + ttl - 1, vec![]);
    w.block(2 + ttl + 50, vec![honest.tx, T::PanelCovered { claim: id }]);
    assert!(w.l.claims.contains_key(&id), "the sealed producer revealed within its seal TTL");
    let fin = w.block(2 + ttl + 50 + pol.challenge_window_daa, vec![]);
    assert!(fin.contains(&E::Final { claim: id, reward }), "a seal placed before the TTL keeps its escrow: {fin:?}");
}

// ── Target 4 (Final race / horizon edge): the pre-Final-default liability horizon is inclusive ─────────────────────────────

/// **Held (Target 4): the pre-Final-default liability horizon (F-C4R3-02) is inclusive at its edge.** A provable lie is turned into
/// a pre-Final availability default (the producer's own Sybil demands, the producer stays silent). The rest of the reservation is
/// kept until `default_daa + liability_daa`. A proof filed at exactly that DAA still convicts; one DAA past it is refused for free.
/// No off-by-one lets the colluders launder the fraud by filing-window timing.
#[test]
fn f_c4r4_04_the_pre_final_default_liability_horizon_convicts_on_its_last_day_and_not_after() {
    let edge = |file_at: u64| -> (Option<(u64, u64, bool)>, Option<String>, u64) {
        let mut w = World::new_opv();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let (_, lie) = w.lying(&job, 3);
        let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
        w.block(10, vec![lie.tx]);
        let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
        // The producer's own Sybil demands at 11; the producer never responds; the demand defaults at 11 + court_deadline (20) = 31.
        w.block(11, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
        let ev = w.block(31, vec![]);
        assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { .. })), "default at the deadline: {ev:?}");
        assert!(matches!(w.state(&id), ClaimStateV1::Unavailable { .. }));
        let until = w.l.claims[&id].liability_until.expect("a pre-Final default now sets the liability horizon");
        let ev = w.block(file_at, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
        (convicted(&ev), refused(&ev), until)
    };

    // default at 31, liability_daa 200 → horizon 231.
    let (_, _, until) = edge(31);
    assert_eq!(until, 231);
    let (conv_on, refused_on, _) = edge(until);
    assert_eq!(conv_on, Some((900, 500, false)), "a proof on the horizon's last DAA convicts (the default kept 900 reserved)");
    assert!(refused_on.is_none());
    let (conv_after, refused_after, _) = edge(until + 1);
    assert_eq!(conv_after, None, "one DAA past the horizon: no conviction");
    assert_eq!(refused_after.as_deref(), Some("past the liability horizon"), "and refused for free");
}

// ── Target 5 / §6.3: F-C4R4-05 — court-work saturation by one filing per block ─────────────────────────────────────────────

/// **F-C4R4-05 (P1 where armed): one dismissed filing must not buy a whole block's court.**
///
/// `charge` debits a `FileProof` the class's DECLARED worst court work before the court runs, from ONE per-block budget every class
/// shares, and a registration only refuses a class whose worst court is ABOVE the block's (`bounds.max_court_work >
/// max_court_work_per_block`). So a single junk filing against any claim of the heaviest registered class — even the attacker's own
/// honest claim — leaves no court work for any other filing in that block: every honest `FileProof` after it is `OverBudget`
/// (dropped, free) and must wait for the next block, where the same one junk filing repeats. The attacker pays ONE
/// `dismissed_proof_fee` per block, not the `max_adjudications_per_block × fee` the end-of-lane bound (6.4 BILI a block, ~1,600 BILI
/// over window + liability) and RFC-0015 §6.3's `censorship_cost` assume. The prosecution reserve does not help: it reserves RUNS, not
/// court work. And `censorship_cost` is evaluated per class with that class's own worst court, so a LIGHT class passes it while a
/// heavy class registered beside it makes its prosecution censorable at one fee per block.
///
/// Here the class is its own heaviest (the block's court budget set to its worst court): the attacker files one junk proof, then the
/// outsider its valid proof, in every block of the window. SAFE: the lie is convicted, or every censored block cost the attacker at
/// least `max_adjudications_per_block × dismissed_proof_fee`.
#[test]
fn f_c4r4_05_one_junk_filing_must_not_buy_a_whole_blocks_court() {
    let worst = {
        let w0 = World::new();
        w0.l.classes[&w0.class].bounds.max_court_work
    };
    let mut pol = policy();
    pol.max_court_work_per_block = worst; // the heaviest class the block admits (registration allows worst == the block's budget)
    let mut w = World::with(pol.clone());
    assert_ne!(w.class, [0; 64], "the class registers with a worst court equal to the block's court budget");

    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let junk = ProsecutionV1::Kernel(vec![1, 2, 3]);

    let spam0 = w.l.bonds[&SPAM1].collateral;
    let mut censored = 0u64;
    let mut convicted_at = None;
    for t in 11..60 {
        let ev = w.block(
            t,
            vec![
                T::FileProof { accuser: SPAM1, claim: id, proof: junk.clone() },
                T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() },
            ],
        );
        if convicted(&ev).is_some() {
            convicted_at = Some(t);
            break;
        }
        assert!(
            ev.iter().any(|e| matches!(e, E::Refused { why, .. } if why.contains("budget"))),
            "the valid proof was held out by the spent court budget: {ev:?}"
        );
        censored += 1;
    }
    let paid = spam0 - w.l.bonds[&SPAM1].collateral;
    let priced = censored * pol.max_adjudications_per_block as u64 * pol.dismissed_proof_fee;
    eprintln!(
        "[F-C4R4-05] worst court {worst} = the block's court budget: {censored} blocks censored for {paid} in dismissal fees \
         ({} per block); the censorship relation prices a block at {} (max_adjudications × fee); convicted at {convicted_at:?}",
        if censored > 0 { paid / censored } else { 0 },
        pol.max_adjudications_per_block as u64 * pol.dismissed_proof_fee
    );
    assert!(
        convicted_at.is_some() || paid >= priced,
        "a valid proof was held out of {censored} blocks for {paid}, below the {priced} the censorship relation prices"
    );
    assert!(paid >= priced, "every censored block cost at least max_adjudications × fee ({paid} < {priced})");
}

// ── Target 5 (economics): F-C4R4-06 — the served-demand-bond rule and an honest claim nobody can read ───────────────────────

/// **OBS F-C4R4-06 (economics, K2S's demand-bond fate).** A served position's demand bond is burned once the claim's liability
/// horizon ends unconvicted. Nothing obliges a producer to PUBLISH its trace: it may serve only on demand. Then checking an honest
/// claim — the only way to check it — costs the verifier one burned demand bond per position it reads, the producer nothing, and
/// nothing pays the verifier back. Against such a producer a rational verifier checks nothing (see F-C4R4-02: an unchecked lie is
/// paid). Quantified on the OPV route: five positions, all served, all bonds burned at the horizon.
#[test]
fn f_c4r4_06_obs_checking_an_unpublished_honest_claim_burns_one_demand_bond_per_position() {
    let mut w = World::new_opv();
    let pol = w.l.policy.clone();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let honest = w.honest(&job, 3);
    let (id, trace) = (honest.claim.id(), honest.trace.clone());
    w.block(10, vec![honest.tx]);
    let nothing = Da(Default::default());
    let OutsiderFindingV1::Demand(missing) = outsider(&w, id, &nothing) else { panic!("nothing is published") };
    let outsider_c0 = w.l.bonds[&OUTSIDER].collateral;
    let demands = missing.iter().map(|(s, p)| T::FileDemand { demander: OUTSIDER, claim: id, stage: *s, position: *p }).collect();
    w.block(11, demands);
    let responses =
        missing.iter().map(|(s, p)| T::Respond { claim: id, stage: *s, position: *p, bytes: position(&trace, *p, |_| {}) }).collect();
    let ev = w.block(12, responses);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count(), missing.len(), "{ev:?}");
    assert_eq!(outsider(&w, id, &nothing), OutsiderFindingV1::Clean, "the claim is honest");

    let mut t = 13;
    let mut finalized = None;
    while finalized.is_none() && t < 200 {
        let ev = w.block(t, vec![]);
        if ev.iter().any(|e| matches!(e, E::Final { claim, .. } if *claim == id)) {
            finalized = Some(t);
        }
        t += 1;
    }
    let until = w.l.claims[&id].liability_until.expect("Final");
    let ev = w.block(until + 1, vec![]);
    let burned = ev.iter().find_map(|e| if let E::ServedDemandBondsBurned { burned, .. } = e { Some(*burned) } else { None });
    let lost = outsider_c0 - w.l.bonds[&OUTSIDER].collateral;
    assert_eq!(burned, Some(missing.len() as u64 * pol.demand_bond), "{ev:?}");
    assert_eq!(lost, missing.len() as u64 * pol.demand_bond);
    eprintln!(
        "[F-C4R4-06 OBS] an honest claim published nothing: the verifier demanded {} positions and lost {lost} (burned at the \
         horizon); the producer was paid {} and served on demand at no cost. A verifier's cost of checking is set by the producer.",
        missing.len(),
        w.consumer.paid(&PRODUCER)
    );
}

// ── Target 5 (economics): F-C4R4-07 — the producer's Sybil holds the earliest seal of its own conviction ───────────────────

/// **OBS F-C4R4-07 (verifier incentive, GAP-R7's earliest-seal rule).** The bounty of a conviction goes to the earliest seal of the
/// convicting bytes. The producer knows its lie from the start, so its Sybil (a) seals its own proof at once and, when the proof
/// bytes do not depend on the verifier's salt, captures the bounty of ANY later conviction by those bytes passively; and (b) in any
/// case sees an honest verifier's `SealProof` — which names the claim in clear at least `claim_seal_delay_daa` before the filing —
/// and files its own sealed proof first, so the verifier's filing is a `Duplicate`. Either way the honest verifier is paid NOTHING
/// and the colluders lose half the reservation (the relation G14-R4 priced: deterrence holds). What does not hold is the verifier's
/// incentive (PRINCIPLES §6, condition 6): against a rational liar a verifier's expected bounty is zero.
#[test]
fn f_c4r4_07_obs_a_lying_producers_sybil_takes_the_bounty_of_its_own_conviction() {
    let run = |front_run: bool| -> (bool, u64, u64, i128) {
        let mut w = World::new_opv();
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let (_, lie) = w.lying(&job, 3);
        let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
        w.block(10, vec![lie.tx]);
        let colluders0 = w.l.bonds[&PRODUCER].collateral as i128 + w.l.bonds[&SPAM1].collateral as i128;
        // The Sybil's proof (its own salt) and the honest outsider's (another salt).
        let fresh = misaka_palw_kernel::ledger::KernelLedgerV1::replay(&w.genesis, &w.blocks);
        let sybil_finding =
            OutsiderV1 { ledger: &fresh, claim: id, material: &da, artifact: &w.params, salt: [0x11; 64] }.check().unwrap();
        let OutsiderFindingV1::Prosecute(sybil_proof) = sybil_finding else { panic!() };
        let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
        let canonical = sybil_proof == proof;
        // The Sybil seals at once.
        w.block(11, vec![T::SealProof { accuser: SPAM1, claim: id, seal: proof_seal_v1(&id, &SPAM1, &sybil_proof) }]);
        // The honest verifier finds the lie later, seals, then files.
        w.block(30, vec![T::SealProof { accuser: OUTSIDER, claim: id, seal: proof_seal_v1(&id, &OUTSIDER, &proof) }]);
        let mut txs = vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }];
        if front_run {
            // The Sybil saw the verifier's seal (it names the claim) and files its own sealed proof first.
            txs.insert(0, T::FileProof { accuser: SPAM1, claim: id, proof: sybil_proof });
        }
        let ev = w.block(31, txs);
        assert!(convicted(&ev).is_some(), "the lie is convicted: {ev:?}");
        let colluders = w.l.bonds[&PRODUCER].collateral as i128
            + w.l.bonds[&SPAM1].collateral as i128
            + w.consumer.paid(&SPAM1) as i128
            + w.consumer.paid(&PRODUCER) as i128
            - colluders0;
        (canonical, w.consumer.paid(&OUTSIDER), w.consumer.paid(&SPAM1), colluders)
    };
    let (canonical, outsider_passive, sybil_passive, _) = run(false);
    let (_, outsider_front, sybil_front, colluders_front) = run(true);
    eprintln!(
        "[F-C4R4-07 OBS] proof bytes salt-independent: {canonical}. Passive pre-seal: outsider {outsider_passive}, Sybil \
         {sybil_passive}. Front-run on the verifier's seal: outsider {outsider_front}, Sybil {sybil_front}, colluders net \
         {colluders_front} (reservation 1000)."
    );
    assert_eq!((outsider_front, sybil_front), (0, 500), "the verifier's public seal lets the Sybil file first and take the bounty");
    assert_eq!(colluders_front, -500, "the colluders lose half the reservation (deterrence holds, verifier incentive does not)");
    if canonical {
        assert_eq!((outsider_passive, sybil_passive), (0, 500), "canonical bytes: the Sybil's pre-seal takes the bounty passively");
    }
}

// ── Target 1 (grinding): F-C4R4-08 — what the beacon accounting reads from the ledger ──────────────────────────────────────

/// `n!/(n−k)!`, saturating.
fn perm(n: u128, k: u128) -> u128 {
    (0..k).fold(1u128, |a, i| a.saturating_mul(n.saturating_sub(i)))
}

/// **F-C4R4-08 (P2, cross-lane: OPV-BOOT's grinding accounting vs G14-R4's F-C4R3-05 fix).** OPV-BOOT's bound on the choices a beacon
/// offers (`beacon_grinding_choices_bound_v1(k, live_cap, F)`, design §6) takes `live_cap = max_live_claims_total` because "with
/// `beacon_window_slots ≤` OPV window + liability, every work that can settle in one window is live at once". That held while a
/// Final claim kept its slot through its liability horizon. Since F-C4R3-05 only PRE-FINAL claims hold slots, a slot frees at Final
/// and is refilled: a beacon window of `B` slots sees up to `hard × ⌈(B − window) / window⌉ + hard` Finals (`hard = total + fresh`).
/// Measured with the example policy (total 5, fresh 2, window 50) over a 120-slot beacon window: 14 Finals, not 5 — `P(14, 2) = 182`
/// ordered lists, not `P(5, 2) = 20` (3 bits understated; at the interim 32 + 16 and window 50: 96 Finals, 14 bits not 10).
///
/// Second, the distinct source rule's consumer: the only ledger state naming a job's poster is its escrow row, removed when the job's
/// Final is paid (`KernelJobV1` has no poster). A node deriving `SourceAttributionV1.consumer_id` from state therefore sees `Absent`
/// for every Final source, and "distinct consumers where known" never binds.
#[test]
fn f_c4r4_08_a_beacon_window_sees_more_finals_than_the_live_cap_and_no_final_names_its_poster() {
    let mut w = World::new_opv();
    let opv = w.l.opv.policy.expect("opv");
    let e = opv.economics;
    let hard = (e.max_live_claims_total + e.fresh_producer_slots) as usize;
    let window = opv.window_daa();
    // Room for two waves of reservations (each held through Final + liability).
    w.block(2, [PRODUCER, HONEST, SQUATTER, SPAM1, SPAM2].iter().map(|b| T::RegisterBond { bond: *b, collateral: 20_000 }).collect());
    let jobs: Vec<KernelJobV1> = (0..(2 * hard + 1) as u8)
        .map(|n| KernelJobV1 {
            class_binding_id: w.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [0x40 + n; 64],
        })
        .collect();
    w.block(3, jobs.iter().map(|j| T::PostJob { job: j.clone() }).collect());
    // Who fills a wave: three per producer up to the total, then one each for producers holding no pre-Final claim.
    let fillers = [PRODUCER, PRODUCER, PRODUCER, HONEST, HONEST, SQUATTER, SPAM1, SPAM2];
    let wave = |w: &World, jobs: &[KernelJobV1]| -> Vec<T> {
        jobs.iter().zip(fillers.iter()).map(|(j, b)| w.honest_by(j, *b, 3).tx).collect()
    };
    let start = 10u64;
    let beacon_window = 120u64;
    // Wave 1 at S: the lane's hard ceiling is reached (the eighth claim is refused).
    let ev = w.block(start, wave(&w, &jobs[..hard + 1]));
    let committed1 = ev.iter().filter(|e| matches!(e, E::ClaimCommitted { .. })).count();
    assert_eq!(committed1, hard, "the hard ceiling: {ev:?}");
    let mut finals = Vec::new();
    let mut t = start + 1;
    while t < start + beacon_window {
        let txs = if t == start + window + 1 { wave(&w, &jobs[hard..2 * hard]) } else { vec![] };
        let ev = w.block(t, txs);
        finals.extend(ev.iter().filter_map(|e| if let E::Final { claim, .. } = e { Some((t, *claim)) } else { None }));
        t += 1;
    }
    let c = finals.len() as u128;
    let (k, live_cap) = (2u128, e.max_live_claims_total as u128);
    eprintln!(
        "[F-C4R4-08] {c} OPV Finals settled inside one {beacon_window}-slot beacon window (live cap {live_cap}, hard {hard}): \
         P({c}, {k}) = {} ordered source lists, the accounting assumes P({live_cap}, {k}) = {}",
        perm(c, k),
        perm(live_cap, k)
    );
    assert_eq!(c, 2 * hard as u128, "two full waves settle inside the window: {finals:?}");
    assert!(c > live_cap, "more Finals compete for one beacon than the live cap the grinding bound reads");

    // No state names the poster of a Final job.
    for (_, claim) in &finals {
        let job = w.l.claims[claim].job_id;
        assert!(!w.l.job_escrows.contains_key(&job), "the escrow row, the only state naming the poster, is gone at Final");
    }
}

// ── Target 4 (Final race): the losing sealer of a job race ────────────────────────────────────────────────────────────────

/// **OBS F-C4R4-12 (the seal race).** Two producers seal the same job; the first to reveal holds it; the other can never reveal (one
/// claim per job) — and its seal expires and FORFEITS its deposit as if it had withheld. Nothing returns a deposit whose reveal another
/// claim made impossible, so a bond that claims a job first burns the deposit of every other sealer of it (here an honest producer that
/// lost the race by one block). The rule "withholding a sealed reveal is never free" (G14-R4, OPV-BOOT #2) also charges a sealer that
/// was not withholding; sealed-source beacon v3 should not read such a forfeit as a withheld source. Small at the interim deposit
/// (1 BILI); recorded because v3 makes sealing many-sided.
#[test]
fn f_c4r4_12_obs_the_losing_sealer_of_a_job_race_forfeits_its_deposit() {
    let mut w = World::new_opv();
    let pol = w.l.policy.clone();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let first = w.honest_by(&job, PRODUCER, 3);
    let second = w.honest_by(&job, HONEST, 3);
    // Both seal in the same block; PRODUCER reveals first.
    w.block(
        3,
        vec![T::SealClaim { producer: HONEST, job: job.id(), seal: misaka_palw_kernel::ledger::claim_seal_v1(&second.claim.id()) }],
    );
    w.block(5, vec![first.tx]);
    let honest_c0 = w.l.bonds[&HONEST].collateral;
    let ev = w.block(6, vec![second.tx]);
    assert_eq!(refused(&ev).as_deref(), Some("another claim already holds the job (one claim per job)"), "{ev:?}");
    let ev = w.block(3 + pol.seal_ttl_daa + 1, vec![]);
    assert!(
        ev.iter().any(
            |e| matches!(e, E::SealForfeited { producer, forfeited, .. } if *producer == HONEST && *forfeited == pol.seal_deposit)
        ),
        "{ev:?}"
    );
    assert_eq!(w.l.bonds[&HONEST].collateral, honest_c0 - pol.seal_deposit, "the race's loser paid the withholding forfeit");
}

// ── Target 3 (lane capture, F-C4R3-05 round 2): how long one slot can be held ─────────────────────────────────────────────

/// **OBS O-C4R4-capture.** G14-R4 prices a continuous capture of the OPV lane as `hard × reservation × (window + liability) / window`
/// locked: each slot is held one window. A colluder's own demand on its own (honest) claim, filed on the window's last DAA and served
/// near the demand's deadline, keeps the claim pre-Final — holding its admission slot — until the proof grace after the service ends:
/// `window + court_deadline + proof_grace` at most (`OpvPolicyV1::hard_deadline`). Measured: a slot held 50 → ~80 DAA, so the locked
/// collateral of a continuous capture falls by ~30 % (240,000 → ~168,000 BILI at the interim terms) for one burned demand bond per slot
/// and cycle. An upper bound in the end-of-lane text, not a defect.
#[test]
fn f_c4r4_obs_capture_a_self_demand_stretches_one_admission_slot_to_the_hard_deadline() {
    let held = |self_demand: bool| -> u64 {
        let mut w = World::new_opv();
        let pol = w.l.policy.clone();
        let opv = w.l.opv.policy.expect("opv");
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let honest = w.honest(&job, 3);
        let (id, trace) = (honest.claim.id(), honest.trace.clone());
        w.block(10, vec![honest.tx]);
        let window_end = 10 + opv.window_daa();
        if self_demand {
            w.block(window_end - 1, vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 }]);
            let serve_at = window_end - 1 + pol.court_deadline_daa - 1;
            let ev = w.block(serve_at, vec![T::Respond { claim: id, stage: 0, position: 0, bytes: position(&trace, 0, |_| {}) }]);
            assert!(ev.iter().any(|e| matches!(e, E::Served { .. })), "{ev:?}");
        }
        let mut t = w.l.daa + 1;
        loop {
            let ev = w.block(t, vec![]);
            if ev.iter().any(|e| matches!(e, E::Final { claim, .. } if *claim == id)) {
                return t - 10;
            }
            assert!(t < 10 + opv.hard_deadline(0, &pol) + 5, "Final by the hard deadline");
            t += 1;
        }
    };
    let (plain, stretched) = (held(false), held(true));
    eprintln!("[O-C4R4-capture] an OPV admission slot is held {plain} DAA plainly, {stretched} DAA with a self-demand served late");
    assert!(stretched > plain + 20, "a self-demand stretches the slot ({plain} → {stretched})");
}
