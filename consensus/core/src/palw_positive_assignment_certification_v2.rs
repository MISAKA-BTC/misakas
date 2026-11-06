//! Checked timing projection of the existing actual Gate C policy for RFC-0045.
//! This is not a P draw, approval, reservation or admission capability. In
//! particular the existing k-positive verdict waits for pending fraud without
//! applying its ordinary failure deadline. A finite no-fraud calendar alone
//! MUST NOT be used as an unconditional all-branch reservation horizon.
use crate::{
    config::params::Params,
    palw_security_certification_v1::{PalwCertificationNetworkRowV1, PalwSecurityCertificationPolicyV1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentGateCClockErrorV2 {
    MissingActualPolicy,
    InvalidPolicy,
    InvalidClockOrder,
    Overflow,
    UnresolvedPendingFraudOwner,
    UnboundedVerifierUniverse,
    VerifierRegistryWorkBound,
    InvalidRegistrationChronology,
    DrawEpochWorkBound,
    InconsistentDrawQueue,
}

/// Original Result/C acceptance is the commit clock, never Assignment intake.
/// Values are exact arithmetic, not saturating lifetime estimates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentGateCClockV2 {
    pub commit_daa: u64,
    pub anchor_delay_spans: u64,
    pub latest_draw_daa: u64,
    pub no_draw_expiry_daa: u64,
    pub latest_attestation_deadline_daa: u64,
    pub latest_failed_attestation_decision_daa: u64,
    prosecution_window_daa: u64,
    pub drawn_positions: u16,
    pub required_positives: u16,
    pub draws_per_epoch: u32,
    pub epoch_daa: u64,
}
impl AssignmentGateCClockV2 {
    pub fn resolve(params: &Params, policy_at_daa: u64, result_commit_daa: u64) -> Result<Self, AssignmentGateCClockErrorV2> {
        let mirror = params
            .palw_rfc0018_expected_mirror_v1()
            .filter(|mirror| mirror.gate_c_at(policy_at_daa))
            .ok_or(AssignmentGateCClockErrorV2::MissingActualPolicy)?;
        Self::from_existing_policy(&mirror.certification, result_commit_daa)
    }

    /// Pure timing, not authenticated policy selection. Native uses resolve.
    pub fn from_existing_policy(
        policy: &PalwSecurityCertificationPolicyV1,
        result_commit_daa: u64,
    ) -> Result<Self, AssignmentGateCClockErrorV2> {
        policy.validate().map_err(|_| AssignmentGateCClockErrorV2::InvalidPolicy)?;
        let add = |a: u64, b: u64| a.checked_add(b).ok_or(AssignmentGateCClockErrorV2::Overflow);
        let latest_draw_daa = add(result_commit_daa, policy.certify_by_daa)?;
        let latest_attestation_deadline_daa = add(latest_draw_daa, policy.attest_window_daa)?;
        Ok(Self {
            commit_daa: result_commit_daa,
            anchor_delay_spans: policy.anchor_delay_spans.max(1),
            latest_draw_daa,
            no_draw_expiry_daa: add(latest_draw_daa, 1)?,
            latest_attestation_deadline_daa,
            latest_failed_attestation_decision_daa: add(latest_attestation_deadline_daa, 1)?,
            prosecution_window_daa: policy.prosecution_window_daa,
            drawn_positions: policy.m,
            required_positives: policy.k,
            draws_per_epoch: policy.draws_per_epoch,
            epoch_daa: policy.epoch_daa,
        })
    }

    /// Existing Gate C measures prosecution from actual Final, independently
    /// of Result commit/draw. No value is invented before Final is known.
    pub fn prosecution_not_before_daa(&self, actual_final_daa: u64) -> Result<u64, AssignmentGateCClockErrorV2> {
        if actual_final_daa < self.commit_daa {
            return Err(AssignmentGateCClockErrorV2::InvalidClockOrder);
        }
        actual_final_daa.checked_add(self.prosecution_window_daa).ok_or(AssignmentGateCClockErrorV2::Overflow)
    }

    /// No caller integer, Assignment TTL or saturating policy lifetime can
    /// stand in for the real pending-fraud/resource owner. The native complete
    /// owner must derive and reserve that branch before enabling admission.
    pub fn require_complete_native_bound(&self) -> Result<std::convert::Infallible, AssignmentGateCClockErrorV2> {
        Err(AssignmentGateCClockErrorV2::UnresolvedPendingFraudOwner)
    }
}

/// The inclusive epoch interval for a prospective Assignment draw. The
/// accepting DAA is the lower bound because the later-seed requirement is
/// defined in execution spans, not as a fixed additional DAA interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentGateCDrawEpochsV2 {
    first: u64,
    last: u64,
}
impl AssignmentGateCDrawEpochsV2 {
    pub fn from_result_window(
        policy: &PalwSecurityCertificationPolicyV1,
        intake_daa: u64,
        result_before_daa: u64,
        max_epochs: u64,
    ) -> Result<Self, AssignmentGateCClockErrorV2> {
        use AssignmentGateCClockErrorV2 as E;
        if intake_daa >= result_before_daa || result_before_daa == u64::MAX {
            return Err(E::InvalidClockOrder);
        }
        let latest = AssignmentGateCClockV2::from_existing_policy(policy, result_before_daa - 1)?;
        let first = intake_daa / latest.epoch_daa;
        let last = latest.latest_draw_daa / latest.epoch_daa;
        let count = last.checked_sub(first).and_then(|n| n.checked_add(1)).ok_or(E::Overflow)?;
        if count > max_epochs {
            return Err(E::DrawEpochWorkBound);
        }
        Ok(Self { first, last })
    }

