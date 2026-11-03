//! Service projection of the signed pipelined material codec. The coordinator
//! binds an accepted demand, native target bound and literal responder first.
use super::{
    MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1 as Action, MaterialDraftV1, MaterialIntakeScheduleV1,
    MaterialTargetV1, SignedMaterialIntakeV1, material_batch_draft_id_v1,
};
use crate::Hash64;
use crate::palw_batch_court_v1::carriage::BATCH_RESPONSE_WIRE_OVERHEAD_V1;
use crate::palw_batch_court_v1::{BatchCourtErrorV1, BatchCourtWindowV1};
use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
use borsh::BorshSerialize;
use std::collections::BTreeMap;

const PART: u64 = PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
enum Stage {
    Begin,
    Part(u64),
    Finish,
}

/// An exact signed delivery branch of a previously accepted demand. Original
/// admission must separately reserve capacity for the maximum native branch.
pub(crate) struct MaterialBatchDeliveryV1 {
    id: Hash64,
    request_root: Hash64,
    case_window: BatchCourtWindowV1,
    release_daa: u64,
    service_before: u64,
    claim: Hash64,
    scope: Hash64,
    target: MaterialTargetV1,
    signer: PalwBondKeyV2,
    total: u64,
    body_hash: Hash64,
    draft: Hash64,
}

impl MaterialBatchDeliveryV1 {
    /// Resolve the exact target from the already accepted demand before
    /// assigning a delivery. The caller loads the notice and demand from the
    /// same branch and derives the bound from the original source.
    pub(crate) fn from_accepted_demand_v1(
        context: &crate::palw_court_shared_tir_state_v1::SharedTirContextV1,
        demand: &crate::palw_batch_court_v1::BatchDemandV1,
        declaration: &SignedMaterialIntakeV1,
        native_maximum: u64,
    ) -> Result<Self, String> {
        use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
        let request_root = demand.checked_request_root_v1().map_err(|err| format!("Batch demand: {err:?}"))?;
        if declaration.claim != context.claim
            || declaration.scope_id != context.scope_id
            || demand.case_window.claim_accepted_daa != context.original_accepted_daa
            || demand.case_window.material_until_daa != context.material_until_daa
            || demand.response_deadline_daa > context.hard_daa
            || u64::from(demand.part_bytes) != PART
        {
            return Err("Batch material demand substitutes the original context or part width".into());
        }
        let target = declaration.target.id_v1(context.claim)?;
        let range = demand
            .ranges
            .binary_search_by_key(&target, |range| range.id)
            .ok()
            .map(|index| &demand.ranges[index])
            .ok_or("Batch material target is outside the accepted demand")?;
        let release_daa = demand.opened_daa.checked_add(1).ok_or("Batch demand opening overflows")?;
        let delivery =
            Self::new(request_root, declaration, native_maximum, demand.case_window, release_daa, demand.response_deadline_daa)?;
        if range.authentication != BatchRangeAuthenticationV1::WholeRange || delivery.total != u64::from(range.bytes) {
            return Err("Batch material declaration differs from the accepted range".into());
        }
        Ok(delivery)
    }

