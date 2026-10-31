//! A4's concrete link to the independent A1 checker. Native carrier authorization
//! and atomic admission are mandatory separate ports, with no success default.
use crate::palw_positive_court_v2::coordinator::contract::{CandidateFailureV2, CourtCheckerV2, TerminalKindV2};
use crate::{
    palw_positive_check_v2::CheckOutcome,
    palw_positive_contract_v2::{AdapterError, CheckContext, CheckedTerminal, ContractProgress, TirNodeChecker},
};
use misaka_palw_positive_contract::{
    identity::{ResultId, WitnessId},
    messages::CoverageDescriptor,
    ports::NodeChecker,
    wire::EvidenceFrame,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct TirCourtCheckerV2;
impl NodeChecker for TirCourtCheckerV2 {
    type Context = CheckContext;
    type Progress = ContractProgress;
    type CheckedTerminal = CheckedTerminal;
    type Error = AdapterError;
    fn derive_plan(&self, context: &CheckContext) -> Result<CoverageDescriptor, AdapterError> {
        TirNodeChecker.derive_plan(context)
    }
    fn check_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        frame: EvidenceFrame<'_>,
    ) -> Result<(), AdapterError> {
        TirNodeChecker.check_part(context, progress, frame)
    }
    fn finish(&self, context: &CheckContext, progress: ContractProgress) -> Result<CheckedTerminal, AdapterError> {
        TirNodeChecker.finish(context, progress)
    }
}
impl CourtCheckerV2 for TirCourtCheckerV2 {
    fn start(&self, context: &CheckContext) -> Result<ContractProgress, AdapterError> {
        context.start()
    }
    fn same_context(&self, left: &CheckContext, right: &CheckContext) -> bool {
        left.same_identity(right)
    }
    fn failure_kind(&self, progress: &ContractProgress) -> CandidateFailureV2 {
        match progress.snapshot().outcome {
            CheckOutcome::WitnessRejected => CandidateFailureV2::Rejected,
            _ => CandidateFailureV2::Unverified,
        }
    }
    fn context_witness_id(&self, context: &CheckContext) -> WitnessId {
        context.witness_id()
    }
    fn terminal_identity(&self, terminal: &CheckedTerminal) -> (ResultId, WitnessId, CoverageDescriptor) {
        (terminal.result(), terminal.witness(), terminal.descriptor())
    }
    fn terminal_kind(&self, terminal: &CheckedTerminal) -> TerminalKindV2 {
        match terminal.outcome() {
            CheckOutcome::Matched => TerminalKindV2::PositivePassed,
            CheckOutcome::OutputMismatch => TerminalKindV2::ClaimRefuted,
            CheckOutcome::WitnessRejected => TerminalKindV2::WitnessRejected,
            _ => TerminalKindV2::EndedUnverified,
        }
    }
}
