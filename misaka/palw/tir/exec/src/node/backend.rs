//! **The generic IR backend** (RFC-0002 Phase F, F9; design §2.10): one IR class, served through
//! `PalwExecutionBackendV1` from a mapped PALWTIR1 artifact by the typed executor.
//!
//! * `job_for_anchor` — the chain's own J5 derivation for an IR class
//!   (`palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1` over the class's cached job
//!   facts): the canonical context with the anchor's id and seed, and the anchor's prompt ids over
//!   the program's `token_bound`, committed in the class's form.
//! * `execute` — the job run into its step leg and roots ([`super::run`]); the material is a
//!   [`TirCaptureV1`]: the binding, the prompt, the committed logits trace, and — within a byte cap —
//!   every leaf preimage (a DENSE capture, which anyone holding it opens at any leaf, as the legacy
//!   families' captures are); past the cap a FOLD, whose leaves are re-derived by replay.
//! * `verify_material` — a seat's check that the material answers for the claim: the claim's job,
//!   roots and output root, the binding, the trace root over the committed rows and ids, and the step
//!   root over the committed leaves (a dense capture's own; a fold's re-derived). It is not a
//!   judgement of honesty — that is the replay's (`execute_for_verdict`) and the court's.
//! * `bisect_prefix_state` — [`super::evidence::tir_bisect_prefix_state_v1`] over the capture's
//!   leaves (a dense capture's own, a fold's replayed).
//! * Readiness: the root, the leaf hashes and the drawn operands from the held inventory tree.
//! * The IR court's close proofs — `PalwCourtVerdictProofV2::TirCone`, `TirLogits`,
//!   `TirDecodeTokenTiled` / `TirDecodeToken` (F5, appended to the close proofs) — are built by
//!   inherent methods here ([`TirBackendV1::cone_close`], [`TirBackendV1::logits_close`],
//!   [`TirBackendV1::decode_token_close`]) over [`super::evidence::TirEvidenceV1`], under the rules
//!   the court grades them by ([`TirBackendV1::court_rules`]); the node's court flow asks them of an
//!   IR class (the trait's legacy verbs cannot carry an IR close). `supports_court` is `true`.

use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;
use kaspa_consensus_core::palw_attempt_rules_v1::{PalwAttemptRulesV1, palw_attempt_output_root_v1};
use kaspa_consensus_core::palw_backend::{
    PalwCaptureShapeV1, PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{
    PalwDecodeTokenPinV1, PalwTiledDecodePinV1, PalwTiledDecodeTokensV1, tiled_logits_scheme_id_v1,
};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_job_for_anchor_of_v1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirLogitsConsistencyV1, PalwTirStepLeafDisclosureV1,
    build_tir_dissect_bottom_bounded_v1, build_tir_dissect_round_bounded_v1, build_tir_named_leaf_refutation_v1,
    build_tir_root_claim_bounded_v1, build_tir_step_leaf_disclosure_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirRootClaimV1};
use kaspa_consensus_core::palw_tir_step_v1::{
    PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1, verify_tir_binding_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::annex::{PALW_TIR_LEAF_ANNEX_VERSION_V1, PalwTirAnnexTraceV1, PalwTirLeafAnnexV1, tir_annex_trace_v1};
use super::artifact::TirArtifactV1;
use super::evidence::{TirEvidenceV1, TirRetainedJobV1, TirTraceV1, tir_bisect_prefix_state_v1};
use super::run::TirClassRunnerV1;

/// The 8-byte head of an encoded [`TirCaptureV1`].
pub const TIR_CAPTURE_MAGIC_V1: [u8; 8] = *b"PALWTIRC";

/// A capture holds every preimage while their lanes stay within this many bytes (a dense
/// capture); past it, a fold. (A leaf is up to 16 KiB of lanes — a 4,096-lane logits tile — so the
/// cap is in bytes, not leaves.)
pub const TIR_DENSE_CAPTURE_BYTES_V1: usize = 64 << 20;

/// Node policy for one proof-construction move. The rule's work limit is shared
/// across all evaluations; 65,536 requested output entries bound result/header
/// materialization even when the values are supplied directly by committed leaves.
/// A refusal means no move is built and certifies no verdict, soundness or fee.
pub fn tir_court_builder_limits_v1(
    rules: &PalwTirCourtRulesV1,
) -> kaspa_consensus_core::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1 {
    kaspa_consensus_core::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1 {
        evaluation: rules.limits,
        max_target_requests: 65_536,
        // Explicit node policy, not a proof that the rule's element/term counters include
        // all reads: eight input edges per computed item plus source-only target requests.
        max_source_reads: rules.limits.max_elements.saturating_add(rules.limits.max_terms).saturating_mul(8).saturating_add(65_536),
    }
}

/// Whether a new [`TirBackendV1`] runs the fused kernels ([`set_tir_fused_kernels_default_v1`]).
static TIR_FUSED_KERNELS_DEFAULT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// **Run the fused kernels in every IR backend built from now on** (RFC-0002 §7, Phase G) — the
/// node's `--palw-tir-fused-kernels`, set once at start-up. Node software in no consensus object:
/// byte-identical to the generic kernels (tir-lower's `fused_gate`), so it moves speed and nothing
/// else. OFF by default, and kept off until the D-F drills pass with it on.
pub fn set_tir_fused_kernels_default_v1(on: bool) {
    TIR_FUSED_KERNELS_DEFAULT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// **The Metal backend of the fused unit-row kernels** — the node's `--palw-tir-metal` (RFC-0002 §7 F-6): bit-identical to the CPU
/// kernels, node software in no consensus object. Turning it on turns the fused kernels on (it is one of their implementations);
/// `Err` names why this node cannot (no `metal` build, no device).
pub fn set_tir_metal_v1(on: bool) -> Result<(), String> {
    crate::fused::metal::set_metal_backend(on)?;
    if on {
        set_tir_fused_kernels_default_v1(true);
    }
    Ok(())
}

/// What a new [`TirBackendV1`] starts with ([`set_tir_fused_kernels_default_v1`]).
pub fn tir_fused_kernels_default_v1() -> bool {
    TIR_FUSED_KERNELS_DEFAULT.load(std::sync::atomic::Ordering::Relaxed)
}

/// **One retained job, kept process-wide** — keyed by the class and the job's context hash. A fold's
/// every evidence build (an annex asked, a dissection's round) re-derives the execution it opens
/// ([`TirBackendV1::retain`], a whole run); a backend is built per request, so the memo lives here.
/// One memo entry holds its reservation with the retained hashes/trace/resume points. Requests
/// still using an evicted entry keep its reservation until the last owner drops it; concurrent
/// re-executions must each pass admission. Byte envelopes are operational, not measured RSS.
static TIR_RETAINED_MEMO: std::sync::Mutex<Option<(Hash64, Hash64, Arc<super::memory::TirRetainedJobLeaseV1>)>> =
    std::sync::Mutex::new(None);

/// **Why an IR class takes no free prompt here** (RFC-0002 Phase H): its free-prompt lane opens only with the TIR public frame
/// (`palw_tir_public_conviction_v1`), and the backend runs it only for a tiled class, a job the frame judges (greedy, seeded V3 or
/// V4 selection; no tail, constraint, prefix state or image), and a host that knows its network. The CLI and the RPC refuse with
/// these words below the fence.
pub const TIR_FREE_PROMPT_CLOSED_V1: &str =
    "free-prompt claims of an IR class open only with the TIR public frame (palw_tir_public_conviction_v1, RFC-0002 Phase H)";

/// **The drill a free-prompt run carries** (G14 D-05 / D-06): none, one committed lane of a step leaf moved (every root re-derived
/// from that capture), or the committed token at a decode position replaced by the best lane the job's rule does not choose (the
/// arithmetic after it computed honestly from it). Callers refuse to reach a drill on a network carrying value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TirFpDrillV1 {
    None,
    Leaf(u64),
    Selection(u32),
}

/// **The child a challenger disputes** (RFC-0002 F7): the first of `phase`'s pending children whose
/// claimed partials are not the ones `honest` computes — the challenger's own round over the same
/// range, against the same root ([`TirBackendV1::dissect_round`] over its own execution). `None` when
/// every child agrees (the responder's children are the truth there) or the two rounds are not of one
/// shape.
pub fn tir_dissect_choice_v1(phase: &PalwTirDissectPhaseV1, honest: &PalwTirDissectRoundV1) -> Option<u8> {
    if phase.pending().len() != honest.children.len() {
        return None;
    }
    phase.pending().iter().zip(&honest.children).position(|(claimed, truth)| claimed != truth).and_then(|i| u8::try_from(i).ok())
}

/// **What an IR producer retains and serves for one execution** — the material of its outcome.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TirCaptureV1 {
    pub binding: PalwTirStepBindingV1,
    /// The job's prompt ids (they commit to the context's `prompt_token_ids_hash`).
    pub prompt: Vec<u32>,
    /// The committed logits trace: every selecting row and the ids it selected.
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
    /// Every leaf preimage in step order (a dense capture), or none (a fold).
    pub leaves: Vec<PalwStepTileLeafV1>,
}

impl TirCaptureV1 {
    /// The 8-byte head of an encoded capture ([`TIR_CAPTURE_MAGIC_V1`]).
    pub const MAGIC: [u8; 8] = TIR_CAPTURE_MAGIC_V1;

    pub fn encode(&self) -> Vec<u8> {
        let mut out = TIR_CAPTURE_MAGIC_V1.to_vec();
        borsh::to_writer(&mut out, self).expect("a Vec writer does not fail");
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let body = bytes.strip_prefix(&TIR_CAPTURE_MAGIC_V1[..]).ok_or("not an IR capture")?;
        super::capture_codec::decode(body, bytes.len())
    }

    pub fn is_dense(&self) -> bool {
        !self.leaves.is_empty()
    }
}

/// Authenticate a public capture's prompt/trace before a partial verifier uses its
/// tokens. Matching claimed root fields alone does not authenticate the carried ids.
/// Step preimages are authenticated independently by a dense tree or requested runs.
pub fn tir_authenticate_capture_metadata_v1(
    capture: &TirCaptureV1,
    class_id: Hash64,
    prompt_form: PalwPromptIdsFormV1,
    cap: u64,
) -> Result<(), String> {
    let binding = &capture.binding;
    let verified = verify_tir_binding_v1(binding, cap).map_err(|e| e.to_string())?;
    let ctx = &binding.job_context;
    // token_bound bounds INPUT token ids. A legal program can declare a different
    // number of OUTPUT logits lanes (for example the H-window corpus is 16 -> 4).
    // Match the core court's exact post/logits tensor shape, not the input bound.
    let program = &verified.space.program;
    let logits_lanes = program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.elements_at(1);
    if binding.class.class_id(&binding.artifact_root) != class_id
        || !prompt_token_ids_match_v1(prompt_form, &capture.prompt, &ctx.prompt_token_ids_hash)
        || capture.prompt.len() as u64 != u64::from(ctx.declared_prefill_tokens)
        || capture.generated.len() as u64 != u64::from(ctx.exact_decode_tokens)
        || capture.logits_rows.len() != capture.generated.len()
        || capture.prompt.iter().any(|t| *t >= program.token_bound)
        || capture.generated.iter().any(|t| u64::from(*t) >= logits_lanes)
        || capture.generated.iter().take(capture.generated.len().saturating_sub(1)).any(|t| *t >= program.token_bound)
        || capture.logits_rows.iter().any(|row| row.len() as u64 != logits_lanes)
    {
        return Err("the public capture's class, prompt, token or trace shape is not its binding".into());
    }
    let trace = if Hash64::from_bytes(verified.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
        kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated)
    } else {
        Some(kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated))
    };
    if trace != Some(binding.full_logits_trace_root) {
        return Err("the public capture's rows and generated ids do not produce its committed trace root".into());
    }
    Ok(())
}

