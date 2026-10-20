//! RFC-0026 service reservation component. Not wired to state, block admission,
//! codecs, default, or any preset. The caller must derive the COMPLETE canonical
//! packet set from an adopted profile, including every allowed hostile branch.
//! A successful reservation does not establish network/inclusion assumptions.

use std::collections::{BTreeMap, BTreeSet};

/// Independent deterministic capacity dimensions; never measured runtime CPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceCost {
    pub bytes: u64,
    pub work: u64,
    pub io: u64,
    pub controls: u64,
}

impl ServiceCost {
    fn subtract(self, cost: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_sub(cost.bytes)?,
            work: self.work.checked_sub(cost.work)?,
            io: self.io.checked_sub(cost.io)?,
            controls: self.controls.checked_sub(cost.controls)?,
        })
    }
}

/// An atomic codec packet, NOT an entire potentially fragmented response. IDs
/// bind claim/ruleset/root/coordinate/turn/packet in the future profile adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServicePacket {
    pub id: [u8; 64],
    pub release_daa: u64,
    /// Exclusive end of the service interval. Does not set notice equality.
    pub service_before: u64,
    pub cost: ServiceCost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceRefusal {
    InvalidInterval,
    DuplicateIdentity,
    NoCapacity,
    InvalidBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceReservations {
    budget: ServiceCost,
    remaining: BTreeMap<u64, ServiceCost>,
    assignments: BTreeMap<[u8; 64], u64>,
}

impl ServiceReservations {
    pub fn new(budget: ServiceCost) -> Result<Self, ServiceRefusal> {
        if budget.bytes == 0 || budget.work == 0 || budget.io == 0 || budget.controls == 0 {
            return Err(ServiceRefusal::InvalidBudget);
        }
        Ok(Self { budget, remaining: BTreeMap::new(), assignments: BTreeMap::new() })
    }

    /// All-or-nothing admission. Sort by deadline and canonical packet identity;
    /// never shift an existing assignment. Conservative first-fit may refuse a
    /// schedulable set; it must never report an unreserved set as guaranteed.
    /// Runtime is bounded by packet count times the caller-validated interval.
    pub fn reserve(&mut self, packets: &[ServicePacket], hard_deadline: u64) -> Result<(), ServiceRefusal> {
        let mut seen = BTreeSet::new();
        for packet in packets {
            if packet.release_daa >= packet.service_before
                || packet.service_before > hard_deadline
                || packet.service_before - packet.release_daa > crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA
                || packet.cost.controls != 1
                || packet.cost.bytes == 0
                || packet.cost.work == 0
            {
                return Err(ServiceRefusal::InvalidInterval);
            }
            if self.assignments.contains_key(&packet.id) || !seen.insert(packet.id) {
                return Err(ServiceRefusal::DuplicateIdentity);
            }
            if self.budget.subtract(packet.cost).is_none() {
                return Err(ServiceRefusal::NoCapacity);
            }
        }
        let mut candidate = self.clone();
        let mut ordered: Vec<_> = packets.iter().collect();
        ordered.sort_by_key(|packet| (packet.service_before, packet.id));
        for packet in ordered {
            let slot = (packet.release_daa..packet.service_before)
                .find_map(|daa| {
                    let left = candidate.remaining.get(&daa).copied().unwrap_or(candidate.budget);
                    left.subtract(packet.cost).map(|remaining| (daa, remaining))
                })
                .ok_or(ServiceRefusal::NoCapacity)?;
            candidate.remaining.insert(slot.0, slot.1);
            candidate.assignments.insert(packet.id, slot.0);
        }
        *self = candidate;
        Ok(())
    }

    pub fn assigned_daa(&self, id: &[u8; 64]) -> Option<u64> {
        self.assignments.get(id).copied()
    }
}
