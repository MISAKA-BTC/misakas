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
    assert_eq!(
        conv_on,
        Some((900, 450, false)),
        "a proof on the horizon's last DAA convicts (the default kept 900 reserved; F-C4R4-15: the bounty is the share of the 900)"
    );
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
    pol.prosecution_reserve_permille = 0; // Isolate dismissed-proof pricing at the full-block ceiling.
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

// ══ Round 4b (2026-10-10): G14 under full collusion — the producer and every Panel seat collude; one outsider bond ═════════════
//
// The user's priority of 2026-10-10: even if the producer and ALL Panel seats collude, ONE public bonded verifier outside the Panel
// reaches an objective conviction (or the correct DA default, or the dismissal of an honest claim) from public authenticated material
// only. The tests below try to break that at the kernel's reference level, in the order the Lead set: (1) pre-empt / censor the
// outsider's filing, (2) force a timeout or withhold so the case ends with no default, (3) reach Final before the outsider can file,
// (4) make the outsider need secret state, (5) exhaust the shared court budget.

/// One block whose objects are signed by the bonds given (the harness's `block` signs every object that names no actor with the
/// producer; a `Respond` names none, so a junk responder needs its own signer).
fn signed_block(w: &mut World, daa: u64, txs: Vec<(Digest, T)>) -> Vec<E> {
    use misaka_palw_kernel::ledger::LedgerBlockV1;
    let b = LedgerBlockV1 { daa, txs: txs.into_iter().flat_map(|(signer, t)| t.into_txs(signer)).collect() };
    let ev = w.consumer.apply(&mut w.l, &b);
    w.blocks.push(b);
    w.events.extend(ev.iter().cloned());
    ev
}

/// **F-C4R4-13 (G14 Q2, Panel route; P2 where armed): a service must not let a never-covered claim time out before the proof it
/// enables can be filed.**
///
/// A Panel-licensed claim is `Checking` until the Panel's covered tally or its receipts' deadline (`committed + check_window_daa`).
/// An open demand holds it (`Disputed` never times out), and a service closes the demand with a dismissal verdict that returns the
/// claim to `Checking`. The proof grace a service grants (`ProofGrace`) holds only FINAL, not the timeout: served one DAA past the
/// receipts' deadline, the claim times out in the SAME block's tick, its reservation is released, and the proof the served values
/// enable is refused ("the claim ended without passing and holds nothing"). The colluding Panel simply never covers.
///
/// So when an outsider demands a lying claim while it is still `Checking`, the colluders choose the outcome: withholding costs the
/// default penalty, but SERVING after the deadline costs nothing — neither a conviction nor a default, though the fault is now public.
/// A detected attempt is free (no deterrence for fraud caught early), and the outsider's demand bond is merely refunded. SAFE: the
/// outsider's proof, filed in the block after the service, convicts.
#[test]
fn f_c4r4_13_a_service_past_the_check_deadline_must_not_time_the_claim_out_before_its_proof() {
    let mut w = World::new();
    let pol = w.l.policy.clone();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, trace) = (lie.claim.id(), lie.trace.clone());
    // The producer publishes everything but the faulty position; the colluding Panel never covers the claim.
    let da = Da::publishing(&lie.trace, &[at]);
    w.block(10, vec![lie.tx]);
    let deadline = 10 + pol.check_window_daa;
    assert!(matches!(w.state(&id), ClaimStateV1::Checking { deadline_daa, .. } if deadline_daa == deadline));
    let producer_c0 = w.l.bonds[&PRODUCER].collateral;

    // The outsider finds the one position it cannot check and demands it on the receipts' last DAA.
    assert_eq!(outsider(&w, id, &da), OutsiderFindingV1::Demand(vec![(0, at.0)]));
    let ev = w.block(deadline, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { .. })), "{ev:?}");
    // The demand holds the claim past the receipts' deadline (a Disputed claim does not time out) …
    let ev = w.block(deadline + 1, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
    // … and the producer serves the TRUE committed values (the fault is now public) one DAA past it.
    assert!(ev.iter().any(|e| matches!(e, E::Served { .. })), "{ev:?}");
    let timed_out = ev.iter().any(|e| matches!(e, E::TimedOut { claim } if *claim == id));
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("the served values make the lie provable") };
    let ev = w.block(deadline + 2, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    eprintln!(
        "[F-C4R4-13] served at {} (receipts' deadline {deadline}): timed out in the serving block = {timed_out}; the proof one DAA \
         later: {:?}; producer collateral {} → {} (reserved {})",
        deadline + 1,
        convicted(&ev).map(|c| format!("convicted {c:?}")).or(refused(&ev)),
        producer_c0,
        w.l.bonds[&PRODUCER].collateral,
        w.l.bonds[&PRODUCER].reserved,
    );
    assert!(convicted(&ev).is_some(), "the fault the service made public must still convict: {ev:?}");
}

