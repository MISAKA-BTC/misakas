//! Source-only full-relation prototype over unchanged original sparse TIR.
//! Caller-local record domains are for tests; no wire ID or activation is assigned.
#![cfg(feature = "positive-contract")]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_check_v2::{
        CheckOutcome,
        expanded_plan::{ExpandedPlanLimits, ExpandedRelation, ExpandedRelationKind},
        full_relation::{
            FullRelationChecker, FullRelationError, FullRelationLimits, FullRelationPreparation, LegacyTarget, PrototypeDomains,
            PrototypePanelRecord,
        },
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::PalwStepInputRowV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::{
        PalwTirCourtRulesV1, PalwTirEvidenceStoreV1, PalwTirParamOpeningV1, PalwTirTraceEventDisclosureV1,
        source_only::{OwnedTirSourceOnlyMaterialV1, TirSourceOnlyLimitsV1, build_source_only_legacy_v1, build_source_only_node_v1},
        tir_logits_event_disclosure_v1,
    },
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
};
use misaka_palw_positive_contract::{identity::*, messages::*, tir::greedy_selector_id};
use misaka_palw_tir::{
    ConeEnv, DType, Interpreter, MapParams, Ref, RunState, Tensor, TensorType, TirProgramV1, builder::ProgramBuilder,
    demand::DemandLimits, prim::Rounding, program::StateKind,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1},
};
use std::collections::BTreeMap;

