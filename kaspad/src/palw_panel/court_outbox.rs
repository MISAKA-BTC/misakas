//! Node-local ownership of court pending moves, their due dates and submission
//! debounce. Historical enqueue-attempt stamps are retained at their existing sites;
//! neither those stamps nor successful mempool submission are chain acceptance. Native
//! reads and existing filer/recovery books remain the authority for acceptance.
use super::{Hash64, PalwConsensusObjectV2, palw_court_queue_edf_v1};
use std::collections::HashMap;
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1};
#[path = "court_outbox_tickets.rs"]
mod tickets;
use tickets::CourtPayloadTicketsV1;

pub(super) type CourtMoveKeyV1 = (Hash64, u32, bool);
pub(super) type CourtMoveV1 = (Hash64, u32, bool, PalwConsensusObjectV2);

#[derive(Default)]
pub(crate) struct CourtOutboxV1 {
    pending: Vec<CourtMoveV1>,
    due: HashMap<CourtMoveKeyV1, u64>,
    submitted: HashMap<CourtMoveKeyV1, u64>,
    // Pending payloads drop before the tickets that authorize their storage.
    payload_tickets: CourtPayloadTicketsV1,
}
impl CourtOutboxV1 {
    pub(super) fn pending(&self) -> &[CourtMoveV1] {
        &self.pending
    }
    /// Existing callers decide replacement/duplicate policy; no new dedup rule.
    pub(super) fn push_pending(&mut self, item: CourtMoveV1) {
        self.pending.push(item);
    }
    /// Copy admission precedes deep Clone. The outbox retains the payload ticket
    /// through its carrier batch and any retry; a refusal leaves the queue unchanged.
    pub(super) fn try_push_clone(&mut self, key: CourtMoveKeyV1, source: &PalwConsensusObjectV2) -> Result<(), NativeCoverageV1> {
        let slot = self.payload_tickets.prepare()?;
        let copy = NativeRetainedV1::try_from_clone_v1(source)?;
        self.pending.try_reserve_exact(1).map_err(|_| NativeCoverageV1::unsupported::<Self>(
            "court outbox copied payload slot", "queue allocation refused"))?;
        let (object, ticket) = copy.into_parts_v1();
        self.payload_tickets.insert(key, ticket, slot);
        self.pending.push((key.0, key.1, key.2, object));
        Ok(())
    }
    pub(super) fn retain_pending(&mut self, keep: impl FnMut(&CourtMoveV1) -> bool) {
        self.pending.retain(keep);
        self.payload_tickets.prune(&self.pending);
    }
    pub(super) fn set_due(&mut self, key: CourtMoveKeyV1, due: u64) -> Option<u64> {
        self.due.insert(key, due)
    }
    pub(super) fn forget_due(&mut self, key: &CourtMoveKeyV1) -> Option<u64> {
        self.due.remove(key)
    }
    pub(super) fn date_if_absent(&mut self, key: CourtMoveKeyV1, due: u64) {
        self.due.entry(key).or_insert(due);
    }
    pub(super) fn tighten_due(&mut self, key: CourtMoveKeyV1, due: u64) {
        self.due.entry(key).and_modify(|at| *at = (*at).min(due)).or_insert(due);
    }
    pub(super) fn tighten_existing_due(&mut self, key: CourtMoveKeyV1, due: u64) {
        if let Some(at) = self.due.get_mut(&key) {
            *at = (*at).min(due);
        }
    }
    pub(super) fn extend_due(&mut self, due: impl IntoIterator<Item = (CourtMoveKeyV1, u64)>) {
        self.due.extend(due);
    }
    pub(super) fn retain_due(&mut self, keep: impl FnMut(&CourtMoveKeyV1, &mut u64) -> bool) {
        self.due.retain(keep);
    }
    pub(super) fn submitted_at(&self, key: &CourtMoveKeyV1) -> Option<&u64> {
        self.submitted.get(key)
    }
    pub(super) fn has_submitted(&self, key: &CourtMoveKeyV1) -> bool {
        self.submitted.contains_key(key)
    }
    /// Historical move/debounce timestamp: never a Done/accepted transition.
    /// The carrier stamps successful submission; legacy eager sites still stamp queue attempts.
    pub(super) fn mark_submitted(&mut self, key: CourtMoveKeyV1, at: u64) -> Option<u64> {
        self.submitted.insert(key, at)
    }
    pub(super) fn forget_submitted(&mut self, key: &CourtMoveKeyV1) -> Option<u64> {
        self.submitted.remove(key)
    }
    pub(super) fn retain_submitted(&mut self, keep: impl FnMut(&CourtMoveKeyV1, &mut u64) -> bool) {
        self.submitted.retain(keep);
    }
    pub(super) fn take_priority(&mut self) -> Vec<CourtMoveV1> {
        self.payload_tickets.start_flush();
        palw_court_queue_edf_v1(&mut self.pending, &self.due);
        std::mem::take(&mut self.pending)
    }
    /// Restore exactly the unsent items and prune due dates exactly as the old
    /// carrier loop did. Submission debounce deliberately survives this step.
    pub(super) fn finish_flush(&mut self, unsent: Vec<CourtMoveV1>) {
        self.pending = unsent;
        self.payload_tickets.finish_flush(&self.pending);
        self.due.retain(|key, _| self.pending.iter().any(|(s, r, b, _)| (*s, *r, *b) == *key));
    }
    /// Test-only adapter for pre-refactor fixtures. Production takes the typed owner.
    #[cfg(test)]
    pub(crate) fn with_fixture_parts<R>(
        pending: &mut Vec<CourtMoveV1>,
        due: &mut HashMap<CourtMoveKeyV1, u64>,
        submitted: &mut HashMap<CourtMoveKeyV1, u64>,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut owner = Self::from_parts(std::mem::take(pending), std::mem::take(due), std::mem::take(submitted));
        let result = run(&mut owner);
        (*pending, *due, *submitted) = owner.into_parts();
        result
    }
    #[cfg(test)]
    pub(super) fn from_parts(
        pending: Vec<CourtMoveV1>,
        due: HashMap<CourtMoveKeyV1, u64>,
        submitted: HashMap<CourtMoveKeyV1, u64>,
    ) -> Self {
        Self { pending, due, submitted, payload_tickets: CourtPayloadTicketsV1::default() }
    }
    #[cfg(test)]
    pub(super) fn into_parts(self) -> (Vec<CourtMoveV1>, HashMap<CourtMoveKeyV1, u64>, HashMap<CourtMoveKeyV1, u64>) {
        assert!(self.payload_tickets.is_empty(), "fixture extraction must not detach payload tickets");
        (self.pending, self.due, self.submitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(session: u64, round: u32) -> CourtMoveV1 {
        let key = Hash64::from_u64_word(session);
        (key, round, false, PalwConsensusObjectV2::ReceiptLicensed { claim: key, receipts: Vec::new() })
    }
    fn key(item: &CourtMoveV1) -> CourtMoveKeyV1 {
        (item.0, item.1, item.2)
    }

    /// Several flushes with a single free carrier: preserve stable EDF, duplicates,
    /// the remaining due dates and the submission debounce, even when no acceptance arrives.
    #[test]
    fn constrained_flush_and_lost_submission_match_the_legacy_queue() {
        let pending = vec![item(1, 0), item(2, 7), item(3, 1), item(2, 5), item(3, 1), item(4, 0)];
        let due = [(key(&pending[1]), 90), (key(&pending[2]), 50), (key(&pending[3]), 120)].into_iter().collect();
        let mut owner = CourtOutboxV1::from_parts(pending.clone(), due, HashMap::new());
        let (mut old_pending, mut old_due, mut old_moved) = (pending, owner.due.clone(), HashMap::new());
        for daa in [100, 101, 100 + super::super::COURT_MOVE_REPLAN_DAA] {
            palw_court_queue_edf_v1(&mut old_pending, &old_due);
            let mut batch = std::mem::take(&mut old_pending);
            let mut actual = owner.take_priority();
            assert_eq!(actual, batch);
            let sent = batch.remove(0);
            assert_eq!(actual.remove(0), sent);
            // This is the old carrier's success path: one slot, all other items unsent.
            old_moved.insert(key(&sent), daa);
            owner.mark_submitted(key(&sent), daa);
            old_pending = batch;
            old_due.retain(|k, _| old_pending.iter().any(|item| key(item) == *k));
            owner.finish_flush(actual);
            assert_eq!(
                (owner.pending.clone(), owner.due.clone(), owner.submitted.clone()),
                (old_pending.clone(), old_due.clone(), old_moved.clone())
            );
        }
        // A mempool yes is not a settled claim: old recovery may drop its stamp and rebuild.
        let lost = key(&item(3, 1));
        assert!(owner.has_submitted(&lost));
        owner.forget_submitted(&lost);
        old_moved.remove(&lost);
        owner.push_pending(item(3, 1));
        old_pending.push(item(3, 1));
        owner.date_if_absent(lost, 140);
        old_due.entry(lost).or_insert(140);
        palw_court_queue_edf_v1(&mut old_pending, &old_due);
        assert_eq!(owner.take_priority(), old_pending);
        assert_eq!(owner.submitted, old_moved);
    }

    #[test]
    fn payload_ticket_child() {
        if std::env::var("MISAKA_COURT_PAYLOAD_TICKETS").ok().as_deref() != Some("1") {
            return;
        }
        use kaspa_consensus_core::palw_native_resource_v1::{
            NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
        };
        let memory = 1 << 20;
        let source = item(77, 3);
        let source_key = key(&source);
        let mut owner = CourtOutboxV1::default();
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        let blocker = reserve_native_requests_v1(memory).unwrap();
        assert!(owner.try_push_clone(source_key, &source.3).is_err());
        assert!(owner.pending.is_empty());
        assert!(owner.payload_tickets.is_empty());
        drop(blocker);
        owner.try_push_clone(source_key, &source.3).unwrap();
        owner.try_push_clone(source_key, &source.3).unwrap();
        assert!(reserve_native_requests_v1(memory).is_err());
        owner.set_due(source_key, 1);
        owner.forget_due(&source_key);
        owner.mark_submitted(source_key, 2);
        owner.forget_submitted(&source_key);
        assert!(reserve_native_requests_v1(memory).is_err());
        let mut batch = owner.take_priority();
        owner.retain_pending(|_| false);
        assert!(reserve_native_requests_v1(memory).is_err(), "batch still owns copied data");
        drop(batch.pop());
        owner.finish_flush(batch);
        assert!(reserve_native_requests_v1(memory).is_err(), "duplicate key still pending");
        let batch = owner.take_priority();
        drop(batch);
        owner.finish_flush(Vec::new());
        assert!(owner.payload_tickets.is_empty());
        assert!(reserve_native_requests_v1(memory).is_ok(), "last payload and its tickets released");
        owner.try_push_clone(source_key, &source.3).unwrap();
        drop(owner.take_priority()); // Carrier future canceled before finish_flush.
        assert!(reserve_native_requests_v1(memory).is_err());
        let next = owner.take_priority();
        assert!(next.is_empty());
        owner.finish_flush(next);
        assert!(reserve_native_requests_v1(memory).is_ok(), "next flush cleans up canceled batch tickets");
    }

    #[test]
    fn copied_payload_tickets_survive_flush_retry_and_duplicates() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "palw_panel::court_outbox::tests::payload_ticket_child", "--nocapture"])
            .env("MISAKA_COURT_PAYLOAD_TICKETS", "1")
            .output().unwrap();
        assert!(out.status.success(), "{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
    }

    #[test]
    fn missing_deadline_is_only_inserted_by_the_existing_inserting_operations() {
        let mut owner = CourtOutboxV1::default();
        let undated = item(8, 0);
        let dated = item(9, 0);
        owner.push_pending(undated.clone());
        owner.push_pending(dated.clone());
        owner.tighten_existing_due(key(&undated), 1);
        owner.set_due(key(&dated), 30);
        owner.date_if_absent(key(&dated), 2);
        owner.tighten_due(key(&dated), 20);
        assert_eq!(owner.take_priority(), vec![dated.clone(), undated]);
        assert_eq!(owner.due.get(&key(&dated)), Some(&20));
        assert_eq!(owner.due.len(), 1);
    }
}