    pub fn epochs(self) -> std::ops::RangeInclusive<u64> {
        self.first..=self.last
    }

    pub fn epoch_count(self) -> u64 {
        // Construction checks this arithmetic before returning the interval.
        self.last - self.first + 1
    }
}

/// Project the real shared N row at an independently chosen epoch. Expanding
/// to a later epoch resets the draw count. A future row or an overfull current
/// row cannot be read as an empty capacity owner.
pub fn assignment_gate_c_draws_used_v2(
    network: &PalwCertificationNetworkRowV1,
    epoch: u64,
    draws_per_epoch: u32,
) -> Result<u32, AssignmentGateCClockErrorV2> {
    use AssignmentGateCClockErrorV2 as E;
    if draws_per_epoch == 0 {
        return Err(E::InvalidPolicy);
    }
    if network.epoch > epoch || (network.epoch == epoch && network.draws > draws_per_epoch) {
        return Err(E::InconsistentDrawQueue);
    }
    Ok(if network.epoch == epoch { network.draws } else { 0 })
}

/// Pure DATA bound for the global registry work needed by ANY legal later C.
/// It neither freezes the actual draw pool nor authorizes an anchor snapshot.
/// Native must independently resolve the signed anchor's real ancestor state,
/// audit its registration chronology, price all its candidate records and own
/// the query/index/build/retention capacity. An at-intake eligible count does
/// not bound future eligibility and is deliberately not an input here.
///
/// `None` means no registration DAA can mature before the exclusive Result
/// cutoff. `Some(cutoff)` means all possibly eligible registrations precede the
/// anchor DAA strictly. The strict inequality covers later accepting blocks
/// whose DAA equals the anchor's. Future active status/stake/exclusion and the
/// actual randomly drawn approval remain the unchanged Gate C predicates.
pub fn assignment_verifier_registration_cutoff_v2(
    policy: &PalwSecurityCertificationPolicyV1,
    anchor_daa: u64,
    result_before_daa: u64,
) -> Result<Option<u64>, AssignmentGateCClockErrorV2> {
    policy.validate().map_err(|_| AssignmentGateCClockErrorV2::InvalidPolicy)?;
    if result_before_daa == u64::MAX || result_before_daa <= anchor_daa {
        return Err(AssignmentGateCClockErrorV2::InvalidClockOrder);
    }
    let latest_commit = result_before_daa.checked_sub(1).ok_or(AssignmentGateCClockErrorV2::InvalidClockOrder)?;
    let Some(cutoff) = latest_commit.checked_sub(policy.verifier_maturity_daa) else {
        return Ok(None);
    };
    if cutoff >= anchor_daa {
        return Err(AssignmentGateCClockErrorV2::UnboundedVerifierUniverse);
    }
    Ok(Some(cutoff))
}

/// Bound the future verifier count from the actual intake prefix. Removed bonds
/// cannot re-enter with an old registration date. New registrations use their
/// accepting DAA, so the strict ancestor cutoff excludes every later entrant,
/// including one in the same DAA. The native caller owns ancestry and the
/// registry's provenance. Eligibility is evaluated afresh at the actual draw.
pub fn assignment_verifier_registry_bound_v2(
    policy: &PalwSecurityCertificationPolicyV1,
    anchor_daa: u64,
    result_before_daa: u64,
    intake_daa: u64,
    maximum_scan_rows: u64,
    registrations: impl ExactSizeIterator<Item = u64>,
) -> Result<u64, AssignmentGateCClockErrorV2> {
    if anchor_daa > intake_daa || intake_daa >= result_before_daa {
        return Err(AssignmentGateCClockErrorV2::InvalidClockOrder);
    }
    let Some(cutoff) = assignment_verifier_registration_cutoff_v2(policy, anchor_daa, result_before_daa)? else {
        return Ok(0);
    };
    if registrations.len() as u64 > maximum_scan_rows {
        return Err(AssignmentGateCClockErrorV2::VerifierRegistryWorkBound);
    }
    let mut candidates = 0;
    for registered in registrations {
        if registered > intake_daa {
            return Err(AssignmentGateCClockErrorV2::InvalidRegistrationChronology);
        }
        if registered <= cutoff {
            candidates += 1;
        }
    }
    Ok(candidates)
}
