//! Actual JSON-RPC dispatch against synthetic typed provider heads. This is an
//! adapter contract test, not native EVM execution or certification evidence.
use super::*;
use std::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Latest,
    Tag(String),
    Logs(u64, u64),
    Fees(u64),
}
struct Provider {
    safe: Option<u64>,
    finalized: Option<u64>,
    read_fault: bool,
    calls: Mutex<Vec<Call>>,
}
impl Provider {
    fn new() -> Arc<Self> {
        Arc::new(Self { safe: Some(7), finalized: Some(3), read_fault: false, calls: Mutex::new(Vec::new()) })
    }
}
fn block(number: u64) -> EthBlock {
    EthBlock {
        number,
        hash: [1; 32],
        parent_hash: [0; 32],
        state_root: [0; 32],
        transactions_root: [0; 32],
        receipts_root: [0; 32],
        logs_bloom: vec![0; 256],
        timestamp: 0,
        gas_used: 0,
        gas_limit: 1_000_000,
        base_fee_per_gas: [0; 32],
        miner: [0; 20],
        tx_hashes: vec![],
        size: 0,
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
        self.calls.lock().unwrap().push(Call::Latest);
        Ok(99)
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
    async fn transaction_by_hash(&self, _: [u8; 32]) -> EthResult<Option<EthTx>> {
        unreachable!()
    }
    async fn evm_tx_status(&self, _: [u8; 32]) -> EthResult<EthEvmTxStatus> {
        unreachable!()
    }
    async fn block_by_number(&self, _: u64) -> EthResult<Option<EthBlock>> {
        unreachable!()
    }
    async fn block_by_hash(&self, _: [u8; 32]) -> EthResult<Option<EthBlock>> {
        unreachable!()
    }
    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>> {
        self.calls.lock().unwrap().push(Call::Tag(tag.into()));
        if self.read_fault {
            return Err(EthRpcError::server("canonical head store read fault"));
        }
        Ok(match tag {
            "safe" => self.safe,
            "finalized" => self.finalized,
            _ => unreachable!(),
        }
        .map(block))
    }
    async fn get_logs(&self, from: u64, to: u64, _: Vec<[u8; 20]>, _: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>> {
        self.calls.lock().unwrap().push(Call::Logs(from, to));
        Ok(vec![])
    }
    async fn fee_history(&self, _: u64, newest: u64, _: Vec<f64>) -> EthResult<EthFeeHistory> {
        self.calls.lock().unwrap().push(Call::Fees(newest));
        Ok(EthFeeHistory { oldest_block: newest, base_fee_per_gas: vec![[0; 32]; 2], gas_used_ratio: vec![0.0], reward: None })
    }
}
async fn reply(provider: Arc<Provider>, method: &str, params: Value) -> Value {
    let provider: Arc<dyn EthProvider> = provider;
    serde_json::to_value(handle_one(&provider, json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params})).await.unwrap())
        .unwrap()
}
fn success(answer: &Value) {
    assert!(answer.get("error").is_none_or(Value::is_null), "{answer}");
}
fn request(which: usize, selector: Value) -> (&'static str, Value) {
    match which {
        0 => ("eth_getLogs", json!([{"fromBlock":selector,"toBlock":"0x40"}])),
        1 => ("eth_getLogs", json!([{"fromBlock":"0x0","toBlock":selector}])),
        2 => ("eth_feeHistory", json!(["0x1", selector])),
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn certified_range_both_log_endpoints_and_fee_history_use_exact_provider_heads() {
    for (tag, number) in [("safe", 7), ("finalized", 3)] {
        for which in 0..3 {
            let provider = Provider::new();
            let (method, params) = request(which, json!(tag));
            let answer = reply(provider.clone(), method, params).await;
            success(&answer);
            let downstream = match which {
                0 => Call::Logs(number, 64),
                1 => Call::Logs(0, number),
                _ => Call::Fees(number),
            };
            assert_eq!(*provider.calls.lock().unwrap(), vec![Call::Tag(tag.into()), downstream]);
            if which == 2 {
                assert_eq!(answer["result"]["oldestBlock"], quantity(number as u128));
            }
        }
    }
    let provider = Provider::new();
    success(&reply(provider.clone(), "eth_getLogs", json!([{"fromBlock":"finalized","toBlock":"safe"}])).await);
    assert_eq!(*provider.calls.lock().unwrap(), vec![Call::Tag("finalized".into()), Call::Tag("safe".into()), Call::Logs(3, 7)]);
}

#[tokio::test]
async fn certified_range_missing_or_unreadable_heads_never_use_latest_zero_or_downstream() {
    for tag in ["safe", "finalized"] {
        for which in 0..3 {
            for read_fault in [false, true] {
                let provider = Arc::new(Provider { safe: None, finalized: None, read_fault, calls: Mutex::new(Vec::new()) });
                let (method, params) = request(which, json!(tag));
                let answer = reply(provider.clone(), method, params).await;
                assert_eq!(answer["error"]["code"], codes::SERVER_ERROR);
                assert!(answer["error"]["message"].as_str().unwrap().contains(if read_fault {
                    "canonical head store read fault"
                } else {
                    "head unavailable"
                }));
                assert_eq!(*provider.calls.lock().unwrap(), vec![Call::Tag(tag.into())]);
            }
        }
    }
}

#[tokio::test]
async fn certified_range_preserves_defaults_pending_earliest_and_hex_numbers() {
    for which in 0..3 {
        for (selector, number, latest) in [("latest", 99, true), ("pending", 99, true), ("earliest", 0, false), ("0x2a", 42, false)] {
            let provider = Provider::new();
            let (method, params) = request(which, json!(selector));
            success(&reply(provider.clone(), method, params).await);
            let mut expected = if latest { vec![Call::Latest] } else { vec![] };
            expected.push(match which {
                0 => Call::Logs(number, 64),
                1 => Call::Logs(0, number),
                _ => Call::Fees(number),
            });
            assert_eq!(*provider.calls.lock().unwrap(), expected);
        }
    }
    let provider = Provider::new();
    success(&reply(provider.clone(), "eth_getLogs", json!([{}])).await);
    assert_eq!(*provider.calls.lock().unwrap(), vec![Call::Latest, Call::Latest, Call::Logs(99, 99)]);
    let provider = Provider::new();
    success(&reply(provider.clone(), "eth_feeHistory", json!(["0x1"])).await);
    assert_eq!(*provider.calls.lock().unwrap(), vec![Call::Latest, Call::Fees(99)]);
}

#[tokio::test]
async fn certified_range_explicit_wrong_types_are_not_default_latest() {
    for which in 0..3 {
        for selector in [Value::Null, json!(7), json!(true), json!([]), json!({}), json!("unknown")] {
            let provider = Provider::new();
            let (method, params) = request(which, selector);
            let answer = reply(provider.clone(), method, params).await;
            assert_eq!(answer["error"]["code"], codes::INVALID_PARAMS);
            assert!(provider.calls.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn certified_range_real_zero_number_is_distinct_from_an_absent_head() {
    for tag in ["safe", "finalized"] {
        let provider = Arc::new(Provider { safe: Some(0), finalized: Some(0), read_fault: false, calls: Mutex::new(Vec::new()) });
        success(&reply(provider.clone(), "eth_getLogs", json!([{"fromBlock":tag,"toBlock":tag}])).await);
        success(&reply(provider.clone(), "eth_feeHistory", json!(["0x1", tag])).await);
        assert_eq!(
            *provider.calls.lock().unwrap(),
            vec![Call::Tag(tag.into()), Call::Tag(tag.into()), Call::Logs(0, 0), Call::Tag(tag.into()), Call::Fees(0)]
        );
    }
}
