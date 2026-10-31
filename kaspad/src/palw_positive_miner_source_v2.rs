//! Own-node source duty adapter. Its constructor takes the actual consensus
//! session, never a peer DTO, arbitrary read-port implementation or saved flag.
//! Run on a bounded blocking worker; a session must not be retained by a queue.
use kaspa_consensus_core::palw_positive_source_read_v2::{PositiveSourceReadErrorV2, PositiveSourceReadV2, PositiveSourceViewIdV2};
use kaspa_consensusmanager::ConsensusProxy;
use misaka_palw_positive_contract::{
    identity::{PUBKEY_BYTES, RequestId, ResultId, SIGNATURE_BYTES},
    messages::{AdmissionContext, MinerDataStatus, MinerResultCommit, MinerServiceContract, SourceRequest},
    ports::MinerSource,
};
use misaka_palw_remote::miner::positive_v2::{
    CanonicalDataDutyV2, DutyBoundsV2, MinerErrorV2, MinerResponderV2, PreparationMeterV2, PreparedDataV2, ProcessLimitsV2,
    SourceUnitBoundsV2, StoredProviderV2,
    recovery::{DurableMinerResponderV2, JournalLimitsV2},
    rocksdb::{RocksAnchorDbV2, RocksCaseAnchorV2},
};
use std::path::Path;

/// Caller-selected recovery mode. An absent/corrupt protected store must never
/// automatically fall back to Create or replace the namespace after reorg.
pub enum SourceJournalOpenV2 {
    Create,
    Resume,
    DiscardUnanchoredTail,
}
pub struct SignedSourceCaseV2<'a> {
    pub admission: &'a AdmissionContext,
    pub result: MinerResultCommit,
    pub service: MinerServiceContract,
    pub key: &'a [u8; PUBKEY_BYTES],
    pub signature: &'a [u8; SIGNATURE_BYTES],
    pub now: u64,
}

/// Concrete own-node path: authenticated local result -> exclusive protected
/// case lease -> write-ahead journal -> actual-session native duty -> G14 units.
/// No method treats these local units as accepted service or a payable claim.
pub struct NativeMinerSourceV2<'p> {
    result: MinerResultCommit,
    responder: DurableMinerResponderV2<'p, NativeSourceDutyV2, RocksCaseAnchorV2>,
}
impl<'p> NativeMinerSourceV2<'p> {
    pub fn open(
        signed: SignedSourceCaseV2<'_>,
        providers: &'p [&'p dyn StoredProviderV2],
        process_limits: ProcessLimitsV2,
        database: &RocksAnchorDbV2,
        journal: &Path,
        journal_limits: JournalLimitsV2,
        mode: SourceJournalOpenV2,
    ) -> Result<Self, MinerErrorV2> {
        // Authenticate before even occupying the immutable protected case slot.
        let inner = MinerResponderV2::new(
            signed.admission,
            signed.result.clone(),
            signed.service,
            signed.now,
            signed.key,
            signed.signature,
            providers,
            process_limits,
        )?;
        let anchors = database.bind(&inner)?;
        let responder = match mode {
            SourceJournalOpenV2::Create => DurableMinerResponderV2::create(journal, inner, journal_limits, anchors),
            SourceJournalOpenV2::Resume => DurableMinerResponderV2::resume(journal, inner, journal_limits, anchors),
            SourceJournalOpenV2::DiscardUnanchoredTail => {
                DurableMinerResponderV2::resume_discarding_unanchored_tail(journal, inner, journal_limits, anchors)
            }
        }?;
        Ok(Self { result: signed.result, responder })
    }

    /// Call in one bounded blocking worker and drop its session after this
    /// response. Data is submitted through A4 op4; intake independently resolves
    /// the canonical request, rechecks G14 data and creates the actual receipt.
    pub fn respond(&mut self, session: ConsensusProxy, request: RequestId) -> Result<PreparedDataV2, MinerErrorV2> {
        let duty = NativeSourceDutyV2::resolve(session, request, &self.result)?;
        self.responder.respond(&duty)
    }

    pub fn meter(&self) -> PreparationMeterV2 {
        self.responder.meter()
    }
}

/// Not serializable or constructible outside the own-node adapter. The read
/// port's concrete implementation belongs to consensus and must recheck the
/// current state generation/root/accepted prefix on every call.
pub(crate) struct NativeSourceDutyV2 {
    _session: ConsensusProxy,
    read: Box<dyn PositiveSourceReadV2>,
    request: SourceRequest,
    result: ResultId,
    view: PositiveSourceViewIdV2,
}
impl NativeSourceDutyV2 {
    pub(crate) fn resolve(session: ConsensusProxy, request: RequestId, result: &MinerResultCommit) -> Result<Self, MinerErrorV2> {
        let read = session
            .with_consensus_api_v1(|consensus| consensus.palw_positive_source_duty_v2(request, result.id()))
            .map_err(source_error)?;
        let literal = *read.request();
        if literal.id() != request || literal.case != result.case_id() || literal.contract != result.service_contract {
            return Err(MinerErrorV2::Binding);
        }
        let duty = Self { view: read.view_id(), _session: session, read, request: literal, result: result.id() };
        duty.authorize(result)?;
        Ok(duty)
    }

    fn check_view(&self) -> Result<(), MinerErrorV2> {
        if self.read.view_id() != self.view || self.read.request() != &self.request {
            return Err(MinerErrorV2::Binding);
        }
        Ok(())
    }
}
impl CanonicalDataDutyV2 for NativeSourceDutyV2 {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        self.check_view()?;
        if result.id() != self.result {
            return Err(MinerErrorV2::Binding);
        }
        let bounds = self.read.authorize(result).map_err(source_error)?;
        self.request.validate(bounds.source_units)?;
        if bounds.now >= bounds.cutoff {
            return Err(MinerErrorV2::Closed);
        }
        Ok(DutyBoundsV2 {
            source_units: bounds.source_units,
            max_unit_bytes: bounds.max_unit_bytes,
            work_per_read: bounds.work_per_read,
            budget: bounds.budget,
            now: bounds.now,
            cutoff: bounds.cutoff,
        })
    }
    fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        self.check_view()?;
        if !(self.request.first..self.request.end).contains(&index) {
            return Err(MinerErrorV2::Binding);
        }
        self.read.status(index).map_err(source_error)
    }
    fn unit_bounds(&self, index: u64) -> Result<Option<SourceUnitBoundsV2>, MinerErrorV2> {
        self.check_view()?;
        if !(self.request.first..self.request.end).contains(&index) {
            return Err(MinerErrorV2::Binding);
        }
        let bound = self.read.unit_bounds(index).map_err(source_error)?;
        Ok(Some(SourceUnitBoundsV2 { data_bytes: bound.data_bytes, body_bytes: bound.body_bytes }))
    }
}
fn source_error(error: PositiveSourceReadErrorV2) -> MinerErrorV2 {
    match error {
        PositiveSourceReadErrorV2::Deadline | PositiveSourceReadErrorV2::StaleView | PositiveSourceReadErrorV2::Inactive => {
            MinerErrorV2::Closed
        }
        PositiveSourceReadErrorV2::Unavailable | PositiveSourceReadErrorV2::Missing | PositiveSourceReadErrorV2::UnsupportedSource => {
            MinerErrorV2::Unavailable
        }
        PositiveSourceReadErrorV2::Budget => MinerErrorV2::Budget,
        PositiveSourceReadErrorV2::Binding | PositiveSourceReadErrorV2::Range => MinerErrorV2::Binding,
    }
}
