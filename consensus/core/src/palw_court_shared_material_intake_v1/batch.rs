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

pub(crate) mod admission;
pub(crate) mod constraint_stop;
pub(crate) mod native_ranges;
pub(crate) mod numerical_verification;
pub(crate) mod plain_output;
pub(crate) mod selection;

const PART: u64 = PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;

fn check_external_batch_filer_v1(
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    filer: PalwBondKeyV2,
) -> Result<(), String> {
    if filer == frame.custody_v1().pledge_v1().producer
        || frame.original_custody_v1().credit_v1(&filer).is_some()
        || frame.original_custody_v1().grant_v1(&filer).is_some()
    {
        return Err("Batch filer is liable on this claim".into());
    }
    Ok(())
}

/// Per-submitter draft ceiling for a Batch-only original. Every reserved
/// Part includes its full payload in the booked byte cost. Counting the
/// other packets as well is conservative; range, fee and exact-slot checks
/// still govern each draft independently.
pub(crate) fn booked_material_ledger_ceiling_v1(
    state: &PalwChainStateV2,
    context: &crate::palw_court_shared_tir_state_v1::SharedTirContextV1,
) -> Result<u64, String> {
    let service = state.court_service_state_v1().ok_or("Batch material has no rooted service book")?;
    context.check_binding(service)?;
    let binding = service.claims.get(&context.claim).ok_or("Batch material has no original reservation")?;
    sum_booked_wire_bytes_v1(&service.book, &binding.packet_ids)
}

fn sum_booked_wire_bytes_v1(book: &ServiceReservations, owned: &std::collections::BTreeSet<[u8; 64]>) -> Result<u64, String> {
    owned.iter().try_fold(0u64, |total, id| {
        let packet = book.packet(id).ok_or("Batch ledger reservation has no booked packet")?;
        total.checked_add(packet.cost.bytes).ok_or_else(|| "Batch ledger reservation bytes overflow".into())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub(crate) enum Stage {
    Begin,
    Part(u64),
    Finish,
}

/// A response position that can be reserved before the notice and demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize)]
pub(crate) struct BatchResponsePositionV1 {
    pub demand_ordinal: u64,
    pub range_index: u32,
}

impl BatchResponsePositionV1 {
    fn packet_id(self, claim: Hash64, scope: Hash64, stage: Stage) -> Result<[u8; 64], String> {
        Ok(crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-batch-slot/v1", &(claim, scope, self, stage))?
            .as_bytes())
    }

    /// Reserve up to the native maximum for this position. The cost callback
    /// adds profile-specific work and IO. It is independent of the later
    /// request root, responder and authenticated body.
    pub(crate) fn reservation_graph_v1(
        self,
        claim: Hash64,
        scope: Hash64,
        maximum_bytes: u64,
        service: std::ops::Range<u64>,
        mut cost: impl FnMut(Stage, u64) -> Result<ServiceCost, String>,
    ) -> Result<Vec<CausalServicePacketV1>, String> {
        if self.range_index as usize >= crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1
            || claim == Hash64::default()
            || scope == Hash64::default()
            || !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&maximum_bytes)
            || service.is_empty()
        {
            return Err("Batch response reservation outside native bounds".into());
        }
        let parts = maximum_bytes.div_ceil(PART);
        let mut graph = Vec::with_capacity(parts as usize + 2);
        let mut append = |stage, action, payload, predecessors| -> Result<(), String> {
            // Unit has the widest MaterialTargetV1 encoding. All the other
            // variable fields here have exact bounded sizes.
            let envelope = SignedMaterialIntakeV1 {
                version: MATERIAL_BATCH_VERSION,
                claim,
                scope_id: scope,
                target: MaterialTargetV1::Unit { ordinal: 0 },
                submitter: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::default(), 0)),
                action,
                signature: Vec::new(),
            };
            let minimum = borsh::object_length(&envelope).map_err(|e| e.to_string())? as u64
                + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64
                + payload
                + BATCH_RESPONSE_WIRE_OVERHEAD_V1;
            let cost = cost(stage, minimum)?;
            if cost.bytes < minimum || cost.controls != 1 || cost.work == 0 {
                return Err("Batch delivery service cost omits signed control".into());
            }
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket {
                    id: self.packet_id(claim, scope, stage)?,
                    release_daa: service.start,
                    service_before: service.end,
                    cost,
                },
                predecessors,
            });
            Ok(())
        };
        let begin = self.packet_id(claim, scope, Stage::Begin)?;
        let dummy = Hash64::default();
        append(Stage::Begin, Action::Begin { total_bytes: maximum_bytes, body_hash: dummy }, 0, Vec::new())?;
        let mut predecessors = Vec::with_capacity(parts as usize);
        for index in 0..parts {
            let offset = index * PART;
            let stage = Stage::Part(offset);
            append(stage, Action::Part { draft_id: dummy, offset, bytes: Vec::new() }, PART.min(maximum_bytes - offset), vec![begin])?;
            predecessors.push(self.packet_id(claim, scope, stage)?);
        }
        append(Stage::Finish, Action::Finish { draft_id: dummy }, 0, predecessors)?;
        Ok(graph)
    }
}

/// Refuse a signed native Open if any declared range exceeds its original
/// response reservation. No provider identity or body is needed for this check.
pub(crate) fn check_open_response_slots_v1(
    rooted: &crate::palw_batch_court_v1::rooted::RootedBatchDemandV1,
    book: &ServiceReservations,
    owned: &std::collections::BTreeSet<[u8; 64]>,
    schedule: MaterialIntakeScheduleV1,
    phased: bool,
) -> Result<(), String> {
    let deadline =
        batch_open_response_deadline_v1(rooted.claim, rooted.scope_id, rooted.ordinal, &rooted.demand, book, owned, schedule, phased)?;
    // A later deadline would advertise response time with no corresponding
    // slots and could extend the wait into a reserved fallback turn.
    if rooted.demand.response_deadline_daa != deadline {
        return Err("Batch signed response deadline differs from reserved service".into());
    }
    Ok(())
}

