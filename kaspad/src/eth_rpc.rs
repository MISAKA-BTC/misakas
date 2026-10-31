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
    BlockId, EthAccessListItem, EthAccountState, EthBlock, EthCallFrame, EthCallRequest, EthCandidateTrace, EthEvmTxStatus,
    EthFeeHistory, EthLog, EthLogEntry, EthLogEvent, EthPrestateAccount, EthProvider, EthReceipt, EthResult, EthRpcError,
    EthStructLog, EthStructLogTrace, EthTx,
};
use kaspa_hashes::EvmH256;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_utils::triggers::SingleTrigger;
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
    pub fn spawn_head_pump(self: Arc<Self>, shutdown: kaspa_utils::triggers::Listener) {
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
            loop {
                let notification = tokio::select! {
                    biased;
                    _ = shutdown.clone() => break,
                    notification = rx.recv() => match notification { Ok(n) => n, Err(_) => break },
                };
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
                let log_added = added.clone();
                let heads = if want_heads {
                    retry_reorg_logs_v2(
                        || {
                            let added = added.clone();
                            let manager = self.consensus_manager.clone();
                            async move {
                                let session = manager.consensus().session().await;
                                session.spawn_blocking(move |c| collect_heads_checked_v3(c, &added)).await
                            }
                        },
                        || self.heads_tx.receiver_count() > 0,
                        shutdown.clone(),
                        std::time::Duration::from_millis(250),
                    )
                    .await
                    .unwrap_or_default()
                } else {
                    Vec::new()
                };
                // Lossy under lag — a slow WS subscriber drops events and reconnects
                // + backfills (design R-5); the sends never block the pump.
                for head in heads {
                    let _ = self.heads_tx.send(head);
                }
                if want_logs {
                    // Keep this notification pending on a local read fault. No partial
                    // reorg batch is published and no later notification overtakes it.
                    // Each attempt releases its consensus session before the delay.
                    let log_events = retry_reorg_logs_v2(
                        || {
                            let added = log_added.clone();
                            let removed = removed.clone();
                            let manager = self.consensus_manager.clone();
                            async move {
                                let session = manager.consensus().session().await;
                                session
                                    .spawn_blocking(move |c| reorg_ordered_checked_v2(&removed, &added, |l1| c.get_evm_block_logs(l1)))
                                    .await
                            }
                        },
                        || self.logs_tx.receiver_count() > 0,
                        shutdown.clone(),
                        std::time::Duration::from_millis(250),
                    )
                    .await;
                    if let Some(events) = log_events {
                        for (e, removed) in events {
                            let _ = self.logs_tx.send(EthLogEvent { log: to_eth_log_entry(e), removed });
                        }
                    }
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
        let header =
            session.spawn_blocking(|c| c.get_evm_head_header()).await.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
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
        let header =
            session.spawn_blocking(|c| c.get_evm_head_header()).await.map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
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
        // safe/finalized read certified canonical prefix heads; a
        // numeric/earliest tag reads the historical block. Fail CLOSED if the
        // resolved block has no snapshot (pruned / pre-activation) so a caller
        // never silently gets latest state for a historical query.
        let session = self.consensus_manager.consensus().session().await;
        session
            .spawn_blocking(move |c| -> EthResult<Option<kaspa_consensus_core::evm::EvmAccountSnapshot>> {
                let target = kaspa_consensus_core::evm::EvmAddress::from_bytes(address);
                with_resolved_state_selector(c, &block, |l1| {
                    // Selector did not resolve to a known block (e.g. a future number) ⇒ no account.
                    let Some(l1) = l1 else {
                        if matches!(&block, BlockId::Tag(t) if matches!(t.as_str(), "safe" | "finalized" | "latest" | "pending")) {
                            return Err(EthRpcError::server("requested executed EVM head unavailable"));
                        }
                        return Ok(None);
                    };
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
            // Audit H-1: StateUnavailable is TRANSIENT (no canonical state view yet) —
            // a retryable -32000, distinct from the permanent class-1 rejection. The
            // client may resubmit once the sink has a committed EVM snapshot.
            Err(e @ EvmMempoolError::StateUnavailable(_)) => {
                Err(EthRpcError::new(-32000, format!("evm tx temporarily not admissible (retry): {e}")))
            }
            Err(e) => Err(EthRpcError::new(-32000, format!("evm tx rejected: {e:?}"))),
        }
    }

    async fn transaction_receipt(&self, tx_hash: [u8; 32]) -> EthResult<Option<EthReceipt>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(tx_hash);
        // One spawn_blocking: the receipt view + (locate + decode) the raw tx for
        // from/to/contractAddress/type (RocksDB reads + k256 recovery are blocking).
        let resolved = session
            .spawn_blocking(move |c| {
                with_canonical_receipt_v2(c, h, |view| {
                    // Existing optional enrichment remains compatible; only
                    // the canonical receipt authority is checked here.
                    let base_fee =
                        c.get_evm_header_of(view.accepting_block).ok().flatten().and_then(|hdr| hdr.base_fee_per_gas.try_to_u128());
                    let decoded = c.get_evm_raw_tx(h).ok().flatten().and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok());
                    Ok((view, decoded, base_fee))
                })
            })
            .await?;
        Ok(resolved.map(|(v, decoded, accepting_base_fee)| {
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
        Ok(self
            .replay_trace(tx_hash, false)
            .await?
            .map(|(traced, _accepting, _originating)| traced.prestate.iter().map(convert_prestate_account).collect()))
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
        let session = self.consensus_manager.consensus().session().await;
        let prepared = session
            .clone()
            .spawn_blocking(move |c| with_candidate_authority_v2(c, h, pending_raw, |raw, skip, accepted| Ok((raw, skip, accepted))))
            .await?;
        let Some((raw, recorded_skip_class, accepted)) = prepared else {
            return Ok(None);
        };
        // Resolve authority before head reconstruction or candidate execution.
        let (snapshot, env) = self.head_snapshot_and_env().await?;
        // §11.5: cap concurrent replays; the permit is held across the blocking work.
        let permit = self
            .trace_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EthRpcError::server("too many concurrent traces in flight; retry shortly"))?;
        session
            .spawn_blocking(move |_c| {
                let _permit = permit;
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
            .spawn_blocking(move |c| with_status_authority_v2(c, h, |locations, receipt| Ok((Some(locations), receipt))))
            .await?;
        // The L1 block ids are 64-byte (BLAKE2b-512); expose the leading 32 as
        // standard-shaped, client-opaque ids (as elsewhere here).
        let to32 = |b: &kaspa_hashes::Hash64| {
            let mut x = [0u8; 32];
            x.copy_from_slice(&b.as_bytes()[..32]);
            x
        };
        let canonical_accepted = canon.as_ref().map(|v| (to32(&v.accepting_block), v.receipt_index));
        let (included_in, all_accepted, last_skip_class) = match &locs {
            Some(l) => (
                l.included_in.iter().map(to32).collect::<Vec<_>>(),
                l.accepted_in.iter().map(|(b, idx)| (to32(b), *idx)).collect::<Vec<_>>(),
                l.last_skip_class,
            ),
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
                with_transaction_authority_v2(c, h, |raw, receipt| {
                    let decoded = raw.and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok());
                    let ctx = receipt.map(|v| {
                        let bh = v.accepting_block.as_bytes();
                        let mut block_hash = [0u8; 32];
                        block_hash.copy_from_slice(&bh[..32]);
                        (v.evm_number, block_hash, v.receipt_index)
                    });
                    Ok((decoded, ctx))
                })
            })
            .await?;
        // Pending fallback (audit M-08): a tx that is in this node's mempool but
        // not yet in any payload is decoded from the pool, with null block context.
        let decoded = match decoded {
            Some(d) => Some(d),
            None => {
                self.flow_context.mining_manager().get_evm_transaction_raw(&h).and_then(|raw| kaspa_evm::tx::decode_eth_tx(&raw).ok())
            }
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
        session.spawn_blocking(move |c| checked_block_with_parent_v3(c, c.get_evm_block_by_number(number))).await
    }

    async fn block_by_tag(&self, tag: &str) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let tag = tag.to_string();
        // Resolve each tag to its real L1 block (audit H-04): latest/pending = the
        // latest fully executed canonical result; safe/finalized = certified prefix
        // pointers or explicit absence; earliest = number 0. Missing heads never alias the sink.
        session
            .spawn_blocking(move |c| {
                let heads = c.get_evm_canonical_heads().map_err(|e| EthRpcError::server(format!("canonical EVM heads: {e}")))?;
                let resp = match tag.as_str() {
                    "earliest" => c.get_evm_block_by_number(0),
                    "safe" => match heads {
                        Some(hd) => match hd.safe_head() {
                            Some(h) => c.get_evm_block_by_l1_hash(h),
                            None => Ok(None),
                        },
                        None => Ok(None),
                    },
                    "finalized" => match heads {
                        Some(hd) => match hd.finalized_head() {
                            Some(h) => c.get_evm_block_by_l1_hash(h),
                            None => Ok(None),
                        },
                        None => Ok(None),
                    },
                    _ => match heads.and_then(|h| h.latest_head()) {
                        Some(h) => c.get_evm_block_by_l1_hash(h),
                        None => Ok(None),
                    }, // latest / pending
                };
                checked_block_with_parent_v3(c, resp)
            })
            .await
    }

    async fn block_by_hash(&self, hash: [u8; 32]) -> EthResult<Option<EthBlock>> {
        let session = self.consensus_manager.consensus().session().await;
        let h = EvmH256::from_bytes(hash);
        session.spawn_blocking(move |c| checked_block_with_parent_v3(c, c.get_evm_block_by_rpc_hash(h))).await
    }

    async fn get_logs(&self, from: u64, to: u64, addresses: Vec<[u8; 20]>, topics: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>> {
        let session = self.consensus_manager.consensus().session().await;
        let addrs: Vec<_> = addresses.into_iter().map(kaspa_consensus_core::evm::EvmAddress::from_bytes).collect();
        let tpcs: Vec<Vec<_>> = topics.into_iter().map(|p| p.into_iter().map(EvmH256::from_bytes).collect()).collect();
        let entries = session
            .spawn_blocking(move |c| c.get_evm_logs(from, to, addrs, tpcs))
            .await
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?;
        Ok(entries.into_iter().map(to_eth_log_entry).collect())
    }

    async fn get_logs_by_block_hash(
        &self,
        hash: [u8; 32],
        addresses: Vec<[u8; 20]>,
        topics: Vec<Vec<[u8; 32]>>,
    ) -> EthResult<Option<Vec<EthLogEntry>>> {
        let session = self.consensus_manager.consensus().session().await;
        session.spawn_blocking(move |c| exact_hash_logs_v2(c, hash, addresses, topics)).await
    }

    async fn fee_history(&self, block_count: u64, newest: u64, reward_percentiles: Vec<f64>) -> EthResult<EthFeeHistory> {
        let session = self.consensus_manager.consensus().session().await;
        let np = reward_percentiles.len();
        let (base_fees, ratios, oldest) = session
            .spawn_blocking(move |c| {
                let oldest = newest.saturating_sub(block_count.saturating_sub(1));
                let head_base = c.get_evm_head_header().ok().flatten().map(|h| h.base_fee_per_gas.to_be_bytes()).unwrap_or([0u8; 32]);
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
    ) -> EthResult<Option<(kaspa_evm::trace::TracedTx, kaspa_consensus_core::BlockHash, Option<kaspa_consensus_core::BlockHash>)>>
    {
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
                with_replay_authority_v2(c, h, |view, body, parent_header, parent_snapshot| {
                    let accepting = view.accepting_block;
                    // Replay under the SAME activation fences the accepting block executed
                    // with (gas-pool v1/v2, withdraw-cap, bridge ledger, F003) — read from the
                    // network Params, not assumed inert (testnet runs a finite gas-pool-v2 fence).
                    let (gas_pool_v2, f002_withdraw_cap, bridge_ledger, f003_mldsa_verify) = c.evm_activation_fences();
                    let traced = with_replay_market_owned_v2(c, accepting, &body, |market| {
                        kaspa_evm::trace::trace_accepted_tx_with_market_owned_v2(
                            &parent_snapshot,
                            parent_header.as_ref(),
                            &body,
                            view.receipt_index,
                            &view.receipt,
                            gas_pool_v2,
                            f002_withdraw_cap,
                            bridge_ledger,
                            f003_mldsa_verify,
                            capture_struct_logs,
                            kaspa_evm::trace::TraceLimits::default(),
                            market,
                        )
                        .map_err(|e| EthRpcError::server(format!("{e}")))
                    })?;
                    let originating = kaspa_evm::trace::candidate_index_for_receipt(&body, view.receipt_index)
                        .map(|i| body.txs[i].originating_payload_block);
                    Ok((traced, accepting, originating))
                })
            })
            .await
    }

    /// Fetch the canonical-head EVM state snapshot + the call env (one spawn_blocking).
    async fn head_snapshot_and_env(&self) -> EthResult<(kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_evm::sim::EthCallEnv)> {
        let session = self.consensus_manager.consensus().session().await;
        session.spawn_blocking(|c| canonical_head_snapshot_and_env_v2(c)).await
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
                with_resolved_simulation_target(c, &block, |l1| {
                    // §12.6 env from the TARGET block's committed header.
                    let header = match c.get_evm_header_of(l1) {
                        Ok(Some(h)) => h,
                        Ok(None) => {
                            return Err(EthRpcError::server("eth_call/eth_estimateGas: the requested block is not an EVM block"));
                        }
                        Err(e) => return Err(EthRpcError::server(format!("consensus: {e:?}"))),
                    };
                    // State: hot snapshot, else §12 reconstruction past the window.
                    let snapshot = match c.get_evm_state_snapshot_of(l1) {
                        Ok(Some(s)) => s,
                        Ok(None) | Err(_) => match c.reconstruct_evm_state_at(l1) {
                            Ok(Some(s)) => s,
                            Ok(None) => {
                                return Err(EthRpcError::server("eth_call/eth_estimateGas: state unavailable (not an EVM block)"));
                            }
                            Err(e) => return Err(EthRpcError::server(format!("EVM state unavailable at the requested block: {e}"))),
                        },
                    };
                    // PREA P0-1: F003 activation at the TARGET block — the SAME comparison
                    // the executor/trace replay use, against THIS block's L1 DAA score
                    // (`B.header.daa_score >= f003_mldsa_verify_activation_daa_score`; see
                    // kaspa_evm::executor `f003_active` and trace::trace_accepted_tx). The
                    // fence is `u64::MAX`-inert on every network today ⇒ `false` exactly as
                    // before, but tracks the executor automatically once F003 is deployed.
                    let (.., f003_mldsa_verify_fence) = c.evm_activation_fences();
                    with_executed_l1_daa(c, l1, |target_daa| {
                        with_historical_market_view_v2(c, l1, |market| {
                            let env = kaspa_evm::sim::EthCallEnv {
                                chain_id: EVM_CHAIN_ID,
                                number: header.evm_number,
                                timestamp: header.evm_timestamp_sec,
                                coinbase: header.coinbase,
                                gas_limit: header.gas_limit,
                                f003_active: target_daa >= f003_mldsa_verify_fence,
                                palw_view: None,
                                palw_view_owned_v2: market.as_ref().map(|(view, _)| view.clone()),
                                market_fences: market.as_ref().map(|(_, fences)| *fences).unwrap_or_default(),
                                chain_id_for_holders: EVM_CHAIN_ID,
                            };
                            Ok((snapshot, env))
                        })
                    })
                })
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

fn with_replay_market_owned_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    accepting: kaspa_consensus_core::BlockHash,
    body: &kaspa_consensus_core::evm::EvmTraceReplayBodyV1,
    visit: impl FnOnce(Option<&kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2>) -> EthResult<T>,
) -> EthResult<T> {
    let context = c
        .palw_evm_replay_market_owned_v2(accepting)
        .map_err(|e| EthRpcError::server(format!("historical replay market unavailable: {e}")))?;
    if let Some(context) = &context
        && (context.accepting != accepting
            || context.selected_parent != body.selected_parent
            || context.accepting_daa != body.env.daa_score)
    {
        return Err(EthRpcError::server("historical replay market identity differs from accepted replay body"));
    }
    visit(context.as_ref())
}

fn transaction_read_v2<T>(label: &str, result: kaspa_consensus_core::errors::consensus::ConsensusResult<T>) -> EthResult<T> {
    result.map_err(|e| EthRpcError::server(format!("{label}: {e}")))
}

#[cfg(test)]
#[path = "eth_rpc_replay_market_tests_v2.rs"]
mod replay_market_tests_v2;

/// Exact immutable hash query; unlike the number range it may name a retained
/// side branch. Filter in place and keep the existing 10,000-result limit.
fn exact_hash_logs_v2(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    hash: [u8; 32],
    addresses: Vec<[u8; 20]>,
    topics: Vec<Vec<[u8; 32]>>,
) -> EthResult<Option<Vec<EthLogEntry>>> {
    let Some(mut logs) = transaction_read_v2("exact block-hash logs", c.get_evm_block_logs_by_rpc_hash(EvmH256::from_bytes(hash)))?
    else {
        return Ok(None);
    };
    if logs.iter().any(|log| log.block_l1_hash.as_bytes()[..32] != hash[..]) {
        return Err(EthRpcError::server("exact block-hash reader returned a different block"));
    }
    logs.retain(|log| {
        (addresses.is_empty() || addresses.contains(&log.address.as_bytes()))
            && topics.iter().enumerate().all(|(index, allowed)| {
                allowed.is_empty() || log.topics.get(index).is_some_and(|topic| allowed.contains(&topic.as_bytes()))
            })
    });
    if logs.len() > 10_000 {
        return Err(EthRpcError::server("eth_getLogs: query matched more than 10000 logs; narrow the filters"));
    }
    Ok(Some(logs.into_iter().map(to_eth_log_entry).collect()))
}

fn with_candidate_authority_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tx: EvmH256,
    pending_raw: Option<Vec<u8>>,
    visit: impl FnOnce(Vec<u8>, Option<u8>, bool) -> EthResult<T>,
) -> EthResult<Option<T>> {
    let raw = match pending_raw {
        Some(raw) => raw,
        None => match transaction_read_v2("candidate raw transaction", c.get_evm_raw_tx(tx))? {
            Some(raw) => raw,
            None => return Ok(None),
        },
    };
    let locations = transaction_read_v2("candidate transaction locations", c.get_evm_tx_locations(tx))?;
    let receipt = with_canonical_receipt_v2(c, tx, Ok)?;
    visit(raw, locations.last_skip_class, receipt.is_some()).map(Some)
}

fn with_status_authority_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tx: EvmH256,
    visit: impl FnOnce(kaspa_consensus_core::evm::EvmTxLocations, Option<kaspa_consensus_core::evm::EvmTxReceiptView>) -> EthResult<T>,
) -> EthResult<T> {
    let locations = transaction_read_v2("transaction status locations", c.get_evm_tx_locations(tx))?;
    let receipt = with_canonical_receipt_v2(c, tx, Ok)?;
    visit(locations, receipt)
}

fn with_transaction_authority_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tx: EvmH256,
    visit: impl FnOnce(Option<Vec<u8>>, Option<kaspa_consensus_core::evm::EvmTxReceiptView>) -> EthResult<T>,
) -> EthResult<T> {
    let raw = transaction_read_v2("raw transaction lookup", c.get_evm_raw_tx(tx))?;
    let receipt = with_canonical_receipt_v2(c, tx, Ok)?;
    visit(raw, receipt)
}

fn with_replay_authority_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tx: EvmH256,
    visit: impl FnOnce(
        kaspa_consensus_core::evm::EvmTxReceiptView,
        kaspa_consensus_core::evm::EvmTraceReplayBodyV1,
        Option<kaspa_consensus_core::evm::EvmExecutionHeader>,
        kaspa_consensus_core::evm::EvmStateSnapshot,
    ) -> EthResult<T>,
) -> EthResult<Option<T>> {
    with_canonical_receipt_v2(c, tx, |view| {
        let body = transaction_read_v2("trace replay body", c.get_evm_trace_replay_body(view.accepting_block))?.ok_or_else(|| {
            EthRpcError::server("trace data unavailable for this transaction's block (pruned or recorded before trace support)")
        })?;
        let parent_header = transaction_read_v2("trace parent execution header", c.get_evm_header_of(body.selected_parent))?;
        // A true absent parent header keeps the existing genesis prestate
        // contract. An unreadable header must never select that default.
        let snapshot = if parent_header.is_some() {
            transaction_read_v2("trace parent execution state", c.get_evm_state_snapshot_of(body.selected_parent))?
                .ok_or_else(|| EthRpcError::server("historical state unavailable for trace (selected-parent state snapshot pruned)"))?
        } else {
            Default::default()
        };
        visit(view, body, parent_header, snapshot)
    })
}

/// Only a true absent canonical receipt is null. Local authority read faults
/// stop enrichment and rendering instead of selecting an unknown result.
fn with_canonical_receipt_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tx: EvmH256,
    visit: impl FnOnce(kaspa_consensus_core::evm::EvmTxReceiptView) -> EthResult<T>,
) -> EthResult<Option<T>> {
    match c.get_evm_tx_receipt(tx).map_err(|e| EthRpcError::server(format!("canonical EVM receipt unavailable: {e}")))? {
        Some(view) => visit(view).map(Some),
        None => Ok(None),
    }
}

/// Build one coherent latest/pending environment from the canonical executed
/// head identity. The selected PALW tip may be newer than this prefix head.
fn canonical_head_snapshot_and_env_v2(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
) -> EthResult<(kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_evm::sim::EthCallEnv)> {
    let heads = c
        .get_evm_canonical_heads()
        .map_err(|e| EthRpcError::server(format!("canonical EVM heads: {e}")))?
        .ok_or_else(|| EthRpcError::server("canonical EVM heads unavailable"))?;
    let head = heads.latest_head().ok_or_else(|| EthRpcError::server("latest executed EVM head unavailable"))?;
    with_executed_l1_daa(c, head, |head_daa| {
        let snapshot = c
            .get_evm_state_snapshot_of(head)
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?
            .ok_or_else(|| EthRpcError::server("EVM state snapshot unavailable for head; refusing to simulate against empty state"))?;
        let header = c
            .get_evm_header_of(head)
            .map_err(|e| EthRpcError::server(format!("consensus: {e:?}")))?
            .ok_or_else(|| EthRpcError::server("latest executed EVM header unavailable"))?;
        with_historical_market_view_v2(c, head, |market| {
            let (.., f003_fence) = c.evm_activation_fences();
            let env = kaspa_evm::sim::EthCallEnv {
                chain_id: EVM_CHAIN_ID,
                number: header.evm_number,
                timestamp: header.evm_timestamp_sec,
                coinbase: header.coinbase,
                gas_limit: header.gas_limit,
                f003_active: head_daa >= f003_fence,
                palw_view: None,
                palw_view_owned_v2: market.as_ref().map(|(view, _)| view.clone()),
                market_fences: market.as_ref().map(|(_, fences)| *fences).unwrap_or_default(),
                chain_id_for_holders: EVM_CHAIN_ID,
            };
            Ok((snapshot, env))
        })
    })
}

fn with_historical_market_view_v2<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    target: kaspa_consensus_core::BlockHash,
    visit: impl FnOnce(Option<kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewAtV2>) -> EthResult<T>,
) -> EthResult<T> {
    let view =
        c.palw_evm_view_at_owned_v2(target).map_err(|e| EthRpcError::server(format!("historical PALW view unavailable: {e}")))?;
    visit(view)
}

