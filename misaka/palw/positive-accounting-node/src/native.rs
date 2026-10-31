//! A5 fee projections checked by the EXISTING native funded-escrow engine.
//!
//! This module does not create a Positive native funding row, reinterpret TFO1's
//! execution root, install a schedule into a preset, skip ordinary transaction
//! scripts, or grant credit. A6/A4 must still commit the accepted transaction,
//! resource/case changes and the existing escrow delta atomically. The native
//! Positive admission/binding resolver remains unassigned.

use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, mldsa87_key_id},
    palw_funded_escrow_v1::{
        PalwEscrowDueKindV1, PalwEscrowDueV1, PalwEscrowFeeScheduleV1, PalwEscrowRefusalV1, PalwEscrowSpendPlanV1,
        PalwFundedEscrowRowV1, palw_escrow_qualified_spend_v1,
    },
    palw_state_v2::PalwChainStateV2,
    tx::Transaction,
};
use misaka_palw_positive_accounting::fees::{AccountingAuthority, AccountingError, FeeSettlementProjection, PrivateAccountingStore};
use misaka_palw_positive_contract::{
    identity::{Digest, PrincipalId, ResultId},
    messages::AuthenticatedResult,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExistingEscrowFeeError {
    Accounting(AccountingError),
    TransactionIdentity,
    ResultBinding,
    EscrowBinding,
    Totals,
    RecipientKey,
    LiabilityStillOpen,
    Budget,
    MissingRootedEscrow,
    RootedEscrowChanged,
    Escrow(PalwEscrowRefusalV1),
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

    /// A6 must recheck the exact currently rooted BEFORE row at commit time;
    /// copying a row into this private checker never establishes that it exists.
    pub fn recheck_rooted_before(&self, state: &PalwChainStateV2) -> Result<(), ExistingEscrowFeeError> {
        let current = state.funded_escrows_v1().get(&self.plan.old.contract_id).ok_or(ExistingEscrowFeeError::MissingRootedEscrow)?;
        if *current != self.plan.old {
            return Err(ExistingEscrowFeeError::RootedEscrowChanged);
        }
        Ok(())
    }
}

fn h(digest: Digest) -> Hash64 {
    Hash64::from_bytes(digest.0)
}

/// Connect a current A5 projection to the existing money engine. No second
/// integer settlement implementation or new settlement-leg ID is introduced.
/// The fee schedule is cumulative in ACTUAL prior settlement order, so a late
/// lower-numbered source unit cannot rewrite an already consumed prefix.
#[allow(clippy::too_many_arguments)]
pub fn check_existing_escrow_fee_spend<A: AccountingAuthority>(
    accounting: &PrivateAccountingStore<A>,
    result: &AuthenticatedResult<'_>,
    projection: &FeeSettlementProjection,
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
    // New result-derived claim IDs must be explicit. A legacy row's claim can
    // never be borrowed merely because the producer supplied its identifier.
    if escrow.claim != h(result.claim_id().0)
        || escrow.contract_id != h(intent.funding_consent)
        || escrow.payer_key_id != h(projection.payer().0)
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
