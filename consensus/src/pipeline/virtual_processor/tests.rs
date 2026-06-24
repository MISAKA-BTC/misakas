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
    dns_finality::p2pkh_mldsa87_spk,
    tx::{Transaction, TransactionOutpoint},
};
use std::{collections::VecDeque, thread::JoinHandle};

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
        // First call returns the fixed set; subsequent calls (the builder's
        // rejection re-selection loop) return empty so the loop terminates
        // instead of unwrapping `None`.
        self.txs.take().unwrap_or_default()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        // Record the rejection so `is_successful` reports failure and
        // `build_block_template` surfaces the per-tx `RuleError` (instead of
        // panicking or silently dropping the tx).
        self.rejected = true;
    }

    fn is_successful(&self) -> bool {
        !self.rejected
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

    /// kaspa-pq ADR-0018 §G (DAG-2 harness): build ONE block from a template with a
    /// custom `miner_data` (so the coinbase can pay a known, spendable key) and a
    /// custom tx set fed through `OnetimeTxSelector` (so the coinbase is computed
    /// correctly and the block can reach a valid UTXO tip — unlike
    /// `build_block_with_parents_and_transactions`, which builds a utxo-invalid
    /// coinbase). Parents are auto-selected from the current virtual tips, so the
    /// caller just mines a linear chain. Returns the inserted (immutable) block so
    /// the caller can read its coinbase outputs / daa score. NOTE: an invalid tx in
    /// `txs` makes the template builder call `OnetimeTxSelector::reject_selection`,
    /// which panics — i.e. an invalid funded spend fails loudly here.
    pub async fn mine_block(&mut self, miner_data: MinerData, txs: Vec<Transaction>) -> Block {
        self.simulated_time += self.consensus.params().target_time_per_block();
        let mut t = self
            .consensus
            .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
            .unwrap();
        t.block.header.timestamp = self.simulated_time;
        t.block.header.nonce = self.simulated_time;
        t.block.header.finalize();
        let block = t.block.to_immutable();
        self.validate_and_insert_block(block.clone()).await;
        block
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

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 2): overlay **and** v2-economics ACTIVE integration
/// test. With `dns_activation_daa_score = 0` AND `pos_v2_activation_daa_score = 0` (plus shrunk
/// windows so epochs actually bury within a short chain), the full v2 machinery RUNS on every
/// block: the fence-gated 70/30 participation/quality split, the per-block quality-pool
/// persistence (`block_quality_pool_store`, written non-empty here since the §F carve funds a
/// validator pool even with no attestations), the per-epoch accumulator recompute + finalization
/// (`update_epoch_accumulator`), and the deferred quality-bonus payout
/// (`deferred_quality_bonus_outputs` — incl. the finalization-crossing detection and the φS gate).
///
/// This chain carries no bonds/attestations, so every *reward* set is empty — but the code paths
/// execute, write the stores, and the chain must still mine and validate to a valid UTXO tip.
/// Because the validation path rebuilds the coinbase and rejects any mismatch, reaching a valid
/// UTXO tip proves the v2 economics neither break block production nor desynchronise coinbase
/// construction vs validation. (A reward-bearing e2e — real bonds + attestations paid a non-empty
/// bonus — needs the funded-bond DAG harness, DAG-2.)
#[tokio::test]
async fn pos_v2_active_empty_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Activate the v2 economics from genesis and shrink the finalization window
            // (= reward_uniqueness_window + max_reorg_horizon = 4) so epochs bury and the
            // deferred-bonus crossing fires within a short chain.
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // threshold(E) = (E+1)·2 + 4, so by ~daa 12 several epochs have finalized and the deferred
    // quality-bonus path has fired (with empty included sets — exercised, not paid).
    for _ in 0..12 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx.assert_tips_num(1);
}

/// kaspa-pq ADR-0018 §G DAG-2 (funded-bond milestone — retires the "fund a bond
/// from a coinbase UTXO" wall). With the overlay + v2 economics ACTIVE, a real
/// ML-DSA-87 keypair mines a coinbase; after maturity its output is SPENT into a
/// funded stake-bond tx (output-0 = locked stake, input-0 signed over the v2 tx
/// sighash under `MLDSA87_TX_CONTEXT`). The block carrying the bond must reach a
/// valid UTXO tip — proving the script engine (`OpCheckSigMlDsa87`) accepts the
/// real ML-DSA-87 P2PKH spend through full consensus validation, the precondition
/// for the reward-bearing / slashing DAG e2e (DAG-2..6).
#[tokio::test]
async fn pos_v2_funded_bond_chain_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            // Shrink coinbase maturity so the funding coinbase is spendable within a short chain.
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A known validator/funding key; its coinbase P2PKH spk.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // 1) Mine a run of blocks whose coinbase pays K. In Kaspa a block's coinbase
    //    rewards the blocks it MERGES (each merged block's reported miner script),
    //    not its own miner — so K's reward for the funding block b1 (which merges
    //    only genesis → 0 reward) appears in the coinbase of the block that merges
    //    b1 (the harvest block b2).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let harvest = ctx.mine_block(k_miner.clone(), vec![]).await;
    let coinbase = &harvest.transactions[0];
    let coinbase_id = coinbase.id();
    let coinbase_daa = harvest.header.daa_score;
    let (idx, out) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_public_key == k_spk)
        .expect("the harvest coinbase must pay the known key");
    let coinbase_outpoint = TransactionOutpoint::new(coinbase_id, idx as u32);
    let coinbase_value = out.value;
    assert!(coinbase_value > 200_000, "coinbase value must cover the bond + fee");

    // 2) Mine filler blocks so the harvested coinbase matures (coinbase_maturity = 2).
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // 3) Spend the matured coinbase into a funded, ML-DSA-87-signed stake-bond tx.
    let amount = coinbase_value - 100_000; // small fee; bond almost the whole coinbase
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _validator_id, _reward_payload) = dns_harness::funded_signed_bond_tx(
        seed,
        coinbase_outpoint,
        coinbase_value,
        coinbase_daa,
        amount,
        0,
        storage_mass_parameter,
    );
    let bond_tx_id = bond_tx.id();

    // 4) Mine the block carrying the bond tx; it must reach a valid UTXO tip.
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert!(
        bond_block.transactions.iter().any(|t| t.id() == bond_tx_id),
        "the funded stake-bond tx must be included in the block"
    );
    assert_eq!(
        ctx.consensus.block_status(bond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying the funded ML-DSA-87 stake-bond spend must be UTXO-valid (construction == validation)"
    );
    ctx.assert_valid_utxo_tip();
}

/// ADR-0018 §F bridge wiring — one scenario of the finality-fee e2e: fund a key,
/// spend its matured coinbase into a deposit-lock tx (fee 100_000), mine it, then
/// harvest the next block's coinbase. Returns `(worker_output_value,
/// lock_block_subsidy)` — the worker payout for the block that carried the bridge tx,
/// and that block's subsidy (parsed from its coinbase payload: blue_score u64 LE ‖
/// subsidy u64 LE ‖ …).
///
/// `evm_active` toggles `evm_activation_daa_score` (0 vs u64::MAX). evm-active
/// templates COMMIT to the header timestamp (the EVM execution env derives from it),
/// so in that mode blocks are inserted exactly as templated — no
/// `TestContext::mine_block` timestamp/nonce mutation (the same insertion pattern the
/// EVM lane e2e tests use); the inert mode exercises the ordinary v1 mine path.
async fn finality_fee_bridge_scenario(finality_fence: u64, evm_active: bool) -> (u64, u64) {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // Shrink coinbase maturity so the funding coinbase is spendable quickly.
            p.coinbase_maturity = 2;
            // The classification is doubly gated: the §F fence AND EVM-lane activation
            // (the bridge only exists on an EVM-active net). MAINNET_PARAMS is EVM-inert
            // (u64::MAX) by default.
            p.evm_activation_daa_score = if evm_active { 0 } else { u64::MAX };
            let mut dns = p.dns_params.clone().unwrap();
            dns.finality_fee_activation_daa_score = finality_fence;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Insert one block: template-as-is when evm-active (timestamp is commitment-bound),
    // else the ordinary simulated-time mine path.
    async fn mine(ctx: &mut TestContext, evm_active: bool, miner: MinerData, txs: Vec<Transaction>) -> Block {
        if evm_active {
            let t = ctx
                .consensus
                .build_block_template(miner, Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
                .unwrap();
            let block = t.block.to_immutable();
            ctx.validate_and_insert_block(block.clone()).await;
            block
        } else {
            ctx.mine_block(miner, txs).await
        }
    }

    // Fund: harvest a coinbase paying the known key K (a block's coinbase rewards
    // the blocks it MERGES, so K's reward for b1 appears in the harvest block).
    let seed = [0x5Au8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = mine(&mut ctx, evm_active, k_miner.clone(), vec![]).await;
    let harvest = mine(&mut ctx, evm_active, k_miner.clone(), vec![]).await;
    let coinbase = &harvest.transactions[0];
    let (idx, out) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_public_key == k_spk)
        .expect("the harvest coinbase must pay the known key");
    let coinbase_outpoint = TransactionOutpoint::new(coinbase.id(), idx as u32);
    let (coinbase_value, coinbase_daa) = (out.value, harvest.header.daa_score);
    assert!(coinbase_value > 200_000, "coinbase must cover the lock + fee");

    // Mature the coinbase (coinbase_maturity = 2).
    for _ in 0..5 {
        mine(&mut ctx, evm_active, new_miner_data(), vec![]).await;
    }

    // The bridge tx: matured coinbase → one EVM_DEPOSIT_LOCK output, fee 100_000.
    let lock_tx = dns_harness::funded_signed_deposit_lock_tx(
        seed,
        coinbase_outpoint,
        coinbase_value,
        coinbase_daa,
        ctx.consensus.params().storage_mass_parameter,
    );
    let lock_tx_id = lock_tx.id();

    // Mine it under a distinct miner spk so its worker payout is findable.
    let lock_miner_spk = p2pkh_mldsa87_spk(&[0x33u8; 64]);
    let lock_block = mine(&mut ctx, evm_active, MinerData::new(lock_miner_spk.clone(), vec![]), vec![lock_tx]).await;
    assert!(lock_block.transactions.iter().any(|t| t.id() == lock_tx_id), "the bridge tx must be included");
    assert_eq!(
        ctx.consensus.block_status(lock_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying the deposit-lock tx must be UTXO-valid (construction == validation)"
    );
    // The lock block's subsidy, from its coinbase payload (blue_score ‖ subsidy ‖ …).
    let payload = &lock_block.transactions[0].payload;
    let subsidy = u64::from_le_bytes(payload[8..16].try_into().unwrap());

    // Harvest: the next block's coinbase pays the lock block's worker share.
    let harvest2 = mine(&mut ctx, evm_active, new_miner_data(), vec![]).await;
    ctx.assert_valid_utxo_tip();
    let worker_out = harvest2.transactions[0]
        .outputs
        .iter()
        .find(|o| o.script_public_key == lock_miner_spk)
        .expect("the next coinbase must pay the lock block's miner")
        .value;
    (worker_out, subsidy)
}

/// kaspa-pq ADR-0018 §F bridge wiring e2e (EVM-active net): an accepted L1 tx that
/// CREATES an `EVM_DEPOSIT_LOCK` output (ADR-0020 §9.2 bridge deposit) is
/// **finality-class** — its fee is split at the validator-primary finality ratios
/// (Worker 25%) instead of the normal-tx ratios (Worker 90%) — through the REAL
/// template→validate coinbase path over a chain (classification at
/// `calculate_utxo_state`, payout via `expected_coinbase_transaction`; every mined
/// block reaching `StatusUTXOValid` proves construction == validation). The fenced
/// twin (`finality_fee_activation_daa_score = u64::MAX`) runs the identical chain
/// shape and pays the Worker the normal 90% — the exact pre-wiring math — proving the
/// §F fence isolates the change. evm-feature-gated: an evm-active template requires
/// the executor (a non-evm build refuses evm-active blocks by design).
#[tokio::test]
#[cfg(feature = "evm")]
async fn finality_fee_bridge_tx_pays_validator_primary_split() {
    use kaspa_consensus_core::dns_finality::{split_block_subsidy, split_finality_fees, split_normal_tx_fees};
    kaspa_core::log::try_init_logger("info");

    // Active fence (0, the PRODUCTION preset value): the 100_000 bridge fee splits at
    // the finality ratios — the Worker gets 25%, the Validator share (75%) funds the
    // §E pool (don't-mint burned here: no bonded validators).
    let (worker_active, subsidy_a) = finality_fee_bridge_scenario(0, true).await;
    // Inert §F fence: the same chain shape pays the normal-tx 90% — the pre-wiring math.
    let (worker_inert, subsidy_b) = finality_fee_bridge_scenario(u64::MAX, true).await;
    assert_eq!(subsidy_a, subsidy_b, "identical chain shape ⇒ identical lock-block subsidy");

    let dns = MAINNET_PARAMS.dns_params.clone().unwrap();
    let fs = &dns.reward_params.fee_split;
    let worker_base = split_block_subsidy(subsidy_a, fs).worker_base_sompi;
    assert_eq!(
        worker_active,
        worker_base + split_finality_fees(100_000, fs).worker_sompi,
        "bridge-tx fee pays the Worker the FINALITY share (25%)"
    );
    assert_eq!(
        worker_inert,
        worker_base + split_normal_tx_fees(100_000, fs).worker_sompi,
        "below the §F fence the same fee pays the Worker the NORMAL share (90%) — byte-identical to pre-wiring"
    );
    assert_eq!(
        worker_inert - worker_active,
        split_normal_tx_fees(100_000, fs).worker_sompi - split_finality_fees(100_000, fs).worker_sompi,
        "the Worker delta is exactly the normal→finality reclassification (the Validator gains it)"
    );
}

/// kaspa-pq ADR-0018 §F bridge wiring — the EVM-activation gate: deposit-lock OUTPUTS
/// are consensus-legal on every net (the output-class exemption is unconditional),
/// but on an EVM-INERT net (`evm_activation_daa_score = u64::MAX` — mainnet today)
/// the classification must NOT fire even with the §F fence at 0: the lock-bearing
/// tx's fee stays normal-class (Worker 90%), byte-identical to the pre-wiring math.
/// Without this gate a miner on an EVM-inert net could self-include a never-claimable
/// lock tx and reroute fees into the §E pool. Runs on the default (non-evm) build —
/// inert nets produce ordinary v1 blocks.
#[tokio::test]
async fn finality_fee_inert_on_evm_inert_net() {
    use kaspa_consensus_core::dns_finality::{split_block_subsidy, split_normal_tx_fees};
    kaspa_core::log::try_init_logger("info");

    // §F fence ACTIVE (0, the production value) but the EVM lane INERT.
    let (worker_out, subsidy) = finality_fee_bridge_scenario(0, false).await;
    let dns = MAINNET_PARAMS.dns_params.clone().unwrap();
    let fs = &dns.reward_params.fee_split;
    assert_eq!(
        worker_out,
        split_block_subsidy(subsidy, fs).worker_base_sompi + split_normal_tx_fees(100_000, fs).worker_sompi,
        "on an EVM-inert net a lock-bearing tx's fee stays NORMAL-class (Worker 90%) — the EVM gate holds"
    );
}

/// kaspa-pq ADR-0018 §G DAG-2 (reward-bearing e2e): the full overlay + v2 reward
/// path over a real BlockDAG. A funded ML-DSA-87 bond is created (as in the funding
/// milestone), then the validator ML-DSA-signs a recent attestation; the block that
/// includes the attestation shard must pay the validator a non-empty §E
/// participation reward in its coinbase AND validate to a UTXO-valid tip — proving
/// the reward fan-out (eligibility → distribution → coinbase) is
/// construction == validation with real bonds + attestations, not just by unit test.
#[tokio::test]
async fn pos_v2_reward_bearing_attestation_validates() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            // Reward recency must comfortably cover the canonical anchor, which is buried by
            // attestation_lag + backoff below the tip (blue_score ~ DAA on this linear chain).
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            // DNS v3 blue_score epochs: small so an epoch buries within this short chain.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A known validator/funding key.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: b1 funds, then two harvest blocks — h_a pays K for b1, h_b pays K for h_a.
    // coinbase_a funds the bond; coinbase_b funds the attestation shard tx (a 0-input
    // shard tx is rejected by the isolation `NoTxInputs` check, so production funds it).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(bond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the bond block must be UTXO-valid"
    );
    assert_eq!(reward_payload, k_payload, "rewards pay back to K");

    // Bury several blue_score epochs past the bond so a ready, bond-active canonical anchor
    // exists — DNS v3 pays the §E reward only to an attestation naming the canonical anchor.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // The validator ML-DSA-signs the CANONICAL anchor for a ready epoch (DNS v3). net_id =
    // genesis hash (Addendum A.3); VSC is a domain-separation field only (P-1D zero).
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);

    // The block that includes the attestation shard pays the validator the §E
    // participation reward (to owner_reward_spk_payload == k_spk) and must validate.
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the attestation-including block must be UTXO-valid with a non-empty reward coinbase"
    );
    let reward_value = reward_block.transactions[0].outputs.iter().find(|o| o.script_public_key == k_spk).map(|o| o.value);
    assert!(
        reward_value.unwrap_or(0) > 0,
        "the coinbase must pay the validator a non-empty §E participation reward (got {reward_value:?})"
    );
}

