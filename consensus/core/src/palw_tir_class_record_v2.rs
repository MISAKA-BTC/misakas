//! Explicit profile-aware derivation of the EXISTING schema-1 TIR class record.
//! This is DATA, not accepted registration, a rooted-row capability, admission,
//! funds, court compatibility, resource admission or public activation. Callers
//! must obtain the registry key/artifact/layout from their own authenticated
//! view. A wide row must not enter a native map until its explicit V2 admission
//! and every applicable consumer support the profile. Legacy factories stay closed.
//!
//! Schema fields/Borsh/rooted bytes are unchanged. In particular facts.tiled keeps
//! its OLD exact-scheme predicate (false for wide); it is NOT a tiled-family bit.
//! All V2 jobs and tile interpretation dispatch through the resolved profile.

use crate::{
    Hash64,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_admission_v1::{PALW_TIR_CLASS_RECORD_VERSION_V1, PalwTirClassRecordV1},
    palw_tir_attempt_v1::PalwTirJobFactsV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_profile_resolver_v2::{ResolvedTirJobProfileV2, TirProfileResolveErrorV2, TirTraceProfileV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{TirProgramV1, program::MAX_PROGRAM_BYTES};

/// Explicit LOCAL parsing/copy limits, not consensus policy or allocator leases.
#[derive(Clone, Copy, Debug)]
pub struct TirClassRecordProfileLimitsV2 {
    pub max_program_bytes: usize,
    pub max_layout_entries: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirClassRecordProfileErrorV2 {
    Budget,
    Resolve(TirProfileResolveErrorV2),
    RowVersion,
    RowMismatch,
    LogitsWidth,
}

/// No Borsh/serde conversion, new row version, new domain or native permit.
/// The row remains schema1; profile is rederived from its original program.
pub struct ProfiledTirClassRecordV2 {
    row: PalwTirClassRecordV1,
    profile: ResolvedTirJobProfileV2,
}
fn check_caps(
    program: &[u8],
    layout: &PalwTirLayoutV1,
    limits: TirClassRecordProfileLimitsV2,
) -> Result<(), TirClassRecordProfileErrorV2> {
    let entries = layout.commit_tiles.len().checked_add(layout.state_tiles.len()).ok_or(TirClassRecordProfileErrorV2::Budget)?;
    if limits.max_program_bytes == 0
        || limits.max_layout_entries == 0
        || program.is_empty()
        || program.len() > MAX_PROGRAM_BYTES
        || program.len() > limits.max_program_bytes
        || entries > limits.max_layout_entries
    {
        return Err(TirClassRecordProfileErrorV2::Budget);
    }
    Ok(())
}
impl ProfiledTirClassRecordV2 {
    /// Derive only metadata from the original class/artifact. No model inference,
    /// issuance, tier change or model review occurs. Owners reserve bounded
    /// program decoding, identity hashing, dissection metadata and one program
    /// copy before calling; the local limits are not a peak-memory/work quote.
    pub fn build(
        class: &PalwTirClassV1,
        artifact: Hash64,
        limits: TirClassRecordProfileLimitsV2,
    ) -> Result<Self, TirClassRecordProfileErrorV2> {
        check_caps(&class.program, &class.layout, limits)?;
        let profile =
            ResolvedTirJobProfileV2::new(class, artifact, limits.max_program_bytes).map_err(TirClassRecordProfileErrorV2::Resolve)?;
        // A second bounded program decode is metadata work, not a second model
        // run. It avoids changing the already reviewed resolver API/semantics.
        let program =
            class.decode_program().map_err(|e| TirClassRecordProfileErrorV2::Resolve(TirProfileResolveErrorV2::Program(e)))?;
        let logits = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
        let logits_vocab =
            u32::try_from(logits.out.elements_at(1)).ok().filter(|v| *v > 0).ok_or(TirClassRecordProfileErrorV2::LogitsWidth)?;
        let row = PalwTirClassRecordV1 {
            version: PALW_TIR_CLASS_RECORD_VERSION_V1,
            facts: PalwTirJobFactsV1::of(class, &program, profile.class_id()),
            graph_ir_root: class.graph_ir_root(),
            layout_digest: class.layout_digest(),
            tokenizer_id: class.tokenizer_id,
            prim_set_id: Hash64::from_bytes(program.prim_set_id),
            logits_vocab,
            program_bytes: u32::try_from(class.program.len()).map_err(|_| TirClassRecordProfileErrorV2::Budget)?,
            // Pure graph/cone metadata: this helper neither reads nor sizes
            // legacy leaf bytes and does not authorize the old dissection court.
            dissected: crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&program),
            program: std::sync::Arc::new(class.program.clone()),
        };
        Ok(Self { row, profile })
    }
    /// Join an existing stored schema1 record to its independently authenticated
    /// registry key, artifact and layout preimage. Full row derivation is compared,
    /// not just program hash/length. This is still NOT authenticated provenance.
    /// At peak there are bounded temporary program/layout copies and decodes;
    /// the returned row shares the original stored program Arc (no retained copy).
    pub fn resolve_record(
        expected_class_id: Hash64,
        record: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        artifact: Hash64,
        limits: TirClassRecordProfileLimitsV2,
    ) -> Result<Self, TirClassRecordProfileErrorV2> {
        check_caps(&record.program, layout, limits)?;
        if record.version != PALW_TIR_CLASS_RECORD_VERSION_V1 {
            return Err(TirClassRecordProfileErrorV2::RowVersion);
        }
        if record.program.len() != record.program_bytes as usize {
            return Err(TirClassRecordProfileErrorV2::RowMismatch);
        }
        let decoded = TirProgramV1::decode_canonical(&record.program)
            .map_err(|e| TirClassRecordProfileErrorV2::Resolve(TirProfileResolveErrorV2::Program(e)))?;
        // Check exact geometry BEFORE cloning a caller's layout Vecs. The
        // bound comes from the parsed original program, not an invented policy.
        let committed = decoded.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
        if layout.commit_tiles.len() != committed || layout.state_tiles.len() != decoded.states.len() {
            return Err(TirClassRecordProfileErrorV2::Resolve(TirProfileResolveErrorV2::LayoutGeometry));
        }
        drop(decoded);
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: record.program.as_ref().clone(),
            layout: layout.clone(),
            tokenizer_id: record.tokenizer_id,
        };
        let mut derived = Self::build(&class, artifact, limits)?;
        if derived.profile.class_id() != expected_class_id || derived.row != *record {
            return Err(TirClassRecordProfileErrorV2::RowMismatch);
        }
        derived.row.program = std::sync::Arc::clone(&record.program);
        Ok(derived)
    }
    pub fn profile(&self) -> TirTraceProfileV2 {
        self.profile.profile()
    }
    pub fn class_id(&self) -> Hash64 {
        self.profile.class_id()
    }
    pub fn canonical(&self) -> Option<(u32, u32)> {
        self.profile.canonical()
    }
    pub fn ctx(&self) -> Option<PalwJobContextV2> {
        self.profile.ctx()
    }
    pub fn job_for_anchor(&self, anchor: &Hash64, form: PalwPromptIdsFormV1) -> Option<(PalwJobContextV2, Vec<u32>)> {
        self.profile.job_for_anchor(anchor, form)
    }
    /// Geometry only. Does not authorize old four-byte trace/opening consumers.
    pub fn logits_tiles(&self) -> u32 {
        match self.profile() {
            TirTraceProfileV2::FlatI32 => 1,
            TirTraceProfileV2::TiledI32 | TirTraceProfileV2::TiledI64Q24 => {
                self.row.logits_vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32)
            }
        }
    }
    /// Storage data only. A wide row's facts.tiled is false by frozen definition;
    /// old row.logits_tiles()/raw-facts context APIs MUST NOT dispatch wide work.
    pub fn storage_row(&self) -> &PalwTirClassRecordV1 {
        &self.row
    }
    /// No installation or permission. Native owners must select an explicit V2
    /// admission and profile-aware consumers before storing this unchanged row.
    pub fn into_storage_row(self) -> PalwTirClassRecordV1 {
        self.row
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_HELD, LOGITS_I64_Q24_SCHEME_V2};
    use misaka_palw_tir::{DType, Node, Prim, Ref};
    fn limits() -> TirClassRecordProfileLimitsV2 {
        TirClassRecordProfileLimitsV2 { max_program_bytes: MAX_PROGRAM_BYTES, max_layout_entries: 8192 }
    }
    fn fixture(profile: TirTraceProfileV2, held: bool, vocab: usize) -> (PalwTirClassV1, Hash64) {
        let seed = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let original = if vocab == 8 {
            seed
        } else {
            let template = seed.binding.job_context;
            crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in_vocab(
                move |class, id, prompt| {
                    let mut ctx = template;
                    ctx.shape_profile_id = id;
                    ctx.tokenizer_id = class.tokenizer_id;
                    ctx.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(prompt);
                    ctx
                },
                None,
                &|_, _, _| 0,
                vocab,
            )
        };
        let mut class = original.binding.class;
        class.layout.max_context = 64;
        let mut program = class.decode_program().unwrap();
        program.logits_scheme_id = profile.program_scheme_id().as_bytes();
        if held {
            program.history_bound = HISTORY_BOUND_V1_HELD;
        }
        if profile == TirTraceProfileV2::TiledI64Q24 {
            let post = &mut program.blocks[program.schedule.post as usize];
            let mut out = post.nodes[program.logits as usize].out.clone();
            out.dtype = DType::I64;
            post.nodes.push(Node { prim: Prim::Cast, inputs: vec![Ref::Node(program.logits)], out, commit: true });
            program.logits = (post.nodes.len() - 1) as u16;
            class.layout.commit_tiles.push(4);
        }
        class.program = program.encode();
        (class, original.binding.artifact_root)
    }
    #[test]
    fn narrow_schema1_record_rooted_bytes_and_jobs_are_identical() {
        for profile in [TirTraceProfileV2::FlatI32, TirTraceProfileV2::TiledI32] {
            let (class, artifact) = fixture(profile, false, 8);
            let (old, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).unwrap();
            let explicit = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap();
            assert_eq!(explicit.storage_row(), &old);
            assert_eq!(borsh::to_vec(explicit.storage_row()).unwrap(), borsh::to_vec(&old).unwrap());
            assert_eq!(explicit.storage_row().rooted_bytes_v1(), old.rooted_bytes_v1());
            assert_eq!(borsh::to_vec(&old.facts).unwrap().len(), 74);
            let loaded =
                ProfiledTirClassRecordV2::resolve_record(old.facts.class_id, &old, &class.layout, artifact, limits()).unwrap();
            assert!(std::sync::Arc::ptr_eq(&loaded.storage_row().program, &old.program));
            assert_eq!(loaded.logits_tiles(), old.logits_tiles());
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                let anchor = Hash64::from_u64_word(8);
                assert_eq!(
                    loaded.job_for_anchor(&anchor, form),
                    crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(&old.facts, &anchor, (7, 2), form)
                );
            }
        }
    }
    #[test]
    fn wide_schema1_payload_resolves_explicit_profile_without_redefining_tiled_bit() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 4097);
        assert!(crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).is_err());
        let explicit = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap();
        assert_eq!(explicit.storage_row().version, 1);
        assert!(!explicit.storage_row().facts.tiled); // Frozen old exact-scheme predicate.
        assert_eq!(explicit.storage_row().logits_tiles(), 1); // Why raw consumers stay closed.
        assert_eq!(explicit.logits_tiles(), 2);
        let raw = borsh::to_vec(explicit.storage_row()).unwrap();
        let row: PalwTirClassRecordV1 = borsh::from_slice(&raw).unwrap();
        let resolved = ProfiledTirClassRecordV2::resolve_record(explicit.class_id(), &row, &class.layout, artifact, limits()).unwrap();
        assert_eq!(resolved.profile(), TirTraceProfileV2::TiledI64Q24);
        assert_eq!(resolved.ctx().unwrap().trace_scheme_id.as_bytes(), LOGITS_I64_Q24_SCHEME_V2);
        let (job, _) = resolved.job_for_anchor(&Hash64::from_u64_word(8), PalwPromptIdsFormV1::Flat).unwrap();
        assert_eq!(job.trace_scheme_id.as_bytes(), LOGITS_I64_Q24_SCHEME_V2);
        assert_eq!(job.tokenizer_id, Hash64::default());
        assert_eq!(job.cu_ruleset_id, Hash64::default());
        assert_eq!(job.max_context_tokens, class.layout.max_context + 1);
        assert_eq!(borsh::to_vec(resolved.storage_row()).unwrap(), raw);
        assert!(crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).is_err());
    }
    #[test]
    fn stored_row_artifact_key_layout_and_derived_fact_substitutions_are_rejected() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
        let original = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap().into_storage_row();
        let id = original.facts.class_id;
        let reject = |key, row: &PalwTirClassRecordV1, layout: &PalwTirLayoutV1, art| {
            assert!(ProfiledTirClassRecordV2::resolve_record(key, row, layout, art, limits()).is_err())
        };
        reject(Hash64::from_u64_word(99), &original, &class.layout, artifact);
        reject(id, &original, &class.layout, Hash64::from_u64_word(99));
        let mut layout = class.layout.clone();
        layout.commit_tiles[0] *= 2;
        reject(id, &original, &layout, artifact);
        for field in 0..7 {
            let mut row = original.clone();
            match field {
                0 => row.facts.tiled = true,
                1 => row.facts.token_bound += 1,
                2 => row.facts.held = !row.facts.held,
                3 => row.logits_vocab += 1,
                4 => row.prim_set_id = Hash64::default(),
                5 => row.dissected.push((0, 0)),
                _ => row.tokenizer_id = Hash64::default(),
            }
            reject(id, &row, &class.layout, artifact);
        }
    }
    #[test]
    fn caps_precede_copy_or_decode_and_no_unknown_scheme_falls_back() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
        let row = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap().into_storage_row();
        let mut cap = limits();
        cap.max_program_bytes = class.program.len() - 1;
        assert!(matches!(
            ProfiledTirClassRecordV2::resolve_record(row.facts.class_id, &row, &class.layout, artifact, cap),
            Err(TirClassRecordProfileErrorV2::Budget)
        ));
        let mut cap = limits();
        cap.max_layout_entries = 0;
        assert!(matches!(ProfiledTirClassRecordV2::build(&class, artifact, cap), Err(TirClassRecordProfileErrorV2::Budget)));
        let mut unknown = class;
        let mut program = unknown.decode_program().unwrap();
        program.logits_scheme_id = [0x88; 64];
        // An i32 final declaration keeps unknown-scheme refusal independent of
        // the wide-only commit exception in structural validation.
        program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype = DType::I32;
        unknown.program = program.encode();
        assert!(matches!(
            ProfiledTirClassRecordV2::build(&unknown, artifact, limits()),
            Err(TirClassRecordProfileErrorV2::Resolve(TirProfileResolveErrorV2::UnsupportedScheme))
        ));
    }
    #[test]
    fn original_held_prompt_form_survives_schema1_record_reconstruction() {
        let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, true, 8);
        let row = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap().into_storage_row();
        let loaded = ProfiledTirClassRecordV2::resolve_record(row.facts.class_id, &row, &class.layout, artifact, limits()).unwrap();
        let (job, ids) = loaded.job_for_anchor(&Hash64::from_u64_word(8), PalwPromptIdsFormV1::Flat).unwrap();
        assert_eq!(
            job.prompt_token_ids_hash,
            crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &ids).unwrap()
        );
    }

    #[test]
    fn dissection_sites_remain_graph_metadata_with_narrow_history_and_wide_final() {
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::{Dim, TensorType};
        let mut pb = ProgramBuilder::new(4, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let history = pb.hist_state("history", DType::I32, &[1], 3, true);
        let carry = TensorType::fixed(DType::I32, &[1]);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.iota(DType::I32, &[Dim::Fixed(1)], 0, 1, 0);
            b.finish(&[x])
        };
        let (layer, site) = {
            let mut b = pb.block("layer", vec![carry.clone()]);
            let rows = b.hist_append(history, Ref::CarryIn(0));
            let sum = b.reduce_sum(rows, 0, DType::I32);
            let Ref::Node(site) = sum else { unreachable!() };
            (b.finish(&[sum]), site)
        };
        let (post, logits) = {
            let mut b = pb.block("post", vec![carry]);
            let out = b.cast(Ref::CarryIn(0), DType::I64);
            b.commit(out);
            let Ref::Node(logits) = out else { unreachable!() };
            (b.finish(&[]), logits)
        };
        let mut program = pb.finish(pre, vec![layer], post, logits);
        program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            layout: PalwTirLayoutV1 {
                version: crate::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: vec![4; 3],
                state_tiles: vec![4],
            },
            tokenizer_id: Hash64::from_u64_word(19),
        };
        let artifact = Hash64::default(); // Metadata-only declaration; no native registration receipt.
        let wide = ProfiledTirClassRecordV2::build(&class, artifact, limits()).unwrap();
        assert_eq!(wide.storage_row().dissected, vec![(layer, site)]);
        // Only the final output profile/dtype differs. The graph-only detector
        // retains the SAME H-reduction sites; no leaf bytes/court path is used.
        let mut narrow = class;
        program.logits_scheme_id = TirTraceProfileV2::TiledI32.program_scheme_id().as_bytes();
        program.blocks[post as usize].nodes[logits as usize].out.dtype = DType::I32;
        narrow.program = program.encode();
        let (row, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&narrow, &artifact).unwrap();
        assert_eq!(wide.storage_row().dissected, row.dissected);
    }
}
