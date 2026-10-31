//! Positive Court R2 role adapters (RFC-0044 / ADR-0212).
//!
//! The Miner uses the ordinary executor with `NoSink`, retaining only its output
//! IDs. The Panel independently runs once, records its own computation and
//! assembles G14 TIR court material. Neither adapter grants consensus credit.

use super::{
    TirCaptureV1, TirClassRunnerV1, artifact::TirArtifactV1, evidence::TirEvidenceV1, inventory::TirParamOpenerV1,
    wide_q24::TirWideCaptureV2,
};
use crate::{NoSink, Slice, TirExecutor, TirParams, TirPlan};
use kaspa_consensus_core::palw_step_leg::{step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_court_v2::{ExactPartV2, ExactPolicyV2, MinerResultV2, PanelWitnessV2},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{PalwTirCourtRulesV1, PalwTirTraceEventDisclosureV1, tir_logits_event_disclosure_v1},
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1},
    palw_v2::PalwJobContextV2,
};

/// Open an existing, bounded PALWTIR1 source assembled by a cold Panel. The
/// caller owns the local cache path and keeps it immutable during open/replay,
/// as required by the existing mapped artifact loader. A rooted `registered`
/// class and admitted signed result must come from the role admission layer.
///
/// This authenticates data identity only. It neither qualifies a model nor
/// decides a claim, and never invokes a Miner witness/trace generation API.
pub fn open_panel_artifact_v2(
    path: &std::path::Path,
    result: &MinerResultV2,
    registered: &PalwTirClassV1,
    max_file_bytes: u64,
    max_header_bytes: u32,
) -> Result<TirArtifactV1, String> {
    let class_id = registered.class_id(&result.artifact);
    if result.version != 2
        || result.job.version != 2
        || registered.version != kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1
        || result.job.shape_profile_id != class_id
        || result.job.tokenizer_id != registered.tokenizer_id
        || !result.sampling.is_greedy()
    {
        return Err("positive Panel artifact result/class identity mismatch".into());
    }
    let metadata = std::fs::metadata(path).map_err(|e| format!("positive Panel artifact metadata: {e}"))?;
    if !metadata.is_file() || metadata.len() > max_file_bytes || max_header_bytes == 0 {
        return Err("positive Panel artifact file/header reservation exceeded".into());
    }
    // The bounded reader checks both declared lengths before allocating a header
    // and validates tensor extents before the normal loader maps any model data.
    let bounded = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(path, max_file_bytes, max_header_bytes)
        .map_err(|e| format!("positive Panel artifact preflight: {e}"))?;
    if bounded.header.program != registered.program
        || bounded.header.layout != borsh::to_vec(&registered.layout).map_err(|e| e.to_string())?
        || bounded.header.tokenizer_id != registered.tokenizer_id.as_bytes()
    {
        return Err("positive Panel artifact header differs from rooted registration".into());
    }
    let scheme = Hash64::from_bytes(bounded.program.logits_scheme_id);
    if result.job.trace_scheme_id != scheme
        || (scheme != tiled_logits_scheme_id_v1() && scheme != kaspa_consensus_core::palw_logits_q24_v2::scheme_id())
    {
        return Err("positive Panel artifact trace profile mismatch".into());
    }
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(registered).map_err(|e| e.to_string())?;
    let job = space.job_shape(&result.job).map_err(|e| e.to_string())?;
    if job.positions >= result.job.max_context_tokens {
        return Err("positive Panel artifact job context exceeded".into());
    }
    let artifact = TirArtifactV1::open(path)?;
    if artifact.container().file_len != bounded.file_len
        || artifact.container().header != bounded.header
        || artifact.class()? != *registered
    {
        return Err("positive Panel artifact changed after bounded preflight".into());
    }
    artifact.check_weights_unmodified()?;
    let (root, _) = artifact.inventory_root()?;
    artifact.check_weights_unmodified()?;
    if root != result.artifact || artifact.class()?.class_id(&root) != class_id {
        return Err("positive Panel artifact inventory differs from signed result/rooted class".into());
    }
    Ok(artifact)
}

