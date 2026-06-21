//! kaspa-pq EVM Lane (ADR-0020 §16): node-side wiring for the Ethereum
//! JSON-RPC adapter crate (`kaspa-eth-rpc`). Compiled ONLY under `--features
//! evm` — the thin adapter crate links no revm, and this node-side
//! [`EthProvider`] implementation (the only place that touches consensus EVM
//! state) is gated here, so the default secp-free node never pulls it.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use kaspa_consensus_core::evm::EVM_CHAIN_ID;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_eth_rpc::{
    EthBlock, EthCallRequest, EthEvmTxStatus, EthFeeHistory, EthLog, EthLogEntry, EthProvider, EthReceipt, EthResult, EthRpcError, EthTx,
};
use kaspa_hashes::EvmH256;
use kaspa_p2p_flows::flow_context::FlowContext;

const ETH_RPC: &str = "eth-rpc";

/// [`EthProvider`] over the node's consensus stores + the EVM mempool (the
/// mempool seam powers `eth_sendRawTransaction`).
pub struct NodeEthProvider {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    client_version: String,
}

impl NodeEthProvider {
    pub fn new(consensus_manager: Arc<ConsensusManager>, flow_context: Arc<FlowContext>) -> Self {
        Self { consensus_manager, flow_context, client_version: format!("misaka-kaspad/v{}", env!("CARGO_PKG_VERSION")) }
    }
}

#[async_trait]
impl EthProvider for NodeEthProvider {
    fn chain_id(&self) -> u64 {
        EVM_CHAIN_ID
    }

    fn client_version(&self) -> String {
        self.client_version.clone()
    }

    async fn block_number(&self) -> EthResult<u64> {
        let session = self.consensus_manager.consensus().session().await;
        // Store read → spawn_blocking (do not block the async executor on RocksDB).
        let header = session
            .spawn_blocking(|c| c.get_evm_head_header())
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(header.map(|h| h.evm_number).unwrap_or(0))
    }

    async fn is_syncing(&self) -> bool {
        // The endpoint only serves once the node is up; report ready (Ethereum
        // tooling treats `false` as "synced"). Refined later if needed.
        false
    }

    async fn gas_price(&self) -> EthResult<u128> {
        // MVP: a fixed suggested price (1 gwei). Refined to the head base-fee
        // once the U256→u128 read is wired (Increment 3+).
        Ok(1_000_000_000)
    }

