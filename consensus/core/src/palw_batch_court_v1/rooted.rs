//! Branch-retained Batch demand and its original's reserved opening slot.
use super::{BatchCourtWindowV1, BatchDemandV1, BatchRangeV1, MAX_BATCH_PART_BYTES_V1, MAX_BATCH_RANGES_V1, carriage::BatchStopControlV1};
use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::batch_payload;
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_native_resource_v1::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, wire_bad};
use crate::palw_staged_court_state_v1::{PalwSharedCourtV1, PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2};
use crate::{BlockHash, Hash64};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io;

/// Numerical result of one reserved verification slot. The terminal operation
/// consumes this finding separately from claim credit and accounting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootedBatchNumericalReceiptV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope: Hash64,
    pub source: Hash64,
    pub notice: Hash64,
    pub leaf: u64,
    pub intent_digest: Hash64,
    pub at: PalwBlockContextV2,
    pub material_until_daa: u64,
    pub fault_evidence: Option<Hash64>,
    /// V2 retains the canonical TIR operands without the registered program.
    pub fault_bytes: Option<PalwSharedCourtV1<Vec<u8>>>,
}

pub(crate) const MAX_NUMERICAL_FAULT_BYTES_V1: usize = crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(
    crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS as u64,
) as usize;

impl RootedBatchNumericalReceiptV1 {
    pub(crate) fn digest_v1(&self) -> Result<Hash64, String> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-numerical-finding/v1", self)
    }

    pub(crate) fn check(&self, key: &Key) -> Result<(), String> {
        let valid_fault_shape = match self.version {
            1 => self.fault_bytes.is_none(),
            2 => self.fault_evidence.is_some() == self.fault_bytes.is_some()
                && self.fault_bytes.as_ref().is_none_or(|bytes| !bytes.is_empty() && bytes.len() <= MAX_NUMERICAL_FAULT_BYTES_V1),
            _ => false,
        };
        if !valid_fault_shape
            || *key != Key::CourtBatchNumericalReceipt(self.claim)
            || [self.claim, self.scope, self.source, self.notice, self.intent_digest, self.at.block].contains(&Hash64::default())
            || self.at.daa_score >= self.material_until_daa
            || self.fault_evidence == Some(Hash64::default())
        {
            return Err("invalid rooted numerical Batch receipt".into());
        }
        Ok(())
    }

    pub(crate) fn check_context(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2) -> Result<(), String> {
        let context = shared::context(state, self.claim)?;
        let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(self.notice)) else {
            return Err("numerical Batch receipt has no accepted notice".into());
        };
        if (context.scope_id, context.source_id, context.material_until_daa)
            != (self.scope, self.source, self.material_until_daa)
            || (notice.claim, notice.scope_id) != (self.claim, self.scope)
            || checked_initial_notice_target_v1(state, context, notice, self.notice)?
                != Some(shared::BatchNoticeTargetV1::Numerical { leaf: self.leaf })
            || self.at.daa_score <= notice.opened_daa
            || self.at.daa_score >= context.hard_daa
            || self.at.daa_score > point.daa_score
            || self.at.blue_score > point.blue_score
            || (self.at.blue_score == point.blue_score && self.at != *point)
        {
            return Err("numerical Batch receipt differs from its original or accepting point".into());
        }
        Ok(())
    }
}