/// Exclusive response deadline. Batch-only originals sign the end of the
/// immutable Finish window, so legal repairs do not change the signed root.
/// The input's proposed response deadline is intentionally not read here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn batch_open_response_deadline_v1(
    claim: Hash64,
    scope: Hash64,
    ordinal: u64,
    demand: &crate::palw_batch_court_v1::BatchDemandV1,
    book: &ServiceReservations,
    owned: &std::collections::BTreeSet<[u8; 64]>,
    schedule: MaterialIntakeScheduleV1,
    phased: bool,
) -> Result<u64, String> {
    use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
    if u64::from(demand.part_bytes) != PART || !(1..=crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1).contains(&demand.ranges.len()) {
        return Err("Batch native demand has an invalid part width or range count".into());
    }
    let parts: u64 = demand.ranges.iter().map(|range| u64::from(range.bytes).div_ceil(PART)).sum();
    if parts > crate::palw_batch_court_v1::MAX_BATCH_PARTS_V1 as u64 {
        return Err("Batch native demand exceeds the aggregate part bound".into());
    }
    let release = demand.opened_daa.checked_add(1).ok_or("Batch demand opening overflows")?;
    let hard = demand
        .case_window
        .claim_accepted_daa
        .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
        .ok_or("Batch original clock overflows")?;
    let service = release..hard.min(demand.case_window.collect_until_daa);
    let open_before = if phased {
        let id = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(claim, scope, ordinal)?.as_bytes();
        let packet = book.packet(&id).ok_or("Batch phased response has no reserved Open")?;
        if !owned.contains(&id) || book.assigned_daa(&id) != Some(demand.opened_daa) {
            return Err("Batch phased response differs from its reserved Open".into());
        }
        Some(packet.service_before)
    } else {
        None
    };
    let mut deadline = release;
    for (range_index, range) in demand.ranges.iter().enumerate() {
        if range.authentication != BatchRangeAuthenticationV1::WholeRange {
            return Err("Batch native demand requires whole range authentication".into());
        }
        let position = BatchResponsePositionV1 { demand_ordinal: ordinal, range_index: range_index as u32 };
        let span =
            checked_response_span_v1(position, claim, scope, u64::from(range.bytes), service.clone(), book, owned, open_before)?;
        let expires = schedule
            .draft_expires_v1(span.earliest_begin, hard, demand.case_window.material_until_daa)
            .map_err(|_| "Batch response draft expiry cannot be derived")?;
        if span.finish_before > expires {
            return Err("Batch response finish crosses the draft expiry".into());
        }
        deadline = deadline.max(span.finish_before);
    }
    if deadline >= demand.case_window.collect_until_daa || deadline > hard {
        return Err("Batch response has no exclusive deadline inside the collection window".into());
    }
    Ok(deadline)
}

struct ResponseSpanV1 {
    begin: u64,
    finish: u64,
    earliest_begin: u64,
    finish_before: u64,
}

