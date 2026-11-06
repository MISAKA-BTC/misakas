//! A0's TIR v1 interchange annex. Existing G14 cone/trace bytes are opaque here;
//! A1 must bounded-scan and authenticate them before computing. No new Merkle
//! source tree, arithmetic verifier or native transaction tag is defined.
use crate::identity::hash;
use crate::wire::{EVIDENCE_HEADER_BYTES, EvidenceFrame};
use crate::{ContractError, identity::*, messages::MinerResultCommit};

pub const EXACT_PART_HEADER_BYTES: usize = 16;
const MAGIC: &[u8; 8] = b"PC2R1G14";
const ABSENT: u32 = u32::MAX;

/// Raw cone+trace allowance inside both private wrappers. A1's raw-part meter
/// and A4's total-frame reservation must not silently use the same number.
pub fn raw_part_budget(max_frame_bytes: u64) -> Result<u64, ContractError> {
    max_frame_bytes
        .min(EVIDENCE_HEADER_BYTES as u64 + u32::MAX as u64)
        .checked_sub((EVIDENCE_HEADER_BYTES + EXACT_PART_HEADER_BYTES) as u64)
        .filter(|x| *x > 0)
        .ok_or(ContractError::Budget)
}
pub fn decode_frame<'a>(frame: &EvidenceFrame<'a>, max_frame_bytes: u64) -> Result<ExactPart<'a>, ContractError> {
    if frame.wire_bytes() > max_frame_bytes {
        return Err(ContractError::Budget);
    }
    if frame.header().relation_count != 1 {
        return Err(ContractError::NonCanonical);
    }
    ExactPart::decode(frame.payload(), max_frame_bytes.checked_sub(EVIDENCE_HEADER_BYTES as u64).ok_or(ContractError::Budget)?)
}

pub fn exact_derivation_id() -> Digest {
    hash(
        b"misaka/positive/tir-plan-order/v2/r1",
        &"TIR-v1/all-nodes-committed/canonical-step-leaves/fixed-hist-checkpoints/all-logits-lanes/greedy-lowest-index/i32-tiled/one-relation-per-frame",
    )
}
pub fn greedy_selector_id() -> Digest {
    hash(b"misaka/positive/tir-selector/v2/r1", &"TIR-v1/argmax-i32/ties-lowest-index/temperature-zero")
}
/// Declaration of the immutable source identities, not proof of availability,
/// arithmetic truth, service completion, or a replacement for G14 openings.
pub fn provenance_id(result: &MinerResultCommit) -> Digest {
    hash(
        b"misaka/positive/tir-provenance/v2/r1",
        &(
            result.registered_class,
            result.artifact,
            result.program,
            result.input,
            result.tokenizer,
            result.execution_rules,
            result.dependencies,
        ),
    )
}

/// One canonical step leaf's existing G14 cone and, only for logits, trace
/// disclosure. No duplicate binding/class/program encoding is invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactPart<'a> {
    cone: &'a [u8],
    trace: Option<&'a [u8]>,
}
impl<'a> ExactPart<'a> {
    pub fn new(cone: &'a [u8], trace: Option<&'a [u8]>, max_payload_bytes: u64) -> Result<Self, ContractError> {
        if cone.is_empty() || trace.is_some_and(|x| x.is_empty()) {
            return Err(ContractError::NonCanonical);
        }
        if cone.len() >= ABSENT as usize || trace.is_some_and(|x| x.len() >= ABSENT as usize) {
            return Err(ContractError::Budget);
        }
        let n = EXACT_PART_HEADER_BYTES
            .checked_add(cone.len())
            .and_then(|n| n.checked_add(trace.map_or(0, <[u8]>::len)))
            .ok_or(ContractError::Budget)?;
        if n as u64 > max_payload_bytes || n as u64 > u32::MAX as u64 {
            return Err(ContractError::Budget);
        }
        Ok(Self { cone, trace })
    }
    pub fn cone(&self) -> &'a [u8] {
        self.cone
    }
    pub fn trace(&self) -> Option<&'a [u8]> {
        self.trace
    }
    pub fn wire_bytes(&self) -> usize {
        EXACT_PART_HEADER_BYTES + self.cone.len() + self.trace.map_or(0, <[u8]>::len)
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.wire_bytes());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(self.cone.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.trace.map_or(ABSENT, |x| x.len() as u32).to_le_bytes());
        out.extend_from_slice(self.cone);
        if let Some(trace) = self.trace {
            out.extend_from_slice(trace);
        }
        out
    }
    pub fn decode(bytes: &'a [u8], max_payload_bytes: u64) -> Result<Self, ContractError> {
        if bytes.len() < EXACT_PART_HEADER_BYTES || bytes.len() as u64 > max_payload_bytes {
            return Err(ContractError::Budget);
        }
        if &bytes[..8] != MAGIC {
            return Err(ContractError::Version);
        }
        let cone = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| ContractError::NonCanonical)?) as usize;
        let trace = u32::from_le_bytes(bytes[12..16].try_into().map_err(|_| ContractError::NonCanonical)?);
        let end = EXACT_PART_HEADER_BYTES.checked_add(cone).ok_or(ContractError::Budget)?;
        let total = end.checked_add(if trace == ABSENT { 0 } else { trace as usize }).ok_or(ContractError::Budget)?;
        if total != bytes.len() {
            return Err(ContractError::NonCanonical);
        }
        Self::new(&bytes[EXACT_PART_HEADER_BYTES..end], if trace == ABSENT { None } else { Some(&bytes[end..]) }, max_payload_bytes)
    }
}
