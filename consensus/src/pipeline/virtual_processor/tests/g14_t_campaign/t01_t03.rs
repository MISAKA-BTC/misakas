//! T01 honest miner / wrong challenger; T02 permanent miner silence; T03 forged signatures and fake Panel completion.
use super::*;

/// **T01 (route part): an honest miner and a WRONG challenger.** Every seat signs honestly-passing receipts; a challenger demands a
/// position of an honest claim; the miner serves it; a fresh verifier built from the read API finds nothing to prosecute; a junk proof
/// the challenger files anyway is dismissed against the challenger. The claim reaches Final on its own clock, the miner is paid its
/// reward exactly once out of the poster's escrow, and the challenger's demand bond is NOT refunded at the unconvicted horizon (the
/// wrong demand costs its filer, never the honest miner).
#[tokio::test]
async fn t01_honest_miner_and_wrong_challenger_route_outcomes_are_funded_and_exact() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let escrow = w.net.ledger().job_escrows[&job.id()].amount;
    let miner_before = w.net.collateral(0);
    let honest = w.claim(0, &job, false).await;
    let seats = w.seats(&honest.id);
    w.cover(&honest.id).await;
    let challenger = w.outsiders(&honest, &seats, 1)[0];
    let challenger_before = w.net.collateral(challenger);
    let pol = w.policy();
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else {
        panic!("{:?}", w.net.claim_state(&honest.id))
    };

    w.demand(challenger, &honest.id, 1).await;
    assert_eq!(w.net.kernel_reserved(challenger), u128::from(pol.demand_bond), "the wrong demand reserves its filer's bond");
    w.serve(0, &honest, 1).await;
    assert!(w.net.ledger().demands.is_empty(), "the honest miner answered");
    let finding = w.fresh(0x01).check(honest.id, &honest.published(&w.fx, &[]), &w.fx.params);
    assert_eq!(finding, OutsiderFindingV1::Clean, "nothing to prosecute on an honest claim");

    // The challenger files a proof anyway: dismissed, at its own cost, never the miner's.
    let o = w.net.route(
        challenger,
        &K::FileProof { accuser: w.net.kid(challenger), claim: honest.id, proof: ProsecutionV1::Kernel(vec![0u8; 64]) },
    );
    w.net.send(vec![(challenger, o)]).await;
    assert!(!w.net.ledger().claims[&honest.id].convicted, "a wrong proof never convicts");
    assert_eq!(w.net.slashed(0), 0, "and never touches the honest miner");
    assert!(w.net.collateral(challenger) < challenger_before, "the dismissed filing costs its filer");

    w.net.beat_to(window_end_daa + pol.court_deadline_daa + pol.proof_grace_daa).await;
    assert!(matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&honest.id));
    let (minted, owed) = minted_and_owed(&w.net, &w.net.chain, 0);
    assert_eq!(minted + owed, escrow, "the miner's reward is paid exactly once, out of the escrow (minted or still queued)");
    assert_eq!(escrow, pol.claim_reward);
    assert!(w.net.ledger().job_escrows.is_empty(), "the escrow paid once");
    let horizon = w.net.ledger().claims[&honest.id].liability_until.expect("liability runs after Final");
    w.net.beat_to(horizon + 1).await;
    assert_eq!(w.net.kernel_reserved(0), 0, "the honest miner's reservation is released at the horizon");
    assert_eq!(w.net.collateral(0), miner_before, "and its bond is whole");
    assert_eq!(w.net.kernel_reserved(challenger), 0, "the wrong demand's bond is settled at the horizon");
    assert!(
        w.net.collateral(challenger) <= challenger_before - pol.demand_bond,
        "the unconvicted horizon burns the wrong demand's bond (G14-R4), never refunds it"
    );
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T01 (shared-original part)**: the same honest miner / wrong challenger, but through the COMPLETE shared-original duty graph —
/// funded original admission reserving every legal branch, the paid public ingress carrying the challenger, and the escrow
/// settlement paying the miner and charging the challenger by the funded outcome table.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-original-admission (funded original admission), r5-escrow-settlement (settlement paths), r5-f03-ingress.
#[tokio::test]
#[ignore = "blocked on values V2+V3+V4 and a node-level shared original (lanes merged on r7)"]
async fn t01_honest_miner_and_wrong_challenger_through_shared_original_duties_and_funded_outcomes() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        use kaspa_consensus_core::palw_court_original_escrow_v1::PalwOriginalEscrowOutcomeV1 as Outcome;
        let mut w = World::new().await;
        // INTENDED API (r5-original-admission): admit the claim's original with every legal branch reserved at admission.
        let (honest, seats) = covered_claim(&mut w, 0, false).await;
        let admitted = w.net.chain.tip_state().1.palw_funded_original_v1(&honest.id).expect("funded original admitted");
        assert!(admitted.reserves_every_legal_branch(), "every legal branch is funded at admission");
        let challenger = w.outsiders(&honest, &seats, 1)[0];
        // INTENDED API (r5-f03-ingress): the challenger enters through the paid public ingress, not a seat.
        let ticket = w.net.chain.tip_state().1.palw_paid_ingress_quote_v1(&honest.id).expect("ingress quote");
        let o = w.net.route(challenger, &K::PaidIngressOpen { claim: honest.id, payer: w.net.kid(challenger), fee: ticket.fee });
        w.net.send(vec![(challenger, o)]).await;
        // INTENDED API (r5-escrow-settlement): the outcome row is the funded one for an honest claim.
        let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else { panic!() };
        w.net.beat_to(window_end_daa + 64).await;
        let outcome = w.net.chain.tip_state().1.palw_original_escrow_outcome_v1(&honest.id).expect("settled");
        assert_eq!(outcome, Outcome::HonestFinal { challenger_charged: ticket.fee });
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!("no funded original can be admitted on a node (palw_funded_original_schedule_v1 typed-missing, V2) so no PalwFundedEscrowRowV1 exists; escrow dues palw_escrow_fee_schedule_v1 = None (V3); there is no paid-ingress object (the paid ingress is a once-only fee on TIR CourtOpened, palw_court_ingress_schedule_v1 typed-missing, V4); intended palw_funded_original_v1 / palw_paid_ingress_quote_v1 / palw_original_escrow_outcome_v1 / K::PaidIngressOpen do not exist; a kernel-route claim carries no shared original");
}

