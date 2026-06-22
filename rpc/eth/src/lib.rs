//! Ethereum JSON-RPC compatibility adapter for the kaspa-pq EVM lane
//! (ADR-0020 §16). A thin HTTP JSON-RPC 2.0 front-end that translates the
//! standard `eth_*` / `net_*` / `web3_*` methods onto the node-side
//! [`EthProvider`] trait, so unmodified Ethereum tooling (Foundry, Hardhat,
//! ethers, viem, MetaMask) can talk to a MISAKA node.
//!
//! This crate is deliberately dependency-light: it links NO revm/secp. All
//! consensus reads and the read-only revm simulation behind `eth_call` /
//! `eth_estimateGas` live in the node-side `EthProvider` implementation, which
//! kaspad compiles only under its `evm` feature.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use kaspa_consensus_core::evm::{EvmAccountSnapshot, EvmU256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// JSON-RPC 2.0 error codes used by the adapter (the standard subset).
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Generic server error (Ethereum convention reserves -32000..=-32099).
    pub const SERVER_ERROR: i64 = -32000;
}

/// An error surfaced through the JSON-RPC `error` member.
#[derive(Debug, Clone)]
pub struct EthRpcError {
    pub code: i64,
    pub message: String,
}

impl EthRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }
    pub fn server(message: impl Into<String>) -> Self {
        Self::new(codes::SERVER_ERROR, message)
    }
}

impl std::fmt::Display for EthRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
impl std::error::Error for EthRpcError {}

pub type EthResult<T> = Result<T, EthRpcError>;

/// Format a non-negative integer as an Ethereum JSON-RPC QUANTITY (minimal hex,
/// no leading zeros, `0x0` for zero).
pub fn quantity(n: u128) -> Value {
    json!(format!("0x{n:x}"))
}

/// A parsed `eth_call` / `eth_estimateGas` request (primitive types, so this
/// crate stays free of the EVM executor types — the node-side impl converts it).
#[derive(Clone, Debug, Default)]
pub struct EthCallRequest {
    pub from: [u8; 20],
    /// `None` ⇒ contract creation.
    pub to: Option<[u8; 20]>,
    /// Call value in wei, big-endian 32 bytes.
    pub value: [u8; 32],
    pub data: Vec<u8>,
    /// Gas limit; `0` ⇒ unspecified (use the block limit).
    pub gas: u64,
}

/// A decoded EVM transaction for `eth_getTransactionByHash`. Block context is
/// `None` when the tx is known but not yet on the selected chain (pending).
#[derive(Clone, Debug)]
pub struct EthTx {
    pub hash: [u8; 32],
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub value: [u8; 32],
    pub gas: u64,
    pub gas_price: u128,
    pub max_priority_fee_per_gas: Option<u128>,
    pub input: Vec<u8>,
    pub tx_type: u8,
    pub chain_id: Option<u64>,
    pub block_number: Option<u64>,
    pub block_hash: Option<[u8; 32]>,
    pub tx_index: Option<u32>,
}

/// One log entry of an [`EthReceipt`] (the node-side impl fills it from the
/// committed EVM receipt; the adapter renders the standard JSON shape).
#[derive(Clone, Debug)]
pub struct EthLog {
    pub address: [u8; 20],
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

/// A mined EVM transaction's receipt (`eth_getTransactionReceipt`). Primitive
/// fields only, so this crate stays free of the consensus receipt types — the
/// node-side impl maps its `EvmTxReceiptView` onto this.
#[derive(Clone, Debug)]
pub struct EthReceipt {
    pub tx_hash: [u8; 32],
    /// `true` ⇒ status `0x1`; `false` ⇒ `0x0` (reverted/failed, still mined).
    pub status: bool,
    pub block_number: u64,
    /// A 32-byte block identifier (the accepting L1 block hash truncated to 32).
    pub block_hash: [u8; 32],
    pub tx_index: u32,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    /// Block-global index of this receipt's first log (audit H-05): each log's
    /// `logIndex` is rendered as `log_index_offset + i` so it matches eth_getLogs.
    pub log_index_offset: u32,
    /// The standard EIP-234 logs bloom over this receipt's logs (audit H-05 —
    /// was a zero constant).
    pub logs_bloom: [u8; 256],
    pub logs: Vec<EthLog>,
    /// Sender / recipient, recovered/decoded from the raw tx (`None` if the raw
    /// tx could not be located — degrades to a base receipt).
    pub from: Option<[u8; 20]>,
    pub to: Option<[u8; 20]>,
    /// `CREATE(from, nonce)` for a contract-creation tx (what `forge create` reads).
    pub contract_address: Option<[u8; 20]>,
    /// 0 = legacy, 1 = EIP-2930, 2 = EIP-1559.
    pub tx_type: u8,
    /// Gas price paid (wei); the adapter renders it as `effectiveGasPrice`.
    pub effective_gas_price: u128,
}

/// An EVM block for `eth_getBlockByNumber` / `eth_getBlockByHash`. Primitive
/// fields only; the node-side impl maps its consensus `EvmBlockResponse` here
/// and the adapter renders the standard Ethereum block JSON. `tx_hashes` are the
/// accepted txs in order (the adapter returns hashes; full-tx objects are a later
/// increment alongside `eth_getTransactionByHash`).
#[derive(Clone, Debug)]
pub struct EthBlock {
    pub number: u64,
    /// 32-byte block id (the accepting L1 hash truncated to 32 — the same id
    /// `eth_getTransactionReceipt` returns as `blockHash`).
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub transactions_root: [u8; 32],
    pub receipts_root: [u8; 32],
    /// 256-byte logs bloom.
    pub logs_bloom: Vec<u8>,
    pub timestamp: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    /// EIP-1559 base fee, big-endian 32 bytes.
    pub base_fee_per_gas: [u8; 32],
    pub miner: [u8; 20],
    pub tx_hashes: Vec<[u8; 32]>,
}

/// One resolved log for `eth_getLogs`. Primitive fields; the node-side impl maps
/// its consensus `EvmLogEntry` here and the adapter renders the standard JSON.
#[derive(Clone, Debug)]
pub struct EthLogEntry {
    pub address: [u8; 20],
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub tx_hash: [u8; 32],
    pub tx_index: u32,
    pub log_index: u32,
}

/// `eth_feeHistory` result. `base_fee_per_gas` has `block_count + 1` entries (the
/// trailing one is the next block's projected base fee); `gas_used_ratio` has
/// `block_count`; `reward` (if percentiles were requested) is `block_count` rows.
#[derive(Clone, Debug)]
pub struct EthFeeHistory {
    pub oldest_block: u64,
    pub base_fee_per_gas: Vec<[u8; 32]>,
    pub gas_used_ratio: Vec<f64>,
    pub reward: Option<Vec<Vec<[u8; 32]>>>,
}

/// The node-side data + action surface the adapter needs. Implemented by kaspad
/// over its `ConsensusManager` + `FlowContext` (and, for simulation, kaspa-evm).
/// Methods are added here as the MVP grows (state / call / receipt / block).
#[async_trait]
pub trait EthProvider: Send + Sync + 'static {
    /// The EVM chain id (`EVM_CHAIN_ID`).
    fn chain_id(&self) -> u64;

