//! Node-local received material and interval owners. The existing admission,
//! retention and resume algorithms stay authoritative; these owners keep the
//! material byte counter and arrival index together and expose no raw material writer.
use super::*;

pub(super) struct PoolBooksV1 {
    pub(super) material: MaterialPoolV1,
    pub(super) intervals: IntervalOpeningPoolV1,
}
pub(super) struct MaterialPoolV1 {
    values: HashMap<Hash64, Vec<Vec<u8>>>,
    bytes: usize,
    arrival: HashMap<Hash64, u64>,
    sequence: u64,
}
pub(super) struct IntervalOpeningPoolV1 {
    values: HashMap<(Hash64, u32), Vec<Vec<u8>>>,
}
impl PoolBooksV1 {
    pub(super) fn new() -> Self {
        let materials: HashMap<Hash64, Vec<Vec<u8>>> = HashMap::new();
        // ADR-0077 Decision 8: the interval openings served to THIS node because it asked. Keyed
        // per `(claim, interval)` and bounded by [`Self::pool_interval_opening`]; the transport
        // has already refused anything unsolicited, over-sized, or past a claim's slot ceiling.
        let interval_openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
        // Total pooled bytes and per-claim arrival order, maintained with `materials` so the
        // ceiling above is enforced at insertion rather than recomputed by walking the pool
        // (audit3 S-02). The sequence is a plain counter: it only has to order arrivals, and a DAA
        // score is not available inside the drain.
        let pool_bytes: usize = 0;
        let pool_arrival: HashMap<Hash64, u64> = HashMap::new();
        let pool_arrival_seq: u64 = 0;
        Self {
            material: MaterialPoolV1 { values: materials, bytes: pool_bytes, arrival: pool_arrival, sequence: pool_arrival_seq },
            intervals: IntervalOpeningPoolV1 { values: interval_openings },
        }
    }
}
impl MaterialPoolV1 {
    pub(super) fn map(&self) -> &HashMap<Hash64, Vec<Vec<u8>>> {
        &self.values
    }
    pub(super) fn admit(&mut self, claim: Hash64, bytes: Vec<u8>) {
        pool_admit_material_v1(&mut self.values, &mut self.arrival, &mut self.sequence, &mut self.bytes, claim, bytes);
    }
    pub(super) fn sweep(&mut self, keep: impl Fn(&Hash64) -> bool) {
        pool_sweep_material_v1(&mut self.values, &mut self.arrival, &mut self.bytes, keep);
    }
    /// Only the gossip arrival path applied this whole-pool eviction. Retained and
    /// remade captures keep their original admission path; no new eviction policy.
    pub(super) fn evict_after_gossip(&mut self, claim: Hash64, first_seen: &mut HashMap<Hash64, u64>) {
        while self.values.len() > PANEL_POOL_MAX_CLAIMS || self.bytes > PANEL_POOL_MAX_BYTES {
            // Never evict the claim that just arrived: that would make the pool
            // drop exactly the payload it was asked to hold, which is the M2-1
            // failure in a different costume.
            let Some(oldest) = self.arrival.iter().filter(|(id, _)| **id != claim).min_by_key(|(_, seq)| **seq).map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(evicted) = self.values.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(evicted.iter().map(|b| b.len()).sum::<usize>());
            }
            self.arrival.remove(&oldest);
            first_seen.remove(&oldest);
        }
    }
}
impl IntervalOpeningPoolV1 {
    pub(super) fn map(&self) -> &HashMap<(Hash64, u32), Vec<Vec<u8>>> {
        &self.values
    }
    /// Existing segment and public-court consumers may discard unusable openings.
    /// Separate ownership keeps that borrow disjoint from the material reader.
    pub(super) fn resume_mut(&mut self) -> &mut HashMap<(Hash64, u32), Vec<Vec<u8>>> {
        &mut self.values
    }
    pub(super) fn admit(&mut self, claim: Hash64, interval: u32, bytes: Vec<u8>, pairs: usize) -> bool {
        PalwPanelService::pool_interval_opening(&mut self.values, claim, interval, bytes, pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_material_arrival_eviction_and_retirement_leave_a_reusable_pool() {
        let mut books = PoolBooksV1::new();
        let mut first_seen = HashMap::new();
        let claim = |n| Hash64::from_u64_word(n);
        for n in 0..=PANEL_POOL_MAX_CLAIMS as u64 {
            books.material.admit(claim(n), vec![n as u8; 3]);
            first_seen.insert(claim(n), n);
            books.material.evict_after_gossip(claim(n), &mut first_seen);
        }
        assert!(!books.material.map().contains_key(&claim(0)));
        assert!(!first_seen.contains_key(&claim(0)));
        assert_eq!(books.material.map().len(), PANEL_POOL_MAX_CLAIMS);
        let latest = claim(PANEL_POOL_MAX_CLAIMS as u64);
        for size in 4..=8 {
            books.material.admit(latest, vec![42; size]);
        }
        assert_eq!(books.material.map()[&latest].iter().map(Vec::len).collect::<Vec<_>>(), [5, 6, 7, 8]);
        books.material.sweep(|id| *id == latest);
        assert_eq!(books.material.bytes, 26);
        assert_eq!(books.material.arrival.len(), 1);
        let old_sequence = books.material.sequence;
        books.material.sweep(|_| false);
        assert!(books.material.map().is_empty());
        assert!(books.material.arrival.is_empty());
        assert_eq!(books.material.bytes, 0);
        books.material.admit(latest, vec![9; 7]);
        assert_eq!(books.material.bytes, 7);
        assert_eq!(books.material.arrival[&latest], old_sequence + 1);
    }
}
