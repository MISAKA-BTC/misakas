//! A5 fee projections checked by the EXISTING native funded-escrow engine.
//!
//! This module does not create a Positive native funding row, reinterpret TFO1's
//! execution root, install a schedule into a preset, skip ordinary transaction
//! scripts, or grant credit. A6/A4 must still commit the accepted transaction,
//! resource/case changes and the existing escrow delta atomically. The native
//! Positive admission/binding resolver remains unassigned.

use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, mldsa87_key_id, p2pkh_mldsa87_payload},
    palw_court_original_escrow_v1::{ActualCarrierFundingV1, FundingRefusalV1, FundingTermsV1, inspect_transaction_v1},
    palw_funded_escrow_v1::{
        PalwEscrowDueKindV1, PalwEscrowDueV1, PalwEscrowFeeScheduleV1, PalwEscrowRefusalV1, PalwEscrowSpendPlanV1,
        PalwFundedEscrowRowV1, palw_escrow_qualified_spend_v1,
    },
    tx::{Transaction, TransactionOutpoint},
};
use misaka_palw_positive_accounting::fees::{AccountingAuthority, AccountingError, FeeSettlementProjection, PrivateAccountingStore};
use misaka_palw_positive_contract::{
    ContractError,
    funding::AuthenticatedFunding,
    identity::{Digest, FundingConsentId, FundingScopeId, PrincipalId, ResultId},
    messages::AuthenticatedResult,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExistingEscrowFeeError {
    Accounting(AccountingError),
    TransactionIdentity,
    ResultBinding,
    EscrowBinding,
    Totals,
    RecipientKey,
    LiabilityStillOpen,
    Budget,
    LegacyTerms(FundingRefusalV1),
    Escrow(PalwEscrowRefusalV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeeOnlyFundingError {
    Contract(ContractError),
    Accounting(AccountingError),
    Carrier(FundingRefusalV1),
    ResultBinding,
    IntentBinding,
    UnsupportedCapital,
    PayerFeeCap,
    MinerSourceFeeCap,
    QuotedFeeCeiling,
    Output,
}

/// Payer-signature and literal own-output preflight ONLY. In particular, the
/// policy/schedule hashes in FundingPlanBasis do not resolve their canonical
/// preimages or authorize arbitrary prices. Rooted policy/schedule derivation,
/// actual UTXO/input authorization and the atomic reservation remain required.
/// No constructor or serialization can promote this to native admission.
pub struct PayerSignedFeeOnlyOutput<'a> {
    actual: ActualCarrierFundingV1<'a>,
    consent: FundingConsentId,
    scope: FundingScopeId,
    result: ResultId,
}
impl PayerSignedFeeOnlyOutput<'_> {
    pub fn outpoint(&self) -> TransactionOutpoint {
        self.actual.outpoint()
    }
    pub fn amount(&self) -> u64 {
        self.actual.output().value
    }
    pub fn consent(&self) -> FundingConsentId {
        self.consent
    }
    pub fn scope(&self) -> FundingScopeId {
        self.scope
    }
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn require_native_admission(&self) -> Result<std::convert::Infallible, ContractError> {
        misaka_palw_positive_contract::require_native_activation()
    }
}

/// Authenticate the acyclic A0 funding links and check the actual carrier's own
/// output, using the WHOLE admitted source/court schedule rather than just the
/// units already delivered. This initial adapter is strictly fee-only; capital
/// must not be silently counted as a fee hold or automatically refunded.
pub fn inspect_signed_fee_only_funding<'a, A: AccountingAuthority>(
    accounting: &PrivateAccountingStore<A>,
    result: &AuthenticatedResult<'_>,
    funding: &AuthenticatedFunding<'_>,
    projection: &FeeSettlementProjection,
    carrier: &'a Transaction,
    now: u64,
) -> Result<PayerSignedFeeOnlyOutput<'a>, FeeOnlyFundingError> {
    let consent = funding.consent();
    let service = funding.service();
    let basis = funding.basis();
    consent.validate(funding.result(), service, basis, now).map_err(FeeOnlyFundingError::Contract)?;
    if funding.result() != result.commit() {
        return Err(FeeOnlyFundingError::ResultBinding);
    }
    if basis.required_capital != 0 {
        return Err(FeeOnlyFundingError::UnsupportedCapital);
    }
    let intent = projection.intent();
    if intent.scope != result.commit().scope
        || intent.case != result.commit().case_id()
        || intent.result != result.commit().id()
        || intent.funding_consent != consent.id().0
        || intent.resource_quote != basis.resource_quote
        || intent.expiry > consent.admission_expiry
        || now >= intent.expiry
    {
        return Err(FeeOnlyFundingError::IntentBinding);
    }
    let ceilings = accounting.schedule_ceilings(projection).map_err(FeeOnlyFundingError::Accounting)?;
    if ceilings.payer != consent.payer
        || ceilings.fee_cap > consent.max_fees
        || ceilings.fee_hold > consent.max_locked
        || ceilings.all_fees_and_burn > consent.max_fees
    {
        return Err(FeeOnlyFundingError::PayerFeeCap);
    }
    if ceilings.source_fees > service.budget.fee {
        return Err(FeeOnlyFundingError::MinerSourceFeeCap);
    }
    if ceilings.all_fees_and_burn > basis.required_fee || ceilings.fee_hold < basis.required_fee {
        return Err(FeeOnlyFundingError::QuotedFeeCeiling);
    }
    let actual = inspect_transaction_v1(carrier, consent.funding_output_index).map_err(FeeOnlyFundingError::Carrier)?;
    if actual.output().value != ceilings.fee_hold
        || actual.output().value > consent.max_locked
        || p2pkh_mldsa87_payload(&actual.output().script_public_key) != Some(h(consent.refund_payload))
    {
        return Err(FeeOnlyFundingError::Output);
    }
    Ok(PayerSignedFeeOnlyOutput { actual, consent: consent.id(), scope: consent.scope, result: result.commit().id() })
}