    /// `web3_clientVersion` string (e.g. "misaka/kaspad/v1.1.0").
    fn client_version(&self) -> String;

    /// The current canonical EVM head block number (`eth_blockNumber`).
    async fn block_number(&self) -> EthResult<u64>;

    /// Whether the node is still syncing (true ⇒ `eth_syncing` reports progress).
    async fn is_syncing(&self) -> bool;

    /// Suggested gas price in wei (`eth_gasPrice`) — the head base fee.
    async fn gas_price(&self) -> EthResult<u128>;

    /// The account state at the canonical EVM head (the "latest" tag). `None` =
    /// the account does not exist (⇒ zero balance/nonce, empty code/storage).
    /// MVP serves the head snapshot; historical block tags land with the block
    /// index (Increment 6).
    async fn latest_account(&self, address: [u8; 20]) -> EthResult<Option<EvmAccountSnapshot>>;

    /// Account state at a specific block selector (audit H-04): honors
    /// `latest`/`pending` (sink), `safe`/`finalized` (canonical heads — a
    /// non-reorgable read), `earliest`, and a numeric block. The default ignores
    /// the selector and serves latest (providers without historical state);
    /// kaspad resolves the selector and fails closed if that block's snapshot is
    /// unavailable (pruned / pre-activation).
    async fn account_at(&self, address: [u8; 20], _block: BlockId) -> EthResult<Option<EvmAccountSnapshot>> {
        self.latest_account(address).await
    }

    /// The "pending" nonce for `eth_getTransactionCount(…,"pending")` (audit M-08):
    /// the chain nonce plus this node's contiguous pending EVM txs for the account,
    /// so back-to-back wallet sends increment instead of colliding. Default = the
    /// latest (accepted) nonce, for providers without a mempool overlay.
    async fn pending_nonce(&self, address: [u8; 20]) -> EthResult<u64> {
        Ok(self.latest_account(address).await?.map(|a| a.nonce).unwrap_or(0))
    }

    /// `eth_call`: read-only execution at the canonical head; returns the call's
    /// output bytes (revert data on a revert, surfaced as an error by the caller).
    async fn eth_call(&self, req: EthCallRequest) -> EthResult<Vec<u8>>;

    /// `eth_estimateGas`: the minimal gas limit that lets the call succeed.
    async fn estimate_gas(&self, req: EthCallRequest) -> EthResult<u64>;

    /// `eth_sendRawTransaction`: admit a signed raw EIP-2718 transaction into the
    /// EVM mempool. Returns the Ethereum tx hash (keccak256 of the raw bytes).
    async fn send_raw_transaction(&self, raw: Vec<u8>) -> EthResult<[u8; 32]>;