    fn new(
        request_root: Hash64,
        declaration: &SignedMaterialIntakeV1,
        native_maximum: u64,
        case_window: BatchCourtWindowV1,
        release_daa: u64,
        service_before: u64,
    ) -> Result<Self, String> {
        let Action::Begin { total_bytes, body_hash } = declaration.action else {
            return Err("Batch delivery requires a declaration".into());
        };
        if declaration.version != MATERIAL_BATCH_VERSION
            || request_root == Hash64::default()
            || release_daa >= service_before
            || declaration.claim == Hash64::default()
            || declaration.scope_id == Hash64::default()
            || total_bytes == 0
            || total_bytes > native_maximum.min(MATERIAL_PIPELINED_MAX_BYTES)
        {
            return Err("Batch declaration outside native demand bounds".into());
        }
        let draft = material_batch_draft_id_v1(
            request_root,
            declaration.claim,
            declaration.target,
            declaration.submitter,
            total_bytes,
            body_hash,
        )?;
        let id = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/material-batch-delivery/v1",
            &(request_root, declaration.scope_id, draft),
        )?;
        Ok(Self {
            id,
            request_root,
            case_window,
            release_daa,
            service_before,
            claim: declaration.claim,
            scope: declaration.scope_id,
            target: declaration.target,
            signer: declaration.submitter,
            total: total_bytes,
            body_hash,
            draft,
        })
    }

    pub(crate) fn request_root_v1(&self) -> Hash64 {
        self.request_root
    }

    fn packet_id(&self, stage: Stage) -> Result<[u8; 64], String> {
        Ok(crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-batch-packet/v1", &(self.id, stage))?
            .as_bytes())
    }

    /// Include the signed Borsh envelope, even for the Begin and Finish
    /// controls. The native profile adds carrier, parser, signature and IO costs.
    pub(crate) fn service_graph_v1(
        &self,
        release_daa: u64,
        service_before: u64,
        mut cost: impl FnMut(u64) -> Result<ServiceCost, String>,
    ) -> Result<Vec<CausalServicePacketV1>, String> {
        if release_daa < self.release_daa || service_before > self.service_before || release_daa >= service_before {
            return Err("Batch delivery service interval is outside its accepted demand".into());
        }
        let parts = self.total.div_ceil(PART);
        let mut graph = Vec::with_capacity(parts as usize + 2);
        let mut append = |stage, action, payload, predecessors| -> Result<(), String> {
            let envelope = SignedMaterialIntakeV1 {
                version: MATERIAL_BATCH_VERSION,
                claim: self.claim,
                scope_id: self.scope,
                target: self.target,
                submitter: self.signer,
                action,
                signature: Vec::new(),
            };
            let minimum = borsh::object_length(&envelope).map_err(|e| e.to_string())? as u64
                + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64
                + payload
                + BATCH_RESPONSE_WIRE_OVERHEAD_V1;
            let cost = cost(minimum)?;
            if cost.bytes < minimum || cost.controls != 1 || cost.work == 0 {
                return Err("Batch delivery service cost omits signed control".into());
            }
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id: self.packet_id(stage)?, release_daa, service_before, cost },
                predecessors,
            });
            Ok(())
        };
        let begin = self.packet_id(Stage::Begin)?;
        append(Stage::Begin, Action::Begin { total_bytes: self.total, body_hash: self.body_hash }, 0, Vec::new())?;
        let mut predecessors = Vec::with_capacity(parts as usize);
        for index in 0..parts {
            let offset = index * PART;
            let stage = Stage::Part(offset);
            append(
                stage,
                Action::Part { draft_id: self.draft, offset, bytes: Vec::new() },
                PART.min(self.total - offset),
                vec![begin],
            )?;
            predecessors.push(self.packet_id(stage)?);
        }
        append(Stage::Finish, Action::Finish { draft_id: self.draft }, 0, predecessors)?;
        Ok(graph)
    }

    fn check_case_dependencies_v1(&self, nodes: &BTreeMap<[u8; 64], &CausalServicePacketV1>) -> Result<(), ServiceRefusal> {
        use ServiceRefusal::InvalidState;
        let id = |stage| self.packet_id(stage).map_err(|_| InvalidState);
        let begin = id(Stage::Begin)?;
        if !nodes.contains_key(&begin) {
            return Err(InvalidState);
        }
        let finish = nodes.get(&id(Stage::Finish)?).ok_or(InvalidState)?;
        for offset in (0..self.total).step_by(PART as usize) {
            let part_id = id(Stage::Part(offset))?;
            let part = nodes.get(&part_id).ok_or(InvalidState)?;
            if !part.predecessors.contains(&begin) || !finish.predecessors.contains(&part_id) {
                return Err(InvalidState);
            }
        }
        Ok(())
    }

    /// Validate the actual booked span against the same expiry as the native
    /// fold. A Finish exactly on the expiry DAA is outside the draft.
    pub(crate) fn check_draft_schedule_v1(
        &self,
        book: &ServiceReservations,
        schedule: MaterialIntakeScheduleV1,
    ) -> Result<(), ServiceRefusal> {
        self.check_draft_assignments_v1(book, schedule, None)
    }

    fn check_draft_assignments_v1(
        &self,
        book: &ServiceReservations,
        schedule: MaterialIntakeScheduleV1,
        retained: Option<&MaterialDraftV1>,
    ) -> Result<(), ServiceRefusal> {
        use ServiceRefusal::{DeadlineExhausted, InvalidState};
        let assigned = |stage| {
            let id = self.packet_id(stage).map_err(|_| InvalidState)?;
            let daa = book.assigned_daa(&id).ok_or(InvalidState)?;
            if !(self.release_daa..self.service_before).contains(&daa) {
                return Err(DeadlineExhausted);
            }
            Ok(daa)
        };
        let begin = assigned(Stage::Begin)?;
        let finish = assigned(Stage::Finish)?;
        let hard = self
            .case_window
            .claim_accepted_daa
            .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
            .ok_or(DeadlineExhausted)?;
        let expires = match retained {
            Some(draft) if draft.begun_daa == begin && draft.expires_daa <= hard => draft.expires_daa,
            Some(_) => return Err(InvalidState),
            None => schedule.draft_expires_v1(begin, hard, self.case_window.material_until_daa).map_err(|_| DeadlineExhausted)?,
        };
        if finish >= expires {
            return Err(DeadlineExhausted);
        }
        for offset in (0..self.total).step_by(PART as usize) {
            let part = assigned(Stage::Part(offset))?;
            if part <= begin || part >= finish {
                return Err(DeadlineExhausted);
            }
        }
        Ok(())
    }

    fn check_retained_draft_v1(&self, draft: &MaterialDraftV1) -> Result<(), ServiceRefusal> {
        use ServiceRefusal::InvalidState;
        draft.check(&Key::CourtMaterialDraft(self.claim, self.draft)).map_err(|_| InvalidState)?;
        if draft.version != MATERIAL_BATCH_VERSION
            || draft.scope_id != self.scope
            || draft.head != self.request_root
            || draft.target != self.target
            || draft.submitter != self.signer
            || draft.total_bytes != self.total
            || draft.body_hash != self.body_hash
            || draft.material_until_daa != self.case_window.material_until_daa
        {
            return Err(InvalidState);
        }
        Ok(())
    }

    /// Match the actual signed action to its case-specific booked slot. The
    /// accepting caller must take the book from the live claim binding, and
    /// still verify the signature, native body and actual predecessor rows.
    pub(crate) fn check_slot_v1(
        &self,
        book: &ServiceReservations,
        daa: u64,
        signed: &SignedMaterialIntakeV1,
    ) -> Result<[u8; 64], String> {
        if (signed.version, signed.claim, signed.scope_id, signed.target, signed.submitter)
            != (MATERIAL_BATCH_VERSION, self.claim, self.scope, self.target, self.signer)
        {
            return Err("Batch material action has a foreign scope or signer".into());
        }
        let stage = match &signed.action {
            Action::Begin { total_bytes, body_hash } if (*total_bytes, *body_hash) == (self.total, self.body_hash) => Stage::Begin,
            Action::Part { draft_id, offset, bytes }
                if *draft_id == self.draft
                    && offset.is_multiple_of(PART)
                    && *offset < self.total
                    && bytes.len() as u64 == PART.min(self.total - *offset) =>
            {
                Stage::Part(*offset)
            }
            Action::Finish { draft_id } if *draft_id == self.draft => Stage::Finish,
            _ => return Err("Batch material action differs from its declaration".into()),
        };
        let id = self.packet_id(stage)?;
        if !(self.release_daa..self.service_before).contains(&daa) || book.assigned_daa(&id) != Some(daa) {
            return Err("Batch material action has no assigned service at this DAA".into());
        }
        let bytes = borsh::object_length(signed).map_err(|e| e.to_string())? as u64 + BATCH_RESPONSE_WIRE_OVERHEAD_V1;
        if book.packet(&id).is_none_or(|packet| packet.cost.bytes < bytes) {
            return Err("Batch material action exceeds its booked signed bytes".into());
        }
        Ok(id)
    }
}

