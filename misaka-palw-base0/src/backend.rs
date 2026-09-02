//! **BASE-0 behind the backend seam.**
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
    base0_execute_for_attempt_capped_v1, base0_material_decode_v1, base0_material_encode_v1, base0_material_matches_claim_v1,
    base0_rc_job_v1,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// The deterministic integer family's backend, bound to one resolved class.
///
/// Constructed from what the CHAIN named (`resolve_class_v1` has already refused anything whose
/// graph or weights disagree with the registration), so by the time a backend exists the question
/// "is this the right class" is settled and the producer never re-asks it.
/// The network id `rc_job_context` stamps into every context of this family. Named here because
/// the free-prompt derivation takes it as an argument and the two must agree byte for byte: a
/// context built under a different id is a context the court recomputes differently.
const RC_NETWORK_ID: &[u8] = b"misaka-palw-rc";

pub struct Base0Backend {
    model_id: String,
    profile: PalwShapeProfileV3,
    artifact: Base0ArtifactV1,
    canonical_job: (u32, u32),
    /// The geometry this class's `artifact_root` was matched under — see `ResolvedClassV1`.
    inventory_geometry: kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1,
    /// **The ladder top this instance measures a served capture against** — the ruleset's
    /// `PalwCourtParamsV2::max_step_leaf_count`, defaulting to the leg's own constant.
    ///
    /// It is also the bound that was missing entirely here. `leaves_by_position` allocates
    /// `step_leaf_count` `Hash64`s, and `step_leaf_count` is a plain `u64` out of a gossiped borsh
    /// blob: `produce::base0_material_matches_claim_v1` writes the whole reasoning down (a few
    /// hundred bytes asking for `2^48` leaves is a `2^54`-byte allocation, which is
    /// `handle_alloc_error` and a process ABORT, not a catchable panic) and the A16 backend guards
    /// both of its own entry points. This backend's two — `bisect_prefix_state` and
    /// `refutation_with_prompt` — did not, and they are reached from the same relayed blob.
    step_ladder_cap: u64,
}

impl Base0Backend {
    pub fn new(resolved: ResolvedClassV1) -> Self {
        Self {
            model_id: resolved.model_id.to_string(),
            profile: resolved.profile,
            artifact: resolved.artifact,
            canonical_job: resolved.canonical_job,
            inventory_geometry: resolved.inventory_geometry,
            step_ladder_cap: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
        }
    }

    /// **The ladder top from the ruleset**, for a caller that holds `PalwCourtParamsV2`. Passing
    /// `max_step_leaf_count` is the only correct argument; [`Base0Backend::new`] passes the leg's
    /// default, which is what every shipped preset froze.
    pub fn with_step_ladder_cap(mut self, max_step_leaf_count: u64) -> Self {
        self.step_ladder_cap = max_step_leaf_count;
        self
    }

    /// The ladder top this instance refuses a served capture above.
    pub fn step_ladder_cap(&self) -> u64 {
        self.step_ladder_cap
    }

    /// The graph, for the callers that still need it directly (the retention writer names the
    /// class in its path). Exposed rather than leaked through the trait: the trait's job is the
    /// three verbs, and a `profile()` on it would be an invitation to reach past them.
    pub fn profile(&self) -> &PalwShapeProfileV3 {
        &self.profile
    }
}

impl Base0Backend {
    /// The one prover behind both [`PalwExecutionBackendV1::refutation_for_index`] and
    /// [`PalwExecutionBackendV1::refutation_for_free_prompt_index`]: `carried` is `None` for an
    /// attempt (the prompt is re-derived from the anchor) and the user's ids for a free prompt.
    fn refutation_with_prompt(
        &self,
        material: &[u8],
        index: u64,
        carried: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let (binding, tiles, logits_rows, generated, _) =
            base0_material_decode_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        // **Bound the count BEFORE `leaves_by_position` allocates from it** — see
        // `step_ladder_cap`. The cap is the ruleset's ladder top, not a module literal.
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.step_ladder_cap {
            return Err("the binding's leaf count is outside the ruleset's ladder".to_string());
        }
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
        // **The prompt: re-derived for an attempt, carried for a free prompt** (ADR-0073 Decision
        // 1c). An embedding leaf is adjudicated against the token it read, so a refutation with no
        // prompt reads `Unadjudicable` at leaf 0 — and the retained material does not carry the
        // ids. An attempt's job does not need them carried: its `job_id` IS the anchor and the
        // prompt is a pure function of it, so a carried copy would be a second place the producer
        // could disagree with the chain about what it was asked. A free prompt is the user's and
        // derives from nothing — the caller hands it over, and one that is not the binding's own
        // is refused HERE: the court reads a wrong list as `InputSetNotCanonical` and files no
        // verdict, which is the shape every free-prompt close on this floor used to take.
        let prompt_token_ids: Vec<u32> = match carried {
            Some(ids) => {
                if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(ids) != binding.job_context.prompt_token_ids_hash {
                    return Err("the carried prompt is not the one this capture's job context commits to".to_string());
                }
                ids.to_vec()
            }
            None => {
                let (_, prompt) = crate::produce::base0_rc_job_v1(
                    &binding.shape_profile,
                    binding.job_context.job_id,
                    self.artifact.shape.vocab,
                    binding.job_context.declared_prefill_tokens,
                    binding.job_context.exact_decode_tokens,
                );
                prompt.iter().map(|t| *t as u32).collect()
            }
        };
        crate::legs::base0_refutation_from_capture_v1(
            &binding.shape_profile.clone(),
            &binding.job_context.clone(),
            &step_tiles,
            binding,
            coord,
            prompt_token_ids,
            Some(pin),
            None,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

/// **The floor's kernels, as a seat's interval replay needs them** (ADR-0077 Decision 8).
///
/// A thin adapter and deliberately not a second execution path: the window is walked by
/// [`crate::fp_interval::base0_fp_replay_interval_v1`], which is the capture's own loop, and this
/// supplies only the two things a family owns — how to restore the cache the interval resumes from
/// and how to run one forward call. A family that re-implemented the coordinate rule here would
/// commit its replay at coordinates the leg does not use, and every comparison would fail for a
/// reason that is not the producer's.
struct Base0IntervalKernels<'a> {
    artifact: &'a Base0ArtifactV1,
}

impl crate::fp_interval::Base0FpIntervalKernelsV1 for Base0IntervalKernels<'_> {
    fn replay_interval(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &crate::fp_interval::Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
    ) -> Result<Vec<(u64, Hash64)>, String> {
        use crate::engine::{Base0Engine, KvCache};
        let engine = Base0Engine::new(self.artifact);
        // **ADR-0049 Decision F's obligation before the first token, on the SEAT's side too.** A
        // seat that replayed under a graph the profile does not name would recompute rows the
        // court recomputes differently and report a fault against an honest producer.
        for kv_len in 1..=2 {
            crate::plan::base0_check_graph_v1(engine.plan().map_err(|e| format!("{e:?}"))?, profile, &self.artifact.shape, kv_len)
                .map_err(|e| format!("{e:?}"))?;
        }
        let mut cache = match start {
            crate::fp_interval::Base0FpIntervalStartV1::Genesis { .. } => KvCache::new(self.artifact),
            crate::fp_interval::Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                let positions = kaspa_consensus_core::palw_state_chunk_map::integer_kv_positions_at_v1(ctx, *covered_decode_call);
                let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions).map_err(|e| format!("{e:?}"))?;
                KvCache::from_state_chunks(self.artifact, &geometry, chunks).map_err(|e| format!("{e:?}"))?
            }
        };
        let vocab = self.artifact.shape.vocab;
        crate::fp_interval::base0_fp_replay_interval_v1(profile, ctx, start, first_call, last_call, |token, position| {
            if token >= vocab {
                return Err(format!("token {token} is outside this class's vocabulary of {vocab}"));
            }
            let (logits, probe) = engine.forward_token_probed(&mut cache, token, position).map_err(|e| format!("{e:?}"))?;
            Ok((logits, crate::legs::base0_captured_rows_v1(&probe)))
        })
    }
}

