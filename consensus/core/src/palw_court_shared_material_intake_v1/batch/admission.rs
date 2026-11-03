//! Preclaim capacity projection for bounded native Batch material paths.
use super::{BatchResponsePositionV1, MaterialIntakeScheduleV1, PART, Stage};
use super::{constraint_stop::ConstraintStopBatchOperandsV1, native_ranges::NumericalBatchEnvelopeV1};
use super::numerical_verification::NumericalBatchReplayBudgetV1;
use crate::{
    Hash64,
    palw_batch_court_v1::{BatchCourtErrorV1, BatchCourtWindowV1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1},
    palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA},
    palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        reserve_native_requests_v1,
    },
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Supplied by the native profile compiler. A fallback demand has its own
/// ordinal and depends on the prior opportunity, even if that response is absent.
pub(crate) struct MaximumBatchDemandV1<'a> {
    pub ordinal: u64,
    pub range_maximums: &'a [u64],
    pub predecessors: &'a [[u8; 64]],
    /// Native graph node that checks this demand's completed material.
    pub verification: [u8; 64],
}

/// Scope is derived from the prepared original. The caller supplies every
/// legal demand and the other native control, verification and terminal nodes.
pub(crate) struct BatchAdmissionV1<'a> {
    pub claim: Hash64,
    pub scope: Hash64,
    pub window: BatchCourtWindowV1,
    pub demands: &'a [MaximumBatchDemandV1<'a>],
    pub retained_controls: &'a [BatchRetainedControlV1],
    pub terminal: [u8; 64],
    pub recovery_daa: u64,
    pub maximum_packets: usize,
}

/// A native control with signed operands retained after filing, such as Stop.
pub(crate) struct BatchRetainedControlV1 {
    pub packet: [u8; 64],
    pub payload_bytes: u64,
}

/// The work at Finish authenticates the complete response. Its carrier size
/// alone cannot price that work. The profile sees the original range bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BatchDeliveryV1 {
    Open { ordinal: u64, range_count: usize },
    Response { position: BatchResponsePositionV1, stage: Stage, maximum_body_bytes: u64 },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct BatchAdmissionQuoteV1 {
    /// Canonical work and retention terms to bind into the payer's scope.
    /// Assigned DAA values are excluded because legal repair may move them.
    pub relative_graph_id: Hash64,
    pub required_daa: u64,
    pub service: ServiceCost,
    pub retained_payload_bytes: u64,
    pub retained_payload_byte_daa: u64,
    /// Maximum sum of carrier storage charges, independent of escrow funding.
    pub carrier_storage_rent_sompi: u64,
    /// Node-produced bytes are charged separately from the provider's carriers.
    pub retained_numerical_fault_bytes: u64,
    pub retained_numerical_fault_byte_daa: u64,
    pub numerical_fault_storage_rent_sompi: u64,
}

struct BatchRelativeGraphV1<'a> {
    plan: &'a BatchAdmissionV1<'a>,
    nodes: &'a [CausalServicePacketV1],
    ranks: &'a BTreeMap<[u8; 64], u64>,
    storage: MaterialIntakeScheduleV1,
}

impl borsh::BorshSerialize for BatchRelativeGraphV1<'_> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let plan = self.plan;
        let a = plan.window.claim_accepted_daa;
        let relative = |daa: u64| daa.checked_sub(a).ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::InvalidData, "Batch packet predates its original",
        ));
        let rank = |packet| self.ranks.get(packet).copied().ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::InvalidData, "Batch funding edge has no relative node",
        ));
        (plan.claim, rank(&plan.terminal)?, plan.recovery_daa, relative(plan.window.collect_until_daa)?,
            self.storage.sompi_per_byte, self.storage.draft_ttl_daa).serialize(writer)?;
        (plan.demands.len() as u64).serialize(writer)?;
        for demand in plan.demands {
            (demand.ordinal, demand.range_maximums, rank(&demand.verification)?).serialize(writer)?;
        }
        (self.nodes.len() as u64).serialize(writer)?;
        for node in self.nodes {
            let packet = &node.packet;
            let retained = plan.retained_controls.binary_search_by_key(&packet.id, |control| control.packet).ok()
                .map_or(0, |index| plan.retained_controls[index].payload_bytes);
            (relative(packet.release_daa)?, relative(packet.service_before)?, packet.cost, retained).serialize(writer)?;
            (node.predecessors.len() as u64).serialize(writer)?;
            for parent in &node.predecessors {
                rank(parent)?.serialize(writer)?;
            }
        }
        Ok(())
    }
}

