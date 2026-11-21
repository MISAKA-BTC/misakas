//! RFC-0065 local decision functions. Inputs are verdicts from the owning
//! canonical evidence reducers. This module assigns no control wire tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Unassessed,
    Checking,
    ReviewHold,
    Rejected,
    RejectedUnresolved,
    Passed,
}

pub struct Assessment {
    pub execution: Verdict,
    pub collision: Verdict,
    pub cost: Verdict,
    pub usefulness: Verdict,
    pub structure: Verdict,
    pub current_rules_and_evidence: Verdict,
}

impl Default for Assessment {
    /// Registration provides identity and a market, not any of the six verdicts.
    fn default() -> Self {
        Self {
            execution: Verdict::Unknown,
            collision: Verdict::Unknown,
            cost: Verdict::Unknown,
            usefulness: Verdict::Unknown,
            structure: Verdict::Unknown,
            current_rules_and_evidence: Verdict::Unknown,
        }
    }
}

impl Assessment {
    pub fn decision(&self) -> Admission {
        let gates = [self.execution, self.collision, self.cost, self.usefulness, self.structure, self.current_rules_and_evidence];
        if gates.contains(&Verdict::Fail) {
            Admission::Rejected
        } else if gates.contains(&Verdict::Unknown) {
            Admission::ReviewHold
        } else {
            Admission::Passed
        }
    }
}

impl Admission {
    /// Apply the original unresolved risk-case deadline. Other states and
    /// previously vested effects do not acquire an expiry from this event.
    pub fn risk_deadline(self, now: u64, deadline: u64) -> Self {
        if self == Self::ReviewHold && now >= deadline { Self::RejectedUnresolved } else { self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    AdmissionHold,
    StructuralFailure,
    BftSafetyStop,
    RuleDisabled,
    BurnPause,
}

/// Applicable stops are the full set for this artifact/profile and rule version
/// after the canonical batch has applied its controls. This predicate cannot
/// authenticate a stop, release one, or modify registration, markets or old claims.
pub fn new_mining_allowed(
    activated: bool,
    admission: Admission,
    evidence: &Assessment,
    stops: &[StopReason],
    claim_gates: bool,
) -> bool {
    activated && admission == Admission::Passed && evidence.decision() == Admission::Passed && stops.is_empty() && claim_gates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mas1_new_mining_checks_every_gate_and_keeps_timeout_closed() {
        let mut a = Assessment {
            execution: Verdict::Pass,
            collision: Verdict::Pass,
            cost: Verdict::Pass,
            usefulness: Verdict::Pass,
            structure: Verdict::Pass,
            current_rules_and_evidence: Verdict::Pass,
        };
        assert!(new_mining_allowed(true, a.decision(), &a, &[], true));
        for stop in [
            StopReason::AdmissionHold,
            StopReason::StructuralFailure,
            StopReason::BftSafetyStop,
            StopReason::RuleDisabled,
            StopReason::BurnPause,
        ] {
            assert!(!new_mining_allowed(true, Admission::Passed, &a, &[stop], true));
        }
        assert!(!new_mining_allowed(false, Admission::Passed, &a, &[], true));
        assert!(!new_mining_allowed(true, Admission::Passed, &a, &[], false));
        a.cost = Verdict::Unknown;
        assert!(!new_mining_allowed(true, Admission::Passed, &a, &[], true));
        assert_eq!(a.decision().risk_deadline(9, 10), Admission::ReviewHold);
        assert_eq!(a.decision().risk_deadline(10, 10), Admission::RejectedUnresolved);
        a.structure = Verdict::Fail;
        assert_eq!(a.decision(), Admission::Rejected);
    }
}