fn limits() -> FullRelationLimits {
    FullRelationLimits {
        plan: ExpandedPlanLimits {
            max_positions: 32,
            max_plan_bytes: 1 << 24,
            max_geometry_visits: 1 << 24,
            max_legacy_leaves: 1 << 20,
            max_relations: 1 << 20,
            max_target_lanes: 1 << 30,
            max_peak_target_bytes: 1 << 24,
            uncommitted_tile_lanes: 3,
            max_elements_per_relation: 1 << 20,
            max_terms_per_relation: 1 << 24,
            max_total_elements: 1 << 60,
            max_total_terms: 1 << 60,
        },
        source: TirSourceOnlyLimitsV1 {
            max_auth_storage_bytes: 1 << 50,
            max_workspace_bytes: 1 << 60,
            max_source_reads: 1 << 20,
            max_target_lanes: 1 << 16,
            max_target_bytes: 1 << 24,
            max_material_bytes: 1 << 24,
            max_material_units: 1 << 20,
        },
        max_record_bytes: 1 << 30,
        max_record_heap_bytes: 1 << 30,
        max_record_geometry_visits: 1 << 60,
        max_total_material_bytes: 1 << 60,
        max_total_source_reads: 1 << 60,
    }
}
fn d(x: u8) -> Digest {
    Digest([x; 64])
}
fn native(h: Hash64) -> Digest {
    Digest(h.as_bytes())
}
fn miner_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]).verification_key.as_ref()
}
fn panel_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]).verification_key.as_ref()
}
fn domains() -> PrototypeDomains {
    PrototypeDomains::local_only(b"A1/full-relation/local-test-record", b"A1/FULL/LOCAL/TEST").unwrap()
}
struct OwnedLegacy {
    opening: PalwStepOpeningV1,
    preimage: PalwStepTileLeafV1,
    trace: Option<PalwTirTraceEventDisclosureV1>,
}
impl OwnedLegacy {
    fn borrowed(&self) -> LegacyTarget<'_> {
        LegacyTarget { opening: &self.opening, preimage: &self.preimage, trace: self.trace.as_ref() }
    }
}
struct Part {
    relation: ExpandedRelation,
    material: OwnedTirSourceOnlyMaterialV1,
    legacy: Option<OwnedLegacy>,
}
struct Fixture {
    class: PalwTirClassV1,
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    claims: Vec<Vec<i128>>,
    parts: Vec<Part>,
    genuine_first_step: PalwStepInputRowV1,
}
impl Fixture {
    fn result(&self) -> MinerResultCommit {
        let job = &self.binding.job_context;
        MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: native(job.job_id),
            execution: native(job.context_hash()),
            miner: principal(&miner_key()),
            registered_class: native(self.class.class_id(&self.binding.artifact_root)),
            artifact: native(self.binding.artifact_root),
            program: native(self.class.graph_ir_root()),
            input: native(job.prompt_token_ids_hash),
            tokenizer: native(job.tokenizer_id),
            execution_rules: native(job.cu_ruleset_id),
            selector: greedy_selector_id(),
            generation_seed: job.execution_seed,
            temperature_q24: 0,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output: native(palw_attempt_output_root_v1(job, &self.generated)),
            dependencies: d(21),
            service_contract: d(22),
        }
    }
    fn prepare(&self, cap: FullRelationLimits) -> FullRelationPreparation {
        self.prepare_result(self.result(), cap, domains())
    }
    fn prepare_result(
        &self,
        result: MinerResultCommit,
        cap: FullRelationLimits,
        local_domains: PrototypeDomains,
    ) -> FullRelationPreparation {
        let rooted = AdmissionContext {
            scope: result.scope,
            expected_job: result.job,
            expected_execution: result.execution,
            expected_miner: result.miner,
            expected_input: result.input,
            registered_class: result.registered_class,
            artifact: result.artifact,
            program: result.program,
            tokenizer: result.tokenizer,
            execution_rules: result.execution_rules,
            selector: result.selector,
            required_form: ClaimForm::PositiveResultV2,
        };
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let signature: [u8; SIGNATURE_BYTES] =
            *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        FullRelationPreparation::authenticate(
            &rooted,
            &self.class,
            self.binding.artifact_root,
            &self.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            cap,
            local_domains,
            result,
            &miner_key(),
            &signature,
        )
        .unwrap()
    }
    fn checker(&self, claims: Vec<Vec<i128>>, cap: FullRelationLimits) -> FullRelationChecker {
        let prepared = self.prepare(cap);
        let record = prepared.freeze_record(self.binding.clone(), self.generated.clone(), principal(&panel_key()), claims).unwrap();
        let signature = sign_record(&record, record.signature_context());
        prepared.begin(record, &panel_key(), &signature).unwrap()
    }
}
fn sign_record(record: &PrototypePanelRecord, signature_context: &[u8]) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &record.signing_digest().0, signature_context, [0; 32]).unwrap().as_ref()
}
fn submit(checker: &mut FullRelationChecker, part: &Part) {
    checker
        .submit(&part.material.as_borrowed(), part.legacy.as_ref().map(OwnedLegacy::borrowed))
        .unwrap_or_else(|error| panic!("relation {:?}, progress {:?}: {error:?}", part.relation, checker.progress()));
}
fn finish_all(f: &Fixture, checker: &mut FullRelationChecker, start: usize) {
    for part in &f.parts[start..] {
        submit(checker, part);
    }
    assert_eq!(checker.finish().unwrap(), CheckOutcome::Matched);
    assert_eq!(checker.progress().next_relation, f.parts.len() as u64);
    assert_eq!(checker.progress().legacy_frontier, f.binding.step_leaf_count);
    assert_eq!(checker.progress().checked_rows, f.generated.len() as u32);
}