/// Costs supplied by the versioned native profile. A reservation covers
/// delivery and these service stages, without recording a verdict or credit.
pub(crate) struct NativeBatchDeliveryCostsV1 {
    pub numerical_verification: NumericalBatchReplayBudgetV1,
    pub terminal: ServiceCost,
    pub recovery_daa: u64,
    pub maximum_packets: usize,
    pub timing: NativeBatchDeliveryTimingV1,
}

/// Widths of the service phases and deliberate wait between phases. Absolute
/// windows are written into the packets so repair cannot consume the waits.
#[derive(Clone, Copy)]
pub(crate) struct NativeBatchDeliveryTimingV1 {
    pub open_daa: u64,
    pub provider_wait_daa: u64,
    pub begin_daa: u64,
    pub parts_daa: u64,
    pub finish_daa: u64,
    pub verification_daa: u64,
    pub stop_daa: u64,
    pub terminal_daa: u64,
}

struct DeliveryWindowsV1 {
    open: Range<u64>,
    begin: Range<u64>,
    parts: Range<u64>,
    finish: Range<u64>,
}

impl DeliveryWindowsV1 {
    fn response(&self, index: usize, last: usize) -> &Range<u64> {
        if index == 0 {
            &self.begin
        } else if index == last {
            &self.finish
        } else {
            &self.parts
        }
    }
}

fn take_service_window_v1(cursor: &mut u64, width: u64, before: u64) -> Result<Range<u64>, BatchCourtErrorV1> {
    let end = cursor.checked_add(width).ok_or(BatchCourtErrorV1::Clock)?;
    if width == 0 || end > before {
        return Err(BatchCourtErrorV1::Window);
    }
    let range = *cursor..end;
    *cursor = end;
    Ok(range)
}

pub(crate) fn batch_numerical_service_ids_v1(claim: Hash64, scope: Hash64) -> Result<([u8; 64], [u8; 64]), BatchCourtErrorV1> {
    let id = |stage: u8| {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-original-service/v1", &(claim, scope, stage))
            .map(|id| id.as_bytes())
            .map_err(|_| BatchCourtErrorV1::CaseGraph)
    };
    Ok((id(0)?, id(1)?))
}

