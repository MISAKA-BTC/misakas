//! Concrete assigned Case native owner. Local A1 facts remain local until the
//! complete native archive/current prefix, original immutable admission inputs,
//! exact Case projections and the closed initial fraud topology are rebound.
//! No serialized progress, saved Pass bit, public environment or trait object
//! can construct the two capabilities in this child.
use super::*;
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, TirNativeRequestsV1, reserve_native_requests_v1},
    palw_positive_assignment_case_material_v2::assigned_case_native_carrier_matches_v2,
    palw_positive_assignment_material_v2::AssignmentAdmissionMaterialV2,
    palw_positive_common_replay_v2::LocalCheckedCourtFactV2,
    palw_positive_common_source_replay_v2::CommonSourceReplayStateV2,
    palw_positive_contract_v2::PreparedCheck,
    palw_positive_court_v2::coordinator::{
        carrier::{PanelFrameAuthorityV2, PanelSignatureVerifierV2},
        contract::{ContractCourtV2, TerminalKindV2},
        tir_checker::TirCourtCheckerV2,
    },
    palw_positive_funding_relay_v2::canonical_equal,
    palw_positive_journal_chain_v2::{CaseJournalPinV2, CaseJournalSourceV2},
    palw_positive_rows_v2::{PositiveRowBundleV2, PositiveRowsV2},
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        rows::{AssignmentRow, AssignmentStatusV2},
    },
    identity::{CaseId, Digest, PUBKEY_BYTES, PrincipalId, ResultId},
    journal::{CaseJournalLimits, CaseJournalRecordView},
    messages::{MinerResultCommit, PanelWitnessCommit},
    native::NativeRowKey,
    rows::{AcceptedMaterial, AcceptedOrigin, CaseRow, JournalHeadRow, NativeRowBody},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(super) mod source;

fn refuse(why: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("assigned Case: {why:?}"))
}
fn missing() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable)
}
fn required(state: &PalwChainStateV2, key: NativeRowKey) -> Result<NativeRowBody, PalwStateV2Error> {
    PositiveRowsV2::new(state).read(key).map_err(refuse)?.map(|row| row.body).ok_or_else(missing)
}
fn assignment(state: &PalwChainStateV2, id: AssignmentId) -> Result<AssignmentRow, PalwStateV2Error> {
    match required(state, NativeRowKey::Assignment(id))? {
        NativeRowBody::Assignment(row) => Ok(row),
        _ => Err(refuse("Assignment key/value")),
    }
}
fn rooted_bundle(
    state: &PalwChainStateV2,
    id: AssignmentId,
) -> Result<(AssignmentRow, PositiveRowBundleV2, CaseJournalPinV2), PalwStateV2Error> {
    let a = assignment(state, id)?;
    let AssignmentStatusV2::Consumed { case, result } = a.status else {
        return Err(refuse("Case requires consumed Assignment"));
    };
    let NativeRowBody::Result(result_row) = required(state, NativeRowKey::Result(result))? else {
        return Err(refuse("Result row"));
    };
    let bundle = PositiveRowsV2::new(state).bundle(result_row.result.claim_id()).map_err(refuse)?;
    let NativeRowBody::AssignmentUse(use_row) = required(state, NativeRowKey::AssignmentUse(id))? else {
        return Err(refuse("Use row"));
    };
    let NativeRowBody::JournalHead(head) = required(state, NativeRowKey::JournalHead(case))? else {
        return Err(refuse("Case journal"));
    };
    if bundle.case.case != case
        || bundle.result.result.id() != result
        || use_row.assignment != id
        || use_row.case != case
        || use_row.result != result
        || use_row.origin != bundle.result.origin
        || bundle.result.role_assignment != (AcceptedMaterial { origin: a.origin, digest: id.0 })
        || bundle.result.dependencies != a.dependencies
        || head.case != case
        || head.sequence == 0
        || head.history.origin != bundle.case.last_event
    {
        return Err(refuse("actual Assignment/Use/Result/Case/head links"));
    }
    let pin = CaseJournalPinV2 { scope: bundle.result.result.scope, case, result, head };
    Ok((a, bundle, pin))
}

impl PalwChainStateV2 {
    /// Resolve a follow-up against the committed selected parent's actual
    /// Assignment consumption, signed Result, Case and journal head.
    pub fn assigned_case_journal_pin_v2(
        &self,
        case: CaseId,
        expected_result: Option<ResultId>,
    ) -> Result<Option<(AssignmentId, CaseJournalPinV2)>, PalwStateV2Error> {
        use crate::palw_positive_rows_v2::PositiveRowErrorV2;
        let rows = PositiveRowsV2::new(self);
        let case_row = match rows.read(NativeRowKey::Case(case)) {
            Ok(Some(value)) => match value.body {
                NativeRowBody::Case(row) => row,
                _ => return Err(refuse("Case key/value")),
            },
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(why) => return Err(refuse(why)),
        };
        if expected_result.is_some_and(|result| result != case_row.result) {
            return Ok(None);
        }
        let NativeRowBody::Result(result) = required(self, NativeRowKey::Result(case_row.result))? else {
            return Err(refuse("Case Result key/value"));
        };
        let assignment = AssignmentId(result.role_assignment.digest);
        if rows.read(NativeRowKey::Assignment(assignment)).map_err(refuse)?.is_none() {
            return Ok(None);
        }
        let (_, _, pin) = rooted_bundle(self, assignment)?;
        if pin.case != case || pin.result != case_row.result {
            return Err(refuse("Case and original Assignment refer to different subjects"));
        }
        Ok(Some((assignment, pin)))
    }
}

/// No constructor outside this child. The current counter is cross-checked
/// after full replay, exact empty dependency preimage and all alias routes.
/// This is the INITIAL closed-ingress topology, never generic fraud clearance.
pub(super) struct ClosedAssignedFraudV2 {
    latest_decision_daa: u64,
    pending: bool,
}
impl ClosedAssignedFraudV2 {
    pub(super) fn has_pending(&self) -> bool {
        self.pending
    }
    pub(super) fn latest_decision_daa(&self) -> u64 {
        self.latest_decision_daa
    }
}

/// Actual retained nonserialized machine. Native writer owns this map and drops
/// it on checkpoint rollback/DB failure/reorg. `root` is refreshed only AFTER
/// successful native row/journal writes; mismatches require one cold full replay.
/// `ContractCourtV2` and `PreparedCheck` have no Deserialize path.
pub(super) struct NativeAssignedCaseCacheV2 {
    root: Hash64,
    context: PalwBlockContextV2,
    assignment: AssignmentRow,
    material: AssignmentAdmissionMaterialV2,
    pin: CaseJournalPinV2,
    checker: NativeAssignedCourtV2,
    source: CommonSourceReplayStateV2,
    positive_final: Option<AcceptedOrigin>,
    _lease: NativeDemandLeaseV1,
}

/// Journal slots, encoded bytes and replay work owed to accepted requests.
pub(super) struct AssignedSourceJournalReserveV2 {
    pub(super) frames: u64,
    pub(super) bytes: u64,
    pub(super) work: u64,
}

impl AssignedSourceJournalReserveV2 {
    pub(super) const EMPTY: Self = Self { frames: 0, bytes: 0, work: 0 };

    pub(super) fn check_journal_v2(
        &self,
        material: &AssignmentAdmissionMaterialV2,
        frames: u64,
        bytes: u64,
    ) -> Result<(), PalwStateV2Error> {
        if frames.checked_add(self.frames).is_none_or(|n| n > u64::from(material.assignment_policy.max_history_frames))
            || bytes.checked_add(self.bytes).is_none_or(|n| n > material.positive_policy.max_case_journal_bytes)
        {
            return Err(refuse("Case journal must retain capacity for its outstanding responses"));
        }
        Ok(())
    }
}

/// Arithmetic replay keeps the checked state and its native allocation owner
/// together. Source charges and Gate C votes have separate replay owners.
struct NativeAssignedCourtV2 {
    prepared: PreparedCheck,
    record_limits: crate::palw_positive_contract_v2::record::RecordLimits,
    authority: NativeAssignedPanelAuthorityV2,
    court: ContractCourtV2<TirCourtCheckerV2, PanelSignatureVerifierV2<NativeAssignedPanelAuthorityV2>>,
    facts: Vec<LocalCheckedCourtFactV2>,
    accepted_finish: Option<AcceptedMaterial>,
    check_limits: crate::palw_positive_check_v2::CheckLimits,
    source: crate::palw_positive_assignment_resources_v2::ReservedAssignmentSourceGeometryV2,
    source_requests:
        BTreeMap<misaka_palw_positive_contract::identity::RequestId, crate::palw_positive_source_v2::RetainedSourceRequestV2>,
    source_responses: BTreeMap<Digest, source::CheckedAssignedSourceResponseV2>,
    source_ledger: Option<crate::palw_positive_source_v2::NativeSourceLedgerV2>,
    settled_source_fee: u64,
    source_reserve: AssignedSourceJournalReserveV2,
    derived_heap: u64,
    checker_heap: u64,
    available_heap: u64,
    remaining_work: u64,
    relay_envelope: Option<(u64, u32)>,
    candidate_heap: u64,
    candidate_lease: Option<NativeDemandLeaseV1>,
    _lease: NativeDemandLeaseV1,
}
/// Private current-step role cursor. It contains an actually resolved current
/// role, never a caller pass flag or a reused admission-time author.
struct NativeAssignedPanelStepV2 {
    result: misaka_palw_positive_contract::identity::ResultId,
    author: PrincipalId,
    key: [u8; PUBKEY_BYTES],
    max_signed_bytes: u64,
    before_daa: u64,
    origin: AcceptedOrigin,
}
#[derive(Clone)]
struct NativeAssignedPanelAuthorityV2 {
    step: Rc<RefCell<Option<NativeAssignedPanelStepV2>>>,
}
impl PanelFrameAuthorityV2 for NativeAssignedPanelAuthorityV2 {
    type Error = PalwStateV2Error;
    fn authorize(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<u64, Self::Error> {
        let step = self.step.borrow();
        let step = step.as_ref().ok_or_else(missing)?;
        if step.result != result.id()
            || step.author != witness.author
            || step.origin.accepted_daa >= step.before_daa
            || misaka_palw_positive_contract::identity::principal(&step.key) != witness.author
        {
            return Err(refuse("actual historical/current prover role"));
        }
        Ok(step.max_signed_bytes)
    }
}

/// Borrowed native capability, only extracted from this child's actual machine
/// after full source/Court replay and closed-topology checks. A later native
/// mutation invalidates the complete root pin, including same-block ordering.
pub(super) struct NativeAssignedCaseSessionV2<'a> {
    root: Hash64,
    context: PalwBlockContextV2,
    assignment: AssignmentId,
    result: &'a MinerResultCommit,
    case: CaseRow,
    head: JournalHeadRow,
    policy: &'a PalwSecurityCertificationPolicyV1,
    terminal: Option<&'a LocalCheckedCourtFactV2>,
    material: AcceptedMaterial,
    positive_final: Option<AcceptedOrigin>,
    closed: ClosedAssignedFraudV2,
}
impl NativeAssignedCaseSessionV2<'_> {
    pub(super) fn ensure_current(
        &self,
        state: &PalwChainStateV2,
        ctx: &PalwBlockContextV2,
    ) -> Result<(AssignmentRow, PositiveRowBundleV2), PalwStateV2Error> {
        if state.state_root() != self.root || *ctx != self.context {
            return Err(refuse("stale native Case session"));
        }
        let (a, bundle, pin) = rooted_bundle(state, self.assignment)?;
        if bundle.result.result != *self.result
            || bundle.case != self.case
            || pin.head != self.head
            || a.id() != self.assignment
            || a.bundle.terms.branch_anchor.0 != self.result.scope.branch.0
        {
            return Err(refuse("native Case session lineage or before-row changed"));
        }
        reject_legacy_aliases(state, &a, self.result)?;
        Ok((a, bundle))
    }
    pub(super) fn result(&self) -> &MinerResultCommit {
        self.result
    }
    pub(super) fn assignment(&self) -> AssignmentId {
        self.assignment
    }
    pub(super) fn checked_terminal(&self) -> Option<&LocalCheckedCourtFactV2> {
        self.terminal
    }
    pub(super) fn positive_final_origin(&self) -> Option<AcceptedOrigin> {
        self.positive_final
    }
    pub(super) fn court_material(&self) -> AcceptedMaterial {
        self.material
    }
    pub(super) fn certification_policy(&self) -> &PalwSecurityCertificationPolicyV1 {
        self.policy
    }
    pub(super) fn pending_fraud_bound(&self) -> &ClosedAssignedFraudV2 {
        &self.closed
    }
}

