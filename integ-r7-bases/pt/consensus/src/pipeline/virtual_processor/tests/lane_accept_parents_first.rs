//! **Lane accept-order (`Params::palw_lane_accept_parents_first`, post-launch): a merging block applies
//! a tied round lane parents-first — and below the fence exactly as it always did.**
//!
//! Every round block of a lane carries its anchor's blue work (ADR-0125), so the consensus order
//! `(blue_work, hash)` lists a tied lane by hash, and the merging block accepted it that way: on
//! testnet-12 at DAA 316 the merging block `42285c86…` skipped three carriers in the first round block
//! holding them because their inputs came from round blocks that sort later. Here a two-block lane
//! whose CHILD sorts first (its nonce is ground for it) carries a chained pair — `A` in the parent,
//! `B` spending `A`'s output in the child — and both blocks hold their permits:
//!
//! * below the fence (dormant, and armed at a height the chain never reaches) `B` is skipped and the
//!   round payout collects `A`'s fee alone — today's behaviour, pinned — and the two runs are the same
//!   blocks, hash for hash;
//! * past the fence the lane is applied parents-first: `A` then `B`, both accepted, both fees paid;
//! * a node fed the same DAG headers-first then bodies reaches the same sink, UTXO-valid;
//! * from the fence a round block may not carry an EVM payload;
//! * testnet-12 with EVERY post-launch fence at one height crosses it with round lanes below and above.
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, t12_genesis_chain, t12_with_harness_cards};
use super::{
    OnetimeTxSelector, TestContext, adr0125_config_funded_for, adr0125_harness_bond, adr0125_round_block, adr0125_schedule_of,
    adr0125_sign_round_block, new_miner_data,
};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use std::ops::Deref;
use std::thread::JoinHandle;

const FEE_A: u64 = 30_000;
const FEE_B: u64 = 50_000;

/// A spend of `outpoint` (worth `value`, an output of `spk`) to `spk` less `fee`, signed by `kp`.
fn signed_spend(
    kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    spk: &ScriptPublicKey,
    outpoint: TransactionOutpoint,
    value: u64,
    utxo_daa: u64,
    is_coinbase: bool,
    fee: u64,
    storage_mass_parameter: u64,
) -> Transaction {
    use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_consensus_core::mass::MassCalculator;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_consensus_core::tx::{PopulatedTransaction, TransactionInput, TransactionOutput};
    use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(value - fee, spk.clone())],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let utxo = UtxoEntry::new(value, spk.clone(), utxo_daa, is_coinbase);
    let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
        .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
        .expect("contextual mass is computable for the spend")
        .storage_mass;
    tx.set_mass(storage_mass);
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let sig_hash = calc_mldsa87_signature_hash(&PopulatedTransaction::new(&tx, vec![utxo]), 0, SIG_HASH_ALL, &reused);
    let sig = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x31u8; 32])
        .expect("ML-DSA-87 sign on the 64-byte sighash");
    let mut sig_item = sig.as_ref().to_vec();
    sig_item.push(SIG_HASH_ALL.to_u8());
    tx.inputs[0].signature_script = ScriptBuilder::new()
        .add_data(&sig_item)
        .expect("the signature push fits")
        .add_data(kp.verification_key.as_ref())
        .expect("the key fits")
        .drain();
    tx
}

/// The harness bond's span-0 schedule, planted at the sink's state (as a pruned sync would install a
/// state, `adr0127_round_burst`): the even rounds are the harness bond's permits. The same inputs on
/// every node give the same state.
fn plant_the_schedule(consensus: &TestConsensus, bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2) {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let vp = consensus.virtual_processor().clone();
    let sink = consensus.get_sink();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    assert_eq!(tip, sink);
    let bond = state.bond(&adr0125_harness_bond()).expect("row 0").clone();
    let schedule = adr0125_schedule_of(
        0,
        &[PalwExecFinalV1 {
            domain: bundle.base_class_id,
            bond: adr0125_harness_bond(),
            operator_id: bond.operator_id,
            claim_id: Hash64::from_u64_word(0xC1A1),
            execution_root: Hash64::from_u64_word(0xE0),
            credit: 1,
            accepted_blue_score: 0,
        }],
    );
    let scheduled = {
        let mut carriage = PalwStateCarriageV2::from_state(&state);
        carriage.round_schedules.insert(0, schedule);
        carriage.into_state(&bundle.state, None).expect("a carriage with a schedule rebuilds")
    };
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &scheduled).unwrap();
}