struct Oracle {
    before: Vec<RunState>,
    after: Vec<RunState>,
    commits: BTreeMap<(u32, u8, Option<u16>, u16), Tensor>,
    tokens: Vec<u32>,
}
impl Oracle {
    fn new(program: &TirProgramV1, map: &MapParams, tokens: Vec<u32>, expected_rows: &[Vec<i32>], prefill: usize) -> Self {
        let interp = Interpreter::new(program).unwrap();
        let mut state = RunState::default();
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut commits = BTreeMap::new();
        for (pos, token) in tokens.iter().enumerate() {
            before.push(state.clone());
            let step = interp.step(map, &mut state, *token).unwrap();
            if pos + 1 >= prefill {
                assert_eq!(step.logits.data, expected_rows[pos + 1 - prefill].iter().map(|v| *v as i128).collect::<Vec<_>>());
            }
            for c in step.commits {
                commits.insert((pos as u32, c.block, c.layer, c.node), c.value);
            }
            after.push(state.clone());
        }
        Self { before, after, commits, tokens }
    }
    fn node(&self, program: &TirProgramV1, map: &MapParams, pos: u32, occurrence: u16, node: u16) -> Tensor {
        let occurrences = program.occurrences();
        let (block, layer) = occurrences[occurrence as usize];
        let supplied = self
            .commits
            .iter()
            .filter(|((p, b, l, n), _)| *p == pos && *b == block && *l == layer && *n != node)
            .map(|((_, _, _, n), v)| (*n, v.clone()))
            .collect();
        let mut carry_in = BTreeMap::new();
        if occurrence != 0 {
            let (previous_block, previous_layer) = occurrences[occurrence as usize - 1];
            for (k, n) in program.blocks[previous_block as usize].carry_out.iter().enumerate() {
                carry_in.insert(k as u8, self.commits[&(pos, previous_block, previous_layer, *n)].clone());
            }
        }
        let mut fixed = BTreeMap::new();
        let mut hist_prior = BTreeMap::new();
        for (id, state) in program.states.iter().enumerate() {
            if state.per_layer != layer.is_some() {
                continue;
            }
            match state.kind {
                StateKind::Fixed { .. } => {
                    let shape: Vec<_> = state.shape.iter().map(|x| *x as usize).collect();
                    fixed.insert(
                        id as u16,
                        self.before[pos as usize]
                            .fixed
                            .get(&(id as u16, layer))
                            .cloned()
                            .unwrap_or_else(|| Tensor::zeros(state.dtype, &shape)),
                    );
                }
                StateKind::Hist { .. } => {
                    hist_prior.insert(
                        id as u16,
                        self.before[pos as usize]
                            .hist
                            .get(&(id as u16, layer))
                            .map(|rows| rows.iter().cloned().collect())
                            .unwrap_or_default(),
                    );
                }
            }
        }
        Interpreter::new(program)
            .unwrap()
            .eval_cone(
                block,
                layer,
                node,
                map,
                &ConeEnv { token: Some(self.tokens[pos as usize]), pos, carry_in, fixed, hist_prior, supplied },
            )
            .unwrap()
    }
    fn claim(&self, program: &TirProgramV1, map: &MapParams, relation: &ExpandedRelation) -> Vec<i128> {
        match &relation.kind {
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                let value = self.node(program, map, ctx.pos, ctx.occurrence, *node);
                value.data[*first_lane as usize..*first_lane as usize + *lane_count as usize].to_vec()
            }
            ExpandedRelationKind::Legacy(leaf) => match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                    let value = self.node(program, map, leaf.position, occurrence as u16, node);
                    value.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
                }
                PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
                    let value = &self.after[leaf.position as usize].fixed[&(state, layer)];
                    value.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
                }
                PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
                    let rows = &self.after[leaf.position as usize].hist[&(state, layer)];
                    let start = leaf.position as usize + 1 - rows.len();
                    let mut values = Vec::new();
                    for position in first_position..=leaf.position {
                        values.extend_from_slice(
                            &rows[position as usize - start].data[first_lane as usize..first_lane as usize + row_lanes as usize],
                        );
                    }
                    values
                }
            },
        }
    }
}

