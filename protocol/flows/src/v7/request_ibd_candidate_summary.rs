//! Answers "which chain are you on?" — the question a syncing node has to be able to ask.
//!
//! Serving this is what lets a peer mid-IBD learn what else is on offer without fetching blocks.
//! Everything here is already public: the virtual selected parent header is relayed to everyone
//! anyway, and the pruning point is served on request. Nothing here is a commitment or a promise —
//! it is this node reporting its own current view, which is exactly what the asker must treat it
//! as (see `ClaimedBlueWork`).

use std::sync::Arc;

use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    convert::header::HeaderFormat,
    dequeue_with_request_id, make_response,
    pb::{IbdCandidateSummaryMessage, kaspad_message::Payload},
};

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestIbdCandidateSummaryFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
    header_format: HeaderFormat,
}

#[async_trait::async_trait]
impl Flow for RequestIbdCandidateSummaryFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestIbdCandidateSummaryFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, header_format: HeaderFormat) -> Self {
        Self { ctx, router, incoming_route, header_format }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (_msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestIbdCandidateSummary)?;

            let session = self.ctx.consensus().session().await;
            let sink = session.async_get_sink().await;
            let header = session.async_get_header(sink).await?;
            let pruning_point = session.async_pruning_point().await;
            drop(session);

            self.router
                .enqueue(make_response!(
                    Payload::IbdCandidateSummary,
                    IbdCandidateSummaryMessage {
                        virtual_selected_parent: Some((self.header_format, header.as_ref()).into()),
                        pruning_point: Some(pruning_point.into()),
                        // Repeated from the handshake so the asker can reject on rules alone,
                        // without having to trust that the connection was checked elsewhere.
                        genesis_hash: self.ctx.config.genesis.hash.as_bytes().to_vec(),
                        consensus_params_id: self.ctx.config.params.consensus_params_id().as_bytes().to_vec(),
                    },
                    request_id
                ))
                .await?;
        }
    }
}
