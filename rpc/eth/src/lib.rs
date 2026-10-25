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

/// §9: the RFC 6455 WebSocket transport (`eth_subscribe`). A `serve_conn` that
/// sees an `Upgrade: websocket` request hands the socket to [`ws::serve_ws`].
mod ws;

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
    /// ECDSA signature for the full tx object (audit R-3). `v` is the EIP-155 /
    /// y-parity value, `r`/`s` are big-endian 32 bytes, `y_parity` the EIP-2718 bit.
    pub v: u64,
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub y_parity: bool,
    /// EIP-2930/1559 access list (empty for legacy txs).
    pub access_list: Vec<EthAccessListItem>,
}

/// One EIP-2930/1559 access-list entry of an [`EthTx`].
#[derive(Clone, Debug)]
pub struct EthAccessListItem {
    pub address: [u8; 20],
    pub storage_keys: Vec<[u8; 32]>,
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
    /// RPC §7.3 `size`: byte length of the block's accepted tx data (was `0x0`).
    pub size: u64,
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

/// One §9 `eth_subscribe("logs")` event: a log plus its reorg disposition. On a
/// reorg the node emits detached logs with `removed = true` (oldest-first) before
/// the new canonical logs with `removed = false` (Ethereum log-stream semantics).
#[derive(Clone, Debug)]
pub struct EthLogEvent {
    pub log: EthLogEntry,
    pub removed: bool,
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
/// One node of a `debug_traceTransaction` `callTracer` result (§11.4). A primitive
/// (no revm) wire type: kaspad's provider converts the executor's call-frame tree
/// into this, and [`render_call_frame`] emits the Geth JSON. The MISAKA extensions
/// are set on the ROOT frame only.
#[derive(Clone, Debug)]
pub struct EthCallFrame {
    /// `CALL` / `CALLCODE` / `DELEGATECALL` / `STATICCALL` / `CREATE` / `CREATE2`.
    pub call_type: String,
    pub from: [u8; 20],
    /// `None` for a CREATE whose address is unknown (creation failed).
    pub to: Option<[u8; 20]>,
    /// Big-endian U256 call value.
    pub value: [u8; 32],
    pub gas: u64,
    pub gas_used: u64,
    pub input: Vec<u8>,
    pub output: Vec<u8>,
    /// `Some` for a failed frame (e.g. "execution reverted", "out of gas").
    pub error: Option<String>,
    /// Decoded Solidity revert reason, when present.
    pub revert_reason: Option<String>,
    pub calls: Vec<EthCallFrame>,
    /// §11.4 root-only extension: the payload (DAG) block that carried the tx.
    pub misaka_originating_payload_block: Option<Vec<u8>>,
    /// §11.4 root-only extension: the accepting block whose context the trace used.
    pub misaka_accepting_block: Option<Vec<u8>>,
}

/// One account's `prestateTracer` (diffMode) state view (§11.1). `code` empty ⇒ no
/// code; `storage` holds the diff-relevant `(slot, value)` big-endian pairs.
#[derive(Clone, Debug)]
pub struct EthAccountState {
    pub balance: [u8; 32],
    pub nonce: u64,
    pub code: Vec<u8>,
    pub storage: Vec<([u8; 32], [u8; 32])>,
}

/// One account's `prestateTracer` diffMode entry. `pre = None` ⇒ created by the tx;
/// `post = None` ⇒ self-destructed.
#[derive(Clone, Debug)]
pub struct EthPrestateAccount {
    pub address: [u8; 20],
    pub pre: Option<EthAccountState>,
    pub post: Option<EthAccountState>,
}

/// The `misaka_traceEvmCandidate` diagnosis of a tx with no receipt (§11.6): the
/// result of replaying it against the current head, plus its recorded historical
/// skip class and whether it is in fact accepted now.
#[derive(Clone, Debug)]
pub struct EthCandidateTrace {
    /// `false` ⇒ failed pre-execution validation (nonce/funds/gas) — the class-2 family.
    pub executed: bool,
    /// Top-level success when `executed`.
    pub succeeded: bool,
    pub gas_used: u64,
    pub output: Vec<u8>,
    /// Pre-validation error or decoded revert reason.
    pub reason: Option<String>,
    /// The §6.1 class of the most recent recorded skip (2/3/5), when never accepted.
    pub recorded_skip_class: Option<u8>,
    /// Whether the tx is currently accepted on the selected chain (⇒ use
    /// `debug_traceTransaction` for the canonical trace instead).
    pub accepted: bool,
    /// The call tree when the candidate executed.
    pub frame: Option<EthCallFrame>,
}

/// One opcode step of the Geth default (struct) logger (§11.1). Memory/storage are
/// omitted (§11.5 — off by default).
#[derive(Clone, Debug)]
pub struct EthStructLog {
    pub pc: u64,
    pub op: String,
    pub gas: u64,
    pub gas_cost: u64,
    pub depth: u32,
    /// Stack (bottom→top) as big-endian 32-byte words.
    pub stack: Vec<[u8; 32]>,
    pub error: Option<String>,
}

/// The Geth default struct-logger result: total gas, failure flag, return data, and
/// the per-opcode log.
#[derive(Clone, Debug)]
pub struct EthStructLogTrace {
    pub gas: u64,
    pub failed: bool,
    pub return_value: Vec<u8>,
    pub struct_logs: Vec<EthStructLog>,
}

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

