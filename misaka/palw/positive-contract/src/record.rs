//! Primitive-only interchange of a Panel's existing TIR execution record.
//! Registered class, program and job are deliberately absent: A1 reconstructs
//! its TIR binding from the independently resolved native context. This codec
//! establishes framing/bindings only, never arithmetic truth or native credit.
//! A third party can corrupt a transported record. Codec/binding errors reject
//! that delivery; alone they neither reject the signed Witness nor consume a
//! Panel attempt, and never attribute a calculation fault to the Miner.
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, ResultId},
    messages::{MinerResultCommit, PanelWitnessCommit},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const RECORD_MAGIC: &[u8; 8] = b"PC2R1REC";
pub const RECORD_HEADER_BYTES: usize = 4 + 64 + 4 * 64 + 8;
pub const RECORD_OVERHEAD_BYTES: usize = RECORD_MAGIC.len() + RECORD_HEADER_BYTES + 4;

/// Fixed-size DTO. These are the roots from the Panel's existing TIR capture,
/// not a second commitment algorithm and not a Miner-owned trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RecordHeader {
    pub version: u16,
    pub revision: u16,
    pub result: ResultId,
    pub artifact: Digest,
    pub full_logits_trace_root: Digest,
    pub step_leaf_count: u64,
    pub step_merkle_root: Digest,
    pub committed_execution_root: Digest,
}
impl RecordHeader {
    pub fn validate_version(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        Ok(())
    }
}

/// Validated framing with borrowed four-byte token IDs. There is no Vec or
/// deserializer which allocates from the peer's declared generated count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionRecord<'a> {
    header: RecordHeader,
    generated: &'a [u8],
}
impl<'a> ExecutionRecord<'a> {
    pub fn decode(bytes: &'a [u8], max_record_bytes: u64, expected_decode: u32) -> Result<Self, ContractError> {
        if bytes.len() < RECORD_OVERHEAD_BYTES || bytes.len() as u64 > max_record_bytes {
            return Err(ContractError::Budget);
        }
        if &bytes[..RECORD_MAGIC.len()] != RECORD_MAGIC {
            return Err(ContractError::Version);
        }
        let header: RecordHeader = borsh::from_slice(&bytes[RECORD_MAGIC.len()..RECORD_MAGIC.len() + RECORD_HEADER_BYTES])
            .map_err(|_| ContractError::NonCanonical)?;
        header.validate_version()?;
        let count_start = RECORD_MAGIC.len() + RECORD_HEADER_BYTES;
        let count = u32::from_le_bytes(bytes[count_start..RECORD_OVERHEAD_BYTES].try_into().map_err(|_| ContractError::NonCanonical)?);
        if expected_decode == 0 || count != expected_decode {
            return Err(ContractError::Range);
        }
        let required = wire_size(count)?;
        if required > max_record_bytes {
            return Err(ContractError::Budget);
        }
        if required != bytes.len() as u64 {
            return Err(ContractError::NonCanonical);
        }
        Ok(Self { header, generated: &bytes[RECORD_OVERHEAD_BYTES..] })
    }

    pub fn header(&self) -> RecordHeader {
        self.header
    }
    pub fn generated_bytes(&self) -> &'a [u8] {
        self.generated
    }
    pub fn generated_count(&self) -> u32 {
        // Decode first checked the exact extent against its u32 count.
        (self.generated.len() / 4) as u32
    }
    pub fn token(&self, index: u32) -> Option<u32> {
        let start = usize::try_from(index).ok()?.checked_mul(4)?;
        let end = start.checked_add(4)?;
        Some(u32::from_le_bytes(self.generated.get(start..end)?.try_into().ok()?))
    }
    pub fn tokens(&self) -> impl ExactSizeIterator<Item = u32> + DoubleEndedIterator + 'a {
        self.generated.chunks_exact(4).map(|bytes| u32::from_le_bytes(bytes.try_into().expect("validated four-byte token")))
    }
    pub fn wire_bytes(&self) -> u64 {
        RECORD_OVERHEAD_BYTES as u64 + self.generated.len() as u64
    }

    /// DTO-level cross-binding only. A1 must still verify that the independently
    /// reconstructed class/job plus these roots form a valid TIR binding, and
    /// derive candidate_output from the decoded IDs using the existing core
    /// output rule. No candidate output or registered program is trusted here.
    /// An error here is an unauthenticated transport refusal, not a verdict on
    /// the signed Panel witness. Only A1's properly attributed checks can do that.
    pub fn validate_binding(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<(), ContractError> {
        result.validate_shape()?;
        witness.validate_binding(result)?;
        if self.header.result != result.id()
            || self.header.artifact != result.artifact
            || self.header.committed_execution_root != witness.execution_record
        {
            return Err(ContractError::Identity);
        }
        if self.generated_count() != result.decode {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}

pub fn wire_size(count: u32) -> Result<u64, ContractError> {
    (count as u64).checked_mul(4).and_then(|n| n.checked_add(RECORD_OVERHEAD_BYTES as u64)).ok_or(ContractError::Budget)
}

/// A3 writes exactly the roots and IDs from its retained capture. No class,
/// program, job, logits values or dtype conversion is introduced by this codec.
/// `expected_decode` is the A0 result's fixed decode count, not a source nonce.
pub fn encode_record(
    header: RecordHeader,
    generated: &[u32],
    expected_decode: u32,
    max_record_bytes: u64,
) -> Result<Vec<u8>, ContractError> {
    header.validate_version()?;
    if expected_decode == 0 || generated.len() as u64 != expected_decode as u64 {
        return Err(ContractError::Range);
    }
    let length = wire_size(expected_decode)?;
    if length > max_record_bytes {
        return Err(ContractError::Budget);
    }
    let capacity = usize::try_from(length).map_err(|_| ContractError::Budget)?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(RECORD_MAGIC);
    header.serialize(&mut out).map_err(|_| ContractError::NonCanonical)?;
    out.extend_from_slice(&expected_decode.to_le_bytes());
    for token in generated {
        out.extend_from_slice(&token.to_le_bytes());
    }
    debug_assert_eq!(out.len(), capacity);
    Ok(out)
}