impl Base0Backend {
    /// The cadence this family checkpoints at — a class fact, read from the family's own
    /// registration and never off a capture. The seat's interval count is derived from it and two
    /// chain numbers ([`crate::fp_interval::base0_fp_interval_count_for_v1`]).
    fn checkpoint_interval(&self) -> u32 {
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1
    }

    /// **ADR-0077 SA-6, at the job boundary.** The floor derives its weights from a seed and holds
    /// them in owned memory, so there is no mapped page to fault on — stated rather than assumed,
    /// because the two model tiers answer this differently and a caller reads one seam.
    fn artifact_read_probe_v1(&self) -> Result<(), String> {
        Ok(())
    }
}

impl PalwExecutionBackendV1 for Base0Backend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        Ok(base0_rc_job_v1(&self.profile, anchor, self.artifact.shape.vocab, self.canonical_job.0, self.canonical_job.1))
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let run = base0_execute_for_attempt_capped_v1(&self.artifact, &self.profile, job, prompt, self.step_ladder_cap)
            .map_err(|e| e.to_string())?;
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

    /// The non-streaming verb IS the streaming one with a callback that does nothing — never the
    /// reverse (ADR-0077 Decision 2). One inference, one capture, one commitment.
    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_streaming(job, prompt_tokens, &mut |_| {})
    }

    fn execute_free_prompt_streaming(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_job_context_v3, PalwFpClassFactsV3, PalwFpRunFactsV3};
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;

        // ADR-0077 SA-6: an artifact this host can no longer read is a job failure named at the
        // boundary, not a fault taken three layers into a kernel.
        self.artifact_read_probe_v1()?;

        // The job declares how many tokens it is a job about, and the caller hands the tokens. If
        // those two disagree the derivation would build a context for a run that did not happen —
        // caught here, where the caller can still say which one it meant.
        if job.prompt_tokens as usize != prompt_tokens.len() {
            return Err(format!("the job declares {} prompt tokens and {} were supplied", job.prompt_tokens, prompt_tokens.len()));
        }

        // **What this class is, in the terms the derivation asks for.** The integer family's
        // identity IS its graph: `rc_job_context` leaves `model_profile_id`, the runtime hashes and
        // the CU ruleset at their defaults on the attempt lane for the same reason, and a value
        // invented here would be one the court does not recompute.
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: self.profile.shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        };

        // **This family decodes a declared budget, so the count and the stop reason are known
        // before the run rather than after it.** ADR-0044 Decision 7's early stop is a property of
        // a sampler that can emit end-of-generation; `base0_execute_for_attempt_v1` runs
        // `exact_decode_tokens` and returns. `ExactBudgetReached` is therefore the only honest stop
        // reason here, and the context builder enforces the pairing (`executed == limit`) rather
        // than trusting this comment.
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };

        // The context the COURT will recompute against — built first, and then run under. The
        // roots in `shape` are placeholders and are not read: the context is the job's shape, the
        // roots belong to the execution root derived from it afterwards.
        let ctx = palw_fp_job_context_v3(job, &class, &shape, RC_NETWORK_ID).map_err(|e| format!("{e:?}"))?;

        let run = crate::produce::base0_execute_for_attempt_streaming_v1(&self.artifact, &self.profile, &ctx, prompt_tokens, on_token)
            .map_err(|e| e.to_string())?;

        // The four legs, measured. They exist on every attempt this family makes — it is what makes
        // its claims adjudicable — and this is the first caller that needed them by name.
        //
        // The checkpoint and step legs are the DERIVED roots, not the merkle roots the binding
        // stores: `committed_execution_root` is built from `checkpoint_leg_root_v2` and
        // `step_leg_root_v1` over those merkle roots plus their counts and profiles. Committing the
        // bare merkle roots here type-checks, runs, and produces an execution root the court
        // recomputes differently — which the round trip below caught.
        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
        let facts = PalwFpRunFactsV3 {
            full_logits_trace_root: run.binding.full_logits_trace_root,
            activation_leg_root: run.binding.activation_leg_root,
            checkpoint_leg_root,
            step_leg_root,
            // The price (ADR-0074 Decision 5): read off the binding, never declared.
            step_leaf_count: run.binding.step_leaf_count,
            ..shape
        };
        let material = base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
        Ok(kaspa_consensus_core::palw_backend::PalwFpRunV1 {
            outcome: PalwExecutionOutcomeV1 {
                trace_root: run.trace_root,
                output_root: run.output_root,
                execution_root: run.execution_root,
                trace_manifest_root: run.trace_manifest_root,
                trace_chunk_count: run.trace_chunk_count,
                material,
            },
            facts,
            output_token_ids: run.generated_token_ids,
        })
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Ok(decoded) = base0_material_decode_v1(material) else {
            // Bytes that do not decode are bytes that were not served — the seat's honest
            // `Unavailable`, not an accusation that the producer computed the wrong thing.
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // **Which question did this answer?** The roots say the arithmetic is self-consistent;
        // they cannot say the job was the one this claim's block asked for. A capture carries its
        // own `job_id`, so reading the anchor from it and then checking the capture against it
        // would be a tautology — and it is precisely the tautology that made a gossiped capture
        // re-mineable by anyone, forever, with no inference. The caller derives the anchor from the
        // BLOCK; here we only insist the capture answers it.
        if claim.anchor != Hash64::default() && decoded.0.job_context.job_id != claim.anchor {
            return PalwMaterialVerdictV1::Mismatch;
        }
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
        let mut run = base0_execute_for_attempt_capped_v1(&self.artifact, &self.profile, job, prompt, self.step_ladder_cap)
            .map_err(|e| e.to_string())?;
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
            &run.checkpoints,
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

    /// BASE-0 is the family that can take a court's turn: both methods below are real (audit3 H4).
    fn supports_court(&self) -> bool {
        true
    }

    fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
        let (binding, ..) = base0_material_decode_v1(material).ok()?;
        Some(kaspa_consensus_core::palw_backend::PalwCaptureShapeV1 {
            job_context: binding.job_context.clone(),
            step_leaf_count: binding.step_leaf_count,
        })
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<kaspa_hashes::Hash64> {
        let (binding, tiles, _, _, _) = base0_material_decode_v1(material).ok()?;
        // **Bound the count BEFORE `leaves_by_position` allocates from it** — see
        // `step_ladder_cap`. Without this the relayed blob decides the allocation.
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.step_ladder_cap {
            return None;
        }
        let leaves = leaves_by_position(&binding, &tiles);
        Some(crate::legs::base0_bisect_prefix_state_v1(&binding.job_context, &leaves, index))
    }

    fn refutation_for_index(
        &self,
        material: &[u8],
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.refutation_with_prompt(material, index, None)
    }

    fn refutation_for_free_prompt_index(
        &self,
        material: &[u8],
        index: u64,
        prompt_token_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.refutation_with_prompt(material, index, Some(prompt_token_ids))
    }

    // ---- ADR-0077 Decision 8: the interval seam -------------------------------------------
    //
    // The count a seat draws from comes from CHAIN data (`fp_interval_count_for`); the count read
    // off a capture (`fp_interval_count`) exists so the two can be pinned equal on every capture
    // this family produces, and for nothing else. An executor whose reported count a seat trusted
    // could shrink it and predict the draw.

    fn fp_interval_count(&self, capture: &[u8]) -> Option<u32> {
        let (binding, ..) = base0_material_decode_v1(capture).ok()?;
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(&binding, self.checkpoint_interval())
            .ok()
            .map(|g| g.interval_count)
    }

    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        crate::fp_interval::base0_fp_interval_count_for_v1(prompt_tokens, decode_tokens_executed, self.checkpoint_interval())
    }

    fn open_fp_interval(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        let material = base0_material_decode_v1(capture).map_err(|_| "the capture does not decode".to_string())?;
        crate::fp_interval::base0_open_fp_interval_v1(&material, index, prompt_token_ids, self.checkpoint_interval())
            .map_err(|e| e.to_string())
    }

    fn verify_fp_interval_opening(
        &self,
        opening: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        work_leaves: u64,
    ) -> kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1 {
        crate::fp_interval::base0_verify_fp_interval_opening_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            self.checkpoint_interval(),
            &Base0IntervalKernels { artifact: &self.artifact },
        )
    }

    fn operand_openings_for(
        &self,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
        let inventory = crate::inventory::base0_inventory_v1(&self.artifact, self.inventory_geometry).map_err(|e| format!("{e:?}"))?;
        let recorder = kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1::new(inventory.operands());
        // **The verdict is not ours to read here.** This runs the adjudicator only to learn WHICH
        // rows it resolves, and it resolves the same rows whether the step clears or convicts —
        // so an error return (including `NoFaultFound`, which is what an honest party's own close
        // produces) is not a reason to withhold the openings. The chain re-runs the same check
        // against these rows and says what it means.
        let _ = kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_v1(refutation, &recorder);
        recorder.openings().ok_or_else(|| "the inventory cannot open a row its own oracle resolved".to_string())
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

    /// The class facts the integer family answers for. `rc_job_context` leaves the model and
    /// runtime identities at their defaults on the attempt lane — the graph IS the identity here —
    /// and inventing values for the free-prompt lane would file facts no court recomputes.
    fn floor_class_facts(backend: &Base0Backend) -> kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
        kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: backend.profile().shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        }
    }

    fn free_prompt_job(
        backend: &Base0Backend,
        prompt_tokens: u32,
        decode_token_limit: u32,
    ) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: backend.profile().shape_profile_id(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 1234,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(0x71),
            prompt_tokens,
            decode_token_limit,
            max_context_tokens: backend.profile().n_ctx,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
        }
    }

    /// **The free-prompt run commits a root the court will recompute — the round trip.**
    ///
    /// This is the only property that decides whether this family can serve the free-prompt lane
    /// at all. `adjudicate_court_close_v2` binds a refutation against the CLAIM's execution root,
    /// and `palw_fp_execution_root_v3` is what a verifier recomputes from the claim's context and
    /// leg roots. If that value and the root the producer actually committed while running were
    /// two different numbers, an honest producer's every dispute would die at
    /// `ExecutionRootMismatch` — fail-closed, unconvictable, and unpayable.
    ///
    /// So the assertion is equality between the derivation and the run, not that either is
    /// non-zero. A one-way check would pass with both sides wrong in the same way.
    #[test]
    fn a_free_prompt_run_commits_the_root_the_derivation_recomputes() {
        use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_execution_root_v3, palw_fp_job_context_v3};
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;

        let backend = floor_backend();
        // The caller's tokens — the whole point of the lane. Any ids inside the vocabulary do:
        // this asserts a binding, not an answer.
        let prompt: Vec<usize> = vec![7, 11, 13, 17];
        let job = free_prompt_job(&backend, prompt.len() as u32, 2);

        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
        let (outcome, facts) = (&run.outcome, &run.facts);

        // Recompute the way a verifier does: from the job and the facts, with no access to the run.
        let class = floor_class_facts(&backend);
        let context = palw_fp_job_context_v3(&job, &class, facts, RC_NETWORK_ID).expect("the finished run implies a context");
        assert_eq!(
            palw_fp_execution_root_v3(&context, facts),
            outcome.execution_root,
            "the derivation and the run must agree, or the court convicts the honest"
        );

        // The facts are measurements, not defaults: a run that reported four zero legs would pass
        // the equality above and mean nothing.
        assert_ne!(facts.full_logits_trace_root, Hash64::default());
        assert_ne!(facts.step_leg_root, Hash64::default());
        assert_eq!(facts.decode_tokens_executed, job.decode_token_limit);
        assert_eq!(facts.stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
        // The answer, which is the other half of the one inference and the reason anyone ran it.
        assert_eq!(run.output_token_ids.len(), job.decode_token_limit as usize);
    }

    /// **FP-R2: the run becomes a commitment, and every priced field is derived.**
    ///
    /// The assembly is where a producer would cheat if it could — a chosen CU, a chosen schedule,
    /// a chosen execution root — so this asserts each against its own derivation rather than
    /// against a literal. The CU especially: invariant F7 puts pricing at assembly and not at the
    /// worker, and a commitment carrying a price the weights do not produce is refused by
    /// `validate_stateless_v3` on the way in.
    #[test]
    fn a_free_prompt_run_assembles_a_commitment_whose_fields_are_all_derived() {
        use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_commitment_v3, palw_fp_job_context_v3};

        let backend = floor_backend();
        let prompt: Vec<usize> = vec![3, 5, 8, 13, 21];
        let job = free_prompt_job(&backend, prompt.len() as u32, 2);
        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");

        let class = floor_class_facts(&backend);
        let retention = 4_096;
        let commitment = palw_fp_commitment_v3(&job, &class, &run, RC_NETWORK_ID, retention).expect("a finished run commits");

        // Priced by the capture's leaf count (ADR-0074 Decision 5) — read off the binding the run
        // committed, never carried up as a number the executor chose.
        assert_eq!(commitment.work_leaves, run.facts.step_leaf_count);
        assert!(commitment.work_leaves > 0, "a run that touched no leaf committed nothing");

        // The schedule is a function of the context and the counts. Recomputing it the way a
        // verifier does must land on the same value.
        let context = palw_fp_job_context_v3(&job, &class, &run.facts, RC_NETWORK_ID).expect("the run implies a context");
        let (schedule, _) = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(
            &context.context_hash(),
            job.prompt_tokens,
            run.facts.decode_tokens_executed,
        );
        assert_eq!(commitment.schedule_root, schedule);

        // Adjudicable, which is the property a null root destroys: `apply_palw_transition_v3`
        // refuses a commitment whose execution root is the default, and the lane was fail-closed
        // on exactly that before the derivation existed.
        assert_ne!(commitment.execution_root, Hash64::default());
        assert_eq!(commitment.execution_root, run.outcome.execution_root);

        // The retention promise is the caller's, and it is carried verbatim: a producer that
        // shortened it here would be promising the panel less than the operator said.
        assert_eq!(commitment.trace_retention_daa, retention);
        assert_eq!(commitment.trace_chunk_count, run.outcome.trace_chunk_count);
    }

    /// **The seam produces what the header needs, end to end** — and the floor still runs through
    /// it, which is the only thing that makes the refactor safe to land.
    #[test]
    fn the_floor_executes_through_the_seam() {
        let backend = floor_backend();
        let anchor = Hash64::from_u64_word(0x5EA_u64);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the floor's canonical job runs");
        assert_ne!(outcome.trace_root, Hash64::default());
        assert_ne!(outcome.execution_root, Hash64::default());
        assert!(!outcome.material.is_empty(), "a producer that retained nothing could not answer a challenge");

        // The seat's half, against the roots this very run committed.
        let claim =
            PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root, anchor: Hash64::default() };
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
        use kaspa_consensus_core::palw_step_refute::{check_execution_step_refutation_v1, PalwStepRefuteError};

        let backend = floor_backend();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xC0117)).expect("job");
        let outcome = backend.execute(&job, &prompt).expect("the floor runs");
        let (binding, tiles, logits, generated, checkpoint_chunks) =
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
        // The run's OWN checkpoints, re-derived from the material it served: this fixture is a
        // producer lying about one tile, not about its checkpoint leg.
        let checkpoints =
            crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(&ctx, &profile, &binding.checkpoint_profile, &checkpoint_chunks)
                .expect("the served chunks re-derive");
        let lying_binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &lying,
            &checkpoints,
            binding.full_logits_trace_root,
            crate::produce::base0_activation_leg_root_v1(&ctx),
        )
        .expect("a tampered capture still commits to itself");
        let lying_material =
            borsh::to_vec(&(&lying_binding, &lying_tiles, &logits, &generated, &checkpoint_chunks)).expect("serializes");
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

    /// **A capture answers a question, and the question has to be this block's.**
    ///
    /// The roots say the arithmetic is self-consistent. They do not say which job was run, so a
    /// gossiped capture used to be a re-usable asset: mine a fresh block, announce the borrowed
    /// roots, and every check agreed — the seat compared roots (they match, they are real roots)
    /// and the challenger re-executed the anchor the capture itself named (it reproduces, it is a
    /// real execution). One inference, unlimited blocks, by parties that ran nothing.
    ///
    /// So the claim carries the anchor its own block implies, and material that answers a
    /// different anchor is a `Mismatch` — the same verdict a wrong root gets, because it is the
    /// same lie: this is not the work this claim was paid for.
    #[test]
    fn material_that_answers_another_blocks_job_is_a_mismatch() {
        let backend = floor_backend();
        let anchor = Hash64::from_u64_word(0xA0C0DE);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the floor runs");

        // Its own block: roots and anchor both this run's.
        let mine = PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root, anchor };
        assert_eq!(backend.verify_material(&outcome.material, mine), PalwMaterialVerdictV1::Matches);

        // Somebody else's block, borrowing this run's roots. The roots are genuine and the
        // execution is genuine; what is missing is that anyone ever asked this question here.
        let borrowed = PalwClaimRootsV1 { anchor: Hash64::from_u64_word(0xB0_44_0D), ..mine };
        assert_eq!(
            backend.verify_material(&outcome.material, borrowed),
            PalwMaterialVerdictV1::Mismatch,
            "a capture that answers another block's job must not license this one"
        );

        // And the derivation itself is a function of the block, so two blocks never share an
        // anchor — which is what makes the check above bite at all.
        let bond = kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(9), 0);
        let net = Hash64::from_u64_word(0x7E57);
        // Same bucket for both, so what this compares is the pre-pow hash and not the bucket.
        let a =
            backend.job_anchor_v1(net, Hash64::from_u64_word(1), Hash64::from_u64_word(0xC), &bond, 0).expect("the floor has a job");
        let b =
            backend.job_anchor_v1(net, Hash64::from_u64_word(2), Hash64::from_u64_word(0xC), &bond, 0).expect("the floor has a job");
        assert_ne!(a, b, "two blocks must ask different questions, or re-mining is free again");
    }

    /// **A close carries the rows the court reads — exactly those, and it still goes both ways.**
    ///
    /// `a_court_goes_both_ways_through_one_prover` proves the ARITHMETIC by handing the adjudicator
    /// the entire inventory, which no real close can do: a close has a byte ceiling and a class's
    /// weights do not fit under it. So the panel's actual path is the one under test here — ask the
    /// backend for the openings, prove those alone against the class root, and require the same two
    /// verdicts. A `Vec::new()` here (what the panel shipped before) reads as `NoFaultFound` on the
    /// guilty side, so a court wired that way could never convict; that is the defect this pins.
    #[test]
    fn a_close_carries_exactly_the_rows_the_court_reads() {
        use kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1;
        use kaspa_consensus_core::palw_step::{canonical_step_coordinates, canonical_step_leaf_index};
        use kaspa_consensus_core::palw_step_refute::{check_execution_step_refutation_v1, PalwStepRefuteError};

        let backend = floor_backend();
        let class_root = crate::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x09E_4ED)).expect("job");
        let outcome = backend.execute(&job, &prompt).expect("the floor runs");
        let (binding, tiles, logits, generated, checkpoint_chunks) =
            crate::produce::base0_material_decode_v1(&outcome.material).expect("our own material decodes");
        let profile = binding.shape_profile.clone();
        let ctx = binding.job_context.clone();
        let (index, _) = (0..binding.step_leaf_count)
            .find_map(|i| {
                let c = canonical_step_coordinates(&profile, &ctx, i)?;
                let idx = canonical_step_leaf_index(&profile, &ctx, &c)?;
                tiles.iter().any(|(t, _)| *t == idx).then_some((idx, c))
            })
            .expect("the capture holds at least one openable main leaf");

        // --- honest: the recorded rows clear the step, and the SAME rows prove against the root ---
        let honest = backend.refutation_for_index(&outcome.material, index).expect("an honest capture opens");
        let honest_openings = backend.operand_openings_for(&honest).expect("the class opens its own rows");
        let honest_oracle =
            PalwProvenOperandsV1::from_openings_v1(&honest_openings, class_root).expect("recorded rows prove against the class root");
        assert!(
            matches!(check_execution_step_refutation_v1(&honest, &honest_oracle), Err(PalwStepRefuteError::NoFaultFound)),
            "an honest close must clear itself from the rows it carried"
        );

        // ...and it carried a close-sized set, not the artifact. This is the whole reason the
        // recording oracle exists, so it is asserted rather than assumed.
        let whole = crate::inventory::base0_inventory_v1(&backend.artifact, backend.inventory_geometry).expect("a real inventory");
        assert!(
            honest_openings.len() < whole.operands().len(),
            "a close that carries every row is a close no ceiling admits ({} of {})",
            honest_openings.len(),
            whole.operands().len()
        );
        assert!(!honest_openings.is_empty(), "a step that reads no weight at all would make the artifact root decorative");

        // --- guilty: one tampered lane at that same step, closed the same way, convicts ---
        let mut lying_tiles = tiles.clone();
        let pos = lying_tiles.iter().position(|(t, _)| *t == index).expect("the disputed tile is held");
        lying_tiles[pos].1.values_le[0] = lying_tiles[pos].1.values_le[0].wrapping_add(1);
        let lying = crate::legs::Base0StepTilesV1 { leaves: leaves_by_position(&binding, &lying_tiles), tiles: lying_tiles.clone() };
        let checkpoints =
            crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(&ctx, &profile, &binding.checkpoint_profile, &checkpoint_chunks)
                .expect("the served chunks re-derive");
        let lying_binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &lying,
            &checkpoints,
            binding.full_logits_trace_root,
            crate::produce::base0_activation_leg_root_v1(&ctx),
        )
        .expect("a tampered capture still commits to itself");
        let lying_material =
            borsh::to_vec(&(&lying_binding, &lying_tiles, &logits, &generated, &checkpoint_chunks)).expect("serializes");
        let guilty = backend.refutation_for_index(&lying_material, index).expect("a tampered capture opens too");
        let guilty_openings = backend.operand_openings_for(&guilty).expect("the class opens the guilty step's rows too");
        let guilty_oracle =
            PalwProvenOperandsV1::from_openings_v1(&guilty_openings, class_root).expect("recorded rows prove against the class root");
        assert!(
            check_execution_step_refutation_v1(&guilty, &guilty_oracle).is_ok(),
            "a tampered lane must convict from the rows the close carried, not read as no fault"
        );

        // --- and the empty set the panel used to send convicts NOTHING: the bug, pinned ---
        let empty = PalwProvenOperandsV1::from_openings_v1(&[], class_root).expect("an empty set is well-formed");
        assert!(
            check_execution_step_refutation_v1(&guilty, &empty).is_err(),
            "a close carrying no operands must not be able to convict — if it can, the court is reading something it was not given"
        );
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
        let (binding, tiles, _, _, _) = crate::produce::base0_material_decode_v1(&honest.material).expect("decodes");
        let leaf = tiles.first().map(|(i, _)| *i).expect("the capture holds a tile");
        let lying = backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drill fault runs");

        // 1. It really is a different execution.
        assert_ne!(lying.execution_root, honest.execution_root, "a drill that commits the honest root disputes nothing");

        // 2. And it is SELF-CONSISTENT: the liar's own material verifies against the liar's own
        //    claim, so no seat check refuses it and the claim licenses normally.
        let its_own =
            PalwClaimRootsV1 { execution_root: lying.execution_root, trace_root: lying.trace_root, anchor: Hash64::default() };
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
            let (b2, _, _, _, _) = crate::produce::base0_material_decode_v1(&lying.material).expect("decodes");
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
        let claim =
            PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root, anchor: Hash64::default() };

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

    /// **A free-prompt capture adjudicates in the same court as an attempt's** (ADR-0073
    /// Decision 1; the FP twin of ADR-0069's drill). The floor runs a caller's prompt; the capture
    /// it retains verifies against the claim's roots under the FP anchor — the job id — and under
    /// no attempt anchor; handed the user's prompt, every leaf of the step space clears the honest
    /// capture; the attempt-path prover, handed the same capture, derives a foreign prompt and the
    /// court reads NO verdict at all (the shape every FP close used to take); a foreign prompt is
    /// refused at the prover; and one tampered lane convicts at kernels of different shapes.
    #[test]
    fn every_free_prompt_leaf_adjudicates_and_a_tampered_one_convicts() {
        use kaspa_consensus_core::palw_artifact::{open_artifact_leaf_v1, PalwProvenOperandsV1};
        use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_job_context_v3, PalwFpRunFactsV3};
        use kaspa_consensus_core::palw_freeprompt_v3::{fp_job_id_v3, PalwFpStopReasonV3};
        use kaspa_consensus_core::palw_step_refute::{check_execution_step_refutation_v1, PalwStepRefuteError};
        use kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2;

        let backend = floor_backend();
        let prompt: Vec<usize> = vec![3, 5, 8, 13, 21];
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut job = free_prompt_job(&backend, prompt.len() as u32, 2);
        // The user's prompt, hash-bound to the job the way the 0x4a payload binds it.
        job.prompt_token_ids_hash = prompt_token_ids_hash_v2(&ids);
        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
        let capture = run.outcome.material.clone();
        let (binding, ..) = base0_material_decode_v1(&capture).expect("an FP capture is the family tuple");
        assert_eq!(binding.job_context.job_id, fp_job_id_v3(&job), "an FP capture names its job by the job id");
        assert_eq!(binding.job_context.prompt_token_ids_hash, job.prompt_token_ids_hash);

        // The seat's half: the FP anchor IS the job id (ADR-0073 Decision 1b), and the capture
        // verifies under it — and under nothing an attempt-lane derivation could produce, which
        // is what kept every FP close from assembling.
        let roots = PalwClaimRootsV1 {
            execution_root: run.outcome.execution_root,
            trace_root: run.outcome.trace_root,
            anchor: fp_job_id_v3(&job),
        };
        assert_eq!(backend.verify_material(&capture, roots), PalwMaterialVerdictV1::Matches);
        let attempt_anchor = PalwClaimRootsV1 { anchor: Hash64::from_u64_word(0xA71E), ..roots };
        assert_ne!(backend.verify_material(&capture, attempt_anchor), PalwMaterialVerdictV1::Matches);

        // One proven oracle over the whole inventory — the production path a close takes.
        let inventory = crate::inventory::base0_inventory_v1(&backend.artifact, backend.inventory_geometry).expect("inventory");
        let openings: Vec<_> =
            (0..inventory.operands().len()).map(|i| open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap()).collect();
        let oracle = PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).expect("every row proves against its root");

        // The sweep: every leaf clears the honest capture when the prover holds the user's prompt.
        for index in 0..binding.step_leaf_count {
            let refutation = backend
                .refutation_for_free_prompt_index(&capture, index, &ids)
                .unwrap_or_else(|e| panic!("leaf {index} must open from an honest FP capture: {e}"));
            let got = check_execution_step_refutation_v1(&refutation, &oracle);
            assert!(
                matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
                "an honest free-prompt execution must clear itself at leaf {index} (coord {:?}): got {got:?}",
                refutation.output_preimage.coord
            );
        }

        // The attempt-path prover derives the prompt from the anchor. For an FP capture that is a
        // foreign list, and the court's guard reads it as NO verdict — a close that assembles and
        // never adjudicates. That is ADR-0073 §4 G1, kept as the negative control.
        let from_anchor = backend.refutation_for_index(&capture, 0).expect("the attempt-path prover still assembles");
        let got = check_execution_step_refutation_v1(&from_anchor, &oracle);
        assert!(
            matches!(got, Err(PalwStepRefuteError::InputSetNotCanonical(..))),
            "the derived prompt is not the user's: got {got:?}"
        );
        // …and a foreign prompt handed to the FP prover is refused before any court sees it.
        let mut foreign = ids.clone();
        foreign[0] ^= 1;
        assert!(
            backend.refutation_for_free_prompt_index(&capture, 0, &foreign).is_err(),
            "a prompt that is not the binding's is refused"
        );

        // The other direction: one tampered lane convicts. The fault is injected into the SAME
        // job context `execute_free_prompt` ran under, so the lying capture is an FP capture.
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };
        let ctx = palw_fp_job_context_v3(&job, &floor_class_facts(&backend), &shape, RC_NETWORK_ID).expect("the FP context");
        assert_eq!(ctx.job_id, binding.job_context.job_id, "the drill runs the job the honest capture ran");
        for &index in &[0u64, binding.step_leaf_count / 3, binding.step_leaf_count - 1] {
            let lying = backend.execute_with_injected_fault(&ctx, &prompt, index).expect("a tampered FP capture still commits");
            assert_ne!(lying.execution_root, run.outcome.execution_root, "the lie commits its own root");
            let refutation =
                backend.refutation_for_free_prompt_index(&lying.material, index, &ids).expect("a tampered capture opens too");
            let openings = backend.operand_openings_for(&refutation).expect("the prover opens what the court resolves");
            let proven = PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).expect("recorded openings prove");
            assert!(
                check_execution_step_refutation_v1(&refutation, &proven).is_ok(),
                "a tampered lane at leaf {index} of a free-prompt capture must convict, not read as no fault"
            );
        }
    }

    /// **Decision 2: the stream is the run, not a replay of it.**
    ///
    /// The streamed ids must be the committed ids, in decode order, and the run they come out of
    /// must be the run whose roots the caller gets — a worker that streamed one answer and
    /// committed another is exactly what F1 dies on. Asserted three ways: same ids, same order,
    /// and the same roots as the non-streaming verb over the same job, which is what makes "one
    /// inference, both halves" checkable rather than asserted.
    #[test]
    fn the_streamed_ids_are_the_committed_ids_of_the_same_run() {
        let backend = floor_backend();
        let prompt: Vec<usize> = vec![7, 11, 13, 17];
        let job = free_prompt_job(&backend, prompt.len() as u32, 4);

        let mut streamed: Vec<u32> = Vec::new();
        let run = backend
            .execute_free_prompt_streaming(&job, &prompt, &mut |id| streamed.push(id))
            .expect("the floor streams a caller's prompt");
        assert_eq!(streamed, run.output_token_ids, "the stream is the answer, id for id and in order");
        assert_eq!(streamed.len(), job.decode_token_limit as usize);

        // …and it is the same inference the non-streaming verb performs. A default implementation
        // that ran the job and replayed the ids afterwards would also satisfy the equality above;
        // this is the part that says the callback rides the decode loop, because the roots of the
        // two runs are identical and the streaming one produced its ids before it had any roots.
        let plain = backend.execute_free_prompt(&job, &prompt).expect("the floor runs the same job");
        assert_eq!(plain.outcome.execution_root, run.outcome.execution_root);
        assert_eq!(plain.output_token_ids, run.output_token_ids);
    }

    /// **The interval seam, through the trait a node holds** (ADR-0077 Decision 8, P-08).
    ///
    /// The seat's count comes from the chain (`fp_interval_count_for`) and must equal the one the
    /// capture implies; every drawn interval opens and verifies; and the bytes a seat fetches are
    /// the opening, never the capture.
    #[test]
    fn the_backend_opens_and_verifies_the_intervals_a_seat_draws() {
        use kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1;
        use kaspa_consensus_core::palw_fp_interval_v1::{palw_fp_interval_draw_v1, PALW_FP_SEAT_INTERVAL_SAMPLES_V1};

        let backend = floor_backend();
        let prompt: Vec<usize> = vec![7, 11, 13, 17];
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut job = free_prompt_job(&backend, prompt.len() as u32, 5);
        job.prompt_token_ids_hash = kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids);
        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");

        // The count a seat draws against: two chain numbers and the class's own cadence. Nothing
        // in it came from the capture.
        let from_chain = backend
            .fp_interval_count_for(job.prompt_tokens, run.facts.decode_tokens_executed)
            .expect("the floor has a free-prompt path");
        assert_eq!(from_chain, backend.fp_interval_count(&run.outcome.material).expect("the capture reads"));
        assert!(from_chain > 1, "the fixture exercises both the genesis and the anchored arms");

        let claim = PalwClaimRootsV1 {
            execution_root: run.outcome.execution_root,
            trace_root: run.outcome.trace_root,
            // The FP lane's anchor is the job id the derivation produced, which is what a seat
            // reads off the accepted commitment.
            anchor: Hash64::default(),
        };
        let draw = palw_fp_interval_draw_v1(
            &job.network_domain,
            &job.anchor_block,
            &Hash64::from_u64_word(0xC1A1),
            2,
            PALW_FP_SEAT_INTERVAL_SAMPLES_V1,
            from_chain,
        );
        assert!(!draw.is_empty());
        for index in draw {
            let opening =
                backend.open_fp_interval(&run.outcome.material, index, &ids).unwrap_or_else(|e| panic!("interval {index} opens: {e}"));
            assert!(
                opening.len() < run.outcome.material.len(),
                "a seat fetches the opening ({} bytes), never the capture ({} bytes)",
                opening.len(),
                run.outcome.material.len()
            );
            assert_eq!(
                backend.verify_fp_interval_opening(&opening, claim, index, &ids, run.facts.step_leaf_count),
                PalwFpIntervalVerdictV1::Valid,
                "interval {index}"
            );
        }
    }

    /// **A gossiped leaf count decides an allocation, so it is bounded before it is believed.**
    ///
    /// `leaves_by_position` builds `vec![Hash64::default(); step_leaf_count]`, and
    /// `step_leaf_count` is a plain `u64` inside a relayed borsh blob. `2^48` of them is a
    /// `2^54`-byte allocation — under `isize::MAX`, so it is `handle_alloc_error` and a process
    /// ABORT rather than a catchable panic, on every seat the blob reached. `produce`'s claim
    /// checker and the A16 backend both bound it; these two entry points did not.
    #[test]
    fn a_capture_claiming_a_leaf_count_past_the_ladder_is_refused_before_it_allocates() {
        let backend = floor_backend();
        assert_eq!(
            backend.step_ladder_cap(),
            kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            "the shipped default is the leg's own ladder top — nothing here arms a deeper one"
        );
        let anchor = Hash64::from_u64_word(0xB0_11_3D);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the floor runs");
        assert!(backend.bisect_prefix_state(&outcome.material, 1).is_some(), "the honest capture answers");

        let (mut binding, tiles, rows, generated, chunks) =
            base0_material_decode_v1(&outcome.material).expect("the honest capture decodes");
        let honest_count = binding.step_leaf_count;
        for bent_count in [1u64 << 48, 0, kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES + 1] {
            binding.step_leaf_count = bent_count;
            let bent = borsh::to_vec(&(&binding, &tiles, &rows, &generated, &chunks)).expect("serializes");
            assert_eq!(backend.bisect_prefix_state(&bent, 1), None, "a {bent_count}-leaf claim must not be allocated from");
            assert!(backend.refutation_for_index(&bent, 1).is_err(), "nor opened at {bent_count}");
        }
        // The bound is the field's, not the blob's: restoring the honest count restores both verbs.
        binding.step_leaf_count = honest_count;
        let restored = borsh::to_vec(&(&binding, &tiles, &rows, &generated, &chunks)).expect("serializes");
        assert_eq!(
            backend.bisect_prefix_state(&restored, 1),
            backend.bisect_prefix_state(&outcome.material, 1),
            "and an honest count is untouched by the guard"
        );
    }
}

