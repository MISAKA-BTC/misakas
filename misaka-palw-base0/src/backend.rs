//! **Family D behind the backend seam** (ADR-0051 step 1).
//!
//! Nothing here is new arithmetic. It is the existing floor path — `resolve_class_v1` for the
//! material, `base0_rc_job_v1` for the job, `base0_execute_for_attempt_v1` for the run,
//! `base0_material_matches_claim_v1` for a seat's check — expressed as
//! [`PalwExecutionBackendV1`] so the producer and the panel stop naming this crate.
//!
//! The value is what it makes possible rather than what it changes: a second family can be added
//! without touching either consumer, and — more immediately — the two consumers can no longer
//! *accidentally* assume the floor. They asked for `base0_profile_v1(PALW_RC_BASE0_GEOMETRY)` by
//! name until today, which is how `class_id` came to be a configurable value that decided nothing.

use crate::artifact::Base0ArtifactV1;
use crate::classes::ResolvedClassV1;
use crate::produce::{base0_execute_for_attempt_v1, base0_material_decode_v1, base0_material_encode_v1, base0_material_matches_claim_v1, base0_rc_job_v1};
use kaspa_consensus_core::palw_backend::{
    PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionFamilyV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// The deterministic integer family's backend, bound to one resolved class.
///
/// Constructed from what the CHAIN named (`resolve_class_v1` has already refused anything whose
/// graph or weights disagree with the registration), so by the time a backend exists the question
/// "is this the right class" is settled and the producer never re-asks it.
pub struct Base0Backend {
    model_id: String,
    profile: PalwShapeProfileV3,
    artifact: Base0ArtifactV1,
    canonical_job: (u32, u32),
}

impl Base0Backend {
    pub fn new(resolved: ResolvedClassV1) -> Self {
        Self {
            model_id: resolved.model_id.to_string(),
            profile: resolved.profile,
            artifact: resolved.artifact,
            canonical_job: resolved.canonical_job,
        }
    }

    /// The graph, for the callers that still need it directly (the retention writer names the
    /// class in its path). Exposed rather than leaked through the trait: the trait's job is the
    /// three verbs, and a `profile()` on it would be an invitation to reach past them.
    pub fn profile(&self) -> &PalwShapeProfileV3 {
        &self.profile
    }
}

impl PalwExecutionBackendV1 for Base0Backend {
    fn family(&self) -> PalwExecutionFamilyV1 {
        PalwExecutionFamilyV1::DeterministicInteger
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        Ok(base0_rc_job_v1(&self.profile, anchor, self.artifact.shape.vocab, self.canonical_job.0, self.canonical_job.1))
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let run = base0_execute_for_attempt_v1(&self.artifact, &self.profile, job, prompt).map_err(|e| e.to_string())?;
        // Encoded HERE, while the run is in hand. The producer used to reach into `run.tiles` to
        // write its retention file, which meant the retention format and the broadcast format were
        // two decisions in two places; the codec has been one function since the panel service
        // landed, and the seam is where that becomes structural.
        let material = base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            material,
        })
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Ok(decoded) = base0_material_decode_v1(material) else {
            // Bytes that do not decode are bytes that were not served — the seat's honest
            // `Unavailable`, not an accusation that the producer computed the wrong thing.
            return PalwMaterialVerdictV1::Unverifiable;
        };
        match base0_material_matches_claim_v1(&decoded, claim.execution_root, claim.trace_root) {
            Ok(true) => PalwMaterialVerdictV1::Matches,
            Ok(false) => PalwMaterialVerdictV1::Mismatch,
            Err(_) => PalwMaterialVerdictV1::Unverifiable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;

    fn floor_backend() -> Base0Backend {
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves from nothing"))
    }

    /// **The seam produces what the header needs, end to end** — and the floor still runs through
    /// it, which is the only thing that makes the refactor safe to land.
    #[test]
    fn the_floor_executes_through_the_seam() {
        let backend = floor_backend();
        assert_eq!(backend.family(), PalwExecutionFamilyV1::DeterministicInteger);
        assert!(backend.family().is_court_adjudicable(), "the floor is the family a court can convict in");

        let anchor = Hash64::from_u64_word(0x5EA_u64);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the floor's canonical job runs");
        assert_ne!(outcome.trace_root, Hash64::default());
        assert_ne!(outcome.execution_root, Hash64::default());
        assert!(!outcome.material.is_empty(), "a producer that retained nothing could not answer a challenge");

        // The seat's half, against the roots this very run committed.
        let claim = PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root };
        assert_eq!(backend.verify_material(&outcome.material, claim), PalwMaterialVerdictV1::Matches);
    }

    /// **The three verdicts are three, and each is reachable.** Collapsing `Mismatch` into
    /// `Unverifiable` would have a seat accuse an honest producer of withholding; collapsing the
    /// other way would let a wrong execution pass as a network hiccup.
    #[test]
    fn a_seat_separates_did_not_decode_from_does_not_match() {
        let backend = floor_backend();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(7)).expect("job");
        let outcome = backend.execute(&job, &prompt).expect("runs");
        let claim = PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root };

        assert_eq!(backend.verify_material(b"not material at all", claim), PalwMaterialVerdictV1::Unverifiable);
        // Real material, a claim committing a DIFFERENT execution: the case a rubber stamp signs.
        let other = PalwClaimRootsV1 { execution_root: Hash64::from_u64_word(0xBAD), ..claim };
        assert_eq!(backend.verify_material(&outcome.material, other), PalwMaterialVerdictV1::Mismatch);
    }

    /// The anchor decides the job, so two anchors are two jobs — the property that stops a
    /// producer from choosing an input whose output it likes.
    #[test]
    fn the_anchor_decides_the_job() {
        let backend = floor_backend();
        let (a, pa) = backend.job_for_anchor(Hash64::from_u64_word(1)).expect("job");
        let (b, pb) = backend.job_for_anchor(Hash64::from_u64_word(2)).expect("job");
        assert_ne!(pa, pb, "a different anchor is a different prompt");
        assert_ne!(a.prompt_token_ids_hash, b.prompt_token_ids_hash);
        assert_eq!(a.declared_prefill_tokens, b.declared_prefill_tokens, "the SHAPE is the class's, not the anchor's");
    }
}
