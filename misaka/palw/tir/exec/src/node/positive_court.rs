//! Positive Court R2 role adapters (RFC-0044 / ADR-0212).
//!
//! The Miner uses the ordinary executor with `NoSink`, retaining only its output
//! IDs. The Panel independently runs once, records its own computation and
//! assembles G14 TIR court material. Neither adapter grants consensus credit.

use super::{TirCaptureV1, TirClassRunnerV1, evidence::TirEvidenceV1, inventory::TirParamOpenerV1, wide_q24::TirWideCaptureV2};
use crate::{NoSink, Slice, TirExecutor, TirParams, TirPlan};
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
            }.ok_or("empty positive logits")?;
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
        if runner.space.program != runner.plan.program || runner.class_id != class.class_id(&artifact) {
            return Err("positive Panel runner identity mismatch".into());
        }
        let job = runner.space.job_shape(ctx).map_err(|e| e.to_string())?;
        let count = runner.space.leaf_count_capped(ctx, limits.max_leaves).map_err(|e| e.to_string())?;
        let lanes = runner.space.maximum_leaf_lanes_v1(&job);
        let vocab = runner.plan.program.blocks[runner.plan.program.schedule.post as usize].nodes[runner.plan.program.logits as usize]
            .out
            .elements_at(1);
        let width = if runner.plan.program.wide_logits_q24() { 8u128 } else { 4u128 };
        let capture_bound = (count as u128) * ((lanes as u128) * width + 512)
            + (ctx.exact_decode_tokens as u128) * ((vocab as u128) * width + 64)
            + (prompt.len() as u128) * 4;
        if capture_bound > limits.max_capture_bytes as u128 {
            return Err("positive Panel capture reservation exceeded".into());
        }
        let mut leaves = Vec::with_capacity(count as usize);
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
        for index in 0..self.binding().step_leaf_count {
            let cone = store.cone_refutation(index, rules).map_err(|e| e.to_string())?;
            let leaf = runner.space.leaf_at(&self.binding().job_context, index).ok_or("positive Panel missing canonical leaf")?;
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence as usize == runner.space.occurrences().len() - 1 && node == runner.plan.program.logits =>
                {
                    let row = (leaf.position + 1)
                        .checked_sub(self.binding().job_context.declared_prefill_tokens)
                        .ok_or("positive logits row")?;
                    let tile = first_element / PALW_LOGITS_TILE_LANES as u64;
                    Some(match &self.capture {
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