/// Probe each existing staged namespace by its ordered subject prefix.
/// A large unrelated archive cannot enlarge this into a full key walk.
fn has_legacy_staged_alias_v2<V>(
    rows: &std::collections::BTreeMap<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, V>,
    subject: Hash64,
) -> bool {
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as K;
    let exact = [
        K::Claim(subject),
        K::Inventory(subject),
        K::TirRawChunk(subject),
        K::TirAcceptedBody(subject),
        K::TirClaim(subject),
        K::CourtDutyClock(subject),
        K::CourtSharedContext(subject),
        K::CourtSharedQuery(subject),
        K::CourtSharedIds(subject),
        K::CourtSharedStream(subject),
        K::CourtSharedRound(subject),
        K::CourtSharedBottom(subject),
        K::CourtEndArchive(subject),
    ];
    if exact.iter().any(|key| rows.contains_key(key)) {
        return true;
    }
    let low = Hash64::from_bytes([0; 64]);
    let high = Hash64::from_bytes([u8::MAX; 64]);
    let first_bond = PALW_BOND_KEY_V2_MIN;
    let last_bond = PalwBondKeyV2(TransactionOutpoint::new(high, u32::MAX));
    let first_side = PalwCourtSideV1::Challenger;
    let last_side = PalwCourtSideV1::Executor;
    [
        (K::Owner(subject, first_bond), K::Owner(subject, last_bond)),
        (K::InventoryOpen(subject, first_bond), K::InventoryOpen(subject, last_bond)),
        (K::TirAcquisition(subject, first_bond, low), K::TirAcquisition(subject, last_bond, high)),
        (K::TirPreparation(subject, low), K::TirPreparation(subject, high)),
        (K::TirWitness(subject, first_bond), K::TirWitness(subject, last_bond)),
        (K::CourtSharedChunk(subject, 0), K::CourtSharedChunk(subject, u64::MAX)),
        (K::CourtSharedResponse(subject, first_bond), K::CourtSharedResponse(subject, last_bond)),
        (K::CourtCloseReceipt(subject, first_side), K::CourtCloseReceipt(subject, last_side)),
        (K::CourtSharedRoot(subject, 0), K::CourtSharedRoot(subject, u64::MAX)),
        (K::CourtArchiveProgress(subject, first_side), K::CourtArchiveProgress(subject, last_side)),
        (K::CourtMaterialDraft(subject, low), K::CourtMaterialDraft(subject, high)),
        (K::CourtMaterial(subject, low), K::CourtMaterial(subject, high)),
        (K::CourtMaterialLedger(subject, first_bond), K::CourtMaterialLedger(subject, last_bond)),
    ]
    .into_iter()
    .any(|(first, last)| rows.range(first..=last).next().is_some())
}

impl TransitionBuilder<'_> {
    pub(super) fn reject_positive_job_alias_v2(&self, job: Hash64) -> Result<(), PalwStateV2Error> {
        if self
            .state
            .kernel_route
            .as_ref()
            .is_some_and(|route| route.contains_positive_job_v2(job) || route.contains_open_assignment_job_v2(job))
        {
            return Err(refuse("legacy ingress overlaps a rooted Positive job"));
        }
        Ok(())
    }

    /// The Open row protects this job against old ingress until Bind or close.
    pub(super) fn require_unowned_assignment_job_v2(&self, job: Hash64) -> Result<(), PalwStateV2Error> {
        self.reject_positive_job_alias_v2(job)?;
        let route = self.state.kernel_route_state_v1().ok_or_else(missing)?;
        if self.state.has_claim_for_job_v2(job)
            || has_legacy_staged_alias_v2(&self.state.staged_courts, job)
            || route
                .has_legacy_case_alias_v2(None, &job.as_bytes())
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, |_| missing()))?
        {
            return Err(refuse("Assignment job already belongs to a legacy claim or Court"));
        }
        Ok(())
    }

    /// A rooted Positive claim index owns this claim identity. Check at the
    /// legacy writer before it allocates or publishes a new row.
    pub(super) fn reject_positive_claim_alias_v2(&self, claim: Hash64) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::reserve_native_requests_v1;
        use misaka_palw_positive_contract::{CONTRACT_REVISION, CONTRACT_VERSION, identity::ClaimId, native::KERNEL_AUX_TABLE};

        let Some(route) = self.state.kernel_route.as_ref() else {
            return Ok(());
        };
        if route.aux.range((KERNEL_AUX_TABLE, Vec::new())..(KERNEL_AUX_TABLE + 1, Vec::new())).next().is_none() {
            return Ok(());
        }
        let key = (CONTRACT_VERSION, CONTRACT_REVISION, NativeRowKey::ClaimIndex(ClaimId(Digest(claim.as_bytes()))));
        let len = borsh::object_length(&key).map_err(refuse)?;
        let _lease = reserve_native_requests_v1(len as u64).map_err(PalwStateV2Error::NativeResource)?;
        let mut encoded = Vec::new();
        encoded.try_reserve_exact(len).map_err(|_| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<NativeRowKey>(
                "legacy claim alias key",
                "allocator refused the fixed key",
            ))
        })?;
        borsh::to_writer(&mut encoded, &key).map_err(refuse)?;
        if route.aux.contains_key(&(KERNEL_AUX_TABLE, encoded)) {
            return Err(refuse("legacy ingress overlaps a rooted Positive claim"));
        }
        Ok(())
    }
}

/// Exact native aliases must be absent in EVERY old entry surface. A zero
/// unresolved counter alone is deliberately not accepted as evidence of this.
pub(super) fn reject_legacy_aliases(
    state: &PalwChainStateV2,
    a: &AssignmentRow,
    result: &MinerResultCommit,
) -> Result<(), PalwStateV2Error> {
    let claim = Hash64::from_bytes(result.claim_id().0.0);
    let job = Hash64::from_bytes(result.job.0);
    if state.claims.contains_key(&claim)
        || state.has_claim_for_job_v2(job)
        || state.panels.contains_key(&claim)
        || state.court_sessions.contains_key(&claim)
        || state.legacy_disputes.contains_key(&claim)
        || state.panel_liabilities.contains_key(&claim)
        || state.staged_tir_by_claim.contains_key(&claim)
        || state.staged_court_service_by_claim.contains_key(&claim)
        || state.open_courts_by_claim.get(&claim).is_some_and(|n| *n != 0)
        || state.has_funded_escrow_for_claim_v1(claim)
    {
        return Err(refuse("legacy claim/TFO/service/funds alias is not closed initial topology"));
    }
    if has_legacy_staged_alias_v2(&state.staged_courts, claim) || has_legacy_staged_alias_v2(&state.staged_courts, job) {
        return Err(refuse("actual staged native alias"));
    }
    let route = state.kernel_route_state_v1().ok_or_else(missing)?;
    if route
        .has_legacy_case_alias_v2(Some(&claim.as_bytes()), &job.as_bytes())
        .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, |_| missing()))?
    {
        return Err(refuse("kernel job/claim/OPV alias"));
    }
    if a.bundle.terms.empty_dependencies_id() != a.dependencies.digest {
        return Err(refuse("initial assigned topology requires exact empty dependencies"));
    }
    Ok(())
}

impl NativeAssignedCaseCacheV2 {
    pub(super) fn session<'a>(
        &'a self,
        state: &PalwChainStateV2,
        ctx: &PalwBlockContextV2,
    ) -> Result<NativeAssignedCaseSessionV2<'a>, PalwStateV2Error> {
        if self.root != state.state_root() || self.context != *ctx {
            return Err(refuse("cache root/context stale; cold replay required"));
        }
        let (a, bundle, pin) = rooted_bundle(state, self.assignment.id())?;
        self.material.check_row(&a).map_err(refuse)?;
        if a != self.assignment
            || pin != self.pin
            || self.source.case_row().map_err(refuse)? != bundle.case
            || self.checker.court.result() != &bundle.result.result
            || bundle.case.unresolved_fraud != 0
        {
            return Err(refuse("actual replay before rows disagree"));
        }
        let terminal = self.checker.facts.last();
        let material = self.checker.accepted_finish.ok_or_else(missing)?;
        let fact = terminal.ok_or_else(missing)?;
        if fact.case() != pin.case
            || fact.result() != pin.result
            || fact.scope() != pin.scope
            || fact.terminal().descriptor() != bundle.case.coverage
            || bundle.case.positive_witness != (fact.terminal_kind() == TerminalKindV2::PositivePassed).then_some(fact.witness())
        {
            return Err(refuse("actual checked terminal/coverage links"));
        }
        let origin = self.positive_final.ok_or_else(missing)?;
        if origin != material.origin || origin.accepted_daa > ctx.daa_score {
            return Err(refuse("Finish material/origin"));
        }
        let latest = origin
            .accepted_daa
            .checked_add(self.material.certification_policy.prosecution_window_daa)
            .ok_or_else(|| refuse("finite actual Final prosecution"))?;
        // The native release path starts strictly after the last decision.
        // Preserve the entire original refund window after that point.
        let refund_before = latest
            .checked_add(1)
            .and_then(|daa| daa.max(bundle.case.maturity_daa).checked_add(self.material.assignment_policy.refund_window_daa))
            .ok_or_else(|| refuse("finite actual Final refund window"))?;
        if refund_before > a.bundle.terms.reserve_until_daa {
            return Err(refuse("actual Final exceeds original liability and refund reservation"));
        }
        let session = NativeAssignedCaseSessionV2 {
            root: self.root,
            context: *ctx,
            assignment: a.id(),
            result: self.checker.court.result(),
            case: bundle.case,
            head: pin.head,
            policy: &self.material.certification_policy,
            terminal,
            material,
            positive_final: Some(origin),
            closed: ClosedAssignedFraudV2 { latest_decision_daa: latest, pending: false },
        };
        session.ensure_current(state, ctx)?;
        Ok(session)
    }
}

