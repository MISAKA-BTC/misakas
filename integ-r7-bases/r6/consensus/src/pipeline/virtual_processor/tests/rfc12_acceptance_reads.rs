//! Acceptance observations must distinguish a known zero balance from missing
//! history, and read the exact outpoint from the same virtual state stamp.
use super::*;

#[tokio::test]
async fn rfc12_staged_candidate_rows_are_not_committed_reserve_evidence() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let ctx = TestContext::new(TestConsensus::new(&config));
    let positive = BlockHash::from_bytes([0x93; 64]);
    let zero = BlockHash::from_bytes([0x94; 64]);
    let mut pending_batch = rocksdb::WriteBatch::default();
    let vp = ctx.consensus.virtual_processor();
    // These are the real cached stores and staging methods the commit path
    // uses. No validated status or durable batch has been published yet.
    for block in [positive, zero] {
        vp.acceptance_data_store.insert_batch(&mut pending_batch, block, std::sync::Arc::new(vec![])).unwrap();
    }
    vp.reserve_balance_store.insert_batch(&mut pending_batch, positive, 17).unwrap();
    let read = ctx.consensus.get_palw_acceptance_reads_v1(vec![positive, zero], vec![]).unwrap();
    assert_eq!(read.reserve_balances, vec![(positive, None), (zero, None)]);
}

#[tokio::test]
async fn rfc12_reserve_reader_distinguishes_validated_zero_from_unknown_history() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    assert_eq!(ctx.consensus.get_dns_reserve_balance(config.genesis.hash), Some(0));
    let unknown = BlockHash::from_bytes([0x91; 64]);
    assert_eq!(ctx.consensus.get_dns_reserve_balance(unknown), None);

    let first = ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;
    assert_eq!(ctx.consensus.block_status(first.header.hash), BlockStatus::StatusUTXOValid);
    assert_eq!(ctx.consensus.get_dns_reserve_balance(first.header.hash), Some(0));
    assert_eq!(ctx.consensus.get_dns_reserve_balance(unknown), None);
}

#[tokio::test]
async fn rfc12_acceptance_batch_binds_exact_utxos_to_the_virtual_stamp_and_enforces_bounds() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    // The first post-genesis template need not have a payable parent reward.
    ctx.mine_block(new_miner_data(), vec![]).await;
    let first = ctx.mine_block(new_miner_data(), vec![]).await;
    ctx.mine_block(new_miner_data(), vec![]).await;
    let coinbase = &first.transactions[0];
    let (index, output) = coinbase
        .outputs
        .iter()
        .enumerate()
        .find(|(_, output)| output.value > 0)
        .expect("the fixture must contain a real coinbase payment");
    let present = TransactionOutpoint::new(coinbase.id(), index as u32);
    let absent = TransactionOutpoint::new(coinbase.id(), u32::MAX);
    let unknown = BlockHash::from_bytes([0x92; 64]);
    let read = ctx.consensus.get_palw_acceptance_reads_v1(vec![first.header.hash, unknown], vec![present, absent, present]).unwrap();
    assert_eq!(read.sink, ctx.consensus.get_sink());
    assert_eq!(read.virtual_daa, ctx.consensus.get_virtual_daa_score());
    assert_eq!(read.reserve_balances, vec![(first.header.hash, Some(0)), (unknown, None)]);
    assert_eq!(read.outpoints.len(), 3);
    let entry = read.outpoints[0].1.as_ref().expect("accepted coinbase output is unspent");
    assert_eq!(read.outpoints[0].0, present);
    assert_eq!(entry.amount, output.value);
    assert_eq!(entry.script_public_key, output.script_public_key);
    assert!(entry.is_coinbase);
    assert_eq!(read.outpoints[1], (absent, None));
    assert_eq!(read.outpoints[2], read.outpoints[0]);
    assert!(ctx.consensus.get_palw_acceptance_reads_v1(vec![unknown; 65], vec![]).is_err());
    assert!(ctx.consensus.get_palw_acceptance_reads_v1(vec![], vec![absent; 65]).is_err());
}
