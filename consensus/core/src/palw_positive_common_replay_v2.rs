//! Read-only replay of the common A0/A1/A4 Court from a complete case journal.
//!
//! Link checking is identity checking, not native accepted-prefix authority.
//! Every native port below is mandatory and has no successful default. The
//! checker itself is fixed to the real A1 TIR implementation. A local result
//! cannot create an admitted case, spend/lock funds, or authorize Gate C credit.
//! No live admission prepare/commit or saved-progress deserializer is used.
use crate::{
    Hash64,
    palw_positive_check_v2::CheckLimits,
    palw_positive_contract_v2::{AdapterError, CheckedTerminal, PreparedCheck, record::RecordLimits},
    palw_positive_court_v2::coordinator::{
        carrier::PanelPacketSignatureV2,
        contract::{
            CandidateOpenV2, CarrierVerifierV2, CheckedCourtFactV2, ContractAttemptV2, ContractCourtV2, CourtErrorV2, CourtLimitsV2,
            TerminalKindV2,
        },
        native_body::{
            NativeBodyErrorV2, NativeBodyLimitsV2, PositiveNativeBodyV2, PositiveNativeOperationV2, decode_native_body_v2,
            scan_native_body_v2,
        },
        record_ingress::{RecordIngressErrorV2, preflight_witness_record_v2},
        tir_checker::TirCourtCheckerV2,
    },
    palw_positive_journal_chain_v2::{
        CaseJournalChainErrorV2, CaseJournalPinV2, CaseJournalSourceV2, CaseJournalVisitErrorV2, CheckedCaseJournalLinksV2,
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    funding::{FundingConsent, FundingPlanBasis},
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES, WitnessId},
    journal::{CaseJournalEventKind, CaseJournalRecordView},
    messages::{AdmissionContext, ClaimStatus, MinerResultCommit, MinerServiceContract},
    rows::AcceptedMaterial,
    wire::EvidenceFrame,
};
use std::mem::size_of;

pub type LocalCheckedCourtFactV2 = CheckedCourtFactV2<CheckedTerminal>;

/// Independently resolved historical inputs, not peer-owned class/job data.
/// Their allocations and the A1 preparation/context/progress copies must be in
/// NativeReplayStepQuoteV2's native heap envelope before this method is called.
pub struct RootedReplayAdmissionV2<'a> {
    pub context: AdmissionContext,
    pub registered: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub job: &'a PalwJobContextV2,
    pub form: PalwPromptIdsFormV1,
    pub check_limits: CheckLimits,
    pub record_limits: RecordLimits,
    pub court_limits: CourtLimitsV2,
    pub miner_key: &'a [u8; PUBKEY_BYTES],
}

/// Literal already-preflighted actual op9 fields, borrowed from the concrete
/// native replay environment. No synthetic old ResultAdmission is constructed.
/// This is local replay DATA, never accepted Case, funding or P authority.
pub struct AssignedInitialReplayV2<'a> {
    pub result: &'a MinerResultCommit,
    pub miner_signature: &'a [u8; SIGNATURE_BYTES],
    pub service: &'a MinerServiceContract,
    pub basis: &'a FundingPlanBasis,
    pub consent: &'a FundingConsent,
    pub payer_signature: &'a [u8; SIGNATURE_BYTES],
    pub admission: RootedReplayAdmissionV2<'a>,
}

/// A native owner derives this from bounded raw input and actual historical
/// policy/metadata, BEFORE any corresponding verification or allocation. Work
/// includes signatures, opaque G14/record preflight, context comparisons, A1
/// derivation/evaluation and external source/funding/attestation validation.
/// The first record's quote must also include fixed setup and final freshness,
/// row reconciliation and clock-validation work, not just per-event work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeReplayStepQuoteV2 {
    pub work_units: u64,
    pub hash_bytes: u64,
    /// Maximum simultaneous native-owned heap for the ENTIRE replay state at
    /// this step: retained inputs/prepared plan/current context/progress plus
    /// temporary clones, interpreter, Court receipt Vec, verifier, historical
    /// guard/resolver/ref scratch and cumulative external shadow. Excludes
    /// separately charged engine buffers below.
    pub peak_native_heap_bytes: u64,
}

