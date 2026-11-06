//! Fresh GLOBAL Case/source projection for common-journal local replay.
//!
//! This helper must run inside the owner's historical-step guard after native
//! acceptance, complete-journal and canonical-preimage authentication. It uses
//! the actual source engine/G14 opening checks, never saved usage/union/paid
//! flags. It creates no native duty, admission, funding or credit capability.
//! All source maps, fee intents, geometry and temporary native-policy buffers
//! belong in the caller's native replay quote. The fee-intent BTreeMap has at
//! most SourceLimitsV2::max_responses entries, one per real first receipt; quote
//! its keys, values and tree allocation overhead simultaneously with the source
//! ledger's own receipts/reservations, not instead of them.
//! Qualified native settlement and
//! receipt-refresh capabilities are not supplied; kind1 is typed unavailable.
use crate::{
    palw_positive_common_replay_v2::{ExternalReplayEffectV2, LocalCourtSnapshotV2},
    palw_positive_court_v2::coordinator::native_body::{PositiveNativeBodyV2, PositiveNativeOperationV2},
    palw_positive_source_geometry_v2::TirSourceGeometryV2,
    palw_positive_source_v2::{
        NativeSourceContextV2, NativeSourceLedgerV2, SourceErrorV2, SourceFeeIntentV2, SourceLimitsV2, SourceRequestEventV2,
        SourceResponseEventV2, history::NativeSourceHistoryV2,
    },
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, ResultId},
    journal::{CaseJournalEventKind, CaseJournalRecordView},
    messages::{ClaimStatus, CoverageDescriptor, MinerDataStatus, MinerResultCommit, MinerServiceContract, SourceKind, WitnessStatus},
    reservations::RemainingCapacity,
    rows::{AcceptedMaterial, AcceptedOrigin, AggregateUsage, CaseRow, ServiceReceiptRow, SourceUnionRow},
};
use std::collections::BTreeMap;

type UnitKey = (SourceKind, Digest, u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommonSourceReplayErrorV2 {
    Source(SourceErrorV2),
    Contract(ContractError),
    Binding,
    Order,
    Budget,
    NoOp,
    Poisoned,
    NativePolicy,
    MissingSettlementPort,
}
impl From<SourceErrorV2> for CommonSourceReplayErrorV2 {
    fn from(error: SourceErrorV2) -> Self {
        Self::Source(error)
    }
}
impl From<ContractError> for CommonSourceReplayErrorV2 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// Only independently reconstructed admission metadata, never a saved CaseRow
/// or AggregateUsage. Coverage comes from the actual A1-derived plan; reservation
/// and liability/maturity come from the real historical native admission view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreshSourceCaseBasisV2 {
    pub coverage: CoverageDescriptor,
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    pub maturity_daa: u64,
    pub origin: AcceptedOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtUsageDeltaV2 {
    pub signed_wire_bytes: u64,
    pub attempts: u32,
    pub elements: u64,
    pub terms: u64,
}
impl CourtUsageDeltaV2 {
    pub fn between(before: Option<LocalCourtSnapshotV2>, after: LocalCourtSnapshotV2) -> Result<Self, CommonSourceReplayErrorV2> {
        let (wire, attempts, elements, terms) =
            before.map_or((0, 0, 0, 0), |b| (b.charged_wire_bytes, b.attempts, b.charged_elements, b.charged_terms));
        Ok(Self {
            signed_wire_bytes: after.charged_wire_bytes.checked_sub(wire).ok_or(CommonSourceReplayErrorV2::Binding)?,
            attempts: after.attempts.checked_sub(attempts).ok_or(CommonSourceReplayErrorV2::Binding)?,
            elements: after.charged_elements.checked_sub(elements).ok_or(CommonSourceReplayErrorV2::Binding)?,
            terms: after.charged_terms.checked_sub(terms).ok_or(CommonSourceReplayErrorV2::Binding)?,
        })
    }
}

/// EXCLUSIVE additional charges, not cumulative counters. Court signed-wire
/// bytes/attempts and the complete `source_delta` supplied to the mapper are
/// already accounted for and MUST NOT be included again. No successful Default
/// or automatic elements/terms-to-work conversion is supplied.
/// AggregateUsage.fee is source-service-only: only the real source ledger's
/// first receipt increases it. Court/control/Panel fees require their separate
/// qualified owner and are not mapped into this source-service budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeUsageChargeV2 {
    pub extra_transmitted_bytes: u64,
    pub additional_work: u64,
    /// Actual canonical retained bytes under native policy. Court receipt Vec
    /// capacity is workspace and must NEVER be treated as canonical retention.
    pub retained_bytes: u64,
}
pub trait NativeCommonUsagePolicyV2 {
    /// Required rooted policy/work mapper, re-resolved for this historical
    /// guarded step. Returning a rate from test data is not native authority.
    fn map_usage_delta(
        &self,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        court: CourtUsageDeltaV2,
        source_delta: AggregateUsage,
    ) -> Result<NativeUsageChargeV2, CommonSourceReplayErrorV2>;