/// A round block of `round` carrying exactly `txs` (behind its coinbase), signed for the harness
/// bond's permit 0 with `nonce`. The template cannot carry `B` (its input is not in the virtual UTXO
/// set until `A` is accepted), so the body is set by hand — a round block is never UTXO-validated on
/// its own, only its merging block accepts or skips what it carries.
fn round_block_carrying(
    ctx: &TestContext,
    config: &Config,
    round: u64,
    payout: &ScriptPublicKey,
    txs: Vec<Transaction>,
    nonce: u64,
) -> MutableBlock {
    let template = ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    let mut block =
        ctx.consensus.virtual_processor().round_adapt_block_template(template, round, payout.clone()).expect("the lane adapts").block;
    block.transactions.truncate(1);
    block.transactions.extend(txs);
    block.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(block.transactions.iter());
    adr0125_sign_round_block(&mut block, config, round, 0, nonce);
    assert!(block.evm_payload.is_empty(), "the node's round template carries no EVM payload");
    block
}

/// What one lane run leaves at the merging block.
struct LaneRun {
    ctx: TestContext,
    config: Config,
    bundle: kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    /// Every block in insertion order, and the sink the schedule was planted at.
    blocks: Vec<Block>,
    planted_at: BlockHash,
    /// `[parent, child]` of the lane: the child sorts FIRST by hash.
    lane: [BlockHash; 2],
    /// `[A, B]`: `B` spends `A`'s output.
    txs: [TransactionId; 2],
    merging: Block,
    /// The merging block's acceptance, in its mergeset order: (merged block, accepted tx ids).
    acceptance: Vec<(BlockHash, Vec<TransactionId>)>,
    /// What the merging block's coinbase pays the round payout.
    round_payout: u64,
}

impl LaneRun {
    fn accepted(&self, tx: TransactionId) -> bool {
        self.acceptance.iter().any(|(_, ids)| ids.contains(&tx))
    }

    fn position(&self, block: BlockHash) -> usize {
        self.acceptance.iter().position(|(hash, _)| *hash == block).expect("a merged block has an acceptance row")
    }
}

