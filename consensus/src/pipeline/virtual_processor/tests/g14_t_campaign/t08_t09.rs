//! T08 miner PC offline / distinct court signer / third-party relay; T09 shortest dependency leases and one bond behind many claims.
use super::*;

/// **T08 (route part): the miner's PC is offline; a third party relays the miner's signed response.** After committing, card 0 never
/// funds or mines another carrier. Its response to the outsider's demand — signed by the miner's bond key (prepared before it went
/// offline, the current duty's authority) — is carried and paid for by an unrelated relay card's Full-UTXO carrier. The fold accepts
/// it by the OBJECT's signature and the served material's commitment, not by who paid the carrier; a relayed response never makes
/// the relay liable or paid. (Whether a response signed by a NON-duty key may serve public material is the full T08 below.)
#[tokio::test]
async fn t08_miner_offline_third_party_relay_carries_the_current_duty_signature() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let others = w.outsiders(&lie, &seats, 2);
    let (outsider, relay) = (others[0], others[1]);
    let relay_before = (w.net.collateral(relay), w.net.kernel_reserved(relay));
    w.demand(outsider, &lie.id, lie.at.0).await;

    let bytes = lie.position(&w.fx, lie.at.0, |_| {});
    // The miner's own signed response (signed while online), carried and paid by the relay.
    let signed_by_miner = w.net.route(0, &K::Respond { claim: lie.id, stage: 0, position: lie.at.0, bytes });
    let funding_of_miner = w.net.funding[0].clone();
    w.net.send(vec![(relay, signed_by_miner)]).await;
    assert_eq!(w.net.funding[0], funding_of_miner, "the offline miner funded nothing");
    assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "the relayed miner response answers the demand");
    assert_eq!((w.net.collateral(relay), w.net.kernel_reserved(relay)), relay_before, "the relay is neither liable nor charged");

    // The court continues without the miner: the outsider convicts from the served values.
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x08);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(w.net.owed(relay), 0, "the relay earns nothing from the court");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T08 (full)**: a DISTINCT court signer — not the miner's bond key — answers under a current-duty delegation the miner signed;
/// the relay carries it; after the delegation's lease ends the same signer is refused.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-clock-deps-relay (current-duty authority / relay delegation object).
#[tokio::test]
#[ignore = "blocked on design: designated court-signer delegation not implemented (r5-clock-deps-relay notes 4)"]
async fn t08_distinct_court_signer_under_current_duty_authority_and_relay() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (lie, seats) = covered_claim(&mut w, 0, true).await;
        let others = w.outsiders(&lie, &seats, 3);
        let (outsider, relay, court_signer) = (others[0], others[1], others[2]);
        // INTENDED API: the miner's signed delegation of its current duty to `court_signer` for a bounded lease.
        let delegation = K::DelegateDuty { claim: lie.id, delegate: w.net.kid(court_signer), until_daa: w.net.daa() + 40 };
        let o = w.net.route(0, &delegation);
        w.net.send(vec![(relay, o)]).await;
        w.demand(outsider, &lie.id, lie.at.0).await;
        let bytes = lie.position(&w.fx, lie.at.0, |_| {});
        let o = w.net.route(court_signer, &K::Respond { claim: lie.id, stage: 0, position: lie.at.0, bytes });
        w.net.send(vec![(relay, o)]).await;
        assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "the delegate answered under current authority");
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!("a designated court-signer key / duty delegation is NOT implemented (r5-clock-deps-relay notes 4: needs a versioned bond record, RFC-0026 6 separate spec); K::DelegateDuty does not exist. The merged authority is palw_court_current_signer_key_v2 (the bond's CURRENT own key), which the route-part relay test t08_miner_offline_third_party_relay_carries_the_current_duty_signature exercises");
}

/// **T09 (route part): several claims share ONE bond; the aggregate reservation is funded and settled claim by claim.** Card 0 commits
/// two claims on two jobs; its real bond carries both reservations at once. One is a lie and is convicted (only its reservation is
/// slashed); the other is honest, finalizes and releases at its own horizon — the shared bond never pays for the wrong claim and is
/// never over-reserved.
#[tokio::test]
async fn t09_multiple_claims_sharing_one_bond_reserve_in_aggregate_and_settle_independently() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let before = w.net.collateral(0);
    let job_a = w.job().await;
    let job_b = w.job().await;
    let lie = w.claim(0, &job_a, true).await;
    let honest = w.claim(0, &job_b, false).await;
    assert_eq!(w.net.kernel_reserved(0), 2 * u128::from(pol.claim_collateral), "both claims are reserved on the one bond");
    assert!(u128::from(w.net.collateral(0)) >= w.net.kernel_reserved(0), "and the aggregate reservation is funded");
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    w.cover(&honest.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x09);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert!(!w.net.ledger().claims[&honest.id].convicted, "the shared bond's other claim is untouched");
    assert_eq!(w.net.collateral(0), before - pol.claim_collateral, "only the liar's reservation was slashed");
    assert_eq!(w.net.kernel_reserved(0), u128::from(pol.claim_collateral), "the honest claim's reservation stays held");
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else {
        panic!("{:?}", w.net.claim_state(&honest.id))
    };
    w.net.beat_to(window_end_daa).await;
    let horizon = w.net.ledger().claims[&honest.id].liability_until.expect("liability runs");
    w.net.beat_to(horizon + 1).await;
    assert_eq!(w.net.kernel_reserved(0), 0, "released at the honest claim's own horizon");
    assert_eq!(w.net.collateral(0), before - pol.claim_collateral);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T09 (full)**: the SHORTEST signed dependency leases (material, relay, retention) and several claims on one bond — the aggregate
/// reservation at admission covers every claim's worst legal branch to its lease end, and a claim whose lease would end before its
/// latest lawful response is refused at admission.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-clock-deps-relay (signed dependency leases), r5-original-admission (aggregate reservation at admission).
#[tokio::test]
#[ignore = "blocked on value V2 and unrecorded lease horizons (lanes merged on r7)"]
async fn t09_shortest_signed_dependency_leases_and_shared_bond_aggregate_reservation() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (a, _) = covered_claim(&mut w, 0, false).await;
        let (b, _) = covered_claim(&mut w, 0, false).await;
        let state = w.net.chain.tip_state().1;
        let agg = state.palw_bond_aggregate_reservation_v1(&w.net.bond(0));
        let need: u128 = [a.id, b.id].iter().map(|c| state.palw_funded_original_v1(c).unwrap().worst_branch_cost()).sum();
        assert!(agg >= need, "aggregate reservation {agg} covers {need}");
        let short = state.palw_dependency_leases_v1(&a.id).into_iter().map(|l| l.until_daa).min().unwrap();
        assert!(short >= state.palw_funded_original_v1(&a.id).unwrap().latest_lawful_response_daa());
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!("the aggregate funded reservation needs an admitted funded original (palw_funded_original_schedule_v1 typed-missing, V2); signed dependency leases are not state fields (r5-clock-deps-relay notes 3 OPEN); intended palw_bond_aggregate_reservation_v1 / palw_dependency_leases_v1 do not exist");
}
