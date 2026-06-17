//! kaspa-pq EVM Lane v0.4 (§14.2 / §9.2): pending EVM deposit-claim relay flows
//! (protocol ≥ 102). Sibling of [`super::txrelay_evm`] (inv → request → claim/
//! not-found), with the claim-specific rules:
//!
//! - A claim's identity is its deposit-lock [`TransactionOutpoint`], NOT a hash.
//! - Unlike an EVM tx (self-validating via keccak-recompute), a `DepositClaim`'s
//!   asserted fields (`evm_address`/`amount_sompi`/`claim_tip_sompi`) are derived
//!   from the on-chain `EVM_DEPOSIT_LOCK` output. So on receipt we RE-RESOLVE the
//!   outpoint against our own LIVE virtual UTXO view and rebuild the canonical
//!   claim (mirroring the RPC submit path); a relayed claim is never trusted on
//!   its asserted fields. A field that contradicts the present lock = misbehavior;
//!   a lock absent/spent in our view = benign (we may simply be behind).
//! - Same low-priority lane as EVM-tx gossip (Drop overflow, 4x batch interval).
//! - A non-`evm` build cannot run the admission precheck, so it never requests
//!   claims (invs drained + ignored) — same as the EVM-tx lane.
//!
//! The authoritative validation is still the VSP template path
//! (`validate_evm_deposit_claims`); this receive-side check is an early filter
//! that avoids amplifying junk and punishes deterministically-misbehaving peers.

use crate::{flow_context::FlowContext, flow_trait::Flow, flowcontext::evm_deposit_claims::MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG};
use kaspa_consensus_core::{
    evm::{DepositClaim, EvmAddress},
    tx::TransactionOutpoint,
};
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    dequeue, make_message,
    pb::{EvmDepositClaimMessage, EvmDepositClaimNotFoundMessage, Outpoint, RequestEvmDepositClaimsMessage, kaspad_message::Payload},
};
use std::sync::Arc;
use tokio::time::timeout;

enum Response {
    Claim(DepositClaim),
    NotFound(TransactionOutpoint),
}

fn outpoint_from_wire(o: Outpoint) -> Result<TransactionOutpoint, ProtocolError> {
    TransactionOutpoint::try_from(o).map_err(|_| ProtocolError::Other("invalid outpoint in deposit-claim message"))
}

fn opt_outpoint_from_wire(o: Option<Outpoint>) -> Result<TransactionOutpoint, ProtocolError> {
    outpoint_from_wire(o.ok_or(ProtocolError::Other("deposit-claim not-found message missing its outpoint"))?)
}

