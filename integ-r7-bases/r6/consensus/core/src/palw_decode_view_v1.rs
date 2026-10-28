//! **The decode view of a claim** — exactly what the public logits/generated-id courts read,
//! independent of how the class commits its step space.
//!
//! The tiled and flat logits schemes, the decode pins and the selected-event sources read only:
//! the job context (its hash, decode count and prefill count), the logits width and scheme, the
//! integer lane, the claim's `full_logits_trace_root` and its execution root. A legacy
//! [`crate::palw_step_leg::PalwStepBindingV2`] carries these through its shape profile; an IR
//! (TIR) class carries them through its registered program record and the native-admitted public
//! header. This view lets one court serve both without a TIR binding (whose program and layout
//! are too large for a carrier) ever riding a source answer: the TIR view is derived natively from
//! the admitted header and the registered class record, and the header's execution root is
//! recomputed under the TIR domain before anything is believed.

use crate::Hash64;
use crate::palw_step::PalwStepLaneV1;
use crate::palw_v2::PalwJobContextV2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDecodeViewV1 {
    /// The legacy shape profile id or the IR class id (both are the context's `shape_profile_id`).
    pub class_id: Hash64,
    pub job_context: PalwJobContextV2,
    pub vocab_size: u32,
    pub logits_scheme_id: Hash64,
    pub lane: PalwStepLaneV1,
    pub full_logits_trace_root: Hash64,
    pub committed_execution_root: Hash64,
    pub step_leaf_count: u64,
}

impl PalwDecodeViewV1 {
    /// From a legacy binding the caller has ALREADY verified (`verify_binding_v1`).
    pub fn of_verified_binding_v1(binding: &crate::palw_step_leg::PalwStepBindingV2) -> Self {
        Self {
            class_id: binding.shape_profile.shape_profile_id(),
            job_context: binding.job_context.clone(),
            vocab_size: binding.shape_profile.vocab_size,
            logits_scheme_id: binding.shape_profile.logits_scheme_id,
            lane: binding.shape_profile.lane,
            full_logits_trace_root: binding.full_logits_trace_root,
            committed_execution_root: binding.committed_execution_root,
            step_leaf_count: binding.step_leaf_count,
        }
    }
    /// From a legacy binding, verifying it first.
    pub fn from_binding_v1(binding: &crate::palw_step_leg::PalwStepBindingV2) -> Result<Self, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        Ok(Self::of_verified_binding_v1(binding))
    }
    /// From the native-admitted TIR public header and the class's native registration record.
    /// The header's execution root is recomputed under the TIR domain from its own fields, so a
    /// header cannot pair a foreign trace root with this claim's execution. IR lanes are 4-byte
    /// integers (PALW-TIR-5), so the integer selection rules apply to an IR logits row.
    pub fn from_tir_header_v1(
        header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        record.check_program_v1().map_err(str::to_string)?;
        crate::palw_slash::check_job_context_shape(&header.job_context).map_err(|e| e.to_string())?;
        let class_id = record.facts.class_id;
        if header.version != crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1
            || header.job_context.shape_profile_id != class_id
            || header.job_context.tokenizer_id != record.tokenizer_id
            || header.step_leaf_count == 0
            || header.step_leaf_count > maximum_leaves
            || record.logits_vocab == 0
        {
            return Err("TIR decode view changes the registered class, tokenizer or bounded step space".into());
        }
        let context_hash = header.job_context.context_hash();
        let root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &context_hash,
            &header.full_logits_trace_root,
            &class_id,
            header.step_leaf_count,
            &header.step_merkle_root,
        );
        if root != header.committed_execution_root {
            return Err("TIR public header does not reproduce its committed execution root".into());
        }
        let logits_scheme_id = if record.facts.tiled {
            crate::palw_step_refute::tiled_logits_scheme_id_v1()
        } else {
            crate::palw_step_refute::flat_logits_scheme_id_v1()
        };
        Ok(Self {
            class_id,
            job_context: header.job_context.clone(),
            vocab_size: record.logits_vocab,
            logits_scheme_id,
            lane: PalwStepLaneV1::Int32,
            full_logits_trace_root: header.full_logits_trace_root,
            committed_execution_root: header.committed_execution_root,
            step_leaf_count: header.step_leaf_count,
        })
    }
    pub fn is_tiled_v1(&self) -> bool {
        self.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1()
    }
    pub fn tiles_v1(&self) -> u64 {
        u64::from(self.vocab_size).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64)
    }
}

