//! The simulation actors. Each owns one [`SimNode`] and reacts only to simulator events — no actor
//! reads the wall clock, and no actor polls on a timeout it does not need, because the simulation
//! ends when the event queue drains.

use std::sync::Arc;

use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, TemplateBuildMode, TemplateTransactionSelector};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutpoint};
use kaspa_utils::sim::{Environment, Process, Resumption, Suspension};

use super::node::SimNode;
use super::{BLOCK_INTERVAL_MS, BOND_BURIAL_BLOCKS, COINBASE_MATURITY, FUNDING_BLOCKS, NUM_BLOCKS, SimMsg};
use crate::config::Config;
use crate::model::stores::headers::HeaderStoreReader;
use crate::pipeline::virtual_processor::tests::dns_harness;

/// Sompi left on the funding coinbase as the bond carrier's fee (the e2e harness's value).
const BOND_CARRIER_FEE: u64 = 100_000;

/// Selector handing the block builder one fixed tx set. A rejection is recorded so
/// `is_successful` reports failure and `build_block_template` surfaces the `RuleError` instead of
/// silently dropping the tx (the harness then defers it to a later block).
struct OnetimeTxSelector {
    txs: Option<Vec<Transaction>>,
    rejected: bool,
}

impl OnetimeTxSelector {
    fn new(txs: Vec<Transaction>) -> Self {
        Self { txs: Some(txs), rejected: false }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.txs.take().unwrap_or_default()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        self.rejected = true;
    }

    fn is_successful(&self) -> bool {
        !self.rejected
    }
}

/// The ML-DSA-87 P2PKH the harness validator is funded on and paid back to — the same script
/// `funded_signed_bond_tx` derives internally from `seed`, so the funding coinbases it consumes must
/// pay exactly this one.
pub(super) fn validator_spk(seed: [u8; 32]) -> ScriptPublicKey {
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed);
    let payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes();
    p2pkh_mldsa87_spk(&payload)
}

/// The sole block producer. Mines on a fixed virtual-time cadence, broadcasts every block (hearing
/// its own instantly through the topology) and folds any tx it was sent into the next template.
///
/// It never returns `Suspension::Halt`: halting tears the simulation down with events still queued,
/// which would leave the other nodes short of blocks. Instead it goes `Idle` after its quota, and
/// the run ends when the queue drains naturally.
pub(super) struct MinerActor {
    node: Arc<SimNode>,
    /// Coinbases of the first `FUNDING_BLOCKS` blocks pay the validator.
    validator_spk: ScriptPublicKey,
    produced: u64,
    pending: Vec<Transaction>,
    seen: BlockHashSet,
}

impl MinerActor {
    pub(super) fn new(node: Arc<SimNode>, validator_seed: [u8; 32]) -> Self {
        Self { node, validator_spk: validator_spk(validator_seed), produced: 0, pending: Vec::new(), seen: BlockHashSet::default() }
    }

    fn miner_data(&self) -> MinerData {
        // A block's coinbase pays the miners of the blocks it MERGES, so paying the validator for
        // the first `FUNDING_BLOCKS` blocks yields validator-owned coinbase outputs in blocks
        // 2..=FUNDING_BLOCKS+1. Later blocks pay a throwaway key.
        if self.produced < FUNDING_BLOCKS {
            MinerData::new(self.validator_spk.clone(), vec![])
        } else {
            MinerData::new(p2pkh_mldsa87_spk(&[0x07; 64]), vec![])
        }
    }

    /// Builds the next block at virtual time `now`. The template's timestamp (which the builder
    /// stamps from `unix_now()`) is overwritten with simulated time and the header re-finalized, so
    /// the block hash is a function of the simulated content alone.
    fn build_block(&mut self, now: u64) -> Block {
        let miner_data = self.miner_data();
        let session = self.node.consensus.acquire_session();
        let template = match self.node.consensus.build_block_template(
            miner_data.clone(),
            Box::new(OnetimeTxSelector::new(self.pending.clone())),
            TemplateBuildMode::Standard,
        ) {
            Ok(template) => {
                self.pending.clear();
                template
            }
            // A pending tx does not fit this template yet (e.g. its funding is not mature). Keep it
            // and mine an empty block; it gets another chance next round.
            Err(_) => self
                .node
                .consensus
                .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
                .expect("an empty block template is always buildable"),
        };
        drop(session);
        let mut block = template.block;
        block.header.timestamp = now;
        block.header.finalize();
        block.to_immutable()
    }
}

