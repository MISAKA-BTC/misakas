//! A4 Court lifecycle over the A0 r1 contract and an independently implemented
//! Node checker. This module does not alias the old private prototype IDs,
//! allocate wire/signature domains, hold coins, or authorize native credit.
//!
//! Callers resolve admission from rooted state and commit it with funds and the
//! journal through AtomicCourtStore. A local Court object is not that receipt.
use super::admission::{AdmittedCourtV2, CourtAdmissionRootV2};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{CaseId, ChainScope, PUBKEY_BYTES, PlanId, PrincipalId, ResultId, SIGNATURE_BYTES, WitnessId},
    messages::{
        AdmissionContext, ClaimForm, ClaimStatus, CoverageCursor, CoverageDescriptor, MinerResultCommit, MinerServiceContract,
        PanelWitnessCommit, WitnessStatus,
    },
    ports::{NodeChecker, require_next},
    wire::{EvidenceFrame, EvidenceHeader},
};

/// The implementation must delegate to A1's actual independent checker. There
/// is deliberately no successful default checker or terminal classification.
pub trait CourtCheckerV2: NodeChecker {
    fn start(&self, context: &Self::Context) -> Result<Self::Progress, Self::Error>;
    fn terminal_kind(&self, terminal: &Self::CheckedTerminal) -> TerminalKindV2;
    /// Candidate-only classification from the checker's opaque progress. An
    /// error string/category cannot manufacture a Miner fault or a pass.
    fn failure_kind(&self, progress: &Self::Progress) -> CandidateFailureV2;
    fn context_witness_id(&self, context: &Self::Context) -> WitnessId;
    fn terminal_identity(&self, terminal: &Self::CheckedTerminal) -> (ResultId, WitnessId, CoverageDescriptor);
}

/// An ingress adapter must verify the actual complete carrier bytes and its
/// authorized submitter. A Panel witness signature authenticates the commitment
/// only, and MUST NOT be substituted for authorization of arbitrary packets.
/// Native ingress remains unavailable until its existing carrier adapter exists.
pub trait CarrierVerifierV2 {
    type Authorization;
    type Error;
    /// Complete carrier overhead in addition to an unsigned EvidenceFrame.
    /// It participates in the claim-wide worst-case reservation.
    fn max_carrier_overhead(&self) -> u64;
    fn verify(
        &self,
        result: &MinerResultCommit,
        witness: &PanelWitnessCommit,
        frame: &EvidenceFrame<'_>,
        authorization: &Self::Authorization,
    ) -> Result<u64, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKindV2 {
    PositivePassed,
    ClaimRefuted,
    WitnessRejected,
    EndedUnverified,
}
/// Carries no calculation-pass, Miner-refutation, penalty or payment authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateFailureV2 {
    Rejected,
    Unverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtLimitsV2 {
    pub max_attempts: u32,
    pub max_total_wire_bytes: u64,
    pub max_part_bytes: u64,
    pub max_reserved_elements: u64,
    pub max_reserved_terms: u64,
    pub deadline: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateOpenV2 {
    Opened,
    AlreadyActive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractAttemptV2 {
    pub ordinal: u32,
    pub witness: WitnessId,
    pub author: PrincipalId,
    pub status: WitnessStatus,
}
#[derive(Debug)]
pub enum CourtErrorV2<CheckError, CarrierError> {
    Contract(ContractError),
    Check(CheckError),
    Carrier(CarrierError),
    Budget,
    Closed,
    Deadline,
    Order,
    Incomplete,
}

/// Opaque checked fact. Cannot be decoded from a journal's saved Passed bit.
/// Only finish over a complete independently checked history creates this type.
pub struct CheckedCourtFactV2<Terminal> {
    scope: ChainScope,
    case: CaseId,
    result: ResultId,
    witness: WitnessId,
    plan: PlanId,
    kind: TerminalKindV2,
    terminal: Terminal,
}
impl<Terminal> CheckedCourtFactV2<Terminal> {
    pub fn scope(&self) -> ChainScope {
        self.scope
    }
    pub fn case(&self) -> CaseId {
        self.case
    }
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn witness(&self) -> WitnessId {
        self.witness
    }
    pub fn plan(&self) -> PlanId {
        self.plan
    }
    pub fn terminal_kind(&self) -> TerminalKindV2 {
        self.kind
    }
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }
}

/// A checked computation attached to the actual atomic-store receipt. Only the
/// admission coordinator can construct this after rechecking its current view.
/// Holding this is still not Gate C, a payment, or native activation authority.
pub struct AdmittedCheckedFactV2<'a, Case, Terminal> {
    admitted: &'a AdmittedCourtV2<Case>,
    fact: CheckedCourtFactV2<Terminal>,
    rooted: CourtAdmissionRootV2,
}
impl<'a, Case, Terminal> AdmittedCheckedFactV2<'a, Case, Terminal> {
    pub fn admitted_case(&self) -> &'a AdmittedCourtV2<Case> {
        self.admitted
    }
    pub fn checked_fact(&self) -> &CheckedCourtFactV2<Terminal> {
        &self.fact
    }
    pub fn root(&self) -> CourtAdmissionRootV2 {
        self.rooted
    }
    /// Retain the immutable checked computation while discarding this stale
    /// receipt binding. Rebinding still requires a fresh actual store receipt
    /// and the admission coordinator's current-root checks; no status is reset.
    pub fn into_checked_fact(self) -> CheckedCourtFactV2<Terminal> {
        self.fact
    }
}