    /// `eth_getTransactionReceipt`: the receipt of a mined EVM tx, or `None` if
    /// it is unknown / still pending (not yet accepted on the selected chain).
    async fn transaction_receipt(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthReceipt>>;

    /// `eth_getTransactionByHash`: the decoded tx (with block context if mined),
    /// or `None` if the raw tx is unknown to this node.
    async fn transaction_by_hash(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthTx>>;

    /// `misaka_getEvmTxStatus`: the full EVM-lane lifecycle of a tx by hash. Beyond
    /// `eth_getTransactionReceipt`'s accepted-or-null, this distinguishes `pending`
    /// (still in the EVM mempool), `included` (in a payload block but acceptance
    /// pending under mergeset delayed acceptance), `accepted` (mined on the selected
    /// chain), `skipped` (last seen deterministically skipped — class 2/3/5,
    /// re-includable), or `unknown` — the visibility a bare receipt cannot give.
    async fn evm_tx_status(&self, tx_hash: [u8; 32]) -> EthResult<EthEvmTxStatus>;

    /// `eth_getBlockByNumber` for a numeric block (canonical EVM block at that number).
    async fn block_by_number(&self, number: u64) -> EthResult<Option<EthBlock>>;

    /// `eth_getBlockByNumber` for a tag: `latest`/`pending` (the sink), `safe`,
    /// `finalized`, or `earliest`. Unknown tags ⇒ `invalid_params` by the caller.
    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>>;

    /// `eth_getBlockByHash` for a 32-byte eth-rpc block id.
    async fn block_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthBlock>>;

    /// `eth_getLogs`: canonical logs over the `evm_number` range `[from, to]`,
    /// filtered by `addresses` (empty = any) and per-position `topics` (an empty
    /// inner vec = wildcard). The caller bounds the block range.
    async fn get_logs(
        &self,
        from: u64,
        to: u64,
        addresses: Vec<[u8; 20]>,
        topics: Vec<Vec<[u8; 32]>>,
    ) -> EthResult<Vec<EthLogEntry>>;

    /// `eth_feeHistory`: base fees + gas-used ratios over the last `block_count`
    /// blocks ending at `newest` (used by EIP-1559 tooling — Foundry/ethers/MetaMask).
    async fn fee_history(&self, block_count: u64, newest: u64, reward_percentiles: Vec<f64>) -> EthResult<EthFeeHistory>;
}

/// The EVM-lane lifecycle of a tx (`misaka_getEvmTxStatus`). `state` is a best-effort
/// summary derived from the fields below: `accepted` (mined) ▸ `pending` (in the
/// mempool, will be retried) ▸ `included` (in a payload, acceptance pending) ▸
/// `skipped` (last seen skipped, no longer pending) ▸ `unknown`.
#[derive(Debug, Clone)]
pub struct EthEvmTxStatus {
    pub tx_hash: [u8; 32],
    pub state: &'static str,
    /// Payload (DAG) blocks whose payload carries this tx, as eth-rpc 32-byte ids.
    pub included_in: Vec<[u8; 32]>,
    /// The CANONICAL (selected-chain) accepting block (eth-rpc 32-byte id) + the
    /// receipt index, resolved via the canonical receipt lookup — `Some` ONLY when
    /// the tx is accepted on the current selected chain. `state == "accepted"`
    /// tracks this field, so a side-branch (orphaned) acceptance is never reported
    /// as accepted (audit H-06: off-chain key release / sale finalization must not
    /// trust a reorg-able acceptance).
    pub accepted_in: Option<([u8; 32], u32)>,
    /// Non-canonical acceptances seen in the location index (side branches not on
    /// the selected chain). Diagnostic only — never drives `accepted`.
    pub orphaned_acceptances: Vec<([u8; 32], u32)>,
    /// The most recent §6.1 skip class (2 = acceptance-invalid, 3 = duplicate, 5 = over-cap),
    /// set while the tx has been included but not yet accepted.
    pub last_skip_class: Option<u8>,
    /// Whether the tx is currently in this node's EVM mempool (active, re-includable).
    pub in_mempool: bool,
    /// Decoded metadata (from the pooled raw tx, when pending).
    pub sender: Option<[u8; 20]>,
    pub nonce: Option<u64>,
    pub gas_limit: Option<u64>,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 envelope
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    id: Value,
}

#[derive(Serialize)]
struct RpcErrorObj {
    code: i64,
    message: String,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcErrorObj>,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    fn err(id: Value, e: EthRpcError) -> Self {
        Self { jsonrpc: "2.0", id, result: None, error: Some(RpcErrorObj { code: e.code, message: e.message }) }
    }
}

/// Dispatch a single decoded JSON-RPC request to the provider.
async fn dispatch(provider: &Arc<dyn EthProvider>, req: RpcRequest) -> RpcResponse {
    let id = req.id.clone();
    let result: EthResult<Value> = match req.method.as_str() {
        "web3_clientVersion" => Ok(json!(provider.client_version())),
        "web3_sha3" => web3_sha3(&req.params),
        // net_version is the chain id as a DECIMAL string (Ethereum convention).
        "net_version" => Ok(json!(provider.chain_id().to_string())),
        "net_listening" => Ok(json!(true)),
        "net_peerCount" => Ok(quantity(0)),
        "eth_chainId" => Ok(quantity(provider.chain_id() as u128)),
        "eth_blockNumber" => provider.block_number().await.map(|n| quantity(n as u128)),
        "eth_syncing" => Ok(json!(provider.is_syncing().await)),
        "eth_gasPrice" => provider.gas_price().await.map(quantity),
        // The lane has no separate priority-fee market yet; report 0.
        "eth_maxPriorityFeePerGas" => Ok(quantity(0)),
        "eth_accounts" => Ok(json!([] as [Value; 0])),
        "eth_getBalance" => eth_get_balance(provider, &req.params).await,
        "eth_getTransactionCount" => eth_get_transaction_count(provider, &req.params).await,
        "eth_getCode" => eth_get_code(provider, &req.params).await,
        "eth_getStorageAt" => eth_get_storage_at(provider, &req.params).await,
        "eth_call" => eth_call_handler(provider, &req.params).await,
        "eth_estimateGas" => eth_estimate_gas_handler(provider, &req.params).await,
        "eth_sendRawTransaction" => eth_send_raw_transaction(provider, &req.params).await,
        "eth_getTransactionReceipt" => eth_get_transaction_receipt(provider, &req.params).await,
        "misaka_getEvmTxStatus" => misaka_get_evm_tx_status(provider, &req.params).await,
        "eth_getBlockByNumber" => eth_get_block_by_number(provider, &req.params).await,
        "eth_getBlockByHash" => eth_get_block_by_hash(provider, &req.params).await,
        "eth_getBlockTransactionCountByNumber" => eth_get_block_tx_count_by_number(provider, &req.params).await,
        "eth_getBlockTransactionCountByHash" => eth_get_block_tx_count_by_hash(provider, &req.params).await,
        "eth_getLogs" => eth_get_logs(provider, &req.params).await,
        "eth_feeHistory" => eth_fee_history(provider, &req.params).await,
        "eth_getTransactionByHash" => eth_get_transaction_by_hash(provider, &req.params).await,
        other => Err(EthRpcError::new(codes::METHOD_NOT_FOUND, format!("the method {other} does not exist / is not available"))),
    };
    match result {
        Ok(v) => RpcResponse::ok(id, v),
        Err(e) => RpcResponse::err(id, e),
    }
}

/// `web3_sha3`: keccak256 of the hex-encoded input data.
fn web3_sha3(params: &Value) -> EthResult<Value> {
    let hex = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params("web3_sha3 expects [data]"))?;
    let bytes = decode_hex(hex)?;
    let digest = alloy_primitives::keccak256(&bytes);
    Ok(json!(format!("0x{}", faster_hex::hex_string(digest.as_slice()))))
}

// --- eth_* state queries (Increment 3) — honor the block selector (audit H-04) ---

/// Resolve the account at the optional block param `idx` (default: latest).
async fn account_at_param(
    provider: &Arc<dyn EthProvider>,
    addr: [u8; 20],
    params: &Value,
    idx: usize,
) -> EthResult<Option<EvmAccountSnapshot>> {
    match params.as_array().and_then(|a| a.get(idx)) {
        Some(_) => provider.account_at(addr, parse_block_param(params, idx)?).await,
        None => provider.latest_account(addr).await,
    }
}

async fn eth_get_balance(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    Ok(account_at_param(provider, addr, params, 1).await?.map(|a| quantity_from_be32(&a.balance.to_be_bytes())).unwrap_or_else(|| json!("0x0")))
}

async fn eth_get_transaction_count(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    // "pending" includes this node's mempool overlay (M-08); other tags resolve
    // historical/finalized state (H-04); absent ⇒ latest.
    let nonce = match params.get(1).and_then(|v| v.as_str()) {
        Some("pending") => provider.pending_nonce(addr).await?,
        _ => account_at_param(provider, addr, params, 1).await?.map(|a| a.nonce).unwrap_or(0),
    };
    Ok(quantity(nonce as u128))
}

async fn eth_get_code(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    let code = account_at_param(provider, addr, params, 1).await?.map(|a| a.code).unwrap_or_default();
    Ok(json!(format!("0x{}", faster_hex::hex_string(&code))))
}

async fn eth_get_storage_at(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    let slot = parse_slot_param(params, 1)?;
    // The block selector for storageAt is param #2.
    let value = account_at_param(provider, addr, params, 2).await?.and_then(|a| a.storage.into_iter().find(|(k, _)| *k == slot).map(|(_, v)| v));
    // getStorageAt returns a full 32-byte DATA value (zero-padded).
    let bytes = value.map(|v| v.to_be_bytes()).unwrap_or([0u8; 32]);
    Ok(json!(format!("0x{}", faster_hex::hex_string(&bytes))))
}

/// Format 32 big-endian bytes as an Ethereum JSON-RPC QUANTITY (minimal hex,
/// no leading zeros, `0x0` for zero).
fn quantity_from_be32(bytes: &[u8; 32]) -> Value {
    match bytes.iter().position(|&b| b != 0) {
        None => json!("0x0"),
        Some(i) => {
            let hex = faster_hex::hex_string(&bytes[i..]);
            let trimmed = hex.trim_start_matches('0');
            json!(format!("0x{}", if trimmed.is_empty() { "0" } else { trimmed }))
        }
    }
}

/// Parse a 20-byte address from `params[idx]` (a `0x`-hex string).
fn parse_address_param(params: &Value, idx: usize) -> EthResult<[u8; 20]> {
    let s = params
        .as_array()
        .and_then(|a| a.get(idx))
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params(format!("expected a hex address at param #{idx}")))?;
    parse_addr20(s)
}

