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
    /// **The job this claim's block asked for**, derived from the block — never read off the
    /// capture being checked.
    ///
    /// The roots alone say "this material computes to what the claim announced". They do not say
    /// WHICH question was answered, and the anchor is the question: it is a pure function of the
    /// claim's own block (its pre-PoW hash), its network, its class and its executor bond. Without
    /// it a gossiped capture is a re-usable asset — anyone can mine a fresh block, announce the
    /// borrowed roots, and both halves of the verification agree, because a seat compares roots and
    /// a challenger re-executes the anchor the capture itself names. One inference, unlimited
    /// blocks, by parties that ran nothing.
    ///
    /// `Hash64::default()` means "this caller has no block to bind to" and skips the check — the
    /// producer checking its own fresh run, and the fixtures. Every path that judges SOMEBODY
    /// ELSE's material must supply it.
    pub anchor: Hash64,
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

    /// **A party's answer at one rung of the bisection: its execution's state at `index`.**
    ///
    /// The ladder converges only if this is a PREFIX commitment — two executions agreeing through
    /// `index` must agree here, and two differing before it must not — because that is what makes
    /// "the first index we disagree on" the same as "the first leaf our executions differ at".
    ///
    /// `None` is the honest answer for a family the court cannot adjudicate, and for material this
    /// backend cannot read. A silent party loses its rung, which is the correct outcome for a party
    /// that cannot substantiate its own execution.
    fn bisect_prefix_state(&self, _material: &[u8], _index: u64) -> Option<Hash64> {
        None
    }

    /// **The terminal move's evidence: everything the court needs to recompute step `index`.**
    ///
    /// Returned by BOTH sides, and deliberately the same call for both: an honest executor closing
    /// its own case and a challenger closing a real fraud assemble the identical object, and
    /// `adjudicate_court_close_v2` is what decides which way it reads. A prover that could only be
    /// run by one side would be a prover that decides the verdict.
    ///
    /// `Err` for a family with no court, and for an index this capture cannot open.
    fn refutation_for_index(
        &self,
        _material: &[u8],
        _index: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        Err("this execution family cannot be adjudicated".to_string())
    }

    /// **The anchor a block asks its job of** — the family's own derivation, from chain facts only.
    ///
    /// Every input is recomputable by anyone holding the block: the network domain, the header's
    /// pre-PoW hash, the class, and the executor's bond outpoint. That is what makes the anchor
    /// checkable rather than merely declared — a producer cannot choose it, and a party verifying
    /// somebody else's claim derives the same value the producer was forced to use.
    ///
    /// **The default is the shared derivation, deliberately** — the producer computes the anchor
    /// before it resolves a backend at all, so every family already runs the job this names. A
    /// `None` default would have been the quiet kind of wrong: a family that simply never
    /// implemented the method would answer "I cannot derive it", every seat would decline to judge
    /// its claims, and the class would stop licensing with nothing in any log saying why.
    ///
    /// A family that genuinely derives its job differently overrides this. `None` is reserved for
    /// a family with no canonical job at all, and a caller that gets it must decline to judge
    /// rather than fall back to the anchor named inside the material — that is the thing under
    /// test.
    fn job_anchor_v1(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        class_id: Hash64,
        executor_bond: &crate::tx::TransactionOutpoint,
    ) -> Option<Hash64> {
        Some(crate::palw_attempt_v2::palw_job_anchor_v1(network_domain, pre_pow_hash, class_id, executor_bond))
    }

    /// **The weight rows that refutation reads — exactly those, proven against the class root.**
    ///
    /// The court recomputes the disputed step, and recomputing it means reading operands out of
    /// the registered artifact. It holds no weights of its own, so a close must carry them; and it
    /// refuses any row that does not prove against the `artifact_root` the class registered under,
    /// so carrying the wrong ones is the same as carrying none.
    ///
    /// **Asked of the adjudicator, never enumerated here.** The set of rows a step reads is decided
    /// by the arithmetic the adjudicator walks, and a second enumeration written on the prover side
    /// would be a second opinion about that — one that agrees today and diverges the first time a
    /// kernel changes which operand it touches, in the direction where an honest producer cannot
    /// close. So the implementation runs the real adjudicator against the full inventory through a
    /// recording oracle and opens what it actually resolved. Opening the whole inventory instead
    /// would be correct and unaffordable: a close has a byte ceiling, and a class's weights do not
    /// fit under it.
    ///
    /// `Err` for a family with no court and for an artifact this backend cannot root.
    fn operand_openings_for(
        &self,
        _refutation: &crate::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<crate::palw_artifact::PalwArtifactOpeningV1>, String> {
        Err("this execution family cannot be adjudicated".to_string())
    }

    /// **A DRILL fault: run the job, corrupt one lane of one tile, and commit to the result.**
    ///
    /// A court that has never convicted on a live chain is a court nobody has evidence works, and
    /// the only way to get that evidence is for some producer to actually be wrong. Re-deriving
    /// the commitment from the corrupted capture is what makes this a real fraud rather than a
    /// mismatch: the producer's roots are self-consistent and honestly its own, and the ONLY way
    /// to catch it is to run the canonical job yourself — which is exactly the fraud the court
    /// exists for and exactly the one no seat check can see.
    ///
    /// Callers must refuse to reach this on a network carrying value. It is a method rather than a
    /// test helper because the drill has to go through the same production path the honest
    /// producer does; a fault injected somewhere else would prove something about the injector.
    fn execute_with_injected_fault(
        &self,
        _job: &PalwJobContextV2,
        _prompt: &[usize],
        _leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        Err("this execution family has no drill fault".to_string())
    }
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
