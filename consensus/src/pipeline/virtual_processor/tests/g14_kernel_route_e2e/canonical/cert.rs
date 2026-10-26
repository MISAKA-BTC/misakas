//! **lane CERT — spine stage S3a': the per-claim lottery approval (`SecurityCertified`, RFC-0018 Gate C) on the kernel route**
//! (2026-10-26). The slice is the spine's: the Panel-licensed kernel route with every seat colluding, card 1's class, the single-prompt
//! decode job. The kernel ledger reaches `Final` from the Panel pass and the window alone; past Gate C that `Final` pays nothing: the
//! escrow reward is held (aux 136 `'H'`) and the claim's `'K'` certification row is committed. The global bonded pool is drawn from a
//! seed anchor later than the commit; only `k` positive attestations of drawn bonds, with the prosecution window elapsed and nothing
//! pending, certify — then the producer is paid once. Any other outcome returns the reward to the poster. No cryptographic proof of
//! computation is used (PRINCIPLES §8): the drawn verifiers' signed approval stands under the explicit honest-fraction premise.
//!
//! **TEST-ONLY certification policy.** `m`/`k` and the windows below are NOT approved values: the design values await lane VAL's
//! economic audit and the user's approval, and `validate_palw_rfc0018_v1` refuses Gate C armed at any real height (its arming record
//! list is empty) — the harness bypasses validation exactly as `canonical_config` does. Small numbers keep the slice fast.

use super::*;
use kaspa_consensus_core::palw_rfc0018_v1::PalwRfc0018MirrorV1;
use kaspa_consensus_core::palw_security_certification_v1::*;

/// **TEST ONLY — not an approved policy.** m = 5 positions, k = 3 (a strict majority), no veto, one anchor span of delay, short
/// windows, every Active genesis bond in the pool at once.
pub(super) const TEST_ONLY_KERNEL_CERT_POLICY_V1: PalwSecurityCertificationPolicyV1 = PalwSecurityCertificationPolicyV1 {
    m: 5,
    k: 3,
    veto: 0,
    anchor_delay_spans: 1,
    attest_window_daa: 600,
    prosecution_window_daa: 40,
    certify_by_daa: 4_000,
    verifier_maturity_daa: 0,
    verifier_min_collateral_sompi: 1,
    draws_per_epoch: 64,
    epoch_daa: 1_000_000,
};

/// The canonical Panel network with Gate C (`palw_independently_work_certified_v1`) armed at 0 and the TEST-ONLY policy mirrored.
pub(super) fn cert_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = canonical_config(false);
    let mut params = config.params.clone();
    params.palw_independently_work_certified_v1 = Some(ForkActivation::new(0));
    let mut tmp = params.clone();
    tmp.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    tmp.sync_palw_rfc0018_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(mut bundle) = tmp.palw_consensus_mode else { unreachable!() };
    let mirror = PalwRfc0018MirrorV1 { certification: TEST_ONLY_KERNEL_CERT_POLICY_V1, ..*bundle.state.rfc0018_v1().expect("Gate C mirrored") };
    assert!(mirror.gate_c_at(0), "Gate C governs from DAA 0 in this harness");
    TEST_ONLY_KERNEL_CERT_POLICY_V1.validate().expect("the test policy is well-formed");
    bundle.state = bundle.state.clone().with_rfc0018_v1(Some(mirror));
    // The node reads the bundle on its own params: the same mirror there.
    params.sync_palw_rfc0018_v1();
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &mut params.palw_consensus_mode {
        node_bundle.state = node_bundle.state.clone().with_rfc0018_v1(Some(mirror));
    }
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
}

pub(super) fn cert_row(net: &Net, claim: &Digest) -> Option<PalwClaimCertificationRowV1> {
    let key = certification_kernel_claim_key_v1(&palw_kernel_claim_certification_id_v1(claim));
    net.api()?.aux.get(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)).map(|raw| borsh::from_slice(raw).unwrap())
}