/// Local execution/capture admission, supplied explicitly by the caller. These
/// are development process bounds, not qualified public-network parameters.
#[derive(Clone, Copy, Debug)]
pub struct RoleLimitsV2 {
    pub max_prefill: u32,
    pub max_decode: u32,
    pub max_leaves: u64,
    pub max_capture_bytes: u64,
}

/// No trace root or intermediate values: this is the inference-only Miner result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinerInferenceV2 {
    pub generated: Vec<u32>,
    pub output_root: Hash64,
}

fn validate_inputs(
    class: &PalwTirClassV1,
    artifact: Hash64,
    plan: &TirPlan,
    ctx: &PalwJobContextV2,
    prompt: &[u32],
    form: PalwPromptIdsFormV1,
    limits: RoleLimitsV2,
) -> Result<(), String> {
    if ctx.version != 2
        || ctx.declared_prefill_tokens == 0
        || ctx.exact_decode_tokens == 0
        || ctx.declared_prefill_tokens > limits.max_prefill
        || ctx.exact_decode_tokens > limits.max_decode
        || prompt.len() != ctx.declared_prefill_tokens as usize
        || prompt.iter().any(|t| *t >= plan.program.token_bound)
        || !prompt_token_ids_match_v1(form, prompt, &ctx.prompt_token_ids_hash)
        || ctx.shape_profile_id != class.class_id(&artifact)
        || class.program != plan.program.encode()
        || ctx.tokenizer_id != class.tokenizer_id
    {
        return Err("positive role job/class/input admission refused".into());
    }
    let scheme = Hash64::from_bytes(plan.program.logits_scheme_id);
    if scheme != tiled_logits_scheme_id_v1() && scheme != kaspa_consensus_core::palw_logits_q24_v2::scheme_id() {
        return Err("positive roles initially support tiled i32 and explicit wide Q24 TIR only".into());
    }
    if ctx.trace_scheme_id != scheme {
        return Err("positive role trace scheme mismatch".into());
    }
    let positions = ctx.declared_prefill_tokens.checked_add(ctx.exact_decode_tokens - 1).ok_or("positive role position overflow")?;
    if positions > class.layout.max_context || positions >= ctx.max_context_tokens {
        return Err("positive role context length exceeded".into());
    }
    Ok(())
}

/// Ordinary inference, no `TirClassRunner`, capture, Merkle tree, or evidence
/// builder. State required by the model remains normal inference state.
#[allow(clippy::too_many_arguments)]
pub fn miner_infer_v2(
    class: &PalwTirClassV1,
    artifact: Hash64,
    plan: &TirPlan,
    params: &TirParams<'_>,
    ctx: &PalwJobContextV2,
    prompt: &[u32],
    form: PalwPromptIdsFormV1,
    limits: RoleLimitsV2,
) -> Result<MinerInferenceV2, String> {
    validate_inputs(class, artifact, plan, ctx, prompt, form, limits)?;
    let mut executor = TirExecutor::new(plan, params).map_err(|e| e.to_string())?;
    let positions = ctx.declared_prefill_tokens + ctx.exact_decode_tokens - 1;
    let mut generated = Vec::with_capacity(ctx.exact_decode_tokens as usize);
    for position in 0..positions {
        let token = if position < ctx.declared_prefill_tokens {
            prompt[position as usize]
        } else {
            *generated.last().ok_or("positive inference decode input absent")?
        };
        let run_post = position + 1 >= ctx.declared_prefill_tokens;
        executor.step_opt(token, &mut NoSink, run_post).map_err(|e| e.to_string())?;
        if run_post {
            // Read the executor's normal output buffer directly. No copied
            // logits row is retained, and ties stay at the first lane.
            let best = match executor.logits().1 {
                Slice::I32(values) => greedy_index(values),
                Slice::I64(values) => greedy_index(values),
                _ => return Err("positive inference logits dtype".into()),
            }
            .ok_or("empty positive logits")?;
            generated.push(u32::try_from(best).map_err(|_| "positive vocabulary exceeds u32")?);
        }
    }
    Ok(MinerInferenceV2 { output_root: palw_attempt_output_root_v1(ctx, &generated), generated })
}

