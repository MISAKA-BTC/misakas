//! Same signed record bytes, bounded sequential claims, SAME Node evaluator.
//! No new codec/domain/authority. Before EOF/hash/signature completion this API
//! exposes no computation verdict, even if an unauthenticated claim disagrees.
use super::*;
use std::io::{Cursor, Read};
/// Unknown provider retention/scratch is refused. Declarations are operational
/// bounds, not authentication or permission; the caller owns upstream providers.
/// A provider must include its complete coexisting retained buffers and maximum
/// scratch, including hidden aliases; `external_retained_bytes` covers separate
/// preparation/model/material/export owners not transferred into this reader.
/// Cursor<&[u8]> quotes only its view: its backing allocation remains external.
/// File quotes the Rust handle, not kernel cache/disk capacity; that reservation
/// and its persistence/retention contract remain the external disk owner's duty.
pub trait ClaimsReaderV2: Read {
    fn retained_request_bytes(&self) -> Option<u64> {
        None
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        None
    }
}
impl<R: ClaimsReaderV2 + ?Sized> ClaimsReaderV2 for &mut R {
    fn retained_request_bytes(&self) -> Option<u64> {
        R::retained_request_bytes(&**self)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        R::scratch_request_bytes(&**self)
    }
}
impl ClaimsReaderV2 for std::fs::File {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(size_of::<Self>() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
impl ClaimsReaderV2 for Cursor<Vec<u8>> {
    fn retained_request_bytes(&self) -> Option<u64> {
        (size_of::<Self>() as u64).checked_add(self.get_ref().capacity() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
impl ClaimsReaderV2 for Cursor<&[u8]> {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(size_of::<Self>() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct StreamRecordLimitsV2 {
    pub max_record_bytes: u64,
    pub max_claim_values: u32,
    pub max_read_bytes: u64,
    pub max_read_calls: u64,
    pub max_extra_heap_bytes: u64,
    pub metadata_auth_bytes: u64,
    pub signature_workspace_bytes: u64,
    /// All independent aliases/provider/metadata/preparation owners, pre-summed.
    /// A same-name field or a nonzero number is not an actual reservation receipt.
    pub external_retained_bytes: u64,
}
pub type StreamRecordLeaseV2 = Box<dyn Send + Sync>;
pub type StreamRecordReserverV2<'a> = dyn Fn(u64) -> Result<StreamRecordLeaseV2, FullRelationError> + 'a;
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamReadMeterV2 {
    pub read_bytes: u64,
    pub read_calls: u64,
    pub geometry_visits: u64,
    pub relations_read: u64,
}
/// Producer-owned metadata/digest, not a proof of numerical correctness. No
/// Deserialize, saved progress, native receipt, or reusable acceptance capability.
pub struct StreamPanelRecordV2 {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    author: PrincipalId,
    commitment: Digest,
    domains: PrototypeDomains,
    _lease: StreamRecordLeaseV2,
}
impl StreamPanelRecordV2 {
    pub fn signing_digest(&self) -> Digest {
        self.commitment
    }
    pub fn signature_context(&self) -> &[u8] {
        &self.domains.signature_context
    }
    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        &self.binding
    }
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }
    pub fn author(&self) -> PrincipalId {
        self.author
    }
}
struct StreamDataV2 {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    index: u64,
    current: Option<Vec<i128>>,
}
impl super::record_access_sealed::Sealed for StreamDataV2 {}
impl WideRecordAccessV2 for StreamDataV2 {
    fn binding(&self) -> &PalwTirStepBindingV1 {
        &self.binding
    }
    fn generated(&self) -> &[u32] {
        &self.generated
    }
    fn claim(&self, index: u64) -> Option<&[i128]> {
        (index == self.index).then_some(())?;
        self.current.as_deref()
    }
}
struct ClaimStreamV2<R> {
    reader: R,
    writer: HashWriter,
    limits: StreamRecordLimitsV2,
    meter: StreamReadMeterV2,
    count: u64,
    failed: bool,
}
impl<R: ClaimsReaderV2> ClaimStreamV2<R> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), FullRelationError> {
        if self.failed {
            return Err(FullRelationError::Unverified);
        }
        let mut filled = 0;
        while filled < bytes.len() {
            self.meter.read_calls = reserve(self.meter.read_calls, 1, self.limits.max_read_calls)?;
            self.meter.read_bytes = reserve(self.meter.read_bytes, (bytes.len() - filled) as u64, self.limits.max_read_bytes)?;
            match self.reader.read(&mut bytes[filled..]) {
                Ok(0) => {
                    self.failed = true;
                    return Err(FullRelationError::Delivery);
                }
                Ok(n) if n <= bytes.len() - filled => {
                    self.writer.write_all(&bytes[filled..filled + n]).map_err(|_| FullRelationError::Budget)?;
                    filled += n;
                }
                Ok(_) => {
                    self.failed = true;
                    return Err(FullRelationError::Delivery);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => {
                    self.failed = true;
                    return Err(FullRelationError::Delivery);
                }
            }
        }
        Ok(())
    }
    fn start_count(&mut self, expected: u64) -> Result<(), FullRelationError> {
        let mut b = [0; 4];
        self.read(&mut b)?;
        if u64::from(u32::from_le_bytes(b)) != expected {
            return Err(FullRelationError::Delivery);
        }
        self.count = expected;
        Ok(())
    }
    fn next(&mut self, prep: &FullRelationPreparationV2) -> Result<Vec<i128>, FullRelationError> {
        if self.meter.relations_read >= self.count {
            return Err(FullRelationError::Incomplete);
        }
        // One prepaid R*G scan from the unchanged 2*R*G base allowance is
        // consumed incrementally here, rather than by resident shape validation.
        // Each relation can be read only once; source retries keep that claim.
        let prepaid = mul(self.count, prep.plan.summary().geometry_visit_bound)?;
        self.meter.geometry_visits = reserve(self.meter.geometry_visits, prep.plan.summary().geometry_visit_bound, prepaid)?;
        let r = prep.plan.relation_at(self.meter.relations_read).ok_or(FullRelationError::Binding)?;
        let (dtype, n) = match r.kind {
            ExpandedRelationKind::Legacy(l) => (l.dtype, l.value_count),
            ExpandedRelationKind::RecomputeNode { dtype, lane_count, .. } => (dtype, lane_count),
        };
        let mut b = [0; 4];
        self.read(&mut b)?;
        if u32::from_le_bytes(b) != n || n == 0 || n > self.limits.max_claim_values {
            return Err(FullRelationError::Delivery);
        }
        let mut out = Vec::new();
        out.try_reserve_exact(n as usize).map_err(|_| FullRelationError::Budget)?;
        if out.capacity() != n as usize {
            return Err(FullRelationError::Budget);
        }
        let mut left = n as usize;
        let mut chunk = [0; 4096];
        while left > 0 {
            let take = left.min(chunk.len() / 16);
            self.read(&mut chunk[..take * 16])?;
            for b in chunk[..take * 16].chunks_exact(16) {
                let v = i128::from_le_bytes(b.try_into().map_err(|_| FullRelationError::Delivery)?);
                if !dtype.contains(v) {
                    return Err(FullRelationError::Delivery);
                }
                out.push(v)
            }
            left -= take;
        }
        self.meter.relations_read += 1;
        Ok(out)
    }
    fn eof(&mut self) -> Result<(), FullRelationError> {
        if self.failed || self.meter.relations_read != self.count {
            return Err(FullRelationError::Incomplete);
        }
        self.meter.read_calls = reserve(self.meter.read_calls, 1, self.limits.max_read_calls)?;
        self.meter.read_bytes = reserve(self.meter.read_bytes, 1, self.limits.max_read_bytes)?;
        let mut b = [0];
        match self.reader.read(&mut b) {
            Ok(0) => Ok(()),
            _ => {
                self.failed = true;
                Err(FullRelationError::Delivery)
            }
        }
    }
    fn digest(&self) -> Digest {
        Digest(self.writer.state.clone().finalize().as_bytes().try_into().expect("64"))
    }
}
fn acquire<R: ClaimsReaderV2>(
    prep: &FullRelationPreparationV2,
    binding: &PalwTirStepBindingV1,
    generated: &Vec<u32>,
    reader: &R,
    l: StreamRecordLimitsV2,
    reserver: &StreamRecordReserverV2<'_>,
) -> Result<StreamRecordLeaseV2, FullRelationError> {
    if binding.class.program.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || binding.class.layout.commit_tiles.len()
            > misaka_palw_tir::program::MAX_BLOCKS * misaka_palw_tir::program::MAX_NODES_PER_BLOCK
        || binding.class.layout.state_tiles.len() > misaka_palw_tir::program::MAX_STATES
        || binding.job_context.network_id.len() > crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES
    {
        return Err(FullRelationError::Binding);
    }
    u32::try_from(prep.plan.summary().relations).map_err(|_| FullRelationError::Budget)?;
    if [
        l.max_record_bytes,
        u64::from(l.max_claim_values),
        l.max_read_bytes,
        l.max_read_calls,
        l.max_extra_heap_bytes,
        l.metadata_auth_bytes,
        l.signature_workspace_bytes,
    ]
    .contains(&0)
        || l.max_record_bytes > prep.limits.max_record_bytes
        || u64::from(l.max_claim_values) > prep.limits.plan.max_target_lanes
    {
        return Err(FullRelationError::Budget);
    }
    let reader_bytes = reader.retained_request_bytes().ok_or(FullRelationError::Budget)?;
    let scratch = reader.scratch_request_bytes().ok_or(FullRelationError::Budget)?;
    let p = prep.plan.step_space().program();
    let layers = (p.schedule.layers.len() as u64).checked_add(1).ok_or(FullRelationError::Budget)?;
    let floor = [
        mul(prep.class.program.len() as u64, 512)?,
        mul(p.params.len() as u64, 1024u64.checked_add(mul(layers, 32)?).ok_or(FullRelationError::Budget)?)?,
        mul(p.states.len() as u64, 1024u64.checked_add(mul(layers, 256)?).ok_or(FullRelationError::Budget)?)?,
        65536,
    ]
    .into_iter()
    .try_fold(0u64, |a, b| a.checked_add(b).ok_or(FullRelationError::Budget))?;
    if l.metadata_auth_bytes < floor {
        return Err(FullRelationError::Budget);
    }
    let empty = Vec::new();
    let header = record_heap(binding, generated, &empty)?;
    let extra = [
        header,
        size_of::<R>() as u64,
        reader_bytes,
        scratch,
        mul(u64::from(l.max_claim_values), 16)?,
        size_of::<HashWriter>() as u64,
        4096,
        (PUBKEY_BYTES + SIGNATURE_BYTES) as u64,
        l.metadata_auth_bytes,
        l.signature_workspace_bytes,
        l.external_retained_bytes,
    ]
    .into_iter()
    .try_fold(0u64, |a, b| a.checked_add(b).ok_or(FullRelationError::Budget))?;
    if extra > l.max_extra_heap_bytes
        || header.checked_add(mul(u64::from(l.max_claim_values), 16)?).is_none_or(|n| n > prep.limits.max_record_heap_bytes)
    {
        return Err(FullRelationError::Budget);
    }
    reserver(extra)
}
impl FullRelationPreparationV2 {
    /// Hash/shape-check the producer's claims once, retaining only header/IDs
    /// and one tile. This does not execute a model or certify its arithmetic.
    /// Owned binding/generated inputs already exist at entry: the caller must
    /// reserve every export/copy BEFORE creating it and keep that owner until
    /// this factory has acquired its mandatory transferred-record lease.
    /// Independently retained preparation/provider/material/aliases stay under
    /// their external owner, including the stated signature implementation's
    /// actual scratch bound. A nonzero declaration alone is not qualification.
    #[allow(clippy::too_many_arguments)]
    pub fn freeze_stream_record<R: ClaimsReaderV2>(
        &self,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        author: PrincipalId,
        reader: &mut R,
        limits: StreamRecordLimitsV2,
        reserver: &StreamRecordReserverV2<'_>,
    ) -> Result<StreamPanelRecordV2, FullRelationError> {
        let lease = acquire(self, &binding, &generated, &reader, limits, reserver)?;
        self.validate_binding(&binding, &generated)?;
        let policy = self.limits.words();
        let writer = record_hash_prefix(
            self.result.id(),
            author,
            &binding,
            &generated,
            &policy,
            self.form,
            &self.domains,
            limits.max_record_bytes,
        )?;
        let mut stream = ClaimStreamV2 { reader, writer, limits, meter: StreamReadMeterV2::default(), count: 0, failed: false };
        stream.start_count(self.plan.summary().relations)?;
        while stream.meter.relations_read < stream.count {
            drop(stream.next(self)?)
        }
        stream.eof()?;
        Ok(StreamPanelRecordV2 {
            binding,
            generated,
            author,
            commitment: stream.digest(),
            domains: self.domains.clone(),
            _lease: lease,
        })
    }
    /// Receiver boundary for existing literal signed record metadata + claims
    /// bytes. No producer marker or saved acceptance flag is trusted. The Node
    /// reads claims ONCE and cannot emit a verdict before exact EOF/hash/signature.
    /// Input decoding, metadata copies and independent preparation/material
    /// lifetimes remain externally reserved; only transferred record/read scratch
    /// is acquired here. Keep all coexisting owners accounted during transfer.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_stream<R: ClaimsReaderV2>(
        self,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        author: PrincipalId,
        commitment: Digest,
        reader: R,
        key: [u8; PUBKEY_BYTES],
        signature: [u8; SIGNATURE_BYTES],
        limits: StreamRecordLimitsV2,
        reserver: &StreamRecordReserverV2<'_>,
    ) -> Result<StreamFullRelationCheckerV2<R>, FullRelationError> {
        let lease = acquire(&self, &binding, &generated, &reader, limits, reserver)?;
        self.validate_binding(&binding, &generated)?;
        if principal(&key) != author {
            return Err(FullRelationError::Signature);
        }
        let writer = record_hash_prefix(
            self.result.id(),
            author,
            &binding,
            &generated,
            &self.limits.words(),
            self.form,
            &self.domains,
            limits.max_record_bytes,
        )?;
        // Same 2*R*G upfront allowance as resident begin: it includes this
        // reader's single incremental shape scan. Shared submit charges each
        // numerical relation (including unsuccessful delivery) separately.
        let geometry = mul(mul(self.plan.summary().relations, 2)?, self.plan.summary().geometry_visit_bound)?;
        let mut stream = ClaimStreamV2 { reader, writer, limits, meter: StreamReadMeterV2::default(), count: 0, failed: false };
        stream.start_count(self.plan.summary().relations)?;
        Ok(StreamFullRelationCheckerV2 {
            inner: FullRelationCheckerV2 {
                prepared: self,
                record: StreamDataV2 { binding, generated, index: 0, current: None },
                progress: FullRelationProgress { charged_geometry_visits: geometry, ..Default::default() },
                best: None,
                next_lane: 0,
            },
            stream,
            commitment,
            key,
            signature,
            pending_mismatch: false,
            closed: false,
            _lease: lease,
        })
    }
}
/// This control signal conveys no culprit or outcome before authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamSubmitV2 {
    Continue,
    NeedEof,
}
pub struct StreamFullRelationCheckerV2<R: ClaimsReaderV2> {
    inner: FullRelationCheckerV2<StreamDataV2>,
    stream: ClaimStreamV2<R>,
    commitment: Digest,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
    pending_mismatch: bool,
    closed: bool,
    _lease: StreamRecordLeaseV2,
}
impl<R: ClaimsReaderV2> StreamFullRelationCheckerV2<R> {
    pub fn preparation(&self) -> &FullRelationPreparationV2 {
        &self.inner.prepared
    }
    /// Progress is local coverage only, never a signed/complete computation fact.
    pub fn checked_relations(&self) -> u64 {
        self.inner.progress.next_relation
    }
    pub fn checked_legacy_frontier(&self) -> u64 {
        self.inner.progress.legacy_frontier
    }
    pub fn read_meter(&self) -> StreamReadMeterV2 {
        self.stream.meter
    }
    pub fn submit(
        &mut self,
        material: &TirSourceOnlyMaterialV2<'_>,
        target: Option<StepTargetV2<'_>>,
    ) -> Result<StreamSubmitV2, FullRelationError> {
        if self.closed {
            return Err(FullRelationError::Closed);
        }
        if self.pending_mismatch {
            return Ok(StreamSubmitV2::NeedEof);
        }
        if self.inner.record.current.is_none() {
            match self.stream.next(&self.inner.prepared) {
                Ok(claim) => {
                    self.inner.record.index = self.inner.progress.next_relation;
                    self.inner.record.current = Some(claim)
                }
                Err(e) => {
                    self.closed = true;
                    return Err(e);
                }
            }
        }
        match self.inner.submit(material, target) {
            Ok(()) => {
                self.inner.record.current = None;
                Ok(if self.inner.progress.next_relation == self.stream.count {
                    StreamSubmitV2::NeedEof
                } else {
                    StreamSubmitV2::Continue
                })
            }
            Err(FullRelationError::PanelMismatch) => {
                self.pending_mismatch = true;
                self.inner.record.current = None;
                Ok(StreamSubmitV2::NeedEof)
            }
            Err(e @ FullRelationError::Delivery) => Err(e),
            Err(e) => {
                self.closed = true;
                Err(e)
            }
        }
    }
    /// Authenticate the SAME observed bytes before exposing ANY outcome.
    /// A changed tape/short IO/wrong root never attributes a Miner error. A
    /// pending local mismatch may reject only the authenticated Panel witness.
    pub fn finish(&mut self) -> Result<CheckOutcome, FullRelationError> {
        if self.closed {
            return Err(FullRelationError::Closed);
        }
        if !self.pending_mismatch && self.inner.progress.next_relation != self.stream.count {
            return Err(FullRelationError::Incomplete);
        }
        self.closed = true;
        if self.pending_mismatch {
            while self.stream.meter.relations_read < self.stream.count {
                drop(self.stream.next(&self.inner.prepared)?);
            }
        }
        self.stream.eof()?;
        if self.stream.digest() != self.commitment {
            return Err(FullRelationError::Delivery);
        }
        if !verify_signature(&self.key, self.commitment, &self.signature, &self.inner.prepared.domains.signature_context) {
            return Err(FullRelationError::Signature);
        }
        if self.pending_mismatch {
            return Ok(CheckOutcome::WitnessRejected);
        }
        self.inner.finish()
    }
}