/// Read the one accepted finding that the booked terminal operation can use.
/// An absent finding or an exhausted slot cannot yield a terminal decision.
pub(crate) fn reserved_numerical_terminal_v1<'a>(
    state: &'a PalwChainStateV2,
    claim: Hash64,
    receipt_digest: Hash64,
    at: &PalwBlockContextV2,
) -> Result<&'a RootedBatchNumericalReceiptV1, String> {
    let key = Key::CourtBatchNumericalReceipt(claim);
    let Some(Row::CourtBatchNumericalReceipt(receipt)) = state.staged_court_row_v1(&key) else {
        return Err("numerical terminal has no accepted verification receipt".into());
    };
    receipt.check(&key)?;
    if receipt.fault_evidence.is_some() && receipt.fault_bytes.is_none() {
        return Err("numerical terminal fault has no retained adjudication material".into());
    }
    let context = shared::context(state, claim)?;
    if (receipt.scope, receipt.source, receipt.material_until_daa) != (context.scope_id, context.source_id, context.material_until_daa)
        || state.court_shared_initial_notice_v1(claim).is_none_or(|(_, key)| *key != Key::CourtSharedQuery(receipt.notice))
    {
        return Err("numerical terminal differs from its original or accepted notice".into());
    }
    let service = state.court_service_row_v3_v1().ok_or("numerical terminal has no causal service book")?;
    context.check_binding(&service.state)?;
    let binding = service.state.claims.get(&claim).ok_or("numerical terminal has no original booking")?;
    let (verification, terminal) = crate::palw_court_shared_material_intake_v1::batch::admission::batch_numerical_service_ids_v1(
        claim, receipt.scope,
    ).map_err(|_| "numerical terminal packet identities are invalid")?;
    if receipt.digest_v1()? != receipt_digest
        || !binding.packet_ids.contains(&verification)
        || !binding.packet_ids.contains(&terminal)
        || service.state.book.assigned_daa(&verification) != Some(receipt.at.daa_score)
        || service.state.book.assigned_daa(&terminal) != Some(at.daa_score)
        || service.causal.get(&terminal).is_none_or(|parents| parents.binary_search(&verification).is_err())
        || at.daa_score <= receipt.at.daa_score
        || at.daa_score >= context.hard_daa.min(receipt.material_until_daa)
    {
        return Err("numerical terminal differs from its accepted finding or reserved slot".into());
    }
    Ok(receipt)
}

impl BorshSerialize for RootedBatchNumericalReceiptV1 {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        self.check(&Key::CourtBatchNumericalReceipt(self.claim)).map_err(|why| wire_bad(&why))?;
        (self.version, self.claim, self.scope, self.source, self.notice, self.leaf, self.intent_digest, self.at,
            self.material_until_daa, self.fault_evidence).serialize(writer)?;
        if self.version == 2 {
            self.fault_bytes.as_ref().map_or(&[][..], |bytes| bytes.as_slice()).serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for RootedBatchNumericalReceiptV1 {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(reader)?;
        if !matches!(version, 1 | 2) {
            return Err(wire_bad("unknown numerical Batch receipt version"));
        }
        let mut receipt = Self {
            version,
            claim: Hash64::deserialize_reader(reader)?,
            scope: Hash64::deserialize_reader(reader)?,
            source: Hash64::deserialize_reader(reader)?,
            notice: Hash64::deserialize_reader(reader)?,
            leaf: u64::deserialize_reader(reader)?,
            intent_digest: Hash64::deserialize_reader(reader)?,
            at: PalwBlockContextV2::deserialize_reader(reader)?,
            material_until_daa: u64::deserialize_reader(reader)?,
            fault_evidence: Option::<Hash64>::deserialize_reader(reader)?,
            fault_bytes: None,
        };
        if version == 2 {
            let bytes = crate::palw_public_binding_v1::read_bounded_bytes_v1(reader, MAX_NUMERICAL_FAULT_BYTES_V1)?;
            if !bytes.is_empty() {
                receipt.fault_bytes = Some(Box::new(bytes).into());
            }
        }
        receipt.check(&Key::CourtBatchNumericalReceipt(receipt.claim)).map_err(|why| wire_bad(&why))?;
        Ok(receipt)
    }
}

impl NativeWireScanV1 for RootedBatchNumericalReceiptV1 {
    fn scan_native_wire_v1<R: io::Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let version = reader.short()?;
        if !matches!(version, 1 | 2) {
            return Err(wire_bad("unknown numerical Batch receipt version"));
        }
        let mut shape = NativeWireShapeV1::default();
        for _ in 0..4 {
            shape = shape.merge(Hash64::scan_native_wire_v1(reader)?)?;
        }
        shape = shape.merge(u64::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(Hash64::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(PalwBlockContextV2::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(u64::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(Option::<Hash64>::scan_native_wire_v1(reader)?)?;
        if version == 2 {
            reader.step()?;
            let bytes = u64::from(reader.word()?);
            if bytes > MAX_NUMERICAL_FAULT_BYTES_V1 as u64 {
                return Err(wire_bad("numerical Batch fault exceeds its retained byte cap"));
            }
            reader.skip(bytes)?;
            shape = shape.merge(NativeWireShapeV1 { byte_entries: bytes, ..Default::default() }.allocation(bytes, 1)?)?;
            if bytes != 0 {
                // Box-to-Arc conversion and the shared Vec header.
                shape = shape.allocation(1, 2 * size_of::<Vec<u8>>() + 3 * size_of::<usize>())?;
            }
        }
        Ok(shape)
    }
}

/// One immutable constrained Stop finding on the accepting branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootedBatchStopReceiptV1 {
    pub version: u16,
    pub claim: Hash64,
    pub source_id: Hash64,
    pub operands_digest: Hash64,
    pub at: PalwBlockContextV2,
    pub material_until_daa: u64,
    pub fault_position: Option<u32>,
    /// V2 keeps the actual signed operands on the same retention horizon.
    pub operands: Option<PalwSharedCourtV1<BatchStopControlV1>>,
}

impl RootedBatchStopReceiptV1 {
    pub(crate) fn digest_operands(operands: &[u8]) -> Result<Hash64, String> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-stop-receipt/v1", &operands)
    }

    pub(crate) fn check(&self, key: &Key) -> Result<(), String> {
        if *key != Key::CourtBatchStopReceipt(self.claim)
            || [self.claim, self.source_id, self.operands_digest, self.at.block].contains(&Hash64::default())
            || self.at.daa_score >= self.material_until_daa
        {
            return Err("invalid rooted Batch Stop receipt".into());
        }
        match (self.version, &self.operands) {
            (1, None) => Ok(()),
            (2, Some(operands)) => {
                let (claim, _) = batch_payload::batch_stop_identity_v1(&operands.operands).map_err(|err| err.to_string())?;
                if claim != self.claim || Self::digest_operands(&operands.operands)? != self.operands_digest {
                    return Err("retained Batch Stop operands differ from their receipt".into());
                }
                Ok(())
            }
            _ => Err("Batch Stop receipt version and retained operands disagree".into()),
        }
    }

    pub(crate) fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim)
            .ok_or("Batch Stop receipt lost its original frame")?;
        let point = state.last_point().ok_or("Batch Stop receipt has no accepted snapshot")?;
        let original = frame.original_v1();
        let plan = frame.plan_v1();
        if !original.job.is_constraint()
            || self.source_id != plan.source_id
            || self.material_until_daa != plan.terms.material_until_daa
            || self.at.daa_score < plan.terms.accepted_daa
            || self.at.daa_score
                >= plan
                    .terms
                    .accepted_daa
                    .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
                    .ok_or("Batch Stop receipt original clock overflows")?
            || self.at.daa_score > point.daa_score
            || self.at.blue_score > point.blue_score
            || (self.at.blue_score == point.blue_score && self.at != *point)
            || self.fault_position.is_some_and(|position| position >= original.decode_tokens_executed)
        {
            return Err("Batch Stop receipt differs from its retained original or accepting point".into());
        }
        Ok(())
    }
}