/// **F-C4R4-14 (G14, the honest-claim leg; P1 where armed): free junk responses must not force an honest producer's default.**
///
/// A rejected `Respond` pays nothing, yet it spends one of the block's NON-reserved runs before it is classified. A griefer demands
/// one position of an HONEST claim, then fills the `max_adjudications − reserved` runs of every block until the demand's deadline with
/// junk responses ordered before the producer's valid one: the honest response is `OverBudget` (dropped) in every block, the demand
/// defaults, the honest producer is charged the default penalty (its claim voided, no reward) and the griefer, as the demander, takes
/// the accuser's share of it. G14's third outcome (the dismissal of an honest claim) fails. Two ways to send the junk:
///
/// * (a) from any bond, on the victim's own demand (a `Respond` names no actor — any registered bond may sign it, an exiting one too);
/// * (b) from the griefer AS THE PRODUCER of a claim of its own, on a demand its own Sybil opened on that claim — so restricting
///   `Respond` to the claim's producer does not close this: what is free is a REJECTED response, whoever signs it.
///
/// This is O-C4R4-respond (round 4) measured end to end; G14-R4's end-of-lane item 9 names the fix space. SAFE (both ways): a
/// producer that sends its valid response in every block before the deadline is never defaulted.
#[test]
fn f_c4r4_14_free_junk_responses_must_not_force_an_honest_producers_default() {
    // G14R's fix (the response lane): a response spends no run of the shared budget, and a rejected one costs its signer the
    // dismissal fee. Returned: (default, honest loss, griefer paid, junk count, served at, griefer's cost).
    let run = |own_claim: bool| -> (Option<(u64, u64)>, u64, u64, u64, Option<u64>, u64) {
        let mut w = World::new();
        let pol = w.l.policy.clone();
        let free_runs = pol.max_adjudications_per_block - pol.prosecution_reserved_runs();
        w.block(2, vec![bond(SPAM1, 10_000)]);
        let job = w.post_job(3, &[3, 17, 9], 3, 1);
        let h = w.honest(&job, 3);
        let (id, trace) = (h.claim.id(), h.trace.clone());
        let mut txs = vec![h.tx, T::PanelCovered { claim: id }];
        // (b) the griefer's own honest claim of another job, which its Sybil (SPAM2) will demand.
        let own = own_claim.then(|| {
            let job2 = w.post_job(4, &[5, 1, 2], 3, 2);
            let generated = w.greedy(&w.params, &job2.prompt, 3);
            w.produce(&job2, SPAM1, generated, &w.params, |_| {})
        });
        if let Some(o) = &own {
            txs.push(o.tx.clone());
            txs.push(T::PanelCovered { claim: o.claim.id() });
        }
        w.block(10, txs);
        // The griefer (SPAM1) demands position 1 of the honest claim.
        let mut demands = vec![T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 1 }];
        if let Some(o) = &own {
            demands.push(T::FileDemand { demander: SPAM2, claim: o.claim.id(), stage: 0, position: 0 });
        }
        let ev = w.block(11, demands);
        let deadline = ev
            .iter()
            .find_map(|e| match e {
                E::DemandOpened { claim, deadline, .. } if *claim == id => Some(*deadline),
                _ => None,
            })
            .expect("the demand opens");
        let producer_c0 = w.l.bonds[&PRODUCER].collateral;
        let valid = position(&trace, 1, |_| {});
        let griefer = if own_claim { SPAM1 } else { SPAM2 };
        let griefer_c0 = w.l.bonds[&griefer].collateral;
        // (a) junk on the victim's own demand (position 1); (b) junk on the griefer's own demand (its claim's position 0).
        let (target, at, signer) = match &own {
            Some(o) => (o.claim.id(), 0, SPAM1),
            None => (id, 1, SPAM2),
        };
        let (mut junk, mut dropped, mut served) = (0u64, 0u64, None);
        let mut defaulted = None;
        for t in 12..=deadline {
            let mut txs: Vec<(Digest, T)> = (0..free_runs)
                .map(|i| (signer, T::Respond { claim: target, stage: 0, position: at, bytes: vec![0xEE, i as u8, (t & 0xFF) as u8] }))
                .collect();
            junk += txs.len() as u64;
            txs.push((PRODUCER, T::Respond { claim: id, stage: 0, position: 1, bytes: valid.clone() }));
            let ev = signed_block(&mut w, t, txs);
            if ev.iter().any(|e| matches!(e, E::Served { claim, .. } if *claim == id)) {
                served = Some(t);
                break;
            }
            if refused(&ev).is_some_and(|why| why.contains("adjudication budget")) {
                dropped += 1;
            }
            if let Some(p) = ev.iter().find_map(|e| match e {
                E::ProducerDefault { claim, penalty, .. } if *claim == id => Some(*penalty),
                _ => None,
            }) {
                defaulted = Some((t, p));
            }
        }
        let lost = producer_c0 - w.l.bonds[&PRODUCER].collateral;
        eprintln!(
            "[F-C4R4-14 {}] {free_runs} rejected responses a block ({junk} in all, free) kept the honest response over budget in \
             {dropped} blocks; served at {served:?}; default {defaulted:?}: the honest producer lost {lost}, the griefer was paid {}",
            if own_claim { "(b) as the producer of its own claim" } else { "(a) from any bond" },
            w.consumer.paid(&SPAM1)
        );
        (defaulted, lost, w.consumer.paid(&SPAM1), junk, served, griefer_c0 - w.l.bonds[&griefer].collateral)
    };
    let fee = policy().dismissed_proof_fee;
    for (case, r) in [("(a)", run(false)), ("(b)", run(true))] {
        let (defaulted, lost, griefer_paid, junk, served, griefer_cost) = r;
        // (1) the honest producer's valid answer is never crowded out: served in the first block, never defaulted, nothing lost;
        assert!(defaulted.is_none() && served == Some(12) && lost == 0, "{case} the honest answer was crowded out: {r:?}");
        // (2) every rejected response cost its sender the dismissal fee;
        assert_eq!(griefer_cost, junk * fee, "{case} junk is never free: {r:?}");
        // (3) nobody profits from forcing a default: the griefer was paid nothing.
        assert_eq!(griefer_paid, 0, "{case} the griefer was paid: {r:?}");
    }
}

