//! Node-policy plans for the additional Positive Court path on the G14 basis.
//! These are private work plans, not native carrier objects, signatures or court
//! verdicts. Actual progress comes from the Node's ExactCase, not a Panel ACK.
use super::{KernelGoalV1, kernel_goal_of_finding_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::{
        ClaimStatusV2, ExactCaseV2, MinerResultV2, WitnessStatusV2,
        gate::{AdmissionScopeV2, ClaimFormV2},
    },
};
use misaka_palw_kernel::ledger::OutsiderFindingV1;

pub mod sources;
pub mod submission;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositiveGoalV2 {
    ReexecuteAndBuild,
    /// The legacy finding may concern a trace signed only by the Panel in the
    /// new format. Recompute and compare with the Miner's signed RESULT instead
    /// of forwarding a legacy trace conviction against that Miner.
    ReexecuteForResultComparison,
    /// The role adapter must use the bounded source ledger. Old stage/position
    /// demands are not an entitlement to request a full trace from the Miner.
    AcquireBoundedSources,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum G14GoalV2 {
    Legacy(Option<KernelGoalV1>),
    Positive(PositiveGoalV2),
}

/// Callers select `form` from their ADMITTED ruleset. Existing fraud pursuit is
/// preserved exactly; a local Clean finding merely starts Positive construction.
pub fn kernel_goal_with_positive_v2(form: ClaimFormV2, finding: OutsiderFindingV1) -> G14GoalV2 {
    if form == ClaimFormV2::LegacyTraceV1 {
        return G14GoalV2::Legacy(kernel_goal_of_finding_v1(finding));
    }
    G14GoalV2::Positive(match finding {
        OutsiderFindingV1::Clean => PositiveGoalV2::ReexecuteAndBuild,
        OutsiderFindingV1::Prosecute(_) => PositiveGoalV2::ReexecuteForResultComparison,
        OutsiderFindingV1::Demand(_) => PositiveGoalV2::AcquireBoundedSources,
    })
}

/// Local replay and construction progress, not Node validation status. A
/// recorded replay is reusable for both positive and negative evidence, so a
/// retry of witness assembly does not request another normal inference run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelBuildStageV2 {
    NoReplay,
    ReplayRecorded { capture: Hash64 },
    WitnessSigned { witness: Hash64, parts: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositivePursuitStepV2 {
    ReplayOnce,
    BuildFromRecorded {
        capture: Hash64,
    },
    CommitWitness {
        witness: Hash64,
    },
    SubmitParts {
        witness: Hash64,
        first: u64,
        count: u64,
    },
    FinishNodeCheck,
    /// This is a computation outcome only; it carries NO native credit permit.
    NodeCompleted {
        outcome: ClaimStatusV2,
    },
    WitnessRejected,
    EndedUnverified,
    RevalidateBranch,
    Refused {
        why: &'static str,
    },
}

/// Derive a resumable bounded submission from the exact Node cursor. Different
/// relays/Panel keys do not restart that cursor. On restart the caller first
/// reconstructs the Node case by replaying its authenticated journal; a local
/// serialized `Passed` or `last_sent` field is never accepted here.
#[allow(clippy::too_many_arguments)]
pub fn plan_positive_pursuit_v2(
    admission: &AdmissionScopeV2,
    result: &MinerResultV2,
    current_branch: Hash64,
    stage: PanelBuildStageV2,
    node: Option<&ExactCaseV2>,
    max_parts_per_send: u64,
    now: u64,
    deadline: u64,
) -> PositivePursuitStepV2 {
    use PositivePursuitStepV2 as S;
    if admission.check_result(result).is_err() {
        return S::Refused { why: "result is outside admitted ruleset/scope" };
    }
    if result.version != 2 || !result.sampling.is_greedy() {
        return S::Refused { why: "unsupported result version or sampling" };
    }
    if current_branch != admission.branch {
        return S::RevalidateBranch;
    }
    let Some(node) = node else {
        if now >= deadline {
            return S::EndedUnverified;
        }
        if max_parts_per_send == 0 {
            return S::Refused { why: "zero submission allowance" };
        }
        return match stage {
            PanelBuildStageV2::NoReplay => S::ReplayOnce,
            PanelBuildStageV2::ReplayRecorded { capture } => S::BuildFromRecorded { capture },
            PanelBuildStageV2::WitnessSigned { witness, parts } if parts > 0 => S::CommitWitness { witness },
            _ => S::Refused { why: "empty witness" },
        };
    };
    if node.result().id() != result.id() {
        return S::Refused { why: "Node is checking another signed result" };
    }
    match node.statuses() {
        (ClaimStatusV2::Revoked, _) => return S::RevalidateBranch,
        (ClaimStatusV2::EndedUnverified, _) => return S::EndedUnverified,
        (ClaimStatusV2::Passed | ClaimStatusV2::Refuted, WitnessStatusV2::Complete) => {
            return S::NodeCompleted { outcome: node.statuses().0 };
        }
        (_, WitnessStatusV2::Rejected) => return S::WitnessRejected,
        (_, WitnessStatusV2::Incomplete) => return S::EndedUnverified,
        _ => {}
    }
    if now >= deadline {
        return S::EndedUnverified;
    }
    if max_parts_per_send == 0 {
        return S::Refused { why: "zero submission allowance" };
    }
    let PanelBuildStageV2::WitnessSigned { witness, parts } = stage else {
        return S::Refused { why: "reconstruct the admitted signed witness before resuming" };
    };
    if witness != node.witness().id() || parts != node.witness().binding.step_leaf_count {
        return S::Refused { why: "local witness is not the Node's canonical candidate" };
    }
    let Some(remaining) = parts.checked_sub(node.cursor()) else {
        return S::Refused { why: "invalid Node cursor" };
    };
    if remaining == 0 {
        S::FinishNodeCheck
    } else {
        S::SubmitParts { witness, first: node.cursor(), count: remaining.min(max_parts_per_send) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_finding_adds_work_and_never_means_node_passed() {
        assert_eq!(kernel_goal_with_positive_v2(ClaimFormV2::LegacyTraceV1, OutsiderFindingV1::Clean), G14GoalV2::Legacy(None));
        assert_eq!(
            kernel_goal_with_positive_v2(ClaimFormV2::MinerResultV2, OutsiderFindingV1::Clean),
            G14GoalV2::Positive(PositiveGoalV2::ReexecuteAndBuild)
        );
    }
    #[test]
    fn old_position_demands_do_not_become_new_miner_trace_duties() {
        let positions = vec![(1, 2), (3, 4)];
        assert_eq!(
            kernel_goal_with_positive_v2(ClaimFormV2::LegacyTraceV1, OutsiderFindingV1::Demand(positions.clone())),
            G14GoalV2::Legacy(Some(KernelGoalV1::Demand(positions.clone())))
        );
        assert_eq!(
            kernel_goal_with_positive_v2(ClaimFormV2::MinerResultV2, OutsiderFindingV1::Demand(positions)),
            G14GoalV2::Positive(PositiveGoalV2::AcquireBoundedSources)
        );
    }

    #[test]
    fn legacy_trace_prosecution_is_not_a_new_result_conviction() {
        let proof = misaka_palw_kernel::ledger::ProsecutionV1::Segmented(vec![1, 2, 3]);
        assert_eq!(
            kernel_goal_with_positive_v2(ClaimFormV2::LegacyTraceV1, OutsiderFindingV1::Prosecute(proof.clone())),
            G14GoalV2::Legacy(Some(KernelGoalV1::Prove(proof.clone())))
        );
        assert_eq!(
            kernel_goal_with_positive_v2(ClaimFormV2::MinerResultV2, OutsiderFindingV1::Prosecute(proof)),
            G14GoalV2::Positive(PositiveGoalV2::ReexecuteForResultComparison)
        );
    }
}