/// Parse an `EvmU256` storage slot key from `params[idx]` (`0x`-hex, ≤32 bytes,
/// right-aligned big-endian).
fn parse_slot_param(params: &Value, idx: usize) -> EthResult<EvmU256> {
    let s = params
        .as_array()
        .and_then(|a| a.get(idx))
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params(format!("expected a hex value at param #{idx}")))?;
    // A slot key is a QUANTITY (odd-length hex like "0x0" is valid).
    Ok(EvmU256::from_be_bytes(be32_from_hex(s)?))
}

// --- eth_call / eth_estimateGas (Increment 4) ---

/// eth_call / eth_estimateGas execute only against the latest head. Reject a
/// historical block selector with a clear error instead of silently returning a
/// head result for a historical query (audit H-03). `latest`/`pending`/absent OK.
fn require_latest_exec_block(params: &Value, idx: usize) -> EthResult<()> {
    match params.as_array().and_then(|a| a.get(idx)).and_then(|v| v.as_str()) {
        None | Some("latest") | Some("pending") => Ok(()),
        Some(other) => Err(EthRpcError::invalid_params(format!(
            "eth_call/eth_estimateGas execute only at \"latest\"; historical execution at \"{other}\" is not supported"
        ))),
    }
}

async fn eth_call_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    require_latest_exec_block(params, 1)?;
    let req = parse_call_request(params)?;
    let out = provider.eth_call(req).await?;
    Ok(json!(format!("0x{}", faster_hex::hex_string(&out))))
}

async fn eth_estimate_gas_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    require_latest_exec_block(params, 1)?;
    let req = parse_call_request(params)?;
    Ok(quantity(provider.estimate_gas(req).await? as u128))
}

// --- eth_sendRawTransaction / eth_getTransactionReceipt (Increment 5) ---

async fn eth_send_raw_transaction(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hex = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params("eth_sendRawTransaction expects [rawTx]"))?;
    let raw = decode_hex(hex)?;
    if raw.is_empty() {
        return Err(EthRpcError::invalid_params("empty raw transaction"));
    }
    let hash = provider.send_raw_transaction(raw).await?;
    Ok(json!(format!("0x{}", faster_hex::hex_string(&hash))))
}

async fn eth_get_transaction_receipt(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let s = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params("eth_getTransactionReceipt expects [txHash]"))?;
    let bytes = decode_hex(s)?;
    if bytes.len() != 32 {
        return Err(EthRpcError::invalid_params("transaction hash must be 32 bytes"));
    }
    let mut tx_hash = [0u8; 32];
    tx_hash.copy_from_slice(&bytes);
    match provider.transaction_receipt(tx_hash).await? {
        None => Ok(Value::Null),
        Some(r) => Ok(format_receipt(&r)),
    }
}

async fn misaka_get_evm_tx_status(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let tx_hash = parse_hash32_param(params, 0)?;
    Ok(format_evm_tx_status(&provider.evm_tx_status(tx_hash).await?))
}

