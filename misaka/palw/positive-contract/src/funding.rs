//! Payer authorization for a light result, before any Panel witness exists.
//! Signatures authorize terms only: actual UTXO/output qualification and locking
//! belong to the native acceptor and its single atomic state/UTXO transaction.
use crate::identity::hash;
use crate::{CONTRACT_REVISION, CONTRACT_VERSION, ContractError, identity::*, messages::*};
use borsh::{BorshDeserialize, BorshSerialize};

/// Derived independently by A1/A4 from the registered program/canonical job and
/// adopted policy. These are not a peer's claimed resource or price authority.
/// No field may hash the final ResultId, ServiceId, FundingScopeId, consent ID or
/// carrying transaction ID back into this pre-consent basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingPlanBasis {
    pub profile: CheckProfile,
    pub derivation: Digest,
    pub mandatory_relations: u64,
    pub source_catalog: Digest,
    pub resource_quote: Digest,
    pub policy: Digest,
    pub schedule: Digest,
    pub required_fee: u64,
    pub required_capital: u64,
}

/// Shared source-due DATA, not a payment or admission capability. The native
/// one-writer must re-resolve the canonical rate/provider, first receipt,
/// funding/leases/current generation and full before-images atomically.
/// No new wire identity: the existing ServiceReceipt key is the once-only key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFeeDeltaIntent {
    pub receipt_key: crate::native::NativeRowKey,
    pub result: ResultId,
    pub funding_scope: FundingScopeId,
    pub schedule: Digest,
    pub provider: PrincipalId,
    pub amount: u64,
    pub accepted_response: crate::rows::AcceptedMaterial,
}

/// Explicit acyclic projection. The FULL Miner result and service contract are
/// still signed/hashed normally. Only their two self-referential link fields
/// are zeroed in this SEPARATE domain; every other literal field is included.
pub fn funding_scope(result: &MinerResultCommit, service: &MinerServiceContract, basis: &FundingPlanBasis) -> FundingScopeId {
    let mut result = result.clone();
    result.service_contract = Digest([0; 64]);
    let mut service = *service;
    service.funding_consent = Digest([0; 64]);
    FundingScopeId(hash(b"misaka/positive/funding-scope/v2/r1", &(result, service, basis)))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingConsent {
    pub version: u16,
    pub revision: u16,
    pub case: CaseId,
    pub scope: FundingScopeId,
    pub payer: PrincipalId,
    pub max_fees: u64,
    pub max_locked: u64,
    /// Index in the actual carrier's outputs. Never its own transaction ID.
    pub funding_output_index: u32,
    /// Existing native ML-DSA P2PKH address payload, NOT the unkeyed key ID.
    pub refund_payload: Digest,
    /// Last admissible clock, exclusive. Not a refund/lock release deadline.
    pub admission_expiry: u64,
}
impl FundingConsent {
    pub fn id(&self) -> FundingConsentId {
        FundingConsentId(hash(b"misaka/positive/funding-id/v2/r1", self))
    }
    pub fn validate(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        basis: &FundingPlanBasis,
        now: u64,
    ) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        result.validate_shape()?;
        service.validate(result, now)?;
        if basis.profile != CheckProfile::ExactStreamV1 {
            return Err(ContractError::UnsupportedProfile);
        }
        if self.case != result.case_id()
            || self.scope != funding_scope(result, service, basis)
            || service.funding_consent != self.id().0
            || basis.source_catalog != service.source_catalog
        {
            return Err(ContractError::Identity);
        }
        let required = basis.required_fee.checked_add(basis.required_capital).ok_or(ContractError::Budget)?;
        if basis.mandatory_relations == 0
            || now >= self.admission_expiry
            || self.admission_expiry > service.deadline
            || self.max_fees > self.max_locked
            || basis.required_fee > self.max_fees
            || required > self.max_locked
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn authenticate<'a>(
        &'a self,
        result: &'a MinerResultCommit,
        service: &'a MinerServiceContract,
        basis: &'a FundingPlanBasis,
        now: u64,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<AuthenticatedFunding<'a>, ContractError> {
        self.validate(result, service, basis, now)?;
        if principal(key) != self.payer {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id().0, signature, FUNDING_SIGN_CONTEXT) {
            return Err(ContractError::Signature);
        }
        Ok(AuthenticatedFunding { consent: self, result, service, basis })
    }
}

/// Literal payer signature checked, not a funding receipt, a reserved resource,
/// native admission, a lock, or spend authorization. No native success methods.
pub struct AuthenticatedFunding<'a> {
    consent: &'a FundingConsent,
    result: &'a MinerResultCommit,
    service: &'a MinerServiceContract,
    basis: &'a FundingPlanBasis,
}
impl<'a> AuthenticatedFunding<'a> {
    pub fn consent(&self) -> &'a FundingConsent {
        self.consent
    }
    pub fn result(&self) -> &'a MinerResultCommit {
        self.result
    }
    pub fn service(&self) -> &'a MinerServiceContract {
        self.service
    }
    pub fn basis(&self) -> &'a FundingPlanBasis {
        self.basis
    }
}