/// kaspa-pq ADR-0018 §G (DAG-6): full-consensus equivocation-slashing e2e. A funded,
/// ML-DSA-87-signed bond goes active; the validator then EQUIVOCATES — two signed
/// attestations for the same `(bond, validator, epoch)` but DIFFERENT anchors — and a
/// `SlashingEvidence` tx carries both. The block including it must validate
/// (construction == validation), and as a consensus side-effect must REMOVE the locked
/// stake UTXO (the bond's output-0 leaves the supply) and MINT the reporter reward
/// (`slashing_reporter_reward_bps` = 10%) at `(slashing_tx, 0)`. This proves the slashing
/// economics end-to-end through `mine_block`/validate-and-insert, not just at the
/// `UtxoDiff` unit level (closes the audit's DAG-6 test gap).
#[tokio::test]
async fn pos_v2_slashing_evidence_removes_bond_and_pays_reporter() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // One validator/funding/reporter key suffices to exercise the slashing mechanism.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: h_a pays K (funds the bond), h_b pays K (funds the evidence tx).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // Fund + mine the bond — active from activation_daa_score = 0.
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(bond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the bond block must be UTXO-valid"
    );
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    assert!(
        ctx.consensus.get_virtual_utxos(None, 100_000, false).iter().any(|(o, _)| *o == bond_outpoint),
        "the bond's locked-stake UTXO must exist before slashing"
    );
    // Bury the bond so its record is committed into the active bond view the slashing
    // verifier reads (mirrors the burial the reward-bearing e2e does before attesting).
    let mut buried = Vec::new();
    for _ in 0..5 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Equivocation: two ML-DSA-87-signed attestations, same (bond, validator, epoch),
    // DIFFERENT target_hash (approving two conflicting anchors) — the punishable act.
    let net_id = ctx.consensus.params().genesis.hash;
    let epoch = 1u64;
    // A buried block's DAA: past the bond's (inclusion-set) activation so the bond is
    // Active at the target, and well within `evidence_window_blocks` of the including block.
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v, net_id.as_byte_slice(), bond_outpoint, epoch, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v, net_id.as_byte_slice(), bond_outpoint, epoch, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    // Sanity (localizes a signature/net_id mismatch vs a freshness/status rejection): both
    // attestations must verify under the net_id (genesis hash) the consensus slashing
    // verifier reconstructs the digest with.
    for att in [&att_a, &att_b] {
        let msg = kaspa_consensus_core::dns_finality::stake_attestation_message(
            net_id.as_byte_slice(),
            att.epoch,
            att.target_hash,
            att.target_daa_score,
            att.validator_set_commitment,
            att.bond_outpoint,
        );
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                &v.pubkey,
                &msg.as_bytes()[..],
                &att.signature,
                kaspa_consensus_core::dns_finality::ATTESTATION_MLDSA87_CONTEXT
            )
            .unwrap(),
            "attestation must self-verify under the consensus net_id"
        );
    }
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx =
        dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage_mass_parameter);
    let slash_tx_id = slash_tx.id();

    // The block including the slashing evidence must validate AND apply the side-effects.
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the slashing-evidence block must be UTXO-valid (construction == validation of the slashing side-effects)"
    );

    // Consensus side-effects: the locked stake is REMOVED and the reporter reward is minted.
    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!utxos.contains_key(&bond_outpoint), "the slashed bond's locked stake must be removed from the UTXO set");
    let reporter_mint = TransactionOutpoint::new(slash_tx_id, 0);
    let reporter_bps = ctx.consensus.params().dns_params.clone().unwrap().reward_params.slashing_reporter_reward_bps as u128;
    let expected_reporter = (bond_amount as u128 * reporter_bps / 10_000) as u64;
    let r = utxos.get(&reporter_mint).expect("the reporter reward must be minted at (slashing_tx, 0)");
    assert_eq!(r.amount, expected_reporter, "reporter reward = bond_amount * reporter_bps / 10000");
    assert_eq!(r.script_public_key, k_spk, "the reporter reward pays the declared reporter P2PKH");

    // ── Supply invariant (audit M-01) ──────────────────────────────────────────────────────
    // The 4-way slashing split is value-conserving: reporter + reserve + victim + burn equals the
    // slashed amount EXACTLY (no coins are created or destroyed by slashing), and only the reporter
    // is re-minted into the UTXO set. With a single (self-)validator there is no honest epoch peer,
    // so no victim-compensation output is emitted at (slash_tx, 2); the reserve share is pool-accrued
    // (not a UTXO) and the victim/burn shares leave the supply with the removed locked stake. Hence
    // minted (reporter) ≤ slashed ⇒ slashing cannot inflate supply.
    let rp = ctx.consensus.params().dns_params.clone().unwrap().reward_params;
    let dist = kaspa_consensus_core::dns_finality::compute_slashing_distribution(
        bond_amount,
        rp.slashing_reporter_reward_bps,
        rp.security_reserve_bps,
        rp.victim_epoch_pool_bps,
    );
    assert_eq!(
        dist.reporter_reward_sompi + dist.security_reserve_sompi + dist.victim_epoch_pool_sompi + dist.burned_sompi,
        bond_amount,
        "slashing split conserves value: reporter + reserve + victim + burn == slashed amount"
    );
    assert_eq!(dist.reporter_reward_sompi, expected_reporter, "minted reporter reward == the split's reporter share");
    assert!(dist.reporter_reward_sompi <= bond_amount, "the minted reporter reward never exceeds the slashed amount (no inflation)");
    assert!(
        !utxos.contains_key(&TransactionOutpoint::new(slash_tx_id, 2)),
        "no victim-compensation output is minted with a single (self-)validator"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — victim compensation (audit M-01). TWO validators: A
/// (equivocator) and B (honest). B attests the canonical anchor for a ready epoch E and is rewarded
/// (so it joins epoch E's accumulator `included` set); A then EQUIVOCATES the SAME epoch E (two
/// conflicting attestations) and is slashed. The slashing routes the victim-epoch share of A's
/// slashed stake to epoch E's honest peers = {B} (A is dropped by its `owner_reward_spk_payload`),
/// minting a victim-compensation output to B's reward P2PKH at `(slash_tx, 2)`. Proves the
/// multi-validator victim-compensation economics end-to-end through `mine_block`.
#[tokio::test]
async fn pos_v2_slashing_victim_compensates_honest_peer() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Ensure a non-zero victim pool (the 4-way split): reporter 10% / reserve 40% /
            // victim 40% / burn 10%.
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000;
            dns.reward_params.victim_epoch_pool_bps = 4000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs two coinbases (bond + slashing-evidence tx), B two (bond + attestation shard).
    // A block's coinbase pays its MERGESET (the previous block's miner), not its own, so mine a
    // batch per miner and SCAN all coinbases for ones paying each validator.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await); // mature the coinbases
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    // Bond A and B (active from activation_daa_score = 0).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_a_amount = va1 - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, vb1 - 100_000, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));

    // Bury so a ready, bond-active canonical anchor exists for a real epoch E.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };
    let epoch_e = anchor.epoch;

    // B HONESTLY attests the canonical anchor for epoch E → B is rewarded ⇒ joins epoch E's
    // accumulator `included` set (keyed by the attestation epoch).
    let att_b = dns_harness::build_signed_attestation(
        &vb, genesis_hash.as_byte_slice(), bond_b_outpoint, epoch_e, anchor.anchor_hash, anchor.anchor_daa_score, Hash64::default(),
    );
    let shard_b = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b2, vb2, db2, att_b, storage);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_b]).await;
    assert!(
        reward_block.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b),
        "B must be rewarded for attesting epoch E (so it joins the epoch's included set)"
    );

    // Bury so A's bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..5 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // A EQUIVOCATES the SAME epoch E: two conflicting attestations (different anchors).
    let target_daa = buried[1].header.daa_score;
    let att_a1 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, epoch_e, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, epoch_e, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a, // reporter paid to A's address (payout is independent of who is slashed)
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, evidence, storage);
    let slash_tx_id = slash_tx.id();
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the slashing block must validate AND mint the victim-compensation outputs (construction == validation)"
    );

    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!utxos.contains_key(&bond_a_outpoint), "A's slashed locked stake is removed");
    let dist = compute_slashing_distribution(
        bond_a_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    );
    let reporter = utxos.get(&TransactionOutpoint::new(slash_tx_id, 0)).expect("reporter reward minted at (slash_tx, 0)");
    assert_eq!(reporter.amount, dist.reporter_reward_sompi, "reporter = reporter_bps share");
    // VICTIM COMPENSATION: the single honest peer B receives the whole victim pool at (slash_tx, 2).
    let victim = utxos.get(&TransactionOutpoint::new(slash_tx_id, 2)).expect("victim-compensation output minted at (slash_tx, 2)");
    assert_eq!(victim.script_public_key, spk_b, "victim compensation pays the honest peer B");
    assert_eq!(victim.amount, dist.victim_epoch_pool_sompi, "the lone honest peer receives the entire victim pool");
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — multiple slashings in ONE block (audit M-01). TWO
/// independently-bonded validators A and B BOTH equivocate and are slashed by SEPARATE evidence
/// transactions carried in the SAME block. Proves the slashing pipeline applies N>1 side-effects
/// atomically and independently: both locked stakes are removed, each reporter reward is minted at
/// its own `(slash_tx, 0)`, each bond's 4-way split conserves value, and — the multi-slash-specific
/// invariant — the block's committed security-reserve accrual is the SUM of both bonds' reserve
/// shares (`apply_slashing_side_effects`'s fold over the resolved effects, persisted by the
/// `parent_balance + reserve_accrual − drip` recurrence). With no honest epoch peer, no
/// victim-compensation output is minted for either.
#[tokio::test]
async fn pos_v2_multi_slashing_in_one_block() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // 4-way split with non-zero reserve + victim shares (so the summed reserve accrual is observable).
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000;
            dns.reward_params.victim_epoch_pool_bps = 4000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: each validator needs two coinbases (bond + slashing-evidence tx). A block's coinbase pays
    // its MERGESET (the previous block's) miner, so mine a batch per miner and SCAN all coinbases.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await); // mature the coinbases
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    // Bond A and B (active from activation_daa_score = 0).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let (bond_a_amount, bond_b_amount) = (va1 - 100_000, vb1 - 100_000);
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, bond_b_amount, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));

    // Bury so BOTH bonds are committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Both A and B equivocate: each emits two conflicting attestations for the same epoch (different
    // anchors). With no honest peer the epoch only has to be shared by each validator's own pair.
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let epoch = 1u64;
    let target_daa = buried[1].header.daa_score; // past each bond's activation, within evidence_window of the slash block
    let att_a1 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, epoch, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, epoch, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    let att_b1 = dns_harness::build_signed_attestation(
        &vb, genesis_hash.as_byte_slice(), bond_b_outpoint, epoch, Hash64::from_bytes([0xc3u8; 64]), target_daa, Hash64::default(),
    );
    let att_b2 = dns_harness::build_signed_attestation(
        &vb, genesis_hash.as_byte_slice(), bond_b_outpoint, epoch, Hash64::from_bytes([0xd4u8; 64]), target_daa, Hash64::default(),
    );
    let ev_a = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a,
    };
    let ev_b = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_b_outpoint,
        attestation_a: att_b1,
        attestation_b: att_b2,
        reporter_reward_spk_payload: payload_b,
    };
    let slash_a = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, ev_a, storage);
    let slash_b = dns_harness::funded_signed_slashing_evidence_tx([0x43u8; 32], cb_b2, vb2, db2, ev_b, storage);
    let (slash_a_id, slash_b_id) = (slash_a.id(), slash_b.id());

    // BOTH slashing-evidence txs in ONE block.
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_a, slash_b]).await;
    assert_eq!(
        ctx.consensus.block_status(slash_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block carrying TWO slashing-evidence txs must validate (both side-effects apply atomically)"
    );
    // A block's own transactions (and thus their slashing side-effects + the reserve accrual they
    // commit) are applied to the persisted UTXO state only once the block becomes a SELECTED PARENT.
    // Mine one empty block on top so `slash_block`'s effects settle into a committed chain block
    // (`settle`), whose `reserve_balance_store` row we read below.
    let settle = ctx.mine_block(new_miner_data(), vec![]).await;

    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    // Both locked stakes removed.
    assert!(!utxos.contains_key(&bond_a_outpoint), "A's slashed locked stake is removed");
    assert!(!utxos.contains_key(&bond_b_outpoint), "B's slashed locked stake is removed");

    let rp = ctx.consensus.params().dns_params.clone().unwrap().reward_params;
    let dist_a = compute_slashing_distribution(bond_a_amount, rp.slashing_reporter_reward_bps, rp.security_reserve_bps, rp.victim_epoch_pool_bps);
    let dist_b = compute_slashing_distribution(bond_b_amount, rp.slashing_reporter_reward_bps, rp.security_reserve_bps, rp.victim_epoch_pool_bps);
    // Each reporter reward minted independently at its own (slash_tx, 0).
    let ra = utxos.get(&TransactionOutpoint::new(slash_a_id, 0)).expect("A's reporter reward minted at (slash_a, 0)");
    let rb = utxos.get(&TransactionOutpoint::new(slash_b_id, 0)).expect("B's reporter reward minted at (slash_b, 0)");
    assert_eq!((ra.amount, &ra.script_public_key), (dist_a.reporter_reward_sompi, &spk_a), "A's reporter share pays A");
    assert_eq!((rb.amount, &rb.script_public_key), (dist_b.reporter_reward_sompi, &spk_b), "B's reporter share pays B");
    // No honest epoch peer ⇒ no victim-compensation output for either bond.
    assert!(!utxos.contains_key(&TransactionOutpoint::new(slash_a_id, 2)), "no victim output without an honest peer (A)");
    assert!(!utxos.contains_key(&TransactionOutpoint::new(slash_b_id, 2)), "no victim output without an honest peer (B)");

    // Per-bond value conservation: each 4-way split sums back to the slashed amount.
    assert_eq!(
        dist_a.reporter_reward_sompi + dist_a.security_reserve_sompi + dist_a.victim_epoch_pool_sompi + dist_a.burned_sompi,
        bond_a_amount,
        "A's slash split conserves value"
    );
    assert_eq!(
        dist_b.reporter_reward_sompi + dist_b.security_reserve_sompi + dist_b.victim_epoch_pool_sompi + dist_b.burned_sompi,
        bond_b_amount,
        "B's slash split conserves value"
    );

    // MULTI-SLASH INVARIANT: the committed security-reserve accrual is the SUM of both bonds' reserve
    // shares (the fold in `apply_slashing_side_effects`). It commits under `settle` (the block whose
    // selected parent is `slash_block`, so its mergeset carries the two slash txs). `settle`'s parent
    // (`slash_block`) accrued no reserve (balance 0 ⇒ no drip), so the recurrence reduces to
    // `0 + (reserve_a + reserve_b) − 0`.
    let committed_reserve = ctx.consensus.virtual_processor().reserve_balance_store.get(settle.header.hash).unwrap_or(0);
    assert_eq!(
        committed_reserve,
        dist_a.security_reserve_sompi + dist_b.security_reserve_sompi,
        "the block's reserve accrual is the SUM of both slashed bonds' reserve shares"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4) — security-reserve DRIP (audit M-01). Closes the
/// reserve loop end-to-end: a slashing accrues its reserve share to the pool, and when an epoch the
/// pool can pay finalizes, the reserve DRIPS back out into that block's coinbase, stake-proportionally
/// to the epoch's honest included validators. TWO validators: A is slashed for equivocation (its
/// `security_reserve_bps` share accrues to the reserve pool); B honestly attests the canonical anchor
/// for a ready epoch E and joins `included[E]`. Once epoch E finalizes (its `(E+1)·L + finalization_depth`
/// DAA threshold is crossed), the finalizing block's coinbase pays B the whole reserve (cap set high,
/// B the sole included validator). Proves accrued-in == dripped-out (value conservation).
///
/// NOTE the config sets `epoch_length_blocks == attestation_epoch_length_blue_score`: the drip pays
/// the FINALIZING epoch's `included` set, which `recompute_epoch_tallies` keys by the ATTESTATION
/// epoch, while `epochs_finalized_at` selects epochs by the DAA epoch (`daa_score / epoch_length_blocks`).
/// The two numberings coincide (on a linear chain blue_score ≈ daa_score) only when those two lengths
/// are equal — which is exactly the production reality (both = 100 in GENESIS_ACTIVE/PRODUCTION_DNS_PARAMS).
#[tokio::test]
async fn pos_v2_reserve_drip_pays_finalized_epoch() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution, ready_epoch_from_tip_blue_score,
        },
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            // epoch_length_blocks == attestation_epoch_length_blue_score (production reality) so the
            // attestation epoch B signs and the DAA epoch the drip finalizes are the same number.
            dns.epoch_length_blocks = 3;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            // Reward recency must comfortably cover the canonical anchor (buried by lag + backoff
            // below the tip); finalization_depth = window + max_reorg_horizon = 52.
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.stake_score_window_blue_score = 10_000;
            // Isolate the drip: participation takes the full validator pool (quality-bonus pool = 0), so
            // the only post-attestation coinbase output to B is the reserve drip.
            dns.reward_params.validator_participation_bps = 10_000;
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000; // 40% of the slashed bond accrues to the reserve
            dns.reward_params.victim_epoch_pool_bps = 4000;
            dns.reward_params.reserve_drip_per_epoch_cap_sompi = u64::MAX; // the whole reserve drips in one epoch
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs two coinbases (bond + slashing-evidence tx), B two (bond + attestation shard).
    // A block's coinbase pays its MERGESET miner, so mine a batch per miner and SCAN all coinbases.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    let storage = ctx.consensus.params().storage_mass_parameter;
    let genesis_hash = ctx.consensus.params().genesis.hash;

    // ── B bonds and HONESTLY attests the ready canonical epoch E ────────────────────────────────
    // B bonds and attests FIRST: A is bonded only later (below), strictly after E's anchor, so A is
    // not part of E's expected-stake denominator — leaving B the sole included validator at E, which
    // makes the drip pay B the WHOLE reserve (a crisp value-conservation assertion). The stake-
    // proportional split when a slashed peer co-existed at the anchor is exercised separately.
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, vb1 - 100_000, 0, storage);
    let bond_b_id = bond_b_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let bond_b_outpoint = TransactionOutpoint::new(bond_b_id, 0);
    // Bury so a ready, bond-active canonical anchor exists for B's epoch E.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };
    let epoch_e = anchor.epoch;
    let att_b = dns_harness::build_signed_attestation(
        &vb, genesis_hash.as_byte_slice(), bond_b_outpoint, epoch_e, anchor.anchor_hash, anchor.anchor_daa_score, Hash64::default(),
    );
    let shard_b = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b2, vb2, db2, att_b, storage);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_b]).await;
    assert!(
        reward_block.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b),
        "B must be rewarded for attesting epoch E (so it joins included[E])"
    );

    // ── Accrue the reserve: bond A (AFTER E's anchor) and slash it for equivocation ─────────────
    let bond_a_amount = va1 - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let bond_a_id = bond_a_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    let bond_a_outpoint = TransactionOutpoint::new(bond_a_id, 0);
    // Bury so A's bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    // A equivocates an arbitrary epoch (1) DISJOINT from B's epoch E, so A's slash mints no victim
    // output (epoch 1 has no honest included peer) — only the reserve accrues.
    let target_daa = buried[1].header.daa_score;
    let att_a1 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, 1, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, 1, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, evidence, storage);
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(ctx.consensus.block_status(slash_block.header.hash), BlockStatus::StatusUTXOValid, "the slashing block must validate");
    // Settle so the reserve accrual commits (a block's own txs apply once it becomes a selected parent).
    let reserve_settle = ctx.mine_block(new_miner_data(), vec![]).await;
    let dist = compute_slashing_distribution(
        bond_a_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    );
    let reserve_accrued = ctx.consensus.virtual_processor().reserve_balance_store.get(reserve_settle.header.hash).unwrap_or(0);
    assert_eq!(reserve_accrued, dist.security_reserve_sompi, "A's slash accrues its reserve share to the pool");
    assert!(reserve_accrued > 0, "the reserve must be non-zero to drip");

    // ── Mine until epoch E's DAA-finalization; the drip pays B in that block's coinbase ─────────
    let target_final_daa = (epoch_e + 1) * dns.epoch_length_blocks + dns.reward_uniqueness_window_blocks + dns.max_reorg_horizon_blocks;
    let mut drip_block = None;
    for _ in 0..80 {
        let blk = ctx.mine_block(new_miner_data(), vec![]).await;
        // The reserve drip is appended to the coinbase of the block that finalizes epoch E. B got its
        // one-time participation reward at `reward_block` (cross-block dedup blocks re-payment), and the
        // §D worker bounty pays the includer — so the only later coinbase output to B is the drip.
        if blk.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b) {
            drip_block = Some(blk);
            break;
        }
        if blk.header.daa_score > target_final_daa + 5 {
            break;
        }
    }
    let drip_block = drip_block.expect("a block after the reward must drip the reserve to B at epoch E's finalization");
    let drip_out = drip_block.transactions[0].outputs.iter().find(|o| o.script_public_key == spk_b).expect("drip pays B");
    // The sole included validator B receives the WHOLE reserve (cap is u64::MAX): accrued-in == dripped-out.
    assert_eq!(drip_out.value, reserve_accrued, "the entire accrued reserve drips to the lone included validator B");
}