fn snapshot(
    court: &ContractCourtV2<TirCourtCheckerV2, PanelSignatureVerifierV2<NativeAssignedPanelAuthorityV2>>,
) -> crate::palw_positive_common_replay_v2::LocalCourtSnapshotV2 {
    crate::palw_positive_common_replay_v2::LocalCourtSnapshotV2 {
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
struct NativeAssignedCourtAdvanceV2 {
    before: crate::palw_positive_common_replay_v2::LocalCourtSnapshotV2,
    after: crate::palw_positive_common_replay_v2::LocalCourtSnapshotV2,
    outcome: crate::palw_positive_common_replay_v2::LocalCourtOutcomeV2,
}

fn assigned_prover_rows_v2(
    aux: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    prefix: Vec<u8>,
) -> impl Iterator<Item = (&[u8], &[u8])> {
    use misaka_palw_positive_contract::native::KERNEL_AUX_TABLE;

    aux.range((KERNEL_AUX_TABLE, prefix.clone())..)
        .take_while(move |((table, key), _)| *table == KERNEL_AUX_TABLE && key.starts_with(&prefix))
        .map(|((_, key), bytes)| (key.as_slice(), bytes.as_slice()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AssignedProverUseV2 {
    Witness,
    Frame,
    SourceRequest,
}

/// Resolve the literal original or accepted funded op13 role in the ACTUAL
/// prefix. Historical replay calls with carriage.prefix_state(), never a later
/// state, and a retired actor cannot buy a replacement global Court budget.
fn find_prover_step(
    state: &PalwChainStateV2,
    a: &AssignmentRow,
    material: &AssignmentAdmissionMaterialV2,
    author: PrincipalId,
    origin: AcceptedOrigin,
    purpose: AssignedProverUseV2,
) -> Result<Option<NativeAssignedPanelStepV2>, PalwStateV2Error> {
    use misaka_palw_positive_contract::assignment::{AssignmentRoleV2, rows::AssignmentRoleAuthorizationV2};
    use misaka_palw_positive_contract::rows::NativeRowValue;

    let route = state.kernel_route_state_v1().ok_or_else(missing)?;
    let assignment = a.id();
    let mut resolved = None;
    let mut count = 0u32;
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<u8>(1024).map_err(PalwStateV2Error::NativeResource)?;
    requests
        .exact_v1::<u8>(misaka_palw_positive_contract::rows::MAX_NATIVE_KEY_BYTES as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _range_owner =
        reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<NativeRowKey>(
                "assigned Case prover role range",
                "actual checker is not armed",
            ))
        })?;
    let mut prefix = NativeRowKey::AssignmentRole { assignment, role: AssignmentRoleV2::Prover, bond: a.bundle.terms.prover }.encode();
    // The canonical last field is the outpoint (64-byte txid and u32 index).
    // Keep AssignmentId AND role in the prefix before entering the ordered map.
    prefix.truncate(prefix.len() - (64 + 4));
    for (key, bytes) in assigned_prover_rows_v2(&route.aux, prefix) {
        count = count.checked_add(1).ok_or_else(|| refuse("role count overflow"))?;
        if count > material.assignment_policy.max_role_candidates {
            return Err(refuse("bounded native role candidates"));
        }
        let value = NativeRowValue::decode(key, bytes).map_err(refuse)?;
        let NativeRowBody::AssignmentRole(role) = value.body else {
            return Err(refuse("role key/value"));
        };
        if role.assignment != assignment || role.role != AssignmentRoleV2::Prover {
            return Err(refuse("foreign native prover role key/value"));
        }
        if misaka_palw_positive_contract::identity::principal(&role.key) != author {
            continue;
        }
        role.authenticate(a).map_err(refuse)?;
        if role.origin.accepted_daa > origin.accepted_daa {
            return Err(refuse("role begins after the actual event prefix"));
        }
        if role.retired && purpose != AssignedProverUseV2::Frame {
            if purpose == AssignedProverUseV2::SourceRequest {
                continue;
            }
            return Err(refuse("retired role cannot open a candidate"));
        }
        let registered = state
            .bond(&PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes(role.bond.transaction.0), role.bond.index)))
            .ok_or_else(missing)?;
        if registered.pubkey.as_slice() != role.key.as_slice() {
            return Err(refuse("actual registered work key differs"));
        }
        let (before_daa, max_submitted_bytes) = match &role.authorization {
            AssignmentRoleAuthorizationV2::Initial => (a.prover_consent.build_before_daa, a.prover_consent.max_submitted_bytes),
            AssignmentRoleAuthorizationV2::Joined(join) => {
                let NativeRowBody::AssignmentProverFunding(funding) =
                    required(state, NativeRowKey::AssignmentProverFunding { assignment: a.id(), prover: role.bond })?
                else {
                    return Err(refuse("joined prover has no actual separate purpose account"));
                };
                funding.validate_shape().map_err(refuse)?;
                if funding.consent != *join
                    || funding.payer_key != role.key
                    || funding.payer_signature != role.signature
                    || funding.funding_origin != role.origin
                    || role.funding.is_none()
                {
                    return Err(refuse("joined role actual funding link"));
                }
                (join.service_before_daa, join.max_submitted_bytes)
            }
        };
        let max_signed_bytes = material
            .positive_policy
            .max_frame_bytes
            .checked_add(misaka_palw_positive_contract::identity::SIGNATURE_BYTES as u64)
            .filter(|n| *n <= max_submitted_bytes)
            .ok_or_else(|| refuse("original signed prover packet cap"))?;
        if origin.accepted_daa >= before_daa && purpose == AssignedProverUseV2::SourceRequest {
            continue;
        }
        if origin.accepted_daa >= before_daa || resolved.is_some() {
            return Err(refuse("prover opportunity/ambiguous registered author"));
        }
        resolved = Some(NativeAssignedPanelStepV2 {
            result: match a.status {
                AssignmentStatusV2::Consumed { result, .. } => result,
                _ => return Err(refuse("unconsumed role")),
            },
            author,
            key: role.key,
            max_signed_bytes,
            before_daa,
            origin,
        });
    }
    Ok(resolved)
}

impl NativeAssignedCaseCacheV2 {
    /// Advances the SAME real global Court, with A1 authentication/evaluation.
    /// Caller must own event work/heap, exact current root/head and native raw
    /// acceptance. Any failure after mutation drops this cache and restores the
    /// enclosing native checkpoint; there is no reusable prepared-row DTO.
    fn advance_court(
        &mut self,
        state: &PalwChainStateV2,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<NativeAssignedCourtAdvanceV2, PalwStateV2Error> {
        if state.state_root() != self.root
            || self.context != *ctx
            || action.origin().accepting_block.0 != ctx.block.as_bytes()
            || action.origin().accepted_daa != ctx.daa_score
            || action.origin().accepted_blue_score != ctx.blue_score
            || !canonical_equal(body, action.transaction().payload.get(3..).ok_or_else(missing)?)
        {
            return Err(refuse("actual native Case current prefix/raw operation"));
        }
        self.checker.advance(state, (&self.assignment, &self.material, self.pin), action.origin(), body)
    }
}

impl NativeAssignedCourtV2 {
    fn advance(
        &mut self,
        state: &PalwChainStateV2,
        (assignment, material, pin): (&AssignmentRow, &AssignmentAdmissionMaterialV2, CaseJournalPinV2),
        origin: AcceptedOrigin,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<NativeAssignedCourtAdvanceV2, PalwStateV2Error> {
        use crate::palw_positive_common_replay_v2::{LocalCourtErrorV2, LocalCourtOutcomeV2 as Outcome};
        use crate::palw_positive_court_v2::coordinator::{
            carrier::PanelPacketSignatureV2,
            contract::{CandidateOpenV2, CourtErrorV2},
            native_body::PositiveNativeOperationV2 as Op,
            record_ingress::preflight_witness_record_v2,
        };
        use misaka_palw_positive_contract::wire::EvidenceFrame;
        let before = snapshot(&self.court);
        let semantic = |error| -> Result<Outcome, PalwStateV2Error> {
            let error = match error {
                CourtErrorV2::Check(error) => LocalCourtErrorV2::Check(error),
                CourtErrorV2::Contract(error) => LocalCourtErrorV2::Contract(error),
                CourtErrorV2::Closed => LocalCourtErrorV2::Closed,
                other => return Err(refuse(other)),
            };
            Ok(Outcome::SemanticError(error))
        };
        let outcome = match &body.operation {
            Op::WitnessCommit { witness, panel_signature, .. } => {
                let step = find_prover_step(state, assignment, material, witness.author, origin, AssignedProverUseV2::Witness)?
                    .ok_or_else(missing)?;
                let key = step.key;
                *self.authority.step.borrow_mut() = Some(step);
                let limits = self.record_limits;
                let record = preflight_witness_record_v2(self.court.result(), body, limits.max_record_bytes).map_err(refuse)?;
                let context = self
                    .prepared
                    .authenticate_record(witness.clone(), record.record_bytes(), limits, &key, panel_signature)
                    .map_err(refuse)?;
                match self.court.open_candidate(witness.clone(), &key, panel_signature, context, origin.accepted_daa) {
                    Ok(CandidateOpenV2::Opened) => Outcome::CandidateOpened,
                    Ok(CandidateOpenV2::AlreadyActive) => {
                        return Err(refuse("fully idempotent candidate has no new journal event"));
                    }
                    Err(error) => semantic(error)?,
                }
            }
            Op::ExactFrame { frame, panel_signature } => {
                let frame = EvidenceFrame::decode(frame, material.positive_policy.max_frame_bytes).map_err(refuse)?;
                let header = frame.header();
                let active = self.court.last_attempt().ok_or_else(|| refuse("frame without active candidate"))?;
                if header.case != pin.case
                    || header.result != pin.result
                    || header.witness != active.witness
                    || header.relation_count != 1
                    || header.first_relation != self.court.cursor()
                {
                    return Err(refuse("foreign/future/repeated exact frame"));
                }
                let NativeRowBody::Witness(witness) = required(state, NativeRowKey::Witness(active.witness))? else {
                    return Err(refuse("actual witness row"));
                };
                witness.witness.authenticate(self.court.result(), &witness.panel_key, &witness.panel_signature).map_err(refuse)?;
                let step = find_prover_step(state, assignment, material, witness.witness.author, origin, AssignedProverUseV2::Frame)?
                    .ok_or_else(missing)?;
                if step.key != witness.panel_key {
                    return Err(refuse("exact frame work key substitution"));
                }
                let signature = PanelPacketSignatureV2 { key: step.key, signature: *panel_signature };
                *self.authority.step.borrow_mut() = Some(step);
                match self.court.submit_frame(frame, &signature, origin.accepted_daa) {
                    Ok(()) => Outcome::FrameChecked,
                    Err(error) => semantic(error)?,
                }
            }
            Op::Finish { case, result, witness } => {
                if *case != pin.case || *result != pin.result || self.court.last_attempt().map(|a| a.witness) != Some(*witness) {
                    return Err(refuse("Finish exact active candidate"));
                }
                match self.court.finish(origin.accepted_daa) {
                    Ok(fact) => {
                        if self.facts.len() as u64 >= u64::from(material.positive_policy.max_attempts)
                            || self.facts.len() == self.facts.capacity()
                        {
                            return Err(refuse("actual finite fact capacity"));
                        }
                        let kind = fact.terminal_kind();
                        self.facts.push(fact);
                        Outcome::Finished(kind)
                    }
                    Err(error) => semantic(error)?,
                }
            }
            Op::Expire { case, result } => {
                if *case != pin.case || *result != pin.result {
                    return Err(refuse("Expire Case identity"));
                }
                match self.court.expire_pending_v2(origin.accepted_daa) {
                    Ok(()) => Outcome::Expired,
                    Err(error) => semantic(error)?,
                }
            }
            Op::SourceRequest { request, .. } => {
                if request.case != pin.case {
                    return Err(refuse("source request Case"));
                }
                Outcome::External
            }
            Op::SourceResponse { case, .. } => {
                if *case != pin.case {
                    return Err(refuse("source response Case"));
                }
                Outcome::External
            }
            Op::ResultAdmission { .. } => {
                return Err(refuse("assigned admission must be genuine op9"));
            }
            Op::PositiveCourtAttestation { .. } => {
                return Err(refuse("P owner handles actual op7"));
            }
        };
        *self.authority.step.borrow_mut() = None;
        let after = snapshot(&self.court);
        if matches!(outcome, Outcome::SemanticError(_)) && before == after {
            return Err(refuse("uncharged checker refusal is not a native event"));
        }
        Ok(NativeAssignedCourtAdvanceV2 { before, after, outcome })
    }
}

/// Borrowed index over real committed archive bytes. This validates native
/// boundary/input provenance BEFORE the generic local journal checker. It never
/// treats absent/pruned data as an empty Case, and never clones global states.
struct NativeAssignedArchiveV2<'a> {
    backing: &'a NativeCommittedAssignedCaseV2,
    records: Vec<(Digest, usize)>,
    limits: CaseJournalLimits,
    _index_lease: NativeDemandLeaseV1,
}
impl<'a> NativeAssignedArchiveV2<'a> {
    fn new(
        current: &PalwChainStateV2,
        backing: &'a NativeCommittedAssignedCaseV2,
        limits: CaseJournalLimits,
    ) -> Result<Self, PalwStateV2Error> {
        limits.validate().map_err(refuse)?;
        backing.material().check_row(backing.assignment()).map_err(refuse)?;
        let count = backing.records().len() as u64;
        if count == 0
            || count != backing.pin().head.sequence
            || count > u64::from(backing.material().assignment_policy.max_history_frames)
        {
            return Err(refuse("missing/noncomplete native Case archive"));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<(Digest, usize)>(count).map_err(PalwStateV2Error::NativeResource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Self>(
                "assigned Case archive",
                "actual native replay index lease is absent",
            ))
        })?;
        let mut records = Vec::new();
        records.try_reserve_exact(backing.records().len()).map_err(|_| refuse("native Case index allocation"))?;
        let mut last = Digest([0; 64]);
        let mut total = 0u64;
        let mut previous_origin: Option<AcceptedOrigin> = None;
        for (index, archived) in backing.records().iter().enumerate() {
            total = total
                .checked_add(archived.raw().len() as u64)
                .filter(|n| *n <= backing.material().positive_policy.max_case_journal_bytes)
                .ok_or_else(|| refuse("actual full native Case archive bytes"))?;
            let record = CaseJournalRecordView::decode(archived.raw(), limits).map_err(refuse)?;
            let header = record.header();
            if header.sequence != index as u64 + 1
                || header.previous != last
                || header.scope != backing.pin().scope
                || header.case != backing.pin().case
                || header.result != backing.pin().result
                || header.origin != archived.carriage().origin()
            {
                return Err(refuse("actual native archive scope/link/origin"));
            }
            if let Some(prior) = previous_origin {
                let later = if prior.accepting_block == header.origin.accepting_block {
                    prior.accepted_daa == header.origin.accepted_daa
                        && prior.accepted_blue_score == header.origin.accepted_blue_score
                        && (prior.accepted_tx_ordinal, prior.event_ordinal)
                            < (header.origin.accepted_tx_ordinal, header.origin.event_ordinal)
                } else {
                    prior.accepted_blue_score < header.origin.accepted_blue_score && prior.accepted_daa <= header.origin.accepted_daa
                };
                if !later {
                    return Err(refuse("archive actual accepted-prefix ordering"));
                }
            }
            Self::check_carriage(current, backing, record, archived.carriage())?;
            let head = record.head();
            records.push((head, index));
            last = head;
            previous_origin = Some(header.origin);
            if index + 1 == backing.records().len()
                && (head != backing.pin().head.head
                    || record.history() != backing.pin().head.history
                    || header.previous != backing.pin().head.previous)
            {
                return Err(refuse("native archive latest pin"));
            }
        }
        records.sort_unstable_by_key(|(head, _)| *head);
        if records.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(refuse("native Case record identity collision/duplicate"));
        }
        Ok(Self { backing, records, limits, _index_lease: lease })
    }
    fn check_carriage(
        current: &PalwChainStateV2,
        backing: &NativeCommittedAssignedCaseV2,
        record: CaseJournalRecordView<'_>,
        carriage: &NativeAcceptedCaseCarriageV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::{
            palw_positive_assignment_case_material_v2::{AssignmentCaseBoundaryV2, rooted_initial_assigned_case_bind_v2},
            palw_positive_rows_v2::PositiveRowErrorV2,
        };
        let tx = carriage.transaction();
        let h = record.header();
        if !tx.id_is_current()
            || crate::hashing::tx::id(tx).as_bytes() != h.origin.carrier_transaction.0
            || h.origin.event_ordinal != 0
            || !assigned_case_native_carrier_matches_v2(tx, record)
        {
            return Err(refuse("literal native carrier/origin"));
        }
        let mut boundary = None;
        for aux in record.aux() {
            // A bounded fixed decode is safe before choosing the exact semantic
            // domain. Arbitrary equal-length data cannot match its committed id.
            if let Ok(value) = AssignmentCaseBoundaryV2::decode(aux.preimage())
                && value.material() == aux.material()
                && value.case == h.case
                && value.origin == h.origin
                && boundary.replace(value).is_some()
            {
                return Err(refuse("duplicate native Case boundary"));
            }
        }
        let boundary = boundary.ok_or_else(missing)?;
        let prefix = carriage.prefix_state().map_err(|_| missing())?;
        if boundary.pre_event_root.0 != prefix.state_root().as_bytes()
            || boundary.selected_parent.0 != carriage.selected_parent().as_bytes()
        {
            return Err(refuse("native actual executed-prefix boundary root/lineage"));
        }
        boundary.check_populated_data(tx, carriage.entries(), carriage.fee()).map_err(refuse)?;
        if h.sequence == 1 {
            let id = backing.assignment().id();
            rooted_initial_assigned_case_bind_v2(current, id, record, boundary).map_err(|why| match why {
                PositiveRowErrorV2::MissingRequiredRow | PositiveRowErrorV2::MissingKernelRoute => missing(),
                other => refuse(other),
            })?;
            // The current pre-event Assignment can be cancelled/consumed only
            // after this exact op9; a final saved Use is not historical authority.
            let a = assignment(prefix, backing.assignment().id())?;
            if a.status != AssignmentStatusV2::Open
                || a.bundle != backing.assignment().bundle
                || PositiveRowsV2::new(prefix).read(NativeRowKey::Case(h.case)).map_err(refuse)?.is_some()
                || PositiveRowsV2::new(prefix).read(NativeRowKey::AssignmentUse(a.id())).map_err(refuse)?.is_some()
            {
                return Err(refuse("original op9 pre-event Assignment/Case state"));
            }
        } else {
            let (_, bundle, pin) = rooted_bundle(prefix, backing.assignment().id())?;
            if pin.head.head != h.previous
                || pin.head.sequence.checked_add(1) != Some(h.sequence)
                || bundle.result.result.id() != h.result
            {
                return Err(refuse("actual historical event prefix differs from linked Case"));
            }
        }
        Ok(())
    }
    fn archived(&self, head: Digest) -> Result<&NativeCommittedCaseRecordV2, PalwStateV2Error> {
        let at = self.records.binary_search_by_key(&head, |(id, _)| *id).map_err(|_| missing())?;
        self.backing.records().get(self.records[at].1).ok_or_else(missing)
    }
}
impl CaseJournalSourceV2 for NativeAssignedArchiveV2<'_> {
    type Error = PalwStateV2Error;
    fn with_record<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> Result<T, Self::Error> {
        let record = self.archived(head)?;
        if record.raw().len() as u64 > max_bytes || max_bytes > self.limits.max_record_bytes {
            return Err(refuse("native Case source byte cap"));
        }
        Ok(visit(record.raw()))
    }
}