fn fixture(program: TirProgramV1, map: MapParams, prompt: Vec<u32>, decode: u32) -> Fixture {
    let program = node_common::tiled(program);
    misaka_palw_tir::interval::analyze_ranges(&program).expect("fixture must satisfy original TIR range admission");
    let original = program.encode();
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: original.clone(),
        layout: node_common::layout(&program, 3, 2, 2, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let mut job = node_common::job(&program, prompt.len() as u32, decode, class_id, &prompt, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&job, &prompt, limits().plan.max_legacy_leaves, true, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: job,
        class: class.clone(),
        artifact_root: artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let mut tokens = prompt.clone();
    tokens.extend_from_slice(&run.generated[..run.generated.len() - 1]);
    let oracle = Oracle::new(&program, &map, tokens, &run.logits_rows, prompt.len());
    let cap = limits();
    let expanded = kaspa_consensus_core::palw_positive_check_v2::expanded_plan::ExpandedPlan::derive(
        &class,
        artifact,
        &binding.job_context,
        cap.plan,
    )
    .unwrap();
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: cap.plan.max_legacy_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: cap.plan.max_elements_per_relation, max_terms: cap.plan.max_terms_per_relation },
    };
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &prompt,
        &run.logits_rows,
        &run.generated,
        &leaves,
        &inventory,
        PalwPromptIdsFormV1::Flat,
        cap.plan.max_legacy_leaves,
    )
    .unwrap();
    let genuine_first_step = PalwStepInputRowV1 {
        preimages: vec![store.step_leaf(0).unwrap()],
        run_siblings: vec![store.step_range_siblings(0, 1).unwrap()],
    };
    let mut claims = Vec::new();
    let mut parts = Vec::new();
    for relation in expanded.iter() {
        claims.push(oracle.claim(&program, &map, &relation));
        let (material, legacy) = match &relation.kind {
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => (
                build_source_only_node_v1(
                    &binding,
                    relation.legacy_frontier,
                    *ctx,
                    *node,
                    *first_lane,
                    *lane_count,
                    &store,
                    &rules,
                    cap.source,
                )
                .unwrap(),
                None,
            ),
            ExpandedRelationKind::Legacy(leaf) => {
                let preimage = store.step_leaf(leaf.index).unwrap();
                let decoded: Vec<i128> = preimage
                    .values_le
                    .chunks_exact(4)
                    .map(|b| {
                        if leaf.dtype == DType::Idx {
                            u32::from_le_bytes(b.try_into().unwrap()) as i128
                        } else {
                            i32::from_le_bytes(b.try_into().unwrap()) as i128
                        }
                    })
                    .collect();
                assert_eq!(&decoded, claims.last().unwrap(), "independent whole-node interpreter matches retained legacy target");
                let trace = match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                        if occurrence as usize == space.occurrences().len() - 1 && node == program.logits =>
                    {
                        Some(
                            tir_logits_event_disclosure_v1(
                                &binding,
                                &run.logits_rows,
                                &run.generated,
                                leaf.position + 1 - binding.job_context.declared_prefill_tokens,
                                (first_element / kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u64) as u8,
                            )
                            .unwrap(),
                        )
                    }
                    _ => None,
                };
                (
                    build_source_only_legacy_v1(&binding, leaf, &store, &rules, cap.source).unwrap(),
                    Some(OwnedLegacy { opening: store.step_opening(leaf.index).unwrap(), preimage, trace }),
                )
            }
        };
        parts.push(Part { relation, material, legacy });
    }
    assert_eq!(class.program, original, "full checking must preserve original program bytes and sparse flags");
    Fixture { class, binding, generated: run.generated, claims, parts, genuine_first_step }
}
fn wide_fixture() -> Fixture {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 2], false);
    let scale = p.konst(DType::I64, &[2], &[1 << 32, 1 << 32]);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let row = pre.cast(row, DType::I64);
    let wide = pre.mul(row, scale, DType::I64);
    let square = pre.mul(wide, wide, DType::I128);
    let first = pre.slice(square, 0, 0, 1);
    let first = pre.shr(first, 66, Rounding::Floor, DType::I128);
    let out = pre.clamp(first, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let pre = pre.finish(&[out]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let program = p.finish(pre, vec![], post, logits);
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 2], vec![2, 3, 2, 3, 2, 3, 2, 3]).unwrap());
    fixture(program, map, vec![0, 1], 2)
}
fn rounding_fixture() -> Fixture {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 2], false);
    let two = p.scalar(DType::I64, 2);
    let bias = p.konst(DType::I32, &[2], &[0, 6]);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let wide = pre.cast(row, DType::I64);
    let q = pre.div(wide, two, Rounding::HalfAwayFromZero, DType::I64);
    let out = pre.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let pre = pre.finish(&[out]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    // Carry declarations establish their dtype interval, not the upstream
    // node's narrower range; bind this addition's safe range explicitly.
    let bounded = post.clamp(Ref::CarryIn(0), -64, 64, DType::I32);
    let logits = post.add(bounded, bias, DType::I32);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let program = p.finish(pre, vec![], post, logits);
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 2], vec![5, -5, 5, -5, 5, -5, 5, -5]).unwrap());
    fixture(program, map, vec![0, 1], 2)
}
fn history_fixture() -> Fixture {
    let (_, mut p, map) = node_common::h_program();
    let state = p.states.len() as u16;
    p.states.push(misaka_palw_tir::program::StateDecl {
        name: "accumulator".into(),
        kind: StateKind::Fixed { lo: -128, hi: 127 },
        dtype: DType::I8,
        shape: vec![4],
        per_layer: false,
    });
    let pre = &mut p.blocks[p.schedule.pre as usize];
    let sum = pre.nodes.len() as u16;
    pre.nodes.push(misaka_palw_tir::program::Node {
        prim: misaka_palw_tir::Prim::Add,
        inputs: vec![Ref::Node(0), Ref::State(state)],
        out: TensorType::fixed(DType::I64, &[4]),
        commit: false,
    });
    let clamp = pre.nodes.len() as u16;
    pre.nodes.push(misaka_palw_tir::program::Node {
        prim: misaka_palw_tir::Prim::Clamp { lo: -128, hi: 127 },
        inputs: vec![Ref::Node(sum)],
        out: TensorType::fixed(DType::I8, &[4]),
        commit: false,
    });
    pre.nodes.push(misaka_palw_tir::program::Node {
        prim: misaka_palw_tir::Prim::StateWrite { state },
        inputs: vec![Ref::Node(clamp)],
        out: TensorType::fixed(DType::I8, &[4]),
        commit: false,
    });
    fixture(p, map, vec![3, 8, 13], 6)
}