    /// `eth_call`: read-only execution at `block` (default `latest`); returns the
    /// call's output bytes (revert data on a revert, surfaced as an error by the
    /// caller). §12.5/§12.6: a historical block reconstructs that block's state and
    /// uses that block's env (number/timestamp/coinbase/gas limit/chain id).
    async fn eth_call(&self, req: EthCallRequest, block: BlockId) -> EthResult<Vec<u8>>;

    /// `eth_estimateGas`: the minimal gas limit that lets the call succeed at `block`.
    async fn estimate_gas(&self, req: EthCallRequest, block: BlockId) -> EthResult<u64>;

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
    async fn get_logs(&self, from: u64, to: u64, addresses: Vec<[u8; 20]>, topics: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>>;

    /// `eth_feeHistory`: base fees + gas-used ratios over the last `block_count`
    /// blocks ending at `newest` (used by EIP-1559 tooling — Foundry/ethers/MetaMask).
    async fn fee_history(&self, block_count: u64, newest: u64, reward_percentiles: Vec<f64>) -> EthResult<EthFeeHistory>;

    /// §9 (`eth_subscribe("newPendingTransactions")`): a broadcast receiver
    /// yielding the keccak256 hash of each EVM transaction newly admitted to this
    /// node's mempool. Lossy under lag (drop-oldest) so a slow WebSocket consumer
    /// never blocks admission — the §9.5 reconnect protocol covers any gap.
    ///
    /// Default: a closed channel (no mempool overlay) — the WebSocket forwarder
    /// ends at once and the subscription emits nothing. kaspad overrides this with
    /// a live bridge off the mining manager's admission broadcast (§9 slice 1).
    fn subscribe_pending_txs(&self) -> tokio::sync::broadcast::Receiver<[u8; 32]> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }

    /// §9 (`eth_subscribe("newHeads")`): a broadcast receiver yielding one
    /// [`EthBlock`] header per EVM-active block newly ADDED to the selected chain,
    /// in commit order (so a reorg re-announces a head at the same number with a
    /// new hash). Default: a closed channel — the WebSocket forwarder ends at
    /// once. kaspad overrides this with a pump off the consensus
    /// `VirtualChainChanged` notification.
    fn subscribe_new_heads(&self) -> tokio::sync::broadcast::Receiver<EthBlock> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }

    /// §9 (`eth_subscribe("logs")`): a broadcast receiver yielding every canonical
    /// log event in reorg order — detached logs (`removed = true`, oldest-first)
    /// then attached logs (`removed = false`). The per-subscription address/topic
    /// FILTER is applied by the WebSocket layer (shared with `eth_getLogs`), so
    /// this stream is unfiltered. Default: a closed channel; kaspad overrides it
    /// with a pump off the consensus `VirtualChainChanged` notification.
    fn subscribe_logs(&self) -> tokio::sync::broadcast::Receiver<EthLogEvent> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }

    /// `debug_traceTransaction` with the Geth `callTracer` (design §11): re-execute
    /// the accepted tx with a call-frame inspector against the exact pre-state and
    /// return its call tree, reconciled against the committed receipt. `None` = the
    /// tx is unknown / not an accepted (traceable) tx on the selected chain
    /// (§11.6 — skipped txs report a reason via `misaka_getEvmTxStatus`). Default:
    /// unsupported (providers without the EVM executor).
    async fn trace_transaction(&self, _tx_hash: [u8; 32]) -> EthResult<Option<EthCallFrame>> {
        Err(EthRpcError::new(codes::METHOD_NOT_FOUND, "debug_traceTransaction is not available on this node"))
    }

    /// `debug_traceTransaction` with the `prestateTracer` (diffMode, §11.1): the
    /// per-account pre/post state diff of the accepted tx (same replay as the
    /// callTracer). `None` = unknown / not accepted. Default: unsupported.
    async fn trace_prestate(&self, _tx_hash: [u8; 32]) -> EthResult<Option<Vec<EthPrestateAccount>>> {
        Err(EthRpcError::new(codes::METHOD_NOT_FOUND, "debug_traceTransaction is not available on this node"))
    }

    /// `misaka_traceEvmCandidate` (§11.6): diagnose a tx that has no receipt
    /// (skipped class 2/3/5 or still pending) by replaying it against the current
    /// head. `None` ⇒ the raw tx is unknown to this node. Default: unsupported.
    async fn trace_evm_candidate(&self, _tx_hash: [u8; 32]) -> EthResult<Option<EthCandidateTrace>> {
        Err(EthRpcError::new(codes::METHOD_NOT_FOUND, "misaka_traceEvmCandidate is not available on this node"))
    }

    /// `debug_traceTransaction` with the Geth default opcode/struct logger (§11.1) —
    /// the SAME replay as the callTracer, capturing per-opcode logs. `None` =
    /// unknown / not accepted. Default: unsupported.
    async fn trace_struct_log(&self, _tx_hash: [u8; 32]) -> EthResult<Option<EthStructLogTrace>> {
        Err(EthRpcError::new(codes::METHOD_NOT_FOUND, "debug_traceTransaction is not available on this node"))
    }
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
        "debug_traceTransaction" => debug_trace_transaction(provider, &req.params).await,
        "trace_transaction" => trace_transaction_flat(provider, &req.params).await,
        "misaka_traceEvmCandidate" => misaka_trace_evm_candidate(provider, &req.params).await,
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
    Ok(account_at_param(provider, addr, params, 1)
        .await?
        .map(|a| quantity_from_be32(&a.balance.to_be_bytes()))
        .unwrap_or_else(|| json!("0x0")))
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
    let value = account_at_param(provider, addr, params, 2)
        .await?
        .and_then(|a| a.storage.into_iter().find(|(k, _)| *k == slot).map(|(_, v)| v));
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