/// Rebind the original C relay against both its pending pre-op9 rows and
/// the current account. The pending pair comes from the committed F prefix.
fn check_recovered_case_relay_v2(
    state: &PalwChainStateV2,
    bind_prefix: &PalwChainStateV2,
    assignment: &AssignmentRow,
    bundle: &PositiveRowBundleV2,
    minimum_service_cutoff: u64,
    (maximum_heap, maximum_work): (u64, u64),
    schedule: &crate::palw_positive_assignment_resources_v2::AssignmentWorkScheduleV2,
) -> Result<u64, PalwStateV2Error> {
    use crate::palw_positive_funding_relay_v2::{FundingRelayBodyErrorV2, verify_funding_relay_package_signatures_v2};
    use misaka_palw_positive_contract::{
        CONTRACT_REVISION, CONTRACT_VERSION,
        assignment::{
            funding_relay::{FUNDING_RELAY_OPERATION_V2, FundingRelayRegistrationBodyV2, RelayFundingPurposeV2},
            funding_relay_rows::FundingRelayBindingV2,
        },
        rows::{MAX_NATIVE_KEY_BYTES, NativeRowValue},
    };
    let unavailable = || {
        PalwStateV2Error::NativeContextUnavailable(
            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
        )
    };
    let mut quote = TirNativeRequestsV1::new();
    quote.exact_v1::<NativeRowValue>(4).map_err(PalwStateV2Error::NativeResource)?;
    quote.exact_v1::<FundingRelayRegistrationBodyV2>(1).map_err(PalwStateV2Error::NativeResource)?;
    quote.exact_v1::<u8>((2 * MAX_NATIVE_KEY_BYTES) as u64).map_err(PalwStateV2Error::NativeResource)?;
    let scratch = quote.bytes_v1();
    let signature_heap = maximum_heap.checked_sub(scratch).ok_or_else(unavailable)?;
    let _lease = reserve_native_requests_v1(scratch).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(unavailable)?;
    let account = &bundle.funding;
    let funding = account.original_outpoint;
    let purpose = RelayFundingPurposeV2::CaseC;
    let authorization = NativeRowKey::FundingRelayAuthorization { funding, purpose };
    let custody = NativeRowKey::FundingRelayCustody { funding, purpose };
    let NativeRowBody::FundingRelayAuthorization(auth) = required(bind_prefix, authorization)? else {
        return Err(missing());
    };
    let NativeRowBody::FundingRelayCustody(pending) = required(bind_prefix, custody)? else {
        return Err(missing());
    };
    let NativeRowBody::FundingRelayAuthorization(current_auth) = required(state, authorization)? else {
        return Err(missing());
    };
    let NativeRowBody::FundingRelayCustody(mut current) = required(state, custody)? else {
        return Err(missing());
    };
    let origin = account.funding_origin;
    current.check_pair(&current_auth).map_err(refuse)?;
    if current.binding.take()
        != Some(FundingRelayBindingV2 {
            origin,
            original_funding: funding,
            predecessor: None,
            original_contract: account.consent.id().0,
            reservation: bundle.case.reservation,
        })
        || current != pending
        || current_auth != auth
        || auth.payer_key != account.payer_key
        || auth.origin.accepting_block == origin.accepting_block
        || auth.origin.accepted_daa > origin.accepted_daa
        || pending
            .retention_until_daa
            .checked_sub(auth.policy.rollback_margin_daa)
            .is_none_or(|cutoff| cutoff < minimum_service_cutoff)
    {
        return Err(refuse("recovered Case relay differs from the original binding or service horizon"));
    }
    crate::palw_positive_assignment_funds_v2::check_initial_case_relay_target_v2(
        &auth.terms,
        &assignment.bundle,
        bundle.result.result.id(),
        &account.consent,
        funding,
        account.initial_locked,
        origin.accepted_daa,
    )
    .map_err(refuse)?;
    let body = FundingRelayRegistrationBodyV2 {
        version: CONTRACT_VERSION,
        revision: CONTRACT_REVISION,
        operation: FUNDING_RELAY_OPERATION_V2,
        terms: auth.terms,
        purpose_signature: auth.purpose_signature,
        relay_signature: pending.relay_signature,
    };
    let work = schedule
        .court_body_work(scratch)
        .and_then(|decode| {
            schedule.court_operation_work(2, 4, 0, 0).and_then(|check| {
                decode.checked_add(check).ok_or(crate::palw_positive_assignment_resources_v2::AssignmentResourceErrorV2::Budget)
            })
        })
        .map_err(refuse)?;
    let remaining_work = maximum_work.checked_sub(work).ok_or_else(|| refuse("Case relay replay work cap"))?;
    verify_funding_relay_package_signatures_v2(&body, &auth.payer_key, signature_heap.min(auth.quote.usage.heap_bytes)).map_err(
        |why| match why {
            FundingRelayBodyErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
            FundingRelayBodyErrorV2::Contract(why) => refuse(why),
        },
    )?;
    Ok(remaining_work)
}