impl BorshSerialize for RootedBatchStopReceiptV1 {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let operands = match (self.version, &self.operands) {
            (1, None) => None,
            (2, Some(operands)) => Some(operands),
            _ => return Err(wire_bad("Batch Stop receipt version and retained operands disagree")),
        };
        self.version.serialize(writer)?;
        self.claim.serialize(writer)?;
        self.source_id.serialize(writer)?;
        self.operands_digest.serialize(writer)?;
        self.at.serialize(writer)?;
        self.material_until_daa.serialize(writer)?;
        self.fault_position.serialize(writer)?;
        if let Some(operands) = operands {
            operands.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for RootedBatchStopReceiptV1 {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(reader)?;
        if !matches!(version, 1 | 2) {
            return Err(wire_bad("unknown Batch Stop receipt version"));
        }
        Ok(Self {
            version,
            claim: Hash64::deserialize_reader(reader)?,
            source_id: Hash64::deserialize_reader(reader)?,
            operands_digest: Hash64::deserialize_reader(reader)?,
            at: PalwBlockContextV2::deserialize_reader(reader)?,
            material_until_daa: u64::deserialize_reader(reader)?,
            fault_position: Option::<u32>::deserialize_reader(reader)?,
            operands: if version == 2 { Some(PalwSharedCourtV1::deserialize_reader(reader)?) } else { None },
        })
    }
}

impl NativeWireScanV1 for RootedBatchStopReceiptV1 {
    fn scan_native_wire_v1<R: io::Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let version = reader.short()?;
        if !matches!(version, 1 | 2) {
            return Err(wire_bad("unknown Batch Stop receipt version"));
        }
        let mut shape = NativeWireShapeV1::default();
        for _ in 0..3 {
            shape = shape.merge(Hash64::scan_native_wire_v1(reader)?)?;
        }
        shape = shape.merge(PalwBlockContextV2::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(u64::scan_native_wire_v1(reader)?)?;
        shape = shape.merge(Option::<u32>::scan_native_wire_v1(reader)?)?;
        if version == 2 {
            shape = shape.merge(PalwSharedCourtV1::<BatchStopControlV1>::scan_native_wire_v1(reader)?)?;
        }
        Ok(shape)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootedBatchDemandV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub ordinal: u64,
    pub notice_id: Hash64,
    pub opened_block: BlockHash,
    pub request_root: Hash64,
    pub demand: BatchDemandV1,
}

impl RootedBatchDemandV1 {
    pub(crate) fn key(&self) -> Key {
        Key::CourtBatchDemand(self.claim, self.ordinal)
    }

