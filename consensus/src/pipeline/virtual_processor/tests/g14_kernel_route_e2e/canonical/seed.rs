//! **lane SEED — the Panel=1 seat draw and sample seed come from a seed anchor later than the commit** (2026-10-26; G14 independent
//! review MAJOR-1 / MINOR-1, `docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md`).
//!
//! Below `palw_kernel_route_panel_one_v1` the interim seats, challenge anchor and sample seed are `H(claim id)`: a producer re-forming
//! its claim id could pick a claim whose one seat is its own Sybil operator, or whose sample misses the lie. Past the fence nothing is
//! drawn at the commit; the seat, the challenge anchor and the sample seed are drawn from `H(claim ‖ round seed anchor)` at the first
//! block holding an anchor recorded strictly after the commit (ADR-0130 / ADR-0170, the same kind RFC-0018 Gate C draws from). No new
//! randomness source and no cryptographic proof of computation (PRINCIPLES §8): the anchor is randomness only.
//!
//! The slice is the spine's (the Panel-licensed kernel route, card 1's class, the single-prompt decode job), with the Panel=1 fence and
//! Gate C armed WITHOUT their validation, exactly as `cert` arms Gate C under its TEST-ONLY certification policy. The seat that the draw
//! picks is a harness card and colludes (it is the producer's "Sybil" here: it signs a passing receipt having done no work).

use super::cert::{TEST_ONLY_KERNEL_CERT_POLICY_V1, cert_config, cert_row, hold, until_decided, until_drawn};
use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::{
    palw_kernel_interim_challenge_v1, palw_kernel_panel_one_anchor_after_commit_v1, palw_kernel_panel_one_challenge_v1,
};
use kaspa_consensus_core::palw_security_certification_v1::{PalwCertificationOutcomeV1, palw_kernel_claim_security_certified_v1};

/// [`cert_config`] with `palw_kernel_route_panel_one_v1` armed at 0 (its mirror synced on the node's bundle and on the returned one).
fn seed_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = cert_config();
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(0));
    params.sync_palw_g14_gates_v1();
    let mut bundle = bundle;
    // The class relation needs the external gain bound; the 4-run test seam uses F03k2's TEST-ONLY bound (as `panel_one_config`).
    bundle.state = bundle
        .state
        .clone()
        .with_kernel_route_panel_one_from_daa(Some(0))
        .with_test_only_kernel_panel_one_external_gain_bound_v1(Some(super::f03k::TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI));
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &mut params.palw_consensus_mode {
        *node_bundle = bundle.clone();
    }
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &params.palw_consensus_mode {
        assert_eq!(node_bundle.state.kernel_route_panel_one_from_daa(), Some(0), "the node's fold reads the Panel=1 mirror");
    }
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
}

/// Attempt blocks (which record round seed anchors) until the claim's Panel=1 assignment exists. Returns the anchor the draw block's
/// state held and the DAA it appeared at.
async fn until_seated(net: &mut Net, claim: &Digest, commit_daa: u64) -> (kaspa_consensus_core::palw_execution_lane_v1::PalwExecSeedAnchorV1, u64) {
    let ttpb = net.ttpb();
    for round in 0..32u64 {
        let before = net.chain.tip_state().1.round_seed_anchor().copied();
        net.chain.attempt(5 + (round % 3) as usize, ttpb, Vec::new(), &|_| true).await;
        if let Some(a) = net.api().unwrap().assignment_of(claim) {
            // The closing tick drew from the anchor its state held: the one before this block's own attempt, or this block's.
            let after = *net.chain.tip_state().1.round_seed_anchor().expect("a drawn claim had an anchor");
            let used = [before, Some(after)]
                .into_iter()
                .flatten()
                .find(|x| palw_kernel_panel_one_challenge_v1(claim, x) == (a.challenge_anchor, a.sample_seed))
                .expect("the challenge pair is a held anchor's");
            assert!([1, 5].iter().any(|s| palw_kernel_panel_one_anchor_after_commit_v1(commit_daa, *s, Some(&used))), "(a) drawn from an anchor later than the commit");
            return (used, net.daa());
        }
    }
    panic!("no post-commit anchor seated the claim");
}

