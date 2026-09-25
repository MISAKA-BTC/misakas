use super::coinbase_mock::CoinbaseManagerMock;
use kaspa_consensus_core::{
    api::{
        ConsensusApi,
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    },
    block::{BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector, VirtualStateApproxId},
    coinbase::MinerData,
    constants::BLOCK_VERSION,
    errors::{
        block::RuleError,
        coinbase::CoinbaseResult,
        tx::{TxResult, TxRuleError},
    },
    header::{CompressedParents, Header},
    mass::{ContextualMasses, NonContextualMasses, transaction_estimated_serialized_size},
    merkle::calc_hash_merkle_root,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_core::time::unix_now;
use kaspa_hashes::{Hash64, ZERO_HASH64}; // kaspa-pq: block ids + merkle roots + utxo_commitment are all Hash64

use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

pub(crate) struct ConsensusMock {
    transactions: RwLock<HashMap<TransactionId, Arc<Transaction>>>,
    statuses: RwLock<HashMap<TransactionId, TxResult<()>>>,
    utxos: RwLock<UtxoCollection>,
    /// kaspa-pq audit v24 (H-1): a settable sink blue score so attestation-overlay tests can
    /// drive the latest-ready-epoch computation. Default `0` (no ready epoch) for legacy tests.
    sink_blue_score: RwLock<u64>,
    /// M1: the `(bond, class)` rows whose possession proofs escalate at the mock's tip, and how urgently.
    palw_readiness_urgency: RwLock<
        HashMap<
            (kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, Hash64),
            kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1,
        >,
    >,
}

impl ConsensusMock {
    pub(crate) fn new() -> Self {
        Self {
            transactions: RwLock::new(HashMap::default()),
            statuses: RwLock::new(HashMap::default()),
            utxos: RwLock::new(HashMap::default()),
            sink_blue_score: RwLock::new(0),
            palw_readiness_urgency: RwLock::new(Default::default()),
        }
    }

    /// M1: how urgently the row `(bond, class)` needs its proof at the mock's tip (`None`: it does not).
    #[allow(dead_code)]
    pub(crate) fn set_palw_readiness_urgency(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        class_id: Hash64,
        urgency: Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>,
    ) {
        match urgency {
            Some(urgency) => self.palw_readiness_urgency.write().insert((bond, class_id), urgency),
            None => self.palw_readiness_urgency.write().remove(&(bond, class_id)),
        };
    }

    /// kaspa-pq audit v24 (H-1): set the mock sink blue score (for attestation-overlay tests).
    #[allow(dead_code)]
    pub(crate) fn set_sink_blue_score(&self, blue_score: u64) {
        *self.sink_blue_score.write() = blue_score;
    }

    pub(crate) fn set_status(&self, transaction_id: TransactionId, status: TxResult<()>) {
        self.statuses.write().insert(transaction_id, status);
    }

    pub(crate) fn add_transaction(&self, transaction: Transaction, block_daa_score: u64) {
        let transaction = MutableTransaction::from_tx(transaction);
        let mut transactions = self.transactions.write();
        let mut utxos = self.utxos.write();

        // Remove the spent UTXOs
        transaction.tx.inputs.iter().for_each(|x| {
            utxos.remove(&x.previous_outpoint);
        });
        // Create the new UTXOs
        transaction.tx.outputs.iter().enumerate().for_each(|(i, x)| {
            utxos.insert(
                TransactionOutpoint::new(transaction.id(), i as u32),
                UtxoEntry::new(x.value, x.script_public_key.clone(), block_daa_score, transaction.tx.is_coinbase()),
            );
        });
        // Register the transaction
        transactions.insert(transaction.id(), transaction.tx);
    }

    pub(crate) fn can_finance_transaction(&self, transaction: &MutableTransaction) -> bool {
        let utxos = self.utxos.read();
        for outpoint in transaction.missing_outpoints() {
            if !utxos.contains_key(&outpoint) {
                return false;
            }
        }
        true
    }
}

impl ConsensusApi for ConsensusMock {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        mut tx_selector: Box<dyn TemplateTransactionSelector>,
        _build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        let mut txs = tx_selector.select_transactions();
        let coinbase_manager = CoinbaseManagerMock::new();
        let coinbase = coinbase_manager.expected_coinbase_transaction(miner_data.clone());
        txs.insert(0, coinbase.tx);
        let now = unix_now();
        let hash_merkle_root = self.calc_transaction_hash_merkle_root(&txs);
        let header = Header::new_finalized(
            BLOCK_VERSION,
            CompressedParents::default(),
            hash_merkle_root,
            ZERO_HASH64, // PR-9.5e: accepted_id_merkle_root (Hash64)
            ZERO_HASH64, // kaspa-pq (ADR-0004 / design §12): utxo_commitment (Hash64)
            now,
            123456789u32, // bits
            0,            // nonce
            0,            // pow_algo_id
            0,            // daa_score
            0.into(),     // blue_work
            0,            // blue_score
            ZERO_HASH64,  // PR-9.5e: pruning_point (Hash64)
        );
        let mutable_block = MutableBlock::new(header, txs);

        Ok(BlockTemplate::new(
            mutable_block,
            miner_data,
            coinbase.has_red_reward,
            coinbase.miner_script_output_indices,
            now,
            0,
            ZERO_HASH64,
            vec![],
            vec![],
            vec![], // audit v24 H-5: no attestation-template drops in the mock
        )) // PR-9.5e: selected parent is a block hash (Hash64)
    }

    fn palw_readiness_urgency_v1(
        &self,
        carriers: &[kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1],
    ) -> Vec<Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>> {
        let urgency = self.palw_readiness_urgency.read();
        carriers.iter().map(|carrier| urgency.get(&(carrier.bond, carrier.class_id)).copied()).collect()
    }

    fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, _: &TransactionValidationArgs) -> TxResult<()> {
        // If a predefined status was registered to simulate an error, return it right away
        if let Some(status) = self.statuses.read().get(&mutable_tx.id())
            && status.is_err()
        {
            return status.clone();
        }
        let utxos = self.utxos.read();
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            // Keep existing entries
            if mutable_tx.entries[i].is_some() {
                continue;
            }
            // Try add missing entries
            if let Some(entry) = utxos.get(&mutable_tx.tx.inputs[i].previous_outpoint) {
                mutable_tx.entries[i] = Some(entry.clone());
            } else {
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        // At this point we know all UTXO entries are populated, so we can safely calculate the fee
        let total_in: u64 = mutable_tx.entries.iter().map(|x| x.as_ref().unwrap().amount).sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|x| x.value).sum();
        mutable_tx.tx.set_mass(self.calculate_transaction_contextual_masses(mutable_tx).unwrap().storage_mass);

        if mutable_tx.calculated_fee.is_none() {
            let calculated_fee = total_in - total_out;
            mutable_tx.calculated_fee = Some(calculated_fee);
        }
        Ok(())
    }

    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        _: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|x| self.validate_mempool_transaction(x, &Default::default())).collect()
    }

    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|x| self.validate_mempool_transaction(x, &Default::default())).collect()
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> NonContextualMasses {
        let mass = if transaction.is_coinbase() { 0 } else { transaction_estimated_serialized_size(transaction) };
        NonContextualMasses::new(mass, mass)
    }

    fn calculate_transaction_contextual_masses(&self, _transaction: &MutableTransaction) -> Option<ContextualMasses> {
        Some(ContextualMasses::new(0))
    }

    fn get_virtual_daa_score(&self) -> u64 {
        0
    }

    fn get_sink_blue_score(&self) -> u64 {
        *self.sink_blue_score.read()
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.get_virtual_daa_score(), 0.into(), ZERO_HASH64) // PR-9.5e: sink is a block hash (Hash64)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        let coinbase_manager = CoinbaseManagerMock::new();
        Ok(coinbase_manager.modify_coinbase_payload(payload, miner_data))
    }

    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction]) -> Hash64 {
        calc_hash_merkle_root(txs.iter())
    }
}