/// **An IR capture's answer to a trace-event demand** (RFC-0002 Phase F, F6 D): the event `(row,
/// tile)` of the committed logits trace, disclosed in the IR form from the capture's own binding,
/// rows and ids (`tir_logits_event_disclosure_v1`) — or `OutOfRange` over the binding alone when the
/// event is not in the run. No execution: the answer is the committed trace, opened. `None` when the
/// bytes are not an IR capture (a legacy family answers them).
pub fn tir_trace_event_disclosure_of_capture_v1(
    capture: &[u8],
    row: u32,
    tile: u8,
) -> Option<Result<kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1, String>> {
    use kaspa_consensus_core::palw_tir_court_v1::{PalwTirTraceEventDisclosureV1, tir_logits_event_disclosure_v1};
    if !capture.starts_with(&TIR_CAPTURE_MAGIC_V1) {
        return None;
    }
    Some(TirCaptureV1::decode(capture).map(|c| {
        tir_logits_event_disclosure_v1(&c.binding, &c.logits_rows, &c.generated, row, tile)
            .unwrap_or_else(|| PalwTirTraceEventDisclosureV1::OutOfRange { binding: Box::new(c.binding) })
    }))
}

fn count_of(binding: &PalwTirStepBindingV1) -> u64 {
    binding.step_leaf_count
}

/// **DRILL ONLY (RFC-0006, D-S3): the consistent boundary lie a producer commits** — process-wide, set once at start-up by
/// `--palw-drill-tamper-boundary` and consulted by [`TirBackendV1`]'s injected-fault runs (a node that sets none never lies).
static TIR_BOUNDARY_LIE_V1: std::sync::Mutex<Option<super::run::TirBoundaryLieV1>> = std::sync::Mutex::new(None);

pub fn set_tir_drill_boundary_lie_v1(lie: Option<super::run::TirBoundaryLieV1>) {
    *TIR_BOUNDARY_LIE_V1.lock().unwrap_or_else(|p| p.into_inner()) = lie;
}

fn tir_drill_boundary_lie_v1() -> Option<super::run::TirBoundaryLieV1> {
    *TIR_BOUNDARY_LIE_V1.lock().unwrap_or_else(|p| p.into_inner())
}

/// One IR class served from a mapped artifact.
pub struct TirBackendV1 {
    model_id: String,
    artifact: Arc<TirArtifactV1>,
    class: PalwTirClassV1,
    space: PalwTirStepSpaceV1,
    artifact_root: Hash64,
    class_id: Hash64,
    canonical: PalwJobContextV2,
    /// What the class's attempt jobs are a function of (consensus's `PalwTirJobFactsV1`, cached).
    facts: PalwTirJobFactsV1,
    /// The network's prompt form; the class's own is `facts.prompt_ids_form(network_form)`.
    network_form: PalwPromptIdsFormV1,
    /// The form this class commits its prompt ids in.
    prompt_ids_form: PalwPromptIdsFormV1,
    /// The ruleset's `max_step_leaf_count` (the ladder every opening and root is capped at).
    ladder: u64,
    attempt_rules: PalwAttemptRulesV1,
    /// The leaf hashes of the last fold re-executed, by job context — the ladder asks a fold's
    /// prefix state at every rung, and each answer would otherwise be a whole re-execution.
    fold_hashes: std::sync::Mutex<Option<(Hash64, Arc<super::memory::TirFoldHashesLeaseV1>)>>,
    /// The lane bytes up to which a capture is dense ([`TIR_DENSE_CAPTURE_BYTES_V1`] by default).
    dense_capture_bytes: usize,
    /// Run the fused kernels ([`set_tir_fused_kernels_default_v1`], [`Self::with_fused_kernels`]).
    fused: bool,
    memory_reserver: Option<super::memory::TirMemoryReserverV1>,
    /// RFC-0002 Phase H: the network id a free-prompt context is built under (`palw_fp_job_context_v3`); empty refuses the lane.
    network_id: Vec<u8>,
    /// RFC-0002 Phase H: the class facts a free-prompt context carries — the class id in `shape_profile_id`, and the class's
    /// registered semantics' runtime ids where it has them (zero otherwise).
    fp_class: kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3,
}

