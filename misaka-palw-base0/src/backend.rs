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
use crate::produce::{
    base0_execute_for_attempt_v1, base0_material_decode_v1, base0_material_encode_v1, base0_material_matches_claim_v1, base0_rc_job_v1,
};
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

    fn execute_with_injected_fault(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        let mut run = base0_execute_for_attempt_v1(&self.artifact, &self.profile, job, prompt).map_err(|e| e.to_string())?;
        let ctx_hash = job.context_hash();
        let profile_hash = self.profile.shape_profile_id();
        {
            let slot = run
                .tiles
                .tiles
                .iter_mut()
                .find(|(i, _)| *i == leaf_index)
                .ok_or_else(|| format!("the capture holds no tile at leaf {leaf_index}"))?;
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            run.tiles.leaves[leaf_index as usize] =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &slot.1);
        }
        // **Re-derive, do not patch.** The commitment must be the corrupted capture's OWN, or this
        // is a producer whose roots disagree with its material — which any seat catches without a
        // court, and which is therefore not the fraud under test.
        let binding = crate::legs::base0_binding_from_capture_v1(
            &self.profile,
            job,
            &run.tiles,
            run.trace_root,
            crate::produce::base0_activation_leg_root_v1(job),
        )
        .map_err(|e| format!("{e:?}"))?;
        run.execution_root = binding.committed_execution_root;
        run.binding = binding;
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

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<kaspa_hashes::Hash64> {
        let (binding, tiles, _, _) = base0_material_decode_v1(material).ok()?;
        let leaves = leaves_by_position(&binding, &tiles);
        Some(crate::legs::base0_bisect_prefix_state_v1(&binding.job_context, &leaves, index))
    }

    fn refutation_for_index(
        &self,
        material: &[u8],
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let (binding, tiles, logits_rows, generated) =
            base0_material_decode_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        // The ladder narrows to an INDEX; the prover addresses a COORDINATE. `canonical_step_
        // coordinates` is the inverse of the index the ladder counts in, and it answers `None` for
        // the KV aux leaves, which live in their own coordinate space and cannot be opened this way.
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        let leaves = leaves_by_position(&binding, &tiles);
        let step_tiles = crate::legs::Base0StepTilesV1 { leaves, tiles };
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(
            kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 { logits_rows, generated_token_ids: generated },
        );
        // **The prompt, re-derived rather than carried.** An embedding leaf is adjudicated against
        // the token it read, so a refutation with no prompt reads `Unadjudicable` at leaf 0 — and
        // the retained material does not carry the ids. It does not need to: the job's own
        // `job_id` IS the anchor, and the prompt is a pure function of the anchor. Re-deriving is
        // also the safer half of the choice, because a carried prompt would be a second place the
        // producer could disagree with the chain about what it was asked.
        let (_, prompt) = crate::produce::base0_rc_job_v1(
            &binding.shape_profile,
            binding.job_context.job_id,
            self.artifact.shape.vocab,
            binding.job_context.declared_prefill_tokens,
            binding.job_context.exact_decode_tokens,
        );
        let prompt_token_ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        crate::legs::base0_refutation_from_capture_v1(
            &binding.shape_profile.clone(),
            &binding.job_context.clone(),
            &step_tiles,
            binding,
            coord,
            prompt_token_ids,
            Some(pin),
        )
        .map_err(|e| format!("{e:?}"))
    }
}

