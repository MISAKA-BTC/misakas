//! Private occupancy reservations shared by Positive/Fraud admission adapters.
//! Separate lane ceilings protect Fraud rights from Positive workload. This is
//! NOT a coin ledger, durable admission transaction, or cumulative work meter.
//! A0/A5/A6 must commit this state together with funds and case journal. Clone a
//! proposed state inside that transaction; never publish a reservation first.
use super::{ErrorV2, Hash64};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CourtLaneV2 {
    Positive,
    Fraud,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CourtResourcesV2 {
    pub work: u64,
    pub carrier_bytes: u64,
    pub live_bytes: u64,
}
impl CourtResourcesV2 {
    fn add(self, other: Self) -> Result<Self, ErrorV2> {
        Ok(Self {
            work: self.work.checked_add(other.work).ok_or(ErrorV2::Budget)?,
            carrier_bytes: self.carrier_bytes.checked_add(other.carrier_bytes).ok_or(ErrorV2::Budget)?,
            live_bytes: self.live_bytes.checked_add(other.live_bytes).ok_or(ErrorV2::Budget)?,
        })
    }
    fn fits(self, cap: Self) -> bool {
        self.work <= cap.work && self.carrier_bytes <= cap.carrier_bytes && self.live_bytes <= cap.live_bytes
    }
    fn sub(self, other: Self) -> Self {
        Self {
            work: self.work - other.work,
            carrier_bytes: self.carrier_bytes - other.carrier_bytes,
            live_bytes: self.live_bytes - other.live_bytes,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtLaneCapacityV2 {
    pub max_cases: u64,
    pub resources: CourtResourcesV2,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CourtLaneUsageV2 {
    pub cases: u64,
    pub resources: CourtResourcesV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservationEffectV2 {
    Reserved,
    AlreadyReserved,
}
#[derive(Clone, Debug)]
pub struct CourtCapacityV2 {
    caps: [CourtLaneCapacityV2; 2],
    usage: [CourtLaneUsageV2; 2],
    entries: BTreeMap<(Hash64, CourtLaneV2), CourtResourcesV2>,
}
impl CourtLaneV2 {
    fn index(self) -> usize {
        match self {
            Self::Positive => 0,
            Self::Fraud => 1,
        }
    }
}
impl CourtCapacityV2 {
    pub fn new(positive: CourtLaneCapacityV2, fraud: CourtLaneCapacityV2) -> Result<Self, ErrorV2> {
        for cap in [positive, fraud] {
            if cap.max_cases == 0 || cap.resources.work == 0 || cap.resources.carrier_bytes == 0 || cap.resources.live_bytes == 0 {
                return Err(ErrorV2::Budget);
            }
        }
        // Also reject a configuration whose combined finite pool overflows.
        positive.max_cases.checked_add(fraud.max_cases).ok_or(ErrorV2::Budget)?;
        positive.resources.add(fraud.resources)?;
        Ok(Self { caps: [positive, fraud], usage: [CourtLaneUsageV2::default(); 2], entries: BTreeMap::new() })
    }
    pub fn usage(&self, lane: CourtLaneV2) -> CourtLaneUsageV2 {
        self.usage[lane.index()]
    }
    /// `case` must be the A0-resolved branch-scoped case identity. Node-derived
    /// resources must include all retries/carriers/retention for that admission.
    /// Panel claims of low resource use are not authoritative admission inputs.
    pub fn reserve(&mut self, case: Hash64, lane: CourtLaneV2, resources: CourtResourcesV2) -> Result<ReservationEffectV2, ErrorV2> {
        if let Some(old) = self.entries.get(&(case, lane)) {
            return if *old == resources { Ok(ReservationEffectV2::AlreadyReserved) } else { Err(ErrorV2::Binding) };
        }
        if resources.work == 0 || resources.carrier_bytes == 0 || resources.live_bytes == 0 {
            return Err(ErrorV2::Budget);
        }
        let index = lane.index();
        let usage = self.usage[index];
        let proposed =
            CourtLaneUsageV2 { cases: usage.cases.checked_add(1).ok_or(ErrorV2::Budget)?, resources: usage.resources.add(resources)? };
        if proposed.cases > self.caps[index].max_cases || !proposed.resources.fits(self.caps[index].resources) {
            return Err(ErrorV2::Budget);
        }
        self.entries.insert((case, lane), resources);
        self.usage[index] = proposed;
        Ok(ReservationEffectV2::Reserved)
    }
    /// Release only after the journal/retention owner establishes the horizon.
    /// Does not erase per-claim attempt/service budgets or refund/pay any funds.
    /// A duplicate release is harmless and cannot free another case's capacity.
    pub fn release(&mut self, case: Hash64, lane: CourtLaneV2) -> bool {
        let Some(resources) = self.entries.remove(&(case, lane)) else { return false };
        let usage = &mut self.usage[lane.index()];
        usage.cases -= 1;
        usage.resources = usage.resources.sub(resources);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resources(n: u64) -> CourtResourcesV2 {
        CourtResourcesV2 { work: n, carrier_bytes: n, live_bytes: n }
    }
    fn cap(n: u64) -> CourtLaneCapacityV2 {
        CourtLaneCapacityV2 { max_cases: n, resources: resources(n) }
    }
    fn id(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    #[test]
    fn positive_exhaustion_preserves_fraud_capacity_and_failed_reservation_is_atomic() {
        let mut pool = CourtCapacityV2::new(cap(2), cap(1)).unwrap();
        pool.reserve(id(1), CourtLaneV2::Positive, resources(2)).unwrap();
        let before = pool.usage(CourtLaneV2::Positive);
        assert_eq!(pool.reserve(id(2), CourtLaneV2::Positive, resources(1)), Err(ErrorV2::Budget));
        assert_eq!(pool.usage(CourtLaneV2::Positive), before);
        assert_eq!(pool.reserve(id(2), CourtLaneV2::Fraud, resources(1)), Ok(ReservationEffectV2::Reserved));
    }
    #[test]
    fn duplicate_and_conflicting_reservations_and_release_cannot_mint_capacity() {
        let mut pool = CourtCapacityV2::new(cap(2), cap(1)).unwrap();
        pool.reserve(id(1), CourtLaneV2::Positive, resources(1)).unwrap();
        assert_eq!(pool.reserve(id(1), CourtLaneV2::Positive, resources(1)), Ok(ReservationEffectV2::AlreadyReserved));
        assert_eq!(pool.reserve(id(1), CourtLaneV2::Fraud, resources(1)), Ok(ReservationEffectV2::Reserved));
        assert_eq!(pool.reserve(id(1), CourtLaneV2::Positive, resources(2)), Err(ErrorV2::Binding));
        assert_eq!(pool.usage(CourtLaneV2::Positive).cases, 1);
        assert!(!pool.release(id(9), CourtLaneV2::Positive));
        assert!(pool.release(id(1), CourtLaneV2::Positive));
        assert!(!pool.release(id(1), CourtLaneV2::Positive));
        assert_eq!(pool.usage(CourtLaneV2::Positive), CourtLaneUsageV2::default());
        assert_eq!(pool.usage(CourtLaneV2::Fraud).cases, 1);
    }
    #[test]
    fn staging_can_be_discarded_if_funds_or_journal_fail() {
        let original = CourtCapacityV2::new(cap(2), cap(1)).unwrap();
        let mut proposed = original.clone();
        proposed.reserve(id(1), CourtLaneV2::Positive, resources(1)).unwrap();
        drop(proposed); // External admission transaction rejected its fund hold.
        assert_eq!(original.usage(CourtLaneV2::Positive), CourtLaneUsageV2::default());
    }
    #[test]
    fn overflow_and_each_resource_dimension_fail_before_mutation() {
        assert!(matches!(CourtCapacityV2::new(cap(u64::MAX), cap(1)), Err(ErrorV2::Budget)));
        let mut pool = CourtCapacityV2::new(cap(2), cap(1)).unwrap();
        for resources in [
            CourtResourcesV2 { work: 3, ..resources(1) },
            CourtResourcesV2 { carrier_bytes: 3, ..resources(1) },
            CourtResourcesV2 { live_bytes: 3, ..resources(1) },
        ] {
            assert_eq!(pool.reserve(id(1), CourtLaneV2::Positive, resources), Err(ErrorV2::Budget));
            assert_eq!(pool.usage(CourtLaneV2::Positive), CourtLaneUsageV2::default());
        }
    }
}
