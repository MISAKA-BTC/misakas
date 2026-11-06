//! New-version Positive funding DATA calculations for A0/native-owner review.
//!
//! These functions do not install a lock, authorize an input, resolve canonical
//! prices/history, mutate a row or bypass scripts. Public `ValidatedTransaction`
//! and row DTOs are constructible data, not accepted-prefix capabilities. The
//! actual owner must re-resolve every input and CAS in its single UTXO/PALW DB
//! batch. No old FundingTerms ID, relay domain, key kind or native authority is
//! manufactured here. Initial transaction payload support is empty only.

use crate::{dues::SourceFeeDeltaIntent, native::FeeRecipientKeys};
use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::p2pkh_mldsa87_payload,
    tx::{PopulatedTransaction, Transaction, TransactionOutpoint},
};
use misaka_palw_positive_contract::{
    ContractError,
    funding::funding_scope,
    identity::{CaseId, ChainScope, Digest, FundingScopeId, PrincipalId, ResultId},
    messages::{ClaimStatus, SourceKind, coverage_plan_id},
    native::NativeRowKey,
    rows::{
        AcceptedMaterial, AcceptedOrigin, BalanceLeg, CaseRow, FundingRow, NativeHash64, NativeOutpoint, NativeRowBody, ResultRow,
        ServiceReceiptRow,
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub struct FundingData<'a> {
    pub result: &'a ResultRow,
    pub case: &'a CaseRow,
    pub funding: &'a FundingRow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingIdentityData {
    pub result: ResultId,
    pub case: CaseId,
    pub chain_scope: ChainScope,
    pub funding_scope: FundingScopeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositiveFundsError {
    Contract(ContractError),
    Binding,
    Budget,
    Overflow,
    DuplicateLock,
    DrainedLock,
    DuplicateInput,
    ProvisionalSpend,
    MultipleLockedInputs,
    MissingPositiveLock,
    LegacyNeedsQualification,
    CompareAndSwap,
    TransactionIdentity,
    UnsupportedPayload,
    Entry,
    SourceReceipt,
    AlreadySettled,
    DuplicateDue,
    Recipient,
    CompletionNotEarned,
    ReleaseEvidence,
    Output,
    Conservation,
    BurnAllocation,
    NoValueMovement,
    SettlementOrigin,
    Transition,
}

/// Never satisfied by row membership, a DTO, signature or caller callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingNativeFundsOwner {
    ActualAcceptedMoneyContext,
}

fn h(d: Digest) -> Hash64 {
    Hash64::from_bytes(d.0)
}
fn outpoint(point: NativeOutpoint) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_bytes(point.transaction.0), point.index)
}
fn point_key(point: TransactionOutpoint) -> ([u8; 64], u32) {
    (point.transaction_id.as_bytes(), point.index)
}
fn remaining(row: &FundingRow) -> Result<u64, PositiveFundsError> {
    row.fee
        .remaining
        .checked_add(row.capital.remaining)
        .and_then(|n| n.checked_add(row.surplus.remaining))
        .ok_or(PositiveFundsError::Overflow)
}
fn identity(data: FundingData<'_>) -> Result<FundingIdentityData, PositiveFundsError> {
    NativeRowBody::Result(data.result.clone()).validate_shape().map_err(PositiveFundsError::Contract)?;
    NativeRowBody::Case(*data.case).validate_shape().map_err(PositiveFundsError::Contract)?;
    NativeRowBody::Funding(data.funding.clone()).validate_shape().map_err(PositiveFundsError::Contract)?;
    let result = &data.result.result;
    let row = data.funding;
    row.consent
        .validate(result, &data.case.service, &row.basis, row.funding_origin.accepted_daa)
        .map_err(PositiveFundsError::Contract)?;
    if result.case_id() != data.case.case
        || result.id() != data.case.result
        || result.service_contract != data.case.service.id()
        || row.consent.case != data.case.case
        || data.result.funding_scope != row.consent.scope
        || funding_scope(result, &data.case.service, &row.basis) != row.consent.scope
        || data.result.policy != row.basis.policy
    {
        return Err(PositiveFundsError::Binding);
    }
    if data.result.origin != row.funding_origin
        || data.case.coverage.scope != result.scope
        || data.case.coverage.profile != row.basis.profile
        || data.case.coverage.mandatory_relations != row.basis.mandatory_relations
        || data.case.coverage.plan
            != coverage_plan_id(result, row.basis.profile, row.basis.derivation, row.basis.mandatory_relations)
                .map_err(PositiveFundsError::Contract)?
    {
        return Err(PositiveFundsError::Binding);
    }
    Ok(FundingIdentityData { result: result.id(), case: data.case.case, chain_scope: result.scope, funding_scope: row.consent.scope })
}

