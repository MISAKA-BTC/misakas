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

// ---------------------------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------------------------
//
// **What was open.** An FP job names a `tokenizer_id`, the artifact commits to a tokenizer, and nothing on chain
// compared the two: a job whose ids came from some other `tokenizer.json` was an honest claim about those ids and a
// useless one to anybody who wanted to read the answer back (the tokenizer in a job's context is whatever the producer
// wrote; a consumer looking the class up finds a different one). RFC-0001 §2.9 closes it where the chain holds a
// listing to compare against.
//
// **What a listing is.** A class's registry row carries its tokenizer commitment when the class is an IR or generative
// class (`PalwTirClassRecordV1::tokenizer_id`, `PalwGenClassRecordV1::tokenizer_id`: the id the class's own identity
// hashes over). A dense A16 class registered through the legacy path lists none — its tokenizer commitment lives
// inside an artifact digest the chain cannot read — so the rule is SILENT for it, exactly as it was. Past
// `Params::palw_fp_tokenizer_match`, a commitment on a class that lists a tokenizer is skipped, by name, unless its
// job names that tokenizer.
//
// **Where it runs.** The extraction walk, per class, from the same closure that supplies every other per-class cap
// (`PalwFpClassCapsV1`), so a build and a producer ask the question of one function.

use crate::Hash64;

/// What the walk holds of a class's tokenizer listing at the accepting block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpTokenizerRuleV1 {
    /// Below `palw_fp_tokenizer_match`, or a network that never arms it: nothing is compared.
    Dormant,
    /// Past the fence, and the class lists this tokenizer: the job must name it.
    Listed(Hash64),
    /// Past the fence, and the class lists none (a dense class, or a listing of the empty commitment): silent.
    Unlisted,
}

impl PalwFpTokenizerRuleV1 {
    /// The rule for a class: `armed` is the fence in force at the accepting block, `listed` the class's listing.
    /// The all-zero commitment is "none" (`Hash64::default()` is not an opinion — a class that lists it says nothing).
    pub fn of(armed: bool, listed: Option<Hash64>) -> Self {
        match (armed, listed) {
            (false, _) => Self::Dormant,
            (true, Some(t)) if t != Hash64::default() => Self::Listed(t),
            (true, _) => Self::Unlisted,
        }
    }

    /// Does a job naming `job_tokenizer` meet the rule? `Err` is the walk's skip reason.
    pub fn check(self, job_tokenizer: &Hash64) -> Result<(), &'static str> {
        match self {
            Self::Dormant | Self::Unlisted => Ok(()),
            Self::Listed(listed) if listed == *job_tokenizer => Ok(()),
            Self::Listed(_) => Err("the job's tokenizer_id is not the tokenizer its class lists (palw_fp_tokenizer_match)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    #[test]
    fn the_rule_is_silent_until_armed_and_for_a_class_that_lists_nothing() {
        assert_eq!(PalwFpTokenizerRuleV1::of(false, Some(h(1))), PalwFpTokenizerRuleV1::Dormant);
        assert_eq!(PalwFpTokenizerRuleV1::of(true, None), PalwFpTokenizerRuleV1::Unlisted);
        assert_eq!(PalwFpTokenizerRuleV1::of(true, Some(Hash64::default())), PalwFpTokenizerRuleV1::Unlisted);
        for rule in [PalwFpTokenizerRuleV1::Dormant, PalwFpTokenizerRuleV1::Unlisted] {
            assert_eq!(rule.check(&h(7)), Ok(()));
            assert_eq!(rule.check(&Hash64::default()), Ok(()));
        }
    }

    #[test]
    fn a_listed_class_admits_its_own_tokenizer_and_no_other() {
        let rule = PalwFpTokenizerRuleV1::of(true, Some(h(5)));
        assert_eq!(rule, PalwFpTokenizerRuleV1::Listed(h(5)));
        assert_eq!(rule.check(&h(5)), Ok(()));
        assert!(rule.check(&h(6)).unwrap_err().contains("palw_fp_tokenizer_match"));
        assert!(rule.check(&Hash64::default()).is_err(), "an unbound (zero) tokenizer is not the listed one");
    }
}
