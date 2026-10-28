//! **Lane sink at the processor: a testnet-12 chain crossing the model sink binding**
//! (`Params::palw_model_sink_bound`, the 2026-09-25 Position review's #1, post-launch fence).
//!
//! Two nodes on one genesis — testnet-12 as released (the fence `None`) and the same ruleset with the
//! fence armed at [`FENCE`] — are fed the SAME blocks, built by the released node's own template and
//! the heartbeat lane testnet-12's clock runs on:
//!
//! * **below the fence** every block is valid on both, UTXO-valid on both, and both reach the same
//!   sink and the same PALW state root — including the block at DAA `FENCE - 1` that carries a bare
//!   transfer into a model sink, the burn the fence closes (its sink UTXO is in both UTXO sets);
//! * **at the fence** a bound `ModelBuy` carrier (change at output 0, the sink at 1 — the shape
//!   `misaka palw model-buy` builds) is valid on both, and its MSK is recorded: the fold refuses the
//!   buy (no market is open on the line) and writes the P-B1 refund row to the payer;
//! * **past the fence** a block carrying an unbound sink is REFUSED by the armed node with the header
//!   context's own error (`TxInContextFailed(_, ModelSinkUnbound)`), while the released node accepts
//!   the very same block — the flag day, not a silent fork: the fork id names the height
//!   (`palw_model_sink_bound_is_t12_only`); and the armed node's own template refuses the carrier
//!   before it is ever mined.
//!
//! The chain is `t12_round_lane_e2e`'s harness (testnet-12 as shipped, harness keys on its eight
//! cards, the premine imported, the EVM lane inert); the carriers spend the cards' fee floats. The
//! blocks are heartbeats with the carrier appended after the template was adapted (and the merkle
//! root recomputed): that is a block any miner can publish, and it is judged by consensus alone —
//! not by this node's template gates, which would keep a buy on a held class out of its own template.
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_model_market_v1::palw_model_sink_spk_v1;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2 as Obj, palw_model_refund_payout_key_v1};
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;

/// The fence's height in this chain: low, so the chain crosses it in a minute of heartbeats (the
/// operator arms testnet-12 at the common post-launch height, 500; the rule is the height's, not the
/// number's — `a_model_sink_is_valid_only_bound_from_the_sink_fence` asks both at the door).
const FENCE: u64 = 60;
/// What each carrier pays into the sink, and its fee — out of a card's 100 MSK fee float.
const PAID: u64 = 1_000_000_000;
const FEE: u64 = 300_000;

/// A heartbeat on `chain`'s tip, built by its own template and adapted into the lane, NOT inserted.
fn heartbeat_block(chain: &mut T12Chain, nonce: u64) -> MutableBlock {
    chain.ctx.simulated_time += chain.config.params.target_time_per_block();
    let mut t = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&chain.config.params, &mut t.block.header, chain.ctx.simulated_time);
    t.block.header.nonce = nonce;
    t.block.header.finalize();
    let (t, _) = chain.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
    chain.ctx.simulated_time = chain.ctx.simulated_time.max(t.block.header.timestamp);
    t.block
}

/// `block` carrying `txs` after its coinbase, its merkle root recomputed — a block any miner can
/// publish, judged by consensus alone.
fn carrying(mut block: MutableBlock, txs: Vec<Transaction>) -> Block {
    block.transactions.extend(txs);
    block.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(block.transactions.iter());
    block.header.finalize();
    block.to_immutable()
}

/// Plain beats on `released`, fed to both nodes, until the next beat's DAA score reaches `daa`; that
/// beat is returned NOT inserted. A beat earns the chain a DAA only at the clock's next slot, so the
/// score does not move on every beat.
async fn beat_to(released: &mut T12Chain, armed: &T12Chain, nonce: &mut u64, daa: u64) -> MutableBlock {
    loop {
        *nonce += 1;
        let beat = heartbeat_block(released, *nonce);
        if beat.header.daa_score >= daa {
            return beat;
        }
        both_accept(released, armed, &beat.to_immutable(), "a plain beat").await;
    }
}

/// Insert `block` into `chain` and return the node's verdict (the virtual-state task's).
async fn insert(chain: &T12Chain, block: &Block) -> Result<BlockStatus, RuleError> {
    chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await
}

