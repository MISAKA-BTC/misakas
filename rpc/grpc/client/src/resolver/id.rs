use crate::{
    error::{Error, Result},
    resolver::{KaspadResponseReceiver, KaspadResponseSender, Resolver},
};
use kaspa_core::trace;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::oneshot;

#[derive(Debug)]
struct Pending {
    timestamp: Instant,
    sender: KaspadResponseSender,
    _memory: kaspa_core::memory_budget::MemoryLease,
}

impl Pending {
    fn new(sender: KaspadResponseSender, memory: kaspa_core::memory_budget::MemoryLease) -> Self {
        Self { timestamp: Instant::now(), sender, _memory: memory }
    }
}

#[derive(Debug)]
pub(crate) struct IdResolver {
    pending_calls: Arc<Mutex<HashMap<u64, Pending>>>,
}

impl IdResolver {
    pub(crate) fn new() -> Self {
        Self { pending_calls: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl Resolver for IdResolver {
    fn register_request(&self, _: KaspadPayloadOps, request: &KaspadRequest) -> Result<KaspadResponseReceiver> {
        let memory = kaspa_core::memory_budget::global_memory_budget()
            .try_reserve(kaspa_core::memory_budget::MemoryClass::Transport, 512)
            .map_err(|error| Error::TonicStatus(tonic::Status::resource_exhausted(error.to_string())))?;
        let (sender, receiver) = oneshot::channel::<Result<KaspadResponse>>();
        {
            let mut pending_calls = self.pending_calls.lock().unwrap();
            if pending_calls.len() >= super::MAX_PENDING_REQUESTS {
                return Err(Error::ChannelSendError);
            }
            pending_calls.insert(request.id, Pending::new(sender, memory.clone()));
            memory.materialized();
            drop(pending_calls);
        }
        Ok(receiver)
    }

    fn cancel_request(&self, request_id: u64) {
        self.pending_calls.lock().unwrap().remove(&request_id);
    }

    fn handle_response(&self, response: KaspadResponse) {
        match self.pending_calls.lock().unwrap().remove(&response.id) {
            Some(pending) => {
                trace!("[Resolver] handle_response has matching request with id {}", response.id);
                match pending.sender.send(Ok(response)) {
                    Ok(_) => {}
                    Err(err) => {
                        trace!("[Resolver] handle_response failed to send the response of a pending request: {:?}", err);
                    }
                }
            }
            None => {
                trace!("[Resolver] handle_response: response id {} has no pending request", response.id);
            }
        }
    }

    fn remove_expired_requests(&self, timeout: std::time::Duration) {
        let mut pending_calls = self.pending_calls.lock().unwrap();
        let mut purge = Vec::<u64>::new();
        for (id, pending) in pending_calls.iter() {
            if pending.timestamp.elapsed() > timeout {
                purge.push(*id);
            }
        }
        for id in purge.iter() {
            let pending = pending_calls.remove(id).expect("the pending request to remove does exist in the map");
            match pending.sender.send(Err(Error::Timeout)) {
                Ok(_) => {}
                Err(err) => {
                    trace!("[Resolver] the timeout monitor failed to send a timeout error: {:?}", err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_and_cancelled_calls_do_not_remain_pending() {
        let resolver = Arc::new(IdResolver::new());
        let request = KaspadRequest { id: 7, ..Default::default() };
        let receiver = resolver.register_request(KaspadPayloadOps::GetInfo, &request).unwrap();
        let guard = crate::resolver::PendingRequestGuard::new(resolver.clone(), 7);
        assert_eq!(resolver.pending_calls.lock().unwrap().len(), 1);
        drop(guard); // queue rejection or cancellation of the call future
        assert!(resolver.pending_calls.lock().unwrap().is_empty());
        assert!(receiver.blocking_recv().is_err());
    }
    #[test]
    fn too_many_pending_calls_fail_without_wait_and_resume_after_cancel() {
        let resolver = IdResolver::new();
        let mut replies = Vec::new();
        for id in 0..super::super::MAX_PENDING_REQUESTS as u64 {
            replies.push(resolver.register_request(KaspadPayloadOps::GetInfo, &KaspadRequest { id, ..Default::default() }).unwrap());
        }
        assert!(resolver.register_request(KaspadPayloadOps::GetInfo, &KaspadRequest { id: 9000, ..Default::default() }).is_err());
        resolver.cancel_request(0);
        assert!(resolver.register_request(KaspadPayloadOps::GetInfo, &KaspadRequest { id: 9000, ..Default::default() }).is_ok());
        drop(replies);
    }
}