/// adr0125's harness (the lane open from genesis, span 0 scheduled for the harness bond) with lane
/// accept-order at `fence`, a funded key, and a lane `r0 ← r1` carrying `A` / `B`, merged by the next
/// chain block. Deterministic: the miner data, the keys and every signature's randomness are fixed.
async fn lane_run(fence: Option<ForkActivation>) -> LaneRun {
    let (mut config, bundle) = adr0125_config_funded_for(32);
    config.params.palw_lane_accept_parents_first = fence;
    config.params.validate_palw_v2().expect("the harness with lane accept-order validates");
    // So the funding coinbase is spendable within a short chain (after the ruleset check, as
    // `adr0127_round_burst` does: a V2 ruleset derives its maturity from the 120 s rate).
    config.params.coinbase_maturity = 2;
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let fixed_miner = MinerData::new(p2pkh_mldsa87_spk(&[0x5Au8; 64]), vec![]);
    ctx.miner_data = fixed_miner.clone();
    let mut blocks: Vec<Block> = Vec::new();
    fn row(ctx: &mut TestContext, blocks: &mut Vec<Block>) {
        ctx.simulated_time += ctx.consensus.params().target_time_per_block();
        let t = ctx.build_block_template(blocks.len() as u64, ctx.simulated_time);
        blocks.push(t.block.to_immutable());
    }
    for _ in 0..4 {
        row(&mut ctx, &mut blocks);
        ctx.validate_and_insert_block(blocks.last().unwrap().clone()).await.assert_valid_utxo_tip();
    }

    // A key this test spends from (adr0127_round_burst's funding: two siblings to the key, the block
    // merging them pays the one that is not its selected parent).
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x29u8; 32]);
    let address_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes();
    let spk = p2pkh_mldsa87_spk(&address_payload);
    ctx.miner_data = MinerData::new(spk.clone(), vec![]);
    ctx.simulated_time += config.params.target_time_per_block();
    let siblings = [ctx.build_block_template(900, ctx.simulated_time), ctx.build_block_template(901, ctx.simulated_time)];
    for sibling in siblings.iter() {
        blocks.push(sibling.block.clone().to_immutable());
        ctx.validate_and_insert_block(sibling.block.clone().to_immutable()).await;
    }
    ctx.miner_data = fixed_miner.clone();
    ctx.simulated_time += config.params.target_time_per_block();
    let harvest = ctx.build_block_template(40, ctx.simulated_time);
    blocks.push(harvest.block.clone().to_immutable());
    ctx.validate_and_insert_block(harvest.block.clone().to_immutable()).await.assert_valid_utxo_tip();
    let coinbase = &harvest.block.transactions[0];
    let (index, output) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_public_key == spk)
        .expect("the block merging the siblings pays the one that is not its selected parent");
    let funding = TransactionOutpoint::new(coinbase.id(), index as u32);
    let (value, funding_daa) = (output.value, harvest.block.header.daa_score);
    for _ in 0..4 {
        row(&mut ctx, &mut blocks);
        ctx.validate_and_insert_block(blocks.last().unwrap().clone()).await.assert_valid_utxo_tip();
    }

    // `A` spends the funding; `B` spends `A`'s output — a chain across two round blocks.
    let storage = config.params.storage_mass_parameter;
    let a = signed_spend(&kp, &spk, funding, value, funding_daa, true, FEE_A, storage);
    let b = signed_spend(&kp, &spk, TransactionOutpoint::new(a.id(), 0), value - FEE_A, funding_daa, false, FEE_B, storage);

    plant_the_schedule(&ctx.consensus, &bundle);
    let planted_at = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor().clone();
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let payout = p2pkh_mldsa87_spk(state.bond(&adr0125_harness_bond()).expect("row 0").payout_payload.as_byte_slice());
    let genesis_ts = config.params.genesis.timestamp;
    let first_round = {
        let r = (vp.headers_store.get_timestamp(planted_at).unwrap() - genesis_ts) / 1_000 + 2;
        r + r % 2
    };

    // The lane: r0 carries A; r1 names r0 alone, carries B, and sorts before r0 (its nonce ground).
    // Slots by the hash's top bit, so the child sorts first whatever hashes the chain drew.
    let r0 = (1..4096)
        .map(|nonce| round_block_carrying(&ctx, &config, first_round, &payout, vec![a.clone()], nonce))
        .find(|r0| r0.header.hash.as_bytes()[0] >= 0x80)
        .expect("a nonce in the upper half");
    ctx.consensus.validate_and_insert_block(r0.clone().to_immutable()).virtual_state_task.await.expect("r0 is valid at the header");
    blocks.push(r0.clone().to_immutable());
    let r1 = (1..4096)
        .map(|nonce| round_block_carrying(&ctx, &config, first_round + 2, &payout, vec![b.clone()], nonce))
        .find(|r1| r1.header.hash.as_bytes()[0] < 0x80)
        .expect("a nonce in the lower half");
    assert!(r1.header.hash < r0.header.hash, "the child sorts first");
    assert_eq!(r1.header.direct_parents(), &[r0.header.hash], "r1 extends the lane from r0 alone");
    ctx.consensus.validate_and_insert_block(r1.clone().to_immutable()).virtual_state_task.await.expect("r1 is valid at the header");
    blocks.push(r1.clone().to_immutable());
    let (r0_hash, r1_hash) = (r0.header.hash, r1.header.hash);
    assert_eq!(vp.ghostdag_store.get_blue_work(r0_hash).unwrap(), vp.ghostdag_store.get_blue_work(r1_hash).unwrap(), "a lane ties");
    assert_eq!(ctx.consensus.get_sink(), planted_at, "round blocks never move the sink");

    // The merging block.
    ctx.simulated_time = ctx.simulated_time.max(genesis_ts + (first_round + 2) * 1_000) + config.params.target_time_per_block();
    let merging = ctx.build_block_template(50, ctx.simulated_time).block.to_immutable();
    ctx.validate_and_insert_block(merging.clone()).await.assert_valid_utxo_tip();
    blocks.push(merging.clone());
    let merging_hash = merging.header.hash;
    assert_eq!(ctx.consensus.get_sink(), merging_hash, "the merging block is the sink");
    assert_eq!(ctx.consensus.block_status(merging_hash), BlockStatus::StatusUTXOValid, "template and validation agree");
    let data = vp.ghostdag_store.get_data(merging_hash).unwrap();
    assert!(data.mergeset_reds.contains(&r0_hash) && data.mergeset_reds.contains(&r1_hash), "the lane is merged as reds");
    let consensus_order: Vec<BlockHash> = data.consensus_ordered_mergeset_without_selected_parent(vp.ghostdag_store.deref()).collect();
    let pos = |hash| consensus_order.iter().position(|h| *h == hash).unwrap();
    assert!(pos(r1_hash) < pos(r0_hash), "the consensus order lists the lane's child first (its hash)");
    let acceptance = ctx
        .consensus
        .get_block_acceptance_data(merging_hash)
        .unwrap()
        .iter()
        .map(|m| (m.block_hash, m.accepted_transactions.iter().map(|e| e.transaction_id).collect::<Vec<_>>()))
        .collect();
    let round_payout = merging.transactions[0].outputs.iter().filter(|o| o.script_public_key == payout).map(|o| o.value).sum();
    LaneRun {
        ctx,
        config,
        bundle,
        blocks,
        planted_at,
        lane: [r0_hash, r1_hash],
        txs: [a.id(), b.id()],
        merging,
        acceptance,
        round_payout,
    }
}