/// Prepare the numerical delivery and optional constrained Stop in a private
/// original service row. Node verification follows every numerical Finish;
/// the common terminal follows verification and any Stop filing. Role,
/// payer, and complete adjudication policy remain the coordinator's checks.
pub(crate) fn reserve_native_batch_delivery_v1(
    candidate: crate::palw_court_service_release_v1::CourtServiceRowV3,
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    envelope: &NumericalBatchEnvelopeV1,
    stop: Option<(&ConstraintStopBatchOperandsV1, ServiceCost, ServiceCost)>,
    costs: NativeBatchDeliveryCostsV1,
    storage: MaterialIntakeScheduleV1,
    cost: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
) -> Result<(BatchAdmissionQuoteV1, crate::palw_court_service_release_v1::CourtServiceRowV3), BatchCourtErrorV1> {
    use BatchCourtErrorV1::{CaseGraph, Clock, DeliveryCost, LocalResource, Window};
    let original = frame.plan_v1();
    let claim = original.original_claim;
    let scope = envelope.scope_v1();
    let binding = candidate.state.claims.get(&claim).ok_or(CaseGraph)?;
    if envelope.claim != claim
        || envelope.profile != crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(candidate.state.profile_id)
        || (binding.allocation_id, binding.source_id, binding.original_accepted_daa)
            != (original.allocation_id, original.source_id, original.terms.accepted_daa)
        || stop.is_some() != frame.original_v1().job.is_constraint()
        || stop.is_some_and(|(operands, _, _)| !operands.matches_original_v1(claim, scope))
    {
        return Err(CaseGraph);
    }
    let valid_cost = |cost: ServiceCost| cost.work > 0 && cost.controls == 1;
    let numerical_verification = costs.numerical_verification.service_cost_v1().ok_or(DeliveryCost)?;
    if !costs.numerical_verification.covers_terminal_recheck_v1(costs.terminal)
        || stop.is_some_and(|(_, verify, file)| !valid_cost(verify) || !valid_cost(file))
    {
        return Err(DeliveryCost);
    }
    let a = binding.original_accepted_daa;
    let hard = a.checked_add(DISPUTE_WINDOW_DAA).ok_or(Clock)?;
    if hard != binding.hard_deadline_daa {
        return Err(Window);
    }
    let notice = a.checked_add(NOTICE_WINDOW_DAA).ok_or(Clock)?;
    let first = notice.checked_add(1).ok_or(Clock)?;
    let before = hard.checked_sub(costs.recovery_daa).ok_or(Window)?;
    if costs.recovery_daa == 0 || first >= before {
        return Err(Window);
    }
    let (verification, terminal) = batch_numerical_service_ids_v1(claim, scope)?;
    let timing = costs.timing;
    let mut cursor = first;
    let open = take_service_window_v1(&mut cursor, timing.open_daa, before)?;
    if timing.provider_wait_daa == 0 {
        return Err(Window);
    }
    cursor = cursor.checked_add(timing.provider_wait_daa).ok_or(Clock)?;
    let windows = DeliveryWindowsV1 {
        open,
        begin: take_service_window_v1(&mut cursor, timing.begin_daa, before)?,
        parts: take_service_window_v1(&mut cursor, timing.parts_daa, before)?,
        finish: take_service_window_v1(&mut cursor, timing.finish_daa, before)?,
    };
    let expires = storage.draft_expires_v1(windows.begin.start, hard, original.terms.material_until_daa)
        .map_err(|_| Window)?;
    if windows.finish.end > expires {
        return Err(Window);
    }
    let verification_window = take_service_window_v1(&mut cursor, timing.verification_daa, before)?;
    let retained_numerical_fault_bytes = costs.numerical_verification.max_fault_bytes;
    let retention_daa = original.terms.material_until_daa.checked_sub(verification_window.start).ok_or(Window)?;
    if retention_daa == 0 {
        return Err(Window);
    }
    let retained_numerical_fault_byte_daa = retained_numerical_fault_bytes.checked_mul(retention_daa).ok_or(DeliveryCost)?;
    let numerical_fault_storage_rent_sompi = storage.rent_for_bytes_v1(retained_numerical_fault_bytes).map_err(|_| DeliveryCost)?;
    let stop_window = stop.map(|_| take_service_window_v1(&mut cursor, timing.stop_daa, before)).transpose()?;
    let terminal_window = take_service_window_v1(&mut cursor, timing.terminal_daa, before)?;
    let packet = |id, cost, window: &Range<u64>| ServicePacket { id, release_daa: window.start, service_before: window.end, cost };
    let demand_count = envelope.demand_ranges_v1().count();
    let demand_capacity = demand_count.checked_add(usize::from(stop.is_some())).ok_or(CaseGraph)?;
    if demand_count == 0 || demand_capacity > costs.maximum_packets {
        return Err(CaseGraph);
    }
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<MaximumBatchDemandV1>(demand_capacity as u64).map_err(LocalResource)?;
    q.vec_v1::<CausalServicePacketV1>(if stop.is_some() { 4 } else { 2 }).map_err(LocalResource)?;
    q.vec_v1::<[u8; 64]>(2).map_err(LocalResource)?;
    let _workspace = reserve_native_requests_v1(q.bytes_v1()).map_err(LocalResource)?;
    let mut demands = Vec::with_capacity(demand_capacity);
    demands.extend(envelope.demand_ranges_v1().enumerate().map(|(ordinal, range_maximums)| {
        MaximumBatchDemandV1 { ordinal: ordinal as u64, range_maximums, predecessors: &[], verification }
    }));
    let mut graph = Vec::with_capacity(if stop.is_some() { 4 } else { 2 });
    graph.push(CausalServicePacketV1 {
        packet: packet(verification, numerical_verification, &verification_window),
        predecessors: vec![],
    });
    graph.push(CausalServicePacketV1 {
        packet: packet(terminal, costs.terminal, &terminal_window),
        predecessors: Vec::with_capacity(usize::from(stop.is_some())),
    });
    let retained = if let Some(((operands, verify, file), stop_window)) = stop.zip(stop_window) {
        let (demand, mut nodes, retained) = operands.graph_v1(first..before, verify, file).map_err(|_| DeliveryCost)?;
        nodes[0].packet = packet(nodes[0].packet.id, verify, &verification_window);
        nodes[1].packet = packet(nodes[1].packet.id, file, &stop_window);
        demands.push(demand);
        graph[1].predecessors.push(retained.packet);
        graph.extend(nodes);
        Some(retained)
    } else {
        None
    };
    let plan = BatchAdmissionV1 {
        claim,
        scope,
        window: BatchCourtWindowV1 {
            claim_accepted_daa: a,
            challenge_opened_daa: notice,
            collect_until_daa: hard,
            material_until_daa: original.terms.material_until_daa,
        },
        demands: &demands,
        retained_controls: retained.as_slice(),
        terminal,
        recovery_daa: costs.recovery_daa,
        maximum_packets: costs.maximum_packets,
    };
    let (mut quote, candidate) = plan.reserve_for_original_with_windows_v1(candidate, graph, storage, cost, Some(&windows))?;
    // The earliest assigned slot may move during repair. Report the bound
    // for the latest terminal permitted by its immutable packet interval.
    quote.required_daa = terminal_window.end - a + costs.recovery_daa;
    quote.retained_numerical_fault_bytes = retained_numerical_fault_bytes;
    quote.retained_numerical_fault_byte_daa = retained_numerical_fault_byte_daa;
    quote.numerical_fault_storage_rent_sompi = numerical_fault_storage_rent_sompi;
    Ok((quote, candidate))
}

