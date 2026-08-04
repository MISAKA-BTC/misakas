//! The simulation actors. Each owns one [`SimNode`] and reacts only to simulator events — no actor
//! reads the wall clock, and no actor polls on a timeout it does not need, because the simulation
//! ends when the event queue drains.

use std::sync::{Arc, Mutex};

use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, TemplateBuildMode, TemplateTransactionSelector};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutpoint};
use kaspa_utils::sim::{Environment, Process, Resumption, Suspension};

use super::node::SimNode;
use super::{BOND_BURIAL_BLOCKS, COINBASE_MATURITY, SimMsg};
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

/// Who a miner's coinbases pay.
#[derive(Clone, Copy)]
pub(super) enum Payout {
    /// The first `n` blocks pay the harness validator, the rest a throwaway key.
    ValidatorFirstN(u64),
    /// Every block pays the harness validator — the validator needs a fresh coinbase per
    /// attestation shard, so a long-running validator must be paid continuously.
    ValidatorAlways,
    /// Never pays the validator (a miner on a branch with no validator on it).
    Never,
}

/// Everything that distinguishes one miner from another. Kept as data so a scenario is a
/// declarative timeline rather than a family of actor types.
pub(super) struct MinerCfg {
    /// Simulator process id — the `sender` of this miner's broadcasts, i.e. what the topology
    /// routes (and partitions) on. Getting this wrong silently disables the partition cut.
    pub(super) id: u64,
    /// Virtual ms from simulation start until the first mining attempt.
    pub(super) start_delay_ms: u64,
    /// Virtual ms between mining attempts.
    pub(super) interval_ms: u64,
    /// ABSOLUTE virtual time at which mining stops (the simulation clock starts at the genesis
    /// timestamp, so this is `genesis_ts + offset`, not an offset). `u64::MAX` means "no bound".
    pub(super) mine_end: u64,
    /// Cap on produced blocks. `u64::MAX` means "no bound" (`mine_end` is then the bound).
    pub(super) max_blocks: u64,
    pub(super) payout: Payout,
    /// If set, the first mining attempt asserts this node already has a confirmed DNS anchor — the
    /// deterministic guard for scenarios that must fork a chain only AFTER an anchor confirmed on
    /// the shared prefix. It fails loudly at the cause instead of silently at the outcome.
    pub(super) assert_confirmed_on_start: bool,
}

/// A block producer. Mines on a fixed virtual-time cadence, broadcasts every block (hearing its own
/// instantly through the topology) and folds any tx it was sent into the next template.
///
/// It never returns `Suspension::Halt`: halting tears the simulation down with events still queued,
/// which would leave the other nodes short of blocks. Instead it goes `Idle` after its quota, and
/// the run ends when the queue drains naturally.
pub(super) struct MinerActor {
    node: Arc<SimNode>,
    cfg: MinerCfg,
    /// The script this miner's coinbases pay when `payout` says so.
    validator_spk: ScriptPublicKey,
    produced: u64,
    started: bool,
    pending: Vec<Transaction>,
    seen: BlockHashSet,
}

impl MinerActor {
    pub(super) fn new(node: Arc<SimNode>, validator_seed: [u8; 32], cfg: MinerCfg) -> Self {
        Self {
            node,
            cfg,
            validator_spk: validator_spk(validator_seed),
            produced: 0,
            started: false,
            pending: Vec::new(),
            seen: BlockHashSet::default(),
        }
    }