/// kaspa-pq ADR-0016 §D.2 — the bond-UTXO spend-gate races the slashing side-effect (audit M-01). A
/// validator's locked stake (the bond's output-0) is NOT releasable while the bond is Active, so a
/// block that SPENDS it must be rejected — even when the SAME block also carries a slashing-evidence
/// tx for that bond (which would otherwise remove output-0). The spend-gate wins the race: the block
/// is disqualified (`NonReleasableBondSpendInBlock`), so NEITHER the spend NOR the slash takes effect
/// — the locked stake survives intact and no reporter reward is minted. Proves a validator cannot
/// reclaim locked capital by smuggling a self-spend into a block, and that the spend-gate takes
/// precedence over the slashing side-effect.
#[tokio::test]
async fn pos_v2_spend_gate_rejects_locked_bond_racing_slash() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // One validator/funding/reporter key.
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund: h_a pays K (funds the bond), h_b pays K (funds the slashing-evidence tx).
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // Bond: output-0 is the locked stake (a P2PKH to K), Active from activation 0.
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _payload) = dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block must be UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let bond_daa = bond_block.header.daa_score;

    // Bury so the bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }

    // Equivocation evidence (two conflicting attestations for the same (bond, epoch)).
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v, genesis_hash.as_byte_slice(), bond_outpoint, 1, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v, genesis_hash.as_byte_slice(), bond_outpoint, 1, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage);
    let slash_tx_id = slash_tx.id();
    // A self-spend of the still-locked bond output-0 (the spend-gate violation).
    let spend_tx = dns_harness::funded_signed_p2pkh_spend(seed, bond_outpoint, bond_amount, bond_daa, storage);

    // ONE block carries BOTH: the slash (which would remove output-0) AND the self-spend of output-0.
    // The Active bond is not releasable ⇒ the spend-gate disqualifies the whole block.
    let race_block = ctx.mine_block(new_miner_data(), vec![slash_tx, spend_tx]).await;
    assert_ne!(
        ctx.consensus.block_status(race_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "spending the locked bond output-0 must disqualify the block (the spend-gate wins over the slash)"
    );

    // The block had NO effect: the locked stake survives (neither spent nor slashed-away) and no
    // reporter reward was minted.
    let utxos: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(utxos.contains_key(&bond_outpoint), "the locked stake survives — the spend-gate rejected the racing block");
    assert!(
        !utxos.contains_key(&TransactionOutpoint::new(slash_tx_id, 0)),
        "no reporter reward — the slash never applied (block disqualified)"
    );
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2) — slashing is REORG-RESISTANT and reorg-SAFE (audit M-01). An
/// equivocator cannot escape its slash by getting the network to reorg onto a heavier branch that
/// omits the evidence. A bond X is buried in a shared prefix; one branch (A) slashes X — committing
/// the side-effect (output-0 removed, reporter minted at `(slash_tx, 0)`) — while a HEAVIER competing
/// branch (B), built by a second consensus instance over the SAME prefix, omits the slash. When B's
/// blocks arrive the node reorgs onto B (the reorg gate is held dormant — Bootstrap stage, since
/// `min_active_validators` is raised so a lone bond never activates it — so selection is pure
/// blue_work). The slash block leaves the SELECTED chain, but branch A is still a DAG tip and is
/// MERGED into the virtual, so the slash side-effect is RECOMPUTED and re-applies deterministically:
/// X stays slashed and the reporter stays minted, exactly once (no double-removal, no panic, supply
/// conserved). This is the economically correct, reorg-safe outcome — the equivocation evidence is
/// permanent in the DAG, so the punishment survives the reorg rather than being stranded or replayed.
#[tokio::test]
async fn pos_v2_slashing_survives_reorg_via_evidence_merge() {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            // A large reorg horizon so the fork is within range (the gate is dormant anyway).
            dns.max_reorg_horizon_blocks = 1000;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Keep the rollout stage in Bootstrap (one bond can never reach Active), so the reorg gate
            // stays GateInactive and selection is pure blue_work — the heaviest branch wins.
            dns.min_active_validators = 100;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // ── Shared prefix on the honest node: fund + bond X + bury (collected for delivery to the 2nd
    //    instance, so both branches share an identical bond-creation history) ────────────────────
    let mut prefix = Vec::new();
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    prefix.push(ctx.mine_block(k_miner.clone(), vec![]).await);
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    prefix.push(h_a.clone());
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    prefix.push(h_b.clone());
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        prefix.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _rp) = dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage);
    let bond_tx_id = bond_tx.id();
    prefix.push(ctx.mine_block(new_miner_data(), vec![bond_tx]).await);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    let mut buried = Vec::new();
    for _ in 0..6 {
        let b = ctx.mine_block(new_miner_data(), vec![]).await;
        buried.push(b.clone());
        prefix.push(b);
    }

    // ── Second instance: replay the SAME prefix, then build a HEAVIER no-slash branch B ─────────
    let mut atk = TestContext::new(TestConsensus::new(&config));
    for b in &prefix {
        atk.validate_and_insert_block(b.clone()).await;
    }
    atk.simulated_time = ctx.simulated_time; // so branch B's timestamps stay ahead of the prefix
    let mut branch_b = Vec::new();
    for _ in 0..12 {
        branch_b.push(atk.mine_block(new_miner_data(), vec![]).await);
    }
    let branch_b_tip = branch_b.last().unwrap().header.hash;

    // ── Honest branch A: slash X (equivocation) and settle so the side-effect is COMMITTED ──────
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let target_daa = buried[1].header.daa_score;
    let att_a = dns_harness::build_signed_attestation(
        &v, genesis_hash.as_byte_slice(), bond_outpoint, 1, Hash64::from_bytes([0xa1u8; 64]), target_daa, Hash64::default(),
    );
    let att_b = dns_harness::build_signed_attestation(
        &v, genesis_hash.as_byte_slice(), bond_outpoint, 1, Hash64::from_bytes([0xb2u8; 64]), target_daa, Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint,
        attestation_a: att_a,
        attestation_b: att_b,
        reporter_reward_spk_payload: k_payload,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx(seed, coinbase_b, value_b, daa_b, evidence, storage);
    let slash_tx_id = slash_tx.id();
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(ctx.consensus.block_status(slash_block.header.hash), BlockStatus::StatusUTXOValid, "branch A's slash block validates");
    ctx.mine_block(new_miner_data(), vec![]).await; // settle so the slash side-effect commits

    // Slash applied on branch A: X's locked stake is gone and the reporter reward is minted.
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let expected_reporter = kaspa_consensus_core::dns_finality::compute_slashing_distribution(
        bond_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    )
    .reporter_reward_sompi;
    let reporter_outpoint = TransactionOutpoint::new(slash_tx_id, 0);
    let pre: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(!pre.contains_key(&bond_outpoint), "branch A: the slashed bond's locked stake is removed");
    assert_eq!(pre.get(&reporter_outpoint).map(|u| u.amount), Some(expected_reporter), "branch A: the reporter reward is minted");

    // ── Deliver branch B → the node reorgs onto the heavier no-slash branch ─────────────────────
    for b in &branch_b {
        ctx.validate_and_insert_block(b.clone()).await;
    }
    assert_eq!(ctx.consensus.get_sink(), branch_b_tip, "the node reorged onto the heavier branch B (gate dormant ⇒ pure blue_work)");

    // ── The slash SURVIVES the reorg: branch A leaves the selected chain but is merged back into the
    //    virtual, so the side-effect re-applies deterministically — X stays slashed, reporter stays
    //    minted EXACTLY ONCE (no double-removal, no double-mint, no panic). ──────────────────────
    let post: std::collections::HashMap<_, _> = ctx.consensus.get_virtual_utxos(None, 100_000, false).into_iter().collect();
    assert!(
        !post.contains_key(&bond_outpoint),
        "after reorg: the equivocator is STILL slashed — its locked stake stays removed (evidence merged back)"
    );
    assert_eq!(
        post.get(&reporter_outpoint).map(|u| u.amount),
        Some(expected_reporter),
        "after reorg: the reporter reward is still minted, exactly once (recomputed over the new selected chain + merge set)"
    );
}

