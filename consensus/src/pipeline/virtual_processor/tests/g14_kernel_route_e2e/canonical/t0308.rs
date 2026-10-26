//! **lane T0308 — T03 (forged signatures / fake Panel completion / fake court responses) and T08 (miner offline, distinct court
//! signer, third-party relay, no old-signature replay) on the SPINE slice** (2026-10-26).
//!
//! The slice is SPINE's: the Panel-licensed kernel route (`Mode::Panel`, every seat colluding), the class card 1 registers in
//! [`world`] on the canonical testnet-12 harness, the single-prompt decode job of [`claimed`]. Every object goes through the node's
//! mempool, its own template and the chain block's fold (`Net::send`), funded by real Full-UTXO carriers and signed with ML-DSA-87.
//! A replay node (`Net::replay`) must agree at the end. No cryptographic proof of computation is used (PRINCIPLES §8): signatures
//! authenticate who said what; a response is judged only against the claim's own committed values.
//!
//! Authority on this route: a kernel route object verifies under the CURRENT key of an Active V2 bond (processor preflight), and
//! `Respond` is open to any registered bond (`KernelLedgerV1::respond`: "a valid response pays nothing and closes the demand, whoever
//! signs it"). The carrier's funder (the relay) is not an actor of the object: it is neither charged nor credited. The fixture's
//! distinct card keys show distinct SIGNERS only; they are no evidence that the operators are independent.
use super::*;
use super::super::g14_t_campaign::offer_forged;

/// Every bond's (collateral, slashed, kernel reserved, owed), the money a forged object must not move.
fn money(w: &World) -> Vec<(u64, u64, u128, u64)> {
    (0..w.net.chain.bonds.len()).map(|c| (w.net.collateral(c), w.net.slashed(c), w.net.kernel_reserved(c), w.net.owed(c))).collect()
}

/// What a replayed or late response could move once the clock runs on: the claim's verdict (state kind, convicted, served,
/// open demands of it) and every bond's collateral and slash. (Queued payouts drain through coinbases and reservations release on
/// their own horizon, so those are compared only where no time-driven release can intervene.)
fn verdict_and_bonds(w: &World, claim: &Digest) -> (String, bool, usize, usize, Vec<(u64, u64)>) {
    let l = w.net.ledger();
    let state = format!("{:?}", l.claims[claim].life.state).split([' ', '{']).next().unwrap_or_default().to_string();
    let served = l.served.keys().filter(|k| k.0 == *claim).count();
    let open = l.demands.keys().filter(|k| k.0 == *claim).count();
    let bonds = (0..w.net.chain.bonds.len()).map(|c| (w.net.collateral(c), w.net.slashed(c))).collect();
    (state, l.claims[claim].convicted, served, open, bonds)
}