    async fn latest_account(&self, address: [u8; 20]) -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
        let session = self.consensus_manager.consensus().session().await;
        // Read the canonical head's EVM state snapshot (spawn_blocking — RocksDB).
        let snapshot = session
            .spawn_blocking(|c| {
                let sink = c.get_sink();
                c.get_evm_state_snapshot_of(sink)
            })
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        let target = kaspa_consensus_core::evm::EvmAddress::from_bytes(address);
        Ok(snapshot.and_then(|s| s.accounts.into_iter().find(|a| a.address == target)))
    }

    async fn eth_call(&self, req: EthCallRequest) -> EthResult<Vec<u8>> {
        let (snapshot, env) = self.head_snapshot_and_env().await?;
        let call = to_sim_call(&req);
        // revm execution is CPU-bound → spawn_blocking.
        let outcome = tokio::task::spawn_blocking(move || kaspa_evm::sim::simulate_call(&snapshot, &env, &call))
            .await
            .map_err(|e| EthRpcError::server(format!("eth_call task: {e}")))?
            .map_err(|e| EthRpcError::server(format!("eth_call: {e}")))?;
        if outcome.success {
            Ok(outcome.output)
        } else {
            // Ethereum convention: code 3 "execution reverted", revert data in the message.
            Err(EthRpcError::new(3, format!("execution reverted: 0x{}", faster_hex::hex_string(&outcome.output))))
        }
    }

    async fn estimate_gas(&self, req: EthCallRequest) -> EthResult<u64> {
        let (snapshot, env) = self.head_snapshot_and_env().await?;
        let call = to_sim_call(&req);
        tokio::task::spawn_blocking(move || kaspa_evm::sim::estimate_gas(&snapshot, &env, &call))
            .await
            .map_err(|e| EthRpcError::server(format!("estimate_gas task: {e}")))?
            .map_err(|e| EthRpcError::server(format!("estimate_gas: {e}")))
    }

    async fn send_raw_transaction(&self, raw: Vec<u8>) -> EthResult<[u8; 32]> {
        use kaspa_mining::evm_mempool::EvmMempoolError;
        // Route through the flow_context so the tx is BOTH admitted to this node's
        // EVM mempool AND P2P-broadcast to EVM-relay peers (§14.2) — the same path
        // the UTXO RPC submit uses. Without the broadcast, a tx sent to a
        // non-mining node would never reach a miner. The class-1 admission rule
        // (decode / signer / chain-id / gas band) runs inside; a non-evm node refuses.
        match self.flow_context.submit_rpc_evm_transaction(raw).await {
            Ok(h) => Ok(h.as_bytes()),
            // Idempotent: a duplicate submit returns the already-pending hash,
            // so a retrying wallet still gets its tx id back.
            Err(EvmMempoolError::Duplicate(h)) => Ok(h.as_bytes()),
            Err(e) => Err(EthRpcError::new(-32000, format!("evm tx rejected: {e:?}"))),
        }
    }

    async fn transaction_receipt(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthReceipt>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(tx_hash);
        // One spawn_blocking: the receipt view + (locate + decode) the raw tx for
        // from/to/contractAddress/type (RocksDB reads + k256 recovery are blocking).
        let (view, decoded) = session
            .spawn_blocking(move |c| {
                let view = c.get_evm_tx_receipt(h).ok().flatten();
                let decoded = (|| {
                    let locs = c.get_evm_tx_locations(h).ok()?;
                    for block in locs.included_in {
                        if let Ok(payload) = c.get_block_evm_payload(block) {
                            for raw in &payload.transactions {
                                if kaspa_evm::tx::tx_hash(raw) == h {
                                    return kaspa_evm::tx::decode_eth_tx(raw).ok();
                                }
                            }
                        }
                    }
                    None
                })();
                (view, decoded)
            })
            .await;
        Ok(view.map(|v| {
            // The accepting L1 block hash is 64 bytes (BLAKE2b-512); expose the
            // leading 32 as a standard-shaped, client-opaque `blockHash`.
            let bh = v.accepting_block.as_bytes();
            let mut block_hash = [0u8; 32];
            block_hash.copy_from_slice(&bh[..32]);
            EthReceipt {
                tx_hash,
                status: v.receipt.succeeded,
                block_number: v.evm_number,
                block_hash,
                tx_index: v.receipt_index,
                gas_used: v.receipt.gas_used,
                cumulative_gas_used: v.receipt.cumulative_gas_used,
                logs: v
                    .receipt
                    .logs
                    .iter()
                    .map(|lg| EthLog {
                        address: lg.address.as_bytes(),
                        topics: lg.topics.iter().map(|t| t.as_bytes()).collect(),
                        data: lg.data.clone(),
                    })
                    .collect(),
                from: decoded.as_ref().map(|d| d.from),
                to: decoded.as_ref().and_then(|d| d.to),
                contract_address: decoded.as_ref().and_then(|d| d.contract_address),
                tx_type: decoded.as_ref().map(|d| d.tx_type).unwrap_or(0),
                effective_gas_price: decoded.as_ref().map(|d| d.max_fee_per_gas).unwrap_or(0),
            }
        }))
    }

    /// `misaka_getEvmTxStatus`: the full EVM-lane lifecycle of a tx by hash —
    /// the visibility the §6.1 gas-pool work needs (`pending`/`included`/
    /// `accepted`/`skipped`), since a tx skipped (class 2/5) in one chain
    /// block's canonical order is retried and accepted once the order shifts.
    async fn evm_tx_status(&self, tx_hash: [u8; 32]) -> EthResult<EthEvmTxStatus> {
        let h = EvmH256::from_bytes(tx_hash);
        // Mempool view (in-memory, no DB): is the tx still active on this node,
        // plus its decoded metadata while pending.
        let pending_raw = self.flow_context.mining_manager().get_evm_transaction_raw(&h);
        let in_mempool = pending_raw.is_some();
        let (sender, nonce, gas_limit) = match pending_raw.as_ref().and_then(|raw| kaspa_evm::tx::admit_tx_info(raw).ok()) {
            Some(info) => (Some(info.sender.as_bytes()), Some(info.nonce), Some(info.gas_limit)),
            None => (None, None, None),
        };
        // Acceptance/inclusion view (RocksDB read): the §6.1 location index.
        let session = self.consensus_manager.consensus().session().await;
        let locs = session.spawn_blocking(move |c| c.get_evm_tx_locations(h).ok()).await;
        let (included_in, accepted_in, last_skip_class) = match locs {
            Some(l) => {
                // The L1 block ids are 64-byte (BLAKE2b-512); expose the leading
                // 32 as standard-shaped, client-opaque ids (as elsewhere here).
                let to32 = |b: &kaspa_hashes::Hash64| {
                    let mut x = [0u8; 32];
                    x.copy_from_slice(&b.as_bytes()[..32]);
                    x
                };
                let included = l.included_in.iter().map(to32).collect();
                let accepted = l.accepted_in.first().map(|(b, idx)| (to32(b), *idx));
                (included, accepted, l.last_skip_class)
            }
            None => (Vec::new(), None, None),
        };
        // Priority: accepted (mined) ▸ pending (in pool, will retry) ▸ included
        // (in a payload, acceptance pending) ▸ skipped (last seen skipped, gone
        // from the pool) ▸ unknown.
        let state = if accepted_in.is_some() {
            "accepted"
        } else if in_mempool {
            "pending"
        } else if !included_in.is_empty() {
            "included"
        } else if last_skip_class.is_some() {
            "skipped"
        } else {
            "unknown"
        };
        Ok(EthEvmTxStatus { tx_hash, state, included_in, accepted_in, last_skip_class, in_mempool, sender, nonce, gas_limit })
    }

    async fn transaction_by_hash(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthTx>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(tx_hash);
        let (decoded, ctx) = session
            .spawn_blocking(move |c| {
                let decoded = (|| {
                    let locs = c.get_evm_tx_locations(h).ok()?;
                    for block in locs.included_in {
                        if let Ok(payload) = c.get_block_evm_payload(block) {
                            for raw in &payload.transactions {
                                if kaspa_evm::tx::tx_hash(raw) == h {
                                    return kaspa_evm::tx::decode_eth_tx(raw).ok();
                                }
                            }
                        }
                    }
                    None
                })();
                // Canonical block context (None ⇒ pending / not on the selected chain).
                let ctx = c.get_evm_tx_receipt(h).ok().flatten().map(|v| {
                    let bh = v.accepting_block.as_bytes();
                    let mut block_hash = [0u8; 32];
                    block_hash.copy_from_slice(&bh[..32]);
                    (v.evm_number, block_hash, v.receipt_index)
                });
                (decoded, ctx)
            })
            .await;
        Ok(decoded.map(|d| EthTx {
            hash: tx_hash,
            from: d.from,
            to: d.to,
            nonce: d.nonce,
            value: d.value,
            gas: d.gas_limit,
            gas_price: d.max_fee_per_gas,
            max_priority_fee_per_gas: d.max_priority_fee_per_gas,
            input: d.input,
            tx_type: d.tx_type,
            chain_id: d.chain_id,
            block_number: ctx.map(|(n, _, _)| n),
            block_hash: ctx.map(|(_, b, _)| b),
            tx_index: ctx.map(|(_, _, i)| i),
        }))
    }

    async fn block_by_number(&self, number: u64) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let resp = session
            .spawn_blocking(move |c| c.get_evm_block_by_number(number))
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(to_eth_block))
    }

    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let tag = tag.to_string();
        // latest/pending/safe/finalized all map to the current sink (the DAG's
        // fast finality makes them equivalent for the MVP); earliest = number 0.
        let resp = session
            .spawn_blocking(move |c| match tag.as_str() {
                "earliest" => c.get_evm_block_by_number(0),
                _ => c.get_evm_block_by_l1_hash(c.get_sink()),
            })
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(to_eth_block))
    }

    async fn block_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(hash);
        let resp = session
            .spawn_blocking(move |c| c.get_evm_block_by_rpc_hash(h))
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(to_eth_block))
    }

    async fn get_logs(
        &self,
        from: u64,
        to: u64,
        addresses: Vec<[u8; 20]>,
        topics: Vec<Vec<[u8; 32]>>,
    ) -> EthResult<Vec<EthLogEntry>> {
        let session = self.consensus_manager.consensus().session().await;
        let addrs: Vec<_> = addresses.into_iter().map(kaspa_consensus_core::evm::EvmAddress::from_bytes).collect();
        let tpcs: Vec<Vec<_>> = topics.into_iter().map(|p| p.into_iter().map(EvmH256::from_bytes).collect()).collect();
        let entries = session
            .spawn_blocking(move |c| c.get_evm_logs(from, to, addrs, tpcs))
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(entries.into_iter().map(to_eth_log_entry).collect())
    }

    async fn fee_history(&self, block_count: u64, newest: u64, reward_percentiles: Vec<f64>) -> EthResult<EthFeeHistory> {
        let session = self.consensus_manager.consensus().session().await;
        let np = reward_percentiles.len();
        let (base_fees, ratios, oldest) = session
            .spawn_blocking(move |c| {
                let oldest = newest.saturating_sub(block_count.saturating_sub(1));
                let head_base =
                    c.get_evm_head_header().ok().flatten().map(|h| h.base_fee_per_gas.to_be_bytes()).unwrap_or([0u8; 32]);
                let mut base_fees: Vec<[u8; 32]> = Vec::new();
                let mut ratios: Vec<f64> = Vec::new();
                for n in oldest..=newest {
                    match c.get_evm_block_by_number(n).ok().flatten() {
                        Some(b) => {
                            let gl = b.header.gas_limit.max(1);
                            base_fees.push(b.header.base_fee_per_gas.to_be_bytes());
                            ratios.push(b.header.gas_used as f64 / gl as f64);
                        }
                        None => {
                            base_fees.push(head_base);
                            ratios.push(0.0);
                        }
                    }
                }
                // The trailing +1 entry is the next block's projected base fee (flat).
                let projection = *base_fees.last().unwrap_or(&head_base);
                base_fees.push(projection);
                (base_fees, ratios, oldest)
            })
            .await;
        // No separate priority-fee market yet → zero reward at every requested percentile.
        let reward = if np > 0 { Some(ratios.iter().map(|_| vec![[0u8; 32]; np]).collect()) } else { None };
        Ok(EthFeeHistory { oldest_block: oldest, base_fee_per_gas: base_fees, gas_used_ratio: ratios, reward })
    }
}