/// Authenticate the same initial checker that a cold Case replay will use.
/// The derivation uses the original registered program and job inputs before
/// op8, while payer identity comes from the actual state immediately before
/// op9. A stored coverage descriptor is only a comparison target.
fn prepare_initial_case_checker_v2(
    state: &PalwChainStateV2,
    backing: &NativeCommittedAssignedCaseV2,
    bundle: &PositiveRowBundleV2,
) -> Result<NativeAssignedCourtV2, PalwStateV2Error> {
    use crate::{
        palw_native_resource_v1::{NativeHeapV1, preflight_native_value_v1},
        palw_positive_assignment_material_v2::AssignmentSourceQuoteDescriptorV2,
        palw_positive_assignment_resources_v2::{AssignmentResourceErrorV2, ReservedAssignmentDerivedResourcesV2},
        palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeOperationV2 as Op, decode_assignment_body_v2, scan_assignment_body_v2,
        },
        palw_tir_class_v1::PalwTirLayoutV1,
    };

    let capacity = || {
        PalwStateV2Error::NativeContextUnavailable(
            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
        )
    };
    let reserve = |bytes| reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity);
    let material = backing.material();
    let row = backing.assignment();
    let policy = &material.assignment_policy;
    let positive = &material.positive_policy;
    let physical = &material.native_preimages;
    let max_heap = positive.max_case_heap_bytes.min(policy.max_heap_bytes).min(physical.physical.max_heap_bytes);
    let retained = backing.native_heap_v1("retained initial Case archive");
    let retained = retained.bytes().ok_or(PalwStateV2Error::NativeResource(retained))?;
    let mut heap = max_heap.checked_sub(retained).ok_or_else(capacity)?;
    let rules = NativeAssignmentRulesV2::from_original_material_data_v2(row, material).map_err(refuse)?;
    let limits = rules.body_limits().map_err(refuse)?;
    let open = backing.open_carriage();
    let first = backing.records().first().ok_or_else(missing)?.carriage();
    let native_body = |tx: &crate::tx::Transaction| {
        tx.payload.get(..3) == Some(&[1, 0, 176]) && tx.subnetwork_id == crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
    };
    if !native_body(open.transaction())
        || !native_body(first.transaction())
        || open.origin() != row.origin
        || first.origin() != bundle.result.origin
        || backing.pin().scope != bundle.result.result.scope
    {
        return Err(refuse("initial Case carrier origins and scope"));
    }
    let open_raw = &open.transaction().payload[3..];
    let bind_raw = &first.transaction().payload[3..];
    let open_shape = scan_assignment_body_v2(open_raw, limits).map_err(refuse)?;
    let bind_shape = scan_assignment_body_v2(bind_raw, limits).map_err(refuse)?;
    let decode_heap = open_shape
        .heap
        .checked_add(bind_shape.heap)
        .and_then(|n| n.checked_add((2 * size_of::<misaka_palw_positive_contract::rows::NativeRowValue>()) as u64))
        .ok_or_else(capacity)?;
    heap = heap.checked_sub(decode_heap).ok_or_else(capacity)?;
    let _decode = reserve(decode_heap)?;
    let open_body = decode_assignment_body_v2(open_raw, limits).map_err(refuse)?;
    let bind_body = decode_assignment_body_v2(bind_raw, limits).map_err(refuse)?;
    let Op::AcceptCanonicalAssignment { bundle: signed, source, prover, signatures, layout } = &open_body.operation else {
        return Err(refuse("original op8 body"));
    };
    let Op::AdmitAssignedResult { assignment: id, payload } = &bind_body.operation else {
        return Err(refuse("original op9 body"));
    };
    if signed != &row.bundle
        || source != &row.source_consent
        || prover != &row.prover_consent
        || *id != row.id()
        || payload.result != bundle.result.result
        || payload.service != bundle.case.service
        || payload.result.case_id() != backing.pin().case
        || payload.result.id() != backing.pin().result
    {
        return Err(refuse("initial Case and original Assignment identities"));
    }
    let bind_prefix = first.prefix_state().map_err(|_| missing())?;
    let prior = assignment(bind_prefix, *id)?;
    if prior.status != AssignmentStatusV2::Open
        || prior.bundle != row.bundle
        || prior.admission_material != row.admission_material
        || prior.worker_key != row.worker_key
    {
        return Err(refuse("original op9 did not consume the same open Assignment"));
    }
    let NativeRowBody::AssignmentFunding(funding) = required(bind_prefix, NativeRowKey::AssignmentFunding(*id))? else {
        return Err(refuse("original Case payer row"));
    };
    if funding.authorization != row.bundle.funding || funding.funding_origin != row.origin {
        return Err(refuse("original Case payer identity"));
    }
    payload
        .consent
        .authenticate(
            &payload.result,
            &payload.service,
            &payload.basis,
            first.origin().accepted_daa,
            &funding.payer_key,
            &payload.payer_signature,
        )
        .map_err(refuse)?;
    let qualified = crate::palw_positive_assignment_funds_v2::qualify_accepted_case_funding_output_v2(
        &prior,
        &funding,
        &payload.result,
        &payload.service,
        &payload.basis,
        &payload.consent,
        &funding.payer_key,
        first.transaction(),
        first.origin().accepted_daa,
    )
    .map_err(refuse)?;
    let rooted_funding = &bundle.funding;
    let [_, fee, capital, surplus] = qualified.initial_legs();
    if rooted_funding.funding_origin != first.origin()
        || rooted_funding.original_outpoint != qualified.outpoint()
        || rooted_funding.initial_locked != qualified.amount()
        || [rooted_funding.fee.initial, rooted_funding.capital.initial, rooted_funding.surplus.initial]
            != [fee.initial, capital.initial, surplus.initial]
        || rooted_funding.consent != payload.consent
        || rooted_funding.payer_key != funding.payer_key
        || rooted_funding.payer_signature != payload.payer_signature
    {
        return Err(refuse("rooted Case funds differ from the actual accepted C output"));
    }
    let layout_quote = preflight_native_value_v1::<PalwTirLayoutV1>(layout.as_slice(), layout.len() as u64).map_err(refuse)?;
    let layout_heap = layout_quote.logical_requests_v1();
    heap = heap.checked_sub(layout_heap).ok_or_else(capacity)?;
    let _layout = reserve(layout_heap)?;
    let layout: PalwTirLayoutV1 = borsh::from_slice(layout).map_err(refuse)?;
    let open_prefix = open.prefix_state().map_err(|_| missing())?;
    let registered = open_prefix.tir_class_v1(&Hash64::from_bytes(row.bundle.terms.registered_class.0)).ok_or_else(missing)?;
    let prompt_form = registered.facts.prompt_ids_form(material.network_prompt_form);
    if prompt_form != material.class_prompt_form {
        return Err(refuse("original Case prompt commitment form"));
    }
    let reserved = ReservedAssignmentDerivedResourcesV2::derive(
        open_prefix,
        signed.terms,
        &layout,
        policy,
        positive,
        &physical.source_policy,
        material.network_prompt_form,
        heap,
    )
    .map_err(|why| match why {
        AssignmentResourceErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
        AssignmentResourceErrorV2::MissingNativeLease | AssignmentResourceErrorV2::ConstructionCap { .. } => capacity(),
        other => refuse(other),
    })?;
    let resources = reserved.resources();
    let binding = resources.binding();
    material.job.check_native(binding.job()).map_err(refuse)?;
    binding.authenticate_bundle_roles(open_prefix, signed, signatures).map_err(refuse)?;
    binding.check_result(&payload.result).map_err(refuse)?;
    if material.source != AssignmentSourceQuoteDescriptorV2::from(resources.source_quote())
        || payload.basis.source_catalog != material.source.digest
        || payload.basis.policy != row.positive_policy
        || payload.basis.schedule != policy.case_fee_schedule
    {
        return Err(refuse("original Case source and policy"));
    }
    let (horizon, source_ledger) =
        super::positive_assignment::assignment_horizon_v2(&rules, signed, source, prover, resources, row.origin.accepted_daa)?;
    if bundle.case.reservation
        != crate::palw_positive_assignment_material_v2::assigned_case_reservation_v2(
            row,
            first.origin(),
            &bundle.case,
            &bundle.funding,
        )
    {
        return Err(refuse("recovered Case reservation differs from the actual Assignment, Result and original coin"));
    }
    let accepted_daa = first.origin().accepted_daa;
    let gate_c = rules.gate_c_clock(accepted_daa).map_err(refuse)?;
    horizon
        .check_case_source_floor_v2(
            &payload.service,
            &gate_c,
            policy.refund_window_daa,
            positive.max_attempts,
            resources.all_attempt_signed_frame_bytes_v2().map_err(refuse)?,
            source_ledger,
        )
        .map_err(refuse)?;
    horizon.check_case_maturity_v2(accepted_daa, bundle.case.maturity_daa, policy.refund_window_daa).map_err(refuse)?;
    let mut remaining_work = positive.max_case_work.min(policy.max_replay_work);
    if material.funding_relay.requires_relay() {
        remaining_work = check_recovered_case_relay_v2(
            state,
            bind_prefix,
            row,
            bundle,
            horizon.retention_until_daa,
            (heap, remaining_work),
            &physical.source_policy.work,
        )?;
    }
    if bundle.funding.basis != payload.basis
        || payload.basis.required_fee < horizon.case_money.fee
        || payload.basis.required_capital < horizon.case_money.capital
    {
        return Err(refuse("recovered Case funding or maturity differs from its original envelope"));
    }
    // The preparation retains another class, job, and plan while the original
    // derivation is still live. Reuse the typed program/StepSpace quote for
    // that second owner and reserve it before authenticating the Result.
    let derived_heap = reserved.allocation_quote().total_requests;
    let checker_heap = derived_heap.checked_add(size_of::<PreparedCheck>() as u64).ok_or_else(capacity)?;
    let fact_count = usize::try_from(positive.max_attempts.min(payload.service.budget.witness_attempts)).map_err(|_| capacity())?;
    let fact_heap = (fact_count as u64).checked_mul(size_of::<LocalCheckedCourtFactV2>() as u64).ok_or_else(capacity)?;
    let checker_heap = checker_heap
        .checked_add(fact_heap)
        .and_then(|n| n.checked_add(size_of::<NativeAssignedCourtV2>() as u64))
        .ok_or_else(capacity)?;
    if heap.checked_sub(derived_heap).is_none_or(|left| checker_heap > left) {
        return Err(capacity());
    }
    let lease = reserve(checker_heap)?;
    let prepared = PreparedCheck::authenticate(
        binding.admission(),
        resources.class(),
        Hash64::from_bytes(binding.admission().artifact.0),
        binding.job(),
        prompt_form,
        resources.check_limits(),
        payload.result.clone(),
        &row.worker_key,
        &payload.miner_signature,
    )
    .map_err(refuse)?;
    let coverage = prepared.descriptor();
    if coverage != bundle.case.coverage
        || coverage.mandatory_relations != payload.basis.mandatory_relations
        || coverage.profile != payload.basis.profile
    {
        return Err(refuse("independently prepared Case coverage differs from initial and rooted coverage"));
    }
    use crate::palw_positive_court_v2::coordinator::contract::CourtLimitsV2;
    let receipt_bytes = coverage.mandatory_relations.checked_mul(64).ok_or_else(capacity)?;
    let max_generated_heap_bytes = u64::from(payload.result.decode) * size_of::<u32>() as u64;
    let record_limits = crate::palw_positive_contract_v2::record::RecordLimits {
        max_record_bytes: positive
            .max_case_journal_bytes
            .min(crate::palw_positive_court_v2::coordinator::native_body::MAX_OPAQUE_BYTES_V2),
        max_generated_heap_bytes,
    };
    let authority = NativeAssignedPanelAuthorityV2 { step: Rc::new(RefCell::new(None)) };
    let court = ContractCourtV2::new(
        TirCourtCheckerV2,
        PanelSignatureVerifierV2::new(authority.clone()),
        binding.admission(),
        payload.result.clone(),
        &row.worker_key,
        &payload.miner_signature,
        payload.service,
        CourtLimitsV2 {
            max_attempts: fact_count as u32,
            max_total_wire_bytes: positive.max_all_attempt_bytes,
            max_part_bytes: positive.max_frame_bytes,
            max_receipt_bytes: receipt_bytes,
            max_reserved_elements: positive.max_all_attempt_elements,
            max_reserved_terms: positive.max_all_attempt_terms,
            deadline: horizon.court_before_daa,
        },
        first.origin().accepted_daa,
    )
    .map_err(refuse)?;
    let mut facts = Vec::new();
    facts.try_reserve_exact(fact_count).map_err(|_| capacity())?;
    if facts.capacity() > fact_count {
        return Err(capacity());
    }
    let check_limits = resources.check_limits();
    let source = reserved.into_source_geometry_v2(&payload.result).map_err(refuse)?;
    let checker_heap = checker_heap.checked_add(source.heap_bytes()).ok_or_else(capacity)?;
    Ok(NativeAssignedCourtV2 {
        prepared,
        record_limits,
        authority,
        court,
        facts,
        accepted_finish: None,
        check_limits,
        source,
        source_requests: BTreeMap::new(),
        source_responses: BTreeMap::new(),
        source_ledger: None,
        settled_source_fee: 0,
        source_reserve: AssignedSourceJournalReserveV2::EMPTY,
        derived_heap,
        checker_heap,
        available_heap: max_heap - retained,
        remaining_work,
        relay_envelope: match &material.funding_relay {
            crate::config::params::AssignmentFundingRelayProfileV2::RequiresRelayV2 { policy, .. } => {
                Some((horizon.retention_until_daa, policy.max_account_continuations))
            }
            crate::config::params::AssignmentFundingRelayProfileV2::CooperativeV1 => None,
        },
        candidate_heap: 0,
        candidate_lease: None,
        _lease: lease,
    })
}