pub(super) fn hold(net: &Net, claim: &Digest) -> Option<PalwKernelCertifiedRewardHoldV1> {
    let key = certification_kernel_hold_key_v1(&palw_kernel_claim_certification_id_v1(claim));
    net.api()?.aux.get(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)).map(|raw| borsh::from_slice(raw).unwrap())
}

/// The honest slice up to the kernel `Final`: returns the world and the claim. Asserts the NEGATIVE half of S3a': `Final` without
/// SecurityCertified pays the producer nothing, the reward sits in the hold, the certification is committed and undecided.
async fn honest_final_uncertified() -> (World, Digest) {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on(Net::over_cfg(cert_config(), TestConsensus::new)).await;
    let (claim, seats) = claimed(&mut w, Mode::Panel, 0, false).await;
    assert_eq!(seats.len(), 3, "the colluding Panel covered it");
    let floor = final_floor(&w, Mode::Panel, &claim.id);
    let policy = w.policy();
    let owed_poster = w.net.owed(1);
    w.net.beat_to(floor).await;
    assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
    assert_eq!(w.net.owed(0), 0, "S3a' negative: a kernel Final without SecurityCertified pays the producer nothing");
    assert_eq!(w.net.owed(1), owed_poster, "and returns nothing yet");
    let h = hold(&w.net, &claim.id).expect("the Final reward is held for certification");
    assert_eq!((h.version, h.amount), (PALW_KERNEL_CERTIFIED_REWARD_HOLD_VERSION_V1, Rules::final_reward(&policy)));
    let row = cert_row(&w.net, &claim.id).expect("the certification is committed at Final");
    assert!(row.outcome.is_none() && row.anchor.is_none(), "undecided and not drawn at the commit: {:?}", row.outcome);
    assert_eq!(row.producer_bond, w.net.kid(0));
    (w, claim.id)
}

/// Seed anchors from attempt blocks until the certification is drawn (a seed that did not exist at the commit).
pub(super) async fn until_drawn(net: &mut Net, claim: &Digest) -> PalwClaimCertificationRowV1 {
    let ttpb = net.ttpb();
    for round in 0..64u64 {
        if cert_row(net, claim).is_some_and(|r| r.anchor.is_some()) {
            break;
        }
        net.chain.attempt(5 + (round % 3) as usize, ttpb, Vec::new(), &|_| true).await;
        let to = net.daa() + 8;
        net.beat_to(to).await;
    }
    let row = cert_row(net, claim).expect("the certification row");
    assert!(row.anchor.is_some(), "drawn from a later seed anchor: {:?}", row.outcome);
    assert_eq!(row.draws.len(), TEST_ONLY_KERNEL_CERT_POLICY_V1.m as usize, "m positions drawn from the global pool");
    assert!(!row.draws.contains(&net.kid(0)), "never the producer's operator");
    row
}

/// One block at a time until the certification decides (at most `limit` DAA); returns what the producer's and the poster's
/// payees were owed in the coinbase queue just before the deciding block (the queue drains into later coinbases, so the
/// settlement is read at the deciding block itself).
pub(super) async fn until_decided(net: &mut Net, claim: &Digest, limit: u64) -> (u64, u64) {
    let end = net.daa() + limit;
    loop {
        assert!(net.daa() <= end, "the certification did not decide by DAA {end}");
        let before = (net.owed(0), net.owed(1));
        let next = net.daa() + 1;
        net.beat_to(next).await;
        if cert_row(net, claim).is_some_and(|r| r.outcome.is_some()) {
            return before;
        }
    }
}

/// The cards whose bonds hold at least one drawn position, with their position counts.
fn drawn_cards(net: &Net, row: &PalwClaimCertificationRowV1) -> Vec<(usize, u32)> {
    (0..8usize)
        .filter_map(|card| {
            let n = row.draws.iter().filter(|d| **d == net.kid(card)).count() as u32;
            (n > 0).then_some((card, n))
        })
        .collect()
}

