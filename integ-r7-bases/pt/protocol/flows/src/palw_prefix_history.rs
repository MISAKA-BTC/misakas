//! Small public-record pulls, authenticated by the caller's accepted court head.
use kaspa_consensus_core::palw_prefix_history_v1::{
    PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1,
};
use kaspa_hashes::Hash64;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub const MAX_PENDING: usize = 16;
pub const MAX_CONCURRENT_PUBLIC_READS: usize = 2;
const IDLE_TTL: Duration = Duration::from_secs(30);
const RETRY: Duration = Duration::from_secs(3);
const DISPATCH_INTERVAL: Duration = Duration::from_millis(250);

struct Pending {
    request: PalwPrefixHistoryRequestV1,
    touched: Instant,
    sent: Option<Instant>,
    ready: Option<PalwPrefixHistoryRecordV1>,
}

pub struct PalwPrefixHistoryBrokerV1 {
    pending: Mutex<BTreeMap<Hash64, Pending>>,
    last_dispatch: Mutex<Option<Instant>>,
    public_reads: Arc<Semaphore>,
}

impl Default for PalwPrefixHistoryBrokerV1 {
    fn default() -> Self {
        Self {
            pending: Mutex::default(),
            last_dispatch: Mutex::default(),
            public_reads: Arc::new(Semaphore::new(MAX_CONCURRENT_PUBLIC_READS)),
        }
    }
}

impl PalwPrefixHistoryBrokerV1 {
    pub(crate) fn try_public_read_v1(&self) -> Option<OwnedSemaphorePermit> {
        self.public_reads.clone().try_acquire_owned().ok()
    }

    pub fn poll_v1(&self, request: &PalwPrefixHistoryRequestV1) -> Result<Option<PalwPrefixHistoryRecordV1>, String> {
        self.poll_at(request, Instant::now())
    }

    fn poll_at(&self, request: &PalwPrefixHistoryRequestV1, now: Instant) -> Result<Option<PalwPrefixHistoryRecordV1>, String> {
        let id = request.context_id_v1()?;
        let mut rows = self.pending.lock().unwrap();
        rows.retain(|_, row| now.saturating_duration_since(row.touched) < IDLE_TTL);
        if let Some(row) = rows.get_mut(&id) {
            row.touched = now;
            if row.ready.is_some() {
                return Ok(rows.remove(&id).unwrap().ready);
            }
        } else if rows.len() < MAX_PENDING {
            rows.insert(id, Pending { request: request.clone(), touched: now, sent: None, ready: None });
        }
        Ok(None)
    }

    pub(crate) fn due_v1(&self, now: Instant) -> Vec<PalwPrefixHistoryRequestV1> {
        let mut dispatched = self.last_dispatch.lock().unwrap();
        if dispatched.is_some_and(|last| now.saturating_duration_since(last) < DISPATCH_INTERVAL) {
            return Vec::new();
        }
        let mut rows = self.pending.lock().unwrap();
        rows.retain(|_, row| now.saturating_duration_since(row.touched) < IDLE_TTL);
        // Oldest attempt first: fixed key ordering must not starve later owners.
        let next = rows
            .values_mut()
            .filter(|row| row.ready.is_none() && row.sent.is_none_or(|sent| now.saturating_duration_since(sent) >= RETRY))
            .min_by_key(|row| row.sent);
        let Some(row) = next else { return Vec::new() };
        row.sent = Some(now);
        *dispatched = Some(now);
        vec![row.request.clone()]
    }

    pub(crate) fn receive_v1(&self, context: Hash64, raw: &[u8]) -> bool {
        if raw.is_empty() || raw.len() > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 {
            return false;
        }
        let mut rows = self.pending.lock().unwrap();
        let Some(row) = rows.get_mut(&context) else { return false };
        if row.ready.is_some() || row.touched.elapsed() >= IDLE_TTL {
            return false;
        }
        let Ok(record) = borsh::from_slice::<PalwPrefixHistoryRecordV1>(raw) else { return false };
        if record.authenticates_request_v1(&row.request).is_err() {
            return false;
        }
        row.ready = Some(record);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        BlockHash,
        palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
        palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        palw_state_v2::PalwBondKeyV2,
        tx::{TransactionId, TransactionOutpoint},
    };

