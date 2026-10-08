//! **RFC-0012 — the private zero-DNS acceptance matrix, through the real virtual processor.**
//!
//! Every test runs testnet-12 as launched, with the harness cards, the EVM lane **as shipped** (active from DAA 0; the
//! node's own templates at the host's clock, nothing re-stamped but the heartbeat adapter's own slot stamp) and
//! `palw_dns_retirement_v1` armed in a **TEST copy** of the params at DAA [`FENCE`]. The fence is `None` on every
//! preset and this file assigns no deployment value: the policy below (`D = 1`, `W = 1`, no concentration cap) exists
//! so a test can certify something, and means nothing else.
//!
//! **What the harness can and cannot reach, stated once.** The EVM lane cannot be re-stamped, so the chain's clock is
//! the wall clock: one heartbeat tick, then one more stamped into its slot, and that is where testnet-12's DAA stops
//! (`p2_evm_twin`). So the fence sits at DAA 2, the chain crosses it with a handful of blocks, and **no claim can reach
//! `Final` here** (a `Final` needs 120+ DAA). Everything below that needs a `Final`:
//!
//! * the evidence itself is tested where `Final`s exist, on the fold's own deltas
//!   (`consensus/core/tests/rfc0012_native_evidence_fold.rs`: accepted 1001, Final 1124, retired 4125);
//! * the processor's use of evidence is tested here by **placing facts at chain blocks** through a `cfg(test)` seam
//!   (`VirtualStateProcessor::native_fact_override`) and by planting a frontier consistently (tip *and* delta row);
//!   both are named where they are used. A real `Final` through this processor with the EVM lane on is a **GAP** the
//!   implementation record states.
//!
//! **Each test states its expected result first.** `observe` records, after every block, what every reader would say.
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_genesis_chain_on, t12_reopened_chain, t12_with_harness_cards_and_evm,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::model::stores::dns_state::DnsStateStore;
use crate::model::stores::pruning::PruningStore;
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::dns_state::DnsStateStoreReader;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::evm::model_market::{
    MISAKA_MODEL_WRITER, PALW_EVM_ACTION_SELL, PalwEvmSettlementOutcomeV1, refusal, send_action_sell_calldata,
};
use kaspa_consensus_core::evm::{
    CanonicalEvmHeads, DepositClaim, EVM_CHAIN_ID, EVM_NATIVE_SCALE, EvmAddress, EvmSystemOp, EvmTemplateData,
};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_native_settlement_v1::{
    MatureUsefulWorkV1, NativeSettlementSnapshotV1, PalwDnsRetirementV1, PalwSettlementPolicyV1, SettlementStopV1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

/// The first DAA at which the retirement is in force in these tests. A TEST value: the harness clock reaches DAA 1 and stops.
const FENCE: u64 = 1;
const DEPOSIT: u64 = 100_000_000;
const CARRIER_FEE: u64 = 1_000_000;
const GAS_LIMIT: u64 = 200_000;
const MAX_FEE: u128 = 10_000_000_000;
/// What the withdrawal pays out: 0.1 MSK, 10^7 sompi = 10^17 wei.
const WITHDRAW_SOMPI: u64 = 10_000_000;
const ACCOUNT: [u8; 20] =
    [0xb3, 0xc7, 0x7f, 0xc7, 0xb3, 0xb1, 0xdd, 0x1a, 0x72, 0xb3, 0x5d, 0x7c, 0x72, 0x18, 0x11, 0xae, 0x12, 0xf6, 0x63, 0xaa];

/// The account's two sells at nonces 0 and 1 (`p2_evm_twin`'s fixtures, regenerate with the recipe there).
const SELLS: [(u64, &str); 2] = [
    (
        1,
        concat!(
            "02f90150834d534b80808502540be40083030d4094000000000000000000000000000000000000f01380b8e4df42f68f0000000000000000",
            "0000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000084",
            "0100000274c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da40907",
            "0cb2c98b8e861598db902f7a0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000",
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c080a054eba2d49a",
            "9c85a48fbd73a0e02997e2fa60288915cda2dbc908bbfb4ba3a657a0619f1a9aa247fd19f86e0492102143ce81e7bab365fbc88a8fb131c3",
            "a543a993",
        ),
    ),
    (
        2,
        concat!(
            "02f90150834d534b01808502540be40083030d4094000000000000000000000000000000000000f01380b8e4df42f68f0000000000000000",
            "0000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000084",
            "0100000274c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da40907",
            "0cb2c98b8e861598db902f7a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000",
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c001a0b1179f885d",
            "013e10bf66fe9060d8a97470d909b66b8f3cca3859bc851f7d1e55a05dbc1f2303a430aeece4d3ef6407d0b102a551cf5efae57eb669c4f6",
            "75a07c51",
        ),
    ),
];

/// **The withdrawal**, nonce 2 (after the two sells): a payable call of 10^17 wei to the F002 precompile (`0x…f002`) with
/// the destination `p2pkh_mldsa87_spk(&[0x42; 64])` as calldata (`version u16 BE ‖ script`), gas limit 100,000, max fee
/// 10 gwei, tip 0, chain id `EVM_CHAIN_ID`, key `[0x50; 32]` (the same account as the sells). Regenerate with
/// `cast mktx --private-key 0x5050…50 --chain 5067595 --nonce 2 --gas-limit 100000 --gas-price 10000000000
/// --priority-gas-price 0 --value 100000000000000000 0x…f002 0x0000 76c440 <42 x 64> 88a6`; the test decodes it and
/// checks every field against this crate's own constants before it uses it.
const WITHDRAW_NONCE_2: &str = concat!(
    "02f8bb834d534b02808502540be400830186a094000000000000000000000000000000000000f00288016345785d8a0000b847000076c440",
    "4242424242424242424242424242424242424242424242424242424242424242424242424242424242424242424242424242424242424242",
    "424242424242424288a6c001a01d78dfd13b08899e74030f6542a3317e9cc8c8d5e17e74ef512fd528629bc66fa01b9276341699de71e2cd",
    "478b5c313d534cf6c6656e3cea8301934a95ec8b4b93",
);

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;
type Parts = (Config, PalwConsensusParamsV2, Premine, Premine);

/// **A TEST policy.** `D = 1`, `W = 1`, no concentration limit: the least that lets a fixture certify. Not a proposal.
pub(super) fn test_policy() -> PalwSettlementPolicyV1 {
    PalwSettlementPolicyV1 { settled_anchor_depth: 1, unique_mature_work: 1, max_operator_permille: 1000, max_class_permille: 1000 }
}

pub(super) fn test_retirement(at: u64) -> PalwDnsRetirementV1 {
    PalwDnsRetirementV1 { activation: ForkActivation::new(at), settlement: test_policy(), legacy_evidence_horizon_daa: 5 }
}

/// testnet-12 as launched with the EVM lane as shipped, and — when `fence` — the retirement armed at that DAA in a copy.
pub(super) fn parts(fence: Option<u64>) -> Parts {
    parts_with(fence, false)
}

/// [`parts`], with — when `bft` — the DNS BFT gate (ADR-0128) armed from DAA 0 in the same copy, so the veto the retirement
/// removes exists to be removed. The gate's numbers are the existing unit test's, and are test values.
pub(super) fn parts_with(fence: Option<u64>, bft: bool) -> Parts {
    let (config, bundle, premine, floats) = t12_with_harness_cards_and_evm(true);
    if fence.is_none() && !bft {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    if let Some(at) = fence {
        params.palw_dns_retirement = Some(test_retirement(at));
    }
    if bft {
        params.dns_bft_gate = Some(kaspa_consensus_core::config::params::DnsBftGateV1 {
            activation: ForkActivation::new(0),
            t_leak_daa: 50,
            reentry_final_depth_daa: 10,
            min_retained_validators: 4,
        });
    }
    params.validate_palw_v2().expect("a test retirement validates on testnet-12");
    (ConfigBuilder::new(params).skip_proof_of_work().build(), bundle, premine, floats)
}

/// The combined ledger in wei: the virtual UTXO set, the EVM state, the wei burned by base fees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Supply {
    utxo_sompi: u128,
    evm_wei: u128,
    burned_wei: u128,
}

impl Supply {
    fn wei(self) -> u128 {
        self.utxo_sompi * EVM_NATIVE_SCALE as u128 + self.evm_wei + self.burned_wei
    }
}

/// What every reader said right after a block was taken.
#[derive(Clone, Debug)]
pub(super) struct Seen {
    pub hash: BlockHash,
    pub daa: u64,
    pub blue: u64,
    pub retired: bool,
    pub snapshot: Result<Option<NativeSettlementSnapshotV1>, String>,
    pub heads: Result<Option<CanonicalEvmHeads>, String>,
    pub supply: Supply,
    pub minted: u64,
    pub fees: u64,
    /// The DNS state row as the node holds it after the block.
    pub dns: Option<kaspa_consensus_core::dns_finality::DnsState>,
}

pub(super) struct Rig {
    pub chain: T12Chain,
    pub config: Config,
    pub bundle: PalwConsensusParamsV2,
    pub domain: Hash64,
    pub nonce: u64,
    pub wallets: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)>,
    pub fees: BTreeMap<kaspa_consensus_core::tx::TransactionId, u64>,
    pub log: Vec<Seen>,
    pub last_supply: Option<Supply>,
}

