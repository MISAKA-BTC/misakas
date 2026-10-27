//! T06 last inclusive notice and latest lawful response; T07 −1 / = / +1 at every connected deadline (node-level, observed fold DAA).
use super::*;

/// **T06 (route part): the last inclusive notice and the latest lawful response, with every remaining branch inside the hard bound.**
/// The demand folds in the LAST block whose DAA is still inside the challenge window (`daa < window_end`); the producer answers in
/// the LAST block before its deadline (`daa < deadline`); the proof the served values enable folds inside the proof grace and
/// convicts before Final; and the claim's Final bound `window_end + court_deadline + proof_grace` is never exceeded by any branch
/// (checked on an honest twin that takes the same latest path and must finalize within it).
#[tokio::test]
async fn t06_last_inclusive_notice_and_latest_lawful_response_fit_the_hard_bound() {
    kaspa_core::log::try_init_logger("warn");
    for lie in [true, false] {
        let mut w = World::new().await;
        let pol = w.policy();
        let (claim, seats) = covered_claim(&mut w, 0, lie).await;
        let outsider = w.outsiders(&claim, &seats, 1)[0];
        let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&claim.id) else { panic!() };
        let hard = window_end_daa + pol.court_deadline_daa + pol.proof_grace_daa;

        // The latest notice: step one block at a time until the next fold would leave the window, then file.
        let mut notice = None;
        for lead in (2..=6u64).rev() {
            if w.net.daa() + lead < window_end_daa {
                w.net.beat_to(window_end_daa - lead).await;
            }
            w.demand(outsider, &claim.id, claim.at.0).await;
            if let Some(row) = w.net.ledger().demands.get(&(claim.id, 0, claim.at.0)) {
                notice = Some(row.clone());
                break;
            }
        }
        let row = notice.expect("a demand inside the window");
        assert!(row.filed_daa < window_end_daa, "inclusive: the notice folded inside the window");
        assert!(row.filed_daa + 6 >= window_end_daa, "and in its last blocks ({} of {window_end_daa})", row.filed_daa);
        let deadline = row.deadline_daa;

        // The latest lawful response.
        w.net.beat_to(deadline.saturating_sub(3)).await;
        w.serve(0, &claim, claim.at.0).await;
        let served_at = w.net.ledger().daa;
        assert!(served_at < deadline, "the response folded before the deadline ({served_at} < {deadline})");
        assert!(!w.net.ledger().demands.contains_key(&(claim.id, 0, claim.at.0)), "and answered the demand");

        if lie {
            let proof = w.prosecution(&claim.id, &claim.published(&w.fx, &[]), 0x06);
            w.proof(outsider, &claim.id, proof).await;
            assert!(w.net.ledger().daa < served_at + pol.proof_grace_daa, "the proof folded inside the grace");
            assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Convicted { .. }), "convicted BEFORE Final");
            w.net.beat_to(hard + 2).await;
            let (minted, owed) = minted_and_owed(&w.net, &w.net.chain, 0);
            assert_eq!((minted, owed), (0, 0), "the liar never earns the reward");
        } else {
            w.net.beat_to(hard + 2).await;
            let ClaimStateV1::Final { final_daa } = w.net.claim_state(&claim.id) else { panic!("{:?}", w.net.claim_state(&claim.id)) };
            assert!(final_daa <= hard, "Final {final_daa} within the hard bound {hard} on the latest lawful path");
        }
        let z = w.net.replay().await;
        w.net.assert_same(&z, "replay");
    }
}

/// **T06 (full)**: every remaining LEGAL BRANCH of the shared original (appeal/escalation, relay, recovery, settlement) fits the
/// original's hard bound when notice is at its last inclusive DAA and every response is at its latest lawful DAA — computed by the
/// node from the original's signed dependency horizon, not by the test.
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-clock-deps-relay (signed dependency horizon / atomic deadline update), r5-original-admission.
#[tokio::test]
#[ignore = "blocked on values V2+V9 and unrecorded lease horizons (lanes merged on r7)"]
async fn t06_every_remaining_legal_branch_fits_the_original_hard_bound() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let mut w = World::new().await;
        let (claim, _) = covered_claim(&mut w, 0, true).await;
        let state = w.net.chain.tip_state().1;
        let original = state.palw_funded_original_v1(&claim.id).expect("admitted original");
        for branch in original.legal_branches() {
            let latest = branch.latest_lawful_end_daa(original.last_inclusive_notice_daa());
            assert!(latest <= original.hard_until_daa(), "branch {branch:?} ends at {latest} past the hard bound");
        }
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!(
        "legal branches are derived only by derive_funded_admission_v1 under FundedAdmissionScheduleV1, whose only constructor is #[cfg(test)] (V2); EXEC worker / provider lease horizons are not state fields (r5-clock-deps-relay notes 3 OPEN); the staged court retention on network_v1 is below a+10,256 (V9); intended palw_funded_original_v1().legal_branches() does not exist"
    );
}