/// The optional block parameter of `eth_call` / `eth_estimateGas` at `params[idx]`
/// (§12.5): a tag/quantity string or an EIP-1898 object; absent ⇒ `latest`.
fn exec_block_param(params: &Value, idx: usize) -> EthResult<BlockId> {
    match params.as_array().and_then(|a| a.get(idx)) {
        None | Some(Value::Null) => Ok(BlockId::Tag("latest".to_string())),
        Some(v) => parse_block_id_value(v),
    }
}

async fn eth_call_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let block = exec_block_param(params, 1)?;
    let req = parse_call_request(params)?;
    let out = provider.eth_call(req, block).await?;
    Ok(json!(format!("0x{}", faster_hex::hex_string(&out))))
}

async fn eth_estimate_gas_handler(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let block = exec_block_param(params, 1)?;
    let req = parse_call_request(params)?;
    Ok(quantity(provider.estimate_gas(req, block).await? as u128))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockId {
    Number(u64),
    Tag(String),
    /// EIP-1898 `{ blockHash, requireCanonical }`: a 32-byte eth block id (the first
    /// 32 bytes of the L1 hash) and whether a non-canonical (side-branch) block must
    /// be rejected.
    Hash {
        hash: [u8; 32],
        require_canonical: bool,
    },
}

/// A block-selector tag string (`latest`/`pending`/`safe`/`finalized`/`earliest`) vs
/// a hex QUANTITY number.
fn block_id_from_str(s: &str) -> EthResult<BlockId> {
    Ok(match s {
        "latest" | "pending" | "safe" | "finalized" | "earliest" => BlockId::Tag(s.to_string()),
        hex => BlockId::Number(u64_from_hex(hex)?),
    })
}

/// Parse a 32-byte `0x`-hex hash from a JSON string value.
fn hash32_from_str(s: &str) -> EthResult<[u8; 32]> {
    let b = decode_hex(s)?;
    if b.len() != 32 {
        return Err(EthRpcError::invalid_params("blockHash must be 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

/// Parse an EIP-1898 block parameter: a tag/number string, OR an object
/// `{ "blockNumber": "0x.." }` / `{ "blockHash": "0x..", "requireCanonical": bool }`.
fn parse_block_id_value(v: &Value) -> EthResult<BlockId> {
    if let Some(s) = v.as_str() {
        return block_id_from_str(s);
    }
    if let Some(obj) = v.as_object() {
        // EIP-1898: blockHash takes precedence; else blockNumber.
        if let Some(bh) = obj.get("blockHash") {
            let s = bh.as_str().ok_or_else(|| EthRpcError::invalid_params("blockHash must be a 0x-hex string"))?;
            let hash = hash32_from_str(s)?;
            let require_canonical = obj.get("requireCanonical").and_then(|x| x.as_bool()).unwrap_or(false);
            return Ok(BlockId::Hash { hash, require_canonical });
        }
        if let Some(bn) = obj.get("blockNumber").and_then(|x| x.as_str()) {
            return block_id_from_str(bn);
        }
        return Err(EthRpcError::invalid_params("EIP-1898 block object requires \"blockNumber\" or \"blockHash\""));
    }
    Err(EthRpcError::invalid_params("expected a block number/tag or an EIP-1898 { blockNumber | blockHash } object"))
}

/// Parse a block selector from `params[idx]` (tag / number / EIP-1898 object).
fn parse_block_param(params: &Value, idx: usize) -> EthResult<BlockId> {
    let v = params
        .as_array()
        .and_then(|a| a.get(idx))
        .ok_or_else(|| EthRpcError::invalid_params(format!("expected a block number, tag, or EIP-1898 object at param #{idx}")))?;
    parse_block_id_value(v)
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
        // EIP-1898 by hash: the provider resolves the 32-byte eth id; requireCanonical
        // is enforced by the state path (account_at), not the block lookup.
        BlockId::Hash { hash, .. } => provider.block_by_hash(hash).await,
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
        "size": quantity(b.size as u128),
        "gasLimit": quantity(b.gas_limit as u128),
        "gasUsed": quantity(b.gas_used as u128),
        "timestamp": quantity(b.timestamp as u128),
        "baseFeePerGas": quantity_from_be32(&b.base_fee_per_gas),
        "transactions": txs,
        "uncles": [],
    })
}

/// Render the §9 `newHeads` notification payload for `b`. geth's `newHeads`
/// carries the block HEADER only — no `transactions`/`uncles` arrays — so this is
/// [`render_block`]'s header subset, with the same placeholder fields.
fn render_head(b: &EthBlock) -> Value {
    let hx = |bytes: &[u8]| format!("0x{}", faster_hex::hex_string(bytes));
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
        "extraData": "0x",
        "gasLimit": quantity(b.gas_limit as u128),
        "gasUsed": quantity(b.gas_used as u128),
        "timestamp": quantity(b.timestamp as u128),
        "baseFeePerGas": quantity_from_be32(&b.base_fee_per_gas),
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
            .map(|x| {
                x.as_str().ok_or_else(|| EthRpcError::invalid_params("address array entries must be strings")).and_then(parse_addr20)
            })
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
                .map(|o| {
                    o.as_str().ok_or_else(|| EthRpcError::invalid_params("topic options must be strings")).and_then(parse_topic32)
                })
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
    Ok(Value::Array(logs.iter().map(|l| render_log(l, false)).collect()))
}

/// Render one log object. `removed` is `false` for `eth_getLogs` (canonical) and
/// per-event for the §9 `eth_subscribe("logs")` stream (true for detached logs).
fn render_log(e: &EthLogEntry, removed: bool) -> Value {
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
        "removed": removed,
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
    obj.insert("baseFeePerGas".to_string(), Value::Array(fh.base_fee_per_gas.iter().map(quantity_from_be32).collect()));
    obj.insert("gasUsedRatio".to_string(), json!(fh.gas_used_ratio));
    if let Some(reward) = fh.reward {
        obj.insert(
            "reward".to_string(),
            Value::Array(reward.iter().map(|row| Value::Array(row.iter().map(quantity_from_be32).collect())).collect()),
        );
    }
    Ok(Value::Object(obj))
}

// --- eth_getTransactionByHash (Increment 6: tx index) ---

async fn eth_get_transaction_by_hash(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.transaction_by_hash(hash).await?.map(|t| render_tx(&t)).unwrap_or(Value::Null))
}

/// `debug_traceTransaction(txHash, { tracer })` (§11.1). With no tracer it returns
/// the Geth default opcode/struct logs; `callTracer` returns the call tree and
/// `prestateTracer` the state diff. Any other named tracer is `invalid_params`.
async fn debug_trace_transaction(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    let requested_tracer = params
        .as_array()
        .and_then(|a| a.get(1))
        .filter(|cfg| !cfg.is_null())
        .and_then(|cfg| cfg.get("tracer"))
        .and_then(|t| t.as_str());
    match requested_tracer {
        // Geth-faithful: omitting the tracer yields the opcode/struct logger.
        None => Ok(provider.trace_struct_log(hash).await?.map(|t| render_struct_log(&t)).unwrap_or(Value::Null)),
        Some("callTracer") => Ok(provider.trace_transaction(hash).await?.map(|f| render_call_frame(&f)).unwrap_or(Value::Null)),
        Some("prestateTracer") => Ok(provider.trace_prestate(hash).await?.map(|a| render_prestate(&a)).unwrap_or(Value::Null)),
        Some(other) => Err(EthRpcError::invalid_params(format!(
            "unsupported tracer {other:?}; supported: \"callTracer\", \"prestateTracer\", or omit for the opcode logger"
        ))),
    }
}

/// Render the Geth default struct-logger result (§11.1). `pc/gas/gasCost/depth` are
/// JSON numbers; `stack` entries and `returnValue` are hex WITHOUT a `0x` prefix
/// (Geth convention). Memory/storage are omitted (§11.5 off by default).
fn render_struct_log(t: &EthStructLogTrace) -> Value {
    let hexn = |b: &[u8]| faster_hex::hex_string(b);
    let logs: Vec<Value> = t
        .struct_logs
        .iter()
        .map(|l| {
            let mut m = serde_json::Map::new();
            m.insert("pc".to_string(), json!(l.pc));
            m.insert("op".to_string(), json!(l.op));
            m.insert("gas".to_string(), json!(l.gas));
            m.insert("gasCost".to_string(), json!(l.gas_cost));
            m.insert("depth".to_string(), json!(l.depth));
            m.insert("stack".to_string(), Value::Array(l.stack.iter().map(|w| json!(hexn(w))).collect()));
            if let Some(e) = &l.error {
                m.insert("error".to_string(), json!(e));
            }
            Value::Object(m)
        })
        .collect();
    json!({ "gas": t.gas, "failed": t.failed, "returnValue": hexn(&t.return_value), "structLogs": logs })
}

/// `misaka_traceEvmCandidate(txHash)` (§11.6): diagnose a tx with no receipt.
async fn misaka_trace_evm_candidate(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    Ok(provider.trace_evm_candidate(hash).await?.map(|c| render_candidate_trace(&c)).unwrap_or(Value::Null))
}

/// Render a [`EthCandidateTrace`] (§11.6). `status`/`trace` are present only when
/// the candidate executed; `reason` carries the pre-validation error or the revert
/// reason; `recordedSkipClass` is the historical §6.1 skip class (2/3/5) if any.
fn render_candidate_trace(c: &EthCandidateTrace) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let mut obj = serde_json::Map::new();
    obj.insert("executed".to_string(), json!(c.executed));
    obj.insert("accepted".to_string(), json!(c.accepted));
    obj.insert("status".to_string(), if c.executed { quantity(c.succeeded as u128) } else { Value::Null });
    obj.insert("gasUsed".to_string(), quantity(c.gas_used as u128));
    obj.insert("output".to_string(), json!(hx(&c.output)));
    obj.insert("reason".to_string(), c.reason.as_ref().map(|r| json!(r)).unwrap_or(Value::Null));
    obj.insert("recordedSkipClass".to_string(), c.recorded_skip_class.map(|n| json!(n)).unwrap_or(Value::Null));
    obj.insert("trace".to_string(), c.frame.as_ref().map(render_call_frame).unwrap_or(Value::Null));
    Value::Object(obj)
}

/// Render the `prestateTracer` diffMode result as `{ "pre": {addr: state}, "post":
/// {addr: state} }` (§11.1). An account appears under `pre` unless it was created,
/// and under `post` unless it was self-destructed.
fn render_prestate(accounts: &[EthPrestateAccount]) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let render_state = |s: &EthAccountState| {
        let mut m = serde_json::Map::new();
        m.insert("balance".to_string(), quantity_from_be32(&s.balance));
        m.insert("nonce".to_string(), quantity(s.nonce as u128));
        if !s.code.is_empty() {
            m.insert("code".to_string(), json!(hx(&s.code)));
        }
        if !s.storage.is_empty() {
            let mut st = serde_json::Map::new();
            for (k, v) in &s.storage {
                st.insert(hx(k), json!(hx(v)));
            }
            m.insert("storage".to_string(), Value::Object(st));
        }
        Value::Object(m)
    };
    let mut pre = serde_json::Map::new();
    let mut post = serde_json::Map::new();
    for a in accounts {
        let addr = hx(&a.address);
        if let Some(s) = &a.pre {
            pre.insert(addr.clone(), render_state(s));
        }
        if let Some(s) = &a.post {
            post.insert(addr, render_state(s));
        }
    }
    json!({ "pre": Value::Object(pre), "post": Value::Object(post) })
}

