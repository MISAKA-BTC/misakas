//! Private interchange framing, NOT an allocated native transaction/lifecycle tag.
//! Commit DTOs are flat/fixed-size. Evidence is borrowed and bounded before any
//! inner G14 decoder is invoked by A1/A4.
use crate::{ContractError, identity::*, messages::*};
use borsh::{BorshDeserialize, BorshSerialize};

const DTO_MAGIC: &[u8; 8] = b"PC2R1DTO";
const EVIDENCE_MAGIC: &[u8; 8] = b"PC2R1EVD";
pub const MAX_COMMIT_BYTES: usize = 4096;
pub const EVIDENCE_HEADER_BYTES: usize = 8 + 4 * 64 + 8 + 8 + 4;

fn encode<T: BorshSerialize>(kind: u8, value: &T) -> Result<Vec<u8>, ContractError> {
    let bytes = borsh::to_vec(value).map_err(|_| ContractError::NonCanonical)?;
    if bytes.len() > MAX_COMMIT_BYTES {
        return Err(ContractError::Budget);
    }
    let mut out = Vec::with_capacity(9 + bytes.len());
    out.extend_from_slice(DTO_MAGIC);
    out.push(kind);
    out.extend_from_slice(&bytes);
    Ok(out)
}
fn decode<T: BorshDeserialize>(kind: u8, bytes: &[u8]) -> Result<T, ContractError> {
    if bytes.len() < 9 || bytes.len() > 9 + MAX_COMMIT_BYTES {
        return Err(ContractError::Budget);
    }
    if &bytes[..8] != DTO_MAGIC || bytes[8] != kind {
        return Err(ContractError::Version);
    }
    // Only the three fixed-size, Vec-free DTOs below can reach this generic helper.
    borsh::from_slice(&bytes[9..]).map_err(|_| ContractError::NonCanonical)
}
pub fn encode_result(value: &MinerResultCommit) -> Result<Vec<u8>, ContractError> {
    encode(0, value)
}
pub fn encode_witness(value: &PanelWitnessCommit) -> Result<Vec<u8>, ContractError> {
    encode(1, value)
}
pub fn encode_request(value: &SourceRequest) -> Result<Vec<u8>, ContractError> {
    encode(2, value)
}
pub fn decode_result(bytes: &[u8]) -> Result<MinerResultCommit, ContractError> {
    let result: MinerResultCommit = decode(0, bytes)?;
    result.validate_shape()?;
    Ok(result)
}
pub fn decode_witness(bytes: &[u8]) -> Result<PanelWitnessCommit, ContractError> {
    let result: PanelWitnessCommit = decode(1, bytes)?;
    version(result.version, result.revision)?;
    Ok(result)
}
pub fn decode_request(bytes: &[u8]) -> Result<SourceRequest, ContractError> {
    let result: SourceRequest = decode(2, bytes)?;
    version(result.version, result.revision)?;
    if result.first >= result.end {
        return Err(ContractError::Range);
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvidenceHeader {
    pub case: CaseId,
    pub result: ResultId,
    pub witness: WitnessId,
    pub plan: PlanId,
    pub first_relation: u64,
    pub relation_count: u64,
    pub payload_bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceFrame<'a> {
    header: EvidenceHeader,
    payload: &'a [u8],
}
/// Authorship of these exact bytes only. This is not native admission, Panel
/// draw membership, delivery acceptance, an arithmetic verdict, or fee authority.
/// A4 resolves the already authenticated witness and key from its rooted state.
pub struct AuthenticatedEvidenceFrame<'a> {
    frame: EvidenceFrame<'a>,
    author: PrincipalId,
}
impl<'a> AuthenticatedEvidenceFrame<'a> {
    pub fn frame(&self) -> EvidenceFrame<'a> {
        self.frame
    }
    pub fn author(&self) -> PrincipalId {
        self.author
    }
    pub fn id(&self) -> EvidencePartId {
        self.frame.id()
    }
}
impl EvidenceHeader {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.relation_count == 0 || self.first_relation.checked_add(self.relation_count).is_none() {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}
impl<'a> EvidenceFrame<'a> {
    /// Digest of the canonical whole frame, streamed without copying the opaque
    /// G14 payload. The witness ID in the header fixes the literal Panel author.
    pub fn id(&self) -> EvidencePartId {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka/positive/evidence-frame/v2/r1").to_state();
        state.update(EVIDENCE_MAGIC);
        state.update(&borsh::to_vec(&self.header).expect("fixed evidence header"));
        state.update(self.payload);
        EvidencePartId(Digest(state.finalize().as_bytes().try_into().expect("64")))
    }
    pub fn signed_wire_bytes(&self) -> u64 {
        self.wire_bytes() + SIGNATURE_BYTES as u64
    }
    /// An untrusted relay must not be able to reject an honest witness by
    /// submitting bad openings on its behalf. Authenticate BEFORE NodeChecker.
    pub fn authenticate(
        self,
        witness: &PanelWitnessCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        max_signed_bytes: u64,
    ) -> Result<AuthenticatedEvidenceFrame<'a>, ContractError> {
        if self.signed_wire_bytes() > max_signed_bytes {
            return Err(ContractError::Budget);
        }
        witness.validate_shape()?;
        if self.header.witness != witness.id()
            || self.header.result != witness.result
            || self.header.case != witness.case
            || self.header.plan != witness.plan
        {
            return Err(ContractError::Identity);
        }
        if principal(key) != witness.author {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id().0, signature, EVIDENCE_SIGN_CONTEXT) {
            return Err(ContractError::Signature);
        }
        Ok(AuthenticatedEvidenceFrame { frame: self, author: witness.author })
    }
    pub fn new(header: EvidenceHeader, payload: &'a [u8], max_frame_bytes: u64) -> Result<Self, ContractError> {
        header.validate()?;
        let length = EVIDENCE_HEADER_BYTES.checked_add(payload.len()).ok_or(ContractError::Budget)?;
        if length as u64 > max_frame_bytes {
            return Err(ContractError::Budget);
        }
        if payload.len() as u64 != header.payload_bytes as u64 {
            return Err(ContractError::NonCanonical);
        }
        Ok(Self { header, payload })
    }
    pub fn header(&self) -> EvidenceHeader {
        self.header
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub fn wire_bytes(&self) -> u64 {
        EVIDENCE_HEADER_BYTES as u64 + self.payload.len() as u64
    }
    pub fn decode(bytes: &'a [u8], max_frame_bytes: u64) -> Result<Self, ContractError> {
        if bytes.len() as u64 > max_frame_bytes || bytes.len() < EVIDENCE_HEADER_BYTES {
            return Err(ContractError::Budget);
        }
        if &bytes[..8] != EVIDENCE_MAGIC {
            return Err(ContractError::Version);
        }
        let header: EvidenceHeader = borsh::from_slice(&bytes[8..EVIDENCE_HEADER_BYTES]).map_err(|_| ContractError::NonCanonical)?;
        header.validate()?;
        let payload = &bytes[EVIDENCE_HEADER_BYTES..];
        if payload.len() as u64 != header.payload_bytes as u64 {
            return Err(ContractError::NonCanonical);
        }
        Self::new(header, payload, max_frame_bytes)
    }
    pub fn encode(&self, max_frame_bytes: u64) -> Result<Vec<u8>, ContractError> {
        self.header.validate()?;
        let len = EVIDENCE_HEADER_BYTES.checked_add(self.payload.len()).ok_or(ContractError::Budget)?;
        if len as u64 > max_frame_bytes || self.header.payload_bytes as u64 != self.payload.len() as u64 {
            return Err(ContractError::Budget);
        }
        let mut bytes = Vec::with_capacity(len);
        bytes.extend_from_slice(EVIDENCE_MAGIC);
        bytes.extend_from_slice(&borsh::to_vec(&self.header).map_err(|_| ContractError::NonCanonical)?);
        bytes.extend_from_slice(self.payload);
        Ok(bytes)
    }
}
