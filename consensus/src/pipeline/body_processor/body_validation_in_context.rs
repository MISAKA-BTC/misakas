use super::BlockBodyProcessor;
use crate::{
    errors::{BlockProcessResult, RuleError},
    model::stores::statuses::StatusesStoreReader,
    processes::{
        transaction_validator::{
            TransactionValidator,
            tx_validation_in_header_context::{LockTimeArg, LockTimeType},
        },
        window::WindowManager,
    },
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::block::Block;
use kaspa_database::prelude::StoreResultExt;
use kaspa_txscript::script_class::ScriptClass;
use once_cell::unsync::Lazy;
use std::sync::Arc;

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context(block)
    }

    fn check_block_transactions_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // Use lazy evaluation to avoid unnecessary work, as most of the time we expect the txs not to have lock time.
        let lazy_pmt_res = Lazy::new(|| self.window_manager.calc_past_median_time_for_known_hash(block.hash()));

        for tx in block.transactions.iter() {
            let lock_time_arg = match TransactionValidator::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(block.header.daa_score),
                // We only evaluate the pmt calculation when actually needed
                LockTimeType::Time => LockTimeArg::MedianTime((*lazy_pmt_res).clone()?),
            };
            if let Err(e) = self.transaction_validator.validate_tx_in_header_context(tx, lock_time_arg) {
                return Err(RuleError::TxInContextFailed(tx.id(), e));
            };
        }
        Ok(())
    }

    fn check_parent_bodies_exist(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let statuses_read_guard = self.statuses_store.read();
        let missing: Vec<BlockHash> = block
            .header
            .direct_parents()
            .iter()
            .copied()
            .filter(|parent| {
                let status_option = statuses_read_guard.get(*parent).optional().unwrap();
                status_option.is_none_or(|s| !s.has_block_body())
            })
            .collect();
        if !missing.is_empty() {
            return Err(RuleError::MissingParents(missing));
        }

        Ok(())
    }

    fn check_coinbase_blue_score_and_subsidy(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        match self.coinbase_manager.deserialize_coinbase_payload(&block.transactions[0].payload) {
            Ok(data) => {
                if data.blue_score != block.header.blue_score {
                    return Err(RuleError::BadCoinbasePayloadBlueScore(data.blue_score, block.header.blue_score));
                }

                let expected_subsidy = self.coinbase_manager.calc_block_subsidy(block.header.daa_score);

                if data.subsidy != expected_subsidy {
                    return Err(RuleError::WrongSubsidy(expected_subsidy, data.subsidy));
                }

                // kaspa-pq PQ-only invariant: the coinbase payload's miner script must itself be
                // ML-DSA P2PKH. The block's own coinbase OUTPUTS are PQ-class-checked in isolation,
                // but the payload miner script is a SEPARATE field that descendant blocks read into
                // their reward fan-out (`expected_coinbase_transaction` pays this block's merged
                // reward to `reward_data.script_public_key` = this miner script). A non-PQ script
                // here would force every descendant's coinbase to carry a non-PQ output, which the
                // PQ output-class rule rejects — a reward-path / liveness poison rather than a
                // mintable non-PQ UTXO. Reject it at the source. Gated by the PQ script policy,
                // active from genesis on every kaspa-pq network (`pq_activation_daa_score = 0`).
                if self.transaction_validator.resolved_script_policy(block.header.daa_score).pq_only
                    && !ScriptClass::from_script(&data.miner_data.script_public_key).is_pq_standard()
                {
                    return Err(RuleError::NonPqCoinbasePayloadScript);
                }

                Ok(())
            }
            Err(e) => Err(RuleError::BadCoinbasePayload(e)),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        config::ConfigBuilder,
        consensus::test_consensus::TestConsensus,
        constants::TX_VERSION,
        errors::RuleError,
        model::stores::ghostdag::GhostdagStoreReader,
        processes::{transaction_validator::errors::TxRuleError, window::WindowManager},
    };
    use kaspa_consensus_core::{
        BlockHash, // PR-9.5e: block ids are Hash64
        api::ConsensusApi,
        coinbase::MinerData,
        config::params::MAINNET_PARAMS,
        dns_finality::p2pkh_mldsa87_spk,
        merkle::calc_hash_merkle_root,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_core::assert_match;

    #[tokio::test]
    async fn validate_body_in_context_test() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| p.deflationary_phase_daa_score = p.genesis.daa_score + 2)
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        consensus.add_header_only_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();

        {
            let block = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
            // We expect a missing parents error since the parent is header only.
            assert_match!(body_processor.validate_body_in_context(&block.to_immutable()), Err(RuleError::MissingParents(_)));
        }

        let valid_block = consensus.build_block_with_parents_and_transactions(3.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(valid_block.to_immutable()).virtual_state_task.await.unwrap();
        {
            let mut block = consensus.build_block_with_parents_and_transactions(2.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            // kaspa-pq: pre-deflationary subsidy == year-1 per-block subsidy at 10 BPS (table[0].div_ceil(10)).
            assert_match!(
                consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 370468345);

            // The second time we send an invalid block we expect it to be a known invalid.
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::KnownInvalid)
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(4.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[0..8].copy_from_slice(&(100_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayloadBlueScore(_, _))
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(5.into(), vec![3.into()], vec![]);
            block.transactions[0].payload = vec![];
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayload(_))
            );
        }

        let valid_block_child = consensus.build_block_with_parents_and_transactions(6.into(), vec![3.into()], vec![]);
        consensus.validate_and_insert_block(valid_block_child.clone().to_immutable()).virtual_state_task.await.unwrap();
        {
            // The block DAA score is 2 (>= deflationary_phase_daa_score), so the subsidy comes from
            // the decay table: month 0 => table[0].div_ceil(10) = 370_468_345 sompi.
            let mut block = consensus.build_block_with_parents_and_transactions(7.into(), vec![6.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
            assert_match!(consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 370468345);
        }

        {
            // Check that the same daa score as the block's daa score or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 8.into(), tip_daa_score + 1, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 9.into(), tip_daa_score, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 10.into(), tip_daa_score - 1, 0, true).await;

            let valid_block_child_gd = consensus.ghostdag_store().get_data(valid_block_child.header.hash).unwrap();
            let (valid_block_child_gd_pmt, _) = consensus.window_manager().calc_past_median_time(&valid_block_child_gd).unwrap();
            let past_median_time = valid_block_child_gd_pmt + 1;

            // Check that the same past median time as the block's or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 11.into(), past_median_time + 1, 0, false)
                .await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 12.into(), past_median_time, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 13.into(), past_median_time - 1, 0, true)
                .await;

            // We check that if the transaction is marked as finalized it'll pass for any lock time.
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                14.into(),
                past_median_time + 1,
                u64::MAX,
                true,
            )
            .await;

            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 15.into(), tip_daa_score + 1, u64::MAX, true)
                .await;
        }

        consensus.shutdown(wait_handles);
    }

    /// kaspa-pq PQ-only invariant (audit Finding A): the coinbase **payload** miner script must
    /// be ML-DSA P2PKH, independently of the coinbase outputs. A non-PQ payload miner script
    /// would flow into descendant blocks' reward fan-out and force a non-PQ reward output (which
    /// the PQ output-class rule rejects) — a reward-path / liveness poison. Verifies the source
    /// block is rejected at body validation even though its coinbase carries no outputs (so the
    /// isolation output-class check cannot catch it).
    #[tokio::test]
    async fn coinbase_payload_miner_script_must_be_pq() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        // Control: an ML-DSA P2PKH miner script in the coinbase payload passes body validation.
        let good = consensus.build_utxo_valid_block_with_parents(
            1.into(),
            vec![config.genesis.hash],
            MinerData::new(p2pkh_mldsa87_spk(&[0x07; 64]), vec![]),
            vec![],
        );
        body_processor.validate_body_in_context(&good.to_immutable()).unwrap();

        // Finding A: a non-PQ miner script (here a bare OP_TRUE) in the payload is rejected,
        // although the genesis-child coinbase has no outputs for the output-class rule to flag.
        let non_pq_spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let bad = consensus.build_utxo_valid_block_with_parents(
            2.into(),
            vec![config.genesis.hash],
            MinerData::new(non_pq_spk, vec![]),
            vec![],
        );
        assert_match!(body_processor.validate_body_in_context(&bad.to_immutable()), Err(RuleError::NonPqCoinbasePayloadScript));

        consensus.shutdown(wait_handles);
    }

    async fn check_for_lock_time_and_sequence(
        consensus: &TestConsensus,
        parent: BlockHash,
        block_hash: BlockHash,
        lock_time: u64,
        sequence: u64,
        should_pass: bool,
    ) {
        // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
        let block = consensus.build_block_with_parents_and_transactions(
            block_hash,
            vec![parent],
            vec![Transaction::new(
                TX_VERSION,
                vec![TransactionInput::new(TransactionOutpoint::new(1.into(), 0), vec![], sequence, 0)],
                vec![],
                lock_time,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            )],
        );

        if should_pass {
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
        } else {
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::TxInContextFailed(_, e)) if matches!(e, TxRuleError::NotFinalized(_)));
        }
    }
}
