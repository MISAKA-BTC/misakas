//! Bounded, node-local telemetry from successful authoritative court folds.
//! Never serialized into consensus state. A guard drops every observation unless
//! its caller explicitly finishes it; the processor publishes only after commit.
use crate::{Hash64, palw_shard_court_v1::PalwShardCourtVerdictV1};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    marker::PhantomData,
    rc::Rc,
    time::Instant,
};

pub const MAX_OUTCOMES_V1: usize = 64;
pub const MAX_BLOCKS_V1: usize = 64;
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourtOutcomeV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub object_id: Hash64,
    pub object_tag: u8,
    pub route: &'static str,
    pub leaf_index: u64,
    pub verdict: &'static str,
    /// Local elapsed adjudication plus settlement; never a consensus cost or policy value.
    pub adjudication_and_settlement_micros: Option<u64>,
}
struct Capture {
    generation: u64,
    block: Hash64,
    rows: Vec<CourtOutcomeV1>,
    overflow: bool,
}
thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}
/// Telemetry snapshot travels with the builder's internal checkpoint, never its state encoding.
pub type CourtObservationCheckpointV1 = Option<(u64, usize, bool)>;
pub fn checkpoint_v1() -> CourtObservationCheckpointV1 {
    CAPTURE.with(|c| c.borrow().as_ref().map(|v| (v.generation, v.rows.len(), v.overflow)))
}
pub fn restore_v1(saved: CourtObservationCheckpointV1) {
    CAPTURE.with(|c| {
        let mut slot = c.borrow_mut();
        if let (Some(current), Some((generation, len, overflow))) = (slot.as_mut(), saved)
            && current.generation == generation
        {
            current.rows.truncate(len);
            current.overflow = overflow;
        }
    });
}
/// Not Send: the synchronous fold and its guard must stay on the same thread.
/// A nested capture temporarily replaces the outer one; it cannot append to it.
pub struct CourtObservationGuardV1 {
    active: bool,
    previous: Option<Capture>,
    _thread: PhantomData<Rc<()>>,
}
impl CourtObservationGuardV1 {
    pub fn enter(block: Hash64) -> Self {
        let generation = GENERATION.with(|g| {
            let next = g.get().checked_add(1);
            if let Some(n) = next {
                g.set(n);
            }
            next
        });
        let previous =
            CAPTURE.with(|c| c.replace(generation.map(|generation| Capture { generation, block, rows: Vec::new(), overflow: false })));
        Self { active: true, previous, _thread: PhantomData }
    }
    pub fn finish(mut self) -> Option<Vec<CourtOutcomeV1>> {
        self.active = false;
        CAPTURE.with(|c| c.replace(self.previous.take()).and_then(|capture| (!capture.overflow).then_some(capture.rows)))
    }
}
impl Drop for CourtObservationGuardV1 {
    fn drop(&mut self) {
        if self.active {
            CAPTURE.with(|c| {
                c.replace(self.previous.take());
            });
        }
    }
}
/// Start only inside the authoritative capture; includes court verification and economic writes.
pub fn start_v1(block: Hash64) -> Option<Instant> {
    CAPTURE.with(|c| c.borrow().as_ref().filter(|v| v.block == block).map(|_| Instant::now()))
}
/// Call only after this exact object's verdict and economic effects succeeded.
#[allow(clippy::too_many_arguments)]
pub fn record_v1<T: borsh::BorshSerialize>(
    block: Hash64,
    claim_id: Hash64,
    class_id: Hash64,
    object_tag: u8,
    route: &'static str,
    leaf_index: u64,
    verdict: PalwShardCourtVerdictV1,
    accusation: &T,
    started: Option<Instant>,
) {
    CAPTURE.with(|c| {
        let mut slot = c.borrow_mut();
        let Some(capture) = slot.as_mut().filter(|v| v.block == block) else {
            return;
        };
        if capture.rows.len() >= MAX_OUTCOMES_V1 {
            capture.overflow = true;
            return;
        }
        let verdict = match verdict {
            PalwShardCourtVerdictV1::ExecutorGuilty => "ExecutorGuilty",
            PalwShardCourtVerdictV1::FalseAccusation => "FalseAccusation",
            PalwShardCourtVerdictV1::NeedsDissection => return,
        };
        let adjudication_and_settlement_micros = started.and_then(|t| u64::try_from(t.elapsed().as_micros()).ok());
        let object_id =
            Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/LOCAL-COURT-OBSERVATION/V1", &(object_tag, accusation)));
        capture.rows.push(CourtOutcomeV1 {
            claim_id,
            class_id,
            object_id,
            object_tag,
            route,
            leaf_index,
            verdict,
            adjudication_and_settlement_micros,
        });
    });
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommittedCourtObservationV1 {
    pub block: Hash64,
    pub post_state_root: Hash64,
    pub daa: u64,
    pub outcomes: Vec<CourtOutcomeV1>,
}
impl CommittedCourtObservationV1 {
    pub fn json_at_sink_v1(&self, sink: Hash64) -> Result<String, String> {
        serde_json::to_string(&serde_json::json!({
            "version": 1, "kind": "nativeCommittedMemoryRoutingOutcomeV1", "nodeLocalObservationOnly": true,
            "releaseApproval": false, "securitySoundnessMeasured": false, "sink": sink.to_string(), "observation": self,
        }))
        .map_err(|e| e.to_string())
    }
}
#[derive(Default)]
pub struct CourtObservationCacheV1 {
    rows: VecDeque<CommittedCourtObservationV1>,
}
impl CourtObservationCacheV1 {
    /// The node calls this only after successful atomic UTXO/PALW commit.
    pub fn publish_after_commit(&mut self, row: CommittedCourtObservationV1) {
        if row.outcomes.is_empty() || row.outcomes.len() > MAX_OUTCOMES_V1 {
            return;
        }
        self.rows.retain(|old| old.block != row.block);
        if self.rows.len() == MAX_BLOCKS_V1 {
            self.rows.pop_front();
        }
        self.rows.push_back(row);
    }
    /// No historical authority is inferred from a cache entry: the node supplies
    /// the current selected-chain membership and the stored post-state root.
    pub fn read_current(&self, block: Hash64, claim: Hash64, selected: bool, root: Hash64) -> Option<CommittedCourtObservationV1> {
        if !selected {
            return None;
        }
        let row = self.rows.iter().find(|row| row.block == block && row.post_state_root == root)?;
        let outcomes: Vec<_> = row.outcomes.iter().filter(|o| o.claim_id == claim).cloned().collect();
        (!outcomes.is_empty()).then(|| CommittedCourtObservationV1 { outcomes, ..row.clone() })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn emit(block: Hash64) {
        record_v1(block, h(2), h(3), 244, "RoutedStep", 7, PalwShardCourtVerdictV1::ExecutorGuilty, &42u64, None);
    }
    #[test]
    fn elapsed_measurement_requires_active_matching_capture() {
        assert!(start_v1(h(1)).is_none());
        let guard = CourtObservationGuardV1::enter(h(1));
        assert!(start_v1(h(2)).is_none());
        let started = start_v1(h(1));
        assert!(started.is_some());
        record_v1(h(1), h(2), h(3), 244, "RoutedStep", 7, PalwShardCourtVerdictV1::ExecutorGuilty, &42u64, started);
        emit(h(1));
        let rows = guard.finish().unwrap();
        assert!(rows[0].adjudication_and_settlement_micros.is_some());
        assert!(rows[1].adjudication_and_settlement_micros.is_none());
    }
    #[test]
    fn failed_fold_drop_and_wrong_context_cannot_publish() {
        {
            let _guard = CourtObservationGuardV1::enter(h(1));
            emit(h(1));
        }
        let guard = CourtObservationGuardV1::enter(h(1));
        emit(h(9));
        assert!(guard.finish().unwrap().is_empty());
        let guard = CourtObservationGuardV1::enter(h(1));
        emit(h(1));
        let rows = guard.finish().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].verdict, "ExecutorGuilty");
    }
    #[test]
    fn checkpoint_restore_discards_inner_success_and_overflow_when_outer_fold_succeeds() {
        let guard = CourtObservationGuardV1::enter(h(1));
        emit(h(1));
        let checkpoint = checkpoint_v1();
        for _ in 0..=MAX_OUTCOMES_V1 {
            emit(h(1));
        }
        restore_v1(checkpoint);
        let rows = guard.finish().expect("rolled-back overflow is not current history");
        assert_eq!(rows.len(), 1, "inner successful verdicts rolled back with their state");
    }
    #[test]
    fn nested_capture_cannot_publish_into_outer_and_wrong_generation_cannot_restore_it() {
        let outer = CourtObservationGuardV1::enter(h(1));
        emit(h(1));
        let old = checkpoint_v1();
        {
            let inner = CourtObservationGuardV1::enter(h(1));
            emit(h(1));
            emit(h(1));
            restore_v1(old);
            assert_eq!(inner.finish().unwrap().len(), 2);
        }
        {
            let _discarded = CourtObservationGuardV1::enter(h(1));
            emit(h(1));
        }
        assert_eq!(outer.finish().unwrap().len(), 1);
    }
    #[test]
    fn overflow_is_unknown_not_a_truncated_complete_observation() {
        let guard = CourtObservationGuardV1::enter(h(1));
        for _ in 0..=MAX_OUTCOMES_V1 {
            emit(h(1));
        }
        assert!(guard.finish().is_none());
    }
    #[test]
    fn only_published_commits_current_root_and_chain_survive_and_restart_is_unknown() {
        let guard = CourtObservationGuardV1::enter(h(1));
        emit(h(1));
        let rows = guard.finish().unwrap();
        let mut cache = CourtObservationCacheV1::default();
        assert!(cache.read_current(h(1), h(2), true, h(4)).is_none());
        cache.publish_after_commit(CommittedCourtObservationV1 {
            block: h(1),
            post_state_root: h(4),
            daa: 10,
            outcomes: rows.clone(),
        });
        assert!(cache.read_current(h(1), h(2), true, h(4)).is_some());
        assert!(cache.read_current(h(1), h(2), false, h(4)).is_none());
        assert!(cache.read_current(h(1), h(2), true, h(5)).is_none());
        assert!(cache.read_current(h(1), h(9), true, h(4)).is_none());
        assert!(CourtObservationCacheV1::default().read_current(h(1), h(2), true, h(4)).is_none());
        for n in 2..=65 {
            cache.publish_after_commit(CommittedCourtObservationV1 {
                block: h(n),
                post_state_root: h(4),
                daa: n as u64,
                outcomes: rows.clone(),
            });
        }
        assert!(cache.read_current(h(1), h(2), true, h(4)).is_none());
    }
}