impl BatchCourtWindowV1 {
    /// Book the complete case atomically with the native storage windows.
    /// The coordinator supplies every delivery selected by the case graph.
    pub(crate) fn reserve_latest_notice_material_v1(
        self,
        book: &mut ServiceReservations,
        graph: &[CausalServicePacketV1],
        terminal: [u8; 64],
        recovery_daa: u64,
        deliveries: &[MaterialBatchDeliveryV1],
        schedule: MaterialIntakeScheduleV1,
    ) -> Result<u64, BatchCourtErrorV1> {
        if deliveries.is_empty() || deliveries.iter().any(|delivery| delivery.case_window != self) {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        self.reserve_latest_notice_checked_v1(book, graph, terminal, recovery_daa, |candidate, nodes| {
            for delivery in deliveries {
                delivery.check_case_dependencies_v1(nodes)?;
                delivery.check_draft_schedule_v1(candidate, schedule)?;
            }
            Ok(())
        })
    }
}

/// Recover unresolved Batch deliveries against the same branch's accepted
/// drafts. A completed or otherwise retired delivery is resolved by the
/// coordinator before this repair path.
pub(crate) fn repair_batch_material_v1(
    state: &PalwChainStateV2,
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    graph: &[CausalServicePacketV1],
    deliveries: &[MaterialBatchDeliveryV1],
    schedule: MaterialIntakeScheduleV1,
) -> Result<usize, ServiceRefusal> {
    use ServiceRefusal::InvalidState;
    let claim = deliveries.first().ok_or(InvalidState)?.claim;
    let service = state.court_service_state_v1().ok_or(InvalidState)?;
    let binding = service.claims.get(&claim).ok_or(InvalidState)?;
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(|_| InvalidState)?;
    context.check_binding(service).map_err(|_| InvalidState)?;
    if &service.book != &*book
        || deliveries.iter().any(|delivery| delivery.claim != claim || delivery.scope != context.scope_id)
        || graph.iter().any(|node| !binding.packet_ids.contains(&node.packet.id))
    {
        return Err(InvalidState);
    }
    let retained = deliveries
        .iter()
        .map(|delivery| match state.staged_court_row_v1(&Key::CourtMaterialDraft(delivery.claim, delivery.draft)) {
            Some(Row::CourtMaterialDraft(draft)) => Ok((delivery, Some(draft.as_ref()))),
            Some(_) => Err(ServiceRefusal::InvalidState),
            None => Ok((delivery, None)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    repair_batch_material_with_drafts_v1(window, book, graph, &retained, schedule)
}

fn repair_batch_material_with_drafts_v1(
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    graph: &[CausalServicePacketV1],
    deliveries: &[(&MaterialBatchDeliveryV1, Option<&MaterialDraftV1>)],
    schedule: MaterialIntakeScheduleV1,
) -> Result<usize, ServiceRefusal> {
    use ServiceRefusal::{DeadlineExhausted, InvalidState};
    window.check().map_err(|_| InvalidState)?;
    let hard = window
        .claim_accepted_daa
        .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
        .ok_or(DeadlineExhausted)?
        .min(window.collect_until_daa);
    if deliveries.is_empty() || deliveries.iter().any(|(delivery, _)| delivery.case_window != window) {
        return Err(InvalidState);
    }
    book.check_causal_v1(graph)?;
    let nodes: BTreeMap<_, _> = graph.iter().map(|node| (node.packet.id, node)).collect();
    let mut expires = BTreeMap::new();
    for (delivery, retained) in deliveries {
        delivery.check_case_dependencies_v1(&nodes)?;
        if let Some(draft) = retained {
            delivery.check_retained_draft_v1(draft)?;
            let id = |stage| delivery.packet_id(stage).map_err(|_| InvalidState);
            // Begin already exists on this branch. A repair may not move it.
            expires.insert(id(Stage::Begin)?, draft.begun_daa.checked_add(1).ok_or(DeadlineExhausted)?);
            expires.insert(id(Stage::Finish)?, draft.expires_daa);
            for offset in (0..delivery.total).step_by(PART as usize) {
                expires.insert(id(Stage::Part(offset))?, draft.expires_daa);
            }
        }
        delivery.check_draft_assignments_v1(book, schedule, *retained)?;
    }
    let ids = nodes.keys().copied().collect();
    let edges = graph.iter().map(|node| (node.packet.id, node.predecessors.clone())).collect();
    book.repair_missing_ordered_checked_v1(
        &ids,
        hard,
        &edges,
        |id| expires.get(id).copied(),
        |candidate| {
            candidate.check_causal_v1(graph)?;
            for (delivery, draft) in deliveries {
                delivery.check_draft_assignments_v1(candidate, schedule, *draft)?;
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn test_case() -> BatchCourtWindowV1 {
        BatchCourtWindowV1 { claim_accepted_daa: 2, challenge_opened_daa: 7, collect_until_daa: 20, material_until_daa: 11_000 }
    }

    #[test]
    fn latest_case_refuses_finish_on_draft_expiry_atomically() {
        let h = Hash64::from_u64_word;
        let a = 2;
        let notice = a + crate::palw_court_policy_v1::NOTICE_WINDOW_DAA;
        let first = notice + 1;
        let case = BatchCourtWindowV1 {
            claim_accepted_daa: a,
            challenge_opened_daa: notice,
            collect_until_daa: a + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
            material_until_daa: 11_000,
        };
        let begin = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: h(1),
            scope_id: h(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
            action: Action::Begin { total_bytes: 1, body_hash: h(4) },
            signature: Vec::new(),
        };
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let delivery = MaterialBatchDeliveryV1::new(h(5), &begin, 1, case, first, first + 8).unwrap();
        let mut graph = delivery.service_graph_v1(first, first + 8, cost).unwrap();
        let finish = graph[2].packet.id;
        let terminal = [18; 64];
        graph.push(CausalServicePacketV1 {
            packet: ServicePacket { id: terminal, release_daa: first, service_before: first + 8, cost: cost(1).unwrap() },
            predecessors: vec![finish],
        });
        let mut schedule = super::super::material_intake_schedule_v1().unwrap();
        schedule.draft_ttl_daa = 3;
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 1, io: 1, controls: 1 }).unwrap();
        let initial = book.clone();
        graph[2].packet.release_daa = first + 3;
        let mut plain = initial.clone();
        case.reserve_latest_notice_case_v1(&mut plain, &graph, terminal, 1).unwrap();
        assert_eq!(plain.assigned_daa(&finish), Some(first + 3));
        assert_eq!(
            case.reserve_latest_notice_material_v1(&mut book, &graph, terminal, 1, std::slice::from_ref(&delivery), schedule),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::DeadlineExhausted)),
        );
        assert_eq!(book, initial);
        graph[2].packet.release_daa = first + 2;
        let omitted = graph[1].packet.id;
        let mut wrong_graph = graph.clone();
        wrong_graph[2].predecessors = vec![graph[0].packet.id];
        wrong_graph[3].predecessors.push(omitted);
        assert_eq!(
            case.reserve_latest_notice_material_v1(&mut book, &wrong_graph, terminal, 1, std::slice::from_ref(&delivery), schedule),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidState)),
        );
        assert_eq!(book, initial);
        case.reserve_latest_notice_material_v1(&mut book, &graph, terminal, 1, &[delivery], schedule).unwrap();
        assert_eq!(book.assigned_daa(&finish), Some(first + 2));
        assert_eq!(book.assigned_daa(&terminal), Some(first + 3));
    }

    #[test]
    fn missing_batch_part_repair_keeps_retained_begin_and_expiry() {
        let h = Hash64::from_u64_word;
        let case = BatchCourtWindowV1 {
            claim_accepted_daa: 2,
            challenge_opened_daa: 3_122,
            collect_until_daa: 10_002,
            material_until_daa: 11_000,
        };
        let first = case.challenge_opened_daa + 1;
        let begin = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: h(1),
            scope_id: h(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
            action: Action::Begin { total_bytes: 1, body_hash: h(4) },
            signature: Vec::new(),
        };
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let delivery = MaterialBatchDeliveryV1::new(h(5), &begin, 1, case, first, first + 12).unwrap();
        let mut graph = delivery.service_graph_v1(first, first + 12, cost).unwrap();
        graph[2].packet.release_daa = first + 5;
        let (begin_packet, part_packet, finish_packet) = (graph[0].packet.id, graph[1].packet.id, graph[2].packet.id);
        let terminal = [18; 64];
        graph.push(CausalServicePacketV1 {
            packet: ServicePacket { id: terminal, release_daa: first, service_before: first + 12, cost: cost(1).unwrap() },
            predecessors: vec![finish_packet],
        });
        let mut schedule = super::super::material_intake_schedule_v1().unwrap();
        schedule.draft_ttl_daa = 7;
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 1, io: 1, controls: 1 }).unwrap();
        case.reserve_latest_notice_material_v1(&mut book, &graph, terminal, 1, std::slice::from_ref(&delivery), schedule).unwrap();
        let begun_block = h(20);
        book.observe_block(first, begun_block.as_bytes(), ServiceCost::default()).unwrap();
        let draft = MaterialDraftV1 {
            version: begin.version,
            claim: begin.claim,
            scope_id: begin.scope_id,
            target: begin.target,
            submitter: begin.submitter,
            draft_id: delivery.draft,
            total_bytes: 1,
            body_hash: h(4),
            begun_block,
            begun_daa: first,
            last_daa: first,
            accepted_bytes: 0,
            head: delivery.request_root,
            expires_daa: first + 7,
            material_until_daa: case.material_until_daa,
        };
        let mut repaired: ServiceReservations = borsh::from_slice(&borsh::to_vec(&book).unwrap()).unwrap();
        let retained: MaterialDraftV1 = borsh::from_slice(&borsh::to_vec(&draft).unwrap()).unwrap();
        repaired.observe_block(first + 2, h(21).as_bytes(), ServiceCost::default()).unwrap();
        assert_eq!(
            repair_batch_material_with_drafts_v1(case, &mut repaired, &graph, &[(&delivery, Some(&retained))], schedule),
            Ok(1),
        );
        assert_eq!(repaired.assigned_daa(&begin_packet), Some(first));
        assert_eq!(repaired.assigned_daa(&part_packet), Some(first + 3));
        assert_eq!(repaired.assigned_daa(&finish_packet), Some(first + 5));