/// **OBS (G14 Q4, ADR-0177's condition): without the registered model's tensors an outsider cannot convict a MatMul lie** — even with
/// every committed value of the claim in hand. A kernel fault proof opens one column of the weight against the registered commitment;
/// the chain never serves a weight (demands are per position: node values and stage inputs only), so for a closed model the detection
/// probability `p` is 0, exactly as ADR-0177 states (G14 is conditional on the verifier having acquired the model). Pinned so nobody
/// reads a closed class's quiet window as verification.
#[test]
fn f_c4r4_obs_g14_q4_without_the_model_an_outsider_cannot_convict_a_matmul_lie() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let fresh = misaka_palw_kernel::ledger::KernelLedgerV1::replay(&w.genesis, &w.blocks);
    let closed = misaka_palw_tir::MapParams::default();
    let without = OutsiderV1 { ledger: &fresh, claim: id, material: &da, artifact: &closed, salt: [0x5A; 64] }.check();
    let with = outsider(&w, id, &da);
    eprintln!("[O-C4R4-Q4] every value served: with the model {with:?}; without it {without:?}");
    assert!(matches!(with, OutsiderFindingV1::Prosecute(_)), "with the model the lie is provable");
    assert!(!matches!(without, Ok(OutsiderFindingV1::Prosecute(_))), "without the model no proof exists: p = 0 for a closed model");
}

