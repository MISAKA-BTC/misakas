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
    /// The proposed block spends capacity reserved for accepted obligations.
    ReservedCapacitySpent,
    /// Accepted-chain DAA may repeat, but cannot move backwards without rollback.
    DaaWentBackwards,
    /// A historical reservation cannot be added after its service unit occurred.
    ReservationInThePast,
}

/// A qualified empty block still offers service under the explicit inclusion
/// assumption. This is resource accounting, never evidence of personal delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceSupply {
    pub accepting_block: [u8; 64],
    pub reserved: ServiceCost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceObservation {
    FirstBlock,
    AdditionalBlockAtSameDaa,
}

/// None of these outcomes authorizes slashing. An opportunity is only one of
/// RFC-0026's prerequisites; duty, signatures, response eligibility and adopted
/// network/inclusion assumptions must be checked by the future fold adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceAssessment {
    UnknownObligation,
    Pending,
    OpportunityProvided,
    /// Keep Final, collateral and evidence on hold; never infer party default.
    MissingSupply,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceReservations {
    budget: ServiceCost,
    remaining: BTreeMap<u64, ServiceCost>,
    assignments: BTreeMap<[u8; 64], u64>,
    packets: BTreeMap<[u8; 64], ServicePacket>,
    supply: BTreeMap<u64, ServiceSupply>,
    last_observed_daa: Option<u64>,
}

impl ServiceReservations {
    pub fn new(budget: ServiceCost) -> Result<Self, ServiceRefusal> {
        if budget.bytes == 0 || budget.work == 0 || budget.io == 0 || budget.controls == 0 {
            return Err(ServiceRefusal::InvalidBudget);
        }
        Ok(Self {
            budget,
            remaining: BTreeMap::new(),
            assignments: BTreeMap::new(),
            packets: BTreeMap::new(),
            supply: BTreeMap::new(),
            last_observed_daa: None,
        })
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
            if self.last_observed_daa.is_some_and(|daa| packet.release_daa <= daa) {
                return Err(ServiceRefusal::ReservationInThePast);
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
            candidate.packets.insert(packet.id, packet.clone());
        }
        *self = candidate;
        Ok(())
    }

    /// Account one accepting-chain block using ACTUAL deterministic cost of
    /// unreserved court operations. The caller must derive that cost from the
    /// admitted codec objects and the pinned policy, not a producer/Panel flag.
    /// Ordinary transaction capacity must be disjoint from `budget` in block
    /// admission. Those adapters are not wired yet: this component cannot arm.
    ///
    /// The first block at a DAA must leave every assigned packet's quota intact;
    /// later blocks cannot repair or duplicate that first opportunity. Skipped
    /// DAA positions have no supply record. No loop over skipped DAA is needed.
    pub fn observe_block(
        &mut self,
        accepting_daa: u64,
        accepting_block: [u8; 64],
        unreserved_court_cost: ServiceCost,
    ) -> Result<ServiceObservation, ServiceRefusal> {
        if self.last_observed_daa.is_some_and(|last| accepting_daa < last) {
            return Err(ServiceRefusal::DaaWentBackwards);
        }
        let free = self.budget.subtract(unreserved_court_cost).ok_or(ServiceRefusal::ReservedCapacitySpent)?;
        if self.last_observed_daa == Some(accepting_daa) {
            return Ok(ServiceObservation::AdditionalBlockAtSameDaa);
        }
        let remaining = self.remaining.get(&accepting_daa).copied().unwrap_or(self.budget);
        let reserved = self.budget.subtract(remaining).expect("remaining quota is derived by checked subtraction");
        free.subtract(reserved).ok_or(ServiceRefusal::ReservedCapacitySpent)?;
        // Empty blocks and silence still advance the absolute observation clock.
        if reserved.controls != 0 {
            self.supply.insert(accepting_daa, ServiceSupply { accepting_block, reserved });
        }
        self.last_observed_daa = Some(accepting_daa);
        Ok(ServiceObservation::FirstBlock)
    }

    pub fn supply_at(&self, daa: u64) -> Option<&ServiceSupply> {
        self.supply.get(&daa)
    }

    /// Assess against the accepting chain's observed clock, never wall time or
    /// mempool reports. A missing first block cannot become miner culpability.
    pub fn assess(&self, id: &[u8; 64]) -> ServiceAssessment {
        let Some(packet) = self.packets.get(id) else { return ServiceAssessment::UnknownObligation };
        if self.last_observed_daa.is_none_or(|daa| daa < packet.service_before) {
            return ServiceAssessment::Pending;
        }
        let Some(assigned) = self.assignments.get(id) else { return ServiceAssessment::UnknownObligation };
        if self.supply.contains_key(assigned) { ServiceAssessment::OpportunityProvided } else { ServiceAssessment::MissingSupply }
    }

    pub fn assigned_daa(&self, id: &[u8; 64]) -> Option<u64> {
        self.assignments.get(id).copied()
    }
}
