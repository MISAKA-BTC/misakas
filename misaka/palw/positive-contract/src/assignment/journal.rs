//! Result-free, bounded assignment journal framing. Hashes authenticate bytes,
//! never computation, accepted ancestry, a live lock or a recovered Pass.
use super::{
    AssignmentId,
    rows::{AssignmentJournalFrameRow, AssignmentJournalHeadRow},
};
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, hash},
    rows::{AcceptedMaterial, AcceptedOrigin},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const ASSIGNMENT_JOURNAL_FRAME_DOMAIN_V2: &[u8] = b"misaka/positive/assignment-journal-frame/v2/r1";
pub const ASSIGNMENT_JOURNAL_PAYLOAD_DOMAIN_V2: &[u8] = b"misaka/positive/assignment-journal-payload/v2/r1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentJournalEventKindV2 {
    Open,
    JoinProver,
    MetadataReceipt,
    Cancel,
    Expire,
    BindResult,
    Settlement,
}
/// The only pre-Case service purpose is actual admission/fixed metadata
/// custody. Data/prover receipt purposes cannot be decoded through this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentMetadataPurposeV2 {
    Admission,
    FixedMetadataRetention,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentMetadataReceiptV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub purpose: AssignmentMetadataPurposeV2,
    pub provider: crate::identity::PrincipalId,
    pub bytes: u64,
    pub work: u64,
    pub fee: u64,
    pub schedule: Digest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentJournalHeaderV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub ordinal: u64,
    pub previous: Digest,
    pub kind: AssignmentJournalEventKindV2,
    pub origin: AcceptedOrigin,
    pub payload_digest: Digest,
    pub payload_bytes: u32,
}
impl AssignmentJournalHeaderV2 {
    pub fn id(&self) -> Digest {
        hash(ASSIGNMENT_JOURNAL_FRAME_DOMAIN_V2, self)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.payload_bytes == 0 || self.ordinal == u64::MAX {
            return Err(ContractError::Range);
        }
        if (self.ordinal == 0) != (self.kind == AssignmentJournalEventKindV2::Open)
            || (self.ordinal == 0 && self.previous != Digest([0; 64]))
        {
            return Err(ContractError::NonCanonical);
        }
        Ok(())
    }
    pub fn check_payload(&self, payload: &[u8], max_bytes: u64) -> Result<(), ContractError> {
        self.validate()?;
        if max_bytes == 0 || max_bytes == u64::MAX || payload.len() as u64 > max_bytes || payload.len() != self.payload_bytes as usize
        {
            return Err(ContractError::Budget);
        }
        if assignment_payload_digest_v2(payload) != self.payload_digest {
            return Err(ContractError::Identity);
        }
        // New native records retain the actual pre-event boundary in a
        // distinct envelope. Legacy bounded DATA fixtures keep their original
        // representation; the native writer independently requires an envelope.
        let payload = if payload.starts_with(&super::control_material::ASSIGNMENT_CONTROL_MAGIC_V2) {
            super::control_material::AssignmentControlViewV2::decode(payload, max_bytes)?.inner()
        } else {
            payload
        };
        // Body operations retain their own complete bounded codec/authorization.
        // Metadata receipts have an independent typed payload, never op3/op4.
        if self.kind == AssignmentJournalEventKindV2::Open {
            super::open_material::AssignmentOpenViewV2::decode(
                payload,
                super::open_material::AssignmentOpenLimitsV2::within_payload(max_bytes),
            )?;
        } else if self.kind == AssignmentJournalEventKindV2::MetadataReceipt {
            let receipt = borsh::from_slice::<AssignmentMetadataReceiptV2>(payload).map_err(|_| ContractError::NonCanonical)?;
            if receipt.version != CONTRACT_VERSION
                || receipt.revision != CONTRACT_REVISION
                || receipt.assignment != self.assignment
                || receipt.bytes == 0
                || receipt.bytes == u64::MAX
                || receipt.work == 0
                || receipt.work == u64::MAX
                || receipt.fee == u64::MAX
                || receipt.schedule == Digest([0; 64])
            {
                return Err(ContractError::Range);
            }
        } else {
            let op = match self.kind {
                AssignmentJournalEventKindV2::Open => 8,
                AssignmentJournalEventKindV2::JoinProver => 13,
                AssignmentJournalEventKindV2::Cancel => 10,
                AssignmentJournalEventKindV2::Expire => 11,
                AssignmentJournalEventKindV2::BindResult => 9,
                AssignmentJournalEventKindV2::Settlement => 12,
                AssignmentJournalEventKindV2::MetadataReceipt => unreachable!(),
            };
            if payload.len() < 5 || payload[..5] != [2, 0, 1, 0, op] {
                return Err(ContractError::NonCanonical);
            }
        }
        Ok(())
    }
}
pub fn assignment_payload_digest_v2(payload: &[u8]) -> Digest {
    hash(ASSIGNMENT_JOURNAL_PAYLOAD_DOMAIN_V2, &payload)
}
impl AssignmentJournalFrameRow {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        self.header.validate()?;
        if self.payload.origin != self.header.origin || self.payload.digest != self.header.payload_digest {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
    /// Returns the next fixed head only when the caller's exact expected head
    /// matches. Native commits frame/material/head with all other state changes.
    pub fn append_to(
        &self,
        current: Option<&AssignmentJournalHeadRow>,
        retention_until_daa: u64,
    ) -> Result<AssignmentJournalHeadRow, ContractError> {
        self.validate_shape()?;
        if retention_until_daa == u64::MAX || retention_until_daa <= self.header.origin.accepted_daa {
            return Err(ContractError::Range);
        }
        match current {
            None if self.header.ordinal == 0 => {}
            Some(head)
                if head.assignment == self.header.assignment
                    && head.next_ordinal == self.header.ordinal
                    && head.last_frame == self.header.previous
                    && retention_until_daa == head.retention_until_daa
                    && self.header.origin.accepted_daa >= head.origin.accepted_daa =>
            {
                if head.origin.accepting_block == self.header.origin.accepting_block
                    && (head.origin.accepted_blue_score != self.header.origin.accepted_blue_score
                        || (head.origin.accepted_tx_ordinal, head.origin.event_ordinal)
                            >= (self.header.origin.accepted_tx_ordinal, self.header.origin.event_ordinal))
                {
                    return Err(ContractError::NonCanonical);
                }
            }
            _ => return Err(ContractError::NonCanonical),
        }
        Ok(AssignmentJournalHeadRow {
            assignment: self.header.assignment,
            next_ordinal: self.header.ordinal + 1,
            last_frame: self.header.id(),
            origin: self.header.origin,
            retention_until_daa,
        })
    }
}
/// Constructs a literal frame reference. This does not establish accepted origin.
pub fn assignment_frame_v2(
    assignment: AssignmentId,
    ordinal: u64,
    previous: Digest,
    kind: AssignmentJournalEventKindV2,
    origin: AcceptedOrigin,
    payload: &[u8],
    max_bytes: u64,
) -> Result<AssignmentJournalFrameRow, ContractError> {
    // Reject the untrusted extent before doing any payload hash work.
    if max_bytes == 0
        || max_bytes == u64::MAX
        || payload.is_empty()
        || payload.len() as u64 > max_bytes
        || payload.len() as u64 > u64::from(u32::MAX)
    {
        return Err(ContractError::Budget);
    }
    let header = AssignmentJournalHeaderV2 {
        version: CONTRACT_VERSION,
        revision: CONTRACT_REVISION,
        assignment,
        ordinal,
        previous,
        kind,
        origin,
        payload_digest: assignment_payload_digest_v2(payload),
        payload_bytes: u32::try_from(payload.len()).map_err(|_| ContractError::Budget)?,
    };
    header.check_payload(payload, max_bytes)?;
    Ok(AssignmentJournalFrameRow { payload: AcceptedMaterial { origin, digest: header.payload_digest }, header })
}

pub const ASSIGNMENT_JOURNAL_HEADER_BYTES_V2: usize = 425;
/// Borrowed fixed-header codec. No allocating Deserialize exists for a record.
pub struct AssignmentJournalRecordViewV2<'a> {
    header: AssignmentJournalHeaderV2,
    payload: &'a [u8],
}
impl<'a> AssignmentJournalRecordViewV2<'a> {
    pub fn decode(raw: &'a [u8], max_record_bytes: u64) -> Result<Self, ContractError> {
        if max_record_bytes == 0
            || max_record_bytes == u64::MAX
            || raw.len() as u64 > max_record_bytes
            || raw.len() <= ASSIGNMENT_JOURNAL_HEADER_BYTES_V2
        {
            return Err(ContractError::Budget);
        }
        let header = borsh::from_slice::<AssignmentJournalHeaderV2>(&raw[..ASSIGNMENT_JOURNAL_HEADER_BYTES_V2])
            .map_err(|_| ContractError::NonCanonical)?;
        let payload = &raw[ASSIGNMENT_JOURNAL_HEADER_BYTES_V2..];
        header.check_payload(payload, max_record_bytes)?;
        Ok(Self { header, payload })
    }
    pub fn header(&self) -> AssignmentJournalHeaderV2 {
        self.header
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub fn frame(&self) -> AssignmentJournalFrameRow {
        AssignmentJournalFrameRow {
            header: self.header,
            payload: AcceptedMaterial { origin: self.header.origin, digest: self.header.payload_digest },
        }
    }
}
/// Caller must reserve this owned buffer before calling. The borrowed decoder
/// is preferred for ingress/replay and never allocates from attacker lengths.
pub fn encode_assignment_journal_record_v2(
    header: AssignmentJournalHeaderV2,
    payload: &[u8],
    max_record_bytes: u64,
) -> Result<Vec<u8>, ContractError> {
    header.check_payload(payload, max_record_bytes)?;
    let bytes = ASSIGNMENT_JOURNAL_HEADER_BYTES_V2.checked_add(payload.len()).ok_or(ContractError::Budget)?;
    if bytes as u64 > max_record_bytes {
        return Err(ContractError::Budget);
    }
    let mut raw = Vec::new();
    raw.try_reserve_exact(bytes).map_err(|_| ContractError::Budget)?;
    header.serialize(&mut raw).map_err(|_| ContractError::NonCanonical)?;
    raw.extend_from_slice(payload);
    Ok(raw)
}