/// **T07 (demand window, node-level): −1 / = / +1 at `window_end`.** One card files a demand for every position in ONE send: the i-th
/// carrier rides the i-th block and folds in the next, so the folds land on consecutive blocks whose DAA steps by at most one — the
/// fold DAA of every filing is READ from the chain, not assumed. Every filing folding at `window_end − 1` or earlier opens; every one
/// at `window_end` or later is refused (`demand_window_open`: `daa < window_end`). Coverage of −1, = and +1 is asserted (a late
/// filing after the window covers +1 if the burst did not reach it).
#[tokio::test]
async fn t07_demand_window_minus_one_equal_plus_one_at_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (claim, seats) = covered_claim(&mut w, 0, false).await;
    let demander = w.outsiders(&claim, &seats, 1)[0];
    let ClaimStateV1::ProbabilisticPass { window_end_daa: end, .. } = w.net.claim_state(&claim.id) else { panic!() };
    let positions = {
        let ledger = w.net.ledger();
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[&claim.id].body else { panic!() };
        evidence.positions
    };
    assert!(positions >= 4, "enough positions for a burst across the boundary");
    w.net.beat_to(end - 2).await;
    let skip = chain_len(&w.net);
    let mut items = Vec::new();
    for p in 0..positions {
        let o = w.net.route(demander, &K::FileDemand { demander: w.net.kid(demander), claim: claim.id, stage: 0, position: p });
        items.push((demander, o));
    }
    w.net.send(items).await;
    let daas = chain_daas_after(&w.net, skip);
    assert_eq!(daas.len(), positions as usize + 1, "one carrying block per filing plus the last folding block");
    let mut seen = std::collections::BTreeSet::new();
    let ledger = w.net.ledger();
    for p in 0..positions {
        let fold = daas[p as usize + 1];
        let opened = ledger.demands.contains_key(&(claim.id, 0, p));
        assert_eq!(opened, fold < end, "position {p} folded at DAA {fold} (window end {end}): opened = {opened}");
        seen.insert(fold as i64 - end as i64);
    }
    if !seen.contains(&1) {
        w.net.beat_to(end + 1).await;
        let before = w.net.ledger().demands.len();
        let o = w.net.route(seats[0], &K::FileDemand { demander: w.net.kid(seats[0]), claim: claim.id, stage: 0, position: 0 });
        w.net.send(vec![(seats[0], o)]).await;
        assert!(w.net.ledger().daa >= end + 1);
        assert_eq!(w.net.ledger().demands.len(), before, "+1: refused");
        seen.insert(1);
    }
    for off in [-1i64, 0, 1] {
        assert!(seen.contains(&off), "offset {off} at the window end was exercised: {seen:?}");
    }
}

/// **T07 (court deadline, node-level): a response folding at `deadline − 1` or AT `deadline` answers; at `deadline + 1` the demand is
/// already gone (the deadline block's tick defaulted it).** The kernel ledger folds a block's objects before its tick
/// (`Ledger::apply_block`: inputs, then `tick`, whose availability default is `daa >= deadline_daa`), so the deadline block is the
/// last lawful response block — the rule the kernel's own route test pins (`misaka/palw/kernel/tests/k2_ledger_route.rs`, "the
/// producer answers both at the deadline (79)"). Studio T4 (2026-10-22): the first run expected `fold < deadline` and failed at
/// `fold == deadline` (answered); that expectation contradicted the ledger's rule and is corrected here, and −1, = and +1 are now all
/// REQUIRED to be exercised (was: −1 and any ≥ 0).
/// Fresh worlds with the response sent at decreasing leads; each observed fold DAA decides the expected outcome.
#[tokio::test]
async fn t07_court_deadline_minus_one_equal_plus_one_at_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut seen = std::collections::BTreeMap::new();
    for lead in [3i64, 2, 1, 0, -1] {
        let mut w = World::new().await;
        let (claim, seats) = covered_claim(&mut w, 0, true).await;
        let demander = w.outsiders(&claim, &seats, 1)[0];
        w.demand(demander, &claim.id, claim.at.0).await;
        let deadline = w.net.ledger().demands[&(claim.id, 0, claim.at.0)].deadline_daa;
        w.net.beat_to((deadline as i64 - lead).max(0) as u64).await;
        let bytes = claim.position(&w.fx, claim.at.0, |_| {});
        let o = w.net.route(0, &K::Respond { claim: claim.id, stage: 0, position: claim.at.0, bytes });
        w.net.send(vec![(0, o)]).await;
        // The carrier folds in the block after its carrying block: the response's fold DAA is the one before the last block's.
        let daas = chain_daas_after(&w.net, chain_len(&w.net) - 2);
        let fold = daas[1];
        let defaulted = matches!(w.net.claim_state(&claim.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. });
        let answered = w.net.ledger().served.contains_key(&(claim.id, 0, claim.at.0));
        assert_eq!(answered, fold <= deadline, "fold {fold} vs deadline {deadline}: answered = {answered}");
        assert_eq!(defaulted, !answered, "exactly one of answer / default");
        seen.insert(fold as i64 - deadline as i64, answered);
    }
    assert!(seen.contains_key(&-1) && seen.contains_key(&0) && seen.contains_key(&1), "−1, = and +1 exercised: {seen:?}");
}

