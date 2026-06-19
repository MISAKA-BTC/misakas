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
use kaspa_eth_rpc::{EthCallRequest, EthProvider, EthResult, EthRpcError};

const ETH_RPC: &str = "eth-rpc";

/// [`EthProvider`] over the node's consensus stores + (later) the EVM mempool.
pub struct NodeEthProvider {
    consensus_manager: Arc<ConsensusManager>,
    client_version: String,
}

impl NodeEthProvider {
    pub fn new(consensus_manager: Arc<ConsensusManager>) -> Self {
        Self { consensus_manager, client_version: format!("misaka-kaspad/v{}", env!("CARGO_PKG_VERSION")) }
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
    pub fn new(addr: SocketAddr, consensus_manager: Arc<ConsensusManager>) -> Self {
        Self { addr, provider: Arc::new(NodeEthProvider::new(consensus_manager)) }
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