/// Insert `block` into both nodes and demand the same answer: valid, UTXO-valid, the sink, and the
/// same PALW state root.
async fn both_accept(released: &T12Chain, armed: &T12Chain, block: &Block, what: &str) {
    for (name, chain) in [("released", released), ("armed", armed)] {
        insert(chain, block).await.unwrap_or_else(|e| panic!("{what}: the {name} node refused {}: {e}", block.header.hash));
        assert_eq!(chain.ctx.consensus.block_status(block.header.hash), BlockStatus::StatusUTXOValid, "{what}: {name} UTXO-valid");
        assert_eq!(chain.sink(), block.header.hash, "{what}: {name} sink");
    }
    assert_eq!(
        released.tip_state().1.state_root(),
        armed.tip_state().1.state_root(),
        "{what}: the released rule and the armed one hold one PALW state"
    );
}

/// A card's fee float spent into `outputs` beyond the change at output 0, signed by the card.
fn spend_float(
    float: &(TransactionOutpoint, UtxoEntry),
    card: usize,
    subnetwork: kaspa_consensus_core::subnets::SubnetworkId,
    payload: Vec<u8>,
    rest: Vec<TransactionOutput>,
    storage_mass_parameter: u64,
) -> Transaction {
    let (outpoint, entry) = float.clone();
    let paid: u64 = rest.iter().map(|o| o.value).sum();
    let mut outputs = vec![TransactionOutput::new(entry.amount - paid - FEE, card_payout_spk(card))];
    outputs.extend(rest);
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        outputs,
        0,
        subnetwork,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, card, storage_mass_parameter);
    tx
}