/// **The whole lane, end to end, with a real execution in it.**
///
/// Every other test in this tree exercises one link. This one runs the chain: a caller's prompt is
/// executed by the floor, the commitment is derived from what the run measured, the chain's state
/// machine opens a claim from it, the claim is walked to `Final` through the same transitions the
/// pipeline applies, and `check_palw_receipt_spend_admission_v3` — the eight items that decide
/// whether a receipt block may be mined — admits a spend of its first quantum.
///
/// The synthetic fixture in `palw_fp_admission_v3` proves the admission logic. This proves the
/// thing that fixture cannot: that a REAL free-prompt run produces a claim the admission accepts.
/// Its roots, its class, its bond and its CU all come from the execution rather than from
/// constants, so a change that made real runs inadmissible would fail here and pass there.
#[cfg(test)]
mod end_to_end_tests {
    use super::*;
    use crate::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_commitment_v3, PalwFpClassFactsV3};
    use kaspa_consensus_core::palw_freeprompt_v3::{
        fp_claim_id_v3, fp_quanta_v3, PalwBeaconFactV3, PalwFreePromptJobV3, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER,
        PALW_FP_V3_VERSION,
    };
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    use kaspa_consensus_core::palw_state_v2::{
        apply_palw_transition_v2, PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
        PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const MATURITY: u64 = 5;
    const USE_WINDOW: u64 = 50;
    const NETWORK: &[u8] = b"misaka-palw-rc";

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    #[test]
    fn a_real_prompts_execution_certifies_and_the_chain_admits_a_receipt_block_for_it() {
        // ---- the run: a caller's tokens on the registered floor -------------------------------
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        let backend = Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves"));

        let bond_outpoint = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
        // A REAL ML-DSA-87 identity, because the point of this test is the full admission — the
        // stateless signature check included. A fixture pubkey passes the stateful items and would
        // leave `validate_signature_v3` unexercised on the only end-to-end path in the tree.
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([0x42u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]);
        let pubkey = key.public_key().to_vec();
        let ctx_max = backend.profile().n_ctx as usize;
        let prompt: Vec<usize> = vec![11];
        let decode = (ctx_max - prompt.len()) as u32;
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: h(999),
            class_id: entry.class_id(),
            executor_bond: bond_outpoint,
            executor_pubkey: pubkey.clone(),
            operator_id: h(90),
            anchor_block: h(0xA0),
            anchor_daa: 100,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(
                &prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(),
            ),
            prompt_tokens: prompt.len() as u32,
            decode_token_limit: decode,
            max_context_tokens: backend.profile().n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        };
        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: backend.profile().shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        };
        let commitment = palw_fp_commitment_v3(&job, &class, &run, NETWORK, 999_999).expect("a finished run commits");
        let claim_id = fp_claim_id_v3(&commitment);
        // The floor's quantum is an eighth of its own canonical job (ADR-0074 Decision 5).
        let (canonical_ctx, _) = backend.job_for_anchor(h(0xF1)).expect("the floor implies a canonical job");
        let canonical_leaves = kaspa_consensus_core::palw_step::step_leaf_count(backend.profile(), &canonical_ctx).expect("counts");
        let quantum = kaspa_consensus_core::palw_freeprompt_v3::fp_class_quantum_leaves_v1(canonical_leaves, 8);
        let quanta = fp_quanta_v3(commitment.work_leaves, quantum, 64);
        assert!(
            quanta >= 1,
            "this job must earn at least one draw to be worth certifying, got {quanta} at {} leaves",
            commitment.work_leaves
        );

        // ---- the chain: register, commit, bind, license, finalise ------------------------------
        // The base class is the floor's REAL id — the state machine refuses a first registration
        // that is not the declared base, and the whole point here is that this is the floor.
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, entry.class_id(), 4, 1000, 100, 800, 0)
            .unwrap()
            .with_fp_quanta(8, 64)
            .unwrap();
        let at =
            |block: u64, daa: u64, blue: u64| PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: 0 };
        let registrations = vec![
            Obj::ClassRegistered {
                class_id: entry.class_id(),
                artifact_root: root,
                slash_value_per_pwu: 5,
                // The floor's own canonical job, so the transition's quantum is the 963 leaves
                // this test priced against (ADR-0074 Decision 5).
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: canonical_leaves },
                // Every ticket admits: the lottery is tested where it lives, and a target that
                // refused here would make this test about luck.
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            Obj::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint),
                pubkey: pubkey.clone(),
                operator_pubkey: vec![21; 8],
                // Sized for the work it backs: the floor's job is thousands of leaves at slash 5,
                // and admission item 8 now reaches this lane (ADR-0074 review) — a bond that
                // cannot back its own claim is refused, not admitted and hoped over.
                collateral: 100_000,
                payout_payload: h(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &at(1, 100, 1), &registrations, None).unwrap();

        // **The claim, from the run.** Every field here is the execution's, not a constant.
        let committed = Obj::FreePromptCommitted {
            claim: claim_id,
            class_id: entry.class_id(),
            bond: PalwBondKeyV2(bond_outpoint),
            executor_pubkey: pubkey.clone(),
            work_leaves: commitment.work_leaves,
            prompt_token_ids_hash: commitment.job.prompt_token_ids_hash,
            decode_tokens_executed: commitment.decode_tokens_executed,
            trace_root: commitment.trace_root,
            output_root: commitment.output_root,
            execution_root: commitment.execution_root,
            trace_chunk_count: commitment.trace_chunk_count,
            trace_retention_daa: commitment.trace_retention_daa,
        };
        let (s2, _) = apply_palw_transition_v2(&s1, &params, &at(2, 101, 2), &[committed], None).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: PalwBondKeyV2(bond_outpoint), operator_id: h(90) }];
        let (s3, _) =
            apply_palw_transition_v2(&s2, &params, &at(3, 102, 3), &[Obj::PanelBound { claim: claim_id, anchor: h(77), seats }], None)
                .unwrap();
        // The bound panel's duty names the LANE: a seat handed this claim replays the caller's
        // job (FP-R6) — feeding it the attempt lane's anchor-derived verifier would mismatch
        // every honest material and default the executor by Unavailable quorum.
        let duties = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&s3, &params, &[PalwBondKeyV2(bond_outpoint)]);
        let duty = duties.iter().find(|d| d.claim_id == claim_id).expect("the seat sees the prompt's claim");
        assert!(duty.free_prompt, "a free-prompt claim's duty says so, or the seat verifies the wrong lane");
        // **And it carries what the chain PRICED the claim at.** Roots alone cannot tell a seat
        // whether the material it was served is the work this claim was paid for: a commitment's
        // execution_root rides its payload verbatim while its pwu comes from the job shape the
        // payload DECLARES, so a producer can declare a huge job, serve a trivial material whose
        // roots are honestly that material's, and a root-only seat certifies it. The seat
        // re-prices what it actually ran against these two numbers (kaspad's `fp_price_of`), so
        // they have to be on the duty or the check is unwritable.
        assert_eq!(duty.pwu, quanta as u64 * quantum, "the duty carries the claim's pwu — whole quanta of the floor's own job");
        assert_eq!(duty.work_leaves, commitment.work_leaves, "and the leaves it was priced from");
        assert_eq!(duty.quanta, quanta, "and its quanta");
        let (s4, _) = apply_palw_transition_v2(
            &s3,
            &params,
            &at(4, 103, 4),
            &[Obj::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
            None,
        )
        .unwrap();
        let (state, _) = apply_palw_transition_v2(&s4, &params, &at(5, 124, 5), &[], None).unwrap();
        assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the prompt's claim certifies");

        // ---- the block: does the chain admit a receipt spending this work? --------------------
        let beacon = PalwBeaconFactV3 { beacon_block: h(0xBEAC), beacon_daa: 130, prev_attempt_daa: 120 };
        // **The envelope the PRODUCER builds** — `build_fp_receipt_spend_envelope`, signing with
        // the bond's real key — admitted by the FULL check: stateless shape, the ML-DSA-87
        // signature, and all eight stateful items. This is the carriage a receipt block's header
        // carries under algo 7, produced by the same function a mining node calls.
        let (pph, ts, nonce) = (h(0xB0), 1_700u64, 9u64);
        let envelope = key.build_fp_receipt_spend_envelope(h(999), pph, ts, nonce, claim_id, 0, bond_outpoint, h(0xBEAC));
        let admitted = kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v3(
            &state,
            &at(6, 131, 6),
            h(999),
            pph,
            ts,
            nonce,
            MATURITY,
            USE_WINDOW,
            &beacon,
            &envelope,
            |pk: &[u8], m: &[u8], c: &[u8], sig: &[u8]| kaspa_txscript::verify_mldsa87_with_context(pk, m, c, sig).unwrap_or(false),
        )
        .expect("the chain admits a receipt block for a certified free-prompt claim, signature and all");
        assert_ne!(admitted, Hash64::default(), "and it returns the spend id the block is identified by");

        // **The producer's envelope builder is the one the admission accepts.** `produce_receipt`
        // builds the receipt carriage this way — header position, bond key — so admitting a
        // hand-built envelope proves the check, and admitting the producer's proves the seam a
        // mining node actually uses.
        let producer_envelope = key.build_fp_receipt_spend_envelope(h(999), pph, ts, nonce, claim_id, 0, bond_outpoint, h(0xBEAC));
        let via_producer = kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v3(
            &state,
            &at(6, 131, 6),
            h(999),
            pph,
            ts,
            nonce,
            MATURITY,
            USE_WINDOW,
            &beacon,
            &producer_envelope,
            |pk: &[u8], m: &[u8], c: &[u8], sig: &[u8]| kaspa_txscript::verify_mldsa87_with_context(pk, m, c, sig).unwrap_or(false),
        )
        .expect("the producer-built envelope is admitted just as the hand-built one was");
        assert_eq!(via_producer, admitted, "and it identifies the same spend");

        // Double-spend of a quantum is `QuantumAlreadySpent`, tested where that transition lives.
    }
}