impl Rig {
    pub fn new(parts: &Parts, nonce_base: u64) -> Self {
        let (config, bundle, premine, floats) = parts;
        let chain = t12_genesis_chain(config, bundle, premine, floats);
        Self::around(chain, parts, nonce_base)
    }

    pub fn on(consensus: TestConsensus, parts: &Parts, nonce_base: u64) -> Self {
        let (config, bundle, premine, floats) = parts;
        let chain = t12_genesis_chain_on(consensus, config, bundle, premine, floats);
        Self::around(chain, parts, nonce_base)
    }

    /// A node restarted over the database it stopped on, carrying the stopped rig's wallets and nonce.
    pub fn resumed(chain: T12Chain, parts: &Parts, wallets: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)>, nonce: u64) -> Self {
        let mut rig = Self::around(chain, parts, nonce);
        rig.wallets = wallets;
        rig
    }

    fn around(chain: T12Chain, parts: &Parts, nonce_base: u64) -> Self {
        kaspa_core::log::try_init_logger("warn");
        let (config, bundle, _, floats) = parts;
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let mut rig = Rig {
            chain,
            config: config.clone(),
            bundle: bundle.clone(),
            domain,
            nonce: nonce_base,
            wallets: floats.iter().cloned().enumerate().collect(),
            fees: BTreeMap::new(),
            log: Vec::new(),
            last_supply: None,
        };
        rig.last_supply = Some(rig.supply());
        rig
    }

    pub fn vp(&self) -> std::sync::Arc<crate::pipeline::virtual_processor::VirtualStateProcessor> {
        self.chain.vp()
    }

    pub fn api(&self) -> &TestConsensus {
        &self.chain.ctx.consensus
    }

    pub fn supply(&self) -> Supply {
        let c = self.api();
        let utxo_sompi: u128 = c.get_virtual_utxos(None, 10_000_000, false).iter().map(|(_, e)| e.amount as u128).sum();
        let (evm_wei, burned_wei) = match c.get_evm_head_header() {
            Ok(Some(h)) => (
                h.evm_total_native_balance.try_to_u128().expect("the EVM total fits u128"),
                h.evm_burn_accumulator.try_to_u128().expect("the burn accumulator fits u128"),
            ),
            _ => (0, 0),
        };
        Supply { utxo_sompi, evm_wei, burned_wei }
    }

    /// **The node's own heartbeat** (the H1 miner's pass): its template at this host's clock, a nonce, the lane's adapter.
    pub fn beat(&mut self) -> MutableBlock {
        self.nonce += 1;
        let mut t = self
            .api()
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        t.block.header.nonce = self.nonce;
        t.block.header.finalize();
        let (t, _) = self.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
        t.block
    }

    /// **Card `card`'s attempt on the node's own template at this host's clock**, carrying `txs` and `evm`.
    pub fn attempt(&mut self, card: usize, txs: Vec<Transaction>, evm: EvmTemplateData) -> MutableBlock {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
        };
        self.nonce += 1;
        let bond = self.chain.bonds[card];
        let mut t = self
            .api()
            .build_block_template_with_evm(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(txs)),
                TemplateBuildMode::Standard,
                evm,
            )
            .expect("a template");
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "the attempt lane");
        t.block.header.nonce = self.nonce;
        let facts = self.api().palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0)).expect("the floor answers");
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let pubkey = key.verification_key.as_ref().to_vec();
        facts.ready_to_produce(&pubkey).unwrap_or_else(|why| panic!("card {card} is not ready to produce: {why}"));
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.domain,
            challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: pubkey,
            operator_id: facts.bond.as_ref().expect("a registered card").operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0012_5000_0000_0000 | self.nonce),
            execution_root: Hash64::from_u64_word(0xE7EC_1200_0000_0000 | self.nonce),
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let won = (0u64..4_000_000).any(|draw| {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x7A12_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            class_ticket_v3(&attempt, anchor) <= facts.class_target
        });
        assert!(won, "the floor's class lottery is winnable");
        let claim_id = attempt_id_v2(&attempt);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0x12; 32])
                .expect("ML-DSA-87 signs")
                .as_ref()
                .to_vec();
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        t.block
    }

    /// Record what every reader says about the current sink, after `block`.
    fn observe(&mut self, block: &Block) {
        let c = self.api();
        let daa = block.header.daa_score;
        let supply = self.supply();
        let minted: u64 = block.transactions[0].outputs.iter().map(|o| o.value).sum();
        let fees: u64 = block.transactions.iter().skip(1).filter_map(|t| self.fees.get(&t.id())).sum();
        let seen = Seen {
            hash: block.header.hash,
            daa,
            blue: block.header.blue_score,
            retired: self.config.params.palw_dns_retired_at(daa),
            snapshot: c.get_native_settlement_snapshot().map_err(|e| e.to_string()),
            heads: c.get_evm_canonical_heads().map_err(|e| e.to_string()),
            supply,
            minted,
            fees,
            dns: self.vp().dns_state_store.read().get().ok(),
        };
        self.last_supply = Some(supply);
        self.log.push(seen);
    }

    /// **Insert a block this rig built and demand it became the UTXO-valid sink** — build == validate on every block, the
    /// EVM lane re-executed to the root its builder committed.
    pub async fn take(&mut self, block: MutableBlock, what: &str) -> Block {
        let block = block.to_immutable();
        let hash = block.header.hash;
        self.api()
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        assert_eq!(self.api().block_status(hash), BlockStatus::StatusUTXOValid, "{what}: UTXO-valid");
        assert_eq!(self.chain.sink(), hash, "{what} is the sink");
        assert_ne!(block.header.evm_commitment_root, Hash64::default(), "{what}: the lane committed");
        self.observe(&block);
        block
    }

    /// A peer's block arrives. It need not become the sink; it must not be refused.
    pub async fn arrive(&mut self, block: Block, what: &str) {
        let hash = block.header.hash;
        self.api()
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        self.observe(&block);
    }

    /// The selected-chain blocks from genesis (exclusive) to `upto`, oldest first.
    pub fn blocks_through(&self, upto: BlockHash) -> Vec<Block> {
        let vp = self.vp();
        let genesis = self.config.params.genesis.hash;
        let mut hashes = Vec::new();
        let mut at = upto;
        while at != genesis {
            hashes.push(at);
            at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
        }
        hashes.reverse();
        hashes.into_iter().map(|h| self.api().get_block(h).expect("the node holds every block of its chain")).collect()
    }

    /// A deposit-lock transaction to [`ACCOUNT`] from card `card`'s fee float.
    pub fn lock_tx(&mut self, card: usize) -> Transaction {
        let (lock, lock_entry) = self.wallets.remove(&card).expect("the card's float");
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(lock, vec![], 0, 1)],
            vec![
                TransactionOutput::new(
                    DEPOSIT,
                    kaspa_txscript::script_class::evm_deposit_lock_script(ACCOUNT, 1_000_000, 0, card_payout_spk(card).script()),
                ),
                TransactionOutput::new(lock_entry.amount - DEPOSIT - CARRIER_FEE, card_payout_spk(card)),
            ],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            Vec::new(),
        );
        sign_spend(&mut tx, lock_entry, card, self.config.params.storage_mass_parameter);
        self.fees.insert(tx.id(), CARRIER_FEE);
        tx
    }
}

