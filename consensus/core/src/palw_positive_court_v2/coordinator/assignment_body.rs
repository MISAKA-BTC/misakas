//! RFC-0045 bounded syntax for local operations 8--13 under object 176.
//! This module does not activate a decoder, admit native state, lock an output,
//! authenticate roles or authorize computation/credit/payment. Actual native
//! ingress must select the Assignment fence before this larger signature path.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentAdmissionBundleV2, AssignmentId,
        consents::{AssignmentProverConsentV2, AssignmentSourceConsentV2},
        role_actions::{AssignmentCancellationV2, AssignmentProverJoinV2},
    },
    funding::{FundingConsent, FundingPlanBasis},
    identity::{CaseId, SIGNATURE_BYTES},
    messages::{MinerResultCommit, MinerServiceContract},
    rows::AcceptedMaterial,
};

/// Literal old body0 payload fields/order, without its outer version/kind.
/// A later AssignmentId is not retrofitted onto a separately accepted op0.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignedResultPayloadV2 {
    pub result: MinerResultCommit,
    pub miner_signature: [u8; SIGNATURE_BYTES],
    pub service: MinerServiceContract,
    pub basis: FundingPlanBasis,
    pub consent: FundingConsent,
    pub payer_signature: [u8; SIGNATURE_BYTES],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveAssignmentNativeBodyV2 {
    pub version: u16,
    pub revision: u16,
    pub operation: PositiveAssignmentNativeOperationV2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
#[allow(clippy::large_enum_variant)] // fixed inline signature slots are explicitly quoted below
pub enum PositiveAssignmentNativeOperationV2 {
    AcceptCanonicalAssignment {
        bundle: AssignmentAdmissionBundleV2,
        source: AssignmentSourceConsentV2,
        prover: AssignmentProverConsentV2,
        /// Poster, worker, prover order. Keys are resolved from native bonds.
        signatures: [[u8; SIGNATURE_BYTES]; 3],
        /// Bounded canonical existing PalwTirLayoutV1 bytes. Mandatory inner
        /// native scan/registered-layout validation precedes use or allocation.
        layout: Vec<u8>,
    } = 8,
    AdmitAssignedResult {
        assignment: AssignmentId,
        payload: AssignedResultPayloadV2,
    } = 9,
    CancelAssignment {
        cancellation: AssignmentCancellationV2,
        poster_signature: [u8; SIGNATURE_BYTES],
        worker_signature: [u8; SIGNATURE_BYTES],
    } = 10,
    ExpireAssignment {
        assignment: AssignmentId,
    } = 11,
    SettleAssignment {
        assignment: AssignmentId,
        case: Option<CaseId>,
        /// Actual accepted due/Case references only. No amount, clock, pass,
        /// liability-closed flag or entitlement comes from the carrier.
        receipts: Vec<AcceptedMaterial>,
    } = 12,
    JoinAssignmentProver {
        consent: AssignmentProverJoinV2,
        signature: [u8; SIGNATURE_BYTES],
    } = 13,
}

// This staged schema deliberately has no unchecked public `decode` helper.
// The native reader must first scan all lengths, inline/opaque shape, exact
// versions and per-kind signature count under its actual rooted finite policy,
// reserve typed slots plus opaque/inner workspace, then deserialize and verify.
// Old operations retain NativeBodyLimitsV2::max_signatures <= 2; only op8 may
// quote exactly three role signatures. Nothing here widens the old decoder.

use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
use std::{
    io::{self, Read},
    mem::size_of,
};

pub const MAX_ASSIGNMENT_BODY_BYTES_V2: u64 = 16 * 1024 * 1024;
pub const MAX_ASSIGNMENT_LAYOUT_BYTES_V2: u64 = 8 * 1024 * 1024;
pub const MAX_ASSIGNMENT_ROLE_SIGNATURES_V2: u64 = 3;
pub const MAX_ASSIGNMENT_DECODE_HEAP_BYTES_V2: u64 = 64 * 1024 * 1024;
pub const ASSIGNMENT_TERMS_BYTES_V2: u64 = 849;
pub const ASSIGNMENT_BUNDLE_BYTES_V2: u64 = 1833;
pub const ASSIGNMENT_SOURCE_CONSENT_BYTES_V2: u64 = 332;
pub const ASSIGNMENT_PROVER_CONSENT_BYTES_V2: u64 = 316;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentBodyLimitsV2 {
    pub max_body_bytes: u64,
    pub max_layout_bytes: u64,
    pub max_layout_entries: u32,
    pub max_settlement_receipts: u32,
    pub max_outer_signatures: u64,
    pub max_decode_heap: u64,
}
impl AssignmentBodyLimitsV2 {
    /// Finite syntax limits derived from the complete policy DATA. The native
    /// owner must resolve that policy from its accepted context and acquire
    /// actual leases; this conversion grants no such authority.
    pub fn from_policy(
        policy: &misaka_palw_positive_contract::assignment::policy::AssignmentPolicyV2,
    ) -> Result<Self, AssignmentBodyErrorV2> {
        policy.validate_shape().map_err(|_| AssignmentBodyErrorV2::Budget)?;
        let limits = Self {
            max_body_bytes: policy.max_frame_bytes.min(MAX_ASSIGNMENT_BODY_BYTES_V2),
            max_layout_bytes: policy.max_frame_bytes.min(MAX_ASSIGNMENT_LAYOUT_BYTES_V2),
            max_layout_entries: policy.max_layout_entries,
            max_settlement_receipts: policy.max_history_frames,
            max_outer_signatures: MAX_ASSIGNMENT_ROLE_SIGNATURES_V2,
            max_decode_heap: policy.max_heap_bytes.min(MAX_ASSIGNMENT_DECODE_HEAP_BYTES_V2),
        };
        bounded(limits)?;
        Ok(limits)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssignmentBodySummaryV2 {
    pub kind: u8,
    pub signatures: u64,
    pub canonical_bytes: u64,
    pub layout_bytes: u64,
    pub layout_entries: u64,
    pub receipt_entries: u64,
    pub heap: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentBodyErrorV2 {
    Version,
    Kind,
    Budget,
    Wire,
    NonCanonical,
    Layout,
}
/// Reads the fixed continuation identity before any original-policy decode.
/// The result is a lookup key. The full decoder still checks the whole body.
pub fn scan_assignment_continuation_id_v2(raw: &[u8]) -> Result<AssignmentId, AssignmentBodyErrorV2> {
    if raw.len() as u64 > MAX_ASSIGNMENT_BODY_BYTES_V2 {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    if raw.get(..4) != Some(&[2, 0, 1, 0]) {
        return Err(AssignmentBodyErrorV2::Version);
    }
    let start = match raw.get(4) {
        Some(9..=12) => 5,
        Some(13) if raw.get(5..9) == Some(&[2, 0, 1, 0]) => 9,
        Some(13) => return Err(AssignmentBodyErrorV2::Version),
        _ => return Err(AssignmentBodyErrorV2::Kind),
    };
    let id = raw.get(start..start + 64).ok_or(AssignmentBodyErrorV2::Wire)?.try_into().map_err(|_| AssignmentBodyErrorV2::Wire)?;
    Ok(AssignmentId(misaka_palw_positive_contract::identity::Digest(id)))
}
fn bounded(limits: AssignmentBodyLimitsV2) -> Result<(), AssignmentBodyErrorV2> {
    if limits.max_body_bytes == 0
        || limits.max_body_bytes > MAX_ASSIGNMENT_BODY_BYTES_V2
        || limits.max_layout_bytes == 0
        || limits.max_layout_bytes > MAX_ASSIGNMENT_LAYOUT_BYTES_V2
        || limits.max_layout_entries == 0
        || limits.max_layout_entries == u32::MAX
        || limits.max_settlement_receipts == 0
        || limits.max_settlement_receipts == u32::MAX
        || limits.max_outer_signatures > MAX_ASSIGNMENT_ROLE_SIGNATURES_V2
        || limits.max_decode_heap == 0
        || limits.max_decode_heap > MAX_ASSIGNMENT_DECODE_HEAP_BYTES_V2
    {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    Ok(())
}
fn wire<T>(r: io::Result<T>) -> Result<T, AssignmentBodyErrorV2> {
    r.map_err(|_| AssignmentBodyErrorV2::Wire)
}
fn version<R: Read>(r: &mut NativeWireReaderV1<R>) -> Result<(), AssignmentBodyErrorV2> {
    if wire(r.short())? != 2 || wire(r.short())? != 1 {
        return Err(AssignmentBodyErrorV2::Version);
    }
    Ok(())
}
fn fixed<R: Read>(r: &mut NativeWireReaderV1<R>, bytes: u64) -> Result<(), AssignmentBodyErrorV2> {
    version(r)?;
    wire(r.skip(bytes - 4))
}
fn signatures<R: Read>(
    r: &mut NativeWireReaderV1<R>,
    summary: &mut AssignmentBodySummaryV2,
    n: u64,
    limits: AssignmentBodyLimitsV2,
) -> Result<(), AssignmentBodyErrorV2> {
    if n > limits.max_outer_signatures {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    summary.signatures = n;
    wire(r.skip(n * SIGNATURE_BYTES as u64))
}
fn add_heap(total: &mut u64, count: u64, unit: usize) -> Result<(), AssignmentBodyErrorV2> {
    *total = total
        .checked_add(count.checked_mul(unit as u64).ok_or(AssignmentBodyErrorV2::Budget)?)
        .ok_or(AssignmentBodyErrorV2::Budget)?;
    Ok(())
}
fn scan_reader<R: Read>(
    r: &mut NativeWireReaderV1<R>,
    limits: AssignmentBodyLimitsV2,
) -> Result<AssignmentBodySummaryV2, AssignmentBodyErrorV2> {
    bounded(limits)?;
    let start = r.remaining();
    version(r)?;
    let kind = wire(r.byte())?;
    let mut summary = AssignmentBodySummaryV2 { kind, ..Default::default() };
    match kind {
        8 => {
            version(r)?;
            wire(r.skip(3 * 64))?;
            if wire(r.byte())? != 0 {
                return Err(AssignmentBodyErrorV2::Kind);
            }
            wire(r.skip(ASSIGNMENT_BUNDLE_BYTES_V2 - 4 - 3 * 64 - 1))?;
            fixed(r, ASSIGNMENT_SOURCE_CONSENT_BYTES_V2)?;
            fixed(r, ASSIGNMENT_PROVER_CONSENT_BYTES_V2)?;
            signatures(r, &mut summary, 3, limits)?;
            let bytes = wire(r.word())? as u64;
            if bytes < 22 || bytes > limits.max_layout_bytes || bytes > r.remaining() {
                return Err(AssignmentBodyErrorV2::Budget);
            }
            let before = r.remaining();
            if wire(r.short())? != crate::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1 {
                return Err(AssignmentBodyErrorV2::Layout);
            }
            wire(r.skip(12))?;
            let commits = wire(r.word())? as u64;
            if commits > limits.max_layout_entries as u64 {
                return Err(AssignmentBodyErrorV2::Budget);
            }
            wire(r.skip(commits * 4))?;
            let states = wire(r.word())? as u64;
            let entries = commits.checked_add(states).ok_or(AssignmentBodyErrorV2::Budget)?;
            if entries > limits.max_layout_entries as u64 {
                return Err(AssignmentBodyErrorV2::Budget);
            }
            wire(r.skip(states * 4))?;
            if before - r.remaining() != bytes {
                return Err(AssignmentBodyErrorV2::NonCanonical);
            }
            summary.layout_bytes = bytes;
            summary.layout_entries = entries;
            // Pinned Borsh capacity envelopes for the raw Vec<u8> and the two
            // later typed Vec<u32>s. No opaque encoded-byte/RSS multiplier.
            add_heap(&mut summary.heap, bytes.checked_mul(2).ok_or(AssignmentBodyErrorV2::Budget)?, 1)?;
            add_heap(&mut summary.heap, entries.checked_mul(2).ok_or(AssignmentBodyErrorV2::Budget)?, size_of::<u32>())?;
        }
        9 => {
            wire(r.skip(64))?;
            if limits.max_outer_signatures < 2 {
                return Err(AssignmentBodyErrorV2::Budget);
            }
            let payload_bytes = super::native_body::RESULT_ADMISSION_PAYLOAD_BYTES_V2;
            let bytes = payload_bytes + 5;
            // Same literal r1 payload uses the EXISTING op0 scanner, under a
            // virtual fixed framing prefix. This is syntax only: no separate
            // op0 acceptance/Case is created, and no Result fields are zeroed.
            let prefix = [2, 0, 1, 0, 0];
            let stream = prefix.as_slice().chain(r.by_ref().take(payload_bytes));
            let mut payload = NativeWireReaderV1::new(stream, bytes);
            let shape = wire(super::native_body::PositiveNativeBodyV2::scan_native_wire_v1(&mut payload))?;
            if payload.remaining() != 0 {
                return Err(AssignmentBodyErrorV2::NonCanonical);
            }
            summary.signatures = 2;
            summary.heap = shape.heap;
        }
        10 => {
            wire(r.skip(3 * 64 + 8))?;
            signatures(r, &mut summary, 2, limits)?;
        }
        11 => {
            wire(r.skip(64))?;
        }
        12 => {
            wire(r.skip(64))?;
            wire(<Option<CaseId>>::scan_native_wire_v1(r))?;
            let n = wire(r.word())? as u64;
            if n > limits.max_settlement_receipts as u64 {
                return Err(AssignmentBodyErrorV2::Budget);
            }
            for _ in 0..n {
                wire(AcceptedMaterial::scan_native_wire_v1(r))?;
            }
            summary.receipt_entries = n;
            add_heap(&mut summary.heap, n.checked_mul(2).ok_or(AssignmentBodyErrorV2::Budget)?, size_of::<AcceptedMaterial>())?;
        }
        13 => {
            version(r)?;
            wire(r.skip(128))?;
            wire(<Option<(CaseId, misaka_palw_positive_contract::identity::ResultId)>>::scan_native_wire_v1(r))?;
            wire(r.skip(276))?;
            signatures(r, &mut summary, 1, limits)?;
        }
        _ => return Err(AssignmentBodyErrorV2::Kind),
    }
    summary.canonical_bytes = start - r.remaining();
    if summary.canonical_bytes > limits.max_body_bytes {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    // Audited logical typed slots for outer/body/variant moves and bounded
    // validation, separate from Vec capacity. This is not a compiler frame or
    // physical RSS promise. The native owner must reserve before decode/use.
    add_heap(&mut summary.heap, 8, size_of::<PositiveAssignmentNativeBodyV2>())?;
    if summary.heap > limits.max_decode_heap {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    Ok(summary)
}
pub fn scan_assignment_body_v2(
    bytes: &[u8],
    limits: AssignmentBodyLimitsV2,
) -> Result<AssignmentBodySummaryV2, AssignmentBodyErrorV2> {
    bounded(limits)?;
    if bytes.len() as u64 > limits.max_body_bytes {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    let mut r = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let summary = scan_reader(&mut r, limits)?;
    if r.remaining() != 0 {
        return Err(AssignmentBodyErrorV2::NonCanonical);
    }
    Ok(summary)
}
pub fn decode_assignment_body_v2(
    bytes: &[u8],
    limits: AssignmentBodyLimitsV2,
) -> Result<PositiveAssignmentNativeBodyV2, AssignmentBodyErrorV2> {
    scan_assignment_body_v2(bytes, limits)?;
    borsh::from_slice(bytes).map_err(|_| AssignmentBodyErrorV2::Wire)
}
pub fn encode_assignment_body_v2(
    body: &PositiveAssignmentNativeBodyV2,
    limits: AssignmentBodyLimitsV2,
) -> Result<Vec<u8>, AssignmentBodyErrorV2> {
    bounded(limits)?;
    match &body.operation {
        PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment { layout, .. }
            if layout.len() as u64 > limits.max_layout_bytes =>
        {
            return Err(AssignmentBodyErrorV2::Budget);
        }
        PositiveAssignmentNativeOperationV2::SettleAssignment { receipts, .. }
            if receipts.len() as u64 > limits.max_settlement_receipts as u64 =>
        {
            return Err(AssignmentBodyErrorV2::Budget);
        }
        _ => {}
    }
    if borsh::object_length(body).map_err(|_| AssignmentBodyErrorV2::Wire)? as u64 > limits.max_body_bytes {
        return Err(AssignmentBodyErrorV2::Budget);
    }
    let bytes = borsh::to_vec(body).map_err(|_| AssignmentBodyErrorV2::Wire)?;
    scan_assignment_body_v2(&bytes, limits)?;
    Ok(bytes)
}
impl NativeHeapV1 for PositiveAssignmentNativeBodyV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        match &self.operation {
            PositiveAssignmentNativeOperationV2::AcceptCanonicalAssignment { bundle, source, prover, layout, .. } => bundle
                .native_heap_v1(path)
                .add::<Self>(source.native_heap_v1(path), path)
                .add::<Self>(prover.native_heap_v1(path), path)
                .add::<Self>(layout.native_heap_v1(path), path),
            PositiveAssignmentNativeOperationV2::AdmitAssignedResult { assignment, payload } => assignment
                .native_heap_v1(path)
                .add::<Self>(payload.result.native_heap_v1(path), path)
                .add::<Self>(payload.service.native_heap_v1(path), path)
                .add::<Self>(payload.basis.native_heap_v1(path), path)
                .add::<Self>(payload.consent.native_heap_v1(path), path),
            PositiveAssignmentNativeOperationV2::CancelAssignment { cancellation, .. } => cancellation.native_heap_v1(path),
            PositiveAssignmentNativeOperationV2::ExpireAssignment { assignment } => assignment.native_heap_v1(path),
            PositiveAssignmentNativeOperationV2::SettleAssignment { assignment, case, receipts } => assignment
                .native_heap_v1(path)
                .add::<Self>(case.native_heap_v1(path), path)
                .add::<Self>(receipts.native_heap_v1(path), path),
            PositiveAssignmentNativeOperationV2::JoinAssignmentProver { consent, .. } => consent.native_heap_v1(path),
        }
    }
}

impl NativeWireScanV1 for PositiveAssignmentNativeBodyV2 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let limits = AssignmentBodyLimitsV2 {
            max_body_bytes: MAX_ASSIGNMENT_BODY_BYTES_V2,
            max_layout_bytes: MAX_ASSIGNMENT_LAYOUT_BYTES_V2,
            max_layout_entries: (MAX_ASSIGNMENT_LAYOUT_BYTES_V2 / 4) as u32,
            max_settlement_receipts: (MAX_ASSIGNMENT_BODY_BYTES_V2 / 280) as u32,
            max_outer_signatures: MAX_ASSIGNMENT_ROLE_SIGNATURES_V2,
            max_decode_heap: MAX_ASSIGNMENT_DECODE_HEAP_BYTES_V2,
        };
        let summary =
            scan_reader(reader, limits).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid Assignment body v2/r1"))?;
        Ok(NativeWireShapeV1 {
            heap: summary.heap,
            collection_entries: summary
                .layout_entries
                .checked_add(summary.receipt_entries)
                .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "Assignment entry count overflow"))?,
            outer_entries: summary.layout_bytes.max(summary.receipt_entries),
            byte_entries: summary.layout_bytes,
            ..Default::default()
        })
    }
}