fn greedy_index<T: Ord>(values: &[T]) -> Option<usize> {
    values.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).map(|x| x.0)
}

/// Fill a new-version result object after inference. Signature creation belongs
/// to the identity holder; neither output nor a signature implies Node checking.
#[allow(clippy::too_many_arguments)]
pub fn miner_result_v2(
    mut result: MinerResultV2,
    class: &PalwTirClassV1,
    plan: &TirPlan,
    params: &TirParams<'_>,
    prompt: &[u32],
    form: PalwPromptIdsFormV1,
    limits: RoleLimitsV2,
) -> Result<(MinerResultV2, Vec<u32>), String> {
    if result.version != 2 || !result.sampling.is_greedy() {
        return Err("positive role supports v2 fixed-length greedy only".into());
    }
    let run = miner_infer_v2(class, result.artifact, plan, params, &result.job, prompt, form, limits)?;
    result.output = run.output_root;
    Ok((result, run.generated))
}

/// A Panel-owned capture, never a Miner trace commitment. It is intentionally
/// private and cannot be constructed by submitting a producer's 'clean' flag.
pub struct PanelCaptureV2 {
    capture: Capture,
}
enum Capture {
    I32(TirCaptureV1),
    Wide(TirWideCaptureV2),
}

impl PanelCaptureV2 {
    /// One independent Panel replay records arithmetic and evidence data together.
    /// Before running, reserve a conservative retained capture bound (all leaves
    /// at the largest leaf width, hashes, rows, and per-leaf DTO overhead).
    pub fn replay(
        runner: &TirClassRunnerV1<'_>,
        class: &PalwTirClassV1,
        artifact: Hash64,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        form: PalwPromptIdsFormV1,
        limits: RoleLimitsV2,
    ) -> Result<Self, String> {
        validate_inputs(class, artifact, runner.plan, ctx, prompt, form, limits)?;
        if runner.space.program != runner.plan.program
            || runner.space.layout != class.layout
            || runner.class_id != class.class_id(&artifact)
        {
            return Err("positive Panel runner identity mismatch".into());
        }
        let count = capture_reservation(runner, ctx, prompt.len(), limits)?;
        let mut leaves = Vec::new();
        leaves
            .try_reserve_exact(usize::try_from(count).map_err(|_| "positive Panel leaf count exceeds usize")?)
            .map_err(|e| format!("positive Panel leaf allocation refused: {e}"))?;
        let mut on_leaf = |leaf: &super::run::TirLeafOutV1<'_>| leaves.push(leaf.preimage.clone());
        let capture = if runner.plan.program.wide_logits_q24() {
            let run = runner.run_wide_q24_v2(ctx, prompt, limits.max_leaves, &mut on_leaf)?;
            let binding = PalwTirStepBindingV1 {
                version: PALW_TIR_STEP_BINDING_VERSION_V1,
                job_context: ctx.clone(),
                class: class.clone(),
                artifact_root: artifact,
                full_logits_trace_root: run.trace_root,
                step_leaf_count: run.leaf_count,
                step_merkle_root: run.step_merkle_root,
                committed_execution_root: run.execution_root,
            };
            Capture::Wide(TirWideCaptureV2 {
                binding,
                prompt: prompt.to_vec(),
                logits_rows: run.logits_rows,
                generated: run.generated,
                leaves,
            })
        } else {
            let run = runner.run(ctx, prompt, limits.max_leaves, true, &mut on_leaf)?;
            let binding = PalwTirStepBindingV1 {
                version: PALW_TIR_STEP_BINDING_VERSION_V1,
                job_context: ctx.clone(),
                class: class.clone(),
                artifact_root: artifact,
                full_logits_trace_root: run.trace_root,
                step_leaf_count: run.leaf_count,
                step_merkle_root: run.step_merkle_root,
                committed_execution_root: run.execution_root,
            };
            Capture::I32(TirCaptureV1 {
                binding,
                prompt: prompt.to_vec(),
                logits_rows: run.logits_rows,
                generated: run.generated,
                leaves,
            })
        };
        Ok(Self { capture })
    }
    /// Data identity of the recorded replay. It authenticates the binding and
    /// retained material; it is never a Node judgement of arithmetic correctness.
    pub fn capture_id(&self) -> Hash64 {
        self.binding().committed_execution_root
    }