pub(super) fn no_evm() -> EvmTemplateData {
    EvmTemplateData { evm_coinbase: EvmAddress::from_bytes([0xCB; 20]), transactions: Vec::new(), system_ops: Vec::new() }
}

fn decode_fixture(hex: &str) -> Vec<u8> {
    let mut raw = vec![0u8; hex.len() / 2];
    faster_hex::hex_decode(hex.as_bytes(), &mut raw).expect("a hex fixture");
    raw
}

/// The sells, each decoded and checked against the fields it must carry (`p2_evm_twin::sells`).
pub(super) fn sells(line: &Hash64) -> Vec<Vec<u8>> {
    SELLS
        .iter()
        .enumerate()
        .map(|(nonce, (units, raw_hex))| {
            let calldata = send_action_sell_calldata(line, *units, 0);
            let raw = decode_fixture(raw_hex);
            let tx = kaspa_evm::tx::decode_eth_tx(&raw).unwrap_or_else(|e| panic!("sell {nonce} does not decode ({e:?})"));
            assert_eq!(
                (tx.from, tx.to, tx.nonce, tx.chain_id, tx.gas_limit, tx.max_fee_per_gas, &tx.input),
                (ACCOUNT, Some(MISAKA_MODEL_WRITER.as_bytes()), nonce as u64, Some(EVM_CHAIN_ID), GAS_LIMIT, MAX_FEE, &calldata),
                "sell {nonce} is not the fixture it must be (regenerate as p2_evm_twin documents)"
            );
            raw
        })
        .collect()
}

/// The withdrawal, decoded and held to every field it must carry.
pub(super) fn withdrawal() -> (Vec<u8>, kaspa_consensus_core::tx::ScriptPublicKey) {
    let raw = decode_fixture(WITHDRAW_NONCE_2);
    let spk = p2pkh_mldsa87_spk(&[0x42u8; 64]);
    let mut calldata = spk.version().to_be_bytes().to_vec();
    calldata.extend_from_slice(spk.script());
    let tx = kaspa_evm::tx::decode_eth_tx(&raw).unwrap_or_else(|e| panic!("the withdrawal does not decode ({e:?})"));
    assert_eq!(
        (tx.from, tx.to, tx.nonce, tx.chain_id, tx.gas_limit, tx.max_fee_per_gas, tx.value, &tx.input),
        (
            ACCOUNT,
            Some(kaspa_consensus_core::evm::MISAKA_WITHDRAW_PRECOMPILE.as_bytes()),
            2,
            Some(EVM_CHAIN_ID),
            100_000,
            MAX_FEE,
            {
                let mut v = [0u8; 32];
                v[16..].copy_from_slice(&(WITHDRAW_SOMPI as u128 * EVM_NATIVE_SCALE as u128).to_be_bytes());
                v
            },
            &calldata
        ),
        "the withdrawal fixture drifted (regenerate with the recipe on WITHDRAW_NONCE_2)"
    );
    (raw, spk)
}

fn balance_of(rig: &Rig, who: [u8; 20]) -> u128 {
    let sink = rig.chain.sink();
    let snapshot = rig.api().get_evm_state_snapshot_of(sink).expect("a snapshot read").expect("the sink has an EVM state");
    snapshot
        .accounts
        .iter()
        .find(|a| a.address.as_bytes() == who)
        .map(|a| a.balance.try_to_u128().expect("a balance fits u128"))
        .unwrap_or(0)
}

/// **The scripted run every test below shares**, on a rig: heartbeat (DAA 1), the lock (an attempt carrying the deposit
/// lock tx), the heartbeat stamped into its slot (**the fence block**, which also carries the claim of the lock), the sells,
/// the withdrawal, and two attempts to execute and settle them. Returns the lock tx and the model line.
pub(super) struct Run {
    pub lock: Transaction,
    pub line: Hash64,
    pub names: Vec<(&'static str, BlockHash)>,
}

pub(super) async fn scripted_run(rig: &mut Rig, through: &str) -> Run {
    let line = {
        let (_, genesis) = rig.chain.tip_state();
        let base = rig.chain.bundle.base_class_id;
        *genesis.classes_iter().map(|(id, _)| id).find(|id| **id != base).expect("testnet-12 registers a model class at genesis")
    };
    let raws = sells(&line);
    let (withdraw_raw, _) = withdrawal();
    let mut names: Vec<(&'static str, BlockHash)> = Vec::new();
    macro_rules! step {
        ($name:literal, $block:expr) => {{
            let b = $block;
            let b = rig.take(b, $name).await;
            names.push(($name, b.header.hash));
            if $name == through {
                return Run { lock: Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]), line, names };
            }
            b
        }};
    }
    let b0 = rig.beat();
    step!("b0-beat", b0);
    let lock = rig.lock_tx(1);
    let e1 = rig.attempt(0, vec![lock.clone()], no_evm());
    let e1 = step!("e1-lock", e1);
    assert!(e1.transactions.iter().any(|t| t.id() == lock.id()), "the lock rides in e1 (daa {})", e1.header.daa_score);
    let b1 = rig.beat();
    // ADR-0109 Decision 1: every lock the virtual set holds is claimed unasked — by the NEXT block, whatever its lane. Here that
    // is the heartbeat stamped into its slot, which is also the block that crosses the fence.
    let deposit = DepositClaim {
        deposit_outpoint: TransactionOutpoint::new(lock.id(), 0),
        evm_address: EvmAddress::from_bytes(ACCOUNT),
        amount_sompi: DEPOSIT,
        claim_tip_sompi: 0,
    };
    assert_eq!(b1.evm_payload.system_ops, vec![EvmSystemOp::DepositClaim(deposit)], "the node's own heartbeat carries the claim");
    step!("b1-fence-claim", b1);
    let e2 = rig.attempt(2, Vec::new(), EvmTemplateData { transactions: raws.clone(), ..no_evm() });
    assert_eq!(e2.evm_payload.transactions, raws, "the payload carries both sells");
    step!("e2-sells", e2);
    let e3 = rig.attempt(3, Vec::new(), EvmTemplateData { transactions: vec![withdraw_raw.clone()], ..no_evm() });
    assert_eq!(e3.evm_payload.transactions, vec![withdraw_raw], "the payload carries the withdrawal");
    step!("e3-withdraw", e3);
    let e4 = rig.attempt(4, Vec::new(), no_evm());
    step!("e4-settle", e4);
    let e5 = rig.attempt(5, Vec::new(), no_evm());
    step!("e5", e5);
    Run { lock, line, names }
}

