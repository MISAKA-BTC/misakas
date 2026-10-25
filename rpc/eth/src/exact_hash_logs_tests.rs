//! EIP-234 actual dispatcher controls. Synthetic provider logs are adapter
//! inputs, not native execution or bridge settlement evidence.
use super::*;
use std::sync::Mutex;
struct Provider {
    reply: EthResult<Option<Vec<EthLogEntry>>>,
    calls: Mutex<Vec<String>>,
}
impl Provider {
    fn new(reply: EthResult<Option<Vec<EthLogEntry>>>) -> Arc<Self> {
        Arc::new(Self { reply, calls: Mutex::new(vec![]) })
    }
}
fn log(hash: [u8; 32]) -> EthLogEntry {
    EthLogEntry {
        address: [7; 20],
        topics: vec![[8; 32]],
        data: vec![9],
        block_number: 42,
        block_hash: hash,
        tx_hash: [4; 32],
        tx_index: 2,
        log_index: 5,
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
        self.calls.lock().unwrap().push("number".into());
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
    async fn block_by_tag(&self, _: &str) -> EthResult<Option<EthBlock>> {
        unreachable!("hash query must not resolve a canonical tag")
    }
    async fn get_logs_by_block_hash(
        &self,
        hash: [u8; 32],
        addresses: Vec<[u8; 20]>,
        topics: Vec<Vec<[u8; 32]>>,
    ) -> EthResult<Option<Vec<EthLogEntry>>> {
        self.calls.lock().unwrap().push(format!("exact:{hash:?}:{addresses:?}:{topics:?}"));
        self.reply.clone()
    }
    async fn get_logs(&self, from: u64, to: u64, _: Vec<[u8; 20]>, _: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>> {
        self.calls.lock().unwrap().push(format!("range:{from}:{to}"));
        Ok(vec![])
    }
    async fn fee_history(&self, _: u64, _: u64, _: Vec<f64>) -> EthResult<EthFeeHistory> {
        unreachable!("hash query must not request fee history")
    }
}
async fn reply(p: Arc<Provider>, filter: Value) -> Value {
    let p: Arc<dyn EthProvider> = p;
    serde_json::to_value(handle_one(&p, json!({"jsonrpc":"2.0","id":1,"method":"eth_getLogs","params":[filter]})).await.unwrap())
        .unwrap()
}
fn hash() -> String {
    format!("0x{}", "03".repeat(32))
}
#[tokio::test]
async fn exact_hash_logs_dispatch_retains_exact_fork_identity_without_number_resolution() {
    let p = Provider::new(Ok(Some(vec![log([3; 32])])));
    let a = reply(
        p.clone(),
        json!({"blockHash":hash(),"address":format!("0x{}","07".repeat(20)),"topics":[format!("0x{}","08".repeat(32))]}),
    )
    .await;
    assert!(a["error"].is_null(), "{a}");
    assert_eq!(a["result"][0]["blockHash"], hash());
    assert_eq!(a["result"][0]["blockNumber"], "0x2a");
    assert_eq!(a["result"][0]["logIndex"], "0x5");
    assert_eq!(*p.calls.lock().unwrap(), [format!("exact:{:?}:{:?}:{:?}", [3u8; 32], vec![[7u8; 20]], vec![vec![[8u8; 32]]])]);
}
#[tokio::test]
async fn exact_hash_logs_dispatch_missing_fault_and_wrong_identity_are_errors_not_empty() {
    for result in [Ok(None), Err(EthRpcError::server("native immutable read fault")), Ok(Some(vec![log([4; 32])]))] {
        let p = Provider::new(result);
        let a = reply(p.clone(), json!({"blockHash":hash()})).await;
        assert_eq!(a["error"]["code"], codes::SERVER_ERROR, "{a}");
        assert!(a.get("result").is_none_or(Value::is_null));
        assert_eq!(p.calls.lock().unwrap().len(), 1);
    }
    let p = Provider::new(Ok(Some(vec![])));
    let a = reply(p.clone(), json!({"blockHash":hash()})).await;
    assert_eq!(a["result"], json!([]));
    assert_eq!(p.calls.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn exact_hash_logs_dispatch_rejects_ambiguous_or_malformed_selectors_before_provider() {
    for filter in [
        json!({"blockHash":hash(),"fromBlock":"0x2a"}),
        json!({"blockHash":hash(),"toBlock":null}),
        json!({"blockHash":null}),
        json!({"blockHash":3}),
        json!({"blockHash":"0x03"}),
    ] {
        let p = Provider::new(Ok(None));
        let a = reply(p.clone(), filter).await;
        assert_eq!(a["error"]["code"], codes::INVALID_PARAMS, "{a}");
        assert!(p.calls.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn exact_hash_logs_dispatch_preserves_numeric_range_path() {
    let p = Provider::new(Ok(None));
    let a = reply(p.clone(), json!({"fromBlock":"0x2a","toBlock":"0x2b"})).await;
    assert_eq!(a["result"], json!([]));
    assert_eq!(*p.calls.lock().unwrap(), ["range:42:43"]);
}