async fn attest(net: &mut Net, claim: &Digest, card: usize, verdict: misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1) {
    use misaka_palw_kernel::model_eligibility_v1::*;
    let attestation = EligibilityAttestationV1 {
        version: ELIGIBILITY_ATTESTATION_VERSION_V1,
        network: net.domain.as_bytes(),
        exam: palw_kernel_claim_certification_id_v1(claim),
        verifier: net.kid(card),
        verdict,
        transcript: if verdict == EligibilityVerdictV1::Reproduced { [0xC0 | card as u8; 64] } else { [0u8; 64] },
        claim_mismatches: vec![],
    };
    let o = net.route(card, &K::AttestEligibilityExamV1 { attestation });
    net.send(vec![(card, o)]).await;
}

/// **S3a' positive, then reorg and IBD**: every drawn bond attests `Reproduced`; at `k` positives the decision still waits for the
/// prosecution window; then SecurityCertified and the producer is paid exactly the held reward, once. A node reorged onto a heavier
/// branch forked before the attestations reaches no payment on that branch (the approval is not carried by the reorg), and a node
/// syncing the certified chain by IBD agrees on every row.
#[tokio::test]
async fn g14_cert_kernel_route_final_pays_only_after_the_drawn_verifiers_certify_and_a_reorg_cannot_carry_the_payment() {
    s3a_prime_certified_reorg_ibd().await;
}

pub(super) async fn s3a_prime_certified_reorg_ibd() {
    let (mut w, claim) = honest_final_uncertified().await;
    let policy = w.policy();
    let row = until_drawn(&mut w.net, &claim).await;
    let cards = drawn_cards(&w.net, &row);
    let k = TEST_ONLY_KERNEL_CERT_POLICY_V1.k as u32;
    use misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1::Reproduced;
    // Approvals one drawn bond at a time; the attestation that crosses k is the one a reorg removes.
    let mut positive = 0u32;
    let mut rest = cards.iter().copied();
    let crossing = loop {
        let (card, positions) = rest.next().expect("the drawn positions reach k");
        if positive + positions >= k {
            break card;
        }
        attest(&mut w.net, &claim, card, Reproduced).await;
        positive += positions;
    };
    let fork = w.net.chain.sink();
    let funding_at_fork = w.net.funding.clone();
    attest(&mut w.net, &claim, crossing, Reproduced).await;
    assert!(cert_row(&w.net, &claim).unwrap().positive_positions() >= k, "k positives on A");

    // ---- reorg (inside the shipped shallow-tie window): a heavier branch B forked just before the crossing approval ----
    let z = super::super::recovery::reorged_node(&w.net, fork, funding_at_fork).await;
    let on_b = cert_row(&z, &claim).expect("the commit is below the fork");
    assert!(on_b.positive_positions() < k, "B lacks the crossing approval: {}", on_b.positive_positions());
    assert!(on_b.outcome.is_none(), "B is undecided: {:?}", on_b.outcome);
    assert_eq!(z.owed(0), 0, "on B the kernel Final pays nothing: a reorg past an approval cannot carry a payment");
    assert!(hold(&z, &claim).is_some(), "B still holds the reward");
    drop(z);

    for (card, _) in rest {
        attest(&mut w.net, &claim, card, Reproduced).await;
    }
    let attested = cert_row(&w.net, &claim).unwrap();
    assert_eq!(attested.positive_positions(), TEST_ONLY_KERNEL_CERT_POLICY_V1.m as u32, "every drawn position approved");
    assert_eq!(w.net.owed(0), 0, "k positives alone do not pay: the decision waits for the prosecution window");
    let (producer_before, _) = until_decided(&mut w.net, &claim, TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 50).await;
    let decided = cert_row(&w.net, &claim).unwrap();
    assert_eq!(decided.outcome.map(|(o, _)| o), Some(PalwCertificationOutcomeV1::Certified), "SecurityCertified");
    assert!(palw_kernel_claim_security_certified_v1(&w.net.chain.tip_state().1, &claim));
    assert!(decided.outcome.is_some_and(|(_, at)| at >= row.final_daa + TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa));
    assert_eq!(
        w.net.owed(0),
        producer_before + Rules::final_reward(&policy),
        "S3a' positive: certified, the producer is paid the held reward"
    );
    assert!(hold(&w.net, &claim).is_none(), "the hold is settled once: it can never pay again");

    // ---- IBD: a cold node agrees on the certified chain ----
    let node = ibd_node(&w.net).await;
    agree(&w.net, &node, "the IBD node of the certified kernel claim");
    assert!(palw_kernel_claim_security_certified_v1(&node.chain.tip_state().1, &claim), "the IBD node reads it certified");
}