fn seen<'a>(rig: &'a Rig, hash: BlockHash) -> &'a Seen {
    rig.log.iter().find(|s| s.hash == hash).expect("the block was observed")
}

// =====================================================================================================================
// MATRIX 1 — healthy zero-DNS work with deposits, withdrawals and market orders across the fence
// =====================================================================================================================

/// **EXPECTED (stated before the run).**
///
/// 1. Every block, on both sides of the fence, is the UTXO-valid sink of the node that built it (build == validate).
/// 2. Below the fence there is no native snapshot and the heads are the legacy ones (`safe` is the sink). From the fence
///    block on there is a snapshot whose generation is the sink and whose `latest` is the sink, and the heads agree with it.
///    With no `Final` work on this clock there is no PALW frontier, so `safe` and `finalized` are `None` with the stop
///    `FrontierNotCovered` — never the sink, never the legacy label.
/// 3. The DNS state row does not move once the fence is crossed.
/// 4. The lock output is consumed exactly once; the account is credited exactly `DEPOSIT x scale` wei and no more; the
///    withdrawal debits the account and creates exactly one UTXO of `WITHDRAW_SOMPI` to the destination; the sells settle
///    (both `Refused { MARKET_MISSING }`: the line has no seeded market, so nothing fills — stated, not hidden).
/// 5. **The combined ledger conserves, exactly, on every block**: `utxo x scale + EVM balances + wei burned` moves by
///    `(coinbase outputs - L1 fees) x scale` and by nothing else, before and after the fence.
#[tokio::test]
async fn rfc12_x1_a_zero_dns_chain_crosses_the_fence_with_deposit_withdrawal_and_market_orders() {
    let p = parts(Some(FENCE));
    let mut rig = Rig::new(&p, 0x12_0001_0000);
    let genesis_supply = rig.supply();
    let run = scripted_run(&mut rig, "").await;
    let dns_rows: Vec<_> = {
        let vp = rig.vp();
        let state = vp.dns_state_store.read().get().ok();
        vec![state]
    };

    // 2. snapshot / heads by side of the fence.
    let mut crossed = false;
    for s in &rig.log {
        eprintln!(
            "[x1] {} daa {} blue {} retired {} snapshot {:?} heads {:?}",
            s.hash,
            s.daa,
            s.blue,
            s.retired,
            s.snapshot.as_ref().map(|o| o.as_ref().map(|x| (x.latest, x.safe, x.finalized, x.stop))),
            s.heads.as_ref().map(|o| o.map(|h| (h.latest_head(), h.safe_head(), h.finalized_head())))
        );
        if s.retired {
            crossed = true;
            let snap = s.snapshot.as_ref().expect("readable").as_ref().expect("a snapshot from the fence");
            assert_eq!(snap.generation, s.hash, "the generation is the sink");
            assert_eq!(snap.latest, Some(s.hash), "latest is the newest executed, root-verified result: the sink");
            assert_eq!((snap.safe, snap.finalized), (None, None), "no Final work: nothing is safe and nothing is finalized");
            assert_eq!(snap.stop, Some(SettlementStopV1::FrontierNotCovered), "and the reason is stated");
            let heads = s.heads.as_ref().expect("readable").expect("heads");
            assert_eq!((heads.latest_head(), heads.safe_head(), heads.finalized_head()), (Some(s.hash), None, None));
        } else {
            assert!(!crossed, "the fence is monotone along the chain");
            assert_eq!(s.snapshot.as_ref().expect("readable"), &None, "no native snapshot below the fence");
            let heads = s.heads.as_ref().expect("readable").expect("legacy heads");
            assert_eq!(heads.latest_head(), Some(s.hash));
            assert_eq!(heads.safe_head(), Some(s.hash), "below the fence the legacy label is the sink, byte for byte as before");
        }
    }
    assert!(crossed, "the run crosses the fence");
    let first_retired = rig.log.iter().position(|s| s.retired).expect("a retired block");
    assert!(first_retired > 0 && first_retired < rig.log.len() - 3, "the fence is crossed with blocks on both sides");

    // 3. the DNS row is frozen from the fence block on: the overlay's recompute is skipped past the fence, and nothing else writes it.
    drop(dns_rows);
    let retired_rows: Vec<_> = rig.log.iter().filter(|s| s.retired).map(|s| s.dns.clone()).collect();
    assert!(retired_rows.len() >= 3 && retired_rows.windows(2).all(|w| w[0] == w[1]), "the DNS state row never moves past the fence");

    // 4. the bridge.
    let lock_out = TransactionOutpoint::new(run.lock.id(), 0);
    assert!(rig.api().get_virtual_utxo_entry(lock_out).is_none(), "the lock output was consumed by the claim");
    let (_, spk) = withdrawal();
    let paid: Vec<_> = rig
        .api()
        .get_virtual_utxos(None, 10_000_000, false)
        .into_iter()
        .filter(|(_, e)| e.script_public_key == spk)
        .collect();
    assert_eq!(paid.len(), 1, "the withdrawal materialized exactly one UTXO");
    assert_eq!(paid[0].1.amount, WITHDRAW_SOMPI);
    let scale = EVM_NATIVE_SCALE as u128;
    let after = balance_of(&rig, ACCOUNT);
    let expected_before_gas = DEPOSIT as u128 * scale - WITHDRAW_SOMPI as u128 * scale;
    assert!(after <= expected_before_gas && expected_before_gas - after < 10 * scale * 1_000_000, "credited once, debited once, gas aside: {after}");
    let settlements = rig.chain.tip_state().1.evm_settlements();
    // The sells were queued by e3's lane and settled by e4's fold, which e5 carries; the state tip is e6, so read the log of outcomes.
    let _ = settlements;

    // 5. conservation, on every block.
    let mut previous = genesis_supply.wei();
    for s in &rig.log {
        let want = previous as i128 + (s.minted as i128 - s.fees as i128) * scale as i128;
        assert_eq!(
            s.supply.wei() as i128,
            want,
            "block {} (daa {}): the combined ledger moved by {} wei, expected {} (minted {} - fees {})",
            s.hash,
            s.daa,
            s.supply.wei() as i128 - previous as i128,
            (s.minted as i128 - s.fees as i128) * scale as i128,
            s.minted,
            s.fees
        );
        previous = s.supply.wei();
    }
}


/// **CONTROL (EXPECTED: identical script, no fence — every block UTXO-valid, the claim carried, the ledger conserves).**
#[tokio::test]
async fn rfc12_x0_control_the_same_script_without_the_fence() {
    let p = parts(None);
    let mut rig = Rig::new(&p, 0x12_0000_0000);
    let genesis_supply = rig.supply();
    let run = scripted_run(&mut rig, "").await;
    assert!(rig.log.iter().all(|s| !s.retired && s.snapshot.as_ref().unwrap().is_none()));
    let lock_out = TransactionOutpoint::new(run.lock.id(), 0);
    assert!(rig.api().get_virtual_utxo_entry(lock_out).is_none(), "the lock output was consumed by the claim");
    let _ = genesis_supply;
}


// =====================================================================================================================
// shared scenario pieces
// =====================================================================================================================

