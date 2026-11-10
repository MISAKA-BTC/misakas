//! Checked remaining-capacity arithmetic for an already chosen usage projection.
//! The owner must derive reservations from actual duties, other roles, retention
//! horizons and service opportunities. These scalars create no lease, admission,
//! native policy, fee schedule or authority. They do not assign new wire fields
//! or choose how source and whole-Case counters map to native resource pools.
use crate::{ContractError, messages::AggregateBudget, rows::AggregateUsage};

/// Outstanding demand in the SAME units and pool as the supplied usage/budget.
/// Include every protected role; do not reset this demand on Panel replacement
/// or release retention merely because a source response completed. No Default
/// is supplied: the policy owner must explicitly resolve every dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemainingCapacity {
    pub transmitted_bytes: u64,
    pub additional_work: u64,
    pub retained_bytes: u64,
}
impl RemainingCapacity {
    pub fn checked_scale(self, count: u64) -> Result<Self, ContractError> {
        Ok(Self {
            transmitted_bytes: self.transmitted_bytes.checked_mul(count).ok_or(ContractError::Budget)?,
            additional_work: self.additional_work.checked_mul(count).ok_or(ContractError::Budget)?,
            retained_bytes: self.retained_bytes.checked_mul(count).ok_or(ContractError::Budget)?,
        })
    }

    pub fn checked_add(self, other: Self) -> Result<Self, ContractError> {
        Ok(Self {
            transmitted_bytes: self.transmitted_bytes.checked_add(other.transmitted_bytes).ok_or(ContractError::Budget)?,
            additional_work: self.additional_work.checked_add(other.additional_work).ok_or(ContractError::Budget)?,
            retained_bytes: self.retained_bytes.checked_add(other.retained_bytes).ok_or(ContractError::Budget)?,
        })
    }

    /// Check the post-transition consumed amount PLUS the outstanding demand.
    /// Reservation-to-consumption transfers are supplied together by the owner;
    /// this check neither charges the reservation again nor releases anything.
    pub fn check(&self, consumed: &AggregateUsage, budget: &AggregateBudget) -> Result<(), ContractError> {
        consumed.validate(budget)?;
        for (used, remaining, limit) in [
            (consumed.transmitted_bytes, self.transmitted_bytes, budget.transmitted_bytes),
            (consumed.additional_work, self.additional_work, budget.additional_work),
            (consumed.retained_bytes, self.retained_bytes, budget.retained_bytes),
        ] {
            if used.checked_add(remaining).is_none_or(|total| total > limit) {
                return Err(ContractError::Budget);
            }
        }
        Ok(())
    }
}