/// Explicit caller policy; none of these limits is a public-network default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommonReplayLimitsV2 {
    pub body: NativeBodyLimitsV2,
    pub max_events: u64,
    pub max_facts: u64,
    pub max_total_record_bytes: u64,
    pub max_hash_bytes: u64,
    pub max_work_units: u64,
    /// Includes retained link index/token, output/fact/diagnostic storage, one
    /// borrowed-record backing buffer, body allocation/inline and native peak.
    /// Logical reservation, not a compiler stack-frame or physical RSS bound.
    pub max_known_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommonReplayQuoteV2 {
    pub events: u64,
    pub record_reads: u64,
    pub total_record_bytes: u64,
    pub hash_bytes: u64,
    pub work_units: u64,
    pub known_buffer_bytes: u64,
    pub native_peak_heap_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalCourtSnapshotV2 {
    pub status: ClaimStatus,
    pub attempts: u32,
    pub cursor: u64,
    pub last_attempt: Option<ContractAttemptV2>,
    pub charged_wire_bytes: u64,
    pub charged_elements: u64,
    pub charged_terms: u64,
    pub reserved_receipt_bytes: u64,
}
fn snapshot<V: CarrierVerifierV2>(court: &ContractCourtV2<TirCourtCheckerV2, V>) -> LocalCourtSnapshotV2 {
    LocalCourtSnapshotV2 {
        status: court.status(),
        attempts: court.attempts(),
        cursor: court.cursor(),
        last_attempt: court.last_attempt(),
        charged_wire_bytes: court.charged_wire_bytes(),
        charged_elements: court.charged_elements(),
        charged_terms: court.charged_terms(),
        reserved_receipt_bytes: court.reserved_receipt_bytes(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalCourtErrorV2 {
    Contract(ContractError),
    Check(AdapterError),
    Budget,
    Closed,
    Deadline,
    Order,
    Incomplete,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalCourtOutcomeV2 {
    External,
    AdmittedLocally,
    CandidateOpened,
    FrameChecked,
    Finished(TerminalKindV2),
    Expired,
    SemanticError(LocalCourtErrorV2),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalReplayEffectV2 {
    Changed,
    NoOp,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalReplayEventV2 {
    pub sequence: u64,
    /// Body operation 0..7; None denotes an actual native transaction payload.
    pub operation: Option<u8>,
    pub court: LocalCourtOutcomeV2,
    pub external: ExternalReplayEffectV2,
}

/// Native implementations must hold a stable actual accepted-prefix/root view.
/// Before invoking the engine, that owner must hold a real resource lease for
/// `CommonReplayLimitsV2::max_known_buffer_bytes` (including source backing,
/// index, body, facts/diagnostics and private native heaps) and the quoted work.
/// Limits/quotes below are checked logical bounds, not a resource-pool ticket.
/// Keep the owner/lease alive while outputs or their extracted parts remain
/// resident; this local-only API does not manufacture native RAM admission.
/// A test/peer implementation is not native authority. Public trait use can
/// produce LOCAL facts only; the separate native recovery factory must verify
/// its own genuine committed case receipt, epoch, current rows and journal head.
pub trait NativeReplayEnvironmentV2 {
    type Error;
    type CarrierError;
    type ExternalState;
    type Verifier<'a>: CarrierVerifierV2<Authorization = PanelPacketSignatureV2, Error = Self::CarrierError>
    where
        Self: 'a;
    /// Pins this exact historical step in the verifier's native-owned cursor
    /// until dropped. No fallback to admission-time/current-time authority.
    type HistoricalStepGuard<'a>
    where
        Self: 'a;

    /// Bounded, nonallocating quote, derived without trusting saved counters.
    fn quote_record(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<NativeReplayStepQuoteV2, Self::Error>;
    fn activate_historical_step<'a>(
        &'a self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
    ) -> Result<Self::HistoricalStepGuard<'a>, Self::Error>;
    /// Actual native accepted-prefix inclusion, canonical complete body/Tx and
    /// state/cost-changing case consumption. A linked DTO is insufficient.
    /// Future-position/preauth-rejected frames and fully idempotent deliveries
    /// MUST be refused. Kind1 requires actual accepted qualified native Tx data.
    fn validate_native_record(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Self::Error>;
    /// Derive the canonical sorted exact list from actual native operation
    /// references. Any temporary list storage is part of the precharged quote.
    fn with_expected_aux<T>(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        visit: impl FnOnce(&[AcceptedMaterial]) -> T,
    ) -> Result<T, Self::Error>;
    /// Verify canonical preimages, their original domains and accepted origins;
    /// root membership is data provenance, never computation certification.
    fn validate_canonical_preimages(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Self::Error>;
    fn admission_inputs<'a>(
        &'a self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        body: &PositiveNativeBodyV2,
    ) -> Result<RootedReplayAdmissionV2<'a>, Self::Error>;
    /// Optional assigned-first path, ONLY for sequence1/NativeTransaction.
    /// The actual native environment must authenticate the exact full accepted
    /// transaction/op9, consumed Assignment binding, independent expectations,
    /// original material domains and all funding/aux links. quote_record must
    /// precharge actual transaction and op9 buffers before this method runs.
    /// None is refusal, never an old op0 or saved-pass fallback.
    fn assigned_initial_inputs<'a>(
        &'a self,
        _pin: &CaseJournalPinV2,
        _record: CaseJournalRecordView<'_>,
    ) -> Result<Option<AssignedInitialReplayV2<'a>>, Self::Error> {
        Ok(None)
    }
    fn panel_key(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        witness: WitnessId,
    ) -> Result<[u8; PUBKEY_BYTES], Self::Error>;
    fn make_verifier(&self, pin: &CaseJournalPinV2) -> Result<Self::Verifier<'_>, Self::Error>;
    /// Creates a fresh source ledger/funding shadow, never restores saved usage
    /// or calls live AtomicCourtStore admission/funding commit.
    fn new_external_state(&self, pin: &CaseJournalPinV2) -> Result<Self::ExternalState, Self::Error>;
    /// Read-only source/funding/attestation replay against separate actual native
    /// verified contexts. Called for EVERY event, including initial admission,
    /// charged checker failures and kind1 settlements. Interleaved Court usage
    /// updates must be derived here from actual before/after state, not caches.
    /// Source/attestation/Tx records require Changed; NoOp is not journal input.
    #[allow(clippy::too_many_arguments)]
    fn apply_external_event(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        before: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        outcome: &LocalCourtOutcomeV2,
        facts: &[LocalCheckedCourtFactV2],
        state: &mut Self::ExternalState,
    ) -> Result<ExternalReplayEffectV2, Self::Error>;
    /// Reconcile actual native committed transition semantics, including an
    /// Err(Check) that leaves cost/candidate changes. No saved error flag grants
    /// acceptance, no unauthenticated rejection is promoted to an accepted op.
    #[allow(clippy::too_many_arguments)]
    fn validate_committed_effect(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        before: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        outcome: &LocalCourtOutcomeV2,
        external: ExternalReplayEffectV2,
        state: &Self::ExternalState,
    ) -> Result<(), Self::Error>;
    fn current_daa(&self, pin: &CaseJournalPinV2) -> Result<u64, Self::Error>;
    /// Must compare recomputed history/cost/rows/receipts and current actual
    /// scope/head/generation/clock to the pinned native view. Historical rows and
    /// present-clock expiry are distinct; no synthetic clock-origin is created.
    #[allow(clippy::too_many_arguments)]
    fn validate_final(
        &self,
        pin: &CaseJournalPinV2,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        historical: LocalCourtSnapshotV2,
        current: LocalCourtSnapshotV2,
        now: u64,
        facts: &[LocalCheckedCourtFactV2],
        state: &Self::ExternalState,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum CommonReplayErrorV2<S, E, V> {
    Chain(CaseJournalChainErrorV2<S>),
    Environment(E),
    Contract(ContractError),
    Body(NativeBodyErrorV2),
    Record(RecordIngressErrorV2),
    RecordAuthentication(crate::palw_positive_contract_v2::record::RecordDeliveryError),
    Preparation(AdapterError),
    Court(CourtErrorV2<AdapterError, V>),
    Limits,
    Budget,
    InitialAdmission,
    Identity,
    NonChangingEvent,
    RejectedCourtOperation,
    Clock,
    ChangedQuote,
}

/// All fields remain private until both complete forward visits and final
/// freshness/row checks succeed. Facts are actual A1/A4 facts, not DTO Pass bits.
pub struct LocalReplayedCaseV2<X> {
    pin: CaseJournalPinV2,
    result: MinerResultCommit,
    service: MinerServiceContract,
    historical: LocalCourtSnapshotV2,
    current: LocalCourtSnapshotV2,
    observed_daa: u64,
    facts: Vec<LocalCheckedCourtFactV2>,
    events: Vec<LocalReplayEventV2>,
    external: X,
    quote: CommonReplayQuoteV2,
}
impl<X> LocalReplayedCaseV2<X> {
    pub fn pin(&self) -> &CaseJournalPinV2 {
        &self.pin
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn service(&self) -> &MinerServiceContract {
        &self.service
    }
    pub fn historical_court(&self) -> LocalCourtSnapshotV2 {
        self.historical
    }
    pub fn current_court(&self) -> LocalCourtSnapshotV2 {
        self.current
    }
    pub fn observed_daa(&self) -> u64 {
        self.observed_daa
    }
    pub fn facts(&self) -> &[LocalCheckedCourtFactV2] {
        &self.facts
    }
    pub fn events(&self) -> &[LocalReplayEventV2] {
        &self.events
    }
    pub fn external(&self) -> &X {
        &self.external
    }
    pub fn quote(&self) -> CommonReplayQuoteV2 {
        self.quote
    }
    /// These remain local checked facts. Native rebinding requires the separate
    /// A4 genuine committed-case recovery port and current native checks.
    pub fn into_local_parts(self) -> (CaseJournalPinV2, Vec<LocalCheckedCourtFactV2>, X) {
        (self.pin, self.facts, self.external)
    }
}

type Error<S, E> = CommonReplayErrorV2<
    <S as CaseJournalSourceV2>::Error,
    <E as NativeReplayEnvironmentV2>::Error,
    <E as NativeReplayEnvironmentV2>::CarrierError,
>;

fn map_visit<S: CaseJournalSourceV2, E: NativeReplayEnvironmentV2>(
    error: CaseJournalVisitErrorV2<S::Error, Error<S, E>>,
) -> Error<S, E> {
    match error {
        CaseJournalVisitErrorV2::Chain(error) => CommonReplayErrorV2::Chain(error),
        CaseJournalVisitErrorV2::Visitor(error) => error,
    }
}

#[derive(Default)]
struct Preflight {
    native_work: u64,
    native_hash: u64,
    native_heap: u64,
    record_peak: u64,
    body_peak: u64,
}
fn add(a: u64, b: u64) -> Result<u64, ()> {
    a.checked_add(b).ok_or(())
}
fn mul(a: u64, b: u64) -> Result<u64, ()> {
    a.checked_mul(b).ok_or(())
}

fn quote_replay<S: CaseJournalSourceV2, E: NativeReplayEnvironmentV2>(
    links: &CheckedCaseJournalLinksV2,
    limits: CommonReplayLimitsV2,
    preflight: &Preflight,
    facts_capacity: u64,
    events_capacity: u64,
) -> Result<CommonReplayQuoteV2, Error<S, E>> {
    let calculate = || -> Result<CommonReplayQuoteV2, ()> {
        let link = links.quote();
        // Link quote already includes backwalk + one forward visit. Twice that
        // conservatively covers backwalk + our two forward visits (4N >= 3N).
        let raw = mul(link.total_record_bytes, 2)?;
        let hash = add(mul(link.hash_bytes, 2)?, mul(preflight.native_hash, 2)?)?;
        let work = add(add(raw, mul(link.record_decodes, 2)?)?, mul(preflight.native_work, 2)?)?;
        let output = add(
            add(
                size_of::<LocalReplayedCaseV2<E::ExternalState>>() as u64,
                mul(facts_capacity, size_of::<LocalCheckedCourtFactV2>() as u64)?,
            )?,
            mul(events_capacity, size_of::<LocalReplayEventV2>() as u64)?,
        )?;
        let buffers = [
            link.index_bytes,
            link.retained_inline_bytes,
            link.decode_workspace_bytes,
            preflight.record_peak,
            preflight.body_peak,
            preflight.native_heap,
            output,
        ]
        .into_iter()
        .try_fold(0, add)?;
        Ok(CommonReplayQuoteV2 {
            events: link.events,
            record_reads: mul(link.record_reads, 2)?,
            total_record_bytes: raw,
            hash_bytes: hash,
            work_units: work,
            known_buffer_bytes: buffers,
            native_peak_heap_bytes: preflight.native_heap,
        })
    };
    let q = calculate().map_err(|_| CommonReplayErrorV2::Budget)?;
    if q.events > limits.max_events
        || facts_capacity > limits.max_facts
        || q.total_record_bytes > limits.max_total_record_bytes
        || q.hash_bytes > limits.max_hash_bytes
        || q.work_units > limits.max_work_units
        || q.known_buffer_bytes > limits.max_known_buffer_bytes
    {
        return Err(CommonReplayErrorV2::Budget);
    }
    Ok(q)
}

fn validate_native<S: CaseJournalSourceV2, E: NativeReplayEnvironmentV2>(
    env: &E,
    pin: &CaseJournalPinV2,
    record: CaseJournalRecordView<'_>,
) -> Result<(), Error<S, E>> {
    env.validate_native_record(pin, record).map_err(CommonReplayErrorV2::Environment)?;
    env.with_expected_aux(pin, record, |refs| record.require_exact_refs(refs))
        .map_err(CommonReplayErrorV2::Environment)?
        .map_err(CommonReplayErrorV2::Contract)?;
    env.validate_canonical_preimages(pin, record).map_err(CommonReplayErrorV2::Environment)
}

/// Replays the fixed independent checker. The caller supplies already checked
/// LINKS, not native authority. All effects below remain private read-only
/// shadow state until complete history, final rows and current clock agree.
pub fn replay_common_case_v2<S: CaseJournalSourceV2, E: NativeReplayEnvironmentV2>(
    source: &S,
    links: &CheckedCaseJournalLinksV2,
    env: &E,
    limits: CommonReplayLimitsV2,
) -> Result<LocalReplayedCaseV2<E::ExternalState>, Error<S, E>> {
    replay_common_case_retained_v2(source, links, env, limits).map(|(local, _machine)| local)
}

/// Nonserialized checker continuation. Only the same actual native owner may
/// retain this under its exact current root/head/epoch/lineage and real lease.
/// It carries real A1 progress, not reconstructed saved counters or Pass flags.
pub(crate) struct RetainedCommonCourtV2<V: CarrierVerifierV2> {
    court: ContractCourtV2<TirCourtCheckerV2, V>,
    prepared: PreparedCheck,
    record_limits: RecordLimits,
}
impl<V: CarrierVerifierV2> RetainedCommonCourtV2<V> {
    pub(crate) fn into_parts(self) -> (ContractCourtV2<TirCourtCheckerV2, V>, PreparedCheck, RecordLimits) {
        (self.court, self.prepared, self.record_limits)
    }
}

pub(crate) fn replay_common_case_retained_v2<'a, S: CaseJournalSourceV2, E: NativeReplayEnvironmentV2>(
    source: &S,
    links: &CheckedCaseJournalLinksV2,
    env: &'a E,
    limits: CommonReplayLimitsV2,
) -> Result<(LocalReplayedCaseV2<E::ExternalState>, RetainedCommonCourtV2<E::Verifier<'a>>), Error<S, E>> {
    if [
        limits.max_events,
        limits.max_facts,
        limits.max_total_record_bytes,
        limits.max_hash_bytes,
        limits.max_work_units,
        limits.max_known_buffer_bytes,
    ]
    .contains(&0)
    {
        return Err(CommonReplayErrorV2::Limits);
    }
    let pin = *links.pin();
    let count = links.quote().events;
    // Source.with_record may allocate its backing buffer BEFORE our visitor.
    // Reserve the source's complete allowed record cap before the first visit,
    // and the body decoder's complete opaque cap before any owned body decode.
    let mut preflight = Preflight {
        record_peak: links.record_limits().max_record_bytes,
        body_peak: add(
            size_of::<PositiveNativeBodyV2>() as u64,
            mul(limits.body.max_opaque_bytes, 2).map_err(|_| CommonReplayErrorV2::Budget)?,
        )
        .map_err(|_| CommonReplayErrorV2::Budget)?,
        ..Preflight::default()
    };
    // Reserve worst-case fact/diagnostic storage before any native verification.
    let fact_limit = limits.max_facts.min(count);
    quote_replay::<S, E>(links, limits, &preflight, fact_limit, count)?;
    links
        .visit_forward(source, |record| -> Result<(), Error<S, E>> {
            let step = env.quote_record(&pin, record).map_err(CommonReplayErrorV2::Environment)?;
            if step.work_units == 0 || step.peak_native_heap_bytes == 0 {
                return Err(CommonReplayErrorV2::Limits);
            }
            preflight.native_work = add(preflight.native_work, step.work_units).map_err(|_| CommonReplayErrorV2::Budget)?;
            preflight.native_hash = add(preflight.native_hash, step.hash_bytes).map_err(|_| CommonReplayErrorV2::Budget)?;
            preflight.native_heap = preflight.native_heap.max(step.peak_native_heap_bytes);
            preflight.record_peak = preflight.record_peak.max(record.quote().record_bytes);
            if record.header().kind == CaseJournalEventKind::NativeBody {
                let summary = scan_native_body_v2(record.payload(), limits.body).map_err(CommonReplayErrorV2::Body)?;
                preflight.body_peak = preflight.body_peak.max(
                    add(
                        size_of::<PositiveNativeBodyV2>() as u64,
                        mul(summary.opaque_bytes, 2).map_err(|_| CommonReplayErrorV2::Budget)?,
                    )
                    .map_err(|_| CommonReplayErrorV2::Budget)?,
                );
            }
            quote_replay::<S, E>(links, limits, &preflight, fact_limit, count)?;
            let _guard = env.activate_historical_step(&pin, record).map_err(CommonReplayErrorV2::Environment)?;
            validate_native::<S, E>(env, &pin, record)
        })
        .map_err(map_visit::<S, E>)?;

    let mut events = Vec::new();
    events
        .try_reserve_exact(usize::try_from(count).map_err(|_| CommonReplayErrorV2::Budget)?)
        .map_err(|_| CommonReplayErrorV2::Budget)?;
    quote_replay::<S, E>(links, limits, &preflight, fact_limit, events.capacity() as u64)?;
    let mut facts = Vec::new();
    let mut court = None;
    let mut prepared = None;
    let mut record_limits = None;
    let mut service = None;
    let mut attempt_limit = 0u64;
    let mut external = env.new_external_state(&pin).map_err(CommonReplayErrorV2::Environment)?;
    let mut execution_work = 0u64;
    let mut execution_hash = 0u64;
    let mut latest_daa = 0u64;
    links
        .visit_forward(source, |record| -> Result<(), Error<S, E>> {
            let header = record.header();
            latest_daa = latest_daa.max(header.origin.accepted_daa);
            let step = env.quote_record(&pin, record).map_err(CommonReplayErrorV2::Environment)?;
            execution_work = add(execution_work, step.work_units).map_err(|_| CommonReplayErrorV2::Budget)?;
            execution_hash = add(execution_hash, step.hash_bytes).map_err(|_| CommonReplayErrorV2::Budget)?;
            if step.work_units == 0
                || step.peak_native_heap_bytes > preflight.native_heap
                || execution_work > preflight.native_work
                || execution_hash > preflight.native_hash
            {
                return Err(CommonReplayErrorV2::ChangedQuote);
            }
            let _guard = env.activate_historical_step(&pin, record).map_err(CommonReplayErrorV2::Environment)?;
            validate_native::<S, E>(env, &pin, record)?;
            let body = if header.kind == CaseJournalEventKind::NativeBody {
                Some(decode_native_body_v2(record.payload(), limits.body).map_err(CommonReplayErrorV2::Body)?)
            } else {
                None
            };
            let before = court.as_ref().map(snapshot);
            let operation = body.as_ref().map(|_| record.payload()[4]);
            let outcome = if header.sequence == 1 {
                let (result, miner_signature, signed_service, input) = if let Some(body) = body.as_ref() {
                    let PositiveNativeOperationV2::ResultAdmission { result, miner_signature, service: signed_service, .. } =
                        &body.operation
                    else {
                        return Err(CommonReplayErrorV2::InitialAdmission);
                    };
                    let input = env.admission_inputs(&pin, record, body).map_err(CommonReplayErrorV2::Environment)?;
                    (result, miner_signature, signed_service, input)
                } else {
                    if header.kind != CaseJournalEventKind::NativeTransaction {
                        return Err(CommonReplayErrorV2::InitialAdmission);
                    }
                    let assigned = env
                        .assigned_initial_inputs(&pin, record)
                        .map_err(CommonReplayErrorV2::Environment)?
                        .ok_or(CommonReplayErrorV2::InitialAdmission)?;
                    assigned
                        .consent
                        .validate(assigned.result, assigned.service, assigned.basis, header.origin.accepted_daa)
                        .map_err(|error| CommonReplayErrorV2::Court(CourtErrorV2::Contract(error)))?;
                    // Literal r1 payload stays op9. Generic engine neither
                    // synthesizes op0 nor claims native accepted authority.
                    (assigned.result, assigned.miner_signature, assigned.service, assigned.admission)
                };
                if result.scope != pin.scope || result.case_id() != pin.case || result.id() != pin.result {
                    return Err(CommonReplayErrorV2::Identity);
                }
                attempt_limit = u64::from(input.court_limits.max_attempts);
                if attempt_limit == 0 {
                    return Err(CommonReplayErrorV2::Limits);
                }
                let capacity = fact_limit.min(attempt_limit);
                facts
                    .try_reserve_exact(usize::try_from(capacity).map_err(|_| CommonReplayErrorV2::Budget)?)
                    .map_err(|_| CommonReplayErrorV2::Budget)?;
                if facts.capacity() as u64 > capacity {
                    return Err(CommonReplayErrorV2::Budget);
                }
                quote_replay::<S, E>(links, limits, &preflight, facts.capacity() as u64, events.capacity() as u64)?;
                prepared = Some(
                    PreparedCheck::authenticate(
                        &input.context,
                        input.registered,
                        input.artifact,
                        input.job,
                        input.form,
                        input.check_limits,
                        result.clone(),
                        input.miner_key,
                        miner_signature,
                    )
                    .map_err(CommonReplayErrorV2::Preparation)?,
                );
                court = Some(
                    ContractCourtV2::new(
                        TirCourtCheckerV2,
                        env.make_verifier(&pin).map_err(CommonReplayErrorV2::Environment)?,
                        &input.context,
                        result.clone(),
                        input.miner_key,
                        miner_signature,
                        *signed_service,
                        input.court_limits,
                        header.origin.accepted_daa,
                    )
                    .map_err(CommonReplayErrorV2::Court)?,
                );
                record_limits = Some(input.record_limits);
                service = Some(*signed_service);
                LocalCourtOutcomeV2::AdmittedLocally
            } else {
                let court = court.as_mut().ok_or(CommonReplayErrorV2::InitialAdmission)?;
                match body.as_ref().map(|b| &b.operation) {
                    Some(PositiveNativeOperationV2::ResultAdmission { .. }) => return Err(CommonReplayErrorV2::InitialAdmission),
                    Some(PositiveNativeOperationV2::WitnessCommit { witness, panel_signature, .. }) => {
                        let record_limits = record_limits.ok_or(CommonReplayErrorV2::InitialAdmission)?;
                        let borrowed = preflight_witness_record_v2(
                            court.result(),
                            body.as_ref().expect("witness body"),
                            record_limits.max_record_bytes,
                        )
                        .map_err(CommonReplayErrorV2::Record)?;
                        let key = env.panel_key(&pin, record, witness.id()).map_err(CommonReplayErrorV2::Environment)?;
                        let context = prepared
                            .as_ref()
                            .ok_or(CommonReplayErrorV2::InitialAdmission)?
                            .authenticate_record(witness.clone(), borrowed.record_bytes(), record_limits, &key, panel_signature)
                            .map_err(CommonReplayErrorV2::RecordAuthentication)?;
                        match court.open_candidate(witness.clone(), &key, panel_signature, context, header.origin.accepted_daa) {
                            Ok(CandidateOpenV2::Opened) => LocalCourtOutcomeV2::CandidateOpened,
                            Ok(CandidateOpenV2::AlreadyActive) => return Err(CommonReplayErrorV2::NonChangingEvent),
                            Err(error) => semantic_error(error)?,
                        }
                    }
                    Some(PositiveNativeOperationV2::ExactFrame { frame, panel_signature }) => {
                        let frame =
                            EvidenceFrame::decode(frame, limits.body.max_opaque_bytes).map_err(CommonReplayErrorV2::Contract)?;
                        let active = court.last_attempt().ok_or(CommonReplayErrorV2::RejectedCourtOperation)?;
                        let fh = frame.header();
                        // Pure Court currently charges some order errors. Native
                        // journal rules reject them BEFORE Case cost, so never call
                        // that mutable path for a future/repeated/foreign relation.
                        if fh.case != pin.case
                            || fh.result != pin.result
                            || fh.witness != active.witness
                            || fh.relation_count != 1
                            || fh.first_relation != court.cursor()
                        {
                            return Err(CommonReplayErrorV2::RejectedCourtOperation);
                        }
                        let key = env.panel_key(&pin, record, fh.witness).map_err(CommonReplayErrorV2::Environment)?;
                        let authorization = PanelPacketSignatureV2 { key, signature: *panel_signature };
                        match court.submit_frame(frame, &authorization, header.origin.accepted_daa) {
                            Ok(()) => LocalCourtOutcomeV2::FrameChecked,
                            Err(error) => semantic_error(error)?,
                        }
                    }
                    Some(PositiveNativeOperationV2::Finish { case, result, witness }) => {
                        if *case != pin.case || *result != pin.result || court.last_attempt().map(|a| a.witness) != Some(*witness) {
                            return Err(CommonReplayErrorV2::Identity);
                        }
                        match court.finish(header.origin.accepted_daa) {
                            Ok(fact) => {
                                if facts.len() as u64 >= fact_limit
                                    || facts.len() as u64 >= attempt_limit
                                    || facts.len() as u64 >= u64::from(court.attempts())
                                {
                                    return Err(CommonReplayErrorV2::Budget);
                                }
                                let kind = fact.terminal_kind();
                                facts.push(fact);
                                LocalCourtOutcomeV2::Finished(kind)
                            }
                            Err(error) => semantic_error(error)?,
                        }
                    }
                    Some(PositiveNativeOperationV2::Expire { case, result }) => {
                        if *case != pin.case || *result != pin.result {
                            return Err(CommonReplayErrorV2::Identity);
                        }
                        match court.expire(header.origin.accepted_daa) {
                            Ok(()) => LocalCourtOutcomeV2::Expired,
                            Err(error) => semantic_error(error)?,
                        }
                    }
                    Some(PositiveNativeOperationV2::SourceRequest { request, .. }) => {
                        if request.case != pin.case {
                            return Err(CommonReplayErrorV2::Identity);
                        }
                        LocalCourtOutcomeV2::External
                    }
                    Some(PositiveNativeOperationV2::SourceResponse { case, .. }) => {
                        if *case != pin.case {
                            return Err(CommonReplayErrorV2::Identity);
                        }
                        LocalCourtOutcomeV2::External
                    }
                    Some(PositiveNativeOperationV2::PositiveCourtAttestation { .. }) | None => LocalCourtOutcomeV2::External,
                }
            };
            let after = snapshot(court.as_ref().ok_or(CommonReplayErrorV2::InitialAdmission)?);
            let court_changed = before != Some(after);
            // A Court error with no state/cost effect is rejected, even if a native
            // adapter accidentally tries to call it an accepted charged operation.
            if matches!(outcome, LocalCourtOutcomeV2::SemanticError(_)) && !court_changed {
                return Err(CommonReplayErrorV2::RejectedCourtOperation);
            }
            let external_effect = env
                .apply_external_event(&pin, record, body.as_ref(), before, after, &outcome, &facts, &mut external)
                .map_err(CommonReplayErrorV2::Environment)?;
            if !court_changed && external_effect == ExternalReplayEffectV2::NoOp {
                return Err(CommonReplayErrorV2::NonChangingEvent);
            }
            env.validate_committed_effect(&pin, record, before, after, &outcome, external_effect, &external)
                .map_err(CommonReplayErrorV2::Environment)?;
            events.push(LocalReplayEventV2 { sequence: header.sequence, operation, court: outcome, external: external_effect });
            Ok(())
        })
        .map_err(map_visit::<S, E>)?;
    if execution_work != preflight.native_work || execution_hash != preflight.native_hash || events.len() as u64 != count {
        return Err(CommonReplayErrorV2::ChangedQuote);
    }
    let court = court.ok_or(CommonReplayErrorV2::InitialAdmission)?;
    let historical = snapshot(&court);
    let now = env.current_daa(&pin).map_err(CommonReplayErrorV2::Environment)?;
    if now < latest_daa {
        return Err(CommonReplayErrorV2::Clock);
    }
    let (clock_status, clock_attempt, clock_receipts) = court.preview_expiry_v2(now).map_err(CommonReplayErrorV2::Court)?;
    let mut current = historical;
    current.status = clock_status;
    current.last_attempt = clock_attempt;
    current.reserved_receipt_bytes = clock_receipts;
    let service = service.ok_or(CommonReplayErrorV2::InitialAdmission)?;
    env.validate_final(&pin, court.result(), &service, historical, current, now, &facts, &external)
        .map_err(CommonReplayErrorV2::Environment)?;
    if env.current_daa(&pin).map_err(CommonReplayErrorV2::Environment)? != now {
        return Err(CommonReplayErrorV2::Clock);
    }
    let quote = quote_replay::<S, E>(links, limits, &preflight, facts.capacity() as u64, events.capacity() as u64)?;
    let result = court.result().clone();
    let machine = RetainedCommonCourtV2 {
        court,
        prepared: prepared.ok_or(CommonReplayErrorV2::InitialAdmission)?,
        record_limits: record_limits.ok_or(CommonReplayErrorV2::InitialAdmission)?,
    };
    Ok((LocalReplayedCaseV2 { pin, result, service, historical, current, observed_daa: now, facts, events, external, quote }, machine))
}

fn semantic_error<S, E, V>(error: CourtErrorV2<AdapterError, V>) -> Result<LocalCourtOutcomeV2, CommonReplayErrorV2<S, E, V>> {
    // Carrier errors and pure admission/order/budget refusals are never accepted
    // journal events. Only actual checker/terminal or deadline state changes may
    // continue, and before/after plus native committed-effect checks still gate
    // them above. Err(Check) does not itself manufacture a terminal fact.
    let error = match error {
        CourtErrorV2::Check(error) => LocalCourtErrorV2::Check(error),
        CourtErrorV2::Contract(error) => LocalCourtErrorV2::Contract(error),
        CourtErrorV2::Closed => LocalCourtErrorV2::Closed,
        other => return Err(CommonReplayErrorV2::Court(other)),
    };
    Ok(LocalCourtOutcomeV2::SemanticError(error))
}
