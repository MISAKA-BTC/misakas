//! Admitted EDF ordering for the production court queue. Two compact arrays
//! replace the legacy HashMap and stable-sort scratch; payloads are never cloned.
use super::{CourtMoveKeyV1, CourtMoveV1};
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use kaspa_hashes::Hash64;
use std::{collections::HashMap, mem::size_of};

struct Row {
    key: Hash64,
    round: u32,
    original: usize,
    due: Option<u64>,
}
pub(super) struct CourtOrderPlanV1 {
    rows: Vec<Row>,
    destinations: Vec<usize>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl CourtOrderPlanV1 {
    pub(super) fn new(queue: &[CourtMoveV1], due: &HashMap<CourtMoveKeyV1, u64>) -> Result<Self, NativeCoverageV1> {
        let component = "court EDF ordering";
        let count = queue.len();
        let bytes = size_of::<Row>()
            .checked_add(size_of::<usize>())
            .and_then(|n| n.checked_mul(count))
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>(component))?;
        let lease = reserve_native_requests_v1(bytes)?;
        let mut rows = Vec::new();
        let mut destinations = Vec::new();
        rows.try_reserve_exact(count).map_err(|_| NativeCoverageV1::unsupported::<Self>(component, "row allocation refused"))?;
        destinations
            .try_reserve_exact(count)
            .map_err(|_| NativeCoverageV1::unsupported::<Self>(component, "permutation allocation refused"))?;
        for (original, (key, round, role, _)) in queue.iter().enumerate() {
            rows.push(Row { key: *key, round: *round, original, due: due.get(&(*key, *round, *role)).copied() });
            destinations.push(0);
        }
        // All dated moves of a claim share its earliest queued deadline. A due
        // entry for a move absent from this queue must not date that claim.
        rows.sort_unstable_by_key(|r| r.key);
        let mut start = 0;
        while start < rows.len() {
            let mut end = start + 1;
            while end < rows.len() && rows[end].key == rows[start].key {
                end += 1;
            }
            let earliest = rows[start..end].iter().filter_map(|r| r.due).min();
            for row in &mut rows[start..end] {
                row.due = earliest;
            }
            start = end;
        }
        rows.sort_unstable_by_key(|r| match r.due {
            Some(at) => (0u8, at, r.key, r.round, r.original),
            // The explicit original index preserves the legacy stable order,
            // including equal dated keys/rounds and every undated move.
            None => (1u8, 0, Hash64::default(), 0, r.original),
        });
        for (destination, row) in rows.iter().enumerate() {
            destinations[row.original] = destination;
        }
        Ok(Self { rows, destinations, _lease: lease })
    }
    /// Construction and allocation are complete before mutating any queue item.
    pub(super) fn apply(mut self, queue: &mut [CourtMoveV1]) {
        assert_eq!(queue.len(), self.rows.len(), "EDF plan is for this exact queue");
        for i in 0..queue.len() {
            while self.destinations[i] != i {
                let j = self.destinations[i];
                queue.swap(i, j);
                self.destinations.swap(i, j);
            }
        }
    }
}
