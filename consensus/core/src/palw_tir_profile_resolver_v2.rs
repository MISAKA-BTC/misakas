//! Explicit, bounded metadata resolution for canonical narrow/wide TIR jobs.
//! This is not registration, rooted class authentication, a native row, a
//! resource lease or permission to execute/credit a claim. In particular, a
//! recomputed class ID must still match independently authenticated metadata.
//! No old fact encoding, wire, fence, runtime or court is changed here.

use crate::{
    Hash64,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_attempt_v1::PalwTirJobFactsV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{DType, TirError, program::LOGITS_I64_Q24_SCHEME_V2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirTraceProfileV2 {
    FlatI32,
    TiledI32,
    TiledI64Q24,
}
impl TirTraceProfileV2 {
    /// Program scheme identity. The legacy flat context intentionally uses a
    /// DIFFERENT trace identity; callers must not substitute this into old jobs.
    pub fn program_scheme_id(self) -> Hash64 {
        match self {
            Self::FlatI32 => crate::palw_step_refute::flat_logits_scheme_id_v1(),
            Self::TiledI32 => crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            Self::TiledI64Q24 => Hash64::from_bytes(LOGITS_I64_Q24_SCHEME_V2),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirProfileResolveErrorV2 {
    /// Caller-selected raw program cap only, NOT a total heap/work reservation.
    ProgramBytes,
    ClassVersion,
    Program(TirError),
    LayoutGeometry,
    ContextBound,
    UnsupportedScheme,
    OutputDtype,
}

/// Private, nonserialized metadata: do not persist its legacy facts as if they
/// described the wide trace. There is deliberately no raw-facts getter or
/// conversion into a native permission. Caller-owned class and decoded program
/// storage need their own preflight reservation; the raw cap is not such a lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTirJobProfileV2 {
    facts: PalwTirJobFactsV1,
    profile: TirTraceProfileV2,
}
impl ResolvedTirJobProfileV2 {
    pub fn new(class: &PalwTirClassV1, artifact: Hash64, max_program_bytes: usize) -> Result<Self, TirProfileResolveErrorV2> {
        if max_program_bytes == 0 || class.program.len() > max_program_bytes {
            return Err(TirProfileResolveErrorV2::ProgramBytes);
        }
        if class.version != PALW_TIR_CLASS_VERSION_V1 || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
            return Err(TirProfileResolveErrorV2::ClassVersion);
        }
        let program = class.decode_program().map_err(TirProfileResolveErrorV2::Program)?;
        // Derived exact lengths bound layout hashing without an implicit local
        // policy ceiling. This is not the full layout/admission/court validator.
        let committed = program.blocks.iter().flat_map(|block| &block.nodes).filter(|node| node.commit).count();
        if class.layout.commit_tiles.len() != committed || class.layout.state_tiles.len() != program.states.len() {
            return Err(TirProfileResolveErrorV2::LayoutGeometry);
        }
        if class.layout.max_context > program.history_bound {
            return Err(TirProfileResolveErrorV2::ContextBound);
        }
        let scheme = Hash64::from_bytes(program.logits_scheme_id);
        let profile = if scheme == crate::palw_step_refute::flat_logits_scheme_id_v1() {
            TirTraceProfileV2::FlatI32
        } else if scheme == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
            TirTraceProfileV2::TiledI32
        } else if program.wide_logits_q24() {
            TirTraceProfileV2::TiledI64Q24
        } else {
            return Err(TirProfileResolveErrorV2::UnsupportedScheme);
        };
        let logits = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
        let dtype = if profile == TirTraceProfileV2::TiledI64Q24 { DType::I64 } else { DType::I32 };
        if logits.out.dtype != dtype {
            return Err(TirProfileResolveErrorV2::OutputDtype);
        }
        // Class ID binds the carried original program/layout/tokenizer AND this
        // artifact. It is recomputed, never accepted as an argument/peer hint.
        let facts = PalwTirJobFactsV1::of(class, &program, class.class_id(&artifact));
        Ok(Self { facts, profile })
    }
    pub fn class_id(&self) -> Hash64 {
        self.facts.class_id
    }
    pub fn max_context(&self) -> u32 {
        self.facts.max_context
    }
    pub fn token_bound(&self) -> u32 {
        self.facts.token_bound
    }
    pub fn profile(&self) -> TirTraceProfileV2 {
        self.profile
    }
    pub fn canonical(&self) -> Option<(u32, u32)> {
        crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(self.facts.max_context)
    }
    pub fn prompt_ids_form(&self, network: PalwPromptIdsFormV1) -> PalwPromptIdsFormV1 {
        self.facts.prompt_ids_form(network)
    }
    fn apply_profile(&self, mut ctx: PalwJobContextV2) -> PalwJobContextV2 {
        // Preserve every old narrow byte, including flat trace_scheme_id_v2.
        if self.profile == TirTraceProfileV2::TiledI64Q24 {
            ctx.trace_scheme_id = self.profile.program_scheme_id();
        }
        ctx
    }
    /// Canonical yardstick only. No caller-selected prefill/decode pair; no
    /// tokenizer/CU rewrite (both remain canonical zero, class ID binds tokenizer).
    pub fn ctx(&self) -> Option<PalwJobContextV2> {
        Some(self.apply_profile(crate::palw_tir_attempt_v1::palw_tir_job_context_v1(&self.facts, self.canonical()?)))
    }
    /// Existing anchor-derived prompt/seed/job formula, then the explicit trace
    /// profile. Returns the FULL undrawn canonical context, not a draw/assignment
    /// authorization. The program's unchanged history bound bounds the canonical
    /// prompt count. Caller reserves prompt/hash/context storage and work before
    /// calling (canonical() exposes the exact count); no native lease is issued.
    pub fn job_for_anchor(&self, anchor: &Hash64, network_form: PalwPromptIdsFormV1) -> Option<(PalwJobContextV2, Vec<u32>)> {
        let (ctx, ids) =
            crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(&self.facts, anchor, self.canonical()?, network_form)?;
        Some((self.apply_profile(ctx), ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_attempt_v1::{palw_tir_attempt_job_for_anchor_v1, palw_tir_canonical_context_v1};
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_HELD, MAX_PROGRAM_BYTES};
    use misaka_palw_tir::{Node, Prim, Ref};

    fn fixture(profile: TirTraceProfileV2, held: bool) -> (PalwTirClassV1, Hash64) {
        let original = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut class = original.binding.class;
        class.layout.max_context = 64;
        // Registered tokenizer is independently bound by class ID, not copied
        // into the canonical job's deliberately zero tokenizer field.
        class.tokenizer_id = Hash64::from_u64_word(99);
        let mut program = class.decode_program().unwrap();
        program.logits_scheme_id = profile.program_scheme_id().as_bytes();
        if held {
            program.history_bound = HISTORY_BOUND_V1_HELD;
        }
        if profile == TirTraceProfileV2::TiledI64Q24 {
            let post = &mut program.blocks[program.schedule.post as usize];
            let mut output = post.nodes[program.logits as usize].out.clone();
            output.dtype = DType::I64;
            post.nodes.push(Node { prim: Prim::Cast, inputs: vec![Ref::Node(program.logits)], out: output, commit: true });
            program.logits = (post.nodes.len() - 1) as u16;
            class.layout.commit_tiles.push(4);
        }
        class.program = program.encode();
        (class, original.binding.artifact_root)
    }
    fn resolve(class: &PalwTirClassV1, artifact: Hash64) -> ResolvedTirJobProfileV2 {
        ResolvedTirJobProfileV2::new(class, artifact, MAX_PROGRAM_BYTES).unwrap()
    }

    #[test]
    fn explicit_wide_profile_preserves_full_canonical_job_and_original_zero_fields() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        let resolved = resolve(&class, artifact);
        assert_eq!(resolved.class_id(), class.class_id(&artifact));
        assert_eq!(resolved.profile(), TirTraceProfileV2::TiledI64Q24);
        assert_eq!(resolved.canonical(), Some((7, 2)));
        let yardstick = resolved.ctx().unwrap();
        assert_eq!(yardstick.trace_scheme_id.as_bytes(), LOGITS_I64_Q24_SCHEME_V2);
        let anchor = Hash64::from_bytes([17; 64]);
        let (job, ids) = resolved.job_for_anchor(&anchor, PalwPromptIdsFormV1::Flat).unwrap();
        assert_eq!((job.declared_prefill_tokens, job.exact_decode_tokens, job.max_context_tokens), (7, 2, 65));
        assert_eq!(job.shape_profile_id, resolved.class_id());
        assert_eq!(job.tokenizer_id, Hash64::default());
        assert_ne!(job.tokenizer_id, class.tokenizer_id);
        assert_eq!(job.cu_ruleset_id, Hash64::default());
        assert_eq!(job.job_id, anchor);
        assert_eq!(job.execution_seed, [17; 32]);
        assert_eq!(ids, crate::palw_attempt_rules_v1::palw_attempt_prompt_ids_v1(&anchor, u64::from(resolved.token_bound()), 7));
        assert_eq!(
            job.prompt_token_ids_hash,
            crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &ids).unwrap()
        );
        let mut expected = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_context_v1(
            &resolved.facts,
            &anchor,
            (7, 2),
            job.prompt_token_ids_hash,
        );
        expected.trace_scheme_id = Hash64::from_bytes(LOGITS_I64_Q24_SCHEME_V2);
        assert_eq!(job, expected);
    }

    #[test]
    fn old_class_taking_helpers_fail_closed_on_wide() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        let id = class.class_id(&artifact);
        assert!(PalwTirJobFactsV1::of_class(&class, id).is_err());
        assert!(palw_tir_canonical_context_v1(&class, id, (7, 2)).is_none());
        assert!(palw_tir_attempt_job_for_anchor_v1(&class, id, (7, 2), &Hash64::default(), PalwPromptIdsFormV1::Flat).is_none());
    }

    #[test]
    fn old_fact_borsh_field_order_width_and_round_trip_are_frozen() {
        let facts = PalwTirJobFactsV1 {
            class_id: Hash64::from_bytes([0x5a; 64]),
            max_context: 128,
            token_bound: 17,
            tiled: true,
            held: false,
        };
        let mut expected = vec![0x5a; 64];
        expected.extend_from_slice(&128u32.to_le_bytes());
        expected.extend_from_slice(&17u32.to_le_bytes());
        expected.extend_from_slice(&[1, 0]);
        assert_eq!(expected.len(), 74);
        assert_eq!(borsh::to_vec(&facts).unwrap(), expected);
        assert_eq!(borsh::from_slice::<PalwTirJobFactsV1>(&expected).unwrap(), facts);
    }

    #[test]
    fn narrow_contexts_and_jobs_keep_all_their_old_bytes_including_flat_identity() {
        for profile in [TirTraceProfileV2::FlatI32, TirTraceProfileV2::TiledI32] {
            let (class, artifact) = fixture(profile, false);
            let resolved = resolve(&class, artifact);
            let id = class.class_id(&artifact);
            let old = palw_tir_canonical_context_v1(&class, id, (7, 2)).unwrap();
            let new = resolved.ctx().unwrap();
            assert_eq!(old, new);
            assert_eq!(borsh::to_vec(&old).unwrap(), borsh::to_vec(&new).unwrap());
            if profile == TirTraceProfileV2::FlatI32 {
                assert_eq!(new.trace_scheme_id, crate::palw_v2::trace_scheme_id_v2());
                assert_ne!(new.trace_scheme_id, profile.program_scheme_id());
            }
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                let anchor = Hash64::from_u64_word(18);
                assert_eq!(
                    resolved.job_for_anchor(&anchor, form),
                    palw_tir_attempt_job_for_anchor_v1(&class, id, (7, 2), &anchor, form)
                );
            }
        }
    }

    #[test]
    fn class_artifact_and_tokenizer_substitutions_cannot_reuse_the_resolved_identity() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        let original = resolve(&class, artifact);
        let changed_artifact = resolve(&class, Hash64::from_u64_word(987));
        let mut changed_class = class.clone();
        changed_class.tokenizer_id = Hash64::from_u64_word(988);
        let changed_class = resolve(&changed_class, artifact);
        for other in [changed_artifact, changed_class] {
            assert_ne!(original.class_id(), other.class_id());
            assert_ne!(original.ctx().unwrap().context_hash(), other.ctx().unwrap().context_hash());
        }
    }

    #[test]
    fn raw_program_cap_is_checked_before_decode_and_unknown_scheme_never_falls_back() {
        let (mut class, artifact) = fixture(TirTraceProfileV2::TiledI32, false);
        let len = class.program.len();
        assert_eq!(ResolvedTirJobProfileV2::new(&class, artifact, len - 1).unwrap_err(), TirProfileResolveErrorV2::ProgramBytes);
        assert_eq!(ResolvedTirJobProfileV2::new(&class, artifact, 0).unwrap_err(), TirProfileResolveErrorV2::ProgramBytes);
        resolve(&class, artifact);
        class.program[0] = 255;
        assert_eq!(ResolvedTirJobProfileV2::new(&class, artifact, len - 1).unwrap_err(), TirProfileResolveErrorV2::ProgramBytes);
        let (mut class, artifact) = fixture(TirTraceProfileV2::TiledI32, false);
        let mut program = class.decode_program().unwrap();
        program.logits_scheme_id = [0x77; 64];
        class.program = program.encode();
        assert_eq!(
            ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap_err(),
            TirProfileResolveErrorV2::UnsupportedScheme
        );
    }

    #[test]
    fn held_prompt_form_is_kept_for_each_resolved_profile() {
        for profile in [TirTraceProfileV2::FlatI32, TirTraceProfileV2::TiledI32, TirTraceProfileV2::TiledI64Q24] {
            let (class, artifact) = fixture(profile, true);
            let resolved = resolve(&class, artifact);
            assert_eq!(resolved.prompt_ids_form(PalwPromptIdsFormV1::Flat), PalwPromptIdsFormV1::MerkleV1);
            let (job, ids) = resolved.job_for_anchor(&Hash64::from_u64_word(18), PalwPromptIdsFormV1::Flat).unwrap();
            assert_eq!(
                job.prompt_token_ids_hash,
                crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &ids).unwrap()
            );
            assert_ne!(
                job.prompt_token_ids_hash,
                crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &ids).unwrap()
            );
        }
    }

    #[test]
    fn layout_hashing_and_prompt_counts_are_bounded_by_derived_original_geometry() {
        let (mut class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        class.layout.commit_tiles.push(1);
        assert_eq!(
            ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap_err(),
            TirProfileResolveErrorV2::LayoutGeometry
        );
        class.layout.commit_tiles.pop();
        class.layout.state_tiles.push(1);
        assert_eq!(
            ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap_err(),
            TirProfileResolveErrorV2::LayoutGeometry
        );
        class.layout.state_tiles.pop();
        class.layout.max_context = u32::MAX;
        assert_eq!(
            ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap_err(),
            TirProfileResolveErrorV2::ContextBound
        );
    }

    #[test]
    fn old_class_record_factory_refuses_wide_instead_of_persisting_legacy_facts() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        // The source is a canonical wide program with a valid four-lane tile.
        class.decode_program().unwrap();
        assert_eq!(*class.layout.commit_tiles.last().unwrap(), 4);
        let error = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).err().unwrap();
        assert_eq!(error.kind, misaka_palw_tir::TirErrorKind::NormalForm);
        assert_eq!(error.msg, "wide Q24 class records require an explicit v2 record factory, not legacy job facts");
        // Existing narrow records remain available and bind the same identity.
        let (narrow, artifact) = fixture(TirTraceProfileV2::TiledI32, false);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&narrow, &artifact).unwrap();
        assert_eq!(record.facts.class_id, narrow.class_id(&artifact));
    }

    const LEGACY_WIDTH_REFUSAL: &str = "wide Q24 requires an explicit v2 step space; legacy step leaves are four-byte lanes";
    fn assert_width_refusal(error: crate::palw_tir_step_v1::PalwTirStepError) {
        assert_eq!(error, crate::palw_tir_step_v1::PalwTirStepError::Layout(LEGACY_WIDTH_REFUSAL.into()));
    }

    #[test]
    fn both_old_step_space_constructors_refuse_wide_before_legacy_geometry() {
        use crate::palw_tir_step_v1::PalwTirStepSpaceV1;
        let (class, _) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        let program = class.decode_program().unwrap();
        let info = misaka_palw_tir::validate::validate(&program).unwrap();
        assert_eq!(*class.layout.commit_tiles.last().unwrap(), 4);
        assert_width_refusal(PalwTirStepSpaceV1::new(&class).err().unwrap());
        assert_width_refusal(PalwTirStepSpaceV1::from_program(program, info, class.layout.clone()).err().unwrap());
        // A narrow counterpart has the exact same geometry and a valid layout,
        // so unrelated tile/schedule restrictions cannot mask these guards.
        let mut counterpart = class;
        let mut program = counterpart.decode_program().unwrap();
        program.logits_scheme_id = TirTraceProfileV2::TiledI32.program_scheme_id().as_bytes();
        program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype = DType::I32;
        counterpart.program = program.encode();
        PalwTirStepSpaceV1::new(&counterpart).unwrap();
    }

    #[test]
    fn old_binding_verifier_reports_width_even_with_consistent_identity_count_and_root() {
        use crate::palw_tir_step_v1::{
            PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
            verify_tir_binding_v1,
        };
        let (wide, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false);
        let mut narrow = wide.clone();
        let mut program = narrow.decode_program().unwrap();
        program.logits_scheme_id = TirTraceProfileV2::TiledI32.program_scheme_id().as_bytes();
        program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype = DType::I32;
        narrow.program = program.encode();
        let class_id = narrow.class_id(&artifact);
        let ctx = palw_tir_canonical_context_v1(&narrow, class_id, (7, 2)).unwrap();
        let space = PalwTirStepSpaceV1::new(&narrow).unwrap();
        let count = space.leaf_count_capped(&ctx, 1_000_000).unwrap();
        let trace_root = Hash64::from_u64_word(123);
        let step_root = Hash64::from_u64_word(124);
        // This establishes the binding's structural identities, not execution
        // correctness or the availability of actual leaf/opening material.
        let mut binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            committed_execution_root: palw_tir_execution_root_v1(&ctx.context_hash(), &trace_root, &class_id, count, &step_root),
            job_context: ctx,
            class: narrow,
            artifact_root: artifact,
            full_logits_trace_root: trace_root,
            step_leaf_count: count,
            step_merkle_root: step_root,
        };
        verify_tir_binding_v1(&binding, 1_000_000).unwrap();
        // Keep a valid legacy job-context scheme deliberately: otherwise its
        // earlier shape check would hide the missing wide step-space guard.
        binding.class = wide;
        let class_id = binding.class.class_id(&artifact);
        binding.job_context.shape_profile_id = class_id;
        binding.committed_execution_root =
            palw_tir_execution_root_v1(&binding.job_context.context_hash(), &trace_root, &class_id, count, &step_root);
        assert_width_refusal(verify_tir_binding_v1(&binding, 1_000_000).err().unwrap());
    }
}
