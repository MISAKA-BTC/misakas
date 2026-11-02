//! RFC-0046 fixed relay consent DATA. Neither this codec nor a valid signature
//! admits R, creates a future UTXO, binds F, pays fees or grants computation use.
use super::AssignmentId;
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{
        Digest, FundingConsentId, FundingScopeId, PUBKEY_BYTES, PrincipalId, ResultId, SIGNATURE_BYTES, hash, principal,
        verify_signature,
    },
    rows::{NativeHash64, NativeOutpoint},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const FUNDING_RELAY_TERMS_DOMAIN_V2: &[u8] = b"misaka/positive/funding-relay-terms/v2/r1";
pub const FUNDING_RELAY_REGISTER_CONTEXT_V2: &[u8] = b"misaka-positive/funding-relay-register/v2/r1";
pub const FUNDING_RELAY_MATERIAL_DOMAIN_V2: &[u8] = b"misaka/positive/funding-relay-material/v2/r1";
pub const FUNDING_RELAY_OPERATION_V2: u8 = 14;
pub const FUNDING_RELAY_MAX_TERMS_BYTES_V2: usize = 1087;
pub const FUNDING_RELAY_MAX_BODY_BYTES_V2: usize = 5 + FUNDING_RELAY_MAX_TERMS_BYTES_V2 + 2 * SIGNATURE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RelayFundingPurposeV2 {
    PosterA = 0,
    CaseC = 1,
    ProverJoin = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RelayTemplateV2 {
    NativeOp12NoCaseNoReceiptsV1 = 0,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RelayTargetV2 {
    InitialAssignment {
        bundle: Digest,
        poster_nonce: u64,
    } = 0,
    InitialCase {
        result: ResultId,
        funding_scope: FundingScopeId,
        consent: FundingConsentId,
    } = 1,
    InitialProver {
        prover: NativeOutpoint,
        join: Digest,
    } = 2,
    Continuation {
        original_funding: NativeOutpoint,
        predecessor: NativeOutpoint,
        parent_spend: NativeHash64,
        original_contract: Digest,
    } = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayTermsV2 {
    pub version: u16,
    pub revision: u16,
    pub contract_network: Digest,
    pub contract_ruleset: Digest,
    pub assignment: AssignmentId,
    pub contract_anchor: NativeHash64,
    pub relay_ruleset: Digest,
    pub relay_policy: Digest,
    pub payer_bond: NativeOutpoint,
    pub payer: PrincipalId,
    pub refund_payload: Digest,
    pub funding_purpose: RelayFundingPurposeV2,
    pub target: RelayTargetV2,
    pub funding_outpoint: NativeOutpoint,
    pub expected_amount: u64,
    pub template: RelayTemplateV2,
    pub register_before_daa: u64,
    pub funding_admission_before_daa: u64,
    pub custody_until_daa: u64,
    pub metadata_fee_schedule: Digest,
    pub physical_custody_profile: Digest,
    pub max_metadata_fee: u64,
}
impl FundingRelayTermsV2 {
    pub fn id(&self) -> Digest {
        hash(FUNDING_RELAY_TERMS_DOMAIN_V2, self)
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if [
            self.expected_amount,
            self.register_before_daa,
            self.funding_admission_before_daa,
            self.custody_until_daa,
            self.max_metadata_fee,
        ]
        .into_iter()
        .any(|n| n == 0 || n == u64::MAX)
        {
            return Err(ContractError::Range);
        }
        if self.register_before_daa > self.funding_admission_before_daa || self.funding_admission_before_daa >= self.custody_until_daa
        {
            return Err(ContractError::Range);
        }
        match (self.funding_purpose, self.target) {
            (RelayFundingPurposeV2::PosterA, RelayTargetV2::InitialAssignment { .. })
            | (RelayFundingPurposeV2::CaseC, RelayTargetV2::InitialCase { .. }) => {}
            (RelayFundingPurposeV2::ProverJoin, RelayTargetV2::InitialProver { prover, .. }) if prover == self.payer_bond => {}
            (_, RelayTargetV2::Continuation { original_funding, predecessor, parent_spend, .. })
                if parent_spend == self.funding_outpoint.transaction
                    && original_funding != self.funding_outpoint
                    && predecessor != self.funding_outpoint => {}
            _ => return Err(ContractError::Identity),
        }
        Ok(())
    }
    /// Consent identity only. Native must independently resolve bond/key,
    /// original scope, refund script, live permissions, actual fee and custody.
    pub fn authenticate_purpose(&self, key: &[u8; PUBKEY_BYTES], signature: &[u8; SIGNATURE_BYTES]) -> Result<(), ContractError> {
        self.validate_shape()?;
        if principal(key) != self.payer {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id(), signature, FUNDING_RELAY_REGISTER_CONTEXT_V2) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }
}

/// Exactly the append-only body14, with no arbitrary Vec or accepted origin.
/// Its separate native fence must be selected BEFORE calling this decoder.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayRegistrationBodyV2 {
    pub version: u16,
    pub revision: u16,
    pub operation: u8,
    pub terms: FundingRelayTermsV2,
    pub purpose_signature: [u8; SIGNATURE_BYTES],
    pub relay_signature: [u8; SIGNATURE_BYTES],
}
impl FundingRelayRegistrationBodyV2 {
    pub fn decode(raw: &[u8], maximum: u64) -> Result<Self, ContractError> {
        if maximum == 0 || maximum == u64::MAX || raw.len() as u64 > maximum || raw.len() > FUNDING_RELAY_MAX_BODY_BYTES_V2 {
            return Err(ContractError::Budget);
        }
        let body: Self = borsh::from_slice(raw).map_err(|_| ContractError::NonCanonical)?;
        if body.version != CONTRACT_VERSION || body.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if body.operation != FUNDING_RELAY_OPERATION_V2 {
            return Err(ContractError::NonCanonical);
        }
        body.terms.validate_shape()?;
        Ok(body)
    }
    pub fn material_id(&self) -> Digest {
        hash(FUNDING_RELAY_MATERIAL_DOMAIN_V2, self)
    }
}
