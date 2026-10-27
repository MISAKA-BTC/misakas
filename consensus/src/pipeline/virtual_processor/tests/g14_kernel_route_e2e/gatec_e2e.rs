//! **lane GC-E2E — Gate C (`SecurityCertified`, RFC-0018) on a CHAIN claim, end to end on the node** (2026-10-27).
//!
//! The subject is a LegacyPanelAttempt floor claim (`'C'` row of aux 136), not the kernel route (lane CERT covers `'K'`, lane KDIST
//! the genesis Panel=1 path). The claim is made by card 0's attempt block, bound to the panel the chain derives, licensed by a signed
//! seat quorum carried in a `0x4b` carrier through the mempool and the node's own template, and reaches `Final` past the challenge
//! window. Past Gate C its `Final` commits the `'C'` certification row; a later seed anchor draws the global bonded pool; the drawn
//! bonds sign `AttestEligibilityExamV1` (inner 75) through the same carrier path. No cryptographic proof of computation is made or
//! used (PRINCIPLES §8): the drawn verifiers' signed approval stands under the explicit honest-fraction premise.
//!
//! **TEST-ONLY certification policy** (`TEST_ONLY_KERNEL_CERT_POLICY_V1`, m = 5 / k = 3): NOT the approved 75/41 — the harness has 8
//! cards and the design windows would make the run days long. The approved m/k/f are covered by the unit tests in
//! `palw_security_certification_v1`. Gate C is armed WITHOUT its validation (the harness bypass every G14 world uses).
use super::*;
use kaspa_consensus_core::palw_rfc0018_v1::{PalwGateCViewV1, PalwRfc0018MirrorV1, palw_independently_work_certified_v1};
use kaspa_consensus_core::palw_security_certification_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1;

/// **TEST ONLY — not an approved policy** (the same numbers as lane CERT's `TEST_ONLY_KERNEL_CERT_POLICY_V1`).
const TEST_ONLY_KERNEL_CERT_POLICY_V1: PalwSecurityCertificationPolicyV1 = PalwSecurityCertificationPolicyV1 {
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

/// The kernel world (aux 136 lives in the route) + the frozen model tier + Gate C at 0, the TEST-ONLY policy mirrored.
fn gatec_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config();
    let mut params = config.params.clone();
    params.palw_independently_work_certified_v1 = Some(ForkActivation::new(0));
    if params.palw_model_tier_v1.is_none() {
        params.palw_model_tier_v1 = Some(kaspa_consensus_core::palw_model_tier_v1::PalwModelTierFenceV1::at(ForkActivation::new(0)));
    }
    let mut tmp = params.clone();
    tmp.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    tmp.sync_palw_rfc0018_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(mut bundle) = tmp.palw_consensus_mode else {
        unreachable!()
    };
    let mirror =
        PalwRfc0018MirrorV1 { certification: TEST_ONLY_KERNEL_CERT_POLICY_V1, ..*bundle.state.rfc0018_v1().expect("Gate C mirrored") };
    assert!(mirror.gate_c_at(0));
    bundle.state = bundle.state.clone().with_rfc0018_v1(Some(mirror));
    params.sync_palw_rfc0018_v1();
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &mut params.palw_consensus_mode {
        node_bundle.state = node_bundle.state.clone().with_rfc0018_v1(Some(mirror));
    }
    (Config::new(params), bundle, premine, floats)
}

fn c_row(net: &Net, claim: &Hash64) -> Option<PalwClaimCertificationRowV1> {
    let key = certification_claim_key_v1(&palw_certification_id_v1(claim));
    net.api()?.aux.get(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)).map(|raw| borsh::from_slice(raw).unwrap())
}

fn certified(net: &Net, claim: &Hash64) -> bool {
    let (_, state) = net.chain.tip_state();
    let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
    let c = state.claim(claim).expect("the claim").clone();
    palw_independently_work_certified_v1(claim, &c, &PalwGateCViewV1::of(&params, &state))
}

/// Card 0's floor attempt → panel bound → a Valid seat quorum carried through the mempool → `Final`. Returns the claim.
async fn chain_claim_final(net: &mut Net) -> Hash64 {
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    let ttpb = net.ttpb();
    net.chain.heartbeat(ttpb, Vec::new()).await;
    let (_, claim_id) = net.chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    net.chain.attempt_at_the_anchor_slot(claim_id, 7).await;
    let (_, state) = net.chain.tip_state();
    assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "bound");
    let panel = state.panel(&claim_id).expect("a panel").clone();
    let signed_daa = net.chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .take(net.bundle.panel.quorum() as usize)
        .map(|seat| {
            let card = net.chain.bonds.iter().position(|b| *b == seat.bond).expect("a genesis card");
            let message = palw_receipt_message_v2(net.domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x11u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = net.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
    net.send(vec![(1, object)]).await;
    for _ in 0..2_000 {
        if net.chain.tip_state().1.claim(&claim_id).is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. })) {
            return claim_id;
        }
        let to = net.daa() + 1;
        net.beat_to(to).await;
    }
    panic!("the chain claim never reached Final: {:?}", net.chain.tip_state().1.claim(&claim_id).map(|c| c.phase.clone()));
}