/// `trace_transaction` (Parity/OpenEthereum flat-call format, design §11.1): the
/// SAME replay as `debug_traceTransaction`'s callTracer, flattened into the flat
/// trace list `[{action, result|error, subtraces, traceAddress, type}]`. Reuses
/// `EthProvider::trace_transaction` so it inherits the fence/reconciliation safety.
async fn trace_transaction_flat(provider: &Arc<dyn EthProvider>, params: &Value) -> EthResult<Value> {
    let hash = parse_hash32_param(params, 0)?;
    match provider.trace_transaction(hash).await? {
        Some(frame) => {
            let mut out = Vec::new();
            flatten_call_frame(&frame, Vec::new(), &mut out);
            Ok(Value::Array(out))
        }
        None => Ok(Value::Null),
    }
}

/// Depth-first flatten of a [`EthCallFrame`] tree into Parity flat-trace objects.
/// `trace_address` is the path of child indices from the root (`[]` = root).
fn flatten_call_frame(f: &EthCallFrame, trace_address: Vec<u64>, out: &mut Vec<Value>) {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let is_create = f.call_type == "CREATE" || f.call_type == "CREATE2";
    let mut obj = serde_json::Map::new();

    let mut action = serde_json::Map::new();
    action.insert("from".to_string(), json!(hx(&f.from)));
    action.insert("gas".to_string(), quantity(f.gas as u128));
    action.insert("value".to_string(), quantity_from_be32(&f.value));
    if is_create {
        action.insert("init".to_string(), json!(hx(&f.input)));
        obj.insert("type".to_string(), json!("create"));
    } else {
        action.insert("callType".to_string(), json!(f.call_type.to_lowercase()));
        action.insert("to".to_string(), f.to.map(|a| json!(hx(&a))).unwrap_or(Value::Null));
        action.insert("input".to_string(), json!(hx(&f.input)));
        obj.insert("type".to_string(), json!("call"));
    }
    obj.insert("action".to_string(), Value::Object(action));

    // A failed frame carries `error` and a null `result` (Parity convention).
    if let Some(err) = &f.error {
        obj.insert("error".to_string(), json!(err));
        obj.insert("result".to_string(), Value::Null);
    } else {
        let mut result = serde_json::Map::new();
        result.insert("gasUsed".to_string(), quantity(f.gas_used as u128));
        if is_create {
            result.insert("address".to_string(), f.to.map(|a| json!(hx(&a))).unwrap_or(Value::Null));
            result.insert("code".to_string(), json!(hx(&f.output)));
        } else {
            result.insert("output".to_string(), json!(hx(&f.output)));
        }
        obj.insert("result".to_string(), Value::Object(result));
    }

    obj.insert("subtraces".to_string(), json!(f.calls.len()));
    obj.insert("traceAddress".to_string(), json!(trace_address));
    // MISAKA extensions ride the root object only (set by the provider on root).
    if let Some(b) = &f.misaka_originating_payload_block {
        obj.insert("misakaOriginatingPayloadBlock".to_string(), json!(hx(b)));
    }
    if let Some(b) = &f.misaka_accepting_block {
        obj.insert("misakaAcceptingBlock".to_string(), json!(hx(b)));
    }
    out.push(Value::Object(obj));

    for (i, child) in f.calls.iter().enumerate() {
        let mut child_addr = trace_address.clone();
        child_addr.push(i as u64);
        flatten_call_frame(child, child_addr, out);
    }
}