/// Identity half of the bridge. The sibling admission coordinator must check
/// namespace, current generation/clock and rooted admission BEFORE calling it.
pub(super) fn bind_admitted_fact<'a, Case, Terminal>(
    admitted: &'a AdmittedCourtV2<Case>,
    fact: CheckedCourtFactV2<Terminal>,
    rooted: CourtAdmissionRootV2,
) -> Result<AdmittedCheckedFactV2<'a, Case, Terminal>, ContractError> {
    let intent = admitted.intent();
    if rooted.scope != intent.scope || fact.scope != intent.scope || fact.case != intent.case || fact.result != intent.result {
        return Err(ContractError::Identity);
    }
    Ok(AdmittedCheckedFactV2 { admitted, fact, rooted })
}

struct Candidate<C: CourtCheckerV2> {
    witness: PanelWitnessCommit,
    plan: CoverageDescriptor,
    context: C::Context,
    progress: Option<C::Progress>,
    next: u64,
    status: WitnessStatus,
    // Bounded by max_part_bytes; a single last frame suffices for a retry ACK.
    last: Option<(EvidenceHeader, Vec<u8>)>,
}

/// Claim-wide counters survive candidate/prover changes. No Deserialize and no
/// API accepts caller-supplied booleans for a computation verdict or credit.
pub struct ContractCourtV2<C: CourtCheckerV2, V: CarrierVerifierV2> {
    checker: C,
    verifier: V,
    result: MinerResultCommit,
    limits: CourtLimitsV2,
    claim_status: ClaimStatus,
    attempts: u32,
    wire_bytes: u64,
    elements: u64,
    terms: u64,
    now: u64,
    candidate: Option<Candidate<C>>,
    last_attempt: Option<ContractAttemptV2>,
}
impl<C: CourtCheckerV2, V: CarrierVerifierV2> ContractCourtV2<C, V> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        checker: C,
        verifier: V,
        rooted: &AdmissionContext,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        service: MinerServiceContract,
        limits: CourtLimitsV2,
        now: u64,
    ) -> Result<Self, CourtErrorV2<C::Error, V::Error>> {
        rooted.check(ClaimForm::PositiveResultV2, &result).map_err(CourtErrorV2::Contract)?;
        result.authenticate(miner_key, signature).map_err(CourtErrorV2::Contract)?;
        service.validate(&result, now).map_err(CourtErrorV2::Contract)?;
        if now >= limits.deadline || limits.deadline > service.deadline {
            return Err(CourtErrorV2::Deadline);
        }
        if limits.max_attempts == 0
            || limits.max_attempts > service.budget.witness_attempts
            || limits.max_part_bytes == 0
            || limits.max_total_wire_bytes == 0
            || limits.max_reserved_elements == 0
            || limits.max_reserved_terms == 0
        {
            return Err(CourtErrorV2::Budget);
        }
        Ok(Self {
            checker,
            verifier,
            result,
            limits,
            claim_status: ClaimStatus::Pending,
            attempts: 0,
            wire_bytes: 0,
            elements: 0,
            terms: 0,
            now,
            candidate: None,
            last_attempt: None,
        })
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn status(&self) -> ClaimStatus {
        self.claim_status
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
    pub fn cursor(&self) -> u64 {
        self.candidate.as_ref().map_or(0, |x| x.next)
    }
    pub fn witness_status(&self) -> Option<WitnessStatus> {
        self.last_attempt.map(|x| x.status)
    }
    pub fn last_attempt(&self) -> Option<ContractAttemptV2> {
        self.last_attempt
    }
    pub fn charged_wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    pub fn charged_elements(&self) -> u64 {
        self.elements
    }
    pub fn charged_terms(&self) -> u64 {
        self.terms
    }

    pub fn open_candidate(
        &mut self,
        witness: PanelWitnessCommit,
        panel_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        context: C::Context,
        now: u64,
    ) -> Result<CandidateOpenV2, CourtErrorV2<C::Error, V::Error>> {
        self.ensure_open(now)?;
        witness.authenticate(&self.result, panel_key, signature).map_err(CourtErrorV2::Contract)?;
        if let Some(current) = &self.candidate {
            if current.status == WitnessStatus::Checking {
                return if current.witness == witness { Ok(CandidateOpenV2::AlreadyActive) } else { Err(CourtErrorV2::Order) };
            }
        }
        if self.attempts >= self.limits.max_attempts {
            return Err(CourtErrorV2::Budget);
        }
        if witness.attempt != self.attempts {
            return Err(CourtErrorV2::Order);
        }
        // A signed candidate consumes its ordinal even if its context/plan is
        // invalid; changing the author or commitment cannot create a free retry.
        self.last_attempt = Some(ContractAttemptV2 {
            ordinal: self.attempts,
            witness: witness.id(),
            author: witness.author,
            status: WitnessStatus::Incomplete,
        });
        self.attempts += 1;
        self.candidate = None;
        if self.checker.context_witness_id(&context) != witness.id() {
            return Err(CourtErrorV2::Contract(ContractError::Identity));
        }
        let plan = self.checker.derive_plan(&context).map_err(CourtErrorV2::Check)?;
        if plan.scope != self.result.scope
            || plan.result != self.result.id()
            || plan.plan != witness.plan
            || plan.profile != witness.profile
            || plan.mandatory_relations == 0
            || plan.reserved_elements == 0
            || plan.reserved_terms == 0
            || plan.max_part_bytes == 0
            || plan.max_part_bytes > self.limits.max_part_bytes
        {
            return Err(CourtErrorV2::Contract(ContractError::Identity));
        }
        let signed_part_ceiling = plan.max_part_bytes.checked_add(self.verifier.max_carrier_overhead()).ok_or(CourtErrorV2::Budget)?;
        let wire_reservation = plan
            .mandatory_relations
            .checked_mul(signed_part_ceiling)
            .and_then(|x| x.checked_mul(self.limits.max_attempts as u64))
            .ok_or(CourtErrorV2::Budget)?;
        if wire_reservation > self.limits.max_total_wire_bytes {
            return Err(CourtErrorV2::Budget);
        }
        let reserve = |unit: u64, cap: u64| unit.checked_mul(self.limits.max_attempts as u64).is_some_and(|x| x <= cap);
        if !reserve(plan.reserved_elements, self.limits.max_reserved_elements)
            || !reserve(plan.reserved_terms, self.limits.max_reserved_terms)
        {
            return Err(CourtErrorV2::Budget);
        }
        let elements = self
            .elements
            .checked_add(plan.reserved_elements)
            .filter(|x| *x <= self.limits.max_reserved_elements)
            .ok_or(CourtErrorV2::Budget)?;
        let terms = self
            .terms
            .checked_add(plan.reserved_terms)
            .filter(|x| *x <= self.limits.max_reserved_terms)
            .ok_or(CourtErrorV2::Budget)?;
        // Reserve before start. Failed checker construction still spends its
        // admitted work ceiling and cannot replenish CPU by alternating authors.
        self.elements = elements;
        self.terms = terms;
        let progress = self.checker.start(&context).map_err(CourtErrorV2::Check)?;
        self.candidate =
            Some(Candidate { witness, plan, context, progress: Some(progress), next: 0, status: WitnessStatus::Checking, last: None });
        self.last_attempt.as_mut().expect("attempt recorded").status = WitnessStatus::Checking;
        Ok(CandidateOpenV2::Opened)
    }

    pub fn submit_frame(
        &mut self,
        frame: EvidenceFrame<'_>,
        authorization: &V::Authorization,
        now: u64,
    ) -> Result<(), CourtErrorV2<C::Error, V::Error>> {
        self.ensure_open(now)?;
        if frame.wire_bytes() > self.limits.max_part_bytes {
            return Err(CourtErrorV2::Budget);
        }
        let candidate = self.candidate.as_mut().ok_or(CourtErrorV2::Order)?;
        if candidate.status != WitnessStatus::Checking {
            return Err(CourtErrorV2::Closed);
        }
        // The verifier authenticates THIS payload, not merely a replayable
        // witness signature. No invalid transport reaches A1's mutable progress.
        let billed_bytes =
            self.verifier.verify(&self.result, &candidate.witness, &frame, authorization).map_err(CourtErrorV2::Carrier)?;
        let signed_ceiling = frame.wire_bytes().checked_add(self.verifier.max_carrier_overhead()).ok_or(CourtErrorV2::Budget)?;
        if billed_bytes < frame.wire_bytes() || billed_bytes > signed_ceiling {
            return Err(CourtErrorV2::Budget);
        }
        let total = self
            .wire_bytes
            .checked_add(billed_bytes)
            .filter(|x| *x <= self.limits.max_total_wire_bytes)
            .ok_or(CourtErrorV2::Budget)?;
        self.wire_bytes = total;
        if candidate.last.as_ref().is_some_and(|(header, payload)| *header == frame.header() && payload == frame.payload()) {
            return Ok(());
        }
        let end = require_next(
            &candidate.plan,
            CoverageCursor { plan: candidate.plan.plan, next_relation: candidate.next },
            &candidate.witness,
            &frame,
        )
        .map_err(CourtErrorV2::Contract)?;
        let progress = candidate.progress.as_mut().ok_or(CourtErrorV2::Closed)?;
        // Discard the old retry ACK before decoding the next part. Capture
        // the new ACK only after the checker drops its transient part buffers.
        candidate.last = None;
        if let Err(error) = self.checker.check_part(&candidate.context, progress, frame) {
            // Only authenticated material reached the checker. Read the actual
            // candidate outcome; budget/unadjudicable failures remain unverified.
            // Neither branch creates a checked fact or changes the Pending claim.
            candidate.status = match self.checker.failure_kind(progress) {
                CandidateFailureV2::Rejected => WitnessStatus::Rejected,
                CandidateFailureV2::Unverified => WitnessStatus::Incomplete,
            };
            self.last_attempt.as_mut().expect("attempt recorded").status = candidate.status;
            candidate.progress = None;
            candidate.last = None;
            return Err(CourtErrorV2::Check(error));
        }
        candidate.next = end;
        candidate.last = Some((frame.header(), frame.payload().to_vec()));
        Ok(())
    }

    pub fn finish(&mut self, now: u64) -> Result<CheckedCourtFactV2<C::CheckedTerminal>, CourtErrorV2<C::Error, V::Error>> {
        self.ensure_open(now)?;
        let candidate = self.candidate.as_mut().ok_or(CourtErrorV2::Order)?;
        if candidate.status != WitnessStatus::Checking {
            return Err(CourtErrorV2::Closed);
        }
        if candidate.next != candidate.plan.mandatory_relations {
            return Err(CourtErrorV2::Incomplete);
        }
        let progress = candidate.progress.take().ok_or(CourtErrorV2::Closed)?;
        let terminal = match self.checker.finish(&candidate.context, progress) {
            Ok(terminal) => terminal,
            Err(error) => {
                candidate.status = WitnessStatus::Incomplete;
                self.last_attempt.as_mut().expect("attempt recorded").status = WitnessStatus::Incomplete;
                return Err(CourtErrorV2::Check(error));
            }
        };
        let identity = self.checker.terminal_identity(&terminal);
        if identity != (self.result.id(), candidate.witness.id(), candidate.plan) {
            candidate.status = WitnessStatus::Incomplete;
            self.last_attempt.as_mut().expect("attempt recorded").status = WitnessStatus::Incomplete;
            candidate.last = None;
            return Err(CourtErrorV2::Contract(ContractError::Identity));
        }
        let kind = self.checker.terminal_kind(&terminal);
        match kind {
            TerminalKindV2::PositivePassed => {
                self.claim_status = ClaimStatus::PositivePassed;
                candidate.status = WitnessStatus::Complete;
            }
            TerminalKindV2::ClaimRefuted => {
                self.claim_status = ClaimStatus::ClaimRefuted;
                candidate.status = WitnessStatus::Complete;
            }
            TerminalKindV2::WitnessRejected => {
                candidate.status = WitnessStatus::Rejected;
            }
            TerminalKindV2::EndedUnverified => {
                self.claim_status = ClaimStatus::EndedUnverified;
                candidate.status = WitnessStatus::Incomplete;
            }
        }
        self.last_attempt.as_mut().expect("attempt recorded").status = candidate.status;
        candidate.last = None;
        Ok(CheckedCourtFactV2 {
            scope: self.result.scope,
            case: self.result.case_id(),
            result: self.result.id(),
            witness: candidate.witness.id(),
            plan: candidate.plan.plan,
            kind,
            terminal,
        })
    }

    pub fn expire(&mut self, now: u64) -> Result<(), CourtErrorV2<C::Error, V::Error>> {
        if now < self.now {
            return Err(CourtErrorV2::Order);
        }
        self.now = now;
        if now >= self.limits.deadline && self.claim_status == ClaimStatus::Pending {
            self.claim_status = ClaimStatus::EndedUnverified;
            if let Some(candidate) = &mut self.candidate {
                if candidate.status == WitnessStatus::Checking {
                    candidate.status = WitnessStatus::Incomplete;
                    self.last_attempt.as_mut().expect("attempt recorded").status = WitnessStatus::Incomplete;
                }
                candidate.progress = None;
                candidate.last = None;
            }
        }
        Ok(())
    }
    pub fn revoke(&mut self) {
        self.claim_status = ClaimStatus::Revoked;
        if let Some(candidate) = &mut self.candidate {
            if candidate.status == WitnessStatus::Checking {
                candidate.status = WitnessStatus::Incomplete;
                self.last_attempt.as_mut().expect("attempt recorded").status = WitnessStatus::Incomplete;
            }
            candidate.progress = None;
            candidate.last = None;
        }
    }
    fn ensure_open(&mut self, now: u64) -> Result<(), CourtErrorV2<C::Error, V::Error>> {
        self.expire(now)?;
        if self.claim_status != ClaimStatus::Pending {
            return Err(CourtErrorV2::Closed);
        }
        Ok(())
    }
}