impl TirBackendV1 {
    fn check_tir_own_original_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt: &[u32],
    ) -> Result<(), String> {
        let ctx = &header.job_context;
        self.tir_view_v1(header)?;
        if ctx.shape_profile_id != self.class_id
            || job.class_id != self.class_id
            || header.artifact_root != self.artifact_root
            || kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(job) != ctx.job_id
            || job.tokenizer_id != ctx.tokenizer_id
            || job.prompt_token_ids_hash != ctx.prompt_token_ids_hash
            || job.prompt_tokens != ctx.declared_prefill_tokens
            || job.max_context_tokens != ctx.max_context_tokens
            || ctx.exact_decode_tokens == 0
            || ctx.exact_decode_tokens > job.decode_token_limit
            || prompt.len() as u64 != u64::from(job.prompt_tokens)
            || !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &ctx.prompt_token_ids_hash)
        {
            return Err("the own TIR source changes the exact original public job, class or prompt".into());
        }
        Ok(())
    }

    /// Shared by both own replay APIs: constraint, V4 pipeline and sampled V3
    /// selection use the original job, never the attempt runner's default greedy.
    fn tir_original_selector_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    ) -> Result<super::run::TirDecodeSelectorV1, String> {
        let mask = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_mask_for_host_v1(
            job,
            self.token_table_for_job_v1(job),
        )?;
        let sampling = job.sampling_v2();
        let pipeline = job.decode.as_ref().filter(|_| job.decodes_under_v4_rules()).cloned();
        Ok(Arc::new(move |before: &[u32], row: &[i32], index: u32| {
            if let Some(mask) = mask.as_ref() {
                return mask.select(mask.state_after(before).as_ref(), row, index, &sampling).0;
            }
            if let Some(config) = pipeline.as_ref() {
                if let Some(lane) =
                    kaspa_consensus_core::palw_decode_pipeline_v4::decode_select_v4(config, &sampling, before, row, &|_| true)
                {
                    return lane as u32;
                }
            }
            kaspa_consensus_core::palw_decode_select_v2::decode_token_select_v2(row, &sampling.seed, index, sampling.temperature_q)
                as u32
        }))
    }

    fn tir_original_selector_need_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    ) -> Result<u64, String> {
        let mut bytes = 0u64;
        if let Some(constraint) = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_tail_v1(job) {
            let table = self.token_table_for_job_v1(job).ok_or("own TIR constraint table is not publicly registered on this host")?;
            // table.root() simultaneously holds raw and indexed hashes and a
            // folding level; include capacity growth, EOG sorting and the lane
            // admission mask. The table's resident renderings are held by the
            // separately admitted backend/table role, not cloned by the selector.
            bytes = (table.entries.len() as u64).checked_add(1).and_then(|n| n.checked_mul(256))
                .and_then(|n| n.checked_add((table.eog_token_ids.len() as u64).checked_mul(16)?))
                // Borsh automaton nodes/edges plus canonical reserialization and
                // bounded stack clones are conservatively covered per raw byte.
                .and_then(|n| n.checked_add((constraint.len() as u64).checked_mul(64)?))
                .and_then(|n| n.checked_add(16_384))
                .ok_or("own TIR constraint selector quote overflow")?;
        }
        if let Some(config) = job.decode.as_ref().filter(|_| job.decodes_under_v4_rules()) {
            // The selector owns its config clone and one penalty-window count
            // vector. No full logits-row copy is made by decode_select_v4.
            let stops = config
                .stop_sequences
                .iter()
                .try_fold(0u64, |n, sequence| n.checked_add(24)?.checked_add((sequence.len() as u64).checked_mul(4)?))
                .ok_or("own TIR stop sequence quote overflow")?;
            bytes = bytes
                .checked_add(stops)
                .and_then(|n| n.checked_add((config.logit_bias.len() as u64).checked_mul(8)?))
                .and_then(|n| n.checked_add(u64::from(config.penalty_window).checked_mul(24)?))
                .ok_or("own TIR decode selector quote overflow")?;
        }
        Ok(bytes)
    }

    fn tir_public_capture_context_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    ) -> Result<(PalwTirStepBindingV1, kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicSourceV1), String> {
        let (registered, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&self.class, &self.artifact_root)
            .map_err(|e| e.to_string())?;
        let public = kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_v1(
            &registered,
            &self.class.layout,
            header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            self.ladder,
        )?;
        let binding =
            header.resolve_registered_layout_v1(&registered, &self.class.layout, header.committed_execution_root, self.ladder)?;
        Ok((binding, public))
    }
    /// Serve the class `artifact` declares under `artifact_root` (the inventory root the chain
    /// registered — derived from the artifact by the caller, never read from a sidecar here), at the
    /// canonical job `canonical` (its `shape_profile_id` must be the class id).
    pub fn new(
        model_id: String,
        artifact: Arc<TirArtifactV1>,
        artifact_root: Hash64,
        canonical: PalwJobContextV2,
        prompt_ids_form: PalwPromptIdsFormV1,
        ladder: u64,
    ) -> Result<Self, String> {
        let class = artifact.class()?;
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let class_id = class.class_id(&artifact_root);
        let facts = PalwTirJobFactsV1::of(&class, &space.program, class_id);
        let network_form = prompt_ids_form;
        let prompt_ids_form = facts.prompt_ids_form(network_form);
        if canonical.shape_profile_id != class_id {
            return Err("the canonical job names another class".into());
        }
        space.leaf_count_capped(&canonical, ladder).map_err(|e| format!("the canonical job does not count: {e}"))?;
        Ok(Self {
            model_id,
            artifact,
            class,
            space,
            artifact_root,
            class_id,
            canonical,
            facts,
            network_form,
            prompt_ids_form,
            ladder,
            attempt_rules: PalwAttemptRulesV1::CoreV1,
            fold_hashes: std::sync::Mutex::new(None),
            dense_capture_bytes: TIR_DENSE_CAPTURE_BYTES_V1,
            fused: tir_fused_kernels_default_v1(),
            memory_reserver: None,
            network_id: Vec::new(),
            fp_class: kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
                model_profile_id: Hash64::default(),
                runtime_manifest_hash: Hash64::default(),
                runtime_class_id: Hash64::default(),
                shape_profile_id: class_id,
                cu_ruleset_id: Hash64::default(),
            },
        })
    }

    /// A scoped retained/fold-memo ledger probe. Executor/evidence-store working
    /// reservations use the daemon's process-wide callback.
    pub fn with_memory_reserver_v1(mut self, reserver: super::memory::TirMemoryReserverV1) -> Self {
        self.memory_reserver = Some(reserver);
        self
    }

    /// **RFC-0002 Phase H: the network this backend's free-prompt contexts are built under** (the lineage passes the node's).
    pub fn with_network_id_v1(mut self, network_id: &[u8]) -> Self {
        self.network_id = network_id.to_vec();
        self
    }

    /// **RFC-0002 Phase H: the runtime ids a free-prompt context carries** — the class's registered semantics
    /// (`PalwClassSemanticsV1`: model profile, runtime manifest and class, CU ruleset), which the frame matches the header's
    /// context against. `shape_profile_id` stays this class's id whatever is passed.
    pub fn with_fp_class_facts_v1(mut self, facts: kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3) -> Self {
        self.fp_class = kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 { shape_profile_id: self.class_id, ..facts };
        self
    }

    /// Run the fused kernels (or not), whatever the process default says.
    pub fn with_fused_kernels(mut self, on: bool) -> Self {
        self.fused = on;
        self
    }

    pub fn fused_kernels(&self) -> bool {
        self.fused
    }

    /// Keep captures dense only while their lanes fit `bytes` (0: every capture is a fold).
    pub fn with_dense_capture_bytes(mut self, bytes: usize) -> Self {
        self.dense_capture_bytes = bytes;
        self
    }

    pub fn class(&self) -> &PalwTirClassV1 {
        &self.class
    }

    pub fn class_id(&self) -> Hash64 {
        self.class_id
    }

    pub fn artifact_root(&self) -> Hash64 {
        self.artifact_root
    }

    pub fn space(&self) -> &PalwTirStepSpaceV1 {
        &self.space
    }

    pub fn artifact(&self) -> &Arc<TirArtifactV1> {
        &self.artifact
    }

    pub fn canonical(&self) -> &PalwJobContextV2 {
        &self.canonical
    }

    pub fn runner(&self) -> TirClassRunnerV1<'_> {
        TirClassRunnerV1::new(&self.space, self.artifact.plan(), self.artifact.params(), self.class_id)
            .expect("the space and the plan are of one program")
            .with_fused(self.fused)
    }

    fn ids(prompt: &[usize]) -> Result<Vec<u32>, String> {
        prompt.iter().map(|t| u32::try_from(*t).map_err(|_| "a prompt id past u32".to_string())).collect()
    }

    /// **Run a job and retain it** (the producer's retention: leaf hashes and resume points).
    pub fn retain(&self, job: &PalwJobContextV2, prompt: &[u32]) -> Result<TirRetainedJobV1, String> {
        if !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        self.runner().retain(&self.class, self.artifact_root, job, prompt, self.ladder)
    }

    /// [`Self::retain`], through the process-wide memo ([`TIR_RETAINED_MEMO`]): evidence
    /// builds can share a held job and its reservation. Concurrent misses each require admission.
    pub fn retain_memo(&self, job: &PalwJobContextV2, prompt: &[u32]) -> Result<Arc<super::memory::TirRetainedJobLeaseV1>, String> {
        let ctx_hash = job.context_hash();
        let reserver = self.memory_reserver.clone().or_else(super::memory::memory_reserver_v1);
        let admission_id = super::memory::reserver_id_v1(&reserver);
        // A tool's previously unreserved memo must not bypass a newly armed daemon ledger.
        if let Ok(mut memo) = TIR_RETAINED_MEMO.lock() {
            if let Some((class, ctx, held)) = memo.as_ref()
                && *class == self.class_id
                && *ctx == ctx_hash
                && held.prompt == prompt
                && held.admission_id == admission_id
            {
                return Ok(held.clone());
            }
            // Release idle memo data before asking admission; in-flight Arcs keep theirs.
            *memo = None;
        }
        let need = super::memory::tir_retained_storage_need_v1(&self.space, &self.class, job, self.ladder)?;
        let lease = super::memory::reserve_v1(&reserver, "tir-retained-job", self.class_id, job, need)?;
        let retained = self.retain(job, prompt)?;
        let held = Arc::new(super::memory::TirRetainedJobLeaseV1::new(retained, reserver, lease, need));
        if let Ok(mut memo) = TIR_RETAINED_MEMO.lock() {
            *memo = Some((self.class_id, ctx_hash, held.clone()));
        }
        Ok(held)
    }

    /// Evict this backend's idle memo; in-flight requests retain both data and reservation.
    pub fn evict_retained_memo_v1(&self) {
        let reserver = self.memory_reserver.clone().or_else(super::memory::memory_reserver_v1);
        let admission = super::memory::reserver_id_v1(&reserver);
        if let Ok(mut memo) = TIR_RETAINED_MEMO.lock()
            && memo.as_ref().is_some_and(|(class, _, held)| *class == self.class_id && held.admission_id == admission)
        {
            *memo = None;
        }
        if let Ok(mut memo) = self.fold_hashes.lock() {
            *memo = None;
        }
    }

    /// Run a job into a capture — dense while its lanes fit the dense-capture bytes —
    /// optionally with one lane of leaf `fault` corrupted and the commitment re-derived over the lie
    /// (a drill; always dense).
    fn capture_run(&self, job: &PalwJobContextV2, prompt: &[u32], fault: Option<u64>) -> Result<TirCaptureV1, String> {
        self.capture_run_selecting(job, prompt, fault, None)
    }

    /// [`Self::capture_run`] with each generated id selected by `selector` (a free-prompt job's own rule) instead of the attempt
    /// lane's greedy rule.
    fn capture_run_selecting(
        &self,
        job: &PalwJobContextV2,
        prompt: &[u32],
        fault: Option<u64>,
        selector: Option<super::run::TirDecodeSelectorV1>,
    ) -> Result<TirCaptureV1, String> {
        if !prompt_token_ids_match_v1(self.prompt_ids_form, prompt, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        let (mut dense, mut bytes) = (true, 0usize);
        let mut leaves = Vec::new();
        // The drill's consistent boundary lie replaces the single-leaf fault: the lie is committed AND computed on.
        let boundary = fault.and_then(|_| tir_drill_boundary_lie_v1());
        let fault = if boundary.is_some() { None } else { fault };
        let runner = self.runner().with_boundary_lie(boundary);
        let runner = match selector {
            Some(selector) => runner.with_selector(selector),
            None => runner,
        };
        let run = runner.run(job, prompt, self.ladder, false, &mut |l| {
            if !dense {
                return;
            }
            bytes += l.preimage.values_le.len();
            if bytes > self.dense_capture_bytes && fault.is_none() && boundary.is_none() {
                (dense, leaves) = (false, Vec::new());
                return;
            }
            leaves.push(l.preimage.clone());
        })?;
        let mut binding = PalwTirStepBindingV1 {
            version: kaspa_consensus_core::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: job.clone(),
            class: self.class.clone(),
            artifact_root: self.artifact_root,
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        };
        if let Some(i) = fault {
            let leaf = leaves.get_mut(i as usize).ok_or_else(|| format!("no leaf {i} to corrupt"))?;
            let lane = (leaf.value_count as usize / 2) * 4;
            let v = i32::from_le_bytes(leaf.values_le[lane..lane + 4].try_into().expect("four bytes"));
            leaf.values_le[lane..lane + 4].copy_from_slice(&v.wrapping_add(1).to_le_bytes());
            let ctx_hash = job.context_hash();
            let hashes: Vec<Hash64> = leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect();
            binding.step_merkle_root = step_merkle_root_capped_v1(&hashes, self.ladder).map_err(|e| e.to_string())?;
            binding.committed_execution_root = palw_tir_execution_root_v1(
                &ctx_hash,
                &binding.full_logits_trace_root,
                &self.class_id,
                binding.step_leaf_count,
                &binding.step_merkle_root,
            );
        }
        Ok(TirCaptureV1 { binding, prompt: prompt.to_vec(), logits_rows: run.logits_rows, generated: run.generated, leaves })
    }

    /// **The free-prompt context of `job` at `count` decode tokens** — `palw_fp_job_context_v3` over this class's facts and the
    /// network id: the context the run is performed under and `TFP1` re-derives from the header.
    fn fp_context_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        count: u32,
    ) -> Result<PalwJobContextV2, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_job_context_v3, palw_fp_run_facts_for_executed_v1};
        if self.network_id.is_empty() {
            return Err("this IR backend holds no network id: its free-prompt contexts cannot be built".into());
        }
        palw_fp_job_context_v3(job, &self.fp_class, &palw_fp_run_facts_for_executed_v1(job, count), &self.network_id)
            .map_err(|e| format!("{e:?}"))
    }

    /// **RFC-0002 Phase H: an IR class's free-prompt run** — the integer engine under the job's own FP context, every id
    /// selected by the job's own rule as the frame's decode-token door states it (`PalwTirSelectionRuleV1`: greedy, seeded V3,
    /// V4), run to exactly where the job's decoder stops (`palw_fp_decode_run_v1`: once at the budget, again at an earlier stop).
    /// Refused by name: a flat-scheme class, a job tail (evaluation, generation, images), a constrained or prefix-state job (a
    /// staged semantic stage), an id past the program's token bound. A drill runs once at the budget.
    fn execute_free_prompt_tir_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
        drill: TirFpDrillV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_tir_court_v1::PalwTirSelectionRuleV1;
        if job.class_id != self.class_id {
            return Err("the free-prompt job names another class".into());
        }
        if !self.facts.tiled {
            return Err(format!("a flat-scheme IR class has no bounded frame door: {TIR_FREE_PROMPT_CLOSED_V1}"));
        }
        if job.prompt_tokens as usize != prompt_tokens.len() || prompt_tokens.is_empty() {
            return Err(format!("the job declares {} prompt tokens and {} were supplied", job.prompt_tokens, prompt_tokens.len()));
        }
        let prompt = Self::ids(prompt_tokens)?;
        if let Some(bad) = prompt.iter().find(|t| **t >= self.facts.token_bound) {
            return Err(format!("token {bad} is outside this class's token bound {}", self.facts.token_bound));
        }
        if job.is_prefix_state() || job.is_v5() {
            return Err("a prefix-state or image job is a staged semantic stage, outside the IR free-prompt lane".into());
        }
        let rule = Arc::new(PalwTirSelectionRuleV1::of_job_v1(job).ok_or_else(|| {
            "a constrained job or a job with a tail has no frame selection rule (a staged semantic stage)".to_string()
        })?);
        let vocab = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&self.class, &self.artifact_root)
            .map_err(|e| e.to_string())?
            .0
            .logits_vocab;
        let capture = match drill {
            TirFpDrillV1::Selection(position) => {
                let selector: super::run::TirDecodeSelectorV1 = Arc::new(move |before: &[u32], row: &[i32], index: u32| {
                    let honest = rule.select_v1(before, row, None).unwrap_or(0);
                    if index == position { rule.select_v1(before, row, Some(honest)).unwrap_or(honest) } else { honest }
                });
                let context = self.fp_context_v1(job, job.decode_token_limit)?;
                let capture = self.capture_run_selecting(&context, &prompt, None, Some(selector))?;
                capture.generated.iter().for_each(|id| on_token(*id));
                capture
            }
            TirFpDrillV1::None | TirFpDrillV1::Leaf(_) => {
                let leaf = match drill {
                    TirFpDrillV1::Leaf(leaf) => Some(leaf),
                    _ => None,
                };
                let selector: super::run::TirDecodeSelectorV1 =
                    Arc::new(move |before: &[u32], row: &[i32], _| rule.select_v1(before, row, None).unwrap_or(0));
                let (capture, _stop) = kaspa_consensus_core::palw_decode_pipeline_v4::palw_fp_decode_run_v1(
                    job,
                    vocab,
                    on_token,
                    |count, select, stream| {
                        let context = self.fp_context_v1(job, count)?;
                        let capture = self.capture_run_selecting(&context, &prompt, leaf, Some(selector.clone()))?;
                        // The job's decoder reads the committed rows in order: it commits, and stops, as the producer's pipeline does.
                        for row in &capture.logits_rows {
                            let id = select(row.as_slice());
                            stream(id);
                        }
                        Ok(capture)
                    },
                    |capture: &TirCaptureV1| capture.generated.as_slice(),
                )?;
                capture
            }
        };
        self.fp_run_of_capture_v1(job, &capture)
    }

    /// The [`kaspa_consensus_core::palw_backend::PalwFpRunV1`] a free-prompt capture IS: its TIR roots, the free-prompt lane's
    /// retained-trace manifest over the selecting rows' tiled row roots, and the facts whose IR root
    /// (`palw_fp_tir_execution_root_v1`: trace, class, leaf count, step root) the commitment assembly recomputes.
    fn fp_run_of_capture_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        capture: &TirCaptureV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{
            PalwFpRunFactsV3, palw_fp_run_facts_for_executed_v1, palw_fp_tir_execution_root_v1,
        };
        let b = &capture.binding;
        let ctx_hash = b.job_context.context_hash();
        let events: Vec<Hash64> = capture
            .logits_rows
            .iter()
            .enumerate()
            .map(|(r, row)| kaspa_consensus_core::palw_step_refute::tiled_logits_row_root_v1(&ctx_hash, r as u32, row))
            .collect::<Option<_>>()
            .ok_or("the run's rows build no retained-trace manifest")?;
        let (manifest, chunks, _) = kaspa_consensus_core::palw_freeprompt_v3::fp_trace_manifest_v3(b.job_context.job_id, &events);
        let mut outcome = self.outcome(capture);
        outcome.trace_manifest_root = manifest;
        outcome.trace_chunk_count = chunks;
        let facts = PalwFpRunFactsV3 {
            full_logits_trace_root: b.full_logits_trace_root,
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: b.step_merkle_root,
            step_leaf_count: b.step_leaf_count,
            ..palw_fp_run_facts_for_executed_v1(job, b.job_context.exact_decode_tokens)
        };
        if palw_fp_tir_execution_root_v1(&b.job_context, &facts) != outcome.execution_root {
            return Err("the IR free-prompt capture does not reproduce its own execution root".into());
        }
        Ok(kaspa_consensus_core::palw_backend::PalwFpRunV1 { outcome, facts, output_token_ids: capture.generated.clone() })
    }

    fn outcome(&self, capture: &TirCaptureV1) -> PalwExecutionOutcomeV1 {
        let b = &capture.binding;
        PalwExecutionOutcomeV1 {
            trace_root: b.full_logits_trace_root,
            output_root: palw_attempt_output_root_v1(&b.job_context, &capture.generated),
            execution_root: b.committed_execution_root,
            trace_manifest_root: kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(b.full_logits_trace_root, 1),
            trace_chunk_count: 1,
            material: capture.encode(),
        }
    }

    /// **Reassemble a capture from answered runs** (RFC-0006, D-S4): a seat that demanded a job's leaves on chain (`TirStepRun` units
    /// covering `[0, step_leaf_count)`) and was answered holds every preimage; this is the capture they make. `runs` are `(first leaf,
    /// preimages)` answers, in any order, which must tile the whole leaf range with no gap or overlap; the binding (as the answers
    /// carry it, program stripped) is verified and filled; **the step root is recomputed over the leaves and must be the binding's**
    /// (so one false leaf in any answer is refused here, whatever the openings said); the logits rows are read back from the last
    /// occurrence's logits tiles and the generated ids are their greedy selections. Nothing is believed that the root does not bind —
    /// the committed trace root is the cell verifier's own check at the last shard.
    pub fn capture_from_runs_v1(
        &self,
        mut binding: PalwTirStepBindingV1,
        prompt: Vec<u32>,
        runs: &[(u64, Vec<PalwStepTileLeafV1>)],
    ) -> Result<TirCaptureV1, String> {
        use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
        binding.class.program = self.class.program.clone();
        if binding.class.class_id(&binding.artifact_root) != self.class_id || binding.artifact_root != self.artifact_root {
            return Err("the answers' binding is of another class".into());
        }
        verify_tir_binding_v1(&binding, self.ladder).map_err(|e| format!("the answers' binding does not verify: {e}"))?;
        let mut order: Vec<&(u64, Vec<PalwStepTileLeafV1>)> = runs.iter().collect();
        order.sort_by_key(|r| r.0);
        let mut leaves: Vec<PalwStepTileLeafV1> = Vec::with_capacity(binding.step_leaf_count as usize);
        for (first, run) in order {
            if *first != leaves.len() as u64 {
                return Err(format!("the answered runs leave a gap or overlap at leaf {}", leaves.len()));
            }
            leaves.extend(run.iter().cloned());
        }
        if leaves.len() as u64 != binding.step_leaf_count {
            return Err(format!("{} leaves answered of {}", leaves.len(), binding.step_leaf_count));
        }
        let ctx = binding.job_context.clone();
        let ctx_hash = ctx.context_hash();
        let hashes: Vec<Hash64> = leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect();
        if step_merkle_root_capped_v1(&hashes, self.ladder).ok() != Some(binding.step_merkle_root) {
            return Err("the answered leaves do not reach the claim's step root".into());
        }
        // The logits rows: at every position that selects a token, the post block's logits node's tiles, in leaf order.
        let job = self.space.job_shape(&ctx).map_err(|e| e.to_string())?;
        let post = (self.space.occurrences().len() - 1) as u32;
        let mut rows: Vec<Vec<i32>> = Vec::new();
        let mut generated: Vec<u32> = Vec::new();
        for a in 0..job.positions {
            if !job.runs_post(a) {
                continue;
            }
            let mut row: Vec<i32> = Vec::new();
            for leaf in self.space.leaves_of_position(&ctx, a) {
                if matches!(leaf.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == self.space.program.logits)
                {
                    let pre = leaves.get(leaf.index as usize).ok_or("a logits leaf past the answered range")?;
                    row.extend(pre.values_le.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])));
                }
            }
            if row.is_empty() {
                return Err(format!("no logits at position {a}"));
            }
            generated.push(kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&row) as u32);
            rows.push(row);
        }
        Ok(TirCaptureV1 { binding, prompt, logits_rows: rows, generated, leaves })
    }

    /// A capture of THIS class, decoded.
    pub fn decode_capture(&self, material: &[u8]) -> Result<TirCaptureV1, String> {
        let capture = TirCaptureV1::decode(material)?;
        if capture.binding.class.class_id(&capture.binding.artifact_root) != self.class_id {
            return Err("a capture of another class".into());
        }
        Ok(capture)
    }

    /// The leaf hashes a capture answers with: a dense capture's own; a fold's, replayed (which
    /// is its producer's execution when that execution was honest).
    fn capture_leaf_hashes(&self, capture: &TirCaptureV1) -> Result<Arc<super::memory::TirFoldHashesLeaseV1>, String> {
        let ctx = &capture.binding.job_context;
        let ctx_hash = ctx.context_hash();
        let count = self.space.leaf_count_capped(ctx, self.ladder).map_err(|e| e.to_string())?;
        if count != capture.binding.step_leaf_count || (capture.is_dense() && capture.leaves.len() as u64 != count) {
            return Err("the capture hash count is not its capped job".into());
        }
        let reserver = self.memory_reserver.clone().or_else(super::memory::memory_reserver_v1);
        let admission_id = super::memory::reserver_id_v1(&reserver);
        let need = super::memory::tir_fold_hash_storage_need_v1(count)?;
        if capture.is_dense() {
            let lease = super::memory::reserve_v1(&reserver, "tir-fold-hashes", self.class_id, ctx, need)?;
            let hashes = capture.leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &self.class_id, p)).collect();
            return Ok(Arc::new(super::memory::TirFoldHashesLeaseV1 { hashes, admission_id, _lease: lease, _reserver: reserver }));
        }
        // The context commits to the prompt, so a re-execution is a function of the context once
        // the prompt is checked against it — which is what makes the context a sound memo key.
        if !prompt_token_ids_match_v1(self.prompt_ids_form, &capture.prompt, &ctx.prompt_token_ids_hash) {
            return Err("the capture's prompt does not commit to its job".into());
        }
        let mut memo = self.fold_hashes.lock().map_err(|_| "the fold memo is poisoned")?;
        if let Some((key, hashes)) = memo.as_ref()
            && *key == ctx_hash
            && hashes.admission_id == admission_id
        {
            return Ok(hashes.clone());
        }
        // Drop only idle old data before admission. Arcs held by other requests
        // continue to own the old reservation; failed admission publishes nothing.
        *memo = None;
        let lease = super::memory::reserve_v1(&reserver, "tir-fold-hashes", self.class_id, ctx, need)?;
        let run = self.runner().run(ctx, &capture.prompt, self.ladder, false, &mut |_| {})?;
        let hashes = Arc::new(super::memory::TirFoldHashesLeaseV1 {
            hashes: run.leaf_hashes,
            admission_id,
            _lease: lease,
            _reserver: reserver,
        });
        *memo = Some((ctx_hash, hashes.clone()));
        Ok(hashes)
    }

    /// **The canonical cone refutation of leaf `index` of a capture** — the same object whichever
    /// party asks: from a dense capture's own preimages, or — for a fold — from this node's
    /// re-execution, which must be the capture's execution (a fold of a forged execution is opened
    /// with [`Self::challenger_refutation`] instead).
    pub fn cone_refutation(
        &self,
        material: &[u8],
        index: u64,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(material, rules.prompt_form, |store, _| store.cone_refutation(index, rules).map_err(|e| e.to_string()))
    }

    /// **The evidence store over a capture, and the capture's binding** — a dense capture's own
    /// preimages, or — for a fold — this node's re-execution, which must be the capture's execution
    /// (a fold of an execution this node does not reproduce opens nothing here). What every builder
    /// that speaks for a capture's commitments reads: the cone close, and F7's root claim, rounds and
    /// bottom.
    fn with_capture_store<R>(
        &self,
        material: &[u8],
        prompt_form: PalwPromptIdsFormV1,
        f: impl FnOnce(&TirEvidenceV1<'_>, &PalwTirStepBindingV1) -> Result<R, String>,
    ) -> Result<R, String> {
        let capture = self.decode_capture(material)?;
        let runner = self.runner();
        if capture.is_dense() {
            let store = TirEvidenceV1::dense(
                &runner,
                &capture.binding,
                &capture.prompt,
                &capture.logits_rows,
                &capture.generated,
                &capture.leaves,
                self.artifact.as_ref(),
                prompt_form,
                self.ladder,
            )?;
            return f(&store, &capture.binding);
        }
        let own = self.retain_memo(&capture.binding.job_context, &capture.prompt)?;
        if own.binding != capture.binding {
            return Err("a fold of an execution this node does not reproduce: open it as a challenger".into());
        }
        let store = TirEvidenceV1::own(&runner, &own, self.artifact.as_ref(), prompt_form, self.ladder)?;
        f(&store, &own.binding)
    }

    /// **The served annex of leaf `leaf` of this node's capture** (RFC-0002's evidence transport, option
    /// B; [`super::annex`]): the claim's binding with its program stripped, the leaf's preimage and
    /// opening, and the trace summary — built from a dense capture's own preimages, or from this node's
    /// re-derivation of its fold (the executor answering for its own claim).
    pub fn leaf_annex(&self, material: &[u8], leaf: u64) -> Result<PalwTirLeafAnnexV1, String> {
        let capture = self.decode_capture(material)?;
        use kaspa_consensus_core::palw_tir_court_v1::PalwTirEvidenceStoreV1;
        let (opening, preimage, mut binding) = self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            let opening = store.step_opening(leaf).ok_or_else(|| format!("leaf {leaf} does not open"))?;
            let preimage = store.step_leaf(leaf).ok_or_else(|| format!("leaf {leaf} is not held"))?;
            Ok((opening, preimage, binding.clone()))
        })?;
        let trace = tir_annex_trace_v1(&self.space, &binding.job_context, &capture.logits_rows, &capture.generated, leaf)?;
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
        Ok(PalwTirLeafAnnexV1 { version: PALW_TIR_LEAF_ANNEX_VERSION_V1, binding, opening, preimage, trace })
    }

    /// **The answer to a `TirStepLeaf { index }` data-availability unit of this node's claim** (RFC-0002
    /// evidence transport C, the second IR fence): consensus's ONE builder
    /// (`build_tir_step_leaf_disclosure_v1`) over this capture's store — a dense capture's own
    /// preimages, or this node's re-derivation of its fold (the executor answering for its own claim) —
    /// with the row pin exactly where the fold asks for one, self-checked by the fold's own check, the
    /// program stripped.
    pub fn step_leaf_disclosure(&self, material: &[u8], index: u64) -> Result<PalwTirStepLeafDisclosureV1, String> {
        self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            build_tir_step_leaf_disclosure_v1(binding, index, store, self.ladder).map_err(|e| format!("step leaf {index}: {e}"))
        })
    }

    /// **The answer to an IR step unit of this node's claim** (the second IR fence's DA units,
    /// evidence transport C): a step leaf by its disclosure ([`Self::step_leaf_disclosure`]'s
    /// builder), an interior step node by its frontier and opening
    /// (`build_tir_step_node_disclosure_v1` over the store's tree), a node of the tiled trace's rows
    /// tree by its frontier (a row's: its tile leaves) and opening with the ids
    /// (`build_tir_row_node_disclosure_v1` over the capture's rows), and a unit past this execution —
    /// a leaf at or past its leaf count, a node past its tree, a row at or past the decode count, a
    /// rows node past the rows tree or of a flat trace — by the claim's binding proving so
    /// (`TirStepOutOfRange`, the program stripped). Every answer is self-checked by the fold's own
    /// check before it is returned.
    pub fn step_unit_answer(
        &self,
        material: &[u8],
        unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1,
    ) -> Result<kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1, String> {
        use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
        use kaspa_consensus_core::palw_tir_court_v1::{
            build_tir_row_node_disclosure_v1, build_tir_step_node_disclosure_v1, build_tir_step_run_disclosure_v1,
            palw_tir_step_tree_width_v1,
        };
        let tiled = Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1();
        self.with_capture_store(material, self.prompt_ids_form, |store, binding| {
            let count = binding.step_leaf_count;
            let out_of_range = || {
                let mut proof = binding.clone();
                kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut proof);
                Ok(PalwDaAnswerV1::TirStepOutOfRange(Box::new(proof)))
            };
            match unit {
                PalwDaUnitV1::TirStepLeaf { index } if index >= count => out_of_range(),
                PalwDaUnitV1::TirStepLeaf { index } => build_tir_step_leaf_disclosure_v1(binding, index, store, self.ladder)
                    .map(|d| PalwDaAnswerV1::TirStepLeaf(Box::new(d)))
                    .map_err(|e| format!("step leaf {index}: {e}")),
                PalwDaUnitV1::TirStepNode { level: 0, index } => Err(format!("step node (0, {index}): a leaf is a TirStepLeaf")),
                PalwDaUnitV1::TirStepNode { level, index } if palw_tir_step_tree_width_v1(count, level).is_none_or(|w| index >= w) => {
                    out_of_range()
                }
                PalwDaUnitV1::TirStepNode { level, index } => {
                    build_tir_step_node_disclosure_v1(binding, level, index, store, self.ladder)
                        .map(|d| PalwDaAnswerV1::TirStepNode(Box::new(d)))
                        .map_err(|e| format!("step node ({level}, {index}): {e}"))
                }
                // The rows tree is the tiled scheme's (a flat trace hashes every row at once): a row
                // at or past the decode count, a node past the tree, or any node of a flat trace is
                // proven out of range — the fold's own predicate (`check_tir_step_out_of_range_v1`).
                PalwDaUnitV1::TirRowNode { level, index }
                    if !tiled
                        || palw_tir_step_tree_width_v1(u64::from(binding.job_context.exact_decode_tokens), level)
                            .is_none_or(|w| index >= w) =>
                {
                    out_of_range()
                }
                PalwDaUnitV1::TirRowNode { level, index } => {
                    build_tir_row_node_disclosure_v1(binding, level, index, store, self.ladder)
                        .map(|d| PalwDaAnswerV1::TirRowNode(Box::new(d)))
                        .map_err(|e| format!("rows-tree node ({level}, {index}): {e}"))
                }
                // RFC-0006's run unit: a contiguous run of leaves a cell reads, with one range opening. A run that is not
                // wholly inside the execution is proven out of range by the binding, as a leaf past it is.
                PalwDaUnitV1::TirStepRun { first, count }
                    if count == 0 || first.saturating_add(u64::from(count)) > count_of(binding) =>
                {
                    out_of_range()
                }
                PalwDaUnitV1::TirStepRun { first, count } => {
                    build_tir_step_run_disclosure_v1(binding, first, count, store, self.ladder)
                        .map(|d| PalwDaAnswerV1::TirStepRun(Box::new(d)))
                        .map_err(|e| format!("step run [{first}, +{count}): {e}"))
                }
                other => Err(format!("{other:?} is not an IR step unit")),
            }
        })
    }

    /// **A challenger's store over a served annex** — its own execution `own` of the same job for every
    /// leaf before the annex's, the annex's leaf and trace summary for the accused's. `binding` is the
    /// annex's, verified and filled ([`palw_tir_leaf_annex_verify_v1`]).
    fn with_annex_store<R>(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
        f: impl FnOnce(&TirEvidenceV1<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        let runner = self.runner();
        let pins: Vec<PalwTiledDecodePinV1> = match &annex.trace {
            PalwTirAnnexTraceV1::Tiled { pin: Some(pin), .. } => vec![pin.clone()],
            _ => Vec::new(),
        };
        let trace = match &annex.trace {
            PalwTirAnnexTraceV1::Flat { logits_rows, generated_token_ids } => {
                TirTraceV1::Rows { rows: logits_rows, generated: generated_token_ids }
            }
            PalwTirAnnexTraceV1::Tiled { rows_root, generated_token_ids, .. } => {
                TirTraceV1::Summary { rows_root: *rows_root, generated: generated_token_ids, pins: &pins }
            }
        };
        let store = TirEvidenceV1::challenger_with_trace(
            &runner,
            binding,
            own,
            &annex.opening,
            annex.preimage.clone(),
            trace,
            self.artifact.as_ref(),
            self.prompt_ids_form,
            self.ladder,
        )?;
        f(&store)
    }

    /// **The cone close of the annex's leaf** from a challenger's own execution and the annex (the
    /// first leaf the two differ at): a `TirCone` the court convicts on when that leaf is false.
    pub fn annex_cone_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let refutation =
            self.with_annex_store(binding, annex, own, |store| store.cone_refutation(annex.leaf(), rules).map_err(|e| e.to_string()))?;
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) })
    }

    /// **The named-leaf proof of the annex's leaf** (a dissected leaf's one-move challenge, F7) — the
    /// accused's binding, the leaf's opening and preimage, nothing else.
    pub fn annex_named_leaf(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let refutation = self.with_annex_store(binding, annex, own, |store| {
            build_tir_named_leaf_refutation_v1(binding, annex.leaf(), store).map_err(|e| e.to_string())
        })?;
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) })
    }

    /// **The logits door at the annex's leaf** (a leaf of the logits node): its step tile against the
    /// same row's lanes in the committed trace, from the annex's pin.
    pub fn annex_logits_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        own: &TirRetainedJobV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let accusation = self.with_annex_store(binding, annex, own, |store| store.logits_consistency(annex.leaf()))?;
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(accusation) })
    }

    /// **The tiled decode-token door from an annex's pin**: the committed token of row `row` against
    /// `beat_lane` (the challenger's own token), which the pin's beat tile must hold.
    pub fn annex_decode_token_close(
        &self,
        binding: &PalwTirStepBindingV1,
        annex: &PalwTirLeafAnnexV1,
        row: u32,
        beat_lane: u32,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        let PalwTirAnnexTraceV1::Tiled { pin: Some(pin), .. } = &annex.trace else {
            return Err("the annex carries no pin (not a leaf of the logits node, or the flat scheme)".into());
        };
        let pins = [pin.clone()];
        let trace = TirTraceV1::Summary { rows_root: Hash64::default(), generated: &[], pins: &pins };
        let pin = trace
            .pin_v1(&binding.job_context, row, beat_lane)
            .ok_or("the annex's pin is not of that row, or its tile holds no such lane")?;
        let mut binding = binding.clone();
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
        Ok(PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding: Box::new(binding), pin })
    }

    /// **F7's bottom close for a challenger that holds no accused capture** (RFC-0002's evidence
    /// transport, option D): the accused's disputed leaf as its ON-CHAIN root claim carries it (the
    /// finalize's opening and preimage, and its decode pin when the finalize read one), every other leaf
    /// from the challenger's own execution of the same job (`own_material`, its capture). `root_binding`
    /// is the root claim's finalize binding with the class's program put back.
    pub fn dissect_bottom_from_root_claim(
        &self,
        own_material: &[u8],
        root: &PalwTirRootClaimV1,
        root_binding: &PalwTirStepBindingV1,
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        let own_capture = self.decode_capture(own_material)?;
        let own = self.retain_memo(&own_capture.binding.job_context, &own_capture.prompt)?;
        let runner = self.runner();
        let finalize = &root.finalize;
        let trace = match &finalize.decode_tokens {
            Some(PalwDecodeTokenPinV1::TiledV1(pin)) => {
                TirTraceV1::Summary { rows_root: pin.rows_root, generated: &pin.generated_token_ids, pins: &[] }
            }
            Some(PalwDecodeTokenPinV1::Base0V1(pin)) => {
                TirTraceV1::Rows { rows: &pin.logits_rows, generated: &pin.generated_token_ids }
            }
            _ => TirTraceV1::Absent,
        };
        let store = TirEvidenceV1::challenger_with_trace(
            &runner,
            root_binding,
            &own,
            &finalize.output_opening,
            finalize.output_preimage.clone(),
            trace,
            self.artifact.as_ref(),
            rules.prompt_form,
            self.ladder,
        )?;
        build_tir_dissect_bottom_bounded_v1(root_binding, phase, &store, rules, tir_court_builder_limits_v1(rules))
            .map(|(object, _)| object)
            .map_err(|e| e.to_string())
    }

    /// **RFC-0002 F7: the responder's IR root claim** at the narrowed dissected leaf `narrowed` of its
    /// own capture (`build_tir_root_claim_v1`): every reduction over `H`'s honest totals, the elements
    /// the finalize reads, and the finalize's carriage. The program rides in its binding; a filer
    /// strips it.
    pub fn root_claim(&self, material: &[u8], narrowed: u64, rules: &PalwTirCourtRulesV1) -> Result<PalwTirRootClaimV1, String> {
        self.with_capture_store(material, rules.prompt_form, |store, binding| {
            build_tir_root_claim_bounded_v1(binding, narrowed, store, rules, tir_court_builder_limits_v1(rules))
                .map(|(object, _)| object)
                .map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: one round's children of `phase`'s disputed range**, from a capture's commitments
    /// (`build_tir_dissect_round_v1` at the class's history tile): what the responder files from its
    /// own capture, and what a challenger computes from ITS own execution to find the child it
    /// disputes ([`tir_dissect_choice_v1`]).
    pub fn dissect_round(
        &self,
        material: &[u8],
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirDissectRoundV1, String> {
        let tile = self.space.layout.h_tile;
        self.with_capture_store(material, rules.prompt_form, |store, binding| {
            build_tir_dissect_round_bounded_v1(binding, phase, tile, store, rules, tir_court_builder_limits_v1(rules))
                .map(|(object, _)| object)
                .map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: the bottom close's carriage** over `phase`'s terminal tile, from the ACCUSED's
    /// capture (`build_tir_dissect_bottom_v1`) — the same object whichever party builds it; the court
    /// decides which way it reads (`PalwCourtVerdictProofV2::TirDissection`).
    pub fn dissect_bottom(
        &self,
        accused: &[u8],
        phase: &PalwTirDissectPhaseV1,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(accused, rules.prompt_form, |store, binding| {
            build_tir_dissect_bottom_bounded_v1(binding, phase, store, rules, tir_court_builder_limits_v1(rules))
                .map(|(object, _)| object)
                .map_err(|e| e.to_string())
        })
    }

    /// **RFC-0002 F7: a one-move accusation's proof at a DISSECTED leaf** — the accused's leaf `index`
    /// named and nothing else (`build_tir_named_leaf_refutation_v1`): under the held regime the chain
    /// opens the dissection there instead of adjudicating.
    pub fn named_leaf_refutation(
        &self,
        accused: &[u8],
        index: u64,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        self.with_capture_store(accused, rules.prompt_form, |store, binding| {
            build_tir_named_leaf_refutation_v1(binding, index, store).map_err(|e| e.to_string())
        })
    }

    /// **A challenger's refutation of the accused's leaf `disputed_opening.leaf_index`**: this
    /// node's own honest execution of the accused's job before the disputed leaf, the accused's
    /// disputed leaf and path, and the accused's committed trace.
    #[allow(clippy::too_many_arguments)]
    pub fn challenger_refutation(
        &self,
        accused: &PalwTirStepBindingV1,
        prompt: &[u32],
        disputed_opening: &PalwStepOpeningV1,
        disputed_preimage: PalwStepTileLeafV1,
        accused_rows: &[Vec<i32>],
        accused_generated: &[u32],
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwTirConeRefutationV1, String> {
        let own = self.retain_memo(&accused.job_context, prompt)?;
        let runner = self.runner();
        let store = TirEvidenceV1::challenger(
            &runner,
            accused,
            &own,
            disputed_opening,
            disputed_preimage,
            accused_rows,
            accused_generated,
            self.artifact.as_ref(),
            rules.prompt_form,
            self.ladder,
        )?;
        store.cone_refutation(disputed_opening.leaf_index, rules).map_err(|e| e.to_string())
    }

    /// The logits-consistency accusation over logits leaf `index` of a capture.
    pub fn logits_consistency(&self, material: &[u8], index: u64) -> Result<PalwTirLogitsConsistencyV1, String> {
        let capture = self.decode_capture(material)?;
        let runner = self.runner();
        if !capture.is_dense() {
            return Err("a fold carries no step tile to accuse".into());
        }
        let store = TirEvidenceV1::dense(
            &runner,
            &capture.binding,
            &capture.prompt,
            &capture.logits_rows,
            &capture.generated,
            &capture.leaves,
            self.artifact.as_ref(),
            self.prompt_ids_form,
            self.ladder,
        )?;
        store.logits_consistency(index)
    }

    /// The tiled decode pin of a capture's decode row `row` against lane `beat_lane`.
    pub fn decode_token_pin(&self, material: &[u8], row: u32, beat_lane: u32) -> Result<PalwTiledDecodePinV1, String> {
        let capture = self.decode_capture(material)?;
        kaspa_consensus_core::palw_step_refute::tiled_decode_pin_v1(
            &capture.binding.job_context,
            &capture.logits_rows,
            &capture.generated,
            row,
            beat_lane,
        )
        .ok_or_else(|| "the capture's trace has no such row or lane".into())
    }

    /// **The first step leaf at which an accused capture parts from this node's own execution of
    /// the same job** — found as the ladder finds it (`tir_first_divergence_v1` over both captures'
    /// leaves: a dense capture's own, a fold's re-derived). `None` when every leaf agrees (a lie in
    /// the trace alone is the logits or the decode-token door's). What a one-move accusation names.
    pub fn first_divergent_leaf(&self, accused: &[u8], own: &[u8]) -> Result<Option<u64>, String> {
        let (a, o) = (self.decode_capture(accused)?, self.decode_capture(own)?);
        if a.binding.job_context != o.binding.job_context {
            return Err("the two captures are of different jobs".into());
        }
        let (ha, ho) = (self.capture_leaf_hashes(&a)?, self.capture_leaf_hashes(&o)?);
        Ok(super::evidence::tir_first_divergence_v1(&a.binding.job_context, &ha, &ho))
    }

    /// **The rules the court grades this class's closes under** (`adjudicate_close_proof_v2`'s IR
    /// arm): this backend's ladder and prompt form, and the court's IR work limits
    /// (`palw_tir_court_limits_v1`).
    pub fn court_rules(&self, court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.ladder,
            prompt_form: self.prompt_ids_form,
            limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(court),
        }
    }

    /// **The IR cone close of leaf `index`** — `PalwCourtVerdictProofV2::TirCone`, what a
    /// `CourtClosed` carries (the same object whichever party files it).
    pub fn cone_close(&self, material: &[u8], index: u64, rules: &PalwTirCourtRulesV1) -> Result<PalwCourtVerdictProofV2, String> {
        Ok(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(self.cone_refutation(material, index, rules)?) })
    }

    /// **The IR logits close** over logits leaf `index` (`PalwCourtVerdictProofV2::TirLogits`).
    pub fn logits_close(&self, material: &[u8], index: u64) -> Result<PalwCourtVerdictProofV2, String> {
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(self.logits_consistency(material, index)?) })
    }

    /// **The IR decode-token close** of decode row `row` in the class's scheme: the tiled door with
    /// `beat_lane` as the lane said to beat the committed token, or the flat door over every row.
    pub fn decode_token_close(&self, material: &[u8], row: u32, beat_lane: u32) -> Result<PalwCourtVerdictProofV2, String> {
        let capture = self.decode_capture(material)?;
        let binding = Box::new(capture.binding.clone());
        if Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            Ok(PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding, pin: self.decode_token_pin(material, row, beat_lane)? })
        } else {
            let pin = kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: capture.logits_rows,
                generated_token_ids: capture.generated,
            };
            Ok(PalwCourtVerdictProofV2::TirDecodeToken { binding, pin, position: row })
        }
    }

    /// **The step leaf of the logits node at decode row `row` whose tile holds lane `lane`** — the
    /// leaf a decode-token door's pin rides with (`lane` the seat's own token), or the one a logits
    /// door accuses (`lane` the first lane a disclosed trace tile parts at). `None` for a row before
    /// the first selecting position, or a lane no logits tile of that row holds.
    pub fn logits_leaf_holding(&self, ctx: &PalwJobContextV2, row: u32, lane: u64) -> Option<u64> {
        let position = (ctx.declared_prefill_tokens + row).checked_sub(1)?;
        let post = u32::try_from(self.space.occurrences().len().checked_sub(1)?).ok()?;
        let logits = self.space.program.logits;
        self.space
            .leaves_of_position(ctx, position)
            .into_iter()
            .find(|l| {
                matches!(l.kind, kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence == post && node == logits && (first_element..first_element + u64::from(l.value_count)).contains(&lane))
            })
            .map(|l| l.index)
    }

    /// **The logits door over a trace the steps did not compute, from a disclosed trace event** (the
    /// end of the second IR fence's rows-tree descent): a seat whose own steps are the claim's — its
    /// step root, beside the claim's trace root, gives the claim's execution root — descends the
    /// claim's rows tree (`TirRowNode`) to tile `tile` of row `row`, and the claim's executor discloses
    /// that tile (`TirEvent`, tiled). The close is the seat's OWN step tile holding the first lane at
    /// which the disclosed tile parts from the seat's own row — the claim's own leaf, since the step
    /// trees are one — opened in its own tree, beside the disclosed tile: `TirLogits`, which the court
    /// convicts on (`TirLogitsTraceMismatch`). `accused` is the claim's binding, its program filled.
    pub fn trace_logits_close(
        &self,
        own: &TirRetainedJobV1,
        accused: &PalwTirStepBindingV1,
        row: u32,
        tile: u8,
        event: &kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
        use kaspa_consensus_core::palw_tir_court_v1::{PalwTirEvidenceStoreV1, PalwTirTraceEventDisclosureV1, PalwTirTraceLanesV1};
        let PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = event
        else {
            return Err("the rows tree ends at a tiled trace event".into());
        };
        let theirs = (&accused.job_context, accused.step_leaf_count, accused.step_merkle_root);
        if (&own.binding.job_context, own.binding.step_leaf_count, own.binding.step_merkle_root) != theirs {
            return Err("the seat's own steps are not the claim's: the step tree's descent is the path".into());
        }
        let first = usize::from(tile) * PALW_LOGITS_TILE_LANES;
        let own_row = own.logits_rows.get(row as usize).ok_or("the row is past the seat's own run")?;
        let own_lanes = own_row.get(first..(first + PALW_LOGITS_TILE_LANES).min(own_row.len())).ok_or("the tile is past the row")?;
        if own_lanes.len() != tile_lanes.len() {
            return Err("the disclosed tile is not the row's tile width".into());
        }
        let at = own_lanes.iter().zip(tile_lanes).position(|(a, b)| a != b).ok_or("the disclosed tile is the seat's own")?;
        let lane = (first + at) as u64;
        let leaf = self
            .logits_leaf_holding(&accused.job_context, row, lane)
            .ok_or_else(|| format!("no logits step tile of row {row} holds lane {lane}"))?;
        let runner = self.runner();
        let store = TirEvidenceV1::own(&runner, own, self.artifact.as_ref(), self.prompt_ids_form, self.ladder)?;
        let accusation = PalwTirLogitsConsistencyV1 {
            binding: accused.clone(),
            step_opening: store.step_opening(leaf).ok_or_else(|| format!("step leaf {leaf} does not open"))?,
            step_preimage: store.step_leaf(leaf).ok_or_else(|| format!("step leaf {leaf} is not re-derived"))?,
            trace: PalwTirTraceLanesV1::Tiled {
                generated_token_ids: generated_token_ids.clone(),
                row_root: *row_root,
                row_opening: row_opening.clone(),
                tile_lanes: tile_lanes.clone(),
                tile_opening: tile_opening.clone(),
            },
        };
        Ok(PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(accusation) })
    }

    /// The operands of the drawn leaves, in the draw's order. `base` is where the drawn tree starts in the
    /// artifact's inventory: 0 for a single artifact, the split for a composite's adapter section — the draw
    /// is over the section's own leaves (RFC-0004 §6.7), each returned under the index it was drawn at.
    fn drawn_operands(
        &self,
        draw: &[u32],
        base: u32,
    ) -> Result<Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>, String> {
        use super::inventory::TirParamOpenerV1;
        draw.iter()
            .map(|i| {
                let leaf = base.checked_add(*i).ok_or_else(|| format!("inventory leaf {i} is past the inventory"))?;
                self.artifact
                    .param_opening(leaf)
                    .map(|o| (*i, o.operand))
                    .ok_or_else(|| format!("inventory leaf {leaf} does not open"))
            })
            .collect()
    }

    /// The step opening of leaf `index` of a capture (the disclosure the bisection's last rung asks).
    pub fn step_opening(&self, material: &[u8], index: u64) -> Result<(PalwStepOpeningV1, PalwStepTileLeafV1), String> {
        let capture = self.decode_capture(material)?;
        if !capture.is_dense() {
            return Err("a fold discloses by replay: open its leaves with a node's own retention".into());
        }
        let hashes = self.capture_leaf_hashes(&capture)?;
        let tree_need = super::memory::tir_full_tree_storage_need_v1(hashes.len() as u64)?;
        let _tree_memory = super::memory::reserve_v1(
            &super::memory::memory_reserver_v1(),
            "tir-full-tree",
            self.class_id,
            &capture.binding.job_context,
            tree_need,
        )?;
        let opening = super::tree::TirStepTreeV1::full(&hashes).opening(index).ok_or("no such leaf")?;
        Ok((opening, capture.leaves[index as usize].clone()))
    }
}

