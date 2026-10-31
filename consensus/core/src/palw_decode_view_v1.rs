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
        // Cheap deny-only guard on the rooted program: in canonical TirProgramV1 Borsh,
        // logits_scheme_id is the final [u8;64] field. This is not a parser, profile
        // authenticator or acceptance classifier. Keep every legacy narrow interpretation;
        // the known wide identity must never reach the flat/Int32 fallback below.
        if header.job_context.trace_scheme_id == Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2)
            || record.program.ends_with(&misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2)
        {
            return Err("wide Q24 requires an explicit V2 decode view; V1 lanes are Int32".into());
        }
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
    // Actual canonical program fixtures, with a coherent public header. DATA only.
    fn profile_fixture(
        dtype: misaka_palw_tir::DType,
    ) -> (crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1, crate::palw_tir_admission_v1::PalwTirClassRecordV1) {
        use misaka_palw_tir::{DType, Node, Prim, Ref};
        let original = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut class = original.binding.class;
        class.layout.max_context = 64;
        let mut program = class.decode_program().unwrap();
        let post = &mut program.blocks[program.schedule.post as usize];
        let mut output = post.nodes[program.logits as usize].out.clone();
        output.dtype = dtype;
        post.nodes.push(Node { prim: Prim::Cast, inputs: vec![Ref::Node(program.logits)], out: output, commit: true });
        program.logits = (post.nodes.len() - 1) as u16;
        class.layout.commit_tiles.push(4);
        if dtype == DType::I64 {
            program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
        }
        class.program = program.encode();
        let (record, context) = if dtype == DType::I64 {
            let profile = crate::palw_tir_class_record_v2::ProfiledTirClassRecordV2::build(
                &class,
                original.binding.artifact_root,
                crate::palw_tir_class_record_v2::TirClassRecordProfileLimitsV2 {
                    max_program_bytes: 1 << 20,
                    max_layout_entries: 8192,
                },
            )
            .unwrap();
            let context = profile.ctx().unwrap();
            (profile.into_storage_row(), context)
        } else {
            let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &original.binding.artifact_root).unwrap();
            let context = crate::palw_tir_attempt_v1::palw_tir_job_context_v1(&record.facts, (7, 2));
            (record, context)
        };
        let mut header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: context,
            artifact_root: original.binding.artifact_root,
            full_logits_trace_root: Hash64::from_u64_word(5),
            step_leaf_count: 40,
            step_merkle_root: Hash64::from_u64_word(6),
            committed_execution_root: Hash64::default(),
        };
        header.job_context.tokenizer_id = record.tokenizer_id;
        refresh_execution_root(&mut header, &record);
        (header, record)
    }
    fn refresh_execution_root(
        header: &mut crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    ) {
        header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &header.full_logits_trace_root,
            &record.facts.class_id,
            header.step_leaf_count,
            &header.step_merkle_root,
        );
    }

    #[test]
    fn deny_only_program_suffix_tracks_the_actual_canonical_schema_field() {
        for dtype in
            [misaka_palw_tir::DType::I8, misaka_palw_tir::DType::I16, misaka_palw_tir::DType::I32, misaka_palw_tir::DType::I64]
        {
            let (_, record) = profile_fixture(dtype);
            let decoded = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).unwrap();
            let encoded = decoded.encode();
            assert_eq!(&encoded[encoded.len() - decoded.logits_scheme_id.len()..], &decoded.logits_scheme_id);
            assert_eq!(encoded.ends_with(&misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2), dtype == misaka_palw_tir::DType::I64);
        }
    }

    #[test]
    fn v1_decode_rejects_wide_program_even_with_a_coherent_lying_old_header() {
        let (mut header, record) = profile_fixture(misaka_palw_tir::DType::I64);
        assert!(!record.facts.tiled);
        assert!(PalwDecodeViewV1::from_tir_header_v1(&header, &record, 1 << 20).unwrap_err().contains("explicit V2 decode view"));
        header.job_context.trace_scheme_id = crate::palw_step_refute::flat_logits_scheme_id_v1();
        refresh_execution_root(&mut header, &record);
        assert!(PalwDecodeViewV1::from_tir_header_v1(&header, &record, 1 << 20).unwrap_err().contains("explicit V2 decode view"));
    }

    #[test]
    fn v1_narrow_decode_is_unchanged_and_rejects_a_wide_header_scheme() {
        for dtype in [misaka_palw_tir::DType::I8, misaka_palw_tir::DType::I16, misaka_palw_tir::DType::I32] {
            let (mut header, record) = profile_fixture(dtype);
            let view = PalwDecodeViewV1::from_tir_header_v1(&header, &record, 1 << 20).unwrap();
            assert_eq!(view.lane, PalwStepLaneV1::Int32);
            assert_eq!(view.logits_scheme_id, crate::palw_step_refute::tiled_logits_scheme_id_v1());
            assert_eq!(view.job_context, header.job_context);
            header.job_context.trace_scheme_id = Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2);
            refresh_execution_root(&mut header, &record);
            assert!(PalwDecodeViewV1::from_tir_header_v1(&header, &record, 1 << 20).unwrap_err().contains("explicit V2 decode view"));
        }
    }
}