    /// Reuse the existing versioned capture framing. Check encoded length before
    /// allocating a second copy; atomic disk replacement belongs to the caller.
    pub fn encode_recorded(&self, max_bytes: u64) -> Result<Vec<u8>, String> {
        let body_length = match &self.capture {
            Capture::I32(c) => borsh::object_length(c),
            Capture::Wide(c) => borsh::object_length(c),
        }
        .map_err(|e| e.to_string())?;
        let length = body_length.checked_add(8).ok_or("positive Panel capture frame overflows")?;
        if u64::try_from(length).map_err(|_| "positive Panel frame exceeds u64")? > max_bytes {
            return Err("positive Panel encoded capture exceeds reservation".into());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(|e| format!("positive Panel frame allocation refused: {e}"))?;
        match &self.capture {
            Capture::I32(c) => {
                bytes.extend_from_slice(&TirCaptureV1::MAGIC);
                borsh::to_writer(&mut bytes, c).map_err(|e| e.to_string())?;
            }
            Capture::Wide(c) => {
                bytes.extend_from_slice(&TirWideCaptureV2::MAGIC);
                borsh::to_writer(&mut bytes, c).map_err(|e| e.to_string())?;
            }
        }
        Ok(bytes)
    }

    /// Restore only material belonging to the caller's authenticated candidate.
    /// The ordinary bounded decoder reserves at most two frame lengths plus
    /// 16 MiB; include the input frame too before invoking it. Every commitment,
    /// vector shape and canonical leaf position is checked without replaying the
    /// model. A restored candidate still requires all of the Node's exact checks.
    /// `witness` must be authenticated by the role/Node admission layer; this
    /// method neither verifies signatures nor turns the snapshot into a verdict.
    #[allow(clippy::too_many_arguments)]
    pub fn restore_recorded(
        bytes: &[u8],
        runner: &TirClassRunnerV1<'_>,
        result: &MinerResultV2,
        witness: &PanelWitnessV2,
        policy: ExactPolicyV2,
        rules: &PalwTirCourtRulesV1,
        limits: RoleLimitsV2,
    ) -> Result<Self, String> {
        let decode_reservation = (bytes.len() as u128) * 3 + (16u128 << 20);
        if decode_reservation > limits.max_capture_bytes as u128 {
            return Err("positive Panel restore exceeds capture reservation".into());
        }
        if witness.version != 2
            || witness.result != result.id()
            || witness.policy != policy.id()
            || rules.max_step_leaf_count != policy.max_leaves
            || rules.limits.max_elements != policy.max_elements_per_leaf
            || rules.limits.max_terms != policy.max_terms_per_leaf
        {
            return Err("positive Panel restore candidate/policy mismatch".into());
        }
        let capture = if runner.plan.program.wide_logits_q24() {
            Capture::Wide(TirWideCaptureV2::decode(bytes)?)
        } else {
            Capture::I32(TirCaptureV1::decode(bytes)?)
        };
        let panel = Self { capture };
        if panel.binding() != &witness.binding || panel.generated() != witness.generated {
            return Err("positive Panel restored material is not the expected candidate".into());
        }
        let (prompt, leaves) = match &panel.capture {
            Capture::I32(c) => (&c.prompt, &c.leaves),
            Capture::Wide(c) => (&c.prompt, &c.leaves),
        };
        let binding = panel.binding();
        validate_inputs(&binding.class, binding.artifact_root, runner.plan, &binding.job_context, prompt, rules.prompt_form, limits)?;
        if runner.space.program != runner.plan.program
            || runner.space.layout != binding.class.layout
            || runner.class_id != binding.class.class_id(&binding.artifact_root)
        {
            return Err("positive Panel restore runner identity mismatch".into());
        }
        let cap = limits.max_leaves.min(policy.max_leaves);
        let count = capture_reservation(runner, &binding.job_context, prompt.len(), limits)?;
        if count != binding.step_leaf_count || leaves.len() as u64 != count {
            return Err("positive Panel restore requires a complete dense capture".into());
        }
        // The input frame, decoded DTO storage, leaf hashes and the Merkle fold
        // coexist. The fold holds at most one full level plus its half-sized
        // successor; include those with the separately retained leaf hashes.
        if decode_reservation + (count as u128) * 160 > limits.max_capture_bytes as u128 {
            return Err("positive Panel restore hashing exceeds capture reservation".into());
        }
        // This checks result context/artifact and fixed-length greedy selection;
        // the terminal output is deliberately allowed to differ for fraud filing.
        if panel.witness(result, policy, witness.attempt)? != *witness {
            return Err("positive Panel restore signed result mismatch".into());
        }
        match &panel.capture {
            Capture::I32(c) => super::backend::tir_authenticate_capture_metadata_v1(c, runner.class_id, rules.prompt_form, cap)?,
            Capture::Wide(c) => {
                c.authenticate(cap)?;
                let program = &runner.plan.program;
                let vocab = program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.elements_at(1);
                if c.generated.len() as u64 != u64::from(binding.job_context.exact_decode_tokens)
                    || c.logits_rows.len() != c.generated.len()
                    || c.generated.iter().any(|id| u64::from(*id) >= vocab)
                    || c.generated.iter().take(c.generated.len().saturating_sub(1)).any(|id| *id >= program.token_bound)
                {
                    return Err("positive Panel restored wide token/trace shape".into());
                }
            }
        }
        let ctx_hash = binding.job_context.context_hash();
        let mut hashes = Vec::new();
        hashes.try_reserve_exact(leaves.len()).map_err(|e| format!("positive Panel restore hash allocation refused: {e}"))?;
        for (index, preimage) in leaves.iter().enumerate() {
            let leaf = runner.space.leaf_at(&binding.job_context, index as u64).ok_or("positive Panel restored leaf absent")?;
            let is_wide = leaf.dtype == misaka_palw_tir::DType::I64;
            let width = if is_wide { 8u64 } else { 4 };
            let version = if is_wide { 2 } else { kaspa_consensus_core::palw_tir_step_v1::PALW_TIR_STEP_LEAF_VERSION_V1 };
            if preimage.coord != leaf.coord
                || preimage.value_count != leaf.value_count
                || preimage.version != version
                || preimage.values_le.len() as u64 != u64::from(leaf.value_count) * width
            {
                return Err(format!("positive Panel restored leaf {index} has noncanonical shape/order"));
            }
            hashes.push(step_tile_leaf_hash_v1(&ctx_hash, &runner.class_id, preimage));
        }
        if step_merkle_root_capped_v1(&hashes, cap).map_err(|e| e.to_string())? != binding.step_merkle_root {
            return Err("positive Panel restored leaves do not match witness root".into());
        }
        Ok(panel)
    }

    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        match &self.capture {
            Capture::I32(c) => &c.binding,
            Capture::Wide(c) => &c.binding,
        }
    }
    pub fn generated(&self) -> &[u32] {
        match &self.capture {
            Capture::I32(c) => &c.generated,
            Capture::Wide(c) => &c.generated,
        }
    }
    pub fn witness(&self, result: &MinerResultV2, policy: ExactPolicyV2, attempt: u32) -> Result<PanelWitnessV2, String> {
        if result.version != 2
            || result.job != self.binding().job_context
            || result.artifact != self.binding().artifact_root
            || !result.sampling.is_greedy()
        {
            return Err("positive Panel capture/result mismatch".into());
        }
        Ok(PanelWitnessV2 {
            version: 2,
            result: result.id(),
            policy: policy.id(),
            attempt,
            binding: self.binding().clone(),
            generated: self.generated().to_vec(),
        })
    }