fn format_evm_tx_status(s: &EthEvmTxStatus) -> Value {
    json!({
        "transactionHash": format!("0x{}", faster_hex::hex_string(&s.tx_hash)),
        "state": s.state,
        "inMempool": s.in_mempool,
        "includedIn": s.included_in.iter().map(|h| json!(format!("0x{}", faster_hex::hex_string(h)))).collect::<Vec<_>>(),
        "acceptedIn": s
            .accepted_in
            .map(|(h, idx)| json!({ "block": format!("0x{}", faster_hex::hex_string(&h)), "receiptIndex": idx }))
            .unwrap_or(Value::Null),
        "orphanedAcceptances": s
            .orphaned_acceptances
            .iter()
            .map(|(h, idx)| json!({ "block": format!("0x{}", faster_hex::hex_string(h)), "receiptIndex": idx }))
            .collect::<Vec<_>>(),
        "lastSkipClass": s.last_skip_class.map(|c| json!(c)).unwrap_or(Value::Null),
        "sender": s.sender.map(|a| json!(format!("0x{}", faster_hex::hex_string(&a)))).unwrap_or(Value::Null),
        "nonce": s.nonce.map(|n| quantity(n as u128)).unwrap_or(Value::Null),
        "gasLimit": s.gas_limit.map(|g| quantity(g as u128)).unwrap_or(Value::Null),
    })
}

/// Render an [`EthReceipt`] as the standard `eth_getTransactionReceipt` JSON.
fn format_receipt(r: &EthReceipt) -> Value {
    let tx_hash = format!("0x{}", faster_hex::hex_string(&r.tx_hash));
    let block_hash = format!("0x{}", faster_hex::hex_string(&r.block_hash));
    let block_number = quantity(r.block_number as u128);
    let tx_index = quantity(r.tx_index as u128);
    let logs: Vec<Value> = r
        .logs
        .iter()
        .enumerate()
        .map(|(i, lg)| {
            let topics: Vec<Value> = lg.topics.iter().map(|t| json!(format!("0x{}", faster_hex::hex_string(t)))).collect();
            json!({
                "address": format!("0x{}", faster_hex::hex_string(&lg.address)),
                "topics": topics,
                "data": format!("0x{}", faster_hex::hex_string(&lg.data)),
                "blockNumber": block_number.clone(),
                "blockHash": block_hash.clone(),
                "transactionHash": tx_hash.clone(),
                "transactionIndex": tx_index.clone(),
                "logIndex": quantity((r.log_index_offset as u128) + i as u128),
                "removed": false,
            })
        })
        .collect();
    json!({
        "transactionHash": tx_hash,
        "transactionIndex": tx_index,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "from": r.from.map(|a| json!(format!("0x{}", faster_hex::hex_string(&a)))).unwrap_or(Value::Null),
        "to": r.to.map(|a| json!(format!("0x{}", faster_hex::hex_string(&a)))).unwrap_or(Value::Null),
        "cumulativeGasUsed": quantity(r.cumulative_gas_used as u128),
        "gasUsed": quantity(r.gas_used as u128),
        "contractAddress": r.contract_address.map(|a| json!(format!("0x{}", faster_hex::hex_string(&a)))).unwrap_or(Value::Null),
        "logs": logs,
        "logsBloom": format!("0x{}", faster_hex::hex_string(&r.logs_bloom)),
        "status": if r.status { "0x1" } else { "0x0" },
        "type": quantity(r.tx_type as u128),
        "effectiveGasPrice": quantity(r.effective_gas_price),
    })
}

// --- eth_getBlockBy* / block tx count (Increment 6: block index) ---

#[derive(Clone)]
pub enum BlockId {
    Number(u64),
    Tag(String),
}

/// Parse a block selector (`"latest"`/`"safe"`/`"finalized"`/`"earliest"`/`"pending"`
/// or a hex QUANTITY block number) from `params[idx]`.
fn parse_block_param(params: &Value, idx: usize) -> EthResult<BlockId> {
    let s = params
        .as_array()
        .and_then(|a| a.get(idx))
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params(format!("expected a block number or tag at param #{idx}")))?;
    Ok(match s {
        "latest" | "pending" | "safe" | "finalized" | "earliest" => BlockId::Tag(s.to_string()),
        hex => BlockId::Number(u64_from_hex(hex)?),
    })
}

