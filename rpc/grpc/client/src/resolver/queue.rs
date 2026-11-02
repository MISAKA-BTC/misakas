use crate::{
    error::{Error, Result},
    resolver::{KaspadResponseReceiver, KaspadResponseSender, Resolver, matcher::Matcher},
};
use kaspa_core::trace;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use kaspa_utils_tower::transport::RetainedSize;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::oneshot;

#[derive(Debug)]
struct Pending {
    timestamp: Instant,
    op: KaspadPayloadOps,
    request: KaspadRequest,
    sender: KaspadResponseSender,
    _memory: kaspa_core::memory_budget::MemoryLease,
}

impl Pending {
    fn new(
        op: KaspadPayloadOps,
        request: KaspadRequest,
        sender: KaspadResponseSender,
        memory: kaspa_core::memory_budget::MemoryLease,
    ) -> Self {
        Self { timestamp: Instant::now(), op, request, sender, _memory: memory }
    }

    fn is_matching(&self, response: &KaspadResponse, response_op: KaspadPayloadOps) -> bool {
        self.op == response_op && self.request.is_matching(response)
    }
}

#[derive(Debug)]
pub(crate) struct QueueResolver {
    pending_calls: Arc<Mutex<VecDeque<Pending>>>,
}

impl QueueResolver {
    pub(crate) fn new() -> Self {
        Self { pending_calls: Arc::new(Mutex::new(VecDeque::new())) }
    }
}

impl Resolver for QueueResolver {
    fn register_request(&self, op: KaspadPayloadOps, request: &KaspadRequest) -> Result<KaspadResponseReceiver> {
        // The compatibility resolver really clones the request. Charge the
        // physical copy before clone; sharing the outgoing lease would undercount.
        let bytes = request.retained_heap_bytes().saturating_add(std::mem::size_of::<Pending>() as u64).saturating_add(128);
        let memory = kaspa_core::memory_budget::global_memory_budget()
            .try_reserve(kaspa_core::memory_budget::MemoryClass::Transport, bytes)
            .map_err(|error| Error::TonicStatus(tonic::Status::resource_exhausted(error.to_string())))?;
        let (sender, receiver) = oneshot::channel::<Result<KaspadResponse>>();
        {
            let mut pending_calls = self.pending_calls.lock().unwrap();
            if pending_calls.len() >= super::MAX_PENDING_REQUESTS {
                return Err(Error::ChannelSendError);
            }
            let pending = Pending::new(op, request.clone(), sender, memory.clone());
            pending_calls.push_back(pending);
            memory.materialized();
            drop(pending_calls);
        }
        Ok(receiver)
    }

    fn cancel_request(&self, request_id: u64) {
        let mut pending = self.pending_calls.lock().unwrap();
        if let Some(index) = pending.iter().position(|call| call.request.id == request_id) {
            pending.remove(index);
        }
    }

    fn handle_response(&self, response: KaspadResponse) {
        let response_op: KaspadPayloadOps = response.payload.as_ref().unwrap().try_into().expect("response is not a notification");
        trace!("[Resolver] handle_response type: {:?}", response_op);
        let mut pending_calls = self.pending_calls.lock().unwrap();
        let mut pending: Option<Pending> = None;
        if pending_calls.front().is_some() {
            if pending_calls.front().unwrap().is_matching(&response, response_op) {
                pending = pending_calls.pop_front();
            } else {
                let pending_slice = pending_calls.make_contiguous();
                // Iterate the queue front to back, so older pendings first
                for i in 0..pending_slice.len() {
                    if pending_calls.get(i).unwrap().is_matching(&response, response_op) {
                        pending = pending_calls.remove(i);
                        break;
                    }
                }
            }
        }
        drop(pending_calls);
        if let Some(pending) = pending {
            trace!("[Resolver] handle_response matching request found: {:?}", pending.request);
            match pending.sender.send(Ok(response)) {
                Ok(_) => {}
                Err(err) => {
                    trace!("[Resolver] handle_response failed to send the response of a pending: {:?}", err);
                }
            }
        }
    }