/// **T02 (route part): permanent miner silence.** The miner publishes everything but one value and never answers again; an outsider
/// whose fresh check needs that position (the actual supplied opportunity: a demand on chain with its deadline) files it; the
/// deadline's tick is an eligible availability default — the fixed penalty, never a conviction; the demander is paid the default
/// share and its bond returns; past the default's liability horizon nothing more is taken from the silent miner.
#[tokio::test]
async fn t02_permanent_miner_silence_is_a_correct_eligible_default_after_an_actual_opportunity() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let silent = w.claim(0, &job, true).await;
    let seats = w.seats(&silent.id);
    w.cover(&silent.id).await;
    let outsider = w.outsiders(&silent, &seats, 1)[0];
    let pol = w.policy();

    let da = silent.published(&w.fx, &[silent.at]);
    let finding = w.fresh(0x02).check(silent.id, &da, &w.fx.params);
    assert_eq!(finding, OutsiderFindingV1::Demand(vec![(0, silent.at.0)]), "the opportunity is the one missing position");
    w.demand(outsider, &silent.id, silent.at.0).await;
    let row = w.net.ledger().demands[&(silent.id, 0, silent.at.0)].clone();
    assert_eq!(row.deadline_daa, row.filed_daa + pol.court_deadline_daa, "the opportunity is the full court deadline");
    // The miner is silent forever: no Respond from card 0 from here on.
    w.net.beat_to(row.deadline_daa).await;
    assert!(
        matches!(w.net.claim_state(&silent.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        w.net.claim_state(&silent.id)
    );
    assert!(!w.net.ledger().claims[&silent.id].convicted, "silence is a default, not fraud");
    assert_eq!(w.net.collateral(0), before - pol.default_penalty, "exactly the fixed penalty");
    assert_eq!(w.net.owed(outsider), default_share(&pol, None), "the eligible demander takes the default share");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "its demand bond returns");
    let horizon = w.net.ledger().claims[&silent.id].liability_until.expect("the default's liability horizon");
    w.net.beat_to(horizon + 5).await;
    assert_eq!(w.net.collateral(0), before - pol.default_penalty, "permanent silence costs nothing past the penalty");
    assert_eq!(w.net.kernel_reserved(0), 0);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T02 (native shared-original part)**: the silent miner's default is classified by the CURRENT duty authority on the shared
/// original (F02 wire), against an actually supplied service window, never a caller's flag.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-f02-wire (shared-court op object / F02 wire), r5-clock-deps-relay (current-duty authority).
#[tokio::test]
#[ignore = "blocked: no production CourtSharedContext installer; tag 178 has no OpenDuty op (lanes merged on r7)"]
async fn t02_permanent_silence_on_the_shared_original_uses_current_duty_authority() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (silent, seats) = covered_claim(&mut w, 0, true).await;
        let outsider = w.outsiders(&silent, &seats, 1)[0];
        // INTENDED API (r5-f02-wire): the shared-court op opening a supplied duty against the original.
        let op = K::SharedCourtOp { claim: silent.id, actor: w.net.kid(outsider), op: misaka_palw_kernel::court::SharedOpV1::OpenDuty };
        let o = w.net.route(outsider, &op);
        w.net.send(vec![(outsider, o)]).await;
        let duty = w.net.chain.tip_state().1.palw_shared_duty_v1(&silent.id).expect("supplied duty");
        assert!(duty.supplied_opportunity_daa > 0, "an actual supplied opportunity, not a flag");
        w.net.beat_to(duty.deadline_daa).await;
        assert!(w.net.chain.tip_state().1.palw_shared_duty_v1(&silent.id).unwrap().defaulted_by_current_authority());
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!("tag 178 SharedCourtOpV1 carries Body/Ids/Request/Numerical/NativeHeldDefault (no OpenDuty) and needs a CourtSharedContext row that only #[cfg(test)] code installs; the post-Final default (NativeHeldDefault) needs a text29/funded11 held row and its slash PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1 = None (V6); the real reader court_duty_clocks_for_claim_v1 is empty for every kernel-route claim; intended palw_shared_duty_v1 does not exist");
}

