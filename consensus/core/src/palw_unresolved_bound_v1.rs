//! **RFC-0023 / ADR-0186 stage 1 (user decision 2026-10-17): a slow claim A holds the shared frontier at most to its own liability
//! bound, and keeps its liability after it.** Fence `palw_unresolved_bound_v1` — `None` on every preset, refused when armed (no
//! measurement, drill or activation decision yet); candidate for the next fence (testnet-12 DAA 16,000 or so).
//!
//! The frontier formula is unchanged (F02): the deepest frontier-eligible `Final` inside the resolved prefix, never below the floor.
//! What stage 1 changes is only which open claims END the resolved prefix:
//!
//! ```text
//! bound(c)        = c.trace_retention_daa — fixed at acceptance (the producer's promise, the court-demanding retention past
//!                   palw_court_capacity_v1: 4W + W_disclose from the licence, plus Gate C's certification extension). New
//!                   accusations never restart it (the hard deadline is a function of the claim alone). L_class = bound − accepted.
//! guaranteed      every DA session and court row the claim can be served by is admitted no later than bound − W_disclose and,
//!                   by the court capacity's bounded admission + EDF reserve, adjudicated by then (11.1); a claim whose class
//!                   cannot promise it is not reward-bearing (CourtClaimAdmissionFull / COURT_RETENTION_BELOW_GUARANTEE).
//! resolved_through = (accepted blue score of the oldest open claim still within its bound) − 1
//! past its bound  an open claim no longer ends the prefix. That is NOT proof of fraud: nothing is slashed, its reservation,
//!                   collateral, disputes and retained evidence are untouched (the claim stays open in `unresolved`, so pruning's
//!                   ceiling still stops below it), and it never earns consensus credit: a claim whose `Final` comes after its
//!                   bound is not frontier-eligible and its certification is withdrawn (no credit, its held reward never named).
//! floor           a retirement raises the floor only through the resolved prefix; a higher retirement is deferred (one row)
//!                   and applied once the prefix reaches it — the floor never passes an open claim inside its bound.
//! ```
//!
//! Below the fence and on every shipped network the rule is byte-identical (the helpers below return the old answer). No
//! cryptographic computation proof is used (PRINCIPLES §8).
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwClaimStateV2, PalwStateParamsV2};

/// **A claim's unresolved bound** (`accepted DAA + L_class`): its committed trace retention.
pub fn palw_unresolved_bound_daa_v1(claim: &PalwClaimStateV2) -> u64 {
    claim.trace_retention_daa
}

/// Does stage 1 govern `claim` — the fence in force at its acceptance?
pub fn palw_unresolved_bound_governs_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    params.unresolved_bound_from_daa().is_some_and(|from| claim.accepted_daa >= from)
}

/// **A governed claim that lapsed its bound**: still open past it, or `Final` only after it. It never earns consensus credit.
pub fn palw_claim_lapsed_bound_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, now_daa: u64) -> bool {
    if !palw_unresolved_bound_governs_v1(params, claim) {
        return false;
    }
    let bound = palw_unresolved_bound_daa_v1(claim);
    match &claim.phase {
        PalwClaimPhaseV2::Final { final_daa } => *final_daa > bound,
        PalwClaimPhaseV2::Voided { .. } => false,
        _ => now_daa > bound,
    }
}

/// **The end of the resolved prefix at `now_daa`** (F02's `resolved_through`). Below the fence (or for claims accepted below it):
/// the oldest open claim ends it. Past it: the oldest open claim still within its bound.
pub fn palw_resolved_through_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, now_daa: u64) -> u64 {
    for (blue_score, key) in state.unresolved_iter() {
        let lapsed = state.claim(key).is_some_and(|claim| palw_claim_lapsed_bound_v1(params, claim, now_daa));
        if !lapsed {
            return blue_score.saturating_sub(1);
        }
    }
    u64::MAX
}

/// The steady-state lag a single open claim can impose on the frontier past the fence: `bound − accepted` (L_class), in DAA.
pub fn palw_unresolved_lag_bound_daa_v1(claim: &PalwClaimStateV2) -> u64 {
    palw_unresolved_bound_daa_v1(claim).saturating_sub(claim.accepted_daa)
}

/// Evidence needed to arm RFC-0023 stage 1 on one exact assembled ruleset.
/// Digests name executed artifacts and independent review, never estimates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwUnresolvedBoundRecordV1 {
    pub network_domain: [u8; 64],
    pub policy_root: [u8; 64],
    pub activation_daa: u64,
    pub lag_measurement: [u8; 32],
    pub node_crossing_restart_replay: [u8; 32],
    pub review: [u8; 32],
}