    pub(crate) fn material_until(&self) -> u64 {
        self.demand.case_window.material_until_daa
    }

    pub(crate) fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != self.key()
            || [self.claim, self.scope_id, self.notice_id, self.opened_block, self.request_root].contains(&Hash64::default())
            || self.demand.checked_request_root_v1().map_err(|err| format!("Batch demand: {err:?}"))? != self.request_root
        {
            return Err("rooted Batch demand version, key or request identity".into());
        }
        Ok(())
    }

    pub(crate) fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check_context_target_v1(state).map(|_| ())
    }

    fn check_context_target_v1(&self, state: &PalwChainStateV2) -> Result<Option<shared::BatchNoticeTargetV1>, String> {
        let context = shared::context(state, self.claim)?;
        let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(self.notice_id)) else {
            return Err("Batch demand lost its accepted notice".into());
        };
        let target = checked_initial_notice_target_v1(state, context, notice, self.notice_id)?;
        if notice.claim != self.claim
            || notice.scope_id != self.scope_id
            || notice.material_until_daa != self.material_until()
            || context.scope_id != self.scope_id
            || context.original_accepted_daa != self.demand.case_window.claim_accepted_daa
            || context.material_until_daa != self.material_until()
            || self.demand.response_deadline_daa > context.hard_daa
            || (target.is_some() && self.demand.case_window.collect_until_daa != context.hard_daa)
            || notice.opened_daa != self.demand.case_window.challenge_opened_daa
            || batch_case_id_v1(self.claim, self.scope_id, self.notice_id, notice.opened_block, notice.opened_daa, self.ordinal)?
                != self.demand.case_id
        {
            return Err("Batch demand substitutes its original or accepted notice".into());
        }
        check_notice_route_v1(state, self.claim, target, self.ordinal, &self.demand.ranges)?;
        Ok(target)
    }

    pub(crate) fn check_adoption(
        &self, state: &PalwChainStateV2, at: &PalwBlockContextV2, signed_bytes: u64,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        use crate::palw_state_v2::PalwStateV2Error::{CarriageInconsistent as Invalid, NativeResource};
        self.check(&self.key()).map_err(Invalid)?;
        let target = self.check_context_target_v1(state).map_err(Invalid)?;
        if (self.opened_block, self.demand.opened_daa) != (at.block, at.daa_score) {
            return Err(Invalid("Batch demand differs from the accepting point".into()));
        }
        let (book, binding, opened_daa) = reserved_open_slot_v1(state, self.claim, self.scope_id, self.ordinal, signed_bytes)
            .map_err(Invalid)?;
        if opened_daa != at.daa_score {
            return Err(Invalid("Batch demand opening differs from its reserved DAA".into()));
        }
        use crate::palw_court_shared_material_intake_v1 as material;
        let schedule = material::material_intake_schedule_v1().map_err(|_| Invalid("Batch material storage schedule missing".into()))?;
        material::batch::check_open_response_slots_v1(self, book, &binding.packet_ids, schedule, target.is_some()).map_err(Invalid)?;
        if let Some(shared::BatchNoticeTargetV1::Numerical { leaf }) = target {
            use material::batch::native_ranges::{MAX_BOOKED_NUMERICAL_RANGES_V1, numerical_demand_from_source_v1};
            let ordinal = usize::try_from(self.ordinal).map_err(|_| Invalid("numerical Batch demand ordinal overflows".into()))?;
            let context = shared::context(state, self.claim).map_err(Invalid)?;
            let expected = numerical_demand_from_source_v1(state, context, &binding.packet_ids, leaf, ordinal, MAX_BOOKED_NUMERICAL_RANGES_V1)
                .map_err(|why| match why {
                    NativeDecodeErrorV1::Decode(why) => Invalid(why),
                    NativeDecodeErrorV1::Resource(why) => NativeResource(why),
                })?;
            expected.check_signed_ranges_v1(&self.demand.ranges).map_err(Invalid)?;
        }
        Ok(())
    }
}