impl BatchAdmissionV1<'_> {
    /// Extend a privately prepared original's service row before its single
    /// journalled write. The caller supplies the current native admission
    /// candidate, never an already published row to change in place.
    pub(crate) fn reserve_for_original_v1(
        &self,
        candidate: crate::palw_court_service_release_v1::CourtServiceRowV3,
        graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        cost: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(BatchAdmissionQuoteV1, crate::palw_court_service_release_v1::CourtServiceRowV3), BatchCourtErrorV1> {
        self.reserve_for_original_with_windows_v1(candidate, graph, storage, cost, None)
    }

    fn reserve_for_original_with_windows_v1(
        &self,
        mut candidate: crate::palw_court_service_release_v1::CourtServiceRowV3,
        graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        cost: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        windows: Option<&DeliveryWindowsV1>,
    ) -> Result<(BatchAdmissionQuoteV1, crate::palw_court_service_release_v1::CourtServiceRowV3), BatchCourtErrorV1> {
        let binding = candidate.state.claims.get_mut(&self.claim).ok_or(BatchCourtErrorV1::CaseGraph)?;
        if binding.original_accepted_daa != self.window.claim_accepted_daa
            || binding.original_accepted_daa.checked_add(DISPUTE_WINDOW_DAA) != Some(binding.hard_deadline_daa)
        {
            return Err(BatchCourtErrorV1::Window);
        }
        let (quote, graph) = self.reserve_with_v1(&mut candidate.state.book, graph, storage, cost, reserve_native_requests_v1, windows)?;
        let (graph, _graph_lease) = graph.into_parts_v1();
        for node in graph {
            binding.packet_ids.insert(node.packet.id);
            if !node.predecessors.is_empty() {
                let mut predecessors = node.predecessors;
                predecessors.sort_unstable();
                candidate.causal.insert(node.packet.id, predecessors);
            }
        }
        candidate.check().map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        Ok((quote, candidate))
    }

    /// Add exact worst-case BCT1 byte sizes and causal edges to the native
    /// graph. Returned packets must remain with the original's rooted binding.
    /// No notice, request root, or responder needs to exist when this runs.
    pub(crate) fn reserve_v1(
        &self,
        book: &mut ServiceReservations,
        graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        cost: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(BatchAdmissionQuoteV1, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        self.reserve_with_v1(book, graph, storage, cost, reserve_native_requests_v1, None)
    }

    fn reserve_with_v1(
        &self,
        book: &mut ServiceReservations,
        mut graph: Vec<CausalServicePacketV1>,
        storage: MaterialIntakeScheduleV1,
        mut cost: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
        windows: Option<&DeliveryWindowsV1>,
    ) -> Result<(BatchAdmissionQuoteV1, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, Clock, DeliveryCost, Window};
        let a = self.window.claim_accepted_daa;
        let hard = a.checked_add(DISPUTE_WINDOW_DAA).ok_or(Clock)?;
        let first = a.checked_add(NOTICE_WINDOW_DAA + 1).ok_or(Clock)?;
        let before = hard.min(self.window.collect_until_daa).checked_sub(self.recovery_daa).ok_or(Window)?;
        if self.demands.is_empty()
            || self.demands.len() > self.maximum_packets
            || graph.len() > self.maximum_packets
            || self.retained_controls.len() > graph.len()
        {
            return Err(CaseGraph);
        }

        // Count the complete expansion before allocating graph/index scratch.
        let mut packets = graph.len();
        let mut ranges = 0usize;
        let mut retained_payload_bytes = 0u64;
        for demand in self.demands {
            if !(1..=MAX_BATCH_RANGES_V1).contains(&demand.range_maximums.len()) || demand.verification == self.terminal {
                return Err(CaseGraph);
            }
            let parts = demand.range_maximums.iter().try_fold(0u64, |n, &bytes| {
                if !(1..=super::MATERIAL_PIPELINED_MAX_BYTES).contains(&bytes) {
                    return Err(DeliveryCost);
                }
                retained_payload_bytes = retained_payload_bytes.checked_add(bytes).ok_or(DeliveryCost)?;
                n.checked_add(bytes.div_ceil(PART)).ok_or(DeliveryCost)
            })?;
            if parts > MAX_BATCH_PARTS_V1 as u64 {
                return Err(DeliveryCost);
            }
            packets = packets.checked_add(1 + demand.range_maximums.len() * 2 + parts as usize).ok_or(CaseGraph)?;
            ranges = ranges.checked_add(demand.range_maximums.len()).ok_or(CaseGraph)?;
            if packets > self.maximum_packets {
                return Err(CaseGraph);
            }
        }

        let bytes = expansion_workspace_v1(&graph, self.demands, packets, ranges).map_err(BatchCourtErrorV1::LocalResource)?;
        let lease = reserve(bytes).map_err(BatchCourtErrorV1::LocalResource)?;
        let native_nodes: BTreeMap<_, _> = graph.iter().enumerate().map(|(index, node)| (node.packet.id, index)).collect();
        let terminal = *native_nodes.get(&self.terminal).ok_or(CaseGraph)?;
        if native_nodes.len() != graph.len() {
            return Err(CaseGraph);
        }
        // The profile supplies each retained control once in canonical packet order.
        if self.retained_controls.windows(2).any(|pair| pair[0].packet >= pair[1].packet) {
            return Err(CaseGraph);
        }
        for control in self.retained_controls {
            let index = *native_nodes.get(&control.packet).ok_or(CaseGraph)?;
            if !(1..=graph[index].packet.cost.bytes).contains(&control.payload_bytes) {
                return Err(DeliveryCost);
            }
            retained_payload_bytes = retained_payload_bytes.checked_add(control.payload_bytes).ok_or(DeliveryCost)?;
        }
        let carrier_storage_rent_sompi = storage.rent_for_bytes_v1(retained_payload_bytes).map_err(|_| DeliveryCost)?;
        let mut ordinals = BTreeSet::new();
        for demand in self.demands {
            if !ordinals.insert(demand.ordinal) || !native_nodes.contains_key(&demand.verification) {
                return Err(CaseGraph);
            }
        }
        graph.reserve_exact(packets - graph.len());
        let mut spans = Vec::with_capacity(ranges);
        let mut verifications = BTreeSet::new();
        for demand in self.demands {
            let open = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(self.claim, self.scope, demand.ordinal)
                .map_err(|_| CaseGraph)?
                .as_bytes();
            let bytes = crate::palw_batch_court_v1::carriage::batch_open_wire_bytes_v1(demand.range_maximums.len())
                .map_err(|_| DeliveryCost)?;
            let open_cost = cost(BatchDeliveryV1::Open { ordinal: demand.ordinal, range_count: demand.range_maximums.len() }, bytes)
                .map_err(|_| DeliveryCost)?;
            if open_cost.bytes < bytes || open_cost.work == 0 || open_cost.controls != 1 {
                return Err(DeliveryCost);
            }
            let open_window = windows.map_or(first..before, |windows| windows.open.clone());
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id: open, release_daa: open_window.start, service_before: open_window.end, cost: open_cost },
                predecessors: demand.predecessors.to_vec(),
            });
            let verification = native_nodes[&demand.verification];
            verifications.insert(demand.verification);
            for (index, &maximum) in demand.range_maximums.iter().enumerate() {
                let position = BatchResponsePositionV1 { demand_ordinal: demand.ordinal, range_index: index as u32 };
                let mut response = position
                    .reservation_graph_v1(self.claim, self.scope, maximum, first..before, |stage, bytes| {
                        cost(BatchDeliveryV1::Response { position, stage, maximum_body_bytes: maximum }, bytes)
                    })
                    .map_err(|_| DeliveryCost)?;
                response[0].predecessors.push(open);
                if let Some(windows) = windows {
                    let last = response.len() - 1;
                    for (index, node) in response.iter_mut().enumerate() {
                        let window = windows.response(index, last);
                        node.packet.release_daa = window.start;
                        node.packet.service_before = window.end;
                    }
                }
                let finish = response[response.len() - 1].packet.id;
                graph[verification].predecessors.push(finish);
                spans.push((response[0].packet.id, finish, maximum));
                graph.append(&mut response);
            }
        }
        let terminal_predecessors = &mut graph[terminal].predecessors;
        for predecessor in terminal_predecessors.iter() {
            verifications.remove(predecessor);
        }
        terminal_predecessors.extend(verifications);
        // Packet IDs contain the actual accepted allocation. Use the
        // native compiler's stage/range expansion order as relative ranks.
        let ranks = graph.iter().enumerate().map(|(index, node)| (node.packet.id, index as u64)).collect::<BTreeMap<_, _>>();
        for node in &mut graph {
            node.predecessors.sort_unstable_by_key(|parent| ranks.get(parent).copied().unwrap_or(u64::MAX));
        }
        let relative_graph_id = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/batch-funding-graph/v1",
            &BatchRelativeGraphV1 { plan: self, nodes: &graph, ranks: &ranks, storage },
        ).map_err(|_| CaseGraph)?;
        drop(ranks);
        let service = graph.iter().try_fold(ServiceCost::default(), |sum, node| {
            let cost = node.packet.cost;
            Ok::<_, BatchCourtErrorV1>(ServiceCost {
                bytes: sum.bytes.checked_add(cost.bytes).ok_or(DeliveryCost)?,
                work: sum.work.checked_add(cost.work).ok_or(DeliveryCost)?,
                io: sum.io.checked_add(cost.io).ok_or(DeliveryCost)?,
                controls: sum.controls.checked_add(cost.controls).ok_or(DeliveryCost)?,
            })
        })?;
        let mut retained_payload_byte_daa = 0u64;
        let required_daa =
            self.window.reserve_latest_notice_checked_v1(book, &graph, self.terminal, self.recovery_daa, |book, _| {
                let mut account_retained = |start, bytes: u64| {
                    let duration = self.window.material_until_daa.checked_sub(start).ok_or(ServiceRefusal::DeadlineExhausted)?;
                    let holding = bytes.checked_mul(duration).ok_or(ServiceRefusal::InvalidBudget)?;
                    retained_payload_byte_daa = retained_payload_byte_daa.checked_add(holding).ok_or(ServiceRefusal::InvalidBudget)?;
                    Ok(())
                };
                for (begin, finish, bytes) in spans {
                    let begin = book.assigned_daa(&begin).ok_or(ServiceRefusal::InvalidState)?;
                    let finish = book.assigned_daa(&finish).ok_or(ServiceRefusal::InvalidState)?;
                    let expires = storage
                        .draft_expires_v1(begin, hard, self.window.material_until_daa)
                        .map_err(|_| ServiceRefusal::DeadlineExhausted)?;
                    if finish >= expires {
                        return Err(ServiceRefusal::DeadlineExhausted);
                    }
                    account_retained(begin, bytes)?;
                }
                for control in self.retained_controls {
                    let at = book.assigned_daa(&control.packet).ok_or(ServiceRefusal::InvalidState)?;
                    account_retained(at, control.payload_bytes)?;
                }
                Ok::<_, ServiceRefusal>(())
            })?;
        let quote = BatchAdmissionQuoteV1 {
            relative_graph_id,
            required_daa,
            service,
            retained_payload_bytes,
            retained_payload_byte_daa,
            carrier_storage_rent_sompi,
            retained_numerical_fault_bytes: 0,
            retained_numerical_fault_byte_daa: 0,
            numerical_fault_storage_rent_sompi: 0,
        };
        Ok((quote, NativeRetainedV1::from_owner_reserved_v1(graph, lease)))
    }
}

