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
use std::{cell::RefCell, rc::Rc};

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
    prepared: PreparedCheck,
    record_limits: crate::palw_positive_contract_v2::record::RecordLimits,
    authority: NativeAssignedPanelAuthorityV2,
    court: ContractCourtV2<TirCourtCheckerV2, PanelSignatureVerifierV2<NativeAssignedPanelAuthorityV2>>,
    source: CommonSourceReplayStateV2,
    facts: Vec<LocalCheckedCourtFactV2>,
    terminal_material: Option<AcceptedMaterial>,
    positive_final: Option<AcceptedOrigin>,
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
    pub(super) fn ensure_current(&self, state: &PalwChainStateV2, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
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
        Ok(())
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

/// Exact native aliases must be absent in EVERY old entry surface. A zero
/// unresolved counter alone is deliberately not accepted as evidence of this.
fn reject_legacy_aliases(state: &PalwChainStateV2, a: &AssignmentRow, result: &MinerResultCommit) -> Result<(), PalwStateV2Error> {
    let claim = Hash64::from_bytes(result.claim_id().0.0);
    let job = Hash64::from_bytes(result.job.0);
    if state.claims.contains_key(&claim)
        || state.claims.values().any(|row| row.job_identity == job)
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
    // Real typed projection owns its native allocation/decoder; unsupported or
    // missing projection is an error, never an empty kernel ledger.
    let ledger = route
        .ledger_native_projection_checked_v9()
        .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, |_| missing()))?;
    let claim = result.claim_id().0.0;
    let job = result.job.0;
    if ledger.claims.contains_key(&claim)
        || ledger.claims.values().any(|row| row.job_id == job)
        || ledger.jobs.contains_key(&job)
        || ledger.pipeline_jobs.contains_key(&job)
        || ledger.job_claims.contains_key(&job)
        || ledger.job_claims.values().any(|id| *id == claim)
        || ledger.job_escrows.contains_key(&job)
        || ledger.seals.range((job, [0; 64])..=(job, [u8::MAX; 64])).next().is_some()
        || ledger.proof_seals.range((claim, [0; 64])..=(claim, [u8::MAX; 64])).next().is_some()
        || ledger.opv.claims.contains_key(&claim)
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
            || self.court.result() != &bundle.result.result
            || bundle.case.unresolved_fraud != 0
        {
            return Err(refuse("actual replay before rows disagree"));
        }
        let terminal = self.facts.last();
        let material = self.terminal_material.ok_or_else(missing)?;
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
        if latest > a.bundle.terms.reserve_until_daa {
            return Err(refuse("actual Final exceeds original liability owner"));
        }
        let session = NativeAssignedCaseSessionV2 {
            root: self.root,
            context: *ctx,
            assignment: a.id(),
            result: self.court.result(),
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

/// Resolve the literal original or accepted funded op13 role in the ACTUAL
/// prefix. Historical replay calls with carriage.prefix_state(), never a later
/// state, and a retired actor cannot buy a replacement global Court budget.
fn prover_step(
    state: &PalwChainStateV2,
    a: &AssignmentRow,
    material: &AssignmentAdmissionMaterialV2,
    author: PrincipalId,
    origin: AcceptedOrigin,
    new_candidate: bool,
) -> Result<NativeAssignedPanelStepV2, PalwStateV2Error> {
    use misaka_palw_positive_contract::assignment::{AssignmentRoleV2, rows::AssignmentRoleAuthorizationV2};
    use misaka_palw_positive_contract::rows::NativeRowValue;
    let route = state.kernel_route_state_v1().ok_or_else(missing)?;
    let mut resolved = None;
    let mut count = 0u32;
    for ((table, key), bytes) in &route.aux {
        if *table != misaka_palw_positive_contract::native::KERNEL_AUX_TABLE || key.get(4) != Some(&15) {
            continue;
        }
        let value = NativeRowValue::decode(key, bytes).map_err(refuse)?;
        let NativeRowBody::AssignmentRole(role) = value.body else {
            return Err(refuse("role key/value"));
        };
        if role.assignment != a.id() || role.role != AssignmentRoleV2::Prover {
            continue;
        }
        count = count.checked_add(1).ok_or_else(|| refuse("role count overflow"))?;
        if count > material.assignment_policy.max_role_candidates {
            return Err(refuse("bounded native role candidates"));
        }
        if misaka_palw_positive_contract::identity::principal(&role.key) != author {
            continue;
        }
        role.authenticate(a).map_err(refuse)?;
        if (new_candidate && role.retired) || role.origin.accepted_daa > origin.accepted_daa {
            return Err(refuse("role absent/retired at actual event prefix"));
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
    resolved.ok_or_else(missing)
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
        use crate::palw_positive_common_replay_v2::{LocalCourtErrorV2, LocalCourtOutcomeV2 as Outcome};
        use crate::palw_positive_court_v2::coordinator::{
            carrier::PanelPacketSignatureV2,
            contract::{CandidateOpenV2, CourtErrorV2},
            native_body::PositiveNativeOperationV2 as Op,
            record_ingress::preflight_witness_record_v2,
        };
        use misaka_palw_positive_contract::wire::EvidenceFrame;
        if state.state_root() != self.root
            || self.context != *ctx
            || action.origin().accepting_block.0 != ctx.block.as_bytes()
            || action.origin().accepted_daa != ctx.daa_score
            || action.origin().accepted_blue_score != ctx.blue_score
            || !canonical_equal(body, action.transaction().payload.get(3..).ok_or_else(missing)?)
        {
            return Err(refuse("actual native Case current prefix/raw operation"));
        }
        let before = snapshot(&self.court);
        let origin = action.origin();
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
                let step = prover_step(state, &self.assignment, &self.material, witness.author, origin, true)?;
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
                    Ok(CandidateOpenV2::AlreadyActive) => return Err(refuse("fully idempotent candidate has no new journal event")),
                    Err(error) => semantic(error)?,
                }
            }
            Op::ExactFrame { frame, panel_signature } => {
                let frame = EvidenceFrame::decode(frame, self.material.positive_policy.max_frame_bytes).map_err(refuse)?;
                let header = frame.header();
                let active = self.court.last_attempt().ok_or_else(|| refuse("frame without active candidate"))?;
                if header.case != self.pin.case
                    || header.result != self.pin.result
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
                let step = prover_step(state, &self.assignment, &self.material, witness.witness.author, origin, false)?;
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
                if *case != self.pin.case
                    || *result != self.pin.result
                    || self.court.last_attempt().map(|a| a.witness) != Some(*witness)
                {
                    return Err(refuse("Finish exact active candidate"));
                }
                match self.court.finish(origin.accepted_daa) {
                    Ok(fact) => {
                        if self.facts.len() as u64 >= u64::from(self.material.positive_policy.max_attempts)
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
                if *case != self.pin.case || *result != self.pin.result {
                    return Err(refuse("Expire Case identity"));
                }
                match self.court.expire(origin.accepted_daa) {
                    Ok(()) => Outcome::Expired,
                    Err(error) => semantic(error)?,
                }
            }
            Op::SourceRequest { request, .. } => {
                if request.case != self.pin.case {
                    return Err(refuse("source request Case"));
                }
                Outcome::External
            }
            Op::SourceResponse { case, .. } => {
                if *case != self.pin.case {
                    return Err(refuse("source response Case"));
                }
                Outcome::External
            }
            Op::ResultAdmission { .. } => return Err(refuse("assigned admission must be genuine op9")),
            Op::PositiveCourtAttestation { .. } => return Err(refuse("P owner handles actual op7")),
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

impl TransitionBuilder<'_> {
    fn check_case_predecessor_v2(
        &self,
        action: &PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        let backing = action.case_backing().map_err(|_| missing())?;
        let (parent, assignment, pin) = match backing {
            NativeAssignedCaseBackingV2::CommittedHead { parent, assignment, pin, record, .. } => {
                if record.carriage().origin() != pin.head.history.origin {
                    return Err(refuse("Case predecessor origin differs from its rooted journal head"));
                }
                (*parent, *assignment, *pin)
            }
            NativeAssignedCaseBackingV2::Committed(backing) => (backing.parent(), backing.assignment().id(), *backing.pin()),
            NativeAssignedCaseBackingV2::AbsentAtParent { .. } => return Err(missing()),
        };
        let Some((case, expected_result)) = body.operation.case_target_v2().map_err(refuse)? else {
            return Err(refuse("a Case follow-up requires an explicit subject"));
        };
        if parent != action.selected_parent() || case != pin.case || expected_result.is_some_and(|id| id != pin.result) {
            return Err(refuse("Case follow-up differs from its selected-parent archive subject"));
        }
        let (_, _, current) = rooted_bundle(&self.state, assignment)?;
        if current != pin {
            // A preceding operation in this block can advance the head. Its
            // unpublished material must be replayed before this operation.
            return Err(missing());
        }
        Ok(())
    }
}

// The concrete archive/Remaining/service factory is still required. These raw
// routes explicitly preserve unavailability instead of constructing a session
// from rooted summaries or accepting a local cached positive verdict.
impl TransitionBuilder<'_> {
    pub(super) fn apply_positive_assignment_case_raw_v2(
        &mut self,
        _ctx: &PalwBlockContextV2,
        action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        self.check_case_predecessor_v2(action, body)?;
        Err(missing())
    }
    pub(super) fn apply_positive_assignment_attestation_raw_v2(
        &mut self,
        _ctx: &PalwBlockContextV2,
        action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        self.check_case_predecessor_v2(action, body)?;
        Err(missing())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as K;
    use std::collections::BTreeMap;

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
        for key in aliases {
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
    }
}