    /// Build existing G14 exact cone material once per canonical leaf, carrying
    /// the trace bridge on every logits tile. The callback may persist/send one
    /// bounded part at a time. Returning `Err` stops construction immediately.
    pub fn stream_parts(
        &self,
        runner: &TirClassRunnerV1<'_>,
        params: &dyn TirParamOpenerV1,
        rules: &PalwTirCourtRulesV1,
        max_part_bytes: u64,
        consume: &mut dyn FnMut(u64, ExactPartV2) -> Result<(), String>,
    ) -> Result<(), String> {
        self.stream_parts_range(runner, params, rules, 0, self.binding().step_leaf_count, max_part_bytes, consume)
    }

    /// Build exactly one canonical range, starting at the authenticated Node
    /// cursor after restart/takeover. Previously submitted prefixes are neither
    /// regenerated nor replayed. A callback failure stops before the next part;
    /// callers resume from the Node cursor, never from a local "last sent" flag.
    #[allow(clippy::too_many_arguments)]
    pub fn stream_parts_range(
        &self,
        runner: &TirClassRunnerV1<'_>,
        params: &dyn TirParamOpenerV1,
        rules: &PalwTirCourtRulesV1,
        first: u64,
        count: u64,
        max_part_bytes: u64,
        consume: &mut dyn FnMut(u64, ExactPartV2) -> Result<(), String>,
    ) -> Result<(), String> {
        validate_range(self.binding().step_leaf_count, first, count, max_part_bytes)?;
        self.check_runner(runner)?;
        if count == 0 {
            return Ok(());
        }
        self.prepare_parts(runner, params, rules)?.stream_range(first, count, max_part_bytes, consume)
    }