/// The capture's leaf hashes, laid out BY POSITION over the whole step space.
///
/// The retained material carries `(index, leaf)` pairs and need not arrive ordered, while both the
/// Merkle scheme and the bisection address by position — so the vector is sized from the binding's
/// own `step_leaf_count` and filled by index, exactly as `base0_step_tiles_v1` builds it. Taking
/// the pairs in arrival order instead would silently re-number every leaf.
fn leaves_by_position(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)],
) -> Vec<kaspa_hashes::Hash64> {
    use kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1;
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![kaspa_hashes::Hash64::default(); binding.step_leaf_count as usize];
    for (index, leaf) in tiles {
        if let Some(slot) = leaves.get_mut(*index as usize) {
            *slot = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
        }
    }
    leaves
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

    /// **Both sides of a court, from the same two functions.**
    ///
    /// Nothing in this tree used to construct a `CourtDisclosed`, so a dispute could be opened and
    /// never answered — and the audit's acceptance condition for the adjudication layer was a ROUND
    /// TRIP, because a one-way green is what let two defects hide: a court that convicts everything
    /// and a court that convicts nothing both pass a test that only ever runs one direction.
    ///
    /// So: an honest capture must refute to `NoFaultFound` at the disputed step (the executor
    /// clears itself), and a capture with one tampered lane must convict at that same step (a
    /// challenger takes it) — through the SAME `refutation_for_index`, because a prover only one
    /// side could run would be a prover that decides the verdict.
    #[test]
    fn a_court_goes_both_ways_through_one_prover() {
        use kaspa_consensus_core::palw_step::{canonical_step_coordinates, canonical_step_leaf_index};
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let backend = floor_backend();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xC0117)).expect("job");
        let outcome = backend.execute(&job, &prompt).expect("the floor runs");
        let (binding, tiles, logits, generated) =
            crate::produce::base0_material_decode_v1(&outcome.material).expect("our own material decodes");
        let profile = binding.shape_profile.clone();
        let ctx = binding.job_context.clone();

        // A step both sides can address: a main coordinate whose tile the capture actually holds.
        let (index, coord) = (0..binding.step_leaf_count)
            .find_map(|i| {
                let c = canonical_step_coordinates(&profile, &ctx, i)?;
                let idx = canonical_step_leaf_index(&profile, &ctx, &c)?;
                tiles.iter().any(|(t, _)| *t == idx).then_some((idx, c))
            })
            .expect("the capture holds at least one openable main leaf");

        // --- the executor's side: its own capture clears it at that step ---
        let honest = backend.refutation_for_index(&outcome.material, index).expect("an honest capture opens");
        // One oracle over the WHOLE inventory, proven against its own root — the production path.
        let inventory =
            crate::inventory::base0_inventory_v1(&backend.artifact, kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
                .expect("a real inventory");
        let inv_root = inventory.root();
        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inv_root)
            .expect("the inventory proves against its own root");
        let got = check_execution_step_refutation_v1(&honest, &oracle);
        assert!(
            matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
            "an honest execution must clear itself at the disputed step (leaf {index}, coord {coord:?}); got {got:?}"
        );

        // --- the challenger's side: one tampered lane at that same step convicts ---
        let mut lying_tiles = tiles.clone();
        {
            let slot = lying_tiles.iter_mut().find(|(i, _)| *i == index).expect("the tile is held");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        }
        let lying = crate::legs::Base0StepTilesV1 { leaves: leaves_by_position(&binding, &lying_tiles), tiles: lying_tiles.clone() };
        let lying_binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &lying,
            binding.full_logits_trace_root,
            crate::produce::base0_activation_leg_root_v1(&ctx),
        )
        .expect("a tampered capture still commits to itself");
        let lying_material = borsh::to_vec(&(&lying_binding, &lying_tiles, &logits, &generated)).expect("serializes");
        let guilty = backend.refutation_for_index(&lying_material, index).expect("a tampered capture opens too");
        assert!(
            check_execution_step_refutation_v1(&guilty, &oracle).is_ok(),
            "a tampered lane at the disputed step must convict, not read as no fault"
        );

        // --- and the bisection can find it: the two executions agree before the step and differ at it ---
        let honest_before = backend.bisect_prefix_state(&outcome.material, index).expect("prefix state");
        let lying_before = backend.bisect_prefix_state(&lying_material, index).expect("prefix state");
        assert_eq!(honest_before, lying_before, "the prefix BEFORE the tampered leaf is shared, so a ladder narrows into it");
        let honest_after = backend.bisect_prefix_state(&outcome.material, index + 1).expect("prefix state");
        let lying_after = backend.bisect_prefix_state(&lying_material, index + 1).expect("prefix state");
        assert_ne!(honest_after, lying_after, "and including it, they differ — which is what makes the rung informative");
    }

    /// **The drill fault has to be the fraud a court is FOR, not a mismatch anyone can see.**
    ///
    /// A producer whose roots disagree with its own material is caught by every seat before any
    /// court opens, so injecting that would prove nothing about the court. The fault under test is
    /// the other one: a producer that ran a wrong execution and committed to it honestly. Its
    /// capture verifies against its own claim, and the ONLY way to see the lie is to run the
    /// canonical job yourself and get different roots.
    ///
    /// Both halves are asserted here, because a drill that fails either way is a drill that proves
    /// the wrong thing.
    #[test]
    fn the_drill_fault_is_self_consistent_and_only_a_re_execution_finds_it() {
        let backend = floor_backend();
        let anchor = Hash64::from_u64_word(0xD8111);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
        let honest = backend.execute(&job, &prompt).expect("the floor runs");

        // A leaf the capture actually holds.
        let (binding, tiles, _, _) = crate::produce::base0_material_decode_v1(&honest.material).expect("decodes");
        let leaf = tiles.first().map(|(i, _)| *i).expect("the capture holds a tile");
        let lying = backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drill fault runs");

        // 1. It really is a different execution.
        assert_ne!(lying.execution_root, honest.execution_root, "a drill that commits the honest root disputes nothing");

        // 2. And it is SELF-CONSISTENT: the liar's own material verifies against the liar's own
        //    claim, so no seat check refuses it and the claim licenses normally.
        let its_own = PalwClaimRootsV1 { execution_root: lying.execution_root, trace_root: lying.trace_root };
        assert_eq!(
            backend.verify_material(&lying.material, its_own),
            PalwMaterialVerdictV1::Matches,
            "a fraud a seat can see is not the fraud the court exists for"
        );

        // 3. The only thing that finds it is running the job again.
        assert_ne!(
            backend.execute(&job, &prompt).expect("re-runs").execution_root,
            lying.execution_root,
            "a challenger re-running the canonical job must not reproduce the liar's commitment"
        );
        // …and the honest re-run is byte-identical to the first, which is what makes the
        // comparison evidence rather than noise.
        assert_eq!(backend.execute(&job, &prompt).expect("re-runs").execution_root, honest.execution_root);
        assert_eq!(binding.step_leaf_count, {
            let (b2, _, _, _) = crate::produce::base0_material_decode_v1(&lying.material).expect("decodes");
            b2.step_leaf_count
        });
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
