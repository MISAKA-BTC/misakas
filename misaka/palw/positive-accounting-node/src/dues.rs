//! Bounded source-due arithmetic over existing common row DTOs.
//!
//! These proposals neither authenticate accepted history nor reserve/pay money.
//! The A4 writer must re-resolve first eligible duty/response, canonical prices,
//! provider, row absence and actual funding in its SAME atomic transaction and
//! compare its private current generation. Equal DTOs cannot detect rollback ABA.
//! No new wire, key, fee schedule, reservation ledger or native capability exists
//! here. In particular, `settlement: None` does not establish accepted delivery.

use misaka_palw_positive_accounting::fees::{FeeScheduleCeilings, Recipient, ServiceTerms};
use misaka_palw_positive_contract::{
    ContractError,
    funding::funding_scope,
    messages::{SourceKind, coverage_plan_id},
    native::NativeRowKey,
    rows::{AggregateUsage, CaseRow, FundingRow, NativeRowBody, ResultRow, ServiceReceiptRow},
};

/// The A0-adopted shared DATA type; preserves this module's existing import path.
/// Neither this re-export nor supplied terms establish native spend authority.
pub use misaka_palw_positive_contract::funding::SourceFeeDeltaIntent;

/// Expected/new DTOs for the writer's conditional application. No state mutation
/// occurs here. Native origin/generation checks remain additional requirements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDueProposal {
    pub fee_delta: Option<SourceFeeDeltaIntent>,
    pub expected_usage: AggregateUsage,
    pub next_usage: AggregateUsage,
    pub expected_receipt: Option<ServiceReceiptRow>,
    pub next_receipt: ServiceReceiptRow,
}

/// Already-calculated A2 update DATA to independently recheck. This does not
/// attest history, a logical NoOp or a newly assigned one-shot opportunity.
pub struct SourceDueUpdate<'a> {
    pub next_usage: AggregateUsage,
    pub receipt: &'a ServiceReceiptRow,
    pub existing: Option<&'a ServiceReceiptRow>,
    pub terms: ServiceTerms,
    pub claimed_delta: Option<SourceFeeDeltaIntent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceDueError {
    Contract(ContractError),
    Binding,
    ReceiptConflict,
    UnexpectedSettlement,
    Recipient,
    Usage,
    Budget,
    Overflow,
    StaleSnapshot,
    UpdateMismatch,
}

impl SourceDueProposal {
    /// DTO comparison only; the A4 owner must ALSO check its private generation,
    /// canonical prefix, funding and native authority before an atomic write.
    pub fn check_snapshot(&self, usage: AggregateUsage, receipt: Option<&ServiceReceiptRow>) -> Result<(), SourceDueError> {
        if usage != self.expected_usage || receipt != self.expected_receipt.as_ref() {
            return Err(SourceDueError::StaleSnapshot);
        }
        Ok(())
    }
}

/// Use the WHOLE source schedule as a conservative pre-request quote; do not
/// quote just delivered units. Success is arithmetic only, never a reservation.
/// Canonical preimages and the actual reservation must precede the native duty.
pub fn check_source_quote(
    result: &ResultRow,
    case: &CaseRow,
    funding: &FundingRow,
    ceilings: FeeScheduleCeilings,
) -> Result<(), SourceDueError> {
    check_links(result, case, funding)?;
    if ceilings.payer != funding.consent.payer
        || ceilings.fee_cap > funding.consent.max_fees
        || ceilings.fee_hold > funding.consent.max_locked
        || ceilings.fee_hold < funding.basis.required_fee
        || ceilings.source_fees < case.usage.fee
        || ceilings.source_fees > case.service.budget.fee
        || ceilings.source_fees > ceilings.all_fees_and_burn
        || ceilings.all_fees_and_burn > ceilings.fee_cap
        || ceilings.all_fees_and_burn > funding.basis.required_fee
    {
        return Err(SourceDueError::Budget);
    }
    Ok(())
}