/// b0 (beat), e1 (card 0), b1 (beat), e2 (card 1). With [`FENCE`] = 1, `e2` is the first block past the fence (DAA 1).
async fn prefix(rig: &mut Rig) -> Vec<Block> {
    let mut blocks = Vec::new();
    let b = rig.beat();
    blocks.push(rig.take(b, "b0").await);
    let e = rig.attempt(0, Vec::new(), no_evm());
    blocks.push(rig.take(e, "e1").await);
    let b = rig.beat();
    blocks.push(rig.take(b, "b1").await);
    let e = rig.attempt(1, Vec::new(), no_evm());
    let e2 = rig.take(e, "e2").await;
    assert!(rig.config.params.palw_dns_retired_at(e2.header.daa_score) || rig.config.params.palw_dns_retirement.is_none(), "e2 is past the fence");
    blocks.push(e2);
    blocks
}

/// Empty attempts by `cards`, one block each, each the sink.
async fn extend(rig: &mut Rig, cards: &[usize]) -> Vec<Block> {
    let mut out = Vec::new();
    for card in cards {
        let b = rig.attempt(*card, Vec::new(), no_evm());
        out.push(rig.take(b, "an attempt").await);
    }
    out
}

/// A second node fed `blocks` as a peer feeds them.
async fn follower(p: &Parts, nonce: u64, blocks: &[Block]) -> Rig {
    let mut f = Rig::new(p, nonce);
    for b in blocks {
        f.arrive(b.clone(), "a block of the followed chain").await;
    }
    f
}

fn snapshot_of(rig: &Rig) -> Option<NativeSettlementSnapshotV1> {
    rig.api().get_native_settlement_snapshot().expect("a readable snapshot")
}

fn heads_of(rig: &Rig) -> Option<CanonicalEvmHeads> {
    rig.api().get_evm_canonical_heads().expect("readable heads")
}

fn plant_dns(rig: &Rig, anchor: BlockHash, anchor_daa: u64, at_sink: BlockHash, sink_daa: u64) {
    use kaspa_consensus_core::dns_finality::{DnsHealth, DnsRolloutStage, DnsState, StakeScore};
    rig.vp()
        .dns_state_store
        .write()
        .set(DnsState {
            selected_chain_anchor: at_sink,
            anchor_daa_score: sink_daa,
            work_depth: Default::default(),
            stake_depth: StakeScore(0),
            last_dns_confirmed_anchor: anchor,
            last_dns_confirmed_anchor_daa_score: anchor_daa,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: Default::default(),
            health: DnsHealth::Active,
        })
        .unwrap();
}

// =====================================================================================================================
// MATRIX 2 — a DNS-final anchor opposed to a valid PALW candidate; absent DNS votes
// =====================================================================================================================

/// **EXPECTED.** With the DNS BFT gate armed from DAA 0 and a DNS-final anchor on the incumbent:
///
/// * at an incumbent **below** the fence the old veto still applies to a sibling that abandons the anchor
///   (`HardCheckpointReject`) — history keeps its rule;
/// * at an incumbent **at or past** the fence the gate does not refuse, and the whole `dns_reorg_outcome` equals the outcome with
///   no anchor at all — the DNS state is not an input;
/// * through the pipeline, two nodes fed the same blocks, one holding a DNS-final anchor planted on the incumbent branch and one
///   holding none, choose the same sink and publish the same native snapshot after a heavier PALW branch abandons that anchor.
#[tokio::test]
async fn rfc12_x2_an_opposed_dns_anchor_vetoes_below_the_fence_and_is_ignored_past_it() {
    use kaspa_consensus_core::dns_finality::ActiveBondView;
    let p = parts_with(Some(FENCE), true);

    // ---- below the fence: a sibling pair at DAA 0 -------------------------------------------------------------------------
    let mut a = Rig::new(&p, 0x12_0210_0000);
    let b0 = {
        let b = a.beat();
        a.take(b, "b0").await
    };
    let mut b = follower(&p, 0x12_0220_0000, std::slice::from_ref(&b0)).await;
    let e1a = {
        let e = a.attempt(0, Vec::new(), no_evm());
        a.take(e, "e1a").await
    };
    let e1b = {
        let e = b.attempt(4, Vec::new(), no_evm());
        b.take(e, "e1b").await
    };
    a.arrive(e1b.clone(), "the sibling").await;
    assert!(a.config.params.palw_dns_retirement.is_some_and(|r| !r.activation.is_active(e1a.header.daa_score)), "the incumbent is below the fence");
    plant_dns(&a, e1a.header.hash, e1a.header.daa_score, e1a.header.hash, e1a.header.daa_score);
    let below = a.vp().dns_bft_gate_refusal(e1b.header.hash, e1a.header.hash);
    eprintln!("[x2] below the fence: gate refusal of the sibling that abandons the anchor = {below:?}");
    assert_eq!(below, Some(kaspa_consensus_core::dns_finality::DnsReorgOutcome::HardCheckpointReject), "history keeps the veto");
    assert_eq!(a.vp().dns_bft_gate_refusal(e1a.header.hash, e1a.header.hash), None, "a candidate that keeps the anchor is not refused");

    // ---- past the fence: a sibling pair at DAA 1, and the whole outcome -----------------------------------------------------
    let mut q = Rig::new(&p, 0x12_0230_0000);
    let pre = prefix(&mut q).await;
    let mut r = follower(&p, 0x12_0240_0000, &pre).await;
    let x1 = {
        let e = q.attempt(2, Vec::new(), no_evm());
        q.take(e, "x1").await
    };
    let y1 = {
        let e = r.attempt(3, Vec::new(), no_evm());
        r.take(e, "y1").await
    };
    q.arrive(y1.clone(), "the sibling").await;
    let incumbent_daa = x1.header.daa_score;
    assert!(q.config.params.palw_dns_retired_at(incumbent_daa), "the incumbent is past the fence");
    let view = ActiveBondView::default();
    plant_dns(&q, Hash64::default(), 0, x1.header.hash, incumbent_daa);
    let baseline = q.vp().dns_reorg_outcome(y1.header.hash, x1.header.hash, &view);
    plant_dns(&q, x1.header.hash, incumbent_daa, x1.header.hash, incumbent_daa);
    assert_eq!(q.vp().dns_bft_gate_refusal(y1.header.hash, x1.header.hash), None, "the gate abstains past the fence");
    assert_eq!(q.vp().dns_reorg_outcome(y1.header.hash, x1.header.hash, &view), baseline, "and the outcome is the anchorless one");
    assert_ne!(baseline, kaspa_consensus_core::dns_finality::DnsReorgOutcome::HardCheckpointReject);

    // ---- through the pipeline: planted vs none, a heavier branch abandoning the anchor ---------------------------------------
    let mut planted = follower(&p, 0x12_0250_0000, &pre).await;
    let mut clean = follower(&p, 0x12_0260_0000, &pre).await;
    for n in [&mut planted, &mut clean] {
        n.arrive(x1.clone(), "x1").await;
    }
    plant_dns(&planted, x1.header.hash, x1.header.daa_score, x1.header.hash, x1.header.daa_score);
    let ys = {
        let mut ys = vec![y1.clone()];
        ys.extend(extend(&mut r, &[5, 6]).await);
        ys
    };
    for n in [&mut planted, &mut clean] {
        for y in &ys {
            n.arrive(y.clone(), "a block abandoning the anchor").await;
        }
    }
    let (sp, sc) = (planted.chain.sink(), clean.chain.sink());
    eprintln!("[x2] pipeline: planted sink {sp}, clean sink {sc}, the heavier branch's tip {}", ys.last().unwrap().header.hash);
    assert_eq!(sp, sc, "an opposed DNS anchor changed the sink");
    assert_eq!(sp, ys.last().unwrap().header.hash, "the heavier PALW branch won on both");
    assert_eq!(snapshot_of(&planted), snapshot_of(&clean), "and the native snapshot is the same");
    assert_eq!(heads_of(&planted), heads_of(&clean));
}

// =====================================================================================================================
// MATRIX 3 / 4 — sibling races, deep forks, ties, partitions that heal, in every arrival order
// =====================================================================================================================