fn check_replayed_court_projection_v2(
    state: &PalwChainStateV2,
    row: &CaseRow,
    machine: &NativeAssignedCourtV2,
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_positive_contract::{
        messages::{ClaimStatus, WitnessStatus},
        rows::CoverageState,
    };
    let checked = snapshot(&machine.court);
    let status_matches = checked.status == row.status
        || (checked.status == ClaimStatus::Pending && checked.attempts == 0 && row.status == ClaimStatus::ResultCommitted);
    let witness =
        if checked.status == ClaimStatus::PositivePassed { Some(checked.last_attempt.ok_or_else(missing)?.witness) } else { None };
    if !status_matches
        || checked.attempts != row.usage.witness_attempts
        || checked.last_attempt.map_or(WitnessStatus::Building, |attempt| attempt.status) != row.witness_status
        || witness != row.positive_witness
    {
        return Err(refuse("replayed arithmetic outcome differs from rooted Case"));
    }
    if checked.status == ClaimStatus::PositivePassed {
        let fact = machine.facts.last().ok_or_else(missing)?;
        if fact.terminal_kind() != TerminalKindV2::PositivePassed
            || fact.case() != row.case
            || fact.result() != row.result
            || Some(fact.witness()) != witness
            || fact.terminal().descriptor() != row.coverage
        {
            return Err(refuse("replayed Finish differs from rooted terminal coverage"));
        }
    }
    if let Some(active) = checked.last_attempt.filter(|attempt| attempt.status == WitnessStatus::Checking) {
        let NativeRowBody::Coverage(coverage) = required(state, NativeRowKey::Coverage(active.witness))? else {
            return Err(refuse("archived active Coverage row"));
        };
        if coverage.witness != active.witness
            || coverage.case != row.case
            || coverage.result != row.result
            || coverage.plan != row.coverage.plan
            || coverage.mandatory_relations != row.coverage.mandatory_relations
            || coverage.next_relation != checked.cursor
            || coverage.state != CoverageState::Checking
        {
            return Err(refuse("replayed active cursor differs from rooted Coverage"));
        }
    }
    Ok(())
}

impl NativeAssignedCourtV2 {
    /// Authenticate the source request against its exact Case and pre-event role.
    fn check_source_request_v2(
        &mut self,
        backing: &NativeCommittedAssignedCaseV2,
        state: &PalwChainStateV2,
        assignment: &AssignmentRow,
        prior: &CaseRow,
        origin: AcceptedOrigin,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<Option<[u8; PUBKEY_BYTES]>, PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2;
        use misaka_palw_positive_contract::assignment::{
            control_material::AssignmentControlViewV2,
            journal::{AssignmentJournalEventKindV2, AssignmentJournalRecordViewV2},
            open_material::{AssignmentOpenLimitsV2, AssignmentOpenPreimageKindV2, AssignmentOpenViewV2},
        };

        let PositiveNativeOperationV2::SourceRequest { request, requester, signature } = &body.operation else {
            return Ok(None);
        };
        let result = self.court.result();
        let material = backing.material();
        if prior.case != result.case_id()
            || prior.result != result.id()
            || prior.service.id() != result.service_contract
            || origin.accepted_daa >= prior.service.deadline.min(assignment.source_consent.service_before_daa)
        {
            return Err(refuse("source request differs from the accepted Case duty"));
        }
        let geometry = &self.source.geometry;
        geometry.validate_request(request).map_err(refuse)?;
        let units = geometry.source_units(request.kind);

        let schedule = &material.native_preimages.source_policy.work;
        let role_work =
            schedule.court_operation_work(1, u64::from(material.assignment_policy.max_role_candidates), 0, 0).map_err(refuse)?;
        self.remaining_work = self.remaining_work.checked_sub(role_work).ok_or_else(|| refuse("source role lookup work"))?;
        let key = if let Some(step) =
            find_prover_step(state, assignment, material, *requester, origin, AssignedProverUseV2::SourceRequest)?
        {
            step.key
        } else {
            // The archive already authenticated this Open record against the
            // selected parent and actual op8. Bound the second read and the
            // verifier lookups before decoding the retained roster.
            let raw = backing.open_raw();
            let quote = schedule.court_body_work(raw.len() as u64).map_err(refuse)?;
            let quote = quote.checked_mul(2).ok_or_else(|| refuse("source roster wire work overflow"))?;
            self.remaining_work = self.remaining_work.checked_sub(quote).ok_or_else(|| refuse("source roster wire work"))?;
            let open =
                AssignmentJournalRecordViewV2::decode(raw, material.native_assignment_journal_storage_limit_v2()).map_err(refuse)?;
            let header = open.header();
            if header.kind != AssignmentJournalEventKindV2::Open
                || header.assignment != assignment.id()
                || header.origin != assignment.origin
            {
                return Err(refuse("source roster must belong to the original Open"));
            }
            let cap = material.assignment_policy.max_frame_bytes;
            let envelope = AssignmentControlViewV2::decode(open.payload(), cap).map_err(refuse)?;
            let open = AssignmentOpenViewV2::decode(envelope.inner(), AssignmentOpenLimitsV2::within_payload(cap)).map_err(refuse)?;
            let roster = open.preimage(AssignmentOpenPreimageKindV2::ReservationCustody);
            let expected = material.custody.preimage_bytes[AssignmentOpenPreimageKindV2::ReservationCustody as usize];
            if crate::palw_positive_assignment_material_v2::assignment_custody_bytes_id_v2(roster) != expected {
                return Err(refuse("source requester roster differs from original custody"));
            }
            let roster = super::positive_assignment_verifiers::AssignmentVerifierCandidatesV2::decode_with_work_bound_v2(
                roster,
                schedule.per_control,
                &mut self.remaining_work,
            )?;
            super::positive_assignment_certification::resolve_drawn_source_key_v2(
                state,
                assignment,
                result,
                material,
                origin,
                *requester,
                roster,
                &mut self.remaining_work,
            )?
        };
        if misaka_palw_positive_contract::identity::principal(&key) != *requester {
            return Err(refuse("actual source requester key"));
        }
        request.authenticate(units, &key, signature).map_err(refuse)?;
        Ok(Some(key))
    }

    /// Admit this exact operation before decoding or applying it. A candidate's
    /// allocation lease travels with the machine across cold and current steps.
    fn reserve_operation_v2(
        &mut self,
        body_bytes: &[u8],
        material: &AssignmentAdmissionMaterialV2,
    ) -> Result<NativeDemandLeaseV1, PalwStateV2Error> {
        use crate::{
            palw_positive_assignment_resources_v2::ASSIGNED_COURT_ROLE_SIGNATURES_V2,
            palw_positive_court_v2::coordinator::native_body::{PositiveNativeBodyV2, scan_native_body_v2},
        };
        let capacity = || {
            PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
            )
        };
        let policy = &material.positive_policy;
        let bytes = body_bytes.len() as u64;
        let schedule = &material.native_preimages.source_policy.work;
        let codec_work = schedule.court_body_work(bytes).map_err(refuse)?;
        self.remaining_work = self.remaining_work.checked_sub(codec_work).ok_or_else(|| refuse("Case replay codec work"))?;
        let shape = scan_native_body_v2(body_bytes, case_replay_body_limits_v2(policy)).map_err(refuse)?;
        let is_witness = shape.kind == 1;
        let is_frame = shape.kind == 2;
        let check = self.check_limits;
        let authentication_work = schedule
            .court_operation_work(
                shape.signatures + u64::from(is_witness || is_frame) * ASSIGNED_COURT_ROLE_SIGNATURES_V2,
                if is_witness || is_frame { u64::from(material.assignment_policy.max_role_candidates) } else { 0 },
                if is_frame { check.max_elements_per_leaf } else { 0 },
                if is_frame { check.max_terms_per_leaf } else { 0 },
            )
            .map_err(refuse)?;
        self.remaining_work =
            self.remaining_work.checked_sub(authentication_work).ok_or_else(|| refuse("Case replay operation work"))?;
        if is_witness && self.candidate_lease.is_none() {
            // A new candidate can coexist with the old context and progress.
            // Quote both pairs plus the temporary authentication copy.
            self.candidate_heap = self
                .derived_heap
                .checked_mul(5)
                .and_then(|n| self.record_limits.max_generated_heap_bytes.checked_mul(5).and_then(|g| n.checked_add(g)))
                .and_then(|n| self.prepared.descriptor().mandatory_relations.checked_mul(128).and_then(|r| n.checked_add(r)))
                .ok_or_else(capacity)?;
            if self.available_heap.checked_sub(self.checker_heap).is_none_or(|left| self.candidate_heap > left) {
                return Err(capacity());
            }
            self.candidate_lease =
                Some(reserve_native_requests_v1(self.candidate_heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?);
        }
        let body_heap = shape
            .opaque_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<PositiveNativeBodyV2>() as u64))
            .and_then(|n| n.checked_add((4 * size_of::<misaka_palw_positive_contract::rows::NativeRowValue>()) as u64))
            .and_then(|n| n.checked_add(if is_frame { policy.max_part_heap } else { 0 }))
            .ok_or_else(capacity)?;
        let role_heap =
            if is_witness || is_frame { (1024 + misaka_palw_positive_contract::rows::MAX_NATIVE_KEY_BYTES) as u64 } else { 0 };
        if self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|left| left.checked_sub(self.candidate_heap))
            .and_then(|left| left.checked_sub(role_heap))
            .is_none_or(|left| body_heap > left)
        {
            return Err(capacity());
        }
        reserve_native_requests_v1(body_heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)
    }
}

fn case_replay_body_limits_v2(
    policy: &crate::config::params::PositiveCourtPolicyV2,
) -> crate::palw_positive_court_v2::coordinator::native_body::NativeBodyLimitsV2 {
    use crate::palw_positive_court_v2::coordinator::native_body::{
        MAX_BODY_BYTES_V2, MAX_EXACT_RELATIONS_V2, MAX_OPAQUE_BYTES_V2, MAX_SIGNATURES_V2, NativeBodyLimitsV2,
    };
    NativeBodyLimitsV2 {
        max_body_bytes: policy.max_case_journal_bytes.min(MAX_BODY_BYTES_V2),
        max_opaque_bytes: policy.max_case_journal_bytes.min(MAX_OPAQUE_BYTES_V2),
        max_exact_relations: MAX_EXACT_RELATIONS_V2,
        max_signatures: MAX_SIGNATURES_V2,
    }
}

