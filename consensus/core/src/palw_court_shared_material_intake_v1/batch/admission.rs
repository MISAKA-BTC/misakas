//! Preclaim capacity projection for bounded native Batch material paths.
use super::{BatchResponsePositionV1, MaterialIntakeScheduleV1, PART};
use crate::{
    Hash64,
    palw_batch_court_v1::{BatchCourtErrorV1, BatchCourtWindowV1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1},
    palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA},
    palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1},
};
use std::collections::BTreeSet;

/// Supplied by the native profile compiler. A fallback demand has its own
/// ordinal and depends on the prior opportunity, even if that response is absent.
pub(crate) struct MaximumBatchDemandV1<'a> {
    pub ordinal: u64,
    pub range_maximums: &'a [u64],
    pub predecessors: &'a [[u8; 64]],
}

/// Scope is derived from the prepared original. The caller supplies every
/// legal demand and the other native control, verification and terminal nodes.
pub(crate) struct BatchAdmissionV1<'a> {
    pub claim: Hash64,
    pub scope: Hash64,
    pub window: BatchCourtWindowV1,
    pub demands: &'a [MaximumBatchDemandV1<'a>],
    pub terminal: [u8; 64],
    pub recovery_daa: u64,
    pub maximum_packets: usize,
}

impl BatchAdmissionV1<'_> {
    /// Extend a privately prepared original's service row before its single
    /// journalled write. The caller supplies the current native admission
    /// candidate, never an already published row to change in place.
    pub(crate) fn reserve_for_original_v1(
        &self,
        mut candidate: crate::palw_court_service_release_v1::CourtServiceRowV3,
        graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        cost: impl FnMut(u64) -> Result<ServiceCost, String>,
    ) -> Result<(u64, crate::palw_court_service_release_v1::CourtServiceRowV3), BatchCourtErrorV1> {
        let binding = candidate.state.claims.get_mut(&self.claim).ok_or(BatchCourtErrorV1::CaseGraph)?;
        if binding.original_accepted_daa != self.window.claim_accepted_daa
            || binding.original_accepted_daa.checked_add(DISPUTE_WINDOW_DAA) != Some(binding.hard_deadline_daa)
        {
            return Err(BatchCourtErrorV1::Window);
        }
        let (required, graph) = self.reserve_v1(&mut candidate.state.book, graph, storage, cost)?;
        for node in graph {
            binding.packet_ids.insert(node.packet.id);
            if !node.predecessors.is_empty() {
                let mut predecessors = node.predecessors;
                predecessors.sort_unstable();
                candidate.causal.insert(node.packet.id, predecessors);
            }
        }
        candidate.check().map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        Ok((required, candidate))
    }

    /// Add exact worst-case BCT1 byte sizes and causal edges to the native
    /// graph. Returned packets must remain with the original's rooted binding.
    /// No notice, request root, or responder needs to exist when this runs.
    pub(crate) fn reserve_v1(
        &self,
        book: &mut ServiceReservations,
        mut graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        mut cost: impl FnMut(u64) -> Result<ServiceCost, String>,
    ) -> Result<(u64, Vec<CausalServicePacketV1>), BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, Clock, DeliveryCost, Window};
        let a = self.window.claim_accepted_daa;
        let hard = a.checked_add(DISPUTE_WINDOW_DAA).ok_or(Clock)?;
        let first = a.checked_add(NOTICE_WINDOW_DAA + 1).ok_or(Clock)?;
        let before = hard.min(self.window.collect_until_daa).checked_sub(self.recovery_daa).ok_or(Window)?;
        if self.demands.is_empty() || self.demands.len() > self.maximum_packets || graph.len() > self.maximum_packets {
            return Err(CaseGraph);
        }

        // Check the complete expansion before allocating or iterating over parts.
        let mut ordinals = BTreeSet::new();
        let mut packets = graph.len();
        for demand in self.demands {
            if !ordinals.insert(demand.ordinal) || !(1..=MAX_BATCH_RANGES_V1).contains(&demand.range_maximums.len()) {
                return Err(CaseGraph);
            }
            let parts = demand.range_maximums.iter().try_fold(0u64, |n, &bytes| {
                if !(1..=super::MATERIAL_PIPELINED_MAX_BYTES).contains(&bytes) {
                    return Err(DeliveryCost);
                }
                n.checked_add(bytes.div_ceil(PART)).ok_or(DeliveryCost)
            })?;
            if parts > MAX_BATCH_PARTS_V1 as u64 {
                return Err(DeliveryCost);
            }
            packets = packets.checked_add(1 + demand.range_maximums.len() * 2 + parts as usize).ok_or(CaseGraph)?;
            if packets > self.maximum_packets {
                return Err(CaseGraph);
            }
        }

        let mut spans = Vec::new();
        for demand in self.demands {
            let open = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(self.claim, self.scope, demand.ordinal)
                .map_err(|_| CaseGraph)?
                .as_bytes();
            let bytes = crate::palw_batch_court_v1::carriage::batch_open_wire_bytes_v1(demand.range_maximums.len())
                .map_err(|_| DeliveryCost)?;
            let open_cost = cost(bytes).map_err(|_| DeliveryCost)?;
            if open_cost.bytes < bytes {
                return Err(DeliveryCost);
            }
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id: open, release_daa: first, service_before: before, cost: open_cost },
                predecessors: demand.predecessors.to_vec(),
            });
            for (index, &maximum) in demand.range_maximums.iter().enumerate() {
                let position = BatchResponsePositionV1 { demand_ordinal: demand.ordinal, range_index: index as u32 };
                let mut response = position
                    .reservation_graph_v1(self.claim, self.scope, maximum, first..before, &mut cost)
                    .map_err(|_| DeliveryCost)?;
                response[0].predecessors.push(open);
                spans.push((response[0].packet.id, response[response.len() - 1].packet.id));
                graph.append(&mut response);
            }
        }
        let terminal = graph.iter_mut().find(|node| node.packet.id == self.terminal).ok_or(CaseGraph)?;
        terminal.predecessors.extend(spans.iter().map(|&(_, finish)| finish));
        let required = self.window.reserve_latest_notice_checked_v1(book, &graph, self.terminal, self.recovery_daa, |book, _| {
            for (begin, finish) in spans {
                let begin = book.assigned_daa(&begin).ok_or(ServiceRefusal::InvalidState)?;
                let finish = book.assigned_daa(&finish).ok_or(ServiceRefusal::InvalidState)?;
                let expires = storage
                    .draft_expires_v1(begin, hard, self.window.material_until_daa)
                    .map_err(|_| ServiceRefusal::DeadlineExhausted)?;
                if finish >= expires {
                    return Err(ServiceRefusal::DeadlineExhausted);
                }
            }
            Ok(())
        })?;
        Ok((required, graph))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Stage;
    use super::*;

    #[test]
    fn maximum_graph_with_latest_notice_and_recovery_is_atomic_at_10000() {
        let h = Hash64::from_u64_word;
        let first = 3_123;
        let terminal = h(3).as_bytes();
        let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
        let graph = |before| {
            vec![CausalServicePacketV1 {
                packet: ServicePacket { id: terminal, release_daa: first, service_before: before, cost: cost(1) },
                predecessors: vec![],
            }]
        };
        let schedule = MaterialIntakeScheduleV1 {
            sompi_per_byte: 0,
            draft_ttl_daa: 3,
            cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
                max_original_leaves: 1,
                max_total_edges: 1,
                max_total_coordinates: 1,
                max_root_memberships: 1,
            },
        };
        let window = BatchCourtWindowV1 {
            claim_accepted_daa: 2,
            challenge_opened_daa: first - 1,
            collect_until_daa: 10_002,
            material_until_daa: 11_000,
        };
        let demands = [MaximumBatchDemandV1 { ordinal: 5, range_maximums: &[PART + 1], predecessors: &[] }];
        let plan = BatchAdmissionV1 {
            claim: h(1),
            scope: h(2),
            window,
            demands: &demands,
            terminal,
            recovery_daa: 6_874,
            maximum_packets: 6,
        };
        let mut book = ServiceReservations::new(ServiceCost { bytes: 64 * 1024, work: 2, io: 2, controls: 2 }).unwrap();
        let empty = book.clone();
        let short = BatchAdmissionV1 { recovery_daa: plan.recovery_daa + 1, ..plan };
        assert!(short.reserve_v1(&mut book, graph(3_127), schedule, |n| Ok(cost(n))).is_err());
        assert_eq!(book, empty);
        let expired = MaterialIntakeScheduleV1 { draft_ttl_daa: 2, ..schedule };
        assert!(plan.reserve_v1(&mut book, graph(3_128), expired, |n| Ok(cost(n))).is_err());
        assert_eq!(book, empty);
        let capped = BatchAdmissionV1 { maximum_packets: 5, ..plan };
        assert!(matches!(capped.reserve_v1(&mut book, graph(3_128), schedule, |n| Ok(cost(n))), Err(BatchCourtErrorV1::CaseGraph)));
        assert_eq!(book, empty);
        let (required, graph) = plan.reserve_v1(&mut book, graph(3_128), schedule, |n| Ok(cost(n))).unwrap();
        assert_eq!(required, 10_000);
        assert_eq!(graph.len(), 6);
        assert_eq!(book.assigned_daa(&terminal), Some(3_127));
        assert_eq!(book.assigned_daa(&graph[3].packet.id), book.assigned_daa(&graph[4].packet.id));
        let reserved = book.clone();
        let first_finish = BatchResponsePositionV1 { demand_ordinal: 5, range_index: 0 }.packet_id(h(1), h(2), Stage::Finish).unwrap();
        let demands = [
            MaximumBatchDemandV1 { ordinal: 5, range_maximums: &[1], predecessors: &[] },
            MaximumBatchDemandV1 { ordinal: 6, range_maximums: &[1], predecessors: &[first_finish] },
        ];
        let fallback_plan = BatchAdmissionV1 { demands: &demands, recovery_daa: 1, maximum_packets: 9, ..plan };
        let terminal_graph = vec![CausalServicePacketV1 {
            packet: ServicePacket { id: terminal, release_daa: first, service_before: 10_001, cost: cost(1) },
            predecessors: vec![],
        }];
        assert!(fallback_plan.reserve_v1(&mut book, terminal_graph.clone(), schedule, |n| Ok(cost(n))).is_err());
        assert_eq!(book, reserved);
        let mut book = empty;
        fallback_plan.reserve_v1(&mut book, terminal_graph, schedule, |n| Ok(cost(n))).unwrap();
        let fallback_open = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(h(1), h(2), 6).unwrap().as_bytes();
        assert!(book.assigned_daa(&fallback_open).unwrap() > book.assigned_daa(&first_finish).unwrap());
    }
}