    fn check_runner(&self, runner: &TirClassRunnerV1<'_>) -> Result<(), String> {
        if runner.plan.program != runner.space.program
            || runner.space.layout != self.binding().class.layout
            || runner.class_id != self.binding().class.class_id(&self.binding().artifact_root)
        {
            return Err("positive Panel submission runner identity mismatch".into());
        }
        Ok(())
    }

    /// Prepare the dense evidence tree once, then stream many bounded batches.
    /// Reusing this object avoids rebuilding the whole Merkle tree per batch;
    /// retained tree/scratch storage still uses the existing memory admission.
    pub fn prepare_parts<'a>(
        &'a self,
        runner: &'a TirClassRunnerV1<'a>,
        params: &'a dyn TirParamOpenerV1,
        rules: &'a PalwTirCourtRulesV1,
    ) -> Result<PreparedPanelPartsV2<'a>, String> {
        self.check_runner(runner)?;
        let store = match &self.capture {
            Capture::I32(c) => TirEvidenceV1::dense(
                runner,
                &c.binding,
                &c.prompt,
                &c.logits_rows,
                &c.generated,
                &c.leaves,
                params,
                rules.prompt_form,
                rules.max_step_leaf_count,
            )?,
            Capture::Wide(c) => TirEvidenceV1::dense_wide_q24_v2(runner, c, params, rules.prompt_form, rules.max_step_leaf_count)?,
        };
        Ok(PreparedPanelPartsV2 { panel: self, runner, rules, store })
    }
}

/// Reusable Panel-side evidence working set. This is construction state only;
/// every submitted part must still be authenticated and checked by the Node.
pub struct PreparedPanelPartsV2<'a> {
    panel: &'a PanelCaptureV2,
    runner: &'a TirClassRunnerV1<'a>,
    rules: &'a PalwTirCourtRulesV1,
    store: TirEvidenceV1<'a>,
}

