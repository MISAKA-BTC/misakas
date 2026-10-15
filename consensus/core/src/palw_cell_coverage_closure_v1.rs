//! **R6-CELL round 4 (2026-10-15): the coverage closure** — the dormant fence `palw_cell_coverage_closure_v1`, `None` on every
//! preset and refused when armed by [`Params::validate_palw_cell_coverage_closure_v1`]. It pays and weights nothing, so it is NOT in
//! `PALW_N3_GATED_REWARD_FENCES_V1`.
//!
//! **What it closes.** The R6-CELL checker ([`crate::palw_cell_coverage_v1`]) found the legacy step court fail-OPEN on a state chunk
//! map it cannot enumerate: `palw_step_leg::checkpoint_fault` reads `palw_state_chunk_count_at_v1`'s `None` ("this crate cannot
//! enumerate that map") as NO FAULT, and a version-1/2 profile may register ANY map id (`validate_shape` pins only version 3's). An
//! unknown map is therefore a class whose checkpoints no court can judge and whose disputes end in the producer's favour — a G14
//! soundness break. It also named four uncovered kinds ([`PalwCellUncoveredKindV1`]): the hybrid v1/v2 recurrence half, the solo
//! recurrence maps, multi-router tables and the fenced Kimi router.
//!
//! **The rule past the fence** (one predicate, [`palw_cell_closure_legacy_refusal_v1`]: the legacy checker answers `Err`):
//!
//! ```text
//! REQ-CELL-1  a ClassRegistered (tag 3) carrying a profile, folded at a DAA >= the fence, whose legacy coverage has any gap
//!             (an unknown map id, or any uncovered kind) is refused at registration (`object_preconditions_v1`; keyed on the
//!             class's registration DAA = the block's)
//! REQ-CELL-1b a claim ACCEPTED at a DAA >= the fence of a class whose graph the fold holds (`fp_work_profile_of`) and whose
//!             coverage has a gap is refused at the claim gate (`check_class_admits_claim`, code CELL_UNCOVERABLE) on every lane
//!             that gate serves (attempt, free-prompt, evaluation)
//! REQ-CELL-2  where admission cannot see the map (a class registered before the fence, or a genesis class, whose graph the fold
//!             does not hold), a court close of a claim accepted >= the fence whose binding (pinned to the claim's execution root
//!             and to the registered class) is uncoverable adjudicates ExecutorGuilty — objective, from the binding alone, never
//!             NoFaultFound / ChallengerDefeated (`adjudicate_court_close_v4`); the pure step-leg checker's closure form
//!             (`check_step_refutation_closure_capped_v1`) answers ShapeProfileNotCanonical for such a binding and never reads
//!             an unenumerable map's `None` as no fault
//! REQ-CELL-3  below the fence (every shipped network: the mirror is None) every predicate here answers "old rule": the same
//!             verdicts, writes and roots; a claim accepted before the fence keeps its old ruleset to its end
//! ```
//!
//! **Every uncovered kind is REFUSED AT ADMISSION, none covered by a new court path in this round** — each needs a verifier the court
//! does not have (see each [`PalwCellUncoveredKindV1`] variant): a class that wants the recurrence covered registers the v3/v4/v5
//! hybrid composition (which the court enumerates); a multi-router or Kimi-routed table has no unique router slot for a routed product.
//! TIR classes are outside this fence: their coverage is per descriptor (`palw_tir_cell_coverage_v1`), and no shipped fixture has a gap.

use crate::config::params::{ForkActivation, Params};
use crate::palw_cell_coverage_v1::{PalwCellCoverageGapV1, PalwCellUncoveredKindV1, palw_legacy_cell_coverage_v1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::PalwStateParamsV2;
use crate::palw_step::PalwShapeProfileV3;

/// The refusal code the claim gate and the registration refusal report (`PalwStateV2Error::ClassNotRewardable` /
/// `PalwStateV2Error::CellCoverageRefused`).
pub const PALW_CELL_UNCOVERABLE_CODE_V1: &str = "CELL_UNCOVERABLE";

/// **Why a legacy class is not coverable** past the fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellClosureRefusalV1 {
    /// A boundary the checker enumerated and found no verifier for.
    Uncovered(PalwCellUncoveredKindV1),
    /// A profile kind the checker does not know — an unknown state chunk map id, an unknown profile version: fail closed.
    UnknownProfileKind,
}

impl PalwCellClosureRefusalV1 {
    /// The refusal, by name (stable text; the fold's error carries it).
    pub fn why(self) -> &'static str {
        use PalwCellUncoveredKindV1 as K;
        match self {
            Self::UnknownProfileKind => "a state chunk map id or profile kind the court does not know: no court can enumerate it",
            Self::Uncovered(K::HybridRecurrenceUnenumerated) => {
                "a hybrid v1/v2 map: its recurrence half is not enumerated by the court (register the v3/v4/v5 composition)"
            }
            Self::Uncovered(K::SoloRecurrenceMap) => "a standalone recurrence map: its geometry is the executor's, not the court's",
            Self::Uncovered(K::MapSectionNotSpelled) => "the registered map does not name a memory section this profile needs",
            Self::Uncovered(K::MapNotEnumerable) => "the registered map's chunk count is not derivable at a checkpoint of this profile",
            Self::Uncovered(K::MultiRouterTable) => "more than one router in a layer table: routed products are unadjudicable",
            Self::Uncovered(K::KimiRouter) => "a Kimi router: its routed products resolve no router slot and are unadjudicable",
            Self::Uncovered(K::TirFamilyUnchecked | K::TirCarryNotCommitted | K::TirStateUnchecked) => {
                "an IR boundary without a verifier"
            }
        }
    }
}