struct PositiveLockData {
    identity: FundingIdentityData,
    before: FundingRow,
}

/// DATA selected-parent lock view. All callers use the same input classifier:
/// mempool, block and template must not independently weaken this predicate.
/// Same-prefix accepted funding/continuations are provisional and unspendable.
/// Native owners must build this from actual roots/prefix, never peer rows.
pub struct PositiveLockViewData {
    positive: BTreeMap<([u8; 64], u32), PositiveLockData>,
    legacy: BTreeSet<([u8; 64], u32)>,
    provisional: BTreeSet<([u8; 64], u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockedInputData {
    None,
    NeedsLegacyQualification { input: usize, outpoint: TransactionOutpoint },
    NeedsPositiveQualification { input: usize, outpoint: TransactionOutpoint },
}

impl PositiveLockViewData {
    pub fn new(
        positive: &[FundingData<'_>],
        legacy: &[TransactionOutpoint],
        provisional: &[TransactionOutpoint],
        max_locks: usize,
    ) -> Result<Self, PositiveFundsError> {
        let count = positive
            .len()
            .checked_add(legacy.len())
            .and_then(|n| n.checked_add(provisional.len()))
            .ok_or(PositiveFundsError::Overflow)?;
        if count > max_locks {
            return Err(PositiveFundsError::Budget);
        }
        let mut all = BTreeSet::new();
        let mut cases = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut view = Self { positive: BTreeMap::new(), legacy: BTreeSet::new(), provisional: BTreeSet::new() };
        for data in positive {
            let id = identity(*data)?;
            if !cases.insert(id.case) || !origins.insert(point_key(outpoint(data.funding.original_outpoint))) {
                return Err(PositiveFundsError::DuplicateLock);
            }
            let balance = remaining(data.funding)?;
            if balance == 0 {
                if data.funding.current_outpoint.is_some() {
                    return Err(PositiveFundsError::DrainedLock);
                }
                continue;
            }
            let point = point_key(outpoint(data.funding.current_outpoint.ok_or(PositiveFundsError::MissingPositiveLock)?));
            if !all.insert(point) {
                return Err(PositiveFundsError::DuplicateLock);
            }
            view.positive.insert(point, PositiveLockData { identity: id, before: data.funding.clone() });
        }
        for point in legacy {
            let point = point_key(*point);
            if !all.insert(point) {
                return Err(PositiveFundsError::DuplicateLock);
            }
            view.legacy.insert(point);
        }
        for point in provisional {
            let point = point_key(*point);
            if !all.insert(point) {
                return Err(PositiveFundsError::DuplicateLock);
            }
            view.provisional.insert(point);
        }
        Ok(view)
    }

    /// Returns NEEDS qualification, never an input-spend permission. Two locked
    /// inputs (old/new included) cannot count each other as foreign financing.
    pub fn classify_spend(&self, tx: &Transaction) -> Result<LockedInputData, PositiveFundsError> {
        let mut seen = BTreeSet::new();
        let mut locked = LockedInputData::None;
        for (input, item) in tx.inputs.iter().enumerate() {
            let point = point_key(item.previous_outpoint);
            if !seen.insert(point) {
                return Err(PositiveFundsError::DuplicateInput);
            }
            if self.provisional.contains(&point) {
                return Err(PositiveFundsError::ProvisionalSpend);
            }
            let next = if self.positive.contains_key(&point) {
                LockedInputData::NeedsPositiveQualification { input, outpoint: item.previous_outpoint }
            } else if self.legacy.contains(&point) {
                LockedInputData::NeedsLegacyQualification { input, outpoint: item.previous_outpoint }
            } else {
                continue;
            };
            if locked != LockedInputData::None {
                return Err(PositiveFundsError::MultipleLockedInputs);
            }
            locked = next;
        }
        Ok(locked)
    }
}

/// Source ledger produces the delta on FIRST unique receipt only. A pending
/// (`settlement=None`) receipt can be due. Settlement is attached atomically by
/// the actual money owner; neither None nor this DTO authenticates its history.
pub struct SourcePaymentData<'a> {
    pub due: &'a SourceFeeDeltaIntent,
    pub expected_receipt: &'a ServiceReceiptRow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtWholeFeeKind {
    Control,
    Carrier,
    PanelCompletion,
    ContractualBurn,
}

/// DATA terms, not canonical price authority. No new serialized global key.
pub struct CourtWholeFeeData {
    pub kind: CourtWholeFeeKind,
    pub result: ResultId,
    pub funding_scope: FundingScopeId,
    pub schedule: Digest,
    pub provider: Option<PrincipalId>,
    pub amount: u64,
    pub accepted_service: AcceptedMaterial,
}

/// Refund arithmetic only. The native owner must resolve this EXACT liability
/// preimage and current conditions; a supplied reference does not release funds.
/// It must also preserve all unpaid earned dues and reserved future Court fees.
/// This calculator cannot derive that obligation floor from a hash/usage total;
/// reference equality below is NOT evidence that returning a fee is permissible.
#[derive(Clone, Copy, Debug, Default)]
pub struct RefundData {
    pub fee: u64,
    pub capital: u64,
    pub surplus: u64,
    pub resolution: Option<AcceptedMaterial>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OtherBurnData {
    pub bond: u64,
    pub legacy_escrow: u64,
}

pub struct PositiveSpendData<'a> {
    pub current: FundingData<'a>,
    pub locks: &'a PositiveLockViewData,
    pub transaction: &'a PopulatedTransaction<'a>,
    pub source: &'a [SourcePaymentData<'a>],
    pub court_whole: &'a [CourtWholeFeeData],
    pub recipients: &'a FeeRecipientKeys,
    pub refunds: RefundData,
    pub other_burn: OtherBurnData,
    pub settlement: AcceptedMaterial,
    pub now: u64,
    pub max_dues: usize,
}

/// Concrete three-leg / outputs / CAS calculation, never a native authority.
pub struct PositiveSpendProposalData {
    identity: FundingIdentityData,
    before_result: ResultRow,
    before_case: CaseRow,
    before: FundingRow,
    after: FundingRow,
    source_receipts: Vec<(ServiceReceiptRow, ServiceReceiptRow)>,
    source_fee_paid: u64,
    court_fee_paid: u64,
    positive_burn: u64,
    returned: u64,
    continued: u64,
    actual_fee: u64,
}
impl PositiveSpendProposalData {
    pub fn identity(&self) -> FundingIdentityData {
        self.identity
    }
    pub fn before(&self) -> &FundingRow {
        &self.before
    }
    pub fn after(&self) -> &FundingRow {
        &self.after
    }
    pub fn source_receipts(&self) -> &[(ServiceReceiptRow, ServiceReceiptRow)] {
        &self.source_receipts
    }
    pub fn source_fee_paid(&self) -> u64 {
        self.source_fee_paid
    }
    pub fn court_fee_paid(&self) -> u64 {
        self.court_fee_paid
    }
    pub fn positive_burn(&self) -> u64 {
        self.positive_burn
    }
    pub fn returned(&self) -> u64 {
        self.returned
    }
    pub fn continued(&self) -> u64 {
        self.continued
    }
    pub fn actual_fee(&self) -> u64 {
        self.actual_fee
    }
    pub fn check_snapshot(&self, current: FundingData<'_>) -> Result<(), PositiveFundsError> {
        if identity(current)? != self.identity
            || *current.result != self.before_result
            || *current.case != self.before_case
            || *current.funding != self.before
        {
            return Err(PositiveFundsError::CompareAndSwap);
        }
        Ok(())
    }
    /// Current receipts relevant to this proposal, after any source-first-due
    /// candidate update and before its money settlement. Native source absence
    /// CAS/accepted generation remain separate owner obligations.
    pub fn check_source_snapshot(&self, current: &[ServiceReceiptRow]) -> Result<(), PositiveFundsError> {
        if current.len() != self.source_receipts.len() {
            return Err(PositiveFundsError::CompareAndSwap);
        }
        for (expected, _) in &self.source_receipts {
            let matches = current
                .iter()
                .filter(|row| {
                    row.case == expected.case
                        && row.kind == expected.kind
                        && row.source == expected.source
                        && row.unit == expected.unit
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 || *matches[0] != *expected {
                return Err(PositiveFundsError::CompareAndSwap);
            }
        }
        Ok(())
    }
    pub fn require_native_commit(&self) -> Result<std::convert::Infallible, MissingNativeFundsOwner> {
        Err(MissingNativeFundsOwner::ActualAcceptedMoneyContext)
    }
}

fn add(total: &mut u64, amount: u64) -> Result<(), PositiveFundsError> {
    *total = total.checked_add(amount).ok_or(PositiveFundsError::Overflow)?;
    Ok(())
}
fn subtract(leg: &mut BalanceLeg, amount: u64) -> Result<(), PositiveFundsError> {
    leg.remaining = leg.remaining.checked_sub(amount).ok_or(PositiveFundsError::Conservation)?;
    Ok(())
}
fn output(tx: &Transaction, next: &mut usize, amount: u64, payload: Hash64) -> Result<(), PositiveFundsError> {
    if amount == 0 {
        return Ok(());
    }
    let item = tx.outputs.get(*next).ok_or(PositiveFundsError::Output)?;
    if item.value != amount || p2pkh_mldsa87_payload(&item.script_public_key) != Some(payload) {
        return Err(PositiveFundsError::Output);
    }
    *next = next.checked_add(1).ok_or(PositiveFundsError::Overflow)?;
    Ok(())
}

fn not_after(earlier: AcceptedOrigin, later: AcceptedOrigin) -> bool {
    earlier.accepted_daa <= later.accepted_daa
        && earlier.accepted_blue_score <= later.accepted_blue_score
        && (earlier.accepting_block != later.accepting_block
            || (earlier.accepted_daa == later.accepted_daa
                && earlier.accepted_blue_score == later.accepted_blue_score
                && (earlier.accepted_tx_ordinal, earlier.event_ordinal) <= (later.accepted_tx_ordinal, later.event_ordinal)))
}

pub fn propose_positive_spend(input: PositiveSpendData<'_>) -> Result<PositiveSpendProposalData, PositiveFundsError> {
    let id = identity(input.current)?;
    let row = input.current.funding;
    let tx = input.transaction.tx;
    if !tx.id_is_current() {
        return Err(PositiveFundsError::TransactionIdentity);
    }
    if !tx.payload.is_empty() {
        return Err(PositiveFundsError::UnsupportedPayload);
    }
    if input.source.len().checked_add(input.court_whole.len()).ok_or(PositiveFundsError::Overflow)? > input.max_dues {
        return Err(PositiveFundsError::Budget);
    }
    let current_point = outpoint(row.current_outpoint.ok_or(PositiveFundsError::MissingPositiveLock)?);
    match input.locks.classify_spend(tx)? {
        LockedInputData::NeedsPositiveQualification { input: 0, outpoint } if outpoint == current_point => {}
        LockedInputData::NeedsLegacyQualification { .. } => return Err(PositiveFundsError::LegacyNeedsQualification),
        _ => return Err(PositiveFundsError::MissingPositiveLock),
    }
    let lock = input.locks.positive.get(&point_key(current_point)).ok_or(PositiveFundsError::MissingPositiveLock)?;
    if lock.before != *row || lock.identity != id {
        return Err(PositiveFundsError::CompareAndSwap);
    }
    if input.transaction.entries.len() != tx.inputs.len() || input.now < row.funding_origin.accepted_daa {
        return Err(PositiveFundsError::Entry);
    }
    let entry = input.transaction.entries.first().ok_or(PositiveFundsError::Entry)?;
    if entry.amount != remaining(row)? || p2pkh_mldsa87_payload(&entry.script_public_key) != Some(h(row.consent.refund_payload)) {
        return Err(PositiveFundsError::Entry);
    }
    if input.settlement.origin.carrier_transaction.0 != tx.id().as_bytes() || input.settlement.origin.accepted_daa != input.now {
        return Err(PositiveFundsError::SettlementOrigin);
    }
    if !not_after(row.funding_origin, input.settlement.origin)
        || !not_after(input.current.case.last_event, input.settlement.origin)
        || row.last_settlement.is_some_and(|last| {
            !not_after(last.origin, input.settlement.origin)
                || (last.origin.accepting_block == input.settlement.origin.accepting_block
                    && (last.origin.accepted_tx_ordinal, last.origin.event_ordinal)
                        == (input.settlement.origin.accepted_tx_ordinal, input.settlement.origin.event_ordinal))
        })
    {
        return Err(PositiveFundsError::SettlementOrigin);
    }
    let mut after = row.clone();
    let mut next = 0usize;
    let mut source_total = 0u64;
    let mut court_total = 0u64;
    let mut burn = 0u64;
    let mut seen = BTreeSet::new();
    let mut receipts = Vec::with_capacity(input.source.len());
    for item in input.source {
        let due = item.due;
        let receipt = item.expected_receipt;
        let key = match due.receipt_key {
            NativeRowKey::ServiceReceipt { case, kind, source, unit } => {
                if case != id.case || receipt.case != case || receipt.kind != kind || receipt.source != source || receipt.unit != unit
                {
                    return Err(PositiveFundsError::SourceReceipt);
                }
                let expected_source = match kind {
                    SourceKind::ArtifactInventory => input.current.result.result.artifact,
                    SourceKind::Input => input.current.result.result.input,
                };
                if source != expected_source {
                    return Err(PositiveFundsError::SourceReceipt);
                }
                due.receipt_key.encode()
            }
            _ => return Err(PositiveFundsError::SourceReceipt),
        };
        if !seen.insert(key) {
            return Err(PositiveFundsError::DuplicateDue);
        }
        if receipt.settlement.is_some() {
            return Err(PositiveFundsError::AlreadySettled);
        }
        if due.result != id.result
            || receipt.result != id.result
            || due.funding_scope != id.funding_scope
            || due.schedule != row.basis.schedule
            || receipt.provider != due.provider
            || receipt.response != due.accepted_response
        {
            return Err(PositiveFundsError::SourceReceipt);
        }
        if !not_after(due.accepted_response.origin, input.settlement.origin)
            || !not_after(receipt.duty.origin, input.settlement.origin)
        {
            return Err(PositiveFundsError::SettlementOrigin);
        }
        output(tx, &mut next, due.amount, input.recipients.payload(due.provider).map_err(|_| PositiveFundsError::Recipient)?)?;
        add(&mut source_total, due.amount)?;
        let mut receipt_after = *receipt;
        receipt_after.settlement = Some(input.settlement);
        receipts.push((*receipt, receipt_after));
    }
    // Source usage is the source ledger's first-due counter. Settlement NEVER
    // increments it a second time and Court/control fees never enter it.
    if source_total > input.current.case.usage.fee {
        return Err(PositiveFundsError::Budget);
    }
    let mut court_seen = Vec::new();
    for due in input.court_whole {
        if due.result != id.result || due.funding_scope != id.funding_scope || due.schedule != row.basis.schedule {
            return Err(PositiveFundsError::Binding);
        }
        // One completion per case/plan and one carrier fee per physical
        // transaction, regardless of attempt or accepted-origin relabeling.
        if court_seen.iter().any(|(kind, material): &(CourtWholeFeeKind, AcceptedMaterial)| {
            *kind == due.kind
                && match kind {
                    CourtWholeFeeKind::PanelCompletion | CourtWholeFeeKind::ContractualBurn => true,
                    CourtWholeFeeKind::Carrier => {
                        material.origin.carrier_transaction == due.accepted_service.origin.carrier_transaction
                    }
                    CourtWholeFeeKind::Control => *material == due.accepted_service,
                }
        }) {
            return Err(PositiveFundsError::DuplicateDue);
        }
        court_seen.push((due.kind, due.accepted_service));
        if !not_after(due.accepted_service.origin, input.settlement.origin) {
            return Err(PositiveFundsError::SettlementOrigin);
        }
        if due.kind == CourtWholeFeeKind::PanelCompletion && input.current.case.status != ClaimStatus::PositivePassed {
            return Err(PositiveFundsError::CompletionNotEarned);
        }
        match (due.kind, due.provider) {
            (CourtWholeFeeKind::ContractualBurn, None) => add(&mut burn, due.amount)?,
            (CourtWholeFeeKind::ContractualBurn, Some(_)) | (_, None) => return Err(PositiveFundsError::Recipient),
            (_, Some(provider)) => {
                output(tx, &mut next, due.amount, input.recipients.payload(provider).map_err(|_| PositiveFundsError::Recipient)?)?;
                add(&mut court_total, due.amount)?;
            }
        }
    }
    let paid = source_total.checked_add(court_total).ok_or(PositiveFundsError::Overflow)?;
    subtract(&mut after.fee, paid)?;
    subtract(&mut after.fee, burn)?;
    add(&mut after.fee.paid, paid)?;
    add(&mut after.fee.burned, burn)?;
    let refunds = input.refunds;
    let returned =
        refunds.fee.checked_add(refunds.capital).and_then(|n| n.checked_add(refunds.surplus)).ok_or(PositiveFundsError::Overflow)?;
    if returned > 0 && refunds.resolution != Some(row.liability) {
        return Err(PositiveFundsError::ReleaseEvidence);
    }
    for (leg, amount) in [(&mut after.fee, refunds.fee), (&mut after.capital, refunds.capital), (&mut after.surplus, refunds.surplus)]
    {
        subtract(leg, amount)?;
        add(&mut leg.refunded, amount)?;
    }
    output(tx, &mut next, returned, h(row.consent.refund_payload))?;
    let continued = remaining(&after)?;
    if paid == 0 && burn == 0 && returned == 0 {
        return Err(PositiveFundsError::NoValueMovement);
    }
    after.current_outpoint = if continued > 0 {
        let index = u32::try_from(next).map_err(|_| PositiveFundsError::Overflow)?;
        output(tx, &mut next, continued, h(row.consent.refund_payload))?;
        Some(NativeOutpoint { transaction: NativeHash64(tx.id().as_bytes()), index })
    } else {
        None
    };
    after.last_settlement = Some(input.settlement);
    let in_sum =
        input.transaction.entries.iter().try_fold(0u64, |n, entry| n.checked_add(entry.amount)).ok_or(PositiveFundsError::Overflow)?;
    let out_sum = tx.outputs.iter().try_fold(0u64, |n, item| n.checked_add(item.value)).ok_or(PositiveFundsError::Overflow)?;
    let actual_fee = in_sum.checked_sub(out_sum).ok_or(PositiveFundsError::Conservation)?;
    let other_in = input
        .transaction
        .entries
        .iter()
        .skip(1)
        .try_fold(0u64, |n, entry| n.checked_add(entry.amount))
        .ok_or(PositiveFundsError::Overflow)?;
    let other_out =
        tx.outputs.iter().skip(next).try_fold(0u64, |n, item| n.checked_add(item.value)).ok_or(PositiveFundsError::Overflow)?;
    if other_in < other_out {
        return Err(PositiveFundsError::Conservation);
    }
    let all_burn = input
        .other_burn
        .bond
        .checked_add(input.other_burn.legacy_escrow)
        .and_then(|n| n.checked_add(burn))
        .ok_or(PositiveFundsError::Overflow)?;
    if actual_fee < all_burn {
        return Err(PositiveFundsError::BurnAllocation);
    }
    NativeRowBody::Funding(after.clone()).validate_shape().map_err(PositiveFundsError::Contract)?;
    // All immutable fields are preserved by cloning the exact CAS before-row.
    Ok(PositiveSpendProposalData {
        identity: id,
        before_result: input.current.result.clone(),
        before_case: *input.current.case,
        before: row.clone(),
        after,
        source_receipts: receipts,
        source_fee_paid: source_total,
        court_fee_paid: court_total,
        positive_burn: burn,
        returned,
        continued,
        actual_fee,
    })
}

#[cfg(test)]
mod tests;