// ── ADR-0032 49% (2026-10-10) on the kernel route ─────────────────────────────────────────────────────────────────────────────

/// **Control (holds): a plain self-conviction at 49% loses at least 51% of what it collected**, integer rounding included: the
/// colluders' Sybil, holding the earliest seal of the canonical proof, is paid `⌊490 × slashed / 1000⌋`.
#[test]
fn f_c4r4_15_control_a_plain_self_conviction_at_49_percent_loses_at_least_51_percent() {
    for collateral in [999u64, 1000, 1001, 4_321] {
        let mut pol = policy();
        pol.accuser_reward_permille = 490;
        pol.claim_collateral = collateral;
        let mut w = World::with(pol.clone());
        let job = w.post_job(2, &[3, 17, 9], 3, 1);
        let (_, lie) = w.lying(&job, 3);
        let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
        w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
        let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
        w.block(11, vec![T::SealProof { accuser: SPAM1, claim: id, seal: proof_seal_v1(&id, &SPAM1, &proof) }]);
        let ev = w.block(20, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
        let (slashed, reward, _) = convicted(&ev).expect("convicted");
        assert_eq!(slashed, collateral);
        assert_eq!(w.consumer.paid(&SPAM1), reward, "the Sybil's earlier seal takes the bounty");
        assert_eq!(reward, collateral * 490 / 1000, "⌊49% × collected⌋");
        assert!((slashed - reward) * 100 >= slashed * 51, "net loss {} of {slashed} collected", slashed - reward);
    }
}

/// **F-C4R4-15 (P2 where armed; ADR-0032 49%): a default, then a conviction, must not recycle more than the accuser's share of what
/// they collected together.**
///
/// C4 F-C4R3-02's rule pays a conviction after a pre-Final default its bounty "as if no default had come first" (basis = the slash +
/// what the default took), so the honest accuser is never diluted — but the default's own demander share was ALREADY paid out of that
/// same reservation. The colluders hold both: their Sybil joins the outsider's demand (the demanders split `accuser_share × penalty`
/// equally, so each extra Sybil dilutes the outsider) and their Sybil's earlier seal of the canonical proof takes the bounty. They
/// recover `s × (R + P × k/(k+1))` of the `R` collected — with `s` = 49%, `P/R` = 10% (the interim ratio, 100 of 1,000 BILI) and one
/// Sybil demander, 51.4%; with many, up to 53.9% — so the self-dealing net loss falls under ADR-0032's 51% (at the interim 500‰, to
/// 45%). Not a profit (a defaulted claim earns no reward), but the deterrence is below the stated bound exactly where the colluders
/// choose it: when they withheld from an outsider who later obtains the material. SAFE: what the colluders recover is at most 49% of
/// what the default and the conviction collected.
#[test]
fn f_c4r4_15_a_default_then_a_self_conviction_must_not_recycle_more_than_the_accuser_share() {
    let mut pol = policy();
    pol.accuser_reward_permille = 490;
    let mut w = World::with(pol.clone());
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let id = lie.claim.id();
    let (withheld, full) = (Da::publishing(&lie.trace, &[at]), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    // The colluders know their lie: their Sybil SPAM2 seals the canonical proof at once.
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &full) else { panic!() };
    w.block(11, vec![T::SealProof { accuser: SPAM2, claim: id, seal: proof_seal_v1(&id, &SPAM2, &proof) }]);
    // The outsider cannot check the withheld position and demands it; the colluders' Sybil SPAM1 joins the demand.
    assert_eq!(outsider(&w, id, &withheld), OutsiderFindingV1::Demand(vec![(0, at.0)]));
    w.block(
        12,
        vec![
            T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 },
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: at.0 },
        ],
    );
    let colluders0 = [PRODUCER, SPAM1, SPAM2].iter().map(|b| w.l.bonds[b].collateral as i128).sum::<i128>();
    let collateral0 = w.l.bonds[&PRODUCER].collateral;
    // The producer withholds: the demand defaults at its deadline (12 + 20).
    let ev = w.block(12 + pol.court_deadline_daa, vec![]);
    let penalty = ev
        .iter()
        .find_map(|e| match e {
            E::ProducerDefault { penalty, .. } => Some(*penalty),
            _ => None,
        })
        .expect("the producer defaulted");
    // The outsider later obtains the material (another copy) and files the proof inside the default's liability horizon.
    let ev = w.block(60, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    let (slashed, reward, _) = convicted(&ev).expect("the default does not erase the fraud");
    let collected = (collateral0 - w.l.bonds[&PRODUCER].collateral) as i128;
    assert_eq!(collected, (penalty + slashed) as i128);
    let recovered = (w.consumer.paid(&SPAM1) + w.consumer.paid(&SPAM2)) as i128;
    let colluders_net =
        [PRODUCER, SPAM1, SPAM2].iter().map(|b| w.l.bonds[b].collateral as i128).sum::<i128>() + recovered - colluders0;
    eprintln!(
        "[F-C4R4-15] collected {collected} (default {penalty} + slash {slashed}); colluders recovered {recovered} (demander share {} + \
         bounty {reward}) = {:.1}% of it; net loss {} = {:.1}% (ADR-0032 bound: ≥ 51%); the outsider was paid {}",
        w.consumer.paid(&SPAM1),
        recovered as f64 * 100.0 / collected as f64,
        -colluders_net,
        -colluders_net as f64 * 100.0 / collected as f64,
        w.consumer.paid(&OUTSIDER)
    );
    assert!(recovered * 1000 <= collected * 490, "the colluders recovered {recovered} of {collected}: more than 49%");
}