/// **G14's TIR route hooks over this executor** (the four `PalwExecutionBackendV1` TIR methods; contracts in
/// `kaspa_consensus_core::palw_backend`): every answer is read from a [`TirCaptureV1`] of THIS class whose binding IS the claim's
/// admitted public header, or — for the verifier's replay — from this node's own run of the registered program.
impl TirBackendV1 {
    /// The capture `material`, refused unless it is this class's run of exactly `header` (job context, trace root, step tree and
    /// execution root) and its committed rows and ids reproduce the header's trace root.
    fn capture_of_tir_header_v1(
        &self,
        material: &[u8],
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    ) -> Result<TirCaptureV1, String> {
        let capture = self.decode_capture(material)?;
        if kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&capture.binding) != *header {
            return Err("the capture is not this TIR header's run".into());
        }
        tir_authenticate_capture_metadata_v1(&capture, self.class_id, self.prompt_ids_form, self.ladder)?;
        Ok(capture)
    }

    /// The decode view the chain derives for `header` under this class's registration record.
    fn tir_view_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    ) -> Result<kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1, String> {
        let (record, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&self.class, &self.artifact_root)
            .map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, &record, self.ladder)
    }

    /// The step leaf preimages a capture answers for, by index (a dense capture's own; a fold's re-derived by this node's replay,
    /// which must reproduce the capture's binding), and the leaf hashes its tree is over.
    fn tir_leaf_source_v1<R>(
        &self,
        material: &[u8],
        f: impl FnOnce(&dyn Fn(u64) -> Option<PalwStepTileLeafV1>, &[Hash64]) -> Result<R, String>,
    ) -> Result<R, String> {
        use kaspa_consensus_core::palw_tir_court_v1::PalwTirEvidenceStoreV1;
        let capture = self.decode_capture(material)?;
        let hashes = self.capture_leaf_hashes(&capture)?;
        self.with_capture_store(material, self.prompt_ids_form, |store, _| f(&|index| store.step_leaf(index), &hashes.hashes))
    }
}