#[test]
fn original_sparse_wide_program_checks_every_relation_and_only_then_completes() {
    let f = wide_fixture();
    let program = f.class.decode_program().unwrap();
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.out.dtype == DType::I64 && !n.commit));
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.out.dtype == DType::I128 && !n.commit));
    assert_eq!(f.generated, vec![0, 0]);
    let mut checker = f.checker(f.claims.clone(), limits());
    assert_eq!(checker.finish(), Err(FullRelationError::Incomplete));
    for part in &f.parts[..f.parts.len() - 1] {
        submit(&mut checker, part);
    }
    assert_eq!(checker.finish(), Err(FullRelationError::Incomplete));
    submit(&mut checker, f.parts.last().unwrap());
    let completed = checker.progress();
    // An extra unsigned delivery after full coverage cannot poison a candidate
    // which has not yet requested terminal comparison.
    assert_eq!(checker.submit(&f.parts[0].material.as_borrowed(), None), Err(FullRelationError::Incomplete));
    assert_eq!(checker.progress(), completed);
    assert_eq!(checker.finish(), Ok(CheckOutcome::Matched));
    assert_eq!(checker.progress().legacy_frontier, f.binding.step_leaf_count);
    assert_eq!(checker.progress().checked_rows, 2);
}

#[test]
fn a_signed_lie_in_an_output_unused_i128_lane_is_rejected_without_rewriting_legacy_roots() {
    let f = wide_fixture();
    let index = f
        .parts
        .iter()
        .position(|p| {
            matches!(&p.relation.kind,
        ExpandedRelationKind::RecomputeNode { ctx, dtype: DType::I128, shape, first_lane: 0, lane_count: 2, .. }
            if ctx.pos == 0 && shape == &vec![2])
        })
        .unwrap();
    assert_eq!(f.claims[index], vec![1i128 << 66, 9i128 * (1i128 << 64)]);
    let original_binding = f.binding.clone();
    let mut lies = f.claims.clone();
    lies[index][1] += 1; // Downstream slice reads lane0, so greedy output stays0.
    let mut checker = f.checker(lies, limits());
    for part in &f.parts[..index] {
        submit(&mut checker, part);
    }
    assert_eq!(checker.submit(&f.parts[index].material.as_borrowed(), None), Err(FullRelationError::PanelMismatch));
    assert_eq!(checker.progress().next_relation, index as u64);
    assert_eq!(checker.progress().outcome, CheckOutcome::WitnessRejected);
    assert_eq!(checker.finish(), Err(FullRelationError::Closed));
    assert_eq!(f.binding, original_binding);
    assert_eq!(f.generated, vec![0, 0]);
}

