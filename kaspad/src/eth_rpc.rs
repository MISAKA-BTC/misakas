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
    BlockId, EthAccessListItem, EthAccountState, EthBlock, EthCallFrame, EthCallRequest, EthCandidateTrace, EthEvmTxStatus, EthFeeHistory,
    EthLog, EthLogEntry, EthLogEvent, EthPrestateAccount, EthProvider, EthReceipt, EthResult, EthRpcError, EthStructLog, EthStructLogTrace,
    EthTx,
};
use kaspa_hashes::EvmH256;
use kaspa_p2p_flows::flow_context::FlowContext;
// §9 newHeads: tap the consensus VirtualChainChanged notification.
use kaspa_consensus_notify::{notification::Notification, notifier::ConsensusNotifier};
use kaspa_notify::{
    connection::{ChannelConnection, ChannelType},
    listener::ListenerLifespan,
    scope::{Scope, VirtualChainChangedScope},
};
use tokio::sync::broadcast;

const ETH_RPC: &str = "eth-rpc";

/// §9 newHeads: bound on each subscriber's head backlog. Heads are infrequent
/// (one per virtual resolve) so a small cap suffices; a subscriber that falls
/// this far behind gets `Lagged` (drop-oldest) and reconnects + backfills.
const EVM_HEADS_CHANNEL_CAP: usize = 256;

/// §9 logs: bound on each subscriber's log backlog (higher than heads — a block
/// can carry many logs). Over-lag is `Lagged` (drop-oldest) → reconnect + backfill.
const EVM_LOGS_CHANNEL_CAP: usize = 4096;

/// §11.5: max concurrent `debug_traceTransaction` replays (each is CPU-bound — a
/// full re-execution of the accepting block up to the target tx plus an inspected
/// run). The public profile keeps this small; raise it for a private trace node.
const MAX_CONCURRENT_TRACES: usize = 2;

/// [`EthProvider`] over the node's consensus stores + the EVM mempool (the
/// mempool seam powers `eth_sendRawTransaction`).
pub struct NodeEthProvider {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    /// §9 newHeads: the consensus notifier the head pump registers a
    /// VirtualChainChanged listener on.
    consensus_notifier: Arc<ConsensusNotifier>,
    /// §9 newHeads: fan-out of canonical EVM block headers to WS subscribers
    /// (fed by [`Self::spawn_head_pump`]); `subscribe_new_heads` clones a receiver.
    heads_tx: broadcast::Sender<EthBlock>,
    /// §9 logs: fan-out of reorg-ordered log events to WS subscribers (detached
    /// removed=true oldest-first, then attached removed=false). Unfiltered — the
    /// WS layer applies each subscription's address/topic filter.
    logs_tx: broadcast::Sender<EthLogEvent>,
    /// §11.5: bounds concurrent `debug_traceTransaction` replays.
    trace_semaphore: Arc<tokio::sync::Semaphore>,
    client_version: String,
}

