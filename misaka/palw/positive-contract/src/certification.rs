//! P-subject Gate C statements specified by RFC-0044. This authenticates a
//! verifier's statement, never the underlying computation or native eligibility.
//! Native draw rows, selection, liability, outcomes and credit remain unassigned
//! here. Old `Reproduced` statements are not reinterpreted as these statements.
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{hash, *},
    messages::MinerResultCommit,
    native::certification_subject,
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const POSITIVE_ATTESTATION_ID_DOMAIN_V2: &[u8] = b"misaka/positive/attestation-id/v2/r1";
pub const POSITIVE_ATTESTATION_SIGN_CONTEXT_V2: &[u8] = b"misaka/positive/attestation/v2/r1";
/// Two u16 versions, thirteen 64-byte digests and one verdict byte. This is
/// the fixed DTO body, excluding its separate ML-DSA-87 signature/carrier.
pub const POSITIVE_ATTESTATION_BYTES_V2: usize = 4 + 13 * 64 + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PositiveCourtVerdictV2 {
    /// The verifier independently checked the complete public Court material.
    /// Its vote is counted only under the admitted honest-fraction premise;
    /// neither this signature nor its transcript substitutes for Node checking.
    PositiveChecked,
    Unavailable,
    /// Failure to reproduce/check this claim's Court, not a model-level review
    /// or an independently adjudicated computation fault by the Miner.
    NotReproducible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCourtAttestationV2 {
    pub version: u16,
    pub revision: u16,
    pub scope: ChainScope,
    pub claim: ClaimId,
    pub result: ResultId,
    pub case: CaseId,
    pub subject: Digest,
    pub policy: Digest,
    /// ID of the actual native immutable draw RECORD: it must bind its roster,
    /// seed, policy and accepted origin. A raw beacon alone is insufficient.
    /// Its codec/derivation belong to the native owner, not this DTO module.
    pub draw_anchor: Digest,
    pub witness: WitnessId,
    /// Native kernel bond ID, independently resolved from the actual draw.
    pub verifier_bond: Digest,
    /// A0's key-derived principal. The native pool's operator identity is a
    /// separate value, used by native eligibility/exclusion rules.
    pub verifier: PrincipalId,
    pub verdict: PositiveCourtVerdictV2,
    /// The exact canonical full Court material history root/reference. All
    /// verdicts identify the SAME expected material, even if it was unavailable.
    /// Only `PositiveChecked` asserts that the verifier checked it completely.
    pub checked_material: Digest,
}

/// Expectations resolved by the native owner from the admitted full result,
/// selected draw, role/key assignment and canonical Court history. Constructing
/// this context from the peer's statement is NOT authentication of those facts.
/// This is neither a wire DTO nor an eligibility/draw/credit capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveAttestationExpectationsV2 {
    pub scope: ChainScope,
    pub policy: Digest,
    pub draw_anchor: Digest,
    pub witness: WitnessId,
    pub verifier_bond: Digest,
    pub verifier: PrincipalId,
    pub checked_material: Digest,
}

impl PositiveCourtAttestationV2 {
    pub fn id(&self) -> Digest {
        hash(POSITIVE_ATTESTATION_ID_DOMAIN_V2, self)
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        Ok(())
    }
    pub fn validate_binding(
        &self,
        result: &MinerResultCommit,
        expected: &PositiveAttestationExpectationsV2,
    ) -> Result<(), ContractError> {
        self.validate_shape()?;
        result.validate_shape()?;
        if result.scope != expected.scope
            || self.scope != expected.scope
            || self.claim != result.claim_id()
            || self.result != result.id()
            || self.case != result.case_id()
            || self.subject != certification_subject(result)
            || self.policy != expected.policy
            || self.draw_anchor != expected.draw_anchor
            || self.witness != expected.witness
            || self.verifier_bond != expected.verifier_bond
            || self.verifier != expected.verifier
            || self.checked_material != expected.checked_material
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Authenticates the literal statement only. The native fold must separately
    /// require an actual draw, live assignment, current branch/deadline, unique
    /// drawn bond, sufficient liability and the correct certification outcome.
    /// No negative vote automatically convicts the Miner, and no positive vote
    /// manufactures checked coverage or consensus credit.
    pub fn authenticate(
        &self,
        result: &MinerResultCommit,
        expected: &PositiveAttestationExpectationsV2,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        self.validate_binding(result, expected)?;
        if principal(key) != self.verifier {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id(), signature, POSITIVE_ATTESTATION_SIGN_CONTEXT_V2) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }

    /// Fixed local body codec; native envelope/tag allocation is not defined here.
    pub fn encode(&self) -> Result<[u8; POSITIVE_ATTESTATION_BYTES_V2], ContractError> {
        self.validate_shape()?;
        let mut bytes = [0; POSITIVE_ATTESTATION_BYTES_V2];
        let mut remaining = &mut bytes[..];
        borsh::to_writer(&mut remaining, self).map_err(|_| ContractError::NonCanonical)?;
        if !remaining.is_empty() {
            return Err(ContractError::NonCanonical);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != POSITIVE_ATTESTATION_BYTES_V2 {
            return Err(ContractError::NonCanonical);
        }
        let value: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        value.validate_shape()?;
        Ok(value)
    }
}