/// Replay the arithmetic projection in event order against actual native
/// pre-event role state. Source and certification reconcile separately.
fn replay_assigned_court_history_v2(
    state: &PalwChainStateV2,
    backing: &NativeCommittedAssignedCaseV2,
    rooted: &PositiveRowBundleV2,
) -> Result<NativeAssignedCourtV2, PalwStateV2Error> {
    use crate::palw_positive_court_v2::coordinator::native_body::{PositiveNativeOperationV2 as Op, decode_native_body_v2};
    let mut machine = prepare_initial_case_checker_v2(state, backing, rooted)?;
    let material = backing.material();
    let pin = *backing.pin();
    let limits = case_replay_body_limits_v2(&material.positive_policy);
    let mut retained = backing.records().first().ok_or_else(missing)?.raw().len() as u64;
    for (index, record) in backing.records().iter().enumerate().skip(1) {
        retained = retained.checked_add(record.raw().len() as u64).ok_or_else(missing)?;
        let frames = index as u64 + 1;
        let carriage = record.carriage();
        let prefix = carriage.prefix_state().map_err(|_| missing())?;
        let a = assignment(prefix, backing.assignment().id())?;
        if a.bundle != backing.assignment().bundle
            || a.admission_material != backing.assignment().admission_material
            || a.status != (AssignmentStatusV2::Consumed { case: pin.case, result: pin.result })
        {
            return Err(refuse("archived Court Assignment prefix differs"));
        }
        let NativeRowBody::Case(prior) = required(prefix, NativeRowKey::Case(pin.case))? else {
            return Err(refuse("archived pre-event Case row"));
        };
        check_replayed_court_projection_v2(prefix, &prior, &machine)?;
        if carriage.transaction().payload.get(7) == Some(&12) {
            let after = match backing.records().get(index + 1) {
                Some(next) => next.carriage().prefix_state().map_err(|_| missing())?,
                None => state,
            };
            machine.replay_case_source_payment_v2(backing, prefix, after, &prior, carriage)?;
            machine.source_reserve.check_journal_v2(material, frames, retained)?;
            continue;
        }
        let body_bytes = carriage.transaction().payload.get(3..).ok_or_else(missing)?;
        let work_before = machine.remaining_work;
        let _body = machine.reserve_operation_v2(body_bytes, material)?;
        let body = decode_native_body_v2(body_bytes, limits).map_err(refuse)?;
        let Some((case, result)) = body.operation.case_target_v2().map_err(refuse)? else {
            return Err(refuse("legacy admission in assigned Case history"));
        };
        if case != pin.case || result.is_some_and(|id| id != pin.result) {
            return Err(refuse("archived Court operation subject differs"));
        }
        let key = machine.check_source_request_v2(backing, prefix, &a, &prior, carriage.origin(), &body)?;
        let response = if let Op::SourceResponse { .. } = &body.operation {
            Some(
                machine
                    .check_source_response_v2(prefix, &a, material, &prior, carriage.origin(), &body, _body.memory_v1())?
                    .ok_or_else(|| refuse("duplicate source delivery changed the rooted journal"))?,
            )
        } else {
            None
        };
        if let Some(update) = machine.prepare_assigned_source_update_v2(
            prefix,
            &a,
            material,
            &prior,
            carriage.origin(),
            &body,
            body_bytes,
            _body.memory_v1(),
            (key, response.as_ref()),
            work_before,
        )? {
            let after = match backing.records().get(index + 1) {
                Some(next) => next.carriage().prefix_state().map_err(|_| missing())?,
                None => state,
            };
            machine.apply_replayed_source_update_v2(after, update)?;
        }
        machine.remember_source_request_v2(material, carriage.origin(), &body, _body.memory_v1())?;
        if let Some(response) = response {
            machine.remember_source_response_v2(material, response, _body.memory_v1())?;
        }
        if !matches!(&body.operation, Op::PositiveCourtAttestation { .. }) {
            let event = machine.advance(prefix, (&a, material, pin), carriage.origin(), &body)?;
            use crate::palw_positive_common_replay_v2::LocalCourtOutcomeV2;
            use misaka_palw_positive_contract::{messages::WitnessStatus, rows::CoverageState};
            let terminal_coverage = match event.outcome {
                LocalCourtOutcomeV2::Expired => Some(CoverageState::EndedUnverified),
                LocalCourtOutcomeV2::Finished(TerminalKindV2::PositivePassed) => Some(CoverageState::Matched),
                _ => None,
            };
            if terminal_coverage == Some(CoverageState::Matched) {
                use misaka_palw_positive_contract::journal::CaseJournalEventKind;
                let policy = &material.positive_policy;
                let finish = CaseJournalRecordView::decode(
                    record.raw(),
                    CaseJournalLimits {
                        max_record_bytes: policy.max_case_journal_bytes,
                        max_payload_bytes: policy.max_case_journal_bytes,
                        max_aux_entries: 1,
                        max_aux_preimage_bytes: crate::palw_positive_assignment_case_material_v2::ASSIGNED_CASE_BOUNDARY_BYTES_V2
                            as u64,
                        max_total_preimage_bytes: policy.max_case_journal_bytes,
                        max_logical_heap_bytes: policy.max_case_heap_bytes,
                    },
                )
                .map_err(refuse)?;
                let after = match backing.records().get(index + 1) {
                    Some(next) => next.carriage().prefix_state().map_err(|_| missing())?,
                    None => state,
                };
                let NativeRowBody::JournalHead(head) = required(after, NativeRowKey::JournalHead(pin.case))? else {
                    return Err(refuse("archived Finish lacks the accepted journal head"));
                };
                NativeAssignedArchiveV2::check_carriage(state, backing, finish, carriage)?;
                let history = finish.history();
                if finish.header().kind != CaseJournalEventKind::NativeBody
                    || finish.payload() != body_bytes
                    || history.origin != carriage.origin()
                    || head.sequence != frames
                    || head.head != finish.head()
                    || head.history != history
                {
                    return Err(refuse("archived Finish differs from accepted journal material"));
                }
                machine.accepted_finish = Some(history);
            }
            if let Some(expected_state) = terminal_coverage
                && let Some(attempt) = event.before.last_attempt.filter(|attempt| attempt.status == WitnessStatus::Checking)
            {
                let after = match backing.records().get(index + 1) {
                    Some(next) => next.carriage().prefix_state().map_err(|_| missing())?,
                    None => state,
                };
                let key = NativeRowKey::Coverage(attempt.witness);
                let NativeRowBody::Coverage(mut expected) = required(prefix, key)? else {
                    return Err(refuse("archived terminal lacks pre-event Coverage"));
                };
                expected.state = expected_state;
                expected.last_event = carriage.origin();
                if required(after, key)? != NativeRowBody::Coverage(expected) {
                    return Err(refuse("archived terminal did not update the active Coverage"));
                }
            }
        }
        machine.source_reserve.check_journal_v2(material, frames, retained)?;
        if machine.source_reserve.work > machine.remaining_work {
            return Err(refuse("Case replay must retain work for its outstanding responses"));
        }
    }
    check_replayed_court_projection_v2(state, &rooted.case, &machine)?;
    Ok(machine)
}

impl TransitionBuilder<'_> {
    fn prepare_case_predecessor_v2<'a>(
        &self,
        action: &'a PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(&'a NativeCommittedAssignedCaseV2, NativeAssignedCourtV2, CaseRow), PalwStateV2Error> {
        let Some((case, expected_result)) = body.operation.case_target_v2().map_err(refuse)? else {
            return Err(refuse("a Case follow-up requires an explicit subject"));
        };
        self.prepare_case_subject_v2(action, case, expected_result)
    }

    fn prepare_case_subject_v2<'a>(
        &self,
        action: &'a PositiveAssignmentRawActionV2,
        case: CaseId,
        expected_result: Option<ResultId>,
    ) -> Result<(&'a NativeCommittedAssignedCaseV2, NativeAssignedCourtV2, CaseRow), PalwStateV2Error> {
        let backing = action.case_backing().map_err(|_| missing())?;
        let NativeAssignedCaseBackingV2::Committed(backing) = backing else {
            return Err(missing());
        };
        let assignment = backing.assignment().id();
        let pin = *backing.pin();
        let record = backing.records().last().ok_or_else(missing)?;
        if backing.records().len() as u64 != pin.head.sequence || record.carriage().origin() != pin.head.history.origin {
            return Err(refuse("Case archive differs from its rooted journal extent or latest origin"));
        }
        if backing.parent() != action.selected_parent() || case != pin.case || expected_result.is_some_and(|id| id != pin.result) {
            return Err(refuse("Case follow-up differs from its selected-parent archive subject"));
        }
        let (_, bundle, current) = rooted_bundle(&self.state, assignment)?;
        if current != pin {
            // A preceding operation in this block can advance the head. Its
            // unpublished material must be replayed before this operation.
            return Err(missing());
        }
        let machine = replay_assigned_court_history_v2(&self.state, backing, &bundle)?;
        Ok((backing, machine, bundle.case))
    }
}

