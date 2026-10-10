//! RFC12 private maturation: both branches obtain their own signed Panel quorum and actual node Final.
//! Expected before running: the later private frontier can win and reverse X, despite the public claim being Final.
//! This is a pre-safe fork race, not a 5,400-DAA settlement attack or a security approval. The parent harness skips PoW,
//! grinds synthetic floor trace commitments, controls all genesis seat keys and leaves EVM inert. It proves neither
//! honest native execution nor a detection-probability floor. No Final/frontier/state fact is planted.
use super::*;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;

async fn wait_for_phase(chain: &mut T12Chain, id: BlockHash, expected: impl Fn(&PalwClaimPhaseV2) -> bool) {
    for _ in 0..400 {
        if expected(&chain.tip_state().1.claim(&id).unwrap().phase) {
            return;
        }
        chain.heartbeat(chain.config.params.target_time_per_block(), vec![]).await;
    }
    assert!(expected(&chain.tip_state().1.claim(&id).unwrap().phase), "branch lifecycle did not reach its phase in 400 heartbeats");
}

async fn own_final(chain: &mut T12Chain, card: usize, payment: Transaction, float: (TransactionOutpoint, UtxoEntry)) -> BlockHash {
    let (_, id) = chain.attempt(card, 1_000, vec![payment], &|_| true).await;
    chain.attempt_at_the_anchor_slot(id, 7).await;
    let (_, state) = chain.tip_state();
    let panel = state.panel(&id).expect("actual node panel");
    let signed_daa = chain.daa_of(chain.sink());
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        chain.config.params.net.to_string().as_bytes(),
        Some(chain.config.params.genesis.hash),
    );
    let receipts: Vec<_> = panel
        .seats
        .iter()
        .take(chain.bundle.panel.quorum() as usize)
        .map(|seat| {
            let signer = chain.bonds.iter().position(|b| *b == seat.bond).expect("a genesis card");
            let message = palw_receipt_message_v2(domain, id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(signer as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x21; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(id, &receipts).expect("node verifies the signed quorum");
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    let (outpoint, entry) = float;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - 300_000, card_payout_spk(0))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, 0, chain.config.params.storage_mass_parameter);
    chain.heartbeat(1_000, vec![tx]).await;
    wait_for_phase(chain, id, |phase| matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. })).await;
    wait_for_phase(chain, id, |phase| matches!(phase, PalwClaimPhaseV2::Final { .. })).await;
    id
}

#[tokio::test]
async fn rfc12_private_final_can_advance_its_frontier_and_reverse_a_pre_safe_payment() {
    kaspa_core::log::try_init_logger("warn");
    let mut d = duel_on(|params| {
        for fence in kaspa_consensus_core::config::params::PALW_T12_POST_LAUNCH_FENCES_V1 {
            (fence.set)(params, Some(ForkActivation::new(1)));
        }
        kaspa_consensus_core::config::params::palw_t12_arm_capacity_fences_v1(params, Some(ForkActivation::new(1)))
            .expect("the complete release and capacity fence set");
        params.palw_dns_retirement = Some(PalwDnsRetirementV1 {
            activation: ForkActivation::new(1),
            settlement: PalwSettlementPolicyV1 {
                settled_anchor_depth: 1,
                unique_mature_work: 1,
                max_operator_permille: 1000,
                max_class_permille: 1000,
            },
            legacy_evidence_horizon_daa: 5,
        });
    });
    // duel_on constructs both nodes only after ConfigBuilder and complete params validation, including retirement.
    d.config.params.validate_palw_v2().unwrap();
    for _ in 0..3 {
        honest_slot_mirrored(&mut d).await;
    }
    let fork = d.victim.sink();
    assert!(d.victim.vp().dns_retired_at(d.victim.daa_of(d.victim.sink())));
    let public_claim = own_final(&mut d.victim, 0, d.x.clone(), d.floats[0].clone()).await;
    // Delay the private claim and add blue-work siblings, but use the same single claim and the same existing bond capital.
    for _ in 0..4 {
        private_slot(&mut d.attacker, &mut d.nonce, 2, vec![]).await;
    }
    let private_claim = own_final(&mut d.attacker, 2, d.y.clone(), d.floats[0].clone()).await;
    while d.victim.daa_of(d.victim.sink()) < d.attacker.daa_of(d.attacker.sink()) {
        d.victim.heartbeat(d.config.params.target_time_per_block(), vec![]).await;
    }
    let public = d.victim.vp().palw_candidate_order_v2(d.victim.sink()).unwrap();
    let private = d.attacker.vp().palw_candidate_order_v2(d.attacker.sink()).unwrap();
    assert_ne!(private_claim, public_claim);
    assert!(private.safe_frontier_blue_score > public.safe_frontier_blue_score, "private Final advances its own frontier");
    assert!(matches!(d.victim.tip_state().1.claim(&public_claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    assert!(matches!(d.attacker.tip_state().1.claim(&private_claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    assert!(bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork) < d.config.params.finality_depth());
    assert_eq!(payments("private Final before release", &d), (true, false));
    // Feed the actual competing branch to the victim as blocks; the public fork is never fed to the private builder.
    let blocks = blocks_in_topological_order(&d.attacker);
    let at = blocks.iter().position(|b| b.header.hash == fork).unwrap();
    let result = release("RFC12 private Final", &mut d.victim, &blocks[at + 1..], fork, &mut d.nonce, 0).await;
    assert!(result.refused.is_empty(), "the victim validates every private block");
    assert!(result.flipped(), "the active comparator allows the private branch's greater frontier");
    assert_eq!(payments("private Final after release", &d), (false, true));
    assert_eq!(d.victim.sink(), d.attacker.sink());
    assert!(matches!(d.victim.tip_state().1.claim(&private_claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    eprintln!("[RFC12 private Final] public {public:?}, private {private:?}; reverse X before safe, no DNS authority");
}