impl NodeEthProvider {
    /// Fetch the canonical-head EVM state snapshot + the call env (one spawn_blocking).
    async fn head_snapshot_and_env(
        &self,
    ) -> EthResult<(kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_evm::sim::EthCallEnv)> {
        let session = self.consensus_manager.consensus().session().await;
        let (snap, header) = session
            .spawn_blocking(|c| {
                let sink = c.get_sink();
                (c.get_evm_state_snapshot_of(sink), c.get_evm_head_header())
            })
            .await;
        let snap = snap.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?.unwrap_or_default();
        let header = header.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        let env = kaspa_evm::sim::EthCallEnv {
            chain_id: EVM_CHAIN_ID,
            number: header.as_ref().map(|h| h.evm_number).unwrap_or(0),
            timestamp: header.as_ref().map(|h| h.evm_timestamp_sec).unwrap_or(0),
            coinbase: header.as_ref().map(|h| h.coinbase).unwrap_or_default(),
            gas_limit: header.as_ref().map(|h| h.gas_limit).unwrap_or(30_000_000),
        };
        Ok((snap, env))
    }
}

/// Map a consensus `EvmBlockResponse` to the adapter's primitive [`EthBlock`].
/// The 32-byte `hash` is the first 32 bytes of the 64-byte L1 hash (the same id
/// the receipt exposes as `blockHash`). `parent_hash` is left zero for now — a
/// correct parentHash (the `evm_number − 1` block id) lands with the full-tx /
/// `eth_getTransactionByHash` increment.
fn to_eth_block(resp: kaspa_consensus_core::evm::EvmBlockResponse) -> EthBlock {
    let h = &resp.header;
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&resp.l1_hash.as_bytes()[..32]);
    EthBlock {
        number: h.evm_number,
        hash,
        parent_hash: [0u8; 32],
        state_root: h.state_root.as_bytes(),
        transactions_root: h.transactions_root.as_bytes(),
        receipts_root: h.receipts_root.as_bytes(),
        logs_bloom: h.logs_bloom.as_bytes().to_vec(),
        timestamp: h.evm_timestamp_sec,
        gas_used: h.gas_used,
        gas_limit: h.gas_limit,
        base_fee_per_gas: h.base_fee_per_gas.to_be_bytes(),
        miner: h.coinbase.as_bytes(),
        tx_hashes: resp.tx_hashes.iter().map(|t| t.as_bytes()).collect(),
    }
}