pub(crate) fn checked_initial_notice_target_v1(
    state: &PalwChainStateV2,
    context: &shared::SharedTirContextV1,
    notice: &shared::SharedTirQueryV1,
    notice_id: Hash64,
) -> Result<Option<shared::BatchNoticeTargetV1>, String> {
    let key = Key::CourtSharedQuery(notice_id);
    notice.check(&key)?;
    let (daa, initial_key) = state.court_shared_initial_notice_v1(context.claim).ok_or("Batch original has no accepted initial notice")?;
    if daa != notice.opened_daa || (context.batch_notice_only_v1() && *initial_key != key) {
        return Err("Batch demand must use the original's accepted initial notice".into());
    }
    if context.batch_notice_only_v1() {
        return notice.check_context_and_batch_target(state);
    }
    Ok(None)
}

fn check_notice_route_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    target: Option<shared::BatchNoticeTargetV1>,
    ordinal: u64,
    ranges: &[BatchRangeV1],
) -> Result<(), String> {
    use crate::palw_court_shared_material_intake_v1::{BATCH_STOP_ORDINAL_V1, batch_stop_full_ids_range_v1};
    use shared::BatchNoticeTargetV1::{Numerical, StopFullIds};
    match target {
        None => Ok(()),
        Some(Numerical { .. }) if ordinal != BATCH_STOP_ORDINAL_V1 => Ok(()),
        Some(StopFullIds) if ordinal == BATCH_STOP_ORDINAL_V1 => {
            let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("Batch Stop original frame absent")?;
            let range = batch_stop_full_ids_range_v1(claim, frame.original_v1().decode_tokens_executed)?;
            if ranges != [range] {
                return Err("Batch Stop notice requires its canonical FullIds range".into());
            }
            Ok(())
        }
        _ => Err("Batch Open route differs from its initial notice".into()),
    }
}

pub(crate) fn reserved_open_slot_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    scope: Hash64,
    ordinal: u64,
    signed_bytes: u64,
) -> Result<(&crate::palw_court_service_v1::ServiceReservations, &crate::palw_court_service_state_v2::ServiceClaimBinding, u64), String> {
    let service = state.court_service_state_v1().ok_or("Batch demand has no rooted service book")?;
    shared::context(state, claim)?.check_binding(service)?;
    let binding = service.claims.get(&claim).ok_or("Batch demand has no original binding")?;
    let packet_id = batch_open_packet_v1(claim, scope, ordinal)?.as_bytes();
    let packet = service.book.packet(&packet_id).ok_or("Batch demand has no reserved opening packet")?;
    if !binding.packet_ids.contains(&packet_id) || packet.cost.bytes < signed_bytes {
        return Err("Batch demand opening lacks reserved original capacity".into());
    }
    let daa = service.book.assigned_daa(&packet_id).ok_or("Batch demand opening has no assigned DAA")?;
    Ok((&service.book, binding, daa))
}

pub(super) fn quote_open_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    notice_id: Hash64,
    ordinal: u64,
    mut ranges: Vec<BatchRangeV1>,
) -> Result<super::carriage::SignedBatchDemandV1, String> {
    use crate::palw_court_shared_material_intake_v1 as material;
    let context = shared::context(state, claim)?;
    let notice_key = Key::CourtSharedQuery(notice_id);
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&notice_key) else {
        return Err("Batch quote has no accepted notice".into());
    };
    let target = checked_initial_notice_target_v1(state, context, notice, notice_id)?;
    if notice.claim != claim
        || notice.scope_id != context.scope_id
        || notice.material_until_daa != context.material_until_daa
        || state.staged_court_row_v1(&Key::CourtBatchDemand(claim, ordinal)).is_some()
    {
        return Err("Batch quote does not name an unused opening of the accepted notice".into());
    }
    check_notice_route_v1(state, claim, target, ordinal, &ranges)?;
    let signed_bytes = super::carriage::batch_open_wire_bytes_v1(ranges.len())?;
    let scope = context.scope_id;
    let (book, binding, opened_daa) = reserved_open_slot_v1(state, claim, scope, ordinal, signed_bytes)?;
    if state.last_point().is_some_and(|point| point.daa_score > opened_daa) {
        return Err("Batch quote's reserved opening DAA has passed".into());
    }
    ranges.sort_unstable_by_key(|range| range.id);
    let mut demand = BatchDemandV1 {
        case_id: batch_case_id_v1(claim, scope, notice_id, notice.opened_block, notice.opened_daa, ordinal)?,
        case_window: BatchCourtWindowV1 {
            claim_accepted_daa: context.original_accepted_daa,
            challenge_opened_daa: notice.opened_daa,
            collect_until_daa: context.hard_daa,
            material_until_daa: context.material_until_daa,
        },
        opened_daa,
        response_deadline_daa: 0,
        maximum_bytes: ranges.iter().map(|range| u64::from(range.bytes)).sum(),
        part_bytes: MAX_BATCH_PART_BYTES_V1,
        ranges,
    };
    let schedule = material::material_intake_schedule_v1().map_err(|_| "Batch material storage schedule missing")?;
    demand.response_deadline_daa = material::batch::batch_open_response_deadline_v1(
        claim, scope, ordinal, &demand, book, &binding.packet_ids, schedule, context.batch_notice_only_v1(),
    )?;
    demand.checked_request_root_v1().map_err(|why| format!("Batch quote shape: {why:?}"))?;
    Ok(super::carriage::SignedBatchDemandV1 { version: 1, claim, scope_id: scope, ordinal, notice_id, signer: notice.opened_by,
        demand, signature: Vec::new() })
}

