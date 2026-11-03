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
    identity::{Digest, PUBKEY_BYTES, PrincipalId},
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

        let Some(route) = self.state.kernel_route.as_ref() else { return Ok(()) };
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

fn assigned_prover_rows_v2(
    aux: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    prefix: Vec<u8>,
) -> impl Iterator<Item = (&[u8], &[u8])> {
    use misaka_palw_positive_contract::native::KERNEL_AUX_TABLE;

    aux.range((KERNEL_AUX_TABLE, prefix.clone())..)
        .take_while(move |((table, key), _)| *table == KERNEL_AUX_TABLE && key.starts_with(&prefix))
        .map(|((_, key), bytes)| (key.as_slice(), bytes.as_slice()))
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
    let assignment = a.id();
    let mut resolved = None;
    let mut count = 0u32;
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<u8>(1024).map_err(PalwStateV2Error::NativeResource)?;
    requests.exact_v1::<u8>(misaka_palw_positive_contract::rows::MAX_NATIVE_KEY_BYTES as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _range_owner = reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(|| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<NativeRowKey>(
            "assigned Case prover role range", "actual checker is not armed",
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
                match self.court.expire_pending_v2(origin.accepted_daa) {
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
        use crate::palw_positive_assignment_case_material_v2::{AssignmentCaseBoundaryV2, initial_assigned_case_bind_header_v2};
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
            let NativeRowBody::AssignmentUse(use_row) = required(current, NativeRowKey::AssignmentUse(id))? else {
                return Err(refuse("actual initial Case has no AssignmentUse row"));
            };
            let bind = initial_assigned_case_bind_header_v2(record, boundary, &use_row).map_err(refuse)?;
            let NativeRowBody::AssignmentJournalFrame(frame) =
                required(current, NativeRowKey::AssignmentJournalFrame { assignment: id, ordinal: bind.ordinal })?
            else {
                return Err(refuse("actual initial Case has no BindResult frame"));
            };
            if frame.header != bind {
                return Err(refuse("initial Case differs from the rooted BindResult frame"));
            }
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

// The concrete archive/Remaining/service factory is still required. These raw
// routes explicitly preserve unavailability instead of constructing a session
// from rooted summaries or accepting a local cached positive verdict.
impl TransitionBuilder<'_> {
    pub(super) fn apply_positive_assignment_case_raw_v2(
        &mut self,
        _ctx: &PalwBlockContextV2,
        _action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        _body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        Err(missing())
    }
    pub(super) fn apply_positive_assignment_attestation_raw_v2(
        &mut self,
        _ctx: &PalwBlockContextV2,
        _action: &super::positive_assignment_raw::PositiveAssignmentRawActionV2,
        _body: &crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
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
        assert!(matches!(
            builder.write_legacy_dispute(claim, Some(Default::default())),
            Err(PalwStateV2Error::KernelRouteRefused(_))
        ));

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