#[test]
fn unsigned_missing_tampered_unused_and_future_sources_remain_retryable_deliveries() {
    let f = wide_fixture();
    let first = &f.parts[0];
    assert_eq!(first.relation.legacy_frontier, 0);
    assert!(!first.material.params.is_none());
    let mut cases = Vec::new();
    let mut missing = first.material.clone();
    missing.params = PalwTirParamOpeningV1::None;
    cases.push((0, missing));
    let mut tampered = first.material.clone();
    let PalwTirParamOpeningV1::Single(proof) = &mut tampered.params else { panic!("single inventory fixture") };
    proof.opened[0].1.bytes[0] ^= 1;
    cases.push((0, tampered));
    let mut wrong_prompt = first.material.clone();
    wrong_prompt.prompt_token_ids[0] = 2;
    cases.push((0, wrong_prompt));
    let mut future = first.material.clone();
    future.operands = f.genuine_first_step.clone();
    cases.push((0, future));
    let index = f
        .parts
        .iter()
        .position(|p| {
            matches!(&p.relation.kind,
        ExpandedRelationKind::RecomputeNode { ctx, node: 0, .. } if ctx.pos == 1)
        })
        .unwrap();
    assert!(f.parts[index].relation.legacy_frontier > 0);
    assert!(f.parts[index].material.operands.preimages.is_empty());
    let mut unused_past = f.parts[index].material.clone();
    unused_past.operands = f.genuine_first_step.clone();
    cases.push((index, unused_past));
    let index = f.parts.iter().position(|p| p.legacy.as_ref().is_some_and(|legacy| legacy.trace.is_some())).unwrap();
    assert!(f.parts[index].material.params.is_none());
    let mut unused_param = f.parts[index].material.clone();
    unused_param.params = first.material.params.clone();
    cases.push((index, unused_param));
    for (index, bad) in cases {
        let mut checker = f.checker(f.claims.clone(), limits());
        for part in &f.parts[..index] {
            submit(&mut checker, part);
        }
        let prior = checker.progress();
        assert_eq!(
            checker.submit(&bad.as_borrowed(), f.parts[index].legacy.as_ref().map(OwnedLegacy::borrowed)),
            Err(FullRelationError::Delivery)
        );
        let after = checker.progress();
        assert_eq!(after.next_relation, prior.next_relation);
        assert_eq!(after.legacy_frontier, prior.legacy_frontier);
        assert_eq!(after.checked_rows, prior.checked_rows);
        assert_eq!(after.outcome, CheckOutcome::Checking);
        assert!(after.charged_material_bytes > prior.charged_material_bytes);
        assert!(after.charged_source_reads > prior.charged_source_reads);
        assert!(after.charged_geometry_visits > prior.charged_geometry_visits);
        finish_all(&f, &mut checker, index);
    }
}

#[test]
fn original_fixed_replay_history_and_signed_checkpoint_claims_are_all_checked() {
    let f = history_fixture();
    let mut checker = f.checker(f.claims.clone(), limits());
    finish_all(&f, &mut checker, 0);
    let program = f.class.decode_program().unwrap();
    let legacy_space = PalwTirStepSpaceV1::new(&f.class).unwrap();
    assert_eq!(program.schedule.layers[0], program.schedule.layers[1]);
    let mut shapes = BTreeMap::new();
    for p in &f.parts {
        if let ExpandedRelationKind::RecomputeNode { ctx, layer: Some(layer), node: 0, shape, .. } = &p.relation.kind {
            shapes.insert((ctx.pos, *layer), shape.clone());
        }
    }
    for layer in 0..3 {
        for pos in 0..8 {
            assert_eq!(shapes[&(pos, layer)], vec![(pos as usize + 1).min(5), 4]);
        }
    }
    for history in [false, true] {
        let index = f
            .parts
            .iter()
            .position(|part| {
                matches!(&part.relation.kind,
            ExpandedRelationKind::Legacy(leaf) if matches!(leaf.kind, PalwTirLeafKindV1::HistTile { .. }) == history
                && matches!(leaf.kind, PalwTirLeafKindV1::State { .. } | PalwTirLeafKindV1::HistTile { .. }))
            })
            .unwrap();
        let mut lies = f.claims.clone();
        lies[index][0] = if lies[index][0] == 127 { 126 } else { lies[index][0] + 1 };
        let mut checker = f.checker(lies, limits());
        for part in &f.parts[..index] {
            submit(&mut checker, part);
        }
        assert_eq!(
            checker.submit(&f.parts[index].material.as_borrowed(), f.parts[index].legacy.as_ref().map(OwnedLegacy::borrowed)),
            Err(FullRelationError::PanelMismatch)
        );
        assert_eq!(checker.progress().outcome, CheckOutcome::WitnessRejected);
    }
    assert!(legacy_space.fixed_instances().len() == 1 && legacy_space.hist_instances().len() == 3);
}

