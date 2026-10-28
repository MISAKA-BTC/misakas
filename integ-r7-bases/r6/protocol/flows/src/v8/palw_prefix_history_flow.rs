//! Direct, bounded public archive reads. A peer cannot substitute its tip for the
//! requester's rooted history head and responses are never relayed.
use crate::{
    flow_context::{FlowContext, PROTOCOL_VERSION_PALW_PREFIX_HISTORY},
    flow_trait::Flow,
};
use kaspa_consensus_core::palw_prefix_history_v1::{PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRequestV1};
use kaspa_hashes::Hash64;
use kaspa_p2p_lib::{IncomingRoute, Router, common::ProtocolError, make_message, pb::kaspad_message::Payload};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub struct PalwPrefixHistoryFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming: IncomingRoute,
}
impl PalwPrefixHistoryFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming: IncomingRoute) -> Self {
        Self { ctx, router, incoming }
    }
}

#[async_trait::async_trait]
impl Flow for PalwPrefixHistoryFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }
    async fn start(&mut self) -> Result<(), ProtocolError> {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut served_at = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if !self.ctx.palw_v2_active() { continue; }
                    for request in self.ctx.palw_prefix_history().due_v1(Instant::now()) {
                        let message = make_message!(Payload::PalwPrefixHistoryRequest,
                            kaspa_p2p_lib::pb::PalwPrefixHistoryRequestMessage { request: borsh::to_vec(&request).unwrap() });
                        self.ctx.hub().broadcast_to_peers_with_min_version(message, PROTOCOL_VERSION_PALW_PREFIX_HISTORY).await;
                    }
                }
                message = self.incoming.recv() => {
                    let message = message.ok_or(ProtocolError::ConnectionClosed)?;
                    if !self.ctx.palw_v2_active() { continue; }
                    match message.payload {
                        Some(Payload::PalwPrefixHistoryRecord(response)) => {
                            if let Some(id) = response.context_id.and_then(|id| Hash64::try_from(id).ok()) {
                                self.ctx.palw_prefix_history().receive_v1(id, &response.record);
                            }
                        }
                        Some(Payload::PalwPrefixHistoryRequest(raw)) => {
                            // Charge the attempt before a DB read; do not accumulate 90k responses
                            // in the router's much deeper generic outgoing queue.
                            if raw.request.len() > 512 || served_at.elapsed() < Duration::from_millis(250)
                                || self.router.queued_outgoing_messages_v1() >= 2 { continue; }
                            served_at = Instant::now();
                            let Ok(request) = borsh::from_slice::<PalwPrefixHistoryRequestV1>(&raw.request) else { continue };
                            let Ok(id) = request.context_id_v1() else { continue };
                            let Some(read_permit) = self.ctx.palw_prefix_history().try_public_read_v1() else { continue };
                            let session = self.ctx.consensus().unguarded_session();
                            let record = session.spawn_blocking(move |c| {
                                // A cancelled flow cannot free capacity while its blocking DB read still runs.
                                let _read_permit = read_permit;
                                c.palw_prefix_history_record_current_v1(request).ok()?
                            }).await;
                            if let Some(record) = record {
                                let Ok(raw) = borsh::to_vec(&record) else { continue };
                                if raw.len() > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 { continue; }
                                self.router.enqueue(make_message!(Payload::PalwPrefixHistoryRecord,
                                    kaspa_p2p_lib::pb::PalwPrefixHistoryRecordMessage { context_id: Some(id.into()), record: raw })).await?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
