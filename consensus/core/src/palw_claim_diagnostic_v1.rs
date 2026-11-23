//! Read-only claim diagnostics derived from a single selected-parent state.
use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2, Qf1OriginalVerification};
use crate::{Hash64, palw_security_certification_v1 as certification};
use misaka_palw_kernel::{ledger::ClaimRowV1, lifecycle::ClaimStateV1, rows::TABLE_CLAIMS_V1};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStage {
    Admission,
    Panel,
    Court,
    Final,
    GateC,
    Ticket,
    RightConsumption,
    AwardBlock,
    Moe2Settlement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDisposition {
    Observed,
    Waiting,
    Rejected,
    NotRecorded,
    NotApplicable,
    NotImplemented,
    LocalFailure,
    PrerequisiteMissing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClaimStageRead {
    pub stage: ClaimStage,
    pub disposition: ClaimDisposition,
    pub code: &'static str,
    pub next: &'static str,
    pub detail: Option<String>,
    pub daa: Option<u64>,
    pub block: Option<Hash64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ClaimAttemptFailure {
    pub claim: Hash64,
    pub selected_parent: Hash64,
    pub candidate: Hash64,
    pub daa: u64,
    pub stage: ClaimStage,
    pub disposition: ClaimDisposition,
    pub code: &'static str,
    pub next: &'static str,
    pub repetitions: u32,
}

#[derive(Default)]
pub struct ClaimAttemptFailureCache {
    rows: std::collections::VecDeque<ClaimAttemptFailure>,
}

impl ClaimAttemptFailureCache {
    pub fn record(&mut self, mut failure: ClaimAttemptFailure) {
        let previous =
            self.rows.iter().position(|entry| (entry.claim, entry.selected_parent) == (failure.claim, failure.selected_parent));
        if let Some(previous) = previous.and_then(|index| self.rows.remove(index)) {
            if (previous.stage, previous.code) == (failure.stage, failure.code) {
                failure.repetitions = previous.repetitions.saturating_add(1);
            }
        }
        const MAX_FAILURES: usize = 128;
        if self.rows.len() == MAX_FAILURES {
            self.rows.pop_front();
        }
        self.rows.push_back(failure);
    }

    pub fn current(&self, claim: Hash64, parent: Hash64) -> Option<ClaimAttemptFailure> {
        self.rows.iter().find(|entry| (entry.claim, entry.selected_parent) == (claim, parent)).copied()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PalwClaimDiagnosticV1 {
    pub schema_version: u16,
    pub claim: Hash64,
    pub network: String,
    pub genesis: Hash64,
    pub config_id: String,
    pub build_commit: Option<&'static str>,
    pub profile: &'static str,
    pub selected_parent: Hash64,
    pub state_root: Option<Hash64>,
    pub state_root_code: &'static str,
    pub evaluated_daa: Option<u64>,
    pub original_deadline_daa: Option<u64>,
    pub work_key: Option<Hash64>,
    pub stages: [ClaimStageRead; 9],
    /// A refused local attempt, separate from committed stage observations.
    pub last_local_failure: Option<ClaimAttemptFailure>,
}

impl PalwClaimDiagnosticV1 {
    pub fn new(claim: Hash64, selected_parent: Hash64) -> Self {
        use ClaimStage::*;
        let stages = [Admission, Panel, Court, Final, GateC, Ticket, RightConsumption, AwardBlock, Moe2Settlement].map(|stage| {
            ClaimStageRead {
                stage,
                disposition: ClaimDisposition::PrerequisiteMissing,
                code: "PALW.PREREQUISITE_MISSING",
                next: "resolve_earlier_stage",
                detail: None,
                daa: None,
                block: None,
            }
        });
        Self {
            schema_version: 1,
            claim,
            network: String::new(),
            genesis: Hash64::default(),
            config_id: String::new(),
            build_commit: None,
            profile: "unknown",
            selected_parent,
            state_root: None,
            state_root_code: "PALW.NATIVE_STATE_UNAVAILABLE",
            evaluated_daa: None,
            original_deadline_daa: None,
            work_key: None,
            stages,
            last_local_failure: None,
        }
    }

    fn record(&mut self, stage: ClaimStage, disposition: ClaimDisposition, code: &'static str, next: &'static str) {
        let entry = &mut self.stages[stage as usize];
        entry.disposition = disposition;
        entry.code = code;
        entry.next = next;
    }

    pub fn unavailable(&mut self) {
        self.record(ClaimStage::Admission, ClaimDisposition::LocalFailure, "PALW.NATIVE_STATE_UNAVAILABLE", "restore_native_state");
    }

    pub fn observe(&mut self, state: &PalwChainStateV2, params: &PalwStateParamsV2) {
        use ClaimDisposition::*;
        use ClaimStage::*;
        let Some(point) = state.last_point().filter(|point| point.block == self.selected_parent) else {
            self.unavailable();
            return;
        };
        self.evaluated_daa = Some(point.daa_score);
        if let Some(settled) = state.private_moe2_exec_settlement(&self.claim) {
            self.profile = "private_moe2";
            if let Some((block, deadline)) = state.qf1_original_receipt(self.claim) {
                self.record(Admission, Observed, "PALW.ORIGINAL_RECORDED", "none");
                self.stages[Admission as usize].block = Some(block);
                self.original_deadline_daa = Some(deadline);
            } else {
                self.record(Admission, NotRecorded, "PALW.ORIGINAL_NOT_RETAINED", "inspect_archived_history");
            }
            for stage in [Panel, Court, Final, GateC, Ticket] {
                self.record(stage, NotRecorded, "PALW.STAGE_NOT_ESTABLISHED_BY_SETTLEMENT_ROW", "inspect_archived_history");
            }
            for (stage, code) in [
                (RightConsumption, "PALW.CONSUMPTION_RECORDED"),
                (AwardBlock, "PALW.AWARD_BLOCK_RECORDED"),
                (Moe2Settlement, "PALW.SETTLEMENT_RECORDED"),
            ] {
                self.record(stage, Observed, code, "none");
                self.stages[stage as usize].block = Some(settled.position.point.block);
                self.stages[stage as usize].daa = Some(settled.position.point.daa_score);
            }
            return;
        }
        if let Some((block, deadline)) = state.qf1_original_receipt(self.claim) {
            self.profile = "private_qf1_moe2";
            self.original_deadline_daa = Some(deadline);
            self.record(Admission, Observed, "PALW.ORIGINAL_RECORDED", "none");
            self.stages[Admission as usize].block = Some(block);
            match state.qf1_original_verification(self.claim) {
                Some(Qf1OriginalVerification::ReplayApproved) => self.record(Panel, Observed, "PALW.REPLAY_APPROVED", "none"),
                Some(Qf1OriginalVerification::AwaitingReplay) => {
                    self.record(Panel, Waiting, "PALW.PANEL_REPLAY_PENDING", "wait_for_panel_replay");
                }
                Some(Qf1OriginalVerification::Unverified) => self.record(Panel, Rejected, "PALW.PANEL_REPLAY_EXPIRED", "end_claim"),
                None => self.record(Panel, LocalFailure, "PALW.PANEL_STATE_UNAVAILABLE", "restore_native_state"),
            }
            match state.qf1_terminal_outcome_for_diagnostic(self.claim) {
                Some((_, true)) => self.record(Court, Rejected, "PALW.QF1_COURT_REFUTED", "end_claim"),
                Some((pending, false)) if pending > 0 => {
                    self.record(Court, Waiting, "PALW.QF1_COURT_PENDING", "wait_for_court_outcome")
                }
                _ => self.record(Court, NotRecorded, "PALW.NO_QF1_AWARD_BLOCK_RECORDED", "inspect_required_court_checks"),
            }
            self.record(Final, NotImplemented, "PALW.QF1_SUCCESSFUL_FINAL_NOT_CONNECTED", "requires_native_integration");
            self.record(Ticket, NotImplemented, "PALW.MOE2_WINNING_RIGHT_NOT_CONNECTED", "requires_native_integration");
            return;
        }
        if let Some(claim) = state.claim(&self.claim) {
            self.profile = "native_palw";
            self.work_key = claim.work_id;
            self.original_deadline_daa = state.deadline_of(&self.claim);
            self.record(Admission, Observed, "PALW.CLAIM_ACCEPTED", "none");
            self.stages[Admission as usize].block = Some(claim.accepted_block);
            self.stages[Admission as usize].daa = Some(claim.accepted_daa);
            match &claim.phase {
                PalwClaimPhaseV2::Provisional => self.record(Panel, Waiting, "PALW.PANEL_BINDING_PENDING", "wait_for_panel_binding"),
                PalwClaimPhaseV2::PanelBound { .. } => {
                    self.record(Panel, Waiting, "PALW.PANEL_RECEIPTS_PENDING", "wait_for_panel_receipts")
                }
                PalwClaimPhaseV2::ReceiptLicensed { .. } => self.record(Panel, Observed, "PALW.CLAIM_LICENSED", "none"),
                PalwClaimPhaseV2::DefaultDisputed { .. } | PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {
                    self.record(Panel, NotRecorded, "PALW.PANEL_OUTCOME_NOT_IN_CURRENT_PHASE", "inspect_archived_history");
                }
            }
            if state.open_courts_of(&self.claim) != 0 || matches!(claim.phase, PalwClaimPhaseV2::DefaultDisputed { .. }) {
                self.record(Court, Waiting, "PALW.COURT_PENDING", "wait_for_court_outcome");
            } else {
                self.record(Court, NotRecorded, "PALW.NO_OPEN_COURT_RECORDED", "inspect_required_court_checks");
            }
            match claim.phase {
                PalwClaimPhaseV2::Final { final_daa } => {
                    self.record(Final, Observed, "PALW.FINAL_RECORDED", "none");
                    self.stages[Final as usize].daa = Some(final_daa);
                    let status = certification::palw_claim_certification_status_v1(state, params, &self.claim, claim);
                    match status {
                        "not-governed" => self.record(GateC, NotApplicable, "PALW.GATE_C_NOT_GOVERNED", "none"),
                        "certified" | "public-check-certified" => self.record(GateC, Observed, "PALW.GATE_C_CERTIFIED", "none"),
                        "pending" => self.record(GateC, Waiting, "PALW.GATE_C_PENDING", "wait_for_certification"),
                        _ => self.record(GateC, Rejected, "PALW.GATE_C_REFUSED", "inspect_certification_outcome"),
                    }
                    self.stages[GateC as usize].detail = Some(status.to_string());
                    self.stages[GateC as usize].daa = certification::palw_claim_certified_daa_v1(state, &self.claim);
                    self.record_untracked_award();
                }
                PalwClaimPhaseV2::Voided { voided_daa, reason } => {
                    self.record(Final, Rejected, "PALW.CLAIM_VOIDED", "end_claim");
                    self.stages[Final as usize].daa = Some(voided_daa);
                    self.stages[Final as usize].detail = Some(format!("{reason:?}"));
                }
                _ => self.record(Final, Waiting, "PALW.FINAL_PENDING", "wait_for_claim_transition"),
            }
            return;
        }
        if let Some(bytes) =
            state.kernel_route_state_v1().and_then(|route| route.rows.get(&(TABLE_CLAIMS_V1, self.claim.as_bytes().to_vec())))
        {
            self.profile = "kernel_palw";
            use crate::palw_native_resource_v1::{
                NativeDecodeErrorV1, preflight_native_value_v1, retain_native_value_decode_classified_v1,
            };
            let bytes = bytes.as_slice();
            if preflight_native_value_v1::<ClaimRowV1>(bytes, bytes.len() as u64).is_err() {
                self.record(Admission, LocalFailure, "PALW.KERNEL_CLAIM_CORRUPT", "inspect_native_storage");
                return;
            }
            let row = match retain_native_value_decode_classified_v1(bytes, || borsh::from_slice::<ClaimRowV1>(bytes)) {
                Ok(row) => row,
                Err(NativeDecodeErrorV1::Decode(_)) => {
                    self.record(Admission, LocalFailure, "PALW.KERNEL_CLAIM_CORRUPT", "inspect_native_storage");
                    return;
                }
                Err(NativeDecodeErrorV1::Resource(_)) => {
                    self.record(Admission, LocalFailure, "PALW.NATIVE_CAPACITY_UNAVAILABLE", "restore_read_capacity");
                    return;
                }
            };
            self.record(Admission, Observed, "PALW.CLAIM_ACCEPTED", "none");
            self.stages[Admission as usize].daa = Some(row.committed_daa);
            self.original_deadline_daa = row.liability_until;
            match &row.life.state {
                ClaimStateV1::Committed | ClaimStateV1::ChallengeBound { .. } | ClaimStateV1::Checking { .. } => {
                    self.record(Panel, Waiting, "PALW.KERNEL_CHECK_PENDING", "wait_for_kernel_check");
                }
                ClaimStateV1::ProbabilisticPass { .. } => self.record(Panel, Observed, "PALW.KERNEL_CHECK_PASSED", "none"),
                ClaimStateV1::Challengeable { .. } => self.record(Panel, NotApplicable, "PALW.OPEN_CHALLENGE_PROFILE", "none"),
                _ => self.record(Panel, NotRecorded, "PALW.PANEL_OUTCOME_NOT_IN_CURRENT_PHASE", "inspect_archived_history"),
            }
            match &row.life.state {
                ClaimStateV1::Disputed { .. } => self.record(Court, Waiting, "PALW.COURT_PENDING", "wait_for_court_outcome"),
                ClaimStateV1::Challengeable { window_end_daa, .. } | ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => {
                    self.record(Court, Waiting, "PALW.CHALLENGE_WINDOW", "wait_for_window_and_court");
                    self.stages[Court as usize].daa = Some(*window_end_daa);
                }
                _ => self.record(Court, NotRecorded, "PALW.COURT_OUTCOME_NOT_IN_CURRENT_PHASE", "inspect_archived_history"),
            }
            match &row.life.state {
                ClaimStateV1::Final { final_daa } => {
                    self.record(Final, Observed, "PALW.FINAL_RECORDED", "none");
                    self.stages[Final as usize].daa = Some(*final_daa);
                    let cert = certification::palw_kernel_claim_certification_row_v1(state, &self.claim.as_bytes());
                    match cert.and_then(|row| row.outcome) {
                        Some((outcome, daa)) => {
                            let (disposition, code, next) = if outcome.certifies() {
                                (Observed, "PALW.GATE_C_CERTIFIED", "none")
                            } else {
                                (Rejected, "PALW.GATE_C_REFUSED", "inspect_certification_outcome")
                            };
                            self.record(GateC, disposition, code, next);
                            self.stages[GateC as usize].daa = Some(daa);
                            self.stages[GateC as usize].detail = Some(format!("{outcome:?}"));
                        }
                        None => {
                            self.record(GateC, NotRecorded, "PALW.GATE_C_OUTCOME_NOT_RECORDED", "inspect_certification_applicability")
                        }
                    }
                    self.record_untracked_award();
                }
                ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. } => {
                    self.record(Final, Rejected, "PALW.KERNEL_CLAIM_ENDED", "end_claim");
                }
                _ => self.record(Final, Waiting, "PALW.FINAL_PENDING", "wait_for_claim_transition"),
            }
            return;
        }
        self.record(Admission, NotRecorded, "PALW.CLAIM_NOT_RETAINED", "check_claim_id_or_historical_state");
    }

    fn record_untracked_award(&mut self) {
        use ClaimStage::*;
        for stage in [Ticket, RightConsumption, AwardBlock, Moe2Settlement] {
            self.record(stage, ClaimDisposition::NotRecorded, "PALW.AWARD_STAGE_NOT_OBSERVED_FOR_PROFILE", "inspect_profile_owner");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_failures_are_bounded_and_pinned_to_the_same_parent() {
        let claim = Hash64::from_u64_word(1);
        let parent = Hash64::from_u64_word(2);
        let mut failures = ClaimAttemptFailureCache::default();
        let failure = ClaimAttemptFailure {
            claim,
            selected_parent: parent,
            candidate: Hash64::from_u64_word(3),
            daa: 42,
            stage: ClaimStage::RightConsumption,
            disposition: ClaimDisposition::Rejected,
            code: "PALW.MOE2_RESERVATION_MISSING",
            next: "check_candidate_reservation",
            repetitions: 1,
        };
        failures.record(failure);
        failures.record(ClaimAttemptFailure { candidate: Hash64::from_u64_word(4), ..failure });
        assert_eq!(failures.current(claim, parent).unwrap().repetitions, 2);
        assert!(failures.current(claim, Hash64::from_u64_word(99)).is_none());
        for n in 100..228 {
            failures.record(ClaimAttemptFailure { claim: Hash64::from_u64_word(n), ..failure });
        }
        assert!(failures.current(claim, parent).is_none());
        assert_eq!(failures.rows.len(), 128);
    }
}