async fn until_drawn(net: &mut Net, claim: &Hash64) -> PalwClaimCertificationRowV1 {
    let ttpb = net.ttpb();
    for round in 0..64u64 {
        if c_row(net, claim).is_some_and(|r| r.anchor.is_some() || r.outcome.is_some()) {
            break;
        }
        net.chain.attempt(5 + (round % 3) as usize, ttpb, Vec::new(), &|_| true).await;
        let to = net.daa() + 8;
        net.beat_to(to).await;
    }
    let row = c_row(net, claim).expect("the 'C' row");
    assert!(row.anchor.is_some(), "drawn from a later seed anchor: {:?}", row.outcome);
    assert_eq!(row.draws.len(), TEST_ONLY_KERNEL_CERT_POLICY_V1.m as usize, "m positions");
    assert!(!row.draws.contains(&net.kid(0)), "never the producer");
    row
}

fn drawn_cards(net: &Net, row: &PalwClaimCertificationRowV1) -> Vec<usize> {
    (0..8usize).filter(|card| row.draws.contains(&net.kid(*card))).collect()
}

async fn attest(net: &mut Net, claim: &Hash64, card: usize, verdict: EligibilityVerdictV1) {
    use misaka_palw_kernel::model_eligibility_v1::*;
    let attestation = EligibilityAttestationV1 {
        version: ELIGIBILITY_ATTESTATION_VERSION_V1,
        network: net.domain.as_bytes(),
        exam: palw_certification_id_v1(claim),
        verifier: net.kid(card),
        verdict,
        transcript: if verdict == EligibilityVerdictV1::Reproduced { [0xC0 | card as u8; 64] } else { [0u8; 64] },
        claim_mismatches: vec![],
    };
    let o = net.route(card, &K::AttestEligibilityExamV1 { attestation });
    net.send(vec![(card, o)]).await;
}

async fn until_decided(net: &mut Net, claim: &Hash64, limit: u64) -> PalwCertificationOutcomeV1 {
    let end = net.daa() + limit;
    while net.daa() <= end {
        if let Some((o, _)) = c_row(net, claim).and_then(|r| r.outcome) {
            return o;
        }
        let to = net.daa() + 1;
        net.beat_to(to).await;
    }
    panic!("undecided by DAA {end}: {:?}", c_row(net, claim));
}

/// **(a) honest chain claim**: `Final` alone is not SecurityCertified; the `'C'` row is committed at `Final`, drawn from a later seed
/// anchor, every drawn bond attests `Reproduced` through the mempool, and after the prosecution window the claim is certified (the
/// predicate fork choice and settlement read). A second node replaying the chain agrees on every root.
#[tokio::test]
async fn gatec_e2e_chain_claim_is_certified_only_by_the_drawn_verifiers() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(gatec_config(), TestConsensus::new);
    let claim = chain_claim_final(&mut net).await;
    let row = c_row(&net, &claim).expect("Final commits the 'C' certification row");
    assert!(row.outcome.is_none() && row.anchor.is_none() && row.attempt, "committed undecided: {row:?}");
    assert!(!certified(&net, &claim), "Final is not SecurityCertified");
    let row = until_drawn(&mut net, &claim).await;
    let cards = drawn_cards(&net, &row);
    for card in &cards {
        attest(&mut net, &claim, *card, EligibilityVerdictV1::Reproduced).await;
    }
    let attested = c_row(&net, &claim).unwrap();
    assert_eq!(attested.positive_positions(), TEST_ONLY_KERNEL_CERT_POLICY_V1.m as u32, "every drawn position approved");
    let outcome = until_decided(&mut net, &claim, TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 200).await;
    assert_eq!(outcome, PalwCertificationOutcomeV1::Certified);
    assert!(certified(&net, &claim), "SecurityCertified: the claim now counts for fork choice and settlement");
    let replay = net.replay().await;
    net.assert_same(&replay, "Gate C chain-claim certification");
}