/// kaspa-pq ADR-0018 §F (DAG-3) — STAGED reward-split rollout across the `full_reward_split_daa_score`
/// boundary. The §F carve selects the fee/subsidy split deterministically from the block's DAA score:
/// below `full_reward_split_daa_score` the BOOTSTRAP split (smaller validator carve — worker base
/// 8200bps), at/above it the FULL split (worker base 6200bps; validator 30% — re-genesis raised it
/// from 25%). This mines a constant-miner chain straight across the boundary and asserts (a) EVERY
/// block stays UTXO-valid — the coinbase carve the template builds equals the one validation
/// recomputes, on BOTH sides AND at the crossing block (construction == validation across a staged
/// consensus parameter), and (b) the miner's per-block subsidy share visibly DROPS at the boundary
/// (bootstrap 82% → full 62% of subsidy), proving the split actually changed rather than the stage
/// being inert.
#[tokio::test]
async fn pos_v2_staged_full_reward_split_across_boundary() {
    kaspa_core::log::try_init_logger("info");
    const H: u64 = 20; // full_reward_split_daa_score
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0; // overlay active ⇒ the §F carve applies (Some(split))
            dns.full_reward_split_daa_score = H; // Stage 2 (bootstrap) below H, Stage 3 (full) at/above
            // pos_v2 stays fenced (preset u64::MAX) — §F fee-split staging is independent of the v2 economics.
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // A single, constant miner so every coinbase pays the same spk (the reward is the prev block's
    // carved subsidy — the coinbase pays its mergeset miner).
    let v = dns_harness::harness_validator([0x42u8; 32]);
    let k_spk = p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes());
    let miner = MinerData::new(k_spk.clone(), vec![]);

    let mut rewards: Vec<(u64, u64)> = Vec::new(); // (block daa_score, miner's coinbase reward)
    for _ in 0..(H + 15) {
        let b = ctx.mine_block(miner.clone(), vec![]).await;
        assert_eq!(
            ctx.consensus.block_status(b.header.hash),
            BlockStatus::StatusUTXOValid,
            "every block stays UTXO-valid across the staged-split boundary (construction == validation)"
        );
        let r: u64 = b.transactions[0].outputs.iter().filter(|o| o.script_public_key == k_spk).map(|o| o.value).sum();
        rewards.push((b.header.daa_score, r));
    }

    // The coinbase of a block at DAA d carves the mergeset (prev block's) subsidy by the split
    // SELECTED FROM d. Sample a block clearly in Stage 2 (bootstrap) and one clearly in Stage 3
    // (full); both adjacent enough that subsidy decay is negligible, so the ratio isolates the carve.
    let stage2 = rewards.iter().rev().find(|(d, r)| *d < H && *r > 0).map(|(_, r)| *r).expect("a Stage-2 reward");
    let stage3 = rewards.iter().find(|(d, r)| *d >= H && *r > 0).map(|(_, r)| *r).expect("a Stage-3 reward");
    // Worker base share drops 8200bps → 6200bps ⇒ ratio ≈ 0.7561. Tolerance absorbs the tiny per-block decay.
    let ratio = stage3 as f64 / stage2 as f64;
    assert!(
        (0.74..=0.77).contains(&ratio),
        "the miner's subsidy share drops at the boundary by the bootstrap→full worker-base carve (8200→6200bps ≈ 0.756); got stage2={stage2} stage3={stage3} ratio={ratio:.4}"
    );
}

/// kaspa-pq ADR-0018 §G (DAG-7) — MULTI-NODE mesh convergence with the DNS overlay ACTIVE. Three
/// independent consensus instances (same overlay-active config) each mine a DIVERGENT chain from
/// genesis; then every block is gossiped to every node. All three must converge on the SAME sink —
/// i.e. the overlay's per-block machinery (epoch accumulator / reserve / rewarded-keys stores) and
/// the reorg gate (dormant here: no attestations ⇒ no confirmed anchor) do NOT break GHOSTDAG's
/// deterministic multi-node convergence. Complements the single-instance wide-DAG anchor-agreement
/// test (which proves divergent VIEWS pick one anchor) with real cross-instance block exchange.
#[tokio::test]
async fn dag7_multi_node_mesh_converges_with_overlay_active() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 8; // enough to merge the divergent tips
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0; // overlay ACTIVE on every node
            p.dns_params = Some(dns);
        })
        .build();

    // Three nodes, each mining a chain of a DIFFERENT length from genesis (genuinely divergent tips).
    let mut nodes: Vec<TestContext> = (0..3).map(|_| TestContext::new(TestConsensus::new(&config))).collect();
    let lengths = [5usize, 8, 6];
    let mut chains: Vec<Vec<Block>> = Vec::new();
    for i in 0..nodes.len() {
        let mut blocks = Vec::new();
        for _ in 0..lengths[i] {
            blocks.push(nodes[i].mine_block(new_miner_data(), vec![]).await);
        }
        chains.push(blocks);
    }

    // Before gossip the nodes disagree (each sees only its own chain's tip).
    let pre: Vec<_> = nodes.iter().map(|n| n.consensus.get_sink()).collect();
    assert!(pre[0] != pre[1] || pre[1] != pre[2], "pre-gossip the nodes' sinks diverge");

    // Gossip: deliver every OTHER node's chain (parents-first) to each node.
    for i in 0..nodes.len() {
        for j in 0..chains.len() {
            if i == j {
                continue;
            }
            for b in &chains[j] {
                nodes[i].validate_and_insert_block(b.clone()).await;
            }
        }
    }

    // After gossip every node holds the identical union DAG ⇒ all converge on ONE sink.
    let sinks: Vec<_> = nodes.iter().map(|n| n.consensus.get_sink()).collect();
    assert_eq!(sinks[0], sinks[1], "node 0 and node 1 converge on the same sink ({} vs {})", sinks[0], sinks[1]);
    assert_eq!(sinks[1], sinks[2], "node 1 and node 2 converge on the same sink ({} vs {})", sinks[1], sinks[2]);
    // The converged sink is the heaviest divergent chain's tip (node 1's 8-block chain), and every
    // node's chosen sink is one of the gossiped tips (a real block, not genesis).
    let tips: std::collections::HashSet<_> = chains.iter().map(|c| c.last().unwrap().header.hash).collect();
    assert!(tips.contains(&sinks[0]), "the converged sink is one of the mined chain tips");
}

/// kaspa-pq ADR-0018 §D/§E (DAG-5) — SELECTIVE attestation CENSORSHIP does not pay: a miner that
/// excludes a validator's attestation cannot redistribute that validator's reward share to the
/// validators it DID include, because the §E participation pool is divided by the EXPECTED (total
/// active) stake — a fixed denominator set by the active BOND set, not by who was included
/// (test-plan DNS-D3: "included-fraction drops, φS gates, NOT redistributed; denominator fixed").
///
/// Two equal-stake validators A and B are both bonded for the whole test. In epoch E1 both attest
/// and both are included → each earns `pool/2` (equal stake ⇒ equal reward). In epoch E2 the miner
/// CENSORS B (includes only A's attestation) — yet A still earns `pool/2`, NOT the whole pool: B's
/// bond is still active so the denominator stays `A+B`, and B's uncredited share is simply not
/// minted (don't-mint rollover). The decisive check is `A_reward(E2) ≈ A_reward(E1)` (NOT ≈ 2×):
/// censoring B left A's reward unchanged, so censorship yields the miner/included-validator nothing.
#[tokio::test]
async fn dag5_selective_censorship_does_not_redistribute_reward() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs three coinbases (bond + E1 shard + E2 shard), B two (bond + E1 shard).
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 3 && b_funds.len() >= 2, "need ≥3 A / ≥2 B funding coinbases (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a_bond, va1, da_bond), (cb_a_e1, va_e1, da_a_e1), (cb_a_e2, va_e2, da_a_e2)) = (a_funds[0], a_funds[1], a_funds[2]);
    let ((cb_b_bond, vb1, db_bond), (cb_b_e1, vb_e1, db_b_e1)) = (b_funds[0], b_funds[1]);

    // Bond A and B with EXACTLY equal stake (so equal-stake ⇒ equal reward is exact).
    let storage = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = va1.min(vb1) - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a_bond, va1, da_bond, bond_amount, 0, storage);
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b_bond, vb1, db_bond, bond_amount, 0, storage);
    let (bond_a_id, bond_b_id) = (bond_a_tx.id(), bond_b_tx.id());
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let (bond_a_outpoint, bond_b_outpoint) = (TransactionOutpoint::new(bond_a_id, 0), TransactionOutpoint::new(bond_b_id, 0));
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let anchor_at = |ctx: &TestContext, epoch: u64| {
        let vp = ctx.consensus.virtual_processor();
        vp.canonical_anchor_by_blue_score(epoch, ctx.consensus.get_sink(), &dns).expect("canonical anchor")
    };
    let ready_epoch = |ctx: &TestContext| -> u64 {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(ctx.consensus.get_sink()).unwrap();
        ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready")
    };

    // ── Epoch E1: BOTH A and B attest and BOTH are included ⇒ each earns pool/2 (equal stake) ──
    let e1 = ready_epoch(&ctx);
    let anchor_e1 = anchor_at(&ctx, e1);
    let att_a1 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, e1, anchor_e1.anchor_hash, anchor_e1.anchor_daa_score, Hash64::default(),
    );
    let att_b1 = dns_harness::build_signed_attestation(
        &vb, genesis_hash.as_byte_slice(), bond_b_outpoint, e1, anchor_e1.anchor_hash, anchor_e1.anchor_daa_score, Hash64::default(),
    );
    let shard_a1 = dns_harness::funded_signed_shard_tx([0x42u8; 32], cb_a_e1, va_e1, da_a_e1, att_a1, storage);
    let shard_b1 = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b_e1, vb_e1, db_b_e1, att_b1, storage);
    let block_full = ctx.mine_block(new_miner_data(), vec![shard_a1, shard_b1]).await;
    let reward = |blk: &Block, spk: &kaspa_consensus_core::tx::ScriptPublicKey| -> u64 {
        blk.transactions[0].outputs.iter().filter(|o| o.script_public_key == *spk).map(|o| o.value).sum()
    };
    let (a_reward_e1, b_reward_e1) = (reward(&block_full, &spk_a), reward(&block_full, &spk_b));

    // ── Mine forward to the NEXT ready epoch E2 ──
    let mut e2 = ready_epoch(&ctx);
    let mut guard = 0;
    while e2 <= e1 && guard < 20 {
        ctx.mine_block(new_miner_data(), vec![]).await;
        e2 = ready_epoch(&ctx);
        guard += 1;
    }
    assert!(e2 > e1, "a later epoch E2 must become ready");

    // ── Epoch E2: the miner CENSORS B (includes only A's attestation). B's bond is still active, so
    //    the §E denominator stays A+B and A earns the SAME pool/2 — B's share is NOT redistributed. ──
    let anchor_e2 = anchor_at(&ctx, e2);
    let att_a2 = dns_harness::build_signed_attestation(
        &va, genesis_hash.as_byte_slice(), bond_a_outpoint, e2, anchor_e2.anchor_hash, anchor_e2.anchor_daa_score, Hash64::default(),
    );
    let shard_a2 = dns_harness::funded_signed_shard_tx([0x42u8; 32], cb_a_e2, va_e2, da_a_e2, att_a2, storage);
    let block_censor = ctx.mine_block(new_miner_data(), vec![shard_a2]).await; // B's E2 attestation is omitted
    let (a_reward_e2, b_reward_censor) = (reward(&block_censor, &spk_a), reward(&block_censor, &spk_b));

    // E1, both included, equal stake ⇒ equal, non-zero rewards.
    assert!(a_reward_e1 > 0 && b_reward_e1 > 0, "E1: both included validators are rewarded");
    assert_eq!(a_reward_e1, b_reward_e1, "E1: equal stake ⇒ equal §E participation reward");

    // E2, B censored: A is rewarded, B earns NOTHING (its attestation was excluded).
    assert!(a_reward_e2 > 0, "E2: the included validator A is still rewarded");
    assert_eq!(b_reward_censor, 0, "E2: the CENSORED validator B earns nothing");

    // DECISIVE anti-redistribution check: censoring B did NOT hand B's share to A. A's E2 reward is
    // the SAME pool/2 it earned in E1 (the denominator stayed A+B) — NOT ≈ 2× (the whole pool), which
    // is what a redistribute-to-included bug would produce.
    assert!(
        a_reward_e2 <= a_reward_e1 && a_reward_e2 * 100 >= a_reward_e1 * 90,
        "censorship is not rewarded: A_reward(E2)={a_reward_e2} must stay ≈ A_reward(E1)={a_reward_e1} (same pool/2 share), not be inflated toward the full pool"
    );
}

