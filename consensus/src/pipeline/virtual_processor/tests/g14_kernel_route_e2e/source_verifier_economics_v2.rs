//! Private mechanism acceptance on the actual pipeline: signed source funding
//! and selected approval -> earned native payout -> accepted coinbase -> ACK.
//! Explicit small-program/OPV eligibility and artifact seams remain; this is
//! neither K3 nor a security/economic campaign and approves no public values.
use super::*;
use kaspa_consensus_core::block::MutableBlock;
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::palw_source_bootstrap_v1::*;
use kaspa_consensus_core::palw_source_verifier_economics_v2::*;
use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
use std::collections::BTreeSet;

const FREEZE: u64 = 30;
const ANCHOR: u64 = 40;
const START: u64 = 50;
const END: u64 = 150;
const HORIZON: u64 = 600;
const FEE_PER_CHECKER: u64 = 5_000_007;

fn framed_hash<T: borsh::BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(domain, value))
}
fn source_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config_opv(opv_admitted());
    let mut p = config.params.clone();
    p.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    let network =
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let mut rules = [0; 64];
    rules[..32].copy_from_slice(&p.consensus_identity_id().as_bytes());
    let rules = Hash64::from_bytes(rules);
    let fx = fixture();
    let class = single_class_id_v1(
        k2_tir_v2_descriptor().digest(),
        &fx.program.encode(),
        &fx.plan,
        &fx.pc,
        VerificationModeV1::OptimisticPublicVerification,
    );
    let header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: network.as_bytes(),
        ruleset_digest: rules.as_bytes(),
        class_binding_id: class,
        program_root: program_root_v1(&fx.program.encode()),
        artifact_root: fx.pc.root(),
        plan_root: framed_hash(misaka_palw_kernel::plan::PLAN_ROOT_DOMAIN_V1, &fx.plan).as_bytes(),
    };
    let policy = PalwSelectedSourceBootstrapPolicyV1 {
        version: 1,
        chain_genesis: p.genesis.hash,
        network,
        ruleset: rules,
        epoch: 1,
        freeze_cutoff_daa: FREEZE,
        anchor_cutoff_daa: ANCHOR,
        starts_at_daa: START,
        ends_at_daa: END,
        stake_release_daa: HORIZON,
        selection: misaka_palw_challenge::bootstrap_epoch_v1::BootstrapPolicyV1 {
            version: 1,
            seats: 2,
            quorum: 2,
            maximum_population: 8,
            maximum_draw_attempts: 100,
            maximum_source_works: 2,
        },
        source_profile: Hash64::from_bytes(class),
        checker_scope: framed_hash(b"MISAKA/PALW/SOURCE-COMPLETE-CHECKER/V1", &header),
        maximum_claim_bytes: 64 << 20,
        maximum_check_work: 1_000_000_000_000,
        maximum_history_headers: 100,
        security_review: Hash64::from_bytes([0x61; 64]),
        economic_review: Hash64::from_bytes([0x62; 64]),
    };
    policy.validate().unwrap();
    p.palw_selected_source_bootstrap_v1 = Some(PalwSelectedSourceBootstrapFenceV1 { at: ForkActivation::new(1), policy });
    p.palw_source_verifier_economics_v2 = Some(PalwSourceVerifierEconomicsFenceV2 {
        activation: ForkActivation::new(1),
        policy: PalwSourceVerifierEconomicsPolicyV2 {
            version: 2,
            network,
            bootstrap_policy: policy.id(),
            exposure_per_work_sompi: 7,
            minimum_fee_per_checker_sompi: FEE_PER_CHECKER,
            maximum_funded_works: 2,
            maximum_liabilities: 4,
        },
    });
    p.sync_palw_selected_source_bootstrap_v1();
    p.sync_palw_source_verifier_economics_v2();
    assert_eq!(
        &p.consensus_identity_id().as_bytes()[..],
        &rules.as_bytes()[..32],
        "non-genesis private fences normalize out of the native ruleset identity"
    );
    assert!(p.validate_palw_selected_source_bootstrap_v1().is_err());
    assert!(p.validate_palw_source_verifier_economics_v2().is_err());
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    (Config::new(p.clone()), bundle.clone(), premine, floats)
}
fn sign_source(net: &mut Net, card: usize, action: PalwSourceBootstrapActionV1) -> Obj {
    let actor = net.bond(card);
    let message = palw_source_bootstrap_message_v1(net.domain, actor, &action);
    net.rnd = net.rnd.wrapping_add(1);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1,
        [net.rnd; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    Obj::SourceBootstrapV1 { actor, action, signature }
}
fn funded(state: &PalwChainStateV2, work: Hash64) -> PalwSourceVerifierFundingV2 {
    *state.source_verifier_funding_v2(work).unwrap().unwrap()
}
fn liability(state: &PalwChainStateV2, actor: PalwBondKeyV2, work: Hash64) -> PalwSourceVerifierLiabilityV2 {
    let mut key = vec![2];
    key.extend(borsh::to_vec(&actor).unwrap());
    key.extend_from_slice(work.as_byte_slice());
    state.kernel_route().unwrap().aux_row(SOURCE_VERIFIER_ECONOMICS_TABLE_V2, &key).unwrap()
}
// Count the complete virtual set, including a side branch's retained coins.
// Restricting to selected-chain block IDs could hide a double credit on reorg.
fn virtual_coinbase_total(chain: &T12Chain, card: usize) -> u128 {
    Net::utxos_of(chain)
        .values()
        .filter(|entry| entry.is_coinbase && entry.script_public_key == card_payout_spk(card))
        .map(|entry| u128::from(entry.amount))
        .sum()
}
async fn open_and_fund(w: &mut World, producer: usize, claim: &Claim) -> Hash64 {
    let policy = w.net.config.params.palw_selected_source_bootstrap_v1.unwrap().policy;
    let prospective = w.net.chain.tip_state().1.source_bootstrap_prepare_work_v1(Hash64::from_bytes(claim.id)).unwrap();
    let object = sign_source(
        &mut w.net,
        producer,
        PalwSourceBootstrapActionV1::Open { policy: policy.id(), epoch: policy.epoch, claim: Hash64::from_bytes(claim.id) },
    );
    w.net.send(vec![(producer, object)]).await;
    let id = prospective.id();
    let state = w.net.chain.tip_state().1;
    assert_eq!(state.source_bootstrap_work_v1(id).unwrap().unwrap().work, prospective, "actual signed Open creates exact native work");
    let terms = PalwSourceVerifierFeeTermsV2 {
        version: 2,
        policy: w.net.config.params.palw_source_verifier_economics_v2.unwrap().policy.id(),
        work: id,
        sponsor: w.net.bond(producer),
        fee_per_checker_sompi: FEE_PER_CHECKER,
    };
    let before = w.net.collateral(producer);
    let object = sign_source(&mut w.net, producer, PalwSourceBootstrapActionV1::FundVerifierEconomicsV2 { terms });
    w.net.send(vec![(producer, object)]).await;
    let f = funded(&w.net.chain.tip_state().1, id);
    assert_eq!(f.terms, terms);
    assert_eq!(before - w.net.collateral(producer), f.prepaid, "funding debits actual native bond, not R");
    assert!(f.conserves());
    id
}
async fn approve_selected(w: &mut World, claim: &Claim, work: Hash64) -> BTreeSet<usize> {
    let epoch = w.net.chain.tip_state().1.source_bootstrap_epoch_v1().unwrap().unwrap();
    let funding = funded(&w.net.chain.tip_state().1, work).terms.id();
    let mut actors = BTreeSet::new();
    let mut objects = Vec::new();
    for (slot, seat) in epoch.cohort.iter().enumerate() {
        let card = (0..w.net.chain.bonds.len()).find(|i| w.net.kid(*i) == seat.bond).unwrap();
        // Full local deterministic check precedes each selected signature; it
        // supplies no consensus credit without that selected actor's signature.
        assert_eq!(w.fresh(0x80 + slot as u8).check(claim.id, &claim.published(&w.fx, &[]), &w.fx.params), OutsiderFindingV1::Clean);
        let signed_daa = w.net.daa();
        objects.push((
            card,
            sign_source(
                &mut w.net,
                card,
                PalwSourceBootstrapActionV1::ApproveWithLiabilityV2 {
                    work,
                    slot: slot as u16,
                    statement: work,
                    signed_daa,
                    funding_id: funding,
                },
            ),
        ));
        actors.insert(card);
    }
    w.net.send(objects).await;
    let state = w.net.chain.tip_state().1;
    assert_eq!(state.source_bootstrap_work_v1(work).unwrap().unwrap().approvals.len(), epoch.cohort.len());
    for card in &actors {
        assert_eq!(liability(&state, w.net.bond(*card), work).exposure, 7);
    }
    actors
}
fn assert_ack(net: &Net, work: Hash64, actors: &BTreeSet<usize>, refund_work: Hash64, block: &Block) {
    let state = net.chain.tip_state().1;
    for card in actors {
        assert!(
            matches!(liability(&state,net.bond(*card),work).entitlement,PalwSourceVerifierEntitlementV2::QueueAcknowledged {acknowledged_block,acknowledged_daa,amount,..} if acknowledged_block==block.header.hash && acknowledged_daa==block.header.daa_score && amount==FEE_PER_CHECKER)
        );
    }
    let refund = funded(&state, refund_work);
    assert_eq!(refund.refund_acknowledged_block, Some(block.header.hash));
    assert_eq!(refund.refund_acknowledged_daa, Some(block.header.daa_score));
}

#[tokio::test]
async fn rfc08_source_checker_fee_and_unused_principal_reach_accepted_coinbase_once_after_restart_and_reorg() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(source_config(), |c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on_opv(net).await;
    w.net.beat_to(START).await;
    let first_job = w.job().await;
    let first = w.claim(0, &first_job, false).await;
    let work = open_and_fund(&mut w, 0, &first).await;
    let actors = approve_selected(&mut w, &first, work).await;
    // A second real, distinct input earns no selected approval. Its explicitly
    // funded principal must return without becoming a verifier reward.
    let second_job = KernelJobV1 {
        class_binding_id: w.class,
        prompt: vec![4, 17, 9],
        max_new_tokens: 2,
        decode: DecodeRuleV1::Greedy,
        nonce: [0x42; 64],
    };
    let object = w.net.route(1, &K::PostJob { job: second_job.clone() });
    w.net.send(vec![(1, object)]).await;
    let second = w.claim(2, &second_job, false).await;
    let refund_work = open_and_fund(&mut w, 2, &second).await;
    assert_ne!(work, refund_work);
    assert!(w.net.chain.tip_state().1.source_bootstrap_work_v1(refund_work).unwrap().unwrap().approvals.is_empty());
    w.net.beat_to(HORIZON).await;
    let before = w.net.chain.tip_state().1;
    assert!(
        before.source_bootstrap_work_v1(work).unwrap().unwrap().settlement.is_some(),
        "selected approval plus actual native settlement"
    );
    assert!(!funded(&before, work).closed && !funded(&before, refund_work).closed, "inclusive horizon still holds both prepayments");
    let baseline =
        (0..w.net.chain.bonds.len()).map(|card| (card, virtual_coinbase_total(&w.net.chain, card))).collect::<BTreeMap<_, _>>();
    let ttpb = w.net.ttpb();
    // Consecutive native blocks may share a DAA. The horizon is inclusive,
    // so observe the first actual block strictly beyond it, not one heartbeat.
    let queued_block = loop {
        let block = w.net.chain.heartbeat(ttpb, vec![]).await;
        if block.header.daa_score > HORIZON {
            break block;
        }
        assert_eq!(block.header.daa_score, HORIZON);
        let same_daa = w.net.chain.tip_state().1;
        for pending in [funded(&same_daa, work), funded(&same_daa, refund_work)] {
            assert!(!pending.closed, "another block at H cannot release the inclusive hold");
            assert_eq!((pending.queued, pending.refunded), (0, 0), "same-DAA block cannot allocate fees or refund early");
        }
    };
    let queued = w.net.chain.tip_state().1;
    let fees = funded(&queued, work);
    let principal = funded(&queued, refund_work);
    assert_eq!(fees.queued, FEE_PER_CHECKER * actors.len() as u64);
    assert_eq!(fees.prepaid, fees.queued);
    assert_eq!(fees.refunded, 0);
    assert!(fees.closed && fees.conserves());
    assert_eq!(principal.refunded, principal.prepaid);
    assert_eq!(principal.queued, 0);
    assert!(principal.closed && principal.conserves());
    assert!(principal.refund_acknowledged_block.is_none());
    let mut expected = BTreeMap::<usize, u64>::new();
    for card in &actors {
        *expected.entry(*card).or_default() += FEE_PER_CHECKER;
    }
    *expected.entry(2).or_default() += principal.prepaid;
    for (card, amount) in &expected {
        assert_eq!(w.net.owed(*card), *amount, "queue originates from actual source funding and approval");
    }
    let queue_root = queued.state_root();
    let queue_utxos = Net::utxos_of(&w.net.chain);
    w = w.restart(db.clone());
    assert_eq!(w.net.chain.tip_state().1.state_root(), queue_root);
    assert_eq!(Net::utxos_of(&w.net.chain), queue_utxos);
    let payment = w.net.chain.heartbeat(ttpb, vec![]).await;
    assert_eq!(w.net.chain.ctx.consensus.block_status(payment.header.hash), BlockStatus::StatusUTXOValid);
    for (card, amount) in &expected {
        assert!(
            payment.transactions[0].outputs.iter().any(|o| o.value == *amount && o.script_public_key == card_payout_spk(*card)),
            "actual template coinbase pays exact source allocation to card {card}"
        );
    }
    assert_ack(&w.net, work, &actors, refund_work, &payment);
    // Coinbase outputs enter the accepted UTXO set when the next block accepts it.
    w.net.chain.heartbeat(ttpb, vec![]).await;
    for (card, amount) in &expected {
        assert_eq!(
            (virtual_coinbase_total(&w.net.chain, *card), w.net.owed(*card)),
            (baseline[card] + u128::from(*amount), 0),
            "accepted UTXO credit once"
        );
    }

    // An independent native replay has the same earned pending queue, then
    // rejects a coinbase whose source payment differs by one sompi.
    let victim = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    for block in chain_blocks(&w.net.chain, queued_block.header.hash) {
        arrive(&victim, block, "pre-payment replay").await;
    }
    assert_eq!(victim.tip_state().1.state_root(), queue_root);
    let mut bad = MutableBlock::new((*payment.header).clone(), payment.transactions.as_ref().clone());
    let (card, amount) = expected.iter().next().unwrap();
    let output =
        bad.transactions[0].outputs.iter_mut().find(|o| o.value == *amount && o.script_public_key == card_payout_spk(*card)).unwrap();
    output.value += 1;
    bad.transactions[0].finalize();
    bad.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(bad.transactions.iter());
    bad.header.finalize();
    let bad_hash = bad.header.hash;
    let _ = victim.ctx.consensus.validate_and_insert_block(bad.to_immutable()).virtual_state_task.await;
    assert_eq!(
        victim.ctx.consensus.block_status(bad_hash),
        BlockStatus::StatusDisqualifiedFromChain,
        "real FullUTXO coinbase comparison rejects changed source payment"
    );
    assert_eq!(victim.tip_state().1.state_root(), queue_root, "rejected payment cannot acknowledge duties");

    // Rewind to the earned queue on another branch. Its later first payment
    // has its own block/DAA receipt; importing the heavier branch undoes A's ACK.
    let mut branch = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let prefix = chain_blocks(&w.net.chain, queued_block.header.hash);
    branch.ctx.simulated_time = prefix.last().unwrap().header.timestamp;
    for block in prefix {
        arrive(&branch, block, "queue branch prefix").await;
    }
    let branch_payment = branch.heartbeat(ttpb, vec![]).await;
    assert_ne!(branch_payment.header.hash, payment.header.hash);
    let mut competing = vec![branch_payment.clone()];
    for _ in 0..4 {
        competing.push(branch.heartbeat(ttpb, vec![]).await);
    }
    for block in competing {
        arrive(&w.net.chain, block, "longer payment branch").await;
    }
    assert_eq!(w.net.chain.sink(), branch.sink(), "actual heavier branch causes the reorg");
    assert_ack(&w.net, work, &actors, refund_work, &branch_payment);
    for (card, amount) in &expected {
        assert_eq!(
            (virtual_coinbase_total(&w.net.chain, *card), w.net.owed(*card)),
            (baseline[card] + u128::from(*amount), 0),
            "reorg credit is once, not one payment per fork"
        );
    }
    let final_root = w.net.chain.tip_state().1.state_root();
    let final_utxos = Net::utxos_of(&w.net.chain);
    w = w.restart(db.clone());
    assert_eq!(w.net.chain.tip_state().1.state_root(), final_root);
    assert_eq!(Net::utxos_of(&w.net.chain), final_utxos);
    let replay = w.net.replay().await;
    w.net.assert_same(&replay, "source fee/refund final replay");
    let fee_end = funded(&w.net.chain.tip_state().1, work);
    let refund_end = funded(&w.net.chain.tip_state().1, refund_work);
    assert_eq!((fee_end.queued, refund_end.refunded), (fees.queued, principal.refunded), "ACKs preserve cumulative allocations");
}