fn expansion_workspace_v1(
    graph: &[CausalServicePacketV1],
    demands: &[MaximumBatchDemandV1<'_>],
    packets: usize,
    ranges: usize,
) -> Result<u64, NativeCoverageV1> {
    type Id = [u8; 64];
    let overflow = || NativeCoverageV1::overflow::<CausalServicePacketV1>("Batch admission graph expansion");
    let original_edges =
        graph.iter().try_fold(0u64, |n, node| n.checked_add(node.predecessors.capacity() as u64)).ok_or_else(overflow)?;
    let demand_edges =
        demands.iter().try_fold(0u64, |n, demand| n.checked_add(demand.predecessors.len() as u64)).ok_or_else(overflow)?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<CausalServicePacketV1>(packets as u64)?; // original, expanded and one response during append
    q.add_v1((packets as u64).checked_mul(btree_node_envelope_bytes_v1::<Id, u64>() as u64).ok_or_else(overflow)?)?;
    q.exact_v1::<(Id, Id, u64)>(ranges as u64)?;
    q.add_v1((graph.len() as u64).checked_mul(btree_node_envelope_bytes_v1::<Id, usize>() as u64).ok_or_else(overflow)?)?;
    let per_demand = btree_node_envelope_bytes_v1::<u64, ()>() + btree_node_envelope_bytes_v1::<Id, ()>();
    q.add_v1((demands.len() as u64).checked_mul(per_demand as u64).ok_or_else(overflow)?)?;
    // A response part has one parent, each Finish has all its parts as
    // parents, and verification/terminal edges grow from existing vectors.
    let edges = (packets as u64)
        .checked_mul(10)
        .and_then(|n| n.checked_add(original_edges.checked_mul(2)?))
        .and_then(|n| n.checked_add(demand_edges))
        .ok_or_else(overflow)?;
    q.exact_v1::<Id>(edges)?;
    Ok(q.bytes_v1())
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
        let verification = h(4).as_bytes();
        let fallback_verification = h(5).as_bytes();
        let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
        let node = |id, before| CausalServicePacketV1 {
            packet: ServicePacket { id, release_daa: first, service_before: before, cost: cost(1) },
            predecessors: vec![],
        };
        let graph = |before| vec![node(terminal, before), node(verification, before)];
        let schedule = MaterialIntakeScheduleV1 {
            sompi_per_byte: 2,
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
        let demands = [MaximumBatchDemandV1 { ordinal: 5, range_maximums: &[PART + 1], predecessors: &[], verification }];
        let plan = BatchAdmissionV1 {
            claim: h(1),
            scope: h(2),
            window,
            demands: &demands,
            retained_controls: &[],
            terminal,
            recovery_daa: 6_873,
            maximum_packets: 7,
        };
        let mut book = ServiceReservations::new(ServiceCost { bytes: 64 * 1024, work: 2, io: 2, controls: 2 }).unwrap();
        let empty = book.clone();
        let denied = plan.reserve_with_v1(
            &mut book,
            graph(3_129),
            schedule,
            |_, _| panic!("cost before capacity"),
            |bytes| {
                assert!(bytes > 0);
                Err(NativeCoverageV1::unsupported::<CausalServicePacketV1>("test", "quota exhausted"))
            },
            None,
        );
        assert!(matches!(denied, Err(BatchCourtErrorV1::LocalResource(_))));
        assert_eq!(book, empty);
        let unbounded = BatchAdmissionV1 { window: BatchCourtWindowV1 { material_until_daa: u64::MAX, ..window }, ..plan };
        assert!(matches!(
            unbounded.reserve_v1(&mut book, graph(3_129), schedule, |_, bytes| Ok(cost(bytes))),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(book, empty);
        let short = BatchAdmissionV1 { recovery_daa: plan.recovery_daa + 1, ..plan };
        assert!(short.reserve_v1(&mut book, graph(3_128), schedule, |_, bytes| Ok(cost(bytes))).is_err());
        assert_eq!(book, empty);
        let expired = MaterialIntakeScheduleV1 { draft_ttl_daa: 2, ..schedule };
        assert!(plan.reserve_v1(&mut book, graph(3_129), expired, |_, bytes| Ok(cost(bytes))).is_err());
        assert_eq!(book, empty);
        let unaffordable = MaterialIntakeScheduleV1 { sompi_per_byte: u64::MAX, ..schedule };
        assert!(matches!(
            plan.reserve_v1(&mut book, graph(3_129), unaffordable, |_, bytes| Ok(cost(bytes))),
            Err(BatchCourtErrorV1::DeliveryCost)
        ));
        assert_eq!(book, empty);
        let capped = BatchAdmissionV1 { maximum_packets: 6, ..plan };
        assert!(matches!(capped.reserve_v1(&mut book, graph(3_129), schedule, |_, bytes| Ok(cost(bytes))), Err(BatchCourtErrorV1::CaseGraph)));
        assert_eq!(book, empty);
        for invalid in [terminal, fallback_verification] {
            let demands = [MaximumBatchDemandV1 { verification: invalid, ..demands[0] }];
            let invalid_plan = BatchAdmissionV1 { demands: &demands, ..plan };
            assert!(matches!(
                invalid_plan.reserve_v1(&mut book, graph(3_129), schedule, |_, bytes| Ok(cost(bytes))),
                Err(BatchCourtErrorV1::CaseGraph)
            ));
            assert_eq!(book, empty);
        }
        let bad_open = |delivery, bytes| match delivery {
            BatchDeliveryV1::Open { .. } => Ok(ServiceCost { work: 0, ..cost(bytes) }),
            _ => Ok(cost(bytes)),
        };
        assert!(matches!(
            plan.reserve_v1(&mut book, graph(3_129), schedule, bad_open),
            Err(BatchCourtErrorV1::DeliveryCost)
        ));
        assert_eq!(book, empty);
        let project = |finish_work| {
            move |delivery, bytes| match delivery {
                BatchDeliveryV1::Response { position, stage: Stage::Finish, maximum_body_bytes } => {
                    assert_eq!((position.demand_ordinal, position.range_index), (5, 0));
                    assert_eq!(maximum_body_bytes, PART + 1);
                    assert!(bytes < maximum_body_bytes);
                    Ok(ServiceCost { work: finish_work, io: 2, ..cost(bytes) })
                }
                _ => Ok(cost(bytes)),
            }
        };
        assert!(plan.reserve_v1(&mut book, graph(3_129), schedule, project(3)).is_err());
        assert_eq!(book, empty);
        let (quote, graph) = plan.reserve_v1(&mut book, graph(3_129), schedule, project(2)).unwrap();
        assert_eq!(quote.required_daa, 10_000);
        let graph_id = |plan: &BatchAdmissionV1<'_>, graph: &[CausalServicePacketV1]| {
            let ranks = graph.iter().enumerate().map(|(index, node)| (node.packet.id, index as u64)).collect();
            crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/batch-funding-graph/v1",
                &BatchRelativeGraphV1 { plan, nodes: graph, ranks: &ranks, storage: schedule },
            ).unwrap()
        };
        assert_eq!(quote.relative_graph_id, graph_id(&plan, &graph));
        let mut shifted = graph.to_vec();
        let rename = |mut packet: [u8; 64]| { packet[0] ^= 1; packet };
        for node in &mut shifted {
            node.packet.release_daa += 100;
            node.packet.service_before += 100;
            node.packet.id = rename(node.packet.id);
            for parent in &mut node.predecessors {
                *parent = rename(*parent);
            }
        }
        let shifted_demands = [MaximumBatchDemandV1 { verification: rename(verification), ..demands[0] }];
        let later = BatchAdmissionV1 {
            scope: h(99),
            terminal: rename(terminal),
            demands: &shifted_demands,
            window: BatchCourtWindowV1 {
                claim_accepted_daa: window.claim_accepted_daa + 100,
                challenge_opened_daa: window.challenge_opened_daa + 100,
                collect_until_daa: window.collect_until_daa + 100,
                material_until_daa: window.material_until_daa + 100,
            },
            ..plan
        };
        assert_eq!(quote.relative_graph_id, graph_id(&later, &shifted));
        shifted[0].packet.cost.io += 1;
        assert_ne!(quote.relative_graph_id, graph_id(&later, &shifted));
        assert_eq!(quote.service.controls, 7);
        assert_eq!((quote.service.work, quote.service.io), (8, 8));
        assert_eq!(quote.service.bytes, graph.iter().map(|node| node.packet.cost.bytes).sum::<u64>());
        assert_eq!(quote.retained_payload_bytes, PART + 1);
        assert_eq!(quote.carrier_storage_rent_sompi, (PART + 1) * 2);
        let begin = book.assigned_daa(&graph[3].packet.id).unwrap();
        assert_eq!(quote.retained_payload_byte_daa, (PART + 1) * (window.material_until_daa - begin));
        assert_eq!(graph.len(), 7);
        assert_eq!(book.assigned_daa(&terminal), Some(3_128));
        assert_eq!(book.assigned_daa(&verification), Some(3_127));
        assert_eq!(book.assigned_daa(&graph[4].packet.id), book.assigned_daa(&graph[5].packet.id));
        let reserved = book.clone();
        let first_finish = BatchResponsePositionV1 { demand_ordinal: 5, range_index: 0 }.packet_id(h(1), h(2), Stage::Finish).unwrap();
        let demands = [
            MaximumBatchDemandV1 { ordinal: 5, range_maximums: &[1], predecessors: &[], verification },
            MaximumBatchDemandV1 {
                ordinal: 6,
                range_maximums: &[1],
                predecessors: &[verification],
                verification: fallback_verification,
            },
        ];
        let fallback_plan = BatchAdmissionV1 { demands: &demands, recovery_daa: 1, maximum_packets: 11, ..plan };
        let terminal_graph = vec![node(terminal, 10_001), node(verification, 10_001), node(fallback_verification, 10_001)];
        assert!(fallback_plan.reserve_v1(&mut book, terminal_graph.clone(), schedule, |_, bytes| Ok(cost(bytes))).is_err());
        assert_eq!(book, reserved);
        let mut book = empty;
        fallback_plan.reserve_v1(&mut book, terminal_graph, schedule, |_, bytes| Ok(cost(bytes))).unwrap();
        let fallback_open = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(h(1), h(2), 6).unwrap().as_bytes();
        assert!(book.assigned_daa(&verification).unwrap() > book.assigned_daa(&first_finish).unwrap());
        assert!(book.assigned_daa(&fallback_open).unwrap() > book.assigned_daa(&verification).unwrap());
    }
}
