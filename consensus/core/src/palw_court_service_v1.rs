//! RFC-0026 service reservation component. Persistable in a proposed rooted
//! deny-only adapter; not wired to original admission, actual block supply,
//! default permission or any preset. The caller must derive the COMPLETE canonical
//! packet set from an adopted profile, including every allowed hostile branch.
//! A successful reservation does not establish network/inclusion assumptions.

pub mod causal;

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// Independent deterministic capacity dimensions; never measured runtime CPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
    InvalidState,
    DeadlineExhausted,
    /// G14 lane r6-d1-release: recovery would schedule a packet at or after a packet that causally depends on it (or a dependent at
    /// or before its predecessor). The move is refused; the packet stays `MissingSupply` (a no-slash hold).
    CausalOrder,
}

/// A qualified empty block still offers service under the explicit inclusion
/// assumption. This is resource accounting, never evidence of personal delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ServiceReservations {
    pub(crate) budget: ServiceCost,
    pub(crate) remaining: BTreeMap<u64, ServiceCost>,
    pub(crate) assignments: BTreeMap<[u8; 64], u64>,
    pub(crate) packets: BTreeMap<[u8; 64], ServicePacket>,
    pub(crate) supply: BTreeMap<u64, ServiceSupply>,
    pub(crate) last_observed_daa: Option<u64>,
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

    /// Validate persisted accounting; never trust cached residual quota from a snapshot.
    pub fn check(&self) -> Result<(), &'static str> {
        let mut rebuilt = Self::new(self.budget).map_err(|_| "invalid service budget")?;
        if self.assignments.len() != self.packets.len() {
            return Err("service packet/assignment mismatch");
        }
        for (id, packet) in &self.packets {
            if id != &packet.id
                || packet.release_daa >= packet.service_before
                || packet.service_before - packet.release_daa > crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA
                || packet.cost.controls != 1
                || packet.cost.bytes == 0
                || packet.cost.work == 0
            {
                return Err("invalid service packet");
            }
            let assigned = *self.assignments.get(id).ok_or("missing service assignment")?;
            if !(packet.release_daa..packet.service_before).contains(&assigned) {
                return Err("assignment outside service interval");
            }
            let remaining = rebuilt
                .remaining
                .get(&assigned)
                .copied()
                .unwrap_or(self.budget)
                .subtract(packet.cost)
                .ok_or("overbooked persisted service")?;
            rebuilt.remaining.insert(assigned, remaining);
        }
        if rebuilt.remaining != self.remaining {
            return Err("forged residual service quota");
        }
        for (daa, supplied) in &self.supply {
            if self.last_observed_daa.is_none_or(|last| *daa > last) {
                return Err("service supply beyond observed clock");
            }
            let remaining = self.remaining.get(daa).ok_or("unreserved service history")?;
            if supplied.reserved != self.budget.subtract(*remaining).ok_or("invalid service residual")? {
                return Err("service supply differs from reservation");
            }
        }
        Ok(())
    }

    /// Retry only slots that the accepted clock has PASSED without supply.
    /// Packet identity, cost and original eligibility interval do not change.
    /// All selected missing packets move atomically; other assignments and all
    /// supplied history remain untouched. This does not authorize default.
    pub fn repair_missing(&mut self, ids: &BTreeSet<[u8; 64]>, hard_deadline: u64) -> Result<usize, ServiceRefusal> {
        self.repair_missing_ordered_v1(ids, hard_deadline, &BTreeMap::new())
    }

    /// **G14 lane r6-d1-release: [`Self::repair_missing`] that keeps causal order.** `edges` maps a packet to its causal
    /// predecessors (`CourtServiceRowV3::causal`). Missed packets are placed in topological order (ready packets by deadline then
    /// identity — with no edges exactly the legacy order), each no earlier than one DAA after every predecessor's (possibly just
    /// moved) slot; afterwards every edge touching `ids` must still run strictly forward, else the whole move is refused with
    /// [`ServiceRefusal::CausalOrder`] (nothing changes; the hold stays). With an empty `edges` this is the legacy sweep.
    pub fn repair_missing_ordered_v1(
        &mut self,
        ids: &BTreeSet<[u8; 64]>,
        hard_deadline: u64,
        edges: &BTreeMap<[u8; 64], Vec<[u8; 64]>>,
    ) -> Result<usize, ServiceRefusal> {
        self.check().map_err(|_| ServiceRefusal::InvalidState)?;
        let Some(now) = self.last_observed_daa else {
            if ids.iter().any(|id| !self.packets.contains_key(id)) {
                return Err(ServiceRefusal::InvalidState);
            }
            return Ok(0);
        };
        let mut missing = Vec::new();
        for id in ids {
            let packet = self.packets.get(id).ok_or(ServiceRefusal::InvalidState)?;
            let assigned = *self.assignments.get(id).ok_or(ServiceRefusal::InvalidState)?;
            if packet.service_before > hard_deadline {
                return Err(ServiceRefusal::InvalidInterval);
            }
            if assigned < now && !self.supply.contains_key(&assigned) {
                missing.push(packet.clone());
            }
        }
        if missing.is_empty() {
            return Ok(0);
        }
        let next = now.checked_add(1).ok_or(ServiceRefusal::DeadlineExhausted)?;
        if missing.iter().any(|packet| next >= packet.service_before) {
            return Err(ServiceRefusal::DeadlineExhausted);
        }
        // Topological order over the missed set (ready by deadline then identity: the legacy order when no edge applies).
        let by_id: BTreeMap<[u8; 64], &ServicePacket> = missing.iter().map(|p| (p.id, p)).collect();
        let mut waiting: BTreeMap<[u8; 64], usize> = BTreeMap::new();
        let mut children: BTreeMap<[u8; 64], Vec<[u8; 64]>> = BTreeMap::new();
        let mut ready = BTreeSet::new();
        for packet in &missing {
            let preds = edges.get(&packet.id).map(Vec::as_slice).unwrap_or(&[]);
            let mut count = 0usize;
            for pred in preds {
                if by_id.contains_key(pred) {
                    count += 1;
                    children.entry(*pred).or_default().push(packet.id);
                }
            }
            waiting.insert(packet.id, count);
            if count == 0 {
                ready.insert((packet.service_before, packet.id));
            }
        }
        let mut order = Vec::with_capacity(missing.len());
        while let Some((_, id)) = ready.pop_first() {
            order.push(by_id[&id]);
            for child in children.get(&id).into_iter().flatten() {
                let count = waiting.get_mut(child).ok_or(ServiceRefusal::InvalidState)?;
                *count -= 1;
                if *count == 0 {
                    ready.insert((by_id[child].service_before, *child));
                }
            }
        }
        if order.len() != missing.len() {
            return Err(ServiceRefusal::InvalidState);
        }
        let mut candidate = self.clone();
        for packet in order {
            let old = candidate.assignments[&packet.id];
            let left = candidate.remaining.get(&old).copied().ok_or(ServiceRefusal::InvalidState)?;
            let returned = ServiceCost {
                bytes: left.bytes.checked_add(packet.cost.bytes).ok_or(ServiceRefusal::InvalidState)?,
                work: left.work.checked_add(packet.cost.work).ok_or(ServiceRefusal::InvalidState)?,
                io: left.io.checked_add(packet.cost.io).ok_or(ServiceRefusal::InvalidState)?,
                controls: left.controls.checked_add(packet.cost.controls).ok_or(ServiceRefusal::InvalidState)?,
            };
            candidate.budget.subtract(returned).ok_or(ServiceRefusal::InvalidState)?;
            if returned == candidate.budget {
                candidate.remaining.remove(&old);
            } else {
                candidate.remaining.insert(old, returned);
            }
            let mut floor = next.max(packet.release_daa);
            for pred in edges.get(&packet.id).map(Vec::as_slice).unwrap_or(&[]) {
                let after = candidate.assignments.get(pred).ok_or(ServiceRefusal::InvalidState)?.checked_add(1)
                    .ok_or(ServiceRefusal::DeadlineExhausted)?;
                floor = floor.max(after);
            }
            let (slot, residual) = (floor..packet.service_before)
                .find_map(|daa| {
                    candidate.remaining.get(&daa).copied().unwrap_or(candidate.budget).subtract(packet.cost).map(|left| (daa, left))
                })
                .ok_or(if floor > next.max(packet.release_daa) { ServiceRefusal::CausalOrder } else { ServiceRefusal::NoCapacity })?;
            candidate.remaining.insert(slot, residual);
            candidate.assignments.insert(packet.id, slot);
        }
        // Every edge touching the moved set must still run strictly forward (a retained dependent may now sit at or before its
        // moved predecessor).
        for (child, preds) in edges {
            for pred in preds {
                if !ids.contains(child) && !ids.contains(pred) {
                    continue;
                }
                let (Some(c), Some(p)) = (candidate.assignments.get(child), candidate.assignments.get(pred)) else { continue };
                if c <= p {
                    return Err(ServiceRefusal::CausalOrder);
                }
            }
        }
        candidate.check().map_err(|_| ServiceRefusal::InvalidState)?;
        *self = candidate;
        Ok(missing.len())
    }

    pub fn packet(&self, id: &[u8; 64]) -> Option<&ServicePacket> {
        self.packets.get(id)
    }

    pub fn assigned_daa(&self, id: &[u8; 64]) -> Option<u64> {
        self.assignments.get(id).copied()
    }
}
