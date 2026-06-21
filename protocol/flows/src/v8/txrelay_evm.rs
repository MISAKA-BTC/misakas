//! kaspa-pq EVM Lane v0.4 (§14.2): pending-EVM-tx relay flows (protocol ≥ 101).
//!
//! Mirrors the UTXO tx relay (inv → request → tx/not-found) with the §14.2
//! network rules baked in:
//!
//! - relay happens only AFTER the class-1 admission precheck (`admit_tx_info`
//!   via the EVM mempool): a peer relaying a class-1-invalid tx is misbehaving
//!   by protocol rule, and we never re-announce anything we did not admit;
//! - strictly lower priority than UTXO gossip: separate small channels whose
//!   inv overflow policy is Drop (shed, never disconnect), a tighter per-inv
//!   cap, and the spread side batches on a 4x longer interval;
//! - a non-`evm` build cannot run the precheck, so it never requests pending
//!   EVM txs (invs are drained and ignored) and never serves/punishes either.
//!
//! The hash a peer announces is never trusted: the keccak256 tx hash is
//! recomputed from the raw bytes during admission and compared against the
//! request.

use crate::{
    flow_context::{FlowContext, RequestScope},
    flow_trait::Flow,
    flowcontext::evm_transactions::MAX_INV_PER_EVM_TX_INV_MSG,
};
use kaspa_hashes::EvmH256;
use kaspa_mining::evm_mempool::EvmMempoolError;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    dequeue, make_message,
    pb::{EvmTransactionMessage, EvmTransactionNotFoundMessage, RequestEvmTransactionsMessage, kaspad_message::Payload},
};
use std::sync::Arc;
use tokio::time::timeout;

enum Response {
    Transaction(Vec<u8>),
    NotFound(EvmH256),
}

fn evm_hash_from_wire(bytes: &[u8]) -> Result<EvmH256, ProtocolError> {
    EvmH256::try_from_slice(bytes).map_err(|_| ProtocolError::Other("evm tx hash with invalid length"))
}