impl PreparedPanelPartsV2<'_> {
    pub fn stream_range(
        &self,
        first: u64,
        count: u64,
        max_part_bytes: u64,
        consume: &mut dyn FnMut(u64, ExactPartV2) -> Result<(), String>,
    ) -> Result<(), String> {
        let end = validate_range(self.panel.binding().step_leaf_count, first, count, max_part_bytes)?;
        for index in first..end {
            let cone = self.store.cone_refutation(index, self.rules).map_err(|e| e.to_string())?;
            let leaf =
                self.runner.space.leaf_at(&self.panel.binding().job_context, index).ok_or("positive Panel missing canonical leaf")?;
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence as usize == self.runner.space.occurrences().len() - 1 && node == self.runner.plan.program.logits =>
                {
                    let row = (leaf.position + 1)
                        .checked_sub(self.panel.binding().job_context.declared_prefill_tokens)
                        .ok_or("positive logits row")?;
                    let tile = first_element / PALW_LOGITS_TILE_LANES as u64;
                    Some(match &self.panel.capture {
                        Capture::I32(c) => tir_logits_event_disclosure_v1(
                            &c.binding,
                            &c.logits_rows,
                            &c.generated,
                            row,
                            u8::try_from(tile).map_err(|_| "positive i32 trace tile exceeds legacy event index")?,
                        )
                        .ok_or("positive i32 trace event")?,
                        Capture::Wide(c) => PalwTirTraceEventDisclosureV1::WideQ24V2 {
                            binding: Box::new(c.binding.clone()),
                            event: c
                                .event(row, u32::try_from(tile).map_err(|_| "positive wide tile exceeds u32")?)
                                .ok_or("positive wide trace event")?,
                        },
                    })
                }
                _ => None,
            };
            let bytes = borsh::object_length(&cone).map_err(|e| e.to_string())? as u64
                + trace
                    .as_ref()
                    .map(|t| borsh::object_length(t).map(|n| n as u64))
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .unwrap_or(0);
            if bytes > max_part_bytes {
                return Err(format!("positive Panel part {index} exceeds {max_part_bytes} bytes"));
            }
            consume(index, ExactPartV2 { cone, trace })?;
        }
        Ok(())
    }
}

/// Conservative retained capture and hashing budget, shared by replay/restore.
fn capture_reservation(
    runner: &TirClassRunnerV1<'_>,
    ctx: &PalwJobContextV2,
    prompt_len: usize,
    limits: RoleLimitsV2,
) -> Result<u64, String> {
    let job = runner.space.job_shape(ctx).map_err(|e| e.to_string())?;
    let count = runner.space.leaf_count_capped(ctx, limits.max_leaves).map_err(|e| e.to_string())?;
    let lanes = runner.space.maximum_leaf_lanes_v1(&job);
    let vocab = runner.plan.program.blocks[runner.plan.program.schedule.post as usize].nodes[runner.plan.program.logits as usize]
        .out
        .elements_at(1);
    let width = if runner.plan.program.wide_logits_q24() { 8u128 } else { 4u128 };
    let capture_bound = (count as u128) * ((lanes as u128) * width + 512)
        + (ctx.exact_decode_tokens as u128) * ((vocab as u128) * width + 64)
        + (prompt_len as u128) * 4;
    if capture_bound > limits.max_capture_bytes as u128 {
        return Err("positive Panel capture reservation exceeded".into());
    }
    Ok(count)
}

fn validate_range(total: u64, first: u64, count: u64, max_part_bytes: u64) -> Result<u64, String> {
    let end =
        first.checked_add(count).filter(|end| *end <= total).ok_or("positive Panel submission range is outside canonical coverage")?;
    if max_part_bytes == 0 || (count == 0 && first != total) {
        return Err("positive Panel submission range/part allowance is empty".into());
    }
    Ok(end)
}