#[tokio::test]
async fn t12_a_model_sink_is_bound_from_the_fence_and_the_released_rule_holds_below_it() {
    kaspa_core::log::try_init_logger("info");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_model_sink_bound, None, "testnet-12 ships the fence dormant");
    assert!(config.params.palw_model_market_active_at(0), "testnet-12's market is declared from genesis");
    let armed_config = {
        let mut params = config.params.clone();
        params.palw_model_sink_bound = Some(ForkActivation::new(FENCE));
        let armed = ConfigBuilder::new(params).skip_proof_of_work().build();
        armed.params.validate_palw_v2().expect("testnet-12 armed at a post-launch height is a runnable ruleset");
        assert!(!armed.params.palw_model_sink_bound_active_at(FENCE - 1) && armed.params.palw_model_sink_bound_active_at(FENCE));
        armed
    };
    assert_eq!(armed_config.params.genesis.hash, config.params.genesis.hash, "one genesis: the fence is not a regenesis");
    let mut released = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut armed = t12_genesis_chain(&armed_config, &bundle, &premine, &floats);
    let storage_mass_parameter = config.params.storage_mass_parameter;
    let line: Hash64 = {
        let (_, state) = released.tip_state();
        let line = *state.classes_iter().map(|(id, _)| id).next().expect("testnet-12 registers a class");
        assert!(state.model_market(&line).is_none(), "no market is open on the line");
        line
    };

    // ---- below the fence: the released rule, on both nodes, block for block ---------------------
    // The burn the fence closes: a bare transfer of PAID into the line's sink, no object anywhere.
    let bare = spend_float(
        &floats[0],
        0,
        SUBNETWORK_ID_NATIVE,
        Vec::new(),
        vec![TransactionOutput::new(PAID, palw_model_sink_spk_v1(&line))],
        storage_mass_parameter,
    );
    let mut nonce = 0u64;
    let beat = beat_to(&mut released, &armed, &mut nonce, FENCE - 1).await;
    assert_eq!(beat.header.daa_score, FENCE - 1, "the last block below the fence");
    let below = carrying(beat, vec![bare.clone()]);
    both_accept(&released, &armed, &below, "the unbound sink in the last block below the fence").await;
    assert!(below.transactions.iter().any(|tx| tx.id() == bare.id()), "the bare burn rides the block below the fence");

    // ---- at the fence: a bound buy is valid on both, and its MSK is recorded ---------------------
    let buy = Obj::ModelBuy { line_id: line, holder: Hash64::from_u64_word(0xB0B), msk_in: PAID, min_units_out: 0, sink_index: 1 };
    let bound = spend_float(
        &floats[1],
        1,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: buy }).unwrap(),
        vec![TransactionOutput::new(PAID, palw_model_sink_spk_v1(&line))],
        storage_mass_parameter,
    );
    assert_eq!(kaspa_consensus_core::palw_model_market_v1::palw_model_sink_binding_refusal_v1(&bound), None, "the carrier is bound");
    let at = carrying(beat_to(&mut released, &armed, &mut nonce, FENCE).await, vec![bound.clone()]);
    assert_eq!(at.header.daa_score, FENCE, "the block at the fence");
    both_accept(&released, &armed, &at, "the bound buy at the fence").await;
    // The block that accepts both carriers (past the fence: acceptance does not re-ask a carrying
    // block's header-context rules — the bare burn below the fence stays the released rule's).
    nonce += 1;
    let accepting = heartbeat_block(&mut released, nonce).to_immutable();
    both_accept(&released, &armed, &accepting, "the block accepting both carriers").await;
    for (name, chain) in [("released", &released), ("armed", &armed)] {
        let utxos: std::collections::HashMap<_, _> =
            chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
        let sink_of = |tx: &Transaction| utxos.get(&TransactionOutpoint::new(tx.id(), 1)).map(|e| e.amount);
        assert_eq!(sink_of(&bare), Some(PAID), "{name}: the bare burn below the fence is in the UTXO set, dead by script");
        assert_eq!(sink_of(&bound), Some(PAID), "{name}: the bound buy's sink holds its MSK");
        let (_, state) = chain.tip_state();
        let refund =
            state.pending_payouts_iter().find(|(k, _)| **k == palw_model_refund_payout_key_v1(&bound.id())).map(|(_, row)| row.amount);
        assert_eq!(refund, Some(PAID), "{name}: the bound buy the fold refused is recorded — its P-B1 refund row pays it back");
        assert!(
            !state.pending_payouts_iter().any(|(k, _)| *k == palw_model_refund_payout_key_v1(&bare.id())),
            "{name}: the bare burn is recorded nowhere — the hole, below the fence"
        );
    }

    // ---- past the fence: the armed node refuses an unbound sink; the released node does not -----
    let unbound = spend_float(
        &floats[2],
        2,
        SUBNETWORK_ID_NATIVE,
        Vec::new(),
        vec![TransactionOutput::new(PAID, palw_model_sink_spk_v1(&line))],
        storage_mass_parameter,
    );
    // The armed node's own template refuses it before it is ever mined.
    match armed.ctx.consensus.build_block_template(
        new_miner_data(),
        Box::new(OnetimeTxSelector::new(vec![unbound.clone()])),
        TemplateBuildMode::Standard,
    ) {
        Err(e) => {
            let e = format!("{e:?}");
            assert!(e.contains("ModelSinkUnbound"), "the armed template names the unbound sink: {e}");
        }
        Ok(t) => panic!("past the fence the armed node must not template an unbound sink (DAA {})", t.block.header.daa_score),
    }
    nonce += 1;
    let past = carrying(heartbeat_block(&mut released, nonce), vec![unbound.clone()]);
    assert!(past.header.daa_score >= FENCE, "a block at or past the fence");
    match insert(&armed, &past).await {
        Err(RuleError::TxInContextFailed(id, TxRuleError::ModelSinkUnbound(1, why))) => {
            assert_eq!(id, unbound.id());
            assert!(why.contains("rides only a lifecycle carrier"), "{why}");
        }
        other => panic!("past the fence the armed node refuses the block carrying an unbound sink, got {other:?}"),
    }
    assert_eq!(armed.sink(), accepting.header.hash, "the armed node's tip stays where it was");
    insert(&released, &past).await.expect("the released rule accepts the very same block");
    assert_eq!(released.ctx.consensus.block_status(past.header.hash), BlockStatus::StatusUTXOValid);
    // The armed node is live past its fence: a beat of its own merges nothing unbound.
    armed.ctx.simulated_time = released.ctx.simulated_time;
    nonce += 1;
    let next = heartbeat_block(&mut armed, nonce).to_immutable();
    insert(&armed, &next).await.expect("the armed chain goes on past its fence");
    assert_eq!(armed.sink(), next.header.hash);
}