/// Propose due for the first canonical receipt key exactly once. `source_usage`
/// is metering AFTER a source event, BEFORE this due increment. For A2 updates
/// that already include due, use `recheck_source_due_update` instead.
/// It must retain the old fee; accepting a caller-incremented fee would double
/// charge. The remaining source counters are preserved, never recomputed here.
/// Existing first receipts are immutable, including provider/response. Meter
/// increases here are generic DATA for a distinct, newly authenticated one-shot
/// opportunity, not permission to charge logical duplicates. A2/A4 must recheck
/// auth/literals before classifying a copy of the same op3/op4 as NoOp, before
/// charging or appending history. A different carrier is not fresh consent.
/// Such NoOps retain ALL usage counters, rows and history and emit no fee delta.
/// New/different authored semantic failures or retries are not blanket exempt.
/// The writer must check the whole source quote against the resulting case too;
/// this function alone neither resolves canonical rates nor checks live balances.
pub fn prepare_source_due(
    result: &ResultRow,
    case: &CaseRow,
    funding: &FundingRow,
    source_usage: AggregateUsage,
    receipt: &ServiceReceiptRow,
    existing: Option<&ServiceReceiptRow>,
    terms: ServiceTerms,
) -> Result<SourceDueProposal, SourceDueError> {
    check_links(result, case, funding)?;
    let old = case.usage;
    if source_usage.fee != old.fee
        || source_usage.rounds < old.rounds
        || source_usage.unique_source_bytes < old.unique_source_bytes
        || source_usage.transmitted_bytes < old.transmitted_bytes
        || source_usage.additional_work < old.additional_work
        || source_usage.retained_bytes < old.retained_bytes
        || source_usage.witness_attempts != old.witness_attempts
    {
        return Err(SourceDueError::Usage);
    }
    source_usage.validate(&case.service.budget).map_err(SourceDueError::Contract)?;
    let source = match receipt.kind {
        SourceKind::ArtifactInventory => result.result.artifact,
        SourceKind::Input => result.result.input,
    };
    if receipt.case != case.case || receipt.result != case.result || receipt.source != source {
        return Err(SourceDueError::Binding);
    }
    let key = receipt_key(receipt);
    let mut next_usage = source_usage;
    let fee_delta = if let Some(first) = existing {
        // An existing settlement reference may have been attached by the money
        // owner; it cannot be removed or turn this unit into a new receipt.
        if receipt_key(first) != key || first != receipt {
            return Err(SourceDueError::ReceiptConflict);
        }
        None
    } else {
        if receipt.settlement.is_some() {
            return Err(SourceDueError::UnexpectedSettlement);
        }
        if terms.recipient != Recipient::Fixed(receipt.provider) {
            return Err(SourceDueError::Recipient);
        }
        next_usage.fee = old.fee.checked_add(terms.amount).ok_or(SourceDueError::Overflow)?;
        if next_usage.fee > funding.basis.required_fee || next_usage.fee > funding.consent.max_fees {
            return Err(SourceDueError::Budget);
        }
        Some(SourceFeeDeltaIntent {
            receipt_key: key,
            result: case.result,
            funding_scope: result.funding_scope,
            schedule: funding.basis.schedule,
            provider: receipt.provider,
            amount: terms.amount,
            accepted_response: receipt.response,
        })
    };
    next_usage.validate(&case.service.budget).map_err(SourceDueError::Contract)?;
    Ok(SourceDueProposal {
        fee_delta,
        expected_usage: old,
        next_usage,
        expected_receipt: existing.copied(),
        next_receipt: existing.copied().unwrap_or(*receipt),
    })
}

/// Recompute an already-incremented source update without adding its due twice.
/// Only the fee is projected to its old value for independent arithmetic; the
/// entire resulting usage and shared delta must equal the claimed update.
/// This remains DATA-only: native owners must establish auth/history, first
/// receipt, logical NoOp versus fresh one-shot opportunity, rate and generation.
pub fn recheck_source_due_update(
    result: &ResultRow,
    case: &CaseRow,
    funding: &FundingRow,
    update: SourceDueUpdate<'_>,
) -> Result<SourceDueProposal, SourceDueError> {
    let before_due = AggregateUsage { fee: case.usage.fee, ..update.next_usage };
    let proposal = prepare_source_due(result, case, funding, before_due, update.receipt, update.existing, update.terms)?;
    if proposal.next_usage != update.next_usage || proposal.fee_delta != update.claimed_delta {
        return Err(SourceDueError::UpdateMismatch);
    }
    Ok(proposal)
}

fn receipt_key(receipt: &ServiceReceiptRow) -> NativeRowKey {
    NativeRowKey::ServiceReceipt { case: receipt.case, kind: receipt.kind, source: receipt.source, unit: receipt.unit }
}

fn check_links(result: &ResultRow, case: &CaseRow, funding: &FundingRow) -> Result<(), SourceDueError> {
    let r = &result.result;
    NativeRowBody::Result(result.clone()).validate_shape().map_err(SourceDueError::Contract)?;
    NativeRowBody::Case(*case).validate_shape().map_err(SourceDueError::Contract)?;
    NativeRowBody::Funding(funding.clone()).validate_shape().map_err(SourceDueError::Contract)?;
    funding.consent.validate(r, &case.service, &funding.basis, result.origin.accepted_daa).map_err(SourceDueError::Contract)?;
    case.usage.validate(&case.service.budget).map_err(SourceDueError::Contract)?;
    if case.case != r.case_id()
        || case.result != r.id()
        || result.policy != funding.basis.policy
        || result.funding_scope != funding.consent.scope
        || result.funding_scope != funding_scope(r, &case.service, &funding.basis)
        || result.origin != funding.funding_origin
        || case.coverage.scope != r.scope
        || case.coverage.profile != funding.basis.profile
        || case.coverage.mandatory_relations != funding.basis.mandatory_relations
        || case.coverage.plan
            != coverage_plan_id(r, funding.basis.profile, funding.basis.derivation, funding.basis.mandatory_relations)
                .map_err(SourceDueError::Contract)?
    {
        return Err(SourceDueError::Binding);
    }
    Ok(())
}

#[cfg(test)]
#[path = "dues/tests.rs"]
mod tests;