/// Render a [`EthCallFrame`] tree as Geth `callTracer` JSON (§11.4).
fn render_call_frame(f: &EthCallFrame) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), json!(f.call_type));
    obj.insert("from".to_string(), json!(hx(&f.from)));
    obj.insert("to".to_string(), f.to.map(|a| json!(hx(&a))).unwrap_or(Value::Null));
    obj.insert("value".to_string(), quantity_from_be32(&f.value));
    obj.insert("gas".to_string(), quantity(f.gas as u128));
    obj.insert("gasUsed".to_string(), quantity(f.gas_used as u128));
    obj.insert("input".to_string(), json!(hx(&f.input)));
    obj.insert("output".to_string(), json!(hx(&f.output)));
    obj.insert("error".to_string(), f.error.as_ref().map(|e| json!(e)).unwrap_or(Value::Null));
    obj.insert("revertReason".to_string(), f.revert_reason.as_ref().map(|r| json!(r)).unwrap_or(Value::Null));
    let calls: Vec<Value> = f.calls.iter().map(render_call_frame).collect();
    obj.insert("calls".to_string(), Value::Array(calls));
    if let Some(b) = &f.misaka_originating_payload_block {
        obj.insert("misakaOriginatingPayloadBlock".to_string(), json!(hx(b)));
    }
    if let Some(b) = &f.misaka_accepting_block {
        obj.insert("misakaAcceptingBlock".to_string(), json!(hx(b)));
    }
    Value::Object(obj)
}

