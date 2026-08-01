//! ADR-MA §21.4 catch-up delivery, serve side (protocol ≥ 104): ship the complete
//! content-addressed Compute Set registry record set to a syncer BEFORE it downloads headers.
//!
//! Catch-up IBD validates ALL headers before replaying any body, but on a registry-active net a
//! v5 algo-4 header's §13.2 resolution and its mergers' GHOSTDAG §14 credit both need the exact
//! records the header commits — records that are only folded from ACCEPTED transactions (bodies)
//! or imported with a pruning-point snapshot. Young nets whose pruning point is still genesis
//! never take the snapshot path, so without this pre-delivery every fresh sync fail-stops at the
//! first algo-4 header (`unknown compute_set_id … no registered descriptor` — observed live on
//! testnet-20, 2026-08-01, by every fresh-syncing participant).
//!
//! The records are self-authenticating (each tier is keyed by its recomputed content hash), so a
//! peer can at worst pre-supply preimages body replay would admit later; the fork-local VIEW is
//! never transported on this path (see `import_palw_compute_registry_records_package`).

use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{PalwComputeRegistryRecordsMessage, kaspad_message::Payload},
};
use std::sync::Arc;

pub struct RequestPalwComputeRegistryRecordsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPalwComputeRegistryRecordsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        loop {
            dequeue!(self.incoming_route, Payload::RequestPalwComputeRegistryRecords)?;
            let session = self.ctx.consensus().unguarded_session();
            let package = session.spawn_blocking(move |c| c.palw_compute_registry_records_package()).await;
            let reply = PalwComputeRegistryRecordsMessage {
                package: borsh::to_vec(&package).expect("compute registry records package Borsh is infallible"),
            };
            self.router.enqueue(make_message!(Payload::PalwComputeRegistryRecords, reply)).await?;
        }
    }
}

impl RequestPalwComputeRegistryRecordsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }
}
