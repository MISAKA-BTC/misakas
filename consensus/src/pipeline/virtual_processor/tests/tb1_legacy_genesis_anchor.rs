//! Extend an independently replayed Final using the ordinary model allocation,
//! an executed floor job and the unchanged Gate C roster and deadlines.
use super::*;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::attest_drawn_final;
use kaspa_consensus_core::{palw_backend::PalwExecutionBackendV1, palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1};
use libcrux_ml_dsa::ml_dsa_87;

pub(super) fn save(node: &Rig, claim: Hash64, base: Hash64, path: &Path) {
    let tip = node.chain.sink();
    let mut at = tip;
    let mut blocks = Vec::new();
    while at != base {
        blocks.push(RpcRawBlock::from(&node.api().get_block(at).unwrap()));
        at = node.api().get_ghostdag_data(at).unwrap().selected_parent;
    }
    blocks.reverse();
    let mut output = std::io::BufWriter::new(File::create_new(path).unwrap());
    serde_json::to_writer(&mut output, &CertifiedBranch { base, tip, claim, blocks }).unwrap();
    output.flush().unwrap();
    output.get_ref().sync_all().unwrap();
}

pub(super) async fn continue_final(
    node: &mut Rig,
    prefix: &CertifiedBranch,
    real: &RealAttemptV1,
    backend: &dyn PalwExecutionBackendV1,
    transcript: [u8; 64],
    deadline: u64,
    dir: &Path,
) {
    std::fs::create_dir(dir).unwrap();
    assert_eq!((node.chain.sink(), real.claim), (prefix.tip, prefix.claim));
    let change = prefix
        .blocks
        .iter()
        .rev()
        .find_map(|raw| {
            let block = Block::try_from(raw.clone()).unwrap();
            block
                .transactions
                .iter()
                .find(|tx| tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE)
                .map(|tx| kaspa_consensus_core::tx::TransactionOutpoint::new(tx.id(), 0))
        })
        .unwrap();
    let mut funds = node.api().get_virtual_utxos(None, 1_000_000, false).into_iter().find(|(o, _)| *o == change).unwrap();
    let floor = node.chain.bundle.base_class_id;
    let card = 5;
    let bond = node.chain.bonds[card];
    let (_, state) = node.chain.tip_state();
    let budget = state.bond_budget().unwrap();
    let at = node.chain.daa_of(prefix.tip);
    assert!(budget.model_blocks_available_v1(&floor, at) < PALW_BUDGET_BLOCK_UNIT_V1);
    assert!(budget.assignment_row(&bond).is_none_or(|r| r.sequence == 0 && r.effective.is_empty() && r.pending.is_none()));
    let amount = state.bond(&bond).unwrap().collateral / 2;
    let assignments = vec![(floor, amount)];
    let message = palw_capital_assignment_message_v1(node.domain, &bond, &assignments, 1);
    let signature = ml_dsa_87::sign(
        &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
        message.as_byte_slice(),
        PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1,
        [0x62; 32],
    )
    .unwrap()
    .as_slice()
    .to_vec();
    send_lifecycle(
        &mut node.chain,
        &mut funds,
        PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence: 1, signature },
    )
    .await;
    let step = node.config.params.target_time_per_block();
    loop {
        let (tip, state) = node.chain.tip_state();
        let score = node.chain.daa_of(tip);
        let budget = state.bond_budget().unwrap();
        let available = budget.model_blocks_available_v1(&floor, score);
        assert!(score < deadline, "no floor opportunity within the original claim's draw deadline");
        if available >= PALW_BUDGET_BLOCK_UNIT_V1 {
            peer_log!(
                "funded floor opportunity claim={} base={} tip={} daa={} floor={} card={} assigned={} microblocks={} deadline={}",
                real.claim,
                prefix.tip,
                tip,
                score,
                floor,
                card,
                amount,
                available,
                deadline
            );
            break;
        }
        node.chain.heartbeat(step, vec![]).await;
    }
    let anchor = node.chain.real_attempt(card, step).await;
    let (_, state) = node.chain.tip_state();
    assert!(state.claim(&anchor.claim).is_some(), "the floor opportunity must admit its executed claim");
    assert_eq!(state.round_seed_anchor().unwrap().block, anchor.block.hash());
    use kaspa_consensus_core::palw_security_certification_v1::*;
    loop {
        let (tip, state) = node.chain.tip_state();
        let row: PalwClaimCertificationRowV1 = state
            .kernel_route_state_v1()
            .unwrap()
            .aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_claim_key_v1(&palw_certification_id_v1(&real.claim)))
            .unwrap();
        assert!(row.outcome.is_none(), "the original certification ended: {:?}", row.outcome);
        if row.anchor.is_some() {
            assert_eq!(row.draws.len(), usize::from(node.chain.bundle.state.rfc0018_v1().unwrap().certification.m));
            peer_log!(
                "funded floor draw claim={} anchor_claim={} block={} tip={} daa={} positions={}",
                real.claim,
                anchor.claim,
                anchor.block.hash(),
                tip,
                row.drawn_daa,
                row.draws.len()
            );
            break;
        }
        assert!(node.chain.daa_of(tip) < deadline);
        node.chain.heartbeat(step, vec![]).await;
    }
    save(node, real.claim, prefix.tip, &dir.join("draw.json"));
    let draw_tip = node.chain.sink();
    attest_drawn_final(&mut node.chain, &mut funds, real.claim, |chain, card| {
        let actual = real.replay_with_backend(chain, backend).unwrap();
        assert_eq!(actual, transcript);
        peer_log!("verified Gate C execution {} card={} transcript={}", real.claim, card, faster_hex::hex_string(&actual));
        actual
    })
    .await;
    let certified = executed_certification(&node.chain, real.claim);
    assert!(certified.attestations.iter().all(|a| a.transcript == transcript));
    save(node, real.claim, draw_tip, &dir.join("certification.json"));
    peer_log!(
        "funded floor certified claim={} tip={} outcome={:?} approvals={}",
        real.claim,
        node.chain.sink(),
        certified.outcome,
        certified.positive_positions()
    );
}