/// Flow listening to InvEvmTransactions messages, requesting the corresponding
/// raw txs when missing from the EVM mempool, admitting them (class-1 precheck)
/// and re-announcing the admitted ones to the rest of the network.
pub struct RelayEvmTransactionsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    /// A route specific for invs messages
    invs_route: IncomingRoute,
    /// A route for EvmTransaction and EvmTransactionNotFound messages
    msg_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RelayEvmTransactionsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RelayEvmTransactionsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, invs_route: IncomingRoute, msg_route: IncomingRoute) -> Self {
        Self { ctx, router, invs_route, msg_route }
    }

    pub fn invs_channel_size() -> usize {
        // Deliberately smaller than the UTXO invs channel (4096): EVM gossip is
        // the lower-priority lane and its overflow policy is Drop (§14.2).
        256
    }

    pub fn txs_channel_size() -> usize {
        // Must correlate with the per-inv cap, which upper-bounds outstanding requests
        MAX_INV_PER_EVM_TX_INV_MSG
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        // A node built without the `evm` feature cannot run the class-1
        // admission precheck, so it neither requests nor judges peers — invs
        // are drained and ignored (the routes must keep being consumed).
        let admission_supported = self.ctx.mining_manager().clone().supports_evm_admission();

        loop {
            let inv = dequeue!(self.invs_route, Payload::InvEvmTransactions)?;
            if inv.hashes.len() > MAX_INV_PER_EVM_TX_INV_MSG {
                return Err(ProtocolError::Other("Number of invs in evm tx inv message is over the limit"));
            }
            if !admission_supported {
                continue;
            }
            let tx_hashes = inv.hashes.iter().map(|h| evm_hash_from_wire(h)).collect::<Result<Vec<_>, _>>()?;

            // EVM tx relay obeys the same sync gate as UTXO tx relay
            let session = self.ctx.consensus().unguarded_session();
            if !self.ctx.is_nearly_synced(&session).await {
                continue;
            }

            let requests = self.request_transactions(tx_hashes).await?;
            self.receive_transactions(requests).await?;
        }
    }

    async fn request_transactions(&self, tx_hashes: Vec<EvmH256>) -> Result<Vec<RequestScope<EvmH256>>, ProtocolError> {
        // Hashes unknown to the EVM mempool and not already requested from another peer
        let unknown = self.ctx.mining_manager().clone().evm_unknown_transactions(tx_hashes);
        let mut requests = Vec::new();
        for tx_hash in unknown {
            if let Some(req) = self.ctx.try_adding_evm_transaction_request(tx_hash) {
                requests.push(req);
            }
        }

        if !requests.is_empty() {
            self.router
                .enqueue(make_message!(
                    Payload::RequestEvmTransactions,
                    RequestEvmTransactionsMessage { hashes: requests.iter().map(|x| x.req.as_bytes().to_vec()).collect() }
                ))
                .await?;
        }

        Ok(requests)
    }

    /// Returns the next EvmTransaction or EvmTransactionNotFound message in msg_route
    async fn read_response(&mut self) -> Result<Response, ProtocolError> {
        match timeout(DEFAULT_TIMEOUT, self.msg_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::EvmTransaction(payload)) => Ok(Response::Transaction(payload.raw)),
                        Some(Payload::EvmTransactionNotFound(payload)) => Ok(Response::NotFound(evm_hash_from_wire(&payload.hash)?)),
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::EvmTransaction | Payload::EvmTransactionNotFound),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        }
    }

    async fn receive_transactions(&mut self, requests: Vec<RequestScope<EvmH256>>) -> Result<(), ProtocolError> {
        let mut admitted = Vec::with_capacity(requests.len());
        for request in requests {
            let raw = match self.read_response().await? {
                Response::Transaction(raw) => raw,
                Response::NotFound(tx_hash) => {
                    if tx_hash != request.req {
                        return Err(ProtocolError::OtherOwned(format!(
                            "requested evm tx {} but got not-found for {}",
                            request.req, tx_hash
                        )));
                    }
                    continue;
                }
            };
            // Admission recomputes the keccak256 hash from the raw bytes — the peer's
            // announced hash is verified, never trusted. EVERY outcome (admitted,
            // duplicate, or a benign pool-state rejection) yields that recomputed hash
            // so we can compare it to the request BEFORE crediting the request as
            // obtained (audit #8): otherwise a peer could "fulfill" request X by
            // returning a different valid tx Y that our pool happens to reject.
            let (tx_hash, was_admitted) = match self.ctx.mining_manager().clone().submit_evm_transaction(raw) {
                Ok(tx_hash) => (tx_hash, true),
                Err(EvmMempoolError::Inadmissible(reason)) => {
                    // §14.2: peers must precheck class-1 BEFORE relaying, and class-1
                    // verdicts are deterministic — an inadmissible relay is misbehavior.
                    return Err(ProtocolError::MisbehavingPeer(format!("relayed a class-1-invalid evm tx: {reason}")));
                }
                // audit R2-#3: TooLarge is NOT a transient pool-state condition — a tx
                // that can never fit a payload is deterministically invalid (and is now
                // caught pre-decode in admission too), so a peer relaying it is
                // misbehaving, same class as Inadmissible.
                Err(EvmMempoolError::TooLarge { size, .. }) => {
                    return Err(ProtocolError::MisbehavingPeer(format!("relayed an oversize evm tx ({size} bytes, can never fit a payload)")));
                }
                // Benign: the tx is valid, our pool just will not take it now (already
                // pending, replacement pricing, or capacity). Each carries the
                // recomputed hash for the verification below.
                Err(EvmMempoolError::Duplicate(tx_hash))
                | Err(EvmMempoolError::ReplacementUnderpriced { hash: tx_hash, .. })
                | Err(EvmMempoolError::Full { hash: tx_hash })
                | Err(EvmMempoolError::SenderTxLimit { hash: tx_hash, .. })
                | Err(EvmMempoolError::SenderGasLimit { hash: tx_hash, .. }) => (tx_hash, false),
            };
            if tx_hash != request.req {
                return Err(ProtocolError::OtherOwned(format!(
                    "requested evm tx {} but got a tx hashing to {}",
                    request.req, tx_hash
                )));
            }
            request.report_obtained();
            if was_admitted {
                admitted.push(tx_hash);
            }
        }

        // Re-announce only what we admitted (precheck-then-relay, §14.2)
        self.ctx.broadcast_evm_transactions(admitted).await;
        Ok(())
    }
}

/// Flow listening to RequestEvmTransactions messages, responding with the
/// requested raw txs when pending in the EVM mempool.
pub struct RequestedEvmTransactionsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestedEvmTransactionsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestedEvmTransactionsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestEvmTransactions)?;
            if msg.hashes.len() > MAX_INV_PER_EVM_TX_INV_MSG {
                return Err(ProtocolError::Other("Number of hashes in evm tx request message is over the limit"));
            }
            for hash_bytes in msg.hashes {
                let tx_hash = evm_hash_from_wire(&hash_bytes)?;
                if let Some(raw) = self.ctx.mining_manager().clone().get_evm_transaction_raw(&tx_hash) {
                    self.router.enqueue(make_message!(Payload::EvmTransaction, EvmTransactionMessage { raw })).await?;
                } else {
                    self.router
                        .enqueue(make_message!(
                            Payload::EvmTransactionNotFound,
                            EvmTransactionNotFoundMessage { hash: hash_bytes }
                        ))
                        .await?;
                }
            }
        }
    }
}
