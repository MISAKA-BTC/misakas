//! A4 proposal for the bounded body carried by A0's reserved object 176.
//!
//! This file allocates NO lifecycle tag, fence, row or acceptance authority.
//! Borsh layout: LE u16 version (2), LE u16 revision (1), local u8 operation
//! (0..=7), then the fields below in declaration order. Signatures are fixed
//! arrays; keys come from rooted registration. Every Vec has a LE u32 length.
//!
//! Successful decoding establishes only bounded canonical syntax. Admission,
//! funding/UTXO locks, signatures, existing G14 response authentication, A1 inner
//! evidence checking and all status changes remain mandatory separate steps.
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    certification::{POSITIVE_ATTESTATION_BYTES_V2, PositiveCourtAttestationV2},
    funding::{FundingConsent, FundingPlanBasis},
    identity::{CaseId, PrincipalId, RequestId, ResultId, SIGNATURE_BYTES, WitnessId},
    messages::{MinerResultCommit, MinerServiceContract, PanelWitnessCommit, SourceRequest},
    wire::EVIDENCE_HEADER_BYTES,
};
use std::io::{self, Read};

/// Proposed codec defense ceilings, NOT measured/adopted execution profiles or
/// public network parameters. Native activation and A0 decoder registration are
/// outside this module. A rooted runtime policy must choose limits no larger.
pub const MAX_BODY_BYTES_V2: u64 = 16 * 1024 * 1024;
pub const MAX_OPAQUE_BYTES_V2: u64 = 8 * 1024 * 1024;
pub const MAX_EXACT_RELATIONS_V2: u64 = 4096;
pub const MAX_SIGNATURES_V2: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBodyLimitsV2 {
    pub max_body_bytes: u64,
    pub max_opaque_bytes: u64,
    pub max_exact_relations: u64,
    /// Fixed outer signatures only. Mandatory inner A1/G14 preflight must add
    /// any opaque record/response signatures and work to the native quote.
    pub max_signatures: u64,
}
const CODEC_CEILINGS: NativeBodyLimitsV2 = NativeBodyLimitsV2 {
    max_body_bytes: MAX_BODY_BYTES_V2,
    max_opaque_bytes: MAX_OPAQUE_BYTES_V2,
    max_exact_relations: MAX_EXACT_RELATIONS_V2,
    max_signatures: MAX_SIGNATURES_V2,
};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveNativeBodyV2 {
    pub version: u16,
    pub revision: u16,
    pub operation: PositiveNativeOperationV2,
}
/// These discriminants are local to the proposed body, not global event tags.
/// Append a future kind through A0; never reinterpret or renumber one of these.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PositiveNativeOperationV2 {
    ResultAdmission {
        result: MinerResultCommit,
        miner_signature: [u8; SIGNATURE_BYTES],
        service: MinerServiceContract,
        basis: FundingPlanBasis,
        consent: FundingConsent,
        payer_signature: [u8; SIGNATURE_BYTES],
    } = 0,
    WitnessCommit {
        witness: PanelWitnessCommit,
        panel_signature: [u8; SIGNATURE_BYTES],
        /// Opaque bounded existing record bytes. No inner binding/generated
        /// layout is assigned here; an A0/A1 assigned decoder must validate it.
        record: Vec<u8>,
    } = 1,
    ExactFrame {
        /// Canonical A0 EvidenceFrame.encode bytes. Its inner TIR part is still
        /// independently preflighted and checked by A1 after Panel authentication.
        frame: Vec<u8>,
        panel_signature: [u8; SIGNATURE_BYTES],
    } = 2,
    SourceRequest {
        request: SourceRequest,
        requester: PrincipalId,
        signature: [u8; SIGNATURE_BYTES],
    } = 3,
    SourceResponse {
        case: CaseId,
        request: RequestId,
        /// Existing G14 response codec, not a new data tree or response protocol.
        /// Inner G14 bounded preflight/opening validation remains mandatory.
        response: Vec<u8>,
    } = 4,
    Finish {
        case: CaseId,
        result: ResultId,
        witness: WitnessId,
    } = 5,
    Expire {
        case: CaseId,
        result: ResultId,
    } = 6,
    PositiveCourtAttestation {
        /// Literal claim-level Gate C statement only. Native must independently
        /// pin draw/policy/witness/material/bond/verifier and resolve its key.
        attestation: PositiveCourtAttestationV2,
        signature: [u8; SIGNATURE_BYTES],
    } = 7,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeBodySummaryV2 {
    pub kind: u8,
    /// Does not count signatures hidden in opaque record/response codecs.
    pub signatures: u64,
    pub relations: u64,
    pub opaque_bytes: u64,
    pub canonical_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeBodyErrorV2 {
    Version,
    Kind,
    Budget,
    Wire,
    Empty,
    NonCanonical,
    UnsupportedProfile,
}
fn limits_ok(limits: NativeBodyLimitsV2) -> Result<(), NativeBodyErrorV2> {
    if limits.max_body_bytes == 0
        || limits.max_opaque_bytes == 0
        || limits.max_exact_relations == 0
        || limits.max_body_bytes > MAX_BODY_BYTES_V2
        || limits.max_opaque_bytes > MAX_OPAQUE_BYTES_V2
        || limits.max_exact_relations > MAX_EXACT_RELATIONS_V2
        || limits.max_signatures > MAX_SIGNATURES_V2
    {
        return Err(NativeBodyErrorV2::Budget);
    }
    Ok(())
}
fn version(version: u16, revision: u16) -> Result<(), NativeBodyErrorV2> {
    if version != CONTRACT_VERSION || revision != CONTRACT_REVISION {
        return Err(NativeBodyErrorV2::Version);
    }
    Ok(())
}
// Pinned A0 r1 flat widths, expressed in primitive wire units. This scanner
// never invokes a Borsh deserializer before all outer allocation bounds pass.
const MINER_RESULT_BYTES: u64 = 4 + 16 * 64 + 32 + 4 * 4;
const SERVICE_BYTES: u64 = 4 + 4 * 64 + 7 * 8 + 4 + 8;
const CONSENT_BYTES: u64 = 4 + 4 * 64 + 3 * 8 + 4;
fn skip<R: Read>(reader: &mut NativeWireReaderV1<R>, bytes: u64) -> Result<(), NativeBodyErrorV2> {
    reader.skip(bytes).map_err(|_| NativeBodyErrorV2::Wire)
}
fn fixed_version<R: Read>(reader: &mut NativeWireReaderV1<R>, bytes: u64) -> Result<(), NativeBodyErrorV2> {
    version(reader.short().map_err(|_| NativeBodyErrorV2::Wire)?, reader.short().map_err(|_| NativeBodyErrorV2::Wire)?)?;
    skip(reader, bytes - 4)
}
fn exact_profile<R: Read>(reader: &mut NativeWireReaderV1<R>) -> Result<(), NativeBodyErrorV2> {
    match reader.byte().map_err(|_| NativeBodyErrorV2::Wire)? {
        0 => Ok(()),
        1 => Err(NativeBodyErrorV2::UnsupportedProfile),
        _ => Err(NativeBodyErrorV2::Wire),
    }
}
fn long<R: Read>(reader: &mut NativeWireReaderV1<R>) -> Result<u64, NativeBodyErrorV2> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes).map_err(|_| NativeBodyErrorV2::Wire)?;
    Ok(u64::from_le_bytes(bytes))
}
fn signature<R: Read>(
    reader: &mut NativeWireReaderV1<R>,
    summary: &mut NativeBodySummaryV2,
    limits: NativeBodyLimitsV2,
) -> Result<(), NativeBodyErrorV2> {
    summary.signatures = summary.signatures.checked_add(1).ok_or(NativeBodyErrorV2::Budget)?;
    if summary.signatures > limits.max_signatures {
        return Err(NativeBodyErrorV2::Budget);
    }
    reader.skip(SIGNATURE_BYTES as u64).map_err(|_| NativeBodyErrorV2::Wire)
}
fn opaque_length<R: Read>(
    reader: &mut NativeWireReaderV1<R>,
    summary: &mut NativeBodySummaryV2,
    limits: NativeBodyLimitsV2,
) -> Result<u64, NativeBodyErrorV2> {
    let count = reader.word().map_err(|_| NativeBodyErrorV2::Wire)? as u64;
    if count == 0 {
        return Err(NativeBodyErrorV2::Empty);
    }
    summary.opaque_bytes = summary.opaque_bytes.checked_add(count).ok_or(NativeBodyErrorV2::Budget)?;
    if summary.opaque_bytes > limits.max_opaque_bytes || count > reader.remaining() {
        return Err(NativeBodyErrorV2::Budget);
    }
    Ok(count)
}
fn scan_reader<R: Read>(
    reader: &mut NativeWireReaderV1<R>,
    limits: NativeBodyLimitsV2,
) -> Result<NativeBodySummaryV2, NativeBodyErrorV2> {
    limits_ok(limits)?;
    reader.step().map_err(|_| NativeBodyErrorV2::Budget)?;
    let start = reader.remaining();
    version(reader.short().map_err(|_| NativeBodyErrorV2::Wire)?, reader.short().map_err(|_| NativeBodyErrorV2::Wire)?)?;
    let kind = reader.byte().map_err(|_| NativeBodyErrorV2::Wire)?;
    let mut summary = NativeBodySummaryV2 { kind, ..Default::default() };
    match kind {
        0 => {
            fixed_version(reader, MINER_RESULT_BYTES)?;
            signature(reader, &mut summary, limits)?;
            fixed_version(reader, SERVICE_BYTES)?;
            exact_profile(reader)?;
            skip(reader, 5 * 64 + 3 * 8)?; // FundingPlanBasis after profile
            fixed_version(reader, CONSENT_BYTES)?;
            signature(reader, &mut summary, limits)?;
        }
        1 => {
            version(reader.short().map_err(|_| NativeBodyErrorV2::Wire)?, reader.short().map_err(|_| NativeBodyErrorV2::Wire)?)?;
            skip(reader, 6 * 64 + 4)?; // scope, case/result/author, attempt
            exact_profile(reader)?;
            skip(reader, 4 * 64)?; // plan, execution record, provenance, output
            signature(reader, &mut summary, limits)?;
            let bytes = opaque_length(reader, &mut summary, limits)?;
            reader.skip(bytes).map_err(|_| NativeBodyErrorV2::Wire)?;
        }
        2 => {
            let bytes = opaque_length(reader, &mut summary, limits)?;
            if bytes < EVIDENCE_HEADER_BYTES as u64 {
                return Err(NativeBodyErrorV2::Wire);
            }
            // The magic and header are the existing A0 PC2R1EVD framing, not a
            // new inner codec. No payload vector is allocated during this scan.
            let mut magic = [0; 8];
            reader.read_exact(&mut magic).map_err(|_| NativeBodyErrorV2::Wire)?;
            if &magic != b"PC2R1EVD" {
                return Err(NativeBodyErrorV2::NonCanonical);
            }
            skip(reader, 4 * 64)?; // case/result/witness/plan IDs
            let first = long(reader)?;
            let relations = long(reader)?;
            let payload_bytes = reader.word().map_err(|_| NativeBodyErrorV2::Wire)? as u64;
            if relations == 0 || first.checked_add(relations).is_none() {
                return Err(NativeBodyErrorV2::NonCanonical);
            }
            if bytes != (EVIDENCE_HEADER_BYTES as u64).checked_add(payload_bytes).ok_or(NativeBodyErrorV2::Budget)? {
                return Err(NativeBodyErrorV2::NonCanonical);
            }
            if relations > limits.max_exact_relations {
                return Err(NativeBodyErrorV2::Budget);
            }
            if relations != 1 {
                return Err(NativeBodyErrorV2::UnsupportedProfile);
            }
            summary.relations = relations;
            skip(reader, payload_bytes)?;
            signature(reader, &mut summary, limits)?;
        }
        3 => {
            version(reader.short().map_err(|_| NativeBodyErrorV2::Wire)?, reader.short().map_err(|_| NativeBodyErrorV2::Wire)?)?;
            skip(reader, 3 * 64)?; // case, contract, source
            if reader.byte().map_err(|_| NativeBodyErrorV2::Wire)? > 1 {
                return Err(NativeBodyErrorV2::Wire);
            }
            let first = long(reader)?;
            let end = long(reader)?;
            if first >= end {
                return Err(NativeBodyErrorV2::NonCanonical);
            }
            skip(reader, 64)?; // requester principal
            signature(reader, &mut summary, limits)?;
        }
        4 => {
            skip(reader, 2 * 64)?; // case/request IDs
            let bytes = opaque_length(reader, &mut summary, limits)?;
            reader.skip(bytes).map_err(|_| NativeBodyErrorV2::Wire)?;
        }
        5 => {
            skip(reader, 3 * 64)?; // case/result/witness IDs
        }
        6 => {
            skip(reader, 2 * 64)?; // case/result IDs
        }
        7 => {
            version(reader.short().map_err(|_| NativeBodyErrorV2::Wire)?, reader.short().map_err(|_| NativeBodyErrorV2::Wire)?)?;
            // A0's fixed 837-byte DTO: twelve digests precede verdict, and
            // checked_material is the final digest. No peer-sized allocation.
            skip(reader, POSITIVE_ATTESTATION_BYTES_V2 as u64 - 4 - 1 - 64)?;
            if reader.byte().map_err(|_| NativeBodyErrorV2::Wire)? > 2 {
                return Err(NativeBodyErrorV2::Wire);
            }
            skip(reader, 64)?;
            signature(reader, &mut summary, limits)?;
        }
        _ => return Err(NativeBodyErrorV2::Kind),
    }
    summary.canonical_bytes = start.checked_sub(reader.remaining()).ok_or(NativeBodyErrorV2::Wire)?;
    if summary.canonical_bytes > limits.max_body_bytes {
        return Err(NativeBodyErrorV2::Budget);
    }
    Ok(summary)
}