impl PalwExecutionBackendV1 for TirBackendV1 {
    fn tir_public_capture_need_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        source_length: u64,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirPublicCaptureNeedV1, String> {
        let (binding, public) = self.tir_public_capture_context_v1(header)?;
        let replay_ram = super::memory::tir_executor_storage_need_v1(&self.space, self.artifact.plan(), &header.job_context)?;
        super::public_source::need_v1(&binding, &public, source_length, replay_ram)
    }

    fn open_tir_public_capture_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        source: Arc<dyn kaspa_consensus_core::palw_backend::PalwOriginalCaptureReaderV1>,
        directory: &std::path::Path,
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1>, String> {
        self.tir_public_capture_need_v1(header, source.len_v1())?;
        let (binding, public) = self.tir_public_capture_context_v1(header)?;
        Ok(Box::new(super::public_source::CaptureReader::open_v1(
            source,
            binding,
            public,
            self.prompt_ids_form,
            directory,
            Some(super::public_source::FoldReplayV1 {
                artifact: self.artifact.clone(),
                space: PalwTirStepSpaceV1::new(&self.class).map_err(|e| e.to_string())?,
                fused: self.fused,
                cap: self.ladder,
            }),
        )?))
    }
    fn tir_prepared_capture_need_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        source_length: u64,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirPublicPreparedNeedV1, String> {
        let (binding, public) = self.tir_public_capture_context_v1(header)?;
        super::public_source::prepared_need_v1(&binding, &public, source_length)
    }
    fn reopen_tir_prepared_capture_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        descriptor: &kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1,
        directory: &std::path::Path,
        native_material_until_daa: u64,
        native_now_daa: u64,
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1>, String> {
        let (binding, public) = self.tir_public_capture_context_v1(header)?;
        Ok(Box::new(super::public_source::reopen_prepared_v1(
            binding,
            public,
            descriptor,
            directory,
            native_material_until_daa,
            native_now_daa,
            self.prompt_ids_form,
        )?))
    }
    fn retire_tir_prepared_capture_v1(
        &self,
        descriptor: &kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1,
        directory: &std::path::Path,
        native_material_until_daa: u64,
        native_irreversible_daa: u64,
    ) -> Result<(), String> {
        super::public_source::retire_prepared_v1(descriptor, directory, native_material_until_daa, native_irreversible_daa)
    }
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        // The chain's J5 derivation (`palw_tir_attempt_v1`), from the cached facts.
        let canonical = (self.canonical.declared_prefill_tokens, self.canonical.exact_decode_tokens);
        let (ctx, prompt) = palw_tir_attempt_job_for_anchor_of_v1(&self.facts, &anchor, canonical, self.network_form)
            .ok_or("the anchor's prompt does not commit")?;
        Ok((ctx, prompt.into_iter().map(|t| t as usize).collect()))
    }

    fn set_attempt_rules_v1(&mut self, rules: PalwAttemptRulesV1) {
        self.attempt_rules = rules;
    }

    fn attempt_rules_v1(&self) -> PalwAttemptRulesV1 {
        self.attempt_rules
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let capture = self.capture_run(job, &Self::ids(prompt)?, None)?;
        Ok(self.outcome(&capture))
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Ok(capture) = self.decode_capture(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        let b = &capture.binding;
        // Which question it answers: the claim's job, derived from its block — never the capture's.
        if claim.anchor != Hash64::default() {
            if b.job_context.job_id != claim.anchor {
                return PalwMaterialVerdictV1::Mismatch;
            }
            if let Some(draw) = claim.attempt_draw {
                let Ok((canonical, _)) = self.job_for_anchor(claim.anchor) else {
                    return PalwMaterialVerdictV1::Unverifiable;
                };
                if b.job_context != kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(canonical, draw) {
                    return PalwMaterialVerdictV1::Mismatch;
                }
            }
        }
        if b.committed_execution_root != claim.execution_root || b.full_logits_trace_root != claim.trace_root {
            return PalwMaterialVerdictV1::Mismatch;
        }
        if let Some(committed) = claim.output_root
            && palw_attempt_output_root_v1(&b.job_context, &capture.generated) != committed
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        if verify_tir_binding_v1(b, self.ladder).is_err()
            || !prompt_token_ids_match_v1(self.prompt_ids_form, &capture.prompt, &b.job_context.prompt_token_ids_hash)
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        // **The material answers for the roots — which is not "the execution is honest"** (the
        // families' one reading of this check): the trace root is the committed rows' and ids', and
        // the step root is the committed leaves'. Whether those leaves are the program's is the
        // replay's question (`execute_for_verdict`) and, at a leaf, the court's — a challenger must
        // find a lying capture here to build the close that convicts it.
        let ctx = &b.job_context;
        let trace = if Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated)
        } else {
            Some(kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(ctx, &capture.logits_rows, &capture.generated))
        };
        if trace != Some(b.full_logits_trace_root) {
            return PalwMaterialVerdictV1::Mismatch;
        }
        // The leaves: a dense capture's own preimages; a fold's, re-derived — so a fold answers only
        // for the execution this build reproduces.
        let hashes = match self.capture_leaf_hashes(&capture) {
            Ok(hashes) => hashes,
            Err(_) => return PalwMaterialVerdictV1::Unverifiable,
        };
        if hashes.len() as u64 != b.step_leaf_count
            || step_merkle_root_capped_v1(&hashes, self.ladder).ok() != Some(b.step_merkle_root)
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        PalwMaterialVerdictV1::Matches
    }

    fn capture_shape(&self, material: &[u8]) -> Option<PalwCaptureShapeV1> {
        let capture = self.decode_capture(material).ok()?;
        let layers = self.space.program.schedule.layers.len();
        Some(PalwCaptureShapeV1 {
            job_context: capture.binding.job_context.clone(),
            step_leaf_count: capture.binding.step_leaf_count,
            layer_count: u16::try_from(layers).unwrap_or(u16::MAX),
        })
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<Hash64> {
        let capture = self.decode_capture(material).ok()?;
        if capture.binding.step_leaf_count == 0 || capture.binding.step_leaf_count > self.ladder {
            return None;
        }
        let hashes = self.capture_leaf_hashes(&capture).ok()?;
        Some(tir_bisect_prefix_state_v1(&capture.binding.job_context, &hashes, index))
    }

    /// The replay a seat licenses an attempt by: the roots of this build's own execution of the
    /// job — no capture laid out.
    fn execute_for_verdict(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String> {
        let ids = Self::ids(prompt)?;
        if !prompt_token_ids_match_v1(self.prompt_ids_form, &ids, &job.prompt_token_ids_hash) {
            return Err("the prompt does not commit to the job's prompt hash".into());
        }
        let run = self.runner().run(job, &ids, self.ladder, false, &mut |_| {})?;
        Ok(kaspa_consensus_core::palw_backend::PalwReplayRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            work_leaves: None,
            output_root: Some(run.output_root),
        })
    }

    /// **The IR court is this family's**: the ladder's rungs (`bisect_prefix_state`) and the close
    /// (the IR proofs this backend builds — `cone_close`, `logits_close`, `decode_token_close` — which
    /// the node's court flow asks of it for an IR class).
    fn supports_court(&self) -> bool {
        true
    }

    /// **Proposal B**: this backend's executor against the reference evaluator on a few positions, the reference streamed from the
    /// container ([`super::conformance`]). A composite candidate is tested as its parent is, so it has no job of its own.
    fn executor_conformance_job_v1(&self) -> Option<Box<dyn FnOnce() -> Result<String, String> + Send + 'static>> {
        if self.artifact.composite_ref().is_some() {
            return None;
        }
        let artifact = self.artifact.clone();
        Some(Box::new(move || {
            super::conformance::tir_executor_conformance_v1(&artifact, super::conformance::TIR_CONFORMANCE_POSITIONS_V1)
                .map(|c| c.summary())
        }))
    }

    fn artifact_root_and_leaf_count(&self) -> Result<(Hash64, u32), String> {
        if self.artifact.composite_ref().is_some() {
            // A composite candidate's root is the composite root over its two sections, not its tree's.
            return self.artifact.inventory_root();
        }
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaf_count()))
    }

    /// **RFC-0004 §6.3/§6.7 (spec 17 §17.7.1): a composite candidate's possession tree is its adapter
    /// section** — the section's own root and leaf count. `None` for every single artifact.
    fn artifact_possession_tree_v1(&self) -> Option<Result<(Hash64, u32), String>> {
        match self.artifact.possession_section() {
            Ok(None) => None,
            Ok(Some((_, root, leaves))) => {
                Some(u32::try_from(leaves.len()).map(|n| (root, n)).map_err(|_| "the adapter section has too many leaves".to_string()))
            }
            Err(e) => Some(Err(e)),
        }
    }

    /// The readiness material from the held inventory tree: its root, every leaf hash, and the drawn
    /// leaves' operands in the draw's order. (The IR inventory is in declaration order, not the
    /// legacy digest's name order, so it is served from the tree, never through a digest.) A composite
    /// candidate serves its adapter section instead (the section's root and leaves, the draw rebased).
    fn artifact_readiness_material(
        &self,
        draw: &[u32],
    ) -> Result<(Hash64, Vec<Hash64>, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String> {
        if let Some((split, root, leaves)) = self.artifact.possession_section()? {
            return Ok((root, leaves.to_vec(), self.drawn_operands(draw, split)?));
        }
        let tree = self.artifact.inventory_tree()?;
        Ok((tree.root(), tree.leaves().to_vec(), self.drawn_operands(draw, 0)?))
    }

    fn artifact_readiness_material_streamed_v1(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(Hash64),
    ) -> Option<Result<(Hash64, u32, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String>> {
        Some((|| {
            if let Some((split, root, leaves)) = self.artifact.possession_section()? {
                for leaf in leaves {
                    on_leaf(*leaf);
                }
                let count = u32::try_from(leaves.len()).map_err(|_| "the adapter section has too many leaves".to_string())?;
                return Ok((root, count, self.drawn_operands(draw, split)?));
            }
            let tree = self.artifact.inventory_tree()?;
            for leaf in tree.leaves() {
                on_leaf(*leaf);
            }
            Ok((tree.root(), tree.leaf_count(), self.drawn_operands(draw, 0)?))
        })())
    }

    fn output_root_for_context_v1(&self, context: &PalwJobContextV2, output_token_ids: &[u32]) -> Option<Hash64> {
        Some(palw_attempt_output_root_v1(context, output_token_ids))
    }

    /// RFC-0002 Phase H: an IR class's free-prompt run ([`TirBackendV1::execute_free_prompt_tir_v1`]).
    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_tir_v1(job, prompt_tokens, &mut |_| {}, TirFpDrillV1::None)
    }

    fn execute_free_prompt_streaming(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_tir_v1(job, prompt_tokens, on_token, TirFpDrillV1::None)
    }

    /// G14 drill D-05 on an IR class: one committed lane of step leaf `leaf_index` moved, every root re-derived from that capture.
    fn execute_free_prompt_with_injected_fault(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        leaf_index: u64,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_tir_v1(job, prompt_tokens, &mut |_| {}, TirFpDrillV1::Leaf(leaf_index))
    }

    /// G14 drill D-06 on an IR class: the committed token at decode `position` is the best lane the job's rule does not choose.
    fn execute_free_prompt_with_selection_fault_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        position: u32,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_tir_v1(job, prompt_tokens, &mut |_| {}, TirFpDrillV1::Selection(position))
    }

    /// **G14 R1: the ORIGINAL public binding header of a TIR capture** — what `TFP1` carries and the frame resolves against the
    /// registered class and layout (`PalwPublicTirBindingHeaderV1::of_binding_v1`).
    fn public_execution_binding_header_v1(
        &self,
        material: &[u8],
    ) -> Result<kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1, String> {
        let capture = self.decode_capture(material)?;
        Ok(kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(
            kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&capture.binding),
        ))
    }

    fn execute_with_injected_fault(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        let capture = self.capture_run(job, &Self::ids(prompt)?, Some(leaf_index))?;
        Ok(self.outcome(&capture))
    }

    fn canonical_job_prefill_tokens(&self) -> Option<usize> {
        Some(self.canonical.declared_prefill_tokens as usize)
    }

    fn artifact_row_opening(&self, index: u32) -> Result<PalwArtifactOpeningV1, String> {
        use super::inventory::TirParamOpenerV1;
        self.artifact.param_opening(index).ok_or_else(|| format!("inventory leaf {index} does not open"))
    }

    /// G14: the selected tiled event `(row, tile)` of a TIR claim's committed logits, from this class's capture of exactly the
    /// claim's admitted header — the reference construction (`of_public_rows_view_v1` over the header's view, self-authenticated).
    fn disclose_tir_selected_event_v1(
        &self,
        material: &[u8],
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        row: u32,
        tile: u32,
        maximum_leaves: u64,
    ) -> Result<kaspa_consensus_core::palw_selected_event_source_v1::PalwSelectedTiledEventV1, String> {
        let capture = self.capture_of_tir_header_v1(material, header)?;
        let view = self.tir_view_v1(header)?;
        kaspa_consensus_core::palw_selected_event_source_v1::PalwSelectedTiledEventV1::of_public_rows_view_v1(
            &view,
            &capture.logits_rows,
            &capture.generated,
            kaspa_consensus_core::palw_selected_event_source_v1::PalwSelectedEventCoordinateV1 { row, tile },
            maximum_leaves,
        )
    }

    /// G14: the outside verifier's OWN run of the registered program on the job's authenticated prompt ids, selecting exactly
    /// `exact_decode_tokens` ids with the job's own rule over this run's own prefix: a constrained job's masked greedy key (the
    /// host's token table for the job, refused without it), a V4 job's pipeline, otherwise the shipped greedy rule.
    fn replay_tir_generated_ids_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirReplayV1, String> {
        self.check_tir_own_original_v1(header, job, prompt_token_ids)?;
        let ctx = &header.job_context;
        let select = self.tir_original_selector_v1(job)?;
        let run = self.runner().with_selector(select).run(ctx, prompt_token_ids, self.ladder, false, &mut |_| {})?;
        if run.generated.len() as u64 != u64::from(ctx.exact_decode_tokens) {
            return Err("the TIR replay selected another number of ids than the header's exact decode count".into());
        }
        Ok(kaspa_consensus_core::palw_backend::PalwTirReplayV1 {
            generated_ids: run.generated,
            full_logits_trace_root: run.trace_root,
            execution_root: run.execution_root,
            output_root: run.output_root,
        })
    }

    fn tir_own_step_source_need_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceNeedV1, String> {
        self.check_tir_own_original_v1(header, job, prompt_token_ids)?;
        let ctx = &header.job_context;
        let leaf_count = self.space.leaf_count_capped(ctx, self.ladder).map_err(|e| e.to_string())?;
        let mut disk_bytes = super::own_source::tree_disk_bytes_v1(leaf_count)?;
        if Hash64::from_bytes(self.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            disk_bytes = disk_bytes
                .checked_add(super::own_source::tree_disk_bytes_v1(u64::from(ctx.exact_decode_tokens))?)
                .ok_or("own TIR disk quote overflow")?;
        }
        let selector_bytes = self.tir_original_selector_need_v1(job)?;
        let resident_ram_bytes = super::memory::tir_executor_storage_need_v1(&self.space, self.artifact.plan(), ctx)?
            // Includes generated-vector capacity plus state_after's temporary
            // borrowed-rendering vector across the longest decode prefix.
            .checked_add(u64::from(ctx.exact_decode_tokens).checked_mul(64).ok_or("own TIR ID quote overflow")?)
            .and_then(|n| n.checked_add(selector_bytes))
            .and_then(|n| n.checked_add(1_048_576))
            .ok_or("own TIR RAM quote overflow")?;
        Ok(kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceNeedV1 { resident_ram_bytes, disk_bytes, leaf_count })
    }

    fn open_tir_own_step_source_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
        cache_directory: &std::path::Path,
    ) -> Result<Arc<dyn kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceV1>, String> {
        let need = self.tir_own_step_source_need_v1(header, job, prompt_token_ids)?;
        let runner = self.runner().with_selector(self.tir_original_selector_v1(job)?);
        super::own_source::OwnStepSource::build(cache_directory, header, &runner, need.leaf_count, self.ladder, prompt_token_ids)
            .map(|source| Arc::new(source) as Arc<dyn kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceV1>)
    }

    fn tir_logits_step_leaf_index_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        row: u32,
        lane: u64,
    ) -> Result<u64, String> {
        self.tir_view_v1(header)?;
        self.logits_leaf_holding(&header.job_context, row, lane)
            .ok_or("TIR logits coordinate is outside the original public step space".into())
    }

    fn build_tir_public_cone_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        prompt_token_ids: &[u32],
        trace: Option<&PalwTiledDecodeTokensV1>,
        runs: &[kaspa_consensus_core::palw_tir_court_v1::PalwTirStepRunDisclosureV1],
        leaf_index: u64,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<PalwCourtVerdictProofV2, String> {
        self.build_tir_public_cone_bounded_v1(
            header,
            prompt_token_ids,
            trace,
            runs,
            leaf_index,
            rules,
            tir_court_builder_limits_v1(rules),
            super::copies::DEFAULT_COPY_LIMIT_V1,
        )
        .map(|(proof, _)| proof)
    }

    fn probe_tir_public_cone_v1(
        &self,
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        prompt_token_ids: &[u32],
        trace: Option<&PalwTiledDecodeTokensV1>,
        runs: &[kaspa_consensus_core::palw_tir_court_v1::PalwTirStepRunDisclosureV1],
        leaf_index: u64,
        rules: &PalwTirCourtRulesV1,
        limits: kaspa_consensus_core::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1,
        copy_budget_bytes: u64,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirPublicConeProbeV1, String> {
        let (registered, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&self.class, &self.artifact_root)
            .map_err(|e| e.to_string())?;
        let binding =
            header.resolve_registered_layout_v1(&registered, &self.class.layout, header.committed_execution_root, self.ladder)?;
        if rules.max_step_leaf_count != self.ladder
            || rules.prompt_form != self.prompt_ids_form
            || leaf_index >= binding.step_leaf_count
        {
            return Err("public TIR builder changes native class ladder, prompt form or original leaf".into());
        }
        let mut compact = binding.clone();
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut compact);
        let mut inputs = super::cell::TirRunInputsV1::default();
        for run in runs {
            if run.binding != compact
                || run.range.leaf_hashes.is_empty()
                || run.range.leaf_hashes.len() > kaspa_consensus_core::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1 as usize
            {
                return Err("public TIR operand range changes original header/layout or bounded unit".into());
            }
            inputs.add_run(
                binding.step_leaf_count,
                &binding.step_merkle_root,
                &run.range,
                &run.preimages,
                &binding.job_context,
                &self.class_id,
                self.ladder,
            )?;
        }
        let trace = match trace {
            Some(pin) => TirTraceV1::Summary { rows_root: pin.rows_root, generated: &pin.generated_token_ids, pins: &[] },
            None => TirTraceV1::Absent,
        };
        let runner = self.runner();
        let store = TirEvidenceV1::public_runs_v1(
            &runner,
            &binding,
            prompt_token_ids,
            trace,
            &inputs,
            self.artifact.as_ref(),
            self.prompt_ids_form,
            self.ladder,
        )?;
        let store = store.with_carried_copy_limits_v1(copy_budget_bytes, copy_budget_bytes);
        Ok(store.probe_cone_v1(leaf_index, rules, limits))
    }

    /// G14: step leaf `leaf_index` of a TIR claim — its canonical lanes and its capped opening against the header's step root,
    /// from this class's capture of exactly the header (a fold's leaves re-derived, and refused unless they reach its root).
    fn disclose_tir_step_leaf_v1(
        &self,
        material: &[u8],
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        leaf_index: u64,
        maximum_leaves: u64,
    ) -> Result<(Vec<u8>, PalwStepOpeningV1), String> {
        self.capture_of_tir_header_v1(material, header)?;
        if leaf_index >= header.step_leaf_count {
            return Err(format!("leaf {leaf_index} is past the TIR claim's {} step leaves", header.step_leaf_count));
        }
        self.tir_leaf_source_v1(material, |leaf, hashes| {
            if hashes.len() as u64 != header.step_leaf_count
                || step_merkle_root_capped_v1(hashes, self.ladder).ok() != Some(header.step_merkle_root)
            {
                return Err("the capture's leaves do not reach the TIR header's step root".into());
            }
            let preimage = leaf(leaf_index).ok_or_else(|| format!("step leaf {leaf_index} is not held"))?;
            let opening = kaspa_consensus_core::palw_step_leg::step_opening_from_leaf_source_capped_v1(
                header.step_leaf_count,
                leaf_index,
                maximum_leaves,
                |i| hashes[i as usize],
            )
            .map_err(|e| e.to_string())?;
            Ok((preimage.values_le, opening))
        })
    }

    /// G14: the executor's declaration of a TIR prefix state — `palw_fp_prefix_state_root_v1(class, k, palw_tir_prefix_kv_digest_v1
    /// over the run's own boundary lanes)` — for an aligned boundary only (refused otherwise, as the chain refuses its quote). The
    /// chain also requires the class layout (tag 169) published before such a claim; that is the producer's chain step, not this.
    fn declare_tir_prefix_state_v1(
        &self,
        material: &[u8],
        header: &kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        prefix_tokens: u32,
    ) -> Result<Hash64, String> {
        self.capture_of_tir_header_v1(material, header)?;
        let ctx = &header.job_context;
        self.space
            .prefix_state_leaf_count_v1(ctx, prefix_tokens)
            .ok_or("an unaligned or out-of-prompt TIR prefix boundary has no committed state")?;
        let digest = self.tir_leaf_source_v1(material, |leaf, _| {
            kaspa_consensus_core::palw_tir_prefix_state_v1::palw_tir_prefix_kv_digest_v1(&self.space, ctx, prefix_tokens, |boundary| {
                leaf(boundary.index).map(|p| p.values_le).ok_or_else(|| format!("boundary leaf {} is not held", boundary.index))
            })
        })?;
        Ok(kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&self.class_id, prefix_tokens, &digest))
    }
}
