//! **RFC-0001 §2.5: the decode constraint's second subset and automaton bounds; dormant behind `palw_fp_constraint_v2`.**
//!
//! (The module's logic follows its fence: this first section is the fence's own plumbing — the accessor, the
//! prerequisites `validate_palw_v2` asks by name, and the drill entry a salted chain arms it with.)

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms `palw_fp_constraint_v2` with** (`--palw-drill-fp-constraint-v2-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_FP_CONSTRAINT_V2_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_fp_constraint_v2", set: |params, at| params.palw_fp_constraint_v2 = at };

/// The drill's one-entry list.
pub const PALW_DRILL_FP_CONSTRAINT_V2_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_CONSTRAINT_V2_ENTRY];

impl Params {
    /// `palw_fp_constraint_v2`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_fp_constraint_v2_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_constraint_v2) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_fp_constraint_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_constraint_v2_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **`palw_fp_constraint_v2`'s own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule over its prerequisites,
    /// each in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_fp_constraint_v2_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_constraint_v2.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_constraint_v2 is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_fp_decode_constraint) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_constraint_v2 needs palw_fp_decode_constraint in force at or below it: every constraint is carried under it",
            ));
        }
        if !at_or_below(self.palw_fp_decode_rules) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_constraint_v2 needs palw_fp_decode_rules in force at or below it: a constraint rides a V4 job",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The second automaton form
// ---------------------------------------------------------------------------------------------
//
// **What changes and what does not.** The automaton's STEP FUNCTION is not touched: a discriminated union is frames, `Key`
// edges and per-branch nodes; an inlined `$ref` is a frame; an integer range is a literal trie. Nothing new has to be
// executed by a seat or tried by a court, so [`PalwDecodeConstraintV1`] is also the second form's type, its `admit`, its
// state function and its court reading unchanged. What a second subset needs is room — a union of sixteen closed objects of
// sixteen members, or a range of a thousand literals, is bigger than the first form's 64 KiB — and a header word that says
// which compiler's bound a constraint was admitted under, so the first form's refusals stay exactly what they were.
//
// **The rule.** Past `Params::palw_fp_constraint_v2` a constraint of header version 2 is admitted under [`PALW_CONSTRAINT_MAX_BYTES_V2`] bytes and
// [`PALW_CONSTRAINT_MAX_NODES_V2`] nodes; below it version 2 is refused by name. Version 1 is admitted at every height under
// its own bounds, byte for byte as before. The constraint's id is unchanged (`constraint_id_v1` over the canonical bytes).

use crate::palw_decode_constraint_v1::{PalwDecodeConstraintError, PalwDecodeConstraintV1};

/// The second form's header version.
pub const PALW_DECODE_CONSTRAINT_VERSION_V2: u16 = 2;
/// The largest second-form constraint, serialized: 256 KiB (four times the first form's ceiling).
pub const PALW_CONSTRAINT_MAX_BYTES_V2: usize = 256 * 1024;
/// The most nodes a second-form automaton holds across all its frames (a frame still names its nodes by a `u16`).
pub const PALW_CONSTRAINT_MAX_NODES_V2: usize = 262_143;

/// Why a constraint is not admitted under the second form's rule.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwConstraintFormErrorV1 {
    #[error("a version-2 constraint below Params::palw_fp_constraint_v2")]
    SecondFormNotArmed,
    #[error("the constraint's header version {0} is neither 1 nor 2")]
    UnknownVersion(u16),
    #[error(transparent)]
    Bytes(#[from] PalwDecodeConstraintError),
}

/// **A constraint's bytes, admitted under whichever form their header says** (RFC-0001 §2.5): version 1 under the first
/// form's bounds at every height; version 2 under the second form's only where `armed`
/// (`Params::palw_fp_constraint_v2_active_at`). The header word is read first (bytes 0..2), so an unknown version costs one
/// comparison and a version-2 blob below the fence is refused before it is parsed.
pub fn palw_constraint_admitted_v1(bytes: &[u8], armed: bool) -> Result<PalwDecodeConstraintV1, PalwConstraintFormErrorV1> {
    let version = bytes.get(..2).map(|w| u16::from_le_bytes([w[0], w[1]]));
    match version {
        Some(crate::palw_decode_constraint_v1::PALW_DECODE_CONSTRAINT_VERSION_V1) => Ok(PalwDecodeConstraintV1::from_bytes(bytes)?),
        Some(PALW_DECODE_CONSTRAINT_VERSION_V2) if armed => Ok(PalwDecodeConstraintV1::from_bytes_form(
            bytes,
            PALW_DECODE_CONSTRAINT_VERSION_V2,
            PALW_CONSTRAINT_MAX_NODES_V2,
            PALW_CONSTRAINT_MAX_BYTES_V2,
        )?),
        Some(PALW_DECODE_CONSTRAINT_VERSION_V2) => Err(PalwConstraintFormErrorV1::SecondFormNotArmed),
        Some(other) => Err(PalwConstraintFormErrorV1::UnknownVersion(other)),
        None => Err(PalwConstraintFormErrorV1::Bytes(PalwDecodeConstraintError::Malformed("no header version".to_string()))),
    }
}

/// [`PalwDecodeConstraintV1::validate`] for a second-form automaton (the compiler's own check, so a constraint it emits is
/// one the chain admits).
pub fn palw_constraint_validate_v2_v1(c: &PalwDecodeConstraintV1) -> Result<(), PalwDecodeConstraintError> {
    c.validate_form(PALW_DECODE_CONSTRAINT_VERSION_V2, PALW_CONSTRAINT_MAX_NODES_V2, PALW_CONSTRAINT_MAX_BYTES_V2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_decode_constraint_v1::{
        PalwConstraintActionV1, PalwConstraintEdgeV1, PalwConstraintFrameV1, PalwConstraintNodeV1,
    };

    /// A tiny well-formed automaton (one frame, one accepting node) at `version`, padded with `extra` empty frames.
    fn automaton(version: u16, extra_frames: usize) -> PalwDecodeConstraintV1 {
        let leaf = PalwConstraintNodeV1 { accepting: true, edges: vec![] };
        let start = PalwConstraintNodeV1 {
            accepting: false,
            edges: vec![PalwConstraintEdgeV1 { lo: b'x', hi: b'x', action: PalwConstraintActionV1::Goto(1) }],
        };
        let mut frames = vec![PalwConstraintFrameV1 { start: 0, nodes: vec![start, leaf.clone()] }];
        for _ in 0..extra_frames {
            frames.push(PalwConstraintFrameV1 { start: 0, nodes: vec![leaf.clone()] });
        }
        PalwDecodeConstraintV1 { version, compiler_id: Hash64::from_u64_word(7), start_frame: 0, frames }
    }

    #[test]
    fn version_one_is_admitted_everywhere_and_version_two_only_where_armed() {
        let v1 = automaton(1, 0).to_bytes();
        let v2 = automaton(2, 0).to_bytes();
        for armed in [false, true] {
            assert_eq!(palw_constraint_admitted_v1(&v1, armed).unwrap().version, 1, "armed={armed}");
        }
        assert_eq!(palw_constraint_admitted_v1(&v2, false), Err(PalwConstraintFormErrorV1::SecondFormNotArmed));
        assert_eq!(palw_constraint_admitted_v1(&v2, true).unwrap().version, 2);
        assert_eq!(palw_constraint_admitted_v1(&automaton(3, 0).to_bytes(), true), Err(PalwConstraintFormErrorV1::UnknownVersion(3)));
        assert!(matches!(palw_constraint_admitted_v1(&[], true), Err(PalwConstraintFormErrorV1::Bytes(_))));
        // The first form refuses a version-2 header and the second a version-1 one: one form per header word.
        assert!(PalwDecodeConstraintV1::from_bytes(&v2).is_err());
        assert!(palw_constraint_validate_v2_v1(&automaton(1, 0)).is_err());
        assert!(palw_constraint_validate_v2_v1(&automaton(2, 0)).is_ok());
    }

    #[test]
    fn the_second_form_has_its_own_ceilings_and_the_first_keeps_its_own() {
        // Past the first form's 64 KiB, inside the second's 256 KiB: a thousand-odd frames of one node each, as
        // `to_bytes` counts them.
        let big_v2 = automaton(2, 14_000);
        let len = big_v2.to_bytes().len();
        assert!(len > crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_BYTES_V1 && len < PALW_CONSTRAINT_MAX_BYTES_V2, "{len}");
        assert!(palw_constraint_admitted_v1(&big_v2.to_bytes(), true).is_ok(), "the second form admits it");
        let big_v1 = automaton(1, 14_000);
        assert!(matches!(
            palw_constraint_admitted_v1(&big_v1.to_bytes(), true),
            Err(PalwConstraintFormErrorV1::Bytes(PalwDecodeConstraintError::TooLarge(_)))
        ));
        // Past the second form's ceiling too.
        let huge = automaton(2, 45_000);
        assert!(huge.to_bytes().len() > PALW_CONSTRAINT_MAX_BYTES_V2);
        assert!(matches!(
            palw_constraint_admitted_v1(&huge.to_bytes(), true),
            Err(PalwConstraintFormErrorV1::Bytes(PalwDecodeConstraintError::TooLarge(_)))
        ));
        // A frame count the u16 cannot name is refused in either form.
        assert!(PALW_CONSTRAINT_MAX_NODES_V2 > crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_NODES_V1);
    }
}