// No record is fabricated from the activation request. The release coordinator
// pins evidence only after the matching harness and independent review finish.
pub const PALW_UNRESOLVED_BOUND_RECORDS_V1: &[PalwUnresolvedBoundRecordV1] = &[];

impl Params {
    pub fn palw_unresolved_bound_policy_root_v1(&self) -> Option<[u8; 64]> {
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return None };
        Some(misaka_palw_kernel::hash::object_id(b"misaka-palw/rfc23/stage1-policy/v1", &bundle.state))
    }

    pub fn validate_palw_unresolved_bound_with_records_v1(
        &self,
        records: &[PalwUnresolvedBoundRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_unresolved_bound_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        let at = at.daa_score();
        let network_domain = self.palw_rfc0608_network_domain_v1();
        let policy_root = self.palw_unresolved_bound_policy_root_v1();
        if !records.iter().any(|r| {
            r.network_domain == network_domain && Some(r.policy_root) == policy_root && r.activation_daa == at
                && r.lag_measurement != [0; 32] && r.node_crossing_restart_replay != [0; 32] && r.review != [0; 32]
                && r.lag_measurement != r.node_crossing_restart_replay
                && r.review != r.lag_measurement && r.review != r.node_crossing_restart_replay
        }) {
            return Err(PalwModeV2Error::Invalid(
                "palw_unresolved_bound_v1 cannot be armed: no pinned steady-state lag measurement, node crossing/restart/replay and independent review for this exact network, height and state policy",
            ));
        }
        Ok(())
    }
}

impl Params {
    /// Mirror `palw_unresolved_bound_v1` into the V2 bundle's state params.
    pub fn sync_palw_unresolved_bound_v1(&mut self) {
        let from = self.palw_unresolved_bound_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_unresolved_bound_from_daa(from);
        }
    }

    /// **The fence's refusals**: the mirror agrees, and no ruleset arms it (stage 1 has no measured `L_class` steady-state lag, no
    /// fence-crossing drill and no activation decision).
    pub fn validate_palw_unresolved_bound_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.unresolved_bound_from_daa(),
            _ => None,
        };
        let armed = self.palw_unresolved_bound_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_unresolved_bound_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_unresolved_bound_v1",
            ));
        }
        self.validate_palw_unresolved_bound_with_records_v1(PALW_UNRESOLVED_BOUND_RECORDS_V1)
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;
    #[test]
    fn stage1_record_binds_network_height_policy_and_distinct_executed_evidence() {
        let mut p = crate::config::params::palw_t12_shipped_params();
        p.palw_unresolved_bound_v1 = Some(ForkActivation::new(17_400));
        p.sync_palw_unresolved_bound_v1();
        assert!(p.validate_palw_unresolved_bound_v1().is_err());
        let record = PalwUnresolvedBoundRecordV1 {
            network_domain: p.palw_rfc0608_network_domain_v1(),
            policy_root: p.palw_unresolved_bound_policy_root_v1().unwrap(),
            activation_daa: 17_400,
            lag_measurement: [1; 32],
            node_crossing_restart_replay: [2; 32],
            review: [3; 32],
        };
        // These are test-only gate inputs, never shipping evidence.
        p.validate_palw_unresolved_bound_with_records_v1(&[record]).unwrap();
        for bad in [
            PalwUnresolvedBoundRecordV1 { network_domain: [0; 64], ..record },
            PalwUnresolvedBoundRecordV1 { policy_root: [0; 64], ..record },
            PalwUnresolvedBoundRecordV1 { activation_daa: 17_100, ..record },
            PalwUnresolvedBoundRecordV1 { lag_measurement: [0; 32], ..record },
            PalwUnresolvedBoundRecordV1 { node_crossing_restart_replay: [1; 32], ..record },
            PalwUnresolvedBoundRecordV1 { review: [2; 32], ..record },
        ] {
            assert!(p.validate_palw_unresolved_bound_with_records_v1(&[bad]).is_err());
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
            let changed = bundle.state.claim_retirement_daa().checked_add(1).unwrap();
            bundle.state = bundle.state.clone().with_claim_retirement_daa(changed).unwrap();
        }
        assert!(p.validate_palw_unresolved_bound_with_records_v1(&[record]).is_err());
    }
}