#[allow(clippy::too_many_arguments)]
fn checked_response_span_v1(
    position: BatchResponsePositionV1,
    claim: Hash64,
    scope: Hash64,
    bytes: u64,
    service: std::ops::Range<u64>,
    book: &ServiceReservations,
    owned: &std::collections::BTreeSet<[u8; 64]>,
    open_before: Option<u64>,
) -> Result<ResponseSpanV1, String> {
    let graph = position.reservation_graph_v1(claim, scope, bytes, service.clone(), |_, bytes| {
        Ok(ServiceCost { bytes, work: 1, io: 0, controls: 1 })
    })?;
    let slot = |id| {
        book.assigned_daa(&id)
            .filter(|daa| service.contains(daa))
            .ok_or("Batch response position has no assigned service within the demand")
    };
    let packet = |id| book.packet(&id).ok_or("Batch response has no reserved packet");
    for node in &graph {
        let id = node.packet.id;
        let at = slot(id)?;
        let booked = packet(id)?;
        if !owned.contains(&id) || booked.cost.bytes < node.packet.cost.bytes {
            return Err("Batch response exceeds the original reserved packet".into());
        }
        if open_before.is_some() && (booked.release_daa < service.start || booked.service_before > service.end) {
            return Err("Batch response phase escapes the original collection window".into());
        }
        for &predecessor in &node.predecessors {
            if slot(predecessor)? >= at {
                return Err("Batch response reservation does not preserve causal order".into());
            }
            if open_before.is_some() && packet(predecessor)?.service_before > booked.release_daa {
                return Err("Batch response phases overlap their prerequisites".into());
            }
        }
    }
    let begin_id = graph[0].packet.id;
    let finish_id = graph[graph.len() - 1].packet.id;
    let begin = slot(begin_id)?;
    let finish = slot(finish_id)?;
    let (earliest_begin, finish_before) = if let Some(open_before) = open_before {
        let first = packet(begin_id)?.release_daa;
        if first <= open_before {
            return Err("Batch response has no preparation interval after Open".into());
        }
        (first, packet(finish_id)?.service_before)
    } else {
        (begin, finish.checked_add(1).ok_or("Batch response deadline overflows")?)
    };
    Ok(ResponseSpanV1 { begin, finish, earliest_begin, finish_before })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchResponsePartSlotV1 {
    pub offset: u64,
    pub bytes: u64,
    pub assigned_daa: u64,
    pub accepted_daa: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchResponseQuoteV1 {
    pub request_root: Hash64,
    pub draft_id: Hash64,
    pub total_bytes: u64,
    pub begin_daa: u64,
    pub finish_daa: u64,
    pub response_deadline_daa: u64,
    pub draft_expires_daa: u64,
    /// Accepted Begin, if a draft is still retained. Shared material may outlive that draft.
    pub retained_submitter: Option<PalwBondKeyV2>,
    pub retained_body_hash: Option<Hash64>,
    /// The original's authenticated material is available on this branch.
    /// It may have arrived through another request or provider.
    pub material_available: bool,
    pub parts: Vec<BatchResponsePartSlotV1>,
}

/// Read the selected branch's exact slots and still-retained partial progress.
/// The quote neither signs a response nor authorizes a provider or a body.
pub fn quote_batch_response_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    ordinal: u64,
    target: MaterialTargetV1,
) -> Result<BatchResponseQuoteV1, String> {
    use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
    let key = Key::CourtBatchDemand(claim, ordinal);
    let Some(Row::CourtBatchDemand(rooted)) = state.staged_court_row_v1(&key) else {
        return Err("Batch response quote has no accepted demand".into());
    };
    rooted.check(&key)?;
    rooted.check_context(state)?;
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim)?;
    let service = state.court_service_state_v1().ok_or("Batch response quote has no service book")?;
    context.check_binding(service)?;
    let owned = &service.claims.get(&claim).ok_or("Batch response quote has no live original binding")?.packet_ids;
    let target_id = target.id_v1(claim)?;
    let demand = &rooted.demand;
    let index = demand
        .ranges
        .binary_search_by_key(&target_id, |range| range.id)
        .map_err(|_| "Batch response target is outside the accepted demand")?;
    let range = &demand.ranges[index];
    if range.authentication != BatchRangeAuthenticationV1::WholeRange || u64::from(demand.part_bytes) != PART {
        return Err("Batch response quote has a foreign authentication mode or part width".into());
    }
    let total_bytes = u64::from(range.bytes);
    let position = BatchResponsePositionV1 { demand_ordinal: ordinal, range_index: index as u32 };
    let release = demand.opened_daa.checked_add(1).ok_or("Batch demand opening overflows")?;
    let span = checked_response_span_v1(
        position,
        claim,
        rooted.scope_id,
        total_bytes,
        release..demand.response_deadline_daa,
        &service.book,
        owned,
        None,
    )?;
    let (begin_daa, finish_daa) = (span.begin, span.finish);
    let request_root = rooted.request_root;
    let draft_id = material_batch_draft_id_v1(request_root, claim, target)?;
    let draft_key = Key::CourtMaterialDraft(claim, draft_id);
    let (draft_expires_daa, retained_submitter, retained_body_hash) = match state.staged_court_row_v1(&draft_key) {
        Some(Row::CourtMaterialDraft(draft)) => {
            draft.check(&draft_key)?;
            if draft.version != MATERIAL_BATCH_VERSION
                || draft.head != request_root
                || draft.target != target
                || draft.scope_id != rooted.scope_id
                || draft.total_bytes != total_bytes
                || draft.begun_daa != begin_daa
                || draft.material_until_daa != context.material_until_daa
                || draft.expires_daa > context.hard_daa
            {
                return Err("Batch response quote substitutes the retained draft".into());
            }
            (draft.expires_daa, Some(draft.submitter), Some(draft.body_hash))
        }
        None => {
            let schedule = super::material_intake_schedule_v1().map_err(|_| "Batch material storage schedule missing")?;
            (schedule.draft_expires_v1(begin_daa, context.hard_daa, context.material_until_daa)?, None, None)
        }
        _ => return Err("Batch response draft has another row type".into()),
    };
    if finish_daa >= draft_expires_daa {
        return Err("Batch response Finish crosses the draft expiry".into());
    }
    let mut parts = Vec::with_capacity(total_bytes.div_ceil(PART) as usize);
    for offset in (0..total_bytes).step_by(PART as usize) {
        let packet = position.packet_id(claim, rooted.scope_id, Stage::Part(offset))?;
        let assigned_daa = service.book.assigned_daa(&packet).ok_or("Batch response part has no reserved DAA")?;
        let accepted_daa = match state.staged_court_row_v1(&Key::CourtMaterialPart(draft_id, offset)) {
            Some(Row::CourtMaterialPart(part)) if part.version == MATERIAL_BATCH_VERSION && part.claim == claim => {
                part.check_loaded_context(state)?;
                Some(part.daa)
            }
            None => None,
            _ => return Err("Batch response part has another row type".into()),
        };
        parts.push(BatchResponsePartSlotV1 { offset, bytes: PART.min(total_bytes - offset), assigned_daa, accepted_daa });
    }
    Ok(BatchResponseQuoteV1 {
        request_root,
        draft_id,
        total_bytes,
        begin_daa,
        finish_daa,
        response_deadline_daa: demand.response_deadline_daa,
        draft_expires_daa,
        retained_submitter,
        retained_body_hash,
        material_available: super::target_complete_v1(state, claim, target)?,
        parts,
    })
}