/// kaspa-pq H-06 (unbond lifecycle): full-consensus unbond-REQUEST e2e + the client-side
/// funded builder (`funded_signed_unbond_tx`). A funded, ML-DSA-87-signed bond goes
/// Active; the owner then submits a funded, signed `StakeUnbondRequest` — the shape an
/// operator's exit tool produces. The including block must validate, exercising the live
/// unbond-authorization rule (`unbond_request_authorized`: bond present, Pending/Active,
/// owner-key binding `validator_id_from_pubkey(owner) == bond.owner_pubkey_hash`, and the
/// ML-DSA-87 signature over the bond-bound `unbond_request_message` under
/// `UNBOND_REQUEST_CONTEXT`). The release-after-`unbonding_period_blocks` spend is covered
/// by the apply-path unit tests (`allows_spend_of_releasable_bond`).
#[tokio::test]
async fn pos_v2_funded_unbond_request_validates() {
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let bond_amount = value_a - 100_000;
    let (bond_tx, _vid, _rp) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, bond_amount, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(bond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the bond block must be UTXO-valid"
    );
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    // Bury the bond so its record is committed into the active bond view the unbond rule reads.
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // The owner submits a funded, ML-DSA-87-signed unbond request; the block must validate.
    // audit M-04: the authorization binds the network id (genesis hash), as the consensus rule reconstructs it.
    let net_id = ctx.consensus.params().genesis.hash;
    let unbond_tx =
        dns_harness::funded_signed_unbond_tx(seed, net_id.as_byte_slice(), coinbase_b, value_b, daa_b, bond_outpoint, storage_mass_parameter);
    let unbond_block = ctx.mine_block(new_miner_data(), vec![unbond_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(unbond_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the owner-authorized funded unbond request must validate through full consensus"
    );
}

/// kaspa-pq DNS v3 (PR2b): the processor's blue_score canonical-anchor walk
/// (`canonical_anchor_by_blue_score`) feeds the pure core the *real* selected-chain
/// `(hash, blue_score, daa_score)` ancestors, so the anchor it returns is a genuine
/// selected-chain block, most-recent-at-or-below the epoch cutoff, and stable as the tip
/// advances (the v3 position-invariance property). The hot path does not call it yet (PR4
/// wires it into the verifier), so this white-box test is the only thing exercising the
/// store walk until then. A future / unburied epoch must return `None`, never the tip.
#[tokio::test]
async fn dns_v3_canonical_anchor_walk_matches_chain() {
    use std::collections::HashMap;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Tiny blue_score epochs so several bury within a short linear chain.
            // L=3, backoff=1 -> cutoff(E) = (E+1)*3 - 1 - 1 = 3E+1; lag=2.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // A linear chain (one block per row); each block's only parent is the prior tip, so
    // mergeset_blues = {selected_parent} and blue_score increments by exactly 1 (genesis = 0).
    let miner = new_miner_data();
    let mut by_blue: HashMap<u64, BlockHash> = HashMap::new();
    for _ in 0..20 {
        let b = ctx.mine_block(miner.clone(), vec![]).await;
        by_blue.insert(b.header.blue_score, b.header.hash);
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let tip = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor();

    // cutoff(E) = 3E+1 on this dense chain, and every integer blue_score 0..=20 is present
    // exactly once, so the most-recent-at-or-below is the block whose blue_score == cutoff(E).
    let a0 = vp.canonical_anchor_by_blue_score(0, tip, &dns).expect("epoch 0 buried");
    assert_eq!(a0.epoch, 0);
    assert_eq!(a0.cutoff_blue_score, 1);
    assert_eq!(a0.anchor_blue_score, 1);
    assert_eq!(a0.anchor_hash, by_blue[&1], "epoch 0 anchors the real bs=1 block");
    assert!(!a0.duplicate_of_previous_anchor);

    let a1 = vp.canonical_anchor_by_blue_score(1, tip, &dns).expect("epoch 1 buried");
    assert_eq!(a1.cutoff_blue_score, 4);
    assert_eq!(a1.anchor_blue_score, 4);
    assert_eq!(a1.anchor_hash, by_blue[&4], "epoch 1 anchors the real bs=4 block");
    assert!(!a1.duplicate_of_previous_anchor); // distinct anchors on a dense chain

    // Position-invariance: anchor(0) is the SAME block no matter how far the tip advanced
    // (the walk reads blue_score, not the store index) — the core v3 property.
    let mid = by_blue[&10];
    let a0_mid = vp.canonical_anchor_by_blue_score(0, mid, &dns).expect("epoch 0 buried at mid-chain tip");
    assert_eq!(a0_mid.anchor_hash, a0.anchor_hash, "the anchor is independent of the observing tip");

    // A future / unburied epoch has no canonical anchor on this chain (cutoff > tip.blue_score)
    // and must NOT degenerate to returning the tip.
    assert!(vp.canonical_anchor_by_blue_score(1_000_000, tip, &dns).is_none());
}

/// kaspa-pq DNS v3 (PR4) — POSITIVE: an attestation that names THIS chain's canonical
/// lagged anchor for a ready blue_score epoch IS credited by the v3 verifier
/// (`collect_stake_contributions_v2`) with the bond's full stake, the per-epoch
/// denominator is keyed by the CANONICAL anchor DAA, and a ready epoch the validator did
/// NOT attest still appears in the denominator (so a participation gap is visible to φS
/// instead of vanishing — the v1 weakness). Reuses the funded-bond + funded-shard DAG-2
/// harness; the attestation is signed over the canonical `(epoch, anchor_hash,
/// anchor_daa_score)` rather than a free-floating self-reported target.
#[tokio::test]
async fn dns_v3_canonical_attestation_credited() {
    use crate::model::stores::{headers::HeaderStoreReader, stake_bonds::StakeBondsStoreReader};
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 2;
            dns.max_reorg_horizon_blocks = 2;
            // Small blue_score epochs so several bury within this chain: L=3, backoff=1 ->
            // cutoff(E)=3E+1; lag=2 -> epoch E ready once tip_blue >= 3E+4.
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund a bond (coinbase_a) + a shard-funding coinbase (coinbase_b), same as the e2e.
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid, "the bond block is UTXO-valid");
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);

    // Bury several blue_score epochs past the bond so a ready, bond-active anchor exists.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    // THIS chain's canonical anchor for the latest ready epoch at the current sink.
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let latest_ready =
            ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(latest_ready, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // Sign an attestation that names the canonical anchor exactly, fund + include it.
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the canonical-attestation block validates"
    );
    // Mine 2 fillers so the shard is MERGED -> accepted by a chain block in past(sink), the
    // view the StakeScore verifier walks (accepted txs, not a block's own body).
    ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;

    let new_sink = ctx.consensus.get_sink();
    let (contributions, denom, bond_amount) = {
        let vp = ctx.consensus.virtual_processor();
        let bonds: Vec<_> =
            vp.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let bond_amount = bonds.iter().find(|b| b.bond_outpoint == bond_outpoint).expect("the funded bond is persisted").amount;
        let (c, d) = vp.collect_stake_contributions_v2(new_sink, None, &bonds, genesis_hash.as_byte_slice(), &dns);
        (c, d, bond_amount)
    };

    // The canonical attestation is credited with the bond's full stake at its epoch.
    let credited =
        contributions.iter().find(|c| c.bond_outpoint == bond_outpoint).expect("the canonical attestation is credited");
    assert_eq!(credited.epoch, anchor.epoch, "credited at the canonical epoch");
    assert_eq!(credited.signed_stake_sompi, bond_amount, "credited with the bond's full stake");
    // The denominator is keyed by the CANONICAL anchor DAA for that epoch.
    assert_eq!(denom.get(&anchor.epoch).copied(), Some(anchor.anchor_daa_score), "denominator keyed by the canonical anchor DAA");
    // A ready epoch with no attestation still appears in the denominator (visible gap).
    assert!(
        denom.keys().any(|&e| !contributions.iter().any(|c| c.epoch == e)),
        "a ready, un-attested epoch is still in the denominator (got epochs {:?})",
        denom.keys().collect::<Vec<_>>()
    );
}

/// kaspa-pq DNS v3 (PR4) — NEGATIVE: a validly-signed, bonded, reward-eligible attestation
/// for a ready epoch whose `target_hash` is NOT this chain's canonical anchor is NOT
/// credited by the v3 verifier. The including block still validates (the reward path is
/// migrated to the canonical rule in PR5; until then a non-canonical attestation can still
/// earn the v1 reward), which is exactly the divergence PR5 closes — here we prove the
/// StakeScore verifier already refuses the non-canonical target.
#[tokio::test]
async fn dns_v3_noncanonical_attestation_rejected() {
    use crate::model::stores::{headers::HeaderStoreReader, stake_bonds::StakeBondsStoreReader};
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            // Wide reward recency so the canonical anchor is comfortably in-window: the only
            // reason the bogus-target attestation earns nothing is the v3 canonical gate.
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let latest_ready =
            ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(latest_ready, sink, &dns).expect("canonical anchor for the ready epoch")
    };

    // Same ready epoch + canonical DAA, but a BOGUS target_hash (not this chain's anchor).
    let bogus_target = Hash64::from_bytes([0xdeu8; 64]);
    assert_ne!(bogus_target, anchor.anchor_hash);
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        bogus_target,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(
        ctx.consensus.block_status(reward_block.header.hash),
        BlockStatus::StatusUTXOValid,
        "the block still validates — the canonical-gated reward fan-out simply pays nothing for the non-canonical attestation (same in construction + validation)"
    );
    // PR5: the §E reward fan-out is canonical-gated, so the non-canonical attestation earns
    // NO coinbase reward (only output to K would be the §E reward; the miner is a different spk).
    let reward_to_validator = reward_block.transactions[0].outputs.iter().find(|o| o.script_public_key == k_spk).map(|o| o.value);
    assert_eq!(reward_to_validator, None, "a non-canonical attestation earns no §E reward (PR5)");

    ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;

    let new_sink = ctx.consensus.get_sink();
    let (contributions, denom) = {
        let vp = ctx.consensus.virtual_processor();
        let bonds: Vec<_> =
            vp.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        vp.collect_stake_contributions_v2(new_sink, None, &bonds, genesis_hash.as_byte_slice(), &dns)
    };

    // The non-canonical attestation also earns NO StakeScore credit (PR4)...
    assert!(
        contributions.iter().all(|c| c.bond_outpoint != bond_outpoint),
        "a non-canonical-target attestation must not be credited"
    );
    // ...even though its epoch IS a ready, creditable epoch (present in the denominator).
    assert!(denom.contains_key(&anchor.epoch), "the epoch is ready/creditable; only the non-canonical target is rejected");
}

/// kaspa-pq DNS v3 (PR3) — the signer hands the validator the canonical lagged anchor, NOT
/// the live sink. The singular `get_validator_attestation_target` returns the latest READY
/// epoch's canonical anchor (epoch/target_hash/target_daa_score all match
/// `canonical_anchor_by_blue_score`, VSC = zero per P-1D), and the batch
/// `get_validator_attestation_targets` returns every ready, non-duplicate epoch ascending up
/// to the latest — so a fallen-behind validator can catch up. Both feed the exact target the
/// PR4 verifier credits.
#[tokio::test]
async fn dns_v3_signer_produces_canonical_ready_targets() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{Hash64, dns_finality::ready_epoch_from_tip_blue_score};
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let miner = new_miner_data();
    let first = ctx.mine_block(miner.clone(), vec![]).await;
    // Any outpoint works — the signer assembles the message for whatever bond it is asked
    // about; eligibility is the validator service's concern, not the signer's.
    let outpoint = TransactionOutpoint::new(first.transactions[0].id(), 0);
    for _ in 0..20 {
        ctx.mine_block(miner.clone(), vec![]).await;
    }

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();

    // The singular target == the canonical anchor for the latest ready epoch.
    let target = ctx.consensus.get_validator_attestation_target(outpoint).expect("a ready canonical target");
    let (latest_ready, anchor) = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        (lr, vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the latest ready epoch"))
    };
    assert_eq!(target.epoch, latest_ready, "signs the latest ready epoch");
    assert_eq!(target.target_hash, anchor.anchor_hash, "target is the canonical anchor hash");
    assert_eq!(target.target_daa_score, anchor.anchor_daa_score, "target daa is the canonical anchor daa");
    assert_eq!(target.validator_set_commitment, Hash64::default(), "VSC is a fixed zero (P-1D)");

    // The batch returns every ready, non-duplicate epoch ascending up to the latest.
    let targets = ctx.consensus.get_validator_attestation_targets(outpoint, 0, 100);
    assert!(!targets.is_empty());
    assert!(targets.windows(2).all(|w| w[0].epoch < w[1].epoch), "ascending, unique epochs");
    assert_eq!(targets.last().unwrap().epoch, latest_ready, "the batch reaches the latest ready epoch");
    {
        let vp = ctx.consensus.virtual_processor();
        for t in &targets {
            let a = vp.canonical_anchor_by_blue_score(t.epoch, sink, &dns).expect("each batched epoch has a canonical anchor");
            assert!(!a.duplicate_of_previous_anchor, "duplicate epochs are excluded from the batch");
            assert_eq!(t.target_hash, a.anchor_hash);
            assert_eq!(t.target_daa_score, a.anchor_daa_score);
        }
    }

    // A `from_epoch` past the latest ready epoch yields nothing (no future epochs to sign).
    assert!(ctx.consensus.get_validator_attestation_targets(outpoint, latest_ready + 1, 100).is_empty());
}

/// kaspa-pq DNS v3 (PR6) — high-parallel no-hole: on a WIDE DAG the selected chain's
/// blue_score jumps by the merged-set size, skipping whole epoch [start, end] ranges. Every
/// buried epoch must still resolve to a canonical anchor (the most-recent selected-chain block
/// at-or-below its cutoff — which, for a skipped epoch, is a block below the jump → a
/// correctly-flagged duplicate), NEVER a hole (None / panic). This is the DAG-level analogue of
/// PR2a's pure `no-hole-on-jump` test, exercising the real store walk over a jumpy chain.
#[tokio::test]
async fn dns_v3_high_parallel_blue_score_jump_no_hole() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 16;
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Small epochs vs. wide merges (up to 16) so a single merge jumps past whole epochs.
            dns.attestation_epoch_length_blue_score = 5;
            dns.attestation_lag_blue_score = 3;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 100_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Warm up, then alternate WIDE antichains + single merge blocks so the selected chain's
    // blue_score jumps by the merged set size (skipping whole epoch ranges), then settle.
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    for _ in 0..4 {
        ctx.build_block_template_row(0..14).validate_and_insert_row().await; // wide antichain
        ctx.build_block_template_row(0..1).validate_and_insert_row().await; // merge -> blue_score jump
    }
    for _ in 0..6 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    ctx.assert_tips_num(1);

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let vp = ctx.consensus.virtual_processor();
    let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
    let latest_ready =
        ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("epochs are ready on a chain this long");
    assert!(latest_ready >= 2, "the chain spans several epochs (latest_ready = {latest_ready})");

    // NO HOLE + monotonic: every buried epoch resolves to a canonical anchor whose blue_score is
    // non-decreasing across epochs, even across the blue_score jumps.
    let mut prev_blue = 0u64;
    let mut distinct = std::collections::HashSet::new();
    for e in 0..=latest_ready {
        let a = vp.canonical_anchor_by_blue_score(e, sink, &dns).unwrap_or_else(|| panic!("epoch {e} has no canonical anchor (hole)"));
        assert!(a.anchor_blue_score >= prev_blue, "anchor blue_score is monotonic across epochs");
        prev_blue = a.anchor_blue_score;
        distinct.insert(a.anchor_hash);
    }
    // The wide merges actually skipped >=1 epoch range: fewer distinct anchors than epochs (some
    // epochs share an anchor) — proving the test exercised real blue_score jumps, not a dense chain.
    assert!(
        distinct.len() <= latest_ready as usize,
        "a blue_score jump made >=1 epoch reuse a prior anchor ({} distinct anchors over {} epochs)",
        distinct.len(),
        latest_ready + 1
    );
}

/// kaspa-pq DNS v3 — the validator FUNCTIONS end-to-end: a bonded validator's canonical
/// attestation drives the StakeScore over `required_stake_depth`, so `update_dns_state`
/// promotes the overlay to the `Active` stage AND records a DNS-confirmed anchor — the
/// precondition the §H reorg gate needs to protect finality. Shrunk params: a single
/// validator is the whole active stake, so one fully-attested ready epoch earns exactly
/// `1·SCALE`, clearing `required_stake_depth = SCALE/2`. (Foundation for the 51%-attack test.)
#[tokio::test]
async fn dns_v3_validator_drives_confirmed_anchor() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, headers::HeaderStoreReader};
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DnsRolloutStage, STAKE_SCORE_SCALE, StakeScore, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap(); // GENESIS_ACTIVE: TwoDimensionalDominance
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            // Confirm on a short chain: work threshold trivial, one fully-attested epoch suffices.
            dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
            dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);

    // Fund a bond + a shard-funding coinbase.
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _reward_payload) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);

    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
    };
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    assert_eq!(ctx.consensus.block_status(reward_block.header.hash), BlockStatus::StatusUTXOValid);

    // Mine generously so the shard merges (accepted on the selected chain), the attested epoch
    // buries, and update_dns_state recomputes (it throttles to once per blue_score epoch) with
    // the attestation credited -> stake_depth >= required -> the anchor confirms.
    for _ in 0..15 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let state = {
        let vp = ctx.consensus.virtual_processor();
        vp.dns_state_store.read().get().expect("DnsState is written once the overlay is active")
    };
    assert_eq!(state.rollout_stage, DnsRolloutStage::Active, "one active validator -> Active stage");
    assert!(
        state.stake_depth >= StakeScore(STAKE_SCORE_SCALE / 2),
        "the validator's canonical attestation drove StakeScore over the threshold (got {:?})",
        state.stake_depth
    );
    assert_ne!(
        state.last_dns_confirmed_anchor,
        Hash64::default(),
        "a DNS-confirmed anchor is recorded (the reorg gate now protects it)"
    );
}

