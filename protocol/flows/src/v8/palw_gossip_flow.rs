
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
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    make_message,
    pb::{PalwSeatReceiptBroadcastMessage, PalwTraceMaterialBroadcastMessage, kaspad_message::Payload},
};
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
                _ => {}
            }
        }
    }
}