#[test]
fn exact_signed_rounding_and_lowest_lane_greedy_rules_cannot_be_replaced_by_floor_rounding() {
    let f = rounding_fixture();
    let program = f.class.decode_program().unwrap();
    let div = f
        .parts
        .iter()
        .position(|part| {
            matches!(&part.relation.kind,
        ExpandedRelationKind::RecomputeNode { ctx,block,node,.. } if ctx.pos==0
            && matches!(program.blocks[*block as usize].nodes[*node as usize].prim, misaka_palw_tir::Prim::Div { .. }))
        })
        .unwrap();
    assert_eq!(f.claims[div], vec![3, -3]); // signed HAFZ ties: ±5 / 2.
    assert_eq!(f.generated, vec![0, 0]); // [3,3] logits tie at lane0.
    let mut honest = f.checker(f.claims.clone(), limits());
    finish_all(&f, &mut honest, 0);
    let mut floor_like = f.claims.clone();
    floor_like[div][0] = 2;
    let mut checker = f.checker(floor_like, limits());
    for part in &f.parts[..div] {
        submit(&mut checker, part);
    }
    assert_eq!(checker.submit(&f.parts[div].material.as_borrowed(), None), Err(FullRelationError::PanelMismatch));
    assert_eq!(checker.progress().outcome, CheckOutcome::WitnessRejected);
}

#[test]
fn record_memory_and_evaluation_budgets_never_become_arithmetic_faults() {
    let f = wide_fixture();
    for heap in [false, true] {
        let mut cap = limits();
        if heap {
            cap.max_record_heap_bytes = 1;
        } else {
            cap.max_record_bytes = 1;
        }
        let prepared = f.prepare(cap);
        assert!(matches!(
            prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()),
            Err(FullRelationError::Budget)
        ));
    }
    for bound in 0..3 {
        let mut cap = limits();
        match bound {
            0 => cap.source.max_source_reads = 1,
            1 => cap.source.max_auth_storage_bytes = 1,
            _ => cap.source.max_workspace_bytes = 1,
        }
        let mut checker = f.checker(f.claims.clone(), cap);
        assert_eq!(checker.submit(&f.parts[0].material.as_borrowed(), None), Err(FullRelationError::Budget));
        assert_eq!(checker.progress().next_relation, 0);
        assert_eq!(checker.progress().outcome, CheckOutcome::EndedUnverified);
    }
}

#[test]
fn repeated_unsigned_delivery_failure_spends_geometry_budget_without_advancing_coverage() {
    let f = wide_fixture();
    let prepared = f.prepare(limits());
    let summary = prepared.plan().summary();
    let count = summary.relations;
    let mut cap = limits();
    cap.max_record_geometry_visits = 3 * count * summary.geometry_visit_bound;
    let mut checker = f.checker(f.claims.clone(), cap);
    assert_eq!(checker.progress().charged_geometry_visits, 2 * count * summary.geometry_visit_bound);
    let mut missing = f.parts[0].material.clone();
    missing.params = PalwTirParamOpeningV1::None;
    for _ in 0..count {
        assert_eq!(checker.submit(&missing.as_borrowed(), None), Err(FullRelationError::Delivery));
        assert_eq!(checker.progress().next_relation, 0);
        assert_eq!(checker.progress().outcome, CheckOutcome::Checking);
    }
    assert_eq!(checker.progress().charged_geometry_visits, cap.max_record_geometry_visits);
    assert_eq!(checker.submit(&f.parts[0].material.as_borrowed(), None), Err(FullRelationError::Budget));
    assert_eq!(checker.progress().next_relation, 0);
    assert_eq!(checker.progress().outcome, CheckOutcome::EndedUnverified);
}

