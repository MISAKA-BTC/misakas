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
    pb::{PruningPointEvmStateMessage, PruningPointOverlaySnapshotMessage, PruningPointPalwStateMessage, kaspad_message::Payload},
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

/// **Serve the pruning point's PALW V2 chain state** (launch blockers §1).
///
/// A node joining by pruned IBD had no `PalwChainStateV2` at all — `process_genesis` is its only
/// writer — and absent state was read as "no policy", silently disabling every PALW consensus rule.
/// This is the half that lets such a node acquire it.
///
/// Served only when the stored tip really names the requested point: the store holds ONE
/// materialized snapshot, and the requester verifies the carriage against that point's own header,
/// so a mismatched answer would be refused there anyway. `found: false` is the honest reply, and on
/// a network with no V2 ruleset it is the only one.
pub struct RequestPruningPointPalwStateFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointPalwStateFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointPalwStateFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestPruningPointPalwState)?;
            let pp = req_pruning_point(msg.pruning_point_hash)?;
            let session = self.ctx.consensus().unguarded_session();
            // The state AND the declarations its classes were registered under (ADR-0067
            // Decision 6). A pruned-syncing peer never walks the blocks whose acceptance wrote
            // those rows, so without this it holds classes it cannot serve. Collected in the same
            // blocking hop, because two hops could straddle a reorg and hand the peer a state and
            // a declaration set from different chain points.
            let (carriage, class_carriages) =
                session.spawn_blocking(move |c| (c.pruning_point_palw_state(pp), c.palw_class_carriages_for_sync_v1())).await;
            let reply = match carriage {
                Some(c) => PruningPointPalwStateMessage {
                    found: true,
                    palw_state: borsh::to_vec(&c).expect("PalwStateCarriageV2 borsh is infallible"),
                    class_carriages: class_carriages
                        .into_iter()
                        .map(|(class_id, carriage)| kaspa_p2p_lib::pb::PalwClassCarriageEntry {
                            class_id: Some(class_id.into()),
                            carriage,
                        })
                        .collect(),
                },
                None => PruningPointPalwStateMessage { found: false, palw_state: vec![], class_carriages: vec![] },
            };
            self.router.enqueue(make_message!(Payload::PruningPointPalwState, reply)).await?;
        }
    }
}