pub(crate) fn batch_case_id_v1(
    claim: Hash64,
    scope: Hash64,
    notice: Hash64,
    notice_block: BlockHash,
    notice_daa: u64,
    demand_ordinal: u64,
) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/batch-case-demand/v1",
        &(claim, scope, notice, notice_block, notice_daa, demand_ordinal),
    )
}

/// Reserve the opening slot by original and ordinal before the notice or
/// request bytes exist. The committed row binds the selected request to it.
pub(crate) fn batch_open_packet_v1(claim: Hash64, scope: Hash64, ordinal: u64) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-open/v1", &(claim, scope, ordinal))
}

impl NativeWireScanV1 for RootedBatchDemandV1 {
    fn scan_native_wire_v1<R: io::Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        if reader.short()? != 1 {
            return Err(wire_bad("unknown rooted Batch demand version"));
        }
        reader.skip(5 * 64 + 8)?;
        // Demand case, four clocks, opening/response, maximum bytes and part width.
        reader.skip(64 + 7 * 8 + 4)?;
        let count = usize::try_from(reader.word()?).map_err(|_| wire_bad("rooted Batch range count overflow"))?;
        if !(1..=MAX_BATCH_RANGES_V1).contains(&count) {
            return Err(wire_bad("rooted Batch demand range count"));
        }
        for _ in 0..count {
            reader.step()?;
            reader.skip(64 + 4)?;
            if reader.byte()? > 1 {
                return Err(wire_bad("rooted Batch authentication mode"));
            }
        }
        NativeWireShapeV1 { collection_entries: count as u64, ..Default::default() }
            .allocation(count as u64, std::mem::size_of::<BatchRangeV1>())
    }
}

impl crate::palw_native_resource_v1::NativeHeapV1 for RootedBatchDemandV1 {
    fn native_members_v1(&self) -> Option<u64> {
        Some(self.demand.ranges.len() as u64)
    }

