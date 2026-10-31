//! T04 separate floods (normal tx / paid court / invalid codec); T05 every owner occupied and refilled by colluders, one outsider.
use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;

/// A route carrier whose signed bytes are NOT a decodable kernel object (an invalid-codec filing), validly signed by `card`.
fn junk_codec(net: &mut Net, card: usize, n: u8) -> Obj {
    let bytes = vec![0xC0 ^ n; 96 + n as usize];
    let signer = net.bond(card);
    let message = palw_kernel_route_message_v1(net.domain, &signer, &bytes);
    net.rnd = net.rnd.wrapping_add(1);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        [net.rnd; 32],
    )
    .expect("ML-DSA-87 signs")
    .as_ref()
    .to_vec();
    Obj::KernelRouteV1 { bytes, signer, signature }
}

fn block_budget(net: &Net) -> Option<(u64, u32, u64)> {
    net.chain.tip_state().1.kernel_route().and_then(|k| k.aux_row(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
}

/// **T04 (route part): three SEPARATE floods, each bounded, none starving the honest proof.** Phase A — an invalid-codec flood (every
/// flooder card, several carriers each): refused before any court, no state moved, the chain keeps ticking. Phase B — a paid-court
/// flood (demands on every position from every flooder): each block's adjudications stay within the policy's per-block budget, every
/// session lives at most its court deadline. Phase C — the outsider's direct proof convicts in the block that carries it despite the
/// flood, and every flooded session settles moot with its bond returned. The phases are separate so each measurement is attributable.
#[tokio::test]
async fn t04_invalid_codec_and_paid_court_floods_are_bounded_separately_and_never_starve_a_proof() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let mut flooders = w.outsiders(&lie, &seats, 3);
    let outsider = flooders.pop().unwrap();
    let claim_before = w.net.ledger().claims[&lie.id].clone();

    // ---- A: invalid-codec flood ----
    let start_daa = w.net.daa();
    let mut codec_items = 0;
    let mut pool_refused = 0;
    for (i, card) in flooders.clone().into_iter().enumerate() {
        for k in 0..3u8 {
            let o = junk_codec(&mut w.net, card, (i as u8) * 4 + k);
            codec_items += 1;
            if !offer_forged(&mut w.net, card, o).await {
                pool_refused += 1;
            }
        }
    }
    eprintln!("t04/A: {pool_refused} of {codec_items} invalid-codec carriers refused at the pool; the rest dropped at the fold");
    assert!(w.net.daa() > start_daa, "the chain ticks through the codec flood");
    assert_eq!(w.net.ledger().claims[&lie.id], claim_before, "the codec flood moved nothing");
    for c in &flooders {
        assert_eq!(w.net.kernel_reserved(*c), 0, "no codec filing reserved anything");
    }
    if let Some((_, adjudications, _)) = block_budget(&w.net) {
        assert!(u64::from(adjudications) <= u64::from(pol.max_adjudications_per_block), "codec junk never exceeds the block budget");
    }
    eprintln!("t04/A: {codec_items} invalid-codec carriers, DAA {start_daa} -> {}", w.net.daa());

    // ---- B: paid-court flood ----
    let positions = {
        let ledger = w.net.ledger();
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[&lie.id].body else { panic!() };
        evidence.positions
    };
    let mut items = Vec::new();
    for p in 0..positions {
        for c in &flooders {
            let o = w.net.route(*c, &K::FileDemand { demander: w.net.kid(*c), claim: lie.id, stage: 0, position: p });
            items.push((*c, o));
        }
    }
    let before_len = chain_len(&w.net);
    w.net.send(items).await;
    for (_, adjudications, _) in block_budget(&w.net).into_iter() {
        assert!(u64::from(adjudications) <= u64::from(pol.max_adjudications_per_block), "the court flood is bounded per block");
    }
    assert!(chain_len(&w.net) > before_len, "the flood rode blocks without stalling the chain");
    for d in w.net.ledger().demands.values() {
        assert!(d.deadline_daa <= d.filed_daa + pol.court_deadline_daa, "every flooded session lives at most the court deadline");
    }

    // ---- C: the outsider's proof convicts through the flood ----
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x04);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the direct proof convicts despite both floods");
    assert!(w.net.ledger().demands.is_empty(), "every flooded session settled moot");
    for c in &flooders {
        assert_eq!(w.net.kernel_reserved(*c), 0, "card {c}: its flood bonds returned at the conviction");
    }
    assert_eq!(w.net.slashed(0), pol.claim_collateral, "one slash");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T04 (native part)**: the normal-tx flood, the paid-court flood and the invalid-codec flood measured SEPARATELY against the
/// bounded native intake — actual supply per block and the calibrated per-op cost table, with a normal ordinary-tx flood never
/// displacing a paid court op that paid its quote.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-f02-wire (shared-court op object + native intake bound), r5-f03-ingress (paid ingress quote).
#[tokio::test]
#[ignore = "blocked on values V1+V4+V5 (measurement M-C) (lanes merged on r7)"]
async fn t04_native_intake_separates_normal_paid_court_and_invalid_codec_floods_with_calibrated_costs() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (lie, seats) = covered_claim(&mut w, 0, true).await;
        let outsider = w.outsiders(&lie, &seats, 1)[0];
        // INTENDED API: the native intake's per-block supply and the calibrated cost table the quote is derived from.
        let intake = w.net.chain.tip_state().1.palw_native_court_intake_v1().expect("bounded native intake");
        let quote = w.net.chain.tip_state().1.palw_paid_ingress_quote_v1(&lie.id).expect("quote");
        assert_eq!(quote.fee, intake.calibrated_cost_of(misaka_palw_kernel::court::SharedOpKindV1::Prosecute));
        // A normal-tx flood filling the block's ordinary mass must not displace the paid op.
        let o = w.net.route(outsider, &K::PaidIngressOpen { claim: lie.id, payer: w.net.kid(outsider), fee: quote.fee });
        w.net.send(vec![(outsider, o)]).await;
        assert!(w.net.chain.tip_state().1.palw_paid_ingress_v1(&lie.id, &w.net.kid(outsider)).is_some());
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!(
        "tag 255 MaterialIntakeV1 is refused on every node (material_intake_schedule_v1 typed-missing, V5); the paid-ingress fee is typed-missing (V4); no calibrated per-op cost table exists (V1 from measurement M-C); intended palw_native_court_intake_v1 / palw_paid_ingress_quote_v1 / palw_paid_ingress_v1 / K::PaidIngressOpen do not exist"
    );
}