/// **(c) shortfall**: one drawn bond reports `Unavailable`, the rest withhold. Past the attestation deadline the row is
/// `NotCertified`, the claim never counts for fork choice or settlement, and Gate C failing alone slashes nobody.
#[tokio::test]
async fn gatec_e2e_chain_claim_shortfall_is_not_certified_and_slashes_nobody() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(gatec_config(), TestConsensus::new);
    let claim = chain_claim_final(&mut net).await;
    let row = until_drawn(&mut net, &claim).await;
    let cards = drawn_cards(&net, &row);
    let before: Vec<(u64, u64)> = (0..8).map(|c| (net.collateral(c), net.slashed(c))).collect();
    attest(&mut net, &claim, cards[0], EligibilityVerdictV1::Unavailable).await;
    let limit = row.deadline_daa.saturating_sub(net.daa()) + TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 200;
    let outcome = until_decided(&mut net, &claim, limit).await;
    assert_eq!(outcome, PalwCertificationOutcomeV1::NotCertified);
    assert!(!certified(&net, &claim), "an uncertified Final carries no fork-choice or settlement credit");
    let after: Vec<(u64, u64)> = (0..8).map(|c| (net.collateral(c), net.slashed(c))).collect();
    assert_eq!(before, after, "Gate C failing alone slashes no bond (producer nor verifiers)");
    let replay = net.replay().await;
    net.assert_same(&replay, "Gate C chain-claim shortfall");
}

/// **lane GC-DUTY: the drawn verifiers find their own duty and the honest claim certifies with no hand-picked attestor.** Each of
/// the 8 cards asks the node's read (`palw_certification_duties_v1`, what kaspad's `palw_cert_duty` polls every DAA) for the rows it
/// owes and attests each; nobody else attests. The replay itself is kaspad's (crate boundary): here an owed row whose job
/// descriptor matches the claim record's roots stands in for a reproduced replay. Every drawn position approves, the row certifies
/// after the prosecution window, a second node agrees, and nothing is owed afterwards.
#[tokio::test]
async fn gatec_duty_e2e_drawn_bonds_find_their_duty_and_the_honest_claim_certifies() {
    use misaka_palw_kernel::model_eligibility_v1::*;
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(gatec_config(), TestConsensus::new);
    let claim = chain_claim_final(&mut net).await;
    let row = until_drawn(&mut net, &claim).await;
    let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
    let daa = net.daa();
    let (_, state) = net.chain.tip_state();
    let mut attested = 0;
    for card in 0..8usize {
        let bond = net.chain.bonds[card];
        let owed = palw_certification_duties_v1(&state, &params, &bond, daa);
        assert_eq!(owed.is_empty(), !row.draws.contains(&net.kid(card)), "card {card}: owes exactly when drawn");
        for d in owed {
            let c = state.claim(&d.claim_id).expect("the claim");
            let job = d.job.as_ref().expect("a chain claim carries lane B's job");
            assert!(!d.native && d.seat_duty.is_some() && !job.free_prompt, "an Attempt row routes to the attempt seam");
            let reproduced = job.execution_root == c.execution_root && job.trace_root == c.trace_root;
            let mut seen = Vec::new();
            seen.extend_from_slice(&d.certification_id);
            seen.extend_from_slice(&net.kid(card));
            seen.extend_from_slice(&job.execution_root.as_bytes());
            let attestation = EligibilityAttestationV1 {
                version: ELIGIBILITY_ATTESTATION_VERSION_V1,
                network: net.domain.as_bytes(),
                exam: d.certification_id,
                verifier: net.kid(card),
                verdict: if reproduced { EligibilityVerdictV1::Reproduced } else { EligibilityVerdictV1::NotReproducible },
                transcript: if reproduced {
                    misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/gate-c-duty/transcript/v1", &seen)
                } else {
                    [0u8; 64]
                },
                claim_mismatches: vec![],
            };
            let o = net.route(card, &K::AttestEligibilityExamV1 { attestation });
            net.send(vec![(card, o)]).await;
            attested += 1;
        }
    }
    assert_eq!(attested, drawn_cards(&net, &row).len(), "each drawn bond attests once");
    assert_eq!(c_row(&net, &claim).unwrap().positive_positions(), TEST_ONLY_KERNEL_CERT_POLICY_V1.m as u32);
    let outcome = until_decided(&mut net, &claim, TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 200).await;
    assert_eq!(outcome, PalwCertificationOutcomeV1::Certified);
    assert!(certified(&net, &claim));
    let daa = net.daa();
    let (_, state) = net.chain.tip_state();
    assert!((0..8).all(|card| palw_certification_duties_v1(&state, &params, &net.chain.bonds[card], daa).is_empty()), "nothing owed");
    let replay = net.replay().await;
    net.assert_same(&replay, "Gate C duty auto-attestation");
}
