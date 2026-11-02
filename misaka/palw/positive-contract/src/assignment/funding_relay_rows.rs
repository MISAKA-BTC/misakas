//! RFC-0046 retained relay DATA. A complete fixed package survives historical
//! raw pruning; it still requires actual native lineage, fee and custody replay.
//! These values create no UTXO, binding capability, service receipt or payment.
use super::{funding_relay::*, funding_relay_policy::*};
use crate::{
    ContractError,
    identity::{Digest, PUBKEY_BYTES, SIGNATURE_BYTES, principal},
    rows::{AcceptedMaterial, AcceptedOrigin, NativeOutpoint},
};
use borsh::{BorshDeserialize, BorshSerialize};

/// One purpose signature, distinct from the normal transaction signature in
/// the other row. All references must be resolved by native restore/admission.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayAuthorizationRowV2 {
    pub terms: FundingRelayTermsV2,
    pub payer_key: [u8; PUBKEY_BYTES],
    pub purpose_signature: [u8; SIGNATURE_BYTES],
    pub origin: AcceptedOrigin,
    pub material: AcceptedMaterial,
    pub metadata_quote: AcceptedMaterial,
    pub metadata_charge: AcceptedMaterial,
    /// Complete ORIGINAL preimages and quote, retained even after raw pruning.
    /// Native must recompute the usage from actual construction/staging.
    pub policy: FundingRelayPolicyV2,
    pub fees: FundingRelayFeeScheduleV2,
    pub physical: FundingRelayPhysicalProfileV2,
    pub quote: FundingRelayMoneyQuoteV2,
}
impl FundingRelayAuthorizationRowV2 {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        self.terms.validate_shape()?;
        let quote = self.fees.quote(&self.policy, &self.physical, &self.terms, self.quote.usage, self.origin.accepted_daa)?;
        if quote != self.quote || self.metadata_quote.digest != quote.id() {
            return Err(ContractError::Identity);
        }
        if principal(&self.payer_key) != self.terms.payer
            || self.origin.accepted_daa >= self.terms.register_before_daa
            || self.material.origin != self.origin
            || self.metadata_quote.origin != self.origin
            || self.metadata_charge.origin != self.origin
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}

/// Only actual F/S acceptance may write this value. Its shape and signatures
/// alone do not establish that the named native carrier was ever accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayBindingV2 {
    pub origin: AcceptedOrigin,
    pub original_funding: NativeOutpoint,
    pub predecessor: Option<NativeOutpoint>,
    pub original_contract: Digest,
    /// Actual account current-slot reservation, separate from pending R custody.
    pub reservation: AcceptedMaterial,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayCustodyRowV2 {
    pub funding_outpoint: NativeOutpoint,
    pub purpose: RelayFundingPurposeV2,
    pub terms: Digest,
    pub relay_signature: [u8; SIGNATURE_BYTES],
    pub material: AcceptedMaterial,
    pub reservation: AcceptedMaterial,
    pub retention_until_daa: u64,
    pub binding: Option<FundingRelayBindingV2>,
}
impl FundingRelayCustodyRowV2 {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.retention_until_daa == 0 || self.retention_until_daa == u64::MAX {
            return Err(ContractError::Range);
        }
        if self.reservation.origin != self.material.origin {
            return Err(ContractError::Identity);
        }
        if let Some(binding) = self.binding
            && (binding.origin.carrier_transaction != self.funding_outpoint.transaction
                || binding.origin.accepted_daa < self.material.origin.accepted_daa
                || binding.reservation.origin != binding.origin)
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }

    /// Crosslinks and exact literal material only. In particular this does not
    /// authenticate a stored AcceptedOrigin, native custody or a future output.
    pub fn check_pair(&self, auth: &FundingRelayAuthorizationRowV2) -> Result<(), ContractError> {
        self.validate_shape()?;
        auth.validate_shape()?;
        if self.funding_outpoint != auth.terms.funding_outpoint
            || self.purpose != auth.terms.funding_purpose
            || self.terms != auth.terms.id()
            || self.material != auth.material
            || self.retention_until_daa < auth.terms.custody_until_daa
        {
            return Err(ContractError::Identity);
        }
        let literal = FundingRelayRegistrationBodyV2 {
            version: 2,
            revision: 1,
            operation: FUNDING_RELAY_OPERATION_V2,
            terms: auth.terms,
            purpose_signature: auth.purpose_signature,
            relay_signature: self.relay_signature,
        };
        if literal.material_id() != self.material.digest {
            return Err(ContractError::Identity);
        }
        if let Some(binding) = self.binding {
            if binding.origin.accepted_daa >= auth.terms.funding_admission_before_daa {
                return Err(ContractError::Range);
            }
            match auth.terms.target {
                RelayTargetV2::Continuation { original_funding, predecessor, original_contract, .. }
                    if binding.original_funding == original_funding
                        && binding.predecessor == Some(predecessor)
                        && binding.original_contract == original_contract => {}
                RelayTargetV2::InitialAssignment { .. } | RelayTargetV2::InitialCase { .. } | RelayTargetV2::InitialProver { .. }
                    if binding.original_funding == self.funding_outpoint && binding.predecessor.is_none() => {}
                _ => return Err(ContractError::Identity),
            }
        }
        Ok(())
    }
}