/// **T03: forged signatures and fake Panel completion reports are rejected before any culpability or monetary effect.** (a) A proof
/// signed with another card's key under the accuser's bond; (b) a seat receipt with a corrupted ML-DSA signature (a fake "Panel
/// completed" report); (c) a receipt from a card that holds no seat of the claim. Each is refused at intake or dropped at the fold:
/// the claim row, every bond's collateral and slash, every reservation and the payout queue are byte-identical before and after. The
/// genuine path still works afterwards (real receipts cover; the real proof convicts).
#[tokio::test]
async fn t03_forged_signatures_and_fake_panel_completion_have_no_culpability_or_monetary_effect() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    let outsiders = w.outsiders(&lie, &seats, 2);
    let (forger, stranger) = (outsiders[0], outsiders[1]);
    let snapshot = |w: &World| {
        let n = cards(w);
        let money: Vec<(u64, u64, u128, u64)> =
            (0..n).map(|c| (w.net.collateral(c), w.net.slashed(c), w.net.kernel_reserved(c), w.net.owed(c))).collect();
        (w.net.ledger().claims[&lie.id].clone(), money)
    };
    let before = snapshot(&w);

    // (b) fake Panel completion: every seat's receipt, signature corrupted.
    let daa = w.net.daa();
    for (card, mut o) in colluding_receipts(&mut w.net, lie.id, daa) {
        let Obj::KernelConstraintReceiptV1 { signature, .. } = &mut o else { unreachable!("a receipt") };
        signature[7] ^= 0x40;
        offer_forged(&mut w.net, card, o).await;
    }
    assert!(
        !matches!(w.net.claim_state(&lie.id), ClaimStateV1::ProbabilisticPass { .. }),
        "forged receipts never complete the Panel: {:?}",
        w.net.claim_state(&lie.id)
    );
    // (c) a receipt for a seat the stranger does not hold, validly signed by the stranger.
    let mut real = colluding_receipts(&mut w.net, lie.id, daa);
    let (_, Obj::KernelConstraintReceiptV1 { receipt, .. }) = real.remove(0) else { unreachable!() };
    let o = w.net.receipt(stranger, *receipt);
    offer_forged(&mut w.net, stranger, o).await;
    assert_eq!(snapshot(&w), before, "no forged receipt moved a claim, a bond or a payout");

    // (a) a proof under the forger's bond signed with ANOTHER card's key (the stranger's).
    w.cover(&lie.id).await;
    let before = snapshot(&w);
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x03);
    let mut forged = w.net.route(stranger, &K::FileProof { accuser: w.net.kid(forger), claim: lie.id, proof: proof.clone() });
    let Obj::KernelRouteV1 { signer, .. } = &mut forged else { unreachable!() };
    *signer = w.net.bond(forger); // claims the forger's bond; the signature is the stranger's
    offer_forged(&mut w.net, forger, forged).await;
    assert!(!w.net.ledger().claims[&lie.id].convicted, "a forged accuser signature never convicts");
    assert_eq!(snapshot(&w), before, "and has no monetary effect on anyone");

    // The genuine path is intact.
    w.proof(forger, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(w.net.slashed(0), w.policy().claim_collateral);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
