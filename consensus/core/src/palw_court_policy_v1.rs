//! RFC-0026 §2–3: checked derivation from ORIGINAL accepting-chain DAA.
//! No wire/state format, fence, preset or existing claim is changed here.
//! Fold integration requires a separately versioned, reviewed ruleset. In
//! particular, legacy clocks/turn durations are unchanged. The separate shared
//! initial-notice DAA predicate follows the privately approved ADR-0198 A.

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
    /// Every dependency must also cover this claim's material retention. Other
    /// claims on the same bond extend collateral hold, not this dependency path.
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
        let material_until = add(hard_deadline, reorg_retention)?;
        let dependency_until = add(hard_deadline, required_followup)?.max(material_until);
        if shortest_signed_dependency < dependency_until {
            return Err(CourtPolicyError::DependencyTooShort);
        }
        Ok(Self {
            notice_boundary,
            hard_deadline,
            material_until,
            bond_exit_after: material_until.max(dependency_until).max(other_bond_liability),
        })
    }
}

/// ADR-0198 A: clock condition ONLY for a new shared initial notice. The native
/// coordinator must separately authenticate the actual original accepting point,
/// same-DAA causal ordering, signature, funds and all-branch reservation. This
/// does not create a notice, reset an original, extend hard or authorize default.
/// Both original boundaries must be representable; no saturating fallback.
pub fn shared_initial_notice_daa_in_window_v1(original_acceptance: u64, notice_daa: u64) -> Result<bool, CourtPolicyError> {
    let end = original_acceptance.checked_add(NOTICE_WINDOW_DAA).ok_or(CourtPolicyError::Overflow)?;
    original_acceptance.checked_add(DISPUTE_WINDOW_DAA).ok_or(CourtPolicyError::Overflow)?;
    Ok(original_acceptance <= notice_daa && notice_daa <= end)
}
