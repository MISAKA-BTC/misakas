//! **The execution-backend seam** (ADR-0051 implementation step 1; the adapter surface ADR-0026
//! adopted from Ambient and never made a type).
//!
//! A PALW node does three things with an execution and knows, at each of them, only what the CHAIN
//! told it: run the job this template implies, commit to what it ran, and — as a panel seat —
//! decide whether somebody else's material answers for the claim they published. Until now all
//! three reached for `misaka-palw-base0` by name, so the node could execute exactly one family of
//! class: the deterministic integer one.
//!
//! # Why a family and not a class
//!
//! ADR-0051 splits the network into two **execution families**, where a family is a *verification
//! scheme* rather than a model:
//!
//! * [`PalwExecutionFamilyV1::DeterministicInteger`] — pinned arithmetic, a graph projected from a
//!   canonical IR, disputes ending in the ADR-0049 court. Verification is exact and a liar can be
//!   convicted from one opened tile.
//! * [`PalwExecutionFamilyV1::MetalGguf`] — a pinned GGUF on a pinned Apple-Silicon/Metal runtime
//!   build, committing what the inference *said*, verified by bonded same-family replay. No court:
//!   a tolerance can acquit but never convict, so the only slashable offenses are the objective
//!   ones (contradictory receipts, equivocation, withholding).
//!
//! The families differ in what verification MEANS, which is exactly what a trait boundary is for.
//! They do not differ in what an attempt carries — [`PalwExecutionOutcomeV1`] is the same four
//! roots either way, because those roots are the block header's business and the header does not
//! know which family produced them.
//!
//! # What this trait deliberately does not abstract
//!
//! Not the court. A backend has no `adjudicate`, because only one family has one; a Family-M
//! dispute has no arithmetic terminal and inventing a uniform `verify_step` would imply otherwise.
//! Not the artifact format either: the floor DERIVES its weights from a seed and a converted class
//! ships a file, which `misaka_palw_base0::classes::resolve_class_v1` already reconciles against
//! the chain's `(class_id, artifact_root)` pair.

use crate::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Which verification scheme a class is registered under (ADR-0051 Decision 1).
///
/// Carried as its own type rather than inferred from the profile, because the *economy* keys on
/// it: the share table is capped at 500‰ per family, so "which family" is a fact the chain reasons
/// about and not a property a node re-derives from a graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwExecutionFamilyV1 {
    /// BASE-0 and every class whose disputes end in the ADR-0049 court.
    DeterministicInteger = 0,
    /// Pinned GGUF on a pinned Apple-Silicon/Metal runtime (ADR-0051).
    MetalGguf = 1,
}

impl PalwExecutionFamilyV1 {
    /// **Can a dispute in this family end in a conviction?**
    ///
    /// The one question the rest of the system must never get wrong. `false` means the court is
    /// not merely unimplemented for this family — it is unavailable in principle, because the
    /// comparison the family verifies with is a tolerance and a tolerance cannot separate "lied by
    /// ε" from "rounded by ε". A close arm that ignored this would convict on rounding.
    pub fn is_court_adjudicable(self) -> bool {
        matches!(self, Self::DeterministicInteger)
    }
}

/// The roots an attempt carries, and the bytes that answer for them.
///
/// Family-agnostic on purpose: a block header commits these four values and does not know which
/// backend produced them. What differs between families is how `material` is checked, not what
/// the header holds.
pub struct PalwExecutionOutcomeV1 {
    /// The logits leg. Which SCHEME this root is under is a class fact — the integer family
    /// commits `base0_logits_trace_root_v1`, the float families the v2 event-tree root — and the
    /// dispatch on the class's registered lane already exists in `palw_step_refute`.
    pub trace_root: Hash64,
    pub output_root: Hash64,
    /// The composite the court (where there is one) pins a refutation's binding against.
    pub execution_root: Hash64,
    pub trace_manifest_root: Hash64,
    pub trace_chunk_count: u32,
    /// What the producer retains for `trace_retention_daa` and broadcasts to its panel. Opaque
    /// here: only the backend that wrote it can read it, which is the point of the seam.
    pub material: Vec<u8>,
}

/// The two roots a seat checks material against — the claim's own, read from chain state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimRootsV1 {
    pub execution_root: Hash64,
    pub trace_root: Hash64,
}

/// What a seat concluded about served material.
///
/// Three outcomes and not two, because "I could not verify" and "this does not match" are
/// different accusations and the receipt lane already distinguishes them: the first is
/// `Unavailable` against the producer's data-availability obligation, the second is a claim that
/// simply gathers no quorum and voids. Collapsing them would either accuse an honest producer of
/// withholding or let a mismatch pass as a network hiccup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMaterialVerdictV1 {
    /// The material answers for the claim's committed roots.
    Matches,
    /// It decoded and does not answer for them.
    Mismatch,
    /// It could not be checked at all (undecodable, wrong family, unavailable operand).
    Unverifiable,
}

/// One family's execution path, as a node uses it.
///
/// Implementors: `misaka_palw_base0::backend::Base0Backend` (Family D) and, when ADR-0051 step 2
/// lands, the Metal/GGUF backend.
pub trait PalwExecutionBackendV1: Send + Sync {
    fn family(&self) -> PalwExecutionFamilyV1;

    /// A human-readable identity for logs — the model id for a converted class, the floor's name
    /// for the derived one. Never used for dispatch: the chain's `class_id` is.
    fn model_id(&self) -> &str;

    /// **The job this anchor implies.** A producer must not choose its own prompt — a class whose
    /// executor picks the input is a class where "run the model" and "find an input whose output I
    /// like" are the same move — so the job is derived from the template's anchor, and this is
    /// where that derivation lives for each family.
    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String>;

    /// Run the job and commit to it. Pure CPU/GPU work with no chain access: the caller runs it off
    /// the async runtime.
    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String>;

    /// **A seat's check, before it signs.** Never a conviction — a mismatch is the court's to
    /// convict where a court exists, and where one does not the claim simply fails to gather a
    /// quorum and voids.
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The court boundary is a property of the family, asserted once.** Every consumer that
    /// decides whether to open, accept or close a court reads this; a family added without an
    /// answer here would default to whatever the first `match` arm happened to be.
    #[test]
    fn only_the_deterministic_family_can_be_convicted() {
        assert!(PalwExecutionFamilyV1::DeterministicInteger.is_court_adjudicable());
        assert!(
            !PalwExecutionFamilyV1::MetalGguf.is_court_adjudicable(),
            "a tolerance cannot separate a lie from a rounding, so a Metal class must never reach a conviction"
        );
    }

    /// The discriminants are on the wire (a class registration carries its family), so they are
    /// pinned the way every other borsh discriminant in this tree is.
    #[test]
    fn the_family_discriminants_are_pinned() {
        assert_eq!(borsh::to_vec(&PalwExecutionFamilyV1::DeterministicInteger).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&PalwExecutionFamilyV1::MetalGguf).unwrap(), vec![1]);
    }
}
