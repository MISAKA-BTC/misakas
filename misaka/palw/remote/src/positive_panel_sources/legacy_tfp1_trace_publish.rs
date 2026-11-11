//! Bounded projection of original native capture units to existing provider
//! Trace paths. Local I/O does not confer native publication authority.

use std::{path::Path, sync::Arc};

use super::{
    LegacyTfp1ReplayContextV1, PanelSourceErrorR1, PublicTraceAcquisitionV1, check_public_replay_trace_budget_v1,
    checked_public_trace_entry_v1, legacy_tfp1_replay::check_legacy_tfp1_pairing_v1,
};
use crate::{
    evidence::fs::write_provider_chunk,
    public_material::{
        MATERIAL_UNIT_MAX_BYTES, public_original_trace_index_path, public_original_trace_tile_path, trace_tile_wire_overhead,
    },
};
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{
        PalwExecutionBackendV1, PalwOriginalCaptureReaderV1, PalwTirPublicCaptureNeedV1, PalwTirPublicCaptureReaderV1,
        PalwTirPublicCaptureSliceV1,
    },
    palw_tir_court_v1::PublicOriginalTraceIndexV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicResponseV1, PalwTirPublicSourceV1, PalwTirPublicUnitBoundV1},
};
use misaka_palw_positive_contract::{
    identity::Digest,
    public_one_honest::{CheckedManifestV1, OriginalId, SignedPublicOriginalV1},
};
use misaka_palw_tir_exec::node::{TirBackendV1, memory::TirMemoryLeaseV1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyTfp1TracePublishScopeV1 {
    pub claim: Hash64,
    pub original: OriginalId,
    pub companion_carrier: Digest,
    pub source: Hash64,
    pub material_until_daa: u64,
}

#[derive(Clone, Copy)]
pub struct LegacyTfp1TracePublishLimitsV1 {
    pub max_capture_bytes: u64,
    /// Counts every admitted poll, including preparation progress and errors.
    pub max_polls: u64,
    /// Includes serialized opening overhead and the fixed trace index.
    pub max_retained_trace_bytes: u64,
    pub max_leaves: u64,
}

/// Adapter to the owner's current native permit, operational debits and
/// provider retention owner. There is no unrestricted default implementation.
/// Implementations must debit before returning a lease and keep the provider
/// retention reservation independently of this cursor, even after a partial
/// export, retry or failed write. An existing phase-12 prepared-index permit
/// does not itself authorize writing these provider files.
pub trait LegacyTfp1TracePublishAdmissionV1 {
    /// Reserve the reader owner and independently book the entire provider
    /// retention envelope before opening the capture. The returned lease
    /// owns the reader; provider retention outlives it.
    fn reserve_capture_v1(
        &mut self,
        scope: LegacyTfp1TracePublishScopeV1,
        need: PalwTirPublicCaptureNeedV1,
        maximum_retained_trace_bytes: u64,
    ) -> Result<TirMemoryLeaseV1, String>;

    /// Check the exact native scope/cursor and consume its work. Include
    /// native-bound authentication and encoded-buffer RAM alongside the
    /// persistent reader ticket.
    fn reserve_read_v1(
        &mut self,
        scope: LegacyTfp1TracePublishScopeV1,
        request: PalwTirPublicRequestV1,
        quote: PalwTirPublicCaptureSliceV1,
        native_bound: PalwTirPublicUnitBoundV1,
        maximum_encoded_bytes: u64,
    ) -> Result<TirMemoryLeaseV1, String>;

    /// The leaf is None only for the fixed index after all tiles are written.
    /// Account for the bounded existing-file read, temporary write, rename
    /// and sync, with retained bytes held through the accepted horizon.
    fn reserve_write_v1(
        &mut self,
        scope: LegacyTfp1TracePublishScopeV1,
        leaf: Option<u64>,
        bytes: u64,
    ) -> Result<TirMemoryLeaseV1, String>;
}

// The backend rechecks the length before opening its reader. Give it the
// same extent used by the admission quote, even if the original path grows.
struct PinnedCaptureV1 {
    reader: Arc<dyn PalwOriginalCaptureReaderV1>,
    length: u64,
}

impl PalwOriginalCaptureReaderV1 for PinnedCaptureV1 {
    fn len_v1(&self) -> u64 {
        self.length
    }

    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        if offset.checked_add(u64::from(length)).is_none_or(|end| end > self.length) {
            return Err("trace publisher capture read exceeds its admitted extent".into());
        }
        let bytes = self.reader.read_v1(offset, length)?;
        if bytes.len() != length as usize {
            return Err("trace publisher capture changed read length".into());
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyTfp1TraceExportV1 {
    Pending,
    Tile { index: u64 },
    Complete { scope: LegacyTfp1TracePublishScopeV1, leaves: u64, wire_bytes: u64 },
}

/// One attempt, held by the caller's already selected publication owner.
/// The signed manifest describes all scopes, so finishing Trace alone does
/// not publish it or signal complete original availability.
pub struct LegacyTfp1TracePublisherV1<'a> {
    scope: LegacyTfp1TracePublishScopeV1,
    source: &'a PalwTirPublicSourceV1,
    reader: Box<dyn PalwTirPublicCaptureReaderV1>,
    _capture: TirMemoryLeaseV1,
    provider_root: &'a Path,
    limits: LegacyTfp1TracePublishLimitsV1,
    trace_index: PublicOriginalTraceIndexV1,
    value_cap: usize,
    remaining_values: u64,
    retained_bytes: u64,
    polls: u64,
    next: u64,
    complete: bool,
}

impl<'a> LegacyTfp1TracePublisherV1<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn open_v1(
        context: LegacyTfp1ReplayContextV1<'a>,
        signed: &SignedPublicOriginalV1,
        manifest: &CheckedManifestV1,
        backend: &TirBackendV1,
        capture: Arc<dyn PalwOriginalCaptureReaderV1>,
        cache_directory: &Path,
        provider_root: &'a Path,
        material_until_daa: u64,
        limits: LegacyTfp1TracePublishLimitsV1,
        admission: &mut impl LegacyTfp1TracePublishAdmissionV1,
    ) -> Result<Self, PanelSourceErrorR1> {
        check_legacy_tfp1_pairing_v1(context, signed, manifest, backend)?;
        let source = context.source;
        let header = source.header_v1();
        let trace_index = PublicOriginalTraceIndexV1 {
            full_logits_trace_root: header.full_logits_trace_root,
            step_leaf_count: header.step_leaf_count,
            step_merkle_root: header.step_merkle_root,
        };
        let trace = checked_public_trace_entry_v1(&signed.original, manifest)?;
        trace_index.check(&signed.original, limits.max_leaves).map_err(|_| PanelSourceErrorR1::Binding)?;
        let maximum_retained_trace_bytes = check_public_replay_trace_budget_v1(
            trace_index.step_leaf_count,
            trace.total_bytes,
            PublicTraceAcquisitionV1 { max_tiles: limits.max_leaves, max_wire_bytes: limits.max_retained_trace_bytes },
        )?;
        let value_cap = usize::try_from(u64::from(trace.chunk_bytes).min(source.widest_leaf_value_bytes_v1()))
            .map_err(|_| PanelSourceErrorR1::Budget)?;
        let capture_len = capture.len_v1();
        if capture_len == 0
            || capture_len > limits.max_capture_bytes
            || limits.max_polls < trace_index.step_leaf_count
            || material_until_daa == 0
        {
            return Err(PanelSourceErrorR1::Budget);
        }
        let scope = LegacyTfp1TracePublishScopeV1 {
            claim: context.claim,
            original: context.original,
            companion_carrier: context.companion_carrier,
            source: context.source_id,
            material_until_daa,
        };
        let need = backend.tir_public_capture_need_v1(header, capture_len).map_err(PanelSourceErrorR1::ReplayExecution)?;
        let owner =
            admission.reserve_capture_v1(scope, need, maximum_retained_trace_bytes).map_err(PanelSourceErrorR1::ReplayExecution)?;
        let capture = Arc::new(PinnedCaptureV1 { reader: capture, length: capture_len });
        let reader =
            backend.open_tir_public_capture_v1(header, capture, cache_directory).map_err(PanelSourceErrorR1::ReplayExecution)?;
        Ok(Self {
            scope,
            source,
            reader,
            _capture: owner,
            provider_root,
            limits,
            trace_index,
            value_cap,
            remaining_values: trace.total_bytes - PublicOriginalTraceIndexV1::WIRE_BYTES as u64,
            retained_bytes: 0,
            polls: 0,
            next: 0,
            complete: false,
        })
    }

    /// At most one reader poll and one provider write. The index is written
    /// last, after exact signed byte totals and every authenticated leaf.
    pub fn advance_v1(
        &mut self,
        admission: &mut impl LegacyTfp1TracePublishAdmissionV1,
    ) -> Result<LegacyTfp1TraceExportV1, PanelSourceErrorR1> {
        if self.complete {
            return Ok(self.completion_v1());
        }
        if self.next == self.trace_index.step_leaf_count {
            if self.remaining_values != 0 {
                return Err(PanelSourceErrorR1::Binding);
            }
            let size = PublicOriginalTraceIndexV1::WIRE_BYTES as u64;
            let after = self.checked_retained_v1(size)?;
            let _write = admission.reserve_write_v1(self.scope, None, size).map_err(PanelSourceErrorR1::ReplayProvider)?;
            let bytes = borsh::to_vec(&self.trace_index).map_err(|_| PanelSourceErrorR1::Binding)?;
            write_provider_chunk(&self.provider_root.join(public_original_trace_index_path(self.scope.original)), &bytes)
                .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.to_string()))?;
            self.retained_bytes = after;
            self.complete = true;
            return Ok(self.completion_v1());
        }
        if self.polls == self.limits.max_polls {
            return Err(PanelSourceErrorR1::Budget);
        }
        let index = self.next;
        let request = PalwTirPublicRequestV1::StepLeaf { index };
        let native_bound = self.source.unit_bound_v1(&request).map_err(PanelSourceErrorR1::ReplayExecution)?;
        let value_cap = self.value_cap.min(usize::try_from(self.remaining_values).unwrap_or(usize::MAX));
        let wire_cap = trace_tile_wire_overhead(self.trace_index.step_leaf_count)
            .checked_add(value_cap)
            .ok_or(PanelSourceErrorR1::Budget)?
            .min(MATERIAL_UNIT_MAX_BYTES);
        let quote = self.reader.slice_quote_v1(&request).map_err(PanelSourceErrorR1::ReplayExecution)?;
        if quote.source_id != self.scope.source {
            return Err(PanelSourceErrorR1::Binding);
        }
        let _read = admission
            .reserve_read_v1(self.scope, request, quote, native_bound, wire_cap as u64)
            .map_err(PanelSourceErrorR1::ReplayExecution)?;
        self.polls += 1;
        let Some(response) = self.reader.poll_quoted_v1(&request, &quote).map_err(PanelSourceErrorR1::ReplayExecution)? else {
            return Ok(LegacyTfp1TraceExportV1::Pending);
        };
        let authenticated = self.source.authenticate_v1(&request, &response, None).map_err(|_| PanelSourceErrorR1::Binding)?;
        drop(response);
        let PalwTirPublicResponseV1::StepLeaf { opening, preimage } = authenticated.response_v1() else {
            return Err(PanelSourceErrorR1::Binding);
        };
        if preimage.values_le.len() > value_cap {
            return Err(PanelSourceErrorR1::Binding);
        }
        let size = borsh::object_length(&(opening, preimage)).map_err(|_| PanelSourceErrorR1::Binding)?;
        if size > wire_cap {
            return Err(PanelSourceErrorR1::Budget);
        }
        let after = self.checked_retained_v1(size as u64)?;
        let _write = admission.reserve_write_v1(self.scope, Some(index), size as u64).map_err(PanelSourceErrorR1::ReplayProvider)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|_| PanelSourceErrorR1::Budget)?;
        if bytes.capacity() != size {
            return Err(PanelSourceErrorR1::Budget);
        }
        borsh::BorshSerialize::serialize(&(opening, preimage), &mut bytes).map_err(|_| PanelSourceErrorR1::Binding)?;
        write_provider_chunk(&self.provider_root.join(public_original_trace_tile_path(self.scope.original, index)), &bytes)
            .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.to_string()))?;
        self.retained_bytes = after;
        self.remaining_values -= preimage.values_le.len() as u64;
        self.next += 1;
        Ok(LegacyTfp1TraceExportV1::Tile { index })
    }

    fn checked_retained_v1(&self, added: u64) -> Result<u64, PanelSourceErrorR1> {
        self.retained_bytes
            .checked_add(added)
            .filter(|size| *size <= self.limits.max_retained_trace_bytes)
            .ok_or(PanelSourceErrorR1::Budget)
    }

    fn completion_v1(&self) -> LegacyTfp1TraceExportV1 {
        LegacyTfp1TraceExportV1::Complete { scope: self.scope, leaves: self.next, wire_bytes: self.retained_bytes }
    }
}