    fn native_heap_v1(&self, component: &str) -> crate::palw_native_resource_v1::NativeCoverageV1 {
        use crate::palw_native_resource_v1::NativeCoverageV1;
        match self.demand.ranges.capacity().checked_mul(std::mem::size_of::<BatchRangeV1>()).and_then(|n| u64::try_from(n).ok()) {
            Some(bytes) => NativeCoverageV1::Bounded { bytes },
            None => NativeCoverageV1::overflow::<Self>(component),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_batch_court_v1::{BatchCourtWindowV1, BatchRangeAuthenticationV1};

    #[test]
    fn numerical_receipt_wire_checks_original_key_and_version() {
        let h = Hash64::from_u64_word;
        let receipt = RootedBatchNumericalReceiptV1 {
            version: 1, claim: h(1), scope: h(2), source: h(3), notice: h(4),
            leaf: 7, intent_digest: h(5),
            at: PalwBlockContextV2 { block: h(6), daa_score: 4_000, blue_score: 5_000, subsidy: 0 },
            material_until_daa: 12_000, fault_evidence: Some(h(8)), fault_bytes: None,
        };
        let key = Key::CourtBatchNumericalReceipt(receipt.claim);
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 34);
        let row = Row::CourtBatchNumericalReceipt(Box::new(receipt.clone()));
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes[0], 41);
        assert_eq!(&bytes[1..], borsh::to_vec(&(
            1u16, receipt.claim, receipt.scope, receipt.source, receipt.notice, receipt.leaf, receipt.intent_digest,
            receipt.at, receipt.material_until_daa, receipt.fault_evidence,
        )).unwrap());
        assert_eq!(borsh::from_slice::<Row>(&bytes).unwrap(), row);
        assert!(receipt.check(&key).is_ok());
        assert!(receipt.check(&Key::CourtBatchNumericalReceipt(h(9))).is_err());
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = Row::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert_eq!(shape.collection_entries, 0);
        let mut unsupported = bytes.clone();
        unsupported[1] = 3;
        for malformed in [&unsupported[..], &bytes[..bytes.len() - 1]] {
            let mut reader = NativeWireReaderV1::new(malformed, malformed.len() as u64);
            assert!(Row::scan_native_wire_v1(&mut reader).is_err());
        }
        let receipt = RootedBatchNumericalReceiptV1 { version: 2, fault_bytes: Some(Box::new(vec![7, 8]).into()), ..receipt };
        let stored = Row::CourtBatchNumericalReceipt(Box::new(receipt.clone()));
        let mut bytes = borsh::to_vec(&stored).unwrap();
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        assert_eq!(Row::scan_native_wire_v1(&mut reader).unwrap().byte_entries, 2);
        assert_eq!(reader.remaining(), 0);
        assert_eq!(borsh::from_slice::<Row>(&bytes).unwrap(), stored);
        let length = bytes.len() - 2 - size_of::<u32>();
        bytes[length..length + size_of::<u32>()].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        assert!(Row::scan_native_wire_v1(&mut reader).is_err());
        assert!(borsh::from_slice::<Row>(&bytes).is_err());
        assert!(RootedBatchNumericalReceiptV1 { fault_bytes: None, ..receipt }.check(&key).is_err());
    }