/// **T07 (liability horizon, node-level): a proof folding at `liability_until` still convicts; at `liability_until + 1` it is refused
/// before any court (no fee).** Fresh worlds, decreasing leads, outcome read against the observed fold DAA.
#[tokio::test]
async fn t07_liability_horizon_minus_one_equal_plus_one_at_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut seen = std::collections::BTreeSet::new();
    for lead in [3i64, 2, 1, 0, -1, -2] {
        let mut w = World::new().await;
        let (lie, seats) = covered_claim(&mut w, 0, true).await;
        let outsider = w.outsiders(&lie, &seats, 1)[0];
        let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&lie.id) else { panic!() };
        w.net.beat_to(window_end_daa).await;
        assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }));
        let until = w.net.ledger().claims[&lie.id].liability_until.expect("liability runs");
        w.net.beat_to((until as i64 - lead) as u64).await;
        let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x07);
        let accuser_before = w.net.collateral(outsider);
        w.proof(outsider, &lie.id, proof).await;
        let fold = w.net.ledger().daa;
        let convicted = w.net.ledger().claims[&lie.id].convicted;
        assert_eq!(convicted, fold <= until, "fold {fold} vs liability_until {until}: convicted = {convicted}");
        if !convicted {
            assert_eq!(w.net.collateral(outsider), accuser_before, "refused before any court: no fee");
        }
        seen.insert(fold as i64 - until as i64);
    }
    for off in [-1i64, 0, 1] {
        assert!(seen.contains(&off), "offset {off} at the liability horizon was exercised: {seen:?}");
    }
}

/// **T07 (full)**: −1 / = / +1 at EVERY connected native deadline and rule boundary — original admission accept_until, the dependency
/// lease end, the escrow settlement deadline, the position-terminal deadline and the original's hard bound — on the node, each the
/// observed fold DAA. Pure predicates are only partial evidence (the ledger row says so).
// MERGED on g14/integration-r7 (studio T4) — the remaining blocker is the #[ignore] reason; was: DEPENDS ON lane r5-clock-deps-relay, r5-escrow-settlement, r5-p2-terminal, r5-original-admission.
#[tokio::test]
#[ignore = "blocked on values V2+V3+V8 and unrecorded lease horizons (lanes merged on r7)"]
async fn t07_every_connected_native_deadline_minus_one_equal_plus_one() {
    #[cfg(feature = "g14-t-campaign-intended-api")]
    {
        let w = World::new().await;
        // INTENDED API: the node lists every connected deadline of an admitted original; each is driven like the three above.
        let deadlines = w.net.chain.tip_state().1.palw_connected_deadlines_v1();
        assert!(!deadlines.is_empty(), "the node enumerates the connected deadlines");
        for d in deadlines {
            for off in [-1i64, 0, 1] {
                let outcome = d.drive_at_offset_on_node(off);
                assert_eq!(outcome.accepted, d.accepts(outcome.fold_daa), "{d:?} at {off}");
            }
        }
    }
    #[cfg(not(feature = "g14-t-campaign-intended-api"))]
    blocked_on!(
        "funded-original accept_until, escrow settlement and position-terminal deadlines live only on rows no node can create (V2, V3, V8 typed-missing); the dependency lease end is not a state field; intended palw_connected_deadlines_v1 does not exist"
    );
}