/// kaspa-pq DNS v3 (§H finality) — **51%-PoW attack is stopped**: a stake-less attacker that
/// out-mines the honest chain (strictly higher blue_work — a PoW majority) CANNOT rewrite a
/// DNS-confirmed anchor. The honest node bonds a validator and reaches a confirmed anchor;
/// a second consensus instance (the attacker, same genesis) mines a longer STAKE-LESS chain;
/// its heavier blocks are delivered to the honest node, whose sink-search runs the
/// `TwoDimensionalDominance` gate (`dns_reorg_allows`): the candidate exits the confirmed
/// prefix and out-Works but does NOT out-Stake (zero attestations) → `DominanceViolation` →
/// soft-reject. The honest sink therefore STILL contains the confirmed anchor, never the
/// heavier attacker tip — PoW surplus does not substitute for a PoS deficit (the
/// non-substitutability finality property). Completes the PR6-deferred 51%-finality-stop sim.
#[tokio::test]
async fn dns_v3_pow_majority_cannot_rewrite_confirmed_anchor() {
    use crate::model::stores::{dns_state::DnsStateStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader};
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{DnsRolloutStage, STAKE_SCORE_SCALE, StakeScore, ready_epoch_from_tip_blue_score},
    };
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap(); // GENESIS_ACTIVE: TwoDimensionalDominance
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            dns.epoch_length_blocks = 2;
            dns.reward_uniqueness_window_blocks = 50;
            // Large reorg horizon so a from-genesis fork is GATE-ELIGIBLE (the dominance test
            // runs) instead of being auto-rejected as deeper than the horizon.
            dns.max_reorg_horizon_blocks = 1000;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            dns.stake_score_window_blue_score = 10_000;
            dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
            dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // ---- Honest node: bond a validator, attest, reach a DNS-confirmed anchor. ----
    let seed = [0x42u8; 32];
    let v = dns_harness::harness_validator(seed);
    let k_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&v.pubkey).as_bytes();
    let k_spk = p2pkh_mldsa87_spk(&k_payload);
    let k_miner = MinerData::new(k_spk.clone(), vec![]);
    let _b1 = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_a = ctx.mine_block(k_miner.clone(), vec![]).await;
    let h_b = ctx.mine_block(k_miner.clone(), vec![]).await;
    let cb_a = &h_a.transactions[0];
    let (ia, oa) = cb_a.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_a pays K");
    let (coinbase_a, value_a, daa_a) = (TransactionOutpoint::new(cb_a.id(), ia as u32), oa.value, h_a.header.daa_score);
    let cb_b = &h_b.transactions[0];
    let (ib, ob) = cb_b.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == k_spk).expect("h_b pays K");
    let (coinbase_b, value_b, daa_b) = (TransactionOutpoint::new(cb_b.id(), ib as u32), ob.value, h_b.header.daa_score);
    for _ in 0..5 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let storage_mass_parameter = ctx.consensus.params().storage_mass_parameter;
    let (bond_tx, _vid, _rp) =
        dns_harness::funded_signed_bond_tx(seed, coinbase_a, value_a, daa_a, value_a - 100_000, 0, storage_mass_parameter);
    let bond_tx_id = bond_tx.id();
    let bond_block = ctx.mine_block(new_miner_data(), vec![bond_tx]).await;
    assert_eq!(ctx.consensus.block_status(bond_block.header.hash), BlockStatus::StatusUTXOValid);
    let bond_outpoint = TransactionOutpoint::new(bond_tx_id, 0);
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor")
    };
    let att = dns_harness::build_signed_attestation(
        &v,
        genesis_hash.as_byte_slice(),
        bond_outpoint,
        anchor.epoch,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_tx = dns_harness::funded_signed_shard_tx(seed, coinbase_b, value_b, daa_b, att, storage_mass_parameter);
    ctx.mine_block(new_miner_data(), vec![shard_tx]).await;
    for _ in 0..15 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }

    let honest_sink = ctx.consensus.get_sink();
    let (confirmed_anchor, honest_work) = {
        let vp = ctx.consensus.virtual_processor();
        let st = vp.dns_state_store.read().get().expect("DnsState");
        assert_eq!(st.rollout_stage, DnsRolloutStage::Active, "honest node is Active");
        assert_ne!(st.last_dns_confirmed_anchor, Hash64::default(), "honest node has a confirmed anchor");
        (st.last_dns_confirmed_anchor, vp.ghostdag_store.get_blue_work(honest_sink).unwrap())
    };

    // ---- Attacker: a SEPARATE instance (same genesis) mines a longer STAKE-LESS chain. ----
    let mut atk = TestContext::new(TestConsensus::new(&config));
    let mut attacker_blocks = Vec::new();
    for _ in 0..60 {
        attacker_blocks.push(atk.mine_block(new_miner_data(), vec![]).await);
    }
    let attacker_tip = attacker_blocks.last().unwrap().header.hash;
    let attacker_work = { atk.consensus.virtual_processor().ghostdag_store.get_blue_work(attacker_tip).unwrap() };
    assert!(
        attacker_work > honest_work,
        "the attacker is a genuine PoW majority (heavier blue_work): attacker {attacker_work} vs honest {honest_work}"
    );

    // ---- Deliver the attacker's heavier branch to the honest node. ----
    for b in &attacker_blocks {
        ctx.validate_and_insert_block(b.clone()).await;
    }

    // ---- Finality held: the honest sink STILL contains the confirmed anchor, NOT the heavier
    //      attacker tip. PoW surplus could not substitute for the attacker's zero stake. ----
    let new_sink = ctx.consensus.get_sink();
    assert_ne!(new_sink, attacker_tip, "the honest node did NOT reorg onto the heavier stake-less attacker chain");
    {
        let vp = ctx.consensus.virtual_processor();
        assert!(
            vp.reachability_service.is_chain_ancestor_of(confirmed_anchor, new_sink),
            "the DNS-confirmed anchor is still on the selected chain (the reorg gate stopped the 51% attack)"
        );
        // The confirmed anchor is unchanged — finality was not rewritten.
        let st = vp.dns_state_store.read().get().expect("DnsState");
        assert_eq!(st.last_dns_confirmed_anchor, confirmed_anchor, "the confirmed anchor was not rewritten by the attack");
    }
}

/// kaspa-pq DNS v3 — **many validators converge on ONE anchor at the epoch boundary**, the
/// core reason v3 replaces v1 current-sink signing. Under fast mining / a wide DAG, validators
/// transiently observe DIFFERENT sinks (the multi-tip frontier + propagation lag) — v1 had each
/// sign its own differing sink, splitting honest stake below φS. Here we build a wide DAG, take
/// many divergent validator VIEWS (the multiple frontier tips a fast network produces + lagging
/// ancestors at different heights), and show that although their views differ (≥2 distinct
/// blocks → ≥2 distinct v1 sink-targets), every one of them computes the SAME v3 canonical
/// lagged anchor for a buried epoch (exactly 1) — unanimous, so honest stake never splits.
#[tokio::test]
async fn dns_v3_many_validators_agree_on_anchor_under_fast_wide_dag() {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
    use std::collections::HashSet;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 16;
            p.mergeset_size_limit = 16;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            // Moderate epoch + a generous lag so the latest ready epoch's anchor is buried well
            // below the churning multi-tip frontier (where the views diverge) into shared history.
            dns.attestation_epoch_length_blue_score = 5;
            dns.attestation_lag_blue_score = 20;
            dns.attestation_anchor_backoff_blue_score = 2;
            dns.stake_score_window_blue_score = 100_000;
            p.dns_params = Some(dns);
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Fast mining / wide DAG: wide antichains merged repeatedly, ENDING on a wide antichain so the
    // frontier is genuinely multi-tip (the different sinks a fast network's validators observe).
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await;
    }
    for _ in 0..6 {
        ctx.build_block_template_row(0..12).validate_and_insert_row().await; // wide antichain
        ctx.build_block_template_row(0..1).validate_and_insert_row().await; // merge -> blue_score jump
    }
    ctx.build_block_template_row(0..12).validate_and_insert_row().await; // leave a multi-tip frontier

    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();

    // Collect many divergent VALIDATOR VIEWS: every frontier tip (a fast network's competing
    // sinks) + several RECENT lagging ancestors at different heights (validators a little behind
    // on propagation — but still past the readiness threshold, like real honest nodes).
    let mut views: Vec<BlockHash> = ctx.consensus.get_tips().into_iter().collect();
    {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        for anc in vp.reachability_service.default_backward_chain_iterator(sink) {
            let b = vp.headers_store.get_blue_score(anc).unwrap();
            // Only "slightly behind" validators (recent ancestors); stop once we'd reach nodes too
            // far back to have a ready epoch (a genesis-deep view is not a realistic poll state).
            if sink_blue.saturating_sub(b) > 40
                || ready_epoch_from_tip_blue_score(b, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                    .is_none()
            {
                break;
            }
            views.push(anc);
            if views.len() >= 24 {
                break;
            }
        }
    }
    views.sort();
    views.dedup();

    let (anchors, blue_scores, buried_epoch) = {
        let vp = ctx.consensus.virtual_processor();
        // A buried epoch every view agrees is ready: the min over views of each view's latest
        // ready epoch (so canonical_anchor_by_blue_score returns Some for every view).
        let buried_epoch = views
            .iter()
            .map(|t| {
                let b = vp.headers_store.get_blue_score(*t).unwrap();
                ready_epoch_from_tip_blue_score(b, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
                    .expect("each view has at least one ready epoch")
            })
            .min()
            .unwrap();
        let anchors: HashSet<BlockHash> = views
            .iter()
            .map(|t| vp.canonical_anchor_by_blue_score(buried_epoch, *t, &dns).expect("every view resolves the buried epoch").anchor_hash)
            .collect();
        let blue_scores: HashSet<u64> = views.iter().map(|t| vp.headers_store.get_blue_score(*t).unwrap()).collect();
        (anchors, blue_scores, buried_epoch)
    };

    // The views are genuinely divergent (a fast network: many distinct tips at several heights) —
    // under v1 these would be ≥2 different current-sink targets, splitting honest stake.
    assert!(views.len() >= 5, "many validator views ({})", views.len());
    assert!(blue_scores.len() >= 2, "the views sit at genuinely different positions (would be different v1 sinks)");
    // ...yet under v3 every view computes the SAME canonical anchor for the buried epoch.
    assert_eq!(
        anchors.len(),
        1,
        "all {} validator views must agree on ONE canonical anchor for epoch {} (got {} distinct)",
        views.len(),
        buried_epoch,
        anchors.len()
    );
}

/// kaspa-pq Layer-0 (audit M-3, updated for ADR-0007 Phase 3): a header whose
/// `pow_algo_id` is not the algo the network mandates at its DAA score is
/// rejected by header-in-isolation validation. On the BLAKE2b-SHA3-active mainnet
/// params the mandated id is `3`, so both the wrong-but-known Phase-1 id (`1` —
/// a miner trying the cheap kHeavyHash on a BLAKE2b-SHA3 network) and a garbage id
/// (`99`) must be rejected, before the PoW seed — which consumes algo_id — is
/// even derived.
#[tokio::test]
async fn header_with_unknown_pow_algo_id_is_rejected() {
    use kaspa_consensus_core::errors::block::RuleError;
    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // Establish a virtual chain with one valid (template-built ⇒ correct algo id) block.
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();

    // Corrupt the algo id to the wrong-but-known Phase-1 id and re-finalize.
    let mut t = ctx.build_block_template(0, ctx.simulated_time + 1_000);
    t.block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
    t.block.header.finalize();
    let res = ctx.consensus.validate_and_insert_block(t.block.to_immutable()).block_task.await;
    assert!(matches!(res, Err(RuleError::UnknownPowAlgoId(1))), "expected UnknownPowAlgoId(1), got {res:?}");

    // A garbage id is rejected the same way.
    let mut t = ctx.build_block_template(0, ctx.simulated_time + 2_000);
    t.block.header.pow_algo_id = 99;
    t.block.header.finalize();
    let res = ctx.consensus.validate_and_insert_block(t.block.to_immutable()).block_task.await;
    assert!(matches!(res, Err(RuleError::UnknownPowAlgoId(99))), "expected UnknownPowAlgoId(99), got {res:?}");
}

// ============================================================================
// kaspa-pq ADR-0018 §G — DNS-overlay DAG integration harness (foundation).
//
// Retires the "ML-DSA-87 signing unavailable in the consensus test crate"
// blocker for the reward-bearing / reorg / slashing DAG tests (DAG-2..7): these
// helpers let a consensus test build stake-bond + attestation-shard txs and
// produce an attestation signature the §B.4 verifier
// (`kaspa_txscript::verify_mldsa87_with_context` under
// `ATTESTATION_MLDSA87_CONTEXT`) accepts. Funding a bond tx from a coinbase UTXO
// (so a full reward-bearing chain validates) is the next harness step (DAG-2).
// ============================================================================
#[cfg(test)]
mod dns_harness {
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            ATTESTATION_MLDSA87_CONTEXT, DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, StakeAttestation, StakeBondPayload,
            StakeUnbondRequestPayload, UNBOND_REQUEST_CONTEXT, attestations_from_accepted_txs, p2pkh_mldsa87_spk,
            single_attestation_shard, stake_attestation_message, stake_attestation_shard_tx, unbond_request_message,
            validator_id_from_pubkey,
        },
        hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        hashing::sighash_type::SIG_HASH_ALL,
        mass::MassCalculator,
        subnets::{
            SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_SLASHING_EVIDENCE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, SUBNETWORK_ID_STAKE_BOND,
            SUBNETWORK_ID_STAKE_UNBOND,
        },
        tx::{PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// A test validator: an ML-DSA-87 key (re-derived deterministically from
    /// `seed`) plus its 2592-byte pubkey and overlay `validator_id`.
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
    pub(super) fn build_stake_bond_tx(v: &HarnessValidator, amount: u64, activation_daa_score: u64, reward_payload: [u8; 64]) -> Transaction {
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

    /// kaspa-pq ADR-0018 §G (DAG-2): build a FUNDED, ML-DSA-87-signed stake-bond tx.
    /// Spends the matured coinbase UTXO `coinbase_outpoint` (value `coinbase_value`,
    /// paid to this validator's own P2PKH) into output-0 = `amount` locked stake
    /// (P2PKH to the same key), carrying the `StakeBondPayload`. Input-0 is signed
    /// over `calc_mldsa87_signature_hash(.., SIG_HASH_ALL)` under `MLDSA87_TX_CONTEXT`
    /// — the exact 64-byte digest `OpCheckSigMlDsa87` recomputes — so the block
    /// validates through the full script engine (construction == validation).
    /// Returns `(signed tx, validator_id, owner_reward_spk_payload)`.
    pub(super) fn funded_signed_bond_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        amount: u64,
        activation_daa_score: u64,
        storage_mass_parameter: u64,
    ) -> (Transaction, Hash64, [u8; 64]) {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let validator_id = validator_id_from_pubkey(&pubkey);
        // Keyed BLAKE2b-512 address payload (the same digest the spk's OP_BLAKE2B_512
        // recomputes); rewards + the locked-stake output both pay this P2PKH.
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);

        let payload = StakeBondPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            owner_pubkey_hash: validator_id,
            validator_pubkey_hash: validator_id,
            validator_pubkey: pubkey.clone(),
            amount,
            activation_daa_score,
            unbonding_period_blocks: 700,
            owner_reward_spk_payload: reward_payload,
        };
        // input-0 spends the coinbase; output-0 = the locked stake; fee = coinbase_value - amount.
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(amount, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_BOND,
            0,
            borsh::to_vec(&payload).unwrap(),
        );

        // KIP-9 storage-mass commitment: value-based, so independent of the (still
        // empty) signature_script — committing it now matches the validator's
        // `calc_contextual_masses(..).storage_mass` recheck (else WrongMass).
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded bond tx")
            .storage_mass;
        tx.set_mass(storage_mass);

        // Sign input-0 over the SIG_HASH_ALL digest of the (mass-committed) tx.
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x77u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        (tx, validator_id, reward_payload)
    }

    /// kaspa-pq ADR-0018 §G (DAG-2): build a FUNDED, ML-DSA-87-signed attestation
    /// shard tx — the production shape (`build_funded_shard_tx`). A canonical 0-input
    /// shard tx is rejected by the isolation `NoTxInputs` check, so the shard must
    /// spend a (matured) coinbase like any other tx: one P2PKH change output back to
    /// the same key, with the attestation carried verbatim in the payload on
    /// `SUBNETWORK_ID_STAKE_ATTESTATION_SHARD`. Input-0 is ML-DSA-signed over the v2
    /// tx sighash; the storage mass is committed.
    pub(super) fn funded_signed_shard_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        attestation: StakeAttestation,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // The payload is exactly what the canonical zero-input shard builder emits.
        let payload = stake_attestation_shard_tx(&single_attestation_shard(attestation)).payload;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(coinbase_value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
            0,
            payload,
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded shard tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x88u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// kaspa-pq ADR-0018 §G (DAG-6): build a FUNDED, ML-DSA-87-signed slashing-evidence
    /// tx. Spends the matured coinbase `coinbase_outpoint` (paid to this key's P2PKH) with
    /// **no outputs** — the reporter reward is minted by consensus as a side-effect at
    /// `(tx, 0)` (ADR-0013 Addendum C.2), so any declared output would collide with the
    /// mint — carrying the `SlashingEvidencePayload` on `SUBNETWORK_ID_SLASHING_EVIDENCE`.
    /// Input-0 is ML-DSA-87-signed over the v2 sighash and the storage mass is committed,
    /// so the block validates through the full script engine (construction == validation).
    pub(super) fn funded_signed_slashing_evidence_tx(
        seed: [u8; 32],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        evidence: SlashingEvidencePayload,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // Evidence tx: input-0 funds it (the entire value becomes fee), NO outputs.
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![],
            0,
            SUBNETWORK_ID_SLASHING_EVIDENCE,
            0,
            borsh::to_vec(&evidence).unwrap(),
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded slashing-evidence tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x99u8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// kaspa-pq H-06 (unbond lifecycle): build a FUNDED, ML-DSA-87-signed stake-unbond
    /// request tx — the client-side shape an operator submits to exit a bond. Spends the
    /// matured coinbase into one P2PKH change output (subnet `SUBNETWORK_ID_STAKE_UNBOND`),
    /// carrying a `StakeUnbondRequestPayload` whose `signature` is the owner's ML-DSA-87
    /// signature over `unbond_request_message(bond)` under `UNBOND_REQUEST_CONTEXT`
    /// (the digest the stateful `unbond_request_authorized` rule reconstructs). Input-0 is
    /// signed over the v2 tx sighash so the block validates through the script engine.
    pub(super) fn funded_signed_unbond_tx(
        seed: [u8; 32],
        net_id: &[u8],
        coinbase_outpoint: TransactionOutpoint,
        coinbase_value: u64,
        coinbase_daa_score: u64,
        bond_outpoint: TransactionOutpoint,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // Owner authorization: ML-DSA-87 signature over the network- and bond-bound unbond message (M-04).
        let auth_digest = unbond_request_message(net_id, bond_outpoint);
        let auth_sig = mldsa::sign(&kp.signing_key, &auth_digest.as_bytes()[..], UNBOND_REQUEST_CONTEXT, [0xaau8; 32])
            .expect("ML-DSA-87 unbond authorization sign");
        let payload = borsh::to_vec(&StakeUnbondRequestPayload {
            version: DNS_PAYLOAD_VERSION_V1,
            bond_outpoint,
            owner_pubkey: pubkey.clone(),
            signature: auth_sig.as_ref().to_vec(),
        })
        .unwrap();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(coinbase_outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(coinbase_value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_UNBOND,
            0,
            payload,
        );
        let utxo = UtxoEntry::new(coinbase_value, spk, coinbase_daa_score, true);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the funded unbond tx")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xabu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// Build a FUNDED, ML-DSA-87-signed NATIVE spend of a P2PKH UTXO (e.g. a bond's locked
    /// output-0) back to the same key. Exercises the ADR-0016 §D.2 bond-UTXO spend-gate: consensus
    /// must reject a block that spends a still-locked (non-releasable) bond output. The spent output
    /// is a regular (non-coinbase) tx output; the sighash commits its value + spk (both supplied),
    /// so the signature verifies through the full script engine.
    pub(super) fn funded_signed_p2pkh_spend(
        seed: [u8; 32],
        outpoint: TransactionOutpoint,
        value: u64,
        daa_score: u64,
        storage_mass_parameter: u64,
    ) -> Transaction {
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(value - 100_000, spk.clone())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let utxo = UtxoEntry::new(value, spk, daa_score, false);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the bond-output spend")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xacu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// ADR-0018 §F bridge wiring: a fully ML-DSA-87-signed BRIDGE tx — spends `seed`'s
    /// P2PKH `outpoint` into a single `EVM_DEPOSIT_LOCK` output (`value − 100_000`; fee
    /// 100_000), whose refund path is the same key's P2PKH. Mirrors
    /// [`funded_signed_p2pkh_spend`]; the lock output makes the tx **finality-class**
    /// past `finality_fee_activation_daa_score`.
    pub(super) fn funded_signed_deposit_lock_tx(
        seed: [u8; 32],
        outpoint: TransactionOutpoint,
        value: u64,
        daa_score: u64,
        storage_mass_parameter: u64,
    ) -> Transaction {
        use kaspa_txscript::script_class::evm_deposit_lock_script;
        let kp = mldsa::generate_key_pair(seed);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let reward_payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        let spk = p2pkh_mldsa87_spk(&reward_payload);
        // The deposit lock: 20-byte EVM address, far-future timeout (refund path never taken
        // here), small claim tip; refund = the spender's own ML-DSA P2PKH.
        let lock_spk = evm_deposit_lock_script([0xABu8; 20], 100_000_000, 7, spk.script());
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(value - 100_000, lock_spk)],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let utxo = UtxoEntry::new(value, spk, daa_score, false);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![utxo.clone()]))
            .expect("contextual mass is computable for the deposit-lock spend")
            .storage_mass;
        tx.set_mass(storage_mass);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sig_hash = {
            let populated = PopulatedTransaction::new(&tx, vec![utxo]);
            calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
        };
        let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0xadu8; 32])
            .expect("ML-DSA-87 sign on the 64-byte sighash");
        let mut sig_item = sig.as_ref().to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        let sig_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .expect("ML-DSA-87 signature push fits MAX_SCRIPT_ELEMENT_SIZE")
            .add_data(&pubkey)
            .expect("ML-DSA-87 public-key push fits MAX_SCRIPT_ELEMENT_SIZE")
            .drain();
        tx.inputs[0].signature_script = sig_script;
        tx
    }

    /// Build a fully ML-DSA-87-signed attestation for `bond_outpoint`, signing
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
        let sig = mldsa::sign(&kp.signing_key, &mb[..], ATTESTATION_MLDSA87_CONTEXT, [0x55u8; 32]).expect("ml-dsa-87 sign");
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
        let bond_tx = build_stake_bond_tx(&v, 10_000_000_000, 0, [0x33u8; 64]);
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
            kaspa_txscript::verify_mldsa87_with_context(&v.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA87_CONTEXT).unwrap(),
            "the §B.4 verifier must accept the harness-signed attestation"
        );
        // A different key must NOT verify (sanity).
        let v2 = harness_validator([0x99u8; 32]);
        assert!(!kaspa_txscript::verify_mldsa87_with_context(&v2.pubkey, &mb[..], &att.signature, ATTESTATION_MLDSA87_CONTEXT).unwrap());

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
    // kaspa-pq PQ-only: coinbase outputs must be the standard ML-DSA-87 P2PKH class
    // (enforced with no exemption — see `check_transaction_pq_output_classes`). Use a
    // random 64-byte hash payload: the script is class-valid but unspendable (no
    // preimage), which is all this helper needs, and stays distinct per call.
    let mut payload = [0u8; 64];
    for b in payload.iter_mut() {
        *b = rand::random();
    }
    MinerData::new(p2pkh_mldsa87_spk(&payload), vec![])
}