impl Process<SimMsg> for MinerActor {
    fn resume(&mut self, resumption: Resumption<SimMsg>, env: &mut Environment<SimMsg>) -> Suspension {
        match resumption {
            Resumption::Initial => Suspension::Timeout(BLOCK_INTERVAL_MS),
            Resumption::Scheduled => {
                if self.produced >= NUM_BLOCKS {
                    return Suspension::Idle;
                }
                let block = self.build_block(env.now());
                self.produced += 1;
                env.broadcast(super::MINER_ID, SimMsg::Block(block));
                Suspension::Timeout(BLOCK_INTERVAL_MS)
            }
            Resumption::Message(SimMsg::Block(block)) => {
                self.node.insert_if_new(block, &mut self.seen);
                Suspension::Idle
            }
            Resumption::Message(SimMsg::Tx(tx)) => {
                self.pending.push(tx);
                Suspension::Idle
            }
        }
    }
}

/// A passive node: it only validates what it hears. Its convergence with the miner is the
/// harness's cheapest end-to-end check.
pub(super) struct ObserverActor {
    node: Arc<SimNode>,
    seen: BlockHashSet,
}

impl ObserverActor {
    pub(super) fn new(node: Arc<SimNode>) -> Self {
        Self { node, seen: BlockHashSet::default() }
    }
}

impl Process<SimMsg> for ObserverActor {
    fn resume(&mut self, resumption: Resumption<SimMsg>, _env: &mut Environment<SimMsg>) -> Suspension {
        if let Resumption::Message(SimMsg::Block(block)) = resumption {
            self.node.insert_if_new(block, &mut self.seen);
        }
        Suspension::Idle
    }
}

/// A coinbase output the validator owns: `(outpoint, value, the DAA score of the paying block)`.
type Funding = (TransactionOutpoint, u64, u64);

/// Where the validator is in the DNS overlay lifecycle. It advances at most one step per block it
/// validates — the harness is purely event driven, since a timeout-polling actor would keep the
/// event queue alive forever.
enum ValidatorPhase {
    /// Waiting for two owned coinbase outputs (one funds the bond, one funds the shard) and for the
    /// bond's funding to mature.
    Funding,
    /// Bond tx sent to the miner; waiting to see it accepted into a block.
    BondSent { bond_outpoint: TransactionOutpoint },
    /// Bond accepted at `accepted_daa`; letting blue-score epochs bury it before attesting.
    BondBurying { bond_outpoint: TransactionOutpoint, remaining: u64 },
    /// Attestation shard sent — nothing left to do.
    Done,
}

impl ValidatorPhase {
    /// Diagnostic name — a stalled lifecycle (e.g. a partition eating the bond tx) is diagnosed
    /// from the phase-transition trace, not from the eventual assert failure.
    fn name(&self) -> &'static str {
        match self {
            ValidatorPhase::Funding => "Funding",
            ValidatorPhase::BondSent { .. } => "BondSent",
            ValidatorPhase::BondBurying { .. } => "BondBurying",
            ValidatorPhase::Done => "Done",
        }
    }
}

/// The DNS validator: it validates every block it hears and, on top of that, drives one full
/// overlay lifecycle — bond the stake, wait for it to bury, then attest the canonical anchor of a
/// ready blue-score epoch. Both txs are unicast to the miner, which is the only block producer.
pub(super) struct ValidatorActor {
    node: Arc<SimNode>,
    config: Arc<Config>,
    seed: [u8; 32],
    spk: ScriptPublicKey,
    funding: Vec<Funding>,
    phase: ValidatorPhase,
    seen: BlockHashSet,
}

impl ValidatorActor {
    pub(super) fn new(node: Arc<SimNode>, config: Arc<Config>, seed: [u8; 32]) -> Self {
        Self {
            node,
            config,
            seed,
            spk: validator_spk(seed),
            funding: Vec::new(),
            phase: ValidatorPhase::Funding,
            seen: BlockHashSet::default(),
        }
    }