// The concrete archive/Remaining/service factory is still required. These raw
// routes explicitly preserve unavailability instead of constructing a session
// from rooted summaries or accepting a local cached positive verdict.
impl TransitionBuilder<'_> {
    pub(super) fn apply_positive_assignment_case_raw_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        let (backing, mut machine, prior) = self.prepare_case_predecessor_v2(action, body)?;
        let origin = action.origin();
        if origin.accepting_block.0 != ctx.block.as_bytes()
            || origin.accepted_daa != ctx.daa_score
            || origin.accepted_blue_score != ctx.blue_score
        {
            return Err(refuse("current Court operation clock and origin"));
        }
        let material = backing.material();
        let raw = action.transaction().payload.get(3..).ok_or_else(missing)?;
        let work_before = machine.remaining_work;
        let _body = machine.reserve_operation_v2(raw, material)?;
        if !canonical_equal(body, raw) {
            return Err(refuse("current Court operation differs from literal carrier"));
        }
        let current_assignment = assignment(&self.state, backing.assignment().id())?;
        let key = machine.check_source_request_v2(backing, &self.state, &current_assignment, &prior, origin, body)?;
        let response = if matches!(
            &body.operation,
            crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2::SourceResponse { .. }
        ) {
            machine.check_source_response_v2(&self.state, &current_assignment, material, &prior, origin, body, _body.memory_v1())?
        } else {
            None
        };
        let source = machine.prepare_assigned_source_update_v2(
            &self.state,
            &current_assignment,
            material,
            &prior,
            origin,
            body,
            raw,
            _body.memory_v1(),
            (key, response.as_ref()),
            work_before,
        )?;
        let replayed = machine.advance(&self.state, (&current_assignment, material, *backing.pin()), origin, body)?;
        use crate::palw_positive_common_replay_v2::LocalCourtOutcomeV2;
        let expired = matches!(replayed.outcome, LocalCourtOutcomeV2::Expired);
        let passed = matches!(replayed.outcome, LocalCourtOutcomeV2::Finished(TerminalKindV2::PositivePassed));
        let journal = (expired || passed || source.as_ref().is_some_and(|update| update.changes_case()))
            .then(|| {
                self.prepare_assigned_case_journal_v2(
                    backing,
                    misaka_palw_positive_contract::journal::CaseJournalEventKind::NativeBody,
                    &machine.source_reserve,
                )
            })
            .transpose()?;
        let delta = crate::palw_positive_common_source_replay_v2::CourtUsageDeltaV2::between(Some(replayed.before), replayed.after)
            .map_err(refuse)?;
        if prior
            .usage
            .transmitted_bytes
            .checked_add(delta.signed_wire_bytes)
            .is_none_or(|n| n > prior.service.budget.transmitted_bytes)
            || prior.usage.witness_attempts.checked_add(delta.attempts).is_none_or(|n| n > prior.service.budget.witness_attempts)
        {
            return Err(refuse("current Court operation exceeds signed service usage"));
        }
        if expired || passed {
            use misaka_palw_positive_contract::{messages::WitnessStatus, rows::CoverageState};
            let (positive_witness, coverage_descriptor, coverage_state) = if passed {
                let fact = machine.facts.last().ok_or_else(missing)?;
                if fact.case() != prior.case || fact.result() != prior.result || fact.scope() != prior.coverage.scope {
                    return Err(refuse("checked Finish differs from the rooted Case"));
                }
                (Some(fact.witness()), fact.terminal().descriptor(), CoverageState::Matched)
            } else {
                (None, prior.coverage, CoverageState::EndedUnverified)
            };
            let case = CaseRow {
                status: replayed.after.status,
                positive_witness,
                coverage: coverage_descriptor,
                witness_status: replayed.after.last_attempt.map_or(WitnessStatus::Building, |attempt| attempt.status),
                last_event: origin,
                ..prior
            };
            let journal = journal.ok_or_else(missing)?;
            let checkpoint = self.try_checkpoint()?;
            let result = (|| {
                if let Some(attempt) = replayed.before.last_attempt.filter(|attempt| attempt.status == WitnessStatus::Checking) {
                    let NativeRowBody::Coverage(mut coverage) = required(&self.state, NativeRowKey::Coverage(attempt.witness))? else {
                        return Err(refuse("Court terminal requires the active Coverage row"));
                    };
                    coverage.state = coverage_state;
                    coverage.last_event = origin;
                    self.write_assignment_aux_v2(NativeRowBody::Coverage(coverage))?;
                }
                self.write_assignment_aux_v2(NativeRowBody::Case(case))?;
                self.write_prepared_assigned_case_journal_v2(journal)
            })();
            if result.is_err() {
                self.restore(checkpoint);
            }
            return result;
        }
        if let Some(update) = source {
            if !update.changes_case() {
                return Ok(());
            }
            let receipt = update.receipt().copied();
            if receipt.is_some() != response.is_some()
                || update.fee_intent().is_some() != response.is_some()
                || receipt.is_some_and(|row| row.settlement.is_some())
            {
                return Err(refuse("source event receipt does not match a checked first response"));
            }
            let case = *update.case_row().ok_or_else(missing)?;
            let union = *update.union().ok_or_else(missing)?;
            let journal = journal.ok_or_else(missing)?;
            let checkpoint = self.try_checkpoint()?;
            let result = (|| {
                self.write_assignment_aux_v2(NativeRowBody::Case(case))?;
                self.write_assignment_aux_v2(NativeRowBody::SourceUnion(union))?;
                if let Some(receipt) = receipt {
                    self.write_assignment_aux_v2(NativeRowBody::ServiceReceipt(receipt))?;
                }
                self.write_prepared_assigned_case_journal_v2(journal)
            })();
            if result.is_err() {
                self.restore(checkpoint);
            }
            return result;
        }
        Err(missing())
    }
    pub(super) fn apply_positive_assignment_attestation_raw_v2(
        &mut self,
        _ctx: &PalwBlockContextV2,
        action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        self.prepare_case_predecessor_v2(action, body)?;
        Err(missing())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as K;
    use std::collections::BTreeMap;

    #[test]
    fn native_claim_job_lookup_tracks_repeated_jobs_and_delta_undo() {
        let job = Hash64::from_bytes([3; 64]);
        let other = Hash64::from_bytes([4; 64]);
        let first = ZERO_HASH64;
        let last = Hash64::from_bytes([u8::MAX; 64]);
        let mut state = PalwChainStateV2::genesis();
        let initial_root = state.state_root();
        let unset = <PalwClaimStateV2 as borsh::BorshDeserialize>::deserialize(&mut &[0u8; 8192][..]).unwrap();
        assert_eq!(unset.job_identity, ZERO_HASH64);
        let mut named = unset.clone();
        named.job_identity = job;

        let one = PalwDeltaEntryV2::Claim { key: first, old: None, new: Some(unset.clone()) };
        let two = PalwDeltaEntryV2::Claim { key: last, old: None, new: Some(named.clone()) };
        let rename = PalwDeltaEntryV2::Claim { key: first, old: Some(unset), new: Some(named.clone()) };
        let remove = PalwDeltaEntryV2::Claim { key: last, old: Some(named), new: None };
        apply_delta_entry(&mut state, &one, false).unwrap();
        assert!(state.has_claim_for_job_v2(ZERO_HASH64));
        apply_delta_entry(&mut state, &two, false).unwrap();
        apply_delta_entry(&mut state, &rename, false).unwrap();
        apply_delta_entry(&mut state, &remove, false).unwrap();
        assert!(state.has_claim_for_job_v2(job));
        assert!(!state.has_claim_for_job_v2(ZERO_HASH64));
        assert!(!state.has_claim_for_job_v2(other));

        apply_delta_entry(&mut state, &remove, true).unwrap();
        apply_delta_entry(&mut state, &rename, true).unwrap();
        assert!(state.has_claim_for_job_v2(ZERO_HASH64));
        assert!(state.has_claim_for_job_v2(job));
        apply_delta_entry(&mut state, &two, true).unwrap();
        assert!(!state.has_claim_for_job_v2(job));
        apply_delta_entry(&mut state, &one, true).unwrap();
        assert!(!state.has_claim_for_job_v2(ZERO_HASH64));
        assert_eq!(state.state_root(), initial_root);
    }

    #[test]
    fn rooted_positive_claim_refuses_legacy_primary_and_kernel_holder_writes() {
        use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1};
        use misaka_palw_kernel::rows::{TABLE_CLAIMS_V1, TABLE_JOB_CLAIMS_V1, TABLE_OPV_CLAIMS_V1, TABLE_PROOF_SEALS_V1};
        use misaka_palw_positive_contract::{identity::ClaimId, native::KERNEL_AUX_TABLE};

        let claim = Hash64::from_bytes([13; 64]);
        let other = Hash64::from_bytes([17; 64]);
        let mut state = PalwChainStateV2::genesis();
        state.kernel_route = Some(PalwKernelRouteStateV1::new(
            palw_kernel_route_policy_v1(claim, other),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1::default(),
        ));
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, other, 4, 1000, 100, 1000, 0).unwrap();
        let extras = PalwTransitionExtrasV1::default();
        let key = NativeRowKey::ClaimIndex(ClaimId(Digest(claim.as_bytes()))).encode();
        state.kernel_route.as_mut().unwrap().aux.insert((KERNEL_AUX_TABLE, key), vec![1]);
        let root = state.state_root();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(builder.reject_positive_claim_alias_v2(other).is_ok());
        let row = <PalwClaimStateV2 as borsh::BorshDeserialize>::deserialize(&mut &[0u8; 8192][..]).unwrap();
        assert!(matches!(builder.write_claim(claim, Some(row)), Err(PalwStateV2Error::KernelRouteRefused(_))));
        let panel = PalwPanelStateV2 { anchor: other, seats: Vec::new(), bound_daa: 12 };
        assert!(matches!(builder.write_panel(claim, Some(panel)), Err(PalwStateV2Error::KernelRouteRefused(_))));
        assert!(matches!(builder.write_legacy_dispute(claim, Some(Default::default())), Err(PalwStateV2Error::KernelRouteRefused(_))));

        use crate::palw_bisect::{PalwBisectLadderV1, PalwBisectSpaceV1};
        let bond = PalwBondKeyV2(TransactionOutpoint::new(other, 0));
        let court = PalwCourtSessionStateV2 {
            claim,
            challenger_bond: bond,
            opened_daa: 20,
            deadline_daa: 120,
            ladder: PalwBisectLadderV1::open(
                &Hash64::from_u64_word(31),
                &Hash64::from_u64_word(32),
                &Hash64::from_u64_word(33),
                &Hash64::from_u64_word(34),
                PalwBisectSpaceV1::StepLeaves,
                8,
                20,
                24,
            )
            .unwrap(),
            dissection: None,
        };
        assert!(matches!(builder.write_court(other, Some(court)), Err(PalwStateV2Error::KernelRouteRefused(_))));

        use crate::{palw_court_duty_clock_v1::CourtDutyClockV1, palw_staged_court_state_v1::PalwStagedCourtRowV1};
        let duty = CourtDutyClockV1 {
            version: 1,
            profile_id: other,
            duty_id: other,
            claim,
            allocation_id: other,
            source_id: other,
            acquisition: K::TirAcquisition(claim, bond, other),
            answerer: bond,
            demand_id: other,
            turn: 0,
            predecessor_block: other,
            predecessor_daa: 20,
            first_eligible_daa: 21,
            response_deadline_daa: 24,
            original_hard_daa: 25,
            material_until_daa: 30,
        };
        assert!(matches!(
            builder.write_staged_row_v1(K::CourtDutyClock(other), Some(PalwStagedCourtRowV1::CourtDutyClock(Box::new(duty)))),
            Err(PalwStateV2Error::KernelRouteRefused(_))
        ));

        for (table, key, value) in [
            (TABLE_CLAIMS_V1, claim.as_bytes().to_vec(), vec![1]),
            (TABLE_OPV_CLAIMS_V1, claim.as_bytes().to_vec(), vec![1]),
            (TABLE_JOB_CLAIMS_V1, other.as_bytes().to_vec(), claim.as_bytes().to_vec()),
            (TABLE_PROOF_SEALS_V1, [claim.as_bytes(), other.as_bytes()].concat(), vec![1]),
        ] {
            assert!(matches!(builder.write_kernel_row(table, key, Some(value)), Err(PalwStateV2Error::KernelRouteRefused(_))));
        }
        assert!(builder.entries.is_empty());
        assert_eq!(builder.state.state_root(), root);
    }

    #[test]
    fn staged_alias_probes_cover_subject_namespaces_and_boundaries() {
        let subject = Hash64::from_bytes([2; 64]);
        let other = Hash64::from_bytes([3; 64]);
        let high = Hash64::from_bytes([u8::MAX; 64]);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(high, u32::MAX));
        let side = PalwCourtSideV1::Executor;
        let aliases = [
            K::Claim(subject),
            K::Owner(subject, bond),
            K::InventoryOpen(subject, bond),
            K::Inventory(subject),
            K::TirAcquisition(subject, bond, high),
            K::TirRawChunk(subject),
            K::TirAcceptedBody(subject),
            K::TirPreparation(subject, high),
            K::TirClaim(subject),
            K::TirWitness(subject, bond),
            K::CourtDutyClock(subject),
            K::CourtSharedContext(subject),
            K::CourtSharedQuery(subject),
            K::CourtSharedIds(subject),
            K::CourtSharedStream(subject),
            K::CourtSharedChunk(subject, u64::MAX),
            K::CourtSharedResponse(subject, bond),
            K::CourtEndArchive(subject),
            K::CourtCloseReceipt(subject, side),
            K::CourtSharedRoot(subject, u64::MAX),
            K::CourtSharedRound(subject),
            K::CourtSharedBottom(subject),
            K::CourtArchiveProgress(subject, side),
            K::CourtMaterialDraft(subject, high),
            K::CourtMaterial(subject, high),
            K::CourtMaterialLedger(subject, bond),
        ];
        use super::super::palw_staged_court_fold_v1::staged_legacy_alias_subject_v2;
        for key in aliases {
            assert_eq!(staged_legacy_alias_subject_v2(key), Some(subject));
            let rows = BTreeMap::from([(key, ())]);
            assert!(has_legacy_staged_alias_v2(&rows, subject), "{key:?}");
            assert!(!has_legacy_staged_alias_v2(&rows, other), "{key:?}");
        }
        let rows = BTreeMap::from([
            (K::TirLayout(subject), ()),
            (K::TirContainer(subject), ()),
            (K::CourtService, ()),
            (K::CourtMaterialPart(subject, 0), ()),
        ]);
        assert!(!has_legacy_staged_alias_v2(&rows, subject));
        assert!(rows.keys().all(|&key| staged_legacy_alias_subject_v2(key).is_none()));
    }
}