/// **T03 on the slice: forged signatures, a fake Panel completion and fake court responses change no culpability and no money
/// before the default.** On a lie whose material at one position is withheld, an outsider's demand is open. Then:
/// (a) every seat's completion receipt with a corrupted signature; (b) the producer's correct response bytes under the producer's
/// bond but signed by another card's key; (c) a validly signed third-party response whose bytes do not serve the position. (a) and
/// (b) are refused before any effect (claim row and every bond's money identical); (c) is classified and costs ONLY its own signer
/// the dismissal fee — the demand stays open, the producer is not relieved. (d) The same signed junk replayed by another carrier is
/// dropped: no second fee (the T08 replay fix, aux 161). Positive control: the silent producer still defaults at the demand's
/// deadline with the fixed penalty, and the demander takes its share.
#[tokio::test]
async fn t0308_t03_slice_forged_signatures_fake_completion_and_fake_responses_have_no_effect() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(Mode::Panel).await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    let others = w.outsiders(&lie, &seats, 3);
    let (outsider, forger, junk_signer) = (others[0], others[1], others[2]);
    let pol = w.policy();

    // (a) a fake Panel completion: every seat's receipt, signature corrupted.
    let before = (w.net.ledger().claims[&lie.id].clone(), money(&w));
    let daa = w.net.daa();
    for (card, mut o) in colluding_receipts(&mut w.net, lie.id, daa) {
        let Obj::KernelConstraintReceiptV1 { signature, .. } = &mut o else { unreachable!("a receipt") };
        signature[11] ^= 0x20;
        offer_forged(&mut w.net, card, o).await;
    }
    assert!(!matches!(w.net.claim_state(&lie.id), ClaimStateV1::ProbabilisticPass { .. }), "forged receipts complete nothing");
    assert_eq!((w.net.ledger().claims[&lie.id].clone(), money(&w)), before, "and move no claim and no money");
    // Positive control: the genuine receipts complete the Panel.
    w.cover(&lie.id).await;

    // The outsider demands the withheld position.
    let p = lie.at.0;
    w.demand(outsider, &lie.id, p).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, p)].deadline_daa;
    let producer_before = w.net.collateral(0);

    // (b) the producer's correct response, its bond named, another card's key signing.
    let bytes = lie.position(&w.fx, p, |_| {});
    let mut forged = w.net.route(forger, &K::Respond { claim: lie.id, stage: 0, position: p, bytes: bytes.clone() });
    let Obj::KernelRouteV1 { signer, .. } = &mut forged else { unreachable!() };
    *signer = w.net.bond(0);
    let before = (w.net.ledger().claims[&lie.id].clone(), money(&w));
    offer_forged(&mut w.net, forger, forged).await;
    assert!(w.net.ledger().demands.contains_key(&(lie.id, 0, p)), "a forged response serves nothing");
    assert!(!w.net.ledger().served.contains_key(&(lie.id, 0, p)));
    assert_eq!((w.net.ledger().claims[&lie.id].clone(), money(&w)), before, "and moves no claim and no money");

    // (c) a validly signed third-party response whose bytes do not serve the position (one committed value dropped).
    let junk_bytes = lie.position(&w.fx, p, |r| {
        let slot = r.iter_mut().find(|s| !s.is_empty()).expect("a committed slot");
        slot.pop();
    });
    let junk = w.net.route(junk_signer, &K::Respond { claim: lie.id, stage: 0, position: p, bytes: junk_bytes });
    let signer_before = w.net.collateral(junk_signer);
    w.net.send(vec![(junk_signer, junk.clone())]).await;
    assert!(w.net.ledger().demands.contains_key(&(lie.id, 0, p)), "a fake response leaves the demand open");
    assert!(!w.net.ledger().served.contains_key(&(lie.id, 0, p)));
    assert_eq!(w.net.collateral(junk_signer), signer_before - pol.dismissed_proof_fee, "only its own signer pays the fee");
    assert_eq!(w.net.collateral(0), producer_before, "the producer is neither relieved nor charged by it");

    // (d) the same signed junk replayed by another carrier (the forger's funds): dropped, no second fee.
    let before = (w.net.ledger().clone().demands, money(&w));
    offer_forged(&mut w.net, forger, junk).await;
    let after = (w.net.ledger().clone().demands, money(&w));
    // The relay paid only its carrier's UTXO fee, which is no bond money.
    assert_eq!(after, before, "an old rejected signature is not judged (or charged) as a new response");

    // Positive control: the producer stays silent; the deadline defaults it, with the fixed penalty only.
    w.net.beat_to(deadline).await;
    assert!(
        matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        w.net.claim_state(&lie.id)
    );
    assert!(!w.net.ledger().claims[&lie.id].convicted, "a default is not a conviction");
    assert_eq!(w.net.collateral(0), producer_before - pol.default_penalty);
    assert_eq!(w.net.owed(outsider), default_share(&pol, None), "the demander's share");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T08 on the slice: the miner's PC is offline; a DISTINCT court signer answers the demand in time and a third-party relay carries