#[test]
fn the_frozen_manifest_authenticates_exact_claims_actor_policy_and_verifier_local_context() {
    let f = wide_fixture();
    let hidden = f
        .parts
        .iter()
        .position(|p| {
            matches!(&p.relation.kind,
        ExpandedRelationKind::RecomputeNode { ctx,dtype:DType::I128,shape,first_lane:0,lane_count:2,.. }
            if ctx.pos==0 && shape==&vec![2])
        })
        .unwrap();
    let prepared = f.prepare(limits());
    let honest_record =
        prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()).unwrap();
    let signature = sign_record(&honest_record, honest_record.signature_context());
    let mut changed = f.claims.clone();
    changed[hidden][1] += 1;
    let altered_record = prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), changed).unwrap();
    assert_ne!(honest_record.signing_digest(), altered_record.signing_digest());
    assert!(matches!(prepared.begin(altered_record, &panel_key(), &signature), Err(FullRelationError::Signature)));

    for wrong_author in [false, true] {
        let prepared = f.prepare(limits());
        let author = if wrong_author { principal(&miner_key()) } else { principal(&panel_key()) };
        let record = prepared.freeze_record(f.binding.clone(), f.generated.clone(), author, f.claims.clone()).unwrap();
        let signature = sign_record(&record, record.signature_context());
        let key = if wrong_author { panel_key() } else { miner_key() };
        assert!(matches!(prepared.begin(record, &key, &signature), Err(FullRelationError::Signature)));
    }

    let verifier = f.prepare(limits());
    let mut different_policy = limits();
    different_policy.max_record_heap_bytes += 1;
    let issuer = f.prepare(different_policy);
    let record = issuer.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()).unwrap();
    let signature = sign_record(&record, record.signature_context());
    assert!(matches!(verifier.begin(record, &panel_key(), &signature), Err(FullRelationError::Binding)));

    let verifier = f.prepare(limits());
    let foreign_domains = PrototypeDomains::local_only(b"A1/foreign/local-test-record", b"A1/FOREIGN/LOCAL/TEST").unwrap();
    let issuer = f.prepare_result(f.result(), limits(), foreign_domains);
    let record = issuer.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()).unwrap();
    let signature = sign_record(&record, record.signature_context());
    assert!(matches!(verifier.begin(record, &panel_key(), &signature), Err(FullRelationError::Binding)));
    let prepared = f.prepare(limits());
    let record = prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()).unwrap();
    let wrong_context_signature = sign_record(&record, b"A1/FOREIGN/LOCAL/TEST");
    assert!(matches!(prepared.begin(record, &panel_key(), &wrong_context_signature), Err(FullRelationError::Signature)));

    let prepared = f.prepare(limits());
    let wide64 = f
        .parts
        .iter()
        .position(|part| matches!(part.relation.kind, ExpandedRelationKind::RecomputeNode { dtype: DType::I64, .. }))
        .unwrap();
    for malformed in 0..3 {
        let mut claims = f.claims.clone();
        match malformed {
            0 => {
                claims.pop();
            }
            1 => {
                claims[hidden].pop();
            }
            _ => claims[wide64][0] = i64::MAX as i128 + 1,
        }
        assert!(matches!(
            prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), claims),
            Err(FullRelationError::Binding)
        ));
    }
}

#[test]
fn separately_signed_miner_output_is_compared_only_after_the_final_full_relation() {
    let f = wide_fixture();
    let mut false_result = f.result();
    false_result.output = d(99);
    let prepared = f.prepare_result(false_result, limits(), domains());
    let record = prepared.freeze_record(f.binding.clone(), f.generated.clone(), principal(&panel_key()), f.claims.clone()).unwrap();
    let signature = sign_record(&record, record.signature_context());
    let mut checker = prepared.begin(record, &panel_key(), &signature).unwrap();
    assert_eq!(checker.finish(), Err(FullRelationError::Incomplete));
    for part in &f.parts[..f.parts.len() - 1] {
        submit(&mut checker, part);
    }
    assert_eq!(checker.progress().outcome, CheckOutcome::Checking);
    assert_eq!(checker.finish(), Err(FullRelationError::Incomplete));
    submit(&mut checker, f.parts.last().unwrap());
    assert_eq!(checker.progress().next_relation, f.parts.len() as u64);
    assert_eq!(checker.progress().legacy_frontier, f.binding.step_leaf_count);
    assert_eq!(checker.progress().checked_rows, 2);
    assert_eq!(checker.progress().outcome, CheckOutcome::Checking);
    assert_eq!(checker.finish(), Ok(CheckOutcome::OutputMismatch));
}
