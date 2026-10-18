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
        if armed.is_some() {
            return Err(PalwModeV2Error::Invalid(
                "palw_unresolved_bound_v1 (RFC-0023 stage 1) cannot be armed: its steady-state lag is unmeasured, no drill crosses \
                 it and no activation decision exists",
            ));
        }
        Ok(())
    }
}