/// **(a) + (b)**: at the commit the claim has NO seat (so its id predicts nothing); the seat, challenge anchor and sample seed appear
/// only once a later anchor exists, and they are the anchor's — not the grindable claim-id-only values. The drawn seat (a colluding
/// harness card, the producer's Sybil) covers the lie: no reward, no Gate C certification, and ONE bonded verifier outside the Panel
/// convicts it from public material in the window — the producer's real bond is slashed.
#[tokio::test]
async fn lane_seed_panel_one_seat_is_drawn_after_the_commit_and_a_sybil_covered_lie_is_slashed_by_one_outsider() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on(Net::over_cfg(seed_config(), TestConsensus::new)).await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let commit_daa = w.net.daa();
    let route = w.net.api().unwrap();
    assert!(route.assignment_of(&lie.id).is_none(), "(a) Panel=1: no seat at the commit — the claim id alone draws nothing");

    let (anchor, drawn_daa) = until_seated(&mut w.net, &lie.id, commit_daa).await;
    assert!(drawn_daa > commit_daa, "(a) the seat is decided strictly after the commit");
    let assignment = w.net.api().unwrap().assignment_of(&lie.id).expect("seated");
    assert_eq!((assignment.seats.len(), assignment.quorum), (1, 1), "Panel=1");
    let (interim_anchor, interim_sample) = palw_kernel_interim_challenge_v1(&lie.id);
    assert_ne!(assignment.challenge_anchor, interim_anchor, "(a) not the claim-id-only challenge anchor");
    assert_ne!(assignment.sample_seed, interim_sample, "(a) not the claim-id-only sample seed");
    assert_eq!(
        (assignment.challenge_anchor, assignment.sample_seed),
        palw_kernel_panel_one_challenge_v1(&lie.id, &anchor),
        "(a) the challenge pair is the post-commit anchor's"
    );
    let seats = w.seats(&lie.id);
    assert!(!seats.contains(&0), "never the producer");
    assert_ne!(
        w.net.chain.tip_state().1.bond(&w.net.bond(seats[0])).unwrap().operator_id,
        w.net.chain.tip_state().1.bond(&w.net.bond(0)).unwrap().operator_id,
        "never the producer's operator"
    );

    // (b) the Sybil seat covers the lie.
    w.cover(&lie.id).await;
    assert_eq!(w.net.owed(0), 0, "(b) a covered lie pays nothing");
    assert!(cert_row(&w.net, &lie.id).is_none(), "(b) no Gate C certification exists before Final, so no credit");
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x5E);
    w.proof(outsider, &lie.id, proof).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "(b) one outsider convicts the Sybil-covered lie");
    let slashed = w.policy().claim_collateral;
    assert_eq!(w.net.collateral(0), before - slashed, "(b) the producer's real bond is slashed");
    assert_eq!(w.net.owed(0), 0, "(b) and was never paid");
    let state = w.net.chain.tip_state().1;
    assert!(!palw_kernel_claim_security_certified_v1(&state, &lie.id), "(b) never SecurityCertified");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the deferred Panel=1 draw");
}

/// **(c) + (d)**: an UNPROSECUTED Sybil-covered lie reaches the kernel `Final`. (d) That `Final` exports no beacon event (no RFC07
/// source), pays the producer nothing (the escrow reward is held for Gate C) and is not SecurityCertified. (c) Gate C's drawn
/// verifiers do not approve: NotCertified, the held reward returns whole to the poster — and the lying producer is NOT slashed by
/// that outcome (only a conviction slashes). Recorded as the residual, not hidden.
#[tokio::test]
async fn lane_seed_an_unprosecuted_sybil_covered_final_earns_nothing_and_gate_c_failure_alone_does_not_slash() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on(Net::over_cfg(seed_config(), TestConsensus::new)).await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let commit_daa = w.net.daa();
    until_seated(&mut w.net, &lie.id, commit_daa).await;
    w.cover(&lie.id).await;
    let floor = final_floor(&w, Mode::Panel, &lie.id);
    let policy = w.policy();
    w.net.beat_to(floor).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&lie.id));

    // (d) the pre-Gate-C Final contributes nothing.
    assert_eq!(w.net.owed(0), 0, "(d) a Final before Gate C pays the producer nothing");
    assert!(hold(&w.net, &lie.id).is_some(), "(d) its reward is held for certification");
    let finals = w.net.api().unwrap().finals_read_v1().expect("op 212 reads");
    let read = finals.iter().find(|f| f.receipt.claim == lie.id).expect("the Final is read");
    assert!(read.event.is_none(), "(d) no beacon-source event: a Panel-licensed Final is never an RFC07 source");
    assert!(!palw_kernel_claim_security_certified_v1(&w.net.chain.tip_state().1, &lie.id), "(d) not SecurityCertified");

    // (c) nobody drawn approves.
    let row = until_drawn(&mut w.net, &lie.id).await;
    assert!(row.outcome.is_none());
    let (producer_before, poster_before) = until_decided(
        &mut w.net,
        &lie.id,
        TEST_ONLY_KERNEL_CERT_POLICY_V1.attest_window_daa + TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 50,
    )
    .await;
    let decided = cert_row(&w.net, &lie.id).unwrap();
    assert_eq!(decided.outcome.map(|(o, _)| o), Some(PalwCertificationOutcomeV1::NotCertified), "(c) {:?}", decided.outcome);
    assert_eq!(w.net.owed(0), producer_before, "(c) never paid");
    assert_eq!(w.net.owed(1), poster_before + Rules::final_reward(&policy), "(c) the held reward returns to the poster");
    assert!(!w.net.ledger().claims[&lie.id].convicted, "(c) NotCertified convicts nothing");
    assert_eq!(
        w.net.slashed(0),
        0,
        "(c) RESIDUAL: Gate C failure alone does not slash the lying producer (its loss is the admission fee and the unearned reward)"
    );
    assert!(w.net.collateral(0) <= before, "(c) only the admission fee left the bond");
}