        let mut late = book;
        late.observe_block(first + 6, h(22).as_bytes(), ServiceCost::default()).unwrap();
        let old = late.clone();
        schedule.draft_ttl_daa = 50;
        assert_eq!(
            repair_batch_material_with_drafts_v1(case, &mut late, &graph, &[(&delivery, Some(&retained))], schedule),
            Err(ServiceRefusal::DeadlineExhausted),
        );
        assert_eq!(late, old);
        let mut foreign = retained;
        foreign.head = h(23);
        assert_eq!(
            repair_batch_material_with_drafts_v1(case, &mut late, &graph, &[(&delivery, Some(&foreign))], schedule),
            Err(ServiceRefusal::InvalidState),
        );
        assert_eq!(late, old);
    }

    #[test]
    fn accepted_demand_fixes_native_range_and_delivery_window() {
        use crate::palw_batch_court_v1::{BatchCourtWindowV1, BatchDemandV1, BatchRangeAuthenticationV1, BatchRangeV1};
        use crate::palw_court_shared_tir_state_v1::SharedTirContextV1;
        let h = Hash64::from_u64_word;
        let action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: h(1),
            scope_id: h(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
            action: Action::Begin { total_bytes: PART + 1, body_hash: h(4) },
            signature: Vec::new(),
        };
        let context = SharedTirContextV1 {
            version: 1,
            claim: action.claim,
            allocation_id: h(5),
            source_id: h(6),
            rules_id: h(7),
            profile_id: h(8),
            scope_id: action.scope_id,
            queries: 8,
            original_accepted_daa: 2,
            hard_daa: 10_002,
            material_until_daa: 11_000,
        };
        let mut demand = BatchDemandV1 {
            case_id: h(9),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 2,
                challenge_opened_daa: 7,
                collect_until_daa: 20,
                material_until_daa: 11_000,
            },
            opened_daa: 10,
            response_deadline_daa: 14,
            maximum_bytes: PART + 1,
            part_bytes: PART as u32,
            ranges: vec![BatchRangeV1 {
                id: action.target.id_v1(action.claim).unwrap(),
                bytes: (PART + 1) as u32,
                authentication: BatchRangeAuthenticationV1::WholeRange,
            }],
        };
        let delivery = MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).unwrap();
        assert_eq!(delivery.request_root_v1(), demand.checked_request_root_v1().unwrap());
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART).is_err());
        let mut wrong_context = context.clone();
        wrong_context.material_until_daa += 1;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&wrong_context, &demand, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::IndependentParts;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::WholeRange;
        demand.ranges[0].id = h(11);
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, &action, PART + 1).is_err());
    }

    #[test]
    fn batch_signature_and_draft_refuse_cross_request_reuse() {
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
        let network = Hash64::from_u64_word(9);
        let request = Hash64::from_u64_word(5);
        let other_request = Hash64::from_u64_word(6);
        let hash = Hash64::from_u64_word(4);
        let mut action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: Hash64::from_u64_word(1),
            scope_id: Hash64::from_u64_word(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            action: Action::Begin { total_bytes: 1, body_hash: hash },
            signature: Vec::new(),
        };
        let own = MaterialBatchDeliveryV1::new(request, &action, 1, test_case(), 11, 14).unwrap();
        let foreign = MaterialBatchDeliveryV1::new(other_request, &action, 1, test_case(), 11, 14).unwrap();
        assert_ne!(own.draft, foreign.draft);
        use super::super::MaterialDraftV1;
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
        let retained = MaterialDraftV1 {
            version: action.version,
            claim: action.claim,
            scope_id: action.scope_id,
            target: action.target,
            submitter: action.submitter,
            draft_id: own.draft,
            total_bytes: 1,
            body_hash: hash,
            begun_block: Hash64::from_u64_word(10),
            begun_daa: 11,
            last_daa: 11,
            accepted_bytes: 0,
            head: request,
            expires_daa: 20,
            material_until_daa: 25,
        };
        let draft_key = PalwStagedCourtKeyV1::CourtMaterialDraft(action.claim, own.draft);
        let mut restored: MaterialDraftV1 = borsh::from_slice(&borsh::to_vec(&retained).unwrap()).unwrap();
        assert_eq!(restored, retained);
        assert!(restored.check(&draft_key).is_ok());
        restored.head = other_request;
        assert!(restored.check(&draft_key).is_err());

        for stage in [
            action.action.clone(),
            Action::Part { draft_id: own.draft, offset: 0, bytes: vec![7] },
            Action::Finish { draft_id: own.draft },
        ] {
            action.action = stage;
            action.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                action.batch_signing_message_v1(network, request).unwrap().as_byte_slice(),
                super::super::MATERIAL_BATCH_CONTEXT_V1,
                [82; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            assert!(action.verify_batch_v1(network, request, key.verification_key.as_ref()).is_ok());
            assert!(action.verify_batch_v1(network, other_request, key.verification_key.as_ref()).is_err());
            assert!(action.verify_v1(network, key.verification_key.as_ref()).is_err());
        }
    }

    #[test]
    fn funded_batch_delivery_binds_signed_fragments_and_fits_parallel_slots() {
        let total = PART + 1;
        let mut action = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: Hash64::from_u64_word(1),
            scope_id: Hash64::from_u64_word(2),
            target: MaterialTargetV1::Unit { ordinal: 7 },
            submitter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            action: Action::Begin { total_bytes: total, body_hash: Hash64::from_u64_word(4) },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let request = Hash64::from_u64_word(5);
        let delivery = MaterialBatchDeliveryV1::new(request, &action, total, test_case(), 11, 14).unwrap();
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let graph = delivery.service_graph_v1(11, 14, cost).unwrap();
        assert_eq!(graph.len(), 4);
        assert!(delivery.service_graph_v1(10, 14, cost).is_err());
        assert!(delivery.service_graph_v1(11, 15, cost).is_err());
        assert!(delivery.service_graph_v1(11, 14, |bytes| cost(bytes - 1)).is_err());
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 2, io: 2, controls: 2 }).unwrap();
        book.reserve_causal_v1(&graph, 14).unwrap();
        assert_eq!(delivery.check_slot_v1(&book, 11, &action).unwrap(), graph[0].packet.id);
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        let other = MaterialBatchDeliveryV1::new(Hash64::from_u64_word(6), &action, total, test_case(), 11, 14).unwrap();
        assert!(other.check_slot_v1(&book, 11, &action).is_err());
        for (index, (offset, len)) in [(0, PART), (PART, 1)].into_iter().enumerate().rev() {
            action.action = Action::Part { draft_id: delivery.draft, offset, bytes: vec![0; len as usize] };
            assert_eq!(delivery.check_slot_v1(&book, 12, &action).unwrap(), graph[index + 1].packet.id);
            assert_eq!(
                borsh::object_length(&action).unwrap() as u64 + BATCH_RESPONSE_WIRE_OVERHEAD_V1,
                graph[index + 1].packet.cost.bytes
            );
        }
        action.action = Action::Part { draft_id: Hash64::from_u64_word(8), offset: 0, bytes: vec![0; PART as usize] };
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        action.action = Action::Finish { draft_id: delivery.draft };
        assert!(delivery.check_slot_v1(&book, 12, &action).is_err());
        assert_eq!(delivery.check_slot_v1(&book, 13, &action).unwrap(), graph[3].packet.id);
        action.submitter = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 1));
        assert!(delivery.check_slot_v1(&book, 13, &action).is_err());
    }
}
