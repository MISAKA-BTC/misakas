use super::error::Result;
use core::fmt::Debug;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: KaspadPayloadOps, request: &KaspadRequest) -> Result<KaspadResponseReceiver>;
    fn cancel_request(&self, request_id: u64);
    fn handle_response(&self, response: KaspadResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

/// Covers local admission failure before the request is sent. Once sent, the
/// legacy resolver must retain its FIFO position until response/timeout; deleting
/// it on future cancellation could deliver its late response to the next call.
pub(crate) struct PendingRequestGuard {
    resolver: DynResolver,
    request_id: u64,
    armed: bool,
}
impl PendingRequestGuard {
    pub(crate) fn new(resolver: DynResolver, request_id: u64) -> Self {
        Self { resolver, request_id, armed: true }
    }
    pub(crate) fn sent(&mut self) {
        self.armed = false;
    }
}
impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        if self.armed {
            self.resolver.cancel_request(self.request_id);
        }
    }
}

pub(crate) const MAX_PENDING_REQUESTS: usize = 4096;

pub(crate) type KaspadResponseSender = oneshot::Sender<Result<KaspadResponse>>;
pub(crate) type KaspadResponseReceiver = oneshot::Receiver<Result<KaspadResponse>>;
