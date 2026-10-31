//! Pure, bounded preparation/replay of accepted Positive source events.
//!
//! This module cannot install native rows, move money, create a duty, or grant
//! credit. Public events/rows are DTOs. A4 must resolve actual current roles,
//! funding, reservations and accepted history, then re-prepare/recheck through
//! its private TransitionBuilder before applying rows to native state.
use crate::palw_positive_source_geometry_v2::{PositiveSourceGeometryErrorV2, TirSourceGeometryV2};
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::*,
    rows::{AcceptedMaterial, AggregateUsage, CaseRow, NativeRowBody, ServiceReceiptRow, SourceUnionRow},
};
use std::{collections::BTreeMap, sync::Arc};
#[path = "palw_positive_source_v2_history.rs"]
pub mod history;
use history::{NativeSourceHistoryV2, SourceHistoryEntryV2, SourceHistoryKindV2};

type SourceKey = (SourceKind, Digest);
type UnitKey = (SourceKind, Digest, u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMissingPortV2 {
    AcceptedHistory,
    CanonicalHistory,
    DrawnRole,
    LiveFunding,
    Reservation,
    FeeSchedule,
    NativeApply,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceErrorV2 {
    Contract(ContractError),
    Geometry(PositiveSourceGeometryErrorV2),
    MissingPort(SourceMissingPortV2),
    Binding,
    Budget,
    Deadline,
    Order,
    MissingRequest,
    History,
    StalePrepared,
}
impl From<ContractError> for SourceErrorV2 {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<PositiveSourceGeometryErrorV2> for SourceErrorV2 {
    fn from(e: PositiveSourceGeometryErrorV2) -> Self {
        Self::Geometry(e)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLimitsV2 {
    pub max_requests: u64,
    pub max_responses: u64,
    pub max_history_events: u64,
    pub max_history_bytes: u64,
    pub max_intervals: u64,
    pub max_units_per_request: u64,
    pub max_native_body_bytes: u64,
}
#[derive(Clone, Copy)]
pub struct SourceRequestEventV2<'a> {
    pub request: SourceRequest,
    pub requester: PrincipalId,
    /// The actual rooted role key, not an additional wire field.
    pub key: &'a [u8; PUBKEY_BYTES],
    pub signature: &'a [u8; SIGNATURE_BYTES],
    pub material: AcceptedMaterial,
    pub native_body: &'a [u8],
}
#[derive(Clone, Copy)]
pub struct SourceResponseEventV2<'a> {
    pub request: RequestId,
    /// Actual accepted carrier/delegation identity; op4 has no provider field.
    pub provider: PrincipalId,
    pub material: AcceptedMaterial,
    pub native_body: &'a [u8],
    pub g14_data_range: (u64, u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceEventCostV2 {
    pub additional_work: u64,
    /// Separate actual carrier reservation quote; not counted as body bytes.
    pub outer_carrier_bytes: u64,
    pub cutoff: u64,
    /// Immutable unit service price for op4; zero for op3. Due is first-only.
    pub service_fee: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceServiceFeeV2 {
    pub amount: u64,
    pub schedule: Digest,
    pub funding_scope: FundingScopeId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFeeIntentV2 {
    pub key: misaka_palw_positive_contract::native::NativeRowKey,
    pub result: ResultId,
    pub funding_scope: FundingScopeId,
    pub schedule: Digest,
    pub provider: PrincipalId,
    pub amount: u64,
    pub accepted_response: AcceptedMaterial,
}
#[derive(Clone, Copy)]
struct FeeReservation {
    quote: SourceServiceFeeV2,
}

/// All methods must resolve actual accepted-prefix/current-view facts. A peer
/// implementation or a successful test context is not native authority.
/// Missing native ports return a typed MissingPort error, never a success flag.
pub trait NativeSourceContextV2 {
    /// Resolve a per-case immutable schedule from actual native policy/funding
    /// state. Public prices are not authority. No successful default is supplied.
    fn service_fee(
        &self,
        _result: &MinerResultCommit,
        _service: &MinerServiceContract,
        _kind: SourceKind,
        _source: Digest,
        _unit: u64,
        _data_bytes: u64,
    ) -> Result<SourceServiceFeeV2, SourceErrorV2> {
        Err(SourceErrorV2::MissingPort(SourceMissingPortV2::FeeSchedule))
    }
    fn history_complete(
        &self,
        result: &MinerResultCommit,
        history: &NativeSourceHistoryV2,
        next: &AcceptedMaterial,
    ) -> Result<(), SourceErrorV2>;
    fn request(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        event: &SourceRequestEventV2<'_>,
    ) -> Result<SourceEventCostV2, SourceErrorV2>;
    fn response(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        event: &SourceResponseEventV2<'_>,
    ) -> Result<SourceEventCostV2, SourceErrorV2>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedSourceRequestV2 {
    pub request: SourceRequest,
    pub requester: PrincipalId,
    pub material: AcceptedMaterial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceEffectV2 {
    Requested,
    Coalesced,
    Responded,
    AlreadyComplete,
    AlreadyApplied,
}
struct SourceState {
    units: u64,
    intervals: Vec<(u64, u64)>,
    completed: u64,
    union: AcceptedMaterial,
}
/// The seal binds updates to this exact cache instance, not merely matching DTO
/// revision numbers. It is not serializable or a native-acceptance capability.
pub struct NativeSourceLedgerV2 {
    result: MinerResultCommit,
    service: MinerServiceContract,
    limits: SourceLimitsV2,
    usage: AggregateUsage,
    revision: u64,
    seal: Arc<()>,
    sources: BTreeMap<SourceKey, SourceState>,
    requests: BTreeMap<RequestId, RetainedSourceRequestV2>,
    receipts: BTreeMap<UnitKey, ServiceReceiptRow>,
    history: NativeSourceHistoryV2,
    reservations: BTreeMap<UnitKey, FeeReservation>,
    pending_fee: u64,
    case_basis: Option<CaseRow>,
}
/// Proposed row/usage data only. Native apply must use the actual current view,
/// accepted origins and live service/funding/reservation facts again.
pub struct PreparedSourceUpdateV2 {
    seal: Arc<()>,
    revision: u64,
    expected_usage: AggregateUsage,
    result: ResultId,
    effect: SourceEffectV2,
    usage: AggregateUsage,
    union: Option<SourceUnionRow>,
    receipt: Option<ServiceReceiptRow>,
    retained: Option<SourceHistoryEntryV2>,
    change: SourceChange,
    fee_intent: Option<SourceFeeIntentV2>,
    expected_case: Option<CaseRow>,
    case_row: Option<CaseRow>,
    next_miner_data: MinerDataStatus,
    touched_source: SourceKey,
    touched_unit: Option<UnitKey>,
}
enum SourceChange {
    None,
    Request { record: Option<RetainedSourceRequestV2>, intervals: Vec<(u64, u64)>, reservations: Vec<(UnitKey, FeeReservation)> },
    Response,
}

impl NativeSourceLedgerV2 {
    pub fn new(
        result: MinerResultCommit,
        service: MinerServiceContract,
        initial_usage: AggregateUsage,
        limits: SourceLimitsV2,
    ) -> Result<Self, SourceErrorV2> {
        result.validate_shape()?;
        service.validate(&result, 0)?;
        initial_usage.validate(&service.budget)?;
        if [
            limits.max_requests,
            limits.max_responses,
            limits.max_history_events,
            limits.max_history_bytes,
            limits.max_intervals,
            limits.max_units_per_request,
            limits.max_native_body_bytes,
        ]
        .contains(&0)
            || limits.max_native_body_bytes > usize::MAX as u64
            || limits.max_history_bytes > usize::MAX as u64
        {
            return Err(SourceErrorV2::Budget);
        }
        // Nonzero source summaries cannot create an interval, receipt, or quota
        // discount. Restore by replaying actual canonical history from admission.
        if initial_usage.rounds != 0
            || initial_usage.unique_source_bytes != 0
            || initial_usage.transmitted_bytes != 0
            || initial_usage.additional_work != 0
            || initial_usage.retained_bytes != 0
            || initial_usage.fee != 0
        {
            return Err(SourceErrorV2::MissingPort(SourceMissingPortV2::CanonicalHistory));
        }
        Ok(Self {
            result,
            service,
            limits,
            usage: initial_usage,
            revision: 0,
            seal: Arc::new(()),
            sources: BTreeMap::new(),
            requests: BTreeMap::new(),
            receipts: BTreeMap::new(),
            history: NativeSourceHistoryV2::default(),
            reservations: BTreeMap::new(),
            pending_fee: 0,
            case_basis: None,
        })
    }
    pub fn usage(&self) -> AggregateUsage {
        self.usage
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn service(&self) -> &MinerServiceContract {
        &self.service
    }
    pub fn history(&self) -> &NativeSourceHistoryV2 {
        &self.history
    }
    pub fn history_events(&self) -> &[SourceHistoryEntryV2] {
        self.history.entries()
    }
    pub fn request(&self, id: RequestId) -> Option<&RetainedSourceRequestV2> {
        self.requests.get(&id)
    }
    pub fn receipt(&self, kind: SourceKind, source: Digest, unit: u64) -> Option<&ServiceReceiptRow> {
        self.receipts.get(&(kind, source, unit))
    }
    pub fn status(&self, kind: SourceKind, source: Digest, unit: u64) -> MinerDataStatus {
        if self.receipts.contains_key(&(kind, source, unit)) {
            return MinerDataStatus::Complete;
        }
        if self.sources.get(&(kind, source)).is_some_and(|state| contains(&state.intervals, unit)) {
            MinerDataStatus::Pending
        } else {
            MinerDataStatus::NotRequested
        }
    }
    pub fn miner_data_status(&self) -> MinerDataStatus {
        if self.sources.is_empty() {
            MinerDataStatus::NotRequested
        } else if self.sources.values().all(|source| interval_units(&source.intervals).ok() == Some(source.completed)) {
            MinerDataStatus::Complete
        } else {
            MinerDataStatus::Pending
        }
    }
    pub fn source_union(&self, kind: SourceKind, source: Digest) -> Option<SourceUnionRow> {
        self.sources.get(&(kind, source)).map(|state| SourceUnionRow {
            case: self.result.case_id(),
            kind,
            source,
            source_units: state.units,
            requested_units: interval_units(&state.intervals).expect("validated interval union"),
            completed_units: state.completed,
            union: state.union,
        })
    }
    pub fn prepare_request(
        &self,
        geometry: &TirSourceGeometryV2,
        context: &impl NativeSourceContextV2,
        event: SourceRequestEventV2<'_>,
    ) -> Result<PreparedSourceUpdateV2, SourceErrorV2> {
        self.check_geometry(geometry)?;
        if event.native_body.len() as u64 > self.limits.max_native_body_bytes
            || event.request.end.checked_sub(event.request.first).filter(|n| *n <= self.limits.max_units_per_request).is_none()
        {
            return Err(SourceErrorV2::Budget);
        }
        geometry.validate_request(&event.request)?;
        history::check_request_literal(&event)?;
        if principal(event.key) != event.requester {
            return Err(SourceErrorV2::Binding);
        }
        event.request.authenticate(geometry.source_units(event.request.kind), event.key, event.signature)?;
        context.history_complete(&self.result, &self.history, &event.material)?;
        let cost = context.request(&self.result, &self.service, &event)?;
        self.check_cost(cost, event.material)?;
        if cost.service_fee != 0 {
            return Err(SourceErrorV2::Binding);
        }
        let retained = self.history.prepare(event.material, SourceHistoryKindV2::Request, event.native_body, self.limits)?;
        if retained.is_none() {
            return Ok(self.prepared(
                SourceEffectV2::AlreadyApplied,
                self.usage,
                None,
                None,
                None,
                SourceChange::None,
                (event.request.kind, event.request.source),
                None,
            ));
        }
        let id = event.request.id();
        let existing_record = self.requests.get(&id);
        if existing_record.is_some_and(|old| old.request != event.request) {
            return Err(SourceErrorV2::Binding);
        }
        if existing_record.is_none() && self.requests.len() as u64 >= self.limits.max_requests {
            return Err(SourceErrorV2::Budget);
        }
        let key = (event.request.kind, event.request.source);
        let old = self.sources.get(&key);
        let empty = Vec::new();
        let previous = old.map_or(&empty, |state| &state.intervals);
        let intervals = merge(previous, (event.request.first, event.request.end));
        let total_intervals = self
            .sources
            .values()
            .try_fold(0u64, |n, state| n.checked_add(state.intervals.len() as u64).ok_or(SourceErrorV2::Budget))?
            .checked_sub(previous.len() as u64)
            .and_then(|n| n.checked_add(intervals.len() as u64))
            .ok_or(SourceErrorV2::Budget)?;
        if total_intervals > self.limits.max_intervals {
            return Err(SourceErrorV2::Budget);
        }
        let mut raw = 0u64;
        let mut fresh = 0u64;
        let mut reservations = Vec::new();
        let mut pending_due = self.pending_fee;
        if add(self.usage.fee, pending_due)? > self.service.budget.fee {
            return Err(SourceErrorV2::Budget);
        }
        for unit in event.request.first..event.request.end {
            if !contains(previous, unit) {
                fresh = add(fresh, 1)?;
                if add(self.reservations.len() as u64, fresh)? > self.limits.max_responses {
                    return Err(SourceErrorV2::Budget);
                }
                let data_bytes = geometry.unit_bound(event.request.kind, unit)?.max_data_bytes;
                raw = add(raw, data_bytes)?;
                let quote =
                    context.service_fee(&self.result, &self.service, event.request.kind, event.request.source, unit, data_bytes)?;
                pending_due = add(pending_due, quote.amount)?;
                if add(self.usage.fee, pending_due)? > self.service.budget.fee {
                    return Err(SourceErrorV2::Budget);
                }
                reservations.push(((event.request.kind, event.request.source, unit), FeeReservation { quote }));
            }
        }
        let mut usage = self.event_usage(cost, event.native_body.len() as u64)?;
        usage.rounds = add(usage.rounds, u64::from(fresh > 0))?;
        usage.unique_source_bytes = add(usage.unique_source_bytes, raw)?;
        usage.validate(&self.service.budget)?;
        let union = SourceUnionRow {
            case: self.result.case_id(),
            kind: event.request.kind,
            source: event.request.source,
            source_units: geometry.source_units(event.request.kind),
            requested_units: interval_units(&intervals)?,
            completed_units: old.map_or(0, |state| state.completed),
            union: event.material,
        };
        let record = existing_record.is_none().then_some(RetainedSourceRequestV2 {
            request: event.request,
            requester: event.requester,
            material: event.material,
        });
        Ok(self.prepared(
            if fresh > 0 { SourceEffectV2::Requested } else { SourceEffectV2::Coalesced },
            usage,
            Some(union),
            None,
            retained,
            SourceChange::Request { record, intervals, reservations },
            key,
            None,
        ))
    }
    pub fn prepare_response(
        &self,
        geometry: &TirSourceGeometryV2,
        context: &impl NativeSourceContextV2,
        event: SourceResponseEventV2<'_>,
    ) -> Result<PreparedSourceUpdateV2, SourceErrorV2> {
        self.check_geometry(geometry)?;
        if event.native_body.len() as u64 > self.limits.max_native_body_bytes {
            return Err(SourceErrorV2::Budget);
        }
        let request = self.requests.get(&event.request).ok_or(SourceErrorV2::MissingRequest)?;
        let raw = history::response_data(self.result.case_id(), &event)?;
        geometry.validate_request(&request.request)?;
        // Do not accept an arbitrary context callback as arithmetic/data truth:
        // decode/authenticate the actual existing G14 opening in this engine.
        let (unit, data) = geometry.decode_g14_response_v2(&request.request, raw)?;
        let facts = geometry.unit_bound(request.request.kind, unit)?;
        let (data_bytes, body_bytes) = data.sizes()?;
        if data_bytes != facts.max_data_bytes || body_bytes != raw.len() as u64 {
            return Err(SourceErrorV2::Binding);
        }
        let key = (request.request.kind, request.request.source, unit);
        let source = self.sources.get(&(key.0, key.1)).ok_or(SourceErrorV2::MissingRequest)?;
        if !contains(&source.intervals, unit) {
            return Err(SourceErrorV2::MissingRequest);
        }
        context.history_complete(&self.result, &self.history, &event.material)?;
        let cost = context.response(&self.result, &self.service, &event)?;
        self.check_cost(cost, event.material)?;
        let reserved = self.reservations.get(&key).ok_or(SourceErrorV2::History)?;
        let actual_quote = context.service_fee(&self.result, &self.service, key.0, key.1, key.2, data_bytes)?;
        if actual_quote != reserved.quote || cost.service_fee != actual_quote.amount {
            return Err(SourceErrorV2::Binding);
        }
        let retained = self.history.prepare(event.material, SourceHistoryKindV2::Response, event.native_body, self.limits)?;
        if retained.is_none() {
            return Ok(self.prepared(
                SourceEffectV2::AlreadyApplied,
                self.usage,
                None,
                None,
                None,
                SourceChange::None,
                (key.0, key.1),
                Some(key),
            ));
        }
        let mut usage = self.event_usage(cost, event.native_body.len() as u64)?;
        let mut union = self.source_union(key.0, key.1).ok_or(SourceErrorV2::MissingRequest)?;
        let receipt = if self.receipts.contains_key(&key) {
            None
        } else {
            if self.receipts.len() as u64 >= self.limits.max_responses {
                return Err(SourceErrorV2::Budget);
            }
            union.completed_units = add(union.completed_units, 1)?;
            usage.fee = add(usage.fee, actual_quote.amount)?;
            usage.validate(&self.service.budget)?;
            // Bind the literal op3 duty to THIS accepted op4's RequestId.
            // Canonical unit identity still deduplicates fulfillment/payment
            // across other, overlapping requests.
            let duty = request.material;
            Some(ServiceReceiptRow {
                case: self.result.case_id(),
                kind: key.0,
                source: key.1,
                unit,
                result: self.result.id(),
                duty,
                response: event.material,
                provider: event.provider,
                bytes: data_bytes,
                additional_work: cost.additional_work,
                settlement: None,
            })
        };
        let effect = if receipt.is_some() { SourceEffectV2::Responded } else { SourceEffectV2::AlreadyComplete };
        Ok(self.prepared(effect, usage, Some(union), receipt, retained, SourceChange::Response, (key.0, key.1), Some(key)))
    }
    /// Pure cache application only. A4 must NOT use this as native acceptance.
    pub fn apply(&mut self, update: PreparedSourceUpdateV2) -> Result<(), SourceErrorV2> {
        self.validate_prepared(&update)?;
        if update.effect == SourceEffectV2::AlreadyApplied {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or(SourceErrorV2::Budget)?;
        if let Some(union) = update.union {
            match update.change {
                SourceChange::Request { record, intervals, reservations } => {
                    for (key, reservation) in reservations {
                        self.pending_fee =
                            self.pending_fee.checked_add(reservation.quote.amount).expect("prepared bounded reservation");
                        self.reservations.insert(key, reservation);
                    }
                    if let Some(record) = record {
                        self.requests.insert(record.request.id(), record);
                    }
                    self.sources.insert(
                        (union.kind, union.source),
                        SourceState { units: union.source_units, intervals, completed: union.completed_units, union: union.union },
                    );
                }
                SourceChange::Response => {
                    let source = self.sources.get_mut(&(union.kind, union.source)).expect("prepared source exists");
                    source.completed = union.completed_units;
                }
                SourceChange::None => {}
            }
        }
        if let Some(receipt) = update.receipt {
            let key = (receipt.kind, receipt.source, receipt.unit);
            let amount = self.reservations.get(&key).expect("prepared unit reservation").quote.amount;
            self.pending_fee = self.pending_fee.checked_sub(amount).expect("prepared pending reservation");
            self.receipts.insert(key, receipt);
        }
        if let Some(retained) = update.retained {
            self.history.apply(retained);
        }
        self.usage = update.usage;
        self.case_basis = update.case_row;
        self.revision = revision;
        Ok(())
    }
    pub fn validate_prepared(&self, update: &PreparedSourceUpdateV2) -> Result<(), SourceErrorV2> {
        if !Arc::ptr_eq(&self.seal, &update.seal)
            || update.revision != self.revision
            || update.result != self.result.id()
            || update.expected_usage != self.usage
        {
            return Err(SourceErrorV2::StalePrepared);
        }
        if update.expected_case != self.case_basis {
            return Err(SourceErrorV2::StalePrepared);
        }
        Ok(())
    }
    pub fn bind_case(&mut self, case: &CaseRow) -> Result<(), SourceErrorV2> {
        self.refresh_case_usage(case)
    }
    /// Pure cache refresh only; the caller must resolve this ACTUAL CaseRow in
    /// its current accepted view. Native apply compares the entire row again.
    pub fn refresh_case_usage(&mut self, case: &CaseRow) -> Result<(), SourceErrorV2> {
        NativeRowBody::Case(*case).validate_shape()?;
        if case.case != self.result.case_id()
            || case.result != self.result.id()
            || case.service != self.service
            || case.coverage.scope != self.result.scope
            || case.miner_data != self.miner_data_status()
            || self.case_basis.as_ref().is_some_and(|old| old.coverage != case.coverage)
            || !dominates(case.usage, self.usage)
        {
            return Err(SourceErrorV2::Binding);
        }
        let revision = self.revision.checked_add(1).ok_or(SourceErrorV2::Budget)?;
        case.usage.validate(&self.service.budget)?;
        if add(case.usage.fee, self.pending_fee)? > self.service.budget.fee {
            return Err(SourceErrorV2::Budget);
        }
        self.usage = case.usage;
        self.case_basis = Some(*case);
        self.revision = revision;
        Ok(())
    }
    fn check_geometry(&self, geometry: &TirSourceGeometryV2) -> Result<(), SourceErrorV2> {
        if geometry.result_id() != self.result.id() { Err(SourceErrorV2::Binding) } else { Ok(()) }
    }
    fn check_cost(&self, cost: SourceEventCostV2, material: AcceptedMaterial) -> Result<(), SourceErrorV2> {
        if cost.additional_work == 0 {
            return Err(SourceErrorV2::MissingPort(SourceMissingPortV2::Reservation));
        }
        if cost.cutoff > self.service.deadline
            || cost.cutoff > self.service.budget.retention_until
            || material.origin.accepted_daa >= cost.cutoff
        {
            return Err(SourceErrorV2::Deadline);
        }
        Ok(())
    }
    fn event_usage(&self, cost: SourceEventCostV2, body: u64) -> Result<AggregateUsage, SourceErrorV2> {
        let mut usage = self.usage;
        usage.transmitted_bytes = add(usage.transmitted_bytes, add(body, cost.outer_carrier_bytes)?)?;
        usage.retained_bytes = add(usage.retained_bytes, body)?;
        usage.additional_work = add(usage.additional_work, cost.additional_work)?;
        // Source preparation does not create a fee schedule, debit, settlement
        // or paid flag. A5's actual owner quotes/checks those via the context.
        usage.validate(&self.service.budget)?;
        Ok(usage)
    }
    fn prepared(
        &self,
        effect: SourceEffectV2,
        usage: AggregateUsage,
        union: Option<SourceUnionRow>,
        receipt: Option<ServiceReceiptRow>,
        retained: Option<SourceHistoryEntryV2>,
        change: SourceChange,
        touched_source: SourceKey,
        touched_unit: Option<UnitKey>,
    ) -> PreparedSourceUpdateV2 {
        let next_miner_data = self.next_miner_data(union.as_ref());
        let case_row = self.case_basis.map(|mut case| {
            if effect != SourceEffectV2::AlreadyApplied {
                case.usage = usage;
                case.miner_data = next_miner_data;
                if let Some(event) = retained.as_ref() {
                    case.last_event = event.material.origin;
                }
            }
            case
        });
        let fee_intent = receipt.as_ref().and_then(|receipt| {
            self.reservations.get(&(receipt.kind, receipt.source, receipt.unit)).map(|reservation| SourceFeeIntentV2 {
                key: misaka_palw_positive_contract::native::NativeRowKey::ServiceReceipt {
                    case: receipt.case,
                    kind: receipt.kind,
                    source: receipt.source,
                    unit: receipt.unit,
                },
                result: receipt.result,
                funding_scope: reservation.quote.funding_scope,
                schedule: reservation.quote.schedule,
                provider: receipt.provider,
                amount: reservation.quote.amount,
                accepted_response: receipt.response,
            })
        });
        PreparedSourceUpdateV2 {
            seal: self.seal.clone(),
            revision: self.revision,
            expected_usage: self.usage,
            result: self.result.id(),
            effect,
            usage,
            union,
            receipt,
            retained,
            change,
            fee_intent,
            expected_case: self.case_basis,
            case_row,
            next_miner_data,
            touched_source,
            touched_unit,
        }
    }
    fn next_miner_data(&self, union: Option<&SourceUnionRow>) -> MinerDataStatus {
        let mut any_requested = false;
        let mut all_complete = true;
        let mut replaced = false;
        for ((kind, source), state) in &self.sources {
            let (requested, complete) = if let Some(update) = union.filter(|row| row.kind == *kind && row.source == *source) {
                replaced = true;
                (update.requested_units, update.completed_units)
            } else {
                (interval_units(&state.intervals).expect("validated union"), state.completed)
            };
            any_requested |= requested != 0;
            all_complete &= requested == complete;
        }
        if let Some(update) = union.filter(|_| !replaced) {
            any_requested |= update.requested_units != 0;
            all_complete &= update.requested_units == update.completed_units;
        }
        if !any_requested {
            MinerDataStatus::NotRequested
        } else if all_complete {
            MinerDataStatus::Complete
        } else {
            MinerDataStatus::Pending
        }
    }
}
impl PreparedSourceUpdateV2 {
    /// Coordinates to compare against actual native rows even when this
    /// proposal emits no row (AlreadyApplied or AlreadyComplete).
    pub fn touched_source(&self) -> (SourceKind, Digest) {
        self.touched_source
    }
    /// Every op4 retains its canonical unit coordinates independently of
    /// whether fulfillment/payment was already recorded.
    pub fn touched_unit(&self) -> Option<(SourceKind, Digest, u64)> {
        self.touched_unit
    }
    pub fn expected_usage(&self) -> AggregateUsage {
        self.expected_usage
    }
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn expected_case(&self) -> Option<&CaseRow> {
        self.expected_case.as_ref()
    }
    pub fn case_row(&self) -> Option<&CaseRow> {
        self.case_row.as_ref()
    }
    pub fn fee_intent(&self) -> Option<&SourceFeeIntentV2> {
        self.fee_intent.as_ref()
    }
    pub fn next_miner_data_status(&self) -> MinerDataStatus {
        self.next_miner_data
    }
    pub fn effect(&self) -> SourceEffectV2 {
        self.effect
    }
    pub fn usage(&self) -> AggregateUsage {
        self.usage
    }
    pub fn union(&self) -> Option<&SourceUnionRow> {
        self.union.as_ref()
    }
    pub fn receipt(&self) -> Option<&ServiceReceiptRow> {
        self.receipt.as_ref()
    }
}
fn add(a: u64, b: u64) -> Result<u64, SourceErrorV2> {
    a.checked_add(b).ok_or(SourceErrorV2::Budget)
}
fn contains(intervals: &[(u64, u64)], unit: u64) -> bool {
    let i = intervals.partition_point(|(_, end)| *end <= unit);
    intervals.get(i).is_some_and(|(first, end)| *first <= unit && unit < *end)
}
fn interval_units(intervals: &[(u64, u64)]) -> Result<u64, SourceErrorV2> {
    intervals.iter().try_fold(0, |n, (a, b)| add(n, b.checked_sub(*a).ok_or(SourceErrorV2::Order)?))
}
fn merge(previous: &[(u64, u64)], added: (u64, u64)) -> Vec<(u64, u64)> {
    let mut ranges = Vec::with_capacity(previous.len() + 1);
    ranges.extend_from_slice(previous);
    ranges.push(added);
    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (first, end) in ranges {
        if let Some(last) = out.last_mut() {
            if first <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        out.push((first, end));
    }
    out
}

fn dominates(next: AggregateUsage, old: AggregateUsage) -> bool {
    next.rounds >= old.rounds
        && next.unique_source_bytes >= old.unique_source_bytes
        && next.transmitted_bytes >= old.transmitted_bytes
        && next.additional_work >= old.additional_work
        && next.retained_bytes >= old.retained_bytes
        && next.fee >= old.fee
        && next.witness_attempts >= old.witness_attempts
}
