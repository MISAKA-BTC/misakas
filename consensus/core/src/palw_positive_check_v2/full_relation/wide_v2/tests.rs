//! Real reference execution + inventory/step openings; no success source stub or native receipt.
use super::*;
use crate::{
    palw_artifact::{
        PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1, palw_artifact_multiproof_from_openings_v1,
    },
    palw_step_leg::{step_merkle_range_siblings_v1, step_opening_v1},
    palw_step_refute::PalwStepInputRowV1,
    palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1},
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_tir_court_v1::{PalwTirParamOpeningV1, source_only::wide_v2::WideGeneratedIdsPinV2},
    palw_tir_step_v1::{
        PalwTirLeafV1,
        wide_v2::{PalwTirStepLegBuilderV2, PalwTirStepSpaceV2, palw_tir_leaf_preimage_v2},
    },
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, RESULT_SIGN_CONTEXT},
};
use misaka_palw_tir::{Interpreter, MapParams, Ref, RunState, Tensor, TensorType, builder::ProgramBuilder};
use std::borrow::Cow;
struct Src<'a>(&'a MapParams);
impl PalwTirTensorSourceV1 for Src<'_> {
    fn tensor_bytes(&self, p: u16, l: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(p, l)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}
struct Fixture {
    binding: PalwTirStepBindingV1,
    preimages: Vec<PalwStepTileLeafV1>,
    hashes: Vec<Hash64>,
    operands: Vec<PalwArtifactOperandV1>,
    tokens: Vec<u32>,
    carries: Vec<Vec<i128>>,
    rows: Vec<Vec<i64>>,
    generated: Vec<u32>,
}
fn fixture(range_failed: bool) -> Fixture {
    let input_dtype = if range_failed { DType::I32 } else { DType::I8 };
    let mut pb = ProgramBuilder::new(4, 1 << 18);
    let embed = pb.param("embed", input_dtype, &[4, 4], false);
    let factor = pb.scalar(DType::I64, 1i128 << 40);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let r = b.gather(embed, Ref::Input(0), 0, 0);
        b.finish(&[r])
    };
    let (post, logits) = {
        let mut b = pb.block("post", vec![TensorType::fixed(input_dtype, &[4])]);
        let x = b.cast(Ref::CarryIn(0), DType::I64);
        let y = b.mul(x, factor, DType::I64);
        b.commit(y);
        let Ref::Node(i) = y else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![], post, logits);
    program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    assert_eq!(misaka_palw_tir::interval::analyze_ranges(&program).is_err(), range_failed);
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 2,
            h_tile: 1,
            commit_tiles: vec![4, 4],
            state_tiles: vec![],
        },
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let mut params = MapParams::default();
    params
        .tensors
        .insert((0, None), Tensor::new(input_dtype, vec![4, 4], vec![1, 2, 3, 4, 2, 4, 1, 3, 3, 1, 4, 2, 4, 3, 2, 1]).unwrap());
    let operands = palw_tir_inventory_operands_v1(&program, &Src(&params)).unwrap();
    let artifact = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    let ctx = crate::palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2::new(&class, artifact, 1 << 18)
        .unwrap()
        .job_for_anchor(&Hash64::from_u64_word(31), PalwPromptIdsFormV1::Flat)
        .unwrap();
    let (job, prompt) = ctx;
    assert_eq!((job.declared_prefill_tokens, job.exact_decode_tokens), (1, 2));
    let interp = Interpreter::new(&program).unwrap();
    let mut state = RunState::default();
    let mut generated = vec![];
    let mut rows = vec![];
    let mut carries = vec![];
    let mut tokens = vec![];
    let mut commits = vec![];
    for pos in 0..2 {
        let token = if pos == 0 { prompt[0] } else { generated[pos - 1] };
        tokens.push(token);
        let step = interp.step(&params, &mut state, token).unwrap();
        carries.push(step.commits.iter().find(|c| c.block == pre).unwrap().value.data.clone());
        let row = step.logits.data.iter().map(|v| i64::try_from(*v).unwrap()).collect::<Vec<_>>();
        let selected = row.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).unwrap().0 as u32;
        rows.push(row);
        generated.push(selected);
        commits.push(step.commits);
    }
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class.class_id(&artifact), 100).unwrap();
    let mut preimages = vec![];
    let mut hashes = vec![];
    while let Some(leaf) = builder.next_leaf() {
        let PalwTirLeafKindV1::Commit { block, node, first_element, .. } = leaf.kind else { unreachable!() };
        let values = &commits[leaf.position as usize].iter().find(|c| c.block == block && c.node == node).unwrap().value.data;
        let values = &values[first_element as usize..first_element as usize + leaf.value_count as usize];
        let preimage = palw_tir_leaf_preimage_v2(&space, &job, &leaf, values).unwrap();
        hashes.push(step_tile_leaf_hash_v1(&job.context_hash(), &class.class_id(&artifact), &preimage));
        preimages.push(preimage);
        builder.push(values).unwrap();
    }
    let (count, root) = builder.finish().unwrap();
    let trace = crate::palw_logits_q24_v2::trace_root(&job, &rows, &generated).unwrap();
    let binding = PalwTirStepBindingV1 {
        version: 1,
        committed_execution_root: crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &job.context_hash(),
            &trace,
            &class.class_id(&artifact),
            count,
            &root,
        ),
        job_context: job,
        class,
        artifact_root: artifact,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: root,
    };
    verify_tir_binding_v2(&binding, 100).unwrap();
    Fixture { binding, preimages, hashes, operands, tokens, carries, rows, generated }
}
fn limits() -> FullRelationLimits {
    FullRelationLimits {
        plan: crate::palw_positive_check_v2::expanded_plan::ExpandedPlanLimits {
            max_positions: 16,
            max_geometry_visits: 1 << 20,
            max_plan_bytes: 1 << 20,
            max_legacy_leaves: 100,
            max_relations: 100,
            max_target_lanes: 1000,
            max_peak_target_bytes: 1024,
            uncommitted_tile_lanes: 4,
            max_elements_per_relation: 128,
            max_terms_per_relation: 512,
            max_total_elements: 1 << 24,
            max_total_terms: 1 << 24,
        },
        source: TirSourceOnlyLimitsV1 {
            max_source_reads: 128,
            max_target_lanes: 64,
            max_target_bytes: 1024,
            max_material_bytes: 1 << 20,
            max_material_units: 2048,
            max_auth_storage_bytes: 1 << 20,
            max_workspace_bytes: 1 << 30,
        },
        max_record_bytes: 1 << 20,
        max_record_heap_bytes: 1 << 20,
        max_total_material_bytes: 1 << 40,
        max_total_source_reads: 1 << 30,
        max_record_geometry_visits: 1 << 30,
    }
}
struct Signed {
    rooted: AdmissionContext,
    result: MinerResultCommit,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
impl Signed {
    fn new(f: &Fixture, wrong_miner: bool) -> Self {
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let key = *pair.verification_key.as_ref();
        let job = &f.binding.job_context;
        let class = &f.binding.class;
        let rooted = AdmissionContext {
            scope: ChainScope { network: Digest([1; 64]), ruleset: Digest([2; 64]), branch: Digest([3; 64]) },
            expected_job: digest(job.job_id),
            expected_execution: digest(job.context_hash()),
            expected_miner: principal(&key),
            expected_input: digest(job.prompt_token_ids_hash),
            registered_class: digest(class.class_id(&f.binding.artifact_root)),
            artifact: digest(f.binding.artifact_root),
            program: digest(class.graph_ir_root()),
            tokenizer: digest(job.tokenizer_id),
            execution_rules: digest(job.cu_ruleset_id),
            selector: greedy_selector_id(),
            required_form: ClaimForm::PositiveResultV2,
        };
        let mut ids = f.generated.clone();
        if wrong_miner {
            ids[0] = (ids[0] + 1) % 4;
        }
        let result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: rooted.scope,
            job: rooted.expected_job,
            execution: rooted.expected_execution,
            miner: rooted.expected_miner,
            registered_class: rooted.registered_class,
            artifact: rooted.artifact,
            program: rooted.program,
            input: rooted.expected_input,
            tokenizer: rooted.tokenizer,
            execution_rules: rooted.execution_rules,
            selector: rooted.selector,
            generation_seed: job.execution_seed,
            temperature_q24: 0,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output: digest(palw_attempt_output_root_v1(job, &ids)),
            dependencies: Digest([4; 64]),
            service_contract: Digest([5; 64]),
        };
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        Self { rooted, result, key, signature }
    }
    fn prep(&self, f: &Fixture, l: FullRelationLimits) -> FullRelationPreparationV2 {
        FullRelationPreparationV2::authenticate(
            &self.rooted,
            &f.binding.class,
            f.binding.artifact_root,
            &f.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            l,
            PrototypeDomains::local_only(b"fixture-wide-exact", b"fixture-wide-exact-sign").unwrap(),
            self.result.clone(),
            &self.key,
            &self.signature,
        )
        .unwrap()
    }
}
fn claims(f: &Fixture, plan: &ExpandedPlanV2) -> Vec<Vec<i128>> {
    plan.iter()
        .map(|r| match r.kind {
            ExpandedRelationKind::RecomputeNode { ctx, first_lane, lane_count, .. } => {
                f.carries[ctx.pos as usize][first_lane as usize..first_lane as usize + lane_count as usize].to_vec()
            }
            ExpandedRelationKind::Legacy(leaf) => {
                let PalwTirLeafKindV1::Commit { block, .. } = leaf.kind else { unreachable!() };
                if block == f.binding.class.decode_program().unwrap().schedule.pre {
                    f.carries[leaf.position as usize].clone()
                } else {
                    f.rows[leaf.position as usize].iter().map(|v| i128::from(*v)).collect()
                }
            }
        })
        .collect()
}
fn checker(f: &Fixture, s: &Signed, alter: bool, l: FullRelationLimits) -> FullRelationCheckerV2 {
    let prep = s.prep(f, l);
    let mut values = claims(f, prep.plan());
    if alter {
        let i = prep.plan().iter().find(|r| matches!(r.kind, ExpandedRelationKind::RecomputeNode { .. })).unwrap().index;
        values[i as usize][0] += 1;
    }
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    let record =
        prep.freeze_record(f.binding.clone(), f.generated.clone(), principal(panel.verification_key.as_ref()), values).unwrap();
    let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
        .unwrap()
        .as_ref();
    prep.begin(record, panel.verification_key.as_ref(), &sig).unwrap()
}
struct Material {
    operands: PalwStepInputRowV1,
    params: PalwTirParamOpeningV1,
    prompt: Vec<u32>,
    pin: Option<WideGeneratedIdsPinV2>,
}
impl Material {
    fn view(&self) -> TirSourceOnlyMaterialV2<'_> {
        TirSourceOnlyMaterialV2 {
            operands: &self.operands,
            params: &self.params,
            prompt_token_ids: &self.prompt,
            prompt_ids_openings: &[],
            decode_tokens: self.pin.as_ref(),
        }
    }
}
fn material(f: &Fixture, r: &crate::palw_positive_check_v2::expanded_plan::ExpandedRelation) -> Material {
    let position = match &r.kind {
        ExpandedRelationKind::Legacy(l) => l.position,
        ExpandedRelationKind::RecomputeNode { ctx, .. } => ctx.pos,
    };
    let pre = matches!(r.kind, ExpandedRelationKind::Legacy(PalwTirLeafV1 { kind: PalwTirLeafKindV1::Commit { block: 0, .. }, .. }));
    if pre {
        let opening = open_artifact_leaf_v1(&f.operands, f.tokens[position as usize]).unwrap();
        let params = PalwTirParamOpeningV1::Single(palw_artifact_multiproof_from_openings_v1(&[opening]).unwrap());
        let (prompt, pin) = if position == 0 {
            (vec![f.tokens[0]], None)
        } else {
            (
                vec![],
                Some(WideGeneratedIdsPinV2 {
                    rows_root: crate::palw_logits_q24_v2::rows_root(&f.binding.job_context, &f.rows).unwrap(),
                    generated_token_ids: f.generated.clone(),
                }),
            )
        };
        Material { operands: PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] }, params, prompt, pin }
    } else {
        let index = position as usize * 2;
        Material {
            operands: PalwStepInputRowV1 {
                preimages: vec![f.preimages[index].clone()],
                run_siblings: vec![step_merkle_range_siblings_v1(&f.hashes, index, 1).unwrap()],
            },
            params: PalwTirParamOpeningV1::None,
            prompt: vec![],
            pin: None,
        }
    }
}
struct Target {
    opening: PalwStepOpeningV1,
    preimage: PalwStepTileLeafV1,
    trace: Option<WideLogitsEventV2>,
}
impl Target {
    fn view(&self) -> StepTargetV2<'_> {
        StepTargetV2 { opening: &self.opening, preimage: &self.preimage, trace: self.trace.as_ref() }
    }
}
fn target(f: &Fixture, r: &crate::palw_positive_check_v2::expanded_plan::ExpandedRelation) -> Option<Target> {
    let ExpandedRelationKind::Legacy(leaf) = r.kind else {
        return None;
    };
    let trace = if leaf.dtype == DType::I64 {
        let row = leaf.position;
        let context = f.binding.job_context.context_hash();
        let roots = f
            .rows
            .iter()
            .enumerate()
            .map(|(i, l)| crate::palw_logits_q24_v2::row_root(&context, i as u32, l).unwrap())
            .collect::<Vec<_>>();
        let tile = crate::palw_logits_q24_v2::tile_leaf(&context, row, 0, &f.rows[row as usize]);
        Some(WideLogitsEventV2 {
            row,
            generated: f.generated.clone(),
            row_root: roots[row as usize],
            row_opening: step_opening_v1(&roots, u64::from(row)).unwrap(),
            lanes: f.rows[row as usize].clone(),
            tile_opening: step_opening_v1(&[tile], 0).unwrap(),
        })
    } else {
        None
    };
    Some(Target {
        opening: step_opening_v1(&f.hashes, leaf.index).unwrap(),
        preimage: f.preimages[leaf.index as usize].clone(),
        trace,
    })
}
fn submit_next(f: &Fixture, c: &mut FullRelationCheckerV2) -> Result<(), FullRelationError> {
    let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
    let m = material(f, &r);
    let t = target(f, &r);
    c.submit(&m.view(), t.as_ref().map(Target::view))
}
#[test]
fn actual_wide_source_relations_and_greedy_rows_complete_without_narrowing() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let original = s.result.id();
    let mut c = checker(&f, &s, false, limits());
    assert!(f.rows.iter().flatten().all(|v| *v > i64::from(i32::MAX)));
    assert!(
        crate::palw_positive_check_v2::full_relation::FullRelationPreparation::authenticate(
            &s.rooted,
            &f.binding.class,
            f.binding.artifact_root,
            &f.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            limits(),
            PrototypeDomains::local_only(b"fixture-wide-exact", b"fixture-wide-exact-sign").unwrap(),
            s.result.clone(),
            &s.key,
            &s.signature
        )
        .is_err()
    );
    while c.progress().next_relation < c.preparation().plan().summary().relations {
        submit_next(&f, &mut c).unwrap();
    }
    assert_eq!(c.progress().checked_rows, 2);
    assert_eq!(c.finish().unwrap(), CheckOutcome::Matched);
    assert_eq!(s.result.id(), original);
    s.result.authenticate(&s.key, &s.signature).unwrap();
}
#[test]
fn signed_wrong_panel_intermediate_rejects_only_witness_not_miner_output() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let mut c = checker(&f, &s, true, limits());
    submit_next(&f, &mut c).unwrap();
    assert_eq!(submit_next(&f, &mut c), Err(FullRelationError::PanelMismatch));
    assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
    assert_eq!(c.finish(), Err(FullRelationError::Closed));
    s.result.authenticate(&s.key, &s.signature).unwrap();
}
#[test]
fn wrong_signed_miner_output_is_compared_only_after_all_wide_relations() {
    let f = fixture(false);
    let s = Signed::new(&f, true);
    let mut c = checker(&f, &s, false, limits());
    let total = c.preparation().plan().summary().relations;
    assert_eq!(c.finish(), Err(FullRelationError::Incomplete));
    while c.progress().next_relation < total {
        submit_next(&f, &mut c).unwrap();
        assert_eq!(c.progress().outcome, CheckOutcome::Checking);
    }
    assert_eq!(c.finish().unwrap(), CheckOutcome::OutputMismatch);
}
#[test]
fn corrupt_transport_or_wide_generated_pin_cannot_advance_coverage() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let mut c = checker(&f, &s, false, limits());
    submit_next(&f, &mut c).unwrap();
    let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
    let mut m = material(&f, &r);
    m.operands.preimages[0].values_le[0] ^= 1;
    let before = c.progress();
    assert_eq!(c.submit(&m.view(), None), Err(FullRelationError::Delivery));
    assert_eq!(c.progress().next_relation, before.next_relation);
    assert_eq!(c.progress().outcome, CheckOutcome::Checking);
    assert!(c.progress().charged_material_bytes > before.charged_material_bytes);
    submit_next(&f, &mut c).unwrap();
    submit_next(&f, &mut c).unwrap();
    let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
    let mut m = material(&f, &r);
    m.pin.as_mut().unwrap().generated_token_ids[0] ^= 1;
    let t = target(&f, &r);
    let before = c.progress();
    assert_eq!(c.submit(&m.view(), t.as_ref().map(Target::view)), Err(FullRelationError::Delivery));
    assert_eq!(c.progress().next_relation, before.next_relation);
    assert_eq!(c.progress().outcome, CheckOutcome::Checking);
}
#[test]
fn insufficient_workspace_or_static_range_profile_never_becomes_miner_fault() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let mut l = limits();
    l.source.max_workspace_bytes = 1;
    let mut c = checker(&f, &s, false, l);
    assert_eq!(submit_next(&f, &mut c), Err(FullRelationError::Budget));
    assert_eq!(c.progress().next_relation, 0);
    assert_eq!(c.progress().outcome, CheckOutcome::EndedUnverified);
    let f = fixture(true);
    let s = Signed::new(&f, false);
    let mut c = checker(&f, &s, false, limits());
    assert_eq!(submit_next(&f, &mut c), Err(FullRelationError::Delivery));
    assert_eq!(c.progress().next_relation, 0);
    assert_eq!(c.progress().outcome, CheckOutcome::Checking);
    assert_eq!(c.finish(), Err(FullRelationError::Incomplete));
}

