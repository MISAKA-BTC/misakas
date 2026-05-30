use crate::{consensus::test_consensus::TestConsensus, model::services::reachability::ReachabilityService};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashSet,
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockhash,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    config::{
        ConfigBuilder,
        params::{DEVNET_PARAMS, MAINNET_PARAMS},
    },
    tx::{ScriptPublicKey, ScriptVec, Transaction},
};
use std::{collections::VecDeque, thread::JoinHandle};

struct OnetimeTxSelector {
    txs: Option<Vec<Transaction>>,
}

impl OnetimeTxSelector {
    fn new(txs: Vec<Transaction>) -> Self {
        Self { txs: Some(txs) }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.txs.take().unwrap()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        unimplemented!()
    }

    fn is_successful(&self) -> bool {
        true
    }
}

struct TestContext {
    consensus: TestConsensus,
    join_handles: Vec<JoinHandle<()>>,
    miner_data: MinerData,
    simulated_time: u64,
    current_templates: VecDeque<BlockTemplate>,
    current_tips: BlockHashSet,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.consensus.shutdown(std::mem::take(&mut self.join_handles));
    }
}

impl TestContext {
    fn new(consensus: TestConsensus) -> Self {
        let join_handles = consensus.init();
        let genesis_hash = consensus.params().genesis.hash;
        let simulated_time = consensus.params().genesis.timestamp;
        Self {
            consensus,
            join_handles,
            miner_data: new_miner_data(),
            simulated_time,
            current_templates: Default::default(),
            current_tips: BlockHashSet::from_iter([genesis_hash]),
        }
    }

    pub fn build_block_template_row(&mut self, nonces: impl Iterator<Item = usize>) -> &mut Self {
        for nonce in nonces {
            self.simulated_time += self.consensus.params().target_time_per_block();
            self.current_templates.push_back(self.build_block_template(nonce as u64, self.simulated_time));
        }
        self
    }

    pub fn assert_row_parents(&mut self) -> &mut Self {
        for t in self.current_templates.iter() {
            assert_eq!(self.current_tips, BlockHashSet::from_iter(t.block.header.direct_parents().iter().copied()));
        }
        self
    }

    pub async fn validate_and_insert_row(&mut self) -> &mut Self {
        self.current_tips.clear();
        while let Some(t) = self.current_templates.pop_front() {
            self.current_tips.insert(t.block.header.hash);
            self.validate_and_insert_block(t.block.to_immutable()).await;
        }
        self
    }

    pub async fn build_and_insert_disqualified_chain(&mut self, mut parents: Vec<BlockHash>, len: usize) -> BlockHash {
        // The chain will be disqualified since build_block_with_parents builds utxo-invalid blocks
        for _ in 0..len {
            self.simulated_time += self.consensus.params().target_time_per_block();
            let b = self.build_block_with_parents(parents, 0, self.simulated_time);
            parents = vec![b.header.hash];
            self.validate_and_insert_block(b.to_immutable()).await;
        }
        parents[0]
    }

    pub fn build_block_template(&self, nonce: u64, timestamp: u64) -> BlockTemplate {
        let mut t = self
            .consensus
            .build_block_template(
                self.miner_data.clone(),
                Box::new(OnetimeTxSelector::new(Default::default())),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        t.block.header.timestamp = timestamp;
        t.block.header.nonce = nonce;
        t.block.header.finalize();
        t
    }

    pub fn build_block_with_parents(&self, parents: Vec<BlockHash>, nonce: u64, timestamp: u64) -> MutableBlock {
        let mut b = self.consensus.build_block_with_parents_and_transactions(blockhash::NONE, parents, Default::default());
        b.header.timestamp = timestamp;
        b.header.nonce = nonce;
        b.header.finalize(); // This overrides the NONE hash we passed earlier with the actual hash
        b
    }

    pub async fn validate_and_insert_block(&mut self, block: Block) -> &mut Self {
        let status = self.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        self
    }

    pub fn assert_tips(&mut self) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()), self.current_tips);
        self
    }

    pub fn assert_tips_num(&mut self, expected_num: usize) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()).len(), expected_num);
        self
    }

    pub fn assert_virtual_parents_subset(&mut self) -> &mut Self {
        assert!(self.consensus.get_virtual_parents().is_subset(&self.current_tips));
        self
    }

    pub fn assert_valid_utxo_tip(&mut self) -> &mut Self {
        // Assert that at least one body tip was resolved with valid UTXO
        assert!(self.consensus.body_tips().iter().copied().any(|h| self.consensus.block_status(h) == BlockStatus::StatusUTXOValid));
        self
    }
}

#[tokio::test]
async fn template_mining_sanity_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let rounds = 10;
    let width = 3;
    for _ in 0..rounds {
        ctx.build_block_template_row(0..width)
            .assert_row_parents()
            .validate_and_insert_row()
            .await
            .assert_tips()
            .assert_virtual_parents_subset()
            .assert_valid_utxo_tip();
    }
}