/// **S3a' approval shortfall and unavailability**: fewer than `k` drawn positions approve (one drawn bond reports the model
/// `Unavailable`, the rest stay silent); past the attestation deadline the certification is `NotCertified`, the producer is never
/// paid, and the held reward returns whole to the poster. A Final the Panel's every seat covered does not substitute for the draw.
#[tokio::test]
async fn g14_cert_kernel_route_an_approval_shortfall_or_unavailable_verifiers_never_pay_the_final() {
    s3a_prime_shortfall().await;
}

pub(super) async fn s3a_prime_shortfall() {
    let (mut w, claim) = honest_final_uncertified().await;
    let policy = w.policy();
    let row = until_drawn(&mut w.net, &claim).await;
    let cards = drawn_cards(&w.net, &row);
    let k = TEST_ONLY_KERNEL_CERT_POLICY_V1.k as u32;
    let mut positive = 0u32;
    let mut silent_or_negative = 0usize;
    for (i, (card, positions)) in cards.iter().enumerate() {
        if i == 0 {
            attest(&mut w.net, &claim, *card, misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1::Unavailable).await;
            silent_or_negative += 1;
        } else if positive + positions < k {
            attest(&mut w.net, &claim, *card, misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1::Reproduced).await;
            positive += positions;
        } else {
            silent_or_negative += 1;
        }
    }
    assert!(silent_or_negative >= 1);
    let short = cert_row(&w.net, &claim).unwrap();
    assert_eq!(short.positive_positions(), positive);
    assert!(short.positive_positions() < k, "short of k = {k}: {}", short.positive_positions());
    let (producer_before, poster_before) =
        until_decided(&mut w.net, &claim, TEST_ONLY_KERNEL_CERT_POLICY_V1.attest_window_daa + TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 50).await;
    let decided = cert_row(&w.net, &claim).unwrap();
    assert_eq!(decided.outcome.map(|(o, _)| o), Some(PalwCertificationOutcomeV1::NotCertified), "{:?}", decided.outcome);
    assert!(matches!(w.net.claim_state(&claim), ClaimStateV1::Final { .. }), "still a kernel Final");
    assert!(decided.outcome.is_some_and(|(_, at)| at > short.deadline_daa), "decided only past the attestation deadline");
    assert_eq!(w.net.owed(0), producer_before, "an uncertified Final never pays the producer");
    let (burn, refund) = palw_kernel_uncertified_split_v1(Rules::final_reward(&policy));
    assert_eq!(burn + refund, Rules::final_reward(&policy), "conservation: burn + refund = the held reward");
    assert!(burn > 0, "the V3 burn share is taken");
    assert_eq!(w.net.owed(1), poster_before + refund, "the poster gets the reward less the burn (PROVISIONAL split)");
    assert!(hold(&w.net, &claim).is_none(), "settled once");
    // Exactly once: later blocks never name the refund (or any producer payment) again.
    for _ in 0..5 {
        let (p0, p1) = (w.net.owed(0), w.net.owed(1));
        let next = w.net.daa() + 1;
        w.net.beat_to(next).await;
        assert!(w.net.owed(0) <= p0 && w.net.owed(1) <= p1, "no second settlement of the held reward");
    }
}
