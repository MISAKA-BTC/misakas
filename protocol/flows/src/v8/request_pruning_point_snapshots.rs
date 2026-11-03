//! kaspa-pq ADR-0022: serve the pruning point's EVM + DNS/PoS-v2 overlay snapshots
//! to a peer performing headers-proof IBD. The snapshots are sent as borsh blobs
//! (the consensus types are already Borsh); `found = false` means this node has no
//! snapshot (overlay/EVM dormant or not yet captured), which the requester treats
//! as "peer cannot serve pruned-IBD on this network".

use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    snapshot_memory::{
        MAX_EVM_HEADER_BYTES, MAX_EVM_STATE_SNAPSHOT_BYTES, MAX_OVERLAY_SNAPSHOT_BYTES, encode_exact, encode_sidecar, encoded_size,
    },
};
use kaspa_consensus_core::BlockHash;
use kaspa_core::{debug, memory_budget::global_memory_budget};
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{
        KaspadMessage, PruningPointEvmStateMessage, PruningPointOverlaySnapshotMessage, PruningPointPalwStateMessage,
        RequestPruningPointPalwStateMessage, kaspad_message::Payload,
    },
};
use std::{
    mem::size_of,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

#[path = "snapshot_serve_diagnostics.rs"]
mod snapshot_serve_diagnostics;
use snapshot_serve_diagnostics::{ReportLimiter, ServeFailure};

fn report_palw_refusal(point: BlockHash, mode: &'static str, failure: ServeFailure) {
    static LIMITER: OnceLock<Mutex<ReportLimiter>> = OnceLock::new();
    let admitted =
        LIMITER.get_or_init(Mutex::default).lock().unwrap_or_else(|error| error.into_inner()).admit(mode, failure, Instant::now());
    if !admitted {
        return;
    }
    kaspa_core::warn!(
        "[palw-sidecar] serve refused point={point} mode={mode} stage={} reason={} io_kind={:?}; replying found=false",
        failure.stage,
        failure.reason,
        failure.io_kind
    );
}

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
            // **The asker is charged before the work, not after the egress** (mainnet audit H-1);
            // see `PalwGossipCenter::reserve_sidecar_serve` for why the reservation is the attempt
            // floor rather than the worst case.
            let peer = self.router.key();
            if !self.ctx.palw_gossip().reserve_sidecar_serve(peer) {
                debug!("[palw-sidecar] {peer} is out of serve allowance this window; answering not-found for {pp}");
                let refused = PruningPointEvmStateMessage { found: false, evm_header: vec![], evm_state_snapshot: vec![] };
                self.router.enqueue(make_message!(Payload::PruningPointEvmState, refused)).await?;
                continue;
            }
            let session = self.ctx.consensus().unguarded_session();
            // The getter can still materialize a cold source snapshot. This
            // admission covers the separate Borsh reply buffers and their owner.
            let encoded = session
                .spawn_blocking(move |c| {
                    let Some((header, snapshot)) = c.pruning_point_evm_state(pp) else {
                        return Ok(None);
                    };
                    let header_bytes = encoded_size(&header, MAX_EVM_HEADER_BYTES as u64)?;
                    let snapshot_bytes = encoded_size(&snapshot, MAX_EVM_STATE_SNAPSHOT_BYTES as u64)?;
                    encode_sidecar(global_memory_budget(), size_of::<KaspadMessage>(), &[header_bytes, snapshot_bytes], || {
                        Ok(PruningPointEvmStateMessage {
                            found: true,
                            evm_header: encode_exact(&header, header_bytes)?,
                            evm_state_snapshot: encode_exact(&snapshot, snapshot_bytes)?,
                        })
                    })
                    .map(Some)
                })
                .await;
            match encoded {
                Ok(Some((reply, lease))) => {
                    let served = (reply.evm_header.len() + reply.evm_state_snapshot.len()) as u64;
                    self.ctx.palw_gossip().charge_sidecar_serve(peer, served);
                    match self
                        .router
                        .enqueue_preallocated(make_message!(Payload::PruningPointEvmState, reply), lease, Vec::new())
                        .await
                    {
                        Err(ProtocolError::LocalTransportCapacityUnavailable(_)) => {}
                        result => {
                            result?;
                            continue;
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => debug!("[palw-sidecar] EVM reply construction refused point={pp} kind={:?}", error.kind()),
            }
            self.router.enqueue(make_message!(Payload::PruningPointEvmState, PruningPointEvmStateMessage::default())).await?;
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
            // **The asker is charged before the work, not after the egress** (mainnet audit H-1);
            // see `PalwGossipCenter::reserve_sidecar_serve` for why the reservation is the attempt
            // floor rather than the worst case.
            let peer = self.router.key();
            if !self.ctx.palw_gossip().reserve_sidecar_serve(peer) {
                debug!("[palw-sidecar] {peer} is out of serve allowance this window; answering not-found for {pp}");
                let refused = PruningPointOverlaySnapshotMessage { found: false, overlay_snapshot: vec![] };
                self.router.enqueue(make_message!(Payload::PruningPointOverlaySnapshot, refused)).await?;
                continue;
            }
            let session = self.ctx.consensus().unguarded_session();
            // The persisted snapshot is the as-of-current-pruning-point one; only serve it when
            // it matches the requested pruning point (otherwise the requester's c==v would fail).
            let encoded = session
                .spawn_blocking(move |c| {
                    let Some(s) = c.pruning_point_overlay_snapshot().filter(|s| s.pruning_point == pp) else {
                        return Ok(None);
                    };
                    let bytes = encoded_size(&s.snapshot, MAX_OVERLAY_SNAPSHOT_BYTES as u64)?;
                    encode_sidecar(global_memory_budget(), size_of::<KaspadMessage>(), &[bytes], || {
                        Ok(PruningPointOverlaySnapshotMessage { found: true, overlay_snapshot: encode_exact(&s.snapshot, bytes)? })
                    })
                    .map(Some)
                })
                .await;
            match encoded {
                Ok(Some((reply, lease))) => {
                    self.ctx.palw_gossip().charge_sidecar_serve(peer, reply.overlay_snapshot.len() as u64);
                    match self
                        .router
                        .enqueue_preallocated(make_message!(Payload::PruningPointOverlaySnapshot, reply), lease, Vec::new())
                        .await
                    {
                        Err(ProtocolError::LocalTransportCapacityUnavailable(_)) => {}
                        result => {
                            result?;
                            continue;
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => debug!("[palw-sidecar] overlay reply construction refused point={pp} kind={:?}", error.kind()),
            }
            self.router
                .enqueue(make_message!(Payload::PruningPointOverlaySnapshot, PruningPointOverlaySnapshotMessage::default()))
                .await?;
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
    chunks: Option<crate::palw_snapshot_chunks::SnapshotSender>,
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
        Self { ctx, router, incoming_route, chunks: None }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // A cached spill expires even if this peer stays connected but sends no more requests.
            let msg = match tokio::time::timeout(std::time::Duration::from_secs(30), async {
                dequeue!(self.incoming_route, Payload::RequestPruningPointPalwState)
            })
            .await
            {
                Ok(result) => result?,
                Err(_) => {
                    if self.chunks.as_ref().is_some_and(|s| s.expired()) {
                        self.chunks = None;
                    }
                    continue;
                }
            };
            if msg.row_stream_generation > kaspa_consensus_core::palw_row_stream_v1::PALW_ROW_STREAM_CURRENT_GENERATION_V1
                || (msg.row_stream_generation != 0
                    && !crate::flow_context::palw_row_stream_negotiated_v1(self.router.properties().protocol_version))
            {
                // One current generation only. A larger id is a history request and is not served.
                debug!(
                    "[palw-sidecar] refusing row-stream generation {}: only the current generation is served",
                    msg.row_stream_generation
                );
                let refused = if msg.chunked {
                    PruningPointPalwStateMessage { chunked: true, ..Default::default() }
                } else {
                    PruningPointPalwStateMessage { found: false, palw_state: vec![], class_carriages: vec![], ..Default::default() }
                };
                self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                continue;
            }
            if msg.chunked {
                self.serve_chunked(msg).await?;
                continue;
            }
            if !msg.snapshot_root.is_empty() || msg.chunk_index != 0 || msg.snapshot_codec != 0 || !msg.snapshot_nonce.is_empty() {
                return Err(ProtocolError::Other("legacy PALW snapshot request carries chunk fields"));
            }
            let pp = req_pruning_point(msg.pruning_point_hash)?;
            // **The asker is charged before the work, not after the egress** (mainnet audit H-1,
            // the same ordering audit3 H6 imposed on the material lane). This loop had no budget,
            // no cost accounting and no rate limit of any kind, and it answered forty bytes with a
            // full PALW-state materialization and a multi-megabyte blob — reachable by any peer
            // that completed the handshake on the p2p port, with no `--profile public-node-rpc`
            // and no operator opt-in. The reservation is the attempt floor, so an honest IBD's one
            // request per window is never the one refused; see `reserve_sidecar_serve`.
            let peer = self.router.key();
            if !self.ctx.palw_gossip().reserve_sidecar_serve(peer) {
                debug!("[palw-sidecar] {peer} is out of serve allowance this window; answering not-found for {pp}");
                report_palw_refusal(pp, "legacy", ServeFailure::new("admission", "serve_allowance"));
                let refused =
                    PruningPointPalwStateMessage { found: false, palw_state: vec![], class_carriages: vec![], ..Default::default() };
                self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                continue;
            }
            // Legacy peers share the same node-wide materialization/session backstop.
            // The session/native lease follows the response through the final HTTP
            // Bytes owner, including a stalled peer after dequeue and compression.
            let mut snapshot_lease = match crate::palw_snapshot_chunks::SnapshotSpill::new() {
                Ok(lease) => lease,
                Err(error) => {
                    report_palw_refusal(pp, "legacy", ServeFailure::spill(&error));
                    self.router
                        .enqueue(make_message!(Payload::PruningPointPalwState, PruningPointPalwStateMessage::default()))
                        .await?;
                    continue;
                }
            };
            // Admit the complete legacy serialization before creating its Vecs.
            // The native state itself has a separate native-resource reservation.
            let Ok(snapshot_bytes) = kaspa_core::memory_budget::global_memory_budget().try_reserve(
                kaspa_core::memory_budget::MemoryClass::Snapshot,
                crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 + (1 << 20),
            ) else {
                report_palw_refusal(pp, "legacy", ServeFailure::new("admission", "snapshot_byte_budget"));
                self.router.enqueue(make_message!(Payload::PruningPointPalwState, PruningPointPalwStateMessage::default())).await?;
                continue;
            };
            let session = self.ctx.consensus().unguarded_session();
            // The state AND the declarations its classes were registered under (ADR-0067
            // Decision 6). A pruned-syncing peer never walks the blocks whose acceptance wrote
            // those rows, so without this it holds classes it cannot serve. Collected in the same
            // blocking hop, because two hops could straddle a reorg and hand the peer a state and
            // a declaration set from different chain points.
            //
            // **The declarations are read only when there is a state to send them with.** They
            // used to be collected unconditionally and then thrown away in the `None` arm below,
            // so a peer naming a hash this node never captured bought a SECOND full tip
            // materialization for a field that was never populated.
            let resource_lease = session.clone().spawn_blocking(move |c| c.palw_reserve_native_snapshot_v1(pp)).await;
            match resource_lease {
                Ok(lease) => snapshot_lease.hold_native_resource_lease(lease),
                Err(_) => {
                    report_palw_refusal(pp, "legacy", ServeFailure::new("admission", "native_capacity"));
                    self.router
                        .enqueue(make_message!(Payload::PruningPointPalwState, PruningPointPalwStateMessage::default()))
                        .await?;
                    continue;
                }
            }
            let (carriage, class_carriages, snapshot_bytes, snapshot_lease) = session
                .spawn_blocking(move |c| {
                    // Capture BOTH guards in the blocking owner. Dropping an
                    // awaiting flow must not release them while this task runs.
                    let mut snapshot_bytes = snapshot_bytes;
                    let answer = (|| {
                        let state = c
                            .try_pruning_point_palw_state_shared_v1(pp)
                            .map_err(|error| ServeFailure::new("snapshot_read", error.code()))?
                            .ok_or(ServeFailure::new("snapshot_read", "not_available"))?;
                        let cap = crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64;
                        let state_bytes = crate::snapshot_memory::encoded_size(&*state, cap)
                            .map_err(|error| ServeFailure::io("canonical_size", &error))?;
                        let remaining = cap
                            .checked_sub(state_bytes)
                            .and_then(|n| n.checked_sub(14))
                            .ok_or_else(|| ServeFailure::new("canonical_size", "envelope_cap"))?;
                        snapshot_bytes
                            .grow(
                                crate::snapshot_memory::declaration_metadata_bytes(state.classes.len())
                                    .map_err(|error| ServeFailure::io("declaration_metadata", &error))?,
                            )
                            .map_err(|_| ServeFailure::new("declaration_metadata", "snapshot_byte_budget"))?;
                        let declarations = c
                            .palw_class_carriages_for_snapshot_bounded_v1(&state, false, remaining)
                            .map_err(|error| ServeFailure::declaration(&error))?;
                        let encoded = crate::palw_snapshot_chunks::encode_legacy_snapshot(&state, &declarations)
                            .map_err(|error| ServeFailure::encoding(&error))?;
                        Ok::<_, ServeFailure>((encoded, declarations))
                    })();
                    let (carriage, declarations) = match answer {
                        Ok((bytes, declarations)) => (Some(bytes), declarations),
                        Err(error) => {
                            report_palw_refusal(pp, "legacy", error);
                            (None, Vec::new())
                        }
                    };
                    // Tuple fields free actual data before generation tickets if
                    // the async receiver was cancelled while this task ran.
                    (carriage, declarations, snapshot_bytes, snapshot_lease)
                })
                .await;
            let reply = match carriage {
                Some(c) => PruningPointPalwStateMessage {
                    found: true,
                    palw_state: c,
                    class_carriages: class_carriages
                        .into_iter()
                        .map(|(class_id, carriage)| kaspa_p2p_lib::pb::PalwClassCarriageEntry {
                            class_id: Some(class_id.into()),
                            carriage,
                        })
                        .collect(),
                    ..Default::default()
                },
                None => {
                    PruningPointPalwStateMessage { found: false, palw_state: vec![], class_carriages: vec![], ..Default::default() }
                }
            };
            // **Lane PA, B-F2: the serving side holds the requester's own cap.** A carriage the requester would refuse after the whole
            // transfer is answered `found: false` instead (the same answer an unprepared peer gives, so the requester tries another
            // peer at once), and a state over half the cap is logged where an operator reads it, long before it matters.
            let reply = {
                let bytes = crate::palw_snapshot_chunks::snapshot_payload_size(
                    reply.palw_state.len() as u64,
                    reply.class_carriages.iter().map(|e| e.carriage.len() as u64),
                )
                .ok();
                if reply.found && bytes.is_none() {
                    report_palw_refusal(pp, "legacy", ServeFailure::new("canonical_size", "envelope_cap"));
                    PruningPointPalwStateMessage { found: false, palw_state: vec![], class_carriages: vec![], ..Default::default() }
                } else {
                    if let Some(bytes) = bytes
                        && bytes > crate::palw_gossip::PALW_PRUNING_STATE_WARN_BYTES as u64
                    {
                        kaspa_core::warn!(
                            "[palw-sidecar] the PALW snapshot for {pp} is {bytes} canonical bytes: past half of the {} byte total cap; \
                             protocol-106 chunking bounds transfer buffers but retains this total limit",
                            crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES
                        );
                    }
                    reply
                }
            };
            let served = crate::palw_snapshot_chunks::snapshot_payload_size(
                reply.palw_state.len() as u64,
                reply.class_carriages.iter().map(|e| e.carriage.len() as u64),
            )
            .unwrap_or(0); // A not-found reply has no state payload and spends only the attempt floor.
            snapshot_bytes.materialized(); // Includes protobuf class-entry conversion above.
            self.ctx.palw_gossip().charge_sidecar_serve(peer, served);
            self.router
                .enqueue_preallocated(
                    make_message!(Payload::PruningPointPalwState, reply),
                    snapshot_bytes,
                    vec![Box::new(snapshot_lease)],
                )
                .await?;
        }
    }
    async fn serve_chunked(&mut self, msg: RequestPruningPointPalwStateMessage) -> Result<(), ProtocolError> {
        use crate::palw_snapshot_chunks::{SNAPSHOT_CODEC_ROWS_V1, SnapshotSender, SnapshotSpill};
        if self.router.properties().protocol_version < crate::flow_context::PROTOCOL_VERSION_PALW_SNAPSHOT_CHUNKS {
            return Err(ProtocolError::Other("PALW snapshot chunks require negotiated protocol 106"));
        }
        if (msg.row_stream_generation == 1) != (msg.snapshot_codec == SNAPSHOT_CODEC_ROWS_V1) {
            return Err(ProtocolError::Other("row generation requires protocol109 codec2"));
        }
        let pp = req_pruning_point(msg.pruning_point_hash)?;
        let codec = msg.snapshot_codec;
        let nonce = crate::palw_snapshot_chunks::compression_request_nonce(
            self.router.properties().protocol_version,
            msg.snapshot_codec,
            &msg.snapshot_nonce,
        )
        .map_err(|_| ProtocolError::Other("PALW snapshot compression/nonce was not negotiated"))?;
        let root = if msg.snapshot_root.is_empty() {
            None
        } else {
            Some(kaspa_consensus_core::Hash64::from_bytes(
                msg.snapshot_root.as_slice().try_into().map_err(|_| ProtocolError::Other("invalid PALW snapshot stream root"))?,
            ))
        };
        if self.chunks.as_ref().is_some_and(|s| s.expired()) {
            // This is not a refusal by itself: a fresh stream may be admitted.
            kaspa_core::debug!("[palw-sidecar] expired chunk sender discarded point={pp}");
            self.chunks = None;
        }
        if self.chunks.is_none() {
            if (msg.chunk_index == 0) != root.is_none() {
                return Err(ProtocolError::Other("PALW snapshot start/resume root and index disagree"));
            }
            let peer = self.router.key();
            let refused = PruningPointPalwStateMessage { chunked: true, ..Default::default() };
            if !self.ctx.palw_gossip().reserve_sidecar_serve(peer) {
                report_palw_refusal(pp, "chunked", ServeFailure::new("admission", "serve_allowance"));
                self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                return Ok(());
            }
            // Acquire the node-wide lease BEFORE the consensus snapshot clone or serialization.
            let mut spill = match SnapshotSpill::new() {
                Ok(spill) => spill,
                Err(error) => {
                    report_palw_refusal(pp, "chunked", ServeFailure::spill(&error));
                    self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                    return Ok(());
                }
            };
            let session = self.ctx.consensus().unguarded_session();
            let sender = session
                .spawn_blocking(move |c| {
                    match c.palw_reserve_native_snapshot_v1(pp) {
                        Ok(lease) => spill.hold_native_resource_lease(lease),
                        Err(_) => return Err(ServeFailure::new("admission", "native_capacity")),
                    }
                    let carriage = c
                        .try_pruning_point_palw_state_shared_v1(pp)
                        .map_err(|error| ServeFailure::new("snapshot_read", error.code()))?
                        .ok_or(ServeFailure::new("snapshot_read", "not_available"))?;
                    let declarations = c
                        .palw_class_carriages_for_snapshot_v1(&carriage, nonce.is_some())
                        .map_err(|error| ServeFailure::declaration(&error))?;
                    match nonce {
                        Some(nonce) if codec == SNAPSHOT_CODEC_ROWS_V1 => {
                            SnapshotSender::encode_row_snapshot(spill, pp, &carriage, &declarations, nonce, c)
                        }
                        Some(nonce) => SnapshotSender::encode_snapshot_compressed(spill, pp, &carriage, &declarations, nonce),
                        None => SnapshotSender::encode_snapshot(spill, pp, &carriage, &declarations),
                    }
                    .map_err(|error| ServeFailure::encoding(&error))
                })
                .await;
            let mut sender = match sender {
                Ok(sender) => sender,
                Err(failure) => {
                    report_palw_refusal(pp, "chunked", failure);
                    self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                    return Ok(());
                }
            };
            // Reserve/charge this ONE traversal exactly like the old whole-message response.
            // Continuations may spend that traversal, but cannot rewind or duplicate a chunk.
            // Compression must not make a large reconstruction cheap to repeat.
            // Charge the larger canonical traversal/wire size before continuation.
            let traversed = sender.total.max(sender.profile.map_or(0, |p| p.decoded));
            self.ctx.palw_gossip().charge_sidecar_serve(peer, traversed);
            if let Some(root) = root
                && let Err(error) = sender.resume(pp, root, msg.chunk_index)
            {
                report_palw_refusal(pp, "chunked", ServeFailure::io("resume", &error));
                self.router.enqueue(make_message!(Payload::PruningPointPalwState, refused)).await?;
                return Ok(());
            }
            self.chunks = Some(sender);
        }
        let sender = self.chunks.as_mut().unwrap();
        if sender.profile.map(|p| p.nonce) != nonce || sender.profile.map_or(0, |p| p.codec) != codec {
            return Err(ProtocolError::Other("PALW snapshot nonce/codec changed during stream"));
        }
        let (reply, _chunk_lease) = sender.reply_message(pp, root, msg.chunk_index).map_err(|why| match why {
            crate::palw_snapshot_chunks::NativeSnapshotDecodeErrorV1::Capacity(why) => {
                ProtocolError::LocalNativeCapacityUnavailable(why)
            }
            crate::palw_snapshot_chunks::NativeSnapshotDecodeErrorV1::LocalStorage(why) => {
                ProtocolError::LocalNativeResourceUnavailable(format!("PALW snapshot sender spill: {why}"))
            }
            crate::palw_snapshot_chunks::NativeSnapshotDecodeErrorV1::Encoding(_) => {
                ProtocolError::Other("invalid PALW snapshot point/root/order")
            }
        })?;
        if sender.next_index() == sender.count {
            self.chunks = None;
        }
        self.router.enqueue(make_message!(Payload::PruningPointPalwState, reply)).await?;
        Ok(())
    }
}