/// A principal ID is an unkeyed public-key identity; it is NEVER an address
/// payload. Derive the native address from the exact key instead of casting IDs.
/// These keys authenticate identity only and make no computation claim.
pub struct FeeRecipientKeys(BTreeMap<PrincipalId, Hash64>);
impl FeeRecipientKeys {
    pub fn new<'a>(keys: impl IntoIterator<Item = &'a [u8]>, max_keys: usize) -> Result<Self, ExistingEscrowFeeError> {
        let mut recipients = BTreeMap::new();
        for key in keys {
            if key.len() != MLDSA87_PUBKEY_LEN {
                return Err(ExistingEscrowFeeError::RecipientKey);
            }
            if recipients.len() >= max_keys {
                return Err(ExistingEscrowFeeError::Budget);
            }
            let principal = PrincipalId(Digest(mldsa87_key_id(key).as_bytes()));
            let payload = kaspa_hashes::blake2b_512_address_payload(key);
            if recipients.insert(principal, payload).is_some() {
                return Err(ExistingEscrowFeeError::RecipientKey);
            }
        }
        Ok(Self(recipients))
    }
    pub fn payload(&self, principal: PrincipalId) -> Result<Hash64, ExistingEscrowFeeError> {
        self.0.get(&principal).copied().ok_or(ExistingEscrowFeeError::RecipientKey)
    }
}

/// A checked PRIVATE proposal, not a native spend/admission capability. The
/// native checker validates exact real Transaction outputs and conservation.
/// Ordinary UTXO existence, inputs and payer script authorization are separate.
pub struct ExistingEscrowFeeCheck {
    result: ResultId,
    plan: PalwEscrowSpendPlanV1,
}
impl ExistingEscrowFeeCheck {
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn plan(&self) -> &PalwEscrowSpendPlanV1 {
        &self.plan
    }
}

fn h(digest: Digest) -> Hash64 {
    Hash64::from_bytes(digest.0)
}