// ── ADR-0177 non-interference: the cumulative scope ──────────────────────────────────────────────────────────────────────────

/// GF(2^61 − 1): exact arithmetic for rebuilding an integer linear map from served activations.
const GF: i128 = (1 << 61) - 1;

fn gf_pow(mut a: i128, mut e: i128) -> i128 {
    let mut r = 1i128;
    a = a.rem_euclid(GF);
    while e > 0 {
        if e & 1 == 1 {
            r = r * a % GF;
        }
        a = a * a % GF;
        e >>= 1;
    }
    r
}

/// Solve `W · x_p = y_p` for `W` (`out × inp`) from the rows `(x_p, y_p)`: `None` while the `x_p` do not span `inp` dimensions.
fn rebuild_linear_map(rows: &[(Vec<i128>, Vec<i128>)], inp: usize, out: usize) -> Option<Vec<i128>> {
    let mut m: Vec<Vec<i128>> =
        rows.iter().map(|(x, y)| x.iter().chain(y.iter()).map(|v| v.rem_euclid(GF)).collect::<Vec<i128>>()).collect();
    let mut rank = 0;
    for col in 0..inp {
        let pivot = (rank..m.len()).find(|&r| m[r][col] != 0)?;
        m.swap(rank, pivot);
        let inv = gf_pow(m[rank][col], GF - 2);
        for v in m[rank].iter_mut() {
            *v = *v * inv % GF;
        }
        for r in 0..m.len() {
            if r != rank && m[r][col] != 0 {
                let f = m[r][col];
                for c in 0..inp + out {
                    m[r][c] = (m[r][c] - f * m[rank][c]).rem_euclid(GF);
                }
            }
        }
        rank += 1;
    }
    let signed = |v: i128| if v > GF / 2 { v - GF } else { v };
    Some((0..out).flat_map(|r| (0..inp).map(move |k| (r, k))).map(|(r, k)| signed(m[k][inp + r])).collect())
}