/// **Below the fence: today's behaviour, pinned** — the child's chained carrier is skipped (its input
/// is made by the parent, which the consensus order applies after it), the round payout collects `A`'s
/// fee alone. Armed at a height the chain never reaches, the run is the SAME chain, block for block.
#[tokio::test]
async fn below_the_fence_a_chained_carrier_in_a_child_that_sorts_first_is_skipped() {
    let dormant = lane_run(None).await;
    let [r0, r1] = dormant.lane;
    let [a, b] = dormant.txs;
    assert!(dormant.position(r1) < dormant.position(r0), "applied in hash order: the child first");
    assert!(dormant.accepted(a), "A is accepted from r0");
    assert!(!dormant.accepted(b), "B, applied before A exists, is skipped — today's behaviour");
    assert_eq!(dormant.round_payout, FEE_A, "the round payout collects A's fee alone");

    let not_yet = lane_run(Some(ForkActivation::new(1_000_000))).await;
    assert!(!not_yet.config.params.palw_lane_accept_parents_first_active_at(not_yet.merging.header.daa_score));
    assert_eq!(not_yet.blocks.len(), dormant.blocks.len());
    for (armed, released) in not_yet.blocks.iter().zip(dormant.blocks.iter()) {
        assert_eq!(armed.header.hash, released.header.hash, "armed above the chain, the node builds and accepts the same blocks");
    }
    assert_eq!(not_yet.acceptance, dormant.acceptance, "and applies them in the same order");
    assert_eq!(not_yet.round_payout, dormant.round_payout);
}

