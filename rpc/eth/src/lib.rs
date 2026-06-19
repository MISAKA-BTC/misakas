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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
    pub logs: Vec<EthLog>,
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

    /// `eth_getBlockByNumber` for a numeric block (canonical EVM block at that number).
    async fn block_by_number(&self, number: u64) -> EthResult<Option<EthBlock>>;

    /// `eth_getBlockByNumber` for a tag: `latest`/`pending` (the sink), `safe`,
    /// `finalized`, or `earliest`. Unknown tags ⇒ `invalid_params` by the caller.
    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>>;

    /// `eth_getBlockByHash` for a 32-byte eth-rpc block id.
    async fn block_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthBlock>>;
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
        "eth_getBlockByNumber" => eth_get_block_by_number(provider, &req.params).await,
        "eth_getBlockByHash" => eth_get_block_by_hash(provider, &req.params).await,
        "eth_getBlockTransactionCountByNumber" => eth_get_block_tx_count_by_number(provider, &req.params).await,
        "eth_getBlockTransactionCountByHash" => eth_get_block_tx_count_by_hash(provider, &req.params).await,
        // Wallets poll this; we do not index pending txs by hash, so report
        // "unknown" (null) — the receipt is the source of truth for inclusion.
        "eth_getTransactionByHash" => Ok(Value::Null),
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

// --- eth_* state queries (Increment 3) — served at the "latest" head snapshot ---

async fn eth_get_balance(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    Ok(provider.latest_account(addr).await?.map(|a| quantity_from_be32(&a.balance.to_be_bytes())).unwrap_or_else(|| json!("0x0")))
}

async fn eth_get_transaction_count(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    Ok(quantity(provider.latest_account(addr).await?.map(|a| a.nonce as u128).unwrap_or(0)))
}

async fn eth_get_code(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    let code = provider.latest_account(addr).await?.map(|a| a.code).unwrap_or_default();
    Ok(json!(format!("0x{}", faster_hex::hex_string(&code))))
}

async fn eth_get_storage_at(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let addr = parse_address_param(params, 0)?;
    let slot = parse_slot_param(params, 1)?;
    let value = provider.latest_account(addr).await?.and_then(|a| a.storage.into_iter().find(|(k, _)| *k == slot).map(|(_, v)| v));
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

async fn eth_call_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let req = parse_call_request(params)?;
    let out = provider.eth_call(req).await?;
    Ok(json!(format!("0x{}", faster_hex::hex_string(&out))))
}

async fn eth_estimate_gas_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
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
                "logIndex": quantity(i as u128),
                "removed": false,
            })
        })
        .collect();
    json!({
        "transactionHash": tx_hash,
        "transactionIndex": tx_index,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "from": Value::Null,
        "to": Value::Null,
        "cumulativeGasUsed": quantity(r.cumulative_gas_used as u128),
        "gasUsed": quantity(r.gas_used as u128),
        "contractAddress": Value::Null,
        "logs": logs,
        "logsBloom": format!("0x{}", "00".repeat(256)),
        "status": if r.status { "0x1" } else { "0x0" },
        "type": "0x0",
    })
}

// --- eth_getBlockBy* / block tx count (Increment 6: block index) ---

enum BlockId {
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

async fn eth_get_block_by_number(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let id = parse_block_param(params, 0)?;
    Ok(resolve_block(provider, id).await?.map(|b| render_block(&b)).unwrap_or(Value::Null))
}

async fn eth_get_block_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.block_by_hash(hash).await?.map(|b| render_block(&b)).unwrap_or(Value::Null))
}

async fn eth_get_block_tx_count_by_number(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let id = parse_block_param(params, 0)?;
    Ok(resolve_block(provider, id).await?.map(|b| quantity(b.tx_hashes.len() as u128)).unwrap_or(Value::Null))
}

async fn eth_get_block_tx_count_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.block_by_hash(hash).await?.map(|b| quantity(b.tx_hashes.len() as u128)).unwrap_or(Value::Null))
}

/// Render an [`EthBlock`] as the standard `eth_getBlockBy*` JSON. Transactions
/// are returned as hashes (full-tx objects land with `eth_getTransactionByHash`).
/// Uncle/PoW fields are the canonical empty-chain constants Ethereum tooling expects.
fn render_block(b: &EthBlock) -> Value {
    let hx = |bytes: &[u8]| format!("0x{}", faster_hex::hex_string(bytes));
    let txs: Vec<Value> = b.tx_hashes.iter().map(|h| json!(hx(h))).collect();
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

/// Dispatch a single request OR a batch array, returning the matching JSON shape.
async fn process(provider: &Arc<dyn EthProvider>, body: Value) -> Value {
    match body {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(serde_json::to_value(handle_one(provider, item).await).unwrap_or(Value::Null));
            }
            Value::Array(out)
        }
        single => serde_json::to_value(handle_one(provider, single).await).unwrap_or(Value::Null),
    }
}

async fn handle_one(provider: &Arc<dyn EthProvider>, item: Value) -> RpcResponse {
    let id = item.get("id").cloned().unwrap_or(Value::Null);
    match serde_json::from_value::<RpcRequest>(item) {
        Ok(req) => dispatch(provider, req).await,
        Err(e) => RpcResponse::err(id, EthRpcError::new(codes::INVALID_REQUEST, format!("invalid request: {e}"))),
    }
}

// ---------------------------------------------------------------------------
// Minimal HTTP/1.1 JSON-RPC server (no axum/hyper — keeps deps + audit small)
// ---------------------------------------------------------------------------

/// Defensive cap on a single JSON-RPC request body.
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Serve the Ethereum JSON-RPC endpoint on `addr` until the process exits.
pub async fn serve(addr: SocketAddr, provider: Arc<dyn EthProvider>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    kaspa_core::info!("[eth-rpc] Ethereum JSON-RPC listening on http://{addr}");
    loop {
        let (stream, _peer) = listener.accept().await?;
        let provider = provider.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_conn(stream, provider).await {
                kaspa_core::trace!("[eth-rpc] connection error: {e}");
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

    let response_json = match serde_json::from_slice::<Value>(&body) {
        Ok(v) => process(&provider, v).await,
        Err(e) => serde_json::to_value(RpcResponse::err(Value::Null, EthRpcError::new(codes::PARSE_ERROR, format!("parse error: {e}"))))
            .unwrap_or(Value::Null),
    };
    let payload = serde_json::to_string(&response_json).unwrap_or_else(|_| "null".to_string());
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