/// **F-C4R4-17 (ADR-0177 non-interference, the cumulative scope; P2 design conflict where armed): claim-specific demands must not
/// rebuild the registered model.**
///
/// ADR-0177 keeps claim-specific evidence and its court but forbids weight-file / range requests and "repeated requests that rebuild
/// the model (a cumulative scope)". The kernel route makes every committed POSITION of a claim demandable at once (one demand bond
/// each), and a position's response is every node value of that position — the input AND the exact integer output of every MatMul.
/// For a linear layer `y = W·x`, `inp` positions whose activations span `inp` dimensions determine `W` exactly. So one bond that posts
/// one job (a prompt of its choice), lets an honest producer of a CLOSED model claim it, and demands every position, forces the
/// producer to serve — or be defaulted — what rebuilds every weight matrix applied to a served activation, verified here against the
/// registered commitments themselves. Its cost is one demand bond per position (burned at the horizon of an honest claim).
///
/// This is the G14 / ADR-0177 tension in one test: G14 needs the faulty position demandable (the outsider cannot know which), and
/// ADR-0177 forbids the union of demands from rebuilding the model. SAFE: the served values rebuild no registered weight tensor.
#[test]
#[ignore = "FAIL F-C4R4-17: demanding every position of one claim rebuilds the class's projection weights exactly (the kernel's hook `cumulative_scope_allows_v1` waits for DA16b's predicate)"]
fn f_c4r4_17_claim_specific_demands_must_not_rebuild_the_registered_weights() {
    use misaka_palw_kernel::trace::tensor_commitment;
    use misaka_palw_tir::Tensor;
    use misaka_palw_tir::program::Ref;
    let mut w = World::new();
    let pol = w.l.policy.clone();
    // A prompt of the extractor's choice: 20 distinct tokens (7 is prime to the 32-token vocabulary).
    let prompt: Vec<u32> = (0..20u32).map(|i| (i * 7 + 3) % 32).collect();
    let job = w.post_job(2, &prompt, 1, 9);
    let h = w.honest(&job, 1);
    let (id, trace) = (h.claim.id(), h.trace.clone());
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    // The producer publishes NOTHING (a closed model). The extractor demands every position — each one claim-specific.
    let positions = trace.values.len() as u32;
    let demands = (0..positions).map(|p| T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: p }).collect();
    w.block(11, demands);
    // An honest producer must serve each demanded position (or be defaulted).
    let responses = (0..positions)
        .map(|p| T::Respond { claim: id, stage: 0, position: p, bytes: position(&trace, p, |_| {}) })
        .collect::<Vec<_>>();
    let ev = w.block(12, responses);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count() as u32, positions, "{ev:?}");

    // Rebuild, from the chain's served values ALONE, every weight a MatMul applies to a served activation.
    let program = w.program.clone();
    let class = w.l.classes[&w.class].clone();
    let served = |p: u32, s: usize, n: usize| -> Option<Tensor> {
        w.l.served.get(&(id, 0u8, p))?.values.get(s)?.get(n)?.as_ref()?.decode().ok()
    };
    let (mut rebuilt, mut tried) = (Vec::new(), 0usize);
    for (s, (b, layer)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            let (misaka_palw_tir::Prim::MatMul, [Ref::Param(j), Ref::Node(i)]) = (&node.prim, node.inputs.as_slice()) else {
                continue;
            };
            let key = (*j, if program.params[*j as usize].per_layer { *layer } else { None });
            let Some(truth) = w.params.tensors.get(&key) else { continue };
            let (out, inp) = (truth.shape[0], truth.shape[1]);
            let rows: Option<Vec<(Vec<i128>, Vec<i128>)>> =
                (0..positions).map(|p| Some((served(p, s, *i as usize)?.data, served(p, s, n)?.data))).collect();
            let Some(rows) = rows else { continue };
            tried += 1;
            if let Some(data) = rebuild_linear_map(&rows, inp, out) {
                let candidate = Tensor { data, ..truth.clone() };
                let authentic = tensor_commitment(&candidate) == class.param_commitments.by_instance[&key];
                assert_eq!(&candidate, truth, "an exact rebuild");
                assert!(authentic, "and it opens the REGISTERED commitment");
                rebuilt.push((program.params[*j as usize].name.clone(), key.1, out * inp));
            }
        }
    }
    let weights: usize = rebuilt.iter().map(|(_, _, n)| n).sum();
    let total: usize = w.params.tensors.values().map(|t| t.data.len()).sum();
    eprintln!(
        "[F-C4R4-17] {positions} claim-specific demands ({} BILI of demand bonds) made the producer serve what rebuilds {} of {tried} \
         MatMul weight instances exactly ({weights} of the model's {total} weight elements), each opening the registered commitment: \
         {:?}",
        positions as u64 * pol.demand_bond,
        rebuilt.len(),
        rebuilt.iter().map(|(name, l, _)| format!("{name}@{l:?}")).collect::<Vec<_>>()
    );
    assert!(rebuilt.is_empty(), "claim-specific demands rebuilt {} registered weight tensors", rebuilt.len());
}