#[tokio::test]
async fn antichain_merge_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Build a large 32-wide antichain
    ctx.build_block_template_row(0..32)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_utxo_tip();

    // Mine a long enough chain s.t. the antichain is fully merged
    for _ in 0..32 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

/// kaspa-pq Phase 10/11 (ADR-0009/0013): first overlay-ACTIVE integration
/// test. With `dns_params = Some` and `dns_activation_daa_score = 0`, the
/// validator-reward code paths that are dormant on every shipping network —
/// the per-block `ActiveBondView` walk, the §B.4 eligibility check, the
/// coinbase reward fan-out (construction + validation), the cross-block
/// uniqueness walk over the rewarded-keys store, and the template
/// ineligible-shard pre-filter — all RUN here (with empty data, since this
/// chain carries no bonds or attestations). The chain must still mine and
/// validate to a valid UTXO tip, proving that activating the overlay does not
/// break block production or validation and that the empty-reward coinbase is
/// reproduced byte-for-byte by the validation path.
///
/// (A full reward-bearing e2e — a real bond tx, an ML-DSA-signed attestation,
/// and a non-empty reward coinbase — needs funded UTXO-valid overlay txs and
/// is a separate harness effort; the reward/eligibility/uniqueness logic is
/// already unit-tested.)
#[tokio::test]
async fn dns_overlay_active_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            // Activate the DNS overlay from genesis (reuse the self-consistent
            // devnet DNS parameters, with activation pulled down to 0).
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine + validate a chain with the overlay active end-to-end.
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

// ============================================================================
// kaspa-pq ADR-0018 §G — DNS-overlay DAG integration harness (foundation).
//
// Retires the "ML-DSA-65 signing unavailable in the consensus test crate"
// blocker for the reward-bearing / reorg / slashing DAG tests (DAG-2..7): these
// helpers let a consensus test build stake-bond + attestation-shard txs and
// produce an attestation signature the §B.4 verifier
// (`kaspa_txscript::verify_mldsa65_with_context` under
// `ATTESTATION_MLDSA65_CONTEXT`) accepts. Funding a bond tx from a coinbase UTXO
// (so a full reward-bearing chain validates) is the next harness step (DAG-2).
// ============================================================================
#[cfg(test)]
mod dns_harness {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            ATTESTATION_MLDSA65_CONTEXT, DNS_PAYLOAD_VERSION_V1, StakeAttestation, StakeBondPayload,
            attestations_from_accepted_txs, single_attestation_shard, stake_attestation_message, stake_attestation_shard_tx,
            validator_id_from_pubkey,
        },
        subnets::{SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND},
        tx::{Transaction, TransactionOutpoint},
    };
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// A test validator: an ML-DSA-65 key (re-derived deterministically from
    /// `seed`) plus its 1952-byte pubkey and overlay `validator_id`.
    pub(super) struct HarnessValidator {
        pub seed: [u8; 32],
        pub pubkey: Vec<u8>,
        pub validator_id: Hash64,
    }

    pub(super) fn harness_validator(seed: [u8; 32]) -> HarnessValidator {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let validator_id = validator_id_from_pubkey(&pubkey);
        HarnessValidator { seed, pubkey, validator_id }
    }

    /// Build a stake-bond tx (subnetwork `SUBNETWORK_ID_STAKE_BOND`, payload =
    /// borsh `StakeBondPayload`). The funded variant (output-0 = `amount` locked
    /// stake spent from a coinbase UTXO) is the next step; here the tx is
    /// payload-first for shape / borsh checks.
    pub(super) fn build_stake_bond_tx(v: &HarnessValidator, amount: u64, activation_daa_score: u64, reward_payload: [u8; 32]) -> Transaction {
        let payload = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: v.validator_id,
            validator_pubkey_hash: v.validator_id,
            validator_pubkey: v.pubkey.clone(),
            amount,
            activation_daa_score,
            unbonding_period_blocks: 700,
            owner_reward_spk_payload: reward_payload,
        };
        Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_BOND, 0, borsh::to_vec(&payload).unwrap())
    }

    /// Build a fully ML-DSA-65-signed attestation for `bond_outpoint`, signing
    /// exactly the digest the §B.4 verifier reconstructs.
    pub(super) fn build_signed_attestation(
        v: &HarnessValidator,
        network_id: &[u8],
        bond_outpoint: TransactionOutpoint,
        epoch: u64,
        target_hash: Hash64,
        target_daa_score: u64,
        validator_set_commitment: Hash64,
    ) -> StakeAttestation {
        let msg = stake_attestation_message(network_id, epoch, target_hash, target_daa_score, validator_set_commitment, bond_outpoint);
        let mb = msg.as_bytes();
        let kp = mldsa::generate_key_pair(v.seed);
        let sig = mldsa::sign(&kp.signing_key, &mb[..], ATTESTATION_MLDSA65_CONTEXT, [0x55u8; 32]).expect("ml-dsa-65 sign");
        StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: v.validator_id,
            bond_outpoint,
            epoch,
            target_hash,
            target_daa_score,
            validator_set_commitment,
            signature: sig.as_ref().to_vec(),
        }
    }

    /// DAG-harness foundation (ADR-0018 §G): a consensus test can build overlay
    /// txs and produce an attestation signature the §B.4 verifier accepts.
    #[test]
    fn dns_harness_signs_attestations_the_verifier_accepts() {
        let v = harness_validator([0x11u8; 32]);
        assert_eq!(v.pubkey.len(), 2592);
        assert_eq!(v.validator_id, validator_id_from_pubkey(&v.pubkey));

        // Stake-bond tx shape + payload round-trip; validator_pubkey_hash binds the pubkey.
        let bond_tx = build_stake_bond_tx(&v, 10_000_000_000, 0, [0x33u8; 32]);
        assert_eq!(bond_tx.subnetwork_id, SUBNETWORK_ID_STAKE_BOND);
        let bond_outpoint = TransactionOutpoint::new(bond_tx.id(), 0);
        let decoded: StakeBondPayload = borsh::from_slice(&bond_tx.payload).unwrap();
        assert_eq!(decoded.amount, 10_000_000_000);
        assert_eq!(decoded.validator_pubkey_hash, validator_id_from_pubkey(&decoded.validator_pubkey));

        // Signed attestation: the §B.4 verifier (txscript) must accept it.
        let net_id = [0xabu8; 32];
        let target_hash = Hash64::from_bytes([0x44u8; 64]);
        let vsc = Hash64::from_bytes([0x22u8; 64]);
        let att = build_signed_attestation(&v, &net_id, bond_outpoint, 7, target_hash, 700, vsc);
        let msg = stake_attestation_message(&net_id, att.epoch, att.target_hash, att.target_daa_score, att.validator_set_commitment, att.bond_outpoint);
        let mb = msg.as_bytes();
        assert!(
            kaspa_txscript::verify_mldsa65_with_context(&v.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA65_CONTEXT).unwrap(),
            "the §B.4 verifier must accept the harness-signed attestation"
        );
        // A different key must NOT verify (sanity).
        let v2 = harness_validator([0x99u8; 32]);
        assert!(!kaspa_txscript::verify_mldsa65_with_context(&v2.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA65_CONTEXT).unwrap());

        // Shard tx wraps exactly one extractable attestation.
        let shard_tx = stake_attestation_shard_tx(&single_attestation_shard(att));
        assert_eq!(shard_tx.subnetwork_id, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD);
        assert_eq!(attestations_from_accepted_txs(std::slice::from_ref(&shard_tx)).len(), 1);
    }
}