/// Parse a 32-byte hash from `params[idx]` (a `0x`-hex string).
fn parse_hash32_param(params: &Value, idx: usize) -> EthResult<[u8; 32]> {
    let s = params
        .as_array()
        .and_then(|a| a.get(idx))
        .and_then(|v| v.as_str())
        .ok_or_else(|| EthRpcError::invalid_params(format!("expected a 32-byte hash at param #{idx}")))?;
    let b = decode_hex(s)?;
    if b.len() != 32 {
        return Err(EthRpcError::invalid_params("hash must be 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

async fn resolve_block(provider: &Arc<dyn EthProvider>, id: BlockId) -> EthResult<Option<EthBlock>> {
    match id {
        BlockId::Number(n) => provider.block_by_number(n).await,
        BlockId::Tag(t) => provider.block_by_tag(&t).await,
    }
}

/// Fetch full tx objects for a block iff `full` (the `fullTransactionObjects`
/// boolean param), else `None` so [`render_block`] emits hashes (audit H-04).
async fn full_txs_for(provider: &Arc<dyn EthProvider>, b: &EthBlock, full: bool) -> EthResult<Option<Vec<EthTx>>> {
    if !full {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(b.tx_hashes.len());
    for h in &b.tx_hashes {
        if let Some(tx) = provider.transaction_by_hash(*h).await? {
            out.push(tx);
        }
    }
    Ok(Some(out))
}

async fn eth_get_block_by_number(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let id = parse_block_param(params, 0)?;
    let full = params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
    match resolve_block(provider, id).await? {
        Some(b) => {
            let txs = full_txs_for(provider, &b, full).await?;
            Ok(render_block(&b, txs.as_deref()))
        }
        None => Ok(Value::Null),
    }
}

async fn eth_get_block_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    let full = params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
    match provider.block_by_hash(hash).await? {
        Some(b) => {
            let txs = full_txs_for(provider, &b, full).await?;
            Ok(render_block(&b, txs.as_deref()))
        }
        None => Ok(Value::Null),
    }
}

async fn eth_get_block_tx_count_by_number(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let id = parse_block_param(params, 0)?;
    Ok(resolve_block(provider, id).await?.map(|b| quantity(b.tx_hashes.len() as u128)).unwrap_or(Value::Null))
}

async fn eth_get_block_tx_count_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.block_by_hash(hash).await?.map(|b| quantity(b.tx_hashes.len() as u128)).unwrap_or(Value::Null))
}

/// Render an [`EthBlock`] as the standard `eth_getBlockBy*` JSON. When
/// `full_txs` is `Some`, the `transactions` array holds full tx objects (the
/// `fullTransactionObjects=true` form, audit H-04); otherwise it holds hashes.
/// Uncle/PoW fields are the canonical empty-chain constants Ethereum tooling expects.
fn render_block(b: &EthBlock, full_txs: Option<&[EthTx]>) -> Value {
    let hx = |bytes: &[u8]| format!("0x{}", faster_hex::hex_string(bytes));
    let txs: Vec<Value> = match full_txs {
        Some(objs) => objs.iter().map(render_tx).collect(),
        None => b.tx_hashes.iter().map(|h| json!(hx(h))).collect(),
    };
    json!({
        "number": quantity(b.number as u128),
        "hash": hx(&b.hash),
        "parentHash": hx(&b.parent_hash),
        "nonce": "0x0000000000000000",
        "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
        "logsBloom": hx(&b.logs_bloom),
        "transactionsRoot": hx(&b.transactions_root),
        "stateRoot": hx(&b.state_root),
        "receiptsRoot": hx(&b.receipts_root),
        "miner": hx(&b.miner),
        "difficulty": "0x0",
        "totalDifficulty": "0x0",
        "extraData": "0x",
        "size": "0x0",
        "gasLimit": quantity(b.gas_limit as u128),
        "gasUsed": quantity(b.gas_used as u128),
        "timestamp": quantity(b.timestamp as u128),
        "baseFeePerGas": quantity_from_be32(&b.base_fee_per_gas),
        "transactions": txs,
        "uncles": [],
    })
}

// --- eth_getLogs (Increment 6: log index) ---

/// Resolve a `fromBlock`/`toBlock` filter value (tag or hex number) to an
/// `evm_number`. Absent / `latest` / `pending` / `safe` / `finalized` ⇒ the head.
async fn resolve_block_number(provider: &Arc<dyn EthProvider>, v: Option<&Value>) -> EthResult<u64> {
    match v.and_then(|x| x.as_str()) {
        None | Some("latest") | Some("pending") | Some("safe") | Some("finalized") => provider.block_number().await,
        Some("earliest") => Ok(0),
        Some(hex) => u64_from_hex(hex),
    }
}

/// Parse the `address` filter: absent/null ⇒ any; a single `0x`-hex string or an
/// array of them.
fn parse_address_list(v: Option<&Value>) -> EthResult<Vec<[u8; 20]>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(vec![parse_addr20(s)?]),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|x| x.as_str().ok_or_else(|| EthRpcError::invalid_params("address array entries must be strings")).and_then(parse_addr20))
            .collect(),
        _ => Err(EthRpcError::invalid_params("address must be a string or an array")),
    }
}

fn parse_topic32(s: &str) -> EthResult<[u8; 32]> {
    let b = decode_hex(s)?;
    if b.len() != 32 {
        return Err(EthRpcError::invalid_params("topic must be 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

/// Parse the `topics` filter into per-position option sets (empty = wildcard):
/// each entry is `null` (wildcard), a single topic, or an array (OR).
fn parse_topic_filter(v: Option<&Value>) -> EthResult<Vec<Vec<[u8; 32]>>> {
    let Some(Value::Array(arr)) = v else { return Ok(Vec::new()) };
    arr.iter()
        .map(|pos| match pos {
            Value::Null => Ok(Vec::new()),
            Value::String(s) => Ok(vec![parse_topic32(s)?]),
            Value::Array(opts) => opts
                .iter()
                .map(|o| o.as_str().ok_or_else(|| EthRpcError::invalid_params("topic options must be strings")).and_then(parse_topic32))
                .collect(),
            _ => Err(EthRpcError::invalid_params("each topic must be null, a string, or an array")),
        })
        .collect()
}

async fn eth_get_logs(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let f = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_object())
        .ok_or_else(|| EthRpcError::invalid_params("eth_getLogs expects a filter object"))?;
    let addresses = parse_address_list(f.get("address"))?;
    let topics = parse_topic_filter(f.get("topics"))?;
    let (from, to) = if let Some(bh) = f.get("blockHash").and_then(|v| v.as_str()) {
        let h = parse_hash32_param(&json!([bh]), 0)?;
        let blk = provider.block_by_hash(h).await?.ok_or_else(|| EthRpcError::invalid_params("unknown blockHash"))?;
        (blk.number, blk.number)
    } else {
        let from = resolve_block_number(provider, f.get("fromBlock")).await?;
        let to = resolve_block_number(provider, f.get("toBlock")).await?;
        (from, to)
    };
    // DoS bound: cap the scanned range (matches the node-side per-result cap).
    const MAX_RANGE: u64 = 10_000;
    if to >= from && to - from >= MAX_RANGE {
        return Err(EthRpcError::new(codes::SERVER_ERROR, "eth_getLogs block range too large (max 10000 blocks)"));
    }
    let logs = provider.get_logs(from, to, addresses, topics).await?;
    Ok(Value::Array(logs.iter().map(render_log).collect()))
}

fn render_log(e: &EthLogEntry) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let topics: Vec<Value> = e.topics.iter().map(|t| json!(hx(t))).collect();
    json!({
        "address": hx(&e.address),
        "topics": topics,
        "data": hx(&e.data),
        "blockNumber": quantity(e.block_number as u128),
        "blockHash": hx(&e.block_hash),
        "transactionHash": hx(&e.tx_hash),
        "transactionIndex": quantity(e.tx_index as u128),
        "logIndex": quantity(e.log_index as u128),
        "removed": false,
    })
}

// --- eth_feeHistory (EIP-1559 fee estimation for Foundry/ethers/viem/MetaMask) ---

async fn eth_fee_history(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let arr = params
        .as_array()
        .ok_or_else(|| EthRpcError::invalid_params("eth_feeHistory expects [blockCount, newestBlock, rewardPercentiles?]"))?;
    let block_count = match arr.first() {
        Some(Value::String(s)) => u64_from_hex(s)?,
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| EthRpcError::invalid_params("blockCount"))?,
        _ => return Err(EthRpcError::invalid_params("blockCount required")),
    };
    let newest = resolve_block_number(provider, arr.get(1)).await?;
    let percentiles: Vec<f64> =
        arr.get(2).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
    let fh = provider.fee_history(block_count.clamp(1, 1024), newest, percentiles).await?;
    let mut obj = serde_json::Map::new();
    obj.insert("oldestBlock".to_string(), quantity(fh.oldest_block as u128));
    obj.insert("baseFeePerGas".to_string(), Value::Array(fh.base_fee_per_gas.iter().map(|b| quantity_from_be32(b)).collect()));
    obj.insert("gasUsedRatio".to_string(), json!(fh.gas_used_ratio));
    if let Some(reward) = fh.reward {
        obj.insert(
            "reward".to_string(),
            Value::Array(reward.iter().map(|row| Value::Array(row.iter().map(|b| quantity_from_be32(b)).collect())).collect()),
        );
    }
    Ok(Value::Object(obj))
}

// --- eth_getTransactionByHash (Increment 6: tx index) ---

async fn eth_get_transaction_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.transaction_by_hash(hash).await?.map(|t| render_tx(&t)).unwrap_or(Value::Null))
}