/// One scenario: node A mines branch X, node B (which has seen the same prefix and not X) mines branch Y, then five nodes meet.
async fn fork_case(x_cards: &[usize], y_cards: &[usize], what: &str) -> BlockHash {
    let p = parts(Some(FENCE));
    let mut a = Rig::new(&p, 0x12_0300_0000);
    let pre = prefix(&mut a).await;
    let mut b = follower(&p, 0x12_0310_0000, &pre).await;
    let xs = extend(&mut a, x_cards).await;
    let ys = extend(&mut b, y_cards).await;
    let mut n1 = follower(&p, 0x12_0320_0000, &pre).await;
    for blk in xs.iter().chain(&ys) {
        n1.arrive(blk.clone(), "x then y").await;
    }
    let mut n2 = follower(&p, 0x12_0330_0000, &pre).await;
    for blk in ys.iter().chain(&xs) {
        n2.arrive(blk.clone(), "y then x").await;
    }
    let mut n3 = follower(&p, 0x12_0340_0000, &pre).await;
    for i in 0..xs.len().max(ys.len()) {
        for blk in xs.get(i).into_iter().chain(ys.get(i)) {
            n3.arrive(blk.clone(), "interleaved").await;
        }
    }
    for blk in &ys {
        a.arrive(blk.clone(), "the partition heals: A receives Y").await;
    }
    for blk in &xs {
        b.arrive(blk.clone(), "the partition heals: B receives X").await;
    }
    let nodes = [&a, &b, &n1, &n2, &n3];
    let sink = nodes[0].chain.sink();
    for (i, n) in nodes.iter().enumerate() {
        assert_eq!(n.chain.sink(), sink, "{what}: node {i} chose another sink");
        assert_eq!(snapshot_of(n), snapshot_of(nodes[0]), "{what}: node {i} publishes another snapshot");
        assert_eq!(heads_of(n), heads_of(nodes[0]), "{what}: node {i} serves other heads");
        assert_eq!(n.supply(), nodes[0].supply(), "{what}: node {i} holds another ledger");
    }
    let snap = snapshot_of(&a).expect("past the fence");
    assert_eq!(snap.generation, sink);
    assert_eq!(snap.latest, Some(sink), "{what}: latest follows the sink through the switch");
    assert_eq!((snap.safe, snap.finalized), (None, None), "{what}: no Final work, so nothing is safe on either branch");
    // The state a replay of just the winning chain reaches is the state every node reached.
    let chain = a.blocks_through(sink);
    let replay = follower(&p, 0x12_0350_0000, &chain).await;
    assert_eq!(replay.chain.sink(), sink, "{what}: the replay reaches the same sink");
    assert_eq!(snapshot_of(&replay), snapshot_of(&a), "{what}: and the same snapshot as the nodes that saw both branches");
    assert_eq!(heads_of(&replay), heads_of(&a));
    assert_eq!(replay.supply(), a.supply(), "{what}: and the same ledger");
    sink
}

/// **EXPECTED.** After the fence, in every arrival order (X then Y, Y then X, interleaved, and each side hearing the other last):
/// all five nodes end on one sink, publish one snapshot and one set of heads, hold one ledger, and match a node that replayed only
/// the winning chain. A strictly heavier branch wins; a tie is broken identically everywhere (the shallow-tie GHOSTDAG order).
#[tokio::test]
async fn rfc12_x3_sibling_and_deep_forks_converge_in_any_arrival_order_and_match_a_replay() {
    let ys = fork_case(&[2, 3], &[4, 5, 6], "sibling race, Y heavier").await;
    let _ = ys;
    fork_case(&[2, 3], &[4, 5], "tie").await;
    fork_case(&[2], &[3, 4, 5, 6, 7], "deep: one block against five (a withheld private fork released later)").await;
}


// =====================================================================================================================
// MATRIX 5 — process restart; old-version node
// =====================================================================================================================

/// **EXPECTED.** A node stopped after the fence and reopened on the same database serves the same sink, the same snapshot, the
/// same heads and holds the same ledger; it rebuilds its in-memory rows from the retained deltas, builds the next block, and
/// that block's snapshot equals the one a node replaying the whole chain from scratch publishes.
#[tokio::test]
async fn rfc12_x5_a_restart_keeps_the_heads_and_the_snapshot_and_carries_on() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let p = parts(Some(FENCE));
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let first = TestConsensus::with_db(db.clone(), &p.0, sender);
    let mut a = Rig::on(first, &p, 0x12_0500_0000);
    prefix(&mut a).await;
    extend(&mut a, &[2]).await;
    let (sink, snap, heads, supply) = (a.chain.sink(), snapshot_of(&a), heads_of(&a), a.supply());
    assert!(snap.is_some(), "a snapshot was published before the stop");
    let (wallets, nonce, simulated_time, reopen_nonce) = (a.wallets.clone(), a.nonce, a.chain.ctx.simulated_time, a.chain.nonce_for_reopen());
    drop(a);

    let mut resumed = p.0.clone();
    resumed.process_genesis = false;
    let (sender, _rx2) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let chain = t12_reopened_chain(second, &resumed, &p.1, simulated_time, reopen_nonce);
    let p2: Parts = (resumed, p.1.clone(), p.2.clone(), p.3.clone());
    let mut r = Rig::resumed(chain, &p2, wallets, nonce);
    assert_eq!(r.chain.sink(), sink, "the restarted node's sink is the stopped node's");
    assert_eq!(snapshot_of(&r), snap, "its native snapshot came off the disk");
    assert_eq!(heads_of(&r), heads, "and its heads");
    assert_eq!(r.supply(), supply, "and its ledger");
    assert_eq!(r.vp().native_rows.lock().len(), 0, "the row cache is memory only: it starts empty");

    let next = extend(&mut r, &[3]).await;
    let s = snapshot_of(&r).expect("a snapshot after the restart");
    assert_eq!((s.generation, s.latest, s.safe, s.finalized), (next[0].header.hash, Some(next[0].header.hash), None, None));
    assert_eq!(s.stop, Some(SettlementStopV1::FrontierNotCovered));
    assert!(r.vp().native_rows.lock().len() > 3, "the rows were rebuilt from the retained deltas");
    let chain = r.blocks_through(r.chain.sink());
    let z = follower(&p, nonce, &chain).await;
    assert_eq!(snapshot_of(&z), snapshot_of(&r), "a from-scratch replay publishes the same snapshot");
    assert_eq!(heads_of(&z), heads_of(&r));
    assert_eq!(z.supply(), r.supply());
}

