//! kaspa-pq ADR-0022: serve the pruning point's EVM + DNS/PoS-v2 overlay snapshots
//! to a peer performing headers-proof IBD. The snapshots are sent as borsh blobs
//! (the consensus types are already Borsh); `found = false` means this node has no
//! snapshot (overlay/EVM dormant or not yet captured), which the requester treats
//! as "peer cannot serve pruned-IBD on this network".

use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_consensus_core::BlockHash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{PruningPointEvmStateMessage, PruningPointOverlaySnapshotMessage, kaspad_message::Payload},
};
use std::sync::Arc;

/// Extract the requested pruning-point hash from a `Request*` message's `Hash` field.
fn req_pruning_point(hash: Option<kaspa_p2p_lib::pb::Hash>) -> Result<BlockHash, ProtocolError> {
    BlockHash::try_from(hash.ok_or(ProtocolError::Other("snapshot request is missing the pruning point hash"))?)
        .map_err(|_| ProtocolError::Other("snapshot request carries an invalid pruning point hash"))
}

pub struct RequestPruningPointEvmStateFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointEvmStateFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointEvmStateFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestPruningPointEvmState)?;
            let pp = req_pruning_point(msg.pruning_point_hash)?;
            let session = self.ctx.consensus().unguarded_session();
            let evm = session.spawn_blocking(move |c| c.pruning_point_evm_state(pp)).await;
            let reply = match evm {
                Some((header, snapshot)) => PruningPointEvmStateMessage {
                    found: true,
                    evm_header: borsh::to_vec(&header).expect("EvmExecutionHeader borsh is infallible"),
                    evm_state_snapshot: borsh::to_vec(&snapshot).expect("EvmStateSnapshot borsh is infallible"),
                },
                None => PruningPointEvmStateMessage { found: false, evm_header: vec![], evm_state_snapshot: vec![] },
            };
            self.router.enqueue(make_message!(Payload::PruningPointEvmState, reply)).await?;
        }
    }
}

pub struct RequestPruningPointOverlaySnapshotFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointOverlaySnapshotFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointOverlaySnapshotFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestPruningPointOverlaySnapshot)?;
            let pp = req_pruning_point(msg.pruning_point_hash)?;
            let session = self.ctx.consensus().unguarded_session();
            // The persisted snapshot is the as-of-current-pruning-point one; only serve it when
            // it matches the requested pruning point (otherwise the requester's c==v would fail).
            let snap = session.spawn_blocking(move |c| c.pruning_point_overlay_snapshot()).await;
            let reply = match snap {
                Some(s) if s.pruning_point == pp => PruningPointOverlaySnapshotMessage {
                    found: true,
                    overlay_snapshot: borsh::to_vec(&s.snapshot).expect("OverlaySnapshot borsh is infallible"),
                },
                _ => PruningPointOverlaySnapshotMessage { found: false, overlay_snapshot: vec![] },
            };
            self.router.enqueue(make_message!(Payload::PruningPointOverlaySnapshot, reply)).await?;
        }
    }
}