/// Flow listening to InvEvmDepositClaims messages, requesting the corresponding
/// claims when missing from the local claim queue, re-resolving + re-validating
/// them against the live UTXO view, and re-announcing the admitted ones.
pub struct RelayEvmDepositClaimsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    /// A route specific for claim inv messages.
    invs_route: IncomingRoute,
    /// A route for EvmDepositClaim and EvmDepositClaimNotFound messages.
    msg_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RelayEvmDepositClaimsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RelayEvmDepositClaimsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, invs_route: IncomingRoute, msg_route: IncomingRoute) -> Self {
        Self { ctx, router, invs_route, msg_route }
    }

    pub fn invs_channel_size() -> usize {
        // Same low-priority lane as the EVM-tx invs channel (Drop overflow).
        256
    }

    pub fn claims_channel_size() -> usize {
        MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        // A non-`evm` build neither requests nor judges peers (symmetry with the
        // EVM-tx lane) — invs are drained and ignored (keep consuming the route).
        let admission_supported = self.ctx.mining_manager().clone().supports_evm_admission();

        loop {
            let inv = dequeue!(self.invs_route, Payload::InvEvmDepositClaims)?;
            if inv.outpoints.len() > MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG {
                return Err(ProtocolError::Other("Number of outpoints in deposit-claim inv message is over the limit"));
            }
            if !admission_supported {
                continue;
            }
            let outpoints = inv.outpoints.into_iter().map(outpoint_from_wire).collect::<Result<Vec<_>, _>>()?;

            // Same sync gate as the EVM-tx / UTXO relay
            let session = self.ctx.consensus().unguarded_session();
            if !self.ctx.is_nearly_synced(&session).await {
                continue;
            }

            let requests = self.request_claims(outpoints).await?;
            self.receive_claims(requests).await?;
        }
    }

    async fn request_claims(
        &self,
        outpoints: Vec<TransactionOutpoint>,
    ) -> Result<Vec<crate::flow_context::RequestScope<TransactionOutpoint>>, ProtocolError> {
        // Outpoints unknown to the local claim queue and not already requested elsewhere.
        let unknown = self.ctx.mining_manager().clone().evm_unknown_deposit_claims(outpoints);
        let mut requests = Vec::new();
        for outpoint in unknown {
            if let Some(req) = self.ctx.try_adding_evm_deposit_claim_request(outpoint) {
                requests.push(req);
            }
        }

        if !requests.is_empty() {
            self.router
                .enqueue(make_message!(
                    Payload::RequestEvmDepositClaims,
                    RequestEvmDepositClaimsMessage { outpoints: requests.iter().map(|x| (&x.req).into()).collect() }
                ))
                .await?;
        }

        Ok(requests)
    }

    async fn read_response(&mut self) -> Result<Response, ProtocolError> {
        match timeout(DEFAULT_TIMEOUT, self.msg_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::EvmDepositClaim(payload)) => {
                            let claim = borsh::from_slice::<DepositClaim>(&payload.claim)
                                .map_err(|_| ProtocolError::Other("invalid borsh DepositClaim in relay message"))?;
                            Ok(Response::Claim(claim))
                        }
                        Some(Payload::EvmDepositClaimNotFound(payload)) => Ok(Response::NotFound(opt_outpoint_from_wire(payload.outpoint)?)),
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::EvmDepositClaim | Payload::EvmDepositClaimNotFound),
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

    async fn receive_claims(
        &mut self,
        requests: Vec<crate::flow_context::RequestScope<TransactionOutpoint>>,
    ) -> Result<(), ProtocolError> {
        let mut admitted = Vec::with_capacity(requests.len());
        let session = self.ctx.consensus().unguarded_session();
        let sink_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;

        for request in requests {
            let claim = match self.read_response().await? {
                Response::Claim(claim) => claim,
                Response::NotFound(outpoint) => {
                    if outpoint != request.req {
                        return Err(ProtocolError::OtherOwned(format!(
                            "requested deposit claim {} but got not-found for {}",
                            request.req, outpoint
                        )));
                    }
                    continue;
                }
            };

            // The peer must answer the exact request (compare outpoints; a claim has
            // no hash to recompute — this is the §14.2 audit-#8 analog).
            if claim.deposit_outpoint != request.req {
                return Err(ProtocolError::OtherOwned(format!(
                    "requested deposit claim {} but got a claim for {}",
                    request.req, claim.deposit_outpoint
                )));
            }

            // Re-resolve against OUR live UTXO view and rebuild the canonical claim
            // (mirrors rpc/service submit_evm_deposit_claim_call). The relayed claim's
            // asserted fields are trusted only after they match the on-chain lock.
            let entry = match session.async_get_virtual_utxo_entry(request.req).await {
                Some(entry) => entry,
                // Absent/spent in our view — we may simply be behind. Benign: do not
                // admit, do not punish, do not re-announce (the lock isn't claimable here).
                None => {
                    request.report_obtained();
                    continue;
                }
            };
            let lock = match kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key) {
                Some(lock) => lock,
                // The outpoint exists but is NOT a deposit lock — deterministic given our
                // view, so a peer asserting a claim for it is misbehaving (§14.2 class-1 analog).
                None => {
                    return Err(ProtocolError::MisbehavingPeer(format!(
                        "relayed a deposit claim for a non-EVM_DEPOSIT_LOCK outpoint {}",
                        request.req
                    )));
                }
            };

            let canonical = DepositClaim {
                deposit_outpoint: request.req,
                evm_address: EvmAddress::from_bytes(lock.evm_address),
                amount_sompi: entry.amount,
                claim_tip_sompi: lock.claim_tip_sompi,
            };
            // A field that contradicts the present lock is deterministic misbehavior.
            if claim != canonical {
                return Err(ProtocolError::MisbehavingPeer(format!(
                    "relayed a deposit claim whose fields contradict EVM_DEPOSIT_LOCK {}",
                    request.req
                )));
            }

            // Lock-property / timing gates (NOT the peer's fault): an unclaimable lock
            // (tip > amount) or one already at/past its refund window is a benign skip —
            // mark the request obtained but do not admit or re-announce.
            if lock.claim_tip_sompi > entry.amount || sink_daa >= lock.timeout_daa_score {
                request.report_obtained();
                continue;
            }

            // Admit the canonical claim (== the relayed one). A full queue is benign.
            let was_admitted = self.ctx.mining_manager().clone().submit_evm_deposit_claim(canonical);
            request.report_obtained();
            if was_admitted {
                admitted.push(request.req);
            }
        }

        // Re-announce only what we admitted (precheck-then-relay, §14.2).
        self.ctx.broadcast_evm_deposit_claims(admitted).await;
        Ok(())
    }
}

/// Flow listening to RequestEvmDepositClaims messages, responding with the
/// requested claims when queued locally.
pub struct RequestedEvmDepositClaimsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestedEvmDepositClaimsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestedEvmDepositClaimsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestEvmDepositClaims)?;
            if msg.outpoints.len() > MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG {
                return Err(ProtocolError::Other("Number of outpoints in deposit-claim request message is over the limit"));
            }
            for o in msg.outpoints {
                let outpoint = outpoint_from_wire(o)?;
                if let Some(claim) = self.ctx.mining_manager().clone().get_evm_deposit_claim(&outpoint) {
                    let claim = borsh::to_vec(&claim).expect("DepositClaim borsh serialization is infallible");
                    self.router.enqueue(make_message!(Payload::EvmDepositClaim, EvmDepositClaimMessage { claim })).await?;
                } else {
                    self.router
                        .enqueue(make_message!(
                            Payload::EvmDepositClaimNotFound,
                            EvmDepositClaimNotFoundMessage { outpoint: Some((&outpoint).into()) }
                        ))
                        .await?;
                }
            }
        }
    }
}
