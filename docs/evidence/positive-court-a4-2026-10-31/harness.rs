pub use core_base::*;
pub mod palw_native_resource_v1 {pub use core_base::palw_native_resource_v1::*; pub mod wire {pub use core_base::palw_native_resource_v1::*;}}
pub mod palw_tir_court_v1 {
pub use core_base::palw_tir_court_v1::*;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use crate::Hash64;
use crate::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOpeningV1, palw_artifact_multiproof_borsh_len_v1, palw_artifact_multiproof_from_openings_v1,
    palw_artifact_multiproof_sibling_count_v1, palw_artifact_opening_path_len_v1, palw_artifact_operand_borsh_len_v1,
    verify_artifact_multiproof_v1,
};
use crate::palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, verify_prompt_ids_opening_v1};
use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_step_leg::{
    PalwStepFaultV1, PalwStepOpeningV1, PalwStepRangeOpeningV1, PalwStepRefutationVerdictV1, PalwStepTileLeafV1,
    step_opening_root_capped_v1, step_range_opening_root_capped_v1, step_refutation_evidence_id, step_tile_leaf_hash_v1,
};
use crate::palw_step_refute::{
    PALW_DECODE_TOKEN_EVIDENCE_KIND, PALW_LOGITS_TILE_LANES, PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, PalwStepInputRowV1,
    PalwStepRefuteError, PalwTiledDecodePinV1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1, tiled_logits_outer_root_v1,
    tiled_logits_scheme_id_v1, tiled_tile_authenticate_v1,
};
use crate::palw_tir_artifact_v1::{
    PALW_TIR_ROW_PIECE_BYTES_V1, palw_tir_inventory_leaf_count_v1, palw_tir_param_instances_v1, palw_tir_tensor_bytes_v1,
};
use crate::palw_tir_step_v1::{
    PALW_TIR_STEP_BINDING_VERSION_V1, PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirJobShapeV1, PalwTirLeafKindV1, PalwTirLeafV1,
    PalwTirStepBindingV1, PalwTirStepSpaceV1, PalwTirVerifiedBindingV1, palw_tir_execution_root_v1,
};
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::demand::{
    DemandContext, DemandError, DemandLimits, DemandRequest, DemandSource, DemandTarget, DemandWork, StateSupply, eval_demanded,
    hist_row_node_v1, state_occurrence_v1, state_writer_v1,
};
use misaka_palw_tir::interval::{Interval, analyze_ranges};
use misaka_palw_tir::{DType, Prim, TirError, TirErrorKind, TirProgramV1, TirResult};
pub(crate) mod test_support {
    use super::*;
    use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
    use crate::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1, step_opening_v1};
    use crate::palw_step_refute::{PalwTiledDecodeTokensV1, base0_decode_token_select_v1, tiled_logits_rows_root_v1};
    use crate::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1};
    use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
    use crate::palw_tir_step_v1::palw_tir_leaf_preimage_v1;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Interpreter, MapParams, Ref, RunState, Tensor, TensorType};
    use std::borrow::Cow;

    pub(crate) const PREFILL: u32 = 3;
    pub(crate) const DECODE: u32 = 2;

    /// A committed IR execution and everything a store answers from.
    pub(crate) struct TinyExecution {
        pub(crate) binding: PalwTirStepBindingV1,
        pub(crate) preimages: Vec<PalwStepTileLeafV1>,
        pub(crate) hashes: Vec<Hash64>,
        pub(crate) ops: Vec<PalwArtifactOperandV1>,
        param_cache: std::cell::RefCell<BTreeMap<u32, PalwArtifactOpeningV1>>,
        pub(crate) prompt: Vec<u32>,
        pub(crate) rows: Vec<Vec<i32>>,
        pub(crate) generated: Vec<u32>,
    }

    impl PalwTirEvidenceStoreV1 for TinyExecution {
        fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
            self.preimages.get(index as usize).cloned()
        }
        fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
            step_opening_v1(&self.hashes, index).ok()
        }
        fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
            step_merkle_range_siblings_v1(&self.hashes, first as usize, count as usize).ok()
        }
        fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
            {
                let mut cache = self.param_cache.borrow_mut();
                if !cache.contains_key(&leaf) { cache.insert(leaf, open_artifact_leaf_v1(&self.ops, leaf)?); }
                cache.get(&leaf).cloned()
            }
        }
        fn prompt_token_ids(&self) -> Option<Vec<u32>> {
            Some(self.prompt.clone())
        }
        fn prompt_ids_opening(&self, _tile: u32) -> Option<PalwPromptIdsOpeningV1> {
            None
        }
        fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
            Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
                rows_root: tiled_logits_rows_root_v1(&self.binding.job_context, &self.rows)?,
                generated_token_ids: self.generated.clone(),
            }))
        }
        fn row_pin(&self, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
            crate::palw_step_refute::tiled_decode_pin_v1(&self.binding.job_context, &self.rows, &self.generated, row, lane)
        }
        fn step_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
            palw_tir_step_node_parts_v1(&self.hashes, level, index)
        }
        fn row_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
            palw_tir_row_node_parts_v1(&self.binding.job_context, &self.rows, level, index)
        }
    }

    struct Src<'a>(&'a MapParams);
    impl PalwTirTensorSourceV1 for Src<'_> {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
        }
    }

    fn program_and_params() -> (TirProgramV1, MapParams) {
        program_and_params_with_vocab(8)
    }

    /// [`program_and_params`] over a `vocab`-word vocabulary (the logits node's leaf holds `min(vocab, 4096)` lanes).
    fn program_and_params_with_vocab(vocab: usize) -> (TirProgramV1, MapParams) {
        let mut pb = ProgramBuilder::new(vocab as u32, HISTORY_BOUND_V1_SMALL);
        let embed = pb.param("embed", DType::I8, &[vocab as u32, 4], false);
        let w = pb.param("w", DType::I8, &[4, 4], true);
        let head = pb.param("head", DType::I8, &[vocab as u32, 4], false);
        let sum = pb.fixed_state("sum", DType::I32, &[4], -1000, 1000, true);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embed, Ref::Input(0), 0, 0);
            let x = b.cast(x, DType::I32);
            b.finish(&[x])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let s = b.add(Ref::State(sum), Ref::CarryIn(0), DType::I64);
            let s = b.state_write(sum, s);
            let s = b.reshape_fixed(s, &[4, 1]);
            let y = b.matmul(w, s, DType::I32);
            let y = b.reshape_fixed(y, &[4]);
            b.finish(&[y])
        };
        let (post, logits) = {
            let mut b = pb.block("post", carry);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let l = b.reshape_fixed(l, &[vocab as u32]);
            let l = b.commit(l);
            let Ref::Node(i) = l else { unreachable!() };
            (b.finish(&[]), i)
        };
        let mut program = pb.finish(pre, vec![layer], post, logits);
        program.logits_scheme_id.copy_from_slice(crate::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        let mut params = MapParams::default();
        let fill = |n: usize, k: i128| (0..n as i128).map(|i| ((i * 37 + k) % 255) - 127).collect::<Vec<_>>();
        params.tensors.insert((0, None), Tensor::new(DType::I8, vec![vocab, 4], fill(vocab * 4, 5)).unwrap());
        params.tensors.insert((1, Some(0)), Tensor::new(DType::I8, vec![4, 4], fill(16, 11)).unwrap());
        params.tensors.insert((2, None), Tensor::new(DType::I8, vec![vocab, 4], fill(vocab * 4, 17)).unwrap());
        (program, params)
    }

    /// The class, the job and an execution — honest, or with lane `lane` of leaf `forge` moved by
    /// one (inside its proven interval).
    pub(crate) fn tiny_execution(forge: Option<(usize, usize)>) -> TinyExecution {
        tiny_execution_in(
            |class, class_id, prompt| {
                let z = Hash64::from_bytes([0u8; 64]);
                PalwJobContextV2 {
                    version: 2,
                    network_id: b"testnet-12".to_vec(),
                    job_id: Hash64::from_bytes([5; 64]),
                    job_nullifier: z,
                    assignment_id: z,
                    execution_seed: [0u8; 32],
                    model_profile_id: z,
                    runtime_manifest_hash: z,
                    runtime_class_id: z,
                    shape_profile_id: class_id,
                    trace_scheme_id: tiled_logits_scheme_id_v1(),
                    cu_ruleset_id: z,
                    tokenizer_id: class.tokenizer_id,
                    prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(prompt),
                    declared_prefill_tokens: PREFILL,
                    exact_decode_tokens: DECODE,
                    max_context_tokens: 64,
                }
            },
            forge,
        )
    }

    /// [`tiny_execution`] in the job context `ctx_of(class, class_id, prompt)` builds — a
    /// `(PREFILL, DECODE)` job over the tiny class, whose layout allows exactly the
    /// `PREFILL + DECODE − 1` positions the job touches.
    pub(crate) fn tiny_execution_in(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
    ) -> TinyExecution {
        tiny_execution_selecting_in(ctx_of, forge, &|_, row, _| base0_decode_token_select_v1(row) as u32)
    }

    /// **G14 G-T2 test support: the honest choice of `rule` over `row` after `before`** (argmax of the rule's lane key, ties to
    /// the lower index, banned lanes never chosen) — or, with `wrong`, the best lane the rule does NOT choose: a committed token
    /// the selection door must refute, over arithmetic that is otherwise honest.
    /// [`tiny_execution_in`] whose decode token at each position is `select(position, row, before)` — the honest rule of a
    /// sampled or V4 job, or a selection violated at one position (the arithmetic of every leaf stays honest: the next position
    /// consumes the committed token).
    pub(crate) fn tiny_execution_selecting_in(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
        select: &dyn Fn(u32, &[i32], &[u32]) -> u32,
    ) -> TinyExecution {
        tiny_execution_selecting_in_vocab(ctx_of, forge, select, 8)
    }

    /// **The tiny class over a 4,096-word vocabulary** — the same program, its logits leaf a full 4,096-lane (16 KiB) tile: a
    /// genuinely computed wide leaf for the acquisition's multi-part fixtures (G14 audit §11 (a) bounds every answer by the
    /// job's ACTUAL leaves, so a part-spanning leaf has to be a real one). Returns the execution and that leaf's index.
    /// Test-only wider class through the same execution/commitment builder.
    pub(crate) fn tiny_execution_selecting_wide_in(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
        select: &dyn Fn(u32, &[i32], &[u32]) -> u32,
    ) -> TinyExecution {
        tiny_execution_selecting_in_vocab(ctx_of, forge, select, 4096)
    }
    pub(crate) fn tiny_execution_wide() -> (TinyExecution, u64) {
        let x = tiny_execution_selecting_in_vocab(
            |class, class_id, prompt| {
                let mut ctx = tiny_execution(None).binding.job_context;
                ctx.shape_profile_id = class_id;
                ctx.tokenizer_id = class.tokenizer_id;
                ctx.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(prompt);
                ctx
            },
            None,
            &|_, row, _| base0_decode_token_select_v1(row) as u32,
            4096,
        );
        let wide = (0..x.preimages.len()).max_by_key(|i| (x.preimages[*i].values_le.len(), usize::MAX - *i)).unwrap() as u64;
        assert_eq!(x.preimages[wide as usize].values_le.len(), 4 * 4096, "the logits leaf is a full tile");
        (x, wide)
    }

    pub(crate) fn tiny_execution_selecting_in_vocab(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
        select: &dyn Fn(u32, &[i32], &[u32]) -> u32,
        vocab: usize,
    ) -> TinyExecution {
        tiny_execution_selecting_in_vocab_with_tokenizer(ctx_of, forge, select, vocab, Hash64::from_bytes([3; 64]))
    }
    /// Test-only real program/class whose tokenizer is bound to actual raw material.
    pub(crate) fn tiny_execution_selecting_in_vocab_with_tokenizer(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
        select: &dyn Fn(u32, &[i32], &[u32]) -> u32,
        vocab: usize,
        tokenizer_id: Hash64,
    ) -> TinyExecution {
        let (program, params) = program_and_params_with_vocab(vocab);
        let bytes = program.encode();
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: bytes,
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: PREFILL + DECODE - 1,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: vec![4, 4, 4096],
                state_tiles: vec![4],
            },
            tokenizer_id,
        };
        let ops = palw_tir_inventory_operands_v1(&program, &Src(&params)).expect("inventory");
        let artifact_root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("root");
        let class_id = class.class_id(&artifact_root);
        let prompt = vec![1u32, 6, 3];
        let interp = Interpreter::new(&program).expect("valid");
        let mut state = RunState::default();
        let positions = PREFILL + DECODE - 1;
        let (mut commits, mut after, mut rows, mut generated) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for a in 0..positions {
            let token = if a < PREFILL { prompt[a as usize] } else { generated[(a - PREFILL) as usize] };
            let step = interp.step(&params, &mut state, token).expect("an honest step");
            commits.push(step.commits);
            after.push(state.clone());
            if a + 1 >= PREFILL {
                let row: Vec<i32> = step.logits.data.iter().map(|v| *v as i32).collect();
                let token = select(generated.len() as u32, &row, &generated);
                generated.push(token);
                rows.push(row);
            }
        }
        let ctx = ctx_of(&class, class_id, &prompt);
        assert_eq!((ctx.declared_prefill_tokens, ctx.exact_decode_tokens), (PREFILL, DECODE), "the tiny job");
        let space = PalwTirStepSpaceV1::new(&class).expect("the layout fits");
        let occ = space.occurrences().to_vec();
        let mut preimages = Vec::new();
        for a in 0..positions {
            for leaf in space.leaves_of_position(&ctx, a) {
                let n = leaf.value_count as usize;
                let mut values: Vec<i128> = match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                        let (block, layer) = occ[occurrence as usize];
                        let c = commits[a as usize].iter().find(|c| c.block == block && c.layer == layer && c.node == node).unwrap();
                        c.value.data[first_element as usize..first_element as usize + n].to_vec()
                    }
                    PalwTirLeafKindV1::State { state: j, layer, first_element, .. } => {
                        let t = after[a as usize].fixed.get(&(j, layer)).expect("written every position");
                        t.data[first_element as usize..first_element as usize + n].to_vec()
                    }
                    PalwTirLeafKindV1::HistTile { .. } => unreachable!("no history"),
                };
                if let Some((i, lane)) = forge
                    && i == preimages.len()
                {
                    values[lane] += if values[lane] < 0 { 1 } else { -1 };
                }
                preimages.push(palw_tir_leaf_preimage_v1(&leaf, &values).expect("a lane of its dtype"));
            }
        }
        let ctx_hash = ctx.context_hash();
        let hashes: Vec<Hash64> = preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &class_id, p)).collect();
        let root = step_merkle_root_v1(&hashes).expect("root");
        let trace = crate::palw_step_refute::tiled_logits_trace_root_v1(&ctx, &rows, &generated).expect("trace");
        let count = hashes.len() as u64;
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            class,
            artifact_root,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: root,
            committed_execution_root: palw_tir_execution_root_v1(&ctx_hash, &trace, &class_id, count, &root),
        };
        TinyExecution { binding, preimages, hashes, ops, prompt, rows, generated, param_cache: Default::default() }
    }

    /// A binding whose parts verify (the leaf count is the job's and the root recomputes), over a
    /// class with a canonical job: the identity rule needs no execution, only a binding.
    /// A binding whose parts verify (the leaf count is the job's and the root recomputes) over the
    /// tiny class widened to 64 positions (so it has a canonical job), at the anchor's J5 context with
    /// `edit` applied: the identity rule needs no execution, only a binding.
    pub(crate) fn attempt_binding(
        anchor: Hash64,
        edit: impl FnOnce(&mut PalwJobContextV2),
    ) -> (PalwTirStepBindingV1, crate::palw_tir_attempt_v1::PalwTirJobFactsV1) {
        use crate::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_context_v1, palw_tir_attempt_prompt_root_v1};
        const MAX: u64 = 1 << 26;
        let x = tiny_execution(None);
        let mut class = x.binding.class.clone();
        class.layout.max_context = 64;
        let class_id = class.class_id(&x.binding.artifact_root);
        let facts = PalwTirJobFactsV1::of_class(&class, class_id).unwrap();
        let canonical = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1(&class).unwrap();
        let root = palw_tir_attempt_prompt_root_v1(&facts, &anchor, canonical.0, PalwPromptIdsFormV1::Flat).unwrap();
        let mut ctx = palw_tir_attempt_context_v1(&facts, &anchor, canonical, root);
        edit(&mut ctx);
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let count = space.leaf_count_capped(&ctx, MAX).unwrap();
        let (trace, step_root) = (Hash64::from_bytes([0x71; 64]), Hash64::from_bytes([0x72; 64]));
        let execution = palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &class_id, count, &step_root);
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            class,
            artifact_root: x.binding.artifact_root,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: step_root,
            committed_execution_root: execution,
        };
        (binding, facts)
    }

    #[test]
    fn the_tiny_execution_is_adjudicable_both_ways() {
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: DemandLimits::UNLIMITED,
        };
        let honest = tiny_execution(None);
        for leaf in 0..honest.preimages.len() as u64 {
            let r = build_tir_cone_refutation_v1(&honest.binding, leaf, &honest, &rules).expect("buildable");
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "leaf {leaf}");
        }
        let forged = tiny_execution(Some((7, 1)));
        let r = build_tir_cone_refutation_v1(&forged.binding, 7, &forged, &rules).expect("buildable");
        assert_eq!(
            check_tir_cone_refutation_v1(&r, &rules).expect("convicted").fault,
            PalwStepFaultV1::ComputationMismatch { value_index: 1 }
        );
    }

    /// **The multiproof vectors on the tiny program** (design §2.12.1): its closes carry a single
    /// inventory leaf (a `Fixed`-state checkpoint whose update reads the gathered embedding row), a run
    /// (a matmul's whole weight) and none; every one is acquitted, is the builder's multiproof of the
    /// leaves read, and is priced to the byte by `palw_tir_param_carriage_bytes_v1`.
    #[test]
    fn the_tiny_program_s_closes_carry_a_single_leaf_a_run_and_none() {
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: DemandLimits::UNLIMITED,
        };
        let x = tiny_execution(None);
        let program = x.binding.class.decode_program().expect("decodes");
        let hashes: Vec<Hash64> = x.ops.iter().map(crate::palw_artifact::artifact_leaf_v1).collect();
        let (mut singles, mut runs, mut none) = (0, 0, 0);
        for leaf in 0..x.preimages.len() as u64 {
            let r = build_tir_cone_refutation_v1(&x.binding, leaf, &x, &rules).expect("buildable");
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "leaf {leaf}");
            let leaves: Vec<u32> = r.params.single().iter().flat_map(|p| p.opened.iter().map(|(l, _)| *l)).collect();
            if !leaves.is_empty() {
                let opened: Vec<_> = leaves.iter().map(|l| (*l, x.ops[*l as usize].clone())).collect();
                assert_eq!(
                    r.params.single(),
                    crate::palw_artifact::palw_artifact_multiproof_v1(&hashes, &opened).as_ref(),
                    "leaf {leaf}"
                );
            }
            let carried = borsh::to_vec(&r.params).unwrap().len() as u64;
            assert_eq!(palw_tir_param_carriage_bytes_v1(&program, PalwTirParamCarriageV1::Multiproof, &leaves), Some(carried));
            match leaves.len() {
                0 => none += 1,
                1 => singles += 1,
                _ if leaves.windows(2).all(|w| w[1] == w[0] + 1) => runs += 1,
                _ => {}
            }
        }
        assert!(singles > 0 && runs > 0 && none > 0, "{singles} single-leaf closes, {runs} runs, {none} with none");
    }

}

}
#[path = "/Users/wata/.codex/worktrees/a4-g14-court/MISAKA-testnet/target/a4-direct/positive/mod.rs"]
pub mod palw_positive_court_v2;