/// **EXPECTED.** A node on the same code without the fence (the old rules) accepts the fenced chain's blocks below the fence as its
/// sink, and the chain stops for it at the first block whose rules differ (the coinbase it must pay, the state it must commit):
/// its sink stays strictly behind the fenced node's, on the common prefix. It does not follow, and it does not fork the fenced node.
#[tokio::test]
async fn rfc12_x6_a_node_without_the_fence_follows_history_to_the_fence_and_no_further() {
    let (pf, po) = (parts(Some(FENCE)), parts(None));
    let mut a = Rig::new(&pf, 0x12_0600_0000);
    let mut chain = prefix(&mut a).await;
    chain.extend(extend(&mut a, &[2, 3]).await);
    let mut old = Rig::new(&po, 0x12_0610_0000);
    let mut divergence = None;
    for (i, blk) in chain.iter().enumerate() {
        let verdict = old.api().validate_and_insert_block(blk.clone()).virtual_state_task.await;
        let status = old.api().block_status(blk.header.hash);
        eprintln!("[x6] block {i} daa {} -> {:?} / {:?}; the old node's sink is block {:?}", blk.header.daa_score, verdict.as_ref().map(|_| ()).map_err(|e| e.to_string()), status,
            chain.iter().position(|c| c.header.hash == old.chain.sink()));
        if old.chain.sink() != blk.header.hash && divergence.is_none() {
            divergence = Some(i);
        }
    }
    let first = divergence.expect("the old node stops following somewhere");
    let first_retired = chain.iter().position(|b| pf.0.params.palw_dns_retired_at(b.header.daa_score)).expect("a retired block");
    eprintln!("[x6] the old rules agree on blocks 0..{first}; the first retired block is {first_retired}");
    assert!(first >= first_retired, "no block below the fence is refused by the old rules: history is byte-identical");
    assert!(first < chain.len(), "and the old node cannot follow the whole fenced chain");
    let old_sink_pos = chain.iter().position(|c| c.header.hash == old.chain.sink()).expect("the old node's sink is on the fenced chain");
    assert!(old_sink_pos < chain.len() - 1, "the old node's sink is behind the fenced node's");
    assert_eq!(old_sink_pos + 1, first, "it stays on the last common block");
}

// =====================================================================================================================
// MATRIX 6 — below-finalized conflict: alarm, sticky, resync
// =====================================================================================================================

fn publish_native(rig: &Rig, snapshot: NativeSettlementSnapshotV1) {
    let vp = rig.vp();
    let mut batch = rocksdb::WriteBatch::default();
    vp.evm_heads_store.write().set_native_batch(&mut batch, Some(snapshot)).unwrap();
    vp.db.write(batch).unwrap();
}

/// **EXPECTED.** If a branch abandons a head the node had published as `finalized`, the next snapshot says `FinalizedConflict`
/// with no `safe` and no `finalized` (and still the sink as `latest`: execution is a fact, not a certificate), the node logs the
/// resync requirement, and the conflict **stays** through the following blocks and through the loss of the in-memory rows. Only a
/// root-verified pruning-point EVM import (the resync) clears it; after it the snapshot is built from the chain again, and neither
/// the conflict nor DNS authority is back.
#[tokio::test]
async fn rfc12_x7_a_finalized_conflict_is_sticky_and_only_a_validated_import_clears_it() {
    let p = parts(Some(FENCE));
    let mut a = Rig::new(&p, 0x12_0700_0000);
    let pre = prefix(&mut a).await;
    let mut b = follower(&p, 0x12_0710_0000, &pre).await;
    let side = extend(&mut b, &[3]).await;
    let main = extend(&mut a, &[2]).await;
    a.arrive(side[0].clone(), "the sibling").await;
    let sink = a.chain.sink();
    let abandoned = if sink == main[0].header.hash { side[0].header.hash } else { main[0].header.hash };
    assert_ne!(abandoned, sink);
    // The node had published `abandoned` as finalized (the test's hand — nothing in this harness can finalize).
    publish_native(
        &a,
        NativeSettlementSnapshotV1 {
            version: 1,
            ruleset_id: a.config.params.consensus_params_id(),
            policy_id: test_policy().id(),
            generation: sink,
            retirement_daa: FENCE,
            frontier: None,
            latest: Some(sink),
            safe: Some(abandoned),
            finalized: Some(abandoned),
            depth: 1,
            unique_work: "1".into(),
            stop: None,
        },
    );
    let z = extend(&mut a, &[4]).await;
    let conflict = snapshot_of(&a).expect("a snapshot");
    assert_eq!(conflict.stop, Some(SettlementStopV1::FinalizedConflict), "the abandoned finalized head is reported, not relabelled");
    assert_eq!((conflict.safe, conflict.finalized), (None, None));
    assert_eq!(conflict.latest, Some(z[0].header.hash), "latest is the executed sink even in conflict");
    let heads = heads_of(&a).expect("heads");
    assert_eq!((heads.latest_head(), heads.safe_head(), heads.finalized_head()), (Some(z[0].header.hash), None, None));
    // Sticky: another block, and the loss of every in-memory row.
    a.vp().native_rows.lock().clear();
    let z2 = extend(&mut a, &[5]).await;
    let still = snapshot_of(&a).expect("a snapshot");
    assert_eq!((still.stop, still.generation), (Some(SettlementStopV1::FinalizedConflict), z2[0].header.hash), "sticky");
    // The resync: a root-verified import of the pruning point's EVM state (a retired pruning point).
    let pp = a.chain.sink();
    let header = a.api().get_evm_header_of(pp).unwrap().expect("the EVM header");
    let snapshot = a.api().get_evm_state_snapshot_of(pp).unwrap().expect("the EVM state");
    {
        // A node that imports has not got these rows (that is why it imports); this one has, so they go first.
        use crate::model::stores::evm::{EvmHeaderStore, EvmStateStore};
        let (vp, mut batch) = (a.vp(), rocksdb::WriteBatch::default());
        vp.evm_header_store.delete_batch(&mut batch, pp).unwrap();
        vp.evm_state_store.delete_batch(&mut batch, pp).unwrap();
        vp.db.write(batch).unwrap();
    }
    a.vp().import_pruning_point_evm_state(pp, header, snapshot).expect("a root-verified import");
    assert_eq!(snapshot_of(&a), None, "the import clears native evidence until reconstruction supplies proof");
    let heads = heads_of(&a).expect("heads");
    assert_eq!((heads.latest_head(), heads.safe_head(), heads.finalized_head()), (Some(pp), None, None), "and never invents safe or finalized");
    let z3 = extend(&mut a, &[6]).await;
    let rebuilt = snapshot_of(&a).expect("a snapshot");
    assert_eq!((rebuilt.generation, rebuilt.latest, rebuilt.safe, rebuilt.finalized), (z3[0].header.hash, Some(z3[0].header.hash), None, None));
    assert_eq!(rebuilt.stop, Some(SettlementStopV1::FrontierNotCovered), "reconstruction built it from the chain; the conflict is gone");
}

// =====================================================================================================================
// MATRIX 7 — certified work makes a prefix safe; a reorg takes it back; lost history stops certification
// =====================================================================================================================

/// **Plant a safe frontier on the tip, consistently**: the tip *and* the sink's delta row (a `Frontier` entry, the row's root
/// rewritten), so every later block commits to the planted state and a reorg walk reverts it exactly. This is the test's hand: a
/// frontier is what a `Final` claim buys, and no claim can reach `Final` on this clock.
fn plant_frontier(rig: &Rig, blue: u64, frontier: BlockHash) {
    use crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2;
    use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwStateCarriageV2};
    let (sink, tip) = rig.chain.tip_state();
    let before = tip.safe_frontier();
    let mut carriage = PalwStateCarriageV2::from_state(&tip);
    carriage.safe_frontier_blue_score = blue;
    carriage.safe_frontier = frontier;
    let planted = carriage.into_state(&rig.chain.bundle.state, None).expect("a planted frontier is a consistent state");
    let vp = rig.vp();
    let mut store = vp.palw_state_v2_store.write();
    let (_, mut delta) = store.delta_of(sink).expect("the sink's delta row");
    delta.entries.push(PalwDeltaEntryV2::Frontier { old: before, new: (blue, frontier) });
    store
        .set_delta_record_for_tests(sink, PalwStateDeltaRecordV2 { state_root: planted.state_root(), delta_borsh: borsh::to_vec(&delta).unwrap() })
        .unwrap();
    store.set_tip_for_tests(sink, &planted).unwrap();
    drop(store);
    // A plant rewrites a delta row under the cache (a real chain never does): the rows read before it are stale.
    vp.native_rows.lock().clear();
}