    fn remove_expired_requests(&self, timeout: std::time::Duration) {
        let mut pending_calls = self.pending_calls.lock().unwrap();
        let mut index: usize = 0;
        loop {
            if index >= pending_calls.len() {
                break;
            }
            let pending = pending_calls.get(index).unwrap();
            if pending.timestamp.elapsed() > timeout {
                let pending = pending_calls.remove(index).unwrap();
                match pending.sender.send(Err(Error::Timeout)) {
                    Ok(_) => {}
                    Err(err) => {
                        trace!("[Resolver] the timeout monitor failed to send a timeout error: {:?}", err);
                    }
                }
            } else {
                // The call to pending_calls.remove moves whichever end is closer to the
                // removal point. So to prevent skipping items, we only increment index when
                // no removal occurs.
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_copy_is_admitted_and_cancellation_drops_its_ticket() {
        let resolver = Arc::new(QueueResolver::new());
        let request = KaspadRequest {
            id: 17,
            payload: Some(kaspa_grpc_core::protowire::kaspad_request::Payload::GetBlockRequest(
                kaspa_grpc_core::protowire::GetBlockRequestMessage { hash: "a".repeat(1 << 20), include_transactions: true },
            )),
        };
        let receiver = resolver.register_request(KaspadPayloadOps::GetBlock, &request).unwrap();
        assert!(resolver.pending_calls.lock().unwrap().front().unwrap()._memory.bytes() >= 1 << 20);
        let guard = crate::resolver::PendingRequestGuard::new(resolver.clone(), request.id);
        drop(guard);
        assert!(resolver.pending_calls.lock().unwrap().is_empty());
        assert!(receiver.blocking_recv().is_err());
    }
    #[test]
    fn sent_request_keeps_its_matching_position_when_caller_cancels() {
        let resolver = Arc::new(QueueResolver::new());
        let request = KaspadRequest { id: 18, ..Default::default() };
        let receiver = resolver.register_request(KaspadPayloadOps::GetInfo, &request).unwrap();
        let mut guard = crate::resolver::PendingRequestGuard::new(resolver.clone(), request.id);
        guard.sent();
        drop(receiver);
        drop(guard);
        assert_eq!(resolver.pending_calls.lock().unwrap().front().unwrap().request.id, 18);
    }
    #[test]
    fn cancelled_sent_call_consumes_its_late_same_op_reply_before_next_call() {
        use kaspa_grpc_core::protowire::{GetInfoRequestMessage, GetInfoResponseMessage, kaspad_request, kaspad_response};
        let resolver = Arc::new(QueueResolver::new());
        let request_a = KaspadRequest { id: 21, payload: Some(kaspad_request::Payload::GetInfoRequest(GetInfoRequestMessage {})) };
        let receiver_a = resolver.register_request(KaspadPayloadOps::GetInfo, &request_a).unwrap();
        let mut guard_a = crate::resolver::PendingRequestGuard::new(resolver.clone(), request_a.id);
        guard_a.sent();
        drop(receiver_a);
        drop(guard_a);
        let request_b = KaspadRequest { id: 22, ..request_a };
        let mut receiver_b = resolver.register_request(KaspadPayloadOps::GetInfo, &request_b).unwrap();
        resolver.handle_response(KaspadResponse {
            id: 701,
            payload: Some(kaspad_response::Payload::GetInfoResponse(GetInfoResponseMessage::default())),
        });
        assert!(matches!(receiver_b.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty)));
        assert_eq!(resolver.pending_calls.lock().unwrap().front().unwrap().request.id, 22);
        resolver.handle_response(KaspadResponse {
            id: 702,
            payload: Some(kaspad_response::Payload::GetInfoResponse(GetInfoResponseMessage::default())),
        });
        assert_eq!(receiver_b.blocking_recv().unwrap().unwrap().id, 702);
    }
}