/// An exact signed delivery branch of a previously accepted demand. Original
/// admission must separately reserve capacity for the maximum native branch.
pub(crate) struct MaterialBatchDeliveryV1 {
    position: BatchResponsePositionV1,
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
        demand_ordinal: u64,
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
        let index = demand
            .ranges
            .binary_search_by_key(&target, |range| range.id)
            .map_err(|_| "Batch material target is outside the accepted demand")?;
        let range = &demand.ranges[index];
        let release_daa = demand.opened_daa.checked_add(1).ok_or("Batch demand opening overflows")?;
        let position = BatchResponsePositionV1 { demand_ordinal, range_index: index as u32 };
        let delivery = Self::new(
            request_root,
            declaration,
            native_maximum,
            demand.case_window,
            release_daa,
            demand.response_deadline_daa,
            position,
        )?;
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
        position: BatchResponsePositionV1,
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
        let draft = material_batch_draft_id_v1(request_root, declaration.claim, declaration.target)?;
        Ok(Self {
            position,
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
        self.position.packet_id(self.claim, self.scope, stage)
    }

    #[cfg(test)]
    fn service_graph_v1(
        &self,
        release_daa: u64,
        service_before: u64,
        mut cost: impl FnMut(u64) -> Result<ServiceCost, String>,
    ) -> Result<Vec<CausalServicePacketV1>, String> {
        if release_daa < self.release_daa || service_before > self.service_before {
            return Err("Batch delivery service interval is outside its accepted demand".into());
        }
        self.position.reservation_graph_v1(self.claim, self.scope, self.total, release_daa..service_before, |_, bytes| cost(bytes))
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
    if service.book != *book
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

/// Bound the canonical service sweep by immutable, accepted Batch windows.
/// The book may move a missed opportunity inside these windows only.
pub(crate) fn reserve_rooted_batch_repair_v1(
    state: &PalwChainStateV2,
    service: &crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{TirNativeRequestsV1, btree_node_envelope_bytes_v1, reserve_native_requests_v1};
    type Id = [u8; 64];
    let mut largest = 0;
    let mut route_largest = 0;
    for binding in service.claims.values().filter(|binding| service.book.has_missed_slot_v1(&binding.packet_ids)) {
        let claim = binding.claim;
        let count = binding.packet_ids.len() as u64;
        if state.staged_court_range_v1(&Key::CourtBatchDemand(claim, 0), &Key::CourtBatchDemand(claim, u64::MAX)).next().is_some() {
            largest = largest.max(count);
        }
        if state.court_shared_initial_notice_v1(claim).is_some()
            && crate::palw_court_shared_tir_state_v1::context(state, claim).is_ok_and(|context| context.batch_notice_only_v1())
        {
            largest = largest.max(count);
            route_largest = route_largest.max(count);
        }
    }
    if largest == 0 {
        return Ok(None);
    }
    let per_slot = btree_node_envelope_bytes_v1::<Id, ()>()
        + btree_node_envelope_bytes_v1::<Id, u64>()
        + btree_node_envelope_bytes_v1::<(Hash64, Hash64), &MaterialDraftV1>();
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(TirNativeRequestsV1::product_v1(largest, per_slot as u64)?)?;
    q.add_v1(TirNativeRequestsV1::product_v1(route_largest, (2 * btree_node_envelope_bytes_v1::<Id, ()>()) as u64)?)?;
    q.vec_v1::<Id>(route_largest)?;
    q.vec_v1::<(Id, Id, u64, u64)>(largest)?;
    reserve_native_requests_v1(q.bytes_v1())
}

pub(crate) fn repair_rooted_batch_slots_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    ids: &std::collections::BTreeSet<[u8; 64]>,
    hard: u64,
    edges: &BTreeMap<[u8; 64], Vec<[u8; 64]>>,
    book: &mut ServiceReservations,
) -> Result<usize, ServiceRefusal> {
    use crate::palw_batch_court_v1::rooted::batch_open_packet_v1;
    use ServiceRefusal::{DeadlineExhausted, InvalidState};
    let first_demand = Key::CourtBatchDemand(claim, 0);
    let last_demand = Key::CourtBatchDemand(claim, u64::MAX);
    let demands = || state.staged_court_range_v1(&first_demand, &last_demand);
    let has_demand = demands().next().is_some();
    let route = selected_batch_repair_route_v1(state, claim)?;
    if route.is_none() && !has_demand {
        return book.repair_missing_ordered_v1(ids, hard, edges);
    }
    let mut repairable = ids.clone();
    if let Some((scope, target, plain_output)) = route {
        exclude_inactive_batch_route_v1(claim, scope, target, plain_output, ids, edges, &mut repairable)?;
        let key = Key::CourtBatchNumericalReceipt(claim);
        if let Some(row) = state.staged_court_row_v1(&key) {
            let Row::CourtBatchNumericalReceipt(receipt) = row else { return Err(InvalidState) };
            receipt.check(&key).map_err(|_| InvalidState)?;
            let notice = Key::CourtSharedQuery(receipt.notice);
            if receipt.scope != scope
                || target != (crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1::Numerical { leaf: receipt.leaf })
                || state.court_shared_initial_notice_v1(claim).is_none_or(|(_, key)| *key != notice)
            {
                return Err(InvalidState);
            }
            let (verification, _) = admission::batch_numerical_service_ids_v1(claim, scope).map_err(|_| InvalidState)?;
            if book.assigned_daa(&verification) != Some(receipt.at.daa_score) || !repairable.remove(&verification) {
                return Err(InvalidState);
            }
        }
        let key = Key::CourtBatchPlainOutputReceiptV2(claim);
        if let Some(row) = state.staged_court_row_v1(&key) {
            let Row::CourtBatchPlainOutputReceiptV2(receipt) = row else { return Err(InvalidState) };
            receipt.check(&key).map_err(|_| InvalidState)?;
            let signed = &receipt.receipt.signed;
            if !plain_output
                || signed.scope != scope
                || target != crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1::PlainOutputFullIds
                || state.court_shared_initial_notice_v1(claim).is_none_or(|(_, key)| *key != Key::CourtSharedQuery(signed.notice))
            {
                return Err(InvalidState);
            }
            let control = plain_output::batch_plain_output_control_id_v2(claim, scope).map_err(|_| InvalidState)?;
            if book.assigned_daa(&control) != Some(receipt.receipt.at.daa_score) || !repairable.remove(&control) {
                return Err(InvalidState);
            }
        }
    }
    if !has_demand {
        return book.repair_missing_ordered_v1(&repairable, hard, edges);
    }
    let mut retained = BTreeMap::new();
    for (key, row) in state.staged_court_range_v1(
        &Key::CourtMaterialDraft(claim, Hash64::from_bytes([0; 64])),
        &Key::CourtMaterialDraft(claim, Hash64::from_bytes([255; 64])),
    ) {
        let Row::CourtMaterialDraft(draft) = row else { return Err(InvalidState) };
        if draft.version == MATERIAL_BATCH_VERSION {
            draft.check(key).map_err(|_| InvalidState)?;
            let target = draft.target.id_v1(claim).map_err(|_| InvalidState)?;
            if retained.len() == ids.len() || retained.insert((draft.head, target), draft.as_ref()).is_some() {
                return Err(InvalidState);
            }
        }
    }
    let schedule = super::material_intake_schedule_v1().map_err(|_| InvalidState)?;
    let full_ids = MaterialTargetV1::FullIds.id_v1(claim).map_err(|_| InvalidState)?;
    let mut expires = BTreeMap::new();
    let mut bound = |id, before| {
        if !ids.contains(&id) || expires.insert(id, before).is_some() {
            return Err(InvalidState);
        }
        Ok(())
    };
    let mut spans = Vec::new();
    for (key, row) in demands() {
        let Row::CourtBatchDemand(rooted) = row else { return Err(InvalidState) };
        rooted.check(key).map_err(|_| InvalidState)?;
        rooted.check_context(state).map_err(|_| InvalidState)?;
        let demand = &rooted.demand;
        let open = batch_open_packet_v1(claim, rooted.scope_id, rooted.ordinal).map_err(|_| InvalidState)?.as_bytes();
        if !ids.contains(&open) || book.assigned_daa(&open) != Some(demand.opened_daa) {
            return Err(InvalidState);
        }
        repairable.remove(&open);
        // A signed Open can use a prefix of the original maximum layout.
        // Unused positions keep their historical slots and causal edges, but
        // must not compete with live response work during repair.
        for index in demand.ranges.len()..crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1 {
            let position = BatchResponsePositionV1 { demand_ordinal: rooted.ordinal, range_index: index as u32 };
            let begin = position.packet_id(claim, rooted.scope_id, Stage::Begin).map_err(|_| InvalidState)?;
            if !ids.contains(&begin) {
                break;
            }
            let finish = position.packet_id(claim, rooted.scope_id, Stage::Finish).map_err(|_| InvalidState)?;
            if !ids.contains(&finish) {
                return Err(InvalidState);
            }
            repairable.remove(&begin);
            repairable.remove(&finish);
            exclude_unused_response_parts_v1(position, claim, rooted.scope_id, 0, ids, &mut repairable)?;
        }
        for (index, range) in demand.ranges.iter().enumerate() {
            let position = BatchResponsePositionV1 { demand_ordinal: rooted.ordinal, range_index: index as u32 };
            let packet = |stage| position.packet_id(claim, rooted.scope_id, stage).map_err(|_| InvalidState);
            let begin = packet(Stage::Begin)?;
            let finish = packet(Stage::Finish)?;
            let mut before = demand.response_deadline_daa;
            let material_available = if range.id == full_ids {
                super::target_complete_v1(state, claim, MaterialTargetV1::FullIds).map_err(|_| InvalidState)?
            } else {
                matches!(state.staged_court_row_v1(&Key::CourtMaterial(claim, range.id)), Some(Row::CourtMaterial(_)))
            };
            let draft = retained.remove(&(rooted.request_root, range.id));
            if let Some(draft) = draft {
                if draft.scope_id != rooted.scope_id
                    || draft.total_bytes != u64::from(range.bytes)
                    || draft.material_until_daa != rooted.material_until()
                    || book.assigned_daa(&begin) != Some(draft.begun_daa)
                {
                    return Err(InvalidState);
                }
                before = before.min(draft.expires_daa);
                repairable.remove(&begin);
            }
            for offset in (0..u64::from(range.bytes)).step_by(PART as usize) {
                let id = packet(Stage::Part(offset))?;
                if let Some(row) = draft.and_then(|draft| state.staged_court_row_v1(&Key::CourtMaterialPart(draft.draft_id, offset))) {
                    let Row::CourtMaterialPart(part) = row else { return Err(InvalidState) };
                    if part.version != MATERIAL_BATCH_VERSION || part.claim != claim || book.assigned_daa(&id) != Some(part.daa) {
                        return Err(InvalidState);
                    }
                    repairable.remove(&id);
                }
                if material_available {
                    repairable.remove(&id);
                }
                bound(id, before)?;
            }
            let first_unused = u64::from(range.bytes).div_ceil(PART);
            exclude_unused_response_parts_v1(position, claim, rooted.scope_id, first_unused, ids, &mut repairable)?;
            bound(begin, before)?;
            bound(finish, before)?;
            if material_available {
                repairable.remove(&begin);
                repairable.remove(&finish);
                continue;
            }
            spans.push((begin, finish, rooted.material_until(), before));
        }
    }
    if !retained.is_empty() {
        return Err(InvalidState);
    }
    book.repair_missing_ordered_checked_v1(
        &repairable,
        hard,
        edges,
        |id| expires.get(id).copied(),
        |candidate| {
            for (begin, finish, material, deadline) in spans {
                let begin = candidate.assigned_daa(&begin).ok_or(InvalidState)?;
                let finish = candidate.assigned_daa(&finish).ok_or(InvalidState)?;
                let expires = schedule.draft_expires_v1(begin, hard, material).map_err(|_| DeadlineExhausted)?;
                if finish >= expires.min(deadline) {
                    return Err(DeadlineExhausted);
                }
            }
            Ok(())
        },
    )
}

fn selected_batch_repair_route_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<(Hash64, crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1, bool)>, ServiceRefusal> {
    use crate::palw_court_shared_tir_state_v1 as shared;
    let Some((_, key)) = state.court_shared_initial_notice_v1(claim) else { return Ok(None) };
    let Ok(context) = shared::context(state, claim) else { return Ok(None) };
    if !context.batch_notice_only_v1() {
        return Ok(None);
    }
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(key) else { return Err(ServiceRefusal::InvalidState) };
    let target = crate::palw_batch_court_v1::rooted::checked_initial_notice_target_v1(state, context, notice, notice.request_id)
        .map_err(|_| ServiceRefusal::InvalidState)?
        .ok_or(ServiceRefusal::InvalidState)?;
    Ok(Some((context.scope_id, target, context.plain_batch_output_v2())))
}

/// The common terminal and non-Batch obligations stay repairable. Keep the
/// unused route's packets and edges in the book for reorg and audit.
fn exclude_inactive_batch_route_v1(
    claim: Hash64,
    scope: Hash64,
    target: crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1,
    plain: bool,
    owned: &std::collections::BTreeSet<[u8; 64]>,
    edges: &BTreeMap<[u8; 64], Vec<[u8; 64]>>,
    repairable: &mut std::collections::BTreeSet<[u8; 64]>,
) -> Result<(), ServiceRefusal> {
    use crate::palw_batch_court_v1::rooted::batch_open_packet_v1;
    use crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1;
    use ServiceRefusal::InvalidState;
    let (numerical, terminal) = admission::batch_numerical_service_ids_v1(claim, scope).map_err(|_| InvalidState)?;
    let parents = edges.get(&terminal).ok_or(InvalidState)?;
    if !owned.contains(&terminal) || !parents.contains(&numerical) {
        return Err(InvalidState);
    }
    let stop = match parents.as_slice() {
        [_] if !plain && matches!(target, BatchNoticeTargetV1::Numerical { .. }) => return Ok(()),
        [first, second] if *first == numerical && *second != numerical => *second,
        [first, second] if *second == numerical && *first != numerical => *first,
        _ => return Err(InvalidState),
    };
    let numerical_open = batch_open_packet_v1(claim, scope, 0).map_err(|_| InvalidState)?.as_bytes();
    let plain_control = plain_output::batch_plain_output_control_id_v2(claim, scope).map_err(|_| InvalidState)?;
    if plain && stop != plain_control
        || matches!(target, BatchNoticeTargetV1::PlainOutputFullIds) && !plain
        || matches!(target, BatchNoticeTargetV1::StopFullIds) && plain
    {
        return Err(InvalidState);
    }
    let ids_ordinal = if plain { plain_output::BATCH_PLAIN_OUTPUT_ORDINAL_V2 } else { constraint_stop::BATCH_STOP_ORDINAL_V1 };
    let stop_open = batch_open_packet_v1(claim, scope, ids_ordinal).map_err(|_| InvalidState)?.as_bytes();
    let mut pending = Vec::with_capacity(owned.len());
    let mut ancestors = |root| {
        let mut seen = std::collections::BTreeSet::from([root]);
        pending.push(root);
        while let Some(id) = pending.pop() {
            if !owned.contains(&id) || id == terminal {
                return Err(InvalidState);
            }
            for &parent in edges.get(&id).into_iter().flatten() {
                if !owned.contains(&parent) {
                    return Err(InvalidState);
                }
                if seen.insert(parent) {
                    pending.push(parent);
                }
            }
        }
        Ok(seen)
    };
    let numerical_route = ancestors(numerical)?;
    let stop_route = ancestors(stop)?;
    if !numerical_route.contains(&numerical_open)
        || !stop_route.contains(&stop_open)
        || numerical_route.contains(&stop_open)
        || stop_route.contains(&numerical_open)
    {
        return Err(InvalidState);
    }
    let (active, inactive) = match target {
        BatchNoticeTargetV1::Numerical { .. } => (numerical_route, stop_route),
        BatchNoticeTargetV1::StopFullIds | BatchNoticeTargetV1::PlainOutputFullIds => (stop_route, numerical_route),
    };
    for id in inactive.difference(&active) {
        repairable.remove(id);
    }
    Ok(())
}

fn exclude_unused_response_parts_v1(
    position: BatchResponsePositionV1,
    claim: Hash64,
    scope: Hash64,
    first_unused: u64,
    owned: &std::collections::BTreeSet<[u8; 64]>,
    repairable: &mut std::collections::BTreeSet<[u8; 64]>,
) -> Result<(), ServiceRefusal> {
    for index in first_unused..crate::palw_batch_court_v1::MAX_BATCH_PARTS_V1 as u64 {
        let id = position.packet_id(claim, scope, Stage::Part(index * PART)).map_err(|_| ServiceRefusal::InvalidState)?;
        if !owned.contains(&id) {
            break;
        }
        repairable.remove(&id);
    }
    Ok(())
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
    fn draft_ceiling_counts_only_owned_immutable_packet_costs() {
        let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
        let packet =
            |id, release_daa, bytes| ServicePacket { id: [id; 64], release_daa, service_before: release_daa + 1, cost: cost(bytes) };
        let mut book = ServiceReservations::new(cost(u64::MAX)).unwrap();
        book.reserve(&[packet(1, 1, 50), packet(2, 2, 60), packet(3, 3, u64::MAX)], 5).unwrap();
        let mut owned = std::collections::BTreeSet::from([[1; 64], [2; 64]]);
        assert_eq!(sum_booked_wire_bytes_v1(&book, &owned), Ok(110));
        owned.insert([3; 64]);
        assert!(sum_booked_wire_bytes_v1(&book, &owned).is_err());
        owned.remove(&[3; 64]);
        owned.insert([4; 64]);
        assert!(sum_booked_wire_bytes_v1(&book, &owned).is_err());
    }

    #[test]
    fn repair_excludes_only_the_unused_tail_of_an_owned_response() {
        let claim = Hash64::from_u64_word(1);
        let scope = Hash64::from_u64_word(2);
        let position = BatchResponsePositionV1 { demand_ordinal: 3, range_index: 0 };
        let id = |stage| position.packet_id(claim, scope, stage).unwrap();
        let begin = id(Stage::Begin);
        let finish = id(Stage::Finish);
        let needed = id(Stage::Part(0));
        let unused = id(Stage::Part(PART));
        let owned = [begin, finish, needed, unused].into_iter().collect::<std::collections::BTreeSet<_>>();
        let mut repairable = owned.clone();
        exclude_unused_response_parts_v1(position, claim, scope, 1, &owned, &mut repairable).unwrap();
        assert_eq!(repairable, [begin, finish, needed].into_iter().collect());
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
        let delivery =
            MaterialBatchDeliveryV1::new(h(5), &begin, 1, case, first, first + 8, BatchResponsePositionV1::default()).unwrap();
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
        let delivery =
            MaterialBatchDeliveryV1::new(h(5), &begin, 1, case, first, first + 12, BatchResponsePositionV1::default()).unwrap();
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
        let delivery = MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, 0, &action, PART + 1).unwrap();
        assert_eq!(delivery.request_root_v1(), demand.checked_request_root_v1().unwrap());
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, 0, &action, PART).is_err());
        let mut wrong_context = context.clone();
        wrong_context.material_until_daa += 1;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&wrong_context, &demand, 0, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::IndependentParts;
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, 0, &action, PART + 1).is_err());
        demand.ranges[0].authentication = BatchRangeAuthenticationV1::WholeRange;
        demand.ranges[0].id = h(11);
        assert!(MaterialBatchDeliveryV1::from_accepted_demand_v1(&context, &demand, 0, &action, PART + 1).is_err());
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
        let own = MaterialBatchDeliveryV1::new(request, &action, 1, test_case(), 11, 14, BatchResponsePositionV1::default()).unwrap();
        let foreign =
            MaterialBatchDeliveryV1::new(other_request, &action, 1, test_case(), 11, 14, BatchResponsePositionV1::default()).unwrap();
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
    fn missed_inactive_route_does_not_block_selected_finish_repair() {
        use crate::palw_batch_court_v1::rooted::batch_open_packet_v1;
        use crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1;
        let h = Hash64::from_u64_word;
        let (claim, scope) = (h(71), h(72));
        let (verify, terminal) = admission::batch_numerical_service_ids_v1(claim, scope).unwrap();
        let numerical_open = batch_open_packet_v1(claim, scope, 0).unwrap().as_bytes();
        let stop_open = batch_open_packet_v1(claim, scope, constraint_stop::BATCH_STOP_ORDINAL_V1).unwrap().as_bytes();
        let (finish, stop, unrelated) = (h(73).as_bytes(), h(74).as_bytes(), h(75).as_bytes());
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let node = |id, release_daa, service_before, predecessors| CausalServicePacketV1 {
            packet: ServicePacket { id, release_daa, service_before, cost },
            predecessors,
        };
        let graph = [
            node(numerical_open, 10, 12, vec![]),
            node(finish, 15, 20, vec![numerical_open]),
            node(verify, 20, 22, vec![finish]),
            node(stop_open, 11, 13, vec![]),
            node(stop, 21, 23, vec![stop_open]),
            node(terminal, 23, 25, vec![verify, stop]),
            node(unrelated, 22, 25, vec![]),
        ];
        let owned = graph.iter().map(|node| node.packet.id).collect::<std::collections::BTreeSet<_>>();
        let edges = graph.iter().map(|node| (node.packet.id, node.predecessors.clone())).collect::<BTreeMap<_, _>>();
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve_causal_v1(&graph, 25).unwrap();
        for at in [10, 16] {
            book.observe_block(at, h(at).as_bytes(), ServiceCost::default()).unwrap();
        }
        let original = book.clone();
        assert_eq!(book.repair_missing_ordered_v1(&owned, 25, &edges), Err(ServiceRefusal::DeadlineExhausted));
        assert_eq!(book, original);
        let mut selected = owned.clone();
        exclude_inactive_batch_route_v1(
            claim,
            scope,
            BatchNoticeTargetV1::Numerical { leaf: 0 },
            false,
            &owned,
            &edges,
            &mut selected,
        )
        .unwrap();
        assert!(selected.contains(&terminal) && selected.contains(&unrelated));
        assert!(!selected.contains(&stop_open) && !selected.contains(&stop));
        assert_eq!(book.repair_missing_ordered_v1(&selected, 25, &edges), Ok(1));
        assert_eq!(book.assigned_daa(&finish), Some(17));
        assert_eq!(book.assigned_daa(&stop_open), original.assigned_daa(&stop_open));
        let mut selected = owned.clone();
        exclude_inactive_batch_route_v1(claim, scope, BatchNoticeTargetV1::StopFullIds, false, &owned, &edges, &mut selected).unwrap();
        assert!(!selected.contains(&numerical_open) && !selected.contains(&finish) && !selected.contains(&verify));
        assert!(selected.contains(&stop_open) && selected.contains(&stop) && selected.contains(&terminal));
    }

    #[test]
    fn opening_requires_all_owned_response_slots_and_the_native_draft_window() {
        use crate::palw_batch_court_v1::{BatchDemandV1, BatchRangeAuthenticationV1, BatchRangeV1, rooted::RootedBatchDemandV1};
        let h = Hash64::from_u64_word;
        let position = BatchResponsePositionV1 { demand_ordinal: 7, range_index: 0 };
        let rooted = RootedBatchDemandV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            ordinal: 7,
            notice_id: h(3),
            opened_block: h(4),
            request_root: h(5),
            demand: BatchDemandV1 {
                case_id: h(6),
                case_window: test_case(),
                opened_daa: 10,
                response_deadline_daa: 14,
                maximum_bytes: 1,
                part_bytes: PART as u32,
                ranges: vec![BatchRangeV1 { id: h(7), bytes: 1, authentication: BatchRangeAuthenticationV1::WholeRange }],
            },
        };
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let graph = position.reservation_graph_v1(rooted.claim, rooted.scope_id, PART + 1, 11..15, |_, bytes| cost(bytes)).unwrap();
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 2, io: 2, controls: 2 }).unwrap();
        book.reserve_causal_v1(&graph, 15).unwrap();
        let mut owned = graph.iter().map(|node| node.packet.id).collect::<std::collections::BTreeSet<_>>();
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
        assert!(check_open_response_slots_v1(&rooted, &book, &owned, schedule, false).is_ok());
        let finish = book.assigned_daa(&graph.last().unwrap().packet.id).unwrap();
        let mut earliest = rooted.clone();
        earliest.demand.response_deadline_daa = finish;
        let deadline = batch_open_response_deadline_v1(
            earliest.claim,
            earliest.scope_id,
            earliest.ordinal,
            &earliest.demand,
            &book,
            &owned,
            schedule,
            false,
        )
        .unwrap();
        assert_eq!(deadline, finish + 1);
        assert!(check_open_response_slots_v1(&earliest, &book, &owned, schedule, false).is_err());
        earliest.demand.response_deadline_daa = deadline;
        assert!(check_open_response_slots_v1(&earliest, &book, &owned, schedule, false).is_ok());
        earliest.demand.response_deadline_daa = deadline + 1;
        assert!(check_open_response_slots_v1(&earliest, &book, &owned, schedule, false).is_err());
        earliest.demand.response_deadline_daa = deadline;
        earliest.demand.case_window.collect_until_daa = deadline;
        assert!(check_open_response_slots_v1(&earliest, &book, &owned, schedule, false).is_err());
        let expired = MaterialIntakeScheduleV1 { draft_ttl_daa: 2, ..schedule };
        assert!(check_open_response_slots_v1(&rooted, &book, &owned, expired, false).is_err());
        owned.remove(&graph[1].packet.id);
        assert!(check_open_response_slots_v1(&rooted, &book, &owned, schedule, false).is_err());
    }

    #[test]
    fn maximum_response_slots_exist_before_the_signed_declaration() {
        let h = Hash64::from_u64_word;
        let position = BatchResponsePositionV1 { demand_ordinal: 7, range_index: 1 };
        let (claim, scope) = (h(1), h(2));
        let cost = |bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
        let graph = position.reservation_graph_v1(claim, scope, PART + 1, 11..15, |_, bytes| cost(bytes)).unwrap();
        let mut book = ServiceReservations::new(ServiceCost { bytes: PART * 2, work: 2, io: 2, controls: 2 }).unwrap();
        book.reserve_causal_v1(&graph, 15).unwrap();

        let mut signed = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim,
            scope_id: scope,
            target: MaterialTargetV1::PromptFlat,
            submitter: PalwBondKeyV2(TransactionOutpoint::new(h(6), 0)),
            action: Action::Begin { total_bytes: 1, body_hash: h(8) },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let delivery = MaterialBatchDeliveryV1::new(h(5), &signed, 1, test_case(), 11, 15, position).unwrap();
        assert_eq!(delivery.check_slot_v1(&book, 11, &signed).unwrap(), graph[0].packet.id);
        signed.action = Action::Part { draft_id: delivery.draft, offset: 0, bytes: vec![0] };
        let part = delivery.packet_id(Stage::Part(0)).unwrap();
        assert_eq!(delivery.check_slot_v1(&book, book.assigned_daa(&part).unwrap(), &signed).unwrap(), part);
        let foreign = BatchResponsePositionV1 { range_index: 2, ..position };
        assert!(book.packet(&foreign.packet_id(claim, scope, Stage::Begin).unwrap()).is_none());
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
        let delivery =
            MaterialBatchDeliveryV1::new(request, &action, total, test_case(), 11, 14, BatchResponsePositionV1::default()).unwrap();
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
        let other = MaterialBatchDeliveryV1::new(
            Hash64::from_u64_word(6),
            &action,
            total,
            test_case(),
            11,
            14,
            BatchResponsePositionV1 { demand_ordinal: 1, range_index: 0 },
        )
        .unwrap();
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
