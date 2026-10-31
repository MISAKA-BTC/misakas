//! RFC-0041 through the node's admission, coinbase, UTXO and ordinary registration paths.
use super::*;
use kaspa_consensus_core::config::params::{PALW_BASE_COINBASE_RECOVERY_WAIT_MS_V1, palw_base_coinbase_recovery_bond_outpoint_v1};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, palw_bond_registration_message_v2,
    palw_operator_possession_message_v1,
};

fn empty_genesis_chain() -> (T12Chain, (TransactionOutpoint, UtxoEntry)) {
    let (config, _, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { unreachable!() };
    bundle.genesis_objects.retain(|object| !matches!(object, Obj::BondRegistered { .. }));
    // These are owned genesis UTXOs, with no associated role in the empty registry. Recommit
    // the actual output set, as a new genesis must; no live network's history is rewritten.
    let main = kaspa_consensus_core::config::premine::premine_outpoint_for(
        params.net,
        kaspa_consensus_core::config::premine::MAIN_PREMINE_INDEX,
    );
    let funding = premine.iter_mut().find(|(outpoint, _)| *outpoint == main).unwrap();
    funding.1.script_public_key = card_payout_spk(0);
    let funding = funding.clone();
    let mut commitment = MuHash::new();
    for (outpoint, entry) in &premine {
        commitment.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = commitment.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("a zero-seat genesis has no genesis jury to lose a spare from");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
    assert!(chain.tip_state().1.bonds_iter().next().is_none());
    (chain, funding)
}

fn recovery(chain: &mut T12Chain, timestamp: u64, win: bool, valid_signature: bool) -> MutableBlock {
    chain.nonce += 1;
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(
            MinerData::new(card_payout_spk(0), vec![]),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    template.block.header.timestamp = timestamp;
    template.block.header.nonce = chain.nonce;
    let header = &template.block.header;
    let bond = palw_base_coinbase_recovery_bond_outpoint_v1();
    let facts = chain.ctx.consensus.palw_producer_facts_v2(chain.bundle.base_class_id, Some(bond)).unwrap();
    assert!(facts.bond.is_none());
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let mut attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: chain.network_domain,
        challenge: challenge_v2(chain.network_domain, pre_pow, timestamp, header.nonce, facts.class_id, &bond),
        class_id: facts.class_id,
        executor_bond: bond,
        executor_pubkey: card_pubkey(0),
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&card_pubkey(0)),
        artifact_root: facts.artifact_root,
        trace_root: Hash64::default(),
        output_root: Hash64::from_u64_word(0x4100 + chain.nonce),
        execution_root: Hash64::from_u64_word(0x4200 + chain.nonce),
        pwu: facts.pwu,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
    };
    let anchor = execution_anchor_v3(chain.network_domain, pre_pow, facts.class_id, &bond, header.nonce);
    let draw = (0..4_000_000u64).find(|draw| {
        attempt.trace_root = Hash64::from_u64_word((chain.nonce << 32) ^ draw ^ 0x4100_0000);
        attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
        (class_ticket_v3(&attempt, anchor) <= facts.class_target) == win
    });
    assert!(draw.is_some(), "the floor has both winning and losing draws");
    let mut signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &card_key(0).signing_key,
        attempt_id_v2(&attempt).as_byte_slice(),
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [0x41; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    if !valid_signature {
        signature[0] ^= 1;
    }
    template.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    template.block.header.finalize();
    template.block
}

async fn rejected(chain: &mut T12Chain, block: MutableBlock) {
    let sink = chain.sink();
    let hash = block.header.hash;
    let before: std::collections::HashMap<_, _> = chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
    let _ = chain.ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await;
    assert_ne!(chain.sink(), hash, "a refused recovery never becomes the selected chain");
    assert_eq!(chain.sink(), sink);
    let after: std::collections::HashMap<_, _> = chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
    assert_eq!(before, after, "a rejected recovery neither mints nor spends any UTXO");
    assert!(chain.tip_state().1.bonds_iter().next().is_none());
}

fn registration(chain: &T12Chain, funding: &(TransactionOutpoint, UtxoEntry), card: usize) -> Transaction {
    let collateral = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let key = card_pubkey(card);
    let payout = card_payout_payload(card);
    let signed = PalwBondKeyV2(TransactionOutpoint::new(Hash64::default(), 1));
    let classes = std::collections::BTreeSet::from([chain.bundle.base_class_id]);
    let message = palw_bond_registration_message_v2(chain.network_domain, &signed, &key, &key, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(chain.network_domain, &signed, &key, &key);
    let sign = |message: &[u8], context: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&card_key(card).signing_key, message, context, [0x42; 32]).unwrap().as_ref().to_vec()
    };
    let mut signature = sign(message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(sign(possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let object = Obj::BondRegistered {
        bond: signed,
        pubkey: key.clone(),
        operator_pubkey: key,
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(funding.0, vec![], 0, 1)],
        vec![
            TransactionOutput::new(funding.1.amount - collateral - 1_000_000_000, card_payout_spk(0)),
            TransactionOutput::new(collateral, card_payout_spk(card)),
        ],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, funding.1.clone(), 0, chain.config.params.storage_mass_parameter);
    tx
}

#[tokio::test]
async fn rfc0041_production_recovery_pays_then_public_bonds_start_palw() {
    let (mut chain, mut funding) = empty_genesis_chain();
    let first = chain.config.params.genesis.timestamp + PALW_BASE_COINBASE_RECOVERY_WAIT_MS_V1;
    let early = recovery(&mut chain, first - 1, true, true);
    rejected(&mut chain, early).await;
    let forged = recovery(&mut chain, first, true, false);
    rejected(&mut chain, forged).await;
    let loser = recovery(&mut chain, first, false, true);
    rejected(&mut chain, loser).await;
    let good = recovery(&mut chain, first, true, true);
    let recovery_hash = good.header.hash;
    chain.ctx.simulated_time = first;
    chain.insert_chain_block(good, "the first public BASE recovery").await;
    let step = chain.config.params.target_time_per_block();
    let paying = chain.heartbeat(step, vec![]).await;
    assert!(
        paying.transactions[0].outputs.iter().any(|output| output.script_public_key == card_payout_spk(0) && output.value > 0),
        "the child coinbase actually pays the recovered worker subsidy"
    );
    assert!(chain.tip_state().1.bonds_iter().next().is_none(), "payment registers no bond or seat");
    assert!(
        !chain.tip_state().1.claims_iter().any(|(_, claim)| claim.accepted_block == recovery_hash),
        "recovery emits no certified work"
    );

    // All eight participants use signed, funded public registrations; the genesis output owner
    // merely supplies spendable funds, and never acquires an automatic role from its ownership.
    for card in 0..8 {
        let tx = registration(&chain, &funding, card);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 1));
        chain.heartbeat(step, vec![tx.clone()]).await;
        chain.heartbeat(step, vec![]).await;
        let state = chain.tip_state().1;
        assert!(state.bond(&bond).is_some(), "ordinary registration created card {card}");
        chain.bonds.push(bond);
        funding = chain
            .ctx
            .consensus
            .get_virtual_utxos(None, 1_000_000, false)
            .into_iter()
            .find(|(outpoint, _)| *outpoint == TransactionOutpoint::new(tx.id(), 0))
            .expect("accepted registration leaves a real spendable change output");
    }
    let maturity = chain.config.params.palw_bond_maturity.expect("the ordinary maturation policy stays scheduled").window_daa;
    let last_registration = chain.tip_state().1.bonds_iter().map(|(_, bond)| bond.registered_daa).max().unwrap();
    // Wait the unchanged policy duration on actual validated heartbeat blocks. Advancing only
    // the timestamp or planting a state would not exercise the ordinary maturation path.
    chain.beat_until(4 * maturity + 100, "ordinary bonds mature", |chain| chain.sink_daa() >= last_registration + maturity).await;
    let (block, claim) = chain.attempt(0, step, vec![], &|_| true).await;
    let state = chain.tip_state().1;
    assert_eq!(state.claim(&claim).expect("a normal producer now creates a PALW claim").accepted_block, block.header.hash);
    assert_eq!(state.claim(&claim).unwrap().bond, chain.bonds[0]);
    let anchor = chain.attempt_at_the_anchor_slot(claim, 7).await;
    let state = chain.tip_state().1;
    let panel = state.panel(&claim).expect("ordinary mature participants fill the actual drawn panel");
    assert_eq!(panel.anchor, anchor.header.hash);
    assert_eq!(panel.seats.len(), chain.bundle.panel.seat_count() as usize);
    assert!(panel.seats.iter().all(|seat| chain.bonds.contains(&seat.bond) && seat.bond != chain.bonds[0]));
}