/// kaspa-pq EVM Lane v0.4 (ADR-0020) — first EVM-ACTIVE pipeline integration
/// test: with `evm_activation_daa_score = 0`, real blocks inserted through the
/// full pipeline (header → body → virtual) drive the lazy chain-context step:
/// each chain block's mergeset acceptance executes ONCE, its result + state
/// snapshot persist atomically with its UTXO diff, the canonical EVM heads
/// move with the sink, a commitment fault disqualifies the block from the
/// chain (the block stays in the DAG — no poison), and the chain recovers
/// past the disqualified block without re-executing prior EVM results.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_active_chain_executes_persists_and_moves_heads() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmPayloadStoreReader, EvmRawTxStoreReader};
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EvmStateSnapshot};
    use kaspa_evm::EvmBlockInput;
    use kaspa_hashes::Hash64;

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| p.evm_activation_daa_score = 0)
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();

    let genesis = consensus.params().genesis.hash;
    let miner_data = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);

    // ---- b1: empty payload. The §4.3 version rule demands v2 post-activation;
    // the producer (this test) computes the mergeset-acceptance commitment the
    // same way the verifier will (mergeset = [genesis], no payloads ⇒ no
    // accepted txs; EVM parent = none ⇒ genesis state).
    let payload1 = EvmExecutionPayload::default();
    let mut b1 = consensus.build_utxo_valid_block_with_parents(1.into(), vec![genesis], miner_data.clone(), vec![]);
    b1.header.version = EVM_HEADER_VERSION;
    b1.header.evm_payload_hash = payload1.payload_hash();
    let input1 = EvmBlockInput {
        parent: None,
        header_timestamp_ms: b1.header.timestamp,
        selected_parent_hash: genesis.as_bytes(),
        blue_work_be: b1.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b1.header.daa_score,
        payload: &payload1,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let (exp1, snap1) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input1).unwrap();
    b1.header.evm_commitment_root = exp1.header.commitment_root();
    b1.evm_payload = payload1;
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();

    assert_eq!(storage.evm_header_store.get(1.into()).unwrap(), exp1.header, "b1's EVM result persisted by the pipeline");
    assert_eq!(exp1.header.evm_number, 1);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(1u64), "heads moved to the sink");

    // ---- b2: carries its OWN non-empty payload (a declared coinbase + extra
    // data) — proving payload persistence at body commit and EVM state chaining
    // b1 → b2 through the real pipeline. (A real DepositClaim needs a funded
    // EVM_DEPOSIT_LOCK UTXO — P4 claim validation rejects a dangling one; the
    // claim/bridge paths are unit-tested in processes::evm.)
    let payload2 = EvmExecutionPayload {
        evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
        extra_data: vec![0x4D, 0x53, 0x4B],
        ..Default::default()
    };
    let mut b2 = consensus.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner_data.clone(), vec![]);
    b2.header.version = EVM_HEADER_VERSION;
    b2.header.evm_payload_hash = payload2.payload_hash();
    let input2 = EvmBlockInput {
        parent: Some(&exp1.header),
        header_timestamp_ms: b2.header.timestamp,
        selected_parent_hash: BlockHash::from(1u64).as_bytes(),
        blue_work_be: b2.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b2.header.daa_score,
        payload: &payload2,
        accepted_txs: &[], // b1's payload was empty ⇒ nothing to accept
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let (exp2, _snap2) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
    b2.header.evm_commitment_root = exp2.header.commitment_root();
    b2.evm_payload = payload2.clone();
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();

    let stored2 = storage.evm_header_store.get(2.into()).unwrap();
    assert_eq!(stored2, exp2.header);
    assert_eq!(stored2.evm_number, 2);
    assert_eq!(stored2.parent_state_root, exp1.header.state_root, "EVM state chains selected-parent-wise");
    assert_eq!(storage.evm_payload_store.get(2.into()).unwrap(), payload2, "own payload persisted at body commit");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64));

    // ---- b3: a commitment FAULT (producer lied about the acceptance result).
    // The block enters the DAG but is disqualified from the chain — exactly the
    // UTXO-fault shape — and no EVM rows are written for it.
    let payload3 = EvmExecutionPayload::default();
    let mut b3 = consensus.build_utxo_valid_block_with_parents(3.into(), vec![2.into()], miner_data.clone(), vec![]);
    b3.header.version = EVM_HEADER_VERSION;
    b3.header.evm_payload_hash = payload3.payload_hash();
    b3.header.evm_commitment_root = Hash64::from_bytes([0xEE; 64]);
    b3.evm_payload = payload3.clone();
    let _ = consensus.validate_and_insert_block(b3.to_immutable()).virtual_state_task.await;
    assert_eq!(consensus.block_status(3.into()), BlockStatus::StatusDisqualifiedFromChain, "commitment mismatch ⇒ chain-disqualified");
    assert!(!storage.evm_header_store.has(3.into()).unwrap(), "no EVM rows for a disqualified block");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64), "heads did NOT follow the faulty block");

    // ---- b4: a valid sibling continuation on b2 — the chain recovers past the
    // disqualified b3 (b3 ∉ past(b4), so b3's payload is NOT accepted by b4)
    // and the heads advance. b1/b2 results are reused (their diffs exist ⇒ the
    // KeyNotFound execution arm is never re-entered: no re-execution on reorg).
    let payload4 = EvmExecutionPayload::default();
    let mut b4 = consensus.build_utxo_valid_block_with_parents(4.into(), vec![2.into()], miner_data, vec![]);
    b4.header.version = EVM_HEADER_VERSION;
    b4.header.evm_payload_hash = payload4.payload_hash();
    let input4 = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: b4.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: b4.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b4.header.daa_score,
        payload: &payload4,
        accepted_txs: &[], // b2's payload txs are empty (system ops are not delayed-accepted)
        gas_pool_v2_activation_daa_score: u64::MAX,
        f002_withdraw_cap_activation_daa_score: u64::MAX,
        f003_mldsa_verify_activation_daa_score: u64::MAX,
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let snap2 = {
        // Recompute b2's child snapshot the same way the node stored it.
        let (_, s) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
        s
    };
    let (exp4, _) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &input4).unwrap();
    b4.header.evm_commitment_root = exp4.header.commitment_root();
    b4.evm_payload = payload4;
    consensus.validate_and_insert_block(b4.to_immutable()).virtual_state_task.await.unwrap();

    assert_eq!(storage.evm_header_store.get(4.into()).unwrap().evm_number, 3, "b4 is EVM block 3 on the selected chain");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(4u64), "heads recovered past the disqualified block");

    // ---- b5: the node's OWN template (§15 producer path) — the builder must
    // declare v2, commit the (empty) payload hash and the REAL acceptance
    // commitment, and the resulting block must validate through the full
    // pipeline. (Template used as-is: on an evm-active net a miner must not
    // mutate the template timestamp — the commitment derives from it.)
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    assert_eq!(template.block.header.version, EVM_HEADER_VERSION, "evm-active template declares v2");
    assert_eq!(template.block.header.evm_payload_hash, EvmExecutionPayload::default().payload_hash());
    assert_ne!(template.block.header.evm_commitment_root, Hash64::default(), "the template committed a real acceptance result");
    let mut b5 = template.block;
    b5.header.hash = 5u64.into(); // test identity (PoW skipped)
    consensus.validate_and_insert_block(b5.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_header_store.has(5.into()).unwrap(), "the self-mined block executed + persisted");
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(5u64));

    // ---- b6 (§16-1): a template with an EVM-mempool candidate — the §15
    // step-6 own-payload path. The fixture is a signed EIP-1559 transfer on
    // EVM_CHAIN_ID (regenerate: `cargo test -p kaspa-evm fixture_generator --
    // --ignored --nocapture`); its sender is UNFUNDED, which is irrelevant for
    // inclusion (data-only) and makes acceptance a deterministic class-2 skip.
    const FIXTURE_TX_NONCE0: &str = "02f86b834d534b8080843b9aca008252089400000000000000000000000000000000000000228201f480c001a03244f5d74a96a52bd1c42fa1b9c336f4d3ae5509190ed9a526f17971c7fd743ca07f58e09399b50636b84f0ae4a7634c60a11c6f32427b613ebf6f4a638d6c68c1";
    let mut raw_n0 = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
    faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw_n0).unwrap();

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            kaspa_consensus_core::evm::EvmTemplateData {
                evm_coinbase: kaspa_consensus_core::evm::EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![raw_n0.clone()],
                system_ops: vec![],
            },
        )
        .unwrap();
    assert_eq!(template.block.evm_payload.transactions, vec![raw_n0.clone()], "the candidate landed in the own payload");
    assert_eq!(
        template.block.evm_payload.evm_coinbase,
        kaspa_consensus_core::evm::EvmAddress::from_bytes([0xCB; 20]),
        "the declared fee recipient landed as the payload coinbase (§8.2)"
    );
    assert_eq!(
        template.block.header.evm_payload_hash,
        template.block.evm_payload.payload_hash(),
        "the header commits the NON-empty payload"
    );
    let mut b6 = template.block;
    b6.header.hash = 6u64.into();
    consensus.validate_and_insert_block(b6.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_payload_store.has(6.into()).unwrap(), "the non-empty own payload persisted at commit_body");

    // ---- b7: the NEXT template accepts b6's payload (mergeset delayed
    // acceptance): the unfunded sender makes the tx a deterministic class-2
    // skip — counted, no receipt, block valid. This closes the full §16-1
    // loop: pool candidate → template inclusion → wire/body validation →
    // acceptance processing by the selected child.
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let mut b7 = template.block;
    b7.header.hash = 7u64.into();
    consensus.validate_and_insert_block(b7.to_immutable()).virtual_state_task.await.unwrap();
    let h7 = storage.evm_header_store.get(7.into()).unwrap();
    assert_eq!(h7.skipped_tx_count, 1, "b6's unfunded payload tx was class-2 skipped at acceptance");
    assert_eq!(h7.accepted_tx_count, 0);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(7u64));

    // §16-3: the tx-lookup index recorded the journey — included in b6 (DA
    // visibility), never accepted, last skip = class 2 (unfunded sender). The
    // exact data misaka_getTxInclusionStatus serves.
    let fixture_hash = kaspa_evm::tx::tx_hash(&{
        let mut raw = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
        faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw).unwrap();
        raw
    });
    let row = storage.evm_tx_index_store.get_or_default(fixture_hash).unwrap();
    assert_eq!(row.included_in, vec![BlockHash::from(6u64)], "DA visibility: the payload block carrying the tx");
    assert!(row.accepted_in.is_empty(), "never executed (unfunded)");
    assert_eq!(row.last_skip_class, Some(2));

    // audit R-2: the raw tx is resolvable DIRECTLY by hash (no included_in scan),
    // recorded at body commit of its carrying payload block (b6) — the path the
    // eth_getTransactionByHash/receipt adapter now uses.
    let stored = storage.evm_raw_tx_store.get(fixture_hash).unwrap().expect("raw tx indexed by hash");
    assert_eq!(stored.raw, raw_n0, "raw EIP-2718 bytes round-trip by hash");
    assert_eq!(stored.payload_block, BlockHash::from(6u64), "carrying payload block recorded");
    assert_eq!(
        consensus.consensus_clone().get_evm_raw_tx(fixture_hash).unwrap(),
        Some(raw_n0.clone()),
        "get_evm_raw_tx resolves the tx without the bounded included_in scan"
    );

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 (§16 RPC / canonical-index fix, R-1): the
/// `evm_number → L1 hash` map is driven by the SELECTED chain at virtual commit,
/// NOT per-block result-commit. A reorg must detach the old canonical block's
/// number and attach the new chain's block at that number; the detached block
/// stays queryable by L1 hash (immutable rows are kept). This exercises
/// `update_evm_canonical_number_map` end-to-end. The conditional-release branch
/// is unit-tested in `model::stores::evm` (`evm_number_store_canonical_*`). The
/// precise sink-search-loser shadow that motivated the fix needs the DNS
/// reorg-gate (overlay-Active); the structural fix prevents it by construction —
/// a non-selected block never writes the map.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_active_canonical_number_map_follows_reorg() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmNumberStoreReader};
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmStateSnapshot};
    use kaspa_evm::EvmBlockInput;

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| p.evm_activation_daa_score = 0)
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();

    let genesis = consensus.params().genesis.hash;
    let miner_data = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);
    let inert = u64::MAX;

    // ---- b1 (#1): empty payload on genesis (mirrors the EVM-active test).
    let payload1 = EvmExecutionPayload::default();
    let mut b1 = consensus.build_utxo_valid_block_with_parents(1.into(), vec![genesis], miner_data.clone(), vec![]);
    b1.header.version = EVM_HEADER_VERSION;
    b1.header.evm_payload_hash = payload1.payload_hash();
    let input1 = EvmBlockInput {
        parent: None,
        header_timestamp_ms: b1.header.timestamp,
        selected_parent_hash: genesis.as_bytes(),
        blue_work_be: b1.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b1.header.daa_score,
        payload: &payload1,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (exp1, snap1) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input1).unwrap();
    b1.header.evm_commitment_root = exp1.header.commitment_root();
    b1.evm_payload = payload1;
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();

    // ---- b2 (#2): on b1.
    let payload2 = EvmExecutionPayload::default();
    let mut b2 = consensus.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner_data.clone(), vec![]);
    b2.header.version = EVM_HEADER_VERSION;
    b2.header.evm_payload_hash = payload2.payload_hash();
    let input2 = EvmBlockInput {
        parent: Some(&exp1.header),
        header_timestamp_ms: b2.header.timestamp,
        selected_parent_hash: BlockHash::from(1u64).as_bytes(),
        blue_work_be: b2.header.blue_work.to_be_bytes().to_vec(),
        daa_score: b2.header.daa_score,
        payload: &payload2,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (exp2, snap2) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap1, &input2).unwrap();
    b2.header.evm_commitment_root = exp2.header.commitment_root();
    b2.evm_payload = payload2;
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_number_store.get(2).unwrap(), Some(BlockHash::from(2u64)), "b2 claims #2");

    // ---- x3 (#3) on b2 — the initial sink. Hash 9 wins the equal-blue-work
    // tiebreak vs y3 (hash 5), so x3 stays canonical at #3 until y4 reorgs.
    let payloadx = EvmExecutionPayload::default();
    let mut x3 = consensus.build_utxo_valid_block_with_parents(9.into(), vec![2.into()], miner_data.clone(), vec![]);
    x3.header.version = EVM_HEADER_VERSION;
    x3.header.evm_payload_hash = payloadx.payload_hash();
    let inputx = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: x3.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: x3.header.blue_work.to_be_bytes().to_vec(),
        daa_score: x3.header.daa_score,
        payload: &payloadx,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expx, _snapx) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &inputx).unwrap();
    assert_eq!(expx.header.evm_number, 3);
    x3.header.evm_commitment_root = expx.header.commitment_root();
    x3.evm_payload = payloadx;
    consensus.validate_and_insert_block(x3.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(9u64), "x3 is the sink");
    assert_eq!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "x3 canonical at #3 before the reorg");

    // ---- y3 (#3) on b2 — a sibling of x3. Equal blue work, lower hash (5 < 9)
    // ⇒ x3 keeps the sink; y3 is inserted but not yet selected/validated.
    let payloady3 = EvmExecutionPayload::default();
    let mut y3 = consensus.build_utxo_valid_block_with_parents(5.into(), vec![2.into()], miner_data.clone(), vec![]);
    y3.header.version = EVM_HEADER_VERSION;
    y3.header.evm_payload_hash = payloady3.payload_hash();
    let inputy3 = EvmBlockInput {
        parent: Some(&exp2.header),
        header_timestamp_ms: y3.header.timestamp,
        selected_parent_hash: BlockHash::from(2u64).as_bytes(),
        blue_work_be: y3.header.blue_work.to_be_bytes().to_vec(),
        daa_score: y3.header.daa_score,
        payload: &payloady3,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expy3, snapy3) = kaspa_evm::snapshot::execute_block_from_snapshot(&snap2, &inputy3).unwrap();
    y3.header.evm_commitment_root = expy3.header.commitment_root();
    y3.evm_payload = payloady3;
    consensus.validate_and_insert_block(y3.to_immutable()).virtual_state_task.await.unwrap();
    assert_eq!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "x3 still canonical at #3 (y3 not selected)");

    // ---- y4 (#4) on y3 — the heavier branch (2 blocks past b2) reorgs the sink
    // from x3 to y4. The selected chain is now ...b2, y3(#3), y4(#4).
    let payloady4 = EvmExecutionPayload::default();
    let mut y4 = consensus.build_utxo_valid_block_with_parents(6.into(), vec![5.into()], miner_data, vec![]);
    y4.header.version = EVM_HEADER_VERSION;
    y4.header.evm_payload_hash = payloady4.payload_hash();
    let inputy4 = EvmBlockInput {
        parent: Some(&expy3.header),
        header_timestamp_ms: y4.header.timestamp,
        selected_parent_hash: BlockHash::from(5u64).as_bytes(),
        blue_work_be: y4.header.blue_work.to_be_bytes().to_vec(),
        daa_score: y4.header.daa_score,
        payload: &payloady4,
        accepted_txs: &[],
        gas_pool_v2_activation_daa_score: inert,
        f002_withdraw_cap_activation_daa_score: inert,
        f003_mldsa_verify_activation_daa_score: inert,
        typed_receipt_root_activation_daa_score: inert,
    };
    let (expy4, _snapy4) = kaspa_evm::snapshot::execute_block_from_snapshot(&snapy3, &inputy4).unwrap();
    assert_eq!(expy4.header.evm_number, 4);
    y4.header.evm_commitment_root = expy4.header.commitment_root();
    y4.evm_payload = payloady4;
    consensus.validate_and_insert_block(y4.to_immutable()).virtual_state_task.await.unwrap();

    // The reorg detached x3 and attached y3(#3) + y4(#4):
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(6u64), "sink reorged to y4");
    assert_eq!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(5u64)), "#3 now resolves to y3 (canonical), not the detached x3");
    assert_eq!(storage.evm_number_store.get(4).unwrap(), Some(BlockHash::from(6u64)), "y4 claimed #4");
    assert_eq!(storage.evm_number_store.get(2).unwrap(), Some(BlockHash::from(2u64)), "#2 (below the fork) is unchanged");
    assert_ne!(storage.evm_number_store.get(3).unwrap(), Some(BlockHash::from(9u64)), "the detached x3 no longer owns #3");
    // The detached x3 stays queryable by L1 hash (immutable rows survive).
    assert!(storage.evm_header_store.has(9.into()).unwrap(), "x3's immutable EVM rows survive the reorg (hash-queryable)");

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 §9.2 — producer-side deposit-claim path: a queued
/// `DepositClaim` (resolved from a real EVM_DEPOSIT_LOCK UTXO, the work the
/// `submitEvmDepositClaim` RPC does) lands in the node's OWN template
/// `system_ops` after the template path re-validates it against the live claim
/// view; a claim for a non-existent/stale lock is dropped — so a queued claim
/// can never make the producer's own block invalid. This closes the production
/// half of the bridge: deposits are now both validatable (P4) AND producible.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_producer_deposit_claim_fills_and_filters_template_system_ops() {
    use kaspa_consensus_core::evm::{DepositClaim, EvmAddress, EvmSystemOp, EvmTemplateData};
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::muhash::MuHashExtensions;
    use kaspa_consensus_core::tx::UtxoEntry;
    use kaspa_muhash::MuHash;
    use kaspa_txscript::script_class::evm_deposit_lock_script;

    kaspa_core::log::try_init_logger("info");

    // A real EVM_DEPOSIT_LOCK output: 1000 sompi locked to an EVM address, claim
    // tip 7, timeout far in the future; refund = a standard ML-DSA P2PKH.
    let evm_addr = [0xAB; 20];
    let refund_spk = p2pkh_mldsa87_spk(&[0x42; 64]);
    let lock_spk = evm_deposit_lock_script(evm_addr, 1_000_000, 7, refund_spk.script());
    let lock_outpoint = TransactionOutpoint::new(99u64.into(), 0);
    let initial_utxos = [(
        lock_outpoint,
        UtxoEntry { amount: 1000, script_public_key: lock_spk, block_daa_score: 0, is_coinbase: false },
    )];

    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| p.evm_activation_daa_score = 0)
        .apply_args(|cfg| {
            let mut ms = MuHash::new();
            initial_utxos.iter().for_each(|(op, u)| ms.add_utxo(op, u));
            cfg.params.genesis.utxo_commitment = ms.finalize();
            let genesis_header: Header = (&cfg.params.genesis).into();
            cfg.params.genesis.hash = genesis_header.hash;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let mut genesis_ms = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&initial_utxos, &mut genesis_ms);
    consensus.import_pruning_point_utxo_set(config.genesis.hash, genesis_ms).unwrap();

    // (1) the valid claim for the real lock; (2) a claim for a non-existent
    // outpoint that re-validation must drop.
    let good_claim = DepositClaim {
        deposit_outpoint: lock_outpoint,
        evm_address: EvmAddress::from_bytes(evm_addr),
        amount_sompi: 1000,
        claim_tip_sompi: 7,
    };
    let bogus_claim = DepositClaim {
        deposit_outpoint: TransactionOutpoint::new(123u64.into(), 0),
        evm_address: EvmAddress::from_bytes([0xCD; 20]),
        amount_sompi: 500,
        claim_tip_sompi: 0,
    };

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            EvmTemplateData {
                evm_coinbase: EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![],
                system_ops: vec![good_claim.clone(), bogus_claim],
            },
        )
        .unwrap();

    assert_eq!(template.block.evm_payload.system_ops.len(), 1, "only the valid claim survives template re-validation");
    assert_eq!(
        template.block.evm_payload.system_ops[0],
        EvmSystemOp::DepositClaim(good_claim),
        "the resolved lock's claim is in the own payload"
    );
    assert_eq!(
        template.block.evm_payload.evm_coinbase,
        EvmAddress::from_bytes([0xCB; 20]),
        "a claim-bearing payload declares the coinbase (the tip routes to it)"
    );
    assert_eq!(template.block.header.evm_payload_hash, template.block.evm_payload.payload_hash(), "header commits the claim payload");

    consensus.shutdown(wait_handles);
}

