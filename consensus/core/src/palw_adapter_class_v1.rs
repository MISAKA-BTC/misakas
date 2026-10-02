//! **RFC-0001 §2.10 (ADR-0163): the adapter class listing; dormant behind `palw_adapter_class_v1`.**
//!
//! (The module's logic follows its fence: this first section is the fence's own plumbing — the accessor, the
//! prerequisites `validate_palw_v2` asks by name, and the drill entry a salted chain arms it with.)

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms `palw_adapter_class_v1` with** (`--palw-drill-adapter-class-v1-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_ADAPTER_CLASS_V1_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_adapter_class_v1", set: |params, at| params.palw_adapter_class_v1 = at };

/// The drill's one-entry list.
pub const PALW_DRILL_ADAPTER_CLASS_V1_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_ADAPTER_CLASS_V1_ENTRY];

impl Params {
    /// `palw_adapter_class_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_adapter_class_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_adapter_class_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_adapter_class_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_adapter_class_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **`palw_adapter_class_v1`'s own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule over its prerequisites,
    /// each in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_adapter_class_v1_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_adapter_class_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_adapter_class_v1 is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_tir_v1.map(|fence| fence.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_adapter_class_v1 needs palw_tir_v1 in force at or below it: an adapter class is an IR class (as its fence activation)",
            ));
        }
        if !at_or_below(self.palw_improvement_v1.map(|fence| fence.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_adapter_class_v1 needs palw_improvement_v1 in force at or below it: the composite machinery lives behind it (as its fence activation)",
            ));
        }
        Ok(())
    }
}