/// **Past the fence: parents first** — `A` then `B`, both accepted, both fees paid to the round
/// payout; the header the template wrote (UTXO commitment, accepted-id merkle root) is the one
/// validation computes (the block is UTXO-valid), and the rule moved only the lane.
#[tokio::test]
async fn past_the_fence_the_lane_is_applied_parents_first_and_the_chained_carrier_is_accepted() {
    let armed = lane_run(Some(ForkActivation::new(1))).await;
    let [r0, r1] = armed.lane;
    let [a, b] = armed.txs;
    assert!(armed.config.params.palw_lane_accept_parents_first_active_at(armed.merging.header.daa_score));
    assert!(armed.position(r0) < armed.position(r1), "applied parents-first: r0, then r1");
    assert!(armed.accepted(a) && armed.accepted(b), "A and the carrier chained on it are both accepted");
    let row = |block| armed.acceptance.iter().find(|(hash, _)| *hash == block).unwrap().1.clone();
    assert!(row(r0).contains(&a) && row(r1).contains(&b), "each from the round block that carries it");
    assert_eq!(armed.round_payout, FEE_A + FEE_B, "the round payout collects both fees");
    assert_eq!(
        armed.acceptance[0].0,
        armed.ctx.consensus.virtual_processor().ghostdag_store.get_selected_parent(armed.merging.header.hash).unwrap()
    );

    let dormant = lane_run(None).await;
    assert_ne!(
        armed.merging.header.accepted_id_merkle_root, dormant.merging.header.accepted_id_merkle_root,
        "the accepted set differs by B"
    );
    let moved: Vec<_> =
        armed.acceptance.iter().map(|(h, _)| *h).zip(dormant.acceptance.iter().map(|(h, _)| *h)).filter(|(x, y)| x != y).collect();
    assert!(moved.iter().all(|(x, y)| armed.lane.contains(x) && armed.lane.contains(y)), "only the lane moved: {moved:?}");
}

/// **One order on every node, whatever the arrival order**: a second node fed the armed DAG
/// headers-first, then the bodies (an IBD's order, not the relay's), with the same schedule planted at
/// the same block, reaches the same sink UTXO-valid — its UTXO commitment and accepted-id merkle root
/// are the ones the first node's template wrote — and records the same acceptance.
#[tokio::test]
async fn two_nodes_that_receive_the_lane_in_different_orders_accept_it_identically() {
    let armed = lane_run(Some(ForkActivation::new(1))).await;
    let other = TestConsensus::new(&armed.config);
    let handles: Vec<JoinHandle<()>> = other.init();
    let planted = armed.blocks.iter().position(|b| b.header.hash == armed.planted_at).expect("the planting point") + 1;
    for block in armed.blocks[..planted].iter() {
        other.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("the chain below the lane");
    }
    plant_the_schedule(&other, &armed.bundle);
    let rest = &armed.blocks[planted..];
    for block in rest.iter() {
        other.validate_and_insert_block(Block::from_header_arc(block.header.clone())).virtual_state_task.await.expect("headers first");
    }
    for block in rest.iter() {
        other.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("then the bodies");
    }
    let merging = armed.merging.header.hash;
    assert_eq!(other.get_sink(), merging);
    assert_eq!(other.block_status(merging), BlockStatus::StatusUTXOValid, "the second node computes the header's commitments");
    let theirs: Vec<(BlockHash, Vec<TransactionId>)> = other
        .get_block_acceptance_data(merging)
        .unwrap()
        .iter()
        .map(|m| (m.block_hash, m.accepted_transactions.iter().map(|e| e.transaction_id).collect()))
        .collect();
    assert_eq!(theirs, armed.acceptance, "the same acceptance, in the same order");
    other.shutdown(handles);
}

