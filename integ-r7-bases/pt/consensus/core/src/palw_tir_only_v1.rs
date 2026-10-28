//! **`palw_tir_only_v1` — RFC-0002 Phase H, the mainnet step: the IR is the admission path for new classes.** A bare height,
//! dormant on every preset (testnet-12 included) and in no flag-day list: **not armed anywhere**. Past it a post-genesis registration
//! of a legacy-family class (`ClassRegistered` carrying an admission carriage: a `PalwShapeProfileV3` of the hand-written kernels) is
//! refused by name — a new model is registered as an IR program (RFC-0002) or an IR pipeline (RFC-0003) or not at all. The genesis
//! registrations (no carriage: the card's classes) and every class already on the chain keep their rules: the fence closes a door, it
//! removes nothing.
//!
//! Hashed Some-only in both fingerprints, the activation visited by `for_each_fence`, the `Some(never())` value collapsed — the shape
//! of every dormant fence here; mirrored on the V2 bundle's state params (`tir_only_from_daa`) because the fold holds only the bundle.
//! [`Params::validate_palw_tir_only_v1`] refuses arming on a ruleset that is not `ConsensusV2` and without `palw_tir_v1` in force at or
//! below it (a network that closes the legacy door must have opened the IR one).

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a flag day (or a drill) arms the fence with**, through its own `set`, which writes the bundle's mirror. In NO flag-day
/// list: Phase H is a mainnet step the user decides, and this build does not arm it anywhere.
pub const PALW_TIR_ONLY_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_tir_only_v1",
    set: |params, at| {
        params.palw_tir_only_v1 = at;
        params.sync_palw_tir_only_v1();
    },
};

/// The drill's one-entry list (`--palw-drill-tir-only-at`).
pub const PALW_DRILL_TIR_ONLY_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_TIR_ONLY_ENTRY_V1];

impl Params {
    /// `palw_tir_only_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real height.
    pub fn palw_tir_only_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_tir_only_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is the IR the only admission path for a new class at `daa_score`?** `false` on every shipped preset.
    pub fn palw_tir_only_active_at(&self, daa_score: u64) -> bool {
        self.palw_tir_only_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// The fence's mirror on the V2 bundle's state params. Call it wherever the fence is set on an assembled ruleset;
    /// [`Self::validate_palw_tir_only_v1`] refuses a ruleset whose copy disagrees.
    pub fn sync_palw_tir_only_v1(&mut self) {
        let from_daa = self.palw_tir_only_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_tir_only_from_daa(from_daa);
        }
    }

    /// **The fence's own refusals**, asked by [`Params::validate_palw_v2`]: a mirror that disagrees; arming off `ConsensusV2`; arming
    /// without `palw_tir_v1` in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_tir_only_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_only_from_daa(),
            _ => None,
        };
        let armed = self.palw_tir_only_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_only_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_tir_only_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_tir_only_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let tir_ok = self.palw_tir_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at);
        if !tir_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_only_v1 needs palw_tir_v1 in force at or below it: a network that closes the legacy admission door must have opened the IR one",
            ));
        }
        Ok(())
    }
}