    /// Harvests the block's coinbase for an output paying this validator.
    fn harvest(&mut self, block: &Block) {
        if self.funding.len() >= 2 {
            return;
        }
        let Some(coinbase) = block.transactions.first() else { return };
        if let Some((index, output)) = coinbase.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == self.spk) {
            self.funding.push((TransactionOutpoint::new(coinbase.id(), index as u32), output.value, block.header.daa_score));
        }
    }

    fn sink_daa(&self) -> u64 {
        let sink = self.node.sink();
        self.node.consensus.storage.headers_store.get_daa_score(sink).expect("the sink has a header")
    }

    /// Builds the funded, ML-DSA-87-signed stake bond and hands it to the miner.
    fn send_bond(&mut self, env: &mut Environment<SimMsg>) -> ValidatorPhase {
        let (outpoint, value, daa) = self.funding[0];
        let (bond_tx, _validator_id, _reward_payload) = dns_harness::funded_signed_bond_tx(
            self.seed,
            outpoint,
            value,
            daa,
            value - BOND_CARRIER_FEE,
            0,
            self.config.params.storage_mass_parameter,
        );
        let bond_outpoint = TransactionOutpoint::new(bond_tx.id(), 0);
        env.unicast(super::VALIDATOR_ID, super::MINER_ID, SimMsg::Tx(bond_tx));
        ValidatorPhase::BondSent { bond_outpoint }
    }

    /// Signs the canonical anchor of the latest ready epoch and hands the carrying shard tx to the
    /// miner. Reads the anchor from this node's OWN consensus — the point of the multi-node setup is
    /// that the validator sees the same canonical anchor the miner will validate against.
    fn send_attestation(&mut self, bond_outpoint: TransactionOutpoint, env: &mut Environment<SimMsg>) -> ValidatorPhase {
        use kaspa_consensus_core::Hash64;
        use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;

        let dns = self.config.params.dns_params.clone().expect("the harness params carry DNS params");
        let sink = self.node.sink();
        let vp = &self.node.consensus.virtual_processor;
        let sink_blue = self.node.consensus.storage.headers_store.get_blue_score(sink).expect("the sink has a header");
        let ready =
            ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                .expect("a DNS attestation epoch is ready by now");
        let anchor = vp.canonical_anchor_by_blue_score(ready, sink, &dns).expect("canonical anchor for the ready epoch");

        let validator = dns_harness::harness_validator(self.seed);
        let attestation = dns_harness::build_signed_attestation(
            &validator,
            self.config.genesis.hash.as_byte_slice(),
            bond_outpoint,
            anchor.epoch,
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            Hash64::default(),
        );
        let (outpoint, value, daa) = self.funding[1];
        let shard_tx = dns_harness::funded_signed_shard_tx(
            self.seed,
            outpoint,
            value,
            daa,
            attestation,
            self.config.params.storage_mass_parameter,
        );
        env.unicast(super::VALIDATOR_ID, super::MINER_ID, SimMsg::Tx(shard_tx));
        ValidatorPhase::Done
    }

    /// Advances the lifecycle by at most one step, after the block has been validated.
    fn step(&mut self, block: &Block, env: &mut Environment<SimMsg>) {
        let before = self.phase.name();
        self.phase = match std::mem::replace(&mut self.phase, ValidatorPhase::Done) {
            ValidatorPhase::Funding => {
                // The bond may only spend a MATURED coinbase; +2 of slack keeps the tx valid for the
                // block the miner actually builds after the round trip.
                if self.funding.len() >= 2 && self.sink_daa() >= self.funding[0].2 + COINBASE_MATURITY + 2 {
                    self.send_bond(env)
                } else {
                    ValidatorPhase::Funding
                }
            }
            ValidatorPhase::BondSent { bond_outpoint } => {
                if block.transactions.iter().any(|tx| tx.id() == bond_outpoint.transaction_id) {
                    ValidatorPhase::BondBurying { bond_outpoint, remaining: BOND_BURIAL_BLOCKS }
                } else {
                    ValidatorPhase::BondSent { bond_outpoint }
                }
            }
            ValidatorPhase::BondBurying { bond_outpoint, remaining } => {
                if remaining > 0 {
                    ValidatorPhase::BondBurying { bond_outpoint, remaining: remaining - 1 }
                } else {
                    self.send_attestation(bond_outpoint, env)
                }
            }
            ValidatorPhase::Done => ValidatorPhase::Done,
        };
        if before != self.phase.name() {
            kaspa_core::trace!("[sim-harness] validator phase: {} -> {} (virtual time {})", before, self.phase.name(), env.now());
        }
    }
}

impl Process<SimMsg> for ValidatorActor {
    fn resume(&mut self, resumption: Resumption<SimMsg>, env: &mut Environment<SimMsg>) -> Suspension {
        if let Resumption::Message(SimMsg::Block(block)) = resumption {
            if self.seen.insert(block.header.hash) {
                self.node.insert(block.clone());
                self.harvest(&block);
                self.step(&block, env);
            }
        }
        Suspension::Idle
    }
}
