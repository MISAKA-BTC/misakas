//! **The interval lane's wire** (ADR-0077 Decision 8; SA-2 and ADR-0079 SA-3 for the
//! authentication) — a free-prompt seat asks the executor for ONE checkpoint interval's opening
//! and is answered directly.
//!
//! Its own flow file rather than an arm of [`super::palw_gossip_flow`], because the two lanes are
//! different transports and share nothing but the center: the material lane is a BROADCAST that
//! every peer relays, and this one is a request whose answer exactly one peer wanted and nobody
//! relays. Keeping them apart also keeps the gossip flow's `match` from growing an arm whose
//! refusal semantics are the opposite of its neighbours' — there, a message that fails a check is
//! silently not relayed; here, a request that fails a check is refused BY NAME in this node's own
//! log, because "the seat was refused" and "the seat heard nothing" are different facts and only
//! one of them is the executor's fault.
//!
//! The refusal is never sent back. A server that answers a bad request with an error is a server
//! that answers an unbonded stranger — the amplifier SA-2 exists to close, one round trip smaller.
//!
//! On a network with no ConsensusV2 ruleset the lane does not exist: everything is dropped without
//! decoding.

use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_core::{debug, info};
use kaspa_hashes::Hash64;
use kaspa_p2p_lib::{IncomingRoute, Router, common::ProtocolError, make_message, pb::kaspad_message::Payload};
use std::sync::Arc;

use crate::palw_gossip::{PalwOpeningRequestV1, PalwServeRefusalV1};

pub struct PalwIntervalFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for PalwIntervalFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl PalwIntervalFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = self.incoming_route.recv().await.ok_or(ProtocolError::ConnectionClosed)?;
            if !self.ctx.palw_v2_active() {
                continue;
            }
            match msg.payload {
                Some(Payload::PalwIntervalOpeningRequest(inner)) => {
                    let Some(claim) = inner.claim_id.and_then(|h| Hash64::try_from(h).ok()) else {
                        continue; // an opening for no claim is addressed to nobody
                    };
                    let request = PalwOpeningRequestV1 {
                        claim,
                        interval_index: Some(inner.interval_index),
                        requested_daa: inner.requested_daa,
                        requester_pubkey: &inner.requester_pubkey,
                        signature: &inner.signature,
                    };
                    match self.ctx.palw_gossip().resolve_interval_opening_for_serve(self.router.key(), &request).await {
                        Ok(opening) => {
                            let msg = make_message!(
                                Payload::PalwIntervalOpening,
                                kaspa_p2p_lib::pb::PalwIntervalOpeningMessage {
                                    claim_id: Some(claim.into()),
                                    interval_index: inner.interval_index,
                                    opening,
                                }
                            );
                            let _ = self.router.enqueue(msg).await;
                        }
                        // Named, and only here. The claim id is a public chain fact; nothing about
                        // the prompt, its ids or its text is in this line (ADR-0077 SA-5,
                        // ADR-0079 SA-7).
                        // A refusal used to be a `debug!` line, which on a node run at the default
                        // level is no line: a seat asked node-0 for four intervals of a 300-token
                        // claim every 100 s for half an hour and node-0's log had nothing to say
                        // (devnet runs 8 and 10). A throttled re-ask is the seat's own cadence and
                        // stays quiet; every other refusal is the operator's to read.
                        Err(PalwServeRefusalV1::Throttled) => {
                            debug!("[palw-interval] throttled a re-ask for claim {claim} interval {}", inner.interval_index)
                        }
                        Err(refusal) => info!(
                            "[palw-interval] refused an opening request for claim {claim} interval {}: {}",
                            inner.interval_index,
                            refusal.name()
                        ),
                    }
                }
                // An answer to this node's own ask. Admitted only for a pair it is waiting on, size
                // capped before anything decodes it, and NEVER relayed — there is nobody else to
                // relay it to.
                Some(Payload::PalwIntervalOpening(inner)) => {
                    let Some(claim) = inner.claim_id.and_then(|h| Hash64::try_from(h).ok()) else {
                        continue;
                    };
                    let _ =
                        self.ctx.palw_gossip().admit_interval_opening(self.router.key(), claim, inner.interval_index, &inner.opening);
                }
                _ => {}
            }
        }
    }
}