    fn miner_data(&self) -> MinerData {
        // A block's coinbase pays the miners of the blocks it MERGES, so paying the validator for
        // the first `n` blocks yields validator-owned coinbase outputs in blocks 2..=n+1.
        let pays_validator = match self.cfg.payout {
            Payout::ValidatorFirstN(n) => self.produced < n,
            Payout::ValidatorAlways => true,
            Payout::Never => false,
        };
        if pays_validator {
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
            Resumption::Initial => Suspension::Timeout(self.cfg.start_delay_ms),
            Resumption::Scheduled => {
                if !std::mem::replace(&mut self.started, true) && self.cfg.assert_confirmed_on_start {
                    assert_ne!(
                        self.node.dns_state().last_dns_confirmed_anchor,
                        kaspa_consensus_core::BlockHash::default(),
                        "miner {} must start on a prefix that already carries a confirmed DNS anchor (virtual time {})",
                        self.cfg.id,
                        env.now()
                    );
                }
                if env.now() >= self.cfg.mine_end || self.produced >= self.cfg.max_blocks {
                    return Suspension::Idle;
                }
                let block = self.build_block(env.now());
                self.produced += 1;
                env.broadcast(self.cfg.id, SimMsg::Block(block));
                Suspension::Timeout(self.cfg.interval_ms)
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

/// A read-only probe: at one scheduled virtual instant it samples a node's DNS-confirmed anchor DAA
/// score into a shared cell. It sends nothing, so it cannot perturb the run — it exists so an assert
/// can compare a mid-run observation (e.g. "at heal time") against the end-of-run state.
pub(super) struct AnchorProbeActor {
    node: Arc<SimNode>,
    /// Virtual ms from simulation start until the sample is taken.
    at_delay_ms: u64,
    sample: Arc<Mutex<Option<u64>>>,
}

impl AnchorProbeActor {
    pub(super) fn new(node: Arc<SimNode>, at_delay_ms: u64, sample: Arc<Mutex<Option<u64>>>) -> Self {
        Self { node, at_delay_ms, sample }
    }
}

impl Process<SimMsg> for AnchorProbeActor {
    fn resume(&mut self, resumption: Resumption<SimMsg>, _env: &mut Environment<SimMsg>) -> Suspension {
        match resumption {
            Resumption::Initial => Suspension::Timeout(self.at_delay_ms),
            Resumption::Scheduled => {
                *self.sample.lock().unwrap() = Some(self.node.dns_state().last_dns_confirmed_anchor_daa_score);
                Suspension::Idle
            }
            Resumption::Message(_) => Suspension::Idle,
        }
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
    /// The first attestation shard was sent. From here the validator keeps attesting: every time
    /// its own sink makes a NEW blue-score epoch ready it signs that epoch's canonical anchor once
    /// (and only once — a repeat attestation for an already-attested epoch would burn a funding
    /// output and trip the reward-uniqueness rule).
    Attesting { bond_outpoint: TransactionOutpoint, last_attested_epoch: u64 },
}

impl ValidatorPhase {
    /// Diagnostic name — a stalled lifecycle (e.g. a partition eating the bond tx) is diagnosed
    /// from the phase-transition trace, not from the eventual assert failure.
    fn name(&self) -> &'static str {
        match self {
            ValidatorPhase::Funding => "Funding",
            ValidatorPhase::BondSent { .. } => "BondSent",
            ValidatorPhase::BondBurying { .. } => "BondBurying",
            ValidatorPhase::Attesting { .. } => "Attesting",
        }
    }
}

/// Wiring of a validator into a scenario.
pub(super) struct ValidatorCfg {
    /// Simulator process id — the `sender` the topology routes this validator's txs on.
    pub(super) id: u64,
    /// The miner this validator unicasts its bond and shard txs to (its own branch's producer).
    pub(super) miner_id: u64,
    /// How many owned coinbase outputs to harvest over the whole run. The smoke scenario caps this
    /// at 2 (bond + one shard) so it keeps performing exactly one attestation; a long-running
    /// validator passes `usize::MAX` and consumes funding FIFO, one output per attestation.
    pub(super) harvest_cap: usize,
}

/// The DNS validator: it validates every block it hears and, on top of that, drives the overlay
/// lifecycle — bond the stake, wait for it to bury, then attest the canonical anchor of every ready
/// blue-score epoch. All txs are unicast to `cfg.miner_id`.
pub(super) struct ValidatorActor {
    node: Arc<SimNode>,
    config: Arc<Config>,
    cfg: ValidatorCfg,
    seed: [u8; 32],
    spk: ScriptPublicKey,
    /// Owned coinbase outputs, oldest first — consumed FIFO (bond, then one per attestation).
    funding: Vec<Funding>,
    /// How many outputs were harvested over the whole run (NOT `funding.len()`, which shrinks as
    /// outputs are spent) — this is what `harvest_cap` bounds.
    harvested: usize,
    phase: ValidatorPhase,
    seen: BlockHashSet,
}

impl ValidatorActor {
    pub(super) fn new(node: Arc<SimNode>, config: Arc<Config>, seed: [u8; 32], cfg: ValidatorCfg) -> Self {
        Self {
            node,
            config,
            cfg,
            seed,
            spk: validator_spk(seed),
            funding: Vec::new(),
            harvested: 0,
            phase: ValidatorPhase::Funding,
            seen: BlockHashSet::default(),
        }
    }

    /// Harvests the block's coinbase for an output paying this validator.
    fn harvest(&mut self, block: &Block) {
        if self.harvested >= self.cfg.harvest_cap {
            return;
        }
        let Some(coinbase) = block.transactions.first() else { return };
        if let Some((index, output)) = coinbase.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == self.spk) {
            self.funding.push((TransactionOutpoint::new(coinbase.id(), index as u32), output.value, block.header.daa_score));
            self.harvested += 1;
        }
    }

    fn sink_daa(&self) -> u64 {
        let sink = self.node.sink();
        self.node.consensus.storage.headers_store.get_daa_score(sink).expect("the sink has a header")
    }

    /// Whether the oldest unspent funding output is spendable: a tx may only spend a MATURED
    /// coinbase, and +2 of slack keeps the tx valid for the block the miner actually builds after
    /// the round trip. A premature spend would make every later template fail to build.
    fn has_mature_funding(&self) -> bool {
        self.funding.first().is_some_and(|(_, _, daa)| self.sink_daa() >= daa + COINBASE_MATURITY + 2)
    }

    /// Pops the oldest funding output (the caller must have checked [`Self::has_mature_funding`]).
    fn take_funding(&mut self) -> Funding {
        self.funding.remove(0)
    }

    /// Builds the funded, ML-DSA-87-signed stake bond and hands it to the miner.
    fn send_bond(&mut self, env: &mut Environment<SimMsg>) -> ValidatorPhase {
        let (outpoint, value, daa) = self.take_funding();
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
        env.unicast(self.cfg.id, self.cfg.miner_id, SimMsg::Tx(bond_tx));
        ValidatorPhase::BondSent { bond_outpoint }
    }

    /// The latest blue-score epoch this node's OWN sink makes ready, if any.
    fn ready_epoch(&self) -> Option<u64> {
        use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
        let dns = self.config.params.dns_params.as_ref().expect("the harness params carry DNS params");
        let sink_blue = self.node.consensus.storage.headers_store.get_blue_score(self.node.sink()).expect("the sink has a header");
        ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
    }

    /// Signs the canonical anchor of `ready` and hands the carrying shard tx to the miner. Reads the
    /// anchor from this node's OWN consensus — the point of the multi-node setup is that the
    /// validator sees the same canonical anchor the miner will validate against. Returns the
    /// attested epoch on success, or `None` if this node cannot resolve a canonical anchor for the
    /// epoch yet (the caller then retries on a later block; no funding is consumed).
    fn send_attestation(&mut self, bond_outpoint: TransactionOutpoint, ready: u64, env: &mut Environment<SimMsg>) -> Option<u64> {
        use kaspa_consensus_core::Hash64;

        let dns = self.config.params.dns_params.clone().expect("the harness params carry DNS params");
        let sink = self.node.sink();
        let vp = &self.node.consensus.virtual_processor;
        let anchor = vp.canonical_anchor_by_blue_score(ready, sink, &dns)?;

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
        // A fresh COINBASE output per shard: `funded_signed_shard_tx` signs against a UTXO entry it
        // marks `is_coinbase = true`, so a non-coinbase output (e.g. a previous shard's change)
        // would not validate.
        let (outpoint, value, daa) = self.take_funding();
        let shard_tx = dns_harness::funded_signed_shard_tx(
            self.seed,
            outpoint,
            value,
            daa,
            attestation,
            self.config.params.storage_mass_parameter,
        );
        env.unicast(self.cfg.id, self.cfg.miner_id, SimMsg::Tx(shard_tx));
        Some(anchor.epoch)
    }

    /// Attests the latest ready epoch, if this node's sink made a NEW one ready (strictly past
    /// `already_attested`) and a matured funding output is available. Returns the attested epoch.
    ///
    /// The "strictly newer epoch" guard is what keeps the validator from re-attesting an epoch it
    /// already signed: a duplicate would burn a funding output on a tx the reward-uniqueness rule
    /// rejects, and a rejected tx sticks in the miner's pending set.
    fn attest_if_new_epoch(
        &mut self,
        bond_outpoint: TransactionOutpoint,
        already_attested: Option<u64>,
        env: &mut Environment<SimMsg>,
    ) -> Option<u64> {
        let ready = self.ready_epoch()?;
        if already_attested.is_some_and(|last| ready <= last) {
            return None;
        }
        if !self.has_mature_funding() {
            return None;
        }
        let attested = self.send_attestation(bond_outpoint, ready, env)?;
        kaspa_core::trace!("[sim-harness] validator {} attested epoch {} (virtual time {})", self.cfg.id, attested, env.now());
        Some(attested)
    }

    /// Advances the lifecycle by at most one step, after the block has been validated.
    fn step(&mut self, block: &Block, env: &mut Environment<SimMsg>) {
        let before = self.phase.name();
        self.phase = match std::mem::replace(&mut self.phase, ValidatorPhase::Funding) {
            ValidatorPhase::Funding => {
                // Two owned outputs: one funds the bond, the next one funds the first shard. The
                // bond may only spend a MATURED coinbase (see `has_mature_funding`).
                if self.funding.len() >= 2 && self.has_mature_funding() { self.send_bond(env) } else { ValidatorPhase::Funding }
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
                    match self.attest_if_new_epoch(bond_outpoint, None, env) {
                        Some(epoch) => ValidatorPhase::Attesting { bond_outpoint, last_attested_epoch: epoch },
                        None => ValidatorPhase::BondBurying { bond_outpoint, remaining: 0 },
                    }
                }
            }
            ValidatorPhase::Attesting { bond_outpoint, last_attested_epoch } => {
                let attested = self.attest_if_new_epoch(bond_outpoint, Some(last_attested_epoch), env).unwrap_or(last_attested_epoch);
                ValidatorPhase::Attesting { bond_outpoint, last_attested_epoch: attested }
            }
        };
        if before != self.phase.name() {
            kaspa_core::trace!("[sim-harness] validator phase: {} -> {} (virtual time {})", before, self.phase.name(), env.now());
        }
    }
}

impl Process<SimMsg> for ValidatorActor {
    fn resume(&mut self, resumption: Resumption<SimMsg>, env: &mut Environment<SimMsg>) -> Suspension {
        if let Resumption::Message(SimMsg::Block(block)) = resumption
            && self.seen.insert(block.header.hash)
        {
            self.node.insert(block.clone());
            self.harvest(&block);
            self.step(&block, env);
        }
        Suspension::Idle
    }
}
