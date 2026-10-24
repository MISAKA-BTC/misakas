//! Actual JSON-RPC dispatcher controls; the provider supplies synthetic typed data, not node acceptance evidence.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Provider {
    missing: bool,
    wrong_hash: bool,
    wrong_context: u8,
    reads: AtomicUsize,
}

fn block() -> EthBlock {
    EthBlock {
        number: 7,
        hash: [1; 32],
        parent_hash: [0; 32],
        state_root: [0; 32],
        transactions_root: [0; 32],
        receipts_root: [0; 32],
        logs_bloom: vec![0; 256],
        timestamp: 0,
        gas_used: 42_000,
        gas_limit: 1_000_000,
        base_fee_per_gas: [0; 32],
        miner: [0; 20],
        tx_hashes: vec![[2; 32], [3; 32]],
        size: 2,
    }
}

fn transaction(hash: [u8; 32]) -> EthTx {
    EthTx {
        hash,
        from: [4; 20],
        to: Some([5; 20]),
        nonce: 0,
        value: [0; 32],
        gas: 21_000,
        gas_price: 1,
        max_priority_fee_per_gas: None,
        input: vec![],
        tx_type: 0,
        chain_id: Some(1),
        block_number: Some(7),
        block_hash: Some([1; 32]),
        tx_index: Some(if hash == [2; 32] { 0 } else { 1 }),
        v: 0,
        r: [0; 32],
        s: [0; 32],
        y_parity: false,
        access_list: vec![],
    }
}

#[async_trait]
impl EthProvider for Provider {
    fn chain_id(&self) -> u64 {
        unreachable!()
    }
    fn client_version(&self) -> String {
        unreachable!()
    }
    async fn block_number(&self) -> EthResult<u64> {
        unreachable!()
    }
    async fn is_syncing(&self) -> bool {
        unreachable!()
    }
    async fn gas_price(&self) -> EthResult<u128> {
        unreachable!()
    }
    async fn latest_account(&self, _: [u8; 20]) -> EthResult<Option<EvmAccountSnapshot>> {
        unreachable!()
    }
    async fn eth_call(&self, _: EthCallRequest, _: BlockId) -> EthResult<Vec<u8>> {
        unreachable!()
    }
    async fn estimate_gas(&self, _: EthCallRequest, _: BlockId) -> EthResult<u64> {
        unreachable!()
    }
    async fn send_raw_transaction(&self, _: Vec<u8>) -> EthResult<[u8; 32]> {
        unreachable!()
    }
    async fn transaction_receipt(&self, _: [u8; 32]) -> EthResult<Option<EthReceipt>> {
        unreachable!()
    }
    async fn transaction_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthTx>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.missing && hash == [3; 32] {
            Ok(None)
        } else {
            let mut tx = transaction(if self.wrong_hash { [9; 32] } else { hash });
            match self.wrong_context {
                1 => tx.block_hash = Some([9; 32]),
                2 => tx.block_number = Some(8),
                3 => tx.tx_index = Some(7),
                4 => tx.block_hash = None,
                5 => tx.block_number = None,
                6 => tx.tx_index = None,
                _ => {}
            }
            Ok(Some(tx))
        }
    }
    async fn evm_tx_status(&self, _: [u8; 32]) -> EthResult<EthEvmTxStatus> {
        unreachable!()
    }
    async fn block_by_number(&self, _: u64) -> EthResult<Option<EthBlock>> {
        Ok(Some(block()))
    }
    async fn block_by_tag(&self, _: &str) -> EthResult<Option<EthBlock>> {
        Ok(Some(block()))
    }
    async fn block_by_hash(&self, _: [u8; 32]) -> EthResult<Option<EthBlock>> {
        Ok(Some(block()))
    }
    async fn get_logs(&self, _: u64, _: u64, _: Vec<[u8; 20]>, _: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>> {
        unreachable!()
    }
    async fn fee_history(&self, _: u64, _: u64, _: Vec<f64>) -> EthResult<EthFeeHistory> {
        unreachable!()
    }
}

async fn reply(provider: Arc<Provider>, by_hash: bool, full: bool) -> Value {
    let (method, selector) =
        if by_hash { ("eth_getBlockByHash", json!(format!("0x{}", "01".repeat(32)))) } else { ("eth_getBlockByNumber", json!("0x7")) };
    let erased: Arc<dyn EthProvider> = provider;
    serde_json::to_value(
        handle_one(&erased, json!({"jsonrpc":"2.0", "id":1, "method":method, "params":[selector,full]})).await.unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn missing_full_transaction_is_a_dispatch_error_for_both_block_methods() {
    for by_hash in [false, true] {
        let provider = Arc::new(Provider { missing: true, wrong_hash: false, wrong_context: 0, reads: AtomicUsize::new(0) });
        let answer = reply(provider, by_hash, true).await;
        assert_eq!(answer["error"]["code"], codes::SERVER_ERROR);
        assert!(answer["error"]["message"].as_str().unwrap().contains("unavailable"));
        assert!(answer.get("result").is_none_or(Value::is_null), "a partial full block must never be a result");
    }
}

#[tokio::test]
async fn hashes_only_remains_available_without_any_full_transaction_lookup() {
    for by_hash in [false, true] {
        let provider = Arc::new(Provider { missing: true, wrong_hash: false, wrong_context: 0, reads: AtomicUsize::new(0) });
        let answer = reply(provider.clone(), by_hash, false).await;
        assert!(answer.get("error").is_none_or(Value::is_null));
        assert_eq!(answer["result"]["transactions"], json!([format!("0x{}", "02".repeat(32)), format!("0x{}", "03".repeat(32))]));
        assert_eq!(provider.reads.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn complete_full_blocks_keep_every_transaction_in_the_committed_order() {
    for by_hash in [false, true] {
        let provider = Arc::new(Provider { missing: false, wrong_hash: false, wrong_context: 0, reads: AtomicUsize::new(0) });
        let answer = reply(provider, by_hash, true).await;
        assert!(answer.get("error").is_none_or(Value::is_null));
        let transactions = answer["result"]["transactions"].as_array().unwrap();
        assert_eq!(transactions.len(), 2);
        assert_eq!(transactions[0]["hash"], format!("0x{}", "02".repeat(32)));
        assert_eq!(transactions[1]["hash"], format!("0x{}", "03".repeat(32)));
    }
}

#[tokio::test]
async fn another_transaction_identity_cannot_replace_the_requested_body() {
    let provider = Arc::new(Provider { missing: false, wrong_hash: true, wrong_context: 0, reads: AtomicUsize::new(0) });
    let answer = reply(provider, true, true).await;
    assert_eq!(answer["error"]["code"], codes::SERVER_ERROR);
    assert!(answer["error"]["message"].as_str().unwrap().contains("identity"));
}

#[tokio::test]
async fn another_or_missing_block_context_cannot_replace_the_requested_position() {
    for wrong_context in 1..=6 {
        for by_hash in [false, true] {
            let provider = Arc::new(Provider { missing: false, wrong_hash: false, wrong_context, reads: AtomicUsize::new(0) });
            let answer = reply(provider, by_hash, true).await;
            assert_eq!(answer["error"]["code"], codes::SERVER_ERROR);
            assert!(answer["error"]["message"].as_str().unwrap().contains("context"));
            assert!(answer.get("result").is_none_or(Value::is_null));
        }
    }
}