/// Allocation-free bounded scan of exactly one complete body, with no trailing
/// bytes. Native admission policy must supply limits; there is no armed default.
pub fn scan_native_body_v2(bytes: &[u8], limits: NativeBodyLimitsV2) -> Result<NativeBodySummaryV2, NativeBodyErrorV2> {
    limits_ok(limits)?;
    if bytes.len() as u64 > limits.max_body_bytes {
        return Err(NativeBodyErrorV2::Budget);
    }
    let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let summary = scan_reader(&mut reader, limits)?;
    if reader.remaining() != 0 {
        return Err(NativeBodyErrorV2::NonCanonical);
    }
    Ok(summary)
}
pub fn decode_native_body_v2(bytes: &[u8], limits: NativeBodyLimitsV2) -> Result<PositiveNativeBodyV2, NativeBodyErrorV2> {
    scan_native_body_v2(bytes, limits)?;
    borsh::from_slice(bytes).map_err(|_| NativeBodyErrorV2::Wire)
}
pub fn encode_native_body_v2(body: &PositiveNativeBodyV2, limits: NativeBodyLimitsV2) -> Result<Vec<u8>, NativeBodyErrorV2> {
    limits_ok(limits)?;
    // Precheck literal Vec ownership and the full serialized length before
    // allocating a wire copy of an arbitrarily large locally supplied body.
    let opaque = match &body.operation {
        PositiveNativeOperationV2::WitnessCommit { record, .. } => record.len() as u64,
        PositiveNativeOperationV2::ExactFrame { frame, .. } => frame.len() as u64,
        PositiveNativeOperationV2::SourceResponse { response, .. } => response.len() as u64,
        _ => 0,
    };
    if opaque > limits.max_opaque_bytes
        || borsh::object_length(body).map_err(|_| NativeBodyErrorV2::Wire)? as u64 > limits.max_body_bytes
    {
        return Err(NativeBodyErrorV2::Budget);
    }
    let bytes = borsh::to_vec(body).map_err(|_| NativeBodyErrorV2::Wire)?;
    scan_native_body_v2(&bytes, limits)?;
    Ok(bytes)
}

impl NativeWireScanV1 for PositiveNativeBodyV2 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let summary = scan_reader(reader, CODEC_CEILINGS)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid Positive body v2/r1"))?;
        NativeWireShapeV1 { outer_entries: summary.opaque_bytes, byte_entries: summary.opaque_bytes, ..Default::default() }.allocation(
            summary
                .opaque_bytes
                .checked_mul(2)
                .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "Positive body heap overflow"))?,
            1,
        )
    }
}
impl NativeHeapV1 for PositiveNativeBodyV2 {
    fn native_heap_v1(&self, component: &str) -> NativeCoverageV1 {
        match &self.operation {
            PositiveNativeOperationV2::WitnessCommit { record, .. } => record.native_heap_v1(component),
            PositiveNativeOperationV2::ExactFrame { frame, .. } => frame.native_heap_v1(component),
            PositiveNativeOperationV2::SourceResponse { response, .. } => response.native_heap_v1(component),
            _ => NativeCoverageV1::zero(),
        }
    }
}