    fn fixture() -> (PalwPrefixHistoryRequestV1, PalwPrefixHistoryRecordV1) {
        let record = PalwPrefixHistoryRecordV1::new(
            Hash64::default(),
            1,
            BlockHash::from_u64_word(8),
            100,
            200,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(2), 0)),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(3),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::PrefixPublicCollect {
                    cursor_id: Hash64::from_u64_word(4),
                    demand_id: Hash64::from_u64_word(5),
                },
                signature: vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap();
        let request = PalwPrefixHistoryRequestV1 {
            claim: record.movement.claim,
            owner: record.movement.reserver,
            reserved_daa: record.movement.reserved_daa,
            seed_id: record.movement.seed_id,
            head: record.head_v1().unwrap(),
            sequence: record.sequence,
            material_until_daa: record.material_until_daa,
        };
        (request, record)
    }

    #[test]
    fn only_requested_authenticated_records_complete_and_bad_replies_do_not_preempt() {
        let broker = PalwPrefixHistoryBrokerV1::default();
        let (request, record) = fixture();
        let raw = borsh::to_vec(&record).unwrap();
        let id = request.context_id_v1().unwrap();
        assert!(!broker.receive_v1(id, &raw));
        assert!(broker.poll_v1(&request).unwrap().is_none());
        let mut bad = record.clone();
        bad.movement.signature[0] ^= 1;
        assert!(!broker.receive_v1(id, &borsh::to_vec(&bad).unwrap()));
        assert!(!broker.receive_v1(id, &vec![0; PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 + 1]));
        assert!(broker.receive_v1(id, &raw));
        assert!(!broker.receive_v1(id, &raw));
        assert_eq!(broker.poll_v1(&request).unwrap(), Some(record));
        assert!(!broker.receive_v1(id, &raw));
        assert!(broker.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn pending_requests_have_a_fixed_cap_retry_interval_and_idle_expiry() {
        let broker = PalwPrefixHistoryBrokerV1::default();
        let (request, _) = fixture();
        let now = Instant::now();
        for n in 0..MAX_PENDING + 20 {
            let mut r = request.clone();
            r.head = Hash64::from_u64_word(n as u64 + 1);
            broker.poll_at(&r, now).unwrap();
        }
        assert_eq!(broker.pending.lock().unwrap().len(), MAX_PENDING);
        let mut served = std::collections::BTreeSet::new();
        for n in 0..MAX_PENDING {
            let at = now + DISPATCH_INTERVAL * n as u32;
            let due = broker.due_v1(at);
            assert_eq!(due.len(), 1);
            assert!(served.insert(due[0].context_id_v1().unwrap()), "an old request never starves untried owners");
            assert!(broker.due_v1(at).is_empty(), "many peer flows share one dispatch limit");
        }
        assert_eq!(served.len(), MAX_PENDING);
        assert_eq!(broker.due_v1(now + DISPATCH_INTERVAL * MAX_PENDING as u32).len(), 1);
        assert!(broker.due_v1(now + IDLE_TTL).is_empty());
        assert!(broker.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn public_read_budget_is_shared_across_peers_and_independent_of_local_pulls() {
        let broker = PalwPrefixHistoryBrokerV1::default();
        let mut permits = Vec::new();
        for _ in 0..MAX_CONCURRENT_PUBLIC_READS {
            permits.push(broker.try_public_read_v1().unwrap());
        }
        assert!(broker.try_public_read_v1().is_none());
        let (request, _) = fixture();
        assert!(broker.poll_v1(&request).unwrap().is_none());
        assert_eq!(broker.due_v1(Instant::now()).len(), 1, "anonymous reads do not hold the verifier pull lane");
        drop(permits.pop());
        assert!(broker.try_public_read_v1().is_some());
        drop(permits);
        assert_eq!(broker.public_reads.available_permits(), MAX_CONCURRENT_PUBLIC_READS);
    }

    #[tokio::test]
    async fn cancelled_flow_retains_its_budget_until_the_blocking_read_finishes() {
        let broker = Arc::new(PalwPrefixHistoryBrokerV1::default());
        let mut permits = Vec::new();
        for _ in 1..MAX_CONCURRENT_PUBLIC_READS {
            permits.push(broker.try_public_read_v1().unwrap());
        }
        let permit = broker.try_public_read_v1().unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            started_tx.send(()).unwrap();
            finish_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            drop(_permit);
            done_tx.send(()).unwrap();
        });
        started_rx.await.unwrap();
        worker.abort();
        assert!(broker.try_public_read_v1().is_none());
        finish_tx.send(()).unwrap();
        done_rx.await.unwrap();
        worker.await.unwrap();
        assert!(broker.try_public_read_v1().is_some());
        drop(permits);
    }
}