/// kaspa-pq EVM Lane v0.4 §14.1/§14.3 — Y9 budget independence, pipeline e2e:
/// a template assembled from an OVERSUPPLIED candidate list fills the payload
/// to the byte cap (and no further), the resulting full-cap block keeps its
/// normal UTXO content and passes the complete pipeline (mass rules included),
/// and the next chain block processes the entire payload at acceptance without
/// invalidating or stalling the UTXO lane. Complements the in-isolation mass
/// equality test (`evm_y9_payload_byte_budget_independent_of_utxo_mass_budget`);
/// the λ·D propagation re-validation with measured payload-laden D is Y10 —
/// testnet work and an activation precondition (§14.3), not a unit concern.
#[tokio::test]
#[cfg(feature = "evm")]
async fn evm_y9_full_cap_payload_block_validates_and_executes() {
    use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader, EvmPayloadStoreReader};
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EvmTemplateData, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};

    kaspa_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| p.evm_activation_daa_score = 0)
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let storage = consensus.consensus_clone().storage.clone();

    // The class-1-valid §16 fixture, oversupplied: duplication is legal at the
    // body (admission is per-tx) and a deterministic skip at acceptance.
    const FIXTURE_TX_NONCE0: &str = "02f86b834d534b8080843b9aca008252089400000000000000000000000000000000000000228201f480c001a03244f5d74a96a52bd1c42fa1b9c336f4d3ae5509190ed9a526f17971c7fd743ca07f58e09399b50636b84f0ae4a7634c60a11c6f32427b613ebf6f4a638d6c68c1";
    let mut raw = vec![0u8; FIXTURE_TX_NONCE0.len() / 2];
    faster_hex::hex_decode(FIXTURE_TX_NONCE0.as_bytes(), &mut raw).unwrap();
    let base = EvmExecutionPayload::default().payload_bytes().len();
    let per_tx = 4 + raw.len();
    let n = (MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK - base) / per_tx;

    let template = consensus
        .build_block_template_with_evm(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
            EvmTemplateData {
                evm_coinbase: EvmAddress::from_bytes([0xCB; 20]),
                transactions: vec![raw.clone(); n + 32], // 32 candidates beyond the cap
                system_ops: vec![],
            },
        )
        .unwrap();
    assert_eq!(template.block.evm_payload.transactions.len(), n, "template fills to the byte cap and not one tx further");
    let assembled = template.block.evm_payload.payload_bytes().len();
    assert!(assembled <= MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, "assembled payload within the cap");
    assert!(assembled > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK - per_tx, "assembled payload NEAR the cap");

    let mut b1 = template.block;
    b1.header.hash = 1u64.into();
    consensus.validate_and_insert_block(b1.to_immutable()).virtual_state_task.await.unwrap();
    assert!(storage.evm_payload_store.has(1.into()).unwrap(), "full-cap payload persisted at commit_body");

    // The next chain block accepts b1's payload: every copy is a deterministic
    // skip (unfunded sender), the block stays valid, the heads advance — a
    // payload-maxed DAG block never blocks the UTXO lane (§14.2).
    let template = consensus
        .build_block_template(
            MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]),
            Box::new(OnetimeTxSelector::new(Default::default())),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let mut b2 = template.block;
    b2.header.hash = 2u64.into();
    consensus.validate_and_insert_block(b2.to_immutable()).virtual_state_task.await.unwrap();
    let h2 = storage.evm_header_store.get(2.into()).unwrap();
    assert_eq!(h2.skipped_tx_count, n as u32, "the full-cap payload was processed: every copy skipped, none accepted");
    assert_eq!(h2.accepted_tx_count, 0);
    assert_eq!(storage.evm_heads_store.read().get().unwrap().latest, BlockHash::from(2u64));

    consensus.shutdown(wait_handles);
}
