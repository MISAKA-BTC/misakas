//! Shared connection points, with no fake successful implementation. Associated
//! capability types are owned by their implementing subsystem, not DTO fields.
use crate::{ContractError, identity::*, messages::*, wire::EvidenceFrame};

/// A1 derives the required plan and checks parts; A3 does not supply authority
/// for a shorter plan. Completion is produced by the checker implementation.
pub trait NodeChecker {
    type Context;
    type Progress;
    type CheckedTerminal;
    type Error;
    fn derive_plan(&self, admitted: &Self::Context) -> Result<CoverageDescriptor, Self::Error>;
    fn check_part(&self, admitted: &Self::Context, progress: &mut Self::Progress, part: EvidenceFrame<'_>) -> Result<(), Self::Error>;
    fn finish(&self, admitted: &Self::Context, progress: Self::Progress) -> Result<Self::CheckedTerminal, Self::Error>;
}

/// A2 exposes source data only. No method can demand a Positive witness, full
/// trace reconstruction or projection computation from the Miner.
pub trait MinerSource {
    type AuthenticatedDuty;
    type G14Response;
    type Error;
    /// The duty itself binds the exact canonical request, scope, source catalog,
    /// permitted units, clocks, fees and aggregate reservation. No independently
    /// supplied request can be substituted after duty authentication.
    fn respond(&mut self, duty: &Self::AuthenticatedDuty) -> Result<Self::G14Response, Self::Error>;
}

/// A3 replay/capture and evidence construction are separate from transport;
/// a sender can resume without interpreting a local Clean flag as acceptance.
pub trait PanelBuilder {
    type AdmittedResult;
    type Capture;
    type G14EvidencePart;
    type Error;
    fn replay_and_capture(&self, result: &Self::AdmittedResult) -> Result<Self::Capture, Self::Error>;
    fn commit(
        &self,
        capture: &Self::Capture,
        plan: &CoverageDescriptor,
        author: PrincipalId,
        attempt: u32,
    ) -> Result<PanelWitnessCommit, Self::Error>;
    fn build_part(&self, capture: &Self::Capture, cursor: CoverageCursor) -> Result<Self::G14EvidencePart, Self::Error>;
}

/// A4+A5+A6 transaction intent. Native implementations resolve all roots,
/// signatures, capacity, real funding and conditional obligations BEFORE the
/// single commit boundary. Preparation alone creates no public obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionIntent {
    pub scope: ChainScope,
    pub case: CaseId,
    pub result: ResultId,
    pub resource_quote: Digest,
    pub funding_consent: Digest,
    pub obligations: Digest,
    pub expiry: u64,
}
pub trait AtomicCourtStore {
    type PreparedAdmission;
    type AdmittedCase;
    type CheckedTransition;
    type UndoRecord;
    type Error;
    /// Read-only validation / reversible local reservation. Failure leaves no
    /// funds debit, case row, released resource or externally visible duty.
    fn prepare_admission(&self, intent: AdmissionIntent) -> Result<Self::PreparedAdmission, Self::Error>;
    /// One storage transaction commits case+funding+resource+liability+journal,
    /// or none. Revalidate the prepared intent against CURRENT branch, state
    /// generation, clock, funds, consent and capacity at commit. Same case+same
    /// Result retry is idempotent; same case+different Result is rejected. Never
    /// create another hold or use a stale prepare as current admission authority.
    fn commit_admission(&mut self, prepared: Self::PreparedAdmission) -> Result<Self::AdmittedCase, Self::Error>;
    /// Checked facts have role ownership. A Panel fault cannot become a Miner
    /// refutation without a fully checked terminal comparison to its signature.
    fn apply_transition(
        &mut self,
        case: &Self::AdmittedCase,
        transition: Self::CheckedTransition,
    ) -> Result<Self::UndoRecord, Self::Error>;
    fn rollback(&mut self, undo: Self::UndoRecord) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditUse {
    /// Existing primary issuance/coinbase/model/PWU entitlement only.
    /// Poster-funded worker success payment is a different purpose.
    Reward,
    WorkReceipt,
    DagWeight,
    RuleE,
    Settlement,
    PrefixReuse,
    Dependency,
    /// Existing contract-authorized worker success payment, never primary
    /// issuance. Provided source fees and safe refunds do not use this gate.
    WorkerPayment,
}
/// A5 owns the current rooted predicate. No success implementation is included.
/// A caller may not supply Gate C / complete / mature as boolean parameters.
pub trait CreditGate {
    type RootedClaim;
    type Authorization;
    type Denial;
    fn authorize(&self, claim: &Self::RootedClaim, usage: CreditUse) -> Result<Self::Authorization, Self::Denial>;
}

/// A6 restores checked history and all budgets/accounting under the same branch.
/// It must not return an authority by deserializing a PositivePassed DTO.
pub trait Recovery {
    type TrustedAnchor;
    type BoundedJournal;
    type RestoredState;
    type Error;
    fn replay(&self, anchor: &Self::TrustedAnchor, journal: Self::BoundedJournal) -> Result<Self::RestoredState, Self::Error>;
}

/// Deterministic contiguous coverage is checked at the shared boundary, too.
pub fn require_next(
    plan: &CoverageDescriptor,
    cursor: CoverageCursor,
    witness: &PanelWitnessCommit,
    frame: &EvidenceFrame<'_>,
) -> Result<u64, ContractError> {
    witness.validate_shape()?;
    if plan.profile != CheckProfile::ExactStreamV1 {
        return Err(ContractError::UnsupportedProfile);
    }
    if witness.profile != plan.profile {
        return Err(ContractError::UnsupportedProfile);
    }
    if frame.wire_bytes() > plan.max_part_bytes {
        return Err(ContractError::Budget);
    }
    let header = frame.header();
    if cursor.plan != plan.plan
        || header.plan != plan.plan
        || header.result != plan.result
        || witness.plan != plan.plan
        || witness.result != plan.result
        || witness.scope != plan.scope
        || header.case != witness.case
        || header.witness != witness.id()
    {
        return Err(ContractError::Identity);
    }
    if header.first_relation != cursor.next_relation {
        return Err(ContractError::NonCanonical);
    }
    header.validate()?;
    let end = header.first_relation.checked_add(header.relation_count).ok_or(ContractError::Range)?;
    if end > plan.mandatory_relations {
        return Err(ContractError::Range);
    }
    // Only the new expected cursor. This function performs no arithmetic check
    // and is deliberately incapable of producing CheckedTerminal/credit.
    Ok(end)
}