/// Render an [`EthTx`] as the standard `eth_getTransactionByHash` JSON. `v/r/s`
/// are not surfaced yet (reads rarely need them); block context is null when pending.
fn render_tx(t: &EthTx) -> Value {
    let hx = |b: &[u8]| format!("0x{}", faster_hex::hex_string(b));
    let mut obj = serde_json::Map::new();
    obj.insert("hash".to_string(), json!(hx(&t.hash)));
    obj.insert("from".to_string(), json!(hx(&t.from)));
    obj.insert("to".to_string(), t.to.map(|a| json!(hx(&a))).unwrap_or(Value::Null));
    obj.insert("nonce".to_string(), quantity(t.nonce as u128));
    obj.insert("value".to_string(), quantity_from_be32(&t.value));
    obj.insert("gas".to_string(), quantity(t.gas as u128));
    obj.insert("gasPrice".to_string(), quantity(t.gas_price));
    obj.insert("maxFeePerGas".to_string(), quantity(t.gas_price));
    obj.insert("maxPriorityFeePerGas".to_string(), t.max_priority_fee_per_gas.map(quantity).unwrap_or(Value::Null));
    obj.insert("input".to_string(), json!(hx(&t.input)));
    obj.insert("type".to_string(), quantity(t.tx_type as u128));
    obj.insert("chainId".to_string(), t.chain_id.map(|c| quantity(c as u128)).unwrap_or(Value::Null));
    obj.insert("blockNumber".to_string(), t.block_number.map(|n| quantity(n as u128)).unwrap_or(Value::Null));
    obj.insert("blockHash".to_string(), t.block_hash.map(|h| json!(hx(&h))).unwrap_or(Value::Null));
    obj.insert("transactionIndex".to_string(), t.tx_index.map(|i| quantity(i as u128)).unwrap_or(Value::Null));
    // Signature components (audit R-3): real values, no longer 0x0 placeholders.
    obj.insert("v".to_string(), quantity(t.v as u128));
    obj.insert("r".to_string(), quantity_from_be32(&t.r));
    obj.insert("s".to_string(), quantity_from_be32(&t.s));
    // Typed (EIP-2930/1559) txs surface yParity + accessList; legacy omits them.
    if t.tx_type >= 1 {
        obj.insert("yParity".to_string(), quantity(t.y_parity as u128));
        let al: Vec<Value> = t
            .access_list
            .iter()
            .map(|e| {
                let keys: Vec<Value> = e.storage_keys.iter().map(|k| json!(hx(k))).collect();
                json!({ "address": hx(&e.address), "storageKeys": keys })
            })
            .collect();
        obj.insert("accessList".to_string(), Value::Array(al));
    }
    Value::Object(obj)
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
    if !s.len().is_multiple_of(2) {
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
                return Some(err_value(
                    codes::INVALID_REQUEST,
                    &format!("batch too large: {} items > {MAX_BATCH_ITEMS} max", items.len()),
                ));
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
///
/// This is the never-shutting-down form, kept for callers that own the whole
/// process lifetime (e.g. [`spawn`]). Hosts that need a graceful stop — the
/// node's `AsyncRuntime` service — must use [`serve_with_shutdown`] instead, or
/// the accept loop below blocks forever in `listener.accept()` and wedges the
/// runtime's shutdown join (the kaspad `--evm-rpc-listen` shutdown deadlock).
pub async fn serve(addr: SocketAddr, provider: Arc<dyn EthProvider>) -> std::io::Result<()> {
    serve_with_shutdown(addr, provider, std::future::pending::<()>()).await
}

/// Serve the Ethereum JSON-RPC endpoint on `addr` until `shutdown` resolves.
///
/// When `shutdown` fires, the accept loop stops taking new connections and this
/// future returns `Ok(())`, so a host service's `start()` future can complete
/// and the node can finish shutting down. Connections already accepted run as
/// detached tasks and are NOT drained here — they are force-closed when the
/// tokio runtime is dropped at process exit. That is sufficient to break the
/// shutdown deadlock; a fuller graceful drain of in-flight HTTP/WebSocket
/// connections would be a separate stage.
pub async fn serve_with_shutdown<F>(addr: SocketAddr, provider: Arc<dyn EthProvider>, shutdown: F) -> std::io::Result<()>
where
    F: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind(addr).await?;
    kaspa_core::info!("[eth-rpc] Ethereum JSON-RPC listening on http://{addr}");
    let conn_sem = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tokio::pin!(shutdown);
    loop {
        let (stream, _peer) = tokio::select! {
            // Prefer the shutdown branch when both are ready, so a flood of
            // inbound connections can't starve the stop signal.
            biased;
            _ = &mut shutdown => {
                kaspa_core::info!("[eth-rpc] shutdown received, stopping accept loop on {addr}");
                break;
            }
            accepted = listener.accept() => accepted?,
        };
        // At capacity: drop the new connection immediately (no unbounded spawn).
        let Ok(permit) = conn_sem.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let provider = provider.clone();
        tokio::spawn(async move {
            // The permit is held for the WHOLE connection (HTTP one-shot OR a
            // long-lived WebSocket) so WS conns still count against MAX_CONNECTIONS.
            // The CONN_TIMEOUT is applied per-phase INSIDE serve_conn (header read,
            // then the HTTP exchange) rather than as a blanket deadline here — a
            // blanket deadline would kill every WebSocket at CONN_TIMEOUT (§9).
            let _permit = permit; // released on task end
            if let Err(e) = serve_conn(stream, provider).await {
                kaspa_core::trace!("[eth-rpc] connection error: {e}");
            }
        });
    }
    Ok(())
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

/// Handle ONE connection. Reads the request head (slowloris-bounded by
/// `CONN_TIMEOUT`), then either upgrades to a long-lived §9 WebSocket
/// ([`ws::serve_ws`], NOT under a blanket deadline) or serves a single HTTP/1.1
/// JSON-RPC exchange (`Connection: close` — no keep-alive; clients reconnect).
async fn serve_conn(mut stream: TcpStream, provider: Arc<dyn EthProvider>) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];

    // Phase 1 — read the request head (up to CRLFCRLF), bounded by CONN_TIMEOUT so
    // a slowloris that never finishes its headers is dropped here, not WS-upgraded.
    let header_end = {
        let read_head = async {
            loop {
                if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                    return Ok::<HeaderRead, std::io::Error>(HeaderRead::Done(pos + 4));
                }
                if buf.len() > MAX_BODY_BYTES {
                    return Ok(HeaderRead::TooLarge);
                }
                let n = stream.read(&mut tmp).await?;
                if n == 0 {
                    return Ok(HeaderRead::Eof); // client closed before sending headers
                }
                buf.extend_from_slice(&tmp[..n]);
            }
        };
        match tokio::time::timeout(CONN_TIMEOUT, read_head).await {
            Ok(Ok(HeaderRead::Done(pos))) => pos,
            Ok(Ok(HeaderRead::TooLarge)) => return write_response(&mut stream, 431, "Request Header Fields Too Large", "").await,
            Ok(Ok(HeaderRead::Eof)) => return Ok(()),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Ok(()), // header-phase timeout → drop
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();

    // §9 WebSocket upgrade: a `GET` with `Upgrade: websocket` + a key. Long-lived
    // — runs without a blanket deadline (its own ping keepalive provides liveness),
    // and the connection permit (held by the caller) stays held for its lifetime.
    if let Some(ws_key) = ws::upgrade_key(&head) {
        let leftover = buf[header_end..].to_vec();
        return ws::serve_ws(stream, provider, &ws_key, leftover).await;
    }

    // Phase 2 — a single HTTP JSON-RPC exchange, bounded by CONN_TIMEOUT.
    let exchange = async move {
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
                if k.trim().eq_ignore_ascii_case("content-length") { v.trim().parse::<usize>().ok() } else { None }
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
    };
    match tokio::time::timeout(CONN_TIMEOUT, exchange).await {
        Ok(r) => r,
        Err(_) => Ok(()), // HTTP exchange (body read / dispatch / write) timed out → drop
    }
}

/// Outcome of the [`serve_conn`] header-read phase.
enum HeaderRead {
    /// Header block complete; the value is the byte offset just past CRLFCRLF.
    Done(usize),
    /// Header block exceeded `MAX_BODY_BYTES` before completing.
    TooLarge,
    /// Peer closed before sending a complete header block.
    Eof,
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

#[cfg(test)]
mod trace_flatten_tests {
    use super::*;

    fn frame(call_type: &str, calls: Vec<EthCallFrame>) -> EthCallFrame {
        EthCallFrame {
            call_type: call_type.to_string(),
            from: [0x11; 20],
            to: Some([0x22; 20]),
            value: [0u8; 32],
            gas: 0x100,
            gas_used: 0x40,
            input: vec![0xab],
            output: vec![0xcd],
            error: None,
            revert_reason: None,
            calls,
            misaka_originating_payload_block: None,
            misaka_accepting_block: None,
        }
    }

    /// The flat adapter assigns traceAddress paths + subtraces over a DFS of the tree.
    #[test]
    fn flatten_assigns_trace_address_and_subtraces() {
        // root -> [child0, child1]; child0 -> [grandchild]
        let tree = frame("CALL", vec![frame("STATICCALL", vec![frame("CALL", vec![])]), frame("DELEGATECALL", vec![])]);
        let mut out = Vec::new();
        flatten_call_frame(&tree, Vec::new(), &mut out);
        assert_eq!(out.len(), 4, "root + 2 children + 1 grandchild");
        // DFS order: root, child0, grandchild, child1.
        assert_eq!(out[0]["traceAddress"], json!([] as [u64; 0]));
        assert_eq!(out[0]["subtraces"], json!(2));
        assert_eq!(out[0]["type"], json!("call"));
        assert_eq!(out[1]["traceAddress"], json!([0]));
        assert_eq!(out[1]["action"]["callType"], json!("staticcall"));
        assert_eq!(out[1]["subtraces"], json!(1));
        assert_eq!(out[2]["traceAddress"], json!([0, 0]));
        assert_eq!(out[3]["traceAddress"], json!([1]));
        assert_eq!(out[3]["action"]["callType"], json!("delegatecall"));
        assert_eq!(out[3]["subtraces"], json!(0));
    }

    /// CREATE frames use the create action/result shape; failed frames carry `error`.
    #[test]
    fn flatten_create_and_error_shapes() {
        let mut create = frame("CREATE2", vec![]);
        let mut out = Vec::new();
        flatten_call_frame(&create, Vec::new(), &mut out);
        assert_eq!(out[0]["type"], json!("create"));
        assert!(out[0]["action"].get("init").is_some());
        assert!(out[0]["result"].get("address").is_some());
        assert!(out[0]["action"].get("callType").is_none(), "create has no callType");

        create.error = Some("execution reverted".to_string());
        let mut out2 = Vec::new();
        flatten_call_frame(&create, Vec::new(), &mut out2);
        assert_eq!(out2[0]["error"], json!("execution reverted"));
        assert_eq!(out2[0]["result"], Value::Null);
    }
}

#[cfg(test)]
mod block_id_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tag_and_number_strings() {
        assert_eq!(parse_block_id_value(&json!("latest")).unwrap(), BlockId::Tag("latest".into()));
        assert_eq!(parse_block_id_value(&json!("finalized")).unwrap(), BlockId::Tag("finalized".into()));
        assert_eq!(parse_block_id_value(&json!("0x10")).unwrap(), BlockId::Number(16));
    }

    #[test]
    fn parses_eip1898_block_number_object() {
        assert_eq!(parse_block_id_value(&json!({"blockNumber": "0x2a"})).unwrap(), BlockId::Number(42));
        assert_eq!(parse_block_id_value(&json!({"blockNumber": "latest"})).unwrap(), BlockId::Tag("latest".into()));
    }

    #[test]
    fn parses_eip1898_block_hash_object() {
        let h = format!("0x{}", "11".repeat(32));
        assert_eq!(
            parse_block_id_value(&json!({"blockHash": h, "requireCanonical": true})).unwrap(),
            BlockId::Hash { hash: [0x11; 32], require_canonical: true }
        );
        // requireCanonical defaults to false when omitted.
        let h2 = format!("0x{}", "22".repeat(32));
        assert_eq!(
            parse_block_id_value(&json!({"blockHash": h2})).unwrap(),
            BlockId::Hash { hash: [0x22; 32], require_canonical: false }
        );
    }

    #[test]
    fn rejects_bad_eip1898() {
        assert!(parse_block_id_value(&json!({"requireCanonical": true})).is_err(), "object without blockNumber/blockHash");
        assert!(parse_block_id_value(&json!({"blockHash": "0xdead"})).is_err(), "short blockHash");
        assert!(parse_block_id_value(&json!(42)).is_err(), "bare number (not a string)");
    }

    /// §12.5: the eth_call/eth_estimateGas block param at params[1] defaults to
    /// `latest` when absent/null and otherwise parses tags / numbers / EIP-1898.
    #[test]
    fn exec_block_param_defaults_and_parses() {
        // params with no [1] → latest.
        assert_eq!(exec_block_param(&json!([{"to": "0x00"}]), 1).unwrap(), BlockId::Tag("latest".into()));
        // explicit null → latest.
        assert_eq!(exec_block_param(&json!([{"to": "0x00"}, null]), 1).unwrap(), BlockId::Tag("latest".into()));
        // a historical number is honored (no longer rejected).
        assert_eq!(exec_block_param(&json!([{"to": "0x00"}, "0x10"]), 1).unwrap(), BlockId::Number(16));
        // EIP-1898 object form.
        assert_eq!(exec_block_param(&json!([{"to": "0x00"}, {"blockNumber": "0x2a"}]), 1).unwrap(), BlockId::Number(42));
    }
}