/// Render an [`EthTx`] as the standard `eth_getTransactionByHash` JSON. `v/r/s`
/// are not surfaced yet (reads rarely need them); block context is null when pending.
fn render_tx(t: &EthTx) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    json!({
        "hash": hx(&t.hash),
        "from": hx(&t.from),
        "to": t.to.map(|a| json!(hx(&a))).unwrap_or(Value::Null),
        "nonce": quantity(t.nonce as u128),
        "value": quantity_from_be32(&t.value),
        "gas": quantity(t.gas as u128),
        "gasPrice": quantity(t.gas_price),
        "maxFeePerGas": quantity(t.gas_price),
        "maxPriorityFeePerGas": t.max_priority_fee_per_gas.map(quantity).unwrap_or(Value::Null),
        "input": hx(&t.input),
        "type": quantity(t.tx_type as u128),
        "chainId": t.chain_id.map(|c| quantity(c as u128)).unwrap_or(Value::Null),
        "blockNumber": t.block_number.map(|n| quantity(n as u128)).unwrap_or(Value::Null),
        "blockHash": t.block_hash.map(|h| json!(hx(&h))).unwrap_or(Value::Null),
        "transactionIndex": t.tx_index.map(|i| quantity(i as u128)).unwrap_or(Value::Null),
        "v": "0x0",
        "r": "0x0",
        "s": "0x0",
    })
}

/// Parse the `eth_call` / `eth_estimateGas` call object from `params[0]`.
fn parse_call_request(params: &Value) -> EthResult<EthCallRequest> {
    let obj = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_object())
        .ok_or_else(|| EthRpcError::invalid_params("expected a call object as the first parameter"))?;
    let from = match obj.get("from").and_then(|v| v.as_str()) {
        Some(s) => parse_addr20(s)?,
        None => [0u8; 20],
    };
    let to = match obj.get("to").and_then(|v| v.as_str()) {
        Some(s) => Some(parse_addr20(s)?),
        None => None,
    };
    let data = match obj.get("data").or_else(|| obj.get("input")).and_then(|v| v.as_str()) {
        Some(s) => decode_hex(s)?,
        None => Vec::new(),
    };
    let value = match obj.get("value").and_then(|v| v.as_str()) {
        Some(s) => be32_from_hex(s)?,
        None => [0u8; 32],
    };
    let gas = match obj.get("gas").and_then(|v| v.as_str()) {
        Some(s) => u64_from_hex(s)?,
        None => 0,
    };
    Ok(EthCallRequest { from, to, value, data, gas })
}

/// Parse a 20-byte address from a `0x`-hex string.
fn parse_addr20(s: &str) -> EthResult<[u8; 20]> {
    let bytes = decode_hex(s)?;
    if bytes.len() != 20 {
        return Err(EthRpcError::invalid_params("address must be 20 bytes"));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a hex QUANTITY (odd-length allowed) into a `u64`.
fn u64_from_hex(s: &str) -> EthResult<u64> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).map_err(|e| EthRpcError::invalid_params(format!("invalid u64 hex: {e}")))
}

/// Parse a hex QUANTITY (odd-length allowed, ≤32 bytes) into a right-aligned 32-byte BE array.
fn be32_from_hex(s: &str) -> EthResult<[u8; 32]> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    let padded = if s.len() % 2 == 1 { format!("0{s}") } else { s.to_string() };
    let mut bytes = vec![0u8; padded.len() / 2];
    faster_hex::hex_decode(padded.as_bytes(), &mut bytes).map_err(|e| EthRpcError::invalid_params(format!("invalid hex: {e}")))?;
    if bytes.len() > 32 {
        return Err(EthRpcError::invalid_params("value exceeds 32 bytes"));
    }
    let mut be = [0u8; 32];
    be[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(be)
}

/// Decode a `0x`-prefixed (or bare) hex string to bytes.
pub fn decode_hex(s: &str) -> EthResult<Vec<u8>> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err(EthRpcError::invalid_params("odd-length hex"));
    }
    let mut out = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).map_err(|e| EthRpcError::invalid_params(format!("malformed hex: {e}")))?;
    Ok(out)
}

/// Dispatch a single request OR a batch array. Returns `None` when there is no
/// response to send (a notification, or an all-notification batch — audit L-03).
async fn process(provider: &Arc<dyn EthProvider>, body: Value) -> Option<Value> {
    match body {
        Value::Array(items) => {
            // JSON-RPC: an empty batch is itself an invalid request (audit L-03).
            if items.is_empty() {
                return Some(err_value(codes::INVALID_REQUEST, "empty batch"));
            }
            // Audit H-02: bound the batch so one request cannot fan out into
            // tens of thousands of dispatched calls (a 4 MiB body of tiny calls).
            if items.len() > MAX_BATCH_ITEMS {
                return Some(err_value(codes::INVALID_REQUEST, &format!("batch too large: {} items > {MAX_BATCH_ITEMS} max", items.len())));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if let Some(resp) = handle_one(provider, item).await {
                    out.push(serde_json::to_value(resp).unwrap_or(Value::Null));
                }
            }
            // All-notification batch ⇒ no response body.
            (!out.is_empty()).then_some(Value::Array(out))
        }
        single => handle_one(provider, single).await.map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
    }
}