/// **T05 (route part): every owner slot occupied and refilled by colluders; exactly one external verifier finishes.** The producer and
/// every Panel seat collude; the colluders (the seats themselves) open a demand session on EVERY position of the claim and refill
/// them in every following block. The one outsider — a bonded card outside the Panel, built only from the read API and the public
/// DA — files its direct proof; it convicts in the block that carries it: exactly one conviction, one slash, one accuser reward (to
/// the outsider), every colluding session settled moot.
#[tokio::test]
async fn t05_every_owner_slot_refilled_by_colluders_and_exactly_one_outsider_finishes() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let before = w.net.collateral(0);
    let (lie, seats) = covered_claim(&mut w, 0, true).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let positions = {
        let ledger = w.net.ledger();
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[&lie.id].body else { panic!() };
        evidence.positions
    };
    // Occupy, then refill twice (the refills join or are refused; none frees a slot for the outsider).
    for round in 0..3 {
        let mut items = Vec::new();
        for p in 0..positions {
            let c = seats[(p as usize + round) % seats.len()];
            let o = w.net.route(c, &K::FileDemand { demander: w.net.kid(c), claim: lie.id, stage: 0, position: p });
            items.push((c, o));
        }
        w.net.send(items).await;
        assert_eq!(w.net.ledger().demands.len(), positions as usize, "round {round}: every position's slot is held by colluders");
    }
    // The outsider needs no slot: a direct proof from public material alone.
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x05);
    w.proof(outsider, &lie.id, proof).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "the one outsider finished");
    assert!(ledger.demands.is_empty(), "every colluding session settled moot");
    assert_eq!(w.net.collateral(0), before - pol.claim_collateral, "exactly one slash");
    let reward = pol.claim_collateral * u64::from(pol.accuser_reward_permille) / 1000;
    assert_eq!(w.net.owed(outsider), reward, "exactly one accuser reward, to the outsider");
    for s in &seats {
        assert_eq!(w.net.owed(*s), 0, "no colluder is paid");
    }
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T05 (full)**: all 8 shared-original owner slots occupied and continuously refilled by colluders, and exactly ONE external
/// verifier finishes through FAIR PAID PUBLIC INGRESS (it pays the published quote, is admitted ahead of no one and behind no one,
/// and completes the court op on the shared original). The colluders' refills never evict or delay the paid outsider past its
/// lawful deadline.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-f03-ingress (paid public ingress object), r5-f02-wire (shared-court op), r5-original-admission.
#[tokio::test]
#[ignore = "blocked on value V4 and a node-level shared original (lanes merged on r7)"]
async fn t05_all_owners_refilled_by_colluders_one_outsider_finishes_through_fair_paid_public_ingress() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        const OWNERS: usize = 8;
        let mut w = World::new().await;
        let (lie, seats) = covered_claim(&mut w, 0, true).await;
        let outsider = w.outsiders(&lie, &seats, 1)[0];
        let colluders: Vec<usize> = (0..cards(&w)).filter(|c| *c != outsider && *c != 1).collect();
        for round in 0..4 {
            let mut items = Vec::new();
            for slot in 0..OWNERS {
                let c = colluders[(slot + round) % colluders.len()];
                let quote = w.net.chain.tip_state().1.palw_paid_ingress_quote_v1(&lie.id).unwrap();
                let o = w.net.route(c, &K::PaidIngressOpen { claim: lie.id, payer: w.net.kid(c), fee: quote.fee });
                items.push((c, o));
            }
            w.net.send(items).await;
            assert_eq!(w.net.chain.tip_state().1.palw_paid_ingress_owners_v1(&lie.id).len(), OWNERS, "every owner occupied");
        }
        let quote = w.net.chain.tip_state().1.palw_paid_ingress_quote_v1(&lie.id).unwrap();
        let o = w.net.route(outsider, &K::PaidIngressOpen { claim: lie.id, payer: w.net.kid(outsider), fee: quote.fee });
        w.net.send(vec![(outsider, o)]).await;
        let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x55);
        w.proof(outsider, &lie.id, proof).await;
        assert!(w.net.ledger().claims[&lie.id].convicted, "exactly one external verifier finished");
        let finishers = w.net.chain.tip_state().1.palw_paid_ingress_finishers_v1(&lie.id);
        assert_eq!(finishers, vec![w.net.kid(outsider)]);
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!(
        "no paid-ingress object exists (the fee rides TIR CourtOpened; palw_court_ingress_schedule_v1 typed-missing, V4); the advance-reserved outside slot needs a TIR claim admitted past palw_court_outsider_lane_v1 with a CourtSharedContext (no production installer); intended palw_paid_ingress_quote_v1 / _owners_v1 / _finishers_v1 / K::PaidIngressOpen do not exist"
    );
}