#[tokio::test]
async fn basic_utxo_disqualified_test() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    // Mine a longer disqualified chain
    let disqualified_tip = ctx.build_and_insert_disqualified_chain(vec![config.genesis.hash], 20).await;

    assert_ne!(sink, disqualified_tip);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(BlockHashSet::from_iter([sink, disqualified_tip]), BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter()));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip));
}

#[tokio::test]
async fn double_search_disqualified_test() {
    // TODO: add non-coinbase transactions and concurrency in order to complicate the test

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.min_difficulty_window_size = p.difficulty_window_size;
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine 3 valid blocks over genesis
    ctx.build_block_template_row(0..3)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_utxo_tip();

    // Mark the one expected to remain on virtual chain
    let original_sink = ctx.consensus.get_sink();

    // Find the roots to be used for the disqualified chains
    let mut virtual_parents = ctx.consensus.get_virtual_parents();
    assert!(virtual_parents.remove(&original_sink));
    let mut iter = virtual_parents.into_iter();
    let root_1 = iter.next().unwrap();
    let root_2 = iter.next().unwrap();
    assert_eq!(iter.next(), None);

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    assert!(ctx.consensus.reachability_service().is_chain_ancestor_of(original_sink, sink));

    // Mine a long disqualified chain
    let disqualified_tip_1 = ctx.build_and_insert_disqualified_chain(vec![root_1], 30).await;

    // And another shorter disqualified chain
    let disqualified_tip_2 = ctx.build_and_insert_disqualified_chain(vec![root_2], 20).await;

    assert_eq!(ctx.consensus.get_block_status(root_1), Some(BlockStatus::StatusUTXOValid));
    assert_eq!(ctx.consensus.get_block_status(root_2), Some(BlockStatus::StatusUTXOValid));

    assert_ne!(sink, disqualified_tip_1);
    assert_ne!(sink, disqualified_tip_2);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(
        BlockHashSet::from_iter([sink, disqualified_tip_1, disqualified_tip_2]),
        BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter())
    );
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_1));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_2));

    // Mine a long enough valid chain s.t. both disqualified chains are fully merged
    for _ in 0..30 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

fn new_miner_data() -> MinerData {
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rand::thread_rng();
    let (_sk, pk) = secp.generate_keypair(&mut rng);
    let script = ScriptVec::from_slice(&pk.serialize());
    MinerData::new(ScriptPublicKey::new(0, script), vec![])
}
