//! Causal all-branch scheduling component for RFC-0026. The original coordinator
//! must derive the complete graph, authenticated actors and funded real costs.
//! A planned slot is not prerequisite completion, delivery, supply or default.
use super::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations};
use std::collections::{BTreeMap, BTreeSet};
type Id = [u8; 64];
/// Internal planning input, not a peer-deserializable admission certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalServicePacketV1 {
    pub packet: ServicePacket,
    pub predecessors: Vec<Id>,
}

/// Fixed-size explanation of the first capacity refusal. This is not a quote
/// for replacement capacity, a complete feasibility proof or an admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalCapacityStageV1 {
    AtomicPacketExceedsBudget,
    NoFreeSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CausalCapacityRefusalV1 {
    pub stage: CausalCapacityStageV1,
    pub packet: crate::Hash64,
    pub required: ServiceCost,
    pub budget: ServiceCost,
    /// Includes every predecessor's proposed slot plus one, for NoFreeSlot.
    pub effective_release_daa: u64,
    pub service_before: u64,
    pub existing_packets: usize,
    /// Placements in the disposable candidate; all are rolled back on refusal.
    pub proposed_packets: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CausalReservationErrorV1 {
    pub refusal: ServiceRefusal,
    pub capacity: Option<CausalCapacityRefusalV1>,
}

impl ServiceReservations {
    /// Atomic conservative topological first-fit. Every dependent packet starts
    /// strictly after all its predecessors' reserved slots. Existing assignments
    /// never move. Ready nodes are ordered by deadline then canonical identity.
    /// No wider interval/hard deadline, probabilistic overbooking or owner input.
    /// The caller budgets graph/index/clone ownership and slot-search work first.
    pub fn reserve_causal_v1(&mut self, graph: &[CausalServicePacketV1], hard: u64) -> Result<(), ServiceRefusal> {
        self.reserve_causal_checked_v1(graph, hard, |_| Ok(()))
    }

    /// The identical atomic scheduler with a bounded failure witness. No
    /// retry, widened window or capacity change is performed on a failure.
    pub fn reserve_causal_with_diagnostic_v1(
        &mut self,
        graph: &[CausalServicePacketV1],
        hard: u64,
    ) -> Result<(), CausalReservationErrorV1> {
        self.reserve_causal_checked_diagnostic_v1(graph, hard, |_| Ok(()))
    }

    /// Run a native constraint on the proposed assignments before committing
    /// the candidate. A constraint failure leaves the original book intact.
    pub(crate) fn reserve_causal_checked_v1(
        &mut self,
        graph: &[CausalServicePacketV1],
        hard: u64,
        check_assignments: impl FnOnce(&Self) -> Result<(), ServiceRefusal>,
    ) -> Result<(), ServiceRefusal> {
        self.reserve_causal_checked_diagnostic_v1(graph, hard, check_assignments).map_err(|error| error.refusal)
    }

    pub(crate) fn reserve_causal_checked_diagnostic_v1(
        &mut self,
        graph: &[CausalServicePacketV1],
        hard: u64,
        check_assignments: impl FnOnce(&Self) -> Result<(), ServiceRefusal>,
    ) -> Result<(), CausalReservationErrorV1> {
        let mut capacity = None;
        self.reserve_causal_checked_inner_v1(graph, hard, check_assignments, &mut capacity)
            .map_err(|refusal| CausalReservationErrorV1 { refusal, capacity })
    }

    fn reserve_causal_checked_inner_v1(
        &mut self,
        graph: &[CausalServicePacketV1],
        hard: u64,
        check_assignments: impl FnOnce(&Self) -> Result<(), ServiceRefusal>,
        capacity: &mut Option<CausalCapacityRefusalV1>,
    ) -> Result<(), ServiceRefusal> {
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
                *capacity = Some(CausalCapacityRefusalV1 {
                    stage: CausalCapacityStageV1::AtomicPacketExceedsBudget,
                    packet: crate::Hash64::from_bytes(p.id),
                    required: p.cost,
                    budget: self.budget,
                    effective_release_daa: p.release_daa,
                    service_before: p.service_before,
                    existing_packets: self.packets.len(),
                    proposed_packets: 0,
                });
                #[cfg(test)]
                self.write_capacity_diagnostic_v1(graph, hard, p.id);
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
                .ok_or_else(|| {
                    *capacity = Some(CausalCapacityRefusalV1 {
                        stage: CausalCapacityStageV1::NoFreeSlot,
                        packet: crate::Hash64::from_bytes(p.id),
                        required: p.cost,
                        budget: self.budget,
                        effective_release_daa: p.release_daa,
                        service_before: p.service_before,
                        existing_packets: self.packets.len(),
                        proposed_packets: done,
                    });
                    ServiceRefusal::NoCapacity
                })?;
            candidate.remaining.insert(slot, left);
            candidate.assignments.insert(id, slot);
            candidate.packets.insert(id, n.packet.clone());
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
        check_assignments(&candidate)?;
        *self = candidate;
        Ok(())
    }

    /// Opt-in test diagnostic of the actual refused input. It does not change
    /// costs, reserve slots, or establish that the unbooked graph is valid.
    /// Keep this out of daemon builds and native resource measurements.
    #[cfg(test)]
    fn write_capacity_diagnostic_v1(&self, graph: &[CausalServicePacketV1], hard: u64, refused: Id) {
        use std::io::Write;
        let Some(path) = std::env::var_os("MISAKA_PALW_CAPACITY_DIAGNOSTIC_V1") else { return };
        let cost = |c: super::ServiceCost| {
            serde_json::json!({
                "bytes": c.bytes, "work": c.work, "io": c.io, "controls": c.controls,
            })
        };
        let id = |value: Id| crate::Hash64::from_bytes(value).to_string();
        let value = serde_json::json!({
            "version": 1,
            "kind": "unbookedCausalServiceInput",
            "hard": hard,
            "budget": cost(self.budget),
            "existingPacketCount": self.packets.len(),
            "lastObservedDaa": self.last_observed_daa,
            "refusedPacket": id(refused),
            "graph": graph.iter().map(|node| serde_json::json!({
                "id": id(node.packet.id),
                "release": node.packet.release_daa,
                "before": node.packet.service_before,
                "cost": cost(node.packet.cost),
                "predecessors": node.predecessors.iter().map(|value| id(*value)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
            serde_json::to_writer(&mut file, &value)?;
            file.write_all(b"\n")?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("capacity diagnostic unavailable: {error}");
        }
    }
    /// Conservative recovery: preserve every original promise and supplied slot.
    /// If ordinary repair would move a prerequisite after a retained descendant,
    /// reject the whole proposal and retain the no-slash hold. This is not a
    /// complete causal replanner and never manufactures prerequisite completion.
    pub fn repair_missing_causal_v1(&mut self, graph: &[CausalServicePacketV1], hard: u64) -> Result<usize, ServiceRefusal> {
        self.check_causal_v1(graph)?;
        let ids = graph.iter().map(|n| n.packet.id).collect();
        let mut candidate = self.clone();
        let repaired = candidate.repair_missing(&ids, hard)?;
        candidate.check_causal_v1(graph)?;
        *self = candidate;
        Ok(repaired)
    }
    /// Recheck a projection against the independently retained/derived canonical
    /// graph after load or a proposed repair. The book alone stores no graph and
    /// cannot certify completeness/causality. A native writer must retain both.
    pub fn check_causal_v1(&self, graph: &[CausalServicePacketV1]) -> Result<(), ServiceRefusal> {
        self.check().map_err(|_| ServiceRefusal::InvalidState)?;
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
                || stored.release_daa != n.packet.release_daa
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
    fn repair_cannot_move_a_missing_parent_after_a_retained_child() {
        let g = vec![packet(1, 10, &[]), packet(2, 10, &[1])];
        let mut b = book();
        b.reserve_causal_v1(&g, 10).unwrap();
        // The accepting chain skips the parent's slot, then supplies the child's.
        b.observe_block(2, [8; 64], ServiceCost::default()).unwrap();
        let saved = b.clone();
        let mut ordinary = b.clone();
        assert_eq!(ordinary.repair_missing(&g.iter().map(|n| n.packet.id).collect(), 10), Ok(1));
        assert!(ordinary.check_causal_v1(&g).is_err());
        assert_eq!(b.repair_missing_causal_v1(&g, 10), Err(ServiceRefusal::InvalidState));
        assert_eq!(b, saved);
        assert_eq!(b.packet(&[2; 64]), Some(&g[1].packet));
    }
    #[test]
    fn insufficient_aggregate_capacity_rolls_back_already_assigned_roots() {
        let g = vec![packet(1, 3, &[]), packet(2, 3, &[]), packet(3, 3, &[])];
        let mut b = book();
        let saved = b.clone();
        assert_eq!(b.reserve_causal_v1(&g, 3), Err(ServiceRefusal::NoCapacity));
        assert_eq!(b, saved);
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

    #[test]
    fn diagnostics_distinguish_each_atomic_dimension_without_changing_refusal() {
        for dimension in 0..3 {
            let mut graph = vec![packet(1, 8, &[])];
            let budget = ServiceCost { bytes: 2, work: 2, io: 2, controls: 1 };
            match dimension {
                0 => graph[0].packet.cost.bytes = 3,
                1 => graph[0].packet.cost.work = 3,
                2 => graph[0].packet.cost.io = 3,
                _ => unreachable!(),
            }
            let mut observed = ServiceReservations::new(budget).unwrap();
            let saved = observed.clone();
            let failure = observed.reserve_causal_with_diagnostic_v1(&graph, 8).unwrap_err();
            assert_eq!(failure.refusal, ServiceRefusal::NoCapacity);
            let witness = failure.capacity.unwrap();
            assert_eq!(witness.stage, CausalCapacityStageV1::AtomicPacketExceedsBudget);
            assert_eq!(witness.packet.as_bytes(), graph[0].packet.id);
            assert_eq!(witness.required, graph[0].packet.cost);
            assert_eq!(witness.budget, budget);
            assert_eq!(witness.effective_release_daa, 1);
            assert_eq!(witness.service_before, 8);
            assert_eq!((witness.existing_packets, witness.proposed_packets), (0, 0));
            assert_eq!(observed, saved);
            assert_eq!(observed.reserve_causal_v1(&graph, 8), Err(failure.refusal));
            assert_eq!(observed, saved);
        }
    }

    #[test]
    fn occupied_window_diagnostic_keeps_causal_release_and_rolls_back_candidate() {
        let mut observed = book();
        observed.reserve(&[packet(9, 8, &[]).packet], 8).unwrap();
        let saved = observed.clone();
        let graph = vec![packet(1, 4, &[]), packet(2, 4, &[1]), packet(3, 4, &[1])];
        let failure = observed.reserve_causal_with_diagnostic_v1(&graph, 8).unwrap_err();
        assert_eq!(failure.refusal, ServiceRefusal::NoCapacity);
        let witness = failure.capacity.unwrap();
        assert_eq!(witness.stage, CausalCapacityStageV1::NoFreeSlot);
        assert_eq!(witness.packet.as_bytes(), [3; 64]);
        assert_eq!((witness.effective_release_daa, witness.service_before), (3, 4));
        assert_eq!((witness.existing_packets, witness.proposed_packets), (1, 2));
        assert_eq!(observed, saved);
        assert_eq!(observed.reserve_causal_v1(&graph, 8), Err(failure.refusal));
        assert_eq!(observed, saved);
    }

    #[test]
    fn diagnostic_entry_preserves_success_and_noncapacity_constraint_failure() {
        let graph = vec![packet(1, 8, &[]), packet(2, 8, &[1])];
        let mut old = book();
        let mut observed = old.clone();
        old.reserve_causal_v1(&graph, 8).unwrap();
        observed.reserve_causal_with_diagnostic_v1(&graph, 8).unwrap();
        assert_eq!(old, observed);
        let mut observed = book();
        let saved = observed.clone();
        let failure = observed.reserve_causal_checked_diagnostic_v1(&graph, 8, |_| Err(ServiceRefusal::InvalidBudget)).unwrap_err();
        assert_eq!(failure, CausalReservationErrorV1 { refusal: ServiceRefusal::InvalidBudget, capacity: None });
        assert_eq!(observed, saved);
        let deadline_graph = vec![packet(1, 4, &[]), packet(2, 2, &[1])];
        let failure = observed.reserve_causal_with_diagnostic_v1(&deadline_graph, 4).unwrap_err();
        assert_eq!(failure, CausalReservationErrorV1 { refusal: ServiceRefusal::DeadlineExhausted, capacity: None });
        assert_eq!(observed, saved);
    }
}
