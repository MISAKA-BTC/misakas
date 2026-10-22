//! T12 hard-boundary concentration; T13 the oldest claim loses service while an independent claim still progresses.
use super::*;

/// **T12 (route part, scaled): concentration at one boundary.** `G14_T12_N` claims (default 3) are covered in the SAME block, so their
/// challenge windows all end at the same DAA; a demand is filed on each in the window's last blocks, so every court deadline also
/// concentrates. Every accepted duty is funded (each demand bond reserved, each claim's collateral reserved, each escrow live), every
/// outcome is exact at the boundary (each answered demand returns, each claim reaches Final by the shared hard bound), and the chain
/// does not stall at the concentrated DAA.
#[tokio::test]
async fn t12_concentration_at_one_boundary_every_accepted_duty_funded_and_exact() {
    kaspa_core::log::try_init_logger("warn");
    let n = campaign_n("G14_T12_N", 3);
    let mut w = World::new().await;
    let pol = w.policy();
    let producers: Vec<usize> = (0..cards(&w)).filter(|c| *c != 1).collect();
    let mut claims = Vec::new();
    for i in 0..n {
        let job = w.job().await;
        claims.push(w.claim(producers[i % producers.len()], &job, false).await);
    }
    // Every seat's receipts for every claim in ONE send: the claims pass together.
    let daa = w.net.daa();
    let mut receipts = Vec::new();
    for c in &claims {
        receipts.extend(colluding_receipts(&mut w.net, c.id, daa));
    }
    w.net.send(receipts).await;
    let ends: std::collections::BTreeSet<u64> = claims
        .iter()
        .map(|c| match w.net.claim_state(&c.id) {
            ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => window_end_daa,
            other => panic!("covered: {other:?}"),
        })
        .collect();
    let end = *ends.iter().max().unwrap();
    assert!(ends.len() <= 2, "the windows concentrate on (at most two adjacent) DAAs: {ends:?}");
    // Demands concentrated in the window's last blocks, by distinct demanders.
    let all_seats: std::collections::BTreeSet<usize> = claims.iter().flat_map(|c| w.seats(&c.id)).collect();
    let demanders: Vec<usize> =
        (0..cards(&w)).filter(|c| *c != 1 && !all_seats.contains(c) && !claims.iter().any(|k| k.producer == *c)).collect();
    assert!(!demanders.is_empty(), "a demander outside every Panel");
    w.net.beat_to(end.saturating_sub(4)).await;
    let mut items = Vec::new();
    for (i, c) in claims.iter().enumerate() {
        let d = demanders[i % demanders.len()];
        let o = w.net.route(d, &K::FileDemand { demander: w.net.kid(d), claim: c.id, stage: 0, position: 1 });
        items.push((d, o));
    }
    w.net.send(items).await;
    let open = w.net.ledger().demands.len();
    let mut reserved_demands: BTreeMap<usize, u128> = BTreeMap::new();
    for d in w.net.ledger().demands.values() {
        for (kid, amount) in &d.demanders {
            let card = (0..cards(&w)).find(|c| w.net.kid(*c) == *kid).expect("a card");
            *reserved_demands.entry(card).or_default() += u128::from(*amount);
        }
    }
    for (card, r) in &reserved_demands {
        assert!(w.net.kernel_reserved(*card) >= *r, "card {card}: every accepted demand is funded");
    }
    // Every producer answers at the concentrated deadline's last lawful block.
    let deadline = w.net.ledger().demands.values().map(|d| d.deadline_daa).max().unwrap_or(end);
    w.net.beat_to(deadline.saturating_sub(3)).await;
    let mut answers = Vec::new();
    for c in &claims {
        if w.net.ledger().demands.contains_key(&(c.id, 0, 1)) {
            let bytes = c.position(&w.fx, 1, |_| {});
            answers.push((c.producer, w.net.route(c.producer, &K::Respond { claim: c.id, stage: 0, position: 1, bytes })));
        }
    }
    w.net.send(answers).await;
    assert!(w.net.ledger().demands.is_empty(), "all {open} concentrated demands answered before the boundary");
    let hard = end + pol.court_deadline_daa + pol.proof_grace_daa;
    w.net.beat_to(hard + 1).await;
    for c in &claims {
        let ClaimStateV1::Final { final_daa } = w.net.claim_state(&c.id) else { panic!("{:?}", w.net.claim_state(&c.id)) };
        assert!(final_daa <= hard, "exact outcome within the hard bound");
    }
    assert!(w.net.ledger().job_escrows.is_empty(), "every escrow paid exactly once");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T12 (full)**: concentration at the ORIGINAL's hard boundary of 10,000 DAA — every duty accepted for every admitted original is
/// funded by its admission reservation and settles to its exact outcome at the boundary.
// DEPENDS ON lane r5-original-admission (hard 10,000 boundary), r5-escrow-settlement (exact settlement).
#[tokio::test]
#[ignore = "needs lane r5-original-admission + r5-escrow-settlement merge"]
async fn t12_hard_boundary_concentration_at_10000_daa_every_duty_funded() {
    #[cfg(feature = "g14-t-campaign-merged")]
    {
        let n = campaign_n("G14_T12_N", 3);
        let mut w = World::new().await;
        let mut ids = Vec::new();
        for i in 0..n {
            let (c, _) = covered_claim(&mut w, [0usize, 2, 3][i % 3], false).await;
            ids.push(c.id);
        }
        let hard = w.net.chain.tip_state().1.palw_funded_original_v1(&ids[0]).unwrap().hard_until_daa();
        w.net.beat_to(hard + 1).await;
        for id in &ids {
            let o = w.net.chain.tip_state().1.palw_original_escrow_outcome_v1(id).expect("settled at the boundary");
            assert!(o.is_exact_and_funded());
        }
    }
    #[cfg(not(feature = "g14-t-campaign-merged"))]
    pending_lane!("r5-original-admission + r5-escrow-settlement");
}

/// **T13: the oldest claim loses service; an independent claim still progresses; unresolved evidence and bonds stay held without a
/// false slash.** Claim A (older, card 0) withholds a value and never answers its demand; claim B (newer, a different producer) is
/// honest. B reaches Final while A is still in its demand; A defaults (fixed penalty only — never a fraud slash); the rest of A's
/// reservation and the demand evidence stay HELD through A's liability horizon, then release with no further slash; B's producer
/// and B's escrow are never touched by A's failure.
#[tokio::test]
async fn t13_oldest_claim_loses_service_independent_claim_progresses_and_nothing_is_falsely_slashed() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let job_a = w.job().await;
    let a = w.claim(0, &job_a, true).await;
    let seats_a = w.seats(&a.id);
    w.cover(&a.id).await;
    let b_producer = (0..cards(&w)).find(|c| *c != 0 && *c != 1 && !seats_a.contains(c)).expect("an independent producer");
    let (b, _) = covered_claim(&mut w, b_producer, false).await;
    let outsider = (0..cards(&w))
        .find(|c| ![0, 1, b_producer].contains(c) && !seats_a.contains(c))
        .expect("an outsider");
    let a_before = w.net.collateral(0);
    let b_before = w.net.collateral(b_producer);

    w.demand(outsider, &a.id, a.at.0).await; // A's server is gone: nobody answers
    let deadline = w.net.ledger().demands[&(a.id, 0, a.at.0)].deadline_daa;
    let ClaimStateV1::ProbabilisticPass { window_end_daa: b_end, .. } = w.net.claim_state(&b.id) else { panic!() };
    w.net.beat_to(b_end.max(deadline)).await;
    assert!(matches!(w.net.claim_state(&b.id), ClaimStateV1::Final { .. }), "B progressed: {:?}", w.net.claim_state(&b.id));
    assert!(
        matches!(w.net.claim_state(&a.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "A lost service: {:?}",
        w.net.claim_state(&a.id)
    );
    assert!(!w.net.ledger().claims[&a.id].convicted, "no false fraud slash");
    assert_eq!(w.net.collateral(0), a_before - pol.default_penalty, "only the fixed penalty");
    assert_eq!(w.net.kernel_reserved(0), u128::from(pol.claim_collateral - pol.default_penalty), "the rest of A stays HELD");
    assert_eq!(w.net.collateral(b_producer), b_before, "B's producer untouched by A's failure");
    let horizon = w.net.ledger().claims[&a.id].liability_until.expect("A's liability horizon");
    w.net.beat_to(horizon + 1).await;
    assert_eq!(w.net.kernel_reserved(0), 0, "released at the horizon");
    assert_eq!(w.net.collateral(0), a_before - pol.default_penalty, "never slashed for the held evidence");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
