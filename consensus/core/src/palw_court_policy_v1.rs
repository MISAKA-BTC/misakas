//! RFC-0026 §2–3: checked derivation from ORIGINAL accepting-chain DAA.
//! No wire/state format, fence, preset or existing claim is changed here.
//! Fold integration requires a separately versioned, reviewed ruleset. In
//! particular, this helper does not choose notice equality or turn durations.

pub const NOTICE_WINDOW_DAA: u64 = 3_120;
pub const DISPUTE_WINDOW_DAA: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtPolicyError {
    Overflow,
    DependencyTooShort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtClockBounds {
    pub notice_boundary: u64,
    pub hard_deadline: u64,
    pub material_until: u64,
    pub bond_exit_after: u64,
}

impl CourtClockBounds {
    /// Derive the entire candidate before its caller writes any state. The
    /// dependency horizon is the minimum of ALL signed dependency contracts;
    /// `other_bond_liability` is the maximum of all other obligations on this
    /// bond, including Gate C, post-Final accusation and monetary material.
    /// This does not rewrite signed source frames or release collateral.
    pub fn derive(
        original_acceptance: u64,
        reorg_retention: u64,
        required_followup: u64,
        shortest_signed_dependency: u64,
        other_bond_liability: u64,
    ) -> Result<Self, CourtPolicyError> {
        let add = |a: u64, b| a.checked_add(b).ok_or(CourtPolicyError::Overflow);
        let notice_boundary = add(original_acceptance, NOTICE_WINDOW_DAA)?;
        let hard_deadline = add(original_acceptance, DISPUTE_WINDOW_DAA)?;
        let dependency_until = add(hard_deadline, required_followup)?;
        if shortest_signed_dependency < dependency_until {
            return Err(CourtPolicyError::DependencyTooShort);
        }
        let material_until = add(hard_deadline, reorg_retention)?;
        Ok(Self {
            notice_boundary,
            hard_deadline,
            material_until,
            bond_exit_after: material_until.max(dependency_until).max(other_bond_liability),
        })
    }
}