/// **From the fence a round block carries no EVM payload** (keyed on the round block's own DAA score):
/// a round body that commits to a non-empty payload is refused past the fence and admitted below it;
/// an empty payload, and a payload on a block that is not a round block, are untouched.
#[tokio::test]
async fn from_the_fence_a_round_block_may_not_carry_an_evm_payload() {
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::EvmExecutionPayload;
    for fence in [None, Some(ForkActivation::new(1))] {
        let (mut config, bundle) = adr0125_config_funded_for(8);
        config.params.palw_lane_accept_parents_first = fence;
        let mut ctx = TestContext::new(TestConsensus::new(&config));
        for _ in 0..4 {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        let vp = ctx.consensus.virtual_processor().clone();
        let sink = ctx.consensus.get_sink();
        let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
        let payout = p2pkh_mldsa87_spk(state.bond(&adr0125_harness_bond()).expect("row 0").payout_payload.as_byte_slice());
        let round = {
            let r = (vp.headers_store.get_timestamp(sink).unwrap() - config.params.genesis.timestamp) / 1_000 + 2;
            r + r % 2
        };
        let plain = adr0125_round_block(&ctx, &config, round, 0, payout.clone(), 7);
        assert!(plain.evm_payload.is_empty(), "the node's round template carries no EVM payload");
        // The same block as a v2 header committing to a payload (the body rule reads the committed
        // payload; the header stage's version gate is not what is tested here).
        let with_payload = |payload: EvmExecutionPayload, block: &MutableBlock| {
            let mut block = block.clone();
            block.header.version = EVM_HEADER_VERSION;
            block.header = block.header.clone().with_evm_payload_hash(payload.payload_hash());
            block.evm_payload = payload;
            block.header.finalize();
            block.to_immutable()
        };
        let carrying = with_payload(EvmExecutionPayload { extra_data: vec![0xAB], ..Default::default() }, &plain);
        let empty = with_payload(EvmExecutionPayload::default(), &plain);
        let body = ctx.consensus.block_body_processor().clone();
        let armed = config.params.palw_lane_accept_parents_first_active_at(plain.header.daa_score);
        match (armed, body.validate_body_in_isolation(&carrying)) {
            (true, Err(RuleError::RoundBlockCarriesEvmPayload)) => {}
            (false, Ok(_)) => {}
            (armed, other) => panic!("armed={armed}: a round body committing to a payload got {other:?}"),
        }
        body.validate_body_in_isolation(&empty).expect("an empty payload is always admitted");
        let chain_template = ctx.build_block_template(3, ctx.simulated_time + 1_000);
        let chain_carrying = with_payload(EvmExecutionPayload { extra_data: vec![0xAB], ..Default::default() }, &chain_template.block);
        body.validate_body_in_isolation(&chain_carrying).expect("the rule is the round lane's alone");
    }
}

/// **testnet-12 with EVERY post-launch fence at one height, crossed with round lanes on both sides.**
///
/// The release's list (this fence among its thirteen entries) set to `H` through each entry's own
/// `set`, as the release arms it; heartbeats run the clock through `H`, and below it and past it a
/// two-block round lane whose child sorts first is inserted and merged by the next attempt block. Every
/// chain block is UTXO-valid and the sink (the harness asserts it), the clock never stalls, the lane
/// below `H` is applied in hash order and the one past it parents-first — and a second armed node fed
/// the whole DAG, lanes included, reaches the same sink and the same PALW root.
#[tokio::test]
async fn every_post_launch_fence_at_one_height_is_crossed_with_round_lanes_on_both_sides() {
    use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
    use kaspa_consensus_core::palw_execution_lane_v1::{
        PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_CONTEXT, PalwExecEnvelopeV1, palw_exec_signing_message_v1,
        palw_execution_round_v1,
    };
    const H: u64 = 24;
    kaspa_core::log::try_init_logger("warn");
    let armed_config = |config: &Config| {
        let mut params = config.params.clone();
        for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
            (fence.set)(&mut params, Some(ForkActivation::new(H)));
        }
        params.validate_palw_v2().expect("every post-launch fence at one height is a runnable testnet-12 ruleset");
        assert!(params.palw_lane_accept_parents_first_active_at(H) && !params.palw_lane_accept_parents_first_active_at(H - 1));
        kaspa_consensus_core::config::ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let config = armed_config(&config);
    let mut chain: T12Chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let network = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let ttpb = config.params.target_time_per_block();

    // One round block for card `card`'s bond, round `round`, nonce `nonce` (unpermitted: no schedule
    // grants it, so the merging block applies its coinbase alone — the order is what is crossed here).
    let round_block = |chain: &T12Chain, card: usize, round: u64, nonce: u64| -> MutableBlock {
        let template = chain
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        let mut block = chain.vp().round_adapt_block_template(template, round, card_payout_spk(card)).expect("the lane adapts").block;
        let kp = TestConsensus::palw_v2_registry_keypair(card as u64);
        let bond = chain.bonds[card];
        block.header.nonce = nonce;
        block.header.palw_commitment = Vec::new();
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
        let message = palw_exec_signing_message_v1(network, pre_pow, block.header.timestamp, nonce, round, 0, &bond);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), PALW_EXEC_MLDSA87_CONTEXT, [0u8; 32])
                .expect("ML-DSA-87 sign")
                .as_ref()
                .to_vec();
        block.header.palw_commitment = PalwExecEnvelopeV1 {
            version: PALW_EXEC_ENVELOPE_VERSION_V1,
            network_domain: network,
            round,
            permit_index: 0,
            bond,
            pubkey: kp.verification_key.as_ref().to_vec(),
            signature,
        }
        .encode();
        block.header.finalize();
        block
    };
    let mut all: Vec<Block> = Vec::new();
    let mut lanes: Vec<(u64, [BlockHash; 2], BlockHash)> = Vec::new();
    let mut card = 0usize;
    let mut beats_without_a_tick = 0u32;
    loop {
        let daa = chain.daa_of(chain.sink());
        if daa >= H + 6 {
            break;
        }
        if (daa == H - 4 || daa == H + 2) && lanes.iter().all(|(merged_at, ..)| *merged_at != daa) {
            // A lane: a parent, and a child naming it alone that sorts first.
            let round = palw_execution_round_v1(chain.ctx.simulated_time, config.params.genesis.timestamp) + 2;
            // Slots by the hash's top bit, so the child sorts first whatever hashes the chain drew.
            let parent = (1..4096)
                .map(|nonce| round_block(&chain, card % 8, round, nonce))
                .find(|parent| parent.header.hash.as_bytes()[0] >= 0x80)
                .expect("a nonce in the upper half");
            chain
                .ctx
                .consensus
                .validate_and_insert_block(parent.clone().to_immutable())
                .virtual_state_task
                .await
                .expect("a round block");
            let child = (1..4096)
                .map(|nonce| round_block(&chain, card % 8, round + 1, nonce))
                .find(|child| child.header.hash.as_bytes()[0] < 0x80)
                .expect("a nonce in the lower half");
            assert!(child.header.hash < parent.header.hash, "the child sorts first");
            assert_eq!(child.header.direct_parents(), &[parent.header.hash]);
            chain
                .ctx
                .consensus
                .validate_and_insert_block(child.clone().to_immutable())
                .virtual_state_task
                .await
                .expect("a round block");
            all.push(parent.clone().to_immutable());
            all.push(child.clone().to_immutable());
            chain.ctx.simulated_time = chain.ctx.simulated_time.max(config.params.genesis.timestamp + (round + 2) * 1_000);
            let (merging, _) = chain.attempt(card % 8, ttpb, Vec::new(), &|_| true).await;
            card += 1;
            let data = chain.vp().ghostdag_store.get_data(merging.header.hash).unwrap();
            assert!(data.mergeset_reds.contains(&parent.header.hash) && data.mergeset_reds.contains(&child.header.hash));
            lanes.push((merging.header.daa_score, [parent.header.hash, child.header.hash], merging.header.hash));
            all.push(merging);
        }
        let before = chain.daa_of(chain.sink());
        let beat = chain.heartbeat(ttpb, Vec::new()).await;
        if beat.header.daa_score == before {
            beats_without_a_tick += 1;
            assert!(beats_without_a_tick < 4, "the DAA clock stalled at {before} (fence {H})");
        } else {
            beats_without_a_tick = 0;
        }
        all.push(beat);
    }
    assert_eq!(lanes.len(), 2, "a lane below the fence and one past it");
    for (daa, [parent, child], merging) in lanes.iter() {
        let order: Vec<BlockHash> =
            chain.ctx.consensus.get_block_acceptance_data(*merging).unwrap().iter().map(|m| m.block_hash).collect();
        let (p, c) = (order.iter().position(|h| h == parent).unwrap(), order.iter().position(|h| h == child).unwrap());
        if *daa < H {
            assert!(c < p, "merged at DAA {daa}, below the fence: hash order, the child first");
        } else {
            assert!(p < c, "merged at DAA {daa}, past the fence: parents first");
        }
    }

    // A second armed node, fed the whole DAG — lanes included — in insertion order.
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in all.iter() {
        follower.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.unwrap_or_else(|e| {
            panic!("block {} (DAA {}) refused by a second armed node: {e}", block.header.hash, block.header.daa_score)
        });
    }
    assert_eq!(follower.sink(), chain.sink(), "the second node walks the same chain");
    assert_eq!(follower.tip_state().1.state_root(), chain.tip_state().1.state_root(), "and folds the same PALW root");
    assert_eq!(follower.ctx.consensus.block_status(follower.sink()), BlockStatus::StatusUTXOValid);
}