/// Run a current A5 projection through the existing money-engine calculator.
/// Its old FundingTerms ID/claim namespace remains LITERAL and SEPARATE from
/// the new FundingConsent ID/result. This is not a mapping that lets a Positive
/// claim spend a legacy escrow. No rooted-row authorization is returned here.
/// The fee schedule is cumulative in ACTUAL prior settlement order, so a late
/// lower-numbered source unit cannot rewrite an already consumed prefix.
#[allow(clippy::too_many_arguments)]
pub fn check_existing_escrow_fee_spend<A: AccountingAuthority>(
    accounting: &PrivateAccountingStore<A>,
    result: &AuthenticatedResult<'_>,
    projection: &FeeSettlementProjection,
    legacy_terms: &FundingTermsV1,
    escrow: &PalwFundedEscrowRowV1,
    recipients: &FeeRecipientKeys,
    transaction: &Transaction,
    input_amounts: &[u64],
    daa: u64,
    max_dues: usize,
) -> Result<ExistingEscrowFeeCheck, ExistingEscrowFeeError> {
    accounting.validate_projection(projection).map_err(ExistingEscrowFeeError::Accounting)?;
    if !transaction.id_is_current() {
        return Err(ExistingEscrowFeeError::TransactionIdentity);
    }
    let result = result.commit();
    let intent = projection.intent();
    if intent.scope != result.scope || intent.case != result.case_id() || intent.result != result.id() {
        return Err(ExistingEscrowFeeError::ResultBinding);
    }
    legacy_terms.check_v1().map_err(ExistingEscrowFeeError::LegacyTerms)?;
    // Do not place a new consent ID into a field defined as old terms.id().
    // These comparisons validate calculator operands, not native admission.
    if escrow.claim != legacy_terms.claim
        || escrow.contract_id != legacy_terms.id_v1()
        || escrow.amount_sompi != legacy_terms.funding_amount_sompi
        || escrow.payer_key_id != legacy_terms.payer_key_id
        || escrow.refund_payload != legacy_terms.refund_payload
        || escrow.payer_key_id != h(projection.payer().0)
        || escrow.accepted_daa < legacy_terms.accept_from_daa
        || escrow.accepted_daa > legacy_terms.accept_until_daa
        || daa < escrow.accepted_daa
    {
        return Err(ExistingEscrowFeeError::EscrowBinding);
    }
    let before = projection.before();
    let after = projection.after();
    if !before.conserved()
        || !after.conserved()
        || escrow.amount_sompi != before.initial
        || escrow.paid_sompi != before.paid_unique_service
        || escrow.burned_sompi != before.contractual_burn
        || escrow.returned_sompi != before.refunded
        || escrow.unreleased_v1().map_err(ExistingEscrowFeeError::Escrow)? != before.remaining
    {
        return Err(ExistingEscrowFeeError::Totals);
    }
    if projection.closure().is_some() && escrow.liability_open {
        return Err(ExistingEscrowFeeError::LiabilityStillOpen);
    }
    let mut dues = Vec::new();
    for (_, payment) in projection.payments() {
        // Zero-price units still dedupe in A5; they have no native output.
        if payment.amount == 0 {
            continue;
        }
        if dues.len() >= max_dues {
            return Err(ExistingEscrowFeeError::Budget);
        }
        dues.push(PalwEscrowDueV1 {
            kind: PalwEscrowDueKindV1::WorkPayment,
            payee: Some(recipients.payload(payment.recipient)?),
            amount_sompi: payment.amount,
        });
    }
    if after.contractual_burn > 0 {
        if dues.len() >= max_dues {
            return Err(ExistingEscrowFeeError::Budget);
        }
        dues.push(PalwEscrowDueV1 { kind: PalwEscrowDueKindV1::Burn, payee: None, amount_sompi: after.contractual_burn });
    }
    let schedule = if dues.is_empty() {
        None
    } else {
        Some(PalwEscrowFeeScheduleV1::new_v1(escrow.contract_id, dues, max_dues).map_err(|_| ExistingEscrowFeeError::Budget)?)
    };
    let plan = palw_escrow_qualified_spend_v1(escrow, transaction, input_amounts, daa, schedule.as_ref())
        .map_err(ExistingEscrowFeeError::Escrow)?;
    if plan.new.paid_sompi != after.paid_unique_service
        || plan.new.burned_sompi != after.contractual_burn
        || plan.new.returned_sompi != after.refunded
        || plan.new.unreleased_v1().map_err(ExistingEscrowFeeError::Escrow)? != after.remaining
    {
        // The existing deadline-return path must not silently waive an earned
        // service fee or refund a hold that A4 has not finished closing.
        return Err(ExistingEscrowFeeError::Totals);
    }
    Ok(ExistingEscrowFeeCheck { result: result.id(), plan })
}

#[cfg(test)]
mod tests;