    /// Required protection for the proposed post-event Case and actual replayed
    /// source ledger. Resolve ALL unfinished source/other-role reservations in
    /// this existing projection, plus retention beyond response completion.
    /// The historical native owner must also validate deadline/interval service
    /// opportunity and physical leases; a scalar sum alone cannot do that.
    /// No empty/successful default or guess of unassigned native units exists.
    /// This is readonly: it must not commit live reservations or funding.
    fn remaining_capacity(
        &self,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        source: &NativeSourceLedgerV2,
        proposed: &CaseRow,
    ) -> Result<RemainingCapacity, CommonSourceReplayErrorV2>;
}

#[derive(Clone, Copy)]
pub enum CommonSourceEventV2<'a> {
    None,
    Request(SourceRequestEventV2<'a>),
    Response(SourceResponseEventV2<'a>),
}

/// Private source ledger: all receipt access goes through this helper so a
/// future native-owned settlement refresh cannot be bypassed. No public setter
/// or saved-paid-state constructor exists. A failure poisons this cache: drop
/// it and replay fresh; no partially mutated cache can be republished.
pub struct CommonSourceReplayStateV2 {
    ledger: NativeSourceLedgerV2,
    case: CaseRow,
    geometry: TirSourceGeometryV2,
    last_sequence: u64,
    last_court: Option<LocalCourtSnapshotV2>,
    max_fee_intents: u64,
    fee_intents: BTreeMap<UnitKey, SourceFeeIntentV2>,
    poisoned: bool,
}
impl CommonSourceReplayStateV2 {
    /// Additional complete source shadow allocation bound (excluding geometry,
    /// whose actual independent program/inventory owner is reserved separately).
    pub(crate) fn replay_heap_requests_v2(limits: SourceLimitsV2) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
        use crate::palw_native_resource_v1::{TirNativeRequestsV1, typed::BTreeNodeEnvelopeV1};
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Self>(1)?;
        q.add_v1(NativeSourceLedgerV2::replay_heap_requests_v2(limits)?)?;
        q.exact_v1::<BTreeNodeEnvelopeV1<UnitKey, SourceFeeIntentV2>>(limits.max_responses)?;
        Ok(q.bytes_v1())
    }
    pub fn new(
        result: MinerResultCommit,
        service: MinerServiceContract,
        geometry: TirSourceGeometryV2,
        limits: SourceLimitsV2,
        basis: FreshSourceCaseBasisV2,
    ) -> Result<Self, CommonSourceReplayErrorV2> {
        let id = result.id();
        if geometry.result_id() != id || basis.coverage.scope != result.scope || basis.coverage.result != id {
            return Err(CommonSourceReplayErrorV2::Binding);
        }
        let case = CaseRow {
            case: result.case_id(),
            result: id,
            service,
            coverage: basis.coverage,
            status: ClaimStatus::ResultCommitted,
            miner_data: MinerDataStatus::NotRequested,
            witness_status: WitnessStatus::Building,
            positive_witness: None,
            usage: AggregateUsage::default(),
            reservation: basis.reservation,
            liability: basis.liability,
            maturity_daa: basis.maturity_daa,
            unresolved_fraud: 0,
            last_event: basis.origin,
        };
        let mut ledger = NativeSourceLedgerV2::new(result, service, AggregateUsage::default(), limits)?;
        ledger.bind_case(&case)?;
        Ok(Self {
            ledger,
            case,
            geometry,
            last_sequence: 0,
            last_court: None,
            max_fee_intents: limits.max_responses,
            fee_intents: BTreeMap::new(),
            poisoned: false,
        })
    }
    fn healthy(&self) -> Result<(), CommonSourceReplayErrorV2> {
        if self.poisoned { Err(CommonSourceReplayErrorV2::Poisoned) } else { Ok(()) }
    }
    pub fn case_row(&self) -> Result<CaseRow, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.case)
    }
    pub fn usage(&self) -> Result<AggregateUsage, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.case.usage)
    }
    pub fn history(&self) -> Result<&NativeSourceHistoryV2, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.ledger.history())
    }
    pub fn geometry(&self) -> Result<&TirSourceGeometryV2, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(&self.geometry)
    }
    pub fn result_id(&self) -> Result<ResultId, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.case.result)
    }
    pub fn source_union(&self, kind: SourceKind, source: Digest) -> Result<Option<SourceUnionRow>, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.ledger.source_union(kind, source))
    }
    pub fn receipt(
        &self,
        kind: SourceKind,
        source: Digest,
        unit: u64,
    ) -> Result<Option<ServiceReceiptRow>, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.ledger.receipt(kind, source, unit).copied())
    }

    pub fn fee_intent(
        &self,
        kind: SourceKind,
        source: Digest,
        unit: u64,
    ) -> Result<Option<SourceFeeIntentV2>, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(self.fee_intents.get(&(kind, source, unit)).copied())
    }
    /// Caller already holds the actual historical guard and has charged this
    /// helper's geometry/maps/source-preparation/policy work and peak memory.
    /// Qualified native settlement and receipt-refresh ports are unprovided;
    /// kind1 always returns MissingSettlementPort before any Case charge.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_record<C: NativeSourceContextV2, P: NativeCommonUsagePolicyV2>(
        &mut self,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        before: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        context: &C,
        policy: &P,
        event: CommonSourceEventV2<'_>,
    ) -> Result<ExternalReplayEffectV2, CommonSourceReplayErrorV2> {
        self.healthy()?;
        let result = self.apply_inner(record, body, before, after, context, policy, event, None);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Assigned mode starts with the genuine accepted op9 full transaction.
    /// Its native environment must independently check literal op9 and the
    /// original Assignment/C links; this pure shadow helper creates no receipt.
    pub fn apply_assigned_initial_record_v2<C: NativeSourceContextV2, P: NativeCommonUsagePolicyV2>(
        &mut self,
        record: CaseJournalRecordView<'_>,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        after: LocalCourtSnapshotV2,
        context: &C,
        policy: &P,
    ) -> Result<ExternalReplayEffectV2, CommonSourceReplayErrorV2> {
        self.healthy()?;
        if record.header().kind != CaseJournalEventKind::NativeTransaction || record.header().sequence != 1 {
            return Err(CommonSourceReplayErrorV2::Order);
        }
        let applied = self.apply_inner(record, None, None, after, context, policy, CommonSourceEventV2::None, Some((result, service)));
        if applied.is_err() {
            self.poisoned = true;
        }
        applied
    }
    /// Borrowed actual shadow ledger, for the native owner to protect every
    /// outstanding unit and compare all first-only receipts after full replay.
    pub fn source_ledger_v2(&self) -> Result<&NativeSourceLedgerV2, CommonSourceReplayErrorV2> {
        self.healthy()?;
        Ok(&self.ledger)
    }
    #[allow(clippy::too_many_arguments)]
    fn apply_inner<C: NativeSourceContextV2, P: NativeCommonUsagePolicyV2>(
        &mut self,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        before: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        context: &C,
        policy: &P,
        event: CommonSourceEventV2<'_>,
        assigned_initial: Option<(&MinerResultCommit, &MinerServiceContract)>,
    ) -> Result<ExternalReplayEffectV2, CommonSourceReplayErrorV2> {
        let header = record.header();
        if header.sequence != self.last_sequence.checked_add(1).ok_or(CommonSourceReplayErrorV2::Order)? || before != self.last_court {
            return Err(CommonSourceReplayErrorV2::Order);
        }
        if header.scope != self.case.coverage.scope
            || header.case != self.case.case
            || header.result != self.case.result
            || self.ledger.usage() != self.case.usage
        {
            return Err(CommonSourceReplayErrorV2::Binding);
        }
        check_body(record, body)?;
        if header.kind == CaseJournalEventKind::NativeTransaction && (header.sequence != 1 || assigned_initial.is_none()) {
            return Err(CommonSourceReplayErrorV2::MissingSettlementPort);
        }
        if assigned_initial.is_some() && (header.sequence != 1 || body.is_some()) {
            return Err(CommonSourceReplayErrorV2::Order);
        }
        if header.sequence == 1 {
            let (result, service) = if let Some(assigned) = assigned_initial {
                assigned
            } else if let Some(PositiveNativeBodyV2 {
                operation: PositiveNativeOperationV2::ResultAdmission { result, service, .. },
                ..
            }) = body
            {
                (result, service)
            } else {
                return Err(CommonSourceReplayErrorV2::Order);
            };
            if result != self.ledger.result()
                || *service != self.case.service
                || header.origin != self.case.last_event
                || before.is_some()
                || after.attempts != 0
                || after.charged_wire_bytes != 0
                || after.charged_elements != 0
                || after.charged_terms != 0
                || after.last_attempt.is_some()
            {
                return Err(CommonSourceReplayErrorV2::Binding);
            }
        } else if matches!(body.map(|b| &b.operation), Some(PositiveNativeOperationV2::ResultAdmission { .. })) {
            return Err(CommonSourceReplayErrorV2::Order);
        }
        let court = CourtUsageDeltaV2::between(before, after)?;
        let previous_usage = self.case.usage;
        let mut source_changed = false;
        match (body.map(|b| &b.operation), event) {
            (
                Some(PositiveNativeOperationV2::SourceRequest { request, requester, signature }),
                CommonSourceEventV2::Request(event),
            ) => {
                if before != Some(after)
                    || *request != event.request
                    || *requester != event.requester
                    || *signature != *event.signature
                {
                    return Err(CommonSourceReplayErrorV2::Binding);
                }
                let update = self.ledger.prepare_journal_request_v2(&self.geometry, context, record, event)?;
                if !update.changes_case() {
                    return Err(CommonSourceReplayErrorV2::NoOp);
                }
                let case = *update.case_row().ok_or(CommonSourceReplayErrorV2::Binding)?;
                self.ledger.validate_prepared(&update)?;
                self.ledger.apply(update)?;
                self.case = case;
                source_changed = true;
            }
            (Some(PositiveNativeOperationV2::SourceResponse { case, request, .. }), CommonSourceEventV2::Response(event)) => {
                if before != Some(after) || *case != self.case.case || *request != event.request {
                    return Err(CommonSourceReplayErrorV2::Binding);
                }
                let update = self.ledger.prepare_journal_response_v2(&self.geometry, context, record, event)?;
                if !update.changes_case() {
                    return Err(CommonSourceReplayErrorV2::NoOp);
                }
                let case = *update.case_row().ok_or(CommonSourceReplayErrorV2::Binding)?;
                let intent = update.fee_intent().copied().ok_or(CommonSourceReplayErrorV2::Binding)?;
                let key = update.touched_unit().ok_or(CommonSourceReplayErrorV2::Binding)?;
                if self.fee_intents.contains_key(&key) {
                    return Err(CommonSourceReplayErrorV2::Binding);
                }
                // The real ledger independently caps first receipts at the
                // same max_responses. Bound our additional map before apply
                // and insertion as well; it has its own simultaneous quote.
                if self.fee_intents.len() as u64 >= self.max_fee_intents {
                    return Err(CommonSourceReplayErrorV2::Budget);
                }
                self.ledger.validate_prepared(&update)?;
                self.ledger.apply(update)?;
                self.fee_intents.insert(key, intent);
                self.case = case;
                source_changed = true;
            }
            (Some(PositiveNativeOperationV2::SourceRequest { .. } | PositiveNativeOperationV2::SourceResponse { .. }), _)
            | (_, CommonSourceEventV2::Request(_) | CommonSourceEventV2::Response(_)) => {
                return Err(CommonSourceReplayErrorV2::Binding);
            }
            (_, CommonSourceEventV2::None) => {}
        }
        let source_delta = usage_delta(previous_usage, self.case.usage)?;
        let charge = policy.map_usage_delta(record, body, court, source_delta)?;
        let usage = add_usage(self.case.usage, court, charge)?;
        usage.validate(&self.case.service.budget)?;
        let mut next = self.case;
        next.usage = usage;
        next.status = after.status;
        next.witness_status = after.last_attempt.map_or(WitnessStatus::Building, |a| a.status);
        next.positive_witness = if after.status == ClaimStatus::PositivePassed {
            Some(after.last_attempt.ok_or(CommonSourceReplayErrorV2::Binding)?.witness)
        } else {
            None
        };
        if usage.witness_attempts != after.attempts {
            return Err(CommonSourceReplayErrorV2::Binding);
        }
        let changed = source_changed || next != self.case;
        if changed {
            next.last_event = header.origin;
        }
        // A currently fitting Court charge must not consume the capacity still
        // owed to accepted source duties. Validate the same proposed transition
        // after source reservation-to-consumption transfer, before publishing
        // any Case/cursor. Any failure poisons the whole local projection; it is
        // neither a Miner default nor a change to an accepted Complete receipt.
        policy.remaining_capacity(record, body, &self.ledger, &next)?.check(&next.usage, &next.service.budget)?;
        self.ledger.refresh_case_usage(&next)?;
        self.case = next;
        self.last_sequence = header.sequence;
        self.last_court = Some(after);
        Ok(if changed { ExternalReplayEffectV2::Changed } else { ExternalReplayEffectV2::NoOp })
    }
}