/// Activation metadata belongs to this exact executed L1 block. An unreadable
/// header is unavailable, never a successful simulation with an invented DAA zero.
fn with_executed_l1_daa<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    l1: kaspa_consensus_core::BlockHash,
    then: impl FnOnce(u64) -> EthResult<T>,
) -> EthResult<T> {
    let header = c.get_header(l1).map_err(|e| EthRpcError::server(format!("executed L1 activation header: {e:?}")))?;
    then(header.daa_score)
}

#[cfg(test)]
#[path = "eth_rpc_activation_daa_tests.rs"]
mod activation_daa_tests;

/// Resolve once before either account lookup or historical simulation. API read
/// errors must not become an absent account, unknown selector or canonicality denial.
fn with_resolved_state_selector<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    block: &BlockId,
    then: impl FnOnce(Option<kaspa_consensus_core::BlockHash>) -> EthResult<T>,
) -> EthResult<T> {
    let read_error = |e| EthRpcError::server(format!("EVM selector read: {e:?}"));
    let l1 = match block {
        BlockId::Number(n) => c.get_evm_block_by_number(*n).map_err(read_error)?.map(|b| b.l1_hash),
        BlockId::Tag(t) => match t.as_str() {
            "earliest" => c.get_evm_block_by_number(0).map_err(read_error)?.map(|b| b.l1_hash),
            "safe" | "finalized" => resolve_executed_tag(c, t)?,
            _ => resolve_executed_tag(c, "latest")?, // existing latest / pending semantics
        },
        BlockId::Hash { hash, require_canonical } => {
            match c.get_evm_block_by_rpc_hash(EvmH256::from_bytes(*hash)).map_err(read_error)? {
                Some(b) => {
                    if *require_canonical && !c.is_chain_block(b.l1_hash).map_err(read_error)? {
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
    then(l1)
}

/// Simulation requires an executed target. Missing certified heads are local
/// unavailability; missing explicit historical blocks are invalid selectors.
fn with_resolved_simulation_target<T>(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    block: &BlockId,
    then: impl FnOnce(kaspa_consensus_core::BlockHash) -> EthResult<T>,
) -> EthResult<T> {
    with_resolved_state_selector(c, block, |l1| {
        let l1 = l1.ok_or_else(|| {
            if matches!(block, BlockId::Tag(t) if matches!(t.as_str(), "safe" | "finalized")) {
                EthRpcError::server("requested executed EVM head unavailable")
            } else {
                EthRpcError::invalid_params("eth_call/eth_estimateGas: the requested block is unknown")
            }
        })?;
        then(l1)
    })
}

#[cfg(test)]
#[path = "eth_rpc_state_selector_tests.rs"]
mod state_selector_tests;

/// Errors in canonical evidence are surfaced; an absent safe/finalized tag stays absent.
fn resolve_executed_tag(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    tag: &str,
) -> EthResult<Option<kaspa_consensus_core::BlockHash>> {
    let heads = c.get_evm_canonical_heads().map_err(|e| EthRpcError::server(format!("canonical EVM heads: {e}")))?;
    Ok(heads.and_then(|h| match tag {
        "safe" => h.safe_head(),
        "finalized" => h.finalized_head(),
        _ => h.latest_head(),
    }))
}

/// The eth-rpc 32-byte parentHash of an EVM block: the first 32 bytes of the
/// immutable selected parent's L1 hash. Zero for EVM block 0 or the native
/// API's genuine first-EVM boundary. Read errors must not fabricate genesis.
fn parent_l1_hash32(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    r: &kaspa_consensus_core::evm::EvmBlockResponse,
) -> EthResult<[u8; 32]> {
    if r.header.evm_number == 0 {
        return Ok([0u8; 32]);
    }
    Ok(match transaction_read_v2("EVM immutable parent", c.get_evm_parent_l1_hash(r.l1_hash))? {
        Some(p) => {
            let mut h = [0u8; 32];
            h.copy_from_slice(&p.as_bytes()[..32]);
            h
        }
        None => [0u8; 32],
    })
}

fn checked_block_with_parent_v3(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    response: kaspa_consensus_core::errors::consensus::ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>>,
) -> EthResult<Option<EthBlock>> {
    let Some(block) = transaction_read_v2("EVM block", response)? else { return Ok(None) };
    let parent = parent_l1_hash32(c, &block)?;
    Ok(Some(to_eth_block(block, parent)))
}

fn collect_heads_checked_v3(
    c: &(impl kaspa_consensus_core::api::ConsensusApi + ?Sized),
    added: &[kaspa_consensus_core::BlockHash],
) -> EthResult<Vec<EthBlock>> {
    let mut heads = Vec::new();
    for &l1 in added {
        if let Some(head) = checked_block_with_parent_v3(c, c.get_evm_block_by_l1_hash(l1))? {
            heads.push(head);
        }
    }
    Ok(heads)
}

/// Map a consensus `EvmBlockResponse` to the adapter's primitive [`EthBlock`].
/// The 32-byte `hash` is the first 32 bytes of the 64-byte L1 hash (the same id
/// the receipt exposes as `blockHash`); `parent_hash` is the `evm_number − 1`
/// immutable selected-parent id, computed by [`parent_l1_hash32`].
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
        // will NOT verify against it. See docs/reference/evm/evm-differences-from-ethereum.md
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
#[cfg(test)]
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

/// Checked immutable reads preserve the entire notification as the retry unit.
/// A successful empty block is distinct from an unavailable receipt/header row.
fn reorg_ordered_checked_v2<H: Copy, T, E>(
    removed: &[H],
    added: &[H],
    mut read: impl FnMut(H) -> Result<Vec<T>, E>,
) -> Result<Vec<(T, bool)>, E> {
    let mut out = Vec::new();
    for h in removed.iter().rev().copied() {
        out.extend(read(h)?.into_iter().map(|item| (item, true)));
    }
    for h in added.iter().copied() {
        out.extend(read(h)?.into_iter().map(|item| (item, false)));
    }
    Ok(out)
}

async fn retry_reorg_logs_v2<T, E: std::fmt::Debug, F: std::future::Future<Output = Result<Vec<T>, E>>>(
    mut read: impl FnMut() -> F,
    mut wanted: impl FnMut() -> bool,
    shutdown: kaspa_utils::triggers::Listener,
    delay: std::time::Duration,
) -> Option<Vec<T>> {
    let mut warned = false;
    while wanted() {
        let result = tokio::select! {
            biased;
            _ = shutdown.clone() => return None,
            result = read() => result,
        };
        match result {
            Ok(events) => return wanted().then_some(events),
            Err(error) => {
                if !warned {
                    kaspa_core::warn!(
                        "[{ETH_RPC}] immutable notification read unavailable; retaining notification for retry: {error:?}"
                    );
                    warned = true;
                }
            }
        }
        tokio::select! {
            biased;
            _ = shutdown.clone() => return None,
            _ = tokio::time::sleep(delay) => {},
        }
    }
    None
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
    // Fired by `signal_exit` to break the `serve_with_shutdown` accept loop so the
    // `start` future can complete and the AsyncRuntime shutdown join doesn't wedge
    // (the `--evm-rpc-listen` Ctrl+C deadlock).
    shutdown: SingleTrigger,
}

impl EthRpcService {
    pub fn new(
        addr: SocketAddr,
        consensus_manager: Arc<ConsensusManager>,
        flow_context: Arc<FlowContext>,
        consensus_notifier: Arc<ConsensusNotifier>,
    ) -> Self {
        Self {
            addr,
            provider: Arc::new(NodeEthProvider::new(consensus_manager, flow_context, consensus_notifier)),
            shutdown: SingleTrigger::default(),
        }
    }
}

impl AsyncService for EthRpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        ETH_RPC
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        // Cloned before the async block so `signal_exit` can fire the same trigger
        // from the runtime's shutdown thread.
        let shutdown_signal = self.shutdown.listener.clone();
        Box::pin(async move {
            // §9: register the newHeads pump (VirtualChainChanged → EthBlock fan-out)
            // before serving, so a client that subscribes right after connect sees
            // live heads.
            self.provider.clone().spawn_head_pump(shutdown_signal.clone());
            let provider: Arc<dyn EthProvider> = self.provider.clone();
            // Returning `Ok(())` on a server error (rather than propagating it)
            // preserves the existing "eth-rpc bind/serve failure does not take down
            // the node" behavior.
            if let Err(e) = kaspa_eth_rpc::serve_with_shutdown(self.addr, provider, shutdown_signal).await {
                kaspa_core::warn!("[{ETH_RPC}] server on {} exited: {e}", self.addr);
            }
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        kaspa_core::trace!("sending an exit signal to {}", ETH_RPC);
        self.shutdown.trigger.trigger();
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
                ('B', 0, true), // detached, OLDEST (B) before the more-recent (C)
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
        let pairs = reorg_ordered::<u8, u8>(&[], b"A", |_| vec![1, 2]);
        assert_eq!(pairs, vec![(1, false), (2, false)]);
    }
}

#[cfg(test)]
#[path = "eth_rpc_historical_market_tests.rs"]
mod historical_market_tests;

#[cfg(test)]
#[path = "eth_rpc_canonical_head_env_tests.rs"]
mod canonical_head_env_tests;

#[cfg(test)]
#[path = "eth_rpc_receipt_fault_tests.rs"]
mod receipt_fault_tests;

#[cfg(test)]
#[path = "eth_rpc_transaction_authority_tests.rs"]
mod transaction_authority_tests;

#[cfg(test)]
#[path = "eth_rpc_exact_hash_logs_tests.rs"]
mod exact_hash_logs_tests;

#[cfg(test)]
#[path = "eth_rpc_reorg_retry_tests.rs"]
mod reorg_retry_tests;

#[cfg(test)]
#[path = "eth_rpc_head_fault_tests.rs"]
mod head_fault_tests;