fn err_value(code: i64, msg: &str) -> Value {
    serde_json::to_value(RpcResponse::err(Value::Null, EthRpcError::new(code, msg.to_string()))).unwrap_or(Value::Null)
}

/// Dispatch one request. Returns `None` for a NOTIFICATION (a request with no
/// `id` member — `id:null` is a normal request and still gets a response).
async fn handle_one(provider: &Arc<dyn EthProvider>, item: Value) -> Option<RpcResponse> {
    let is_notification = item.is_object() && item.get("id").is_none();
    let id = item.get("id").cloned().unwrap_or(Value::Null);
    let resp = match serde_json::from_value::<RpcRequest>(item) {
        Ok(req) => dispatch(provider, req).await,
        Err(e) => RpcResponse::err(id, EthRpcError::new(codes::INVALID_REQUEST, format!("invalid request: {e}"))),
    };
    (!is_notification).then_some(resp)
}

// ---------------------------------------------------------------------------
// Minimal HTTP/1.1 JSON-RPC server (no axum/hyper — keeps deps + audit small)
// ---------------------------------------------------------------------------

/// Defensive cap on a single JSON-RPC request body.
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Max JSON-RPC batch items per request (audit H-02).
const MAX_BATCH_ITEMS: usize = 100;
/// Max concurrent connections served at once (audit H-02). Excess connections
/// are dropped immediately (backpressure) rather than queued, so a connection
/// flood cannot accumulate unbounded tasks/sockets.
const MAX_CONNECTIONS: usize = 512;
/// Whole-connection deadline (audit H-02): read + dispatch + write. A slowloris
/// that never finishes its headers/body, or stalls mid-write, is dropped here.
const CONN_TIMEOUT: Duration = Duration::from_secs(30);
/// Max serialized response bytes (audit H-02): refuse to allocate/emit an
/// unbounded response (a 10k-log page can still be large but is now finite).
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Serve the Ethereum JSON-RPC endpoint on `addr` until the process exits.
pub async fn serve(addr: SocketAddr, provider: Arc<dyn EthProvider>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    kaspa_core::info!("[eth-rpc] Ethereum JSON-RPC listening on http://{addr}");
    let conn_sem = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let (stream, _peer) = listener.accept().await?;
        // At capacity: drop the new connection immediately (no unbounded spawn).
        let Ok(permit) = conn_sem.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let provider = provider.clone();
        tokio::spawn(async move {
            let _permit = permit; // released on task end
            match tokio::time::timeout(CONN_TIMEOUT, serve_conn(stream, provider)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => kaspa_core::trace!("[eth-rpc] connection error: {e}"),
                Err(_) => kaspa_core::trace!("[eth-rpc] connection timed out after {CONN_TIMEOUT:?}"),
            }
        });
    }
}

/// Spawn the Ethereum JSON-RPC server on a background task. Logs and exits the
/// task on bind failure (the rest of the node keeps running).
pub fn spawn(addr: SocketAddr, provider: Arc<dyn EthProvider>) {
    tokio::spawn(async move {
        if let Err(e) = serve(addr, provider).await {
            kaspa_core::warn!("[eth-rpc] server on {addr} exited: {e}");
        }
    });
}

/// Handle ONE HTTP/1.1 connection: read the request, dispatch, write the
/// response, close (`Connection: close` — no keep-alive; clients reconnect).
async fn serve_conn(mut stream: TcpStream, provider: Arc<dyn EthProvider>) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];
    // Read until the full header block (CRLFCRLF) is present.
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_BODY_BYTES {
            return write_response(&mut stream, 431, "Request Header Fields Too Large", "").await;
        }
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(()); // client closed before sending headers
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let http_method = request_line.split_whitespace().next().unwrap_or("");

    // CORS preflight for browser dApps (MetaMask in-page, etc.).
    if http_method.eq_ignore_ascii_case("OPTIONS") {
        return write_cors_preflight(&mut stream).await;
    }
    if !http_method.eq_ignore_ascii_case("POST") {
        return write_response(&mut stream, 405, "Method Not Allowed", "").await;
    }

    let content_length = lines
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.trim().eq_ignore_ascii_case("content-length") {
                v.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return write_response(&mut stream, 413, "Payload Too Large", "").await;
    }

    // Body: whatever followed the headers, then read up to content_length.
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);

    let response_json: Option<Value> = match serde_json::from_slice::<Value>(&body) {
        Ok(v) => process(&provider, v).await,
        Err(e) => Some(err_value(codes::PARSE_ERROR, &format!("parse error: {e}"))),
    };
    // Notification(s): no response body (audit L-03).
    let Some(response_json) = response_json else {
        return write_response(&mut stream, 204, "No Content", "").await;
    };
    let payload = serde_json::to_string(&response_json).unwrap_or_else(|_| "null".to_string());
    // Audit H-02: cap the response so a single request cannot emit an unbounded body.
    if payload.len() > MAX_RESPONSE_BYTES {
        let err = serde_json::to_string(&RpcResponse::err(
            Value::Null,
            EthRpcError::new(codes::SERVER_ERROR, format!("response too large ({} bytes); narrow the query", payload.len())),
        ))
        .unwrap_or_else(|_| "null".to_string());
        return write_response(&mut stream, 200, "OK", &err).await;
    }
    write_response(&mut stream, 200, "OK", &payload).await
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

async fn write_response(stream: &mut TcpStream, status: u16, reason: &str, body: &str) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await
}

async fn write_cors_preflight(stream: &mut TcpStream) -> std::io::Result<()> {
    let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await
}
