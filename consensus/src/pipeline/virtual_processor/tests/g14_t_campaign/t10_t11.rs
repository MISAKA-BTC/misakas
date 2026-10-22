//! T10 internal numerical (fold-only) lie; T11 1..N concurrent pursuits, only the last false (scaled: `G14_T11_N`).
use super::*;

/// **T10 (route part): an internal numerical lie, invisible at the output, is rejected by the fold arithmetic.** The producer commits
/// to a trace whose one internal MatMul value is bumped while its delivered tokens are the honest greedy output — every output-level
/// check passes. The fresh verifier's finding is a KERNEL fault proof (the fold's numerical relation), not a decode or output fault,
/// and it convicts on the node: the fold arithmetic was not dropped to make the check cheaper.
#[tokio::test]
async fn t10_an_internal_fold_only_lie_is_convicted_by_the_fold_arithmetic() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let honest_tokens = greedy(&w.fx, &w.net.ledger(), &w.class, &job.prompt, job.max_new_tokens as usize);
    let lie = w.claim(0, &job, true).await;
    let ledger = w.net.ledger();
    let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim: committed, .. } = &ledger.claims[&lie.id].body else {
        panic!("a program claim")
    };
    let delivered = committed.generated.clone();
    assert_eq!(delivered, honest_tokens, "the output is honest; only an internal value lies");
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x10);
    assert!(matches!(proof, ProsecutionV1::Kernel(_)), "the finding is the fold's numerical relation: {proof:?}");
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the fold-only lie is convicted");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **T10 (full)**: fixture coverage does NOT stand in for every paid original — a funded original whose numerical families are not
/// covered by an admitted fixture/detection policy is refused at admission (typed refusal), never admitted on the strength of a
/// different fixture's coverage.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-original-admission (funded original admission + family coverage refusal).
#[tokio::test]
#[ignore = "blocked on value V2 (lanes merged on r7)"]
async fn t10_fixture_coverage_does_not_cover_every_paid_original() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let w = World::new().await;
        let refusal = w.net.chain.tip_state().1.palw_funded_original_admission_preview_v1(&w.class, /* uncovered family */ 0xFFFF);
        assert!(matches!(refusal, Err(e) if e.contains("not covered")), "typed refusal for an uncovered paid original");
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!("every node-level funded admission is refused at the schedule (palw_funded_original_schedule_v1 typed-missing, V2) before family coverage is consulted (see tests/funded_original_native_candidate.rs); intended palw_funded_original_admission_preview_v1 does not exist");
}

/// **T11 (scaled): N concurrent pursuits, only the LAST one false; every retained liability and the capacity accounted.** N claims
/// (`G14_T11_N`, default 3; the audit's full run is 10,000) by rotating producers are all live at once and all covered by colluding
/// seats; only the N-th lies. At peak, every producer's reservation equals its live claims × the claim collateral and the poster's
/// escrow equals N × the reward. The outsider convicts exactly the last one; the N − 1 honest claims finalize and release; the liar
/// is slashed once; replay agrees.
#[tokio::test]
async fn t11_n_concurrent_pursuits_only_the_last_false_with_full_retained_liabilities() {
    kaspa_core::log::try_init_logger("warn");
    let n = campaign_n("G14_T11_N", 3);
    let mut w = World::new().await;
    let pol = w.policy();
    let producers: Vec<usize> = (0..cards(&w)).filter(|c| *c != 1).collect();
    let mut claims = Vec::with_capacity(n);
    for i in 0..n {
        let producer = producers[i % producers.len()];
        let job = w.job().await;
        let claim = w.claim(producer, &job, i + 1 == n).await;
        claims.push(claim);
    }
    // Peak accounting: every live claim reserved on its producer's real bond.
    let mut expect: BTreeMap<usize, u128> = BTreeMap::new();
    for c in &claims {
        *expect.entry(c.producer).or_default() += u128::from(pol.claim_collateral);
    }
    for (card, reserved) in &expect {
        assert_eq!(w.net.kernel_reserved(*card), *reserved, "card {card}: every live claim retained");
    }
    assert_eq!(w.net.ledger().job_escrows.len(), n, "N escrows live at once");
    let escrow_total: u128 = w.net.ledger().job_escrows.values().map(|e| u128::from(e.amount)).sum();
    assert_eq!(escrow_total, n as u128 * u128::from(pol.claim_reward), "and fully funded");
    // The last claim's seats are read while its interim assignment exists (it is gone once the receipts cover it). Studio T4
    // (2026-10-22): the first run read them after `cover` and panicked in the harness ("the claim has an interim assignment").
    let last = claims.last().unwrap();
    let seats = w.seats(&last.id);
    for c in &claims {
        w.cover(&c.id).await;
    }
    let outsider = w.outsiders(last, &seats, 1)[0];
    // The outsider checks every claim; only the last is prosecutable.
    for c in &claims[..n - 1] {
        assert_eq!(w.fresh(0x11).check(c.id, &c.published(&w.fx, &[]), &w.fx.params), OutsiderFindingV1::Clean);
    }
    let proof = w.prosecution(&last.id, &last.published(&w.fx, &[]), 0x11);
    w.proof(outsider, &last.id, proof).await;
    let ledger = w.net.ledger();
    assert_eq!(ledger.claims.values().filter(|r| r.convicted).count(), 1, "exactly the last is convicted");
    assert!(ledger.claims[&last.id].convicted);
    let latest_end = claims[..n - 1]
        .iter()
        .filter_map(|c| match w.net.claim_state(&c.id) {
            ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => Some(window_end_daa),
            _ => None,
        })
        .max()
        .unwrap_or(w.net.daa());
    w.net.beat_to(latest_end).await;
    for c in &claims[..n - 1] {
        assert!(matches!(w.net.claim_state(&c.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&c.id));
    }
    assert_eq!(w.net.slashed(last.producer), pol.claim_collateral, "the liar is slashed once, for the last claim only");
    // Studio T4 (2026-10-22): the first full run failed here with 183,333,333 slashed from an honest producer — a card that ALSO sat
    // on the last claim's Panel and (the harness's colluding seats) signed passing receipts for the lie. That is its seat's G-K1
    // signer liability, not its honest claim: a producer outside the lie's Panel is never slashed, and a producer on it loses at
    // most its seat liability — never its honest claim's collateral (which is released, below).
    for c in &claims[..n - 1] {
        if c.producer == last.producer {
            continue;
        }
        if seats.contains(&c.producer) {
            let s = w.net.slashed(c.producer);
            assert!(s > 0 && s < pol.claim_collateral, "a colluding seat pays its seat liability only, never its honest claim: {s}");
        } else {
            assert_eq!(w.net.slashed(c.producer), 0, "an honest producer is never slashed");
        }
    }
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}