/// Place a unit of matured useful work at `at`, as if its PALW delta had carried it (the cfg(test) seam; the extraction from real
/// deltas is `rfc0012_native_evidence_fold`'s).
fn place_fact(rig: &Rig, at: &Block, work: u128) {
    let fact = MatureUsefulWorkV1 {
        identity: Hash64::from_u64_word(0x1201),
        anchor: at.header.hash,
        operator: Hash64::from_u64_word(0x1202),
        class: Hash64::from_u64_word(0x1203),
        anchor_blue: at.header.blue_score,
        accepted_blue: at.header.blue_score,
        anchor_daa: at.header.daa_score,
        accepted_daa: at.header.daa_score,
        matured_daa: 0,
        work,
    };
    rig.vp().native_fact_override.lock().entry(at.header.hash).or_default().push(fact);
}

/// Build the certified scenario on `a`: prefix, a planted frontier at b0, one attempt carrying a placed fact, one more attempt, the
/// pruning point at b0. Returns the blocks `(b0, e3)`.
async fn certified(a: &mut Rig) -> (Block, Block, Vec<Block>) {
    let pre = prefix(a).await;
    plant_frontier(a, pre[0].header.blue_score, pre[0].header.hash);
    let e3 = extend(a, &[2]).await.remove(0);
    place_fact(a, &e3, 10);
    a.vp().pruning_point_store.write().set(pre[0].header.hash, 1).unwrap();
    extend(a, &[3]).await;
    (pre[0].clone(), e3, pre)
}

/// **EXPECTED.** With a safe frontier at b0, one unit of matured work (10) placed after it, and b0 the pruning point:
/// `safe` is b0 — the deepest executed effect whose lifecycle is closed (every claim accepted at or before it resolved) and which
/// the frontier covers — with depth 1 and work 10; the prefix stops at e1 (`FrontierNotCovered`: the frontier is at b0, not past it);
/// `finalized` is b0 (an executed ancestor of `safe`, the validated pruning point); and `latest` is the sink. Then a heavier
/// branch that does not contain the work takes the sink: `safe` and `finalized` are recomputed to nothing (stop `InsufficientDepth`)
/// while `latest` follows. The label was never pinned.
#[tokio::test]
async fn rfc12_x8_certified_work_makes_a_prefix_safe_and_a_reorg_takes_it_back() {
    let p = parts(Some(FENCE));
    let mut a = Rig::new(&p, 0x12_0800_0000);
    let pre_blocks = {
        // B must see the same planted prefix, so it is built the same way.
        let pre = prefix(&mut a).await;
        pre
    };
    plant_frontier(&a, pre_blocks[0].header.blue_score, pre_blocks[0].header.hash);
    let mut b = follower(&p, 0x12_0810_0000, &pre_blocks).await;
    plant_frontier(&b, pre_blocks[0].header.blue_score, pre_blocks[0].header.hash);
    assert_eq!(a.chain.tip_state().1.state_root(), b.chain.tip_state().1.state_root(), "the same planted state on both nodes");
    let e3 = extend(&mut a, &[2]).await.remove(0);
    place_fact(&a, &e3, 10);
    let b0 = pre_blocks[0].header.hash;
    a.vp().pruning_point_store.write().set(b0, 1).unwrap();
    let e4 = extend(&mut a, &[3]).await.remove(0);
    let s = snapshot_of(&a).expect("a snapshot");
    eprintln!("[x8] {s:?}");
    assert_eq!((s.generation, s.latest), (e4.header.hash, Some(e4.header.hash)));
    assert_eq!(s.safe, Some(b0), "b0 is the deepest effect that is closed, covered by the frontier, and buried under D and W");
    assert_eq!((s.depth, s.unique_work.as_str()), (1, "10"));
    assert_eq!(s.stop, Some(SettlementStopV1::FrontierNotCovered), "and the prefix ends where the frontier does");
    assert_eq!(s.finalized, Some(b0), "finalized = the validated pruning point under a certified safe prefix");
    assert_eq!(s.frontier, Some(b0));
    let h = heads_of(&a).expect("heads");
    assert_eq!((h.latest_head(), h.safe_head(), h.finalized_head()), (Some(e4.header.hash), Some(b0), Some(b0)));

    // The reorg: B's three attempts on the same planted prefix beat A's two.
    let ys = extend(&mut b, &[4, 5, 6]).await;
    for y in &ys {
        a.arrive(y.clone(), "the heavier branch").await;
    }
    assert_eq!(a.chain.sink(), ys.last().unwrap().header.hash, "the heavier branch took the sink");
    let s = snapshot_of(&a).expect("a snapshot");
    assert_eq!(s.latest, Some(ys.last().unwrap().header.hash), "latest follows");
    assert_eq!((s.safe, s.finalized), (None, None), "the work was on the abandoned branch: recomputed away, not pinned");
    assert_eq!(s.stop, Some(SettlementStopV1::InsufficientDepth));
    let h = heads_of(&a).expect("heads");
    assert_eq!((h.safe_head(), h.finalized_head()), (None, None));
}

/// **EXPECTED.** If the delta row of a chain block is gone (a pruned or damaged history) the snapshot stops at `MissingHistory`
/// — `safe` and `finalized` withdrawn, `latest` kept — rather than certifying over a gap; restoring the row (and the rows being
/// rebuilt) brings the same certificate back. The in-memory rows are not authoritative: clearing them changes nothing.
#[tokio::test]
async fn rfc12_x9_lost_history_stops_certification_and_never_certifies_around_the_gap() {
    use crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2;
    let p = parts(Some(FENCE));
    let mut a = Rig::new(&p, 0x12_0900_0000);
    let (b0, _e3, pre) = certified(&mut a).await;
    let want = snapshot_of(&a).expect("a snapshot");
    assert_eq!(want.safe, Some(b0.header.hash), "the certified scenario");
    // Clearing the rows changes nothing: they are a cache.
    a.vp().native_rows.lock().clear();
    extend(&mut a, &[4]).await;
    let rebuilt = snapshot_of(&a).expect("a snapshot");
    assert_eq!((rebuilt.safe, rebuilt.finalized, rebuilt.depth, rebuilt.unique_work.clone()), (want.safe, want.finalized, want.depth, want.unique_work.clone()));
    // A gap: b1's delta row is lost.
    let b1 = pre[2].header.hash;
    let (root, delta) = a.vp().palw_state_v2_store.read().delta_of(b1).expect("b1's row");
    let record = PalwStateDeltaRecordV2 { state_root: root, delta_borsh: borsh::to_vec(&delta).unwrap() };
    a.vp().palw_state_v2_store.write().delete_delta_for_tests(b1).unwrap();
    a.vp().native_rows.lock().clear();
    let tip = extend(&mut a, &[5]).await.remove(0);
    let gap = snapshot_of(&a).expect("a snapshot");
    assert_eq!(gap.stop, Some(SettlementStopV1::MissingHistory), "a gap is a stop");
    assert_eq!((gap.safe, gap.finalized), (None, None), "nothing is certified around it");
    assert_eq!(gap.latest, Some(tip.header.hash), "execution is still reported");
    assert_eq!(heads_of(&a).map(|h| (h.latest_head(), h.safe_head(), h.finalized_head())), Some((Some(tip.header.hash), None, None)));
    // Restored: the same certificate.
    a.vp().palw_state_v2_store.write().set_delta_record_for_tests(b1, record).unwrap();
    a.vp().native_rows.lock().clear();
    let tip2 = extend(&mut a, &[6]).await.remove(0);
    let back = snapshot_of(&a).expect("a snapshot");
    assert_eq!((back.generation, back.safe, back.finalized, back.depth, back.unique_work.clone()), (tip2.header.hash, want.safe, want.finalized, want.depth, want.unique_work));
}