impl NodeEthProvider {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        flow_context: Arc<FlowContext>,
        consensus_notifier: Arc<ConsensusNotifier>,
    ) -> Self {
        let (heads_tx, _) = broadcast::channel(EVM_HEADS_CHANNEL_CAP);
        let (logs_tx, _) = broadcast::channel(EVM_LOGS_CHANNEL_CAP);
        Self {
            consensus_manager,
            flow_context,
            consensus_notifier,
            heads_tx,
            logs_tx,
            trace_semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_TRACES)),
            client_version: format!("misaka-kaspad/v{}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// §9 (`eth_subscribe("newHeads")`): register a consensus VirtualChainChanged
    /// listener and pump each newly ADDED selected-chain block that has an EVM
    /// header out to `heads_tx` as a header (non-EVM blocks are skipped), in commit
    /// order. Spawned once at service start. All store work is skipped while no
    /// client is subscribed (`receiver_count == 0`).
    pub fn spawn_head_pump(self: Arc<Self>) {
        let (tx, rx) = async_channel::unbounded();
        let listener_id = self
            .consensus_notifier
            .register_new_listener(ChannelConnection::new(ETH_RPC, tx, ChannelType::Closable), ListenerLifespan::Dynamic);
        if let Err(e) =
            self.consensus_notifier.try_start_notify(listener_id, Scope::VirtualChainChanged(VirtualChainChangedScope::new(false)))
        {
            kaspa_core::warn!("[{ETH_RPC}] newHeads: failed to subscribe to VirtualChainChanged: {e:?}");
            return;
        }
        tokio::spawn(async move {
            while let Ok(notification) = rx.recv().await {
                let Notification::VirtualChainChanged(vcc) = notification else { continue };
                // Gate each event independently — skip all store work when nobody
                // is subscribed to it.
                let want_heads = self.heads_tx.receiver_count() > 0;
                let want_logs = self.logs_tx.receiver_count() > 0;
                if !want_heads && !want_logs {
                    continue;
                }
                // Each hash is an L1 BlockHash; everything below reads by HASH
                // (immutable once committed) — never the reorg-mutable number map.
                let added: Vec<_> = vcc.added_chain_block_hashes.iter().copied().collect();
                let removed: Vec<_> = vcc.removed_chain_block_hashes.iter().copied().collect();
                let session = self.consensus_manager.consensus().session().await;
                let (heads, log_events): (Vec<EthBlock>, Vec<EthLogEvent>) = session
                    .spawn_blocking(move |c| {
                        // newHeads: each added EVM block's header, in commit order
                        // (added is oldest-first); non-EVM blocks read as None.
                        let heads: Vec<EthBlock> = if want_heads {
                            added
                                .iter()
                                .copied()
                                .filter_map(|l1| {
                                    let resp = c.get_evm_block_by_l1_hash(l1);
                                    let parent = parent_l1_hash32(c, &resp);
                                    match resp {
                                        Ok(Some(r)) => Some(to_eth_block(r, parent)),
                                        _ => None,
                                    }
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        // logs: detached (removed=true, oldest-first) then attached
                        // (removed=false, oldest-first) — the reorg contract lives in
                        // the unit-tested `reorg_ordered`.
                        let log_events: Vec<EthLogEvent> = if want_logs {
                            reorg_ordered(&removed, &added, |l1| c.get_evm_block_logs(l1).unwrap_or_default())
                                .into_iter()
                                .map(|(e, removed)| EthLogEvent { log: to_eth_log_entry(e), removed })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        (heads, log_events)
                    })
                    .await;
                // Lossy under lag — a slow WS subscriber drops events and reconnects
                // + backfills (design R-5); the sends never block the pump.
                for head in heads {
                    let _ = self.heads_tx.send(head);
                }
                for ev in log_events {
                    let _ = self.logs_tx.send(ev);
                }
            }
        });
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
        // Honest sync state (audit M-08): true while the node is still catching up
        // (so tooling doesn't treat a not-yet-synced node as ready), false once
        // nearly synced. (The full Ethereum object form {startingBlock,…} is a
        // later refinement; a truthy value already signals "not ready".)
        let session = self.consensus_manager.consensus().session().await;
        !self.flow_context.is_nearly_synced(&session).await
    }

    async fn gas_price(&self) -> EthResult<u128> {
        // Suggest the live head base fee (audit M-08) — a wallet that pays this as
        // maxFeePerGas is accepted at the current 1559 floor. Falls back to the
        // genesis initial base fee if the head header is briefly unavailable.
        use kaspa_consensus_core::evm::EVM_INITIAL_BASE_FEE;
        let session = self.consensus_manager.consensus().session().await;
        let header = session
            .spawn_blocking(|c| c.get_evm_head_header())
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(header.and_then(|h| h.base_fee_per_gas.try_to_u128()).unwrap_or(EVM_INITIAL_BASE_FEE as u128))
    }

    async fn latest_account(&self, address: [u8; 20]) -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
        use kaspa_consensus_core::evm::FlatHeadAccount;
        let session = self.consensus_manager.consensus().session().await;
        session
            .spawn_blocking(move |c| -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
                let target = kaspa_consensus_core::evm::EvmAddress::from_bytes(address);
                // C-01 S7 (audit H-03): O(1) flat point-lookup at the head. Falls back to the
                // authoritative full-snapshot scan when the flat store is not at the head
                // (shadow backend off / mid-rebase). Both paths return the identical account.
                if let Ok(FlatHeadAccount::AtHead(acct)) = c.get_evm_flat_account_at_head(target) {
                    return Ok(acct);
                }
                let snapshot =
                    c.get_evm_state_snapshot_of(c.get_sink()).map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
                Ok(snapshot.and_then(|s| s.accounts.into_iter().find(|a| a.address == target)))
            })
            .await
    }

    async fn pending_nonce(&self, address: [u8; 20]) -> EthResult<u64> {
        // Chain (accepted) nonce + this node's contiguous pending EVM txs (M-08).
        let state_nonce = self.latest_account(address).await?.map(|a| a.nonce).unwrap_or(0);
        let sender = kaspa_consensus_core::evm::EvmAddress::from_bytes(address);
        Ok(self.flow_context.mining_manager().evm_next_pending_nonce(sender, state_nonce))
    }

    async fn account_at(&self, address: [u8; 20], block: BlockId) -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
        // Resolve the selector → L1 block hash → that block's EVM snapshot (H-04).
        // safe/finalized read the canonical heads (a non-reorgable height); a
        // numeric/earliest tag reads the historical block. Fail CLOSED if the
        // resolved block has no snapshot (pruned / pre-activation) so a caller
        // never silently gets latest state for a historical query.
        let session = self.consensus_manager.consensus().session().await;
        session
            .spawn_blocking(move |c| -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
                let target = kaspa_consensus_core::evm::EvmAddress::from_bytes(address);
                let l1: Option<kaspa_consensus_core::BlockHash> = match &block {
                    BlockId::Number(n) => c.get_evm_block_by_number(*n).ok().flatten().map(|b| b.l1_hash),
                    BlockId::Tag(t) => match t.as_str() {
                        "earliest" => c.get_evm_block_by_number(0).ok().flatten().map(|b| b.l1_hash),
                        "safe" => c.get_evm_canonical_heads().ok().flatten().map(|h| h.safe),
                        "finalized" => c.get_evm_canonical_heads().ok().flatten().map(|h| h.finalized),
                        _ => Some(c.get_sink()), // latest / pending
                    },
                    // EIP-1898 by hash: resolve the 32-byte eth id → its L1 block. With
                    // requireCanonical, a side-branch (non-chain) block is an explicit
                    // error rather than silently reading its state (§12.5).
                    BlockId::Hash { hash, require_canonical } => {
                        let rpc_hash = kaspa_hashes::EvmH256::from_bytes(*hash);
                        match c.get_evm_block_by_rpc_hash(rpc_hash).ok().flatten() {
                            Some(b) => {
                                if *require_canonical && !c.is_chain_block(b.l1_hash).unwrap_or(false) {
                                    return Err(EthRpcError::invalid_params(
                                        "requireCanonical: the requested block is not on the canonical chain",
                                    ));
                                }
                                Some(b.l1_hash)
                            }
                            None => None,
                        }
                    }
                };
                // Selector did not resolve to a known block (e.g. a future number) ⇒ no account.
                let Some(l1) = l1 else { return Ok(None) };
                // C-01 S7 (audit H-03): when the resolved block IS the canonical head, answer from
                // the O(1) flat point-lookup instead of materializing the full state. `Stale` (flat
                // store not at the head) falls through to the authoritative path below.
                if l1 == c.get_sink()
                    && let Ok(kaspa_consensus_core::evm::FlatHeadAccount::AtHead(acct)) = c.get_evm_flat_account_at_head(target)
                {
                    return Ok(acct);
                }
                // Hot path: the full snapshot is still in the reorg window (prefix 206).
                // §12: past the window the snapshot is pruned but the state is
                // reconstructable from the checkpoint/diff history — fall back to that.
                // Fail CLOSED throughout so a historical query never silently reads
                // latest (or empty) state.
                let snapshot = match c.get_evm_state_snapshot_of(l1) {
                    Ok(Some(snap)) => Some(snap),
                    Ok(None) | Err(_) => match c.reconstruct_evm_state_at(l1) {
                        // Reconstructed + state-root-verified historical state (§12.4).
                        Ok(Some(snap)) => Some(snap),
                        // `l1` is not an EVM block (pre-activation): all accounts are the
                        // empty genesis state ⇒ no entry for this address.
                        Ok(None) => None,
                        // EVM block but its history is not retained here, or is corrupt.
                        Err(e) => return Err(EthRpcError::server(format!("EVM state unavailable at the requested block: {e}"))),
                    },
                };
                Ok(snapshot.and_then(|snap| snap.accounts.into_iter().find(|a| a.address == target)))
            })
            .await
    }

    async fn eth_call(&self, req: EthCallRequest, block: BlockId) -> EthResult<Vec<u8>> {
        let (snapshot, env) = self.snapshot_and_env_at(block).await?;
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

    async fn estimate_gas(&self, req: EthCallRequest, block: BlockId) -> EthResult<u64> {
        let (snapshot, env) = self.snapshot_and_env_at(block).await?;
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
        let (view, decoded, accepting_base_fee) = session
            .spawn_blocking(move |c| {
                let view = c.get_evm_tx_receipt(h).ok().flatten();
                // The accepting block's base fee, for the real effectiveGasPrice (M-08).
                let base_fee = view
                    .as_ref()
                    .and_then(|v| c.get_evm_header_of(v.accepting_block).ok().flatten())
                    .and_then(|hdr| hdr.base_fee_per_gas.try_to_u128());
                // audit R-2: resolve the raw tx directly by hash (no included_in scan).
                let decoded = c.get_evm_raw_tx(h).ok().flatten().and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok());
                (view, decoded, base_fee)
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
                log_index_offset: v.log_index_offset,
                logs_bloom: kaspa_evm::roots::receipt_logs_bloom(&v.receipt),
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
                // Real EIP-1559 effective price (M-08): base_fee + min(tip, max_fee −
                // base_fee), where the accepting block supplies base_fee. Falls back
                // to max_fee if the base fee is unavailable (legacy/unknown).
                effective_gas_price: {
                    let max_fee = decoded.as_ref().map(|d| d.max_fee_per_gas).unwrap_or(0);
                    let tip = decoded.as_ref().and_then(|d| d.max_priority_fee_per_gas).unwrap_or(max_fee);
                    match accepting_base_fee {
                        Some(bf) => bf.saturating_add(tip.min(max_fee.saturating_sub(bf))),
                        None => max_fee,
                    }
                },
            }
        }))
    }

    /// `debug_traceTransaction` with the Geth `callTracer` (design §11): the call
    /// tree of the replay, with the §11.4 MISAKA extensions on the root frame.
    async fn trace_transaction(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthCallFrame>> {
        Ok(self.replay_trace(tx_hash, false).await?.map(|(traced, accepting, originating)| {
            let mut root = convert_call_frame(&traced.frame);
            root.misaka_originating_payload_block = originating.map(|h| h.as_bytes().to_vec());
            root.misaka_accepting_block = Some(accepting.as_bytes().to_vec());
            root
        }))
    }

    /// `debug_traceTransaction` with the `prestateTracer` (diffMode, §11.1): the
    /// per-account pre/post state diff — the SAME replay as the callTracer.
    async fn trace_prestate(&self, tx_hash: [u8; 32]) -> EthResult<Option<Vec<EthPrestateAccount>>> {
        Ok(self.replay_trace(tx_hash, false).await?.map(|(traced, _accepting, _originating)| {
            traced.prestate.iter().map(convert_prestate_account).collect()
        }))
    }

    /// `debug_traceTransaction` with no tracer = the Geth opcode/struct logger
    /// (§11.1): the SAME replay as the callTracer, with per-opcode logs captured.
    async fn trace_struct_log(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthStructLogTrace>> {
        Ok(self.replay_trace(tx_hash, true).await?.map(|(traced, _accepting, _originating)| EthStructLogTrace {
            gas: traced.gas_used,
            failed: !traced.succeeded,
            return_value: traced.output.to_vec(),
            struct_logs: traced.struct_logs.unwrap_or_default().iter().map(convert_struct_log).collect(),
        }))
    }

    /// `misaka_traceEvmCandidate` (§11.6): diagnose a tx with no receipt by replaying
    /// it against the current head. The raw tx comes from the mempool (pending) or
    /// the raw-tx store (was carried in a payload). Reports the head replay outcome
    /// plus the recorded historical skip class and whether it is now accepted.
    async fn trace_evm_candidate(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthCandidateTrace>> {
        let h = EvmH256::from_bytes(tx_hash);
        // Mempool raw (pending) — in-memory, no consensus session.
        let pending_raw = self.flow_context.mining_manager().get_evm_transaction_raw(&h);
        // Head pre-state + env (reuses the eth_call head context; fee-free, F003 per
        // the eth_call convention).
        let (snapshot, env) = self.head_snapshot_and_env().await?;
        // §11.5: cap concurrent replays; the permit is held across the blocking work.
        let permit = self
            .trace_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EthRpcError::server("too many concurrent traces in flight; retry shortly"))?;
        let session = self.consensus_manager.consensus().session().await;
        session
            .spawn_blocking(move |c| {
                let _permit = permit;
                // Raw tx: mempool (pending) first, else the raw-tx store. Unknown ⇒ None.
                let raw = match pending_raw {
                    Some(r) => r,
                    None => match c.get_evm_raw_tx(h).ok().flatten() {
                        Some(r) => r,
                        None => return Ok(None),
                    },
                };
                let recorded_skip_class = c.get_evm_tx_locations(h).ok().and_then(|l| l.last_skip_class);
                let accepted = c.get_evm_tx_receipt(h).ok().flatten().is_some();
                let ct = kaspa_evm::trace::trace_candidate_tx(&snapshot, &env, &raw, kaspa_evm::trace::TraceLimits::default())
                    .map_err(|e| EthRpcError::server(format!("{e}")))?;
                Ok(Some(EthCandidateTrace {
                    executed: ct.executed,
                    succeeded: ct.succeeded,
                    gas_used: ct.gas_used,
                    output: ct.output.to_vec(),
                    reason: ct.reason,
                    recorded_skip_class,
                    accepted,
                    frame: ct.frame.as_ref().map(convert_call_frame),
                }))
            })
            .await
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
        // Acceptance/inclusion view (RocksDB read): the §6.1 location index PLUS
        // the CANONICAL receipt. The location index's `accepted_in` may include
        // side branches (per its type contract); `get_evm_tx_receipt` resolves the
        // selected-chain acceptance and is the ONLY source of truth for "accepted"
        // (audit H-06 — a reorged-out acceptance must never read as accepted).
        let session = self.consensus_manager.consensus().session().await;
        let (locs, canon) = session
            .spawn_blocking(move |c| (c.get_evm_tx_locations(h).ok(), c.get_evm_tx_receipt(h).ok().flatten()))
            .await;
        // The L1 block ids are 64-byte (BLAKE2b-512); expose the leading 32 as
        // standard-shaped, client-opaque ids (as elsewhere here).
        let to32 = |b: &kaspa_hashes::Hash64| {
            let mut x = [0u8; 32];
            x.copy_from_slice(&b.as_bytes()[..32]);
            x
        };
        let canonical_accepted = canon.as_ref().map(|v| (to32(&v.accepting_block), v.receipt_index));
        let (included_in, all_accepted, last_skip_class) = match &locs {
            Some(l) => (l.included_in.iter().map(to32).collect::<Vec<_>>(), l.accepted_in.iter().map(|(b, idx)| (to32(b), *idx)).collect::<Vec<_>>(), l.last_skip_class),
            None => (Vec::new(), Vec::new(), None),
        };
        // Acceptances in the index that are NOT the canonical one = orphaned (side
        // branch). Diagnostic only; never drives `accepted`.
        let orphaned_acceptances: Vec<_> = all_accepted.iter().copied().filter(|a| Some(*a) != canonical_accepted).collect();
        // Priority: canonical-accepted (mined on the selected chain) ▸ pending (in
        // pool, will retry) ▸ included (in a payload, acceptance pending) ▸
        // orphaned (accepted only on a since-reorged branch) ▸ skipped ▸ unknown.
        let state = if canonical_accepted.is_some() {
            "accepted"
        } else if in_mempool {
            "pending"
        } else if !included_in.is_empty() {
            "included"
        } else if !orphaned_acceptances.is_empty() {
            "orphaned"
        } else if last_skip_class.is_some() {
            "skipped"
        } else {
            "unknown"
        };
        Ok(EthEvmTxStatus {
            tx_hash,
            state,
            included_in,
            accepted_in: canonical_accepted,
            orphaned_acceptances,
            last_skip_class,
            in_mempool,
            sender,
            nonce,
            gas_limit,
        })
    }

    async fn transaction_by_hash(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthTx>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(tx_hash);
        let (decoded, ctx) = session
            .spawn_blocking(move |c| {
                // audit R-2: resolve the raw tx directly by hash (survives the
                // bounded included_in cap + pruning of the payload's location row).
                let decoded = c.get_evm_raw_tx(h).ok().flatten().and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok());
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
        // Pending fallback (audit M-08): a tx that is in this node's mempool but
        // not yet in any payload is decoded from the pool, with null block context.
        let decoded = match decoded {
            Some(d) => Some(d),
            None => self
                .flow_context
                .mining_manager()
                .get_evm_transaction_raw(&h)
                .and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok()),
        };
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
            v: d.v,
            r: d.r,
            s: d.s,
            y_parity: d.y_parity,
            access_list: d
                .access_list
                .into_iter()
                .map(|(address, storage_keys)| EthAccessListItem { address, storage_keys })
                .collect(),
        }))
    }

    async fn block_by_number(&self, number: u64) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let (resp, parent) = session
            .spawn_blocking(move |c| {
                let resp = c.get_evm_block_by_number(number);
                let parent = parent_l1_hash32(c, &resp);
                (resp, parent)
            })
            .await;
        let resp = resp.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(|r| to_eth_block(r, parent)))
    }

    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let tag = tag.to_string();
        // Resolve each tag to its real L1 block (audit H-04): latest/pending = the
        // sink; safe/finalized = the canonical heads (finalized lags = the pruning
        // point, NOT the sink, so a "finalized" query is non-reorgable); earliest =
        // number 0. safe/finalized fall back to the sink only if heads are unset.
        let (resp, parent) = session
            .spawn_blocking(move |c| {
                let heads = c.get_evm_canonical_heads().ok().flatten();
                let resp = match tag.as_str() {
                    "earliest" => c.get_evm_block_by_number(0),
                    "safe" => match heads {
                        Some(hd) => c.get_evm_block_by_l1_hash(hd.safe),
                        None => c.get_evm_block_by_l1_hash(c.get_sink()),
                    },
                    "finalized" => match heads {
                        Some(hd) => c.get_evm_block_by_l1_hash(hd.finalized),
                        None => c.get_evm_block_by_l1_hash(c.get_sink()),
                    },
                    _ => c.get_evm_block_by_l1_hash(c.get_sink()), // latest / pending
                };
                let parent = parent_l1_hash32(c, &resp);
                (resp, parent)
            })
            .await;
        let resp = resp.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(|r| to_eth_block(r, parent)))
    }

    async fn block_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(hash);
        let (resp, parent) = session
            .spawn_blocking(move |c| {
                let resp = c.get_evm_block_by_rpc_hash(h);
                let parent = parent_l1_hash32(c, &resp);
                (resp, parent)
            })
            .await;
        let resp = resp.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(resp.map(|r| to_eth_block(r, parent)))
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

    /// §9 (`eth_subscribe("newPendingTransactions")`): bridge the mining manager's
    /// EVM admission broadcast (native `EvmH256`, §9 slice 1) into the adapter's
    /// `[u8;32]` hash stream. One bridge task per subscription, scoped to the
    /// subscriber: when the WebSocket forwarder drops its receiver, our `send`
    /// finds no receivers and the bridge ends (it flushes on the next admission —
    /// a parked task until then, never a leak). No global pump and no node startup
    /// hook — the task is spawned lazily inside the RPC runtime when a client
    /// subscribes, the same runtime that serves the socket.
    fn subscribe_pending_txs(&self) -> broadcast::Receiver<[u8; 32]> {
        let mut admit_rx = self.flow_context.mining_manager().evm_tx_admission_receiver();
        let (tx, rx) = broadcast::channel::<[u8; 32]>(4096);
        tokio::spawn(async move {
            loop {
                match admit_rx.recv().await {
                    Ok(hash) => {
                        if tx.send(hash.as_bytes()).is_err() {
                            break; // no subscribers left → stop bridging
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        rx
    }

    /// §9 (`eth_subscribe("newHeads")`): hand out a receiver on the head fan-out
    /// fed by [`Self::spawn_head_pump`] (registered once at service start).
    fn subscribe_new_heads(&self) -> broadcast::Receiver<EthBlock> {
        self.heads_tx.subscribe()
    }

    /// §9 (`eth_subscribe("logs")`): hand out a receiver on the reorg-ordered log
    /// fan-out fed by [`Self::spawn_head_pump`] (unfiltered; the WS layer filters).
    fn subscribe_logs(&self) -> broadcast::Receiver<EthLogEvent> {
        self.logs_tx.subscribe()
    }
}

impl NodeEthProvider {
    /// §11 shared replay: resolve the accepted tx → its accepting block's replay
    /// plan → the selected-parent committed pre-state, replay it once with the
    /// call-frame inspector under the network's real activation fences, and return
    /// the [`kaspa_evm::trace::TracedTx`] (callTracer frame + prestateTracer diff)
    /// plus the accepting block and originating payload block. Both `trace_*`
    /// methods drive off this so a single replay serves either tracer.
    #[allow(clippy::type_complexity)]
    async fn replay_trace(
        &self,
        tx_hash: [u8; 32],
        capture_struct_logs: bool,
    ) -> EthResult<Option<(kaspa_evm::trace::TracedTx, kaspa_consensus_core::BlockHash, Option<kaspa_consensus_core::BlockHash>)>> {
        // §11.5: bound concurrent replays. The permit is MOVED INTO the blocking
        // closure so it is released only when the (uncancellable) replay finishes —
        // not when the awaiting connection times out and drops this future.
        let permit = self
            .trace_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EthRpcError::server("too many concurrent traces in flight; retry shortly"))?;
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(tx_hash);
        session
            .spawn_blocking(move |c| {
                let _permit = permit; // held until the blocking replay completes
                // Resolve the tx to its canonical accepting block + receipt. `None` ⇒
                // unknown / not accepted on the selected chain (skipped txs report a
                // reason via misaka_getEvmTxStatus, §11.6).
                let Some(view) = c.get_evm_tx_receipt(h).ok().flatten() else { return Ok(None) };
                let accepting = view.accepting_block;
                let Some(body) = c.get_evm_trace_replay_body(accepting).ok().flatten() else {
                    return Err(EthRpcError::server(
                        "trace data unavailable for this transaction's block (pruned or recorded before trace support)",
                    ));
                };
                // Selected-parent committed post-state = the replay PRE-state. The
                // parent's EVM header gates snapshot-vs-genesis-default; a header-
                // present-but-snapshot-absent parent is pruned ⇒ unavailable (§11.5).
                let parent_header = c.get_evm_header_of(body.selected_parent).ok().flatten();
                let parent_snapshot = if parent_header.is_some() {
                    match c.get_evm_state_snapshot_of(body.selected_parent).ok().flatten() {
                        Some(s) => s,
                        None => {
                            return Err(EthRpcError::server(
                                "historical state unavailable for trace (selected-parent state snapshot pruned)",
                            ))
                        }
                    }
                } else {
                    Default::default()
                };
                // Replay under the SAME activation fences the accepting block executed
                // with (gas-pool v1/v2, withdraw-cap, F003) — read from the network
                // Params, not assumed inert (testnet runs a finite gas-pool-v2 fence).
                let (gas_pool_v2, f002_withdraw_cap, f003_mldsa_verify) = c.evm_activation_fences();
                let traced = kaspa_evm::trace::trace_accepted_tx(
                    &parent_snapshot,
                    parent_header.as_ref(),
                    &body,
                    view.receipt_index,
                    &view.receipt,
                    gas_pool_v2,
                    f002_withdraw_cap,
                    f003_mldsa_verify,
                    capture_struct_logs,
                    kaspa_evm::trace::TraceLimits::default(),
                )
                .map_err(|e| EthRpcError::server(format!("{e}")))?;
                let originating =
                    kaspa_evm::trace::candidate_index_for_receipt(&body, view.receipt_index).map(|i| body.txs[i].originating_payload_block);
                Ok(Some((traced, accepting, originating)))
            })
            .await
    }

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
        let snap = snap.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        let header = header.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        // Fail CLOSED on a missing snapshot (audit H-03): only a true pre-activation
        // genesis (no head header at all) may legitimately have empty state. A head
        // that exists but whose snapshot is absent means the state is unavailable —
        // simulating against an empty (default) snapshot would return a bogus
        // "success on empty chain" for eth_call / eth_estimateGas.
        let snap = match (snap, header.as_ref()) {
            (Some(s), _) => s,
            (None, None) => Default::default(),
            (None, Some(_)) => {
                return Err(EthRpcError::server(
                    "EVM state snapshot unavailable for head; refusing to simulate against empty state",
                ))
            }
        };
        let env = kaspa_evm::sim::EthCallEnv {
            chain_id: EVM_CHAIN_ID,
            number: header.as_ref().map(|h| h.evm_number).unwrap_or(0),
            timestamp: header.as_ref().map(|h| h.evm_timestamp_sec).unwrap_or(0),
            coinbase: header.as_ref().map(|h| h.coinbase).unwrap_or_default(),
            gas_limit: header.as_ref().map(|h| h.gas_limit).unwrap_or(30_000_000),
            // PREA P0-1: F003 is fence-inert (u64::MAX) on every network, so the
            // simulator registers no F003 handler — identical to the executor below
            // the fence. Wire to (head_daa_score >= activation) when F003 ships.
            f003_active: false,
        };
        Ok((snap, env))
    }

    /// The EVM state snapshot + call env at `block` (§12.5/§12.6). `latest`/`pending`
    /// use the head fast path; any historical selector resolves the block, builds
    /// the env from THAT block's header (number/timestamp/coinbase/gas limit/chain
    /// id), and supplies its state — the hot snapshot (prefix 206) if still in the
    /// reorg window, else the §12 checkpoint/diff reconstruction. Fails CLOSED: a
    /// historical call never silently runs against head or empty state.
    async fn snapshot_and_env_at(
        &self,
        block: BlockId,
    ) -> EthResult<(kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_evm::sim::EthCallEnv)> {
        if matches!(&block, BlockId::Tag(t) if matches!(t.as_str(), "latest" | "pending")) {
            return self.head_snapshot_and_env().await;
        }
        let session = self.consensus_manager.consensus().session().await;
        session
            .spawn_blocking(move |c| -> EthResult<(kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_evm::sim::EthCallEnv)> {
                // Resolve the selector → L1 block (same resolution as account_at).
                let l1: Option<kaspa_consensus_core::BlockHash> = match &block {
                    BlockId::Number(n) => c.get_evm_block_by_number(*n).ok().flatten().map(|b| b.l1_hash),
                    BlockId::Tag(t) => match t.as_str() {
                        "earliest" => c.get_evm_block_by_number(0).ok().flatten().map(|b| b.l1_hash),
                        "safe" => c.get_evm_canonical_heads().ok().flatten().map(|h| h.safe),
                        "finalized" => c.get_evm_canonical_heads().ok().flatten().map(|h| h.finalized),
                        _ => Some(c.get_sink()),
                    },
                    BlockId::Hash { hash, require_canonical } => {
                        let rpc_hash = kaspa_hashes::EvmH256::from_bytes(*hash);
                        match c.get_evm_block_by_rpc_hash(rpc_hash).ok().flatten() {
                            Some(b) => {
                                if *require_canonical && !c.is_chain_block(b.l1_hash).unwrap_or(false) {
                                    return Err(EthRpcError::invalid_params(
                                        "requireCanonical: the requested block is not on the canonical chain",
                                    ));
                                }
                                Some(b.l1_hash)
                            }
                            None => None,
                        }
                    }
                };
                let Some(l1) = l1 else {
                    return Err(EthRpcError::invalid_params("eth_call/eth_estimateGas: the requested block is unknown"));
                };
                // §12.6 env from the TARGET block's committed header.
                let header = match c.get_evm_header_of(l1) {
                    Ok(Some(h)) => h,
                    Ok(None) => return Err(EthRpcError::server("eth_call/eth_estimateGas: the requested block is not an EVM block")),
                    Err(e) => return Err(EthRpcError::server(format!("consensus: {e:?}"))),
                };
                // State: hot snapshot, else §12 reconstruction past the window.
                let snapshot = match c.get_evm_state_snapshot_of(l1) {
                    Ok(Some(s)) => s,
                    Ok(None) | Err(_) => match c.reconstruct_evm_state_at(l1) {
                        Ok(Some(s)) => s,
                        Ok(None) => return Err(EthRpcError::server("eth_call/eth_estimateGas: state unavailable (not an EVM block)")),
                        Err(e) => return Err(EthRpcError::server(format!("EVM state unavailable at the requested block: {e}"))),
                    },
                };
                let env = kaspa_evm::sim::EthCallEnv {
                    chain_id: EVM_CHAIN_ID,
                    number: header.evm_number,
                    timestamp: header.evm_timestamp_sec,
                    coinbase: header.coinbase,
                    gas_limit: header.gas_limit,
                    f003_active: false,
                };
                Ok((snapshot, env))
            })
            .await
    }
}

/// Convert an executor [`kaspa_evm::trace::CallFrame`] into the adapter's primitive
/// [`EthCallFrame`] (recursively). MISAKA extensions are left `None`; the caller
/// sets them on the root frame.
fn convert_call_frame(f: &kaspa_evm::trace::CallFrame) -> EthCallFrame {
    EthCallFrame {
        call_type: f.kind.as_str().to_string(),
        from: f.from.into_array(),
        to: f.to.map(|a| a.into_array()),
        value: f.value.to_be_bytes::<32>(),
        gas: f.gas,
        gas_used: f.gas_used,
        input: f.input.to_vec(),
        output: f.output.to_vec(),
        error: f.error.clone(),
        revert_reason: f.revert_reason.clone(),
        calls: f.calls.iter().map(convert_call_frame).collect(),
        misaka_originating_payload_block: None,
        misaka_accepting_block: None,
    }
}

/// Convert a [`kaspa_evm::trace::PrestateAccount`] into the adapter's primitive
/// [`EthPrestateAccount`] (prestateTracer diffMode wire shape).
fn convert_prestate_account(a: &kaspa_evm::trace::PrestateAccount) -> EthPrestateAccount {
    let conv = |s: &kaspa_evm::trace::AccountStateView| EthAccountState {
        balance: s.balance.to_be_bytes::<32>(),
        nonce: s.nonce,
        code: s.code.to_vec(),
        storage: s.storage.iter().map(|(k, v)| (k.to_be_bytes::<32>(), v.to_be_bytes::<32>())).collect(),
    };
    EthPrestateAccount { address: a.address.into_array(), pre: a.pre.as_ref().map(conv), post: a.post.as_ref().map(conv) }
}

/// Convert a [`kaspa_evm::trace::StructLog`] into the adapter's primitive [`EthStructLog`].
fn convert_struct_log(l: &kaspa_evm::trace::StructLog) -> EthStructLog {
    EthStructLog {
        pc: l.pc,
        op: l.op_name.to_string(),
        gas: l.gas,
        gas_cost: l.gas_cost,
        depth: l.depth,
        stack: l.stack.clone(),
        error: l.error.clone(),
    }
}

/// The eth-rpc 32-byte parentHash of an EVM block: the first 32 bytes of the
/// `evm_number − 1` block's L1 hash (audit H-04). Zero for EVM block 0 or when
/// the parent cannot be read (a non-fatal best-effort field).
fn parent_l1_hash32(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    resp: &kaspa_consensus_core::errors::consensus::ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>>,
) -> [u8; 32] {
    let Ok(Some(r)) = resp else { return [0u8; 32] };
    if r.header.evm_number == 0 {
        return [0u8; 32];
    }
    match c.get_evm_block_by_number(r.header.evm_number - 1) {
        Ok(Some(p)) => {
            let mut h = [0u8; 32];
            h.copy_from_slice(&p.l1_hash.as_bytes()[..32]);
            h
        }
        _ => [0u8; 32],
    }
}

/// Map a consensus `EvmBlockResponse` to the adapter's primitive [`EthBlock`].
/// The 32-byte `hash` is the first 32 bytes of the 64-byte L1 hash (the same id
/// the receipt exposes as `blockHash`); `parent_hash` is the `evm_number − 1`
/// block's id (audit H-04), computed by [`parent_l1_hash32`].
fn to_eth_block(resp: kaspa_consensus_core::evm::EvmBlockResponse, parent_hash: [u8; 32]) -> EthBlock {
    let h = &resp.header;
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&resp.l1_hash.as_bytes()[..32]);
    EthBlock {
        number: h.evm_number,
        hash,
        parent_hash,
        state_root: h.state_root.as_bytes(),
        // `transactionsRoot` is a standard Ethereum keccak256 ordered trie.
        // `receiptsRoot` is a MISAKA-CUSTOM commitment (Borsh EvmReceipt root),
        // NOT the standard typed-receipt RLP trie — standard receipt-trie proofs
        // will NOT verify against it. See docs/evm-differences-from-ethereum.md
        // (audit M-02). Switching to standard RLP is a fenced consensus change.
        transactions_root: h.transactions_root.as_bytes(),
        receipts_root: h.receipts_root.as_bytes(),
        logs_bloom: h.logs_bloom.as_bytes().to_vec(),
        timestamp: h.evm_timestamp_sec,
        gas_used: h.gas_used,
        gas_limit: h.gas_limit,
        base_fee_per_gas: h.base_fee_per_gas.to_be_bytes(),
        miner: h.coinbase.as_bytes(),
        tx_hashes: resp.tx_hashes.iter().map(|t| t.as_bytes()).collect(),
        size: resp.encoded_size,
    }
}

/// §9 logs reorg ordering — the one semantic that MUST be exact. Emits
/// `(item, removed)` pairs as DETACHED blocks first (`removed = true`) oldest-first
/// (`removed_chain_block_hashes` is newest-first — chain_path walks backward from
/// the old sink — so it is REVERSED here), THEN ATTACHED blocks (`removed = false`)
/// oldest-first (`added_chain_block_hashes` is already oldest-first). Items within
/// a block keep their order (logIndex). Generic + side-effect-free so the ordering
/// contract is unit-tested in isolation from the consensus stores.
fn reorg_ordered<H: Copy, T>(removed: &[H], added: &[H], mut read: impl FnMut(H) -> Vec<T>) -> Vec<(T, bool)> {
    let mut out = Vec::new();
    for h in removed.iter().rev().copied() {
        out.extend(read(h).into_iter().map(|t| (t, true)));
    }
    for h in added.iter().copied() {
        out.extend(read(h).into_iter().map(|t| (t, false)));
    }
    out
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
    // Concrete (not `dyn`) so `start` can spawn the §9 newHeads pump on it before
    // handing it to `serve` as the `EthProvider`.
    provider: Arc<NodeEthProvider>,
}

impl EthRpcService {
    pub fn new(
        addr: SocketAddr,
        consensus_manager: Arc<ConsensusManager>,
        flow_context: Arc<FlowContext>,
        consensus_notifier: Arc<ConsensusNotifier>,
    ) -> Self {
        Self { addr, provider: Arc::new(NodeEthProvider::new(consensus_manager, flow_context, consensus_notifier)) }
    }
}

impl AsyncService for EthRpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        ETH_RPC
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            // §9: register the newHeads pump (VirtualChainChanged → EthBlock fan-out)
            // before serving, so a client that subscribes right after connect sees
            // live heads.
            self.provider.clone().spawn_head_pump();
            let provider: Arc<dyn EthProvider> = self.provider.clone();
            if let Err(e) = kaspa_eth_rpc::serve(self.addr, provider).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// §9 slice 5 GATE — the reorg emission contract, isolated from the stores:
    /// detached blocks first (`removed = true`) OLDEST-first (the removed list is
    /// newest-first → reversed), then attached blocks (`removed = false`)
    /// oldest-first; items within a block keep their order. This pins the one hard
    /// semantic — a silent inversion here breaks every downstream log indexer.
    #[test]
    fn reorg_ordered_emits_detached_then_attached_oldest_first() {
        // removed is newest-first: 'C' detached most recently, then 'B' (older).
        let removed = [b'C', b'B'];
        // added is oldest-first: 'D' then 'E'.
        let added = [b'D', b'E'];
        // Each block yields two items (logIndex 0,1) to check intra-block order.
        let pairs = reorg_ordered(&removed, &added, |h| vec![(h, 0u8), (h, 1u8)]);
        let seq: Vec<(char, u8, bool)> = pairs.into_iter().map(|((h, i), removed)| (h as char, i, removed)).collect();
        assert_eq!(
            seq,
            vec![
                ('B', 0, true),  // detached, OLDEST (B) before the more-recent (C)
                ('B', 1, true),
                ('C', 0, true),
                ('C', 1, true),
                ('D', 0, false), // attached, oldest (D) first
                ('D', 1, false),
                ('E', 0, false),
                ('E', 1, false),
            ]
        );
    }

    /// A linear extension (no reorg) emits only attached events, removed=false.
    #[test]
    fn reorg_ordered_linear_extension() {
        let pairs = reorg_ordered::<u8, u8>(&[], &[b'A'], |_| vec![1, 2]);
        assert_eq!(pairs, vec![(1, false), (2, false)]);
    }
}