    #[test]
    fn batch_stop_receipt_has_a_fixed_bounded_row_and_original_key() {
        let h = Hash64::from_u64_word;
        let receipt = RootedBatchStopReceiptV1 {
            version: 1,
            claim: h(1),
            source_id: h(2),
            operands_digest: h(3),
            at: PalwBlockContextV2 { block: h(4), daa_score: 4_000, blue_score: 5_000, subsidy: 0 },
            material_until_daa: 11_000,
            fault_position: Some(2),
            operands: None,
        };
        let key = Key::CourtBatchStopReceipt(receipt.claim);
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 33);
        assert_eq!(borsh::to_vec(&Row::CourtBatchStopReceipt(Box::new(receipt.clone()))).unwrap()[0], 40);
        let bytes = borsh::to_vec(&receipt).unwrap();
        assert_eq!(bytes, borsh::to_vec(&(
            1u16, receipt.claim, receipt.source_id, receipt.operands_digest, receipt.at,
            receipt.material_until_daa, receipt.fault_position,
        )).unwrap());
        let restored: RootedBatchStopReceiptV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(restored, receipt);
        assert!(restored.check(&key).is_ok());
        assert!(restored.check(&Key::CourtBatchStopReceipt(h(5))).is_err());
        assert!(RootedBatchStopReceiptV1 { material_until_daa: receipt.at.daa_score, ..receipt }.check(&key).is_err());
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = RootedBatchStopReceiptV1::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert_eq!(shape.collection_entries, 0);
        let truncated = &bytes[..bytes.len() - 1];
        let mut reader = NativeWireReaderV1::new(truncated, truncated.len() as u64);
        assert!(RootedBatchStopReceiptV1::scan_native_wire_v1(&mut reader).is_err());
    }

    #[test]
    fn rooted_batch_stop_preserves_bounded_signed_operands() {
        use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::SignedConstraintStopIntentV1;
        use crate::palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1;
        use crate::palw_freeprompt_v3::PalwFpStopReasonV3;
        use crate::palw_state_v2::PalwBondKeyV2;
        use crate::palw_step_leg::PalwStepOpeningV1;
        use crate::tx::TransactionOutpoint;

        let h = Hash64::from_u64_word;
        let intent = SignedConstraintStopIntentV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            policy_id: h(3),
            semantics_id: h(4),
            material_id: h(5),
            position: 3,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            filer: PalwBondKeyV2(TransactionOutpoint::new(h(6), 0)),
            signature: vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let eog = PalwTokenTableEogOpeningV1 {
            ids: vec![8],
            opening: PalwStepOpeningV1 { leaf_index: 9, leaf_hash: h(10), siblings: vec![] },
        };
        let operands = borsh::to_vec(&(&intent, &eog, Vec::<u8>::new())).unwrap();
        let receipt = RootedBatchStopReceiptV1 {
            version: 2,
            claim: intent.claim,
            source_id: h(11),
            operands_digest: RootedBatchStopReceiptV1::digest_operands(&operands).unwrap(),
            at: PalwBlockContextV2 { block: h(12), daa_score: 4_000, blue_score: 5_000, subsidy: 0 },
            material_until_daa: 11_000,
            fault_position: Some(2),
            operands: Some(Box::new(BatchStopControlV1 { operands }).into()),
        };
        let key = Key::CourtBatchStopReceipt(receipt.claim);
        let bytes = borsh::to_vec(&receipt).unwrap();
        let restored: RootedBatchStopReceiptV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(restored, receipt);
        assert!(restored.check(&key).is_ok());
        let cloned = restored.clone();
        assert!(std::ptr::eq(cloned.operands.as_ref().unwrap().as_ref(), restored.operands.as_ref().unwrap().as_ref()));

        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = RootedBatchStopReceiptV1::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        let operand_len = receipt.operands.as_ref().unwrap().operands.len();
        assert_eq!(shape.byte_entries, operand_len as u64);
        assert!(shape.heap >= operand_len as u64);
        let length_offset = bytes.len() - operand_len - 4;
        let mut oversized = bytes.clone();
        oversized[length_offset..length_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut reader = NativeWireReaderV1::new(oversized.as_slice(), oversized.len() as u64);
        assert!(RootedBatchStopReceiptV1::scan_native_wire_v1(&mut reader).is_err());
        assert!(borsh::from_slice::<RootedBatchStopReceiptV1>(&oversized).is_err());
        assert!(borsh::from_slice::<RootedBatchStopReceiptV1>(&bytes[..bytes.len() - 1]).is_err());

        let mut altered = restored;
        altered.operands.as_mut().unwrap().operands[2 + 64] ^= 1;
        assert!(altered.check(&key).is_err());
    }

    #[test]
    fn rooted_batch_request_round_trip_binds_key_and_bounded_wire() {
        let h = Hash64::from_u64_word;
        let mut demand = BatchDemandV1 {
            case_id: batch_case_id_v1(h(1), h(2), h(3), h(4), 20, 0).unwrap(),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 10,
                challenge_opened_daa: 20,
                collect_until_daa: 1_000,
                material_until_daa: 11_000,
            },
            opened_daa: 30,
            response_deadline_daa: 40,
            maximum_bytes: 100,
            part_bytes: 16,
            ranges: vec![BatchRangeV1 { id: h(5), bytes: 99, authentication: BatchRangeAuthenticationV1::WholeRange }],
        };
        let row = RootedBatchDemandV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            ordinal: 7,
            notice_id: h(3),
            opened_block: h(6),
            request_root: demand.checked_request_root_v1().unwrap(),
            demand: demand.clone(),
        };
        assert_ne!(demand.case_id, batch_case_id_v1(h(1), h(2), h(3), h(9), 20, 0).unwrap(),);
        assert_ne!(demand.case_id, batch_case_id_v1(h(1), h(2), h(3), h(4), 20, 1).unwrap());
        let key = row.key();
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 32);
        assert_eq!(borsh::to_vec(&Row::CourtBatchDemand(Box::new(row.clone()))).unwrap()[0], 39);
        let body = borsh::to_vec(&row).unwrap();
        let restored: RootedBatchDemandV1 = borsh::from_slice(&body).unwrap();
        assert_eq!(restored, row);
        assert!(restored.check(&key).is_ok());
        assert!(restored.check(&Key::CourtBatchDemand(row.claim, 8)).is_err());
        demand.ranges[0].bytes += 1;
        assert!(RootedBatchDemandV1 { demand, ..restored }.check(&key).is_err());
        let mut reader = NativeWireReaderV1::new(body.as_slice(), body.len() as u64);
        let shape = RootedBatchDemandV1::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert_eq!(shape.collection_entries, 1);
        let mut malformed = body.clone();
        // One encoded range occupies 64 bytes for the id, four for its
        // length and one for its authentication mode.
        let count_at = malformed.len() - (64 + 4 + 1) - 4;
        malformed[count_at..count_at + 4].copy_from_slice(&((MAX_BATCH_RANGES_V1 + 1) as u32).to_le_bytes());
        assert!(borsh::from_slice::<RootedBatchDemandV1>(&malformed).is_err());
        let mut reader = NativeWireReaderV1::new(malformed.as_slice(), malformed.len() as u64);
        assert!(RootedBatchDemandV1::scan_native_wire_v1(&mut reader).is_err());
    }
}
