//! Causal all-branch scheduling component for RFC-0026. The original coordinator
//! must derive the complete graph, authenticated actors and funded real costs.
//! A planned slot is not prerequisite completion, delivery, supply or default.
use super::{ServicePacket, ServiceRefusal, ServiceReservations};
use std::collections::{BTreeMap, BTreeSet};
type Id = [u8; 64];
/// Internal planning input, not a peer-deserializable admission certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalServicePacketV1 {
    pub packet: ServicePacket,
    pub predecessors: Vec<Id>,
}
impl ServiceReservations {
    /// Atomic conservative topological first-fit. Every dependent packet starts
    /// strictly after all its predecessors' reserved slots. Existing assignments
    /// never move. Ready nodes are ordered by deadline then canonical identity.
    /// No wider interval/hard deadline, probabilistic overbooking or owner input.
    /// The caller budgets graph/index/clone ownership and slot-search work first.
    pub fn reserve_causal_v1(&mut self, graph: &[CausalServicePacketV1], hard: u64) -> Result<(), ServiceRefusal> {
        if graph.is_empty() {
            return Err(ServiceRefusal::InvalidState);
        }
        let mut nodes = BTreeMap::new();
        for n in graph {
            let p = &n.packet;
            if nodes.insert(p.id, n).is_some() || self.packets.contains_key(&p.id) {
                return Err(ServiceRefusal::DuplicateIdentity);
            }
            if p.release_daa >= p.service_before
                || p.service_before > hard
                || p.service_before - p.release_daa > crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA
                || p.cost.controls != 1
                || p.cost.bytes == 0
                || p.cost.work == 0
            {
                return Err(ServiceRefusal::InvalidInterval);
            }
            if self.last_observed_daa.is_some_and(|d| p.release_daa <= d) {
                return Err(ServiceRefusal::ReservationInThePast);
            }
            if self.budget.subtract(p.cost).is_none() {
                return Err(ServiceRefusal::NoCapacity);
            }
        }
        let mut pending = BTreeMap::new();
        let mut children: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        let mut ready = BTreeSet::new();
        for n in graph {
            let mut seen = BTreeSet::new();
            for predecessor in &n.predecessors {
                // Cross-original/external dependencies require authenticated native
                // contracts in the coordinator; an arbitrary existing packet cannot
                // be borrowed as an authority or a completed prerequisite here.
                if !nodes.contains_key(predecessor) || !seen.insert(*predecessor) {
                    return Err(ServiceRefusal::InvalidState);
                }
                children.entry(*predecessor).or_default().push(n.packet.id);
            }
            pending.insert(n.packet.id, n.predecessors.len());
            if n.predecessors.is_empty() {
                ready.insert((n.packet.service_before, n.packet.id));
            }
        }
        let mut candidate = self.clone();
        let mut done = 0;
        while let Some((_, id)) = ready.pop_first() {
            let n = nodes[&id];
            let mut p = n.packet.clone();
            for predecessor in &n.predecessors {
                let after = candidate
                    .assignments
                    .get(predecessor)
                    .ok_or(ServiceRefusal::InvalidState)?
                    .checked_add(1)
                    .ok_or(ServiceRefusal::DeadlineExhausted)?;
                p.release_daa = p.release_daa.max(after);
            }
            if p.release_daa >= p.service_before {
                return Err(ServiceRefusal::DeadlineExhausted);
            }
            let (slot, left) = (p.release_daa..p.service_before)
                .find_map(|d| candidate.remaining.get(&d).copied().unwrap_or(candidate.budget).subtract(p.cost).map(|left| (d, left)))
                .ok_or(ServiceRefusal::NoCapacity)?;
            candidate.remaining.insert(slot, left);
            candidate.assignments.insert(id, slot);
            candidate.packets.insert(id, p);
            done += 1;
            for child in children.get(&id).into_iter().flatten() {
                let count = pending.get_mut(child).ok_or(ServiceRefusal::InvalidState)?;
                *count = count.checked_sub(1).ok_or(ServiceRefusal::InvalidState)?;
                if *count == 0 {
                    ready.insert((nodes[child].packet.service_before, *child));
                }
            }
        }
        if done != graph.len() {
            return Err(ServiceRefusal::InvalidState);
        }
        candidate.check().map_err(|_| ServiceRefusal::InvalidState)?;
        candidate.check_causal_v1(graph)?;
        *self = candidate;
        Ok(())
    }
    /// Recheck a projection against the independently retained/derived canonical
    /// graph after load or a proposed repair. The book alone stores no graph and
    /// cannot certify completeness/causality. A native writer must retain both.
    pub fn check_causal_v1(&self, graph: &[CausalServicePacketV1]) -> Result<(), ServiceRefusal> {
        if graph.is_empty() {
            return Err(ServiceRefusal::InvalidState);
        }
        let mut seen = BTreeSet::new();
        for n in graph {
            if !seen.insert(n.packet.id) {
                return Err(ServiceRefusal::DuplicateIdentity);
            }
        }
        for n in graph {
            let stored = self.packet(&n.packet.id).ok_or(ServiceRefusal::InvalidState)?;
            let mut release = n.packet.release_daa;
            let mut parents = BTreeSet::new();
            for predecessor in &n.predecessors {
                if !seen.contains(predecessor) || !parents.insert(*predecessor) {
                    return Err(ServiceRefusal::InvalidState);
                }
                release = release.max(
                    self.assigned_daa(predecessor)
                        .ok_or(ServiceRefusal::InvalidState)?
                        .checked_add(1)
                        .ok_or(ServiceRefusal::DeadlineExhausted)?,
                );
            }
            if stored.id != n.packet.id
                || stored.cost != n.packet.cost
                || stored.service_before != n.packet.service_before
                || stored.release_daa != release
                || self.assigned_daa(&stored.id).is_none_or(|d| d < release)
            {
                return Err(ServiceRefusal::InvalidState);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_service_v1::ServiceCost;
    fn packet(id: u8, before: u64, parents: &[u8]) -> CausalServicePacketV1 {
        CausalServicePacketV1 {
            packet: ServicePacket {
                id: [id; 64],
                release_daa: 1,
                service_before: before,
                cost: ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 },
            },
            predecessors: parents.iter().map(|i| [*i; 64]).collect(),
        }
    }
    fn book() -> ServiceReservations {
        ServiceReservations::new(ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 }).unwrap()
    }
    #[test]
    fn causal_schedule_keeps_all_siblings_and_is_independent_of_input_order() {
        let g = vec![packet(3, 8, &[1]), packet(2, 8, &[1]), packet(1, 8, &[])];
        let mut a = book();
        a.reserve_causal_v1(&g, 8).unwrap();
        assert_eq!((a.assigned_daa(&[1; 64]), a.assigned_daa(&[2; 64]), a.assigned_daa(&[3; 64])), (Some(1), Some(2), Some(3)));
        let mut reversed = g.clone();
        reversed.reverse();
        let mut b = book();
        b.reserve_causal_v1(&reversed, 8).unwrap();
        assert_eq!(a, b);
        let loaded: ServiceReservations = borsh::from_slice(&borsh::to_vec(&a).unwrap()).unwrap();
        loaded.check_causal_v1(&g).unwrap();
    }
    #[test]
    fn ordinary_edf_can_schedule_a_response_before_its_parent_but_causal_admission_refuses() {
        let g = vec![packet(1, 4, &[]), packet(2, 2, &[1])];
        let mut plain = book();
        plain.reserve(&g.iter().map(|n| n.packet.clone()).collect::<Vec<_>>(), 4).unwrap();
        assert!(plain.assigned_daa(&[2; 64]) < plain.assigned_daa(&[1; 64]));
        assert!(plain.check_causal_v1(&g).is_err());
        let mut causal = book();
        let saved = causal.clone();
        assert_eq!(causal.reserve_causal_v1(&g, 4), Err(ServiceRefusal::DeadlineExhausted));
        assert_eq!(causal, saved);
    }
    #[test]
    fn malformed_graphs_and_partial_capacity_failure_leave_the_book_unchanged() {
        for g in [
            vec![packet(1, 8, &[2]), packet(2, 8, &[1])],
            vec![packet(1, 8, &[9])],
            vec![packet(1, 8, &[]), packet(2, 8, &[1, 1])],
            vec![packet(1, 8, &[]), packet(1, 8, &[])],
            vec![packet(1, 8, &[]), packet(2, 8, &[1]), packet(3, 3, &[2])],
        ] {
            let mut b = book();
            let saved = b.clone();
            assert!(b.reserve_causal_v1(&g, 8).is_err());
            assert_eq!(b, saved);
        }
    }
    #[test]
    fn existing_slots_are_preserved_and_a_join_waits_for_every_predecessor() {
        let mut b = book();
        b.reserve(&[packet(9, 8, &[]).packet], 8).unwrap();
        let old = b.assigned_daa(&[9; 64]);
        let g = vec![packet(1, 8, &[]), packet(2, 8, &[]), packet(3, 8, &[1, 2])];
        b.reserve_causal_v1(&g, 8).unwrap();
        assert_eq!(b.assigned_daa(&[9; 64]), old);
        assert!(b.assigned_daa(&[3; 64]) > b.assigned_daa(&[2; 64]));
        let mut broken = b.clone();
        broken.assignments.insert([1; 64], 7);
        assert!(broken.check_causal_v1(&g).is_err());
    }
}