fn check_body(record: CaseJournalRecordView<'_>, body: Option<&PositiveNativeBodyV2>) -> Result<(), CommonSourceReplayErrorV2> {
    match (record.header().kind, body) {
        (CaseJournalEventKind::NativeTransaction, None) => Ok(()),
        (CaseJournalEventKind::NativeBody, Some(body)) => {
            let operation = match &body.operation {
                PositiveNativeOperationV2::ResultAdmission { .. } => 0,
                PositiveNativeOperationV2::WitnessCommit { .. } => 1,
                PositiveNativeOperationV2::ExactFrame { .. } => 2,
                PositiveNativeOperationV2::SourceRequest { .. } => 3,
                PositiveNativeOperationV2::SourceResponse { .. } => 4,
                PositiveNativeOperationV2::Finish { .. } => 5,
                PositiveNativeOperationV2::Expire { .. } => 6,
                PositiveNativeOperationV2::PositiveCourtAttestation { .. } => 7,
            };
            if body.version != CONTRACT_VERSION || body.revision != CONTRACT_REVISION || record.payload().get(4) != Some(&operation) {
                return Err(CommonSourceReplayErrorV2::Binding);
            }
            Ok(())
        }
        _ => Err(CommonSourceReplayErrorV2::Binding),
    }
}
fn usage_delta(old: AggregateUsage, next: AggregateUsage) -> Result<AggregateUsage, CommonSourceReplayErrorV2> {
    let sub = |a: u64, b: u64| a.checked_sub(b).ok_or(CommonSourceReplayErrorV2::Binding);
    Ok(AggregateUsage {
        rounds: sub(next.rounds, old.rounds)?,
        unique_source_bytes: sub(next.unique_source_bytes, old.unique_source_bytes)?,
        transmitted_bytes: sub(next.transmitted_bytes, old.transmitted_bytes)?,
        additional_work: sub(next.additional_work, old.additional_work)?,
        retained_bytes: sub(next.retained_bytes, old.retained_bytes)?,
        fee: sub(next.fee, old.fee)?,
        witness_attempts: next.witness_attempts.checked_sub(old.witness_attempts).ok_or(CommonSourceReplayErrorV2::Binding)?,
    })
}
fn add_usage(
    mut usage: AggregateUsage,
    court: CourtUsageDeltaV2,
    charge: NativeUsageChargeV2,
) -> Result<AggregateUsage, CommonSourceReplayErrorV2> {
    let add = |a: u64, b: u64| a.checked_add(b).ok_or(CommonSourceReplayErrorV2::Budget);
    usage.transmitted_bytes = add(add(usage.transmitted_bytes, court.signed_wire_bytes)?, charge.extra_transmitted_bytes)?;
    usage.witness_attempts = usage.witness_attempts.checked_add(court.attempts).ok_or(CommonSourceReplayErrorV2::Budget)?;
    usage.additional_work = add(usage.additional_work, charge.additional_work)?;
    usage.retained_bytes = add(usage.retained_bytes, charge.retained_bytes)?;
    Ok(usage)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn court(wire: u64, attempts: u32) -> LocalCourtSnapshotV2 {
        LocalCourtSnapshotV2 {
            status: ClaimStatus::Pending,
            attempts,
            cursor: 0,
            last_attempt: None,
            charged_wire_bytes: wire,
            charged_elements: 13,
            charged_terms: 17,
            reserved_receipt_bytes: 4096,
        }
    }
    #[test]
    fn only_delta_is_added_and_receipt_capacity_is_not_retention() {
        let delta = CourtUsageDeltaV2::between(Some(court(100, 1)), court(140, 2)).unwrap();
        let usage = add_usage(
            AggregateUsage { transmitted_bytes: 900, witness_attempts: 1, fee: 3, ..AggregateUsage::default() },
            delta,
            NativeUsageChargeV2 { extra_transmitted_bytes: 7, additional_work: 5, retained_bytes: 11 },
        )
        .unwrap();
        assert_eq!(usage.transmitted_bytes, 947);
        assert_eq!(usage.witness_attempts, 2);
        assert_eq!(usage.retained_bytes, 11);
        assert_eq!(usage.additional_work, 5);
        assert_eq!(usage.fee, 3);
        // Another Court/control step adds its own wire/work/retention, while
        // the existing first-source-receipt due remains unchanged.
        let next = add_usage(
            usage,
            CourtUsageDeltaV2 { signed_wire_bytes: 13, attempts: 0, elements: 2, terms: 4 },
            NativeUsageChargeV2 { extra_transmitted_bytes: 2, additional_work: 9, retained_bytes: 17 },
        )
        .unwrap();
        assert_eq!(next.fee, 3);
        assert_eq!(next.transmitted_bytes, 962);
        assert_eq!(next.additional_work, 14);
        assert_eq!(next.retained_bytes, 28);
        assert_eq!(delta.elements, 0);
        assert_eq!(delta.terms, 0);
    }
    #[test]
    fn counter_rewind_and_overflow_are_refused() {
        assert!(CourtUsageDeltaV2::between(Some(court(140, 2)), court(100, 1)).is_err());
        assert!(usage_delta(AggregateUsage { fee: 3, ..AggregateUsage::default() }, AggregateUsage::default()).is_err());
        assert!(
            add_usage(
                AggregateUsage { transmitted_bytes: u64::MAX, ..AggregateUsage::default() },
                CourtUsageDeltaV2 { signed_wire_bytes: 1, attempts: 0, elements: 0, terms: 0 },
                NativeUsageChargeV2 { extra_transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 },
            )
            .is_err()
        );
    }
}