/// it.** After its claim is covered, card 0 never signs, funds or mines another object. An outsider demands one position. A distinct
/// bonded signer (not the producer, no seat, not the demander) signs the response from the public material it holds; an unrelated
/// relay card's carrier carries it. The demand closes before its deadline, the producer is not defaulted, the honest claim reaches
/// Final on its own clock and is paid once. The signer is the authority and the relay is no actor: neither is charged or credited.
/// The same signed response replayed later (by the relay or anyone) is refused — the position is served — and moves nothing.
#[tokio::test]
async fn t0308_t08_slice_offline_miner_distinct_signer_and_third_party_relay_answer_in_time() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(Mode::Panel).await;
    let (honest, seats) = claimed(&mut w, Mode::Panel, 0, false).await;
    let others = w.outsiders(&honest, &seats, 3);
    let (outsider, signer, relay) = (others[0], others[1], others[2]);
    let floor = final_floor(&w, Mode::Panel, &honest.id);
    let pol = w.policy();
    // From here on the miner is offline.
    let miner_funding = w.net.funding[0].clone();
    let miner_before = w.net.collateral(0);

    let p = honest.at.0;
    w.demand(outsider, &honest.id, p).await;
    let deadline = w.net.ledger().demands[&(honest.id, 0, p)].deadline_daa;
    let (signer_before, relay_before) =
        ((w.net.collateral(signer), w.net.kernel_reserved(signer)), (w.net.collateral(relay), w.net.kernel_reserved(relay)));

    // The distinct signer's response, carried and paid for by the relay.
    let bytes = honest.position(&w.fx, p, |_| {});
    let response = w.net.route(signer, &K::Respond { claim: honest.id, stage: 0, position: p, bytes });
    let Obj::KernelRouteV1 { signer: named, .. } = &response else { unreachable!() };
    assert_eq!(*named, w.net.bond(signer), "the object names the signer's bond, not the relay's");
    let signer_funding = w.net.funding[signer].clone();
    w.net.send(vec![(relay, response.clone())]).await;
    assert!(w.net.daa() < deadline, "answered before the demand's deadline ({} < {deadline})", w.net.daa());
    assert!(w.net.ledger().served.contains_key(&(honest.id, 0, p)), "the distinct signer's response serves the position");
    assert!(!w.net.ledger().demands.contains_key(&(honest.id, 0, p)), "and closes the demand in time");
    assert_eq!(w.net.funding[0], miner_funding, "the offline miner funded nothing");
    assert_eq!(w.net.funding[signer], signer_funding, "the signer funded no carrier: the relay did");
    assert_eq!((w.net.collateral(signer), w.net.kernel_reserved(signer)), signer_before, "a valid response costs its signer nothing");
    assert_eq!((w.net.collateral(relay), w.net.kernel_reserved(relay)), relay_before, "the relay is neither liable nor charged");

    // An old signature replayed as a "new" response: refused, nothing moves.
    let before = verdict_and_bonds(&w, &honest.id);
    offer_forged(&mut w.net, relay, response).await;
    assert_eq!(verdict_and_bonds(&w, &honest.id), before, "the replay is no new response");

    // No default at the deadline; the honest claim reaches Final (after the served values' proof grace) and is paid once.
    // Beat one block at a time and read the payout queue at the block that makes it Final (later coinbases drain it).
    let limit = deadline.max(floor).max(w.net.daa() + pol.proof_grace_daa) + 4;
    while !matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }) {
        assert!(w.net.daa() <= limit, "the answered honest claim finalizes: {:?}", w.net.claim_state(&honest.id));
        let next = w.net.daa() + 1;
        w.net.beat_to(next).await;
    }
    assert_eq!(w.net.collateral(0), miner_before, "the offline miner was not defaulted");
    assert_eq!(w.net.owed(0), pol.claim_reward, "and is paid once (queued at its Final)");
    w.net.beat_to(deadline + 1).await;
    assert_eq!(w.net.collateral(0), miner_before, "and no default lands at the old deadline either");
    assert_eq!((w.net.owed(signer), w.net.owed(relay)), (0, 0), "neither the signer nor the relay is credited");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T08 negative control: a response carried after the deadline is not an in-time fulfilment.** The miner is offline and nobody
/// answers in time: the demand defaults at its deadline. The correct response signed by a distinct signer and relayed afterwards is
/// refused (no open demand) and undoes nothing: the default and its penalty stand, and the late signer is charged nothing.
#[tokio::test]
async fn t0308_t08_slice_a_late_relayed_response_does_not_undo_the_default() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(Mode::Panel).await;
    let (honest, seats) = claimed(&mut w, Mode::Panel, 0, false).await;
    let others = w.outsiders(&honest, &seats, 3);
    let (outsider, signer, relay) = (others[0], others[1], others[2]);
    let pol = w.policy();
    let miner_before = w.net.collateral(0);
    let p = honest.at.0;
    w.demand(outsider, &honest.id, p).await;
    let deadline = w.net.ledger().demands[&(honest.id, 0, p)].deadline_daa;
    let bytes = honest.position(&w.fx, p, |_| {});
    let response = w.net.route(signer, &K::Respond { claim: honest.id, stage: 0, position: p, bytes });
    w.net.beat_to(deadline).await;
    assert!(
        matches!(w.net.claim_state(&honest.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        w.net.claim_state(&honest.id)
    );
    let before = verdict_and_bonds(&w, &honest.id);
    offer_forged(&mut w.net, relay, response).await;
    assert!(!w.net.ledger().served.contains_key(&(honest.id, 0, p)), "a late response serves nothing");
    assert_eq!(verdict_and_bonds(&w, &honest.id), before, "and undoes nothing");
    assert_eq!(w.net.collateral(0), miner_before - pol.default_penalty);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