/// Map a consensus `EvmLogEntry` to the adapter's primitive [`EthLogEntry`].
fn to_eth_log_entry(e: kaspa_consensus_core::evm::EvmLogEntry) -> EthLogEntry {
    let mut block_hash = [0u8; 32];
    block_hash.copy_from_slice(&e.block_l1_hash.as_bytes()[..32]);
    EthLogEntry {
        address: e.address.as_bytes(),
        topics: e.topics.iter().map(|t| t.as_bytes()).collect(),
        data: e.data,
        block_number: e.block_number,
        block_hash,
        tx_hash: e.tx_hash.as_bytes(),
        tx_index: e.tx_index,
        log_index: e.log_index,
    }
}

/// Convert a parsed RPC call request into the kaspa-evm simulation input.
fn to_sim_call(req: &EthCallRequest) -> kaspa_evm::sim::EthCall {
    kaspa_evm::sim::EthCall {
        from: kaspa_consensus_core::evm::EvmAddress::from_bytes(req.from),
        to: req.to.map(kaspa_consensus_core::evm::EvmAddress::from_bytes),
        value: kaspa_consensus_core::evm::EvmU256::from_be_bytes(req.value),
        data: req.data.clone(),
        gas_limit: req.gas,
    }
}

/// [`AsyncService`] that runs the Ethereum JSON-RPC HTTP server on the node's
/// async runtime (registered beside the other services in `daemon.rs`).
pub struct EthRpcService {
    addr: SocketAddr,
    provider: Arc<dyn EthProvider>,
}

impl EthRpcService {
    pub fn new(addr: SocketAddr, consensus_manager: Arc<ConsensusManager>, flow_context: Arc<FlowContext>) -> Self {
        Self { addr, provider: Arc::new(NodeEthProvider::new(consensus_manager, flow_context)) }
    }
}

impl AsyncService for EthRpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        ETH_RPC
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            if let Err(e) = kaspa_eth_rpc::serve(self.addr, self.provider.clone()).await {
                kaspa_core::warn!("[{ETH_RPC}] server on {} exited: {e}", self.addr);
            }
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        kaspa_core::trace!("sending an exit signal to {}", ETH_RPC);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            kaspa_core::trace!("{} stopped", ETH_RPC);
            Ok(())
        })
    }
}