/// **The decode view's twin of `palw_constrained_job_target_v1`**: the view (derived natively from
/// the claim's admitted header) is the claim's own, and the accepted job reproduces its context.
/// For a TIR view, the header's identity was recomputed natively at admission
/// (`palw_tir_execution_root_v1` under the registered class), so no legacy identity rule applies.
pub fn palw_constrained_job_target_view_v1(
    target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
    view: &PalwDecodeViewV1,
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<(), String> {
    let ctx = &view.job_context;
    if view.committed_execution_root != target.execution_root
        || view.class_id != target.class_id
        || target.lane != Some(crate::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt)
        || target.job_identity == Hash64::default()
        || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx) != target.job_identity
        || view.full_logits_trace_root != target.trace_root
        || job.class_id != target.class_id
        || job.executor_bond != target.executor_bond.0
        || crate::palw_freeprompt_v3::fp_job_id_v3(job) != ctx.job_id
        || crate::palw_fp_execution_v3::palw_fp_execution_seed_v3(job) != ctx.execution_seed
        || job.tokenizer_id != ctx.tokenizer_id
        || job.prompt_token_ids_hash != ctx.prompt_token_ids_hash
        || job.prompt_tokens != ctx.declared_prefill_tokens
        || job.max_context_tokens != ctx.max_context_tokens
        || ctx.exact_decode_tokens == 0
        || ctx.exact_decode_tokens > job.decode_token_limit
    {
        return Err("the decode view or job is not the claim's own constrained execution".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tir_view_recomputes_its_execution_root_and_never_borrows_another_class() {
        let class_id = Hash64::from_u64_word(0x71C);
        let mut record = crate::palw_tir_admission_v1::PalwTirClassRecordV1::test_row_v1(class_id);
        record.logits_vocab = 9000;
        record.program = std::sync::Arc::new(vec![1, 2, 3]);
        record.program_bytes = 3;
        record.graph_ir_root = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&record.program);
        let profile =
            crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("base0 geometry");
        let mut context = crate::palw_base0_profile::rc_job_context(&profile, 2, 3);
        context.shape_profile_id = class_id;
        context.tokenizer_id = record.tokenizer_id;
        let trace = Hash64::from_u64_word(5);
        let step = Hash64::from_u64_word(6);
        let root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(&context.context_hash(), &trace, &class_id, 40, &step);
        let header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: context.clone(),
            artifact_root: Hash64::from_u64_word(7),
            full_logits_trace_root: trace,
            step_leaf_count: 40,
            step_merkle_root: step,
            committed_execution_root: root,
        };
        let view = PalwDecodeViewV1::from_tir_header_v1(&header, &record, 1 << 20).unwrap();
        assert_eq!((view.vocab_size, view.tiles_v1(), view.lane), (9000, 3, PalwStepLaneV1::Int32));
        assert_eq!(view.is_tiled_v1(), record.facts.tiled);
        let mut foreign_trace = header.clone();
        foreign_trace.full_logits_trace_root = Hash64::from_u64_word(55);
        assert!(PalwDecodeViewV1::from_tir_header_v1(&foreign_trace, &record, 1 << 20).is_err());
        let mut other_class = record.clone();
        other_class.facts.class_id = Hash64::from_u64_word(0x71D);
        assert!(PalwDecodeViewV1::from_tir_header_v1(&header, &other_class, 1 << 20).is_err());
        assert!(PalwDecodeViewV1::from_tir_header_v1(&header, &record, 39).is_err(), "the class ladder bounds the step space");
    }
}
