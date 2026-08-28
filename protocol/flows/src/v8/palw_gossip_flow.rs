//! The per-peer receiving end of the PALW material/receipt gossip — see
//! [`crate::palw_gossip`] for the design and the flood-control contract.
//!
//! The flow is thin on purpose: every decision (dedup, caps, per-claim budget) belongs to the one
//! shared [`PalwGossipCenter`](crate::palw_gossip::PalwGossipCenter), because a per-peer cache
//! would multiply an attacker's relay budget by the peer count. What the flow owns is the wire:
//! decode the hash, ask the center, and relay a `Fresh` message to every OTHER peer.
//!
//! On a network with no ConsensusV2 ruleset the band does not exist: everything is dropped without
//! decoding, and this node never re-broadcasts. Not a protocol error — a peer speaking it there is
//! wrong about the network, not about the protocol.

use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_hashes::Hash64;
use kaspa_p2p_lib::{IncomingRoute, Router, common::ProtocolError, make_message, pb::kaspad_message::Payload};
use std::sync::Arc;

use crate::palw_gossip::PalwGossipAdmit;

pub struct PalwGossipFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for PalwGossipFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl PalwGossipFlow {
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
                Some(Payload::PalwTraceMaterialBroadcast(inner)) => {
                    let Some(claim) = inner.claim_id.clone().and_then(|h| Hash64::try_from(h).ok()) else {
                        continue; // a material for no claim is addressed to nobody
                    };
                    if self.ctx.palw_gossip().admit_material(claim, &inner.material) == PalwGossipAdmit::Fresh {
                        let relay = make_message!(Payload::PalwTraceMaterialBroadcast, inner);
                        self.ctx.hub().broadcast(relay, Some(self.router.key())).await;
                    }
                }
                Some(Payload::PalwSeatReceiptBroadcast(inner)) => {
                    if self.ctx.palw_gossip().admit_receipt(&inner.receipt) == PalwGossipAdmit::Fresh {
                        let relay = make_message!(Payload::PalwSeatReceiptBroadcast, inner);
                        self.ctx.hub().broadcast(relay, Some(self.router.key())).await;
                    }
                }
                Some(Payload::PalwMaterialRequest(inner)) => {
                    let Some(claim) = inner.claim_id.and_then(|h| Hash64::try_from(h).ok()) else {
                        continue;
                    };
                    // **Answer the ASKER, not the neighbourhood** (audit M2-2). Serving by
                    // broadcast turned a ~70-byte request into up to 16 MiB per peer — with K
                    // planted claim ids cycled past the per-claim throttle, 4.5 KB of requests
                    // enqueued gigabytes of message clones, and the queue is deep enough
                    // (131,328) that back-pressure never arrives. The asker gets what it asked
                    // for; anyone else who needs it can ask, which is what the pull is for.
                    if let Some(bytes) = self.ctx.palw_gossip().resolve_material_for_serve(claim) {
                        let msg = make_message!(
                            Payload::PalwTraceMaterialBroadcast,
                            kaspa_p2p_lib::pb::PalwTraceMaterialBroadcastMessage { claim_id: Some(claim.into()), material: bytes }
                        );
                        let _ = self.router.enqueue(msg).await;
                    }
                }
                _ => {}
            }
        }
    }
}
