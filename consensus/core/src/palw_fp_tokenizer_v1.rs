//! **RFC-0001 §2.9: the job's tokenizer must be the class's listed one; dormant behind `palw_fp_tokenizer_match`.**
//!
//! (The module's logic follows its fence: this first section is the fence's own plumbing — the accessor, the
//! prerequisites `validate_palw_v2` asks by name, and the drill entry a salted chain arms it with.)

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms `palw_fp_tokenizer_match` with** (`--palw-drill-fp-tokenizer-match-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_fp_tokenizer_match", set: |params, at| params.palw_fp_tokenizer_match = at };

/// The drill's one-entry list.
pub const PALW_DRILL_FP_TOKENIZER_MATCH_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY];

impl Params {
    /// `palw_fp_tokenizer_match`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_fp_tokenizer_match_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_tokenizer_match) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_fp_tokenizer_match_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_tokenizer_match_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **`palw_fp_tokenizer_match`'s own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule over its prerequisites,
    /// each in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_fp_tokenizer_match_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_tokenizer_match.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_tokenizer_match is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_tir_v1.map(|fence| fence.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_tokenizer_match needs palw_tir_v1 in force at or below it: the listing lives in an IR class's record (as its fence activation)",
            ));
        }
        if !at_or_below(self.palw_fp_decode_rules) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_tokenizer_match needs palw_fp_decode_rules in force at or below it: the rule reads a V4 job",
            ));
        }
        Ok(())
    }
}