/// **The one predicate**: `None` exactly when [`palw_legacy_cell_coverage_v1`] covers every boundary of `profile`; otherwise the
/// refusal — an unknown kind first (fail closed by name), else the first uncovered boundary's kind (the checker's deterministic
/// order: layers ascending, then memory sections).
pub fn palw_cell_closure_legacy_refusal_v1(profile: &PalwShapeProfileV3) -> Option<PalwCellClosureRefusalV1> {
    let gaps = palw_legacy_cell_coverage_v1(profile).err()?;
    if gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. })) {
        return Some(PalwCellClosureRefusalV1::UnknownProfileKind);
    }
    gaps.iter()
        .find_map(|g| match g {
            PalwCellCoverageGapV1::Uncovered { kind, .. } => Some(PalwCellClosureRefusalV1::Uncovered(*kind)),
            PalwCellCoverageGapV1::UnknownProfileKind { .. } => None,
        })
        // An `Err` with no gap is not a shape the checker produces; fail closed rather than read it as covered.
        .or(Some(PalwCellClosureRefusalV1::UnknownProfileKind))
}

/// **Is the closure in force for something dated `daa`** (a registration's block, a claim's acceptance)? `false` on every shipped
/// network (the mirror is `None`).
pub fn palw_cell_closure_governs_v1(params: &PalwStateParamsV2, daa: u64) -> bool {
    params.cell_coverage_closure_from_daa().is_some_and(|from| daa >= from)
}

/// **REQ-CELL-1: may a carried profile register at `daa_score`?** `None` below the fence, or for a covered profile.
pub fn palw_cell_closure_registration_refusal_v1(
    params: &PalwStateParamsV2,
    daa_score: u64,
    profile: &PalwShapeProfileV3,
) -> Option<PalwCellClosureRefusalV1> {
    if !palw_cell_closure_governs_v1(params, daa_score) {
        return None;
    }
    palw_cell_closure_legacy_refusal_v1(profile)
}

/// **REQ-CELL-1b: the claim gate's question** for a claim accepted at `accepted_daa` of a class whose graph the fold holds
/// (`held_profile`, the fold's `fp_work_profile_of`). `None` below the fence, for a covered class, and — where the fold does not
/// hold the graph — `None` here: that claim is the court's (REQ-CELL-2), which convicts rather than acquits.
pub fn palw_cell_closure_claim_refusal_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    held_profile: Option<&PalwShapeProfileV3>,
) -> Option<PalwCellClosureRefusalV1> {
    if !palw_cell_closure_governs_v1(params, accepted_daa) {
        return None;
    }
    palw_cell_closure_legacy_refusal_v1(held_profile?)
}

/// **REQ-CELL-2: does the court convict on the closure alone?** For a claim accepted at `accepted_daa` under a closure in force from
/// `cell_closure_from_daa`, whose binding's (registered) profile is `profile`: `true` exactly when the profile is uncoverable. The
/// caller has pinned the binding to the claim's execution root and to the registered class first.
pub fn palw_cell_closure_court_convicts_v1(
    cell_closure_from_daa: Option<u64>,
    accepted_daa: u64,
    profile: &PalwShapeProfileV3,
) -> bool {
    cell_closure_from_daa.is_some_and(|from| accepted_daa >= from) && palw_cell_closure_legacy_refusal_v1(profile).is_some()
}

impl Params {
    /// `palw_cell_coverage_closure_v1`'s height, `Some` only for a real one (`never()` is `None`).
    pub fn palw_cell_coverage_closure_v1_from_daa(&self) -> Option<u64> {
        self.palw_cell_coverage_closure_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// **The closure's mirror** on the V2 bundle's state params (`cell_coverage_closure_from_daa`), which the fold and the court
    /// read. Written here and nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_cell_coverage_closure_v1(&mut self) {
        let from = self.palw_cell_coverage_closure_v1_from_daa();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_cell_coverage_closure_from_daa(from);
        }
    }

    /// **Refused when armed at a real height** until its rule is verified (2026-10-15: written, not run); `None` and `Some(never())`
    /// pass. The mirror must agree with the fence.
    pub fn validate_palw_cell_coverage_closure_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.palw_cell_coverage_closure_v1_from_daa().is_some() {
            return Err(PalwModeV2Error::Invalid(
                "palw_cell_coverage_closure_v1 cannot be armed: its rule (RFC-0006 R6-CELL round 4) is not built and verified yet",
            ));
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.cell_coverage_closure_from_daa() != self.palw_cell_coverage_closure_v1_from_daa()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_cell_coverage_closure_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_cell_coverage_closure_v1",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