#[test]
fn coherently_committed_wrong_panel_greedy_id_never_refutes_the_signed_miner() {
    let mut f = fixture(false);
    let s = Signed::new(&f, false);
    let miner_result = s.result.id();
    f.generated[0] = (f.generated[0] + 1) % 4;
    let job = &f.binding.job_context;
    f.binding.full_logits_trace_root = crate::palw_logits_q24_v2::trace_root(job, &f.rows, &f.generated).unwrap();
    f.binding.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
        &job.context_hash(),
        &f.binding.full_logits_trace_root,
        &f.binding.class.class_id(&f.binding.artifact_root),
        f.binding.step_leaf_count,
        &f.binding.step_merkle_root,
    );
    let mut c = checker(&f, &s, false, limits());
    submit_next(&f, &mut c).unwrap();
    submit_next(&f, &mut c).unwrap();
    assert_eq!(submit_next(&f, &mut c), Err(FullRelationError::PanelMismatch));
    assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
    assert_eq!(c.progress().checked_rows, 0);
    assert_eq!(s.result.id(), miner_result);
    s.result.authenticate(&s.key, &s.signature).unwrap();
}
#[test]
fn wide_ids_pin_byte_unit_and_storage_bounds_include_the_new_root_and_ids() {
    let f = fixture(false);
    let s = Signed::new(&f, false);
    let prep = s.prep(&f, limits());
    let relation = prep
        .plan()
        .iter()
        .find(|r| {
            matches!(
                r.kind,
                ExpandedRelationKind::Legacy(PalwTirLeafV1 { position: 1, kind: PalwTirLeafKindV1::Commit { block: 0, .. }, .. })
            )
        })
        .unwrap();
    let m = material(&f, &relation);
    let q = m.view().preflight(limits().source).unwrap();
    let old = crate::palw_tir_court_v1::source_only::TirSourceOnlyMaterialV1 {
        operands: &m.operands,
        params: &m.params,
        prompt_token_ids: &m.prompt,
        prompt_ids_openings: &[],
        decode_tokens: None,
    }
    .preflight(limits().source)
    .unwrap();
    assert_eq!(q.material_bytes - old.material_bytes, 68 + 4 * f.generated.len() as u64);
    assert_eq!(q.material_units - old.material_units, f.generated.len() as u64);
    assert_eq!(q.logical_auth_storage_bytes - old.logical_auth_storage_bytes, 4 * f.generated.len() as u64);
    let mut cap = limits().source;
    cap.max_material_bytes = q.material_bytes - 1;
    assert!(matches!(m.view().preflight(cap), Err(TirSourceOnlyErrorV1::Budget)));
    let mut m = material(&f, &relation);
    m.pin.as_mut().unwrap().rows_root = Hash64::from_u64_word(999);
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: 100,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: 128, max_terms: 512 },
    };
    assert!(matches!(
        authenticate_source_only_v2(&f.binding, m.view(), relation.legacy_frontier, &rules, limits().source),
        Err(TirSourceOnlyErrorV1::Material(_))
    ));
}

mod mixed_state;
