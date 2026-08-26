//! **The execution-backend seam** (ADR-0053; the adapter surface ADR-0026 adopted from Ambient
//! and never made a type).
//!
//! A PALW node does three things with an execution and knows, at each of them, only what the CHAIN
//! told it: run the job this template implies, commit to what it ran, and — as a panel seat —
//! decide whether somebody else's material answers for the claim they published. This trait is
//! where those three verbs live, so that no consumer names a runtime crate directly.
//!
//! # One execution family, and why the seam survives it
//!
//! ADR-0051 proposed a second *family* — a verification scheme, not a model: a pinned GGUF under a
//! pinned Metal runtime, committing what the inference SAID and verified by tolerant replay.
//! **ADR-0053 withdraws it.** A tolerance can acquit but never convict, so half the economy would
//! have been non-convictable work, and the three mechanisms that were supposed to bound that —
//! the 500‰ family cap, the per-class panel, the court exclusion — were respectively never
//! constructed, never consumed, and a runtime `if`. What removed the motive was measurement:
//! Qwen3.6 runs in the integer runtime with 100 % kernel-catalog coverage, so the model the black
//! box existed to serve is adjudicable without it.
//!
//! So there is exactly one family — pinned integer arithmetic, a graph projected from a canonical
//! IR, disputes ending in the ADR-0049 court — and it is not a value any object carries. **Every
//! registered class is court-adjudicable by construction**, which is a stronger statement than a
//! flag that says so: there is no arm to get wrong.
//!
//! # What this trait deliberately does not abstract
//!
//! Not the court's rules — a backend supplies evidence (`bisect_prefix_state`,
//! `refutation_for_index`) and never a verdict. Not the artifact format either: the floor DERIVES
//! its weights from a seed and a converted class ships a file, which
//! `misaka_palw_base0::classes::resolve_class_v1` already reconciles against the chain's
//! `(class_id, artifact_root)` pair.

use crate::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

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

/// The execution path, as a node uses it.
///
/// Implementor: `misaka_palw_base0::backend::Base0Backend`. The trait stays a trait because the
/// consumers must not name that crate — a producer, a seat and the court reach an execution
/// through the same three verbs, and a second implementor is a test double, not a second family.
pub trait PalwExecutionBackendV1: Send + Sync {
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
    /// convict; a seat that disagrees signs nothing on the merits and the claim voids for want of
    /// a quorum.
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1;

    /// **A party's answer at one rung of the bisection: its execution's state at `index`.**
    ///
    /// The ladder converges only if this is a PREFIX commitment — two executions agreeing through
    /// `index` must agree here, and two differing before it must not — because that is what makes
    /// "the first index we disagree on" the same as "the first leaf our executions differ at".
    ///
    /// `None` is the honest answer for material this backend cannot read. A silent party loses its
    /// rung, which is the correct outcome for a party that cannot substantiate its own execution.
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
    /// `Err` for an index this capture cannot open.
    fn refutation_for_index(
        &self,
        _material: &[u8],
        _index: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        Err("this backend cannot open a refutation at that index".to_string())
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
        Err("this backend has no drill fault".to_string())
    }
}

#[cfg(test)]
mod tests {
    /// **There is no family value to get wrong.** The type that used to answer "can a dispute
    /// about this class end in a conviction?" is gone (ADR-0053), and this test is what keeps it
    /// gone: a re-introduced flag would give some future consumer an arm to take, and the arm the
    /// withdrawn family needed was the one that skipped the coverage gate.
    #[test]
    fn the_seam_carries_no_verification_scheme_flag() {
        let src = include_str!("palw_backend.rs");
        for banned in ["PalwExecutionFamilyV1", "is_court_adjudicable", "MetalGguf"] {
            assert!(
                !src.split("fn the_seam_carries_no_verification_scheme_flag").next().unwrap().contains(banned),
                "{banned} is back in the execution seam — ADR-0053 withdrew the second family"
            );
        }
    }
}
